//! Settings' Panels page: every panel, its edge, shape and screen, and the items in its three
//! zones (click an item to move it or take it off; + adds one).

use eframe::egui::{self, Vec2};
use rivet_look::theme::r_md;

use super::{SettingsApp, grid, hint, section};
use crate::config::Edge;
use crate::panels::{ITEMS, PanelCfg, ZONES, label};

/// A change to make after drawing (the list can't change while it's drawn).
enum Op {
    RemovePanel(usize),
    MovePanel(usize, i32),
    AddPanel,
    /// Panel, zone, item, by how many places.
    MoveItem(usize, usize, usize, i32),
    ToZone(usize, usize, usize, usize),
    RemoveItem(usize, usize, usize),
    AddItem(usize, usize, String),
}

fn screen_name(s: &str) -> String {
    if s.is_empty() { "Every screen".into() } else { s.to_string() }
}

impl SettingsApp {
    pub(super) fn panels_editor(&mut self, ui: &mut egui::Ui) {
        let mut ops: Vec<Op> = Vec::new();
        let n = self.s.panels.len();
        let fill = ui.visuals().faint_bg_color;
        let weak = ui.visuals().weak_text_color();
        hint(ui, "Panels go along the top or bottom of every screen, or of one. Each has three zones; click an item to move it or take it off, and + to add one. Panels on the same edge stack, the first nearest the edge.");
        ui.add_space(12.0);
        if n == 0 {
            hint(ui, "No panels.");
            ui.add_space(8.0);
        }
        for i in 0..n {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("Panel {}", i + 1)).strong().size(14.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if rivet_look::widgets::trash_button(ui).on_hover_text("Remove this panel").clicked() {
                        ops.push(Op::RemovePanel(i));
                    }
                    if rivet_look::widgets::arrow_button(ui, false, i + 1 < n).on_hover_text("Later (further from the edge)").clicked() {
                        ops.push(Op::MovePanel(i, 1));
                    }
                    if rivet_look::widgets::arrow_button(ui, true, i > 0).on_hover_text("Earlier (nearer the edge)").clicked() {
                        ops.push(Op::MovePanel(i, -1));
                    }
                });
            });
            ui.add_space(4.0);
            let screens = self.screens.clone();
            let p = &mut self.s.panels[i];
            egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 16)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                grid(ui, &format!("shell-panel-{i}"), &mut |ui| {
                    ui.label("Edge");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut p.edge, Edge::Top, "Top");
                        ui.selectable_value(&mut p.edge, Edge::Bottom, "Bottom");
                    });
                    ui.end_row();
                    ui.label("Shape");
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut p.floating, true, "Floating");
                        ui.selectable_value(&mut p.floating, false, "Edge to edge");
                    });
                    ui.end_row();
                    ui.label("Screen");
                    egui::ComboBox::from_id_salt(("shell-panel-screen", i)).selected_text(screen_name(&p.screen)).width(200.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.screen, String::new(), "Every screen");
                        let mut names = screens.clone();
                        if !p.screen.is_empty() && !names.contains(&p.screen) {
                            names.push(p.screen.clone());
                        }
                        for s in names {
                            ui.selectable_value(&mut p.screen, s.clone(), s);
                        }
                    });
                    ui.end_row();
                });
                ui.add_space(12.0);
                for (z, zone) in ZONES.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::new(64.0, 24.0), egui::Sense::hover());
                        ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, *zone, rivet_look::fonts::ui_font(13.0), weak);
                        // The items wrap under each other, not under the zone's name.
                        ui.horizontal_wrapped(|ui| {
                        let items = p.zones[z].clone();
                        for (k, id) in items.iter().enumerate() {
                            ui.menu_button(label(id), |ui| {
                                if k > 0 && ui.button("Move Left").clicked() {
                                    ops.push(Op::MoveItem(i, z, k, -1));
                                }
                                if k + 1 < items.len() && ui.button("Move Right").clicked() {
                                    ops.push(Op::MoveItem(i, z, k, 1));
                                }
                                for (to, name) in ZONES.iter().enumerate() {
                                    if to != z && ui.button(format!("To the {} Zone", name)).clicked() {
                                        ops.push(Op::ToZone(i, z, k, to));
                                    }
                                }
                                ui.separator();
                                if ui.button("Take Off the Panel").clicked() {
                                    ops.push(Op::RemoveItem(i, z, k));
                                }
                            })
                            .response
                            .on_hover_text(ITEMS.iter().find(|x| x.0 == id).map(|x| x.2).unwrap_or("Not part of the shell yet"));
                        }
                        let missing: Vec<&(&str, &str, &str)> = ITEMS.iter().filter(|x| !p.has(x.0)).collect();
                        ui.add_enabled_ui(!missing.is_empty(), |ui| {
                            ui.menu_button("+", |ui| {
                                for (id, name, what) in missing {
                                    if ui.button(*name).on_hover_text(*what).clicked() {
                                        ops.push(Op::AddItem(i, z, id.to_string()));
                                    }
                                }
                            })
                            .response
                            .on_hover_text(format!("Add to the {} zone", zone.to_lowercase()));
                        });
                        });
                    });
                    ui.add_space(2.0);
                }
            });
            ui.add_space(18.0);
        }
        if ui.button("Add a Panel").clicked() {
            ops.push(Op::AddPanel);
        }
        ui.add_space(22.0);
        section(ui, "Clock", &mut |ui| {
            grid(ui, "shell-clock-grid", &mut |ui| {
                ui.label("Shows");
                egui::ComboBox::from_id_salt("shell-clock").selected_text(chrono::Local::now().format(&self.s.clock_format).to_string()).width(220.0).show_ui(ui, |ui| {
                    for f in ["%H:%M", "%a %b %-d  %H:%M", "%A %-d %B  %H:%M", "%-I:%M %p", "%a %b %-d  %-I:%M %p", "%Y-%m-%d  %H:%M"] {
                        let shown = chrono::Local::now().format(f).to_string();
                        ui.selectable_value(&mut self.s.clock_format, f.to_string(), shown);
                    }
                });
                ui.end_row();
            });
        });
        for op in ops {
            apply(&mut self.s.panels, op);
        }
    }
}

