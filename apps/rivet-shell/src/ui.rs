//! What the shell draws: the panel, the dock, the desktop widgets and the popups (launcher,
//! calendar, power), in the Rivet apps' look. The same code draws them on the desktop (each on
//! its own layer surface) and in screenshots (all on one picture).

use chrono::{Datelike, Local, NaiveDate};
use eframe::egui::{self, Align2, Color32, FontId, Key, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rivet_look::icons::IconTheme;
use rivet_look::live::LiveLook;
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md, r_sm};
use rivet_look::{fonts, quick};
use rivet_xdg::launch::DesktopApp;

use crate::config::{Edge, Settings, Side};
use crate::sys::{Compositor, Net, Power, Status};

/// Heights and room, in points.
pub const BAR: f32 = 36.0;
pub const MARGIN: f32 = 6.0;
const DOCK_PAD: f32 = 8.0;
const DOCK_GAP: f32 = 6.0;
const DOCK_LABEL: f32 = 30.0;

/// The panel surface's height (and the room it keeps from windows).
pub fn panel_height(s: &Settings) -> f32 {
    if s.panel_floating { BAR + MARGIN * 2.0 } else { BAR }
}

/// The dock surface's size, and the room it keeps from windows.
pub fn dock_size(s: &Settings, apps: usize) -> (Vec2, f32) {
    let n = apps.max(1) as f32;
    let w = DOCK_PAD * 2.0 + n * s.dock_icon + (n - 1.0) * DOCK_GAP + MARGIN * 2.0;
    let body = s.dock_icon + DOCK_PAD * 2.0 + MARGIN;
    (Vec2::new(w, body + DOCK_LABEL), body)
}

#[derive(Clone, Debug, PartialEq)]
pub enum PopupKind {
    Launcher { query: String, sel: usize },
    Calendar { month: i32 },
    Power { confirm: Option<Power> },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Popup {
    pub kind: PopupKind,
    /// Where it opens from (x on its screen, in points), under the panel button.
    pub anchor_x: f32,
    /// Its screen's name (empty: whichever the compositor picks).
    pub output: String,
    pub focus: bool,
}

impl Popup {
    pub fn launcher(output: &str) -> Popup {
        Popup { kind: PopupKind::Launcher { query: String::new(), sel: 0 }, anchor_x: 0.0, output: output.to_string(), focus: true }
    }
    /// Whether it needs the keyboard (the launcher's search; Esc for the others).
    pub fn wants_keyboard(&self) -> bool {
        matches!(self.kind, PopupKind::Launcher { .. })
    }
}

/// What the person asked for, done by whoever runs the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Act {
    Launch(String),
    Workspace(String),
    Open(Popup),
    Close,
    Volume(bool),
    Mute,
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

    pub fn app_icon(&mut self, ctx: &egui::Context, app: &DesktopApp, px: f32) -> Option<egui::TextureHandle> {
        let size = (px * ctx.pixels_per_point()).round() as u32;
        // The app's own icon first: a fallback name in the same list could be answered by
        // Rivet's built-in icons before the theme is even asked for the app's.
        let own = if app.icon.starts_with('/') { self.icons.texture_from_path(ctx, std::path::Path::new(&app.icon), size) } else if app.icon.is_empty() { None } else { self.icons.texture(ctx, &[app.icon.as_str()], size) };
        own.or_else(|| self.icons.texture(ctx, &["application-x-executable"], size))
    }
}

/// What a frame needs to know.
pub struct Model<'a> {
    pub s: &'a Settings,
    pub status: &'a Status,
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

fn ui_font(size: f32) -> FontId {
    fonts::ui_font(size)
}

/// A see-through card in the panel's colour, with a soft shadow.
fn card(p: &egui::Painter, r: Rect, v: &egui::Visuals, alpha: f32, radius: impl Into<egui::CornerRadius>) {
    let radius: egui::CornerRadius = radius.into();
    let shadow = egui::Shadow { offset: [0, 2], blur: 14, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
    p.add(shadow.as_shape(r, radius));
    p.rect_filled(r, radius, v.panel_fill.gamma_multiply(alpha));
    p.rect_stroke(r, radius, Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08).gamma_multiply(alpha)), egui::StrokeKind::Inside);
}

fn hover_fill(p: &egui::Painter, r: Rect, v: &egui::Visuals) {
    p.rect_filled(r, r_md(), mix(v.panel_fill, v.text_color(), 0.09));
}

