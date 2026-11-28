//! Rivet Snip's window: the picture in the middle, the tools above it, and the colour and
//! thickness below. Marks are drawn with the pointer; the finished picture is made off this
//! thread and goes to the clipboard or a file.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align, Align2, Color32, Key, Layout, Modifiers, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rivet_look::a11y::{self, Access, Role};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::theme::{Mode, r_lg, r_md, r_sm, status};
use rivet_look::{fonts, palette};

use crate::capture;
use crate::doc::{Doc, Kind, Mark, held};
use crate::paint::{self, Patches, View};
use crate::settings::{self, Settings};

pub const ICON_SVG: &str = include_str!("../data/rivet-snip.svg");

/// The colours to draw with. They are the picture's, not the theme's: a mark looks the same
/// on every desktop.
const COLORS: [(Color32, &str); 8] = [
    (Color32::from_rgb(0xe5, 0x48, 0x4d), "Red"),
    (Color32::from_rgb(0xf7, 0x6b, 0x15), "Orange"),
    (Color32::from_rgb(0xff, 0xd6, 0x0a), "Yellow"),
    (Color32::from_rgb(0x30, 0xa4, 0x6c), "Green"),
    (Color32::from_rgb(0x3e, 0x8e, 0xf7), "Blue"),
    (Color32::from_rgb(0xd5, 0x3f, 0x8c), "Pink"),
    (Color32::from_rgb(0xff, 0xff, 0xff), "White"),
    (Color32::from_rgb(0x16, 0x16, 0x1a), "Black"),
];

/// Line thicknesses, in pixels of a picture about 1600 across (bigger pictures scale them up).
const WIDTHS: [(f32, &str); 3] = [(3.0, "Thin"), (5.0, "Medium"), (9.0, "Thick")];

/// What the picture is opened with.
#[derive(Default)]
pub struct Start {
    /// The picture, or why there isn't one. None: open with nothing yet.
    pub picture: Option<Result<egui::ColorImage, String>>,
    /// The file it came from.
    pub source: Option<PathBuf>,
    /// Save writes over `source`.
    pub replace: bool,
    /// Start with the crop tool (a picture of the whole screen).
    pub crop_first: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tool {
    Select,
    Crop,
    Draw(Kind),
}

impl Tool {
    const ALL: [Tool; 11] = [
        Tool::Select,
        Tool::Crop,
        Tool::Draw(Kind::Arrow),
        Tool::Draw(Kind::Line),
        Tool::Draw(Kind::Rect),
        Tool::Draw(Kind::Ellipse),
        Tool::Draw(Kind::Pen),
        Tool::Draw(Kind::Marker),
        Tool::Draw(Kind::Text),
        Tool::Draw(Kind::Step),
        Tool::Draw(Kind::Blur),
    ];

    fn name(self) -> &'static str {
        match self {
            Tool::Select => "Select",
            Tool::Crop => "Crop",
            Tool::Draw(Kind::Arrow) => "Arrow",
            Tool::Draw(Kind::Line) => "Line",
            Tool::Draw(Kind::Rect) => "Box",
            Tool::Draw(Kind::Ellipse) => "Oval",
            Tool::Draw(Kind::Pen) => "Pen",
            Tool::Draw(Kind::Marker) => "Highlighter",
            Tool::Draw(Kind::Text) => "Text",
            Tool::Draw(Kind::Step) => "Number",
            Tool::Draw(Kind::Blur) => "Blur",
        }
    }

    fn key(self) -> Key {
        match self {
            Tool::Select => Key::V,
            Tool::Crop => Key::C,
            Tool::Draw(Kind::Arrow) => Key::A,
            Tool::Draw(Kind::Line) => Key::L,
            Tool::Draw(Kind::Rect) => Key::R,
            Tool::Draw(Kind::Ellipse) => Key::O,
            Tool::Draw(Kind::Pen) => Key::P,
            Tool::Draw(Kind::Marker) => Key::H,
            Tool::Draw(Kind::Text) => Key::T,
            Tool::Draw(Kind::Step) => Key::N,
            Tool::Draw(Kind::Blur) => Key::B,
        }
    }

    /// What it does, for its tooltip.
    fn tip(self) -> &'static str {
        match self {
            Tool::Select => "Move a mark, or remove it with Delete.",
            Tool::Crop => "Drag over the part to keep.",
            Tool::Draw(Kind::Blur) => "Drag over what shouldn't be read.",
            Tool::Draw(Kind::Text) => "Click, type, then press Enter.",
            Tool::Draw(Kind::Step) => "Each click places the next number.",
            Tool::Draw(Kind::Pen | Kind::Marker) => "",
            Tool::Draw(_) => "Hold Shift to keep it straight or square.",
        }
    }

    fn from_name(s: &str) -> Option<Tool> {
        Tool::ALL.into_iter().find(|t| t.name().eq_ignore_ascii_case(s))
    }
}

/// What the pointer is in the middle of.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    None,
    /// Drawing the last mark.
    Draw,
    /// Moving mark `i`; `moved` once it has gone anywhere (the change is recorded then).
    Move { i: usize, last: Pos2, moved: bool },
    Crop { from: Pos2, to: Pos2 },
    Pan,
}

/// A question that must be answered before going on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prompt {
    Close,
    Open,
}

/// Work done off the frame, and what it came back with.
enum Done {
    /// The picture is on the clipboard (or, when `wl-copy` couldn't take it, is here for the
    /// window's own clipboard).
    Copied { image: Option<egui::ColorImage>, edits: u64 },
    Saved { file: PathBuf, edits: u64 },
    Opened { image: egui::ColorImage, file: PathBuf },
    /// Nothing was chosen in the file picker.
    Cancelled,
    Failed(String),
}

pub struct SnipApp {
    s: Settings,
    look: LiveLook,
    shot: Option<rivet_kit::Shot>,
    base: Option<Arc<egui::ColorImage>>,
    tex: Option<egui::TextureHandle>,
    doc: Option<Doc>,
    source: Option<PathBuf>,
    replace: bool,
    /// Where Save last wrote: the next Save writes there again.
    saved_to: Option<PathBuf>,
    /// The picture as last copied or saved (`Doc::edits`): the same number means nothing is unsaved.
    kept: u64,
    tool: Tool,
    drag: Drag,
    selected: Option<usize>,
    /// The text mark being typed into, and whether it was made just now.
    typing: Option<(usize, bool)>,
    /// None: the picture fits the window.
    zoom: Option<f32>,
    pan: Vec2,
    patches: Patches,
    prompt: Option<Prompt>,
    /// A line at the bottom: what was just done, or what went wrong, and when it was said.
    note: Option<(String, bool, f64)>,
    /// Why there is no picture, on the empty window.
    trouble: Option<String>,
    busy: bool,
    tx: Sender<Done>,
    rx: Receiver<Done>,
    allow_close: bool,
    logo: Option<egui::TextureHandle>,
    crash: Option<Box<rivet_look::crash::Notice>>,
}

