//! The wallpaper: the pictures to choose from, one scaled to fill a screen, and matugen run on
//! it so the desktop's colours (and so every Rivet app's) follow the picture.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, channel};
use std::time::SystemTime;

use eframe::egui;

use rivet_look::theme::Mode;

const KINDS: &[&str] = &["jpg", "jpeg", "png", "webp"];

/// The pictures in a folder (and the folders right inside it), sorted by name.
pub fn pictures(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let walk = |d: &Path, out: &mut Vec<PathBuf>, deeper: &mut Vec<PathBuf>| {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            let name = e.file_name().to_string_lossy().to_lowercase();
            if name.starts_with('.') {
                continue;
            }
            if p.is_dir() {
                deeper.push(p);
            } else if p.extension().and_then(|x| x.to_str()).is_some_and(|x| KINDS.contains(&x.to_lowercase().as_str())) {
                out.push(p);
            }
        }
    };
    let mut sub = Vec::new();
    walk(dir, &mut out, &mut sub);
    for d in sub {
        walk(&d, &mut out, &mut Vec::new());
    }
    out.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    out
}

/// Another picture from the folder: `random` (never the current one, when there are others),
/// or the `next` / `previous` one by name, going round.
pub fn pick(dir: &Path, now: &Path, how: &str) -> Option<PathBuf> {
    let list = pictures(dir);
    choose(&list, now, how, seed())
}

fn seed() -> u64 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos() as u64).unwrap_or(0);
    // Mixed (splitmix64), so close times give far-apart picks.
    let mut z = t ^ ((std::process::id() as u64) << 32);
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

fn choose(list: &[PathBuf], now: &Path, how: &str, seed: u64) -> Option<PathBuf> {
    if list.is_empty() {
        return None;
    }
    let at = list.iter().position(|p| p == now);
    let n = list.len();
    let i = match (how, at) {
        ("next", Some(i)) => (i + 1) % n,
        ("previous", Some(i)) => (i + n - 1) % n,
        ("next", None) => 0,
        ("previous", None) => n - 1,
        // Random among the others.
        (_, Some(i)) if n > 1 => (i + 1 + (seed % (n as u64 - 1)) as usize) % n,
        _ => (seed % n as u64) as usize,
    };
    list.get(i).cloned()
}

/// A picture scaled to cover `w`×`h` (cropping what overflows, centred), as BGRA bytes for a
/// Wayland buffer.
pub fn cover_bgra(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let (iw, ih) = (img.width(), img.height());
    rivet_kit::scale::cover_bgra(img.into_raw(), iw, ih, w, h)
}

