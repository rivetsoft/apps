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
/// A rolled-up sub panel's strip is thicker: it's found by resting the pointer on it, just
/// past its host, where there's no screen edge to stop the pointer.
pub const SUB_STRIP: f32 = 14.0;

/// The items the shell has: id, name, what it is.
pub const ITEMS: &[(&str, &str, &str)] = &[
    ("launcher", "Apps", "Opens the launcher"),
    ("workspaces", "Workspaces", "niri, Hyprland or Sway's workspaces"),
    ("window-title", "Window Title", "The focused window's title"),
    ("clock", "Clock", "Click for a calendar"),
    ("media", "Now Playing", "What's playing, with its cover. Click for the player."),
    ("system", "Processor and Memory", "How busy the computer is"),
    ("volume", "Volume", "Click to mute, scroll to change the volume"),
    ("brightness", "Brightness", "Scroll to change the brightness (laptops)"),
    ("network", "Network", "Wi-Fi or wired. Click to pick a network."),
    ("bluetooth", "Bluetooth", "Click to connect or pair devices"),
    ("battery", "Battery", "Click for quick settings"),
    ("ai", "AI Usage", "Claude, Codex and other plans' limits"),
    ("weather", "Weather", "The sky and the temperature. Click for the forecast."),
    ("user", "User", "Your picture. Click to lock, log out or shut down."),
    ("dock", "Dock", "Pinned and open apps, the same as the dock's"),
    ("tray", "System Tray", "Apps' status icons (chat, sync, games…)"),
    ("notifications", "Notifications", "A bell, with a dot when there are new notifications"),
    ("clipboard", "Clipboard", "What you copied lately. Click to copy something again."),
    ("quick-settings", "Quick Settings", "Sliders, toggles and the notification history"),
    ("power", "Power Menu", "Lock, suspend, log out, restart, shut down"),
    ("spacer", "Spacer", "Blank space between items. Add as many as you like, each with its own width."),
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
    if let Some(name) = id.strip_prefix("sub:") {
        return name.to_string();
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
    /// How tall its background is (how wide, on a side panel), in points. None: it follows the
    /// panel, as tall as the panel's items.
    pub height: Option<u32>,
}

/// The least a group's own height can be.
pub const GROUP_MIN: u32 = 12;

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
        if let Some(h) = self.height {
            t.push_str(&format!("/h:{h}"));
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
                "h" => g.height = x.trim().parse::<u32>().ok().map(|h| h.clamp(GROUP_MIN, *SIZE.end())),
                _ => {}
            }
        }
        g
    }
}

/// What makes a panel a sub panel: one kept rolled up under another panel (its host) until it's
/// asked for, for things wanted close by but not always in view. It's asked for by its item on
/// a panel (`sub:NAME`), by `rivet-shell msg subpanel NAME` (a key binding), or by resting the
/// pointer along the host's inner edge. It takes its edge and screen from its host.
#[derive(Clone, Debug, PartialEq)]
pub struct Sub {
    /// What it's called: "Shortcuts".
    pub name: String,
    /// The panel it's attached to, by its place in the list (None: to none, so it isn't shown).
    pub host: Option<usize>,
    /// Left open until it's closed (true), or rolling back up by itself (false).
    pub stay: bool,
    /// Open now, for one that stays.
    pub open: bool,
    /// Resting the pointer along its edge of the host opens it.
    pub reveal: bool,
    /// How long one that rolls up stays after the pointer leaves it, in seconds.
    pub secs: f32,
    /// As long as its host's bar and in line with it, whatever its own length says.
    pub fit: bool,
    /// Joined to its host like a drawer pulled out of it: no room between the two, square
    /// corners on the host's side, and filled in under the host's rounded corners.
    pub drawer: bool,
    /// The handle shown past the host while it's rolled up (when resting the pointer opens it):
    /// whether it shows, how long and thick it is in points, where along the host it sits, and
    /// its colour (None: the panel's own).
    pub handle: Handle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Handle {
    pub show: bool,
    pub length: u32,
    pub thick: u32,
    pub along: Along,
    pub color: Option<crate::style::Role>,
    pub shape: HandleShape,
    /// How solid it is: 0.15 to 1.
    pub opacity: f32,
    /// How far from the host's bar it sits, in points (below zero: over the bar's edge).
    pub offset: i32,
}

/// What a handle is drawn as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum HandleShape {
    /// A rounded bar.
    #[default]
    Bar,
    /// A plain line with square ends.
    Line,
    /// An arrow pointing the way the sub panel rolls out.
    Arrow,
    /// A row of dots.
    Dots,
}

