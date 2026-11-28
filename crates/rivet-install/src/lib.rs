//! Installing, updating and removing the Rivet apps from their GitHub releases, with no
//! interface. Rivet Welcome (a window and a tray icon) and rivetctl (a terminal) are two faces on
//! this one engine, so they read the same releases, keep the same cache and take the same lock:
//!
//! - `catalog`: the apps by name, and which release of each a channel offers.
//! - `channel`: Stable, Alpha and Edge, for every app and for one app on its own.
//! - `releases`: the list from GitHub, kept in the cache folder.
//! - `local`: what is installed here, and taking an app away.
//! - `install`: download, check, unpack and install a release, with the lock that keeps two
//!   programs off one app.
//! - `state`: each app with what is installed and what its channel offers.
//!
//! Its files are Rivet Welcome's (`~/.config/rivet-welcome`, `~/.cache/rivet-welcome`), so a
//! computer that already has Rivet Welcome is read as it is.

pub mod catalog;
pub mod channel;
pub mod install;
pub mod local;
pub mod releases;
pub mod state;

use std::sync::OnceLock;

/// The program using the engine: named in what it says, and told apart from the other one.
#[derive(Clone, Debug)]
pub struct Host {
    /// How people know it: "Rivet Welcome", "rivetctl".
    pub title: &'static str,
    /// Its version, which it reports for itself when it isn't installed.
    pub version: &'static str,
    /// Its id in the catalog ("welcome", "rivetctl"): that entry is this program, running.
    pub app: &'static str,
}

static HOST: OnceLock<Host> = OnceLock::new();

/// Say which program this is, once, before anything else.
pub fn set_host(h: Host) {
    let _ = HOST.set(h);
}

pub fn host() -> &'static Host {
    HOST.get_or_init(|| Host {
        title: "Rivet Welcome",
        version: env!("CARGO_PKG_VERSION"),
        app: "welcome",
    })
}

/// What it calls itself to GitHub: "rivet-welcome/0.4.0".
pub fn agent() -> String {
    format!(
        "{}/{}",
        host().title.to_lowercase().replace(' ', "-"),
        host().version
    )
}
