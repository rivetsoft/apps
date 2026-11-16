//! The Rivet Notes window: the vault's notes by folder or by tag on the left, the open note
//! (its properties from the frontmatter, then its text) on the right. Built on the Rivet
//! apps' shared look: theme and desktop colors, icons, fonts, in-app windows.

mod draw;
mod live_edit;
mod peek;
mod props;
mod quick;
mod prefs;
mod read;

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers, Vec2};
use rivet_look::icons::{self, IconTheme};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::windows;

use crate::settings::{Browse, Settings};
use crate::vault::{self, Frontmatter, Note, Value};

pub const ICON_SVG: &str = include_str!("../data/rivet-notes.svg");

/// The open note, as it's being edited.
struct Doc {
    path: PathBuf,
    fm: Option<Frontmatter>,
    body: String,
    dirty: bool,
    /// When it was last changed (seconds, the window's clock), for saving after a pause.
    edited_at: f64,
    /// The file's date when read or saved (to notice changes made elsewhere).
    disk: Option<std::time::SystemTime>,
    saved_at: Option<String>,
    /// A property being added: its name.
    new_prop: Option<String>,
    /// The new property's type.
    new_type: crate::props::PropType,
    /// What's being typed into each list or tags property's "add" box.
    item_input: std::collections::HashMap<String, String>,
    /// Each property's type (worked out once, or chosen from its menu).
    types: Vec<(String, crate::props::PropType)>,
    /// A property being renamed: its row and the new name.
    renaming: Option<(usize, String)>,
    /// The properties block is open (not folded).
    props_open: bool,
    /// The title while it's being edited in the heading.
    title_buf: Option<String>,
}

/// An editing command from the text's right-click menu, carried out on the next frame (once
/// the text has the keyboard again).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum EditCmd {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    Delete,
    SelectAll,
}

enum Prompt {
    NewNote { folder: String, title: String },
    NewFolder { parent: String, name: String },
    Rename { path: PathBuf, name: String },
    Delete(PathBuf),
    /// Open a folder as a vault: the path being typed.
    OpenVault(String),
    /// The open note changed on disk while it had unsaved changes.
    Conflict,
}

pub struct NotesApp {
    s: Settings,
    look: LiveLook,
    icons: IconTheme,
    app_icon: Option<egui::TextureHandle>,
    vault: Option<PathBuf>,
    notes: Vec<Note>,
    folders: Vec<String>,
    scanned_at: f64,
    doc: Option<Doc>,
    search: String,
    /// The folder new notes go in (vault-relative).
    current_folder: String,
    prompt: Option<Prompt>,
    note_msg: Option<(String, bool)>,
    prefs_open: bool,
    prefs_win: windows::WinState,
    prefs_page: prefs::Page,
    about_open: bool,
    shot: Option<rivet_kit::Shot>,
    edit_cmd: Option<EditCmd>,
    /// The window title shown (it names the vault).
    title: String,
    /// Installed font families, for Preferences (listed once, when first needed).
    families: Option<(Vec<String>, Vec<String>)>,
    /// The reading view, when open.
    reading: Option<read::Reading>,
    /// A format chosen from a menu, applied to the editor's selection next frame.
    pending_fmt: Option<crate::format::Fmt>,
    /// Where the selection's format bar was last drawn (kept up while the pointer is on it).
    fmt_bar: Option<egui::Rect>,
    /// The note's page inside the window (for a centered column's margins).
    page_rect: egui::Rect,
    /// Quick Open or Quick Actions, while one is up.
    quick: Option<(quick::QuickKind, rivet_look::quick::QuickPanel)>,
}

const SAVE_AFTER: f64 = 1.0;

