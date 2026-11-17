//! The quick panels (Rivet's shared one): Go to File (Ctrl+P) finds a workspace file by
//! typing a few letters of its name or path; Commands (Shift+Ctrl+P) runs anything by name; Go
//! to Line (Ctrl+G); and Open Folder / Open File, where a path is typed with its folders listed.

use std::path::{Path, PathBuf};

use eframe::egui;
use rivet_look::quick::{QuickEvent, QuickPanel, QuickRow, QuickText, fuzzy_score};
use rivet_look::theme::Mode;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuickKind {
    Files,
    Commands,
    Line,
    /// Type a path; `files` also lists (and opens) files, else only folders.
    Browse { files: bool },
}

#[derive(Clone, Debug, PartialEq)]
enum Cmd {
    NewFile,
    NewFileHere,
    OpenFolder,
    OpenFile,
    Save,
    SaveAll,
    CloseTab,
    GoToFile,
    GoToLine,
    Sidebar,
    WordWrap,
    LineNumbers,
    StatusBar,
    MenuBar,
    Theme(Mode),
    Bigger,
    Smaller,
    Preferences,
    Reload,
    Recent(PathBuf),
}

struct Action {
    group: &'static str,
    label: String,
    keys: &'static str,
    state: Option<bool>,
    cmd: Cmd,
}

/// `~/x` → the home folder's x.
fn expand(p: &str) -> PathBuf {
    match p.strip_prefix('~') {
        Some(rest) => rivet_xdg::places::home().join(rest.trim_start_matches('/')),
        None => PathBuf::from(p),
    }
}

/// The home folder as `~`.
fn tilde(p: &Path) -> String {
    let home = rivet_xdg::places::home();
    match p.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~/".into(),
        Ok(rest) => format!("~/{}/", rest.display()),
        Err(_) => format!("{}/", p.display().to_string().trim_end_matches('/')),
    }
}

impl EditApp {
    pub(crate) fn open_quick(&mut self, kind: QuickKind) {
        let mut p = QuickPanel::new();
        if let QuickKind::Browse { .. } = kind {
            p.query = tilde(self.root.as_deref().and_then(Path::parent).unwrap_or(&rivet_xdg::places::home()));
        }
        self.quick = Some((kind, p));
    }

    fn actions(&self) -> Vec<Action> {
        let has_tab = !self.tabs.is_empty();
        let has_root = self.root.is_some();
        let mut out = Vec::new();
        let mut add = |group, label: &str, keys, state, cmd, on: bool| {
            if on {
                out.push(Action { group, label: label.to_string(), keys, state, cmd });
            }
        };
        add("File", "Go to File…", "Ctrl+P", None, Cmd::GoToFile, has_root || has_tab);
        add("File", "New File", "Ctrl+N", None, Cmd::NewFile, true);
        add("File", "New File in the Workspace…", "", None, Cmd::NewFileHere, has_root);
        add("File", "Open Folder…", "Ctrl+O", None, Cmd::OpenFolder, true);
        add("File", "Open File…", "", None, Cmd::OpenFile, true);
        add("File", "Save", "Ctrl+S", None, Cmd::Save, has_tab);
        add("File", "Save All", "Shift+Ctrl+S", None, Cmd::SaveAll, has_tab);
        add("File", "Close Tab", "Ctrl+W", None, Cmd::CloseTab, has_tab);
        add("Go", "Go to Line…", "Ctrl+G", None, Cmd::GoToLine, has_tab);
        add("View", "Sidebar", "F9", Some(self.s.show_sidebar), Cmd::Sidebar, true);
        add("View", "Word Wrap", "Alt+Z", Some(self.s.word_wrap), Cmd::WordWrap, true);
        add("View", "Line Numbers", "", Some(self.s.line_numbers), Cmd::LineNumbers, true);
        add("View", "Status Bar", "", Some(self.s.show_statusbar), Cmd::StatusBar, true);
        add("View", "Menu Bar", "Ctrl+M", Some(self.s.show_menubar), Cmd::MenuBar, true);
        add("View", "Bigger Code", "Ctrl+=", None, Cmd::Bigger, true);
        add("View", "Smaller Code", "Ctrl+-", None, Cmd::Smaller, true);
        add("Theme", "Follow System", "", Some(self.s.theme == Mode::System && !self.s.follow_desktop_colors), Cmd::Theme(Mode::System), true);
        add("Theme", "Light", "", Some(self.s.theme == Mode::Light && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Light), true);
        add("Theme", "Dark", "", Some(self.s.theme == Mode::Dark && !self.s.follow_desktop_colors), Cmd::Theme(Mode::Dark), true);
        add("App", "Preferences", "Ctrl+,", None, Cmd::Preferences, true);
        add("Workspace", "Reload Files", "", None, Cmd::Reload, has_root);
        for p in &self.s.recent {
            if Some(p) != self.root.as_ref() {
                out.push(Action { group: "Recent", label: format!("Open {}", tilde(p).trim_end_matches('/')), keys: "", state: None, cmd: Cmd::Recent(p.clone()) });
            }
        }
        out
    }

