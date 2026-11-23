//! The Softcore page: Softcore's own settings and key bindings, changed from here.
//!
//! Only there when Softcore is the compositor. The controls are drawn from what Softcore says
//! its settings are (see `crate::softcore`), and each change is one line of its settings file
//! changed by Softcore itself, so what was typed there by hand stays, and a change made by
//! hand shows here a moment later. Softcore takes a change up by itself.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use eframe::egui::{self, Color32};

use super::controls::{self, Icon};
use super::{SettingsApp, hint, section};
use crate::softcore::{self, Kind, Setting, State};
use crate::sys::Compositor;

/// The settings the Appearance page's Window Look writes while it is on. They are left to it
/// here, so one thing isn't set in two places.
const WINDOW_LOOK: [&str; 8] = ["gaps", "gaps_out", "border", "radius", "inactive_opacity", "shadows", "border_active", "border_inactive"];

/// What came back from Softcore: everything as it is now, and what went wrong with a change.
struct Answer {
    state: Result<State, String>,
    refused: Option<(String, String)>,
}

#[derive(Default)]
struct Page {
    state: Option<Result<State, String>>,
    /// A read or a change is on its way.
    waiting: bool,
    /// Values being dragged or typed, not yet handed to Softcore, by key.
    edits: HashMap<String, String>,
    /// Why Softcore wouldn't take a change, by key.
    refused: HashMap<String, String>,
    /// The settings files' times when last read, and when they were last looked at.
    times: Vec<Option<SystemTime>>,
    checked: f64,
    new_keys: String,
    new_action: String,
    channel: Option<(Sender<Answer>, Receiver<Answer>)>,
}

static PAGE: LazyLock<Mutex<Page>> = LazyLock::new(Mutex::default);

fn checking() -> bool {
    crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the page is shown: Softcore is the compositor. Nothing of Softcore's is looked for
/// or run anywhere else.
pub(super) fn available() -> bool {
    // Checks: RIVET_SHELL_SOFTCORE=1 shows the page with sample settings, on any compositor.
    if checking() {
        return std::env::var_os("RIVET_SHELL_SOFTCORE").is_some();
    }
    Compositor::detect() == Compositor::Softcore
}

fn times(state: &State) -> Vec<Option<SystemTime>> {
    state.files.iter().map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok()).collect()
}

impl Page {
    fn tx(&mut self) -> Sender<Answer> {
        self.channel.get_or_insert_with(channel).0.clone()
    }

