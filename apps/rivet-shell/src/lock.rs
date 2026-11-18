//! `rivet-shell lock`: the lock screen, its own program (as swaylock and hyprlock are), so the
//! session stays locked even if the shell stops. It locks with the session-lock protocol
//! (ext-session-lock-v1: niri, Hyprland, Sway, mango and the other wlroots compositors), shows
//! lock_ui's face on every screen, and unlocks when PAM takes the password (auth.rs).
//!
//! If it dies while locked, the compositor keeps the screens locked; the shell starts it again
//! (it sees $XDG_RUNTIME_DIR/rivet-shell.locked still there). SIGUSR1 unlocks (what
//! `loginctl unlock-session` asks of the shell). `rivet-shell lock --preview` shows it over the
//! desktop without locking (Escape leaves), to try the look and the password safely.

use std::path::PathBuf;
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
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_session_lock, delegate_shm, registry_handlers};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, QueueHandle};

use crate::config::Settings;
use crate::lock_ui::{self, LockAct};

/// Set by SIGUSR1: unlock.
static UNLOCK: AtomicBool = AtomicBool::new(false);

extern "C" fn on_usr1(_: libc::c_int) {
    UNLOCK.store(true, Ordering::Relaxed);
}

/// Left in the runtime directory while the screens are locked by this program.
pub fn marker() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from).unwrap_or_else(std::env::temp_dir).join("rivet-shell.locked")
}

/// Exit codes the shell tells apart.
pub const EXIT_TAKEN: u8 = 2;

enum LMsg {
    Auth(Result<(), String>),
    Bg { size: (u32, u32), pixels: Option<Vec<u8>> },
}

enum Role {
    Lock(SessionLockSurface),
    Layer(LayerSurface),
}

impl Role {
    fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Role::Lock(s) => s.wl_surface(),
            Role::Layer(l) => l.wl_surface(),
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
    kit: crate::ui::Kit,
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
    lock_state: Option<SessionLockState>,
    layer_shell: Option<LayerShell>,
    session: Option<SessionLock>,
    locked: bool,
    preview: bool,
    surfs: Vec<LSurf>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    mods: Modifiers,
    s: Settings,
    login: String,
    name: String,
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

/// Wipe a password from memory, then empty it.
fn wipe(s: &mut String) {
    // SAFETY: zeros are valid UTF-8.
    unsafe { s.as_bytes_mut().fill(0) };
    s.clear();
}

/// The person's picture: ~/.face, else AccountsService's.
fn avatar(login: &str) -> Option<egui::ColorImage> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let paths = [home.join(".face"), home.join(".face.icon"), PathBuf::from(format!("/var/lib/AccountsService/icons/{login}"))];
    let img = paths.iter().filter(|p| p.is_file()).find_map(|p| image::ImageReader::open(p).ok()?.with_guessed_format().ok()?.decode().ok())?;
    let side = img.width().min(img.height());
    let sq = img.crop_imm((img.width() - side) / 2, (img.height() - side) / 2, side, side).resize_exact(192, 192, image::imageops::FilterType::Triangle).to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied([192, 192], sq.as_raw()))
}

/// The wallpaper for a screen of this size (BGRA), blurred when asked.
fn background(path: &str, w: u32, h: u32, blur: bool) -> Option<Vec<u8>> {
    if path.is_empty() {
        return None;
    }
    let px = crate::wall::cover_bgra(std::path::Path::new(path), w, h)?;
    if !blur {
        return Some(px);
    }
    // Down to a small picture, softened, and back up: a wide blur for little work.
    let img = image::RgbaImage::from_raw(w, h, px)?;
    let (sw, sh) = ((w / 24).max(8), (h / 24).max(8));
    let small = image::imageops::resize(&img, sw, sh, image::imageops::FilterType::Triangle);
    let small = image::imageops::blur(&small, 2.0);
    let big = image::imageops::resize(&small, w, h, image::imageops::FilterType::CatmullRom);
    Some(big.into_raw())
}