impl SnipApp {
    pub fn new(cc: &eframe::CreationContext, mut start: Start, screenshot: Option<PathBuf>) -> SnipApp {
        let mut s = Settings::load();
        let checking = screenshot.is_some();
        if checking {
            settings::NO_SAVE.store(true, Ordering::Relaxed);
            s = Settings { theme: s.theme, accent: s.accent, follow_desktop_colors: s.follow_desktop_colors, ..Settings::default() };
            // Checks draw a made-up desktop, never the person's own screen.
            // Test hook: RIVET_SNIP_EMPTY shows the window with no picture.
            if std::env::var_os("RIVET_SNIP_EMPTY").is_none() && start.picture.is_none() {
                start.picture = Some(Ok(demo::picture()));
            }
        }
        match std::env::var("RIVET_SNIP_THEME").as_deref() {
            Ok("light") => (s.theme, s.follow_desktop_colors) = (Mode::Light, false),
            Ok("dark") => (s.theme, s.follow_desktop_colors) = (Mode::Dark, false),
            _ => {}
        }
        let (tx, rx) = channel();
        let mut app = SnipApp {
            s,
            look: LiveLook::new(),
            shot: screenshot.map(rivet_kit::Shot::new),
            base: None,
            tex: None,
            doc: None,
            source: None,
            replace: start.replace,
            saved_to: None,
            kept: 0,
            tool: if start.crop_first { Tool::Crop } else { Tool::Draw(Kind::Arrow) },
            drag: Drag::None,
            selected: None,
            typing: None,
            zoom: None,
            pan: Vec2::ZERO,
            patches: Patches::default(),
            prompt: None,
            note: None,
            trouble: None,
            busy: false,
            tx,
            rx,
            allow_close: false,
            logo: None,
            crash: if checking { None } else { rivet_look::crash::Notice::unseen(crate::CRASH).map(Box::new) },
        };
        match start.picture {
            Some(Ok(img)) => app.set_picture(&cc.egui_ctx, img, start.source),
            Some(Err(e)) => app.trouble = Some(e),
            None => {}
        }
        let choice = app.choice();
        if checking && let Some(doc) = app.doc.as_mut() {
            // Test hook: RIVET_SNIP_PLAIN leaves the sample picture without its sample marks.
            if std::env::var_os("RIVET_SNIP_PLAIN").is_none() {
                doc.marks = demo::marks();
            }
            // Test hook: RIVET_SNIP_CROP=X0,Y0,X1,Y1 keeps that part of the sample picture.
            if let Some(v) = std::env::var("RIVET_SNIP_CROP").ok().and_then(|v| v.split(',').map(|n| n.trim().parse::<f32>().ok()).collect::<Option<Vec<f32>>>()).filter(|v| v.len() == 4) {
                doc.set_crop(Rect::from_min_max(Pos2::new(v[0], v[1]), Pos2::new(v[2], v[3])));
            }
            // Test hook: RIVET_SNIP_TOOL=NAME picks a tool, RIVET_SNIP_SELECT=N selects mark N,
            // RIVET_SNIP_PROMPT shows the question asked before closing.
            if let Some(t) = std::env::var("RIVET_SNIP_TOOL").ok().and_then(|t| Tool::from_name(&t)) {
                app.tool = t;
            }
            app.selected = std::env::var("RIVET_SNIP_SELECT").ok().and_then(|v| v.parse().ok()).filter(|i| *i < doc.marks.len());
            if app.selected.is_some() {
                app.tool = Tool::Select;
            }
            if std::env::var_os("RIVET_SNIP_PROMPT").is_some() {
                app.prompt = Some(Prompt::Close);
            }
            // Test hook: RIVET_SNIP_EXPORT=FILE also writes the finished picture there, as
            // Save would.
            if let (Some(file), Some(base)) = (std::env::var_os("RIVET_SNIP_EXPORT"), app.base.clone()) {
                let out = crate::export::render(&base, doc, &choice);
                if let Err(e) = rivet_kit::save_png(Path::new(&file), &out) {
                    eprintln!("rivet-snip: {e}");
                }
            }
        }
        app
    }

    fn choice(&self) -> Choice {
        Choice { mode: self.s.theme, accent: self.s.accent, follow_desktop_colors: self.s.follow_desktop_colors, text_size: self.s.text_size }
    }

    fn accent(&self) -> Color32 {
        self.look.accent(&self.choice())
    }

    fn say(&mut self, ctx: &egui::Context, text: impl Into<String>, bad: bool) {
        self.note = Some((text.into(), bad, ctx.input(|i| i.time)));
    }

    /// Start on a picture: the one handed over, opened, pasted or dropped in.
    fn set_picture(&mut self, ctx: &egui::Context, img: egui::ColorImage, source: Option<PathBuf>) {
        let max = ctx.input(|i| i.max_texture_side);
        if img.size[0] > max || img.size[1] > max {
            self.trouble = Some(format!("The picture is {} × {} pixels, and this computer can show at most {max} each way.", img.size[0], img.size[1]));
            return;
        }
        self.doc = Some(Doc::new(img.size));
        self.tex = Some(ctx.load_texture("rivet-snip-picture", img.clone(), egui::TextureOptions::LINEAR));
        self.base = Some(Arc::new(img));
        if source.is_none() {
            self.replace = false;
        }
        self.source = source;
        (self.saved_to, self.kept, self.selected, self.typing, self.zoom, self.pan, self.drag, self.trouble) = (None, 0, None, None, None, Vec2::ZERO, Drag::None, None);
        self.patches = Patches::default();
    }

    /// Marks made on a big picture are drawn thicker, so they look the same once it is
    /// scaled down to be looked at.
    fn scale(&self) -> f32 {
        self.doc.as_ref().map(|d| (d.size[0].max(d.size[1]) as f32 / 1600.0).clamp(1.0, 4.0)).unwrap_or(1.0)
    }

    /// Marks have been made since the picture was last copied or saved.
    fn unsaved(&self) -> bool {
        self.doc.as_ref().is_some_and(|d| d.edits != self.kept)
    }

    // ---------------------------------------------------------------- copy, save, open

    fn copy(&mut self) {
        self.stop_typing();
        let (Some(base), Some(doc), false) = (self.base.clone(), self.doc.clone(), self.busy) else { return };
        if self.shot.is_some() {
            return;
        }
        self.busy = true;
        let (tx, choice) = (self.tx.clone(), self.choice());
        std::thread::spawn(move || {
            let out = crate::export::render(&base, &doc, &choice);
            let done = match rivet_kit::png_bytes(&out) {
                Ok(png) if capture::copy_png(&png) => Done::Copied { image: None, edits: doc.edits },
                _ => Done::Copied { image: Some(out), edits: doc.edits },
            };
            let _ = tx.send(done);
        });
    }

