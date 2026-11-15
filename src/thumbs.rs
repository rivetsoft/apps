//! Thumbnails, the freedesktop way (the same as other file managers):
//!
//! * the shared cache first: `~/.cache/thumbnails/{normal,large,x-large,xx-large}/<md5 of the
//!   file's URI>.png`, valid while its `Thumb::MTime` matches the file;
//! * pictures are made here; anything else (videos, PDFs, fonts, office files, books…) by the
//!   thumbnailers installed on the system (`/usr/share/thumbnailers/*.thumbnailer`);
//! * what's made goes back into the shared cache, and failures into `fail/rivet-files/`, so
//!   nothing slow is tried twice.
//!
//! All on background threads; the view asks with [`Thumbnailer::get`] and repaints when ready.

use eframe::egui;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::OnceLock;
use std::time::{Duration, Instant, SystemTime};

const THUMB_PX: u32 = 256;
/// Largest picture decoded here (the system's thumbnailers have their own limits).
const MAX_FILE: u64 = 40 * 1024 * 1024;
/// A thumbnailer that takes longer than this is stopped.
const TIMEOUT: Duration = Duration::from_secs(15);

/// Checks (screenshots, measurements) read the shared cache but never write to it.
pub static NO_WRITE: AtomicBool = AtomicBool::new(false);

struct Job {
    path: PathBuf,
    mime: String,
    mtime: Option<SystemTime>,
}

pub struct Thumbnailer {
    tx: Sender<Job>,
    rx: Receiver<(PathBuf, Option<SystemTime>, Option<egui::ColorImage>)>,
    pending: HashSet<PathBuf>,
    /// Finished thumbnails: the texture, when it was last asked for, and the file's modified
    /// time it was made from (a newer file gets a new thumbnail).
    done: HashMap<PathBuf, (Option<egui::TextureHandle>, u64, Option<SystemTime>)>,
    tick: u64,
}

/// Thumbnails kept in memory (the least recently shown go first; they come back from the disk
/// cache quickly when needed again).
const KEEP: usize = 800;

/// Pictures Rivet decodes itself.
pub fn can_thumbnail(mime: &str) -> bool {
    matches!(mime, "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/bmp" | "image/svg+xml")
}

/// Can this kind of file get a thumbnail (here, or from an installed thumbnailer)?
pub fn thumbnailable(mime: &str) -> bool {
    can_thumbnail(mime) || thumbnailer_for(mime).is_some()
}

impl Thumbnailer {
    pub fn new(ctx: egui::Context) -> Self {
        let (tx, wrx) = channel::<Job>();
        let (wtx, rx) = channel();
        // A few workers share the queue (thumbnailers for videos can take a moment).
        let wrx = std::sync::Arc::new(std::sync::Mutex::new(wrx));
        for _ in 0..3 {
            let wrx = wrx.clone();
            let wtx = wtx.clone();
            let ctx = ctx.clone();
            std::thread::spawn(move || loop {
                let job = { wrx.lock().unwrap_or_else(|e| e.into_inner()).recv() };
                let Ok(job) = job else { break };
                let img = thumbnail(&job);
                if wtx.send((job.path, job.mtime, img)).is_err() {
                    break;
                }
                ctx.request_repaint();
            });
        }
        Self { tx, rx, pending: HashSet::new(), done: HashMap::new(), tick: 0 }
    }

    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((p, mtime, img)) = self.rx.try_recv() {
            self.pending.remove(&p);
            let tex = img.map(|i| ctx.load_texture(format!("thumb:{}", p.display()), i, egui::TextureOptions::LINEAR));
            self.tick += 1;
            self.done.insert(p, (tex, self.tick, mtime));
        }
        if self.done.len() > KEEP + KEEP / 4 {
            let mut ages: Vec<u64> = self.done.values().map(|(_, t, _)| *t).collect();
            ages.sort_unstable();
            let cut = ages[ages.len() - KEEP];
            self.done.retain(|_, (_, t, _)| *t >= cut);
        }
    }

    pub fn get(&mut self, path: &Path, size: u64, mime: &str, mtime: Option<SystemTime>) -> Option<egui::TextureHandle> {
        self.tick += 1;
        if let Some((t, used, made_from)) = self.done.get_mut(path) {
            // Still the same file: keep it (even while a new one is being made).
            *used = self.tick;
            if *made_from == mtime || self.pending.contains(path) {
                return t.clone();
            }
        }
        let ours = can_thumbnail(mime);
        if (!ours || size <= MAX_FILE) && !self.pending.contains(path) {
            self.pending.insert(path.to_path_buf());
            let _ = self.tx.send(Job { path: path.to_path_buf(), mime: mime.to_string(), mtime });
        }
        // A changed file keeps its old thumbnail until the new one is ready.
        self.done.get(path).and_then(|(t, _, _)| t.clone())
    }

}

