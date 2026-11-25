//! `rivet-shell --screenshot FILE`: the whole desktop in one picture (a demo wallpaper, the
//! widgets, the panel, the dock, a popup when asked), drawn by the same code as the real shell,
//! with made-up status, so checks never show the person's wallpaper or machine.
//!
//! Hooks: RIVET_SHELL_POPUP=launcher|calendar|power|control|notifications|dock-menu|downloads|trash|auth (the password dialog), RIVET_SHELL_THEME=light|dark,
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

use crate::config::{Board, Edge, Placed, Settings, WidgetKind};
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
    /// The wallpaper's picture, until the first frame makes its texture (a large one needs the
    /// frame's own size limit, not the one before any frame).
    wall_img: Option<egui::ColorImage>,
    popup: Option<Popup>,
    shot: rivet_kit::Shot,
}

fn demo_ai() -> crate::ai::Quotas {
    use crate::ai::{Provider, Quotas, State, Window};
    let now = Local::now().timestamp();
    let w = |name: &str, used: f32, hours: i64| Window { name: name.into(), used, reset: if hours > 0 { now + hours * 3600 + 600 } else { 0 } };
    Quotas {
        providers: vec![
            Provider { id: "claude", state: State::Ok, plan: "Max 5x".into(), windows: vec![w("5-hour", 34.0, 3), w("Weekly", 78.0, 50), w("Weekly · Fable", 12.0, 50)], balance: None, at: now - 40, ..Default::default() },
            Provider { id: "codex", state: State::Ok, plan: "Plus".into(), windows: vec![w("5-hour", 8.0, 4), w("Weekly", 21.0, 120)], balance: None, at: now - 70, ..Default::default() },
            Provider { id: "openrouter", state: State::Ok, plan: String::new(), windows: vec![w("Credits", 58.0, 0)], balance: Some("$4.20 left of $10.00".into()), at: now - 70, ..Default::default() },
            Provider {
                id: "hermes",
                state: State::Ok,
                plan: "Last 7 days".into(),
                facts: [("Sessions", "83"), ("Messages", "447"), ("Tool calls", "306"), ("Tokens", "6,238,657"), ("Active", "~15h 36m"), ("Cost", "~$3.55")].iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
                at: now - 300,
                ..Default::default()
            },
            Provider { id: "deepseek", state: State::Missing("Set DEEPSEEK_API_KEY.".into()), ..Default::default() },
        ],
        claude_code: true,
        claude_sessions: 2,
        claude_busy: true,
    }
}

