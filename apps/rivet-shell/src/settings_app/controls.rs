//! The Settings window's own controls, one shape for each job, so a page reads at a glance:
//! a switch turns something on or off, a segmented row picks one of a few, a pill with a symbol
//! does something, and Reset is a quiet link. Sliders, lists and fields are egui's own.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Vec2};
use rivet_look::fonts::ui_font;
use rivet_look::palette::mix;
use rivet_look::theme::r_sm;

/// The symbol on a button, saying what kind of thing it does.
#[derive(Clone, Copy, PartialEq)]
pub(super) enum Icon {
    Plus,
    Copy,
    Reset,
    Folder,
    Picture,
    Play,
    Download,
    /// Opens something outside this window.
    Out,
    /// Takes something away without deleting anything.
    Off,
    Back,
    Forward,
    Check,
    /// A panel under a panel.
    Layers,
    Trash,
}

fn draw_icon(p: &egui::Painter, icon: Icon, c: Pos2, col: Color32) {
    let s = Stroke::new(1.5, col);
    let at = |x: f32, y: f32| c + Vec2::new(x, y);
    match icon {
        Icon::Plus => {
            p.line_segment([at(-4.5, 0.0), at(4.5, 0.0)], s);
            p.line_segment([at(0.0, -4.5), at(0.0, 4.5)], s);
        }
        Icon::Copy => {
            p.rect_stroke(Rect::from_min_max(at(-5.5, -5.5), at(2.0, 2.0)), 1.5, s, egui::StrokeKind::Middle);
            p.rect_stroke(Rect::from_min_max(at(-2.0, -2.0), at(5.5, 5.5)), 1.5, s, egui::StrokeKind::Middle);
        }
        Icon::Reset => {
            // An arrow circling back.
            let pts: Vec<Pos2> = (0..=14).map(|i| { let a = -0.5 + i as f32 * 0.33; at(-a.cos() * 4.8, -a.sin() * 4.8) }).collect();
            let tip = pts[0];
            p.add(Shape::line(pts, Stroke::new(1.4, col)));
            p.add(Shape::convex_polygon(vec![tip + Vec2::new(-2.6, -2.2), tip + Vec2::new(2.4, -1.0), tip + Vec2::new(-1.2, 2.8)], col, Stroke::NONE));
        }
        Icon::Folder => {
            p.add(Shape::closed_line(vec![at(-6.0, -4.5), at(-1.5, -4.5), at(0.0, -2.5), at(6.0, -2.5), at(6.0, 5.0), at(-6.0, 5.0)], s));
        }
        Icon::Picture => {
            p.rect_stroke(Rect::from_center_size(c, Vec2::new(12.5, 10.5)), 2.0, s, egui::StrokeKind::Middle);
            p.circle_filled(at(-2.5, -1.8), 1.3, col);
            p.add(Shape::line(vec![at(-4.5, 3.2), at(-0.8, 0.2), at(1.4, 2.0), at(3.2, 0.4), at(5.0, 2.6)], Stroke::new(1.3, col)));
        }
        Icon::Play => {
            p.add(Shape::convex_polygon(vec![at(-3.0, -5.0), at(5.0, 0.0), at(-3.0, 5.0)], col, Stroke::NONE));
        }
        Icon::Download => {
            p.line_segment([at(0.0, -5.5), at(0.0, 2.0)], s);
            p.add(Shape::line(vec![at(-3.5, -1.2), at(0.0, 2.3), at(3.5, -1.2)], s));
            p.line_segment([at(-5.0, 5.5), at(5.0, 5.5)], s);
        }
        Icon::Out => {
            p.line_segment([at(-4.0, 4.0), at(4.0, -4.0)], s);
            p.add(Shape::line(vec![at(-1.5, -4.2), at(4.2, -4.2), at(4.2, 1.5)], s));
        }
        Icon::Off => {
            p.circle_stroke(c, 5.5, s);
            p.line_segment([at(-3.8, 3.8), at(3.8, -3.8)], s);
        }
        Icon::Back => {
            p.line_segment([at(-5.0, 0.0), at(5.0, 0.0)], s);
            p.add(Shape::line(vec![at(-1.2, -3.8), at(-5.0, 0.0), at(-1.2, 3.8)], s));
        }
        Icon::Forward => {
            p.line_segment([at(-5.0, 0.0), at(5.0, 0.0)], s);
            p.add(Shape::line(vec![at(1.2, -3.8), at(5.0, 0.0), at(1.2, 3.8)], s));
        }
        Icon::Check => {
            p.add(Shape::line(vec![at(-4.5, 0.2), at(-1.5, 3.2), at(4.8, -3.6)], Stroke::new(1.7, col)));
        }
        Icon::Trash => {
            let t = Stroke::new(1.3, col);
            p.line_segment([at(-5.0, -3.5), at(5.0, -3.5)], t);
            p.add(Shape::line(vec![at(-1.8, -3.5), at(-1.8, -5.3), at(1.8, -5.3), at(1.8, -3.5)], t));
            p.add(Shape::line(vec![at(-3.8, -3.5), at(-3.1, 5.3), at(3.1, 5.3), at(3.8, -3.5)], t));
        }
        Icon::Layers => {
            p.rect_stroke(Rect::from_min_max(at(-6.0, -5.0), at(6.0, -0.5)), 1.5, s, egui::StrokeKind::Middle);
            p.rect_filled(Rect::from_min_max(at(-4.0, 2.0), at(4.0, 5.0)), 1.2, col);
        }
    }
}

