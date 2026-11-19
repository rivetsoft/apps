//! The tray in the panel (apps' icons, in their colours or recoloured to the panel's) and the
//! menu an icon opens (its app's own rows, submenus opened in place).

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::r_sm;

use crate::config::TrayColor;
use crate::tray::{Job, MenuItem, Pixmap, Status, Toggle, TrayItem};
use crate::ui::{Act, Kit, Model, hover_fill, ui_font};

/// Each icon's slot, and the icon's size in it.
pub const SLOT: f32 = 26.0;
pub const ICON: f32 = 18.0;

/// The icons the panel shows.
pub fn shown<'a>(m: &'a Model) -> Vec<&'a TrayItem> {
    m.extra.tray.items.iter().filter(|i| m.s.tray_passive || i.status != Status::Passive).collect()
}

/// An app's own pixels as a texture `px` pixels square. They're resized here, with a sharp
/// filter, rather than stretched when drawn: shrinking 22 or 24 px art to the panel's size on the
/// GPU blurred it. `mask`: white, to be tinted (the icon theme's way, shared).
fn pix_texture(kit: &mut Kit, ctx: &egui::Context, key: &str, stamp: u64, p: &Pixmap, px: u32, mask: bool) -> Option<egui::TextureHandle> {
    let k = (format!("{key}:{}:{px}", p.w), stamp, mask);
    if let Some(t) = kit.tray_tex.get(&k) {
        return Some(t.clone());
    }
    let img = image::RgbaImage::from_raw(p.w as u32, p.h as u32, p.rgba.to_vec())?;
    let img = if img.width() != px || img.height() != px { image::imageops::resize(&img, px, px, image::imageops::FilterType::Lanczos3) } else { img };
    let img = egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], img.as_raw());
    let img = if mask { rivet_look::icons::to_mask(img) } else { img };
    let t = ctx.load_texture(format!("tray:{}:{stamp}:{mask}", k.0), img, egui::TextureOptions::LINEAR);
    if kit.tray_tex.len() > 96 {
        kit.tray_tex.clear();
    }
    kit.tray_tex.insert(k, t.clone());
    Some(t)
}

/// An item's icon (its attention icon while it wants a look): a file of its own, a theme icon,
/// else its pixels. `mask`: white, to be tinted.
pub fn texture(kit: &mut Kit, ctx: &egui::Context, it: &TrayItem, mask: bool) -> Option<egui::TextureHandle> {
    let px = (ICON * ctx.pixels_per_point()).round() as u32;
    let attention = it.status == Status::NeedsAttention;
    let (name, pixmap) = if attention && (!it.attention_name.is_empty() || it.attention_pixmap.is_some()) { (&it.attention_name, &it.attention_pixmap) } else { (&it.icon_name, &it.pixmap) };
    if let Some(f) = &it.icon_file
        && name == &it.icon_name
    {
        let t = if mask { kit.icons.mask_from_path(ctx, f, px) } else { kit.icons.texture_from_path(ctx, f, px) };
        if t.is_some() {
            return t;
        }
    }
    if !name.is_empty() && !name.starts_with('/') {
        let names = [name.as_str()];
        let t = if mask { kit.icons.mask(ctx, &names, px) } else { kit.icons.texture(ctx, &names, px) };
        if t.is_some() {
            return t;
        }
    }
    let p = pixmap.as_ref().or(it.pixmap.as_ref())?;
    pix_texture(kit, ctx, &it.key, it.stamp, p, px, mask)
}

