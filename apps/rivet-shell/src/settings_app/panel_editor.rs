//! Settings' Panels page, laid out like the panel it edits: a tab for each panel, a live
//! preview of the chosen one, its place (edge, style, screen), and its three zones side by side
//! as columns of items that are dragged to reorder them or move them between zones; "Add Item"
//! offers the items not on the panel yet.

use eframe::egui::{self, Align2, Color32, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md, r_sm, selected_tile};

use super::{SettingsApp, hint};
use crate::config::Edge;
use crate::glyph;
use crate::config::WsStyle;
use crate::panels::{ITEMS, PanelCfg, ZONES, label};

/// A change to make after drawing (the lists can't change while they're drawn).
enum Op {
    Select(usize),
    AddPanel,
    Duplicate(usize),
    RemovePanel(usize),
    MovePanel(usize, i32),
    /// Panel; from zone and place; to zone and place (before the item now there).
    MoveItem(usize, (usize, usize), (usize, usize)),
    RemoveItem(usize, usize, usize),
    AddItem(usize, usize, String),
}

/// What's held while an item is dragged: its zone and place.
#[derive(Clone, Copy, PartialEq)]
struct Held(usize, usize);

const ROW: f32 = 38.0;

fn ui_font(size: f32) -> egui::FontId {
    rivet_look::fonts::ui_font(size)
}

fn place(p: &PanelCfg) -> String {
    let edge = if p.edge == Edge::Top { "Top" } else { "Bottom" };
    let screen = if p.screen.is_empty() { "every screen".to_string() } else { p.screen.clone() };
    format!("{edge} · {screen}")
}

/// A row of segmented choices (like the Edge and Style switches).
fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, choices: &[(T, &str)]) {
    let v = ui.visuals().clone();
    let accent = v.selection.bg_fill;
    let h = 30.0;
    let widths: Vec<f32> = choices.iter().map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), ui_font(13.0), v.text_color()).size().x + 28.0).collect();
    let (r, _) = ui.allocate_exact_size(Vec2::new(widths.iter().sum::<f32>() + 4.0, h), Sense::hover());
    ui.painter().rect_filled(r, r_md(), mix(v.extreme_bg_color, v.text_color(), 0.06));
    let mut x = r.left() + 2.0;
    for ((val, text), w) in choices.iter().zip(widths) {
        let cell = Rect::from_min_size(Pos2::new(x, r.top() + 2.0), Vec2::new(w, h - 4.0));
        x += w;
        let resp = ui.interact(cell, ui.scope_id().with(("seg", text)), Sense::click());
        let on = *value == *val;
        if on {
            ui.painter().rect_filled(cell, r_sm(), accent);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, r_sm(), mix(v.extreme_bg_color, v.text_color(), 0.12));
        }
        let col = if on { v.selection.stroke.color } else { v.text_color() };
        ui.painter().text(cell.center(), Align2::CENTER_CENTER, *text, ui_font(13.0), col);
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            *value = *val;
        }
    }
}

