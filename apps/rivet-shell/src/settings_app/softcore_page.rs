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
use super::{SettingsApp, hint, on_card, search, section};
use crate::softcore::{self, Kind, Setting, State};
use crate::sys::Compositor;

/// The settings the Appearance page's Window Look writes while it is on. They are left to it
/// here, so one thing isn't set in two places.
const WINDOW_LOOK: [&str; 6] = ["gaps", "gaps_out", "border", "radius", "inactive_opacity", "shadows"];

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
    /// The cards that are open. They start closed: the page is long, and the list at its
    /// top says what is on it.
    open: std::collections::HashSet<String>,
    /// The border colours set to Custom, so the picker shows even while the value is one of
    /// the theme's.
    custom: std::collections::HashSet<String>,
    /// The colours that follow the theme, as Rivet Shell's settings have them: (key, `accent`,
    /// `secondary` or `tertiary`). Changed here while the page is drawn and handed back after.
    roles: Vec<(String, String)>,
    /// Whether a colour can follow the theme: Softcore's settings include Rivet Shell's file,
    /// which is where the shell keeps such a colour up to date. Looked at every two seconds.
    follows: bool,
    follows_at: f64,
    channel: Option<(Sender<Answer>, Receiver<Answer>)>,
    /// A surface's name being typed into the blur picker's menu.
    surface_typed: String,
}

static PAGE: LazyLock<Mutex<Page>> = LazyLock::new(Mutex::default);

fn checking() -> bool {
    crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed)
}

/// Whether the page is shown: Softcore is the compositor. Nothing of Softcore's is looked for
/// or run anywhere else.
pub(super) fn available() -> bool {
    // Checks: RIVET_SHELL_SOFTCORE=1 shows the page with sample settings, on any compositor
    // (a card's name instead of 1 also opens that card).
    if checking() {
        return std::env::var_os("RIVET_SHELL_SOFTCORE").is_some();
    }
    Compositor::detect() == Compositor::Softcore
}

fn times(state: &State) -> Vec<Option<SystemTime>> {
    state.files.iter().map(|f| std::fs::metadata(f).and_then(|m| m.modified()).ok()).collect()
}

impl Page {
    fn role(&self, key: &str) -> Option<&str> {
        self.roles.iter().find(|(k, _)| k == key).map(|(_, r)| r.as_str()).filter(|_| self.follows)
    }

