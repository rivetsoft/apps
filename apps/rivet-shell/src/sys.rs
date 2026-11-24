//! What the panel and widgets show about the computer (workspaces, open apps, volume, network,
//! battery, processor and memory), read on a background thread so drawing never waits, and the
//! things the shell does (switch workspace, change the volume, launch, power).
//!
//! Workspaces and open windows come from the compositor: niri, Hyprland and Sway are asked
//! through their own tools; on other compositors the panel simply has no workspace buttons.
//!
//! Asking means running a program, so the thread asks only when something says there's news:
//! the compositor's event stream, `pactl subscribe` and `playerctl --follow` are left running
//! and each line from one marks its part to be read again (`follow`). A part whose program
//! isn't there, or has stopped, is read every second as before.

use std::io::BufRead;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compositor {
    Niri,
    /// Softcore, Rivet's own. `softcore msg` answers as `niri msg` does.
    Softcore,
    Hyprland,
    /// mango (mangowm), talked to with mmsg.
    Mango,
    Sway,
    Other,
}

impl Compositor {
    /// From the session's environment: each compositor's socket variable, else
    /// XDG_CURRENT_DESKTOP (for a program started some other way).
    pub fn detect() -> Compositor {
        let has = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
        if has("SOFTCORE_SOCKET") {
            return Compositor::Softcore;
        } else if has("NIRI_SOCKET") {
            return Compositor::Niri;
        } else if has("HYPRLAND_INSTANCE_SIGNATURE") {
            return Compositor::Hyprland;
        } else if has("MANGO_INSTANCE_SIGNATURE") {
            return Compositor::Mango;
        } else if has("SWAYSOCK") {
            return Compositor::Sway;
        }
        let desk = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_lowercase();
        let named = |n: &str| desk.split(':').any(|d| d == n);
        if named("softcore") {
            Compositor::Softcore
        } else if named("niri") {
            Compositor::Niri
        } else if named("hyprland") {
            Compositor::Hyprland
        } else if named("mango") || named("mangowc") || named("mangowm") {
            Compositor::Mango
        } else if named("sway") {
            Compositor::Sway
        } else {
            Compositor::Other
        }
    }

