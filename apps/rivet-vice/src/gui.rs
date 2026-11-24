//! Rivet Vice's window: a toolbar, where you are in the archive, what is in that folder, and a
//! line along the bottom that says what is happening. Listing, extracting and compressing all
//! run on threads and report back, so the window never waits on an archive tool.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Color32, Key, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_archive::{Entry, Format};
use rivet_look::icons::IconTheme;
use rivet_look::live::{Choice, LiveLook};
use rivet_look::palette;
use rivet_look::theme::{Mode, r_md, r_sm};
use rivet_xdg::portal::Pick;

pub const ICON_SVG: &str = include_str!("../data/rivet-vice.svg");

/// What the window opens on.
pub enum Start {
    Empty,
    Open(PathBuf),
    /// Files to make an archive of.
    Compress(Vec<PathBuf>),
}

/// What a thread reports back.
enum Msg {
    Listed(PathBuf, Result<Vec<Entry>, String>),
    /// Work finished: what to say, whether it went wrong, and something made to point at.
    Done(String, bool, Option<PathBuf>),
    /// A new archive was made: open it.
    Made(PathBuf),
    Picked(Ask, Result<Option<PathBuf>, String>),
}

/// What the file picker was asked for.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    Archive,
    /// Where to extract everything, or only what is selected.
    ExtractAll,
    ExtractSome,
    /// A folder to make an archive of.
    Folder,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Sort {
    Name,
    Size,
    Changed,
}

/// The archive being made: what goes in, its name without the ending, and its kind.
struct Making {
    sources: Vec<PathBuf>,
    name: String,
    format: Format,
}

pub struct ViceApp {
    look: LiveLook,
    icons: IconTheme,
    shot: Option<rivet_kit::Shot>,
    archive: Option<PathBuf>,
    entries: Vec<Entry>,
    /// The archive being read (its list hasn't arrived).
    loading: bool,
    /// The folder shown, as a path in the archive ("" at its top).
    folder: String,
    selected: BTreeSet<String>,
    /// The row a range selection starts from.
    anchor: Option<String>,
    sort: (Sort, bool),
    /// The rows shown: indexes into `entries`, in order. Made again when `stale`.
    rows: Vec<usize>,
    stale: bool,
    /// Work under way: what it says, and the flag that stops it.
    busy: Option<(String, Arc<AtomicBool>)>,
    /// The line along the bottom: what it says, whether it is trouble, something to show.
    note: Option<(String, bool, Option<PathBuf>)>,
    picking: Option<Ask>,
    making: Option<Making>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
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

/// An entry's time as "2026-11-24 00:10" (the clock the archive tool printed, as it reads).
pub fn when(stamp: i64) -> String {
    if stamp <= 0 {
        return String::new();
    }
    let (days, secs) = (stamp.div_euclid(86400), stamp.rem_euclid(86400));
    // A day count back to a date, in the calendar in use everywhere now.
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}", secs / 3600, secs % 3600 / 60)
}

/// The entries right inside `folder`, folders first, sorted as asked.
fn children(entries: &[Entry], folder: &str, sort: (Sort, bool)) -> Vec<usize> {
    let mut rows: Vec<usize> = entries.iter().enumerate().filter(|(_, e)| e.parent() == folder).map(|(i, _)| i).collect();
    rows.sort_by(|a, b| {
        let (x, y) = (&entries[*a], &entries[*b]);
        let by = match sort.0 {
            Sort::Name => std::cmp::Ordering::Equal,
            Sort::Size => x.size.cmp(&y.size),
            Sort::Changed => x.modified.cmp(&y.modified),
        };
        let by = by.then_with(|| x.name().to_lowercase().cmp(&y.name().to_lowercase()));
        y.dir.cmp(&x.dir).then(if sort.1 { by } else { by.reverse() })
    });
    rows
}

/// How many files are under a folder, and how big they are together.
fn inside(entries: &[Entry], folder: &str) -> (usize, u64) {
    let under = format!("{folder}/");
    entries.iter().filter(|e| !e.dir && e.path.starts_with(&under)).fold((0, 0), |(n, s), e| (n + 1, s + e.size))
}

