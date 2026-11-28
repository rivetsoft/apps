//! The password dialog, shown in the middle of the screen when an app needs an administrator's
//! say-so (polkit.rs is the side that talks to polkit): what's being asked for, the field,
//! what PAM said, and Cancel and Allow. Esc cancels; a click outside does nothing, so a stray
//! click can't turn a request away.

use std::time::Duration;

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_md;

use crate::glyph;
use crate::ui::{Act, Kit, Model, on, ui_font, wrapped};

pub const WIDTH: f32 = 400.0;
pub const PAD: f32 = 20.0;
const HEAD: f32 = 44.0;
const FIELD: f32 = 38.0;
const NOTE: f32 = 24.0;
const BUTTONS: f32 = 34.0;
const INNER: f32 = WIDTH - 2.0 * PAD;

fn message_job(text: &str, color: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::single_section(text.to_string(), egui::TextFormat { font_id: ui_font(13.5), color, ..Default::default() });
    job.wrap = egui::text::TextWrapping { max_width: INNER, max_rows: 4, break_anywhere: false, overflow_character: Some('…') };
    job
}

pub fn height(m: &Model, ctx: &egui::Context) -> f32 {
    let Some(a) = &m.extra.auth else { return 0.0 };
    let text = ctx.fonts_mut(|f| f.layout_job(message_job(&a.message, Color32::WHITE))).size().y;
    PAD * 2.0 + HEAD + text + 16.0 + FIELD + if a.note.is_some() { NOTE } else { 0.0 } + 18.0 + BUTTONS
}

/// `typed` is what's in the field; `round` is the try it was typed for (a new try empties it).
pub fn popup(ui: &mut egui::Ui, typed: &mut String, round: &mut u64, m: &mut Model, kit: &mut Kit) {
    let Some(a) = m.extra.auth.clone() else { return };
    if *round != a.round {
        *round = a.round;
        // Overwritten before it's let go, as far as a String allows.
        unsafe { typed.as_bytes_mut() }.iter_mut().for_each(|b| *b = 0);
        typed.clear();
    }
    let v = ui.visuals().clone();
    let accent = kit.accent(m.s);
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    let area = ui.max_rect();
    let p = ui.painter().clone();

    // Whose password, beside a lock.
    let c = Pos2::new(area.left() + 16.0, area.top() + 16.0);
    p.circle_filled(c, 16.0, mix(v.panel_fill, accent, 0.22));
    glyph::lock(&p, c, accent);
    let title = match &a.other_user {
        Some(u) => format!("Enter {u}'s password"),
        None => "Enter your password".to_string(),
    };
    p.text(Pos2::new(c.x + 28.0, c.y), Align2::LEFT_CENTER, title, ui_font(16.0), v.strong_text_color());

    // What it's for, in polkit's words.
    let g = p.layout_job(message_job(&a.message, v.text_color()));
    let mut y = area.top() + HEAD;
    let text_h = g.size().y;
    p.galley(Pos2::new(area.left(), y), g, v.text_color());
    y += text_h + 16.0;

    // The field: dots while the helper is busy, else what PAM asked for as its hint.
    let field = Rect::from_min_size(Pos2::new(area.left(), y), Vec2::new(area.width(), FIELD));
    let waiting = a.busy || a.prompt.is_none();
    p.rect_filled(field, r_md(), mix(v.panel_fill, v.text_color(), 0.07));
    p.rect_stroke(field, r_md(), Stroke::new(1.0, if waiting { mix(v.panel_fill, v.text_color(), 0.14) } else { accent }), egui::StrokeKind::Inside);
    if waiting {
        let t = ui.input(|i| i.time) as f32;
        for i in 0..3 {
            let k = 0.3 + 0.7 * (((t * 4.0 - i as f32 * 0.6).sin() + 1.0) / 2.0);
            p.circle_filled(Pos2::new(field.center().x + (i as f32 - 1.0) * 12.0, field.center().y), 3.5, v.text_color().gamma_multiply(k));
        }
        ui.ctx().request_repaint_after(Duration::from_millis(50));
    } else {
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field.shrink2(Vec2::new(12.0, 8.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let te = child.add(egui::TextEdit::singleline(typed).id(egui::Id::unique("auth-field")).password(!a.echo).frame(egui::Frame::NONE).font(ui_font(14.0)).hint_text(a.prompt.as_deref().unwrap_or("Password")).desired_width(f32::INFINITY)).accessible_name(a.prompt.as_deref().unwrap_or("Password"));
        if !te.has_focus() {
            te.request_focus();
        }
    }
    y += FIELD;
    if let Some((text, bad)) = &a.note {
        let col = if *bad { danger } else { v.weak_text_color() };
        let g = wrapped(&p, text, ui_font(12.5), col, INNER, 1);
        p.galley(Pos2::new(area.left() + 2.0, y + (NOTE - g.size().y) / 2.0 + 2.0), g, col);
        y += NOTE;
    }
    y += 18.0;

    // Cancel and Allow, at the right.
    let enter = ui.input(|i| i.key_pressed(Key::Enter));
    let button = |ui: &mut egui::Ui, right: f32, label: &str, main: bool, live: bool| -> (bool, f32) {
        let g = p.layout_no_wrap(label.to_string(), ui_font(13.5), Color32::WHITE);
        let r = Rect::from_min_max(Pos2::new(right - g.size().x - 36.0, y), Pos2::new(right, y + BUTTONS));
        let resp = ui.interact(r, egui::Id::unique(("auth-btn", label)), if live { Sense::click() } else { Sense::hover() });
        let hot = live && resp.hovered();
        let fill = if main { mix(accent, v.text_color(), if hot { 0.12 } else { 0.0 }) } else { mix(v.panel_fill, v.text_color(), if hot { 0.16 } else { 0.09 }) };
        let fill = if live { fill } else { fill.gamma_multiply(0.45) };
        p.rect_filled(r, r_md(), fill);
        let col = if main { on(accent) } else { v.text_color() };
        p.text(r.center(), Align2::CENTER_CENTER, label, ui_font(13.5), if live { col } else { col.gamma_multiply(0.6) });
        (live && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked(), r.left())
    };
    let ready = !waiting && (!typed.is_empty() || a.echo);
    let (allow, left) = button(ui, area.right(), "Allow", true, ready);
    let (cancel, _) = button(ui, left - 8.0, "Cancel", false, true);
    if cancel {
        m.acts.push(Act::Close);
    } else if ready && (allow || enter) {
        m.acts.push(Act::Auth(std::mem::take(typed)));
    }
}
