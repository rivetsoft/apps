//! Rivet FTP's preferences, kept in `~/.config/rivet-ftp/settings` as plain `key=value` lines.
//! Checks (screenshot runs) start from the defaults and never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

/// Set for screenshot runs: settings are never written.
pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// What to do when a file being sent is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnExists {
    Ask,
    Replace,
    Skip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Text size in points; 0 follows the desktop's.
    pub text_size: f32,
    pub show_menubar: bool,
    pub show_sidebar: bool,
    pub show_queue: bool,
    pub show_hidden: bool,
    pub on_exists: OnExists,
    /// Double-clicking a file sends it to the other side.
    pub double_click_sends: bool,
    pub confirm_delete: bool,
    pub clear_finished: bool,
    /// Start the local pane where it was last time (else the home folder).
    pub remember_local: bool,
    pub last_local: String,
    /// Desktop app id of the editor for server files; empty: the default app for each type.
    pub editor: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0x35, 0x84, 0xe4],
            follow_desktop_colors: true,
            text_size: 0.0,
            show_menubar: true,
            show_sidebar: true,
            show_queue: true,
            show_hidden: false,
            on_exists: OnExists::Ask,
            double_click_sends: true,
            confirm_delete: true,
            clear_finished: false,
            remember_local: true,
            last_local: String::new(),
            editor: String::new(),
        }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-ftp/settings")
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
                    if let Some(c) = parse_hex(v) {
                        s.accent = c;
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "show_menubar" => s.show_menubar = b,
                "show_sidebar" => s.show_sidebar = b,
                "show_queue" => s.show_queue = b,
                "show_hidden" => s.show_hidden = b,
                "on_exists" => s.on_exists = match v { "replace" => OnExists::Replace, "skip" => OnExists::Skip, _ => OnExists::Ask },
                "double_click_sends" => s.double_click_sends = b,
                "confirm_delete" => s.confirm_delete = b,
                "clear_finished" => s.clear_finished = b,
                "remember_local" => s.remember_local = b,
                "last_local" => s.last_local = v.to_string(),
                "editor" => s.editor = v.to_string(),
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
        let on_exists = match self.on_exists {
            OnExists::Ask => "ask",
            OnExists::Replace => "replace",
            OnExists::Skip => "skip",
        };
        let [r, g, b] = self.accent;
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nshow_menubar={}\nshow_sidebar={}\nshow_queue={}\nshow_hidden={}\non_exists={on_exists}\ndouble_click_sends={}\nconfirm_delete={}\nclear_finished={}\nremember_local={}\nlast_local={}\neditor={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.show_menubar,
            self.show_sidebar,
            self.show_queue,
            self.show_hidden,
            self.double_click_sends,
            self.confirm_delete,
            self.clear_finished,
            self.remember_local,
            self.last_local.replace('\n', ""),
            self.editor.replace('\n', ""),
        );
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#3584e4"), Some([0x35, 0x84, 0xe4]));
        assert_eq!(parse_hex("3584e4"), None);
        assert_eq!(parse_hex("#zzz"), None);
    }
}
