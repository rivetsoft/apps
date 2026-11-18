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
use rivet_look::{fonts, quick};
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
    let id = a.id.trim_end_matches(".desktop").to_lowercase();
    let short = id.rsplit('.').next().unwrap_or(&id).to_string();
    let name = a.name.to_lowercase();
    running.iter().any(|w| *w == id || *w == short || *w == name || id.ends_with(w.as_str()))
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
    /// Sound devices and apps (`drag`: a volume being dragged).
    Sound { drag: Option<(u64, u32)> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub kind: PopupKind,
    /// Where it opens from (x on its screen, in points), under the panel button; for the dock's
    /// menu, from the screen's middle.
    pub anchor_x: f32,
    /// Its screen's name (empty: whichever the compositor picks).
    pub output: String,
    pub focus: bool,
    /// The panel it opens from: that panel's edge, and how far from the edge to open (None:
    /// opened by a key, so under the screen's top panels, or over its bottom ones).
    pub from: Option<(Edge, f32)>,
}

impl Popup {
    pub fn launcher(output: &str) -> Popup {
        Popup::at(PopupKind::Launcher { query: String::new(), sel: 0 }, 0.0, output)
    }
    pub fn at(kind: PopupKind, anchor_x: f32, output: &str) -> Popup {
        Popup { kind, anchor_x, output: output.to_string(), focus: true, from: None }
    }
    /// Whether it needs the keyboard (the launcher's search; Esc for the others).
    pub fn wants_keyboard(&self) -> bool {
        matches!(self.kind, PopupKind::Launcher { .. } | PopupKind::Network { .. })
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
}

/// What the person asked for, done by whoever runs the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Launch(String),
    /// Sound devices and apps: choose, change the volume, mute, move.
    Audio(crate::audio::Job),
    /// A program with no desktop entry, by name.
    Spawn(String),
    Workspace(String),
    Open(Popup),
    Close,
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
}

/// Each surface's look: colours and fonts (following the desktop), and icons.
pub struct Kit {
    pub look: LiveLook,
    pub icons: IconTheme,
    live: bool,
}

impl Kit {
    pub fn new(live: bool) -> Kit {
        Kit { look: LiveLook::new(), icons: IconTheme::new(&rivet_look::icons::detect_system_theme()), live }
    }

    /// Call first in every frame of the surface.
    pub fn begin(&mut self, ctx: &egui::Context, s: &Settings) {
        self.look.set_fonts(ctx, "", "", "");
        self.look.frame(ctx, &s.choice(), self.live);
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&s.choice());
        self.icons.set_builtin(Some(rivet_look::builtin_icons::BuiltinColors { accent, bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }));
    }

    pub fn accent(&self, s: &Settings) -> Color32 {
        self.look.accent(&s.choice())
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
    let radius: egui::CornerRadius = radius.into();
    let shadow = egui::Shadow { offset: [0, 2], blur: 14, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
    p.add(shadow.as_shape(r, radius));
    p.rect_filled(r, radius, v.panel_fill.gamma_multiply(alpha));
    p.rect_stroke(r, radius, Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08).gamma_multiply(alpha)), egui::StrokeKind::Inside);
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
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let items = dock_items(m.s, m.apps, &m.status.running);
    let size = m.s.dock_icon;
    let body_h = size + DOCK_PAD * 2.0;
    let body = Rect::from_min_size(Pos2::new(full.left() + MARGIN, full.bottom() - MARGIN - body_h), Vec2::new(full.width() - MARGIN * 2.0, body_h));
    card(ui.painter(), body, &v, 0.9, r_lg());
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
            p.rect_filled(r.expand(3.0), r_md(), mix(v.panel_fill, v.text_color(), 0.08));
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
            p.circle_filled(Pos2::new(r.center().x, body.bottom() - 4.0), 2.5, accent);
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

