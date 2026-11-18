//! The Launcher page: how the launcher shows the apps (a grid or a list), how big it is, and
//! what else it does as you type.

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_md;

use super::panel_editor::segmented;
use super::{SettingsApp, grid, hint, section};
use crate::config::{LaunchPlace, LaunchSize, LaunchView};

/// A small drawing of the launcher as it's set: the search field over tiles or rows.
fn sketch(ui: &mut egui::Ui, view: LaunchView, size: LaunchSize, icon: u32, dim: bool) {
    let v = ui.visuals().clone();
    let accent = v.selection.bg_fill;
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width().min(520.0), 190.0), Sense::hover());
    let p = ui.painter_at(r);
    // A screen behind it, darker when the launcher dims it.
    let back = mix(accent, egui::Color32::from_rgb(20, 22, 30), 0.72);
    p.rect_filled(r, r_md(), if dim { mix(back, egui::Color32::BLACK, 0.3) } else { back });
    // A fifth of the real thing's size.
    let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(2400.0, 1400.0));
    let real = crate::launcher::size(&crate::config::Settings { launch_view: view, launch_size: size, ..Default::default() }, full);
    let card = Rect::from_center_size(r.center(), real * 0.27);
    p.rect_filled(card, r_md(), v.panel_fill);
    p.rect_stroke(card, r_md(), Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.14)), egui::StrokeKind::Inside);
    let inner = card.shrink(7.0);
    p.rect_filled(Rect::from_min_size(inner.min, Vec2::new(inner.width(), 12.0)), 4.0, mix(v.panel_fill, v.text_color(), 0.09));
    let area = Rect::from_min_max(inner.min + Vec2::new(0.0, 17.0), inner.max);
    let ic = icon as f32 * 0.27;
    let soft = mix(v.panel_fill, v.text_color(), 0.22);
    match view {
        LaunchView::Grid => {
            let tile = Vec2::new((icon as f32 + 56.0) * 0.27, (icon as f32 + 50.0) * 0.27);
            let cols = ((area.width() + 2.0) / (tile.x + 2.0)).floor().max(1.0) as usize;
            let rows = (area.height() / (tile.y + 2.0)).floor() as usize;
            for k in 0..cols * rows {
                let t = Rect::from_min_size(area.min + Vec2::new((k % cols) as f32 * (tile.x + 2.0), (k / cols) as f32 * (tile.y + 2.0)), tile);
                if k == 0 {
                    p.rect_filled(t, 3.0, accent.gamma_multiply(0.3));
                }
                p.rect_filled(Rect::from_center_size(Pos2::new(t.center().x, t.top() + 3.0 + ic / 2.0), Vec2::splat(ic)), 3.0, if k == 0 { accent } else { soft });
                p.rect_filled(Rect::from_center_size(Pos2::new(t.center().x, t.bottom() - 6.0), Vec2::new(tile.x * 0.6, 2.5)), 1.0, soft.gamma_multiply(0.7));
            }
        }
        LaunchView::List => {
            let li = (icon as f32 * 0.67).max(18.0);
            let row = (li + 14.0) * 0.27 + 1.0;
            let n = (area.height() / row).floor() as usize;
            for k in 0..n {
                let t = Rect::from_min_size(area.min + Vec2::new(0.0, k as f32 * row), Vec2::new(area.width(), row - 1.0));
                if k == 0 {
                    p.rect_filled(t, 3.0, accent.gamma_multiply(0.3));
                }
                let s = li * 0.27;
                p.rect_filled(Rect::from_center_size(Pos2::new(t.left() + 4.0 + s / 2.0, t.center().y), Vec2::splat(s)), 2.0, if k == 0 { accent } else { soft });
                p.rect_filled(Rect::from_min_size(Pos2::new(t.left() + 9.0 + s, t.center().y - 1.25), Vec2::new(area.width() * (0.3 + 0.1 * ((k * 7) % 4) as f32), 2.5)), 1.0, soft.gamma_multiply(0.7));
            }
        }
    }
}

impl SettingsApp {
    pub(super) fn launcher_page(&mut self, ui: &mut egui::Ui) {
        section(ui, "Layout", &mut |ui| {
            let s = &mut self.s;
            sketch(ui, s.launch_view, s.launch_size, s.launch_icon, s.launch_dim);
            ui.add_space(12.0);
            grid(ui, "shell-launcher", &mut |ui| {
                ui.label("Apps as");
                segmented(ui, &mut s.launch_view, &[(LaunchView::Grid, "Grid"), (LaunchView::List, "List")]);
                ui.end_row();
                ui.label("Opens");
                segmented(ui, &mut s.launch_place, &[(LaunchPlace::Center, "In the Middle"), (LaunchPlace::Panel, "From the Panel")]);
                ui.end_row();
                ui.label("Size");
                segmented(ui, &mut s.launch_size, &[(LaunchSize::Small, "Small"), (LaunchSize::Medium, "Medium"), (LaunchSize::Large, "Large")]);
                ui.end_row();
                ui.label("Icons");
                ui.add(egui::Slider::new(&mut s.launch_icon, 24..=64).step_by(2.0).suffix(" pt"));
                ui.end_row();
            });
            if s.launch_place == LaunchPlace::Panel {
                hint(ui, "It opens from the panel it was opened on, like that panel's other popups (joined to it when they are). Opened with a key, it comes from the screen's first panel, at the end with the Apps button; with no panel there, in the middle.");
            }
            hint(ui, match s.launch_view {
                LaunchView::Grid => "Icons in rows, each over its name; the arrow keys move in every direction.",
                LaunchView::List => "A row for each app, with smaller icons; up and down move through them.",
            });
        });
        section(ui, "Text", &mut |ui| {
            let follow = super::font_pick::like_appearance(&self.s.font);
            let s = &mut self.s;
            grid(ui, "shell-launcher-text", &mut |ui| {
                ui.label("Font");
                ui.horizontal(|ui| {
                    super::font_pick::font_pick(ui, "shell-launcher-font", &mut s.launcher_font, &follow);
                    if !s.launcher_font.is_empty() && ui.small_button("Reset").clicked() {
                        s.launcher_font.clear();
                    }
                });
                ui.end_row();
            });
            hint(ui, "The launcher's own font; the rest of the shell keeps Appearance's.");
        });
        section(ui, "As You Type", &mut |ui| {
            let s = &mut self.s;
            ui.checkbox(&mut s.launch_calc, "Answer sums");
            hint(ui, "Type 12*4 or (3+5)/2 and Enter copies the answer.");
        });
        section(ui, "Behind It", &mut |ui| {
            let middle = self.s.launch_place == LaunchPlace::Center;
            ui.add_enabled(middle, egui::Checkbox::new(&mut self.s.launch_dim, "Dim the screen"));
            if !middle {
                hint(ui, "Only when it opens in the middle.");
            }
        });
        section(ui, "Keys", &mut |ui| {
            hint(ui, "Bind this in your compositor's settings to open the launcher:");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("rivet-shell msg launcher").monospace().size(12.0));
        });
    }
}
