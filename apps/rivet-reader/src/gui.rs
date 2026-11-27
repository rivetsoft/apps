//! Rivet Reader' window: a bar along the top, the pages, and a sidebar with the thumbnails or the
//! outline. Every document lives on rivet-doc's worker thread; the window asks it for pages at
//! the size they're shown, draws what has come back, and never waits for it.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use eframe::egui::{self, Align2, Color32, Key, Modifiers, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_doc::dark::DarkLook;
use rivet_doc::worker::{DocId, Job, Reply, Request, Worker};
use rivet_doc::{Area, Info, Kind, Link, OpenError, OutlineItem, PageText, Target};
use rivet_kit::mem::ByteCache;
use rivet_look::live::{Choice, LiveLook};
use rivet_look::palette;
use rivet_look::quick::{QuickEvent, QuickPanel, QuickRow, QuickText, fuzzy_score};
use rivet_look::theme::{Mode, r_md, r_sm};
use rivet_xdg::portal::Pick;

use crate::places::{self, Place};
use crate::settings::{DarkPages, Settings, Side};
use crate::view::{self, GAP, MARGIN, Zoom};

pub const ICON_SVG: &str = include_str!("../data/rivet-reader.svg");

/// A page drawn bigger than this (its longest side, in pixels) is drawn at this size, and the
/// part on screen is drawn again sharp on top of it.
const CAP: f32 = 2600.0;
const THUMB_W: f32 = 116.0;
const THUMB_BOX: Vec2 = Vec2::new(116.0, 150.0);
const ZOOMS: [f32; 9] = [0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0, 8.0];

struct Doc {
    id: DocId,
    path: PathBuf,
    kind: Kind,
    sizes: Vec<[f32; 2]>,
    outline: Vec<OutlineItem>,
    info: Info,
    /// Each page's text and links, once read.
    texts: HashMap<usize, (Option<PageText>, Vec<Link>)>,
    asked_text: HashSet<usize>,
}

impl Doc {
    fn name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    fn size(&self, page: usize) -> [f32; 2] {
        self.sizes.get(page).copied().unwrap_or([612.0, 792.0])
    }
}

/// A drawing asked of the worker and not yet back: page, width, height, part, thumbnail.
type Asked = (usize, u32, u32, Option<[u32; 4]>, bool);

struct Tex {
    tex: egui::TextureHandle,
    w: u32,
    h: u32,
}

/// The part of a page on screen drawn sharp, when the page is drawn bigger than [CAP].
struct Detail {
    tex: egui::TextureHandle,
    full: [u32; 2],
    clip: [u32; 4],
}

#[derive(Default)]
struct Find {
    open: bool,
    focus: bool,
    query: String,
    match_case: bool,
    /// What the search under way was started with.
    sent: (String, bool),
    serial: u64,
    changed: Option<Instant>,
    /// Every match found so far, in the order found (from the page the search started on).
    hits: Vec<(usize, Vec<Area>)>,
    current: Option<usize>,
    searched: usize,
    done: bool,
}

/// Text selected on a page: characters from `anchor` to `head`, either way round.
#[derive(Clone, Copy, PartialEq)]
struct Selection {
    page: usize,
    anchor: usize,
    head: usize,
}

impl Selection {
    fn range(&self) -> (usize, usize) {
        (self.anchor.min(self.head), self.anchor.max(self.head) + 1)
    }
}

/// What other threads report back.
enum Msg {
    Picked(Result<Option<PathBuf>, String>),
    Printed(Result<bool, String>),
    CanPrint(bool),
}

struct Password {
    path: PathBuf,
    text: String,
    wrong: bool,
}

#[derive(Clone, Copy)]
enum Cmd {
    Open,
    First,
    Last,
    Page(usize),
    FitWidth,
    FitPage,
    ActualSize,
    ZoomIn,
    ZoomOut,
    RotateRight,
    RotateLeft,
    Continuous,
    DarkPages,
    Sidebar,
    Find,
    Print,
    Properties,
    Back,
}

pub struct DocsApp {
    look: LiveLook,
    shot: Option<rivet_kit::Shot>,
    worker: Worker,
    next_id: DocId,
    doc: Option<Doc>,
    /// A document being opened.
    opening: Option<(DocId, PathBuf)>,
    /// Why the last one didn't open.
    failed: Option<String>,
    password: Option<Password>,
    settings: Settings,
    zoom: Zoom,
    scale: f32,
    scale_changed: Instant,
    rot: u8,
    continuous: bool,
    sidebar: Side,
    dark_pages: Option<DarkLook>,
    scroll: Vec2,
    view: Vec2,
    current: usize,
    /// A page to go to (and how far down it, in points) once the pages are laid out.
    jump: Option<(usize, Option<f32>)>,
    history: Vec<(usize, f32)>,
    pages: ByteCache<usize, Tex>,
    thumbs: ByteCache<usize, Tex>,
    details: HashMap<usize, Detail>,
    asked: HashSet<Asked>,
    showing: HashSet<usize>,
    find: Find,
    selection: Option<Selection>,
    selecting: bool,
    hover_link: Option<String>,
    page_field: String,
    page_field_on: bool,
    outline_open: HashSet<Vec<usize>>,
    thumb_follow: bool,
    quick: Option<QuickPanel>,
    properties: bool,
    note: Option<(String, bool, Instant)>,
    can_print: bool,
    printing: bool,
    picking: bool,
    /// When the place in the file last changed, to remember it a moment later.
    moved: Option<Instant>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    /// Checks: frames drawn since every page asked for came back.
    settled: u32,
    /// Wheel turned at the end of a single page, towards turning it.
    wheel: f32,
    /// Where a zoom should hold still (in the view), when it isn't the middle.
    zoom_anchor: Option<Vec2>,
    /// The page the thumbnails last followed.
    thumbs_at: usize,
    title: String,
    /// Checks: frames drawn in all, so a page that never comes doesn't hold the picture forever.
    frames: u32,
}

