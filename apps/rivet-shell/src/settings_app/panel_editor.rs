//! Settings' Panels page, laid out like the panel it edits: a tab for each panel, a live
//! preview of the chosen one, its place (edge, style, screen), and its three zones side by side
//! as columns of items that are dragged to reorder them or move them between zones; "Add Item"
//! offers the items not on the panel yet.

use eframe::egui::{self, Align2, Color32, Id, LayerId, Order, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md, r_sm, selected_tile};

use super::{SettingsApp, hint};
use crate::config::{Along, DockHide, Edge};
use crate::glyph;
use crate::panels::{Group, ITEMS, MIN_LENGTH, Paint, PanelCfg, ZONES, ZONES_DOWN, label};
use crate::style::Roles;

/// A change to make after drawing (the lists can't change while they're drawn).
enum Op {
    Select(usize),
    AddPanel,
    /// A sub panel for this panel.
    AddSub(usize),
    /// Sub panel, its new name.
    RenameSub(usize, String),
    Duplicate(usize),
    RemovePanel(usize),
    MovePanel(usize, i32),
    /// Panel; from zone and place; to zone and place (before the item now there).
    MoveItem(usize, (usize, usize), (usize, usize)),
    RemoveItem(usize, usize, usize),
    /// Panel, zone, place: the item there becomes this one (a spacer's size is in its id).
    SetItem(usize, usize, usize, String),
    AddItem(usize, usize, String),
    /// Panel, item: into this group (None: out of its group), or a new one.
    Group(usize, String, Option<usize>),
    NewGroup(usize, String),
}

/// What's held while an item is dragged: its zone and place.
#[derive(Clone, Copy, PartialEq)]
struct Held(usize, usize);

/// The Add Item list's width.
const ADD_W: f32 = 330.0;
const ROW: f32 = 38.0;

fn ui_font(size: f32) -> egui::FontId {
    rivet_look::fonts::ui_font(size)
}

/// A panel's name on its tab and in lists: "Panel 2", or a sub panel's own.
fn title(panels: &[PanelCfg], i: usize) -> String {
    match panels.get(i).and_then(|p| p.sub.as_ref()) {
        Some(s) => s.name.clone(),
        None => format!("Panel {}", i + 1),
    }
}

fn place(p: &PanelCfg) -> String {
    match &p.sub {
        Some(s) if s.host.is_none() => return "Not attached".into(),
        Some(s) => return format!("Under Panel {}", s.host.unwrap_or(0) + 1),
        None => {}
    }
    let edge = p.edge.label();
    let screen = if p.screen.is_empty() { "every screen".to_string() } else { p.screen.clone() };
    format!("{edge} · {screen}")
}

/// A soft gradient in the accent, like a wallpaper, for a preview of part of the desktop.
pub(super) fn backdrop(ui: &egui::Ui, r: Rect) {
    let accent = ui.visuals().selection.bg_fill;
    let base = mix(accent, Color32::from_rgb(20, 22, 30), 0.72);
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
}

