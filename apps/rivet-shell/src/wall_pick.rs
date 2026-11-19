//! The wallpaper chooser: a folder (typed, or one of the recent and usual ones), and its
//! pictures as a grid of thumbnails. It holds no settings: it's told the folder and the picture
//! showing, and says what was asked for, so Settings and a panel item can both use it.

use std::path::{Path, PathBuf};

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::theme::r_md;

use crate::portal::Pick;
use crate::wall::{self, Gallery, Picture};

#[derive(Default)]
pub struct Picker {
    gallery: Gallery,
    filter: String,
    dir_text: String,
    /// The picture last asked about by `thumb_of` (its size and time, read once).
    one: Option<Picture>,
}

/// What a frame of the chooser asked for.
#[derive(Default)]
pub struct Out {
    /// A picture was chosen.
    pub picture: Option<PathBuf>,
    /// Another folder was chosen.
    pub dir: Option<PathBuf>,
    /// The desktop's picker was asked for (the caller opens it).
    pub ask: Option<Pick>,
}

/// A texture drawn to cover `r`, cropping its longer side.
pub fn cover(p: &egui::Painter, r: Rect, t: &egui::TextureHandle, radius: impl Into<egui::CornerRadius>) {
    let ts = t.size_vec2();
    let (ax, ay) = (r.width() / r.height(), ts.x / ts.y);
    let uv = if ay > ax {
        let f = ax / ay;
        Rect::from_min_max(Pos2::new((1.0 - f) / 2.0, 0.0), Pos2::new((1.0 + f) / 2.0, 1.0))
    } else {
        let f = ay / ax;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - f) / 2.0), Pos2::new(1.0, (1.0 + f) / 2.0))
    };
    p.add(egui::epaint::RectShape::filled(r, radius, Color32::WHITE).with_texture(t.id(), uv));
}

impl Picker {
    /// Keeps the folder's list and the thumbnails going. Call every frame, shown or not.
    pub fn frame(&mut self, ctx: &egui::Context, dir: &Path) {
        self.gallery.frame(ctx, dir);
    }

    /// A picture's thumbnail (any picture, in the folder or not), once it's ready.
    pub fn thumb_of(&mut self, path: &Path) -> Option<egui::TextureHandle> {
        if self.one.as_ref().is_none_or(|p| p.path != path) {
            let m = std::fs::metadata(path).ok()?;
            self.one = Some(Picture { path: path.to_path_buf(), mtime: m.modified().ok(), size: m.len() });
        }
        let p = self.one.clone()?;
        self.gallery.thumb(&p)
    }

