//! Rivet Shell's settings, kept in `~/.config/rivet-shell/settings` as plain `key=value` lines.
//! The shell reads them again whenever the file changes (the Settings window writes it).
//! Checks (screenshot runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

use crate::panels::PanelCfg;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Edge {
    #[default]
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];
    pub fn name(self) -> &'static str {
        match self {
            Edge::Top => "top",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
            Edge::Right => "right",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Edge::Top => "Top",
            Edge::Bottom => "Bottom",
            Edge::Left => "Left",
            Edge::Right => "Right",
        }
    }
    pub fn from(v: &str) -> Edge {
        Edge::ALL.into_iter().find(|e| e.name() == v).unwrap_or_default()
    }
    /// A panel on it runs down the screen's side.
    pub fn vertical(self) -> bool {
        matches!(self, Edge::Left | Edge::Right)
    }
}

/// Where along its edge a panel shorter than the edge sits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Along {
    Start,
    #[default]
    Center,
    End,
}

impl Along {
    pub fn name(self) -> &'static str {
        match self {
            Along::Start => "start",
            Along::Center => "center",
            Along::End => "end",
        }
    }
    pub fn from(v: &str) -> Along {
        match v {
            "start" => Along::Start,
            "end" => Along::End,
            _ => Along::Center,
        }
    }
}

/// How the launcher shows the apps.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LaunchView {
    /// Icons in rows, each over its name.
    #[default]
    Grid,
    /// A row for each app.
    List,
}

impl LaunchView {
    pub fn from(v: &str) -> LaunchView {
        if v == "list" { LaunchView::List } else { LaunchView::Grid }
    }
    pub fn name(self) -> &'static str {
        match self {
            LaunchView::Grid => "grid",
            LaunchView::List => "list",
        }
    }
}

/// Where the launcher opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LaunchPlace {
    /// In the middle of the screen.
    #[default]
    Center,
    /// From a panel, like the panel's other popups: the one it was opened from, else the
    /// screen's first.
    Panel,
}

impl LaunchPlace {
    pub fn from(v: &str) -> LaunchPlace {
        if v == "panel" { LaunchPlace::Panel } else { LaunchPlace::Center }
    }
    pub fn name(self) -> &'static str {
        match self {
            LaunchPlace::Center => "center",
            LaunchPlace::Panel => "panel",
        }
    }
}

/// Where the on-screen display shows, in the middle of the screen's width.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum OsdPlace {
    Top,
    Middle,
    #[default]
    Bottom,
}

impl OsdPlace {
    pub fn from(v: &str) -> OsdPlace {
        match v {
            "top" => OsdPlace::Top,
            "middle" => OsdPlace::Middle,
            _ => OsdPlace::Bottom,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            OsdPlace::Top => "top",
            OsdPlace::Middle => "middle",
            OsdPlace::Bottom => "bottom",
        }
    }
}

/// Which corner notification cards show in.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NotePlace {
    /// The right, on the first panel's edge (the top when there are none).
    #[default]
    ByPanel,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl NotePlace {
    pub fn from(v: &str) -> NotePlace {
        match v {
            "top-left" => NotePlace::TopLeft,
            "top-right" => NotePlace::TopRight,
            "bottom-left" => NotePlace::BottomLeft,
            "bottom-right" => NotePlace::BottomRight,
            _ => NotePlace::ByPanel,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            NotePlace::ByPanel => "panel",
            NotePlace::TopLeft => "top-left",
            NotePlace::TopRight => "top-right",
            NotePlace::BottomLeft => "bottom-left",
            NotePlace::BottomRight => "bottom-right",
        }
    }
}

/// How big the launcher, the on-screen display or the notification cards are.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LaunchSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl LaunchSize {
    pub fn from(v: &str) -> LaunchSize {
        match v {
            "small" => LaunchSize::Small,
            "large" => LaunchSize::Large,
            _ => LaunchSize::Medium,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            LaunchSize::Small => "small",
            LaunchSize::Medium => "medium",
            LaunchSize::Large => "large",
        }
    }
}

/// When the dock, or a panel, hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DockHide {
    /// Always shown.
    #[default]
    Never,
    /// Slides away; comes back when the pointer reaches its edge of the screen.
    Auto,
    /// Shown while the workspace has no windows; else like Auto.
    Smart,
}

impl DockHide {
    pub fn from(v: &str) -> DockHide {
        match v {
            "auto" => DockHide::Auto,
            "smart" => DockHide::Smart,
            _ => DockHide::Never,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            DockHide::Never => "never",
            DockHide::Auto => "auto",
            DockHide::Smart => "smart",
        }
    }
}

/// How one wallpaper gives way to the next.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Transition {
    None,
    /// The new picture fades in over the old.
    #[default]
    Fade,
    /// A soft edge sweeps across.
    Wipe,
    /// The new picture pushes the old one off.
    Slide,
    /// A circle opens from the middle.
    Grow,
}

impl Transition {
    pub const ALL: [Transition; 5] = [Transition::None, Transition::Fade, Transition::Wipe, Transition::Slide, Transition::Grow];
    pub fn name(self) -> &'static str {
        match self {
            Transition::None => "none",
            Transition::Fade => "fade",
            Transition::Wipe => "wipe",
            Transition::Slide => "slide",
            Transition::Grow => "grow",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Transition::None => "None",
            Transition::Fade => "Fade",
            Transition::Wipe => "Wipe",
            Transition::Slide => "Slide",
            Transition::Grow => "Grow",
        }
    }
    pub fn from(v: &str) -> Transition {
        Transition::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// How the power menu shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum PowerStyle {
    /// A list under the item.
    #[default]
    Menu,
    /// Large tiles in the middle of the screen.
    Grid,
    /// A row of symbols under the item.
    Row,
}

impl PowerStyle {
    pub const ALL: [PowerStyle; 3] = [PowerStyle::Menu, PowerStyle::Grid, PowerStyle::Row];
    pub fn name(self) -> &'static str {
        match self {
            PowerStyle::Menu => "menu",
            PowerStyle::Grid => "grid",
            PowerStyle::Row => "row",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PowerStyle::Menu => "Menu",
            PowerStyle::Grid => "Large Grid",
            PowerStyle::Row => "Row",
        }
    }
    pub fn from(v: &str) -> PowerStyle {
        PowerStyle::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// How the panel's workspaces item looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WsStyle {
    /// Rounded pills with the workspace's number or name; the active one is wider and coloured.
    #[default]
    Pills,
    /// Small dots; the active one stretches into a coloured bar. No text.
    Dots,
    /// Just the numbers or names; the active one coloured and underlined.
    Numbers,
    /// Squares with rounded corners, all one size; the active one coloured.
    Squares,
}

impl WsStyle {
    pub const ALL: [WsStyle; 4] = [WsStyle::Pills, WsStyle::Squares, WsStyle::Dots, WsStyle::Numbers];
    pub fn name(self) -> &'static str {
        match self {
            WsStyle::Pills => "pills",
            WsStyle::Dots => "dots",
            WsStyle::Numbers => "numbers",
            WsStyle::Squares => "squares",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            WsStyle::Pills => "Pills",
            WsStyle::Dots => "Dots",
            WsStyle::Numbers => "Numbers",
            WsStyle::Squares => "Squares",
        }
    }
    pub fn from(v: &str) -> WsStyle {
        WsStyle::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// The quick settings item's symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum QuickIcon {
    #[default]
    Sliders,
    /// Two switches, one on.
    Toggles,
    /// Four tiles, one lit.
    Tiles,
    Gear,
}

impl QuickIcon {
    pub const ALL: [QuickIcon; 4] = [QuickIcon::Sliders, QuickIcon::Toggles, QuickIcon::Tiles, QuickIcon::Gear];
    pub fn name(self) -> &'static str {
        match self {
            QuickIcon::Sliders => "sliders",
            QuickIcon::Toggles => "toggles",
            QuickIcon::Tiles => "tiles",
            QuickIcon::Gear => "gear",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            QuickIcon::Sliders => "Sliders",
            QuickIcon::Toggles => "Switches",
            QuickIcon::Tiles => "Tiles",
            QuickIcon::Gear => "Gear",
        }
    }
    pub fn from(v: &str) -> QuickIcon {
        QuickIcon::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// The launcher item's symbol.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LauncherIcon {
    /// A grid of dots.
    #[default]
    Grid,
    /// The system's logo (the distribution's), from the icon theme.
    System,
    /// An icon by name, or a picture.
    Custom,
}

impl LauncherIcon {
    pub const ALL: [LauncherIcon; 3] = [LauncherIcon::Grid, LauncherIcon::System, LauncherIcon::Custom];
    pub fn name(self) -> &'static str {
        match self {
            LauncherIcon::Grid => "grid",
            LauncherIcon::System => "system",
            LauncherIcon::Custom => "custom",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            LauncherIcon::Grid => "Grid",
            LauncherIcon::System => "System",
            LauncherIcon::Custom => "Custom",
        }
    }
    pub fn from(v: &str) -> LauncherIcon {
        LauncherIcon::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// How the tray shows apps' icons.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum TrayColor {
    /// As the apps draw them.
    #[default]
    Own,
    /// One colour, the panel's text colour (their shapes kept).
    Text,
    /// One colour, the panel's highlight colour.
    Highlight,
}

