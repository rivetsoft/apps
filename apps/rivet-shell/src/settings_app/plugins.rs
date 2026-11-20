//! The Plugins page: the plugins installed (turn one on, put its item on a panel), and hooks:
//! the person's own commands for the shell's events.

use eframe::egui::{self, Align2, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_md, r_sm};

use std::sync::mpsc::{Receiver, TryRecvError, channel};

use super::{SettingsApp, grid, hint, section};
use crate::config::Settings;
use crate::plugin_store::{self, Listing};
use crate::plugins::{EVENTS, SettingKind, forget_found, found_cached, user_dir};

/// The list of plugins other people have shared, and the one install or removal under way.
#[derive(Default)]
pub(super) struct Store {
    /// The list, once it has come (or why it didn't).
    list: Option<Result<Vec<Listing>, String>>,
    coming: Option<Receiver<Result<Vec<Listing>, String>>>,
    /// The plugin being installed, updated or removed, and how it went (what to say).
    job: Option<(String, Receiver<Result<String, String>>)>,
    /// How the last one went, and whether that's an error.
    said: Option<(String, bool)>,
    /// The plugin Remove was clicked on, until it's confirmed or kept.
    removing: Option<String>,
    /// The Get Plugins window: whether it's open, where it is, and which plugins it lists.
    open: bool,
    win: Option<rivet_look::windows::WinState>,
    show: Show,
}

/// Which plugins the Get Plugins window lists.
#[derive(Clone, Copy, PartialEq, Default)]
enum Show {
    #[default]
    All,
    /// The ones that come with the shell.
    Official,
    /// The ones other people have shared.
    ThirdParty,
}

impl Store {
    pub(super) fn asking(removing: Option<String>) -> Store {
        // Checks: RIVET_SHELL_PLUGIN_STORE=all|official|third-party opens the Get Plugins window on that choice.
        let show = std::env::var("RIVET_SHELL_PLUGIN_STORE").ok().filter(|_| std::env::args().any(|a| a == "--screenshot"));
        Store {
            removing,
            open: show.is_some(),
            show: match show.as_deref() {
                Some("official") => Show::Official,
                Some("third-party") => Show::ThirdParty,
                _ => Show::All,
            },
            ..Default::default()
        }
    }
}

/// Turn a plugin off: out of the list of those on, and its item off every panel.
fn turn_off(s: &mut Settings, id: &str) {
    let item = format!("plugin:{id}");
    s.plugins.retain(|p| p != id);
    for p in &mut s.panels {
        for z in &mut p.zones {
            z.retain(|i| i != &item);
        }
        p.tidy_groups();
    }
}

impl SettingsApp {
    /// The list and the job, as far as they've come.
    fn store_tick(&mut self, ui: &egui::Ui) {
        let st = &mut self.store;
        if st.list.is_none() && st.coming.is_none() {
            if self.shot.is_some() {
                // Checks: sample listings, or the file in RIVET_SHELL_REGISTRY.
                st.list = Some(if std::env::var_os("RIVET_SHELL_REGISTRY").is_some() { plugin_store::fetch() } else { Ok(plugin_store::sample()) });
            } else {
                let (tx, rx) = channel();
                std::thread::spawn(move || tx.send(plugin_store::fetch()));
                st.coming = Some(rx);
            }
        }
        let wait = || ui.ctx().request_repaint_after(std::time::Duration::from_millis(200));
        match st.coming.as_ref().map(|rx| rx.try_recv()) {
            Some(Ok(list)) => (st.list, st.coming) = (Some(list), None),
            Some(Err(TryRecvError::Empty)) => wait(),
            Some(Err(_)) => (st.list, st.coming) = (Some(Err("Couldn't get the plugin list.".into())), None),
            None => {}
        }
        match st.job.as_ref().map(|(_, rx)| rx.try_recv()) {
            Some(Ok(end)) => {
                st.said = Some(match end {
                    Ok(m) => (m, false),
                    Err(e) => (e, true),
                });
                st.job = None;
                forget_found();
                // The shell, if it's running, starts or stops the plugin.
                let _ = crate::ipc::send("changed");
            }
            Some(Err(TryRecvError::Empty)) => wait(),
            Some(Err(_)) => st.job = None,
            None => {}
        }
    }

