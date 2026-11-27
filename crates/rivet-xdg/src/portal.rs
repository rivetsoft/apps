//! The desktop's file picker, through the desktop portal (xdg-desktop-portal's FileChooser):
//! whichever picker the desktop uses opens (Rivet Files' when it's set as the picker, else
//! GTK's, KDE's…). Blocking: call it from a thread. Behind the `portal` feature, so only
//! the apps that ask for it build the D-Bus code.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";

/// What to choose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    Folder,
    Picture,
    /// A picture or a drawing (SVG) for an icon.
    Icon,
    /// An archive: a zip, a tar of any kind, a 7z and the like.
    Archive,
    /// A video or audio file, or a playlist.
    Media,
    /// A subtitle file.
    Subtitles,
}

/// Asks the picker; `Ok(None)` when it was cancelled.
pub fn choose(kind: Pick, title: &str, start: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    // The request's object path is known before asking, so its answer can't be missed.
    let sender = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).ok_or("no bus name")?;
    let token = format!("rivet_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0));
    let handle = format!("{PATH}/request/{sender}/{token}");
    let request = Proxy::new(&conn, DEST, handle.as_str(), "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
    let mut answers = request.receive_signal("Response").map_err(|e| e.to_string())?;

    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("modal", Value::from(true));
    if kind == Pick::Folder {
        opts.insert("directory", Value::from(true));
    } else if kind == Pick::Archive {
        const ENDS: [&str; 22] = ["zip", "jar", "7z", "rar", "tar", "tar.gz", "tgz", "tar.xz", "txz", "tar.bz2", "tbz2", "tar.zst", "tzst", "gz", "xz", "bz2", "zst", "iso", "cab", "deb", "rpm", "lz"];
        let pats: Vec<(u32, String)> = ENDS.iter().flat_map(|e| [(0u32, format!("*.{e}")), (0u32, format!("*.{}", e.to_uppercase()))]).collect();
        opts.insert("filters", Value::from(vec![Value::from(("Archives".to_string(), pats)), Value::from(("All files".to_string(), vec![(0u32, "*".to_string())]))]));
    } else if kind == Pick::Media {
        let pats: Vec<(u32, String)> = ["video/*", "audio/*", "application/vnd.apple.mpegurl"].iter().map(|m| (1u32, m.to_string())).collect();
        opts.insert("filters", Value::from(vec![Value::from(("Videos and music".to_string(), pats)), Value::from(("All files".to_string(), vec![(0u32, "*".to_string())]))]));
    } else if kind == Pick::Subtitles {
        const ENDS: [&str; 6] = ["srt", "ass", "ssa", "vtt", "sub", "sup"];
        let pats: Vec<(u32, String)> = ENDS.iter().flat_map(|e| [(0u32, format!("*.{e}")), (0u32, format!("*.{}", e.to_uppercase()))]).collect();
        opts.insert("filters", Value::from(vec![Value::from(("Subtitles".to_string(), pats)), Value::from(("All files".to_string(), vec![(0u32, "*".to_string())]))]));
    } else {
        let mut pats: Vec<(u32, String)> = ["*.jpg", "*.jpeg", "*.png", "*.webp", "*.JPG", "*.JPEG", "*.PNG", "*.WEBP"].iter().map(|g| (0u32, g.to_string())).collect();
        pats.extend(["image/jpeg", "image/png", "image/webp"].iter().map(|m| (1u32, m.to_string())));
        if kind == Pick::Icon {
            pats.extend([(0u32, "*.svg".to_string()), (0, "*.SVG".to_string()), (1, "image/svg+xml".to_string())]);
        }
        opts.insert("filters", Value::from(vec![Value::from(("Pictures".to_string(), pats))]));
    }
    if let Some(d) = start.filter(|d| d.is_dir()) {
        let mut bytes = d.as_os_str().as_encoded_bytes().to_vec();
        bytes.push(0);
        opts.insert("current_folder", Value::from(bytes));
    }
    let chooser = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.FileChooser").map_err(|e| e.to_string())?;
    let _: OwnedObjectPath = chooser.call("OpenFile", &("", title, opts)).map_err(|e| format!("the desktop has no file picker (xdg-desktop-portal): {e}"))?;
    let msg = answers.next().ok_or("the file picker gave no answer")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize().map_err(|e| e.to_string())?;
    if code != 0 {
        return Ok(None);
    }
    let uris: Vec<String> = results.get("uris").and_then(|v| v.try_clone().ok()).and_then(|v| Vec::<String>::try_from(v).ok()).unwrap_or_default();
    Ok(uris.first().and_then(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(crate::places::percent_decode(p))))
}

/// The object a portal request answers on, known before asking so its answer can't be missed.
fn request(conn: &Connection) -> Result<(String, zbus::blocking::proxy::SignalIterator<'static>), String> {
    let sender = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).ok_or("no bus name")?;
    let token = format!("rivet_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0));
    let handle = format!("{PATH}/request/{sender}/{token}");
    let request = Proxy::new(conn, DEST, handle, "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
    let answers = request.receive_signal("Response").map_err(|e| e.to_string())?;
    Ok((token, answers))
}

/// The file a portal answered with (`key` holds one address or a list of them).
fn answered(answers: &mut zbus::blocking::proxy::SignalIterator<'static>, key: &str) -> Result<Option<PathBuf>, String> {
    let msg = answers.next().ok_or("the desktop gave no answer")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize().map_err(|e| e.to_string())?;
    if code != 0 {
        return Ok(None);
    }
    let value = results.get(key).and_then(|v| v.try_clone().ok());
    let uri = match value {
        Some(v) => match v.try_clone().ok().and_then(|v| Vec::<String>::try_from(v).ok()) {
            Some(list) => list.into_iter().next(),
            None => String::try_from(v).ok(),
        },
        None => None,
    };
    Ok(uri.as_deref().and_then(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(crate::places::percent_decode(p))))
}

/// Asks the picker where to save a picture, suggesting `name` in `start`; `Ok(None)` when it
/// was cancelled. The picker has already asked about writing over a file that is there.
pub fn save_picture(title: &str, name: &str, start: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    let (token, mut answers) = request(&conn)?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("modal", Value::from(true));
    opts.insert("current_name", Value::from(name));
    opts.insert("filters", Value::from(vec![Value::from(("PNG pictures".to_string(), vec![(0u32, "*.png".to_string()), (1u32, "image/png".to_string())]))]));
    if let Some(d) = start.filter(|d| d.is_dir()) {
        let mut bytes = d.as_os_str().as_encoded_bytes().to_vec();
        bytes.push(0);
        opts.insert("current_folder", Value::from(bytes));
    }
    let chooser = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.FileChooser").map_err(|e| e.to_string())?;
    let _: OwnedObjectPath = chooser.call("SaveFile", &("", title, opts)).map_err(|e| format!("the desktop has no file picker (xdg-desktop-portal): {e}"))?;
    answered(&mut answers, "uris")
}

/// Asks the desktop for a picture of the screen (xdg-desktop-portal's Screenshot), for
/// desktops where programs can't take one themselves. `Ok(None)` when it was refused.
pub fn screenshot() -> Result<Option<PathBuf>, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    let (token, mut answers) = request(&conn)?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("interactive", Value::from(false));
    let shots = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.Screenshot").map_err(|e| e.to_string())?;
    let _: OwnedObjectPath = shots.call("Screenshot", &("", opts)).map_err(|e| format!("the desktop can't take a screenshot for an app (xdg-desktop-portal): {e}"))?;
    answered(&mut answers, "uri")
}

/// Asks the desktop to let the person pick a color from the screen (xdg-desktop-portal's
/// Screenshot, PickColor): red, green and blue, 0 to 1. `Ok(None)` when it was cancelled.
pub fn pick_color() -> Result<Option<[f64; 3]>, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    let (token, mut answers) = request(&conn)?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    let shots = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.Screenshot").map_err(|e| e.to_string())?;
    let _: OwnedObjectPath = shots.call("PickColor", &("", opts)).map_err(|e| format!("the desktop can't pick a color for an app (xdg-desktop-portal): {e}"))?;
    let msg = answers.next().ok_or("the desktop gave no answer")?;
    let (code, results): (u32, HashMap<String, OwnedValue>) = msg.body().deserialize().map_err(|e| e.to_string())?;
    if code != 0 {
        return Ok(None);
    }
    let color = results.get("color").and_then(|v| v.try_clone().ok()).and_then(|v| <(f64, f64, f64)>::try_from(v).ok());
    Ok(color.map(|(r, g, b)| [r, g, b]))
}

