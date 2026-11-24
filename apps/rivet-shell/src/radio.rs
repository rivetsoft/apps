//! Wi-Fi networks and Bluetooth devices for the Network and Bluetooth popups: listed on a
//! thread (NetworkManager's `nmcli`, BlueZ's `bluetoothctl`) only while one of them is open,
//! and connecting, pairing and forgetting done there too, so a slow or failing tool never holds
//! up drawing and what went wrong can be shown.

use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::mpsc::{RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct WifiNet {
    pub ssid: String,
    /// 0–100.
    pub signal: u8,
    pub secure: bool,
    pub active: bool,
    /// NetworkManager has a connection for it (no password needed again).
    pub saved: bool,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct BtDevice {
    pub mac: String,
    pub name: String,
    pub paired: bool,
    pub connected: bool,
}

/// How a wired connection stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Link {
    Connected,
    Connecting,
    /// A cable is in, and nothing is using it.
    #[default]
    Idle,
    /// No cable.
    Unplugged,
}

/// A wired network adapter.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Wired {
    /// The system's name for the adapter (what connecting takes).
    pub device: String,
    /// The connection in use on it, when there is one.
    pub name: String,
    pub link: Link,
    /// This computer's address on it, and the link's speed in megabits a second (0: not known).
    pub address: String,
    pub speed: u32,
}

/// What the popups show.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Radios {
    /// Strongest first; the one in use first of all.
    pub wifi: Vec<WifiNet>,
    pub wired: Vec<Wired>,
    pub bt: Vec<BtDevice>,
    /// Looked at least once since the popup opened.
    pub wifi_read: bool,
    pub bt_read: bool,
    pub bt_scanning: bool,
    /// The network or device being connected to (its SSID or address).
    pub busy: Option<String>,
    /// What went wrong last.
    pub error: Option<String>,
}

/// What the person asked for.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    WifiConnect { ssid: String, password: Option<String> },
    WifiDisconnect,
    WifiForget(String),
    WifiRescan,
    /// A wired adapter, by its device name.
    WiredConnect(String),
    WiredDisconnect(String),
    BtConnect(String),
    BtDisconnect(String),
    BtForget(String),
    BtScan,
}

/// Which list is wanted.
pub const NONE: u8 = 0;
pub const WIFI: u8 = 1;
pub const BT: u8 = 2;

enum Poke {
    Look,
    Do(Job),
}

#[derive(Clone)]
pub struct RadioLive {
    state: Arc<Mutex<Radios>>,
    generation: Arc<AtomicU64>,
    want: Arc<AtomicU8>,
    poke: Arc<Mutex<Sender<Poke>>>,
}

