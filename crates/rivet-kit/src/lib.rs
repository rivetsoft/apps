//! Tools shared by the Rivet apps for checks and screenshots.

pub mod headless;
pub mod scale;
pub mod soft;
pub mod wldrop;

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
        // RIVET_SHOT_AFTER=N takes the picture at frame N instead (to catch something mid-way).
        let after = std::env::var("RIVET_SHOT_AFTER").ok().and_then(|v| v.parse().ok()).unwrap_or(120);
        Shot { path, after, frames: 0, requested: false, done: Arc::new(AtomicBool::new(false)) }
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
///
/// `{PREFIX}_DRAG=X1,Y1,X2,Y2` presses at the first point, moves to the second over ten frames,
/// and keeps holding, so the picture shows the drag in progress.
pub fn scripted_click(prefix: &str, frame: u32, raw: &mut egui::RawInput) {
    if let Some(v) = std::env::var(format!("{prefix}_DRAG")).ok().and_then(|v| v.split(',').map(|n| n.trim().parse::<f32>().ok()).collect::<Option<Vec<f32>>>()).filter(|v| v.len() == 4) {
        let (a, b) = (egui::Pos2::new(v[0], v[1]), egui::Pos2::new(v[2], v[3]));
        match frame {
            30 => raw.events.push(egui::Event::PointerMoved(a)),
            31 => raw.events.push(egui::Event::PointerButton { pos: a, button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }),
            32..=41 => raw.events.push(egui::Event::PointerMoved(a + (b - a) * ((frame - 31) as f32 / 10.0))),
            _ => {}
        }
        return;
    }
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

/// Save a picture held in memory as a PNG file (a picture pasted from the clipboard).
pub fn save_png(path: &std::path::Path, img: &egui::ColorImage) -> Result<(), String> {
    let [w, h] = img.size;
    let buf: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    image::save_buffer(path, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8).map_err(|e| e.to_string())
}

/// A picture held in memory as the bytes of a PNG file (for the clipboard, or to write out).
pub fn png_bytes(img: &egui::ColorImage) -> Result<Vec<u8>, String> {
    let [w, h] = img.size;
    let buf: Vec<u8> = img.pixels.iter().flat_map(|p| p.to_srgba_unmultiplied()).collect();
    let mut out = std::io::Cursor::new(Vec::new());
    image::write_buffer_with_format(&mut out, &buf, w as u32, h as u32, image::ExtendedColorType::Rgba8, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}