    /// Where Save writes when no file has been chosen.
    fn save_target(&self) -> PathBuf {
        if let Some(f) = &self.saved_to {
            return f.clone();
        }
        match &self.source {
            Some(f) if self.replace => f.clone(),
            // The original is left as it was: the marked-up picture goes beside it.
            Some(f) => {
                let stem = f.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "Picture".into());
                capture::free(f.with_file_name(format!("{stem} marked.png")))
            }
            None => capture::free(capture::folder(&self.s.save_dir).join(capture::name_now())),
        }
    }

    /// Save to `file`, or ask where when it is None.
    fn save(&mut self, file: Option<PathBuf>, ask: bool) {
        self.stop_typing();
        let (Some(base), Some(doc), false) = (self.base.clone(), self.doc.clone(), self.busy) else { return };
        if self.shot.is_some() {
            return;
        }
        self.busy = true;
        let target = file.unwrap_or_else(|| self.save_target());
        let (tx, choice) = (self.tx.clone(), self.choice());
        std::thread::spawn(move || {
            let target = if ask {
                let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(capture::name_now);
                match rivet_xdg::portal::save_picture("Save Picture", &name, target.parent()) {
                    Ok(Some(f)) if f.extension().is_some_and(|e| e.eq_ignore_ascii_case("png")) => f,
                    // The file is a PNG whatever it was called, so it says so.
                    Ok(Some(f)) => PathBuf::from(format!("{}.png", f.display())),
                    Ok(None) => {
                        let _ = tx.send(Done::Cancelled);
                        return;
                    }
                    Err(e) => {
                        eprintln!("rivet-snip: {e}");
                        let _ = tx.send(Done::Failed("The desktop's file picker isn't running. Save still works: it chooses the place itself.".into()));
                        return;
                    }
                }
            } else {
                target
            };
            let out = crate::export::render(&base, &doc, &choice);
            if let Some(d) = target.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            let done = match rivet_kit::png_bytes(&out).and_then(|png| std::fs::write(&target, png).map_err(|e| e.to_string())) {
                Ok(()) => Done::Saved { file: target, edits: doc.edits },
                Err(e) => Done::Failed(format!("Couldn't save {}: {e}.", target.display())),
            };
            let _ = tx.send(done);
        });
    }

    fn open(&mut self) {
        if self.busy || self.shot.is_some() {
            return;
        }
        if self.unsaved() && self.prompt != Some(Prompt::Open) {
            self.prompt = Some(Prompt::Open);
            return;
        }
        self.prompt = None;
        self.busy = true;
        let tx = self.tx.clone();
        let from = self.source.as_ref().and_then(|f| f.parent().map(Path::to_path_buf)).unwrap_or_else(|| capture::folder(&self.s.save_dir));
        std::thread::spawn(move || {
            let done = match rivet_xdg::portal::choose(rivet_xdg::portal::Pick::Picture, "Open Picture", Some(&from)) {
                Ok(Some(file)) => match capture::open(&file) {
                    Ok(image) => Done::Opened { image, file },
                    Err(e) => Done::Failed(format!("{e}.")),
                },
                Ok(None) => Done::Cancelled,
                Err(e) => {
                    eprintln!("rivet-snip: {e}");
                    Done::Failed("The desktop's file picker isn't running. Drop a picture on this window instead.".into())
                }
            };
            let _ = tx.send(done);
        });
    }

    /// Take a new screenshot: this window closes first so it isn't in the picture, and a new
    /// Rivet Snip opens with what was taken.
    fn take_another(&mut self, ctx: &egui::Context) {
        if self.shot.is_some() {
            return;
        }
        let Ok(exe) = std::env::current_exe() else { return };
        match std::process::Command::new(exe).args(["capture", "--delay", "0.6"]).stdin(std::process::Stdio::null()).spawn() {
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            Err(e) => self.say(ctx, format!("Couldn't take a screenshot: {e}."), true),
        }
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(done) = self.rx.try_recv() {
            self.busy = false;
            match done {
                Done::Copied { image, edits } => {
                    if let Some(img) = image {
                        ctx.copy_image(img);
                    }
                    self.kept = edits;
                    self.say(ctx, "Copied to the clipboard", false);
                    if self.s.close_after_copy {
                        self.allow_close = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
                Done::Saved { file, edits } => {
                    self.kept = edits;
                    self.say(ctx, format!("Saved as {}", tidy(&file)), false);
                    self.saved_to = Some(file);
                }
                Done::Opened { image, file } => self.set_picture(ctx, image, Some(file)),
                Done::Cancelled => {}
                Done::Failed(e) => self.say(ctx, e, true),
            }
        }
    }

    // ---------------------------------------------------------------- editing

    fn stop_typing(&mut self) {
        let Some((i, new)) = self.typing.take() else { return };
        let Some(doc) = self.doc.as_mut() else { return };
        if doc.marks.get(i).is_some_and(|m| m.text.trim().is_empty()) {
            if new {
                // Nothing was typed: as if the click never happened.
                doc.forget();
            } else {
                doc.marks.remove(i);
            }
            self.selected = None;
        }
    }

    fn remove_selected(&mut self) {
        if let (Some(i), Some(doc)) = (self.selected.take(), self.doc.as_mut())
            && i < doc.marks.len()
        {
            doc.checkpoint();
            doc.marks.remove(i);
        }
    }

    fn undo(&mut self, redo: bool) {
        self.stop_typing();
        if let Some(doc) = self.doc.as_mut() {
            if redo { doc.redo() } else { doc.undo() }
            self.selected = None;
            (self.zoom, self.pan) = (None, Vec2::ZERO);
        }
    }

    fn reset_crop(&mut self) {
        if let Some(doc) = self.doc.as_mut()
            && doc.set_crop(doc.full())
        {
            (self.zoom, self.pan) = (None, Vec2::ZERO);
        }
    }

    /// The keys typed into a text mark. They are taken, so no shortcut sees them.
    fn type_text(&mut self, ctx: &egui::Context) {
        let Some((i, _)) = self.typing else { return };
        let mut stop = false;
        let events = ctx.input_mut(|inp| std::mem::take(&mut inp.events));
        if let Some(m) = self.doc.as_mut().and_then(|d| d.marks.get_mut(i)) {
            for e in events {
                match e {
                    egui::Event::Text(t) | egui::Event::Paste(t) => m.text.push_str(&t.replace('\r', "")),
                    egui::Event::Key { key: Key::Enter, pressed: true, modifiers, .. } if modifiers.shift => m.text.push('\n'),
                    egui::Event::Key { key: Key::Enter | Key::Escape, pressed: true, .. } => stop = true,
                    egui::Event::Key { key: Key::Backspace, pressed: true, .. } => {
                        m.text.pop();
                    }
                    // The pointer still reaches the picture: a click elsewhere ends the typing.
                    e @ (egui::Event::PointerMoved(_) | egui::Event::PointerButton { .. } | egui::Event::MouseWheel { .. } | egui::Event::PointerGone | egui::Event::MouseMoved(_) | egui::Event::Zoom(_)) => ctx.input_mut(|inp| inp.events.push(e)),
                    _ => {}
                }
            }
        }
        if stop {
            self.stop_typing();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.typing.is_some() || self.prompt.is_some() || self.crash.is_some() {
            return;
        }
        let shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let take = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if take(shift, Key::Z) || take(Modifiers::COMMAND, Key::Y) {
            self.undo(true);
        } else if take(Modifiers::COMMAND, Key::Z) {
            self.undo(false);
        }
        if take(shift, Key::S) {
            self.save(None, true);
        } else if take(Modifiers::COMMAND, Key::S) {
            self.save(None, false);
        }
        // Ctrl+C reaches egui as a Copy event, not a key.
        if ctx.input_mut(|i| {
            let n = i.events.len();
            i.events.retain(|e| !matches!(e, egui::Event::Copy));
            n != i.events.len()
        }) {
            self.copy();
        }
        if take(Modifiers::COMMAND, Key::O) {
            self.open();
        }
        if take(Modifiers::COMMAND, Key::Num0) {
            (self.zoom, self.pan) = (None, Vec2::ZERO);
        }
        if take(Modifiers::COMMAND, Key::Num1) {
            (self.zoom, self.pan) = (Some(1.0), Vec2::ZERO);
        }
        if take(Modifiers::COMMAND, Key::Q) || take(Modifiers::COMMAND, Key::W) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if take(Modifiers::NONE, Key::Delete) || take(Modifiers::NONE, Key::Backspace) {
            self.remove_selected();
        }
        if take(Modifiers::NONE, Key::Escape) {
            self.selected = None;
            if matches!(self.drag, Drag::Crop { .. }) {
                self.drag = Drag::None;
            }
        }
        for t in Tool::ALL {
            if take(Modifiers::NONE, t.key()) {
                self.tool = t;
                if t != Tool::Select {
                    self.selected = None;
                }
            }
        }
        // A picture pasted or dropped in is opened, unless that would lose marks never copied
        // or saved.
        if !self.unsaved() {
            let pasted = ctx.input_mut(|i| {
                let at = i.events.iter().position(|e| matches!(e, egui::Event::PasteImage(_)))?;
                match i.events.remove(at) {
                    egui::Event::PasteImage(img) => Some(img),
                    _ => None,
                }
            });
            if let Some(img) = pasted {
                self.set_picture(ctx, (*img).clone(), None);
            }
            if let Some(file) = ctx.input(|i| i.raw.dropped_files.first().map(|f| f.path().to_path_buf())) {
                match capture::open(&file) {
                    Ok(img) => self.set_picture(ctx, img, Some(file)),
                    Err(e) if self.doc.is_none() => self.trouble = Some(format!("{e}.")),
                    Err(e) => self.say(ctx, format!("{e}."), true),
                }
            }
        }
    }

    // ---------------------------------------------------------------- the bars

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let accent = self.accent();
        let has = self.doc.is_some();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            for t in Tool::ALL {
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
                let on = self.tool == t && has;
                let resp = resp.role(Role::Button, t.name()).toggled(on);
                let fill = if on {
                    palette::mix(v.panel_fill, accent, 0.28)
                } else if resp.hovered() && has {
                    palette::mix(v.panel_fill, v.text_color(), 0.10)
                } else {
                    Color32::TRANSPARENT
                };
                ui.painter().rect_filled(r, r_sm(), fill);
                let col = if !has {
                    v.weak_text_color().gamma_multiply(0.5)
                } else if on {
                    v.strong_text_color()
                } else {
                    v.text_color()
                };
                tool_glyph(ui.painter(), r.shrink(8.0), t, col);
                if resp.clicked() && has {
                    self.stop_typing();
                    self.tool = t;
                    if t != Tool::Select {
                        self.selected = None;
                    }
                }
                let key = t.key().name();
                resp.on_hover_ui(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(t.name()).strong());
                        ui.label(RichText::new(key).color(ui.visuals().weak_text_color()));
                    });
                    if !t.tip().is_empty() {
                        ui.label(t.tip());
                    }
                });
                if matches!(t, Tool::Crop | Tool::Draw(Kind::Marker)) {
                    ui.add_space(5.0);
                    let (line, _) = ui.allocate_exact_size(Vec2::new(1.0, 18.0), Sense::hover());
                    ui.painter().rect_filled(line, 0.0, palette::mix(v.panel_fill, v.text_color(), 0.18));
                    ui.add_space(5.0);
                }
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 6.0;
                ui.menu_button("More", |ui| self.more(ui));
                let busy = self.busy;
                if ui.add_enabled(has && !busy, egui::Button::new("Save").corner_radius(r_sm())).on_hover_text("Ctrl+S").clicked() {
                    self.save(None, false);
                }
                let copy = egui::Button::new(RichText::new("Copy").color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm());
                if ui.add_enabled(has && !busy, copy).on_hover_text("Ctrl+C").clicked() {
                    self.copy();
                }
                if busy {
                    ui.add(egui::Spinner::new().size(16.0));
                }
                ui.add_space(6.0);
                let (can_undo, can_redo) = self.doc.as_ref().map(|d| (d.can_undo(), d.can_redo())).unwrap_or_default();
                for (redo, can, tip) in [(true, can_redo, "Redo (Ctrl+Shift+Z)"), (false, can_undo, "Undo (Ctrl+Z)")] {
                    let (r, resp) = ui.allocate_exact_size(Vec2::splat(30.0), if can { Sense::click() } else { Sense::hover() });
                    if resp.hovered() && can {
                        ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.text_color(), 0.10));
                    }
                    turn_glyph(ui.painter(), r.shrink(8.0), redo, if can { v.text_color() } else { v.weak_text_color().gamma_multiply(0.5) });
                    if resp.role(Role::Button, if redo { "Redo" } else { "Undo" }).tip(tip).clicked() {
                        self.undo(redo);
                    }
                }
            });
        });
    }

    fn more(&mut self, ui: &mut egui::Ui) {
        ui.set_min_width(240.0);
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
        let has = self.doc.is_some();
        let ctx = ui.ctx().clone();
        if item(ui, "Take a Screenshot", "", !self.unsaved()) {
            self.take_another(&ctx);
        }
        if item(ui, "Open…", "Ctrl+O", true) {
            self.open();
        }
        if item(ui, "Save As…", "Ctrl+Shift+S", has) {
            self.save(None, true);
        }
        ui.separator();
        if item(ui, "Undo Crop", "", self.doc.as_ref().is_some_and(|d| d.is_cropped())) {
            self.reset_crop();
        }
        if item(ui, "Zoom to Fit", "Ctrl+0", has) {
            (self.zoom, self.pan) = (None, Vec2::ZERO);
        }
        if item(ui, "Actual Size", "Ctrl+1", has) {
            (self.zoom, self.pan) = (Some(1.0), Vec2::ZERO);
        }
        ui.separator();
        if ui.checkbox(&mut self.s.close_after_copy, "Close after copying").changed() {
            self.s.save();
        }
        ui.separator();
        ui.label(RichText::new(format!("Rivet Snip {}", env!("CARGO_PKG_VERSION"))).small().color(ui.visuals().weak_text_color()));
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let now = ui.input(|i| i.time);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.scope_builder(a11y::area(Role::RadioGroup, "Color"), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, (c, name)) in COLORS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(22.0), Sense::click());
                let on = self.s.color == i;
                let resp = resp.role(Role::RadioButton, *name).selected(on);
                if on {
                    ui.painter().circle_stroke(r.center(), 10.0, Stroke::new(1.6, v.strong_text_color()));
                }
                ui.painter().circle_filled(r.center(), if on { 6.5 } else { 7.5 }, *c);
                // White and black need an edge to show against a light or dark window.
                ui.painter().circle_stroke(r.center(), if on { 6.5 } else { 7.5 }, Stroke::new(1.0, palette::mix(*c, v.text_color(), 0.25)));
                if resp.tip(*name).clicked() {
                    self.s.color = i;
                    self.s.save();
                    // The colour of a selected mark changes with it.
                    if let (Some(sel), Some(doc)) = (self.selected, self.doc.as_mut())
                        && doc.marks.get(sel).is_some_and(|m| m.color != *c && m.kind != Kind::Blur)
                    {
                        doc.checkpoint();
                        doc.marks[sel].color = *c;
                    }
                }
            }
            });
            ui.add_space(10.0);
            let scale = self.scale();
            ui.scope_builder(a11y::area(Role::RadioGroup, "Line thickness"), |ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            for (i, (w, name)) in WIDTHS.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(28.0, 22.0), Sense::click());
                let on = self.s.width == i;
                let resp = resp.role(Role::RadioButton, *name).selected(on);
                if on {
                    ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.text_color(), 0.14));
                }
                ui.painter().line_segment([r.left_center() + Vec2::new(6.0, 0.0), r.right_center() - Vec2::new(6.0, 0.0)], Stroke::new(w * 0.6, if on { v.strong_text_color() } else { v.text_color() }));
                if resp.tip(*name).clicked() {
                    self.s.width = i;
                    self.s.save();
                    if let (Some(sel), Some(doc)) = (self.selected, self.doc.as_mut())
                        && doc.marks.get(sel).is_some_and(|m| m.width != w * scale && m.kind != Kind::Blur)
                    {
                        doc.checkpoint();
                        doc.marks[sel].width = w * scale;
                    }
                }
            }
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(d) = &self.doc {
                    let zoom = self.zoom.map(|z| format!("{:.0}%", z * 100.0)).unwrap_or_else(|| "Fit".into());
                    ui.label(RichText::new(format!("{} × {}  ·  {zoom}", d.crop.width() as usize, d.crop.height() as usize)).font(fonts::ui_font(12.0)).color(v.weak_text_color()));
                    ui.add_space(10.0);
                }
                if let Some((text, bad, at)) = &self.note {
                    if now - at < 6.0 || *bad {
                        let color = if *bad { status(ui.ctx()).danger } else { v.text_color() };
                        ui.add(egui::Label::new(RichText::new(text).font(fonts::ui_font(12.5)).color(color)).truncate());
                    }
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                }
            });
        });
    }

    // ---------------------------------------------------------------- the picture

    fn empty(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let ctx = ui.ctx().clone();
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.5 - 130.0).max(12.0));
            let px = 84.0;
            if self.logo.is_none() {
                self.logo = rivet_look::icons::render_svg(ICON_SVG.as_bytes(), (px * ctx.pixels_per_point()) as u32).map(|img| ctx.load_texture("rivet-snip-logo", img, egui::TextureOptions::LINEAR));
            }
            if let Some(t) = &self.logo {
                ui.add(egui::Image::new(t).fit_to_exact_size(Vec2::splat(px)));
            }
            ui.add_space(12.0);
            ui.label(RichText::new("No picture yet").font(fonts::ui_font(19.0)).strong().color(v.strong_text_color()));
            ui.add_space(4.0);
            match &self.trouble {
                Some(e) => ui.label(RichText::new(e).color(status(&ctx).danger)),
                None => ui.label(RichText::new("Take a screenshot, open a picture, or paste one with Ctrl+V.").color(v.weak_text_color())),
            };
            ui.add_space(16.0);
            // The two buttons, side by side in the middle.
            let w = 300.0;
            ui.allocate_ui_with_layout(Vec2::new(w, 34.0), Layout::left_to_right(Align::Center), |ui| {
                ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.add_space(((w - 250.0) / 2.0).max(0.0));
                if ui.add(egui::Button::new(RichText::new("Take a Screenshot").color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked() {
                    self.take_another(&ctx);
                }
                if ui.add(egui::Button::new("Open…").corner_radius(r_sm())).clicked() {
                    self.open();
                }
            });
        });
    }

    fn canvas(&mut self, ui: &mut egui::Ui) {
        let (Some(base), Some(tex)) = (self.base.clone(), self.tex.clone()) else {
            self.empty(ui);
            return;
        };
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let accent = self.accent();
        let area = ui.available_rect_before_wrap();
        let resp = ui.allocate_rect(area, Sense::CLICK | Sense::DRAG).role(Role::Canvas, "Picture");
        let Some(doc) = self.doc.as_mut() else { return };
        let crop = doc.crop;
        let margin = 18.0;
        // A small picture is shown at its own size, not blown up to fill the window.
        let fit = ((area.width() - margin * 2.0) / crop.width()).min((area.height() - margin * 2.0) / crop.height()).clamp(0.02, 1.0);
        let mut zoom = self.zoom.unwrap_or(fit);
        let pointer = ctx.input(|i| i.pointer.hover_pos()).filter(|p| area.contains(*p));

        // Zooming keeps the point under the pointer where it is.
        let (pinch, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if let (Some(at), true) = (pointer, pinch != 1.0) {
            let old = View { origin: area.center() - crop.size() * zoom / 2.0 + self.pan - crop.min.to_vec2() * zoom, zoom };
            let q = old.to_picture(at) - crop.min;
            zoom = (zoom * pinch).clamp(0.05, 16.0);
            self.pan = at - area.center() + crop.size() * zoom / 2.0 - q * zoom;
            self.zoom = Some(zoom);
        } else if pointer.is_some() && scroll != Vec2::ZERO {
            self.pan += scroll;
        }
        // The picture can be pushed around only as far as it overflows the window.
        let shown = crop.size() * zoom;
        let slack = ((shown - area.size() + Vec2::splat(margin * 2.0)) / 2.0).max(Vec2::ZERO);
        self.pan = self.pan.clamp(-slack, slack);
        let view = View { origin: area.center() - shown / 2.0 + self.pan - crop.min.to_vec2() * zoom, zoom };
        let frame = view.rect(crop);

        // ---- the pointer
        let at = pointer.map(|p| view.to_picture(p));
        let pressed = resp.hovered() && ctx.input(|i| i.pointer.primary_pressed());
        let origin = ctx.input(|i| i.pointer.press_origin()).map(|p| view.to_picture(p));
        let shift = ctx.input(|i| i.modifiers.shift);
        let slack_px = 6.0 / zoom;
        let sizes: Vec<Vec2> = doc.marks.iter().map(|m| if m.kind == Kind::Text { paint::text_size(&ctx, m) } else { Vec2::ZERO }).collect();
        let hit = |doc: &Doc, p: Pos2| (0..doc.marks.len()).rev().find(|&i| doc.marks[i].hit(p, slack_px, sizes[i]));
        let (color, width) = (COLORS[self.s.color.min(COLORS.len() - 1)].0, WIDTHS[self.s.width.min(WIDTHS.len() - 1)].0 * (doc.size[0].max(doc.size[1]) as f32 / 1600.0).clamp(1.0, 4.0));
        let mut end_typing = false;

        if resp.dragged_by(egui::PointerButton::Middle) {
            self.drag = Drag::Pan;
            self.pan += resp.drag_delta();
        } else if self.drag == Drag::Pan {
            self.drag = Drag::None;
        }

        if let (true, Some(p)) = (pressed, at) {
            // A press anywhere ends the typing in hand, then does what the tool does.
            end_typing = self.typing.is_some();
            match self.tool {
                Tool::Select => {
                    self.selected = hit(doc, p);
                    if let Some(i) = self.selected {
                        self.drag = Drag::Move { i, last: p, moved: false };
                    }
                }
                Tool::Crop => {}
                Tool::Draw(Kind::Text) => {
                    // On text that is there: go on typing in it.
                    let old = hit(doc, p).filter(|&i| doc.marks[i].kind == Kind::Text);
                    if end_typing && old.is_some_and(|i| self.typing.is_some_and(|(t, _)| t == i)) {
                        end_typing = false;
                    } else if let Some(i) = old {
                        doc.checkpoint();
                        self.typing = Some((i, false));
                        end_typing = false;
                        self.pending_stop(i);
                    } else if !end_typing {
                        doc.checkpoint();
                        // The click is the middle of the first line's height, where a caret would sit.
                        let mut m = Mark::new(Kind::Text, p, color, width);
                        m.a.y -= m.text_size() * 0.6;
                        doc.marks.push(m);
                        self.typing = Some((doc.marks.len() - 1, true));
                    }
                }
                Tool::Draw(Kind::Step) => {
                    doc.checkpoint();
                    let mut m = Mark::new(Kind::Step, p, color, width);
                    m.n = doc.next_step();
                    doc.marks.push(m);
                }
                Tool::Draw(_) => {}
            }
        }
        if end_typing {
            self.stop_typing();
        }
        let Some(doc) = self.doc.as_mut() else { return };

        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(from) = origin
        {
            match self.tool {
                Tool::Crop => self.drag = Drag::Crop { from, to: from },
                Tool::Draw(kind) if !matches!(kind, Kind::Text | Kind::Step) => {
                    doc.checkpoint();
                    doc.marks.push(Mark::new(kind, from, color, width));
                    self.drag = Drag::Draw;
                    self.selected = None;
                }
                _ => {}
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary)
            && let Some(p) = ctx.input(|i| i.pointer.interact_pos()).map(|p| view.to_picture(p))
        {
            match &mut self.drag {
                Drag::Draw => {
                    if let Some(m) = doc.marks.last_mut() {
                        if matches!(m.kind, Kind::Pen | Kind::Marker) {
                            // A point for each pixel moved is plenty, however fast the frames come.
                            if m.pts.last().is_none_or(|q| q.distance(p) >= 1.0) {
                                m.pts.push(p);
                            }
                        } else {
                            m.b = if shift { held(m.kind, m.a, p) } else { p };
                        }
                    }
                }
                Drag::Move { i, last, moved } => {
                    let d = p - *last;
                    if d != Vec2::ZERO && *i < doc.marks.len() {
                        if !*moved {
                            doc.checkpoint();
                            *moved = true;
                        }
                        doc.marks[*i].shift(d);
                        *last = p;
                    }
                }
                Drag::Crop { to, .. } => *to = p.clamp(crop.min, crop.max),
                _ => {}
            }
        }
        if resp.drag_stopped() {
            match self.drag {
                Drag::Draw => {
                    if doc.marks.last().is_some_and(|m| m.is_slip()) {
                        doc.forget();
                    }
                }
                Drag::Crop { from, to } if doc.set_crop(Rect::from_two_pos(from.clamp(crop.min, crop.max), to)) => {
                    (self.zoom, self.pan) = (None, Vec2::ZERO);
                    let (w, h) = (doc.crop.width() as usize, doc.crop.height() as usize);
                    self.note = Some((format!("Cropped to {w} × {h}"), false, ctx.input(|i| i.time)));
                }
                _ => {}
            }
            self.drag = Drag::None;
        }
        // Double-click text with the Select tool to change it.
        if self.tool == Tool::Select
            && resp.double_clicked()
            && let Some(i) = at.and_then(|p| hit(doc, p)).filter(|&i| doc.marks[i].kind == Kind::Text)
        {
            doc.checkpoint();
            self.typing = Some((i, false));
        }

        let icon = match self.tool {
            _ if self.drag == Drag::Pan => egui::CursorIcon::Grabbing,
            Tool::Select if at.is_some_and(|p| hit(doc, p).is_some()) => egui::CursorIcon::Move,
            Tool::Select => egui::CursorIcon::Default,
            Tool::Draw(Kind::Text) => egui::CursorIcon::Text,
            _ => egui::CursorIcon::Crosshair,
        };
        if resp.hovered() {
            ctx.set_cursor_icon(icon);
        }

        // ---- drawing
        let p = ui.painter_at(area);
        p.rect_filled(frame.expand(1.0), 0.0, palette::mix(v.panel_fill, v.text_color(), 0.22));
        let full = doc.full();
        let uv = Rect::from_min_max((crop.min.to_vec2() / full.size()).to_pos2(), (crop.max.to_vec2() / full.size()).to_pos2());
        p.image(tex.id(), frame, uv, Color32::WHITE);
        let inside = ui.painter_at(area.intersect(frame));
        self.patches.keep(&doc.marks, &base);
        for m in &doc.marks {
            paint::mark(&inside, &view, m, &base, &mut self.patches);
        }
        if let Some(i) = self.selected.filter(|i| *i < doc.marks.len()) {
            let size = if doc.marks[i].kind == Kind::Text { paint::text_size(&ctx, &doc.marks[i]) } else { Vec2::ZERO };
            let r = view.rect(doc.marks[i].bounds(size)).expand(4.0);
            p.rect_stroke(r, r_sm(), Stroke::new(1.5, accent), egui::StrokeKind::Outside);
        }
        if let Some((i, _)) = self.typing
            && let Some(m) = doc.marks.get(i)
        {
            let galley = p.layout_no_wrap(m.text.clone(), egui::FontId::proportional(m.text_size() * zoom), m.color);
            let caret = galley.pos_from_cursor(galley.end()).translate(view.to_screen(m.a).to_vec2());
            let r = Rect::from_min_size(view.to_screen(m.a), galley.size().max(Vec2::new(8.0, m.text_size() * zoom))).expand(4.0);
            p.rect_stroke(r, r_sm(), Stroke::new(1.0, accent), egui::StrokeKind::Outside);
            // The caret blinks, so it reads as a place to type and not as a mark.
            if ctx.input(|i| i.time).fract() < 0.6 || self.shot.is_some() {
                p.line_segment([caret.left_top(), caret.left_bottom()], Stroke::new((1.5 * zoom).max(1.5), m.color));
            }
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if let Drag::Crop { from, to } = self.drag {
            let keep = view.rect(Rect::from_two_pos(from, to));
            let dim = Color32::from_black_alpha(140);
            for part in [
                Rect::from_min_max(frame.min, Pos2::new(frame.max.x, keep.min.y)),
                Rect::from_min_max(Pos2::new(frame.min.x, keep.max.y), frame.max),
                Rect::from_min_max(Pos2::new(frame.min.x, keep.min.y), Pos2::new(keep.min.x, keep.max.y)),
                Rect::from_min_max(Pos2::new(keep.max.x, keep.min.y), Pos2::new(frame.max.x, keep.max.y)),
            ] {
                inside.rect_filled(part, 0.0, dim);
            }
            p.rect_stroke(keep, 0.0, Stroke::new(1.5, Color32::WHITE), egui::StrokeKind::Outside);
            let r = Rect::from_two_pos(from, to);
            let label = format!("{} × {}", (r.max.x.ceil() - r.min.x.floor()) as usize, (r.max.y.ceil() - r.min.y.floor()) as usize);
            let at = Pos2::new(keep.center().x, (keep.max.y + 8.0).min(area.max.y - 22.0));
            let g = p.layout_no_wrap(label, fonts::ui_font(12.0), Color32::WHITE);
            let pill = Rect::from_center_size(at + Vec2::new(0.0, g.size().y / 2.0), g.size() + Vec2::new(14.0, 6.0));
            p.rect_filled(pill, r_md(), Color32::from_black_alpha(200));
            p.galley(pill.center() - g.size() / 2.0, g, Color32::WHITE);
        }
    }

    /// Typing moved to another text mark: the one left behind is finished first.
    fn pending_stop(&mut self, now: usize) {
        if let Some(doc) = self.doc.as_mut() {
            // Empty text marks other than the one in hand are removed, lowest places last so
            // the place being typed in stays right.
            let empties: Vec<usize> = doc.marks.iter().enumerate().filter(|(i, m)| *i != now && m.kind == Kind::Text && m.text.trim().is_empty()).map(|(i, _)| i).collect();
            for i in empties.into_iter().rev() {
                doc.marks.remove(i);
                if let Some((t, _)) = self.typing.as_mut()
                    && *t > i
                {
                    *t -= 1;
                }
            }
        }
    }

    fn prompts(&mut self, ctx: &egui::Context) {
        let Some(p) = self.prompt else { return };
        let (title, yes, no) = match p {
            Prompt::Close => ("Close without copying or saving?", "Close", "Keep Editing"),
            Prompt::Open => ("Open another picture without copying or saving this one?", "Open Another", "Keep Editing"),
        };
        let mut answer = (rivet_look::widgets::return_default(ctx) && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))).then_some(true);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(24)).corner_radius(r_lg());
        egui::Modal::new(egui::Id::unique("rivet-snip-prompt")).frame(frame).show(ctx, |ui| {
            ui.set_width(420.0);
            let v = ui.visuals().clone();
            ui.label(RichText::new(title).heading().color(v.strong_text_color()));
            ui.add_space(4.0);
            ui.label(RichText::new("The marks you made will be lost.").color(v.weak_text_color()));
            ui.add_space(18.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().button_padding = Vec2::new(16.0, 7.0);
                if ui.add(egui::Button::new(RichText::new(yes).color(v.selection.stroke.color).strong()).fill(v.selection.bg_fill).corner_radius(r_sm())).clicked() {
                    answer = Some(true);
                }
                if ui.add(egui::Button::new(no).corner_radius(r_sm())).clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    answer = Some(false);
                }
            });
        });
        match (answer, p) {
            (Some(true), Prompt::Close) => {
                self.prompt = None;
                self.allow_close = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            (Some(true), Prompt::Open) => self.open(),
            (Some(false), _) => self.prompt = None,
            (None, _) => {}
        }
    }
}

