//! Rivet Color's own picker, on compositors with layer-shell and screen capture (niri,
//! Hyprland, Sway and the like): each screen is captured with grim and shown frozen, full
//! size, over everything, with a loupe beside the pointer showing the pixels around it
//! enlarged and the color's code under them. Click or Enter picks; Escape or a right-click
//! stops; the wheel zooms; arrow keys move one pixel (ten with Shift).
//!
//! Drawn by hand into shared memory, not with egui: the screen picture is copied in once, and
//! each move only puts back the patch the loupe covered and draws it again in its new place.

use std::io::Read;
use std::process::{Command, Stdio};

use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::pointer::{CursorIcon, PointerEvent, PointerEventKind, PointerHandler, ThemeSpec, ThemedPointer};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_compositor, delegate_layer, delegate_output, delegate_pointer, delegate_registry, delegate_seat, delegate_shm, registry_handlers};
use wayland_client::globals::{GlobalList, registry_queue_init};
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};
use wayland_protocols::wp::viewporter::client::wp_viewport::WpViewport;
use wayland_protocols::wp::viewporter::client::wp_viewporter::WpViewporter;

use crate::color::Rgb;

/// The loupe's width and height in logical pixels, and how many screen pixels across it can
/// show (the wheel steps through these; odd, so one is in the middle).
const LOUPE: f64 = 150.0;
const ZOOMS: [i32; 6] = [5, 9, 13, 17, 25, 35];
/// The website's ink and cream: the loupe's outline, shadow and label.
const INK: [u8; 4] = [0x1b, 0x12, 0x24, 0xff];
const CREAM: [u8; 4] = [0xf3, 0xee, 0xf7, 0xff];

/// A rectangle in a screen's own pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
struct R {
    x: i32,
    y: i32,
    w: i32,
    h: i32,
}

/// A screen picture, as the four bytes a pixel the compositor wants (blue, green, red, alpha).
struct Picture {
    w: i32,
    h: i32,
    px: Vec<u8>,
}

impl Picture {
    fn at(&self, x: i32, y: i32) -> Option<Rgb> {
        if x < 0 || y < 0 || x >= self.w || y >= self.h {
            return None;
        }
        let i = ((y * self.w + x) * 4) as usize;
        Some(Rgb(self.px[i + 2], self.px[i + 1], self.px[i]))
    }
}

/// A binary PPM picture (what grim writes with `-t ppm`): its size and where its pixels start.
pub fn ppm(data: &[u8]) -> Option<(i32, i32, usize)> {
    if !data.starts_with(b"P6") {
        return None;
    }
    let mut words = Vec::new();
    let mut i = 0;
    while words.len() < 4 {
        while data.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        if data[i] == b'#' {
            while *data.get(i)? != b'\n' {
                i += 1;
            }
            continue;
        }
        let start = i;
        while !data.get(i)?.is_ascii_whitespace() {
            i += 1;
        }
        words.push(std::str::from_utf8(&data[start..i]).ok()?.to_string());
    }
    let (w, h): (i32, i32) = (words[1].parse().ok()?, words[2].parse().ok()?);
    (words[3] == "255" && data.len() >= i + 1 + (w * h * 3) as usize).then_some((w, h, i + 1))
}

/// One screen, captured whole by grim.
fn capture(output: &str) -> Result<Picture, String> {
    let mut child = Command::new("grim").args(["-o", output, "-t", "ppm", "-"]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null()).spawn().map_err(|e| format!("Couldn't run grim: {e}."))?;
    let mut data = Vec::new();
    child.stdout.take().map(|mut o| o.read_to_end(&mut data));
    let _ = child.wait();
    let (w, h, at) = ppm(&data).ok_or("Couldn't capture the screen.")?;
    let px = data[at..at + (w * h * 3) as usize].as_chunks::<3>().0.iter().flat_map(|p| [p[2], p[1], p[0], 0xff]).collect();
    Ok(Picture { w, h, px })
}