pub fn widgets(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let top = full.top() + crate::panels::room(&m.s.panels, m.output, Edge::Top) + 24.0;
    let w = 280.0;
    let x = if m.s.widget_side == Side::Left { full.left() + 28.0 } else { full.right() - 28.0 - w };
    let mut y = top;
    let p = ui.painter().clone();
    let fg = Color32::WHITE;
    if m.s.widget_clock {
        let now = Local::now();
        let t = p.layout_no_wrap(now.format("%H:%M").to_string(), FontId::proportional(64.0), fg);
        let d = p.layout_no_wrap(now.format("%A, %B %-d").to_string(), ui_font(17.0), fg.gamma_multiply(0.88));
        // A soft shadow behind the text, so it reads on any picture.
        for (g, at) in [(t.clone(), Pos2::new(x, y)), (d.clone(), Pos2::new(x + 3.0, y + t.size().y - 4.0))] {
            p.galley(at + Vec2::new(0.0, 1.0), g.clone(), Color32::from_black_alpha(70));
            p.galley(at, g, fg);
        }
        y += t.size().y + d.size().y + 18.0;
    }
    if m.s.widget_system {
        let r = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, 112.0));
        card(&p, r, &v, 0.82, r_lg());
        let txt = v.text_color();
        let weak = v.weak_text_color();
        let row = |label: &str, value: String, frac: f32, yy: f32| {
            p.text(Pos2::new(r.left() + 16.0, yy), Align2::LEFT_CENTER, label, ui_font(12.5), weak);
            p.text(Pos2::new(r.right() - 16.0, yy), Align2::RIGHT_CENTER, value, ui_font(12.5), txt);
            let bar = Rect::from_min_size(Pos2::new(r.left() + 16.0, yy + 10.0), Vec2::new(r.width() - 32.0, 5.0));
            p.rect_filled(bar, 3.0, mix(v.panel_fill, txt, 0.12));
            p.rect_filled(Rect::from_min_size(bar.min, Vec2::new(bar.width() * frac.clamp(0.0, 1.0), bar.height())), 3.0, accent);
        };
        let st = m.status;
        row("Processor", format!("{:.0}%", st.cpu), st.cpu / 100.0, r.top() + 22.0);
        let (used, total) = st.mem;
        let gb = |b: u64| b as f32 / (1u64 << 30) as f32;
        row("Memory", format!("{:.1} of {:.0} GB", gb(used), gb(total)), if total > 0 { used as f32 / total as f32 } else { 0.0 }, r.top() + 58.0);
        match st.battery {
            Some((pct, charging)) => row("Battery", format!("{pct}%{}", if charging { ", charging" } else { "" }), pct as f32 / 100.0, r.top() + 94.0),
            None => {
                let up = crate::sys::uptime();
                p.text(Pos2::new(r.left() + 16.0, r.top() + 94.0), Align2::LEFT_CENTER, "Up for", ui_font(12.5), weak);
                p.text(Pos2::new(r.right() - 16.0, r.top() + 94.0), Align2::RIGHT_CENTER, format!("{} h {} min", up / 3600, up / 60 % 60), ui_font(12.5), txt);
            }
        }
    }
}

// ------------------------------------------------------------------ popups

