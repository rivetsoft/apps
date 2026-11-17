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
use wayland_client::{Connection, QueueHandle};

use crate::config::{Edge, Settings};
use crate::sys::{self, Compositor};
use crate::ui::{self, Act, Kit, Model, Popup};

/// Messages from other threads (status, IPC, wallpaper loading, matugen).
pub enum Msg {
    Wake,
    Ipc(String),
    Wall { path: PathBuf, size: (u32, u32), pixels: Option<Vec<u8>> },
    Matugen(Result<(), String>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Background,
    Panel,
    Dock,
    Popup,
}

struct Surf {
    kind: Kind,
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
    apps: Vec<rivet_xdg::launch::DesktopApp>,
    popup: Option<Popup>,
    comp: Compositor,
    walls: Vec<WallCache>,
    loading: Vec<(PathBuf, (u32, u32))>,
    tx: Sender<Msg>,
    palette_watch: Vec<(PathBuf, Option<SystemTime>)>,
    recolor_until: Option<Instant>,
    last_minute: u32,
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
    let wake_tx = tx.clone();
    let live = sys::start(comp, move || {
        let _ = wake_tx.send(Msg::Wake);
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
        apps: load_apps(),
        popup: None,
        comp,
        walls: Vec::new(),
        loading: Vec::new(),
        tx,
        palette_watch: palette_files(),
        recolor_until: None,
        last_minute: 99,
        exit: false,
    };
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

    fn new_surf(&mut self, kind: Kind, output: Option<&wl_output::WlOutput>) -> Surf {
        let surface = self.compositor.create_surface(&self.qh);
        let layer = match kind {
            Kind::Background => Layer::Background,
            Kind::Panel | Kind::Dock => Layer::Top,
            Kind::Popup => Layer::Overlay,
        };
        let ns = match kind {
            Kind::Background => "rivet-shell-wallpaper",
            Kind::Panel => "rivet-shell-panel",
            Kind::Dock => "rivet-shell-dock",
            Kind::Popup => "rivet-shell-popup",
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
                let edge = if s.panel_edge == Edge::Top { Anchor::TOP } else { Anchor::BOTTOM };
                layer.set_anchor(edge | Anchor::LEFT | Anchor::RIGHT);
                let h = ui::panel_height(s) as u32;
                layer.set_size(0, h);
                layer.set_exclusive_zone(h as i32);
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
            }
            Kind::Dock => {
                let n = s.dock_apps.iter().filter(|id| ui::find_app(&self.apps, id).is_some()).count();
                let (size, room) = ui::dock_size(s, n);
                layer.set_anchor(Anchor::BOTTOM);
                layer.set_size(size.x as u32, size.y as u32);
                layer.set_exclusive_zone(if s.dock_reserve { room as i32 } else { 0 });
                layer.set_keyboard_interactivity(KeyboardInteractivity::None);
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
        Surf {
            kind,
            layer,
            output: output.cloned(),
            output_name: name,
            size: (0, 0),
            scale,
            configured: false,
            dirty: true,
            waiting_frame: false,
            repaint_at: None,
            ctx: egui::Context::default(),
            kit: Kit::new(true),
            tex: soft::Textures::default(),
            pool,
            events: Vec::new(),
            modifiers: egui::Modifiers::NONE,
            start: Instant::now(),
        }
    }

    /// The panel and dock for a screen, as the settings ask.
    fn bars_for(&mut self, output: &wl_output::WlOutput) {
        if self.s.panel {
            let p = self.new_surf(Kind::Panel, Some(output));
            self.surfs.push(p);
        }
        if self.s.dock {
            let d = self.new_surf(Kind::Dock, Some(output));
            self.surfs.push(d);
        }
    }

    /// Settings changed: the panels and docks again.
    fn rebuild_bars(&mut self) {
        self.surfs.retain(|s| matches!(s.kind, Kind::Background | Kind::Popup));
        for o in self.output_state.outputs().collect::<Vec<_>>() {
            self.bars_for(&o);
        }
        for s in &mut self.surfs {
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
        if matches!(p.kind, ui::PopupKind::Launcher { .. }) {
            self.apps = load_apps();
        }
        self.popup = Some(p);
        let s = self.new_surf(Kind::Popup, output.as_ref());
        self.surfs.push(s);
    }

    fn close_popup(&mut self) {
        self.popup = None;
        self.surfs.retain(|s| s.kind != Kind::Popup);
    }

    fn index_of(&self, surface: &wl_surface::WlSurface) -> Option<usize> {
        self.surfs.iter().position(|s| s.layer.wl_surface() == surface)
    }

    // ---------------------------------------------------------------- the loop

    fn on_msg(&mut self, m: Msg) {
        match m {
            Msg::Wake => {}
            Msg::Ipc(line) => {
                let (cmd, arg) = line.trim().split_once(' ').map(|(a, b)| (a, b.trim())).unwrap_or((line.trim(), ""));
                match cmd {
                    "launcher" => self.open_popup(Popup::launcher("")),
                    "settings" => open_settings(),
                    "reload" => self.reload_settings(true),
                    "wallpaper" if !arg.is_empty() => {
                        self.s.wallpaper = arg.to_string();
                        self.s.save();
                        self.s_mtime = crate::config::mtime();
                        self.wallpaper_changed(true);
                    }
                    "quit" => self.exit = true,
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

    /// Twice a second: settings, the desktop's colours, the status, the clock, repaints asked for.
    fn tick(&mut self) {
        if crate::config::mtime() != self.s_mtime {
            self.reload_settings(false);
        }
        let now = Instant::now();
        let watch = palette_files();
        if watch != self.palette_watch {
            self.palette_watch = watch;
            // Each surface checks the colours as it draws (every 2 s): keep drawing for a moment.
            self.recolor_until = Some(now + Duration::from_secs(3));
        }
        let recolor = self.recolor_until.is_some_and(|t| now < t);
        let generation = self.live.generation();
        let status_changed = generation != self.last_gen;
        self.last_gen = generation;
        let minute = chrono::Timelike::minute(&chrono::Local::now());
        let new_minute = minute != self.last_minute;
        self.last_minute = minute;
        for s in &mut self.surfs {
            let clocked = matches!(s.kind, Kind::Panel) || (s.kind == Kind::Background && self.s.widget_clock);
            let shows_status = matches!(s.kind, Kind::Panel | Kind::Dock) || (s.kind == Kind::Background && self.s.widget_system);
            if recolor || (new_minute && clocked) || (status_changed && shows_status) || s.repaint_at.is_some_and(|t| now >= t) {
                s.dirty = true;
            }
        }
    }

    fn reload_settings(&mut self, force: bool) {
        self.s_mtime = crate::config::mtime();
        let new = Settings::load();
        if new == self.s && !force {
            return;
        }
        let wall_changed = new.wallpaper != self.s.wallpaper || new.color_mode != self.s.color_mode || new.scheme_type != self.s.scheme_type;
        self.s = new;
        self.apps = load_apps();
        self.rebuild_bars();
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

    /// Draw every surface that needs it.
    fn redraw(&mut self) {
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

    fn wall_for(&mut self, size: (u32, u32)) -> Option<&[u8]> {
        if self.s.wallpaper.is_empty() {
            return None;
        }
        let path = PathBuf::from(&self.s.wallpaper);
        if let Some(i) = self.walls.iter().position(|w| w.path == path && w.size == size) {
            return Some(&self.walls[i].pixels);
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
        let (w, h) = self.surfs[i].size;
        if w == 0 || h == 0 {
            return;
        }
        let scale = self.surfs[i].scale;
        let (pw, ph) = self.surfs[i].physical();
        let wall = if kind == Kind::Background { self.wall_for((pw, ph)).map(|p| p.to_vec()) } else { None };
        let Shell { surfs, s: settings, apps, popup, comp, .. } = self;
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
        let mut plain = egui::Color32::from_rgb(30, 30, 34);
        let out = surf.ctx.run_ui(raw, |ui| {
            surf.kit.begin(ui.ctx(), settings);
            plain = ui.visuals().extreme_bg_color;
            let mut m = Model { s: settings, status: &status, apps, compositor: *comp, output: &output_name, acts: &mut acts };
            match kind {
                Kind::Background => ui::widgets(ui, &mut m, &mut surf.kit),
                Kind::Panel => ui::panel(ui, &mut m, &mut surf.kit),
                Kind::Dock => ui::dock(ui, &mut m, &mut surf.kit),
                Kind::Popup => {
                    if let Some(p) = popup.as_mut() {
                        ui::popup(ui, p, &mut m, &mut surf.kit);
                    }
                }
            }
        });
        surf.tex.update(&out.textures_delta);
        let prims = surf.ctx.tessellate(out.shapes, scale as f32);
        let stride = pw as i32 * 4;
        let Ok((buffer, canvas)) = surf.pool.create_buffer(pw as i32, ph as i32, stride, wl_shm::Format::Argb8888) else { return };
        match (kind, &wall) {
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
        soft::draw(canvas, pw as usize, ph as usize, soft::Order::Bgra, &prims, &surf.tex, scale as f32);
        let ws = surf.layer.wl_surface();
        ws.damage_buffer(0, 0, pw as i32, ph as i32);
        ws.frame(&self.qh, ws.clone());
        if buffer.attach_to(ws).is_ok() {
            surf.layer.commit();
            surf.waiting_frame = true;
        }
        // Animations and hover effects ask to be drawn again soon.
        let delay = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        surf.repaint_at = (delay < Duration::from_secs(1)).then(|| Instant::now() + delay.max(Duration::from_millis(16)));
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
            Act::Workspace(k) => sys::focus_workspace(self.comp, &k),
            Act::Open(p) => self.open_popup(p),
            Act::Close => self.close_popup(),
            Act::Volume(up) => sys::change_volume(up),
            Act::Mute => sys::toggle_mute(),
            Act::Power(p) => {
                self.close_popup();
                sys::power(self.comp, p);
            }
            Act::Settings => open_settings(),
        }
        for s in &mut self.surfs {
            s.dirty = true;
        }
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
        let bg = self.new_surf(Kind::Background, Some(&output));
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
                    if let Some(p) = &self.pointer {
                        let _ = p.set_cursor(&self.conn, CursorIcon::Default);
                    }
                    s.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => s.events.push(egui::Event::PointerGone),
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
