//! What the shell draws: the panel, the dock, the desktop widgets and the popups (launcher,
//! calendar, power), in the Rivet apps' look; `cc` has the control center, the notification
//! cards and the on-screen display. The same code draws them on the desktop (each on its own
//! layer surface) and in screenshots (all on one picture).

use std::sync::Arc;

use chrono::{DateTime, Datelike, Local, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, Galley, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::icons::IconTheme;
use rivet_look::live::LiveLook;
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md, r_sm};
use rivet_look::fonts;
use rivet_xdg::launch::DesktopApp;

use crate::config::{Edge, Settings};
use crate::notes::Note;
use crate::sys::{Compositor, Power, Status};

/// Heights and room, in points.
pub use crate::panels::MARGIN;
const DOCK_PAD: f32 = 8.0;
const DOCK_GAP: f32 = 6.0;
const DOCK_SEP: f32 = 13.0;
const DOCK_LABEL: f32 = 30.0;

/// An open app's desktop entry, by the id its windows carry.
pub fn open_app<'a>(apps: &'a [DesktopApp], running: &str) -> Option<&'a DesktopApp> {
    find_app(apps, running).or_else(|| apps.iter().find(|a| a.name.to_lowercase() == running))
}

/// Whether an app has windows open.
pub fn is_running(a: &DesktopApp, running: &[String]) -> bool {
    running.iter().any(|w| owns(a, w))
}

/// Whether a window's app id (lowercase) is this app's.
fn owns(a: &DesktopApp, w: &str) -> bool {
    let id = a.id.trim_end_matches(".desktop").to_lowercase();
    let short = id.rsplit('.').next().unwrap_or(&id);
    w == id || w == short || w == a.name.to_lowercase() || id.ends_with(w)
}

/// The app ids of an app's open windows, when the compositor can close them for the dock.
fn closable(m: &Model, app: &str) -> Vec<String> {
    let Some(a) = find_app(m.apps, app).filter(|_| crate::sys::can_close(m.compositor)) else { return Vec::new() };
    m.status.running.iter().filter(|w| owns(a, w)).cloned().collect()
}

/// What the dock shows: the pinned apps, then (when asked) open apps that aren't pinned.
pub fn dock_items(s: &Settings, apps: &[DesktopApp], running: &[String]) -> Vec<(DesktopApp, bool)> {
    let mut out: Vec<(DesktopApp, bool)> = s.dock_apps.iter().filter_map(|id| find_app(apps, id).cloned()).map(|a| (a, true)).collect();
    if s.dock_open_apps {
        for r in running {
            if let Some(a) = open_app(apps, r)
                && !out.iter().any(|(x, _)| x.id == a.id)
            {
                out.push((a.clone(), false));
            }
        }
    }
    out
}

/// Move the pinned app `id` to place `to` among the pins the dock shows (a pin whose app isn't
/// installed isn't shown, and keeps its place beside the one before it).
pub fn place_pin(pins: &mut Vec<String>, apps: &[DesktopApp], id: &str, to: usize) {
    let Some(from) = pins.iter().position(|d| find_app(apps, d).is_some_and(|a| a.id == id)) else { return };
    let moved = pins.remove(from);
    let at = pins.iter().enumerate().filter(|(_, d)| find_app(apps, d).is_some()).map(|(i, _)| i).nth(to).unwrap_or(pins.len());
    pins.insert(at, moved);
}

/// The dock surface's size, and the room it keeps from windows.
pub fn dock_size(s: &Settings, items: &[(DesktopApp, bool)]) -> (Vec2, f32) {
    let places = if s.dock_places { 2 } else { 0 };
    let n = (items.len() + places).max(1) as f32;
    let sep = if items.iter().any(|i| i.1) && items.iter().any(|i| !i.1) { DOCK_SEP } else { 0.0 } + if places > 0 && !items.is_empty() { DOCK_SEP } else { 0.0 };
    let w = DOCK_PAD * 2.0 + n * s.dock_icon + (n - 1.0) * DOCK_GAP + sep + MARGIN * 2.0;
    let body = s.dock_icon + DOCK_PAD * 2.0 + MARGIN;
    (Vec2::new(w, body + DOCK_LABEL), body)
}

