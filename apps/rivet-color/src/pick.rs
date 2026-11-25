//! Picking a color from anywhere on the screen. A window can't reach past itself on Wayland, so
//! this uses Rivet Color's own picker with its loupe (`overlay`) where it can, and otherwise
//! asks whatever can: the compositor (niri), a picker made for it (hyprpicker on
//! Hyprland), grim with slurp on the compositors that let programs read the screen, and last
//! the desktop portal, which GNOME and KDE answer. Each blocks until the color is clicked or
//! the pick is cancelled, so it runs on a thread.

use std::io::Read;
use std::process::{Command, Stdio};

use crate::color::{Rgb, parse};

/// Is a program on the PATH?
pub fn has(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

/// The color picked, `Ok(None)` when it was cancelled, or why nothing could pick.
pub fn pick() -> Result<Option<Rgb>, String> {
    let wayland = std::env::var_os("WAYLAND_DISPLAY").is_some();
    // Our own picker, with the loupe, wherever the screen can be captured and shown over.
    if wayland
        && has("grim")
        && !gnome_or_kde()
        && let Ok(answer) = crate::overlay::pick()
    {
        return Ok(answer);
    }
    if wayland
        && std::env::var_os("NIRI_SOCKET").is_some()
        && has("niri")
        && let Some(answer) = niri()
    {
        return answer;
    }
    if wayland && has("hyprpicker") {
        return hyprpicker();
    }
    if wayland && has("grim") && has("slurp") && !gnome_or_kde() {
        return grim();
    }
    match rivet_xdg::portal::pick_color() {
        Ok(c) => Ok(c.map(|[r, g, b]| Rgb((r.clamp(0.0, 1.0) * 255.0).round() as u8, (g.clamp(0.0, 1.0) * 255.0).round() as u8, (b.clamp(0.0, 1.0) * 255.0).round() as u8))),
        Err(_) if wayland => Err("Picking needs hyprpicker, or grim and slurp, on this desktop. Install one to pick from the screen.".into()),
        Err(_) => Err("This desktop can't pick a color from the screen.".into()),
    }
}

/// GNOME and KDE take their own screenshots and don't let grim read the screen.
fn gnome_or_kde() -> bool {
    let d = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
    d.contains("gnome") || d.contains("kde")
}

/// niri's own picker (`niri msg pick-color`, from niri 25.05). None when this niri is older.
fn niri() -> Option<Result<Option<Rgb>, String>> {
    let out = Command::new("niri").args(["msg", "--json", "pick-color"]).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    if v.is_null() {
        return Some(Ok(None));
    }
    let rgb = v.get("rgb")?.as_array()?;
    let ch = |i: usize| rgb.get(i).and_then(|x| x.as_f64()).map(|x| (x.clamp(0.0, 1.0) * 255.0).round() as u8);
    Some(Ok(Some(Rgb(ch(0)?, ch(1)?, ch(2)?))))
}

fn hyprpicker() -> Result<Option<Rgb>, String> {
    let out = Command::new("hyprpicker").args(["--format=hex", "--no-fancy"]).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| format!("Couldn't run hyprpicker: {e}."))?;
    // It prints nothing when Escape cancels it.
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().find_map(parse))
}

/// slurp chooses a point, grim takes a picture of it.
fn grim() -> Result<Option<Rgb>, String> {
    let at = Command::new("slurp").args(["-p", "-b", "#00000000"]).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| format!("Couldn't run slurp: {e}."))?;
    // slurp leaves with an error when Escape cancels it.
    let geometry = String::from_utf8_lossy(&at.stdout).trim().to_string();
    if !at.status.success() || geometry.is_empty() {
        return Ok(None);
    }
    let mut child = Command::new("grim").args(["-g", &geometry, "-t", "ppm", "-"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("Couldn't run grim: {e}."))?;
    let mut bytes = Vec::new();
    child.stdout.take().map(|mut o| o.read_to_end(&mut bytes));
    let _ = child.wait();
    first_pixel(&bytes).map(Some).ok_or_else(|| "Couldn't read the screen there.".into())
}

/// The first pixel of a binary PPM picture (P6, 8 bits).
fn first_pixel(data: &[u8]) -> Option<Rgb> {
    let (w, h, at) = crate::overlay::ppm(data)?;
    (w > 0 && h > 0).then(|| Rgb(data[at], data[at + 1], data[at + 2]))
}

/// Put text on the clipboard with `wl-copy`, which keeps offering it after this program has
/// gone. False when there's no `wl-copy`.
pub fn copy(text: &str) -> bool {
    if std::env::var_os("WAYLAND_DISPLAY").is_none() || !has("wl-copy") {
        return false;
    }
    Command::new("wl-copy").arg("--").arg(text).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_grims_pixel() {
        let mut ppm = b"P6\n2 1\n255\n".to_vec();
        ppm.extend([213, 63, 140, 0, 0, 0]);
        assert_eq!(first_pixel(&ppm), Some(Rgb(213, 63, 140)));
        let mut odd = b"P6 # a comment\n1 1 255\n".to_vec();
        odd.extend([1, 2, 3]);
        assert_eq!(first_pixel(&odd), Some(Rgb(1, 2, 3)));
        assert_eq!(first_pixel(b"P6\n1 1\n255\n"), None);
        assert_eq!(first_pixel(b"P3\n1 1\n255\n1 2 3"), None);
    }
}