impl NotesApp {
    pub fn new(_cc: &eframe::CreationContext, open: Option<PathBuf>, screenshot: Option<PathBuf>) -> NotesApp {
        let checking = screenshot.is_some();
        if checking {
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        let mut s = if checking { Settings::default() } else { Settings::load() };
        // Test hooks for checks.
        if let Ok(v) = std::env::var("RIVET_NOTES_BROWSE") {
            s.layout = match v.as_str() {
                "tags" => crate::layout::Layout::tabs(Browse::Tags),
                "below" => crate::layout::Layout::stacked(),
                _ => crate::layout::Layout::tabs(Browse::Folders),
            };
        }
        // Test hook: RIVET_NOTES_FONTS=ui|editor|mono (family names, empty for the default).
        if let Ok(f) = std::env::var("RIVET_NOTES_FONTS") {
            let mut it = f.split('|').map(str::to_string);
            s.ui_font = it.next().unwrap_or_default();
            s.editor_font = it.next().unwrap_or_default();
            s.mono_font = it.next().unwrap_or_default();
        }
        // Test hook: RIVET_NOTES_SYSTEM_ICONS uses the desktop's icon theme.
        if std::env::var_os("RIVET_NOTES_SYSTEM_ICONS").is_some() {
            s.builtin_icons = false;
        }
        // Test hooks: RIVET_NOTES_PEEK opens Peek; RIVET_NOTES_WIDTH=centered|full.
        // Test hook: RIVET_NOTES_NO_CRUMBS hides the breadcrumb bar.
        if std::env::var_os("RIVET_NOTES_NO_CRUMBS").is_some() {
            s.show_breadcrumbs = false;
        }
        // Test hook: RIVET_NOTES_DAILY turns the daily notes module on.
        if std::env::var_os("RIVET_NOTES_DAILY").is_some() {
            s.daily_notes = true;
        }
        // Test hook: RIVET_NOTES_QUICK=open|actions[:query] opens a quick panel (applied below).
        // Test hook: RIVET_NOTES_NO_STATUS hides the status bar.
        if std::env::var_os("RIVET_NOTES_NO_STATUS").is_some() {
            s.show_statusbar = false;
        }
        // Test hook: RIVET_NOTES_LIVE turns live mode on.
        if std::env::var_os("RIVET_NOTES_LIVE").is_some() {
            s.live_mode = true;
        }
        if std::env::var_os("RIVET_NOTES_PEEK").is_some() {
            s.show_peek = true;
        }
        match std::env::var("RIVET_NOTES_WIDTH").as_deref() {
            Ok("centered") => s.centered = true,
            Ok("full") => s.centered = false,
            _ => {}
        }
        match std::env::var("RIVET_NOTES_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (rivet_look::theme::Mode::Dark, false),
            _ => {}
        }
        let mut app = NotesApp {
            s,
            look: LiveLook::new(),
            icons: IconTheme::new(&icons::detect_system_theme()),
            app_icon: None,
            vault: None,
            notes: Vec::new(),
            folders: Vec::new(),
            scanned_at: 0.0,
            doc: None,
            search: String::new(),
            current_folder: String::new(),
            prompt: None,
            note_msg: None,
            prefs_open: std::env::var_os("RIVET_NOTES_PREFS").is_some(),
            prefs_win: windows::WinState::new(20, Vec2::new(800.0, 620.0)),
            // Test hook: RIVET_NOTES_PREFS=editor opens that page.
            prefs_page: prefs::Page::from_name(&std::env::var("RIVET_NOTES_PREFS").unwrap_or_default()),
            about_open: false,
            shot: screenshot.map(rivet_kit::Shot::new),
            edit_cmd: None,
            title: String::new(),
            families: None,
            reading: None,
            pending_fmt: None,
            fmt_bar: None,
            page_rect: egui::Rect::NOTHING,
            quick: None,
        };
        let start = open.or_else(|| app.s.vaults.iter().find(|p| p.is_dir()).cloned());
        if let Some(v) = start {
            app.open_vault(v);
        }
        if let Ok(rel) = std::env::var("RIVET_NOTES_OPEN")
            && let Some(v) = &app.vault
        {
            let p = v.join(rel);
            app.open_note(&p);
        }
        // Test hook for checks: RIVET_NOTES_SELECT=A,B selects those characters in the note's text.
        if let Some((a, b)) = std::env::var("RIVET_NOTES_SELECT").ok().and_then(|v| {
            let (a, b) = v.split_once(',')?;
            Some((a.trim().parse::<usize>().ok()?, b.trim().parse::<usize>().ok()?))
        }) {
            let id = egui::Id::unique("note-body");
            let mut st = egui::TextEdit::load_state(&_cc.egui_ctx, id).unwrap_or_default();
            st.cursor.set_char_range(Some(egui::text::CCursorRange::two(egui::text::CCursor::new(a), egui::text::CCursor::new(b))));
            st.store(&_cc.egui_ctx, id);
            _cc.egui_ctx.memory_mut(|m| m.request_focus(id));
        }
        // Test hook for checks: RIVET_NOTES_NEW opens the New Note (or =folder, New Folder) question.
        match std::env::var("RIVET_NOTES_NEW").as_deref() {
            Ok("folder") => app.prompt = Some(Prompt::NewFolder { parent: app.current_folder.clone(), name: String::new() }),
            Ok(_) => app.prompt = Some(Prompt::NewNote { folder: app.current_folder.clone(), title: String::new() }),
            Err(_) => {}
        }
        // Test hook for checks: RIVET_NOTES_READ opens the reading view.
        if std::env::var_os("RIVET_NOTES_READ").is_some() {
            app.toggle_reading(&_cc.egui_ctx);
        }
        // Test hook for checks: RIVET_NOTES_EDIT=selectall|delete runs a text-menu command.
        app.edit_cmd = match std::env::var("RIVET_NOTES_EDIT").as_deref() {
            Ok("selectall") => Some(EditCmd::SelectAll),
            Ok("delete") => Some(EditCmd::Delete),
            _ => None,
        };
        if app.edit_cmd.is_some() {
            _cc.egui_ctx.memory_mut(|m| m.request_focus(egui::Id::unique("note-body")));
        }
        // Test hook for checks: RIVET_NOTES_FMT=bold,6,6 formats the open note's text there.
        if let Ok(spec) = std::env::var("RIVET_NOTES_FMT")
            && let Some(d) = app.doc.as_mut()
        {
            let mut it = spec.split(',');
            let f = match it.next().unwrap_or("") {
                "bold" => Some(crate::format::Fmt::Bold),
                "italic" => Some(crate::format::Fmt::Italic),
                "link" => Some(crate::format::Fmt::Link),
                "h2" => Some(crate::format::Fmt::Heading(2)),
                "task" => Some(crate::format::Fmt::Task),
                _ => None,
            };
            let a: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let b: usize = it.next().and_then(|x| x.parse().ok()).unwrap_or(a);
            if let Some(f) = f {
                let (t, _) = crate::format::apply(&d.body, (a, b), f);
                d.body = t;
                d.dirty = true;
            }
        }
        // Test hook for checks: RIVET_NOTES_APPEND adds text to the open note and, with
        // RIVET_NOTES_WRITE, saves it (only on a throwaway vault).
        if let Ok(extra) = std::env::var("RIVET_NOTES_APPEND")
            && let Some(d) = app.doc.as_mut()
        {
            d.body.push_str(&extra.replace("\\n", "\n"));
            d.dirty = true;
            if std::env::var_os("RIVET_NOTES_WRITE").is_some() {
                crate::settings::NO_SAVE.store(false, std::sync::atomic::Ordering::Relaxed);
                app.save_now();
                crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
            }
        }
        if let Ok(q) = std::env::var("RIVET_NOTES_QUICK") {
            let (k, query) = q.split_once(':').unwrap_or((q.as_str(), ""));
            app.toggle_quick(if k == "actions" { quick::QuickKind::Actions } else { quick::QuickKind::Open });
            if let Some((_, p)) = app.quick.as_mut() {
                p.query = query.to_string();
            }
        }
        // Test hook for checks: RIVET_NOTES_RETITLE with RIVET_NOTES_WRITE renames the open note
        // as if its title had been typed in the heading (only on a throwaway vault).
        if let Ok(t) = std::env::var("RIVET_NOTES_RETITLE")
            && std::env::var_os("RIVET_NOTES_WRITE").is_some()
            && app.doc.is_some()
        {
            crate::settings::NO_SAVE.store(false, std::sync::atomic::Ordering::Relaxed);
            app.retitle(&t);
            crate::settings::NO_SAVE.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        app
    }

    fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    fn icon(&mut self, ctx: &egui::Context, names: &[&str], px: f32) -> Option<egui::TextureHandle> {
        self.icons.texture(ctx, names, (px * ctx.pixels_per_point()).round() as u32)
    }

    fn app_icon(&mut self, ctx: &egui::Context) -> Option<egui::TextureHandle> {
        if self.app_icon.is_none() {
            let px = (96.0 * ctx.pixels_per_point()) as u32;
            self.app_icon = icons::render_svg(ICON_SVG.as_bytes(), px).map(|img| ctx.load_texture("rivet-notes-icon", img, egui::TextureOptions::LINEAR));
        }
        self.app_icon.clone()
    }

    // ---------------------------------------------------------------- vaults

    fn open_vault(&mut self, p: PathBuf) {
        let p = std::fs::canonicalize(&p).unwrap_or(p);
        if !p.is_dir() {
            self.note_msg = Some((format!("There's no folder {}", p.display()), true));
            return;
        }
        self.save_now();
        self.doc = None;
        self.vault = Some(p.clone());
        self.current_folder.clear();
        self.search.clear();
        self.s.use_vault(p);
        self.s.save();
        self.rescan();
    }

    fn rescan(&mut self) {
        let Some(v) = &self.vault else { return };
        self.notes = vault::scan(v);
        self.folders = vault::folders(v);
    }

    fn vault_name(&self) -> String {
        self.vault.as_ref().and_then(|v| v.file_name()).map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "No vault".into())
    }

    // ---------------------------------------------------------------- notes

    fn open_note(&mut self, p: &Path) {
        if self.doc.as_ref().is_some_and(|d| d.path == p) {
            return;
        }
        self.save_now();
        let Ok(text) = std::fs::read_to_string(p) else {
            self.note_msg = Some((format!("Couldn't open {}", p.display()), true));
            return;
        };
        let (fm, body) = vault::split(&text);
        self.doc = Some(Doc { path: p.to_path_buf(), fm, body: body.to_string(), dirty: false, edited_at: 0.0, disk: mtime(p), saved_at: None, new_prop: None,
            new_type: crate::props::PropType::Text,
            item_input: Default::default(),
            types: Vec::new(),
            renaming: None,
            props_open: true,
            title_buf: None,
        });
        if let (Some(v), Some(dir)) = (&self.vault, p.parent()) {
            self.current_folder = dir.strip_prefix(v).map(|r| r.to_string_lossy().replace('\\', "/")).unwrap_or_default();
        }
    }

    /// Write the open note if it has changes.
    fn save_now(&mut self) {
        let Some(d) = self.doc.as_mut() else { return };
        if !d.dirty {
            return;
        }
        if crate::settings::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed) {
            d.dirty = false;
            return;
        }
        let text = vault::join(d.fm.as_ref(), &d.body);
        let tmp = d.path.with_extension("md.rivet-tmp");
        let r = std::fs::write(&tmp, &text).and_then(|_| std::fs::rename(&tmp, &d.path));
        match r {
            Ok(()) => {
                d.dirty = false;
                d.disk = mtime(&d.path);
                d.saved_at = Some(chrono::Local::now().format("%H:%M").to_string());
                // Its title and tags may have changed.
                if let (Some(v), Some(n)) = (&self.vault, self.notes.iter_mut().find(|n| n.path == d.path))
                    && let Some(fresh) = vault::read_note(v, &d.path)
                {
                    *n = fresh;
                }
            }
            Err(e) => self.note_msg = Some((format!("Couldn't save: {e}"), true)),
        }
    }