/// A popup over the whole screen: its card, and a click anywhere else closes it.
pub fn popup(ui: &mut egui::Ui, pop: &mut Popup, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let full = ui.max_rect();
    let top_room = crate::panels::room(&m.s.panels, m.output, Edge::Top);
    let bottom_room = crate::panels::room(&m.s.panels, m.output, Edge::Bottom);
    // Where it opens: under (or over) the panel it came from; else under the top panels, or
    // over the bottom ones when there are only those.
    let (edge, inset) = pop.from.unwrap_or(if top_room == 0.0 && bottom_room > 0.0 { (Edge::Bottom, bottom_room + 4.0) } else { (Edge::Top, top_room + if top_room > 0.0 { 4.0 } else { MARGIN }) });
    let below = top_room + if top_room > 0.0 { 4.0 } else { MARGIN };
    // Esc while a Wi-Fi password is being typed only stops that.
    let esc = ui.input(|i| i.key_pressed(Key::Escape)) && !matches!(pop.kind, PopupKind::Network { ask: Some(_) });
    let size = match &pop.kind {
        PopupKind::Launcher { .. } => Vec2::new(620.0f32.min(full.width() - 40.0), 480.0f32.min(full.height() - 120.0)),
        PopupKind::Calendar { .. } => Vec2::new(300.0, 300.0),
        PopupKind::Power { .. } => Vec2::new(240.0, 238.0),
        PopupKind::Control { .. } => Vec2::new(392.0, crate::quick::height(m, ui.ctx()).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::DockMenu { pinned, .. } => Vec2::new(220.0, if *pinned { 190.0 } else { 116.0 }),
        PopupKind::Network { .. } | PopupKind::Bluetooth => Vec2::new(340.0, crate::conn::height(&pop.kind, m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Ai => Vec2::new(340.0, crate::ai_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
        PopupKind::Sound { .. } => Vec2::new(360.0, crate::sound_pop::height(m).min(full.height() - inset - MARGIN - 8.0)),
    };
    let rect = match pop.kind {
        PopupKind::Launcher { .. } => {
            // A soft dim behind the launcher.
            ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(70));
            Rect::from_min_size(Pos2::new(full.center().x - size.x / 2.0, full.top() + (full.height() * 0.16).max(below + 10.0)), size)
        }
        PopupKind::DockMenu { .. } => {
            // Above the dock, over the icon.
            let items = dock_items(m.s, m.apps, &m.status.running);
            let (_, room) = dock_size(m.s, &items);
            let x = (full.center().x + pop.anchor_x - size.x / 2.0).clamp(full.left() + MARGIN, full.right() - MARGIN - size.x);
            let bottom = full.bottom() - room - 8.0 - bottom_room;
            Rect::from_min_size(Pos2::new(x, bottom - size.y), size)
        }
        _ => {
            // No button to open under (asked by `rivet-shell msg`): the middle.
            let ax = if pop.anchor_x.is_nan() { full.center().x } else { pop.anchor_x };
            let x = (ax - size.x / 2.0).clamp(full.left() + MARGIN, full.right() - MARGIN - size.x);
            let y = if edge == Edge::Top { full.top() + inset } else { full.bottom() - inset - size.y };
            Rect::from_min_size(Pos2::new(x, y), size)
        }
    };
    // Outside the card: close.
    let back = ui.interact(full, egui::Id::unique("popup-back"), Sense::click());
    let outside = ui.input(|i| i.pointer.interact_pos().is_some_and(|p| !rect.contains(p)));
    if (back.clicked() || back.secondary_clicked()) && outside || esc {
        m.acts.push(Act::Close);
    }
    card(ui.painter(), rect, &v, 0.98, r_lg());
    let pad = if matches!(pop.kind, PopupKind::DockMenu { .. }) { 6.0 } else { 14.0 };
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(pad)).layout(egui::Layout::top_down(egui::Align::Min)));
    let focus = std::mem::take(&mut pop.focus);
    match &mut pop.kind {
        PopupKind::Launcher { query, sel } => launcher(&mut child, query, sel, focus, m, kit),
        PopupKind::Calendar { month } => calendar(&mut child, month, kit.accent(m.s)),
        PopupKind::Power { confirm } => power_menu(&mut child, confirm, m),
        PopupKind::Control { drag } => crate::quick::quick_settings(&mut child, drag, m, kit),
        PopupKind::DockMenu { app, pinned } => dock_menu(&mut child, app, *pinned, m),
        PopupKind::Network { ask } => crate::conn::network(&mut child, ask, m, kit),
        PopupKind::Bluetooth => crate::conn::bluetooth(&mut child, m, kit),
        PopupKind::Ai => crate::ai_pop::popup(&mut child, m, kit),
        PopupKind::Sound { drag } => crate::sound_pop::popup(&mut child, drag, m, kit),
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

fn calc_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{v:.0}")
    } else {
        let s = format!("{v:.10}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

fn launcher(ui: &mut egui::Ui, query: &mut String, sel: &mut usize, focus: bool, m: &mut Model, kit: &mut Kit) {
    // A steady cursor: a blinking one redraws the whole launcher twice a second.
    ui.visuals_mut().text_cursor.blink = false;
    let v = ui.visuals().clone();
    let id = egui::Id::unique("launcher-search");
    let field = egui::Frame::NONE.fill(mix(v.panel_fill, v.text_color(), 0.07)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.add(egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).font(ui_font(16.0)).hint_text("Search apps, or type a sum…").desired_width(f32::INFINITY))
    });
    let resp = field.inner;
    if focus || !resp.has_focus() {
        resp.request_focus();
    }
    if resp.changed() {
        *sel = 0;
    }
    ui.add_space(12.0);
    let accent = kit.accent(m.s);
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    // A sum: its answer, and Enter copies it.
    if let Some(answer) = calc(query) {
        let text = calc_text(answer);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::click());
        ui.painter().rect_filled(r, r_md(), accent.gamma_multiply(0.22));
        ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
        ui.painter().text(Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, format!("= {text}"), ui_font(20.0), v.strong_text_color());
        ui.painter().text(Pos2::new(r.right() - 16.0, r.center().y), Align2::RIGHT_CENTER, "Enter to copy", ui_font(12.0), v.weak_text_color());
        if enter || resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::Copy(text));
        }
        return;
    }
    // The apps: best matches first when searching, else by name.
    let mut ranked: Vec<(i32, &DesktopApp)> = m.apps.iter().filter_map(|a| quick::fuzzy_score(&a.name, query).map(|s| (s, a))).collect();
    if !query.is_empty() {
        ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
    }
    let cols = ((ui.available_width() + 6.0) / 104.0).floor().max(1.0) as usize;
    let (up, down, left, right) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) && query.is_empty(),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) && query.is_empty(),
        )
    });
    let n = ranked.len();
    if n > 0 {
        if down {
            *sel = (*sel + cols).min(n - 1);
        }
        if up {
            *sel = sel.saturating_sub(cols);
        }
        if right {
            *sel = (*sel + 1).min(n - 1);
        }
        if left {
            *sel = sel.saturating_sub(1);
        }
        *sel = (*sel).min(n - 1);
    }
    if enter && let Some((_, a)) = ranked.get(*sel) {
        m.acts.push(Act::Launch(a.id.clone()));
    }
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("launcher-grid").spacing([6.0, 6.0]).show(ui, |ui| {
            for (k, (_, a)) in ranked.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(98.0, 92.0), Sense::click());
                // Only what's in view is drawn (and only its icons made).
                if ui.is_rect_visible(r) {
                    if k == *sel {
                        ui.painter().rect_filled(r, r_md(), accent.gamma_multiply(0.22));
                        ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
                    } else if resp.hovered() {
                        hover_fill(ui.painter(), r, &v);
                    }
                    let ir = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 30.0), Vec2::splat(42.0));
                    if let Some(t) = kit.app_icon(ui.ctx(), a, 42.0) {
                        ui.painter().image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    let mut job = egui::text::LayoutJob::single_section(a.name.clone(), egui::TextFormat { font_id: ui_font(12.0), color: v.text_color(), ..Default::default() });
                    job.wrap = egui::text::TextWrapping { max_width: r.width() - 8.0, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
                    job.halign = egui::Align::Center;
                    let g = ui.painter().layout_job(job);
                    ui.painter().galley(Pos2::new(r.center().x, r.top() + 58.0), g, v.text_color());
                }
                if k == *sel && (up || down || left || right) {
                    ui.scroll_to_rect(r, None);
                }
                if resp.clicked() {
                    m.acts.push(Act::Launch(a.id.clone()));
                }
                if (k + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        if ranked.is_empty() {
            ui.label(egui::RichText::new("No apps match").color(v.weak_text_color()));
        }
    });
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

fn power_menu(ui: &mut egui::Ui, confirm: &mut Option<Power>, m: &mut Model) {
    let v = ui.visuals().clone();
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    for (pw, label, sure) in [(Power::Lock, "Lock", false), (Power::Suspend, "Suspend", false), (Power::LogOut, "Log Out", true), (Power::Restart, "Restart", true), (Power::ShutDown, "Shut Down", true)] {
        let asking = *confirm == Some(pw);
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::click());
        if asking {
            ui.painter().rect_filled(r, r_md(), danger.gamma_multiply(0.2));
        } else if resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let text = if asking { format!("{label}? Click again") } else { label.to_string() };
        ui.painter().text(Pos2::new(r.left() + 14.0, r.center().y), Align2::LEFT_CENTER, text, ui_font(14.0), if asking { danger } else { v.text_color() });
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if sure && !asking {
                *confirm = Some(pw);
            } else {
                m.acts.push(Act::Power(pw));
            }
        }
        ui.add_space(4.0);
    }
}

/// Right-click on a dock icon: open it, move it, pin or unpin it.
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
