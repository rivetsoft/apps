//! How the desktop wants things to look and move, for people who need it: higher contrast and
//! less motion. Read from the desktop portal's Settings (`org.freedesktop.appearance`, which
//! Softcore and GNOME serve, and GNOME's own interface keys), else from the GTK settings files.
//! Blocking: call it from a thread. Behind the `portal` feature, like the file picker.

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

use crate::places;

const DEST: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const IFACE: &str = "org.freedesktop.portal.Settings";

/// What the desktop asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Prefs {
    /// Stronger edges and text (the portal's `contrast` is 1, or a high-contrast GTK theme).
    pub high_contrast: bool,
    /// Things should appear and go at once rather than slide or fade.
    pub reduced_motion: bool,
}

/// One setting from the portal, unwrapped from the variant (older portals wrap it twice).
fn read_one(settings: &Proxy, namespace: &str, key: &str) -> Option<OwnedValue> {
    let v: OwnedValue = settings.call("ReadOne", &(namespace, key)).or_else(|_| settings.call("Read", &(namespace, key))).ok()?;
    Some(unwrap(v))
}

fn unwrap(v: OwnedValue) -> OwnedValue {
    match &*v {
        Value::Value(inner) => inner.try_to_owned().map(unwrap).unwrap_or(v),
        _ => v,
    }
}

fn as_u32(v: &OwnedValue) -> Option<u32> {
    u32::try_from(v).ok().or_else(|| i32::try_from(v).ok().map(|n| n.max(0) as u32))
}

fn as_bool(v: &OwnedValue) -> Option<bool> {
    bool::try_from(v).ok()
}

/// A line `key=value` of the GTK settings files, GTK 4's first.
fn gtk_setting(key: &str) -> Option<String> {
    let c = places::config_home();
    for f in ["gtk-4.0/settings.ini", "gtk-3.0/settings.ini"] {
        let Ok(t) = std::fs::read_to_string(c.join(f)) else { continue };
        for l in t.lines() {
            if let Some(v) = l.trim().strip_prefix(key).filter(|v| v.trim_start().starts_with('=')) {
                return Some(v.trim_start_matches([' ', '=']).trim().to_string());
            }
        }
    }
    None
}

fn from_gtk() -> Prefs {
    Prefs {
        high_contrast: gtk_setting("gtk-theme-name").is_some_and(|t| t.to_lowercase().contains("highcontrast")),
        reduced_motion: gtk_setting("gtk-enable-animations").is_some_and(|v| v == "0" || v == "false"),
    }
}

fn from_portal(settings: &Proxy) -> Prefs {
    let gtk = from_gtk();
    let contrast = read_one(settings, "org.freedesktop.appearance", "contrast").and_then(|v| as_u32(&v)).map(|c| c == 1);
    let gnome_contrast = read_one(settings, "org.gnome.desktop.a11y.interface", "high-contrast").and_then(|v| as_bool(&v));
    let reduced = read_one(settings, "org.freedesktop.appearance", "reduced-motion").and_then(|v| as_u32(&v)).map(|r| r == 1);
    let animations = read_one(settings, "org.gnome.desktop.interface", "enable-animations").and_then(|v| as_bool(&v));
    Prefs {
        high_contrast: contrast.unwrap_or(false) || gnome_contrast.unwrap_or(false) || gtk.high_contrast,
        reduced_motion: reduced.unwrap_or(false) || animations == Some(false) || gtk.reduced_motion,
    }
}

fn settings(conn: &Connection) -> Option<Proxy<'_>> {
    Proxy::new(conn, DEST, PATH, IFACE).ok()
}

/// What the desktop asks for now: the portal's answer, or the GTK settings' with no portal.
pub fn read() -> Prefs {
    match Connection::session().ok().as_ref().and_then(settings) {
        Some(s) => from_portal(&s),
        None => from_gtk(),
    }
}

/// Tell `changed` what the desktop asks for, first now and then each time the portal says a
/// setting changed. Blocks for as long as the session bus is there; run it on a thread.
pub fn watch(mut changed: impl FnMut(Prefs)) {
    let Ok(conn) = Connection::session() else {
        changed(from_gtk());
        return;
    };
    let Some(s) = settings(&conn) else {
        changed(from_gtk());
        return;
    };
    let mut last = from_portal(&s);
    changed(last);
    let Ok(signals) = s.receive_signal("SettingChanged") else { return };
    for _ in signals {
        let now = from_portal(&s);
        if now != last {
            last = now;
            changed(now);
        }
    }
}
