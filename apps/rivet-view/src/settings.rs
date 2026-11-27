//! Rivet View's settings, kept in `~/.config/rivet-view/settings` as plain `key=value` lines.
//! Keys this version doesn't know are skipped, so a newer file still loads. Checks (screenshot
//! runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Interface text size in points; 0 follows the desktop's.
    pub text_size: f32,
    /// The details panel is open.
    pub details: bool,
    /// The strip of thumbnails along the bottom is shown.
    pub strip: bool,
    /// Seconds each picture stays up in a slideshow.
    pub slideshow_seconds: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Mode::System, accent: [0x00, 0x84, 0xb8], follow_desktop_colors: true, text_size: 0.0, details: false, strip: true, slideshow_seconds: 5 }
    }
}

pub fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-view/settings")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(path()).map(|t| Settings::parse(&t)).unwrap_or_default()
    }

    pub fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            match k.trim() {
                "theme" => {
                    s.theme = match v {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        _ => Mode::System,
                    }
                }
                "accent" => {
                    if let Some(h) = v.strip_prefix('#').filter(|h| h.len() == 6)
                        && let Ok(n) = u32::from_str_radix(h, 16)
                    {
                        s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse().unwrap_or(0.0),
                "details" => s.details = b,
                "strip" => s.strip = b,
                "slideshow_seconds" => s.slideshow_seconds = v.parse().unwrap_or(5).clamp(1, 600),
                _ => {}
            }
        }
        s
    }

    pub fn text(&self) -> String {
        let theme = match self.theme {
            Mode::Light => "light",
            Mode::Dark => "dark",
            Mode::System => "system",
        };
        let [r, g, b] = self.accent;
        format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\ndetails={}\nstrip={}\nslideshow_seconds={}\n",
            self.follow_desktop_colors, self.text_size, self.details, self.strip, self.slideshow_seconds
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_round_trip() {
        let s = Settings { theme: Mode::Dark, accent: [1, 2, 3], follow_desktop_colors: false, text_size: 14.0, details: true, strip: false, slideshow_seconds: 9 };
        assert_eq!(Settings::parse(&s.text()), s);
        assert_eq!(Settings::parse("unknown=1\n"), Settings::default());
    }
}
