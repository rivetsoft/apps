//! Notifications: Rivet Shell answers on the session bus as `org.freedesktop.Notifications`, so
//! apps' notifications show as cards in the corner of the screen and stay in the control
//! center's history. When another notification service already has the name (mako, dunst,
//! another shell), the shell leaves it be and says so.

use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

use chrono::{DateTime, Local};
use zbus::zvariant::OwnedValue;

pub const NAME: &str = "org.freedesktop.Notifications";
const PATH: &str = "/org/freedesktop/Notifications";

#[derive(Clone, Debug, PartialEq)]
pub struct Note {
    pub id: u32,
    pub app: String,
    /// An icon name or a file path (may be empty).
    pub icon: String,
    /// The sending app's desktop id, when it says.
    pub desktop: String,
    pub summary: String,
    pub body: String,
    /// (key, label); "default" is the card's own click.
    pub actions: Vec<(String, String)>,
    /// 0 low, 1 normal, 2 critical.
    pub urgency: u8,
    /// How long the card shows (None: until closed).
    pub show_for: Option<Duration>,
    pub at: DateTime<Local>,
}

impl Note {
    /// Buttons to show (the "default" action is the card's click, not a button).
    pub fn buttons(&self) -> impl Iterator<Item = &(String, String)> {
        self.actions.iter().filter(|(k, _)| k != "default")
    }
}

/// What the service tells the shell.
pub enum NoteMsg {
    New(Note),
    Close(u32),
}

/// Why a notification closed (the spec's numbers).
#[derive(Clone, Copy)]
pub enum Reason {
    Expired = 1,
    Dismissed = 2,
    Asked = 3,
}

struct Service {
    next: AtomicU32,
    send: Mutex<Box<dyn Fn(NoteMsg) + Send>>,
}

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Service {
    fn get_capabilities(&self) -> Vec<String> {
        ["body", "actions", "persistence", "icon-static"].map(String::from).to_vec()
    }

    #[allow(clippy::too_many_arguments)]
    fn notify(&self, app_name: String, replaces_id: u32, app_icon: String, summary: String, body: String, actions: Vec<String>, hints: std::collections::HashMap<String, OwnedValue>, expire_timeout: i32) -> u32 {
        let id = if replaces_id != 0 { replaces_id } else { self.next.fetch_add(1, Ordering::Relaxed) };
        let hint_str = |k: &str| hints.get(k).and_then(|v| String::try_from(v.clone()).ok()).unwrap_or_default();
        let urgency = hints.get("urgency").and_then(|v| u8::try_from(v.clone()).ok()).unwrap_or(1).min(2);
        let mut icon = app_icon;
        for k in ["image-path", "image_path"] {
            let p = hint_str(k);
            if icon.is_empty() && !p.is_empty() {
                icon = p;
            }
        }
        let note = Note {
            id,
            app: app_name,
            icon: icon.strip_prefix("file://").map(String::from).unwrap_or(icon),
            desktop: hint_str("desktop-entry"),
            summary: plain(&summary),
            body: plain(&body),
            actions: actions.chunks(2).filter(|c| c.len() == 2).map(|c| (c[0].clone(), c[1].clone())).collect(),
            urgency,
            show_for: show_for(expire_timeout, urgency),
            at: Local::now(),
        };
        if let Ok(f) = self.send.lock() {
            f(NoteMsg::New(note));
        }
        id
    }

    fn close_notification(&self, id: u32) {
        if let Ok(f) = self.send.lock() {
            f(NoteMsg::Close(id));
        }
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        ("Rivet Shell".into(), "Rivet".into(), env!("CARGO_PKG_VERSION").into(), "1.2".into())
    }
}

/// How long a card shows: the app's wish, else 6 s; critical ones until closed.
fn show_for(expire_ms: i32, urgency: u8) -> Option<Duration> {
    match expire_ms {
        0 => None,
        _ if urgency >= 2 && expire_ms < 0 => None,
        t if t > 0 => Some(Duration::from_millis(t.clamp(1500, 60_000) as u64)),
        _ => Some(Duration::from_secs(6)),
    }
}

/// The body as plain text: the few tags the spec allows dropped, entities decoded.
pub fn plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_tag = false;
    for c in s.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&#39;", "'").replace("&amp;", "&").trim().to_string()
}

/// The running service: its connection, to tell apps a notification closed or was clicked.
pub struct Server {
    conn: zbus::blocking::Connection,
}

