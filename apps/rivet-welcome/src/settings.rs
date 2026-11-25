//! Rivet Welcome's settings, kept in `~/.config/rivet-welcome/settings` as plain `key=value`
//! lines. Checks (screenshot runs) never save. The tray reads the same file, and goes away when
//! it's turned off here.

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
    /// Keep the tray icon, started when you sign in.
    pub tray: bool,
    /// Say so (a desktop notification) when there are updates.
    pub notify: bool,
    /// Look again every this many hours while signed in (0: only when the tray starts).
    pub check_hours: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            follow_desktop_colors: true,
            text_size: 0.0,
            tray: true,
            notify: true,
            check_hours: 6,
        }
    }
}

pub fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-welcome/settings")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else {
            return s;
        };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
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
                "tray" => s.tray = b,
                "notify" => s.notify = b,
                "check_hours" => s.check_hours = v.parse().unwrap_or(6),
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
            Mode::Light => "light",
            Mode::Dark => "dark",
            Mode::System => "system",
        };
        let [r, g, b] = self.accent;
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\ntray={}\nnotify={}\ncheck_hours={}\n",
            self.follow_desktop_colors, self.text_size, self.tray, self.notify, self.check_hours
        );
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let _ = std::fs::write(p, text);
    }
}

/// The tray's sign-in entry (~/.config/autostart).
fn autostart_file() -> PathBuf {
    rivet_xdg::places::config_home().join("autostart/rivet-welcome-tray.desktop")
}

/// Start the tray when you sign in, or don't.
pub fn set_autostart(on: bool) -> Result<(), String> {
    if NO_SAVE.load(Ordering::Relaxed) {
        return Ok(());
    }
    let p = autostart_file();
    if !on {
        return match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("couldn't remove {}: {e}", p.display()))
            }
            _ => Ok(()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| format!("couldn't find this program: {e}"))?;
    let text = include_str!("../data/rivet-welcome-tray.desktop")
        .replace("Exec=rivet-welcome", &format!("Exec=\"{}\"", exe.display()));
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    std::fs::write(&p, text).map_err(|e| format!("couldn't write {}: {e}", p.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_tray() {
        let s = Settings::default();
        assert!(s.tray && s.notify && s.check_hours == 6);
    }
}