#[derive(Clone, Debug, PartialEq)]
pub enum PopupKind {
    Launcher { query: String, sel: usize },
    /// The month shown (from this one), and the day picked to list its events (None: today).
    Calendar { month: i32, day: Option<NaiveDate> },
    Power { confirm: Option<Power> },
    /// Quick settings, sliders, what's playing and the notifications (`drag`: a slider held).
    Control { drag: Option<(u8, u32)> },
    /// The HUD: widgets over everything (`edit`: being arranged; `query`: what's been typed to
    /// narrow its apps).
    Hud { edit: bool, query: String },
    /// The desktop's widgets being arranged, over the windows.
    WidgetEdit,
    /// Right-click on a dock icon.
    DockMenu { app: String, pinned: bool },
    /// The newest downloads, from the dock's Downloads.
    Downloads,
    /// The Trash's menu on the dock (`confirm`: asking before emptying it).
    TrashMenu { confirm: bool },
    /// Wi-Fi networks (`ask`: the network a password is being typed for, and the password).
    Network { ask: Option<(String, String)> },
    /// Bluetooth devices.
    Bluetooth,
    /// AI plans' usage.
    Ai,
    /// The weather: now, the next hours and the week.
    Weather,
    /// The notification history (and later perhaps more beside it).
    Notifications,
    /// The clipboard history (`sel`: the copy chosen with the arrow keys).
    Clipboard { sel: Option<usize> },
    /// Sound devices and apps (`drag`: a volume being dragged).
    Sound { drag: Option<(u64, u32)> },
    /// A plugin's menu.
    Plugin { id: String },
    /// A tray icon's menu: its item, and the submenus opened (outermost first).
    TrayMenu { key: String, path: Vec<i32> },
    /// The player: what's playing, with its cover and controls.
    Media,
    /// The password dialog, when an app needs an administrator's say-so: what's typed, and
    /// the try it was typed for.
    Auth { typed: String, round: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub kind: PopupKind,
    /// Where it opens from (x on its screen, in points), under the panel button (from a side
    /// panel: y, beside the button); for the dock's menu, from the screen's middle.
    pub anchor_x: f32,
    /// Its screen's name (empty: whichever the compositor picks).
    pub output: String,
    pub focus: bool,
    /// The panel it opens from: that panel's edge, and how far from the edge to open (None:
    /// opened by a key, so under the screen's top panels, or over its bottom ones).
    pub from: Option<(Edge, f32)>,
    /// The panel it's joined to (its popups set to open attached): no gap, square corners on
    /// the panel's side, and the panel's colours.
    pub attach: Option<usize>,
    /// The panel it was opened from (its font is that panel's).
    pub panel: Option<usize>,
    /// Where along that panel it's joined just now (screen points), once drawn: the panel
    /// leaves its outline and shadow out there, so the two are one shape.
    pub join: Option<(f32, f32)>,
}

impl Popup {
    /// Whose font it's in: the launcher's, else the panel's it was opened from.
    pub fn face(&self) -> Face {
        match (&self.kind, self.panel) {
            (PopupKind::Launcher { .. }, _) => Face::Launcher,
            (_, Some(i)) => Face::Panel(i),
            _ => Face::Base,
        }
    }
    pub fn launcher(output: &str) -> Popup {
        Popup::at(PopupKind::Launcher { query: String::new(), sel: 0 }, 0.0, output)
    }
    pub fn at(kind: PopupKind, anchor_x: f32, output: &str) -> Popup {
        Popup { kind, anchor_x, output: output.to_string(), focus: true, from: None, attach: None, panel: None, join: None }
    }
    /// Whether it needs the keyboard (the launcher's search; Esc for the others).
    pub fn wants_keyboard(&self) -> bool {
        matches!(self.kind, PopupKind::Launcher { .. } | PopupKind::Network { .. } | PopupKind::Auth { .. } | PopupKind::Hud { .. } | PopupKind::WidgetEdit | PopupKind::Clipboard { .. })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OsdKind {
    Volume,
    Mic,
    Brightness,
    /// Caps Lock or Num Lock turned on (value 1) or off.
    CapsLock,
    NumLock,
}

/// What the shell itself keeps (not read from the computer): notifications and its toggles.
#[derive(Clone, Debug, Default)]
pub struct Extra {
    /// The history, newest first.
    pub notes: Vec<Note>,
    /// The cards on screen, newest first.
    pub cards: Vec<Note>,
    /// Whether the shell is the notification service.
    pub notifying: bool,
    pub dnd: bool,
    /// None: no night light program installed.
    pub night: Option<bool>,
    /// None: the compositor can't keep the screen awake for the shell.
    pub awake: Option<bool>,
    pub osd: Option<(OsdKind, u32, bool)>,
    pub user: String,
    /// Wi-Fi networks and Bluetooth devices (while their popup is open).
    pub radios: crate::radio::Radios,
    /// AI plans' usage (while a panel shows the AI item).
    pub ai: crate::ai::Quotas,
    /// The weather (while the widget or a panel's item shows it).
    pub weather: crate::weather::Weather,
    /// Sound devices and apps (while the Sound popup is open).
    pub audio: crate::audio::Audio,
    /// Apps' tray icons, and the open tray menu.
    pub tray: crate::tray::Tray,
    /// What the plugins that are on show.
    pub plugins: crate::plugins::Items,
    /// A popup joined to a panel: the panel, and where along it (screen points).
    pub join: Option<(usize, f32, f32)>,
    /// The sound's levels, low notes first, while something shows the visualizer (None:
    /// this computer can't listen).
    pub viz: Option<Vec<f32>>,
    /// The password being asked for (while its dialog is open).
    pub auth: Option<crate::polkit::View>,
    /// Files being dragged over the dock.
    pub drop: Option<DockDrop>,
    /// The clipboard history, newest first (None: off, or the compositor doesn't share the
    /// clipboard).
    pub clip: Option<Vec<crate::clip::Clip>>,
}

/// Files being dragged over the dock: where the pointer is on it, and the files' types once
/// they're known (until then every slot looks as if it takes them).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DockDrop {
    pub pos: Pos2,
    pub mimes: Option<Vec<String>>,
}

/// A place on the dock.
#[derive(Clone, Debug)]
pub enum DockSlot {
    /// An app, and whether it's pinned (else it's only open).
    App(DesktopApp, bool),
    Downloads,
    Trash,
}

/// The dock's slots and where each is on its surface (`full`), and where the lines between
/// its groups go: the pinned apps, the open ones, then Downloads and the Trash.
pub fn dock_layout(s: &Settings, apps: &[DesktopApp], running: &[String], full: Rect) -> (Rect, Vec<(DockSlot, Rect)>, Vec<f32>) {
    let items = dock_items(s, apps, running);
    let size = s.dock_icon;
    let body_h = size + DOCK_PAD * 2.0;
    let body = Rect::from_min_size(Pos2::new(full.left() + MARGIN, full.bottom() - MARGIN - body_h), Vec2::new(full.width() - MARGIN * 2.0, body_h));
    let mut slots: Vec<DockSlot> = items.iter().map(|(a, pinned)| DockSlot::App(a.clone(), *pinned)).collect();
    if s.dock_places {
        slots.extend([DockSlot::Downloads, DockSlot::Trash]);
    }
    let mut x = body.left() + DOCK_PAD;
    let mut out = Vec::new();
    let mut lines = Vec::new();
    for (k, slot) in slots.into_iter().enumerate() {
        // A line where the pinned apps end and the open ones start, and before the places.
        let before = match (&slot, k) {
            (_, 0) => false,
            (DockSlot::App(_, false), _) => matches!(out.last(), Some((DockSlot::App(_, true), _))),
            (DockSlot::Downloads, _) => true,
            _ => false,
        };
        if before {
            lines.push(x + DOCK_SEP / 2.0 - DOCK_GAP / 2.0);
            x += DOCK_SEP;
        }
        out.push((slot, Rect::from_min_size(Pos2::new(x, body.top() + DOCK_PAD), Vec2::splat(size))));
        x += size + DOCK_GAP;
    }
    (body, out, lines)
}

/// The slot under `pos` on the dock's surface, if any (the room between icons counts as theirs).
pub fn dock_slot_at(s: &Settings, apps: &[DesktopApp], running: &[String], full: Rect, pos: Pos2) -> Option<DockSlot> {
    let (_, slots, _) = dock_layout(s, apps, running, full);
    slots.into_iter().find(|(_, r)| r.expand2(Vec2::new(DOCK_GAP / 2.0 + 0.5, DOCK_PAD)).contains(pos)).map(|(slot, _)| slot)
}

/// Whether a slot takes files of these types when they're dropped on it: an app that says it
/// opens every one of them, or the Trash. Types not known yet: anything might.
pub fn dock_takes(slot: &DockSlot, mimes: Option<&[String]>) -> bool {
    match (slot, mimes) {
        // Something to throw away: not a drag of only web addresses.
        (DockSlot::Trash, m) => m.is_none_or(|m| !m.is_empty()),
        (DockSlot::Downloads, _) => false,
        (DockSlot::App(..), None) => true,
        (DockSlot::App(a, _), Some(ms)) => !ms.is_empty() && ms.iter().all(|m| !rivet_xdg::launch::apps_for(std::slice::from_ref(a), m).is_empty()),
    }
}

/// What the person asked for, done by whoever runs the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Launch(String),
    /// A desktop widget dragged to a new place (a fraction of the room it moves in), or put
    /// back in its usual place (None).
    /// A widget moved, resized or put on other displays: which in the list, and as it is now.
    WidgetPut(usize, crate::config::Placed),
    WidgetAdd(crate::config::Placed),
    WidgetRemove(usize),
    /// Widgets being arranged on a board lock to its grid, or move freely.
    BoardGrid(crate::config::Board, bool),
    /// Done editing a board: the HUD goes back to being the HUD, the desktop's editor closes.
    BoardDone,
    /// Sound devices and apps: choose, change the volume, mute, move.
    Audio(crate::audio::Job),
    /// A program with no desktop entry, by name.
    Spawn(String),
    Workspace(String),
    Open(Popup),
    Close,
    /// A sub panel, by name: open, close or keep it, on this screen (empty: every screen).
    SubPanel(String, SubDo, String),
    Volume(bool),
    SetVolume(u32),
    Mute,
    MicMute,
    Brightness(bool),
    SetBrightness(u32),
    Media(&'static str),
    Wifi(bool),
    Bluetooth(bool),
    /// Turn a VPN (`vpn::Vpn::key`) on or off.
    Vpn(String, bool),
    /// Join, leave, pair, forget… (done on the radios' thread).
    Radio(crate::radio::Job),
    /// Read the AI plans' usage again now.
    AiRefresh,
    /// Read the weather again now.
    WeatherRefresh,
    /// Pin a limit to the panel or take it off ("provider:limit name").
    AiPin(String),
    NightLight(bool),
    Dnd(bool),
    KeepAwake(bool),
    /// Float the windows of the workspace with the focus, or put them back in columns.
    FloatingLayout,
    NextProfile,
    DarkStyle(bool),
    Pin(String),
    Unpin(String),
    MovePin(String, i32),
    /// A pinned app dragged along the dock: its desktop id, and its new place among the pins shown.
    PlacePin(String, usize),
    /// The calendar shows the days from one to the other: plugins that give events are told.
    CalendarSpan(NaiveDate, NaiveDate),
    /// Open the Trash in the file manager.
    OpenTrash,
    /// Delete what's in the Trash for good.
    EmptyTrash,
    /// Open a file (of this type) with its app.
    OpenFile(std::path::PathBuf, String),
    /// Start dragging a file out of the shell, to drop in another app.
    DragOut(std::path::PathBuf),
    /// Add an app to the launcher's favourites, or take it off.
    Favorite(String),
    /// Close every window with one of these app ids.
    CloseWindows(Vec<String>),
    /// A notification's card clicked (its default action).
    NoteClick(u32),
    NoteAction(u32, String),
    /// Off the screen (kept in the history).
    NoteHide(u32),
    /// Out of the history.
    NoteRemove(u32),
    ClearNotes,
    Copy(String),
    Power(Power),
    Settings,
    /// The Settings window, on the page of this name.
    SettingsPage(&'static str),
    /// Something for a tray icon's app (a click, its menu…).
    Tray(crate::tray::Job),
    /// Something for a plugin: a click on its item, a row of its menu.
    Plugin(String, crate::plugins::Event),
    /// What was typed into the password dialog.
    Auth(String),
    /// Jump to this many seconds into what's playing.
    MediaSeek(u32),
    /// Put a copy back on the clipboard, pin it, remove it…
    Clip(crate::clip::Job),
}

/// What's asked of a sub panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SubDo {
    Toggle,
    Open,
    Close,
    /// Left open, or back to rolling up by itself.
    Stay,
}

impl SubDo {
    pub fn from(v: &str) -> Option<SubDo> {
        Some(match v.trim() {
            "toggle" => SubDo::Toggle,
            "open" | "show" => SubDo::Open,
            "close" | "hide" => SubDo::Close,
            "stay" | "keep" | "pin" => SubDo::Stay,
            _ => return None,
        })
    }
}

/// Each surface's look: colours and fonts (following the desktop), and icons.
/// Whose font a surface's text is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Face {
    /// Appearance's.
    #[default]
    Base,
    /// A panel's (and its popups'), when it has its own.
    Panel(usize),
    Launcher,
}

/// The icon theme in use: the chosen one, else the desktop's.
pub fn icon_theme(s: &Settings) -> IconTheme {
    if s.icon_theme.is_empty() { IconTheme::new(&rivet_look::icons::detect_system_theme()) } else { IconTheme::new(&s.icon_theme) }
}

pub struct Kit {
    pub look: LiveLook,
    /// Set before `begin`.
    pub face: Face,
    pub icons: IconTheme,
    /// The icon theme setting `icons` was made for (empty: the desktop's).
    icon_key: String,
    /// Tray icons made from apps' pixels (by item, icon and recolouring).
    pub tray_tex: std::collections::HashMap<(String, u64, bool), egui::TextureHandle>,
    /// The clipboard history's thumbnails, by copy.
    clip_tex: std::collections::HashMap<u64, egui::TextureHandle>,
    live: bool,
    /// The person's picture, once looked for (None inside: they have none).
    avatar: Option<Option<egui::TextureHandle>>,
}

impl Kit {
    pub fn new(live: bool) -> Kit {
        Kit { look: LiveLook::new().without_watcher(), icons: IconTheme::new(&rivet_look::icons::detect_system_theme()), icon_key: String::new(), tray_tex: Default::default(), clip_tex: Default::default(), face: Face::Base, live, avatar: None }
    }

    /// The person's picture (as on the lock screen), read the first time it's asked for.
    /// Screenshots and checks never show it.
    pub fn avatar(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let live = self.live;
        self.avatar
            .get_or_insert_with(|| {
                let img = if live { crate::lock::avatar(&crate::auth::login_name()) } else { None };
                img.map(|i| ctx.load_texture("rivet-avatar", i, egui::TextureOptions::LINEAR))
            })
            .clone()
    }

    /// Call first in every frame of the surface.
    pub fn begin(&mut self, ctx: &egui::Context, s: &Settings) {
        self.look.set_fonts(ctx, s.font_for(self.face), "", "");
        s.set_surfaces(&mut self.look);
        self.look.frame(ctx, &s.choice(), self.live);
        if s.icon_theme != self.icon_key {
            self.icon_key = s.icon_theme.clone();
            self.icons = icon_theme(s);
            self.tray_tex.clear();
        }
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&s.choice());
        self.icons.set_builtin(Some(rivet_look::builtin_icons::BuiltinColors { accent, bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }));
    }

    /// A picture's thumbnail from the clipboard history, made the first time it shows.
    pub fn clip_thumb(&mut self, ctx: &egui::Context, id: u64, pic: &crate::clip::Picture) -> Option<egui::TextureHandle> {
        if let Some(t) = self.clip_tex.get(&id) {
            return Some(t.clone());
        }
        let (w, h, px) = &pic.thumb;
        if px.len() != (*w * *h * 4) as usize || *w == 0 {
            return None;
        }
        // Copies come and go: the oldest thumbnails aren't worth keeping track of.
        if self.clip_tex.len() > 200 {
            self.clip_tex.clear();
        }
        let t = ctx.load_texture(format!("rivet-clip-{id}"), egui::ColorImage::from_rgba_unmultiplied([*w as usize, *h as usize], px), egui::TextureOptions::LINEAR);
        self.clip_tex.insert(id, t.clone());
        Some(t)
    }

    pub fn accent(&self, s: &Settings) -> Color32 {
        self.look.accent(&s.choice())
    }

    /// A part's colour: the accent, or the theme's secondary or tertiary where the shell is
    /// set to use them.
    pub fn tone(&self, s: &Settings, slot: crate::style::Slot) -> Color32 {
        crate::style::Roles::new(&self.look, s).tone(slot, s.color_use)
    }

    fn own_icon(&mut self, ctx: &egui::Context, icon: &str, size: u32) -> Option<egui::TextureHandle> {
        if icon.starts_with('/') {
            self.icons.texture_from_path(ctx, std::path::Path::new(icon), size)
        } else if icon.is_empty() {
            None
        } else {
            self.icons.texture(ctx, &[icon], size)
        }
    }

    /// `named_icon` in white, keeping its shape, to tint in one colour.
    pub fn named_mask(&mut self, ctx: &egui::Context, icon: &str, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        if icon.starts_with('/') {
            self.icons.mask_from_path(ctx, std::path::Path::new(icon), size)
        } else if icon.is_empty() {
            None
        } else {
            self.icons.mask(ctx, &[icon], size)
        }
    }