impl TrayColor {
    pub const ALL: [TrayColor; 3] = [TrayColor::Own, TrayColor::Text, TrayColor::Highlight];
    pub fn name(self) -> &'static str {
        match self {
            TrayColor::Own => "own",
            TrayColor::Text => "text",
            TrayColor::Highlight => "highlight",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            TrayColor::Own => "Their Colors",
            TrayColor::Text => "Like the Panel",
            TrayColor::Highlight => "Highlight",
        }
    }
    pub fn from(v: &str) -> TrayColor {
        TrayColor::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

/// What the Now Playing item says beside its picture.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum MediaText {
    #[default]
    TitleArtist,
    ArtistTitle,
    Title,
    /// The picture or symbol alone.
    None,
}

impl MediaText {
    pub const ALL: [MediaText; 4] = [MediaText::TitleArtist, MediaText::ArtistTitle, MediaText::Title, MediaText::None];
    pub fn name(self) -> &'static str {
        match self {
            MediaText::TitleArtist => "title-artist",
            MediaText::ArtistTitle => "artist-title",
            MediaText::Title => "title",
            MediaText::None => "none",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            MediaText::TitleArtist => "Title · Artist",
            MediaText::ArtistTitle => "Artist · Title",
            MediaText::Title => "Title",
            MediaText::None => "No Text",
        }
    }
    pub fn from(v: &str) -> MediaText {
        MediaText::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
    /// The line for a track.
    pub fn of(self, title: &str, artist: &str) -> String {
        match self {
            MediaText::None => String::new(),
            MediaText::Title => title.to_string(),
            _ if artist.is_empty() => title.to_string(),
            _ if title.is_empty() => artist.to_string(),
            MediaText::TitleArtist => format!("{title} · {artist}"),
            MediaText::ArtistTitle => format!("{artist} · {title}"),
        }
    }
}

/// What a level item (volume, brightness, battery) shows in the panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Show {
    /// Its icon and its percent.
    #[default]
    Both,
    /// Just the icon (the percent is in its tooltip).
    Icon,
    /// Just the percent.
    Text,
}

impl Show {
    pub const ALL: [Show; 3] = [Show::Both, Show::Icon, Show::Text];
    pub fn name(self) -> &'static str {
        match self {
            Show::Both => "both",
            Show::Icon => "icon",
            Show::Text => "text",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Show::Both => "Icon & Percent",
            Show::Icon => "Icon",
            Show::Text => "Percent",
        }
    }
    pub fn from(v: &str) -> Show {
        Show::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
    pub fn text(self) -> bool {
        self != Show::Icon
    }
}

/// What a widget shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WidgetKind {
    Clock,
    System,
    Weather,
    /// What is playing, with its controls.
    Media,
    /// A month.
    Calendar,
    /// The events coming up, from the plugins that give the calendar its events.
    Events,
    /// A grid of apps to open.
    Apps,
}

impl WidgetKind {
    pub const ALL: [WidgetKind; 7] = [WidgetKind::Clock, WidgetKind::Calendar, WidgetKind::Events, WidgetKind::Apps, WidgetKind::Media, WidgetKind::Weather, WidgetKind::System];
    pub fn name(self) -> &'static str {
        match self {
            WidgetKind::Clock => "clock",
            WidgetKind::System => "system",
            WidgetKind::Weather => "weather",
            WidgetKind::Media => "media",
            WidgetKind::Calendar => "calendar",
            WidgetKind::Events => "events",
            WidgetKind::Apps => "apps",
        }
    }
    pub fn from(v: &str) -> Option<WidgetKind> {
        WidgetKind::ALL.into_iter().find(|k| k.name() == v)
    }
    /// Its name as a person reads it.
    pub fn label(self) -> &'static str {
        match self {
            WidgetKind::Clock => "Clock",
            WidgetKind::System => "System",
            WidgetKind::Weather => "Weather",
            WidgetKind::Media => "Now Playing",
            WidgetKind::Calendar => "Calendar",
            WidgetKind::Events => "Events",
            WidgetKind::Apps => "Apps",
        }
    }
}

/// Where a widget is: on the desktop, under the windows, or on the HUD, which opens over them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Board {
    Desktop,
    Hud,
}

/// A widget placed on the desktop or the HUD.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub kind: WidgetKind,
    pub board: Board,
    /// The display it's on, by name (empty: every display).
    pub output: String,
    /// Where it is, as a fraction of the room it can move in (0,0 the top left, 1,1 the
    /// bottom right), so it keeps its place on screens of any size. None: its usual place, in
    /// the stack on `widget_side`.
    pub at: Option<(f32, f32)>,
    /// Its size, as a fraction of the room widgets have on the screen. None: its own size.
    pub size: Option<(f32, f32)>,
}

impl Placed {
    pub fn new(kind: WidgetKind, board: Board) -> Placed {
        Placed { kind, board, output: String::new(), at: None, size: None }
    }
    /// On the HUD, with its left and top edges and its size given as fractions of the room.
    fn hud(kind: WidgetKind, x: f32, y: f32, w: f32, h: f32) -> Placed {
        // To the four places the file keeps, so a setup read back is the one written.
        let room = |at: f32, size: f32| if size < 1.0 { ((at / (1.0 - size)).clamp(0.0, 1.0) * 1e4).round() / 1e4 } else { 0.0 };
        Placed { kind, board: Board::Hud, output: String::new(), at: Some((room(x, w), room(y, h))), size: Some((w, h)) }
    }
    /// Whether it shows on this display (a display with no name shows them all).
    pub fn on(&self, output: &str) -> bool {
        self.output.is_empty() || output.is_empty() || self.output == output
    }
    /// "kind|board|display|x,y|w,h", with nothing where a place or size isn't set.
    fn text(&self) -> String {
        let pair = |p: Option<(f32, f32)>| p.map(|(x, y)| format!("{x:.4},{y:.4}")).unwrap_or_default();
        format!("{}|{}|{}|{}|{}", self.kind.name(), if self.board == Board::Hud { "hud" } else { "desktop" }, self.output.replace(['|', '\n'], " "), pair(self.at), pair(self.size))
    }
    fn parse(v: &str) -> Option<Placed> {
        let mut it = v.split('|');
        let kind = WidgetKind::from(it.next()?.trim())?;
        let board = if it.next()?.trim() == "hud" { Board::Hud } else { Board::Desktop };
        let output = it.next().unwrap_or("").trim().to_string();
        let at = it.next().and_then(fraction_pair);
        // A size too small to hold anything is no size.
        let size = it.next().and_then(fraction_pair).filter(|(w, h)| *w >= 0.02 && *h >= 0.02);
        Some(Placed { kind, board, output, at, size })
    }
}

/// The widgets a new setup has: a clock and the system's readings on the desktop, and a HUD
/// with the clock, the month and what's coming up on the left, apps in the middle, and what's
/// playing, the weather and the system on the right.
pub fn default_widgets() -> Vec<Placed> {
    use WidgetKind::*;
    vec![
        Placed::new(Clock, Board::Desktop),
        Placed::new(System, Board::Desktop),
        Placed::hud(Clock, 0.0, 0.0, 0.25, 0.17),
        Placed::hud(Calendar, 0.0, 0.2, 0.25, 0.4),
        Placed::hud(Events, 0.0, 0.63, 0.25, 0.37),
        Placed::hud(Apps, 0.27, 0.0, 0.46, 1.0),
        Placed::hud(Media, 0.75, 0.0, 0.25, 0.36),
        Placed::hud(Weather, 0.75, 0.38, 0.25, 0.32),
        Placed::hud(System, 0.75, 0.72, 0.25, 0.28),
    ]
}

/// A screen corner the pointer opens the HUD from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Corner {
    #[default]
    Off,
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn name(self) -> &'static str {
        match self {
            Corner::Off => "off",
            Corner::TopLeft => "top-left",
            Corner::TopRight => "top-right",
            Corner::BottomLeft => "bottom-left",
            Corner::BottomRight => "bottom-right",
        }
    }
    pub fn from(v: &str) -> Corner {
        [Corner::TopLeft, Corner::TopRight, Corner::BottomLeft, Corner::BottomRight].into_iter().find(|c| c.name() == v).unwrap_or(Corner::Off)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    #[default]
    Left,
    Right,
}

