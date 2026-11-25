//! A notification through the desktop's notification service (org.freedesktop.Notifications),
//! for a program with no window to say something in. Whatever shows notifications shows it:
//! Rivet Shell, or another desktop's. Behind the `portal` feature, which brings the D-Bus code.

use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::Value;

/// Show a notification. `icon` is an icon's name or a picture's path; `ms` how long it stays
/// (-1 for the desktop's usual time). `transient` ones aren't kept in the notification list.
pub fn notify(app: &str, icon: &str, summary: &str, body: &str, ms: i32, transient: bool) -> Result<u32, String> {
    let conn = Connection::session().map_err(|e| format!("no session bus ({e})"))?;
    let n = Proxy::new(&conn, "org.freedesktop.Notifications", "/org/freedesktop/Notifications", "org.freedesktop.Notifications").map_err(|e| e.to_string())?;
    let mut hints: HashMap<&str, Value> = HashMap::new();
    if transient {
        hints.insert("transient", Value::from(true));
    }
    n.call("Notify", &(app, 0u32, icon, summary, body, Vec::<&str>::new(), hints, ms)).map_err(|e| format!("nothing shows notifications ({e})"))
}