    /// The chooser: the folder's row, what's in it, and the pictures (in a scrolling area at
    /// most `max_h` tall). `now` is the picture showing; `can_ask`: the desktop's picker is free.
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut egui::Ui, dir: &Path, recent: &[String], now: &str, accent: Color32, can_ask: bool, max_h: f32) -> Out {
        let mut out = Out::default();
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.label("Folder");
            // The buttons from the right; the field takes what's left.
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add_enabled(dir.is_dir(), egui::Button::new("Open")).on_hover_text("Open the folder in the file manager").clicked() {
                    let _ = std::process::Command::new("xdg-open").arg(dir).stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
                }
                if ui.add_enabled(can_ask, egui::Button::new("Choose…")).on_hover_text("Pick the folder with the file picker").clicked() {
                    out.ask = Some(Pick::Folder);
                }
                let id = egui::Id::unique("shell-wall-dir");
                if !ui.memory(|m| m.has_focus(id)) {
                    self.dir_text = wall::short(dir);
                }
                let r = ui.add(egui::TextEdit::singleline(&mut self.dir_text).id(id).desired_width(ui.available_width()).hint_text("~/Pictures/Wallpapers"));
                // Taken when typing stops (the line under it says when there's no such folder).
                if r.lost_focus() && !self.dir_text.trim().is_empty() {
                    out.dir = Some(wall::expand(&self.dir_text));
                }
            });
        });
        // Other folders, a click away: the recent ones, then the usual places.
        let recent: Vec<PathBuf> = recent.iter().map(PathBuf::from).filter(|d| d.is_dir() && d != dir).collect();
        let usual: Vec<PathBuf> = wall::suggested_dirs().into_iter().filter(|d| d != dir && !recent.contains(d)).collect();
        if !recent.is_empty() || !usual.is_empty() {
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for (d, tip) in recent.iter().map(|d| (d, "A folder used before")).chain(usual.iter().map(|d| (d, "A usual place for wallpapers"))).take(6) {
                    if ui.add(egui::Button::new(egui::RichText::new(wall::short(d)).small())).on_hover_text(tip).clicked() {
                        out.dir = Some(d.clone());
                    }
                }
            });
        }
        ui.add_space(6.0);
        // What's in the folder.
        let n = self.gallery.pictures().len();
        let status = if !dir.is_dir() {
            "This folder doesn't exist.".to_string()
        } else if self.gallery.loading() {
            "Looking for pictures…".to_string()
        } else if n == 0 {
            "No JPEG, PNG or WebP pictures here or in its subfolders one level down.".to_string()
        } else {
            let folders = self.gallery.pictures().iter().filter_map(|p| p.path.parent()).filter(|d| *d != dir).collect::<std::collections::HashSet<_>>().len();
            let inside = match folders {
                0 => String::new(),
                1 => ", some in a folder inside".into(),
                k => format!(", some in {k} folders inside"),
            };
            format!("{n} picture{}{inside}", if n == 1 { "" } else { "s" })
        };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(status).small().color(weak));
            if n > 12 {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.filter).hint_text("Find by name…").desired_width(180.0));
                });
            }
        });
        ui.add_space(8.0);
        egui::ScrollArea::vertical().id_salt("shell-wall-grid").max_height(max_h).min_scrolled_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
            if let Some(p) = self.grid(ui, now, accent) {
                out.picture = Some(p);
            }
        });
        if out.dir.as_deref() == Some(dir) {
            out.dir = None;
        }
        if out.dir.is_some() {
            self.filter.clear();
        }
        out
    }

    fn grid(&mut self, ui: &mut egui::Ui, now: &str, accent: Color32) -> Option<PathBuf> {
        let filter = self.filter.trim().to_lowercase();
        let shown: Vec<Picture> = self.gallery.pictures().iter().filter(|p| filter.is_empty() || p.name().to_lowercase().contains(&filter)).cloned().collect();
        if shown.is_empty() {
            return None;
        }
        let w = ui.available_width() - 8.0;
        let cols = ((w + 10.0) / 170.0).floor().max(1.0) as usize;
        let tw = (w - (cols as f32 - 1.0) * 10.0) / cols as f32;
        let th = tw * 0.62;
        let mut pick = None;
        let faint = ui.visuals().faint_bg_color;
        let hover = ui.visuals().text_color().gamma_multiply(0.5);
        let shimmer = ui.visuals().extreme_bg_color;
        egui::Grid::new("shell-walls").spacing([10.0, 10.0]).show(ui, |ui| {
            for (k, p) in shown.iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(tw, th), Sense::click());
                // Thumbnails only for what's on screen.
                let visible = ui.is_rect_visible(r);
                match if visible { self.gallery.thumb(p) } else { None } {
                    Some(t) => cover(ui.painter(), r, &t, r_md()),
                    None => {
                        ui.painter().rect_filled(r, r_md(), if visible { shimmer } else { faint });
                    }
                }
                if now == p.path.to_string_lossy() {
                    ui.painter().rect_stroke(r, r_md(), Stroke::new(3.0, accent), egui::StrokeKind::Inside);
                } else if resp.hovered() {
                    ui.painter().rect_stroke(r, r_md(), Stroke::new(2.0, hover), egui::StrokeKind::Inside);
                }
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(p.name()).clicked() {
                    pick = Some(p.path.clone());
                }
                if (k + 1) % cols == 0 {
                    ui.end_row();
                }
            }
        });
        pick
    }
}
