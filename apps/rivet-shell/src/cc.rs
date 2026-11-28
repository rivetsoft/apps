//! The notification cards in the corner and the on-screen display (`quick` has the quick
//! settings).

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_md;

use crate::glyph;
use crate::notes::Note;
use crate::ui::{Act, Kit, Model, OsdKind, ago, card, one_line, ui_font, wrapped};

/// The cards' surface width (with room around the cards for shadows), by the width setting.
pub fn cards_width(s: &crate::config::Settings) -> f32 {
    match s.note_width {
        crate::config::LaunchSize::Small => 320.0,
        crate::config::LaunchSize::Medium => 380.0,
        crate::config::LaunchSize::Large => 460.0,
    }
}
const CARD_PAD: f32 = 10.0;
const CARD_GAP: f32 = 8.0;
/// The on-screen display's surface.
pub const OSD_W: f32 = 300.0;
pub const OSD_H: f32 = 64.0;

// ------------------------------------------------------------------ notification cards

fn card_height(n: &Note) -> f32 {
    let base = if n.body.is_empty() { 62.0 } else { 84.0 };
    base + if n.buttons().next().is_some() { 36.0 } else { 0.0 }
}

/// The cards' surface height for these notifications.
pub fn cards_height(notes: &[Note]) -> f32 {
    if notes.is_empty() {
        return 1.0;
    }
    CARD_PAD * 2.0 + notes.iter().map(card_height).sum::<f32>() + CARD_GAP * (notes.len() as f32 - 1.0)
}

/// Notifications in the corner, newest on top.
pub fn cards(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    // The cards' own look over Appearance's: corners, how solid, colour, outline.
    let look = m.s.note_look();
    let (v, highlight) = crate::style::visuals(ui.visuals(), &look, &crate::style::Roles::new(&kit.look, m.s));
    *ui.visuals_mut() = v.clone();
    let accent = if m.s.note_look.highlight.is_none() && m.s.look.highlight.is_none() { kit.tone(m.s, crate::style::Slot::Notify) } else { highlight };
    let radius = crate::style::radius(&look);
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let mut y = full.top() + CARD_PAD;
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    for n in &m.extra.cards {
        let r = Rect::from_min_size(Pos2::new(full.left() + CARD_PAD, y), Vec2::new(full.width() - CARD_PAD * 2.0, card_height(n)));
        y = r.bottom() + CARD_GAP;
        let resp = ui.interact(r, egui::Id::unique(("card", n.id)), Sense::click()).role(Role::Button, format!("{}: {}", n.app, n.summary)).described(n.body.as_str());
        crate::ui::surface(&p, r, &v, look.opacity, radius, look.border);
        if n.urgency >= 2 {
            p.rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 14.0), Vec2::new(3.0, r.height() - 28.0)), 2.0, danger);
        }
        if resp.hovered() {
            p.rect_filled(r.shrink(1.0), radius, mix(v.panel_fill, v.text_color(), 0.04));
        }
        note_body(ui, &p, n, r.shrink2(Vec2::new(14.0, 12.0)), m, kit, accent, false);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::NoteClick(n.id));
        }
    }
}

/// A notification in `r`: icon, app and time, title, text, its buttons, and a close button
/// (`history`: removes it from the history; else takes the card off the screen).
#[allow(clippy::too_many_arguments)]
fn note_body(ui: &mut egui::Ui, p: &egui::Painter, n: &Note, r: Rect, m: &mut Model, kit: &mut Kit, accent: Color32, history: bool) {
    let v = ui.visuals().clone();
    let icon_px = if history { 26.0 } else { 34.0 };
    let ir = Rect::from_min_size(r.min + Vec2::new(0.0, 2.0), Vec2::splat(icon_px));
    match kit.note_icon(ui.ctx(), n, m.apps, icon_px) {
        Some(t) => {
            p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.circle_filled(ir.center(), icon_px / 2.0, accent.gamma_multiply(0.25));
            glyph::bell(p, ir.center(), accent, false);
        }
    }
    let tx = ir.right() + 12.0;
    let tw = r.right() - tx - 6.0;
    let weak = v.weak_text_color();
    let head = format!("{} · {}", if n.app.is_empty() { "Notification" } else { n.app.as_str() }, ago(n.at));
    let g = one_line(p, &head, ui_font(11.5), weak, tw - 20.0);
    p.galley(Pos2::new(tx, r.top()), g, weak);
    let g = one_line(p, &n.summary, ui_font(14.0), v.strong_text_color(), tw);
    let mut yy = r.top() + 17.0;
    let gh = g.size().y;
    p.galley(Pos2::new(tx, yy), g, v.strong_text_color());
    yy += gh + 2.0;
    if !n.body.is_empty() {
        let g = wrapped(p, &n.body, ui_font(12.5), v.text_color(), tw, if history { 1 } else { 2 });
        p.galley(Pos2::new(tx, yy), g, v.text_color());
    }
    // Close.
    let xr = Rect::from_center_size(Pos2::new(r.right() - 6.0, r.top() + 7.0), Vec2::splat(20.0));
    let xresp = ui.interact(xr, egui::Id::unique(("note-x", n.id, history)), Sense::click()).role(Role::Button, "Dismiss");
    if xresp.hovered() {
        p.circle_filled(xr.center(), 10.0, mix(v.panel_fill, v.text_color(), 0.12));
    }
    glyph::x(p, xr.center(), 3.5, weak);
    if xresp.on_hover_text(if history { "Remove" } else { "Hide" }).clicked() {
        m.acts.push(if history { Act::NoteRemove(n.id) } else { Act::NoteHide(n.id) });
    }
    // The app's buttons.
    if !history {
        let mut bx = tx;
        for (k, label) in n.buttons().take(3) {
            let g = p.layout_no_wrap(label.clone(), ui_font(12.5), v.text_color());
            let br = Rect::from_min_size(Pos2::new(bx, r.bottom() - 28.0), Vec2::new(g.size().x + 24.0, 28.0));
            if br.right() > r.right() {
                break;
            }
            let resp = ui.interact(br, egui::Id::unique(("note-btn", n.id, k)), Sense::click());
            p.rect_filled(br, r_md(), mix(v.panel_fill, v.text_color(), if resp.hovered() { 0.16 } else { 0.08 }));
            p.galley(br.center() - g.size() / 2.0, g, v.text_color());
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                m.acts.push(Act::NoteAction(n.id, k.clone()));
            }
            bx = br.right() + 6.0;
        }
    }
}

