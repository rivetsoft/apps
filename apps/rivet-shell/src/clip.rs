//! Clipboard history: what is copied in the session, kept so it can be put back on the clipboard.
//!
//! A thread of its own holds a second Wayland connection and reads the clipboard through the
//! data-control protocol (ext-data-control-v1, else the older zwlr-data-control-v1), so no
//! frame ever waits on another app handing its data over. Where the compositor has neither,
//! the history stays unavailable and everything that shows it is left out.
//!
//! What a password manager marks secret (the `x-kde-passwordManagerHint` type saying
//! `secret`) is never read, and a copy larger than its cap is skipped. The history can be
//! saved for the next session in the data folder, readable only by its owner.

use std::io::{Read, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use calloop::channel::{Channel, Event as ChanEvent, Sender, channel};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, event_created_child};
use wayland_protocols::ext::data_control::v1::client as ext;
use wayland_protocols_wlr::data_control::v1::client as wlr;

/// The types a copy's text is read as, best first.
const TEXT_TYPES: [&str; 5] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain", "STRING", "TEXT"];
/// The pictures the history keeps (the ones it can make a thumbnail of).
const IMAGE_TYPES: [&str; 3] = ["image/png", "image/jpeg", "image/webp"];
/// Offered by password managers (KeePassXC, KDE's): its content is `secret` for a password.
const SECRET_HINT: &str = "x-kde-passwordManagerHint";
/// The most one copy may hold; a larger one is skipped.
const MAX_TEXT: usize = 1 << 20;
const MAX_IMAGE: usize = 16 << 20;
/// The most the pictures that aren't pinned may hold together: past it the oldest go.
const IMAGE_BUDGET: usize = 64 << 20;
/// How long an app gets to hand over a copy.
const READ_TIME: Duration = Duration::from_secs(5);
/// A thumbnail's largest size, in pixels (popup rows show it at about half).
const THUMB: (u32, u32) = (160, 100);
pub const LENGTH: std::ops::RangeInclusive<u32> = 10..=500;

/// A picture in the history: its bytes as copied, its size and a small RGBA thumbnail.
#[derive(PartialEq)]
pub struct Picture {
    pub mime: String,
    pub bytes: Vec<u8>,
    pub size: (u32, u32),
    pub thumb: (u32, u32, Vec<u8>),
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Picture({}, {}x{}, {} bytes)", self.mime, self.size.0, self.size.1, self.bytes.len())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Body {
    Text(Arc<str>),
    Image(Arc<Picture>),
}

