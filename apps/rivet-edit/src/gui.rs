//! The Rivet Edit window: a folder's files in a tree on the left, open files in tabs on the
//! right, with tree-sitter colors. Ctrl+P finds a file by typing part of its name, Shift+Ctrl+P
//! runs any command. Built on the Rivet apps' shared look: theme and desktop colors, icons,
//! fonts, in-app windows and the quick panel.

mod draw;
mod prefs;
mod quick;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use rivet_look::icons::{self, IconTheme};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::windows;

use crate::settings::Settings;
use crate::{syntax, workspace};

/// A finished workspace scan: its files, and whether the list was cut short.
type ScanResult = (Vec<String>, bool);

pub const ICON_SVG: &str = include_str!("../data/rivet-edit.svg");

/// An open file.
pub(crate) struct Tab {
    /// None for a new file not saved yet.
    pub path: Option<PathBuf>,
    pub text: String,
    /// The text as last read or saved (to know whether it changed).
    pub saved: String,
    /// The file's date when read or saved (to notice changes made elsewhere).
    pub disk: Option<SystemTime>,
    pub cache: syntax::Cache,
    /// Line and column of the cursor (from 1), for the status bar.
    pub line_col: (usize, usize),
}

impl Tab {
    pub fn dirty(&self) -> bool {
        self.text != self.saved
    }

    /// The name shown on its tab.
    pub fn name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Untitled".into())
    }
}

/// A question the window is asking.
pub(crate) enum Prompt {
    /// Close this tab, which has changes.
    CloseTab(usize),
    /// Quit with changes not saved.
    Quit,
    /// Save a new file: where (a path; folders made as needed).
    SaveAs { tab: usize, path: String },
    /// A new file in the workspace: its folder (inside it) and name.
    NewFile { folder: String, name: String },
}

pub struct EditApp {
    pub(crate) s: Settings,
    look: LiveLook,
    icons: IconTheme,
    app_icon: Option<egui::TextureHandle>,
    mime: Option<rivet_xdg::mime::MimeDb>,
    icon_names: HashMap<String, Vec<String>>,
    /// The workspace folder, its files (inside it) and folders.
    pub(crate) root: Option<PathBuf>,
    pub(crate) files: Vec<String>,
    pub(crate) folders: Vec<String>,
    pub(crate) files_cut: bool,
    /// Folders open in the sidebar's tree.
    pub(crate) open_dirs: HashSet<String>,
    pub(crate) tabs: Vec<Tab>,
    pub(crate) active: usize,
    pub(crate) quick: Option<(quick::QuickKind, rivet_look::quick::QuickPanel)>,
    pub(crate) prompt: Option<Prompt>,
    pub(crate) prefs_open: bool,
    prefs_win: windows::WinState,
    families: Option<Vec<String>>,
    /// The message in the status bar: what happened, or what went wrong.
    pub(crate) note: Option<(String, bool)>,
    shot: Option<rivet_kit::Shot>,
    title: String,
    checked_at: f64,
    scanned_at: f64,
    /// A workspace scan running on its own thread: its folder and the answer to come.
    scan_rx: Option<(PathBuf, std::sync::mpsc::Receiver<ScanResult>)>,
    /// Bumped whenever the file list changes (Go to File re-ranks only then).
    pub(crate) files_gen: u64,
    /// Go to File's ranking, kept between frames.
    pub(crate) file_rank: Option<quick::FileRank>,
    /// File icons by kind of file and size.
    ext_icons: HashMap<(String, u32), Option<egui::TextureHandle>>,
    /// A line to put the cursor on and show (from Go to Line or Quick Open).
    pub(crate) goto: Option<usize>,
    /// Quitting is fine (changes saved or let go).
    allow_close: bool,
}