struct Buf {
    buffer: Buffer,
    /// Where the loupe is drawn in it, to put back before drawing it somewhere else.
    drawn: Option<R>,
}

struct Screen {
    layer: LayerSurface,
    viewport: WpViewport,
    picture: Picture,
    /// The screen's size in logical pixels, once the compositor has said.
    logical: (u32, u32),
    bufs: Vec<Buf>,
    /// Where the loupe is in what the compositor shows now.
    shown: Option<R>,
    waiting: bool,
    dirty: bool,
}

struct Picker {
    registry: RegistryState,
    outputs: OutputState,
    seats: SeatState,
    shm: Shm,
    compositor: CompositorState,
    pool: SlotPool,
    screens: Vec<Screen>,
    pointer: Option<ThemedPointer>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    conn: Connection,
    /// The screen the pointer is on and where, in its logical pixels.
    at: Option<(usize, f64, f64)>,
    /// Arrow keys' moves from the pointer, in screen pixels.
    nudge: (i32, i32),
    zoom: usize,
    /// Shift keys held down (left, right).
    shift: [bool; 2],
    done: Option<Option<Rgb>>,
}

/// Pick with the loupe. `Err` when this desktop can't show it (no layer-shell, no grim), so
/// another way can be tried.
pub fn pick() -> Result<Option<Rgb>, String> {
    let conn = Connection::connect_to_env().map_err(|e| e.to_string())?;
    let (globals, mut queue) = registry_queue_init::<Picker>(&conn).map_err(|e| e.to_string())?;
    let qh = queue.handle();
    let layers = LayerShell::bind(&globals, &qh).map_err(|_| "no layer-shell".to_string())?;
    let viewporter: WpViewporter = bind_viewporter(&globals, &qh)?;
    let compositor = CompositorState::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let shm = Shm::bind(&globals, &qh).map_err(|e| e.to_string())?;
    let mut p = Picker {
        registry: RegistryState::new(&globals),
        outputs: OutputState::new(&globals, &qh),
        seats: SeatState::new(&globals, &qh),
        pool: SlotPool::new(1 << 20, &shm).map_err(|e| e.to_string())?,
        shm,
        compositor,
        screens: Vec::new(),
        pointer: None,
        keyboard: None,
        conn: conn.clone(),
        at: None,
        nudge: (0, 0),
        zoom: 2,
        shift: [false; 2],
        done: None,
    };
    queue.roundtrip(&mut p).map_err(|e| e.to_string())?;
    // Every screen captured at once, before anything of ours is on them.
    let outs: Vec<(wl_output::WlOutput, String)> = p.outputs.outputs().filter_map(|o| Some((o.clone(), p.outputs.info(&o)?.name?))).collect();
    if outs.is_empty() {
        return Err("no screens".into());
    }
    let shots: Vec<_> = outs.iter().map(|(_, name)| std::thread::spawn({
        let name = name.clone();
        move || capture(&name)
    })).collect();
    for ((output, _), shot) in outs.iter().zip(shots) {
        let picture = shot.join().map_err(|_| "capture failed".to_string())??;
        let surface = p.compositor.create_surface(&qh);
        let viewport = viewporter.get_viewport(&surface, &qh, ());
        let layer = layers.create_layer_surface(&qh, surface, Layer::Overlay, Some("rivet-color-picker"), Some(output));
        layer.set_anchor(Anchor::all());
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::Exclusive);
        layer.commit();
        p.screens.push(Screen { layer, viewport, picture, logical: (0, 0), bufs: Vec::new(), shown: None, waiting: false, dirty: true });
    }
    while p.done.is_none() {
        queue.blocking_dispatch(&mut p).map_err(|e| e.to_string())?;
    }
    p.screens.clear();
    let _ = queue.roundtrip(&mut p);
    Ok(p.done.flatten())
}