/// The panel's tray in `r`: one slot per icon. Click: the app's window (or its menu, for items
/// that are only a menu); right click: its menu; middle click: its second action; scroll: sent on.
#[allow(clippy::too_many_arguments)]
pub fn draw(ui: &mut egui::Ui, r: Rect, down: bool, idx: usize, m: &Model, tex: &[(String, Option<egui::TextureHandle>)], tint: Color32, accent: Color32, hover_r: f32, popup: &dyn Fn(String, f32) -> Act, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let p = ui.painter().clone();
    let items = shown(m);
    // Along the panel: across, or down a side panel.
    let mut x = if down { r.top() } else { r.left() } + 2.0;
    for it in items {
        let slot = if down { Rect::from_min_size(Pos2::new(r.left(), x), Vec2::new(r.width(), SLOT)) } else { Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(SLOT, r.height())) };
        x += SLOT;
        let resp = ui.interact(slot, egui::Id::unique(("tray", idx, &it.key)), Sense::click());
        let open = m.extra.tray.menu.as_ref().is_some_and(|mm| mm.0 == it.key);
        if resp.hovered() || open {
            p.rect_filled(slot.shrink2(if down { Vec2::new(2.0, 1.0) } else { Vec2::new(1.0, 5.0) }), hover_r, mix(v.panel_fill, v.text_color(), 0.09));
        }
        // On whole pixels, the texture's own size: one texture pixel to one screen pixel (a
        // half-pixel offset, or a stretch from 22.5 to 23 px, would smear it).
        let ppp = ui.ctx().pixels_per_point();
        let side = (ICON * ppp).round() / ppp;
        let ir = Rect::from_min_size(egui::emath::GuiRounding::round_to_pixels(slot.center() - Vec2::splat(side / 2.0), ppp), Vec2::splat(side));
        match tex.iter().find(|t| t.0 == it.key).and_then(|t| t.1.as_ref()) {
            Some(t) => {
                p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
            }
            None => {
                // No icon to be found: its first letter.
                p.circle_filled(ir.center(), ICON / 2.0, mix(v.panel_fill, v.text_color(), 0.18));
                p.text(ir.center(), Align2::CENTER_CENTER, it.name().chars().next().unwrap_or('?').to_uppercase().to_string(), ui_font(11.0), v.text_color());
            }
        }
        if it.status == Status::NeedsAttention {
            let c = ir.right_top() + Vec2::new(0.0, 1.0);
            p.circle_filled(c, 3.5, v.panel_fill);
            p.circle_filled(c, 2.5, accent);
        }
        let scroll = if resp.hovered() { ui.input(|i| i.events.iter().filter_map(|e| if let egui::Event::MouseWheel { delta, .. } = e { Some(delta.y) } else { None }).sum::<f32>()) } else { 0.0 };
        if scroll.abs() > 0.5 {
            acts.push(Act::Tray(Job::Scroll(it.key.clone(), if scroll > 0.0 { -120 } else { 120 })));
        }
        let menu = |acts: &mut Vec<Act>| {
            if open {
                acts.push(Act::Close);
            } else {
                // The popup first: opening it closes any other menu.
                acts.push(popup(it.key.clone(), if down { slot.center().y } else { slot.center().x }));
                acts.push(Act::Tray(Job::OpenMenu(it.key.clone())));
            }
        };
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.secondary_clicked() {
            if it.has_menu {
                menu(acts);
            } else {
                acts.push(Act::Tray(Job::ContextMenu(it.key.clone())));
            }
        } else if resp.middle_clicked() {
            acts.push(Act::Tray(Job::Secondary(it.key.clone())));
        } else if resp.clicked() {
            if it.is_menu && it.has_menu {
                menu(acts);
            } else {
                acts.push(Act::Tray(Job::Activate(it.key.clone())));
            }
        } else if !open {
            resp.on_hover_text(it.tooltip());
        }
    }
}

/// The tint the panel draws icons with (white leaves their own colours).
pub fn tint(color: TrayColor, fg: Color32, highlight: Color32) -> Color32 {
    match color {
        TrayColor::Own => Color32::WHITE,
        TrayColor::Text => fg,
        TrayColor::Highlight => highlight,
    }
}

// ------------------------------------------------------------------ the menu

const ROW: f32 = 30.0;
const SEP: f32 = 9.0;

/// The rows at `path` (submenus' ids, outermost first).
fn rows_at<'a>(rows: &'a [MenuItem], path: &[i32]) -> &'a [MenuItem] {
    let mut now = rows;
    for id in path {
        match now.iter().find(|r| r.id == *id) {
            Some(r) => now = &r.children,
            None => return &[],
        }
    }
    now
}

/// The menu popup's height.
pub fn menu_height(m: &Model, path: &[i32]) -> f32 {
    let rows = match &m.extra.tray.menu {
        Some((_, Some(rows))) => rows_at(rows, path),
        _ => return 56.0,
    };
    let back = if path.is_empty() { 0.0 } else { ROW + SEP };
    let h: f32 = rows.iter().map(|r| if r.separator { SEP } else { ROW }).sum();
    (h + back + 30.0 + 12.0).max(56.0)
}

