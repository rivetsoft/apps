//! The Rivet FTP window: saved connections on the left, this computer and the server side by
//! side, and the transfer queue underneath. It uses Rivet's look (theme, desktop colours, icons
//! and fonts) from the shared crates.

mod dialogs;
mod draw;
mod edit;
mod logic;
mod menus;
mod prefs;
mod tabs;
mod preview;
mod toolbar;

pub use prefs::ICON_SVG;

/// The desktop font's file, read on a thread while the window opens: asking fontconfig takes
/// tens of milliseconds, which the first frame would otherwise wait for.
static FONT_LOOKUP: std::sync::Mutex<Option<std::thread::JoinHandle<Option<Vec<u8>>>>> = std::sync::Mutex::new(None);

pub fn start_font_lookup() {
    let job = std::thread::spawn(|| {
        // Fills the cache the text size is read from, too.
        let _ = rivet_look::fonts::desktop_font_cached();
        let (_, path) = rivet_look::fonts::resolve("")?;
        std::fs::read(path).ok()
    });
    *FONT_LOOKUP.lock().unwrap_or_else(|e| e.into_inner()) = Some(job);
}

/// The font file's bytes: from the lookup started early, or looked up now.
fn desktop_font_bytes() -> Option<Vec<u8>> {
    let job = FONT_LOOKUP.lock().unwrap_or_else(|e| e.into_inner()).take();
    match job {
        Some(j) => j.join().ok().flatten(),
        None => std::fs::read(rivet_look::fonts::resolve("")?.1).ok(),
    }
}

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::time::SystemTime;

use eframe::egui::{self, Pos2, Rect, Vec2};
use rivet_connections::{Connection, Protocol, Store};
use rivet_look::icons::IconTheme;
use rivet_look::{palette, windows};
use rivet_xdg::mime::MimeDb;

use crate::listing::Entry;
use crate::settings::Settings;
use crate::sftp::Session;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Side {
    Local,
    Remote,
}

impl Side {
    fn other(self) -> Side {
        match self {
            Side::Local => Side::Remote,
            Side::Remote => Side::Local,
        }
    }
}

#[derive(Default)]
struct Pane {
    path: String,
    /// Shared, so drawing a frame can hold the list without copying it.
    entries: std::sync::Arc<Vec<Entry>>,
    selected: BTreeSet<String>,
    anchor: Option<usize>,
    cursor: Option<usize>,
    loading: bool,
    error: Option<String>,
    /// The path being typed (the path bar in edit mode).
    editing: Option<String>,
    scroll_to_cursor: bool,
    sort: crate::listing::Sort,
    /// A local folder being read on a thread: (folder, order, what was read). A newer reload
    /// replaces it, so an older answer is never shown.
    listing_rx: Option<std::sync::mpsc::Receiver<LocalListing>>,
    /// When that read began: "Loading…" only shows once it takes long enough to notice.
    loading_since: Option<std::time::Instant>,
    /// Select this name once the listing being read arrives (a new folder, a renamed file).
    reselect: Option<String>,
}

type LocalListing = (String, crate::listing::Sort, Result<Vec<Entry>, String>);