pub fn demo_radios() -> crate::radio::Radios {
    use crate::radio::{BtDevice, Radios, WifiNet};
    let net = |ssid: &str, signal, secure, active, saved| WifiNet { ssid: ssid.into(), signal, secure, active, saved };
    let dev = |name: &str, paired, connected| BtDevice { mac: format!("00:11:22:33:44:{:02X}", name.len()), name: name.into(), paired, connected };
    Radios {
        wifi: vec![net("Home", 82, true, true, true), net("Home 5G", 64, true, false, true), net("Corner Café", 51, false, false, false), net("Neighbours", 40, true, false, false), net("Library Guest", 22, true, false, false)],
        wired: vec![crate::radio::Wired { device: "enp3s0".into(), name: "Wired connection 1".into(), link: crate::radio::Link::Connected, address: "192.168.1.24".into(), speed: 1000 }],
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
        media: Some(Media { player: "Music".into(), title: "Low Tide".into(), artist: "The Harbor Lights".into(), playing: true, album: "Shorelines".into(), art: std::env::var("RIVET_SHELL_COVER").unwrap_or_default() }),
        media_pos: (87, 241),
        net: Net::Wifi("Home".into()),
        wifi_on: Some(true),
        bluetooth: Some(false),
        vpns: crate::vpn::demo().list,
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
        own_time: false,
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
        // Only Rivet's own apps in the dock: pictures of the shell show no other project's icon.
        s.dock_apps.retain(|a| a.starts_with("rivet-"));
        match std::env::var("RIVET_SHELL_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors, s.color_mode) = (Mode::Light, false, Mode::Light),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
            _ => {}
        }
        if let Some(n) = std::env::var("RIVET_SHELL_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
            s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
            s.follow_desktop_colors = false;
        }
        // Checks: the on-screen display's place, size and bar colour (RIVET_SHELL_OSD_STYLE=top,large,tertiary).
        if let Ok(o) = std::env::var("RIVET_SHELL_OSD_STYLE") {
            let mut parts = o.split(',');
            s.osd_place = crate::config::OsdPlace::from(parts.next().unwrap_or_default());
            s.osd_size = crate::config::LaunchSize::from(parts.next().unwrap_or_default());
            s.osd_color = parts.next().and_then(crate::style::Role::from);
        }
        // Checks: the notification cards' corner and width (RIVET_SHELL_NOTE_STYLE=bottom-left,large)
        // and their own look (RIVET_SHELL_NOTE_LOOK=radius=4;opacity=0.8, as in the settings file).
        if let Ok(o) = std::env::var("RIVET_SHELL_NOTE_STYLE") {
            let mut parts = o.split(',');
            s.note_place = crate::config::NotePlace::from(parts.next().unwrap_or_default());
            s.note_width = crate::config::LaunchSize::from(parts.next().unwrap_or_default());
        }
        if let Ok(l) = std::env::var("RIVET_SHELL_NOTE_LOOK") {
            s.note_look = crate::style::Style::parse(&l, '=', ';');
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
        // RIVET_SHELL_WEATHER=1: the weather widget and the panel's Weather item, with a sample
        // reading (none: no place chosen; failed: the service can't be reached; night: a clear
        // night). RIVET_SHELL_POPUP=weather opens the item's popup. RIVET_SHELL_UNITS=imperial.
        if std::env::var("RIVET_SHELL_WEATHER").is_ok() || std::env::var("RIVET_SHELL_POPUP").as_deref() == Ok("weather") {
            if !s.widgets.iter().any(|w| w.board == Board::Desktop && w.kind == WidgetKind::Weather) {
                s.widgets.push(Placed::new(WidgetKind::Weather, Board::Desktop));
            }
            (s.weather_place, s.weather_at) = ("Portland, Oregon, United States".into(), Some((45.52, -122.68)));
            s.weather_units = std::env::var("RIVET_SHELL_UNITS").ok().and_then(|u| crate::weather::Units::parse(&u)).unwrap_or(crate::weather::Units::Metric);
            let right = &mut s.panels[0].zones[2];
            let at = right.iter().position(|i| i == "network").unwrap_or(0);
            right.insert(at, "weather".into());
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
        // RIVET_SHELL_LAUNCH=list:large:48:panel:6:4 (view, size, icon size, place, rows, columns):
        // the launcher's layout.
        if let Ok(l) = std::env::var("RIVET_SHELL_LAUNCH") {
            let mut it = l.split(':');
            s.launch_view = crate::config::LaunchView::from(it.next().unwrap_or(""));
            s.launch_size = crate::config::LaunchSize::from(it.next().unwrap_or(""));
            s.launch_icon = it.next().and_then(|n| n.parse().ok()).unwrap_or(42);
            s.launch_place = crate::config::LaunchPlace::from(it.next().unwrap_or(""));
            s.launch_rows = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            s.launch_cols = it.next().and_then(|n| n.parse().ok()).unwrap_or(0);
        }
        // RIVET_SHELL_MEDIA=viz,controls,artist-title,nocover: the Now Playing item's options
        // (on the first panel, added when it isn't there). RIVET_SHELL_COVER=file is its cover.
        if let Ok(m) = std::env::var("RIVET_SHELL_MEDIA") {
            let has = |w: &str| m.split(',').any(|x| x == w);
            (s.media_viz, s.media_controls, s.media_art) = (has("viz"), has("controls"), !has("nocover"));
            if let Some(t) = crate::config::MediaText::ALL.into_iter().find(|t| has(t.name())) {
                s.media_text = t;
            }
            if !s.panels[0].has("media") {
                s.panels[0].zones[1].insert(0, "media".into());
            }
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
        // RIVET_SHELL_WIDGETS=0.5,0.5:1,1: the clock's and the system widget's dragged places (as
        // fractions of the room they move in; a part left empty stays in its usual place).
        if let Ok(w) = std::env::var("RIVET_SHELL_WIDGETS") {
            let mut it = w.split(':');
            let pair = |p: Option<&str>| p.and_then(|p| p.split_once(',')).and_then(|(x, y)| Some((x.parse().ok()?, y.parse().ok()?)));
            for kind in [WidgetKind::Clock, WidgetKind::System] {
                let at = pair(it.next());
                if let Some(w) = s.widgets.iter_mut().find(|w| w.board == Board::Desktop && w.kind == kind) {
                    w.at = at;
                }
            }
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
        // RIVET_SHELL_ROLES=3584e4:e5a50a: a style and accent of the shell's own, with this
        // secondary and tertiary (a part left empty is made from the accent).
        if let Ok(r) = std::env::var("RIVET_SHELL_ROLES") {
            let (a, b) = r.split_once(':').unwrap_or((&r, ""));
            let hex = |h: &str| u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6).map(|n| [(n >> 16) as u8, (n >> 8) as u8, n as u8]);
            (s.follow_desktop_colors, s.secondary, s.tertiary) = (false, hex(a), hex(b));
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
        // RIVET_SHELL_SUB=open|stay|rolled: a sub panel under the first panel, with its button on
        // it: rolled out for a moment, left open (keeping its room), or rolled up.
        if let Ok(how) = std::env::var("RIVET_SHELL_SUB")
            && let Some(i) = crate::panels::add_sub(&mut s.panels, 0)
        {
            let list = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
            s.panels[i].zones = [list(&["dock"]), Vec::new(), list(&["system", "ai"])];
            if let Some(sub) = s.panels[i].sub.as_mut() {
                (sub.stay, sub.open) = (how == "stay", how == "stay");
            }
            crate::panels::rename_sub(&mut s.panels, i, "Shortcuts");
            // RIVET_SHELL_SUB_PLAIN: not a drawer, and its own length.
            if let Some(sub) = s.panels[i].sub.as_mut().filter(|_| std::env::var_os("RIVET_SHELL_SUB_PLAIN").is_some()) {
                (sub.fit, sub.drawer) = (false, false);
            }
            // RIVET_SHELL_SUB_HANDLE=true,120,4,start,tertiary: its handle (as in the settings file).
            if let (Ok(h), Some(sub)) = (std::env::var("RIVET_SHELL_SUB_HANDLE"), s.panels[i].sub.as_mut()) {
                *sub = crate::panels::PanelCfg::parse(&format!("sub=x;handle={h}")).sub.map(|x| crate::panels::Sub { handle: x.handle, ..sub.clone() }).unwrap_or(sub.clone());
            }
            s.tidy_panels();
        }
        if std::env::var_os("RIVET_SHELL_TRAY_PIXMAPS").is_some() && !s.panels[0].zones.iter().flatten().any(|i| i == "tray") {
            // Real apps' tray icons: the tray at the start of the right zone, to see them.
            s.panels[0].zones[2].insert(0, "tray".into());
        }
        // RIVET_SHELL_FAVS=1: the launcher's favourites row, with a few apps in it (=empty: none).
        match std::env::var("RIVET_SHELL_FAVS").as_deref() {
            Ok("1") => {
                s.launch_favorites = true;
                s.launch_favorite_apps = vec!["firefox".into(), "rivet-files".into(), "rivet-terminal".into(), "rivet-notes".into()];
            }
            Ok("empty") => s.launch_favorites = true,
            _ => {}
        }
        if std::env::var("RIVET_SHELL_PANEL_DOCK").as_deref() == Ok("1") {
            // The dock as a panel item, in the left zone, and no dock of its own.
            s.panels[0].zones[0].push("dock".into());
            s.dock = false;
        }
        let dir = std::env::temp_dir().join(format!("rivet-shell-shot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // RIVET_SHELL_WALL=file: that picture as the wallpaper (documentation pictures).
        let own = std::env::var("RIVET_SHELL_WALL").ok().map(std::path::PathBuf::from).and_then(|p| crate::wall::thumbnail(&p, 3840));
        let wall_img = own.or_else(|| crate::wall::demo_png(&dir).and_then(|p| crate::wall::thumbnail(&p, 1100)));
        let _ = cc;
        let _ = std::fs::remove_dir_all(&dir);
        let query = std::env::var("RIVET_SHELL_QUERY").unwrap_or_default();
        // Checks: popups joined to the panel (with RIVET_SHELL_LOOK's `popups=attached`).
        let attached = s.look.attach == Some(true);
        let popup = match std::env::var("RIVET_SHELL_POPUP").as_deref() {
            Ok("launcher") => Some(Popup { kind: PopupKind::Launcher { query, sel: 0 }, anchor_x: 0.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("calendar") => Some(Popup { kind: PopupKind::Calendar { month: 0, day: None }, anchor_x: 550.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("power") => Some(Popup { kind: PopupKind::Power { confirm: None }, anchor_x: 1080.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("control") => Some(Popup { kind: PopupKind::Control { drag: None }, anchor_x: 1050.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("network") => Some(Popup { kind: PopupKind::Network { ask: std::env::var("RIVET_SHELL_ASK").ok().map(|a| (a, "secret".into())) }, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("bluetooth") => Some(Popup { kind: PopupKind::Bluetooth, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("notifications") => Some(Popup { kind: PopupKind::Notifications, anchor_x: 1010.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("ai") => Some(Popup { kind: PopupKind::Ai, anchor_x: 900.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("weather") => Some(Popup { kind: PopupKind::Weather, anchor_x: 900.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("sound") => Some(Popup { kind: PopupKind::Sound { drag: None }, anchor_x: 1000.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("media") => Some(Popup { kind: PopupKind::Media, anchor_x: 760.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("auth") => Some(Popup { kind: PopupKind::Auth { typed: "secret".into(), round: 1 }, anchor_x: 0.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("plugin") => Some(Popup { kind: PopupKind::Plugin { id: "disk".into() }, anchor_x: 640.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("tray") => Some(Popup { kind: PopupKind::TrayMenu { key: ":1.chat/StatusNotifierItem".into(), path: Vec::new() }, anchor_x: 760.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("panel-dock-menu") => Some(Popup { kind: PopupKind::DockMenu { app: "rivet-files".into(), pinned: true }, anchor_x: 330.0, output: String::new(), focus: false, from: Some((Edge::Top, if attached { 40.0 } else { 44.0 })), attach: attached.then_some(0), panel: Some(0), join: None }),
            Ok("downloads") => Some(Popup { kind: PopupKind::Downloads, anchor_x: 200.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            Ok("trash") => Some(Popup { kind: PopupKind::TrashMenu { confirm: std::env::var("RIVET_SHELL_CONFIRM").is_ok() }, anchor_x: 240.0, output: String::new(), focus: false, from: None, attach: None, panel: None, join: None }),
            // RIVET_SHELL_POPUP=hud: the HUD; hud-edit: being arranged; widgets-edit: the
            // desktop's widgets being arranged (RIVET_SHELL_QUERY: typed on the HUD).
            Ok("hud") => Some(Popup::at(PopupKind::Hud { edit: false, query: std::env::var("RIVET_SHELL_QUERY").unwrap_or_default() }, 0.0, "")),
            Ok("hud-edit") => Some(Popup::at(PopupKind::Hud { edit: true, query: String::new() }, 0.0, "")),
            Ok("widgets-edit") => Some(Popup::at(PopupKind::WidgetEdit, 0.0, "")),
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
            weather: match std::env::var("RIVET_SHELL_WEATHER").as_deref() {
                Ok("none") => Default::default(),
                Ok("failed") => crate::weather::Weather { state: crate::weather::State::Failed("Couldn't reach the weather service.".into()), place: "Portland, Oregon, United States".into(), ..Default::default() },
                Ok("night") => {
                    let mut w = crate::weather::sample();
                    if let Some(n) = &mut w.now {
                        (n.day, n.code, n.temp) = (false, 0, -3.4);
                    }
                    w
                }
                _ => crate::weather::sample(),
            },
            audio: crate::audio::demo(),
            join: None,
            auth: Some(crate::polkit::demo()),
            viz: Some(crate::viz::demo()),
            drop: None,
            // RIVET_SHELL_PLUGIN=1: a plugin's item on the first panel (RIVET_SHELL_POPUP=plugin: its menu).
            // RIVET_SHELL_PLUGIN_SAYS=file: the "disk" item is what that file's lines say (a real
            // plugin's output, to see how the shell shows it).
            plugins: if let Some(text) = std::env::var("RIVET_SHELL_PLUGIN_SAYS").ok().and_then(|f| std::fs::read_to_string(f).ok()) {
                let mut it = crate::plugins::Item { name: std::env::var("RIVET_SHELL_PLUGIN_NAME").unwrap_or_else(|_| "Plugin".into()), keyword: std::env::var("RIVET_SHELL_PLUGIN_KEYWORD").unwrap_or_default(), ..Default::default() };
                for line in text.lines().filter(|l| !l.trim().is_empty()) {
                    crate::plugins::take(&mut it, line, false);
                }
                vec![("disk".to_string(), it)]
            } else if std::env::var("RIVET_SHELL_PLUGIN").as_deref() == Ok("calendar") {
                // RIVET_SHELL_PLUGIN=calendar: a calendar plugin's events around today (with
                // RIVET_SHELL_POPUP=calendar, the days marked and today's listed).
                let today = Local::now().date_naive();
                let ev = |d: i64, title: &str, time: &str, color: [u8; 3]| crate::plugins::DayEvent { id: title.into(), day: Some(today + chrono::Duration::days(d)), title: title.into(), time: time.into(), color: Some(color) };
                let (work, home, fam) = ([0x72, 0x87, 0xfd], [0xe0, 0x57, 0x3f], [0x40, 0xa0, 0x2b]);
                let events = vec![ev(0, "Standup", "09:30", work), ev(0, "Lunch with Sam", "12:00", home), ev(0, "Design review", "13:00", work), ev(1, "Gym", "18:00", home), ev(2, "Release planning", "10:00", work), ev(3, "Bins out", "", fam), ev(5, "Dentist", "08:30", home), ev(8, "Trip to the coast", "", fam), ev(9, "Trip to the coast", "", fam), ev(-3, "Mum's birthday", "", fam), ev(-1, "Standup", "09:30", work)];
                vec![("calendar".to_string(), crate::plugins::Item { name: "Calendar".into(), hidden: true, calendar: true, events, ..Default::default() })]
            } else if on("RIVET_SHELL_PLUGIN") {
                let row = |id: &str, label: &str, sub: &str| crate::plugins::MenuRow { id: id.into(), label: label.into(), sub: sub.into(), ..Default::default() };
                vec![("disk".to_string(), crate::plugins::Item { name: "Disk Space".into(), text: "212 GB".into(), icon: "drive-harddisk".into(), tooltip: "212 GB free on /".into(), menu: vec![row("/", "/", "212 GB free of 476 GB"), row("/home", "/home", "640 GB free of 1.8 TB"), row("/boot", "/boot", "410 MB free of 1 GB")], ..Default::default() })]
            } else {
                Vec::new()
            },
            tray: {
                let mut t = crate::tray::demo();
                // RIVET_SHELL_TRAY_PIXMAPS=a.json,b.json: real apps' icons instead, as saved by
                // `busctl --user --json=short get-property … IconPixmap`.
                if let Ok(list) = std::env::var("RIVET_SHELL_TRAY_PIXMAPS") {
                    t.items = list.split(',').filter_map(|f| crate::tray::demo_from_busctl(std::path::Path::new(f))).collect();
                }
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
        ShotApp { s, kit: Kit::new(false), status, extra, apps, wall: None, wall_img, popup, shot: rivet_kit::Shot::new(path) }
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
        if let Some(img) = self.wall_img.take() {
            self.wall = Some(ctx.load_texture("demo-wallpaper", img, egui::TextureOptions::LINEAR));
        }
        match &self.wall {
            Some(t) => {
                // Covering the screen: the picture's middle, in the screen's shape.
                let [tw, th] = t.size().map(|v| v as f32);
                let (sx, sy) = if tw / th > full.width() / full.height() { (full.width() / full.height() * th / tw, 1.0) } else { (1.0, full.height() / full.width() * tw / th) };
                let uv = Rect::from_center_size(Pos2::new(0.5, 0.5), Vec2::new(sx, sy));
                p.image(t.id(), full, uv, Color32::WHITE);
            }
            None => {
                p.rect_filled(full, 0.0, ui.visuals().extreme_bg_color);
            }
        }
        self.extra.join = self.popup.as_ref().and_then(|p| Some((p.attach?, p.join?.0, p.join?.1)));
        // RIVET_SHELL_DROP=image/png: files of that type dragged over the dock, on its third slot.
        if let Ok(mime) = std::env::var("RIVET_SHELL_DROP") {
            let bottom = full.bottom() - crate::panels::room(&self.s.panels, "", Edge::Bottom);
            let (dsize, _) = ui::dock_size(&self.s, &ui::dock_items(&self.s, &self.apps, &self.status.running));
            let dock = Rect::from_min_size(Pos2::new(full.center().x - dsize.x / 2.0, bottom - dsize.y), dsize);
            let (_, slots, _) = ui::dock_layout(&self.s, &self.apps, &self.status.running, dock);
            let pos = slots.get(2).map(|(_, r)| r.center()).unwrap_or_default();
            self.extra.drop = Some(ui::DockDrop { pos, mimes: Some(vec![mime]) });
        }
        let mut acts: Vec<Act> = Vec::new();
        let s = self.s.clone();
        let mut m = Model { s: &s, status: &self.status, extra: &self.extra, apps: &self.apps, compositor: Compositor::Niri, output: "", acts: &mut acts };
        let mut at = |r: Rect, f: &mut dyn FnMut(&mut egui::Ui, &mut Model, &mut Kit)| {
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r));
            f(&mut child, &mut m, &mut self.kit);
        };
        at(full, &mut |ui, m, k| crate::board::desktop(ui, m, k));
        // Each panel against its edge, after the ones before it on that edge.
        let rolled_out = std::env::var("RIVET_SHELL_SUB").as_deref() == Ok("open");
        // RIVET_SHELL_SUB_SLIDE=0.4: a sub panel that's rolled out, caught part of the way (0 out,
        // 1 rolled up), as it's drawn while it slides.
        let sub_slide = std::env::var("RIVET_SHELL_SUB_SLIDE").ok().and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0).clamp(0.0, 1.0);
        for (idx, p) in s.panels.iter().enumerate() {
            // A sub panel shows when it's left open, or asked for; rolled up, only its handle.
            let rolled = p.sub.is_some() && !p.kept() && !rolled_out;
            let h = p.height();
            // (A sub panel's surface begins where its host's bar ends, for a drawer.)
            let reach = if p.sub.is_some() { crate::panels::sub_start(&s.panels, idx, "") + h } else { crate::panels::reach(&s.panels, idx, "") };
            // Side panels run between the top and bottom ones.
            let (t, bt) = (full.top() + crate::panels::room(&s.panels, "", Edge::Top), full.bottom() - crate::panels::room(&s.panels, "", Edge::Bottom));
            let r = match p.edge {
                Edge::Top => Rect::from_min_size(Pos2::new(full.left(), full.top() + reach - h), Vec2::new(full.width(), h)),
                Edge::Bottom => Rect::from_min_size(Pos2::new(full.left(), full.bottom() - reach), Vec2::new(full.width(), h)),
                Edge::Left => Rect::from_min_max(Pos2::new(full.left() + reach - h, t), Pos2::new(full.left() + reach, bt)),
                Edge::Right => Rect::from_min_max(Pos2::new(full.right() - reach, t), Pos2::new(full.right() - reach + h, bt)),
            };
            if rolled || (p.sub.is_some() && sub_slide >= 1.0) {
                at(r, &mut |ui, m, k| ui::sub_handle(ui, m.s, k, idx, ""));
            } else if p.sub.is_some() && sub_slide > 0.0 {
                // Slid toward its host, and cut off where its host's bar ends (as the shell does).
                let mut clip = r;
                match p.edge {
                    Edge::Top => clip.min.y += p.lead,
                    Edge::Bottom => clip.max.y -= p.lead,
                    Edge::Left => clip.min.x += p.lead,
                    Edge::Right => clip.max.x -= p.lead,
                }
                at(r, &mut |ui, m, k| {
                    let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.translate(p.slide(sub_slide))));
                    c.set_clip_rect(clip);
                    crate::bar::panel(&mut c, m, k, idx);
                });
            } else {
                at(r, &mut |ui, m, k| crate::bar::panel(ui, m, k, idx));
            }
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
            let (w, h) = crate::cc::osd_size(&s);
            // About where the compositor puts it: clear of a top panel, the middle, or above the dock.
            let y = match s.osd_place {
                crate::config::OsdPlace::Top => full.top() + 60.0 + h / 2.0,
                crate::config::OsdPlace::Middle => full.center().y,
                crate::config::OsdPlace::Bottom => dock.top() - 12.0 - h / 2.0,
            };
            let r = Rect::from_center_size(Pos2::new(full.center().x, y), Vec2::new(w, h));
            at(r, &mut |ui, m, k| crate::cc::osd(ui, m, k));
        }
        if !self.extra.cards.is_empty() {
            let h = crate::cc::cards_height(&self.extra.cards);
            use crate::config::NotePlace;
            let w = crate::cc::cards_width(&s);
            let (at_top, at_left) = match s.note_place {
                NotePlace::ByPanel => (s.panels.first().is_none_or(|p| p.edge == Edge::Top), false),
                NotePlace::TopLeft => (true, true),
                NotePlace::TopRight => (true, false),
                NotePlace::BottomLeft => (false, true),
                NotePlace::BottomRight => (false, false),
            };
            let top = if at_top { full.top() + top_room + 2.0 } else { bottom - 2.0 - h };
            let left = if at_left { full.left() + 4.0 } else { full.right() - 4.0 - w };
            let r = Rect::from_min_size(Pos2::new(left, top), Vec2::new(w, h));
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
