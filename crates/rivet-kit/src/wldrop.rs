//! Files dropped on a window from other apps, on Wayland.
//!
//! The window toolkit only reports dropped files on X11; on Wayland a drag from a file manager
//! (Rivet Files, or any other) arrives as a `wl_data_device` offer that nobody answers, so the
//! drop silently does nothing. This worker answers it on the window's own Wayland connection
//! (its own event queue and thread): it accepts offers carrying `text/uri-list`, says whether a
//! drag is over the window, and hands over the files once they're dropped.

use std::io::Read;
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use eframe::egui;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::{
    wl_data_device::{self, WlDataDevice},
    wl_data_device_manager::WlDataDeviceManager,
    wl_data_offer::{self, WlDataOffer},
    wl_registry::{self, WlRegistry},
    wl_seat::WlSeat,
};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle};

const URI_LIST: &str = "text/uri-list";

#[derive(Default)]
struct Shared {
    /// A drag with files is over the window.
    hovering: AtomicBool,
    /// Files dropped and not yet taken.
    dropped: Mutex<Vec<PathBuf>>,
}

static SHARED: OnceLock<Option<Arc<Shared>>> = OnceLock::new();

/// Start listening for drops on this window (once). `display` and `surface` are its
/// `wl_display` and `wl_surface`; nothing happens off Wayland. Returns whether it's running.
pub fn start(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, repaint: egui::Context) -> bool {
    SHARED.get_or_init(|| spawn(display, surface, repaint)).is_some()
}

/// Start it from an eframe window, when that window is on Wayland.
pub fn start_for(frame: &eframe::Frame, repaint: &egui::Context) -> bool {
    use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
    if let (Ok(d), Ok(w)) = (frame.display_handle(), frame.window_handle())
        && let (RawDisplayHandle::Wayland(d), RawWindowHandle::Wayland(w)) = (d.as_raw(), w.as_raw())
    {
        return start(d.display.as_ptr(), w.surface.as_ptr(), repaint.clone());
    }
    false
}

/// Is a drag with files over the window right now?
pub fn hovering() -> bool {
    SHARED.get().and_then(|s| s.as_ref()).is_some_and(|s| s.hovering.load(Ordering::Acquire))
}

/// The files dropped since last asked.
pub fn take_dropped() -> Vec<PathBuf> {
    SHARED.get().and_then(|s| s.as_ref()).and_then(|s| s.dropped.lock().ok().map(|mut v| std::mem::take(&mut *v))).unwrap_or_default()
}

/// `%20`-style escapes undone.
fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The local files in a `text/uri-list`.
pub fn parse_uri_list(text: &str) -> Vec<PathBuf> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let rest = l.strip_prefix("file://")?;
            let rest = rest.strip_prefix("localhost").unwrap_or(rest);
            rest.starts_with('/').then(|| PathBuf::from(percent_decode(rest)))
        })
        .collect()
}

struct State {
    shared: Arc<Shared>,
    repaint: egui::Context,
    conn: Connection,
    seat: Option<WlSeat>,
    manager: Option<WlDataDeviceManager>,
    device: Option<WlDataDevice>,
    /// The offer of the drag over the window, and whether it has files.
    offer: Option<(WlDataOffer, bool)>,
}

#[derive(Default)]
struct OfferMimes(Mutex<Vec<String>>);

fn spawn(display: *mut std::ffi::c_void, surface: *mut std::ffi::c_void, repaint: egui::Context) -> Option<Arc<Shared>> {
    if display.is_null() || surface.is_null() {
        return None;
    }
    // SAFETY: the window's own wl_display, alive as long as the window (and so the app).
    let backend = unsafe { wayland_backend::client::Backend::from_foreign_display(display.cast()) };
    let conn = Connection::from_backend(backend);
    let mut queue = conn.new_event_queue::<State>();
    let qh = queue.handle();
    let shared = Arc::new(Shared::default());
    let mut state = State { shared: shared.clone(), repaint, conn: conn.clone(), seat: None, manager: None, device: None, offer: None };
    conn.display().get_registry(&qh, ());
    queue.roundtrip(&mut state).ok()?;
    let (seat, manager) = (state.seat.clone()?, state.manager.clone()?);
    state.device = Some(manager.get_data_device(&seat, &qh, ()));
    queue.roundtrip(&mut state).ok()?;
    std::thread::Builder::new()
        .name("rivet-drops".into())
        .spawn(move || {
            loop {
                if queue.blocking_dispatch(&mut state).is_err() {
                    return;
                }
            }
        })
        .ok()?;
    Some(shared)
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
    fn event(_: &mut Self, _: &WlSeat, _: <WlSeat as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlDataDeviceManager, ()> for State {
    fn event(_: &mut Self, _: &WlDataDeviceManager, _: <WlDataDeviceManager as Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WlDataDevice, ()> for State {
    fn event(state: &mut Self, _: &WlDataDevice, event: wl_data_device::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_data_device::Event::Enter { serial, id, .. } => {
                if let Some((old, _)) = state.offer.take() {
                    old.destroy();
                }
                if let Some(offer) = id {
                    let files = offer.data::<OfferMimes>().and_then(|m| m.0.lock().ok().map(|v| v.iter().any(|t| t == URI_LIST))).unwrap_or(false);
                    if files {
                        offer.accept(serial, Some(URI_LIST.into()));
                        if offer.version() >= 3 {
                            offer.set_actions(DndAction::Copy, DndAction::Copy);
                        }
                    } else {
                        offer.accept(serial, None);
                    }
                    state.shared.hovering.store(files, Ordering::Release);
                    state.offer = Some((offer, files));
                    state.repaint.request_repaint();
                }
            }
            wl_data_device::Event::Leave => {
                if let Some((old, _)) = state.offer.take() {
                    old.destroy();
                }
                state.shared.hovering.store(false, Ordering::Release);
                state.repaint.request_repaint();
            }
            wl_data_device::Event::Drop => {
                state.shared.hovering.store(false, Ordering::Release);
                let Some((offer, true)) = state.offer.take() else {
                    state.repaint.request_repaint();
                    return;
                };
                let mut fds = [0; 2];
                // SAFETY: a fresh pipe; both ends are owned right after.
                if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
                    offer.destroy();
                    return;
                }
                let (read, write) = unsafe { (OwnedFd::from_raw_fd(fds[0]), OwnedFd::from_raw_fd(fds[1])) };
                offer.receive(URI_LIST.into(), write.as_fd());
                drop(write);
                let _ = state.conn.flush();
                let shared = state.shared.clone();
                let repaint = state.repaint.clone();
                let conn = state.conn.clone();
                // Read on the side (the other app answers through the pipe), then finish.
                std::thread::spawn(move || {
                    let mut f = std::fs::File::from(read);
                    let mut buf = Vec::new();
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
                    let files = parse_uri_list(&String::from_utf8_lossy(&buf));
                    if offer.version() >= 3 {
                        offer.finish();
                    }
                    offer.destroy();
                    let _ = conn.flush();
                    if let Ok(mut v) = shared.dropped.lock() {
                        v.extend(files);
                    }
                    repaint.request_repaint();
                });
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

#[cfg(test)]
mod tests {
    use super::parse_uri_list;
    use std::path::PathBuf;

    #[test]
    fn reads_uri_lists() {
        let t = "# comment\r\nfile:///home/me/My%20Photo.png\r\nfile://localhost/tmp/a.txt\r\nhttps://x.org/b\r\n";
        assert_eq!(parse_uri_list(t), [PathBuf::from("/home/me/My Photo.png"), PathBuf::from("/tmp/a.txt")]);
    }
}