impl Body {
    fn bytes(&self) -> &[u8] {
        match self {
            Body::Text(t) => t.as_bytes(),
            Body::Image(p) => &p.bytes,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub id: u64,
    /// When it was copied (or last put back), in seconds since 1970.
    pub at: i64,
    pub pinned: bool,
    pub body: Body,
}

impl Clip {
    /// One line of it for a list: its text with the runs of space and new lines made one
    /// space, or what the picture is.
    pub fn line(&self, max_chars: usize) -> String {
        match &self.body {
            Body::Text(t) => {
                let mut out = String::new();
                for w in t.split_whitespace() {
                    if out.chars().count() >= max_chars {
                        break;
                    }
                    if !out.is_empty() {
                        out.push(' ');
                    }
                    out.push_str(w);
                }
                if out.chars().count() > max_chars {
                    out = out.chars().take(max_chars).collect();
                }
                out
            }
            Body::Image(p) => format!("Picture, {} × {}", p.size.0, p.size.1),
        }
    }
}

/// The history as the list shows it: pinned copies first, each part newest first.
pub fn shown(list: &[Clip]) -> Vec<&Clip> {
    list.iter().filter(|c| c.pinned).chain(list.iter().filter(|c| !c.pinned)).collect()
}

/// What can be asked of the history.
#[derive(Clone, Debug, PartialEq)]
pub enum Job {
    /// Recording on or off (off forgets it all), how many copies to keep, and whether they
    /// are saved for the next session.
    Set { on: bool, length: u32, keep: bool },
    /// Put a copy back on the clipboard.
    Restore(u64),
    Pin(u64, bool),
    Remove(u64),
    /// Everything that isn't pinned.
    Clear,
}

#[derive(Default)]
struct Shared {
    /// None until the worker knows whether the compositor can share its clipboard.
    able: Option<bool>,
    on: bool,
    list: Vec<Clip>,
}

/// The history, as the shell's frames read it.
pub struct ClipLive {
    shared: Arc<Mutex<Shared>>,
    generation: Arc<AtomicU64>,
    tx: Option<Sender<Job>>,
}

impl ClipLive {
    /// Starts the worker with the settings it begins with; `wake` is called after each change.
    pub fn start(on: bool, length: u32, keep: bool, wake: impl Fn() + Send + 'static) -> ClipLive {
        let shared = Arc::new(Mutex::new(Shared { on, ..Default::default() }));
        let generation = Arc::new(AtomicU64::new(0));
        let (tx, rx) = channel();
        let (sh, gen_) = (shared.clone(), generation.clone());
        let started = std::thread::Builder::new().name("rivet-shell clipboard".into()).spawn(move || {
            let notify = move || {
                gen_.fetch_add(1, Ordering::Relaxed);
                wake();
            };
            if let Err(e) = Worker::run(sh.clone(), rx, Job::Set { on, length, keep }, &notify) {
                eprintln!("rivet-shell: clipboard history: {e}");
            }
            if let Ok(mut s) = sh.lock() {
                s.able = Some(false);
            }
            notify();
        });
        ClipLive { shared, generation, tx: started.is_ok().then_some(tx) }
    }

    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// The history, while it can be had and is on (None: leave it out).
    pub fn get(&self) -> Option<Vec<Clip>> {
        let s = self.shared.lock().ok()?;
        (s.able == Some(true) && s.on).then(|| s.list.clone())
    }

    pub fn run(&self, job: Job) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(job);
        }
    }
}

/// A tidy history for screenshots: a few kinds of text, a picture and a pinned copy.
pub fn sample() -> Vec<Clip> {
    let t = now();
    let w = 320;
    let img = image::RgbaImage::from_fn(w, 200, |x, y| {
        // A sky over a hill.
        let hill = 140.0 - 30.0 * ((x as f32 / w as f32) * std::f32::consts::PI).sin();
        if (y as f32) > hill { image::Rgba([0x4c, 0x8c, 0x5a, 255]) } else { image::Rgba([0x6a + (y / 4) as u8, 0x9c + (y / 5) as u8, 0xe8, 255]) }
    });
    let mut png = Vec::new();
    let _ = img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png);
    let text = |id, ago: i64, pinned, s: &str| Clip { id, at: t - ago, pinned, body: Body::Text(s.into()) };
    let mut list = vec![
        text(1, 20, false, "cargo build -p rivet-shell --release"),
        text(2, 140, false, "https://rivetsoft.github.io/docs/shell/"),
        text(4, 1500, false, "The panel's items can be dragged between zones, and each zone keeps its own order when the panel moves to another edge."),
        text(5, 3 * 3600, false, "1600 Pennsylvania Avenue NW, Washington, DC 20500"),
        text(6, 26 * 3600, true, "sam.rivers@example.com"),
    ];
    if let Some(p) = picture("image/png", png) {
        list.insert(2, Clip { id: 3, at: t - 600, pinned: false, body: Body::Image(Arc::new(p)) });
    }
    list
}

// ---- the compositor's side: the two forms of the protocol, behind one face

#[derive(Clone)]
enum Manager {
    Ext(ext::ext_data_control_manager_v1::ExtDataControlManagerV1),
    Wlr(wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1),
}

enum Device {
    Ext(ext::ext_data_control_device_v1::ExtDataControlDeviceV1),
    Wlr(wlr::zwlr_data_control_device_v1::ZwlrDataControlDeviceV1),
}