    /// The Get Plugins window: every plugin there is, the shell's own and the ones other people
    /// have shared, with a switch between the two.
    pub(super) fn plugin_overlay(&mut self, ctx: &egui::Context) {
        if !self.store.open {
            return;
        }
        let area = ctx.content_rect().shrink(18.0);
        // The page behind steps back while the window is up.
        ctx.layer_painter(egui::LayerId::background()).rect_filled(ctx.content_rect(), 0.0, egui::Color32::from_black_alpha(110));
        let chrome = rivet_look::windows::Chrome { title: "Get Plugins".into(), icon: None, close: None, flush: false };
        let found = found_cached();
        let busy = self.store.job.as_ref().map(|(id, _)| id.clone());
        let danger = rivet_look::theme::status(ctx).danger;
        let (mut install, mut turn_on, mut retry) = (None::<Listing>, None::<String>, false);
        let mut win = self.store.win.take().unwrap_or_else(|| rivet_look::windows::WinState::new(71, Vec2::new(620.0, 520.0)));
        let (st, s) = (&mut self.store, &self.s);
        let shown = rivet_look::windows::show(ctx, &mut win, &chrome, area, Vec2::new(460.0, 320.0), |ui| {
            let v = ui.visuals().clone();
            let accent = v.selection.bg_fill;
            super::on_card(ui.visuals_mut(), v.window_fill);
            super::panel_editor::segmented(ui, &mut st.show, &[(Show::All, "All"), (Show::Official, "Official"), (Show::ThirdParty, "Third-Party")]);
            ui.add_space(8.0);
            if let Some((text, bad)) = &st.said {
                ui.label(egui::RichText::new(text).color(if *bad { danger } else { v.text_color() }));
                ui.add_space(4.0);
            }
            // The shell's own first, then the shared ones by name.
            let own: Vec<_> = found.iter().filter(|m| m.official && st.show != Show::ThirdParty).collect();
            let shared: Vec<&Listing> = match &st.list {
                Some(Ok(list)) if st.show != Show::Official => list.iter().filter(|l| !own.iter().any(|m| m.id == l.id)).collect(),
                _ => Vec::new(),
            };
            egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(ui.available_height() - 26.0).show(ui, |ui| {
                let card = |ui: &mut egui::Ui, name: &str, official: bool, description: &str, author: &str, version: &str, buttons: &mut dyn FnMut(&mut egui::Ui)| {
                    egui::Frame::NONE.fill(v.faint_bg_color).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            let text_w = (ui.available_width() - 190.0).max(160.0);
                            ui.vertical(|ui| {
                                ui.set_width(text_w);
                                ui.spacing_mut().item_spacing.y = 2.0;
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(name).strong());
                                    let (text, col) = if official { ("Official", accent) } else { ("Third-party", v.weak_text_color()) };
                                    let g = ui.painter().layout_no_wrap(text.to_string(), rivet_look::fonts::ui_font(11.0), col);
                                    let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(14.0, 6.0), Sense::hover());
                                    ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, mix(v.faint_bg_color, col, 0.6)), egui::StrokeKind::Inside);
                                    ui.painter().text(r.center(), Align2::CENTER_CENTER, text, rivet_look::fonts::ui_font(11.0), col);
                                });
                                if !description.is_empty() {
                                    ui.add(egui::Label::new(egui::RichText::new(description).size(12.5)).wrap());
                                }
                                let by: Vec<String> = [("by", author), ("version", version)].iter().filter(|(_, t)| !t.is_empty()).map(|(w, t)| format!("{w} {t}")).collect();
                                hint(ui, &by.join(" · "));
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| buttons(ui));
                        });
                    });
                    ui.add_space(4.0);
                };
                let state = |ui: &mut egui::Ui, text: &str| {
                    ui.label(egui::RichText::new(text).color(v.weak_text_color()).size(12.0));
                };
                for m in &own {
                    card(ui, &m.name, true, &m.description, &m.author, &m.version, &mut |ui| {
                        // They come with the shell: nothing to install, only to turn on.
                        if s.plugins.contains(&m.id) {
                            state(ui, "On");
                        } else if super::controls::button(ui, super::controls::Icon::Check, "Turn On").clicked() {
                            turn_on = Some(m.id.clone());
                        }
                    });
                }
                for l in &shared {
                    let have = found.iter().find(|m| m.id == l.id);
                    card(ui, &l.name, false, &l.description, &l.author, &l.version, &mut |ui| {
                        if busy.as_deref() == Some(l.id.as_str()) {
                            super::controls::button_if(ui, false, super::controls::Icon::Download, "Installing…");
                        } else if let Some(m) = have {
                            if plugin_store::update(m, std::slice::from_ref(*l)).is_some() {
                                if super::controls::button_if(ui, busy.is_none(), super::controls::Icon::Download, &format!("Update to {}", l.version)).clicked() {
                                    install = Some((*l).clone());
                                }
                            } else if s.plugins.contains(&m.id) {
                                state(ui, "On");
                            } else if super::controls::button(ui, super::controls::Icon::Check, "Turn On").clicked() {
                                turn_on = Some(m.id.clone());
                            }
                        } else if super::controls::primary_if(ui, busy.is_none(), super::controls::Icon::Download, "Install").clicked() {
                            install = Some((*l).clone());
                        }
                        if !l.homepage.is_empty() && super::controls::button(ui, super::controls::Icon::Out, "View Source").on_hover_text(&l.homepage).clicked() {
                            crate::sys::spawn("xdg-open", &[&l.homepage]);
                        }
                    });
                }
                // The shared list, when there's nothing of it to show.
                if st.show != Show::Official {
                    match &st.list {
                        None => hint(ui, "Getting the plugins other people have shared…"),
                        Some(Err(e)) => {
                            ui.label(e);
                            retry = super::controls::button(ui, super::controls::Icon::Reset, "Try Again").clicked();
                        }
                        Some(Ok(_)) if shared.is_empty() => hint(ui, "Plugins other people share will be listed here."),
                        Some(Ok(_)) => {}
                    }
                }
            });
            ui.add_space(6.0);
            hint(ui, "Other people make third-party plugins. A plugin can do anything you can, so turn on only ones you trust.");
        });
        st.open = shown.open;
        st.win = Some(win);
        if retry {
            st.list = None;
        }
        if let Some(id) = turn_on {
            self.s.plugins.retain(|p| p != &id);
            self.s.plugins.push(id);
            self.s.plugins.sort();
        }
        if let Some(l) = install.filter(|_| self.shot.is_none()) {
            self.store_install(&l);
        }
    }

    fn store_install(&mut self, l: &Listing) {
        let (tx, rx) = channel();
        let (l2, was) = (l.clone(), found_cached().iter().any(|m| m.id == l.id));
        std::thread::spawn(move || {
            tx.send(plugin_store::install(&l2).map(|m| if was { format!("{} is updated to version {}.", m.name, m.version) } else { format!("{} is installed. Turn it on to use it.", m.name) }))
        });
        self.store.said = None;
        self.store.job = Some((l.id.clone(), rx));
    }
}