/// A row of segmented choices (like the Edge and Style switches).
pub(super) fn segmented<T: PartialEq + Copy>(ui: &mut egui::Ui, value: &mut T, choices: &[(T, &str)]) {
    let v = ui.visuals().clone();
    let accent = v.selection.bg_fill;
    let h = 30.0;
    let widths: Vec<f32> = choices.iter().map(|(_, t)| ui.painter().layout_no_wrap(t.to_string(), ui_font(13.0), v.text_color()).size().x + 28.0).collect();
    let (r, whole) = ui.allocate_exact_size(Vec2::new(widths.iter().sum::<f32>() + 4.0, h), Sense::hover());
    ui.painter().rect_filled(r, r_md(), v.widgets.inactive.weak_bg_fill);
    ui.painter().rect_stroke(r, r_md(), v.widgets.inactive.bg_stroke, egui::StrokeKind::Inside);
    let mut x = r.left() + 2.0;
    for ((val, text), w) in choices.iter().zip(widths) {
        let cell = Rect::from_min_size(Pos2::new(x, r.top() + 2.0), Vec2::new(w, h - 4.0));
        x += w;
        // Named after this switch: two on a card can share a choice's name ("Left").
        let resp = ui.interact(cell, whole.id.with(("seg", text)), Sense::click());
        let on = *value == *val;
        if on {
            ui.painter().rect_filled(cell, r_sm(), accent);
        } else if resp.hovered() {
            ui.painter().rect_filled(cell, r_sm(), v.widgets.hovered.weak_bg_fill);
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
                // As long as its length, where it sits along the edge.
                let mini = PanelCfg { floating: false, ..p.clone() };
                let inside = scr.shrink(3.0);
                let strip = match p.edge {
                    Edge::Top => Rect::from_min_size(inside.min, Vec2::new(inside.width(), 3.0)),
                    Edge::Bottom => Rect::from_min_size(Pos2::new(inside.left(), inside.bottom() - 3.0), Vec2::new(inside.width(), 3.0)),
                    Edge::Left => Rect::from_min_size(inside.min, Vec2::new(3.0, inside.height())),
                    Edge::Right => Rect::from_min_size(Pos2::new(inside.right() - 3.0, inside.top()), Vec2::new(3.0, inside.height())),
                };
                ui.painter().rect_filled(mini.bar_rect(strip), 1.0, accent);
                ui.painter().text(Pos2::new(r.left() + 46.0, r.top() + 15.0), Align2::LEFT_CENTER, title(&self.s.panels, i), ui_font(13.5), v.strong_text_color());
                ui.painter().text(Pos2::new(r.left() + 46.0, r.top() + 33.0), Align2::LEFT_CENTER, place(p), ui_font(11.5), v.weak_text_color());
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    ops.push(Op::Select(i));
                }
            }
            let (r, resp) = ui.allocate_exact_size(Vec2::new(120.0, 48.0), Sense::click());
            let stroke = Stroke::new(1.0, if resp.hovered() { accent } else { mix(v.panel_fill, v.text_color(), 0.25) });
            ui.painter().rect_stroke(r, r_md(), stroke, egui::StrokeKind::Inside);
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "+  New Panel", ui_font(13.0), if resp.hovered() { v.strong_text_color() } else { v.text_color() });
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Adds a panel along the bottom of the screen").clicked() {
                ops.push(Op::AddPanel);
            }
        });
        ui.add_space(16.0);
        if let Some(note) = self.note_panel.clone() {
            ui.horizontal(|ui| {
                hint(ui, &note);
                if ui.small_button("OK").clicked() {
                    self.note_panel = None;
                }
            });
            ui.add_space(10.0);
        }
        if n == 0 {
            hint(ui, "No panels. Add one to put the clock, the launcher and the rest along an edge of the screen.");
        } else {
            self.preview(ui, sel);
            ui.add_space(16.0);
            self.place_card(ui, sel, &mut ops);
            ui.add_space(20.0);
            self.zones(ui, sel, &mut ops);
            ui.add_space(20.0);
            self.groups_card(ui, sel);
            ui.add_space(20.0);
            self.panel_look_section(ui, sel);
            self.handle_section(ui, sel);
        }
        for op in ops {
            match op {
                Op::Select(i) => self.panel_sel = i,
                Op::AddPanel | Op::AddSub(_) => self.panel_sel = self.s.panels.len(),
                Op::Duplicate(_) => self.panel_sel = self.s.panels.len(),
                Op::MovePanel(i, by) => self.panel_sel = (i as i32 + by).clamp(0, n as i32 - 1) as usize,
                _ => {}
            }
            apply(&mut self.s.panels, op);
        }
        // Sub panels follow their hosts (edge, screen), whatever was changed above.
        self.s.tidy_panels();
    }

    /// A sub panel's handle: the tab shown past its panel while it's rolled up, where pausing
    /// the pointer opens it. (Only for one that opens that way.)
    fn handle_section(&mut self, ui: &mut egui::Ui, sel: usize) {
        let edge = self.s.panels[sel].edge;
        let colors = Roles::new(&self.look, &self.s);
        let surface = ui.visuals().panel_fill;
        let Some(sub) = self.s.panels[sel].sub.as_mut().filter(|s| s.reveal && !s.stay) else { return };
        ui.add_space(20.0);
        let h = &mut sub.handle;
        super::section(ui, "Handle", &mut |ui| {
            super::controls::master(ui, &mut h.show, "Show the handle");
            hint(ui, "Shows past the panel while the sub panel is rolled up, to mark where pausing the pointer opens it.");
            super::controls::rule(ui);
            ui.add_enabled_ui(h.show, |ui| {
                super::grid(ui, &format!("shell-sub-handle-{sel}"), &mut |ui| {
                    ui.label("Shape");
                    segmented(ui, &mut h.shape, &crate::panels::HandleShape::ALL);
                    ui.end_row();
                    ui.label("Length");
                    ui.add(egui::Slider::new(&mut h.length, crate::panels::HANDLE_LENGTH).suffix(" pt"));
                    ui.end_row();
                    ui.label("Thickness");
                    ui.add(egui::Slider::new(&mut h.thick, crate::panels::HANDLE_THICK).suffix(" pt"));
                    ui.end_row();
                    ui.label("Placed");
                    let names = if edge.vertical() { ["Top", "Middle", "Bottom"] } else { ["Left", "Center", "Right"] };
                    segmented(ui, &mut h.along, &[(Along::Start, names[0]), (Along::Center, names[1]), (Along::End, names[2])]);
                    ui.end_row();
                    ui.label("From the panel");
                    ui.add(egui::Slider::new(&mut h.offset, crate::panels::HANDLE_OFFSET).suffix(" pt")).on_hover_text("How far from the panel's bar it sits. Below zero, it sits over the bar's edge.");
                    ui.end_row();
                    ui.label("Opacity");
                    let mut pct = (h.opacity * 100.0).round();
                    if ui.add(egui::Slider::new(&mut pct, 15.0..=100.0).fixed_decimals(0).suffix("%")).changed() {
                        h.opacity = pct / 100.0;
                    }
                    ui.end_row();
                    ui.label("Color");
                    // Surface: the panel's own colour (outlined, to show over anything).
                    use crate::style::Role;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        if let Some(r) = super::look_opts::roles(ui, h.color.unwrap_or(Role::Surface), &Role::ALL, &colors, surface, h.show) {
                            h.color = Some(r).filter(|r| *r != Role::Surface);
                        }
                    });
                    ui.end_row();
                });
            });
        });
    }

    /// The chosen panel as it looks, over a strip of the desktop.
    fn preview(&mut self, ui: &mut egui::Ui, sel: usize) {
        let ctx = ui.ctx().clone();
        // A drawer is shown on its own here: as a bar, not as the part that joins its host.
        let mut p = self.s.panels[sel].clone();
        if let Some(sub) = p.sub.as_mut() {
            sub.drawer = false;
        }
        p.lead = 0.0;
        let w = ui.available_width();
        let ph = p.height();
        let down = p.vertical();
        // Drawn as on a screen at least 1200 points wide, then shrunk to fit; a side panel, as
        // on a small screen (1024 by 640) at half its size.
        let (virt_w, scale) = if down { (1024.0f32.min(w / 0.5), 0.5) } else { (w.max(1200.0), w / w.max(1200.0)) };
        let back_h = if down { 640.0 * scale + 10.0 } else { (ph + 34.0) * scale + 10.0 };
        let (r, _) = ui.allocate_exact_size(Vec2::new(virt_w * scale, back_h), Sense::hover());
        backdrop(ui, r);
        let kit = self.preview_kit.get_or_insert_with(|| crate::ui::Kit::new(self.shot.is_none()));
        kit.begin(&ctx, &self.s);
        let status = crate::shot::demo_status();
        let extra = crate::ui::Extra { notes: crate::shot::demo_notes(), notifying: true, user: String::new(), ..Default::default() };
        let mut acts = Vec::new();
        // The panel on its own layer, scaled down onto the preview.
        let layer = LayerId::new(ui.layer_id().order, Id::unique("shell-panel-preview"));
        let y = if p.edge == Edge::Bottom { (back_h - 10.0) / scale - ph } else { 0.0 };
        let virt = match p.edge {
            Edge::Left => Rect::from_min_size(Pos2::new(5.0 / scale, 5.0 / scale), Vec2::new(ph, 640.0)),
            Edge::Right => Rect::from_min_size(Pos2::new(virt_w - 5.0 / scale - ph, 5.0 / scale), Vec2::new(ph, 640.0)),
            _ => Rect::from_min_size(Pos2::new(0.0, y + 5.0 / scale), Vec2::new(virt_w, ph)),
        };
        // The whole list, as its sub panels' buttons need the sub panels to be there.
        let mut s = self.s.clone();
        s.panels[sel] = p.clone();
        let mut m = crate::ui::Model { s: &s, status: &status, extra: &extra, apps: &self.apps, compositor: crate::sys::Compositor::Niri, output: "", acts: &mut acts };
        let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(virt));
        // Clipped to the preview and to what the page shows of it (in the layer's own, unscaled points).
        let shown = r.intersect(ui.clip_rect());
        child.set_clip_rect(Rect::from_min_max(((shown.min - r.min) / scale).to_pos2(), ((shown.max - r.min) / scale).to_pos2()));
        crate::bar::panel(&mut child, &mut m, kit, sel);
        ctx.set_transform_layer(layer, egui::emath::TSTransform::new(r.min.to_vec2(), scale));
        ctx.set_sublayer(ui.layer_id(), layer);
    }

    /// Where the panel goes: its edge, style and screen; and the panel's own buttons.
    fn place_card(&mut self, ui: &mut egui::Ui, sel: usize, ops: &mut Vec<Op>) {
        let fill = ui.visuals().faint_bg_color;
        let n = self.s.panels.len();
        let screens = self.screens.clone();
        // The panels a sub panel can be attached to.
        let hosts: Vec<(usize, String)> = self.s.panels.iter().enumerate().filter(|(_, p)| p.sub.is_none()).map(|(i, p)| (i, format!("Panel {} ({})", i + 1, p.edge.label()))).collect();
        let is_sub = self.s.panels[sel].sub.is_some();
        egui::Frame::NONE.fill(fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(20, 18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            super::on_card(ui.visuals_mut(), fill);
            let p = &mut self.s.panels[sel];
            let mut no_host = false;
            super::grid(ui, &format!("shell-panel-where-{sel}"), &mut |ui| {
                if let Some(sub) = p.sub.as_mut() {
                    ui.label("Name");
                    // Typed into a copy, and taken when it's done (its button follows the name).
                    let id = Id::unique(("shell-sub-name", sel));
                    let mut name = ui.ctx().data(|d| d.get_temp::<String>(id)).unwrap_or_else(|| sub.name.clone());
                    let resp = ui.add(egui::TextEdit::singleline(&mut name).desired_width(200.0));
                    if resp.lost_focus() {
                        ui.ctx().data_mut(|d| d.remove::<String>(id));
                        if name != sub.name {
                            ops.push(Op::RenameSub(sel, name));
                        }
                    } else if resp.has_focus() {
                        ui.ctx().data_mut(|d| d.insert_temp(id, name));
                    }
                    ui.end_row();
                    ui.label("Attached to");
                    let shown = sub.host.and_then(|h| hosts.iter().find(|x| x.0 == h)).map(|x| x.1.clone()).unwrap_or_else(|| "No Panel".into());
                    egui::ComboBox::from_id_salt(("shell-sub-host", sel)).selected_text(shown).width(200.0).show_ui(ui, |ui| {
                        for (i, name) in &hosts {
                            ui.selectable_value(&mut sub.host, Some(*i), name);
                        }
                    });
                    ui.end_row();
                    no_host = sub.host.is_none();
                } else {
                    ui.label("Position");
                    segmented(ui, &mut p.edge, &[(Edge::Top, "Top"), (Edge::Bottom, "Bottom"), (Edge::Left, "Left"), (Edge::Right, "Right")]);
                    ui.end_row();
                }
                ui.label("Style");
                segmented(ui, &mut p.floating, &[(true, "Floating"), (false, "Edge to Edge")]);
                ui.end_row();
                if !is_sub {
                    ui.label("Screen");
                    egui::ComboBox::from_id_salt(("shell-panel-screen", sel)).selected_text(if p.screen.is_empty() { "Every Screen".to_string() } else { p.screen.clone() }).width(200.0).show_ui(ui, |ui| {
                        ui.selectable_value(&mut p.screen, String::new(), "Every Screen");
                        let mut names = screens.clone();
                        if !p.screen.is_empty() && !names.contains(&p.screen) {
                            names.push(p.screen.clone());
                        }
                        for s in names {
                            ui.selectable_value(&mut p.screen, s.clone(), s);
                        }
                    });
                    ui.end_row();
                }
                ui.label("Length");
                let mut len = p.length.clamp(MIN_LENGTH, 100);
                let of = if p.vertical() { "the screen's height" } else { "the screen's width" };
                if ui.add(egui::Slider::new(&mut len, MIN_LENGTH..=100).suffix("%")).on_hover_text(format!("How much of {of} the panel takes")).changed() {
                    p.length = len;
                }
                ui.end_row();
                if p.length < 100 {
                    ui.label("Placed");
                    let names = if p.vertical() { ["Top", "Middle", "Bottom"] } else { ["Left", "Center", "Right"] };
                    segmented(ui, &mut p.along, &[(Along::Start, names[0]), (Along::Center, names[1]), (Along::End, names[2])]);
                    ui.end_row();
                }
                if !is_sub {
                    ui.label("Hide");
                    segmented(ui, &mut p.hide, &[(DockHide::Never, "Never"), (DockHide::Auto, "Auto-Hide"), (DockHide::Smart, "Smart Hide")]);
                    ui.end_row();
                }
            });
            if no_host {
                ui.add_space(4.0);
                hint(ui, "This sub panel isn't attached to a panel, so it doesn't show. Choose one to bring it back.");
            }
            let edge = p.edge;
            if let Some(sub) = p.sub.as_mut() {
                super::controls::rule(ui);
                super::controls::subhead(ui, "Opening");
                super::grid(ui, &format!("shell-sub-open-{sel}"), &mut |ui| {
                    ui.label("When opened");
                    let was = sub.stay;
                    segmented(ui, &mut sub.stay, &[(false, "Rolls Back Up"), (true, "Stays Open")]);
                    if sub.stay != was {
                        // Left open from now, or back to rolling up.
                        sub.open = sub.stay;
                    }
                    ui.end_row();
                    if !sub.stay {
                        ui.label("Rolls up after");
                        ui.add(egui::Slider::new(&mut sub.secs, crate::panels::SUB_SECS).step_by(0.5).suffix(" s")).on_hover_text("How long it stays once the pointer leaves it");
                        ui.end_row();
                    }
                });
                ui.add_space(6.0);
                if sub.stay {
                    hint(ui, "It stays open, keeping its room from windows, until its button or its key closes it.");
                } else {
                    let side = match edge {
                        Edge::Top => "just below",
                        Edge::Bottom => "just above",
                        Edge::Left => "just right of",
                        Edge::Right => "just left of",
                    };
                    super::controls::switch(ui, &mut sub.reveal, format!("Open when the pointer pauses {side} the panel")).on_hover_text("Pause on the panel's edge on that side, or on the small handle past it. Moving straight through doesn't open it.");
                }
                super::controls::switch(ui, &mut sub.drawer, "Join it to the panel, like a drawer pulled out of it");
                ui.add_enabled_ui(!sub.drawer, |ui| {
                    let mut fit = sub.fit || sub.drawer;
                    if super::controls::switch(ui, &mut fit, "Match the panel's length and place").changed() {
                        sub.fit = fit;
                    }
                });
                super::controls::rule(ui);
                super::controls::subhead(ui, "Key");
                hint(ui, "Bind this in your compositor's settings.");
                ui.add_space(4.0);
                super::controls::command(ui, "Open or close it", &format!("rivet-shell msg subpanel {}", sub.name));
            } else {
                let edge = p.edge.label().to_lowercase();
                let about = match p.hide {
                    DockHide::Never => String::new(),
                    DockHide::Auto => format!("The panel slides away until you move the pointer to the screen's {edge} edge."),
                    DockHide::Smart if super::knows_windows() => format!("The panel shows on an empty workspace and slides away when a window opens. The {edge} edge brings it back."),
                    DockHide::Smart => "Smart Hide needs niri, Hyprland or mango; here it works like Auto-Hide.".to_string(),
                };
                if !about.is_empty() {
                    ui.add_space(4.0);
                    hint(ui, &about);
                }
            }
            super::controls::rule(ui);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                use super::controls::{Icon, button};
                if !is_sub && button(ui, Icon::Layers, "Add Sub Panel").on_hover_text("A panel that rolls out from under this one when you ask for it").clicked() {
                    ops.push(Op::AddSub(sel));
                }
                if button(ui, Icon::Copy, "Duplicate").clicked() {
                    ops.push(Op::Duplicate(sel));
                }
                if n > 1 {
                    if ui.add_enabled_ui(sel > 0, |ui| button(ui, Icon::Back, "Move Earlier")).inner.on_hover_text("Panels on the same edge stack; earlier ones sit closer to the edge").clicked() {
                        ops.push(Op::MovePanel(sel, -1));
                    }
                    if ui.add_enabled_ui(sel + 1 < n, |ui| button(ui, Icon::Forward, "Move Later")).inner.clicked() {
                        ops.push(Op::MovePanel(sel, 1));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if super::controls::danger(ui, if is_sub { "Remove Sub Panel" } else { "Remove Panel" }).clicked() {
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
        let all_panels = self.s.panels.clone();
        let gap = 10.0;
        let col_w = (ui.available_width() - gap * 2.0) / 3.0;
        let colors = Roles::new(&self.look, &self.s);
        let enabled_plugins = self.s.plugins.clone();
        let settings = &mut self.s;
        // Checks: RIVET_SHELL_ITEM_OPTS=volume opens that item's options.
        // A search opens an item's options once.
        let force_open = self.shot.as_ref().and_then(|_| std::env::var("RIVET_SHELL_ITEM_OPTS").ok()).or_else(|| self.open_item.take());
        let shot_add = self.shot.is_some() && std::env::var("RIVET_SHELL_ADD_ITEM").is_ok_and(|v| v == "1");
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (z, zone) in (if panel.vertical() { ZONES_DOWN } else { ZONES }).iter().enumerate() {
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
                                glyph::item(p, id, Pos2::new(r.left() + 34.0, r.center().y), v.text_color(), accent, settings.quick_icon);
                                let g = crate::ui::one_line(p, &label(id), ui_font(13.0), v.text_color(), r.width() - 132.0);
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
                            let size = crate::panels::spacer(id);
                            let tip = ITEMS.iter().find(|x| x.0 == id || (size.is_some() && x.0 == "spacer")).map(|x| x.2).unwrap_or(if id.starts_with("sub:") { "Opens this sub panel" } else { "Not part of the shell yet" });
                            // Remove, and the item's options (a gear) when it has some, on the right.
                            let xr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.center().y), Vec2::splat(24.0));
                            let x_resp = ui.interact(xr, ui.scope_id().with(("item-x", z, k)), Sense::click());
                            if resp.hovered() || x_resp.hovered() {
                                if x_resp.hovered() {
                                    ui.painter().circle_filled(xr.center(), 11.0, mix(v.panel_fill, v.text_color(), 0.12));
                                }
                                glyph::x(ui.painter(), xr.center(), 4.0, v.weak_text_color());
                            }
                            let x_hot = x_resp.hovered();
                            if x_resp.on_hover_text("Remove from the panel").clicked() {
                                ops.push(Op::RemoveItem(sel, z, k));
                            }
                            // A spacer: its size, in the row.
                            if let Some(n) = size {
                                let mut to = n;
                                let dr = Rect::from_center_size(xr.center() - Vec2::new(50.0, 0.0), Vec2::new(60.0, 22.0));
                                // (In a child, so the rows below keep their places.)
                                let mut at = ui.new_child(egui::UiBuilder::new().max_rect(dr).layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight)));
                                if at.add(egui::DragValue::new(&mut to).range(crate::panels::SPACER).suffix(" pt").speed(1.0)).on_hover_text("Width: drag, or click to type a value").changed() {
                                    ops.push(Op::SetItem(sel, z, k, format!("spacer:{to}")));
                                }
                                resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(tip);
                                continue;
                            }
                            let has_opts = super::item_opts::has_options(id);
                            if has_opts {
                                let open = force_open.as_deref() == Some(id.as_str());
                                super::item_opts::gear(ui, xr.center() - Vec2::new(26.0, 0.0), (z, k), id, settings, accent, open);
                            }
                            // Its group: a chip with the group's number in its colours; a ring
                            // while the row is pointed at, to put it in one.
                            let gr = Rect::from_center_size(xr.center() - Vec2::new(52.0, 0.0), Vec2::splat(24.0));
                            let g_resp = ui.interact(gr, ui.scope_id().with(("item-group", z, k)), Sense::click());
                            let mine = panel.group_of(id);
                            match mine.and_then(|g| panel.groups.get(g).map(|x| (g, x))) {
                                Some((g, group)) => {
                                    let (bg, fg) = swatch(group, &colors, &v);
                                    ui.painter().circle_filled(gr.center(), 9.5, bg);
                                    ui.painter().circle_stroke(gr.center(), 9.5, Stroke::new(1.0, mix(bg, v.text_color(), if g_resp.hovered() { 0.6 } else { 0.25 })));
                                    ui.painter().text(gr.center(), Align2::CENTER_CENTER, (g + 1).to_string(), ui_font(11.0), fg);
                                }
                                None if resp.hovered() || x_hot || g_resp.hovered() => {
                                    let col = if g_resp.hovered() { v.text_color() } else { v.weak_text_color() };
                                    ui.painter().circle_stroke(gr.center(), 8.0, Stroke::new(1.2, col));
                                    ui.painter().circle_filled(gr.center(), 2.0, col);
                                }
                                None => {}
                            }
                            let group_tip = match mine {
                                Some(g) => format!("In group {}", g + 1),
                                None => "Put in a group".to_string(),
                            };
                            let g_resp = g_resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(group_tip);
                            egui::Popup::menu(&g_resp).show(|ui| {
                                ui.set_min_width(150.0);
                                for (g, group) in panel.groups.iter().enumerate() {
                                    let names = group.items.iter().filter(|i| *i != id).map(|i| label(i)).collect::<Vec<_>>().join(", ");
                                    let text = if names.is_empty() { format!("Group {}", g + 1) } else { format!("Group {} · {names}", g + 1) };
                                    if ui.selectable_label(mine == Some(g), text).clicked() {
                                        ops.push(Op::Group(sel, id.clone(), Some(g)));
                                        ui.close();
                                    }
                                }
                                if super::controls::button(ui, super::controls::Icon::Plus, "New Group").clicked() {
                                    ops.push(Op::NewGroup(sel, id.clone()));
                                    ui.close();
                                }
                                if mine.is_some() && super::controls::button(ui, super::controls::Icon::Off, "Remove from Group").clicked() {
                                    ops.push(Op::Group(sel, id.clone(), None));
                                    ui.close();
                                }
                            });
                            resp.on_hover_cursor(egui::CursorIcon::Grab).on_hover_text(tip);
                        }
                        if panel.zones[z].is_empty() {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::hover());
                            ui.painter().rect_stroke(r, r_md(), Stroke::new(1.0, mix(v.faint_bg_color, v.text_color(), 0.18)), egui::StrokeKind::Inside);
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Empty · Drop items here", ui_font(12.0), v.weak_text_color());
                            rows.push(Rect::NOTHING);
                        }
                        // Add.
                        let add = ui.add_sized([ui.available_width(), 30.0], egui::Button::new("+  Add Item"));
                        // Checks: RIVET_SHELL_ADD_ITEM=1 opens the first zone's.
                        let mut menu = egui::Popup::menu(&add).width(ADD_W + 16.0);
                        if z == 0 && shot_add {
                            menu = menu.open_memory(Some(egui::SetOpenCommand::Bool(true)));
                        }
                        menu.show(|ui| {
                            ui.set_width(ADD_W + 16.0);
                            ui.spacing_mut().item_spacing.y = 2.0;
                            // One choice: its symbol, its name and a line about it, both kept
                            // inside the row (the whole line shows on hover).
                            let choice = |ui: &mut egui::Ui, id: &str, name: &str, what: &str, on: bool| -> bool {
                                let w = ui.available_width();
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 44.0), if on { Sense::hover() } else { Sense::click() });
                                if !ui.is_rect_visible(r) {
                                    return false;
                                }
                                if !on && resp.hovered() {
                                    ui.painter().rect_filled(r, r_sm(), mix(v.panel_fill, v.text_color(), 0.09));
                                }
                                let col = if on { v.weak_text_color().gamma_multiply(0.6) } else { v.text_color() };
                                let c = Pos2::new(r.left() + 22.0, r.center().y);
                                ui.painter().circle_filled(c, 14.0, mix(v.panel_fill, if on { v.text_color() } else { accent }, if on { 0.05 } else { 0.14 }));
                                glyph::item(ui.painter(), id, c, col, if on { col } else { accent }, settings.quick_icon);
                                // "Added" at the row's end, for the ones on the panel.
                                let end = if on {
                                    let g = ui.painter().layout_no_wrap("Added".into(), ui_font(11.0), v.weak_text_color());
                                    let at = Pos2::new(r.right() - 10.0 - g.size().x, r.center().y - g.size().y / 2.0);
                                    let left = at.x;
                                    ui.painter().galley(at, g, v.weak_text_color());
                                    left - 8.0
                                } else {
                                    r.right() - 10.0
                                };
                                let x = r.left() + 46.0;
                                let g = crate::ui::one_line(ui.painter(), name, ui_font(13.0), col, end - x);
                                ui.painter().galley(Pos2::new(x, r.top() + 13.0 - g.size().y / 2.0), g, col);
                                let sub = if on { v.weak_text_color().gamma_multiply(0.7) } else { v.weak_text_color() };
                                let g = crate::ui::one_line(ui.painter(), what, ui_font(11.5), sub, end - x);
                                let cut = g.elided;
                                ui.painter().galley(Pos2::new(x, r.top() + 31.0 - g.size().y / 2.0), g, sub);
                                let resp = if cut { resp.on_hover_text(what) } else { resp };
                                !on && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                            };
                            let room = (ui.ctx().content_rect().height() - 120.0).clamp(180.0, 460.0);
                            egui::ScrollArea::vertical().max_height(room).auto_shrink([false, true]).show(ui, |ui| {
                                ui.set_width(ADD_W);
                                // The ones that can be added first, then the ones already on the panel.
                                let plugins = crate::plugins::found_cached();
                                for added in [false, true] {
                                    let mut any = false;
                                    let mut head = |ui: &mut egui::Ui| {
                                        if added && !std::mem::replace(&mut any, true) {
                                            ui.add_space(6.0);
                                            ui.label(egui::RichText::new("ON THIS PANEL").font(ui_font(10.5)).extra_letter_spacing(0.8).color(v.weak_text_color()));
                                        }
                                    };
                                    for (id, name, what) in ITEMS {
                                        // A panel takes as many spacers as wanted.
                                        let on = *id != "spacer" && panel.has(id);
                                        if on != added {
                                            continue;
                                        }
                                        head(ui);
                                        if choice(ui, id, name, what, on) {
                                            ops.push(Op::AddItem(sel, z, if *id == "spacer" { "spacer:16".to_string() } else { id.to_string() }));
                                            ui.close();
                                        }
                                    }
                                    // Sub panels' buttons (not on the sub panel itself).
                                    for (i, sub) in all_panels.iter().enumerate().filter_map(|(i, p)| Some((i, p.sub.as_ref()?))) {
                                        let id = sub.item();
                                        if i == sel || panel.has(&id) != added {
                                            continue;
                                        }
                                        head(ui);
                                        if choice(ui, &id, &sub.name, "Opens this sub panel. Right-click it to keep the sub panel open.", added) {
                                            ops.push(Op::AddItem(sel, z, id));
                                            ui.close();
                                        }
                                    }
                                    // The plugins turned on (Settings, Plugins), after the shell's own.
                                    for m in plugins.iter().filter(|m| enabled_plugins.contains(&m.id)) {
                                        let id = format!("plugin:{}", m.id);
                                        if panel.has(&id) != added {
                                            continue;
                                        }
                                        head(ui);
                                        let what = if m.description.is_empty() { "A plugin" } else { m.description.as_str() };
                                        if choice(ui, &id, &m.name, what, added) {
                                            ops.push(Op::AddItem(sel, z, id));
                                            ui.close();
                                        }
                                    }
                                }
                            });
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
        if panel.vertical() {
            hint(ui, "On a side panel, numbers sit under their symbols, the clock wraps onto lines and the window title runs downward. If items don't fit, the ones nearest the middle are hidden first.");
        } else {
            hint(ui, "If items don't fit on a narrow screen, the ones nearest the middle are hidden first. Plugins you turn on appear under Add Item.");
        }
    }
}

