//! Plugins from other people: the list anyone can add theirs to (`plugins/registry.json` in
//! Rivet's repository, which the website shows too), and putting one in the person's plugins
//! folder or taking it out again. Everything here downloads or touches the disk, so it's called
//! from a thread or the command line, never from a frame.
//!
//! A listing names one archive and its SHA-256, so what's installed is the very file that was
//! looked at when the listing was accepted; a new version is a new line in the list. Installing
//! never turns a plugin on: that stays the person's choice in Settings.

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

use crate::plugins::{Manifest, found, user_dir};

/// The list, as published. `RIVET_SHELL_REGISTRY=FILE-or-URL` reads another (checks, and trying
/// a listing before proposing it).
const REGISTRY: &str = "https://raw.githubusercontent.com/ulinaaron/rivet-apps/main/plugins/registry.json";

/// The biggest download taken: a plugin is a script or a small program.
const MAX_BYTES: &str = "33554432";

/// One plugin in the list.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Listing {
    /// The folder it's installed as, and what `plugin:ID` names.
    pub id: String,
    pub name: String,
    pub description: String,
    pub author: String,
    /// Where its source is, for anyone who wants to read it first.
    pub homepage: String,
    pub version: String,
    /// A `.tar.gz` with the plugin's folder (or its files) in it.
    pub download: String,
    pub sha256: String,
}

/// An id is a folder name and part of a setting's key: small letters, digits, `-` and `_`.
pub fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 40 && id.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_') && id.starts_with(|c: char| c.is_ascii_alphanumeric())
}

/// The list from its JSON. A listing that can't be installed safely (no checksum, a download
/// that isn't https, an id that isn't a folder name) is left out rather than failing the rest.
///
/// `local` is for a list read from a file on this computer, which may also name archives here
/// (`file://`): how an author tries a listing before proposing it.
pub fn parse(text: &str, local: bool) -> Result<Vec<Listing>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|_| "the plugin list isn't readable just now".to_string())?;
    let rows = v.get("plugins").and_then(|p| p.as_array()).ok_or("the plugin list isn't readable just now")?;
    let mut out: Vec<Listing> = Vec::new();
    for r in rows {
        let f = |k: &str| r.get(k).and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        let l = Listing { id: f("id"), name: f("name"), description: f("description"), author: f("author"), homepage: f("homepage"), version: f("version"), download: f("download"), sha256: f("sha256").to_lowercase() };
        let sum_ok = l.sha256.len() == 64 && l.sha256.chars().all(|c| c.is_ascii_hexdigit());
        let from_ok = l.download.starts_with("https://") || (local && l.download.starts_with("file://"));
        if valid_id(&l.id) && !l.name.is_empty() && from_ok && sum_ok && !out.iter().any(|o| o.id == l.id) {
            out.push(Listing { homepage: Some(l.homepage.clone()).filter(|h| h.starts_with("https://")).unwrap_or_default(), ..l });
        }
    }
    out.sort_by_key(|l| l.name.to_lowercase());
    Ok(out)
}

/// The list, from the network (or the file or address in `RIVET_SHELL_REGISTRY`).
pub fn fetch() -> Result<Vec<Listing>, String> {
    let from = std::env::var("RIVET_SHELL_REGISTRY").ok().filter(|s| !s.is_empty());
    let local = from.as_deref().is_some_and(|f| !f.contains("://"));
    let text = match from.as_deref() {
        Some(f) if local => std::fs::read_to_string(f).map_err(|e| format!("couldn't read {f}: {e}"))?,
        other => {
            let out = curl().args(["--max-time", "20", other.unwrap_or(REGISTRY)]).output().map_err(no_curl)?;
            if !out.status.success() {
                return Err("Couldn't get the plugin list. Check your connection and try again.".into());
            }
            String::from_utf8_lossy(&out.stdout).into_owned()
        }
    };
    parse(&text, local)
}

fn curl() -> Command {
    let mut c = Command::new("curl");
    c.args(["-fsSL", "--retry", "2", "--connect-timeout", "15", "--max-filesize", MAX_BYTES, "-A", concat!("rivet-shell/", env!("CARGO_PKG_VERSION"))]).stdin(Stdio::null()).stderr(Stdio::null());
    c
}

