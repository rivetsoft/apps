//! What can be installed: the Rivet apps this program knows about, plus any new one that shows
//! up in the releases (a tag `NAME-vX.Y.Z` with a Linux archive for this computer), so a new
//! app is installable here the day it's released, before Rivet Welcome itself knows its name.

use crate::releases::{Asset, Release, Version};

/// What sort of thing it is (changes what "Open" does and how it's described).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A window you open.
    App,
    /// Part of the desktop, started by the compositor (Rivet Shell): "Open" opens its settings.
    Shell,
    /// This program.
    Welcome,
    /// A program for the terminal, with no window (rivetfetch): nothing to open.
    Command,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    /// A short name (files, notes…), also the tag stem for apps found in the releases.
    pub id: String,
    pub title: String,
    /// The program, in ~/.local/bin.
    pub bin: String,
    /// Archive names start with this (rivet-notes-0.2.0-x86_64-linux.tar.gz), and the desktop
    /// file and icon are named after it.
    pub dist: String,
    /// Release tags are this plus the version.
    pub tag_prefix: String,
    pub blurb: String,
    pub kind: Kind,
    /// What "Open" runs the program with.
    pub open_args: Vec<String>,
    pub icon: Option<&'static str>,
}

#[allow(clippy::too_many_arguments)]
fn entry(
    id: &str,
    title: &str,
    bin: &str,
    dist: &str,
    tag_prefix: &str,
    blurb: &str,
    kind: Kind,
    icon: &'static str,
) -> Entry {
    Entry {
        id: id.into(),
        title: title.into(),
        bin: bin.into(),
        dist: dist.into(),
        tag_prefix: tag_prefix.into(),
        blurb: blurb.into(),
        kind,
        open_args: if kind == Kind::Shell {
            vec!["settings".into()]
        } else {
            Vec::new()
        },
        icon: Some(icon),
    }
}

/// The apps Rivet Welcome knows by name, in the order they're shown.
pub fn known() -> Vec<Entry> {
    vec![
        entry(
            "files",
            "Rivet Files",
            "rivet",
            "rivet-files",
            "v",
            "A file manager: Peek previews, split view, filters, bulk rename and Quick Open.",
            Kind::App,
            include_str!("../../rivet-files/data/rivet-files.svg"),
        ),
        entry(
            "notes",
            "Rivet Notes",
            "rivet-notes",
            "rivet-notes",
            "rivet-notes-v",
            "Markdown notes in plain folders, with links, backlinks, tags, daily notes and todos.",
            Kind::App,
            include_str!("../../rivet-notes/data/rivet-notes.svg"),
        ),
        entry(
            "edit",
            "Rivet Edit",
            "rivet-edit",
            "rivet-edit",
            "rivet-edit-v",
            "A lightweight code editor with tree-sitter colors, Go to File and Commands.",
            Kind::App,
            include_str!("../../rivet-edit/data/rivet-edit.svg"),
        ),
        entry(
            "terminal",
            "Rivet Terminal",
            "rivet-terminal",
            "rivet-terminal",
            "rivet-terminal-v",
            "A terminal with tabs and split panes in layouts, in your desktop's colors.",
            Kind::App,
            include_str!("../../rivet-terminal/data/rivet-terminal.svg"),
        ),
        entry(
            "ftp",
            "Rivet FTP",
            "rivet-ftp",
            "rivet-ftp",
            "rivet-ftp-v",
            "Two panes, this computer and the server, with saved connections and a transfer queue.",
            Kind::App,
            include_str!("../../rivet-ftp/data/rivet-ftp.svg"),
        ),
        entry(
            "snip",
            "Rivet Snip",
            "rivet-snip",
            "rivet-snip",
            "rivet-snip-v",
            "Crop and mark up screenshots, with arrows, text and blur.",
            Kind::App,
            include_str!("../../rivet-snip/data/rivet-snip.svg"),
        ),
        entry(
            "color",
            "Rivet Color",
            "rivet-color",
            "rivet-color",
            "rivet-color-v",
            "Pick a color from the screen, with its shades, harmonies, contrast and palettes.",
            Kind::App,
            include_str!("../../rivet-color/data/rivet-color.svg"),
        ),
        entry(
            "vice",
            "Rivet Vice",
            "rivet-vice",
            "rivet-vice",
            "rivet-vice-v",
            "Look inside archives, take out all or some of what's in them, and make new ones.",
            Kind::App,
            include_str!("../../rivet-vice/data/rivet-vice.svg"),
        ),
        entry(
            "calendar",
            "Rivet Calendar",
            "rivet-calendar",
            "rivet-calendar",
            "rivet-calendar-v",
            "Your events by month, week or day, from plain folders, CalDAV, Google or Microsoft.",
            Kind::App,
            include_str!("../../rivet-calendar/data/rivet-calendar.svg"),
        ),
        entry(
            "fetch",
            "Rivet Fetch",
            "rivetfetch",
            "rivet-fetch",
            "rivet-fetch-v",
            "This computer at a glance in the terminal, beside Rivet the mascot.",
            Kind::Command,
            include_str!("../../rivet-fetch/data/rivet-fetch.svg"),
        ),
        entry(
            "shell",
            "Rivet Shell",
            "rivet-shell",
            "rivet-shell",
            "rivet-shell-v",
            "A panel, dock, launcher, widgets and wallpaper for niri, Hyprland, Sway and other Wayland desktops.",
            Kind::Shell,
            include_str!("../../rivet-shell/data/rivet-shell.svg"),
        ),
        entry(
            "welcome",
            "Rivet Welcome",
            "rivet-welcome",
            "rivet-welcome",
            "rivet-welcome-v",
            "This app: installs and updates the others, and tells you when there's something new.",
            Kind::Welcome,
            include_str!("../data/rivet-welcome.svg"),
        ),
    ]
}