#[derive(Clone)]
enum Offer {
    Ext(ext::ext_data_control_offer_v1::ExtDataControlOfferV1),
    Wlr(wlr::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1),
}

enum Source {
    Ext(ext::ext_data_control_source_v1::ExtDataControlSourceV1),
    Wlr(wlr::zwlr_data_control_source_v1::ZwlrDataControlSourceV1),
}

/// An offer's types, as it announces them.
type Types = Mutex<Vec<String>>;
/// What a source of ours hands over, whatever type is asked for.
type Payload = Arc<Body>;

impl Manager {
    fn device(&self, seat: &WlSeat, qh: &QueueHandle<Worker>) -> Device {
        match self {
            Manager::Ext(m) => Device::Ext(m.get_data_device(seat, qh, ())),
            Manager::Wlr(m) => Device::Wlr(m.get_data_device(seat, qh, ())),
        }
    }
    fn source(&self, qh: &QueueHandle<Worker>, payload: Payload) -> Source {
        match self {
            Manager::Ext(m) => Source::Ext(m.create_data_source(qh, payload)),
            Manager::Wlr(m) => Source::Wlr(m.create_data_source(qh, payload)),
        }
    }
}

impl Device {
    fn set_selection(&self, src: &Source) {
        match (self, src) {
            (Device::Ext(d), Source::Ext(s)) => d.set_selection(Some(s)),
            (Device::Wlr(d), Source::Wlr(s)) => d.set_selection(Some(s)),
            _ => {}
        }
    }
    fn destroy(&self) {
        match self {
            Device::Ext(d) => d.destroy(),
            Device::Wlr(d) => d.destroy(),
        }
    }
}

impl Offer {
    fn types(&self) -> Vec<String> {
        let t = match self {
            Offer::Ext(o) => o.data::<Types>(),
            Offer::Wlr(o) => o.data::<Types>(),
        };
        t.and_then(|t| t.lock().ok().map(|t| t.clone())).unwrap_or_default()
    }
    fn receive(&self, mime: &str, fd: &impl AsFd) {
        match self {
            Offer::Ext(o) => o.receive(mime.to_string(), fd.as_fd()),
            Offer::Wlr(o) => o.receive(mime.to_string(), fd.as_fd()),
        }
    }
    fn destroy(&self) {
        match self {
            Offer::Ext(o) => o.destroy(),
            Offer::Wlr(o) => o.destroy(),
        }
    }
}

impl Source {
    fn offer(&self, mime: &str) {
        match self {
            Source::Ext(s) => s.offer(mime.to_string()),
            Source::Wlr(s) => s.offer(mime.to_string()),
        }
    }
    fn id(&self) -> wayland_client::backend::ObjectId {
        match self {
            Source::Ext(s) => s.id(),
            Source::Wlr(s) => s.id(),
        }
    }
}

