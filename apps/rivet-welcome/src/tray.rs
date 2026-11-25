//! `rivet-welcome --tray`: a tray icon (StatusNotifierItem, with a dbusmenu menu: what Rivet
//! Shell, KDE, waybar and the others show) that looks for new releases when it starts and every
//! few hours after, marks itself and sends a notification when there are updates, and can install
//! them all from its menu. Clicking it opens the window. It started when you sign in (an
//! autostart entry), and goes away when the tray is turned off in the window's settings.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};

use zbus::blocking::Connection;
use zbus::message::Type as MsgType;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, StructureBuilder, Value};

use crate::settings::Settings;
use crate::state::{self, Row};
use crate::{local, releases};

/// Only one tray at a time.
pub const BUS_NAME: &str = "io.github.ulinaaron.RivetWelcome.Tray";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const ITEM_PATH: &str = "/StatusNotifierItem";
const MENU_PATH: &str = "/MenuBar";
const MENU: &str = "com.canonical.dbusmenu";

/// Menu rows.
const M_OPEN: i32 = 1;
const M_CHECK: i32 = 2;
const M_UPDATE: i32 = 3;
const M_SEP: i32 = 4;
const M_QUIT: i32 = 5;

enum Cmd {
    Open,
    Check,
    UpdateAll,
    Quit,
    /// The watcher (the panel) came back: register again.
    Register,
    Checked(Result<(), String>),
    Rows(Vec<Row>),
    Busy(Option<String>),
    UpdatesDone(Vec<String>, Vec<String>),
}

#[derive(Default)]
struct TrayState {
    /// "Rivet Files 0.7.0"…
    updates: Vec<String>,
    checking: bool,
    /// What it's installing now.
    busy: Option<String>,
    error: Option<String>,
    revision: u32,
}

impl TrayState {
    fn tooltip(&self) -> String {
        if let Some(b) = &self.busy {
            return b.clone();
        }
        if self.checking {
            return "Looking for updates…".into();
        }
        if let Some(e) = &self.error {
            return format!("Couldn't look for updates: {e}");
        }
        match self.updates.len() {
            0 => "The Rivet apps are up to date".into(),
            1 => format!("An update is ready: {}", self.updates[0]),
            n => format!("{n} updates are ready: {}", self.updates.join(", ")),
        }
    }
}

type Pixmaps = Vec<(i32, i32, Vec<u8>)>;
/// A dbusmenu row: id, properties, children.
type MenuLayout = (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>);

/// The icon at a few sizes, as ARGB32 in network byte order (what the protocol wants), and
/// with a dot for "updates are ready".
fn pixmaps(badge: bool) -> Pixmaps {
    [16u32, 22, 24, 32, 48, 64]
        .into_iter()
        .filter_map(|px| {
            let img = rivet_look::icons::render_svg(crate::gui::ICON_SVG.as_bytes(), px)?;
            let mut rgba: Vec<[u8; 4]> = img
                .pixels
                .iter()
                .map(|p| p.to_srgba_unmultiplied())
                .collect();
            if badge {
                draw_badge(&mut rgba, px as usize);
            }
            Some((
                px as i32,
                px as i32,
                rgba.into_iter()
                    .flat_map(|[r, g, b, a]| [a, r, g, b])
                    .collect(),
            ))
        })
        .collect()
}

/// A blue dot with a white ring, in the bottom right corner.
fn draw_badge(px: &mut [[u8; 4]], size: usize) {
    let r = size as f32 * 0.24;
    let (cx, cy) = (size as f32 - r - 0.5, size as f32 - r - 0.5);
    for y in 0..size {
        for x in 0..size {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            let ring = (r + 0.5 - d).clamp(0.0, 1.0);
            let dot = (r - size as f32 * 0.07 + 0.5 - d).clamp(0.0, 1.0);
            if ring <= 0.0 {
                continue;
            }
            let p = &mut px[y * size + x];
            let blend = |base: [u8; 4], c: [u8; 3], t: f32| -> [u8; 4] {
                let a = base[3] as f32 / 255.0;
                let out_a = t + a * (1.0 - t);
                let mix = |i: usize| {
                    if out_a > 0.0 {
                        ((c[i] as f32 * t + base[i] as f32 * a * (1.0 - t)) / out_a).round() as u8
                    } else {
                        0
                    }
                };
                [mix(0), mix(1), mix(2), (out_a * 255.0).round() as u8]
            };
            *p = blend(*p, [255, 255, 255], ring);
            *p = blend(*p, [0x35, 0x84, 0xe4], dot);
        }
    }
}

