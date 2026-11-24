//! A panel: its bar, and its items laid out in three zones (left, center, right). Each item
//! says how wide it wants to be and draws itself in the room it's given; an item the shell
//! doesn't know (a plugin not installed yet) takes no room.

use chrono::Local;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::palette::mix;

use crate::config::{Edge, MediaText, Show, WsStyle};
use crate::glyph;
use crate::panels::PanelCfg;
use crate::sys::Net;
use crate::style;
use crate::ui::{Act, Kit, Model, Popup, PopupKind, on, one_line, ui_font};

#[cfg(test)]
const GAP: f32 = crate::panels::SPACING;
/// An item with only a symbol.
const SLOT: f32 = 30.0;
/// Inside an item's ends, and between its symbol and its text.
const PAD: f32 = 8.0;
const ICON_GAP: f32 = 6.0;
/// How wide the symbols are drawn (the battery, with its tip, a little wider).
const ICON: f32 = 16.0;
const BATT_ICON: f32 = 20.0;
/// The launcher's icon, when it's a picture (the system's logo, or a custom one).
const LAUNCHER_ICON: f32 = 20.0;
/// Between zones.
const ZONE_GAP: f32 = 14.0;
/// The narrowest a squeezed item (window title, what's playing) gets before it's left out.
const MIN_FLEX: f32 = 70.0;
/// Down a side panel: a symbol with its number under it, and the smaller text used there.
const STACK: f32 = 40.0;
const SMALL: f32 = 10.5;

/// What drawing an item needs.
struct Bar<'a, 'b> {
    p: egui::Painter,
    v: egui::Visuals,
    accent: Color32,
    /// Hover fills' corner radius, with the panel's corners.
    hover_r: f32,
    cy: f32,
    /// Down a side panel: items stack, around `cx`.
    vert: bool,
    cx: f32,
    idx: usize,
    /// Where this panel's popups open: its edge, and how far from it.
    from: (Edge, f32),
    /// Its popups open joined to it.
    attach: bool,
    m: &'a Model<'b>,
    acts: Vec<Act>,
    /// The tray's icons, by item (made before drawing, as they need the surface's kit).
    tray: Vec<(String, Option<egui::TextureHandle>)>,
    /// The launcher's icon, when it isn't the grid (likewise).
    launcher: Option<egui::TextureHandle>,
    /// The cover of what's playing (likewise).
    cover: Option<egui::TextureHandle>,
    /// The dock item's apps and icons (likewise).
    dock: crate::dock_ui::Icons,
    /// Plugins' icons, by plugin (likewise).
    plug_icons: Vec<(String, Option<egui::TextureHandle>)>,
    /// The theme's colours, for groups painted in them, and a group's corner radius.
    roles: style::Roles,
    /// The theme's surface colour (the panel's own fill may be tinted or another colour).
    surface: Color32,
    group_r: f32,
    /// Between items, and how thick an item's cell is (the bar less 4 either side).
    gap: f32,
    cell: f32,
}

impl Bar<'_, '_> {
    fn fg(&self) -> Color32 {
        self.v.text_color()
    }
    fn weak(&self) -> Color32 {
        self.v.weak_text_color()
    }
    /// Where along the panel an item is: what its popup opens beside.
    fn at(&self, r: Rect) -> f32 {
        if self.vert { r.center().y } else { r.center().x }
    }
    fn popup(&self, kind: PopupKind, x: f32) -> Act {
        let mut p = Popup::at(kind, x, self.m.output);
        p.from = Some(self.from);
        p.attach = self.attach.then_some(self.idx);
        p.panel = Some(self.idx);
        Act::Open(p)
    }
    fn control(&self, x: f32) -> Act {
        self.popup(PopupKind::Control { drag: None }, x)
    }
    /// A clickable slot with a hover fill.
    fn button(&self, ui: &mut egui::Ui, r: Rect, id: &str) -> egui::Response {
        let resp = ui.interact(r, egui::Id::unique(("panel", self.idx, id)), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(r, self.hover_r, mix(self.v.panel_fill, self.v.text_color(), 0.09));
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }
    fn text_w(&self, text: String) -> f32 {
        self.p.layout_no_wrap(text, ui_font(12.5), self.fg()).size().x
    }
}

/// How wide an item wants to be, and whether it can be squeezed (None: nothing to show).
fn want(b: &Bar, id: &str) -> Option<(f32, bool)> {
    let st = b.m.status;
    let s = b.m.s;
    if let Some(pid) = id.strip_prefix("plugin:") {
        return plugin_want(b, pid);
    }
    // A sub panel's button, for as long as that sub panel is attached to a panel.
    if let Some(name) = id.strip_prefix("sub:") {
        return crate::panels::find_sub(&s.panels, name).filter(|i| s.panels[*i].on(b.m.output)).map(|_| (SLOT, false));
    }
    // Empty room, along the panel either way (less the gap either side of every item).
    if let Some(n) = crate::panels::spacer(id) {
        return Some(((n as f32 - b.gap * 2.0).max(0.0), false));
    }
    if b.vert {
        return want_down(b, id);
    }
    Some(match id {
        "launcher" | "network" | "quick-settings" | "power" | "user" => (SLOT, false),
        "bluetooth" => (if st.bluetooth.is_some() { SLOT } else { return None }, false),
        "ai" => {
            let parts = ai_parts(b);
            (if parts.is_empty() { SLOT } else { parts.iter().map(|p| ai_part_w(b, p)).sum::<f32>() + AI_SEP * (parts.len() - 1) as f32 + PAD * 2.0 }, false)
        }
        "notifications" => (if b.m.extra.notifying { SLOT } else { return None }, false),
        "dock" => (crate::dock_ui::width(&b.dock)?, false),
        "tray" => {
            let n = crate::tray_ui::shown(b.m).len();
            (if n == 0 { return None } else { n as f32 * crate::tray_ui::SLOT + 4.0 }, false)
        }
        "workspaces" => {
            let ws = workspaces_here(b);
            if ws.is_empty() {
                return None;
            }
            let gap = ws_gap(s.ws_style);
            (ws.iter().map(|w| ws_w(b, &ws_label(b, w), w.active) + gap).sum::<f32>() - gap + WS_PAD * 2.0, false)
        }
        "window-title" => {
            if st.window.is_empty() {
                return None;
            }
            (b.text_w(st.window.clone()).min(360.0) + PAD * 2.0, true)
        }
        "clock" => (b.p.layout_no_wrap(Local::now().format(&s.clock_format).to_string(), ui_font(13.5), b.fg()).size().x + 22.0, false),
        "media" => {
            let md = shown_media(b)?;
            let text = media_text(b, md);
            let tw = if text.is_empty() { 0.0 } else { b.text_w(text).min(s.media_width as f32) + ICON_GAP };
            (PAD * 2.0 + media_lead(b) + tw + if media_viz(b) { VIZ_W + ICON_GAP } else { 0.0 } + if s.media_controls { MEDIA_SKIP * 2.0 + ICON_GAP } else { 0.0 }, tw > 0.0)
        }
        "system" => (b.text_w(system_text(b)) + PAD * 2.0 + ICON + ICON_GAP, false),
        "weather" => match &b.m.extra.weather.now {
            Some(n) => (b.text_w(s.weather_units.temp(n.temp)) + PAD * 2.0 + WEATHER_ICON + ICON_GAP, false),
            None => (SLOT, false),
        },
        "volume" => (level_w(b, s.vol_show, ICON, &volume_text(st.volume?, s.vol_show, false)), false),
        "brightness" => (level_w(b, s.bright_show, ICON, &format!("{}%", st.brightness?)), false),
        "battery" => {
            let (pct, charging) = st.battery?;
            if s.batt_hide_full && charging && pct >= 99 {
                return None;
            }
            (level_w(b, s.batt_show, BATT_ICON, &format!("{pct}%")), false)
        }
        _ => return None,
    })
}

/// How tall an item wants to be down a side panel, where there's only the bar's width: symbols
/// stack over their numbers, the clock breaks into lines, the window's title runs downward.
fn want_down(b: &Bar, id: &str) -> Option<(f32, bool)> {
    let st = b.m.status;
    let s = b.m.s;
    Some(match id {
        "launcher" | "network" | "quick-settings" | "power" | "user" => (SLOT, false),
        "bluetooth" => (if st.bluetooth.is_some() { SLOT } else { return None }, false),
        "notifications" => (if b.m.extra.notifying { SLOT } else { return None }, false),
        "ai" => {
            let n = ai_parts(b).len();
            (if n == 0 { SLOT } else { n as f32 * STACK + 4.0 * (n - 1) as f32 }, false)
        }
        "tray" => {
            let n = crate::tray_ui::shown(b.m).len();
            (if n == 0 { return None } else { n as f32 * crate::tray_ui::SLOT + 4.0 }, false)
        }
        "workspaces" => {
            let ws = workspaces_here(b);
            if ws.is_empty() {
                return None;
            }
            let gap = ws_gap(s.ws_style);
            (ws.iter().map(|w| ws_w(b, &ws_label(b, w), w.active) + gap).sum::<f32>() - gap + WS_PAD * 2.0, false)
        }
        "window-title" => {
            if st.window.is_empty() {
                return None;
            }
            (b.text_w(st.window.clone()).min(300.0) + PAD * 2.0, true)
        }
        "clock" => (clock_lines(&Local::now().format(&s.clock_format).to_string()).len() as f32 * CLOCK_LINE + 10.0, false),
        "media" => (if shown_media(b).is_some() { SLOT } else { return None }, false),
        "system" => (STACK, false),
        "weather" => (if b.m.extra.weather.now.is_some() { STACK } else { SLOT }, false),
        "volume" => (level_w(b, s.vol_show, ICON, &volume_text(st.volume?, s.vol_show, true)), false),
        "brightness" => (level_w(b, s.bright_show, ICON, &format!("{}%", st.brightness?)), false),
        "battery" => {
            let (pct, charging) = st.battery?;
            if s.batt_hide_full && charging && pct >= 99 {
                return None;
            }
            (level_w(b, s.batt_show, BATT_ICON, &format!("{pct}%")), false)
        }
        _ => return None,
    })
}

/// The clock down a side panel: a line for each part of the time ("14:05" as 14 over 05).
fn clock_lines(text: &str) -> Vec<String> {
    text.split(|c: char| c == ':' || c.is_whitespace()).filter(|p| !p.is_empty()).map(String::from).collect()
}
const CLOCK_LINE: f32 = 15.0;

/// Text in the bar's width down a side panel: centred at `c`, made smaller when it's too wide.
fn small(p: &egui::Painter, c: Pos2, text: &str, size: f32, col: Color32) {
    let g = p.layout_no_wrap(text.to_string(), ui_font(size), col);
    let g = if g.size().x > 27.0 { p.layout_no_wrap(text.to_string(), ui_font((size * 27.0 / g.size().x).max(7.5)), col) } else { g };
    p.galley(c - g.size() / 2.0, g, col);
}

/// One part of the AI item: a provider's logo, a short name when several of its limits are
/// pinned ("5h"), its percent used or left, and how used it is.
struct AiPart {
    id: &'static str,
    tag: String,
    val: String,
    used: f32,
}

/// The volume's text. Beside its symbol (which shows the mute) it stays a percent, so muting
/// doesn't move the panel's items; alone, it says "Muted".
fn volume_text((v, muted): (u32, bool), show: Show, down: bool) -> String {
    if muted && show == Show::Text && !down { "Muted".into() } else { format!("{v}%") }
}

/// A level item's width: its symbol alone, its percent alone, or both.
fn level_w(b: &Bar, show: Show, icon_w: f32, text: &str) -> f32 {
    if b.vert {
        return match show {
            Show::Icon => SLOT,
            Show::Text => 24.0,
            Show::Both => STACK,
        };
    }
    match show {
        Show::Icon => SLOT,
        Show::Text => b.text_w(text.to_string()) + PAD * 2.0,
        Show::Both => b.text_w(text.to_string()) + PAD * 2.0 + icon_w + ICON_GAP,
    }
}

/// Draw a level item in `r`: its symbol (`icon_w` wide, drawn by `icon` around the point given)
/// and its percent.
#[allow(clippy::too_many_arguments)]
fn level(p: &egui::Painter, r: Rect, down: bool, show: Show, icon_w: f32, text: &str, col: Color32, icon: impl FnOnce(Pos2)) {
    let cy = r.center().y;
    if down {
        // The symbol over its number (no percent sign: there's no room).
        let num = text.trim_end_matches('%');
        match show {
            Show::Icon => icon(r.center()),
            Show::Text => small(p, r.center(), num, 12.0, col),
            Show::Both => {
                icon(Pos2::new(r.center().x, r.top() + 13.0));
                small(p, Pos2::new(r.center().x, r.bottom() - 9.0), num, SMALL, col);
            }
        }
        return;
    }
    match show {
        Show::Icon => icon(r.center()),
        Show::Both => icon(Pos2::new(r.left() + PAD + icon_w / 2.0, cy)),
        Show::Text => {}
    }
    if show.text() {
        let g = p.layout_no_wrap(text.to_string(), ui_font(12.5), col);
        let x = if show == Show::Both { r.left() + PAD + icon_w + ICON_GAP } else { r.center().x - g.size().x / 2.0 };
        p.galley(Pos2::new(x, cy - g.size().y / 2.0), g, col);
    }
}

/// What the AI item shows: each pinned limit, else the most used limit alone.
fn ai_parts(b: &Bar) -> Vec<AiPart> {
    let q = &b.m.extra.ai;
    let pct = |used: f32| format!("{:.0}%", if b.m.s.ai_left { 100.0 - used } else { used });
    let mut pins = q.pinned(&b.m.s.ai_pins);
    if pins.is_empty() {
        pins.extend(q.headline(&b.m.s.ai_bar));
    }
    pins.iter()
        .map(|(p, w)| {
            let twice = pins.iter().filter(|(o, _)| o.id == p.id).count() > 1;
            AiPart { id: p.id, tag: if twice { crate::ai::short(&w.name) } else { String::new() }, val: pct(w.used), used: w.used }
        })
        .collect()
}

const AI_LOGO: f32 = ICON;
/// The play or pause symbol.
const MEDIA_ICON: f32 = 12.0;
/// Either side of the workspaces.
const WS_PAD: f32 = 4.0;
/// Between parts: room, a thin line, room.
const AI_SEP: f32 = 17.0;
/// The Weather item's sky.
const WEATHER_ICON: f32 = 18.0;

fn ai_part_w(b: &Bar, part: &AiPart) -> f32 {
    let tw = |t: &str, size: f32| b.p.layout_no_wrap(t.to_string(), ui_font(size), b.fg()).size().x;
    AI_LOGO + ICON_GAP + if part.tag.is_empty() { 0.0 } else { tw(&part.tag, 11.5) + 4.0 } + tw(&part.val, 12.5)
}

/// The cover, as the panel shows it.
const COVER: f32 = 20.0;
/// The visualizer beside the text: this many bars.
const VIZ_BARS: usize = 8;
const VIZ_W: f32 = VIZ_BARS as f32 * 5.0 - 2.0;
/// Previous and next, each.
const MEDIA_SKIP: f32 = 22.0;

/// What's playing, when the item shows it (not while paused, where it's set to hide then).
fn shown_media<'a>(b: &'a Bar) -> Option<&'a crate::sys::Media> {
    b.m.status.media.as_ref().filter(|md| md.playing || !b.m.s.media_hide_paused)
}

