//! Rivet Calendar's settings, in `~/.config/rivet-calendar/settings` as plain `key=value` lines.
//! Unknown keys are skipped, so a newer file still loads. Checks (screenshot runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// How the days are shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Day,
    Week,
    Month,
    Agenda,
}

impl View {
    pub fn name(self) -> &'static str {
        match self {
            View::Day => "day",
            View::Week => "week",
            View::Month => "month",
            View::Agenda => "agenda",
        }
    }

    pub fn from(s: &str) -> View {
        match s {
            "day" => View::Day,
            "week" => View::Week,
            "agenda" => View::Agenda,
            _ => View::Month,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    pub text_size: f32,
    pub view: View,
    pub show_sidebar: bool,
    /// Weeks start on Monday (else Sunday).
    pub monday_first: bool,
    /// Times as 14:00 (else 2 PM).
    pub clock_24: bool,
    /// The calendars not shown, by folder name.
    pub hidden: Vec<String>,
    /// Where the calendars are (empty: Rivet's own folder).
    pub folder: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Mode::Dark, accent: [0xe0, 0x57, 0x3f], follow_desktop_colors: true, text_size: 0.0, view: View::Month, show_sidebar: true, monday_first: true, clock_24: true, hidden: Vec::new(), folder: String::new() }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-calendar/settings")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(path()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    /// Settings from the file's text.
    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            let b = v == "true";
            match k {
                "theme" => s.theme = if v == "light" { Mode::Light } else { Mode::Dark },
                "accent" => {
                    if let Some(c) = rivet_cal::store::parse_color(v) {
                        s.accent = c;
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse().unwrap_or(0.0),
                "view" => s.view = View::from(v),
                "show_sidebar" => s.show_sidebar = b,
                "monday_first" => s.monday_first = b,
                "clock_24" => s.clock_24 = b,
                "hidden" => s.hidden = v.split(',').map(str::trim).filter(|x| !x.is_empty()).map(String::from).collect(),
                "folder" => s.folder = v.to_string(),
                _ => {}
            }
        }
        s
    }

    pub fn text(&self) -> String {
        let [r, g, b] = self.accent;
        format!(
            "theme={}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nview={}\nshow_sidebar={}\nmonday_first={}\nclock_24={}\nhidden={}\nfolder={}\n",
            if self.theme == Mode::Light { "light" } else { "dark" },
            self.follow_desktop_colors,
            self.text_size,
            self.view.name(),
            self.show_sidebar,
            self.monday_first,
            self.clock_24,
            self.hidden.join(","),
            self.folder
        )
    }

    pub fn save(&self) {
        if NO_SAVE.load(Ordering::Relaxed) {
            return;
        }
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, self.text());
    }

    /// The folder the calendars are in.
    pub fn root(&self) -> PathBuf {
        match self.folder.strip_prefix("~/") {
            Some(rest) => rivet_xdg::places::home().join(rest),
            None if !self.folder.is_empty() => PathBuf::from(&self.folder),
            None => rivet_cal::default_root(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings { theme: Mode::Light, accent: [1, 2, 3], follow_desktop_colors: false, text_size: 15.0, view: View::Week, show_sidebar: false, monday_first: false, clock_24: false, hidden: vec!["work".into(), "holidays".into()], folder: "~/Calendars".into() };
        assert_eq!(Settings::parse(&s.text()), s);
        assert_eq!(Settings::parse("view=nonsense\nunknown=1\n"), Settings::default());
    }
}
