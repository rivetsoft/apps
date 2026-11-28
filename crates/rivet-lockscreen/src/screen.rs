//! The Wayland program that shows the face on every screen and asks for the password:
//!
//! - `Mode::Lock` locks the session with the session-lock protocol (ext-session-lock-v1: niri,
//!   Hyprland, Sway, Softcore and the other wlroots compositors). If it dies while locked, the
//!   compositor keeps the screens locked.
//! - `Mode::Preview` shows the screen over the desktop on a layer without locking (Escape
//!   leaves), to try the look and the password safely.
//! - `Mode::Greet` is greetd's login screen: a layer over every screen where there's layer
//!   shell, else one full-screen window (cage), with Tab going through the accounts.
//!
//! SIGUSR1 unlocks. The password is checked by the program's own `check` on a thread: it can
//! take seconds (a wrong password waits on purpose).

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use calloop::channel::{Event as ChanEvent, Sender, channel};
use calloop::timer::{TimeoutAction, Timer};
use calloop::{EventLoop, LoopHandle};
use calloop_wayland_source::WaylandSource;
use eframe::egui::{self, Pos2, Rect, Vec2, ViewportId};
use rivet_kit::soft;
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers};
use smithay_client_toolkit::seat::pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::session_lock::{SessionLock, SessionLockHandler, SessionLockState, SessionLockSurface, SessionLockSurfaceConfigure};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::shell::xdg::XdgShell;
use smithay_client_toolkit::shell::xdg::window::{Window, WindowConfigure, WindowDecorations, WindowHandler};
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_session_lock, delegate_shm, delegate_xdg_shell, delegate_xdg_window, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::face::{self, LockAct};
use crate::look::Theme;
use crate::{Person, wipe};

/// Set by SIGUSR1: unlock.
static UNLOCK: AtomicBool = AtomicBool::new(false);

extern "C" fn on_usr1(_: libc::c_int) {
    UNLOCK.store(true, Ordering::Relaxed);
}

/// The exit code when another program holds the lock (or the compositor said no).
pub const EXIT_TAKEN: u8 = 2;

/// What the program is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Lock the session.
    Lock,
    /// Show the lock screen over the desktop, to try it.
    Preview,
    /// greetd's login screen.
    Greet,
}

/// Checks a password: (login, password, the session to start) to Ok, or what to tell them.
pub type Check = fn(&str, &str, &[String]) -> Result<(), String>;

/// What to show, and how to check the password.
pub struct Screen {
    pub mode: Mode,
    /// Who can unlock or sign in: the first is shown first, and Tab goes through the rest
    /// (the greeter's).
    pub people: Vec<Person>,
    pub check: Check,
    /// Told when the compositor has locked the screens (true), and when they unlock (false).
    pub on_locked: fn(bool),
    /// The surfaces' app id and layer namespace.
    pub app_id: &'static str,
    /// The window's title, where it's a window.
    pub title: &'static str,
}

enum LMsg {
    Auth(Result<(), String>),
    Bg { size: (u32, u32), pixels: Option<Vec<u8>> },
}

enum Role {
    Lock(SessionLockSurface),
    Layer(LayerSurface),
    /// A full-screen window: the greeter in a compositor with no layer shell (cage).
    Window(Window),
}

impl Role {
    fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Role::Lock(s) => s.wl_surface(),
            Role::Layer(l) => l.wl_surface(),
            Role::Window(w) => w.wl_surface(),
        }
    }
}

struct LSurf {
    role: Role,
    output: wl_output::WlOutput,
    size: (u32, u32),
    scale: i32,
    configured: bool,
    dirty: bool,
    waiting_frame: bool,
    repaint_at: Option<Instant>,
    ctx: egui::Context,
    theme: Theme,
    tex: soft::Textures,
    pool: SlotPool,
    events: Vec<egui::Event>,
    start: Instant,
    avatar: Option<egui::TextureHandle>,
}