    /// The program that talks to niri, or to Softcore, which takes the same words.
    pub fn msg(self) -> &'static str {
        if self == Compositor::Softcore { "softcore" } else { "niri" }
    }

    pub fn name(self) -> &'static str {
        match self {
            Compositor::Niri => "niri",
            Compositor::Softcore => "Softcore",
            Compositor::Hyprland => "Hyprland",
            Compositor::Mango => "mango",
            Compositor::Sway => "Sway",
            Compositor::Other => "your compositor",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Workspace {
    /// What switching to it is given (niri's index, Hyprland's id, Sway's number).
    pub key: String,
    pub label: String,
    pub active: bool,
    /// Has windows (where the compositor says).
    pub busy: bool,
    pub output: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum Net {
    #[default]
    None,
    Wired,
    Wifi(String),
}

/// What's playing (through MPRIS, by playerctl).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Media {
    pub player: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
    pub album: String,
    /// The cover's picture file (empty: none, or not fetched yet).
    pub art: String,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Status {
    pub workspaces: Vec<Workspace>,
    /// App ids (or window classes) with windows open, lower case, in the order first seen.
    pub running: Vec<String>,
    /// The focused window's title.
    pub window: String,
    /// Volume 0–150 and muted.
    pub volume: Option<(u32, bool)>,
    /// Microphone volume and muted.
    pub mic: Option<(u32, bool)>,
    /// Screen brightness 0–100 (laptops' panels).
    pub brightness: Option<u32>,
    pub media: Option<Media>,
    /// How far into it, and how long it is, in seconds (0 long: the player doesn't say). Kept
    /// apart from `media`, which changes only with the track.
    pub media_pos: (u32, u32),
    pub net: Net,
    /// Wi-Fi radio on (when NetworkManager says).
    pub wifi_on: Option<bool>,
    /// Bluetooth powered (when there's a controller).
    pub bluetooth: Option<bool>,
    /// power-profiles-daemon's profile (power-saver, balanced, performance).
    pub profile: Option<String>,
    /// Battery percent and charging.
    pub battery: Option<(u32, bool)>,
    /// Processor use, 0–100.
    pub cpu: f32,
    /// Memory used and total, in bytes.
    pub mem: (u64, u64),
}

/// The latest status, and a counter bumped whenever it changes.
#[derive(Clone)]
pub struct Live {
    pub status: Arc<Mutex<Status>>,
    pub generation: Arc<AtomicU64>,
    poke: Arc<Mutex<Sender<bool>>>,
    /// The shell changed something itself: every part is read, told of or not.
    force: Arc<AtomicBool>,
}

impl Live {
    pub fn get(&self) -> Status {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// Read again now (after the shell changed something), `all`: the slow parts too.
    pub fn refresh(&self, all: bool) {
        self.force.store(true, Ordering::Relaxed);
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(all);
        }
    }
}

/// One part of the status that a program left running tells of.
#[derive(Default)]
struct Part {
    /// The program is running: without it the part is read every second.
    live: AtomicBool,
    /// It said something since the part was last read.
    news: AtomicBool,
}

impl Part {
    /// Whether to read the part now; `read` is when it last was. One with nothing telling of
    /// it is read every second; even one that's told of is read again every `RESYNC`, in case
    /// something was missed.
    fn due(&self, read: &mut Instant) -> bool {
        let every = if self.live.load(Ordering::Relaxed) { RESYNC } else { TICK - Duration::from_millis(100) };
        let due = self.news.swap(false, Ordering::Relaxed) || read.elapsed() >= every;
        if due {
            *read = Instant::now();
        }
        due
    }
}

#[derive(Default)]
struct Told {
    /// Workspaces, open apps and the focused window's title.
    comp: Part,
    /// The volume and the microphone.
    audio: Part,
    media: Part,
}

const RESYNC: Duration = Duration::from_secs(10);
/// The readings that are only files (processor, memory, battery) are taken this often.
const TICK: Duration = Duration::from_millis(900);
/// At least this long between readings that events ask for (a window's title can change many
/// times a second).
const SETTLE: Duration = Duration::from_millis(150);

/// Where a part's news comes from.
enum Source {
    Program(&'static str, Vec<&'static str>),
    Socket(std::path::PathBuf),
}

/// Keep `source` open on a thread of its own: each line `wants` marks `part` as having news and
/// wakes the status thread. A program that isn't installed is left alone (the part is read
/// every second); one that ends is started again, less and less often if it keeps ending.
fn follow(name: &str, told: Arc<Told>, part: fn(&Told) -> &Part, poke: Sender<bool>, source: Source, wants: fn(&str) -> bool) {
    let _ = std::thread::Builder::new().name(format!("rivet-shell {name}")).spawn(move || {
        let mut wait = Duration::from_secs(2);
        loop {
            let began = Instant::now();
            let mut child = None;
            let lines: Box<dyn BufRead> = match &source {
                Source::Program(cmd, args) => {
                    use std::os::unix::process::CommandExt;
                    let mut c = std::process::Command::new(cmd);
                    c.args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null());
                    // It ends with the shell, even a shell that was killed.
                    unsafe {
                        c.pre_exec(|| {
                            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                            Ok(())
                        });
                    }
                    let Ok(mut c) = c.spawn() else { return };
                    let Some(out) = c.stdout.take() else { return };
                    child = Some(c);
                    Box::new(std::io::BufReader::new(out))
                }
                Source::Socket(path) => match std::os::unix::net::UnixStream::connect(path) {
                    Ok(s) => Box::new(std::io::BufReader::new(s)),
                    Err(_) => return,
                },
            };
            let p = part(&told);
            p.live.store(true, Ordering::Relaxed);
            for line in lines.lines().map_while(Result::ok) {
                if wants(&line) {
                    p.news.store(true, Ordering::Relaxed);
                    if poke.send(false).is_err() {
                        break;
                    }
                }
            }
            p.live.store(false, Ordering::Relaxed);
            if let Some(mut c) = child {
                let _ = c.kill();
                let _ = c.wait();
            }
            if poke.send(false).is_err() {
                return;
            }
            wait = if began.elapsed() > Duration::from_secs(30) { Duration::from_secs(2) } else { (wait * 2).min(Duration::from_secs(120)) };
            std::thread::sleep(wait);
        }
    });
}

/// niri's events about workspaces and windows, but not the ones that come many times a second
/// and change nothing shown (a window's size while it's resized, when it was last focused).
fn niri_wants(line: &str) -> bool {
    (line.starts_with("{\"Workspace") || line.starts_with("{\"Window")) && !line.starts_with("{\"WindowLayoutsChanged") && !line.starts_with("{\"WindowFocusTimestampChanged")
}

/// Sound devices and the default ones: not the apps playing, and not programs coming and going
/// (every `wpctl` run is one).
fn pactl_wants(line: &str) -> bool {
    [" on sink #", " on source #", " on server"].iter().any(|k| line.contains(k))
}

fn follow_all(compositor: Compositor, told: &Arc<Told>, poke: &Sender<bool>) {
    let any = |_: &str| true;
    let comp = match compositor {
        Compositor::Niri | Compositor::Softcore => Some((Source::Program(compositor.msg(), vec!["msg", "-j", "event-stream"]), niri_wants as fn(&str) -> bool)),
        Compositor::Hyprland => std::env::var_os("XDG_RUNTIME_DIR").zip(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")).map(|(run, his)| (Source::Socket(std::path::PathBuf::from(run).join("hypr").join(his).join(".socket2.sock")), any as fn(&str) -> bool)),
        Compositor::Sway => Some((Source::Program("swaymsg", vec!["-m", "-t", "subscribe", "[\"workspace\",\"window\"]"]), any as fn(&str) -> bool)),
        Compositor::Mango | Compositor::Other => None,
    };
    if let Some((source, wants)) = comp {
        follow("compositor events", told.clone(), |t| &t.comp, poke.clone(), source, wants);
    }
    follow("sound events", told.clone(), |t| &t.audio, poke.clone(), Source::Program("pactl", vec!["subscribe"]), pactl_wants);
    follow("player events", told.clone(), |t| &t.media, poke.clone(), Source::Program("playerctl", vec!["metadata", "--follow", "--format", "{{status}}\t{{playerName}}\t{{artist}}\t{{title}}\t{{album}}\t{{mpris:artUrl}}\t{{mpris:length}}"]), any);
}

/// Start reading the status on a thread; `wake` is called after each change.
pub fn start(compositor: Compositor, wake: impl Fn() + Send + 'static) -> Live {
    let (poke, poked) = channel::<bool>();
    let told = Arc::new(Told::default());
    follow_all(compositor, &told, &poke);
    let live = Live { status: Arc::new(Mutex::new(Status::default())), generation: Arc::new(AtomicU64::new(0)), poke: Arc::new(Mutex::new(poke)), force: Arc::default() };
    let l = live.clone();
    std::thread::Builder::new()
        .name("rivet-shell status".into())
        .spawn(move || {
            let mut cpu = CpuMeter::default();
            let long_ago = Instant::now() - Duration::from_secs(60);
            let (mut slow_at, mut tick_at, mut comp_at, mut audio_at, mut media_at) = (long_ago, long_ago, long_ago, long_ago, long_ago);
            // Where the track was when the player was last asked, to count on from.
            let mut media_from = (0u32, 0u32);
            let mut s = Status::default();
            let mut all = false;
            loop {
                let began = Instant::now();
                let forced = l.force.swap(false, Ordering::Relaxed);
                let mut next = s.clone();
                if told.comp.due(&mut comp_at) || forced {
                    next.workspaces = workspaces(compositor);
                    next.running = running(compositor, &s.running);
                    next.window = focused_title(compositor);
                }
                if told.audio.due(&mut audio_at) || forced {
                    next.volume = volume("@DEFAULT_AUDIO_SINK@");
                    next.mic = volume("@DEFAULT_AUDIO_SOURCE@");
                }
                next.brightness = brightness();
                // The player is asked when it has news, and every few seconds while it plays
                // (a jump made in the player itself); between those the seconds are counted here.
                let playing = s.media.as_ref().is_some_and(|m| m.playing);
                if told.media.due(&mut media_at) || forced || (playing && media_at.elapsed() >= Duration::from_secs(4)) {
                    media_at = Instant::now();
                    (next.media, next.media_pos) = match read_media() {
                        Some((m, pos)) => (Some(m), pos),
                        None => (None, (0, 0)),
                    };
                    media_from = next.media_pos;
                } else if playing {
                    let now = media_from.0 + media_at.elapsed().as_secs() as u32;
                    next.media_pos = (if media_from.1 > 0 { now.min(media_from.1) } else { now }, media_from.1);
                }
                if tick_at.elapsed() >= TICK {
                    tick_at = Instant::now();
                    next.cpu = cpu.read();
                    next.mem = memory();
                    next.battery = battery();
                }
                let publish = |next: Status, s: &mut Status| {
                    if next != *s {
                        *s = next;
                        if let Ok(mut g) = l.status.lock() {
                            *g = s.clone();
                        }
                        l.generation.fetch_add(1, Ordering::Relaxed);
                        wake();
                    }
                };
                if all || slow_at.elapsed() >= Duration::from_secs(10) {
                    // The quick readings show first; the slow tools (Bluetooth waits up to 2 s)
                    // follow.
                    publish(next.clone(), &mut s);
                    slow_at = Instant::now();
                    next.net = network();
                    next.wifi_on = wifi_radio();
                    next.bluetooth = bluetooth();
                    next.profile = output("powerprofilesctl", &["get"]).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
                }
                publish(next, &mut s);
                // Asked to look again (a slider moved, a toggle flipped, news from outside):
                // look soon, once it has settled.
                all = match poked.recv_timeout(TICK.saturating_sub(tick_at.elapsed()).max(Duration::from_millis(20))) {
                    Ok(a) => {
                        std::thread::sleep(SETTLE.saturating_sub(began.elapsed()).max(Duration::from_millis(60)));
                        a || poked.try_iter().collect::<Vec<bool>>().contains(&true)
                    }
                    Err(RecvTimeoutError::Timeout) => false,
                    Err(RecvTimeoutError::Disconnected) => return,
                };
            }
        })
        .ok();
    live
}

/// A command's output, or nothing (missing tools are fine: that part just doesn't show).
fn output(cmd: &str, args: &[&str]) -> Option<String> {
    #[cfg(test)]
    tests::RUNS.fetch_add(1, Ordering::Relaxed);
    let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).stderr(std::process::Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn json(cmd: &str, args: &[&str]) -> Option<Value> {
    serde_json::from_str(&output(cmd, args)?).ok()
}

fn workspaces(c: Compositor) -> Vec<Workspace> {
    let s = |v: &Value, k: &str| v.get(k).map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).unwrap_or_default();
    let b = |v: &Value, k: &str| v.get(k).and_then(Value::as_bool).unwrap_or(false);
    let mut out = match c {
        Compositor::Niri | Compositor::Softcore => {
            let Some(Value::Array(list)) = json(c.msg(), &["msg", "-j", "workspaces"]) else { return Vec::new() };
            list.iter()
                .map(|w| {
                    let idx = s(w, "idx");
                    let name = w.get("name").and_then(Value::as_str).map(String::from);
                    Workspace { label: name.unwrap_or_else(|| idx.clone()), key: idx, active: b(w, "is_active"), busy: w.get("active_window_id").is_some_and(|x| !x.is_null()), output: s(w, "output") }
                })
                .collect()
        }
        Compositor::Hyprland => {
            let Some(Value::Array(list)) = json("hyprctl", &["-j", "workspaces"]) else { return Vec::new() };
            let active = json("hyprctl", &["-j", "monitors"]).and_then(|m| m.as_array().cloned()).unwrap_or_default();
            let actives: Vec<String> = active.iter().filter_map(|m| m.pointer("/activeWorkspace/id").map(|x| x.to_string())).collect();
            list.iter()
                .filter(|w| w.get("id").and_then(Value::as_i64).is_some_and(|i| i > 0))
                .map(|w| {
                    let id = s(w, "id");
                    Workspace { label: s(w, "name"), active: actives.contains(&id), key: id, busy: w.get("windows").and_then(Value::as_u64).unwrap_or(0) > 0, output: s(w, "monitor") }
                })
                .collect()
        }
        Compositor::Mango => {
            // Tags: {"all_tags":[{"monitor":"DP-1","tags":[{"index":1,"is_active":true,"client_count":2}…]}]}
            let Some(all) = json("mmsg", &["get", "all-tags"]) else { return Vec::new() };
            let mons = all.get("all_tags").and_then(Value::as_array).cloned().unwrap_or_default();
            mons.iter()
                .flat_map(|m| {
                    let out = s(m, "monitor");
                    m.get("tags").and_then(Value::as_array).cloned().unwrap_or_default().into_iter().map(move |t| {
                        let i = s(&t, "index");
                        Workspace { label: i.clone(), key: i, active: b(&t, "is_active"), busy: t.get("client_count").and_then(Value::as_u64).unwrap_or(0) > 0, output: out.clone() }
                    })
                })
                .collect()
        }
        Compositor::Sway => {
            let Some(Value::Array(list)) = json("swaymsg", &["-t", "get_workspaces", "-r"]) else { return Vec::new() };
            list.iter().map(|w| Workspace { key: s(w, "num"), label: s(w, "name"), active: b(w, "visible"), busy: true, output: s(w, "output") }).collect()
        }
        Compositor::Other => Vec::new(),
    };
    out.sort_by_key(|w| (w.output.clone(), w.key.parse::<i64>().unwrap_or(i64::MAX)));
    out
}

/// The screens' names (DP-1, HDMI-A-1…), from the compositor (none from others).
pub fn screens(c: Compositor) -> Vec<String> {
    let from_list = |v: Option<Value>| -> Vec<String> { v.and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|o| o.get("name").and_then(Value::as_str).map(String::from)).collect() };
    let mut names = match c {
        Compositor::Niri | Compositor::Softcore => json(c.msg(), &["msg", "-j", "outputs"]).and_then(|v| v.as_object().map(|o| o.keys().cloned().collect())).unwrap_or_default(),
        Compositor::Hyprland => from_list(json("hyprctl", &["-j", "monitors"])),
        Compositor::Mango => from_list(json("mmsg", &["get", "all-monitors"]).and_then(|v| v.get("monitors").cloned())),
        Compositor::Sway => from_list(json("swaymsg", &["-t", "get_outputs", "-r"])),
        Compositor::Other => Vec::new(),
    };
    names.sort();
    names
}

/// Open apps, keeping the order they were first seen in (so the dock doesn't shuffle).
fn running(c: Compositor, before: &[String]) -> Vec<String> {
    let list: Vec<String> = match c {
        Compositor::Niri | Compositor::Softcore => json(c.msg(), &["msg", "-j", "windows"]).and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|w| w.get("app_id").and_then(Value::as_str).map(String::from)).collect(),
        Compositor::Hyprland => json("hyprctl", &["-j", "clients"]).and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|w| w.get("class").and_then(Value::as_str).map(String::from)).collect(),
        Compositor::Mango => json("mmsg", &["get", "all-clients"]).and_then(|v| v.get("clients").and_then(Value::as_array).cloned()).unwrap_or_default().iter().filter_map(|w| w.get("appid").and_then(Value::as_str).map(String::from)).collect(),
        _ => Vec::new(),
    };
    let now: Vec<String> = list.into_iter().map(|s| s.to_lowercase()).filter(|s| !s.is_empty()).collect();
    let mut v: Vec<String> = before.iter().filter(|b| now.contains(b)).cloned().collect();
    for n in now {
        if !v.contains(&n) {
            v.push(n);
        }
    }
    v
}