/// A group's colours as its chip and card show them: background (the panel's own when it has
/// none) and text.
fn swatch(g: &Group, colors: &Roles, v: &egui::Visuals) -> (Color32, Color32) {
    let paint = |p: Paint| match p {
        Paint::Auto => None,
        Paint::Surface => Some(v.panel_fill),
        Paint::Primary => Some(colors.primary),
        Paint::Secondary => Some(colors.secondary),
        Paint::Tertiary => Some(colors.tertiary),
        Paint::Custom([r, g, b]) => Some(Color32::from_rgb(r, g, b)),
    };
    let soft = mix(v.panel_fill, v.text_color(), 0.14);
    let bg = g.bg.map(|p| paint(p).unwrap_or(soft)).unwrap_or(v.panel_fill);
    let solid = g.bg.is_some_and(|p| p != Paint::Auto);
    (bg, paint(g.fg).unwrap_or(if solid { crate::ui::on(bg) } else { v.text_color() }))
}

/// A row of colour choices for a group, as dots (their names on hover) after the worded
/// ones: "None" for a background, then `auto` (what leaving it to the shell is called here).
/// The last is any colour, from a picker.
fn paints(ui: &mut egui::Ui, id: (usize, usize, &str), value: &mut Option<Paint>, with_none: bool, auto: &str, colors: &Roles) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        if with_none && ui.selectable_label(value.is_none(), "None").clicked() {
            *value = None;
        }
        if ui.selectable_label(*value == Some(Paint::Auto), auto).clicked() {
            *value = Some(Paint::Auto);
        }
        ui.add_space(4.0);
        let v = ui.visuals().clone();
        for (p, name, col) in [(Paint::Surface, "Surface", v.panel_fill), (Paint::Primary, "Primary", colors.primary), (Paint::Secondary, "Secondary", colors.secondary), (Paint::Tertiary, "Tertiary", colors.tertiary)] {
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
            ui.painter().circle_filled(r.center(), 8.5, col);
            ui.painter().circle_stroke(r.center(), 8.5, Stroke::new(1.0, mix(col, v.text_color(), 0.35)));
            if *value == Some(p) {
                ui.painter().circle_stroke(r.center(), 11.0, Stroke::new(2.0, v.strong_text_color()));
            }
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(name).clicked() {
                *value = Some(p);
            }
        }
        // Any colour: the picker shows the one chosen, or starts from the primary.
        let custom = matches!(value, Some(Paint::Custom(_)));
        let mut rgb = match value {
            Some(Paint::Custom(c)) => *c,
            _ => [colors.primary.r(), colors.primary.g(), colors.primary.b()],
        };
        let resp = ui.push_id(id, |ui| ui.color_edit_button_srgb(&mut rgb)).inner.on_hover_text("Any color");
        if custom {
            ui.painter().rect_stroke(resp.rect.expand(2.0), r_sm(), Stroke::new(2.0, v.strong_text_color()), egui::StrokeKind::Outside);
        }
        if resp.changed() {
            *value = Some(Paint::Custom(rgb));
        }
    });
}

