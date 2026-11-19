//! `rivet-shell --screenshot FILE`: the whole desktop in one picture (a demo wallpaper, the
//! widgets, the panel, the dock, a popup when asked), drawn by the same code as the real shell,
//! with made-up status, so checks never show the person's wallpaper or machine.
//!
//! Hooks: RIVET_SHELL_POPUP=launcher|calendar|power|control|notifications|dock-menu, RIVET_SHELL_THEME=light|dark,
//! RIVET_SHELL_EDGE=bottom, RIVET_SHELL_ACCENT=rrggbb, RIVET_SHELL_CARDS=1 (notification cards),
//! RIVET_SHELL_OSD=1|caps (the volume or Caps Lock display), RIVET_SHELL_POPUP=sound, RIVET_SHELL_QUERY=text (typed into the launcher),
//! RIVET_SHELL_WS=pills|dots|numbers (the workspaces item's style), RIVET_SHELL_LEVELS=both|icon|text
//! (RIVET_SHELL_POWER=menu|grid|row: the power menu's style)
//! (what volume, brightness and battery show); RIVET_SHELL_POPUP=ai also puts
//! the AI item on the panel (RIVET_SHELL_AI_PINS=claude:5-hour,codex:Weekly pins limits).

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

fn demo_ai() -> crate::ai::Quotas {
    use crate::ai::{Provider, Quotas, State, Window};
    let now = Local::now().timestamp();
    let w = |name: &str, used: f32, hours: i64| Window { name: name.into(), used, reset: if hours > 0 { now + hours * 3600 + 600 } else { 0 } };
    Quotas {
        providers: vec![
            Provider { id: "claude", state: State::Ok, plan: "Max 5x".into(), windows: vec![w("5-hour", 34.0, 3), w("Weekly", 78.0, 50), w("Weekly · Fable", 12.0, 50)], balance: None, at: now - 40 },
            Provider { id: "codex", state: State::Ok, plan: "Plus".into(), windows: vec![w("5-hour", 8.0, 4), w("Weekly", 21.0, 120)], balance: None, at: now - 70 },
            Provider { id: "openrouter", state: State::Ok, plan: String::new(), windows: vec![w("Credits", 58.0, 0)], balance: Some("$4.20 left of $10.00".into()), at: now - 70 },
            Provider { id: "deepseek", state: State::Missing("Set DEEPSEEK_API_KEY.".into()), ..Default::default() },
        ],
        claude_code: true,
        claude_sessions: 2,
        claude_busy: true,
    }
}

fn demo_radios() -> crate::radio::Radios {
    use crate::radio::{BtDevice, Radios, WifiNet};
    let net = |ssid: &str, signal, secure, active, saved| WifiNet { ssid: ssid.into(), signal, secure, active, saved };
    let dev = |name: &str, paired, connected| BtDevice { mac: format!("00:11:22:33:44:{:02X}", name.len()), name: name.into(), paired, connected };
    Radios {
        wifi: vec![net("Home", 82, true, true, true), net("Home 5G", 64, true, false, true), net("Corner Café", 51, false, false, false), net("Neighbours", 40, true, false, false), net("Library Guest", 22, true, false, false)],
        bt: vec![dev("Headphones", true, true), dev("Keyboard", true, false), dev("Speaker in the Kitchen", false, false)],
        wifi_read: true,
        bt_read: true,
        ..Default::default()
    }
}