impl DocsApp {
    pub fn new(cc: &eframe::CreationContext, start: Option<PathBuf>, screenshot: Option<PathBuf>) -> DocsApp {
        let ctx = cc.egui_ctx.clone();
        let worker = Worker::start(move || ctx.request_repaint());
        let (tx, rx) = channel();
        let settings = if screenshot.is_some() { Settings::default() } else { Settings::load() };
        let mut app = DocsApp {
            look: LiveLook::new(),
            shot: screenshot.map(rivet_kit::Shot::new),
            worker,
            next_id: 1,
            doc: None,
            opening: None,
            failed: None,
            password: None,
            zoom: Zoom::Width,
            scale: 1.0,
            scale_changed: Instant::now(),
            rot: 0,
            continuous: settings.continuous,
            sidebar: settings.sidebar,
            settings,
            dark_pages: None,
            scroll: Vec2::ZERO,
            view: Vec2::new(800.0, 600.0),
            current: 0,
            jump: None,
            history: Vec::new(),
            pages: ByteCache::new(192 << 20),
            thumbs: ByteCache::new(24 << 20),
            details: HashMap::new(),
            asked: HashSet::new(),
            showing: HashSet::new(),
            find: Find::default(),
            selection: None,
            selecting: false,
            hover_link: None,
            page_field: String::new(),
            page_field_on: false,
            outline_open: HashSet::new(),
            thumb_follow: true,
            quick: None,
            properties: false,
            note: None,
            can_print: false,
            printing: false,
            picking: false,
            moved: None,
            tx,
            rx,
            settled: 0,
            wheel: 0.0,
            zoom_anchor: None,
            thumbs_at: usize::MAX,
            title: String::new(),
            frames: 0,
        };
        if app.live() {
            let tx = app.tx.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Msg::CanPrint(rivet_xdg::portal::can_print()));
            });
        }
        if let Some(p) = start {
            app.open(p, None);
        }
        app.scene();
        app
    }

    fn live(&self) -> bool {
        self.shot.is_none()
    }

    /// Checks: RIVET_READER_* set the scene once the document is open (see `opened`).
    fn scene(&mut self) {
        if self.live() {
            return;
        }
        let env = |k: &str| std::env::var(format!("RIVET_READER_{k}")).ok().filter(|v| !v.is_empty());
        // SIDEBAR: pages, outline or hidden. VIEW: single or continuous. ROTATE: quarter turns.
        match env("SIDEBAR").as_deref() {
            Some("outline") => self.sidebar = Side::Outline,
            Some("hidden") => self.sidebar = Side::Hidden,
            Some("pages") => self.sidebar = Side::Pages,
            _ => {}
        }
        if env("VIEW").as_deref() == Some("single") {
            self.continuous = false;
        }
        // DARK: on or off (else the desktop's choice).
        match env("DARK").as_deref() {
            Some("on") => self.settings.dark_pages = DarkPages::On,
            Some("off") => self.settings.dark_pages = DarkPages::Off,
            _ => {}
        }
        // FIND: a phrase, searched with the bar open. PROPERTIES: the Properties window open.
        // QUICK: Quick Actions open, with this typed.
        if let Some(q) = env("FIND") {
            self.find.open = true;
            self.find.query = q;
            self.find.changed = Some(Instant::now() - Duration::from_secs(1));
        }
        self.properties = env("PROPERTIES").is_some();
        if let Some(q) = env("QUICK") {
            self.quick = Some(QuickPanel { query: if q == "1" { String::new() } else { q }, sel: 0, focus: true });
        }
    }

    // ---- opening

    fn open(&mut self, path: PathBuf, password: Option<String>) {
        self.remember();
        let id = self.next_id;
        self.next_id += 1;
        if let Some(d) = self.doc.take() {
            self.worker.send(Request::Close { doc: d.id });
        }
        self.clear_pictures();
        self.failed = None;
        self.find = Find { open: self.find.open, ..Find::default() };
        self.selection = None;
        self.history.clear();
        self.opening = Some((id, path.clone()));
        self.worker.send(Request::Open { doc: id, path, password });
    }

    fn clear_pictures(&mut self) {
        self.pages.clear();
        self.thumbs.clear();
        self.details.clear();
        self.asked.clear();
        self.showing.clear();
    }

    fn opened(&mut self, id: DocId, result: Result<rivet_doc::worker::Opened, OpenError>) {
        let Some((want, path)) = self.opening.take() else { return };
        if want != id {
            self.opening = Some((want, path));
            return;
        }
        let o = match result {
            Ok(o) => o,
            Err(OpenError::NeedsPassword { wrong }) => {
                self.password = Some(Password { path, text: String::new(), wrong });
                return;
            }
            Err(OpenError::NoPdfium(_)) => {
                self.failed = Some("PDFs need PDFium, which isn't installed. Install PDFium with your package manager, or install Rivet Reader from Rivet Welcome.".into());
                return;
            }
            Err(OpenError::Failed(why)) => {
                self.failed = Some(format!("{} couldn't be opened. {why}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()));
                return;
            }
        };
        let place = if self.live() { places::find(&path) } else { None };
        // Comics and pictures fit the page; documents fit the width.
        self.zoom = if o.kind == Kind::Pdf { Zoom::Width } else { Zoom::Page };
        self.rot = 0;
        self.current = 0;
        self.scroll = Vec2::ZERO;
        if o.kind != Kind::Pdf && self.live() {
            self.continuous = o.kind == Kind::Comic && self.settings.continuous;
        }
        let mut page = 0;
        if let Some(p) = place {
            page = p.page.min(o.sizes.len().saturating_sub(1));
            if let Some(z) = Zoom::parse(&p.zoom) {
                self.zoom = z;
            }
            self.continuous = p.continuous;
            self.rot = p.rotation;
            match p.sidebar.as_str() {
                "pages" => self.sidebar = Side::Pages,
                "outline" => self.sidebar = Side::Outline,
                "hidden" => self.sidebar = Side::Hidden,
                _ => {}
            }
        }
        if !self.live() {
            if let Some(p) = std::env::var("RIVET_READER_PAGE").ok().and_then(|v| v.parse::<usize>().ok()) {
                page = p.saturating_sub(1);
            }
            if let Some(z) = std::env::var("RIVET_READER_ZOOM").ok().and_then(|v| Zoom::parse(&v)) {
                self.zoom = z;
            }
            if let Some(r) = std::env::var("RIVET_READER_ROTATE").ok().and_then(|v| v.parse::<u8>().ok()) {
                self.rot = r % 4;
            }
        }
        // A document with no outline opens on its pages.
        if self.sidebar == Side::Outline && o.outline.is_empty() {
            self.sidebar = Side::Pages;
        }
        if o.outline.len() == 1 {
            self.outline_open.insert(vec![0]);
        }
        self.current = page;
        self.jump = Some((page, None));
        self.doc = Some(Doc { id, path, kind: o.kind, sizes: o.sizes, outline: o.outline, info: o.info, texts: HashMap::new(), asked_text: HashSet::new() });
        if !self.find.query.trim().is_empty() {
            self.find.changed = Some(Instant::now() - Duration::from_secs(1));
        }
        if !self.live()
            && let Some(sel) = std::env::var("RIVET_READER_SELECT").ok()
        {
            // SELECT: "PAGE:FROM-TO", characters on a page (counted from 1 and 0).
            if let Some((p, r)) = sel.split_once(':')
                && let Some((a, b)) = r.split_once('-')
                && let (Ok(p), Ok(a), Ok(b)) = (p.parse::<usize>(), a.parse::<usize>(), b.parse::<usize>())
            {
                self.selection = Some(Selection { page: p.saturating_sub(1), anchor: a, head: b.saturating_sub(1) });
            }
        }
    }

    /// Remember where the file was left.
    fn remember(&mut self) {
        let Some(d) = &self.doc else { return };
        if !self.live() {
            return;
        }
        let (size, modified) = places::stamp(&d.path);
        places::remember(Place {
            path: d.path.clone(),
            size,
            modified,
            page: self.current,
            zoom: self.zoom.text(),
            continuous: self.continuous,
            rotation: self.rot,
            sidebar: match self.sidebar {
                Side::Pages => "pages",
                Side::Outline => "outline",
                Side::Hidden => "hidden",
            }
            .into(),
            opened: 0,
        });
        self.moved = None;
    }

    fn pick(&mut self) {
        if self.picking || !self.live() {
            return;
        }
        self.picking = true;
        let start = self.doc.as_ref().and_then(|d| d.path.parent().map(PathBuf::from)).or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Picked(rivet_xdg::portal::choose(Pick::Document, "Open a Document", start.as_deref())));
        });
    }

    fn print(&mut self) {
        let Some(d) = &self.doc else { return };
        if d.kind != Kind::Pdf || self.printing || !self.can_print || !self.live() {
            return;
        }
        self.printing = true;
        let (path, title) = (d.path.clone(), d.name());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Printed(rivet_xdg::portal::print_pdf(&path, &title)));
        });
    }

    fn say(&mut self, text: impl Into<String>, bad: bool) {
        self.note = Some((text.into(), bad, Instant::now()));
    }

    // ---- what comes back

    fn poll(&mut self, ctx: &egui::Context) {
        while let Some(reply) = self.worker.try_recv() {
            match reply {
                Reply::Opened { doc, result } => self.opened(doc, result),
                Reply::Rendered { job, bitmap } => {
                    let key = (job.page, job.width, job.height, job.clip, job.thumb);
                    self.asked.remove(&key);
                    let Some(d) = &self.doc else { continue };
                    if d.id != job.doc || job.dark != self.dark_pages {
                        continue;
                    }
                    let Ok(b) = bitmap else { continue };
                    let bytes = b.rgba.len();
                    let img = egui::ColorImage::from_rgba_unmultiplied([b.width as usize, b.height as usize], &b.rgba);
                    let tex = ctx.load_texture(format!("docs-{}-{}", job.page, if job.thumb { "t" } else { "p" }), img, egui::TextureOptions::LINEAR);
                    if job.thumb {
                        self.thumbs.insert(job.page, Tex { tex, w: job.width, h: job.height }, bytes);
                    } else if let Some(clip) = job.clip {
                        self.details.insert(job.page, Detail { tex, full: [job.width, job.height], clip });
                    } else {
                        self.pages.insert(job.page, Tex { tex, w: job.width, h: job.height }, bytes);
                    }
                }
                Reply::Text { doc, page, text, links } => {
                    if let Some(d) = self.doc.as_mut().filter(|d| d.id == doc) {
                        d.texts.insert(page, (text, links));
                    }
                }
                Reply::Found { doc, serial, page, hits, searched, done } => {
                    if self.doc.as_ref().is_none_or(|d| d.id != doc) || serial != self.find.serial {
                        continue;
                    }
                    self.find.searched = searched;
                    self.find.done = done;
                    let first = self.find.hits.is_empty();
                    self.find.hits.extend(hits.into_iter().map(|h| (page, h)));
                    if first && !self.find.hits.is_empty() {
                        self.go_to_hit(0);
                    }
                }
            }
        }
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Picked(r) => {
                    self.picking = false;
                    match r {
                        Ok(Some(p)) => self.open(p, None),
                        Ok(None) => {}
                        Err(e) => self.say(format!("The file picker didn't open: {e}."), true),
                    }
                }
                Msg::Printed(r) => {
                    self.printing = false;
                    match r {
                        Ok(true) => self.say("Sent to the printer.", false),
                        Ok(false) => {}
                        Err(e) => self.say(format!("It didn't print: {e}."), true),
                    }
                }
                Msg::CanPrint(yes) => self.can_print = yes,
            }
        }
        if self.opening.is_some() || self.picking || self.printing || (self.find.open && !self.find.done && !self.find.sent.0.is_empty()) {
            ctx.request_repaint_after(Duration::from_millis(150));
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if let Some(p) = dropped.into_iter().find(|p| rivet_doc::can_open(p)) {
            self.open(p, None);
        }
        // Start the search a moment after the typing stops.
        if let Some(at) = self.find.changed
            && at.elapsed() > Duration::from_millis(250)
            && let Some(d) = &self.doc
        {
            self.find.changed = None;
            let sent = (self.find.query.trim().to_string(), self.find.match_case);
            if sent != self.find.sent {
                self.find.serial += 1;
                self.find.hits.clear();
                self.find.current = None;
                self.find.searched = 0;
                self.find.done = sent.0.is_empty();
                self.worker.send(Request::Find { doc: d.id, query: sent.0.clone(), match_case: sent.1, from: self.current, serial: self.find.serial });
                self.find.sent = sent;
            }
        } else if self.find.changed.is_some() {
            ctx.request_repaint_after(Duration::from_millis(100));
        }
        if let Some(at) = self.moved
            && at.elapsed() > Duration::from_secs(1)
        {
            self.remember();
        } else if self.moved.is_some() {
            ctx.request_repaint_after(Duration::from_millis(500));
        }
    }

    // ---- moving about

    fn go(&mut self, page: usize, y: Option<f32>) {
        let Some(d) = &self.doc else { return };
        let page = page.min(d.sizes.len().saturating_sub(1));
        self.jump = Some((page, y));
        self.current = page;
        self.thumb_follow = true;
        self.moved = Some(Instant::now());
    }

    /// Go somewhere a link or the outline points, keeping where we were for Back.
    fn follow(&mut self, page: usize, y: Option<f32>) {
        let here = self.scroll.y;
        self.history.push((self.current, here));
        self.go(page, y);
    }

    fn go_to_hit(&mut self, i: usize) {
        let Some((page, areas)) = self.find.hits.get(i).cloned() else { return };
        self.find.current = Some(i);
        let top = areas.first().map_or(0.0, |a| a[1]);
        // Keep the match a little way down from the top, where the eye is.
        let above = (self.view.y * 0.3) / self.scale.max(0.01);
        self.go(page, Some((top - above).max(0.0)));
    }

    fn set_zoom(&mut self, z: Zoom) {
        self.zoom = z;
        self.moved = Some(Instant::now());
    }

    fn zoom_step(&mut self, up: bool) {
        let s = self.scale;
        let next = if up { ZOOMS.iter().copied().find(|z| *z > s * 1.01) } else { ZOOMS.iter().rev().copied().find(|z| *z < s * 0.99) };
        // The page view keeps the spot in the middle of the view where it is.
        self.set_zoom(Zoom::Scale(next.unwrap_or(s)));
    }

    fn run(&mut self, c: Cmd) {
        let n = self.doc.as_ref().map_or(0, |d| d.sizes.len());
        match c {
            Cmd::Open => self.pick(),
            Cmd::First => self.go(0, None),
            Cmd::Last => self.go(n.saturating_sub(1), None),
            Cmd::Page(p) => self.follow(p, None),
            Cmd::FitWidth => self.set_zoom(Zoom::Width),
            Cmd::FitPage => self.set_zoom(Zoom::Page),
            Cmd::ActualSize => self.set_zoom(Zoom::Scale(1.0)),
            Cmd::ZoomIn => self.zoom_step(true),
            Cmd::ZoomOut => self.zoom_step(false),
            Cmd::RotateRight | Cmd::RotateLeft => {
                self.rot = (self.rot + if matches!(c, Cmd::RotateRight) { 1 } else { 3 }) % 4;
                self.details.clear();
                let p = self.current;
                self.go(p, None);
            }
            Cmd::Continuous => {
                self.continuous = !self.continuous;
                self.settings.continuous = self.continuous;
                self.save_settings();
                let p = self.current;
                self.go(p, None);
            }
            Cmd::DarkPages => {
                let dark_now = self.dark_pages.is_some();
                self.settings.dark_pages = if dark_now { DarkPages::Off } else { DarkPages::On };
                self.save_settings();
            }
            Cmd::Sidebar => {
                self.sidebar = if self.sidebar == Side::Hidden { if self.settings.sidebar == Side::Hidden { Side::Pages } else { self.settings.sidebar } } else { Side::Hidden };
                self.moved = Some(Instant::now());
            }
            Cmd::Find => {
                self.find.open = true;
                self.find.focus = true;
            }
            Cmd::Print => self.print(),
            Cmd::Properties => self.properties = self.doc.is_some(),
            Cmd::Back => {
                if let Some((page, y)) = self.history.pop() {
                    self.current = page;
                    self.scroll.y = y;
                    self.jump = None;
                }
            }
        }
    }

    fn save_settings(&self) {
        if self.live() {
            self.settings.save();
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.quick.is_some() || self.password.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input();
        let cmd = Modifiers::COMMAND;
        let mut run = Vec::new();
        ctx.input_mut(|i| {
            let mut key = |m: Modifiers, k: Key, c: Cmd| {
                if i.consume_key(m, k) {
                    run.push(c);
                }
            };
            key(cmd, Key::O, Cmd::Open);
            key(cmd, Key::F, Cmd::Find);
            key(cmd, Key::P, Cmd::Print);
            key(cmd, Key::Plus, Cmd::ZoomIn);
            key(cmd, Key::Equals, Cmd::ZoomIn);
            key(cmd, Key::Minus, Cmd::ZoomOut);
            key(cmd, Key::Num0, Cmd::FitWidth);
            key(cmd, Key::Num1, Cmd::ActualSize);
            key(cmd, Key::Num2, Cmd::FitPage);
            key(cmd, Key::R, Cmd::RotateRight);
            key(cmd | Modifiers::SHIFT, Key::R, Cmd::RotateLeft);
            key(Modifiers::NONE, Key::F9, Cmd::Sidebar);
            key(Modifiers::ALT, Key::ArrowLeft, Cmd::Back);
            key(Modifiers::ALT, Key::Enter, Cmd::Properties);
        });
        let quick = ctx.input_mut(|i| i.consume_key(cmd | Modifiers::SHIFT, Key::P));
        if quick {
            self.quick = Some(QuickPanel { query: String::new(), sel: 0, focus: true });
        }
        for c in run {
            self.run(c);
        }
        if typing {
            return;
        }
        let copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
        if copy && let Some(text) = self.selected_text() {
            ctx.copy_text(text);
        }
        let all = ctx.input_mut(|i| i.consume_key(cmd, Key::A));
        if all && let Some(n) = self.doc.as_ref().and_then(|d| d.texts.get(&self.current)).and_then(|t| t.0.as_ref()).map(|t| t.chars.len()).filter(|n| *n > 0) {
            self.selection = Some(Selection { page: self.current, anchor: 0, head: n - 1 });
        }
        let esc = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        if esc {
            if self.find.open {
                self.find.open = false;
            } else {
                self.selection = None;
            }
        }
        let (home, end) = ctx.input_mut(|i| (i.consume_key(cmd, Key::Home) || i.consume_key(Modifiers::NONE, Key::Home), i.consume_key(cmd, Key::End) || i.consume_key(Modifiers::NONE, Key::End)));
        if home {
            self.run(Cmd::First);
        }
        if end {
            self.run(Cmd::Last);
        }
    }

    fn selected_text(&self) -> Option<String> {
        let s = self.selection?;
        let (a, b) = s.range();
        let t = self.doc.as_ref()?.texts.get(&s.page)?.0.as_ref()?;
        Some(t.slice(a, b)).filter(|t| !t.is_empty())
    }
}