fn bind_viewporter(globals: &GlobalList, qh: &QueueHandle<Picker>) -> Result<WpViewporter, String> {
    globals.bind(qh, 1..=1, ()).map_err(|_| "no viewporter".to_string())
}

impl Picker {
    /// The screen pixel picked now: the pointer's, moved by the arrow keys.
    fn target(&self, i: usize) -> Option<(i32, i32)> {
        let (on, x, y) = self.at?;
        let s = &self.screens[on];
        if on != i || s.logical.0 == 0 {
            return None;
        }
        let k = s.picture.w as f64 / s.logical.0 as f64;
        Some((((x * k).floor() as i32 + self.nudge.0).clamp(0, s.picture.w - 1), ((y * k).floor() as i32 + self.nudge.1).clamp(0, s.picture.h - 1)))
    }

    fn pick_here(&mut self) {
        let Some((on, ..)) = self.at else { return };
        if let Some((x, y)) = self.target(on) {
            self.done = Some(self.screens[on].picture.at(x, y));
        }
    }

    fn redraw_all(&mut self, qh: &QueueHandle<Picker>) {
        for i in 0..self.screens.len() {
            self.draw(i, qh);
        }
    }

    fn draw(&mut self, i: usize, qh: &QueueHandle<Picker>) {
        let target = self.target(i);
        let cells = ZOOMS[self.zoom];
        let Picker { pool, screens, .. } = self;
        let s = &mut screens[i];
        if s.logical.0 == 0 || s.waiting {
            s.dirty = true;
            return;
        }
        let (w, h) = (s.picture.w, s.picture.h);
        if s.bufs.is_empty() {
            for _ in 0..2 {
                let Ok((buffer, canvas)) = pool.create_buffer(w, h, w * 4, wl_shm::Format::Argb8888) else { return };
                canvas.copy_from_slice(&s.picture.px);
                s.bufs.push(Buf { buffer, drawn: None });
            }
            s.shown = Some(R { x: 0, y: 0, w, h });
        }
        // A buffer the compositor isn't reading; none free means wait for the next frame.
        let Some(k) = (0..s.bufs.len()).find(|&k| s.bufs[k].buffer.canvas(pool).is_some()) else {
            s.dirty = true;
            return;
        };
        let scale = w as f64 / s.logical.0 as f64;
        let b = &mut s.bufs[k];
        let canvas = b.buffer.canvas(pool).expect("a free buffer");
        if let Some(old) = b.drawn {
            restore(canvas, &s.picture, old);
        }
        let new = target.map(|(x, y)| loupe(canvas, &s.picture, x, y, scale, cells));
        b.drawn = new;
        let surface = s.layer.wl_surface();
        if b.buffer.attach_to(surface).is_err() {
            return;
        }
        for r in [s.shown, new].into_iter().flatten() {
            surface.damage_buffer(r.x, r.y, r.w, r.h);
        }
        surface.frame(qh, surface.clone());
        surface.commit();
        s.shown = new;
        s.waiting = true;
        s.dirty = false;
    }
}

/// A rectangle cut to the picture.
fn clip(r: R, w: i32, h: i32) -> Option<R> {
    let (x0, y0, x1, y1) = (r.x.max(0), r.y.max(0), (r.x + r.w).min(w), (r.y + r.h).min(h));
    (x1 > x0 && y1 > y0).then_some(R { x: x0, y: y0, w: x1 - x0, h: y1 - y0 })
}

fn restore(canvas: &mut [u8], pic: &Picture, r: R) {
    let Some(r) = clip(r, pic.w, pic.h) else { return };
    for y in r.y..r.y + r.h {
        let a = ((y * pic.w + r.x) * 4) as usize;
        let b = a + (r.w * 4) as usize;
        canvas[a..b].copy_from_slice(&pic.px[a..b]);
    }
}