// ------------------------------------------------------------------ the panel

pub fn panel(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let (bar, radius) = if m.s.panel_floating {
        (Rect::from_min_size(full.min + Vec2::new(MARGIN, MARGIN), Vec2::new(full.width() - MARGIN * 2.0, BAR)), egui::CornerRadius::from(r_lg()))
    } else {
        (full, egui::CornerRadius::ZERO)
    };
    card(ui.painter(), bar, &v, 0.94, radius);
    let p = ui.painter().clone();
    let fg = v.text_color();
    let weak = v.weak_text_color();
    let cy = bar.center().y;
    let btn = |ui: &mut egui::Ui, r: Rect, id: &str| -> egui::Response {
        let resp = ui.interact(r, egui::Id::unique(("panel", id)), Sense::click());
        if resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    };
    // ---- left: the launcher and the workspaces
    let mut x = bar.left() + 6.0;
    let r = Rect::from_center_size(Pos2::new(x + 15.0, cy), Vec2::new(30.0, 28.0));
    let resp = btn(ui, r, "launcher");
    glyph_grid(&p, r.center(), fg);
    if resp.clicked() {
        m.acts.push(Act::Open(Popup { kind: PopupKind::Launcher { query: String::new(), sel: 0 }, anchor_x: r.center().x, output: m.output.to_string(), focus: true }));
    }
    x = r.right() + 8.0;
    if m.s.show_workspaces {
        let here: Vec<_> = m.status.workspaces.iter().filter(|w| m.output.is_empty() || w.output.is_empty() || w.output == m.output).collect();
        for w in here {
            let label = if w.label.is_empty() { w.key.clone() } else { w.label.clone() };
            let g = p.layout_no_wrap(label.clone(), ui_font(12.5), fg);
            let width = if w.active { (g.size().x + 26.0).max(38.0) } else { (g.size().x + 14.0).max(24.0) };
            let r = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(width, 22.0));
            let resp = ui.interact(r, egui::Id::unique(("ws", &w.key, &w.output)), Sense::click());
            let (fill, col) = if w.active {
                (accent, if rivet_look::palette::luminance(accent) > 0.7 { Color32::BLACK } else { Color32::WHITE })
            } else if resp.hovered() {
                (mix(v.panel_fill, fg, 0.16), fg)
            } else {
                (mix(v.panel_fill, fg, if w.busy { 0.1 } else { 0.05 }), if w.busy { fg } else { weak })
            };
            p.rect_filled(r, 11.0, fill);
            // Laid out again in its colour (a galley keeps the colour it was made with).
            let g = p.layout_no_wrap(label, ui_font(12.5), col);
            p.galley(r.center() - g.size() / 2.0, g, col);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Workspace {} ({})", w.label, m.compositor.name())).clicked() {
                m.acts.push(Act::Workspace(w.key.clone()));
            }
            x = r.right() + 4.0;
        }
    }
    let left_end = x;
    // ---- right: power, settings, battery, network, volume
    let mut rx = bar.right() - 6.0;
    let mut slot = |ui: &mut egui::Ui, w: f32, id: &str| -> (Rect, egui::Response) {
        let r = Rect::from_min_max(Pos2::new(rx - w, cy - 14.0), Pos2::new(rx, cy + 14.0));
        rx = r.left() - 2.0;
        let resp = btn(ui, r, id);
        (r, resp)
    };
    let (r, resp) = slot(ui, 30.0, "power");
    glyph_power(&p, r.center(), fg);
    if resp.on_hover_text("Lock, suspend, restart, shut down").clicked() {
        m.acts.push(Act::Open(Popup { kind: PopupKind::Power { confirm: None }, anchor_x: r.center().x, output: m.output.to_string(), focus: true }));
    }
    let (r, resp) = slot(ui, 30.0, "settings");
    glyph_gear(&p, r.center(), fg);
    if resp.on_hover_text("Settings").clicked() {
        m.acts.push(Act::Settings);
    }
    if m.s.show_battery
        && let Some((pct, charging)) = m.status.battery
    {
        let text = format!("{pct}%");
        let g = p.layout_no_wrap(text, ui_font(12.5), fg);
        let (r, resp) = slot(ui, g.size().x + 40.0, "battery");
        let low = pct <= 15 && !charging;
        let col = if low { rivet_look::theme::status(ui.ctx()).danger } else { fg };
        glyph_battery(&p, Pos2::new(r.left() + 16.0, cy), pct, charging, col, accent);
        p.galley(Pos2::new(r.left() + 30.0, cy - g.size().y / 2.0), g, col);
        resp.on_hover_text(if charging { format!("Battery {pct}%, charging") } else { format!("Battery {pct}%") });
    }
    if m.s.show_network {
        let (r, resp) = slot(ui, 30.0, "network");
        let tip = match &m.status.net {
            Net::Wifi(ssid) => {
                glyph_wifi(&p, r.center(), fg);
                if ssid.is_empty() { "Wi-Fi".to_string() } else { format!("Wi-Fi: {ssid}") }
            }
            Net::Wired => {
                glyph_wired(&p, r.center(), fg);
                "Wired".to_string()
            }
            Net::None => {
                glyph_wifi(&p, r.center(), weak.gamma_multiply(0.5));
                "Not connected".to_string()
            }
        };
        resp.on_hover_text(tip);
    }
    if m.s.show_volume
        && let Some((vol, muted)) = m.status.volume
    {
        let text = if muted { "Muted".to_string() } else { format!("{vol}%") };
        let g = p.layout_no_wrap(text, ui_font(12.5), if muted { weak } else { fg });
        let (r, resp) = slot(ui, g.size().x + 38.0, "volume");
        glyph_speaker(&p, Pos2::new(r.left() + 15.0, cy), muted, if muted { weak } else { fg });
        p.galley(Pos2::new(r.left() + 30.0, cy - g.size().y / 2.0), g, if muted { weak } else { fg });
        if resp.hovered() {
            let dy: f32 = ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum());
            if dy.abs() > 0.5 {
                m.acts.push(Act::Volume(dy > 0.0));
            }
        }
        if resp.on_hover_text("Click to mute · scroll to change").clicked() {
            m.acts.push(Act::Mute);
        }
    }
    // ---- the middle: the clock
    let now = Local::now();
    let text = now.format(&m.s.clock_format).to_string();
    let g = p.layout_no_wrap(text, ui_font(13.5), fg);
    let r = Rect::from_center_size(bar.center(), g.size() + Vec2::new(22.0, 10.0));
    if r.left() > left_end && r.right() < rx {
        let resp = btn(ui, r, "clock");
        p.galley(r.center() - g.size() / 2.0, g, fg);
        if resp.on_hover_text(now.format("%A, %B %-d, %Y").to_string()).clicked() {
            m.acts.push(Act::Open(Popup { kind: PopupKind::Calendar { month: 0 }, anchor_x: r.center().x, output: m.output.to_string(), focus: true }));
        }
    }
}