impl HandleShape {
    pub const ALL: [(HandleShape, &'static str); 4] = [(HandleShape::Bar, "Bar"), (HandleShape::Line, "Line"), (HandleShape::Arrow, "Arrow"), (HandleShape::Dots, "Dots")];
    fn name(self) -> &'static str {
        match self {
            HandleShape::Bar => "bar",
            HandleShape::Line => "line",
            HandleShape::Arrow => "arrow",
            HandleShape::Dots => "dots",
        }
    }
    fn from(v: &str) -> HandleShape {
        match v {
            "line" => HandleShape::Line,
            "arrow" => HandleShape::Arrow,
            "dots" => HandleShape::Dots,
            _ => HandleShape::Bar,
        }
    }
}

pub const HANDLE_OFFSET: std::ops::RangeInclusive<i32> = -8..=40;
pub const HANDLE_OPACITY: std::ops::RangeInclusive<f32> = 0.15..=1.0;

/// How long and thick a handle can be made.
pub const HANDLE_LENGTH: std::ops::RangeInclusive<u32> = 12..=400;
pub const HANDLE_THICK: std::ops::RangeInclusive<u32> = 2..=10;

impl Default for Handle {
    fn default() -> Self {
        Handle { show: true, length: 44, thick: 5, along: Along::Center, color: None, shape: HandleShape::Bar, opacity: 1.0, offset: 7 }
    }
}

impl Handle {
    /// `true,44,5,center,auto,bar,1,7`
    fn text(&self) -> String {
        format!("{},{},{},{},{},{},{},{}", self.show, self.length, self.thick, self.along.name(), self.color.map_or("auto", |r| r.name()), self.shape.name(), self.opacity, self.offset)
    }

    /// How much room it takes: along the panel, and across (an arrow is as tall as it's pointed).
    pub fn size(&self) -> Vec2 {
        let (len, thick) = (self.length as f32, self.thick as f32);
        match self.shape {
            HandleShape::Arrow => Vec2::new(len, (len * 0.28).clamp(4.0, 14.0) + thick * 0.5),
            _ => Vec2::new(len, thick),
        }
    }
    fn parse(v: &str) -> Handle {
        let d = Handle::default();
        let mut it = v.split(',').map(str::trim);
        Handle {
            show: it.next().is_none_or(|x| x != "false"),
            length: it.next().and_then(|x| x.parse().ok()).unwrap_or(d.length).clamp(*HANDLE_LENGTH.start(), *HANDLE_LENGTH.end()),
            thick: it.next().and_then(|x| x.parse().ok()).unwrap_or(d.thick).clamp(*HANDLE_THICK.start(), *HANDLE_THICK.end()),
            along: Along::from(it.next().unwrap_or("center")),
            color: it.next().and_then(crate::style::Role::from).filter(|r| *r != crate::style::Role::Surface),
            shape: HandleShape::from(it.next().unwrap_or("bar")),
            opacity: it.next().and_then(|x| x.parse::<f32>().ok()).unwrap_or(1.0).clamp(*HANDLE_OPACITY.start(), *HANDLE_OPACITY.end()),
            offset: it.next().and_then(|x| x.parse::<i32>().ok()).unwrap_or(d.offset).clamp(*HANDLE_OFFSET.start(), *HANDLE_OFFSET.end()),
        }
    }
}

/// How long a sub panel can be asked to stay, in seconds.
pub const SUB_SECS: std::ops::RangeInclusive<f32> = 1.0..=30.0;

impl Sub {
    pub fn new(name: &str, host: usize) -> Sub {
        Sub { name: name.to_string(), host: Some(host), stay: false, open: false, reveal: true, secs: 4.0, fit: true, drawer: true, handle: Handle::default() }
    }

