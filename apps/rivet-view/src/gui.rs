//! Rivet View's window: the picture in the middle, the tools above it, a strip of the folder's
//! pictures below it, and the details at the side. Reading pictures, listing the folder,
//! saving and moving to the trash all run on threads and report back, so a frame never waits.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align, Align2, Color32, Key, Layout, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::{r_lg, r_md, r_sm};
use rivet_look::{fonts, palette};
use rivet_media::stage::{Shown, Stage, Zoom};
use rivet_media::thumbs::Thumbnailer;
use rivet_xdg::launch::{self, DesktopApp};
use rivet_xdg::mime::MimeDb;

use crate::folder::{self, Item, Order};
use crate::loader::{Loader, Ready};
use crate::save::{self, How};
use crate::settings::Settings;

pub const ICON_SVG: &str = include_str!("../data/rivet-view.svg");

/// What the window opens on.
#[derive(Default)]
pub struct Start {
    pub path: Option<PathBuf>,
    pub order: Order,
    pub slideshow: bool,
}

enum Msg {
    Mime(Arc<MimeDb>),
    Listed { items: Vec<Item>, focus: Option<PathBuf> },
    Apps(Vec<DesktopApp>),
    Picked(Result<Option<PathBuf>, String>),
    Saved(PathBuf, Result<(), String>),
    Trashed(Item, usize, Result<PathBuf, String>),
    Restored(Item, usize, Result<(), String>),
    Said(String, bool),
}

/// What waits on the answer to "Save the turned picture?".
#[derive(Clone, Copy, PartialEq, Eq)]
enum Then {
    Go(usize),
    Open,
}

/// The line over the bottom of the picture: what happened, whether it's trouble, and a picture
/// that can be taken back out of the trash.
struct Note {
    text: String,
    bad: bool,
    undo: Option<(Item, usize, PathBuf)>,
    until: f64,
}

struct Slideshow {
    next_at: f64,
    paused: bool,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Glyph {
    Back,
    Forward,
    ZoomIn,
    ZoomOut,
    TurnLeft,
    TurnRight,
    Flip,
    Play,
    Details,
    Open,
}

pub struct ViewApp {
    look: LiveLook,
    s: Settings,
    shot: Option<rivet_kit::Shot>,
    order: Order,
    items: Vec<Item>,
    idx: usize,
    mime: Option<Arc<MimeDb>>,
    apps: Vec<DesktopApp>,
    loader: Loader,
    stage: Stage,
    thumbs: Thumbnailer,
    /// When the picture shown was first drawn, for its animation.
    anim_start: f64,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    slideshow: Option<Slideshow>,
    fullscreen: bool,
    note: Option<Note>,
    ask: Option<Then>,
    saving: bool,
    picking: bool,
    /// Rivet Shell answers in this session (for Set as Wallpaper).
    shell: bool,
    title: String,
    crash: Option<Box<rivet_look::crash::Notice>>,
}

/// Bytes as a person reads them: "5 bytes", "1.2 MB".
fn human_size(n: u64) -> String {
    const UNITS: [&str; 5] = ["kB", "MB", "GB", "TB", "PB"];
    if n < 1000 {
        return if n == 1 { "1 byte".into() } else { format!("{n} bytes") };
    }
    let (mut v, mut i) = (n as f64 / 1000.0, 0);
    while v >= 1000.0 && i < UNITS.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    if v < 10.0 { format!("{v:.1} {}", UNITS[i]) } else { format!("{v:.0} {}", UNITS[i]) }
}

/// A time on this computer's clock: "2026-11-27 13:05".
fn local_time(t: std::time::SystemTime) -> String {
    let secs = t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as libc::time_t).unwrap_or(0);
    // SAFETY: localtime_r only writes into the tm it's given.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    unsafe { libc::localtime_r(&secs, &mut tm) };
    format!("{:04}-{:02}-{:02} {:02}:{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday, tm.tm_hour, tm.tm_min)
}

/// A folder as a person would say it: "~/Pictures/Trip".
fn tidy(p: &Path) -> String {
    match p.strip_prefix(rivet_xdg::places::home()) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    }
}

/// Rivet Shell's socket in this session, as the shell itself names it.
fn shell_running() -> bool {
    let dir = std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let display = std::env::var("WAYLAND_DISPLAY").unwrap_or_else(|_| "wayland-0".into());
    std::os::unix::net::UnixStream::connect(dir.join(format!("rivet-shell-{}.sock", display.replace('/', "_")))).is_ok()
}

fn env_on(name: &str) -> bool {
    std::env::var(name).is_ok_and(|v| !v.is_empty() && v != "0")
}

