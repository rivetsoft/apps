//! `rivet-shell --screenshot FILE`: the whole desktop in one picture (a demo wallpaper, the
//! widgets, the panel, the dock, a popup when asked), drawn by the same code as the real shell,
//! with made-up status, so checks never show the person's wallpaper or machine.
//!
//! Hooks: RIVET_SHELL_POPUP=launcher|calendar|power|control|dock-menu, RIVET_SHELL_THEME=light|dark,
//! RIVET_SHELL_EDGE=bottom, RIVET_SHELL_ACCENT=rrggbb, RIVET_SHELL_CARDS=1 (notification cards),
//! RIVET_SHELL_OSD=1 (the volume display), RIVET_SHELL_QUERY=text (typed into the launcher).

use std::time::Duration;

use chrono::Local;
use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use rivet_look::theme::Mode;

use crate::config::{Edge, Settings};
use crate::notes::Note;
use crate::sys::{Compositor, Media, Net, Status, Workspace};
use crate::ui::{self, Act, Extra, Kit, Model, OsdKind, Popup, PopupKind};

pub struct ShotApp {
    s: Settings,
    kit: Kit,
    status: Status,
    extra: Extra,
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
        window: "Rivet Files — Documents".into(),
        volume: Some((45, false)),
        mic: Some((80, false)),
        brightness: Some(70),
        media: Some(Media { player: "Music".into(), title: "Low Tide".into(), artist: "The Harbor Lights".into(), playing: true }),
        net: Net::Wifi("Home".into()),
        wifi_on: Some(true),
        bluetooth: Some(false),
        profile: Some("balanced".into()),
        battery: Some((76, false)),
        cpu: 23.0,
        mem: (6_700_000_000, 16_000_000_000),
    }
}

fn demo_notes() -> Vec<Note> {
    let now = Local::now();
    let note = |id: u32, app: &str, icon: &str, summary: &str, body: &str, mins: i64, actions: &[(&str, &str)]| Note {
        id,
        app: app.into(),
        icon: icon.into(),
        desktop: String::new(),
        summary: summary.into(),
        body: body.into(),
        actions: actions.iter().map(|(k, l)| (k.to_string(), l.to_string())).collect(),
        urgency: 1,
        show_for: Some(Duration::from_secs(6)),
        at: now - chrono::Duration::minutes(mins),
    };
    vec![
        note(3, "Rivet Files", "rivet-files", "Copy finished", "42 files copied to Backups", 0, &[("open", "Open Folder")]),
        note(2, "Calendar", "x-office-calendar", "Team check-in in 10 minutes", "", 4, &[]),
        note(1, "Updates", "system-software-update", "Updates are ready", "Restart when it suits you to finish installing them.", 62, &[]),
    ]
}

impl ShotApp {
    pub fn new(cc: &eframe::CreationContext, path: std::path::PathBuf) -> ShotApp {
        crate::config::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        let mut s = Settings::default();
        match std::env::var("RIVET_SHELL_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors, s.color_mode) = (Mode::Light, false, Mode::Light),
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
        let query = std::env::var("RIVET_SHELL_QUERY").unwrap_or_default();
        let popup = match std::env::var("RIVET_SHELL_POPUP").as_deref() {
            Ok("launcher") => Some(Popup { kind: PopupKind::Launcher { query, sel: 0 }, anchor_x: 0.0, output: String::new(), focus: false }),
            Ok("calendar") => Some(Popup { kind: PopupKind::Calendar { month: 0 }, anchor_x: 550.0, output: String::new(), focus: false }),
            Ok("power") => Some(Popup { kind: PopupKind::Power { confirm: None }, anchor_x: 1080.0, output: String::new(), focus: false }),
            Ok("control") => Some(Popup { kind: PopupKind::Control { drag: None }, anchor_x: 1050.0, output: String::new(), focus: false }),
            Ok("dock-menu") => Some(Popup { kind: PopupKind::DockMenu { app: "rivet-terminal".into(), pinned: true }, anchor_x: -50.0, output: String::new(), focus: false }),
            _ => None,
        };
        let mut apps = rivet_xdg::launch::all_apps();
        apps.sort_by_key(|a| a.name.to_lowercase());
        let notes = demo_notes();
        let on = |k: &str| std::env::var(k).is_ok_and(|v| v == "1");
        let extra = Extra {
            cards: if on("RIVET_SHELL_CARDS") { notes[..2].to_vec() } else { Vec::new() },
            notes,
            notifying: true,
            dnd: false,
            night: Some(true),
            awake: Some(false),
            osd: on("RIVET_SHELL_OSD").then_some((OsdKind::Volume, 45, false)),
            user: "Sam Rivers".into(),
        };
        ShotApp { s, kit: Kit::new(false), status: demo_status(), extra, apps, wall, popup, shot: rivet_kit::Shot::new(path) }
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
        let mut m = Model { s: &s, status: &self.status, extra: &self.extra, apps: &self.apps, compositor: Compositor::Niri, output: "", acts: &mut acts };
        let mut at = |r: Rect, f: &mut dyn FnMut(&mut egui::Ui, &mut Model, &mut Kit)| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
            f(&mut child, &mut m, &mut self.kit);
        };
        at(full, &mut |ui, m, k| ui::widgets(ui, m, k));
        let ph = ui::panel_height(&s);
        let panel = if s.panel_edge == Edge::Top { Rect::from_min_size(full.min, Vec2::new(full.width(), ph)) } else { Rect::from_min_size(Pos2::new(full.left(), full.bottom() - ph), Vec2::new(full.width(), ph)) };
        at(panel, &mut |ui, m, k| ui::panel(ui, m, k));
        let items = ui::dock_items(&s, &self.apps, &self.status.running);
        let (dsize, _) = ui::dock_size(&s, &items);
        let bottom = if s.panel_edge == Edge::Bottom { full.bottom() - ph } else { full.bottom() };
        let dock = Rect::from_min_size(Pos2::new(full.center().x - dsize.x / 2.0, bottom - dsize.y), dsize);
        at(dock, &mut |ui, m, k| ui::dock(ui, m, k));
        if self.extra.osd.is_some() {
            let r = Rect::from_center_size(Pos2::new(full.center().x, dock.top() - 12.0 - crate::cc::OSD_H / 2.0), Vec2::new(crate::cc::OSD_W, crate::cc::OSD_H));
            at(r, &mut |ui, m, k| crate::cc::osd(ui, m, k));
        }
        if !self.extra.cards.is_empty() {
            let h = crate::cc::cards_height(&self.extra.cards);
            let top = if s.panel_edge == Edge::Top { full.top() + ph + 2.0 } else { full.bottom() - ph - 2.0 - h };
            let r = Rect::from_min_size(Pos2::new(full.right() - 4.0 - crate::cc::CARDS_W, top), Vec2::new(crate::cc::CARDS_W, h));
            at(r, &mut |ui, m, k| crate::cc::cards(ui, m, k));
        }
        if let Some(pop) = self.popup.as_mut() {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
            ui::popup(&mut child, pop, &mut m, &mut self.kit);
        }
        self.shot.step(&ctx);
    }
}
