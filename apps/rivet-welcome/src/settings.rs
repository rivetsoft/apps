//! Rivet Welcome's settings, kept in `~/.config/rivet-welcome/settings` as plain `key=value`
//! lines. Checks (screenshot runs) never save. The tray reads the same file, and goes away when
//! it's turned off here.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// Which versions to install.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Channel {
    /// Full releases of the apps that have one, and alpha versions of the apps that don't.
    #[default]
    Stable,
    /// Every release, alpha versions (GitHub pre-releases) included.
    Pre,
    /// The nightly builds of main (`rivet-notes-edge`), and the newest release of an app that
    /// has none.
    Edge,
}

impl Channel {
    fn key(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Pre => "pre",
            Channel::Edge => "edge",
        }
    }
}

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
    pub channel: Channel,
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
            channel: Channel::Stable,
        }
    }
}

pub fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-welcome/settings")
}

impl Settings {
    pub fn load() -> Settings {
        std::fs::read_to_string(path())
            .map(|t| Settings::parse(&t))
            .unwrap_or_default()
    }

    fn parse(text: &str) -> Settings {
        let mut s = Settings::default();
        let (mut channel, mut old_pre) = (None, false);
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
                "channel" => {
                    channel = match v {
                        "pre" => Some(Channel::Pre),
                        "edge" => Some(Channel::Edge),
                        "stable" => Some(Channel::Stable),
                        _ => channel,
                    }
                }
                // Rivet Welcome 0.4.0's switch, before there were channels.
                "prereleases" => old_pre = b,
                _ => {}
            }
        }
        s.channel = channel.unwrap_or(if old_pre {
            Channel::Pre
        } else {
            Channel::Stable
        });
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
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\ntray={}\nnotify={}\ncheck_hours={}\nchannel={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.tray,
            self.notify,
            self.check_hours,
            self.channel.key()
        );
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension(format!("part-{}", std::process::id()));
        if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, &p).is_err() {
            let _ = std::fs::remove_file(&tmp);
        }
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
    let exe = crate::local::this_exe().ok_or("couldn't find this program")?;
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
        assert!(s.tray && s.notify && s.check_hours == 6 && s.channel == Channel::Stable);
    }

    #[test]
    fn channels_round_trip_and_0_4_0_settings_carry_over() {
        for c in [Channel::Stable, Channel::Pre, Channel::Edge] {
            let line = format!("channel={}", c.key());
            assert_eq!(Settings::parse(&line).channel, c);
        }
        assert_eq!(Settings::parse("prereleases=true").channel, Channel::Pre);
        assert_eq!(
            Settings::parse("prereleases=false").channel,
            Channel::Stable
        );
        assert_eq!(
            Settings::parse("channel=edge\nprereleases=false").channel,
            Channel::Edge
        );
    }
}