impl ViewApp {
    pub fn new(cc: &eframe::CreationContext, start: Start, screenshot: Option<PathBuf>) -> ViewApp {
        let (tx, rx) = channel();
        let checking = screenshot.is_some();
        let mut s = if checking { Settings::default() } else { Settings::load() };
        let ctx = cc.egui_ctx.clone();
        let mut app = ViewApp {
            look: LiveLook::new(),
            shot: screenshot.map(rivet_kit::Shot::new),
            order: start.order,
            items: Vec::new(),
            idx: 0,
            mime: None,
            apps: Vec::new(),
            loader: Loader::new(&ctx),
            stage: Stage::default(),
            thumbs: Thumbnailer::new(ctx.clone()),
            anim_start: 0.0,
            tx,
            rx,
            slideshow: None,
            fullscreen: false,
            note: None,
            ask: None,
            saving: false,
            picking: false,
            shell: !checking && shell_running(),
            title: String::new(),
            crash: if checking { None } else { rivet_look::crash::Notice::unseen(crate::CRASH).map(Box::new) },
            s: Settings::default(),
        };
        let mut path = start.path;
        if checking {
            // Screenshot runs: the made-up pictures, and the scene the variables ask for.
            s.details = env_on("RIVET_VIEW_DETAILS");
            s.strip = !env_on("RIVET_VIEW_NO_STRIP");
            app.shell = true;
            if path.is_none() && !env_on("RIVET_VIEW_EMPTY") {
                let dir = rivet_xdg::places::home().join("Pictures/Coast trip");
                match crate::demo::make(&dir) {
                    Ok(made) => {
                        let pick = std::env::var("RIVET_VIEW_PICK").ok().and_then(|n| n.parse::<usize>().ok()).unwrap_or(1);
                        path = made.get(pick.min(made.len().saturating_sub(1))).cloned();
                    }
                    Err(e) => eprintln!("rivet-view: couldn't make the sample pictures: {e}"),
                }
            }
        }
        app.s = s;
        // The desktop's file types (for the details and the thumbnails), then the folder.
        {
            let tx = app.tx.clone();
            let ctx = ctx.clone();
            let (path, order) = (path.clone(), app.order);
            std::thread::spawn(move || {
                let db = Arc::new(MimeDb::load());
                let _ = tx.send(Msg::Mime(db.clone()));
                if let Some(p) = path {
                    let (dir, focus) = if p.is_dir() { (p.clone(), None) } else { (p.parent().map(PathBuf::from).unwrap_or_default(), Some(p.clone())) };
                    let items = folder::list(&dir, focus.as_deref(), order, &db);
                    let _ = tx.send(Msg::Listed { items, focus });
                }
                ctx.request_repaint();
            });
        }
        if !checking {
            let tx = app.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::Apps(launch::all_apps()));
            });
        }
        if let Some(p) = path.filter(|p| p.is_file()) {
            app.items = vec![Item::of(&p, None)];
        }
        if start.slideshow || (checking && env_on("RIVET_VIEW_SLIDESHOW")) {
            app.start_slideshow(&ctx);
        }
        if checking {
            match std::env::var("RIVET_VIEW_ZOOM").as_deref() {
                Ok("100") => app.stage.actual_size(),
                Ok(z) => {
                    if let Ok(f) = z.parse::<f32>() {
                        app.stage.zoom = Zoom::Scale(f);
                    }
                }
                Err(_) => {}
            }
            if env_on("RIVET_VIEW_TURN") {
                app.stage.turn_right();
            }
        }
        app
    }

    fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    fn current(&self) -> Option<&Item> {
        self.items.get(self.idx)
    }

    fn say(&mut self, ctx: &egui::Context, text: impl Into<String>, bad: bool) {
        let now = ctx.input(|i| i.time);
        self.note = Some(Note { text: text.into(), bad, undo: None, until: now + if bad { 8.0 } else { 4.0 } });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Mime(db) => {
                    for it in &mut self.items {
                        if let Some(m) = db.guess_by_name(&it.path) {
                            it.mime = m;
                        }
                    }
                    self.mime = Some(db);
                }
                Msg::Listed { items, focus } => {
                    let keep = focus.or_else(|| self.current().map(|i| i.path.clone()));
                    let mut items = items;
                    let at = keep.as_ref().and_then(|k| items.iter().position(|i| &i.path == k));
                    let idx = match (at, keep) {
                        (Some(i), _) => i,
                        // Opened by a name that doesn't look like a picture: show it all the same.
                        (None, Some(k)) if k.is_file() => {
                            items.insert(0, Item::of(&k, self.mime.as_deref()));
                            0
                        }
                        _ => 0,
                    };
                    let changed = self.current().map(|i| i.path.clone()) != items.get(idx).map(|i| i.path.clone());
                    self.items = items;
                    self.idx = idx;
                    if changed {
                        self.stage.reset();
                        self.anim_start = ctx.input(|i| i.time);
                    }
                }
                Msg::Apps(apps) => self.apps = apps,
                Msg::Picked(r) => {
                    self.picking = false;
                    match r {
                        Ok(Some(p)) => self.open(ctx, p),
                        Ok(None) => {}
                        Err(e) => self.say(ctx, format!("The file picker couldn't open: {e}"), true),
                    }
                }
                Msg::Saved(path, r) => {
                    self.saving = false;
                    match r {
                        Ok(()) => {
                            self.loader.forget(&path);
                            if let Some(it) = self.items.iter_mut().find(|i| i.path == path) {
                                *it = Item::of(&path, self.mime.as_deref());
                            }
                            if self.current().is_some_and(|i| i.path == path) {
                                self.stage.reset();
                            }
                            self.say(ctx, "Saved turned.", false);
                        }
                        Err(e) => self.say(ctx, e, true),
                    }
                }
                Msg::Trashed(item, at, r) => match r {
                    Ok(trashed) => {
                        if let Some(i) = self.items.iter().position(|i| i.path == item.path) {
                            self.items.remove(i);
                            if self.idx >= self.items.len() {
                                self.idx = self.items.len().saturating_sub(1);
                            }
                            self.stage.reset();
                            self.anim_start = ctx.input(|i| i.time);
                        }
                        self.loader.forget(&item.path);
                        let text = format!("Moved “{}” to the trash.", item.name);
                        let now = ctx.input(|i| i.time);
                        self.note = Some(Note { text, bad: false, undo: Some((item, at, trashed)), until: now + 8.0 });
                    }
                    Err(e) => self.say(ctx, e, true),
                },
                Msg::Restored(item, at, r) => match r {
                    Ok(()) => {
                        let at = at.min(self.items.len());
                        self.items.insert(at, item);
                        self.idx = at;
                        self.stage.reset();
                        self.note = None;
                    }
                    Err(e) => self.say(ctx, e, true),
                },
                Msg::Said(text, bad) => self.say(ctx, text, bad),
            }
        }
    }

    /// Open a picture (or a folder's first) and list the rest of its folder.
    fn open(&mut self, ctx: &egui::Context, p: PathBuf) {
        let (dir, focus) = if p.is_dir() { (p.clone(), None) } else { (p.parent().map(PathBuf::from).unwrap_or_default(), Some(p.clone())) };
        if let Some(f) = &focus {
            self.items = vec![Item::of(f, self.mime.as_deref())];
            self.idx = 0;
        }
        self.stage.reset();
        self.anim_start = ctx.input(|i| i.time);
        let (tx, order, db, c) = (self.tx.clone(), self.order, self.mime.clone(), ctx.clone());
        std::thread::spawn(move || {
            let db = db.unwrap_or_else(|| Arc::new(MimeDb::load()));
            let items = folder::list(&dir, focus.as_deref(), order, &db);
            let _ = tx.send(Msg::Listed { items, focus });
            c.request_repaint();
        });
    }

    fn pick(&mut self, ctx: &egui::Context) {
        if self.turn_pending() {
            self.ask = Some(Then::Open);
            return;
        }
        if self.picking {
            return;
        }
        self.picking = true;
        let (tx, c) = (self.tx.clone(), ctx.clone());
        let start = self.current().and_then(|i| i.path.parent().map(PathBuf::from));
        std::thread::spawn(move || {
            let r = rivet_xdg::portal::choose(rivet_xdg::portal::Pick::Picture, "Open a Picture", start.as_deref());
            let _ = tx.send(Msg::Picked(r));
            c.request_repaint();
        });
    }

    /// The picture shown, when it's been read.
    fn ready(&mut self) -> Option<Arc<Ready>> {
        let p = self.current()?.path.clone();
        self.loader.get(&p)
    }

    /// How saving the turn would go, when the picture is turned.
    fn how_to_save(&mut self) -> Option<How> {
        if self.stage.orient.is_none() {
            return None;
        }
        let animated = self.ready().is_some_and(|r| r.animated());
        Some(save::how(&self.current()?.path, animated))
    }

    /// Turned in a way that could be saved, and not saved yet.
    fn turn_pending(&mut self) -> bool {
        self.how_to_save().is_some_and(|h| h != How::No)
    }

    fn go(&mut self, ctx: &egui::Context, to: usize) {
        if to >= self.items.len() || to == self.idx {
            return;
        }
        if self.turn_pending() {
            self.ask = Some(Then::Go(to));
            return;
        }
        self.idx = to;
        self.stage.reset();
        self.anim_start = ctx.input(|i| i.time);
    }

    fn step(&mut self, ctx: &egui::Context, by: isize) {
        let n = self.items.len() as isize;
        if n == 0 {
            return;
        }
        let to = self.idx as isize + by;
        if (0..n).contains(&to) {
            self.go(ctx, to as usize);
        }
    }

    fn save_turn(&mut self, ctx: &egui::Context) {
        let (Some(item), Some(ready)) = (self.current().cloned(), self.ready()) else { return };
        if self.saving {
            return;
        }
        self.saving = true;
        let (tx, c, turn, had, animated) = (self.tx.clone(), ctx.clone(), self.stage.orient, ready.orient, ready.animated());
        std::thread::spawn(move || {
            let r = save::save(&item.path, had, turn, animated);
            let _ = tx.send(Msg::Saved(item.path, r));
            c.request_repaint();
        });
    }

    fn trash(&mut self, ctx: &egui::Context) {
        let Some(item) = self.current().cloned() else { return };
        let (tx, c, at) = (self.tx.clone(), ctx.clone(), self.idx);
        std::thread::spawn(move || {
            let r = rivet_xdg::trash::trash(&item.path).map_err(|e| format!("“{}” couldn't be moved to the trash ({e}).", item.name));
            let _ = tx.send(Msg::Trashed(item, at, r));
            c.request_repaint();
        });
    }

    /// Take a picture back out of the trash, to where it was.
    fn restore(&mut self, ctx: &egui::Context, item: Item, at: usize, trashed: PathBuf) {
        let (tx, c) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = if item.path.exists() {
                Err(format!("“{}” is in the trash still: another file has its name now.", item.name))
            } else {
                std::fs::rename(&trashed, &item.path).map_err(|e| format!("“{}” couldn't be put back ({e}).", item.name)).map(|()| {
                    if let (Some(files), Some(name)) = (trashed.parent(), trashed.file_name()) {
                        let info = files.with_file_name("info").join(format!("{}.trashinfo", name.to_string_lossy()));
                        let _ = std::fs::remove_file(info);
                    }
                })
            };
            let _ = tx.send(Msg::Restored(item, at, r));
            c.request_repaint();
        });
    }

    /// Put the picture on the clipboard: the file itself for the kinds every app reads, else
    /// the picture as a PNG.
    fn copy(&mut self, ctx: &egui::Context) {
        let Some(item) = self.current().cloned() else { return };
        let (tx, c) = (self.tx.clone(), ctx.clone());
        std::thread::spawn(move || {
            let r = copy_picture(&item, &c);
            let _ = tx.send(match r {
                Ok(()) => Msg::Said(format!("Copied “{}”.", item.name), false),
                Err(e) => Msg::Said(e, true),
            });
            c.request_repaint();
        });
    }

    fn set_wallpaper(&mut self, ctx: &egui::Context) {
        let Some(item) = self.current().cloned() else { return };
        let shell = launch::rivet_program("rivet-shell").unwrap_or_else(|| PathBuf::from("rivet-shell"));
        let args = ["msg".to_string(), "wallpaper".into(), item.path.to_string_lossy().to_string()];
        match launch::spawn(&shell, &args) {
            Ok(()) => self.say(ctx, format!("“{}” is the wallpaper now.", item.name), false),
            Err(_) => self.say(ctx, "Rivet Shell couldn't be reached to change the wallpaper.", true),
        }
    }

    fn start_slideshow(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.slideshow = Some(Slideshow { next_at: now + self.s.slideshow_seconds as f64, paused: false });
        self.stage.fit();
        if self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(true));
        }
    }

    fn stop_slideshow(&mut self, ctx: &egui::Context) {
        if self.slideshow.take().is_some() && !self.fullscreen {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
        }
    }

    fn toggle_fullscreen(&mut self, ctx: &egui::Context) {
        self.fullscreen = !self.fullscreen;
        if self.slideshow.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.ask.is_some() || self.crash.is_some() {
            return;
        }
        // Keys typed into a field (a menu's) are the field's.
        if ctx.memory(|m| m.focused().is_some()) {
            return;
        }
        let key = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        let shift = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, k));
        let cmd = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::COMMAND, k));
        if let Some(sl) = &mut self.slideshow {
            if key(Key::Escape) || key(Key::F5) {
                self.stop_slideshow(ctx);
                return;
            }
            if key(Key::Space) {
                sl.paused = !sl.paused;
                sl.next_at = ctx.input(|i| i.time) + self.s.slideshow_seconds as f64;
            }
        } else if key(Key::F5) {
            self.start_slideshow(ctx);
        }
        if (key(Key::Escape) && self.fullscreen) || key(Key::F11) || key(Key::F) {
            self.toggle_fullscreen(ctx);
        }
        if key(Key::ArrowRight) || key(Key::PageDown) || (self.slideshow.is_none() && key(Key::Space)) {
            self.step(ctx, 1);
        }
        if key(Key::ArrowLeft) || key(Key::PageUp) || key(Key::Backspace) {
            self.step(ctx, -1);
        }
        if key(Key::Home) {
            self.go(ctx, 0);
        }
        if key(Key::End) {
            self.go(ctx, self.items.len().saturating_sub(1));
        }
        if key(Key::Plus) || key(Key::Equals) || shift(Key::Equals) || shift(Key::Plus) || cmd(Key::Plus) || cmd(Key::Equals) {
            self.stage.zoom_by(1.25);
        }
        if key(Key::Minus) || cmd(Key::Minus) {
            self.stage.zoom_by(0.8);
        }
        if key(Key::Num0) || cmd(Key::Num0) {
            self.stage.fit();
        }
        if key(Key::Num1) || cmd(Key::Num1) {
            self.stage.actual_size();
        }
        if key(Key::R) {
            self.stage.turn_right();
        }
        if shift(Key::R) {
            self.stage.turn_left();
        }
        if key(Key::H) {
            self.stage.flip_across();
        }
        if key(Key::V) {
            self.stage.flip_down();
        }
        if key(Key::I) {
            self.s.details = !self.s.details;
            self.s.save();
        }
        if key(Key::Delete) {
            self.trash(ctx);
        }
        if cmd(Key::C) {
            self.copy(ctx);
        }
        if cmd(Key::O) {
            self.pick(ctx);
        }
        if cmd(Key::S) && self.turn_pending() {
            self.save_turn(ctx);
        }
        if cmd(Key::W) || cmd(Key::Q) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    // ------------------------------------------------------------------ the bars

    fn glyph_button(&mut self, ui: &mut egui::Ui, g: Glyph, tip: &str, enabled: bool, on: bool) -> bool {
        let v = ui.visuals().clone();
        let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.0), if enabled { Sense::click() } else { Sense::hover() });
        let accent = v.selection.bg_fill;
        let fill = if on {
            palette::mix(v.panel_fill, accent, 0.28)
        } else if resp.hovered() && enabled {
            palette::mix(v.panel_fill, v.text_color(), 0.10)
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(r, r_sm(), fill);
        let col = if !enabled { v.weak_text_color().gamma_multiply(0.5) } else if on { v.strong_text_color() } else { v.text_color() };
        glyph(ui.painter(), r.shrink(8.0), g, col);
        resp.on_hover_text(tip).clicked() && enabled
    }

    fn divider(ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        ui.add_space(4.0);
        let (line, _) = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, palette::mix(v.panel_fill, v.text_color(), 0.18));
        ui.add_space(4.0);
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let has = self.current().is_some();
        let (n, idx) = (self.items.len(), self.idx);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            if self.glyph_button(ui, Glyph::Open, "Open a picture (Ctrl+O)", true, false) {
                self.pick(&ctx);
            }
            Self::divider(ui);
            if self.glyph_button(ui, Glyph::Back, "Previous (Left)", idx > 0, false) {
                self.step(&ctx, -1);
            }
            if self.glyph_button(ui, Glyph::Forward, "Next (Right)", idx + 1 < n, false) {
                self.step(&ctx, 1);
            }
            ui.add_space(8.0);
            if let Some(item) = self.current().cloned() {
                let v = ui.visuals().clone();
                ui.label(RichText::new(&item.name).strong().color(v.strong_text_color()));
                ui.add_space(4.0);
                ui.label(RichText::new(format!("{} of {}", idx + 1, n)).color(v.weak_text_color()));
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.menu_button("More", |ui| self.more(ui));
                ui.add_space(3.0);
                let details = self.s.details;
                if self.glyph_button(ui, Glyph::Details, "Details (I)", has, details) {
                    self.s.details = !details;
                    self.s.save();
                }
                if self.glyph_button(ui, Glyph::Play, "Slideshow (F5)", has, false) {
                    self.start_slideshow(&ctx);
                }
                Self::divider(ui);
                if self.glyph_button(ui, Glyph::Flip, "Flip across (H); V flips top to bottom", has, false) {
                    self.stage.flip_across();
                }
                if self.glyph_button(ui, Glyph::TurnRight, "Turn right (R)", has, false) {
                    self.stage.turn_right();
                }
                if self.glyph_button(ui, Glyph::TurnLeft, "Turn left (Shift+R)", has, false) {
                    self.stage.turn_left();
                }
                Self::divider(ui);
                if self.glyph_button(ui, Glyph::ZoomIn, "Zoom in (+)", has, false) {
                    self.stage.zoom_by(1.25);
                }
                let pct = match (self.stage.zoom, self.stage.shown_scale()) {
                    (Zoom::Fit, _) => "Fit".to_string(),
                    (_, Some(s)) => format!("{:.0}%", s * 100.0),
                    _ => "Fit".to_string(),
                };
                let b = egui::Button::new(pct).frame(false).min_size(Vec2::new(52.0, 28.0));
                if ui.add_enabled(has, b).on_hover_text("Fit the window (0) or show at 100% (1)").clicked() {
                    if self.stage.zoom == Zoom::Fit {
                        self.stage.actual_size();
                    } else {
                        self.stage.fit();
                    }
                }
                if self.glyph_button(ui, Glyph::ZoomOut, "Zoom out (−)", has, false) {
                    self.stage.zoom_by(0.8);
                }
            });
        });
    }

    fn more(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.set_min_width(230.0);
        ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
        ui.spacing_mut().item_spacing.y = 3.0;
        let item = |ui: &mut egui::Ui, label: &str, keys: &str, on: bool| -> bool {
            let b = egui::Button::new(label);
            let b = if keys.is_empty() { b } else { b.shortcut_text(keys) };
            let r = ui.add_enabled(on, b).clicked();
            if r {
                ui.close();
            }
            r
        };
        let has = self.current().is_some();
        if item(ui, "Open…", "Ctrl+O", true) {
            self.pick(&ctx);
        }
        if let Some(cur) = self.current().cloned() {
            let choices: Vec<DesktopApp> = launch::apps_for(&self.apps, &cur.mime).into_iter().filter(|a| a.id != "rivet-view.desktop").cloned().collect();
            if !choices.is_empty() {
                ui.menu_button("Open With", |ui| {
                    for a in &choices {
                        if ui.button(&a.name).clicked() {
                            if let Err(e) = launch::launch_app(a, std::slice::from_ref(&cur.path)) {
                                self.say(&ctx, e, true);
                            }
                            ui.close();
                        }
                    }
                });
            }
        }
        ui.separator();
        if item(ui, "Copy", "Ctrl+C", has) {
            self.copy(&ctx);
        }
        if self.shell && item(ui, "Set as Wallpaper", "", has) {
            self.set_wallpaper(&ctx);
        }
        if item(ui, "Move to Trash", "Delete", has) {
            self.trash(&ctx);
        }
        ui.separator();
        if item(ui, "Fill the Screen", "F11", has) {
            self.toggle_fullscreen(&ctx);
        }
        ui.menu_button("Slideshow Speed", |ui| {
            for secs in [3, 5, 10, 20] {
                if ui.radio(self.s.slideshow_seconds == secs, format!("Every {secs} seconds")).clicked() {
                    self.s.slideshow_seconds = secs;
                    self.s.save();
                    ui.close();
                }
            }
        });
        if ui.checkbox(&mut self.s.strip, "Show the strip of pictures").changed() {
            self.s.save();
        }
        ui.separator();
        ui.label(RichText::new(format!("Rivet View {}", env!("CARGO_PKG_VERSION"))).small().color(ui.visuals().weak_text_color()));
    }

    /// The folder's pictures along the bottom, the one shown in the middle.
    fn strip(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let (cell, gap) = (Vec2::new(76.0, 56.0), 6.0);
        let (area, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), cell.y), Sense::hover());
        let fit = (((area.width() + gap) / (cell.x + gap)) as usize).max(1);
        let n = self.items.len();
        let first = if n <= fit { 0 } else { self.idx.saturating_sub(fit / 2).min(n - fit) };
        let shown = fit.min(n);
        let total = shown as f32 * (cell.x + gap) - gap;
        let mut x = area.center().x - total / 2.0;
        let px = (cell.y * ctx.pixels_per_point() * 1.4) as u32;
        let mut go = None;
        for i in first..first + shown {
            let it = &self.items[i];
            let r = Rect::from_min_size(Pos2::new(x, area.top()), cell);
            x += cell.x + gap;
            let resp = ui.interact(r, ui.scope_id().with(("strip", i)), Sense::click());
            ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.extreme_bg_color, 0.6));
            if let Some(t) = self.thumbs.get(&it.path, it.size, &it.mime, it.modified, px) {
                let size = t.size_vec2();
                let scale = (cell.x / size.x).max(cell.y / size.y);
                let uv_w = (cell.x / (size.x * scale)).min(1.0);
                let uv_h = (cell.y / (size.y * scale)).min(1.0);
                let uv = Rect::from_center_size(Pos2::new(0.5, 0.5), Vec2::new(uv_w, uv_h));
                egui::Image::from_texture(&t).uv(uv).corner_radius(r_sm()).paint_at(ui, r);
            } else {
                glyph(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(18.0)), Glyph::Open, v.weak_text_color());
            }
            if i == self.idx {
                ui.painter().rect_stroke(r.expand(1.5), r_sm(), Stroke::new(2.5, v.selection.bg_fill), egui::StrokeKind::Outside);
            } else if resp.hovered() {
                ui.painter().rect_stroke(r, r_sm(), Stroke::new(1.0, v.text_color().gamma_multiply(0.5)), egui::StrokeKind::Inside);
            } else {
                ui.painter().rect_filled(r, r_sm(), Color32::from_black_alpha(60));
            }
            if resp.on_hover_text(&it.name).clicked() {
                go = Some(i);
            }
        }
        if let Some(i) = go {
            self.go(&ctx, i);
        }
    }

    fn details(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let Some(item) = self.current().cloned() else { return };
        let ready = self.ready();
        let card = palette::mix(v.panel_fill, v.extreme_bg_color, 0.5);
        let section = |ui: &mut egui::Ui, title: &str, rows: &[(String, String)]| {
            ui.add_space(10.0);
            ui.label(RichText::new(title.to_uppercase()).small().strong().color(v.weak_text_color()));
            ui.add_space(4.0);
            egui::Frame::new().fill(card).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::Grid::new(title).num_columns(2).spacing(Vec2::new(12.0, 7.0)).show(ui, |ui| {
                    for (k, val) in rows {
                        ui.label(RichText::new(k).color(v.weak_text_color()));
                        ui.add(egui::Label::new(RichText::new(val).color(v.strong_text_color())).wrap());
                        ui.end_row();
                    }
                });
            });
        };
        ui.add_space(4.0);
        ui.add(egui::Label::new(RichText::new(&item.name).size(fonts::ui_font(16.0).size).strong().color(v.strong_text_color())).wrap());
        let kind = self.mime.as_ref().map(|db| db.describe(&item.mime)).unwrap_or_default();
        ui.label(RichText::new(format!("{kind} · {}", human_size(item.size))).color(v.weak_text_color()));
        let mut rows: Vec<(String, String)> = Vec::new();
        if let Some(r) = &ready {
            rows.push(("Size".into(), format!("{} × {} pixels", r.width, r.height)));
            rows.push(("Kind".into(), r.format.clone()));
            if let Some(c) = &r.color {
                rows.push(("Color".into(), c.clone()));
            }
            if r.animated() {
                rows.push(("Frames".into(), format!("{}, {:.1} seconds", r.frames.len(), r.length().as_secs_f64())));
            }
        }
        rows.push(("File size".into(), human_size(item.size)));
        if let Some(m) = item.modified {
            rows.push(("Changed".into(), local_time(m)));
        }
        if let Some(d) = item.path.parent() {
            rows.push(("Folder".into(), tidy(d)));
        }
        section(ui, "Picture", &rows);
        if let Some(r) = &ready
            && !r.exif.is_empty()
        {
            section(ui, "Camera", &r.exif);
        }
    }

    // ------------------------------------------------------------------ the picture

    fn picture(&mut self, ui: &mut egui::Ui, chrome: bool) {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let area = ui.max_rect();
        let now = ctx.input(|i| i.time);
        let bg = if chrome { palette::mix(v.panel_fill, v.extreme_bg_color, 0.6) } else { palette::mix(v.extreme_bg_color, Color32::BLACK, 0.6) };
        ui.painter().rect_filled(area, if chrome { r_lg() } else { 0 }, bg);
        let Some(item) = self.current().cloned() else {
            self.empty(ui, area);
            return;
        };
        let stage_area = if chrome { area.shrink(12.0) } else { area };
        if let Some(ready) = self.ready() {
            let (frame, wait) = ready.frame_at(now - self.anim_start);
            if ready.animated() {
                ctx.request_repaint_after(wait);
            }
            let tex = ready.frames[frame].0.clone();
            self.stage.show(ui, stage_area, Shown { texture: &tex, size: [ready.width, ready.height], alpha: ready.alpha, vector: ready.vector });
            if let Some(n) = &ready.note
                && chrome
            {
                ui.painter().text(Pos2::new(area.center().x, area.bottom() - 14.0), Align2::CENTER_BOTTOM, n, fonts::ui_font(12.0), v.weak_text_color());
            }
        } else if let Some(err) = self.loader.error(&item.path) {
            let err = err.to_string();
            ui.painter().text(area.center() - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, format!("“{}” can't be shown", item.name), fonts::ui_font(15.0), v.strong_text_color());
            ui.painter().text(area.center() + Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, err, fonts::ui_font(13.0), v.weak_text_color());
        } else {
            ui.put(Rect::from_center_size(area.center(), Vec2::splat(28.0)), egui::Spinner::new().size(28.0));
        }
        self.arrows(ui, area);
        self.turn_bar(ui, area);
        self.note_bar(ui, area, now);
        if let Some(sl) = &self.slideshow
            && sl.paused
        {
            let r = Rect::from_center_size(Pos2::new(area.center().x, area.top() + 36.0), Vec2::new(200.0, 34.0));
            ui.painter().rect_filled(r, r_md(), Color32::from_black_alpha(170));
            ui.painter().text(r.center(), Align2::CENTER_CENTER, "Slideshow paused", fonts::ui_font(13.0), Color32::from_gray(235));
        }
    }

    fn empty(&mut self, ui: &mut egui::Ui, area: Rect) {
        let v = ui.visuals().clone();
        let ctx = ui.ctx().clone();
        let c = area.center();
        glyph(ui.painter(), Rect::from_center_size(c - Vec2::new(0.0, 56.0), Vec2::splat(44.0)), Glyph::Open, v.weak_text_color());
        ui.painter().text(c - Vec2::new(0.0, 14.0), Align2::CENTER_CENTER, "No picture open", fonts::ui_font(17.0), v.strong_text_color());
        ui.painter().text(c + Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, "Open one, or drop one here, to look through its folder.", fonts::ui_font(13.0), v.weak_text_color());
        let b = egui::Button::new(RichText::new("Open…").color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm()).min_size(Vec2::new(110.0, 32.0));
        if ui.put(Rect::from_center_size(c + Vec2::new(0.0, 52.0), Vec2::new(110.0, 32.0)), b).clicked() {
            self.pick(&ctx);
        }
    }

    /// Round arrows at the sides, shown while the pointer is near them.
    fn arrows(&mut self, ui: &mut egui::Ui, area: Rect) {
        let ctx = ui.ctx().clone();
        let pointer = ctx.pointer_hover_pos();
        let mut go = None;
        for (dir, x) in [(-1isize, area.left() + 40.0), (1, area.right() - 40.0)] {
            let target = self.idx as isize + dir;
            if target < 0 || target >= self.items.len() as isize {
                continue;
            }
            let r = Rect::from_center_size(Pos2::new(x, area.center().y), Vec2::splat(46.0));
            let near = pointer.is_some_and(|p| area.contains(p) && (p.x - x).abs() < 120.0);
            if !near {
                continue;
            }
            let resp = ui.interact(r, ui.scope_id().with(("arrow", dir)), Sense::click());
            ui.painter().circle_filled(r.center(), 23.0, Color32::from_black_alpha(if resp.hovered() { 190 } else { 120 }));
            glyph(ui.painter(), r.shrink(14.0), if dir < 0 { Glyph::Back } else { Glyph::Forward }, Color32::from_gray(240));
            if resp.on_hover_text(if dir < 0 { "Previous (Left)" } else { "Next (Right)" }).clicked() {
                go = Some(target as usize);
            }
        }
        if let Some(i) = go {
            self.go(&ctx, i);
        }
    }

    /// While the picture is turned: save it that way, or turn it back.
    fn turn_bar(&mut self, ui: &mut egui::Ui, area: Rect) {
        let Some(how) = self.how_to_save() else { return };
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let text = if how == How::No { "Turned for viewing only: this kind of picture can't be saved turned." } else { "Turned for viewing. Save it this way?" };
        let w = if how == How::No { 600.0 } else { 440.0 };
        let r = Rect::from_center_size(Pos2::new(area.center().x, area.bottom() - 40.0), Vec2::new(w, 44.0));
        ui.painter().rect_filled(r, r_md(), v.window_fill);
        ui.painter().rect_stroke(r, r_md(), v.window_stroke, egui::StrokeKind::Inside);
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(Vec2::new(14.0, 6.0))).layout(Layout::left_to_right(Align::Center)));
        child.label(RichText::new(text).color(v.strong_text_color()));
        child.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if how != How::No {
                let b = egui::Button::new(RichText::new("Save").color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm());
                if ui.add_enabled(!self.saving, b).on_hover_text("Ctrl+S").clicked() {
                    self.save_turn(&ctx);
                }
            }
            if ui.add(egui::Button::new("Turn Back").corner_radius(r_sm())).clicked() {
                self.stage.orient = rivet_media::picture::Orient::NONE;
                self.stage.fit();
            }
        });
    }

    fn note_bar(&mut self, ui: &mut egui::Ui, area: Rect, now: f64) {
        let ctx = ui.ctx().clone();
        if self.note.as_ref().is_some_and(|n| now > n.until) {
            self.note = None;
        }
        let turned = self.how_to_save().is_some();
        let Some(note) = &self.note else { return };
        ctx.request_repaint_after(std::time::Duration::from_secs_f64((note.until - now).max(0.05)));
        let v = ui.visuals().clone();
        let font = fonts::ui_font(13.0);
        let tw = ui.painter().layout_no_wrap(note.text.clone(), font.clone(), v.text_color()).size().x;
        let w = tw + 32.0 + if note.undo.is_some() { 80.0 } else { 0.0 };
        let bottom = if turned { area.bottom() - 92.0 } else { area.bottom() - 40.0 };
        let r = Rect::from_center_size(Pos2::new(area.center().x, bottom), Vec2::new(w, 40.0));
        let col = if note.bad { rivet_look::theme::status(&ctx).danger } else { v.strong_text_color() };
        ui.painter().rect_filled(r, r_md(), v.window_fill);
        ui.painter().rect_stroke(r, r_md(), v.window_stroke, egui::StrokeKind::Inside);
        ui.painter().text(Pos2::new(r.left() + 16.0, r.center().y), Align2::LEFT_CENTER, &note.text, font, col);
        if note.undo.is_some() {
            let b = Rect::from_center_size(Pos2::new(r.right() - 46.0, r.center().y), Vec2::new(70.0, 28.0));
            if ui.put(b, egui::Button::new("Undo").corner_radius(r_sm())).clicked()
                && let Some((item, at, trashed)) = self.note.take().and_then(|n| n.undo)
            {
                self.restore(&ctx, item, at, trashed);
            }
        }
    }

    /// "Save the turned picture?", before moving on from one.
    fn ask_ui(&mut self, ctx: &egui::Context) {
        let Some(then) = self.ask else { return };
        let how = self.how_to_save();
        let exif = self.ready().is_some_and(|r| !r.exif.is_empty());
        let body = match how {
            Some(How::Note) => "Only the note saying which way is up changes, so no pixel is lost.".to_string(),
            Some(How::Rewrite(f)) => {
                let name = f.extensions_str().first().map(|e| e.to_uppercase()).unwrap_or_default();
                if exif { format!("It's written again as a {name} with the same pixels. Its camera details aren't kept.") } else { format!("It's written again as a {name} with the same pixels.") }
            }
            _ => {
                self.ask = None;
                return;
            }
        };
        let mut answer: Option<Option<bool>> = None;
        if rivet_look::widgets::return_default(ctx) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter)) {
            answer = Some(Some(true));
        }
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("rivet-view-save")).frame(frame).show(ctx, |ui| {
            ui.set_width(420.0);
            let v = ui.visuals().clone();
            ui.label(RichText::new("Save the turned picture?").heading().color(v.strong_text_color()));
            ui.add_space(4.0);
            ui.label(RichText::new(body).color(v.weak_text_color()));
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                if ui.add(egui::Button::new(RichText::new("Save Turned").color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked() {
                    answer = Some(Some(true));
                }
                if ui.add(egui::Button::new("Leave As It Was").corner_radius(r_sm())).clicked() {
                    answer = Some(Some(false));
                }
                if ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(None);
                }
            });
        });
        let Some(answer) = answer else { return };
        self.ask = None;
        match answer {
            None => {}
            Some(save) => {
                if save {
                    self.save_turn(ctx);
                }
                self.stage.orient = rivet_media::picture::Orient::NONE;
                match then {
                    Then::Go(i) => self.go(ctx, i),
                    Then::Open => self.pick(ctx),
                }
            }
        }
    }
}

