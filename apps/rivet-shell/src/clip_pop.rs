//! The clipboard history's popup, from the panel's Clipboard item or `rivet-shell msg
//! clipboard`: what was copied, newest first under the pinned ones. Choosing a copy puts it
//! back on the clipboard; each can be pinned or removed, and Clear All takes the rest.

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::a11y::{Access, Role};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm};

use crate::clip::{Body, Clip, Job, shown};
use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, ago, one_line, ui_font, wrapped};

pub const WIDTH: f32 = 384.0;
pub const PAD: f32 = 18.0;
const HEAD: f32 = 56.0;
const ROW: f32 = 64.0;
const ROW_GAP: f32 = 4.0;
const EMPTY: f32 = 150.0;
/// A picture's thumbnail in its row.
const THUMB: Vec2 = Vec2::new(72.0, 44.0);

/// The popup's height for what it shows (it scrolls when the screen is shorter).
pub fn height(m: &Model) -> f32 {
    let n = m.extra.clip.as_ref().map_or(0, |l| l.len());
    2.0 * PAD + HEAD + 8.0 + if n == 0 { EMPTY } else { n as f32 * (ROW + ROW_GAP) + 4.0 }
}

pub fn popup(ui: &mut egui::Ui, sel: &mut Option<usize>, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let (fg, weak) = (v.text_color(), v.weak_text_color());
    let w = ui.available_width();
    let mut acts: Vec<Act> = Vec::new();
    ui.spacing_mut().item_spacing.y = 0.0;
    let all = m.extra.clip.clone().unwrap_or_default();
    let list = shown(&all);
    let pinned = list.iter().filter(|c| c.pinned).count();

    // ---- the title row: how many, and Clear All
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 14.0), Align2::LEFT_CENTER, "Clipboard", ui_font(15.0), v.strong_text_color());
    let sub = match (list.len(), pinned) {
        (0, _) => "Nothing copied yet".to_string(),
        (1, _) => "1 copy".to_string(),
        (n, 0) => format!("{n} copies"),
        (n, k) => format!("{n} copies · {k} pinned"),
    };
    let g = one_line(&p, &sub, ui_font(12.0), weak, w - 120.0);
    p.galley(Pos2::new(r.left() + 2.0, r.top() + 29.0), g, weak);
    if list.len() > pinned {
        let g = p.layout_no_wrap("Clear All".into(), ui_font(12.5), fg);
        let br = Rect::from_min_max(Pos2::new(r.right() - g.size().x - 20.0, r.top() + 5.0), Pos2::new(r.right(), r.top() + 37.0));
        let resp = ui.interact(br, egui::Id::unique("clip-clear"), Sense::click()).role(Role::Button, "Clear All");
        p.rect_filled(br, r_sm(), mix(v.panel_fill, fg, if resp.hovered() { 0.12 } else { 0.06 }));
        p.galley(br.center() - g.size() / 2.0, g, if resp.hovered() { v.strong_text_color() } else { fg });
        let resp = if pinned > 0 { resp.on_hover_text("Pinned copies stay") } else { resp };
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.push(Act::Clip(Job::Clear));
        }
    }
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, fg, 0.08)));
    ui.add_space(8.0);

    // ---- the history
    if list.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, EMPTY), Sense::hover());
        let c = r.center() - Vec2::new(0.0, 14.0);
        p.circle_filled(c, 26.0, mix(v.panel_fill, fg, 0.06));
        glyph::clipboard(&p, c, weak);
        p.text(c + Vec2::new(0.0, 46.0), Align2::CENTER_CENTER, "Text and pictures you copy appear here", ui_font(13.0), weak);
        m.acts.extend(acts);
        return;
    }
    // Up and Down choose, Enter puts back, Delete removes.
    let (up, down, enter, del) = ui.input_mut(|i| (i.consume_key(egui::Modifiers::NONE, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::ArrowDown), i.consume_key(egui::Modifiers::NONE, Key::Enter), i.consume_key(egui::Modifiers::NONE, Key::Delete)));
    let last = list.len() - 1;
    if down {
        *sel = Some(sel.map_or(0, |k| (k + 1).min(last)));
    }
    if up {
        *sel = Some(sel.map_or(0, |k| k.saturating_sub(1)));
    }
    if let Some(k) = sel.filter(|k| *k > last) {
        *sel = Some(k.min(last));
    }
    if let Some(c) = sel.and_then(|k| list.get(k)) {
        if enter {
            acts.extend([Act::Clip(Job::Restore(c.id)), Act::Close]);
        }
        if del {
            acts.push(Act::Clip(Job::Remove(c.id)));
        }
    }
    let accent = kit.accent(m.s);
    let max_h = ui.available_height();
    egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(max_h).show(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        for (k, c) in list.iter().enumerate() {
            let chosen = *sel == Some(k);
            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
            if chosen && (up || down) {
                ui.scroll_to_rect(r, None);
            }
            row(ui, r, c, chosen, accent, kit, &mut acts);
            ui.add_space(ROW_GAP);
        }
    });
    m.acts.extend(acts);
}

