//! The person's picture: the file `~/.face`, which the lock screen, the login screen and other
//! desktops' own read too. Set and taken away from the User page; shown by the panel's User item.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui;

/// How big a picture is kept: square, this many pixels a side.
const SIDE: u32 = 512;

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}

/// The picture's file: `~/.face`, or the older `~/.face.icon`, or the one the system keeps
/// for the account (set from another desktop's settings).
pub fn face() -> Option<PathBuf> {
    let login = std::env::var("USER").unwrap_or_default();
    [home().join(".face"), home().join(".face.icon"), PathBuf::from(format!("/var/lib/AccountsService/icons/{login}"))].into_iter().find(|p| p.is_file())
}

/// Whether the picture is one this page can take away (the system's own copy isn't).
pub fn own_face() -> bool {
    [home().join(".face"), home().join(".face.icon")].iter().any(|p| p.is_file())
}

/// A picture cut to its middle square and made `side` pixels a side.
fn square(path: &Path, side: u32) -> Result<image::RgbaImage, String> {
    let img = image::ImageReader::open(path).and_then(|r| r.with_guessed_format()).map_err(|e| e.to_string())?.decode().map_err(|e| e.to_string())?;
    let s = img.width().min(img.height());
    if s == 0 {
        return Err("the picture is empty".to_string());
    }
    Ok(img.crop_imm((img.width() - s) / 2, (img.height() - s) / 2, s, s).resize_exact(side, side, image::imageops::FilterType::Triangle).to_rgba8())
}

/// Make `from` the person's picture: its middle square, kept as `~/.face`. The old one is
/// replaced only once the new one is written whole.
pub fn set_face(from: &Path) -> Result<(), String> {
    set_face_in(&home(), from)
}

fn set_face_in(home: &Path, from: &Path) -> Result<(), String> {
    let img = square(from, SIDE)?;
    let part = home.join(".face.rivet-part");
    img.save_with_format(&part, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    std::fs::rename(&part, home.join(".face")).map_err(|e| e.to_string())
}

/// Take the picture away (both of its names).
pub fn remove_face() -> Result<(), String> {
    for p in [home().join(".face"), home().join(".face.icon")] {
        if p.is_file() {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// The picture as a texture, made once and again only when its file changes.
pub fn texture(ctx: &egui::Context) -> Option<egui::TextureHandle> {
    type Kept = (PathBuf, SystemTime, Option<egui::TextureHandle>);
    let id = egui::Id::unique("rivet-user-face");
    let path = face()?;
    let changed = std::fs::metadata(&path).and_then(|m| m.modified()).ok()?;
    if let Some((p, at, tex)) = ctx.data(|d| d.get_temp::<Kept>(id))
        && p == path
        && at == changed
    {
        return tex;
    }
    let tex = square(&path, 192).ok().map(|img| ctx.load_texture("rivet-user-face", egui::ColorImage::from_rgba_unmultiplied([192, 192], img.as_raw()), egui::TextureOptions::LINEAR));
    ctx.data_mut(|d| d.insert_temp::<Kept>(id, (path, changed, tex.clone())));
    tex
}

/// The picture in a circle at `c`, or the name's first letter on the accent when there is none.
pub fn draw(ui: &egui::Ui, c: egui::Pos2, size: f32, name: &str, accent: egui::Color32) {
    let r = egui::Rect::from_center_size(c, egui::Vec2::splat(size));
    match texture(ui.ctx()) {
        Some(t) => crate::wall_pick::cover(ui.painter(), r, &t, size / 2.0),
        None => {
            let v = ui.visuals();
            ui.painter().circle_filled(c, size / 2.0, accent);
            let letter = name.chars().find(|ch| ch.is_alphanumeric()).map(|ch| ch.to_uppercase().to_string()).unwrap_or_default();
            ui.painter().text(c, egui::Align2::CENTER_CENTER, letter, crate::ui::ui_font(size * 0.48), v.selection.stroke.color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_becomes_a_square_face() {
        let dir = std::env::temp_dir().join(format!("rivet-face-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let wide = dir.join("wide.png");
        image::RgbaImage::from_pixel(300, 100, image::Rgba([10, 20, 30, 255])).save(&wide).unwrap();
        set_face_in(&dir, &wide).unwrap();
        // The file has no ending to go by, so it is read by what is in it.
        let face = image::ImageReader::open(dir.join(".face")).unwrap().with_guessed_format().unwrap().decode().unwrap();
        assert_eq!((face.width(), face.height()), (SIDE, SIDE));
        // Something that isn't a picture leaves the old one alone.
        std::fs::write(dir.join("notes.txt"), "hello").unwrap();
        assert!(set_face_in(&dir, &dir.join("notes.txt")).is_err());
        assert!(dir.join(".face").is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
