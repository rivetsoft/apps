//! Rivet's own OAuth client IDs (`oauth.rs`), from the environment or, when they aren't set
//! there, from `keys.env` at the top of the workspace (`NAME=value` lines). That file is kept out
//! of git: it's how a maintainer's or release machine's builds sign in as Rivet.

use std::path::Path;

/// What each one is called in the build, and the other names it may have in `keys.env`
/// (`clientID` and `clientSecret` alone are Google's, as its console names them).
const KEYS: [(&str, &[&str]); 3] = [
    ("RIVET_GOOGLE_CLIENT_ID", &["GOOGLE_CLIENT_ID", "clientID"]),
    ("RIVET_GOOGLE_CLIENT_SECRET", &["GOOGLE_CLIENT_SECRET", "clientSecret"]),
    ("RIVET_MICROSOFT_CLIENT_ID", &["MICROSOFT_CLIENT_ID", "AZURE_CLIENT_ID"]),
];

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    for (name, _) in KEYS {
        println!("cargo:rerun-if-env-changed={name}");
    }
    let file = Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default()).join("../../keys.env");
    let Ok(text) = std::fs::read_to_string(&file) else { return };
    // Only watched when it's there (a missing file would build this crate again every time).
    println!("cargo:rerun-if-changed={}", file.display());
    for line in text.lines().map(str::trim).filter(|l| !l.starts_with('#')) {
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim().trim_matches('"').trim_matches('\'');
        let k = k.trim();
        if let Some((name, _)) = KEYS.iter().find(|(n, also)| *n == k || also.contains(&k))
            && std::env::var_os(name).is_none()
            && !v.is_empty()
        {
            println!("cargo:rustc-env={name}={v}");
        }
    }
}
