//! Preferences: the look (desktop colors, style, accent, text size, icons), the editor (code
//! font and size, tab width, wrapping, line numbers), the window, and About.

use eframe::egui::{self, Color32, Sense, Stroke, Vec2};
use rivet_look::theme::{ACCENTS, Mode, r_md, r_sm, r_xs};
use rivet_look::{fonts, windows};

use super::*;

impl EditApp {
    pub(crate) fn prefs_window(&mut self, ctx: &egui::Context) {
        let chrome = windows::Chrome {
            title: "Preferences".into(),
            icon: self.icon(ctx, &["preferences-system-symbolic", "emblem-system-symbolic"], 16.0),
            close: self.icon(ctx, &["window-close-symbolic"], 14.0),
            flush: false,
        };
        let area = ctx.content_rect().shrink(6.0);
        let app_icon = self.app_icon(ctx);
        let pal = self.palette();
        let fams = self.families.get_or_insert_with(fonts::mono_families).clone();
        let system_mono = fonts::system_mono_name();
        let before = self.s.clone();
        let mut win = std::mem::replace(&mut self.prefs_win, windows::WinState::new(1, Vec2::ZERO));
        let shown = windows::show(ctx, &mut win, &chrome, area, Vec2::new(460.0, 360.0), |ui| {
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            egui::ScrollArea::vertical().id_salt("edit-prefs").auto_shrink([false, false]).show(ui, |ui| {
                egui::Frame::NONE.inner_margin(egui::Margin { left: 22, right: 22, top: 14, bottom: 20 }).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.spacing_mut().item_spacing = Vec2::new(10.0, 8.0);
                    let card = v.faint_bg_color;
                    let section = |ui: &mut egui::Ui, title: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                        ui.label(egui::RichText::new(title).strong().size(14.5));
                        ui.add_space(4.0);
                        egui::Frame::NONE.fill(card).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            body(ui);
                        });
                        ui.add_space(18.0);
                    };
                    let grid = |ui: &mut egui::Ui, id: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                        egui::Grid::new(id).num_columns(2).spacing([24.0, 14.0]).min_col_width(110.0).show(ui, |ui| body(ui));
                    };
                    section(ui, "Look", &mut |ui| {
                        ui.checkbox(&mut self.s.follow_desktop_colors, "Follow desktop colors");
                        let manual = !self.s.follow_desktop_colors || pal.is_none();
                        ui.add_space(6.0);
                        grid(ui, "edit-look", &mut |ui| {
                            ui.label("Style");
                            ui.add_enabled_ui(manual, |ui| {
                                ui.horizontal(|ui| {
                                    ui.selectable_value(&mut self.s.theme, Mode::System, "Follow system");
                                    ui.selectable_value(&mut self.s.theme, Mode::Light, "Light");
                                    ui.selectable_value(&mut self.s.theme, Mode::Dark, "Dark");
                                });
                            });
                            ui.end_row();
                            ui.label("Accent");
                            ui.add_enabled_ui(manual, |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    for (c, name) in ACCENTS {
                                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                                        let col = Color32::from_rgb(c[0], c[1], c[2]);
                                        ui.painter().circle_filled(r.center(), 9.0, if manual { col } else { col.gamma_multiply(0.4) });
                                        if self.s.accent == c {
                                            ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0, ui.visuals().strong_text_color()));
                                        }
                                        if resp.on_hover_text(name).clicked() && manual {
                                            self.s.accent = c;
                                        }
                                    }
                                });
                            });
                            ui.end_row();
                            ui.label("Text size");
                            ui.horizontal(|ui| {
                                if ui.selectable_label(self.s.text_size == 0.0, "Like the desktop").clicked() {
                                    self.s.text_size = 0.0;
                                }
                                for (pt, label) in [(12.5, "Small"), (13.5, "Medium"), (15.0, "Large")] {
                                    if ui.selectable_label(self.s.text_size == pt, label).clicked() {
                                        self.s.text_size = pt;
                                    }
                                }
                            });
                            ui.end_row();
                            ui.label("Icons");
                            ui.horizontal(|ui| {
                                ui.selectable_value(&mut self.s.builtin_icons, true, "Rivet's own");
                                ui.selectable_value(&mut self.s.builtin_icons, false, "Desktop icon theme");
                            });
                            ui.end_row();
                        });
                    });
                    section(ui, "Editor", &mut |ui| {
                        grid(ui, "edit-editor", &mut |ui| {
                            ui.label("Code font");
                            let shown = if self.s.code_font.is_empty() { format!("System ({system_mono})") } else { self.s.code_font.clone() };
                            egui::ComboBox::from_id_salt("code-font").selected_text(shown).width(240.0).show_ui(ui, |ui| {
                                ui.selectable_value(&mut self.s.code_font, String::new(), format!("System ({system_mono})"));
                                for f in &fams {
                                    ui.selectable_value(&mut self.s.code_font, f.clone(), f.as_str());
                                }
                            });
                            ui.end_row();
                            ui.label("Size");
                            ui.add(egui::Slider::new(&mut self.s.code_size, 8.0..=32.0).step_by(1.0).suffix(" pt"));
                            ui.end_row();
                            ui.label("Tab key");
                            ui.horizontal(|ui| {
                                for n in [2usize, 4, 8] {
                                    ui.selectable_value(&mut self.s.tab_width, n, format!("{n} spaces"));
                                }
                            });
                            ui.end_row();
                        });
                        ui.add_space(8.0);
                        ui.checkbox(&mut self.s.word_wrap, "Wrap long lines (Alt+Z)");
                        ui.checkbox(&mut self.s.line_numbers, "Line numbers");
                    });
                    section(ui, "Window", &mut |ui| {
                        ui.checkbox(&mut self.s.show_menubar, "Menu bar (Ctrl+M)");
                        ui.checkbox(&mut self.s.show_sidebar, "Sidebar (F9)");
                        ui.checkbox(&mut self.s.show_statusbar, "Status bar");
                    });
                    // About.
                    ui.horizontal(|ui| {
                        if let Some(t) = &app_icon {
                            ui.add(egui::Image::from_texture(t).fit_to_exact_size(Vec2::splat(44.0)));
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.label(egui::RichText::new(format!("Rivet Edit {}", env!("CARGO_PKG_VERSION"))).strong());
                            ui.label(egui::RichText::new("A lightweight code editor, part of Rivet Apps. Tree-sitter colors for Rust, Python, JavaScript, TypeScript, JSON, shell, C, Go, HTML, CSS, TOML and YAML.").small().color(weak));
                            ui.hyperlink_to("Source code", "https://github.com/ulinaaron/rivet-apps");
                        });
                    });
                    let _ = (r_sm(), r_xs());
                });
            });
        });
        self.prefs_win = win;
        if !shown.open {
            self.prefs_open = false;
        }
        if self.s != before {
            self.s.save();
        }
    }
}
