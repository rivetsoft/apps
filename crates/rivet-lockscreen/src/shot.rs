//! The face drawn in memory over a made-up wallpaper and saved as a picture, for checks and
//! screenshots. Made-up state from the environment: RIVET_LOCK_TYPED=N (dots),
//! RIVET_LOCK_ERROR=1 (a wrong password, with Caps Lock on), RIVET_LOCK_CONFIRM=1 (Shut Down
//! asking for a second click).

use std::path::PathBuf;
use std::time::Duration;

use eframe::egui::{self, Pos2, Rect};

use crate::face::{self, LockAct};
use crate::look::{Look, Theme};

/// Draw the screen at 1280 by 800 and save it to `path`, over `wallpaper` (else a made-up one).
/// `greeter`: the login screen, with its hint for other accounts.
pub fn run(path: PathBuf, look: Look, greeter: bool, wallpaper: Option<PathBuf>) {
    rivet_kit::headless::run([1280.0, 800.0], move |cc| Box::new(Shot::new(cc, path, look, greeter, wallpaper)));
}

struct Shot {
    look: Look,
    theme: Theme,
    wall: Option<egui::TextureHandle>,
    shot: rivet_kit::Shot,
    greeter: bool,
}

impl Shot {
    fn new(cc: &eframe::CreationContext, path: PathBuf, look: Look, greeter: bool, wallpaper: Option<PathBuf>) -> Shot {
        let dir = std::env::temp_dir().join(format!("rivet-lock-shot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wall = wallpaper.or_else(|| crate::demo_wallpaper(&dir)).and_then(|p| {
            let px = crate::background(&p, 1280, 800, true)?;
            // BGRA, blurred: back to RGBA for egui.
            let rgba: Vec<u8> = px.as_chunks::<4>().0.iter().flat_map(|c| [c[2], c[1], c[0], 255]).collect();
            Some(egui::ColorImage::from_rgba_unmultiplied([1280, 800], &rgba))
        });
        let _ = std::fs::remove_dir_all(&dir);
        let wall = wall.map(|img| cc.egui_ctx.load_texture("lock-wall", img, egui::TextureOptions::LINEAR));
        Shot { look, theme: Theme::new(false), wall, shot: rivet_kit::Shot::new(path), greeter }
    }
}

impl eframe::App for Shot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.theme.begin(&ctx, &self.look);
        let full = ui.max_rect();
        if let Some(t) = &self.wall {
            ui.painter().image(t.id(), full, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
        }
        let env = |k: &str| std::env::var(k).ok();
        let error = env("RIVET_LOCK_ERROR").as_deref() == Some("1");
        let view = face::View {
            name: "Sam Rivers",
            avatar: None,
            typed: env("RIVET_LOCK_TYPED").and_then(|v| v.parse().ok()).unwrap_or(0),
            caps_lock: error,
            busy: false,
            message: error.then_some(("Wrong password", true)).or(self.greeter.then_some((face::ANOTHER, false))),
            since_error: error.then_some(Duration::from_secs(1)),
            power: true,
            confirm: (env("RIVET_LOCK_CONFIRM").as_deref() == Some("1")).then_some(LockAct::ShutDown),
            accent: self.theme.accent(&self.look),
            clock_format: &self.look.clock_format,
        };
        face::draw(ui, &view);
        self.shot.step(&ctx);
    }
}