    fn run(&mut self, cmd: Cmd) {
        match cmd {
            Cmd::NewFile => self.new_tab(),
            Cmd::NewFileHere => self.prompt = Some(Prompt::NewFile { folder: String::new(), name: String::new() }),
            Cmd::OpenFolder => self.open_quick(QuickKind::Browse { files: false }),
            Cmd::OpenFile => self.open_quick(QuickKind::Browse { files: true }),
            Cmd::Save => {
                self.save(self.active);
            }
            Cmd::SaveAll => self.save_all(),
            Cmd::CloseTab => self.close_tab(self.active, false),
            Cmd::GoToFile => self.open_quick(QuickKind::Files),
            Cmd::GoToLine => self.open_quick(QuickKind::Line),
            Cmd::Sidebar => self.s.show_sidebar = !self.s.show_sidebar,
            Cmd::WordWrap => self.s.word_wrap = !self.s.word_wrap,
            Cmd::LineNumbers => self.s.line_numbers = !self.s.line_numbers,
            Cmd::StatusBar => self.s.show_statusbar = !self.s.show_statusbar,
            Cmd::MenuBar => self.s.show_menubar = !self.s.show_menubar,
            Cmd::Theme(m) => {
                self.s.theme = m;
                self.s.follow_desktop_colors = false;
            }
            Cmd::Bigger => self.s.code_size = (self.s.code_size + 1.0).min(32.0),
            Cmd::Smaller => self.s.code_size = (self.s.code_size - 1.0).max(8.0),
            Cmd::Preferences => self.prefs_open = true,
            Cmd::Reload => self.rescan(),
            Cmd::Recent(p) => {
                if p.is_dir() {
                    self.open_folder(p);
                } else {
                    self.open_file(&p);
                }
            }
        }
        self.s.save();
    }