impl RadioLive {
    pub fn get(&self) -> Radios {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// A popup opened (WIFI or BT) or closed (NONE).
    pub fn want(&self, which: u8) {
        let before = self.want.swap(which, Ordering::Relaxed);
        if which != before {
            if let Ok(mut s) = self.state.lock() {
                s.error = None;
                s.wifi_read = false;
                s.bt_read = false;
            }
            match which {
                WIFI => self.run(Job::WifiRescan),
                BT => self.run(Job::BtScan),
                _ => {}
            }
        }
    }
    pub fn run(&self, job: Job) {
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(Poke::Do(job));
        }
    }
    fn look(&self) {
        if let Ok(p) = self.poke.lock() {
            let _ = p.send(Poke::Look);
        }
    }
    fn edit(&self, f: impl FnOnce(&mut Radios)) {
        let changed = match self.state.lock() {
            Ok(mut s) => {
                let before = s.clone();
                f(&mut s);
                *s != before
            }
            Err(_) => false,
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// Start the thread; `wake` is called after each change.
pub fn start(wake: impl Fn() + Send + Sync + 'static) -> RadioLive {
    let (poke, poked) = channel::<Poke>();
    let live = RadioLive { state: Arc::default(), generation: Arc::default(), want: Arc::default(), poke: Arc::new(Mutex::new(poke)) };
    let wake = Arc::new(wake);
    let l = live.clone();
    std::thread::Builder::new()
        .name("rivet-shell radios".into())
        .spawn(move || {
            loop {
                let which = l.want.load(Ordering::Relaxed);
                if which != NONE {
                    let before = l.generation();
                    if which == WIFI {
                        let (list, wired) = (wifi_list(), wired_list());
                        l.edit(|s| {
                            s.wifi = list;
                            s.wired = wired;
                            s.wifi_read = true;
                        });
                    } else {
                        let list = bt_list();
                        l.edit(|s| {
                            s.bt = list;
                            s.bt_read = true;
                        });
                    }
                    if l.generation() != before {
                        wake();
                    }
                }
                let wait = if which == NONE { Duration::from_secs(3600) } else { Duration::from_millis(2500) };
                match poked.recv_timeout(wait) {
                    Ok(Poke::Do(job)) => {
                        // The work itself on its own thread: pairing can take half a minute.
                        let (l2, w2) = (l.clone(), wake.clone());
                        std::thread::spawn(move || {
                            work(&l2, job, &*w2);
                            l2.look();
                            w2();
                        });
                    }
                    Ok(Poke::Look) | Err(RecvTimeoutError::Timeout) => {}
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
        })
        .ok();
    live
}

/// A command's output and whether it worked (stderr when it didn't).
fn run(cmd: &str, args: &[&str]) -> Result<String, String> {
    let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("{cmd}: {e}"))?;
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if out.status.success() {
        Ok(text)
    } else {
        let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
        Err(if err.is_empty() { text.trim().to_string() } else { err })
    }
}

/// bluetoothctl, never waiting forever (it does when bluetoothd isn't running).
fn btctl(secs: u32, args: &[&str]) -> Result<String, String> {
    let secs = secs.to_string();
    let mut all = vec![secs.as_str(), "bluetoothctl"];
    all.extend_from_slice(args);
    let out = run("timeout", &all)?;
    // bluetoothctl says it failed on stdout and still exits 0 now and then.
    match out.lines().find(|l| l.contains("Failed") || l.contains("not available") || l.contains("org.bluez.Error")) {
        Some(l) => Err(l.trim().to_string()),
        None => Ok(out),
    }
}

/// nmcli's terse output splits on ':' with '\:' for a colon inside a field.
fn split_terse(line: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut esc = false;
    for c in line.chars() {
        match (esc, c) {
            (true, _) => {
                out.last_mut().unwrap().push(c);
                esc = false;
            }
            (false, '\\') => esc = true,
            (false, ':') => out.push(String::new()),
            _ => out.last_mut().unwrap().push(c),
        }
    }
    out
}

fn wifi_list() -> Vec<WifiNet> {
    let saved: Vec<String> = run("nmcli", &["-t", "-f", "NAME,TYPE", "connection", "show"])
        .unwrap_or_default()
        .lines()
        .map(split_terse)
        .filter(|f| f.get(1).is_some_and(|t| t == "802-11-wireless"))
        .map(|f| f[0].clone())
        .collect();
    let mut out: Vec<WifiNet> = Vec::new();
    for line in run("nmcli", &["-t", "-f", "IN-USE,SSID,SIGNAL,SECURITY", "device", "wifi", "list", "--rescan", "no"]).unwrap_or_default().lines() {
        let f = split_terse(line);
        if f.len() < 4 || f[1].is_empty() {
            continue;
        }
        let n = WifiNet { active: f[0] == "*", ssid: f[1].clone(), signal: f[2].parse().unwrap_or(0), secure: !f[3].is_empty() && f[3] != "--", saved: saved.contains(&f[1]) };
        // The same network on several access points or bands: once, at its best.
        match out.iter_mut().find(|o| o.ssid == n.ssid) {
            Some(o) => {
                o.active |= n.active;
                o.signal = o.signal.max(n.signal);
                o.secure |= n.secure;
            }
            None => out.push(n),
        }
    }
    out.sort_by_key(|n| (!n.active, !n.saved, std::cmp::Reverse(n.signal)));
    out
}

fn bt_list() -> Vec<BtDevice> {
    let devices = |filter: Option<&str>| -> Vec<(String, String)> {
        let mut args = vec!["devices"];
        args.extend(filter);
        btctl(3, &args)
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let rest = l.trim().strip_prefix("Device ")?;
                let (mac, name) = rest.split_once(' ').unwrap_or((rest, ""));
                Some((mac.to_string(), name.trim().to_string()))
            })
            .collect()
    };
    let paired = devices(Some("Paired"));
    let connected = devices(Some("Connected"));
    let mut out: Vec<BtDevice> = devices(None)
        .into_iter()
        // Unnamed ones only show their address (with dashes for a name): leave those out.
        .filter(|(mac, name)| paired.iter().any(|p| p.0 == *mac) || (!name.is_empty() && name.replace('-', ":") != *mac))
        .map(|(mac, name)| BtDevice { paired: paired.iter().any(|p| p.0 == mac), connected: connected.iter().any(|c| c.0 == mac), mac, name })
        .collect();
    out.sort_by_key(|d| (!d.connected, !d.paired, d.name.to_lowercase()));
    out
}

/// The wired adapters in nmcli's terse `DEVICE,TYPE,STATE,CONNECTION` listing, without the
/// ones NetworkManager leaves alone.
fn wired_from(listing: &str) -> Vec<Wired> {
    listing
        .lines()
        .map(split_terse)
        .filter(|f| f.len() >= 4 && f[1] == "ethernet" && f[2] != "unmanaged")
        .map(|f| {
            let link = match f[2].as_str() {
                s if s.starts_with("connected") => Link::Connected,
                s if s.starts_with("connecting") => Link::Connecting,
                "unavailable" => Link::Unplugged,
                _ => Link::Idle,
            };
            Wired { device: f[0].clone(), name: f[3].clone(), link, ..Default::default() }
        })
        .collect()
}

fn wired_list() -> Vec<Wired> {
    let mut out = wired_from(&run("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device"]).unwrap_or_default());
    for w in out.iter_mut().filter(|w| w.link == Link::Connected) {
        let shown = run("nmcli", &["-t", "-f", "IP4.ADDRESS", "device", "show", &w.device]).unwrap_or_default();
        w.address = shown.lines().find_map(|l| l.split_once(':')).map(|(_, a)| a.split('/').next().unwrap_or(a).to_string()).unwrap_or_default();
        w.speed = std::fs::read_to_string(format!("/sys/class/net/{}/speed", w.device)).ok().and_then(|s| s.trim().parse::<i64>().ok()).filter(|s| *s > 0).unwrap_or(0) as u32;
    }
    out
}

fn wifi_device() -> Option<String> {
    run("nmcli", &["-t", "-f", "DEVICE,TYPE,STATE", "device"]).ok()?.lines().map(split_terse).find(|f| f.len() >= 3 && f[1] == "wifi" && f[2].starts_with("connected")).map(|f| f[0].clone())
}

fn work(l: &RadioLive, job: Job, wake: &dyn Fn()) {
    let busy = match &job {
        Job::WifiConnect { ssid, .. } => Some(ssid.clone()),
        Job::BtConnect(mac) | Job::BtDisconnect(mac) => Some(mac.clone()),
        Job::WiredConnect(dev) | Job::WiredDisconnect(dev) => Some(dev.clone()),
        _ => None,
    };
    if busy.is_some() {
        l.edit(|s| {
            s.busy = busy.clone();
            s.error = None;
        });
        wake();
    }
    let result: Result<(), String> = match job {
        Job::WifiRescan => run("nmcli", &["device", "wifi", "rescan"]).map(|_| ()).or(Ok(())),
        Job::WifiConnect { ssid, password } => {
            let mut args = vec!["device", "wifi", "connect", ssid.as_str()];
            if let Some(p) = password.as_deref() {
                args.extend(["password", p]);
            }
            run("nmcli", &args).map(|_| ()).map_err(|e| {
                // A new network that didn't take its password leaves a profile that will never work.
                if password.is_some() {
                    let _ = run("nmcli", &["connection", "delete", "id", &ssid]);
                }
                if e.contains("Secrets were required") || e.contains("802-11-wireless-security.psk") {
                    format!("Couldn't join {ssid}: wrong password?")
                } else {
                    format!("Couldn't join {ssid}: {}", short(&e))
                }
            })
        }
        Job::WifiDisconnect => match wifi_device() {
            Some(dev) => run("nmcli", &["device", "disconnect", &dev]).map(|_| ()).map_err(|e| short(&e)),
            None => Ok(()),
        },
        Job::WiredConnect(dev) => run("nmcli", &["device", "connect", &dev]).map(|_| ()).map_err(|e| format!("Couldn't connect: {}", short(&e))),
        Job::WiredDisconnect(dev) => run("nmcli", &["device", "disconnect", &dev]).map(|_| ()).map_err(|e| short(&e)),
        Job::WifiForget(ssid) => run("nmcli", &["connection", "delete", "id", &ssid]).map(|_| ()).map_err(|e| short(&e)),
        Job::BtScan => {
            l.edit(|s| s.bt_scanning = true);
            wake();
            let r = btctl(14, &["--timeout", "12", "scan", "on"]).map(|_| ());
            l.edit(|s| s.bt_scanning = false);
            // timeout's exit when bluetoothctl overstays isn't a failure worth showing.
            r.or(Ok(()))
        }
        Job::BtConnect(mac) => {
            let known = btctl(3, &["info", &mac]).unwrap_or_default();
            let paired = known.lines().any(|l| l.trim() == "Paired: yes");
            let mut r = Ok(());
            if !paired {
                r = btctl(30, &["--agent", "NoInputNoOutput", "pair", &mac]).map(|_| ());
                if r.is_ok() {
                    let _ = btctl(5, &["trust", &mac]);
                }
            }
            r.and_then(|_| btctl(20, &["connect", &mac]).map(|_| ())).map_err(|e| format!("Couldn't connect: {}", short(&e)))
        }
        Job::BtDisconnect(mac) => btctl(10, &["disconnect", &mac]).map(|_| ()).map_err(|e| short(&e)),
        Job::BtForget(mac) => btctl(10, &["remove", &mac]).map(|_| ()).map_err(|e| short(&e)),
    };
    l.edit(|s| {
        s.busy = None;
        if let Err(e) = result {
            s.error = Some(e);
        }
    });
}

/// The first line of an error, without the tool's "Error: " in front.
fn short(e: &str) -> String {
    let line = e.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or("it didn't work");
    line.trim_start_matches("Error: ").trim_end_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terse_keeps_escaped_colons() {
        assert_eq!(split_terse(r"*:My\:Net:80:WPA2"), ["*", "My:Net", "80", "WPA2"]);
        assert_eq!(split_terse(" ::60:"), [" ", "", "60", ""]);
    }

    #[test]
    fn wired_adapters() {
        let w = wired_from("wlp0s20f3:wifi:connected:Home\nenp3s0:ethernet:connected:Wired connection 1\nenp4s0:ethernet:unavailable:\nveth1:ethernet:unmanaged:\nenp5s0:ethernet:disconnected:");
        assert_eq!(w.iter().map(|w| (w.device.as_str(), w.link)).collect::<Vec<_>>(), [("enp3s0", Link::Connected), ("enp4s0", Link::Unplugged), ("enp5s0", Link::Idle)]);
        assert_eq!(w[0].name, "Wired connection 1");
    }

    #[test]
    fn short_error() {
        assert_eq!(short("Error: Connection activation failed: Secrets were required.\nmore"), "Connection activation failed: Secrets were required");
    }
}

#[cfg(test)]
mod live_tests {
    #[test]
    #[ignore = "reads this computer's networks and devices"]
    fn lists_here() {
        eprintln!("{:#?}\n{:#?}", super::wifi_list(), super::bt_list());
    }
}