fn focused_title(c: Compositor) -> String {
    let v = match c {
        Compositor::Niri | Compositor::Softcore => json(c.msg(), &["msg", "-j", "focused-window"]),
        Compositor::Hyprland => json("hyprctl", &["-j", "activewindow"]),
        Compositor::Mango => json("mmsg", &["get", "focusing-client"]),
        _ => None,
    };
    v.and_then(|w| w.get("title").and_then(Value::as_str).map(|t| t.trim().to_string())).unwrap_or_default()
}

fn volume(target: &str) -> Option<(u32, bool)> {
    // "Volume: 0.45" or "Volume: 0.45 [MUTED]"
    let t = output("wpctl", &["get-volume", target])?;
    let v: f32 = t.split_whitespace().nth(1)?.parse().ok()?;
    Some(((v * 100.0).round() as u32, t.contains("MUTED")))
}

/// The first backlight's brightness, from the kernel (no tool needed to read it).
fn brightness() -> Option<u32> {
    let d = std::fs::read_dir("/sys/class/backlight").ok()?.flatten().next()?.path();
    let get = |f: &str| std::fs::read_to_string(d.join(f)).ok()?.trim().parse::<f32>().ok();
    let (now, max) = (get("brightness")?, get("max_brightness")?);
    (max > 0.0).then(|| (now / max * 100.0).round() as u32)
}

