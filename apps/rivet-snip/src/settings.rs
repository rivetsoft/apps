//! Rivet Snip's settings, kept in `~/.config/rivet-snip/settings` as plain `key=value` lines.
//! Checks (screenshot runs) never save.

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
    /// Where Save puts a picture that didn't come from a file (empty: Screenshots in Pictures).
    pub save_dir: String,
    /// Close the window once the picture is on the clipboard.
    pub close_after_copy: bool,
    /// The colour and thickness last drawn with, by their place in the lists.
    pub color: usize,
    pub width: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { theme: Mode::System, accent: [0xe6, 0x61, 0x00], follow_desktop_colors: true, text_size: 0.0, save_dir: String::new(), close_after_copy: false, color: 0, width: 1 }
    }
}

pub fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-snip/settings")
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
                "save_dir" => s.save_dir = v.to_string(),
                "close_after_copy" => s.close_after_copy = b,
                "color" => s.color = v.parse().unwrap_or(0),
                "width" => s.width = v.parse().unwrap_or(1),
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
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nsave_dir={}\nclose_after_copy={}\ncolor={}\nwidth={}\n",
            self.follow_desktop_colors, self.text_size, self.save_dir, self.close_after_copy, self.color, self.width
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
    fn settings_survive_the_file() {
        let s = Settings { theme: Mode::Dark, accent: [1, 2, 3], follow_desktop_colors: false, text_size: 14.0, save_dir: "~/Pictures/Shots".into(), close_after_copy: true, color: 4, width: 2 };
        assert_eq!(Settings::parse(&s.text()), s);
        // A line this version doesn't know is skipped, and the rest still loads.
        assert_eq!(Settings::parse("later_thing=1\nwidth=2\n").width, 2);
    }
}
