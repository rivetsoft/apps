//! The VPN page: the connections NetworkManager keeps, Tailscale and ZeroTier's networks
//! (`vpn.rs`), each with a way to turn it on and off, and which one the VPN block in quick
//! settings switches. Read on a thread, only while the page shows.

use std::sync::{Arc, Mutex};

use eframe::egui;

use super::connect::{quiet, row};
use super::{SettingsApp, controls, grid, hint, section};
use crate::glyph;
use crate::vpn::{self, Kind, Vpn, Vpns};

#[derive(Default)]
struct Shared {
    vpns: Vpns,
    /// Read at least once.
    read: bool,
    /// The one being switched (its key), or the step being allowed.
    busy: Option<String>,
    error: Option<String>,
}

pub(super) struct State {
    /// Screenshots: made-up VPNs, and nothing is run.
    demo: bool,
    shared: Arc<Mutex<Shared>>,
    read_at: f64,
    /// The ZeroTier network ID being typed.
    join: String,
}

impl State {
    pub(super) fn new(demo: bool) -> State {
        let shared = Shared { vpns: if demo { vpn::demo() } else { Vpns::default() }, read: demo, ..Default::default() };
        State { demo, shared: Arc::new(Mutex::new(shared)), read_at: -10.0, join: String::new() }
    }

    /// Each frame the page shows: read again every three seconds.
    pub(super) fn tend(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        if self.demo || now - self.read_at < 3.0 {
            return;
        }
        self.read_at = now;
        let (shared, ctx) = (self.shared.clone(), ctx.clone());
        std::thread::spawn(move || {
            let vpns = vpn::read();
            if let Ok(mut s) = shared.lock() {
                s.vpns = vpns;
                s.read = true;
            }
            ctx.request_repaint();
        });
    }

    /// Do `work` on a thread, showing `busy` meanwhile and what went wrong after, then read again.
    fn run(&mut self, ctx: &egui::Context, busy: &str, work: impl FnOnce() -> Result<(), String> + Send + 'static) {
        if self.demo {
            return;
        }
        if let Ok(mut s) = self.shared.lock() {
            s.busy = Some(busy.to_string());
            s.error = None;
        }
        let (shared, ctx) = (self.shared.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = work();
            let vpns = vpn::read();
            if let Ok(mut s) = shared.lock() {
                s.busy = None;
                s.error = result.err();
                s.vpns = vpns;
            }
            ctx.request_repaint();
        });
    }
}

/// One VPN: its name, its line, and Connect or Disconnect when it can be switched from here.
fn vpn_row(ui: &mut egui::Ui, v: &Vpn, busy: &Option<String>, labels: (&str, &str)) -> Option<bool> {
    let working = busy.as_deref() == Some(v.key().as_str());
    let detail = if working { if v.on { "Disconnecting…" } else { "Connecting…" } } else { v.detail.as_str() };
    let mut want = None;
    let on = v.on;
    row(ui, &|p, c, col| glyph::vpn(p, c, col, on), &v.name, detail, v.on, &mut |ui| {
        if !v.switch {
            return;
        }
        let (icon, label) = if v.on { (controls::Icon::Off, labels.1) } else { (controls::Icon::Forward, labels.0) };
        if controls::button_if(ui, busy.is_none(), icon, label).clicked() {
            want = Some(!v.on);
        }
    });
    want
}

/// What went wrong last, in the danger colour.
fn trouble(ui: &mut egui::Ui, error: &str) {
    ui.add_space(4.0);
    ui.add(egui::Label::new(egui::RichText::new(format!("{error}.")).size(12.0).color(rivet_look::theme::status(ui.ctx()).danger)).wrap());
}

