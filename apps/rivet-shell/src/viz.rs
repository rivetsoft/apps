//! The visualizer: what the computer is playing, as bars from low notes to high. The sound
//! comes from PipeWire's own recorder (`pw-record`, listening to the default output rather
//! than a microphone) as plain samples; the shell splits them into bands itself. It runs only
//! while something shows the bars, and where `pw-record` isn't installed there are none.

use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

/// Bands worked out; a drawing with fewer bars joins neighbours (`bars`).
pub const BANDS: usize = 32;
const RATE: usize = 44_100;
/// Samples looked at for each frame (a power of two).
const WINDOW: usize = 2048;
/// New samples per frame: 30 frames a second.
const HOP: usize = RATE / 30;
const LOW_HZ: f32 = 45.0;
const HIGH_HZ: f32 = 14_000.0;

struct Shared {
    want: Mutex<bool>,
    changed: Condvar,
    bands: Mutex<Vec<f32>>,
    generation: AtomicU64,
    /// Whether there's a recorder to run at all.
    able: AtomicBool,
}

#[derive(Clone)]
pub struct VizLive {
    shared: Arc<Shared>,
}

impl VizLive {
    /// `wake` is called (from the visualizer's thread) each time the bars change.
    pub fn new(wake: impl Fn() + Send + 'static) -> VizLive {
        let shared = Arc::new(Shared { want: Mutex::new(false), changed: Condvar::new(), bands: Mutex::new(vec![0.0; BANDS]), generation: AtomicU64::new(0), able: AtomicBool::new(crate::sys::has("pw-record")) });
        let s = shared.clone();
        let _ = std::thread::Builder::new().name("rivet-shell visualizer".into()).spawn(move || run(&s, &wake));
        VizLive { shared }
    }

    /// Whether this computer can show a visualizer.
    pub fn able(&self) -> bool {
        self.shared.able.load(Ordering::Relaxed)
    }

    /// Listen (or stop): on only while something shows the bars.
    pub fn want(&self, on: bool) {
        let mut w = self.shared.want.lock().unwrap_or_else(|e| e.into_inner());
        if *w != on {
            *w = on;
            self.shared.changed.notify_all();
        }
    }

    /// Each band's level, 0 to 1, lowest notes first.
    pub fn get(&self) -> Vec<f32> {
        self.shared.bands.lock().map(|b| b.clone()).unwrap_or_default()
    }
}

fn wanted(s: &Shared) -> bool {
    *s.want.lock().unwrap_or_else(|e| e.into_inner())
}

fn run(s: &Shared, wake: &dyn Fn()) {
    loop {
        // Asleep until something wants the bars.
        {
            let mut w = s.want.lock().unwrap_or_else(|e| e.into_inner());
            while !*w {
                w = s.changed.wait(w).unwrap_or_else(|e| e.into_inner());
            }
        }
        let Some(mut child) = record() else {
            s.able.store(false, Ordering::Relaxed);
            return;
        };
        if let Some(out) = child.stdout.take() {
            listen(s, out, wake);
        }
        let _ = child.kill();
        let _ = child.wait();
        // The bars fall flat once nothing listens.
        if let Ok(mut b) = s.bands.lock() {
            b.iter_mut().for_each(|v| *v = 0.0);
        }
        s.generation.fetch_add(1, Ordering::Relaxed);
        wake();
        // A recorder that ended by itself (no sound server just now): not straight away again.
        if wanted(s) {
            std::thread::sleep(std::time::Duration::from_secs(3));
        }
    }
}