/// A font family's name as it's kept in the settings file (nothing that ends a value there).
pub fn font_name(v: &str) -> String {
    v.replace([';', '=', '\n', '\t'], "").trim().to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // ---- the look (as every Rivet app)
    pub theme: Mode,
    pub accent: [u8; 3],
    /// The shell's own background colour in its dark style and in its light one (None: the
    /// usual greys). With the style and accent, when the colours aren't the desktop's or a palette's.
    pub surface_dark: Option<[u8; 3]>,
    /// With a style and accent of the shell's own: the secondary and tertiary colours chosen
    /// (None: made from the accent).
    pub secondary: Option<[u8; 3]>,
    pub tertiary: Option<[u8; 3]>,
    pub surface_light: Option<[u8; 3]>,
    /// The surface takes a little of the accent, so it changes with it.
    pub surface_tint: bool,
    pub follow_desktop_colors: bool,
    pub text_size: f32,
    /// How much the shell uses the theme's secondary and tertiary colours: 0 the primary
    /// only, 1 some (switches, notifications, the widget's bars), 2 a lot (sliders, dots…).
    pub color_use: u8,
    /// The interface's font, a family's name (empty: the desktop's); the launcher's own
    /// (empty: this one). A panel's own is in its settings.
    pub font: String,
    pub launcher_font: String,
    /// The panels' and the dock's look (each can change any of it).
    pub look: crate::style::Style,
    // ---- wallpaper and colours
    /// The picture on the desktop (empty: a plain background in the theme's colours).
    pub wallpaper: String,
    /// Where the Settings window looks for pictures.
    pub wallpaper_dir: String,
    /// Folders chosen before, the latest first.
    pub wallpaper_dirs: Vec<String>,
    /// Screens with a picture of their own (output name, picture); the rest show `wallpaper`.
    pub wallpaper_screens: Vec<(String, String)>,
    /// The screens as set in Settings, Displays (empty: left to the compositor's config).
    pub displays: Vec<crate::displays::Display>,
    /// How a new wallpaper comes in, and for how long (milliseconds).
    pub wall_transition: Transition,
    pub wall_transition_ms: u32,
    /// The screen whose picture the colours are made from (none: the shared picture).
    pub matugen_screen: String,
    /// matugen's contrast, −1 to 1 (0: as designed).
    pub matugen_contrast: f32,
    /// Run matugen when the wallpaper changes, so every app takes its colours.
    pub matugen: bool,
    /// A built-in palette for the desktop's colours, by name (one of `rivet_look::schemes`;
    /// empty: none). For when matugen isn't making them: only one of the two is on.
    pub palette: String,
    /// The icon theme for apps' and the tray's icons, by its folder's name (empty: the desktop's).
    pub icon_theme: String,
    /// matugen's light or dark scheme.
    pub color_mode: Mode,
    /// matugen's scheme type (`-t`), e.g. scheme-tonal-spot.
    pub scheme_type: String,
    // ---- panels
    /// Every panel, in order (on the same edge, the first is nearest it).
    pub panels: Vec<PanelCfg>,
    /// The clock, in strftime form.
    pub clock_format: String,
    /// The workspaces item: how it looks, names (when a workspace has one) or numbers, whether
    /// empty workspaces are left out, and whether scrolling over it switches workspaces.
    pub ws_style: WsStyle,
    pub ws_names: bool,
    pub ws_hide_empty: bool,
    pub ws_scroll: bool,
    /// The AI item: the providers it reads (ids from ai::PROVIDERS), which one the panel shows
    /// ("auto": the most used limit of any), and whether it shows what's left instead of used.
    pub ai_providers: Vec<String>,
    pub ai_bar: String,
    pub ai_left: bool,
    /// Limits pinned to the panel, as "provider:limit name" (none: the most used one shows).
    pub ai_pins: Vec<String>,
    /// Hermes Agent's week in the AI item, when it's installed. Kept apart from ai_providers so
    /// a list saved before Hermes was added still shows it.
    pub ai_hermes: bool,
    /// The volume, brightness and battery items: what each shows, and whether scrolling over
    /// volume or brightness changes it; the battery can be left out while it's charged.
    // ---- the Now Playing item
    pub media_text: MediaText,
    /// The most room its text takes, in points.
    pub media_width: u32,
    /// The cover in place of the play symbol.
    pub media_art: bool,
    /// Previous and next beside it.
    pub media_controls: bool,
    /// Bars moving with the sound.
    pub media_viz: bool,
    /// A click plays or pauses (else it opens the player's popup).
    pub media_click_plays: bool,
    /// Scrolling over it changes track.
    pub media_scroll: bool,
    pub media_hide_paused: bool,
    pub vol_show: Show,
    pub vol_scroll: bool,
    pub bright_show: Show,
    pub bright_scroll: bool,
    pub batt_show: Show,
    pub batt_hide_full: bool,
    /// The power menu: its style, whether log out, restart and shut down ask for a second
    /// click, and the actions left out (lock, suspend, logout, restart, shutdown).
    pub power_style: PowerStyle,
    pub power_confirm: bool,
    pub power_hide: Vec<String>,
    /// The tray: apps' icons in their own colours or recoloured, and whether icons that say
    /// they're idle ("passive") show too.
    pub tray_color: TrayColor,
    /// The launcher item's symbol, and the custom one (an icon's name, or a picture's path).
    pub launcher_icon: LauncherIcon,
    /// The quick settings item's symbol.
    pub quick_icon: QuickIcon,
    pub launcher_custom: String,
    pub tray_passive: bool,
    // ---- notifications, on-screen display, night light
    /// Be the notification service (cards in the corner, a history in the control center).
    pub notifications: bool,
    /// The cards: their corner, width, seconds on screen (when the app doesn't say), how many
    /// show at once, and their own look over Appearance's.
    pub note_place: NotePlace,
    pub note_width: LaunchSize,
    pub note_secs: u32,
    pub note_max: usize,
    pub note_look: crate::style::Style,
    /// Clipboard history: kept at all, how many copies (pinned ones aside), and saved for the
    /// next session (off: forgotten when the shell ends).
    pub clip_on: bool,
    pub clip_length: u32,
    pub clip_keep: bool,
    /// Show volume and brightness changes in a small pop-up.
    pub osd: bool,
    /// Show Caps Lock and Num Lock turning on and off there too.
    pub osd_keys: bool,
    /// Where it shows, how big it is, and for how long.
    pub osd_place: OsdPlace,
    pub osd_size: LaunchSize,
    pub osd_secs: f32,
    /// The bar's colour; None follows how much the shell uses the secondary and tertiary.
    pub osd_color: Option<crate::style::Role>,
    /// Ask for the password when an app needs an administrator's say-so (be the session's
    /// polkit agent).
    pub ask_passwords: bool,
    /// The launcher: a grid or a list, its size, its icons' size (the grid's; a list's are
    /// two thirds of it), whether sums are answered, and whether the screen dims behind it.
    /// The plugins turned on (their folders' names), and the person's own commands for the
    /// shell's events (event, command), by event.
    pub plugins: Vec<String>,
    pub hooks: Vec<(String, String)>,
    /// Softcore's colours that follow the theme: (Softcore's key, `accent`, `secondary` or
    /// `tertiary`), `softcore_color.KEY=ROLE`. The window-look file carries the colour itself.
    pub softcore_colors: Vec<(String, String)>,
    /// The plugins' own settings: (plugin, key, value), `plugin.ID.KEY=value`.
    pub plugin_set: crate::plugins::Values,
    pub launch_view: LaunchView,
    pub launch_place: LaunchPlace,
    pub launch_size: LaunchSize,
    pub launch_icon: u32,
    /// How many rows the launcher shows, and how many columns its grid has (0: as its size
    /// makes them).
    pub launch_rows: u32,
    pub launch_cols: u32,
    pub launch_calc: bool,
    pub launch_dim: bool,
    /// A row of favourite apps above the rest, and the apps in it (desktop ids).
    pub launch_favorites: bool,
    pub launch_favorite_apps: Vec<String>,
    /// Flatpak, Snap and AppImage apps marked as such.
    pub launch_source: bool,
    /// Night light's warmth, in kelvin.
    pub night_kelvin: u32,
    // ---- the compositor's window look (comp.rs): set by an included overrides file
    pub comp_on: bool,
    pub comp_gaps_in: u32,
    pub comp_gaps_out: u32,
    pub comp_border: u32,
    pub comp_radius: u32,
    /// The focused window's border in the accent colour (else the compositor's colours).
    pub comp_accent_borders: bool,
    pub comp_inactive_opacity: f32,
    pub comp_shadows: bool,
    /// Chromium, Chrome, Brave and Edge take the theme's colour (see `crate::browser`), and
    /// whether that colour is the surface, not the accent.
    pub browser_colors: bool,
    /// The VPN the quick settings block switches (`vpn::Vpn::key`; empty: the first there is).
    pub quick_vpn: String,
    /// ZeroTier networks this computer has joined (ID, name), kept so one that was left can
    /// be joined again from Settings.
    pub zerotier_networks: Vec<(String, String)>,
    pub browser_accent: bool,
    /// How much darker than the desktop's windows the browsers are, 0 to 100 (100: black).
    pub browser_darker: u32,
    /// The accent the browsers get, darker (below 0) or lighter (above), -100 to 100.
    pub browser_accent_shade: i32,
    /// The programs kept in the theme's colours (`themed::App::id`).
    pub themed_apps: Vec<String>,
    // ---- idle and the lock screen (idle.rs, lock.rs); times in seconds, 0 for never
    /// The shell watches for idle (else another program, like hypridle or swayidle, does).
    pub idle_on: bool,
    pub idle_dim: u32,
    pub idle_lock: u32,
    pub idle_screen_off: u32,
    pub idle_suspend: u32,
    /// Not idle while music or a video plays (players that say so over MPRIS).
    pub idle_media: bool,
    /// Lock before the computer sleeps.
    pub lock_on_sleep: bool,
    /// Another program to lock with (swaylock, hyprlock…); empty for Rivet Shell's own.
    pub lock_cmd: String,
    /// The lock screen's wallpaper blurred (else as it is, darkened).
    pub lock_blur: bool,
    /// Suspend, restart and shut down on the lock screen.
    pub lock_power: bool,
    // ---- dock
    pub dock: bool,
    /// Pinned apps by desktop id (rivet-files, org.mozilla.firefox…).
    pub dock_apps: Vec<String>,
    pub dock_icon: f32,
    /// Windows keep clear of the dock (else it floats over them).
    pub dock_reserve: bool,
    /// A dot under apps that have windows open (niri, Hyprland).
    pub dock_running: bool,
    /// Apps with windows open show in the dock too (after the pinned ones).
    pub dock_open_apps: bool,
    /// Whether and when the dock slides out of the way.
    pub dock_hide: DockHide,
    /// Downloads and the Trash at the dock's end.
    pub dock_places: bool,
    /// What is behind see-through panels, the dock and popups blurred, where the compositor can.
    pub blur: bool,
    /// The dock's own look, over Appearance's.
    pub dock_look: crate::style::Style,
    // ---- widgets
    /// The widgets on the desktop and on the HUD, in the order they're drawn.
    pub widgets: Vec<Placed>,
    /// The side of the desktop widgets that haven't been moved stack on.
    pub widget_side: Side,
    /// The corner the pointer opens the HUD from.
    pub hud_corner: Corner,
    /// Widgets being arranged lock to a grid: on the HUD, and on the desktop.
    pub hud_grid: bool,
    pub desktop_grid: bool,
    /// The weather, for the widget and the panel item: the place chosen (its name as shown,
    /// and its latitude and longitude; None: no place yet), and the units it shows in.
    pub weather_place: String,
    pub weather_at: Option<(f32, f32)>,
    pub weather_units: crate::weather::Units,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            surface_dark: None,
            secondary: None,
            tertiary: None,
            surface_light: None,
            surface_tint: false,
            follow_desktop_colors: true,
            text_size: 0.0,
            color_use: 1,
            font: String::new(),
            launcher_font: String::new(),
            look: crate::style::Style::default(),
            wallpaper: String::new(),
            wallpaper_dir: default_wallpaper_dir(),
            wallpaper_dirs: Vec::new(),
            wallpaper_screens: Vec::new(),
            displays: Vec::new(),
            wall_transition: Transition::Fade,
            wall_transition_ms: 600,
            matugen_screen: String::new(),
            matugen_contrast: 0.0,
            matugen: true,
            palette: String::new(),
            icon_theme: String::new(),
            color_mode: Mode::Dark,
            scheme_type: "scheme-tonal-spot".into(),
            panels: vec![PanelCfg::default()],
            clock_format: "%a %b %-d  %H:%M".into(),
            ws_style: WsStyle::Pills,
            ws_names: true,
            ws_hide_empty: false,
            ws_scroll: true,
            ai_providers: crate::ai::PROVIDERS.iter().map(|p| p.0.to_string()).collect(),
            ai_bar: "auto".into(),
            ai_left: false,
            ai_pins: Vec::new(),
            ai_hermes: true,
            media_text: MediaText::TitleArtist,
            media_width: 230,
            media_art: true,
            media_controls: false,
            media_viz: false,
            media_click_plays: false,
            media_scroll: true,
            media_hide_paused: false,
            vol_show: Show::Both,
            vol_scroll: true,
            bright_show: Show::Both,
            bright_scroll: true,
            batt_show: Show::Both,
            batt_hide_full: false,
            power_style: PowerStyle::Menu,
            power_confirm: true,
            power_hide: Vec::new(),
            tray_color: TrayColor::Own,
            launcher_icon: LauncherIcon::Grid,
            quick_icon: QuickIcon::Sliders,
            launcher_custom: String::new(),
            tray_passive: true,
            notifications: true,
            note_place: NotePlace::ByPanel,
            note_width: LaunchSize::Medium,
            note_secs: 6,
            note_max: 4,
            note_look: crate::style::Style::default(),
            clip_on: true,
            clip_length: 50,
            clip_keep: false,
            osd: true,
            osd_keys: true,
            osd_place: OsdPlace::Bottom,
            osd_size: LaunchSize::Medium,
            osd_secs: 1.5,
            osd_color: None,
            ask_passwords: true,
            plugins: Vec::new(),
            hooks: Vec::new(),
            softcore_colors: Vec::new(),
            plugin_set: Vec::new(),
            launch_view: LaunchView::Grid,
            launch_place: LaunchPlace::Center,
            launch_size: LaunchSize::Medium,
            launch_icon: 42,
            launch_rows: 0,
            launch_cols: 0,
            launch_calc: true,
            launch_dim: true,
            launch_favorites: false,
            launch_favorite_apps: Vec::new(),
            launch_source: true,
            night_kelvin: 4000,
            comp_on: false,
            comp_gaps_in: 6,
            comp_gaps_out: 12,
            comp_border: 2,
            comp_radius: 10,
            comp_accent_borders: true,
            comp_inactive_opacity: 1.0,
            comp_shadows: true,
            browser_colors: false,
            quick_vpn: String::new(),
            zerotier_networks: Vec::new(),
            browser_accent: false,
            browser_darker: 0,
            browser_accent_shade: 0,
            themed_apps: Vec::new(),
            idle_on: true,
            idle_dim: 240,
            idle_lock: 300,
            idle_screen_off: 600,
            idle_suspend: 0,
            idle_media: true,
            lock_on_sleep: true,
            lock_cmd: String::new(),
            lock_blur: true,
            lock_power: true,
            dock: true,
            dock_apps: vec!["rivet-files".into(), "rivet-terminal".into(), "rivet-edit".into(), "rivet-notes".into(), "firefox".into()],
            dock_icon: 44.0,
            dock_reserve: true,
            dock_running: true,
            dock_open_apps: true,
            dock_hide: DockHide::Never,
            dock_places: true,
            blur: true,
            dock_look: crate::style::Style::default(),
            widgets: default_widgets(),
            widget_side: Side::Left,
            hud_corner: Corner::Off,
            hud_grid: true,
            desktop_grid: false,
            weather_place: String::new(),
            weather_at: None,
            weather_units: crate::weather::Units::local(),
        }
    }
}

