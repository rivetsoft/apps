//! Appearance's Icons card: the icon themes installed on this system, and the one the shell
//! takes apps' and the tray's icons from.

use eframe::egui::{self, Pos2, Rect, Sense, Vec2};
use rivet_look::theme::r_md;

use super::{SettingsApp, grid, hint, section};

/// The installed themes (looked for once: it reads every icon folder).
fn installed() -> &'static [String] {
    static ALL: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    ALL.get_or_init(|| rivet_look::icons::installed_themes().into_iter().filter(|n| !n.eq_ignore_ascii_case("hicolor") && !n.eq_ignore_ascii_case("default")).collect())
}

/// The desktop's own theme (asked once).
fn desktop() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(rivet_look::icons::detect_system_theme)
}

/// App icons most themes have, to show what one looks like (folders and files are Rivet's own
/// drawings in the shell, so they wouldn't show the theme).
const SAMPLES: [&[&str]; 8] = [
    &["utilities-terminal", "terminal"],
    &["web-browser", "internet-web-browser", "firefox"],
    &["internet-mail", "mail-client", "evolution"],
    &["accessories-text-editor", "text-editor"],
    &["accessories-calculator", "calc"],
    &["multimedia-video-player", "video-player", "vlc"],
    &["system-software-install", "software-store", "org.gnome.Software"],
    &["preferences-system", "preferences-desktop"],
];

impl SettingsApp {
    pub(super) fn icons_section(&mut self, ui: &mut egui::Ui) {
        section(ui, "Icons", &mut |ui| {
            grid(ui, "shell-icons", &mut |ui| {
                ui.label("Icon theme");
                ui.horizontal(|ui| {
                    let like = format!("Like the Desktop ({})", desktop());
                    let shown = if self.s.icon_theme.is_empty() { like.clone() } else { self.s.icon_theme.clone() };
                    egui::ComboBox::from_id_salt("shell-icon-theme").selected_text(shown).width(240.0).height(320.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.s.icon_theme, String::new(), like);
                        ui.separator();
                        for name in installed() {
                            ui.selectable_value(&mut self.s.icon_theme, name.clone(), name);
                        }
                        // One chosen before and since removed stays listed, so it can be seen.
                        if !self.s.icon_theme.is_empty() && !installed().contains(&self.s.icon_theme) {
                            let gone = self.s.icon_theme.clone();
                            ui.selectable_value(&mut self.s.icon_theme, gone.clone(), format!("{gone} (Not Installed)"));
                        }
                    });
                    if !self.s.icon_theme.is_empty() && ui.small_button("Reset").clicked() {
                        self.s.icon_theme.clear();
                    }
                });
                ui.end_row();
            });
            ui.add_space(12.0);
            // What it looks like (drawn a frame after a change, once the theme is read).
            let ctx = ui.ctx().clone();
            let v = ui.visuals().clone();
            let (r, _) = ui.allocate_exact_size(Vec2::new(SAMPLES.len() as f32 * 52.0 + 12.0, 56.0), Sense::hover());
            ui.painter().rect_filled(r, r_md(), v.extreme_bg_color);
            let px = (32.0 * ctx.pixels_per_point()).round() as u32;
            for (i, names) in SAMPLES.iter().enumerate() {
                let at = Rect::from_center_size(Pos2::new(r.left() + 32.0 + i as f32 * 52.0, r.center().y), Vec2::splat(32.0));
                if let Some(t) = self.icons.texture(&ctx, names, px) {
                    ui.painter().image(t.id(), at, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
                }
            }
            ui.add_space(8.0);
            hint(ui, "Used for apps in the dock, launcher, panels and tray. Missing icons come from the theme it inherits from.");
        });
    }
}