    /// Ask Softcore for everything again, after making a change if one is given: `Some` sets
    /// the key, `None` takes its line out.
    fn ask(&mut self, ctx: &egui::Context, change: Option<(String, Option<String>)>) {
        if checking() {
            // Checks never run Softcore or touch its file: the change shows, and that is all.
            if let (Some((key, value)), Some(Ok(state))) = (change, self.state.as_mut()) {
                apply_locally(state, &key, value.as_deref());
            }
            return;
        }
        // The change shows at once; what Softcore then says replaces it.
        if let (Some((key, value)), Some(Ok(state))) = (&change, self.state.as_mut()) {
            apply_locally(state, key, value.as_deref());
            self.refused.remove(key);
        }
        self.waiting = true;
        let (tx, ctx) = (self.tx(), ctx.clone());
        std::thread::spawn(move || {
            let refused = change.and_then(|(key, value)| {
                let done = match &value {
                    Some(v) => softcore::set(&key, v),
                    None => softcore::unset(&key),
                };
                done.err().map(|e| (key, e))
            });
            let _ = tx.send(Answer { state: softcore::read(), refused });
            ctx.request_repaint();
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        while let Some(a) = self.channel.as_ref().and_then(|(_, rx)| rx.try_recv().ok()) {
            self.waiting = false;
            if let Some((key, why)) = a.refused {
                self.refused.insert(key, why);
            }
            if let Ok(s) = &a.state {
                self.times = times(s);
            }
            self.state = Some(a.state);
        }
        if self.state.is_none() && !self.waiting {
            if checking() {
                self.state = Some(Ok(softcore::sample()));
            } else {
                self.ask(ctx, None);
            }
        }
        // A change made by hand, or by `softcore settings set`, is taken in.
        if now - self.checked >= 2.0 && !self.waiting && !checking() {
            self.checked = now;
            if let Some(Ok(s)) = &self.state
                && times(s) != self.times
            {
                self.ask(ctx, None);
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
    }
}

/// Show a change before Softcore has answered.
fn apply_locally(state: &mut State, key: &str, value: Option<&str>) {
    if let Some(keys) = key.strip_prefix("bind.") {
        match (value, state.binds.iter().position(|b| b.keys == keys)) {
            (Some(v), Some(i)) => state.binds[i].action = v.to_string(),
            (Some(v), None) => state.binds.push(softcore::Bind { keys: keys.to_string(), action: v.to_string() }),
            (None, Some(i)) => {
                state.binds.remove(i);
            }
            (None, None) => {}
        }
        return;
    }
    let default = state.settings.iter().find(|s| s.key == key).map(|s| s.default.clone()).unwrap_or_default();
    let now = state.values.entry(key.to_string()).or_default();
    (now.text, now.in_file) = match value {
        Some(v) => (v.to_string(), true),
        None => (default, false),
    };
}

fn rgb(text: &str) -> Option<[u8; 3]> {
    let h = text.trim().strip_prefix('#').filter(|h| h.len() == 6)?;
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

/// A number as Softcore's file has it: no more places than the step needs.
fn tidy(v: f64, step: f64) -> String {
    let places = if step >= 1.0 {
        0
    } else if step >= 0.1 {
        1
    } else {
        2
    };
    let t = format!("{v:.places$}");
    if t.contains('.') { t.trim_end_matches('0').trim_end_matches('.').to_string() } else { t }
}

/// One setting's control. Returns the change to make, if any: `Some(text)` to set it,
/// `None` inside to put it back to its default.
fn control(ui: &mut egui::Ui, s: &Setting, page: &mut Page, now: &softcore::Now, accent: Color32) -> Option<Option<String>> {
    let mut change = None;
    let held = ui.input(|i| i.pointer.any_down());
    let shown = page.edits.get(&s.key).cloned().unwrap_or_else(|| now.text.clone());
    ui.horizontal(|ui| {
        // Nothing set, where nothing means "follow": say so, with a way to choose a value.
        if s.optional && shown.is_empty() && matches!(s.kind, Kind::Int { .. } | Kind::Color) {
            ui.label(egui::RichText::new("Follows").color(ui.visuals().weak_text_color()));
            if controls::button(ui, Icon::Plus, "Set").clicked() {
                change = Some(Some(match &s.kind {
                    Kind::Int { min, .. } => min.to_string(),
                    _ => format!("#{:02x}{:02x}{:02x}", accent.r(), accent.g(), accent.b()),
                }));
            }
            return;
        }
        match &s.kind {
            Kind::Bool => {
                let mut on = shown == "true";
                if controls::switch(ui, &mut on, "").changed() {
                    change = Some(Some(on.to_string()));
                }
            }
            Kind::Int { min, max } => {
                let mut v = shown.parse::<i64>().unwrap_or(*min).clamp(*min, *max);
                if ui.add(egui::Slider::new(&mut v, *min..=*max)).changed() {
                    page.edits.insert(s.key.clone(), v.to_string());
                }
            }
            Kind::Float { min, max, step } => {
                let mut v = shown.parse::<f64>().unwrap_or(*min).clamp(*min, *max);
                if ui.add(egui::Slider::new(&mut v, *min..=*max).step_by(*step)).changed() {
                    page.edits.insert(s.key.clone(), tidy(v, *step));
                }
            }
            Kind::Color => {
                let mut c = rgb(&shown).unwrap_or([0, 0, 0]);
                if ui.color_edit_button_srgb(&mut c).changed() {
                    page.edits.insert(s.key.clone(), format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                }
                ui.label(egui::RichText::new(&shown).monospace().color(ui.visuals().weak_text_color()));
            }
            Kind::Choice(choices) => {
                egui::ComboBox::from_id_salt(("softcore-choice", &s.key)).selected_text(shown.clone()).width(170.0).show_ui(ui, |ui| {
                    for c in choices {
                        if ui.selectable_label(*c == shown, c).clicked() && *c != shown {
                            change = Some(Some(c.clone()));
                        }
                    }
                });
            }
            Kind::Text => {
                let mut text = shown.clone();
                let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width(230.0).hint_text(if s.default.is_empty() { "Not set" } else { s.default.as_str() }));
                if r.changed() {
                    page.edits.insert(s.key.clone(), text.clone());
                }
                // Typing is handed over when the field is left or Enter is pressed, not a letter at a time.
                if r.lost_focus()
                    && let Some(typed) = page.edits.remove(&s.key)
                    && typed.trim() != now.text
                {
                    change = Some(if typed.trim().is_empty() || typed.trim() == s.default { None } else { Some(typed.trim().to_string()) });
                }
            }
        }
        if now.in_file && !page.edits.contains_key(&s.key) && controls::reset(ui).on_hover_text("Back to Softcore's own value").clicked() {
            change = Some(None);
        }
    });
    // A slider or colour is handed over once it is let go of, not at every step of the drag.
    if !held
        && !matches!(s.kind, Kind::Text)
        && let Some(v) = page.edits.remove(&s.key)
        && v != now.text
    {
        change = Some(Some(v));
    }
    change
}

impl SettingsApp {
    pub(super) fn softcore_page(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let ctx = ui.ctx().clone();
        let mut guard = PAGE.lock().unwrap_or_else(|e| e.into_inner());
        let page = &mut *guard;
        page.poll(&ctx);
        let state = match &page.state {
            Some(Ok(s)) => s.clone(),
            Some(Err(e)) => {
                section(ui, "Softcore", &mut |ui| {
                    ui.label(egui::RichText::new("Softcore's settings couldn't be read.").color(rivet_look::theme::status(ui.ctx()).danger));
                    hint(ui, e);
                });
                if controls::button(ui, Icon::Reset, "Try Again").clicked() {
                    page.state = None;
                }
                return;
            }
            None => {
                hint(ui, "Reading Softcore's settings…");
                return;
            }
        };
        // Window Look, when on, writes these itself.
        let theirs = |key: &str| self.s.comp_on && WINDOW_LOOK.contains(&key);
        let danger = rivet_look::theme::status(&ctx).danger;
        let mut changes: Vec<(String, Option<String>)> = Vec::new();

        for group in state.groups() {
            let list: Vec<&Setting> = state.settings.iter().filter(|s| s.group == group && !theirs(&s.key)).collect();
            let hidden = state.settings.iter().any(|s| s.group == group && theirs(&s.key));
            if list.is_empty() && !hidden {
                continue;
            }
            section(ui, &group, &mut |ui| {
                if hidden {
                    hint(ui, "Gaps, borders, corners and shadows are set under Window Look on the Appearance page.");
                    if !list.is_empty() {
                        ui.add_space(8.0);
                    }
                }
                // One label width on every card, so the controls line up down the page; in a
                // narrow window the labels give way and the notes wrap sooner.
                let labels = (ui.available_width() * 0.36).clamp(150.0, 230.0);
                let room = (ui.available_width() - labels - 24.0).max(170.0);
                egui::Grid::new(format!("softcore-{group}")).num_columns(2).spacing([24.0, 14.0]).min_col_width(labels).max_col_width(room).show(ui, |ui| {
                    for s in &list {
                        let now = state.values.get(&s.key).cloned().unwrap_or_default();
                        ui.scope(|ui| {
                            ui.set_max_width(labels);
                            ui.add(egui::Label::new(&s.label).wrap());
                        });
                        ui.vertical(|ui| {
                            ui.set_max_width(room.min(420.0));
                            if let Some(c) = control(ui, s, page, &now, accent) {
                                changes.push((s.key.clone(), c));
                            }
                            match page.refused.get(&s.key) {
                                Some(why) => {
                                    ui.add(egui::Label::new(egui::RichText::new(format!("Softcore didn't take that: {why}.")).size(12.0).color(danger)).wrap());
                                }
                                None => {
                                    hint(ui, &s.help);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
        }

        section(ui, "Key Bindings", &mut |ui| {
            hint(ui, "Softcore's own bindings stay as well, unless they are turned off under Keyboard. A binding with no action takes one of them away.");
            ui.add_space(6.0);
            for b in &state.binds {
                let key = format!("bind.{}", b.keys);
                ui.horizontal(|ui| {
                    // The keys sit over the Add row's first field, so the two columns line up.
                    let (r, _) = ui.allocate_exact_size(egui::Vec2::new(150.0, 24.0), egui::Sense::hover());
                    ui.painter().with_clip_rect(r).text(r.left_center() + egui::Vec2::new(6.0, 0.0), egui::Align2::LEFT_CENTER, &b.keys, egui::TextStyle::Monospace.resolve(ui.style()), ui.visuals().text_color());
                    let mut text = page.edits.get(&key).cloned().unwrap_or_else(|| b.action.clone());
                    let r = ui.add(egui::TextEdit::singleline(&mut text).desired_width((ui.available_width() - 44.0).max(160.0)).font(egui::TextStyle::Monospace).hint_text("No action"));
                    if r.changed() {
                        page.edits.insert(key.clone(), text.clone());
                    }
                    if r.lost_focus()
                        && let Some(typed) = page.edits.remove(&key)
                        && typed.trim() != b.action
                    {
                        changes.push((key.clone(), Some(typed.trim().to_string())));
                    }
                    if controls::icon_button(ui, Icon::Trash).on_hover_text("Remove this binding").clicked() {
                        changes.push((key.clone(), None));
                    }
                });
                if let Some(why) = page.refused.get(&key) {
                    ui.add(egui::Label::new(egui::RichText::new(format!("Softcore didn't take that: {why}.")).size(12.0).color(danger)).wrap());
                }
            }
            if state.binds.is_empty() {
                hint(ui, "No bindings of your own yet.");
            }
            controls::rule(ui);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut page.new_keys).desired_width(150.0).font(egui::TextStyle::Monospace).hint_text("Mod+Shift+T"));
                ui.add(egui::TextEdit::singleline(&mut page.new_action).desired_width((ui.available_width() - 90.0).max(160.0)).font(egui::TextStyle::Monospace).hint_text("spawn rivet-terminal"));
                let ready = !page.new_keys.trim().is_empty();
                if controls::button_if(ui, ready, Icon::Plus, "Add").clicked() {
                    changes.push((format!("bind.{}", page.new_keys.trim()), Some(page.new_action.trim().to_string())));
                }
            });
            // A new binding that was refused stays in the fields, with why.
            let new_key = format!("bind.{}", page.new_keys.trim());
            match page.refused.get(&new_key) {
                Some(why) if !state.binds.iter().any(|b| format!("bind.{}", b.keys) == new_key) => {
                    ui.add(egui::Label::new(egui::RichText::new(format!("Softcore didn't take that: {why}.")).size(12.0).color(danger)).wrap());
                }
                _ => {}
            }
        });
        hint(ui, "Window rules, named workspaces and startup programs are in Softcore's settings file.");

        for change in changes {
            page.ask(&ctx, Some(change));
        }
        // Once Softcore has taken a new binding, the fields are cleared for the next.
        if let Some(Ok(s)) = &page.state
            && !page.waiting
            && s.binds.iter().any(|b| b.keys == page.new_keys.trim())
            && !page.refused.contains_key(&new_key_of(&page.new_keys))
        {
            page.new_keys.clear();
            page.new_action.clear();
        }
    }
}

fn new_key_of(keys: &str) -> String {
    format!("bind.{}", keys.trim())
}