/// Pictures/Wallpapers (or Wallpaper), else Pictures, else home.
pub fn default_wallpaper_dir() -> String {
    let pics = rivet_xdg::places::user_dirs().pictures.unwrap_or_else(rivet_xdg::places::home);
    for name in ["Wallpapers", "Wallpaper", "wallpapers", "wallpaper"] {
        let d = pics.join(name);
        if d.is_dir() {
            return d.to_string_lossy().into_owned();
        }
    }
    pics.to_string_lossy().into_owned()
}

pub fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-shell/settings")
}

/// When the settings file last changed (to notice the Settings window's changes).
/// `#rrggbb` as a colour (anything else: none).
fn rgb_from(v: &str) -> Option<[u8; 3]> {
    let n = u32::from_str_radix(v.strip_prefix('#').filter(|h| h.len() == 6)?, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn rgb_text(c: Option<[u8; 3]>) -> String {
    c.map(|[r, g, b]| format!("#{r:02x}{g:02x}{b:02x}")).unwrap_or_default()
}

pub fn mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(path()).and_then(|m| m.modified()).ok()
}

fn mode_name(m: Mode) -> &'static str {
    match m {
        Mode::System => "system",
        Mode::Light => "light",
        Mode::Dark => "dark",
    }
}

fn mode_from(v: &str) -> Mode {
    match v {
        "light" => Mode::Light,
        "dark" => Mode::Dark,
        _ => Mode::System,
    }
}

/// The single panel of earlier versions (`panel=`, `panel_edge=`, `show_volume=`…), turned into
/// the first of the panels when the file has no panel list yet.
#[derive(Default)]
struct OldPanel {
    seen: bool,
    off: bool,
    bottom: bool,
    edge_to_edge: bool,
    hidden: Vec<&'static str>,
}

impl Settings {
    /// The AI item's providers to read: the ones turned on, and Hermes when it's on.
    pub fn ai_enabled(&self) -> Vec<String> {
        let mut v = self.ai_providers.clone();
        if self.ai_hermes {
            v.push(crate::ai::HERMES.0.to_string());
        }
        v
    }