/// A made-up archive, for screenshots.
fn demo() -> Vec<Entry> {
    let at = 1_795_000_000;
    let f = |path: &str, size: u64, days: i64| Entry { path: path.into(), dir: false, size, modified: at - days * 86400 - size as i64 % 80000 };
    let d = |path: &str, days: i64| Entry { path: path.into(), dir: true, size: 0, modified: at - days * 86400 };
    vec![d("Trip", 3), d("Trip/Photos", 3), f("Trip/Photos/harbor.jpg", 3_412_880, 9), f("Trip/Photos/lighthouse.jpg", 2_904_112, 9), f("Trip/Photos/low tide.jpg", 4_120_034, 8), d("Trip/Receipts", 5), f("Trip/Receipts/ferry.pdf", 88_410, 12), f("Trip/Receipts/hotel.pdf", 131_902, 11), f("Trip/itinerary.md", 4_211, 20), f("Trip/packing list.txt", 912, 25), f("Trip/route.gpx", 248_330, 14)]
}

impl ViceApp {
    pub fn new(_cc: &eframe::CreationContext, start: Start, screenshot: Option<PathBuf>) -> ViceApp {
        let (tx, rx) = channel();
        let mut app = ViceApp {
            look: LiveLook::new(),
            icons: IconTheme::new(&rivet_look::icons::detect_system_theme()),
            shot: screenshot.map(rivet_kit::Shot::new),
            archive: None,
            entries: Vec::new(),
            loading: false,
            folder: String::new(),
            selected: BTreeSet::new(),
            anchor: None,
            sort: (Sort::Name, true),
            rows: Vec::new(),
            stale: true,
            busy: None,
            note: None,
            picking: None,
            making: None,
            tx,
            rx,
        };
        if app.shot.is_some() {
            // Checks draw a made-up archive and run nothing. RIVET_VICE_SCENE picks another
            // state: "empty" (no archive), "compress" (the question asked before making one),
            // "busy" (extracting), "selected" (two rows chosen, inside a folder).
            let scene = std::env::var("RIVET_VICE_SCENE").unwrap_or_default();
            if scene != "empty" {
                app.archive = Some(PathBuf::from("/home/sam/Documents/Trip.tar.gz"));
                app.entries = demo();
                app.folder = "Trip".into();
            }
            match scene.as_str() {
                "compress" => app.making = Some(Making { sources: vec![PathBuf::from("/home/sam/Documents/Trip")], name: "Trip".into(), format: Format::TarGz }),
                "busy" => app.busy = Some(("Extracting Trip.tar.gz…".into(), Arc::default())),
                "selected" => {
                    app.folder = "Trip/Photos".into();
                    app.selected = ["Trip/Photos/harbor.jpg", "Trip/Photos/low tide.jpg"].into_iter().map(String::from).collect();
                }
                "done" => app.note = Some(("Extracted to Documents/Trip.".into(), false, Some(PathBuf::from("/home/sam/Documents/Trip")))),
                _ => {}
            }
            return app;
        }
        match start {
            Start::Empty => {}
            Start::Open(p) => app.open(p),
            Start::Compress(files) => app.ask_compress(files),
        }
        app
    }

    fn live(&self) -> bool {
        self.shot.is_none()
    }

