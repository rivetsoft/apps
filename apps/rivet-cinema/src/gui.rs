//! Rivet Cinema's window. A video fills it, with the controls over the bottom that hide while it
//! plays; an audio file gets the cover, its details and the queue. mpv plays on its own
//! threads, and everything slow (reading folders, covers, seek previews, the portal) runs on
//! threads that report back, so a frame only reads what they said.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align2, Color32, CursorIcon, Key, Modifiers, Pos2, Rect, Sense, Vec2, ViewportCommand};
use rivet_look::live::{Choice, LiveLook};
use rivet_look::palette;
use rivet_look::theme::{Mode, r_lg, r_md, r_sm};
use rivet_media::mpv::{self, Display, Event, Node, Player, ProcAddress, Track};
use rivet_media::preview::duration_text;
use rivet_xdg::mime::MimeDb;
use rivet_xdg::portal::{IdleInhibit, Pick};

use crate::glyphs::{self, G};
use crate::mpris::{Cmd, Mpris, Now, Status};
use crate::previews::Previews;
use crate::queue::{self, Queue, Repeat};
use crate::resume::{self, Positions};
use crate::settings::Settings;
use crate::video::Screen;

pub const ICON_SVG: &str = include_str!("../data/rivet-cinema.svg");

const SPEEDS: [f64; 10] = [0.25, 0.5, 0.75, 1.0, 1.25, 1.5, 1.75, 2.0, 3.0, 4.0];
/// Seconds without the pointer moving before a video's controls hide.
const HIDE_AFTER: f64 = 2.0;
const VIDEO_SIZE: [f32; 2] = [1100.0, 680.0];
const AUDIO_SIZE: [f32; 2] = [440.0, 660.0];

/// The desktop's file types, read once, on whichever thread first needs them.
fn mime_db() -> &'static MimeDb {
    static DB: OnceLock<MimeDb> = OnceLock::new();
    DB.get_or_init(MimeDb::load)
}

