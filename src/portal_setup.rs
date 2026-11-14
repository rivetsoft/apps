//! Make Rivet the desktop's file picker, or stop.
//!
//! Turning it on writes three things, all in the user's own folders:
//! * `~/.local/share/xdg-desktop-portal/portals/rivet.portal` — tells the portal Rivet exists;
//! * `~/.local/share/dbus-1/services/org.freedesktop.impl.portal.desktop.rivet.service` — lets
//!   the session bus start `rivet --portal` when a picker is needed;
//! * a `FileChooser=rivet;…;` line in `~/.config/xdg-desktop-portal/<desktop>-portals.conf`
//!   (or `portals.conf`), with the pickers used before kept after it as fallbacks.
//!
//! What it changed is remembered in `~/.local/state/rivet/portal-install`, so turning it off puts
//! the original line back and removes only the files it made. Then the portal is restarted.

use std::path::{Path, PathBuf};

use crate::places;

const PORTAL_NAME: &str = "rivet";
const INTERFACE: &str = "org.freedesktop.impl.portal.FileChooser";
const SERVICE_FILE: &str = "org.freedesktop.impl.portal.desktop.rivet.service";

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Rivet answers file-picker requests.
    On,
    /// Another picker does (its name, if known).
    Off(Option<String>),
    /// Can't be set up from here (inside Flatpak, or a portal folder the system controls).
    Unavailable(String),
}

fn portal_file() -> PathBuf {
    places::data_home().join("xdg-desktop-portal/portals/rivet.portal")
}

fn service_file() -> PathBuf {
    places::data_home().join("dbus-1/services").join(SERVICE_FILE)
}

fn state_file() -> PathBuf {
    places::state_home().join("rivet/portal-install")
}

/// Desktop names from XDG_CURRENT_DESKTOP, lower-case ("niri", "gnome").
fn desktops() -> Vec<String> {
    std::env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .split(':')
        .map(|d| d.trim().to_lowercase())
        .filter(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'))
        .collect()
}

/// The config file to edit: the first `<desktop>-portals.conf` that exists, else `portals.conf`
/// if it exists, else a new one for the desktop.
fn target_config() -> PathBuf {
    let dir = places::config_home().join("xdg-desktop-portal");
    let mut names: Vec<String> = desktops().iter().map(|d| format!("{d}-portals.conf")).collect();
    names.push("portals.conf".into());
    names.iter().map(|n| dir.join(n)).find(|p| p.is_file()).unwrap_or_else(|| dir.join(&names[0]))
}

/// The `key=value` in `[preferred]`, if any.
fn preferred(text: &str, key: &str) -> Option<String> {
    let mut in_group = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_group = t == "[preferred]";
        } else if in_group
            && let Some((k, v)) = t.split_once('=')
            && k.trim() == key
        {
            return Some(v.trim().to_string());
        }
    }
    None
}

/// Set (or with None, remove) `key=value` in `[preferred]`, keeping everything else as it was.
fn set_preferred(text: &str, key: &str, value: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_group = false;
    let mut group_seen = false;
    let mut done = false;
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if t.starts_with('[') {
            // Leaving [preferred] without having found the key: add it at the group's end.
            if in_group && !done {
                if let Some(v) = value {
                    while out.last().is_some_and(|l| l.trim().is_empty()) {
                        out.pop();
                    }
                    out.push(format!("{key}={v}"));
                    out.push(String::new());
                }
                done = true;
            }
            in_group = t == "[preferred]";
            group_seen |= in_group;
        } else if in_group && t.split_once('=').is_some_and(|(k, _)| k.trim() == key) {
            if let Some(v) = value {
                out.push(format!("{key}={v}"));
            }
            done = true;
            continue;
        }
        out.push(line.to_string());
        if i + 1 == lines.len() && in_group && !done {
            if let Some(v) = value {
                out.push(format!("{key}={v}"));
            }
            done = true;
        }
    }
    if !group_seen
        && let Some(v) = value
    {
        if out.last().is_some_and(|l| !l.trim().is_empty()) {
            out.push(String::new());
        }
        out.push("[preferred]".into());
        out.push(format!("{key}={v}"));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// The pickers in use before Rivet, to keep after it as fallbacks: from the user's config, else
/// the desktop's defaults in /etc or /usr/share.
fn previous_backends(user_text: &str) -> Vec<String> {
    let mut texts = vec![user_text.to_string()];
    for d in desktops() {
        for base in ["/etc/xdg-desktop-portal", "/usr/share/xdg-desktop-portal"] {
            texts.push(std::fs::read_to_string(Path::new(base).join(format!("{d}-portals.conf"))).unwrap_or_default());
        }
    }
    for base in ["/etc/xdg-desktop-portal", "/usr/share/xdg-desktop-portal"] {
        texts.push(std::fs::read_to_string(Path::new(base).join("portals.conf")).unwrap_or_default());
    }
    for t in texts {
        if let Some(v) = preferred(&t, INTERFACE).or_else(|| preferred(&t, "default")) {
            let list: Vec<String> = v.split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty() && s != PORTAL_NAME).collect();
            if !list.is_empty() {
                return list;
            }
        }
    }
    vec!["*".into()]
}