/// A file's place as a person would say it: `~/Pictures/Screenshots/name.png`.
fn tidy(file: &Path) -> String {
    match file.strip_prefix(rivet_xdg::places::home()) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => file.display().to_string(),
    }
}

/// A tool's picture on its button.
fn tool_glyph(p: &egui::Painter, r: Rect, t: Tool, col: Color32) {
    let s = Stroke::new(1.6, col);
    let at = |x: f32, y: f32| r.min + Vec2::new(r.width() * x, r.height() * y);
    match t {
        Tool::Select => {
            p.add(egui::epaint::PathShape::convex_polygon(vec![at(0.12, 0.02), at(0.12, 0.88), at(0.36, 0.66), at(0.76, 0.66)], col, Stroke::NONE));
            p.line_segment([at(0.36, 0.62), at(0.56, 1.0)], Stroke::new(2.2, col));
        }
        Tool::Crop => {
            p.add(egui::epaint::PathShape::line(vec![at(0.22, 0.0), at(0.22, 0.78), at(1.0, 0.78)], s));
            p.add(egui::epaint::PathShape::line(vec![at(0.0, 0.22), at(0.78, 0.22), at(0.78, 1.0)], s));
        }
        Tool::Draw(Kind::Arrow) => {
            p.line_segment([at(0.08, 0.92), at(0.9, 0.1)], s);
            p.add(egui::epaint::PathShape::line(vec![at(0.42, 0.08), at(0.92, 0.08), at(0.92, 0.58)], s));
        }
        Tool::Draw(Kind::Line) => {
            p.line_segment([at(0.08, 0.92), at(0.92, 0.08)], s);
        }
        Tool::Draw(Kind::Rect) => {
            p.rect_stroke(Rect::from_min_max(at(0.04, 0.14), at(0.96, 0.86)), 2.0, s, egui::StrokeKind::Middle);
        }
        Tool::Draw(Kind::Ellipse) => {
            p.add(egui::epaint::EllipseShape::stroke(r.center(), Vec2::new(r.width() * 0.48, r.height() * 0.38), s));
        }
        Tool::Draw(Kind::Pen) => {
            let pts = (0..=16).map(|i| {
                let t = i as f32 / 16.0;
                at(0.04 + t * 0.92, 0.5 - (t * std::f32::consts::TAU).sin() * 0.34)
            });
            p.add(egui::epaint::PathShape::line(pts.collect(), s));
        }
        Tool::Draw(Kind::Marker) => {
            p.line_segment([at(0.06, 0.56), at(0.94, 0.56)], Stroke::new(r.height() * 0.5, col.gamma_multiply(0.45)));
            p.line_segment([at(0.06, 0.84), at(0.94, 0.84)], s);
        }
        Tool::Draw(Kind::Text) => {
            p.line_segment([at(0.1, 0.08), at(0.9, 0.08)], Stroke::new(2.0, col));
            p.line_segment([at(0.5, 0.08), at(0.5, 0.96)], Stroke::new(2.0, col));
            p.line_segment([at(0.34, 0.96), at(0.66, 0.96)], s);
        }
        Tool::Draw(Kind::Step) => {
            p.circle_stroke(r.center(), r.width() * 0.5, s);
            p.text(r.center(), Align2::CENTER_CENTER, "1", fonts::ui_font(11.0), col);
        }
        Tool::Draw(Kind::Blur) => {
            let n = 3;
            for y in 0..n {
                for x in 0..n {
                    let c = Rect::from_min_max(at(x as f32 / n as f32, y as f32 / n as f32), at((x + 1) as f32 / n as f32, (y + 1) as f32 / n as f32)).shrink(0.6);
                    p.rect_filled(c, 1.0, col.gamma_multiply(if (x + y) % 2 == 0 { 0.9 } else { 0.35 }));
                }
            }
        }
    }
}

