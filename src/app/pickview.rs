//! The file picker (when this window answers another app's Open / Save request), laid out like
//! GNOME's: a header bar with Cancel, the title and the Open / Save button; a name field under it
//! when saving; the file-type list along the bottom when the app offers one.

use std::path::PathBuf;

use eframe::egui::{self, Align2, Id, Key, Pos2, Sense, Vec2};

use super::{Action, RivetApp};
use crate::model::{Entry, Location};
use crate::picker::{self, Answer, Mode};

impl RivetApp {
    /// The asking app's name ("Firefox"), from its launcher entry.
    fn picker_app_name(&self) -> Option<String> {
        let id = picker::request()?.app.trim().to_string();
        if id.is_empty() {
            return None;
        }
        let bare = |s: &str| s.trim_end_matches(".desktop").to_lowercase();
        self.apps.iter().find(|a| bare(&a.id) == bare(&id)).map(|a| a.name.clone())
    }

    /// Header bar: Cancel · title (and the asking app) · Open / Save. Then the name field when
    /// saving. Call right after the window header so these sit at the very top.
    pub(super) fn picker_header(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(req) = picker::request() else { return };
        let fill = ctx.global_style().visuals.panel_fill;
        let app = self.picker_app_name();
        let mut accept = false;
        let mut cancel = false;
        egui::Panel::top("picker-head").show_separator_line(false).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(12, 8))).show(ui, |ui| {
            let v = ui.visuals().clone();
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::hover());
            // The title in the middle of the bar, the app below it in small type.
            let title = req.window_title();
            let painter = ui.painter();
            match &app {
                Some(a) => {
                    painter.text(Pos2::new(rect.center().x, rect.center().y - 7.0), Align2::CENTER_CENTER, &title, crate::fonts::ui_font(14.5), v.strong_text_color());
                    painter.text(Pos2::new(rect.center().x, rect.center().y + 9.0), Align2::CENTER_CENTER, a, crate::fonts::ui_font(11.5), v.weak_text_color());
                }
                None => {
                    painter.text(rect.center(), Align2::CENTER_CENTER, &title, crate::fonts::ui_font(14.5), v.strong_text_color());
                }
            }
            let mut left = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
            if left.add(egui::Button::new("Cancel").min_size(Vec2::new(84.0, 32.0))).on_hover_text("Esc").clicked() {
                cancel = true;
            }
            let mut right = ui.new_child(egui::UiBuilder::new().max_rect(rect).layout(egui::Layout::right_to_left(egui::Align::Center)));
            let c = v.selection.bg_fill;
            let btn = egui::Button::new(egui::RichText::new(req.accept_label()).color(v.selection.stroke.color).strong()).fill(c).min_size(Vec2::new(84.0, 32.0));
            if right.add_enabled(self.picker_ready(), btn).clicked() {
                accept = true;
            }
        });
        if req.mode == Mode::Save {
            egui::Panel::top("picker-name").show_separator_line(false).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin { left: 12, right: 12, top: 0, bottom: 8 })).show(ui, |ui| {
                let v = ui.visuals().clone();
                let w = ui.available_width().min(520.0);
                let full = ui.available_width();
                ui.horizontal(|ui| {
                    ui.add_space((full - w) / 2.0);
                    ui.allocate_ui_with_layout(Vec2::new(w, 34.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new("Name").color(v.weak_text_color()));
                        let field_bg = crate::palette::mix(v.extreme_bg_color, v.text_color(), 0.05);
                        let out = egui::Frame::NONE
                            .fill(field_bg)
                            .corner_radius(super::r_md())
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| egui::TextEdit::singleline(&mut self.pick_name).frame(egui::Frame::NONE).desired_width(f32::INFINITY).hint_text("File name").show(ui))
                            .inner;
                        // First time: focus the name with the part before the extension selected.
                        if self.pick_focus {
                            self.pick_focus = false;
                            out.response.request_focus();
                            let stem = self.pick_name.rfind('.').filter(|&i| i > 0).unwrap_or(self.pick_name.len());
                            let mut state = out.state.clone();
                            let stem_chars = self.pick_name[..stem].chars().count();
                            state.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(stem_chars))));
                            state.store(ui.ctx(), out.response.id);
                        }
                        if out.response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                            accept = true;
                        }
                    });
                });
            });
        }
        // Esc closes the picker when there's nothing else for it to do.
        let idle = matches!(self.dialog, super::Dialog::None)
            && !self.search_open
            && !self.path_editing
            && self.tab().selected.is_empty()
            && self.pick_confirm.is_none()
            && self.spotlight.is_none()
            && self.actions.is_none()
            && !ctx.egui_wants_keyboard_input();
        if idle && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Escape)) {
            cancel = true;
        }
        if cancel {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if accept {
            self.picker_accept(ctx);
        }
        self.picker_confirm(ctx);
    }

    /// The bottom strip: what's chosen, and the app's file types. Call before the other panels
    /// so it sits outermost.
    pub(super) fn picker_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(req) = picker::request() else { return };
        let fill = ctx.global_style().visuals.panel_fill;
        egui::Panel::bottom("picker").show_separator_line(false).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(14, 8))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.set_min_height(28.0);
                let v = ui.visuals().clone();
                let text = self.picker_summary();
                ui.label(egui::RichText::new(text).color(v.weak_text_color()));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // The file-type list the app offered.
                    if !req.filters.is_empty() && !req.folders_only() {
                        let current = picker::chosen_filter();
                        let shown = current.and_then(|i| req.filters.get(i)).map(|f| f.name.clone()).unwrap_or_else(|| "All files".into());
                        let mut pick = current;
                        egui::ComboBox::from_id_salt("picker-type").selected_text(shown).width(200.0).show_ui(ui, |ui| {
                            for (i, f) in req.filters.iter().enumerate() {
                                ui.selectable_value(&mut pick, Some(i), f.name.as_str());
                            }
                        });
                        if pick != current {
                            picker::set_chosen_filter(pick);
                            self.resort_all();
                        }
                    }
                });
            });
        });
    }

    /// Right-click on an item while picking: choose it, and a few harmless extras.
    pub(super) fn picker_item_menu(&mut self, ui: &mut egui::Ui, e: &Entry) {
        if !self.tab().selected.contains(&e.path) {
            let t = self.tab_mut();
            t.selected.clear();
            t.selected.insert(e.path.clone());
        }
        let Some(req) = picker::request() else { return };
        if e.is_dir {
            if ui.button("Open Folder").clicked() {
                self.pending.push(Action::Navigate(Location::Dir(e.path.clone())));
            }
        } else if req.mode == Mode::Open && !req.directory {
            if ui.button(req.accept_label()).clicked() {
                self.pending.push(Action::Open(e.path.clone()));
            }
        } else if req.mode == Mode::Save && ui.button("Use This Name").clicked() {
            self.pick_name = e.name.clone();
        }
        if ui.button("Preview").clicked() {
            self.pending.push(Action::QuickView(e.path.clone()));
        }
        ui.separator();
        if ui.button("Rename…").clicked() {
            self.pending.push(Action::Rename(e.path.clone()));
        }
        if ui.button("Copy Location").clicked() {
            self.pending.push(Action::CopyLocation);
        }
        ui.separator();
        self.picker_hidden_toggle(ui);
    }

    /// Right-click on empty space while picking.
    pub(super) fn picker_background_menu(&mut self, ui: &mut egui::Ui) {
        if self.current_dir().is_some() && ui.button("New Folder").clicked() {
            self.pending.push(Action::NewFolder);
        }
        self.picker_hidden_toggle(ui);
    }

    fn picker_hidden_toggle(&mut self, ui: &mut egui::Ui) {
        if Self::menu_toggle(ui, &mut self.s.show_hidden, "Show Hidden Files", "Ctrl+H") {
            self.resort_all();
        }
    }

    /// "2 files selected", "Choose a folder", …
    fn picker_summary(&self) -> String {
        let Some(req) = picker::request() else { return String::new() };
        let sel = self.tab().selected_entries();
        if req.folders_only() {
            let folder = match sel.iter().find(|e| e.is_dir) {
                Some(e) => e.name.clone(),
                None => self.current_dir().and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "/".into()),
            };
            return match req.mode {
                Mode::SaveFiles => format!("Save {} file{} into “{folder}”", req.files.len(), if req.files.len() == 1 { "" } else { "s" }),
                _ => format!("Folder: {folder}"),
            };
        }
        if req.mode == Mode::Save {
            let here = self.current_dir().and_then(|d| d.file_name().map(|n| n.to_string_lossy().to_string())).unwrap_or_else(|| "/".into());
            return format!("Saving in “{here}”");
        }
        let files = sel.iter().filter(|e| !e.is_dir).count();
        match files {
            0 if req.multiple => "Choose one or more files".into(),
            0 => "Choose a file".into(),
            1 => format!("“{}”", sel.iter().find(|e| !e.is_dir).map(|e| e.name.as_str()).unwrap_or("")),
            n => format!("{n} files selected"),
        }
    }

    fn picker_ready(&self) -> bool {
        let Some(req) = picker::request() else { return false };
        match req.mode {
            Mode::Save => {
                let n = self.pick_name.trim();
                !n.is_empty() && !n.contains('/') && self.current_dir().is_some()
            }
            _ if req.folders_only() => self.current_dir().is_some() || self.tab().selected_entries().iter().any(|e| e.is_dir),
            _ => self.tab().selected_entries().iter().any(|e| !e.is_dir),
        }
    }

    /// Double-click / Enter on an item while picking.
    pub(super) fn picker_open(&mut self, ctx: &egui::Context, p: &std::path::Path) {
        let Some(req) = picker::request() else { return };
        if p.is_dir() {
            self.navigate(Location::Dir(p.to_path_buf()));
            return;
        }
        match req.mode {
            Mode::Save => {
                if let Some(n) = p.file_name() {
                    self.pick_name = n.to_string_lossy().to_string();
                }
            }
            Mode::Open if !req.directory => self.picker_accept(ctx),
            _ => {}
        }
    }

    fn picker_accept(&mut self, ctx: &egui::Context) {
        let Some(req) = picker::request() else { return };
        if !self.picker_ready() || self.picked {
            return;
        }
        let here = self.current_dir();
        let sel = self.tab().selected_entries();
        match req.mode {
            Mode::Open if !req.directory => {
                let mut files: Vec<PathBuf> = sel.iter().filter(|e| !e.is_dir).map(|e| e.path.clone()).collect();
                if !req.multiple {
                    files.truncate(1);
                }
                self.picker_done(ctx, files);
            }
            Mode::Open => {
                let dirs: Vec<&crate::model::Entry> = sel.iter().filter(|e| e.is_dir).copied().collect();
                let chosen = match dirs.as_slice() {
                    [one] => vec![one.path.clone()],
                    many if req.multiple && !many.is_empty() => many.iter().map(|e| e.path.clone()).collect(),
                    _ => here.into_iter().collect(),
                };
                self.picker_done(ctx, chosen);
            }
            Mode::Save => {
                let Some(dir) = here else { return };
                let target = dir.join(self.pick_name.trim());
                if target.is_dir() {
                    // A folder's name: go into it instead.
                    self.navigate(Location::Dir(target));
                    self.pick_name.clear();
                } else if target.exists() {
                    self.pick_confirm = Some(vec![target]);
                } else {
                    self.picker_done(ctx, vec![target]);
                }
            }
            Mode::SaveFiles => {
                let dir = match sel.iter().find(|e| e.is_dir) {
                    Some(e) if sel.len() == 1 => e.path.clone(),
                    _ => match here {
                        Some(d) => d,
                        None => return,
                    },
                };
                let clash: Vec<PathBuf> = req.files.iter().map(|n| dir.join(n)).filter(|p| p.exists()).collect();
                if clash.is_empty() {
                    self.picker_done(ctx, vec![dir]);
                } else {
                    self.pick_confirm = Some(std::iter::once(dir).chain(clash).collect());
                }
            }
        }
    }

    /// "Replace?" before saving over existing files.
    fn picker_confirm(&mut self, ctx: &egui::Context) {
        let Some(paths) = self.pick_confirm.clone() else { return };
        let Some(req) = picker::request() else { return };
        // SaveFiles keeps the folder first, then the files that already exist.
        let (answer, clash): (Vec<PathBuf>, Vec<PathBuf>) = match req.mode {
            Mode::SaveFiles => (paths[..1].to_vec(), paths[1..].to_vec()),
            _ => (paths.clone(), paths.clone()),
        };
        let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let (title, body) = if clash.len() == 1 {
            (format!("Replace “{}”?", name(&clash[0])), "A file with this name already exists here. Replacing it overwrites what's in it.".to_string())
        } else {
            (format!("Replace {} files?", clash.len()), format!("{} and others already exist here. Replacing them overwrites what's in them.", clash.iter().take(3).map(name).collect::<Vec<_>>().join(", ")))
        };
        let mut go = false;
        let mut back = false;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(22)).corner_radius(super::r_lg());
        let r = egui::Modal::new(Id::unique("picker-replace")).frame(frame).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(egui::RichText::new(title).heading());
            ui.add_space(8.0);
            ui.add(egui::Label::new(body).wrap());
            ui.add_space(16.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let danger = super::status(ui.ctx()).danger;
                    if ui.add(egui::Button::new(egui::RichText::new("Replace").color(super::StatusColors::on(danger))).fill(danger)).clicked() {
                        go = true;
                    }
                    if ui.button("Cancel").clicked() {
                        back = true;
                    }
                });
            });
        });
        if r.should_close() {
            back = true;
        }
        if go {
            self.pick_confirm = None;
            self.picker_done(ctx, answer);
        } else if back {
            self.pick_confirm = None;
        }
    }

    fn picker_done(&mut self, ctx: &egui::Context, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if let Some(d) = self.current_dir() {
            picker::remember_folder(&d);
        }
        self.picked = true;
        picker::finish(Answer { paths, filter: picker::chosen_filter() });
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}