/// What a thread reports back.
enum Msg {
    /// The files to play among what was opened, and whether to play the first now.
    Expanded(Vec<PathBuf>, bool),
    Picked(Ask, Result<Option<PathBuf>, String>),
    Inhibit(Option<IdleInhibit>),
    /// A cover for a file in the queue, as a picture file (for the desktop's player controls).
    Art(PathBuf, Option<PathBuf>),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Ask {
    File,
    Folder,
    Subtitles,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Empty,
    Video,
    Audio,
}

/// What mpv last said about the file playing.
#[derive(Clone, Default)]
struct State {
    pos: f64,
    length: f64,
    paused: bool,
    volume: f64,
    muted: bool,
    speed: f64,
    tracks: Vec<Track>,
    meta: Vec<(String, String)>,
    size: (i64, i64),
    sub_delay: f64,
    buffering: bool,
    /// Chapter starts and names.
    chapters: Vec<(f64, String)>,
    loaded: bool,
}

impl State {
    fn meta(&self, key: &str) -> Option<&str> {
        self.meta.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
    }
}

pub struct PlayApp {
    ctx: egui::Context,
    look: LiveLook,
    shot: Option<rivet_kit::Shot>,
    settings: Settings,
    queue: Queue,
    positions: Positions,
    player: Option<Player>,
    /// Why nothing can play (libmpv missing, or mpv wouldn't start).
    missing: Option<String>,
    screen: Screen,
    proc: Option<ProcAddress>,
    display: Display,
    st: State,
    /// The resume key of the file playing.
    key: Option<String>,
    /// The queue played to its end; Play starts the last file again.
    ended: bool,
    /// The file playing has no picture of its own (a cover at most).
    audio_mode: bool,
    /// The window size last set for a mode, so it is set once per change.
    sized_for: Option<bool>,
    show_queue: bool,
    show_prefs: bool,
    show_info: bool,
    on_top: bool,
    /// When the pointer last moved or a key was pressed (egui's clock).
    active_at: f64,
    over_controls: bool,
    /// A short line over the picture, until a time.
    note: Option<(String, f64)>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    cmds: Receiver<Cmd>,
    mpris: Option<Mpris>,
    seeked: bool,
    inhibit: Option<IdleInhibit>,
    inhibit_wanted: bool,
    previews: Previews,
    /// The picture-cover file for the desktop, per file.
    art: Option<(PathBuf, PathBuf)>,
    picking: bool,
    title: String,
    saved_at: f64,
    /// Dragging a row of the queue: from where.
    drag_row: Option<usize>,
    /// Dragging along the seek bar: the last place sought.
    seek_drag: Option<f64>,
    /// Screenshots: a still frame for the picture, a cover, and a place hovered on the seek bar.
    still: Option<egui::TextureHandle>,
    cover: Option<egui::TextureHandle>,
    hover_at: Option<f64>,
}

fn name_of(p: &Path) -> String {
    p.file_stem().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

fn speed_text(s: f64) -> String {
    if (s - s.round()).abs() < 0.001 { format!("{}×", s.round() as i64) } else { format!("{s}×") }
}

/// Seconds since 1970, for when a position was saved.
fn unix_now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A language code as its name, for the common ones; else the code as it is.
fn language(code: &str) -> String {
    let name = match code.to_lowercase().as_str() {
        "en" | "eng" => "English",
        "fr" | "fre" | "fra" => "French",
        "de" | "ger" | "deu" => "German",
        "es" | "spa" => "Spanish",
        "it" | "ita" => "Italian",
        "pt" | "por" => "Portuguese",
        "nl" | "dut" | "nld" => "Dutch",
        "sv" | "swe" => "Swedish",
        "pl" | "pol" => "Polish",
        "ru" | "rus" => "Russian",
        "ja" | "jpn" => "Japanese",
        "zh" | "chi" | "zho" => "Chinese",
        "ko" | "kor" => "Korean",
        "ar" | "ara" => "Arabic",
        "hi" | "hin" => "Hindi",
        _ => return code.to_string(),
    };
    name.to_string()
}

/// A track as a menu shows it: "Commentary (English)", "English", or "Track 2".
fn track_label(t: &Track) -> String {
    let lang = t.lang.as_deref().map(language);
    let mut s = match (&t.title, &lang) {
        (Some(title), Some(l)) if !title.contains(l.as_str()) => format!("{title} ({l})"),
        (Some(title), _) => title.clone(),
        (None, Some(l)) => l.clone(),
        (None, None) => format!("Track {}", t.id),
    };
    if t.forced {
        s.push_str(", forced");
    }
    s
}

/// A cover picture for a music file: one beside it, else the one inside it (written out for
/// the desktop to read). Slow: run on a thread.
fn find_cover(file: &Path, key: &str) -> Option<PathBuf> {
    let dir = file.parent()?;
    for name in ["cover", "Cover", "folder", "Folder", "front", "Front", "album", "AlbumArt"] {
        for ext in ["jpg", "jpeg", "png", "webp"] {
            let p = dir.join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let out = rivet_xdg::places::cache_home().join("rivet-cinema/art").join(format!("{key}.png"));
    if out.is_file() {
        return Some(out);
    }
    let img = rivet_media::preview::frame_at(file, 0.0, 512)?;
    std::fs::create_dir_all(out.parent()?).ok()?;
    rivet_kit::save_png(&out, &img).ok()?;
    Some(out)
}

/// mpv's options, from the settings.
fn options(s: &Settings) -> Vec<(&'static str, String)> {
    vec![
        ("vo", "libmpv".into()),
        ("hwdec", if s.hardware { "auto-safe" } else { "no" }.into()),
        ("config", "no".into()),
        ("terminal", "no".into()),
        ("input-default-bindings", "no".into()),
        ("input-vo-keyboard", "no".into()),
        ("osc", "no".into()),
        ("osd-level", "0".into()),
        ("ytdl", "no".into()),
        ("idle", "yes".into()),
        ("keep-open", "no".into()),
        ("audio-client-name", "rivet-cinema".into()),
        ("title", "Rivet Cinema".into()),
        ("sub-auto", "fuzzy".into()),
        ("cover-art-auto", "exact".into()),
        ("volume", format!("{}", s.volume.round())),
        ("mute", if s.muted { "yes" } else { "no" }.into()),
        ("alang", s.audio_lang.clone()),
        ("slang", s.sub_lang.clone()),
        ("sub-scale", format!("{}", s.sub_scale)),
        ("loop-file", if s.repeat == Repeat::One { "inf" } else { "no" }.into()),
    ]
}

impl PlayApp {
    pub fn new(cc: &eframe::CreationContext, files: Vec<PathBuf>, screenshot: Option<PathBuf>) -> PlayApp {
        use raw_window_handle::{HasDisplayHandle, RawDisplayHandle};
        let display = match cc.display_handle().map(|h| h.as_raw()) {
            Ok(RawDisplayHandle::Wayland(h)) => Display::Wayland(h.display.as_ptr()),
            Ok(RawDisplayHandle::Xlib(h)) => h.display.map_or(Display::Unknown, |d| Display::X11(d.as_ptr())),
            _ => Display::Unknown,
        };
        let (tx, rx) = channel();
        let (cmd_tx, cmds) = channel();
        let shot = screenshot.map(rivet_kit::Shot::new);
        let live = shot.is_none();
        let settings = if live { Settings::load() } else { Settings::default() };
        let mut queue = Queue::default();
        queue.repeat = settings.repeat;
        let mut app = PlayApp {
            ctx: cc.egui_ctx.clone(),
            look: LiveLook::new(),
            shot,
            queue,
            positions: if live { Positions::load() } else { Positions::default() },
            player: None,
            missing: None,
            screen: Screen::default(),
            proc: cc.get_proc_address.clone(),
            display,
            st: State { volume: settings.volume, muted: settings.muted, speed: 1.0, ..State::default() },
            settings,
            key: None,
            ended: false,
            audio_mode: false,
            sized_for: None,
            show_queue: false,
            show_prefs: false,
            show_info: false,
            on_top: false,
            active_at: 0.0,
            over_controls: false,
            note: None,
            tx,
            rx,
            cmds,
            mpris: None,
            seeked: false,
            inhibit: None,
            inhibit_wanted: false,
            previews: Previews::default(),
            art: None,
            picking: false,
            title: String::new(),
            saved_at: 0.0,
            drag_row: None,
            seek_drag: None,
            still: None,
            cover: None,
            hover_at: None,
        };
        if !live {
            app.scene();
            return app;
        }
        let ctx = cc.egui_ctx.clone();
        let opts = options(&app.settings);
        let opts: Vec<(&str, &str)> = opts.iter().map(|(k, v)| (*k, v.as_str())).collect();
        match Player::new(&opts, move || ctx.request_repaint()) {
            Ok(p) => app.player = Some(p),
            Err(e) => app.missing = Some(e),
        }
        let ctx = cc.egui_ctx.clone();
        match Mpris::start(cmd_tx, move || ctx.request_repaint()) {
            Ok(m) => app.mpris = Some(m),
            Err(e) => eprintln!("rivet-cinema: the desktop's player controls won't reach Rivet Cinema: {e}"),
        }
        if app.settings.shuffle {
            app.queue.set_shuffle(true, unix_now());
        }
        if !files.is_empty() {
            app.open(files, true);
        }
        app
    }

    fn live(&self) -> bool {
        self.shot.is_none()
    }

    /// Screenshots: a made-up moment in the sample media, picked by RIVET_CINEMA_SCENE: "video"
    /// (the default), "seek" (a frame over the seek bar), "queue" (the queue beside the video),
    /// "audio" (a music file), "prefs" (Preferences), "empty" (nothing open) and "missing"
    /// (libmpv isn't installed). RIVET_CINEMA_SAMPLE is the folder of sample media
    /// (utilities/lib/cinema-samples.sh makes it).
    fn scene(&mut self) {
        let scene = std::env::var("RIVET_CINEMA_SCENE").unwrap_or_default();
        if scene == "missing" {
            self.missing = Some(mpv::MISSING.into());
            return;
        }
        if scene == "empty" {
            return;
        }
        let sample = std::env::var_os("RIVET_CINEMA_SAMPLE").map(PathBuf::from).unwrap_or_default();
        let files = queue::expand(&[sample], mime_db());
        let kind = |p: &PathBuf| mime_db().guess_by_name(p).unwrap_or_default();
        let video = files.iter().position(|p| p.file_stem().is_some_and(|s| s == "Mandelbrot Zoom")).or_else(|| files.iter().position(|p| kind(p).starts_with("video/")));
        let audio = files.iter().position(|p| kind(p).starts_with("audio/"));
        self.queue.add(files.clone());
        let t = |id: i64, kind: &str, title: Option<&str>, lang: Option<&str>, selected: bool| Track { id, kind: kind.into(), title: title.map(String::from), lang: lang.map(String::from), selected, ..Track::default() };
        self.st.volume = 80.0;
        if scene == "audio" {
            let Some(i) = audio else { return };
            self.queue.set_current(i);
            self.audio_mode = true;
            self.st.length = 154.0;
            self.st.pos = 61.0;
            self.st.loaded = true;
            self.st.tracks = vec![t(1, "audio", None, None, true), Track { albumart: true, ..t(1, "video", None, None, true) }];
            self.st.meta = vec![("title".into(), "Low Tide".into()), ("artist".into(), "Rivet Sample Band".into()), ("album".into(), "Tide Tables".into())];
            self.cover = rivet_media::preview::frame_at(&files[i], 0.0, 720).map(|img| self.ctx.load_texture("cover", img, egui::TextureOptions::LINEAR));
            return;
        }
        let Some(i) = video else { return };
        self.queue.set_current(i);
        self.st.length = 120.0;
        self.st.pos = 47.0;
        self.st.loaded = true;
        self.st.tracks = vec![t(1, "video", None, None, true), t(1, "audio", Some("English"), Some("eng"), true), t(2, "audio", Some("Français"), Some("fre"), false), t(1, "sub", None, Some("eng"), true), t(2, "sub", None, Some("fre"), false)];
        self.st.chapters = vec![(0.0, "Wide".into()), (40.0, "Closer".into()), (85.0, "Deep".into())];
        self.still = rivet_media::preview::frame_at(&files[i], 47.0, 1920).map(|img| self.ctx.load_texture("still", img, egui::TextureOptions::LINEAR));
        match scene.as_str() {
            "seek" => {
                self.hover_at = Some(92.0);
                let (_, at) = crate::previews::bucket(92.0, 120.0);
                if let Some(img) = rivet_media::preview::frame_at(&files[i], at, crate::previews::WIDTH) {
                    let tex = self.ctx.load_texture("seek", img, egui::TextureOptions::LINEAR);
                    self.previews.insert(&files[i], 92.0, 120.0, tex);
                }
            }
            "queue" => self.show_queue = true,
            "prefs" => self.show_prefs = true,
            _ => {}
        }
    }

    fn note(&mut self, text: impl Into<String>) {
        let until = self.ctx.input(|i| i.time) + 4.0;
        self.note = Some((text.into(), until));
    }

    fn is_video(&self) -> bool {
        self.st.tracks.iter().any(|t| t.kind == "video" && !t.albumart)
    }

    fn has_cover(&self) -> bool {
        self.st.tracks.iter().any(|t| t.kind == "video" && t.albumart)
    }

    fn view(&self) -> View {
        if self.queue.current().is_none() || self.missing.is_some() {
            View::Empty
        } else if self.audio_mode {
            View::Audio
        } else {
            View::Video
        }
    }

    fn playing(&self) -> bool {
        self.queue.current().is_some() && !self.ended && !self.st.paused
    }

    // ── What to play ─────────────────────────────────────────────────────────────

    /// Add files, folders or playlists to the queue (read on a thread).
    fn open(&mut self, paths: Vec<PathBuf>, play: bool) {
        if !self.live() || paths.is_empty() {
            return;
        }
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let list = queue::expand(&paths, mime_db());
            let _ = tx.send(Msg::Expanded(list, play));
            ctx.request_repaint();
        });
    }

    fn pick(&mut self, ask: Ask) {
        if self.picking || !self.live() {
            return;
        }
        self.picking = true;
        let start = self.queue.current_path().and_then(Path::parent).map(PathBuf::from);
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        std::thread::spawn(move || {
            let (kind, title) = match ask {
                Ask::File => (Pick::Media, "Open a Video or Music File"),
                Ask::Folder => (Pick::Folder, "Open a Folder"),
                Ask::Subtitles => (Pick::Subtitles, "Add a Subtitle File"),
            };
            let _ = tx.send(Msg::Picked(ask, rivet_xdg::portal::choose(kind, title, start.as_deref())));
            ctx.request_repaint();
        });
    }

    /// Where the file playing is, kept for next time.
    fn remember_position(&mut self) {
        if let Some(key) = &self.key
            && self.st.loaded
            && self.settings.resume
        {
            self.positions.remember(key, self.st.pos, self.st.length, unix_now());
            self.positions.save();
        }
    }

    fn play_index(&mut self, i: usize) {
        self.remember_position();
        self.queue.set_current(i);
        let Some(path) = self.queue.current_path().map(Path::to_path_buf) else { return };
        self.key = std::fs::metadata(&path).ok().map(|m| resume::key(&path, m.len()));
        self.st = State { volume: self.st.volume, muted: self.st.muted, speed: self.st.speed, ..State::default() };
        self.ended = false;
        let loaded = self.player.as_ref().map(|p| {
            let r = p.command(&["loadfile", &path.to_string_lossy(), "replace"]);
            if r.is_ok() {
                p.set_flag("pause", false);
            }
            r
        });
        if let Some(Err(e)) = loaded {
            self.note(format!("Couldn't play {}: {e}.", name_of(&path)));
            return;
        }
        self.seeked = true;
    }

    fn next(&mut self, by_hand: bool) {
        match self.queue.next(by_hand) {
            Some(i) => self.play_index(i),
            None => {
                self.remember_position();
                self.ended = true;
                if let Some(p) = &self.player {
                    let _ = p.command(&["stop"]);
                }
            }
        }
    }

    fn previous(&mut self) {
        if self.st.pos > 3.0 {
            self.seek_to(0.0, true);
            return;
        }
        match self.queue.previous() {
            Some(i) => self.play_index(i),
            None => self.seek_to(0.0, true),
        }
    }

    fn toggle_pause(&mut self) {
        if self.ended || !self.st.loaded {
            if let Some(i) = self.queue.current().or((!self.queue.is_empty()).then_some(0)) {
                self.play_index(i);
            }
            return;
        }
        self.set_pause(!self.st.paused);
    }

    fn set_pause(&mut self, on: bool) {
        if let Some(p) = &self.player {
            p.set_flag("pause", on);
        }
        self.st.paused = on;
        if on {
            self.remember_position();
        }
    }

    fn seek_to(&mut self, secs: f64, exact: bool) {
        if let Some(p) = &self.player {
            let _ = p.command(&["seek", &format!("{:.3}", secs.max(0.0)), if exact { "absolute+exact" } else { "absolute+keyframes" }]);
        }
        self.st.pos = secs.max(0.0);
    }

    fn seek_by(&mut self, secs: f64) {
        if let Some(p) = &self.player {
            let _ = p.command(&["seek", &format!("{secs:.3}"), "relative+exact"]);
        }
    }

    fn set_volume(&mut self, v: f64) {
        let v = v.clamp(0.0, 100.0);
        if let Some(p) = &self.player {
            p.set_double("volume", v);
            if v > 0.0 && self.st.muted {
                p.set_flag("mute", false);
            }
        }
        self.st.volume = v;
        self.settings.volume = v;
    }

    fn set_muted(&mut self, on: bool) {
        if let Some(p) = &self.player {
            p.set_flag("mute", on);
        }
        self.st.muted = on;
        self.settings.muted = on;
    }

    fn set_speed(&mut self, s: f64) {
        let s = s.clamp(0.25, 4.0);
        if let Some(p) = &self.player {
            p.set_double("speed", s);
        }
        self.st.speed = s;
        self.note(format!("Speed {}", speed_text(s)));
    }

    fn step_speed(&mut self, up: bool) {
        let s = self.st.speed;
        let next = if up { SPEEDS.into_iter().find(|x| *x > s + 0.001) } else { SPEEDS.into_iter().rev().find(|x| *x < s - 0.001) };
        if let Some(n) = next {
            self.set_speed(n);
        }
    }

    fn set_repeat(&mut self, r: Repeat) {
        self.queue.repeat = r;
        self.settings.repeat = r;
        if let Some(p) = &self.player {
            p.set_str("loop-file", if r == Repeat::One { "inf" } else { "no" });
        }
    }

    fn set_shuffle(&mut self, on: bool) {
        self.queue.set_shuffle(on, unix_now());
        self.settings.shuffle = on;
    }

    fn select_track(&mut self, kind: &str, id: Option<i64>) {
        let prop = match kind {
            "audio" => "aid",
            "sub" => "sid",
            _ => "vid",
        };
        if let Some(p) = &self.player {
            p.set_str(prop, &id.map_or("no".to_string(), |i| i.to_string()));
        }
    }

    /// The next track of a kind, round to Off for subtitles.
    fn cycle_track(&mut self, kind: &str) {
        let list: Vec<&Track> = self.st.tracks.iter().filter(|t| t.kind == kind).collect();
        if list.is_empty() {
            return;
        }
        let at = list.iter().position(|t| t.selected);
        let next = match at {
            Some(i) if i + 1 < list.len() => Some(list[i + 1].id),
            Some(_) if kind == "sub" => None,
            _ => Some(list[0].id),
        };
        let label = next.and_then(|id| list.iter().find(|t| t.id == id)).map(|t| track_label(t)).unwrap_or_else(|| "Off".into());
        self.select_track(kind, next);
        self.note(if kind == "sub" { format!("Subtitles: {label}") } else { format!("Audio: {label}") });
    }

    fn shift_subtitles(&mut self, by: f64) {
        let d = ((self.st.sub_delay + by) * 10.0).round() / 10.0;
        if let Some(p) = &self.player {
            p.set_double("sub-delay", d);
        }
        self.st.sub_delay = d;
        self.note(format!("Subtitle delay {d:+.1} s"));
    }

    fn fullscreen(&self) -> bool {
        self.ctx.input(|i| i.viewport().fullscreen.unwrap_or(false))
    }

    fn set_fullscreen(&mut self, on: bool) {
        self.ctx.send_viewport_cmd(ViewportCommand::Fullscreen(on));
    }

    // ── What the threads and mpv said ────────────────────────────────────────────

    fn poll(&mut self, ctx: &egui::Context) {
        let events: Vec<Event> = self.player.as_ref().map(|p| p.events.try_iter().collect()).unwrap_or_default();
        for e in events {
            self.event(e);
        }
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Expanded(list, play) => {
                    if list.is_empty() {
                        self.note("Nothing to play there.");
                        continue;
                    }
                    let first = self.queue.add(list);
                    if play || self.queue.current().is_none() {
                        self.play_index(first);
                    }
                }
                Msg::Picked(ask, answer) => {
                    self.picking = false;
                    match (ask, answer) {
                        (_, Err(e)) => self.note(format!("Couldn't open the file picker: {e}.")),
                        (_, Ok(None)) => {}
                        (Ask::Subtitles, Ok(Some(p))) => {
                            if let Some(Err(e)) = self.player.as_ref().map(|pl| pl.command(&["sub-add", &p.to_string_lossy(), "select"])) {
                                self.note(format!("Couldn't load those subtitles: {e}."));
                            }
                        }
                        (_, Ok(Some(p))) => self.open(vec![p], true),
                    }
                }
                // Kept only if the screen should still stay awake; dropping it lets it sleep.
                Msg::Inhibit(i) => {
                    if self.inhibit_wanted {
                        self.inhibit = i;
                    }
                }
                Msg::Art(file, art) => {
                    if self.queue.current_path() == Some(file.as_path()) {
                        self.art = art.map(|a| (file, a));
                    }
                }
            }
        }
        let cmds: Vec<Cmd> = self.cmds.try_iter().collect();
        for c in cmds {
            self.command(c, ctx);
        }
    }