// ------------------------------------------------------------------ one thumbnail

fn thumbnail(job: &Job) -> Option<egui::ColorImage> {
    let cache = crate::places::cache_home().join("thumbnails");
    // Never thumbnail the thumbnails.
    if job.path.starts_with(&cache) {
        return None;
    }
    let uri = format!("file://{}", crate::places::percent_encode_path(&job.path));
    let name = format!("{}.png", hex(&md5(uri.as_bytes())));
    let mtime = job.mtime.and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
    // 1. The shared cache (the right size first, then bigger, then smaller).
    for dir in ["large", "x-large", "xx-large", "normal"] {
        if read_cached(&cache.join(dir).join(&name), mtime).is_some()
            && let Ok(img) = image::open(cache.join(dir).join(&name))
        {
            return Some(to_color_image(&backdrop(img.to_rgba8(), &job.mime)));
        }
    }
    // Tried before and failed (and the file hasn't changed since).
    let fail = cache.join("fail/rivet-files").join(&name);
    if cached_mtime(&fail) == Some(mtime) {
        return None;
    }
    // 2. Make it: pictures here, the rest with the system's thumbnailers.
    let made = if can_thumbnail(&job.mime) {
        make_picture(&job.path)
    } else {
        run_thumbnailer(&job.path, &job.mime, &uri)
    };
    // 3. Into the shared cache for next time (and for other apps).
    if !NO_WRITE.load(Ordering::Relaxed) {
        match &made {
            Some(img) => write_png(&cache.join("large").join(&name), img, &uri, mtime),
            None => write_png(&cache.join("fail/rivet-files").join(&name), &egui::ColorImage::new([1, 1], vec![egui::Color32::TRANSPARENT]), &uri, mtime),
        }
    }
    made.map(|i| to_color_image(&backdrop(i, &job.mime)))
}

/// Font samples are black on transparent: show them on white, as other apps do.
fn backdrop(img: image::RgbaImage, mime: &str) -> image::RgbaImage {
    if !(mime.starts_with("font/") || mime.contains("font")) {
        return img;
    }
    let mut out = image::RgbaImage::from_pixel(img.width(), img.height(), image::Rgba([255, 255, 255, 255]));
    image::imageops::overlay(&mut out, &img, 0, 0);
    out
}

fn make_picture(path: &Path) -> Option<image::RgbaImage> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "svg" {
        let c = crate::icons::load_image_file(path, THUMB_PX)?;
        let raw: Vec<u8> = c.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
        return image::RgbaImage::from_raw(c.size[0] as u32, c.size[1] as u32, raw);
    }
    let img = image::open(path).ok()?;
    let img = if img.width() > THUMB_PX || img.height() > THUMB_PX { img.thumbnail(THUMB_PX, THUMB_PX) } else { img };
    Some(img.to_rgba8())
}

fn to_color_image(img: &image::RgbaImage) -> egui::ColorImage {
    egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw())
}

/// The `Thumb::MTime` stored in a cached thumbnail.
fn cached_mtime(path: &Path) -> Option<u64> {
    let f = std::fs::File::open(path).ok()?;
    let reader = png::Decoder::new(std::io::BufReader::new(f)).read_info().ok()?;
    let info = reader.info();
    let latin = info.uncompressed_latin1_text.iter().map(|t| (t.keyword.clone(), t.text.clone()));
    let utf8 = info.utf8_text.iter().filter_map(|t| t.get_text().ok().map(|v| (t.keyword.clone(), v)));
    latin.chain(utf8).find(|(k, _)| k == "Thumb::MTime").and_then(|(_, v)| v.trim().parse().ok())
}