    /// Have a colour follow one of the theme's (None: no longer).
    fn follow(&mut self, key: &str, role: Option<&str>) {
        self.roles.retain(|(k, _)| k != key);
        if let Some(r) = role {
            self.roles.push((key.to_string(), r.to_lowercase()));
            self.roles.sort();
        }
    }

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
                // Checks: RIVET_SHELL_SOFTCORE=Keyboard (or another card's name) shows it open.
                self.open.extend(std::env::var("RIVET_SHELL_SOFTCORE").ok().filter(|v| v != "1"));
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

/// A card that folds away: its title with what it holds beside it, and the card under it only
/// while it is open. Returns true when the title was clicked.
fn fold(ui: &mut egui::Ui, title: &str, holds: &str, open: bool, body: &mut dyn FnMut(&mut egui::Ui)) -> bool {
    let v = ui.visuals().clone();
    let fill = v.faint_bg_color;
    let (r, resp) = ui.allocate_exact_size(egui::Vec2::new(ui.available_width(), 38.0), egui::Sense::click());
    let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
    let p = ui.painter();
    // Closed, the title is the whole card; open, it sits over the card like the other pages'.
    if !open || resp.hovered() {
        p.rect_filled(r, rivet_look::theme::r_md(), if resp.hovered() { rivet_look::palette::mix(fill, v.text_color(), 0.05) } else { fill });
    }
    let c = egui::Pos2::new(r.left() + 16.0, r.center().y);
    let s = egui::Stroke::new(1.6, v.weak_text_color());
    let arrow = if open { [c + egui::Vec2::new(-4.0, -2.0), c + egui::Vec2::new(0.0, 2.5), c + egui::Vec2::new(4.0, -2.0)] } else { [c + egui::Vec2::new(-2.0, -4.0), c + egui::Vec2::new(2.5, 0.0), c + egui::Vec2::new(-2.0, 4.0)] };
    p.add(egui::Shape::line(arrow.to_vec(), s));
    let g = p.layout_no_wrap(title.to_string(), egui::FontId::proportional(15.0), v.strong_text_color());
    let after = r.left() + 32.0 + g.size().x + 12.0;
    p.galley(egui::Pos2::new(r.left() + 32.0, r.center().y - g.size().y / 2.0), g, v.strong_text_color());
    p.text(egui::Pos2::new(after, r.center().y), egui::Align2::LEFT_CENTER, holds, egui::FontId::proportional(12.0), v.weak_text_color());
    let mut whole = r;
    if open {
        ui.add_space(2.0);
        let fr = egui::Frame::NONE.fill(fill).corner_radius(rivet_look::theme::r_md()).inner_margin(egui::Margin::symmetric(20, 18)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            on_card(ui.visuals_mut(), fill);
            body(ui);
        });
        whole = whole.union(fr.response.rect);
        ui.add_space(14.0);
    }
    search::mark(ui, title, whole);
    ui.add_space(6.0);
    resp.clicked()
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
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

/// Rivet Shell's own surfaces that can sit over windows, by the names it gives them, and
/// what Settings calls each.
const SHELL_SURFACES: [(&str, &str); 5] = [("rivet-shell-panel", "Panels"), ("rivet-shell-dock", "Dock"), ("rivet-shell-popup", "Menus and pop-ups"), ("rivet-shell-notifications", "Notifications"), ("rivet-shell-osd", "Volume and brightness pop-up")];

/// A surface's name as a person reads it: Rivet's own by what they are, others as named.
fn surface_label(name: &str) -> &str {
    SHELL_SURFACES.iter().find(|(n, _)| *n == name).map(|(_, l)| *l).unwrap_or(name)
}

/// The surfaces to blur behind, as tags: each picked one with a way to take it off, and a
/// menu of Rivet's surfaces and the others on screen now, with a field for any other name.
fn surface_tags(ui: &mut egui::Ui, now: &str, on_screen: &[String], typed: &mut String) -> Option<Option<String>> {
    let picked: Vec<&str> = now.split(',').map(str::trim).filter(|n| !n.is_empty()).collect();
    let mut next: Option<Vec<String>> = None;
    let with = |name: &str| picked.iter().map(|n| n.to_string()).chain(std::iter::once(name.to_string())).collect::<Vec<_>>();
    ui.horizontal_wrapped(|ui| {
        ui.set_max_width(380.0);
        ui.spacing_mut().item_spacing = egui::Vec2::new(6.0, 6.0);
        let v = ui.visuals().clone();
        for name in &picked {
            egui::Frame::NONE.fill(rivet_look::palette::mix(v.panel_fill, v.text_color(), 0.1)).corner_radius(14.0).inner_margin(egui::Margin { left: 11, right: 4, top: 2, bottom: 2 }).show(ui, |ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let label = surface_label(name);
                let r = ui.label(egui::RichText::new(label).size(13.0));
                if label != *name {
                    r.on_hover_text(*name);
                }
                if ui.add(egui::Button::new(egui::RichText::new("×").size(14.0).color(v.weak_text_color())).frame(false).min_size(egui::Vec2::splat(20.0))).on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text("Remove").clicked() {
                    next = Some(picked.iter().filter(|n| *n != name).map(|n| n.to_string()).collect());
                }
            });
        }
        let add = controls::button(ui, Icon::Plus, "Add");
        egui::Popup::menu(&add).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
            ui.set_width(260.0);
            // The rest on screen now, each that isn't picked yet. Not Rivet's own: the shell asks
            // for blur behind them itself, in their shapes.
            let mut offers: Vec<&str> = on_screen.iter().map(String::as_str).filter(|n| !n.starts_with("rivet-shell-")).collect();
            offers.retain(|n| !picked.contains(n));
            for &name in &offers {
                let shown = surface_label(name);
                let mut text = egui::text::LayoutJob::default();
                text.append(shown, 0.0, egui::TextFormat { font_id: rivet_look::fonts::ui_font(13.5), color: ui.visuals().text_color(), ..Default::default() });
                if shown != name {
                    text.append(name, 8.0, egui::TextFormat { font_id: rivet_look::fonts::ui_font(11.5), color: ui.visuals().weak_text_color(), ..Default::default() });
                }
                if ui.selectable_label(false, text).clicked() {
                    next = Some(with(name));
                    ui.close();
                }
            }
            if offers.is_empty() {
                ui.label(egui::RichText::new("No other bars or docks are open. Type one's name.").size(12.5).color(ui.visuals().weak_text_color()));
            }
            ui.separator();
            let r = ui.add(egui::TextEdit::singleline(typed).hint_text("A surface's name, then Enter").desired_width(ui.available_width()));
            let name = typed.trim().to_string();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) && !name.is_empty() && !name.contains(',') {
                if !picked.contains(&name.as_str()) {
                    next = Some(with(&name));
                }
                typed.clear();
                ui.close();
            }
        });
    });
    next.map(|list| (!list.is_empty()).then(|| list.join(",")))
}