pub fn menu(ui: &mut egui::Ui, key: &str, path: &mut Vec<i32>, m: &mut Model) {
    let v = ui.visuals().clone();
    ui.spacing_mut().item_spacing.y = 0.0;
    let w = ui.available_width();
    let name = m.extra.tray.items.iter().find(|i| i.key == key).map(|i| i.name().to_string()).unwrap_or_default();
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, 30.0), Sense::hover());
    ui.painter().text(Pos2::new(r.left() + 10.0, r.center().y), Align2::LEFT_CENTER, &name, ui_font(12.5), v.weak_text_color());
    let rows: Vec<MenuItem> = match &m.extra.tray.menu {
        Some((k, Some(rows))) if k == key => rows_at(rows, path).to_vec(),
        Some((k, None)) if k == key => {
            ui.label(egui::RichText::new("  …").color(v.weak_text_color()));
            return;
        }
        _ => {
            ui.label(egui::RichText::new("  No menu").color(v.weak_text_color()));
            return;
        }
    };
    if !path.is_empty() {
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, ROW), Sense::click());
        if resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let c = Pos2::new(r.left() + 16.0, r.center().y);
        let s = Stroke::new(1.6, v.text_color());
        ui.painter().line_segment([c + Vec2::new(3.0, -5.0), c + Vec2::new(-2.0, 0.0)], s);
        ui.painter().line_segment([c + Vec2::new(-2.0, 0.0), c + Vec2::new(3.0, 5.0)], s);
        ui.painter().text(Pos2::new(r.left() + 30.0, r.center().y), Align2::LEFT_CENTER, "Back", ui_font(13.5), v.text_color());
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            path.pop();
        }
        sep(ui, w, &v);
    }
    for row in rows {
        if row.separator {
            sep(ui, w, &v);
            continue;
        }
        let (r, resp) = ui.allocate_exact_size(Vec2::new(w, ROW), if row.enabled { Sense::click() } else { Sense::hover() });
        if row.enabled && resp.hovered() {
            hover_fill(ui.painter(), r, &v);
        }
        let col = if row.enabled { v.text_color() } else { v.weak_text_color().gamma_multiply(0.7) };
        let p = ui.painter();
        // A check or a dot in the left margin.
        let mc = Pos2::new(r.left() + 16.0, r.center().y);
        match row.toggle {
            Toggle::Check => {
                p.rect_stroke(Rect::from_center_size(mc, Vec2::splat(12.0)), r_sm().min(3), Stroke::new(1.2, col), egui::StrokeKind::Inside);
                if row.checked {
                    crate::glyph::check(p, mc, col);
                }
            }
            Toggle::Radio => {
                p.circle_stroke(mc, 6.0, Stroke::new(1.2, col));
                if row.checked {
                    p.circle_filled(mc, 3.0, col);
                }
            }
            Toggle::None => {}
        }
        let label = crate::ui::one_line(p, &row.label, ui_font(13.5), col, r.width() - 50.0);
        p.galley(Pos2::new(r.left() + 30.0, r.center().y - label.size().y / 2.0), label, col);
        let sub = !row.children.is_empty();
        if sub {
            let c = Pos2::new(r.right() - 14.0, r.center().y);
            let s = Stroke::new(1.6, col);
            p.line_segment([c + Vec2::new(-3.0, -5.0), c + Vec2::new(2.0, 0.0)], s);
            p.line_segment([c + Vec2::new(2.0, 0.0), c + Vec2::new(-3.0, 5.0)], s);
        }
        if row.enabled && resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if sub {
                path.push(row.id);
                m.acts.push(Act::Tray(Job::OpenSubmenu(key.to_string(), row.id)));
            } else {
                m.acts.push(Act::Tray(Job::MenuClick(key.to_string(), row.id)));
                m.acts.push(Act::Close);
            }
        }
    }
}

fn sep(ui: &mut egui::Ui, w: f32, v: &egui::Visuals) {
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, SEP), Sense::hover());
    ui.painter().line_segment([Pos2::new(r.left() + 8.0, r.center().y), Pos2::new(r.right() - 8.0, r.center().y)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.12)));
}