    /// The panel item that opens it.
    pub fn item(&self) -> String {
        format!("sub:{}", self.name)
    }
}

/// A sub panel's name as it can be kept in the settings file and in an item's id.
pub fn sub_name(v: &str) -> String {
    v.replace([';', ',', '=', '+', '/', ':', '\n'], " ").split_whitespace().collect::<Vec<_>>().join(" ")
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
    /// Set on a sub panel (see `Sub`).
    pub sub: Option<Sub>,
    /// A drawer's reach up under its host's rounded corners, in points: the host's radius.
    /// Worked out from the look when the settings are read (`Settings::tidy_panels`), not kept.
    pub lead: f32,
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
            sub: None,
            lead: 0.0,
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
        // A sub panel attached to nothing has nowhere to be.
        self.sub.as_ref().is_none_or(|s| s.host.is_some()) && (self.screen.is_empty() || output.is_empty() || self.screen == output)
    }

    /// A sub panel that's been left open: it keeps its room, as its host does.
    pub fn kept(&self) -> bool {
        self.sub.as_ref().is_some_and(|s| s.stay && s.open)
    }

    /// A panel with nothing on it.
    pub fn bare() -> PanelCfg {
        PanelCfg { zones: Default::default(), ..PanelCfg::default() }
    }

    /// The surface's thickness: its height, or a side panel's width.
    pub fn height(&self) -> f32 {
        self.off() + self.bar() + self.gap()
    }

    /// A sub panel joined to its host like a drawer.
    pub fn drawer(&self) -> bool {
        self.sub.as_ref().is_some_and(|s| s.drawer)
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
        // A drawer starts under its host's corners.
        if self.drawer() {
            return self.lead;
        }
        if self.floating { self.offset.map(|o| o.min(SPACE_MAX) as f32).unwrap_or(MARGIN) } else { 0.0 }
    }

    /// The room between the bar and windows.
    pub fn gap(&self) -> f32 {
        self.gap.map(|g| g.min(SPACE_MAX) as f32).unwrap_or(if self.floating { MARGIN } else { 0.0 })
    }

