//! Pictures read on threads of their own and kept as textures, so stepping feels instant: the
//! one shown is asked for first, then those on either side. What is kept is held under a size
//! in bytes (rivet-mem's `ByteCache`), least recently shown out first.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use eframe::egui;
use rivet_kit::mem::ByteCache;
use rivet_media::picture::{self, Orient, Picture};
use rivet_media::stage::TEXTURE_OPTIONS;

/// Bytes of pictures kept ready: the one shown and a few either side, even at 24 megapixels.
const BUDGET: usize = 512 << 20;
/// Threads reading pictures.
const WORKERS: usize = 2;

/// A picture ready to draw.
pub struct Ready {
    pub frames: Vec<(egui::TextureHandle, Duration)>,
    pub width: u32,
    pub height: u32,
    pub format: String,
    pub orient: Orient,
    pub color: Option<String>,
    pub alpha: bool,
    pub vector: bool,
    pub exif: Vec<(String, String)>,
    pub note: Option<String>,
}

impl Ready {
    pub fn animated(&self) -> bool {
        self.frames.len() > 1
    }

    /// The whole animation's length.
    pub fn length(&self) -> Duration {
        self.frames.iter().map(|f| f.1).sum()
    }

    /// The frame at `t` seconds into the animation, and how long until the next.
    pub fn frame_at(&self, t: f64) -> (usize, Duration) {
        let total = self.length().as_secs_f64();
        if self.frames.len() < 2 || total <= 0.0 {
            return (0, Duration::MAX);
        }
        let mut t = t.rem_euclid(total);
        for (i, (_, d)) in self.frames.iter().enumerate() {
            let d = d.as_secs_f64();
            if t < d {
                return (i, Duration::from_secs_f64(d - t));
            }
            t -= d;
        }
        (0, Duration::from_millis(20))
    }
}

type Queue = Arc<(Mutex<VecDeque<(PathBuf, u32)>>, Condvar)>;

pub struct Loader {
    queue: Queue,
    rx: Receiver<(PathBuf, Result<Picture, String>)>,
    /// Asked for and not back yet (waiting or being read).
    asked: HashSet<PathBuf>,
    cache: ByteCache<PathBuf, Arc<Ready>>,
    failed: HashMap<PathBuf, String>,
}

impl Loader {
    pub fn new(ctx: &egui::Context) -> Loader {
        let queue: Queue = Arc::new((Mutex::new(VecDeque::new()), Condvar::new()));
        let (tx, rx): (Sender<(PathBuf, Result<Picture, String>)>, _) = channel();
        for n in 0..WORKERS {
            let (queue, tx, ctx) = (queue.clone(), tx.clone(), ctx.clone());
            let _ = std::thread::Builder::new().name(format!("picture reader {n}")).spawn(move || {
                loop {
                    let (path, max) = {
                        let (lock, cv) = &*queue;
                        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
                        loop {
                            if let Some(job) = q.pop_front() {
                                break job;
                            }
                            q = cv.wait(q).unwrap_or_else(|e| e.into_inner());
                        }
                    };
                    let got = picture::decode(&path, max);
                    if tx.send((path, got)).is_err() {
                        return;
                    }
                    ctx.request_repaint();
                }
            });
        }
        Loader { queue, rx, asked: HashSet::new(), cache: ByteCache::new(BUDGET), failed: HashMap::new() }
    }

    /// The pictures wanted now, the most wanted first. Ones asked for before and no longer
    /// wanted are dropped from the queue (one already being read still finishes).
    pub fn want(&mut self, paths: &[&Path], max_side: u32) {
        let (lock, cv) = &*self.queue;
        let mut q = lock.lock().unwrap_or_else(|e| e.into_inner());
        for (p, _) in q.drain(..) {
            self.asked.remove(&p);
        }
        for p in paths {
            if self.cache.contains(&p.to_path_buf()) || self.failed.contains_key(*p) || self.asked.contains(*p) {
                continue;
            }
            self.asked.insert(p.to_path_buf());
            q.push_back((p.to_path_buf(), max_side));
        }
        cv.notify_all();
    }

    /// Take in what the threads finished: each picture's frames become textures.
    pub fn poll(&mut self, ctx: &egui::Context) {
        while let Ok((path, got)) = self.rx.try_recv() {
            self.asked.remove(&path);
            match got {
                Ok(p) => {
                    let bytes = p.bytes();
                    let name = path.display().to_string();
                    let frames = p.frames.into_iter().enumerate().map(|(i, f)| (ctx.load_texture(format!("view:{name}:{i}"), f.image, TEXTURE_OPTIONS), f.delay)).collect();
                    let ready = Ready { frames, width: p.width, height: p.height, format: p.format, orient: p.orient, color: p.color, alpha: p.alpha, vector: p.vector, exif: p.exif, note: p.note };
                    self.cache.insert(path, Arc::new(ready), bytes);
                }
                Err(e) => {
                    self.failed.insert(path, e);
                }
            }
        }
    }

    pub fn get(&mut self, path: &Path) -> Option<Arc<Ready>> {
        self.cache.get(&path.to_path_buf()).cloned()
    }

    pub fn error(&self, path: &Path) -> Option<&str> {
        self.failed.get(path).map(String::as_str)
    }

    /// Read this picture again next time (it changed).
    pub fn forget(&mut self, path: &Path) {
        self.cache.remove(&path.to_path_buf());
        self.failed.remove(path);
    }
}
