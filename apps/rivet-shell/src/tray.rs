//! The system tray: apps' status icons (Discord, Steam, Nextcloud, nm-applet…), by the
//! StatusNotifierItem protocol, the one KDE, waybar and the other shells use.
//!
//! The shell is the tray's *watcher* (`org.kde.StatusNotifierWatcher`, where apps register their
//! icons) when nothing else is; when waybar or another shell already is, it registers with that
//! one as a *host* and reads the same items. Each item's icon (a theme name, a file in the app's
//! own folder, or pixels), tooltip, status and menu (`com.canonical.dbusmenu`) is read on this
//! thread, read again when the app says it changed, and dropped when the app goes away.
//! Old XEmbed tray icons aren't shown (`xembed-sni-proxy` turns them into these).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use zbus::blocking::Connection;
use zbus::message::Type as MsgType;
use zbus::zvariant::{OwnedValue, Value};

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const ITEM: &str = "org.kde.StatusNotifierItem";
const MENU: &str = "com.canonical.dbusmenu";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Status {
    /// Nothing to say just now (shown unless the settings leave these out).
    Passive,
    #[default]
    Active,
    /// Wants a look (a new message…): marked with a dot.
    NeedsAttention,
}

/// An icon's pixels, RGBA (unmultiplied).
#[derive(Clone, Debug, PartialEq)]
pub struct Pixmap {
    pub w: usize,
    pub h: usize,
    pub rgba: Arc<Vec<u8>>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TrayItem {
    /// "bus name/object path".
    pub key: String,
    /// The app's own name for it ("discord", "nm-applet"…).
    pub id: String,
    pub title: String,
    /// The tooltip's title and text.
    pub tip: String,
    pub status: Status,
    /// A theme icon name, or a file (in the app's own icon folder, or given as a path).
    pub icon_name: String,
    pub icon_file: Option<PathBuf>,
    pub pixmap: Option<Pixmap>,
    pub attention_name: String,
    pub attention_pixmap: Option<Pixmap>,
    /// It has a menu (dbusmenu); `is_menu`: a click shows the menu rather than activating.
    pub has_menu: bool,
    pub is_menu: bool,
    /// Changes whenever the icon does (for the panel's texture cache).
    pub stamp: u64,
}

impl TrayItem {
    pub fn name(&self) -> &str {
        if !self.title.is_empty() { &self.title } else { &self.id }
    }
    pub fn tooltip(&self) -> String {
        if self.tip.is_empty() { self.name().to_string() } else { self.tip.clone() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Toggle {
    #[default]
    None,
    Check,
    Radio,
}

/// A row of an item's menu.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MenuItem {
    pub id: i32,
    /// Without the mnemonic underscores.
    pub label: String,
    pub enabled: bool,
    pub separator: bool,
    pub toggle: Toggle,
    pub checked: bool,
    pub icon: String,
    pub children: Vec<MenuItem>,
}

/// What the panel and the menu popup show.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Tray {
    /// In the order they came.
    pub items: Vec<TrayItem>,
    /// The open menu: its item's key and its rows (None while it's read).
    pub menu: Option<(String, Option<Vec<MenuItem>>)>,
    /// The shell is the watcher (else another program is, and the shell reads from it).
    pub watcher: bool,
    pub error: Option<String>,
}

/// What the person did.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// A click: the app's main action (its window, usually).
    Activate(String),
    /// A middle click.
    Secondary(String),
    /// The app's own menu (for items without a dbusmenu).
    ContextMenu(String),
    Scroll(String, i32),
    /// Read an item's menu, for the popup.
    OpenMenu(String),
    /// A submenu is about to show (some apps fill it only then).
    OpenSubmenu(String, i32),
    MenuClick(String, i32),
    CloseMenu,
}

enum Ev {
    Added(String),
    Removed(String),
    /// An item's app says something changed: its unique bus name and object path.
    Changed(String, String),
    /// A bus name went away.
    Gone(String),
    Do(Job),
}

#[derive(Clone)]
pub struct TrayLive {
    state: Arc<Mutex<Tray>>,
    generation: Arc<AtomicU64>,
    started: Arc<AtomicBool>,
    tx: Arc<Mutex<Option<Sender<Ev>>>>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl TrayLive {
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> TrayLive {
        TrayLive { state: Arc::default(), generation: Arc::default(), started: Arc::default(), tx: Arc::default(), wake: Arc::new(wake) }
    }
    pub fn get(&self) -> Tray {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }
    /// Start the tray (once; it then keeps going, so apps' icons stay registered).
    pub fn start(&self) {
        if self.started.swap(true, Ordering::Relaxed) {
            return;
        }
        let l = self.clone();
        let _ = std::thread::Builder::new().name("rivet-shell tray".into()).spawn(move || {
            if let Err(e) = run(&l) {
                eprintln!("rivet-shell: tray: {e}");
                l.edit(|t| t.error = Some(e));
            }
        });
    }
    pub fn run(&self, job: Job) {
        if let Ok(t) = self.tx.lock()
            && let Some(t) = t.as_ref()
        {
            let _ = t.send(Ev::Do(job));
        }
    }
    fn edit(&self, f: impl FnOnce(&mut Tray)) {
        let changed = match self.state.lock() {
            Ok(mut s) => {
                let before = s.clone();
                f(&mut s);
                *s != before
            }
            Err(_) => false,
        };
        if changed {
            self.generation.fetch_add(1, Ordering::Relaxed);
            (self.wake)();
        }
    }
}

// ------------------------------------------------------------------ the watcher

struct Watcher {
    items: Arc<Mutex<Vec<String>>>,
    tx: Mutex<Sender<Ev>>,
}

/// An item's key from what it registered: a bus name (its object is /StatusNotifierItem) or
/// an object path (on the sender's connection).
fn item_key(service: &str, sender: &str) -> String {
    if service.starts_with('/') {
        format!("{sender}{service}")
    } else if service.contains('/') {
        service.to_string()
    } else {
        format!("{service}/StatusNotifierItem")
    }
}

/// A key's bus name and object path.
fn split_key(key: &str) -> (&str, &str) {
    match key.find('/') {
        Some(i) => (&key[..i], &key[i..]),
        None => (key, "/StatusNotifierItem"),
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    async fn register_status_notifier_item(&self, service: String, #[zbus(header)] hdr: zbus::message::Header<'_>, #[zbus(signal_emitter)] em: zbus::object_server::SignalEmitter<'_>) {
        let sender = hdr.sender().map(|s| s.to_string()).unwrap_or_default();
        let key = item_key(&service, &sender);
        let new = match self.items.lock() {
            Ok(mut l) if !l.contains(&key) => {
                l.push(key.clone());
                true
            }
            _ => false,
        };
        if new {
            let _ = Self::status_notifier_item_registered(&em, &key).await;
        }
        if let Ok(t) = self.tx.lock() {
            let _ = t.send(Ev::Added(key));
        }
    }

    async fn register_status_notifier_host(&self, _service: String, #[zbus(signal_emitter)] em: zbus::object_server::SignalEmitter<'_>) {
        let _ = Self::status_notifier_host_registered(&em).await;
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.items.lock().map(|l| l.clone()).unwrap_or_default()
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        true
    }

    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(em: &zbus::object_server::SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_item_unregistered(em: &zbus::object_server::SignalEmitter<'_>, service: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    async fn status_notifier_host_registered(em: &zbus::object_server::SignalEmitter<'_>) -> zbus::Result<()>;
}

/// Signals matching `rule`, each turned into an event by `f`, on a thread of their own.
fn listen(conn: &Connection, rule: zbus::MatchRule<'static>, tx: Sender<Ev>, f: impl Fn(&zbus::Message) -> Option<Ev> + Send + 'static) -> Result<(), String> {
    let it = zbus::blocking::MessageIterator::for_match_rule(rule, conn, Some(256)).map_err(|e| e.to_string())?;
    std::thread::Builder::new()
        .name("rivet-shell tray signals".into())
        .spawn(move || {
            for m in it.flatten() {
                if let Some(ev) = f(&m)
                    && tx.send(ev).is_err()
                {
                    break;
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(())
}

fn run(l: &TrayLive) -> Result<(), String> {
    let (tx, rx) = channel::<Ev>();
    if let Ok(mut t) = l.tx.lock() {
        *t = Some(tx.clone());
    }
    let registered: Arc<Mutex<Vec<String>>> = Arc::default();
    let watcher = Watcher { items: registered.clone(), tx: Mutex::new(tx.clone()) };
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.method_timeout(Duration::from_secs(3)).serve_at(WATCHER_PATH, watcher))
        .and_then(|b| b.build())
        .map_err(|e| format!("no session bus ({e})"))?;
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    let own = matches!(conn.request_name_with_flags(WATCHER, RequestNameFlags::DoNotQueue.into()), Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner));
    let host = format!("org.kde.StatusNotifierHost-{}", std::process::id());
    let _ = conn.request_name(host.as_str());
    l.edit(|t| t.watcher = own);

    // Items saying they changed.
    let rule = zbus::MatchRule::builder().msg_type(MsgType::Signal).interface(ITEM).map_err(|e| e.to_string())?.build();
    listen(&conn, rule, tx.clone(), |m| {
        let h = m.header();
        Some(Ev::Changed(h.sender()?.to_string(), h.path()?.to_string()))
    })?;
    // Apps going away.
    let rule = zbus::MatchRule::builder()
        .msg_type(MsgType::Signal)
        .sender("org.freedesktop.DBus")
        .and_then(|b| b.interface("org.freedesktop.DBus"))
        .and_then(|b| b.member("NameOwnerChanged"))
        .map_err(|e| e.to_string())?
        .build();
    listen(&conn, rule, tx.clone(), |m| {
        let (name, _old, new): (String, String, String) = m.body().deserialize().ok()?;
        new.is_empty().then_some(Ev::Gone(name))
    })?;
    if own {
        let _ = conn.emit_signal(None::<&str>, WATCHER_PATH, WATCHER, "StatusNotifierHostRegistered", &());
    } else {
        // Another watcher: read its items, and follow them.
        let rule = zbus::MatchRule::builder().msg_type(MsgType::Signal).interface(WATCHER).map_err(|e| e.to_string())?.build();
        listen(&conn, rule, tx.clone(), |m| {
            let key: String = m.body().deserialize().ok()?;
            match m.header().member()?.as_str() {
                "StatusNotifierItemRegistered" => Some(Ev::Added(key)),
                "StatusNotifierItemUnregistered" => Some(Ev::Removed(key)),
                _ => None,
            }
        })?;
        let _ = conn.call_method(Some(WATCHER), WATCHER_PATH, Some(WATCHER), "RegisterStatusNotifierHost", &(host.as_str(),));
        if let Ok(m) = conn.call_method(Some(WATCHER), WATCHER_PATH, Some("org.freedesktop.DBus.Properties"), "Get", &(WATCHER, "RegisteredStatusNotifierItems"))
            && let Ok(v) = m.body().deserialize::<OwnedValue>()
            && let Value::Array(a) = &*v
        {
            for k in a.inner() {
                if let Value::Str(s) = k {
                    let _ = tx.send(Ev::Added(s.to_string()));
                }
            }
        }
    }
    serve(l, &conn, &rx, own, &registered);
    Ok(())
}

/// Items' bus names' owners (an item registered by a well-known name sends its signals from its
/// unique name).
fn owner(conn: &Connection, name: &str) -> String {
    if name.starts_with(':') {
        return name.to_string();
    }
    conn.call_method(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", Some("org.freedesktop.DBus"), "GetNameOwner", &(name,)).ok().and_then(|m| m.body().deserialize::<String>().ok()).unwrap_or_else(|| name.to_string())
}

fn serve(l: &TrayLive, conn: &Connection, rx: &Receiver<Ev>, own: bool, registered: &Arc<Mutex<Vec<String>>>) {
    // key -> its sender's unique name.
    let mut owners: HashMap<String, String> = HashMap::new();
    let mut stamp = 0u64;
    let unregister = |key: &str| {
        if own {
            if let Ok(mut r) = registered.lock() {
                r.retain(|k| k != key);
            }
            let _ = conn.emit_signal(None::<&str>, WATCHER_PATH, WATCHER, "StatusNotifierItemUnregistered", &(key,));
        }
    };
    while let Ok(ev) = rx.recv() {
        match ev {
            Ev::Added(key) => {
                let (name, _) = split_key(&key);
                owners.insert(key.clone(), owner(conn, name));
                refresh(l, conn, &key, &mut stamp);
            }
            Ev::Removed(key) => {
                owners.remove(&key);
                l.edit(|t| t.items.retain(|i| i.key != key));
            }
            Ev::Changed(sender, path) => {
                let keys: Vec<String> = owners.iter().filter(|(k, o)| **o == sender && split_key(k).1 == path).map(|(k, _)| k.clone()).collect();
                for k in keys {
                    refresh(l, conn, &k, &mut stamp);
                }
            }
            Ev::Gone(name) => {
                let gone: Vec<String> = owners.iter().filter(|(k, o)| **o == name || split_key(k).0 == name).map(|(k, _)| k.clone()).collect();
                for k in gone {
                    owners.remove(&k);
                    unregister(&k);
                    l.edit(|t| {
                        t.items.retain(|i| i.key != k);
                        if t.menu.as_ref().is_some_and(|m| m.0 == k) {
                            t.menu = None;
                        }
                    });
                }
            }
            Ev::Do(job) => act(l, conn, job),
        }
    }
}

// ------------------------------------------------------------------ an item

fn val_str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.to_string(),
        Value::ObjectPath(p) => p.to_string(),
        Value::Value(b) => val_str(b),
        _ => String::new(),
    }
}

fn val_bool(v: &Value) -> Option<bool> {
    match v {
        Value::Bool(b) => Some(*b),
        Value::Value(b) => val_bool(b),
        _ => None,
    }
}

fn val_i32(v: &Value) -> Option<i32> {
    match v {
        Value::I32(n) => Some(*n),
        Value::U32(n) => Some(*n as i32),
        Value::Value(b) => val_i32(b),
        _ => None,
    }
}

/// The pixmap closest to (and not under) 32 px, from `a(iiay)` (ARGB, big-endian), as RGBA.
fn pixmap(v: &Value) -> Option<Pixmap> {
    let Value::Array(a) = v else {
        return if let Value::Value(b) = v { pixmap(b) } else { None };
    };
    let mut best: Option<(i32, i32, &[Value])> = None;
    for e in a.inner() {
        let Value::Structure(s) = e else { continue };
        let [Value::I32(w), Value::I32(h), Value::Array(px)] = s.fields() else { continue };
        if *w <= 0 || *h <= 0 || px.len() != (*w * *h * 4) as usize {
            continue;
        }
        let score = |w: i32| if w >= 32 { w - 32 } else { 1000 + 32 - w };
        if best.is_none_or(|b| score(*w) < score(b.0)) {
            best = Some((*w, *h, px.inner()));
        }
    }
    let (w, h, px) = best?;
    let bytes: Vec<u8> = px.iter().map(|b| if let Value::U8(b) = b { *b } else { 0 }).collect();
    let mut rgba = Vec::with_capacity(bytes.len());
    for p in bytes.as_chunks::<4>().0 {
        rgba.extend_from_slice(&[p[1], p[2], p[3], p[0]]);
    }
    Some(Pixmap { w: w as usize, h: h as usize, rgba: Arc::new(rgba) })
}

/// An icon by name in the app's own icon folder (`IconThemePath`), the biggest there.
fn in_theme_path(dir: &Path, name: &str) -> Option<PathBuf> {
    let mut found: Vec<PathBuf> = Vec::new();
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
            } else if p.file_stem().is_some_and(|s| s == name) && p.extension().is_some_and(|x| x == "png" || x == "svg") {
                found.push(p);
            }
        }
    }
    let size = |p: &PathBuf| if p.extension().is_some_and(|x| x == "svg") { u64::MAX } else { std::fs::metadata(p).map(|m| m.len()).unwrap_or(0) };
    found.into_iter().max_by_key(size)
}

fn refresh(l: &TrayLive, conn: &Connection, key: &str, stamp: &mut u64) {
    let (name, path) = split_key(key);
    let Ok(m) = conn.call_method(Some(name), path, Some("org.freedesktop.DBus.Properties"), "GetAll", &(ITEM,)) else {
        return;
    };
    let Ok(props) = m.body().deserialize::<HashMap<String, OwnedValue>>() else { return };
    let s = |k: &str| props.get(k).map(|v| val_str(v)).unwrap_or_default();
    let mut it = TrayItem {
        key: key.to_string(),
        id: s("Id"),
        title: s("Title"),
        status: match s("Status").as_str() {
            "Passive" => Status::Passive,
            "NeedsAttention" => Status::NeedsAttention,
            _ => Status::Active,
        },
        icon_name: s("IconName"),
        attention_name: s("AttentionIconName"),
        pixmap: props.get("IconPixmap").and_then(|v| pixmap(v)),
        attention_pixmap: props.get("AttentionIconPixmap").and_then(|v| pixmap(v)),
        has_menu: { let m = s("Menu"); !m.is_empty() && m != "/" },
        is_menu: props.get("ItemIsMenu").and_then(|v| val_bool(v)).unwrap_or(false),
        ..Default::default()
    };
    if let Some(v) = props.get("ToolTip")
        && let Value::Structure(st) = &**v
        && let [_, _, title, body] = st.fields()
    {
        let (title, body) = (val_str(title), val_str(body));
        it.tip = [title, crate::notes::plain(&body)].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join("\n");
    }
    let theme_path = s("IconThemePath");
    if it.icon_name.starts_with('/') {
        it.icon_file = Some(PathBuf::from(&it.icon_name));
    } else if !theme_path.is_empty() && !it.icon_name.is_empty() {
        it.icon_file = in_theme_path(Path::new(&theme_path), &it.icon_name);
    }
    l.edit(|t| {
        let old = t.items.iter().position(|i| i.key == it.key);
        let same_icon = old.is_some_and(|o| {
            let o = &t.items[o];
            (&o.icon_name, &o.icon_file, &o.pixmap, &o.attention_name, &o.attention_pixmap) == (&it.icon_name, &it.icon_file, &it.pixmap, &it.attention_name, &it.attention_pixmap)
        });
        it.stamp = match old {
            Some(o) if same_icon => t.items[o].stamp,
            _ => {
                *stamp += 1;
                *stamp
            }
        };
        match old {
            Some(o) => t.items[o] = it,
            None => t.items.push(it),
        }
    });
}

// ------------------------------------------------------------------ doing things

fn menu_path(conn: &Connection, key: &str) -> Option<String> {
    let (name, path) = split_key(key);
    let m = conn.call_method(Some(name), path, Some("org.freedesktop.DBus.Properties"), "Get", &(ITEM, "Menu")).ok()?;
    let v: OwnedValue = m.body().deserialize().ok()?;
    let p = val_str(&v);
    (!p.is_empty() && p != "/").then_some(p)
}

/// A dbusmenu row and its children, from `(ia{sv}av)`.
fn menu_item(v: &Value) -> Option<MenuItem> {
    let s = match v {
        Value::Structure(s) => s,
        Value::Value(b) => return menu_item(b),
        _ => return None,
    };
    let [Value::I32(id), Value::Dict(props), Value::Array(kids)] = s.fields() else { return None };
    let get = |k: &str| props.iter().find(|(key, _)| matches!(key, Value::Str(s) if s.as_str() == k)).map(|(_, v)| v);
    let visible = get("visible").and_then(val_bool).unwrap_or(true);
    if !visible {
        return None;
    }
    let label = get("label").map(val_str).unwrap_or_default();
    Some(MenuItem {
        id: *id,
        label: plain_label(&label),
        enabled: get("enabled").and_then(val_bool).unwrap_or(true),
        separator: get("type").map(val_str).is_some_and(|t| t == "separator"),
        toggle: match get("toggle-type").map(val_str).as_deref() {
            Some("checkmark") => Toggle::Check,
            Some("radio") => Toggle::Radio,
            _ => Toggle::None,
        },
        checked: get("toggle-state").and_then(val_i32) == Some(1),
        icon: get("icon-name").map(val_str).unwrap_or_default(),
        children: kids.inner().iter().filter_map(menu_item).collect(),
    })
}

/// A label without its mnemonic marks ("_Quit" → "Quit", "__" → "_").
fn plain_label(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '_' {
            if it.peek() == Some(&'_') {
                out.push('_');
                it.next();
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn read_menu(conn: &Connection, key: &str) -> Option<Vec<MenuItem>> {
    let (name, _) = split_key(key);
    let path = menu_path(conn, key)?;
    let _ = conn.call_method(Some(name), path.as_str(), Some(MENU), "AboutToShow", &(0i32,));
    let m = conn.call_method(Some(name), path.as_str(), Some(MENU), "GetLayout", &(0i32, -1i32, Vec::<String>::new())).ok()?;
    #[allow(clippy::type_complexity)]
    let (_rev, (_id, _props, kids)): (u32, (i32, HashMap<String, OwnedValue>, Vec<OwnedValue>)) = m.body().deserialize().ok()?;
    Some(kids.iter().filter_map(|k| menu_item(k)).collect())
}

fn act(l: &TrayLive, conn: &Connection, job: Job) {
    let call = |key: &str, method: &str, body: &(i32, i32)| {
        let (name, path) = split_key(key);
        conn.call_method(Some(name), path, Some(ITEM), method, body).is_ok()
    };
    match job {
        Job::Activate(key) => {
            if !call(&key, "Activate", &(0, 0)) {
                call(&key, "ContextMenu", &(0, 0));
            }
        }
        Job::Secondary(key) => {
            call(&key, "SecondaryActivate", &(0, 0));
        }
        Job::ContextMenu(key) => {
            call(&key, "ContextMenu", &(0, 0));
        }
        Job::Scroll(key, delta) => {
            let (name, path) = split_key(&key);
            let _ = conn.call_method(Some(name), path, Some(ITEM), "Scroll", &(delta, "vertical"));
        }
        Job::OpenMenu(key) => {
            l.edit(|t| t.menu = Some((key.clone(), None)));
            let rows = read_menu(conn, &key);
            match rows {
                Some(rows) => l.edit(|t| {
                    if t.menu.as_ref().is_some_and(|m| m.0 == key) {
                        t.menu = Some((key.clone(), Some(rows)));
                    }
                }),
                // No menu after all: the app's own.
                None => {
                    l.edit(|t| t.menu = None);
                    call(&key, "ContextMenu", &(0, 0));
                }
            }
        }
        Job::OpenSubmenu(key, id) => {
            let (name, _) = split_key(&key);
            if let Some(path) = menu_path(conn, &key)
                && let Ok(m) = conn.call_method(Some(name), path.as_str(), Some(MENU), "AboutToShow", &(id,))
                && m.body().deserialize::<bool>().unwrap_or(false)
                && let Some(rows) = read_menu(conn, &key)
            {
                l.edit(|t| {
                    if t.menu.as_ref().is_some_and(|m| m.0 == key) {
                        t.menu = Some((key.clone(), Some(rows)));
                    }
                });
            }
        }
        Job::MenuClick(key, id) => {
            let (name, _) = split_key(&key);
            if let Some(path) = menu_path(conn, &key) {
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as u32).unwrap_or(0);
                let _ = conn.call_method(Some(name), path.as_str(), Some(MENU), "Event", &(id, "clicked", Value::from(0i32), stamp));
            }
            l.edit(|t| t.menu = None);
        }
        Job::CloseMenu => l.edit(|t| t.menu = None),
    }
}

/// Items for checks (screenshots), with drawn icons.
pub fn demo() -> Tray {
    let disc = |r: u8, g: u8, b: u8| {
        let n = 32usize;
        let mut rgba = vec![0u8; n * n * 4];
        for y in 0..n {
            for x in 0..n {
                let (dx, dy) = (x as f32 - 15.5, y as f32 - 15.5);
                let d = (dx * dx + dy * dy).sqrt();
                let a = (14.5 - d).clamp(0.0, 1.0);
                let inner = if d < 6.0 { 0.55 } else { 1.0 };
                let i = (y * n + x) * 4;
                rgba[i..i + 4].copy_from_slice(&[(r as f32 * inner) as u8, (g as f32 * inner) as u8, (b as f32 * inner) as u8, (a * 255.0) as u8]);
            }
        }
        Some(Pixmap { w: n, h: n, rgba: Arc::new(rgba) })
    };
    let item = |k: &str, title: &str, px: Option<Pixmap>, status: Status| TrayItem { key: format!(":1.{k}/StatusNotifierItem"), id: k.into(), title: title.into(), pixmap: px, status, has_menu: true, stamp: 1, ..Default::default() };
    Tray {
        items: vec![
            item("chat", "Chat", disc(0x58, 0x65, 0xf2), Status::NeedsAttention),
            item("sync", "Sync", disc(0x00, 0x82, 0xc9), Status::Active),
            item("games", "Games", disc(0x1b, 0x28, 0x38), Status::Active),
        ],
        watcher: true,
        ..Default::default()
    }
}

/// A tray item for checks from an app's real pixels, saved by `busctl --user --json=short
/// get-property … IconPixmap` (the pixmap closest to 32 px, as from the bus).
pub fn demo_from_busctl(f: &std::path::Path) -> Option<TrayItem> {
    let v: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(f).ok()?).ok()?;
    let mut best: Option<Pixmap> = None;
    for e in v["data"].as_array()? {
        let (w, h) = (e[0].as_u64()? as usize, e[1].as_u64()? as usize);
        let argb: Vec<u8> = e[2].as_array()?.iter().filter_map(|b| b.as_u64().map(|b| b as u8)).collect();
        if argb.len() != w * h * 4 {
            continue;
        }
        let rgba: Vec<u8> = argb.chunks(4).flat_map(|c| [c[1], c[2], c[3], c[0]]).collect();
        let p = Pixmap { w, h, rgba: Arc::new(rgba) };
        // As `pixmap` picks: the smallest at least 32, else the biggest.
        let better = match &best {
            None => true,
            Some(b) => (p.w >= 32 && (b.w < 32 || p.w < b.w)) || (b.w < 32 && p.w > b.w),
        };
        if better {
            best = Some(p);
        }
    }
    let k = f.file_stem()?.to_string_lossy().into_owned();
    Some(TrayItem { key: format!(":1.{k}/StatusNotifierItem"), id: k.clone(), title: k, pixmap: best, status: Status::Active, stamp: 1, ..Default::default() })
}

/// A menu for checks.
pub fn demo_menu() -> Vec<MenuItem> {
    let row = |id: i32, label: &str| MenuItem { id, label: label.into(), enabled: true, ..Default::default() };
    vec![
        row(1, "Open Chat"),
        MenuItem { toggle: Toggle::Check, checked: true, ..row(2, "Start with the computer") },
        MenuItem { children: vec![row(10, "Online"), row(11, "Away")], ..row(3, "Status") },
        MenuItem { separator: true, ..row(4, "") },
        MenuItem { enabled: false, ..row(5, "Check for updates") },
        row(6, "Quit"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_labels() {
        assert_eq!(item_key("/org/ayatana/NotificationItem/x", ":1.5"), ":1.5/org/ayatana/NotificationItem/x");
        assert_eq!(item_key("org.kde.StatusNotifierItem-12-1", ":1.5"), "org.kde.StatusNotifierItem-12-1/StatusNotifierItem");
        assert_eq!(item_key(":1.9/StatusNotifierItem", ":1.5"), ":1.9/StatusNotifierItem");
        assert_eq!(split_key(":1.5/org/x"), (":1.5", "/org/x"));
        assert_eq!(plain_label("_Quit"), "Quit");
        assert_eq!(plain_label("Save__As"), "Save_As");
    }

    /// Against a fake app (tests/fake_sni.py) on a private bus, from apps/rivet-shell:
    /// `TRAY_FAKE=tests/fake_sni.py TRAY_LOG=/tmp/sni.log dbus-run-session -- cargo test
    /// tray::tests::live -- --ignored`.
    #[test]
    #[ignore]
    fn live() {
        let (fake, log) = (std::env::var("TRAY_FAKE").unwrap(), std::env::var("TRAY_LOG").unwrap());
        let l = TrayLive::new(|| {});
        l.start();
        let mut child = std::process::Command::new("python3").arg(&fake).arg(&log).spawn().unwrap();
        let wait = |what: &str, f: &dyn Fn(&Tray) -> bool| {
            for _ in 0..100 {
                if f(&l.get()) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            panic!("timed out: {what} ({:?})", l.get());
        };
        wait("item", &|t| t.watcher && t.items.len() == 1);
        let it = l.get().items[0].clone();
        assert_eq!((it.title.as_str(), it.id.as_str(), it.has_menu), ("Fake App", "fake", true));
        assert_eq!(it.tip, "Fake App\nSome text");
        let px = it.pixmap.as_ref().unwrap();
        assert_eq!((px.w, &px.rgba[..4]), (2, &[255u8, 0, 0, 255][..]));
        l.run(Job::OpenMenu(it.key.clone()));
        wait("menu", &|t| matches!(&t.menu, Some((_, Some(_)))));
        let rows = l.get().menu.unwrap().1.unwrap();
        assert_eq!(rows.iter().map(|r| r.label.as_str()).collect::<Vec<_>>(), ["Open", "", "Sub", "Check"]);
        assert!(rows[1].separator && rows[3].checked && rows[3].toggle == Toggle::Check);
        assert_eq!(rows[2].children[0].label, "Inner");
        l.run(Job::MenuClick(it.key.clone(), 1));
        l.run(Job::Activate(it.key.clone()));
        // Its title changes (and it says so).
        l.run(Job::Secondary(it.key.clone()));
        wait("renamed", &|t| t.items.first().is_some_and(|i| i.title == "Renamed"));
        // A second tray, as when another program is the watcher: it reads the same items.
        let other = TrayLive::new(|| {});
        other.start();
        for _ in 0..100 {
            if other.get().items.len() == 1 {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let o = other.get();
        assert!(!o.watcher && o.items.len() == 1 && o.items[0].title == "Renamed", "{o:?}");
        // It quits: gone.
        child.kill().unwrap();
        child.wait().unwrap();
        wait("gone", &|t| t.items.is_empty());
        for _ in 0..100 {
            if other.get().items.is_empty() {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(other.get().items.is_empty());
        let text = std::fs::read_to_string(&log).unwrap();
        assert!(text.contains("menu Event (1, 'clicked'"), "{text}");
        assert!(text.contains("item Activate"), "{text}");
    }
}