fn fill(canvas: &mut [u8], pic: &Picture, r: R, c: [u8; 4]) {
    let Some(r) = clip(r, pic.w, pic.h) else { return };
    for y in r.y..r.y + r.h {
        let a = ((y * pic.w + r.x) * 4) as usize;
        for px in canvas[a..a + (r.w * 4) as usize].as_chunks_mut::<4>().0 {
            px.copy_from_slice(&c);
        }
    }
}

/// An outline `t` thick inside a rectangle.
fn frame(canvas: &mut [u8], pic: &Picture, r: R, t: i32, c: [u8; 4]) {
    fill(canvas, pic, R { h: t, ..r }, c);
    fill(canvas, pic, R { y: r.y + r.h - t, h: t, ..r }, c);
    fill(canvas, pic, R { w: t, ..r }, c);
    fill(canvas, pic, R { x: r.x + r.w - t, w: t, ..r }, c);
}

fn bgra(c: Rgb) -> [u8; 4] {
    [c.2, c.1, c.0, 0xff]
}

/// The loupe for the pixel at (`x`, `y`), beside it: the pixels around it enlarged, the middle
/// one ringed, and a label with its color and code. Returns all it covers.
fn loupe(canvas: &mut [u8], pic: &Picture, x: i32, y: i32, scale: f64, cells: i32) -> R {
    let px = |v: f64| (v * scale).round().max(1.0) as i32;
    let cell = (px(LOUPE) / cells).max(1);
    let (border, shadow, gap, off) = (px(3.0), px(5.0), px(6.0), px(22.0));
    let side = cell * cells + border * 2;
    let label_h = px(30.0);
    let (w, h) = (side + shadow, side + gap + label_h + shadow);
    // Below and right of the pointer, or the other side where the screen ends.
    let bx = if x + off + w <= pic.w { x + off } else { x - off - w };
    let by = if y + off + h <= pic.h { y + off } else { y - off - h };
    let (bx, by) = (bx.clamp(0, (pic.w - w).max(0)), by.clamp(0, (pic.h - h).max(0)));
    let boxr = R { x: bx, y: by, w: side, h: side };
    let label = R { x: bx, y: by + side + gap, w: side, h: label_h };
    for r in [boxr, label] {
        fill(canvas, pic, R { x: r.x + shadow, y: r.y + shadow, ..r }, INK);
    }
    fill(canvas, pic, boxr, INK);
    let half = cells / 2;
    for j in 0..cells {
        for i in 0..cells {
            let c = pic.at(x - half + i, y - half + j).map(bgra).unwrap_or(INK);
            fill(canvas, pic, R { x: bx + border + i * cell, y: by + border + j * cell, w: cell, h: cell }, c);
        }
    }
    // The middle pixel: white inside ink, so it shows on any color.
    let mid = R { x: bx + border + half * cell, y: by + border + half * cell, w: cell, h: cell };
    let t = px(1.5);
    frame(canvas, pic, R { x: mid.x - t, y: mid.y - t, w: mid.w + 2 * t, h: mid.h + 2 * t }, t, INK);
    frame(canvas, pic, mid, t.min(cell / 3).max(1), [0xff; 4]);
    // The label: a swatch and the code, on cream.
    let color = pic.at(x, y).unwrap_or(Rgb(0, 0, 0));
    fill(canvas, pic, label, INK);
    let inner = R { x: label.x + border, y: label.y + border, w: label.w - 2 * border, h: label.h - 2 * border };
    fill(canvas, pic, inner, CREAM);
    let sw = inner.h - 2 * px(4.0);
    let swatch = R { x: inner.x + px(4.0), y: inner.y + px(4.0), w: sw, h: sw };
    fill(canvas, pic, swatch, INK);
    fill(canvas, pic, R { x: swatch.x + t, y: swatch.y + t, w: sw - 2 * t, h: sw - 2 * t }, bgra(color));
    let k = px(2.0);
    let text = color.hex();
    let tx = swatch.x + sw + px(8.0);
    let ty = inner.y + (inner.h - 7 * k) / 2;
    for (n, ch) in text.chars().enumerate() {
        let rows = glyph(ch);
        for (row, bits) in rows.iter().enumerate() {
            for col in 0..5 {
                if bits & (0b10000 >> col) != 0 {
                    fill(canvas, pic, R { x: tx + (n as i32 * 6 + col) * k, y: ty + row as i32 * k, w: k, h: k }, INK);
                }
            }
        }
    }
    R { x: bx, y: by, w, h }
}