    /// A new title typed in the note's heading. The `title` property changes when the note has
    /// one; the file is renamed to match when its name was the title (or there's no property).
    pub(super) fn retitle(&mut self, new: &str) {
        let new = new.trim();
        let Some(d) = self.doc.as_mut() else { return };
        let stem = d.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let (old, from) = vault::note_title(d.fm.as_ref(), &d.body, &d.path);
        // A file name can't hold a slash; the rest is kept as typed.
        let file = new.replace(['/', '\\'], "-");
        if new.is_empty() || (new == old && file == stem) {
            return;
        }
        // The title changes where it comes from…
        match from {
            vault::TitleFrom::Property => {
                if let Some(fm) = d.fm.as_mut() {
                    fm.set("title", Value::Text(new.to_string()));
                }
                d.dirty = true;
            }
            vault::TitleFrom::Heading => {
                d.body = vault::set_first_heading(&d.body, new);
                d.dirty = true;
            }
            vault::TitleFrom::File => {}
        }
        // …and the file is renamed to match (saving first), so the sidebar shows it.
        let path = d.path.clone();
        if file != stem {
            self.rename(&path, &file);
        }
        self.save_now();
    }

    /// Today's daily note: opened when it's there, else made in the daily notes folder.
    pub(super) fn daily_note(&mut self) {
        let Some(v) = self.vault.clone() else { return };
        let folder = self.s.daily_folder.trim_matches('/').to_string();
        let dir = v.join(&folder);
        let today = chrono::Local::now();
        let path = dir.join(format!("{}.md", today.format("%Y-%m-%d")));
        if path.exists() {
            self.open_note(&path);
            self.reveal_folder(&folder);
            return;
        }
        if let Err(e) = std::fs::create_dir_all(&dir) {
            self.note_msg = Some((format!("Couldn't make the daily notes folder: {e}"), true));
            return;
        }
        let mut fm = Frontmatter::default();
        fm.set("created", Value::Text(today.format("%Y-%m-%d %H:%M").to_string()));
        fm.set("tags", Value::List(vec!["daily".into()]));
        match std::fs::write(&path, vault::join(Some(&fm), "\n")) {
            Ok(()) => {
                self.rescan();
                self.open_note(&path);
                self.reveal_folder(&folder);
                self.note_msg = Some((format!("Made today's note, {}.", today.format("%A %-d %B")), false));
            }
            Err(e) => self.note_msg = Some((format!("Couldn't make today's note: {e}"), true)),
        }
    }