/// The command the bus runs to start the picker service.
fn exec() -> Option<String> {
    // .service Exec lines have no quoting rules worth trusting: the path must be plain.
    let plain = |p: &Path| !p.to_string_lossy().chars().any(|c| c.is_whitespace() || "\"'\\".contains(c));
    if let (Some(img), Some(dir)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
        && std::env::current_exe().is_ok_and(|e| e.starts_with(&dir))
    {
        let img = PathBuf::from(img);
        return plain(&img).then(|| format!("{} --portal", img.display()));
    }
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    if plain(&exe) {
        return Some(format!("{} --portal", exe.display()));
    }
    // This copy's path has a space: use a `rivet` command on PATH that runs this same program
    // (the link Preferences → System makes, for instance).
    std::env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .map(|d| Path::new(d).join("rivet"))
        .find(|c| plain(c) && c.canonicalize().is_ok_and(|r| r == exe))
        .map(|c| format!("{} --portal", c.display()))
}

pub fn status() -> Status {
    if places::in_flatpak() {
        return Status::Unavailable("Not available inside Flatpak".into());
    }
    if std::env::var_os("XDG_DESKTOP_PORTAL_DIR").is_some() {
        return Status::Unavailable("The system sets where portals live".into());
    }
    let text = std::fs::read_to_string(target_config()).unwrap_or_default();
    let first = preferred(&text, INTERFACE).and_then(|v| v.split(';').map(|s| s.trim().to_string()).find(|s| !s.is_empty()));
    if first.as_deref() == Some(PORTAL_NAME) && portal_file().is_file() && service_file().is_file() {
        Status::On
    } else {
        Status::Off(first.or_else(|| previous_backends(&text).into_iter().find(|b| b != "*")))
    }
}

/// Make Rivet the file picker.
pub fn enable() -> Result<(), String> {
    if let Status::Unavailable(why) = status() {
        return Err(why);
    }
    let exec = exec().ok_or("Rivet's path has a space or quote, which the session bus can't start. Link the terminal command (Preferences → System) or move Rivet somewhere simpler.")?;
    let config = target_config();
    if std::fs::symlink_metadata(&config).is_ok_and(|m| m.file_type().is_symlink()) {
        return Err(format!("{} is a link; set the file picker where that file is generated", config.display()));
    }
    let original = std::fs::read_to_string(&config).ok();
    let text = original.clone().unwrap_or_default();
    let mut chain = vec![PORTAL_NAME.to_string()];
    chain.extend(previous_backends(&text));
    let line = format!("{};", chain.join(";"));
    let write = |p: &Path, s: &str| -> Result<(), String> {
        if let Some(d) = p.parent() {
            std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
        std::fs::write(p, s).map_err(|e| format!("{}: {e}", p.display()))
    };
    // Remember what was there (only the first time, so turning it off restores the real original).
    let made_portal = !portal_file().exists();
    let made_service = !service_file().exists();
    if !state_file().exists() {
        let old = preferred(&text, INTERFACE).unwrap_or_default();
        let state = format!(
            "config={}\nhad_file={}\nhad_line={}\nline={old}\nmade_portal={}\nmade_service={}\n",
            config.display(),
            original.is_some() as u8,
            preferred(&text, INTERFACE).is_some() as u8,
            made_portal as u8,
            made_service as u8
        );
        write(&state_file(), &state)?;
    }
    write(&portal_file(), &format!("[portal]\nDBusName={}\nInterfaces={INTERFACE};\n", crate::portal::BUS_NAME))?;
    write(&service_file(), &format!("[D-BUS Service]\nName={}\nExec={exec}\n", crate::portal::BUS_NAME))?;
    write(&config, &set_preferred(&text, INTERFACE, Some(&line)))?;
    restart_portal();
    Ok(())
}

/// Stop being the file picker: put back what was there before.
pub fn disable() -> Result<(), String> {
    let state = std::fs::read_to_string(state_file()).unwrap_or_default();
    let field = |k: &str| state.lines().find_map(|l| l.strip_prefix(&format!("{k}=")).map(|v| v.to_string()));
    let config = field("config").map(PathBuf::from).unwrap_or_else(target_config);
    if let Ok(text) = std::fs::read_to_string(&config) {
        let restored = if field("had_line").as_deref() == Some("1") {
            set_preferred(&text, INTERFACE, field("line").as_deref())
        } else {
            set_preferred(&text, INTERFACE, None)
        };
        // A file Rivet created that now says nothing but an empty [preferred] goes away.
        let empty = restored.lines().all(|l| l.trim().is_empty() || l.trim() == "[preferred]");
        if empty && field("had_file").as_deref() == Some("0") {
            let _ = std::fs::remove_file(&config);
        } else {
            std::fs::write(&config, restored).map_err(|e| format!("{}: {e}", config.display()))?;
        }
    }
    if field("made_portal").as_deref() != Some("0") {
        let _ = std::fs::remove_file(portal_file());
    }
    if field("made_service").as_deref() != Some("0") {
        let _ = std::fs::remove_file(service_file());
    }
    let _ = std::fs::remove_file(state_file());
    restart_portal();
    Ok(())
}

/// The portal reads its configuration when it starts.
fn restart_portal() {
    // Test hook for checks: leave the running portal alone.
    if std::env::var_os("RIVET_NO_PORTAL_RESTART").is_some() {
        return;
    }
    // The session bus reads its list of startable services on demand only: without this, the
    // portal's first call says Rivet's picker "is not activatable".
    if let Ok(conn) = zbus::blocking::Connection::session()
        && let Ok(proxy) = zbus::blocking::fdo::DBusProxy::new(&conn)
    {
        let _ = proxy.reload_config();
    }
    let _ = std::process::Command::new("systemctl")
        .args(["--user", "restart", "xdg-desktop-portal.service"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONF: &str = "[preferred]\ndefault=gtk;gnome;\norg.freedesktop.impl.portal.FileChooser=dfiles;gnome;\n";

    #[test]
    fn reads_and_replaces_the_preferred_line() {
        assert_eq!(preferred(CONF, INTERFACE).as_deref(), Some("dfiles;gnome;"));
        let on = set_preferred(CONF, INTERFACE, Some("rivet;dfiles;gnome;"));
        assert_eq!(preferred(&on, INTERFACE).as_deref(), Some("rivet;dfiles;gnome;"));
        assert_eq!(preferred(&on, "default").as_deref(), Some("gtk;gnome;"));
        let back = set_preferred(&on, INTERFACE, Some("dfiles;gnome;"));
        assert_eq!(back, CONF);
    }

    #[test]
    fn adds_the_line_or_the_group_when_missing() {
        let t = set_preferred("[preferred]\ndefault=gtk;\n", INTERFACE, Some("rivet;*;"));
        assert_eq!(preferred(&t, INTERFACE).as_deref(), Some("rivet;*;"));
        let t = set_preferred("", INTERFACE, Some("rivet;*;"));
        assert_eq!(t, "[preferred]\norg.freedesktop.impl.portal.FileChooser=rivet;*;\n");
        // Removing it again leaves the rest alone.
        let t = set_preferred("[preferred]\ndefault=gtk;\norg.freedesktop.impl.portal.FileChooser=rivet;\n", INTERFACE, None);
        assert_eq!(t, "[preferred]\ndefault=gtk;\n");
    }

    #[test]
    fn previous_pickers_skip_rivet() {
        let t = "[preferred]\norg.freedesktop.impl.portal.FileChooser=rivet;dfiles;gnome;\n";
        assert_eq!(previous_backends(t), vec!["dfiles".to_string(), "gnome".to_string()]);
    }
}
