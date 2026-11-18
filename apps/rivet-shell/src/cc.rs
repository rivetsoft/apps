//! The notification cards in the corner and the on-screen display (`quick` has the quick
//! settings).

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md};

use crate::glyph;
use crate::notes::Note;
use crate::ui::{Act, Kit, Model, OsdKind, ago, card, one_line, ui_font, wrapped};

/// The cards' surface: its width, and the room around the cards for shadows.
pub const CARDS_W: f32 = 380.0;
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
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let full = ui.max_rect();
    let p = ui.painter().clone();
    let mut y = full.top() + CARD_PAD;
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    for n in &m.extra.cards {
        let r = Rect::from_min_size(Pos2::new(full.left() + CARD_PAD, y), Vec2::new(full.width() - CARD_PAD * 2.0, card_height(n)));
        y = r.bottom() + CARD_GAP;
        let resp = ui.interact(r, egui::Id::unique(("card", n.id)), Sense::click());
        card(&p, r, &v, 0.97, r_lg());
        if n.urgency >= 2 {
            p.rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 14.0), Vec2::new(3.0, r.height() - 28.0)), 2.0, danger);
        }
        if resp.hovered() {
            p.rect_filled(r.shrink(1.0), r_lg(), mix(v.panel_fill, v.text_color(), 0.04));
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
    let xresp = ui.interact(xr, egui::Id::unique(("note-x", n.id, history)), Sense::click());
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

/// Volume or brightness as it changes: a small pill with a bar.
pub fn osd(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let Some((kind, value, muted)) = m.extra.osd else { return };
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let r = ui.max_rect().shrink(8.0);
    let p = ui.painter().clone();
    card(&p, r, &v, 0.97, (r.height() / 2.0) as u8);
    let fg = v.text_color();
    let c = Pos2::new(r.left() + 26.0, r.center().y);
    match kind {
        OsdKind::Volume => glyph::speaker(&p, c + Vec2::new(2.0, 0.0), muted, fg),
        OsdKind::Mic => glyph::mic(&p, c, fg, muted),
        OsdKind::Brightness => glyph::sun(&p, c, fg),
    }
    let text = if muted { "Muted".to_string() } else { format!("{value}%") };
    let g = p.layout_no_wrap(text, ui_font(13.0), fg);
    p.galley(Pos2::new(r.right() - 18.0 - g.size().x, r.center().y - g.size().y / 2.0), g, fg);
    let track = Rect::from_min_max(Pos2::new(r.left() + 48.0, r.center().y - 3.0), Pos2::new(r.right() - 64.0, r.center().y + 3.0));
    p.rect_filled(track, 3.0, mix(v.panel_fill, fg, 0.14));
    let frac = if muted { 0.0 } else { (value as f32 / 100.0).clamp(0.0, 1.0) };
    p.rect_filled(Rect::from_min_size(track.min, Vec2::new(track.width() * frac, track.height())), 3.0, accent);
}