    /// The launcher item's icon, when it isn't the grid: the system's logo (as /etc/os-release
    /// names it, else the icon theme's usual names for one), or the custom icon or picture.
    pub fn launcher_icon(&mut self, ctx: &egui::Context, s: &Settings, px: f32) -> Option<egui::TextureHandle> {
        use crate::config::LauncherIcon;
        let size = (px * ctx.pixels_per_point()).round() as u32;
        match s.launcher_icon {
            LauncherIcon::Grid => None,
            LauncherIcon::Custom => {
                let c = s.launcher_custom.trim();
                let path = crate::wall::expand(c);
                if path.is_absolute() { self.icons.texture_from_path(ctx, &path, size) } else { self.own_icon(ctx, c, size) }
            }
            LauncherIcon::System => {
                static LOGO: std::sync::OnceLock<String> = std::sync::OnceLock::new();
                let logo = LOGO.get_or_init(|| {
                    let text = std::fs::read_to_string("/etc/os-release").or_else(|_| std::fs::read_to_string("/usr/lib/os-release")).unwrap_or_default();
                    text.lines().find_map(|l| l.strip_prefix("LOGO=")).map(|v| v.trim().trim_matches('"').to_string()).unwrap_or_default()
                });
                let names: Vec<&str> = [logo.as_str(), "distributor-logo", "start-here"].into_iter().filter(|n| !n.is_empty()).collect();
                self.icons.texture(ctx, &names, size)
            }
        }
    }

    /// An icon by its name in the icon theme, or a picture's path (a plugin's).
    pub fn named_icon(&mut self, ctx: &egui::Context, icon: &str, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        self.own_icon(ctx, icon, size)
    }

    pub fn app_icon(&mut self, ctx: &egui::Context, app: &DesktopApp, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        // The app's own icon first: a fallback name in the same list could be answered by
        // Rivet's built-in icons before the theme is even asked for the app's.
        self.own_icon(ctx, &app.icon, size).or_else(|| self.icons.texture(ctx, &["application-x-executable"], size))
    }

    /// A notification's picture: its own, else its app's.
    pub fn note_icon(&mut self, ctx: &egui::Context, n: &Note, apps: &[DesktopApp], px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        if let Some(t) = self.own_icon(ctx, &n.icon, size) {
            return Some(t);
        }
        let app = (!n.desktop.is_empty()).then(|| find_app(apps, &n.desktop)).flatten().or_else(|| open_app(apps, &n.app.to_lowercase()))?;
        self.own_icon(ctx, &app.icon.clone(), size)
    }
}

/// What a frame needs to know.
pub struct Model<'a> {
    pub s: &'a Settings,
    pub status: &'a Status,
    pub extra: &'a Extra,
    pub apps: &'a [DesktopApp],
    pub compositor: Compositor,
    /// This surface's screen ("" when unknown: everything shows).
    pub output: &'a str,
    pub acts: &'a mut Vec<Act>,
}

/// An app by desktop id (with or without ".desktop", any case; "firefox" finds
/// "org.mozilla.firefox").
pub fn find_app<'a>(apps: &'a [DesktopApp], id: &str) -> Option<&'a DesktopApp> {
    let want = id.trim_end_matches(".desktop").to_lowercase();
    let key = |a: &DesktopApp| a.id.trim_end_matches(".desktop").to_lowercase();
    apps.iter().find(|a| key(a) == want).or_else(|| apps.iter().find(|a| key(a).rsplit('.').next() == Some(want.as_str())))
}

pub fn ui_font(size: f32) -> FontId {
    fonts::ui_font(size)
}

/// Black or white, whichever reads on `fill`.
pub fn on(fill: Color32) -> Color32 {
    if rivet_look::palette::luminance(fill) > 0.6 { Color32::BLACK } else { Color32::WHITE }
}

/// A see-through card in the panel's colour, with a soft shadow.
pub fn card(p: &egui::Painter, r: Rect, v: &egui::Visuals, alpha: f32, radius: impl Into<egui::CornerRadius>) {
    surface(p, r, v, alpha, radius, true);
}

/// A rolled-up sub panel's handle, in its surface: a small tab past its host, to pause the
/// pointer on (nothing for one that only opens when asked).
pub fn sub_handle(ui: &egui::Ui, s: &Settings, kit: &Kit, idx: usize, output: &str) {
    use crate::panels::HandleShape;
    let Some(r) = crate::panels::handle_rect(&s.panels, idx, ui.max_rect(), output) else { return };
    let Some(h) = s.panels[idx].sub.as_ref().map(|x| x.handle) else { return };
    let v = ui.visuals();
    let p = ui.painter();
    // One of the theme's colours, plain; or the panel's own, outlined so it shows over anything.
    let (fill, edge) = match h.color {
        Some(role) => (crate::style::Roles::new(&kit.look, s).get(role, v.panel_fill), None),
        None => (v.panel_fill, Some(mix(v.panel_fill, v.text_color(), 0.35))),
    };
    let (fill, edge) = (fill.gamma_multiply(h.opacity), edge.map(|c| c.gamma_multiply(h.opacity)));
    let down = s.panels[idx].vertical();
    let thick = h.thick as f32;
    match h.shape {
        HandleShape::Bar | HandleShape::Line => {
            let round = if h.shape == HandleShape::Bar { thick / 2.0 } else { 0.0 };
            p.rect_filled(r, round, fill);
            if let Some(c) = edge {
                p.rect_stroke(r, round, Stroke::new(1.0, c), egui::StrokeKind::Inside);
            }
        }
        HandleShape::Dots => {
            // As many as fit its length, three at least.
            let len = if down { r.height() } else { r.width() };
            let n = ((len / (thick * 2.5)).floor() as usize).max(3);
            let step = if n > 1 { (len - thick) / (n - 1) as f32 } else { 0.0 };
            for k in 0..n {
                let along = thick / 2.0 + step * k as f32;
                let c = if down { Pos2::new(r.center().x, r.top() + along) } else { Pos2::new(r.left() + along, r.center().y) };
                p.circle_filled(c, thick / 2.0, fill);
                if let Some(col) = edge {
                    p.circle_stroke(c, thick / 2.0, Stroke::new(1.0, col));
                }
            }
        }
        HandleShape::Arrow => {
            // Pointing away from the host: the way the sub panel rolls out.
            let w = (thick * 0.5).max(1.5);
            let q = r.shrink(w / 2.0);
            let pts = match s.panels[idx].edge {
                Edge::Top => vec![q.left_top(), q.center_bottom(), q.right_top()],
                Edge::Bottom => vec![q.left_bottom(), q.center_top(), q.right_bottom()],
                Edge::Left => vec![q.left_top(), q.right_center(), q.left_bottom()],
                Edge::Right => vec![q.right_top(), q.left_center(), q.right_bottom()],
            };
            if let Some(c) = edge {
                p.add(egui::Shape::line(pts.clone(), Stroke::new(w + 2.0, c)));
            }
            p.add(egui::Shape::line(pts, Stroke::new(w, fill)));
        }
    }
}

/// A card with or without its outline (the panels and the dock, as their look asks).
pub fn surface(p: &egui::Painter, r: Rect, v: &egui::Visuals, alpha: f32, radius: impl Into<egui::CornerRadius>, border: bool) {
    surface_open(p, r, v, alpha, radius, border, None);
}

fn glass_id() -> egui::Id {
    egui::Id::unique("rivet-glass")
}

/// A see-through shape: the whole of it, its corners, and as much of it as shows.
pub type Glass = (Rect, egui::CornerRadius, Rect);

/// The see-through shapes drawn this frame, taken for the compositor to blur behind. Taking
/// them empties the list for the next frame.
pub fn take_glass(ctx: &egui::Context) -> Vec<Glass> {
    ctx.data_mut(|d| d.remove_temp::<Vec<Glass>>(glass_id())).unwrap_or_default()
}

/// A rounded rectangle as whole-point rectangles (x, y, w, h) that follow its corners in steps,
/// for a compositor that takes regions as rectangles: the middle, and bands across the top and
/// bottom narrowing towards the corners. Only what is inside `shown` is kept: a dock part of
/// the way off its edge is cut straight there, and its corners stay where they are drawn.
pub fn glass_rects(r: Rect, radius: egui::CornerRadius, shown: Rect) -> Vec<[i32; 4]> {
    let (l, t, rr, b) = (r.left().round() as i32, r.top().round() as i32, r.right().round() as i32, r.bottom().round() as i32);
    let top = radius.nw.max(radius.ne) as i32;
    let bottom = radius.sw.max(radius.se) as i32;
    let mut out = Vec::new();
    let mid = (t + top, b - bottom);
    if mid.1 > mid.0 {
        out.push([l, mid.0, rr - l, mid.1 - mid.0]);
    }
    // How far in a corner of radius `c` is, `k` points from its straight edge.
    let inset = |c: i32, k: f32| -> i32 {
        if c == 0 {
            return 0;
        }
        let c = c as f32;
        let dy = c - k;
        (c - (c * c - dy * dy).max(0.0).sqrt()).ceil() as i32
    };
    // Bands two points tall: fine enough that the steps don't show through the blur.
    let mut y = 0;
    while y < top {
        let h = 2.min(top - y);
        // Measured at the band's outer side, so each band stays inside the curve.
        let k = y as f32 + 0.5;
        let (a, z) = (inset(radius.nw as i32, k.min(radius.nw as f32)), inset(radius.ne as i32, k.min(radius.ne as f32)));
        out.push([l + a, t + y, (rr - z) - (l + a), h]);
        y += h;
    }
    let mut y = 0;
    while y < bottom {
        let h = 2.min(bottom - y);
        let k = y as f32 + 0.5;
        let (a, z) = (inset(radius.sw as i32, k.min(radius.sw as f32)), inset(radius.se as i32, k.min(radius.se as f32)));
        out.push([l + a, b - y - h, (rr - z) - (l + a), h]);
        y += h;
    }
    let (sl, st, sr, sb) = (shown.left().round() as i32, shown.top().round() as i32, shown.right().round() as i32, shown.bottom().round() as i32);
    for q in &mut out {
        let (x0, y0, x1, y1) = (q[0].max(sl), q[1].max(st), (q[0] + q[2]).min(sr), (q[1] + q[3]).min(sb));
        *q = [x0, y0, x1 - x0, y1 - y0];
    }
    out.retain(|q| q[2] > 0 && q[3] > 0);
    out
}

/// The clips that leave out a stretch (`lo` to `hi` along it) of `r`'s `side`, and all beyond it.
fn around(clip: Rect, r: Rect, side: Edge, lo: f32, hi: f32) -> [Rect; 3] {
    let (mut a, mut b, mut mid) = (clip, clip, clip);
    if side.vertical() {
        (a.max.y, b.min.y, mid.min.y, mid.max.y) = (lo, hi, lo, hi);
        if side == Edge::Left { mid.min.x = r.left() + 1.5 } else { mid.max.x = r.right() - 1.5 }
    } else {
        (a.max.x, b.min.x, mid.min.x, mid.max.x) = (lo, hi, lo, hi);
        if side == Edge::Top { mid.min.y = r.top() + 1.5 } else { mid.max.y = r.bottom() - 1.5 }
    }
    [a, b, mid]
}