/// Start the window on these: folders (the first is the workspace) and files.
pub struct Start {
    pub folder: Option<PathBuf>,
    pub files: Vec<PathBuf>,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl EditApp {
    pub fn new(_cc: &eframe::CreationContext, start: Start, screenshot: Option<PathBuf>) -> EditApp {
        let mut s = Settings::load();
        if screenshot.is_some() {
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
            // Checks look the same everywhere: Rivet's own icons, the default fonts.
            s.builtin_icons = true;
            s.code_font.clear();
        }
        // Test hooks for checks.
        match std::env::var("RIVET_EDIT_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Dark, false),
            _ => {}
        }
        if std::env::var_os("RIVET_EDIT_WRAP").is_some() {
            s.word_wrap = true;
        }
        let mut app = EditApp {
            s,
            look: LiveLook::new(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            app_icon: None,
            mime: None,
            icon_names: HashMap::new(),
            root: None,
            files: Vec::new(),
            folders: Vec::new(),
            files_cut: false,
            open_dirs: HashSet::new(),
            tabs: Vec::new(),
            active: 0,
            quick: None,
            prompt: None,
            prefs_open: std::env::var_os("RIVET_EDIT_PREFS").is_some(),
            prefs_win: windows::WinState::new(1, egui::Vec2::new(620.0, 560.0)),
            families: None,
            note: None,
            shot: screenshot.map(rivet_kit::Shot::new),
            title: String::new(),
            checked_at: 0.0,
            scanned_at: 0.0,
            scan_rx: None,
            files_gen: 0,
            file_rank: None,
            ext_icons: HashMap::new(),
            goto: None,
            allow_close: false,
        };
        if let Some(f) = start.folder {
            app.open_folder(f);
        }
        for f in start.files {
            app.open_file(&f);
        }
        // Test hooks: RIVET_EDIT_OPEN=a,b opens these workspace files; RIVET_EDIT_LINE=N puts
        // the cursor on that line; RIVET_EDIT_QUICK=files|commands[:query].
        if let (Ok(list), Some(root)) = (std::env::var("RIVET_EDIT_OPEN"), app.root.clone()) {
            for rel in list.split(',').filter(|r| !r.is_empty()) {
                app.open_file(&root.join(rel));
            }
        }
        if let Some(n) = std::env::var("RIVET_EDIT_LINE").ok().and_then(|v| v.parse().ok()) {
            app.goto = Some(n);
        }
        if let Ok(q) = std::env::var("RIVET_EDIT_QUICK") {
            let (k, query) = q.split_once(':').unwrap_or((q.as_str(), ""));
            let kind = match k {
                "commands" => quick::QuickKind::Commands,
                "line" => quick::QuickKind::Line,
                "folder" => quick::QuickKind::Browse { files: false },
                _ => quick::QuickKind::Files,
            };
            app.open_quick(kind);
            if let Some((_, p)) = app.quick.as_mut()
                && !query.is_empty()
            {
                p.query = query.to_string();
            }
        }
        app
    }

    fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    pub(crate) fn accent(&self) -> egui::Color32 {
        self.look.accent(&self.choice())
    }

    pub(crate) fn palette(&self) -> Option<rivet_look::palette::Palette> {
        self.look.palette().cloned()
    }

    pub(crate) fn icon(&mut self, ctx: &egui::Context, names: &[&str], px: f32) -> Option<egui::TextureHandle> {
        self.icons.texture(ctx, names, (px * ctx.pixels_per_point()).round() as u32)
    }

    pub(crate) fn app_icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.app_icon.is_none() {
            let px = (96.0 * ctx.pixels_per_point()) as u32;
            self.app_icon = icons::render_svg(ICON_SVG.as_bytes(), px).map(|img| ctx.load_texture("rivet-edit-icon", img, egui::TextureOptions::LINEAR));
        }
        self.app_icon.clone()
    }

    /// A file's icon, by its type.
    pub(crate) fn file_icon(&mut self, ctx: &egui::Context, name: &str, px: f32) -> Option<egui::TextureHandle> {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_lowercase()).unwrap_or_default();
        if !self.icon_names.contains_key(&ext) {
            let db = self.mime.get_or_insert_with(rivet_xdg::mime::MimeDb::load);
            let mime = db.guess_by_name(Path::new(name)).unwrap_or_else(|| "text/plain".into());
            let mut names = db.icon_names(&mime);
            names.push("text-x-generic".into());
            self.icon_names.insert(ext.clone(), names);
        }
        let key = (ext.clone(), (px * ctx.pixels_per_point()).round() as u32);
        if let Some(t) = self.ext_icons.get(&key) {
            return t.clone();
        }
        let names: Vec<String> = self.icon_names.get(&ext).cloned().unwrap_or_default();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let t = self.icon(ctx, &refs, px);
        self.ext_icons.insert(key, t.clone());
        t
    }

