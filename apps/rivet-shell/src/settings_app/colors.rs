//! Appearance's Colors card: one choice of where the colours come from (the wallpaper, a
//! built-in palette, or a style and accent of the shell's own) and that choice's settings.

use eframe::egui::{self, Color32, Sense, Stroke, Vec2};
use rivet_look::palette;
use rivet_look::theme::{ACCENTS, Mode, r_lg, r_md};

use super::panel_editor::segmented;
use super::{SettingsApp, grid, hint, section};
use crate::config::Settings;
use crate::style::Roles;

/// Where the colours come from.
#[derive(Clone, Copy, PartialEq)]
enum Source {
    Wallpaper,
    Palette,
    Accent,
}

impl Source {
    fn of(s: &Settings) -> Source {
        if !s.palette.is_empty() {
            Source::Palette
        } else if s.follow_desktop_colors {
            Source::Wallpaper
        } else {
            Source::Accent
        }
    }

    fn set(self, s: &mut Settings, dark: bool) {
        match self {
            Source::Wallpaper => {
                s.palette.clear();
                s.follow_desktop_colors = true;
            }
            // Starting from one like what's showing.
            Source::Palette => s.choose_palette(if dark { "One Dark" } else { "One Light" }),
            Source::Accent => {
                s.palette.clear();
                s.follow_desktop_colors = false;
            }
        }
    }
}

/// Is matugen there to run? (Looked for once.)
fn has_matugen() -> bool {
    static HAS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *HAS.get_or_init(|| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join("matugen").is_file())))
}

impl SettingsApp {
    pub(super) fn colors_section(&mut self, ui: &mut egui::Ui) {
        let dark = ui.visuals().dark_mode;
        section(ui, "Colors", &mut |ui| {
            let was = Source::of(&self.s);
            let mut src = was;
            segmented(ui, &mut src, &[(Source::Wallpaper, "From the Wallpaper"), (Source::Palette, "A Palette"), (Source::Accent, "Style and Accent")]);
            if src != was {
                src.set(&mut self.s, dark);
            }
            ui.add_space(6.0);
            hint(ui, match src {
                Source::Wallpaper => "Colors come from the wallpaper, in the shell and every app that reads the desktop's colors.",
                Source::Palette => "A ready-made color set for the shell and every Rivet app.",
                Source::Accent => "A light or dark style with an accent and a surface color, for the shell and every Rivet app.",
            });
            ui.add_space(12.0);
            match src {
                Source::Wallpaper => self.wallpaper_colors(ui),
                Source::Palette => self.palette_tiles(ui),
                Source::Accent => self.own_colors(ui),
            }
        });
    }

