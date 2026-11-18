//! Panels: as many as wanted, each on an edge of every screen or of one, with three zones (left,
//! center, right) of items. Items are named by id ("clock", "volume"…), so plugins can add their
//! own later ("plugin:NAME"); an id the shell doesn't know takes no room.
//!
//! In the settings file each panel is one line:
//! `panel.0=edge=top;floating=true;screen=;left=launcher,workspaces;center=clock;right=power`.

use eframe::egui::{Pos2, Rect, Vec2};

use crate::config::{Along, DockHide, Edge};

/// The panel's bar height, and the room around a floating one.
pub const BAR: f32 = 36.0;
pub const MARGIN: f32 = 6.0;
/// The most a panel's offset from its edge, or its room before windows, can be set to.
pub const SPACE_MAX: u32 = 64;
/// How thick a bar can be made, and the room between its items, in points.
pub const SIZE: std::ops::RangeInclusive<u32> = 24..=72;
pub const SPACING: f32 = 2.0;
pub const SPACING_MAX: u32 = 24;

/// The zones, left to right; and top to bottom on a panel down the screen's side.
pub const ZONES: [&str; 3] = ["Left", "Center", "Right"];
pub const ZONES_DOWN: [&str; 3] = ["Top", "Middle", "Bottom"];
/// The shortest a panel can be made, in percent of its edge.
pub const MIN_LENGTH: u32 = 20;
/// Hidden, a panel keeps this much of the screen's edge to bring it back.
pub const STRIP: f32 = 3.0;

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
    ("network", "Network", "Wi-Fi or wired; click to pick a network"),
    ("bluetooth", "Bluetooth", "Click to connect or pair devices"),
    ("battery", "Battery", "Click for quick settings"),
    ("ai", "AI Usage", "Claude, Codex and other plans' limits"),
    ("dock", "Dock", "Pinned and open apps, the same as the dock's"),
    ("tray", "System Tray", "Apps' status icons (chat, sync, games…)"),
    ("notifications", "Notifications", "A bell with a dot for new ones"),
    ("quick-settings", "Quick Settings", "Sliders, toggles, the notification history"),
    ("power", "Power Menu", "Lock, suspend, log out, restart, shut down"),
    ("spacer", "Spacer", "Empty room between items; as many as wanted, each its own size"),
];

/// A spacer's size in points: its id is `spacer:24` (as many on a panel as wanted).
pub const SPACER: std::ops::RangeInclusive<u32> = 2..=600;
pub fn spacer(id: &str) -> Option<u32> {
    let rest = id.strip_prefix("spacer")?;
    match rest.strip_prefix(':') {
        Some(n) => n.parse::<u32>().ok().map(|n| n.clamp(*SPACER.start(), *SPACER.end())),
        None => rest.is_empty().then_some(16),
    }
}

/// An item's name for people.
pub fn label(id: &str) -> String {
    if spacer(id).is_some() {
        return "Spacer".into();
    }
    match ITEMS.iter().find(|i| i.0 == id) {
        Some(i) => i.1.to_string(),
        None => match id.strip_prefix("plugin:") {
            // Its own name when it's installed.
            Some(p) => match crate::plugins::found_cached().into_iter().find(|m| m.id == p) {
                Some(m) => m.name,
                None => format!("{p} (plugin)"),
            },
            None => format!("{id} (unknown)"),
        },
    }
}

/// A colour for a group of items: left to the shell, one of the theme's, or its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Paint {
    /// A background: a soft lift off the panel; text: whatever reads on the background.
    #[default]
    Auto,
    /// The theme's surface: what panels and windows are filled with.
    Surface,
    Primary,
    Secondary,
    Tertiary,
    Custom([u8; 3]),
}

impl Paint {
    pub fn text(self) -> String {
        match self {
            Paint::Auto => "auto".into(),
            Paint::Surface => "surface".into(),
            Paint::Primary => "primary".into(),
            Paint::Secondary => "secondary".into(),
            Paint::Tertiary => "tertiary".into(),
            Paint::Custom([r, g, b]) => format!("{r:02x}{g:02x}{b:02x}"),
        }
    }
    pub fn parse(v: &str) -> Paint {
        match v.trim() {
            "surface" => Paint::Surface,
            "primary" => Paint::Primary,
            "secondary" => Paint::Secondary,
            "tertiary" => Paint::Tertiary,
            h if h.len() == 6 => u32::from_str_radix(h, 16).map(|n| Paint::Custom([(n >> 16) as u8, (n >> 8) as u8, n as u8])).unwrap_or(Paint::Auto),
            _ => Paint::Auto,
        }
    }
}

