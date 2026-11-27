//! The frames shown over the seek bar: fetched on a thread with rivet-media's frame grab
//! (ffmpeg), one per stretch of the file, kept to a size in bytes. Only the latest request is
//! fetched, so moving along the bar never builds a backlog.

use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};

use eframe::egui;
use rivet_kit::mem::ByteCache;

/// How wide each frame is fetched, in pixels.
pub const WIDTH: u32 = 240;

type Key = (PathBuf, u32);
type Wanted = Arc<(Mutex<Option<(Key, f64)>>, Condvar)>;

pub struct Previews {
    want: Wanted,
    rx: Receiver<(Key, Option<egui::ColorImage>)>,
    tx: Sender<(Key, Option<egui::ColorImage>)>,
    cache: ByteCache<Key, Option<egui::TextureHandle>>,
    started: bool,
    /// Whether ffmpeg is installed, looked up once.
    ffmpeg: Option<bool>,
}

/// Which stretch of the file a moment falls in: a hundredth of it, at least two seconds long.
pub fn bucket(secs: f64, length: f64) -> (u32, f64) {
    let step = (length / 100.0).max(2.0);
    let b = (secs.max(0.0) / step).floor();
    (b as u32, (b + 0.5) * step)
}

impl Default for Previews {
    fn default() -> Self {
        let (tx, rx) = channel();
        Previews { want: Arc::default(), rx, tx, cache: ByteCache::new(16 << 20), started: false, ffmpeg: None }
    }
}

impl Previews {
    /// The frame near `secs`, if it has been fetched; else ask for it (dropping older asks).
    pub fn get(&mut self, ctx: &egui::Context, path: &Path, secs: f64, length: f64) -> Option<egui::TextureHandle> {
        if !*self.ffmpeg.get_or_insert_with(|| rivet_media::preview::has("ffmpeg")) {
            return None;
        }
        self.poll(ctx);
        let (b, at) = bucket(secs, length);
        let key = (path.to_path_buf(), b);
        if let Some(found) = self.cache.get(&key) {
            return found.clone();
        }
        self.start(ctx);
        let (lock, wake) = &*self.want;
        if let Ok(mut w) = lock.lock() {
            *w = Some((key, at));
            wake.notify_one();
        }
        None
    }

    fn start(&mut self, ctx: &egui::Context) {
        if self.started {
            return;
        }
        self.started = true;
        let (want, tx, ctx) = (self.want.clone(), self.tx.clone(), ctx.clone());
        let _ = std::thread::Builder::new().name("seek previews".into()).spawn(move || {
            loop {
                let (key, at) = {
                    let (lock, wake) = &*want;
                    let Ok(mut w) = lock.lock() else { return };
                    while w.is_none() {
                        let Ok(next) = wake.wait(w) else { return };
                        w = next;
                    }
                    let Some(job) = w.take() else { continue };
                    job
                };
                let img = rivet_media::preview::frame_at(&key.0, at, WIDTH);
                if tx.send((key, img)).is_err() {
                    return;
                }
                ctx.request_repaint();
            }
        });
    }

    fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((key, img)) = self.rx.try_recv() {
            let bytes = img.as_ref().map_or(64, |i| i.pixels.len() * 4);
            let tex = img.map(|i| ctx.load_texture(format!("seek-{}-{}", key.0.display(), key.1), i, egui::TextureOptions::LINEAR));
            self.cache.insert(key, tex, bytes);
        }
    }

    /// Put a frame in by hand (screenshots).
    pub fn insert(&mut self, path: &Path, secs: f64, length: f64, tex: egui::TextureHandle) {
        let (b, _) = bucket(secs, length);
        let [w, h] = tex.size();
        self.cache.insert((path.to_path_buf(), b), Some(tex), w * h * 4);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets() {
        assert_eq!(bucket(0.0, 60.0), (0, 1.0));
        assert_eq!(bucket(5.0, 60.0), (2, 5.0));
        assert_eq!(bucket(1800.0, 3600.0), (50, 1818.0));
    }
}
