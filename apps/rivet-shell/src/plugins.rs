//! Plugins: programs that add to the shell. A plugin is a folder with a `plugin.conf` and a
//! program (in any language); the shell runs the program and shows what it says as a panel
//! item ("plugin:ID" in a panel's zones), with a menu when it gives one, and tells it what
//! happens (clicks, and the shell's events: see `EVENTS`). The same events run the person's
//! own commands (hooks: `hook.EVENT=command` in the settings file).
//!
//! Folders looked in: the shell's own (`share/rivet-shell/plugins` beside the program and in
//! the system's data folders: the official ones), then the person's
//! (`~/.local/share/rivet-shell/plugins`: third-party). A plugin runs only once it's turned
//! on in Settings.
//!
//! `plugin.conf` (key=value lines):
//!   name=Disk Space          what people see
//!   description=…            a line about it
//!   author=…  version=…
//!   exec=./disk.sh           the program, from the plugin's folder
//!   mode=poll | stream       poll (the default): run every `interval` seconds, and again for
//!                            each event; stream: started once, kept running
//!   interval=30              seconds between runs (poll)
//!   events=lock,unlock       the shell's events it's told of (besides its own clicks)
//!   setting.vault=folder|Vault|The folder to show
//!                            a setting of its own, shown under it in Settings: its kind
//!                            (folder, text or toggle), label and hint. The program finds
//!                            the value in its environment (`RIVET_SET_VAULT`), and is
//!                            started again when it changes
//!
//! What a plugin says (poll: its whole output; stream: a line at a time) is a JSON object:
//!   {"text":"42%","icon":"drive-harddisk","tooltip":"…","state":"warning","hidden":false,
//!    "badge":true,
//!    "menu":[{"id":"home","label":"/home","sub":"12 GB free"},{"heading":true,"label":"Tasks"}],
//!    "launcher":[{"id":"new","label":"New Note","sub":"In Notes","icon":"…","query":true}],
//!    "notify":{"summary":"…","body":"…"}, "run":"launcher"}
//! every key optional; keys left out keep what they were (stream) or are empty (poll). `run`
//! is a `rivet-shell msg` command. A poll plugin may print plain text instead: the first line
//! is the text, the second the tooltip, the third the state (normal, accent, warning, danger).
//!
//! What a plugin is told: poll plugins in the environment (`RIVET_EVENT=click`,
//! `RIVET_BUTTON=left`…), stream plugins as a JSON line on their input
//! (`{"event":"click","button":"left"}`). Its own events: `start`, `poll`, `click` (button:
//! left, middle, right), `scroll` (direction: up, down), `menu` (id: the row clicked),
//! `launch` (id: its launcher entry chosen; query: what was typed, for an entry with `query`).

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The shell's events: name, what it is, and what comes with it. Plugins that list one in
/// `events=` are told of it, and a hook can be set for each.
pub const EVENTS: &[(&str, &str, &str)] = &[
    ("start", "The shell started", ""),
    ("lock", "The screen is being locked", ""),
    ("unlock", "The screen was unlocked", ""),
    ("idle", "A step of idle was reached", "stage: dim, lock, screen-off, suspend"),
    ("active", "Someone is back after idle", "stage"),
    ("wallpaper", "The wallpaper changed", "path"),
    ("colors", "The desktop's colors were made again", ""),
    ("style", "Dark or light style was chosen", "mode: dark, light"),
    ("notification", "A notification arrived", "app, summary, body, urgency"),
    ("dnd", "Do Not Disturb turned on or off", "on: true, false"),
    ("workspace", "Another workspace is in front", "key, name"),
    ("media", "What's playing changed", "title, artist, player, playing"),
];

/// Something that happened: its name and what comes with it.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Event {
    pub name: String,
    pub data: Vec<(String, String)>,
}