/// This release's version, when it's one of this entry's.
pub fn tag_version(prefix: &str, tag: &str) -> Option<Version> {
    let rest = tag.strip_prefix(prefix)?;
    // `v` (Rivet Files) mustn't take `rivet-notes-v…`: the rest has to start with a digit.
    rest.starts_with(|c: char| c.is_ascii_digit())
        .then(|| Version::parse(rest))
        .flatten()
}

/// The computer's architecture as release archives name it (`uname -m`).
pub fn arch() -> &'static str {
    std::env::consts::ARCH
}

/// The archive for this computer in a release, and its checksum file when there is one.
pub fn archive<'a>(e: &Entry, r: &'a Release) -> Option<(&'a Asset, Option<&'a Asset>)> {
    let tail = format!("-{}-linux.tar.gz", arch());
    let a = r
        .assets
        .iter()
        .find(|a| a.name.starts_with(&format!("{}-", e.dist)) && a.name.ends_with(&tail))
        .or_else(|| r.assets.iter().find(|a| a.name.ends_with(&tail)))?;
    let sum = r
        .assets
        .iter()
        .find(|s| s.name == format!("{}.sha256", a.name));
    Some((a, sum))
}

/// The newest release of this entry that can be installed here.
pub fn latest<'a>(e: &Entry, releases: &'a [Release]) -> Option<(&'a Release, Version)> {
    releases
        .iter()
        .filter_map(|r| Some((r, tag_version(&e.tag_prefix, &r.tag)?)))
        .filter(|(r, _)| archive(e, r).is_some())
        .max_by(|a, b| a.1.cmp(&b.1))
}