/// One setting's control. Returns the change to make, if any: `Some(text)` to set it,
/// `None` inside to put it back to its default.
///
/// `theme` is the theme's colours by name. A border's colour is picked from them, or is one
/// of its own under Custom; the other colours have no use for them.
fn control(ui: &mut egui::Ui, s: &Setting, page: &mut Page, now: &softcore::Now, accent: Color32, theme: &[(&str, Color32)]) -> Option<Option<String>> {
    let mut change = None;
    let held = ui.input(|i| i.pointer.any_down());
    let shown = page.edits.get(&s.key).cloned().unwrap_or_else(|| now.text.clone());
    ui.horizontal(|ui| {
        // Nothing set, where nothing means "follow": say so, with a way to choose a value.
        // A border's first colour has Automatic in its list of colours instead.
        let listed = s.kind == Kind::Color && s.key.starts_with("border_") && !s.key.ends_with("_to") && !theme.is_empty();
        if s.optional && shown.is_empty() && !listed && matches!(s.kind, Kind::Int { .. } | Kind::Color) {
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
                if s.key.starts_with("border_") && !theme.is_empty() {
                    let hex = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
                    // A colour that follows the theme is named in Rivet Shell's settings. Where it
                    // can't follow, Softcore's file has the colour alone, and the theme's colour
                    // it equals is the one chosen.
                    let from = if page.follows {
                        page.role(&s.key).and_then(|r| theme.iter().position(|(n, _)| n.eq_ignore_ascii_case(r)))
                    } else {
                        theme.iter().position(|(_, c)| hex(*c).eq_ignore_ascii_case(shown.trim())).filter(|_| !page.custom.contains(&s.key))
                    };
                    let auto = listed && shown.is_empty() && from.is_none() && !page.custom.contains(&s.key);
                    let dot = |ui: &mut egui::Ui, c: Color32| {
                        let (r, _) = ui.allocate_exact_size(egui::Vec2::splat(14.0), egui::Sense::hover());
                        ui.painter().circle_filled(r.center(), 6.0, c);
                        ui.painter().circle_stroke(r.center(), 6.0, egui::Stroke::new(1.0, ui.visuals().weak_text_color().gamma_multiply(0.5)));
                    };
                    if let Some(i) = from {
                        dot(ui, theme[i].1);
                    }
                    egui::ComboBox::from_id_salt(("softcore-color", &s.key)).selected_text(if auto { "Automatic" } else { from.map_or("Custom", |i| theme[i].0) }).width(120.0).show_ui(ui, |ui| {
                        if listed && ui.selectable_label(auto, "Automatic").clicked() {
                            page.custom.remove(&s.key);
                            page.follow(&s.key, None);
                            if now.in_file {
                                change = Some(None);
                            }
                        }
                        for (i, (name, color)) in theme.iter().enumerate() {
                            ui.horizontal(|ui| {
                                dot(ui, *color);
                                if ui.selectable_label(from == Some(i), *name).clicked() {
                                    page.custom.remove(&s.key);
                                    if page.follows {
                                        // The shell writes the colour, now and when the theme changes;
                                        // a line of Softcore's own would stand over it.
                                        page.follow(&s.key, Some(name));
                                        if now.in_file {
                                            change = Some(None);
                                        }
                                    } else if from != Some(i) {
                                        change = Some(Some(hex(*color)));
                                    }
                                }
                            });
                        }
                        if ui.selectable_label(from.is_none() && !auto, "Custom").clicked() {
                            page.custom.insert(s.key.clone());
                            // It starts as the colour it was; with none of its own, the accent.
                            if let Some(i) = from.filter(|_| page.follows) {
                                page.follow(&s.key, None);
                                change = Some(Some(hex(theme[i].1)));
                            } else if shown.is_empty() {
                                change = Some(Some(hex(accent)));
                            }
                        }
                    });
                    if auto {
                        return;
                    }
                    if from.is_some() {
                        return;
                    }
                    // Custom: the picker alone. Automatic and One Color are the ways back, so no Reset.
                    if ui.color_edit_button_srgb(&mut c).changed() {
                        page.edits.insert(s.key.clone(), format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2]));
                    }
                    return;
                }
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
        let roles = crate::style::Roles::new(&self.look, &self.s);
        // Can a colour follow the theme? Softcore's settings have to include the shell's file.
        let at = ctx.input(|i| i.time);
        if at - page.follows_at >= 2.0 || page.follows_at == 0.0 {
            page.follows_at = at;
            page.follows = checking() || crate::comp::target(Compositor::Softcore).is_some_and(|t| crate::comp::included(&t));
        }
        page.roles = self.s.softcore_colors.clone();
        // Where it can't, a colour is known by its value: without a second and third colour in
        // the theme they are the accent again, and once is enough.
        let mut theme: Vec<(&str, Color32)> = Vec::new();
        for (name, color) in [("Accent", roles.primary), ("Secondary", roles.secondary), ("Tertiary", roles.tertiary)] {
            if page.follows || theme.iter().all(|(_, c)| *c != color) {
                theme.push((name, color));
            }
        }
        let mut changes: Vec<(String, Option<String>)> = Vec::new();

        // What is on the page: each name opens its card and goes to it.
        let mut names = state.groups();
        names.retain(|g| state.settings.iter().any(|s| s.group == *g));
        names.push("Key Bindings".to_string());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::new(6.0, 6.0);
            for name in &names {
                if ui.add(egui::Button::new(egui::RichText::new(name).size(12.5)).corner_radius(rivet_look::theme::r_sm())).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    page.open.insert(name.clone());
                    search::reveal(&ctx, name);
                }
            }
            let all = names.iter().all(|n| page.open.contains(n));
            if ui.add(egui::Button::new(egui::RichText::new(if all { "Close All" } else { "Open All" }).size(12.5).color(ui.visuals().weak_text_color())).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                if all {
                    page.open.clear();
                } else {
                    page.open.extend(names.iter().cloned());
                }
            }
        });
        ui.add_space(14.0);
        let mut toggled: Vec<String> = Vec::new();

        for group in state.groups() {
            // A border's colour and the one it runs to are one row: one colour or two.
            let runs_to = |key: &str| state.settings.iter().find(|x| key.starts_with("border_") && x.kind == Kind::Color && x.key == format!("{key}_to"));
            let second = |key: &str| key.strip_suffix("_to").is_some_and(|first| runs_to(first).is_some());
            let list: Vec<&Setting> = state.settings.iter().filter(|s| s.group == group && !second(&s.key) && !theirs(&s.key)).collect();
            let hidden = state.settings.iter().any(|s| s.group == group && theirs(&s.key));
            if list.is_empty() && !hidden {
                continue;
            }
            let open = page.open.contains(&group);
            let changed = list.iter().filter(|s| state.values.get(&s.key).is_some_and(|n| n.in_file)).count();
            let holds = if changed == 0 { count(list.len(), "setting", "settings") } else { format!("{} · {changed} changed", count(list.len(), "setting", "settings")) };
            let clicked = fold(ui, &group, &holds, open, &mut |ui| {
                if hidden {
                    hint(ui, "Gaps, border width, corners and shadows are set under Window Look on the Appearance page.");
                    if !list.is_empty() {
                        ui.add_space(8.0);
                    }
                }
                // One label width on every card, so the controls line up down the page; in a
                // narrow window the labels give way and the notes wrap sooner.
                let labels = (ui.available_width() * 0.36).clamp(150.0, 230.0);
                let room = (ui.available_width() - labels - 24.0).max(170.0);
                egui::Grid::new(format!("softcore-{group}")).num_columns(2).spacing([24.0, 14.0]).min_col_width(labels).max_col_width(room).show(ui, |ui| {
                    let mut part = 0;
                    for (at, s) in softcore::parts(&list).into_iter().enumerate().flat_map(|(at, p)| p.into_iter().map(move |s| (at, s))) {
                        // Between two parts (borders, shadows, blur…), room and a faint line.
                        if at != part {
                            part = at;
                            let (r, _) = ui.allocate_exact_size(egui::Vec2::new(labels, 9.0), egui::Sense::hover());
                            let line = ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.6);
                            ui.painter().hline(r.left()..=r.left() + labels + 24.0 + room, r.center().y, egui::Stroke::new(1.0, line));
                            ui.end_row();
                        }
                        let now = state.values.get(&s.key).cloned().unwrap_or_default();
                        ui.scope(|ui| {
                            ui.set_max_width(labels);
                            ui.add(egui::Label::new(&s.label).wrap());
                        });
                        ui.vertical(|ui| {
                            ui.set_max_width(room.min(420.0));
                            if let Some(to) = runs_to(&s.key) {
                                let to_now = state.values.get(&to.key).cloned().unwrap_or_default();
                                let was = !to_now.text.is_empty() || page.role(&to.key).is_some();
                                let mut two = was;
                                super::panel_editor::segmented(ui, &mut two, &[(false, "One Color"), (true, "Two Colors")]);
                                if two != was {
                                    // The second colour starts as one of the theme's that the first isn't.
                                    let hex = |c: Color32| format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
                                    if !two {
                                        page.follow(&to.key, None);
                                        if to_now.in_file || !page.follows {
                                            changes.push((to.key.clone(), None));
                                        }
                                    } else if page.follows {
                                        let first = page.role(&s.key).unwrap_or("accent").to_string();
                                        let start = theme.iter().map(|(n, _)| *n).find(|n| !n.eq_ignore_ascii_case(&first)).unwrap_or("Accent");
                                        page.follow(&to.key, Some(start));
                                    } else {
                                        let start = theme.iter().map(|(_, c)| hex(*c)).find(|h| !h.eq_ignore_ascii_case(now.text.trim())).unwrap_or_else(|| hex(accent));
                                        changes.push((to.key.clone(), Some(start)));
                                    }
                                }
                                // The first colour, then the one it runs to: side by side where there
                                // is room for two, one under the other in a narrow window.
                                let mut both = |ui: &mut egui::Ui| {
                                    ui.spacing_mut().item_spacing.x = 12.0;
                                    if let Some(c) = control(ui, s, page, &now, accent, &theme) {
                                        changes.push((s.key.clone(), c));
                                    }
                                    if was && let Some(c) = control(ui, to, page, &to_now, accent, &theme) {
                                        changes.push((to.key.clone(), c));
                                    }
                                };
                                if room >= 400.0 {
                                    ui.horizontal(both);
                                } else {
                                    both(ui);
                                }
                                match page.refused.get(&s.key).or(page.refused.get(&to.key)) {
                                    Some(why) => {
                                        ui.add(egui::Label::new(egui::RichText::new(format!("Softcore didn't take that: {why}.")).size(12.0).color(danger)).wrap());
                                    }
                                    // Said once, under the first of the two borders.
                                    None if s.key == "border_active" && !page.follows => hint(ui, "Automatic follows the theme. Accent, Secondary and Tertiary stay as they are now."),
                                    None => {}
                                }
                                return;
                            }
                            // The surfaces to blur behind are picked as tags, from what is on screen.
                            let c = if s.key == "blur_layers" { surface_tags(ui, &now.text, &state.layers, &mut page.surface_typed) } else { control(ui, s, page, &now, accent, &theme) };
                            if let Some(c) = c {
                                changes.push((s.key.clone(), c));
                            }
                            match page.refused.get(&s.key) {
                                Some(why) => {
                                    ui.add(egui::Label::new(egui::RichText::new(format!("Softcore didn't take that: {why}.")).size(12.0).color(danger)).wrap());
                                }
                                // Softcore's words are for typing the names; these are for picking them.
                                // A Rivet surface named here is blurred all over its rectangle, which for a
                // panel is its whole edge: the shell blurs them in their shapes instead.
                None if s.key == "blur_layers" && now.text.contains("rivet-shell-") => hint(ui, "Take Rivet's surfaces off: Rivet Shell blurs behind them in their own shapes. Turn it on in Appearance."),
                None if s.key == "blur_layers" => hint(ui, "For other bars and docks. All of a surface's rectangle is blurred, so pick ones that fill theirs."),
                                None => {
                                    hint(ui, &s.help);
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            if clicked {
                toggled.push(group.clone());
            }
        }

        let open = page.open.contains("Key Bindings");
        let clicked = fold(ui, "Key Bindings", &count(state.binds.len(), "of your own", "of your own"), open, &mut |ui| {
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
        if clicked {
            toggled.push("Key Bindings".to_string());
        }
        for name in toggled {
            if !page.open.remove(&name) {
                page.open.insert(name);
            }
        }
        ui.add_space(8.0);
        hint(ui, "Window rules, named workspaces and startup programs are in Softcore's settings file.");

        for change in changes {
            page.ask(&ctx, Some(change));
        }
        // Saved with Rivet Shell's settings; the shell then writes the colours where Softcore reads them.
        let roles = std::mem::take(&mut page.roles);
        if page.follows && roles != self.s.softcore_colors {
            self.s.softcore_colors = roles;
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