pub fn demo_status() -> Status {
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

pub fn demo_notes() -> Vec<Note> {
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
        // Checks: a look for the panels and dock (as in the settings file).
        if let Ok(c) = std::env::var("RIVET_SHELL_TRAY") {
            s.tray_color = crate::config::TrayColor::from(&c);
        }
        if let Ok(l) = std::env::var("RIVET_SHELL_LAUNCHER") {
            // "system", or a custom icon's name or path.
            (s.launcher_icon, s.launcher_custom) = if l == "system" { (crate::config::LauncherIcon::System, String::new()) } else { (crate::config::LauncherIcon::Custom, l) };
        }
        if let Some(n) = std::env::var("RIVET_SHELL_SURFACE").ok().and_then(|h| u32::from_str_radix(&h, 16).ok()) {
            let c = Some([(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            (s.surface_dark, s.surface_light) = (c, c);
        }
        s.surface_tint = std::env::var("RIVET_SHELL_TINT").is_ok();
        if let Ok(i) = std::env::var("RIVET_SHELL_ICONS") {
            s.icon_theme = i;
        }
        if let Ok(l) = std::env::var("RIVET_SHELL_LOOK") {
            s.look = crate::style::Style::parse(&l, '=', ';');
        }
        if std::env::var("RIVET_SHELL_POPUP").as_deref() == Ok("ai") {
            let right = &mut s.panels[0].zones[2];
            let at = right.iter().position(|i| i == "network").unwrap_or(0);
            right.insert(at, "ai".into());
            if let Ok(p) = std::env::var("RIVET_SHELL_AI_PINS") {
                s.ai_pins = p.split(',').map(String::from).collect();
            }
        }
        if let Ok(l) = std::env::var("RIVET_SHELL_LEVELS") {
            let show = crate::config::Show::from(&l);
            (s.vol_show, s.bright_show, s.batt_show) = (show, show, show);
        }
        if let Ok(p) = std::env::var("RIVET_SHELL_POWER") {
            s.power_style = crate::config::PowerStyle::from(&p);
        }
        if let Ok(w) = std::env::var("RIVET_SHELL_WS") {
            s.ws_style = crate::config::WsStyle::from(&w);
        }
        if let Ok(e) = std::env::var("RIVET_SHELL_EDGE") {
            s.panels[0].edge = Edge::from(&e);
            if s.panels[0].vertical() {
                s.dock = false;
            }
        }
        // RIVET_SHELL_GROUPS="volume+brightness/bg:primary;network+bluetooth+battery/bg:auto/show:2": groups on the first panel.
        if let Ok(g) = std::env::var("RIVET_SHELL_GROUPS") {
            s.panels[0].groups = g.split(';').map(crate::panels::Group::parse).collect();
            // RIVET_SHELL_FUSE=1: groups side by side join.
            s.panels[0].fuse = std::env::var("RIVET_SHELL_FUSE").is_ok();
        }
        // RIVET_SHELL_LAUNCH=list:large:48:panel (view, size, icon size, place): the launcher's layout.
        if let Ok(l) = std::env::var("RIVET_SHELL_LAUNCH") {
            let mut it = l.split(':');
            s.launch_view = crate::config::LaunchView::from(it.next().unwrap_or(""));
            s.launch_size = crate::config::LaunchSize::from(it.next().unwrap_or(""));
            s.launch_icon = it.next().and_then(|n| n.parse().ok()).unwrap_or(42);
            s.launch_place = crate::config::LaunchPlace::from(it.next().unwrap_or(""));
        }
        // RIVET_SHELL_PLUGIN_ICON=text|highlight: the plugin item's icon in one colour.
        if let Ok(c) = std::env::var("RIVET_SHELL_PLUGIN_ICON") {
            s.set_plugin("disk", crate::plugins::ICON_COLOR, &c);
        }
        if std::env::var("RIVET_SHELL_PLUGIN").as_deref() == Ok("1") {
            // Before the power button (the start of the zone is what a narrow screen leaves out).
            let at = s.panels[0].zones[2].len().saturating_sub(1);
            s.panels[0].zones[2].insert(at, "plugin:disk".into());
        }
        // RIVET_SHELL_FONTS=base:panel:launcher sets the three fonts (a part left empty follows).
        if let Ok(f) = std::env::var("RIVET_SHELL_FONTS") {
            let mut it = f.split(':').map(str::to_string);
            (s.font, s.launcher_font) = (it.next().unwrap_or_default(), { let p = it.next().unwrap_or_default(); if let Some(first) = s.panels.first_mut() { first.font = p; } it.next().unwrap_or_default() });
        }
        // RIVET_SHELL_SIZE=48:10: the first panel's thickness, and the room between its items.
        if let (Some((a, b)), Some(p)) = (std::env::var("RIVET_SHELL_SIZE").ok().and_then(|v| v.split_once(':').map(|(a, b)| (a.parse().ok(), b.parse().ok()))), s.panels.first_mut()) {
            (p.size, p.spacing) = (a, b);
        }
        // RIVET_SHELL_QUICK=toggles: the quick settings item's symbol.
        if let Ok(q) = std::env::var("RIVET_SHELL_QUICK") {
            s.quick_icon = crate::config::QuickIcon::from(&q);
        }
        // RIVET_SHELL_COLORS=2: how much the secondary and tertiary colours are used.
        if let Some(n) = std::env::var("RIVET_SHELL_COLORS").ok().and_then(|n| n.parse().ok()) {
            s.color_use = n;
        }
        // RIVET_SHELL_SPACER=40: a spacer that size after the first panel's first item.
        if let (Some(n), Some(p)) = (std::env::var("RIVET_SHELL_SPACER").ok().and_then(|n| n.parse::<u32>().ok()), s.panels.first_mut()) {
            p.zones[0].insert(1.min(p.zones[0].len()), format!("spacer:{n}"));
        }
        // RIVET_SHELL_SPACE=20:0: the first panel's offset from its edge, and its room before windows.
        if let (Some((o, g)), Some(p)) = (std::env::var("RIVET_SHELL_SPACE").ok().and_then(|v| v.split_once(':').map(|(o, g)| (o.parse().ok(), g.parse().ok()))), s.panels.first_mut()) {
            (p.offset, p.gap) = (o, g);
        }
        // RIVET_SHELL_LEN=60 (or 60:start): a shorter panel.
        if let Ok(l) = std::env::var("RIVET_SHELL_LEN") {
            let (n, along) = l.split_once(':').unwrap_or((&l, "center"));
            s.panels[0].length = n.parse().unwrap_or(100);
            s.panels[0].along = crate::config::Along::from(along);
        }
        if std::env::var("RIVET_SHELL_PANELS").as_deref() == Ok("2") {
            // A second panel along the bottom, edge to edge, with other items in its zones.
            let list = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
            s.panels.push(crate::panels::PanelCfg { edge: Edge::Bottom, floating: false, zones: [list(&["workspaces", "window-title"]), list(&["media"]), list(&["system", "clock"])], ..crate::panels::PanelCfg::bare() });
            s.dock = false;
        }
        if std::env::var("RIVET_SHELL_PANEL_DOCK").as_deref() == Ok("1") {
            // The dock as a panel item, in the left zone, and no dock of its own.
            s.panels[0].zones[0].push("dock".into());
            s.dock = false;
        }
        let dir = std::env::temp_dir().join(format!("rivet-shell-shot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wall = crate::wall::demo_png(&dir).and_then(|p| crate::wall::thumbnail(&p, 1100)).map(|img| cc.egui_ctx.load_texture("demo-wallpaper", img, egui::TextureOptions::LINEAR));
        let _ = std::fs::remove_dir_all(&dir);
        let query = std::env::var("RIVET_SHELL_QUERY").unwrap_or_default();
        // Checks: popups joined to the panel (with RIVET_SHELL_LOOK's `popups=attached`).
        let attached = s.look.attach == Some(true);
        let popup = match std::env::var("RIVET_SHELL_POPUP").as_deref() {
            Ok("launcher") => Some(Popup { kind: PopupKind::Launcher { query, sel: 0 }, anchor_x: 0.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("calendar") => Some(Popup { kind: PopupKind::Calendar { month: 0 }, anchor_x: 550.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("power") => Some(Popup { kind: PopupKind::Power { confirm: None }, anchor_x: 1080.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("control") => Some(Popup { kind: PopupKind::Control { drag: None }, anchor_x: 1050.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("network") => Some(Popup { kind: PopupKind::Network { ask: std::env::var("RIVET_SHELL_ASK").ok().map(|a| (a, "secret".into())) }, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("bluetooth") => Some(Popup { kind: PopupKind::Bluetooth, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("notifications") => Some(Popup { kind: PopupKind::Notifications, anchor_x: 1010.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("ai") => Some(Popup { kind: PopupKind::Ai, anchor_x: 900.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("sound") => Some(Popup { kind: PopupKind::Sound { drag: None }, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("plugin") => Some(Popup { kind: PopupKind::Plugin { id: "disk".into() }, anchor_x: 640.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("tray") => Some(Popup { kind: PopupKind::TrayMenu { key: ":1.chat/StatusNotifierItem".into(), path: Vec::new() }, anchor_x: 760.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("panel-dock-menu") => Some(Popup { kind: PopupKind::DockMenu { app: "rivet-files".into(), pinned: true }, anchor_x: 330.0, output: String::new(), focus: false, from: Some((Edge::Top, if attached { 40.0 } else { 44.0 })), attach: attached.then_some(0), panel: Some(0), join: None }),
            Ok("dock-menu") => Some(Popup { kind: PopupKind::DockMenu { app: "rivet-terminal".into(), pinned: true }, anchor_x: -50.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            _ => None,
        };
        // Joined popups open from the first panel (a floating one along the top).
        let popup = popup.map(|mut p| {
            if attached && !matches!(p.kind, PopupKind::Launcher { .. }) && !(matches!(p.kind, PopupKind::DockMenu { .. }) && p.from.is_none()) {
                (p.from, p.attach) = (Some((Edge::Top, crate::panels::reach(&s.panels, 0, "") - s.panels[0].gap())), Some(0));
            }
            p
        });
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
            osd: match std::env::var("RIVET_SHELL_OSD").as_deref() {
                Ok("1") => Some((OsdKind::Volume, 45, false)),
                Ok("caps") => Some((OsdKind::CapsLock, 1, false)),
                _ => None,
            },
            user: "Sam Rivers".into(),
            radios: demo_radios(),
            ai: demo_ai(),
            audio: crate::audio::demo(),
            join: None,
            // RIVET_SHELL_PLUGIN=1: a plugin's item on the first panel (RIVET_SHELL_POPUP=plugin: its menu).
            // RIVET_SHELL_PLUGIN_SAYS=file: the "disk" item is what that file's lines say (a real
            // plugin's output, to see how the shell shows it).
            plugins: if let Some(text) = std::env::var("RIVET_SHELL_PLUGIN_SAYS").ok().and_then(|f| std::fs::read_to_string(f).ok()) {
                let mut it = crate::plugins::Item { name: std::env::var("RIVET_SHELL_PLUGIN_NAME").unwrap_or_else(|_| "Plugin".into()), ..Default::default() };
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    crate::plugins::take(&mut it, line, false);
                }
                vec![("disk".to_string(), it)]
            } else if on("RIVET_SHELL_PLUGIN") {
                let row = |id: &str, label: &str, sub: &str| crate::plugins::MenuRow { id: id.into(), label: label.into(), sub: sub.into(), ..Default::default() };
                vec![("disk".to_string(), crate::plugins::Item { name: "Disk Space".into(), text: "212 GB".into(), icon: "drive-harddisk".into(), tooltip: "212 GB free on /".into(), menu: vec![row("/", "/", "212 GB free of 476 GB"), row("/home", "/home", "640 GB free of 1.8 TB"), row("/boot", "/boot", "410 MB free of 1 GB")], ..Default::default() })]
            } else {
                Vec::new()
            },
            tray: {
                let mut t = crate::tray::demo();
                if popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::TrayMenu { .. })) {
                    t.menu = Some((":1.chat/StatusNotifierItem".into(), Some(crate::tray::demo_menu())));
                }
                t
            },
        };
        let mut status = demo_status();
        if popup.as_ref().is_some_and(|p| p.kind == PopupKind::Bluetooth) {
            status.bluetooth = Some(true);
        }
        ShotApp { s, kit: Kit::new(false), status, extra, apps, wall, popup, shot: rivet_kit::Shot::new(path) }
    }
}

impl eframe::App for ShotApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // One surface here, so one font: the launcher's when that's what's open, else the first panel's.
        self.kit.face = self.popup.as_ref().map(|p| p.face()).filter(|f| *f == crate::ui::Face::Launcher).unwrap_or(crate::ui::Face::Panel(0));
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
        self.extra.join = self.popup.as_ref().and_then(|p| Some((p.attach?, p.join?.0, p.join?.1)));
        let mut acts: Vec<Act> = Vec::new();
        let s = self.s.clone();
        let mut m = Model { s: &s, status: &self.status, extra: &self.extra, apps: &self.apps, compositor: Compositor::Niri, output: "", acts: &mut acts };
        let mut at = |r: Rect, f: &mut dyn FnMut(&mut egui::Ui, &mut Model, &mut Kit)| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
            f(&mut child, &mut m, &mut self.kit);
        };
        at(full, &mut |ui, m, k| ui::widgets(ui, m, k));
        // Each panel against its edge, after the ones before it on that edge.
        for (idx, p) in s.panels.iter().enumerate() {
            let reach = crate::panels::reach(&s.panels, idx, "");
            let h = p.height();
            // Side panels run between the top and bottom ones.
            let (t, bt) = (full.top() + crate::panels::room(&s.panels, "", Edge::Top), full.bottom() - crate::panels::room(&s.panels, "", Edge::Bottom));
            let r = match p.edge {
                Edge::Top => Rect::from_min_size(Pos2::new(full.left(), full.top() + reach - h), Vec2::new(full.width(), h)),
                Edge::Bottom => Rect::from_min_size(Pos2::new(full.left(), full.bottom() - reach), Vec2::new(full.width(), h)),
                Edge::Left => Rect::from_min_max(Pos2::new(full.left() + reach - h, t), Pos2::new(full.left() + reach, bt)),
                Edge::Right => Rect::from_min_max(Pos2::new(full.right() - reach, t), Pos2::new(full.right() - reach + h, bt)),
            };
            at(r, &mut |ui, m, k| crate::bar::panel(ui, m, k, idx));
        }
        let top_room = crate::panels::room(&s.panels, "", Edge::Top);
        let bottom = full.bottom() - crate::panels::room(&s.panels, "", Edge::Bottom);
        let items = ui::dock_items(&s, &self.apps, &self.status.running);
        let (dsize, _) = ui::dock_size(&s, &items);
        let dock = Rect::from_min_size(Pos2::new(full.center().x - dsize.x / 2.0, bottom - dsize.y), dsize);
        if s.dock {
            at(dock, &mut |ui, m, k| ui::dock(ui, m, k));
        }
        if self.extra.osd.is_some() {
            let r = Rect::from_center_size(Pos2::new(full.center().x, dock.top() - 12.0 - crate::cc::OSD_H / 2.0), Vec2::new(crate::cc::OSD_W, crate::cc::OSD_H));
            at(r, &mut |ui, m, k| crate::cc::osd(ui, m, k));
        }
        if !self.extra.cards.is_empty() {
            let h = crate::cc::cards_height(&self.extra.cards);
            let top = if s.panels.first().is_none_or(|p| p.edge == Edge::Top) { full.top() + top_room + 2.0 } else { bottom - 2.0 - h };
            let r = Rect::from_min_size(Pos2::new(full.right() - 4.0 - crate::cc::CARDS_W, top), Vec2::new(crate::cc::CARDS_W, h));
            at(r, &mut |ui, m, k| crate::cc::cards(ui, m, k));
        }
        if let Some(pop) = self.popup.as_mut() {
            // From a side panel: beside it, a third of the way down.
            if let Some(p) = s.panels.first().filter(|p| p.vertical()) && !matches!(pop.kind, PopupKind::Launcher { .. } | PopupKind::DockMenu { .. }) {
                pop.from = Some((p.edge, crate::panels::reach(&s.panels, 0, "") + 4.0));
                pop.anchor_x = full.height() / 3.0;
            }
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(full));
            ui::popup(&mut child, pop, &mut m, &mut self.kit);
        }
        self.shot.step(&ctx);
    }
}
