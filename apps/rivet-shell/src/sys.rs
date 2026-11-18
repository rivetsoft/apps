//! What the panel and widgets show about the computer (workspaces, open apps, volume, network,
//! battery, processor and memory), read on a background thread so drawing never waits, and the
//! things the shell does (switch workspace, change the volume, launch, power).
//!
//! Workspaces and open windows come from the compositor: niri, Hyprland and Sway are asked
//! through their own tools; on other compositors the panel simply has no workspace buttons.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compositor {
    Niri,
    Hyprland,
    Sway,
    Other,
}

impl Compositor {
    pub fn detect() -> Compositor {
        let has = |k: &str| std::env::var_os(k).is_some_and(|v| !v.is_empty());
        if has("NIRI_SOCKET") {
            Compositor::Niri
        } else if has("HYPRLAND_INSTANCE_SIGNATURE") {
            Compositor::Hyprland
        } else if has("SWAYSOCK") {
            Compositor::Sway
        } else {
            Compositor::Other
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Compositor::Niri => "niri",
            Compositor::Hyprland => "Hyprland",
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
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(all);
        }
    }
}

/// Start reading the status every second on a thread; `wake` is called after each change.
pub fn start(compositor: Compositor, wake: impl Fn() + Send + 'static) -> Live {
    let (poke, poked) = channel::<bool>();
    let live = Live { status: Arc::new(Mutex::new(Status::default())), generation: Arc::new(AtomicU64::new(0)), poke: Arc::new(Mutex::new(poke)) };
    let l = live.clone();
    std::thread::Builder::new()
        .name("rivet-shell status".into())
        .spawn(move || {
            let mut cpu = CpuMeter::default();
            let mut slow_at = Instant::now() - Duration::from_secs(60);
            let mut s = Status::default();
            let mut all = false;
            loop {
                let mut next = s.clone();
                next.workspaces = workspaces(compositor);
                next.running = running(compositor, &s.running);
                next.window = focused_title(compositor);
                next.volume = volume("@DEFAULT_AUDIO_SINK@");
                next.mic = volume("@DEFAULT_AUDIO_SOURCE@");
                next.brightness = brightness();
                next.media = read_media();
                next.cpu = cpu.read();
                next.mem = memory();
                if all || slow_at.elapsed() >= Duration::from_secs(10) {
                    slow_at = Instant::now();
                    next.battery = battery();
                    next.net = network();
                    next.wifi_on = wifi_radio();
                    next.bluetooth = bluetooth();
                    next.profile = output("powerprofilesctl", &["get"]).map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
                }
                if next != s {
                    s = next;
                    if let Ok(mut g) = l.status.lock() {
                        *g = s.clone();
                    }
                    l.generation.fetch_add(1, Ordering::Relaxed);
                    wake();
                }
                // Asked to look again (a slider moved, a toggle flipped): look soon.
                all = match poked.recv_timeout(Duration::from_millis(900)) {
                    Ok(a) => {
                        std::thread::sleep(Duration::from_millis(60));
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
        Compositor::Niri => {
            let Some(Value::Array(list)) = json("niri", &["msg", "-j", "workspaces"]) else { return Vec::new() };
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
        Compositor::Sway => {
            let Some(Value::Array(list)) = json("swaymsg", &["-t", "get_workspaces", "-r"]) else { return Vec::new() };
            list.iter().map(|w| Workspace { key: s(w, "num"), label: s(w, "name"), active: b(w, "visible"), busy: true, output: s(w, "output") }).collect()
        }
        Compositor::Other => Vec::new(),
    };
    out.sort_by_key(|w| (w.output.clone(), w.key.parse::<i64>().unwrap_or(i64::MAX)));
    out
}

/// Open apps, keeping the order they were first seen in (so the dock doesn't shuffle).
fn running(c: Compositor, before: &[String]) -> Vec<String> {
    let list: Vec<String> = match c {
        Compositor::Niri => json("niri", &["msg", "-j", "windows"]).and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|w| w.get("app_id").and_then(Value::as_str).map(String::from)).collect(),
        Compositor::Hyprland => json("hyprctl", &["-j", "clients"]).and_then(|v| v.as_array().cloned()).unwrap_or_default().iter().filter_map(|w| w.get("class").and_then(Value::as_str).map(String::from)).collect(),
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
        Compositor::Niri => json("niri", &["msg", "-j", "focused-window"]),
        Compositor::Hyprland => json("hyprctl", &["-j", "activewindow"]),
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

fn read_media() -> Option<Media> {
    let t = output("playerctl", &["metadata", "--format", "{{status}}\t{{playerName}}\t{{artist}}\t{{title}}"])?;
    let line = t.lines().next()?;
    let f: Vec<&str> = line.splitn(4, '\t').collect();
    if f.len() < 4 || f[0] == "Stopped" || (f[3].trim().is_empty() && f[2].trim().is_empty()) {
        return None;
    }
    Some(Media { playing: f[0] == "Playing", player: f[1].to_string(), artist: f[2].trim().to_string(), title: f[3].trim().to_string() })
}

fn wifi_radio() -> Option<bool> {
    let t = output("nmcli", &["radio", "wifi"])?;
    Some(t.trim() == "enabled")
}

/// Bluetooth's power (`timeout`, as bluetoothctl waits forever when bluetoothd isn't running).
fn bluetooth() -> Option<bool> {
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
    let _ = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

pub fn focus_workspace(c: Compositor, key: &str) {
    match c {
        Compositor::Niri => spawn("niri", &["msg", "action", "focus-workspace", key]),
        Compositor::Hyprland => spawn("hyprctl", &["dispatch", "workspace", key]),
        Compositor::Sway => spawn("swaymsg", &["workspace", "number", key]),
        Compositor::Other => {}
    }
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
            Compositor::Niri => spawn("niri", &["msg", "action", "quit", "--skip-confirmation"]),
            Compositor::Hyprland => spawn("hyprctl", &["dispatch", "exit"]),
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
