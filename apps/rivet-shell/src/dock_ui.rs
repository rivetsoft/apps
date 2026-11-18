//! The dock as a panel item: the same pinned apps (and open ones) as the dock, smaller, in a
//! row in the panel. Pinning, unpinning and moving from either one changes both.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_xdg::launch::DesktopApp;

use crate::ui::{Act, Kit, Model, PopupKind, dock_items, is_running, ui_font};

/// Each app's slot, the icon's size in it, and the room between the pinned and open apps.
pub const SLOT: f32 = 30.0;
pub const ICON: f32 = 20.0;
const SEP: f32 = 9.0;

/// The apps, whether each is pinned, and its icon (made before drawing, with the surface's kit).
pub type Icons = Vec<(DesktopApp, bool, Option<egui::TextureHandle>)>;

pub fn icons(kit: &mut Kit, ctx: &egui::Context, m: &Model) -> Icons {
    dock_items(m.s, m.apps, &m.status.running).into_iter().map(|(a, pinned)| {
        let t = kit.app_icon(ctx, &a, ICON);
        (a, pinned, t)
    }).collect()
}

fn split(items: &Icons) -> bool {
    items.iter().any(|i| i.1) && items.iter().any(|i| !i.1)
}

/// How wide it is (None: nothing pinned or open).
pub fn width(items: &Icons) -> Option<f32> {
    if items.is_empty() {
        return None;
    }
    Some(items.len() as f32 * SLOT + if split(items) { SEP } else { 0.0 } + 4.0)
}

/// Draw it in `r`. `popup` makes an app's menu, opening under the panel at `x`.
pub fn draw(ui: &mut egui::Ui, r: Rect, idx: usize, m: &Model, items: &Icons, accent: Color32, hover_r: f32, popup: &dyn Fn(PopupKind, f32) -> Act, acts: &mut Vec<Act>) {
    let v = ui.visuals().clone();
    let p = ui.painter().clone();
    let mut x = r.left() + 2.0;
    let mut after_pinned = false;
    for (a, pinned, tex) in items {
        if !pinned && !after_pinned {
            after_pinned = true;
            if split(items) {
                let sx = x + SEP / 2.0;
                p.line_segment([Pos2::new(sx, r.top() + 7.0), Pos2::new(sx, r.bottom() - 7.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.2)));
                x += SEP;
            }
        }
        let slot = Rect::from_min_size(Pos2::new(x, r.top()), Vec2::new(SLOT, r.height()));
        x += SLOT;
        let app = a.id.trim_end_matches(".desktop").to_string();
        let resp = ui.interact(slot, egui::Id::unique(("panel-dock", idx, &a.id)), Sense::click());
        if resp.hovered() {
            p.rect_filled(slot.shrink2(Vec2::new(1.0, 2.0)), hover_r, mix(v.panel_fill, v.text_color(), 0.09));
        }
        let ir = Rect::from_center_size(slot.center() - Vec2::new(0.0, 1.0), Vec2::splat(ICON));
        match tex {
            Some(t) => {
                p.image(t.id(), ir, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
            }
            None => {
                p.rect_filled(ir.shrink(1.0), 5.0, mix(accent, v.panel_fill, 0.3));
                p.text(ir.center(), Align2::CENTER_CENTER, a.name.chars().next().unwrap_or('?').to_uppercase().to_string(), ui_font(11.0), Color32::WHITE);
            }
        }
        // Open: a dot under it.
        if m.s.dock_running && (!pinned || is_running(a, &m.status.running)) {
            p.circle_filled(Pos2::new(slot.center().x, r.bottom() - 1.5), 1.8, accent);
        }
        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(&a.name);
        if resp.secondary_clicked() {
            acts.push(popup(PopupKind::DockMenu { app, pinned: *pinned }, slot.center().x));
        } else if resp.clicked() {
            acts.push(Act::Launch(a.id.clone()));
        }
    }
}