impl SettingsApp {
    pub(super) fn vpn_page(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.vpn.tend(&ctx);
        let (all, read, busy, error) = match self.vpn.shared.lock() {
            Ok(s) => (s.vpns.clone(), s.read, s.busy.clone(), s.error.clone()),
            Err(_) => return,
        };
        if !read {
            section(ui, "Connections", &mut |ui| quiet(ui, "Reading…"));
            return;
        }
        // ZeroTier's networks are remembered, so one that was left can be joined again.
        for v in all.list.iter().filter(|v| v.kind == Kind::ZeroTier) {
            match self.s.zerotier_networks.iter_mut().find(|(id, _)| *id == v.id) {
                Some((_, name)) if v.name != v.id && *name != v.name => *name = v.name.clone(),
                Some(_) => {}
                None => self.s.zerotier_networks.push((v.id.clone(), if v.name == v.id { String::new() } else { v.name.clone() })),
            }
        }
        let of = |kind: Kind| all.list.iter().filter(move |v| v.kind == kind);
        let tailscale = of(Kind::Tailscale).next();
        let mut switch: Option<(String, bool)> = None;

        if all.list.is_empty() && !all.zerotier {
            section(ui, "Connections", &mut |ui| quiet(ui, "No VPNs yet. Add a WireGuard or OpenVPN connection to NetworkManager, or install Tailscale or ZeroTier."));
            return;
        }
        if of(Kind::Network).next().is_some() {
            section(ui, "Connections", &mut |ui| {
                for v in of(Kind::Network) {
                    if let Some(on) = vpn_row(ui, v, &busy, ("Connect", "Disconnect")) {
                        switch = Some((v.key(), on));
                    }
                }
            });
        }
        if let Some(v) = tailscale {
            section(ui, "Tailscale", &mut |ui| {
                if let Some(on) = vpn_row(ui, v, &busy, ("Connect", "Disconnect")) {
                    switch = Some((v.key(), on));
                }
                if all.tailscale_signed_out {
                    hint(ui, "Sign in by running \"tailscale login\" in a terminal.");
                }
                if error.as_deref() == Some(vpn::TAILSCALE_LOCKED) {
                    controls::rule(ui);
                    ui.horizontal(|ui| {
                        if controls::primary_if(ui, busy.is_none(), controls::Icon::Check, "Allow").clicked() {
                            self.vpn.run(&ctx, "allow", vpn::allow_tailscale);
                        }
                        hint(ui, "Rivet asks for your password once, to be allowed to turn Tailscale on and off.");
                    });
                }
                if v.on && !all.exits.is_empty() {
                    controls::rule(ui);
                    grid(ui, "shell-vpn-exit", &mut |ui| {
                        ui.label("Exit node").on_hover_text("Sends all of this computer's traffic through another computer on your tailnet.");
                        let cur = all.exits.iter().find(|e| e.in_use);
                        let mut pick: Option<String> = None;
                        egui::ComboBox::from_id_salt("shell-vpn-exit-pick").selected_text(cur.map_or("None", |e| e.name.as_str())).show_ui(ui, |ui| {
                            if ui.selectable_label(cur.is_none(), "None").clicked() && cur.is_some() {
                                pick = Some(String::new());
                            }
                            for e in &all.exits {
                                if ui.selectable_label(e.in_use, &e.name).clicked() && !e.in_use {
                                    pick = Some(e.address.clone());
                                }
                            }
                        }).response.accessible_name("Exit node");
                        if let Some(address) = pick {
                            self.vpn.run(&ctx, "exit", move || vpn::set_exit(&address));
                        }
                        ui.end_row();
                    });
                }
            });
        }
        if all.zerotier {
            section(ui, "ZeroTier", &mut |ui| {
                if all.zerotier_locked {
                    ui.horizontal(|ui| {
                        if controls::primary_if(ui, busy.is_none(), controls::Icon::Check, "Allow").clicked() {
                            self.vpn.run(&ctx, "allow", vpn::allow_zerotier);
                        }
                        hint(ui, "Rivet asks for your password once, to be allowed to see and join ZeroTier's networks.");
                    });
                    return;
                }
                let joined: Vec<&Vpn> = of(Kind::ZeroTier).collect();
                for v in &joined {
                    if let Some(on) = vpn_row(ui, v, &busy, ("Join", "Leave")) {
                        switch = Some((v.key(), on));
                    }
                }
                // The ones left: still here to join again, or to forget.
                let mut forget = None;
                for (id, name) in self.s.zerotier_networks.iter().filter(|(id, _)| !joined.iter().any(|v| v.id == *id)) {
                    let v = Vpn { kind: Kind::ZeroTier, id: id.clone(), name: if name.is_empty() { id.clone() } else { name.clone() }, on: false, switch: true, detail: "Not joined".into() };
                    let working = busy.as_deref() == Some(v.key().as_str());
                    row(ui, &|p, c, col| glyph::vpn(p, c, col, false), &v.name, if working { "Joining…" } else { &v.detail }, false, &mut |ui| {
                        if controls::button_if(ui, busy.is_none(), controls::Icon::Forward, "Join").clicked() {
                            switch = Some((v.key(), true));
                        }
                        if controls::button(ui, controls::Icon::Trash, "Forget").on_hover_text("Takes the network off this list.").clicked() {
                            forget = Some(id.clone());
                        }
                    });
                }
                if let Some(id) = forget {
                    self.s.zerotier_networks.retain(|(i, _)| *i != id);
                }
                if !joined.is_empty() || !self.s.zerotier_networks.is_empty() {
                    controls::rule(ui);
                }
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.vpn.join).hint_text("Network ID").desired_width(190.0)).accessible_name("Network ID");
                    let id = self.vpn.join.trim().to_lowercase();
                    if controls::button_if(ui, busy.is_none() && vpn::zerotier_id(&id), controls::Icon::Plus, "Join Network").clicked() {
                        switch = Some((format!("zerotier:{id}"), true));
                        self.vpn.join.clear();
                    }
                });
                hint(ui, "A network's ID is 16 letters and digits, from whoever runs the network.");
            });
        }
        if let Some(e) = error.as_deref().filter(|e| *e != vpn::TAILSCALE_LOCKED) {
            trouble(ui, e);
            ui.add_space(12.0);
        }
        // Which one the block in quick settings switches, when there is a choice.
        let mut choices: Vec<(String, String)> = all.list.iter().filter(|v| v.switch).map(|v| (v.key(), v.name.clone())).collect();
        for (id, name) in &self.s.zerotier_networks {
            let key = format!("zerotier:{id}");
            if !choices.iter().any(|(k, _)| *k == key) {
                choices.push((key, if name.is_empty() { id.clone() } else { name.clone() }));
            }
        }
        if choices.len() > 1 {
            section(ui, "Quick settings", &mut |ui| {
                grid(ui, "shell-vpn-quick", &mut |ui| {
                    ui.label("The VPN block switches");
                    let cur = choices.iter().find(|(k, _)| *k == self.s.quick_vpn).unwrap_or(&choices[0]).clone();
                    egui::ComboBox::from_id_salt("shell-vpn-quick-pick").selected_text(&cur.1).show_ui(ui, |ui| {
                        for (key, name) in &choices {
                            if ui.selectable_label(*key == cur.0, name).clicked() {
                                self.s.quick_vpn = key.clone();
                            }
                        }
                    }).response.accessible_name("The VPN block switches");
                    ui.end_row();
                });
            });
        }
        if let Some((key, on)) = switch {
            let k = key.clone();
            self.vpn.run(&ctx, &key, move || vpn::set(&k, on));
        }
    }
}
