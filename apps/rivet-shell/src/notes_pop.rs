//! The notification panel, opened from the panel's bell: a title row with Do Not Disturb and
//! Clear All, then the history grouped by app, newest first. Its own popup rather than a
//! section of quick settings, so it can grow other parts later (a calendar, say): each part is
//! a block under the title, with its height counted in `height`.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm, selected_tile};

use crate::glyph;
use crate::notes::Note;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, ago, one_line, ui_font};

pub const WIDTH: f32 = 384.0;
pub const PAD: f32 = 18.0;
const HEAD: f32 = 56.0;
const GROUP: f32 = 30.0;
/// A row with only a title.
const ROW: f32 = 44.0;
/// Title and one line of text; a second line adds LINE.
const ROW_BODY: f32 = 54.0;
const LINE: f32 = 17.0;
/// The body text's width: the row less its icon column and right margin.
const BODY_W: f32 = WIDTH - 2.0 * PAD - 52.0 - 14.0;
const ROW_GAP: f32 = 4.0;
const EMPTY: f32 = 150.0;

fn body_job(text: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: ui_font(12.5), color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: BODY_W, max_rows: 2, break_anywhere: false, overflow_character: Some('…') };
    job
}

/// A row's height: one line of text or two, as the body wraps.
fn row_height(ctx: &egui::Context, n: &Note) -> f32 {
    if n.body.is_empty() {
        return ROW;
    }
    let rows = ctx.fonts_mut(|f| f.layout_job(body_job(&n.body, Color32::WHITE))).rows.len().clamp(1, 2);
    ROW_BODY + (rows - 1) as f32 * LINE
}

/// The notifications by app, in the order each app last sent one.
fn groups(notes: &[Note]) -> Vec<(&str, Vec<&Note>)> {
    let mut g: Vec<(&str, Vec<&Note>)> = Vec::new();
    for n in notes {
        let app = if n.app.is_empty() { "Other" } else { n.app.as_str() };
        match g.iter_mut().find(|(a, _)| *a == app) {
            Some((_, list)) => list.push(n),
            None => g.push((app, vec![n])),
        }
    }
    g
}

/// The list's own height (it scrolls when the screen is shorter).
fn list_height(ctx: &egui::Context, notes: &[Note]) -> f32 {
    if notes.is_empty() {
        return EMPTY;
    }
    groups(notes).iter().map(|(_, l)| GROUP + l.iter().map(|n| row_height(ctx, n) + ROW_GAP).sum::<f32>()).sum::<f32>() + 4.0
}

/// The panel's height for what it shows.
pub fn height(m: &Model, ctx: &egui::Context) -> f32 {
    2.0 * PAD + HEAD + 8.0 + list_height(ctx, &m.extra.notes)
}

pub fn popup(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let accent = kit.tone(m.s, crate::style::Slot::Notify);
    let (fg, weak) = (v.text_color(), v.weak_text_color());
    let w = ui.available_width();
    let mut acts: Vec<Act> = Vec::new();
    ui.spacing_mut().item_spacing.y = 0.0;
    let notes = m.extra.notes.clone();
    let dnd = m.extra.dnd;

    // ---- the title row: how many; Do Not Disturb and Clear All
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 14.0), Align2::LEFT_CENTER, "Notifications", ui_font(15.0), v.strong_text_color());
    let sub = match (notes.len(), dnd) {
        (_, true) => "Do Not Disturb is on · new ones wait here".to_string(),
        (0, _) => "Nothing new".to_string(),
        (1, _) => "1 notification".to_string(),
        (n, _) => format!("{n} notifications"),
    };
    let g = one_line(&p, &sub, ui_font(12.0), weak, w - 150.0);
    p.galley(Pos2::new(r.left() + 2.0, r.top() + 29.0), g, weak);
    let mut right = r.right();
    if !notes.is_empty() {
        let g = p.layout_no_wrap("Clear All".into(), ui_font(12.5), fg);
        let br = Rect::from_min_max(Pos2::new(right - g.size().x - 20.0, r.top() + 5.0), Pos2::new(right, r.top() + 37.0));
        let resp = ui.interact(br, egui::Id::unique("notes-clear"), Sense::click());
        p.rect_filled(br, r_sm(), mix(v.panel_fill, fg, if resp.hovered() { 0.12 } else { 0.06 }));
        p.galley(br.center() - g.size() / 2.0, g, if resp.hovered() { v.strong_text_color() } else { fg });
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.push(Act::ClearNotes);
        }
        right = br.left() - 6.0;
    }
    // Do Not Disturb: lit like a selected item while it's on.
    let dr = Rect::from_min_max(Pos2::new(right - 32.0, r.top() + 5.0), Pos2::new(right, r.top() + 37.0));
    if dnd {
        let (fill, stroke) = selected_tile(&v, v.panel_fill);
        p.rect_filled(dr, r_sm(), fill);
        p.rect_stroke(dr, r_sm(), stroke, egui::StrokeKind::Inside);
    }
    let tip = if dnd { "Turn off Do Not Disturb" } else { "Do Not Disturb: no cards, they only wait here" };
    if tool_button(ui, dr, "notes-dnd", tip, &|p, c, col| glyph::bell(p, c, if dnd { accent } else { col }, dnd)) {
        acts.push(Act::Dnd(!dnd));
    }
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, fg, 0.08)));
    ui.add_space(8.0);

    // ---- the history
    if notes.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, EMPTY), Sense::hover());
        let c = r.center() - Vec2::new(0.0, 14.0);
        p.circle_filled(c, 26.0, mix(v.panel_fill, fg, 0.06));
        glyph::bell(&p, c, weak, dnd);
        p.text(c + Vec2::new(0.0, 46.0), Align2::CENTER_CENTER, "You're all caught up", ui_font(13.0), weak);
    } else {
        let max_h = ui.available_height();
        egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(max_h).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (app, list) in groups(&notes) {
                group_heading(ui, app, &list, &mut acts);
                for n in list {
                    note_row(ui, n, m, kit, accent, &mut acts);
                    ui.add_space(ROW_GAP);
                }
            }
        });
    }
    m.acts.extend(acts);
}