impl Event {
    pub fn new(name: &str, data: &[(&str, &str)]) -> Event {
        Event { name: name.to_string(), data: data.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }
    /// As a stream plugin reads it.
    pub fn json(&self) -> String {
        let mut m = serde_json::Map::new();
        m.insert("event".into(), self.name.clone().into());
        for (k, v) in &self.data {
            m.insert(k.clone(), v.clone().into());
        }
        serde_json::Value::Object(m).to_string()
    }
    /// As a poll plugin, or a hook, finds it in its environment.
    pub fn env(&self) -> Vec<(String, String)> {
        let mut e = vec![("RIVET_EVENT".to_string(), self.name.clone())];
        e.extend(self.data.iter().map(|(k, v)| (format!("RIVET_{}", k.to_uppercase().replace('-', "_")), v.clone())));
        e
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Manifest {
    /// Its folder's name: what "plugin:ID" names.
    pub id: String,
    pub dir: PathBuf,
    pub name: String,
    pub description: String,
    pub author: String,
    pub version: String,
    pub exec: String,
    pub stream: bool,
    pub interval: u32,
    pub events: Vec<String>,
    /// From the shell's own folders (else the person's).
    pub official: bool,
    /// Its own settings, shown under it in Settings.
    pub settings: Vec<Setting>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SettingKind {
    #[default]
    Text,
    Folder,
    Toggle,
}

/// A setting a plugin asks for: `setting.KEY=kind|Label|Hint`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Setting {
    pub key: String,
    pub kind: SettingKind,
    pub label: String,
    pub hint: String,
}

/// The values of plugins' settings: (plugin, key, value).
pub type Values = Vec<(String, String, String)>;

/// One plugin's values, as its program's environment has them (`RIVET_SET_KEY`).
fn env_of(values: &Values, id: &str) -> Vec<(String, String)> {
    values.iter().filter(|(p, _, _)| p == id).map(|(_, k, v)| (format!("RIVET_SET_{}", k.to_uppercase().replace('-', "_")), v.clone())).collect()
}

impl Manifest {
    pub fn parse(id: &str, dir: &Path, official: bool, text: &str) -> Option<Manifest> {
        let mut m = Manifest { id: id.to_string(), dir: dir.to_path_buf(), name: id.to_string(), interval: 30, official, ..Default::default() };
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "name" => m.name = v.to_string(),
                "description" => m.description = v.to_string(),
                "author" => m.author = v.to_string(),
                "version" => m.version = v.to_string(),
                "exec" => m.exec = v.to_string(),
                "mode" => m.stream = v == "stream",
                "interval" => m.interval = v.parse::<u32>().unwrap_or(30).clamp(1, 86_400),
                "events" => m.events = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                k => {
                    if let Some(key) = k.strip_prefix("setting.").filter(|x| !x.is_empty() && x.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')) {
                        let mut parts = v.splitn(3, '|').map(str::trim);
                        let kind = match parts.next().unwrap_or("") {
                            "folder" => SettingKind::Folder,
                            "toggle" => SettingKind::Toggle,
                            _ => SettingKind::Text,
                        };
                        let label = parts.next().filter(|l| !l.is_empty()).unwrap_or(key).to_string();
                        m.settings.push(Setting { key: key.to_string(), kind, label, hint: parts.next().unwrap_or("").to_string() });
                    }
                }
            }
        }
        (!m.exec.is_empty()).then_some(m)
    }

    /// The program to run: a path from the plugin's folder, else a program beside the shell's
    /// own (another Rivet app's), else a command.
    fn command(&self, set: &[(String, String)]) -> Command {
        let mut parts = self.exec.split_whitespace();
        let prog = parts.next().unwrap_or_default();
        let local = self.dir.join(prog);
        let beside = std::env::current_exe().ok().and_then(|e| e.parent().map(|d| d.join(prog))).filter(|p| p.is_file());
        let mut c = match beside {
            _ if prog.contains('/') || local.is_file() => Command::new(local),
            Some(b) => Command::new(b),
            None => Command::new(prog),
        };
        c.args(parts).current_dir(&self.dir).env("RIVET_PLUGIN", &self.id).env("RIVET_PLUGIN_DIR", &self.dir).envs(set.iter().cloned());
        c
    }
}

/// The person's plugins folder.
pub fn user_dir() -> PathBuf {
    rivet_xdg::places::data_home().join("rivet-shell/plugins")
}

/// Where plugins are looked for, and whether each is the shell's own.
pub fn dirs() -> Vec<(PathBuf, bool)> {
    let mut d: Vec<(PathBuf, bool)> = Vec::new();
    // Checks, and plugins being written: RIVET_SHELL_PLUGINS=folder.
    if let Ok(p) = std::env::var("RIVET_SHELL_PLUGINS") {
        d.push((PathBuf::from(p), false));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(prefix) = exe.parent().and_then(Path::parent)
    {
        d.push((prefix.join("share/rivet-shell/plugins"), true));
    }
    // Run from the source tree: the plugins that come with it.
    if cfg!(debug_assertions) {
        d.push((PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/data/plugins")), true));
    }
    let sys = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for s in sys.split(':') {
        d.push((PathBuf::from(s).join("rivet-shell/plugins"), true));
    }
    d.push((user_dir(), false));
    let mut seen = Vec::new();
    d.retain(|(p, _)| {
        let new = !seen.contains(p);
        seen.push(p.clone());
        new
    });
    d
}

/// Every plugin installed, by name; the first folder with an id wins.
pub fn found() -> Vec<Manifest> {
    let mut out: Vec<Manifest> = Vec::new();
    for (dir, official) in dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Some(id) = path.file_name().and_then(|n| n.to_str()).map(String::from) else { continue };
            if out.iter().any(|m| m.id == id) || id.contains([',', ';', '=', '+', '/']) {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(path.join("plugin.conf"))
                && let Some(m) = Manifest::parse(&id, &path, official, &text)
            {
                out.push(m);
            }
        }
    }
    out.sort_by_key(|m| m.name.to_lowercase());
    out
}

/// `found`, read at most every couple of seconds (for what's drawn every frame).
pub fn found_cached() -> Vec<Manifest> {
    static CACHE: Mutex<Option<(Instant, Vec<Manifest>)>> = Mutex::new(None);
    let Ok(mut c) = CACHE.lock() else { return found() };
    match &*c {
        Some((at, list)) if at.elapsed() < Duration::from_secs(2) => list.clone(),
        _ => {
            let list = found();
            *c = Some((Instant::now(), list.clone()));
            list
        }
    }
}

/// How an item's text is coloured.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum State {
    #[default]
    Normal,
    Accent,
    Warning,
    Danger,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuRow {
    pub id: String,
    pub label: String,
    pub sub: String,
    /// A title over the rows after it (a line, when it has no label), not a row to click.
    pub heading: bool,
    /// An icon's name, or a picture's path.
    pub icon: String,
    /// A task's box before the label: ' ' (to do), '/' (started) or 'x' (done).
    pub mark: Option<char>,
    /// The colour of its second line.
    pub state: State,
    /// In weaker text (a note more than a thing).
    pub quiet: bool,
}

/// An entry a plugin puts in the launcher.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct LaunchRow {
    pub id: String,
    pub label: String,
    pub sub: String,
    pub icon: String,
    /// It takes what's typed: it shows for any search, and is told the words.
    pub query: bool,
}

/// What a plugin shows on the panel, and in its menu.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Item {
    /// The plugin's name (for its menu's title and hover text).
    pub name: String,
    pub text: String,
    /// An icon's name in the icon theme, or a picture's path.
    pub icon: String,
    pub tooltip: String,
    pub state: State,
    pub hidden: bool,
    /// A dot on the item: something wants attention.
    pub badge: bool,
    pub menu: Vec<MenuRow>,
    pub launcher: Vec<LaunchRow>,
    /// It stopped and couldn't be started again.
    pub failed: bool,
}

/// The plugins' items, by plugin id.
pub type Items = Vec<(String, Item)>;

pub fn item<'a>(items: &'a Items, id: &str) -> Option<&'a Item> {
    items.iter().find(|(i, _)| i == id).map(|(_, it)| it)
}