    /// Read an archive, and show it when its list arrives.
    fn open(&mut self, path: PathBuf) {
        if !self.live() {
            return;
        }
        self.loading = true;
        self.note = None;
        self.entries.clear();
        self.selected.clear();
        self.folder.clear();
        self.stale = true;
        self.archive = Some(path.clone());
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let list = rivet_archive::list(&path, &AtomicBool::new(false));
            let _ = tx.send(Msg::Listed(path, list));
        });
    }

    /// Ask the desktop's file picker, on a thread.
    fn pick(&mut self, ask: Ask) {
        if self.picking.is_some() || !self.live() {
            return;
        }
        self.picking = Some(ask);
        let start = self.archive.as_ref().and_then(|a| a.parent().map(PathBuf::from)).or_else(|| std::env::var_os("HOME").map(PathBuf::from));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let (kind, title) = match ask {
                Ask::Archive => (Pick::Archive, "Open an Archive"),
                Ask::ExtractAll | Ask::ExtractSome => (Pick::Folder, "Extract Into"),
                Ask::Folder => (Pick::Folder, "Folder to Compress"),
            };
            let _ = tx.send(Msg::Picked(ask, rivet_xdg::portal::choose(kind, title, start.as_deref())));
        });
    }

    /// Run work on a thread, with a line saying so and a way to stop it.
    fn work(&mut self, says: String, job: impl FnOnce(&AtomicBool) -> Msg + Send + 'static) {
        let stop = Arc::new(AtomicBool::new(false));
        self.busy = Some((says, stop.clone()));
        self.note = None;
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(job(&stop));
        });
    }

    fn extract(&mut self, into: PathBuf, only_selected: bool) {
        let Some(archive) = self.archive.clone() else { return };
        let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if only_selected {
            let paths: Vec<String> = self.selected.iter().cloned().collect();
            let count = paths.len();
            self.work(format!("Extracting from {name}…"), move |stop| match rivet_archive::extract_some(&archive, &paths, &into, stop) {
                Ok(()) => Msg::Done(format!("Extracted {} into {}.", if count == 1 { "1 item".to_string() } else { format!("{count} items") }, short(&into)), false, Some(into)),
                Err(e) => Msg::Done(trouble("Couldn't extract", &e), e != "Cancelled", None),
            });
        } else {
            self.work(format!("Extracting {name}…"), move |stop| match rivet_archive::extract(&archive, &into, stop, &rivet_archive::free_name) {
                Ok(made) => Msg::Done(format!("Extracted to {}.", short(&made)), false, Some(made)),
                Err(e) => Msg::Done(trouble("Couldn't extract", &e), e != "Cancelled", None),
            });
        }
    }

    /// Ask what to call the archive and what kind to make, before making it.
    fn ask_compress(&mut self, sources: Vec<PathBuf>) {
        if sources.is_empty() {
            return;
        }
        let format = Format::ALL.into_iter().find(|f| f.available()).unwrap_or(Format::Zip);
        self.making = Some(Making { name: rivet_archive::suggest_name(&sources), sources, format });
    }

    fn compress(&mut self, m: Making) {
        let dir = m.sources[0].parent().map(PathBuf::from).unwrap_or_default();
        let archive = rivet_archive::free_name(&dir, &dir.join(format!("{}{}", m.name.trim(), m.format.ext())));
        let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.work(format!("Making {name}…"), move |stop| match rivet_archive::compress(&m.sources, &archive, m.format, stop) {
            Ok(()) => Msg::Made(archive),
            Err(e) => Msg::Done(trouble("Couldn't make the archive", &e), e != "Cancelled", None),
        });
    }

    /// What the threads have reported since the last frame.
    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Listed(path, list) => {
                    if self.archive.as_ref() != Some(&path) {
                        continue;
                    }
                    self.loading = false;
                    match list {
                        Ok(entries) => {
                            self.entries = entries;
                            // One folder at the top holding everything: start inside it.
                            let top = children(&self.entries, "", self.sort);
                            if let [only] = top.as_slice()
                                && self.entries[*only].dir
                            {
                                self.folder = self.entries[*only].path.clone();
                            }
                        }
                        Err(e) => {
                            self.archive = None;
                            self.note = Some((trouble(&format!("Couldn't open {}", path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()), &e), true, None));
                        }
                    }
                    self.stale = true;
                }
                Msg::Done(says, bad, show) => {
                    self.busy = None;
                    self.note = Some((says, bad, show));
                }
                Msg::Made(archive) => {
                    self.busy = None;
                    self.open(archive.clone());
                    self.note = Some((format!("Made {}.", short(&archive)), false, archive.parent().map(PathBuf::from)));
                }
                Msg::Picked(ask, answer) => {
                    self.picking = None;
                    match (ask, answer) {
                        (_, Err(e)) => self.note = Some((format!("Couldn't open the file picker: {e}."), true, None)),
                        (_, Ok(None)) => {}
                        (Ask::Archive, Ok(Some(p))) => self.open(p),
                        (Ask::ExtractAll, Ok(Some(p))) => self.extract(p, false),
                        (Ask::ExtractSome, Ok(Some(p))) => self.extract(p, true),
                        (Ask::Folder, Ok(Some(p))) => self.ask_compress(vec![p]),
                    }
                }
            }
        }
        if self.busy.is_some() || self.loading || self.picking.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(120));
        }
        // Files dropped on the window: an archive opens, anything else is compressed.
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !dropped.is_empty() && self.busy.is_none() {
            match dropped.as_slice() {
                [one] if rivet_archive::is_archive(one) => self.open(one.clone()),
                _ => self.ask_compress(dropped),
            }
        }
    }

    fn go(&mut self, folder: String) {
        self.folder = folder;
        self.selected.clear();
        self.anchor = None;
        self.stale = true;
    }

    fn up(&mut self) {
        if !self.folder.is_empty() {
            let parent = self.folder.rsplit_once('/').map_or(String::new(), |(p, _)| p.to_string());
            self.go(parent);
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.making.is_some() || ctx.egui_wants_keyboard_input() {
            return;
        }
        let (open, all, up, clear) = ctx.input_mut(|i| (i.consume_key(egui::Modifiers::COMMAND, Key::O), i.consume_key(egui::Modifiers::COMMAND, Key::A), i.consume_key(egui::Modifiers::NONE, Key::Backspace) || i.consume_key(egui::Modifiers::ALT, Key::ArrowUp), i.consume_key(egui::Modifiers::NONE, Key::Escape)));
        if open {
            self.pick(Ask::Archive);
        }
        if all {
            self.selected = self.rows.iter().map(|i| self.entries[*i].path.clone()).collect();
        }
        if up {
            self.up();
        }
        if clear {
            self.selected.clear();
        }
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let has = self.archive.is_some() && !self.loading;
        let free = self.busy.is_none() && self.picking.is_none();
        ui.horizontal(|ui| {
            ui.spacing_mut().button_padding = Vec2::new(12.0, 6.0);
            if ui.add_enabled(free, egui::Button::new("Open…")).on_hover_text("Open an archive (Ctrl+O)").clicked() {
                self.pick(Ask::Archive);
            }
            if ui.add_enabled(free, egui::Button::new("Compress a Folder…")).on_hover_text("Make an archive of a folder. For loose files, drop them on this window.").clicked() {
                self.pick(Ask::Folder);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(has && free, egui::Button::new(egui::RichText::new("Extract All…").strong())).clicked() {
                    self.pick(Ask::ExtractAll);
                }
                let n = self.selected.len();
                if n > 0 && ui.add_enabled(has && free, egui::Button::new(if n == 1 { "Extract 1 Selected…".to_string() } else { format!("Extract {n} Selected…") })).clicked() {
                    self.pick(Ask::ExtractSome);
                }
            });
        });
    }

    /// Where you are: the archive's name, then each folder down to the one shown.
    fn path_bar(&mut self, ui: &mut egui::Ui) {
        let Some(archive) = &self.archive else { return };
        let v = ui.visuals().clone();
        let name = archive.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        let mut go = None;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let mut steps = vec![(name, String::new())];
            let mut so_far = String::new();
            for part in self.folder.split('/').filter(|p| !p.is_empty()) {
                so_far = if so_far.is_empty() { part.to_string() } else { format!("{so_far}/{part}") };
                steps.push((part.to_string(), so_far.clone()));
            }
            let last = steps.len() - 1;
            for (i, (label, path)) in steps.into_iter().enumerate() {
                if i > 0 {
                    ui.label(egui::RichText::new("›").color(v.weak_text_color()));
                }
                let text = egui::RichText::new(label).size(13.5);
                let text = if i == last { text.strong() } else { text.color(v.weak_text_color()) };
                if ui.add(egui::Button::new(text).frame(false)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && i != last {
                    go = Some(path);
                }
            }
        });
        if let Some(path) = go {
            self.go(path);
        }
    }

    fn list(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        if self.loading {
            centered(ui, "Reading the archive…", "");
            return;
        }
        if self.archive.is_none() {
            centered(ui, "No archive open", "Open one, or drop one here. Drop other files or a folder to compress them.");
            return;
        }
        if self.stale {
            self.rows = children(&self.entries, &self.folder, self.sort);
            self.stale = false;
        }
        const ROW: f32 = 30.0;
        let (name_w, size_w, when_w) = ((ui.available_width() - 290.0).max(160.0), 110.0, 150.0);
        // The headings: click one to sort by it, again to turn the order round.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            for (label, by, w) in [("Name", Sort::Name, name_w), ("Size", Sort::Size, size_w), ("Changed", Sort::Changed, when_w)] {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 24.0), Sense::click());
                let on = self.sort.0 == by;
                let col = if on { v.text_color() } else { v.weak_text_color() };
                let end = ui.painter().text(Pos2::new(r.left() + if by == Sort::Name { 42.0 } else { 8.0 }, r.center().y), Align2::LEFT_CENTER, label, egui::FontId::proportional(12.0), col).right();
                if on {
                    // Which way it runs, drawn: not every font has the arrows.
                    let (c, d) = (Pos2::new(end + 9.0, r.center().y), if self.sort.1 { -2.5 } else { 2.5 });
                    ui.painter().add(egui::Shape::line(vec![c + Vec2::new(-3.5, -d), c + Vec2::new(0.0, d), c + Vec2::new(3.5, -d)], Stroke::new(1.4, col)));
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.sort = if on { (by, !self.sort.1) } else { (by, true) };
                    self.stale = true;
                }
            }
        });
        let line = ui.available_rect_before_wrap();
        ui.painter().hline(line.x_range(), line.top(), Stroke::new(1.0, palette::mix(v.panel_fill, v.text_color(), 0.1)));
        if self.rows.is_empty() {
            centered(ui, "This folder is empty", "");
            return;
        }
        let folder_tex = self.icons.texture(ui.ctx(), &["folder"], (20.0 * ui.ctx().pixels_per_point()) as u32);
        let file_tex = self.icons.texture(ui.ctx(), &["text-x-generic", "application-x-generic", "unknown"], (20.0 * ui.ctx().pixels_per_point()) as u32);
        let mut enter = None;
        let rows = self.rows.clone();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW, rows.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for k in range {
                let e = &self.entries[rows[k]];
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click());
                let on = self.selected.contains(&e.path);
                if on {
                    ui.painter().rect_filled(r.shrink2(Vec2::new(0.0, 1.0)), r_sm(), palette::mix(v.panel_fill, accent, 0.26));
                } else if resp.hovered() {
                    ui.painter().rect_filled(r.shrink2(Vec2::new(0.0, 1.0)), r_sm(), palette::mix(v.panel_fill, v.text_color(), 0.06));
                }
                let p = ui.painter().with_clip_rect(r);
                let icon = Rect::from_center_size(Pos2::new(r.left() + 22.0, r.center().y), Vec2::splat(20.0));
                match if e.dir { &folder_tex } else { &file_tex } {
                    Some(t) => {
                        p.image(t.id(), icon, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    }
                    None if e.dir => {
                        p.rect_filled(icon.shrink(2.0), 3.0, accent);
                    }
                    None => {
                        p.rect_stroke(icon.shrink2(Vec2::new(4.0, 2.0)), 2.0, Stroke::new(1.4, v.weak_text_color()), egui::StrokeKind::Inside);
                    }
                }
                let col = if on { v.strong_text_color() } else { v.text_color() };
                let name = p.layout_no_wrap(e.name().to_string(), egui::FontId::proportional(13.5), col);
                p.with_clip_rect(Rect::from_min_max(r.min, Pos2::new(r.left() + name_w - 8.0, r.bottom()))).galley(Pos2::new(r.left() + 42.0, r.center().y - name.size().y / 2.0), name, col);
                let quiet = v.weak_text_color();
                let size = if e.dir {
                    let (n, _) = inside(&self.entries, &e.path);
                    if n == 1 { "1 file".to_string() } else { format!("{n} files") }
                } else {
                    human_size(e.size)
                };
                p.text(Pos2::new(r.left() + name_w + 8.0, r.center().y), Align2::LEFT_CENTER, size, egui::FontId::proportional(12.5), quiet);
                p.text(Pos2::new(r.left() + name_w + size_w + 8.0, r.center().y), Align2::LEFT_CENTER, when(e.modified), egui::FontId::proportional(12.5), quiet);
                if resp.double_clicked() && e.dir {
                    enter = Some(e.path.clone());
                } else if resp.clicked() {
                    let (ctrl, shift) = ui.input(|i| (i.modifiers.command, i.modifiers.shift));
                    let path = e.path.clone();
                    if shift && let Some(from) = self.anchor.as_ref().and_then(|a| rows.iter().position(|i| self.entries[*i].path == *a)) {
                        let (lo, hi) = (from.min(k), from.max(k));
                        self.selected = rows[lo..=hi].iter().map(|i| self.entries[*i].path.clone()).collect();
                    } else if ctrl {
                        if !self.selected.remove(&path) {
                            self.selected.insert(path.clone());
                        }
                        self.anchor = Some(path);
                    } else {
                        self.selected = BTreeSet::from([path.clone()]);
                        self.anchor = Some(path);
                    }
                }
            }
        });
        if let Some(path) = enter {
            self.go(path);
        }
    }

    fn bottom_bar(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.set_min_height(24.0);
            if let Some((says, stop)) = &self.busy {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(says);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("Stop").clicked() {
                        stop.store(true, Ordering::Relaxed);
                    }
                });
                return;
            }
            if let Some((says, bad, show)) = self.note.clone() {
                let col = if bad { rivet_look::theme::status(ui.ctx()).danger } else { v.text_color() };
                ui.add(egui::Label::new(egui::RichText::new(says).color(col)).truncate());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(p) = show
                        && ui.button("Show in Files").clicked()
                        && self.live()
                    {
                        show_in_files(&p);
                    }
                });
                return;
            }
            if self.archive.is_none() || self.loading {
                return;
            }
            // What is selected, else what the whole archive holds.
            let (files, bytes) = if self.selected.is_empty() {
                self.entries.iter().filter(|e| !e.dir).fold((0usize, 0u64), |(n, s), e| (n + 1, s + e.size))
            } else {
                self.selected.iter().filter_map(|p| self.entries.iter().find(|e| e.path == *p)).fold((0, 0), |(n, s), e| if e.dir { (n + inside(&self.entries, &e.path).0, s + inside(&self.entries, &e.path).1) } else { (n + 1, s + e.size) })
            };
            let what = if files == 1 { "1 file".to_string() } else { format!("{files} files") };
            let text = if self.selected.is_empty() { format!("{what}, {} unpacked", human_size(bytes)) } else { format!("{what} selected, {}", human_size(bytes)) };
            ui.label(egui::RichText::new(text).color(v.weak_text_color()).size(12.5));
        });
    }

    /// The question asked before an archive is made: its name and its kind.
    fn making_window(&mut self, ctx: &egui::Context) {
        let Some(mut m) = self.making.take() else { return };
        let v = ctx.global_style().visuals.clone();
        let (mut make, mut cancel) = (false, false);
        egui::Window::new("New Archive").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).frame(egui::Frame::window(&ctx.global_style()).inner_margin(18)).show(ctx, |ui| {
            ui.set_width(400.0);
            let what = match m.sources.as_slice() {
                [one] => one.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
                many => format!("{} items", many.len()),
            };
            ui.label(egui::RichText::new(format!("Compress {what}")).color(v.weak_text_color()));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut m.name).desired_width(280.0));
                ui.label(egui::RichText::new(m.format.ext()).color(v.weak_text_color()));
            });
            ui.add_space(10.0);
            ui.horizontal_wrapped(|ui| {
                for f in Format::ALL {
                    let on = m.format == f;
                    if ui.add_enabled(f.available(), egui::Button::selectable(on, f.ext().trim_start_matches('.'))).on_disabled_hover_text("The tool that makes these isn't installed.").clicked() {
                        m.format = f;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(m.format.hint()).size(12.0).color(v.weak_text_color()));
            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = !m.name.trim().is_empty() && !m.name.contains('/') && m.format.available();
                    if ui.add_enabled(ok, egui::Button::new(egui::RichText::new("Make Archive").strong())).clicked() || (ok && ui.input(|i| i.key_pressed(Key::Enter))) {
                        make = true;
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                        cancel = true;
                    }
                });
            });
        });
        if make && self.live() {
            self.compress(m);
        } else if !cancel {
            self.making = Some(m);
        }
    }
}