/// Items drawn together on a panel, in their own colours: the ones of a group that sit side
/// by side in a zone share a background. A group can show only its first few items, the rest
/// folded behind an arrow.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Group {
    /// The ids of its items (each item is in one group at most).
    pub items: Vec<String>,
    /// The background (None: none, only the text's colour changes).
    pub bg: Option<Paint>,
    /// The text and symbols.
    pub fg: Paint,
    /// How many items show before the rest fold away (0: all of them).
    pub show: u32,
    /// The background fades along the panel to a darker shade of its colour.
    pub gradient: bool,
}

/// The shade a group's gradient fades to: the colour, darker.
pub fn darker(c: [u8; 3]) -> [u8; 3] {
    c.map(|x| (x as f32 * 0.58).round() as u8)
}

impl Group {
    /// `a+b+c/bg:primary/fg:auto/show:2/fade:true`
    pub fn text(&self) -> String {
        let mut t = self.items.join("+");
        t.push_str(&format!("/bg:{}", self.bg.map(Paint::text).unwrap_or_else(|| "none".into())));
        if self.fg != Paint::Auto {
            t.push_str(&format!("/fg:{}", self.fg.text()));
        }
        if self.show > 0 {
            t.push_str(&format!("/show:{}", self.show));
        }
        if self.gradient {
            t.push_str("/fade:true");
        }
        t
    }
    pub fn parse(v: &str) -> Group {
        let mut parts = v.split('/');
        let mut g = Group { items: parts.next().unwrap_or("").split('+').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(), bg: Some(Paint::Auto), ..Group::default() };
        for part in parts {
            let Some((k, x)) = part.split_once(':') else { continue };
            match k.trim() {
                "bg" => g.bg = (x.trim() != "none").then(|| Paint::parse(x)),
                "fg" => g.fg = Paint::parse(x),
                "show" => g.show = x.trim().parse().unwrap_or(0).min(12),
                "fade" => g.gradient = x.trim() == "true",
                _ => {}
            }
        }
        g
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PanelCfg {
    pub edge: Edge,
    /// A floating bar with rounded corners, or one that spans the edge.
    pub floating: bool,
    /// The screen it's on (an output name such as DP-1); empty: every screen.
    pub screen: String,
    /// Item ids in the left, center and right zones, each in order left to right (top to
    /// bottom on a side panel).
    pub zones: [Vec<String>; 3],
    /// How much of its edge it takes, in percent, and where along the edge a shorter one sits.
    pub length: u32,
    pub along: Along,
    /// Whether it slides away (like the dock).
    pub hide: DockHide,
    /// Its own look, over Appearance's.
    pub look: crate::style::Style,
    /// Items grouped, each group in its own colours.
    pub groups: Vec<Group>,
    /// Groups that sit side by side join into one shape.
    pub fuse: bool,
    /// How far a floating bar sits from the screen's edge, and the room kept between the bar
    /// and windows, in points (None: 6 and 6 floating; none edge to edge).
    pub offset: Option<u32>,
    pub gap: Option<u32>,
    /// Its own font, a family's name (empty: Appearance's).
    pub font: String,
    /// The bar's thickness (its height, or a side panel's width) and the room between its
    /// items, in points (None: 36 and 2).
    pub size: Option<u32>,
    pub spacing: Option<u32>,
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
            zones: [list(&["launcher", "workspaces", "window-title"]), list(&["clock"]), list(&["media", "tray", "volume", "brightness", "network", "bluetooth", "battery", "notifications", "quick-settings", "power"])],
            length: 100,
            along: Along::Center,
            hide: DockHide::Never,
            look: Default::default(),
            groups: Vec::new(),
            fuse: false,
            offset: None,
            gap: None,
            font: String::new(),
            size: None,
            spacing: None,
        }
    }
}

impl PanelCfg {
    /// A new panel's start: the clock in the middle.
    pub fn new_bottom() -> PanelCfg {
        PanelCfg { edge: Edge::Bottom, zones: [Vec::new(), list(&["clock"]), Vec::new()], ..PanelCfg::bare() }
    }

