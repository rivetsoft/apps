//! The allocator every Rivet app runs with, and handing freed memory back to the system.
//!
//! Rivet uses the C library's allocator (glibc's malloc), tuned, rather than a different one:
//! on long sessions of Rivet Shell, Files and Terminal it ends up smaller than mimalloc, at the
//! same CPU, with no extra dependency (numbers in docs/memory.md). The tuning:
//!
//! * a fixed trim threshold, so the top of the heap is given back after a free of a few
//!   megabytes rather than only past glibc's moving threshold of up to 32 MB;
//! * a fixed mmap threshold of the same size, so blocks of a few megabytes (a folder's list,
//!   a decoded picture) come from the heap, as glibc's moving threshold would have them, and
//!   bigger ones (a wallpaper, a screen-sized buffer) go back to the system as soon as they're
//!   freed. Without it, fixing the trim threshold would also fix the mmap threshold at 128 KB.
//!
//! The number of arenas is left alone: capping it saves no memory once trims run, and costs
//! CPU in lock contention when many threads allocate at once (2.5 times the CPU at 2 arenas
//! reading a 100,000-item folder in Rivet Files).
//!
//! What can't be given back that way (free memory in the middle of the heap, after a terminal
//! pane with a long scrollback closes or a cache lets go of pictures) is handed back by
//! `malloc_trim`, on a thread of its own: soon after [`released`] reports a big free, and
//! whenever the app has been quiet with a lot of free heap.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

/// Free memory at the top of the heap past this is given back at once, and blocks this big or
/// bigger get memory of their own from the system.
const THRESHOLD: i32 = 4 << 20;
/// Freed bytes reported to [`released`] that are worth a trim.
const WORTH_TRIMMING: usize = 8 << 20;
/// How long after a big free the trim waits, so a run of frees ends in one trim.
const SETTLE: Duration = Duration::from_millis(500);
/// How often the quiet check looks at the free heap, and how much free heap is worth a trim then.
const QUIET_EVERY: Duration = Duration::from_secs(30);
const QUIET_FREE: usize = 16 << 20;

static RELEASED: AtomicUsize = AtomicUsize::new(0);

struct Trimmer {
    asked: Mutex<bool>,
    wake: Condvar,
}

fn trimmer() -> &'static Trimmer {
    static T: OnceLock<Trimmer> = OnceLock::new();
    T.get_or_init(|| {
        let _ = std::thread::Builder::new().name("rivet-mem trim".into()).spawn(run);
        Trimmer { asked: Mutex::new(false), wake: Condvar::new() }
    })
}

/// Call first in `main`, before any thread starts: sets the allocator up the Rivet way and starts
/// the thread that hands free memory back when the app is quiet. `RIVET_MALLOC=default` leaves
/// the C library's own settings, to compare.
pub fn setup() {
    if std::env::var("RIVET_MALLOC").is_ok_and(|v| v == "default") {
        return;
    }
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        // The environment's own choices (GLIBC_TUNABLES, MALLOC_*_) win.
        if std::env::var_os("MALLOC_TRIM_THRESHOLD_").is_none() && !tunable("trim_threshold") {
            libc::mallopt(libc::M_TRIM_THRESHOLD, THRESHOLD);
        }
        if std::env::var_os("MALLOC_MMAP_THRESHOLD_").is_none() && !tunable("mmap_threshold") {
            libc::mallopt(libc::M_MMAP_THRESHOLD, THRESHOLD);
        }
    }
    trimmer();
}

fn tunable(name: &str) -> bool {
    std::env::var("GLIBC_TUNABLES").is_ok_and(|t| t.contains(&format!("glibc.malloc.{name}=")))
}

/// Report that about `bytes` were just freed (a closed pane, pictures let go of). Once enough
/// has been freed, the memory is handed back to the system a moment later, off this thread.
pub fn released(bytes: usize) {
    if RELEASED.fetch_add(bytes, Ordering::Relaxed) + bytes >= WORTH_TRIMMING {
        trim_soon();
    }
}

/// Hand free memory back to the system a moment from now, off this thread.
pub fn trim_soon() {
    let t = trimmer();
    *t.asked.lock().unwrap_or_else(|e| e.into_inner()) = true;
    t.wake.notify_one();
}

/// Hand free memory back to the system now. Takes a few milliseconds on a big heap, during
/// which other threads wait to allocate: prefer [`trim_soon`] on a thread that draws.
pub fn trim_now() {
    RELEASED.store(0, Ordering::Relaxed);
    let before = log().then(crate::heap).flatten();
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    unsafe {
        libc::malloc_trim(0);
    }
    if let Some(b) = before {
        report(&format!("trimmed (heap free {:.1} MB before)", b.free as f64 / 1e6));
    }
}

/// `RIVET_MEM_LOG=1`: a line on standard error at every trim and every quiet check, with the
/// process's memory and the heap, for measuring.
fn log() -> bool {
    static LOG: OnceLock<bool> = OnceLock::new();
    *LOG.get_or_init(|| std::env::var_os("RIVET_MEM_LOG").is_some_and(|v| !v.is_empty()))
}

fn report(what: &str) {
    let heap = crate::heap().map(|h| format!("heap used {:.1} MB, free {:.1} MB", h.used as f64 / 1e6, h.free as f64 / 1e6)).unwrap_or_default();
    let usage = crate::Usage::now().map(|u| u.line()).unwrap_or_default();
    eprintln!("rivet-mem: {what}: {usage}; {heap}");
}

fn run() {
    let t = trimmer();
    loop {
        let mut asked = t.asked.lock().unwrap_or_else(|e| e.into_inner());
        if !*asked {
            asked = t.wake.wait_timeout(asked, QUIET_EVERY).unwrap_or_else(|e| e.into_inner()).0;
        }
        let now = std::mem::take(&mut *asked);
        drop(asked);
        if now {
            // More frees often follow (a window's other panes, the rest of a cache): wait for them.
            std::thread::sleep(SETTLE);
            *t.asked.lock().unwrap_or_else(|e| e.into_inner()) = false;
            trim_now();
        } else if crate::heap().is_some_and(|h| h.free >= QUIET_FREE) {
            trim_now();
        } else if log() {
            report("quiet");
        }
    }
}