fn media_text(b: &Bar, md: &crate::sys::Media) -> String {
    b.m.s.media_text.of(&md.title, &md.artist)
}

/// How wide the item's picture or symbol is.
fn media_lead(b: &Bar) -> f32 {
    if b.cover.is_some() { COVER } else { MEDIA_ICON }
}

/// Whether the bars show: turned on, and this computer can listen.
fn media_viz(b: &Bar) -> bool {
    b.m.s.media_viz && b.m.extra.viz.is_some()
}

/// A picture with rounded corners, filling `r`.
pub fn rounded_image(p: &egui::Painter, t: &egui::TextureHandle, r: Rect, radius: f32, tint: Color32) {
    // The middle of a picture that isn't `r`'s shape, not the whole of it squeezed.
    let (tw, th) = (t.size()[0].max(1) as f32, t.size()[1].max(1) as f32);
    let k = (tw / th) / (r.width() / r.height().max(1.0));
    let (u, v) = if k > 1.0 { (0.5 / k, 0.5) } else { (0.5, 0.5 * k) };
    p.add(egui::epaint::RectShape::filled(r, radius, tint).with_texture(t.id(), Rect::from_min_max(Pos2::new(0.5 - u, 0.5 - v), Pos2::new(0.5 + u, 0.5 + v))));
}

/// Bars for the sound's levels, standing on `r`'s bottom edge and filling its width.
pub fn viz_bars(p: &egui::Painter, r: Rect, levels: &[f32], col: Color32) {
    let n = levels.len().max(1) as f32;
    let step = r.width() / n;
    let w = (step * 0.62).max(1.5);
    for (i, l) in levels.iter().enumerate() {
        let h = (l.clamp(0.0, 1.0) * r.height()).max(2.0);
        let x = r.left() + i as f32 * step + (step - w) / 2.0;
        p.rect_filled(Rect::from_min_max(Pos2::new(x, r.bottom() - h), Pos2::new(x + w, r.bottom())), (w / 2.0).min(2.0), col);
    }
}

/// The Now Playing item along a panel: the cover or play symbol, the track, bars moving with
/// the sound, and previous and next, each as the item's options say.
fn media_item(ui: &mut egui::Ui, b: &mut Bar, r: Rect, id: &str) {
    let Some(md) = shown_media(b).cloned() else { return };
    let (fg, accent, cy, s) = (b.fg(), b.accent, b.cy, b.m.s);
    let p = b.p.clone();
    let resp = b.button(ui, r, id);
    let mut x = r.left() + PAD;
    // Its end first, so the text takes what's left.
    let mut right = r.right() - PAD;
    if s.media_controls {
        for (k, (cmd, tip)) in [("next", "Next"), ("previous", "Previous")].into_iter().enumerate() {
            let br = Rect::from_center_size(Pos2::new(right - MEDIA_SKIP / 2.0, cy), Vec2::new(MEDIA_SKIP, b.cell.min(24.0)));
            right -= MEDIA_SKIP;
            let sr = ui.interact(br, egui::Id::unique(("panel-media", b.idx, cmd)), Sense::click());
            if sr.hovered() {
                p.rect_filled(br, b.hover_r, mix(b.v.panel_fill, fg, 0.16));
            }
            glyph::skip(&p, br.center(), k == 0, if sr.hovered() { b.v.strong_text_color() } else { fg });
            if sr.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                b.acts.push(Act::Media(cmd));
            }
        }
        right -= ICON_GAP;
    }
    if media_viz(b) {
        let vr = Rect::from_min_max(Pos2::new(right - VIZ_W, cy - (b.cell.min(24.0) - 6.0) / 2.0), Pos2::new(right, cy + (b.cell.min(24.0) - 6.0) / 2.0));
        let levels = crate::viz::bars(b.m.extra.viz.as_deref().unwrap_or(&[]), VIZ_BARS);
        viz_bars(&p, vr, &levels, if md.playing { accent } else { b.weak() });
        right -= VIZ_W + ICON_GAP;
    }
    match &b.cover {
        Some(t) => {
            let cr = Rect::from_center_size(Pos2::new(x + COVER / 2.0, cy), Vec2::splat(COVER.min(b.cell)));
            rounded_image(&p, t, cr, 4.0, if md.playing { Color32::WHITE } else { Color32::from_gray(120) });
            if !md.playing {
                glyph::play(&p, cr.center(), false, Color32::WHITE);
            }
            x += COVER;
        }
        None => {
            glyph::play(&p, Pos2::new(x + MEDIA_ICON / 2.0, cy), !md.playing, accent);
            x += MEDIA_ICON;
        }
    }
    let text = media_text(b, &md);
    if !text.is_empty() && right - x - ICON_GAP > 12.0 {
        let g = one_line(&p, &text, ui_font(12.5), fg, right - x - ICON_GAP);
        p.galley(Pos2::new(x + ICON_GAP, cy - g.size().y / 2.0), g, fg);
    }
    media_clicks(ui, b, r, &resp, &md);
}

