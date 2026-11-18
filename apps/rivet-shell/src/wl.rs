//! The shell on the desktop: on every screen, a wallpaper (with the desktop widgets) on the
//! background layer, the panel and the dock on the top layer, and popups (launcher, calendar,
//! power) on the overlay layer, through the wlr layer-shell protocol (niri, Hyprland, Sway, river,
//! labwc, Wayfire, COSMIC and others; not GNOME).
//!
//! Each surface is an egui screen drawn in software (`rivet_kit::soft`) into shared memory: no
//! GPU, and only when something changed.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use calloop::channel::{Channel, Event as ChanEvent, Sender};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportId};
use rivet_kit::soft;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers as KbMods, RawModifiers};
use smithay_client_toolkit::seat::pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1;
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
use crate::idle::{self, Stage};

use crate::config::{DockHide, Edge, Settings};

/// A hidden dock's strip along the edge, in points.
const DOCK_STRIP: u32 = 4;
/// How long the dock stays after the pointer leaves, and how long it takes to slide.
const DOCK_LINGER: Duration = Duration::from_millis(700);
const DOCK_SLIDE: f32 = 0.18;
use crate::sys::{self, Compositor};
use crate::notes::{self, NoteMsg, Notes, Reason};
use crate::ui::{self, Act, Extra, Kit, Model, OsdKind, Popup, PopupKind};

/// Messages from other threads (status, IPC, wallpaper loading, matugen).
pub enum Msg {
    Wake,
    Ipc(String),
    Wall { path: PathBuf, size: (u32, u32), pixels: Option<Vec<u8>> },
    Matugen(Result<(), String>),
    Note(NoteMsg),
    Apps(Vec<rivet_xdg::launch::DesktopApp>),
    /// Caps Lock (else Num Lock) turned on or off.
    LockKey { caps: bool, on: bool },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Background,
    Panel,
    Dock,
    Popup,
    /// Notification cards in the corner.
    Cards,
    /// Volume or brightness as it changes.
    Osd,
    /// The screen dimming before it locks or goes off (idle).
    Dim,
}

struct Surf {
    kind: Kind,
    /// Which panel (for panels).
    idx: usize,
    layer: LayerSurface,
    output: Option<wl_output::WlOutput>,
    output_name: String,
    /// In points, as the compositor configured it.
    size: (u32, u32),
    scale: i32,
    configured: bool,
    dirty: bool,
    waiting_frame: bool,
    repaint_at: Option<Instant>,
    ctx: egui::Context,
    kit: Kit,
    tex: soft::Textures,
    pool: SlotPool,
    events: Vec<egui::Event>,
    modifiers: egui::Modifiers,
    start: Instant,
    // ---- the dock's hiding
    /// How far it has slid away: 0 shown, 1 out of sight.
    slide: f32,
    slide_at: Instant,
    /// Down to the thin strip along the edge that brings it back.
    collapsed: bool,
    /// The pointer is on it, or when it left.
    hovered: bool,
    left_at: Option<Instant>,
    /// The last frame sent (to send only what changed).
    last: Vec<u8>,
}

impl Surf {
    fn physical(&self) -> (u32, u32) {
        (self.size.0 * self.scale as u32, self.size.1 * self.scale as u32)
    }
}

/// A screen's wallpaper, scaled to it (BGRA).
struct WallCache {
    path: PathBuf,
    size: (u32, u32),
    pixels: Vec<u8>,
}

pub struct Shell {
    conn: Connection,
    qh: QueueHandle<Shell>,
    registry: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    layer_shell: LayerShell,
    shm: Shm,
    surfs: Vec<Surf>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    kb_mods: egui::Modifiers,
    s: Settings,
    s_mtime: Option<SystemTime>,
    live: sys::Live,
    last_gen: u64,
    /// Wi-Fi networks and Bluetooth devices, read while their popup is open.
    radio: crate::radio::RadioLive,
    /// Sound devices and apps, read while the Sound popup is open.
    audio: crate::audio::AudioLive,
    audio_gen: u64,
    radio_gen: u64,
    ai: crate::ai::AiLive,
    ai_gen: u64,
    apps: Vec<rivet_xdg::launch::DesktopApp>,
    popup: Option<Popup>,
    comp: Compositor,
    walls: Vec<WallCache>,
    loading: Vec<(PathBuf, (u32, u32))>,
    tx: Sender<Msg>,
    palette_watch: Vec<(PathBuf, Option<SystemTime>)>,
    recolor_until: Option<Instant>,
    last_minute: u32,
    bg_key: (u32, u64, Option<(u32, bool)>),
    prev_status: sys::Status,
    /// The dock's items, to notice when it must change size.
    dock_key: (usize, bool),
    // ---- notifications, the on-screen display, night light, keep awake
    notes: Notes,
    note_server: Option<notes::Server>,
    dnd: bool,
    osd: Option<(OsdKind, u32, bool)>,
    osd_until: Option<Instant>,
    night: sys::NightLight,
    night_ok: bool,
    inhibit_mgr: Option<ZwpIdleInhibitManagerV1>,
    inhibitor: Option<ZwpIdleInhibitorV1>,
    // ---- idle and the lock screen (idle.rs)
    seat: Option<wl_seat::WlSeat>,
    idle_notifier: Option<ExtIdleNotifierV1>,
    idle_notes: Vec<ExtIdleNotificationV1>,
    locker: idle::Locker,
    /// Dimming since then.
    dim_at: Option<Instant>,
    /// The screens turned off (their names, to turn them on again).
    screens_off: Option<Vec<String>>,
    /// Kept awake while media plays.
    media_inhibitor: Option<ZwpIdleInhibitorV1>,
    lock_on_sleep: std::sync::Arc<std::sync::atomic::AtomicBool>,
    own_locker: std::sync::Arc<std::sync::atomic::AtomicBool>,
    user: String,
    /// The last popup's screen state (fonts, icons already made), so the next opens at once.
    spare: Option<(egui::Context, Kit, soft::Textures, Instant)>,
    /// The launcher was drawn once unseen at the start, so its first opening is quick too.
    prewarmed: bool,
    pub exit: bool,
}

