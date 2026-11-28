//! The look editor (corners, opacity, background, highlight, outline) on the Appearance page,
//! the Dock page and each panel. Each row starts with a chip for the level above (Appearance's
//! value, or the default on Appearance itself); changing the row's control gives it its own.

use eframe::egui::{self, Color32, Pos2, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_sm;

use super::{SettingsApp, grid, hint, section};
use crate::style::{BASE, Resolved, Role, Roles, Style};

fn pct(x: f32) -> String {
    format!("{}%", (x * 100.0).round())
}

/// After a row's control: a Reset button, only once the row differs from the level above.
/// True when clicked, and the row goes back to following it.
fn reset(ui: &mut egui::Ui, changed: bool, parent: &str, shows: String) -> bool {
    changed && super::controls::reset(ui).on_hover_text(format!("Back to {parent}: {shows}")).clicked()
}

/// A row of colour choices, each a swatch and its name.
pub(super) fn roles(ui: &mut egui::Ui, value: Role, choices: &[Role], colors: &Roles, surface: Color32, enabled: bool) -> Option<Role> {
    let v = ui.visuals().clone();
    let mut picked = None;
    for &role in choices {
        let col = colors.get(role, surface);
        let g = ui.painter().layout_no_wrap(role.label().to_string(), rivet_look::fonts::ui_font(12.5), v.text_color());
        let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 36.0, 26.0), if enabled { Sense::click() } else { Sense::hover() });
        let on = role == value;
        let p = ui.painter();
        if on {
            p.rect_filled(r, r_sm(), mix(v.extreme_bg_color, v.selection.bg_fill, 0.25));
            p.rect_stroke(r, r_sm(), Stroke::new(1.5, v.selection.bg_fill), egui::StrokeKind::Inside);
        } else if enabled && resp.hovered() {
            p.rect_filled(r, r_sm(), mix(v.extreme_bg_color, v.text_color(), 0.08));
        }
        let fade = |c: Color32| if enabled { c } else { c.gamma_multiply(0.45) };
        let c = Pos2::new(r.left() + 15.0, r.center().y);
        p.circle_filled(c, 7.5, fade(col));
        p.circle_stroke(c, 7.5, Stroke::new(1.0, fade(mix(col, v.text_color(), 0.3))));
        p.galley(Pos2::new(r.left() + 28.0, r.center().y - g.size().y / 2.0), g, fade(v.text_color()));
        if enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            picked = Some(role);
        }
    }
    picked
}