/// What a click, a middle click and a scroll do on the Now Playing item.
fn media_clicks(ui: &egui::Ui, b: &mut Bar, r: Rect, resp: &egui::Response, md: &crate::sys::Media) {
    let s = b.m.s;
    if s.media_scroll && resp.hovered() {
        let dy = wheel(ui);
        if dy.abs() > 0.5 {
            b.acts.push(Act::Media(if dy > 0.0 { "previous" } else { "next" }));
        }
    }
    let toggle = if md.playing { "pause" } else { "play" };
    let line = MediaText::TitleArtist.of(&md.title, &md.artist);
    let tip = if s.media_click_plays { format!("{line} · {} · click to {toggle}", md.player) } else { format!("{line} · {} · middle-click to {toggle}", md.player) };
    if resp.middle_clicked() || (s.media_click_plays && resp.clicked()) {
        b.acts.push(Act::Media("play-pause"));
    } else if resp.clicked() {
        let a = b.popup(PopupKind::Media, b.at(r));
        b.acts.push(a);
    }
    resp.clone().on_hover_text(tip);
}

fn system_text(b: &Bar) -> String {
    let (used, _) = b.m.status.mem;
    format!("{:.0}%  ·  {:.1} GB", b.m.status.cpu, used as f32 / (1u64 << 30) as f32)
}

/// This screen's workspaces, less the empty ones when they're hidden (the active one stays).
fn workspaces_here<'a>(b: &Bar<'a, '_>) -> Vec<&'a crate::sys::Workspace> {
    let out = b.m.output;
    let hide = b.m.s.ws_hide_empty;
    b.m.status.workspaces.iter().filter(|w| (out.is_empty() || w.output.is_empty() || w.output == out) && (!hide || w.busy || w.active)).collect()
}

/// What a workspace shows: its name when it has one (and names are wanted), else its number.
fn ws_label(b: &Bar, w: &crate::sys::Workspace) -> String {
    let named = b.m.s.ws_names && !w.label.is_empty();
    // Down a side panel a name has to fit the bar's width.
    if named && !(b.vert && b.text_w(w.label.clone()) > 20.0) { w.label.clone() } else { w.key.clone() }
}

fn ws_gap(style: WsStyle) -> f32 {
    match style {
        WsStyle::Pills => 4.0,
        WsStyle::Squares => 3.0,
        WsStyle::Dots => 2.0,
        WsStyle::Numbers => 0.0,
    }
}

/// A workspace's width in the panel.
fn ws_w(b: &Bar, label: &str, active: bool) -> f32 {
    let text = || b.p.layout_no_wrap(label.to_string(), ui_font(12.5), b.fg()).size().x;
    if b.vert {
        // How tall, down a side panel.
        return match b.m.s.ws_style {
            WsStyle::Pills => if active { 30.0 } else { 22.0 },
            WsStyle::Dots => if active { 30.0 } else { 14.0 },
            WsStyle::Numbers | WsStyle::Squares => 22.0,
        };
    }
    match b.m.s.ws_style {
        WsStyle::Pills => {
            if active { (text() + 26.0).max(38.0) } else { (text() + 14.0).max(24.0) }
        }
        WsStyle::Dots => {
            if active { 30.0 } else { 14.0 }
        }
        WsStyle::Numbers => (text() + 14.0).max(22.0),
        // Square unless a name needs more.
        WsStyle::Squares => (text() + 10.0).max(22.0),
    }
}

/// Fit a zone's items into `room`: squeezable ones shrink first, then items are left out
/// (from the end away from `keep_start`'s side).
fn fit(items: &mut Vec<(String, f32, bool)>, room: f32, keep_start: bool, gap: f32) {
    let total = |it: &Vec<(String, f32, bool)>| it.iter().map(|i| i.1).sum::<f32>() + gap * it.len().saturating_sub(1) as f32;
    let mut over = total(items) - room;
    for it in items.iter_mut().filter(|i| i.2) {
        if over <= 0.0 {
            break;
        }
        let cut = (it.1 - MIN_FLEX).max(0.0).min(over);
        it.1 -= cut;
        over -= cut;
    }
    while total(items) > room && !items.is_empty() {
        if keep_start {
            items.pop();
        } else {
            items.remove(0);
        }
    }
}

pub fn panel(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit, idx: usize) {
    let Some(cfg) = m.s.panels.get(idx).cloned() else { return };
    let look = m.s.panel_look(idx);
    let surface = ui.visuals().panel_fill;
    let (v, accent) = style::visuals(ui.visuals(), &look, &style::Roles::new(&kit.look, m.s));
    *ui.visuals_mut() = v.clone();
    let full = ui.max_rect();
    // The bar sits against its edge (a floating one with room around it).
    let bar = crate::panels::bar_rect(&m.s.panels, idx, full);
    let mut radius = if cfg.floating { style::radius(&look) } else { egui::CornerRadius::ZERO };
    // A drawer under its host: square where they meet, and nothing of it (its shadow) drawn
    // over the host.
    let joined = crate::panels::joined(&m.s.panels, idx, m.output);
    let mut clip = ui.clip_rect();
    if joined {
        match cfg.edge {
            Edge::Top => (radius.nw, radius.ne, clip.min.y) = (0, 0, clip.min.y.max(bar.top())),
            Edge::Bottom => (radius.sw, radius.se, clip.max.y) = (0, 0, clip.max.y.min(bar.bottom())),
            Edge::Left => (radius.nw, radius.sw, clip.min.x) = (0, 0, clip.min.x.max(bar.left())),
            Edge::Right => (radius.ne, radius.se, clip.max.x) = (0, 0, clip.max.x.min(bar.right())),
        }
    }
    // A side panel's surface is too narrow for hover text: it would wrap a word to a line.
    // (Only on a panel's own surface, which is all panel: narrow, or flat.)
    let screen = ui.ctx().content_rect();
    if screen.width() < 100.0 || screen.height() < 100.0 {
        let delay = if screen.width() < 100.0 { f32::INFINITY } else { 0.5 };
        ui.ctx().all_styles_mut(|s| s.interaction.tooltip_delay = delay);
    }
    // A popup joined to it: no outline or shadow there (the popup's coordinates are the
    // screen's; a side panel's surface starts under the top panels).
    let open = m.extra.join.filter(|j| j.0 == idx).map(|(_, lo, hi)| {
        let off = if cfg.vertical() { crate::panels::room(&m.s.panels, m.output, Edge::Top) - full.top() } else { 0.0 };
        let side = match cfg.edge {
            Edge::Top => Edge::Bottom,
            Edge::Bottom => Edge::Top,
            Edge::Left => Edge::Right,
            Edge::Right => Edge::Left,
        };
        (side, lo - off, hi - off, style::radius(&look).nw as f32)
    });
    crate::ui::surface_open(&ui.painter().with_clip_rect(clip), bar, &v, look.opacity, radius, look.border, open);
    if joined && cfg.lead > 0.0 {
        fillets(ui.painter(), bar, cfg.edge, cfg.lead, v.panel_fill.gamma_multiply(look.opacity));
    }
    let reach = crate::panels::reach(&m.s.panels, idx, m.output);
    let mut acts = Vec::new();
    {
        let hover_r = if cfg.floating { style::inner_radius(&look, 4.0) } else { rivet_look::theme::r_md() as f32 };
        let mask = m.s.tray_color != crate::config::TrayColor::Own;
        let tray = if cfg.has("tray") { crate::tray_ui::shown(m).into_iter().map(|it| (it.key.clone(), crate::tray_ui::texture(kit, ui.ctx(), it, mask))).collect() } else { Vec::new() };
        let launcher = if cfg.has("launcher") { kit.launcher_icon(ui.ctx(), m.s, LAUNCHER_ICON) } else { None };
        let dock = if cfg.has("dock") { crate::dock_ui::icons(kit, ui.ctx(), m) } else { Vec::new() };
        let cover = m.status.media.as_ref().filter(|md| m.s.media_art && cfg.has("media") && !md.art.is_empty()).and_then(|md| kit.named_icon(ui.ctx(), &md.art, COVER));
        let plug_icons = m.extra.plugins.iter().filter(|(id, it)| !it.icon.is_empty() && cfg.has(&format!("plugin:{id}"))).map(|(id, it)| (id.clone(), if m.s.plugin_icon(id) == crate::config::TrayColor::Own { kit.named_icon(ui.ctx(), &it.icon, ICON) } else { kit.named_mask(ui.ctx(), &it.icon, ICON) })).collect();
        let roles = style::Roles::new(&kit.look, m.s);
        let group_r = if rivet_look::theme::square() { 0.0 } else if cfg.floating { (look.radius - 4.0).clamp(0.0, 14.0) } else { 8.0 };
        let mut b = Bar { p: ui.painter().clone(), v, accent, hover_r, cy: bar.center().y, vert: cfg.vertical(), cx: bar.center().x, idx, from: (cfg.edge, if look.attach { reach - cfg.gap() } else { reach + 4.0 }), attach: look.attach, m: &*m, acts: Vec::new(), tray, dock, launcher, cover, plug_icons, roles, surface, group_r, gap: cfg.spacing(), cell: (cfg.bar() - 8.0).max(12.0) };
        layout(ui, &mut b, &cfg, bar);
        acts.append(&mut b.acts);
    }
    m.acts.extend(acts);
}