/// What's playing, and (seconds in, seconds long).
fn read_media() -> Option<(Media, (u32, u32))> {
    let t = output("playerctl", &["metadata", "--format", "{{status}}\t{{playerName}}\t{{artist}}\t{{title}}\t{{album}}\t{{mpris:artUrl}}\t{{mpris:length}}\t{{position}}"])?;
    parse_media(t.lines().next()?)
}

fn parse_media(line: &str) -> Option<(Media, (u32, u32))> {
    let f: Vec<&str> = line.split('\t').collect();
    if f.len() < 4 || f[0] == "Stopped" || (f[3].trim().is_empty() && f[2].trim().is_empty()) {
        return None;
    }
    let field = |i: usize| f.get(i).map(|v| v.trim()).unwrap_or("");
    // Both in millionths of a second.
    let secs = |i: usize| (field(i).parse::<u64>().unwrap_or(0) / 1_000_000) as u32;
    let (len, pos) = (secs(6), secs(7));
    let md = Media { playing: f[0] == "Playing", player: f[1].to_string(), artist: f[2].trim().to_string(), title: f[3].trim().to_string(), album: field(4).to_string(), art: art_file(field(5)) };
    Some((md, (if len > 0 { pos.min(len) } else { pos }, len)))
}

/// The cover as a file on this computer: a player's own file as it is; one on the web
/// fetched (with curl, where it's installed) into the cache, there from the next reading on.
fn art_file(url: &str) -> String {
    if let Some(path) = url.strip_prefix("file://") {
        return percent_decode(path);
    }
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return String::new();
    }
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    url.hash(&mut h);
    let dir = rivet_xdg::places::cache_home().join("rivet-shell/covers");
    let file = dir.join(format!("{:016x}", h.finish()));
    if file.is_file() {
        return file.to_string_lossy().into_owned();
    }
    // Asked for once (the part file marks one on its way), and moved into place when whole.
    let part = file.with_extension("part");
    if !part.exists() && has("curl") && std::fs::create_dir_all(&dir).is_ok() && std::fs::write(&part, b"").is_ok() {
        let (url, part, file) = (url.to_string(), part, file);
        std::thread::spawn(move || {
            let ok = std::process::Command::new("curl").args(["-sfL", "-m", "10", "--max-filesize", "8000000", "-o"]).arg(&part).arg("--").arg(&url).stdin(std::process::Stdio::null()).status().is_ok_and(|s| s.success());
            if ok {
                let _ = std::fs::rename(&part, &file);
            } else {
                let _ = std::fs::remove_file(&part);
            }
            trim_covers(&dir);
        });
    }
    String::new()
}

