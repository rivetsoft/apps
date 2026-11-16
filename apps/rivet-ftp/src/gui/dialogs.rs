//! The connection editor and the small questions: new folder, rename, delete, replace.

use eframe::egui::{self, Color32, Key, Sense, Stroke, Vec2};
use rivet_connections::Protocol;
use rivet_look::palette;
use rivet_look::theme::{paint_tex, r_lg, r_md, r_sm, status};

use super::*;

/// A filled accent button (the main choice in a dialog).
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    let v = ui.visuals().clone();
    ui.add(egui::Button::new(egui::RichText::new(text).color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked()
}

fn plain(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(egui::Button::new(text).corner_radius(r_sm())).clicked()
}

/// A small group heading inside a dialog.
fn heading(ui: &mut egui::Ui, text: &str) {
    ui.label(egui::RichText::new(text).strong().size(13.0).color(ui.visuals().weak_text_color()));
    ui.add_space(6.0);
}

/// A raised text field like Rivet's: a lifted surface, outlined in the accent while typing.
/// True when Enter was pressed in it.
fn raised(ui: &mut egui::Ui, id: &str, text: &mut String, hint: &str, width: f32, accent: Color32, focus: bool) -> bool {
    let v = ui.visuals().clone();
    let field_id = egui::Id::unique(("form-field", id));
    let focused = ui.memory(|m| m.has_focus(field_id));
    let fill = palette::mix(v.window_fill, v.text_color(), 0.06);
    let shadow = egui::Shadow { offset: [0, 1], blur: 6, spread: 0, color: Color32::from_black_alpha(if v.dark_mode { 70 } else { 30 }) };
    let stroke = Stroke::new(1.5, if focused { accent } else { Color32::TRANSPARENT });
    let resp = egui::Frame::NONE
        .fill(fill)
        .shadow(shadow)
        .stroke(stroke)
        .corner_radius(r_md())
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            // The margin (2 × 10) and the outline (2 × 1.5) are outside this width.
            ui.set_width(width - 23.0);
            ui.add(egui::TextEdit::singleline(text).id(field_id).frame(egui::Frame::NONE).hint_text(hint).min_size(Vec2::new(0.0, 22.0)).desired_width(f32::INFINITY))
        })
        .inner;
    if focus {
        resp.request_focus();
    }
    resp.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

/// A text field that has the keyboard; true when Enter was pressed in it.
fn field(ui: &mut egui::Ui, text: &mut String, hint: &str) -> bool {
    let r = ui.add(egui::TextEdit::singleline(text).hint_text(hint).desired_width(f32::INFINITY));
    if !r.has_focus() && !r.lost_focus() {
        r.request_focus();
    }
    r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
}

impl FtpApp {
    pub(super) fn dialogs(&mut self, ctx: &egui::Context) {
        self.form_dialog(ctx);
        self.prompt_dialog(ctx);
    }

