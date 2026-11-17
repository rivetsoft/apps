//! The quick panels (Rivet's shared one): Go to File (Ctrl+P) finds a workspace file by
//! typing a few letters of its name or path; Commands (Shift+Ctrl+P) runs anything by name; Go
//! to Line (Ctrl+G); and Open Folder / Open File, where a path is typed with its folders listed.

use std::path::{Path, PathBuf};

use eframe::egui;
use rivet_look::quick::{QuickEvent, QuickPanel, QuickRow, QuickText, fuzzy_score};
use rivet_look::theme::Mode;

use super::*;

/// At most this many files are listed in Go to File (the best matches).
const SHOWN: usize = 100;

/// Go to File's candidates and their ranking for a query, kept until the query, the file list
/// or the open tabs change.
#[derive(Default)]
pub(crate) struct FileRank {
    query: String,
    files_gen: u64,
    tabs: Vec<Option<PathBuf>>,
    /// Every file: its path and its path inside the workspace.
    pub cands: Vec<(PathBuf, String)>,
    /// Indexes into `cands`, best first.
    pub ranked: Vec<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum QuickKind {
    Files,
    Commands,
    Line,
    /// The editor's color schemes, previewed while highlighted.
    Themes,
    /// Definitions or uses found (Go to Definition with several, Find References).
    Places,
    /// The open tab's language: told from the file, plain text, or one chosen.
    Language,
    /// The open tab's indentation: tabs or spaces, how many, convert, detect.
    Indent,
    /// Type a path; `files` also lists (and opens) files, else only folders; `window` opens the
    /// folder chosen in a new window.
    Browse { files: bool, window: bool },
}

#[derive(Clone, Debug, PartialEq)]
enum Cmd {
    NewFile,
    NewFileHere,
    OpenFolder,
    NewWindow,
    OpenFolderNewWindow,
    OpenFile,
    Save,
    SaveAll,
    CloseTab,
    GoToFile,
    GoToLine,
    Find,
    Replace,
    SearchFiles,
    Sidebar,
    WordWrap,
    LineNumbers,
    StatusBar,
    MenuBar,
    Theme(Mode),
    Bigger,
    Smaller,
    Preferences,
    Themes,
    Language,
    Indentation,
    VimKeys,
    Nav(super::navigate::NavCmd),
    ConvertIndent(bool),
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