// ---- drawing

#[derive(Clone, Copy)]
enum Icon {
    Sidebar,
    Minus,
    Plus,
    Rotate,
    Find,
    Menu,
    Up,
    Down,
    Close,
}

/// A square button with a small drawing on it: no font has to have the symbol.
fn icon_button(ui: &mut egui::Ui, icon: Icon, tip: &str, on: bool) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
    let v = ui.visuals().clone();
    if on || resp.hovered() {
        ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.text_color(), if on { 0.14 } else { 0.08 }));
    }
    let c = if on || resp.hovered() { v.strong_text_color() } else { v.text_color() };
    let s = Stroke::new(1.5, c);
    let p = ui.painter();
    let m = r.center();
    let line = |a: Vec2, b: Vec2| p.line_segment([m + a, m + b], s);
    match icon {
        Icon::Sidebar => {
            p.rect_stroke(Rect::from_center_size(m, Vec2::new(16.0, 13.0)), 2.0, s, egui::StrokeKind::Middle);
            line(Vec2::new(-3.0, -6.5), Vec2::new(-3.0, 6.5));
        }
        Icon::Minus => {
            line(Vec2::new(-5.5, 0.0), Vec2::new(5.5, 0.0));
        }
        Icon::Plus => {
            line(Vec2::new(-5.5, 0.0), Vec2::new(5.5, 0.0));
            line(Vec2::new(0.0, -5.5), Vec2::new(0.0, 5.5));
        }
        Icon::Rotate => {
            let pts: Vec<Pos2> = (0..=20).map(|i| {
                let a = -2.6 + i as f32 / 20.0 * 4.4;
                m + Vec2::new(a.cos(), a.sin()) * 6.0
            }).collect();
            let end = *pts.last().unwrap_or(&m);
            p.add(egui::Shape::line(pts, s));
            p.line_segment([end, end + Vec2::new(-3.5, -1.0)], s);
            p.line_segment([end, end + Vec2::new(0.5, -3.8)], s);
        }
        Icon::Find => {
            p.circle_stroke(m + Vec2::new(-1.5, -1.5), 5.0, s);
            line(Vec2::new(2.2, 2.2), Vec2::new(6.0, 6.0));
        }
        Icon::Menu => {
            for x in [-5.0, 0.0, 5.0] {
                p.circle_filled(m + Vec2::new(x, 0.0), 1.6, c);
            }
        }
        Icon::Up | Icon::Down => {
            let d = if matches!(icon, Icon::Up) { -1.0 } else { 1.0 };
            p.add(egui::Shape::line(vec![m + Vec2::new(-5.0, -2.5 * d), m + Vec2::new(0.0, 2.5 * d), m + Vec2::new(5.0, -2.5 * d)], s));
        }
        Icon::Close => {
            line(Vec2::new(-4.5, -4.5), Vec2::new(4.5, 4.5));
            line(Vec2::new(-4.5, 4.5), Vec2::new(4.5, -4.5));
        }
    }
    resp.on_hover_text(tip)
}