struct Item {
    st: Arc<Mutex<TrayState>>,
    tx: Mutex<Sender<Cmd>>,
    icon: Pixmaps,
    attention: Pixmaps,
}

impl Item {
    fn send(&self, c: Cmd) {
        if let Ok(t) = self.tx.lock() {
            let _ = t.send(c);
        }
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierItem")]
impl Item {
    #[zbus(property)]
    fn category(&self) -> String {
        "SystemServices".into()
    }
    #[zbus(property)]
    fn id(&self) -> String {
        "rivet-welcome".into()
    }
    #[zbus(property)]
    fn title(&self) -> String {
        "Rivet Welcome".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        let s = self
            .st
            .lock()
            .map(|s| !s.updates.is_empty() || s.busy.is_some())
            .unwrap_or(false);
        if s { "NeedsAttention" } else { "Active" }.into()
    }
    #[zbus(property)]
    fn window_id(&self) -> i32 {
        0
    }
    /// Pixels only: the icon may not be in the icon theme (a fresh install, or run from a build).
    #[zbus(property)]
    fn icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn icon_pixmap(&self) -> Pixmaps {
        self.icon.clone()
    }
    #[zbus(property)]
    fn attention_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn attention_icon_pixmap(&self) -> Pixmaps {
        self.attention.clone()
    }
    #[zbus(property)]
    fn overlay_icon_name(&self) -> String {
        String::new()
    }
    #[zbus(property)]
    fn overlay_icon_pixmap(&self) -> Pixmaps {
        Vec::new()
    }
    #[zbus(property)]
    fn tool_tip(&self) -> (String, Pixmaps, String, String) {
        let tip = self.st.lock().map(|s| s.tooltip()).unwrap_or_default();
        (String::new(), Vec::new(), "Rivet Welcome".into(), tip)
    }
    #[zbus(property)]
    fn item_is_menu(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn menu(&self) -> OwnedObjectPath {
        OwnedObjectPath::try_from(MENU_PATH).expect("a valid path")
    }

    fn activate(&self, _x: i32, _y: i32) {
        self.send(Cmd::Open);
    }
    fn secondary_activate(&self, _x: i32, _y: i32) {
        self.send(Cmd::Check);
    }
    fn context_menu(&self, _x: i32, _y: i32) {}
    fn scroll(&self, _delta: i32, _orientation: String) {}
}

struct Menu {
    st: Arc<Mutex<TrayState>>,
    tx: Mutex<Sender<Cmd>>,
}

fn owned(v: Value<'_>) -> OwnedValue {
    OwnedValue::try_from(v).unwrap_or_else(|_| OwnedValue::from(0i32))
}

impl Menu {
    /// Each row's properties, as things stand.
    fn rows(&self) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        let st = self.st.lock();
        let (checking, busy, n) = st
            .as_ref()
            .map(|s| (s.checking, s.busy.clone(), s.updates.len()))
            .unwrap_or((false, None, 0));
        let item = |label: String, enabled: bool, visible: bool| -> HashMap<String, OwnedValue> {
            HashMap::from([
                ("label".into(), owned(Value::from(label))),
                ("enabled".into(), owned(Value::from(enabled))),
                ("visible".into(), owned(Value::from(visible))),
            ])
        };
        let update = match (&busy, n) {
            (Some(b), _) => item(b.clone(), false, true),
            (None, 1) => item("Install the Update".into(), true, true),
            (None, n) => item(format!("Install {n} Updates"), true, n > 0),
        };
        vec![
            (M_OPEN, item("Open Rivet Welcome".into(), true, true)),
            (
                M_CHECK,
                item(
                    if checking {
                        "Looking for Updates…"
                    } else {
                        "Check for Updates"
                    }
                    .into(),
                    !checking && busy.is_none(),
                    true,
                ),
            ),
            (M_UPDATE, update),
            (
                M_SEP,
                HashMap::from([("type".into(), owned(Value::from("separator")))]),
            ),
            (M_QUIT, item("Quit".into(), true, true)),
        ]
    }

    fn clicked(&self, id: i32) {
        let c = match id {
            M_OPEN => Cmd::Open,
            M_CHECK => Cmd::Check,
            M_UPDATE => Cmd::UpdateAll,
            M_QUIT => Cmd::Quit,
            _ => return,
        };
        if let Ok(t) = self.tx.lock() {
            let _ = t.send(c);
        }
    }
}

fn layout_row(id: i32, props: HashMap<String, OwnedValue>) -> Option<OwnedValue> {
    let props: HashMap<String, Value> = props
        .into_iter()
        .map(|(k, v)| (k, Value::from(v)))
        .collect();
    let s = StructureBuilder::new()
        .add_field(id)
        .add_field(props)
        .add_field(Vec::<Value>::new())
        .build()
        .ok()?;
    OwnedValue::try_from(Value::from(s)).ok()
}

#[zbus::interface(name = "com.canonical.dbusmenu")]
impl Menu {
    fn get_layout(
        &self,
        parent_id: i32,
        _recursion_depth: i32,
        _property_names: Vec<String>,
    ) -> (u32, MenuLayout) {
        let rev = self.st.lock().map(|s| s.revision).unwrap_or(0);
        let root = HashMap::from([(
            "children-display".to_string(),
            owned(Value::from("submenu")),
        )]);
        let kids = if parent_id == 0 {
            self.rows()
                .into_iter()
                .filter_map(|(id, p)| layout_row(id, p))
                .collect()
        } else {
            Vec::new()
        };
        (
            rev,
            (
                parent_id,
                if parent_id == 0 { root } else { HashMap::new() },
                kids,
            ),
        )
    }

    fn get_group_properties(
        &self,
        ids: Vec<i32>,
        _property_names: Vec<String>,
    ) -> Vec<(i32, HashMap<String, OwnedValue>)> {
        self.rows()
            .into_iter()
            .filter(|(id, _)| ids.is_empty() || ids.contains(id))
            .collect()
    }

    fn get_property(&self, id: i32, name: String) -> OwnedValue {
        self.rows()
            .into_iter()
            .find(|(i, _)| *i == id)
            .and_then(|(_, mut p)| p.remove(&name))
            .unwrap_or_else(|| owned(Value::from("")))
    }

    fn event(&self, id: i32, event_id: String, _data: OwnedValue, _timestamp: u32) {
        if event_id == "clicked" {
            self.clicked(id);
        }
    }

    fn event_group(&self, events: Vec<(i32, String, OwnedValue, u32)>) -> Vec<i32> {
        for (id, ev, _, _) in events {
            if ev == "clicked" {
                self.clicked(id);
            }
        }
        Vec::new()
    }

    fn about_to_show(&self, _id: i32) -> bool {
        false
    }

    fn about_to_show_group(&self, _ids: Vec<i32>) -> (Vec<i32>, Vec<i32>) {
        (Vec::new(), Vec::new())
    }

    #[zbus(property)]
    fn version(&self) -> u32 {
        3
    }
    #[zbus(property)]
    fn text_direction(&self) -> String {
        "ltr".into()
    }
    #[zbus(property)]
    fn status(&self) -> String {
        "normal".into()
    }
    #[zbus(property)]
    fn icon_theme_path(&self) -> Vec<String> {
        Vec::new()
    }
}

/// Signals matching `rule`, each turned into a command by `f`, on a thread of their own.
fn listen(
    conn: &Connection,
    rule: zbus::MatchRule<'static>,
    tx: Sender<Cmd>,
    f: impl Fn(&zbus::Message) -> Option<Cmd> + Send + 'static,
) {
    let Ok(it) = zbus::blocking::MessageIterator::for_match_rule(rule, conn, Some(64)) else {
        return;
    };
    let _ = std::thread::Builder::new()
        .name("rivet-welcome signals".into())
        .spawn(move || {
            for m in it.flatten() {
                if let Some(c) = f(&m)
                    && tx.send(c).is_err()
                {
                    break;
                }
            }
        });
}

fn register(conn: &Connection, name: &str) -> bool {
    conn.call_method(
        Some(WATCHER),
        WATCHER_PATH,
        Some(WATCHER),
        "RegisterStatusNotifierItem",
        &(name,),
    )
    .is_ok()
}

/// What's been announced already (so signing in again doesn't say the same thing again).
fn told_file() -> PathBuf {
    rivet_xdg::places::cache_home().join("rivet-welcome/told")
}

/// When anything that changes the answer last changed: the releases list and the programs.
fn stamp(rows: &[Row]) -> Vec<Option<SystemTime>> {
    let mut out = vec![releases::cache_stamp()];
    out.extend(rows.iter().map(|r| {
        std::fs::metadata(local::bin_home().join(&r.entry.bin))
            .and_then(|m| m.modified())
            .ok()
    }));
    out
}

pub fn run() -> ExitCode {
    let s = Settings::load();
    if !s.tray {
        eprintln!("rivet-welcome: the tray is turned off (in the window's Settings)");
        return ExitCode::SUCCESS;
    }
    let (tx, rx) = channel::<Cmd>();
    let st: Arc<Mutex<TrayState>> = Arc::default();
    let item = Item {
        st: st.clone(),
        tx: Mutex::new(tx.clone()),
        icon: pixmaps(false),
        attention: pixmaps(true),
    };
    let menu = Menu {
        st: st.clone(),
        tx: Mutex::new(tx.clone()),
    };
    let conn = match zbus::blocking::connection::Builder::session()
        .and_then(|b| {
            b.method_timeout(Duration::from_secs(5))
                .serve_at(ITEM_PATH, item)
        })
        .and_then(|b| b.serve_at(MENU_PATH, menu))
        .and_then(|b| b.build())
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("rivet-welcome: no session bus ({e})");
            return ExitCode::FAILURE;
        }
    };
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    if !matches!(
        conn.request_name_with_flags(BUS_NAME, RequestNameFlags::DoNotQueue.into()),
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner)
    ) {
        println!("Rivet Welcome's tray is already running.");
        return ExitCode::SUCCESS;
    }
    let item_name = format!("org.kde.StatusNotifierItem-{}-1", std::process::id());
    let _ = conn.request_name(item_name.as_str());
    // The panel may start after this (or restart): register whenever a watcher appears.
    if let Ok(rule) = zbus::MatchRule::builder()
        .msg_type(MsgType::Signal)
        .sender("org.freedesktop.DBus")
        .and_then(|b| b.interface("org.freedesktop.DBus"))
        .and_then(|b| b.member("NameOwnerChanged"))
        .and_then(|b| b.arg(0, WATCHER))
    {
        listen(&conn, rule.build(), tx.clone(), |m| {
            let (_, _, new): (String, String, String) = m.body().deserialize().ok()?;
            (!new.is_empty()).then_some(Cmd::Register)
        });
    }
    if !register(&conn, &item_name) {
        eprintln!(
            "rivet-welcome: no tray to show the icon in yet (it appears when the panel starts)"
        );
    }
    // A click on our notification opens the window.
    let notice: Arc<Mutex<u32>> = Arc::default();
    if let Ok(rule) = zbus::MatchRule::builder()
        .msg_type(MsgType::Signal)
        .interface("org.freedesktop.Notifications")
        .and_then(|b| b.member("ActionInvoked"))
    {
        let notice = notice.clone();
        listen(&conn, rule.build(), tx.clone(), move |m| {
            let (id, _action): (u32, String) = m.body().deserialize().ok()?;
            (notice.lock().ok().is_some_and(|n| *n == id && id != 0)).then_some(Cmd::Open)
        });
    }

    let emit = |name: &str| {
        let _ = conn.emit_signal(None::<&str>, ITEM_PATH, ITEM, name, &());
    };
    let changed = |st: &Arc<Mutex<TrayState>>| {
        let (status, rev) = {
            let Ok(mut g) = st.lock() else { return };
            g.revision += 1;
            (
                if !g.updates.is_empty() || g.busy.is_some() {
                    "NeedsAttention"
                } else {
                    "Active"
                },
                g.revision,
            )
        };
        let _ = conn.emit_signal(None::<&str>, ITEM_PATH, ITEM, "NewStatus", &(status,));
        emit("NewToolTip");
        emit("NewIcon");
        emit("NewAttentionIcon");
        let _ = conn.emit_signal(None::<&str>, MENU_PATH, MENU, "LayoutUpdated", &(rev, 0i32));
    };

    let mut rows: Vec<Row> = Vec::new();
    let mut seen = stamp(&rows);
    let mut settings_seen = std::fs::metadata(crate::settings::path())
        .and_then(|m| m.modified())
        .ok();
    let mut next_check = Instant::now();
    let mut retries = 0;
    let mut watched_at = Instant::now();
    // What's in the cache already, first (the check may take a while, or fail).
    if let Some((list, _)) = releases::load_cached() {
        let tx = tx.clone();
        std::thread::spawn(move || tx.send(Cmd::Rows(state::rows(&list))));
    }
    loop {
        let wait = next_check
            .saturating_duration_since(Instant::now())
            .min(Duration::from_secs(3));
        let cmd = rx.recv_timeout(wait).ok();
        match cmd {
            Some(Cmd::Open) => open_window(),
            Some(Cmd::Quit) => break,
            Some(Cmd::Register) => {
                register(&conn, &item_name);
            }
            Some(Cmd::Check) => next_check = Instant::now(),
            Some(Cmd::Checked(r)) => {
                if let Ok(mut g) = st.lock() {
                    g.checking = false;
                    g.error = r.as_ref().err().cloned();
                }
                let hours = Settings::load().check_hours;
                next_check = match r {
                    Ok(()) => {
                        retries = 0;
                        if hours == 0 {
                            Instant::now() + Duration::from_secs(365 * 86_400)
                        } else {
                            Instant::now() + Duration::from_secs(hours as u64 * 3600)
                        }
                    }
                    // Not online yet at sign-in, most likely: try again soon, a few times.
                    Err(_) if retries < 5 => {
                        retries += 1;
                        Instant::now() + Duration::from_secs(60 * retries)
                    }
                    Err(_) => Instant::now() + Duration::from_secs(3600),
                };
                changed(&st);
            }
            Some(Cmd::Rows(r)) => {
                rows = r;
                seen = stamp(&rows);
                let list = state::update_names(&rows);
                if let Ok(mut g) = st.lock() {
                    g.updates = list.clone();
                }
                changed(&st);
                tell(&conn, &list, &notice);
            }
            Some(Cmd::Busy(b)) => {
                if let Ok(mut g) = st.lock() {
                    g.busy = b;
                }
                changed(&st);
            }
            Some(Cmd::UpdatesDone(ok, failed)) => {
                if let Ok(mut g) = st.lock() {
                    g.busy = None;
                }
                changed(&st);
                let (summary, body) = if failed.is_empty() {
                    (
                        if ok.len() == 1 {
                            "Updated".to_string()
                        } else {
                            format!("{} apps updated", ok.len())
                        },
                        ok.join(", "),
                    )
                } else {
                    ("Some updates didn't install".to_string(), failed.join("\n"))
                };
                notify(&conn, &summary, &body, &notice);
                seen = vec![];
            }
            Some(Cmd::UpdateAll) => {
                let busy = st.lock().map(|g| g.busy.is_some()).unwrap_or(true);
                let todo: Vec<Row> = state::updates(&rows).into_iter().cloned().collect();
                if !busy && !todo.is_empty() {
                    let tx = tx.clone();
                    std::thread::spawn(move || update_all(todo, tx));
                }
            }
            None => {}
        }
        if Instant::now() >= next_check && !st.lock().map(|g| g.checking).unwrap_or(true) {
            if let Ok(mut g) = st.lock() {
                g.checking = true;
            }
            changed(&st);
            next_check = Instant::now() + Duration::from_secs(365 * 86_400);
            let tx = tx.clone();
            std::thread::spawn(move || {
                let r = releases::check();
                let ok = r.as_ref().map(|_| ()).map_err(Clone::clone);
                if let Ok(list) = r {
                    let _ = tx.send(Cmd::Rows(state::rows(&list)));
                }
                let _ = tx.send(Cmd::Checked(ok));
            });
        }
        // Now and then: an app installed, updated or removed (here, in the window or by hand),
        // a check made in the window, the tray turned off.
        if watched_at.elapsed() >= Duration::from_secs(3) {
            watched_at = Instant::now();
            let now_settings = std::fs::metadata(crate::settings::path())
                .and_then(|m| m.modified())
                .ok();
            if now_settings != settings_seen {
                settings_seen = now_settings;
                if !Settings::load().tray {
                    break;
                }
            }
            let now = stamp(&rows);
            if now != seen && st.lock().map(|g| g.busy.is_none()).unwrap_or(false) {
                seen = now;
                if let Some((list, _)) = releases::load_cached() {
                    let tx = tx.clone();
                    std::thread::spawn(move || tx.send(Cmd::Rows(state::rows(&list))));
                }
            }
        }
    }
    let _ = conn.call_method(
        Some(WATCHER),
        WATCHER_PATH,
        Some(WATCHER),
        "UnregisterStatusNotifierItem",
        &(item_name.as_str(),),
    );
    ExitCode::SUCCESS
}

