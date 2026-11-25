//! Sound devices and apps for the Sound popup: outputs and inputs (which one is the default,
//! each one's volume), and the apps playing, read from PipeWire's (or PulseAudio's) `pactl`
//! on a thread while the popup is open; choosing a device, volumes, mutes and moving an app to
//! another output are done there too.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

/// What a device is, for its symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DevKind {
    #[default]
    Speaker,
    Headphones,
    Headset,
    Screen,
    Bluetooth,
    Network,
    Mic,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Device {
    pub index: u32,
    /// pactl's name for it (what choosing it takes).
    pub name: String,
    /// Short: "Speaker", "HDMI 1", "WH-1000XM4".
    pub label: String,
    /// The card it's on, or what kind it is.
    pub sub: String,
    pub volume: u32,
    pub muted: bool,
    pub default: bool,
    pub kind: DevKind,
}

/// An app playing sound.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Stream {
    pub index: u32,
    pub app: String,
    /// Its icon name (or its program's name, which is often the same).
    pub icon: String,
    /// What it plays.
    pub title: String,
    pub volume: u32,
    pub muted: bool,
    /// The output it plays on (a Device's index).
    pub device: u32,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Audio {
    pub outputs: Vec<Device>,
    pub inputs: Vec<Device>,
    pub apps: Vec<Stream>,
    /// Read at least once since the popup opened.
    pub read: bool,
    pub error: Option<String>,
}

impl Audio {
    pub fn default_output(&self) -> Option<&Device> {
        self.outputs.iter().find(|d| d.default)
    }
    pub fn default_input(&self) -> Option<&Device> {
        self.inputs.iter().find(|d| d.default)
    }
}

/// Which volume.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Target {
    Output(u32),
    Input(u32),
    App(u32),
}

/// What the person asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// Make this the default output (or input); the apps playing move to it too.
    Default { input: bool, name: String },
    Volume(Target, u32),
    Mute(Target),
    /// An app to another output.
    Move { app: u32, device: u32 },
}

#[derive(Clone)]
pub struct AudioLive {
    state: Arc<Mutex<Audio>>,
    generation: Arc<AtomicU64>,
    want: Arc<AtomicBool>,
    poke: Arc<Mutex<Sender<Option<Job>>>>,
}