/// A drawer's fill under its host's rounded corners: at each end of the side toward the host,
/// the corner-shaped piece between the host's curve and a square corner, `r` points each way,
/// so the host's bar seems to run straight on into the drawer.
fn fillets(p: &egui::Painter, bar: Rect, edge: Edge, r: f32, fill: Color32) {
    // Toward the host, and along the side they share.
    let (back, along) = match edge {
        Edge::Top => (Vec2::new(0.0, -1.0), Vec2::new(1.0, 0.0)),
        Edge::Bottom => (Vec2::new(0.0, 1.0), Vec2::new(1.0, 0.0)),
        Edge::Left => (Vec2::new(-1.0, 0.0), Vec2::new(0.0, 1.0)),
        Edge::Right => (Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)),
    };
    let ends = match edge {
        Edge::Top => [(bar.left_top(), 1.0), (bar.right_top(), -1.0)],
        Edge::Bottom => [(bar.left_bottom(), 1.0), (bar.right_bottom(), -1.0)],
        Edge::Left => [(bar.left_top(), 1.0), (bar.left_bottom(), -1.0)],
        Edge::Right => [(bar.right_top(), 1.0), (bar.right_bottom(), -1.0)],
    };
    for (corner, dir) in ends {
        let inward = along * dir;
        // The host's corner curves about this point; the piece is what's outside the curve.
        let centre = corner + back * r + inward * r;
        let mut mesh = egui::Mesh::default();
        mesh.colored_vertex(corner, fill);
        const STEPS: usize = 12;
        for k in 0..=STEPS {
            let a = std::f32::consts::FRAC_PI_2 * k as f32 / STEPS as f32;
            // From straight out past the host's end, round to the middle of the shared side.
            mesh.colored_vertex(centre - inward * (r * a.cos()) - back * (r * a.sin()), fill);
        }
        for k in 1..=STEPS as u32 {
            mesh.add_triangle(0, k, k + 1);
        }
        p.add(egui::Shape::mesh(mesh));
    }
}

fn layout(ui: &mut egui::Ui, b: &mut Bar, cfg: &PanelCfg, bar: Rect) {
    // Along the panel: left to right, or top to bottom down a side panel.
    let vert = b.vert;
    let inner = if vert { bar.shrink2(Vec2::new(0.0, 6.0)) } else { bar.shrink2(Vec2::new(6.0, 0.0)) };
    let (start, end) = if vert { (inner.top(), inner.bottom()) } else { (inner.left(), inner.right()) };
    let room = end - start;
    let (cx, cy, gap, th) = (b.cx, b.cy, b.gap, b.cell);
    let cell = move |at: f32, len: f32| if vert { Rect::from_min_size(Pos2::new(cx - th / 2.0, at), Vec2::new(th, len)) } else { Rect::from_min_size(Pos2::new(at, cy - th / 2.0), Vec2::new(len, th)) };
    let ctx = ui.ctx().clone();
    let measure = |b: &Bar, zone: &[String]| -> Vec<(String, f32, bool)> { grouped(&ctx, b.idx, cfg, zone.iter().filter_map(|id| want(b, id).map(|(w, f)| (id.clone(), w.ceil(), f))).collect()) };
    let width = |it: &[(String, f32, bool)]| it.iter().map(|i| i.1).sum::<f32>() + gap * it.len().saturating_sub(1) as f32;
    let mut left = measure(b, &cfg.zones[0]);
    let mut center = measure(b, &cfg.zones[1]);
    let mut right = measure(b, &cfg.zones[2]);
    fit(&mut center, room, true, gap);
    let cw = width(&center);
    let (lw, rw) = (width(&left), width(&right));
    let (left_room, right_room) = if center.is_empty() {
        // The two sides share the bar; the right keeps what it needs first.
        let r = rw.min(room);
        ((room - r - ZONE_GAP).max(0.0), r.max(room - lw - ZONE_GAP))
    } else {
        let half = (room - cw) / 2.0 - ZONE_GAP;
        (half.max(0.0), half.max(0.0))
    };
    fit(&mut left, left_room, true, gap);
    fit(&mut right, right_room, false, gap);
    // Whole pixels, so the symbols' thin lines stay sharp.
    for it in left.iter_mut().chain(&mut center).chain(&mut right) {
        it.1 = it.1.floor();
    }
    let cw = width(&center);
    for (zone, from) in [(&left, start), (&center, (start + room / 2.0 - cw / 2.0).round()), (&right, (end - width(&right)).round())] {
        let mut x = from;
        let mut cells: Vec<(&str, Rect, Option<usize>)> = Vec::new();
        for (id, w, _) in zone {
            if id != GROUP_PAD {
                cells.push((id, cell(x, *w), group_at(cfg, id)));
            }
            x += w + gap;
        }
        // Each run of a group's items: its background, then the items in its colours.
        #[allow(clippy::type_complexity)]
        let mut runs: Vec<(std::ops::Range<usize>, Option<usize>, Option<Rect>, [bool; 2])> = Vec::new();
        let mut k = 0;
        while k < cells.len() {
            let g = cells[k].2;
            let n = if g.is_some() { cells[k..].iter().take_while(|c| c.2 == g).count() } else { 1 };
            // A group with a height of its own: its background that tall (the panel's whole
            // height at most), and its items no taller.
            let own = g.and_then(|g| cfg.groups.get(g)).and_then(|g| g.height).map(|h| (h as f32).min(cfg.bar()));
            if let Some(h) = own {
                for c in &mut cells[k..k + n] {
                    c.1 = across(c.1, h.min(th), vert);
                }
            }
            let run = &cells[k..k + n];
            let back = g.and_then(|g| cfg.groups.get(g)).filter(|g| g.bg.is_some()).map(|_| {
                let all = run.iter().fold(run[0].1, |a, c| a.union(c.1));
                let all = if vert { all.expand2(Vec2::new(0.0, GROUP_IN)) } else { all.expand2(Vec2::new(GROUP_IN, 0.0)) };
                own.map_or(all, |h| across(all, h, vert))
            });
            runs.push((k..k + n, g, back, [true, true]));
            k += n;
        }
        // Two groups' backgrounds side by side fuse: they meet half way, square where they do.
        if cfg.fuse {
            for i in 1..runs.len() {
                let (a, b) = runs.split_at_mut(i);
                let (a, b) = (a.last_mut().unwrap(), &mut b[0]);
                if let (Some(ra), Some(rb)) = (a.2.as_mut(), b.2.as_mut()) {
                    if vert {
                        let mid = ((ra.bottom() + rb.top()) / 2.0).round();
                        (ra.max.y, rb.min.y) = (mid, mid);
                    } else {
                        let mid = ((ra.right() + rb.left()) / 2.0).round();
                        (ra.max.x, rb.min.x) = (mid, mid);
                    }
                    (a.3[1], b.3[0]) = (false, false);
                }
            }
        }
        for (range, g, back, round) in runs {
            let run = &cells[range];
            let Some(group) = g.and_then(|g| cfg.groups.get(g)) else {
                draw(ui, b, run[0].0, run[0].1);
                continue;
            };
            let (bg, gv, gaccent) = group_look(b, group);
            if let (Some(bg), Some(back)) = (bg, back) {
                // Rounded at its own ends, square where it meets a neighbour.
                let r = b.group_r.min(if vert { back.width() } else { back.height() } / 2.0).round() as u8;
                let (s, e) = (if round[0] { r } else { 0 }, if round[1] { r } else { 0 });
                let radius = if vert { egui::CornerRadius { nw: s, ne: s, sw: e, se: e } } else { egui::CornerRadius { nw: s, sw: s, ne: e, se: e } };
                if group.gradient {
                    fade(&b.p, back, radius, bg, vert);
                } else {
                    b.p.rect_filled(back, radius, bg);
                }
            }
            let panel_v = std::mem::replace(&mut b.v, gv.clone());
            let panel_accent = std::mem::replace(&mut b.accent, gaccent);
            let ui_v = std::mem::replace(ui.visuals_mut(), gv);
            for (id, r, _) in run {
                match id.strip_prefix(GROUP_MORE).and_then(|g| g.parse::<usize>().ok()) {
                    Some(g) => more(ui, b, g, *r),
                    None => draw(ui, b, id, *r),
                }
            }
            b.v = panel_v;
            b.accent = panel_accent;
            *ui.visuals_mut() = ui_v;
        }
    }
}

