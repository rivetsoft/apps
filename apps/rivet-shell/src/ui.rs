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

use crate::config::{Edge, Settings, Side};
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

/// The dock surface's size, and the room it keeps from windows.
pub fn dock_size(s: &Settings, items: &[(DesktopApp, bool)]) -> (Vec2, f32) {
    let n = items.len().max(1) as f32;
    let sep = if items.iter().any(|i| i.1) && items.iter().any(|i| !i.1) { DOCK_SEP } else { 0.0 };
    let w = DOCK_PAD * 2.0 + n * s.dock_icon + (n - 1.0) * DOCK_GAP + sep + MARGIN * 2.0;
    let body = s.dock_icon + DOCK_PAD * 2.0 + MARGIN;
    (Vec2::new(w, body + DOCK_LABEL), body)
}

#[derive(Clone, Debug, PartialEq)]
pub enum PopupKind {
    Launcher { query: String, sel: usize },
    Calendar { month: i32 },
    Power { confirm: Option<Power> },
    /// Quick settings, sliders, what's playing and the notifications (`drag`: a slider held).
    Control { drag: Option<(u8, u32)> },
    /// Right-click on a dock icon.
    DockMenu { app: String, pinned: bool },
    /// Wi-Fi networks (`ask`: the network a password is being typed for, and the password).
    Network { ask: Option<(String, String)> },
    /// Bluetooth devices.
    Bluetooth,
    /// AI plans' usage.
    Ai,
    /// The notification history (and later perhaps more beside it).
    Notifications,
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
        matches!(self.kind, PopupKind::Launcher { .. } | PopupKind::Network { .. } | PopupKind::Auth { .. })
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
}

/// What the person asked for, done by whoever runs the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Launch(String),
    /// A desktop widget dragged to a new place (a fraction of the room it moves in), or put
    /// back in its usual place (None).
    MoveWidget(Widget, Option<(f32, f32)>),
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
    /// Join, leave, pair, forget… (done on the radios' thread).
    Radio(crate::radio::Job),
    /// Read the AI plans' usage again now.
    AiRefresh,
    /// Pin a limit to the panel or take it off ("provider:limit name").
    AiPin(String),
    NightLight(bool),
    Dnd(bool),
    KeepAwake(bool),
    NextProfile,
    DarkStyle(bool),
    Pin(String),
    Unpin(String),
    MovePin(String, i32),
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
    /// Something for a tray icon's app (a click, its menu…).
    Tray(crate::tray::Job),
    /// Something for a plugin: a click on its item, a row of its menu.
    Plugin(String, crate::plugins::Event),
    /// What was typed into the password dialog.
    Auth(String),
    /// Jump to this many seconds into what's playing.
    MediaSeek(u32),
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
    live: bool,
}

