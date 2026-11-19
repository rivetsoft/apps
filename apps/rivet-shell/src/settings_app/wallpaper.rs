//! Settings' wallpaper card: the picture showing (for every screen, or one), and a popover with
//! the chooser (`wall_pick`) to change it from a folder, or a picture from anywhere.

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use eframe::egui::{self, Align2, Color32, Sense, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md};

use super::{SettingsApp, hint, section};
use crate::portal::Pick;
use crate::wall;

/// Recent folders kept.
const RECENT: usize = 6;

impl SettingsApp {
    /// Use `dir` as the wallpaper folder (and remember it).
    fn set_wall_dir(&mut self, dir: &Path) {
        let d = dir.to_string_lossy().into_owned();
        self.s.wallpaper_dir = d.clone();
        if dir.is_dir() {
            self.s.wallpaper_dirs.retain(|x| *x != d);
            self.s.wallpaper_dirs.insert(0, d);
            self.s.wallpaper_dirs.truncate(RECENT);
        }
    }

    /// Opens the desktop's picker on a thread.
    fn ask_picker(&mut self, kind: Pick) {
        if self.picking.is_some() || self.shot.is_some() {
            return;
        }
        let start = PathBuf::from(&self.s.wallpaper_dir);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let title = if kind == Pick::Folder { "Wallpaper Folder" } else { "Wallpaper" };
            let _ = tx.send((kind, crate::portal::choose(kind, title, Some(&start))));
        });
        self.picking = Some(rx);
    }

    fn take_pick(&mut self, ctx: &egui::Context) {
        let Some(rx) = &self.picking else { return };
        let Ok((kind, answer)) = rx.try_recv() else {
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
            return;
        };
        self.picking = None;
        match answer {
            Ok(Some(p)) if kind == Pick::Folder => self.set_wall_dir(&p),
            Ok(Some(p)) => self.set_wall(p.to_string_lossy().into_owned()),
            Ok(None) => {}
            Err(e) => self.note = Some(format!("Couldn't open the file picker: {e}")),
        }
    }

    /// The screens a picture can be chosen for: the ones connected, and any with a picture of
    /// their own that aren't just now.
    pub(super) fn wall_screens(&self) -> Vec<String> {
        let mut names = self.screens.clone();
        names.extend(self.s.wallpaper_screens.iter().map(|(n, _)| n.clone()).filter(|n| !self.screens.contains(n)));
        names
    }

    /// The picture of the screen being chosen for.
    fn wall_now(&self) -> String {
        self.s.wallpaper_on(&self.wall_screen).to_string()
    }

    fn set_wall(&mut self, picture: String) {
        if self.wall_screen.is_empty() {
            self.s.wallpaper = picture;
        } else {
            let name = self.wall_screen.clone();
            self.s.set_wallpaper_on(&name, &picture);
        }
    }

    /// Which screen the pictures below are for (only with more than one screen).
    fn screen_row(&mut self, ui: &mut egui::Ui) {
        let names = self.wall_screens();
        if names.len() < 2 && self.s.wallpaper_screens.is_empty() {
            self.wall_screen.clear();
            return;
        }
        let mut at = names.iter().position(|n| *n == self.wall_screen).map(|i| i + 1).unwrap_or(0);
        let labels: Vec<String> = names.iter().map(|n| if self.s.has_own_wallpaper(n) { format!("{n} •") } else { n.clone() }).collect();
        let mut choices: Vec<(usize, &str)> = vec![(0, "All Screens")];
        choices.extend(labels.iter().enumerate().map(|(i, l)| (i + 1, l.as_str())));
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            super::panel_editor::segmented(ui, &mut at, &choices);
            self.wall_screen = if at == 0 { String::new() } else { names[at - 1].clone() };
            if !self.wall_screen.is_empty() && self.s.has_own_wallpaper(&self.wall_screen) && ui.button("Use the Shared Picture").on_hover_text("Show the shared wallpaper on this screen").clicked() {
                let name = self.wall_screen.clone();
                self.s.set_wallpaper_on(&name, "");
            }
        });
        ui.add_space(4.0);
        let text = if self.wall_screen.is_empty() {
            "Shows on every screen that doesn't have its own picture (marked •).".to_string()
        } else if self.s.has_own_wallpaper(&self.wall_screen) {
            format!("{} has its own picture.", self.wall_screen)
        } else {
            format!("{} shows the shared picture. Change it to give it its own.", self.wall_screen)
        };
        hint(ui, &text);
        ui.add_space(10.0);
    }

    pub(super) fn picture_section(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let ctx = ui.ctx().clone();
        self.take_pick(&ctx);
        let dir = PathBuf::from(&self.s.wallpaper_dir);
        self.picker.frame(&ctx, &dir);
        section(ui, "Wallpaper", &mut |ui| {
            self.screen_row(ui);
            let now = self.wall_now();
            let v = ui.visuals().clone();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 18.0;
                // The picture showing.
                let (r, _) = ui.allocate_exact_size(Vec2::new(232.0, 144.0), Sense::hover());
                match (!now.is_empty()).then(|| self.picker.thumb_of(Path::new(&now))).flatten() {
                    Some(t) => crate::wall_pick::cover(ui.painter(), r, &t, r_md()),
                    None => {
                        ui.painter().rect_filled(r, r_md(), mix(v.extreme_bg_color, v.text_color(), 0.04));
                        let text = if now.is_empty() { "No wallpaper" } else if Path::new(&now).is_file() { "" } else { "Picture not found" };
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, text, egui::FontId::proportional(12.5), v.weak_text_color());
                    }
                }
                ui.vertical(|ui| {
                    ui.add_space(6.0);
                    let path = PathBuf::from(&now);
                    let name = if now.is_empty() { "A plain background".to_string() } else { path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default() };
                    ui.add(egui::Label::new(egui::RichText::new(name).strong().size(14.5)).truncate());
                    let place = if now.is_empty() { "In the theme's colors".to_string() } else { path.parent().map(wall::short).unwrap_or_default() };
                    ui.add(egui::Label::new(egui::RichText::new(place).small().color(v.weak_text_color())).truncate());
                    ui.add_space(14.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
                        let change = ui.button("Change…").on_hover_text("Pictures from the wallpaper folder");
                        self.chooser(ui, &change, &dir, &now, accent);
                        if ui.add_enabled(self.picking.is_none(), egui::Button::new("Choose a File…")).on_hover_text("A picture from anywhere, with the file picker").clicked() {
                            self.ask_picker(Pick::Picture);
                        }
                        if self.wall_screen.is_empty() && !self.s.wallpaper.is_empty() && ui.button("No Wallpaper").on_hover_text("A plain background in the theme's colors").clicked() {
                            self.s.wallpaper.clear();
                        }
                    });
                });
            });
        });
    }

    /// The popover under "Change…": the folder and its pictures.
    fn chooser(&mut self, ui: &mut egui::Ui, button: &egui::Response, dir: &Path, now: &str, accent: Color32) {
        let width = 580.0f32.min(ui.ctx().content_rect().width() - 40.0);
        let mut popup = egui::Popup::from_toggle_button_response(button)
            .layout(egui::Layout::top_down(egui::Align::Min))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .gap(6.0)
            .width(width)
            .frame(egui::Frame::popup(ui.style()).corner_radius(r_lg()).inner_margin(egui::Margin::same(16)));
        // Checks: RIVET_SHELL_WALL_PICK=1 scrolls to it and opens it (=0 only scrolls).
        if self.shot.is_some() && let Ok(v) = std::env::var("RIVET_SHELL_WALL_PICK") {
            ui.scroll_to_rect(button.rect.expand(140.0), Some(egui::Align::Center));
            if v == "1" {
                popup = popup.open_memory(Some(egui::SetOpenCommand::Bool(true)));
            }
        }
        let recent = self.s.wallpaper_dirs.clone();
        let can_ask = self.picking.is_none() && self.shot.is_none();
        let out = popup.show(|ui| {
            ui.set_width(width);
            self.picker.ui(ui, dir, &recent, now, accent, can_ask, 360.0)
        });
        let Some(out) = out.map(|r| r.inner) else { return };
        if let Some(kind) = out.ask {
            self.ask_picker(kind);
        }
        if let Some(d) = out.dir {
            self.set_wall_dir(&d);
        }
        if let Some(p) = out.picture {
            self.set_wall(p.to_string_lossy().into_owned());
        }
    }
}