/// PipeWire's recorder on the default output, one channel of 16-bit samples to its stdout.
fn record() -> Option<Child> {
    Command::new("pw-record")
        .args(["-P", "{ stream.capture.sink = true node.name = rivet-shell-visualizer node.description = \"Rivet Shell visualizer\" }", "--raw", "--format", "s16", "--rate", &RATE.to_string(), "--channels", "1", "--latency", "30ms", "-"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// Read samples until told to stop (or the recorder ends), a frame of bars for each hop.
fn listen(s: &Shared, mut out: impl Read, wake: &dyn Fn()) {
    let bands = Bands::new();
    let mut window = vec![0.0f32; WINDOW];
    let mut raw = vec![0u8; HOP * 2];
    let mut levels = vec![0.0f32; BANDS];
    let mut quiet = 0u32;
    while wanted(s) {
        if out.read_exact(&mut raw).is_err() {
            return;
        }
        window.copy_within(HOP.., 0);
        let fresh = &mut window[WINDOW - HOP..];
        for (v, b) in fresh.iter_mut().zip(raw.as_chunks::<2>().0) {
            *v = i16::from_le_bytes(*b) as f32 / 32768.0;
        }
        let now = bands.levels(&window);
        // Up at once, down slowly, so the bars don't flicker.
        for (l, n) in levels.iter_mut().zip(&now) {
            *l = if *n > *l { *l + (*n - *l) * 0.65 } else { (*l * 0.82).max(*n) };
            if *l < 0.004 {
                *l = 0.0;
            }
        }
        // Silence is told once, then nothing until there's sound again.
        let silent = levels.iter().all(|v| *v == 0.0);
        quiet = if silent { quiet + 1 } else { 0 };
        if quiet > 1 {
            continue;
        }
        if let Ok(mut b) = s.bands.lock() {
            b.copy_from_slice(&levels);
        }
        s.generation.fetch_add(1, Ordering::Relaxed);
        wake();
    }
}

/// Splitting a window of samples into bands: the window's shape, and which notes each band
/// covers.
struct Bands {
    hann: Vec<f32>,
    /// Each band's first and one-past-last frequency bin.
    edges: Vec<(usize, usize)>,
}

impl Bands {
    fn new() -> Bands {
        let hann = (0..WINDOW).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / (WINDOW - 1) as f32).cos()).collect();
        let bin = RATE as f32 / WINDOW as f32;
        // Evenly spaced by pitch (each band the same musical width), at least a bin each.
        let at = |k: usize| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(k as f32 / BANDS as f32);
        let mut edges = Vec::with_capacity(BANDS);
        let mut from = (at(0) / bin).round().max(1.0) as usize;
        for k in 0..BANDS {
            let to = ((at(k + 1) / bin).round() as usize).max(from + 1).min(WINDOW / 2);
            edges.push((from, to));
            from = to;
        }
        Bands { hann, edges }
    }

    /// Each band's level, 0 to 1.
    fn levels(&self, window: &[f32]) -> Vec<f32> {
        let mut re: Vec<f32> = window.iter().zip(&self.hann).map(|(s, h)| s * h).collect();
        let mut im = vec![0.0f32; WINDOW];
        fft(&mut re, &mut im);
        self.edges
            .iter()
            .enumerate()
            .map(|(k, &(a, b))| {
                let peak = (a..b.max(a + 1)).map(|i| (re[i] * re[i] + im[i] * im[i]).sqrt()).fold(0.0f32, f32::max);
                // In decibels under a full-volume note, with high notes lifted (music has
                // less up there), and the part from -62 to -14 spread over the bar.
                let db = 20.0 * (peak / (WINDOW as f32 / 4.0)).max(1e-9).log10() + k as f32 / BANDS as f32 * 16.0;
                ((db + 62.0) / 48.0).clamp(0.0, 1.0)
            })
            .collect()
    }
}

/// The frequencies in a run of samples (in place; the length is a power of two).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -std::f32::consts::TAU / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (tr, ti) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                (re[b], im[b]) = (re[a] - tr, im[a] - ti);
                re[a] += tr;
                im[a] += ti;
                (cr, ci) = (cr * wr - ci * wi, cr * wi + ci * wr);
            }
        }
        len <<= 1;
    }
}

/// `n` bars from the bands: neighbours joined, the loudest of each group.
pub fn bars(bands: &[f32], n: usize) -> Vec<f32> {
    if bands.is_empty() || n == 0 {
        return vec![0.0; n];
    }
    (0..n)
        .map(|i| {
            let (a, b) = (i * bands.len() / n, ((i + 1) * bands.len() / n).max(i * bands.len() / n + 1).min(bands.len()));
            bands[a.min(bands.len() - 1)..b].iter().copied().fold(0.0, f32::max)
        })
        .collect()
}

/// Made-up bars, for pictures.
pub fn demo() -> Vec<f32> {
    (0..BANDS).map(|i| (0.55 + 0.4 * ((i as f32 * 0.9).sin() * (i as f32 * 0.37 + 1.0).cos()) - i as f32 * 0.008).clamp(0.08, 1.0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, loud: f32) -> Vec<f32> {
        (0..WINDOW).map(|i| loud * (std::f32::consts::TAU * hz * i as f32 / RATE as f32).sin()).collect()
    }

    #[test]
    fn a_note_lights_its_own_band() {
        let b = Bands::new();
        let low = b.levels(&tone(110.0, 0.5));
        let high = b.levels(&tone(5000.0, 0.5));
        let top = |l: &[f32]| l.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i).unwrap_or(0);
        assert!(top(&low) < 8, "110 Hz is among the low bands: {}", top(&low));
        assert!(top(&high) > 20, "5 kHz is among the high bands: {}", top(&high));
        assert!(low[top(&low)] > 0.8);
        // Far from the note, next to nothing.
        assert!(low[28] < 0.1);
    }

    #[test]
    fn silence_is_flat() {
        assert!(Bands::new().levels(&vec![0.0; WINDOW]).iter().all(|v| *v == 0.0));
    }

    #[test]
    fn joins_bands_into_fewer_bars() {
        let bands: Vec<f32> = (0..BANDS).map(|i| i as f32 / BANDS as f32).collect();
        let b = bars(&bands, 8);
        assert_eq!(b.len(), 8);
        assert!(b.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(bars(&bands, BANDS), bands);
    }

    /// Listens to this computer for real for a moment: `cargo test -- --ignored viz`.
    #[test]
    #[ignore]
    fn live() {
        let (tx, rx) = std::sync::mpsc::channel();
        let tx = Mutex::new(tx);
        let v = VizLive::new(move || {
            let _ = tx.lock().unwrap().send(());
        });
        assert!(v.able());
        v.want(true);
        assert!(rx.recv_timeout(std::time::Duration::from_secs(3)).is_ok(), "bars arrived");
        v.want(false);
        assert_eq!(v.get().len(), BANDS);
    }
}