// ------------------------------------------------------------------ the dock

pub fn dock(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let apps: Vec<DesktopApp> = m.s.dock_apps.iter().filter_map(|id| find_app(m.apps, id).cloned()).collect();
    let size = m.s.dock_icon;
    let body_h = size + DOCK_PAD * 2.0;
    let body = Rect::from_min_size(Pos2::new(full.left() + MARGIN, full.bottom() - MARGIN - body_h), Vec2::new(full.width() - MARGIN * 2.0, body_h));
    card(ui.painter(), body, &v, 0.9, r_lg());
    let p = ui.painter().clone();
    let mut x = body.left() + DOCK_PAD;
    let mut label: Option<(String, f32)> = None;
    for a in &apps {
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
        if m.s.dock_running {
            let id = a.id.trim_end_matches(".desktop").to_lowercase();
            let short = id.rsplit('.').next().unwrap_or(&id).to_string();
            let name = a.name.to_lowercase();
            if m.status.running.iter().any(|w| *w == id || *w == short || *w == name || id.ends_with(w.as_str())) {
                p.circle_filled(Pos2::new(r.center().x, body.bottom() - 4.0), 2.5, accent);
            }
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
    let top = full.top() + if m.s.panel && m.s.panel_edge == Edge::Top { panel_height(m.s) } else { 0.0 } + 24.0;
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
                let up = std::fs::read_to_string("/proc/uptime").ok().and_then(|t| t.split_whitespace().next().and_then(|x| x.parse::<f64>().ok())).unwrap_or(0.0) as u64;
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
    let below = if m.s.panel_edge == Edge::Top { panel_height(m.s) + 4.0 } else { MARGIN };
    let esc = ui.input(|i| i.key_pressed(Key::Escape));
    let size = match pop.kind {
        PopupKind::Launcher { .. } => Vec2::new(620.0f32.min(full.width() - 40.0), 480.0f32.min(full.height() - 120.0)),
        PopupKind::Calendar { .. } => Vec2::new(300.0, 300.0),
        PopupKind::Power { .. } => Vec2::new(240.0, 238.0),
    };
    let rect = match pop.kind {
        PopupKind::Launcher { .. } => {
            // A soft dim behind the launcher.
            ui.painter().rect_filled(full, 0.0, Color32::from_black_alpha(70));
            Rect::from_min_size(Pos2::new(full.center().x - size.x / 2.0, full.top() + (full.height() * 0.16).max(below + 10.0)), size)
        }
        _ => {
            let x = (pop.anchor_x - size.x / 2.0).clamp(full.left() + MARGIN, full.right() - MARGIN - size.x);
            let y = if m.s.panel_edge == Edge::Top { full.top() + below } else { full.bottom() - panel_height(m.s) - 4.0 - size.y };
            Rect::from_min_size(Pos2::new(x, y), size)
        }
    };
    // Outside the card: close.
    let back = ui.interact(full, egui::Id::unique("popup-back"), Sense::click());
    if back.clicked() && !ui.input(|i| i.pointer.interact_pos().is_some_and(|p| rect.contains(p))) || esc {
        m.acts.push(Act::Close);
    }
    card(ui.painter(), rect, &v, 0.98, r_lg());
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink(14.0)).layout(egui::Layout::top_down(egui::Align::Min)));
    let focus = std::mem::take(&mut pop.focus);
    match &mut pop.kind {
        PopupKind::Launcher { query, sel } => launcher(&mut child, query, sel, focus, m, kit),
        PopupKind::Calendar { month } => calendar(&mut child, month, kit.accent(m.s)),
        PopupKind::Power { confirm } => power_menu(&mut child, confirm, m),
    }
}

