//! The wallpaper: the pictures to choose from, one scaled to fill a screen, and matugen run on
//! it so the desktop's colours (and so every Rivet app's) follow the picture.

use std::path::{Path, PathBuf};

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

/// A picture scaled to cover `w`×`h` (cropping what overflows, centred), as BGRA bytes for a
/// Wayland buffer.
pub fn cover_bgra(path: &Path, w: u32, h: u32) -> Option<Vec<u8>> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let (iw, ih) = (img.width().max(1), img.height().max(1));
    let scale = (w as f32 / iw as f32).max(h as f32 / ih as f32);
    let (sw, sh) = (((iw as f32 * scale).ceil() as u32).max(w), ((ih as f32 * scale).ceil() as u32).max(h));
    let resized = img.resize_exact(sw, sh, image::imageops::FilterType::Triangle).to_rgba8();
    let (x0, y0) = ((sw - w) / 2, (sh - h) / 2);
    let mut out = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let p = resized.get_pixel(x + x0, y + y0).0;
            out.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Some(out)
}

/// A small copy for choosing from (RGBA, longest side `px`).
pub fn thumbnail(path: &Path, px: u32) -> Option<eframe::egui::ColorImage> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let t = img.thumbnail(px, px).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([t.width() as usize, t.height() as usize], t.as_raw()))
}

/// The arguments matugen gets for a picture.
pub fn matugen_args(path: &Path, mode: Mode, scheme: &str) -> Vec<String> {
    let mode = if mode == Mode::Light { "light" } else { "dark" };
    let mut a = vec!["image".to_string(), path.to_string_lossy().into_owned(), "-m".into(), mode.into()];
    if !scheme.is_empty() {
        a.extend(["-t".into(), scheme.to_string()]);
    }
    a.push("-q".into());
    a
}

/// Run matugen on a picture, in the background: it writes the colour files every app reads
/// (and anything else in its config). The result comes back to `done`.
pub fn run_matugen(path: &Path, mode: Mode, scheme: &str, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    let args = matugen_args(path, mode, scheme);
    std::thread::spawn(move || {
        let r = std::process::Command::new("matugen")
            .args(&args)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .output()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "matugen isn't installed".to_string() } else { e.to_string() })
            .and_then(|o| if o.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&o.stderr).lines().last().unwrap_or("matugen failed").to_string()) });
        done(r);
    });
}

/// A demo picture for screenshots (a soft gradient): checks never show the person's files.
pub fn demo_png(dir: &Path) -> Option<PathBuf> {
    let (w, h) = (640u32, 400u32);
    let img = image::RgbaImage::from_fn(w, h, |x, y| {
        let fx = x as f32 / w as f32;
        let fy = y as f32 / h as f32;
        let r = (40.0 + 120.0 * fx * (1.0 - fy)) as u8;
        let g = (36.0 + 60.0 * fy) as u8;
        let b = (70.0 + 110.0 * (1.0 - fx) + 40.0 * fy) as u8;
        image::Rgba([r, g, b, 255])
    });
    let p = dir.join("demo-wallpaper.png");
    img.save(&p).ok()?;
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_and_lists() {
        let dir = std::env::temp_dir().join(format!("rivet-shell-wall-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("more")).unwrap();
        let p = demo_png(&dir).unwrap();
        std::fs::copy(&p, dir.join("more/b.jpg.png")).unwrap();
        std::fs::write(dir.join("notes.txt"), "x").unwrap();
        let list = pictures(&dir);
        assert_eq!(list.len(), 2);
        let px = cover_bgra(&p, 100, 100).unwrap();
        assert_eq!(px.len(), 100 * 100 * 4);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] == 255));
        assert_eq!(matugen_args(Path::new("/w.png"), Mode::Light, "scheme-vibrant"), ["image", "/w.png", "-m", "light", "-t", "scheme-vibrant", "-q"]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