/// A cached thumbnail, if it's for this version of the file.
fn read_cached(path: &Path, mtime: u64) -> Option<egui::ColorImage> {
    if cached_mtime(path)? != mtime {
        return None;
    }
    let img = image::open(path).ok()?.to_rgba8();
    Some(to_color_image(&img))
}

/// Save a thumbnail with the tags the spec asks for, privately and atomically.
fn write_png(path: &Path, img: &impl ThumbPixels, uri: &str, mtime: u64) {
    let Some(dir) = path.parent() else { return };
    if std::fs::create_dir_all(dir).is_err() {
        return;
    }
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
    let tmp = dir.join(format!(".rivet-{}-{}.png", std::process::id(), mtime));
    let ok = (|| -> Option<()> {
        let f = std::fs::File::create(&tmp).ok()?;
        let (w, h, rgba) = img.rgba();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.add_text_chunk("Thumb::URI".into(), uri.into()).ok()?;
        enc.add_text_chunk("Thumb::MTime".into(), mtime.to_string()).ok()?;
        enc.add_text_chunk("Software".into(), "Rivet Files".into()).ok()?;
        let mut writer = enc.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
        Some(())
    })();
    if ok.is_some() {
        let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        let _ = std::fs::rename(&tmp, path);
    } else {
        let _ = std::fs::remove_file(&tmp);
    }
}

trait ThumbPixels {
    fn rgba(&self) -> (u32, u32, Vec<u8>);
}

impl ThumbPixels for image::RgbaImage {
    fn rgba(&self) -> (u32, u32, Vec<u8>) {
        (self.width(), self.height(), self.as_raw().clone())
    }
}

impl ThumbPixels for egui::ColorImage {
    fn rgba(&self) -> (u32, u32, Vec<u8>) {
        (self.size[0] as u32, self.size[1] as u32, self.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect())
    }
}

// ------------------------------------------------------------------ installed thumbnailers

/// MIME type -> the thumbnailer's Exec line, from every `thumbnailers` data folder (the user's
/// own first). Thumbnailers whose program isn't installed are left out.
fn registry() -> &'static HashMap<String, String> {
    static REG: OnceLock<HashMap<String, String>> = OnceLock::new();
    REG.get_or_init(|| {
        let mut dirs = vec![crate::places::data_home()];
        let sys = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
        dirs.extend(sys.split(':').map(PathBuf::from));
        let mut map = HashMap::new();
        for d in dirs {
            let mut files: Vec<PathBuf> = std::fs::read_dir(d.join("thumbnailers"))
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|x| x == "thumbnailer"))
                .collect();
            files.sort();
            for f in files {
                let Ok(text) = std::fs::read_to_string(&f) else { continue };
                let (mut exec, mut try_exec, mut mimes) = (None, None, Vec::new());
                let mut in_entry = false;
                for line in text.lines() {
                    let l = line.trim();
                    if l.starts_with('[') {
                        in_entry = l == "[Thumbnailer Entry]";
                    } else if in_entry && let Some((k, v)) = l.split_once('=') {
                        match k.trim() {
                            "Exec" => exec = Some(v.trim().to_string()),
                            "TryExec" => try_exec = Some(v.trim().to_string()),
                            "MimeType" => mimes = v.split(';').map(|m| m.trim().to_string()).filter(|m| !m.is_empty()).collect(),
                            _ => {}
                        }
                    }
                }
                let Some(exec) = exec else { continue };
                let program = try_exec.or_else(|| split_exec(&exec).into_iter().next()).unwrap_or_default();
                if !installed(&program) {
                    continue;
                }
                for m in mimes {
                    map.entry(m).or_insert_with(|| exec.clone());
                }
            }
        }
        map
    })
}

fn thumbnailer_for(mime: &str) -> Option<&'static String> {
    registry().get(mime)
}

fn installed(program: &str) -> bool {
    if program.is_empty() {
        return false;
    }
    if program.contains('/') {
        return Path::new(program).is_file();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// Split an Exec line into words (double and single quotes group, no shell).
fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut any = false;
    for c in exec.chars() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), c) => cur.push(c),
            (None, '"' | '\'') => {
                quote = Some(c);
                any = true;
            }
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() || any {
                    out.push(std::mem::take(&mut cur));
                }
                any = false;
            }
            (None, c) => cur.push(c),
        }
    }
    if !cur.is_empty() || any {
        out.push(cur);
    }
    out
}