fn no_curl(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound { "Install curl to download plugins.".into() } else { format!("couldn't run curl: {e}") }
}

/// Is this plugin one the person installed (in their folder), so it can be removed or updated?
pub fn removable(m: &Manifest) -> bool {
    !m.official && m.dir.parent() == Some(user_dir().as_path())
}

/// A folder to work in, inside the plugins folder so the finished plugin moves into place in
/// one step. Its name starts with a dot and the unpacked files go a level down, so a plugin
/// half unpacked is never taken for one that's installed.
struct Staging(PathBuf);

impl Staging {
    fn new() -> Result<Staging, String> {
        let dir = user_dir().join(format!(".installing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("unpacked")).map_err(|e| format!("couldn't make {}: {e}", dir.display()))?;
        Ok(Staging(dir))
    }
}

impl Drop for Staging {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Install a plugin from the list: its archive is downloaded and must match the listing's
/// checksum. The plugin as it is once installed (off until it's turned on).
pub fn install(l: &Listing) -> Result<Manifest, String> {
    let stage = Staging::new()?;
    let file = stage.0.join("download.tar.gz");
    download(&l.download, &file)?;
    if sha256(&file)? != l.sha256 {
        return Err("The download doesn't match the plugin list, so nothing was installed. Try again later.".into());
    }
    place(&stage, &file, Some(&l.id))
}

/// Install from an archive on this computer, a plugin's folder, or an address: for a plugin
/// that isn't in the list (nothing is checked against a checksum, since there is none).
pub fn install_from(source: &str) -> Result<Manifest, String> {
    let stage = Staging::new()?;
    if source.starts_with("https://") || source.starts_with("http://") {
        let file = stage.0.join("download.tar.gz");
        download(source, &file)?;
        return place(&stage, &file, None);
    }
    let path = PathBuf::from(source);
    if path.is_dir() {
        // A folder: copied as it is, under its own name.
        let id = path.canonicalize().ok().and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).unwrap_or_default();
        let to = stage.0.join("unpacked").join(&id);
        copy_dir(&path, &to)?;
        return finish(&stage, &to, &id);
    }
    if !path.is_file() {
        return Err(format!("{source} isn't a file, a folder or an address"));
    }
    place(&stage, &path, None)
}

fn download(url: &str, to: &Path) -> Result<(), String> {
    let status = curl().arg("--max-time").arg("120").arg("-o").arg(to).arg(url).stdout(Stdio::null()).status().map_err(no_curl)?;
    if status.success() { Ok(()) } else { Err("The download failed. Check your connection and try again.".into()) }
}

fn sha256(file: &Path) -> Result<String, String> {
    let out = Command::new("sha256sum").arg(file).stdin(Stdio::null()).output().map_err(|e| format!("couldn't run sha256sum: {e}"))?;
    Ok(String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or("").to_lowercase())
}

/// A path inside an archive that stays inside the folder it's unpacked in.
fn stays_inside(p: &Path) -> bool {
    p.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir))
}

/// Unpack an archive in the staging folder and install the plugin in it. `id` is the listing's;
/// without one it's the name of the folder in the archive, or of the archive.
fn place(stage: &Staging, archive: &Path, id: Option<&str>) -> Result<Manifest, String> {
    let bad = || "That file isn't a plugin: it should be a .tar.gz with a plugin.conf in it.".to_string();
    // Its names are read first: nothing is unpacked from an archive that reaches outside.
    let names = Command::new("tar").arg("-tzf").arg(archive).stdin(Stdio::null()).stderr(Stdio::null()).output().map_err(|e| format!("couldn't run tar: {e}"))?;
    let names = String::from_utf8_lossy(&names.stdout).into_owned();
    if names.trim().is_empty() || !names.lines().all(|n| stays_inside(Path::new(n))) {
        return Err(bad());
    }
    let into = stage.0.join("unpacked");
    let ok = Command::new("tar").arg("-xzf").arg(archive).arg("-C").arg(&into).args(["--no-same-owner", "--no-same-permissions"]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    if !ok {
        return Err(bad());
    }
    // The plugin: the archive's own top, or the one folder in it.
    let (root, name) = if into.join("plugin.conf").is_file() {
        let stem = archive.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        (into.clone(), stem.split('.').next().unwrap_or("").to_string())
    } else {
        let inner = std::fs::read_dir(&into).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join("plugin.conf").is_file()).ok_or_else(bad)?;
        let name = inner.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
        (inner, name)
    };
    finish(stage, &root, id.unwrap_or(&name))
}