impl SettingsApp {
    /// The panel's groups: each one's items, colours, and how many show before the rest fold.
    fn groups_card(&mut self, ui: &mut egui::Ui, sel: usize) {
        let colors = Roles::new(&self.look, &self.s);
        let v = ui.visuals().clone();
        let p = &mut self.s.panels[sel];
        p.tidy_groups();
        let (bar, side) = (p.bar().round() as u32, p.vertical());
        super::section(ui, "Groups", &mut |ui| {
            if p.groups.is_empty() {
                hint(ui, "Grouped items share one background, and a group can fold extra items behind an arrow. To make a group, point at an item above and click its ring.");
                return;
            }
            hint(ui, "Choose a group to change it. To move an item between groups, click its numbered chip in the list above.");
            ui.add_space(6.0);
            super::controls::switch(ui, &mut p.fuse, "Groups that touch fuse into one shape");
            ui.add_space(12.0);
            // The groups as tabs, each in its own colours; one's settings show at a time.
            let sel_id = egui::Id::unique(("shell-group-sel", sel));
            let mut g = ui.data(|d| d.get_temp::<usize>(sel_id)).unwrap_or(0).min(p.groups.len() - 1);
            // Checks: RIVET_SHELL_GROUP=2 shows the second group's settings.
            if crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) && let Some(n) = std::env::var("RIVET_SHELL_GROUP").ok().and_then(|n| n.parse::<usize>().ok()) {
                g = n.saturating_sub(1).min(p.groups.len() - 1);
            }
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(12.0, 12.0);
                ui.add_space(3.0);
                for (i, group) in p.groups.iter().enumerate() {
                    let (bg, fg) = swatch(group, &colors, &v);
                    let names = group.items.iter().map(|i| label(i)).collect::<Vec<_>>().join(", ");
                    let text = crate::ui::one_line(ui.painter(), &format!("{}   {names}", i + 1), ui_font(12.5), fg, 230.0);
                    let cut = text.elided;
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(text.size().x + 24.0, 30.0), Sense::click());
                    let on = i == g;
                    let p = ui.painter();
                    p.rect_filled(r, r_md(), bg);
                    p.rect_stroke(r, r_md(), Stroke::new(1.0, mix(bg, v.text_color(), if resp.hovered() { 0.5 } else { 0.22 })), egui::StrokeKind::Inside);
                    // The chosen one: a ring around it, clear of its own colours.
                    if on {
                        p.rect_stroke(r.expand(3.0), r_md() + 3, Stroke::new(2.0, v.strong_text_color()), egui::StrokeKind::Inside);
                    }
                    p.galley(Pos2::new(r.left() + 12.0, r.center().y - text.size().y / 2.0), text, fg);
                    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                    let resp = if cut { resp.on_hover_text(names) } else { resp };
                    if resp.clicked() {
                        g = i;
                    }
                }
            });
            ui.data_mut(|d| d.insert_temp(sel_id, g));
            ui.add_space(12.0);
            let mut remove = None;
            {
                let group = &mut p.groups[g];
                super::grid(ui, &format!("shell-group-{sel}-{g}"), &mut |ui| {
                    ui.label("Background");
                    ui.horizontal(|ui| {
                        paints(ui, (sel, g, "bg"), &mut group.bg, true, "Soft", &colors);
                        if group.bg.is_some() {
                            ui.add_space(8.0);
                            super::controls::switch(ui, &mut group.gradient, "Fade").on_hover_text("Fades the background to a darker shade along the panel");
                        }
                    });
                    ui.end_row();
                    ui.label("Text");
                    let mut fg = Some(group.fg);
                    paints(ui, (sel, g, "fg"), &mut fg, false, "Automatic", &colors);
                    group.fg = fg.unwrap_or_default();
                    ui.end_row();
                    ui.label(if side { "Width" } else { "Height" });
                    ui.horizontal(|ui| {
                        let mut own = group.height.is_some();
                        segmented(ui, &mut own, &[(false, "The Panel's"), (true, "Its Own")]);
                        match (own, group.height) {
                            (false, _) => group.height = None,
                            // Starting from what it has now: the panel's items' height.
                            (true, None) => group.height = Some(bar.saturating_sub(8).max(crate::panels::GROUP_MIN)),
                            (true, Some(_)) => {}
                        }
                        if let Some(h) = group.height.as_mut() {
                            *h = (*h).clamp(crate::panels::GROUP_MIN, bar.max(crate::panels::GROUP_MIN));
                            ui.add(egui::Slider::new(h, crate::panels::GROUP_MIN..=bar.max(crate::panels::GROUP_MIN)).suffix(" pt")).on_hover_text("At the panel's height, the group fills the panel edge to edge");
                        }
                    });
                    ui.end_row();
                    ui.label("Shows");
                    ui.horizontal(|ui| {
                        let text = |n: u32| match n {
                            0 => "All its items".to_string(),
                            1 => "Its first item".to_string(),
                            n => format!("Its first {n} items"),
                        };
                        egui::ComboBox::from_id_salt(("shell-group-show", sel, g)).selected_text(text(group.show)).width(160.0).show_ui(ui, |ui| {
                            for n in 0..=6 {
                                ui.selectable_value(&mut group.show, n, text(n));
                            }
                        });
                        if group.show > 0 {
                            ui.label(egui::RichText::new("the rest fold behind an arrow").color(v.weak_text_color()).size(12.0));
                        }
                    });
                    ui.end_row();
                });
                ui.add_space(10.0);
                if super::controls::button(ui, super::controls::Icon::Off, &format!("Ungroup {}", g + 1)).on_hover_text("Its items stay on the panel").clicked() {
                    remove = Some(g);
                }
            }
            if let Some(g) = remove {
                p.groups.remove(g);
            }
        });
    }
}