impl SettingsApp {
    pub(super) fn plugins_page(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let accent = v.selection.bg_fill;
        self.store_tick(ui);
        let found = found_cached();
        let listed: Vec<Listing> = self.store.list.as_ref().and_then(|l| l.as_ref().ok()).cloned().unwrap_or_default();
        let busy = self.store.job.as_ref().map(|(id, _)| id.clone());
        let danger = rivet_look::theme::status(ui.ctx()).danger;
        // What a card's buttons asked for, carried out after the cards are drawn.
        let (mut install, mut ask_remove, mut remove, mut keep) = (None::<Listing>, None::<String>, None::<(String, String)>, false);
        // A folder chosen with the picker, for a plugin's setting.
        if let Some((id, key, rx)) = &self.plugin_pick {
            match rx.try_recv() {
                Ok(answer) => {
                    match answer {
                        Ok(Some(p)) => self.s.set_plugin(&id.clone(), &key.clone(), &p.to_string_lossy()),
                        Ok(None) => {}
                        Err(e) => self.note = Some(format!("Couldn't open the file picker: {e}")),
                    }
                    self.plugin_pick = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(std::time::Duration::from_millis(200)),
                Err(_) => self.plugin_pick = None,
            }
        }
        let can_pick = self.plugin_pick.is_none() && self.shot.is_none();
        let mut ask: Option<(String, String, String)> = None;
        section(ui, "Installed", &mut |ui| {
            // How the last install, update or removal went (the Get Plugins window says it too).
            if let Some((text, bad)) = self.store.said.as_ref().filter(|_| !self.store.open) {
                ui.label(egui::RichText::new(text).color(if *bad { danger } else { v.text_color() }));
                ui.add_space(6.0);
            }
            let s = &mut self.s;
            if found.is_empty() {
                hint(ui, "No plugins yet. Put one in your plugins folder and it shows here.");
            }
            for m in &found {
                let mut on = s.plugins.contains(&m.id);
                let item = format!("plugin:{}", m.id);
                egui::Frame::NONE.fill(v.panel_fill).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        if super::controls::switch(ui, &mut on, "").changed() {
                            // Off: its item comes off the panels too.
                            turn_off(s, &m.id);
                            if on {
                                s.plugins.push(m.id.clone());
                                s.plugins.sort();
                            }
                        }
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 2.0;
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new(&m.name).strong());
                                // Where it's from: the shell's own, or the person's folder.
                                let (text, col) = if m.official { ("Official", accent) } else { ("Third-party", v.weak_text_color()) };
                                let g = ui.painter().layout_no_wrap(text.to_string(), rivet_look::fonts::ui_font(11.0), col);
                                let (r, _) = ui.allocate_exact_size(g.size() + Vec2::new(14.0, 6.0), Sense::hover());
                                ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, mix(v.panel_fill, col, 0.6)), egui::StrokeKind::Inside);
                                ui.painter().text(r.center(), Align2::CENTER_CENTER, text, rivet_look::fonts::ui_font(11.0), col);
                            });
                            if !m.description.is_empty() {
                                ui.label(egui::RichText::new(&m.description).size(12.5));
                            }
                            let mut by = Vec::new();
                            if !m.author.is_empty() {
                                by.push(format!("by {}", m.author));
                            }
                            if !m.version.is_empty() {
                                by.push(format!("version {}", m.version));
                            }
                            hint(ui, &by.join(" · "));
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            // One the person installed: it can be removed, and updated when the list has a newer one.
                            if plugin_store::removable(m) {
                                if busy.as_deref() == Some(m.id.as_str()) {
                                    super::controls::button_if(ui, false, super::controls::Icon::Reset, "Working…");
                                    return;
                                }
                                if super::controls::danger_if(ui, busy.is_none(), "Remove…").clicked() {
                                    ask_remove = Some(m.id.clone());
                                }
                                if let Some(l) = plugin_store::update(m, &listed)
                                    && super::controls::button_if(ui, busy.is_none(), super::controls::Icon::Download, &format!("Update to {}", l.version)).clicked()
                                {
                                    install = Some(l.clone());
                                }
                            }
                            if !on {
                                return;
                            }
                            if s.panels.iter().any(|p| p.has(&item)) {
                                ui.label(egui::RichText::new("On a panel").color(v.weak_text_color()).size(12.0));
                            } else if !s.panels.is_empty() && super::controls::button(ui, super::controls::Icon::Plus, "Add to Panel").on_hover_text("Puts its item on the first panel. Move it on the Panels page.").clicked() {
                                s.panels[0].zones[2].insert(0, item.clone());
                            }
                        });
                    });
                    if self.store.removing.as_deref() == Some(m.id.as_str()) {
                        ui.add_space(8.0);
                        ui.label(format!("Remove {}? Its files are deleted. Your settings for it are kept.", m.name));
                        ui.horizontal(|ui| {
                            if super::controls::danger(ui, "Remove Plugin").clicked() {
                                remove = Some((m.id.clone(), m.name.clone()));
                            }
                            if super::controls::button(ui, super::controls::Icon::Back, "Keep It").clicked() {
                                keep = true;
                            }
                        });
                    }
                    // Its own settings, while it's on.
                    if on && !m.settings.is_empty() {
                        ui.add_space(8.0);
                        ui.indent(("shell-plugin-set", &m.id), |ui| {
                            grid(ui, &format!("shell-plugin-set-{}", m.id), &mut |ui| {
                                for set in &m.settings {
                                    let mut value = s.plugin_value(&m.id, &set.key).to_string();
                                    let before = value.clone();
                                    match set.kind {
                                        SettingKind::Toggle => {
                                            ui.label("");
                                            let mut yes = value == "true";
                                            super::controls::switch(ui, &mut yes, &set.label).on_hover_text(&set.hint);
                                            value = if yes { "true".into() } else { String::new() };
                                        }
                                        SettingKind::Text => {
                                            ui.label(&set.label);
                                            ui.add(egui::TextEdit::singleline(&mut value).desired_width(300.0).hint_text(&set.hint));
                                        }
                                        SettingKind::Folder => {
                                            ui.label(&set.label);
                                            ui.horizontal(|ui| {
                                                ui.add(egui::TextEdit::singleline(&mut value).desired_width(300.0).hint_text(&set.hint));
                                                if super::controls::button_if(ui, can_pick, super::controls::Icon::Folder, "Choose…").clicked() {
                                                    ask = Some((m.id.clone(), set.key.clone(), set.label.clone()));
                                                }
                                            });
                                        }
                                    }
                                    if value != before {
                                        s.set_plugin(&m.id, &set.key, value.trim());
                                    }
                                    ui.end_row();
                                }
                            });
                        });
                    }
                });
                ui.add_space(4.0);
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if super::controls::button(ui, super::controls::Icon::Download, "Get Plugins…").clicked() {
                    self.store.open = true;
                }
                if super::controls::button(ui, super::controls::Icon::Folder, "Open Plugins Folder").clicked() {
                    let dir = user_dir();
                    let _ = std::fs::create_dir_all(&dir);
                    crate::sys::spawn("xdg-open", &[&dir.display().to_string()]);
                }
            });
            hint(ui, "A plugin can do anything you can, so turn on only ones you trust.");
        });
        if let Some(id) = ask_remove {
            self.store.removing = Some(id);
        }
        if keep {
            self.store.removing = None;
        }
        if let Some(l) = install.filter(|_| self.shot.is_none()) {
            self.store_install(&l);
        }
        if let Some((id, name)) = remove.filter(|_| self.shot.is_none()) {
            turn_off(&mut self.s, &id);
            let (tx, rx) = channel();
            let id2 = id.clone();
            std::thread::spawn(move || tx.send(plugin_store::remove(&id2).map(|_| format!("{name} is removed."))));
            self.store.removing = None;
            self.store.said = None;
            self.store.job = Some((id, rx));
        }
        if let Some((id, key, title)) = ask {
            let start = Some(self.s.plugin_value(&id, &key)).filter(|v| !v.is_empty()).map(crate::wall::expand).or_else(|| Some(rivet_xdg::places::home()));
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(rivet_xdg::portal::choose(rivet_xdg::portal::Pick::Folder, &title, start.as_deref()));
            });
            self.plugin_pick = Some((id, key, rx));
        }
        section(ui, "Hooks", &mut |ui| {
            hint(ui, "Run a command of your own when one of these happens.");
            ui.add_space(8.0);
            let hooks = &mut self.s.hooks;
            grid(ui, "shell-hooks", &mut |ui| {
                for (name, what, with) in EVENTS {
                    let label = ui.label(*what);
                    // What the command can read: each detail is a RIVET_ variable, then its values if they're fixed.
                    if !with.is_empty() {
                        let (keys, values) = with.split_once(": ").unwrap_or((with, ""));
                        let vars: Vec<String> = keys.split(", ").map(|k| format!("RIVET_{}", k.to_uppercase())).collect();
                        let values = if values.is_empty() { String::new() } else { format!(" ({values})") };
                        label.on_hover_text(format!("Your command can read {}{values}", vars.join(", ")));
                    }
                    let mut cmd = hooks.iter().find(|(e, _)| e == name).map(|(_, c)| c.clone()).unwrap_or_default();
                    if ui.add(egui::TextEdit::singleline(&mut cmd).desired_width(300.0).font(egui::TextStyle::Monospace).hint_text("a command")).changed() {
                        hooks.retain(|(e, _)| e != name);
                        if !cmd.trim().is_empty() {
                            hooks.push((name.to_string(), cmd));
                            hooks.sort();
                        }
                    }
                    ui.end_row();
                }
            });
        });
    }
}