impl Server {
    /// Take the notification name on the session bus. `send` gets each notification.
    pub fn start(send: impl Fn(NoteMsg) + Send + 'static) -> Result<Server, String> {
        let svc = Service { next: AtomicU32::new(1), send: Mutex::new(Box::new(send)) };
        let conn = zbus::blocking::connection::Builder::session().and_then(|b| b.serve_at(PATH, svc)).and_then(|b| b.build()).map_err(|e| format!("no session bus ({e})"))?;
        use zbus::fdo::{RequestNameFlags, RequestNameReply};
        match conn.request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into()) {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(Server { conn }),
            Ok(_) => Err("another notification service is running (mako, dunst or another shell), so Rivet Shell leaves notifications to it".into()),
            Err(e) => Err(format!("couldn't take the notifications name ({e})")),
        }
    }

    pub fn closed(&self, id: u32, why: Reason) {
        let _ = self.conn.emit_signal(None::<&str>, PATH, NAME, "NotificationClosed", &(id, why as u32));
    }

    pub fn invoke(&self, id: u32, key: &str) {
        let _ = self.conn.emit_signal(None::<&str>, PATH, NAME, "ActionInvoked", &(id, key));
    }
}

/// The shell's notifications: the history (newest first) and the cards on screen.
#[derive(Default)]
pub struct Notes {
    pub list: Vec<Note>,
    /// Cards on screen, and when each goes (None: stays).
    pub cards: Vec<(u32, Option<Instant>)>,
}

impl Notes {
    const KEEP: usize = 60;
    const CARDS: usize = 4;

    /// A new (or replaced) notification; `card`: show it on screen too.
    pub fn add(&mut self, n: Note, card: bool) {
        let until = n.show_for.map(|d| Instant::now() + d);
        let id = n.id;
        self.list.retain(|x| x.id != id);
        self.list.insert(0, n);
        self.list.truncate(Self::KEEP);
        self.cards.retain(|(c, _)| *c != id);
        if card {
            self.cards.insert(0, (id, until));
            self.cards.truncate(Self::CARDS);
        }
    }

    /// Cards whose time is up (taken off the screen; they stay in the history).
    pub fn expire(&mut self, now: Instant) -> Vec<u32> {
        let gone: Vec<u32> = self.cards.iter().filter(|(_, t)| t.is_some_and(|t| now >= t)).map(|(id, _)| *id).collect();
        self.cards.retain(|(id, _)| !gone.contains(id));
        gone
    }

    pub fn uncard(&mut self, id: u32) {
        self.cards.retain(|(c, _)| *c != id);
    }

    pub fn remove(&mut self, id: u32) {
        self.uncard(id);
        self.list.retain(|n| n.id != id);
    }

    pub fn get(&self, id: u32) -> Option<&Note> {
        self.list.iter().find(|n| n.id == id)
    }

    /// The cards on screen, newest first.
    pub fn on_screen(&self) -> Vec<Note> {
        self.cards.iter().filter_map(|(id, _)| self.get(*id).cloned()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(id: u32) -> Note {
        Note { id, app: "a".into(), icon: String::new(), desktop: String::new(), summary: format!("n{id}"), body: String::new(), actions: Vec::new(), urgency: 1, show_for: Some(Duration::from_millis(10)), at: Local::now() }
    }

    #[test]
    fn markup_and_times() {
        assert_eq!(plain("<b>Hi</b> &amp; <a href=\"x\">there</a>"), "Hi & there");
        assert_eq!(show_for(-1, 1), Some(Duration::from_secs(6)));
        assert_eq!(show_for(-1, 2), None);
        assert_eq!(show_for(0, 1), None);
        assert_eq!(show_for(100, 1), Some(Duration::from_millis(1500)));
    }

    #[test]
    fn cards_come_and_go() {
        let mut n = Notes::default();
        for i in 1..=6 {
            n.add(note(i), true);
        }
        assert_eq!(n.cards.len(), 4);
        assert_eq!(n.on_screen()[0].id, 6);
        // Replacing keeps one entry.
        n.add(note(6), true);
        assert_eq!(n.list.iter().filter(|x| x.id == 6).count(), 1);
        let gone = n.expire(Instant::now() + Duration::from_secs(1));
        assert_eq!(gone.len(), 4);
        assert!(n.cards.is_empty() && n.list.len() == 6);
        n.remove(3);
        assert!(n.get(3).is_none());
    }

    /// Run with a private bus: `dbus-run-session -- cargo test -p rivet-shell serves -- --ignored`.
    #[test]
    #[ignore]
    fn serves_on_the_bus() {
        let (tx, rx) = std::sync::mpsc::channel();
        let _server = Server::start(move |m| {
            if let NoteMsg::New(n) = m {
                let _ = tx.send(n);
            }
        })
        .expect("the name");
        let conn = zbus::blocking::Connection::session().unwrap();
        let hints: std::collections::HashMap<&str, zbus::zvariant::Value> = [("urgency", zbus::zvariant::Value::U8(2))].into_iter().collect();
        let reply = conn.call_method(Some(NAME), PATH, Some(NAME), "Notify", &("Test", 0u32, "dialog-information", "Hello", "<b>World</b>", vec!["default", "Open", "later", "Later"], hints, -1i32)).unwrap();
        let id: u32 = reply.body().deserialize().unwrap();
        let n = rx.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!((n.id, n.summary.as_str(), n.body.as_str(), n.urgency, n.show_for), (id, "Hello", "World", 2, None));
        assert_eq!(n.buttons().map(|b| b.1.as_str()).collect::<Vec<_>>(), ["Later"]);
    }
}