/// A surface whose outline and shadow leave out a stretch of one side (`open`: the side, and
/// from where to where along it, and how far in from those the popup's flares end: the
/// shadow stays beside the flares), where a popup joins it.
pub fn surface_open(p: &egui::Painter, r: Rect, v: &egui::Visuals, alpha: f32, radius: impl Into<egui::CornerRadius>, border: bool, open: Option<(Edge, f32, f32, f32)>) {
    let radius: egui::CornerRadius = radius.into();
    // See-through: what's behind it is blurred, in its shape (and only as much of it as shows).
    if alpha < 0.97 {
        let shown = r.intersect(p.clip_rect());
        if shown.is_positive() {
            p.ctx().data_mut(|d| d.get_temp_mut_or_default::<Vec<Glass>>(glass_id()).push((r, radius, shown)));
        }
    }
    let shadow = egui::Shadow { offset: [0, 2], blur: 14, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
    let stroke = Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08).gamma_multiply(alpha));
    match open {
        None => {
            p.add(shadow.as_shape(r, radius));
            p.rect_filled(r, radius, v.panel_fill.gamma_multiply(alpha));
            if border {
                p.rect_stroke(r, radius, stroke, egui::StrokeKind::Inside);
            }
        }
        Some((side, lo, hi, flare)) => {
            let clips = around(p.clip_rect(), r, side, lo, hi);
            // The shadow: beside the popup as ever; along it, as if the surface went on under
            // the popup (a shadow thins toward its edge, which would show as a lighter band).
            let [a, b, mut mid] = around(p.clip_rect(), r, side, lo + flare, hi - flare);
            for c in [a, b] {
                p.with_clip_rect(c).add(shadow.as_shape(r, radius));
            }
            let mut on = r;
            match side {
                Edge::Top => (mid.min.y, on.min.y) = (r.top(), r.top() - 60.0),
                Edge::Bottom => (mid.max.y, on.max.y) = (r.bottom(), r.bottom() + 60.0),
                Edge::Left => (mid.min.x, on.min.x) = (r.left(), r.left() - 60.0),
                Edge::Right => (mid.max.x, on.max.x) = (r.right(), r.right() + 60.0),
            }
            p.with_clip_rect(mid).add(shadow.as_shape(on, egui::CornerRadius::ZERO));
            p.rect_filled(r, radius, v.panel_fill.gamma_multiply(alpha));
            if border {
                for c in clips {
                    p.with_clip_rect(c).rect_stroke(r, radius, stroke, egui::StrokeKind::Inside);
                }
            }
        }
    }
}

/// A popup joined to its panel along `edge` (the panel's): no outline or shadow where they
/// meet, flaring out into the panel with rounded corners of `flare` points (the look's
/// radius), as see-through as the panel at the join and fading to solid.
fn joined(p: &egui::Painter, full: Rect, rect: Rect, edge: Edge, v: &egui::Visuals, look: &crate::style::Resolved, flare: f32) {
    let mut cr: egui::CornerRadius = r_lg().into();
    let (mut clip, mut line) = (full, full);
    let depth = if edge.vertical() { rect.width() } else { rect.height() };
    // As see-through as the panel for `hold` points from the join, then solid over `fade` more.
    let room = (depth - cr.nw as f32 - 2.0).max(0.0);
    let hold = 44.0f32.min(room * 0.3).round();
    let fade = 130.0f32.min(room - hold).max(0.0).round();
    // The band of the popup from `a` to `b` points off the join.
    let band = |a: f32, b: f32| match edge {
        Edge::Top => Rect::from_min_max(Pos2::new(rect.left(), rect.top() + a), Pos2::new(rect.right(), rect.top() + b)),
        Edge::Bottom => Rect::from_min_max(Pos2::new(rect.left(), rect.bottom() - b), Pos2::new(rect.right(), rect.bottom() - a)),
        Edge::Left => Rect::from_min_max(Pos2::new(rect.left() + a, rect.top()), Pos2::new(rect.left() + b, rect.bottom())),
        Edge::Right => Rect::from_min_max(Pos2::new(rect.right() - b, rect.top()), Pos2::new(rect.right() - a, rect.bottom())),
    };
    let (grad, rest) = (band(hold, hold + fade), band(hold + fade, depth));
    match edge {
        Edge::Top => ((cr.nw, cr.ne), clip.min.y, line.min.y) = ((0, 0), rect.top(), rect.top() + 1.5),
        Edge::Bottom => ((cr.sw, cr.se), clip.max.y, line.max.y) = ((0, 0), rect.bottom(), rect.bottom() - 1.5),
        Edge::Left => ((cr.nw, cr.sw), clip.min.x, line.min.x) = ((0, 0), rect.left(), rect.left() + 1.5),
        Edge::Right => ((cr.ne, cr.se), clip.max.x, line.max.x) = ((0, 0), rect.right(), rect.right() - 1.5),
    }
    let shadow = egui::Shadow { offset: [0, 2], blur: 14, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
    // Its shadow as if it went on under the panel, so it doesn't thin toward the join.
    let mut under = rect;
    match edge {
        Edge::Top => under.min.y -= 60.0,
        Edge::Bottom => under.max.y += 60.0,
        Edge::Left => under.min.x -= 60.0,
        Edge::Right => under.max.x += 60.0,
    }
    p.with_clip_rect(clip).add(shadow.as_shape(under, cr));
    let (near, far) = (v.panel_fill.gamma_multiply(look.opacity), v.panel_fill.gamma_multiply(look.opacity.max(0.98)));
    p.rect_filled(band(0.0, hold), 0.0, near);
    let mut mesh = egui::Mesh::default();
    for c in [grad.left_top(), grad.right_top(), grad.right_bottom(), grad.left_bottom()] {
        let at_join = match edge {
            Edge::Top => c.y == grad.top(),
            Edge::Bottom => c.y == grad.bottom(),
            Edge::Left => c.x == grad.left(),
            Edge::Right => c.x == grad.right(),
        };
        mesh.colored_vertex(c, if at_join { near } else { far });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
    p.rect_filled(rest, cr, far);
    let stroke = Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08).gamma_multiply(look.opacity));
    if flare >= 1.0 {
        // Along the join and away from it, to the screen.
        let at = |a: f32, d: f32| match edge {
            Edge::Top => Pos2::new(a, rect.top() + d),
            Edge::Bottom => Pos2::new(a, rect.bottom() - d),
            Edge::Left => Pos2::new(rect.left() + d, a),
            Edge::Right => Pos2::new(rect.right() - d, a),
        };
        let (lo, hi) = if edge.vertical() { (rect.top(), rect.bottom()) } else { (rect.left(), rect.right()) };
        // Each side: the corner, then a quarter circle from the panel out to the popup's side.
        for (corner, out) in [(lo, -1.0f32), (hi, 1.0)] {
            let arc = |r: f32| (0..=12).map(move |i| i as f32 / 12.0 * std::f32::consts::FRAC_PI_2).map(move |t| (corner + out * (flare - r * t.sin()), flare - r * t.cos()));
            // A fan from the corner (its thin tip along the panel would trip the smoothing of
            // a filled path), fading with the popup.
            let mut fan = egui::Mesh::default();
            fan.colored_vertex(at(corner, 0.0), near);
            for (a, d) in arc(flare) {
                let t = if fade > 0.0 { ((d - hold) / fade).clamp(0.0, 1.0) } else { 1.0 };
                fan.colored_vertex(at(a, d), v.panel_fill.gamma_multiply(look.opacity + (look.opacity.max(0.98) - look.opacity) * t));
            }
            for i in 1..fan.vertices.len() as u32 - 1 {
                fan.add_triangle(0, i, i + 1);
            }
            p.add(egui::Shape::mesh(fan));
            if look.border {
                // The outline, half a point inside the shape (as the panel's and the popup's).
                p.add(egui::Shape::line(arc(flare + 0.5).map(|(a, d)| at(a, d)).collect(), stroke));
                match edge {
                    Edge::Top => line.min.y = rect.top() + flare,
                    Edge::Bottom => line.max.y = rect.bottom() - flare,
                    Edge::Left => line.min.x = rect.left() + flare,
                    Edge::Right => line.max.x = rect.right() - flare,
                }
            }
        }
    }
    if look.border {
        p.with_clip_rect(line).rect_stroke(rect, cr, stroke, egui::StrokeKind::Inside);
    }
}

pub fn hover_fill(p: &egui::Painter, r: Rect, v: &egui::Visuals) {
    p.rect_filled(r, r_md(), mix(v.panel_fill, v.text_color(), 0.09));
}

/// Text on one line, cut with "…" to fit.
pub fn one_line(p: &egui::Painter, text: &str, font: FontId, color: Color32, max_w: f32) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.replace('\n', " "), egui::TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: 1, break_anywhere: true, overflow_character: Some('…') };
    p.layout_job(job)
}

/// Text wrapped to at most `rows` lines.
pub fn wrapped(p: &egui::Painter, text: &str, font: FontId, color: Color32, max_w: f32, rows: usize) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: font, color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: max_w.max(10.0), max_rows: rows, break_anywhere: false, overflow_character: Some('…') };
    p.layout_job(job)
}

/// "now", "5 min", "2 h", "Mon"…
pub fn ago(at: DateTime<Local>) -> String {
    let s = (Local::now() - at).num_seconds().max(0);
    match s {
        0..60 => "now".into(),
        60..3600 => format!("{} min", s / 60),
        3600..86400 => format!("{} h", s / 3600),
        _ => at.format("%a").to_string(),
    }
}

// ------------------------------------------------------------------ the dock