struct Lock {
    conn: Connection,
    qh: QueueHandle<Lock>,
    handle: LoopHandle<'static, Lock>,
    tx: Sender<LMsg>,
    registry: RegistryState,
    seat_state: SeatState,
    output_state: OutputState,
    compositor: CompositorState,
    shm: Shm,
    layer_shell: Option<LayerShell>,
    xdg: Option<XdgShell>,
    session: Option<SessionLock>,
    locked: bool,
    screen: Screen,
    who: usize,
    surfs: Vec<LSurf>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    avatar: Option<egui::ColorImage>,
    typed: String,
    busy: bool,
    message: Option<(String, bool)>,
    error_at: Option<Instant>,
    fails: u32,
    confirm: Option<(LockAct, Instant)>,
    bgs: Vec<((u32, u32), Vec<u8>)>,
    loading: Vec<(u32, u32)>,
    minute: u32,
    exit: Option<u8>,
}

/// Show the screen until the password is taken (or, previewing, Escape), and return the exit
/// code: 0, or `EXIT_TAKEN` when the compositor wouldn't lock. Err says why it couldn't run.
pub fn run(screen: Screen) -> Result<u8, String> {
    if screen.people.is_empty() {
        return Err("no accounts to sign in to (none with a home and a login shell in /etc/passwd)".into());
    }
    unsafe {
        libc::signal(libc::SIGUSR1, on_usr1 as *const () as libc::sighandler_t);
    }
    let mode = screen.mode;
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session ({e})"))?;
    let (globals, queue) = registry_queue_init::<Lock>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no shared memory (wl_shm)")?;
    let (lock_state, layer_shell, xdg) = match mode {
        Mode::Preview => (None, Some(LayerShell::bind(&globals, &qh).map_err(|_| "no layer-shell protocol to show the preview with")?), None),
        Mode::Lock => (Some(SessionLockState::new(&globals, &qh)), None, None),
        // A layer over every screen where there's layer shell; else one full-screen window.
        Mode::Greet => match LayerShell::bind(&globals, &qh) {
            Ok(l) => (None, Some(l), None),
            Err(_) => (None, None, Some(XdgShell::bind(&globals, &qh).map_err(|_| "the compositor offers neither layer shell nor windows to show the greeter in")?)),
        },
    };
    let mut event_loop: EventLoop<'static, Lock> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone()).map_err(|e| e.to_string())?;
    let (tx, rx) = channel::<LMsg>();
    handle
        .insert_source(rx, |ev, _, l: &mut Lock| {
            if let ChanEvent::Msg(m) = ev {
                l.on_msg(m);
            }
        })
        .map_err(|e| e.to_string())?;
    handle
        .insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, l: &mut Lock| {
            l.tick();
            TimeoutAction::ToDuration(Duration::from_millis(500))
        })
        .map_err(|e| e.to_string())?;
    let mut l = Lock {
        conn,
        qh: qh.clone(),
        handle: handle.clone(),
        tx,
        registry: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        shm,
        layer_shell,
        xdg,
        session: None,
        locked: false,
        screen,
        who: 0,
        surfs: Vec::new(),
        pointer: None,
        keyboard: None,
        mods: Modifiers::default(),
        avatar: None,
        typed: String::new(),
        busy: false,
        message: None,
        error_at: None,
        fails: 0,
        confirm: None,
        bgs: Vec::new(),
        loading: Vec::new(),
        minute: 99,
        exit: None,
    };
    l.show_person(0);
    if let Some(ls) = &lock_state {
        l.session = Some(ls.lock(&qh).map_err(|_| "this compositor doesn't offer the session-lock protocol (ext-session-lock-v1)")?);
    }
    loop {
        let wait = l.surfs.iter().filter_map(|s| s.repaint_at).min().map(|t| t.saturating_duration_since(Instant::now())).unwrap_or(Duration::from_millis(250)).min(Duration::from_millis(250));
        event_loop.dispatch(Some(wait), &mut l).map_err(|e| e.to_string())?;
        if UNLOCK.swap(false, Ordering::Relaxed) {
            l.unlock();
        }
        if let Some(code) = l.exit {
            let _ = l.conn.flush();
            return Ok(code);
        }
        l.redraw();
    }
}