/// What a plugin said, taken into its item; what it asked for besides comes back.
#[derive(Debug, PartialEq, Default)]
pub struct Asks {
    pub notify: Option<(String, String)>,
    pub run: Option<String>,
}

/// Take a plugin's message into `item`. `fresh`: keys left out are emptied (a poll plugin's
/// whole answer) rather than kept (a stream plugin's update).
pub fn take(item: &mut Item, text: &str, fresh: bool) -> Asks {
    let text = text.trim();
    let mut asks = Asks::default();
    let name = std::mem::take(&mut item.name);
    if fresh {
        *item = Item::default();
    }
    item.name = name;
    let Some(v) = text.starts_with('{').then(|| serde_json::from_str::<serde_json::Value>(text).ok()).flatten() else {
        // Plain text: the text, its tooltip, its state.
        if !text.starts_with('{') {
            let mut lines = text.lines();
            item.text = lines.next().unwrap_or("").trim().to_string();
            item.tooltip = lines.next().unwrap_or("").trim().to_string();
            item.state = state(lines.next().unwrap_or(""));
        }
        return asks;
    };
    let s = |k: &str| v.get(k).and_then(|x| x.as_str().map(String::from).or_else(|| x.as_f64().map(|n| n.to_string())));
    if let Some(t) = s("text") {
        item.text = t.replace('\n', " ");
    }
    if let Some(t) = s("icon") {
        item.icon = t;
    }
    if let Some(t) = s("tooltip") {
        item.tooltip = t;
    }
    if let Some(t) = s("state") {
        item.state = state(&t);
    }
    if let Some(h) = v.get("hidden").and_then(|x| x.as_bool()) {
        item.hidden = h;
    }
    if let Some(rows) = v.get("menu").and_then(|x| x.as_array()) {
        let f = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
        item.menu = rows
            .iter()
            .take(60)
            .map(|r| MenuRow { id: f(r, "id"), label: f(r, "label"), sub: f(r, "sub"), heading: b(r, "heading"), icon: f(r, "icon"), mark: f(r, "mark").chars().next().map(|c| c.to_ascii_lowercase()).filter(|c| matches!(c, ' ' | '/' | 'x')), state: state(&f(r, "state")), quiet: b(r, "quiet") })
            .filter(|r| !r.label.is_empty() || r.heading)
            .collect();
    }
    if let Some(h) = v.get("badge").and_then(|x| x.as_bool()) {
        item.badge = h;
    }
    if let Some(rows) = v.get("launcher").and_then(|x| x.as_array()) {
        let f = |r: &serde_json::Value, k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        item.launcher = rows.iter().take(12).map(|r| LaunchRow { id: f(r, "id"), label: f(r, "label"), sub: f(r, "sub"), icon: f(r, "icon"), query: r.get("query").and_then(|x| x.as_bool()).unwrap_or(false) }).filter(|r| !r.label.is_empty() && !r.id.is_empty()).collect();
    }
    if let Some(n) = v.get("notify") {
        let f = |k: &str| n.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let summary = n.as_str().map(String::from).unwrap_or_else(|| f("summary"));
        if !summary.is_empty() {
            asks.notify = Some((summary, f("body")));
        }
    }
    asks.run = s("run").filter(|r| !r.trim().is_empty());
    asks
}