    /// Whether it shows on this screen ("" when the screen's name isn't known: shown).
    pub fn on(&self, output: &str) -> bool {
        self.screen.is_empty() || output.is_empty() || self.screen == output
    }

    /// A panel with nothing on it.
    pub fn bare() -> PanelCfg {
        PanelCfg { zones: Default::default(), ..PanelCfg::default() }
    }

    /// The surface's thickness: its height, or a side panel's width.
    pub fn height(&self) -> f32 {
        self.off() + self.bar() + self.gap()
    }

    /// The bar's own thickness.
    pub fn bar(&self) -> f32 {
        self.size.map(|s| s.clamp(*SIZE.start(), *SIZE.end()) as f32).unwrap_or(BAR)
    }

    /// The room between its items.
    pub fn spacing(&self) -> f32 {
        self.spacing.map(|s| s.min(SPACING_MAX) as f32).unwrap_or(SPACING)
    }

    /// How far the bar sits from the screen's edge (an edge to edge one sits on it).
    pub fn off(&self) -> f32 {
        if self.floating { self.offset.map(|o| o.min(SPACE_MAX) as f32).unwrap_or(MARGIN) } else { 0.0 }
    }

    /// The room between the bar and windows.
    pub fn gap(&self) -> f32 {
        self.gap.map(|g| g.min(SPACE_MAX) as f32).unwrap_or(if self.floating { MARGIN } else { 0.0 })
    }

    /// The room it keeps from windows (none when it hides).
    pub fn reserved(&self) -> f32 {
        if self.hide == DockHide::Never { self.height() } else { 0.0 }
    }

    pub fn vertical(&self) -> bool {
        self.edge.vertical()
    }

    /// The bar within its surface (which spans the edge): against the edge, a floating one
    /// with room around it, as long as its length asks and placed along the edge.
    pub fn bar_rect(&self, full: Rect) -> Rect {
        // In from the edge by its offset (and as much at its ends), the gap on its other side.
        let (o, g) = (self.off(), self.gap());
        let inner = match self.edge {
            Edge::Top => Rect::from_min_max(full.min + Vec2::new(o, o), full.max - Vec2::new(o, g)),
            Edge::Bottom => Rect::from_min_max(full.min + Vec2::new(o, g), full.max - Vec2::new(o, o)),
            Edge::Left => Rect::from_min_max(full.min + Vec2::new(o, o), full.max - Vec2::new(g, o)),
            Edge::Right => Rect::from_min_max(full.min + Vec2::new(g, o), full.max - Vec2::new(o, o)),
        };
        let whole = if self.vertical() { inner.height() } else { inner.width() };
        let len = (whole * self.length.clamp(MIN_LENGTH, 100) as f32 / 100.0).round();
        let off = match self.along {
            Along::Start => 0.0,
            Along::Center => ((whole - len) / 2.0).round(),
            Along::End => whole - len,
        };
        if self.vertical() {
            Rect::from_min_size(Pos2::new(inner.left(), inner.top() + off), Vec2::new(inner.width(), len))
        } else {
            Rect::from_min_size(Pos2::new(inner.left() + off, inner.top()), Vec2::new(len, inner.height()))
        }
    }

    /// The part of its surface that takes the pointer: the bar out to the screen's edge, or,
    /// hidden, a thin strip along the edge (None: all of it).
    pub fn pointer_rect(&self, full: Rect, hidden: bool) -> Option<Rect> {
        if !hidden && self.length >= 100 {
            return None;
        }
        let bar = self.bar_rect(full);
        let mut r = if self.vertical() { Rect::from_x_y_ranges(full.x_range(), bar.y_range()) } else { Rect::from_x_y_ranges(bar.x_range(), full.y_range()) };
        if hidden {
            match self.edge {
                Edge::Top => r.max.y = r.min.y + STRIP,
                Edge::Bottom => r.min.y = r.max.y - STRIP,
                Edge::Left => r.max.x = r.min.x + STRIP,
                Edge::Right => r.min.x = r.max.x - STRIP,
            }
        }
        Some(r)
    }