    /// Rank the files for `q`, reusing the last ranking when nothing changed, and narrowing it
    /// when the query only grew (what didn't match "mai" won't match "main").
    pub(crate) fn rank_files(&mut self, q: &str) {
        let tabs: Vec<Option<PathBuf>> = self.tabs.iter().map(|t| t.path.clone()).collect();
        let mut rank = self.file_rank.take().unwrap_or_default();
        let same_files = rank.files_gen == self.files_gen && rank.tabs == tabs && !rank.cands.is_empty();
        if same_files && rank.query == q {
            self.file_rank = Some(rank);
            return;
        }
        if !same_files {
            let mut cands: Vec<(PathBuf, String)> = Vec::new();
            for p in tabs.iter().flatten() {
                cands.push((p.clone(), self.rel(p)));
            }
            if let Some(r) = &self.root {
                let open: std::collections::HashSet<&PathBuf> = tabs.iter().flatten().collect();
                for f in &self.files {
                    let p = r.join(f);
                    if !open.contains(&p) {
                        cands.push((p, f.clone()));
                    }
                }
            }
            rank.cands = cands;
            rank.ranked.clear();
            rank.query.clear();
        }
        // Narrow the last matches when the query grew; else look at every file.
        let pool: Vec<usize> = if same_files && !rank.query.is_empty() && q.starts_with(rank.query.as_str()) {
            std::mem::take(&mut rank.ranked)
        } else {
            (0..rank.cands.len()).collect()
        };
        let mut scored: Vec<(i32, usize)> = pool
            .into_iter()
            .filter_map(|i| {
                let rel = &rank.cands[i].1;
                let name = rel.rsplit('/').next().unwrap_or(rel);
                fuzzy_score(name, q).max(fuzzy_score(rel, q).map(|s| s - 150)).map(|s| (s, i))
            })
            .collect();
        if !q.is_empty() {
            scored.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| rank.cands[a.1].1.len().cmp(&rank.cands[b.1].1.len())));
        }
        rank.ranked = scored.into_iter().map(|(_, i)| i).collect();
        rank.query = q.to_string();
        rank.files_gen = self.files_gen;
        rank.tabs = tabs;
        self.file_rank = Some(rank);
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
        add("File", "New Window", "Shift+Ctrl+N", None, Cmd::NewWindow, true);
        add("File", "Open Folder in New Window…", "", None, Cmd::OpenFolderNewWindow, true);
        add("File", "Open File…", "", None, Cmd::OpenFile, true);
        add("File", "Save", "Ctrl+S", None, Cmd::Save, has_tab);
        add("File", "Save All", "Shift+Ctrl+S", None, Cmd::SaveAll, has_tab);
        add("File", "Close Tab", "Ctrl+W", None, Cmd::CloseTab, has_tab);
        add("Go", "Go to Line…", "Ctrl+G", None, Cmd::GoToLine, has_tab);
        add("Go", "Go to Definition", "F12", None, Cmd::Nav(super::navigate::NavCmd::Definition), has_tab);
        add("Go", "Find References", "Shift+F12", None, Cmd::Nav(super::navigate::NavCmd::References), has_tab);
        add("Go", "Show What It Is (Hover)", "", None, Cmd::Nav(super::navigate::NavCmd::Hover), has_tab);
        add("Go", "Back", "Alt+Left", None, Cmd::Nav(super::navigate::NavCmd::Back), has_tab);
        add("Go", "Forward", "Alt+Right", None, Cmd::Nav(super::navigate::NavCmd::Forward), has_tab);
        add("Edit", "Find", "Ctrl+F", None, Cmd::Find, has_tab);
        add("Edit", "Replace", "Ctrl+H", None, Cmd::Replace, has_tab);
        add("Edit", "Search in Folder", "Shift+Ctrl+F", None, Cmd::SearchFiles, has_root);
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
        add("Editor", "Color Theme…", "", None, Cmd::Themes, true);
        add("Editor", "Change Language…", "", None, Cmd::Language, has_tab);
        add("Editor", "Vim Keys", "", Some(self.s.vim_keys), Cmd::VimKeys, true);
        add("Editor", "Indentation…", "", None, Cmd::Indentation, has_tab);
        add("Editor", "Convert Indentation to Spaces", "", None, Cmd::ConvertIndent(false), has_tab);
        add("Editor", "Convert Indentation to Tabs", "", None, Cmd::ConvertIndent(true), has_tab);
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
            Cmd::OpenFolder => self.open_quick(QuickKind::Browse { files: false, window: false }),
            Cmd::NewWindow => self.new_window(None),
            Cmd::OpenFolderNewWindow => self.open_quick(QuickKind::Browse { files: false, window: true }),
            Cmd::OpenFile => self.open_quick(QuickKind::Browse { files: true, window: false }),
            Cmd::Save => {
                self.save(self.active);
            }
            Cmd::SaveAll => self.save_all(),
            Cmd::CloseTab => self.close_tab(self.active, false),
            Cmd::GoToFile => self.open_quick(QuickKind::Files),
            Cmd::GoToLine => self.open_quick(QuickKind::Line),
            Cmd::Find => self.find_pending = Some(false),
            Cmd::Replace => self.find_pending = Some(true),
            Cmd::SearchFiles => self.open_search(),
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
            Cmd::Themes => self.open_quick(QuickKind::Themes),
            Cmd::Language => self.open_quick(QuickKind::Language),
            Cmd::Nav(c) => self.nav.pending = Some(c),
            Cmd::VimKeys => {
                self.s.vim_keys = !self.s.vim_keys;
                self.vim = Default::default();
            }
            Cmd::Indentation => self.open_quick(QuickKind::Indent),
            Cmd::ConvertIndent(tabs) => self.convert_indent(tabs),
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