/// While this is kept, the desktop doesn't dim, lock or turn off the screen for being idle
/// (xdg-desktop-portal's Inhibit). Dropping it lets the screen sleep again.
pub struct IdleInhibit {
    conn: Connection,
    handle: OwnedObjectPath,
}

impl Drop for IdleInhibit {
    fn drop(&mut self) {
        if let Ok(request) = Proxy::new(&self.conn, DEST, self.handle.as_str(), "org.freedesktop.portal.Request") {
            let _: Result<(), _> = request.call("Close", &());
        }
    }
}

/// Keep the screen awake, saying why (the desktop may show `reason`). Blocking: call it from a
/// thread. An error means the desktop has no way to be asked, and the screen sleeps as usual.
pub fn inhibit_idle(reason: &str) -> Result<IdleInhibit, String> {
    // The flag for "idle" in the Inhibit portal (1 logout, 2 user switch, 4 suspend, 8 idle).
    const IDLE: u32 = 8;
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    let (token, _answers) = request(&conn)?;
    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("reason", Value::from(reason));
    let inhibit = Proxy::new(&conn, DEST, PATH, "org.freedesktop.portal.Inhibit").map_err(|e| e.to_string())?;
    let handle: OwnedObjectPath = inhibit.call("Inhibit", &("", IDLE, opts)).map_err(|e| format!("the desktop can't keep the screen awake for an app (xdg-desktop-portal): {e}"))?;
    Ok(IdleInhibit { conn, handle })
}
