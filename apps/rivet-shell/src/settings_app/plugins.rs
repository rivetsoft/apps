//! The Plugins page: the plugins installed (turn one on, put its item on a panel), and hooks:
//! the person's own commands for the shell's events.

use eframe::egui::{self, Align2, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm};

use super::{SettingsApp, grid, hint, section};
use crate::plugins::{EVENTS, SettingKind, found_cached, user_dir};

impl SettingsApp {
    pub(super) fn plugins_page(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let accent = v.selection.bg_fill;
        let found = found_cached();
        // A folder chosen with the picker, for a plugin's setting.
        if let Some((id, key, rx)) = &self.plugin_pick {
            match rx.try_recv() {
                Ok(answer) => {
                    match answer {
                        Ok(Some(p)) => self.s.set_plugin(&id.clone(), &key.clone(), &p.to_string_lossy()),
                        Ok(None) => {}
                        Err(e) => self.note = Some(format!("Couldn't open the file picker: {e}")),
                    }
                    self.plugin_pick = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => self.plugin_pick = None,
            }
        }
        let can_pick = self.plugin_pick.is_none() && self.shot.is_none();
        let mut ask: Option<(String, String, String)> = None;
        section(ui, "Installed", &mut |ui| {
            let s = &mut self.s;
            if found.is_empty() {
                hint(ui, "No plugins yet. Put one in your plugins folder and it shows here.");
            }
            for m in &found {
                let mut on = s.plugins.contains(&m.id);
                let item = format!("plugin:{}", m.id);
                egui::Frame::NONE.fill(v.panel_fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if ui.checkbox(&mut on, "").changed() {
                            s.plugins.retain(|p| p != &m.id);
                            if on {
                                s.plugins.push(m.id.clone());
                                s.plugins.sort();
                            } else {
                                // Off: its item comes off the panels too.
                                for p in &mut s.panels {
                                    for z in &mut p.zones {
                                        z.retain(|i| i != &item);
                                    }
                                    p.tidy_groups();
                                }
                            }
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&m.name).strong());
                                // Where it's from: the shell's own, or the person's folder.
                                let (text, col) = if m.official { ("Official", accent) } else { ("Third-party", v.weak_text_color()) };
                                let g = ui.painter().layout_no_wrap(text.to_string(), rivet_look::fonts::ui_font(11.0), col);
                                let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(14.0, 6.0), Sense::hover());
                                ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, mix(v.panel_fill, col, 0.6)), egui::StrokeKind::Inside);
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, text, rivet_look::fonts::ui_font(11.0), col);
                            });
                            if !m.description.is_empty() {
                                ui.label(egui::RichText::new(&m.description).size(12.5));
                            }
                            let mut by = Vec::new();
                            if !m.author.is_empty() {
                                by.push(format!("by {}", m.author));
                            }
                            if !m.version.is_empty() {
                                by.push(format!("version {}", m.version));
                            }
                            hint(ui, &by.join(" · "));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !on {
                                return;
                            }
                            if s.panels.iter().any(|p| p.has(&item)) {
                                ui.label(egui::RichText::new("On a panel").color(v.weak_text_color()).size(12.0));
                            } else if !s.panels.is_empty() && ui.button("Add to Panel").on_hover_text("Puts its item on the first panel. Move it on the Panels page.").clicked() {
                                s.panels[0].zones[2].insert(0, item.clone());
                            }
                        });
                    });
                    // Its own settings, while it's on.
                    if on && !m.settings.is_empty() {
                        ui.add_space(8.0);
                        ui.indent(("shell-plugin-set", &m.id), |ui| {
                            grid(ui, &format!("shell-plugin-set-{}", m.id), &mut |ui| {
                                for set in &m.settings {
                                    let mut value = s.plugin_value(&m.id, &set.key).to_string();
                                    let before = value.clone();
                                    match set.kind {
                                        SettingKind::Toggle => {
                                            ui.label("");
                                            let mut yes = value == "true";
                                            ui.checkbox(&mut yes, &set.label).on_hover_text(&set.hint);
                                            value = if yes { "true".into() } else { String::new() };
                                        }
                                        SettingKind::Text => {
                                            ui.label(&set.label);
                                            ui.add(egui::TextEdit::singleline(&mut value).desired_width(300.0).hint_text(&set.hint));
                                        }
                                        SettingKind::Folder => {
                                            ui.label(&set.label);
                                            ui.horizontal(|ui| {
                                                ui.add(egui::TextEdit::singleline(&mut value).desired_width(300.0).hint_text(&set.hint));
                                                if ui.add_enabled(can_pick, egui::Button::new("Choose…")).clicked() {
                                                    ask = Some((m.id.clone(), set.key.clone(), set.label.clone()));
                                                }
                                            });
                                        }
                                    }
                                    if value != before {
                                        s.set_plugin(&m.id, &set.key, value.trim());
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Open Plugins Folder").clicked() {
                    let dir = user_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    crate::sys::spawn("xdg-open", &[&dir.display().to_string()]);
                }
            });
            hint(ui, "A plugin can do anything you can, so turn on only ones you trust.");
        });
        if let Some((id, key, title)) = ask {
            let start = Some(self.s.plugin_value(&id, &key)).filter(|v| !v.is_empty()).map(crate::wall::expand).or_else(|| Some(rivet_xdg::places::home()));
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(crate::portal::choose(crate::portal::Pick::Folder, &title, start.as_deref()));
            });
            self.plugin_pick = Some((id, key, rx));
        }
        section(ui, "Hooks", &mut |ui| {
            hint(ui, "Run a command of your own when one of these happens.");
            ui.add_space(8.0);
            let hooks = &mut self.s.hooks;
            grid(ui, "shell-hooks", &mut |ui| {
                for (name, what, with) in EVENTS {
                    let label = ui.label(*what);
                    // What the command can read: each detail is a RIVET_ variable, then its values if they're fixed.
                    if !with.is_empty() {
                        let (keys, values) = with.split_once(": ").unwrap_or((with, ""));
                        let vars: Vec<String> = keys.split(", ").map(|k| format!("RIVET_{}", k.to_uppercase())).collect();
                        let values = if values.is_empty() { String::new() } else { format!(" ({values})") };
                        label.on_hover_text(format!("Your command can read {}{values}", vars.join(", ")));
                    }
                    let mut cmd = hooks.iter().find(|(e, _)| e == name).map(|(_, c)| c.clone()).unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut cmd).desired_width(300.0).font(egui::TextStyle::Monospace).hint_text("a command")).changed() {
                        hooks.retain(|(e, _)| e != name);
                        if !cmd.trim().is_empty() {
                            hooks.push((name.to_string(), cmd));
                            hooks.sort();
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
}