    fn form_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut form) = self.form.take() else { return };
        let mut keep = true;
        let mut save: Option<bool> = None; // Some(connect afterwards)
        let icon = self.app_icon(ctx);
        let accent = ctx.global_style().visuals.selection.bg_fill;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("conn-form")).frame(frame).show(ctx, |ui| {
            ui.set_width(460.0);
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            let editing = form.original.is_some();
            // Icon, title, and what it's for.
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                if let Some(t) = &icon {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                    paint_tex(ui.painter(), t, r, Color32::WHITE);
                }
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(egui::RichText::new(if editing { "Edit Connection" } else { "New Connection" }).heading().color(v.strong_text_color()));
                    ui.label(egui::RichText::new("Saved for this window and for rivet-ftp in a terminal").color(weak));
                });
            });
            ui.add_space(18.0);

            let mut enter = false;
            let label_w = 104.0;
            let row = |ui: &mut egui::Ui, label: &str, body: &mut dyn FnMut(&mut egui::Ui)| {
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(label_w, 37.0), Sense::hover());
                    ui.painter().text(r.left_center(), egui::Align2::LEFT_CENTER, label, rivet_look::fonts::ui_font(13.5), v.text_color());
                    body(ui);
                });
                ui.add_space(6.0);
            };
            let first = !form.focused_once;
            form.focused_once = true;
            row(ui, "Name", &mut |ui| { let w = ui.available_width(); enter |= raised(ui, "f-name", &mut form.name, "Work site", w, accent, first) });
            ui.add_space(8.0);
            heading(ui, "Server");
            row(ui, "Type", &mut |ui| {
                ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
                for (p, label) in [(Protocol::Sftp, "SFTP"), (Protocol::Ftps, "FTPS"), (Protocol::Ftp, "FTP")] {
                    let tip = match p {
                        Protocol::Sftp => "Files over SSH: your keys and ssh agent",
                        Protocol::Ftps => "FTP over TLS (encrypted)",
                        Protocol::Ftp => "Plain FTP (nothing is encrypted)",
                    };
                    ui.selectable_value(&mut form.protocol, p, label).on_hover_text(tip);
                }
            });
            row(ui, "Address", &mut |ui| {
                let w = ui.available_width() - 92.0;
                enter |= raised(ui, "f-host", &mut form.host, "example.com", w, accent, false);
                ui.add_space(4.0);
                let port_hint = format!("Port {}", form.protocol.default_port());
                let w = ui.available_width();
                enter |= raised(ui, "f-port", &mut form.port, &port_hint, w, accent, false);
            });
            row(ui, "User", &mut |ui| { let w = ui.available_width(); enter |= raised(ui, "f-user", &mut form.user, "Your user name there", w, accent, false) });
            if form.protocol == Protocol::Sftp {
                row(ui, "SSH key", &mut |ui| { let w = ui.available_width(); enter |= raised(ui, "f-key", &mut form.key, "Your ssh agent or usual keys", w, accent, false) });
            }
            ui.add_space(8.0);
            heading(ui, "Folders");
            row(ui, "On the server", &mut |ui| { let w = ui.available_width(); enter |= raised(ui, "f-remote", &mut form.remote, "Your home folder there", w, accent, false) });
            row(ui, "On this computer", &mut |ui| { let w = ui.available_width(); enter |= raised(ui, "f-local", &mut form.local, "Your home folder", w, accent, false) });

            ui.add_space(6.0);
            let hint = if form.protocol == Protocol::Sftp {
                "Logs in with your SSH keys or ssh agent, like ssh does."
            } else {
                "FTP and FTPS browsing comes next; you can save this now. Passwords will be kept in your keyring."
            };
            ui.add(egui::Label::new(egui::RichText::new(hint).size(12.0).color(weak)).wrap());
            if let Some(e) = &form.error {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(e).color(status(ctx).danger));
            }
            ui.add_space(18.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.spacing_mut().button_padding = Vec2::new(14.0, 6.0);
                if primary(ui, "Save and Connect") || enter {
                    save = Some(true);
                }
                if plain(ui, "Save") {
                    save = Some(false);
                }
                if plain(ui, "Cancel") || ui.input(|i| i.key_pressed(Key::Escape)) {
                    keep = false;
                }
            });
        });
        if let Some(connect) = save {
            match form.build() {
                Err(e) => form.error = Some(e),
                Ok(c) => {
                    let clash = self.store.find(&c.name).is_some_and(|o| Some(&o.name) != form.original.as_ref());
                    if clash {
                        form.error = Some(format!("There's already a connection called \"{}\".", c.name));
                    } else {
                        let backup = self.store.items.clone();
                        if let Some(old) = &form.original {
                            self.store.remove(old);
                        }
                        let saved = self.store.add(c.clone(), true).and_then(|_| self.store.save().map_err(|e| format!("Couldn't save: {e}")));
                        match saved {
                            Ok(()) => {
                                keep = false;
                                self.note = Some((format!("Saved \"{}\".", c.name), false));
                                if connect {
                                    self.connect(c, ctx);
                                } else if self.conn.as_ref().is_some_and(|k| k.host == c.host && self.store.find(&k.name).is_none()) {
                                    // The quick connection now has a name.
                                    self.conn = Some(c);
                                }
                            }
                            Err(e) => {
                                self.store.items = backup;
                                form.error = Some(e);
                            }
                        }
                    }
                }
            }
        }
        if keep {
            self.form = Some(form);
        }
    }

    fn prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some(prompt) = self.prompt.take() else { return };
        let mut next: Option<Prompt> = None;
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(22)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(400.0);
            let esc = ui.input(|i| i.key_pressed(Key::Escape));
            match prompt {
                Prompt::NewFolder(side, mut name) => {
                    ui.heading("New folder");
                    ui.add_space(8.0);
                    let enter = field(ui, &mut name, "Folder name");
                    ui.add_space(10.0);
                    let (mut ok, mut cancel) = (false, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ok = primary(ui, "Create");
                        cancel = plain(ui, "Cancel");
                    });
                    if (ok || enter) && !name.trim().is_empty() {
                        self.new_folder(side, &name);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::NewFolder(side, name));
                    }
                }
                Prompt::Rename(side, old, mut name) => {
                    ui.heading("Rename");
                    ui.add_space(8.0);
                    let enter = field(ui, &mut name, "New name");
                    ui.add_space(10.0);
                    let (mut ok, mut cancel) = (false, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ok = primary(ui, "Rename");
                        cancel = plain(ui, "Cancel");
                    });
                    if ok || enter {
                        self.rename(side, &old, &name);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::Rename(side, old, name));
                    }
                }
                Prompt::Delete(items) => {
                    let what = if items.len() == 1 { format!("\"{}\"", items[0].name) } else { format!("{} items", items.len()) };
                    ui.heading(format!("Delete {what} from the server?"));
                    ui.add_space(6.0);
                    ui.label("This can't be undone. Folders are deleted with everything in them.");
                    ui.add_space(10.0);
                    let (mut ok, mut cancel) = (false, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ok = ui.add(egui::Button::new(egui::RichText::new("Delete").color(rivet_look::theme::StatusColors::on(status(ctx).danger))).fill(status(ctx).danger).corner_radius(r_sm())).clicked();
                        cancel = plain(ui, "Cancel");
                    });
                    if ok {
                        self.delete_remote(items);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::Delete(items));
                    }
                }
                Prompt::Replace(p) => {
                    let n = p.clashes.len();
                    let place = if p.dir == Dir::Up { "on the server" } else { "on this computer" };
                    ui.heading(if n == 1 { format!("\"{}\" is already {place}", p.clashes[0]) } else { format!("{n} items are already {place}") });
                    ui.add_space(6.0);
                    ui.label("Replace them with the ones you're sending?");
                    ui.add_space(10.0);
                    let (mut replace, mut skip, mut cancel) = (false, false, false);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        replace = primary(ui, "Replace");
                        if p.items.len() > n {
                            skip = plain(ui, "Skip those");
                        }
                        cancel = plain(ui, "Cancel");
                    });
                    if replace || skip {
                        self.enqueue(p, replace);
                    } else if !(cancel || esc) {
                        next = Some(Prompt::Replace(p));
                    }
                }
            }
        });
        if next.is_some() {
            self.prompt = next;
        }
    }
}
