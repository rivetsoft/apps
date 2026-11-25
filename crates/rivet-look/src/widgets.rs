//! Small drawn controls shared by the Rivet apps (they look the same in every icon theme).

use eframe::egui::{self, Stroke, Vec2};

/// Return was pressed for a dialog's highlighted button. True only when no control in the
/// dialog has the keyboard: a focused button, checkbox or text field takes Return itself, so
/// Tab to Cancel and Return cancels. Ask before drawing the dialog. It is false in the frame
/// a dialog opens, so the Return that opened it can't also answer it.
pub fn return_default(ctx: &egui::Context) -> bool {
    if !ctx.input(|i| i.modifiers.is_none() && i.key_pressed(egui::Key::Enter)) {
        return false;
    }
    let (modal, focused) = ctx.memory(|m| (m.top_modal_layer(), m.focused()));
    let Some(modal) = modal else { return false };
    focused.is_none_or(|id| ctx.read_response(id).is_none_or(|r| r.layer_id != modal))
}

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

#[cfg(test)]
mod return_tests {
    use super::*;
    use eframe::egui::{Event, Key, Modifiers, RawInput};

    /// One frame of a dialog with a highlighted button, Cancel and (optionally) a text field
    /// that takes the keyboard, with `key` pressed. Gives (Return as the default, OK clicked,
    /// Cancel clicked).
    fn frame(ctx: &egui::Context, key: Option<Key>, mut field: Option<&mut String>) -> (bool, bool, bool) {
        let events = key.map(|key| Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }).into_iter().collect();
        let mut out = (false, false, false);
        let mut full = ctx.run_ui(RawInput { events, ..Default::default() }, |ui| {
            let ctx = ui.ctx().clone();
            out.0 = return_default(&ctx);
            egui::Modal::new(egui::Id::unique("test-dialog")).show(&ctx, |ui| {
                if let Some(text) = field.as_deref_mut() {
                    let r = ui.text_edit_singleline(text);
                    if !r.has_focus() && !r.lost_focus() {
                        r.request_focus();
                    }
                }
                out.1 = ui.button("OK").clicked();
                out.2 = ui.button("Cancel").clicked();
            });
        });
        full.textures_delta.clear();
        out
    }

    #[test]
    fn return_is_the_highlighted_button_until_tab_moves_the_keyboard() {
        let ctx = egui::Context::default();
        // The frame the dialog opens in: a Return that opened it doesn't answer it.
        assert_eq!(frame(&ctx, Some(Key::Enter), None), (false, false, false));
        frame(&ctx, None, None);
        // Nothing focused: Return is the default.
        assert_eq!(frame(&ctx, Some(Key::Enter), None), (true, false, false));
        // Tab reaches OK, then Cancel; Return then presses Cancel and only Cancel.
        frame(&ctx, Some(Key::Tab), None);
        frame(&ctx, None, None);
        assert_eq!(frame(&ctx, Some(Key::Enter), None), (false, true, false));
        frame(&ctx, Some(Key::Tab), None);
        frame(&ctx, None, None);
        assert_eq!(frame(&ctx, Some(Key::Enter), None), (false, false, true));
    }

    #[test]
    fn return_in_a_text_field_is_left_to_the_field() {
        let ctx = egui::Context::default();
        let mut text = String::new();
        frame(&ctx, None, Some(&mut text));
        frame(&ctx, None, Some(&mut text));
        assert_eq!(frame(&ctx, Some(Key::Enter), Some(&mut text)), (false, false, false));
        // Tab leaves the field for OK.
        frame(&ctx, Some(Key::Tab), Some(&mut text));
    }
}
