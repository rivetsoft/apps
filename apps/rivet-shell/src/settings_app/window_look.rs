//! Appearance, Window Look: the compositor's gaps, borders, corners, unfocused opacity and
//! shadows, written to an overrides file the compositor's config includes (see comp.rs).

use eframe::egui;

use super::{SettingsApp, grid, hint, section};
use crate::comp;
use crate::sys::Compositor;

fn name(p: &std::path::Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

impl SettingsApp {
    pub(super) fn window_look_section(&mut self, ui: &mut egui::Ui) {
        // Checks: RIVET_SHELL_COMP=1 shows it turned on.
        if crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) && std::env::var("RIVET_SHELL_COMP").as_deref() == Ok("1") {
            self.s.comp_on = true;
        }
        let comp = Compositor::detect();
        let target = comp::target(comp);
        let ctx = ui.ctx().clone();
        // Whether the config includes ours: read again every couple of seconds (it's a file).
        let key = egui::Id::unique("window-look-included");
        let now = ctx.input(|i| i.time);
        let included = target.as_ref().is_some_and(|t| {
            let cached = ctx.data(|d| d.get_temp::<(f64, bool)>(key));
            match cached {
                Some((at, v)) if now - at < 2.0 => v,
                _ => {
                    let v = comp::included(t);
                    ctx.data_mut(|d| d.insert_temp(key, (now, v)));
                    v
                }
            }
        });
        let msg_key = egui::Id::unique("window-look-msg");
        let mut msg: Option<Result<String, String>> = ctx.data(|d| d.get_temp(msg_key));

        section(ui, "Window Look", &mut |ui| {
            let Some(t) = &target else {
                hint(ui, &format!("Gaps, borders and corners can be set here on niri, Hyprland and mango. This session runs {0}, so set them in {0}'s own settings.", comp.name()));
                return;
            };
            let s = &mut self.s;
            super::controls::master(ui, &mut s.comp_on, format!("Set {}'s gaps, borders and corners here", comp.name()));
            hint(ui, "Borders can follow the accent and change with the wallpaper.");
            if s.comp_on {
                super::controls::rule(ui);
                if !included {
                    let warn = rivet_look::theme::status(ui.ctx()).warning;
                    ui.label(egui::RichText::new(format!("{} doesn't include it yet, so these don't show.", name(&t.main))).color(warn));
                    ui.add_space(4.0);
                    if super::controls::button(ui, super::controls::Icon::Plus, &format!("Add the Include to {}", name(&t.main))).clicked() {
                        let (accent, dark) = comp::shell_look(s);
                        let look = comp::Look::from(s, accent, dark);
                        msg = Some(comp::add_include(t, Some(&look), &s.displays).map(|_| format!("Added. The original is saved as {}.rivet-backup.", name(&t.main))));
                        ctx.data_mut(|d| d.remove::<(f64, bool)>(key));
                    }
                    hint(ui, &format!("Adds one line to the end of {} so these settings take effect. A copy of the original is saved first.{}", name(&t.main), if t.comp == Compositor::Niri { " Needs niri 25.11 or later." } else { "" }));
                }
                match &msg {
                    Some(Ok(m)) => hint(ui, m),
                    Some(Err(e)) => {
                        ui.label(egui::RichText::new(e).color(rivet_look::theme::status(ui.ctx()).danger));
                    }
                    None => {}
                }
                ui.add_space(10.0);
                grid(ui, "window-look", &mut |ui| {
                    ui.label("Gaps between windows");
                    ui.add(egui::Slider::new(&mut s.comp_gaps_in, 0..=32).suffix(" px"));
                    ui.end_row();
                    ui.label("Gaps at the screen's edges");
                    ui.add(egui::Slider::new(&mut s.comp_gaps_out, 0..=64).suffix(" px"));
                    ui.end_row();
                    ui.label("Border");
                    ui.add(egui::Slider::new(&mut s.comp_border, 0..=8).suffix(" px"));
                    ui.end_row();
                    ui.label("Rounded corners");
                    ui.add(egui::Slider::new(&mut s.comp_radius, 0..=24).suffix(" px"));
                    ui.end_row();
                    ui.label("Unfocused windows");
                    let mut pct = (s.comp_inactive_opacity * 100.0).round() as u32;
                    if ui.add(egui::Slider::new(&mut pct, 50..=100).suffix("% opaque")).changed() {
                        s.comp_inactive_opacity = pct as f32 / 100.0;
                    }
                    ui.end_row();
                    ui.label("");
                    super::controls::switch(ui, &mut s.comp_accent_borders, "Focused window's border in the accent color");
                    ui.end_row();
                    ui.label("");
                    super::controls::switch(ui, &mut s.comp_shadows, "Shadows under windows");
                    ui.end_row();
                });
            }
        });
        ctx.data_mut(|d| match &msg {
            Some(m) => {
                d.insert_temp(msg_key, m.clone());
            }
            None => d.remove::<Result<String, String>>(msg_key),
        });
        // Written straight away (the shell does too, when it's running), when something changed.
        let Some(t) = target else { return };
        let snap = (self.s.comp_on, self.s.comp_gaps_in, self.s.comp_gaps_out, self.s.comp_border, self.s.comp_radius, self.s.comp_accent_borders, (self.s.comp_inactive_opacity * 100.0) as u32, self.s.comp_shadows, self.s.accent, self.s.follow_desktop_colors, self.s.theme);
        let snap_key = egui::Id::unique("window-look-applied");
        let first = ctx.data(|d| d.get_temp::<String>(snap_key)).is_none();
        let now_s = format!("{snap:?}");
        if ctx.data(|d| d.get_temp::<String>(snap_key)).as_deref() != Some(now_s.as_str()) {
            ctx.data_mut(|d| d.insert_temp(snap_key, now_s));
            if !first && !crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) {
                let _ = comp::apply_now(t.comp, &self.s);
            }
        }
    }
}
