//! The User page: the person's picture and name, and the place the weather is for.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};

use eframe::egui::{self, Sense, Vec2};

use super::controls::{self, Icon};
use super::{SettingsApp, hint, section, weather_opts};

#[derive(Default)]
pub(super) struct State {
    /// The file picker, while it is open.
    picking: Option<Receiver<Result<Option<PathBuf>, String>>>,
    /// What went wrong with the last picture.
    said: Option<String>,
}

impl SettingsApp {
    pub(super) fn user_page(&mut self, ui: &mut egui::Ui, accent: egui::Color32) {
        let shot = self.shot.is_some();
        // The picker's answer, when it has one.
        if let Some(rx) = &self.user.picking {
            match rx.try_recv() {
                Ok(answer) => {
                    self.user.picking = None;
                    self.user.said = match answer {
                        Ok(Some(p)) => crate::user::set_face(&p).err().map(|e| format!("That picture couldn't be used: {e}")),
                        Ok(None) => None,
                        Err(e) => Some(format!("Couldn't open the file picker: {e}")),
                    };
                }
                Err(_) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
            }
        }
        let name = if shot { "Sam Rivers".to_string() } else { crate::sys::user_name() };
        section(ui, "You", &mut |ui| {
            ui.horizontal(|ui| {
                let (r, _) = ui.allocate_exact_size(Vec2::splat(88.0), Sense::hover());
                crate::user::draw(ui, r.center(), 88.0, &name, accent);
                ui.add_space(10.0);
                ui.vertical(|ui| {
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(&name).size(18.0).strong());
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if controls::button_if(ui, self.user.picking.is_none(), Icon::Picture, "Choose a Picture…").clicked() && !shot {
                            let (tx, rx) = channel();
                            std::thread::spawn(move || {
                                let start = std::env::var_os("HOME").map(PathBuf::from);
                                let _ = tx.send(rivet_xdg::portal::choose(rivet_xdg::portal::Pick::Picture, "Your Picture", start.as_deref()));
                            });
                            self.user.picking = Some(rx);
                        }
                        if !shot && crate::user::own_face() && controls::button(ui, Icon::Off, "Remove").clicked() {
                            self.user.said = crate::user::remove_face().err().map(|e| format!("The picture couldn't be removed: {e}"));
                        }
                    });
                });
            });
            ui.add_space(6.0);
            hint(ui, "Your picture shows on the lock screen, the login screen and the panel's User item.");
            if let Some(e) = &self.user.said {
                ui.add(egui::Label::new(egui::RichText::new(format!("{e}.")).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
            }
        });
        section(ui, "Weather", &mut |ui| {
            weather_opts::options(ui, &mut self.s, 360.0);
        });
    }
}