/// An app's name over its notifications, with a Clear for just those on hover.
fn group_heading(ui: &mut egui::Ui, app: &str, list: &[&Note], acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), GROUP), Sense::hover());
    let p = ui.painter();
    let label = if list.len() > 1 { format!("{app} · {}", list.len()) } else { app.to_string() };
    let g = one_line(p, &label, ui_font(12.0), weak, r.width() - 70.0);
    p.galley(Pos2::new(r.left() + 4.0, r.center().y - g.size().y / 2.0 + 2.0), g, weak);
    if resp.hovered() || ui.rect_contains_pointer(r) {
        let g = p.layout_no_wrap("Clear".into(), ui_font(12.0), v.text_color());
        let br = Rect::from_min_max(Pos2::new(r.right() - g.size().x - 16.0, r.top() + 4.0), Pos2::new(r.right(), r.bottom() - 2.0));
        let bresp = ui.interact(br, egui::Id::unique(("notes-clear-app", app)), Sense::click());
        if bresp.hovered() {
            ui.painter().rect_filled(br, r_sm(), mix(v.panel_fill, v.text_color(), 0.09));
        }
        ui.painter().galley(br.center() - g.size() / 2.0, g, if bresp.hovered() { v.strong_text_color() } else { weak });
        if bresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            acts.extend(list.iter().map(|n| Act::NoteRemove(n.id)));
        }
    }
}

/// One notification: icon, title and when, a couple of lines of text; a close button on
/// hover. Clicking one that the app can open does so.
fn note_row(ui: &mut egui::Ui, n: &Note, m: &mut Model, kit: &mut Kit, accent: Color32, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let weak = v.weak_text_color();
    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_height(ui.ctx(), n)), Sense::click());
    let p = ui.painter().clone();
    let opens = n.actions.iter().any(|(k, _)| k == "default");
    let hovered = resp.hovered() || ui.rect_contains_pointer(r);
    p.rect_filled(r, r_md(), mix(v.panel_fill, v.text_color(), if hovered { 0.08 } else { 0.04 }));
    if n.urgency >= 2 {
        let danger = rivet_look::theme::status(ui.ctx()).danger;
        p.rect_filled(Rect::from_min_size(r.min + Vec2::new(0.0, 10.0), Vec2::new(3.0, r.height() - 20.0)), 2.0, danger);
    }
    let ir = Rect::from_min_size(Pos2::new(r.left() + 12.0, if n.body.is_empty() { r.center().y - 14.0 } else { r.top() + 11.0 }), Vec2::splat(28.0));
    match kit.note_icon(ui.ctx(), n, m.apps, 28.0) {
        Some(t) => {
            p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        None => {
            p.circle_filled(ir.center(), 14.0, accent.gamma_multiply(0.22));
            glyph::bell(&p, ir.center(), accent, false);
        }
    }
    let tx = ir.right() + 12.0;
    let tw = r.right() - tx - 52.0;
    // The title's line: in the middle of a row with no text under it.
    let ty = if n.body.is_empty() { r.center().y } else { r.top() + 18.0 };
    let g = one_line(&p, &n.summary, ui_font(13.5), v.strong_text_color(), tw);
    p.galley(Pos2::new(tx, ty - g.size().y / 2.0), g, v.strong_text_color());
    if !n.body.is_empty() {
        p.galley(Pos2::new(tx, r.top() + 28.0), p.layout_job(body_job(&n.body, v.text_color())), v.text_color());
    }
    if hovered {
        let xr = Rect::from_center_size(Pos2::new(r.right() - 18.0, ty), Vec2::splat(24.0));
        if tool_button(ui, xr, &format!("notes-x-{}", n.id), "Remove", &|p, c, col| glyph::x(p, c, 4.0, col)) {
            acts.push(Act::NoteRemove(n.id));
            return;
        }
    } else {
        p.text(Pos2::new(r.right() - 12.0, ty), Align2::RIGHT_CENTER, ago(n.at), ui_font(11.5), weak);
    }
    let resp = if opens { resp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { resp };
    if opens && resp.clicked() {
        acts.push(Act::NoteClick(n.id));
        acts.push(Act::Close);
    }
}