pub fn dock(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let look = m.s.dock_look();
    let (v, accent) = crate::style::visuals(ui.visuals(), &look, &crate::style::Roles::new(&kit.look, m.s));
    // The dots take the tertiary when the shell uses it (and the dock has no highlight of its own).
    let dot = if m.s.dock_look.highlight.is_none() && m.s.look.highlight.is_none() { kit.tone(m.s, crate::style::Slot::Dot) } else { accent };
    *ui.visuals_mut() = v.clone();
    let hover_r = crate::style::inner_radius(&look, DOCK_PAD - 3.0);
    let full = ui.max_rect();
    let size = m.s.dock_icon;
    let (body, slots, lines) = dock_layout(m.s, m.apps, &m.status.running, full);
    surface(ui.painter(), body, &v, look.opacity, crate::style::radius(&look), look.border);
    let p = ui.painter().clone();
    for sx in lines {
        p.line_segment([Pos2::new(sx, body.top() + 12.0), Pos2::new(sx, body.bottom() - 12.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.2)));
    }
    let mut label: Option<(String, f32)> = None;
    // Files dragged over it: the slots that can't take them fade, and the one under the
    // pointer that can is lit as if hovered.
    let drop = m.extra.drop.clone();
    let mimes = drop.as_ref().and_then(|d| d.mimes.clone());
    let trash_full = crate::places_pop::trash_full();
    // A pinned app held and dragged along the dock (its id, and where on the icon it was
    // taken): it follows the pointer, and the other pins make room where it would land.
    let drag_id = egui::Id::unique("dock-drag");
    let pins: Vec<(String, f32)> = slots.iter().filter_map(|(slot, r)| if let DockSlot::App(a, true) = slot { Some((a.id.clone(), r.left())) } else { None }).collect();
    // With it is kept the place it would land, so it can still be put down once the pointer
    // has left the dock.
    type Carry = (String, f32, usize);
    let kept: Option<Carry> = ui.ctx().data(|d| d.get_temp::<Carry>(drag_id));
    let (down, at) = ui.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
    let mut held = None;
    if let Some((id, grab, mut to)) = kept {
        let known = pins.iter().any(|(p, _)| *p == id);
        if let Some(pos) = at.filter(|_| known) {
            let cx = pos.x - grab;
            to = pins.iter().filter(|(p, x)| *p != id && x + size / 2.0 < cx).count();
            held = Some((id.clone(), cx, to));
        }
        // It is put down when the button comes up, and also when the pointer is gone: past the
        // dock's end the compositor may send the release elsewhere, and a pin that never heard
        // of it would stay in the hand.
        if !down || at.is_none() || !known {
            if known {
                m.acts.push(Act::PlacePin(id, to));
            }
            ui.ctx().data_mut(|d| d.remove::<Carry>(drag_id));
        } else {
            ui.ctx().data_mut(|d| d.insert_temp::<Carry>(drag_id, (id, grab, to)));
        }
    }
    for (slot, r) in &slots {
        let id = match slot {
            DockSlot::App(a, _) => a.id.clone(),
            DockSlot::Downloads => "@downloads".into(),
            DockSlot::Trash => "@trash".into(),
        };
        let can_move = matches!(slot, DockSlot::App(_, true)) && pins.len() > 1;
        let resp = ui.interact(r.expand(2.0), egui::Id::unique(("dock", &id)), if can_move { Sense::click_and_drag() } else { Sense::click() });
        if can_move && resp.drag_started() {
            // Where it was pressed, not where the pointer had got to when it counted as a drag.
            let grab = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()).map_or(0.0, |p| p.x - r.center().x);
            let from = pins.iter().position(|(p, _)| *p == id).unwrap_or(0);
            ui.ctx().data_mut(|d| d.insert_temp::<Carry>(drag_id, (id.clone(), grab, from)));
        }
        // Where it is drawn: its slot, or while a pin is being dragged, under the pointer (the
        // one held) or in the slot it would have were the pin dropped now (the others, moving
        // over to it).
        let mut carried = false;
        let r = &match (slot, &held) {
            (DockSlot::App(_, true), Some((h, cx, _))) if *h == id => {
                carried = true;
                ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
                let x = (cx - size / 2.0).clamp(pins[0].1, pins[pins.len() - 1].1);
                Rect::from_min_size(Pos2::new(x, r.top()), r.size())
            }
            (DockSlot::App(_, true), held) => {
                let x = match held {
                    Some((h, _, to)) => {
                        let k = pins.iter().filter(|(p, _)| p != h).position(|(p, _)| *p == id).unwrap_or(0);
                        ui.ctx().animate_value_with_time(egui::Id::unique(("dock-x", &id)), pins[if k >= *to { k + 1 } else { k }.min(pins.len() - 1)].1, 0.12)
                    }
                    // At rest: in its slot at once (the dock's own changes of size don't glide).
                    None => ui.ctx().animate_value_with_time(egui::Id::unique(("dock-x", &id)), r.left(), 0.0),
                };
                Rect::from_min_size(Pos2::new(x, r.top()), r.size())
            }
            _ => *r,
        };
        // The one carried is drawn over the rest.
        let p = if carried { ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Tooltip, drag_id)) } else { p.clone() };
        let (lit, faded) = match &drop {
            Some(d) => {
                let takes = dock_takes(slot, mimes.as_deref());
                (takes && r.expand2(Vec2::new(DOCK_GAP / 2.0 + 0.5, DOCK_PAD)).contains(d.pos), !takes)
            }
            None => (if held.is_some() { carried } else { resp.hovered() }, false),
        };
        let lift = if lit { -4.0 } else { 0.0 };
        let r2 = r.translate(Vec2::new(0.0, lift));
        let name = match slot {
            DockSlot::App(a, _) => a.name.clone(),
            DockSlot::Downloads => "Downloads".into(),
            DockSlot::Trash => "Trash".into(),
        };
        if lit {
            p.rect_filled(r.expand(3.0), hover_r, mix(v.panel_fill, v.text_color(), 0.08));
            if !carried {
                label = Some((name.clone(), r.center().x));
            }
        }
        let tint = if faded { Color32::from_white_alpha(80) } else { Color32::WHITE };
        let tex = match slot {
            DockSlot::App(a, _) => kit.app_icon(ui.ctx(), a, size),
            DockSlot::Downloads => kit.named_icon(ui.ctx(), "folder-download", size).or_else(|| kit.named_icon(ui.ctx(), "folder", size)),
            DockSlot::Trash => kit.named_icon(ui.ctx(), if trash_full { "user-trash-full" } else { "user-trash" }, size),
        };
        match tex {
            Some(t) => {
                p.image(t.id(), r2, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
            }
            None => {
                p.rect_filled(r2.shrink(4.0), r_md(), mix(accent, v.panel_fill, 0.3).gamma_multiply(if faded { 0.3 } else { 1.0 }));
                p.text(r2.center(), Align2::CENTER_CENTER, name.chars().next().unwrap_or('?').to_uppercase().to_string(), ui_font(size * 0.45), tint);
            }
        }
        // The dock is centred on its screen: a menu's place is the icon's from the middle.
        let at = r.center().x - full.center().x;
        let resp = if held.is_some() { resp } else { resp.on_hover_cursor(egui::CursorIcon::PointingHand) };
        match slot {
            DockSlot::App(a, pinned) => {
                // Open: a dot under it.
                if m.s.dock_running && (!pinned || is_running(a, &m.status.running)) {
                    p.circle_filled(Pos2::new(r.center().x, body.bottom() - 4.0), 2.5, dot);
                }
                if resp.secondary_clicked() {
                    m.acts.push(Act::Open(Popup::at(PopupKind::DockMenu { app: a.id.trim_end_matches(".desktop").to_string(), pinned: *pinned }, at, m.output)));
                } else if resp.clicked() {
                    m.acts.push(Act::Launch(a.id.clone()));
                }
            }
            DockSlot::Downloads => {
                if resp.clicked() || resp.secondary_clicked() {
                    m.acts.push(Act::Open(Popup::at(PopupKind::Downloads, at, m.output)));
                }
            }
            DockSlot::Trash => {
                if resp.secondary_clicked() {
                    m.acts.push(Act::Open(Popup::at(PopupKind::TrashMenu { confirm: false }, at, m.output)));
                } else if resp.clicked() {
                    m.acts.push(Act::OpenTrash);
                }
            }
        }
    }
    // The name above the icon under the pointer.
    if let Some((name, cx)) = label {
        let g = p.layout_no_wrap(name, ui_font(12.5), v.strong_text_color());
        let r = Rect::from_center_size(Pos2::new(cx, body.top() - 15.0), g.size() + Vec2::new(16.0, 8.0));
        let r = r.translate(Vec2::new((full.left() + 2.0 - r.left()).max(0.0) + (full.right() - 2.0 - r.right()).min(0.0), 0.0));
        card(&p, r, &v, 0.96, r_sm());
        p.galley(r.center() - g.size() / 2.0, g, v.strong_text_color());
    }
}

// ------------------------------------------------------------------ popups

const PLUGIN_ROW: f32 = 44.0;

/// A menu row's height: a heading (a line, when it has no words), one line, or two.
fn plugin_row_height(row: &crate::plugins::MenuRow) -> f32 {
    match row {
        r if r.heading && r.label.is_empty() => 11.0,
        r if r.heading => 28.0,
        r if r.sub.is_empty() => 34.0,
        _ => PLUGIN_ROW,
    }
}

const PLUGIN_W: f32 = 340.0;

fn plugin_menu_height(m: &Model, id: &str) -> f32 {
    let rows = crate::plugins::item(&m.extra.plugins, id).map(|i| i.menu.iter().map(|r| plugin_row_height(r) + 2.0).sum::<f32>()).unwrap_or(0.0);
    28.0 + 34.0 + rows.max(PLUGIN_ROW)
}

/// A plugin's menu: its name, then the rows it gave (headings, tasks' boxes, icons); a click
/// tells the plugin which.
fn plugin_menu(ui: &mut egui::Ui, id: &str, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let Some(item) = crate::plugins::item(&m.extra.plugins, id).cloned() else {
        m.acts.push(Act::Close);
        return;
    };
    let accent = kit.accent(m.s);
    let sc = rivet_look::theme::status(ui.ctx());
    ui.spacing_mut().item_spacing.y = 2.0;
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 32.0), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 4.0, r.center().y - 2.0), Align2::LEFT_CENTER, &item.name, ui_font(14.5), v.strong_text_color());
    if !item.tooltip.is_empty() || !item.text.is_empty() {
        // What the item says, beside the name: its hover text when that's short, else its text.
        let side = if !item.tooltip.is_empty() && item.tooltip.chars().count() <= 30 && !item.tooltip.starts_with(&item.name) { &item.tooltip } else { &item.text };
        ui.painter().text(Pos2::new(r.right() - 4.0, r.center().y - 2.0), Align2::RIGHT_CENTER, side, ui_font(12.5), v.weak_text_color());
    }
    egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
        for row in &item.menu {
            let h = plugin_row_height(row);
            if row.heading {
                let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
                if row.label.is_empty() {
                    ui.painter().line_segment([Pos2::new(r.left() + 4.0, r.center().y), Pos2::new(r.right() - 4.0, r.center().y)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.1)));
                } else {
                    ui.painter().text(Pos2::new(r.left() + 6.0, r.bottom() - 9.0), Align2::LEFT_CENTER, &row.label, ui_font(11.5), v.weak_text_color());
                }
                continue;
            }
            let acts = !row.id.is_empty();
            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), if acts { Sense::click() } else { Sense::hover() });
            if acts && resp.hovered() {
                hover_fill(ui.painter(), r, &v);
            }
            let p = ui.painter().clone();
            let mut x = r.left() + 10.0;
            // What's before the words: a task's box, or an icon.
            if let Some(mark) = row.mark {
                let b = Rect::from_center_size(Pos2::new(x + 8.0, if row.sub.is_empty() { r.center().y } else { r.top() + 14.0 }), Vec2::splat(14.0));
                let line = mix(v.panel_fill, v.text_color(), 0.55);
                match mark {
                    'x' => {
                        p.rect_filled(b, 4.0, accent);
                        let c = b.center();
                        p.line(vec![c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.5), c + Vec2::new(3.5, -2.5)], Stroke::new(1.6, v.panel_fill));
                    }
                    '/' => {
                        p.rect_stroke(b, 4.0, Stroke::new(1.3, accent), egui::StrokeKind::Inside);
                        p.rect_filled(Rect::from_min_max(Pos2::new(b.left() + 3.5, b.center().y - 1.0), Pos2::new(b.right() - 3.5, b.center().y + 1.0)), 1.0, accent);
                    }
                    _ => {
                        p.rect_stroke(b, 4.0, Stroke::new(1.3, line), egui::StrokeKind::Inside);
                    }
                }
                x += 26.0;
            } else if !row.icon.is_empty() {
                let ir = Rect::from_center_size(Pos2::new(x + 9.0, r.center().y), Vec2::splat(18.0));
                if let Some(t) = kit.named_icon(ui.ctx(), &row.icon, 18.0) {
                    p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                x += 28.0;
            }
            let room = r.right() - 10.0 - x;
            let main = if row.quiet { v.weak_text_color() } else { v.text_color() };
            let size = if row.quiet { 12.5 } else { 13.5 };
            if row.sub.is_empty() {
                let g = one_line(&p, &row.label, ui_font(size), main, room);
                p.galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g, main);
            } else {
                let g = one_line(&p, &row.label, ui_font(size), main, room);
                p.galley(Pos2::new(x, r.top() + 5.0), g, main);
                let col = match row.state {
                    crate::plugins::State::Normal => v.weak_text_color(),
                    crate::plugins::State::Accent => accent,
                    crate::plugins::State::Warning => sc.warning,
                    crate::plugins::State::Danger => sc.danger,
                };
                let g = one_line(&p, &row.sub, ui_font(11.5), col, room);
                p.galley(Pos2::new(x, r.top() + 24.0), g, col);
            }
            if acts && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                m.acts.push(Act::Plugin(id.to_string(), crate::plugins::Event::new("menu", &[("id", &row.id)])));
                m.acts.push(Act::Close);
            }
        }
        if item.menu.is_empty() {
            ui.label(egui::RichText::new("Nothing to show").color(v.weak_text_color()));
        }
    });
}