fn mtime(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

fn palette_files() -> Vec<(PathBuf, Option<SystemTime>)> {
    rivet_look::palette::load().1.into_iter().map(|p| {
        let t = mtime(&p);
        (p, t)
    }).collect()
}

/// Run the shell until it's asked to quit. `ipc` brings commands from `rivet-shell msg`.
pub fn run(settings: Settings, ipc: Channel<Msg>, tx: Sender<Msg>) -> Result<(), String> {
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session to join ({e})"))?;
    let (globals, queue) = registry_queue_init::<Shell>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let layer_shell = LayerShell::bind(&globals, &qh).map_err(|_| "this compositor doesn't offer the layer-shell protocol (GNOME doesn't; niri, Hyprland, Sway, river, labwc, Wayfire and COSMIC do)".to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no shared memory (wl_shm)")?;
    let comp = Compositor::detect();
    let inhibit_mgr = globals.bind::<ZwpIdleInhibitManagerV1, _, _>(&qh, 1..=1, ()).ok();
    let idle_notifier = globals.bind::<ExtIdleNotifierV1, _, _>(&qh, 1..=1, ()).ok();
    idle::clear_stale_marker();
    let lock_on_sleep = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(settings.lock_on_sleep));
    let own_locker = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(settings.lock_cmd.trim().is_empty()));
    idle::watch_logind(tx.clone(), lock_on_sleep.clone(), own_locker.clone());
    let wake_tx = tx.clone();
    let live = sys::start(comp, move || {
        let _ = wake_tx.send(Msg::Wake);
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let radio = crate::radio::start(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let ai = crate::ai::start(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let audio = crate::audio::start(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let key_tx = tx.clone();
    sys::watch_lock_keys(move |caps, on| {
        let _ = key_tx.send(Msg::LockKey { caps, on });
    });
    let mut shell = Shell {
        conn: conn.clone(),
        qh: qh.clone(),
        registry: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        layer_shell,
        shm,
        surfs: Vec::new(),
        pointer: None,
        keyboard: None,
        kb_mods: egui::Modifiers::NONE,
        s_mtime: crate::config::mtime(),
        s: settings,
        live,
        last_gen: 0,
        radio,
        audio,
        audio_gen: 0,
        radio_gen: 0,
        ai,
        ai_gen: 0,
        apps: load_apps(),
        popup: None,
        comp,
        walls: Vec::new(),
        loading: Vec::new(),
        tx,
        palette_watch: palette_files(),
        recolor_until: None,
        last_minute: 99,
        bg_key: (0, 0, None),
        prev_status: sys::Status::default(),
        dock_key: (0, false),
        notes: Notes::default(),
        note_server: None,
        dnd: false,
        osd: None,
        osd_until: None,
        night: sys::NightLight::new(),
        night_ok: sys::NightLight::available(),
        inhibit_mgr,
        inhibitor: None,
        seat: None,
        idle_notifier,
        idle_notes: Vec::new(),
        locker: idle::Locker::default(),
        dim_at: None,
        screens_off: None,
        media_inhibitor: None,
        lock_on_sleep,
        own_locker,
        user: sys::user_name(),
        spare: None,
        prewarmed: false,
        exit: false,
    };
    shell.set_notifications();
    shell.set_ai();
    shell.apply_comp();
    let mut event_loop: EventLoop<Shell> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle: LoopHandle<Shell> = event_loop.handle();
    WaylandSource::new(conn, queue).insert(handle.clone()).map_err(|e| e.to_string())?;
    handle.insert_source(ipc, |ev, _, shell: &mut Shell| {
        if let ChanEvent::Msg(m) = ev {
            shell.on_msg(m);
        }
    }).map_err(|e| e.to_string())?;
    handle.insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, shell: &mut Shell| {
        shell.tick();
        TimeoutAction::ToDuration(Duration::from_millis(500))
    }).map_err(|e| e.to_string())?;
    // The wallpaper once at the start (and its colours, when they were never made).
    shell.wallpaper_changed(false);
    loop {
        // Wake in time for the next animation step, else at least four times a second.
        let wait = shell.surfs.iter().filter_map(|s| s.repaint_at).min().map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_millis(250)).min(Duration::from_millis(250));
        event_loop.dispatch(Some(wait), &mut shell).map_err(|e| e.to_string())?;
        shell.redraw();
        if shell.exit {
            return Ok(());
        }
    }
}

fn load_apps() -> Vec<rivet_xdg::launch::DesktopApp> {
    let mut apps = rivet_xdg::launch::all_apps();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

impl Shell {
    // ---------------------------------------------------------------- surfaces

    fn new_surf(&mut self, kind: Kind, output: Option<&wl_output::WlOutput>, idx: usize) -> Surf {
        let surface = self.compositor.create_surface(&self.qh);
        let layer = match kind {
            Kind::Background => Layer::Background,
            Kind::Panel | Kind::Dock => Layer::Top,
            Kind::Popup | Kind::Cards | Kind::Osd | Kind::Dim => Layer::Overlay,
        };
        let ns = match kind {
            Kind::Background => "rivet-shell-wallpaper",
            Kind::Panel => "rivet-shell-panel",
            Kind::Dock => "rivet-shell-dock",
            Kind::Popup => "rivet-shell-popup",
            Kind::Cards => "rivet-shell-notifications",
            Kind::Osd => "rivet-shell-osd",
            Kind::Dim => "rivet-shell-dim",
        };
        let layer = self.layer_shell.create_layer_surface(&self.qh, surface, layer, Some(ns), output);
        let name = output.and_then(|o| self.output_state.info(o)).and_then(|i| i.name).unwrap_or_default();
        let scale = output.and_then(|o| self.output_state.info(o)).map(|i| i.scale_factor).unwrap_or(1).max(1);
        let s = &self.s;
        match kind {
            Kind::Background => {
                layer.set_anchor(Anchor::all());
                layer.set_exclusive_zone(-1);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                layer.set_size(0, 0);
            }
            Kind::Panel => {
                if let Some(cfg) = s.panels.get(idx) {
                    shape_panel(&layer, cfg);
                }
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            Kind::Dock => {
                shape_dock(&layer, s, &self.apps, &self.prev_status.running, false);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            Kind::Cards => {
                shape_cards(&layer, s, &self.notes.on_screen());
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            Kind::Osd => {
                layer.set_anchor(Anchor::BOTTOM);
                layer.set_size(crate::cc::OSD_W as u32, crate::cc::OSD_H as u32);
                // Clear of the panel and the dock (a zone of 0 keeps out of others' room).
                layer.set_exclusive_zone(0);
                layer.set_margin(0, 0, 24, 0);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                // Clicks go through it.
                if let Ok(region) = smithay_client_toolkit::compositor::Region::new(&self.compositor) {
                    layer.wl_surface().set_input_region(Some(region.wl_region()));
                }
            }
            Kind::Dim => {
                layer.set_anchor(Anchor::all());
                layer.set_exclusive_zone(-1);
                layer.set_size(0, 0);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
                // Input goes through to the compositor (which ends idle).
                if let Ok(region) = smithay_client_toolkit::compositor::Region::new(&self.compositor) {
                    layer.wl_surface().set_input_region(Some(region.wl_region()));
                }
            }
            Kind::Popup => {
                layer.set_anchor(Anchor::all());
                layer.set_exclusive_zone(-1);
                layer.set_size(0, 0);
                let kb = if self.popup.as_ref().is_some_and(Popup::wants_keyboard) { KeyboardInteractivity::Exclusive } else { KeyboardInteractivity::OnDemand };
                layer.set_keyboard_interactivity(kb);
            }
        }
        layer.wl_surface().set_buffer_scale(scale);
        layer.commit();
        let pool = SlotPool::new(256 * 256 * 4, &self.shm).expect("shared memory for a surface");
        // A popup takes the last one's fonts and icons (making the launcher's icons again is
        // what made it slow to open).
        let (ctx, kit, tex, start) = match self.spare.take().filter(|_| kind == Kind::Popup) {
            Some(parts) => parts,
            None => (egui::Context::default(), Kit::new(true), soft::Textures::default(), Instant::now()),
        };
        Surf {
            kind,
            idx,
            layer,
            output: output.cloned(),
            output_name: name,
            size: (0, 0),
            scale,
            configured: false,
            dirty: true,
            waiting_frame: false,
            repaint_at: None,
            ctx,
            kit,
            tex,
            pool,
            events: Vec::new(),
            modifiers: egui::Modifiers::NONE,
            start,
            slide: 0.0,
            slide_at: Instant::now(),
            collapsed: false,
            hovered: false,
            left_at: None,
            last: Vec::new(),
        }
    }

    /// Become the notification service, or stop being it, as the settings say.
    fn set_notifications(&mut self) {
        if !self.s.notifications {
            self.note_server = None;
            self.notes = Notes::default();
            self.sync_cards();
            return;
        }
        if self.note_server.is_some() {
            return;
        }
        let tx = self.tx.clone();
        match notes::Server::start(move |m| {
            let _ = tx.send(Msg::Note(m));
        }) {
            Ok(s) => self.note_server = Some(s),
            Err(e) => eprintln!("rivet-shell: notifications: {e}"),
        }
    }

    /// The notification cards' surface: made, resized or taken away to fit the cards on screen.
    fn sync_cards(&mut self) {
        let on_screen = self.notes.on_screen();
        if on_screen.is_empty() {
            self.surfs.retain(|s| s.kind != Kind::Cards);
            return;
        }
        let s = self.s.clone();
        let mut have = false;
        for sf in self.surfs.iter_mut().filter(|x| x.kind == Kind::Cards) {
            have = true;
            shape_cards(&sf.layer, &s, &on_screen);
            sf.layer.commit();
            sf.dirty = true;
        }
        if !have {
            let c = self.new_surf(Kind::Cards, None, 0);
            self.surfs.push(c);
        }
    }

    /// Show the on-screen display for a moment.
    fn show_osd(&mut self, kind: OsdKind, value: u32, muted: bool) {
        if !self.s.osd {
            return;
        }
        self.osd = Some((kind, value, muted));
        self.osd_until = Some(Instant::now() + Duration::from_millis(1600));
        if self.surfs.iter().any(|s| s.kind == Kind::Osd) {
            self.redraw_kinds(&[Kind::Osd]);
        } else {
            let o = self.new_surf(Kind::Osd, None, 0);
            self.surfs.push(o);
        }
    }

    /// Keep the screen awake: an idle inhibitor on the panel (or dock, or wallpaper) surface,
    /// which the compositor honours while that surface shows.
    fn set_awake(&mut self, on: bool) {
        if let Some(i) = self.inhibitor.take() {
            i.destroy();
        }
        if !on {
            return;
        }
        let Some(mgr) = &self.inhibit_mgr else { return };
        let host = [Kind::Panel, Kind::Dock, Kind::Background].iter().find_map(|k| self.surfs.iter().find(|s| s.kind == *k));
        if let Some(h) = host {
            self.inhibitor = Some(mgr.create_inhibitor(h.layer.wl_surface(), &self.qh, ()));
        }
    }

    /// Watch for idle as the settings say: one notification per step with a time.
    fn set_idle(&mut self) {
        for n in self.idle_notes.drain(..) {
            n.destroy();
        }
        let (Some(notifier), Some(seat)) = (&self.idle_notifier, &self.seat) else { return };
        if !self.s.idle_on {
            return;
        }
        for st in Stage::ALL {
            let secs = st.secs(&self.s);
            if secs > 0 {
                self.idle_notes.push(notifier.get_idle_notification(secs.saturating_mul(1000), seat, &self.qh, st));
            }
        }
    }

    /// A step of idle reached (`on`), or the person is back.
    fn idle(&mut self, st: Stage, on: bool) {
        match (st, on) {
            (Stage::Dim, true) => {
                if self.dim_at.is_none() {
                    self.dim_at = Some(Instant::now());
                    for o in self.output_state.outputs().collect::<Vec<_>>() {
                        let d = self.new_surf(Kind::Dim, Some(&o), 0);
                        self.surfs.push(d);
                    }
                }
            }
            (Stage::Dim, false) => {
                self.dim_at = None;
                self.surfs.retain(|s| s.kind != Kind::Dim);
            }
            (Stage::Lock, true) => self.locker.lock(&self.s),
            (Stage::ScreenOff, true) => {
                let names: Vec<String> = self.output_state.outputs().filter_map(|o| self.output_state.info(&o).and_then(|i| i.name)).collect();
                idle::screens(self.comp, false, &names);
                self.screens_off = Some(names);
            }
            (Stage::ScreenOff, false) => {
                if let Some(names) = self.screens_off.take() {
                    idle::screens(self.comp, true, &names);
                }
            }
            (Stage::Suspend, true) => sys::spawn("systemctl", &["suspend"]),
            _ => {}
        }
    }

    /// Kept awake while something plays (when the settings ask), like Keep Awake.
    fn set_media_awake(&mut self) {
        let want = self.s.idle_on && self.s.idle_media && self.prev_status.media.as_ref().is_some_and(|m| m.playing);
        if want == self.media_inhibitor.is_some() {
            return;
        }
        if let Some(i) = self.media_inhibitor.take() {
            i.destroy();
        }
        let Some(mgr) = &self.inhibit_mgr else { return };
        if want {
            let host = [Kind::Panel, Kind::Dock, Kind::Background].iter().find_map(|k| self.surfs.iter().find(|s| s.kind == *k));
            if let Some(h) = host {
                self.media_inhibitor = Some(mgr.create_inhibitor(h.layer.wl_surface(), &self.qh, ()));
            }
        }
    }

    /// Settings changed by the shell itself (pins, dark style): saved, then taken in like the
    /// Settings window's changes.
    fn change_settings(&mut self, f: impl FnOnce(&mut Settings)) {
        let mut new = self.s.clone();
        f(&mut new);
        new.save();
        self.reload_settings(false);
    }

    /// The app list, read again on a thread.
    fn reload_apps(&self) {
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Msg::Apps(load_apps()));
        });
    }

    /// The panel and dock for a screen, as the settings ask.
    fn bars_for(&mut self, output: &wl_output::WlOutput) {
        let name = self.output_state.info(output).and_then(|i| i.name).unwrap_or_default();
        for idx in 0..self.s.panels.len() {
            if self.s.panels[idx].on(&name) {
                let p = self.new_surf(Kind::Panel, Some(output), idx);
                self.surfs.push(p);
            }
        }
        if self.s.dock {
            let d = self.new_surf(Kind::Dock, Some(output), 0);
            self.surfs.push(d);
        }
    }

    /// The panels follow their settings: a panel no longer on a screen goes, a new one comes,
    /// a moved or reshaped one is reshaped in place, one whose items changed is drawn again.
    fn sync_panels(&mut self, old: &[crate::panels::PanelCfg]) {
        let new = self.s.panels.clone();
        self.surfs.retain(|sf| sf.kind != Kind::Panel || new.get(sf.idx).is_some_and(|c| c.on(&sf.output_name)));
        for sf in self.surfs.iter_mut().filter(|x| x.kind == Kind::Panel) {
            let (a, b) = (old.get(sf.idx), &new[sf.idx]);
            if a.is_none_or(|a| a.edge != b.edge || a.floating != b.floating) {
                shape_panel(&sf.layer, b);
                sf.layer.commit();
                sf.dirty = true;
            } else if a != Some(b) {
                sf.dirty = true;
            }
        }
        for o in self.output_state.outputs().collect::<Vec<_>>() {
            let name = self.output_state.info(&o).and_then(|i| i.name).unwrap_or_default();
            for (idx, cfg) in new.iter().enumerate() {
                let have = self.surfs.iter().any(|sf| sf.kind == Kind::Panel && sf.idx == idx && sf.output.as_ref() == Some(&o));
                if cfg.on(&name) && !have {
                    let p = self.new_surf(Kind::Panel, Some(&o), idx);
                    self.surfs.push(p);
                }
            }
        }
        if self.inhibitor.is_some() {
            self.set_awake(true);
        }
        if let Some(i) = self.media_inhibitor.take() {
            i.destroy();
        }
    }

    /// Docks on every screen, or none: only those surfaces come or go.
    fn set_bars(&mut self, kind: Kind, on: bool) {
        let have = self.surfs.iter().any(|s| s.kind == kind);
        if on && !have {
            for o in self.output_state.outputs().collect::<Vec<_>>() {
                let s = self.new_surf(kind, Some(&o), 0);
                self.surfs.push(s);
            }
        } else if !on {
            self.surfs.retain(|s| s.kind != kind);
        }
        // Keep awake rides on the panel's surface: onto the new one.
        if self.inhibitor.is_some() {
            self.set_awake(true);
        }
        if let Some(i) = self.media_inhibitor.take() {
            i.destroy();
        }
    }

    /// Mark the surfaces of these kinds to be drawn again (and only those).
    fn redraw_kinds(&mut self, kinds: &[Kind]) {
        for s in self.surfs.iter_mut().filter(|s| kinds.contains(&s.kind)) {
            s.dirty = true;
        }
    }

    fn open_popup(&mut self, p: Popup) {
        // The same popup again closes it.
        if self.popup.as_ref().is_some_and(|o| std::mem::discriminant(&o.kind) == std::mem::discriminant(&p.kind)) {
            self.close_popup();
            return;
        }
        self.close_popup();
        let output = self.output_state.outputs().find(|o| self.output_state.info(o).and_then(|i| i.name).is_some_and(|n| !p.output.is_empty() && n == p.output));
        if matches!(p.kind, PopupKind::Launcher { .. }) {
            // Opens with the list it has; a fresh one (new installs) follows in a moment.
            self.reload_apps();
        }
        if matches!(p.kind, PopupKind::Control { .. } | PopupKind::Network { .. } | PopupKind::Bluetooth) {
            self.live.refresh(true);
        }
        if p.kind == PopupKind::Ai {
            self.ai.refresh(false);
        }
        self.audio.want(matches!(p.kind, PopupKind::Sound { .. }));
        self.radio.want(match p.kind {
            PopupKind::Network { .. } => crate::radio::WIFI,
            PopupKind::Bluetooth => crate::radio::BT,
            _ => crate::radio::NONE,
        });
        self.popup = Some(p);
        let s = self.new_surf(Kind::Popup, output.as_ref(), 0);
        self.surfs.push(s);
    }

    /// The compositor's window look, from the settings and the shell's accent (its overrides
    /// file is rewritten only when it would change).
    fn apply_comp(&self) {
        if let Err(e) = crate::comp::apply_now(self.comp, &self.s) {
            eprintln!("rivet-shell: window look: {e}");
        }
    }

    /// The AI item's readings run while a panel shows it.
    fn set_ai(&self) {
        self.ai.set(self.s.panels.iter().any(|p| p.has("ai")), &self.s.ai_providers);
    }

    fn close_popup(&mut self) {
        self.popup = None;
        self.radio.want(crate::radio::NONE);
        self.audio.want(false);
        if let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Popup) {
            let s = self.surfs.remove(i);
            self.spare = Some((s.ctx, s.kit, s.tex, s.start));
        }
    }

    fn index_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfs.iter().position(|s| s.layer.wl_surface() == surface)
    }

    // ---------------------------------------------------------------- the loop

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Wake => self.status_changed(),
            Msg::LockKey { caps, on } => {
                if self.s.osd_keys {
                    self.show_osd(if caps { OsdKind::CapsLock } else { OsdKind::NumLock }, on as u32, false);
                }
            }
            Msg::Apps(apps) => {
                let key = |l: &[rivet_xdg::launch::DesktopApp]| l.iter().map(|a| format!("{}\t{}\t{}", a.id, a.name, a.icon)).collect::<Vec<_>>();
                if key(&apps) != key(&self.apps) {
                    self.apps = apps;
                    self.reshape_dock(true);
                    self.redraw_kinds(&[Kind::Popup, Kind::Dock]);
                }
            }
            Msg::Note(NoteMsg::New(n)) => {
                // Do Not Disturb keeps them off the screen (all but the urgent), not out of the history.
                let card = !self.dnd || n.urgency >= 2;
                self.notes.add(n, card);
                self.sync_cards();
                self.redraw_kinds(&[Kind::Panel, Kind::Popup]);
            }
            Msg::Note(NoteMsg::Close(id)) => {
                if self.notes.get(id).is_some() {
                    self.notes.remove(id);
                    if let Some(s) = &self.note_server {
                        s.closed(id, Reason::Asked);
                    }
                    self.sync_cards();
                    self.redraw_kinds(&[Kind::Panel, Kind::Popup]);
                }
            }
            Msg::Ipc(line) => {
                let (cmd, arg) = line.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((line.trim(), ""));
                let right = f32::MAX;
                match (cmd, arg) {
                    ("launcher", _) => self.open_popup(Popup::launcher("")),
                    ("control-center" | "control" | "notifications", _) => self.open_popup(Popup::at(PopupKind::Control { drag: None }, right, "")),
                    ("calendar", _) => self.open_popup(Popup::at(PopupKind::Calendar { month: 0 }, f32::NAN, "")),
                    ("power", _) => self.open_popup(Popup::at(PopupKind::Power { confirm: None }, right, "")),
                    ("network" | "wifi", _) => self.open_popup(Popup::at(PopupKind::Network { ask: None }, right, "")),
                    ("bluetooth", _) => self.open_popup(Popup::at(PopupKind::Bluetooth, right, "")),
                    ("ai", _) => self.open_popup(Popup::at(PopupKind::Ai, right, "")),
                    ("sound" | "audio", _) => self.open_popup(Popup::at(PopupKind::Sound { drag: None }, right, "")),
                    ("volume", "up" | "down") => self.act(Act::Volume(arg == "up")),
                    ("volume", "mute") => self.act(Act::Mute),
                    ("mic", "mute") => self.act(Act::MicMute),
                    ("volume", n) if n.parse::<u32>().is_ok() => self.act(Act::SetVolume(n.parse().unwrap_or(50))),
                    ("brightness", "up" | "down") => self.act(Act::Brightness(arg == "up")),
                    ("brightness", n) if n.parse::<u32>().is_ok() => self.act(Act::SetBrightness(n.parse().unwrap_or(50))),
                    ("media", "play-pause" | "next" | "previous" | "play" | "pause" | "stop") => {
                        let c: &'static str = match arg {
                            "next" => "next",
                            "previous" => "previous",
                            "play" => "play",
                            "pause" => "pause",
                            "stop" => "stop",
                            _ => "play-pause",
                        };
                        self.act(Act::Media(c));
                    }
                    ("dnd", _) => self.act(Act::Dnd(!self.dnd)),
                    ("night-light", _) => {
                        let on = self.night.on();
                        self.act(Act::NightLight(!on));
                    }
                    ("keep-awake", _) => self.act(Act::KeepAwake(self.inhibitor.is_none())),
                    ("lock", _) => self.locker.lock(&self.s),
                    ("unlock", _) => self.locker.unlock(),
                    ("settings", _) => open_settings(),
                    ("reload", _) => self.reload_settings(true),
                    ("wallpaper", "random" | "next" | "previous") => {
                        // A picture from the wallpaper folder (the Settings window's), on a thread:
                        // a big folder takes a moment to list.
                        let dir = PathBuf::from(&self.s.wallpaper_dir);
                        let (now, how, tx) = (PathBuf::from(&self.s.wallpaper), arg.to_string(), self.tx.clone());
                        std::thread::spawn(move || match crate::wall::pick(&dir, &now, &how) {
                            Some(p) => {
                                let _ = tx.send(Msg::Ipc(format!("wallpaper {}", p.display())));
                            }
                            None => eprintln!("rivet-shell: no pictures in {} (JPEG, PNG or WebP)", dir.display()),
                        });
                    }
                    ("wallpaper", _) if !arg.is_empty() => {
                        self.s.wallpaper = arg.to_string();
                        self.s.save();
                        self.s_mtime = crate::config::mtime();
                        self.wallpaper_changed(true);
                    }
                    ("quit", _) => self.exit = true,
                    _ => eprintln!("rivet-shell: unknown command {line:?}"),
                }
            }
            Msg::Wall { path, size, pixels } => {
                self.loading.retain(|(p, s)| !(*p == path && *s == size));
                if let Some(pixels) = pixels {
                    self.walls.retain(|w| !(w.path == path && w.size == size));
                    self.walls.push(WallCache { path, size, pixels });
                }
                for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Background) {
                    s.dirty = true;
                }
            }
            Msg::Matugen(r) => {
                if let Err(e) = r {
                    eprintln!("rivet-shell: matugen: {e}");
                }
            }
        }
    }

    /// Draw the launcher once, unseen, at the start: its fonts and the first screen of app icons
    /// are made then, and the first time it opens it takes them over (as later openings do).
    fn prewarm(&mut self) {
        if self.prewarmed || self.spare.is_some() || self.popup.is_some() {
            return;
        }
        let Some(info) = self.output_state.outputs().next().and_then(|o| self.output_state.info(&o)) else { return };
        self.prewarmed = true;
        let scale = info.scale_factor.max(1) as f32;
        let (w, h) = info.logical_size.map(|(w, h)| (w as f32, h as f32)).unwrap_or((1280.0, 720.0));
        let ctx = egui::Context::default();
        let mut kit = Kit::new(true);
        let mut tex = soft::Textures::default();
        let mut raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w, h))), ..Default::default() };
        if let Some(v) = raw.viewports.get_mut(&ViewportId::ROOT) {
            v.native_pixels_per_point = Some(scale);
        }
        let status = self.live.get();
        let extra = Extra::default();
        let mut pop = Popup::launcher("");
        let mut acts = Vec::new();
        let out = ctx.run_ui(raw, |ui| {
            kit.begin(ui.ctx(), &self.s);
            let mut m = Model { s: &self.s, status: &status, extra: &extra, apps: &self.apps, compositor: self.comp, output: "", acts: &mut acts };
            ui::popup(ui, &mut pop, &mut m, &mut kit);
        });
        let mut delta = out.textures_delta;
        tex.update(&delta);
        delta.clear();
        self.spare = Some((ctx, kit, tex, Instant::now()));
    }

    /// Twice a second: settings, the desktop's colours, the status, the clock, repaints asked for.
    fn tick(&mut self) {
        if crate::config::mtime() != self.s_mtime {
            self.reload_settings(false);
        }
        self.prewarm();
        self.locker.check(&self.s);
        self.set_media_awake();
        let now = Instant::now();
        // The colour files' dates only (reading them again is the surfaces' job, when they change).
        if self.palette_watch.iter().any(|(p, t)| mtime(p) != *t) {
            self.palette_watch = palette_files();
            // Borders that follow the accent follow the new colours.
            self.apply_comp();
            // Each surface checks the colours as it draws (every 2 s): keep drawing for a moment.
            self.recolor_until = Some(now + Duration::from_secs(3));
        }
        let recolor = self.recolor_until.is_some_and(|t| now < t);
        self.status_changed();
        let minute = chrono::Timelike::minute(&chrono::Local::now());
        let new_minute = minute != self.last_minute;
        self.last_minute = minute;
        for s in &mut self.surfs {
            // The clock, and the cards' "5 min ago".
            let clocked = matches!(s.kind, Kind::Panel | Kind::Cards) || (s.kind == Kind::Background && self.s.widget_clock) || (s.kind == Kind::Popup && self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Control { .. })));
            if recolor || (new_minute && clocked) || s.repaint_at.is_some_and(|t| now >= t) {
                s.dirty = true;
            }
        }
        // Cards whose time is up, and the on-screen display.
        let gone = self.notes.expire(now);
        if !gone.is_empty() {
            if let Some(srv) = &self.note_server {
                for id in &gone {
                    srv.closed(*id, Reason::Expired);
                }
            }
            self.sync_cards();
        }
        if self.osd_until.is_some_and(|t| now >= t) {
            self.osd_until = None;
            self.osd = None;
            self.surfs.retain(|s| s.kind != Kind::Osd);
        }
    }

    /// The status thread read something new: draw again only the surfaces that show it.
    fn status_changed(&mut self) {
        let ag = self.ai.generation();
        if ag != self.ai_gen {
            self.ai_gen = ag;
            let pop = self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Ai);
            self.redraw_kinds(if pop { &[Kind::Panel, Kind::Popup] } else { &[Kind::Panel] });
        }
        let ag = self.audio.generation();
        if ag != self.audio_gen {
            self.audio_gen = ag;
            if self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Sound { .. })) {
                self.redraw_kinds(&[Kind::Popup]);
            }
        }
        let rg = self.radio.generation();
        if rg != self.radio_gen {
            self.radio_gen = rg;
            if self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Network { .. } | PopupKind::Bluetooth)) {
                self.redraw_kinds(&[Kind::Popup]);
            }
        }
        let generation = self.live.generation();
        if generation == self.last_gen {
            return;
        }
        self.last_gen = generation;
        // The wallpaper's widgets only change with what they show, in the units they show it
        // (whole percent, tenths of a gigabyte): not every time the processor reading wiggles.
        let st = self.live.get();
        let bg_key = (st.cpu.round() as u32, st.mem.0 / (100 << 20), st.battery);
        let bg_changed = bg_key != self.bg_key;
        self.bg_key = bg_key;
        // Likewise the panel and the dock (open apps).
        let prev = std::mem::replace(&mut self.prev_status, st.clone());
        let panel_changed = st.workspaces != prev.workspaces || st.volume != prev.volume || st.net != prev.net || st.battery != prev.battery || st.window != prev.window || st.media != prev.media || st.brightness != prev.brightness || (bg_changed && self.s.panels.iter().any(|p| p.has("system")));
        let dock_changed = st.running != prev.running;
        let control_open = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Control { .. } | PopupKind::Network { .. } | PopupKind::Bluetooth));
        let control_changed = st.volume != prev.volume || st.mic != prev.mic || st.brightness != prev.brightness || st.media != prev.media || st.net != prev.net || st.wifi_on != prev.wifi_on || st.bluetooth != prev.bluetooth || st.profile != prev.profile;
        for s in &mut self.surfs {
            let shows = match s.kind {
                Kind::Panel => panel_changed,
                Kind::Dock => dock_changed,
                Kind::Background => self.s.widget_system && bg_changed,
                Kind::Popup => (control_open && control_changed) || matches!(self.popup.as_ref().map(|p| &p.kind), Some(PopupKind::DockMenu { .. })) && dock_changed,
                Kind::Cards | Kind::Osd | Kind::Dim => false,
            };
            s.dirty |= shows;
        }
        if dock_changed {
            self.reshape_dock(false);
        }
        // Volume and brightness changes show in the middle of the screen's bottom (not while
        // the control center's sliders are moving them, and not at the start).
        if !control_open && prev != sys::Status::default() {
            if let (Some(a), Some(b)) = (prev.volume, st.volume)
                && a != b
            {
                self.show_osd(OsdKind::Volume, b.0, b.1);
            } else if let (Some(a), Some(b)) = (prev.brightness, st.brightness)
                && a != b
            {
                self.show_osd(OsdKind::Brightness, b, false);
            } else if let (Some(a), Some(b)) = (prev.mic, st.mic)
                && a.1 != b.1
            {
                self.show_osd(OsdKind::Mic, b.0, b.1);
            }
        }
    }

    /// The dock's size follows its items (open apps come and go): reshaped only when it must.
    fn reshape_dock(&mut self, force: bool) {
        let items = ui::dock_items(&self.s, &self.apps, &self.prev_status.running);
        let key = (items.len(), items.iter().any(|i| i.1) && items.iter().any(|i| !i.1));
        if key == self.dock_key && !force {
            return;
        }
        self.dock_key = key;
        let (s, apps, running) = (self.s.clone(), self.apps.clone(), self.prev_status.running.clone());
        for sf in self.surfs.iter_mut().filter(|x| x.kind == Kind::Dock) {
            shape_dock(&sf.layer, &s, &apps, &running, sf.collapsed);
            sf.layer.commit();
            sf.dirty = true;
        }
    }

    fn reload_settings(&mut self, force: bool) {
        self.s_mtime = crate::config::mtime();
        let new = Settings::load();
        if new == self.s && !force {
            return;
        }
        let old = std::mem::replace(&mut self.s, new);
        if old.panels != self.s.panels || old.ai_providers != self.s.ai_providers {
            self.set_ai();
        }
        if idle::key(&old) != idle::key(&self.s) {
            self.set_idle();
        }
        self.lock_on_sleep.store(self.s.lock_on_sleep, std::sync::atomic::Ordering::Relaxed);
        self.own_locker.store(self.s.lock_cmd.trim().is_empty(), std::sync::atomic::Ordering::Relaxed);
        self.apply_comp();
        let s = self.s.clone();
        // Only what changed changes: surfaces are reshaped in place (never torn down and made
        // again, which makes them flash and moves the windows twice), and only the surfaces that
        // show a setting are drawn again.
        let look = old.theme != s.theme || old.accent != s.accent || old.follow_desktop_colors != s.follow_desktop_colors || old.text_size != s.text_size;
        let wall_changed = old.wallpaper != s.wallpaper || old.color_mode != s.color_mode || old.scheme_type != s.scheme_type;
        let panels_changed = old.panels != s.panels;
        let panel_content = old.clock_format != s.clock_format || old.ai_bar != s.ai_bar || old.ai_pins != s.ai_pins || old.ai_left != s.ai_left || old.ws_style != s.ws_style || old.ws_names != s.ws_names || old.ws_hide_empty != s.ws_hide_empty || old.ws_scroll != s.ws_scroll || old.notifications != s.notifications;
        let dock_shape = old.dock_apps != s.dock_apps || old.dock_icon != s.dock_icon || old.dock_reserve != s.dock_reserve || old.dock_open_apps != s.dock_open_apps || old.dock_hide != s.dock_hide;
        if old.notifications != s.notifications {
            self.set_notifications();
        }
        if old.night_kelvin != s.night_kelvin && self.night.on() {
            self.night.set(true, s.night_kelvin);
        }
        if old.color_mode != s.color_mode {
            // The control center's Dark Style tile.
            self.redraw_kinds(&[Kind::Popup]);
        }
        let widgets = old.widget_clock != s.widget_clock || old.widget_system != s.widget_system || old.widget_side != s.widget_side;
        if old.dock_apps != s.dock_apps && s.dock_apps.iter().any(|id| ui::find_app(&self.apps, id).is_none()) {
            // A newly pinned app this list doesn't know yet.
            self.apps = load_apps();
        }
        if look || force {
            self.redraw_kinds(&[Kind::Background, Kind::Panel, Kind::Dock, Kind::Popup]);
        }
        // The panel: on or off, moved or reshaped, or just drawn again.
        if panels_changed {
            self.sync_panels(&old.panels);
        }
        if panel_content {
            self.redraw_kinds(&[Kind::Panel]);
        }
        // The dock likewise.
        if old.dock != s.dock {
            self.set_bars(Kind::Dock, s.dock);
        } else if dock_shape {
            self.reshape_dock(true);
        } else if old.dock_running != s.dock_running {
            self.redraw_kinds(&[Kind::Dock]);
        }
        // The wallpaper's widgets sit below the panel: they move with it.
        if widgets || panels_changed {
            self.redraw_kinds(&[Kind::Background]);
        }
        if wall_changed {
            self.wallpaper_changed(true);
        }
    }

    /// The wallpaper setting changed (or the shell started): load it, and give matugen the new
    /// picture so every app takes its colours.
    fn wallpaper_changed(&mut self, run_matugen: bool) {
        self.walls.clear();
        for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Background) {
            s.dirty = true;
        }
        let path = PathBuf::from(&self.s.wallpaper);
        if self.s.wallpaper.is_empty() || !path.is_file() {
            return;
        }
        if run_matugen && self.s.matugen {
            let tx = self.tx.clone();
            crate::wall::run_matugen(&path, self.s.color_mode, &self.s.scheme_type, move |r| {
                let _ = tx.send(Msg::Matugen(r));
            });
        }
    }

    /// The docks show or slide away as the hide setting says: shown while the pointer is on one
    /// (and a moment after it leaves) or its menu is open; with Smart, also while the screen's
    /// workspace has no windows. Out of sight, a dock shrinks to a thin strip along the edge,
    /// which the pointer reaching brings it back.
    fn update_docks(&mut self) {
        let mode = self.s.dock_hide;
        let now = Instant::now();
        let menu = matches!(self.popup.as_ref().map(|p| &p.kind), Some(PopupKind::DockMenu { .. }));
        let (s, apps, running) = (&self.s, &self.apps, &self.prev_status.running);
        let workspaces = &self.prev_status.workspaces;
        for sf in self.surfs.iter_mut().filter(|x| x.kind == Kind::Dock) {
            if sf.left_at.is_some_and(|t| now.duration_since(t) >= DOCK_LINGER) {
                sf.left_at = None;
            }
            let near = sf.hovered || sf.left_at.is_some() || menu;
            // Windows on this screen's workspace (a compositor that doesn't say: as if there were).
            let here: Vec<_> = workspaces.iter().filter(|w| w.active && (w.output.is_empty() || sf.output_name.is_empty() || w.output == sf.output_name)).collect();
            let busy = here.is_empty() || here.iter().any(|w| w.busy);
            let show = match mode {
                DockHide::Never => true,
                DockHide::Auto => near,
                DockHide::Smart => near || !busy,
            };
            if show && sf.collapsed {
                // Full size again (still out of sight), then slide in.
                sf.collapsed = false;
                sf.slide = 1.0;
                sf.slide_at = now;
                shape_dock(&sf.layer, s, apps, running, false);
                sf.layer.commit();
                sf.dirty = true;
                continue;
            }
            let target = if show { 0.0 } else { 1.0 };
            if sf.slide != target && !sf.collapsed {
                let step = now.duration_since(sf.slide_at).as_secs_f32().min(0.05) / DOCK_SLIDE;
                sf.slide = if target > sf.slide { (sf.slide + step).min(1.0) } else { (sf.slide - step).max(0.0) };
                sf.dirty = true;
                sf.repaint_at = Some(now + Duration::from_millis(16));
            }
            sf.slide_at = now;
            if !show && sf.slide >= 1.0 && !sf.collapsed {
                sf.collapsed = true;
                shape_dock(&sf.layer, s, apps, running, true);
                sf.layer.commit();
                sf.dirty = true;
            }
        }
    }

    /// Draw every surface that needs it.
    fn redraw(&mut self) {
        self.update_docks();
        let now = Instant::now();
        for s in &mut self.surfs {
            if s.repaint_at.is_some_and(|t| now >= t) {
                s.repaint_at = None;
                s.dirty = true;
            }
        }
        for i in 0..self.surfs.len() {
            if self.surfs[i].dirty && self.surfs[i].configured && !self.surfs[i].waiting_frame {
                self.draw(i);
            }
        }
    }

    /// Which cached wallpaper fits this size (loading it in the background when none does).
    fn wall_for(&mut self, size: (u32, u32)) -> Option<usize> {
        if self.s.wallpaper.is_empty() {
            return None;
        }
        let path = PathBuf::from(&self.s.wallpaper);
        if let Some(i) = self.walls.iter().position(|w| w.path == path && w.size == size) {
            return Some(i);
        }
        if !self.loading.iter().any(|(p, s)| *p == path && *s == size) {
            self.loading.push((path.clone(), size));
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let pixels = crate::wall::cover_bgra(&path, size.0, size.1);
                if pixels.is_none() {
                    eprintln!("rivet-shell: couldn't read the wallpaper {}", path.display());
                }
                let _ = tx.send(Msg::Wall { path, size, pixels });
            });
        }
        None
    }

    fn draw(&mut self, i: usize) {
        let status = self.live.get();
        let mut acts: Vec<Act> = Vec::new();
        let kind = self.surfs[i].kind;
        let idx = self.surfs[i].idx;
        let (w, h) = self.surfs[i].size;
        if w == 0 || h == 0 {
            return;
        }
        let scale = self.surfs[i].scale;
        let (pw, ph) = self.surfs[i].physical();
        let wall_idx = if kind == Kind::Background { self.wall_for((pw, ph)) } else { None };
        let night = self.night_ok.then(|| self.night.on());
        // Dimming: darker over a second and a half.
        let dim = self.dim_at.map(|t| (t.elapsed().as_secs_f32() / 1.5).min(1.0)).unwrap_or(0.0);
        let extra = Extra {
            notes: if matches!(kind, Kind::Panel | Kind::Popup) { self.notes.list.clone() } else { Vec::new() },
            cards: if kind == Kind::Cards { self.notes.on_screen() } else { Vec::new() },
            notifying: self.note_server.is_some(),
            dnd: self.dnd,
            night,
            awake: self.inhibit_mgr.as_ref().map(|_| self.inhibitor.is_some()),
            osd: self.osd,
            user: self.user.clone(),
            radios: if kind == Kind::Popup { self.radio.get() } else { Default::default() },
            audio: if kind == Kind::Popup { self.audio.get() } else { Default::default() },
            ai: if matches!(kind, Kind::Panel | Kind::Popup) { self.ai.get() } else { Default::default() },
        };
        let Shell { surfs, s: settings, apps, popup, comp, walls, .. } = self;
        let wall: Option<&[u8]> = wall_idx.and_then(|i| walls.get(i)).map(|w| w.pixels.as_slice());
        let surf = &mut surfs[i];
        surf.dirty = false;
        let mut raw = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32))),
            time: Some(surf.start.elapsed().as_secs_f64()),
            predicted_dt: 1.0 / 60.0,
            focused: true,
            events: std::mem::take(&mut surf.events),
            ..Default::default()
        };
        if let Some(v) = raw.viewports.get_mut(&ViewportId::ROOT) {
            v.native_pixels_per_point = Some(scale as f32);
            v.focused = Some(true);
        }
        let output_name = surf.output_name.clone();
        let (slide, collapsed) = (surf.slide, surf.collapsed);
        let mut plain = egui::Color32::from_rgb(30, 30, 34);
        let t_u0 = Instant::now();
        let out = surf.ctx.run_ui(raw, |ui| {
            surf.kit.begin(ui.ctx(), settings);
            plain = ui.visuals().extreme_bg_color;
            let mut m = Model { s: settings, status: &status, extra: &extra, apps, compositor: *comp, output: &output_name, acts: &mut acts };
            match kind {
                Kind::Background => ui::widgets(ui, &mut m, &mut surf.kit),
                Kind::Panel => crate::bar::panel(ui, &mut m, &mut surf.kit, idx),
                Kind::Dock => {
                    // Slid part of the way out (a hidden one, down to its strip, shows nothing).
                    if !collapsed {
                        let r = ui.max_rect();
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(r.translate(Vec2::new(0.0, slide * r.height()))));
                        ui::dock(&mut c, &mut m, &mut surf.kit);
                    }
                }
                Kind::Popup => {
                    if let Some(p) = popup.as_mut() {
                        ui::popup(ui, p, &mut m, &mut surf.kit);
                    }
                }
                Kind::Cards => crate::cc::cards(ui, &mut m, &mut surf.kit),
                Kind::Osd => crate::cc::osd(ui, &mut m, &mut surf.kit),
                Kind::Dim => {
                    ui.painter().rect_filled(ui.max_rect(), 0.0, egui::Color32::from_black_alpha((dim * 150.0) as u8));
                }
            }
        });
        // Take in the frame's texture changes (fonts, icons) and mark them handled: egui
        // insists on that before the changes are dropped.
        let t_ui = t_u0.elapsed();
        let mut out = out;
        let mut delta = std::mem::take(&mut out.textures_delta);
        surf.tex.update(&delta);
        delta.clear();
        let t_t0 = Instant::now();
        let prims = surf.ctx.tessellate(out.shapes, scale as f32);
        let t_tess = t_t0.elapsed();
        let stride = pw as i32 * 4;
        let t_b0 = Instant::now();
        let Ok((buffer, canvas)) = surf.pool.create_buffer(pw as i32, ph as i32, stride, wl_shm::Format::Argb8888) else { return };
        let t_buf = t_b0.elapsed();
        match (kind, wall) {
            (Kind::Background, Some(px)) if px.len() == canvas.len() => canvas.copy_from_slice(px),
            (Kind::Background, _) => {
                // No picture (yet): the theme's background, a little lighter at the top.
                let (top, bottom) = (rivet_look::palette::mix(plain, egui::Color32::WHITE, 0.06), plain);
                for y in 0..ph as usize {
                    let c = rivet_look::palette::mix(top, bottom, y as f32 / ph.max(1) as f32);
                    let row = [c.b(), c.g(), c.r(), 255];
                    for px in canvas[y * pw as usize * 4..(y + 1) * pw as usize * 4].as_chunks_mut::<4>().0 {
                        *px = row;
                    }
                }
            }
            _ => canvas.fill(0),
        }
        let t_r0 = Instant::now();
        soft::draw(canvas, pw as usize, ph as usize, soft::Order::Bgra, &prims, &surf.tex, scale as f32);
        // Only the rows that changed since the last frame go to the compositor (typing in the
        // launcher changes a few lines of a screen-sized surface). The wallpaper changes whole.
        let row_bytes = pw as usize * 4;
        let rows = if kind != Kind::Background && surf.last.len() == canvas.len() {
            let differs = |y: usize| canvas[y * row_bytes..(y + 1) * row_bytes] != surf.last[y * row_bytes..(y + 1) * row_bytes];
            let first = (0..ph as usize).find(|&y| differs(y));
            first.map(|f| (f, (f..ph as usize).rev().find(|&y| differs(y)).unwrap_or(f)))
        } else {
            Some((0, ph as usize - 1))
        };
        if kind != Kind::Background {
            surf.last.clear();
            surf.last.extend_from_slice(canvas);
        }
        let ws = surf.layer.wl_surface();
        if std::env::var_os("RIVET_SHELL_TIMING").is_some() {
            eprintln!("draw {:?} {}x{}: ui {:.1} ms, tessellate {:.1} ms, buffer {:.1} ms, raster {:.1} ms, all {:.1} ms, rows {:?}", kind, pw, ph, t_ui.as_secs_f64() * 1e3, t_tess.as_secs_f64() * 1e3, t_buf.as_secs_f64() * 1e3, t_r0.elapsed().as_secs_f64() * 1e3, t_u0.elapsed().as_secs_f64() * 1e3, rows);
        }
        match rows {
            Some((y0, y1)) => {
                ws.damage_buffer(0, y0 as i32, pw as i32, (y1 - y0 + 1) as i32);
                ws.frame(&self.qh, ws.clone());
                if buffer.attach_to(ws).is_ok() {
                    surf.layer.commit();
                    surf.waiting_frame = true;
                }
            }
            // Nothing changed on screen: nothing to send.
            None => drop(buffer),
        }
        // Animations and hover effects ask to be drawn again soon.
        let delay = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        surf.repaint_at = (delay < Duration::from_secs(1)).then(|| Instant::now() + delay.max(Duration::from_millis(16)));
        if kind == Kind::Dim && dim < 1.0 {
            surf.repaint_at = Some(Instant::now() + Duration::from_millis(33));
        }
        // A dock on its way in or out keeps moving.
        if kind == Kind::Dock && surf.slide > 0.0 && !surf.collapsed {
            surf.repaint_at = Some(Instant::now() + Duration::from_millis(16));
        }
        for a in acts {
            self.act(a);
        }
    }

    fn act(&mut self, a: Act) {
        match a {
            Act::Launch(id) => {
                if let Some(app) = ui::find_app(&self.apps, &id).cloned()
                    && let Err(e) = rivet_xdg::launch::launch_app(&app, &[])
                {
                    eprintln!("rivet-shell: couldn't start {}: {e}", app.name);
                }
                self.close_popup();
            }
            Act::Spawn(cmd) => {
                sys::spawn(&cmd, &[]);
                self.close_popup();
            }
            Act::Workspace(k) => sys::focus_workspace(self.comp, &k),
            Act::Open(p) => self.open_popup(p),
            Act::Close => self.close_popup(),
            Act::Volume(up) => {
                sys::change_volume(up);
                self.live.refresh(false);
            }
            Act::SetVolume(v) => {
                sys::set_volume(v);
                self.live.refresh(false);
            }
            Act::Mute => {
                sys::toggle_mute();
                self.live.refresh(false);
            }
            Act::MicMute => {
                sys::toggle_mic();
                self.live.refresh(false);
            }
            Act::Brightness(up) => {
                sys::change_brightness(up);
                self.live.refresh(false);
            }
            Act::SetBrightness(v) => {
                sys::set_brightness(v);
                self.live.refresh(false);
            }
            Act::Media(cmd) => {
                sys::media(cmd);
                self.live.refresh(false);
            }
            Act::Wifi(on) => {
                sys::set_wifi(on);
                self.live.refresh(true);
            }
            Act::Bluetooth(on) => {
                sys::set_bluetooth(on);
                self.live.refresh(true);
                // Turned on with the Bluetooth popup open: look for devices.
                if on && matches!(self.popup.as_ref().map(|p| &p.kind), Some(PopupKind::Bluetooth)) {
                    self.radio.run(crate::radio::Job::BtScan);
                }
            }
            Act::AiRefresh => self.ai.refresh(true),
            Act::AiPin(key) => self.change_settings(|s| {
                if s.ai_pins.contains(&key) {
                    s.ai_pins.retain(|k| k != &key);
                } else {
                    s.ai_pins.push(key.replace(',', " "));
                }
            }),
            Act::Audio(job) => {
                self.audio.run(job);
                self.live.refresh(false);
            }
            Act::Radio(job) => {
                self.radio.run(job);
                self.live.refresh(true);
            }
            Act::NextProfile => {
                if let Some(p) = self.prev_status.profile.clone() {
                    sys::next_profile(&p);
                }
                self.live.refresh(true);
            }
            Act::NightLight(on) => self.night.set(on, self.s.night_kelvin),
            Act::KeepAwake(on) => self.set_awake(on),
            Act::Dnd(on) => {
                self.dnd = on;
                if on {
                    // Off the screen now, all but the urgent.
                    let quiet: Vec<u32> = self.notes.on_screen().iter().filter(|n| n.urgency < 2).map(|n| n.id).collect();
                    for id in quiet {
                        self.notes.uncard(id);
                    }
                    self.sync_cards();
                }
            }
            Act::DarkStyle(dark) => {
                let mode = if dark { rivet_look::theme::Mode::Dark } else { rivet_look::theme::Mode::Light };
                self.change_settings(|s| {
                    s.color_mode = mode;
                    // Without matugen making the colours, the style itself follows.
                    if !(s.matugen && !s.wallpaper.is_empty()) || s.theme != rivet_look::theme::Mode::System {
                        s.theme = mode;
                    }
                });
            }
            Act::Pin(id) => {
                let apps = self.apps.clone();
                self.change_settings(|s| {
                    if !s.dock_apps.iter().any(|d| ui::find_app(&apps, d).is_some_and(|a| a.id.trim_end_matches(".desktop") == id)) {
                        s.dock_apps.push(id);
                    }
                });
            }
            Act::Unpin(id) => {
                let apps = self.apps.clone();
                self.change_settings(|s| s.dock_apps.retain(|d| !(d == &id || ui::find_app(&apps, d).is_some_and(|a| a.id.trim_end_matches(".desktop") == id))));
            }
            Act::MovePin(id, by) => {
                let apps = self.apps.clone();
                self.change_settings(|s| {
                    if let Some(i) = s.dock_apps.iter().position(|d| d == &id || ui::find_app(&apps, d).is_some_and(|a| a.id.trim_end_matches(".desktop") == id)) {
                        let j = (i as i32 + by).clamp(0, s.dock_apps.len() as i32 - 1) as usize;
                        s.dock_apps.swap(i, j);
                    }
                });
            }
            Act::NoteClick(id) => {
                let default = self.notes.get(id).is_some_and(|n| n.actions.iter().any(|(k, _)| k == "default"));
                if default {
                    if let Some(s) = &self.note_server {
                        s.invoke(id, "default");
                        s.closed(id, Reason::Dismissed);
                    }
                    self.notes.remove(id);
                } else {
                    self.notes.uncard(id);
                }
                self.sync_cards();
            }
            Act::NoteAction(id, key) => {
                if let Some(s) = &self.note_server {
                    s.invoke(id, &key);
                    s.closed(id, Reason::Dismissed);
                }
                self.notes.remove(id);
                self.sync_cards();
            }
            Act::NoteHide(id) => {
                self.notes.uncard(id);
                self.sync_cards();
            }
            Act::NoteRemove(id) => {
                if let Some(s) = &self.note_server {
                    s.closed(id, Reason::Dismissed);
                }
                self.notes.remove(id);
                self.sync_cards();
            }
            Act::ClearNotes => {
                if let Some(s) = &self.note_server {
                    for n in &self.notes.list {
                        s.closed(n.id, Reason::Dismissed);
                    }
                }
                self.notes = Notes::default();
                self.sync_cards();
            }
            Act::Copy(text) => {
                sys::copy(&text);
                self.close_popup();
            }
            Act::Power(sys::Power::Lock) => {
                self.close_popup();
                self.locker.lock(&self.s);
            }
            Act::Power(p) => {
                self.close_popup();
                sys::power(self.comp, p);
            }
            Act::Settings => open_settings(),
        }
        // What the shell keeps (notifications, toggles) shows in the panel and the popup; the
        // rest follows the status when it arrives.
        self.redraw_kinds(&[Kind::Panel, Kind::Popup]);
    }
}