/// "Rivet Clock" from "rivet-clock" (when its release has no better name).
fn title_from(stem: &str) -> String {
    stem.split('-')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// The known apps, then any others the releases have (newest first, once each).
pub fn with_discovered(releases: &[Release]) -> Vec<Entry> {
    let mut out = known();
    for r in releases {
        let Some(i) = r.tag.rfind("-v") else { continue };
        let (stem, prefix) = (&r.tag[..i], &r.tag[..i + 2]);
        let Some(v) = tag_version(prefix, &r.tag) else {
            continue;
        };
        if stem.is_empty() || out.iter().any(|e| e.tag_prefix == prefix) {
            continue;
        }
        // The release's name without its version: "Rivet Clock 0.1.0" → "Rivet Clock".
        let title = r
            .name
            .trim()
            .strip_suffix(&v.to_string())
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .map(String::from)
            .unwrap_or_else(|| title_from(stem));
        let e = Entry {
            id: stem.into(),
            title,
            bin: stem.into(),
            dist: stem.into(),
            tag_prefix: prefix.into(),
            blurb: "New in the Rivet apps.".into(),
            kind: Kind::App,
            open_args: Vec::new(),
            icon: None,
        };
        if archive(&e, r).is_some() {
            out.push(e);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::{Asset, parse};

    #[test]
    fn tags_belong_to_their_app() {
        assert_eq!(tag_version("v", "v0.7.0"), Version::parse("0.7.0"));
        assert_eq!(tag_version("v", "rivet-notes-v0.2.0"), None);
        assert_eq!(
            tag_version("rivet-notes-v", "rivet-notes-v0.2.0"),
            Version::parse("0.2.0")
        );
        assert_eq!(tag_version("rivet-notes-v", "rivet-notes-vx"), None);
    }

    #[test]
    fn latest_is_the_newest_installable() {
        let list = parse(include_str!("../data/demo-releases.json")).unwrap();
        let k = known();
        let files = k.iter().find(|e| e.id == "files").unwrap();
        let (r, v) = latest(files, &list).unwrap();
        assert_eq!(
            (r.tag.as_str(), v.to_string()),
            ("v0.7.0", "0.7.0".to_string())
        );
        let shell = k.iter().find(|e| e.id == "shell").unwrap();
        assert!(latest(shell, &list).is_none());
    }

    /// Every app utilities/rivet-dev releases is listed here by name, under the tag, archive and
    /// program names the release scripts give it (utilities/lib/apps.sh).
    #[test]
    fn every_released_app_is_known() {
        let script = include_str!("../../../utilities/lib/apps.sh");
        let line = script
            .lines()
            .find_map(|l| l.strip_prefix("ALL_APPS="))
            .expect("ALL_APPS in apps.sh");
        let k = known();
        for id in line.trim_matches('"').split_whitespace() {
            let e = k
                .iter()
                .find(|e| e.id == id)
                .unwrap_or_else(|| panic!("{id} is released but Rivet Welcome doesn't list it"));
            let (prefix, bin) = match id {
                "files" => ("v".to_string(), "rivet".to_string()),
                "fetch" => ("rivet-fetch-v".to_string(), "rivetfetch".to_string()),
                _ => (format!("rivet-{id}-v"), format!("rivet-{id}")),
            };
            assert_eq!(
                (e.tag_prefix.as_str(), e.bin.as_str(), e.dist.as_str()),
                (
                    prefix.as_str(),
                    bin.as_str(),
                    format!("rivet-{id}").as_str()
                ),
                "{id}"
            );
        }
        assert_eq!(k.len(), line.trim_matches('"').split_whitespace().count());
    }

    #[test]
    fn new_apps_are_found() {
        let mut list = parse(include_str!("../data/demo-releases.json")).unwrap();
        let mut r = list[1].clone();
        r.tag = "rivet-clock-v0.1.0".into();
        r.name = "Rivet Clock 0.1.0".into();
        r.assets = vec![Asset {
            name: format!("rivet-clock-0.1.0-{}-linux.tar.gz", arch()),
            url: "u".into(),
            size: 1,
        }];
        list.push(r.clone());
        // Without an archive for this computer it isn't offered.
        let mut other = r.clone();
        other.tag = "rivet-paint-v0.1.0".into();
        other.assets = vec![Asset {
            name: "rivet-paint-0.1.0-sparc-linux.tar.gz".into(),
            url: "u".into(),
            size: 1,
        }];
        list.push(other);
        let all = with_discovered(&list);
        let clock = all.iter().find(|e| e.id == "rivet-clock").unwrap();
        assert_eq!(
            (clock.title.as_str(), clock.bin.as_str()),
            ("Rivet Clock", "rivet-clock")
        );
        assert!(all.iter().all(|e| e.id != "rivet-paint"));
        assert_eq!(all.iter().filter(|e| e.id == "notes").count(), 1);
        assert_eq!(title_from("rivet-big-clock"), "Rivet Big Clock");
    }
}