/// Copy the picture to the clipboard, through wl-copy or xclip.
fn copy_picture(item: &Item, ctx: &egui::Context) -> Result<(), String> {
    use std::io::Write;
    use std::process::{Command, Stdio};
    let direct = matches!(item.mime.as_str(), "image/png" | "image/jpeg" | "image/gif" | "image/webp");
    let (bytes, mime) = if direct {
        (std::fs::read(&item.path).map_err(|e| format!("The picture couldn't be read ({e})."))?, item.mime.clone())
    } else {
        let p = rivet_media::picture::decode(&item.path, 8192)?;
        let f = p.frames.into_iter().next().ok_or("The picture couldn't be read.")?;
        (rivet_kit::png_bytes(&f.image)?, "image/png".to_string())
    };
    let has = |bin: &str| std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()));
    let cmd: Option<Vec<String>> = if std::env::var_os("WAYLAND_DISPLAY").is_some() && has("wl-copy") {
        Some(vec!["wl-copy".into(), "--type".into(), mime.clone()])
    } else if std::env::var_os("DISPLAY").is_some() && has("xclip") {
        Some(vec!["xclip".into(), "-selection".into(), "clipboard".into(), "-t".into(), mime.clone()])
    } else {
        None
    };
    let Some(cmd) = cmd else {
        // The window's own clipboard, which holds it while Rivet View is open.
        let img = image::load_from_memory(&bytes).map_err(|_| "The picture couldn't be copied.".to_string())?.into_rgba8();
        ctx.copy_image(egui::ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()));
        return Ok(());
    };
    let mut child = Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null()).spawn().map_err(|_| "The picture couldn't be copied.".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(&bytes).map_err(|_| "The picture couldn't be copied.".to_string())?;
    }
    // wl-copy and xclip stay behind to offer it; they're waited for off to the side.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// A tool's picture on its button.