    /// Files dropped on the window. Read in `ui` only: while the window is hidden, `logic` sees
    /// the last shown frame's input again and would open them over and over.
    fn dropped(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).filter(|p| !p.as_os_str().is_empty()).collect::<Vec<_>>());
        if !dropped.is_empty() {
            let play = self.queue.is_empty() || self.ended;
            self.open(dropped, play);
        }
    }

    fn event(&mut self, e: Event) {
        match e {
            Event::Changed(name, v) => match name {
                "time-pos" => {
                    if self.seek_drag.is_none() {
                        self.st.pos = v.double().unwrap_or(0.0);
                    }
                }
                "duration" => self.st.length = v.double().unwrap_or(0.0),
                "pause" => self.st.paused = v.flag().unwrap_or(false),
                "volume" => self.st.volume = v.double().unwrap_or(self.st.volume),
                "mute" => self.st.muted = v.flag().unwrap_or(false),
                "speed" => self.st.speed = v.double().unwrap_or(1.0),
                "track-list" => {
                    self.st.tracks = mpv::tracks(&v);
                    if self.st.tracks.iter().any(|t| t.kind == "audio" || t.kind == "video") {
                        self.audio_mode = !self.is_video();
                    }
                }
                "metadata" => self.st.meta = mpv::metadata(&v),
                "dwidth" => self.st.size.0 = v.int().unwrap_or(0),
                "dheight" => self.st.size.1 = v.int().unwrap_or(0),
                "sub-delay" => self.st.sub_delay = v.double().unwrap_or(0.0),
                "paused-for-cache" => self.st.buffering = v.flag().unwrap_or(false),
                "chapter-list" => {
                    let Node::List(list) = &v else { return };
                    self.st.chapters = list.iter().filter_map(|c| Some((c.get("time")?.double()?, c.get("title").and_then(Node::str).unwrap_or_default().to_string()))).collect();
                }
                _ => {}
            },
            Event::StartFile => self.st.loaded = false,
            Event::FileLoaded => {
                self.st.loaded = true;
                let length = self.player.as_ref().and_then(|p| p.get_double("duration")).unwrap_or(self.st.length);
                if self.settings.resume
                    && let Some(at) = self.key.as_ref().and_then(|k| self.positions.get(k))
                    && resume::worth_keeping(at, length)
                {
                    self.seek_to(at, true);
                    self.note(format!("Started from {}. Press Home to start over.", duration_text(at)));
                }
                if let (Some(file), Some(key)) = (self.queue.current_path().map(Path::to_path_buf), self.key.clone()) {
                    let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
                    std::thread::spawn(move || {
                        let art = find_cover(&file, &key);
                        let _ = tx.send(Msg::Art(file, art));
                        ctx.request_repaint();
                    });
                }
            }
            Event::EndFile { finished, error } => {
                if let Some(e) = error {
                    let name = self.queue.current_path().map(name_of).unwrap_or_default();
                    self.note(format!("Couldn't play {name}: {e}."));
                }
                if finished {
                    if let Some(k) = self.key.clone() {
                        self.positions.forget(&k);
                        self.positions.save();
                    }
                    self.st.loaded = false;
                    self.next(false);
                } else if !self.ended && self.st.loaded {
                    // Stopped for an error mid-way: go on with the queue.
                    self.st.loaded = false;
                }
            }
            Event::Restarted => self.seeked = true,
            Event::Seeking | Event::Shutdown => {}
        }
    }

    fn command(&mut self, c: Cmd, ctx: &egui::Context) {
        match c {
            Cmd::Raise => ctx.send_viewport_cmd(ViewportCommand::Focus),
            Cmd::Quit => ctx.send_viewport_cmd(ViewportCommand::Close),
            Cmd::Play => {
                if self.ended || !self.st.loaded || self.st.paused {
                    self.toggle_pause();
                }
            }
            Cmd::Pause => self.set_pause(true),
            Cmd::PlayPause => self.toggle_pause(),
            Cmd::Stop => {
                self.remember_position();
                if let Some(p) = &self.player {
                    let _ = p.command(&["stop"]);
                }
                self.ended = true;
            }
            Cmd::Next => self.next(true),
            Cmd::Previous => self.previous(),
            Cmd::Seek(by) => self.seek_by(by),
            Cmd::SetPosition(at) => self.seek_to(at, true),
            Cmd::Open(paths, play) => self.open(paths, play),
            Cmd::Volume(v) => self.set_volume(v),
            Cmd::Rate(r) => self.set_speed(r),
            Cmd::Repeat(r) => self.set_repeat(r),
            Cmd::Shuffle(on) => self.set_shuffle(on),
            Cmd::Fullscreen(on) => self.set_fullscreen(on),
        }
    }

    fn now(&self) -> Now {
        let path = self.queue.current_path();
        let title = self.st.meta("title").map(String::from).or_else(|| path.map(name_of)).unwrap_or_default();
        Now {
            status: if path.is_none() || self.ended {
                Status::Stopped
            } else if self.st.paused {
                Status::Paused
            } else {
                Status::Playing
            },
            index: self.queue.current().unwrap_or(0),
            title,
            artist: self.st.meta("artist").or(self.st.meta("album_artist")).unwrap_or_default().to_string(),
            album: self.st.meta("album").unwrap_or_default().to_string(),
            url: path.map(|p| format!("file://{}", rivet_xdg::places::percent_encode_path(p))).unwrap_or_default(),
            art: self.art.as_ref().filter(|(f, _)| Some(f.as_path()) == path).map(|(_, a)| a.clone()),
            length: self.st.length,
            position: self.st.pos,
            volume: self.st.volume,
            rate: self.st.speed,
            repeat: self.queue.repeat,
            shuffle: self.queue.shuffled(),
            fullscreen: self.fullscreen(),
            can_next: self.queue.next(true).is_some(),
            can_previous: self.queue.current().is_some(),
        }
    }

    /// Once a frame: the desktop's view of what is playing, the screen kept awake, the place
    /// in the file kept, and the window's title and size.
    fn tick(&mut self, ctx: &egui::Context) {
        if !self.live() {
            return;
        }
        let now = self.now();
        if let Some(m) = &mut self.mpris {
            m.update(&now, self.seeked);
        }
        self.seeked = false;
        let want = self.playing() && self.st.loaded && self.is_video();
        if want != self.inhibit_wanted {
            self.inhibit_wanted = want;
            if want {
                let (tx, ctx) = (self.tx.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let _ = tx.send(Msg::Inhibit(rivet_xdg::portal::inhibit_idle("Playing a video").ok()));
                    ctx.request_repaint();
                });
            } else {
                self.inhibit = None;
            }
        }
        let t = ctx.input(|i| i.time);
        if self.playing() && t - self.saved_at > 10.0 {
            self.saved_at = t;
            self.remember_position();
        }
        let title = if now.status == Status::Stopped && self.queue.current().is_none() { "Rivet Cinema".to_string() } else { format!("{} — Rivet Cinema", now.title) };
        if title != self.title {
            ctx.send_viewport_cmd(ViewportCommand::Title(title.clone()));
            self.title = title;
        }
        if self.st.loaded && self.sized_for != Some(self.audio_mode) && !self.fullscreen() {
            // The first file sets the size it needs; after that the window keeps what it was given,
            // unless the kind changes between music and video.
            if self.sized_for.is_some() || self.audio_mode {
                ctx.send_viewport_cmd(ViewportCommand::InnerSize(Vec2::from(if self.audio_mode { AUDIO_SIZE } else { VIDEO_SIZE })));
            }
            self.sized_for = Some(self.audio_mode);
        }
    }

    // ── Keys ─────────────────────────────────────────────────────────────────────

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.show_prefs {
            return;
        }
        let cmd_shift = Modifiers::COMMAND | Modifiers::SHIFT;
        let mut keys = Vec::new();
        ctx.input_mut(|i| {
            // Modified keys first, so plain ones don't take them.
            for (m, k) in [(cmd_shift, Key::O), (Modifiers::COMMAND, Key::O), (Modifiers::COMMAND, Key::Comma), (Modifiers::COMMAND, Key::Q), (Modifiers::COMMAND, Key::W), (Modifiers::SHIFT, Key::ArrowLeft), (Modifiers::SHIFT, Key::ArrowRight), (Modifiers::COMMAND, Key::ArrowLeft), (Modifiers::COMMAND, Key::ArrowRight)] {
                if i.consume_key(m, k) {
                    keys.push((m, k));
                }
            }
            for k in [
                Key::Space,
                Key::K,
                Key::ArrowLeft,
                Key::ArrowRight,
                Key::J,
                Key::L,
                Key::ArrowUp,
                Key::ArrowDown,
                Key::M,
                Key::F,
                Key::Escape,
                Key::OpenBracket,
                Key::CloseBracket,
                Key::Backspace,
                Key::N,
                Key::P,
                Key::PageDown,
                Key::PageUp,
                Key::Comma,
                Key::Period,
                Key::Home,
                Key::A,
                Key::S,
                Key::Z,
                Key::X,
                Key::Q,
                Key::T,
                Key::I,
                Key::Num0,
                Key::Num1,
                Key::Num2,
                Key::Num3,
                Key::Num4,
                Key::Num5,
                Key::Num6,
                Key::Num7,
                Key::Num8,
                Key::Num9,
            ] {
                if i.consume_key(Modifiers::NONE, k) {
                    keys.push((Modifiers::NONE, k));
                }
            }
        });
        if !keys.is_empty() {
            self.active_at = ctx.input(|i| i.time);
        }
        let len = self.st.length;
        for (m, k) in keys {
            match (m == Modifiers::NONE, k) {
                (false, Key::O) if m == cmd_shift => self.pick(Ask::Folder),
                (false, Key::O) => self.pick(Ask::File),
                (false, Key::Comma) => self.show_prefs = true,
                (false, Key::Q | Key::W) => ctx.send_viewport_cmd(ViewportCommand::Close),
                (false, Key::ArrowLeft) => self.seek_by(if m == Modifiers::SHIFT { -1.0 } else { -30.0 }),
                (false, Key::ArrowRight) => self.seek_by(if m == Modifiers::SHIFT { 1.0 } else { 30.0 }),
                (true, Key::Space | Key::K) => self.toggle_pause(),
                (true, Key::ArrowLeft) => self.seek_by(-5.0),
                (true, Key::ArrowRight) => self.seek_by(5.0),
                (true, Key::J) => self.seek_by(-10.0),
                (true, Key::L) => self.seek_by(10.0),
                (true, Key::ArrowUp) => self.set_volume(self.st.volume + 5.0),
                (true, Key::ArrowDown) => self.set_volume(self.st.volume - 5.0),
                (true, Key::M) => self.set_muted(!self.st.muted),
                (true, Key::F) => self.set_fullscreen(!self.fullscreen()),
                (true, Key::Escape) => {
                    if self.fullscreen() {
                        self.set_fullscreen(false);
                    } else if self.on_top {
                        self.on_top = false;
                    }
                }
                (true, Key::OpenBracket) => self.step_speed(false),
                (true, Key::CloseBracket) => self.step_speed(true),
                (true, Key::Backspace) => self.set_speed(1.0),
                (true, Key::N | Key::PageDown) => self.next(true),
                (true, Key::P | Key::PageUp) => self.previous(),
                (true, Key::Comma) => {
                    if let Some(p) = &self.player {
                        let _ = p.command(&["frame-back-step"]);
                    }
                }
                (true, Key::Period) => {
                    if let Some(p) = &self.player {
                        let _ = p.command(&["frame-step"]);
                    }
                }
                (true, Key::Home) => self.seek_to(0.0, true),
                (true, Key::A) => self.cycle_track("audio"),
                (true, Key::S) => self.cycle_track("sub"),
                (true, Key::Z) => self.shift_subtitles(-0.1),
                (true, Key::X) => self.shift_subtitles(0.1),
                (true, Key::Q) => self.show_queue = !self.show_queue,
                (true, Key::T) => self.on_top = !self.on_top && self.is_video(),
                (true, Key::I) => self.show_info = !self.show_info,
                (true, n) => {
                    let digit = [Key::Num0, Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9].iter().position(|d| *d == n);
                    if let Some(d) = digit
                        && len > 0.0
                    {
                        self.seek_to(len * d as f64 / 10.0, false);
                    }
                }
                _ => {}
            }
        }
    }

    // ── Drawing ──────────────────────────────────────────────────────────────────

    /// The seek bar: where it is, chapter marks, and a frame from where the pointer is.
    fn seek_bar(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 22.0), Sense::click_and_drag());
        let len = self.st.length.max(0.0);
        let track = Rect::from_center_size(r.center(), Vec2::new(r.width(), 4.0));
        let frac = if len > 0.0 { (self.st.pos / len).clamp(0.0, 1.0) as f32 } else { 0.0 };
        let p = ui.painter();
        p.rect_filled(track, 2.0, palette::mix(v.panel_fill, v.text_color(), 0.22));
        p.rect_filled(Rect::from_min_max(track.min, Pos2::new(track.left() + track.width() * frac, track.bottom())), 2.0, accent);
        if len > 0.0 {
            for (at, _) in self.st.chapters.iter().filter(|(at, _)| *at > 0.5) {
                let x = track.left() + track.width() * (*at / len) as f32;
                p.line_segment([Pos2::new(x, track.top() - 1.0), Pos2::new(x, track.bottom() + 1.0)], egui::Stroke::new(2.0, v.panel_fill));
            }
        }
        let hover = self.hover_at.map(|s| track.left() + track.width() * (s / len.max(1.0)) as f32).or_else(|| resp.hover_pos().map(|h| h.x));
        if resp.hovered() || resp.dragged() || self.hover_at.is_some() {
            p.circle_filled(Pos2::new(track.left() + track.width() * frac, track.center().y), 6.5, accent);
        }
        if len <= 0.0 {
            return;
        }
        let at = |x: f32| (((x - track.left()) / track.width()).clamp(0.0, 1.0) as f64) * len;
        if let Some(pos) = resp.interact_pointer_pos() {
            let secs = at(pos.x);
            if resp.dragged() {
                // While dragging, seek to the nearest keyframe: quick, and the picture follows.
                if self.seek_drag.is_none_or(|last| (last - secs).abs() > 0.4) {
                    self.seek_drag = Some(secs);
                    self.seek_to(secs, false);
                }
            }
            if resp.drag_stopped() || resp.clicked() {
                self.seek_drag = None;
                self.seek_to(secs, true);
            }
        }
        if let Some(x) = hover {
            let secs = at(x);
            let path = self.queue.current_path().map(Path::to_path_buf);
            let frame = if self.is_video() { path.and_then(|p| self.previews.get(ui.ctx(), &p, secs, len)) } else { None };
            let chapter = self.st.chapters.iter().rev().find(|(c, _)| *c <= secs).map(|(_, n)| n.clone()).filter(|n| !n.is_empty());
            let w = if frame.is_some() { 200.0 } else { 90.0 };
            let x = x.clamp(r.left() + w / 2.0, r.right() - w / 2.0);
            egui::Area::new(egui::Id::unique("seek-preview")).order(egui::Order::Tooltip).interactable(false).fixed_pos(Pos2::new(x, r.top() - 10.0)).pivot(Align2::CENTER_BOTTOM).show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style()).inner_margin(6).show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        if let Some(t) = &frame {
                            let size = t.size_vec2();
                            let size = size * (188.0 / size.x.max(1.0));
                            ui.add(egui::Image::new((t.id(), size)).corner_radius(r_sm()));
                        }
                        let text = match chapter {
                            Some(c) => format!("{}  ·  {c}", duration_text(secs)),
                            None => duration_text(secs),
                        };
                        ui.label(egui::RichText::new(text).size(12.5).strong());
                    });
                });
            });
        }
    }

    fn seek_row(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            let time = |s: f64| egui::RichText::new(duration_text(s)).size(12.5).color(weak).monospace();
            ui.add_sized([52.0, 22.0], egui::Label::new(time(self.st.pos)));
            let w = ui.available_width() - 62.0;
            ui.allocate_ui(Vec2::new(w.max(40.0), 22.0), |ui| self.seek_bar(ui, accent));
            ui.add_sized([52.0, 22.0], egui::Label::new(time(self.st.length)));
        });
    }

    fn volume(&mut self, ui: &mut egui::Ui, accent: Color32, width: f32) {
        let g = if self.st.muted || self.st.volume <= 0.0 {
            G::Muted
        } else if self.st.volume < 50.0 {
            G::Volume(1)
        } else {
            G::Volume(2)
        };
        if glyphs::button(ui, g, 32.0, if self.st.muted { "Unmute (M)" } else { "Mute (M)" }, false, accent).clicked() {
            self.set_muted(!self.st.muted);
        }
        let mut vol = if self.st.muted { 0.0 } else { self.st.volume };
        ui.spacing_mut().slider_width = width;
        if ui.add(egui::Slider::new(&mut vol, 0.0..=100.0).show_value(false)).on_hover_text(format!("Volume {}%", vol.round())).changed() {
            self.set_volume(vol);
        }
    }

    fn track_menus(&mut self, ui: &mut egui::Ui) {
        let speed = self.st.speed;
        ui.menu_button(speed_text(speed), |ui| {
            for s in SPEEDS {
                if ui.add(egui::Button::selectable((s - speed).abs() < 0.001, speed_text(s))).clicked() {
                    self.set_speed(s);
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text("Speed ([ and ])");
        let subs: Vec<Track> = self.st.tracks.iter().filter(|t| t.kind == "sub").cloned().collect();
        ui.menu_button("Subtitles", |ui| {
            if ui.add(egui::Button::selectable(!subs.iter().any(|t| t.selected), "Off")).clicked() {
                self.select_track("sub", None);
                ui.close();
            }
            for t in &subs {
                if ui.add(egui::Button::selectable(t.selected, track_label(t))).clicked() {
                    self.select_track("sub", Some(t.id));
                    ui.close();
                }
            }
            ui.separator();
            if ui.button("Add Subtitle File…").clicked() {
                self.pick(Ask::Subtitles);
                ui.close();
            }
            if subs.iter().any(|t| t.selected) {
                ui.horizontal(|ui| {
                    if ui.button("Earlier").on_hover_text("Z").clicked() {
                        self.shift_subtitles(-0.1);
                    }
                    if ui.button("Later").on_hover_text("X").clicked() {
                        self.shift_subtitles(0.1);
                    }
                    ui.label(egui::RichText::new(format!("{:+.1} s", self.st.sub_delay)).weak());
                });
            }
        })
        .response
        .on_hover_text("Subtitles (S)");
        let audio: Vec<Track> = self.st.tracks.iter().filter(|t| t.kind == "audio").cloned().collect();
        if audio.len() > 1 {
            ui.menu_button("Audio", |ui| {
                for t in &audio {
                    if ui.add(egui::Button::selectable(t.selected, track_label(t))).clicked() {
                        self.select_track("audio", Some(t.id));
                        ui.close();
                    }
                }
            })
            .response
            .on_hover_text("Audio track (A)");
        }
    }

    /// The row of buttons under a video's seek bar.
    fn button_row(&mut self, ui: &mut egui::Ui, accent: Color32) {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            if glyphs::button(ui, G::Previous, 36.0, "Previous (P)", false, accent).clicked() {
                self.previous();
            }
            let playing = self.playing();
            if glyphs::button(ui, if playing { G::Pause } else { G::Play }, 36.0, if playing { "Pause (Space)" } else { "Play (Space)" }, false, accent).clicked() {
                self.toggle_pause();
            }
            if glyphs::button(ui, G::Next, 36.0, "Next (N)", false, accent).clicked() {
                self.next(true);
            }
            ui.add_space(10.0);
            self.volume(ui, accent, 110.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let full = self.fullscreen();
                if glyphs::button(ui, if full { G::Windowed } else { G::Fullscreen }, 36.0, if full { "Leave Fullscreen (F)" } else { "Fullscreen (F)" }, false, accent).clicked() {
                    self.set_fullscreen(!full);
                }
                if glyphs::button(ui, G::OnTop, 36.0, "Keep on Top in a Small Window (T)", self.on_top, accent).clicked() {
                    self.on_top = !self.on_top;
                }
                if glyphs::button(ui, G::Queue, 36.0, "Queue (Q)", self.show_queue, accent).clicked() {
                    self.show_queue = !self.show_queue;
                }
                ui.add_space(6.0);
                self.track_menus(ui);
            });
        });
    }

    fn video(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, accent: Color32) {
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        let resp = ui.allocate_rect(rect, Sense::click());
        if let Some(t) = &self.still {
            let size = t.size_vec2();
            let scale = (rect.width() / size.x).min(rect.height() / size.y);
            ui.painter().image(t.id(), Rect::from_center_size(rect.center(), size * scale), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        } else if let Some(p) = &self.player {
            let ppp = ctx.pixels_per_point();
            let px = [(rect.width() * ppp).round() as i32, (rect.height() * ppp).round() as i32];
            if let Some(id) = self.screen.frame(frame, p, self.proc.as_ref(), self.display, &ctx, px) {
                ui.painter().image(id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
        }
        if let Some(why) = &self.screen.failed {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, why, egui::FontId::proportional(15.0), Color32::from_gray(220));
        }
        if self.st.buffering || (!self.st.loaded && !self.ended && self.live()) {
            egui::Spinner::new().size(28.0).color(Color32::from_gray(220)).paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(28.0)));
        }
        if self.ended {
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Played to the end of the queue", egui::FontId::proportional(17.0), Color32::from_gray(220));
        }
        if resp.double_clicked() {
            self.set_fullscreen(!self.fullscreen());
        }
        if resp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll.abs() > 0.5 {
                self.set_volume(self.st.volume + f64::from(scroll) / 12.0);
            }
        }
        let t = ctx.input(|i| i.time);
        if ctx.input(|i| i.pointer.delta() != Vec2::ZERO || i.pointer.any_pressed()) {
            self.active_at = t;
        }
        let shown = !self.live() || !self.playing() || self.over_controls || ctx.any_popup_open() || t - self.active_at < HIDE_AFTER;
        if !shown {
            if resp.hovered() {
                ctx.set_cursor_icon(CursorIcon::None);
            }
            self.over_controls = false;
            return;
        }
        if self.playing() && self.live() && !self.over_controls {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64((HIDE_AFTER - (t - self.active_at)).max(0.05)));
        }
        let v = ctx.global_style().visuals.clone();
        let w = (rect.width() - 32.0).clamp(300.0, 1000.0);
        let bar = egui::Area::new(egui::Id::unique("play-controls")).order(egui::Order::Foreground).fixed_pos(Pos2::new(rect.center().x, rect.bottom() - 16.0)).pivot(Align2::CENTER_BOTTOM).show(&ctx, |ui| {
            egui::Frame::new().fill(v.panel_fill.gamma_multiply(0.94)).corner_radius(r_lg()).inner_margin(egui::Margin { left: 16, right: 12, top: 8, bottom: 8 }).show(ui, |ui| {
                ui.set_width(w - 28.0);
                self.seek_row(ui, accent);
                self.button_row(ui, accent);
            });
        });
        self.over_controls = ctx.pointer_hover_pos().is_some_and(|p| bar.response.rect.contains(p));
        // In fullscreen, what is playing, along the top.
        if self.fullscreen() {
            let title = self.now().title;
            egui::Area::new(egui::Id::unique("play-title")).order(egui::Order::Foreground).fixed_pos(rect.left_top() + Vec2::new(20.0, 16.0)).interactable(false).show(&ctx, |ui| {
                egui::Frame::new().fill(v.panel_fill.gamma_multiply(0.85)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(12, 6)).show(ui, |ui| {
                    ui.label(egui::RichText::new(title).size(15.0).strong());
                });
            });
        }
    }

    /// The cover (or a music symbol) in a square.
    fn cover(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, r: Rect) {
        let v = ui.visuals().clone();
        let ctx = ui.ctx().clone();
        ui.painter().rect_filled(r, r_lg(), palette::mix(v.panel_fill, v.text_color(), 0.07));
        let fit = |size: Vec2| Rect::from_center_size(r.center(), size * (r.width() / size.x).min(r.height() / size.y));
        if let Some(t) = &self.cover {
            ui.painter().image(t.id(), fit(t.size_vec2()), Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            return;
        }
        if self.has_cover()
            && let Some(p) = &self.player
        {
            let ppp = ctx.pixels_per_point();
            let px = [(r.width() * ppp).round() as i32, (r.height() * ppp).round() as i32];
            if let Some(id) = self.screen.frame(frame, p, self.proc.as_ref(), self.display, &ctx, px) {
                ui.painter().image(id, r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                return;
            }
        }
        glyphs::draw(ui.painter(), Rect::from_center_size(r.center(), Vec2::splat(r.width() * 0.32)), G::Music, palette::mix(v.panel_fill, v.text_color(), 0.35));
    }

    /// Play, previous, next, repeat and shuffle, centered.
    fn transport(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let total = 4.0 * 36.0 + 56.0 + 4.0 * 14.0;
        ui.horizontal(|ui| {
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            ui.spacing_mut().item_spacing.x = 14.0;
            let r = self.queue.repeat;
            let tip = match r {
                Repeat::Off => "Repeat: off",
                Repeat::All => "Repeat: the queue",
                Repeat::One => "Repeat: this one",
            };
            if glyphs::button(ui, if r == Repeat::One { G::RepeatOne } else { G::Repeat }, 36.0, tip, r != Repeat::Off, accent).clicked() {
                self.set_repeat(r.next());
            }
            if glyphs::button(ui, G::Previous, 36.0, "Previous (P)", false, accent).clicked() {
                self.previous();
            }
            if glyphs::big_button(ui, self.playing(), 56.0, accent).clicked() {
                self.toggle_pause();
            }
            if glyphs::button(ui, G::Next, 36.0, "Next (N)", false, accent).clicked() {
                self.next(true);
            }
            let on = self.queue.shuffled();
            if glyphs::button(ui, G::Shuffle, 36.0, if on { "Shuffle: on" } else { "Shuffle: off" }, on, accent).clicked() {
                self.set_shuffle(!on);
            }
        });
    }

    /// A music file: the cover, what it is, the controls, and the queue.
    fn audio(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame, accent: Color32) {
        let full = ui.max_rect().shrink(28.0);
        let wide = full.width() > 760.0;
        let side = if wide { (full.height()).min(full.width() * 0.42) } else { (full.width()).min(340.0).min(full.height() * 0.5) };
        let cover = if wide { Rect::from_min_size(Pos2::new(full.left(), full.center().y - side / 2.0), Vec2::splat(side)) } else { Rect::from_min_size(Pos2::new(full.center().x - side / 2.0, full.top()), Vec2::splat(side)) };
        self.cover(ui, frame, cover);
        let rest = if wide { Rect::from_min_max(Pos2::new(cover.right() + 40.0, full.top()), full.max) } else { Rect::from_min_max(Pos2::new(full.left(), cover.bottom() + 20.0), full.max) };
        let path = self.queue.current_path().map(Path::to_path_buf);
        let title = self.st.meta("title").map(String::from).or_else(|| path.as_deref().map(name_of)).unwrap_or_default();
        let by = [self.st.meta("artist").or(self.st.meta("album_artist")), self.st.meta("album")].into_iter().flatten().collect::<Vec<_>>().join("  ·  ");
        ui.scope_builder(egui::UiBuilder::new().max_rect(rest), |ui| {
            if wide {
                ui.add_space(((rest.height() - 420.0) / 2.0).clamp(0.0, 80.0));
            }
            let center = !wide;
            let text = |ui: &mut egui::Ui, t: egui::RichText| {
                if center {
                    ui.vertical_centered(|ui| ui.add(egui::Label::new(t).truncate()));
                } else {
                    ui.add(egui::Label::new(t).truncate());
                }
            };
            text(ui, egui::RichText::new(title).size(if wide { 26.0 } else { 20.0 }).strong());
            if !by.is_empty() {
                text(ui, egui::RichText::new(by).size(15.0).weak());
            }
            ui.add_space(18.0);
            self.seek_row(ui, accent);
            ui.add_space(8.0);
            self.transport(ui, accent);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let w = 150.0;
                ui.add_space(((ui.available_width() - w - 40.0) / 2.0).max(0.0));
                self.volume(ui, accent, w);
            });
            if self.queue.len() > 1 {
                ui.add_space(14.0);
                self.queue_list(ui, accent);
            }
        });
    }

    /// The queue: what plays, in order. Double-click to play one, drag to reorder.
    fn queue_list(&mut self, ui: &mut egui::Ui, accent: Color32) {
        let v = ui.visuals().clone();
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Queue").size(15.0).strong());
            let n = self.queue.len();
            ui.label(egui::RichText::new(if n == 1 { "1 file".to_string() } else { format!("{n} files") }).weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if n > 0 && ui.add(egui::Button::new("Clear").frame(false)).on_hover_text("Empty the queue; what is playing stops").clicked() {
                    self.remember_position();
                    self.queue.clear();
                    if let Some(p) = &self.player {
                        let _ = p.command(&["stop"]);
                    }
                }
                if self.view() == View::Video {
                    let on = self.queue.shuffled();
                    if glyphs::button(ui, G::Shuffle, 30.0, if on { "Shuffle: on" } else { "Shuffle: off" }, on, accent).clicked() {
                        self.set_shuffle(!on);
                    }
                    let r = self.queue.repeat;
                    if glyphs::button(ui, if r == Repeat::One { G::RepeatOne } else { G::Repeat }, 30.0, "Repeat", r != Repeat::Off, accent).clicked() {
                        self.set_repeat(r.next());
                    }
                }
            });
        });
        ui.add_space(4.0);
        if self.queue.is_empty() {
            ui.label(egui::RichText::new("Nothing queued. Open files or a folder, or drop them here.").weak());
            return;
        }
        const ROW: f32 = 34.0;
        let names: Vec<String> = self.queue.items().iter().map(|p| name_of(p)).collect();
        let current = self.queue.current();
        let (mut play, mut remove, mut moved) = (None, None, None);
        let released = ui.input(|i| i.pointer.any_released());
        egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, ROW, names.len(), |ui, range| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for i in range {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click_and_drag());
                let on = current == Some(i);
                let bg = if on {
                    Some(palette::mix(v.panel_fill, accent, 0.22))
                } else if resp.hovered() {
                    Some(palette::mix(v.panel_fill, v.text_color(), 0.06))
                } else {
                    None
                };
                if let Some(bg) = bg {
                    ui.painter().rect_filled(r.shrink2(Vec2::new(0.0, 1.0)), r_sm(), bg);
                }
                let num = Rect::from_min_size(r.min, Vec2::new(36.0, ROW));
                if on && self.playing() {
                    glyphs::draw(ui.painter(), Rect::from_center_size(num.center(), Vec2::splat(11.0)), G::Play, accent);
                } else {
                    ui.painter().text(num.center(), Align2::CENTER_CENTER, (i + 1).to_string(), egui::FontId::proportional(12.0), v.weak_text_color());
                }
                let col = if on { v.strong_text_color() } else { v.text_color() };
                let g = ui.painter().layout_no_wrap(names[i].clone(), egui::FontId::proportional(13.5), col);
                ui.painter().with_clip_rect(Rect::from_min_max(Pos2::new(r.left() + 36.0, r.top()), Pos2::new(r.right() - 30.0, r.bottom()))).galley(Pos2::new(r.left() + 36.0, r.center().y - g.size().y / 2.0), g, col);
                if resp.hovered() {
                    let x = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.center().y), Vec2::splat(22.0));
                    let over = ui.ctx().pointer_hover_pos().is_some_and(|p| x.contains(p));
                    glyphs::draw(ui.painter(), x.shrink(6.0), G::Close, if over { v.strong_text_color() } else { v.weak_text_color() });
                    if over && resp.clicked() {
                        remove = Some(i);
                        continue;
                    }
                }
                if resp.double_clicked() {
                    play = Some(i);
                }
                if resp.drag_started() {
                    self.drag_row = Some(i);
                }
                if let Some(from) = self.drag_row
                    && from != i
                    && ui.ctx().pointer_hover_pos().is_some_and(|p| r.contains(p))
                {
                    let y = if from < i { r.bottom() } else { r.top() };
                    ui.painter().hline(r.x_range(), y, egui::Stroke::new(2.0, accent));
                    if released {
                        moved = Some((from, i));
                    }
                }
            }
        });
        if released {
            self.drag_row = None;
        }
        if let Some((from, to)) = moved {
            self.queue.move_item(from, to);
        }
        if let Some(i) = remove {
            let was_current = current == Some(i);
            self.queue.remove(i);
            if was_current {
                match self.queue.len() {
                    0 => {
                        if let Some(p) = &self.player {
                            let _ = p.command(&["stop"]);
                        }
                    }
                    n => self.play_index(i.min(n - 1)),
                }
            }
        }
        if let Some(i) = play {
            self.play_index(i);
        }
    }

    /// Nothing open: what to do, or what to install.
    fn empty(&mut self, ui: &mut egui::Ui) {
        let v = ui.visuals().clone();
        let r = ui.max_rect();
        let (title, under) = match &self.missing {
            Some(why) => ("Rivet Cinema needs mpv to play anything", why.clone()),
            None => ("Nothing playing", "Open a video or music file, or a folder, or drop one here.".to_string()),
        };
        ui.painter().text(r.center() - Vec2::new(0.0, 40.0), Align2::CENTER_CENTER, title, egui::FontId::proportional(20.0), v.text_color());
        let g = ui.painter().layout(under, egui::FontId::proportional(14.0), v.weak_text_color(), (r.width() - 80.0).clamp(200.0, 520.0));
        ui.painter().galley(Pos2::new(r.center().x - g.size().x / 2.0, r.center().y - 18.0), g, v.weak_text_color());
        if self.missing.is_none() {
            let row = Rect::from_center_size(r.center() + Vec2::new(0.0, 40.0), Vec2::new(280.0, 34.0));
            ui.scope_builder(egui::UiBuilder::new().max_rect(row), |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().button_padding = Vec2::new(14.0, 7.0);
                    if ui.button(egui::RichText::new("Open File…").strong()).on_hover_text("Ctrl+O").clicked() {
                        self.pick(Ask::File);
                    }
                    if ui.button("Open Folder…").on_hover_text("Ctrl+Shift+O").clicked() {
                        self.pick(Ask::Folder);
                    }
                });
            });
        }
    }

    fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.show_prefs {
            return;
        }
        let mut open = true;
        let before = self.settings.clone();
        let weak = ctx.global_style().visuals.weak_text_color();
        egui::Window::new("Preferences").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).frame(egui::Frame::window(&ctx.global_style()).inner_margin(18)).show(ctx, |ui| {
            ui.set_width(420.0);
            let s = &mut self.settings;
            ui.checkbox(&mut s.resume, "Start where you left off");
            ui.label(egui::RichText::new("For files over a minute long. Press Home to start over.").size(12.5).color(weak));
            ui.add_space(8.0);
            ui.checkbox(&mut s.hardware, "Decode video with the graphics card");
            ui.label(egui::RichText::new("Turn this off if video shows blocks, green frames or nothing at all.").size(12.5).color(weak));
            ui.add_space(12.0);
            ui.label("Audio languages");
            ui.add(egui::TextEdit::singleline(&mut s.audio_lang).hint_text("en, fr").desired_width(200.0));
            ui.add_space(6.0);
            ui.label("Subtitle languages");
            ui.add(egui::TextEdit::singleline(&mut s.sub_lang).hint_text("en").desired_width(200.0));
            ui.label(egui::RichText::new("Language codes, first choice first. A file's own choice is used when none match.").size(12.5).color(weak));
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.label("Subtitle size");
                let mut pct = s.sub_scale * 100.0;
                if ui.add(egui::Slider::new(&mut pct, 50.0..=300.0).suffix("%").step_by(5.0)).changed() {
                    s.sub_scale = pct / 100.0;
                }
            });
        });
        if self.settings != before
            && let Some(p) = &self.player
        {
            let s = &self.settings;
            p.set_str("hwdec", if s.hardware { "auto-safe" } else { "no" });
            p.set_str("alang", &s.audio_lang);
            p.set_str("slang", &s.sub_lang);
            p.set_double("sub-scale", s.sub_scale);
        }
        if !open {
            self.show_prefs = false;
            if self.live() {
                self.settings.save();
            }
        }
    }

    /// What the file playing is: its tracks, size and length.
    fn info_window(&mut self, ctx: &egui::Context) {
        if !self.show_info {
            return;
        }
        let Some(path) = self.queue.current_path().map(Path::to_path_buf) else {
            self.show_info = false;
            return;
        };
        let mut open = true;
        let weak = ctx.global_style().visuals.weak_text_color();
        egui::Window::new("Details").open(&mut open).collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).frame(egui::Frame::window(&ctx.global_style()).inner_margin(18)).show(ctx, |ui| {
            ui.set_width(440.0);
            egui::Grid::new("details").num_columns(2).spacing([16.0, 6.0]).show(ui, |ui| {
                let mut row = |k: &str, v: String| {
                    ui.label(egui::RichText::new(k).color(weak));
                    ui.add(egui::Label::new(v).wrap());
                    ui.end_row();
                };
                row("File", path.display().to_string());
                if self.st.length > 0.0 {
                    row("Length", duration_text(self.st.length));
                }
                if self.st.size.0 > 0 {
                    row("Picture", format!("{} × {}", self.st.size.0, self.st.size.1));
                }
                for t in &self.st.tracks {
                    let kind = match t.kind.as_str() {
                        "video" if t.albumart => "Cover",
                        "video" => "Video",
                        "audio" => "Audio",
                        _ => "Subtitles",
                    };
                    let mut v = track_label(t);
                    if let Some(c) = &t.codec {
                        v.push_str(&format!(", {c}"));
                    }
                    if t.external {
                        v.push_str(", from a file of its own");
                    }
                    row(kind, v);
                }
                for (k, v) in &self.st.meta {
                    if ["title", "artist", "album", "date", "genre"].contains(&k.as_str()) {
                        let mut label = k.clone();
                        label[..1].make_ascii_uppercase();
                        row(&label, v.clone());
                    }
                }
            });
        });
        if !open {
            self.show_info = false;
        }
    }

    /// Keep on top: the video in a small window of its own, which stays over others where
    /// the desktop lets a window ask (X11), and which compositors can match by its app id.
    fn on_top_window(&mut self, ctx: &egui::Context) {
        let Some(id) = (if self.is_video() { self.screen_id() } else { None }) else {
            self.on_top = false;
            return;
        };
        let builder = egui::ViewportBuilder::default().with_title("Rivet Cinema").with_app_id("rivet-cinema-pip").with_inner_size([420.0, 236.0]).with_min_inner_size([200.0, 112.0]).with_decorations(false).with_window_level(egui::WindowLevel::AlwaysOnTop);
        let mut leave = false;
        let mut toggle = false;
        ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("rivet-cinema-pip"), builder, |ui, _| {
            let rect = ui.max_rect();
            ui.painter().rect_filled(rect, 0.0, Color32::BLACK);
            ui.painter().image(id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            let resp = ui.allocate_rect(rect, Sense::click_and_drag());
            if resp.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if resp.double_clicked() || ui.input(|i| i.viewport().close_requested() || i.key_pressed(Key::Escape)) {
                leave = true;
            }
            if resp.clicked() || ui.input(|i| i.key_pressed(Key::Space)) {
                toggle = true;
            }
        });
        if leave {
            self.on_top = false;
            ctx.send_viewport_cmd(ViewportCommand::Focus);
        }
        if toggle {
            self.toggle_pause();
        }
    }

    fn screen_id(&self) -> Option<egui::TextureId> {
        self.still.as_ref().map(|t| t.id()).or(self.screen.id())
    }
}

