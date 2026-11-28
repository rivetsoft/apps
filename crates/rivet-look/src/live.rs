//! What every Rivet app does to look like the others: the desktop's UI font, the theme built
//! from a light/dark choice and an accent, and the desktop's generated colors followed live
//! (their files are checked every two seconds, on a thread that wakes the window only when one
//! changed).

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use eframe::egui::{self, Color32};

use crate::{fonts, palette, theme};

#[derive(Default)]
pub struct LiveLook {
    palette: Option<palette::Palette>,
    watched: Vec<(PathBuf, Option<SystemTime>)>,
    checked: f64,
    /// The thread looking at the colour files, once a live frame has started it.
    watcher: Option<Arc<Watcher>>,
    /// No thread: something else redraws when the colours change (see `without_watcher`).
    no_watcher: bool,
    key: Option<String>,
    fonts_set: bool,
    /// The app's own colours for dark and for light, and what they were made for (see `set_own`).
    own: [Option<palette::Palette>; 2],
    own_key: String,
    /// The fonts last loaded by `set_fonts` (interface, editor, monospace).
    font_key: Option<(String, String, String)>,
}

fn mtime(p: &std::path::Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// What the colour-file thread shares with the window: the files and their times as last read,
/// and whether one changed since.
struct Watcher {
    files: Mutex<Vec<(PathBuf, Option<SystemTime>)>>,
    changed: AtomicBool,
}

impl Watcher {
    /// Look at `files` every two seconds and wake the window when one changed; stops when the
    /// `LiveLook` is gone.
    fn start(files: Vec<(PathBuf, Option<SystemTime>)>, ctx: egui::Context) -> Arc<Watcher> {
        let w = Arc::new(Watcher { files: Mutex::new(files), changed: AtomicBool::new(false) });
        let shared = w.clone();
        std::thread::spawn(move || {
            while Arc::strong_count(&shared) > 1 {
                std::thread::sleep(std::time::Duration::from_secs(2));
                let mut files = shared.files.lock().unwrap_or_else(|e| e.into_inner());
                let mut moved = false;
                for (p, t) in files.iter_mut() {
                    let now = mtime(p);
                    if now != *t {
                        *t = now;
                        moved = true;
                    }
                }
                drop(files);
                if moved {
                    shared.changed.store(true, Ordering::Relaxed);
                    ctx.request_repaint();
                }
            }
        });
        w
    }
}

/// The app's choices.
#[derive(Clone, Copy, Debug)]
pub struct Choice {
    pub mode: theme::Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Text size in points; 0 follows the desktop's.
    pub text_size: f32,
}

/// The font family for an app's editor text (set up by `LiveLook::set_fonts`).
pub fn editor_family() -> egui::FontFamily {
    egui::FontFamily::Name("editor".into())
}

/// The editor font's bold face (the regular one when it has none).
pub fn editor_bold_family() -> egui::FontFamily {
    egui::FontFamily::Name("editor-bold".into())
}

impl LiveLook {
    /// Use these fonts (family names; empty: the interface follows the desktop, the editor
    /// follows the interface, monospace follows the system's). Loaded only when they change.
    /// Families: Proportional (interface), `editor_family()`, Monospace.
    pub fn set_fonts(&mut self, ctx: &egui::Context, ui_font: &str, editor_font: &str, mono_font: &str) {
        let key = (ui_font.to_string(), editor_font.to_string(), mono_font.to_string());
        if self.font_key.as_ref() == Some(&key) {
            return;
        }
        self.font_key = Some(key);
        self.fonts_set = true;
        let mut defs = egui::FontDefinitions::default();
        let mut load = |name: &str, path: Option<std::path::PathBuf>| -> bool {
            match path.and_then(|p| fonts::data(&p)) {
                Some(data) => {
                    defs.font_data.insert(name.into(), data);
                    true
                }
                None => false,
            }
        };
        let ui_ok = load("ui-font", fonts::resolve(ui_font).map(|(_, p)| p));
        let ed_ok = !editor_font.is_empty() && load("editor-font", fonts::file_for(editor_font));
        let mono_ok = load("mono-font", fonts::resolve_mono(mono_font).map(|(_, p)| p));
        let base: Vec<String> = defs.families.get(&egui::FontFamily::Proportional).cloned().unwrap_or_default();
        if ui_ok {
            defs.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui-font".into());
        }
        if mono_ok {
            defs.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "mono-font".into());
        }
        // The editor: its own font, else the interface's; the usual fallbacks after it.
        let mut ed: Vec<String> = Vec::new();
        if ed_ok {
            ed.push("editor-font".into());
        }
        if ui_ok {
            ed.push("ui-font".into());
        }
        ed.extend(base);
        // Its bold face, found by family name (the editor's, else the interface's).
        let fam = if ed_ok { Some(editor_font.to_string()) } else { fonts::resolve(ui_font).map(|(f, _)| f) };
        let bold_ok = fam.and_then(|f| fonts::file_for_style(&f, "Bold")).and_then(|p| fonts::data(&p)).map(|data| {
            defs.font_data.insert("editor-bold".into(), data);
        });
        let mut bold = ed.clone();
        if bold_ok.is_some() {
            bold.insert(0, "editor-bold".into());
        }
        defs.families.insert(editor_family(), ed);
        defs.families.insert(editor_bold_family(), bold);
        ctx.set_fonts(defs);
        fonts::install_fallback(ctx);
    }

    /// Colours of the app's own, for its dark style and its light one, shown in place of any
    /// desktop palette (both None: none). `make` runs only when `key` changes.
    pub fn set_own(&mut self, key: &str, make: impl FnOnce() -> [Option<palette::Palette>; 2]) {
        if self.own_key != key {
            self.own_key = key.to_string();
            self.own = make();
            self.key = None;
        }
    }

    fn has_own(&self) -> bool {
        self.own.iter().any(|p| p.is_some())
    }

    /// The app's own palette for the mode in effect.
    fn own_palette(&self, ctx: &egui::Context, mode: theme::Mode) -> Option<&palette::Palette> {
        let dark = match mode {
            theme::Mode::Dark => true,
            theme::Mode::Light => false,
            theme::Mode::System => ctx.system_theme().map(|t| t == egui::Theme::Dark).unwrap_or_else(palette::prefers_dark),
        };
        self.own[if dark { 0 } else { 1 }].as_ref()
    }

    pub fn new() -> LiveLook {
        let (pal, watched) = palette::load();
        LiveLook { palette: pal, watched: watched.into_iter().map(|p| { let t = mtime(&p); (p, t) }).collect(), ..Default::default() }
    }

    /// No thread watching the colour files, for a program that already watches them and redraws
    /// (Rivet Shell's surfaces): a live frame still looks at them every two seconds.
    pub fn without_watcher(mut self) -> LiveLook {
        self.no_watcher = true;
        self
    }

    /// The desktop's colors, when there are any.
    pub fn palette(&self) -> Option<&palette::Palette> {
        self.palette.as_ref()
    }

    /// The accent in use: the desktop's when followed, else the chosen one.
    pub fn accent(&self, c: &Choice) -> Color32 {
        // The app's own colours: its own accent (a colour file written from them may be a moment old).
        if self.has_own() {
            return Color32::from_rgb(c.accent[0], c.accent[1], c.accent[2]);
        }
        let pal = palette::shown(self.palette.as_ref(), c.follow_desktop_colors);
        pal.and_then(|p| p.accent).unwrap_or(Color32::from_rgb(c.accent[0], c.accent[1], c.accent[2]))
    }

    /// Call once per frame, first. `live`: keep checking the desktop's colors (not in checks).
    pub fn frame(&mut self, ctx: &egui::Context, c: &Choice, live: bool) {
        if !self.fonts_set {
            self.fonts_set = true;
            let mut defs = egui::FontDefinitions::default();
            if let Some((_, path)) = fonts::resolve("")
                && let Some(data) = fonts::data(&path)
            {
                defs.font_data.insert("ui-font".into(), data);
                defs.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "ui-font".into());
            }
            ctx.set_fonts(defs);
        }
        crate::a11y::setup(ctx, live);
        let now = ctx.input(|i| i.time);
        if live {
            // Watched also when not following: a palette chosen in Rivet is taken up all the same.
            if self.watcher.is_none() && !self.no_watcher {
                self.watcher = Some(Watcher::start(self.watched.clone(), ctx.clone()));
            }
            let woken = self.watcher.as_ref().is_some_and(|w| w.changed.swap(false, Ordering::Relaxed));
            if woken || now - self.checked >= 2.0 || self.checked == 0.0 {
                self.checked = now;
                if self.watched.iter().any(|(p, t)| mtime(p) != *t) {
                    *self = LiveLook { fonts_set: true, key: self.key.take(), font_key: self.font_key.take(), own: std::mem::take(&mut self.own), own_key: std::mem::take(&mut self.own_key), checked: now, watcher: self.watcher.take(), no_watcher: self.no_watcher, ..LiveLook::new() };
                    if let Some(w) = &self.watcher {
                        *w.files.lock().unwrap_or_else(|e| e.into_inner()) = self.watched.clone();
                    }
                }
            }
        }
        let size = if c.text_size > 0.0 { c.text_size } else { fonts::desktop_font_cached().map(|(_, pt)| fonts::points_to_ui(pt)).unwrap_or(fonts::BASE_SIZE) };
        fonts::set_scale(size / fonts::BASE_SIZE);
        let accent = self.accent(c);
        let pal = if self.has_own() { self.own_palette(ctx, c.mode) } else { palette::shown(self.palette.as_ref(), c.follow_desktop_colors) }.cloned();
        let pal = pal.as_ref();
        let key = format!("{:?}{accent:?}{pal:?}{size}{}", c.mode, crate::a11y::generation());
        if self.key.as_ref() == Some(&key) {
            return;
        }
        self.key = Some(key);
        theme::apply(ctx, &theme::Look { mode: c.mode, accent, palette: pal, terminal: false });
    }
}
