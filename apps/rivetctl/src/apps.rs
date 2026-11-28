//! What every face of rivetctl does with an app (the commands, the full-screen list and the MCP
//! server): find it, install it, update it, remove it and move it to another channel, all through
//! rivet-install, so Rivet Welcome sees the same thing.

use rivet_install::catalog::{self, Entry, Kind};
use rivet_install::channel::{self, Channel};
use rivet_install::install::{self, Stage};
use rivet_install::local::{self, Source};
use rivet_install::releases::{self, Release, Version};
use rivet_install::state::{self, Row};
use serde_json::{Value, json};

/// The releases, and every app with what's installed and what's offered.
pub struct World {
    pub releases: Vec<Release>,
    pub rows: Vec<Row>,
    /// Why GitHub couldn't be asked, when the list is the cached one.
    pub offline: Option<String>,
}

/// The list from GitHub, or the last one kept when GitHub can't be reached.
pub fn releases_now() -> (Vec<Release>, Option<String>) {
    match releases::check() {
        Ok(l) => (l, None),
        Err(e) => (
            releases::load_cached().map(|(l, _)| l).unwrap_or_default(),
            Some(e),
        ),
    }
}

impl World {
    pub fn load(held: bool) -> World {
        let (releases, offline) = releases_now();
        World::from(releases, offline, held)
    }

    /// From the cached list only, without asking GitHub.
    pub fn cached(held: bool) -> World {
        let releases = releases::load_cached().map(|(l, _)| l).unwrap_or_default();
        World::from(releases, None, held)
    }

    pub fn from(releases: Vec<Release>, offline: Option<String>, held: bool) -> World {
        let rows = if held {
            state::rows_held(&releases, channel::default())
        } else {
            state::rows(&releases, channel::default())
        };
        World {
            releases,
            rows,
            offline,
        }
    }

    /// The app by any of its names: files, rivet, rivet-files, "Rivet Files".
    pub fn find(&self, name: &str, held: bool) -> Result<&Row, String> {
        find(&self.rows, name, held)
    }

    /// What each channel offers this app.
    pub fn offered(&self, e: &Entry) -> [Option<Version>; 3] {
        Channel::ALL.map(|c| catalog::latest(e, &self.releases, c).map(|(_, v)| v))
    }
}

pub fn find<'a>(rows: &'a [Row], name: &str, held: bool) -> Result<&'a Row, String> {
    let n = name.trim().to_lowercase();
    if let Some(r) = rows.iter().find(|r| matches(&r.entry, &n)) {
        return Ok(r);
    }
    // Named, but held back: say so rather than "no app called".
    if !held
        && let Some(e) = catalog::known()
            .into_iter()
            .find(|e| e.held && matches(e, &n))
    {
        return Err(format!(
            "{} is held back from release; include the held apps to use it (--held, or held: true)",
            e.title
        ));
    }
    Err(format!("no app called {name} (see rivetctl list)"))
}

fn matches(e: &Entry, n: &str) -> bool {
    e.id == n || e.bin == n || e.dist == n || e.title.to_lowercase() == n
}

/// The group an app is listed under.
pub fn category(id: &str) -> &'static str {
    match id {
        "shell" | "greeter" => "Desktop",
        "files" | "vice" | "longshot" | "reader" => "Files and documents",
        "edit" | "terminal" | "notes" => "Writing and code",
        "cinema" | "snip" | "color" => "Pictures and media",
        "ftp" | "calendar" => "Internet and time",
        "fetch" | "welcome" | "rivetctl" => "System",
        _ => "New",
    }
}

pub const CATEGORIES: [&str; 7] = [
    "Desktop",
    "Files and documents",
    "Writing and code",
    "Pictures and media",
    "Internet and time",
    "System",
    "New",
];

/// The rows in list order: by category, then as the catalog has them.
pub fn ordered(rows: &[Row]) -> Vec<&Row> {
    let mut out: Vec<&Row> = rows.iter().collect();
    out.sort_by_key(|r| CATEGORIES.iter().position(|c| *c == category(&r.entry.id)));
    out
}

/// What happened when an app was asked to be installed or updated.
#[derive(Debug, PartialEq)]
pub enum Done {
    Installed(Version),
    Already(Version),
}

/// Install the app from its channel, or update it, as Rivet Welcome's Install does. A copy
/// newer than the channel's (a build from source) is left as it is.
pub fn install(r: &Row, set: &dyn Fn(Stage)) -> Result<Done, String> {
    if let Some(i) = &r.installed
        && !i.ours
        && r.install_copy().is_none()
    {
        return Err(format!("not installed here: {}", i.left_alone()));
    }
    let Some((rel, _)) = r.update().or(r.latest.as_ref()) else {
        return Err(format!(
            "{} has nothing released on the {} channel yet",
            r.entry.title,
            r.channel.name()
        ));
    };
    let have = r.installed.as_ref().and_then(|i| i.version.as_ref());
    if r.install_copy().is_none()
        && r.update().is_none()
        && let Some(h) = have
    {
        // The same version, or a newer one from a build: nothing to do.
        return Ok(Done::Already(h.clone()));
    }
    install::run(&r.entry, rel, set).map(Done::Installed)
}

/// Take the app away: its program, launcher entries and icons. Settings and documents stay.
pub fn remove(r: &Row) -> Result<(), String> {
    match &r.installed {
        Some(i) if i.ours => local::remove(&r.entry),
        Some(i) => Err(format!("not removed: {}", i.left_alone())),
        None => Err(format!("{} isn't installed", r.entry.title)),
    }
}

/// Give the app its own channel; "default" lets it follow Rivet Welcome's again.
pub fn set_channel(r: &Row, name: &str) -> Result<Option<Channel>, String> {
    let c = match name.trim().to_lowercase().as_str() {
        "default" | "follow" | "welcome" => None,
        other => Some(Channel::parse(other).ok_or_else(|| {
            format!("{name} isn't a channel: use stable, alpha, edge or default")
        })?),
    };
    channel::set_own(&r.entry.id, c)?;
    Ok(c)
}

pub fn kind(k: Kind) -> &'static str {
    match k {
        Kind::App => "app",
        Kind::Shell => "shell",
        Kind::Welcome => "welcome",
        Kind::Command => "command",
    }
}

fn source(s: Source) -> &'static str {
    match s {
        Source::Flatpak => "flatpak",
        Source::Package => "package",
        Source::Elsewhere => "elsewhere",
    }
}

/// One app as JSON (`list --json`, the MCP server).
pub fn row_json(w: &World, r: &Row) -> Value {
    let [stable, alpha, edge] = w.offered(&r.entry).map(|v| v.map(|v| v.to_string()));
    let installed = r.installed.as_ref().map(|i| {
        json!({
            "version": i.version.as_ref().map(|v| v.to_string()),
            "path": i.path.display().to_string(),
            "managed": i.ours,
            "source": if i.ours { "rivet" } else { source(i.source()) },
        })
    });
    json!({
        "id": r.entry.id,
        "title": r.entry.title,
        "program": r.entry.bin,
        "kind": kind(r.entry.kind),
        "category": category(&r.entry.id),
        "description": r.entry.blurb,
        "held": r.entry.held,
        "installed": installed,
        "channel": r.channel.name(),
        "own_channel": r.own_channel,
        "offered": { "stable": stable, "alpha": alpha, "edge": edge },
        "update": r.update().map(|(_, v)| v.to_string()),
    })
}

/// "0.7.0", "edge 2222222", or "-".
pub fn short(v: Option<&Version>) -> String {
    v.map(Version::short).unwrap_or_else(|| "-".into())
}