/// Lock the session (or, with `preview`, only show the lock screen) until the password is given.
pub fn run(preview: bool) -> std::process::ExitCode {
    match run_inner(preview) {
        Ok(code) => std::process::ExitCode::from(code),
        Err(e) => {
            eprintln!("rivet-shell lock: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run_inner(preview: bool) -> Result<u8, String> {
    unsafe {
        libc::signal(libc::SIGUSR1, on_usr1 as *const () as libc::sighandler_t);
    }
    let conn = Connection::connect_to_env().map_err(|e| format!("no Wayland session ({e})"))?;
    let (globals, queue) = registry_queue_init::<Lock>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let compositor = CompositorState::bind(&globals, &qh).map_err(|_| "the compositor has no wl_compositor")?;
    let shm = Shm::bind(&globals, &qh).map_err(|_| "the compositor has no shared memory (wl_shm)")?;
    let (lock_state, layer_shell) = if preview {
        (None, Some(LayerShell::bind(&globals, &qh).map_err(|_| "no layer-shell protocol to show the preview with")?))
    } else {
        (Some(SessionLockState::new(&globals, &qh)), None)
    };
    let mut event_loop: EventLoop<'static, Lock> = EventLoop::try_new().map_err(|e| e.to_string())?;
    let handle = event_loop.handle();
    WaylandSource::new(conn.clone(), queue).insert(handle.clone()).map_err(|e| e.to_string())?;
    let (tx, rx) = channel::<LMsg>();
    handle.insert_source(rx, |ev, _, l: &mut Lock| {
        if let ChanEvent::Msg(m) = ev {
            l.on_msg(m);
        }
    }).map_err(|e| e.to_string())?;
    handle.insert_source(Timer::from_duration(Duration::from_millis(500)), |_, _, l: &mut Lock| {
        l.tick();
        TimeoutAction::ToDuration(Duration::from_millis(500))
    }).map_err(|e| e.to_string())?;
    let login = crate::auth::login_name();
    let s = Settings::load();
    let mut l = Lock {
        conn: conn.clone(),
        qh: qh.clone(),
        handle: handle.clone(),
        tx,
        registry: RegistryState::new(&globals),
        seat_state: SeatState::new(&globals, &qh),
        output_state: OutputState::new(&globals, &qh),
        compositor,
        shm,
        lock_state,
        layer_shell,
        session: None,
        locked: false,
        preview,
        surfs: Vec::new(),
        pointer: None,
        keyboard: None,
        mods: Modifiers::default(),
        name: crate::sys::user_name(),
        avatar: avatar(&login),
        login,
        s,
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
    if let Some(ls) = &l.lock_state {
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
    fn add_output(&mut self, output: &wl_output::WlOutput) {
        if self.surfs.iter().any(|s| &s.output == output) {
            return;
        }
        let surface = self.compositor.create_surface(&self.qh);
        let role = if let Some(ls) = &self.layer_shell {
            let layer = ls.create_layer_surface(&self.qh, surface, Layer::Overlay, Some("rivet-shell-lock"), Some(output));
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
            kit: crate::ui::Kit::new(true),
            tex: soft::Textures::default(),
            pool,
            events: Vec::new(),
            start: Instant::now(),
            avatar: None,
        });
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
        let (user, mut pw, tx) = (self.login.clone(), std::mem::take(&mut self.typed), self.tx.clone());
        std::thread::spawn(move || {
            let r = crate::auth::check(&user, &pw);
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
            let _ = std::fs::remove_file(marker());
            locked_hint(false);
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
        let args: &[&str] = match a {
            LockAct::Suspend => &["suspend"],
            LockAct::Restart => &["reboot"],
            LockAct::ShutDown => &["poweroff"],
        };
        crate::sys::spawn("systemctl", args);
    }

    fn key(&mut self, e: &KeyEvent) {
        let ctrl = self.mods.ctrl;
        match e.keysym {
            Keysym::Return | Keysym::KP_Enter => self.submit(),
            Keysym::Escape => {
                if self.preview && self.typed.is_empty() {
                    self.exit = Some(0);
                }
                wipe(&mut self.typed);
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
        if !self.loading.contains(&size) && !self.s.wallpaper.is_empty() {
            self.loading.push(size);
            let (path, blur, tx) = (self.s.wallpaper.clone(), self.s.lock_blur, self.tx.clone());
            std::thread::spawn(move || {
                let pixels = background(&path, size.0, size.1, blur);
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
        let Lock { surfs, s, name, avatar, typed, busy, message, error_at, confirm, mods, bgs, .. } = self;
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
            surf.kit.begin(ui.ctx(), s);
            let accent = surf.kit.accent(s);
            let view = lock_ui::View {
                name,
                avatar: surf.avatar.as_ref(),
                typed: typed.chars().count(),
                caps_lock: mods.caps_lock,
                busy: *busy,
                message: message.as_ref().map(|(m, bad)| (m.as_str(), *bad)),
                since_error: error_at.map(|t| t.elapsed()),
                power: s.lock_power,
                confirm: confirm.map(|(a, _)| a),
                accent,
                clock_format: &s.clock_format,
            };
            acts = lock_ui::draw(ui, &view);
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
                // No wallpaper (yet): a deep grey, a little lighter at the top.
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

/// Tell logind the session is locked (or not), for other programs that ask it.
fn locked_hint(on: bool) {
    std::thread::spawn(move || {
        if let Ok(c) = zbus::blocking::Connection::system() {
            let _ = c.call_method(Some("org.freedesktop.login1"), "/org/freedesktop/login1/session/auto", Some("org.freedesktop.login1.Session"), "SetLockedHint", &(on,));
        }
    });
}

// ------------------------------------------------------------------ Wayland's callbacks

impl SessionLockHandler for Lock {
    fn locked(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        self.locked = true;
        let _ = std::fs::write(marker(), std::process::id().to_string());
        locked_hint(true);
    }
    fn finished(&mut self, _: &Connection, _: &QueueHandle<Self>, _: SessionLock) {
        // Never locked: another program holds the lock (or the compositor said no).
        if !self.locked {
            eprintln!("rivet-shell lock: the compositor didn't lock (is another lock screen running?)");
            self.exit = Some(EXIT_TAKEN);
        } else {
            let _ = std::fs::remove_file(marker());
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
delegate_registry!(Lock);

// ------------------------------------------------------------------ the screenshot

/// `rivet-shell lock --screenshot FILE`: the lock screen over the demo wallpaper, with made-up
/// state. Hooks: RIVET_LOCK_TYPED=N (dots), RIVET_LOCK_ERROR=1 (a wrong password),
/// RIVET_LOCK_CONFIRM=1 (Shut Down asking for a second click), RIVET_SHELL_THEME, RIVET_SHELL_ACCENT.
pub fn screenshot(path: PathBuf) -> std::process::ExitCode {
    rivet_kit::headless::run([1280.0, 800.0], move |cc| Box::new(Shot::new(cc, path)));
    std::process::ExitCode::SUCCESS
}

struct Shot {
    s: Settings,
    kit: crate::ui::Kit,
    wall: Option<egui::TextureHandle>,
    shot: rivet_kit::Shot,
}

impl Shot {
    fn new(cc: &eframe::CreationContext, path: PathBuf) -> Shot {
        crate::config::NO_SAVE.store(true, Ordering::Relaxed);
        let mut s = Settings::default();
        if let Ok(t) = std::env::var("RIVET_SHELL_THEME") {
            s.theme = if t == "light" { rivet_look::theme::Mode::Light } else { rivet_look::theme::Mode::Dark };
        }
        if let Some(c) = std::env::var("RIVET_SHELL_ACCENT").ok().and_then(|h| u32::from_str_radix(h.trim_start_matches('#'), 16).ok()) {
            s.accent = [(c >> 16) as u8, (c >> 8) as u8, c as u8];
            s.follow_desktop_colors = false;
        }
        let dir = std::env::temp_dir().join(format!("rivet-shell-lock-shot-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let wall = crate::wall::demo_png(&dir).and_then(|p| {
            let px = background(&p.to_string_lossy(), 1280, 800, true)?;
            // BGRA from cover_bgra, blurred: back to RGBA for egui.
            let rgba: Vec<u8> = px.as_chunks::<4>().0.iter().flat_map(|c| [c[2], c[1], c[0], 255]).collect();
            Some(egui::ColorImage::from_rgba_unmultiplied([1280, 800], &rgba))
        });
        let _ = std::fs::remove_dir_all(&dir);
        let wall = wall.map(|img| cc.egui_ctx.load_texture("lock-wall", img, egui::TextureOptions::LINEAR));
        Shot { s, kit: crate::ui::Kit::new(false), wall, shot: rivet_kit::Shot::new(path) }
    }
}

impl eframe::App for Shot {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.kit.begin(&ctx, &self.s);
        let full = ui.max_rect();
        if let Some(t) = &self.wall {
            ui.painter().image(t.id(), full, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), egui::Color32::WHITE);
        }
        let env = |k: &str| std::env::var(k).ok();
        let error = env("RIVET_LOCK_ERROR").as_deref() == Some("1");
        let view = lock_ui::View {
            name: "Sam Rivers",
            avatar: None,
            typed: env("RIVET_LOCK_TYPED").and_then(|v| v.parse().ok()).unwrap_or(0),
            caps_lock: error,
            busy: false,
            message: error.then_some(("Wrong password", true)),
            since_error: error.then_some(Duration::from_secs(1)),
            power: true,
            confirm: (env("RIVET_LOCK_CONFIRM").as_deref() == Some("1")).then_some(LockAct::ShutDown),
            accent: self.kit.accent(&self.s),
            clock_format: &self.s.clock_format,
        };
        lock_ui::draw(ui, &view);
        self.shot.step(&ctx);
    }
}