    /// Draw the open quick panel, if any, and act on what's chosen.
    pub(crate) fn quick_panel(&mut self, ctx: &egui::Context) {
        let Some((kind, mut panel)) = self.quick.take() else { return };
        let q = panel.query.trim().to_string();
        let search = self.icon(ctx, &["edit-find-symbolic", "system-search-symbolic"], 18.0);
        let mut keep = true;
        match kind {
            QuickKind::Files => {
                // Workspace files by name, then by path; open tabs (and files outside the
                // workspace) too. With nothing typed: the open tabs first, then the files.
                let root = self.root.clone();
                let mut cands: Vec<(PathBuf, String)> = Vec::new();
                for t in &self.tabs {
                    if let Some(p) = &t.path {
                        cands.push((p.clone(), self.rel(p)));
                    }
                }
                if let Some(r) = &root {
                    for f in &self.files {
                        let p = r.join(f);
                        if !cands.iter().any(|(c, _)| *c == p) {
                            cands.push((p, f.clone()));
                        }
                    }
                }
                let mut scored: Vec<(i32, usize)> = cands
                    .iter()
                    .enumerate()
                    .filter_map(|(i, (_, rel))| {
                        let name = rel.rsplit('/').next().unwrap_or(rel);
                        fuzzy_score(name, &q).max(fuzzy_score(rel, &q).map(|s| s - 150)).map(|s| (s, i))
                    })
                    .collect();
                if !q.is_empty() {
                    scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| cands[a.1].1.len().cmp(&cands[b.1].1.len())));
                }
                scored.truncate(200);
                let rows: Vec<QuickRow> = scored
                    .iter()
                    .map(|(_, i)| {
                        let rel = &cands[*i].1;
                        let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
                        let icon = self.file_icon(ctx, name, 24.0);
                        QuickRow::Item { icon, name: name.to_string(), sub: if dir.is_empty() { self.workspace_name().unwrap_or_default() } else { dir.replace('/', " › ") } }
                    })
                    .collect();
                let empty = if self.root.is_none() && self.tabs.is_empty() { "Open a folder first (Ctrl+O)" } else { "No matching files" };
                let text = QuickText { hint: "Go to a file…", empty, legend: "Up/Down choose   ·   Enter open   ·   Esc close" };
                match panel.show(ctx, "edit-quick-files", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some(p) = scored.get(i).map(|(_, c)| cands[*c].0.clone()) {
                            self.open_file(&p);
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Commands => {
                let mut ranked: Vec<(i32, Action)> = self
                    .actions()
                    .into_iter()
                    .filter_map(|a| {
                        let by_label = fuzzy_score(&a.label, &q);
                        let by_both = fuzzy_score(&format!("{} {}", a.group, a.label), &q).map(|v| v - 200);
                        by_label.max(by_both).map(|v| (v, a))
                    })
                    .collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, a)| QuickRow::Command { group: a.group.to_string(), label: a.label.clone(), keys: a.keys.to_string(), state: a.state }).collect();
                let text = QuickText { hint: "Type a command or option…", empty: "No matching commands", legend: "Up/Down choose   ·   Enter run   ·   Esc close" };
                match panel.show(ctx, "edit-quick-commands", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some((_, a)) = ranked.into_iter().nth(i) {
                            // On / off options keep the list open, to flip several in a row.
                            let stay = a.state.is_some() && !matches!(a.cmd, Cmd::Theme(_));
                            self.quick = None;
                            self.run(a.cmd);
                            if stay && self.quick.is_none() {
                                self.quick = Some((kind, panel));
                            }
                            return;
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Line => {
                let lines = self.tabs.get(self.active).map(|t| t.text.lines().count().max(1)).unwrap_or(1);
                let n = q.parse::<usize>().ok().filter(|n| *n >= 1);
                let rows: Vec<QuickRow> = match n {
                    Some(n) => vec![QuickRow::Command { group: "Go".into(), label: format!("Line {}", n.min(lines)), keys: String::new(), state: None }],
                    None => Vec::new(),
                };
                let hint = format!("A line number, 1 to {lines}…");
                let text = QuickText { hint: &hint, empty: "Type a number", legend: "Enter go   ·   Esc close" };
                match panel.show(ctx, "edit-quick-line", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(_)) => {
                        if let Some(n) = n {
                            self.goto = Some(n.min(lines));
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Browse { files } => {
                // The folder typed so far, and the word after it.
                let (dir_s, word) = match q.rfind('/') {
                    Some(i) => (q[..=i].to_string(), q[i + 1..].to_string()),
                    None => ("~/".to_string(), q.clone()),
                };
                let dir = expand(&dir_s);
                let mut entries: Vec<(bool, String)> = std::fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        let is_dir = e.path().is_dir();
                        (is_dir || files).then_some((is_dir, name))
                    })
                    .filter(|(_, n)| word.starts_with('.') || !n.starts_with('.'))
                    .collect();
                let mut scored: Vec<(i32, (bool, String))> = entries.drain(..).filter_map(|e| fuzzy_score(&e.1, &word).map(|s| (s + if e.0 { 1 } else { 0 }, e))).collect();
                scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| b.1.0.cmp(&a.1.0)).then_with(|| a.1.1.to_lowercase().cmp(&b.1.1.to_lowercase())));
                scored.truncate(200);
                // First row: open the folder typed (folder mode).
                let folder_icon = self.icon(ctx, &["folder"], 24.0);
                let mut rows: Vec<QuickRow> = Vec::new();
                let open_here = !files && word.is_empty() && dir.is_dir();
                if open_here {
                    rows.push(QuickRow::Item { icon: self.icon(ctx, &["folder-open", "folder"], 24.0), name: format!("Open {}", dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir_s.clone())), sub: "As the workspace".into() });
                }
                for (_, (is_dir, name)) in &scored {
                    let icon = if *is_dir { folder_icon.clone() } else { self.file_icon(ctx, name, 24.0) };
                    rows.push(QuickRow::Item { icon, name: if *is_dir { format!("{name}/") } else { name.clone() }, sub: dir_s.trim_end_matches('/').to_string() });
                }
                let hint = if files { "A file to open… (type a path)" } else { "A folder to open… (type a path)" };
                let text = QuickText { hint, empty: "Nothing here matches", legend: "Up/Down choose   ·   Enter open or go in   ·   Esc close" };
                match panel.show(ctx, "edit-quick-browse", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if open_here && i == 0 {
                            self.open_folder(dir.clone());
                            keep = false;
                        } else if let Some((_, (is_dir, name))) = scored.get(i - usize::from(open_here)) {
                            if *is_dir {
                                // Into the folder, to keep typing.
                                panel.query = format!("{dir_s}{name}/");
                                panel.sel = 0;
                                panel.focus = true;
                            } else {
                                self.open_file(&dir.join(name));
                                keep = false;
                            }
                        }
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
        }
        if keep && self.quick.is_none() {
            self.quick = Some((kind, panel));
        }
    }
}