/// A rendered page (or the part `sub` of it, as fractions of the page before rotation) drawn
/// over the page's place on screen, turned as the page is.
fn paint_page(p: &egui::Painter, tex: &egui::TextureHandle, r: Rect, rot: u8, sub: [f32; 4]) {
    let mut mesh = egui::Mesh::with_texture(tex.id());
    let corners = [[sub[0], sub[1]], [sub[2], sub[1]], [sub[2], sub[3]], [sub[0], sub[3]]];
    let uvs = [Pos2::new(0.0, 0.0), Pos2::new(1.0, 0.0), Pos2::new(1.0, 1.0), Pos2::new(0.0, 1.0)];
    for (c, uv) in corners.into_iter().zip(uvs) {
        let [u, v] = view::to_shown(rot, c);
        mesh.vertices.push(egui::epaint::Vertex { pos: Pos2::new(r.left() + u * r.width(), r.top() + v * r.height()), uv, color: Color32::WHITE });
    }
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

fn rgb(c: Color32) -> [u8; 3] {
    [c.r(), c.g(), c.b()]
}

/// A row of the outline, as shown.
struct OutlineRow {
    path: Vec<usize>,
    depth: usize,
    title: String,
    page: Option<usize>,
    parent: bool,
}

fn outline_rows(items: &[OutlineItem], at: &mut Vec<usize>, open: Option<&HashSet<Vec<usize>>>, out: &mut Vec<OutlineRow>) {
    for (i, it) in items.iter().enumerate() {
        at.push(i);
        out.push(OutlineRow { path: at.clone(), depth: at.len() - 1, title: it.title.clone(), page: it.page, parent: !it.children.is_empty() });
        if open.is_none_or(|o| o.contains(at)) {
            outline_rows(&it.children, at, open, out);
        }
        at.pop();
    }
}

/// A line in the middle of the empty space, with a quieter one under it.
fn centered(ui: &mut egui::Ui, r: Rect, title: &str, under: &str) {
    let v = ui.visuals().clone();
    ui.painter().text(r.center() - Vec2::new(0.0, 30.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(17.0), v.text_color());
    let g = ui.painter().layout(under.to_string(), egui::FontId::proportional(13.0), v.weak_text_color(), (r.width() - 80.0).clamp(120.0, 520.0));
    ui.painter().galley(Pos2::new(r.center().x - g.size().x / 2.0, r.center().y - 10.0), g, v.weak_text_color());
}

impl DocsApp {
    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().button_padding = Vec2::new(10.0, 5.0);
            let Some((name, n, kind, path)) = self.doc.as_ref().map(|d| (d.info.title.clone().filter(|t| t.len() < 80).unwrap_or_else(|| d.name()), d.sizes.len(), d.kind, d.path.display().to_string())) else {
                if ui.add_enabled(!self.picking, egui::Button::new("Open…")).on_hover_text("Ctrl+O").clicked() {
                    self.pick();
                }
                return;
            };
            if icon_button(ui, Icon::Sidebar, "Sidebar (F9)", self.sidebar != Side::Hidden).clicked() {
                self.run(Cmd::Sidebar);
            }
            ui.add_space(4.0);
            ui.add(egui::Label::new(egui::RichText::new(name).strong().size(14.0)).truncate()).on_hover_text(path);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                let menu = icon_button(ui, Icon::Menu, "More", false);
                egui::Popup::menu(&menu).show(|ui| {
                    ui.set_min_width(190.0);
                    if ui.button("Open…").clicked() {
                        self.run(Cmd::Open);
                        ui.close();
                    }
                    if kind == Kind::Pdf && self.can_print && ui.add_enabled(!self.printing, egui::Button::new("Print…")).clicked() {
                        self.run(Cmd::Print);
                        ui.close();
                    }
                    if ui.button("Properties").clicked() {
                        self.run(Cmd::Properties);
                        ui.close();
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("Dark pages").size(12.0).color(v.weak_text_color()));
                    for (label, what) in [("With the Desktop", DarkPages::Auto), ("Always", DarkPages::On), ("Never", DarkPages::Off)] {
                        if ui.add(egui::Button::selectable(self.settings.dark_pages == what, label)).clicked() {
                            self.settings.dark_pages = what;
                            self.save_settings();
                        }
                    }
                });
                if icon_button(ui, Icon::Find, "Find (Ctrl+F)", self.find.open).clicked() {
                    if self.find.open {
                        self.find.open = false;
                    } else {
                        self.run(Cmd::Find);
                    }
                }
                if kind == Kind::Pdf && ui.add(egui::Button::selectable(self.dark_pages.is_some(), "Dark Pages")).on_hover_text("Light text on dark pages, with pictures as they are").clicked() {
                    self.run(Cmd::DarkPages);
                }
                if ui.add(egui::Button::selectable(!self.continuous, "Single Page")).on_hover_text("One page at a time, instead of all in a scroll").clicked() {
                    self.run(Cmd::Continuous);
                }
                if icon_button(ui, Icon::Rotate, "Rotate (Ctrl+R)", false).clicked() {
                    self.run(Cmd::RotateRight);
                }
                ui.add_space(6.0);
                if icon_button(ui, Icon::Plus, "Zoom In (Ctrl+Plus)", false).clicked() {
                    self.run(Cmd::ZoomIn);
                }
                let label = match self.zoom {
                    Zoom::Width => "Fit Width".to_string(),
                    Zoom::Page => "Fit Page".to_string(),
                    Zoom::Scale(s) => format!("{:.0} %", s * 100.0),
                };
                let z = ui.add(egui::Button::new(label).min_size(Vec2::new(84.0, 0.0)));
                egui::Popup::menu(&z).show(|ui| {
                    for (label, c) in [("Fit Width", Cmd::FitWidth), ("Fit Page", Cmd::FitPage)] {
                        if ui.button(label).clicked() {
                            self.run(c);
                            ui.close();
                        }
                    }
                    ui.separator();
                    for s in ZOOMS {
                        if ui.add(egui::Button::selectable(self.zoom == Zoom::Scale(s), format!("{:.0} %", s * 100.0))).clicked() {
                            self.set_zoom(Zoom::Scale(s));
                            ui.close();
                        }
                    }
                });
                if icon_button(ui, Icon::Minus, "Zoom Out (Ctrl+Minus)", false).clicked() {
                    self.run(Cmd::ZoomOut);
                }
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("of {n}")).color(v.weak_text_color()));
                if !self.page_field_on {
                    self.page_field = (self.current + 1).to_string();
                }
                let field = ui.add(egui::TextEdit::singleline(&mut self.page_field).desired_width(38.0).align(Align2::CENTER_CENTER));
                self.page_field_on = field.has_focus();
                if field.lost_focus()
                    && ui.input(|i| i.key_pressed(Key::Enter))
                    && let Ok(p) = self.page_field.trim().parse::<usize>()
                {
                    self.follow(p.clamp(1, n) - 1, None);
                }
            });
        });
    }

    fn find_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let edit = ui.add(egui::TextEdit::singleline(&mut self.find.query).hint_text("Find in the document").desired_width(260.0));
            if self.find.focus {
                edit.request_focus();
                self.find.focus = false;
            }
            if edit.changed() {
                self.find.changed = Some(Instant::now());
            }
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
            let back = ui.input(|i| i.modifiers.shift);
            if enter {
                edit.request_focus();
            }
            let n = self.find.hits.len();
            let step = |app: &mut DocsApp, forward: bool| {
                if n == 0 {
                    return;
                }
                let i = app.find.current.map_or(0, |c| if forward { (c + 1) % n } else { (c + n - 1) % n });
                app.go_to_hit(i);
            };
            if icon_button(ui, Icon::Up, "Previous (Shift+Enter)", false).clicked() || (enter && back) {
                step(self, false);
            }
            if icon_button(ui, Icon::Down, "Next (Enter)", false).clicked() || (enter && !back) {
                step(self, true);
            }
            if ui.add(egui::Button::selectable(self.find.match_case, "Match Case")).clicked() {
                self.find.match_case = !self.find.match_case;
                self.find.changed = Some(Instant::now() - Duration::from_secs(1));
            }
            let total = self.doc.as_ref().map_or(0, |d| d.sizes.len());
            let says = if self.find.sent.0.is_empty() {
                String::new()
            } else if n == 0 && self.find.done {
                "Not found".to_string()
            } else if n == 0 {
                format!("Searching page {} of {total}…", self.find.searched.max(1))
            } else {
                let at = self.find.current.map_or(String::new(), |c| format!("{} of ", c + 1));
                if self.find.done { format!("{at}{n}") } else { format!("{at}{n}, searching…") }
            };
            ui.label(egui::RichText::new(says).color(v.weak_text_color()));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if icon_button(ui, Icon::Close, "Close (Esc)", false).clicked() {
                    self.find.open = false;
                }
            });
        });
    }

    fn sidebar(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let has_outline = self.doc.as_ref().is_some_and(|d| !d.outline.is_empty());
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if ui.add(egui::Button::selectable(self.sidebar == Side::Pages, "Pages")).clicked() {
                self.sidebar = Side::Pages;
                self.settings.sidebar = Side::Pages;
                self.thumbs_at = usize::MAX;
                self.save_settings();
            }
            if ui.add_enabled(has_outline, egui::Button::selectable(self.sidebar == Side::Outline, "Outline")).on_disabled_hover_text("This document has no outline.").clicked() {
                self.sidebar = Side::Outline;
                self.settings.sidebar = Side::Outline;
                self.save_settings();
            }
        });
        ui.add_space(6.0);
        if self.sidebar == Side::Outline && has_outline {
            self.outline(ui, accent);
        } else {
            self.thumbnails(ui, accent);
        }
    }

    fn thumbnails(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let Some(d) = &self.doc else { return };
        let v = ui.visuals().clone();
        let (n, id, rot, kind) = (d.sizes.len(), d.id, self.rot, d.kind);
        let sizes = d.sizes.clone();
        let row_h = THUMB_BOX.y + 26.0;
        let ppp = ui.ctx().pixels_per_point();
        let mut area = egui::ScrollArea::vertical().id_salt("docs-thumbs").auto_shrink([false, false]);
        let hovered = ui.rect_contains_pointer(ui.available_rect_before_wrap());
        if self.thumbs_at != self.current && (!hovered || self.thumb_follow) {
            self.thumbs_at = self.current;
            area = area.vertical_scroll_offset((self.current as f32 * row_h - (ui.available_height() - row_h) / 2.0).max(0.0));
        }
        let paper = if self.dark_pages.is_some() { v.extreme_bg_color } else { Color32::WHITE };
        let mut go = None;
        let mut wanted = Vec::new();
        area.show_rows(ui, row_h, n, |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for p in range {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                let size = sizes[p];
                let [w, h] = view::turned(size, rot);
                let fit = (THUMB_BOX.x / w.max(1.0)).min(THUMB_BOX.y / h.max(1.0));
                let tr = Rect::from_center_size(Pos2::new(r.center().x, r.top() + 4.0 + THUMB_BOX.y / 2.0), Vec2::new(w * fit, h * fit));
                let painter = ui.painter();
                painter.rect_filled(tr, 2.0, paper);
                let (uw, uh) = ((size[0] * fit * ppp).round().max(1.0) as u32, (size[1] * fit * ppp).round().max(1.0) as u32);
                match self.thumbs.get(&p) {
                    Some(t) => {
                        paint_page(painter, &t.tex, tr, rot, [0.0, 0.0, 1.0, 1.0]);
                        if (t.w, t.h) != (uw, uh) {
                            wanted.push((p, uw, uh));
                        }
                    }
                    None => wanted.push((p, uw, uh)),
                }
                let on = p == self.current;
                let stroke = if on { Stroke::new(2.0, accent) } else if resp.hovered() { Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.4)) } else { Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.12)) };
                painter.rect_stroke(tr.expand(if on { 2.0 } else { 0.5 }), 3.0, stroke, egui::StrokeKind::Middle);
                let label = if on { egui::RichText::new((p + 1).to_string()).strong().color(v.strong_text_color()) } else { egui::RichText::new((p + 1).to_string()).color(v.weak_text_color()) };
                painter.text(Pos2::new(r.center().x, tr.bottom() + 11.0), Align2::CENTER_CENTER, label.text(), egui::FontId::proportional(12.0), if on { v.strong_text_color() } else { v.weak_text_color() });
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    go = Some(p);
                }
            }
        });
        let dark = if kind == Kind::Pdf { self.dark_pages } else { None };
        for (p, w, h) in wanted {
            let key = (p, w, h, None, true);
            if self.asked.insert(key) {
                self.worker.send(Request::Render(Job { doc: id, page: p, width: w, height: h, clip: None, dark, thumb: true }));
            }
        }
        if let Some(p) = go {
            self.thumbs_at = p;
            self.go(p, None);
            self.thumb_follow = false;
        }
    }

    fn outline(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let Some(d) = &self.doc else { return };
        let v = ui.visuals().clone();
        let mut rows = Vec::new();
        outline_rows(&d.outline, &mut Vec::new(), Some(&self.outline_open), &mut rows);
        // The section the current page is in: the last entry starting on or before it.
        let mut all = Vec::new();
        outline_rows(&d.outline, &mut Vec::new(), None, &mut all);
        let here = all.iter().filter(|r| r.page.is_some_and(|p| p <= self.current)).max_by_key(|r| r.page).map(|r| r.path.clone());
        let mut toggle = None;
        let mut go = None;
        egui::ScrollArea::vertical().id_salt("docs-outline").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for row in &rows {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                // The current section, or the entry it is folded away under.
                let on = here.as_ref().is_some_and(|h| *h == row.path || (h.starts_with(&row.path) && !self.outline_open.contains(&row.path)));
                if on {
                    ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, accent, 0.22));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), palette::mix(v.panel_fill, v.text_color(), 0.06));
                }
                let x = r.left() + 6.0 + row.depth as f32 * 14.0;
                if row.parent {
                    let open = self.outline_open.contains(&row.path);
                    let c = Pos2::new(x + 5.0, r.center().y);
                    let pts = if open { vec![c + Vec2::new(-3.5, -1.5), c + Vec2::new(0.0, 2.0), c + Vec2::new(3.5, -1.5)] } else { vec![c + Vec2::new(-1.5, -3.5), c + Vec2::new(2.0, 0.0), c + Vec2::new(-1.5, 3.5)] };
                    ui.painter().add(egui::Shape::line(pts, Stroke::new(1.4, v.weak_text_color())));
                }
                let col = if on { v.strong_text_color() } else { v.text_color() };
                // A long title ends in an ellipsis, with the whole of it on hover.
                let room = r.right() - (x + 16.0) - 4.0;
                let mut job = egui::text::LayoutJob::simple_singleline(row.title.clone(), egui::FontId::proportional(13.0), col);
                job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                let g = ui.painter().layout_job(job);
                let text_at = Pos2::new(x + 16.0, r.center().y - g.size().y / 2.0);
                let cut = g.elided;
                ui.painter().galley(text_at, g, col);
                let resp = if cut { resp.on_hover_text(&row.title) } else { resp };
                if resp.clicked() {
                    let on_arrow = resp.interact_pointer_pos().is_some_and(|p| p.x < x + 14.0);
                    if row.parent && (on_arrow || row.page.is_none()) {
                        toggle = Some(row.path.clone());
                    } else if let Some(p) = row.page {
                        go = Some(p);
                    }
                }
                if resp.double_clicked() && row.parent {
                    toggle = Some(row.path.clone());
                }
            }
        });
        if let Some(t) = toggle
            && !self.outline_open.remove(&t)
        {
            self.outline_open.insert(t);
        }
        if let Some(p) = go {
            self.follow(p, None);
        }
    }
}