/// Check the unpacked plugin and move it into the plugins folder as `id`, in place of the one
/// that's there.
fn finish(stage: &Staging, root: &Path, id: &str) -> Result<Manifest, String> {
    if !valid_id(id) {
        return Err(format!("\"{id}\" can't be a plugin's folder name: use small letters, digits and dashes."));
    }
    check_tree(root)?;
    let text = std::fs::read_to_string(root.join("plugin.conf")).map_err(|_| "That folder has no plugin.conf in it.".to_string())?;
    if Manifest::parse(id, root, false, &text).is_none() {
        return Err("Its plugin.conf doesn't say which program to run, so it can't be installed.".into());
    }
    // One of the shell's own would hide it: the shell's folders are looked in first.
    if let Some(own) = found().into_iter().find(|m| m.id == id && !removable(m) && m.dir != root) {
        return Err(format!("{} is already here under that name ({}), so this one would never run.", own.name, own.dir.display()));
    }
    let to = user_dir().join(id);
    if to.symlink_metadata().is_ok() {
        std::fs::rename(&to, stage.0.join("before")).map_err(|e| format!("couldn't replace {}: {e}", to.display()))?;
    }
    std::fs::rename(root, &to).map_err(|e| format!("couldn't put the plugin in {}: {e}", to.display()))?;
    Manifest::parse(id, &to, false, &text).ok_or_else(|| "the plugin didn't install".to_string())
}

/// Nothing in a plugin's folder may point outside it: links are followed when it runs.
fn check_tree(dir: &Path) -> Result<(), String> {
    for e in std::fs::read_dir(dir).map_err(|e| e.to_string())?.flatten() {
        let (path, kind) = (e.path(), e.file_type().map_err(|e| e.to_string())?);
        if kind.is_symlink() {
            let target = std::fs::read_link(&path).map_err(|e| e.to_string())?;
            if !target.components().all(|c| matches!(c, Component::Normal(_) | Component::CurDir)) {
                return Err("That plugin has a link to somewhere outside its own folder, so it wasn't installed.".into());
            }
        } else if kind.is_dir() {
            check_tree(&path)?;
        } else if !kind.is_file() {
            return Err("That plugin has something in it that isn't a file or a folder, so it wasn't installed.".into());
        }
    }
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    for e in std::fs::read_dir(from).map_err(|e| format!("couldn't read {}: {e}", from.display()))?.flatten() {
        let (src, dst, kind) = (e.path(), to.join(e.file_name()), e.file_type().map_err(|e| e.to_string())?);
        if kind.is_dir() {
            copy_dir(&src, &dst)?;
        } else if kind.is_symlink() {
            let target = std::fs::read_link(&src).map_err(|e| e.to_string())?;
            std::os::unix::fs::symlink(target, &dst).map_err(|e| e.to_string())?;
        } else {
            // `copy` keeps the file's permissions, so a script stays runnable.
            std::fs::copy(&src, &dst).map_err(|e| format!("couldn't copy {}: {e}", src.display()))?;
        }
    }
    Ok(())
}

/// Take a plugin out of the person's folder. Its settings stay in the settings file, so
/// installing it again finds them.
pub fn remove(id: &str) -> Result<(), String> {
    if !valid_id(id) {
        return Err(format!("no plugin called {id} is installed"));
    }
    let dir = user_dir().join(id);
    let meta = dir.symlink_metadata().map_err(|_| format!("no plugin called {id} is installed in {}", user_dir().display()))?;
    // A link (a plugin being written somewhere else): only the link goes.
    let done = if meta.is_dir() { std::fs::remove_dir_all(&dir) } else { std::fs::remove_file(&dir) };
    done.map_err(|e| format!("couldn't remove {}: {e}", dir.display()))
}