fn launcher(ui: &mut egui::Ui, query: &mut String, sel: &mut usize, focus: bool, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let id = egui::Id::unique("launcher-search");
    let field = egui::Frame::NONE.fill(mix(v.panel_fill, v.text_color(), 0.07)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 9)).show(ui, |ui| {
        ui.add(egui::TextEdit::singleline(query).id(id).frame(egui::Frame::NONE).font(ui_font(16.0)).hint_text("Search apps…").desired_width(f32::INFINITY))
    });
    let resp = field.inner;
    if focus || !resp.has_focus() {
        resp.request_focus();
    }
    if resp.changed() {
        *sel = 0;
    }
    ui.add_space(12.0);
    // The apps: best matches first when searching, else by name.
    let mut ranked: Vec<(i32, &DesktopApp)> = m.apps.iter().filter_map(|a| quick::fuzzy_score(&a.name, query).map(|s| (s, a))).collect();
    if !query.is_empty() {
        ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
    }
    let cols = ((ui.available_width() + 6.0) / 104.0).floor().max(1.0) as usize;
    let (up, down, left, right, enter) = ui.input_mut(|i| {
        (
            i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowLeft) && query.is_empty(),
            i.consume_key(egui::Modifiers::NONE, Key::ArrowRight) && query.is_empty(),
            i.key_pressed(Key::Enter),
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
    let accent = kit.accent(m.s);
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Grid::new("launcher-grid").spacing([6.0, 6.0]).show(ui, |ui| {
            for (k, (_, a)) in ranked.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(98.0, 92.0), Sense::click());
                if k == *sel {
                    ui.painter().rect_filled(r, r_md(), accent.gamma_multiply(0.22));
                    ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.6)), egui::StrokeKind::Inside);
                    if up || down || left || right {
                        ui.scroll_to_rect(r, None);
                    }
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
            if rivet_look::palette::luminance(accent) > 0.55 { Color32::BLACK } else { Color32::WHITE }
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

// ------------------------------------------------------------------ drawn symbols

fn arc(p: &egui::Painter, c: Pos2, r: f32, from: f32, to: f32, s: Stroke) {
    let n = 14;
    let pts: Vec<Pos2> = (0..=n).map(|i| {
        let a = from + (to - from) * i as f32 / n as f32;
        c + Vec2::new(a.cos(), a.sin()) * r
    }).collect();
    p.add(Shape::line(pts, s));
}

fn glyph_grid(p: &egui::Painter, c: Pos2, col: Color32) {
    for i in -1..=1 {
        for j in -1..=1 {
            p.circle_filled(c + Vec2::new(i as f32 * 5.0, j as f32 * 5.0), 1.7, col);
        }
    }
}

fn glyph_power(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.7, col);
    use std::f32::consts::PI;
    arc(p, c + Vec2::new(0.0, 1.0), 6.5, -PI / 2.0 + 0.7, 3.0 * PI / 2.0 - 0.7, s);
    p.line_segment([c + Vec2::new(0.0, -7.0), c + Vec2::new(0.0, 0.5)], s);
}