/// A plugin's item, when it has something to show: its icon and text.
fn plugin_item<'a>(b: &Bar<'a, '_>, pid: &str) -> Option<&'a crate::plugins::Item> {
    crate::plugins::item(&b.m.extra.plugins, pid).filter(|it| !it.hidden && (!it.text.is_empty() || !it.icon.is_empty() || it.failed))
}

fn plugin_icon<'a>(b: &'a Bar, pid: &str) -> Option<&'a egui::TextureHandle> {
    b.plug_icons.iter().find(|(i, _)| i == pid).and_then(|(_, t)| t.as_ref())
}

/// How long a plugin's text may run on the panel.
const PLUGIN_TEXT: f32 = 240.0;

fn plugin_want(b: &Bar, pid: &str) -> Option<(f32, bool)> {
    let it = plugin_item(b, pid)?;
    let icon = plugin_icon(b, pid).is_some();
    if b.vert {
        return Some((if icon && !it.text.is_empty() { STACK } else { SLOT }, false));
    }
    if it.text.is_empty() {
        return Some((SLOT, false));
    }
    Some((b.text_w(it.text.clone()).min(PLUGIN_TEXT) + PAD * 2.0 + if icon { ICON + ICON_GAP } else { 0.0 }, false))
}

/// A plugin's item: its icon and text in its state's colour. A click opens its menu when it
/// has one, and every click and scroll is told to the plugin.
fn plugin_draw(ui: &mut egui::Ui, b: &mut Bar, pid: &str, r: Rect) {
    let Some(it) = plugin_item(b, pid) else { return };
    let resp = b.button(ui, r, &format!("plugin:{pid}"));
    let sc = rivet_look::theme::status(ui.ctx());
    let col = match it.state {
        _ if it.failed => b.weak(),
        crate::plugins::State::Normal => b.fg(),
        crate::plugins::State::Accent => b.accent,
        crate::plugins::State::Warning => sc.warning,
        crate::plugins::State::Danger => sc.danger,
    };
    let p = b.p.clone();
    let icon = plugin_icon(b, pid);
    let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    let text = if it.failed && it.text.is_empty() { "!" } else { it.text.as_str() };
    // The icon in its own colours, or in one of the panel's (Settings, the item's options).
    let tint = crate::tray_ui::tint(b.m.s.plugin_icon(pid), b.fg(), b.accent);
    if b.vert {
        match icon {
            Some(t) if !text.is_empty() => {
                p.image(t.id(), Rect::from_center_size(Pos2::new(r.center().x, r.top() + 13.0), Vec2::splat(ICON)), uv, tint);
                small(&p, Pos2::new(r.center().x, r.bottom() - 9.0), text, SMALL, col);
            }
            Some(t) => {
                p.image(t.id(), Rect::from_center_size(r.center(), Vec2::splat(ICON)), uv, tint);
            }
            None => small(&p, r.center(), text, 12.0, col),
        }
    } else {
        let mut x = r.left() + PAD;
        if let Some(t) = icon {
            let at = if text.is_empty() { r.center().x } else { x + ICON / 2.0 };
            p.image(t.id(), Rect::from_center_size(Pos2::new(at, r.center().y), Vec2::splat(ICON)), uv, tint);
            x += ICON + ICON_GAP;
        }
        if !text.is_empty() {
            let g = one_line(&p, text, ui_font(12.5), col, r.right() - PAD - x + 1.0);
            let at = if icon.is_none() { r.center().x - g.size().x / 2.0 } else { x };
            p.galley(Pos2::new(at, r.center().y - g.size().y / 2.0), g, col);
        }
    }
    // Something wants attention: a dot at the icon's corner (the item's, when it has no icon).
    if it.badge && !it.failed {
        let c = if b.vert || text.is_empty() || icon.is_none() { Pos2::new(r.center().x + 7.0, r.center().y - 7.0) } else { Pos2::new(r.left() + PAD + ICON - 1.0, r.center().y - 7.0) };
        let c = if icon.is_none() { Pos2::new(r.right() - 4.0, r.top() + 6.0) } else { c };
        p.circle_filled(c, 4.0, b.v.panel_fill);
        p.circle_filled(c, 3.0, b.accent);
    }
    let tip = if it.failed { format!("{} stopped", it.name) } else if it.tooltip.is_empty() { it.name.clone() } else { it.tooltip.clone() };
    let has_menu = !it.menu.is_empty();
    let hovered = resp.hovered();
    let resp = resp.on_hover_text(tip);
    let click = |button: &str| Act::Plugin(pid.to_string(), crate::plugins::Event::new("click", &[("button", button)]));
    if resp.clicked() {
        b.acts.push(click("left"));
        if has_menu {
            let a = b.popup(PopupKind::Plugin { id: pid.to_string() }, b.at(r));
            b.acts.push(a);
        }
    } else if resp.middle_clicked() {
        b.acts.push(click("middle"));
    } else if resp.secondary_clicked() {
        b.acts.push(click("right"));
    }
    let dy = if hovered { wheel(ui) } else { 0.0 };
    if dy != 0.0 {
        b.acts.push(Act::Plugin(pid.to_string(), crate::plugins::Event::new("scroll", &[("direction", if dy > 0.0 { "up" } else { "down" })])));
    }
}

/// A space either side of a group's background, and the arrow that folds a group's items
/// away ("group-more:" and the group's number): items the layout adds to a zone.
const GROUP_PAD: &str = "group-pad";
const GROUP_MORE: &str = "group-more:";
/// How far a group's background reaches past its items.
const GROUP_IN: f32 = 4.0;

/// `r` made `size` across the panel (its height on a bar, its width on a side panel), about its
/// middle.
fn across(r: Rect, size: f32, vert: bool) -> Rect {
    if vert { Rect::from_center_size(r.center(), Vec2::new(size, r.height())) } else { Rect::from_center_size(r.center(), Vec2::new(r.width(), size)) }
}

/// The group a zone's item is drawn in.
fn group_at(cfg: &PanelCfg, id: &str) -> Option<usize> {
    match id.strip_prefix(GROUP_MORE) {
        Some(g) => g.parse().ok(),
        None => cfg.group_of(id),
    }
}

fn group_open_id(panel: usize, group: usize) -> egui::Id {
    egui::Id::unique(("panel-group-open", panel, group))
}

/// A zone's measured items with its groups worked in: a group that shows only some of its
/// items loses the rest (until its arrow is clicked) and gains the arrow, and a group with a
/// background gets room at its ends.
fn grouped(ctx: &egui::Context, panel: usize, cfg: &PanelCfg, items: Vec<(String, f32, bool)>) -> Vec<(String, f32, bool)> {
    if cfg.groups.is_empty() {
        return items;
    }
    let mut out = Vec::with_capacity(items.len() + 4);
    let mut k = 0;
    while k < items.len() {
        let g = cfg.group_of(&items[k].0);
        let n = items[k..].iter().take_while(|i| cfg.group_of(&i.0) == g).count();
        let Some((gi, group)) = g.and_then(|g| cfg.groups.get(g).map(|x| (g, x))) else {
            out.push(items[k].clone());
            k += 1;
            continue;
        };
        let run = &items[k..k + n];
        k += n;
        let pad = group.bg.is_some();
        if pad {
            out.push((GROUP_PAD.to_string(), GROUP_IN, false));
        }
        let show = group.show as usize;
        if show > 0 && run.len() > show {
            let open = ctx.data(|d| d.get_temp::<bool>(group_open_id(panel, gi))).unwrap_or(false);
            out.extend(run[..if open { run.len() } else { show }].iter().cloned());
            out.push((format!("{GROUP_MORE}{gi}"), 18.0, false));
        } else {
            out.extend(run.iter().cloned());
        }
        if pad {
            out.push((GROUP_PAD.to_string(), GROUP_IN, false));
        }
    }
    out
}

/// A group's gradient background: its colour fading along the panel to its darker shade.
/// The rounded rectangle is tessellated as egui would a plain one (so its edge is as smooth),
/// and each corner of the mesh then takes the gradient's colour where it is.
fn fade(p: &egui::Painter, r: Rect, radius: egui::CornerRadius, bg: Color32, down: bool) {
    let [dr, dg, db] = crate::panels::darker([bg.r(), bg.g(), bg.b()]);
    let to = Color32::from_rgb(dr, dg, db);
    let shape = egui::Shape::rect_filled(r, radius, Color32::WHITE);
    let ctx = p.ctx();
    let prims = ctx.tessellate(vec![egui::epaint::ClippedShape { clip_rect: p.clip_rect(), shape, transform_after_tessellation: egui::emath::TSTransform::IDENTITY }], ctx.pixels_per_point());
    for prim in prims {
        let egui::epaint::Primitive::Mesh(mut mesh) = prim.primitive else { continue };
        for v in &mut mesh.vertices {
            let t = if down { (v.pos.y - r.top()) / r.height().max(1.0) } else { (v.pos.x - r.left()) / r.width().max(1.0) };
            // The smoothing's outer corners are see-through, and stay so.
            v.color = mix(bg, to, t.clamp(0.0, 1.0)).gamma_multiply(v.color.a() as f32 / 255.0);
        }
        p.add(egui::Shape::mesh(mesh));
    }
}

