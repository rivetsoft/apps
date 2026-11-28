//! The dock's places: the Downloads list (the newest files, to open or drag somewhere) and
//! the Trash's menu.

use rivet_look::a11y::{Access, Role};
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Vec2};
use rivet_xdg::mime::MimeDb;

use crate::ui::{Act, Kit, Model, hover_fill, one_line, ui_font};

/// How many of the newest downloads the list shows.
const SHOWN: usize = 8;
const ROW: f32 = 40.0;
const HEAD: f32 = 30.0;
const FOOT: f32 = 44.0;
pub const WIDTH: f32 = 330.0;

/// A file in Downloads: where it is, its type, and when it last changed.
#[derive(Clone)]
pub struct Download {
    pub path: PathBuf,
    pub mime: String,
    pub when: SystemTime,
}

static MIME: LazyLock<MimeDb> = LazyLock::new(MimeDb::load);

/// A file's type, from its name (or what it is, for a folder).
pub fn mime_of(path: &std::path::Path) -> String {
    MIME.guess(path, path.is_dir(), false)
}

/// The Downloads folder, if this account has one.
pub fn folder() -> Option<PathBuf> {
    rivet_xdg::places::user_dirs().download.filter(|d| d.is_dir())
}

/// The newest downloads, newest first. Read again at most every two seconds, since the list
/// is drawn at every change of the pointer over it.
pub fn newest() -> Vec<Download> {
    if checking() {
        return demo();
    }
    /// The list, and when it was read.
    type Read = Option<(Instant, Vec<Download>)>;
    static CACHE: LazyLock<Mutex<Read>> = LazyLock::new(Mutex::default);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, list)) = c.as_ref()
        && at.elapsed() < Duration::from_secs(2)
    {
        return list.clone();
    }
    let mut list: Vec<Download> = folder()
        .and_then(|d| std::fs::read_dir(d).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        // Not ones still coming in (browsers' part files).
        .filter(|e| !matches!(e.path().extension().and_then(|x| x.to_str()), Some("part" | "crdownload" | "download")))
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            let path = e.path();
            let mime = MIME.guess(&path, meta.is_dir(), false);
            Some(Download { path, mime, when: meta.modified().ok()? })
        })
        .collect();
    list.sort_by_key(|d| std::cmp::Reverse(d.when));
    list.truncate(SHOWN);
    *c = Some((Instant::now(), list.clone()));
    list
}

/// Screenshots and checks: made-up files, so a picture never shows what this account has.
fn checking() -> bool {
    crate::config::NO_SAVE.load(std::sync::atomic::Ordering::Relaxed)
}

fn demo() -> Vec<Download> {
    let now = SystemTime::now();
    [("sunset-over-the-bay.jpg", 120), ("invoice-0412.pdf", 3600 * 2), ("rivet-0.9.AppImage", 86400 * 3), ("notes from the meeting.md", 86400 * 9)]
        .iter()
        .map(|(name, ago)| {
            let path = PathBuf::from("/home/rivet/Downloads").join(name);
            Download { mime: MIME.guess_by_name(&path).unwrap_or_else(|| "application/octet-stream".into()), path, when: now - Duration::from_secs(*ago) }
        })
        .collect()
}

/// Whether anything is in the Trash (in checks, as if there were).
pub fn trash_full() -> bool {
    checking() || !rivet_xdg::trash::is_empty()
}

/// The list's height for this many files.
pub fn height(n: usize) -> f32 {
    HEAD + if n == 0 { 56.0 } else { n as f32 * ROW } + FOOT
}

/// How long ago, shortly: "now", "5 min", "3 h", "Mon", "12 Oct".
fn ago(when: SystemTime) -> String {
    let secs = SystemTime::now().duration_since(when).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..60 => "now".into(),
        60..3600 => format!("{} min", secs / 60),
        3600..86400 => format!("{} h", secs / 3600),
        _ => {
            let t: chrono::DateTime<chrono::Local> = when.into();
            if secs < 6 * 86400 { t.format("%a").to_string() } else { t.format("%-d %b").to_string() }
        }
    }
}

/// A file type's icon, from the first of its names the theme has.
fn type_icon(kit: &mut Kit, ctx: &egui::Context, mime: &str, px: f32) -> Option<egui::TextureHandle> {
    MIME.icon_names(mime).iter().find_map(|n| kit.named_icon(ctx, n, px)).or_else(|| kit.named_icon(ctx, "text-x-generic", px))
}

