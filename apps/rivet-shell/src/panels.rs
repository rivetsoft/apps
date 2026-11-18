//! Panels: as many as wanted, each on an edge of every screen or of one, with three zones (left,
//! center, right) of items. Items are named by id ("clock", "volume"…), so plugins can add their
//! own later ("plugin:NAME"); an id the shell doesn't know takes no room.
//!
//! In the settings file each panel is one line:
//! `panel.0=edge=top;floating=true;screen=;left=launcher,workspaces;center=clock;right=power`.

use crate::config::Edge;

/// The panel's bar height, and the room around a floating one.
pub const BAR: f32 = 36.0;
pub const MARGIN: f32 = 6.0;

/// The zones, left to right.
pub const ZONES: [&str; 3] = ["Left", "Center", "Right"];

/// The items the shell has: id, name, what it is.
pub const ITEMS: &[(&str, &str, &str)] = &[
    ("launcher", "Apps", "Opens the launcher"),
    ("workspaces", "Workspaces", "niri, Hyprland or Sway's workspaces"),
    ("window-title", "Window Title", "The focused window's title"),
    ("clock", "Clock", "Click for a calendar"),
    ("media", "Now Playing", "What's playing; click to pause"),
    ("system", "Processor and Memory", "How busy the computer is"),
    ("volume", "Volume", "Click to mute, scroll to change"),
    ("brightness", "Brightness", "Scroll to change (laptops)"),
    ("network", "Network", "Wi-Fi or wired; click for quick settings"),
    ("battery", "Battery", "Click for quick settings"),
    ("notifications", "Notifications", "A bell with a dot for new ones"),
    ("quick-settings", "Quick Settings", "Sliders, toggles, the notification history"),
    ("power", "Power Menu", "Lock, suspend, log out, restart, shut down"),
];

/// An item's name for people.
pub fn label(id: &str) -> String {
    match ITEMS.iter().find(|i| i.0 == id) {
        Some(i) => i.1.to_string(),
        None => match id.strip_prefix("plugin:") {
            Some(p) => format!("{p} (plugin)"),
            None => format!("{id} (unknown)"),
        },
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelCfg {
    pub edge: Edge,
    /// A floating bar with rounded corners, or one that spans the edge.
    pub floating: bool,
    /// The screen it's on (an output name such as DP-1); empty: every screen.
    pub screen: String,
    /// Item ids in the left, center and right zones, each in order left to right.
    pub zones: [Vec<String>; 3],
}

fn list(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

impl Default for PanelCfg {
    /// The panel the shell starts with.
    fn default() -> Self {
        PanelCfg {
            edge: Edge::Top,
            floating: true,
            screen: String::new(),
            zones: [list(&["launcher", "workspaces", "window-title"]), list(&["clock"]), list(&["media", "volume", "brightness", "network", "battery", "notifications", "quick-settings", "power"])],
        }
    }
}

impl PanelCfg {
    /// A new panel's start: the clock in the middle.
    pub fn new_bottom() -> PanelCfg {
        PanelCfg { edge: Edge::Bottom, floating: true, screen: String::new(), zones: [Vec::new(), list(&["clock"]), Vec::new()] }
    }

    /// Whether it shows on this screen ("" when the screen's name isn't known: shown).
    pub fn on(&self, output: &str) -> bool {
        self.screen.is_empty() || output.is_empty() || self.screen == output
    }

    /// The surface's height (and the room it keeps from windows).
    pub fn height(&self) -> f32 {
        if self.floating { BAR + MARGIN * 2.0 } else { BAR }
    }

    pub fn has(&self, id: &str) -> bool {
        self.zones.iter().any(|z| z.iter().any(|i| i == id))
    }

    pub fn text(&self) -> String {
        format!(
            "edge={};floating={};screen={};left={};center={};right={}",
            if self.edge == Edge::Bottom { "bottom" } else { "top" },
            self.floating,
            self.screen.replace([';', '\n'], ""),
            self.zones[0].join(","),
            self.zones[1].join(","),
            self.zones[2].join(",")
        )
    }

    pub fn parse(v: &str) -> PanelCfg {
        let mut p = PanelCfg { edge: Edge::Top, floating: true, screen: String::new(), zones: Default::default() };
        let items = |s: &str| s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>();
        for part in v.split(';') {
            let Some((k, val)) = part.split_once('=') else { continue };
            let val = val.trim();
            match k.trim() {
                "edge" => p.edge = if val == "bottom" { Edge::Bottom } else { Edge::Top },
                "floating" => p.floating = val != "false",
                "screen" => p.screen = val.to_string(),
                "left" => p.zones[0] = items(val),
                "center" => p.zones[1] = items(val),
                "right" => p.zones[2] = items(val),
                _ => {}
            }
        }
        p
    }
}

/// The room panels take along an edge of a screen.
pub fn room(panels: &[PanelCfg], output: &str, edge: Edge) -> f32 {
    panels.iter().filter(|p| p.edge == edge && p.on(output)).map(PanelCfg::height).sum()
}

/// How far from its edge panel `idx` ends (panels on the same edge stack in order, the first
/// nearest the edge): where its popups open.
pub fn reach(panels: &[PanelCfg], idx: usize, output: &str) -> f32 {
    let Some(me) = panels.get(idx) else { return 0.0 };
    panels.iter().take(idx + 1).filter(|p| p.edge == me.edge && p.on(output)).map(PanelCfg::height).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_stacks() {
        let p = PanelCfg { screen: "DP-1".into(), ..PanelCfg::default() };
        assert_eq!(PanelCfg::parse(&p.text()), p);
        let b = PanelCfg { floating: false, ..PanelCfg::new_bottom() };
        assert_eq!(PanelCfg::parse(&b.text()), b);
        let all = [PanelCfg::default(), b.clone(), PanelCfg { screen: "HDMI-A-1".into(), ..PanelCfg::default() }];
        assert!(all[2].on("HDMI-A-1") && !all[2].on("DP-1") && all[2].on(""));
        assert_eq!(room(&all, "DP-1", Edge::Top), BAR + MARGIN * 2.0);
        assert_eq!(room(&all, "HDMI-A-1", Edge::Top), 2.0 * (BAR + MARGIN * 2.0));
        assert_eq!(reach(&all, 2, "HDMI-A-1"), 2.0 * (BAR + MARGIN * 2.0));
        assert_eq!(reach(&all, 1, "DP-1"), BAR);
        assert_eq!(label("plugin:weather"), "weather (plugin)");
    }
}