    // ---------------------------------------------------------------- workspace and files

    pub(crate) fn open_folder(&mut self, p: PathBuf) {
        let p = std::fs::canonicalize(&p).unwrap_or(p);
        if !p.is_dir() {
            self.note = Some((format!("There's no folder {}", p.display()), true));
            return;
        }
        self.root = Some(p.clone());
        self.open_dirs.clear();
        self.files.clear();
        self.folders.clear();
        self.files_gen += 1;
        self.rescan();
        self.s.remember(p);
        self.s.save();
    }

    /// List the workspace's files again, on a thread of its own (a big folder takes seconds);
    /// the answer is taken in by `take_scan`. Checks scan right away, to draw the result.
    pub(crate) fn rescan(&mut self) {
        let Some(root) = self.root.clone() else { return };
        if self.shot.is_some() {
            let (files, cut) = workspace::scan(&root);
            self.set_files(files, cut);
            return;
        }
        if self.scan_rx.as_ref().is_some_and(|(r, _)| *r == root) {
            return; // one is running already
        }
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(workspace::scan(&root));
        });
        self.scan_rx = Some((self.root.clone().unwrap_or_default(), rx));
    }

    fn set_files(&mut self, files: Vec<String>, cut: bool) {
        if files != self.files {
            self.folders = workspace::folders(&files);
            self.files = files;
            self.files_gen += 1;
        }
        self.files_cut = cut;
    }

    pub(crate) fn scan_running(&self) -> bool {
        self.scan_rx.is_some()
    }

    /// A finished scan, when it's for the workspace still open.
    fn take_scan(&mut self, ctx: &egui::Context) {
        let Some((root, rx)) = &self.scan_rx else { return };
        match rx.try_recv() {
            Ok((files, cut)) => {
                let current = self.root.as_ref() == Some(root);
                self.scan_rx = None;
                if current {
                    self.set_files(files, cut);
                    ctx.request_repaint();
                }
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            Err(std::sync::mpsc::TryRecvError::Disconnected) => self.scan_rx = None,
        }
    }

    /// Open a file in a tab (or show its tab when it's open already).
    pub(crate) fn open_file(&mut self, p: &Path) {
        let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
        if let Some(i) = self.tabs.iter().position(|t| t.path.as_deref() == Some(p.as_path())) {
            self.active = i;
            return;
        }
        if p.is_dir() {
            self.open_folder(p);
            return;
        }
        match workspace::read_text(&p) {
            Ok(text) => {
                self.tabs.push(Tab { path: Some(p.clone()), saved: text.clone(), text, disk: mtime(&p), cache: Default::default(), line_col: (1, 1) });
                self.active = self.tabs.len() - 1;
                // A file from outside the workspace is remembered for itself.
                if !self.root.as_ref().is_some_and(|r| p.starts_with(r)) {
                    self.s.remember(p);
                    self.s.save();
                }
                // Show it in the tree.
                if let (Some(root), Some(path)) = (self.root.clone(), self.tabs.last().and_then(|t| t.path.clone()))
                    && let Ok(rel) = path.strip_prefix(&root)
                {
                    let mut dir = rel.parent();
                    while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                        self.open_dirs.insert(d.to_string_lossy().replace('\\', "/"));
                        dir = d.parent();
                    }
                }
            }
            Err(e) => self.note = Some((format!("Couldn't open {}: {e}", p.display()), true)),
        }
    }

    pub(crate) fn new_tab(&mut self) {
        self.tabs.push(Tab { path: None, text: String::new(), saved: String::new(), disk: None, cache: Default::default(), line_col: (1, 1) });
        self.active = self.tabs.len() - 1;
    }

    /// Save a tab; a new file asks where first. Returns whether it was saved.
    pub(crate) fn save(&mut self, i: usize) -> bool {
        if crate::settings::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) && std::env::var_os("RIVET_EDIT_WRITE").is_none() {
            return false;
        }
        let Some(t) = self.tabs.get_mut(i) else { return false };
        let Some(p) = t.path.clone() else {
            let dir = self.root.clone().unwrap_or_else(rivet_xdg::places::home);
            self.prompt = Some(Prompt::SaveAs { tab: i, path: format!("{}/untitled.txt", dir.display()) });
            return false;
        };
        match workspace::write_text(&p, &t.text) {
            Ok(()) => {
                t.saved = t.text.clone();
                t.disk = mtime(&p);
                self.note = Some((format!("Saved {}", t.name()), false));
                true
            }
            Err(e) => {
                self.note = Some((format!("Couldn't save {}: {e}", p.display()), true));
                false
            }
        }
    }

    pub(crate) fn save_all(&mut self) {
        for i in 0..self.tabs.len() {
            if self.tabs[i].dirty() && self.tabs[i].path.is_some() {
                self.save(i);
            }
        }
    }

    /// Close a tab, asking first when it has changes (unless `force`).
    pub(crate) fn close_tab(&mut self, i: usize, force: bool) {
        if i >= self.tabs.len() {
            return;
        }
        if !force && self.tabs[i].dirty() {
            self.prompt = Some(Prompt::CloseTab(i));
            return;
        }
        self.tabs.remove(i);
        if self.active >= self.tabs.len() {
            self.active = self.tabs.len().saturating_sub(1);
        } else if self.active > i {
            self.active -= 1;
        }
    }

    pub(crate) fn rel(&self, p: &Path) -> String {
        match &self.root {
            Some(r) => p.strip_prefix(r).map(|x| x.to_string_lossy().into_owned()).unwrap_or_else(|_| p.display().to_string()),
            None => p.display().to_string(),
        }
    }

    pub(crate) fn workspace_name(&self) -> Option<String> {
        self.root.as_ref().and_then(|r| r.file_name()).map(|n| n.to_string_lossy().into_owned())
    }

    /// Files changed elsewhere: reloaded when they have no changes here.
    fn check_disk(&mut self) {
        for t in &mut self.tabs {
            let Some(p) = &t.path else { continue };
            let now = mtime(p);
            if now != t.disk {
                if t.dirty() {
                    self.note = Some((format!("{} changed on disk; saving here will replace that", t.name()), true));
                    t.disk = now;
                } else if let Ok(text) = workspace::read_text(p) {
                    t.text = text.clone();
                    t.saved = text;
                    t.disk = now;
                }
            }
        }
    }

    // ---------------------------------------------------------------- keys

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.quick.is_some() || self.prompt.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers { shift: true, ..Modifiers::COMMAND };
        let alt = Modifiers::ALT;
        let k = |m: Modifiers, key: Key| KeyboardShortcut::new(m, key);
        // Shifted ones first: a plain Ctrl+P pattern also matches with Shift held.
        let (cmds, files, save_all, save, new, open, close, goto, sidebar, wrap, prefs, menubar, next, bigger, smaller) = ctx.input_mut(|i| {
            (
                i.consume_shortcut(&k(shift, Key::P)),
                i.consume_shortcut(&k(cmd, Key::P)),
                i.consume_shortcut(&k(shift, Key::S)),
                i.consume_shortcut(&k(cmd, Key::S)),
                i.consume_shortcut(&k(cmd, Key::N)),
                i.consume_shortcut(&k(cmd, Key::O)),
                i.consume_shortcut(&k(cmd, Key::W)),
                i.consume_shortcut(&k(cmd, Key::G)),
                i.consume_shortcut(&k(Modifiers::NONE, Key::F9)),
                i.consume_shortcut(&k(alt, Key::Z)),
                i.consume_shortcut(&k(cmd, Key::Comma)),
                i.consume_shortcut(&k(cmd, Key::M)),
                i.consume_shortcut(&k(cmd, Key::Tab)) || i.consume_shortcut(&k(cmd, Key::PageDown)),
                i.consume_shortcut(&k(cmd, Key::Equals)) || i.consume_shortcut(&k(cmd, Key::Plus)),
                i.consume_shortcut(&k(cmd, Key::Minus)),
            )
        });
        if ctx.input_mut(|i| i.consume_shortcut(&k(cmd, Key::Q))) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if cmds {
            self.open_quick(quick::QuickKind::Commands);
        } else if files {
            self.open_quick(quick::QuickKind::Files);
        }
        if save_all {
            self.save_all();
        } else if save {
            self.save(self.active);
        }
        if new {
            self.new_tab();
        }
        if open {
            self.open_quick(quick::QuickKind::Browse { files: false });
        }
        if close {
            self.close_tab(self.active, false);
        }
        if goto && !self.tabs.is_empty() {
            self.open_quick(quick::QuickKind::Line);
        }
        if sidebar {
            self.s.show_sidebar = !self.s.show_sidebar;
            self.s.save();
        }
        if wrap {
            self.s.word_wrap = !self.s.word_wrap;
            self.s.save();
        }
        if prefs {
            self.prefs_open = !self.prefs_open;
        }
        if menubar {
            self.s.show_menubar = !self.s.show_menubar;
            self.s.save();
        }
        if next && !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
        if bigger || smaller {
            self.s.code_size = (self.s.code_size + if bigger { 1.0 } else { -1.0 }).clamp(8.0, 32.0);
            self.s.save();
        }
    }
}