/// A popup over the whole screen: its card, and a click anywhere else closes it.
pub fn popup(ui: &mut egui::Ui, pop: &mut Popup, m: &mut Model, kit: &mut Kit) {
    // The boards take the whole screen: no card, and their own way of closing.
    match &mut pop.kind {
        PopupKind::Hud { edit, query } => return crate::board::hud(ui, edit, query, m, kit),
        PopupKind::WidgetEdit => return crate::board::desktop_edit(ui, m, kit),
        _ => {}
    }
    let v = ui.visuals().clone();
    let full = ui.max_rect();
    let top_room = crate::panels::room(&m.s.panels, m.output, Edge::Top);
    let bottom_room = crate::panels::room(&m.s.panels, m.output, Edge::Bottom);
    // The launcher set to open from a panel, opened by a key: from the screen's first panel.
    let is_launcher = matches!(pop.kind, PopupKind::Launcher { .. });
    if is_launcher
        && m.s.launch_place == crate::config::LaunchPlace::Panel
        && pop.from.is_none()
        && let Some((idx, from, attach, at)) = crate::launcher::home(m.s, m.output)
    {
        (pop.from, pop.attach, pop.anchor_x) = (Some(from), attach.then_some(idx), at);
    }
    // Else (or with no panel on this screen) it's in the middle.
    let centred_launcher = is_launcher && (m.s.launch_place == crate::config::LaunchPlace::Center || pop.from.is_none());
    // Where it opens: under (or over) the panel it came from; else under the top panels, or
    // over the bottom ones when there are only those.
    let (edge, inset) = pop.from.unwrap_or(if top_room == 0.0 && bottom_room > 0.0 { (Edge::Bottom, bottom_room + 4.0) } else { (Edge::Top, top_room + if top_room > 0.0 { 4.0 } else { MARGIN }) });
    let below = top_room + if top_room > 0.0 { 4.0 } else { MARGIN };
    // Clear of the side panels. From a side panel a popup opens beside it, with the room
    // between the top and bottom panels for its height.
    let left = full.left() + crate::panels::room(&m.s.panels, m.output, Edge::Left) + MARGIN;
    let right = full.right() - crate::panels::room(&m.s.panels, m.output, Edge::Right) - MARGIN;
    let beside = edge.vertical();
    let (edge_in, inset) = if beside { (inset, top_room + bottom_room + MARGIN) } else { (0.0, inset) };
    // Esc while a Wi-Fi password is being typed only stops that.
    let esc = ui.input(|i| i.key_pressed(Key::Escape)) && !matches!(pop.kind, PopupKind::Network { ask: Some(_) });
    let size = match &pop.kind {
        PopupKind::Launcher { .. } => {
            let size = crate::launcher::size(m.s, full);
            if centred_launcher { size } else { Vec2::new(size.x, size.y.min(full.height() - inset - MARGIN - 8.0)) }
        }
        PopupKind::Calendar { month, day } => Vec2::new(300.0, calendar_height(m, *month, *day)),
        PopupKind::Power { .. } => crate::power_ui::size(m.s),
        PopupKind::Control { .. } => Vec2::new(408.0, crate::quick::height(m, ui.ctx()).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Downloads => Vec2::new(crate::places_pop::WIDTH, crate::places_pop::height(crate::places_pop::newest().len()) + 12.0),
        PopupKind::TrashMenu { confirm } => Vec2::new(if *confirm { 280.0 } else { 220.0 }, crate::places_pop::trash_height(*confirm)),
        PopupKind::DockMenu { app, pinned } => Vec2::new(220.0, if *pinned { 190.0 } else { 116.0 } + if closable(m, app).is_empty() { 0.0 } else { 34.0 }),
        PopupKind::Network { .. } | PopupKind::Bluetooth => Vec2::new(340.0, crate::conn::height(&pop.kind, m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Ai => Vec2::new(340.0, crate::ai_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Weather => Vec2::new(340.0, crate::weather_ui::popup_height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Notifications => Vec2::new(crate::notes_pop::WIDTH, crate::notes_pop::height(m, ui.ctx()).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Clipboard { .. } => Vec2::new(crate::clip_pop::WIDTH, crate::clip_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Sound { .. } => Vec2::new(360.0, crate::sound_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Plugin { id } => Vec2::new(PLUGIN_W, plugin_menu_height(m, id).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::TrayMenu { path, .. } => Vec2::new(260.0, crate::tray_ui::menu_height(m, path).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Auth { .. } => Vec2::new(crate::auth_pop::WIDTH, crate::auth_pop::height(m, ui.ctx())),
        PopupKind::Media => Vec2::new(crate::media_pop::WIDTH, crate::media_pop::height(m)),
        PopupKind::Hud { .. } | PopupKind::WidgetEdit => Vec2::ZERO,
    };
    let mut anchored = false;
    // Joined to a panel, it stays along the straight part of the panel's side (clear of
    // rounded corners), where it fits.
    let straight = pop.attach.and_then(|i| {
        let c = m.s.panels.get(i)?;
        let area = if c.vertical() { Rect::from_min_max(Pos2::new(full.left(), full.top() + top_room), Pos2::new(full.right(), full.bottom() - bottom_room)) } else { full };
        let bar = crate::panels::bar_rect(&m.s.panels, i, area);
        // Its own flare into the panel takes the look's radius again.
        let r = crate::style::radius(&m.s.panel_look(i)).nw as f32;
        let rad = if c.floating { r * 2.0 } else { r };
        Some(if c.vertical() { (bar.top() + rad, bar.bottom() - rad) } else { (bar.left() + rad, bar.right() - rad) })
    });
    let rect = match pop.kind {
        PopupKind::Launcher { .. } if centred_launcher => {
            // A soft dim behind the launcher.
            if m.s.launch_dim {
                ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(70));
            }
            Rect::from_min_size(Pos2::new(full.center().x - size.x / 2.0, full.top() + (full.height() * 0.16).max(below + 10.0)), size)
        }
        PopupKind::Power { .. } if crate::power_ui::centred(m.s) => {
            ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(90));
            Rect::from_center_size(full.center(), size)
        }
        PopupKind::Auth { .. } => {
            ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(110));
            Rect::from_center_size(full.center(), size)
        }
        PopupKind::DockMenu { .. } | PopupKind::Downloads | PopupKind::TrashMenu { .. } if pop.from.is_none() => {
            // Above the dock, over the icon.
            let items = dock_items(m.s, m.apps, &m.status.running);
            let (_, room) = dock_size(m.s, &items);
            let x = (full.center().x + pop.anchor_x - size.x / 2.0).clamp(full.left() + MARGIN, full.right() - MARGIN - size.x);
            let bottom = full.bottom() - room - 8.0 - bottom_room;
            Rect::from_min_size(Pos2::new(x, bottom - size.y), size)
        }
        _ => {
            // No button to open under (asked by `rivet-shell msg`): the middle.
            anchored = true;
            let ax = if pop.anchor_x.is_nan() { full.center().x } else { pop.anchor_x };
            if beside {
                let x = if edge == Edge::Left { full.left() + edge_in } else { full.right() - edge_in - size.x };
                let (lo, hi) = (full.top() + top_room + MARGIN, full.bottom() - bottom_room - MARGIN - size.y);
                let ay = if pop.anchor_x.is_nan() { full.center().y } else { pop.anchor_x };
                let (lo, hi) = straight.filter(|(a, b)| b - a >= size.y).map(|(a, b)| (a, b - size.y)).unwrap_or((lo, hi));
                Rect::from_min_size(Pos2::new(x, (ay - size.y / 2.0).clamp(lo, hi.max(lo)).round()), size)
            } else {
                let (left, right) = straight.filter(|(a, b)| b - a >= size.x).unwrap_or((left, right));
                let x = (ax - size.x / 2.0).clamp(left, (right - size.x).max(left)).round();
                let y = if edge == Edge::Top { full.top() + inset } else { full.bottom() - inset - size.y };
                Rect::from_min_size(Pos2::new(x, y), size)
            }
        }
    };
    // Outside the card: close.
    let back = ui.interact(full, egui::Id::unique("popup-back"), Sense::click());
    let outside = ui.input(|i| i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
    // The password dialog stays through a click outside it (Esc or Cancel turns it away).
    let stays = matches!(pop.kind, PopupKind::Auth { .. });
    if (back.clicked() || back.secondary_clicked()) && outside && !stays || esc {
        m.acts.push(Act::Close);
    }
    match pop.attach.filter(|_| anchored && pop.from.is_some()) {
        Some(idx) => {
            // Joined to its panel: in the panel's colours and as see-through, square where they meet, and no
            // shadow cast back over the panel.
            let look = m.s.panel_look(idx);
            let (pv, _) = crate::style::visuals(&v, &look, &crate::style::Roles::new(&kit.look, m.s));
            let flare = crate::style::radius(&look).nw as f32;
            joined(ui.painter(), full, rect, edge, &pv, &look, flare);
            *ui.visuals_mut() = pv;
            pop.join = Some(if beside { (rect.top() - flare, rect.bottom() + flare) } else { (rect.left() - flare, rect.right() + flare) });
        }
        None => {
            pop.join = None;
            card(ui.painter(), rect, &v, 0.98, r_lg());
        }
    }
    let pad = match pop.kind {
        PopupKind::DockMenu { .. } | PopupKind::TrayMenu { .. } | PopupKind::Downloads | PopupKind::TrashMenu { .. } => 6.0,
        PopupKind::Control { .. } => crate::quick::PAD,
        PopupKind::Notifications => crate::notes_pop::PAD,
        PopupKind::Clipboard { .. } => crate::clip_pop::PAD,
        PopupKind::Auth { .. } => crate::auth_pop::PAD,
        PopupKind::Media => crate::media_pop::PAD,
        _ => 14.0,
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(pad)).layout(egui::Layout::top_down(egui::Align::Min)));
    let focus = std::mem::take(&mut pop.focus);
    match &mut pop.kind {
        PopupKind::Launcher { query, sel } => crate::launcher::launcher(&mut child, query, sel, focus, m, kit),
        PopupKind::Calendar { month, day } => calendar(&mut child, month, day, kit.accent(m.s), m),
        PopupKind::Power { confirm } => crate::power_ui::menu(&mut child, confirm, m),
        PopupKind::Control { drag } => crate::quick::quick_settings(&mut child, drag, m, kit),
        PopupKind::DockMenu { app, pinned } => dock_menu(&mut child, app, *pinned, m),
        PopupKind::Downloads => crate::places_pop::downloads(&mut child, m, kit),
        PopupKind::TrashMenu { confirm } => crate::places_pop::trash_menu(&mut child, confirm, m),
        PopupKind::Network { ask } => crate::conn::network(&mut child, ask, m, kit),
        PopupKind::Bluetooth => crate::conn::bluetooth(&mut child, m, kit),
        PopupKind::Ai => crate::ai_pop::popup(&mut child, m, kit),
        PopupKind::Weather => crate::weather_ui::popup(&mut child, m, kit),
        PopupKind::Notifications => crate::notes_pop::popup(&mut child, m, kit),
        PopupKind::Clipboard { sel } => crate::clip_pop::popup(&mut child, sel, m, kit),
        PopupKind::Sound { drag } => crate::sound_pop::popup(&mut child, drag, m, kit),
        PopupKind::TrayMenu { key, path } => crate::tray_ui::menu(&mut child, key, path, m),
        PopupKind::Plugin { id } => plugin_menu(&mut child, id, m, kit),
        PopupKind::Auth { typed, round } => crate::auth_pop::popup(&mut child, typed, round, m, kit),
        PopupKind::Media => crate::media_pop::popup(&mut child, m, kit),
        PopupKind::Hud { .. } | PopupKind::WidgetEdit => {}
    }
}

/// A simple calculator for the launcher: + − × ÷ % ^ and brackets.
pub fn calc(q: &str) -> Option<f64> {
    let q = q.trim().trim_start_matches('=');
    if !q.chars().any(|c| c.is_ascii_digit()) || !q.chars().any(|c| "+-*/%^×÷x(".contains(c)) || q.chars().any(|c| c.is_alphabetic() && c != 'x') {
        return None;
    }
    let toks: Vec<char> = q
        .chars()
        .filter(|c| !c.is_whitespace() && *c != ',')
        .map(|c| match c {
            '×' | 'x' => '*',
            '÷' => '/',
            c => c,
        })
        .collect();
    struct P {
        t: Vec<char>,
        i: usize,
    }
    impl P {
        fn peek(&self) -> Option<char> {
            self.t.get(self.i).copied()
        }
        fn sum(&mut self) -> Option<f64> {
            let mut v = self.product()?;
            while let Some(c @ ('+' | '-')) = self.peek() {
                self.i += 1;
                let r = self.product()?;
                v = if c == '+' { v + r } else { v - r };
            }
            Some(v)
        }
        fn product(&mut self) -> Option<f64> {
            let mut v = self.power()?;
            while let Some(c @ ('*' | '/' | '%')) = self.peek() {
                self.i += 1;
                let r = self.power()?;
                v = match c {
                    '*' => v * r,
                    '/' => v / r,
                    _ => v % r,
                };
            }
            Some(v)
        }
        fn power(&mut self) -> Option<f64> {
            let b = self.unary()?;
            if self.peek() == Some('^') {
                self.i += 1;
                let e = self.power()?;
                return Some(b.powf(e));
            }
            Some(b)
        }
        fn unary(&mut self) -> Option<f64> {
            match self.peek()? {
                '-' => {
                    self.i += 1;
                    Some(-self.unary()?)
                }
                '+' => {
                    self.i += 1;
                    self.unary()
                }
                '(' => {
                    self.i += 1;
                    let v = self.sum()?;
                    if self.peek() == Some(')') {
                        self.i += 1;
                    }
                    Some(v)
                }
                _ => {
                    let start = self.i;
                    while self.peek().is_some_and(|c| c.is_ascii_digit() || c == '.') {
                        self.i += 1;
                    }
                    self.t[start..self.i].iter().collect::<String>().parse().ok()
                }
            }
        }
    }
    let mut p = P { t: toks, i: 0 };
    let v = p.sum()?;
    (p.i == p.t.len() && v.is_finite()).then_some(v)
}

pub fn calc_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The first day of the month `month` months from this one.
fn calendar_month(month: i32) -> NaiveDate {
    let today = Local::now().date_naive();
    let base = today.with_day(1).unwrap_or(today);
    let total = base.year() * 12 + base.month0() as i32 + month;
    NaiveDate::from_ymd_opt(total.div_euclid(12), total.rem_euclid(12) as u32 + 1, 1).unwrap_or(base)
}

/// The days a month's grid shows: from the Monday on or before its first, six weeks.
pub fn calendar_span(month: i32) -> (NaiveDate, NaiveDate) {
    let first = calendar_month(month);
    let start = first - chrono::Duration::days(first.weekday().num_days_from_monday() as i64);
    (start, start + chrono::Duration::days(42))
}

/// The events plugins gave for a day, with the plugin each came from.
fn day_events<'a>(m: &'a Model, day: NaiveDate) -> Vec<(&'a str, &'a crate::plugins::DayEvent)> {
    let mut out: Vec<(&str, &crate::plugins::DayEvent)> = m.extra.plugins.iter().flat_map(|(id, it)| it.events.iter().filter(move |e| e.day == Some(day)).map(move |e| (id.as_str(), e))).collect();
    // Whole days first, then by time.
    out.sort_by(|a, b| (!a.1.time.is_empty(), &a.1.time).cmp(&(!b.1.time.is_empty(), &b.1.time)));
    out
}

/// Some plugin gives the calendar events (else there's no list of a day's).
pub(crate) fn has_calendar(m: &Model) -> bool {
    m.extra.plugins.iter().any(|(_, it)| it.calendar)
}

/// A day's list shows this many events, then how many more.
const DAY_ROWS: usize = 5;

/// The calendar popup's height: the month, and the picked day's events under it.
fn calendar_height(m: &Model, month: i32, day: Option<NaiveDate>) -> f32 {
    let first = calendar_month(month);
    let weeks = ((first.weekday().num_days_from_monday() as i64 + days_in_month(first)) as f32 / 7.0).ceil();
    // The popup's padding, the month's name and the room under it, the days' names, the weeks.
    let grid = 14.0 * 2.0 + 24.0 + 8.0 + 24.0 + weeks * 30.0;
    if !has_calendar(m) {
        return 300.0;
    }
    let today = Local::now().date_naive();
    let n = day_events(m, day.unwrap_or(today)).len();
    let rows = n.clamp(1, DAY_ROWS) as f32 + if n > DAY_ROWS { 1.0 } else { 0.0 };
    grid + 16.0 + 26.0 + rows * 30.0 + 4.0
}

fn days_in_month(first: NaiveDate) -> i64 {
    let (y, mo) = (first.year(), first.month());
    NaiveDate::from_ymd_opt(if mo == 12 { y + 1 } else { y }, if mo == 12 { 1 } else { mo + 1 }, 1).map(|n| (n - first).num_days()).unwrap_or(30)
}

/// The month, with a dot under each day that has events (in their calendars' colours), and the
/// picked day's events under it, from the plugins that give them. Clicking one opens it in its
/// app; a day picks it.
pub(crate) fn calendar(ui: &mut egui::Ui, month: &mut i32, picked: &mut Option<NaiveDate>, accent: Color32, m: &mut Model) {
    let v = ui.visuals().clone();
    // Rows as tall as they're made, so calendar_height adds up.
    ui.spacing_mut().item_spacing.y = 0.0;
    let today = Local::now().date_naive();
    let was = *month;
    let first = calendar_month(*month);
    ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 24.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        if ui.small_button("‹").clicked() {
            *month -= 1;
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("›").clicked() {
                *month += 1;
            }
            ui.centered_and_justified(|ui| {
                let t = first.format("%B %Y").to_string();
                if ui.add(egui::Label::new(egui::RichText::new(t).strong().size(15.0)).sense(Sense::click())).on_hover_text("Back to this month").clicked() {
                    *month = 0;
                    *picked = None;
                }
            });
        });
    });
    ui.add_space(8.0);
    let r = ui.available_rect_before_wrap();
    let cw = r.width() / 7.0;
    let ch = 30.0;
    for (i, d) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
        ui.painter().text(Pos2::new(r.left() + cw * (i as f32 + 0.5), r.top() + 10.0), Align2::CENTER_CENTER, *d, ui_font(11.5), v.weak_text_color());
    }
    let calendar = has_calendar(m);
    let shown = picked.unwrap_or(today);
    let offset = first.weekday().num_days_from_monday() as i64;
    let days = days_in_month(first);
    for d in 0..days {
        let cell = d + offset;
        let c = Pos2::new(r.left() + cw * ((cell % 7) as f32 + 0.5), r.top() + 34.0 + ch * (cell / 7) as f32);
        let date = first + chrono::Duration::days(d);
        let hit = Rect::from_center_size(c, Vec2::new(cw.min(34.0), ch - 2.0));
        let resp = ui.interact(hit, egui::Id::unique(("calendar-day", date)), if calendar { Sense::click() } else { Sense::hover() });
        let p = ui.painter();
        let col = if date == today {
            p.circle_filled(c, 13.0, accent);
            on(accent)
        } else {
            if calendar && date == shown {
                p.circle_filled(c, 13.0, mix(v.panel_fill, v.text_color(), 0.14));
            } else if resp.hovered() {
                p.circle_filled(c, 13.0, mix(v.panel_fill, v.text_color(), 0.07));
            }
            if (cell % 7) >= 5 { v.weak_text_color() } else { v.text_color() }
        };
        p.text(c, Align2::CENTER_CENTER, (d + 1).to_string(), ui_font(13.0), col);
        // Its events: a dot for each calendar they're in, up to three.
        if calendar {
            let mut colors: Vec<Color32> = Vec::new();
            for (_, e) in day_events(m, date) {
                let c = e.color.map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(accent);
                if !colors.contains(&c) {
                    colors.push(c);
                }
            }
            let n = colors.len().min(3);
            for (k, dc) in colors.iter().take(3).enumerate() {
                let x = c.x + (k as f32 - (n as f32 - 1.0) / 2.0) * 5.0;
                p.circle_filled(Pos2::new(x, c.y + 15.5), 1.8, *dc);
            }
        }
        if resp.on_hover_cursor(if calendar { egui::CursorIcon::PointingHand } else { egui::CursorIcon::Default }).clicked() {
            *picked = Some(date);
        }
    }
    let weeks = ((offset + days) as f32 / 7.0).ceil();
    // A month further on or back: plugins are told which days it shows, and nothing is picked.
    if *month != was {
        *picked = None;
        let (a, b) = calendar_span(*month);
        m.acts.push(Act::CalendarSpan(a, b));
    }
    if !calendar {
        return;
    }
    // ---- the picked day's events
    ui.allocate_exact_size(Vec2::new(r.width(), 24.0 + ch * weeks), Sense::hover());
    let (line, _) = ui.allocate_exact_size(Vec2::new(r.width(), 16.0), Sense::hover());
    ui.painter().hline(line.x_range(), line.center().y, Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08)));
    let head = if shown == today { "Today".to_string() } else { shown.format("%A %-d %B").to_string() };
    let (hr, _) = ui.allocate_exact_size(Vec2::new(r.width(), 26.0), Sense::hover());
    ui.painter().text(Pos2::new(hr.left() + 2.0, hr.center().y), Align2::LEFT_CENTER, head, ui_font(12.5), v.weak_text_color());
    let list = day_events(m, shown);
    if list.is_empty() {
        let (er, _) = ui.allocate_exact_size(Vec2::new(r.width(), 30.0), Sense::hover());
        ui.painter().text(Pos2::new(er.left() + 2.0, er.center().y), Align2::LEFT_CENTER, "Nothing on this day", ui_font(13.0), v.weak_text_color());
        return;
    }
    let mut open: Option<(String, String)> = None;
    for (pid, e) in list.iter().take(DAY_ROWS) {
        let (row, resp) = ui.allocate_exact_size(Vec2::new(r.width(), 30.0), Sense::click());
        if resp.hovered() {
            hover_fill(ui.painter(), row, &v);
        }
        let c = e.color.map(|[r, g, b]| Color32::from_rgb(r, g, b)).unwrap_or(accent);
        ui.painter().rect_filled(Rect::from_min_size(Pos2::new(row.left() + 4.0, row.top() + 7.0), Vec2::new(3.0, row.height() - 14.0)), 1.5, c);
        let time = if e.time.is_empty() { "All day" } else { e.time.as_str() };
        let tg = ui.painter().layout_no_wrap(time.to_string(), ui_font(11.5), v.weak_text_color());
        ui.painter().galley(Pos2::new(row.left() + 14.0, row.center().y - tg.size().y / 2.0), tg, v.weak_text_color());
        let g = one_line(ui.painter(), &e.title, ui_font(13.0), v.text_color(), row.width() - 84.0);
        ui.painter().galley(Pos2::new(row.left() + 72.0, row.center().y - g.size().y / 2.0), g, v.text_color());
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            open = Some((pid.to_string(), e.id.clone()));
        }
    }
    if list.len() > DAY_ROWS {
        let (mr, _) = ui.allocate_exact_size(Vec2::new(r.width(), 30.0), Sense::hover());
        ui.painter().text(Pos2::new(mr.left() + 14.0, mr.center().y), Align2::LEFT_CENTER, format!("{} more", list.len() - DAY_ROWS), ui_font(12.5), v.weak_text_color());
    }
    if let Some((pid, id)) = open {
        m.acts.push(Act::Close);
        m.acts.push(Act::Plugin(pid, crate::plugins::Event::new("open", &[("id", &id)])));
    }
}

