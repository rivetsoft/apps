//! Drag and drop, through the seat's data device: files dragged over the dock (dropped on an
//! app, they open in it; on the Trash, they're thrown away), and a file dragged out of the
//! Downloads list into another app. A part of `wl.rs`, for the shell's state.

use std::io::{Read, Write};
use std::path::PathBuf;

use eframe::egui::{Pos2, Rect, Vec2};
use smithay_client_toolkit::data_device_manager::WritePipe;
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::data_device_manager::data_device::{DataDeviceData, DataDeviceHandler};
use smithay_client_toolkit::data_device_manager::data_offer::{DataOfferHandler, DragOffer};
use smithay_client_toolkit::data_device_manager::data_source::DataSourceHandler;
use wayland_client::protocol::wl_data_device::WlDataDevice;
use wayland_client::protocol::wl_data_device_manager::DndAction;
use wayland_client::protocol::wl_data_source::WlDataSource;
use wayland_client::protocol::wl_surface::WlSurface;
use wayland_client::{Connection, Proxy, QueueHandle};

use super::{Kind, Msg, Shell};
use crate::ui::{self, DockSlot};

/// The type files are dragged as, by every file manager and browser.
const URI_LIST: &str = "text/uri-list";

/// Files being dragged over the dock.
pub struct DragIn {
    pub pos: Pos2,
    /// The files' types, once the files are read (dragged from a window, they come a moment
    /// after the drag reaches the dock).
    pub mimes: Option<Vec<String>>,
}

/// The local files in a `text/uri-list`: one address a line, `#` lines are comments, and
/// what isn't a local file (a web address) is left out.
pub fn files_in(list: &str) -> Vec<PathBuf> {
    list.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| l.strip_prefix("file://"))
        // `file://host/path`: the path starts at its first slash.
        .filter_map(|rest| rest.find('/').map(|i| &rest[i..]))
        .map(|p| PathBuf::from(rivet_xdg::places::percent_decode(p)))
        .collect()
}

/// A file as a line of a `text/uri-list`.
fn uri_of(path: &std::path::Path) -> String {
    format!("file://{}\r\n", rivet_xdg::places::percent_encode_path(path))
}

/// Read what a drag offers as files, on a thread, and hand them to the shell: `dropped` says
/// whether it's for the drop or only to know their types.
fn read_files(offer: &DragOffer, conn: &Connection, tx: &calloop::channel::Sender<Msg>, dropped: bool) {
    let Ok(mut pipe) = offer.receive(URI_LIST.to_string()) else { return };
    let _ = conn.flush();
    let tx = tx.clone();
    std::thread::spawn(move || {
        let mut text = String::new();
        let _ = pipe.read_to_string(&mut text);
        let _ = tx.send(Msg::DragFiles(files_in(&text), dropped));
    });
}

impl Shell {
    /// The dock's surface the drag is on, and the drag's offer.
    fn dock_drag(&self, device: &WlDataDevice) -> Option<(usize, DragOffer)> {
        let offer = device.data::<DataDeviceData>()?.drag_offer()?;
        let i = self.index_of(&offer.surface)?;
        (self.surfs[i].kind == Kind::Dock).then_some((i, offer))
    }

    /// The slot under the drag on the dock's surface `i`.
    fn drag_slot(&self, i: usize, pos: Pos2) -> Option<DockSlot> {
        let (w, h) = self.surfs[i].size;
        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
        ui::dock_slot_at(&self.s, &self.apps, &self.prev_status.running, full, pos)
    }

    /// Say whether the slot under the drag would take it, so the pointer shows it.
    fn answer_drag(&mut self, i: usize, offer: &DragOffer) {
        let pos = Pos2::new(offer.x as f32, offer.y as f32);
        let mimes = self.drag_in.as_ref().and_then(|d| d.mimes.clone());
        let takes = self.drag_slot(i, pos).is_some_and(|slot| ui::dock_takes(&slot, mimes.as_deref()));
        offer.accept_mime_type(offer.serial, takes.then(|| URI_LIST.to_string()));
        if let Some(d) = self.drag_in.as_mut() {
            d.pos = pos;
        }
        self.surfs[i].dirty = true;
    }

