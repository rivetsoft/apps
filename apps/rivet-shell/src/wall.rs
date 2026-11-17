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
///
/// Templates written for other shells may use colours those shells pass in, such as
/// DankMaterialShell's `dank16` (a sixteen-colour terminal palette): they're made here from the
/// picture's own colours and passed in too, so such templates work and matugen doesn't stop
/// halfway (it stops at the first template that fails, leaving the rest unwritten).
pub fn run_matugen(path: &Path, mode: Mode, scheme: &str, done: impl FnOnce(Result<(), String>) + Send + 'static) {
    let mut args = matugen_args(path, mode, scheme);
    let probe: Vec<String> = matugen_args(path, mode, scheme).into_iter().filter(|a| a != "-q").chain(["--dry-run".into(), "--json".into(), "hex".into()]).collect();
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