/// The Settings window (its own program, so the shell never waits on it).
pub fn open_settings() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).arg("settings").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    }
}

// ------------------------------------------------------------------ Wayland's callbacks

impl CompositorHandler for Shell {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        if let Some(i) = self.index_of(surface) {
            let s = &mut self.surfs[i];
            s.scale = factor.max(1);
            s.layer.wl_surface().set_buffer_scale(s.scale);
            s.dirty = true;
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(i) = self.index_of(surface) {
            self.surfs[i].waiting_frame = false;
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Shell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let bg = self.new_surf(Kind::Background, Some(&output), 0);
        self.surfs.push(bg);
        self.bars_for(&output);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        let name = self.output_state.info(&output).and_then(|i| i.name).unwrap_or_default();
        for s in self.surfs.iter_mut().filter(|s| s.output.as_ref() == Some(&output)) {
            s.output_name = name.clone();
            s.dirty = true;
        }
    }
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.surfs.retain(|s| s.output.as_ref() != Some(&output));
    }
}

impl LayerShellHandler for Shell {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        if self.surfs.iter().any(|s| &s.layer == layer && s.kind == Kind::Popup) {
            self.popup = None;
            self.radio.want(crate::radio::NONE);
            self.audio.want(false);
        }
        self.surfs.retain(|s| &s.layer != layer);
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, cfg: LayerSurfaceConfigure, _: u32) {
        if let Some(s) = self.surfs.iter_mut().find(|s| &s.layer == layer) {
            s.size = (cfg.new_size.0.max(1), cfg.new_size.1.max(1));
            s.configured = true;
            s.waiting_frame = false;
            s.dirty = true;
        }
    }
}

