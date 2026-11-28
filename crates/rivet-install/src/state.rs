//! Each app with what's installed and what's released: what the window lists and what the tray
//! counts.

use crate::catalog::{self, Entry, Kind};
use crate::channel::Channel;
use crate::local::{self, Installed, Running};
use crate::releases::{Release, Version};

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub entry: Entry,
    pub installed: Option<Installed>,
    /// The newest release that can be installed here.
    pub latest: Option<(Release, Version)>,
    /// The channel it follows: its own, or the one for every app.
    pub channel: Channel,
    /// It has a channel of its own (`rivetctl channel`).
    pub own_channel: bool,
}

impl Row {
    /// The release to update to, when there's a newer one and it's ours to update. An edge
    /// build is replaced by any other edge build (they're told apart by commit), and by the
    /// channel's release once the channel isn't Edge any more, even though that's older.
    pub fn update(&self) -> Option<&(Release, Version)> {
        let i = self.installed.as_ref()?;
        let l = self.latest.as_ref()?;
        let have_edge = i.version.as_ref().is_some_and(|v| v.edge().is_some());
        let newer = match &i.version {
            None => true,
            Some(v) if l.1.edge().is_some() => *v < l.1 || (have_edge && *v != l.1),
            Some(v) => have_edge || *v < l.1,
        };
        (i.ours && newer).then_some(l)
    }

    /// The update goes back from an edge build to a release.
    pub fn leaves_edge(&self) -> bool {
        self.update().is_some_and(|l| l.1.edge().is_none())
            && self
                .installed
                .as_ref()
                .and_then(|i| i.version.as_ref())
                .is_some_and(|v| v.edge().is_some())
    }

    /// Can be installed (released, and not here yet).
    pub fn installable(&self) -> Option<&(Release, Version)> {
        if self.installed.is_some() {
            None
        } else {
            self.latest.as_ref()
        }
    }

    /// Rivet Welcome running without being installed: the release that would install it.
    pub fn install_copy(&self) -> Option<&(Release, Version)> {
        let i = self.installed.as_ref()?;
        (i.running != Running::No)
            .then_some(self.latest.as_ref())
            .flatten()
    }

    pub fn is_welcome(&self) -> bool {
        self.entry.kind == Kind::Welcome
    }
}

/// Every app, with what's installed (each program is asked its version, side by side), and what
/// its channel offers: its own, else `channel` (the setting).
pub fn rows(releases: &[Release], channel: Channel) -> Vec<Row> {
    rows_of(catalog::with_discovered(releases), releases, channel)
}

/// The same, held apps included (`rivetctl --held`).
pub fn rows_held(releases: &[Release], channel: Channel) -> Vec<Row> {
    rows_of(catalog::discovered(releases, true), releases, channel)
}

fn rows_of(entries: Vec<Entry>, releases: &[Release], channel: Channel) -> Vec<Row> {
    let own = crate::channel::own();
    let found: Vec<Option<Installed>> = std::thread::scope(|s| {
        let hs: Vec<_> = entries
            .iter()
            .map(|e| s.spawn(move || local::detect(e)))
            .collect();
        hs.into_iter().map(|h| h.join().ok().flatten()).collect()
    });
    entries
        .into_iter()
        .zip(found)
        .map(|(entry, installed)| {
            let mine = own.get(&entry.id).copied();
            let channel = mine.unwrap_or(channel);
            let latest = catalog::latest(&entry, releases, channel).map(|(r, v)| (r.clone(), v));
            Row {
                entry,
                installed,
                latest,
                channel,
                own_channel: mine.is_some(),
            }
        })
        .collect()
}

/// The ones with updates.
pub fn updates(rows: &[Row]) -> Vec<&Row> {
    rows.iter().filter(|r| r.update().is_some()).collect()
}

/// "Rivet Files 0.7.0", for each update.
pub fn update_names(rows: &[Row]) -> Vec<String> {
    updates(rows)
        .iter()
        .filter_map(|r| Some(format!("{} {}", r.entry.title, r.update()?.1.short())))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::parse;

    fn row(id: &str, installed: Option<(&str, bool)>) -> Row {
        let list = parse(include_str!(
            "../../../apps/rivet-welcome/data/demo-releases.json"
        ))
        .unwrap();
        let entry = catalog::known().into_iter().find(|e| e.id == id).unwrap();
        let latest = catalog::latest(&entry, &list, Channel::Stable).map(|(r, v)| (r.clone(), v));
        let installed = installed.map(|(v, ours)| Installed {
            path: "/x".into(),
            version: Version::parse(v),
            ours,
            running: Running::No,
        });
        Row {
            entry,
            installed,
            latest,
            channel: Channel::Stable,
            own_channel: false,
        }
    }

    #[test]
    fn what_needs_updating() {
        assert!(row("files", Some(("0.6.0", true))).update().is_some());
        assert!(row("files", Some(("0.7.0", true))).update().is_none());
        // Newer than released (a build from source): nothing to do.
        assert!(row("files", Some(("0.8.0", true))).update().is_none());
        // From a package: left to the package manager.
        assert!(row("files", Some(("0.6.0", false))).update().is_none());
        assert!(row("files", None).installable().is_some());
        assert!(row("shell", None).installable().is_none());
        // Rivet Welcome from an AppImage: here already, so no Install and no update to count,
        // though a copy can still be installed when asked for.
        let mut w = row("welcome", Some(("0.0.1", false)));
        w.installed.as_mut().unwrap().running = Running::AppImage;
        assert!(w.installable().is_none() && w.update().is_none());
        assert!(w.install_copy().is_some());
        assert!(
            row("files", Some(("0.6.0", false)))
                .install_copy()
                .is_none()
        );
        let rows = [
            row("files", Some(("0.6.0", true))),
            row("notes", Some(("0.1.0", true))),
            row("edit", Some(("0.1.0", true))),
        ];
        assert_eq!(
            update_names(&rows),
            ["Rivet Files 0.7.0", "Rivet Notes 0.2.0"]
        );
    }

    #[test]
    fn edge_builds_update_by_commit_and_go_back_to_releases() {
        let with = |have: &str, offered: &str| {
            let mut r = row("notes", Some((have, true)));
            r.latest.as_mut().unwrap().1 = Version::parse(offered).unwrap();
            r
        };
        let (e1, e2) = (
            "0.2.1-edge.20261126.g1111111",
            "0.2.1-edge.20261127.g2222222",
        );
        // Edge: from a release to the edge build, and from one commit to another.
        assert!(with("0.2.0", e1).update().is_some());
        assert!(with(e1, e2).update().is_some());
        assert!(with(e2, e1).update().is_some());
        assert!(with(e2, e2).update().is_none());
        // A build from source newer than the edge build is left alone.
        assert!(with("0.3.0", e1).update().is_none());
        // Back to Stable: the release replaces the edge build, though its number is lower.
        let back = with(e2, "0.2.0");
        assert!(back.update().is_some() && back.leaves_edge());
        assert!(!with("0.1.0", "0.2.0").leaves_edge());
    }
}
