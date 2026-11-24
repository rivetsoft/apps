//! The shell on the desktop: on every screen, a wallpaper (with the desktop widgets) on the
//! background layer, the panel and the dock on the top layer, and popups (launcher, calendar,
//! power) on the overlay layer, through the wlr layer-shell protocol (niri, Hyprland, Sway, river,
//! labwc, Wayfire, COSMIC and others; not GNOME).
//!
//! Each surface is an egui screen drawn in software (`rivet_kit::soft`) into shared memory: no
//! GPU, and only when something changed.

use std::sync::Arc;
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
use smithay_client_toolkit::data_device_manager::DataDeviceManagerState;
use smithay_client_toolkit::data_device_manager::data_device::DataDevice;
use smithay_client_toolkit::data_device_manager::data_offer::DragOffer;
use smithay_client_toolkit::data_device_manager::data_source::DragSource;
use smithay_client_toolkit::{delegate_data_device, delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibit_manager_v1::ZwpIdleInhibitManagerV1;
use wayland_protocols::wp::idle_inhibit::zv1::client::zwp_idle_inhibitor_v1::ZwpIdleInhibitorV1;
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notification_v1::{self, ExtIdleNotificationV1};
use wayland_protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_manager_v1::{self, ExtBackgroundEffectManagerV1};
use wayland_protocols::ext::background_effect::v1::client::ext_background_effect_surface_v1::ExtBackgroundEffectSurfaceV1;
use crate::idle::{self, Stage};

#[path = "dnd.rs"]
mod dnd;

use crate::config::{DockHide, Edge, Settings};

/// A hidden dock's strip along the edge, in points.
const DOCK_STRIP: u32 = 4;
/// How long the dock stays after the pointer leaves, and how long it takes to slide.
const DOCK_LINGER: Duration = Duration::from_millis(700);
const DOCK_SLIDE: f32 = 0.18;
/// How long the pointer rests along a rolled-up sub panel's edge before it opens: passing over
/// on the way to the panel doesn't open it.
const SUB_DWELL: Duration = Duration::from_millis(220);
use crate::sys::{self, Compositor};
use crate::notes::{self, NoteMsg, Notes, Reason};
use crate::ui::{self, Act, Extra, Kit, Model, OsdKind, Popup, PopupKind};

/// Messages from other threads (status, IPC, wallpaper loading, matugen).
pub enum Msg {
    /// A drag's files, read: for the drop (true), or to know their types.
    DragFiles(Vec<PathBuf>, bool),
    /// Downloads or the Trash changed (from the dock's own doing).
    PlacesChanged,
    Wake,
    Ipc(String),
    Wall { path: PathBuf, size: (u32, u32), pixels: Option<Vec<u8>> },
    Matugen(Result<(), String>),
    Note(NoteMsg),
    /// The visualizer has new bars.
    Viz,
    /// From the password prompt's threads.
    Polkit(crate::polkit::Event),
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
    /// Its blur behind, made the first time it's needed (one per surface). Before `layer`, so
    /// it goes before the surface does.
    effect: Option<Blur>,
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
    // ---- a sub panel's
    /// When the pointer came onto it.
    entered: Option<Instant>,
    /// On a panel with sub panels: since when the pointer has been on the side they roll out from.
    reveal_at: Option<Instant>,
    /// Asked for (by its item or a key): open until then, unless the pointer is on it.
    summon: Option<Instant>,
    /// Closed by its item or a key while the pointer was on it: stays shut until the pointer leaves.
    shut: bool,
    /// A panel's part that takes the pointer, as last set (None: all of it).
    region: Option<[i32; 4]>,
    /// The last frame sent (to send only what changed).
    last: Vec<u8>,
    // ---- a wallpaper surface's picture
    /// The picture showing, the one it's coming in over, and when that began.
    wall_now: Option<(PathBuf, Arc<Vec<u8>>)>,
    wall_from: Option<Arc<Vec<u8>>>,
    wall_at: Instant,
    /// The picture last sent whole or in part, and the rows its widgets were drawn on.
    wall_rows: Option<(Arc<Vec<u8>>, Rows)>,
    /// The region last asked to be blurred, as rectangles.
    blur_last: Vec<[i32; 4]>,
}

/// A surface's blur, asked of the compositor: let go of when the surface is.
struct Blur(ExtBackgroundEffectSurfaceV1);

impl Drop for Blur {
    fn drop(&mut self) {
        self.0.destroy();
    }
}

/// The first and last row of a run of rows (none: no rows).
type Rows = Option<(usize, usize)>;

impl Surf {
    fn physical(&self) -> (u32, u32) {
        (self.size.0 * self.scale as u32, self.size.1 * self.scale as u32)
    }
}

/// A screen's wallpaper, scaled to it (BGRA).
struct WallCache {
    path: PathBuf,
    size: (u32, u32),
    pixels: Arc<Vec<u8>>,
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
    /// The event loop, where a held key's repeats are timed.
    handle: LoopHandle<'static, Shell>,
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
    /// The sound's levels, listened to while something shows them.
    viz: crate::viz::VizLive,
    plugins: crate::plugins::Plugins,
    plugins_gen: u64,
    /// The lock screen was up at the last look (for the lock and unlock events).
    was_locked: bool,
    radio_gen: u64,
    ai: crate::ai::AiLive,
    ai_gen: u64,
    weather: crate::weather::WeatherLive,
    weather_gen: u64,
    tray: crate::tray::TrayLive,
    tray_gen: u64,
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
    /// The password prompt, while the shell is the session's polkit agent.
    polkit: Option<crate::polkit::Agent>,
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
    // ---- drag and drop (dnd.rs)
    ddm: Option<DataDeviceManagerState>,
    data_device: Option<DataDevice>,
    /// Files dragged over the dock.
    drag_in: Option<dnd::DragIn>,
    /// Files dropped on the dock, being read, and the slot they were dropped on.
    dropping: Option<(ui::DockSlot, DragOffer)>,
    /// A file being dragged out of the Downloads list.
    drag_source: Option<(DragSource, PathBuf)>,
    /// The last press of a pointer button, which a drag out starts from.
    press_serial: u32,
    /// Blur behind see-through surfaces (`ext-background-effect`), and whether the
    /// compositor says it can blur now.
    blur_mgr: Option<ExtBackgroundEffectManagerV1>,
    blur_cap: bool,
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
    // Drag and drop (the dock takes files; Downloads gives them): left out where it's missing.
    let ddm = DataDeviceManagerState::bind(&globals, &qh).ok();
    // Blur behind the shell's see-through parts, in their own shapes: left out where it's missing.
    let blur_mgr = globals.bind::<ExtBackgroundEffectManagerV1, _, _>(&qh, 1..=1, ()).ok();
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
    let weather = crate::weather::start(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let tray = crate::tray::TrayLive::new(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let viz_tx = std::sync::Mutex::new(tx.clone());
    let viz = crate::viz::VizLive::new(move || {
        if let Ok(t) = viz_tx.lock() {
            let _ = t.send(Msg::Viz);
        }
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let audio = crate::audio::start(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let wake_tx = std::sync::Mutex::new(tx.clone());
    let plugins = crate::plugins::Plugins::new(move || {
        if let Ok(t) = wake_tx.lock() {
            let _ = t.send(Msg::Wake);
        }
    });
    let key_tx = tx.clone();
    sys::watch_lock_keys(move |caps, on| {
        let _ = key_tx.send(Msg::LockKey { caps, on });
    });
    let mut event_loop: EventLoop<Shell> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle: LoopHandle<Shell> = event_loop.handle();
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
        handle: handle.clone(),
        kb_mods: egui::Modifiers::NONE,
        s_mtime: crate::config::mtime(),
        s: settings,
        live,
        last_gen: 0,
        radio,
        audio,
        audio_gen: 0,
        viz,
        plugins,
        plugins_gen: 0,
        was_locked: false,
        radio_gen: 0,
        ai,
        ai_gen: 0,
        weather,
        weather_gen: 0,
        tray,
        tray_gen: 0,
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
        polkit: None,
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
        ddm,
        data_device: None,
        drag_in: None,
        dropping: None,
        drag_source: None,
        press_serial: 0,
        blur_mgr,
        blur_cap: false,
    };
    shell.set_notifications();
    shell.set_polkit();
    shell.set_ai();
    shell.apply_comp();
    shell.plugins.sync(&shell.s.plugins.clone(), &shell.s.plugin_set.clone());
    shell.event("start", &[]);
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
                shape_panel(&layer, &s.panels, idx, &name);
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
                // Centred across the screen; with no top or bottom anchor, centred down it too.
                match s.osd_place {
                    crate::config::OsdPlace::Top => layer.set_anchor(Anchor::TOP),
                    crate::config::OsdPlace::Middle => layer.set_anchor(Anchor::empty()),
                    crate::config::OsdPlace::Bottom => layer.set_anchor(Anchor::BOTTOM),
                }
                let (w, h) = crate::cc::osd_size(s);
                layer.set_size(w as u32, h as u32);
                // Clear of the panel and the dock (a zone of 0 keeps out of others' room).
                layer.set_exclusive_zone(0);
                layer.set_margin(24, 0, 24, 0);
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
        let rolled = kind == Kind::Panel && self.s.panels.get(idx).is_some_and(|p| p.sub.is_some() && !p.kept());
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
            wall_now: None,
            wall_from: None,
            wall_at: Instant::now(),
            wall_rows: None,
            effect: None,
            blur_last: Vec::new(),
            ctx,
            kit,
            tex,
            pool,
            events: Vec::new(),
            modifiers: egui::Modifiers::NONE,
            start,
            // A sub panel starts rolled up, unless it was left open.
            slide: if rolled { 1.0 } else { 0.0 },
            slide_at: Instant::now(),
            collapsed: rolled,
            hovered: false,
            left_at: None,
            entered: None,
            reveal_at: None,
            summon: None,
            shut: false,
            region: None,
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

    /// Be the session's password prompt (or stop), as the setting says. Where another prompt
    /// already answers for the session, or there's no polkit, the shell goes without.
    fn set_polkit(&mut self) {
        if !self.s.ask_passwords {
            if self.polkit.take().is_some() {
                self.sync_auth();
            }
            return;
        }
        if self.polkit.is_some() {
            return;
        }
        let tx = self.tx.clone();
        match crate::polkit::Agent::start(move |e| {
            let _ = tx.send(Msg::Polkit(e));
        }) {
            Ok(a) => self.polkit = Some(a),
            Err(e) => eprintln!("rivet-shell: password prompt: {e}"),
        }
    }

    /// The password dialog follows what's being asked: opened for a request, closed when none
    /// is left.
    fn sync_auth(&mut self) {
        let asking = self.polkit.as_ref().is_some_and(|a| a.view().is_some());
        let open = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Auth { .. }));
        if asking && !open {
            self.open_popup(Popup::at(PopupKind::Auth { typed: String::new(), round: 0 }, f32::NAN, ""));
        } else if !asking && open {
            self.close_popup();
        }
        self.redraw_kinds(&[Kind::Popup]);
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
        self.osd_until = Some(Instant::now() + Duration::from_secs_f32(self.s.osd_secs));
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

    /// Something happened that plugins and the person's hooks may want to hear of.
    fn event(&self, name: &str, data: &[(&str, &str)]) {
        let ev = crate::plugins::Event::new(name, data);
        self.plugins.emit(&ev);
        if let Some((_, cmd)) = self.s.hooks.iter().find(|(e, _)| e == name) {
            crate::plugins::run_hook(cmd, &ev);
        }
    }

    /// A step of idle reached (`on`), or the person is back.
    fn idle(&mut self, st: Stage, on: bool) {
        let stage = match st {
            Stage::Dim => "dim",
            Stage::Lock => "lock",
            Stage::ScreenOff => "screen-off",
            Stage::Suspend => "suspend",
        };
        self.event(if on { "idle" } else { "active" }, &[("stage", stage)]);
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

    /// Whether a panel's Now Playing item shows the visualizer.
    fn panel_viz(&self, idx: usize) -> bool {
        self.s.media_viz && self.s.panels.get(idx).is_some_and(|p| p.has("media"))
    }

    /// The visualizer listens only while something plays and something shows its bars: a
    /// panel's Now Playing item set to, or the player's popup.
    fn set_viz(&mut self) {
        let playing = self.prev_status.media.as_ref().is_some_and(|m| m.playing);
        let shown = (0..self.s.panels.len()).any(|i| self.panel_viz(i)) || self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Media);
        self.viz.want(playing && shown && !self.locker.running());
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
            // Its place among the others too: a sub panel left open moves the ones around it.
            if a.is_none_or(|a| a.floating != b.floating) || panel_shape(old, sf.idx, &sf.output_name) != panel_shape(&new, sf.idx, &sf.output_name) {
                shape_panel(&sf.layer, &new, sf.idx, &sf.output_name);
                sf.layer.commit();
                sf.dirty = true;
            } else if a != Some(b) {
                sf.dirty = true;
            }
            // A sub panel no longer left open rolls up in its own time, not at once.
            if let Some(sub) = b.sub.as_ref().filter(|sub| !sub.stay)
                && a.is_some_and(|a| a.kept())
            {
                sf.summon = Some(Instant::now() + Duration::from_secs_f32(sub.secs));
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
        // The password dialog is answered before anything else opens.
        if !matches!(p.kind, PopupKind::Auth { .. }) && self.popup.as_ref().is_some_and(|o| matches!(o.kind, PopupKind::Auth { .. })) {
            return;
        }
        // The same popup again closes it.
        if self.popup.as_ref().is_some_and(|o| std::mem::discriminant(&o.kind) == std::mem::discriminant(&p.kind)) {
            self.close_popup();
            return;
        }
        self.close_popup();
        // The calendar: plugins that give events are told the days it shows, for theirs.
        if let PopupKind::Calendar { month, .. } = &p.kind {
            let (from, to) = ui::calendar_span(*month);
            self.plugins.emit(&crate::plugins::Event::new("month", &[("from", &from.format("%Y-%m-%d").to_string()), ("to", &to.format("%Y-%m-%d").to_string())]));
        }
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
        // An hour-old reading is read again as its popup opens.
        if p.kind == PopupKind::Weather && chrono::Utc::now().timestamp() - self.weather.get().at > 3600 {
            self.weather.refresh();
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
        // The browsers follow the same colours. Off the event loop: it looks through the
        // running programs for them.
        // So do the terminals and editors chosen on the Applications page.
        let s = self.s.clone();
        std::thread::spawn(move || {
            crate::browser::apply(&s);
            crate::themed::apply(&s);
        });
    }

    /// The AI item's readings run while a panel shows it.
    fn set_ai(&self) {
        self.ai.set(self.s.panels.iter().any(|p| p.has("ai")), &self.s.ai_providers);
        self.weather.set(self.s.shows_weather(), &self.s.weather_place, self.s.weather_at);
        // The tray starts once a panel has it (and then keeps apps' icons registered).
        if self.s.panels.iter().any(|p| p.has("tray")) {
            self.tray.start();
        }
    }

    fn close_popup(&mut self) {
        // Closing the password dialog (Esc, Cancel, another popup opened over it) is a no.
        let auth = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Auth { .. }));
        if self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::TrayMenu { .. })) {
            self.tray.run(crate::tray::Job::CloseMenu);
        }
        if self.popup.take().is_some_and(|p| p.join.is_some()) {
            // The panel it was joined to gets its outline back.
            self.redraw_kinds(&[Kind::Panel]);
        }
        self.radio.want(crate::radio::NONE);
        self.audio.want(false);
        if let Some(i) = self.surfs.iter().position(|s| s.kind == Kind::Popup) {
            let s = self.surfs.remove(i);
            self.spare = Some((s.ctx, s.kit, s.tex, s.start));
        }
        if auth && let Some(a) = self.polkit.as_mut() {
            a.cancel();
            // Another request may have been waiting behind it.
            self.sync_auth();
        }
    }

    fn index_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfs.iter().position(|s| s.layer.wl_surface() == surface)
    }

    // ---------------------------------------------------------------- the loop

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Wake => self.status_changed(),
            Msg::DragFiles(files, dropped) => self.drag_files(files, dropped),
            Msg::PlacesChanged => self.redraw_kinds(&[Kind::Dock, Kind::Popup]),
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
            Msg::Viz => {
                let pop = self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Media);
                let panels: Vec<bool> = (0..self.s.panels.len()).map(|i| self.panel_viz(i)).collect();
                for s in &mut self.surfs {
                    s.dirty |= match s.kind {
                        Kind::Panel => panels.get(s.idx).copied().unwrap_or(false),
                        Kind::Popup => pop,
                        _ => false,
                    };
                }
            }
            Msg::Polkit(e) => {
                if self.polkit.as_mut().is_some_and(|a| a.on_event(e)) {
                    self.sync_auth();
                }
            }
            Msg::Note(NoteMsg::New(n)) => {
                self.event("notification", &[("app", &n.app), ("summary", &n.summary), ("body", &n.body), ("urgency", ["low", "normal", "critical"][n.urgency.min(2) as usize])]);
                // Do Not Disturb keeps them off the screen (all but the urgent), not out of the history.
                let card = !self.dnd || n.urgency >= 2;
                let mut n = n;
                if !n.own_time && n.show_for.is_some() {
                    n.show_for = Some(Duration::from_secs(self.s.note_secs as u64));
                }
                self.notes.add(n, card, self.s.note_max);
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
                    ("control-center" | "control" | "quick-settings", _) => self.open_popup(Popup::at(PopupKind::Control { drag: None }, right, "")),
                    ("notifications" | "notification-center", _) => self.open_popup(Popup::at(PopupKind::Notifications, right, "")),
                    ("calendar", _) => self.open_popup(Popup::at(PopupKind::Calendar { month: 0, day: None }, f32::NAN, "")),
                    ("power", _) => self.open_popup(Popup::at(PopupKind::Power { confirm: None }, right, "")),
                    ("network" | "wifi", _) => self.open_popup(Popup::at(PopupKind::Network { ask: None }, right, "")),
                    ("bluetooth", _) => self.open_popup(Popup::at(PopupKind::Bluetooth, right, "")),
                    ("ai", _) => self.open_popup(Popup::at(PopupKind::Ai, right, "")),
                    ("weather", _) => self.open_popup(Popup::at(PopupKind::Weather, right, "")),
                    ("sound" | "audio", _) => self.open_popup(Popup::at(PopupKind::Sound { drag: None }, right, "")),
                    ("volume", "up" | "down") => self.act(Act::Volume(arg == "up")),
                    ("volume", "mute") => self.act(Act::Mute),
                    ("mic", "mute") => self.act(Act::MicMute),
                    ("volume", n) if n.parse::<u32>().is_ok() => self.act(Act::SetVolume(n.parse().unwrap_or(50))),
                    ("brightness", "up" | "down") => self.act(Act::Brightness(arg == "up")),
                    ("brightness", n) if n.parse::<u32>().is_ok() => self.act(Act::SetBrightness(n.parse().unwrap_or(50))),
                    ("media" | "player", "") => self.open_popup(Popup::at(PopupKind::Media, f32::NAN, "")),
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
                    ("subpanel" | "sub-panel", _) => {
                        // "NAME" or "NAME open" (close, toggle, stay); no name: the first one.
                        let (name, what) = match arg.rsplit_once(' ') {
                            Some((n, w)) if ui::SubDo::from(w).is_some() => (n.trim(), ui::SubDo::from(w).unwrap_or(ui::SubDo::Toggle)),
                            _ if crate::panels::find_sub(&self.s.panels, arg).is_none() && ui::SubDo::from(arg).is_some() => ("", ui::SubDo::from(arg).unwrap_or(ui::SubDo::Toggle)),
                            _ => (arg, ui::SubDo::Toggle),
                        };
                        let name = if name.is_empty() { self.s.panels.iter().find_map(|p| p.sub.as_ref().map(|x| x.name.clone())).unwrap_or_default() } else { name.to_string() };
                        if crate::panels::find_sub(&self.s.panels, &name).is_none() {
                            eprintln!("rivet-shell: no sub panel named {name:?}");
                        }
                        self.sub_panel(&name, what, "");
                    }
                    ("lock", _) => self.locker.lock(&self.s),
                    ("unlock", _) => self.locker.unlock(),
                    ("settings", _) => open_settings(None),
                    ("reload", _) => self.reload_settings(true),
                    // The Settings window saved: take it now rather than at the next tick.
                    ("changed", _) => self.reload_settings(false),
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
                    self.walls.push(WallCache { path, size, pixels: Arc::new(pixels) });
                }
                for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Background) {
                    s.dirty = true;
                }
            }
            Msg::Matugen(r) => {
                match r {
                    Err(e) => eprintln!("rivet-shell: matugen: {e}"),
                    Ok(()) => {
                        self.event("colors", &[]);
                        self.publish_greeter();
                    }
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
            kit.face = ui::Face::Launcher;
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
        let locked = self.locker.running();
        if locked != self.was_locked {
            self.was_locked = locked;
            self.event(if locked { "lock" } else { "unlock" }, &[]);
        }
        self.set_media_awake();
        self.set_viz();
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
            let clocked = matches!(s.kind, Kind::Panel | Kind::Cards) || (s.kind == Kind::Background && self.s.widget_clock) || (s.kind == Kind::Popup && self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Control { .. } | PopupKind::Notifications)));
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
        let pg = self.plugins.generation();
        if pg != self.plugins_gen {
            self.plugins_gen = pg;
            self.redraw_kinds(&[Kind::Panel, Kind::Popup]);
        }
        let tg = self.tray.generation();
        if tg != self.tray_gen {
            self.tray_gen = tg;
            let pop = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::TrayMenu { .. }));
            self.redraw_kinds(if pop { &[Kind::Panel, Kind::Popup] } else { &[Kind::Panel] });
        }
        let ag = self.ai.generation();
        if ag != self.ai_gen {
            self.ai_gen = ag;
            let pop = self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Ai);
            self.redraw_kinds(if pop { &[Kind::Panel, Kind::Popup] } else { &[Kind::Panel] });
        }
        let wg = self.weather.generation();
        if wg != self.weather_gen {
            self.weather_gen = wg;
            let pop = self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Weather);
            self.redraw_kinds(if pop { &[Kind::Background, Kind::Panel, Kind::Popup] } else { &[Kind::Background, Kind::Panel] });
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
        let front = |s: &sys::Status| s.workspaces.iter().find(|w| w.active).map(|w| (w.key.clone(), w.label.clone()));
        if let Some((key, name)) = front(&st).filter(|f| Some(f) != front(&prev).as_ref()) {
            self.event("workspace", &[("key", &key), ("name", &name)]);
        }
        if st.media != prev.media {
            let md = st.media.clone().unwrap_or_default();
            self.event("media", &[("title", &md.title), ("artist", &md.artist), ("player", &md.player), ("playing", if md.playing { "true" } else { "false" })]);
        }
        let panel_changed = st.workspaces != prev.workspaces || st.volume != prev.volume || st.net != prev.net || st.battery != prev.battery || st.window != prev.window || st.media != prev.media || st.brightness != prev.brightness || (bg_changed && self.s.panels.iter().any(|p| p.has("system")));
        let dock_changed = st.running != prev.running;
        // The dock item in a panel shows open apps too.
        let panel_changed = panel_changed || (dock_changed && self.s.panels.iter().any(|p| p.has("dock")));
        let control_open = self.popup.as_ref().is_some_and(|p| matches!(p.kind, PopupKind::Control { .. } | PopupKind::Network { .. } | PopupKind::Bluetooth));
        let control_changed = st.volume != prev.volume || st.mic != prev.mic || st.brightness != prev.brightness || st.media != prev.media || st.net != prev.net || st.wifi_on != prev.wifi_on || st.bluetooth != prev.bluetooth || st.vpns != prev.vpns || st.profile != prev.profile;
        for s in &mut self.surfs {
            let shows = match s.kind {
                Kind::Panel => panel_changed,
                Kind::Dock => dock_changed,
                Kind::Background => self.s.widget_system && bg_changed,
                Kind::Popup => (control_open && control_changed) || matches!(self.popup.as_ref().map(|p| &p.kind), Some(PopupKind::DockMenu { .. })) && dock_changed || (self.popup.as_ref().is_some_and(|p| p.kind == PopupKind::Media) && (st.media != prev.media || st.media_pos != prev.media_pos)),
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
        let look = old.color_use != s.color_use || old.theme != s.theme || old.accent != s.accent || old.follow_desktop_colors != s.follow_desktop_colors || old.text_size != s.text_size || old.icon_theme != s.icon_theme || old.palette != s.palette || old.surface_dark != s.surface_dark || old.surface_light != s.surface_light || old.surface_tint != s.surface_tint || old.secondary != s.secondary || old.tertiary != s.tertiary;
        let pictures = old.wallpaper != s.wallpaper || old.wallpaper_screens != s.wallpaper_screens;
        let colours = old.matugen_picture() != s.matugen_picture() || old.color_mode != s.color_mode || old.scheme_type != s.scheme_type || old.matugen_contrast != s.matugen_contrast || (s.makes_colors() && !old.makes_colors());
        let panels_changed = old.panels != s.panels;
        if old.plugins != s.plugins || old.plugin_set != s.plugin_set {
            self.plugins.sync(&s.plugins, &s.plugin_set);
            // A plugin's icon colour is among them.
            self.redraw_kinds(&[Kind::Panel]);
        }
        if old.wallpaper != s.wallpaper {
            self.event("wallpaper", &[("path", &s.wallpaper)]);
        }
        if old.color_mode != s.color_mode {
            self.event("style", &[("mode", if s.color_mode == rivet_look::theme::Mode::Dark { "dark" } else { "light" })]);
        }
        let panel_content = old.quick_icon != s.quick_icon || old.launcher_icon != s.launcher_icon || old.launcher_custom != s.launcher_custom || old.tray_color != s.tray_color || old.tray_passive != s.tray_passive || old.vol_show != s.vol_show || old.vol_scroll != s.vol_scroll || old.bright_show != s.bright_show || old.bright_scroll != s.bright_scroll || old.batt_show != s.batt_show || old.batt_hide_full != s.batt_hide_full || old.clock_format != s.clock_format || old.ai_bar != s.ai_bar || old.ai_pins != s.ai_pins || old.ai_left != s.ai_left || old.ws_style != s.ws_style || old.ws_names != s.ws_names || old.ws_hide_empty != s.ws_hide_empty || old.ws_scroll != s.ws_scroll || old.notifications != s.notifications;
        let dock_shape = old.dock_apps != s.dock_apps || old.dock_icon != s.dock_icon || old.dock_reserve != s.dock_reserve || old.dock_open_apps != s.dock_open_apps || old.dock_hide != s.dock_hide;
        if old.notifications != s.notifications {
            self.set_notifications();
        }
        if old.ask_passwords != s.ask_passwords {
            self.set_polkit();
        }
        if old.night_kelvin != s.night_kelvin && self.night.on() {
            self.night.set(true, s.night_kelvin);
        }
        if old.color_mode != s.color_mode {
            // The control center's Dark Style tile.
            self.redraw_kinds(&[Kind::Popup]);
        }
        let widgets = old.widget_clock != s.widget_clock || old.widget_system != s.widget_system || old.widget_side != s.widget_side || old.widget_clock_at != s.widget_clock_at || old.widget_system_at != s.widget_system_at;
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
        // The dock item in a panel shares the dock's apps and settings.
        let dock_item = s.panels.iter().any(|p| p.has("dock")) && (dock_shape || old.dock_running != s.dock_running);
        if panel_content || dock_item {
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
        if pictures || colours {
            self.wallpaper_changed(colours);
        }
        // The notification cards on screen move, resize or take their new look.
        if (old.note_place, old.note_width, old.note_look) != (s.note_place, s.note_width, s.note_look) {
            self.surfs.retain(|x| x.kind != Kind::Cards);
            self.sync_cards();
        } else if look || old.look != s.look {
            self.redraw_kinds(&[Kind::Cards]);
        }
        // The on-screen display moved, resized or recoloured: show it at the volume, to see.
        if (old.osd_place, old.osd_size, old.osd_secs, old.osd_color) != (s.osd_place, s.osd_size, s.osd_secs, s.osd_color) || (s.osd && !old.osd) {
            self.surfs.retain(|x| x.kind != Kind::Osd);
            let (value, muted) = self.prev_status.volume.unwrap_or((50, false));
            self.show_osd(OsdKind::Volume, value, muted);
        }
    }

    /// The wallpaper setting changed (or the shell started): load it, and give matugen the new
    /// picture so every app takes its colours.
    fn wallpaper_changed(&mut self, run_matugen: bool) {
        // Pictures no screen shows any more are let go (the ones on screen stay, to fade from).
        let settings = &self.s;
        let shown: Vec<PathBuf> = self.surfs.iter().filter(|s| s.kind == Kind::Background).map(|s| PathBuf::from(settings.wallpaper_on(&s.output_name))).collect();
        self.walls.retain(|w| shown.contains(&w.path));
        for s in self.surfs.iter_mut().filter(|s| s.kind == Kind::Background) {
            s.dirty = true;
        }
        let path = PathBuf::from(self.s.matugen_picture());
        if path.as_os_str().is_empty() || !path.is_file() {
            return;
        }
        self.publish_greeter();
        if run_matugen && self.s.makes_colors() {
            let tx = self.tx.clone();
            crate::wall::run_matugen(&path, self.s.color_mode, &self.s.scheme_type, self.s.matugen_contrast, move |r| {
                let _ = tx.send(Msg::Matugen(r));
            });
        }
    }

    /// Leave the wallpaper and the colours shown now for the greeter (when it's set up).
    fn publish_greeter(&self) {
        if !crate::greeter::dir().is_dir() {
            return;
        }
        let Some(sf) = self.surfs.first() else { return };
        let r = crate::style::Roles::new(&sf.kit.look, &self.s);
        let rgb = |c: egui::Color32| [c.r(), c.g(), c.b()];
        crate::greeter::publish(&self.s, [rgb(r.primary), rgb(r.secondary), rgb(r.tertiary)]);
    }

    /// Open, close or keep a sub panel, as its item or a key asked (`output` empty: on every
    /// screen it's on). One that stays open is opened and closed for good, and its room kept or
    /// given back; one that rolls up is opened for its time, or shut now.
    fn sub_panel(&mut self, name: &str, what: ui::SubDo, output: &str) {
        use ui::SubDo;
        let Some(idx) = crate::panels::find_sub(&self.s.panels, name) else { return };
        let Some(sub) = self.s.panels[idx].sub.clone() else { return };
        let set = |s: &mut Settings, stay: bool, open: bool| {
            if let Some(x) = s.panels.get_mut(idx).and_then(|p| p.sub.as_mut()) {
                (x.stay, x.open) = (stay, open);
            }
        };
        match (what, sub.stay) {
            // Left open, or back to rolling up by itself (after its usual time).
            (SubDo::Stay, stay) => return self.change_settings(|s| set(s, !stay, !stay)),
            (SubDo::Toggle, true) => return self.change_settings(|s| set(s, true, !sub.open)),
            (SubDo::Open, true) if !sub.open => return self.change_settings(|s| set(s, true, true)),
            (SubDo::Close, true) if sub.open => return self.change_settings(|s| set(s, true, false)),
            (_, true) => return,
            _ => {}
        }
        let now = Instant::now();
        let until = now + Duration::from_secs_f32(sub.secs);
        let panels = &self.s.panels;
        let here = |sf: &Surf| sf.kind == Kind::Panel && (output.is_empty() || sf.output_name.is_empty() || sf.output_name == output);
        // Open already (on any of the screens asked about): a toggle shuts it.
        let showing = self.surfs.iter().any(|sf| here(sf) && sf.idx == idx && !sf.collapsed && (sf.summon.is_some() || sf.hovered || sf.left_at.is_some()));
        let open = match what {
            SubDo::Open => true,
            SubDo::Close => false,
            _ => !showing,
        };
        for sf in self.surfs.iter_mut().filter(|sf| here(sf)) {
            let other = panels.get(sf.idx).and_then(|p| p.sub.as_ref()).filter(|x| sf.idx != idx && x.host == sub.host && !x.stay);
            if sf.idx == idx {
                sf.summon = open.then_some(until);
                sf.left_at = None;
                sf.shut = !open && sf.hovered;
                sf.dirty = true;
            } else if open && other.is_some() {
                // One at a time under a host: the others that roll up make way.
                sf.summon = None;
                sf.left_at = None;
                sf.shut = sf.hovered;
                sf.dirty = true;
            }
        }
    }

    /// The docks and the panels show or slide away as their hide settings say: shown while the
    /// pointer is on one (and a moment after it leaves) or a popup of its is open; with Smart,
    /// also while the screen's workspace has no windows. Out of sight, a dock shrinks to a thin
    /// strip along the edge, which the pointer reaching brings it back; a panel keeps its
    /// surface and takes the pointer only along such a strip.
    fn update_docks(&mut self) {
        let now = Instant::now();
        let menu = matches!(self.popup.as_ref().map(|p| &p.kind), Some(PopupKind::DockMenu { .. } | PopupKind::Downloads | PopupKind::TrashMenu { .. }));
        // A panel stays while a popup is open on its screen.
        let popup_on = self.popup.as_ref().filter(|_| !menu).map(|p| p.output.clone());
        let (s, apps, running) = (&self.s, &self.apps, &self.prev_status.running);
        let workspaces = &self.prev_status.workspaces;
        // The panel a popup was opened from: a sub panel stays for its own popups only.
        let popup_from = self.popup.as_ref().filter(|_| !menu).and_then(|p| p.panel);
        // Hosts with a sub panel in view (by screen): a host that hides stays out with it.
        let hosting: Vec<(usize, String)> = self.surfs.iter().filter(|x| x.kind == Kind::Panel && !x.collapsed && x.slide < 1.0).filter_map(|x| Some((s.panels.get(x.idx)?.sub.as_ref()?.host?, x.output_name.clone()))).collect();
        // Hosts the pointer has paused on, on the side their sub panels roll out from.
        let revealing: Vec<(usize, String)> = self.surfs.iter().filter(|x| x.kind == Kind::Panel && x.reveal_at.is_some_and(|t| now.duration_since(t) >= SUB_DWELL)).map(|x| (x.idx, x.output_name.clone())).collect();
        for sf in self.surfs.iter_mut().filter(|x| matches!(x.kind, Kind::Dock | Kind::Panel)) {
            let dock = sf.kind == Kind::Dock;
            let cfg = if dock { None } else { s.panels.get(sf.idx) };
            let sub = cfg.and_then(|c| c.sub.as_ref());
            let mode = if dock { s.dock_hide } else { cfg.map(|p| p.hide).unwrap_or_default() };
            // A sub panel stays a good while after the pointer leaves; the rest, a moment.
            let linger = sub.map_or(DOCK_LINGER, |x| Duration::from_secs_f32(x.secs));
            if sf.left_at.is_some_and(|t| now.duration_since(t) >= linger) {
                sf.left_at = None;
            }
            if sf.summon.is_some_and(|t| now >= t) {
                sf.summon = None;
            }
            let open = if dock { menu } else { popup_on.as_ref().is_some_and(|o| o.is_empty() || sf.output_name.is_empty() || *o == sf.output_name) };
            let near = sf.hovered || sf.left_at.is_some() || open || hosting.iter().any(|(h, o)| *h == sf.idx && !dock && *o == sf.output_name);
            // Windows on this screen's workspace (a compositor that doesn't say: as if there were).
            let here: Vec<_> = workspaces.iter().filter(|w| w.active && (w.output.is_empty() || sf.output_name.is_empty() || w.output == sf.output_name)).collect();
            let busy = here.is_empty() || here.iter().any(|w| w.busy);
            let show = match (sub, mode) {
                (Some(x), _) if x.stay => x.open,
                (Some(x), _) => {
                    // Asked for from its host's side: as if by its button, for as long as the
                    // pointer stays there and its usual time after.
                    if x.reveal && !sf.shut && revealing.iter().any(|(h, o)| Some(*h) == x.host && *o == sf.output_name) {
                        sf.summon = Some(now + linger);
                    }
                    // Rolled up, the pointer has to rest on its edge a moment to open it.
                    let resting = sf.hovered && (!sf.collapsed || sf.entered.is_some_and(|t| now.duration_since(t) >= SUB_DWELL));
                    !sf.shut && (resting || sf.left_at.is_some() || sf.summon.is_some() || (open && popup_from == Some(sf.idx)))
                }
                (None, DockHide::Never) => true,
                (None, DockHide::Auto) => near,
                (None, DockHide::Smart) => near || !busy,
            };
            if show && sf.collapsed {
                // Full size again (still out of sight), then slide in.
                sf.collapsed = false;
                sf.slide = 1.0;
                sf.slide_at = now;
                if dock {
                    shape_dock(&sf.layer, s, apps, running, false);
                    sf.layer.commit();
                }
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
                if dock {
                    shape_dock(&sf.layer, s, apps, running, true);
                    sf.layer.commit();
                }
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
        // Drawing a surface carries out what was clicked on it, which can close other surfaces
        // (a popup, the notification cards): the list is then shorter and its places have moved,
        // so the walk starts over. One just drawn waits for its frame and isn't drawn twice.
        let mut i = 0;
        while i < self.surfs.len() {
            let s = &self.surfs[i];
            if s.dirty && s.configured && !s.waiting_frame {
                let n = self.surfs.len();
                self.draw(i);
                if self.surfs.len() != n {
                    i = 0;
                    continue;
                }
            }
            i += 1;
        }
    }

    /// Which cached wallpaper fits this size (loading it in the background when none does).
    fn wall_for(&mut self, output: &str, size: (u32, u32)) -> Option<usize> {
        let path = PathBuf::from(self.s.wallpaper_on(output));
        if path.as_os_str().is_empty() {
            return None;
        }
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
        let wall_idx = if kind == Kind::Background {
            let name = self.surfs[i].output_name.clone();
            self.wall_for(&name, (pw, ph))
        } else {
            None
        };
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
            plugins: if matches!(kind, Kind::Panel | Kind::Popup) { self.plugins.get() } else { Default::default() },
            ai: if matches!(kind, Kind::Panel | Kind::Popup) { self.ai.get() } else { Default::default() },
            weather: if matches!(kind, Kind::Background | Kind::Panel | Kind::Popup) { self.weather.get() } else { Default::default() },
            tray: if matches!(kind, Kind::Panel | Kind::Popup) { self.tray.get() } else { Default::default() },
            join: if kind == Kind::Panel { self.popup.as_ref().filter(|p| p.output.is_empty() || p.output == self.surfs[i].output_name).and_then(|p| Some((p.attach?, p.join?.0, p.join?.1))) } else { None },
            auth: if kind == Kind::Popup { self.polkit.as_ref().and_then(|a| a.view()) } else { None },
            viz: self.viz.able().then(|| if matches!(kind, Kind::Panel | Kind::Popup) { self.viz.get() } else { Vec::new() }),
            drop: if kind == Kind::Dock { self.drag_in.as_ref().map(|d| ui::DockDrop { pos: d.pos, mimes: d.mimes.clone() }) } else { None },
        };
        let join_was = self.popup.as_ref().and_then(|p| p.join);
        let Shell { surfs, s: settings, apps, popup, comp, walls, compositor, blur_mgr, blur_cap, qh, .. } = self;
        let surf = &mut surfs[i];
        surf.dirty = false;
        if kind == Kind::Panel {
            // A panel shorter than its edge, or hidden, takes the pointer only where it is (or
            // along its strip); the rest of its surface lets clicks through.
            let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(w as f32, h as f32));
            let want = crate::panels::pointer_rect(&settings.panels, idx, full, surf.collapsed, &surf.output_name).map(|r| [r.left() as i32, r.top() as i32, r.width().ceil() as i32, r.height().ceil() as i32]);
            if want != surf.region {
                surf.region = want;
                match want.and_then(|q| smithay_client_toolkit::compositor::Region::new(&*compositor).ok().map(|r| (q, r))) {
                    Some((q, region)) => {
                        region.add(q[0], q[1], q[2], q[3]);
                        surf.layer.wl_surface().set_input_region(Some(region.wl_region()));
                    }
                    None => surf.layer.wl_surface().set_input_region(None),
                }
                surf.layer.commit();
            }
        }
        if kind == Kind::Background {
            // A new picture, once it's read, comes in over the one showing (which stays up
            // while the new one loads).
            if settings.wallpaper_on(&surf.output_name).is_empty() {
                (surf.wall_now, surf.wall_from) = (None, None);
            } else if let Some(wc) = wall_idx.and_then(|i| walls.get(i))
                && !surf.wall_now.as_ref().is_some_and(|(_, px)| Arc::ptr_eq(px, &wc.pixels))
            {
                let old = surf.wall_now.take();
                let moves = settings.wall_transition != crate::config::Transition::None;
                surf.wall_from = old.filter(|(p, px)| moves && *p != wc.path && px.len() == wc.pixels.len()).map(|(_, px)| px);
                surf.wall_at = Instant::now();
                surf.wall_now = Some((wc.path.clone(), wc.pixels.clone()));
            }
        }
        let wall: Option<Arc<Vec<u8>>> = surf.wall_now.as_ref().map(|(_, px)| px.clone());
        let mut wall_moving = false;
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
            surf.kit.face = match kind {
                Kind::Panel => ui::Face::Panel(idx),
                Kind::Popup => popup.as_ref().map(|p| p.face()).unwrap_or_default(),
                _ => ui::Face::Base,
            };
            surf.kit.begin(ui.ctx(), settings);
            plain = ui.visuals().extreme_bg_color;
            let mut m = Model { s: settings, status: &status, extra: &extra, apps, compositor: *comp, output: &output_name, acts: &mut acts };
            match kind {
                Kind::Background => ui::widgets(ui, &mut m, &mut surf.kit),
                Kind::Panel => {
                    // Slid part of the way off its edge (a hidden one shows nothing, but a
                    // sub panel's handle: where to pause the pointer to open it).
                    if collapsed {
                        ui::sub_handle(ui, settings, &surf.kit, idx, &output_name);
                    }
                    if !collapsed {
                        let by = settings.panels.get(idx).map(|c| c.slide(slide)).unwrap_or_default();
                        let mut c = ui.new_child(egui::UiBuilder::new().max_rect(ui.max_rect().translate(by)));
                        // A drawer rolls up behind its host: nothing of it shows over the
                        // host's bar on the way (its surface reaches under the host's corners).
                        if let Some(cfg) = settings.panels.get(idx).filter(|p| slide > 0.0 && p.drawer()) {
                            let mut clip = ui.max_rect();
                            match cfg.edge {
                                Edge::Top => clip.min.y += cfg.lead,
                                Edge::Bottom => clip.max.y -= cfg.lead,
                                Edge::Left => clip.min.x += cfg.lead,
                                Edge::Right => clip.max.x -= cfg.lead,
                            }
                            c.set_clip_rect(clip);
                        }
                        crate::bar::panel(&mut c, &mut m, &mut surf.kit, idx);
                    }
                }
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
        // What's see-through was drawn in these shapes: the compositor blurs behind them, and
        // nothing else of the surface (a panel's surface spans its edge, its bar may not).
        let glass = ui::take_glass(&surf.ctx);
        let want: Vec<[i32; 4]> = if settings.blur && *blur_cap && kind != Kind::Background { glass.iter().flat_map(|(r, c)| ui::glass_rects(*r, *c)).collect() } else { Vec::new() };
        if want != surf.blur_last
            && let Some(mgr) = blur_mgr.as_ref()
        {
            let effect = &surf.effect.get_or_insert_with(|| Blur(mgr.get_background_effect(surf.layer.wl_surface(), qh, ()))).0;
            match smithay_client_toolkit::compositor::Region::new(&*compositor).ok().filter(|_| !want.is_empty()) {
                Some(region) => {
                    for q in &want {
                        region.add(q[0], q[1], q[2], q[3]);
                    }
                    effect.set_blur_region(Some(region.wl_region()));
                }
                None => effect.set_blur_region(None),
            }
            surf.blur_last = want;
            // Applied on the surface's next commit, which an unchanged frame might not make.
            surf.layer.commit();
        }
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
        match (kind, wall.as_ref()) {
            (Kind::Background, Some(px)) if px.len() == canvas.len() => {
                let t = surf.wall_at.elapsed().as_secs_f32() * 1000.0 / settings.wall_transition_ms.max(1) as f32;
                match surf.wall_from.as_ref().filter(|f| t < 1.0 && f.len() == px.len()) {
                    Some(from) => {
                        crate::wall::transition(settings.wall_transition, from, px, canvas, pw as usize, t);
                        wall_moving = true;
                    }
                    None => {
                        surf.wall_from = None;
                        canvas.copy_from_slice(px);
                    }
                }
            }
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
        let span = |differs: &dyn Fn(usize) -> bool| {
            let first = (0..ph as usize).find(|&y| differs(y));
            first.map(|f| (f, (f..ph as usize).rev().find(|&y| differs(y)).unwrap_or(f)))
        };
        let rows = if kind != Kind::Background && surf.last.len() == canvas.len() {
            span(&|y| canvas[y * row_bytes..(y + 1) * row_bytes] != surf.last[y * row_bytes..(y + 1) * row_bytes])
        } else if let Some(px) = wall.as_ref().filter(|px| kind == Kind::Background && !wall_moving && px.len() == canvas.len()) {
            // The wallpaper with widgets drawn on it: the rows the widgets are on are the ones
            // that differ from the picture, so only those (and the ones they were on last
            // time) are sent, not the whole screen each time a widget's number changes.
            let on = span(&|y| canvas[y * row_bytes..(y + 1) * row_bytes] != px[y * row_bytes..(y + 1) * row_bytes]);
            let same_picture = surf.wall_rows.as_ref().is_some_and(|(p, _)| Arc::ptr_eq(p, px));
            let before = surf.wall_rows.replace((px.clone(), on));
            match before {
                Some((_, was)) if same_picture => match (was, on) {
                    (Some(a), Some(b)) => Some((a.0.min(b.0), a.1.max(b.1))),
                    (a, b) => a.or(b),
                },
                _ => Some((0, ph as usize - 1)),
            }
        } else {
            surf.wall_rows = None;
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
        if wall_moving {
            surf.repaint_at = Some(Instant::now() + Duration::from_millis(16));
        }
        if kind == Kind::Dim && dim < 1.0 {
            surf.repaint_at = Some(Instant::now() + Duration::from_millis(33));
        }
        // A dock or a panel on its way in or out keeps moving.
        if matches!(kind, Kind::Dock | Kind::Panel) && surf.slide > 0.0 && !surf.collapsed {
            surf.repaint_at = Some(Instant::now() + Duration::from_millis(16));
        }
        // A popup that joined a panel (or moved along it): the panel opens its outline there.
        if kind == Kind::Popup && self.popup.as_ref().and_then(|p| p.join) != join_was {
            self.redraw_kinds(&[Kind::Panel]);
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
            Act::SubPanel(name, what, output) => self.sub_panel(&name, what, &output),
            Act::CloseWindows(ids) => sys::close_windows(self.comp, ids),
            Act::Auth(mut text) => {
                if let Some(a) = self.polkit.as_mut() {
                    a.answer(&text);
                }
                // Overwritten before it's let go, as far as a String allows.
                unsafe { text.as_bytes_mut() }.iter_mut().for_each(|b| *b = 0);
                self.redraw_kinds(&[Kind::Popup]);
            }
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
            Act::MediaSeek(secs) => {
                sys::media_seek(secs);
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
            Act::Vpn(key, on) => {
                // Its program can take seconds: on a thread, and the status is read again after.
                let live = self.live.clone();
                std::thread::spawn(move || {
                    if let Err(e) = crate::vpn::set(&key, on) {
                        eprintln!("rivet-shell: VPN: {e}");
                    }
                    live.refresh(true);
                });
            }
            Act::AiRefresh => self.ai.refresh(true),
            Act::WeatherRefresh => self.weather.refresh(),
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
                self.event("dnd", &[("on", if on { "true" } else { "false" })]);
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
                    if !(s.makes_colors() && !s.matugen_picture().is_empty()) || s.theme != rivet_look::theme::Mode::System {
                        s.theme = mode;
                    }
                });
            }
            Act::MoveWidget(which, at) => self.change_settings(|s| match which {
                ui::Widget::Clock => s.widget_clock_at = at,
                ui::Widget::System => s.widget_system_at = at,
                ui::Widget::Weather => s.widget_weather_at = at,
            }),
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
            Act::OpenTrash => {
                // In the desktop's file manager, at the address they all take for it.
                let trash = [PathBuf::from("trash:///")];
                let opened = rivet_xdg::launch::default_app(&self.apps, "inode/directory").map(|a| rivet_xdg::launch::launch_app(a, &trash));
                if !matches!(opened, Some(Ok(()))) {
                    let _ = rivet_xdg::launch::open_url("trash:///");
                }
            }
            Act::EmptyTrash => {
                let tx = self.tx.clone();
                std::thread::spawn(move || {
                    let _ = rivet_xdg::trash::empty();
                    let _ = tx.send(Msg::PlacesChanged);
                });
            }
            Act::OpenFile(path, mime) => {
                if let Err(e) = rivet_xdg::launch::open_default(&self.apps, &mime, &path) {
                    eprintln!("rivet-shell: {e}");
                }
            }
            Act::DragOut(path) => self.drag_out(path),
            Act::Favorite(id) => {
                let apps = self.apps.clone();
                self.change_settings(|s| {
                    let same = |d: &String| d == &id || ui::find_app(&apps, d).is_some_and(|a| a.id.trim_end_matches(".desktop") == id);
                    if s.launch_favorite_apps.iter().any(same) {
                        s.launch_favorite_apps.retain(|d| !same(d));
                    } else {
                        s.launch_favorite_apps.push(id);
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
            Act::Settings => open_settings(None),
            Act::SettingsPage(page) => open_settings(Some(page)),
            Act::Tray(job) => self.tray.run(job),
            Act::Plugin(id, ev) => self.plugins.send(&id, ev),
            Act::CalendarSpan(from, to) => self.plugins.emit(&crate::plugins::Event::new("month", &[("from", &from.format("%Y-%m-%d").to_string()), ("to", &to.format("%Y-%m-%d").to_string())])),
        }
        // What the shell keeps (notifications, toggles) shows in the panel and the popup; the
        // rest follows the status when it arrives.
        self.redraw_kinds(&[Kind::Panel, Kind::Popup]);
    }
}

/// The Settings window (its own program, so the shell never waits on it).
/// Open the Settings window, on `page` when one is named.
pub fn open_settings(page: Option<&str>) {
    let mut cmd = match std::env::current_exe() {
        Ok(exe) => std::process::Command::new(exe),
        Err(_) => return,
    };
    if let Some(p) = page {
        cmd.env("RIVET_SHELL_PAGE", p);
    }
    if let Ok(child) = cmd.arg("settings").stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn()
    {
        sys::reap(child);
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
            let auth = self.popup.take().is_some_and(|p| matches!(p.kind, PopupKind::Auth { .. }));
            self.radio.want(crate::radio::NONE);
            self.audio.want(false);
            // The compositor took the password dialog away: a no.
            if auth && let Some(a) = self.polkit.as_mut() {
                a.cancel();
            }
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
        if self.data_device.is_none()
            && let Some(ddm) = &self.ddm
        {
            self.data_device = Some(ddm.get_data_device(qh, &seat));
        }
        if cap == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self.seat_state.get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            // With repeats, so a held arrow keeps moving through a list and held Backspace keeps
            // deleting, at the rate the compositor sets.
            self.keyboard = self.seat_state.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), Box::new(|shell: &mut Shell, _, e| shell.key(&e, true, true))).ok();
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
            if let PointerEventKind::Press { serial, .. } = e.kind {
                self.press_serial = serial;
            }
            let pos = Pos2::new(e.position.0 as f32, e.position.1 as f32);
            let mods = self.kb_mods;
            let s = &mut self.surfs[i];
            match &e.kind {
                PointerEventKind::Enter { .. } => {
                    s.hovered = true;
                    s.left_at = None;
                    s.entered = Some(Instant::now());
                    if s.collapsed {
                        // Look again once the pointer has rested (a sub panel's edge).
                        s.repaint_at = Some(Instant::now() + SUB_DWELL);
                    }
                    if let Some(p) = &self.pointer {
                        let _ = p.set_cursor(&self.conn, CursorIcon::Default);
                    }
                    s.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => {
                    s.hovered = false;
                    s.reveal_at = None;
                    // Passing over a rolled-up sub panel's edge isn't a visit to linger after.
                    let passing = s.collapsed && s.kind == Kind::Panel && self.s.panels.get(s.idx).is_some_and(|p| p.sub.is_some());
                    s.left_at = (!passing && !s.shut).then(Instant::now);
                    s.shut = false;
                    s.events.push(egui::Event::PointerGone);
                }
                PointerEventKind::Motion { .. } => {
                    // Pausing on the side a panel's sub panels roll out from opens them.
                    if s.kind == Kind::Panel {
                        let full = Rect::from_min_size(Pos2::ZERO, Vec2::new(s.size.0 as f32, s.size.1 as f32));
                        if !crate::panels::on_reveal_side(&self.s.panels, s.idx, full, pos) {
                            s.reveal_at = None;
                        } else if s.reveal_at.is_none() {
                            s.reveal_at = Some(Instant::now());
                            s.repaint_at = Some(Instant::now() + SUB_DWELL);
                        }
                    }
                    s.events.push(egui::Event::PointerMoved(pos));
                }
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
        // Letters and digits, for shortcuts with Ctrl (Ctrl+A, the launcher's Ctrl+D); what
        // they type comes as text.
        _ => {
            let c = char::from_u32(k.raw()).filter(char::is_ascii_alphanumeric)?;
            return egui::Key::from_name(&c.to_ascii_uppercase().to_string());
        }
    })
}

impl Shell {
    fn key(&mut self, e: &KeyEvent, pressed: bool, repeat: bool) {
        // A held Return or Escape acts once: a repeat could answer whatever opens next.
        if repeat && matches!(e.keysym, Keysym::Return | Keysym::KP_Enter | Keysym::Escape) {
            return;
        }
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
delegate_data_device!(Shell);

/// The panel's place and size: spanning its edge (a shorter one draws on part of that, and
/// takes the pointer only there), keeping windows clear of it unless it hides.
fn shape_panel(layer: &LayerSurface, panels: &[crate::panels::PanelCfg], idx: usize, output: &str) {
    let Some(p) = panels.get(idx) else { return };
    let (edge, h, zone, margin, ends) = panel_shape(panels, idx, output);
    match edge {
        Edge::Top | Edge::Bottom => {
            layer.set_anchor(if edge == Edge::Top { Anchor::TOP } else { Anchor::BOTTOM } | Anchor::LEFT | Anchor::RIGHT);
            layer.set_size(0, h);
        }
        Edge::Left | Edge::Right => {
            layer.set_anchor(if edge == Edge::Left { Anchor::LEFT } else { Anchor::RIGHT } | Anchor::TOP | Anchor::BOTTOM);
            layer.set_size(h, 0);
        }
    }
    layer.set_exclusive_zone(zone);
    if p.sub.is_some() {
        // Down a side, it runs between the top and bottom panels, as its host does.
        let m = |e: Edge| if e == edge { margin } else if edge.vertical() && e == Edge::Top { ends.0 } else if edge.vertical() && e == Edge::Bottom { ends.1 } else { 0 };
        layer.set_margin(m(Edge::Top), m(Edge::Right), m(Edge::Bottom), m(Edge::Left));
    }
}

/// A panel's surface: its edge, its thickness, the room it keeps from windows, and how far in
/// from the edge it sits. A sub panel keeps no room of its own (its host keeps it, for one left
/// open) and is placed where its host ends, whatever else is along that edge.
fn panel_shape(panels: &[crate::panels::PanelCfg], idx: usize, output: &str) -> (Edge, u32, i32, i32, (i32, i32)) {
    let Some(p) = panels.get(idx) else { return (Edge::Top, 0, 0, 0, (0, 0)) };
    let room = |e: Edge| crate::panels::room(panels, output, e) as i32;
    match p.sub {
        Some(_) => (p.edge, p.height() as u32, -1, crate::panels::sub_start(panels, idx, output) as i32, (room(Edge::Top), room(Edge::Bottom))),
        None => (p.edge, p.height() as u32, crate::panels::reserved(panels, idx) as i32, 0, (0, 0)),
    }
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
    use crate::config::NotePlace;
    let anchor = match s.note_place {
        // On the right, on the side of the first panel (the top when there are none).
        NotePlace::ByPanel => (if s.panels.first().is_some_and(|p| p.edge == Edge::Bottom) { Anchor::BOTTOM } else { Anchor::TOP }) | Anchor::RIGHT,
        NotePlace::TopLeft => Anchor::TOP | Anchor::LEFT,
        NotePlace::TopRight => Anchor::TOP | Anchor::RIGHT,
        NotePlace::BottomLeft => Anchor::BOTTOM | Anchor::LEFT,
        NotePlace::BottomRight => Anchor::BOTTOM | Anchor::RIGHT,
    };
    layer.set_anchor(anchor);
    layer.set_size(crate::cc::cards_width(s) as u32, crate::cc::cards_height(cards).ceil() as u32);
    layer.set_exclusive_zone(0);
    layer.set_margin(2, 4, 2, 4);
}

// The idle-inhibit protocol has no events the shell needs.
// Blur: the compositor says whether it can blur (and again if that changes); the shapes are
// asked for again at the next drawing.
impl Dispatch<ExtBackgroundEffectManagerV1, ()> for Shell {
    fn event(shell: &mut Self, _: &ExtBackgroundEffectManagerV1, ev: ext_background_effect_manager_v1::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {
        if let ext_background_effect_manager_v1::Event::Capabilities { flags } = ev {
            shell.blur_cap = matches!(flags, wayland_client::WEnum::Value(f) if f.contains(ext_background_effect_manager_v1::Capability::Blur));
            for s in &mut shell.surfs {
                s.dirty = true;
            }
        }
    }
}

impl Dispatch<ExtBackgroundEffectSurfaceV1, ()> for Shell {
    fn event(_: &mut Self, _: &ExtBackgroundEffectSurfaceV1, _: <ExtBackgroundEffectSurfaceV1 as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

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