/// Keep the newest covers only.
fn trim_covers(dir: &std::path::Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    let mut files: Vec<_> = rd.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    files.sort_by_key(|f| std::cmp::Reverse(f.0));
    for (_, p) in files.into_iter().skip(60) {
        let _ = std::fs::remove_file(p);
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && let Some(v) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok()) {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

pub fn wifi_radio() -> Option<bool> {
    let t = output("nmcli", &["radio", "wifi"])?;
    Some(t.trim() == "enabled")
}

/// Bluetooth's power (`timeout`, as bluetoothctl waits forever when bluetoothd isn't running).
pub fn bluetooth() -> Option<bool> {
    let any = std::fs::read_dir("/sys/class/bluetooth").ok()?.flatten().next().is_some();
    if !any {
        return None;
    }
    let t = output("timeout", &["2", "bluetoothctl", "show"])?;
    t.lines().find_map(|l| l.trim().strip_prefix("Powered:").map(|v| v.trim() == "yes"))
}

fn read(p: &str) -> Option<String> {
    std::fs::read_to_string(p).ok().map(|s| s.trim().to_string())
}

fn battery() -> Option<(u32, bool)> {
    for e in std::fs::read_dir("/sys/class/power_supply").ok()?.flatten() {
        let p = e.path();
        let p = p.to_string_lossy();
        if read(&format!("{p}/type")).as_deref() == Some("Battery") && read(&format!("{p}/scope")).as_deref() != Some("Device") {
            let cap: u32 = read(&format!("{p}/capacity"))?.parse().ok()?;
            let charging = matches!(read(&format!("{p}/status")).as_deref(), Some("Charging" | "Full"));
            return Some((cap.min(100), charging));
        }
    }
    None
}

fn network() -> Net {
    let Ok(dir) = std::fs::read_dir("/sys/class/net") else { return Net::None };
    let mut wired = false;
    for e in dir.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name == "lo" || name.starts_with("veth") || name.starts_with("docker") || name.starts_with("br-") || name.starts_with("virbr") || name.starts_with("zt") || name.starts_with("tun") {
            continue;
        }
        let p = e.path();
        if read(&format!("{}/operstate", p.display())).as_deref() != Some("up") {
            continue;
        }
        if p.join("wireless").exists() {
            let ssid = output("nmcli", &["-t", "-f", "active,ssid", "dev", "wifi"])
                .and_then(|t| t.lines().find_map(|l| l.strip_prefix("yes:").map(String::from)))
                .or_else(|| output("iwgetid", &["-r"]).map(|s| s.trim().to_string()))
                .unwrap_or_default();
            return Net::Wifi(ssid);
        }
        wired = true;
    }
    if wired { Net::Wired } else { Net::None }
}

#[derive(Default)]
struct CpuMeter {
    last: Option<(u64, u64)>,
}

impl CpuMeter {
    fn read(&mut self) -> f32 {
        let Some(line) = std::fs::read_to_string("/proc/stat").ok().and_then(|s| s.lines().next().map(String::from)) else { return 0.0 };
        let v: Vec<u64> = line.split_whitespace().skip(1).filter_map(|x| x.parse().ok()).collect();
        if v.len() < 4 {
            return 0.0;
        }
        let idle = v[3] + v.get(4).copied().unwrap_or(0);
        let total: u64 = v.iter().sum();
        let pct = match self.last {
            Some((t0, i0)) if total > t0 => 100.0 * (1.0 - (idle - i0) as f32 / (total - t0) as f32),
            _ => 0.0,
        };
        self.last = Some((total, idle));
        (pct * 10.0).round() / 10.0
    }
}

fn memory() -> (u64, u64) {
    let t = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let get = |k: &str| t.lines().find_map(|l| l.strip_prefix(k)).and_then(|r| r.split_whitespace().next()).and_then(|n| n.parse::<u64>().ok()).unwrap_or(0) * 1024;
    let total = get("MemTotal:");
    // Rounded to 64 MB, so the status (and the screen) doesn't change every second.
    let used = total.saturating_sub(get("MemAvailable:")) / (64 << 20) * (64 << 20);
    (used, total)
}

// ------------------------------------------------------------------ doing things

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Power {
    Lock,
    Suspend,
    LogOut,
    Restart,
    ShutDown,
}

/// Run a program without waiting for it (and without its output).
pub fn spawn(cmd: &str, args: &[&str]) {
    if let Ok(child) = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        reap(child);
    }
}