/// One frame of a wallpaper giving way to the next: `from` and `to` (BGRA, `w` wide) mixed into
/// `out`, `t` of the way through (0 to 1).
pub fn transition(kind: crate::config::Transition, from: &[u8], to: &[u8], out: &mut [u8], w: usize, t: f32) {
    use crate::config::Transition as T;
    if from.len() != to.len() || out.len() != to.len() || w == 0 || t >= 1.0 || kind == T::None {
        out.copy_from_slice(&to[..out.len().min(to.len())]);
        return;
    }
    let t = t.max(0.0);
    // Quick at first, settling at the end.
    let e = 1.0 - (1.0 - t).powi(3);
    let h = to.len() / (w * 4);
    let row = w * 4;
    let mix = |out: &mut [u8], a: &[u8], b: &[u8], k: u32| {
        for ((o, a), b) in out.iter_mut().zip(a).zip(b) {
            *o = ((*a as u32 * (256 - k) + *b as u32 * k) >> 8) as u8;
        }
    };
    match kind {
        T::None => {}
        T::Fade => mix(out, from, to, ((t * t * (3.0 - 2.0 * t)) * 256.0) as u32),
        T::Wipe => {
            // The edge runs left to right, soft over an eighth of the width.
            let soft = (w / 8).max(1);
            let edge = (e * (w + soft) as f32) as usize;
            let (x0, x1) = (edge.saturating_sub(soft).min(w), edge.min(w));
            for y in 0..h {
                let (o, a, b) = (&mut out[y * row..(y + 1) * row], &from[y * row..(y + 1) * row], &to[y * row..(y + 1) * row]);
                o[..x0 * 4].copy_from_slice(&b[..x0 * 4]);
                o[x1 * 4..].copy_from_slice(&a[x1 * 4..]);
                for x in x0..x1 {
                    let k = ((edge - x) * 256 / soft).min(256) as u32;
                    mix(&mut o[x * 4..x * 4 + 4], &a[x * 4..x * 4 + 4], &b[x * 4..x * 4 + 4], k);
                }
            }
        }
        T::Slide => {
            // The new picture comes in from the right, pushing the old one out.
            let off = ((e * w as f32) as usize).min(w) * 4;
            for y in 0..h {
                let (o, a, b) = (&mut out[y * row..(y + 1) * row], &from[y * row..(y + 1) * row], &to[y * row..(y + 1) * row]);
                o[..row - off].copy_from_slice(&a[off..]);
                o[row - off..].copy_from_slice(&b[..off]);
            }
        }
        T::Grow => {
            let (cx, cy) = (w as f32 / 2.0, h as f32 / 2.0);
            let r = e * (cx * cx + cy * cy).sqrt();
            for y in 0..h {
                let (o, a, b) = (&mut out[y * row..(y + 1) * row], &from[y * row..(y + 1) * row], &to[y * row..(y + 1) * row]);
                o.copy_from_slice(a);
                let dy = y as f32 + 0.5 - cy;
                if dy.abs() < r {
                    let half = (r * r - dy * dy).sqrt();
                    let (x0, x1) = ((cx - half).max(0.0) as usize, ((cx + half) as usize).min(w));
                    o[x0 * 4..x1 * 4].copy_from_slice(&b[x0 * 4..x1 * 4]);
                }
            }
        }
    }
}

/// A small copy for choosing from (RGBA, longest side `px`).
pub fn thumbnail(path: &Path, px: u32) -> Option<eframe::egui::ColorImage> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let t = img.thumbnail(px, px).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw()))
}

/// The arguments matugen gets for a picture.
pub fn matugen_args(path: &Path, mode: Mode, scheme: &str, contrast: f32) -> Vec<String> {
    let mode = if mode == Mode::Light { "light" } else { "dark" };
    let mut a = vec!["image".to_string(), path.to_string_lossy().into_owned(), "-m".into(), mode.into()];
    if !scheme.is_empty() {
        a.extend(["-t".into(), scheme.to_string()]);
    }
    if contrast.abs() > 0.004 {
        a.push(format!("--contrast={:.2}", contrast.clamp(-1.0, 1.0)));
    }
    a.push("-q".into());
    a
}

/// Run matugen on a picture, in the background: it writes the colour files every app reads
/// (and anything else in its config). The result comes back to `done`.
///
/// Templates written for other shells may use colours those shells pass in, such as
/// DankMaterialShell's `dank16` (a sixteen-colour terminal palette): they're made here from the
/// picture's own colours and passed in too, so such templates work and matugen doesn't stop
/// halfway (it stops at the first template that fails, leaving the rest unwritten).
pub fn run_matugen(path: &Path, mode: Mode, scheme: &str, contrast: f32, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    let mut args = matugen_args(path, mode, scheme, contrast);
    let probe: Vec<String> = matugen_args(path, mode, scheme, contrast).into_iter().filter(|a| a != "-q").chain(["--dry-run".into(), "--json".into(), "hex".into()]).collect();
    std::thread::spawn(move || {
        // The picture's colours first (nothing written), to make the extra palettes from.
        let colours = std::process::Command::new("matugen").args(&probe).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok().filter(|o| o.status.success()).and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
        if let Some(c) = colours.as_ref().and_then(|v| v.get("colors")) {
            args.push("--import-json-string".into());
            args.push(serde_json::json!({ "dank16": dank16(c) }).to_string());
        }
        let r = std::process::Command::new("matugen")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "matugen isn't installed".to_string() } else { e.to_string() })
            .and_then(|o| {
                if o.status.success() {
                    return Ok(());
                }
                // matugen prints its errors on standard output, coloured.
                let text = format!("{}\n{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
                Err(matugen_error(&text))
            });
        done(r);
    });
}