impl Lock {
    fn person(&self) -> &Person {
        &self.screen.people[self.who]
    }

    fn add_output(&mut self, output: &wl_output::WlOutput) {
        if self.surfs.iter().any(|s| &s.output == output) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let role = if let Some(xdg) = &self.xdg {
            // One window, full screen on the first screen.
            if !self.surfs.is_empty() {
                return;
            }
            let w = xdg.create_window(surface, WindowDecorations::None, &self.qh);
            w.set_title(self.screen.title);
            w.set_app_id(self.screen.app_id);
            w.set_fullscreen(Some(output));
            w.commit();
            Role::Window(w)
        } else if let Some(ls) = &self.layer_shell {
            let layer = ls.create_layer_surface(&self.qh, surface, Layer::Overlay, Some(self.screen.app_id), Some(output));
            layer.set_anchor(Anchor::all());
            layer.set_exclusive_zone(-1);
            layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
            layer.set_size(0, 0);
            layer.commit();
            Role::Layer(layer)
        } else if let Some(session) = &self.session {
            Role::Lock(session.create_lock_surface(surface, output, &self.qh))
        } else {
            return;
        };
        let scale = self.output_state.info(output).map(|i| i.scale_factor).unwrap_or(1).max(1);
        role.wl_surface().set_buffer_scale(scale);
        let pool = SlotPool::new(256 * 256 * 4, &self.shm).expect("shared memory for a surface");
        self.surfs.push(LSurf {
            role,
            output: output.clone(),
            size: (0, 0),
            scale,
            configured: false,
            dirty: true,
            waiting_frame: false,
            repaint_at: None,
            ctx: egui::Context::default(),
            theme: Theme::new(true),
            tex: soft::Textures::default(),
            pool,
            events: Vec::new(),
            start: Instant::now(),
            avatar: None,
        });
    }

    /// Show this person, with their name, picture, wallpaper and look.
    fn show_person(&mut self, i: usize) {
        if i >= self.screen.people.len() {
            return;
        }
        self.who = i;
        self.avatar = crate::avatar(&self.person().faces);
        self.bgs.clear();
        self.loading.clear();
        wipe(&mut self.typed);
        self.message = None;
        self.fails = 0;
        for s in &mut self.surfs {
            s.avatar = None;
            s.dirty = true;
        }
    }

    fn configured(&mut self, surface: &wl_surface::WlSurface, size: (u32, u32)) {
        if let Some(s) = self.surfs.iter_mut().find(|s| s.role.wl_surface() == surface) {
            s.size = (size.0.max(1), size.1.max(1));
            s.configured = true;
            s.waiting_frame = false;
            s.dirty = true;
        }
    }

    fn dirty_all(&mut self) {
        for s in &mut self.surfs {
            s.dirty = true;
        }
    }

    fn on_msg(&mut self, m: LMsg) {
        match m {
            LMsg::Auth(r) => {
                self.busy = false;
                match r {
                    Ok(()) => self.unlock(),
                    Err(e) => {
                        self.fails += 1;
                        self.error_at = Some(Instant::now());
                        let more = if self.fails >= 3 { format!(" ({} tries)", self.fails) } else { String::new() };
                        self.message = Some((format!("{e}{more}"), true));
                    }
                }
            }
            LMsg::Bg { size, pixels } => {
                self.loading.retain(|s| *s != size);
                if let Some(px) = pixels {
                    self.bgs.push((size, px));
                }
            }
        }
        self.dirty_all();
    }

    /// Twice a second: the clock, the shaking field, a power button's question running out.
    fn tick(&mut self) {
        let minute = chrono::Timelike::minute(&chrono::Local::now());
        let mut d = minute != self.minute;
        self.minute = minute;
        if self.confirm.is_some_and(|(_, t)| t.elapsed() > Duration::from_secs(4)) {
            self.confirm = None;
            d = true;
        }
        if self.error_at.is_some_and(|t| t.elapsed() < Duration::from_secs(3)) {
            d = true;
        }
        if d {
            self.dirty_all();
        }
    }