impl DocsApp {
    fn pages_view(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let rect = ui.available_rect_before_wrap();
        let resp = ui.interact(rect, egui::Id::unique("docs-pages"), Sense::click_and_drag());
        let v = ui.visuals().clone();
        let Some(d) = &self.doc else {
            let (title, under) = if let Some((_, p)) = &self.opening {
                (format!("Opening {}…", p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), String::new())
            } else if let Some(why) = &self.failed {
                ("Nothing to show".to_string(), why.clone())
            } else {
                ("No document open".to_string(), "Open a PDF or a comic, or drop one here.".to_string())
            };
            centered(ui, rect, &title, &under);
            if self.opening.is_none() {
                let b = Rect::from_center_size(rect.center() + Vec2::new(0.0, 40.0), Vec2::new(110.0, 32.0));
                if ui.put(b, egui::Button::new("Open…")).clicked() {
                    self.pick();
                }
            }
            return;
        };
        let (id, kind, sizes) = (d.id, d.kind, d.sizes.clone());
        let ppp = ui.ctx().pixels_per_point();
        let view = rect.size();
        self.view = view;
        let rot = self.rot;

        // The zoom, held still at the middle of the view (or under the pointer).
        let single = |c: usize, continuous: bool| (!continuous).then_some(c);
        let old = view::layout(&sizes, rot, self.scale, single(self.current, self.continuous), view);
        let scale = view::scale_for(self.zoom, &sizes, rot, view, self.current);
        if (scale - self.scale).abs() > 1e-4 {
            let at = self.zoom_anchor.take().unwrap_or(view / 2.0);
            let spot = self.scroll + at;
            let p = old.page_at(spot.y);
            let r = old.rects[p];
            let frac = (spot - r.min.to_vec2()) / r.size().max(Vec2::splat(1.0));
            self.scale = scale;
            self.scale_changed = Instant::now();
            let new = view::layout(&sizes, rot, scale, single(self.current, self.continuous), view);
            let nr = new.rects[p];
            if !nr.is_negative() {
                self.scroll = nr.min.to_vec2() + frac * nr.size() - at;
            }
        }
        self.zoom_anchor = None;
        let lay = view::layout(&sizes, rot, self.scale, single(self.current, self.continuous), view);
        if let Some((p, y)) = self.jump.take()
            && let Some(r) = lay.rects.get(p).filter(|r| !r.is_negative())
        {
            let down = if rot == 0 { y.unwrap_or(0.0) * self.scale } else { 0.0 };
            // The page's top edge just below the bar, with no sliver of the page before it.
            self.scroll.y = r.top() - if p == 0 { MARGIN } else { GAP / 2.0 } + down;
            self.scroll.x = (lay.size.x - view.x) / 2.0;
        }

        // Scrolling, zooming and the keys.
        let (wheel, pinch) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        let over = resp.contains_pointer();
        if over && pinch != 1.0 {
            self.zoom_anchor = resp.hover_pos().map(|p| p - rect.min);
            self.set_zoom(Zoom::Scale((self.scale * pinch).clamp(view::MIN_SCALE, view::MAX_SCALE)));
        } else if over {
            self.scroll -= wheel;
            if !self.continuous && lay.size.y <= view.y + 1.0 && wheel.y != 0.0 {
                self.wheel += wheel.y;
            }
        }
        let typing = ui.ctx().egui_wants_keyboard_input();
        let n = sizes.len();
        if !typing && self.quick.is_none() && self.password.is_none() {
            let (down, up, pg_down, pg_up, left, right) = ui.input_mut(|i| {
                let k = |i: &mut egui::InputState, key| i.consume_key(Modifiers::NONE, key);
                (k(i, Key::ArrowDown), k(i, Key::ArrowUp), k(i, Key::PageDown) || k(i, Key::Space), k(i, Key::PageUp) || i.consume_key(Modifiers::SHIFT, Key::Space), k(i, Key::ArrowLeft), k(i, Key::ArrowRight))
            });
            let bottom = (lay.size.y - view.y).max(0.0);
            if down {
                self.scroll.y += 48.0;
            }
            if up {
                self.scroll.y -= 48.0;
            }
            if self.continuous {
                if pg_down {
                    self.scroll.y += view.y * 0.9;
                }
                if pg_up {
                    self.scroll.y -= view.y * 0.9;
                }
                if lay.size.x <= view.x + 1.0 {
                    if right {
                        self.go(self.current + 1, None);
                    }
                    if left && self.current > 0 {
                        self.go(self.current - 1, None);
                    }
                } else {
                    self.scroll.x += if right { 48.0 } else { 0.0 } - if left { 48.0 } else { 0.0 };
                }
            } else {
                // One page at a time: Page Down scrolls to the end of the page, then turns it.
                let next = right || (pg_down && self.scroll.y >= bottom - 1.0) || self.wheel < -60.0;
                let prev = left || (pg_up && self.scroll.y <= 1.0) || self.wheel > 60.0;
                if next && self.current + 1 < n {
                    self.wheel = 0.0;
                    self.go(self.current + 1, None);
                } else if prev && self.current > 0 {
                    self.wheel = 0.0;
                    self.go(self.current - 1, None);
                } else if pg_down {
                    self.scroll.y += view.y * 0.9;
                } else if pg_up {
                    self.scroll.y -= view.y * 0.9;
                }
                if next || prev {
                    self.wheel = 0.0;
                }
            }
        }
        // Dragging moves the page round, where there is no text to select.
        let on_text = kind == Kind::Pdf;
        if resp.dragged_by(egui::PointerButton::Middle) || (!on_text && resp.dragged_by(egui::PointerButton::Primary)) {
            self.scroll -= resp.drag_delta();
        }
        self.scroll.x = self.scroll.x.clamp(0.0, (lay.size.x - view.x).max(0.0));
        self.scroll.y = self.scroll.y.clamp(0.0, (lay.size.y - view.y).max(0.0));
        if self.continuous {
            let now = lay.page_at(self.scroll.y + view.y * 0.3);
            if now != self.current {
                self.current = now;
                self.moved = Some(Instant::now());
            }
        }

        // The pages.
        let painter = ui.painter_at(rect);
        let origin = rect.min.to_vec2() - self.scroll;
        let shown = lay.pages_in(self.scroll.y, self.scroll.y + view.y);
        let paper = if self.dark_pages.is_some() { v.extreme_bg_color } else { Color32::WHITE };
        let dark = if kind == Kind::Pdf { self.dark_pages } else { None };
        let settled = self.scale_changed.elapsed() > Duration::from_millis(200);
        let mut renders = Vec::new();
        let mut texts = Vec::new();
        let pointer = resp.hover_pos();
        let mut under: Option<(usize, Rect)> = None;
        for &p in &shown {
            let r = lay.rects[p].translate(origin);
            if pointer.is_some_and(|q| r.contains(q)) {
                under = Some((p, r));
            }
            let size = sizes[p];
            painter.rect_filled(r.expand(1.0), 1.0, palette::mix(v.panel_fill, Color32::BLACK, 0.25));
            painter.rect_filled(r, 0.0, paper);
            let (fw, fh) = (size[0] * self.scale * ppp, size[1] * self.scale * ppp);
            let k = (CAP / fw.max(fh)).min(1.0);
            let (bw, bh) = ((fw * k).round().max(1.0) as u32, (fh * k).round().max(1.0) as u32);
            let have = self.pages.get(&p).map(|t| {
                paint_page(&painter, &t.tex, r, rot, [0.0, 0.0, 1.0, 1.0]);
                (t.w, t.h)
            });
            if have != Some((bw, bh)) && (settled || have.is_none()) {
                renders.push(Job { doc: id, page: p, width: bw, height: bh, clip: None, dark, thumb: false });
            }
            if k < 1.0 {
                // Too big to draw whole: the part on screen again, sharp.
                let full = [fw.round() as u32, fh.round() as u32];
                let seen = r.intersect(rect);
                let a = view::screen_to_page(r, size, rot, seen.min);
                let b = view::screen_to_page(r, size, rot, seen.max);
                let fx = |x: f32| (x / size[0].max(1.0)).clamp(0.0, 1.0);
                let fy = |y: f32| (y / size[1].max(1.0)).clamp(0.0, 1.0);
                let need = [(fx(a[0].min(b[0])) * fw) as u32, (fy(a[1].min(b[1])) * fh) as u32, (fx(a[0].max(b[0])) * fw).ceil() as u32, (fy(a[1].max(b[1])) * fh).ceil() as u32];
                let fresh = self.details.get(&p).filter(|dt| dt.full == full);
                if let Some(dt) = fresh {
                    let c = dt.clip;
                    paint_page(&painter, &dt.tex, r, rot, [c[0] as f32 / fw, c[1] as f32 / fh, c[2] as f32 / fw, c[3] as f32 / fh]);
                }
                let covers = fresh.is_some_and(|dt| dt.clip[0] <= need[0] && dt.clip[1] <= need[1] && dt.clip[2] >= need[2] && dt.clip[3] >= need[3]);
                if !covers && settled {
                    // Some room round it, so a little scrolling needs nothing new.
                    let (mw, mh) = ((need[2] - need[0]) / 3 + 64, (need[3] - need[1]) / 3 + 64);
                    let clip = [need[0].saturating_sub(mw), need[1].saturating_sub(mh), (need[2] + mw).min(full[0]), (need[3] + mh).min(full[1])];
                    if !self.asked.iter().any(|a| a.0 == p && a.3.is_some() && a.1 == full[0]) {
                        renders.push(Job { doc: id, page: p, width: full[0], height: full[1], clip: Some(clip), dark, thumb: false });
                    }
                }
            }
            if kind == Kind::Pdf && !self.doc.as_ref().is_some_and(|d| d.asked_text.contains(&p)) {
                texts.push(p);
            }
            // Search matches, then the selection.
            let doc = self.doc.as_ref().expect("open");
            for (i, (hp, areas)) in self.find.hits.iter().enumerate() {
                if *hp != p {
                    continue;
                }
                let current = self.find.current == Some(i);
                for a in areas {
                    let s = view::area_to_screen(r, size, rot, *a).expand(1.0);
                    painter.rect_filled(s, 2.0, if current { accent.gamma_multiply(0.55) } else { Color32::from_rgb(255, 214, 10).gamma_multiply(0.45) });
                    if current {
                        painter.rect_stroke(s, 2.0, Stroke::new(1.5, accent), egui::StrokeKind::Outside);
                    }
                }
            }
            if let Some(sel) = self.selection.filter(|s| s.page == p)
                && let Some((Some(t), _)) = doc.texts.get(&p)
            {
                let (a, b) = sel.range();
                for area in t.areas(a, b) {
                    painter.rect_filled(view::area_to_screen(r, size, rot, area), 0.0, accent.gamma_multiply(0.35));
                }
            }
        }

        // Links and selecting text, on the page under the pointer.
        self.hover_link = None;
        let mut follow: Option<Target> = None;
        if let Some((p, r)) = under
            && let Some(q) = pointer
        {
            let size = sizes[p];
            let at = view::screen_to_page(r, size, rot, q);
            let doc = self.doc.as_ref().expect("open");
            let (text, links) = match doc.texts.get(&p) {
                Some((t, l)) => (t.as_ref(), l.as_slice()),
                None => (None, &[][..]),
            };
            let link = links.iter().find(|l| at[0] >= l.area[0] && at[0] <= l.area[2] && at[1] >= l.area[1] && at[1] <= l.area[3]).cloned();
            if let Some(l) = &link {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                self.hover_link = Some(match &l.to {
                    Target::Url(u) => u.clone(),
                    Target::Page(n, _) => format!("Page {}", n + 1),
                });
            } else if let Some(t) = text
                && t.boxes.iter().any(|b| at[0] >= b[0] && at[0] <= b[2] && at[1] >= b[1] && at[1] <= b[3])
            {
                ui.ctx().set_cursor_icon(egui::CursorIcon::Text);
            }
            if let Some(t) = text {
                if resp.drag_started_by(egui::PointerButton::Primary) && link.is_none() {
                    if let Some(i) = t.nearest(at[0], at[1]) {
                        self.selection = Some(Selection { page: p, anchor: i, head: i });
                        self.selecting = true;
                    }
                } else if self.selecting
                    && resp.dragged_by(egui::PointerButton::Primary)
                    && let (Some(sel), Some(i)) = (self.selection.as_mut(), t.nearest(at[0], at[1]))
                    && sel.page == p
                {
                    sel.head = i;
                }
                if resp.triple_clicked() {
                    if let Some(i) = t.nearest(at[0], at[1]) {
                        let (a, b) = t.line(i);
                        self.selection = Some(Selection { page: p, anchor: a, head: b.saturating_sub(1).max(a) });
                    }
                } else if resp.double_clicked()
                    && let Some(i) = t.nearest(at[0], at[1])
                {
                    let (a, b) = t.word(i);
                    self.selection = Some(Selection { page: p, anchor: a, head: b.saturating_sub(1).max(a) });
                }
            }
            if resp.clicked() {
                match link {
                    Some(l) => follow = Some(l.to),
                    None => self.selection = None,
                }
            }
        }
        if resp.drag_stopped() {
            self.selecting = false;
        }
        // While selecting, the view scrolls when the pointer goes past its top or bottom.
        if self.selecting && let Some(q) = resp.interact_pointer_pos() {
            if q.y > rect.bottom() - 20.0 {
                self.scroll.y += 12.0;
            } else if q.y < rect.top() + 20.0 {
                self.scroll.y -= 12.0;
            }
        }
        match follow {
            Some(Target::Page(p, y)) => self.follow(p, y),
            Some(Target::Url(u)) => {
                if self.live()
                    && let Err(e) = rivet_xdg::launch::open_url(&u)
                {
                    self.say(format!("The link didn't open: {e}."), true);
                }
            }
            None => {}
        }

        // The scroll bar.
        if lay.size.y > view.y + 1.0 {
            let track = Rect::from_min_max(Pos2::new(rect.right() - 10.0, rect.top() + 4.0), Pos2::new(rect.right() - 3.0, rect.bottom() - 4.0));
            let h = (view.y / lay.size.y * track.height()).max(28.0);
            let room = (track.height() - h).max(1.0);
            let max = lay.size.y - view.y;
            let top = track.top() + self.scroll.y / max * room;
            let thumb = Rect::from_min_size(Pos2::new(track.left(), top), Vec2::new(track.width(), h));
            let bar = ui.interact(track.expand2(Vec2::new(3.0, 0.0)), egui::Id::unique("docs-scroll"), Sense::click_and_drag());
            if bar.dragged() {
                self.scroll.y = (self.scroll.y + bar.drag_delta().y / room * max).clamp(0.0, max);
            } else if bar.clicked()
                && let Some(q) = bar.interact_pointer_pos()
            {
                self.scroll.y = ((q.y - track.top() - h / 2.0) / room * max).clamp(0.0, max);
            }
            let strong = bar.hovered() || bar.dragged();
            painter.rect_filled(thumb, 4.0, palette::mix(v.panel_fill, v.text_color(), if strong { 0.45 } else { 0.25 }));
        }

        // A line along the bottom: where a link goes, or what just happened.
        let note = self.hover_link.clone().map(|t| (t, false)).or_else(|| self.note.as_ref().filter(|n| n.2.elapsed() < Duration::from_secs(4)).map(|n| (n.0.clone(), n.1)));
        if let Some((text, bad)) = note {
            let col = if bad { rivet_look::theme::status(ui.ctx()).danger } else { v.text_color() };
            let g = painter.layout(text, egui::FontId::proportional(12.5), col, (view.x - 40.0).max(100.0));
            let b = Rect::from_min_size(Pos2::new(rect.left() + 10.0, rect.bottom() - g.size().y - 18.0), g.size() + Vec2::new(16.0, 10.0));
            painter.rect_filled(b, r_sm(), v.window_fill);
            painter.rect_stroke(b, r_sm(), Stroke::new(1.0, palette::mix(v.window_fill, v.text_color(), 0.15)), egui::StrokeKind::Inside);
            painter.galley(b.min + Vec2::new(8.0, 5.0), g, col);
        }

        // Ask for what is missing, and say which pages are on screen.
        let near: HashSet<usize> = shown.iter().flat_map(|p| [p.saturating_sub(1), *p, (*p + 1).min(n.saturating_sub(1))]).collect();
        if near != self.showing {
            self.asked.retain(|a| a.4 || near.contains(&a.0));
            self.showing = near.clone();
            self.worker.send(Request::Showing { doc: id, pages: near });
        }
        for job in renders {
            let key = (job.page, job.width, job.height, job.clip, job.thumb);
            if self.asked.insert(key) {
                self.worker.send(Request::Render(job));
            }
        }
        if let Some(d) = self.doc.as_mut() {
            for p in texts {
                d.asked_text.insert(p);
                self.worker.send(Request::Text { doc: id, page: p });
            }
        }
    }