/// Wait for a program that was started and left to itself, so that when it ends it's gone
/// (one nobody waits for stays in the process list until the shell quits).
pub fn reap(mut child: std::process::Child) {
    let _ = std::thread::Builder::new().name("rivet-shell reap".into()).stack_size(64 * 1024).spawn(move || {
        let _ = child.wait();
    });
}

pub fn focus_workspace(c: Compositor, key: &str) {
    match c {
        Compositor::Niri | Compositor::Softcore => spawn(c.msg(), &["msg", "action", "focus-workspace", key]),
        Compositor::Hyprland => spawn("hyprctl", &["dispatch", "workspace", key]),
        Compositor::Mango => spawn("mmsg", &["dispatch", &format!("view,{key},0")]),
        Compositor::Sway => spawn("swaymsg", &["workspace", "number", key]),
        Compositor::Other => {}
    }
}

/// Whether the shell can close an app's windows on this compositor.
pub fn can_close(c: Compositor) -> bool {
    matches!(c, Compositor::Niri | Compositor::Softcore | Compositor::Hyprland)
}

/// Closes every window whose app id (lowercase) is one of `ids`, on a thread.
pub fn close_windows(c: Compositor, ids: Vec<String>) {
    std::thread::spawn(move || {
        let has = |w: &Value, key: &str| w.get(key).and_then(Value::as_str).is_some_and(|a| ids.contains(&a.to_lowercase()));
        match c {
            Compositor::Niri | Compositor::Softcore => {
                for w in json(c.msg(), &["msg", "-j", "windows"]).and_then(|v| v.as_array().cloned()).unwrap_or_default() {
                    if has(&w, "app_id")
                        && let Some(id) = w.get("id").and_then(Value::as_u64)
                    {
                        output(c.msg(), &["msg", "action", "close-window", "--id", &id.to_string()]);
                    }
                }
            }
            Compositor::Hyprland => {
                for w in json("hyprctl", &["-j", "clients"]).and_then(|v| v.as_array().cloned()).unwrap_or_default() {
                    if has(&w, "class")
                        && let Some(at) = w.get("address").and_then(Value::as_str)
                    {
                        output("hyprctl", &["dispatch", "closewindow", &format!("address:{at}")]);
                    }
                }
            }
            _ => {}
        }
    });
}

pub fn change_volume(up: bool) {
    spawn("wpctl", &["set-volume", "-l", "1.5", "@DEFAULT_AUDIO_SINK@", if up { "5%+" } else { "5%-" }]);
}

pub fn toggle_mute() {
    spawn("wpctl", &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"]);
}

pub fn toggle_mic() {
    spawn("wpctl", &["set-mute", "@DEFAULT_AUDIO_SOURCE@", "toggle"]);
}

pub fn set_volume(pct: u32) {
    spawn("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{}%", pct.min(150))]);
}