pub fn downloads(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let v = ui.visuals().clone();
    let list = newest();
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEAD), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "Downloads", ui_font(12.5), v.weak_text_color());
    if list.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 56.0), Sense::hover());
        let text = if folder().is_some() { "Nothing downloaded yet" } else { "This account has no Downloads folder" };
        ui.painter().text(r.center(), Align2::CENTER_CENTER, text, ui_font(13.0), v.weak_text_color());
    }
    for d in &list {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), ROW), Sense::click_and_drag());
        let resp = resp.role(Role::ListBoxOption, d.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default());
        if resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let ir = Rect::from_center_size(Pos2::new(r.left() + 22.0, r.center().y), Vec2::splat(26.0));
        if let Some(t) = type_icon(kit, ui.ctx(), &d.mime, 26.0) {
            ui.painter().image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let when = ui.painter().layout_no_wrap(ago(d.when), ui_font(11.5), v.weak_text_color());
        let name = d.path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let g = one_line(ui.painter(), &name, ui_font(13.5), v.text_color(), r.right() - ir.right() - 30.0 - when.size().x);
        ui.painter().galley(Pos2::new(ir.right() + 10.0, r.center().y - g.size().y / 2.0), g, v.text_color());
        ui.painter().galley(Pos2::new(r.right() - 10.0 - when.size().x, r.center().y - when.size().y / 2.0), when, v.weak_text_color());
        // Dragged: the file goes wherever it's dropped (a window, a folder), as from a file manager.
        if resp.drag_started() {
            m.acts.push(Act::DragOut(d.path.clone()));
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::Close);
            m.acts.push(Act::OpenFile(d.path.clone(), d.mime.clone()));
        }
    }
    ui.add_space(6.0);
    if let Some(dir) = folder() {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), FOOT - 10.0), Sense::click());
        let resp = resp.role(Role::Button, "Open Downloads");
        if resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, "Open Downloads", ui_font(13.5), v.text_color());
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            m.acts.push(Act::Close);
            m.acts.push(Act::OpenFile(dir, "inode/directory".into()));
        }
    }
}

/// The Trash's menu's height: its two items, or the question before emptying it.
pub fn trash_height(confirm: bool) -> f32 {
    if confirm { 108.0 } else { 30.0 + 34.0 * 2.0 + 12.0 }
}

/// What's in the Trash, counted: the files and folders at its top.
fn trash_count() -> usize {
    if checking() {
        return 3;
    }
    std::fs::read_dir(rivet_xdg::places::trash_dir().join("files")).map(|r| r.count()).unwrap_or(0)
}

pub fn trash_menu(ui: &mut egui::Ui, confirm: &mut bool, m: &mut Model) {
    ui.spacing_mut().item_spacing.y = 0.0;
    let v = ui.visuals().clone();
    let n = trash_count();
    let row = |ui: &mut egui::Ui, label: &str, enabled: bool| {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), if enabled { Sense::click() } else { Sense::hover() });
        if enabled && resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let col = if enabled { v.text_color() } else { v.weak_text_color().gamma_multiply(0.6) };
        ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, label, ui_font(13.5), col);
        enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
    };
    if *confirm {
        // Emptying can't be undone: say so, and name the action on its button.
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Empty the Trash?").font(ui_font(14.5)).color(v.strong_text_color()));
        ui.add_space(4.0);
        let what = if n == 1 { "The 1 item in it is deleted for good.".to_string() } else { format!("The {n} items in it are deleted for good.") };
        ui.label(egui::RichText::new(what).font(ui_font(12.5)).color(v.weak_text_color()));
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button("Keep Them").clicked() {
                *confirm = false;
            }
            let red = rivet_look::theme::status(ui.ctx()).danger;
            if ui.add(egui::Button::new(egui::RichText::new("Empty Trash").color(Color32::WHITE)).fill(red)).clicked() {
                m.acts.push(Act::Close);
                m.acts.push(Act::EmptyTrash);
            }
        });
        return;
    }
    let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
    let head = match n {
        0 => "Trash is empty".to_string(),
        1 => "Trash · 1 item".to_string(),
        n => format!("Trash · {n} items"),
    };
    ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, head, ui_font(12.5), v.weak_text_color());
    if row(ui, "Open", true) {
        m.acts.push(Act::Close);
        m.acts.push(Act::OpenTrash);
    }
    if row(ui, "Empty Trash…", n > 0) {
        *confirm = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ages_read_shortly() {
        let now = SystemTime::now();
        assert_eq!(ago(now), "now");
        assert_eq!(ago(now - Duration::from_secs(300)), "5 min");
        assert_eq!(ago(now - Duration::from_secs(3 * 3600 + 5)), "3 h");
    }
}