    /// Open a folder and its parents in the sidebar, and make it where new notes go.
    fn reveal_folder(&mut self, folder: &str) {
        self.s.collapsed.retain(|k| !(folder == k || folder.starts_with(&format!("{k}/"))));
        self.current_folder = folder.to_string();
    }

    fn new_note(&mut self, folder: &str, title: &str) {
        let Some(v) = self.vault.clone() else { return };
        let dir = v.join(folder);
        let _ = std::fs::create_dir_all(&dir);
        let path = vault::note_file(&dir, title);
        let mut fm = Frontmatter::default();
        fm.set("title", Value::Text(title.trim().to_string()));
        fm.set("created", Value::Text(chrono::Local::now().format("%Y-%m-%d %H:%M").to_string()));
        fm.set("tags", Value::List(Vec::new()));
        let text = vault::join(Some(&fm), "\n");
        match std::fs::write(&path, text) {
            Ok(()) => {
                self.rescan();
                self.open_note(&path);
            }
            Err(e) => self.note_msg = Some((format!("Couldn't make the note: {e}"), true)),
        }
    }

    fn rename(&mut self, path: &Path, name: &str) {
        let name = name.trim();
        if name.is_empty() || name.contains('/') {
            return;
        }
        let is_dir = path.is_dir();
        let new_name = if is_dir || name.ends_with(".md") || name.ends_with(".markdown") { name.to_string() } else { format!("{name}.md") };
        let to = path.with_file_name(new_name);
        if to.exists() {
            self.note_msg = Some((format!("\"{}\" already exists there.", name), true));
            return;
        }
        self.save_now();
        match std::fs::rename(path, &to) {
            Ok(()) => {
                if self.s.note_widths.iter().any(|(p, _)| p.starts_with(path)) {
                    self.s.moved(path, &to);
                    self.s.save();
                }
                if let Some(d) = self.doc.as_mut() {
                    if d.path == path {
                        d.path = to.clone();
                    } else if let Ok(rest) = d.path.strip_prefix(path) {
                        d.path = to.join(rest);
                    }
                }
                self.rescan();
            }
            Err(e) => self.note_msg = Some((format!("Couldn't rename: {e}"), true)),
        }
    }

