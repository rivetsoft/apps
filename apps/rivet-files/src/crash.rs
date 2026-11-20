//! Crash reports, kept on this computer only (`rivet_xdg::crash` writes them, with personal
//! details taken out; `rivet_look::crash` shows one once on the next start).

pub use rivet_xdg::crash::{NO_WRITE, Report};
use rivet_xdg::crash::App;

pub const APP: App = App { name: "Rivet Files", id: "files", state: "rivet", version: env!("CARGO_PKG_VERSION"), repo: env!("CARGO_PKG_REPOSITORY") };

pub fn install() {
    rivet_xdg::crash::install(APP);
}

pub fn dir() -> std::path::PathBuf {
    rivet_xdg::crash::dir(&APP)
}

pub fn reports() -> Vec<std::path::PathBuf> {
    rivet_xdg::crash::reports(&APP)
}