    fn password_window(&mut self, ctx: &egui::Context) {
        let Some(mut pw) = self.password.take() else { return };
        let (mut open, mut cancel) = (false, false);
        let name = pw.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        egui::Modal::new(egui::Id::unique("docs-password")).show(ctx, |ui| {
            ui.set_width(360.0);
            ui.label(egui::RichText::new(format!("{name} is locked")).strong().size(15.0));
            ui.add_space(4.0);
            let says = if pw.wrong { "That password didn't open it. Try again." } else { "Type its password to open it." };
            ui.label(says);
            ui.add_space(10.0);
            let edit = ui.add(egui::TextEdit::singleline(&mut pw.text).password(true).desired_width(f32::INFINITY));
            edit.request_focus();
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(!pw.text.is_empty(), egui::Button::new(egui::RichText::new("Open").strong())).clicked() || (!pw.text.is_empty() && ui.input(|i| i.key_pressed(Key::Enter))) {
                        open = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        });
        if open {
            self.open(pw.path, Some(pw.text));
        } else if !cancel {
            self.password = Some(pw);
        }
    }

    fn properties_window(&mut self, ctx: &egui::Context) {
        if !self.properties {
            return;
        }
        let Some(d) = &self.doc else {
            self.properties = false;
            return;
        };
        let mut rows: Vec<(&str, String)> = vec![("File", d.name()), ("Folder", d.path.parent().map(|p| p.display().to_string()).unwrap_or_default())];
        for (label, value) in [("Title", &d.info.title), ("Author", &d.info.author), ("Subject", &d.info.subject), ("Made with", &d.info.producer)] {
            if let Some(v) = value {
                rows.push((label, v.clone()));
            }
        }
        if let Some(v) = &d.info.version {
            rows.push(("PDF version", v.clone()));
        }
        rows.push(("Pages", d.sizes.len().to_string()));
        if d.kind == Kind::Pdf {
            let [w, h] = d.size(self.current);
            rows.push(("Page size", format!("{:.0} × {:.0} mm ({:.1} × {:.1} in)", w / 72.0 * 25.4, h / 72.0 * 25.4, w / 72.0, h / 72.0)));
        }
        if let Ok(m) = std::fs::metadata(&d.path) {
            rows.push(("File size", human_size(m.len())));
        }
        let mut close = false;
        egui::Modal::new(egui::Id::unique("docs-properties")).show(ctx, |ui| {
            ui.set_width(440.0);
            ui.label(egui::RichText::new("Properties").strong().size(15.0));
            ui.add_space(10.0);
            egui::Grid::new("docs-props").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                for (k, v) in &rows {
                    ui.label(egui::RichText::new(*k).color(ui.visuals().weak_text_color()));
                    ui.add(egui::Label::new(v.as_str()).wrap());
                    ui.end_row();
                }
            });
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Close").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    close = true;
                }
            });
        });
        if close {
            self.properties = false;
        }
    }

    fn quick_panel(&mut self, ctx: &egui::Context) {
        let Some(mut panel) = self.quick.take() else { return };
        let q = panel.query.trim().to_string();
        let n = self.doc.as_ref().map_or(0, |d| d.sizes.len());
        let mut cmds: Vec<(String, String, &str, Cmd)> = Vec::new();
        if let Ok(p) = q.parse::<usize>()
            && (1..=n).contains(&p)
        {
            cmds.push(("Go".into(), format!("Go to Page {p}"), "", Cmd::Page(p - 1)));
        }
        let mut list: Vec<(&str, &str, &str, Cmd)> = vec![("File", "Open…", "Ctrl+O", Cmd::Open)];
        if n > 0 {
            list.extend([
                ("Go", "First Page", "Home", Cmd::First),
                ("Go", "Last Page", "End", Cmd::Last),
                ("Go", "Back", "Alt+Left", Cmd::Back),
                ("Zoom", "Fit Width", "Ctrl+0", Cmd::FitWidth),
                ("Zoom", "Fit Page", "Ctrl+2", Cmd::FitPage),
                ("Zoom", "Actual Size", "Ctrl+1", Cmd::ActualSize),
                ("Zoom", "Zoom In", "Ctrl+Plus", Cmd::ZoomIn),
                ("Zoom", "Zoom Out", "Ctrl+Minus", Cmd::ZoomOut),
                ("View", "Rotate Right", "Ctrl+R", Cmd::RotateRight),
                ("View", "Rotate Left", "Ctrl+Shift+R", Cmd::RotateLeft),
                ("View", "Single Page", "", Cmd::Continuous),
                ("View", "Sidebar", "F9", Cmd::Sidebar),
                ("Find", "Find", "Ctrl+F", Cmd::Find),
                ("File", "Properties", "Alt+Enter", Cmd::Properties),
            ]);
            if self.doc.as_ref().is_some_and(|d| d.kind == Kind::Pdf) {
                list.push(("View", "Dark Pages", "", Cmd::DarkPages));
                if self.can_print {
                    list.push(("File", "Print…", "Ctrl+P", Cmd::Print));
                }
            }
        }
        cmds.extend(list.into_iter().map(|(g, l, k, c)| (g.to_string(), l.to_string(), k, c)));
        let mut ranked: Vec<(i32, QuickRow, Cmd)> = cmds
            .into_iter()
            .filter_map(|(g, l, k, c)| {
                let state = match c {
                    Cmd::Continuous => Some(!self.continuous),
                    Cmd::DarkPages => Some(self.dark_pages.is_some()),
                    _ => None,
                };
                let s = fuzzy_score(&l, &q).or_else(|| fuzzy_score(&format!("{g} {l}"), &q).map(|v| v - 200))?;
                Some((s, QuickRow::Command { group: g, label: l, keys: k.to_string(), state }, c))
            })
            .collect();
        // The outline's entries, by name.
        if let Some(d) = &self.doc {
            let mut all = Vec::new();
            outline_rows(&d.outline, &mut Vec::new(), None, &mut all);
            for r in all {
                if let (Some(p), Some(s)) = (r.page, fuzzy_score(&r.title, &q)) {
                    ranked.push((s - 50, QuickRow::Item { icon: None, name: r.title, sub: format!("Page {}", p + 1) }, Cmd::Page(p)));
                }
            }
        }
        if !q.is_empty() {
            ranked.sort_by_key(|r| std::cmp::Reverse(r.0));
        }
        ranked.truncate(200);
        let rows: Vec<QuickRow> = ranked.iter().map(|r| match &r.1 {
            QuickRow::Command { group, label, keys, state } => QuickRow::Command { group: group.clone(), label: label.clone(), keys: keys.clone(), state: *state },
            QuickRow::Item { name, sub, .. } => QuickRow::Item { icon: None, name: name.clone(), sub: sub.clone() },
        }).collect();
        let text = QuickText { hint: "Type a command, a section or a page number…", empty: "Nothing matches", legend: "Up/Down choose   ·   Enter run   ·   Esc close" };
        match panel.show(ctx, "docs-quick", &rows, &text, None) {
            Some(QuickEvent::Pick(i)) => {
                if let Some((_, _, c)) = ranked.get(i) {
                    self.run(*c);
                }
            }
            Some(QuickEvent::Close) => {}
            None => self.quick = Some(panel),
        }
    }
}