/// The same handling for both forms of the protocol: `$p` is its module, the rest are its
/// interfaces' modules and types.
macro_rules! dispatch {
    ($p:ident, $Variant:ident, $mgr_m:ident, $Mgr:ident, $dev_m:ident, $Dev:ident, $off_m:ident, $Off:ident, $src_m:ident, $Src:ident) => {
        impl Dispatch<$p::$mgr_m::$Mgr, ()> for Worker {
            fn event(_: &mut Self, _: &$p::$mgr_m::$Mgr, _: $p::$mgr_m::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
        }

        impl Dispatch<$p::$dev_m::$Dev, ()> for Worker {
            fn event(w: &mut Self, _: &$p::$dev_m::$Dev, ev: $p::$dev_m::Event, _: &(), conn: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $p::$dev_m::Event::Selection { id } => w.selection(id.map(Offer::$Variant), conn),
                    // Only the clipboard is kept, not the text selected with the mouse.
                    $p::$dev_m::Event::PrimarySelection { id: Some(o) } => o.destroy(),
                    $p::$dev_m::Event::Finished => {
                        if let Some(d) = w.device.take() {
                            d.destroy();
                        }
                    }
                    _ => {}
                }
            }

            event_created_child!(Worker, $p::$dev_m::$Dev, [
                $p::$dev_m::EVT_DATA_OFFER_OPCODE => ($p::$off_m::$Off, Types::default())
            ]);
        }

        impl Dispatch<$p::$off_m::$Off, Types> for Worker {
            fn event(_: &mut Self, _: &$p::$off_m::$Off, ev: $p::$off_m::Event, types: &Types, _: &Connection, _: &QueueHandle<Self>) {
                if let $p::$off_m::Event::Offer { mime_type } = ev
                    && let Ok(mut t) = types.lock()
                {
                    t.push(mime_type);
                }
            }
        }

        impl Dispatch<$p::$src_m::$Src, Payload> for Worker {
            fn event(w: &mut Self, src: &$p::$src_m::$Src, ev: $p::$src_m::Event, payload: &Payload, _: &Connection, _: &QueueHandle<Self>) {
                match ev {
                    $p::$src_m::Event::Send { fd, .. } => hand_over(fd, payload.clone()),
                    $p::$src_m::Event::Cancelled => {
                        src.destroy();
                        w.sources.retain(|s| s.id() != src.id());
                    }
                    _ => {}
                }
            }
        }
    };
}