    /// Rewrite the open file's indentation as tabs or spaces (and indent with them from now on).
    pub(crate) fn convert_indent(&mut self, tabs: bool) {
        let Some(t) = self.tabs.get_mut(self.active) else { return };
        let new = crate::ops::convert_indent(&t.text, tabs, t.indent.width);
        t.indent.tabs = tabs;
        t.sels.clear();
        let changed = new != t.text;
        t.text = new;
        if changed {
            t.preview = false;
        }
        self.note = Some((if changed { format!("Indentation is now {}", if tabs { "tabs" } else { "spaces" }) } else { "The indentation was already like that".into() }, false));
    }

    /// Draw the open quick panel, if any, and act on what's chosen.
    pub(crate) fn quick_panel(&mut self, ctx: &egui::Context) {
        let Some((kind, mut panel)) = self.quick.take() else { return };
        let q = panel.query.trim().to_string();
        let search = self.icon(ctx, &["edit-find-symbolic", "system-search-symbolic"], 18.0);
        let mut keep = true;
        match kind {
            QuickKind::Files => {
                self.rank_files(&q);
                let rank = self.file_rank.take().unwrap_or_default();
                // Only the rows shown are drawn (icons are cached per kind of file).
                let rows: Vec<QuickRow> = rank
                    .ranked
                    .iter()
                    .take(SHOWN)
                    .map(|i| {
                        let rel = &rank.cands[*i].1;
                        let (dir, name) = rel.rsplit_once('/').unwrap_or(("", rel));
                        let icon = self.file_icon(ctx, name, 24.0);
                        QuickRow::Item { icon, name: name.to_string(), sub: if dir.is_empty() { self.workspace_name().unwrap_or_default() } else { dir.replace('/', " › ") } }
                    })
                    .collect();
                let empty = if self.root.is_none() && self.tabs.is_empty() {
                    "Open a folder first (Ctrl+O)"
                } else if self.scan_running() && self.files.is_empty() {
                    "Listing the folder's files…"
                } else {
                    "No matching files"
                };
                let text = QuickText { hint: "Go to a file…", empty, legend: "Up/Down choose   ·   Enter open   ·   Esc close" };
                let picked = match panel.show(ctx, "edit-quick-files", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        keep = false;
                        rank.ranked.get(i).map(|c| rank.cands[*c].0.clone())
                    }
                    Some(QuickEvent::Close) => {
                        keep = false;
                        None
                    }
                    None => None,
                };
                self.file_rank = Some(rank);
                if let Some(p) = picked {
                    self.preview_file(&p);
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
            QuickKind::Themes => {
                // "Follow the app", then every scheme, dark ones first; the highlighted one is
                // shown in the editor straight away (Esc puts the old one back).
                let mut names: Vec<(String, &'static str)> = vec![(String::new(), "Rivet")];
                names.extend(crate::schemes::SCHEMES.iter().filter(|s| s.dark).map(|s| (s.name.to_string(), "Dark")));
                names.extend(crate::schemes::SCHEMES.iter().filter(|s| !s.dark).map(|s| (s.name.to_string(), "Light")));
                let shown_name = |n: &str| if n.is_empty() { "Follow the App's Colors".to_string() } else { n.to_string() };
                let mut ranked: Vec<(i32, usize)> = names.iter().enumerate().filter_map(|(i, (n, g))| fuzzy_score(&shown_name(n), &q).max(fuzzy_score(&format!("{g} {}", shown_name(n)), &q).map(|v| v - 200)).map(|v| (v, i))).collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let current = self.s.code_scheme.clone();
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, i)| QuickRow::Command { group: names[*i].1.to_string(), label: shown_name(&names[*i].0), keys: String::new(), state: Some(names[*i].0 == current) }).collect();
                let text = QuickText { hint: "Choose an editor color theme…", empty: "No matching themes", legend: "Up/Down preview   ·   Enter choose   ·   Esc keep the current one" };
                let ev = panel.show(ctx, "edit-quick-themes", &rows, &text, search.as_ref());
                // Preview the highlighted one.
                let hi = ranked.get(panel.sel).map(|(_, i)| names[*i].0.clone());
                if self.scheme_preview != hi {
                    self.scheme_preview = hi;
                    ctx.request_repaint();
                }
                match ev {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some((_, k)) = ranked.get(i) {
                            self.s.code_scheme = names[*k].0.clone();
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
            QuickKind::Places => {
                // Filter by file name or line text.
                let ranked: Vec<usize> = (0..self.nav.list.len())
                    .filter(|i| {
                        q.is_empty() || {
                            let s = &self.nav.list[*i];
                            fuzzy_score(&format!("{} {}", s.path.to_string_lossy(), s.preview), &q).is_some()
                        }
                    })
                    .collect();
                let rows: Vec<QuickRow> = self.spot_rows(ctx, &ranked);
                let title = self.nav.list_title.clone();
                let text = QuickText { hint: &title, empty: "Nothing matches", legend: "Up/Down choose   ·   Enter go there   ·   Esc close" };
                match panel.show(ctx, "edit-quick-places", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some(s) = ranked.get(i).and_then(|k| self.nav.list.get(*k)).cloned() {
                            self.jump_to(s.path, s.ch);
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Language => {
                use crate::syntax::{Lang, Pick};
                let Some(t) = self.tabs.get(self.active) else { return };
                let found = crate::syntax::shown_name(&t.name(), &t.text, Pick::Auto);
                let now = t.lang;
                let mut picks: Vec<(Pick, String, &str)> = vec![(Pick::Auto, format!("Auto-detect ({found})"), "File"), (Pick::Plain, "Plain Text".into(), "File")];
                picks.extend(Lang::ALL.iter().map(|l| (Pick::Lang(*l), l.name().to_string(), "Language")));
                let mut ranked: Vec<(i32, usize)> = picks.iter().enumerate().filter_map(|(i, (_, label, _))| fuzzy_score(label, &q).map(|v| (v, i))).collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, i)| QuickRow::Command { group: picks[*i].2.to_string(), label: picks[*i].1.clone(), keys: String::new(), state: Some(picks[*i].0 == now) }).collect();
                let text = QuickText { hint: "Show this file as…", empty: "No matching languages", legend: "Enter choose   ·   Esc close" };
                match panel.show(ctx, "edit-quick-lang", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let (Some((_, k)), Some(t)) = (ranked.get(i), self.tabs.get_mut(self.active)) {
                            t.lang = picks[*k].0;
                        }
                        keep = false;
                    }
                    Some(QuickEvent::Close) => keep = false,
                    None => {}
                }
            }
            QuickKind::Indent => {
                let Some(t) = self.tabs.get(self.active) else { return };
                let now = t.indent;
                let found = crate::ops::detect_indent(&t.text);
                #[derive(Clone, Copy)]
                enum Do {
                    Tabs(bool),
                    Width(usize),
                    Detect,
                    Convert(bool),
                }
                let mut opts: Vec<(Do, String, &str, Option<bool>)> = vec![
                    (Do::Tabs(false), "Indent Using Spaces".into(), "Tab key", Some(!now.tabs)),
                    (Do::Tabs(true), "Indent Using Tabs".into(), "Tab key", Some(now.tabs)),
                ];
                for w in [2usize, 3, 4, 8] {
                    opts.push((Do::Width(w), format!("{}: {w}", if now.tabs { "Tab Size" } else { "Spaces" }), "Width", Some(now.width == w)));
                }
                let told = match found {
                    Some(i) if i.tabs => " (tabs)".to_string(),
                    Some(i) => format!(" ({} spaces)", i.width),
                    None => " (not indented yet)".to_string(),
                };
                opts.push((Do::Detect, format!("Detect From the File{told}"), "File", None));
                opts.push((Do::Convert(false), "Convert Indentation to Spaces".into(), "File", None));
                opts.push((Do::Convert(true), "Convert Indentation to Tabs".into(), "File", None));
                let mut ranked: Vec<(i32, usize)> = opts.iter().enumerate().filter_map(|(i, o)| fuzzy_score(&o.1, &q).map(|v| (v, i))).collect();
                if !q.is_empty() {
                    ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
                }
                let rows: Vec<QuickRow> = ranked.iter().map(|(_, i)| QuickRow::Command { group: opts[*i].2.to_string(), label: opts[*i].1.clone(), keys: String::new(), state: opts[*i].3 }).collect();
                let text = QuickText { hint: "Indentation for this file…", empty: "Nothing matches", legend: "Enter choose   ·   Esc close" };
                match panel.show(ctx, "edit-quick-indent", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if let Some((_, k)) = ranked.get(i) {
                            match opts[*k].0 {
                                Do::Tabs(tabs) => {
                                    if let Some(t) = self.tabs.get_mut(self.active) {
                                        t.indent.tabs = tabs;
                                    }
                                }
                                Do::Width(w) => {
                                    if let Some(t) = self.tabs.get_mut(self.active) {
                                        t.indent.width = w;
                                    }
                                }
                                Do::Detect => {
                                    let text = self.tabs.get(self.active).map(|t| t.text.clone()).unwrap_or_default();
                                    let ind = self.indent_for(&text);
                                    let ind = if self.s.detect_indent { ind } else { crate::ops::detect_indent(&text).unwrap_or(ind) };
                                    if let Some(t) = self.tabs.get_mut(self.active) {
                                        t.indent = ind;
                                    }
                                }
                                Do::Convert(tabs) => self.convert_indent(tabs),
                            }
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
            QuickKind::Browse { files, window } => {
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
                let folder_icon = self.folder_icon(ctx, "", false, 24.0);
                let mut rows: Vec<QuickRow> = Vec::new();
                let open_here = !files && word.is_empty() && dir.is_dir();
                // Tab fills in the highlighted folder or file (the best match when the
                // "Open" row is highlighted), to keep typing below it.
                if !scored.is_empty() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab)) {
                    let k = if open_here { panel.sel.saturating_sub(1) } else { panel.sel }.min(scored.len() - 1);
                    let (is_dir, name) = &scored[k].1;
                    panel.query = if *is_dir { format!("{dir_s}{name}/") } else { format!("{dir_s}{name}") };
                    panel.sel = 0;
                    panel.focus = true;
                    ctx.request_repaint();
                }
                if open_here {
                    rows.push(QuickRow::Item { icon: self.folder_icon(ctx, "", true, 24.0), name: format!("Open {}", dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| dir_s.clone())), sub: if window { "As the workspace of a new window".into() } else { "As the workspace".into() } });
                }
                for (_, (is_dir, name)) in &scored {
                    let icon = if *is_dir { folder_icon.clone() } else { self.file_icon(ctx, name, 24.0) };
                    rows.push(QuickRow::Item { icon, name: if *is_dir { format!("{name}/") } else { name.clone() }, sub: dir_s.trim_end_matches('/').to_string() });
                }
                let hint = if files {
                    "A file to open… (type a path)"
                } else if window {
                    "A folder to open in a new window… (type a path)"
                } else {
                    "A folder to open… (type a path)"
                };
                let text = QuickText { hint, empty: "Nothing here matches", legend: "Up/Down choose   ·   Tab fill in   ·   Enter open or go in   ·   Esc close" };
                match panel.show(ctx, "edit-quick-browse", &rows, &text, search.as_ref()) {
                    Some(QuickEvent::Pick(i)) => {
                        if open_here && i == 0 {
                            if window {
                                self.new_window(Some(dir.clone()));
                            } else {
                                self.open_folder(dir.clone());
                            }
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