pub fn set_brightness(pct: u32) {
    // Never quite dark: a black screen can't be turned back up by looking at it.
    spawn("brightnessctl", &["-q", "set", &format!("{}%", pct.clamp(1, 100))]);
}

pub fn change_brightness(up: bool) {
    spawn("brightnessctl", &["-q", "-n2", "set", if up { "5%+" } else { "5%-" }]);
}

/// playerctl: play-pause, next, previous.
pub fn media(cmd: &str) {
    spawn("playerctl", &[cmd]);
}

/// Jump to `secs` into what's playing.
pub fn media_seek(secs: u32) {
    spawn("playerctl", &["position", &secs.to_string()]);
}

pub fn set_wifi(on: bool) {
    spawn("nmcli", &["radio", "wifi", if on { "on" } else { "off" }]);
}

pub fn set_bluetooth(on: bool) {
    spawn("timeout", &["5", "bluetoothctl", "power", if on { "on" } else { "off" }]);
}

/// The next power profile: power-saver → balanced → performance (where offered) → power-saver.
pub fn next_profile(now: &str) {
    let offered = output("powerprofilesctl", &["list"]).unwrap_or_default();
    let all: Vec<&str> = ["power-saver", "balanced", "performance"].into_iter().filter(|p| offered.contains(&format!("{p}:"))).collect();
    if all.is_empty() {
        return;
    }
    let i = all.iter().position(|p| *p == now).map(|i| (i + 1) % all.len()).unwrap_or(0);
    spawn("powerprofilesctl", &["set", all[i]]);
}

/// Text to the clipboard (wl-copy).
pub fn copy(text: &str) {
    use std::io::Write;
    if let Ok(mut c) = std::process::Command::new("wl-copy").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
        if let Some(mut i) = c.stdin.take() {
            let _ = i.write_all(text.as_bytes());
        }
        std::thread::spawn(move || c.wait());
    }
}

pub fn has(cmd: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(cmd).is_file()))
}

/// Night light: warmer colours by whichever of wlsunset, gammastep or hyprsunset is installed,
/// running for as long as it's on.
pub struct NightLight {
    child: Option<std::process::Child>,
}

impl NightLight {
    pub fn new() -> NightLight {
        NightLight { child: None }
    }
    pub fn available() -> bool {
        ["gammastep", "hyprsunset", "wlsunset"].iter().any(|c| has(c))
    }
    pub fn on(&mut self) -> bool {
        // Ended by itself (another tool took over, or the compositor refused): off.
        if let Some(c) = &mut self.child
            && !matches!(c.try_wait(), Ok(None))
        {
            self.child = None;
        }
        self.child.is_some()
    }
    pub fn set(&mut self, on: bool, kelvin: u32) {
        if let Some(mut c) = self.child.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        if !on {
            return;
        }
        let k = kelvin.clamp(2500, 6000).to_string();
        let hi = (kelvin.clamp(2500, 6000) + 1).to_string();
        let both = format!("{k}:{k}");
        // Each keeps running (Wayland undoes the colour when the program ends): gammastep with
        // the same warmth day and night, wlsunset with a night that lasts all day.
        let tries: [(&str, Vec<&str>); 3] = [("gammastep", vec!["-r", "-l", "0:0", "-t", &both]), ("hyprsunset", vec!["-t", &k]), ("wlsunset", vec!["-T", &hi, "-t", &k, "-S", "23:59", "-s", "00:00", "-d", "60"])];
        for (cmd, args) in tries {
            if has(cmd)
                && let Ok(c) = std::process::Command::new(cmd).args(&args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()
            {
                self.child = Some(c);
                return;
            }
        }
    }
}

impl Drop for NightLight {
    fn drop(&mut self) {
        self.set(false, 0);
    }
}

/// Watch the keyboards' Caps Lock and Num Lock lights (/sys/class/leds, readable by anyone, so
/// it works whichever window has the keyboard): `f(caps, on)` when one changes.
pub fn watch_lock_keys(f: impl Fn(bool, bool) + Send + 'static) {
    std::thread::Builder::new()
        .name("rivet-shell lock keys".into())
        .spawn(move || {
            // Each keyboard's light is a file; which keyboards there are is looked up now and then
            // (one may be plugged in), the lights themselves each time.
            let lights = |suffix: &str| -> Vec<std::path::PathBuf> { std::fs::read_dir("/sys/class/leds").into_iter().flatten().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(suffix)).map(|e| e.path().join("brightness")).collect() };
            let read = |files: &[std::path::PathBuf]| -> Option<bool> { (!files.is_empty()).then(|| files.iter().any(|f| std::fs::read_to_string(f).is_ok_and(|t| t.trim() != "0"))) };
            let (mut caps, mut num) = (lights("::capslock"), lights("::numlock"));
            let mut last = (read(&caps), read(&num));
            if last == (None, None) {
                return;
            }
            let mut listed = Instant::now();
            loop {
                std::thread::sleep(Duration::from_millis(120));
                if listed.elapsed() >= Duration::from_secs(5) {
                    listed = Instant::now();
                    (caps, num) = (lights("::capslock"), lights("::numlock"));
                }
                let now = (read(&caps), read(&num));
                if let (Some(a), Some(b)) = (last.0, now.0)
                    && a != b
                {
                    f(true, b);
                }
                if let (Some(a), Some(b)) = (last.1, now.1)
                    && a != b
                {
                    f(false, b);
                }
                last = now;
            }
        })
        .ok();
}