    fn submit(&mut self) {
        if self.busy || self.typed.is_empty() {
            return;
        }
        self.busy = true;
        self.message = None;
        let (user, session) = (self.person().login.clone(), self.person().session.clone());
        let (mut pw, tx, check) = (std::mem::take(&mut self.typed), self.tx.clone(), self.screen.check);
        std::thread::spawn(move || {
            let r = check(&user, &pw, &session);
            wipe(&mut pw);
            let _ = tx.send(LMsg::Auth(r));
        });
    }

    fn unlock(&mut self) {
        wipe(&mut self.typed);
        if let Some(s) = self.session.take()
            && self.locked
        {
            s.unlock();
        }
        self.surfs.clear();
        if self.locked {
            (self.screen.on_locked)(false);
        }
        self.exit = Some(0);
    }

    fn power(&mut self, a: LockAct) {
        // Twice within a few seconds (a click by accident does nothing).
        if self.confirm.map(|(c, _)| c) != Some(a) {
            self.confirm = Some((a, Instant::now()));
            return;
        }
        self.confirm = None;
        if let Ok(mut child) = std::process::Command::new("systemctl").arg(a.systemctl()).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn() {
            std::thread::spawn(move || child.wait());
        }
    }

    fn key(&mut self, e: &KeyEvent) {
        let ctrl = self.mods.ctrl;
        let mode = self.screen.mode;
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => self.submit(),
            Keysym::Escape => {
                if mode == Mode::Preview && self.typed.is_empty() {
                    self.exit = Some(0);
                }
                wipe(&mut self.typed);
            }
            // The greeter: another account.
            Keysym::Tab | Keysym::ISO_Left_Tab | Keysym::Down | Keysym::Up if mode == Mode::Greet && !self.busy && self.screen.people.len() > 1 => {
                let n = self.screen.people.len();
                let back = matches!(e.keysym, Keysym::ISO_Left_Tab | Keysym::Up) || self.mods.shift;
                self.show_person(if back { (self.who + n - 1) % n } else { (self.who + 1) % n });
            }
            Keysym::BackSpace if ctrl => wipe(&mut self.typed),
            Keysym::BackSpace => {
                self.typed.pop();
            }
            Keysym::u | Keysym::U if ctrl => wipe(&mut self.typed),
            _ if self.busy || ctrl => {}
            _ => {
                if let Some(t) = e.utf8.as_ref().filter(|t| !t.is_empty() && t.chars().all(|c| !c.is_control()))
                    && self.typed.len() < 512
                {
                    self.typed.push_str(t);
                    if self.message.as_ref().is_some_and(|m| m.1) {
                        self.message = None;
                    }
                }
            }
        }
        self.dirty_all();
    }

    fn bg_for(&mut self, size: (u32, u32)) -> Option<usize> {
        if let Some(i) = self.bgs.iter().position(|(s, _)| *s == size) {
            return Some(i);
        }
        let (wallpaper, blur) = (self.person().wallpaper.clone(), self.person().look.blur);
        if let Some(path) = wallpaper.filter(|w| !w.as_os_str().is_empty())
            && !self.loading.contains(&size)
        {
            self.loading.push(size);
            let tx = self.tx.clone();
            std::thread::spawn(move || {
                let pixels = crate::background(&path, size.0, size.1, blur);
                let _ = tx.send(LMsg::Bg { size, pixels });
            });
        }
        None
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        for i in 0..self.surfs.len() {
            let s = &mut self.surfs[i];
            if s.repaint_at.is_some_and(|t| now >= t) {
                s.repaint_at = None;
                s.dirty = true;
            }
            if s.dirty && s.configured && !s.waiting_frame {
                self.draw(i);
            }
        }
    }

    fn draw(&mut self, i: usize) {
        let (w, h) = self.surfs[i].size;
        let scale = self.surfs[i].scale;
        let (pw, ph) = (w * scale as u32, h * scale as u32);
        let bg = self.bg_for((pw, ph));
        let mut acts = Vec::new();
        let several = self.screen.mode == Mode::Greet && self.screen.people.len() > 1 && self.typed.is_empty();
        let Lock { surfs, screen, who, avatar, typed, busy, message, error_at, confirm, mods, bgs, .. } = self;
        let person = &screen.people[*who];
        let look = &person.look;
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
        if surf.avatar.is_none()
            && let Some(img) = avatar.as_ref()
        {
            surf.avatar = Some(surf.ctx.load_texture("lock-avatar", img.clone(), egui::TextureOptions::LINEAR));
        }
        let out = surf.ctx.run_ui(raw, |ui| {
            surf.theme.begin(ui.ctx(), look);
            let view = face::View {
                name: &person.name,
                avatar: surf.avatar.as_ref(),
                typed: typed.chars().count(),
                caps_lock: mods.caps_lock,
                busy: *busy,
                message: message.as_ref().map(|(m, bad)| (m.as_str(), *bad)).or(several.then_some((face::ANOTHER, false))),
                since_error: error_at.map(|t| t.elapsed()),
                power: look.power,
                confirm: confirm.map(|(a, _)| a),
                accent: surf.theme.accent(look),
                clock_format: &look.clock_format,
            };
            acts = face::draw(ui, &view);
        });
        let mut out = out;
        let mut delta = std::mem::take(&mut out.textures_delta);
        surf.tex.update(&delta);
        delta.clear();
        let prims = surf.ctx.tessellate(out.shapes, scale as f32);
        let Ok((buffer, canvas)) = surf.pool.create_buffer(pw as i32, ph as i32, pw as i32 * 4, wl_shm::Format::Argb8888) else { return };
        match bg.and_then(|i| bgs.get(i)) {
            Some((_, px)) if px.len() == canvas.len() => canvas.copy_from_slice(px),
            _ => {
                // No wallpaper (yet): a deep gray, a little lighter at the top.
                for y in 0..ph as usize {
                    let t = y as f32 / ph.max(1) as f32;
                    let c = (34.0 - 16.0 * t) as u8;
                    let row = [c + 2, c, c, 255];
                    for px in canvas[y * pw as usize * 4..(y + 1) * pw as usize * 4].as_chunks_mut::<4>().0 {
                        *px = row;
                    }
                }
            }
        }
        soft::draw(canvas, pw as usize, ph as usize, soft::Order::Bgra, &prims, &surf.tex, scale as f32);
        let ws = surf.role.wl_surface();
        ws.damage_buffer(0, 0, pw as i32, ph as i32);
        ws.frame(&self.qh, ws.clone());
        if buffer.attach_to(ws).is_ok() {
            ws.commit();
            surf.waiting_frame = true;
        }
        let delay = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.repaint_delay).unwrap_or(Duration::MAX);
        surf.repaint_at = (delay < Duration::from_secs(1)).then(|| Instant::now() + delay.max(Duration::from_millis(16)));
        for a in acts {
            self.power(a);
            self.dirty_all();
        }
    }
}