dispatch!(ext, Ext, ext_data_control_manager_v1, ExtDataControlManagerV1, ext_data_control_device_v1, ExtDataControlDeviceV1, ext_data_control_offer_v1, ExtDataControlOfferV1, ext_data_control_source_v1, ExtDataControlSourceV1);
dispatch!(wlr, Wlr, zwlr_data_control_manager_v1, ZwlrDataControlManagerV1, zwlr_data_control_device_v1, ZwlrDataControlDeviceV1, zwlr_data_control_offer_v1, ZwlrDataControlOfferV1, zwlr_data_control_source_v1, ZwlrDataControlSourceV1);

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for Worker {
    fn event(_: &mut Self, _: &wl_registry::WlRegistry, _: wl_registry::Event, _: &GlobalListContents, _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlSeat, ()> for Worker {
    fn event(_: &mut Self, _: &WlSeat, _: wayland_client::protocol::wl_seat::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

/// Writes a copy of ours to the app pasting it, on a thread (it reads at its own pace).
fn hand_over(fd: OwnedFd, payload: Payload) {
    std::thread::spawn(move || {
        let mut f = std::fs::File::from(fd);
        let _ = f.write_all(payload.bytes());
    });
}

// ---- the worker

/// A copy read from the clipboard, for the selection numbered `seq`.
struct Got {
    seq: u64,
    body: Body,
}

struct Worker {
    shared: Arc<Mutex<Shared>>,
    qh: QueueHandle<Worker>,
    handle: LoopHandle<'static, Worker>,
    manager: Manager,
    device: Option<Device>,
    /// The clipboard's offer now.
    current: Option<Offer>,
    /// Counts the clipboard's changes, so a copy that took long to read doesn't land on top of
    /// a newer one.
    seq: u64,
    reads: Sender<Got>,
    /// Copies of ours on the clipboard (put back from the history).
    sources: Vec<Source>,
    on: bool,
    length: u32,
    keep: bool,
    next_id: u64,
    /// A save is waiting on its timer.
    saving: bool,
    changed: bool,
}

impl Worker {
    fn run(shared: Arc<Mutex<Shared>>, jobs: Channel<Job>, first: Job, notify: &dyn Fn()) -> Result<(), String> {
        let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session ({e})"))?;
        let (globals, queue) = registry_queue_init::<Worker>(&conn).map_err(|e| e.to_string())?;
        let qh = queue.handle();
        let manager = match globals.bind::<ext::ext_data_control_manager_v1::ExtDataControlManagerV1, _, _>(&qh, 1..=1, ()) {
            Ok(m) => Manager::Ext(m),
            Err(_) => match globals.bind::<wlr::zwlr_data_control_manager_v1::ZwlrDataControlManagerV1, _, _>(&qh, 1..=2, ()) {
                Ok(m) => Manager::Wlr(m),
                Err(_) => {
                    // GNOME and others that keep the clipboard to themselves: quietly none.
                    if let Ok(mut s) = shared.lock() {
                        s.able = Some(false);
                    }
                    notify();
                    return Ok(());
                }
            },
        };
        let seat = globals.bind::<WlSeat, _, _>(&qh, 1..=1, ()).map_err(|_| "the compositor has no seat")?;
        let device = manager.device(&seat, &qh);
        let mut event_loop: EventLoop<Worker> = EventLoop::try_new().map_err(|e| e.to_string())?;
        let handle = event_loop.handle();
        let (reads, read_rx) = channel::<Got>();
        let mut w = Worker { shared: shared.clone(), qh, handle: handle.clone(), manager, device: Some(device), current: None, seq: 0, reads, sources: Vec::new(), on: false, length: 50, keep: false, next_id: 1, saving: false, changed: false };
        if let Ok(mut s) = shared.lock() {
            s.able = Some(true);
        }
        w.job(first);
        notify();
        WaylandSource::new(conn, queue).insert(handle.clone()).map_err(|e| e.to_string())?;
        handle.insert_source(jobs, |ev, _, w: &mut Worker| {
            if let ChanEvent::Msg(j) = ev {
                w.job(j);
            }
        }).map_err(|e| e.to_string())?;
        handle.insert_source(read_rx, |ev, _, w: &mut Worker| {
            if let ChanEvent::Msg(r) = ev
                && w.on
                && r.seq == w.seq
            {
                w.add(r.body);
            }
        }).map_err(|e| e.to_string())?;
        loop {
            event_loop.dispatch(None, &mut w).map_err(|e| format!("the Wayland connection ended ({e})"))?;
            if std::mem::take(&mut w.changed) {
                notify();
            }
        }
    }

    fn with<R>(&mut self, f: impl FnOnce(&mut Vec<Clip>) -> R) -> R {
        let mut s = self.shared.lock().unwrap_or_else(|e| e.into_inner());
        let r = f(&mut s.list);
        trim(&mut s.list, self.length as usize);
        drop(s);
        self.changed = true;
        self.save_soon();
        r
    }

    fn job(&mut self, job: Job) {
        match job {
            Job::Set { on, length, keep } => {
                let length = length.clamp(*LENGTH.start(), *LENGTH.end());
                let first = self.next_id == 1;
                let (was_on, was_keep) = (self.on, self.keep);
                (self.on, self.length, self.keep) = (on, length, keep && on);
                if let Ok(mut s) = self.shared.lock() {
                    s.on = on;
                }
                if !on {
                    // Off forgets what there was, here and on disk.
                    self.with(|l| l.clear());
                    let _ = std::fs::remove_file(store());
                    return;
                }
                if self.keep && (first || !was_on) {
                    let loaded = load(&store(), &mut self.next_id);
                    self.with(|l| {
                        for c in loaded {
                            if !l.iter().any(|x| x.body == c.body) {
                                l.push(c);
                            }
                        }
                        l.sort_by_key(|c| std::cmp::Reverse(c.at));
                    });
                } else if was_keep && !self.keep {
                    let _ = std::fs::remove_file(store());
                } else {
                    self.with(|_| {});
                }
            }
            Job::Restore(id) => {
                let Some(body) = self.with(|l| {
                    let k = l.iter().position(|c| c.id == id)?;
                    let mut c = l.remove(k);
                    c.at = now();
                    let body = c.body.clone();
                    l.insert(0, c);
                    Some(body)
                }) else {
                    return;
                };
                let Some(device) = &self.device else { return };
                let types: Vec<&str> = match &body {
                    Body::Text(_) => TEXT_TYPES.to_vec(),
                    Body::Image(p) => vec![p.mime.as_str()],
                };
                let src = self.manager.source(&self.qh, Arc::new(body.clone()));
                for t in types {
                    src.offer(t);
                }
                device.set_selection(&src);
                self.sources.push(src);
            }
            Job::Pin(id, pinned) => self.with(|l| {
                if let Some(c) = l.iter_mut().find(|c| c.id == id) {
                    c.pinned = pinned;
                }
            }),
            Job::Remove(id) => self.with(|l| l.retain(|c| c.id != id)),
            Job::Clear => self.with(|l| l.retain(|c| c.pinned)),
        }
    }

    /// The clipboard changed: read the new copy on a thread.
    fn selection(&mut self, offer: Option<Offer>, conn: &Connection) {
        self.seq += 1;
        if let Some(old) = self.current.take() {
            old.destroy();
        }
        let Some(offer) = offer else { return };
        if !self.on {
            offer.destroy();
            return;
        }
        let types = offer.types();
        let Some(want) = want(&types) else {
            self.current = Some(offer);
            return;
        };
        let secret_hint = types.iter().any(|t| t == SECRET_HINT);
        let (o, conn, tx, seq) = (offer.clone(), conn.clone(), self.reads.clone(), self.seq);
        self.current = Some(offer);
        std::thread::spawn(move || {
            // A password manager's mark first: a copy marked secret is never asked for.
            if secret_hint && read_type(&o, &conn, SECRET_HINT, 64).is_none_or(|h| String::from_utf8_lossy(&h).trim() == "secret") {
                return;
            }
            let body = match want {
                Want::Text(t) => read_type(&o, &conn, t, MAX_TEXT).and_then(|b| String::from_utf8(b).ok()).filter(|s| !s.trim().is_empty()).map(|s| Body::Text(s.into())),
                Want::Image(t) => read_type(&o, &conn, t, MAX_IMAGE).and_then(|b| picture(t, b)).map(|p| Body::Image(Arc::new(p))),
            };
            if let Some(body) = body {
                let _ = tx.send(Got { seq, body });
            }
        });
    }

    fn add(&mut self, body: Body) {
        let id = self.next_id;
        self.next_id += 1;
        self.with(|l| add(l, body, id, now()));
    }

    /// Saves the history a couple of seconds after it changes (once for a burst of copies).
    fn save_soon(&mut self) {
        if !self.keep || self.saving {
            return;
        }
        self.saving = true;
        let _ = self.handle.insert_source(Timer::from_duration(Duration::from_secs(2)), |_, _, w: &mut Worker| {
            w.saving = false;
            if w.keep {
                let list = w.shared.lock().map(|s| s.list.clone()).unwrap_or_default();
                if let Err(e) = save(&store(), &list) {
                    eprintln!("rivet-shell: couldn't save the clipboard history: {e}");
                }
            }
            TimeoutAction::Drop
        });
    }
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

/// A new copy goes on top; the same copy again moves up instead.
fn add(list: &mut Vec<Clip>, body: Body, id: u64, at: i64) {
    match list.iter().position(|c| c.body == body) {
        Some(k) => {
            let mut c = list.remove(k);
            c.at = at;
            list.insert(0, c);
        }
        None => list.insert(0, Clip { id, at, pinned: false, body }),
    }
}

/// Keeps pinned copies, and the newest `length` of the rest within the pictures' budget.
fn trim(list: &mut Vec<Clip>, length: usize) {
    let (mut kept, mut images) = (0, 0);
    list.retain(|c| {
        if c.pinned {
            return true;
        }
        kept += 1;
        if let Body::Image(p) = &c.body {
            images += p.bytes.len();
        }
        kept <= length && images <= IMAGE_BUDGET
    });
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Want {
    Text(&'static str),
    Image(&'static str),
}

/// What to read of a copy offered as these types: its text when it has any, else a picture.
fn want(types: &[String]) -> Option<Want> {
    let has = |t: &str| types.iter().any(|x| x.eq_ignore_ascii_case(t));
    TEXT_TYPES.iter().find(|t| has(t)).map(|t| Want::Text(t)).or_else(|| IMAGE_TYPES.iter().find(|t| has(t)).map(|t| Want::Image(t)))
}

/// Asks for the copy as `mime` and reads it: None when it's larger than `cap`, or the app
/// takes longer than READ_TIME.
fn read_type(offer: &Offer, conn: &Connection, mime: &str, cap: usize) -> Option<Vec<u8>> {
    let (mut r, w) = std::io::pipe().ok()?;
    offer.receive(mime, &w);
    drop(w);
    conn.flush().ok()?;
    let until = Instant::now() + READ_TIME;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let left = until.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return None;
        }
        let mut pfd = libc::pollfd { fd: r.as_raw_fd(), events: libc::POLLIN, revents: 0 };
        // SAFETY: one pollfd for a pipe this function owns, for the time left.
        let n = unsafe { libc::poll(&mut pfd, 1, left.as_millis().min(i32::MAX as u128) as i32) };
        if n <= 0 {
            if n < 0 && std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return None;
        }
        match r.read(&mut buf) {
            Ok(0) => return Some(out),
            Ok(k) => {
                out.extend_from_slice(&buf[..k]);
                if out.len() > cap {
                    return None;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}

/// A picture with its size and thumbnail, when it decodes.
fn picture(mime: &str, bytes: Vec<u8>) -> Option<Picture> {
    let img = image::load_from_memory(&bytes).ok()?;
    let size = (img.width(), img.height());
    // A small picture is its own thumbnail (thumbnail() would enlarge it).
    let t = if size.0 <= THUMB.0 && size.1 <= THUMB.1 { img.into_rgba8() } else { img.thumbnail(THUMB.0, THUMB.1).into_rgba8() };
    Some(Picture { mime: mime.to_string(), bytes, size, thumb: (t.width(), t.height(), t.into_raw()) })
}

// ---- saved between sessions

/// Where the history is kept when it's saved.
fn store() -> PathBuf {
    rivet_xdg::places::data_home().join("rivet-shell/clipboard")
}

const MAGIC: &str = "rivet-clipboard 1\n";

/// Writes the history, readable only by its owner: a line per copy (kind, pinned, when, how
/// many bytes, type) followed by its bytes.
fn save(path: &Path, list: &[Clip]) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    let tmp = path.with_extension("new");
    let _ = std::fs::remove_file(&tmp);
    let mut f = std::fs::OpenOptions::new().write(true).create_new(true).mode(0o600).open(&tmp)?;
    let mut out = std::io::BufWriter::new(&mut f);
    out.write_all(MAGIC.as_bytes())?;
    for c in list {
        let (kind, mime) = match &c.body {
            Body::Text(_) => ('t', "text/plain"),
            Body::Image(p) => ('i', p.mime.as_str()),
        };
        let bytes = c.body.bytes();
        writeln!(out, "{kind} {} {} {} {mime}", u8::from(c.pinned), c.at, bytes.len())?;
        out.write_all(bytes)?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    drop(out);
    f.sync_all()?;
    std::fs::rename(&tmp, path)?;
    // A file from before stays private too.
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))
}

/// Reads a saved history (none, or what can be read of a damaged one).
fn load(path: &Path, next_id: &mut u64) -> Vec<Clip> {
    let Ok(data) = std::fs::read(path) else { return Vec::new() };
    let Some(mut rest) = data.strip_prefix(MAGIC.as_bytes()) else { return Vec::new() };
    let mut out = Vec::new();
    while let Some(nl) = rest.iter().position(|b| *b == b'\n') {
        let head = String::from_utf8_lossy(&rest[..nl]).to_string();
        rest = &rest[nl + 1..];
        let f: Vec<&str> = head.splitn(5, ' ').collect();
        let [kind, pinned, at, len, mime] = f[..] else { break };
        let (Ok(at), Ok(len)) = (at.parse::<i64>(), len.parse::<usize>()) else { break };
        if rest.len() < len + 1 {
            break;
        }
        let bytes = rest[..len].to_vec();
        rest = &rest[len + 1..];
        let body = match kind {
            "t" => String::from_utf8(bytes).ok().map(|s| Body::Text(s.into())),
            "i" => IMAGE_TYPES.iter().find(|t| **t == mime).and_then(|t| picture(t, bytes)).map(|p| Body::Image(Arc::new(p))),
            _ => None,
        };
        if let Some(body) = body {
            out.push(Clip { id: *next_id, at, pinned: pinned == "1", body });
            *next_id += 1;
        }
    }
    out
}

/// A hash of a copy's bytes, for checks.
#[cfg(test)]
fn digest(b: &Body) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    b.bytes().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Body {
        Body::Text(s.into())
    }

    #[test]
    fn picks_text_then_pictures() {
        let t = |v: &[&str]| want(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
        assert_eq!(t(&["text/html", "text/plain", "UTF8_STRING"]), Some(Want::Text("UTF8_STRING")));
        assert_eq!(t(&["TEXT/PLAIN;charset=UTF-8"]), Some(Want::Text("text/plain;charset=utf-8")));
        assert_eq!(t(&["text/html", "image/png"]), Some(Want::Image("image/png")));
        assert_eq!(t(&["image/bmp", "text/html"]), None);
    }

    #[test]
    fn copies_again_move_up_and_trim_keeps_pins() {
        let mut l = Vec::new();
        for (i, s) in ["a", "b", "c"].iter().enumerate() {
            add(&mut l, text(s), i as u64 + 1, i as i64);
        }
        add(&mut l, text("a"), 9, 10);
        assert_eq!(l.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 3, 2]);
        assert_eq!(l[0].at, 10);
        l[2].pinned = true;
        trim(&mut l, 1);
        assert_eq!(l.iter().map(|c| c.id).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(shown(&l).iter().map(|c| c.id).collect::<Vec<_>>(), [2, 1]);
    }

    #[test]
    fn one_line() {
        let c = Clip { id: 1, at: 0, pinned: false, body: text("  fn main() {\n    println!();\n}\n") };
        assert_eq!(c.line(80), "fn main() { println!(); }");
        assert_eq!(c.line(6), "fn mai");
    }

    #[test]
    fn saves_privately_and_loads() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("rivet-shell-clip-{}", std::process::id()));
        let path = dir.join("sub/clipboard");
        let mut png = Vec::new();
        image::RgbaImage::from_pixel(40, 20, image::Rgba([10, 20, 30, 255])).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let pic = picture("image/png", png).unwrap();
        assert_eq!((pic.size, pic.thumb.0, pic.thumb.1), ((40, 20), 40, 20));
        let list = vec![Clip { id: 1, at: 5, pinned: true, body: text("two\nlines with \u{e9}") }, Clip { id: 2, at: 4, pinned: false, body: Body::Image(Arc::new(pic)) }];
        save(&path, &list).unwrap();
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        assert_eq!(std::fs::metadata(path.parent().unwrap()).unwrap().permissions().mode() & 0o777, 0o700);
        let mut next = 7;
        let back = load(&path, &mut next);
        assert_eq!(back.iter().map(|c| (c.at, c.pinned, digest(&c.body))).collect::<Vec<_>>(), list.iter().map(|c| (c.at, c.pinned, digest(&c.body))).collect::<Vec<_>>());
        assert_eq!(next, 9);
        // A damaged file gives what it can.
        let data = std::fs::read(&path).unwrap();
        std::fs::write(&path, &data[..data.len() - 30]).unwrap();
        assert_eq!(load(&path, &mut next).len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