/// matugen's error output, made readable: which template, where, and what.
pub fn matugen_error(text: &str) -> String {
    let plain = strip_ansi(text);
    let lines: Vec<&str> = plain.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    // "╭─[ zed:53:39 ]" names the template and place; the line with "╰──" says what's wrong.
    let place = lines.iter().find_map(|l| l.strip_prefix("╭─[").map(|r| r.trim_end_matches(']').trim().to_string()));
    let what = lines.iter().find_map(|l| l.rsplit_once("──").filter(|_| l.contains('╰')).map(|(_, m)| m.trim().to_string())).filter(|m| !m.is_empty());
    let kind = lines.iter().find_map(|l| l.strip_prefix("Error:").map(|r| r.trim().to_string()));
    match (place, what) {
        (Some(p), Some(w)) => {
            let template = p.split(':').next().unwrap_or(&p).to_string();
            format!("the template \"{template}\" ({p}): {w}")
        }
        _ => kind.or_else(|| lines.iter().rev().find(|l| !l.starts_with("Run with") && !l.starts_with("Backtrace")).map(|l| l.to_string())).unwrap_or_else(|| "matugen failed".into()),
    }
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // ESC [ … letter
            if chars.peek() == Some(&'[') {
                chars.next();
                for d in chars.by_ref() {
                    if d.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

fn hex(c: eframe::egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

fn parse(v: Option<&serde_json::Value>, mode: &str) -> Option<eframe::egui::Color32> {
    let h = v?.get(mode)?.as_str()?.trim_start_matches('#');
    let n = u32::from_str_radix(h.get(..6)?, 16).ok()?;
    Some(eframe::egui::Color32::from_rgb((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// A sixteen-colour terminal palette (`dank16.colorN.dark.hex` / `.light.hex`) from matugen's
/// colours for a picture: its red, green… when the config has them (custom colours, already
/// matched to the picture), else Rivet's leaning toward the picture's primary, secondary and
/// tertiary.
pub fn dank16(colors: &serde_json::Value) -> serde_json::Value {
    use eframe::egui::Color32;
    use rivet_look::palette::mix;
    let get = |name: &str, mode: &str| parse(colors.get(name), mode);
    let rgb = |h: u32| Color32::from_rgb((h >> 16) as u8, (h >> 8) as u8, h as u8);
    // Red, green, yellow, blue, magenta, cyan: for dark backgrounds, then for light ones.
    let base = [[0xe8706a, 0x9fcf7c, 0xe9c46a, 0x6faee8, 0xc792e0, 0x5fc6c0], [0xc0392b, 0x3d8a28, 0x9a6b00, 0x1f64b8, 0x8b3fb8, 0x0f8079]];
    let names: [&[&str]; 6] = [&["red"], &["green"], &["yellow"], &["blue"], &["purple", "pink", "magenta"], &["cyan"]];
    let mut out = serde_json::Map::new();
    let mut put = |i: usize, dark: Color32, light: Color32| {
        out.insert(format!("color{i}"), serde_json::json!({ "dark": { "hex": hex(dark) }, "light": { "hex": hex(light) } }));
    };
    let mut set: [[Color32; 16]; 2] = [[Color32::BLACK; 16]; 2];
    for (m, mode) in ["dark", "light"].iter().enumerate() {
        let dark = m == 0;
        let trio = [get("primary", mode), get("secondary", mode), get("tertiary", mode)];
        let trio = [trio[0].unwrap_or(rgb(0xe66100)), trio[1].or(trio[0]).unwrap_or(rgb(0xe66100)), trio[2].or(trio[0]).unwrap_or(rgb(0xe66100))];
        let surface = get("surface", mode).unwrap_or(if dark { rgb(0x1e1e1e) } else { rgb(0xfafafa) });
        let text = get("on_surface", mode).unwrap_or(if dark { rgb(0xeeeeee) } else { rgb(0x222222) });
        for k in 0..6 {
            let c = names[k].iter().find_map(|n| get(n, mode)).unwrap_or_else(|| rivet_look::highlight::lean(rgb(base[m][k]), trio, if k == 0 { 14.0 } else { 34.0 }));
            set[m][k + 1] = c;
            set[m][k + 9] = mix(c, if dark { Color32::WHITE } else { Color32::BLACK }, 0.2);
        }
        set[m][0] = if dark { mix(surface, text, 0.12) } else { mix(text, surface, 0.1) };
        set[m][8] = get("outline", mode).unwrap_or(mix(surface, text, 0.45));
        set[m][7] = if dark { mix(text, surface, 0.12) } else { mix(surface, text, 0.35) };
        set[m][15] = if dark { text } else { mix(surface, text, 0.12) };
    }
    for i in 0..16 {
        put(i, set[0][i], set[1][i]);
    }
    serde_json::Value::Object(out)
}

/// A demo picture for screenshots (a soft gradient): checks never show the person's files.
pub fn demo_png(dir: &Path) -> Option<PathBuf> {
    rivet_lockscreen::demo_wallpaper(dir)
}

// ------------------------------------------------------------------ choosing a picture

/// A picture folder's pictures, listed on a thread and listed again only when the folder (or a
/// folder right inside it) changes, with thumbnails from the desktop's shared cache
/// (`~/.cache/thumbnails`, as file managers keep it): made once, then read back. Settings'
/// Wallpaper page uses it, and a wallpaper popup on the panel can too.
pub struct Gallery {
    dir: PathBuf,
    /// The folder and its folders' modified times when last listed.
    stamp: Vec<(PathBuf, Option<SystemTime>)>,
    pictures: Vec<Picture>,
    #[allow(clippy::type_complexity)]
    listing: Option<Receiver<(PathBuf, Vec<Picture>, Vec<(PathBuf, Option<SystemTime>)>)>>,
    /// Whether the folder has been listed at least once (vs. still being read).
    listed: bool,
    checked: f64,
    thumbs: Option<rivet_media::thumbs::Thumbnailer>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Picture {
    pub path: PathBuf,
    pub mtime: Option<SystemTime>,
    pub size: u64,
}

impl Picture {
    pub fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }
    fn mime(&self) -> &'static str {
        match self.path.extension().and_then(|x| x.to_str()).map(str::to_lowercase).as_deref() {
            Some("png") => "image/png",
            Some("webp") => "image/webp",
            _ => "image/jpeg",
        }
    }
}

/// How often the folder is checked for changes, in seconds.
const RECHECK: f64 = 2.0;

fn stamp_of(dir: &Path, list: &[Picture]) -> Vec<(PathBuf, Option<SystemTime>)> {
    let mut dirs: Vec<PathBuf> = vec![dir.to_path_buf()];
    for p in list {
        if let Some(d) = p.path.parent()
            && !dirs.iter().any(|x| x == d)
        {
            dirs.push(d.to_path_buf());
        }
    }
    // Folders inside without pictures count too (one gaining pictures changes its time).
    for e in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let p = e.path();
        if p.is_dir() && !dirs.contains(&p) && !e.file_name().to_string_lossy().starts_with('.') {
            dirs.push(p);
        }
    }
    dirs.into_iter().map(|d| {
        let t = std::fs::metadata(&d).and_then(|m| m.modified()).ok();
        (d, t)
    }).collect()
}

fn mtimes(stamp: &[(PathBuf, Option<SystemTime>)]) -> Vec<Option<SystemTime>> {
    stamp.iter().map(|(d, _)| std::fs::metadata(d).and_then(|m| m.modified()).ok()).collect()
}

impl Default for Gallery {
    fn default() -> Self {
        Gallery { dir: PathBuf::new(), stamp: Vec::new(), pictures: Vec::new(), listing: None, listed: false, checked: 0.0, thumbs: None }
    }
}

impl Gallery {
    /// Shows `dir` (listing it when it's new or has changed). Call every frame it's shown.
    pub fn frame(&mut self, ctx: &egui::Context, dir: &Path) {
        if dir != self.dir {
            self.dir = dir.to_path_buf();
            self.pictures.clear();
            self.stamp.clear();
            self.listed = false;
            self.relist(ctx);
        }
        if let Some(rx) = &self.listing
            && let Ok((d, list, stamp)) = rx.try_recv()
        {
            self.listing = None;
            if d == self.dir {
                self.pictures = list;
                self.stamp = stamp;
                self.listed = true;
            }
        }
        let now = ctx.input(|i| i.time);
        if self.listing.is_none() && now - self.checked >= RECHECK {
            self.checked = now;
            // Only the folders' times: cheap, so it's done here.
            let changed = self.stamp.is_empty() || mtimes(&self.stamp) != self.stamp.iter().map(|s| s.1).collect::<Vec<_>>();
            if changed {
                self.relist(ctx);
            }
        }
        ctx.request_repaint_after(std::time::Duration::from_secs_f64(RECHECK));
        let thumbs = self.thumbs.get_or_insert_with(|| rivet_media::thumbs::Thumbnailer::new(ctx.clone()));
        thumbs.poll(ctx);
    }

    /// Lists the folder again now (e.g. a "Refresh" button).
    pub fn relist(&mut self, ctx: &egui::Context) {
        let (tx, rx) = channel();
        let dir = self.dir.clone();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let list: Vec<Picture> = pictures(&dir)
                .into_iter()
                .take(MAX_PICTURES)
                .map(|path| {
                    let m = std::fs::metadata(&path).ok();
                    Picture { mtime: m.as_ref().and_then(|m| m.modified().ok()), size: m.map(|m| m.len()).unwrap_or(0), path }
                })
                .collect();
            let stamp = stamp_of(&dir, &list);
            let _ = tx.send((dir, list, stamp));
            ctx.request_repaint();
        });
        self.listing = Some(rx);
    }

    pub fn pictures(&self) -> &[Picture] {
        &self.pictures
    }

    /// Still reading the folder for the first time.
    pub fn loading(&self) -> bool {
        !self.listed
    }

    /// A picture's thumbnail, once it's ready.
    pub fn thumb(&mut self, p: &Picture) -> Option<egui::TextureHandle> {
        self.thumbs.as_mut()?.get(&p.path, p.size, p.mime(), p.mtime, 256)
    }
}

/// The most pictures a folder shows.
const MAX_PICTURES: usize = 2000;

/// `~/x` and `$HOME/x` to a full path.
pub fn expand(path: &str) -> PathBuf {
    let path = path.trim();
    let home = rivet_xdg::places::home();
    if path == "~" {
        return home;
    }
    match path.strip_prefix("~/").or_else(|| path.strip_prefix("$HOME/")) {
        Some(rest) => home.join(rest),
        None => PathBuf::from(path),
    }
}

/// A path for people: the home folder as `~`.
pub fn short(path: &Path) -> String {
    let home = rivet_xdg::places::home();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// Folders that usually hold wallpapers, the ones that exist: the pictures folder's
/// Wallpapers, the pictures folder, the desktop's own backgrounds.
pub fn suggested_dirs() -> Vec<PathBuf> {
    let pics = rivet_xdg::places::user_dirs().pictures.unwrap_or_else(rivet_xdg::places::home);
    let mut out: Vec<PathBuf> = ["Wallpapers", "Wallpaper", "wallpapers", "wallpaper"].iter().map(|n| pics.join(n)).collect();
    out.push(pics);
    out.push(rivet_xdg::places::data_home().join("backgrounds"));
    out.push(PathBuf::from("/usr/share/backgrounds"));
    out.push(PathBuf::from("/usr/share/wallpapers"));
    out.retain(|d| d.is_dir());
    out.dedup();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_another_picture() {
        let list: Vec<PathBuf> = ["a.jpg", "b.png", "c.webp"].iter().map(PathBuf::from).collect();
        let now = PathBuf::from("b.png");
        for seed in 0..50 {
            let p = choose(&list, &now, "random", seed).unwrap();
            assert_ne!(p, now);
        }
        assert_eq!(choose(&list, &now, "next", 0), Some(PathBuf::from("c.webp")));
        assert_eq!(choose(&list, Path::new("c.webp"), "next", 0), Some(PathBuf::from("a.jpg")));
        assert_eq!(choose(&list, Path::new("a.jpg"), "previous", 0), Some(PathBuf::from("c.webp")));
        assert_eq!(choose(&list[..1], Path::new("a.jpg"), "random", 7), Some(PathBuf::from("a.jpg")));
        assert_eq!(choose(&[], &now, "random", 1), None);
    }

    #[test]
    fn transitions_start_old_and_end_new() {
        use crate::config::Transition;
        let (w, h) = (16usize, 6usize);
        let (from, to) = (vec![10u8; w * h * 4], vec![200u8; w * h * 4]);
        for kind in Transition::ALL {
            let mut out = vec![0u8; w * h * 4];
            transition(kind, &from, &to, &mut out, w, 1.0);
            assert_eq!(out, to, "{kind:?} ends on the new picture");
            if kind != Transition::None {
                transition(kind, &from, &to, &mut out, w, 0.0);
                assert_eq!(out, from, "{kind:?} starts on the old picture");
                transition(kind, &from, &to, &mut out, w, 0.5);
                assert!(out != from && out != to, "{kind:?} is part way at the middle");
            }
        }
    }

    #[test]
    fn covers_and_lists() {
        let dir = std::env::temp_dir().join(format!("rivet-shell-wall-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("more")).unwrap();
        let p = demo_png(&dir).unwrap();
        std::fs::copy(&p, dir.join("more/b.jpg.png")).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let list = pictures(&dir);
        assert_eq!(list.len(), 2);
        assert_eq!(expand("~/Pictures"), rivet_xdg::places::home().join("Pictures"));
        assert_eq!(short(&rivet_xdg::places::home().join("a b")), "~/a b");
        let px = cover_bgra(&p, 100, 100).unwrap();
        assert_eq!(px.len(), 100 * 100 * 4);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        assert_eq!(matugen_args(Path::new("/w.png"), Mode::Light, "scheme-vibrant", 0.0), ["image", "/w.png", "-m", "light", "-t", "scheme-vibrant", "-q"]);
        assert_eq!(matugen_args(Path::new("/w.png"), Mode::Dark, "", -0.5), ["image", "/w.png", "-m", "dark", "--contrast=-0.50", "-q"]);
        // A palette for templates that expect dank16, with or without custom colours.
        let c = serde_json::json!({ "primary": { "dark": "#ccbeff", "light": "#625690" }, "red": { "dark": "#ff8080", "light": "#a01010" } });
        let d = dank16(&c);
        assert_eq!(d["color1"]["dark"]["hex"], "#ff8080");
        assert_eq!(d["color1"]["light"]["hex"], "#a01010");
        assert!(d["color15"]["light"]["hex"].as_str().unwrap().starts_with('#'));
        assert_eq!(d.as_object().unwrap().len(), 16);
        // matugen's coloured error output, made readable.
        let out = "\x1b[31mError:\x1b[0m ResolveError\n    ╭─[ zed:53:39 ]\n 53 │ \"color\": \"{{dank16.color4.light.hex}}\",\n    │      ╰──────── Value does not exist in the context\n";
        assert_eq!(matugen_error(out), "the template \"zed\" (zed:53:39): Value does not exist in the context");
        std::fs::remove_dir_all(&dir).ok();
    }
}