// ------------------------------------------------------------------ Wayland's callbacks

impl SessionLockHandler for Lock {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        self.locked = true;
        (self.screen.on_locked)(true);
    }
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // Never locked: another program holds the lock (or the compositor said no).
        if !self.locked {
            eprintln!("{}: the compositor didn't lock (is another lock screen running?)", self.screen.app_id);
            self.exit = Some(EXIT_TAKEN);
        } else {
            (self.screen.on_locked)(false);
            self.exit = Some(0);
        }
        self.session = None;
        self.surfs.clear();
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: SessionLockSurface, cfg: SessionLockSurfaceConfigure, _: u32) {
        self.configured(surface.wl_surface(), cfg.new_size);
    }
}

impl LayerShellHandler for Lock {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface) {
        self.surfs.retain(|s| s.role.wl_surface() != layer.wl_surface());
        if self.surfs.is_empty() {
            self.exit = Some(0);
        }
    }
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, layer: &LayerSurface, cfg: LayerSurfaceConfigure, _: u32) {
        self.configured(layer.wl_surface(), cfg.new_size);
    }
}

impl WindowHandler for Lock {
    fn request_close(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &Window) {}
    fn configure(&mut self, _: &Connection, _: &QueueHandle<Self>, window: &Window, cfg: WindowConfigure, _: u32) {
        // The compositor's size; with none, the screen's.
        let screen = self.surfs.iter().find(|s| s.role.wl_surface() == window.wl_surface()).and_then(|s| self.output_state.info(&s.output)).and_then(|i| i.logical_size).map(|(w, h)| (w.max(1) as u32, h.max(1) as u32));
        let size = match cfg.new_size {
            (Some(w), Some(h)) => (w.get(), h.get()),
            _ => screen.unwrap_or((1280, 800)),
        };
        self.configured(window.wl_surface(), size);
    }
}

