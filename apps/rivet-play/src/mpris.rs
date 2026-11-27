//! MPRIS: how the desktop's media keys, Rivet Shell's player controls and `playerctl` see and
//! steer Rivet Play. A D-Bus service on threads of its own: calls become [`Cmd`]s for the
//! window, and the window hands over what is playing ([`Now`]), which is announced when it changes.
//!
//! It also carries `org.rivetsoft.Play.Enqueue`, so opening files while Rivet Play runs adds
//! them to the running one's queue.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};

use zbus::blocking::Connection;
use zbus::zvariant::{ObjectPath, Value};

use crate::queue::Repeat;

pub const NAME: &str = "org.mpris.MediaPlayer2.rivet_play";
const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

/// What the desktop asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Cmd {
    Raise,
    Quit,
    Play,
    Pause,
    PlayPause,
    Stop,
    Next,
    Previous,
    /// Seconds forward (or back, below zero).
    Seek(f64),
    /// Seconds from the start.
    SetPosition(f64),
    /// Files to add to the queue, and whether to play the first of them now.
    Open(Vec<PathBuf>, bool),
    Volume(f64),
    Rate(f64),
    Repeat(Repeat),
    Shuffle(bool),
    Fullscreen(bool),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    Playing,
    Paused,
    #[default]
    Stopped,
}

/// What is playing, as the desktop is told it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Now {
    pub status: Status,
    /// Which file in the queue (it names the track to the desktop).
    pub index: usize,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub url: String,
    pub art: Option<PathBuf>,
    /// Seconds.
    pub length: f64,
    pub position: f64,
    /// 0–100.
    pub volume: f64,
    pub rate: f64,
    pub repeat: Repeat,
    pub shuffle: bool,
    pub fullscreen: bool,
    pub can_next: bool,
    pub can_previous: bool,
}

type Shared = Arc<Mutex<Now>>;

#[derive(Clone)]
struct Link {
    now: Shared,
    tx: Sender<Cmd>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Link {
    fn send(&self, c: Cmd) {
        let _ = self.tx.send(c);
        (self.wake)();
    }

    fn now(&self) -> Now {
        self.now.lock().map(|n| n.clone()).unwrap_or_default()
    }
}

struct Root(Link);

#[zbus::interface(name = "org.mpris.MediaPlayer2")]
impl Root {
    fn raise(&self) {
        self.0.send(Cmd::Raise);
    }

    fn quit(&self) {
        self.0.send(Cmd::Quit);
    }

    #[zbus(property)]
    fn can_quit(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_raise(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_set_fullscreen(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn fullscreen(&self) -> bool {
        self.0.now().fullscreen
    }

    #[zbus(property)]
    fn set_fullscreen(&mut self, on: bool) {
        self.0.send(Cmd::Fullscreen(on));
    }

    #[zbus(property)]
    fn has_track_list(&self) -> bool {
        false
    }

    #[zbus(property)]
    fn identity(&self) -> String {
        "Rivet Play".into()
    }

    #[zbus(property)]
    fn desktop_entry(&self) -> String {
        "rivet-play".into()
    }

    #[zbus(property)]
    fn supported_uri_schemes(&self) -> Vec<String> {
        vec!["file".into()]
    }

    #[zbus(property)]
    fn supported_mime_types(&self) -> Vec<String> {
        mime_types()
    }
}

/// The types in the desktop file's MimeType line.
fn mime_types() -> Vec<String> {
    include_str!("../data/rivet-play.desktop").lines().find_map(|l| l.strip_prefix("MimeType=")).unwrap_or_default().split(';').filter(|m| !m.is_empty()).map(String::from).collect()
}

struct Player(Link);

fn status_text(s: Status) -> &'static str {
    match s {
        Status::Playing => "Playing",
        Status::Paused => "Paused",
        Status::Stopped => "Stopped",
    }
}

fn loop_text(r: Repeat) -> &'static str {
    match r {
        Repeat::Off => "None",
        Repeat::All => "Playlist",
        Repeat::One => "Track",
    }
}

fn micros(secs: f64) -> i64 {
    (secs.max(0.0) * 1_000_000.0) as i64
}

fn track_id(index: usize) -> String {
    format!("/org/rivetsoft/Play/track/{index}")
}

/// The track's details in MPRIS's names.
fn metadata(n: &Now) -> HashMap<String, Value<'static>> {
    let mut m: HashMap<String, Value<'static>> = HashMap::new();
    if n.status == Status::Stopped && n.title.is_empty() {
        if let Ok(p) = ObjectPath::try_from("/org/mpris/MediaPlayer2/TrackList/NoTrack") {
            m.insert("mpris:trackid".into(), Value::from(p));
        }
        return m;
    }
    if let Ok(p) = ObjectPath::try_from(track_id(n.index)) {
        m.insert("mpris:trackid".into(), Value::from(p));
    }
    if n.length > 0.0 {
        m.insert("mpris:length".into(), Value::from(micros(n.length)));
    }
    if !n.title.is_empty() {
        m.insert("xesam:title".into(), Value::from(n.title.clone()));
    }
    if !n.artist.is_empty() {
        m.insert("xesam:artist".into(), Value::from(vec![n.artist.clone()]));
    }
    if !n.album.is_empty() {
        m.insert("xesam:album".into(), Value::from(n.album.clone()));
    }
    if !n.url.is_empty() {
        m.insert("xesam:url".into(), Value::from(n.url.clone()));
    }
    if let Some(a) = &n.art {
        m.insert("mpris:artUrl".into(), Value::from(format!("file://{}", rivet_xdg::places::percent_encode_path(a))));
    }
    m
}

#[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
impl Player {
    fn next(&self) {
        self.0.send(Cmd::Next);
    }

