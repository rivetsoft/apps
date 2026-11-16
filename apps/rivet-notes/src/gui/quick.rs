//! Quick Open (Ctrl+P): jump to any note in the vault by typing part of its title or path.
//! Quick Actions (Shift+Ctrl+P): every command and option, by name. Both use Rivet's shared
//! quick panel, the same one Rivet Files has.

use eframe::egui;
use rivet_look::quick::{QuickEvent, QuickPanel, QuickRow, QuickText, fuzzy_score};
use rivet_look::theme::Mode;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum QuickKind {
    Open,
    Actions,
}

/// A command in Quick Actions.
#[derive(Clone, Debug, PartialEq)]
enum Cmd {
    NewNote,
    NewFolder,
    DailyNote,
    Reading,
    Sidebar,
    Peek,
    Properties,
    Breadcrumbs,
    Menubar,
    Statusbar,
    Centered,
    Theme(Mode),
    Preferences,
    Reload,
    SwitchVault(PathBuf),
    OpenVault,
}

struct Action {
    group: &'static str,
    label: String,
    keys: &'static str,
    state: Option<bool>,
    cmd: Cmd,
}

impl NotesApp {
    /// Open or close a quick panel (opening one closes the other).
    pub(super) fn toggle_quick(&mut self, kind: QuickKind) {
        match &self.quick {
            Some((k, _)) if *k == kind => self.quick = None,
            _ => self.quick = Some((kind, QuickPanel::new())),
        }
    }

    fn actions(&self) -> Vec<Action> {
        let has_vault = self.vault.is_some();
        let has_doc = self.doc.is_some();
        let mut out = Vec::new();
        let mut add = |group, label: &str, keys, state, cmd, on: bool| {
            if on {
                out.push(Action { group, label: label.to_string(), keys, state, cmd });
            }
        };
        add("Note", "New Note", "Ctrl+N", None, Cmd::NewNote, has_vault);
        add("Note", "New Folder", "Shift+Ctrl+N", None, Cmd::NewFolder, has_vault);
        add("Note", "Daily Note", "Ctrl+D", None, Cmd::DailyNote, has_vault && self.s.daily_notes);
        add("View", "Reading View", "Ctrl+Space", None, Cmd::Reading, has_doc);
        add("View", "Sidebar", "F9", Some(self.s.show_sidebar), Cmd::Sidebar, true);
        add("View", "Peek", "F11", Some(self.s.show_peek), Cmd::Peek, true);
        add("View", "Properties", "", Some(self.s.show_properties), Cmd::Properties, true);
        add("View", "Breadcrumbs", "", Some(self.s.show_breadcrumbs), Cmd::Breadcrumbs, true);
        add("View", "Menu Bar", "Ctrl+M", Some(self.s.show_menubar), Cmd::Menubar, true);
        add("View", "Status Bar", "", Some(self.s.show_statusbar), Cmd::Statusbar, true);
        add("View", "Center Notes by Default", "", Some(self.s.centered), Cmd::Centered, true);
        add("Theme", "Follow System", "", Some(self.s.theme == Mode::System && !self.s.follow_desktop_colors), Cmd::Theme(Mode::System), true);
        add("Theme", "Light", "", Some(self.s.theme == Mode::Light && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Light), true);
        add("Theme", "Dark", "", Some(self.s.theme == Mode::Dark && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Dark), true);
        add("App", "Preferences", "Ctrl+,", None, Cmd::Preferences, true);
        add("Vault", "Reload Vault", "F5", None, Cmd::Reload, has_vault);
        add("Vault", "Open Folder as Vault…", "", None, Cmd::OpenVault, true);
        for p in &self.s.vaults {
            if Some(p) != self.vault.as_ref() {
                let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                out.push(Action { group: "Vault", label: format!("Switch to {name}"), keys: "", state: None, cmd: Cmd::SwitchVault(p.clone()) });
            }
        }
        out
    }