impl CompositorHandler for Lock {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, factor: i32) {
        if let Some(s) = self.surfs.iter_mut().find(|s| s.role.wl_surface() == surface) {
            s.scale = factor.max(1);
            s.role.wl_surface().set_buffer_scale(s.scale);
            s.dirty = true;
        }
    }
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, _: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(s) = self.surfs.iter_mut().find(|s| s.role.wl_surface() == surface) {
            s.waiting_frame = false;
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Lock {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.add_output(&output);
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, output: wl_output::WlOutput) {
        self.surfs.retain(|s| s.output != output);
    }
}

impl SeatHandler for Lock {
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
            // Held keys repeat (Backspace, mostly).
            self.keyboard = self.seat_state.get_keyboard_with_repeat(qh, &seat, None, self.handle.clone(), Box::new(|l: &mut Lock, _, e| l.key(&e))).ok();
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

impl KeyboardHandler for Lock {
    fn enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32, _: &[u32], _: &[Keysym]) {}
    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: &wl_surface::WlSurface, _: u32) {}
    fn press_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e);
    }
    fn repeat_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, e: KeyEvent) {
        self.key(&e);
    }
    fn release_key(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, _: KeyEvent) {}
    fn update_modifiers(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_keyboard::WlKeyboard, _: u32, m: Modifiers, _: RawModifiers, _: u32) {
        if m.caps_lock != self.mods.caps_lock {
            self.dirty_all();
        }
        self.mods = m;
    }
}

impl PointerHandler for Lock {
    fn pointer_frame(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            let Some(s) = self.surfs.iter_mut().find(|s| s.role.wl_surface() == &e.surface) else { continue };
            let pos = Pos2::new(e.position.0 as f32, e.position.1 as f32);
            match &e.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(p) = &self.pointer {
                        let _ = p.set_cursor(&self.conn, CursorIcon::Default);
                    }
                    s.events.push(egui::Event::PointerMoved(pos));
                }
                PointerEventKind::Leave { .. } => s.events.push(egui::Event::PointerGone),
                PointerEventKind::Motion { .. } => s.events.push(egui::Event::PointerMoved(pos)),
                PointerEventKind::Press { button, .. } | PointerEventKind::Release { button, .. } if *button == 0x110 => {
                    let pressed = matches!(e.kind, PointerEventKind::Press { .. });
                    s.events.push(egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() });
                }
                _ => continue,
            }
            s.dirty = true;
        }
    }
}

impl ShmHandler for Lock {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Lock {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(Lock);
delegate_output!(Lock);
delegate_shm!(Lock);
delegate_seat!(Lock);
delegate_keyboard!(Lock);
delegate_pointer!(Lock);
delegate_layer!(Lock);
delegate_session_lock!(Lock);
delegate_xdg_shell!(Lock);
delegate_xdg_window!(Lock);
delegate_registry!(Lock);