impl SettingsApp {
    pub(super) fn panels_editor(&mut self, ui: &mut egui::Ui) {
        let mut ops: Vec<Op> = Vec::new();
        let v = ui.visuals().clone();
        let accent = v.selection.bg_fill;
        let n = self.s.panels.len();
        self.panel_sel = self.panel_sel.min(n.saturating_sub(1));
        let sel = self.panel_sel;
        // ---- a tab for each panel, and a new one
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
            for (i, p) in self.s.panels.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(168.0, 48.0), Sense::click());
                if i == sel {
                    let (fill, stroke) = selected_tile(&v, v.panel_fill);
                    ui.painter().rect_filled(r, r_md(), fill);
                    ui.painter().rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
                } else {
                    ui.painter().rect_filled(r, r_md(), if resp.hovered() { mix(v.faint_bg_color, v.text_color(), 0.06) } else { v.faint_bg_color });
                }
                // A tiny screen with the bar along its edge.
                let scr = Rect::from_center_size(Pos2::new(r.left() + 24.0, r.center().y), Vec2::new(26.0, 18.0));
                ui.painter().rect_stroke(scr, 3.0, Stroke::new(1.2, v.weak_text_color()), egui::StrokeKind::Inside);
                let bar_y = if p.edge == Edge::Top { scr.top() + 3.0 } else { scr.bottom() - 6.0 };
                ui.painter().rect_filled(Rect::from_min_size(Pos2::new(scr.left() + 3.0, bar_y), Vec2::new(scr.width() - 6.0, 3.0)), 1.0, accent);
                ui.painter().text(Pos2::new(r.left() + 46.0, r.top() + 15.0), Align2::LEFT_CENTER, format!("Panel {}", i + 1), ui_font(13.5), v.strong_text_color());
                ui.painter().text(Pos2::new(r.left() + 46.0, r.top() + 33.0), Align2::LEFT_CENTER, place(p), ui_font(11.5), v.weak_text_color());
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    ops.push(Op::Select(i));
                }
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(120.0, 48.0), Sense::click());
            let stroke = Stroke::new(1.0, if resp.hovered() { accent } else { mix(v.panel_fill, v.text_color(), 0.25) });
            ui.painter().rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "+  New Panel", ui_font(13.0), if resp.hovered() { v.strong_text_color() } else { v.text_color() });
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Add a panel along the bottom (change its edge below)").clicked() {
                ops.push(Op::AddPanel);
            }
        });
        ui.add_space(16.0);
        if n == 0 {
            hint(ui, "No panels. Add one to put the clock, the launcher and the rest along an edge of the screen.");
        } else {
            self.preview(ui, sel);
            ui.add_space(16.0);
            self.place_card(ui, sel, &mut ops);
            ui.add_space(20.0);
            self.zones(ui, sel, &mut ops);
        }
        for op in ops {
            match op {
                Op::Select(i) => self.panel_sel = i,
                Op::AddPanel => self.panel_sel = self.s.panels.len(),
                Op::Duplicate(_) => self.panel_sel = self.s.panels.len(),
                Op::MovePanel(i, by) => self.panel_sel = (i as i32 + by).clamp(0, n as i32 - 1) as usize,
                _ => {}
            }
            apply(&mut self.s.panels, op);
        }
    }

    /// The chosen panel as it looks, over a strip of the desktop.
    fn preview(&mut self, ui: &mut egui::Ui, sel: usize) {
        let ctx = ui.ctx().clone();
        let p = self.s.panels[sel].clone();
        let w = ui.available_width();
        // Drawn as on a screen at least 1200 points wide, then shrunk to fit.
        let virt_w = w.max(1200.0);
        let scale = w / virt_w;
        let ph = p.height();
        let back_h = (ph + 34.0) * scale + 10.0;
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, back_h), Sense::hover());
        let accent = ui.visuals().selection.bg_fill;
        let base = mix(accent, Color32::from_rgb(20, 22, 30), 0.72);
        // A soft gradient, like a wallpaper, under the bar.
        let (c1, c2) = (base, mix(base, accent, 0.3));
        let mut mesh = egui::Mesh::default();
        let inner = r.shrink(2.0);
        for (pt, c) in [(inner.left_top(), c1), (inner.right_top(), c2), (inner.right_bottom(), mix(c2, c1, 0.4)), (inner.left_bottom(), mix(c1, Color32::BLACK, 0.15))] {
            mesh.colored_vertex(pt, c);
        }
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        ui.painter().rect_filled(r, r_sm(), c1);
        ui.painter().add(egui::Shape::mesh(mesh));
        ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, mix(base, Color32::WHITE, 0.08)), egui::StrokeKind::Inside);
        let kit = self.preview_kit.get_or_insert_with(|| crate::ui::Kit::new(self.shot.is_none()));
        kit.begin(&ctx, &self.s);
        let status = crate::shot::demo_status();
        let extra = crate::ui::Extra { notes: crate::shot::demo_notes(), notifying: true, user: String::new(), ..Default::default() };
        let mut acts = Vec::new();
        // The panel on its own layer, scaled down onto the preview.
        let layer = LayerId::new(ui.layer_id().order, Id::unique("shell-panel-preview"));
        let y = if p.edge == Edge::Top { 0.0 } else { (back_h - 10.0) / scale - ph };
        let virt = Rect::from_min_size(Pos2::new(0.0, y + 5.0 / scale), Vec2::new(virt_w, ph));
        let mut s = self.s.clone();
        // Shown on the screen it's for.
        s.panels = vec![p.clone()];
        let mut m = crate::ui::Model { s: &s, status: &status, extra: &extra, apps: &self.apps, compositor: crate::sys::Compositor::Niri, output: "", acts: &mut acts };
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(virt));
        // Clipped to the preview and to what the page shows of it (in the layer's own, unscaled points).
        let shown = r.intersect(ui.clip_rect());
        child.set_clip_rect(Rect::from_min_max(((shown.min - r.min) / scale).to_pos2(), ((shown.max - r.min) / scale).to_pos2()));
        crate::bar::panel(&mut child, &mut m, kit, 0);
        ctx.set_transform_layer(layer, egui::emath::TSTransform::new(r.min.to_vec2(), scale));
        ctx.set_sublayer(ui.layer_id(), layer);
    }

    /// Where the panel goes: its edge, style and screen; and the panel's own buttons.
    fn place_card(&mut self, ui: &mut egui::Ui, sel: usize, ops: &mut Vec<Op>) {
        let fill = ui.visuals().faint_bg_color;
        let n = self.s.panels.len();
        let screens = self.screens.clone();
        egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(18, 14)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            let p = &mut self.s.panels[sel];
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);
                ui.label("Position");
                segmented(ui, &mut p.edge, &[(Edge::Top, "Top"), (Edge::Bottom, "Bottom")]);
                ui.add_space(12.0);
                ui.label("Style");
                segmented(ui, &mut p.floating, &[(true, "Floating"), (false, "Edge to Edge")]);
                ui.add_space(12.0);
                ui.label("Screen");
                egui::ComboBox::from_id_salt(("shell-panel-screen", sel)).selected_text(if p.screen.is_empty() { "Every screen".to_string() } else { p.screen.clone() }).width(170.0).show_ui(ui, |ui| {
                    ui.selectable_value(&mut p.screen, String::new(), "Every screen");
                    let mut names = screens.clone();
                    if !p.screen.is_empty() && !names.contains(&p.screen) {
                        names.push(p.screen.clone());
                    }
                    for s in names {
                        ui.selectable_value(&mut p.screen, s.clone(), s);
                    }
                });
            });
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if ui.button("Duplicate").on_hover_text("A copy of this panel, to change").clicked() {
                    ops.push(Op::Duplicate(sel));
                }
                if n > 1 {
                    if ui.add_enabled(sel > 0, egui::Button::new("Move Earlier")).on_hover_text("Panels on the same edge stack; earlier ones sit nearer the edge").clicked() {
                        ops.push(Op::MovePanel(sel, -1));
                    }
                    if ui.add_enabled(sel + 1 < n, egui::Button::new("Move Later")).clicked() {
                        ops.push(Op::MovePanel(sel, 1));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let danger = rivet_look::theme::status(ui.ctx()).danger;
                    if ui.add(egui::Button::new(egui::RichText::new("Remove Panel").color(danger))).clicked() {
                        ops.push(Op::RemovePanel(sel));
                    }
                });
            });
        });
    }

    /// The three zones as columns of items: drag to reorder or move between them.
    fn zones(&mut self, ui: &mut egui::Ui, sel: usize, ops: &mut Vec<Op>) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let accent = v.selection.bg_fill;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Items").strong().size(14.5));
            ui.label(egui::RichText::new("Drag to reorder or move between zones").color(v.weak_text_color()).size(12.0));
        });
        ui.add_space(6.0);
        let held = egui::DragAndDrop::payload::<Held>(&ctx).map(|h| *h);
        let pointer = ctx.pointer_latest_pos();
        let released = ctx.input(|i| i.pointer.any_released());
        let panel = self.s.panels[sel].clone();
        let gap = 10.0;
        let col_w = (ui.available_width() - gap * 2.0) / 3.0;
        let clock_format = &mut self.s.clock_format;
        let (ws_style, ws_names, ws_hide_empty, ws_scroll) = (&mut self.s.ws_style, &mut self.s.ws_names, &mut self.s.ws_hide_empty, &mut self.s.ws_scroll);
        let (ai_providers, ai_bar, ai_left, ai_pins) = (&mut self.s.ai_providers, &mut self.s.ai_bar, &mut self.s.ai_left, &mut self.s.ai_pins);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (z, zone) in ZONES.iter().enumerate() {
                ui.allocate_ui_with_layout(Vec2::new(col_w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(col_w);
                    let fr = egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(r_lg()).inner_margin(egui::Margin::same(10)).show(ui, |ui| {
                        ui.set_width(col_w - 20.0);
                        ui.spacing_mut().item_spacing.y = 6.0;
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(*zone).strong());
                            ui.label(egui::RichText::new(panel.zones[z].len().to_string()).color(v.weak_text_color()).size(12.0));
                        });
                        let mut rows: Vec<Rect> = Vec::new();
                        for (k, id) in panel.zones[z].iter().enumerate() {
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click_and_drag());
                            rows.push(r);
                            resp.dnd_set_drag_payload(Held(z, k));
                            let me = held == Some(Held(z, k));
                            let draw = |p: &egui::Painter, r: Rect, lifted: bool| {
                                let fill = if lifted { mix(v.panel_fill, accent, 0.18) } else { v.panel_fill };
                                p.rect_filled(r, r_md(), fill);
                                if lifted {
                                    p.rect_stroke(r, r_md(), Stroke::new(1.0, accent), egui::StrokeKind::Inside);
                                }
                                // The grip.
                                for (dx, dy) in [(0.0, -4.0), (0.0, 0.0), (0.0, 4.0), (4.0, -4.0), (4.0, 0.0), (4.0, 4.0)] {
                                    p.circle_filled(Pos2::new(r.left() + 10.0 + dx, r.center().y + dy), 1.2, v.weak_text_color());
                                }
                                glyph::item(p, id, Pos2::new(r.left() + 34.0, r.center().y), v.text_color(), accent);
                                let g = crate::ui::one_line(p, &label(id), ui_font(13.0), v.text_color(), r.width() - 80.0);
                                p.galley(Pos2::new(r.left() + 52.0, r.center().y - g.size().y / 2.0), g, v.text_color());
                            };
                            if me {
                                // Its place stays as a gap; the row follows the pointer.
                                ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, mix(v.faint_bg_color, v.text_color(), 0.2)), egui::StrokeKind::Inside);
                                if let Some(pos) = pointer {
                                    let lp = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("shell-item-drag")));
                                    draw(&lp, Rect::from_center_size(pos, r.size()), true);
                                }
                                ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
                                continue;
                            }
                            draw(ui.painter(), r, false);
                            let tip = ITEMS.iter().find(|x| x.0 == id).map(|x| x.2).unwrap_or("Not part of the shell yet");
                            // Remove (and for the clock, how it shows), on the right.
                            let xr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.center().y), Vec2::splat(24.0));
                            let x_resp = ui.interact(xr, ui.scope_id().with(("item-x", z, k)), Sense::click());
                            if resp.hovered() || x_resp.hovered() {
                                if x_resp.hovered() {
                                    ui.painter().circle_filled(xr.center(), 11.0, mix(v.panel_fill, v.text_color(), 0.12));
                                }
                                glyph::x(ui.painter(), xr.center(), 4.0, v.weak_text_color());
                            }
                            if x_resp.on_hover_text("Take off the panel").clicked() {
                                ops.push(Op::RemoveItem(sel, z, k));
                            }
                            if id == "clock" {
                                let gr = xr.translate(Vec2::new(-26.0, 0.0));
                                let g_resp = ui.interact(gr, ui.scope_id().with(("item-gear", z, k)), Sense::click());
                                if g_resp.hovered() {
                                    ui.painter().circle_filled(gr.center(), 11.0, mix(v.panel_fill, v.text_color(), 0.12));
                                }
                                glyph::gear(ui.painter(), gr.center(), v.weak_text_color());
                                egui::Popup::menu(&g_resp).show(|ui| {
                                    ui.label(egui::RichText::new("The clock shows").small().color(v.weak_text_color()));
                                    for f in ["%H:%M", "%a %b %-d  %H:%M", "%A %-d %B  %H:%M", "%-I:%M %p", "%a %b %-d  %-I:%M %p", "%Y-%m-%d  %H:%M"] {
                                        let shown = chrono::Local::now().format(f).to_string();
                                        ui.selectable_value(clock_format, f.to_string(), shown);
                                    }
                                });
                                g_resp.on_hover_text("How the clock shows");
                            }
                            if id == "workspaces" {
                                let gr = xr.translate(Vec2::new(-26.0, 0.0));
                                let g_resp = ui.interact(gr, ui.scope_id().with(("item-gear", z, k)), Sense::click());
                                if g_resp.hovered() {
                                    ui.painter().circle_filled(gr.center(), 11.0, mix(v.panel_fill, v.text_color(), 0.12));
                                }
                                glyph::gear(ui.painter(), gr.center(), v.weak_text_color());
                                egui::Popup::menu(&g_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                                    ui.set_min_width(220.0);
                                    ui.label(egui::RichText::new("Style").small().color(v.weak_text_color()));
                                    ui.horizontal(|ui| {
                                        for st in WsStyle::ALL {
                                            ui.selectable_value(ws_style, st, st.label());
                                        }
                                    });
                                    ui.add_space(4.0);
                                    if *ws_style != WsStyle::Dots {
                                        ui.label(egui::RichText::new("Each shows its").small().color(v.weak_text_color()));
                                        ui.horizontal(|ui| {
                                            ui.selectable_value(ws_names, true, "Name").on_hover_text("Its name when it has one, else its number");
                                            ui.selectable_value(ws_names, false, "Number");
                                        });
                                        ui.add_space(4.0);
                                    }
                                    ui.checkbox(ws_hide_empty, "Hide empty workspaces").on_hover_text("Workspaces with no windows are left out (the one you're on always shows)");
                                    ui.checkbox(ws_scroll, "Scroll to switch workspaces");
                                });
                                g_resp.on_hover_text("How the workspaces show");
                            }
                            if id == "ai" {
                                let gr = xr.translate(Vec2::new(-26.0, 0.0));
                                let g_resp = ui.interact(gr, ui.scope_id().with(("item-gear", z, k)), Sense::click());
                                if g_resp.hovered() {
                                    ui.painter().circle_filled(gr.center(), 11.0, mix(v.panel_fill, v.text_color(), 0.12));
                                }
                                glyph::gear(ui.painter(), gr.center(), v.weak_text_color());
                                egui::Popup::menu(&g_resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                                    ui.set_min_width(260.0);
                                    ui.label(egui::RichText::new("Read the usage of").small().color(v.weak_text_color()));
                                    for (pid, pname) in crate::ai::PROVIDERS {
                                        let mut on = ai_providers.iter().any(|x| x == pid);
                                        if ui.checkbox(&mut on, *pname).changed() {
                                            if on {
                                                ai_providers.push(pid.to_string());
                                                // Kept in the providers' order.
                                                ai_providers.sort_by_key(|x| crate::ai::PROVIDERS.iter().position(|p| p.0 == x));
                                            } else {
                                                ai_providers.retain(|x| x != pid);
                                            }
                                        }
                                    }
                                    ui.add_space(4.0);
                                    ui.label(egui::RichText::new("With nothing pinned (pin limits in the item's popup), the panel shows").small().color(v.weak_text_color()));
                                    egui::ComboBox::from_id_salt("ai-bar").selected_text(if ai_bar == "auto" { "The most used limit" } else { crate::ai::name(ai_bar) }).show_ui(ui, |ui| {
                                        ui.selectable_value(ai_bar, "auto".to_string(), "The most used limit");
                                        for (pid, pname) in crate::ai::PROVIDERS {
                                            ui.selectable_value(ai_bar, pid.to_string(), format!("{pname}'s most used limit"));
                                        }
                                    });
                                    let n = ai_pins.len();
                                    if n > 0 && ui.button(format!("Unpin all {n}")).clicked() {
                                        ai_pins.clear();
                                    }
                                    ui.add_space(4.0);
                                    ui.horizontal(|ui| {
                                        ui.selectable_value(ai_left, false, "Used");
                                        ui.selectable_value(ai_left, true, "Left");
                                    });
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(format!("Claude, Codex and Grok use their own sign-ins. OpenRouter and DeepSeek take OPENROUTER_API_KEY and DEEPSEEK_API_KEY from the environment or {}.", crate::ai::env_file().display()))
                                            .small()
                                            .color(v.weak_text_color()),
                                    );
                                });
                                g_resp.on_hover_text("What the AI item reads and shows");
                            }
                            resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(tip);
                        }
                        if panel.zones[z].is_empty() {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
                            ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, mix(v.faint_bg_color, v.text_color(), 0.18)), egui::StrokeKind::Inside);
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Empty · drop items here", ui_font(12.0), v.weak_text_color());
                            rows.push(Rect::NOTHING);
                        }
                        // Add.
                        let add = ui.add_sized([ui.available_width(), 30.0], egui::Button::new("+  Add Item"));
                        egui::Popup::menu(&add).width(300.0).show(|ui| {
                            ui.set_min_width(300.0);
                            for (id, name, what) in ITEMS {
                                let on = panel.has(id);
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(300.0, 42.0), if on { Sense::hover() } else { Sense::click() });
                                if !on && resp.hovered() {
                                    ui.painter().rect_filled(r, r_sm(), mix(v.panel_fill, v.text_color(), 0.09));
                                }
                                let col = if on { v.weak_text_color().gamma_multiply(0.6) } else { v.text_color() };
                                glyph::item(ui.painter(), id, Pos2::new(r.left() + 18.0, r.center().y), col, if on { col } else { accent });
                                ui.painter().text(Pos2::new(r.left() + 38.0, r.top() + 13.0), Align2::LEFT_CENTER, *name, ui_font(13.0), col);
                                let sub = if on { "Already on this panel" } else { what };
                                ui.painter().text(Pos2::new(r.left() + 38.0, r.top() + 30.0), Align2::LEFT_CENTER, sub, ui_font(11.5), v.weak_text_color());
                                if !on && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    ops.push(Op::AddItem(sel, z, id.to_string()));
                                    ui.close();
                                }
                            }
                        });
                        rows
                    });
                    // Dropping into this column: where among its rows.
                    let rows = fr.inner;
                    let area = fr.response.rect;
                    if let (Some(h), Some(pos)) = (held, pointer)
                        && area.contains(pos)
                    {
                        let at = rows.iter().filter(|r| r.is_positive() && r.center().y < pos.y).count();
                        let y = match rows.get(at) {
                            Some(r) if r.is_positive() => r.top() - 3.0,
                            _ => rows.iter().rev().find(|r| r.is_positive()).map(|r| r.bottom() + 3.0).unwrap_or(area.top() + 40.0),
                        };
                        ui.painter().line_segment([Pos2::new(area.left() + 10.0, y), Pos2::new(area.right() - 10.0, y)], Stroke::new(2.5, accent));
                        if released {
                            egui::DragAndDrop::clear_payload(&ctx);
                            ops.push(Op::MoveItem(sel, (h.0, h.1), (z, at)));
                        }
                    }
                });
            }
        });
        ui.add_space(14.0);
        hint(ui, "Zones: Left starts at the panel's left end, Center sits in the middle, Right ends at its right end. Items that don't fit on a narrow screen are left out from the inner end. Plugins will add their own items here.");
    }
}