impl eframe::App for PlayApp {
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        if let Some(shot) = &self.shot {
            raw.focused = true;
            rivet_kit::scripted_click("RIVET_CINEMA", shot.frames(), raw);
        }
    }

    /// Playback goes on with the window hidden or the screen locked, when no frame is drawn:
    /// mpv's events, the desktop's player controls and the screen kept awake are handled here,
    /// which eframe still calls then.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll(ctx);
        self.tick(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let choice = Choice { mode: Mode::System, accent: [0x35, 0x84, 0xe4], follow_desktop_colors: true, text_size: 0.0 };
        let live = self.live();
        self.look.frame(&ctx, &choice, live);
        let accent = self.look.accent(&choice);
        self.dropped(&ctx);
        self.shortcuts(&ctx);
        let v = ctx.global_style().visuals.clone();
        let view = self.view();
        if self.show_queue && view != View::Audio {
            egui::Panel::right("play-queue").resizable(false).exact_size(320.0).show_separator_line(false).frame(egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(12, 12))).show(ui, |ui| self.queue_list(ui, accent));
        }
        match view {
            View::Video => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(Color32::BLACK)).show(ui, |ui| self.video(ui, frame, accent));
            }
            View::Audio => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(v.panel_fill)).show(ui, |ui| self.audio(ui, frame, accent));
            }
            View::Empty => {
                egui::CentralPanel::default().frame(egui::Frame::new().fill(v.panel_fill)).show(ui, |ui| self.empty(ui));
            }
        }
        self.prefs_window(&ctx);
        self.info_window(&ctx);
        if self.on_top && live {
            self.on_top_window(&ctx);
        }
        let t = ctx.input(|i| i.time);
        if let Some((text, until)) = self.note.clone() {
            if t < until {
                egui::Area::new(egui::Id::unique("play-note")).order(egui::Order::Foreground).interactable(false).anchor(Align2::CENTER_TOP, Vec2::new(0.0, 18.0)).show(&ctx, |ui| {
                    egui::Frame::new().fill(v.panel_fill.gamma_multiply(0.94)).corner_radius(r_md()).inner_margin(egui::Margin::symmetric(14, 8)).show(ui, |ui| {
                        ui.label(egui::RichText::new(text).size(14.0));
                    });
                });
                ctx.request_repaint_after(std::time::Duration::from_secs_f64(until - t));
            } else {
                self.note = None;
            }
        }
        if let Some(shot) = self.shot.as_mut() {
            shot.step(&ctx);
        }
    }

    fn on_exit(&mut self, gl: Option<&eframe::glow::Context>) {
        if !self.live() {
            return;
        }
        self.remember_position();
        self.settings.volume = self.st.volume;
        self.settings.muted = self.st.muted;
        // Written here, not on a thread: the program is about to end.
        let _ = std::fs::create_dir_all(rivet_xdg::places::config_home().join("rivet-cinema"));
        let _ = std::fs::write(rivet_xdg::places::config_home().join("rivet-cinema/settings"), self.settings.text());
        self.screen.free(gl);
        self.player = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels() {
        assert_eq!(speed_text(1.0), "1×");
        assert_eq!(speed_text(1.25), "1.25×");
        let t = Track { id: 2, kind: "sub".into(), lang: Some("eng".into()), ..Track::default() };
        assert_eq!(track_label(&t), "English");
        let t = Track { title: Some("Commentary".into()), ..t };
        assert_eq!(track_label(&t), "Commentary (English)");
        let t = Track { title: Some("English (SDH)".into()), ..t };
        assert_eq!(track_label(&t), "English (SDH)");
        assert_eq!(track_label(&Track { id: 3, ..Track::default() }), "Track 3");
        assert_eq!(language("tlh"), "tlh");
    }

    #[test]
    fn mpv_is_told_the_settings() {
        let s = Settings { volume: 42.0, repeat: Repeat::One, hardware: false, ..Settings::default() };
        let o = options(&s);
        let get = |k: &str| o.iter().find(|(n, _)| *n == k).map(|(_, v)| v.clone());
        assert_eq!(get("volume").as_deref(), Some("42"));
        assert_eq!(get("loop-file").as_deref(), Some("inf"));
        assert_eq!(get("hwdec").as_deref(), Some("no"));
        assert_eq!(get("config").as_deref(), Some("no"));
    }
}
