//! The connection editor and the small questions: new folder, rename, delete, replace.

use eframe::egui::{self, Key};
use rivet_connections::Protocol;
use rivet_look::theme::{r_sm, status};

use super::*;

/// A filled accent button (the main choice in a dialog).
fn primary(ui: &mut egui::Ui, text: &str) -> bool {
    let v = ui.visuals().clone();
    ui.add(egui::Button::new(egui::RichText::new(text).color(v.selection.stroke.color)).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked()
}

fn plain(ui: &mut egui::Ui, text: &str) -> bool {
    ui.add(egui::Button::new(text).corner_radius(r_sm())).clicked()
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
        egui::Modal::new(egui::Id::unique("conn-form")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.heading(if form.original.is_some() { "Edit connection" } else { "New connection" });
            ui.add_space(10.0);
            egui::Grid::new("form-grid").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label("Name");
                ui.add(egui::TextEdit::singleline(&mut form.name).hint_text("Work site").desired_width(300.0));
                ui.end_row();
                ui.label("Type");
                let label = |p: Protocol| match p {
                    Protocol::Sftp => "SFTP (SSH)",
                    Protocol::Ftps => "FTPS (FTP over TLS)",
                    Protocol::Ftp => "FTP",
                };
                egui::ComboBox::from_id_salt("proto").width(300.0).selected_text(label(form.protocol)).show_ui(ui, |ui| {
                    for p in [Protocol::Sftp, Protocol::Ftps, Protocol::Ftp] {
                        ui.selectable_value(&mut form.protocol, p, label(p));
                    }
                });
                ui.end_row();
                ui.label("Server");
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut form.host).hint_text("example.com").desired_width(226.0));
                    ui.add(egui::TextEdit::singleline(&mut form.port).hint_text(format!("port {}", form.protocol.default_port())).desired_width(66.0));
                });
                ui.end_row();
                ui.label("User");
                ui.add(egui::TextEdit::singleline(&mut form.user).desired_width(300.0));
                ui.end_row();
                if form.protocol == Protocol::Sftp {
                    ui.label("SSH key");
                    ui.add(egui::TextEdit::singleline(&mut form.key).hint_text("Your ssh agent or usual keys").desired_width(300.0));
                    ui.end_row();
                }
                ui.label("Server folder");
                ui.add(egui::TextEdit::singleline(&mut form.remote).hint_text("Your home folder there").desired_width(300.0));
                ui.end_row();
                ui.label("Local folder");
                ui.add(egui::TextEdit::singleline(&mut form.local).hint_text("Your home folder").desired_width(300.0));
                ui.end_row();
            });
            ui.add_space(6.0);
            let hint = if form.protocol == Protocol::Sftp {
                "Logs in with your SSH keys or ssh agent. From a terminal, use rivet-ftp ssh with the connection's name."
            } else {
                "FTP and FTPS browsing comes next; the connection can be saved now. Passwords will be kept in your keyring."
            };
            ui.label(egui::RichText::new(hint).small().weak());
            if let Some(e) = &form.error {
                ui.add_space(4.0);
                ui.label(egui::RichText::new(e).color(status(ctx).danger));
            }
            ui.add_space(10.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if primary(ui, "Save and connect") {
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
        egui::Modal::new(egui::Id::unique("prompt")).show(ctx, |ui| {
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
