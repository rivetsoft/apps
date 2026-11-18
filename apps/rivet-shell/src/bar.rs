//! A panel: its bar, and its items laid out in three zones (left, center, right). Each item
//! says how wide it wants to be and draws itself in the room it's given; an item the shell
//! doesn't know (a plugin not installed yet) takes no room.

use chrono::Local;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_lg;

use crate::config::Edge;
use crate::glyph;
use crate::panels::{BAR, MARGIN, PanelCfg};
use crate::sys::Net;
use crate::ui::{Act, Kit, Model, Popup, PopupKind, card, hover_fill, on, one_line, ui_font};

const GAP: f32 = 2.0;
/// Between zones.
const ZONE_GAP: f32 = 14.0;
/// The narrowest a squeezed item (window title, what's playing) gets before it's left out.
const MIN_FLEX: f32 = 70.0;

/// What drawing an item needs.
struct Bar<'a, 'b> {
    p: egui::Painter,
    v: egui::Visuals,
    accent: Color32,
    cy: f32,
    idx: usize,
    /// Where this panel's popups open: its edge, and how far from it.
    from: (Edge, f32),
    m: &'a Model<'b>,
    acts: Vec<Act>,
}

impl Bar<'_, '_> {
    fn fg(&self) -> Color32 {
        self.v.text_color()
    }
    fn weak(&self) -> Color32 {
        self.v.weak_text_color()
    }
    fn popup(&self, kind: PopupKind, x: f32) -> Act {
        let mut p = Popup::at(kind, x, self.m.output);
        p.from = Some(self.from);
        Act::Open(p)
    }
    fn control(&self, x: f32) -> Act {
        self.popup(PopupKind::Control { drag: None }, x)
    }
    /// A clickable slot with a hover fill.
    fn button(&self, ui: &mut egui::Ui, r: Rect, id: &str) -> egui::Response {
        let resp = ui.interact(r, egui::Id::unique(("panel", self.idx, id)), Sense::click());
        if resp.hovered() {
            hover_fill(ui.painter(), r, &self.v);
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
    Some(match id {
        "launcher" | "network" | "quick-settings" | "power" => (30.0, false),
        "bluetooth" => (if st.bluetooth.is_some() { 30.0 } else { return None }, false),
        "notifications" => (if b.m.extra.notifying { 30.0 } else { return None }, false),
        "workspaces" => {
            let ws = workspaces_here(b);
            if ws.is_empty() {
                return None;
            }
            (ws.iter().map(|(label, active)| pill_w(b, label, *active) + 4.0).sum::<f32>() - 4.0, false)
        }
        "window-title" => {
            if st.window.is_empty() {
                return None;
            }
            (b.text_w(st.window.clone()).min(360.0), true)
        }
        "clock" => (b.p.layout_no_wrap(Local::now().format(&s.clock_format).to_string(), ui_font(13.5), b.fg()).size().x + 22.0, false),
        "media" => {
            let md = st.media.as_ref()?;
            (b.text_w(media_text(md)).min(230.0) + 34.0, true)
        }
        "system" => (b.text_w(system_text(b)) + 20.0, false),
        "volume" => {
            let (v, muted) = st.volume?;
            (b.text_w(if muted { "Muted".into() } else { format!("{v}%") }) + 38.0, false)
        }
        "brightness" => (b.text_w(format!("{}%", st.brightness?)) + 38.0, false),
        "battery" => (b.text_w(format!("{}%", st.battery?.0)) + 40.0, false),
        _ => return None,
    })
}

fn media_text(md: &crate::sys::Media) -> String {
    if md.artist.is_empty() { md.title.clone() } else { format!("{} · {}", md.title, md.artist) }
}

fn system_text(b: &Bar) -> String {
    let (used, _) = b.m.status.mem;
    format!("{:.0}%  ·  {:.1} GB", b.m.status.cpu, used as f32 / (1u64 << 30) as f32)
}

fn workspaces_here(b: &Bar) -> Vec<(String, bool)> {
    let out = b.m.output;
    b.m.status.workspaces.iter().filter(|w| out.is_empty() || w.output.is_empty() || w.output == out).map(|w| (if w.label.is_empty() { w.key.clone() } else { w.label.clone() }, w.active)).collect()
}

fn pill_w(b: &Bar, label: &str, active: bool) -> f32 {
    let w = b.p.layout_no_wrap(label.to_string(), ui_font(12.5), b.fg()).size().x;
    if active { (w + 26.0).max(38.0) } else { (w + 14.0).max(24.0) }
}

/// Fit a zone's items into `room`: squeezable ones shrink first, then items are left out
/// (from the end away from `keep_start`'s side).
fn fit(items: &mut Vec<(String, f32, bool)>, room: f32, keep_start: bool) {
    let total = |it: &Vec<(String, f32, bool)>| it.iter().map(|i| i.1).sum::<f32>() + GAP * it.len().saturating_sub(1) as f32;
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
    let v = ui.visuals().clone();
    let full = ui.max_rect();
    // The bar sits against its edge (a floating one with room around it).
    let (bar, radius) = if cfg.floating {
        let y = if cfg.edge == Edge::Top { full.top() + MARGIN } else { full.bottom() - MARGIN - BAR };
        (Rect::from_min_size(Pos2::new(full.left() + MARGIN, y), Vec2::new(full.width() - MARGIN * 2.0, BAR)), egui::CornerRadius::from(r_lg()))
    } else {
        (full, egui::CornerRadius::ZERO)
    };
    card(ui.painter(), bar, &v, 0.94, radius);
    let reach = crate::panels::reach(&m.s.panels, idx, m.output);
    let mut acts = Vec::new();
    {
        let accent = kit.accent(m.s);
        let mut b = Bar { p: ui.painter().clone(), v, accent, cy: bar.center().y, idx, from: (cfg.edge, reach + 4.0), m: &*m, acts: Vec::new() };
        layout(ui, &mut b, &cfg, bar);
        acts.append(&mut b.acts);
    }
    m.acts.extend(acts);
}

fn layout(ui: &mut egui::Ui, b: &mut Bar, cfg: &PanelCfg, bar: Rect) {
    let inner = bar.shrink2(Vec2::new(6.0, 0.0));
    let measure = |b: &Bar, zone: &[String]| -> Vec<(String, f32, bool)> { zone.iter().filter_map(|id| want(b, id).map(|(w, f)| (id.clone(), w, f))).collect() };
    let width = |it: &[(String, f32, bool)]| it.iter().map(|i| i.1).sum::<f32>() + GAP * it.len().saturating_sub(1) as f32;
    let mut left = measure(b, &cfg.zones[0]);
    let mut center = measure(b, &cfg.zones[1]);
    let mut right = measure(b, &cfg.zones[2]);
    fit(&mut center, inner.width(), true);
    let cw = width(&center);
    let (lw, rw) = (width(&left), width(&right));
    let (left_room, right_room) = if center.is_empty() {
        // The two sides share the bar; the right keeps what it needs first.
        let r = rw.min(inner.width());
        ((inner.width() - r - ZONE_GAP).max(0.0), r.max(inner.width() - lw - ZONE_GAP))
    } else {
        let half = (inner.width() - cw) / 2.0 - ZONE_GAP;
        (half.max(0.0), half.max(0.0))
    };
    fit(&mut left, left_room, true);
    fit(&mut right, right_room, false);
    let mut x = inner.left();
    for (id, w, _) in &left {
        draw(ui, b, id, Rect::from_min_size(Pos2::new(x, b.cy - 14.0), Vec2::new(*w, 28.0)));
        x += w + GAP;
    }
    let mut x = inner.center().x - cw / 2.0;
    for (id, w, _) in &center {
        draw(ui, b, id, Rect::from_min_size(Pos2::new(x, b.cy - 14.0), Vec2::new(*w, 28.0)));
        x += w + GAP;
    }
    let mut x = inner.right() - width(&right);
    for (id, w, _) in &right {
        draw(ui, b, id, Rect::from_min_size(Pos2::new(x, b.cy - 14.0), Vec2::new(*w, 28.0)));
        x += w + GAP;
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
    match id {
        "launcher" => {
            let resp = b.button(ui, r, id);
            glyph::grid(&p, r.center(), fg);
            if resp.on_hover_text("Apps").clicked() {
                let a = b.popup(PopupKind::Launcher { query: String::new(), sel: 0 }, r.center().x);
                b.acts.push(a);
            }
        }
        "workspaces" => {
            let mut x = r.left();
            for w in st.workspaces.iter().filter(|w| b.m.output.is_empty() || w.output.is_empty() || w.output == b.m.output) {
                let label = if w.label.is_empty() { w.key.clone() } else { w.label.clone() };
                let pr = Rect::from_min_size(Pos2::new(x, cy - 11.0), Vec2::new(pill_w(b, &label, w.active), 22.0));
                x = pr.right() + 4.0;
                if pr.right() > r.right() + 0.5 {
                    break;
                }
                let resp = ui.interact(pr, egui::Id::unique(("ws", b.idx, &w.key, &w.output)), Sense::click());
                let (fill, col) = if w.active {
                    (accent, on(accent))
                } else if resp.hovered() {
                    (mix(b.v.panel_fill, fg, 0.16), fg)
                } else {
                    (mix(b.v.panel_fill, fg, if w.busy { 0.1 } else { 0.05 }), if w.busy { fg } else { weak })
                };
                p.rect_filled(pr, 11.0, fill);
                // Laid out in its colour (a galley keeps the colour it was made with).
                let g = p.layout_no_wrap(label, ui_font(12.5), col);
                p.galley(pr.center() - g.size() / 2.0, g, col);
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(format!("Workspace {} ({})", w.label, b.m.compositor.name())).clicked() {
                    b.acts.push(Act::Workspace(w.key.clone()));
                }
            }
        }
        "window-title" => {
            let g = one_line(&p, &st.window, ui_font(12.5), weak, r.width());
            p.galley(Pos2::new(r.left(), cy - g.size().y / 2.0), g, weak);
        }
        "clock" => {
            let now = Local::now();
            let resp = b.button(ui, r, id);
            let g = p.layout_no_wrap(now.format(&b.m.s.clock_format).to_string(), ui_font(13.5), fg);
            p.galley(r.center() - g.size() / 2.0, g, fg);
            if resp.on_hover_text(now.format("%A, %B %-d, %Y").to_string()).clicked() {
                let a = b.popup(PopupKind::Calendar { month: 0 }, r.center().x);
                b.acts.push(a);
            }
        }
        "media" => {
            let Some(md) = &st.media else { return };
            let resp = b.button(ui, r, id);
            glyph::play(&p, Pos2::new(r.left() + 14.0, cy), !md.playing, accent);
            let g = one_line(&p, &media_text(md), ui_font(12.5), fg, r.width() - 34.0);
            p.galley(Pos2::new(r.left() + 26.0, cy - g.size().y / 2.0), g, fg);
            if resp.on_hover_text(format!("{} · click to {}", md.player, if md.playing { "pause" } else { "play" })).clicked() {
                b.acts.push(Act::Media("play-pause"));
            }
        }
        "system" => {
            let resp = ui.interact(r, egui::Id::unique(("panel", b.idx, id)), Sense::hover());
            let g = p.layout_no_wrap(system_text(b), ui_font(12.5), fg);
            p.galley(Pos2::new(r.left() + 10.0, cy - g.size().y / 2.0), g, fg);
            resp.on_hover_text("Processor use · memory in use");
        }
        "volume" => {
            let Some((vol, muted)) = st.volume else { return };
            let col = if muted { weak } else { fg };
            let resp = b.button(ui, r, id);
            glyph::speaker(&p, Pos2::new(r.left() + 15.0, cy), muted, col);
            let g = p.layout_no_wrap(if muted { "Muted".into() } else { format!("{vol}%") }, ui_font(12.5), col);
            p.galley(Pos2::new(r.left() + 30.0, cy - g.size().y / 2.0), g, col);
            if resp.hovered() {
                let dy = wheel(ui);
                if dy.abs() > 0.5 {
                    b.acts.push(Act::Volume(dy > 0.0));
                }
            }
            if resp.on_hover_text("Click to mute · scroll to change").clicked() {
                b.acts.push(Act::Mute);
            }
        }
        "brightness" => {
            let Some(v) = st.brightness else { return };
            let resp = b.button(ui, r, id);
            glyph::sun(&p, Pos2::new(r.left() + 15.0, cy), fg);
            let g = p.layout_no_wrap(format!("{v}%"), ui_font(12.5), fg);
            p.galley(Pos2::new(r.left() + 30.0, cy - g.size().y / 2.0), g, fg);
            if resp.hovered() {
                let dy = wheel(ui);
                if dy.abs() > 0.5 {
                    b.acts.push(Act::Brightness(dy > 0.0));
                }
            }
            if resp.on_hover_text("Brightness · scroll to change").clicked() {
                let a = b.control(r.center().x);
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
                let a = b.popup(PopupKind::Network { ask: None }, r.center().x);
                b.acts.push(a);
            }
        }
        "bluetooth" => {
            let Some(on) = st.bluetooth else { return };
            let resp = b.button(ui, r, id);
            glyph::bluetooth(&p, r.center(), if on { fg } else { weak.gamma_multiply(0.6) }, on);
            let tip = if on { "Bluetooth on · click for devices" } else { "Bluetooth off · click for devices" };
            if resp.on_hover_text(tip).clicked() {
                let a = b.popup(PopupKind::Bluetooth, r.center().x);
                b.acts.push(a);
            }
        }
        "battery" => {
            let Some((pct, charging)) = st.battery else { return };
            let col = if pct <= 15 && !charging { rivet_look::theme::status(ui.ctx()).danger } else { fg };
            let resp = b.button(ui, r, id);
            glyph::battery(&p, Pos2::new(r.left() + 16.0, cy), pct, charging, col, accent);
            let g = p.layout_no_wrap(format!("{pct}%"), ui_font(12.5), col);
            p.galley(Pos2::new(r.left() + 30.0, cy - g.size().y / 2.0), g, col);
            let mut tip = if charging { format!("Battery {pct}%, charging") } else { format!("Battery {pct}%") };
            if let Some(pr) = &st.profile {
                tip.push_str(&format!(" · {}", pr.replace('-', " ")));
            }
            if resp.on_hover_text(tip).clicked() {
                let a = b.control(r.center().x);
                b.acts.push(a);
            }
        }
        "notifications" => {
            let resp = b.button(ui, r, id);
            glyph::bell(&p, r.center(), fg, ex.dnd);
            let n = ex.notes.len();
            if n > 0 && !ex.dnd {
                p.circle_filled(r.center() + Vec2::new(6.0, -6.0), 3.5, accent);
            }
            let tip = if ex.dnd { "Do not disturb".to_string() } else if n == 0 { "No notifications".to_string() } else { format!("{n} notification{}", if n == 1 { "" } else { "s" }) };
            if resp.on_hover_text(tip).clicked() {
                let a = b.control(r.center().x);
                b.acts.push(a);
            }
        }
        "quick-settings" => {
            let resp = b.button(ui, r, id);
            glyph::sliders(&p, r.center(), fg);
            if resp.on_hover_text("Quick settings").clicked() {
                let a = b.control(r.center().x);
                b.acts.push(a);
            }
        }
        "power" => {
            let resp = b.button(ui, r, id);
            glyph::power(&p, r.center(), fg);
            if resp.on_hover_text("Lock, suspend, restart, shut down").clicked() {
                let a = b.popup(PopupKind::Power { confirm: None }, r.center().x);
                b.acts.push(a);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn squeezes_then_leaves_out() {
        let mut z = vec![("a".to_string(), 30.0, false), ("title".to_string(), 300.0, true), ("b".to_string(), 30.0, false)];
        fit(&mut z, 200.0, true);
        assert_eq!(z.len(), 3);
        assert!((z[1].1 - (200.0 - 60.0 - 2.0 * GAP)).abs() < 0.01);
        fit(&mut z, 100.0, true);
        assert_eq!(z.iter().map(|i| i.0.as_str()).collect::<Vec<_>>(), ["a"]);
        let mut z = vec![("a".to_string(), 30.0, false), ("b".to_string(), 30.0, false)];
        fit(&mut z, 40.0, false);
        assert_eq!(z[0].0, "b");
    }
}
