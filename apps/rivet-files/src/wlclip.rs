//! Files on the Wayland clipboard, and dragging files out to other apps.
//!
//! egui's clipboard only carries text. Other file managers, browsers, chat apps and editors want
//! files in the standard forms: `text/uri-list`, `x-special/gnome-copied-files` (GNOME, which also
//! says copy or cut), `application/x-kde-cutselection` (KDE's cut flag) and plain paths for
//! terminals. This worker talks Wayland on the window's own connection (its own event queue, its
//! own thread): it offers copied files in all those forms, reads files another app copied, and
//! hands a drag that leaves the window over to the desktop as a real file drag.
//!
//! X11 has none of this (there, text copy and paste still work through egui).

use std::io::{Read, Write};
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::{
    wl_data_device::{self, WlDataDevice},
    wl_data_device_manager::WlDataDeviceManager,
    wl_data_offer::{self, WlDataOffer},
    wl_data_source::{self, WlDataSource},
    wl_keyboard::{self, WlKeyboard},
    wl_pointer::{self, WlPointer},
    wl_registry::{self, WlRegistry},
    wl_seat::{self, WlSeat},
    wl_surface::WlSurface,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

const URI_LIST: &str = "text/uri-list";
const GNOME: &str = "x-special/gnome-copied-files";
const KDE_CUT: &str = "application/x-kde-cutselection";
const TEXT: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];

enum Cmd {
    /// Put these files on the clipboard (cut = move when pasted).
    SetFiles(Vec<PathBuf>, bool),
    /// Hand this drag to the desktop.
    Drag(Vec<PathBuf>),
    /// Read files another app copied; the answer goes to `Shared::pasted`.
    ReadFiles,
}

#[derive(Default)]
struct Shared {
    /// Rivet's own files are on the clipboard (false once another app copies something).
    owner: AtomicBool,
    /// Whatever is on the clipboard now carries files.
    has_files: AtomicBool,
    /// Files read from the clipboard: (paths, cut).
    pasted: Mutex<Option<(Vec<PathBuf>, bool)>>,
    /// A drag handed to the desktop has ended (dropped or cancelled).
    drag_done: AtomicBool,
}

struct Worker {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
}

static WORKER: OnceLock<Option<Worker>> = OnceLock::new();

/// Start the worker for this window (once). `display` and `surface` are the window's
/// `wl_display` and `wl_surface` pointers; nothing happens off Wayland.
pub fn start(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, repaint: eframe::egui::Context) {
    WORKER.get_or_init(|| spawn(display, surface, repaint));
}

fn worker() -> Option<&'static Worker> {
    WORKER.get().and_then(|w| w.as_ref())
}

pub fn available() -> bool {
    worker().is_some()
}

/// Offer these files to other apps. Returns false when there's no Wayland worker.
pub fn set_files(paths: Vec<PathBuf>, cut: bool) -> bool {
    match worker() {
        Some(w) => {
            w.shared.owner.store(true, Ordering::Release);
            w.tx.send(Cmd::SetFiles(paths, cut)).is_ok()
        }
        None => false,
    }
}

/// Is Rivet's own copy still the newest thing on the clipboard?
pub fn still_ours() -> bool {
    worker().is_none_or(|w| w.shared.owner.load(Ordering::Acquire))
}

/// Does the clipboard hold files from another app?
pub fn others_have_files() -> bool {
    worker().is_some_and(|w| !w.shared.owner.load(Ordering::Acquire) && w.shared.has_files.load(Ordering::Acquire))
}

/// Ask for the files another app copied; collect them with [`take_pasted`].
pub fn read_files() {
    if let Some(w) = worker() {
        let _ = w.tx.send(Cmd::ReadFiles);
    }
}

pub fn take_pasted() -> Option<(Vec<PathBuf>, bool)> {
    worker()?.shared.pasted.lock().ok()?.take()
}

pub fn drag(paths: Vec<PathBuf>) -> bool {
    match worker() {
        Some(w) => {
            w.shared.drag_done.store(false, Ordering::Release);
            w.tx.send(Cmd::Drag(paths)).is_ok()
        }
        None => false,
    }
}

/// Has the drag handed to the desktop ended? (Reads and clears.)
pub fn drag_ended() -> bool {
    worker().is_some_and(|w| w.shared.drag_done.swap(false, Ordering::AcqRel))
}

// ------------------------------------------------------------------ the formats

fn uri(p: &std::path::Path) -> String {
    format!("file://{}", crate::places::percent_encode_path(p))
}