/// A group's colours: its background (None: none), the visuals its items draw with, and
/// their highlight.
fn group_look(b: &Bar, g: &crate::panels::Group) -> (Option<Color32>, egui::Visuals, Color32) {
    use crate::panels::Paint;
    let paint = |p: Paint| match p {
        Paint::Auto => None,
        Paint::Surface => Some(b.surface),
        Paint::Primary => Some(b.roles.primary),
        Paint::Secondary => Some(b.roles.secondary),
        Paint::Tertiary => Some(b.roles.tertiary),
        Paint::Custom([r, g, b]) => Some(Color32::from_rgb(r, g, b)),
    };
    let bg = g.bg.map(|p| paint(p).unwrap_or_else(|| mix(b.v.panel_fill, b.v.text_color(), 0.10)));
    // With a gradient, text has to read on the middle of it.
    let dark = bg.map(|c| crate::panels::darker([c.r(), c.g(), c.b()])).map(|[r, g, b]| Color32::from_rgb(r, g, b));
    let fill = match (bg, dark) {
        (Some(c), Some(d)) if g.gradient => mix(c, d, 0.5),
        _ => bg.unwrap_or(b.v.panel_fill),
    };
    // Left to the shell, text reads on a painted background and stays the panel's otherwise.
    let solid = g.bg.is_some_and(|p| p != Paint::Auto);
    let fg = paint(g.fg).unwrap_or_else(|| if solid { on(fill) } else { b.v.text_color() });
    let mut v = b.v.clone();
    v.panel_fill = fill;
    v.override_text_color = Some(fg);
    v.weak_text_color = Some(mix(fg, fill, 0.35));
    // A highlight that would vanish into the background takes the text's colour.
    let far = |a: Color32, c: Color32| (a.r() as i32 - c.r() as i32).abs() + (a.g() as i32 - c.g() as i32).abs() + (a.b() as i32 - c.b() as i32).abs();
    let accent = if far(b.accent, fill) < 120 { fg } else { b.accent };
    (bg, v, accent)
}

/// The arrow at the end of a group that folds: shows the rest of its items, or hides them.
fn more(ui: &mut egui::Ui, b: &mut Bar, group: usize, r: Rect) {
    let id = group_open_id(b.idx, group);
    let open = ui.ctx().data(|d| d.get_temp::<bool>(id)).unwrap_or(false);
    let resp = b.button(ui, r, &format!("{GROUP_MORE}{group}"));
    let c = r.center();
    // Pointing along the panel: toward the hidden items, or back.
    let d = if open { -1.0 } else { 1.0 };
    let pts = if b.vert { vec![c + Vec2::new(-4.0, -2.0 * d), c + Vec2::new(0.0, 2.0 * d), c + Vec2::new(4.0, -2.0 * d)] } else { vec![c + Vec2::new(-2.0 * d, -4.0), c + Vec2::new(2.0 * d, 0.0), c + Vec2::new(-2.0 * d, 4.0)] };
    b.p.add(egui::Shape::line(pts, egui::Stroke::new(1.6, if resp.hovered() { b.fg() } else { b.weak() })));
    if resp.on_hover_text(if open { "Show fewer" } else { "Show the rest" }).clicked() {
        ui.ctx().data_mut(|d| d.insert_temp(id, !open));
        ui.ctx().request_repaint();
    }
}

/// A sub panel's button: a bar with an arrow to where the sub panel rolls out (back toward the
/// bar while it's left open). A click opens or closes it; a right-click keeps it open or lets
/// it roll up by itself again.
fn sub_button(ui: &mut egui::Ui, b: &mut Bar, id: &str, name: &str, r: Rect) {
    let Some(sub) = crate::panels::find_sub(&b.m.s.panels, name).and_then(|i| b.m.s.panels[i].sub.clone()) else { return };
    let resp = b.button(ui, r, id);
    let col = if sub.stay && sub.open { b.accent } else { b.fg() };
    // Away from the screen's edge, where the sub panel is.
    let out = match b.m.s.panels.get(sub.host.unwrap_or(b.idx)).map(|p| p.edge).unwrap_or(Edge::Top) {
        Edge::Top => Vec2::new(0.0, 1.0),
        Edge::Bottom => Vec2::new(0.0, -1.0),
        Edge::Left => Vec2::new(1.0, 0.0),
        Edge::Right => Vec2::new(-1.0, 0.0),
    };
    let along = Vec2::new(out.y.abs(), out.x.abs());
    let c = r.center();
    let stroke = egui::Stroke::new(1.6, col);
    b.p.line_segment([c - out * 4.5 - along * 5.5, c - out * 4.5 + along * 5.5], stroke);
    let d = if sub.stay && sub.open { -1.0 } else { 1.0 };
    let tip = c + out * (1.5 + 2.0 * d);
    b.p.add(egui::Shape::line(vec![tip - out * 4.0 * d - along * 4.0, tip, tip - out * 4.0 * d + along * 4.0], stroke));
    let hint = if sub.stay { "Right-click to let it roll up by itself." } else { "Right-click to keep it open." };
    let resp = resp.on_hover_text(format!("{}. {hint}", sub.name));
    if resp.secondary_clicked() {
        b.acts.push(Act::SubPanel(sub.name.clone(), crate::ui::SubDo::Stay, b.m.output.to_string()));
    } else if resp.clicked() {
        b.acts.push(Act::SubPanel(sub.name.clone(), crate::ui::SubDo::Toggle, b.m.output.to_string()));
    }
}

fn wheel(ui: &egui::Ui) -> f32 {
    ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum())
}

