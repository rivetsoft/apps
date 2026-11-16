//! The Rivet FTP window: saved connections on the left, this computer and the server side by
//! side, and the transfer queue underneath. It uses Rivet's look (theme, desktop colours, icons
//! and fonts) from the shared crates.

mod dialogs;
mod draw;
mod logic;

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use rivet_connections::{Connection, Protocol, Store};
use rivet_look::icons::IconTheme;
use rivet_look::palette;
use rivet_xdg::mime::MimeDb;

use crate::listing::Entry;
use crate::sftp::Session;

const DEFAULT_ACCENT: Color32 = Color32::from_rgb(0x35, 0x84, 0xe4);

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
    entries: Vec<Entry>,
    selected: BTreeSet<String>,
    anchor: Option<usize>,
    cursor: Option<usize>,
    loading: bool,
    error: Option<String>,
    /// The path being typed (the path bar in edit mode).
    editing: Option<String>,
    scroll_to_cursor: bool,
}

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
    is_dir: bool,
    size: u64,
    /// For downloads: the local file, so progress can be read from its size.
    local_target: Option<PathBuf>,
    state: JobState,
}

/// Items waiting for "replace?" before they're sent.
struct Pending {
    dir: Dir,
    items: Vec<Entry>,
    from_dir: String,
    to_dir: String,
    clashes: Vec<String>,
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
    error: Option<String>,
}

impl Form {
    fn blank() -> Form {
        Form { original: None, name: String::new(), protocol: Protocol::Sftp, host: String::new(), port: String::new(), user: String::new(), key: String::new(), remote: String::new(), local: String::new(), error: None }
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
            error: None,
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
        })
    }
}

enum Prompt {
    NewFolder(Side, String),
    Rename(Side, String, String),
    Delete(Vec<Entry>),
    Replace(Pending),
}

/// What's being dragged between the panes.
#[derive(Clone)]
struct DragItems {
    from: Side,
    items: Vec<Entry>,
}

pub struct FtpApp {
    store: Store,
    mime: MimeDb,
    icons: IconTheme,
    palette: Option<palette::Palette>,
    look_key: Option<String>,
    font_set: bool,

    local: Pane,
    remote: Pane,
    focus: Side,
    show_hidden: bool,

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
    shot_wait: u32,
}

impl eframe::App for FtpApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.apply_look(&ctx);
        self.poll(&ctx);
        self.keys(&ctx);
        let v = ctx.global_style().visuals.clone();
        let chrome = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin::symmetric(12, 8));
        egui::Panel::top("toolbar").frame(chrome).show(ui, |ui| self.toolbar(ui));
        egui::Panel::left("sites").frame(chrome).resizable(true).default_size(220.0).show(ui, |ui| self.sidebar(ui));
        egui::Panel::bottom("queue").frame(chrome).resizable(true).default_size(150.0).show(ui, |ui| self.queue_panel(ui));
        let main = egui::Frame::new().fill(v.panel_fill).inner_margin(egui::Margin { left: 0, right: 12, top: 0, bottom: 0 });
        egui::CentralPanel::default().frame(main).show(ui, |ui| {
            let r = ui.max_rect();
            let gap = 10.0;
            let w = (r.width() - gap) / 2.0;
            let left = Rect::from_min_size(r.min, Vec2::new(w, r.height()));
            let right = Rect::from_min_size(Pos2::new(left.right() + gap, r.top()), Vec2::new(w, r.height()));
            self.draw_pane(ui, Side::Local, left);
            self.draw_pane(ui, Side::Remote, right);
        });
        self.dialogs(&ctx);
        if let Some(shot) = self.shot.as_mut() {
            // Checks: wait for logging in and listing to finish first (up to about 10 s).
            let busy = self.connecting || self.remote.loading || !self.pending.is_empty() || self.hook_send.is_some() || self.queue.iter().any(|j| j.state == JobState::Waiting);
            if busy && self.shot_wait < 800 {
                self.shot_wait += 1;
                ctx.request_repaint();
            } else {
                shot.step(&ctx);
            }
        }
    }
}