/// The bytes for one offered type.
fn payload(mime: &str, paths: &[PathBuf], cut: bool) -> Vec<u8> {
    let uris: Vec<String> = paths.iter().map(|p| uri(p)).collect();
    match mime {
        URI_LIST => uris.iter().map(|u| format!("{u}\r\n")).collect::<String>().into_bytes(),
        GNOME => format!("{}\n{}", if cut { "cut" } else { "copy" }, uris.join("\n")).into_bytes(),
        KDE_CUT => if cut { b"1".to_vec() } else { b"0".to_vec() },
        _ => paths.iter().map(|p| p.to_string_lossy().to_string()).collect::<Vec<_>>().join("\n").into_bytes(),
    }
}

/// Files out of what another app offered, by type. Returns (paths, cut).
fn parse(mime: &str, text: &str) -> (Vec<PathBuf>, bool) {
    let mut cut = false;
    let mut lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty() && !l.starts_with('#')).collect();
    if mime == GNOME && let Some(first) = lines.first() {
        cut = *first == "cut";
        if matches!(*first, "cut" | "copy") {
            lines.remove(0);
        }
    }
    let paths = lines
        .into_iter()
        .map(|l| match l.strip_prefix("file://") {
            // file://host/path: only local files.
            Some(rest) => PathBuf::from(crate::places::percent_decode(rest.strip_prefix("localhost").unwrap_or(rest))),
            None => PathBuf::from(l),
        })
        .filter(|p| p.is_absolute() && p.exists())
        .collect();
    (paths, cut)
}

// ------------------------------------------------------------------ the worker

#[derive(Default)]
struct OfferMimes(Mutex<Vec<String>>);

enum SourceKind {
    Selection,
    Drag,
}

struct State {
    shared: Arc<Shared>,
    repaint: eframe::egui::Context,
    seat: Option<WlSeat>,
    manager: Option<WlDataDeviceManager>,
    device: Option<WlDataDevice>,
    pointer: Option<WlPointer>,
    keyboard: Option<WlKeyboard>,
    /// The newest input serial (needed to set the clipboard), and the last button press's
    /// (needed to start a drag).
    serial: u32,
    press_serial: u32,
    /// The clipboard offer from other apps, and the drag offer passing over (ignored).
    selection: Option<WlDataOffer>,
    dnd_offer: Option<WlDataOffer>,
    /// What Rivet offers: (source, files, cut).
    copy_source: Option<(WlDataSource, Vec<PathBuf>, bool)>,
    drag_source: Option<(WlDataSource, Vec<PathBuf>)>,
}

fn spawn(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, repaint: eframe::egui::Context) -> Option<Worker> {
    if display.is_null() || surface.is_null() {
        return None;
    }
    // SAFETY: the window's own wl_display, alive as long as the window (and so the app).
    let backend = unsafe { wayland_backend::client::Backend::from_foreign_display(display.cast()) };
    let conn = Connection::from_backend(backend);
    // SAFETY: the window's own wl_surface on this connection.
    let surface_id = unsafe { wayland_backend::client::ObjectId::from_ptr(WlSurface::interface(), surface.cast()) }.ok()?;
    let origin = WlSurface::from_id(&conn, surface_id).ok()?;
    let mut queue = conn.new_event_queue::<State>();
    let qh = queue.handle();
    let shared = Arc::new(Shared::default());
    let mut state = State {
        shared: shared.clone(),
        repaint,
        seat: None,
        manager: None,
        device: None,
        pointer: None,
        keyboard: None,
        serial: 0,
        press_serial: 0,
        selection: None,
        dnd_offer: None,
        copy_source: None,
        drag_source: None,
    };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state).ok()?;
    let (seat, manager) = (state.seat.clone()?, state.manager.clone()?);
    state.device = Some(manager.get_data_device(&seat, &qh, ()));
    queue.roundtrip(&mut state).ok()?;
    let (tx, rx) = channel();
    std::thread::Builder::new().name("rivet-clipboard".into()).spawn(move || run(conn, queue, state, rx, origin)).ok()?;
    Some(Worker { tx, shared })
}

fn run(conn: Connection, mut queue: wayland_client::EventQueue<State>, mut state: State, rx: Receiver<Cmd>, origin: WlSurface) {
    let qh = queue.handle();
    loop {
        while let Ok(cmd) = rx.try_recv() {
            state.command(cmd, &qh, &conn, &origin);
        }
        let _ = conn.flush();
        if queue.dispatch_pending(&mut state).is_err() {
            return;
        }
        // Wait for events (or every 30 ms, for new commands).
        if let Some(guard) = queue.prepare_read() {
            let fd = guard.connection_fd();
            use std::os::fd::AsRawFd;
            let mut pfd = libc::pollfd { fd: fd.as_raw_fd(), events: libc::POLLIN, revents: 0 };
            // SAFETY: one pollfd for the connection's socket.
            let n = unsafe { libc::poll(&mut pfd, 1, 30) };
            if n > 0 {
                if guard.read().is_err() {
                    return;
                }
            } else {
                drop(guard);
            }
        }
        if queue.dispatch_pending(&mut state).is_err() {
            return;
        }
    }
}