fn state(v: &str) -> State {
    match v.trim() {
        "accent" => State::Accent,
        "warning" => State::Warning,
        "danger" | "error" | "critical" => State::Danger,
        _ => State::Normal,
    }
}

/// Run a hook's command (a shell line), told of the event in its environment.
pub fn run_hook(cmd: &str, ev: &Event) {
    let mut c = Command::new("sh");
    c.args(["-c", cmd]).envs(ev.env()).stdin(Stdio::null());
    match c.spawn() {
        // Waited for, off to the side, so it doesn't stay a zombie.
        Ok(mut child) => {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
        }
        Err(e) => eprintln!("rivet-shell: hook for {}: {e}", ev.name),
    }
}

enum Tell {
    Event(Event),
    Stop,
}

struct Running {
    manifest: Manifest,
    /// Its settings as it was started with them.
    set: Vec<(String, String)>,
    /// Set when it's stopped: what it still says is dropped (another run may have its place).
    retired: Arc<AtomicBool>,
    tx: Sender<Tell>,
}

/// The plugins that are on: their programs kept running, and what they show.
pub struct Plugins {
    items: Arc<Mutex<Items>>,
    generation: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
    running: Vec<Running>,
}

impl Plugins {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Plugins {
        Plugins { items: Default::default(), generation: Default::default(), wake: Arc::new(wake), running: Vec::new() }
    }