/// Bytes as a person reads them: "5 bytes", "1.2 MB".
pub fn human_size(n: u64) -> String {
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

impl Drop for DocsApp {
    fn drop(&mut self) {
        self.remember();
    }
}

impl eframe::App for DocsApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if self.shot.is_some() {
            raw.focused = true;
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let choice = Choice { mode: Mode::System, accent: [0x35, 0x84, 0xe4], follow_desktop_colors: true, text_size: 0.0 };
        self.look.frame(&ctx, &choice, self.live());
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&choice);
        // Dark pages: when asked for (or the desktop is dark), for documents.
        let want = match self.settings.dark_pages {
            DarkPages::Auto => v.dark_mode,
            DarkPages::On => true,
            DarkPages::Off => false,
        };
        let look = (want && self.doc.as_ref().is_some_and(|d| d.kind == Kind::Pdf)).then(|| DarkLook { paper: rgb(v.extreme_bg_color), ink: rgb(v.text_color()) });
        if look != self.dark_pages {
            self.dark_pages = look;
            self.clear_pictures();
        }
        self.poll(&ctx);
        self.shortcuts(&ctx);
        let title = self.doc.as_ref().map_or("Rivet Reader".to_string(), |d| format!("{} - Rivet Reader", d.name()));
        if self.live() && title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        let bar = |top: i8, bottom: i8| egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 10, top, bottom });
        egui::Panel::top("docs-top").show_separator_line(false).frame(bar(8, 6)).show(ui, |ui| {
            self.top_bar(ui);
            if self.find.open && self.doc.is_some() {
                ui.add_space(4.0);
                self.find_bar(ui);
            }
        });
        if self.doc.is_some() && self.sidebar != Side::Hidden {
            egui::Panel::left("docs-side").show_separator_line(false).resizable(false).exact_size(THUMB_W + 44.0).frame(bar(4, 8)).show(ui, |ui| self.sidebar(ui, accent));
        }
        let middle = egui::Frame::new().fill(palette::mix(v.panel_fill, v.extreme_bg_color, 0.5)).corner_radius(r_md());
        egui::CentralPanel::default().frame(middle).show(ui, |ui| self.pages_view(ui, accent));
        self.password_window(&ctx);
        self.properties_window(&ctx);
        self.quick_panel(&ctx);
        if self.shot.is_some() {
            // Checks: the picture is taken once every page asked for has been drawn.
            let busy = self.opening.is_some() || !self.asked.is_empty() || (self.find.open && !self.find.sent.0.is_empty() && !self.find.done) || self.find.changed.is_some();
            self.settled = if busy { 0 } else { self.settled + 1 };
            self.frames += 1;
            if self.settled > 3 || self.frames > 3000 || self.shot.as_ref().is_some_and(|s| s.frames() > 0) {
                if let Some(shot) = self.shot.as_mut() {
                    shot.step(&ctx);
                }
            } else {
                ctx.request_repaint();
            }
        }
    }
}
