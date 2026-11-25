//! The Network and Bluetooth pages: Wi-Fi networks and Bluetooth devices as lists, with the same
//! listing and joining the panel's popups use (`radio.rs`), read only while one of the two pages
//! is showing.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, Color32, Pos2, Sense, Vec2};

use super::{SettingsApp, controls, hint, section};
use crate::glyph;
use crate::radio::{self, BtDevice, Job, Link, RadioLive, Radios, WifiNet, Wired};

/// Whether Wi-Fi and Bluetooth are on (None: not known yet, or there is no such radio).
#[derive(Clone, Copy, Default)]
struct Power {
    wifi: Option<bool>,
    bt: Option<bool>,
}

pub(super) struct State {
    /// The thread that lists and connects; started the first time a page needs it.
    live: Option<RadioLive>,
    /// Screenshots: made-up networks and devices, and nothing is run.
    demo: Option<Radios>,
    power: Arc<Mutex<Power>>,
    /// When the switches were last read, and when one was last pressed (what was pressed shows
    /// until the radio has had time to follow).
    read_at: f64,
    pressed: Option<(u8, bool, f64)>,
    /// The network a password is being typed for, and what is typed.
    ask: Option<(String, String)>,
}

impl State {
    pub(super) fn new(demo: bool) -> State {
        State { live: None, demo: demo.then(crate::shot::demo_radios), power: Arc::new(Mutex::new(if demo { Power { wifi: Some(true), bt: Some(true) } } else { Power::default() })), read_at: -10.0, pressed: None, ask: None }
    }

    /// Each frame: list what the page showing needs (`radio::WIFI`, `radio::BT`), or stop
    /// (`radio::NONE`).
    pub(super) fn tend(&mut self, ctx: &egui::Context, which: u8) {
        if self.demo.is_some() {
            return;
        }
        if which == radio::NONE {
            if let Some(l) = &self.live {
                l.want(radio::NONE);
            }
            return;
        }
        let live = self.live.get_or_insert_with(|| {
            let ctx = ctx.clone();
            radio::start(move || ctx.request_repaint())
        });
        live.want(which);
        // The switches, every two seconds, on a thread: the tools that answer can be slow.
        let now = ctx.input(|i| i.time);
        if now - self.read_at >= 2.0 {
            self.read_at = now;
            let (power, ctx) = (self.power.clone(), ctx.clone());
            std::thread::spawn(move || {
                let read = if which == radio::WIFI { crate::sys::wifi_radio() } else { crate::sys::bluetooth() };
                if let Ok(mut p) = power.lock() {
                    let slot = if which == radio::WIFI { &mut p.wifi } else { &mut p.bt };
                    if *slot != read {
                        *slot = read;
                        ctx.request_repaint();
                    }
                }
            });
        }
    }

    fn radios(&self) -> Radios {
        self.demo.clone().or_else(|| self.live.as_ref().map(|l| l.get())).unwrap_or_default()
    }

    fn run(&self, job: Job) {
        if let Some(l) = &self.live {
            l.run(job);
        }
    }

    /// Whether a radio is on: what was pressed a moment ago, or what was read.
    fn on(&self, which: u8, now: f64) -> Option<bool> {
        if let Some((w, on, at)) = self.pressed
            && w == which
            && now - at < 4.0
        {
            return Some(on);
        }
        let p = self.power.lock().map(|p| *p).unwrap_or_default();
        if which == radio::WIFI { p.wifi } else { p.bt }
    }

    fn press(&mut self, which: u8, on: bool, now: f64) {
        self.pressed = Some((which, on, now));
        if self.demo.is_none() {
            if which == radio::WIFI { crate::sys::set_wifi(on) } else { crate::sys::set_bluetooth(on) }
            if on {
                self.run(if which == radio::WIFI { Job::WifiRescan } else { Job::BtScan });
            }
        }
    }
}

/// One network or device: its symbol, its name over a quieter line, and its buttons at the far end.
pub(super) fn row(ui: &mut egui::Ui, icon: &dyn Fn(&egui::Painter, Pos2, Color32), title: &str, sub: &str, strong: bool, buttons: &mut dyn FnMut(&mut egui::Ui)) {
    let v = ui.visuals().clone();
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(26.0, 38.0), Sense::hover());
        icon(ui.painter(), r.center(), if strong { v.selection.bg_fill } else { v.text_color() });
        // The buttons first, from the far end, so the words take what is left and are cut short
        // with … when that isn't enough.
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            buttons(ui);
            ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                ui.add_space(if sub.is_empty() { 10.0 } else { 3.0 });
                let name = egui::RichText::new(title).size(13.5);
                ui.add(egui::Label::new(if strong { name.strong() } else { name }).truncate());
                if !sub.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(sub).size(11.5).color(v.weak_text_color())).truncate()).on_hover_text(sub);
                }
            });
        });
    });
}