    /// Whether anything shows the weather: a widget, or the item on a panel.
    pub fn shows_weather(&self) -> bool {
        self.widgets.iter().any(|w| w.kind == WidgetKind::Weather) || self.panels.iter().any(|p| p.has("weather"))
    }

    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return s };
        s.parse(&text);
        s
    }

    fn parse(&mut self, text: &str) {
        let mut old = OldPanel::default();
        let mut panels: Vec<(usize, PanelCfg)> = Vec::new();
        let mut count: Option<usize> = None;
        let mut placed: Vec<(usize, Placed)> = Vec::new();
        let mut widget_count: Option<usize> = None;
        // Each old desktop widget: whether it was on (None: not said), and where.
        #[derive(Default)]
        struct Was {
            clock: (Option<bool>, Option<(f32, f32)>),
            system: (Option<bool>, Option<(f32, f32)>),
            weather: (Option<bool>, Option<(f32, f32)>),
        }
        let mut was = Was::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            let k = k.trim();
            if let Some(name) = k.strip_prefix("wallpaper.").filter(|n| !n.is_empty()) {
                self.set_wallpaper_on(name, v);
                continue;
            }
            if let Some(name) = k.strip_prefix("display.").filter(|n| !n.is_empty()) {
                self.displays.retain(|d| d.name != name);
                self.displays.push(crate::displays::Display::parse(name, v));
                continue;
            }
            if let Some((id, key)) = k.strip_prefix("plugin.").and_then(|r| r.split_once('.')).filter(|(i, k)| !i.is_empty() && !k.is_empty()) {
                self.set_plugin(id, key, v.trim());
                continue;
            }
            if let Some(key) = k.strip_prefix("softcore_color.").filter(|n| !n.is_empty()) {
                self.softcore_colors.retain(|(k, _)| k != key);
                if matches!(v.trim(), "accent" | "secondary" | "tertiary") {
                    self.softcore_colors.push((key.to_string(), v.trim().to_string()));
                    self.softcore_colors.sort();
                }
                continue;
            }
            if let Some(ev) = k.strip_prefix("hook.").filter(|n| !n.is_empty()) {
                self.hooks.retain(|(e, _)| e != ev);
                if !v.trim().is_empty() {
                    self.hooks.push((ev.to_string(), v.trim().to_string()));
                    self.hooks.sort();
                }
                continue;
            }
            if let Some(n) = k.strip_prefix("panel.").and_then(|n| n.parse::<usize>().ok()) {
                panels.push((n, PanelCfg::parse(v)));
                continue;
            }
            if let Some(n) = k.strip_prefix("widget.").and_then(|n| n.parse::<usize>().ok()) {
                if let Some(w) = Placed::parse(v) {
                    placed.push((n, w));
                }
                continue;
            }
            // The old single panel's settings.
            let shown = |id: &'static str, old: &mut OldPanel| {
                old.seen = true;
                if !b {
                    old.hidden.push(id);
                }
            };
            match k {
                "theme" => self.theme = mode_from(v),
                "accent" => {
                    if let Some(h) = v.strip_prefix('#').filter(|h| h.len() == 6)
                        && let Ok(n) = u32::from_str_radix(h, 16)
                    {
                        self.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
                    }
                }
                "surface_dark" => self.surface_dark = rgb_from(v),
                "secondary" => self.secondary = rgb_from(v),
                "tertiary" => self.tertiary = rgb_from(v),
                "surface_light" => self.surface_light = rgb_from(v),
                "surface_tint" => self.surface_tint = b,
                "follow_desktop_colors" => self.follow_desktop_colors = b,
                "text_size" => self.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "color_use" => self.color_use = v.parse::<u8>().unwrap_or(1).min(crate::style::COLOR_USE_MAX),
                "font" => self.font = font_name(v),
                "launcher_font" => self.launcher_font = font_name(v),
                "wallpaper" => self.wallpaper = v.to_string(),
                "wallpaper_dir" => self.wallpaper_dir = v.to_string(),
                "wallpaper_dirs" => self.wallpaper_dirs = v.split('\t').filter(|x| !x.is_empty()).map(String::from).collect(),
                "matugen" => self.matugen = b,
                "palette" => self.palette = v.to_string(),
                "icon_theme" => self.icon_theme = v.to_string(),
                "wall_transition" => self.wall_transition = Transition::from(v),
                "wall_transition_ms" => self.wall_transition_ms = v.parse::<u32>().unwrap_or(600).clamp(100, 5000),
                "matugen_screen" => self.matugen_screen = v.to_string(),
                "matugen_contrast" => self.matugen_contrast = v.parse::<f32>().unwrap_or(0.0).clamp(-1.0, 1.0),
                "color_mode" => self.color_mode = if v == "light" { Mode::Light } else { Mode::Dark },
                "scheme_type" => self.scheme_type = v.to_string(),
                "panel_count" => count = v.parse().ok(),
                "panel" => {
                    old.seen = true;
                    old.off = !b;
                }
                "panel_edge" => {
                    old.seen = true;
                    old.bottom = v == "bottom";
                }
                "panel_floating" => {
                    old.seen = true;
                    old.edge_to_edge = !b;
                }
                "show_workspaces" => shown("workspaces", &mut old),
                "show_volume" => shown("volume", &mut old),
                "show_network" => shown("network", &mut old),
                "show_battery" => shown("battery", &mut old),
                "show_window_title" => shown("window-title", &mut old),
                "show_media" => shown("media", &mut old),
                "show_brightness" => shown("brightness", &mut old),
                "show_bell" => shown("notifications", &mut old),
                "clock_format" => self.clock_format = v.to_string(),
                "ws_style" => self.ws_style = WsStyle::from(v),
                "ws_names" => self.ws_names = b,
                "ws_hide_empty" => self.ws_hide_empty = b,
                "ws_scroll" => self.ws_scroll = b,
                "ai_providers" => self.ai_providers = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "ai_bar" => self.ai_bar = if v.is_empty() { "auto".into() } else { v.to_string() },
                "ai_left" => self.ai_left = b,
                "ai_pins" => self.ai_pins = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "ai_hermes" => self.ai_hermes = b,
                "media_text" => self.media_text = MediaText::from(v),
                "media_width" => self.media_width = v.parse().unwrap_or(230).clamp(80, 480),
                "media_art" => self.media_art = b,
                "media_controls" => self.media_controls = b,
                "media_viz" => self.media_viz = b,
                "media_click_plays" => self.media_click_plays = b,
                "media_scroll" => self.media_scroll = b,
                "media_hide_paused" => self.media_hide_paused = b,
                "vol_show" => self.vol_show = Show::from(v),
                "vol_scroll" => self.vol_scroll = b,
                "bright_show" => self.bright_show = Show::from(v),
                "bright_scroll" => self.bright_scroll = b,
                "batt_show" => self.batt_show = Show::from(v),
                "batt_hide_full" => self.batt_hide_full = b,
                "power_style" => self.power_style = PowerStyle::from(v),
                "power_confirm" => self.power_confirm = b,
                "power_hide" => self.power_hide = v.split(',').filter(|x| !x.is_empty()).map(String::from).collect(),
                "tray_color" => self.tray_color = TrayColor::from(v),
                "launcher_icon" => self.launcher_icon = LauncherIcon::from(v),
                "quick_icon" => self.quick_icon = QuickIcon::from(v),
                "launcher_custom" => self.launcher_custom = v.to_string(),
                "tray_passive" => self.tray_passive = b,
                "notifications" => self.notifications = b,
                "note_place" => self.note_place = NotePlace::from(v),
                "note_width" => self.note_width = LaunchSize::from(v),
                "note_secs" => self.note_secs = v.parse::<u32>().unwrap_or(6).clamp(2, 60),
                "note_max" => self.note_max = v.parse::<usize>().unwrap_or(4).clamp(1, 8),
                "note_look" => self.note_look = crate::style::Style::parse(v, '=', ';'),
                "clip_on" => self.clip_on = b,
                "clip_length" => self.clip_length = v.parse::<u32>().unwrap_or(50).clamp(*crate::clip::LENGTH.start(), *crate::clip::LENGTH.end()),
                "clip_keep" => self.clip_keep = b,
                "osd" => self.osd = b,
                "comp_on" => self.comp_on = b,
                "comp_gaps_in" => self.comp_gaps_in = v.parse::<u32>().unwrap_or(6).min(64),
                "comp_gaps_out" => self.comp_gaps_out = v.parse::<u32>().unwrap_or(12).min(96),
                "comp_border" => self.comp_border = v.parse::<u32>().unwrap_or(2).min(16),
                "comp_radius" => self.comp_radius = v.parse::<u32>().unwrap_or(10).min(40),
                "comp_accent_borders" => self.comp_accent_borders = b,
                "comp_inactive_opacity" => self.comp_inactive_opacity = v.parse::<f32>().unwrap_or(1.0).clamp(0.5, 1.0),
                "comp_shadows" => self.comp_shadows = b,
                "browser_colors" => self.browser_colors = b,
                "quick_vpn" => self.quick_vpn = v.to_string(),
                "zerotier_networks" => self.zerotier_networks = v.split('|').filter_map(|n| n.split_once(':')).map(|(id, name)| (id.to_string(), name.to_string())).collect(),
                "browser_accent" => self.browser_accent = b,
                "browser_darker" => self.browser_darker = v.parse::<u32>().unwrap_or(0).min(100),
                "browser_accent_shade" => self.browser_accent_shade = v.parse::<i32>().unwrap_or(0).clamp(-100, 100),
                "themed_apps" => self.themed_apps = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "idle_on" => self.idle_on = b,
                "idle_dim" => self.idle_dim = v.parse::<u32>().unwrap_or(0).min(86_400),
                "idle_lock" => self.idle_lock = v.parse::<u32>().unwrap_or(0).min(86_400),
                "idle_screen_off" => self.idle_screen_off = v.parse::<u32>().unwrap_or(0).min(86_400),
                "idle_suspend" => self.idle_suspend = v.parse::<u32>().unwrap_or(0).min(86_400),
                "idle_media" => self.idle_media = b,
                "lock_on_sleep" => self.lock_on_sleep = b,
                "lock_cmd" => self.lock_cmd = v.to_string(),
                "lock_blur" => self.lock_blur = b,
                "lock_power" => self.lock_power = b,
                "osd_keys" => self.osd_keys = b,
                "osd_place" => self.osd_place = OsdPlace::from(v),
                "osd_size" => self.osd_size = LaunchSize::from(v),
                "osd_secs" => self.osd_secs = v.parse::<f32>().unwrap_or(1.5).clamp(0.5, 5.0),
                "osd_color" => self.osd_color = crate::style::Role::from(v).filter(|r| *r != crate::style::Role::Surface),
                "ask_passwords" => self.ask_passwords = b,
                "plugins" => self.plugins = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "launch_view" => self.launch_view = LaunchView::from(v),
                "launch_place" => self.launch_place = LaunchPlace::from(v),
                "launch_size" => self.launch_size = LaunchSize::from(v),
                "launch_icon" => self.launch_icon = v.parse::<u32>().unwrap_or(42).clamp(24, 64),
                "launch_rows" => self.launch_rows = v.parse::<u32>().unwrap_or(0).min(20),
                "launch_cols" => self.launch_cols = v.parse::<u32>().unwrap_or(0).min(12),
                "launch_calc" => self.launch_calc = b,
                "launch_dim" => self.launch_dim = b,
                "launch_favorites" => self.launch_favorites = b,
                "launch_source" => self.launch_source = b,
                "launch_favorite_apps" => self.launch_favorite_apps = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "night_kelvin" => self.night_kelvin = v.parse::<u32>().unwrap_or(4000).clamp(2500, 6000),
                "dock_open_apps" => self.dock_open_apps = b,
                "dock_places" => self.dock_places = b,
                "blur" => self.blur = b,
                "dock_hide" => {
                    self.dock_hide = match v {
                        "auto" => DockHide::Auto,
                        "smart" => DockHide::Smart,
                        _ => DockHide::Never,
                    }
                }
                "dock" => self.dock = b,
                "look" => self.look = crate::style::Style::parse(v, '=', ';'),
                "dock_look" => self.dock_look = crate::style::Style::parse(v, '=', ';'),
                "dock_apps" => self.dock_apps = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "dock_icon" => self.dock_icon = v.parse::<f32>().unwrap_or(44.0).clamp(24.0, 96.0),
                "dock_reserve" => self.dock_reserve = b,
                "dock_running" => self.dock_running = b,
                "widget_count" => widget_count = v.parse().ok(),
                "hud_corner" => self.hud_corner = Corner::from(v),
                "hud_grid" => self.hud_grid = b,
                "desktop_grid" => self.desktop_grid = b,
                "widget_side" => self.widget_side = if v == "right" { Side::Right } else { Side::Left },
                // The three desktop widgets as they were kept before there was a list.
                "widget_clock" => was.clock.0 = Some(b),
                "widget_system" => was.system.0 = Some(b),
                "widget_weather" => was.weather.0 = Some(b),
                "widget_clock_at" => was.clock.1 = fraction_pair(v),
                "widget_system_at" => was.system.1 = fraction_pair(v),
                "widget_weather_at" => was.weather.1 = fraction_pair(v),
                "weather_place" => self.weather_place = v.to_string(),
                "weather_at" => self.weather_at = lat_lon(v),
                "weather_units" => self.weather_units = crate::weather::Units::parse(v).unwrap_or(self.weather_units),
                _ => {}
            }
        }
        if widget_count.is_some() || !placed.is_empty() {
            placed.sort_by_key(|p| p.0);
            self.widgets = placed.into_iter().map(|p| p.1).take(widget_count.unwrap_or(usize::MAX)).collect();
        } else if was.clock.0.is_some() || was.system.0.is_some() || was.weather.0.is_some() {
            // A file from before the list: its desktop widgets where they were, on (or off)
            // as they were, and the HUD as a new setup has it.
            let desktop = [(WidgetKind::Clock, was.clock, true), (WidgetKind::System, was.system, true), (WidgetKind::Weather, was.weather, false)];
            let mut list: Vec<Placed> = desktop.into_iter().filter(|(_, (on, _), usual)| on.unwrap_or(*usual)).map(|(kind, (_, at), _)| Placed { at, ..Placed::new(kind, Board::Desktop) }).collect();
            list.extend(default_widgets().into_iter().filter(|w| w.board == Board::Hud));
            self.widgets = list;
        }
        if count.is_some() || !panels.is_empty() {
            panels.sort_by_key(|p| p.0);
            self.panels = panels.into_iter().map(|p| p.1).take(count.unwrap_or(usize::MAX)).collect();
            self.tidy_panels();
        } else if old.seen {
            self.panels = if old.off {
                Vec::new()
            } else {
                let mut p = PanelCfg { edge: if old.bottom { Edge::Bottom } else { Edge::Top }, floating: !old.edge_to_edge, ..PanelCfg::default() };
                for z in &mut p.zones {
                    z.retain(|i| !old.hidden.contains(&i.as_str()));
                }
                vec![p]
            };
        }
    }

    pub fn text(&self) -> String {
        let [r, g, b] = self.accent;
        let one = |s: &str| s.replace('\n', " ");
        let mut t = format!(
            "theme={}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nwallpaper={}\nwallpaper_dir={}\nmatugen={}\ncolor_mode={}\nscheme_type={}\nclock_format={}\nws_style={}\nws_names={}\nws_hide_empty={}\nws_scroll={}\nai_providers={}\nai_bar={}\nai_left={}\nai_pins={}\nai_hermes={}\nvol_show={}\nvol_scroll={}\nbright_show={}\nbright_scroll={}\nbatt_show={}\nbatt_hide_full={}\ndock={}\ndock_apps={}\ndock_icon={}\ndock_reserve={}\ndock_running={}\nwidget_side={}\nnotifications={}\nosd={}\nnight_kelvin={}\ndock_open_apps={}\ndock_hide={}\npanel_count={}\n",
            mode_name(self.theme),
            self.follow_desktop_colors,
            self.text_size,
            one(&self.wallpaper),
            one(&self.wallpaper_dir),
            self.matugen,
            mode_name(self.color_mode),
            one(&self.scheme_type),
            one(&self.clock_format),
            self.ws_style.name(),
            self.ws_names,
            self.ws_hide_empty,
            self.ws_scroll,
            self.ai_providers.join(","),
            one(&self.ai_bar),
            self.ai_left,
            self.ai_pins.join(","),
            self.ai_hermes,
            self.vol_show.name(),
            self.vol_scroll,
            self.bright_show.name(),
            self.bright_scroll,
            self.batt_show.name(),
            self.batt_hide_full,
            self.dock,
            self.dock_apps.join(","),
            self.dock_icon,
            self.dock_reserve,
            self.dock_running,
            if self.widget_side == Side::Right { "right" } else { "left" },
            self.notifications,
            self.osd,
            self.night_kelvin,
            self.dock_open_apps,
            self.dock_hide.name(),
            self.panels.len(),
        );
        t.push_str(&format!("wall_transition={}\nwall_transition_ms={}\nmatugen_screen={}\nmatugen_contrast={}\npalette={}\nicon_theme={}\nsurface_dark={}\nsurface_light={}\nsurface_tint={}\n", self.wall_transition.name(), self.wall_transition_ms, one(&self.matugen_screen), self.matugen_contrast, one(&self.palette), one(&self.icon_theme), rgb_text(self.surface_dark), rgb_text(self.surface_light), self.surface_tint));
        for (name, pic) in &self.wallpaper_screens {
            t.push_str(&format!("wallpaper.{}={}\n", one(name), one(pic)));
        }
        for d in &self.displays {
            t.push_str(&format!("display.{}={}\n", one(&d.name), d.text()));
        }
        t.push_str(&format!("power_style={}\npower_confirm={}\npower_hide={}\n", self.power_style.name(), self.power_confirm, self.power_hide.join(",")));
        t.push_str(&format!("tray_color={}\ntray_passive={}\n", self.tray_color.name(), self.tray_passive));
        t.push_str(&format!("color_use={}\nsecondary={}\ntertiary={}\n", self.color_use, rgb_text(self.secondary), rgb_text(self.tertiary)));
        let at = |p: Option<(f32, f32)>| p.map(|(x, y)| format!("{x:.4},{y:.4}")).unwrap_or_default();
        t.push_str(&format!("weather_place={}\nweather_at={}\nweather_units={}\n", one(&self.weather_place), at(self.weather_at), self.weather_units.name()));
        t.push_str(&format!("hud_corner={}\nhud_grid={}\ndesktop_grid={}\nwidget_count={}\n", self.hud_corner.name(), self.hud_grid, self.desktop_grid, self.widgets.len()));
        for (i, w) in self.widgets.iter().enumerate() {
            t.push_str(&format!("widget.{i}={}\n", w.text()));
        }
        t.push_str(&format!("font={}\nlauncher_font={}\n", font_name(&self.font), font_name(&self.launcher_font)));
        t.push_str(&format!("quick_icon={}\n", self.quick_icon.name()));
        t.push_str(&format!("launcher_icon={}\nlauncher_custom={}\n", self.launcher_icon.name(), self.launcher_custom.replace('\n', "")));
        t.push_str(&format!("wallpaper_dirs={}\n", self.wallpaper_dirs.iter().map(|d| d.replace(['\t', '\n'], " ")).collect::<Vec<_>>().join("\t")));
        t.push_str(&format!("look={}\ndock_look={}\n", self.look.text('=', ';'), self.dock_look.text('=', ';')));
        t.push_str(&format!(
            "comp_on={}\ncomp_gaps_in={}\ncomp_gaps_out={}\ncomp_border={}\ncomp_radius={}\ncomp_accent_borders={}\ncomp_inactive_opacity={}\ncomp_shadows={}\n",
            self.comp_on, self.comp_gaps_in, self.comp_gaps_out, self.comp_border, self.comp_radius, self.comp_accent_borders, self.comp_inactive_opacity, self.comp_shadows
        ));
        t.push_str(&format!("browser_colors={}\nbrowser_accent={}\nbrowser_darker={}\nbrowser_accent_shade={}\nthemed_apps={}\nquick_vpn={}\nzerotier_networks={}\n", self.browser_colors, self.browser_accent, self.browser_darker, self.browser_accent_shade, self.themed_apps.join(","), self.quick_vpn.replace('\n', ""), self.zerotier_networks.iter().map(|(id, name)| format!("{id}:{}", name.replace(['|', '\n'], " "))).collect::<Vec<_>>().join("|")));
        t.push_str(&format!(
            "idle_on={}\nidle_dim={}\nidle_lock={}\nidle_screen_off={}\nidle_suspend={}\nidle_media={}\nlock_on_sleep={}\nlock_cmd={}\nlock_blur={}\nlock_power={}\n",
            self.idle_on, self.idle_dim, self.idle_lock, self.idle_screen_off, self.idle_suspend, self.idle_media, self.lock_on_sleep, one(&self.lock_cmd), self.lock_blur, self.lock_power
        ));
        t.push_str(&format!("osd_keys={}\nask_passwords={}\n", self.osd_keys, self.ask_passwords));
        t.push_str(&format!("clip_on={}\nclip_length={}\nclip_keep={}\n", self.clip_on, self.clip_length, self.clip_keep));
        t.push_str(&format!(
            "osd_place={}\nosd_size={}\nosd_secs={}\nosd_color={}\nnote_place={}\nnote_width={}\nnote_secs={}\nnote_max={}\nnote_look={}\n",
            self.osd_place.name(),
            self.osd_size.name(),
            self.osd_secs,
            self.osd_color.map_or("auto", |r| r.name()),
            self.note_place.name(),
            self.note_width.name(),
            self.note_secs,
            self.note_max,
            self.note_look.text('=', ';')
        ));
        t.push_str(&format!(
            "media_text={}\nmedia_width={}\nmedia_art={}\nmedia_controls={}\nmedia_viz={}\nmedia_click_plays={}\nmedia_scroll={}\nmedia_hide_paused={}\n",
            self.media_text.name(), self.media_width, self.media_art, self.media_controls, self.media_viz, self.media_click_plays, self.media_scroll, self.media_hide_paused
        ));
        t.push_str(&format!("dock_places={}\nblur={}\n", self.dock_places, self.blur));
        t.push_str(&format!("launch_rows={}\nlaunch_cols={}\n", self.launch_rows, self.launch_cols));
        t.push_str(&format!("launch_view={}\nlaunch_place={}\nlaunch_size={}\nlaunch_icon={}\nlaunch_calc={}\nlaunch_dim={}\nlaunch_favorites={}\nlaunch_favorite_apps={}\nlaunch_source={}\n", self.launch_view.name(), self.launch_place.name(), self.launch_size.name(), self.launch_icon, self.launch_calc, self.launch_dim, self.launch_favorites, self.launch_favorite_apps.join(","), self.launch_source));
        t.push_str(&format!("plugins={}\n", self.plugins.join(",")));
        for (ev, cmd) in self.hooks.iter().filter(|(e, c)| !e.is_empty() && !c.trim().is_empty()) {
            t.push_str(&format!("hook.{ev}={}\n", one(cmd)));
        }
        for (key, role) in &self.softcore_colors {
            t.push_str(&format!("softcore_color.{key}={role}\n"));
        }
        for (id, key, v) in &self.plugin_set {
            t.push_str(&format!("plugin.{id}.{key}={}\n", one(v)));
        }
        for (i, p) in self.panels.iter().enumerate() {
            t.push_str(&format!("panel.{i}={}\n", p.text()));
        }
        t
    }

    /// The picture a screen shows: its own, else the shared one.
    pub fn wallpaper_on(&self, output: &str) -> &str {
        self.wallpaper_screens.iter().find(|(n, _)| n == output && !output.is_empty()).map(|(_, p)| p.as_str()).unwrap_or(&self.wallpaper)
    }

    /// Whether a screen has a picture of its own.
    pub fn has_own_wallpaper(&self, output: &str) -> bool {
        self.wallpaper_screens.iter().any(|(n, _)| n == output)
    }

    /// Give a screen its own picture (an empty one: back to the shared picture).
    pub fn set_wallpaper_on(&mut self, output: &str, picture: &str) {
        self.wallpaper_screens.retain(|(n, _)| n != output);
        if !picture.is_empty() {
            self.wallpaper_screens.push((output.to_string(), picture.to_string()));
            self.wallpaper_screens.sort();
        }
    }

    /// The picture the colours are made from.
    pub fn matugen_picture(&self) -> &str {
        self.wallpaper_on(&self.matugen_screen)
    }

    /// A plugin's setting ("" when it was never set).
    pub fn plugin_value(&self, id: &str, key: &str) -> &str {
        self.plugin_set.iter().find(|(p, k, _)| p == id && k == key).map(|(_, _, v)| v.as_str()).unwrap_or("")
    }

    /// How a plugin's panel icon shows: its own colours, or one of the panel's.
    pub fn plugin_icon(&self, id: &str) -> TrayColor {
        TrayColor::from(self.plugin_value(id, crate::plugins::ICON_COLOR))
    }

    /// Set a plugin's setting (an empty value forgets it).
    pub fn set_plugin(&mut self, id: &str, key: &str, value: &str) {
        self.plugin_set.retain(|(p, k, _)| !(p == id && k == key));
        if !value.is_empty() {
            self.plugin_set.push((id.to_string(), key.to_string(), value.to_string()));
            self.plugin_set.sort();
        }
    }

    pub fn save(&self) {
        if NO_SAVE.load(Ordering::Relaxed) {
            return;
        }
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, self.text()).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
        // The chosen palette, or the shell's own style and accent, as the colour file every Rivet
        // app reads (the colours are the desktop's: no such file).
        let css = rivet_look::schemes::Scheme::by_name(&self.palette).map(|s| s.palette_css()).or_else(|| {
            let dark = match self.theme {
                Mode::Dark => true,
                Mode::Light => false,
                Mode::System => rivet_look::palette::prefers_dark(),
            };
            self.own_colors().then(|| self.own_css(dark))
        });
        if let Err(e) = rivet_look::palette::set_chosen(css.as_deref()) {
            eprintln!("rivet-shell: the palette's colors: {e}");
        }
    }

    /// The usual surface greys, dark and light.
    pub const SURFACES: [[u8; 3]; 2] = rivet_lockscreen::look::SURFACES;

    /// The shell's own surface colour in its dark style or its light one: the chosen one (else
    /// the usual grey), with a little of the accent when tinted. None: the usual, untouched.
    pub fn surface(&self, dark: bool) -> Option<eframe::egui::Color32> {
        rivet_lockscreen::look::surface(self.accent, if dark { self.surface_dark } else { self.surface_light }, self.surface_tint, dark)
    }

    /// How the lock screen looks (and the login screen, which is left a copy).
    pub fn lock_look(&self) -> rivet_lockscreen::Look {
        rivet_lockscreen::Look {
            theme: self.theme,
            accent: self.accent,
            secondary: self.secondary,
            tertiary: self.tertiary,
            color_use: self.color_use,
            text_size: self.text_size,
            font: self.font.clone(),
            clock_format: self.clock_format.clone(),
            blur: self.lock_blur,
            power: self.lock_power,
            surface_dark: self.surface_dark,
            surface_light: self.surface_light,
            surface_tint: self.surface_tint,
            follow_desktop_colors: self.follow_desktop_colors,
            own_surfaces: self.own_colors(),
        }
    }

    /// Are the colours the shell's own (its style, accent and surface), not the desktop's or a palette's?
    pub fn own_colors(&self) -> bool {
        self.palette.is_empty() && !self.follow_desktop_colors
    }

    /// The shell's own colours as a colour file's text, for its dark style or its light one.
    pub fn own_css(&self, dark: bool) -> String {
        let [r, g, b] = self.accent;
        rivet_look::palette::own_css(dark, eframe::egui::Color32::from_rgb(r, g, b), self.surface(dark))
    }

    /// Give a look the shell's own colours, when those are the ones in use.
    pub fn set_surfaces(&self, look: &mut rivet_look::live::LiveLook) {
        if !self.own_colors() {
            look.set_own("", || [None, None]);
            return;
        }
        let key = format!("{:?}{:?}{:?}{}", self.accent, self.surface_dark, self.surface_light, self.surface_tint);
        look.set_own(&key, || [rivet_look::palette::from_css(&self.own_css(true)), rivet_look::palette::from_css(&self.own_css(false))]);
    }

    /// Does the shell make the colours from the wallpaper (with matugen)? Only when that's where
    /// the colours come from: not with a palette chosen, nor with the shell's own style and accent.
    pub fn makes_colors(&self) -> bool {
        self.matugen && self.palette.is_empty() && self.follow_desktop_colors
    }

    /// Choose a built-in palette (empty: none). The shell follows it, in its light or dark; while
    /// one is chosen no colours are made from the wallpaper (see `makes_colors`).
    pub fn choose_palette(&mut self, name: &str) {
        let Some(sc) = rivet_look::schemes::Scheme::by_name(name) else {
            self.palette.clear();
            return;
        };
        self.palette = sc.name.to_string();
        self.follow_desktop_colors = true;
        self.color_mode = if sc.dark { Mode::Dark } else { Mode::Light };
    }

    /// The font a surface's text is in (a family's name; empty: the desktop's).
    pub fn font_for(&self, face: crate::ui::Face) -> &str {
        let own = match face {
            crate::ui::Face::Base => "",
            crate::ui::Face::Panel(i) => self.panels.get(i).map(|p| p.font.as_str()).unwrap_or(""),
            crate::ui::Face::Launcher => &self.launcher_font,
        };
        if own.is_empty() { &self.font } else { own }
    }

    /// The secondary and tertiary chosen, when the colours are a style and accent of the shell's
    /// own (with the wallpaper's or a palette's, the theme's own count).
    pub fn own_roles(&self) -> (Option<eframe::egui::Color32>, Option<eframe::egui::Color32>) {
        if !self.own_colors() {
            return (None, None);
        }
        let c = |c: Option<[u8; 3]>| c.map(|[r, g, b]| eframe::egui::Color32::from_rgb(r, g, b));
        (c(self.secondary), c(self.tertiary))
    }

    pub fn choice(&self) -> rivet_look::live::Choice {
        rivet_look::live::Choice { mode: self.theme, accent: self.accent, follow_desktop_colors: self.follow_desktop_colors, text_size: self.text_size }
    }
}