/// Run the installed thumbnailer for this file into a temporary PNG and load it.
fn run_thumbnailer(path: &Path, mime: &str, uri: &str) -> Option<image::RgbaImage> {
    let exec = thumbnailer_for(mime)?;
    let out = std::env::temp_dir().join(format!("rivet-thumb-{}-{}.png", std::process::id(), hex(&md5(uri.as_bytes()))));
    let args: Vec<String> = split_exec(exec)
        .into_iter()
        .map(|w| {
            w.replace("%s", &THUMB_PX.to_string())
                .replace("%u", uri)
                .replace("%i", &path.to_string_lossy())
                .replace("%o", &out.to_string_lossy())
                .replace("%%", "%")
        })
        .collect();
    let (prog, rest) = args.split_first()?;
    let mut child = std::process::Command::new(prog)
        .args(rest)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let start = Instant::now();
    let ok = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s.success(),
            Ok(None) if start.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(30)),
            Err(_) => break false,
        }
    };
    let img = if ok { image::open(&out).ok() } else { None };
    let _ = std::fs::remove_file(&out);
    let img = img?;
    let img = if img.width() > THUMB_PX || img.height() > THUMB_PX { img.thumbnail(THUMB_PX, THUMB_PX) } else { img };
    Some(img.to_rgba8())
}

// ------------------------------------------------------------------ md5 (for cache names)

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// MD5, as the thumbnail spec names files by it.
fn md5(input: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4,
        11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];
    let k: Vec<u32> = (0..64).map(|i| ((i as f64 + 1.0).sin().abs() * 4294967296.0) as u32).collect();
    let (mut a0, mut b0, mut c0, mut d0) = (0x67452301u32, 0xefcdab89u32, 0x98badcfeu32, 0x10325476u32);
    let mut msg = input.to_vec();
    let bits = (input.len() as u64).wrapping_mul(8);
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&bits.to_le_bytes());
    for chunk in msg.chunks(64) {
        let m: Vec<u32> = chunk.chunks(4).map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]])).collect();
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);
        for i in 0..64 {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f.wrapping_add(a).wrapping_add(k[i]).wrapping_add(m[g]);
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(S[i]));
        }
        a0 = a0.wrapping_add(a);
        b0 = b0.wrapping_add(b);
        c0 = c0.wrapping_add(c);
        d0 = d0.wrapping_add(d);
    }
    let mut out = [0u8; 16];
    out[..4].copy_from_slice(&a0.to_le_bytes());
    out[4..8].copy_from_slice(&b0.to_le_bytes());
    out[8..12].copy_from_slice(&c0.to_le_bytes());
    out[12..].copy_from_slice(&d0.to_le_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn md5_matches_known_values() {
        assert_eq!(hex(&md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(&md5(b"The quick brown fox jumps over the lazy dog")), "9e107d9d372bb6826bd81d3542a419d6");
        // The spec's own example: file:///home/jens/photos/me.png
        assert_eq!(hex(&md5(b"file:///home/jens/photos/me.png")), "c6ee772d9e49320e97ec29a7eb5b1697");
    }

    #[test]
    fn exec_lines_split_like_desktop_files() {
        assert_eq!(split_exec("ffmpegthumbnailer -i %i -o %o -s %s"), vec!["ffmpegthumbnailer", "-i", "%i", "-o", "%o", "-s", "%s"]);
        assert_eq!(split_exec("sh -c 'a b' \"\""), vec!["sh", "-c", "a b", ""]);
    }

    #[test]
    fn cache_round_trip_checks_the_mtime() {
        let dir = std::env::temp_dir().join(format!("rivet-thumb-test-{}", std::process::id()));
        let p = dir.join("t.png");
        let img = image::RgbaImage::from_pixel(4, 3, image::Rgba([10, 20, 30, 255]));
        write_png(&p, &img, "file:///x", 1234);
        assert_eq!(cached_mtime(&p), Some(1234));
        assert!(read_cached(&p, 1234).is_some_and(|c| c.size == [4, 3]));
        assert!(read_cached(&p, 99).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