fn glyph_gear(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.7, col);
    p.circle_stroke(c, 5.2, s);
    p.circle_filled(c, 1.8, col);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + d * 5.2, c + d * 7.6], Stroke::new(2.2, col));
    }
}

fn glyph_battery(p: &egui::Painter, c: Pos2, pct: u32, charging: bool, col: Color32, accent: Color32) {
    let body = Rect::from_center_size(c, Vec2::new(18.0, 10.0));
    p.rect_stroke(body, 2.0, Stroke::new(1.4, col), egui::StrokeKind::Inside);
    p.rect_filled(Rect::from_min_size(Pos2::new(body.right() + 0.5, c.y - 2.5), Vec2::new(2.0, 5.0)), 1.0, col);
    let inner = body.shrink(2.5);
    let w = inner.width() * (pct as f32 / 100.0).clamp(0.05, 1.0);
    p.rect_filled(Rect::from_min_size(inner.min, Vec2::new(w, inner.height())), 1.0, if charging { accent } else { col });
}

fn glyph_wifi(p: &egui::Painter, c: Pos2, col: Color32) {
    use std::f32::consts::PI;
    let base = c + Vec2::new(0.0, 5.0);
    for r in [5.0, 9.0] {
        arc(p, base, r, -PI * 0.78, -PI * 0.22, Stroke::new(1.7, col));
    }
    p.circle_filled(base + Vec2::new(0.0, -1.0), 1.7, col);
}

fn glyph_wired(p: &egui::Painter, c: Pos2, col: Color32) {
    let s = Stroke::new(1.6, col);
    p.rect_stroke(Rect::from_center_size(c + Vec2::new(0.0, 2.0), Vec2::new(14.0, 8.0)), 1.5, s, egui::StrokeKind::Inside);
    p.line_segment([c + Vec2::new(0.0, -2.0), c + Vec2::new(0.0, -7.0)], s);
    for dx in [-3.5, 0.0, 3.5] {
        p.line_segment([c + Vec2::new(dx, 1.0), c + Vec2::new(dx, 3.5)], Stroke::new(1.2, col));
    }
}

fn glyph_speaker(p: &egui::Painter, c: Pos2, muted: bool, col: Color32) {
    let pts = vec![c + Vec2::new(-7.0, -2.5), c + Vec2::new(-4.0, -2.5), c + Vec2::new(0.0, -6.0), c + Vec2::new(0.0, 6.0), c + Vec2::new(-4.0, 2.5), c + Vec2::new(-7.0, 2.5)];
    p.add(Shape::convex_polygon(pts, col, Stroke::NONE));
    let s = Stroke::new(1.5, col);
    if muted {
        p.line_segment([c + Vec2::new(3.0, -3.0), c + Vec2::new(8.0, 3.0)], s);
        p.line_segment([c + Vec2::new(8.0, -3.0), c + Vec2::new(3.0, 3.0)], s);
    } else {
        use std::f32::consts::PI;
        arc(p, c + Vec2::new(0.0, 0.0), 4.0, -PI / 3.5, PI / 3.5, s);
        arc(p, c + Vec2::new(0.0, 0.0), 7.5, -PI / 3.5, PI / 3.5, s);
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
        let apps = [app("org.mozilla.firefox.desktop", "Firefox"), app("rivet-files.desktop", "Rivet Files")];
        assert_eq!(find_app(&apps, "firefox").map(|a| a.name.as_str()), Some("Firefox"));
        assert_eq!(find_app(&apps, "rivet-files").map(|a| a.name.as_str()), Some("Rivet Files"));
        assert!(find_app(&apps, "nope").is_none());
        let s = Settings::default();
        let (size, room) = dock_size(&s, 5);
        assert!(size.x > 5.0 * s.dock_icon && room < size.y);
        assert_eq!(panel_height(&Settings { panel_floating: false, ..Settings::default() }), BAR);
    }
}