fn pill(ui: &mut egui::Ui, icon: Icon, text: &str, kind: Kind) -> Response {
    let v = ui.visuals().clone();
    let g = ui.painter().layout_no_wrap(text.to_string(), ui_font(13.0), Color32::PLACEHOLDER);
    let h = 28.0;
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 44.0, h), Sense::click());
    let on = ui.is_enabled();
    let w = if resp.is_pointer_button_down_on() { &v.widgets.active } else if resp.hovered() && on { &v.widgets.hovered } else { &v.widgets.inactive };
    let accent = v.selection.bg_fill;
    let (fill, line, ink, mark) = match kind {
        // Greyed while it can't be used, so it doesn't call for a click that does nothing.
        Kind::Strong if !on => (w.weak_bg_fill, w.bg_stroke, v.weak_text_color(), v.weak_text_color()),
        Kind::Strong => (if resp.hovered() { mix(accent, v.text_color(), 0.12) } else { accent }, Stroke::NONE, v.selection.stroke.color, v.selection.stroke.color),
        Kind::Plain => (w.weak_bg_fill, w.bg_stroke, v.text_color(), accent),
        Kind::Danger => {
            let red = rivet_look::theme::status(ui.ctx()).danger;
            let ink = mix(v.text_color(), red, 0.6);
            (if resp.hovered() && on { red.gamma_multiply(0.16) } else { Color32::TRANSPARENT }, Stroke::new(1.0, red.gamma_multiply(if resp.hovered() { 1.0 } else { 0.7 })), ink, ink)
        }
    };
    let p = ui.painter();
    p.rect_filled(r, h / 2.0, fill);
    p.rect_stroke(r, h / 2.0, line, egui::StrokeKind::Inside);
    draw_icon(p, icon, Pos2::new(r.left() + 19.0, r.center().y), mark);
    p.galley(Pos2::new(r.left() + 31.0, r.center().y - g.size().y / 2.0), g, ink);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

#[derive(Clone, Copy)]
enum Kind {
    Plain,
    Strong,
    Danger,
}

/// A button that does something: a pill with a symbol for what.
pub(super) fn button(ui: &mut egui::Ui, icon: Icon, text: &str) -> Response {
    pill(ui, icon, text, Kind::Plain)
}

/// A button that can't be used for now.
pub(super) fn button_if(ui: &mut egui::Ui, enabled: bool, icon: Icon, text: &str) -> Response {
    ui.add_enabled_ui(enabled, |ui| pill(ui, icon, text, Kind::Plain)).inner
}

