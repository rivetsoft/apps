//! Which versions to install. Every app follows Rivet Welcome's channel (its `channel` setting),
//! unless it has been given one of its own (`rivetctl channel notes edge`), kept in
//! `~/.config/rivet-welcome/channels` as `notes=edge` lines. Rivet Welcome 0.4 and older don't
//! read that file, and an app with no line follows the setting as before.

use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
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
    pub const ALL: [Channel; 3] = [Channel::Stable, Channel::Pre, Channel::Edge];

    /// As it is written in the files.
    pub fn key(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Pre => "pre",
            Channel::Edge => "edge",
        }
    }

    /// As people read it, and type it: "stable", "alpha", "edge".
    pub fn name(self) -> &'static str {
        match self {
            Channel::Stable => "stable",
            Channel::Pre => "alpha",
            Channel::Edge => "edge",
        }
    }

    /// From the files or from someone typing it ("alpha" and "pre" are the same).
    pub fn parse(s: &str) -> Option<Channel> {
        match s.trim().to_lowercase().as_str() {
            "stable" => Some(Channel::Stable),
            "pre" | "alpha" | "prerelease" => Some(Channel::Pre),
            "edge" => Some(Channel::Edge),
            _ => None,
        }
    }
}

/// Rivet Welcome's settings, which hold the channel for every app.
pub fn settings_path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-welcome/settings")
}

/// The channel in Rivet Welcome's settings text. Rivet Welcome 0.4.0 had a switch,
/// `prereleases`, before there were channels, and it still counts when there's no `channel`.
pub fn from_settings(text: &str) -> Channel {
    let (mut channel, mut old_pre) = (None, false);
    for line in text.lines() {
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim();
        match k.trim() {
            "channel" => {
                channel = match v {
                    "pre" => Some(Channel::Pre),
                    "edge" => Some(Channel::Edge),
                    "stable" => Some(Channel::Stable),
                    _ => channel,
                }
            }
            "prereleases" => old_pre = v == "true",
            _ => {}
        }
    }
    channel.unwrap_or(if old_pre {
        Channel::Pre
    } else {
        Channel::Stable
    })
}

/// The channel every app follows unless it has its own.
pub fn default() -> Channel {
    std::fs::read_to_string(settings_path())
        .map(|t| from_settings(&t))
        .unwrap_or_default()
}

pub fn own_path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-welcome/channels")
}

fn parse_own(text: &str) -> HashMap<String, Channel> {
    text.lines()
        .filter_map(|l| l.split_once('='))
        .filter_map(|(k, v)| Some((k.trim().to_string(), Channel::parse(v)?)))
        .filter(|(k, _)| !k.is_empty())
        .collect()
}

/// The apps that have a channel of their own. Checks (`RIVET_WELCOME_INSTALLED`) have none, so
/// a screenshot doesn't depend on this computer.
pub fn own() -> HashMap<String, Channel> {
    if std::env::var_os("RIVET_WELCOME_INSTALLED").is_some() {
        return HashMap::new();
    }
    std::fs::read_to_string(own_path())
        .map(|t| parse_own(&t))
        .unwrap_or_default()
}

/// Give an app its own channel, or (None) let it follow the one for every app again.
pub fn set_own(id: &str, c: Option<Channel>) -> Result<(), String> {
    let mut all = own();
    match c {
        Some(c) => all.insert(id.to_string(), c),
        None => all.remove(id),
    };
    let mut ids: Vec<&String> = all.keys().collect();
    ids.sort();
    let text: String = ids
        .into_iter()
        .map(|k| format!("{k}={}\n", all[k].key()))
        .collect();
    let p = own_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("couldn't make {}: {e}", d.display()))?;
    }
    let tmp = p.with_extension(format!("part-{}", std::process::id()));
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, &p))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("couldn't write {}: {e}", p.display())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip_and_0_4_0_settings_carry_over() {
        for c in Channel::ALL {
            assert_eq!(from_settings(&format!("channel={}", c.key())), c);
            assert_eq!(Channel::parse(c.key()), Some(c));
            assert_eq!(Channel::parse(c.name()), Some(c));
        }
        assert_eq!(from_settings("prereleases=true"), Channel::Pre);
        assert_eq!(from_settings("prereleases=false"), Channel::Stable);
        assert_eq!(
            from_settings("channel=edge\nprereleases=false"),
            Channel::Edge
        );
        assert_eq!(from_settings("theme=dark"), Channel::Stable);
        assert_eq!(Channel::parse("latest"), None);
    }

    #[test]
    fn own_channels_skip_what_they_dont_know() {
        let own = parse_own("notes=edge\nfiles=alpha\nedit=nightly\n=stable\njunk\n");
        assert_eq!(own.len(), 2);
        assert_eq!(own["notes"], Channel::Edge);
        assert_eq!(own["files"], Channel::Pre);
    }
}