/// The loupe over a made-up screen, saved as a PNG: checks can't open the real picker.
pub fn shot(path: &std::path::Path, scale: f64) -> Result<(), String> {
    let (w, h) = ((480.0 * scale) as i32, (300.0 * scale) as i32);
    // Bands of the website's colors, with a soft fade down them, for the loupe to enlarge.
    let bands = [Rgb(0xd5, 0x3f, 0x8c), Rgb(0x00, 0x84, 0xb8), Rgb(0xff, 0xf2, 0x70), Rgb(0x3e, 0xcf, 0x9a), Rgb(0xf7, 0xee, 0xf3), Rgb(0x24, 0x12, 0x1b)];
    let px = (0..w * h)
        .flat_map(|i| {
            let (x, y) = (i % w, i / w);
            let band = bands[((x + y / 3) / (24.0 * scale) as i32) as usize % bands.len()];
            bgra(band.mix(Rgb(0, 0, 0), y as f32 / h as f32 * 0.4))
        })
        .collect();
    let pic = Picture { w, h, px };
    let mut canvas = pic.px.clone();
    loupe(&mut canvas, &pic, (145.0 * scale) as i32, (70.0 * scale) as i32, scale, ZOOMS[2]);
    let rgba = canvas.as_chunks::<4>().0.iter().map(|p| eframe::egui::Color32::from_rgb(p[2], p[1], p[0])).collect();
    rivet_kit::save_png(path, &eframe::egui::ColorImage { size: [w as usize, h as usize], source_size: eframe::egui::Vec2::new(w as f32, h as f32), pixels: rgba })
}

/// A 5×7 pixel letter for a color's code, a row a number, the left pixel the high bit.
fn glyph(c: char) -> [u8; 7] {
    match c {
        '#' => [0b01010, 0b01010, 0b11111, 0b01010, 0b11111, 0b01010, 0b01010],
        '0' => [0b01110, 0b10001, 0b10011, 0b10101, 0b11001, 0b10001, 0b01110],
        '1' => [0b00100, 0b01100, 0b00100, 0b00100, 0b00100, 0b00100, 0b01110],
        '2' => [0b01110, 0b10001, 0b00001, 0b00010, 0b00100, 0b01000, 0b11111],
        '3' => [0b11111, 0b00010, 0b00100, 0b00010, 0b00001, 0b10001, 0b01110],
        '4' => [0b00010, 0b00110, 0b01010, 0b10010, 0b11111, 0b00010, 0b00010],
        '5' => [0b11111, 0b10000, 0b11110, 0b00001, 0b00001, 0b10001, 0b01110],
        '6' => [0b00110, 0b01000, 0b10000, 0b11110, 0b10001, 0b10001, 0b01110],
        '7' => [0b11111, 0b00001, 0b00010, 0b00100, 0b01000, 0b01000, 0b01000],
        '8' => [0b01110, 0b10001, 0b10001, 0b01110, 0b10001, 0b10001, 0b01110],
        '9' => [0b01110, 0b10001, 0b10001, 0b01111, 0b00001, 0b00010, 0b01100],
        'a' => [0, 0, 0b01110, 0b00001, 0b01111, 0b10001, 0b01111],
        'b' => [0b10000, 0b10000, 0b10110, 0b11001, 0b10001, 0b10001, 0b11110],
        'c' => [0, 0, 0b01110, 0b10000, 0b10000, 0b10001, 0b01110],
        'd' => [0b00001, 0b00001, 0b01101, 0b10011, 0b10001, 0b10001, 0b01111],
        'e' => [0, 0, 0b01110, 0b10001, 0b11111, 0b10000, 0b01110],
        'f' => [0b00110, 0b01001, 0b01000, 0b11100, 0b01000, 0b01000, 0b01000],
        _ => [0; 7],
    }
}