impl Kit {
    pub fn new(live: bool) -> Kit {
        Kit { look: LiveLook::new(), icons: IconTheme::new(&rivet_look::icons::detect_system_theme()), icon_key: String::new(), tray_tex: Default::default(), face: Face::Base, live }
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
    let items = dock_items(m.s, m.apps, &m.status.running);
    let size = m.s.dock_icon;
    let body_h = size + DOCK_PAD * 2.0;
    let body = Rect::from_min_size(Pos2::new(full.left() + MARGIN, full.bottom() - MARGIN - body_h), Vec2::new(full.width() - MARGIN * 2.0, body_h));
    surface(ui.painter(), body, &v, look.opacity, crate::style::radius(&look), look.border);
    let p = ui.painter().clone();
    let mut x = body.left() + DOCK_PAD;
    let mut label: Option<(String, f32)> = None;
    let mut after_pinned = false;
    for (a, pinned) in &items {
        if !pinned && !after_pinned {
            after_pinned = true;
            if items.iter().any(|i| i.1) {
                // Between the pinned apps and the open ones.
                let sx = x + DOCK_SEP / 2.0 - DOCK_GAP / 2.0;
                p.line_segment([Pos2::new(sx, body.top() + 12.0), Pos2::new(sx, body.bottom() - 12.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.2)));
                x += DOCK_SEP;
            }
        }
        let r = Rect::from_min_size(Pos2::new(x, body.top() + DOCK_PAD), Vec2::splat(size));
        let resp = ui.interact(r.expand(2.0), egui::Id::unique(("dock", &a.id)), Sense::click());
        let lift = if resp.hovered() { -4.0 } else { 0.0 };
        let r2 = r.translate(Vec2::new(0.0, lift));
        if resp.hovered() {
            p.rect_filled(r.expand(3.0), hover_r, mix(v.panel_fill, v.text_color(), 0.08));
            label = Some((a.name.clone(), r.center().x));
        }
        match kit.app_icon(ui.ctx(), a, size) {
            Some(t) => {
                p.image(t.id(), r2, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                p.rect_filled(r2.shrink(4.0), r_md(), mix(accent, v.panel_fill, 0.3));
                p.text(r2.center(), Align2::CENTER_CENTER, a.name.chars().next().unwrap_or('?').to_uppercase().to_string(), ui_font(size * 0.45), Color32::WHITE);
            }
        }
        // Open: a dot under it.
        if m.s.dock_running && (!pinned || is_running(a, &m.status.running)) {
            p.circle_filled(Pos2::new(r.center().x, body.bottom() - 4.0), 2.5, dot);
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.secondary_clicked() {
            // The dock is centred on its screen: the menu's place is the icon's from the middle.
            m.acts.push(Act::Open(Popup::at(PopupKind::DockMenu { app: a.id.trim_end_matches(".desktop").to_string(), pinned: *pinned }, r.center().x - full.center().x, m.output)));
        } else if resp.clicked() {
            m.acts.push(Act::Launch(a.id.clone()));
        }
        x += size + DOCK_GAP;
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

// ------------------------------------------------------------------ desktop widgets

/// The desktop widgets, each of which can be dragged to a place of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Widget {
    Clock,
    System,
}

/// Where a widget of `size` goes: its saved place in `area`, else `usual`; while it's being
/// dragged, where the pointer has taken it. Letting go saves the place; a double-click puts it
/// back in its usual place.
fn widget_place(ui: &egui::Ui, m: &mut Model, which: Widget, area: Rect, usual: Pos2, size: Vec2) -> Pos2 {
    let saved = match which {
        Widget::Clock => m.s.widget_clock_at,
        Widget::System => m.s.widget_system_at,
    };
    let room = (area.size() - size).max(Vec2::ZERO);
    let base = saved.map(|(fx, fy)| area.min + Vec2::new(room.x * fx, room.y * fy)).unwrap_or(usual);
    let id = egui::Id::unique(("desktop-widget", which));
    let resp = ui.interact(Rect::from_min_size(base, size), id, Sense::click_and_drag());
    let moved = ui.ctx().data_mut(|d| {
        let by = d.get_temp_mut_or_default::<Vec2>(id);
        if resp.dragged() {
            *by += resp.drag_delta();
        }
        *by
    });
    let at = (base + moved).clamp(area.min, area.min + room);
    if resp.drag_stopped() {
        ui.ctx().data_mut(|d| d.remove::<Vec2>(id));
        let frac = |d: f32, r: f32| if r > 0.0 { (d / r).clamp(0.0, 1.0) } else { 0.0 };
        if moved != Vec2::ZERO {
            m.acts.push(Act::MoveWidget(which, Some((frac(at.x - area.left(), room.x), frac(at.y - area.top(), room.y)))));
        }
    } else if resp.double_clicked() && saved.is_some() {
        m.acts.push(Act::MoveWidget(which, None));
    }
    at
}

pub fn widgets(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let side = |e: Edge| crate::panels::room(&m.s.panels, m.output, e);
    // The room the widgets move in: the screen less its panels, with a margin.
    let area = Rect::from_min_max(Pos2::new(full.left() + side(Edge::Left) + 28.0, full.top() + side(Edge::Top) + 24.0), Pos2::new(full.right() - side(Edge::Right) - 28.0, full.bottom() - side(Edge::Bottom) - 24.0));
    let w = 280.0;
    let x = if m.s.widget_side == Side::Left { area.left() } else { area.right() - w };
    // The next free place in the usual stack, for a widget that hasn't been moved.
    let mut y = area.top();
    let p = ui.painter().clone();
    let fg = Color32::WHITE;
    if m.s.widget_clock {
        let now = Local::now();
        let t = p.layout_no_wrap(now.format("%H:%M").to_string(), FontId::proportional(64.0), fg);
        let d = p.layout_no_wrap(now.format("%A, %B %-d").to_string(), ui_font(17.0), fg.gamma_multiply(0.88));
        let size = Vec2::new(t.size().x.max(d.size().x + 3.0), t.size().y + d.size().y - 4.0);
        let at = widget_place(ui, m, Widget::Clock, area, Pos2::new(x, y), size);
        // A soft shadow behind the text, so it reads on any picture.
        for (g, at) in [(t.clone(), at), (d.clone(), at + Vec2::new(3.0, t.size().y - 4.0))] {
            p.galley(at + Vec2::new(0.0, 1.0), g.clone(), Color32::from_black_alpha(70));
            p.galley(at, g, fg);
        }
        if m.s.widget_clock_at.is_none() {
            y += t.size().y + d.size().y + 18.0;
        }
    }
    if m.s.widget_system {
        let st = m.status;
        // Three rows; the last has a bar under it only for the battery. The card's padding
        // below its last line matches the padding above its first.
        let (pad, step) = (16.0, 36.0);
        let first = pad + 8.0;
        let last_bottom = first + 2.0 * step + if st.battery.is_some() { 15.0 } else { 8.0 };
        let size = Vec2::new(w, last_bottom + pad);
        let at = widget_place(ui, m, Widget::System, area, Pos2::new(x, y), size);
        let r = Rect::from_min_size(at, size);
        card(&p, r, &v, 0.82, r_lg());
        let txt = v.text_color();
        let weak = v.weak_text_color();
        let (bar2, bar3) = (kit.tone(m.s, crate::style::Slot::Bar2), kit.tone(m.s, crate::style::Slot::Bar3));
        let row = |label: &str, value: String, frac: f32, yy: f32, accent: Color32| {
            p.text(Pos2::new(r.left() + pad, yy), Align2::LEFT_CENTER, label, ui_font(12.5), weak);
            p.text(Pos2::new(r.right() - pad, yy), Align2::RIGHT_CENTER, value, ui_font(12.5), txt);
            let bar = Rect::from_min_size(Pos2::new(r.left() + pad, yy + 10.0), Vec2::new(r.width() - 2.0 * pad, 5.0));
            p.rect_filled(bar, 3.0, mix(v.panel_fill, txt, 0.12));
            p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac.clamp(0.0, 1.0), bar.height())), 3.0, accent);
        };
        let ys = [r.top() + first, r.top() + first + step, r.top() + first + 2.0 * step];
        row("Processor", format!("{:.0}%", st.cpu), st.cpu / 100.0, ys[0], accent);
        let (used, total) = st.mem;
        let gb = |b: u64| b as f32 / (1u64 << 30) as f32;
        row("Memory", format!("{:.1} of {:.0} GB", gb(used), gb(total)), if total > 0 { used as f32 / total as f32 } else { 0.0 }, ys[1], bar2);
        match st.battery {
            Some((pct, charging)) => row("Battery", format!("{pct}%{}", if charging { ", charging" } else { "" }), pct as f32 / 100.0, ys[2], bar3),
            None => {
                let up = crate::sys::uptime();
                p.text(Pos2::new(r.left() + pad, ys[2]), Align2::LEFT_CENTER, "Up for", ui_font(12.5), weak);
                p.text(Pos2::new(r.right() - pad, ys[2]), Align2::RIGHT_CENTER, format!("{} h {} min", up / 3600, up / 60 % 60), ui_font(12.5), txt);
            }
        }
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
        PopupKind::Calendar { .. } => Vec2::new(300.0, 300.0),
        PopupKind::Power { .. } => crate::power_ui::size(m.s),
        PopupKind::Control { .. } => Vec2::new(408.0, crate::quick::height(m, ui.ctx()).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::DockMenu { app, pinned } => Vec2::new(220.0, if *pinned { 190.0 } else { 116.0 } + if closable(m, app).is_empty() { 0.0 } else { 34.0 }),
        PopupKind::Network { .. } | PopupKind::Bluetooth => Vec2::new(340.0, crate::conn::height(&pop.kind, m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Ai => Vec2::new(340.0, crate::ai_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Notifications => Vec2::new(crate::notes_pop::WIDTH, crate::notes_pop::height(m, ui.ctx()).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Sound { .. } => Vec2::new(360.0, crate::sound_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Plugin { id } => Vec2::new(PLUGIN_W, plugin_menu_height(m, id).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::TrayMenu { path, .. } => Vec2::new(260.0, crate::tray_ui::menu_height(m, path).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Auth { .. } => Vec2::new(crate::auth_pop::WIDTH, crate::auth_pop::height(m, ui.ctx())),
        PopupKind::Media => Vec2::new(crate::media_pop::WIDTH, crate::media_pop::height(m)),
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
        PopupKind::DockMenu { .. } if pop.from.is_none() => {
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
        PopupKind::DockMenu { .. } | PopupKind::TrayMenu { .. } => 6.0,
        PopupKind::Control { .. } => crate::quick::PAD,
        PopupKind::Notifications => crate::notes_pop::PAD,
        PopupKind::Auth { .. } => crate::auth_pop::PAD,
        PopupKind::Media => crate::media_pop::PAD,
        _ => 14.0,
    };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(pad)).layout(egui::Layout::top_down(egui::Align::Min)));
    let focus = std::mem::take(&mut pop.focus);
    match &mut pop.kind {
        PopupKind::Launcher { query, sel } => crate::launcher::launcher(&mut child, query, sel, focus, m, kit),
        PopupKind::Calendar { month } => calendar(&mut child, month, kit.accent(m.s)),
        PopupKind::Power { confirm } => crate::power_ui::menu(&mut child, confirm, m),
        PopupKind::Control { drag } => crate::quick::quick_settings(&mut child, drag, m, kit),
        PopupKind::DockMenu { app, pinned } => dock_menu(&mut child, app, *pinned, m),
        PopupKind::Network { ask } => crate::conn::network(&mut child, ask, m, kit),
        PopupKind::Bluetooth => crate::conn::bluetooth(&mut child, m, kit),
        PopupKind::Ai => crate::ai_pop::popup(&mut child, m, kit),
        PopupKind::Notifications => crate::notes_pop::popup(&mut child, m, kit),
        PopupKind::Sound { drag } => crate::sound_pop::popup(&mut child, drag, m, kit),
        PopupKind::TrayMenu { key, path } => crate::tray_ui::menu(&mut child, key, path, m),
        PopupKind::Plugin { id } => plugin_menu(&mut child, id, m, kit),
        PopupKind::Auth { typed, round } => crate::auth_pop::popup(&mut child, typed, round, m, kit),
        PopupKind::Media => crate::media_pop::popup(&mut child, m, kit),
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

fn calendar(ui: &mut egui::Ui, month: &mut i32, accent: Color32) {
    let v = ui.visuals().clone();
    let today = Local::now().date_naive();
    let base = today.with_day(1).unwrap_or(today);
    let total = base.year() * 12 + base.month0() as i32 + *month;
    let (y, mo) = (total.div_euclid(12), total.rem_euclid(12) as u32 + 1);
    let first = NaiveDate::from_ymd_opt(y, mo, 1).unwrap_or(base);
    ui.horizontal(|ui| {
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
                }
            });
        });
    });
    ui.add_space(8.0);
    let r = ui.available_rect_before_wrap();
    let cw = r.width() / 7.0;
    let ch = 30.0;
    let p = ui.painter();
    for (i, d) in ["Mo", "Tu", "We", "Th", "Fr", "Sa", "Su"].iter().enumerate() {
        p.text(Pos2::new(r.left() + cw * (i as f32 + 0.5), r.top() + 10.0), Align2::CENTER_CENTER, *d, ui_font(11.5), v.weak_text_color());
    }
    let offset = first.weekday().num_days_from_monday() as i32;
    let days = NaiveDate::from_ymd_opt(if mo == 12 { y + 1 } else { y }, if mo == 12 { 1 } else { mo + 1 }, 1).map(|n| (n - first).num_days()).unwrap_or(30) as i32;
    for d in 0..days {
        let cell = d + offset;
        let c = Pos2::new(r.left() + cw * ((cell % 7) as f32 + 0.5), r.top() + 34.0 + ch * (cell / 7) as f32);
        let date = first + chrono::Duration::days(d as i64);
        let col = if date == today {
            p.circle_filled(c, 13.0, accent);
            on(accent)
        } else if cell % 7 >= 5 {
            v.weak_text_color()
        } else {
            v.text_color()
        };
        p.text(c, Align2::CENTER_CENTER, (d + 1).to_string(), ui_font(13.0), col);
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

    fn app(id: &str, name: &str) -> DesktopApp {
        DesktopApp { id: id.into(), name: name.into(), exec: String::new(), icon: String::new(), mimes: Vec::new(), terminal: false }
    }

    #[test]
    fn finds_apps_and_sizes() {
        let apps = [app("org.mozilla.firefox.desktop", "Firefox"), app("rivet-files.desktop", "Rivet Files"), app("kitty.desktop", "kitty")];
        assert_eq!(find_app(&apps, "firefox").map(|a| a.name.as_str()), Some("Firefox"));
        assert_eq!(find_app(&apps, "rivet-files").map(|a| a.name.as_str()), Some("Rivet Files"));
        assert!(find_app(&apps, "nope").is_none());
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