    /// Draw the open quick panel, if any, and act on what's chosen.
    pub(super) fn quick_panel(&mut self, ctx: &egui::Context) {
        let Some((kind, mut panel)) = self.quick.take() else { return };
        let q = panel.query.trim().to_string();
        let search = self.icon(ctx, &["edit-find-symbolic", "system-search-symbolic"], 18.0);
        let event = match kind {
            QuickKind::Open => {
                let icon = self.icon(ctx, &["text-markdown", "text-x-markdown", "text-x-generic"], 24.0);
                // Notes ranked by title, then by path ("proj rivet"); with nothing typed, the
                // most recently edited first.
                let mut scored: Vec<(i32, &Note)> = self
                    .notes
                    .iter()
                    .filter_map(|n| {
                        let by_title = fuzzy_score(&n.title, &q);
                        let by_path = fuzzy_score(&n.rel, &q).map(|s| s - 200);
                        by_title.max(by_path).map(|s| (s, n))
                    })
                    .collect();
                if q.is_empty() {
                    scored.sort_by_key(|s| std::cmp::Reverse(s.1.modified));
                } else {
                    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.title.to_lowercase().cmp(&b.1.title.to_lowercase())));
                }
                scored.truncate(200);
                let paths: Vec<PathBuf> = scored.iter().map(|(_, n)| n.path.clone()).collect();
                let rows: Vec<QuickRow> = scored
                    .iter()
                    .map(|(_, n)| {
                        let folder = if n.folder().is_empty() { "Top of the vault".to_string() } else { n.folder().replace('/', " › ") };
                        let tags = n.tags.iter().take(4).map(|t| format!("#{t}")).collect::<Vec<_>>().join(" ");
                        QuickRow::Item { icon: icon.clone(), name: n.title.clone(), sub: if tags.is_empty() { folder } else { format!("{folder}   ·   {tags}") } }
                    })
                    .collect();
                let text = QuickText { hint: "Go to a note…", empty: "No matching notes", legend: "Up/Down choose   ·   Enter open   ·   Esc close" };
                match panel.show(ctx, "notes-quick-open", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some(p) = paths.get(i) {
                            let folder = self.notes.iter().find(|n| n.path == *p).map(|n| n.folder().to_string()).unwrap_or_default();
                            self.open_note(p);
                            self.s.collapsed.retain(|k| !(folder == *k || folder.starts_with(&format!("{k}/"))));
                            self.current_folder = folder;
                        }
                        Some(QuickEvent::Close)
                    }
                    e => e,
                }
            }
            QuickKind::Actions => {
                let all = self.actions();
                let mut ranked: Vec<(i32, Action)> = all
                    .into_iter()
                    .filter_map(|a| {
                        let by_label = fuzzy_score(&a.label, &q);
                        // "view peek", "theme dark": the group's name counts too.
                        let by_both = fuzzy_score(&format!("{} {}", a.group, a.label), &q).map(|v| v - 200);
                        by_label.max(by_both).map(|v| (v, a))
                    })
                    .collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, a)| QuickRow::Command { group: a.group.to_string(), label: a.label.clone(), keys: a.keys.to_string(), state: a.state }).collect();
                let text = QuickText { hint: "Type a command or option…", empty: "No matching commands", legend: "Up/Down choose   ·   Enter run   ·   Esc close" };
                match panel.show(ctx, "notes-quick-actions", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => match ranked.get(i) {
                        // On / off options keep the list open, to flip several in a row.
                        Some((_, a)) if a.state.is_some() && !matches!(a.cmd, Cmd::Theme(_)) => {
                            self.run(ctx, a.cmd.clone());
                            None
                        }
                        Some((_, a)) => {
                            let c = a.cmd.clone();
                            self.quick = None;
                            self.run(ctx, c);
                            return;
                        }
                        None => None,
                    },
                    e => e,
                }
            }
        };
        if event != Some(QuickEvent::Close) && self.quick.is_none() {
            self.quick = Some((kind, panel));
        }
    }

    fn run(&mut self, ctx: &egui::Context, cmd: Cmd) {
        match cmd {
            Cmd::NewNote => self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() }),
            Cmd::NewFolder => self.prompt = Some(Prompt::NewFolder { parent: self.current_folder.clone(), name: String::new() }),
            Cmd::DailyNote => self.daily_note(),
            Cmd::Reading => self.toggle_reading(ctx),
            Cmd::Sidebar => self.s.show_sidebar = !self.s.show_sidebar,
            Cmd::Peek => self.s.show_peek = !self.s.show_peek,
            Cmd::Properties => self.s.show_properties = !self.s.show_properties,
            Cmd::Breadcrumbs => self.s.show_breadcrumbs = !self.s.show_breadcrumbs,
            Cmd::Menubar => self.s.show_menubar = !self.s.show_menubar,
            Cmd::Statusbar => self.s.show_statusbar = !self.s.show_statusbar,
            Cmd::Centered => self.s.centered = !self.s.centered,
            Cmd::Theme(m) => {
                self.s.theme = m;
                self.s.follow_desktop_colors = false;
            }
            Cmd::Preferences => self.prefs_open = true,
            Cmd::Reload => self.rescan(),
            Cmd::SwitchVault(p) => self.open_vault(p),
            Cmd::OpenVault => self.prompt = Some(Prompt::OpenVault(String::new())),
        }
        self.s.save();
    }
}