pub(super) fn primary_if(ui: &mut egui::Ui, enabled: bool, icon: Icon, text: &str) -> Response {
    ui.add_enabled_ui(enabled, |ui| pill(ui, icon, text, Kind::Strong)).inner
}

/// A button that removes something: outlined in the theme's danger color.
pub(super) fn danger(ui: &mut egui::Ui, text: &str) -> Response {
    pill(ui, Icon::Trash, text, Kind::Danger)
}

pub(super) fn danger_if(ui: &mut egui::Ui, enabled: bool, text: &str) -> Response {
    ui.add_enabled_ui(enabled, |ui| pill(ui, Icon::Trash, text, Kind::Danger)).inner
}

/// The one button on a card that the card is there for, filled with the accent.
pub(super) fn primary(ui: &mut egui::Ui, icon: Icon, text: &str) -> Response {
    pill(ui, icon, text, Kind::Strong)
}

/// "Reset" after a control: a quiet link, since it's there on every row that's been changed.
pub(super) fn reset(ui: &mut egui::Ui) -> Response {
    let v = ui.visuals().clone();
    let g = ui.painter().layout_no_wrap("Reset".to_string(), ui_font(12.0), Color32::PLACEHOLDER);
    let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 26.0, 24.0), Sense::click());
    let col = if resp.hovered() { v.text_color() } else { v.weak_text_color() };
    if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), v.widgets.inactive.weak_bg_fill);
    }
    draw_icon(ui.painter(), Icon::Reset, Pos2::new(r.left() + 11.0, r.center().y), col);
    ui.painter().galley(Pos2::new(r.left() + 21.0, r.center().y - g.size().y / 2.0), g, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A button that is only its symbol (copy this, open that), named in its tooltip.
pub(super) fn icon_button(ui: &mut egui::Ui, icon: Icon) -> Response {
    let v = ui.visuals().clone();
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(26.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
    }
    draw_icon(ui.painter(), icon, r.center(), if resp.hovered() { v.text_color() } else { v.weak_text_color() });
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

const TRACK: Vec2 = Vec2::new(32.0, 18.0);

fn draw_switch(ui: &egui::Ui, track: Rect, on: bool, id: egui::Id, hovered: bool) {
    let v = ui.visuals();
    let t = ui.ctx().animate_bool_with_time(id.with("switch"), on, 0.12);
    let off = if hovered { v.widgets.hovered.bg_fill } else { v.widgets.inactive.bg_fill };
    let p = ui.painter();
    p.rect_filled(track, TRACK.y / 2.0, mix(off, v.selection.bg_fill, t));
    let x = egui::lerp(track.left() + 9.0..=track.right() - 9.0, t);
    p.circle_filled(Pos2::new(x, track.center().y), 6.5, mix(v.text_color(), v.selection.stroke.color, t));
}

/// On or off: a switch, then what it turns on. Anywhere a checkbox would go.
pub(super) fn switch(ui: &mut egui::Ui, on: &mut bool, text: impl ToString) -> Response {
    let text = text.to_string();
    let col = ui.visuals().text_color();
    let gap = if text.is_empty() { 0.0 } else { 10.0 };
    let g = ui.painter().layout(text, egui::TextStyle::Body.resolve(ui.style()), col, (ui.available_width() - TRACK.x - gap).max(80.0));
    let (r, mut resp) = ui.allocate_exact_size(Vec2::new(TRACK.x + gap + g.size().x, g.size().y.max(24.0)), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    let track = Rect::from_min_size(Pos2::new(r.left(), r.top() + (24.0 - TRACK.y) / 2.0), TRACK);
    draw_switch(ui, track, *on, resp.id, resp.hovered());
    ui.painter().galley(Pos2::new(track.right() + gap, r.top() + (24.0_f32.max(g.size().y) - g.size().y) / 2.0), g, col);
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// The switch a whole card hangs on (Show the dock): its name across the card's top, the
/// switch at the far end, and a line under it before what it governs.
pub(super) fn master(ui: &mut egui::Ui, on: &mut bool, text: impl ToString) -> Response {
    let text = text.to_string();
    let v = ui.visuals().clone();
    let (r, mut resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 26.0), Sense::click());
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, text, ui_font(14.0), v.strong_text_color());
    draw_switch(ui, Rect::from_center_size(Pos2::new(r.right() - TRACK.x / 2.0, r.center().y), TRACK), *on, resp.id, resp.hovered());
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A hairline between two parts of a card.
pub(super) fn rule(ui: &mut egui::Ui) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
    ui.painter().rect_filled(r, 0.0, ui.visuals().widgets.inactive.bg_stroke.color.gamma_multiply(0.7));
    ui.add_space(8.0);
}

/// What marks an area as a picture of the settings, not something to click: an eye and
/// "Preview" over it. Call before the preview, and `preview_edge` once its place is known.
pub(super) fn preview_head(ui: &mut egui::Ui) {
    let weak = ui.visuals().weak_text_color();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    let c = egui::pos2(r.left() + 8.0, r.center().y);
    let p = ui.painter();
    p.add(egui::epaint::EllipseShape::stroke(c, Vec2::new(6.5, 4.0), Stroke::new(1.3, weak)));
    p.circle_filled(c, 1.8, weak);
    p.text(egui::pos2(r.left() + 22.0, r.center().y), Align2::LEFT_CENTER, "PREVIEW", ui_font(10.5), weak);
    ui.add_space(6.0);
}

/// The dashed line around a preview, a little outside it.
pub(super) fn preview_edge(ui: &egui::Ui, r: Rect) {
    let r = r.expand(4.0);
    let stroke = Stroke::new(1.0, rivet_look::palette::mix(ui.visuals().panel_fill, ui.visuals().text_color(), 0.34));
    let k = 6.0;
    for (a, b) in [(r.left_top() + Vec2::new(k, 0.0), r.right_top() - Vec2::new(k, 0.0)), (r.right_top() + Vec2::new(0.0, k), r.right_bottom() - Vec2::new(0.0, k)), (r.right_bottom() - Vec2::new(k, 0.0), r.left_bottom() + Vec2::new(k, 0.0)), (r.left_bottom() - Vec2::new(0.0, k), r.left_top() + Vec2::new(0.0, k))] {
        ui.painter().extend(egui::Shape::dashed_line(&[a, b], stroke, 5.0, 4.0));
    }
}

/// A small heading over a few related rows on a long card.
pub(super) fn subhead(ui: &mut egui::Ui, text: &str) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 18.0), Sense::hover());
    ui.painter().text(r.left_center(), Align2::LEFT_CENTER, text.to_uppercase(), ui_font(10.5), ui.visuals().weak_text_color());
    ui.add_space(2.0);
}

/// A command to bind to a key: what it does, the command in a well to select, and Copy.
pub(super) fn command(ui: &mut egui::Ui, what: &str, cmd: &str) {
    let v = ui.visuals().clone();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(184.0, 28.0), Sense::hover());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, what, egui::TextStyle::Body.resolve(ui.style()), v.text_color());
        egui::Frame::NONE.fill(v.widgets.inactive.weak_bg_fill).corner_radius(r_sm()).inner_margin(egui::Margin::symmetric(8, 4)).show(ui, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(cmd).monospace().size(12.0)).selectable(true));
        });
        // The part to copy: "volume up | down" lists choices, so the first is the one copied.
        let id = egui::Id::unique(("copied", cmd));
        let at: Option<f64> = ui.ctx().data(|d| d.get_temp(id));
        let now = ui.input(|i| i.time);
        let done = at.is_some_and(|t| now - t < 1.5);
        if icon_button(ui, if done { Icon::Check } else { Icon::Copy }).on_hover_text("Copy").clicked() {
            ui.ctx().copy_text(cmd.split(" | ").next().unwrap_or(cmd).to_string());
            ui.ctx().data_mut(|d| d.insert_temp(id, now));
        }
    });
}
