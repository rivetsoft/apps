//! What Rivet Shell's lock screen and Rivet Greeter (greetd's login screen) share, so the two
//! look the same and stay that way:
//!
//! - `face`: the screen itself, drawn with egui: the time, who, the password field, power.
//! - `look`: the part of a person's settings the screen uses (accent, style, font, clock), as
//!   text and as an egui theme.
//! - `screen`: the Wayland program that shows the face on every screen and asks for the
//!   password, as a session lock, a layer over the desktop or one full-screen window.
//! - `handoff`: `/var/lib/rivet-greeter`, where each person's session leaves its wallpaper,
//!   look, name, picture and session for the login screen, and the login screen finds them.
//! - `shot`: the face drawn in memory and saved as a picture, for checks and screenshots.
//!
//! Checking the password is left to the program: PAM for the lock screen, greetd for the
//! greeter.

pub mod face;
pub mod handoff;
pub mod look;
pub mod screen;
pub mod shot;

pub use face::glyph;
pub use look::Look;

use std::path::{Path, PathBuf};

/// Someone the screen is for: who, how their screen looks, and the session to start (the
/// greeter's).
#[derive(Clone, Debug)]
pub struct Person {
    pub login: String,
    pub name: String,
    pub look: Look,
    pub wallpaper: Option<PathBuf>,
    /// Pictures of them, tried in order.
    pub faces: Vec<PathBuf>,
    pub session: Vec<String>,
}

/// Wipe a password from memory, then empty it.
pub fn wipe(s: &mut String) {
    // SAFETY: zeros are valid UTF-8.
    unsafe { s.as_bytes_mut().fill(0) };
    s.clear();
}

/// The login name of who's running this: the real user, not $USER (which anyone can set).
pub fn login_name() -> String {
    let uid = unsafe { libc::getuid() };
    let pw = unsafe { libc::getpwuid(uid) };
    if !pw.is_null() {
        let name = unsafe { std::ffi::CStr::from_ptr((*pw).pw_name) };
        return name.to_string_lossy().into_owned();
    }
    std::env::var("USER").unwrap_or_default()
}

/// A person's picture as AccountsService keeps it.
pub fn accounts_face(login: &str) -> PathBuf {
    PathBuf::from(format!("/var/lib/AccountsService/icons/{login}"))
}

/// The first of these pictures that opens, cut square.
pub fn avatar(paths: &[PathBuf]) -> Option<eframe::egui::ColorImage> {
    let img = paths.iter().filter(|p| p.is_file()).find_map(|p| image::ImageReader::open(p).ok()?.with_guessed_format().ok()?.decode().ok())?;
    let side = img.width().min(img.height());
    let sq = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).resize_exact(192, 192, image::imageops::FilterType::Triangle).to_rgba8();
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied([192, 192], sq.as_raw()))
}

/// The wallpaper for a screen of this size (BGRA), blurred when asked.
pub fn background(path: &Path, w: u32, h: u32, blur: bool) -> Option<Vec<u8>> {
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?.into_rgba8();
    let (iw, ih) = (img.width(), img.height());
    let px = rivet_kit::scale::cover_bgra(img.into_raw(), iw, ih, w, h)?;
    if !blur {
        return Some(px);
    }
    // Down to a small picture, softened, and back up: a wide blur for little work.
    let img = image::RgbaImage::from_raw(w, h, px)?;
    let (sw, sh) = ((w / 24).max(8), (h / 24).max(8));
    let small = image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle);
    let small = image::imageops::blur(&small, 2.0);
    let big = image::imageops::resize(&small, w, h, image::imageops::FilterType::CatmullRom);
    Some(big.into_raw())
}

/// A made-up wallpaper (a soft gradient) saved in `dir`, for checks and screenshots.
pub fn demo_wallpaper(dir: &Path) -> Option<PathBuf> {
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