fn glyph(p: &egui::Painter, r: Rect, g: Glyph, col: Color32) {
    let s = Stroke::new(1.6, col);
    let at = |x: f32, y: f32| r.min + Vec2::new(r.width() * x, r.height() * y);
    let arc = |cx: f32, cy: f32, rad: f32, from: f32, to: f32| -> Vec<Pos2> {
        (0..=14).map(|i| {
            let a = from + (to - from) * i as f32 / 14.0;
            at(cx + a.cos() * rad, cy + a.sin() * rad)
        }).collect()
    };
    use std::f32::consts::PI;
    match g {
        Glyph::Back => {
            p.add(egui::epaint::PathShape::line(vec![at(0.66, 0.1), at(0.28, 0.5), at(0.66, 0.9)], Stroke::new(2.0, col)));
        }
        Glyph::Forward => {
            p.add(egui::epaint::PathShape::line(vec![at(0.34, 0.1), at(0.72, 0.5), at(0.34, 0.9)], Stroke::new(2.0, col)));
        }
        Glyph::ZoomIn | Glyph::ZoomOut => {
            p.circle_stroke(at(0.42, 0.42), r.width() * 0.34, s);
            p.line_segment([at(0.68, 0.68), at(0.98, 0.98)], Stroke::new(2.2, col));
            p.line_segment([at(0.26, 0.42), at(0.58, 0.42)], s);
            if g == Glyph::ZoomIn {
                p.line_segment([at(0.42, 0.26), at(0.42, 0.58)], s);
            }
        }
        Glyph::TurnLeft | Glyph::TurnRight => {
            let right = g == Glyph::TurnRight;
            let m = |x: f32| if right { 1.0 - x } else { x };
            let pts: Vec<Pos2> = arc(0.5, 0.56, 0.38, -PI * 0.55, PI * 0.95).into_iter().map(|q| Pos2::new(r.min.x + r.width() * m((q.x - r.min.x) / r.width()), q.y)).collect();
            let head = pts[0];
            p.add(egui::epaint::PathShape::line(pts, s));
            let d = Vec2::new(r.width() * 0.2 * if right { -1.0 } else { 1.0 }, 0.0);
            p.add(egui::epaint::PathShape::line(vec![head + d + Vec2::new(0.0, -r.height() * 0.16), head, head + d * 0.2 + Vec2::new(0.0, r.height() * 0.2)], s));
        }
        Glyph::Flip => {
            p.add(egui::epaint::PathShape::convex_polygon(vec![at(0.38, 0.14), at(0.38, 0.86), at(0.02, 0.86)], col, Stroke::NONE));
            p.add(egui::epaint::PathShape::closed_line(vec![at(0.62, 0.14), at(0.98, 0.86), at(0.62, 0.86)], s));
            for k in 0..4 {
                let y = k as f32 * 0.27;
                p.line_segment([at(0.5, y), at(0.5, y + 0.14)], s);
            }
        }
        Glyph::Play => {
            p.rect_stroke(Rect::from_min_max(at(0.0, 0.1), at(1.0, 0.9)), 2.0, s, egui::StrokeKind::Middle);
            p.add(egui::epaint::PathShape::convex_polygon(vec![at(0.4, 0.32), at(0.68, 0.5), at(0.4, 0.68)], col, Stroke::NONE));
        }
        Glyph::Details => {
            p.circle_stroke(r.center(), r.width() * 0.48, s);
            p.circle_filled(at(0.5, 0.28), 1.4, col);
            p.line_segment([at(0.5, 0.44), at(0.5, 0.76)], Stroke::new(2.0, col));
        }
        Glyph::Open => {
            p.rect_stroke(Rect::from_min_max(at(0.0, 0.08), at(1.0, 0.92)), 2.0, s, egui::StrokeKind::Middle);
            p.circle_filled(at(0.7, 0.32), r.width() * 0.09, col);
            p.add(egui::epaint::PathShape::line(vec![at(0.04, 0.82), at(0.36, 0.46), at(0.58, 0.7), at(0.72, 0.58), at(0.96, 0.82)], s));
        }
    }
}

