//! Rivet Terminal's settings, kept in `~/.config/rivet-terminal/settings` as plain `key=value`
//! lines. Checks (screenshot runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;
use rivet_term::{CursorStyle, MAX_SCROLLBACK};

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Interface text size in points; 0 follows the desktop's.
    pub text_size: f32,
    pub builtin_icons: bool,
    pub show_menubar: bool,
    /// The terminal's font (a monospace family; empty: the system's) and size.
    pub font: String,
    pub font_size: f32,
    /// A colour scheme by name (empty: follow the app's colours).
    pub scheme: String,
    pub cursor: CursorStyle,
    pub bold_is_bright: bool,
    /// Room around the text, in points.
    pub padding: f32,
    /// Lines of scrollback.
    pub scrollback: usize,
    /// The program to start (empty: your login shell).
    pub shell: String,
    pub copy_on_select: bool,
    pub visual_bell: bool,
    /// Ask before closing a tab or window where something is still running.
    pub confirm_close: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            follow_desktop_colors: true,
            text_size: 0.0,
            builtin_icons: true,
            show_menubar: false,
            font: String::new(),
            font_size: 13.0,
            scheme: String::new(),
            cursor: CursorStyle::Block,
            bold_is_bright: true,
            padding: 10.0,
            scrollback: 10_000,
            shell: String::new(),
            copy_on_select: false,
            visual_bell: true,
            confirm_close: true,
        }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-terminal/settings")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            match k.trim() {
                "theme" => s.theme = match v { "light" => Mode::Light, "dark" => Mode::Dark, _ => Mode::System },
                "accent" => {
                    if let Some(h) = v.strip_prefix('#').filter(|h| h.len() == 6)
                        && let Ok(n) = u32::from_str_radix(h, 16)
                    {
                        s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "builtin_icons" => s.builtin_icons = b,
                "show_menubar" => s.show_menubar = b,
                "font" => s.font = v.to_string(),
                "font_size" => s.font_size = v.parse::<f32>().unwrap_or(13.0).clamp(6.0, 48.0),
                "scheme" => s.scheme = v.to_string(),
                "cursor" => s.cursor = CursorStyle::from_name(v),
                "bold_is_bright" => s.bold_is_bright = b,
                "padding" => s.padding = v.parse::<f32>().unwrap_or(10.0).clamp(0.0, 40.0),
                "scrollback" => s.scrollback = v.parse::<usize>().unwrap_or(10_000).clamp(100, MAX_SCROLLBACK),
                "shell" => s.shell = v.to_string(),
                "copy_on_select" => s.copy_on_select = b,
                "visual_bell" => s.visual_bell = b,
                "confirm_close" => s.confirm_close = b,
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        if NO_SAVE.load(Ordering::Relaxed) {
            return;
        }
        let theme = match self.theme {
            Mode::System => "system",
            Mode::Light => "light",
            Mode::Dark => "dark",
        };
        let [r, g, b] = self.accent;
        let one = |s: &str| s.replace('\n', " ");
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nbuiltin_icons={}\nshow_menubar={}\nfont={}\nfont_size={}\nscheme={}\ncursor={}\nbold_is_bright={}\npadding={}\nscrollback={}\nshell={}\ncopy_on_select={}\nvisual_bell={}\nconfirm_close={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.builtin_icons,
            self.show_menubar,
            one(&self.font),
            self.font_size,
            one(&self.scheme),
            self.cursor.name(),
            self.bold_is_bright,
            self.padding,
            self.scrollback,
            one(&self.shell),
            self.copy_on_select,
            self.visual_bell,
            self.confirm_close,
        );
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_sane() {
        let s = Settings::default();
        assert_eq!(s.cursor, CursorStyle::Block);
        assert!(s.scrollback >= 1000 && s.font_size > 8.0);
    }
}