impl CompositorHandler for Picker {
    fn scale_factor_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: i32) {}
    fn transform_changed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: wl_output::Transform) {}
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, surface: &wl_surface::WlSurface, _: u32) {
        if let Some(i) = self.screens.iter().position(|s| s.layer.wl_surface() == surface) {
            self.screens[i].waiting = false;
            if self.screens[i].dirty {
                self.draw(i, qh);
            }
        }
    }
    fn surface_enter(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
    fn surface_leave(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: &wl_output::WlOutput) {}
}

impl OutputHandler for Picker {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.outputs
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for Picker {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.done = Some(None);
    }
    fn configure(&mut self, _: &Connection, qh: &QueueHandle<Self>, layer: &LayerSurface, cfg: LayerSurfaceConfigure, _: u32) {
        if let Some(i) = self.screens.iter().position(|s| &s.layer == layer) {
            let s = &mut self.screens[i];
            let (w, h) = (cfg.new_size.0.max(1), cfg.new_size.1.max(1));
            s.logical = (w, h);
            // The picture is the screen's own pixels; the compositor fits it to the screen.
            s.viewport.set_destination(w as i32, h as i32);
            s.waiting = false;
            self.draw(i, qh);
        }
    }
}

impl SeatHandler for Picker {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seats
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(&mut self, _: &Connection, qh: &QueueHandle<Self>, seat: wl_seat::WlSeat, cap: Capability) {
        if cap == Capability::Pointer && self.pointer.is_none() {
            let surface = self.compositor.create_surface(qh);
            self.pointer = self.seats.get_pointer_with_theme(qh, &seat, self.shm.wl_shm(), surface, ThemeSpec::default()).ok();
        }
        if cap == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = Some(seat.get_keyboard(qh, ()));
        }
    }
    fn remove_capability(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat, _: Capability) {}
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl PointerHandler for Picker {
    fn pointer_frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_pointer::WlPointer, events: &[PointerEvent]) {
        for e in events {
            let Some(i) = self.screens.iter().position(|s| s.layer.wl_surface() == &e.surface) else { continue };
            match &e.kind {
                PointerEventKind::Enter { .. } => {
                    if let Some(p) = &self.pointer {
                        let _ = p.set_cursor(&self.conn, CursorIcon::Crosshair);
                    }
                    let before = self.at.map(|a| a.0);
                    self.at = Some((i, e.position.0, e.position.1));
                    if let Some(b) = before.filter(|b| *b != i) {
                        self.draw(b, qh);
                    }
                }
                PointerEventKind::Leave { .. } => {
                    if self.at.is_some_and(|a| a.0 == i) {
                        self.at = None;
                    }
                }
                PointerEventKind::Motion { .. } => {
                    self.at = Some((i, e.position.0, e.position.1));
                    self.nudge = (0, 0);
                }
                PointerEventKind::Press { button, .. } => match *button {
                    // BTN_LEFT, BTN_RIGHT
                    0x110 => self.pick_here(),
                    0x111 => self.done = Some(None),
                    _ => {}
                },
                PointerEventKind::Axis { vertical, .. } => {
                    let d = if vertical.discrete != 0 { vertical.discrete as f64 } else { vertical.absolute };
                    if d < 0.0 && self.zoom > 0 {
                        self.zoom -= 1;
                    } else if d > 0.0 && self.zoom + 1 < ZOOMS.len() {
                        self.zoom += 1;
                    }
                }
                _ => {}
            }
            self.draw(i, qh);
        }
    }
}