impl SeatHandler for Shell {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if self.seat.is_none() {
            self.seat = Some(seat.clone());
            self.set_idle();
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self.seat_state.get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer {
            self.pointer = None;
        }
        if cap == Capability::Keyboard
            && let Some(k) = self.keyboard.take()
        {
            k.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Shell {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            let Some(i) = self.index_of(&e.surface) else { continue };
            let pos = Pos2::new(e.position.0 as f32, e.position.1 as f32);
            let mods = self.kb_mods;
            let s = &mut self.surfs[i];
            match &e.kind {
                PointerEventKind::Enter { .. } => {
                    s.hovered = true;
                    s.left_at = None;
                    if let Some(p) = &self.pointer {
                        let _ = p.set_cursor(&self.conn, CursorIcon::Default);
                    }
                    s.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => {
                    s.hovered = false;
                    s.left_at = Some(Instant::now());
                    s.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Motion { .. } => s.events.push(egui::Event::PointerMoved(pos)),
                PointerEventKind::Press { button, .. } | PointerEventKind::Release { button, .. } => {
                    let pressed = matches!(e.kind, PointerEventKind::Press { .. });
                    let b = match button {
                        0x110 => egui::PointerButton::Primary,
                        0x111 => egui::PointerButton::Secondary,
                        0x112 => egui::PointerButton::Middle,
                        _ => continue,
                    };
                    s.events.push(egui::Event::PointerButton { pos, button: b, pressed, modifiers: mods });
                }
                PointerEventKind::Axis { horizontal, vertical, .. } => {
                    let delta = Vec2::new(-horizontal.absolute as f32, -vertical.absolute as f32);
                    s.events.push(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, phase: egui::TouchPhase::Move, source: egui::MouseWheelSource::Unknown, modifiers: mods });
                }
            }
            s.dirty = true;
        }
    }
}

fn key_of(k: Keysym) -> Option<egui::Key> {
    Some(match k {
        Keysym::Escape => egui::Key::Escape,
        Keysym::Return | Keysym::KP_Enter => egui::Key::Enter,
        Keysym::BackSpace => egui::Key::Backspace,
        Keysym::Delete => egui::Key::Delete,
        Keysym::Tab => egui::Key::Tab,
        Keysym::Left => egui::Key::ArrowLeft,
        Keysym::Right => egui::Key::ArrowRight,
        Keysym::Up => egui::Key::ArrowUp,
        Keysym::Down => egui::Key::ArrowDown,
        Keysym::Home => egui::Key::Home,
        Keysym::End => egui::Key::End,
        Keysym::a | Keysym::A => egui::Key::A,
        _ => return None,
    })
}

impl Shell {
    fn key(&mut self, e: &KeyEvent, pressed: bool, repeat: bool) {
        let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Popup) else { return };
        let mods = self.kb_mods;
        let s = &mut self.surfs[i];
        if let Some(k) = key_of(e.keysym) {
            s.events.push(egui::Event::Key { key: k, physical_key: None, pressed, repeat, modifiers: mods });
        }
        if pressed
            && !mods.ctrl
            && let Some(t) = e.utf8.as_ref().filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()))
        {
            s.events.push(egui::Event::Text(t.clone()));
        }
        s.dirty = true;
    }
}

