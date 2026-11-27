//! Rivet Reader' settings: ~/.config/rivet-reader/settings, key=value lines a person can edit, with
//! unknown keys skipped so a newer file still loads.

use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DarkPages {
    /// Dark pages when the desktop is dark.
    Auto,
    On,
    Off,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Pages,
    Outline,
    Hidden,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub continuous: bool,
    pub dark_pages: DarkPages,
    pub sidebar: Side,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { continuous: true, dark_pages: DarkPages::Auto, sidebar: Side::Pages }
    }
}

pub fn config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
    base.join("rivet-reader")
}

impl Settings {
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            match (k.trim(), v.trim()) {
                ("view", "single") => s.continuous = false,
                ("view", "continuous") => s.continuous = true,
                ("dark_pages", "on") => s.dark_pages = DarkPages::On,
                ("dark_pages", "off") => s.dark_pages = DarkPages::Off,
                ("dark_pages", "auto") => s.dark_pages = DarkPages::Auto,
                ("sidebar", "pages") => s.sidebar = Side::Pages,
                ("sidebar", "outline") => s.sidebar = Side::Outline,
                ("sidebar", "hidden") => s.sidebar = Side::Hidden,
                _ => {}
            }
        }
        s
    }

    pub fn text(&self) -> String {
        let dark = match self.dark_pages {
            DarkPages::Auto => "auto",
            DarkPages::On => "on",
            DarkPages::Off => "off",
        };
        let side = match self.sidebar {
            Side::Pages => "pages",
            Side::Outline => "outline",
            Side::Hidden => "hidden",
        };
        format!("view={}\ndark_pages={dark}\nsidebar={side}\n", if self.continuous { "continuous" } else { "single" })
    }

    pub fn load() -> Settings {
        std::fs::read_to_string(config_dir().join("settings")).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn save(&self) {
        let dir = config_dir();
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(dir.join("settings"), self.text());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let s = Settings { continuous: false, dark_pages: DarkPages::On, sidebar: Side::Outline };
        assert_eq!(Settings::parse(&s.text()), s);
        assert!(!Settings::parse("view=single\nsomething_new=1\n").continuous);
        assert_eq!(Settings::parse(""), Settings::default());
    }
}