    /// Move a note or folder into a folder of the vault ("" for the top), keeping the open note
    /// open where it went.
    pub(super) fn move_into(&mut self, src: &Path, folder: &str) {
        let Some(v) = self.vault.clone() else { return };
        let dest = v.join(folder);
        let Some(name) = src.file_name() else { return };
        let to = dest.join(name);
        if src.parent() == Some(dest.as_path()) || dest.starts_with(src) {
            return;
        }
        if to.exists() {
            self.note_msg = Some((format!("There's already a \"{}\" in {}.", name.to_string_lossy(), if folder.is_empty() { "the top of the vault" } else { folder }), true));
            return;
        }
        self.save_now();
        match std::fs::rename(src, &to) {
            Ok(()) => {
                if self.s.note_widths.iter().any(|(p, _)| p.starts_with(src)) {
                    self.s.moved(src, &to);
                }
                if let Some(d) = self.doc.as_mut() {
                    if d.path == src {
                        d.path = to.clone();
                    } else if let Ok(rest) = d.path.strip_prefix(src) {
                        d.path = to.join(rest);
                    }
                    d.disk = mtime(&d.path);
                }
                self.current_folder = folder.to_string();
                // Show where it went.
                self.s.collapsed.retain(|k| k != folder);
                let shown = if folder.is_empty() { "the top of the vault".to_string() } else { format!("\"{folder}\"") };
                self.note_msg = Some((format!("Moved \"{}\" to {shown}.", name.to_string_lossy()), false));
                self.rescan();
            }
            Err(e) => self.note_msg = Some((format!("Couldn't move it: {e}"), true)),
        }
    }

    /// A copy of a note beside it ("Plans copy.md"), opened.
    fn duplicate(&mut self, path: &Path) {
        self.save_now();
        let Some(dir) = path.parent() else { return };
        let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let to = vault::note_file(dir, &format!("{stem} copy"));
        match std::fs::copy(path, &to) {
            Ok(_) => {
                self.rescan();
                self.open_note(&to);
            }
            Err(e) => self.note_msg = Some((format!("Couldn't duplicate: {e}"), true)),
        }
    }

    /// Move to the desktop's Trash (through gio, like the desktop does).
    fn trash(&mut self, path: &Path) {
        if self.doc.as_ref().is_some_and(|d| d.path == path || d.path.starts_with(path)) {
            self.doc = None;
        }
        let ok = std::process::Command::new("gio").arg("trash").arg(path).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
        if ok {
            self.note_msg = Some((format!("Moved \"{}\" to the Trash.", path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()), false));
        } else {
            self.note_msg = Some(("Couldn't move it to the Trash (gio is needed).".into(), true));
        }
        self.rescan();
    }

    /// Tags in use, with how many notes have each, most used first.
    fn tag_counts(&self) -> Vec<(String, usize)> {
        let mut m: BTreeMap<String, (String, usize)> = BTreeMap::new();
        for n in &self.notes {
            for t in &n.tags {
                let e = m.entry(t.to_lowercase()).or_insert((t.clone(), 0));
                e.1 += 1;
            }
        }
        let mut v: Vec<(String, usize)> = m.into_values().collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
        v
    }

    fn matches(&self, n: &Note) -> bool {
        let q = self.search.trim().to_lowercase();
        if q.is_empty() {
            return true;
        }
        if let Some(t) = q.strip_prefix('#') {
            return n.tags.iter().any(|x| x.to_lowercase().starts_with(t));
        }
        n.title.to_lowercase().contains(&q) || n.rel.to_lowercase().contains(&q) || n.snippet.to_lowercase().contains(&q) || n.tags.iter().any(|x| x.to_lowercase().contains(&q))
    }

