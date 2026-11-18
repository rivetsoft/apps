//! The desktop's file picker, through the desktop portal (xdg-desktop-portal's FileChooser):
//! whichever picker the desktop uses opens (Rivet Files' when it's set as the picker, else
//! GTK's, KDE's…). Blocking: call it from a thread.

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
}

/// Asks the picker; `Ok(None)` when it was cancelled.
pub fn choose(kind: Pick, title: &str, start: Option<&Path>) -> Result<Option<PathBuf>, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    // The request's object path is known before asking, so its answer can't be missed.
    let sender = conn.unique_name().map(|n| n.trim_start_matches(':').replace('.', "_")).ok_or("no bus name")?;
    let token = format!("rivet_shell_{}_{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0));
    let handle = format!("{PATH}/request/{sender}/{token}");
    let request = Proxy::new(&conn, DEST, handle.as_str(), "org.freedesktop.portal.Request").map_err(|e| e.to_string())?;
    let mut answers = request.receive_signal("Response").map_err(|e| e.to_string())?;

    let mut opts: HashMap<&str, Value> = HashMap::new();
    opts.insert("handle_token", Value::from(token.as_str()));
    opts.insert("modal", Value::from(true));
    if kind == Pick::Folder {
        opts.insert("directory", Value::from(true));
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
    Ok(uris.first().and_then(|u| u.strip_prefix("file://")).map(|p| PathBuf::from(rivet_xdg::places::percent_decode(p))))
}