    /// The files of a drag, read: their types for the dock to show what takes them, or, for
    /// the drop, what is done with them.
    pub(super) fn drag_files(&mut self, files: Vec<PathBuf>, dropped: bool) {
        if !dropped {
            if let Some(d) = self.drag_in.as_mut() {
                d.mimes = Some(files.iter().map(|f| crate::places_pop::mime_of(f)).collect());
            }
            self.redraw_kinds(&[Kind::Dock]);
            return;
        }
        let Some((slot, offer)) = self.dropping.take() else { return };
        offer.finish();
        offer.destroy();
        if files.is_empty() {
            return;
        }
        match slot {
            DockSlot::App(app, _) => {
                if let Err(e) = rivet_xdg::launch::launch_app(&app, &files) {
                    eprintln!("rivet-shell: {e}");
                }
            }
            DockSlot::Trash => {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    for f in &files {
                        if let Err(e) = rivet_xdg::trash::trash(f) {
                            eprintln!("rivet-shell: couldn't move {} to the Trash ({e})", f.display());
                        }
                    }
                    let _ = tx.send(Msg::PlacesChanged);
                });
            }
            DockSlot::Downloads => {}
        }
    }

    /// Start dragging a file out (from the Downloads list), as the press that began it.
    pub(super) fn drag_out(&mut self, path: PathBuf) {
        let (Some(ddm), Some(device)) = (self.ddm.as_ref(), self.data_device.as_ref()) else { return };
        let Some(origin) = self.surfs.iter().find(|s| s.kind == Kind::Popup).map(|s| s.layer.wl_surface().clone()) else { return };
        let source = ddm.create_drag_and_drop_source(&self.qh, [URI_LIST, "text/plain;charset=utf-8"], DndAction::Copy);
        source.start_drag(device, &origin, None, self.press_serial);
        self.drag_source = Some((source, path));
    }

    fn end_drag_out(&mut self, source: &WlDataSource) {
        if self.drag_source.as_ref().is_some_and(|(s, _)| s.inner() == source) {
            self.drag_source = None;
            source.destroy();
        }
    }
}

impl DataDeviceHandler for Shell {
    fn enter(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, _: f64, _: f64, _: &WlSurface) {
        let Some((i, offer)) = self.dock_drag(device) else {
            // Not the dock: nothing here takes a drop.
            if let Some(offer) = device.data::<DataDeviceData>().and_then(|d| d.drag_offer()) {
                offer.accept_mime_type(offer.serial, None);
            }
            return;
        };
        if !offer.with_mime_types(|m| m.iter().any(|t| t == URI_LIST)) {
            offer.accept_mime_type(offer.serial, None);
            return;
        }
        // Copy only: the files stay where they are, whatever is done with them here.
        offer.set_actions(DndAction::Copy, DndAction::Copy);
        self.drag_in = Some(DragIn { pos: Pos2::new(offer.x as f32, offer.y as f32), mimes: None });
        read_files(&offer, conn, &self.tx, false);
        // As if the pointer were on it: a dock that hides comes out.
        let s = &mut self.surfs[i];
        s.hovered = true;
        s.left_at = None;
        self.answer_drag(i, &offer);
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        if self.drag_in.take().is_some() {
            for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Dock) {
                s.hovered = false;
                s.left_at = Some(std::time::Instant::now());
                s.dirty = true;
            }
        }
    }

    fn motion(&mut self, _: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice, _: f64, _: f64) {
        if self.drag_in.is_some()
            && let Some((i, offer)) = self.dock_drag(device)
        {
            self.answer_drag(i, &offer);
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {}

    fn drop_performed(&mut self, conn: &Connection, _: &QueueHandle<Self>, device: &WlDataDevice) {
        let Some((i, offer)) = self.dock_drag(device) else { return };
        let mimes = self.drag_in.as_ref().and_then(|d| d.mimes.clone());
        let pos = Pos2::new(offer.x as f32, offer.y as f32);
        match self.drag_slot(i, pos).filter(|slot| ui::dock_takes(slot, mimes.as_deref())) {
            Some(slot) => {
                read_files(&offer, conn, &self.tx, true);
                self.dropping = Some((slot, offer));
            }
            None => offer.destroy(),
        }
    }
}

impl DataOfferHandler for Shell {
    fn source_actions(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
    fn selected_action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &mut DragOffer, _: DndAction) {}
}

impl DataSourceHandler for Shell {
    fn accept_mime(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: Option<String>) {}

    fn send_request(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource, mime: String, mut fd: WritePipe) {
        let Some((_, path)) = self.drag_source.as_ref().filter(|(s, _)| s.inner() == source) else { return };
        let text = if mime == URI_LIST { uri_of(path) } else { path.display().to_string() };
        let _ = fd.write_all(text.as_bytes());
    }

    fn cancelled(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_drag_out(source);
    }

    fn dnd_dropped(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource) {}

    fn dnd_finished(&mut self, _: &Connection, _: &QueueHandle<Self>, source: &WlDataSource) {
        self.end_drag_out(source);
        // The file is where it was taken: the list it came from has done its job.
        self.close_popup();
    }

    fn action(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataSource, _: DndAction) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_list_gives_its_local_files() {
        let list = "# from a browser\r\nfile:///home/me/My%20Photo.jpg\r\nhttps://example.com/a\r\nfile://host/tmp/b.txt\r\n";
        assert_eq!(files_in(list), vec![PathBuf::from("/home/me/My Photo.jpg"), PathBuf::from("/tmp/b.txt")]);
        assert_eq!(uri_of(std::path::Path::new("/home/me/My Photo.jpg")), "file:///home/me/My%20Photo.jpg\r\n");
    }
}
