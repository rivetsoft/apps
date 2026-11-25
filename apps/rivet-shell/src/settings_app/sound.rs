//! The Sound page: where sound plays and where it is recorded from, each one's volume, and the
//! apps playing. It reads and changes them the way the panel's Sound popup does (`audio.rs`),
//! only while the page is showing.

use std::collections::HashMap;

use eframe::egui;

use super::connect::{quiet, row};
use super::{SettingsApp, controls, grid, section};
use crate::audio::{self, Audio, AudioLive, Device, Job, Target};
use crate::glyph;

pub(super) struct State {
    /// The thread that reads and changes; started the first time the page shows.
    live: Option<AudioLive>,
    /// Screenshots: made-up devices, and nothing is run.
    demo: Option<Audio>,
    /// Volumes set here a moment ago, shown until the next reading has caught up, so a slider
    /// being dragged doesn't jump back.
    held: HashMap<(u8, u32), (u32, f64)>,
}

fn key(t: Target) -> (u8, u32) {
    match t {
        Target::Output(i) => (0, i),
        Target::Input(i) => (1, i),
        Target::App(i) => (2, i),
    }
}

impl State {
    pub(super) fn new(demo: bool) -> State {
        State { live: None, demo: demo.then(audio::demo), held: HashMap::new() }
    }

    /// Each frame: read while the page shows, stop when it doesn't.
    pub(super) fn tend(&mut self, ctx: &egui::Context, showing: bool) {
        if self.demo.is_some() {
            return;
        }
        if !showing {
            if let Some(l) = &self.live {
                l.want(false);
            }
            return;
        }
        let live = self.live.get_or_insert_with(|| {
            let ctx = ctx.clone();
            audio::start(move || ctx.request_repaint())
        });
        live.want(true);
    }

    fn audio(&self) -> Audio {
        self.demo.clone().or_else(|| self.live.as_ref().map(|l| l.get())).unwrap_or_default()
    }

    fn run(&self, job: Job) {
        if let Some(l) = &self.live {
            l.run(job);
        }
    }

    /// A volume slider for `t`, which reads `volume` now.
    fn volume(&mut self, ui: &mut egui::Ui, t: Target, volume: u32) {
        let now = ui.input(|i| i.time);
        self.held.retain(|_, (_, at)| now - *at < 1.5);
        let mut v = self.held.get(&key(t)).map_or(volume, |(v, _)| *v);
        // Up to 100%, or as far as it has already been pushed.
        let top = v.max(100);
        if ui.add(egui::Slider::new(&mut v, 0..=top).suffix("%")).changed() {
            self.held.insert(key(t), (v, now));
            self.run(Job::Volume(t, v));
        }
    }

    /// A card of devices to choose between, then the chosen one's volume and mute.
    fn devices(&mut self, ui: &mut egui::Ui, list: &[Device], input: bool, read: bool) {
        if list.is_empty() {
            quiet(ui, if !read { "Reading…" } else if input { "No microphones found." } else { "No sound devices found." });
            return;
        }
        for d in list {
            let mut choose = false;
            let sub = if d.default && !d.sub.is_empty() { format!("In use · {}", d.sub) } else if d.default { "In use".to_string() } else { d.sub.clone() };
            row(ui, &|p, c, col| if input { glyph::mic(p, c, col, false) } else { glyph::speaker(p, c, false, col) }, &d.label, &sub, d.default, &mut |ui| {
                if !d.default && controls::button(ui, controls::Icon::Check, "Use").on_hover_text(if input { "Records from this microphone" } else { "Plays sound here, and moves the apps playing to it" }).clicked() {
                    choose = true;
                }
            });
            if choose {
                self.run(Job::Default { input, name: d.name.clone() });
            }
        }
        let Some(d) = list.iter().find(|d| d.default) else { return };
        let t = if input { Target::Input(d.index) } else { Target::Output(d.index) };
        controls::rule(ui);
        grid(ui, if input { "shell-sound-in" } else { "shell-sound-out" }, &mut |ui| {
            ui.label("Volume");
            self.volume(ui, t, d.volume);
            ui.end_row();
            ui.label("Mute");
            let mut muted = d.muted;
            if controls::switch(ui, &mut muted, "").changed() {
                self.run(Job::Mute(t));
            }
            ui.end_row();
        });
    }
}

impl SettingsApp {
    pub(super) fn sound_page(&mut self, ui: &mut egui::Ui) {
        let a = self.sound.audio();
        if let Some(e) = &a.error {
            section(ui, "Output", &mut |ui| quiet(ui, &format!("Sound couldn't be read: {e}.")));
            return;
        }
        section(ui, "Output", &mut |ui| self.sound.devices(ui, &a.outputs, false, a.read));
        section(ui, "Input", &mut |ui| self.sound.devices(ui, &a.inputs, true, a.read));
        if a.apps.is_empty() {
            return;
        }
        section(ui, "Apps", &mut |ui| {
            grid(ui, "shell-sound-apps", &mut |ui| {
                for app in &a.apps {
                    ui.label(&app.app).on_hover_text(&app.title);
                    ui.horizontal(|ui| {
                        self.sound.volume(ui, Target::App(app.index), app.volume);
                        // Which output it plays on, when there is more than one to choose from.
                        if a.outputs.len() > 1 {
                            let cur = a.outputs.iter().find(|d| d.index == app.device).map_or("", |d| d.label.as_str());
                            egui::ComboBox::from_id_salt(("shell-sound-app", app.index)).selected_text(cur).show_ui(ui, |ui| {
                                for d in &a.outputs {
                                    if ui.selectable_label(d.index == app.device, &d.label).clicked() && d.index != app.device {
                                        self.sound.run(Job::Move { app: app.index, device: d.index });
                                    }
                                }
                            });
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }
}