/// A quiet line where a list would be: nothing found, still looking, the radio is off.
pub(super) fn quiet(ui: &mut egui::Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(egui::RichText::new(text).size(12.5).color(ui.visuals().weak_text_color()));
}

/// What went wrong last, in the danger colour.
fn trouble(ui: &mut egui::Ui, error: &Option<String>) {
    if let Some(e) = error {
        ui.add_space(4.0);
        ui.add(egui::Label::new(egui::RichText::new(format!("{e}.")).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
    }
}

impl SettingsApp {
    pub(super) fn network_page(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let radios = self.conn.radios();
        let wifi = self.conn.on(radio::WIFI, now);
        if !radios.wired.is_empty() {
            section(ui, "Wired", &mut |ui| {
                for w in &radios.wired {
                    self.wired_row(ui, w, &radios.busy, radios.wired.len() > 1);
                }
                // Without Wi-Fi there is no card below to say what went wrong.
                if wifi.is_none() {
                    trouble(ui, &radios.error);
                }
            });
            // A computer with a cable and no Wi-Fi has nothing more to show.
            if wifi.is_none() {
                return;
            }
        }
        section(ui, "Wi-Fi", &mut |ui| {
            let Some(mut on) = wifi else {
                quiet(ui, "Wi-Fi isn't available on this computer.");
                return;
            };
            if controls::master(ui, &mut on, "Wi-Fi").changed() {
                self.conn.press(radio::WIFI, on, now);
            }
            controls::rule(ui);
            if !on {
                quiet(ui, "Turn Wi-Fi on to see networks.");
                return;
            }
            if radios.wifi.is_empty() {
                quiet(ui, if radios.wifi_read { "No networks found." } else { "Looking for networks…" });
            }
            for n in &radios.wifi {
                self.wifi_row(ui, n, &radios.busy);
            }
            trouble(ui, &radios.error);
            ui.add_space(6.0);
            if controls::button(ui, controls::Icon::Reset, "Look Again").clicked() {
                self.conn.run(Job::WifiRescan);
            }
        });
    }

    fn wired_row(&mut self, ui: &mut egui::Ui, w: &Wired, busy: &Option<String>, several: bool) {
        let working = busy.as_deref() == Some(w.device.as_str());
        let sub = match w.link {
            Link::Connected if working => "Disconnecting…".to_string(),
            Link::Connected => {
                let mut parts = vec!["Connected".to_string()];
                if w.speed > 0 {
                    parts.push(if w.speed >= 1000 { format!("{} Gb/s", w.speed as f32 / 1000.0) } else { format!("{} Mb/s", w.speed) });
                }
                if !w.address.is_empty() {
                    parts.push(w.address.clone());
                }
                parts.join(" · ")
            }
            Link::Connecting => "Connecting…".to_string(),
            Link::Idle if working => "Connecting…".to_string(),
            Link::Idle => "Not connected".to_string(),
            Link::Unplugged => "No cable plugged in".to_string(),
        };
        // The connection's name when one is in use. Otherwise what it is, with the adapter's own
        // name only when there are several to tell apart.
        let title = match (w.name.is_empty(), several) {
            (false, _) => w.name.clone(),
            (true, false) => "Ethernet".to_string(),
            (true, true) => format!("Ethernet ({})", w.device),
        };
        let mut job = None;
        row(ui, &|p, c, col| glyph::wired(p, c, col), &title, &sub, w.link == Link::Connected, &mut |ui| match w.link {
            Link::Connected | Link::Connecting => {
                if controls::button_if(ui, !working, controls::Icon::Off, "Disconnect").clicked() {
                    job = Some(Job::WiredDisconnect(w.device.clone()));
                }
            }
            Link::Idle => {
                if controls::button_if(ui, !working, controls::Icon::Forward, "Connect").clicked() {
                    job = Some(Job::WiredConnect(w.device.clone()));
                }
            }
            Link::Unplugged => {}
        });
        if let Some(j) = job {
            self.conn.run(j);
        }
    }

    fn wifi_row(&mut self, ui: &mut egui::Ui, n: &WifiNet, busy: &Option<String>) {
        let joining = busy.as_deref() == Some(n.ssid.as_str());
        let sub = match (joining, n.active, n.saved, n.secure) {
            (true, ..) => "Connecting…",
            (_, true, ..) => "Connected",
            (_, _, true, _) => "Saved",
            (_, _, _, true) => "Secured",
            _ => "Open",
        };
        let signal = n.signal;
        let mut job = None;
        let mut ask = false;
        row(ui, &|p, c, col| glyph::wifi_signal(p, c, col, signal), &n.ssid, sub, n.active, &mut |ui| {
            if n.active {
                if controls::button(ui, controls::Icon::Off, "Disconnect").clicked() {
                    job = Some(Job::WifiDisconnect);
                }
            } else if controls::button_if(ui, !joining, controls::Icon::Forward, "Join").clicked() {
                // A new secured network asks for its password first.
                if n.secure && !n.saved {
                    ask = true;
                } else {
                    job = Some(Job::WifiConnect { ssid: n.ssid.clone(), password: None });
                }
            }
            if n.saved && controls::button(ui, controls::Icon::Trash, "Forget").on_hover_text("Forgets the password, so joining asks for it again.").clicked() {
                job = Some(Job::WifiForget(n.ssid.clone()));
            }
        });
        if ask {
            self.conn.ask = Some((n.ssid.clone(), String::new()));
        }
        if self.conn.ask.as_ref().is_some_and(|(ssid, _)| *ssid == n.ssid) {
            let mut done = false;
            ui.horizontal(|ui| {
                ui.add_space(34.0);
                let Some((_, typed)) = self.conn.ask.as_mut() else { return };
                let field = ui.add(egui::TextEdit::singleline(typed).password(true).hint_text("Password").desired_width(220.0));
                if ask {
                    field.request_focus();
                }
                let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (controls::primary_if(ui, !typed.is_empty(), controls::Icon::Forward, "Join").clicked() || enter) && !typed.is_empty() {
                    job = Some(Job::WifiConnect { ssid: n.ssid.clone(), password: Some(typed.clone()) });
                    done = true;
                }
                if controls::button(ui, controls::Icon::Off, "Cancel").clicked() {
                    done = true;
                }
            });
            if done {
                self.conn.ask = None;
            }
        }
        if let Some(j) = job {
            self.conn.run(j);
        }
    }

    pub(super) fn bluetooth_page(&mut self, ui: &mut egui::Ui) {
        let now = ui.input(|i| i.time);
        let radios = self.conn.radios();
        let Some(mut on) = self.conn.on(radio::BT, now) else {
            section(ui, "Bluetooth", &mut |ui| quiet(ui, "No Bluetooth adapter was found on this computer."));
            return;
        };
        section(ui, "Bluetooth", &mut |ui| {
            if controls::master(ui, &mut on, "Bluetooth").changed() {
                self.conn.press(radio::BT, on, now);
            }
            if !on {
                controls::rule(ui);
                quiet(ui, "Turn Bluetooth on to see devices.");
            }
        });
        if !on {
            return;
        }
        let (mine, others): (Vec<&BtDevice>, Vec<&BtDevice>) = radios.bt.iter().partition(|d| d.paired);
        section(ui, "My devices", &mut |ui| {
            if mine.is_empty() {
                quiet(ui, if radios.bt_read { "None paired yet. Pair one from the devices nearby." } else { "Looking for devices…" });
            }
            for d in &mine {
                self.bt_row(ui, d, &radios.busy);
            }
            trouble(ui, &radios.error);
        });
        section(ui, "Devices nearby", &mut |ui| {
            if others.is_empty() {
                quiet(ui, if radios.bt_scanning || !radios.bt_read { "Looking for devices…" } else { "None found. Make the device discoverable, then look again." });
            }
            for d in &others {
                self.bt_row(ui, d, &radios.busy);
            }
            ui.add_space(6.0);
            if controls::button_if(ui, !radios.bt_scanning, controls::Icon::Reset, if radios.bt_scanning { "Looking…" } else { "Look Again" }).clicked() {
                self.conn.run(Job::BtScan);
            }
            hint(ui, "Pairing here works for devices that don't ask for a code.");
        });
    }

    fn bt_row(&mut self, ui: &mut egui::Ui, d: &BtDevice, busy: &Option<String>) {
        let working = busy.as_deref() == Some(d.mac.as_str());
        let sub = match (working, d.connected, d.paired) {
            (true, true, _) => "Disconnecting…",
            (true, _, true) => "Connecting…",
            (true, ..) => "Pairing…",
            (_, true, _) => "Connected",
            (_, _, true) => "Not connected",
            _ => "",
        };
        let mut job = None;
        row(ui, &|p, c, col| glyph::bluetooth(p, c, col, true), &d.name, sub, d.connected, &mut |ui| {
            if d.connected {
                if controls::button_if(ui, !working, controls::Icon::Off, "Disconnect").clicked() {
                    job = Some(Job::BtDisconnect(d.mac.clone()));
                }
            } else if controls::button_if(ui, !working, controls::Icon::Forward, if d.paired { "Connect" } else { "Pair" }).clicked() {
                job = Some(Job::BtConnect(d.mac.clone()));
            }
            if d.paired && controls::button(ui, controls::Icon::Trash, "Forget").on_hover_text("Removes the pairing, so the device has to be paired again.").clicked() {
                job = Some(Job::BtForget(d.mac.clone()));
            }
        });
        if let Some(j) = job {
            self.conn.run(j);
        }
    }
}