    /// The colours as they're used, at the top of the page: the theme's four surfaces, each a
    /// step above the last, with text on every one and the three colours as buttons on the top one.
    pub(super) fn colors_preview(&self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let colors = Roles::new(&self.look, &self.s);
        super::controls::preview_head(ui);
        let w = ui.available_width();
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, 196.0), Sense::hover());
        super::controls::preview_edge(ui, r);
        let p = ui.painter_at(r);
        let (text, weak) = (v.text_color(), v.weak_text_color());
        let font = |size: f32| rivet_look::fonts::ui_font(size);
        // Each surface starts a step in from the one under it, which leaves that one a strip
        // down its left for its name and text.
        let step = (w * 0.165).max(96.0);
        let levels = [("Base", v.extreme_bg_color), ("Card", v.faint_bg_color), ("Panel", v.panel_fill), ("Pop-up", v.window_fill)];
        let mut top = r;
        for (i, (name, fill)) in levels.into_iter().enumerate() {
            let n = i as f32;
            let at = egui::Rect::from_min_max(r.min + Vec2::new(step * n, 13.0 * n), r.max - Vec2::splat(13.0 * n));
            if i == 3 {
                p.add(v.popup_shadow.as_shape(at, r_md()));
            }
            p.rect_filled(at, if i == 0 { r_lg() } else { r_md() }, fill);
            p.rect_stroke(at, if i == 0 { r_lg() } else { r_md() }, Stroke::new(1.0, palette::mix(fill, text, 0.1)), egui::StrokeKind::Inside);
            let from = at.min + Vec2::new(14.0, 12.0);
            p.text(from, egui::Align2::LEFT_TOP, name.to_uppercase(), font(10.5), weak);
            if i < 3 {
                p.text(from + Vec2::new(0.0, 22.0), egui::Align2::LEFT_TOP, "Text", font(13.5), text);
                p.text(from + Vec2::new(0.0, 42.0), egui::Align2::LEFT_TOP, "Muted", font(12.0), weak);
            }
            top = at;
        }
        // On the pop-up: a button in each colour, then an outlined one and a switch that's on.
        let inner = top.shrink2(Vec2::new(14.0, 0.0));
        let gap = 8.0;
        let bw = ((inner.width() - gap * 2.0) / 3.0).min(112.0);
        let y = top.top() + 36.0;
        for (i, (name, fill)) in [("Primary", colors.primary), ("Secondary", colors.secondary), ("Tertiary", colors.tertiary)].into_iter().enumerate() {
            let b = egui::Rect::from_min_size(egui::pos2(inner.left() + (bw + gap) * i as f32, y), Vec2::new(bw, 28.0));
            p.rect_filled(b, 14.0, fill);
            p.text(b.center(), egui::Align2::CENTER_CENTER, name, font(12.0), crate::ui::on(fill));
        }
        let y = y + 28.0 + 12.0;
        let b = egui::Rect::from_min_size(egui::pos2(inner.left(), y), Vec2::new(bw, 28.0));
        p.rect_filled(b, 14.0, palette::mix(v.window_fill, text, 0.07));
        p.rect_stroke(b, 14.0, Stroke::new(1.0, palette::mix(v.window_fill, text, 0.2)), egui::StrokeKind::Inside);
        p.text(b.center(), egui::Align2::CENTER_CENTER, "Button", font(12.0), text);
        let track = egui::Rect::from_min_size(egui::pos2(b.right() + gap + 4.0, b.center().y - 9.0), Vec2::new(32.0, 18.0));
        let on = colors.tone(crate::style::Slot::Toggle, self.s.color_use);
        p.rect_filled(track, 9.0, on);
        p.circle_filled(egui::pos2(track.right() - 9.0, track.center().y), 6.5, crate::ui::on(on));
        p.text(egui::pos2(track.right() + 10.0, b.center().y), egui::Align2::LEFT_CENTER, "Text", font(13.5), text);
        ui.add_space(24.0);
    }

    /// The colours made from the wallpaper: by matugen, run here, or by whatever else writes them.
    fn wallpaper_colors(&mut self, ui: &mut egui::Ui) {
        super::controls::switch(ui, &mut self.s.matugen, "Make colors with matugen when the wallpaper changes");
        if !self.s.matugen {
            hint(ui, "The shell follows the colors another theme tool sets for the desktop.");
            return;
        }
        if !has_matugen() {
            let warn = rivet_look::theme::status(ui.ctx()).warning;
            ui.label(egui::RichText::new("matugen isn't installed, so no colors are made. Pick a palette to use colors without it.").small().color(warn));
        }
        ui.add_space(10.0);
        grid(ui, "shell-matugen", &mut |ui| {
            ui.label("Style");
            segmented(ui, &mut self.s.color_mode, &[(Mode::Dark, "Dark"), (Mode::Light, "Light")]);
            ui.end_row();
            ui.label("Scheme");
            let shown = scheme_name(self.s.scheme_type.trim_start_matches("scheme-"));
            egui::ComboBox::from_id_salt("shell-scheme-type").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                for t in ["tonal-spot", "content", "expressive", "fidelity", "fruit-salad", "monochrome", "neutral", "rainbow", "vibrant"] {
                    ui.selectable_value(&mut self.s.scheme_type, format!("scheme-{t}"), scheme_name(t));
                }
            });
            ui.end_row();
            ui.label("Contrast");
            ui.horizontal(|ui| {
                ui.add(egui::Slider::new(&mut self.s.matugen_contrast, -1.0..=1.0).step_by(0.05).fixed_decimals(2)).on_hover_text("Lower for softer colors, higher for stronger ones. 0 keeps the scheme as designed.");
                if self.s.matugen_contrast != 0.0 && super::controls::reset(ui).clicked() {
                    self.s.matugen_contrast = 0.0;
                }
            });
            ui.end_row();
            // With pictures per screen: whose picture the colors come from.
            if !self.s.wallpaper_screens.is_empty() || self.screens.len() > 1 {
                ui.label("Made from");
                let shown = if self.s.matugen_screen.is_empty() { "The Shared Picture".to_string() } else { format!("{}'s Picture", self.s.matugen_screen) };
                egui::ComboBox::from_id_salt("shell-matugen-screen").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.s.matugen_screen, String::new(), "The Shared Picture");
                    for name in self.wall_screens() {
                        let own = self.s.has_own_wallpaper(&name);
                        ui.selectable_value(&mut self.s.matugen_screen, name.clone(), if own { format!("{name}'s Picture") } else { format!("{name} (Shows the Shared Picture)") });
                    }
                });
                ui.end_row();
            }
            ui.label("Rivet apps");
            if super::controls::button(ui, super::controls::Icon::Plus, "Add Rivet's Colors to matugen").on_hover_text("Rivet apps then change color with the wallpaper, like the rest of the desktop").clicked() {
                self.note = Some(match palette::install_matugen_template() {
                    Ok(m) => m,
                    Err(e) => format!("Couldn't update matugen's config: {e}"),
                });
            }
            ui.end_row();
        });
        if let Some(n) = &self.note {
            ui.add_space(4.0);
            hint(ui, n);
        }
    }

    /// The shell's own style and accent.
    fn own_colors(&mut self, ui: &mut egui::Ui) {
        grid(ui, "shell-look", &mut |ui| {
            ui.label("Style");
            segmented(ui, &mut self.s.theme, &[(Mode::System, "Follow System"), (Mode::Light, "Light"), (Mode::Dark, "Dark")]);
            ui.end_row();
            ui.label("Accent");
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (c, name) in ACCENTS {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                    ui.painter().circle_filled(r.center(), 9.0, Color32::from_rgb(c[0], c[1], c[2]));
                    if self.s.accent == c {
                        ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0, ui.visuals().strong_text_color()));
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(name).clicked() {
                        self.s.accent = c;
                    }
                }
            });
            ui.end_row();
            // What the shell would make from the accent, for the "From the Accent" swatch.
            let made = Roles::new(&self.look, &Settings { secondary: None, tertiary: None, ..self.s.clone() });
            ui.label("Secondary");
            let picked = role_pick(ui, "secondary", &mut self.s.secondary, made.secondary);
            ui.end_row();
            ui.label("Tertiary");
            let picked = role_pick(ui, "tertiary", &mut self.s.tertiary, made.tertiary) || picked;
            ui.end_row();
            // A color chosen should show: with Accent Only, nothing would use it.
            if picked && self.s.color_use == 0 {
                self.s.color_use = 1;
            }
            ui.label("Surface");
            self.surfaces(ui);
            ui.end_row();
            ui.label("");
            super::controls::switch(ui, &mut self.s.surface_tint, "Tint the surface with the accent");
            ui.end_row();
        });
    }

    /// The surface colour, for the style showing (dark and light keep one each): a scale from
    /// the most contrast to the least, with the usual grey in its place, or any colour.
    fn surfaces(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let dark = v.dark_mode;
        let (slot, choices) = if dark { (&mut self.s.surface_dark, &SURFACES_DARK[..]) } else { (&mut self.s.surface_light, &SURFACES_LIGHT[..]) };
        let [r, g, b] = Settings::SURFACES[if dark { 0 } else { 1 }];
        let usual = Color32::from_rgb(r, g, b);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let ring = Stroke::new(2.0, v.strong_text_color());
            let rim = Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.35));
            for (c, name) in choices.iter() {
                // None in the list: the usual grey.
                let col = c.map(|c| Color32::from_rgb(c[0], c[1], c[2])).unwrap_or(usual);
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                ui.painter().circle_filled(r.center(), 9.0, col);
                ui.painter().circle_stroke(r.center(), 9.0, rim);
                if *slot == *c {
                    ui.painter().circle_stroke(r.center(), 11.0, ring);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(*name).clicked() {
                    *slot = *c;
                }
            }
            // Any other colour.
            ui.add_space(4.0);
            let mut c = slot.unwrap_or([usual.r(), usual.g(), usual.b()]);
            if ui.color_edit_button_srgb(&mut c).on_hover_text("Any color").changed() {
                *slot = Some(c);
            }
        });
    }
}