/// Install every update in turn, saying which one it's on.
fn update_all(todo: Vec<Row>, tx: Sender<Cmd>) {
    let (mut ok, mut failed) = (Vec::new(), Vec::new());
    for r in &todo {
        let Some((rel, v)) = r.update() else { continue };
        let _ = tx.send(Cmd::Busy(Some(format!("Updating {}…", r.entry.title))));
        match crate::install::run(&r.entry, rel, &|_| {}) {
            Ok(_) => ok.push(format!("{} {v}", r.entry.title)),
            Err(e) => failed.push(format!("{}: {e}", r.entry.title)),
        }
    }
    let _ = tx.send(Cmd::UpdatesDone(ok, failed));
}

/// A notification about updates not mentioned before.
fn tell(conn: &Connection, list: &[String], notice: &Arc<Mutex<u32>>) {
    if list.is_empty() || !Settings::load().notify {
        return;
    }
    let told: HashSet<String> = std::fs::read_to_string(told_file())
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    if list.iter().all(|u| told.contains(u)) {
        return;
    }
    let p = told_file();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&p, list.join("\n") + "\n");
    let summary = if list.len() == 1 {
        "An update for a Rivet app".to_string()
    } else {
        format!("{} updates for the Rivet apps", list.len())
    };
    notify(conn, &summary, &list.join(", "), notice);
}

