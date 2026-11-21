//! The quick panel: every command (Ctrl+Shift+P), and the colour themes, shown in the terminal
//! as you move through them.

use eframe::egui;
use rivet_look::quick::{QuickEvent, QuickPanel, QuickRow, QuickText, fuzzy_score};
use rivet_look::theme::Mode;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuickKind {
    Commands,
    Themes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Cmd {
    NewTab,
    NewPane,
    NewWindow,
    Detach,
    MoveTo(egui::ViewportId),
    ClosePane,
    CloseTab,
    NextTab,
    PrevTab,
    NextPane,
    PrevPane,
    NextLayout,
    Layout(Layout),
    Zoom,
    Copy,
    Paste,
    SelectAll,
    Find,
    Clear,
    Bigger,
    Smaller,
    UsualSize,
    Themes,
    Theme(Mode),
    MenuBar,
    CopyOnSelect,
    Preferences,
    Keyboard,
}

struct Action {
    group: &'static str,
    label: String,
    keys: &'static str,
    state: Option<bool>,
    cmd: Cmd,
}

impl TermApp {
    pub(crate) fn open_quick(&mut self, kind: QuickKind) {
        self.w.quick = Some((kind, QuickPanel::new()));
    }

    fn actions(&self) -> Vec<Action> {
        let layout = self.w.tabs.get(self.w.active).map(|t| t.layout).unwrap_or_default();
        let mut out = Vec::new();
        let mut add = |group, label: &str, keys, state, cmd| out.push(Action { group, label: label.to_string(), keys, state, cmd });
        add("File", "New Tab", "Ctrl+Shift+T", None, Cmd::NewTab);
        add("File", "New Pane", "Ctrl+Shift+Enter", None, Cmd::NewPane);
        add("File", "New Window", "Ctrl+Shift+N", None, Cmd::NewWindow);
        if self.w.panes() > 1 {
            add("File", "Detach Pane", "", None, Cmd::Detach);
        }
        for (vp, name) in self.window_names() {
            add("File", &format!("Move Pane To {name}"), "", None, Cmd::MoveTo(vp));
        }
        add("File", "Close Pane", "Ctrl+Shift+W", None, Cmd::ClosePane);
        add("File", "Close Tab", "Ctrl+Shift+Q", None, Cmd::CloseTab);
        add("Go", "Next Tab", "Ctrl+Shift+Right", None, Cmd::NextTab);
        add("Go", "Previous Tab", "Ctrl+Shift+Left", None, Cmd::PrevTab);
        add("Go", "Next Pane", "Ctrl+Shift+]", None, Cmd::NextPane);
        add("Go", "Previous Pane", "Ctrl+Shift+[", None, Cmd::PrevPane);
        add("Layout", "Next Layout", "Ctrl+Shift+L", None, Cmd::NextLayout);
        for l in Layout::ALL {
            add("Layout", l.name(), "", Some(layout == l), Cmd::Layout(l));
        }
        add("Layout", "One Pane at a Time", "Ctrl+Shift+Z", Some(layout == Layout::One), Cmd::Zoom);
        add("Edit", "Copy", "Ctrl+Shift+C", None, Cmd::Copy);
        add("Edit", "Paste", "Ctrl+Shift+V", None, Cmd::Paste);
        add("Edit", "Select All", "Ctrl+Shift+A", None, Cmd::SelectAll);
        add("Edit", "Find in Scrollback", "Ctrl+Shift+F", None, Cmd::Find);
        add("Edit", "Clear Scrollback", "Ctrl+Shift+K", None, Cmd::Clear);
        add("Edit", "Copy on Select", "", Some(self.s.copy_on_select), Cmd::CopyOnSelect);
        add("View", "Bigger Text", "Ctrl+Shift+=", None, Cmd::Bigger);
        add("View", "Smaller Text", "Ctrl+Shift+-", None, Cmd::Smaller);
        add("View", "Usual Text Size", "Ctrl+Shift+Backspace", None, Cmd::UsualSize);
        add("View", "Color Theme…", "", None, Cmd::Themes);
        add("View", "Menu Bar", "Ctrl+Shift+M", Some(self.s.show_menubar), Cmd::MenuBar);
        add("Theme", "Follow System", "", Some(self.s.theme == Mode::System && !self.s.follow_desktop_colors), Cmd::Theme(Mode::System));
        add("Theme", "Light", "", Some(self.s.theme == Mode::Light && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Light));
        add("Theme", "Dark", "", Some(self.s.theme == Mode::Dark && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Dark));
        add("Edit", "Preferences", "Ctrl+,", None, Cmd::Preferences);
        add("App", "Keyboard Shortcuts", "", None, Cmd::Keyboard);
        out
    }

    fn run(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::NewTab => {
                let h = self.here();
                self.new_tab(h);
            }
            Cmd::NewPane => self.split(),
            Cmd::NewWindow => self.new_window(),
            Cmd::Detach => self.send_pane(Dest::NewWindow),
            Cmd::MoveTo(vp) => self.send_pane(Dest::Window(vp)),
            Cmd::ClosePane => {
                let f = self.w.tabs.get(self.w.active).map(|t| t.focus).unwrap_or(0);
                self.close_pane(self.w.active, f, false);
            }
            Cmd::CloseTab => self.close_tab(self.w.active, false),
            Cmd::NextTab => self.go_tab(1),
            Cmd::PrevTab => self.go_tab(-1),
            Cmd::NextPane => self.focus_pane(1),
            Cmd::PrevPane => self.focus_pane(-1),
            Cmd::NextLayout => self.next_layout(),
            Cmd::Layout(l) => self.set_layout(l),
            Cmd::Zoom => self.toggle_zoom(),
            Cmd::Copy => self.copy(),
            Cmd::Paste => self.paste(),
            Cmd::SelectAll => self.select_all(),
            Cmd::Find => self.open_find(),
            Cmd::Clear => self.clear_scrollback(),
            Cmd::Bigger => self.zoom = (self.zoom + 1.0).min(30.0),
            Cmd::Smaller => self.zoom = (self.zoom - 1.0).max(-10.0),
            Cmd::UsualSize => self.zoom = 0.0,
            Cmd::Themes => self.open_quick(QuickKind::Themes),
            Cmd::Theme(m) => {
                self.s.theme = m;
                self.s.follow_desktop_colors = false;
            }
            Cmd::MenuBar => self.s.show_menubar = !self.s.show_menubar,
            Cmd::CopyOnSelect => self.s.copy_on_select = !self.s.copy_on_select,
            Cmd::Preferences => self.w.prefs_open = true,
            Cmd::Keyboard => {
                self.w.prefs_page = super::prefs::Page::Keyboard;
                self.w.prefs_open = true;
            }
        }
        self.s.save();
        self.w.refocus = true;
    }

    /// Draw the open quick panel, if any, and act on what's chosen.
    pub(crate) fn quick_panel(&mut self, ctx: &egui::Context) {
        let Some((kind, mut panel)) = self.w.quick.take() else { return };
        let q = panel.query.trim().to_string();
        let search = self.icon(ctx, &["edit-find-symbolic", "system-search-symbolic"], 18.0);
        let mut keep = true;
        match kind {
            QuickKind::Commands => {
                let mut ranked: Vec<(i32, Action)> = self.actions().into_iter().filter_map(|a| fuzzy_score(&a.label, &q).max(fuzzy_score(&format!("{} {}", a.group, a.label), &q).map(|v| v - 200)).map(|v| (v, a))).collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, a)| QuickRow::Command { group: a.group.to_string(), label: a.label.clone(), keys: a.keys.to_string(), state: a.state }).collect();
                let text = QuickText { hint: "Run a command…", empty: "No matching commands", legend: "Up/Down choose   ·   Enter run   ·   Esc close" };
                match panel.show(ctx, "term-quick-commands", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some((_, a)) = ranked.into_iter().nth(i) {
                            self.run(a.cmd);
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Themes => {
                let mut names: Vec<(String, &'static str)> = vec![(String::new(), "Rivet")];
                names.extend(rivet_look::schemes::SCHEMES.iter().filter(|s| s.dark).map(|s| (s.name.to_string(), "Dark")));
                names.extend(rivet_look::schemes::SCHEMES.iter().filter(|s| !s.dark).map(|s| (s.name.to_string(), "Light")));
                let shown = |n: &str| if n.is_empty() { "Follow the App's Colors".to_string() } else { n.to_string() };
                let mut ranked: Vec<(i32, usize)> = names.iter().enumerate().filter_map(|(i, (n, g))| fuzzy_score(&shown(n), &q).max(fuzzy_score(&format!("{g} {}", shown(n)), &q).map(|v| v - 200)).map(|v| (v, i))).collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let current = self.s.scheme.clone();
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, i)| QuickRow::Command { group: names[*i].1.to_string(), label: shown(&names[*i].0), keys: String::new(), state: Some(names[*i].0 == current) }).collect();
                let text = QuickText { hint: "Choose a color theme for the terminal…", empty: "No matching themes", legend: "Up/Down preview   ·   Enter choose   ·   Esc keep the current one" };
                let ev = panel.show(ctx, "term-quick-themes", &rows, &text, search.as_ref());
                let hi = ranked.get(panel.sel).map(|(_, i)| names[*i].0.clone());
                if self.scheme_preview != hi {
                    self.scheme_preview = hi;
                    ctx.request_repaint();
                }
                match ev {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some((_, k)) = ranked.get(i) {
                            self.s.scheme = names[*k].0.clone();
                            self.s.save();
                        }
                        self.scheme_preview = None;
                        keep = false;
                    }
                    Some(QuickEvent::Close) => {
                        self.scheme_preview = None;
                        keep = false;
                    }
                    None => {}
                }
            }
        }
        if keep && self.w.quick.is_none() {
            self.w.quick = Some((kind, panel));
        } else if !keep {
            self.w.refocus = true;
        }
    }
}