impl eframe::App for ViewApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_VIEW", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        self.look.frame(&ctx, &c, self.shot.is_none());
        self.poll(&ctx);
        self.loader.poll(&ctx);
        self.thumbs.poll(&ctx);
        if let Some(p) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty())) {
            self.open(&ctx, p);
        }
        // Read the picture shown first, then those either side, the way the next step goes.
        let max_side = ctx.input(|i| i.max_texture_side).min(16384) as u32;
        let n = self.items.len();
        let order = [0isize, 1, -1, 2, -2];
        let wanted: Vec<PathBuf> = order.iter().filter_map(|d| usize::try_from(self.idx as isize + d).ok()).filter(|&i| i < n).map(|i| self.items[i].path.clone()).collect();
        let refs: Vec<&Path> = wanted.iter().map(|p| p.as_path()).collect();
        self.loader.want(&refs, max_side);
        self.shortcuts(&ctx);
        // The slideshow moves on, round to the start at the end.
        let now = ctx.input(|i| i.time);
        if let Some(sl) = &mut self.slideshow
            && !sl.paused
            && self.shot.is_none()
        {
            if now >= sl.next_at && n > 0 {
                sl.next_at = now + self.s.slideshow_seconds as f64;
                self.stage.reset();
                self.idx = (self.idx + 1) % n;
                self.anim_start = now;
            }
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((sl.next_at - now).max(0.05)));
        }
        let title = self.current().map(|i| format!("{} – Rivet View", i.name)).unwrap_or_else(|| "Rivet View".into());
        if title != self.title && self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        }
        self.title = title;

        let chrome = self.slideshow.is_none() && !self.fullscreen;
        let v = ctx.global_style().visuals.clone();
        if chrome {
            let bar = |top: i8, bottom: i8| egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top, bottom });
            egui::Panel::top("view-top").show_separator_line(false).frame(bar(8, 6)).show(ui, |ui| self.top_bar(ui));
            if self.s.strip && self.items.len() > 1 {
                egui::Panel::bottom("view-strip").show_separator_line(false).frame(bar(8, 10)).show(ui, |ui| self.strip(ui));
            }
            if self.s.details && self.current().is_some() {
                egui::Panel::right("view-details").show_separator_line(false).resizable(false).exact_size(300.0).frame(bar(0, 8)).show(ui, |ui| {
                    egui::ScrollArea::vertical().id_salt("details").show(ui, |ui| self.details(ui));
                });
            }
        }
        let middle = egui::Frame::new().fill(v.panel_fill).inner_margin(if chrome { egui::Margin { left: 8, right: 8, top: 0, bottom: 0 } } else { egui::Margin::ZERO });
        egui::CentralPanel::default().frame(middle).show(ui, |ui| self.picture(ui, chrome));
        self.ask_ui(&ctx);
        if self.crash.as_mut().is_some_and(|n| n.show(&ctx)) {
            self.crash = None;
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}
