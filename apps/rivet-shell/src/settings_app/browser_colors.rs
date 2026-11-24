//! Applications, Browser colors: Chromium, Chrome, Brave and Edge take the theme's colour.
//! What it does and the one-time password step are `crate::browser`'s.

use std::sync::Mutex;

use eframe::egui;

use super::controls::{self, Icon};
use super::{SettingsApp, grid, hint, panel_editor, section};

/// The one-time step, while it runs and how it ended.
#[derive(Default)]
struct Asking {
    running: bool,
    /// Why it didn't work, when it didn't.
    said: Option<String>,
}

static ASKING: Mutex<Asking> = Mutex::new(Asking { running: false, said: None });

impl SettingsApp {
    pub(super) fn browser_colors_section(&mut self, ui: &mut egui::Ui) {
        let checking = crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed);
        let ctx = ui.ctx().clone();
        // Which browsers are here and whether Rivet may colour them: looked at every two
        // seconds (the path and a file), and never in checks, which show one that is ready.
        let key = egui::Id::unique("browser-colors-found");
        let now = ctx.input(|i| i.time);
        let (names, ready) = match ctx.data(|d| d.get_temp::<(f64, Vec<&'static str>, bool)>(key)) {
            Some((at, names, ready)) if now - at < 2.0 => (names, ready),
            _ => {
                let (names, ready) = if checking {
                    (vec!["Chromium", "Brave"], std::env::var("RIVET_SHELL_BROWSER").as_deref() != Ok("ask"))
                } else {
                    let all = crate::browser::found();
                    let mut names: Vec<&'static str> = all.iter().map(|b| b.name).collect();
                    names.dedup();
                    (names, !all.is_empty() && all.iter().all(|b| b.ready()))
                };
                ctx.data_mut(|d| d.insert_temp(key, (now, names.clone(), ready)));
                (names, ready)
            }
        };
        // Checks: RIVET_SHELL_BROWSER=1 shows it turned on, =ask before the password was given.
        if checking && std::env::var_os("RIVET_SHELL_BROWSER").is_some() {
            self.s.browser_colors = true;
        }
        section(ui, "Browser colors", &mut |ui| {
            if names.is_empty() {
                hint(ui, "Chromium, Chrome, Brave and Edge can take the theme's color. None of them is installed.");
                return;
            }
            controls::switch(ui, &mut self.s.browser_colors, format!("{} in the theme's color", list(&names)));
            hint(ui, "The browser changes with the wallpaper, and its menu says it is managed by your organization.");
            if !self.s.browser_colors {
                return;
            }
            ui.add_space(8.0);
            if !ready {
                let mut asking = ASKING.lock().unwrap_or_else(|e| e.into_inner());
                ui.horizontal(|ui| {
                    if controls::button_if(ui, !asking.running, Icon::Check, "Allow").clicked() {
                        asking.running = true;
                        asking.said = None;
                        let (s, ctx) = (self.s.clone(), ctx.clone());
                        std::thread::spawn(move || {
                            let done = crate::browser::allow();
                            if done.is_ok() {
                                crate::browser::apply(&s);
                            }
                            let mut asking = ASKING.lock().unwrap_or_else(|e| e.into_inner());
                            asking.running = false;
                            asking.said = done.err();
                            // Look again now, not in two seconds.
                            ctx.data_mut(|d| d.remove::<(f64, Vec<&'static str>, bool)>(key));
                            ctx.request_repaint();
                        });
                    }
                    ui.add(egui::Label::new("Rivet asks for your password once, to be allowed to set the browser's color.").wrap());
                });
                if let Some(why) = &asking.said {
                    ui.add(egui::Label::new(egui::RichText::new(format!("That didn't work: {why}.")).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
                }
                return;
            }
            grid(ui, "shell-browser-colors", &mut |ui| {
                ui.label("Color");
                panel_editor::segmented(ui, &mut self.s.browser_accent, &[(false, "Windows"), (true, "Accent")]);
                ui.end_row();
                if self.s.browser_accent {
                    ui.label("Shade");
                    let words = |v: f64, _: std::ops::RangeInclusive<usize>| match v as i32 {
                        0 => "As it is".to_string(),
                        v if v < 0 => format!("{}% darker", -v),
                        v => format!("{v}% lighter"),
                    };
                    ui.add(egui::Slider::new(&mut self.s.browser_accent_shade, -100..=100).step_by(5.0).custom_formatter(words));
                } else {
                    ui.label("Darker");
                    ui.add(egui::Slider::new(&mut self.s.browser_darker, 0..=100).step_by(5.0).suffix("%")).on_hover_text("The browser draws its toolbar a step lighter than its window, so the toolbar stays dark gray.");
                }
                ui.end_row();
            });
            hint(ui, if self.s.browser_accent { "The whole browser window takes the accent, which is bright under most themes." } else { "The browser takes the color of the desktop's windows, so it is dark when they are, with the accent on its buttons." });
        });
    }
}

/// "Chromium", "Chromium and Brave", "Chromium, Chrome and Brave".
fn list(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
