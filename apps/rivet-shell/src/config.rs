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
    // ---- notifications, on-screen display, night light
    /// Be the notification service (cards in the corner, a history in the control center).
    pub notifications: bool,
    /// Show volume and brightness changes in the middle of the screen's bottom.
    pub osd: bool,
    /// Night light's warmth, in kelvin.
    pub night_kelvin: u32,
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
            notifications: true,
            osd: true,
            night_kelvin: 4000,
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
                "notifications" => self.notifications = b,
                "osd" => self.osd = b,
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
            "theme={}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nwallpaper={}\nwallpaper_dir={}\nmatugen={}\ncolor_mode={}\nscheme_type={}\nclock_format={}\ndock={}\ndock_apps={}\ndock_icon={}\ndock_reserve={}\ndock_running={}\nwidget_clock={}\nwidget_system={}\nwidget_side={}\nnotifications={}\nosd={}\nnight_kelvin={}\ndock_open_apps={}\ndock_hide={}\npanel_count={}\n",
            mode_name(self.theme),
            self.follow_desktop_colors,
            self.text_size,
            one(&self.wallpaper),
            one(&self.wallpaper_dir),
            self.matugen,
            mode_name(self.color_mode),
            one(&self.scheme_type),
            one(&self.clock_format),
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
        let mut s = Settings { wallpaper: "/p/a b.jpg".into(), dock_hide: DockHide::Smart, widget_side: Side::Right, color_mode: Mode::Light, ..Settings::default() };
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
