//! Applications, Terminals and editors: the programs that take the theme's colours through a
//! file of Rivet's in their settings folder (`themed.rs`), a switch each.

use eframe::egui;

use super::controls::{self, Icon};
use super::{SettingsApp, hint, section};
use crate::themed::{self, Hook, Reload};

impl SettingsApp {
    pub(super) fn themed_apps_section(&mut self, ui: &mut egui::Ui) {
        // Screenshots show every one, as if all were installed.
        let shot = self.shot.is_some();
        let here: Vec<&themed::App> = themed::APPS.iter().filter(|a| shot || a.installed()).collect();
        section(ui, "Terminals and editors", &mut |ui| {
            if here.is_empty() {
                hint(ui, "kitty, Ghostty, Alacritty, foot, Zed and btop can take the theme's colors. None of them is installed.");
                return;
            }
            hint(ui, "Each one turned on changes with the wallpaper.");
            ui.add_space(4.0);
            for a in &here {
                let mut on = self.s.themed_apps.iter().any(|id| id == a.id);
                if controls::switch(ui, &mut on, a.name).changed() {
                    self.s.themed_apps.retain(|id| id != a.id);
                    if on {
                        self.s.themed_apps.push(a.id.to_string());
                    }
                }
                if !on {
                    continue;
                }
                // What is still to do before it shows, and when a running one catches up.
                let later = match a.reload {
                    Reload::Later => " It shows in windows opened after a change.",
                    _ => "",
                };
                ui.indent(("themed", a.id), |ui| match a.hook {
                    Hook::Tell(what) => hint(ui, &format!("{what}{later}")),
                    Hook::Line { file, line } | Hook::FirstLine { file, line } => {
                        if !shot && a.hooked() {
                            hint(ui, later.trim());
                            return;
                        }
                        ui.horizontal(|ui| {
                            if controls::button(ui, Icon::Plus, "Add the Line").on_hover_text(format!("Adds \"{line}\" to {file}. Everything else in the file stays.")).clicked() && !shot {
                                self.note = a.hook_up().err().map(|e| format!("Couldn't add the line to {file}: {e}"));
                            }
                            hint(ui, &format!("{} reads the colors once {file} has the line \"{line}\".{later}", a.name));
                        });
                    }
                });
            }
            // Only this card's own trouble: the note is shared with other pages.
            if let Some(n) = self.note.as_ref().filter(|n| n.starts_with("Couldn't add the line")) {
                ui.add_space(4.0);
                ui.add(egui::Label::new(egui::RichText::new(format!("{n}.")).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
            }
        });
    }
}