/// One copy: its text (two lines of it) or its picture, when it was copied, and on hover
/// Pin and Remove.
fn row(ui: &mut egui::Ui, r: Rect, c: &Clip, chosen: bool, accent: Color32, kit: &mut Kit, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let resp = ui.interact(r, egui::Id::unique(("clip-row", c.id)), Sense::click()).role(Role::Button, c.line(80));
    let resp = if c.pinned { resp.described("Pinned") } else { resp };
    let p = ui.painter().clone();
    let hovered = resp.hovered() || ui.rect_contains_pointer(r);
    p.rect_filled(r, r_md(), mix(v.panel_fill, v.text_color(), if hovered { 0.08 } else { 0.04 }));
    if chosen {
        p.rect_stroke(r, r_md(), Stroke::new(1.0, accent.gamma_multiply(0.7)), egui::StrokeKind::Inside);
    }
    let mut tx = r.left() + 12.0;
    let tw_right = r.right() - 64.0;
    let meta_y = r.bottom() - 13.0;
    match &c.body {
        Body::Text(_) => {
            let g = wrapped(&p, &c.line(400), ui_font(13.0), v.strong_text_color(), tw_right - tx, 2);
            p.galley(Pos2::new(tx, r.top() + 8.0), g, v.strong_text_color());
        }
        Body::Image(pic) => {
            let box_r = Rect::from_min_size(Pos2::new(tx, r.center().y - THUMB.y / 2.0), THUMB);
            p.rect_filled(box_r, r_sm(), mix(v.panel_fill, v.text_color(), 0.06));
            if let Some(t) = kit.clip_thumb(ui.ctx(), c.id, pic) {
                let (tw, th) = (pic.thumb.0 as f32, pic.thumb.1 as f32);
                let k = (THUMB.x / tw).min(THUMB.y / th);
                let ir = Rect::from_center_size(box_r.center(), Vec2::new(tw * k, th * k));
                p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            tx = box_r.right() + 12.0;
            let g = one_line(&p, &c.line(80), ui_font(13.0), v.strong_text_color(), tw_right - tx);
            p.galley(Pos2::new(tx, r.top() + 12.0), g, v.strong_text_color());
        }
    }
    // When, with a pin before it when it's pinned.
    let when = chrono::DateTime::from_timestamp(c.at, 0).map(|t| ago(t.with_timezone(&chrono::Local))).unwrap_or_default();
    let mut mx = tx;
    if c.pinned {
        glyph::pin(&p, Pos2::new(mx + 5.0, meta_y), accent, true);
        mx += 14.0;
    }
    p.text(Pos2::new(mx, meta_y), Align2::LEFT_CENTER, if c.pinned { format!("Pinned · {when}") } else { when }, ui_font(11.5), weak);
    if hovered || chosen {
        let y = r.top() + 18.0;
        let pr = Rect::from_center_size(Pos2::new(r.right() - 44.0, y), Vec2::splat(24.0));
        let (tip, pinned) = if c.pinned { ("Unpin", true) } else { ("Pin", false) };
        if tool_button(ui, pr, &format!("clip-pin-{}", c.id), tip, &|p, ctr, col| glyph::pin(p, ctr, if pinned { accent } else { col }, pinned)) {
            acts.push(Act::Clip(Job::Pin(c.id, !c.pinned)));
        }
        let xr = Rect::from_center_size(Pos2::new(r.right() - 16.0, y), Vec2::splat(24.0));
        if tool_button(ui, xr, &format!("clip-x-{}", c.id), "Remove", &|p, ctr, col| glyph::x(p, ctr, 4.0, col)) {
            acts.push(Act::Clip(Job::Remove(c.id)));
        }
    }
    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Put back on the clipboard").clicked() {
        acts.extend([Act::Clip(Job::Restore(c.id)), Act::Close]);
    }
}