impl KeyboardHandler for Shell {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e, true, false);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e, true, true);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e, false, false);
    }
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: KbMods, _: RawModifiers, _: u32) {
        self.kb_mods = egui::Modifiers { alt: m.alt, ctrl: m.ctrl, shift: m.shift, mac_cmd: false, command: m.ctrl };
        for s in &mut self.surfs {
            s.modifiers = self.kb_mods;
        }
    }
}

impl ShmHandler for Shell {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Shell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Shell);
delegate_output!(Shell);
delegate_shm!(Shell);
delegate_seat!(Shell);
delegate_keyboard!(Shell);
delegate_pointer!(Shell);
delegate_layer!(Shell);
delegate_registry!(Shell);

/// The panel's place and size: along the top or bottom edge, keeping windows clear of it.
fn shape_panel(layer: &LayerSurface, p: &crate::panels::PanelCfg) {
    let edge = if p.edge == Edge::Top { Anchor::TOP } else { Anchor::BOTTOM };
    layer.set_anchor(edge | Anchor::LEFT | Anchor::RIGHT);
    let h = p.height() as u32;
    layer.set_size(0, h);
    layer.set_exclusive_zone(h as i32);
}

/// The dock's size for its pinned apps, at the bottom, keeping windows clear of it when asked.
fn shape_dock(layer: &LayerSurface, s: &Settings, apps: &[rivet_xdg::launch::DesktopApp], running: &[String], collapsed: bool) {
    let items = ui::dock_items(s, apps, running);
    let (size, room) = ui::dock_size(s, &items);
    layer.set_anchor(Anchor::BOTTOM);
    // Hidden: a thin strip as wide as the dock (at least a hand's width), to bring it back.
    let h = if collapsed { DOCK_STRIP } else { size.y as u32 };
    let w = if collapsed { (size.x as u32).max(220) } else { size.x as u32 };
    layer.set_size(w, h);
    // A dock that hides never keeps windows clear of it.
    let reserve = s.dock_reserve && s.dock_hide == DockHide::Never;
    layer.set_exclusive_zone(if reserve { room as i32 } else { 0 });
}