impl eframe::App for EditApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            rivet_kit::scripted_click("RIVET_EDIT", shot.frames(), raw);
            // Checks: RIVET_EDIT_KEY=tab (a key name) presses that key once things settle.
            if shot.frames() == 40
                && let Some(key) = std::env::var("RIVET_EDIT_KEY").ok().and_then(|k| Key::from_name(&k))
            {
                raw.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        // The code font is the monospace family; the interface follows the desktop.
        self.look.set_fonts(&ctx, "", "", &self.s.code_font);
        self.look.frame(&ctx, &c, self.shot.is_none());
        let kit = self.s.builtin_icons.then(|| {
            let v = ctx.global_style().visuals.clone();
            rivet_look::builtin_icons::BuiltinColors { accent: self.look.accent(&c), bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }
        });
        self.icons.set_builtin(kit);
        // Quit was chosen in a question: let this close through.
        if draw::QUIT.swap(false, std::sync::atomic::Ordering::Relaxed) {
            self.allow_close = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Quitting with changes: ask first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.tabs.iter().any(Tab::dirty) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.prompt = Some(Prompt::Quit);
        }
        self.shortcuts(&ctx);
        let now = ctx.input(|i| i.time);
        self.take_scan(&ctx);
        if self.shot.is_none() && now - self.checked_at > 2.0 {
            self.checked_at = now;
            self.check_disk();
            ctx.request_repaint_after(std::time::Duration::from_secs(2));
        }
        // The file list again every few seconds, in the background (not for huge folders).
        if self.shot.is_none() && self.root.is_some() && !self.files_cut && now - self.scanned_at > 5.0 {
            self.scanned_at = now;
            self.rescan();
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        let title = match (self.tabs.get(self.active), self.workspace_name()) {
            (Some(t), Some(w)) => format!("{}{} – {w} – Rivet Edit", if t.dirty() { "● " } else { "" }, t.name()),
            (Some(t), None) => format!("{}{} – Rivet Edit", if t.dirty() { "● " } else { "" }, t.name()),
            (None, Some(w)) => format!("{w} – Rivet Edit"),
            (None, None) => "Rivet Edit".into(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        self.draw(ui, &ctx);
        if self.prefs_open {
            self.prefs_window(&ctx);
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}
