//! Small drawn controls shared by the Rivet apps (they look the same in every icon theme).

use eframe::egui::{self, Stroke, Vec2};

/// A small flat button with a drawn up / down chevron (looks the same in every icon theme).
pub fn arrow_button(ui: &mut egui::Ui, up: bool, enabled: bool) -> egui::Response {
    let resp = ui.add_enabled(enabled, egui::Button::new("").frame_when_inactive(false).min_size(Vec2::new(24.0, 22.0)));
    let c = resp.rect.center();
    let col = if enabled { ui.visuals().text_color() } else { ui.visuals().weak_text_color().gamma_multiply(0.5) };
    let (dy, tip) = if up { (-2.5, 2.5) } else { (2.5, -2.5) };
    let stroke = Stroke::new(1.8, col);
    ui.painter().line_segment([c + Vec2::new(-4.5, tip), c + Vec2::new(0.0, dy)], stroke);
    ui.painter().line_segment([c + Vec2::new(0.0, dy), c + Vec2::new(4.5, tip)], stroke);
    resp
}

/// A small flat button with a drawn trash can (red on hover).
pub fn trash_button(ui: &mut egui::Ui) -> egui::Response {
    let resp = ui.add(egui::Button::new("").frame_when_inactive(false).min_size(Vec2::new(24.0, 22.0)));
    let c = resp.rect.center();
    let col = if resp.hovered() { crate::theme::status(ui.ctx()).danger } else { ui.visuals().text_color() };
    let s = Stroke::new(1.5, col);
    let p = ui.painter();
    // lid and handle
    p.line_segment([c + Vec2::new(-5.5, -4.0), c + Vec2::new(5.5, -4.0)], s);
    p.line_segment([c + Vec2::new(-2.0, -4.0), c + Vec2::new(-2.0, -6.0)], s);
    p.line_segment([c + Vec2::new(-2.0, -6.0), c + Vec2::new(2.0, -6.0)], s);
    p.line_segment([c + Vec2::new(2.0, -6.0), c + Vec2::new(2.0, -4.0)], s);
    // can body
    p.add(egui::Shape::closed_line(
        vec![c + Vec2::new(-4.2, -2.0), c + Vec2::new(4.2, -2.0), c + Vec2::new(3.4, 6.0), c + Vec2::new(-3.4, 6.0)],
        s,
    ));
    p.line_segment([c + Vec2::new(-1.3, 0.0), c + Vec2::new(-1.1, 4.0)], Stroke::new(1.1, col));
    p.line_segment([c + Vec2::new(1.3, 0.0), c + Vec2::new(1.1, 4.0)], Stroke::new(1.1, col));
    resp
}

/// A button for an action that removes or deletes something: outlined in the theme's danger
/// color, with a faint tint on hover, so it reads as a warning without shouting.
pub fn danger_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    let danger = crate::theme::status(ui.ctx()).danger;
    let label = crate::palette::mix(ui.visuals().text_color(), danger, 0.6);
    ui.scope(|ui| {
        let w = &mut ui.visuals_mut().widgets;
        w.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        w.inactive.bg_stroke = Stroke::new(1.0, danger.gamma_multiply(0.7));
        w.hovered.weak_bg_fill = danger.gamma_multiply(0.14);
        w.hovered.bg_stroke = Stroke::new(1.0, danger);
        w.active.weak_bg_fill = danger.gamma_multiply(0.24);
        w.active.bg_stroke = Stroke::new(1.0, danger);
        ui.add(egui::Button::new(egui::RichText::new(text).color(label)).corner_radius(crate::theme::r_sm()))
    })
    .inner
}