impl State {
    fn offer(&self, qh: &QueueHandle<State>, kind: SourceKind, with_gnome: bool) -> Option<WlDataSource> {
        let m = self.manager.as_ref()?;
        let s = m.create_data_source(qh, kind);
        s.offer(URI_LIST.into());
        if with_gnome {
            s.offer(GNOME.into());
            s.offer(KDE_CUT.into());
        }
        for t in TEXT {
            s.offer(t.into());
        }
        Some(s)
    }

    fn command(&mut self, cmd: Cmd, qh: &QueueHandle<State>, conn: &Connection, origin: &WlSurface) {
        match cmd {
            Cmd::SetFiles(paths, cut) => {
                let Some(src) = self.offer(qh, SourceKind::Selection, true) else { return };
                if let Some(d) = &self.device {
                    d.set_selection(Some(&src), self.serial);
                }
                if let Some((old, _, _)) = self.copy_source.replace((src, paths, cut)) {
                    old.destroy();
                }
            }
            Cmd::Drag(paths) => {
                let Some(src) = self.offer(qh, SourceKind::Drag, false) else { return };
                if src.version() >= 3 {
                    src.set_actions(DndAction::Copy);
                }
                if let Some(d) = &self.device {
                    d.start_drag(Some(&src), origin, None, self.press_serial);
                }
                if let Some((old, _)) = self.drag_source.replace((src, paths)) {
                    old.destroy();
                }
            }
            Cmd::ReadFiles => {
                let Some(offer) = self.selection.clone() else { return };
                let mimes = offer.data::<OfferMimes>().map(|m| m.0.lock().map(|v| v.clone()).unwrap_or_default()).unwrap_or_default();
                let Some(mime) = [GNOME, URI_LIST].into_iter().chain(TEXT).find(|t| mimes.iter().any(|m| m == t)) else { return };
                let mut fds = [0; 2];
                // SAFETY: a fresh pipe; both ends are owned right after.
                if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                    return;
                }
                let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
                offer.receive(mime.into(), write.as_fd());
                drop(write);
                let _ = conn.flush();
                let shared = self.shared.clone();
                let repaint = self.repaint.clone();
                let mime = mime.to_string();
                std::thread::spawn(move || {
                    let mut f = std::fs::File::from(read);
                    let mut buf = Vec::new();
                    // Another app is answering; give up after a few seconds.
                    let deadline = std::time::Instant::now() + Duration::from_secs(3);
                    let mut chunk = [0u8; 8192];
                    use std::os::fd::AsRawFd;
                    while std::time::Instant::now() < deadline {
                        let mut pfd = libc::pollfd { fd: f.as_raw_fd(), events: libc::POLLIN, revents: 0 };
                        // SAFETY: one pollfd for the pipe.
                        if unsafe { libc::poll(&mut pfd, 1, 200) } <= 0 {
                            continue;
                        }
                        match f.read(&mut chunk) {
                            Ok(0) | Err(_) => break,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let (paths, cut) = parse(&mime, &String::from_utf8_lossy(&buf));
                    if let Ok(mut p) = shared.pasted.lock() {
                        *p = Some((paths, cut));
                    }
                    repaint.request_repaint();
                });
            }
        }
    }
}

impl Dispatch<WlRegistry, ()> for State {
    fn event(state: &mut Self, reg: &WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_seat" if state.seat.is_none() => state.seat = Some(reg.bind(name, version.min(7), qh, ())),
                "wl_data_device_manager" => state.manager = Some(reg.bind(name, version.min(3), qh, ())),
                _ => {}
            }
        }
    }
}