impl Pane {
    fn selected_entries(&self) -> Vec<Entry> {
        self.entries.iter().filter(|e| self.selected.contains(&e.name)).cloned().collect()
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Dir {
    Up,
    Down,
}

#[derive(Clone, PartialEq)]
enum JobState {
    Waiting,
    Done,
    Failed(String),
}

struct Job {
    id: u64,
    dir: Dir,
    name: String,
    /// Where it lands, shown in the queue.
    to: String,
    /// Folder it lands in (to refresh that pane when it's showing it).
    to_dir: String,
    /// Where it comes from (to send it again).
    from: String,
    is_dir: bool,
    size: u64,
    /// For downloads: the local file, so progress can be read from its size.
    local_target: Option<PathBuf>,
    state: JobState,
    /// When it started moving, and how long it took once done.
    started: Option<std::time::Instant>,
    took: Option<std::time::Duration>,
    /// Recent (time, bytes so far) readings, for the speed.
    samples: std::collections::VecDeque<(std::time::Instant, u64)>,
}

/// Items waiting for "replace?" before they're sent.
struct Pending {
    dir: Dir,
    items: Vec<Entry>,
    from_dir: String,
    to_dir: String,
    clashes: Vec<String>,
    /// What's already there under those names (to compare).
    existing: Vec<Entry>,
}

/// The connection editor.
struct Form {
    /// The name being edited (None: a new connection).
    original: Option<String>,
    name: String,
    protocol: Protocol,
    host: String,
    port: String,
    user: String,
    key: String,
    remote: String,
    local: String,
    category: String,
    error: Option<String>,
    /// The name field gets the keyboard once, when the editor opens.
    focused_once: bool,
    /// The key is a file typed by hand (not one from ~/.ssh).
    key_other: bool,
    /// A new password to keep in the keyring (empty: leave it as it is).
    password: String,
    /// A password is in the keyring for this connection.
    has_saved: bool,
    /// Forget the saved password and passphrase.
    forget: bool,
}

impl Form {
    fn blank() -> Form {
        Form { original: None, name: String::new(), protocol: Protocol::Sftp, host: String::new(), port: String::new(), user: String::new(), key: String::new(), remote: String::new(), local: String::new(), category: String::new(), error: None, focused_once: false, key_other: false, password: String::new(), has_saved: false, forget: false }
    }
    fn from(c: &Connection) -> Form {
        Form {
            original: Some(c.name.clone()),
            name: c.name.clone(),
            protocol: c.protocol,
            host: c.host.clone(),
            port: c.port.map(|p| p.to_string()).unwrap_or_default(),
            user: c.user.clone().unwrap_or_default(),
            key: c.key.clone().unwrap_or_default(),
            remote: c.remote.clone().unwrap_or_default(),
            local: c.local.clone().unwrap_or_default(),
            category: c.category.clone().unwrap_or_default(),
            error: None,
            focused_once: false,
            key_other: false,
            password: String::new(),
            has_saved: false,
            forget: false,
        }
    }
    fn build(&self) -> Result<Connection, String> {
        let opt = |s: &str| Some(s.trim().to_string()).filter(|s| !s.is_empty());
        if self.name.trim().is_empty() {
            return Err("Give the connection a name.".into());
        }
        if self.host.trim().is_empty() {
            return Err("Enter the server's address.".into());
        }
        let port = match self.port.trim() {
            "" => None,
            p => Some(p.parse::<u16>().map_err(|_| format!("\"{p}\" isn't a port number."))?),
        };
        Ok(Connection {
            name: self.name.trim().to_string(),
            protocol: self.protocol,
            host: self.host.trim().to_string(),
            port,
            user: opt(&self.user),
            key: if self.protocol == Protocol::Sftp { opt(&self.key) } else { None },
            remote: opt(&self.remote),
            local: opt(&self.local),
            note: None,
            category: opt(&self.category),
        })
    }
}

/// The New SSH Key dialog.
struct NewKey {
    name: String,
    kind: rivet_connections::keys::KeyType,
    comment: String,
    pass: String,
    pass2: String,
    error: Option<String>,
    /// Opened from the connection editor: use the new key there.
    for_form: bool,
    focused_once: bool,
}

impl NewKey {
    fn new(for_form: bool) -> NewKey {
        use rivet_connections::keys;
        let kind = keys::KeyType::Ed25519;
        NewKey { name: keys::suggest_name(&keys::ssh_dir(), kind), kind, comment: keys::default_comment(), pass: String::new(), pass2: String::new(), error: None, for_form, focused_once: false }
    }
}

enum Prompt {
    NewFolder(Side, String),
    Rename(Side, String, String),
    Delete(Vec<Entry>),
    Replace(Pending),
    /// An edited file changed on the server since it was opened (index into `edits`).
    EditConflict(usize),
    /// The server wants a password or the key's passphrase.
    Secret(SecretAsk),
    /// Details and permissions of a file or folder.
    Info(InfoState),
    /// Put a connection in a new category: (connection, name being typed).
    NewCategory(String, String),
    /// Rename a category: (old name, new name being typed).
    RenameCategory(String, String),
}

pub(super) struct SecretAsk {
    conn: Connection,
    kind: rivet_connections::secrets::Kind,
    text: String,
    remember: bool,
    /// The last one given was refused.
    wrong: bool,
    focused_once: bool,
}

pub(super) struct InfoState {
    side: Side,
    entry: Entry,
    /// Where it is: a path, or the server address.
    location: String,
    mode: u32,
    recursive: bool,
}

/// What's being dragged between the panes.
#[derive(Clone)]
struct DragItems {
    from: Side,
    items: Vec<Entry>,
}

pub struct FtpApp {
    /// For work on other threads to wake the window when it's done.
    ctx: egui::Context,
    store: Store,
    mime: MimeDb,
    icons: IconTheme,
    palette: Option<palette::Palette>,
    look_key: Option<String>,
    font_set: bool,

    local: Pane,
    remote: Pane,
    focus: Side,
    s: Settings,
    /// The server's starting folder (Go → Server Home).
    remote_home: Option<String>,
    /// Put the keyboard in the quick-connect field next frame.
    focus_quick: bool,

    prefs_open: bool,
    prefs_page: prefs::Page,
    prefs_win: windows::WinState,
    keys_open: bool,
    keys_win: windows::WinState,
    app_icon: Option<egui::TextureHandle>,
    /// Desktop color files and when they last changed (checked every couple of seconds).
    palette_watch: Vec<(PathBuf, Option<SystemTime>)>,
    palette_checked: f64,

    conn: Option<Connection>,
    browse: Option<Session>,
    transfer: Option<Session>,
    connecting: bool,
    /// Browse requests in flight: id → what it does (to report errors and refresh).
    pending: HashMap<u64, String>,
    list_req: Option<u64>,

    queue: Vec<Job>,
    quick: String,
    /// The message beside the queue heading: what just happened, or what went wrong.
    note: Option<(String, bool)>,
    form: Option<Form>,
    prompt: Option<Prompt>,
    shot: Option<rivet_kit::Shot>,
    /// Test hook RIVET_FTP_SEND="up:a,b;down:c": transfers to start once the server folder is listed.
    hook_send: Option<String>,

    /// SSH keys in ~/.ssh (read in the background when first needed).
    keys: Vec<rivet_connections::keys::SshKey>,
    keys_rx: Option<std::sync::mpsc::Receiver<Vec<rivet_connections::keys::SshKey>>>,
    keys_asked: bool,
    new_key: Option<NewKey>,

    /// Server files open in editors.
    edits: Vec<edit::EditSession>,
    /// Every tab; the open one's state is in the fields above (its slot holds a blank).
    tabs: Vec<tabs::ConnTab>,
    active: usize,
    /// A background tab is swapped in (for its replies only).
    in_background: bool,
    /// Test hook RIVET_FTP_TABS=NAME,NAME: open these, each in a tab.
    hook_tabs: Option<String>,
    /// The light box, when open.
    preview: Option<preview::PreviewState>,
    /// Test hook RIVET_FTP_PREVIEW=local:NAME or remote:NAME.
    hook_preview: Option<String>,
    apps: Option<Vec<rivet_xdg::launch::DesktopApp>>,
    /// The installed apps, being read on a thread since startup.
    apps_rx: Option<std::sync::mpsc::Receiver<Vec<rivet_xdg::launch::DesktopApp>>>,
    /// Test hook RIVET_FTP_EDIT=NAME: edit this server file once the folder is listed.
    hook_edit: Option<String>,
    /// Test hook RIVET_FTP_INFO=NAME: open Info for this server file once listed.
    hook_info: Option<String>,

    /// The password or passphrase the current connection logged in with.
    secret: Option<crate::sftp::Secret>,
    /// Keep these in the keyring once the login works: (connection, kind, secret).
    remember: Vec<(String, rivet_connections::secrets::Kind, String)>,
    shot_wait: u32,
    /// Time spent building each frame, and when the first was ready (for RIVET_FRAME_STATS).
    frame_times: Vec<f64>,
    first_frame_ms: Option<f64>,
}

impl eframe::App for FtpApp {
    /// Test hook for checks: RIVET_FTP_RIGHTCLICK=X,Y right-clicks there once the window has
    /// settled (screenshot runs only).
    fn raw_input_hook(&mut self, _ctx: &egui::Context, raw: &mut egui::RawInput) {
        let Some(shot) = &self.shot else { return };
        // RIVET_FTP_CLICK=X,Y does the same with the left button.
        let (var, button) = if std::env::var_os("RIVET_FTP_CLICK").is_some() { ("RIVET_FTP_CLICK", egui::PointerButton::Primary) } else { ("RIVET_FTP_RIGHTCLICK", egui::PointerButton::Secondary) };
        let Some((x, y)) = std::env::var(var).ok().and_then(|v| {
            let (a, b) = v.split_once(',')?;
            Some((a.trim().parse::<f32>().ok()?, b.trim().parse::<f32>().ok()?))
        }) else {
            return;
        };
        let pos = Pos2::new(x, y);
        // RIVET_FTP_HOVER: only move the pointer there (to show a tooltip).
        if std::env::var_os("RIVET_FTP_HOVER").is_some() {
            if shot.frames() == 30 {
                raw.events.push(egui::Event::PointerMoved(pos));
            }
            return;
        }
        match shot.frames() {
            30 => raw.events.push(egui::Event::PointerMoved(pos)),
            31 => raw.events.push(egui::Event::PointerButton { pos, button, pressed: true, modifiers: Default::default() }),
            32 => raw.events.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers: Default::default() }),
            _ => {}
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let frame_start = std::time::Instant::now();
        self.watch_palette(&ctx);
        self.apply_look(&ctx);
        self.poll(&ctx);
        self.poll_local();
        self.poll_background(&ctx);
        if let Some(list) = self.hook_tabs.take() {
            for name in list.split(',') {
                if let Some(c) = self.store.find(name.trim()).cloned() {
                    self.open_connection(c, &ctx, false);
                }
            }
        }
        self.shortcuts(&ctx);
        self.keys(&ctx);
        let v = ctx.global_style().visuals.clone();
        if self.s.show_menubar {
            let bar = egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin::symmetric(6, 2));
            egui::Panel::top("menubar").show_separator_line(false).frame(bar).show(ui, |ui| {
                egui::MenuBar::new().ui(ui, |ui| self.menus(ui));
            });
        }
        let chrome = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 6, bottom: 8 });
        egui::Panel::top("toolbar").show_separator_line(false).frame(chrome).show(ui, |ui| self.toolbar(ui));
        let side = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 10, right: 8, top: 0, bottom: 8 });
        if self.s.show_sidebar {
            egui::Panel::left("sites").show_separator_line(false).frame(side).resizable(true).default_size(220.0).size_range(170.0..=360.0).show(ui, |ui| self.sidebar(ui));
        }
        if self.s.show_queue {
            let q = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 2, bottom: 8 });
            egui::Panel::bottom("queue").show_separator_line(false).frame(q).resizable(true).default_size(150.0).size_range(90.0..=400.0).show(ui, |ui| self.queue_panel(ui));
        } else {
            let st = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 12, right: 12, top: 4, bottom: 6 });
            egui::Panel::bottom("status").show_separator_line(false).frame(st).show(ui, |ui| self.status_line(ui));
        }
        let left_pad = if self.s.show_sidebar { 0 } else { 12 };
        // The tabs sit above the panes (beside the sidebar, not over it).
        if self.tab_count() > 1 {
            let bar = egui::Frame::NONE.fill(v.panel_fill).inner_margin(egui::Margin { left: left_pad, right: 12, top: 0, bottom: 6 });
            egui::Panel::top("tabs").show_separator_line(false).frame(bar).show(ui, |ui| self.tab_bar(ui));
        }
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: left_pad, right: 12, top: 0, bottom: 0 });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            let r = ui.max_rect();
            let gap = 10.0;
            let w = (r.width() - gap) / 2.0;
            let left = Rect::from_min_size(r.min, Vec2::new(w, r.height()));
            let right = Rect::from_min_size(Pos2::new(left.right() + gap, r.top()), Vec2::new(w, r.height()));
            self.draw_pane(ui, Side::Local, left);
            self.draw_pane(ui, Side::Remote, right);
        });
        self.watch_edits(&ctx);
        self.windows(&ctx);
        self.dialogs(&ctx);
        self.preview_window(&ctx);
        if let Some(shot) = self.shot.as_mut() {
            // Checks: wait for logging in and listing to finish first (up to about 10 s).
            let editing = self.hook_edit.is_some() || self.edits.iter().any(|e| e.saved_at.is_none() && !matches!(e.state, edit::EditState::Failed(_)));
            let previewing = self.preview.as_ref().is_some_and(|p| !p.lb.current().ready) || self.hook_preview.is_some();
            let busy = previewing || self.connecting || self.local.loading || self.remote.loading || !self.pending.is_empty() || self.hook_send.is_some() || self.queue.iter().any(|j| j.state == JobState::Waiting) || editing;
            if busy && self.shot_wait < 800 {
                self.shot_wait += 1;
                ctx.request_repaint();
            } else {
                shot.step(&ctx);
            }
            if std::env::var_os("RIVET_FRAME_STATS").is_some() {
                self.frame_times.push(frame_start.elapsed().as_secs_f64() * 1000.0);
                self.first_frame_ms.get_or_insert_with(|| crate::STARTED.get().map(|t| t.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0));
                if shot.frames() == shot.after {
                    print_frame_stats(&self.frame_times, self.first_frame_ms);
                }
            }
        }
    }
}

/// Checks: how long frames took in a screenshot run. The first ten are left out of the median,
/// since fonts, icons and the folders load then.
fn print_frame_stats(times: &[f64], first: Option<f64>) {
    if let Some(f) = first {
        println!("first frame ready {f:.0} ms after start");
    }
    if times.len() <= 10 {
        return;
    }
    let mut t = times[10..].to_vec();
    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let head: f64 = times[..10].iter().sum();
    println!("first 10 frames: {head:.1} ms total; afterwards median {:.2} ms, 95th percentile {:.2} ms per frame", t[t.len() / 2], t[t.len() * 95 / 100]);
}