// The keys are read as the keyboard's own key codes (evdev's), not through a keymap: the few
// the picker answers to are in the same place on every layout, and it leaves libxkbcommon out
// of the program, so the portable build links against nothing newer than its glibc.
impl Dispatch<wl_keyboard::WlKeyboard, ()> for Picker {
    fn event(p: &mut Self, _: &wl_keyboard::WlKeyboard, event: wl_keyboard::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        let wl_keyboard::Event::Key { key, state, .. } = event else { return };
        let down = state == WEnum::Value(wl_keyboard::KeyState::Pressed);
        // KEY_LEFTSHIFT, KEY_RIGHTSHIFT
        if key == 42 || key == 54 {
            p.shift[usize::from(key == 54)] = down;
            return;
        }
        if !down {
            return;
        }
        let step = if p.shift.contains(&true) { 10 } else { 1 };
        match key {
            // KEY_ESC
            1 => p.done = Some(None),
            // KEY_ENTER, KEY_KPENTER, KEY_SPACE
            28 | 96 | 57 => p.pick_here(),
            // KEY_LEFT, KEY_RIGHT, KEY_UP, KEY_DOWN
            105 => p.nudge.0 -= step,
            106 => p.nudge.0 += step,
            103 => p.nudge.1 -= step,
            108 => p.nudge.1 += step,
            _ => return,
        }
        p.redraw_all(qh);
    }
}

impl ShmHandler for Picker {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for Picker {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry
    }
    registry_handlers![OutputState, SeatState];
}

impl Dispatch<WpViewporter, ()> for Picker {
    fn event(_: &mut Self, _: &WpViewporter, _: <WpViewporter as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

impl Dispatch<WpViewport, ()> for Picker {
    fn event(_: &mut Self, _: &WpViewport, _: <WpViewport as wayland_client::Proxy>::Event, _: &(), _: &Connection, _: &QueueHandle<Self>) {}
}

delegate_compositor!(Picker);
delegate_output!(Picker);
delegate_shm!(Picker);
delegate_seat!(Picker);
delegate_pointer!(Picker);
delegate_layer!(Picker);
delegate_registry!(Picker);

#[cfg(test)]
mod tests {
    use super::*;

    fn picture(w: i32, h: i32) -> Picture {
        let px = (0..w * h).flat_map(|i| [(i % 256) as u8, (i / 7 % 256) as u8, 40, 0xff]).collect();
        Picture { w, h, px }
    }

    #[test]
    fn reads_grims_pictures() {
        let mut data = b"P6\n2 1\n255\n".to_vec();
        data.extend([213, 63, 140, 1, 2, 3]);
        assert_eq!(ppm(&data), Some((2, 1, 11)));
        assert_eq!(ppm(&data[..14]), None);
        assert_eq!(ppm(b"P3\n1 1\n255\n1 2 3"), None);
    }

    #[test]
    fn the_loupe_stays_on_screen_and_cleans_up() {
        let pic = picture(400, 300);
        let mut canvas = pic.px.clone();
        // Near the bottom-right corner it flips to the other side of the pointer.
        let r = loupe(&mut canvas, &pic, 390, 290, 1.0, 9);
        assert!(r.x + r.w <= 390 && r.y + r.h <= 290, "{r:?}");
        assert_ne!(canvas, pic.px);
        restore(&mut canvas, &pic, r);
        assert_eq!(canvas, pic.px);
        // Twice the scale, near the top-left: below and right, the whole of it on the screen.
        let big = picture(1200, 900);
        let mut canvas = big.px.clone();
        let r = loupe(&mut canvas, &big, 3, 3, 2.0, 35);
        assert!(r.x > 3 && r.y > 3 && r.x + r.w <= 1200 && r.y + r.h <= 900, "{r:?}");
    }

    #[test]
    fn every_code_letter_has_a_glyph() {
        for c in "#0123456789abcdef".chars() {
            assert_ne!(glyph(c), [0; 7], "{c}");
        }
    }
}