fn notify(conn: &Connection, summary: &str, body: &str, notice: &Arc<Mutex<u32>>) {
    let replaces = notice.lock().map(|n| *n).unwrap_or(0);
    let hints: HashMap<&str, Value> =
        HashMap::from([("desktop-entry", Value::from("rivet-welcome"))]);
    let reply = conn.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            "Rivet Welcome",
            replaces,
            "rivet-welcome",
            summary,
            body,
            vec!["default", "Open"],
            hints,
            -1i32,
        ),
    );
    if let Ok(m) = reply
        && let Ok(id) = m.body().deserialize::<u32>()
        && let Ok(mut n) = notice.lock()
    {
        *n = id;
    }
}

/// The window, as its own program (it says so and quits when one is open already).
pub fn open_window() {
    if let Ok(exe) = std::env::current_exe() {
        let entry = crate::catalog::known()
            .into_iter()
            .find(|e| e.kind == crate::catalog::Kind::Welcome);
        if let Some(e) = entry {
            let _ = local::open(
                &crate::catalog::Entry {
                    open_args: Vec::new(),
                    ..e
                },
                &exe,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icons_and_menu() {
        let p = pixmaps(true);
        assert_eq!(p.len(), 6);
        let (w, h, data) = &p[3];
        assert_eq!((*w, *h, data.len()), (32, 32, 32 * 32 * 4));
        // The badge's corner is opaque blue-ish.
        let i = ((32 - 6) * 32 + (32 - 6)) * 4;
        assert_eq!(data[i], 255);
        assert!(data[i + 3] > data[i + 1]);
        let (tx, _rx) = channel();
        let st: Arc<Mutex<TrayState>> = Arc::default();
        st.lock().unwrap().updates = vec!["Rivet Files 0.7.0".into(), "Rivet Notes 0.2.0".into()];
        let m = Menu {
            st: st.clone(),
            tx: Mutex::new(tx),
        };
        let rows = m.rows();
        assert_eq!(rows.len(), 5);
        let label = rows[2]
            .1
            .get("label")
            .and_then(|v| String::try_from(v.try_clone().ok()?).ok());
        assert_eq!(label.as_deref(), Some("Install 2 Updates"));
        assert!(
            st.lock()
                .unwrap()
                .tooltip()
                .starts_with("2 updates are ready")
        );
        assert!(m.get_layout(0, -1, vec![]).1.2.len() == 5);
    }
}