/// The notification cards: in the corner on the panel's side, clear of the panel.
fn shape_cards(layer: &LayerSurface, s: &Settings, cards: &[crate::notes::Note]) {
    // On the side of the first panel (the top when there are none).
    let edge = if s.panels.first().is_some_and(|p| p.edge == Edge::Bottom) { Anchor::BOTTOM } else { Anchor::TOP };
    layer.set_anchor(edge | Anchor::RIGHT);
    layer.set_size(crate::cc::CARDS_W as u32, crate::cc::cards_height(cards).ceil() as u32);
    layer.set_exclusive_zone(0);
    layer.set_margin(2, 4, 2, 0);
}

// The idle-inhibit protocol has no events the shell needs.
impl Dispatch<ZwpIdleInhibitManagerV1, ()> for Shell {
    fn event(_: &mut Self, _: &ZwpIdleInhibitManagerV1, _: <ZwpIdleInhibitManagerV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ZwpIdleInhibitorV1, ()> for Shell {
    fn event(_: &mut Self, _: &ZwpIdleInhibitorV1, _: <ZwpIdleInhibitorV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

// Idle: the notifier has no events; each step's notification says idled or resumed.
impl Dispatch<ExtIdleNotifierV1, ()> for Shell {
    fn event(_: &mut Self, _: &ExtIdleNotifierV1, _: <ExtIdleNotifierV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<ExtIdleNotificationV1, Stage> for Shell {
    fn event(shell: &mut Self, _: &ExtIdleNotificationV1, ev: ext_idle_notification_v1::Event, st: &Stage, _: &Connection, _: &QueueHandle<Self>) {
        match ev {
            ext_idle_notification_v1::Event::Idled => shell.idle(*st, true),
            ext_idle_notification_v1::Event::Resumed => shell.idle(*st, false),
            _ => {}
        }
    }
}