/// Who's logged in: their full name (or login name).
pub fn user_name() -> String {
    let login = std::env::var("USER").unwrap_or_default();
    let full = std::fs::read_to_string("/etc/passwd").ok().and_then(|t| t.lines().find(|l| l.split(':').next() == Some(login.as_str())).and_then(|l| l.split(':').nth(4).map(|g| g.split(',').next().unwrap_or("").trim().to_string())));
    full.filter(|f| !f.is_empty()).unwrap_or(login)
}

/// How long the computer has been up, in seconds.
pub fn uptime() -> u64 {
    std::fs::read_to_string("/proc/uptime").ok().and_then(|t| t.split_whitespace().next().and_then(|x| x.parse::<f64>().ok())).unwrap_or(0.0) as u64
}

pub fn power(c: Compositor, p: Power) {
    match p {
        Power::Lock => spawn("loginctl", &["lock-session"]),
        Power::Suspend => spawn("systemctl", &["suspend"]),
        Power::Restart => spawn("systemctl", &["reboot"]),
        Power::ShutDown => spawn("systemctl", &["poweroff"]),
        Power::LogOut => match c {
            Compositor::Niri | Compositor::Softcore => spawn(c.msg(), &["msg", "action", "quit", "--skip-confirmation"]),
            Compositor::Hyprland => spawn("hyprctl", &["dispatch", "exit"]),
            Compositor::Mango => spawn("mmsg", &["dispatch", "quit"]),
            Compositor::Sway => spawn("swaymsg", &["exit"]),
            Compositor::Other => {
                if let Ok(id) = std::env::var("XDG_SESSION_ID") {
                    spawn("loginctl", &["terminate-session", &id]);
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Programs run to read something, counted.
    pub static RUNS: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn events_are_picked() {
        assert!(niri_wants(r#"{"WorkspaceActivated":{"id":3,"focused":true}}"#));
        assert!(niri_wants(r#"{"WindowOpenedOrChanged":{"window":{}}}"#));
        assert!(!niri_wants(r#"{"WindowLayoutsChanged":{"changes":[]}}"#));
        assert!(!niri_wants(r#"{"KeyboardLayoutsChanged":{}}"#));
        assert!(pactl_wants("Event 'change' on sink #53"));
        assert!(pactl_wants("Event 'change' on server #4294967295"));
        // Every program that asks the sound server something is a client coming and going.
        assert!(!pactl_wants("Event 'new' on client #167774"));
        assert!(!pactl_wants("Event 'change' on sink-input #90"));
    }

    #[test]
    fn a_part_told_of_waits_for_news() {
        let p = Part::default();
        let mut read = Instant::now();
        // Nothing telling of it: read every second.
        assert!(!p.due(&mut read));
        read = Instant::now() - TICK;
        assert!(p.due(&mut read));
        // Told of: only on news, or after a long while.
        p.live.store(true, Ordering::Relaxed);
        read = Instant::now() - TICK;
        assert!(!p.due(&mut read));
        p.news.store(true, Ordering::Relaxed);
        assert!(p.due(&mut read) && !p.due(&mut read));
        read = Instant::now() - RESYNC;
        assert!(p.due(&mut read));
    }

    /// Reads this computer for real for a few seconds: `cargo test -- --ignored status_live`.
    /// With the event streams running, a quiet desktop is asked next to nothing.
    #[test]
    #[ignore]
    fn status_live() {
        let live = start(Compositor::detect(), || {});
        std::thread::sleep(Duration::from_secs(3));
        let first = RUNS.swap(0, Ordering::Relaxed);
        std::thread::sleep(Duration::from_secs(6));
        let later = RUNS.load(Ordering::Relaxed);
        let s = live.get();
        eprintln!("programs run: {first} at the start, {later} in the next 6 s; {} workspaces, window {:?}, volume {:?}, media {:?} at {:?}", s.workspaces.len(), s.window, s.volume, s.media.as_ref().map(|m| &m.title), s.media_pos);
        assert!(s.mem.1 > 0);
        assert!(later < 20, "{later} programs in 6 s");
    }

    #[test]
    fn reads_whats_playing() {
        let (m, pos) = parse_media("Playing\tspotify\tThe Harbor Lights\tLow Tide\tShorelines\tfile:///tmp/a%20b.png\t241000000\t61500000").unwrap();
        assert_eq!((m.title.as_str(), m.artist.as_str(), m.album.as_str(), m.art.as_str(), m.playing), ("Low Tide", "The Harbor Lights", "Shorelines", "/tmp/a b.png", true));
        assert_eq!(pos, (61, 241));
        // An older playerctl line, and a stopped player.
        assert_eq!(parse_media("Paused\tmpv\t\tA film").unwrap().1, (0, 0));
        assert!(parse_media("Stopped\tmpv\tx\ty").is_none());
    }

    #[test]
    fn reads_the_computer() {
        let (used, total) = memory();
        assert!(total > 0 && used <= total);
        let mut m = CpuMeter::default();
        let _ = m.read();
        std::thread::sleep(Duration::from_millis(50));
        assert!((0.0..=100.0).contains(&m.read()));
    }
}