fn apply(panels: &mut Vec<PanelCfg>, op: Op) {
    match op {
        Op::RemovePanel(i) => {
            panels.remove(i);
        }
        Op::MovePanel(i, by) => {
            let j = (i as i32 + by).clamp(0, panels.len() as i32 - 1) as usize;
            panels.swap(i, j);
        }
        Op::AddPanel => {
            // The first one on top; then along the bottom.
            panels.push(if panels.is_empty() { PanelCfg::default() } else { PanelCfg::new_bottom() });
        }
        Op::MoveItem(p, z, k, by) => {
            let zone = &mut panels[p].zones[z];
            let j = (k as i32 + by).clamp(0, zone.len() as i32 - 1) as usize;
            zone.swap(k, j);
        }
        Op::ToZone(p, z, k, to) => {
            let id = panels[p].zones[z].remove(k);
            // Into the end of the zone nearest where it came from.
            if to > z {
                panels[p].zones[to].insert(0, id);
            } else {
                panels[p].zones[to].push(id);
            }
        }
        Op::RemoveItem(p, z, k) => {
            panels[p].zones[z].remove(k);
        }
        Op::AddItem(p, z, id) => panels[p].zones[z].push(id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits() {
        let mut ps = vec![PanelCfg::default()];
        apply(&mut ps, Op::AddPanel);
        assert_eq!(ps[1].edge, Edge::Bottom);
        apply(&mut ps, Op::ToZone(0, 1, 0, 2));
        assert_eq!(ps[0].zones[2][0], "clock");
        apply(&mut ps, Op::ToZone(0, 2, 0, 0));
        assert_eq!(ps[0].zones[0].last().map(String::as_str), Some("clock"));
        apply(&mut ps, Op::AddItem(1, 0, "system".into()));
        apply(&mut ps, Op::MovePanel(1, -1));
        assert!(ps[0].has("system"));
        apply(&mut ps, Op::RemovePanel(0));
        assert_eq!(ps.len(), 1);
    }
}
