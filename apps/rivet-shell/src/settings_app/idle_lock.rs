//! The Idle & Lock page: what happens as the computer sits untouched, and the lock screen.

use eframe::egui;

use super::{SettingsApp, grid, hint, section};

/// The times offered, in seconds (0: never).
const TIMES: [u32; 12] = [0, 30, 60, 120, 180, 300, 600, 900, 1200, 1800, 2700, 3600];

fn time_label(secs: u32) -> String {
    match secs {
        0 => "Never".into(),
        s if s < 60 => format!("{s} seconds"),
        60 => "1 minute".into(),
        s if s < 3600 && s % 60 != 0 => format!("{} min {} s", s / 60, s % 60),
        s if s < 3600 => format!("{} minutes", s / 60),
        3600 => "1 hour".into(),
        s => format!("{:.1} hours", s as f32 / 3600.0),
    }
}

/// A time picked from the list (a time set by hand that isn't in it shows too).
fn time_combo(ui: &mut egui::Ui, id: &str, v: &mut u32) {
    egui::ComboBox::from_id_salt(id).selected_text(time_label(*v)).width(150.0).show_ui(ui, |ui| {
        let mut all = TIMES.to_vec();
        if !all.contains(v) {
            all.push(*v);
            all.sort();
        }
        for t in all {
            ui.selectable_value(v, t, time_label(t));
        }
    });
}

impl SettingsApp {
    pub(super) fn idle_page(&mut self, ui: &mut egui::Ui) {
        let other = crate::idle::other_daemon();
        section(ui, "When idle", &mut |ui| {
            let s = &mut self.s;
            ui.checkbox(&mut s.idle_on, "Let Rivet Shell handle idle");
            if let Some(d) = &other {
                let warn = rivet_look::theme::status(ui.ctx()).warning;
                ui.label(egui::RichText::new(format!("{d} is also running, so each step would happen twice. Stop {d}, or turn this off and let {d} handle idle.")).color(warn));
            } else {
                hint(ui, "Each step counts from your last key press or mouse move. Playing video and Keep Awake hold them off.");
            }
            if !s.idle_on {
                return;
            }
            ui.add_space(10.0);
            grid(ui, "idle-times", &mut |ui| {
                ui.label("Dim the screen");
                time_combo(ui, "idle-dim", &mut s.idle_dim);
                ui.end_row();
                ui.label("Lock");
                time_combo(ui, "idle-lock", &mut s.idle_lock);
                ui.end_row();
                ui.label("Turn the screens off");
                time_combo(ui, "idle-off", &mut s.idle_screen_off);
                ui.end_row();
                ui.label("Suspend");
                time_combo(ui, "idle-suspend", &mut s.idle_suspend);
                ui.end_row();
            });
            ui.add_space(8.0);
            ui.checkbox(&mut s.idle_media, "Not while music or a video plays");
            hint(ui, "Any player that shows in Now Playing counts.");
        });
        section(ui, "Lock screen", &mut |ui| {
            let s = &mut self.s;
            ui.checkbox(&mut s.lock_on_sleep, "Lock before the computer sleeps");
            ui.add_space(6.0);
            let mut own = s.lock_cmd.trim().is_empty();
            ui.horizontal(|ui| {
                ui.radio_value(&mut own, true, "Rivet Shell's Lock Screen");
                ui.radio_value(&mut own, false, "Another Program");
            });
            if own && !s.lock_cmd.trim().is_empty() {
                s.lock_cmd.clear();
            } else if !own && s.lock_cmd.trim().is_empty() {
                s.lock_cmd = ["hyprlock", "swaylock"].into_iter().find(|p| crate::sys::has(p)).unwrap_or("swaylock").to_string();
            }
            ui.add_space(6.0);
            if own {
                ui.checkbox(&mut s.lock_blur, "Blur the wallpaper");
                ui.checkbox(&mut s.lock_power, "Show suspend, restart and shut down buttons");
                ui.add_space(4.0);
                ui.add_space(6.0);
                if ui.button("Try It").on_hover_text("Shows the lock screen without locking anything. Press Escape to leave.").clicked()
                    && let Ok(exe) = std::env::current_exe()
                {
                    {
                        let _ = std::process::Command::new(exe).args(["lock", "--preview"]).stdin(std::process::Stdio::null()).spawn();
                    }
                }
            } else {
                grid(ui, "lock-cmd", &mut |ui| {
                    ui.label("Command");
                    ui.add(egui::TextEdit::singleline(&mut s.lock_cmd).desired_width(260.0).font(egui::TextStyle::Monospace));
                    ui.end_row();
                });
                hint(ui, "Use a locker that stays open until you unlock, so the shell knows the screen is locked. With swaylock, leave out -f.");
            }
        });
        section(ui, "Keys", &mut |ui| {
            hint(ui, "Bind this in your compositor's settings to lock right away:");
            ui.add_space(4.0);
            ui.label(egui::RichText::new("rivet-shell msg lock").monospace().size(12.0));
            hint(ui, "loginctl lock-session does the same, while the shell runs.");
        });
    }
}