    /// The room it keeps from windows (none when it hides; a sub panel's is kept by its host).
    pub fn reserved(&self) -> f32 {
        if self.sub.is_none() && self.hide == DockHide::Never { self.height() } else { 0.0 }
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
    #[cfg(test)]
    pub fn pointer_rect(&self, full: Rect, hidden: bool) -> Option<Rect> {
        self.pointer_rect_for(full, self.bar_rect(full), self.length, hidden, 0.0, SUB_STRIP)
    }

    /// The same for a bar placed by `bar_rect` below (a sub panel's follows its host's).
    /// `skip`: how far into its surface a sub panel's strip begins (clear of its host's surface).
    /// `sub_strip`: how thick that strip is (enough to take in its handle).
    fn pointer_rect_for(&self, full: Rect, bar: Rect, length: u32, hidden: bool, skip: f32, sub_strip: f32) -> Option<Rect> {
        if !hidden && length >= 100 {
            return None;
        }
        // A rolled-up sub panel that only opens when asked takes no pointer at all.
        if hidden && self.sub.as_ref().is_some_and(|s| s.stay || !s.reveal) {
            return Some(Rect::from_min_size(full.min, Vec2::ZERO));
        }
        let mut r = if self.vertical() { Rect::from_x_y_ranges(full.x_range(), bar.y_range()) } else { Rect::from_x_y_ranges(bar.x_range(), full.y_range()) };
        if hidden {
            let (lead, strip) = (skip, if self.sub.is_some() { sub_strip } else { STRIP });
            match self.edge {
                Edge::Top => (r.min.y, r.max.y) = (r.min.y + lead, r.min.y + lead + strip),
                Edge::Bottom => (r.min.y, r.max.y) = (r.max.y - lead - strip, r.max.y - lead),
                Edge::Left => (r.min.x, r.max.x) = (r.min.x + lead, r.min.x + lead + strip),
                Edge::Right => (r.min.x, r.max.x) = (r.max.x - lead - strip, r.max.x - lead),
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
        if let Some(s) = &self.sub {
            let host = s.host.map(|h| h.to_string()).unwrap_or_else(|| "none".into());
            t.push_str(&format!(";sub={};host={host};stay={};open={};reveal={};linger={};fit={};drawer={}", sub_name(&s.name), s.stay, s.open, s.reveal, s.secs, s.fit, s.drawer));
            if s.handle != Handle::default() {
                t.push_str(&format!(";handle={}", s.handle.text()));
            }
        }
        t
    }

    pub fn parse(v: &str) -> PanelCfg {
        let mut p = PanelCfg::bare();
        let mut sub = Sub { name: String::new(), host: None, stay: false, open: false, reveal: true, secs: 4.0, fit: true, drawer: true, handle: Handle::default() };
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
                "sub" => sub.name = sub_name(val),
                "host" => sub.host = val.parse().ok(),
                "stay" => sub.stay = val == "true",
                "open" => sub.open = val == "true",
                "reveal" => sub.reveal = val != "false",
                "fit" => sub.fit = val != "false",
                "drawer" => sub.drawer = val != "false",
                "handle" => sub.handle = Handle::parse(val),
                "linger" => sub.secs = val.parse::<f32>().unwrap_or(4.0).clamp(*SUB_SECS.start(), *SUB_SECS.end()),
                _ => {}
            }
        }
        if !sub.name.is_empty() {
            p.sub = Some(sub);
        }
        p
    }
}

/// The panel a sub panel is attached to (never a sub panel itself).
fn host_of(panels: &[PanelCfg], idx: usize) -> Option<usize> {
    panels.get(idx)?.sub.as_ref()?.host.filter(|h| panels.get(*h).is_some_and(|p| p.sub.is_none()))
}

/// Where a host's sub panels sit, as (panel, how far from the screen's edge its bar begins, how
/// far its room ends): the ones left open one after another from the host, then each of the
/// rest in the same place after those (they roll out one at a time). `end` is where the host
/// ends. A drawer right under the host starts at the host's bar, with no room between them.
fn stack(panels: &[PanelCfg], host: usize, end: f32) -> Vec<(usize, f32, f32)> {
    let Some(h) = panels.get(host) else { return Vec::new() };
    let subs = || panels.iter().enumerate().filter(move |(i, _)| host_of(panels, *i) == Some(host));
    let place = |p: &PanelCfg, at: f32| {
        let bar = if p.drawer() { if at == end { end - h.gap() } else { at } } else { at + p.off() };
        (bar, bar + p.bar() + p.gap())
    };
    let mut out = Vec::new();
    let mut at = end;
    for (i, p) in subs().filter(|(_, p)| p.kept()) {
        let (bar, to) = place(p, at);
        out.push((i, bar, to));
        at = to.max(at);
    }
    for (i, p) in subs().filter(|(_, p)| !p.kept()) {
        let (bar, to) = place(p, at);
        out.push((i, bar, to));
    }
    out
}

/// The room a host keeps for its sub panels that are left open: they sit between it and windows.
fn held(panels: &[PanelCfg], host: usize) -> f32 {
    if panels.get(host).is_none_or(|h| h.sub.is_some() || h.hide != DockHide::Never) {
        return 0.0;
    }
    // (Measured from the host's end, wherever on the edge that is.)
    stack(panels, host, 0.0).iter().filter(|x| panels[x.0].kept()).map(|x| x.2).fold(0.0, f32::max)
}

/// The room panel `idx` keeps from windows, with its open sub panels' (none when it hides).
pub fn reserved(panels: &[PanelCfg], idx: usize) -> f32 {
    panels.get(idx).map_or(0.0, |p| p.reserved()) + held(panels, idx)
}

/// The room panels keep along an edge of a screen (ones that hide keep none).
pub fn room(panels: &[PanelCfg], output: &str, edge: Edge) -> f32 {
    panels.iter().enumerate().filter(|(_, p)| p.edge == edge && p.on(output)).map(|(i, _)| reserved(panels, i)).sum()
}

/// A sub panel's place under its host: how far from the screen's edge its bar begins, and its
/// room ends.
fn sub_place(panels: &[PanelCfg], idx: usize, output: &str) -> Option<(f32, f32)> {
    let host = host_of(panels, idx)?;
    stack(panels, host, reach(panels, host, output)).into_iter().find(|x| x.0 == idx).map(|x| (x.1, x.2))
}

/// How far from the screen's edge a sub panel's surface begins: its bar, less the room before
/// it (a drawer's reach under its host's corners).
pub fn sub_start(panels: &[PanelCfg], idx: usize, output: &str) -> f32 {
    sub_place(panels, idx, output).map_or(0.0, |(bar, _)| bar - panels[idx].off())
}

/// Whether a sub panel is a drawer right under its host just now: the two drawn as one shape.
pub fn joined(panels: &[PanelCfg], idx: usize, output: &str) -> bool {
    let Some(host) = host_of(panels, idx).filter(|_| panels[idx].drawer()) else { return false };
    let end = reach(panels, host, output);
    sub_place(panels, idx, output).is_some_and(|(bar, _)| bar == end - panels[host].gap())
}

/// Whether the pointer, at `pos` on panel `idx`'s surface, is on the side of the panel its sub
/// panels roll out from: the last few points of its bar and the room it keeps beyond. Pausing
/// there opens a sub panel that asks for it, as pausing on the sub panel's own handle does.
pub fn on_reveal_side(panels: &[PanelCfg], idx: usize, full: Rect, pos: Pos2) -> bool {
    let Some(p) = panels.get(idx).filter(|p| p.sub.is_none()) else { return false };
    if !panels.iter().enumerate().any(|(i, x)| host_of(panels, i) == Some(idx) && x.sub.as_ref().is_some_and(|s| s.reveal && !s.stay)) {
        return false;
    }
    let bar = p.bar_rect(full);
    const IN: f32 = 5.0;
    match p.edge {
        Edge::Top => pos.y >= bar.bottom() - IN && bar.x_range().contains(pos.x),
        Edge::Bottom => pos.y <= bar.top() + IN && bar.x_range().contains(pos.x),
        Edge::Left => pos.x >= bar.right() - IN && bar.y_range().contains(pos.y),
        Edge::Right => pos.x <= bar.left() + IN && bar.y_range().contains(pos.y),
    }
}

/// Where a rolled-up sub panel's handle is drawn, in its surface: along its host's bar, as far
/// from it as asked (as near as the surface reaches, for one that begins past the host's room).
pub fn handle_rect(panels: &[PanelCfg], idx: usize, full: Rect, output: &str) -> Option<Rect> {
    let p = panels.get(idx)?;
    let h = p.sub.as_ref().filter(|s| s.reveal && !s.stay).map(|s| s.handle).filter(|h| h.show)?;
    let host = host_of(panels, idx)?;
    let bar = bar_rect(panels, idx, full);
    // No longer than the bar, less the host's rounded ends.
    let whole = if p.vertical() { bar.height() } else { bar.width() };
    let mut size = h.size();
    size.x = size.x.min((whole - 24.0).max(12.0));
    let (lo, hi) = if p.vertical() { (bar.top(), bar.bottom()) } else { (bar.left(), bar.right()) };
    let mid = match h.along {
        Along::Start => lo + 12.0 + size.x / 2.0,
        Along::Center => (lo + hi) / 2.0,
        Along::End => hi - 12.0 - size.x / 2.0,
    };
    // Across: from where the host's bar ends, which may be before this surface begins.
    let thick = if p.vertical() { full.width() } else { full.height() };
    let host_end = reach(panels, host, output) - panels[host].gap() - sub_start(panels, idx, output);
    let near = (host_end + h.offset as f32).clamp(0.0, (thick - size.y).max(0.0));
    let across = near + size.y / 2.0;
    let (c, size) = match p.edge {
        Edge::Top => (Pos2::new(mid, full.top() + across), size),
        Edge::Bottom => (Pos2::new(mid, full.bottom() - across), size),
        Edge::Left => (Pos2::new(full.left() + across, mid), Vec2::new(size.y, size.x)),
        Edge::Right => (Pos2::new(full.right() - across, mid), Vec2::new(size.y, size.x)),
    };
    Some(Rect::from_center_size(c, size))
}

/// A panel's bar within its surface. A sub panel's can follow its host's: as long, and in line.
pub fn bar_rect(panels: &[PanelCfg], idx: usize, full: Rect) -> Rect {
    let Some(p) = panels.get(idx) else { return full };
    let mut r = p.bar_rect(full);
    let follows = p.sub.as_ref().is_some_and(|s| s.fit || s.drawer);
    if let Some(h) = host_of(panels, idx).filter(|_| follows).map(|h| &panels[h]) {
        // (Both surfaces run the same length of the edge.)
        let hr = h.bar_rect(full);
        if p.vertical() { (r.min.y, r.max.y) = (hr.min.y, hr.max.y) } else { (r.min.x, r.max.x) = (hr.min.x, hr.max.x) }
    }
    r
}

/// The part of a panel's surface that takes the pointer (see `PanelCfg::pointer_rect`).
pub fn pointer_rect(panels: &[PanelCfg], idx: usize, full: Rect, hidden: bool, output: &str) -> Option<Rect> {
    let p = panels.get(idx)?;
    let follows = p.sub.as_ref().is_some_and(|s| s.fit || s.drawer);
    let length = host_of(panels, idx).filter(|_| follows).map_or(p.length, |h| panels[h].length);
    // A drawer's surface begins under its host's bar: its strip is past the host's own surface
    // (the bar's corners, then the room the host keeps), where the pointer reaches it.
    let skip = host_of(panels, idx).filter(|_| p.drawer()).map_or(0.0, |h| p.lead + panels[h].gap());
    // With a handle drawn, the pointer reaches it anywhere from where the host's bar ends to a
    // little past the handle, so the handle and the space around it are easy to hit.
    let thick = if p.vertical() { full.width() } else { full.height() };
    let (start, end) = handle_rect(panels, idx, full, output).map_or((skip, skip + SUB_STRIP), |r| {
        let (near, far) = match p.edge {
            Edge::Top => (r.top() - full.top(), r.bottom() - full.top()),
            Edge::Bottom => (full.bottom() - r.bottom(), full.bottom() - r.top()),
            Edge::Left => (r.left() - full.left(), r.right() - full.left()),
            Edge::Right => (full.right() - r.right(), full.right() - r.left()),
        };
        let host_end = host_of(panels, idx).map_or(0.0, |h| reach(panels, h, output) - panels[h].gap() - sub_start(panels, idx, output));
        (host_end.clamp(0.0, skip).min(near), (far + 8.0).max(skip + SUB_STRIP))
    });
    let end = end.min(thick);
    p.pointer_rect_for(full, bar_rect(panels, idx, full), length, hidden, start, (end - start).max(0.0))
}

/// How far from its edge panel `idx` ends (panels on the same edge stack in order, the first
/// nearest the edge): where its popups open.
pub fn reach(panels: &[PanelCfg], idx: usize, output: &str) -> f32 {
    let Some(me) = panels.get(idx) else { return 0.0 };
    if me.sub.is_some() {
        return sub_place(panels, idx, output).map_or(me.height(), |x| x.1);
    }
    // One that hides keeps no room, so it sits inside every panel that does.
    if me.hide != DockHide::Never {
        return room(panels, output, me.edge) + me.height();
    }
    panels.iter().enumerate().take(idx).filter(|(_, p)| p.edge == me.edge && p.on(output)).map(|(i, _)| reserved(panels, i)).sum::<f32>() + me.height()
}

/// Put the list right after a change: a sub panel takes its host's edge and screen, one whose
/// host is gone (or isn't a panel that can have one) is attached to nothing, and no two share
/// a name.
pub fn tidy(panels: &mut [PanelCfg]) {
    let mut names: Vec<String> = Vec::new();
    for i in 0..panels.len() {
        let Some(mut sub) = panels[i].sub.clone() else { continue };
        sub.host = sub.host.filter(|h| *h != i && panels.get(*h).is_some_and(|p| p.sub.is_none()));
        if let Some(h) = sub.host {
            panels[i].edge = panels[h].edge;
            panels[i].screen = panels[h].screen.clone();
        }
        let base = if sub_name(&sub.name).is_empty() { "Sub Panel".to_string() } else { sub_name(&sub.name) };
        let mut name = base.clone();
        let mut n = 2;
        while names.contains(&name) {
            name = format!("{base} {n}");
            n += 1;
        }
        names.push(name.clone());
        sub.name = name;
        panels[i].sub = Some(sub);
    }
}

/// Take a panel out. Its sub panels stay, attached to nothing until they're given another.
pub fn remove(panels: &mut Vec<PanelCfg>, idx: usize) {
    if idx >= panels.len() {
        return;
    }
    let gone = panels.remove(idx);
    for s in panels.iter_mut().filter_map(|p| p.sub.as_mut()) {
        s.host = match s.host {
            Some(h) if h == idx => None,
            Some(h) if h > idx => Some(h - 1),
            h => h,
        };
    }
    // A sub panel's item goes with it.
    if let Some(s) = &gone.sub {
        let id = s.item();
        for p in panels.iter_mut() {
            p.zones.iter_mut().for_each(|z| z.retain(|i| *i != id));
            p.tidy_groups();
        }
    }
    tidy(panels);
}

/// Swap two panels' places in the list, their sub panels following them.
pub fn swap(panels: &mut [PanelCfg], a: usize, b: usize) {
    if a >= panels.len() || b >= panels.len() {
        return;
    }
    panels.swap(a, b);
    for s in panels.iter_mut().filter_map(|p| p.sub.as_mut()) {
        s.host = s.host.map(|h| if h == a { b } else if h == b { a } else { h });
    }
}

/// A new sub panel for `host`, at the list's end, with its item on the host. Its place in the list.
pub fn add_sub(panels: &mut Vec<PanelCfg>, host: usize) -> Option<usize> {
    let h = panels.get(host).filter(|p| p.sub.is_none())?.clone();
    // Floating like its host, close under it: the host's own gap is the room between them.
    let sub = PanelCfg { floating: h.floating, offset: h.floating.then_some(0), edge: h.edge, screen: h.screen.clone(), size: h.size, sub: Some(Sub::new("Sub Panel", host)), ..PanelCfg::bare() };
    panels.push(sub);
    tidy(panels);
    let idx = panels.len() - 1;
    let id = panels[idx].sub.as_ref().map(Sub::item)?;
    // Before quick settings and the power menu, which close a panel's end.
    let zone = &mut panels[host].zones[2];
    let at = zone.iter().position(|i| i == "quick-settings" || i == "power").unwrap_or(zone.len());
    zone.insert(at, id);
    Some(idx)
}

/// Give a sub panel another name: its item on any panel, and in any group, follows.
pub fn rename_sub(panels: &mut [PanelCfg], idx: usize, name: &str) {
    let Some(old) = panels.get(idx).and_then(|p| p.sub.as_ref()).map(Sub::item) else { return };
    if let Some(s) = panels[idx].sub.as_mut() {
        s.name = sub_name(name);
    }
    tidy(panels);
    let Some(new) = panels[idx].sub.as_ref().map(Sub::item) else { return };
    for p in panels.iter_mut() {
        for id in p.zones.iter_mut().flatten().chain(p.groups.iter_mut().flat_map(|g| g.items.iter_mut())) {
            if *id == old {
                *id = new.clone();
            }
        }
    }
}

/// The sub panel with this name (any case), by its place in the list.
pub fn find_sub(panels: &[PanelCfg], name: &str) -> Option<usize> {
    let want = name.trim().to_lowercase();
    panels.iter().position(|p| p.sub.as_ref().is_some_and(|s| s.name.to_lowercase() == want))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sub_panels() {
        // A top panel, a bottom one, and a sub panel under the first: it takes the host's edge,
        // has its button on the host, and comes back from the settings file as it was.
        let mut ps = vec![PanelCfg::default(), PanelCfg { floating: false, ..PanelCfg::new_bottom() }];
        let i = add_sub(&mut ps, 0).unwrap();
        assert_eq!((i, ps[i].edge, ps[0].has("sub:Sub Panel")), (2, Edge::Top, true));
        assert_eq!(add_sub(&mut ps, i), None);
        rename_sub(&mut ps, i, "Short;cuts");
        assert!(ps[0].has("sub:Short cuts") && find_sub(&ps, "short CUTS") == Some(i));
        assert_eq!(PanelCfg::parse(&ps[i].text()), ps[i]);
        // Its handle, changed: kept in the line; at the bar's start, in from the corner.
        ps[i].sub.as_mut().unwrap().handle = Handle { show: true, length: 80, thick: 3, along: Along::Start, color: Some(crate::style::Role::Tertiary), shape: HandleShape::Dots, opacity: 0.5, offset: 20 };
        assert_eq!(PanelCfg::parse(&ps[i].text()), ps[i]);
        // (No reach under the host's corners yet: its surface begins where the host's bar ends.)
        let strip = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, ps[i].height()));
        assert_eq!(handle_rect(&ps, i, strip, "").map(|r| (r.left(), r.top(), r.width(), r.height())), Some((MARGIN + 12.0, 20.0, 80.0, 3.0)));
        // The strip that takes the pointer runs from the host's bar to a little past the handle,
        // so the whole handle can be pointed at.
        let hit = pointer_rect(&ps, i, strip, true, "").unwrap();
        assert_eq!((hit.top(), hit.bottom()), (0.0, 31.0));
        assert!(hit.contains_rect(handle_rect(&ps, i, strip, "").unwrap()));
        ps[i].sub.as_mut().unwrap().handle.show = false;
        assert_eq!(handle_rect(&ps, i, strip, ""), None);
        // A drawer: its bar begins where the host's ends, with no room between, and its surface
        // reaches under the host's corners by their radius. It's as long as the host's bar.
        ps[0].length = 60;
        ps[i].lead = 10.0;
        let (end, full) = (BAR + MARGIN * 2.0, Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, ps[i].height())));
        assert_eq!((ps[i].height(), sub_start(&ps, i, ""), reach(&ps, i, ""), joined(&ps, i, "")), (10.0 + BAR + MARGIN, end - MARGIN - 10.0, end - MARGIN + BAR + MARGIN, true));
        let (bar, host) = (bar_rect(&ps, i, full), ps[0].bar_rect(full));
        assert_eq!((bar.x_range(), bar.top()), (host.x_range(), 10.0));
        assert_eq!(pointer_rect(&ps, i, full, true, "").map(|r| (r.top(), r.height(), r.x_range())), Some((10.0 + MARGIN, SUB_STRIP, host.x_range())));
        // Not a drawer, and not matching: its own length, after the host's room.
        let sub = ps[i].sub.as_mut().unwrap();
        (sub.drawer, sub.fit) = (false, false);
        (ps[0].length, ps[i].lead) = (100, 0.0);
        assert!(!joined(&ps, i, "") && bar_rect(&ps, i, full).width() == 1000.0);
        // Rolled up it keeps no room, and sits where its host ends; left open, the host keeps
        // its room and popups under the top panels open below it.
        let host = BAR + MARGIN * 2.0;
        let h = ps[i].height();
        assert_eq!((room(&ps, "", Edge::Top), sub_start(&ps, i, ""), reach(&ps, i, ""), reach(&ps, 0, "")), (host, host, host + h, host));
        let sub = ps[i].sub.as_mut().unwrap();
        (sub.stay, sub.open) = (true, true);
        assert_eq!((room(&ps, "", Edge::Top), reserved(&ps, 0), ps[i].reserved(), reach(&ps, 0, "")), (host + h, host + h, 0.0, host));
        // Rolled up, it takes the pointer along its edge only when resting there opens it.
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(1000.0, h));
        assert_eq!(ps[i].pointer_rect(full, true).map(|r| r.height()), Some(0.0));
        ps[i].sub.as_mut().unwrap().stay = false;
        assert_eq!(ps[i].pointer_rect(full, true).map(|r| r.height()), Some(SUB_STRIP));
        // A second one gets a name of its own; moving the host to the bottom takes both along.
        let j = add_sub(&mut ps, 0).unwrap();
        rename_sub(&mut ps, j, "Short cuts");
        assert_eq!(ps[j].sub.as_ref().unwrap().name, "Short cuts 2");
        ps[0].edge = Edge::Bottom;
        tidy(&mut ps);
        assert_eq!((ps[i].edge, ps[j].edge), (Edge::Bottom, Edge::Bottom));
        // Swapping the two panels: the sub panels follow their host.
        swap(&mut ps, 0, 1);
        assert_eq!(ps[i].sub.as_ref().unwrap().host, Some(1));
        // The host removed: they stay, attached to nothing and on no screen, until given another.
        remove(&mut ps, 1);
        assert_eq!((ps.len(), ps[1].sub.as_ref().unwrap().host, ps[1].on("DP-1")), (3, None, false));
        ps[1].sub.as_mut().unwrap().host = Some(0);
        tidy(&mut ps);
        assert!(ps[1].on("DP-1") && ps[1].edge == ps[0].edge);
        // One removed: its button goes from the panels.
        ps[0].zones[0].push("sub:Short cuts".into());
        remove(&mut ps, 1);
        assert!(!ps[0].has("sub:Short cuts") && ps.len() == 2);
    }

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
        g.groups.push(Group { items: list(&["volume", "brightness"]), bg: Some(Paint::Primary), fg: Paint::Custom([255, 0, 128]), show: 1, gradient: true, height: Some(30) });
        g.groups.push(Group { items: list(&["clock"]), bg: None, fg: Paint::Tertiary, show: 0, gradient: false, height: None });
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