/// Edits `own`, a look over `parent` (named `parent_name`).
fn editor(ui: &mut egui::Ui, id: &str, own: &mut Style, parent: Resolved, parent_name: &str, colors: &Roles, popups: bool) {
    let surface = ui.visuals().panel_fill;
    let now = own.on(parent);
    super::controls::subhead(ui, "Shape");
    grid(ui, &format!("{id}-shape"), &mut |ui| {
        ui.label("Corners");
        ui.horizontal(|ui| {
            let mut r = now.radius;
            if ui.add(egui::Slider::new(&mut r, 0.0..=24.0).step_by(1.0).fixed_decimals(0).suffix(" pt")).accessible_name("Corners").changed() {
                own.radius = Some(r);
            }
            if reset(ui, own.radius.is_some(), parent_name, format!("{} pt", parent.radius)) {
                own.radius = None;
            }
        });
        ui.end_row();
        ui.label("Opacity");
        ui.horizontal(|ui| {
            let mut o = now.opacity * 100.0;
            if ui.add(egui::Slider::new(&mut o, 30.0..=100.0).step_by(1.0).fixed_decimals(0).suffix("%")).accessible_name("Opacity").changed() {
                own.opacity = Some(o / 100.0);
            }
            if reset(ui, own.opacity.is_some(), parent_name, pct(parent.opacity)) {
                own.opacity = None;
            }
        });
        ui.end_row();
        ui.label("Outline");
        ui.horizontal(|ui| {
            let mut b = now.border;
            super::controls::switch(ui, &mut b, "").accessible_name("Outline");
            if b != now.border {
                own.border = Some(b);
            }
            if reset(ui, own.border.is_some(), parent_name, if parent.border { "On".into() } else { "Off".into() }) {
                own.border = None;
            }
        });
        ui.end_row();
        if popups {
            ui.label("Popups");
            ui.horizontal(|ui| {
                let mut a = now.attach;
                super::panel_editor::segmented(ui, &mut a, &[(false, "Float"), (true, "Attached")]);
                if a != now.attach {
                    own.attach = Some(a);
                }
                if reset(ui, own.attach.is_some(), parent_name, if parent.attach { "Attached".into() } else { "Float".into() }) {
                    own.attach = None;
                }
            })
            .response
            .on_hover_text("Whether menus and popups float off the panel or join it");
            ui.end_row();
        }
    });
    ui.add_space(14.0);
    super::controls::subhead(ui, "Color");
    grid(ui, &format!("{id}-color"), &mut |ui| {
        ui.label("Background");
        ui.horizontal_wrapped(|ui| {
            if let Some(r) = roles(ui, now.fill, &Role::ALL, colors, surface, true) {
                own.fill = Some(r);
            }
            if reset(ui, own.fill.is_some(), parent_name, parent.fill.label().to_string()) {
                own.fill = None;
            }
        });
        ui.end_row();
        ui.label("Tint");
        ui.horizontal(|ui| {
            let tinted = now.fill != Role::Surface;
            ui.add_enabled_ui(tinted, |ui| {
                let mut t = now.tint * 100.0;
                if ui.add(egui::Slider::new(&mut t, 0.0..=100.0).step_by(5.0).fixed_decimals(0).suffix("%")).accessible_name("Tint").on_hover_text("How much of the background color the surface takes").changed() {
                    own.tint = Some(t / 100.0);
                }
                if reset(ui, own.tint.is_some(), parent_name, pct(parent.tint)) {
                    own.tint = None;
                }
            });
        });
        ui.end_row();
        ui.label("Highlight");
        ui.horizontal_wrapped(|ui| {
            if let Some(r) = roles(ui, now.highlight, &Role::ACCENTS, colors, surface, true) {
                own.highlight = Some(r);
            }
            if reset(ui, own.highlight.is_some(), parent_name, parent.highlight.label().to_string()) {
                own.highlight = None;
            }
        });
        ui.end_row();
    });
    if !own.is_empty() {
        super::controls::rule(ui);
        if super::controls::button(ui, super::controls::Icon::Reset, "Reset All").on_hover_text(format!("Every row back to {parent_name}")).clicked() {
            *own = Style::default();
        }
    }
}

impl SettingsApp {
    pub(super) fn look_section(&mut self, ui: &mut egui::Ui) {
        let colors = Roles::new(&self.look, &self.s);
        section(ui, "Panels and dock", &mut |ui| {
            hint(ui, "Every panel and the dock use this look unless you give one its own on its page.");
            ui.add_space(10.0);
            editor(ui, "shell-look-base", &mut self.s.look, BASE, "Default", &colors, true);
            super::controls::rule(ui);
            super::controls::switch(ui, &mut self.s.blur, "Blur what's behind them");
            hint(ui, "When they're see-through, on compositors that can blur, such as Softcore.");
        });
    }

    pub(super) fn dock_look_section(&mut self, ui: &mut egui::Ui) {
        let colors = Roles::new(&self.look, &self.s);
        let parent = self.s.base_look();
        section(ui, "Look", &mut |ui| {
            hint(ui, "Starts from Appearance. What you change here applies to the dock only.");
            ui.add_space(10.0);
            editor(ui, "shell-look-dock", &mut self.s.dock_look, parent, "Appearance", &colors, false);
        });
    }