/// Draw one item in `r`, and note what clicking it asks for.
fn draw(ui: &mut egui::Ui, b: &mut Bar, id: &str, r: Rect) {
    let (fg, weak, accent, cy) = (b.fg(), b.weak(), b.accent, b.cy);
    let p = b.p.clone();
    let st = b.m.status;
    let ex = b.m.extra;
    if let Some(pid) = id.strip_prefix("plugin:") {
        plugin_draw(ui, b, pid, r);
        return;
    }
    if let Some(name) = id.strip_prefix("sub:") {
        sub_button(ui, b, id, name, r);
        return;
    }
    // Down a side panel, the items with text draw another way.
    if b.vert && draw_down(ui, b, id, r) {
        return;
    }
    match id {
        "launcher" => {
            let resp = b.button(ui, r, id);
            match &b.launcher {
                Some(t) => {
                    p.image(t.id(), Rect::from_center_size(r.center(), Vec2::splat(LAUNCHER_ICON)), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                }
                // The grid; also when the system's or the custom icon can't be found.
                None => glyph::grid(&p, r.center(), fg),
            }
            if resp.on_hover_text("Apps").clicked() {
                let a = b.popup(PopupKind::Launcher { query: String::new(), sel: 0 }, b.at(r));
                b.acts.push(a);
            }
        }
        "workspaces" => {
            let style = b.m.s.ws_style;
            let ws = workspaces_here(b);
            let mut x = if b.vert { r.top() } else { r.left() } + WS_PAD;
            for w in &ws {
                let label = ws_label(b, w);
                let len = ws_w(b, &label, w.active);
                let (pw, ph) = (b.cell.min(24.0), b.cell.min(22.0));
                let pr = if b.vert { Rect::from_min_size(Pos2::new(r.center().x - pw / 2.0, x), Vec2::new(pw, len)) } else { Rect::from_min_size(Pos2::new(x, cy - ph / 2.0), Vec2::new(len, ph)) };
                x += len + ws_gap(style);
                if if b.vert { pr.bottom() > r.bottom() + 0.5 } else { pr.right() > r.right() + 0.5 } {
                    break;
                }
                let resp = ui.interact(pr, egui::Id::unique(("ws", b.idx, &w.key, &w.output)), Sense::click());
                let hot = resp.hovered();
                match style {
                    WsStyle::Pills | WsStyle::Squares => {
                        let (fill, col) = if w.active {
                            (accent, on(accent))
                        } else if hot {
                            (mix(b.v.panel_fill, fg, 0.16), fg)
                        } else {
                            (mix(b.v.panel_fill, fg, if w.busy { 0.1 } else { 0.05 }), if w.busy { fg } else { weak })
                        };
                        p.rect_filled(if style == WsStyle::Squares && b.vert { Rect::from_center_size(pr.center(), Vec2::splat(22.0)) } else { pr }, if style == WsStyle::Squares { 6.0 } else { 11.0 }, fill);
                        // Laid out in its colour (a galley keeps the colour it was made with).
                        let g = p.layout_no_wrap(label, ui_font(12.5), col);
                        p.galley(pr.center() - g.size() / 2.0, g, col);
                    }
                    WsStyle::Dots => {
                        let c = pr.center();
                        if w.active {
                            p.rect_filled(Rect::from_center_size(c, if b.vert { Vec2::new(8.0, 22.0) } else { Vec2::new(22.0, 8.0) }), 4.0, accent);
                        } else if w.busy {
                            p.circle_filled(c, if hot { 4.5 } else { 3.75 }, if hot { fg } else { fg.gamma_multiply(0.75) });
                        } else {
                            // Empty: a ring.
                            p.circle_stroke(c, if hot { 4.0 } else { 3.25 }, egui::Stroke::new(1.3, if hot { fg } else { weak.gamma_multiply(0.8) }));
                        }
                    }
                    WsStyle::Numbers => {
                        let col = if w.active {
                            accent
                        } else if hot || w.busy {
                            fg
                        } else {
                            weak.gamma_multiply(0.75)
                        };
                        if hot && !w.active {
                            p.rect_filled(pr, 6.0, mix(b.v.panel_fill, fg, 0.1));
                        }
                        let g = p.layout_no_wrap(label, ui_font(if w.active { 13.5 } else { 12.5 }), col);
                        p.galley(pr.center() - g.size() / 2.0 - Vec2::new(0.0, 1.0), g, col);
                        if w.active {
                            p.rect_filled(Rect::from_center_size(Pos2::new(pr.center().x, pr.bottom() - 0.5), Vec2::new(12.0, 2.5)), 1.25, accent);
                        }
                    }
                }
                let comp = b.m.compositor.name();
                let name = if w.label.is_empty() || w.label == w.key { format!("Workspace {} ({comp})", w.key) } else { format!("Workspace {} · {} ({comp})", w.key, w.label) };
                let tip = if b.m.s.ws_scroll { format!("{name} · scroll to switch") } else { name };
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tip).clicked() {
                    b.acts.push(Act::Workspace(w.key.clone()));
                }
            }
            // Scrolling over the item steps through the workspaces (at most one step per 150 ms,
            // so a touchpad's stream of small scrolls doesn't race to the end).
            if b.m.s.ws_scroll && ui.rect_contains_pointer(r) {
                let dy = wheel(ui);
                let now = ui.input(|i| i.time);
                let key = egui::Id::unique(("ws-scroll", b.idx));
                let last = ui.ctx().data(|d| d.get_temp::<f64>(key)).unwrap_or(f64::MIN);
                if dy.abs() > 0.5
                    && now - last > 0.15
                    && let Some(at) = ws.iter().position(|w| w.active)
                {
                    let to = if dy > 0.0 { at.checked_sub(1) } else { Some(at + 1).filter(|&i| i < ws.len()) };
                    if let Some(to) = to {
                        b.acts.push(Act::Workspace(ws[to].key.clone()));
                        ui.ctx().data_mut(|d| d.insert_temp(key, now));
                    }
                }
            }
        }
        "window-title" => {
            let g = one_line(&p, &st.window, ui_font(12.5), weak, r.width() - PAD * 2.0);
            p.galley(Pos2::new(r.left() + PAD, cy - g.size().y / 2.0), g, weak);
        }
        "clock" => {
            let now = Local::now();
            let resp = b.button(ui, r, id);
            let g = p.layout_no_wrap(now.format(&b.m.s.clock_format).to_string(), ui_font(13.5), fg);
            p.galley(r.center() - g.size() / 2.0, g, fg);
            if resp.on_hover_text(now.format("%A, %B %-d, %Y").to_string()).clicked() {
                let a = b.popup(PopupKind::Calendar { month: 0, day: None }, b.at(r));
                b.acts.push(a);
            }
        }
        "media" => media_item(ui, b, r, id),
        "system" => {
            let resp = ui.interact(r, egui::Id::unique(("panel", b.idx, id)), Sense::hover());
            glyph::chip(&p, Pos2::new(r.left() + PAD + ICON / 2.0, cy), fg);
            let g = p.layout_no_wrap(system_text(b), ui_font(12.5), fg);
            p.galley(Pos2::new(r.left() + PAD + ICON + ICON_GAP, cy - g.size().y / 2.0), g, fg);
            resp.on_hover_text("Processor use · memory in use");
        }
        "weather" => {
            use crate::weather::Sky;
            let w = &ex.weather;
            let units = b.m.s.weather_units;
            let resp = b.button(ui, r, id);
            let tip = match &w.now {
                Some(n) => {
                    let sky = Sky::of(n.code);
                    let temp = units.temp(n.temp);
                    if b.vert {
                        crate::weather_ui::icon(&p, Pos2::new(r.center().x, r.top() + 13.0), WEATHER_ICON, sky, n.day, fg, accent);
                        small(&p, Pos2::new(r.center().x, r.bottom() - 9.0), &temp, SMALL, fg);
                    } else {
                        crate::weather_ui::icon(&p, Pos2::new(r.left() + PAD + WEATHER_ICON / 2.0, cy), WEATHER_ICON, sky, n.day, fg, accent);
                        let g = p.layout_no_wrap(temp.clone(), ui_font(12.5), fg);
                        p.galley(Pos2::new(r.left() + PAD + WEATHER_ICON + ICON_GAP, cy - g.size().y / 2.0), g, fg);
                    }
                    let today = w.days.first().map(|d| format!(" · high {} low {}", units.temp(d.high), units.temp(d.low))).unwrap_or_default();
                    format!("{}\n{temp}, {}{today}\nClick for the forecast", w.place, crate::weather::words(n.code).to_lowercase())
                }
                None => {
                    crate::weather_ui::icon(&p, r.center(), WEATHER_ICON, Sky::Cloudy, true, weak.gamma_multiply(0.6), accent);
                    w.message().unwrap_or("Weather").to_string()
                }
            };
            if resp.on_hover_text(tip).clicked() {
                let a = b.popup(PopupKind::Weather, b.at(r));
                b.acts.push(a);
            }
        }
        "volume" => {
            let Some((vol, muted)) = st.volume else { return };
            let col = if muted { weak } else { fg };
            let resp = b.button(ui, r, id);
            let s = b.m.s;
            level(&p, r, b.vert, s.vol_show, ICON, &volume_text((vol, muted), s.vol_show, b.vert), col, |c| glyph::speaker(&p, c, muted, col));
            if s.vol_scroll && resp.hovered() {
                let dy = wheel(ui);
                if dy.abs() > 0.5 {
                    b.acts.push(Act::Volume(dy > 0.0));
                }
            }
            let now = if muted { "Muted".to_string() } else { format!("Volume {vol}%") };
            let click = if muted { "unmute" } else { "mute" };
            let tip = if s.vol_scroll { format!("{now} · click for sound devices · middle-click to {click} · scroll to change") } else { format!("{now} · click for sound devices · middle-click to {click}") };
            if resp.middle_clicked() {
                b.acts.push(Act::Mute);
            }
            if resp.on_hover_text(tip).clicked() {
                let a = b.popup(PopupKind::Sound { drag: None }, b.at(r));
                b.acts.push(a);
            }
        }
        "brightness" => {
            let Some(v) = st.brightness else { return };
            let resp = b.button(ui, r, id);
            let s = b.m.s;
            level(&p, r, b.vert, s.bright_show, ICON, &format!("{v}%"), fg, |c| glyph::sun(&p, c, fg));
            if s.bright_scroll && resp.hovered() {
                let dy = wheel(ui);
                if dy.abs() > 0.5 {
                    b.acts.push(Act::Brightness(dy > 0.0));
                }
            }
            let tip = if s.bright_scroll { format!("Brightness {v}% · scroll to change") } else { format!("Brightness {v}%") };
            if resp.on_hover_text(tip).clicked() {
                let a = b.control(b.at(r));
                b.acts.push(a);
            }
        }
        "network" => {
            let resp = b.button(ui, r, id);
            let tip = match &st.net {
                Net::Wifi(ssid) => {
                    glyph::wifi(&p, r.center(), fg);
                    if ssid.is_empty() { "Wi-Fi".to_string() } else { format!("Wi-Fi: {ssid}") }
                }
                Net::Wired => {
                    glyph::wired(&p, r.center(), fg);
                    "Wired".to_string()
                }
                Net::None => {
                    glyph::wifi(&p, r.center(), weak.gamma_multiply(0.5));
                    "Not connected".to_string()
                }
            };
            if resp.on_hover_text(format!("{tip} · click for networks")).clicked() {
                let a = b.popup(PopupKind::Network { ask: None }, b.at(r));
                b.acts.push(a);
            }
        }
        "ai" => {
            let q = &ex.ai;
            let resp = b.button(ui, r, id);
            let parts = ai_parts(b);
            let logo_at = |x: f32| Rect::from_center_size(Pos2::new(x + AI_LOGO / 2.0, cy), Vec2::splat(AI_LOGO));
            // Claude Code at work: a dot by Claude's logo (or the item's corner).
            let mut busy_at = None;
            if parts.is_empty() {
                let lr = Rect::from_center_size(r.center(), Vec2::splat(AI_LOGO));
                crate::ai_pop::logo(ui, &p, "claude", lr, fg);
                busy_at = Some(lr.right_top());
            } else {
                let sc = rivet_look::theme::status(ui.ctx());
                let mut x = r.left() + PAD;
                for (k, part) in parts.iter().enumerate() {
                    if k > 0 {
                        let lx = x + AI_SEP / 2.0 - 0.5;
                        p.line_segment([Pos2::new(lx, cy - 8.0), Pos2::new(lx, cy + 8.0)], egui::Stroke::new(1.0, mix(b.v.panel_fill, fg, 0.3)));
                        x += AI_SEP;
                    }
                    let lr = logo_at(x);
                    crate::ai_pop::logo(ui, &p, part.id, lr, fg);
                    if part.id == "claude" && busy_at.is_none() {
                        busy_at = Some(lr.right_top());
                    }
                    x += AI_LOGO + ICON_GAP;
                    if !part.tag.is_empty() {
                        let g = p.layout_no_wrap(part.tag.clone(), ui_font(11.5), weak);
                        p.galley(Pos2::new(x, cy - g.size().y / 2.0), g.clone(), weak);
                        x += g.size().x + 4.0;
                    }
                    let col = if part.used >= 90.0 { sc.danger } else if part.used >= 75.0 { sc.warning } else { fg };
                    let g = p.layout_no_wrap(part.val.clone(), ui_font(12.5), col);
                    p.galley(Pos2::new(x, cy - g.size().y / 2.0), g.clone(), col);
                    x += g.size().x;
                }
            }
            if q.claude_busy && let Some(c) = busy_at {
                p.circle_filled(c + Vec2::new(0.5, 1.5), 3.5, b.v.panel_fill);
                p.circle_filled(c + Vec2::new(0.5, 1.5), 2.5, accent);
            }
            let mut tip: Vec<String> = q
                .providers
                .iter()
                .filter(|p| p.state == crate::ai::State::Ok && !p.windows.is_empty())
                .map(|p| format!("{}: {}", crate::ai::name(p.id), p.windows.iter().map(|w| format!("{} {:.0}%", w.name, if b.m.s.ai_left { 100.0 - w.used } else { w.used })).collect::<Vec<_>>().join(" · ")))
                .collect();
            if q.claude_sessions > 0 {
                tip.push(format!("Claude Code: {} running{}", if q.claude_sessions == 1 { "1 session".to_string() } else { format!("{} sessions", q.claude_sessions) }, if q.claude_busy { ", working" } else { "" }));
            }
            if tip.is_empty() {
                tip.push("AI usage".into());
            }
            tip.push(format!("{} · click for details", if b.m.s.ai_left { "Left" } else { "Used" }));
            if resp.on_hover_text(tip.join("\n")).clicked() {
                let a = b.popup(PopupKind::Ai, b.at(r));
                b.acts.push(a);
            }
        }
        "bluetooth" => {
            let Some(on) = st.bluetooth else { return };
            let resp = b.button(ui, r, id);
            glyph::bluetooth(&p, r.center(), if on { fg } else { weak.gamma_multiply(0.6) }, on);
            let tip = if on { "Bluetooth on · click for devices" } else { "Bluetooth off · click for devices" };
            if resp.on_hover_text(tip).clicked() {
                let a = b.popup(PopupKind::Bluetooth, b.at(r));
                b.acts.push(a);
            }
        }
        "battery" => {
            let Some((pct, charging)) = st.battery else { return };
            let col = if pct <= 15 && !charging { rivet_look::theme::status(ui.ctx()).danger } else { fg };
            let resp = b.button(ui, r, id);
            let show = b.m.s.batt_show;
            // With no icon, the percent takes the accent while charging.
            let tcol = if show == Show::Text && charging { accent } else { col };
            level(&p, r, b.vert, show, BATT_ICON, &format!("{pct}%"), tcol, |c| glyph::battery(&p, c, pct, charging, col, accent));
            let mut tip = if charging { format!("Battery {pct}%, charging") } else { format!("Battery {pct}%") };
            if let Some(pr) = &st.profile {
                tip.push_str(&format!(" · {}", pr.replace('-', " ")));
            }
            if resp.on_hover_text(tip).clicked() {
                let a = b.control(b.at(r));
                b.acts.push(a);
            }
        }
        "dock" => {
            let popup = |kind: PopupKind, x: f32| b.popup(kind, x);
            let mut acts = Vec::new();
            crate::dock_ui::draw(ui, r, b.idx, b.m, &b.dock, accent, b.hover_r, &popup, &mut acts);
            b.acts.append(&mut acts);
        }
        "tray" => {
            let tint = crate::tray_ui::tint(b.m.s.tray_color, fg, accent);
            let from = b.from;
            let attach = b.attach.then_some(b.idx);
            let idx = b.idx;
            let output = b.m.output.to_string();
            let popup = move |key: String, x: f32| {
                let mut p = Popup::at(PopupKind::TrayMenu { key, path: Vec::new() }, x, &output);
                p.from = Some(from);
                p.attach = attach;
                p.panel = Some(idx);
                Act::Open(p)
            };
            let mut acts = Vec::new();
            crate::tray_ui::draw(ui, r, b.vert, b.idx, b.m, &b.tray, tint, accent, b.hover_r, &popup, &mut acts);
            b.acts.append(&mut acts);
        }
        "notifications" => {
            let resp = b.button(ui, r, id);
            glyph::bell(&p, r.center(), fg, ex.dnd);
            let n = ex.notes.len();
            if n > 0 && !ex.dnd {
                // The tertiary when the shell uses it (and this panel has no highlight of its own).
                let own = b.m.s.look.highlight.is_some() || b.m.s.panels.get(b.idx).is_some_and(|c| c.look.highlight.is_some());
                p.circle_filled(r.center() + Vec2::new(6.0, -6.0), 3.5, if own { accent } else { b.roles.tone(style::Slot::Notify, b.m.s.color_use) });
            }
            let tip = if ex.dnd { "Do Not Disturb".to_string() } else if n == 0 { "No notifications".to_string() } else { format!("{n} notification{}", if n == 1 { "" } else { "s" }) };
            if resp.on_hover_text(tip).clicked() {
                let a = b.popup(PopupKind::Notifications, b.at(r));
                b.acts.push(a);
            }
        }
        "quick-settings" => {
            let resp = b.button(ui, r, id);
            glyph::quick(&p, r.center(), fg, b.m.s.quick_icon);
            if resp.on_hover_text("Quick settings").clicked() {
                let a = b.control(b.at(r));
                b.acts.push(a);
            }
        }
        "user" => {
            let resp = b.button(ui, r, id);
            crate::user::draw(ui, r.center(), 22.0, &ex.user, accent);
            if resp.on_hover_text(format!("{}\nLock, log out, restart, shut down", ex.user)).clicked() {
                let a = b.popup(PopupKind::Power { confirm: None }, b.at(r));
                b.acts.push(a);
            }
        }
        "power" => {
            let resp = b.button(ui, r, id);
            glyph::power(&p, r.center(), fg);
            if resp.on_hover_text("Lock, suspend, restart, shut down").clicked() {
                let a = b.popup(PopupKind::Power { confirm: None }, b.at(r));
                b.acts.push(a);
            }
        }
        _ => {}
    }
}

