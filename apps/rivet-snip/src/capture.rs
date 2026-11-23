//! Getting a picture when none was handed over: of the screen, or from the clipboard.
//!
//! A compositor that takes its own screenshots passes the file. Elsewhere `rivet-snip capture`
//! takes one itself: with `grim` where the compositor lets programs read the screen (Sway,
//! Hyprland, niri, Softcore and the like), else by asking the desktop (GNOME, KDE).

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use eframe::egui;

/// Is `name` a program that can be run?
pub fn has(name: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(name).is_file()))
}

/// A picture from the bytes of a PNG, JPEG or WebP file.
pub fn decode(bytes: &[u8]) -> Result<egui::ColorImage, String> {
    let img = image::load_from_memory(bytes).map_err(|e| format!("it isn't a picture this can read ({e})"))?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    if size[0] == 0 || size[1] == 0 {
        return Err("the picture is empty".into());
    }
    Ok(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

pub fn open(path: &Path) -> Result<egui::ColorImage, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Couldn't read {}: {e}", path.display()))?;
    decode(&bytes).map_err(|e| format!("Couldn't open {}: {e}", path.display()))
}

/// The picture on standard input (`rivet-snip -`).
pub fn stdin() -> Result<egui::ColorImage, String> {
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).map_err(|e| format!("couldn't read the picture: {e}"))?;
    if bytes.is_empty() {
        return Err("no picture came in".into());
    }
    decode(&bytes)
}

/// A picture of the whole screen.
pub fn screen() -> Result<egui::ColorImage, String> {
    let mut why = String::new();
    if std::env::var_os("WAYLAND_DISPLAY").is_some() && has("grim") {
        match Command::new("grim").args(["-t", "png", "-"]).stdin(Stdio::null()).stderr(Stdio::piped()).output() {
            Ok(o) if o.status.success() && !o.stdout.is_empty() => return decode(&o.stdout),
            Ok(o) => why = String::from_utf8_lossy(&o.stderr).trim().to_string(),
            Err(e) => why = e.to_string(),
        }
    }
    match rivet_xdg::portal::screenshot() {
        Ok(Some(file)) => {
            let img = open(&file);
            // The desktop leaves its picture in the Pictures folder; the one kept is the one saved here.
            let _ = std::fs::remove_file(&file);
            img
        }
        Ok(None) => Err("the desktop didn't allow a screenshot".into()),
        Err(e) if why.is_empty() => Err(e),
        Err(e) => Err(format!("{why}; {e}")),
    }
}

/// The picture on the clipboard, when there is one (asked of `wl-paste`).
pub fn clipboard() -> Result<egui::ColorImage, String> {
    if !has("wl-paste") {
        return Err("reading the clipboard from the command line needs wl-paste (wl-clipboard)".into());
    }
    let o = Command::new("wl-paste").args(["--type", "image/png"]).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| e.to_string())?;
    if !o.status.success() || o.stdout.is_empty() {
        return Err("there is no picture on the clipboard".into());
    }
    decode(&o.stdout)
}

/// Put a PNG on the clipboard with `wl-copy`, which keeps offering it after this window has
/// closed. False when it isn't there or failed: the window's own clipboard is used instead.
pub fn copy_png(png: &[u8]) -> bool {
    use std::io::Write;
    if std::env::var_os("WAYLAND_DISPLAY").is_none() || !has("wl-copy") {
        return false;
    }
    let Ok(mut child) = Command::new("wl-copy").args(["--type", "image/png"]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn() else { return false };
    let wrote = child.stdin.take().is_some_and(|mut s| s.write_all(png).is_ok());
    wrote && child.wait().is_ok_and(|s| s.success())
}

/// The local time as numbers: year, month, day, hour, minute, second.
fn now() -> [i32; 6] {
    // SAFETY: `localtime_r` fills the struct it is given from a time read just before.
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        [tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min, tm.tm_sec]
    }
}

/// A name for a picture saved now, as desktops name their screenshots.
pub fn name_now() -> String {
    let [y, mo, d, h, mi, s] = now();
    format!("Screenshot from {y:04}-{mo:02}-{d:02} {h:02}-{mi:02}-{s:02}.png")
}

/// The folder screenshots are saved in: the one chosen, else Screenshots in Pictures.
pub fn folder(chosen: &str) -> PathBuf {
    match chosen.trim() {
        "" => rivet_xdg::places::user_dirs().pictures.unwrap_or_else(|| rivet_xdg::places::home().join("Pictures")).join("Screenshots"),
        d => match d.strip_prefix("~/") {
            Some(rest) => rivet_xdg::places::home().join(rest),
            None => PathBuf::from(d),
        },
    }
}

/// `file` or, when that is taken, the first of `name 2.png`, `name 3.png`… that isn't.
pub fn free(file: PathBuf) -> PathBuf {
    if !file.exists() {
        return file;
    }
    let stem = file.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    (2..).map(|n| file.with_file_name(format!("{stem} {n}.png"))).find(|p| !p.exists()).unwrap_or(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_survives_png_and_back() {
        let img = egui::ColorImage::new([3, 2], (0..6).map(|i| egui::Color32::from_rgb(i * 40, 255 - i * 40, 7)).collect());
        let png = rivet_kit::png_bytes(&img).unwrap();
        assert_eq!(decode(&png).unwrap(), img);
        assert!(decode(b"not a picture").is_err());
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let dir = std::env::temp_dir().join(format!("rivet-snip-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Shot.png");
        assert_eq!(free(file.clone()), file);
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(free(file.clone()), dir.join("Shot 2.png"));
        std::fs::write(dir.join("Shot 2.png"), b"x").unwrap();
        assert_eq!(free(file), dir.join("Shot 3.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
