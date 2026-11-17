//! `--screenshot`, drawn in memory.
//!
//! A real window only gets new frames while the compositor is showing it: with the screen off,
//! locked or on another workspace, Wayland (and Xwayland) stop asking for frames and a window-
//! based screenshot never comes. So checks don't use a window at all: the app runs its usual
//! frames here, against a 1100×700 "window" that exists only in memory, and the last frame is
//! drawn with a small software renderer (egui's triangles, textures and premultiplied blending,
//! as its GPU painter does) and saved as a PNG. Nothing can take focus, and nothing waits for
//! the screen.

use std::sync::Arc;
use std::time::{Duration, Instant};

use eframe::egui::{self, Pos2, Rect, Vec2, ViewportCommand, ViewportId};

/// Give up after this long (a check stops a program at 90 s).
const LIMIT: Duration = Duration::from_secs(60);

/// Run the app's frames in memory until it closes itself (after saving its screenshot).
pub fn run(size: [f32; 2], make: impl FnOnce(&eframe::CreationContext) -> Box<dyn eframe::App>) {
    let ctx = egui::Context::default();
    let cc = eframe::CreationContext::_new_kittest(ctx.clone());
    let mut app = make(&cc);
    let mut frame = eframe::Frame::_new_kittest();
    let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0], size[1]));
    let mut textures = crate::soft::Textures::default();
    let mut pending_events: Vec<egui::Event> = Vec::new();
    let start = Instant::now();
    loop {
        let mut raw = egui::RawInput {
            screen_rect: Some(screen),
            time: Some(start.elapsed().as_secs_f64()),
            predicted_dt: 1.0 / 60.0,
            focused: false,
            events: std::mem::take(&mut pending_events),
            ..Default::default()
        };
        if let Some(info) = raw.viewports.get_mut(&ViewportId::ROOT) {
            info.native_pixels_per_point = Some(1.0);
            info.inner_rect = Some(screen);
            info.outer_rect = Some(screen);
            info.focused = Some(false);
        }
        app.raw_input_hook(&ctx, &mut raw);
        let mut out = ctx.run_ui(raw, |ui| {
            let ctx = ui.ctx().clone();
            app.logic(&ctx, &mut frame);
            app.ui(ui, &mut frame);
        });
        // Keep the textures the frame uses (fonts, icons, thumbnails). The update is handled
        // right here, in full; epaint panics if one is dropped unread.
        let mut delta = std::mem::take(&mut out.textures_delta);
        textures.update(&delta);
        delta.clear();
        let mut close = false;
        let commands = out.viewport_output.get(&ViewportId::ROOT).map(|v| v.commands.clone()).unwrap_or_default();
        for cmd in commands {
            match cmd {
                ViewportCommand::Close => close = true,
                ViewportCommand::Screenshot(callback) => {
                    let prims = ctx.tessellate(out.shapes.clone(), out.pixels_per_point);
                    let clear = app.clear_color(&ctx.global_style().visuals);
                    let image = crate::soft::render(&prims, &textures, [size[0] as usize, size[1] as usize], clear);
                    callback.complete(Arc::new(image));
                }
                _ => {}
            }
        }
        if close || start.elapsed() > LIMIT {
            if !close {
                eprintln!("screenshot timed out");
            }
            return;
        }
        // About 60 frames a second, so background work (icons, thumbnails) has time to land.
        std::thread::sleep(Duration::from_millis(12));
        pending_events.clear();
    }
}