    /// How far it's moved as it slides away (`slide` 0 shown, 1 out of sight).
    pub fn slide(&self, slide: f32) -> Vec2 {
        let d = slide * self.height();
        match self.edge {
            Edge::Top => Vec2::new(0.0, -d),
            Edge::Bottom => Vec2::new(0.0, d),
            Edge::Left => Vec2::new(-d, 0.0),
            Edge::Right => Vec2::new(d, 0.0),
        }
    }

    pub fn has(&self, id: &str) -> bool {
        self.zones.iter().any(|z| z.iter().any(|i| i == id))
    }

    /// The group an item is in.
    pub fn group_of(&self, id: &str) -> Option<usize> {
        self.groups.iter().position(|g| g.items.iter().any(|i| i == id))
    }

    /// Put an item in a group (None: in none), beside the group's other items in its zone.
    pub fn set_group(&mut self, id: &str, group: Option<usize>) {
        for g in &mut self.groups {
            g.items.retain(|i| i != id);
        }
        if let Some(g) = group.and_then(|g| self.groups.get_mut(g)) {
            g.items.push(id.to_string());
            let members = g.items.clone();
            if let Some(zone) = self.zones.iter_mut().find(|z| z.iter().any(|i| i == id))
                && let Some(from) = zone.iter().position(|i| i == id)
            {
                // Beside the group's others in this zone: after the last of them, unless it
                // already touches one.
                let member = |k: usize| zone.get(k).is_some_and(|i| i != id && members.contains(i));
                let touches = member(from + 1) || (from > 0 && member(from - 1));
                let last = (0..zone.len()).rev().find(|k| member(*k));
                if let Some(last) = last
                    && !touches
                {
                    let it = zone.remove(from);
                    zone.insert(if from < last { last } else { last + 1 }, it);
                }
            }
        }
        self.tidy_groups();
    }

    /// Drop items no longer on the panel from their groups, and groups left with none.
    pub fn tidy_groups(&mut self) {
        let zones = &self.zones;
        for g in &mut self.groups {
            g.items.retain(|i| zones.iter().any(|z| z.contains(i)));
        }
        self.groups.retain(|g| !g.items.is_empty());
    }

    pub fn text(&self) -> String {
        let mut t = format!(
            "edge={};floating={};screen={};left={};center={};right={}",
            self.edge.name(),
            self.floating,
            self.screen.replace([';', '\n'], ""),
            self.zones[0].join(","),
            self.zones[1].join(","),
            self.zones[2].join(",")
        );
        if self.length < 100 {
            t.push_str(&format!(";length={};along={}", self.length.max(MIN_LENGTH), self.along.name()));
        }
        if self.hide != DockHide::Never {
            t.push_str(&format!(";hide={}", self.hide.name()));
        }
        if !self.look.is_empty() {
            t.push_str(&format!(";look={}", self.look.text(':', ',')));
        }
        for g in self.groups.iter().filter(|g| !g.items.is_empty()) {
            t.push_str(&format!(";group={}", g.text().replace([';', '\n'], "")));
        }
        if self.fuse {
            t.push_str(";fuse=true");
        }
        if let Some(o) = self.offset {
            t.push_str(&format!(";offset={o}"));
        }
        if let Some(g) = self.gap {
            t.push_str(&format!(";gap={g}"));
        }
        if let Some(s) = self.size {
            t.push_str(&format!(";size={s}"));
        }
        if let Some(s) = self.spacing {
            t.push_str(&format!(";spacing={s}"));
        }
        if !self.font.is_empty() {
            t.push_str(&format!(";font={}", crate::config::font_name(&self.font)));
        }
        t
    }

    pub fn parse(v: &str) -> PanelCfg {
        let mut p = PanelCfg::bare();
        let items = |s: &str| s.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect::<Vec<_>>();
        for part in v.split(';') {
            let Some((k, val)) = part.split_once('=') else { continue };
            let val = val.trim();
            match k.trim() {
                "edge" => p.edge = Edge::from(val),
                "length" => p.length = val.parse().unwrap_or(100).clamp(MIN_LENGTH, 100),
                "along" => p.along = Along::from(val),
                "hide" => p.hide = DockHide::from(val),
                "floating" => p.floating = val != "false",
                "screen" => p.screen = val.to_string(),
                "left" => p.zones[0] = items(val),
                "center" => p.zones[1] = items(val),
                "right" => p.zones[2] = items(val),
                "look" => p.look = crate::style::Style::parse(val, ':', ','),
                "group" => p.groups.push(Group::parse(val)),
                "fuse" => p.fuse = val == "true",
                "offset" => p.offset = val.parse::<u32>().ok().map(|o| o.min(SPACE_MAX)),
                "gap" => p.gap = val.parse::<u32>().ok().map(|g| g.min(SPACE_MAX)),
                "font" => p.font = crate::config::font_name(val),
                "size" => p.size = val.parse::<u32>().ok().map(|s| s.clamp(*SIZE.start(), *SIZE.end())),
                "spacing" => p.spacing = val.parse::<u32>().ok().map(|s| s.min(SPACING_MAX)),
                _ => {}
            }
        }
        p
    }
}

