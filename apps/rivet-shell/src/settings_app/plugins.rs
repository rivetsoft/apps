//! The Plugins page: the plugins installed (turn one on, put its item on a panel), and hooks:
//! the person's own commands for the shell's events.

use eframe::egui::{self, Align2, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm};

use super::{SettingsApp, grid, hint, section};
use crate::plugins::{EVENTS, found_cached, user_dir};

impl SettingsApp {
    pub(super) fn plugins_page(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let accent = v.selection.bg_fill;
        let found = found_cached();
        section(ui, "Installed", &mut |ui| {
            let s = &mut self.s;
            if found.is_empty() {
                hint(ui, "No plugins yet. A plugin is a folder with a plugin.conf and a program; put one in your plugins folder and it shows here.");
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
                            by.push(if m.stream { "keeps running".to_string() } else { format!("runs every {} s", m.interval) });
                            if !m.events.is_empty() {
                                by.push(format!("hears of: {}", m.events.join(", ")));
                            }
                            hint(ui, &by.join(" · "));
                            hint(ui, &m.dir.display().to_string());
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if !on {
                                return;
                            }
                            if s.panels.iter().any(|p| p.has(&item)) {
                                ui.label(egui::RichText::new("On a panel").color(v.weak_text_color()).size(12.0));
                            } else if !s.panels.is_empty() && ui.button("Add to Panel").on_hover_text("Puts its item on the first panel; move it on the Panels page").clicked() {
                                s.panels[0].zones[2].insert(0, item.clone());
                            }
                        });
                    });
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Open Plugins Folder").on_hover_text(user_dir().display().to_string()).clicked() {
                    let dir = user_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    crate::sys::spawn("xdg-open", &[&dir.display().to_string()]);
                }
            });
            hint(ui, "A plugin is a program that runs as you: turn on only ones you trust. Third-party plugins live in your plugins folder; official ones come with the shell.");
        });
        section(ui, "Hooks", &mut |ui| {
            hint(ui, "Your own commands, run when something happens. Each is a shell line; what comes with the event is in its environment (RIVET_EVENT, and the names shown on hover as RIVET_PATH, RIVET_STAGE…).");
            ui.add_space(8.0);
            let hooks = &mut self.s.hooks;
            grid(ui, "shell-hooks", &mut |ui| {
                for (name, what, with) in EVENTS {
                    let tip = if with.is_empty() { format!("hook.{name}") } else { format!("hook.{name} · comes with {with}") };
                    ui.label(*what).on_hover_text(tip);
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
        section(ui, "Writing One", &mut |ui| {
            hint(ui, "A folder with a plugin.conf (name, exec, mode, interval, events) and a program in any language. The program prints what its panel item shows (plain text, or JSON with text, icon, tooltip, state and a menu) and is told of clicks and the events above. The Disk Space plugin that comes with the shell is a short example to copy; PLUGINS.md, beside the shell's README, has the whole of it.");
        });
    }
}