/// The items that draw another way down a side panel (false: drawn as on any panel).
fn draw_down(ui: &mut egui::Ui, b: &mut Bar, id: &str, r: Rect) -> bool {
    let (fg, weak, accent) = (b.fg(), b.weak(), b.accent);
    let p = b.p.clone();
    let st = b.m.status;
    let c = r.center();
    match id {
        "window-title" => {
            // Running downward, read from the right.
            let g = one_line(&p, &st.window, ui_font(12.5), weak, r.height() - PAD * 2.0);
            let at = Pos2::new(c.x + g.size().y / 2.0, r.top() + PAD);
            p.add(egui::epaint::TextShape::new(at, g, weak).with_angle(std::f32::consts::FRAC_PI_2));
        }
        "clock" => {
            let now = Local::now();
            let resp = b.button(ui, r, id);
            let lines = clock_lines(&now.format(&b.m.s.clock_format).to_string());
            let top = c.y - lines.len() as f32 * CLOCK_LINE / 2.0;
            for (k, line) in lines.iter().enumerate() {
                small(&p, Pos2::new(c.x, top + (k as f32 + 0.5) * CLOCK_LINE), line, 13.0, fg);
            }
            if resp.on_hover_text(now.format("%A, %B %-d, %Y").to_string()).clicked() {
                let a = b.popup(PopupKind::Calendar { month: 0, day: None }, b.at(r));
                b.acts.push(a);
            }
        }
        "media" => {
            let Some(md) = shown_media(b).cloned() else { return true };
            let resp = b.button(ui, r, id);
            match &b.cover {
                Some(t) => rounded_image(&p, t, Rect::from_center_size(c, Vec2::splat(COVER)), 4.0, if md.playing { Color32::WHITE } else { Color32::from_gray(120) }),
                None => glyph::play(&p, c, !md.playing, accent),
            }
            media_clicks(ui, b, r, &resp, &md);
        }
        "system" => {
            glyph::chip(&p, Pos2::new(c.x, r.top() + 13.0), fg);
            small(&p, Pos2::new(c.x, r.bottom() - 9.0), &format!("{:.0}", st.cpu), SMALL, fg);
        }
        "ai" => {
            let parts = ai_parts(b);
            if parts.is_empty() {
                return false;
            }
            let resp = b.button(ui, r, id);
            let sc = rivet_look::theme::status(ui.ctx());
            let mut y = r.top();
            for part in &parts {
                let lr = Rect::from_center_size(Pos2::new(c.x, y + 13.0), Vec2::splat(AI_LOGO));
                crate::ai_pop::logo(ui, &p, part.id, lr, fg);
                if part.id == "claude" && b.m.extra.ai.claude_busy {
                    p.circle_filled(lr.right_top() + Vec2::new(0.5, 1.5), 2.5, accent);
                }
                let col = if part.used >= 90.0 { sc.danger } else if part.used >= 75.0 { sc.warning } else { fg };
                small(&p, Pos2::new(c.x, y + STACK - 9.0), part.val.trim_end_matches('%'), SMALL, col);
                y += STACK + 4.0;
            }
            if resp.clicked() {
                let a = b.popup(PopupKind::Ai, b.at(r));
                b.acts.push(a);
            }
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squeezes_then_leaves_out() {
        let mut z = vec![("a".to_string(), 30.0, false), ("title".to_string(), 300.0, true), ("b".to_string(), 30.0, false)];
        fit(&mut z, 200.0, true, GAP);
        assert_eq!(z.len(), 3);
        assert!((z[1].1 - (200.0 - 60.0 - 2.0 * GAP)).abs() < 0.01);
        fit(&mut z, 100.0, true, GAP);
        assert_eq!(z.iter().map(|i| i.0.as_str()).collect::<Vec<_>>(), ["a"]);
        let mut z = vec![("a".to_string(), 30.0, false), ("b".to_string(), 30.0, false)];
        fit(&mut z, 40.0, false, GAP);
        assert_eq!(z[0].0, "b");
    }
}