    // ---------------------------------------------------------------- keys and menus

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.prompt.is_some() {
            return;
        }
        // Ctrl+Space: the rendered page; while it's up, it has the keyboard.
        if ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, Key::Space))) {
            self.toggle_reading(ctx);
        }
        if self.reading.is_some() {
            return;
        }
        let cmd = Modifiers::COMMAND;
        let shift = Modifiers { shift: true, ..Modifiers::COMMAND };
        let daily_on = self.s.daily_notes;
        let (f, n, s, comma, m, b, t, qa, qo, find, pk, day) = ctx.input_mut(|i| {
            (
                // Shift first: a plain Ctrl+N pattern also matches with Shift held.
                i.consume_shortcut(&KeyboardShortcut::new(shift, Key::N)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::N)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::S)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::Comma)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::M)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F9)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::T)),
                i.consume_shortcut(&KeyboardShortcut::new(shift, Key::P)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::P)),
                i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::F)),
                i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F11)),
                daily_on && i.consume_shortcut(&KeyboardShortcut::new(cmd, Key::D)),
            )
        });
        if day && self.vault.is_some() {
            self.daily_note();
        }
        if n && self.vault.is_some() {
            self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() });
        }
        if f && self.vault.is_some() {
            self.prompt = Some(Prompt::NewFolder { parent: self.current_folder.clone(), name: String::new() });
        }
        if s {
            self.save_now();
        }
        if comma {
            self.prefs_open = !self.prefs_open;
        }
        if m {
            self.s.show_menubar = !self.s.show_menubar;
            self.s.save();
        }
        if b {
            self.s.show_sidebar = !self.s.show_sidebar;
            self.s.save();
        }
        if pk && self.vault.is_some() {
            self.s.show_peek = !self.s.show_peek;
            self.s.save();
        }
        if t {
            // Switch the tabs; with no tabs, fold or open the last pane.
            let l = &mut self.s.layout;
            if let Some(i) = l.tabbed() {
                let s = &mut l.slots[i];
                s.active = (s.active + 1) % s.panels.len();
            } else if let Some(last) = l.slots.last() {
                let k = crate::layout::fold_key(last);
                if let Some(i) = self.s.collapsed.iter().position(|x| *x == k) {
                    self.s.collapsed.remove(i);
                } else {
                    self.s.collapsed.push(k);
                }
            }
            self.s.save();
        }
        // Quick Actions first: a plain Ctrl+P pattern also matches with Shift held.
        if qa {
            self.toggle_quick(quick::QuickKind::Actions);
        } else if qo && self.vault.is_some() {
            self.toggle_quick(quick::QuickKind::Open);
        }
        if find {
            ctx.memory_mut(|m| m.request_focus(egui::Id::unique("notes-search")));
        }
        // F2 renames the open note; Delete (when not typing) moves it to the Trash.
        let typing = ctx.memory(|m| m.focused().is_some());
        let (f2, del) = ctx.input_mut(|i| {
            (i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::F2)), !typing && i.consume_shortcut(&KeyboardShortcut::new(Modifiers::NONE, Key::Delete)))
        });
        if let Some(p) = self.doc.as_ref().map(|d| d.path.clone()) {
            if f2 {
                let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                self.prompt = Some(Prompt::Rename { path: p.clone(), name });
            }
            if del {
                self.prompt = Some(Prompt::Delete(p));
            }
        }
    }

    fn menus(&mut self, ui: &mut egui::Ui) {
        let has_vault = self.vault.is_some();
        let style = |ui: &mut egui::Ui| {
            ui.set_min_width(220.0);
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            ui.spacing_mut().item_spacing.y = 3.0;
        };
        ui.menu_button("File", |ui| {
            style(ui);
            if ui.add_enabled(has_vault, egui::Button::new("New Note…").shortcut_text("Ctrl+N")).clicked() {
                self.prompt = Some(Prompt::NewNote { folder: self.current_folder.clone(), title: String::new() });
                ui.close();
            }
            if self.s.daily_notes && ui.add_enabled(has_vault, egui::Button::new("Daily Note").shortcut_text("Ctrl+D")).clicked() {
                self.daily_note();
                ui.close();
            }
            if ui.add_enabled(has_vault, egui::Button::new("Create New Folder…").shortcut_text("Shift+Ctrl+N")).clicked() {
                self.prompt = Some(Prompt::NewFolder { parent: self.current_folder.clone(), name: String::new() });
                ui.close();
            }
            ui.separator();
            ui.menu_button("Switch Vault", |ui| {
                style(ui);
                self.vault_list(ui);
            });
            if ui.button("Open Folder as Vault…").clicked() {
                self.prompt = Some(Prompt::OpenVault(String::new()));
                ui.close();
            }
            if ui.add_enabled(has_vault, egui::Button::new("Show Vault Folder")).clicked() {
                if let Some(v) = &self.vault {
                    let _ = rivet_xdg::launch::open_url(&v.to_string_lossy());
                }
                ui.close();
            }
            ui.separator();
            if ui.add_enabled(self.doc.as_ref().is_some_and(|d| d.dirty), egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                self.save_now();
                ui.close();
            }
            if ui.add(egui::Button::new("Close Window").shortcut_text("Ctrl+Q")).clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                ui.close();
            }
        });
        ui.menu_button("Edit", |ui| {
            style(ui);
            let open = self.doc.as_ref().map(|d| d.path.clone());
            if ui.add_enabled(open.is_some(), egui::Button::new("Rename Note…").shortcut_text("F2")).clicked() {
                if let Some(p) = open.clone() {
                    let name = p.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
                    self.prompt = Some(Prompt::Rename { path: p, name });
                }
                ui.close();
            }
            if ui.add_enabled(open.is_some(), egui::Button::new("Move Note to Trash…")).clicked() {
                if let Some(p) = open {
                    self.prompt = Some(Prompt::Delete(p));
                }
                ui.close();
            }
            ui.separator();
            if ui.add(egui::Button::new("Find").shortcut_text("Ctrl+F")).clicked() {
                ui.ctx().memory_mut(|m| m.request_focus(egui::Id::unique("notes-search")));
                ui.close();
            }
            ui.separator();
            if ui.add(egui::Button::new("Preferences").shortcut_text("Ctrl+,")).clicked() {
                self.prefs_open = true;
                ui.close();
            }
        });
        ui.menu_button("Format", |ui| {
            style(ui);
            self.format_items(ui);
        });
        ui.menu_button("View", |ui| {
            style(ui);
            use crate::layout::Layout;
            for (label, preset) in [("By Folder", Layout::tabs(Browse::Folders)), ("By Tag", Layout::tabs(Browse::Tags)), ("Tags Below Folders", Layout::stacked())] {
                let on = self.s.layout.slots == preset.slots;
                if ui.add(egui::RadioButton::new(on, label)).clicked() {
                    let split = self.s.layout.split;
                    self.s.layout = Layout { split, ..preset };
                    self.s.save();
                    ui.close();
                }
            }
            ui.label(egui::RichText::new("Or drag a panel's title in the sidebar").small().weak());
            ui.separator();
            let c = |ui: &mut egui::Ui, v: &mut bool, label: &str, sc: &str| {
                let mut b = *v;
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut b, label).changed() {
                        *v = b;
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.label(egui::RichText::new(sc).weak()));
                });
            };
            let before = self.s.clone();
            c(ui, &mut self.s.show_properties, "Properties", "");
            c(ui, &mut self.s.show_sidebar, "Sidebar", "F9");
            c(ui, &mut self.s.show_peek, "Peek", "F11");
            c(ui, &mut self.s.live_mode, "Live Mode", "");
            c(ui, &mut self.s.show_breadcrumbs, "Breadcrumbs", "");
            c(ui, &mut self.s.centered, "Center Notes by Default", "");
            c(ui, &mut self.s.show_menubar, "Menubar", "Ctrl+M");
            c(ui, &mut self.s.show_statusbar, "Status Bar", "");
            if self.s != before {
                self.s.save();
            }
            ui.separator();
            if ui.add_enabled(self.doc.is_some(), egui::Button::new("Reading View").shortcut_text("Ctrl+Space")).clicked() {
                let ctx = ui.ctx().clone();
                self.toggle_reading(&ctx);
                ui.close();
            }
            if ui.add(egui::Button::new("Reload Vault").shortcut_text("F5")).clicked() {
                self.rescan();
                ui.close();
            }
        });
        ui.menu_button("Help", |ui| {
            style(ui);
            if ui.button("About Rivet Notes").clicked() {
                self.about_open = true;
                self.prefs_open = true;
                ui.close();
            }
        });
    }

    /// The Format menu's items (also in the text's right-click menu).
    pub(super) fn format_items(&mut self, ui: &mut egui::Ui) {
        use crate::format::Fmt;
        let on = self.doc.is_some();
        for (k, (f, label, sc)) in Fmt::ALL.iter().enumerate() {
            // Groups: emphasis, link, headings, lists, code block.
            if [4, 5, 9, 14].contains(&k) {
                ui.separator();
            }
            if ui.add_enabled(on, egui::Button::new(*label).shortcut_text(*sc)).clicked() {
                self.pending_fmt = Some(*f);
                ui.close();
            }
        }
    }

    /// The vaults as menu items (switch to one), and Open Folder as Vault.
    fn vault_list(&mut self, ui: &mut egui::Ui) {
        let current = self.vault.clone();
        let vaults = self.s.vaults.clone();
        if vaults.is_empty() {
            ui.label(egui::RichText::new("No vaults yet").weak());
        }
        for v in vaults {
            let name = v.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| v.display().to_string());
            let here = current.as_ref() == Some(&v);
            let missing = !v.is_dir();
            let r = ui.add_enabled(!missing, egui::Button::selectable(here, name)).on_hover_text(v.display().to_string());
            if r.clicked() && !here {
                self.open_vault(v);
                ui.close();
            }
        }
        ui.separator();
        if ui.button("Open Folder as Vault…").clicked() {
            self.prompt = Some(Prompt::OpenVault(String::new()));
            ui.close();
        }
    }
}

