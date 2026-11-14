//! The file picker's bar (when this window answers another app's Open / Save request):
//! a name field when saving, the file-type list, and Cancel / Open (or Save).

use std::path::PathBuf;

use eframe::egui::{self, Id, Key, Vec2};

use super::RivetApp;
use crate::model::Location;
use crate::picker::{self, Answer, Mode};

impl RivetApp {
    /// Draw the picker bar along the bottom of the window. Call before the other panels so it
    /// sits outermost.
    pub(super) fn picker_bar(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let Some(req) = picker::request() else { return };
        let fill = ctx.global_style().visuals.panel_fill;
        let mut accept = false;
        let mut cancel = false;
        egui::Panel::bottom("picker").show_separator_line(false).frame(egui::Frame::NONE.fill(fill).inner_margin(egui::Margin::symmetric(14, 10))).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let v = ui.visuals().clone();
                match req.mode {
                    Mode::Save => {
                        ui.label("Name");
                        let field_bg = crate::palette::mix(v.extreme_bg_color, v.text_color(), 0.05);
                        let out = egui::Frame::NONE
                            .fill(field_bg)
                            .corner_radius(super::r_sm())
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                egui::TextEdit::singleline(&mut self.pick_name)
                                    .frame(egui::Frame::NONE)
                                    .desired_width((ui.available_width() - 330.0).clamp(160.0, 420.0))
                                    .hint_text("File name")
                                    .show(ui)
                            })
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
                    }
                    _ => {
                        let text = self.picker_summary();
                        ui.label(egui::RichText::new(text).color(v.weak_text_color()));
                    }
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let label = req.accept_label();
                    let ready = self.picker_ready();
                    let c = v.selection.bg_fill;
                    let btn = egui::Button::new(egui::RichText::new(label).color(v.selection.stroke.color).strong()).fill(c).min_size(Vec2::new(84.0, 30.0));
                    if ui.add_enabled(ready, btn).clicked() {
                        accept = true;
                    }
                    if ui.add(egui::Button::new("Cancel").min_size(Vec2::new(76.0, 30.0))).clicked() {
                        cancel = true;
                    }
                    // The file-type list the app offered.
                    if !req.filters.is_empty() && !req.folders_only() {
                        let current = picker::chosen_filter();
                        let shown = current.and_then(|i| req.filters.get(i)).map(|f| f.name.clone()).unwrap_or_else(|| "All files".into());
                        let mut pick = current;
                        egui::ComboBox::from_id_salt("picker-type").selected_text(shown).width(180.0).show_ui(ui, |ui| {
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
