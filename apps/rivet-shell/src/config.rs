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
}

/// When the dock hides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum DockHide {
    /// Always shown.
    #[default]
    Never,
    /// Slides away; comes back when the pointer reaches the screen's bottom edge.
    Auto,
    /// Shown while the workspace has no windows; else like Auto.
    Smart,
}

impl DockHide {
    pub fn name(self) -> &'static str {
        match self {
            DockHide::Never => "never",
            DockHide::Auto => "auto",
            DockHide::Smart => "smart",
        }
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
}

impl WsStyle {
    pub const ALL: [WsStyle; 3] = [WsStyle::Pills, WsStyle::Dots, WsStyle::Numbers];
    pub fn name(self) -> &'static str {
        match self {
            WsStyle::Pills => "pills",
            WsStyle::Dots => "dots",
            WsStyle::Numbers => "numbers",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            WsStyle::Pills => "Pills",
            WsStyle::Dots => "Dots",
            WsStyle::Numbers => "Numbers",
        }
    }
    pub fn from(v: &str) -> WsStyle {
        WsStyle::ALL.into_iter().find(|w| w.name() == v).unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    #[default]
    Left,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    // ---- the look (as every Rivet app)
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    pub text_size: f32,
    // ---- wallpaper and colours
    /// The picture on the desktop (empty: a plain background in the theme's colours).
    pub wallpaper: String,
    /// Where the Settings window looks for pictures.
    pub wallpaper_dir: String,
    /// Run matugen when the wallpaper changes, so every app takes its colours.
    pub matugen: bool,
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
    // ---- notifications, on-screen display, night light
    /// Be the notification service (cards in the corner, a history in the control center).
    pub notifications: bool,
    /// Show volume and brightness changes in the middle of the screen's bottom.
    pub osd: bool,
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
    // ---- desktop widgets
    pub widget_clock: bool,
    pub widget_system: bool,
    pub widget_side: Side,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            follow_desktop_colors: true,
            text_size: 0.0,
            wallpaper: String::new(),
            wallpaper_dir: default_wallpaper_dir(),
            matugen: true,
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
            notifications: true,
            osd: true,
            night_kelvin: 4000,
            comp_on: false,
            comp_gaps_in: 6,
            comp_gaps_out: 12,
            comp_border: 2,
            comp_radius: 10,
            comp_accent_borders: true,
            comp_inactive_opacity: 1.0,
            comp_shadows: true,
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
            widget_clock: true,
            widget_system: true,
            widget_side: Side::Left,
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
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            let k = k.trim();
            if let Some(n) = k.strip_prefix("panel.").and_then(|n| n.parse::<usize>().ok()) {
                panels.push((n, PanelCfg::parse(v)));
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
                "follow_desktop_colors" => self.follow_desktop_colors = b,
                "text_size" => self.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "wallpaper" => self.wallpaper = v.to_string(),
                "wallpaper_dir" => self.wallpaper_dir = v.to_string(),
                "matugen" => self.matugen = b,
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
                "notifications" => self.notifications = b,
                "osd" => self.osd = b,
                "comp_on" => self.comp_on = b,
                "comp_gaps_in" => self.comp_gaps_in = v.parse::<u32>().unwrap_or(6).min(64),
                "comp_gaps_out" => self.comp_gaps_out = v.parse::<u32>().unwrap_or(12).min(96),
                "comp_border" => self.comp_border = v.parse::<u32>().unwrap_or(2).min(16),
                "comp_radius" => self.comp_radius = v.parse::<u32>().unwrap_or(10).min(40),
                "comp_accent_borders" => self.comp_accent_borders = b,
                "comp_inactive_opacity" => self.comp_inactive_opacity = v.parse::<f32>().unwrap_or(1.0).clamp(0.5, 1.0),
                "comp_shadows" => self.comp_shadows = b,
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
                "night_kelvin" => self.night_kelvin = v.parse::<u32>().unwrap_or(4000).clamp(2500, 6000),
                "dock_open_apps" => self.dock_open_apps = b,
                "dock_hide" => {
                    self.dock_hide = match v {
                        "auto" => DockHide::Auto,
                        "smart" => DockHide::Smart,
                        _ => DockHide::Never,
                    }
                }
                "dock" => self.dock = b,
                "dock_apps" => self.dock_apps = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "dock_icon" => self.dock_icon = v.parse::<f32>().unwrap_or(44.0).clamp(24.0, 96.0),
                "dock_reserve" => self.dock_reserve = b,
                "dock_running" => self.dock_running = b,
                "widget_clock" => self.widget_clock = b,
                "widget_system" => self.widget_system = b,
                "widget_side" => self.widget_side = if v == "right" { Side::Right } else { Side::Left },
                _ => {}
            }
        }
        if count.is_some() || !panels.is_empty() {
            panels.sort_by_key(|p| p.0);
            self.panels = panels.into_iter().map(|p| p.1).take(count.unwrap_or(usize::MAX)).collect();
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
            "theme={}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nwallpaper={}\nwallpaper_dir={}\nmatugen={}\ncolor_mode={}\nscheme_type={}\nclock_format={}\nws_style={}\nws_names={}\nws_hide_empty={}\nws_scroll={}\nai_providers={}\nai_bar={}\nai_left={}\nai_pins={}\ndock={}\ndock_apps={}\ndock_icon={}\ndock_reserve={}\ndock_running={}\nwidget_clock={}\nwidget_system={}\nwidget_side={}\nnotifications={}\nosd={}\nnight_kelvin={}\ndock_open_apps={}\ndock_hide={}\npanel_count={}\n",
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
            self.dock,
            self.dock_apps.join(","),
            self.dock_icon,
            self.dock_reserve,
            self.dock_running,
            self.widget_clock,
            self.widget_system,
            if self.widget_side == Side::Right { "right" } else { "left" },
            self.notifications,
            self.osd,
            self.night_kelvin,
            self.dock_open_apps,
            self.dock_hide.name(),
            self.panels.len(),
        );
        t.push_str(&format!(
            "comp_on={}\ncomp_gaps_in={}\ncomp_gaps_out={}\ncomp_border={}\ncomp_radius={}\ncomp_accent_borders={}\ncomp_inactive_opacity={}\ncomp_shadows={}\n",
            self.comp_on, self.comp_gaps_in, self.comp_gaps_out, self.comp_border, self.comp_radius, self.comp_accent_borders, self.comp_inactive_opacity, self.comp_shadows
        ));
        t.push_str(&format!(
            "idle_on={}\nidle_dim={}\nidle_lock={}\nidle_screen_off={}\nidle_suspend={}\nidle_media={}\nlock_on_sleep={}\nlock_cmd={}\nlock_blur={}\nlock_power={}\n",
            self.idle_on, self.idle_dim, self.idle_lock, self.idle_screen_off, self.idle_suspend, self.idle_media, self.lock_on_sleep, one(&self.lock_cmd), self.lock_blur, self.lock_power
        ));
        for (i, p) in self.panels.iter().enumerate() {
            t.push_str(&format!("panel.{i}={}\n", p.text()));
        }
        t
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
    }

    pub fn choice(&self) -> rivet_look::live::Choice {
        rivet_look::live::Choice { mode: self.theme, accent: self.accent, follow_desktop_colors: self.follow_desktop_colors, text_size: self.text_size }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let mut s = Settings { wallpaper: "/p/a b.jpg".into(), dock_hide: DockHide::Smart, widget_side: Side::Right, color_mode: Mode::Light, ws_style: WsStyle::Dots, ws_names: false, ws_hide_empty: true, ws_scroll: false, ai_providers: vec!["claude".into()], ai_bar: "codex".into(), ai_left: true, ai_pins: vec!["claude:5-hour".into(), "claude:Weekly · Fable".into()], ..Settings::default() };
        s.dock_apps = vec!["a".into(), "b.desktop".into()];
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
    fn comp_round_trips() {
        let s = Settings { comp_on: true, comp_gaps_in: 3, comp_gaps_out: 20, comp_border: 0, comp_radius: 14, comp_accent_borders: false, comp_inactive_opacity: 0.85, comp_shadows: false, ..Settings::default() };
        let mut t = Settings::default();
        t.parse(&s.text());
        assert_eq!(s, t);
    }

    #[test]
    fn idle_round_trips() {
        let s = Settings { idle_on: false, idle_dim: 0, idle_lock: 120, idle_screen_off: 900, idle_suspend: 1800, idle_media: false, lock_on_sleep: false, lock_cmd: "swaylock -f".into(), lock_blur: false, lock_power: false, ..Settings::default() };
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