fn apply(panels: &mut Vec<PanelCfg>, op: Op) {
    match op {
        Op::Select(_) => {}
        Op::AddPanel => {
            // The first one on top; then along the bottom.
            panels.push(if panels.is_empty() { PanelCfg::default() } else { PanelCfg::new_bottom() });
        }
        Op::Duplicate(i) => {
            let copy = panels[i].clone();
            panels.push(copy);
        }
        Op::RemovePanel(i) => {
            panels.remove(i);
        }
        Op::MovePanel(i, by) => {
            let j = (i as i32 + by).clamp(0, panels.len() as i32 - 1) as usize;
            panels.swap(i, j);
        }
        Op::MoveItem(p, (fz, fk), (tz, mut tk)) => {
            if fk >= panels[p].zones[fz].len() {
                return;
            }
            let id = panels[p].zones[fz].remove(fk);
            if fz == tz && fk < tk {
                tk -= 1;
            }
            let zone = &mut panels[p].zones[tz];
            zone.insert(tk.min(zone.len()), id);
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
        // The clock from the center to the right zone's start, then to the end of the left.
        apply(&mut ps, Op::MoveItem(0, (1, 0), (2, 0)));
        assert_eq!(ps[0].zones[2][0], "clock");
        let left = ps[0].zones[0].len();
        apply(&mut ps, Op::MoveItem(0, (2, 0), (0, left)));
        assert_eq!(ps[0].zones[0].last().map(String::as_str), Some("clock"));
        // Within a zone: the first item dropped after the second.
        let first = ps[0].zones[0][0].clone();
        apply(&mut ps, Op::MoveItem(0, (0, 0), (0, 2)));
        assert_eq!(ps[0].zones[0][1], first);
        apply(&mut ps, Op::AddItem(1, 0, "system".into()));
        apply(&mut ps, Op::Duplicate(1));
        assert!(ps[2].has("system"));
        apply(&mut ps, Op::MovePanel(1, -1));
        assert!(ps[0].has("system"));
        apply(&mut ps, Op::RemovePanel(0));
        assert_eq!(ps.len(), 2);
    }
}
