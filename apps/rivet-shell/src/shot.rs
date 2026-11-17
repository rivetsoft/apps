//! `rivet-shell --screenshot FILE`: the whole desktop in one picture (a demo wallpaper, the
//! widgets, the panel, the dock, a popup when asked), drawn by the same code as the real shell,
//! with made-up status, so checks never show the person's wallpaper or machine.
//!
//! Hooks: RIVET_SHELL_POPUP=launcher|calendar|power, RIVET_SHELL_THEME=light|dark,
//! RIVET_SHELL_EDGE=bottom, RIVET_SHELL_ACCENT=rrggbb.

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use rivet_look::theme::Mode;

use crate::config::{Edge, Settings};
use crate::sys::{Compositor, Net, Status, Workspace};
use crate::ui::{self, Act, Kit, Model, Popup, PopupKind};

pub struct ShotApp {
    s: Settings,
    kit: Kit,
    status: Status,
    apps: Vec<rivet_xdg::launch::DesktopApp>,
    wall: Option<egui::TextureHandle>,
    popup: Option<Popup>,
    shot: rivet_kit::Shot,
}

fn demo_status() -> Status {
    let ws = |k: &str, active: bool, busy: bool| Workspace { key: k.into(), label: k.into(), active, busy, output: String::new() };
    Status {
        workspaces: vec![ws("1", false, true), ws("2", true, true), ws("3", false, true), ws("4", false, false)],
        running: vec!["rivet-files".into(), "rivet-terminal".into()],
        volume: Some((45, false)),
        net: Net::Wifi("Home".into()),
        battery: Some((76, false)),
        cpu: 23.0,
        mem: (6_700_000_000, 16_000_000_000),
    }
}

impl ShotApp {
    pub fn new(cc: &eframe::CreationContext, path: std::path::PathBuf) -> ShotApp {
        crate::config::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut s = Settings::default();
        match std::env::var("RIVET_SHELL_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
            _ => {}
        }
        if let Some(n) = std::env::var("RIVET_SHELL_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
            s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            s.follow_desktop_colors = false;
        }
        if std::env::var("RIVET_SHELL_EDGE").as_deref() == Ok("bottom") {
            s.panel_edge = Edge::Bottom;
        }
        let dir = std::env::temp_dir().join(format!("rivet-shell-shot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wall = crate::wall::demo_png(&dir).and_then(|p| crate::wall::thumbnail(&p, 1100)).map(|img| cc.egui_ctx.load_texture("demo-wallpaper", img, egui::TextureOptions::LINEAR));
        let _ = std::fs::remove_dir_all(&dir);
        let popup = match std::env::var("RIVET_SHELL_POPUP").as_deref() {
            Ok("launcher") => Some(Popup::launcher("")),
            Ok("calendar") => Some(Popup { kind: PopupKind::Calendar { month: 0 }, anchor_x: 550.0, output: String::new(), focus: false }),
            Ok("power") => Some(Popup { kind: PopupKind::Power { confirm: None }, anchor_x: 1080.0, output: String::new(), focus: false }),
            _ => None,
        };
        let mut apps = rivet_xdg::launch::all_apps();
        apps.sort_by_key(|a| a.name.to_lowercase());
        ShotApp { s, kit: Kit::new(false), status: demo_status(), apps, wall, popup, shot: rivet_kit::Shot::new(path) }
    }
}

impl eframe::App for ShotApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.kit.begin(&ctx, &self.s);
        let full = ui.max_rect();
        let p = ui.painter().clone();
        match &self.wall {
            Some(t) => {
                p.image(t.id(), full, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                p.rect_filled(full, 0.0, ui.visuals().extreme_bg_color);
            }
        }
        let mut acts: Vec<Act> = Vec::new();
        let s = self.s.clone();
        let mut m = Model { s: &s, status: &self.status, apps: &self.apps, compositor: Compositor::Niri, output: "", acts: &mut acts };
        let mut at = |r: Rect, f: &mut dyn FnMut(&mut egui::Ui, &mut Model, &mut Kit)| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
            f(&mut child, &mut m, &mut self.kit);
        };
        at(full, &mut |ui, m, k| ui::widgets(ui, m, k));
        let ph = ui::panel_height(&s);
        let panel = if s.panel_edge == Edge::Top { Rect::from_min_size(full.min, Vec2::new(full.width(), ph)) } else { Rect::from_min_size(Pos2::new(full.left(), full.bottom() - ph), Vec2::new(full.width(), ph)) };
        at(panel, &mut |ui, m, k| ui::panel(ui, m, k));
        let n = s.dock_apps.iter().filter(|id| ui::find_app(&self.apps, id).is_some()).count();
        let (dsize, _) = ui::dock_size(&s, n);
        let bottom = if s.panel_edge == Edge::Bottom { full.bottom() - ph } else { full.bottom() };
        let dock = Rect::from_min_size(Pos2::new(full.center().x - dsize.x / 2.0, bottom - dsize.y), dsize);
        at(dock, &mut |ui, m, k| ui::dock(ui, m, k));
        if let Some(pop) = self.popup.as_mut() {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
            ui::popup(&mut child, pop, &mut m, &mut self.kit);
        }
        self.shot.step(&ctx);
    }
}