fn apply(panels: &mut Vec<PanelCfg>, op: Op) {
    match op {
        Op::Select(_) => {}
        Op::AddPanel => {
            // The first one on top; then along the bottom.
            panels.push(if panels.is_empty() { PanelCfg::default() } else { PanelCfg::new_bottom() });
        }
        Op::AddSub(host) => {
            crate::panels::add_sub(panels, host);
        }
        Op::RenameSub(i, name) => crate::panels::rename_sub(panels, i, &name),
        Op::Duplicate(i) => {
            // (A sub panel's copy gets a name of its own.)
            let copy = panels[i].clone();
            panels.push(copy);
            crate::panels::tidy(panels);
        }
        Op::RemovePanel(i) => crate::panels::remove(panels, i),
        Op::MovePanel(i, by) => {
            let j = (i as i32 + by).clamp(0, panels.len() as i32 - 1) as usize;
            crate::panels::swap(panels, i, j);
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
        Op::SetItem(p, z, k, id) => {
            if let Some(it) = panels[p].zones[z].get_mut(k) {
                *it = id;
            }
        }
        Op::RemoveItem(p, z, k) => {
            panels[p].zones[z].remove(k);
            panels[p].tidy_groups();
        }
        Op::Group(p, id, g) => panels[p].set_group(&id, g),
        Op::NewGroup(p, id) => {
            panels[p].groups.push(Group { bg: Some(Paint::Auto), ..Group::default() });
            let g = panels[p].groups.len() - 1;
            panels[p].set_group(&id, Some(g));
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
        // A new group from one item; a second joins it; taking both off the panel ends it.
        let mut ps = vec![PanelCfg::default()];
        apply(&mut ps, Op::NewGroup(0, "volume".into()));
        apply(&mut ps, Op::Group(0, "network".into(), Some(0)));
        assert_eq!(ps[0].groups[0].items, ["volume", "network"]);
        apply(&mut ps, Op::Group(0, "volume".into(), None));
        let k = ps[0].zones[2].iter().position(|i| i == "network").unwrap();
        apply(&mut ps, Op::RemoveItem(0, 2, k));
        assert!(ps[0].groups.is_empty());
    }
}