    fn previous(&self) {
        self.0.send(Cmd::Previous);
    }

    fn pause(&self) {
        self.0.send(Cmd::Pause);
    }

    fn play_pause(&self) {
        self.0.send(Cmd::PlayPause);
    }

    fn stop(&self) {
        self.0.send(Cmd::Stop);
    }

    fn play(&self) {
        self.0.send(Cmd::Play);
    }

    /// Microseconds forward or back.
    fn seek(&self, offset: i64) {
        self.0.send(Cmd::Seek(offset as f64 / 1_000_000.0));
    }

    fn set_position(&self, track_id: ObjectPath<'_>, position: i64) {
        // Ignored unless it names the track playing, as MPRIS asks.
        if track_id.as_str() == self::track_id(self.0.now().index) {
            self.0.send(Cmd::SetPosition(position as f64 / 1_000_000.0));
        }
    }

    fn open_uri(&self, uri: String) {
        let path = uri.strip_prefix("file://").map(rivet_xdg::places::percent_decode).unwrap_or(uri);
        self.0.send(Cmd::Open(vec![PathBuf::from(path)], true));
    }

    #[zbus(property)]
    fn playback_status(&self) -> String {
        status_text(self.0.now().status).into()
    }

    #[zbus(property)]
    fn loop_status(&self) -> String {
        loop_text(self.0.now().repeat).into()
    }

    #[zbus(property)]
    fn set_loop_status(&mut self, value: String) {
        let r = match value.as_str() {
            "Track" => Repeat::One,
            "Playlist" => Repeat::All,
            _ => Repeat::Off,
        };
        self.0.send(Cmd::Repeat(r));
    }

    #[zbus(property)]
    fn rate(&self) -> f64 {
        self.0.now().rate
    }

    #[zbus(property)]
    fn set_rate(&mut self, value: f64) {
        if value > 0.0 {
            self.0.send(Cmd::Rate(value.clamp(0.25, 4.0)));
        }
    }

    #[zbus(property)]
    fn shuffle(&self) -> bool {
        self.0.now().shuffle
    }

    #[zbus(property)]
    fn set_shuffle(&mut self, value: bool) {
        self.0.send(Cmd::Shuffle(value));
    }

    #[zbus(property)]
    fn metadata(&self) -> HashMap<String, Value<'static>> {
        metadata(&self.0.now())
    }

    /// 0.0–1.0, as MPRIS counts it.
    #[zbus(property)]
    fn volume(&self) -> f64 {
        self.0.now().volume / 100.0
    }

    #[zbus(property)]
    fn set_volume(&mut self, value: f64) {
        self.0.send(Cmd::Volume((value * 100.0).clamp(0.0, 100.0)));
    }

    #[zbus(property(emits_changed_signal = "false"))]
    fn position(&self) -> i64 {
        micros(self.0.now().position)
    }

    #[zbus(property)]
    fn minimum_rate(&self) -> f64 {
        0.25
    }

    #[zbus(property)]
    fn maximum_rate(&self) -> f64 {
        4.0
    }

    #[zbus(property)]
    fn can_go_next(&self) -> bool {
        self.0.now().can_next
    }

    #[zbus(property)]
    fn can_go_previous(&self) -> bool {
        self.0.now().can_previous
    }