/// A path as a person says it: from the home folder where it is under it.
fn short(p: &Path) -> String {
    match std::env::var_os("HOME").map(PathBuf::from).and_then(|h| p.strip_prefix(h).ok().map(PathBuf::from)) {
        Some(rest) if !rest.as_os_str().is_empty() => rest.display().to_string(),
        _ => p.display().to_string(),
    }
}

/// What went wrong as a sentence, or that it was stopped.
fn trouble(what: &str, e: &str) -> String {
    if e == "Cancelled" { "Stopped.".to_string() } else { format!("{what}: {}.", e.trim_end_matches('.')) }
}

/// Open the file manager on what was made: its folder, with it selected where the file
/// manager can do that.
fn show_in_files(p: &Path) {
    let folder = if p.is_dir() { p } else { p.parent().unwrap_or(p) };
    let _ = std::process::Command::new("xdg-open").arg(folder).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

/// A line in the middle of the empty space, with a quieter one under it.
fn centered(ui: &mut egui::Ui, title: &str, under: &str) {
    let v = ui.visuals().clone();
    let r = ui.available_rect_before_wrap();
    ui.painter().text(r.center() - Vec2::new(0.0, 12.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(17.0), v.text_color());
    let g = ui.painter().layout(under.to_string(), egui::FontId::proportional(13.0), v.weak_text_color(), (r.width() - 60.0).max(120.0));
    ui.painter().galley(Pos2::new(r.center().x - g.size().x / 2.0, r.center().y + 8.0), g, v.weak_text_color());
    ui.allocate_rect(r, Sense::hover());
}

impl eframe::App for ViceApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_VICE", shot.frames(), raw);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let choice = Choice { mode: Mode::System, accent: [0x35, 0x84, 0xe4], follow_desktop_colors: true, text_size: 0.0 };
        self.look.frame(&ctx, &choice, self.live());
        self.poll(&ctx);
        self.shortcuts(&ctx);
        let v = ctx.global_style().visuals.clone();
        let accent = self.look.accent(&choice);
        self.icons.set_builtin(Some(rivet_look::builtin_icons::BuiltinColors { accent, bg: v.extreme_bg_color, fg: v.text_color(), dark: v.dark_mode }));
        let bar = |top: i8, bottom: i8| egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top, bottom });
        egui::Panel::top("vice-top").show_separator_line(false).frame(bar(10, 6)).show(ui, |ui| {
            self.top_bar(ui);
            if self.archive.is_some() {
                ui.add_space(6.0);
                self.path_bar(ui);
            }
        });
        egui::Panel::bottom("vice-bottom").show_separator_line(false).frame(bar(6, 8)).show(ui, |ui| self.bottom_bar(ui));
        let middle = egui::Frame::new().fill(palette::mix(v.panel_fill, v.extreme_bg_color, 0.5)).inner_margin(egui::Margin::symmetric(8, 4)).corner_radius(r_md());
        egui::CentralPanel::default().frame(middle).show(ui, |ui| self.list(ui, accent));
        self.making_window(&ctx);
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_times() {
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(999), "999 bytes");
        assert_eq!(human_size(4_211), "4.2 kB");
        assert_eq!(human_size(3_412_880), "3.4 MB");
        assert_eq!(human_size(131_902), "132 kB");
        assert_eq!(when(1795479022), "2026-11-24 00:10");
        assert_eq!(when(86401), "1970-01-02 00:00");
        assert_eq!(when(0), "");
    }

    #[test]
    fn a_folders_rows() {
        let all = demo();
        let name = |i: &usize| all[*i].name().to_string();
        // Folders first, then by name.
        assert_eq!(children(&all, "Trip", (Sort::Name, true)).iter().map(name).collect::<Vec<_>>(), ["Photos", "Receipts", "itinerary.md", "packing list.txt", "route.gpx"]);
        // Turned round, the biggest file comes first; folders stay on top, in turned-round order too.
        assert_eq!(children(&all, "Trip", (Sort::Size, false)).iter().map(name).collect::<Vec<_>>(), ["Receipts", "Photos", "route.gpx", "itinerary.md", "packing list.txt"]);
        assert_eq!(children(&all, "", (Sort::Name, true)).iter().map(name).collect::<Vec<_>>(), ["Trip"]);
        assert_eq!(inside(&all, "Trip/Receipts"), (2, 88_410 + 131_902));
        assert_eq!(inside(&all, "Trip").0, 8);
    }
}
