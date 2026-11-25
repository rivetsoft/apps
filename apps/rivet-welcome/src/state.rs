//! Each app with what's installed and what's released: what the window lists and what the tray
//! counts.

use crate::catalog::{self, Entry, Kind};
use crate::local::{self, Installed, Running};
use crate::releases::{Release, Version};

#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub entry: Entry,
    pub installed: Option<Installed>,
    /// The newest release that can be installed here.
    pub latest: Option<(Release, Version)>,
}

impl Row {
    /// The release to update to, when there's a newer one and it's ours to update.
    pub fn update(&self) -> Option<&(Release, Version)> {
        let i = self.installed.as_ref()?;
        let l = self.latest.as_ref()?;
        (i.ours && i.version.as_ref().is_none_or(|v| *v < l.1)).then_some(l)
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

/// Every app, with what's installed (each program is asked its version, side by side).
pub fn rows(releases: &[Release]) -> Vec<Row> {
    let entries = catalog::with_discovered(releases);
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
            let latest = catalog::latest(&entry, releases).map(|(r, v)| (r.clone(), v));
            Row {
                entry,
                installed,
                latest,
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
        .filter_map(|r| Some(format!("{} {}", r.entry.title, r.update()?.1)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::releases::parse;

    fn row(id: &str, installed: Option<(&str, bool)>) -> Row {
        let list = parse(include_str!("../data/demo-releases.json")).unwrap();
        let entry = catalog::known().into_iter().find(|e| e.id == id).unwrap();
        let latest = catalog::latest(&entry, &list).map(|(r, v)| (r.clone(), v));
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
}