    pub fn get(&self) -> Items {
        self.items.lock().map(|i| i.clone()).unwrap_or_default()
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Start the plugins turned on that aren't running, stop the ones turned off (or changed,
    /// or gone).
    pub fn sync(&mut self, enabled: &[String], values: &Values) {
        let found = if enabled.is_empty() { Vec::new() } else { found() };
        let want: Vec<&Manifest> = found.iter().filter(|m| enabled.contains(&m.id)).collect();
        let mut changed = false;
        self.running.retain(|r| {
            let keep = want.iter().any(|m| **m == r.manifest && env_of(values, &m.id) == r.set);
            if !keep {
                r.retired.store(true, Ordering::Relaxed);
                let _ = r.tx.send(Tell::Stop);
                changed = true;
            }
            keep
        });
        if changed && let Ok(mut items) = self.items.lock() {
            items.retain(|(id, _)| want.iter().any(|m| &m.id == id) && self.running.iter().any(|r| &r.manifest.id == id));
        }
        for m in want {
            if self.running.iter().any(|r| r.manifest.id == m.id) {
                continue;
            }
            let (tx, rx) = mpsc::channel();
            let retired = Arc::new(AtomicBool::new(false));
            let shared = Shared { id: m.id.clone(), items: self.items.clone(), generation: self.generation.clone(), wake: self.wake.clone(), retired: retired.clone() };
            shared.update(|it| it.name = m.name.clone());
            let (man, set) = (m.clone(), env_of(values, &m.id));
            let theirs = set.clone();
            std::thread::spawn(move || if man.stream { stream(man, theirs, rx, shared) } else { poll(man, theirs, rx, shared) });
            self.running.push(Running { manifest: m.clone(), set, retired, tx });
            changed = true;
        }
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Tell one plugin of something (a click on its item).
    pub fn send(&self, id: &str, ev: Event) {
        if let Some(r) = self.running.iter().find(|r| r.manifest.id == id) {
            let _ = r.tx.send(Tell::Event(ev));
        }
    }

    /// Tell every plugin that asked to hear of this event.
    pub fn emit(&self, ev: &Event) {
        for r in self.running.iter().filter(|r| r.manifest.events.iter().any(|e| e == &ev.name)) {
            let _ = r.tx.send(Tell::Event(ev.clone()));
        }
    }

    pub fn stop(&mut self) {
        for r in self.running.drain(..) {
            let _ = r.tx.send(Tell::Stop);
        }
    }
}

impl Drop for Plugins {
    fn drop(&mut self) {
        self.stop();
    }
}

/// A plugin's place in what the shell shows.
struct Shared {
    id: String,
    items: Arc<Mutex<Items>>,
    generation: Arc<AtomicU64>,
    wake: Arc<dyn Fn() + Send + Sync>,
    retired: Arc<AtomicBool>,
}

impl Shared {
    fn update(&self, f: impl FnOnce(&mut Item)) {
        if self.retired.load(Ordering::Relaxed) {
            return;
        }
        let changed = match self.items.lock() {
            Ok(mut items) => {
                let at = match items.iter().position(|(i, _)| i == &self.id) {
                    Some(at) => at,
                    None => {
                        items.push((self.id.clone(), Item::default()));
                        items.len() - 1
                    }
                };
                let before = items[at].1.clone();
                f(&mut items[at].1);
                items[at].1 != before
            }
            Err(_) => false,
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
            (self.wake)();
        }
    }

    /// What the plugin said: its item, and what it asked for.
    fn said(&self, name: &str, text: &str, fresh: bool) {
        let mut asks = Asks::default();
        self.update(|it| {
            asks = take(it, text, fresh);
            it.failed = false;
        });
        if let Some((summary, body)) = asks.notify {
            crate::sys::spawn("notify-send", &["-a", name, &summary, &body]);
        }
        if let Some(cmd) = asks.run
            && let Err(e) = crate::ipc::send(cmd.trim())
        {
            eprintln!("rivet-shell: plugin {}: {e}", self.id);
        }
    }

    fn gone(&self) {
        if self.retired.load(Ordering::Relaxed) {
            return;
        }
        if let Ok(mut items) = self.items.lock() {
            items.retain(|(i, _)| i != &self.id);
        }
        self.generation.fetch_add(1, Ordering::Relaxed);
        (self.wake)();
    }
}

/// A poll plugin: run now, every `interval`, and for each event; what it prints is its item.
fn poll(m: Manifest, set: Vec<(String, String)>, rx: Receiver<Tell>, shared: Shared) {
    let mut ev = Event::new("start", &[]);
    loop {
        let mut c = m.command(&set);
        c.envs(ev.env()).stdin(Stdio::null()).stderr(Stdio::inherit());
        match c.output() {
            Ok(out) => shared.said(&m.name, &String::from_utf8_lossy(&out.stdout), true),
            Err(e) => {
                eprintln!("rivet-shell: plugin {}: {e}", m.id);
                shared.update(|it| it.failed = true);
            }
        }
        match rx.recv_timeout(Duration::from_secs(m.interval as u64)) {
            Ok(Tell::Event(e)) => ev = e,
            Err(RecvTimeoutError::Timeout) => ev = Event::new("poll", &[]),
            Ok(Tell::Stop) | Err(RecvTimeoutError::Disconnected) => break,
        }
    }
    shared.gone();
}

/// A stream plugin: kept running (started again when it stops, a few times a minute at
/// most); each line it prints updates its item, and events are written to it.
fn stream(m: Manifest, set: Vec<(String, String)>, rx: Receiver<Tell>, shared: Shared) {
    let mut starts: Vec<Instant> = Vec::new();
    let shared = Arc::new(shared);
    'run: loop {
        let now = Instant::now();
        starts.retain(|t| now.duration_since(*t) < Duration::from_secs(60));
        if starts.len() >= 5 {
            eprintln!("rivet-shell: plugin {} keeps stopping; left off", m.id);
            shared.update(|it| it.failed = true);
            // Still listening, so turning it off (or the shell quitting) ends this.
            loop {
                match rx.recv() {
                    Ok(Tell::Event(_)) => {}
                    _ => break 'run,
                }
            }
        }
        starts.push(now);
        let mut c = m.command(&set);
        c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit());
        let mut child = match c.spawn() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("rivet-shell: plugin {}: {e}", m.id);
                std::thread::sleep(Duration::from_secs(2));
                continue;
            }
        };
        let mut stdin = child.stdin.take();
        let done = Arc::new(AtomicBool::new(false));
        if let Some(out) = child.stdout.take() {
            let (sh, name, done) = (shared.clone(), m.name.clone(), done.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(out).lines().map_while(Result::ok) {
                    if !line.trim().is_empty() {
                        sh.said(&name, &line, false);
                    }
                }
                done.store(true, Ordering::Relaxed);
            });
        }
        let mut tell = |ev: &Event| {
            if let Some(i) = stdin.as_mut() {
                let _ = writeln!(i, "{}", ev.json()).and_then(|_| i.flush());
            }
        };
        tell(&Event::new("start", &[]));
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Tell::Event(e)) => tell(&e),
                Err(RecvTimeoutError::Timeout) => {
                    if done.load(Ordering::Relaxed) || !matches!(child.try_wait(), Ok(None)) {
                        let _ = child.kill();
                        let _ = child.wait();
                        std::thread::sleep(Duration::from_secs(2));
                        continue 'run;
                    }
                }
                Ok(Tell::Stop) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break 'run;
                }
            }
        }
    }
    shared.gone();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Real programs: a poll plugin run for its start and for a click, a stream plugin told of
    /// an event it asked for, and both gone once turned off.
    #[test]
    fn takes_settings_rows_and_launcher_entries() {
        let m = Manifest::parse("notes", Path::new("/p/notes"), true, "name=Notes\nexec=rivet-notes shell-plugin\nsetting.vault=folder|Vault|The folder\nsetting.all=toggle|Every task\nsetting.bad key=text|x\nsetting.word=odd\n").unwrap();
        assert_eq!(m.settings.iter().map(|s| (s.key.as_str(), s.kind, s.label.as_str(), s.hint.as_str())).collect::<Vec<_>>(), [("vault", SettingKind::Folder, "Vault", "The folder"), ("all", SettingKind::Toggle, "Every task", ""), ("word", SettingKind::Text, "word", "")]);
        let values: Values = vec![("notes".into(), "vault".into(), "/v".into()), ("other".into(), "x".into(), "1".into())];
        assert_eq!(env_of(&values, "notes"), [("RIVET_SET_VAULT".to_string(), "/v".to_string())]);
        let mut it = Item::default();
        take(&mut it, r#"{"badge":true,"menu":[{"heading":true,"label":"Tasks"},{"heading":true},{"id":"a","label":"Call","sub":"Today","mark":"/","state":"accent"},{"label":"Nothing","quiet":true},{"id":"x"}],"launcher":[{"id":"new","label":"New Note","query":true},{"label":"no id"}]}"#, false);
        assert!(it.badge);
        assert_eq!(it.menu.len(), 4);
        assert!(it.menu[0].heading && it.menu[1].heading && it.menu[1].label.is_empty());
        assert_eq!((it.menu[2].mark, it.menu[2].state, it.menu[3].quiet), (Some('/'), State::Accent, true));
        assert_eq!(it.launcher, [LaunchRow { id: "new".into(), label: "New Note".into(), query: true, ..Default::default() }]);
        // Left out later: kept.
        take(&mut it, r#"{"text":"2"}"#, false);
        assert!(it.badge && it.launcher.len() == 1);
    }

    #[test]
    fn runs_plugins() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("rivet-shell-plugins-{}", std::process::id()));
        let make = |id: &str, conf: &str, script: &str| {
            let d = root.join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("plugin.conf"), conf).unwrap();
            std::fs::write(d.join("run.sh"), script).unwrap();
            std::fs::set_permissions(d.join("run.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
        };
        make("poller", "name=Poller\nexec=./run.sh\ninterval=600\n", "#!/bin/sh\necho \"$RIVET_EVENT $RIVET_BUTTON\"\necho tip\n");
        make("streamer", "name=Streamer\nexec=./run.sh\nmode=stream\nevents=lock\n", "#!/bin/sh\necho '{\"text\":\"up\",\"icon\":\"x\"}'\nwhile read -r line; do case \"$line\" in *lock*) echo '{\"text\":\"locked\"}';; esac; done\n");
        // SAFETY: only this test reads the variable.
        unsafe { std::env::set_var("RIVET_SHELL_PLUGINS", &root) };
        let mut p = Plugins::new(|| {});
        let none = Values::new();
        p.sync(&["poller".to_string(), "streamer".to_string(), "missing".to_string()], &none);
        let wait = |p: &Plugins, id: &str, text: &str| {
            let end = Instant::now() + Duration::from_secs(5);
            while Instant::now() < end {
                if item(&p.get(), id).is_some_and(|i| i.text == text) {
                    return true;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            false
        };
        assert!(wait(&p, "poller", "start"), "{:?}", p.get());
        assert_eq!(item(&p.get(), "poller").map(|i| (i.name.clone(), i.tooltip.clone())), Some(("Poller".into(), "tip".into())));
        p.send("poller", Event::new("click", &[("button", "left")]));
        assert!(wait(&p, "poller", "click left"), "{:?}", p.get());
        assert!(wait(&p, "streamer", "up"), "{:?}", p.get());
        // Not asked for: not told. Asked for: told, and the icon it gave before stays.
        p.emit(&Event::new("wallpaper", &[]));
        p.emit(&Event::new("lock", &[]));
        assert!(wait(&p, "streamer", "locked"), "{:?}", p.get());
        assert_eq!(item(&p.get(), "streamer").map(|i| i.icon.clone()), Some("x".into()));
        assert_eq!(item(&p.get(), "poller").map(|i| i.text.clone()), Some("click left".into()));
        // A setting of its own: in its environment, and it's run again when it changes.
        make("asker", "name=Asker\nexec=./run.sh\ninterval=600\nsetting.place=folder|Place|Where\n", "#!/bin/sh\necho \"at $RIVET_SET_PLACE\"\n");
        let on = ["poller".to_string(), "streamer".to_string(), "asker".to_string()];
        p.sync(&on, &none);
        assert!(wait(&p, "asker", "at"), "{:?}", p.get());
        p.sync(&on, &vec![("asker".to_string(), "place".to_string(), "/tmp".to_string())]);
        assert!(wait(&p, "asker", "at /tmp"), "{:?}", p.get());
        assert_eq!(item(&p.get(), "streamer").map(|i| i.text.clone()), Some("locked".into()));
        // Turned off: stopped, and its item goes.
        p.sync(&["poller".to_string()], &none);
        assert!(item(&p.get(), "streamer").is_none());
        p.sync(&[], &none);
        let end = Instant::now() + Duration::from_secs(5);
        while Instant::now() < end && !p.get().is_empty() {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(p.get().is_empty(), "{:?}", p.get());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn reads_a_manifest_and_what_plugins_say() {
        let m = Manifest::parse("disk", Path::new("/p/disk"), true, "# a plugin\nname=Disk Space\nexec=./disk.sh --all\nmode=stream\nevents=unlock, wallpaper\ninterval=0\n").unwrap();
        assert_eq!((m.name.as_str(), m.stream, m.interval, m.events.len(), m.official), ("Disk Space", true, 1, 2, true));
        assert!(Manifest::parse("x", Path::new("/p/x"), false, "name=No program").is_none());
        // A stream plugin's update keeps what it leaves out.
        let mut it = Item { name: "Disk".into(), ..Default::default() };
        let asks = take(&mut it, r#"{"text":"42%","icon":"drive-harddisk","state":"warning","menu":[{"id":"h","label":"/home","sub":"1 GB"},{"id":"x"}],"notify":{"summary":"Low","body":"b"},"run":"launcher"}"#, false);
        assert_eq!((it.text.as_str(), it.state, it.menu.len(), it.name.as_str()), ("42%", State::Warning, 1, "Disk"));
        assert_eq!(asks, Asks { notify: Some(("Low".into(), "b".into())), run: Some("launcher".into()) });
        take(&mut it, r#"{"tooltip":"t"}"#, false);
        assert_eq!((it.text.as_str(), it.tooltip.as_str(), it.icon.as_str()), ("42%", "t", "drive-harddisk"));
        // A poll plugin's answer is all there is; plain text works too.
        take(&mut it, "12 GB\nFree on /\ndanger\n", true);
        assert_eq!((it.text.as_str(), it.tooltip.as_str(), it.state, it.icon.as_str(), it.menu.len()), ("12 GB", "Free on /", State::Danger, "", 0));
        // Broken JSON changes nothing it shows.
        take(&mut it, "{oops", false);
        assert_eq!(it.text, "12 GB");
        let ev = Event::new("idle", &[("stage", "screen-off")]);
        assert_eq!(ev.json(), r#"{"event":"idle","stage":"screen-off"}"#);
        assert!(ev.env().contains(&("RIVET_STAGE".to_string(), "screen-off".to_string())));
    }
}