/// The room panels keep along an edge of a screen (ones that hide keep none).
pub fn room(panels: &[PanelCfg], output: &str, edge: Edge) -> f32 {
    panels.iter().filter(|p| p.edge == edge && p.on(output)).map(PanelCfg::reserved).sum()
}

/// How far from its edge panel `idx` ends (panels on the same edge stack in order, the first
/// nearest the edge): where its popups open.
pub fn reach(panels: &[PanelCfg], idx: usize, output: &str) -> f32 {
    let Some(me) = panels.get(idx) else { return 0.0 };
    // One that hides keeps no room, so it sits inside every panel that does.
    if me.hide != DockHide::Never {
        return room(panels, output, me.edge) + me.height();
    }
    panels.iter().take(idx + 1).filter(|p| p.edge == me.edge && p.on(output)).map(PanelCfg::reserved).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_stacks() {
        let p = PanelCfg { screen: "DP-1".into(), look: crate::style::Style { radius: Some(6.0), fill: Some(crate::style::Role::Primary), ..Default::default() }, ..PanelCfg::default() };
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
        assert_eq!((spacer("spacer:24"), spacer("spacer"), spacer("spacer:9999"), spacer("spacers"), label("spacer:24").as_str()), (Some(24), Some(16), Some(600), None, "Spacer"));
        // Groups: kept in the panel's line; an item joins beside the group's others.
        let mut g = PanelCfg::default();
        g.groups.push(Group { items: list(&["volume", "brightness"]), bg: Some(Paint::Primary), fg: Paint::Custom([255, 0, 128]), show: 1, gradient: true });
        g.groups.push(Group { items: list(&["clock"]), bg: None, fg: Paint::Tertiary, show: 0, gradient: false });
        g.fuse = true;
        (g.offset, g.gap) = (Some(14), Some(0));
        assert_eq!((g.height(), g.bar_rect(Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, 50.0)))), (50.0, Rect::from_min_max(Pos2::new(14.0, 14.0), Pos2::new(986.0, 50.0))));
        g.groups.push(Group { items: list(&["power"]), bg: Some(Paint::Surface), ..Group::default() });
        assert_eq!(PanelCfg::parse(&g.text()), g);
        g.set_group("power", Some(0));
        assert_eq!(g.group_of("power"), Some(0));
        let right = &g.zones[2];
        assert_eq!(right.iter().position(|i| i == "power"), right.iter().position(|i| i == "brightness").map(|k| k + 1));
        g.set_group("clock", None);
        assert_eq!(g.groups.len(), 1);
        // A side panel, 60% of its edge at the end, that hides: it keeps no room, and opens its
        // popups from inside the panels that do.
        let side = PanelCfg { edge: Edge::Left, length: 60, along: Along::End, hide: DockHide::Auto, ..PanelCfg::new_bottom() };
        assert_eq!(PanelCfg::parse(&side.text()), side);
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(BAR + MARGIN * 2.0, 1012.0));
        let bar = side.bar_rect(full);
        assert_eq!((bar.width(), bar.height(), bar.bottom()), (BAR, 600.0, 1012.0 - MARGIN));
        assert_eq!(side.pointer_rect(full, true).map(|r| (r.width(), r.height())), Some((STRIP, 600.0)));
        assert_eq!(PanelCfg::default().pointer_rect(full, false), None);
        let two = [PanelCfg { edge: Edge::Left, ..PanelCfg::default() }, side];
        assert_eq!(room(&two, "", Edge::Left), BAR + MARGIN * 2.0);
        assert_eq!(reach(&two, 1, ""), 2.0 * (BAR + MARGIN * 2.0));
    }
}