impl Dispatch<WlSeat, ()> for State {
    fn event(state: &mut Self, seat: &WlSeat, event: wl_seat::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities: WEnum::Value(caps) } = event {
            if caps.contains(wl_seat::Capability::Pointer) && state.pointer.is_none() {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
            if caps.contains(wl_seat::Capability::Keyboard) && state.keyboard.is_none() {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}

impl Dispatch<WlPointer, ()> for State {
    fn event(state: &mut Self, _: &WlPointer, event: wl_pointer::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { serial, .. } => state.serial = serial,
            wl_pointer::Event::Button { serial, state: WEnum::Value(wl_pointer::ButtonState::Pressed), .. } => {
                state.serial = serial;
                state.press_serial = serial;
            }
            wl_pointer::Event::Button { serial, .. } => state.serial = serial,
            _ => {}
        }
    }
}

impl Dispatch<WlKeyboard, ()> for State {
    fn event(state: &mut Self, _: &WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_keyboard::Event::Enter { serial, .. } | wl_keyboard::Event::Key { serial, .. } => state.serial = serial,
            // The keymap's file descriptor closes when dropped here.
            _ => {}
        }
    }
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(_: &mut Self, _: &WlDataDeviceManager, _: <WlDataDeviceManager as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(state: &mut Self, _: &WlDataDevice, event: wl_data_device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_device::Event::Selection { id } => {
                if let Some(old) = state.selection.take()
                    && Some(&old) != id.as_ref()
                {
                    old.destroy();
                }
                let files = id.as_ref().and_then(|o| o.data::<OfferMimes>()).and_then(|m| m.0.lock().ok().map(|v| v.iter().any(|t| t == URI_LIST || t == GNOME))).unwrap_or(false);
                state.shared.has_files.store(files, Ordering::Release);
                state.selection = id;
                state.repaint.request_repaint();
            }
            // Drags passing over the window are winit's business; just tidy up our copy.
            wl_data_device::Event::Enter { id, .. } => {
                if let Some(old) = std::mem::replace(&mut state.dnd_offer, id) {
                    old.destroy();
                }
            }
            wl_data_device::Event::Leave | wl_data_device::Event::Drop => {
                if let Some(old) = state.dnd_offer.take() {
                    old.destroy();
                }
            }
            _ => {}
        }
    }

    wayland_client::event_created_child!(State, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, OfferMimes::default()),
    ]);
}

impl Dispatch<WlDataOffer, OfferMimes> for State {
    fn event(_: &mut Self, _: &WlDataOffer, event: wl_data_offer::Event, data: &OfferMimes, _: &Connection, _: &QueueHandle<Self>) {
        if let wl_data_offer::Event::Offer { mime_type } = event
            && let Ok(mut v) = data.0.lock()
        {
            v.push(mime_type);
        }
    }
}

impl Dispatch<WlDataSource, SourceKind> for State {
    fn event(state: &mut Self, src: &WlDataSource, event: wl_data_source::Event, kind: &SourceKind, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let (paths, cut) = match kind {
                    SourceKind::Selection => match &state.copy_source {
                        Some((s, p, c)) if s == src => (p.clone(), *c),
                        _ => return,
                    },
                    SourceKind::Drag => match &state.drag_source {
                        Some((s, p)) if s == src => (p.clone(), false),
                        _ => return,
                    },
                };
                // Write on the side: a slow reader mustn't hold up the clipboard.
                std::thread::spawn(move || {
                    let mut f = std::fs::File::from(fd);
                    let _ = f.write_all(&payload(&mime_type, &paths, cut));
                });
            }
            wl_data_source::Event::Cancelled => {
                src.destroy();
                match kind {
                    SourceKind::Selection => {
                        if state.copy_source.as_ref().is_some_and(|(s, _, _)| s == src) {
                            state.copy_source = None;
                            state.shared.owner.store(false, Ordering::Release);
                        }
                    }
                    SourceKind::Drag => {
                        if state.drag_source.as_ref().is_some_and(|(s, _)| s == src) {
                            state.drag_source = None;
                        }
                        state.shared.drag_done.store(true, Ordering::Release);
                    }
                }
                state.repaint.request_repaint();
            }
            wl_data_source::Event::DndFinished => {
                src.destroy();
                if state.drag_source.as_ref().is_some_and(|(s, _)| s == src) {
                    state.drag_source = None;
                }
                state.shared.drag_done.store(true, Ordering::Release);
                state.repaint.request_repaint();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_round_trip() {
        let dir = std::env::temp_dir();
        let paths = vec![dir.clone()];
        let gnome = String::from_utf8(payload(GNOME, &paths, true)).unwrap();
        assert!(gnome.starts_with("cut\nfile://"));
        assert_eq!(parse(GNOME, &gnome), (paths.clone(), true));
        let list = String::from_utf8(payload(URI_LIST, &paths, false)).unwrap();
        assert!(list.ends_with("\r\n"));
        assert_eq!(parse(URI_LIST, &list), (paths.clone(), false));
        assert_eq!(parse("text/plain", &dir.to_string_lossy()), (paths, false));
        assert_eq!(payload(KDE_CUT, &[], true), b"1");
        // Comments and missing files are skipped.
        assert_eq!(parse(URI_LIST, "# x\r\nfile:///no/such/file\r\n").0, Vec::<PathBuf>::new());
    }
}