impl AudioLive {
    pub fn get(&self) -> Audio {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// The popup opened or closed: read only while it's open.
    pub fn want(&self, on: bool) {
        if self.want.swap(on, Ordering::Relaxed) != on && on {
            if let Ok(mut s) = self.state.lock() {
                s.read = false;
                s.error = None;
            }
            self.poke(None);
        }
    }
    pub fn run(&self, job: Job) {
        self.poke(Some(job));
    }
    fn poke(&self, j: Option<Job>) {
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(j);
        }
    }
    fn set(&self, a: Audio) {
        let changed = match self.state.lock() {
            Ok(mut s) => {
                let c = *s != a;
                *s = a;
                c
            }
            Err(_) => false,
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Start the thread; `wake` is called after each change.
pub fn start(wake: impl Fn() + Send + Sync + 'static) -> AudioLive {
    let (poke, poked) = channel::<Option<Job>>();
    let live = AudioLive { state: Arc::default(), generation: Arc::default(), want: Arc::default(), poke: Arc::new(Mutex::new(poke)) };
    let l = live.clone();
    std::thread::Builder::new()
        .name("rivet-shell audio".into())
        .spawn(move || {
            loop {
                if l.want.load(Ordering::Relaxed) {
                    let before = l.generation();
                    l.set(read());
                    if l.generation() != before {
                        wake();
                    }
                }
                // Once a second while open (devices come and go, apps start playing), at once
                // after something was asked.
                let wait = if l.want.load(Ordering::Relaxed) { Duration::from_millis(1000) } else { Duration::from_secs(3600) };
                match poked.recv_timeout(wait) {
                    Ok(first) => {
                        // A slider dragged sends many: only the last for each volume counts.
                        let mut jobs: Vec<Job> = first.into_iter().collect();
                        while let Ok(j) = poked.try_recv() {
                            jobs.extend(j);
                        }
                        let mut last: Vec<Job> = Vec::new();
                        for j in jobs {
                            if let Job::Volume(t, _) = &j {
                                last.retain(|x| !matches!(x, Job::Volume(u, _) if u == t));
                            }
                            last.push(j);
                        }
                        for j in last {
                            if let Err(e) = run(&j) {
                                let mut a = l.get();
                                a.error = Some(e);
                                l.set(a);
                            }
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .expect("a thread for sound devices");
    live
}

fn pactl(args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new("pactl").args(args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("pactl isn't installed ({e})"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn json(kind: &str) -> Vec<Value> {
    pactl(&["-f", "json", "list", kind]).ok().and_then(|t| serde_json::from_str::<Value>(&t).ok()).and_then(|v| v.as_array().cloned()).unwrap_or_default()
}

/// The average of the channels' volumes, in percent.
fn percent(v: &Value) -> u32 {
    let Some(ch) = v.get("volume").and_then(Value::as_object) else { return 0 };
    let all: Vec<u32> = ch.values().filter_map(|c| c.get("value_percent")?.as_str()?.trim_end_matches('%').trim().parse().ok()).collect();
    if all.is_empty() { 0 } else { all.iter().sum::<u32>() / all.len() as u32 }
}

fn prop<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get("properties").and_then(|p| p.get(key)).and_then(Value::as_str).unwrap_or("")
}

/// A device as the popup shows it (None: one not to show).
fn device(v: &Value, input: bool, default: &str) -> Option<Device> {
    let name = v.get("name")?.as_str()?.to_string();
    // A source that only listens to an output.
    if input && (prop(v, "device.class") == "monitor" || name.ends_with(".monitor")) {
        return None;
    }
    let description = v.get("description").and_then(Value::as_str).unwrap_or(&name).to_string();
    let card = prop(v, "device.description");
    let nick = prop(v, "node.nick");
    let is_default = name == default;
    // A screen's sound with no screen there, and the like.
    let port = v.get("active_port").and_then(Value::as_str).unwrap_or("");
    let gone = v.get("ports").and_then(Value::as_array).and_then(|ps| ps.iter().find(|p| p.get("name").and_then(Value::as_str) == Some(port))).and_then(|p| p.get("availability")).and_then(Value::as_str) == Some("not available");
    if gone && !is_default {
        return None;
    }
    let bus = prop(v, "device.bus");
    let what = format!("{nick} {description} {port}").to_lowercase();
    let kind = if bus == "bluetooth" {
        DevKind::Bluetooth
    } else if name.starts_with("raop") || name.starts_with("tunnel") || prop(v, "device.api") == "raop" {
        DevKind::Network
    } else if what.contains("hdmi") || what.contains("displayport") {
        DevKind::Screen
    } else if what.contains("headset") {
        DevKind::Headset
    } else if what.contains("headphone") {
        DevKind::Headphones
    } else if input {
        DevKind::Mic
    } else {
        DevKind::Speaker
    };
    // The short name: the node's nickname (a card's "Speaker"), else its description.
    let label = if !nick.is_empty() && bus != "bluetooth" { nick.to_string() } else { description };
    let sub = match kind {
        DevKind::Bluetooth => "Bluetooth".to_string(),
        DevKind::Network => "Over the network".to_string(),
        _ if !card.is_empty() && card != label => card.to_string(),
        _ => String::new(),
    };
    Some(Device { index: v.get("index")?.as_u64()? as u32, name, label, sub, volume: percent(v), muted: v.get("mute").and_then(Value::as_bool).unwrap_or(false), default: is_default, kind })
}

/// Read everything now.
pub fn read() -> Audio {
    let def_out = pactl(&["get-default-sink"]).map(|s| s.trim().to_string());
    let Ok(def_out) = def_out else {
        return Audio { read: true, error: Some("Couldn't read the sound devices (is pactl installed?)".into()), ..Default::default() };
    };
    let def_in = pactl(&["get-default-source"]).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut outputs: Vec<Device> = json("sinks").iter().filter_map(|v| device(v, false, &def_out)).collect();
    let mut inputs: Vec<Device> = json("sources").iter().filter_map(|v| device(v, true, &def_in)).collect();
    // The default first, then by name.
    for l in [&mut outputs, &mut inputs] {
        l.sort_by(|a, b| b.default.cmp(&a.default).then(a.label.to_lowercase().cmp(&b.label.to_lowercase())));
    }
    let apps = json("sink-inputs")
        .iter()
        .filter_map(|v| {
            // Event sounds (a click, a notification's chime) come and go: not listed.
            if prop(v, "media.role") == "event" {
                return None;
            }
            let bin = prop(v, "application.process.binary");
            let app = [prop(v, "application.name"), bin].into_iter().find(|s| !s.is_empty()).unwrap_or("An app").to_string();
            let icon = [prop(v, "application.icon_name"), bin].into_iter().find(|s| !s.is_empty()).unwrap_or("").to_lowercase();
            Some(Stream { index: v.get("index")?.as_u64()? as u32, app, icon, title: prop(v, "media.name").to_string(), volume: percent(v), muted: v.get("mute").and_then(Value::as_bool).unwrap_or(false), device: v.get("sink")?.as_u64()? as u32 })
        })
        .collect();
    Audio { outputs, inputs, apps, read: true, error: None }
}

fn run(j: &Job) -> Result<(), String> {
    match j {
        Job::Default { input, name } => {
            pactl(&[if *input { "set-default-source" } else { "set-default-sink" }, name])?;
            // The apps already playing follow (PipeWire moves them only when they never chose).
            if !input {
                for v in json("sink-inputs") {
                    if let Some(i) = v.get("index").and_then(Value::as_u64) {
                        let _ = pactl(&["move-sink-input", &i.to_string(), name]);
                    }
                }
            }
            Ok(())
        }
        Job::Volume(t, pct) => {
            let (cmd, i) = match t {
                Target::Output(i) => ("set-sink-volume", i),
                Target::Input(i) => ("set-source-volume", i),
                Target::App(i) => ("set-sink-input-volume", i),
            };
            pactl(&[cmd, &i.to_string(), &format!("{}%", (*pct).min(150))]).map(|_| ())
        }
        Job::Mute(t) => {
            let (cmd, i) = match t {
                Target::Output(i) => ("set-sink-mute", i),
                Target::Input(i) => ("set-source-mute", i),
                Target::App(i) => ("set-sink-input-mute", i),
            };
            pactl(&[cmd, &i.to_string(), "toggle"]).map(|_| ())
        }
        Job::Move { app, device } => pactl(&["move-sink-input", &app.to_string(), &device.to_string()]).map(|_| ()),
    }
}

/// Made-up devices and apps, for screenshots.
pub fn demo() -> Audio {
    let dev = |index, label: &str, sub: &str, volume, default, kind| Device { index, name: format!("dev{index}"), label: label.into(), sub: sub.into(), volume, muted: false, default, kind };
    let app = |index, app: &str, icon: &str, title: &str, volume, device| Stream { index, app: app.into(), icon: icon.into(), title: title.into(), volume, muted: false, device };
    Audio {
        outputs: vec![dev(1, "Speaker", "Built-in Audio", 45, true, DevKind::Speaker), dev(2, "WH-1000XM4", "Bluetooth", 60, false, DevKind::Bluetooth), dev(3, "HDMI 1", "Built-in Audio", 100, false, DevKind::Screen), dev(4, "Living Room TV", "Over the network", 70, false, DevKind::Network)],
        inputs: vec![dev(5, "Digital Microphone", "Built-in Audio", 80, true, DevKind::Mic), dev(6, "Headset Microphone", "Built-in Audio", 60, false, DevKind::Headset)],
        apps: vec![app(7, "Firefox", "firefox", "Low Tide — The Harbor Lights", 100, 1), app(8, "Spotify", "spotify", "", 72, 2)],
        read: true,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_device() {
        let v: Value = serde_json::from_str(r#"{"index":63,"name":"alsa_output.x.HiFi__Speaker__sink","description":"Meteor Lake-P HD Audio Controller Speaker","mute":false,
            "volume":{"front-left":{"value_percent":"40%"},"front-right":{"value_percent":"44%"}},"active_port":"[Out] Speaker",
            "ports":[{"name":"[Out] Speaker","availability":"availability unknown"}],
            "properties":{"node.nick":"Speaker","device.description":"Meteor Lake-P HD Audio Controller","device.bus":"pci"}}"#).unwrap();
        let d = device(&v, false, "alsa_output.x.HiFi__Speaker__sink").unwrap();
        assert_eq!((d.label.as_str(), d.sub.as_str(), d.volume, d.default, d.kind), ("Speaker", "Meteor Lake-P HD Audio Controller", 42, true, DevKind::Speaker));
        // A screen's output with nothing plugged in is left out (unless it's the default).
        let v: Value = serde_json::from_str(r#"{"index":62,"name":"hdmi1","description":"HDMI","mute":false,"volume":{},"active_port":"p",
            "ports":[{"name":"p","availability":"not available"}],"properties":{"node.nick":"HDMI 1"}}"#).unwrap();
        assert!(device(&v, false, "other").is_none());
        assert_eq!(device(&v, false, "hdmi1").map(|d| d.kind), Some(DevKind::Screen));
    }

    /// This computer's devices, printed (`cargo test reads_this_computer -- --ignored --nocapture`).
    #[test]
    #[ignore]
    fn reads_this_computer() {
        let a = read();
        for d in a.outputs.iter().chain(&a.inputs) {
            println!("{:>3} {:?} {:<28} {:<40} {}% {}{}", d.index, d.kind, d.label, d.sub, d.volume, if d.muted { "muted " } else { "" }, if d.default { "(default)" } else { "" });
        }
        for s in &a.apps {
            println!("app {} {:?} {:?} {}% on {}", s.index, s.app, s.title, s.volume, s.device);
        }
        assert!(a.read && a.error.is_none());
    }
}