/// "latitude,longitude" in degrees (the weather's place); anything else is None.
fn lat_lon(v: &str) -> Option<(f32, f32)> {
    let (lat, lon) = v.split_once(',')?;
    let (lat, lon) = (lat.trim().parse::<f32>().ok()?, lon.trim().parse::<f32>().ok()?);
    (lat.abs() <= 90.0 && lon.abs() <= 180.0).then_some((lat, lon))
}

/// "x,y" with both from 0 to 1 (a widget's place); anything else is None.
fn fraction_pair(v: &str) -> Option<(f32, f32)> {
    let (x, y) = v.split_once(',')?;
    let (x, y) = (x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?);
    (x.is_finite() && y.is_finite()).then(|| (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The three desktop widgets a file from before the list had come over as they were, and
    /// taking every widget away stays that way.
    #[test]
    fn widgets_come_over_from_the_old_keys() {
        let mut s = Settings::default();
        s.parse("widget_clock=false\nwidget_system=true\nwidget_system_at=0.5,0.25\nwidget_weather=true\n");
        let desktop: Vec<_> = s.widgets.iter().filter(|w| w.board == Board::Desktop).map(|w| (w.kind, w.at)).collect();
        assert_eq!(desktop, [(WidgetKind::System, Some((0.5, 0.25))), (WidgetKind::Weather, None)]);
        assert!(s.widgets.iter().any(|w| w.board == Board::Hud && w.kind == WidgetKind::Apps));
        assert!(s.shows_weather());
        s.widgets.clear();
        let mut t = Settings::default();
        t.parse(&s.text());
        assert!(t.widgets.is_empty());
        // The HUD a new setup has fits in its room, each widget clear of the next.
        let hud: Vec<Placed> = default_widgets().into_iter().filter(|w| w.board == Board::Hud).collect();
        assert!(hud.iter().all(|w| w.at.is_some() && w.size.is_some_and(|(w, h)| w <= 1.0 && h <= 1.0)));
    }

    #[test]
    fn round_trips() {
        let mut s = Settings { wallpaper: "/p/a b.jpg".into(), dock_hide: DockHide::Smart, dock_places: false, blur: false, widget_side: Side::Right, color_mode: Mode::Light, ws_style: WsStyle::Dots, ws_names: false, ws_hide_empty: true, ws_scroll: false, ai_providers: vec!["claude".into()], ai_bar: "codex".into(), ai_left: true, ai_pins: vec!["claude:5-hour".into(), "claude:Weekly · Fable".into()], ai_hermes: false, vol_show: Show::Icon, vol_scroll: false, bright_show: Show::Text, bright_scroll: false, batt_show: Show::Icon, batt_hide_full: true, ..Settings::default() };
        s.dock_apps = vec!["a".into(), "b.desktop".into()];
        s.widgets = vec![Placed { at: Some((0.25, 0.5)), ..Placed::new(WidgetKind::System, Board::Desktop) }, Placed { output: "DP-2".into(), at: Some((1.0, 0.0)), size: Some((0.25, 0.5)), ..Placed::new(WidgetKind::Weather, Board::Hud) }];
        (s.hud_corner, s.hud_grid, s.desktop_grid) = (Corner::BottomRight, false, true);
        (s.weather_place, s.weather_at, s.weather_units) = ("Portland, Oregon".into(), Some((45.5234, -122.6762)), crate::weather::Units::Imperial);
        s.secondary = Some([0x35, 0x84, 0xe4]);
        (s.wallpaper_screens, s.wall_transition, s.wall_transition_ms, s.matugen_screen, s.matugen_contrast) = (vec![("DP-2".into(), "/p/two.png".into())], Transition::Grow, 900, "DP-2".into(), 0.5);
        assert_eq!((s.wallpaper_on("DP-2"), s.wallpaper_on("DP-1"), s.matugen_picture()), ("/p/two.png", "/p/a b.jpg", "/p/two.png"));
        (s.power_style, s.power_confirm, s.power_hide) = (PowerStyle::Grid, false, vec!["suspend".into(), "logout".into()]);
        (s.clip_on, s.clip_length, s.clip_keep) = (false, 120, true);
        s.panels.push(PanelCfg { screen: "DP-2".into(), ..PanelCfg::new_bottom() });
        let mut t = Settings::default();
        t.parse(&s.text());
        assert_eq!(s, t);
        // No panels at all stays no panels.
        let none = Settings { panels: Vec::new(), ..Settings::default() };
        let mut t = Settings::default();
        t.parse(&none.text());
        assert!(t.panels.is_empty());
    }

    #[test]
    fn fonts_round_trip_and_cascade() {
        use crate::ui::Face;
        let mut s = Settings { font: "Inter".into(), launcher_font: "DejaVu Serif".into(), ..Settings::default() };
        s.panels.push(PanelCfg { font: "Maple Mono NF".into(), ..PanelCfg::new_bottom() });
        let mut t = Settings::default();
        t.parse(&s.text());
        assert_eq!(s, t);
        assert_eq!(t.font_for(Face::Base), "Inter");
        assert_eq!(t.font_for(Face::Panel(0)), "Inter");
        assert_eq!(t.font_for(Face::Panel(1)), "Maple Mono NF");
        assert_eq!(t.font_for(Face::Launcher), "DejaVu Serif");
        t.launcher_font.clear();
        assert_eq!(t.font_for(Face::Launcher), "Inter");
        assert_eq!(Settings::default().font_for(Face::Panel(0)), "");
    }

    #[test]
    fn comp_round_trips() {
        let s = Settings { comp_on: true, comp_gaps_in: 3, comp_gaps_out: 20, comp_border: 0, comp_radius: 14, comp_accent_borders: false, comp_inactive_opacity: 0.85, comp_shadows: false, browser_colors: true, browser_accent: true, browser_darker: 40, browser_accent_shade: -25, themed_apps: vec!["kitty".into(), "zed".into()], quick_vpn: "zerotier:8056c2e21c000001".into(), zerotier_networks: vec![("8056c2e21c000001".into(), "home: lab".into()), ("1d71939404000002".into(), String::new())], ..Settings::default() };
        let mut t = Settings::default();
        t.parse(&s.text());
        assert_eq!(s, t);
    }

    #[test]
    fn idle_round_trips() {
        let s = Settings { idle_on: false, idle_dim: 0, idle_lock: 120, idle_screen_off: 900, idle_suspend: 1800, idle_media: false, lock_on_sleep: false, lock_cmd: "swaylock -f".into(), lock_blur: false, lock_power: false, osd_keys: false, osd_place: OsdPlace::Top, osd_size: LaunchSize::Large, osd_secs: 2.5, osd_color: Some(crate::style::Role::Tertiary), note_place: NotePlace::BottomLeft, note_width: LaunchSize::Large, note_secs: 12, note_max: 2, note_look: crate::style::Style { radius: Some(4.0), opacity: Some(0.8), ..Default::default() }, ask_passwords: false, media_text: MediaText::ArtistTitle, media_width: 300, media_art: false, media_controls: true, media_viz: true, media_click_plays: true, media_scroll: false, media_hide_paused: true, plugins: vec!["disk".into()], plugin_set: vec![("notes".into(), "vault".into(), "/home/me/My Notes".into())], hooks: vec![("lock".into(), "playerctl pause".into()), ("wallpaper".into(), "echo $RIVET_PATH >> /tmp/w".into())], softcore_colors: vec![("border_active".into(), "accent".into()), ("border_active_to".into(), "tertiary".into())], launch_view: LaunchView::List, launch_place: LaunchPlace::Panel, launch_size: LaunchSize::Large, launch_icon: 56, launch_rows: 7, launch_cols: 5, launch_calc: false, launch_dim: false, launch_favorites: true, launch_favorite_apps: vec!["firefox".into(), "rivet-files".into()], launch_source: false, ..Settings::default() };
        let mut t = Settings::default();
        t.parse(&s.text());
        assert_eq!(s, t);
    }

    #[test]
    fn takes_in_the_old_single_panel() {
        let mut t = Settings::default();
        t.parse("panel=true\npanel_edge=bottom\npanel_floating=false\nshow_volume=false\nshow_bell=true\n");
        assert_eq!(t.panels.len(), 1);
        let p = &t.panels[0];
        assert!(p.edge == Edge::Bottom && !p.floating && !p.has("volume") && p.has("notifications") && p.has("clock"));
        let mut t = Settings::default();
        t.parse("panel=false\n");
        assert!(t.panels.is_empty());
    }
}