// ------------------------------------------------------------------ on-screen display

/// How much the on-screen display is scaled by its size setting.
fn osd_scale(s: &crate::config::Settings) -> f32 {
    match s.osd_size {
        crate::config::LaunchSize::Small => 0.85,
        crate::config::LaunchSize::Medium => 1.0,
        crate::config::LaunchSize::Large => 1.25,
    }
}

/// The on-screen display's surface size, in points.
pub fn osd_size(s: &crate::config::Settings) -> (f32, f32) {
    let k = osd_scale(s);
    ((OSD_W * k).round(), (OSD_H * k).round())
}

/// Volume or brightness as it changes: a small pill with a bar.
pub fn osd(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let Some((kind, value, muted)) = m.extra.osd else { return };
    let v = ui.visuals().clone();
    let accent = match m.s.osd_color {
        Some(role) => crate::style::Roles::new(&kit.look, m.s).get(role, v.panel_fill),
        None => kit.tone(m.s, crate::style::Slot::Slider),
    };
    let k = osd_scale(m.s);
    let font = |size: f32| ui_font(size * k);
    let r = ui.max_rect().shrink(8.0 * k);
    let p = ui.painter().clone();
    card(&p, r, &v, 0.97, (r.height() / 2.0) as u8);
    let fg = v.text_color();
    // The symbols are drawn at their usual size on a layer scaled by `k`, so they stay sharp.
    let glyphs = egui::LayerId::new(egui::Order::Foreground, egui::Id::unique("rivet-osd-glyphs"));
    ui.ctx().set_transform_layer(glyphs, egui::emath::TSTransform::from_scaling(k));
    let gp = ui.ctx().layer_painter(glyphs);
    let at = |c: Pos2| Pos2::new(c.x / k, c.y / k);
    if matches!(kind, OsdKind::CapsLock | OsdKind::NumLock) {
        // The key's symbol (lit in the accent when on), its name, and on or off.
        let on = value != 0;
        let c = Pos2::new(r.left() + 30.0 * k, r.center().y);
        let col = if on { accent } else { v.weak_text_color() };
        let key = Rect::from_center_size(at(c), Vec2::splat(24.0));
        gp.rect_stroke(key, 5.0, egui::Stroke::new(1.6, col), egui::StrokeKind::Inside);
        if kind == OsdKind::CapsLock {
            // An arrow up over a bar: ⇪.
            let s = egui::Stroke::new(1.6, col);
            let c = at(c);
            gp.add(egui::Shape::closed_line(vec![c + Vec2::new(0.0, -7.0), c + Vec2::new(5.5, -1.0), c + Vec2::new(2.5, -1.0), c + Vec2::new(2.5, 3.0), c + Vec2::new(-2.5, 3.0), c + Vec2::new(-2.5, -1.0), c + Vec2::new(-5.5, -1.0)], s));
            gp.line_segment([c + Vec2::new(-2.5, 6.0), c + Vec2::new(2.5, 6.0)], s);
        } else {
            p.text(c, egui::Align2::CENTER_CENTER, "1", font(13.0), col);
        }
        let name = if kind == OsdKind::CapsLock { "Caps Lock" } else { "Num Lock" };
        p.text(Pos2::new(r.left() + 56.0 * k, r.center().y), egui::Align2::LEFT_CENTER, name, font(14.0), fg);
        p.text(Pos2::new(r.right() - 20.0 * k, r.center().y), egui::Align2::RIGHT_CENTER, if on { "On" } else { "Off" }, font(13.0), if on { accent } else { v.weak_text_color() });
        return;
    }
    let c = at(Pos2::new(r.left() + 26.0 * k, r.center().y));
    match kind {
        OsdKind::Volume => glyph::speaker(&gp, c + Vec2::new(2.0, 0.0), muted, fg),
        OsdKind::Mic => glyph::mic(&gp, c, fg, muted),
        OsdKind::Brightness => glyph::sun(&gp, c, fg),
        OsdKind::CapsLock | OsdKind::NumLock => {}
    }
    let text = if muted { "Muted".to_string() } else { format!("{value}%") };
    let g = p.layout_no_wrap(text, font(13.0), fg);
    p.galley(Pos2::new(r.right() - 18.0 * k - g.size().x, r.center().y - g.size().y / 2.0), g, fg);
    let track = Rect::from_min_max(Pos2::new(r.left() + 48.0 * k, r.center().y - 3.0 * k), Pos2::new(r.right() - 64.0 * k, r.center().y + 3.0 * k));
    p.rect_filled(track, 3.0 * k, mix(v.panel_fill, fg, 0.14));
    let frac = if muted { 0.0 } else { (value as f32 / 100.0).clamp(0.0, 1.0) };
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 3.0 * k, accent);
}
