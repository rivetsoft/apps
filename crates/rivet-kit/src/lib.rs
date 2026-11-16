//! Tools shared by the Rivet apps for checks and screenshots.

pub mod headless;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui;

/// `--screenshot FILE.png` for an app: call `step` once per frame. After `after` frames it asks
/// for a picture of the window, saves it as a PNG, and then closes the window.
pub struct Shot {
    pub path: PathBuf,
    pub after: u32,
    frames: u32,
    requested: bool,
    done: Arc<AtomicBool>,
}

impl Shot {
    pub fn new(path: PathBuf) -> Shot {
        Shot { path, after: 120, frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) }
    }

    /// Frames drawn so far.
    pub fn frames(&self) -> u32 {
        self.frames
    }

    pub fn step(&mut self, ctx: &egui::Context) {
        self.frames += 1;
        ctx.request_repaint();
        if self.done.load(Ordering::Acquire) || self.frames > 900 {
            if self.frames > 900 {
                eprintln!("screenshot timed out");
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if self.frames >= self.after && !self.requested {
            self.requested = true;
            let (done, path, ctx2) = (self.done.clone(), self.path.clone(), ctx.clone());
            ctx.request_screenshot(move |img: Arc<egui::ColorImage>| {
                let [w, h] = img.size;
                let buf: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                if let Err(e) = image::save_buffer(&path, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8) {
                    eprintln!("could not save screenshot: {e}");
                }
                done.store(true, Ordering::Release);
                ctx2.request_repaint();
            });
        }
    }
}

/// For checks: `{PREFIX}_RIGHTCLICK=X,Y` (or `{PREFIX}_CLICK=X,Y` for the left button) clicks
/// there once the window has settled (frame 30). Call from `raw_input_hook` in screenshot runs.
pub fn scripted_click(prefix: &str, frame: u32, raw: &mut egui::RawInput) {
    let (var, button) = if std::env::var_os(format!("{prefix}_CLICK")).is_some() { (format!("{prefix}_CLICK"), egui::PointerButton::Primary) } else { (format!("{prefix}_RIGHTCLICK"), egui::PointerButton::Secondary) };
    let Some(pos) = std::env::var(var).ok().and_then(|v| {
        let (a, b) = v.split_once(',')?;
        Some(egui::Pos2::new(a.trim().parse().ok()?, b.trim().parse().ok()?))
    }) else {
        return;
    };
    match frame {
        30 => raw.events.push(egui::Event::PointerMoved(pos)),
        31 => raw.events.push(egui::Event::PointerButton { pos, button, pressed: true, modifiers: Default::default() }),
        32 => raw.events.push(egui::Event::PointerButton { pos, button, pressed: false, modifiers: Default::default() }),
        _ => {}
    }
}