    #[zbus(property)]
    fn can_play(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_pause(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn can_seek(&self) -> bool {
        self.0.now().length > 0.0
    }

    #[zbus(property)]
    fn can_control(&self) -> bool {
        true
    }
}

/// Rivet Play's own calls, beside MPRIS.
struct Own(Link);

#[zbus::interface(name = "org.rivetsoft.Play")]
impl Own {
    /// Add files (or folders, or playlists) to the queue; `play` starts the first of them.
    fn enqueue(&self, paths: Vec<String>, play: bool) {
        self.0.send(Cmd::Open(paths.into_iter().map(PathBuf::from).collect(), play));
        self.0.send(Cmd::Raise);
    }
}

/// Hand files to a Rivet Play that is already running. False when none is (or it didn't answer).
pub fn send_to_running(paths: &[PathBuf]) -> bool {
    let Ok(conn) = Connection::session() else { return false };
    let Ok(proxy) = zbus::blocking::Proxy::new(&conn, NAME, PATH, "org.rivetsoft.Play") else { return false };
    let list: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    proxy.call::<_, _, ()>("Enqueue", &(list, true)).is_ok()
}

/// The running service: hand it what is playing with [`Mpris::update`].
pub struct Mpris {
    now: Shared,
    tx: Sender<(Now, bool)>,
    last: Now,
}

impl Mpris {
    /// Start the service; the desktop's calls arrive on `cmds`, with `wake` called after each.
    pub fn start(cmds: Sender<Cmd>, wake: impl Fn() + Send + Sync + 'static) -> Result<Mpris, String> {
        let now: Shared = Arc::default();
        let link = Link { now: now.clone(), tx: cmds, wake: Arc::new(wake) };
        let build = |name: String| -> zbus::Result<Connection> { zbus::blocking::connection::Builder::session()?.name(name)?.serve_at(PATH, Root(link.clone()))?.serve_at(PATH, Player(link.clone()))?.serve_at(PATH, Own(link.clone()))?.build() };
        // A second window (rivet-play --new-window) takes a name of its own.
        let conn = build(NAME.to_string()).or_else(|_| build(format!("{NAME}.instance{}", std::process::id()))).map_err(|e| e.to_string())?;
        let (tx, rx) = channel();
        std::thread::Builder::new().name("mpris".into()).spawn(move || announce(conn, rx)).map_err(|e| e.to_string())?;
        Ok(Mpris { now, tx, last: Now::default() })
    }

    /// What is playing now; `seeked` when the position jumped (a seek, or a new file).
    /// Only changes are announced; the position alone is read when asked.
    pub fn update(&mut self, now: &Now, seeked: bool) {
        if let Ok(mut n) = self.now.lock() {
            *n = now.clone();
        }
        let mut quiet = now.clone();
        quiet.position = self.last.position;
        if quiet != self.last || seeked {
            let _ = self.tx.send((now.clone(), seeked));
            self.last = now.clone();
        }
    }
}

/// Send PropertiesChanged for what differs from before, and Seeked after a jump.
fn announce(conn: Connection, rx: Receiver<(Now, bool)>) {
    let mut before = Now::default();
    let mut first = true;
    while let Ok((now, seeked)) = rx.recv() {
        let mut player: HashMap<&str, Value> = HashMap::new();
        if first || now.status != before.status {
            player.insert("PlaybackStatus", Value::from(status_text(now.status)));
        }
        if first || now.repeat != before.repeat {
            player.insert("LoopStatus", Value::from(loop_text(now.repeat)));
        }
        if first || now.shuffle != before.shuffle {
            player.insert("Shuffle", Value::from(now.shuffle));
        }
        if first || now.rate != before.rate {
            player.insert("Rate", Value::from(now.rate));
        }
        if first || now.volume != before.volume {
            player.insert("Volume", Value::from(now.volume / 100.0));
        }
        if first || (now.index, &now.title, &now.artist, &now.album, &now.art, now.length.round() as i64, &now.url) != (before.index, &before.title, &before.artist, &before.album, &before.art, before.length.round() as i64, &before.url) {
            player.insert("Metadata", Value::from(metadata(&now)));
            player.insert("CanSeek", Value::from(now.length > 0.0));
        }
        if first || now.can_next != before.can_next {
            player.insert("CanGoNext", Value::from(now.can_next));
        }
        if first || now.can_previous != before.can_previous {
            player.insert("CanGoPrevious", Value::from(now.can_previous));
        }
        if !player.is_empty() {
            let _ = conn.emit_signal(None::<&str>, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", &(PLAYER, player, Vec::<String>::new()));
        }
        if first || now.fullscreen != before.fullscreen {
            let mut root: HashMap<&str, Value> = HashMap::new();
            root.insert("Fullscreen", Value::from(now.fullscreen));
            let _ = conn.emit_signal(None::<&str>, PATH, "org.freedesktop.DBus.Properties", "PropertiesChanged", &(ROOT, root, Vec::<String>::new()));
        }
        if seeked {
            let _ = conn.emit_signal(None::<&str>, PATH, PLAYER, "Seeked", &(micros(now.position),));
        }
        before = now;
        first = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_in_mpris_names() {
        let n = Now { status: Status::Playing, index: 2, title: "Low Tide".into(), artist: "Rivet Sample Band".into(), album: "Tones".into(), url: "file:///m/a.flac".into(), art: Some(PathBuf::from("/c/a b.png")), length: 2.5, ..Now::default() };
        let m = metadata(&n);
        assert_eq!(m.get("mpris:length"), Some(&Value::from(2_500_000i64)));
        assert_eq!(m.get("xesam:title"), Some(&Value::from("Low Tide".to_string())));
        assert_eq!(m.get("xesam:artist"), Some(&Value::from(vec!["Rivet Sample Band".to_string()])));
        assert_eq!(m.get("mpris:artUrl"), Some(&Value::from("file:///c/a%20b.png".to_string())));
        assert_eq!(m.get("mpris:trackid"), Some(&Value::from(ObjectPath::try_from("/org/rivetsoft/Play/track/2").unwrap())));
        assert_eq!(metadata(&Now::default()).len(), 1);
        assert_eq!(loop_text(Repeat::One), "Track");
        assert!(mime_types().iter().any(|m| m == "video/mp4"));
    }
}