/// Right-click on a dock icon: open it, move it, pin or unpin it, close its windows.
fn dock_menu(ui: &mut egui::Ui, app: &str, pinned: bool, m: &mut Model) {
    let v = ui.visuals().clone();
    let name = find_app(m.apps, app).map(|a| a.name.clone()).unwrap_or_else(|| app.to_string());
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let g = one_line(ui.painter(), &name, ui_font(12.5), v.weak_text_color(), r.width() - 20.0);
    ui.painter().galley(Pos2::new(r.left() + 10.0, r.center().y - g.size().y / 2.0), g, v.weak_text_color());
    let at = m.s.dock_apps.iter().position(|d| find_app(m.apps, d).is_some_and(|a| a.id.trim_end_matches(".desktop") == app));
    let last = m.s.dock_apps.len().saturating_sub(1);
    let mut items: Vec<(&str, Act, bool)> = vec![("Open", Act::Launch(app.to_string()), true)];
    if pinned {
        items.push(("Move Left", Act::MovePin(app.to_string(), -1), at.is_some_and(|i| i > 0)));
        items.push(("Move Right", Act::MovePin(app.to_string(), 1), at.is_some_and(|i| i < last)));
        items.push(("Unpin from Dock", Act::Unpin(app.to_string()), true));
    } else {
        items.push(("Pin to Dock", Act::Pin(app.to_string()), true));
    }
    let open = closable(m, app);
    if !open.is_empty() {
        items.push(("Close", Act::CloseWindows(open), true));
    }
    for (label, act, enabled) in items {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), if enabled { Sense::click() } else { Sense::hover() });
        if enabled && resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let col = if enabled { v.text_color() } else { v.weak_text_color().gamma_multiply(0.6) };
        ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, ui_font(13.5), col);
        if enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::Close);
            m.acts.push(act);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glass_follows_the_corners_inside_the_shape() {
        let r = Rect::from_min_size(Pos2::new(10.0, 20.0), Vec2::new(200.0, 40.0));
        assert_eq!(glass_rects(r, egui::CornerRadius::ZERO, r), vec![[10, 20, 200, 40]]);
        let rects = glass_rects(r, egui::CornerRadius::same(12), r);
        // Every row of the shape is covered once, and nothing reaches past it.
        let rows: i32 = rects.iter().map(|q| q[3]).sum();
        assert_eq!(rows, 40);
        assert!(rects.iter().all(|q| q[0] >= 10 && q[0] + q[2] <= 210 && q[1] >= 20 && q[1] + q[3] <= 60));
        // The top row is cut in at the corners; the middle isn't.
        assert!(rects.iter().find(|q| q[1] == 20).is_some_and(|q| q[0] > 10));
        assert!(rects.iter().any(|q| q[0] == 10 && q[2] == 200));
        // Sliding off the bottom, with only its top five points showing: nothing is asked for
        // outside those, and the cut edge isn't rounded.
        let shown = Rect::from_min_max(r.min, egui::pos2(210.0, 25.0));
        let rects = glass_rects(r, egui::CornerRadius::same(12), shown);
        assert!(!rects.is_empty() && rects.iter().all(|q| q[1] >= 20 && q[1] + q[3] <= 25));
        let half = Rect::from_min_max(r.min, egui::pos2(210.0, 45.0));
        assert!(glass_rects(r, egui::CornerRadius::same(12), half).iter().any(|q| q[1] + q[3] == 45 && q[0] == 10 && q[2] == 200));
    }

    /// A pinned icon pressed, carried past the others and let go asks for its new place; a
    /// press let go where it was is still a click.
    #[test]
    fn a_pin_dragged_along_the_dock_is_placed() {
        let apps = [app("a.desktop", "A"), app("b.desktop", "B"), app("c.desktop", "C")];
        let s = Settings { dock_apps: vec!["a".into(), "b".into(), "c".into()], dock_places: false, ..Settings::default() };
        let (status, extra) = (Status::default(), Extra::default());
        let (size, _) = dock_size(&s, &dock_items(&s, &apps, &[]));
        let full = Rect::from_min_size(Pos2::ZERO, size);
        let (_, slots, _) = dock_layout(&s, &apps, &[], full);
        let (first, last) = (slots[0].1.center(), slots[2].1.center());
        let ctx = egui::Context::default();
        let mut kit = Kit::new(false);
        let mut time = 0.0;
        let mut frame = |events: Vec<egui::Event>| {
            let mut acts = Vec::new();
            time += 0.05;
            let raw = egui::RawInput { screen_rect: Some(full), time: Some(time), events, ..Default::default() };
            let mut out = ctx.run_ui(raw, |ui| {
                kit.begin(ui.ctx(), &s);
                let mut m = Model { s: &s, status: &status, extra: &extra, apps: &apps, compositor: Compositor::Other, output: "", acts: &mut acts };
                dock(ui, &mut m, &mut kit);
            });
            out.textures_delta.clear();
            acts
        };
        let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(vec![egui::Event::PointerMoved(first)]);
        frame(vec![button(first, true)]);
        // Over to the last icon, a step at a time, and a little past its middle.
        for k in 1..=6 {
            let at = first + (last - first) * (k as f32 / 6.0) + Vec2::new(if k == 6 { 4.0 } else { 0.0 }, 0.0);
            assert!(frame(vec![egui::Event::PointerMoved(at)]).is_empty());
        }
        let acts = frame(vec![button(last + Vec2::new(4.0, 0.0), false)]);
        assert!(matches!(acts.as_slice(), [Act::PlacePin(id, 2)] if id == "a.desktop"), "{acts:?}");
        // Let go: nothing is held any more, and a plain press and release opens the app.
        assert!(frame(vec![egui::Event::PointerMoved(first)]).is_empty());
        frame(vec![button(first, true)]);
        let acts = frame(vec![button(first, false)]);
        assert!(matches!(acts.as_slice(), [Act::Launch(id)] if id == "a.desktop"));
        // Carried off the end of the dock, where the release never arrives: it is put down in
        // the place it had reached, not left in the hand.
        frame(vec![egui::Event::PointerMoved(last)]);
        frame(vec![button(last, true)]);
        for k in 1..=6 {
            frame(vec![egui::Event::PointerMoved(last + (first - last) * (k as f32 / 6.0) - Vec2::new(if k == 6 { 4.0 } else { 0.0 }, 0.0))]);
        }
        let acts = frame(vec![egui::Event::PointerGone]);
        assert!(matches!(acts.as_slice(), [Act::PlacePin(id, 0)] if id == "c.desktop"), "{acts:?}");
        assert!(frame(vec![egui::Event::PointerMoved(first)]).is_empty());
        assert!(frame(vec![egui::Event::PointerMoved(last)]).is_empty());
    }

    fn app(id: &str, name: &str) -> DesktopApp {
        DesktopApp { id: id.into(), name: name.into(), exec: String::new(), icon: String::new(), mimes: Vec::new(), terminal: false }
    }

    #[test]
    fn finds_apps_and_sizes() {
        let apps = [app("org.mozilla.firefox.desktop", "Firefox"), app("rivet-files.desktop", "Rivet Files"), app("kitty.desktop", "kitty")];
        assert_eq!(find_app(&apps, "firefox").map(|a| a.name.as_str()), Some("Firefox"));
        assert_eq!(find_app(&apps, "rivet-files").map(|a| a.name.as_str()), Some("Rivet Files"));
        assert!(find_app(&apps, "nope").is_none());
        // Dragged along the dock: places count the pins shown, and one not installed stays put.
        let abc = [app("a.desktop", "A"), app("b.desktop", "B"), app("c.desktop", "C")];
        let mut pins: Vec<String> = ["a", "gone", "b", "c"].map(String::from).to_vec();
        place_pin(&mut pins, &abc, "a.desktop", 2);
        assert_eq!(pins, ["gone", "b", "c", "a"]);
        place_pin(&mut pins, &abc, "c.desktop", 0);
        assert_eq!(pins, ["gone", "c", "b", "a"]);
        place_pin(&mut pins, &abc, "b.desktop", 1);
        assert_eq!(pins, ["gone", "c", "b", "a"]);
        let s = Settings { dock_apps: vec!["firefox".into(), "rivet-files".into()], ..Settings::default() };
        // Open apps that aren't pinned come after the pinned ones, once each.
        let items = dock_items(&s, &apps, &["kitty".into(), "firefox".into()]);
        assert_eq!(items.iter().map(|(a, p)| (a.name.as_str(), *p)).collect::<Vec<_>>(), [("Firefox", true), ("Rivet Files", true), ("kitty", false)]);
        let (size, room) = dock_size(&s, &items);
        assert!(size.x > 3.0 * s.dock_icon + DOCK_SEP && room < size.y);
    }

    #[test]
    fn sums() {
        assert_eq!(calc("2+2"), Some(4.0));
        assert_eq!(calc("= (1 + 2) * 3 ^ 2"), Some(27.0));
        assert_eq!(calc("10 / 4"), Some(2.5));
        assert_eq!(calc("-3 × 2"), Some(-6.0));
        assert_eq!(calc("firefox"), None);
        assert_eq!(calc("42"), None);
        assert_eq!(calc("2+"), None);
        assert_eq!(calc_text(2.5), "2.5");
        assert_eq!(calc_text(4.0), "4");
    }
}