/// The arrow that turns back (Undo) or forward (Redo).
fn turn_glyph(p: &egui::Painter, r: Rect, forward: bool, col: Color32) {
    let s = Stroke::new(1.6, col);
    let flip = |x: f32| if forward { 1.0 - x } else { x };
    let at = |x: f32, y: f32| r.min + Vec2::new(r.width() * flip(x), r.height() * y);
    // A hook: along the top to the right, round, and back along the bottom.
    let mut pts = vec![at(0.08, 0.34), at(0.62, 0.34)];
    pts.extend((0..=8).map(|i| {
        let a = -std::f32::consts::FRAC_PI_2 + i as f32 / 8.0 * std::f32::consts::PI;
        at(0.62 + a.cos() * 0.28, 0.62 + a.sin() * 0.28)
    }));
    pts.push(at(0.3, 0.9));
    p.add(egui::epaint::PathShape::line(pts, s));
    p.add(egui::epaint::PathShape::line(vec![at(0.34, 0.06), at(0.06, 0.34), at(0.34, 0.62)], s));
}

/// A made-up desktop and a few marks on it, for screenshot runs.
mod demo {
    use super::*;

    pub fn picture() -> egui::ColorImage {
        let (w, h) = (1280usize, 800usize);
        let mut px = vec![Color32::BLACK; w * h];
        let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, c: Color32| {
            for y in y0..y1.min(h) {
                px[y * w + x0..y * w + x1.min(w)].fill(c);
            }
        };
        // A wallpaper that goes from dusk blue to pink.
        for y in 0..h {
            let t = y as f32 / h as f32;
            fill(0, y, w, y + 1, palette::mix(Color32::from_rgb(0x2b, 0x2f, 0x5c), Color32::from_rgb(0xc8, 0x5a, 0x8e), t));
        }
        // A panel along the top.
        fill(0, 0, w, 34, Color32::from_rgb(0x1c, 0x1d, 0x26));
        for i in 0..4 {
            fill(16 + i * 30, 12, 34 + i * 30, 22, Color32::from_rgb(0x6c, 0x70, 0x86));
        }
        fill(w - 110, 12, w - 20, 22, Color32::from_rgb(0x9a, 0x9e, 0xb4));
        // A window: a title bar, a sidebar and some lines of text.
        fill(170, 110, 1110, 700, Color32::from_rgb(0x24, 0x26, 0x31));
        fill(170, 110, 1110, 150, Color32::from_rgb(0x2e, 0x31, 0x3f));
        fill(190, 124, 330, 136, Color32::from_rgb(0xa9, 0xad, 0xc1));
        fill(170, 150, 390, 700, Color32::from_rgb(0x20, 0x22, 0x2c));
        for i in 0..7 {
            fill(194, 178 + i * 38, 194 + 110 + (i * 37 % 60), 190 + i * 38, Color32::from_rgb(0x6c, 0x70, 0x86));
        }
        for i in 0..9 {
            fill(430, 190 + i * 44, 430 + 380 + (i * 131 % 240), 204 + i * 44, Color32::from_rgb(0x83, 0x88, 0xa0));
        }
        fill(430, 620, 560, 660, Color32::from_rgb(0xd5, 0x3f, 0x8c));
        fill(576, 620, 700, 660, Color32::from_rgb(0x3a, 0x3d, 0x4d));
        egui::ColorImage::new([w, h], px)
    }

