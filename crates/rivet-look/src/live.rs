//! What every Rivet app does to look like the others: the desktop's UI font, the theme built
//! from a light/dark choice and an accent, and the desktop's generated colors followed live
//! (their files are checked every two seconds).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;

use eframe::egui::{self, Color32};

use crate::{fonts, palette, theme};

#[derive(Default)]
pub struct LiveLook {
    palette: Option<palette::Palette>,
    watched: Vec<(PathBuf, Option<SystemTime>)>,
    checked: f64,
    key: Option<String>,
    fonts_set: bool,
}

fn mtime(p: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// The app's choices.
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub mode: theme::Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Text size in points; 0 follows the desktop's.
    pub text_size: f32,
}

impl LiveLook {
    pub fn new() -> LiveLook {
        let (pal, watched) = palette::load();
        LiveLook { palette: pal, watched: watched.into_iter().map(|p| { let t = mtime(&p); (p, t) }).collect(), ..Default::default() }
    }

    /// The desktop's colors, when there are any.
    pub fn palette(&self) -> Option<&palette::Palette> {
        self.palette.as_ref()
    }

    /// The accent in use: the desktop's when followed, else the chosen one.
    pub fn accent(&self, c: &Choice) -> Color32 {
        let pal = if c.follow_desktop_colors { self.palette.as_ref() } else { None };
        pal.and_then(|p| p.accent).unwrap_or(Color32::from_rgb(c.accent[0], c.accent[1], c.accent[2]))
    }

    /// Call once per frame, first. `live`: keep checking the desktop's colors (not in checks).
    pub fn frame(&mut self, ctx: &egui::Context, c: &Choice, live: bool) {
        if !self.fonts_set {
            self.fonts_set = true;
            let mut defs = egui::FontDefinitions::default();
            if let Some((_, path)) = fonts::resolve("")
                && let Ok(bytes) = std::fs::read(&path)
            {
                defs.font_data.insert("ui-font".into(), Arc::new(egui::FontData::from_owned(bytes)));
                defs.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui-font".into());
            }
            ctx.set_fonts(defs);
        }
        let now = ctx.input(|i| i.time);
        if live && (now - self.checked >= 2.0 || self.checked == 0.0) {
            self.checked = now;
            if self.watched.iter().any(|(p, t)| mtime(p) != *t) {
                *self = LiveLook { fonts_set: true, key: self.key.take(), checked: now, ..LiveLook::new() };
            }
            if c.follow_desktop_colors {
                ctx.request_repaint_after(std::time::Duration::from_secs(2));
            }
        }
        let size = if c.text_size > 0.0 { c.text_size } else { fonts::desktop_font_cached().map(|(_, pt)| fonts::points_to_ui(pt)).unwrap_or(fonts::BASE_SIZE) };
        fonts::set_scale(size / fonts::BASE_SIZE);
        let accent = self.accent(c);
        let pal = if c.follow_desktop_colors { self.palette.as_ref() } else { None };
        let key = format!("{:?}{accent:?}{pal:?}{size}", c.mode);
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        theme::apply(ctx, &theme::Look { mode: c.mode, accent, palette: pal, terminal: false });
    }
}