    pub(super) fn note_look_section(&mut self, ui: &mut egui::Ui) {
        let colors = Roles::new(&self.look, &self.s);
        let parent = self.s.base_look();
        section(ui, "Notification look", &mut |ui| {
            hint(ui, "Starts from Appearance. What you change here applies to notifications only.");
            ui.add_space(10.0);
            editor(ui, "shell-look-notes", &mut self.s.note_look, parent, "Appearance", &colors, false);
        });
    }

    pub(super) fn panel_look_section(&mut self, ui: &mut egui::Ui, sel: usize) {
        let colors = Roles::new(&self.look, &self.s);
        let parent = self.s.base_look();
        let floating = self.s.panels[sel].floating;
        let base_font = super::font_pick::like_appearance(&self.s.font);
        section(ui, "Look", &mut |ui| {
            if !floating {
                hint(ui, "Edge-to-edge panels have square corners.");
            }
            // Where the bar sits: these are the panel's own (not Appearance's).
            let p = &mut self.s.panels[sel];
            super::controls::subhead(ui, "Size and spacing");
            super::grid(ui, &format!("shell-panel-space-{sel}"), &mut |ui| {
                let max = crate::panels::SPACE_MAX;
                ui.label("Font");
                ui.horizontal(|ui| {
                    super::font_pick::font_pick(ui, &format!("shell-panel-font-{sel}"), &mut p.font, &base_font);
                    if !p.font.is_empty() && super::controls::reset(ui).clicked() {
                        p.font.clear();
                    }
                });
                ui.end_row();
                let side = p.vertical();
                ui.label(if side { "Width" } else { "Height" });
                ui.horizontal(|ui| {
                    let mut v = p.bar() as u32;
                    if ui.add(egui::Slider::new(&mut v, crate::panels::SIZE).suffix(" pt")).accessible_name("Width or height").on_hover_text("Symbols and text keep their size.").changed() {
                        p.size = Some(v);
                    }
                    if p.size.is_some() && super::controls::reset(ui).clicked() {
                        p.size = None;
                    }
                });
                ui.end_row();
                ui.label("Between items");
                ui.horizontal(|ui| {
                    let mut v = p.spacing() as u32;
                    if ui.add(egui::Slider::new(&mut v, 0..=crate::panels::SPACING_MAX).suffix(" pt")).accessible_name("Between items").changed() {
                        p.spacing = Some(v);
                    }
                    if p.spacing.is_some() && super::controls::reset(ui).clicked() {
                        p.spacing = None;
                    }
                });
                ui.end_row();
                ui.label("From the screen's edge");
                ui.horizontal(|ui| {
                    let mut o = p.off() as u32;
                    if ui.add_enabled(floating, egui::Slider::new(&mut o, 0..=max).suffix(" pt")).accessible_name("From the screen's edge").on_hover_text("How far the panel floats from the screen's edges").on_disabled_hover_text("Edge-to-edge panels sit on the edge").changed() {
                        p.offset = Some(o);
                    }
                    if p.offset.is_some() && floating && super::controls::reset(ui).clicked() {
                        p.offset = None;
                    }
                });
                ui.end_row();
                ui.label("Space before windows");
                ui.horizontal(|ui| {
                    let mut g = p.gap() as u32;
                    if ui.add(egui::Slider::new(&mut g, 0..=max).suffix(" pt")).accessible_name("Space before windows").changed() {
                        p.gap = Some(g);
                    }
                    if p.gap.is_some() && super::controls::reset(ui).clicked() {
                        p.gap = None;
                    }
                });
                ui.end_row();
            });
            super::controls::rule(ui);
            hint(ui, "Starts from Appearance. What you change here applies to this panel only.");
            ui.add_space(10.0);
            editor(ui, &format!("shell-look-panel-{sel}"), &mut self.s.panels[sel].look, parent, "Appearance", &colors, true);
        });
    }
}