    pub fn marks() -> Vec<Mark> {
        fn mark(kind: Kind, a: (f32, f32), b: (f32, f32), color: usize, width: f32) -> Mark {
            let mut m = Mark::new(kind, Pos2::new(a.0, a.1), COLORS[color].0, width);
            m.b = Pos2::new(b.0, b.1);
            m
        }
        let mut text = mark(Kind::Text, (912.0, 528.0), (0.0, 0.0), 0, 5.0);
        text.text = "Press this one".into();
        let mut marker = mark(Kind::Marker, (430.0, 373.0), (0.0, 0.0), 2, 5.0);
        marker.pts = vec![Pos2::new(430.0, 373.0), Pos2::new(940.0, 373.0)];
        let mut out = vec![
            mark(Kind::Blur, (186.0, 250.0), (378.0, 420.0), 0, 5.0),
            mark(Kind::Rect, (418.0, 608.0), (712.0, 672.0), 0, 5.0),
            mark(Kind::Arrow, (900.0, 560.0), (724.0, 636.0), 0, 5.0),
            text,
            marker,
            mark(Kind::Ellipse, (1150.0, 2.0), (1272.0, 32.0), 3, 3.0),
        ];
        for (n, y) in [(1, 184.0), (2, 222.0)] {
            let mut s = mark(Kind::Step, (360.0, y), (0.0, 0.0), 4, 5.0);
            s.n = n;
            out.push(s);
        }
        out
    }
}

impl eframe::App for SnipApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_SNIP", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let c = self.choice();
        self.look.frame(&ctx, &c, self.shot.is_none());
        self.poll(&ctx);
        // Closing with marks that were never copied or saved asks first.
        if ctx.input(|i| i.viewport().close_requested()) && !self.allow_close && self.unsaved() && self.shot.is_none() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.stop_typing();
            self.prompt = Some(Prompt::Close);
        }
        self.type_text(&ctx);
        self.shortcuts(&ctx);
        let v = ctx.global_style().visuals.clone();
        let bar = |top: i8, bottom: i8| egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top, bottom });
        egui::Panel::top("snip-top").show_separator_line(false).frame(bar(8, 6)).show(ui, |ui| self.top_bar(ui));
        egui::Panel::bottom("snip-bottom").show_separator_line(false).frame(bar(6, 8)).show(ui, |ui| self.bottom_bar(ui));
        let middle = egui::Frame::new().fill(palette::mix(v.panel_fill, v.extreme_bg_color, 0.6));
        egui::CentralPanel::default().frame(middle).show(ui, |ui| self.canvas(ui));
        self.prompts(&ctx);
        if self.crash.as_mut().is_some_and(|n| n.show(&ctx)) {
            self.crash = None;
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}