/// A secondary or tertiary colour: made from the accent (None, the first swatch, showing what
/// that makes), one of the accents, or any colour. True when one was chosen.
fn role_pick(ui: &mut egui::Ui, salt: &str, slot: &mut Option<[u8; 3]>, made: Color32) -> bool {
    let v = ui.visuals().clone();
    let ring = Stroke::new(2.0, v.strong_text_color());
    let mut chosen = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        // From the accent: half the dot in the colour it makes, so it reads as "automatic".
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
        ui.painter().circle_stroke(r.center(), 8.5, Stroke::new(1.5, made));
        ui.painter().circle_filled(r.center(), 5.0, made);
        if slot.is_none() {
            ui.painter().circle_stroke(r.center(), 11.0, ring);
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("From the accent").clicked() {
            *slot = None;
        }
        ui.add_space(4.0);
        for (c, name) in ACCENTS {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            ui.painter().circle_filled(r.center(), 9.0, Color32::from_rgb(c[0], c[1], c[2]));
            if *slot == Some(c) {
                ui.painter().circle_stroke(r.center(), 11.0, ring);
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(name).clicked() {
                *slot = Some(c);
                chosen = true;
            }
        }
        ui.add_space(4.0);
        let mut c = slot.unwrap_or([made.r(), made.g(), made.b()]);
        if ui.push_id(salt, |ui| ui.color_edit_button_srgb(&mut c).on_hover_text("Any color")).inner.changed() {
            *slot = Some(c);
            chosen = true;
        }
    });
    chosen
}

/// Surface colours, most contrast first (None: the usual grey), for dark and for light.
/// A matugen scheme as a choice shows it: "tonal-spot" as "Tonal Spot".
fn scheme_name(t: &str) -> String {
    t.split('-').map(|w| { let mut c = w.chars(); c.next().map(|f| f.to_uppercase().chain(c).collect::<String>()).unwrap_or_default() }).collect::<Vec<_>>().join(" ")
}

const SURFACES_DARK: [(Option<[u8; 3]>, &str); 6] = [(Some([0x00, 0x00, 0x00]), "Black"), (Some([0x0c, 0x0c, 0x0c]), "Near Black"), (Some([0x17, 0x17, 0x17]), "Darker"), (Some([0x22, 0x22, 0x22]), "Dark"), (None, "Default"), (Some([0x3e, 0x3e, 0x3e]), "Soft")];
const SURFACES_LIGHT: [(Option<[u8; 3]>, &str); 5] = [(Some([0xff, 0xff, 0xff]), "White"), (Some([0xf5, 0xf5, 0xf6]), "Bright"), (None, "Default"), (Some([0xdd, 0xdd, 0xe0]), "Dim"), (Some([0xce, 0xce, 0xd2]), "Grey")];