fn mtime(p: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

impl eframe::App for NotesApp {
    /// Carries out the text menu's command (as the keys or clipboard events it stands for), and
    /// the scripted clicks of checks.
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            rivet_kit::scripted_click("RIVET_NOTES", shot.frames(), raw);
            // RIVET_NOTES_KEY=end presses that key once the window has settled.
            if shot.frames() == 40
                && let Ok(k) = std::env::var("RIVET_NOTES_KEY")
            {
                let (modifiers, name) = match k.strip_prefix("ctrl+") {
                    Some(n) => (Modifiers::COMMAND, n.to_string()),
                    None => (Modifiers::NONE, k),
                };
                if let Some(key) = Key::from_name(&name) {
                    raw.events.push(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers });
                }
            }
            // RIVET_NOTES_DRAG=X1,Y1>X2,Y2: press at one point, move to the other, let go.
            if let Some((a, b)) = std::env::var("RIVET_NOTES_DRAG").ok().and_then(|v| {
                let (a, b) = v.split_once('>')?;
                let pt = |s: &str| {
                    let (x, y) = s.split_once(',')?;
                    Some(egui::Pos2::new(x.trim().parse().ok()?, y.trim().parse().ok()?))
                };
                Some((pt(a)?, pt(b)?))
            }) {
                let f = shot.frames();
                let button = egui::PointerButton::Primary;
                match f {
                    30 => raw.events.push(egui::Event::PointerMoved(a)),
                    31 => raw.events.push(egui::Event::PointerButton { pos: a, button, pressed: true, modifiers: Default::default() }),
                    32..=45 => raw.events.push(egui::Event::PointerMoved(a + (b - a) * ((f - 31) as f32 / 14.0))),
                    // Hold still over the target for a while (so the drop marker can be seen).
                    46..=59 if std::env::var_os("RIVET_NOTES_DRAG_HOLD").is_some() => {}
                    46 | 60 => raw.events.push(egui::Event::PointerButton { pos: b, button, pressed: false, modifiers: Default::default() }),
                    _ => {}
                }
            }
        }
        let Some(cmd) = self.edit_cmd.take() else { return };
        let key = |key: Key, modifiers: Modifiers| egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers };
        let shift_cmd = Modifiers { shift: true, ..Modifiers::COMMAND };
        match cmd {
            EditCmd::Undo => raw.events.push(key(Key::Z, Modifiers::COMMAND)),
            EditCmd::Redo => raw.events.push(key(Key::Z, shift_cmd)),
            EditCmd::Cut => raw.events.push(egui::Event::Cut),
            EditCmd::Copy => raw.events.push(egui::Event::Copy),
            // The desktop's clipboard is read by the window, which then sends the paste.
            EditCmd::Paste => ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste),
            EditCmd::Delete => raw.events.push(key(Key::Delete, Modifiers::NONE)),
            EditCmd::SelectAll => raw.events.push(key(Key::A, Modifiers::COMMAND)),
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        // Fonts first, so the look doesn't load the default interface font as well.
        self.look.set_fonts(&ctx, &self.s.ui_font, &self.s.editor_font, &self.s.mono_font);
        self.look.frame(&ctx, &c, self.shot.is_none());
        // Rivet's icon kit, in the theme's colors (they change with it).
        let kit = self.s.builtin_icons.then(|| {
            let v = ctx.global_style().visuals.clone();
            rivet_look::builtin_icons::BuiltinColors { accent: self.look.accent(&c), bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }
        });
        self.icons.set_builtin(kit);
        self.shortcuts(&ctx);
        let now = ctx.input(|i| i.time);
        // Save after a pause in typing; pick up changes made elsewhere every few seconds.
        if let Some(d) = &self.doc
            && d.dirty
        {
            if now - d.edited_at >= SAVE_AFTER {
                self.save_now();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(SAVE_AFTER));
            }
        }
        if self.vault.is_some() && now - self.scanned_at > 5.0 && self.shot.is_none() {
            self.scanned_at = now;
            if self.notes.len() < 5000 {
                self.rescan();
            }
            if let Some(d) = &self.doc
                && mtime(&d.path) != d.disk
            {
                if d.dirty {
                    self.prompt = Some(Prompt::Conflict);
                } else {
                    let p = d.path.clone();
                    self.doc = None;
                    if p.exists() {
                        self.open_note(&p);
                    }
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(5));
        }
        // The window's title names the vault (the toolbar no longer does).
        let title = match &self.vault {
            Some(_) => format!("{} – Rivet Notes", self.vault_name()),
            None => "Rivet Notes".to_string(),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        self.draw(ui, &ctx);
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
    }
}