/// The listing a newer version of this installed plugin is in. Versions are compared as text:
/// the list is the authority, so anything other than what it names is behind it.
pub fn update<'a>(m: &Manifest, list: &'a [Listing]) -> Option<&'a Listing> {
    list.iter().find(|l| l.id == m.id && removable(m) && !l.version.is_empty() && l.version != m.version)
}

/// Sample listings, for screenshots.
pub fn sample() -> Vec<Listing> {
    let l = |id: &str, name: &str, description: &str, author: &str, version: &str| Listing { id: id.into(), name: name.into(), description: description.into(), author: author.into(), homepage: format!("https://example.org/{id}"), version: version.into(), download: format!("https://example.org/{id}.tar.gz"), sha256: "0".repeat(64) };
    vec![l("weather", "Weather", "Today's forecast on the panel, and the week in its menu.", "Sam Rivera", "1.2"), l("pomodoro", "Pomodoro", "A work timer on the panel that tells you when to take a break.", "Kit Okafor", "0.4")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listings_that_cant_be_trusted_are_left_out() {
        let sum = "a".repeat(64);
        let text = format!(
            r#"{{"plugins":[
            {{"id":"weather","name":"Weather","version":"1.2","download":"https://x.org/w.tar.gz","sha256":"{sum}","homepage":"javascript:alert(1)"}},
            {{"id":"no-sum","name":"No Sum","download":"https://x.org/w.tar.gz"}},
            {{"id":"plain","name":"Plain","download":"http://x.org/w.tar.gz","sha256":"{sum}"}},
            {{"id":"../up","name":"Up","download":"https://x.org/w.tar.gz","sha256":"{sum}"}},
            {{"id":"weather","name":"Twice","download":"https://x.org/w.tar.gz","sha256":"{sum}"}}
            ]}}"#
        );
        let list = parse(&text, false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!((list[0].id.as_str(), list[0].homepage.as_str()), ("weather", ""));
        assert!(parse("<html>", false).is_err());
        assert!(valid_id("disk-space_2") && !valid_id("Disk") && !valid_id(".hidden") && !valid_id("a/b") && !valid_id(""));
    }

    /// The list in the repository: every listing in it must be one the shell would show, and
    /// none may take the name of a plugin that comes with the shell.
    #[test]
    fn the_published_list_is_sound() {
        let text = include_str!("../../../plugins/registry.json");
        let raw: serde_json::Value = serde_json::from_str(text).unwrap();
        let list = parse(text, false).unwrap();
        assert_eq!(list.len(), raw["plugins"].as_array().unwrap().len(), "a listing is missing a field, or its id, download or sha256 isn't valid");
        let own = concat!(env!("CARGO_MANIFEST_DIR"), "/data/plugins");
        for l in &list {
            assert!(!Path::new(own).join(&l.id).exists(), "{} is the id of a plugin that comes with the shell", l.id);
            assert!(!l.description.is_empty() && !l.author.is_empty() && !l.version.is_empty() && !l.homepage.is_empty(), "{} needs a description, author, version and homepage", l.id);
        }
    }

    #[test]
    fn archives_stay_in_their_folder() {
        assert!(stays_inside(Path::new("weather/plugin.conf")) && stays_inside(Path::new("./plugin.conf")));
        assert!(!stays_inside(Path::new("../x")) && !stays_inside(Path::new("/etc/x")) && !stays_inside(Path::new("a/../../x")));
    }

    #[test]
    fn links_out_of_a_plugin_are_refused() {
        let dir = std::env::temp_dir().join(format!("rivet-shell-store-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        std::fs::write(dir.join("plugin.conf"), "exec=./a.sh\n").unwrap();
        std::os::unix::fs::symlink("lib", dir.join("inside")).unwrap();
        assert!(check_tree(&dir).is_ok());
        std::os::unix::fs::symlink("/etc/passwd", dir.join("lib/out")).unwrap();
        assert!(check_tree(&dir).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
