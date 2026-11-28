//! Appearance's palettes: the built-in colour schemes (the ones Rivet Edit and Rivet Terminal
//! colour code with) as the desktop's colours, for when matugen isn't making them from the
//! wallpaper. The chosen one is written as a colour file that every Rivet app reads.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::schemes::{SCHEMES, Scheme};
use rivet_look::theme::r_md;

use super::panel_editor::segmented;
use super::SettingsApp;

const TILE_W: f32 = 148.0;
const TILE_H: f32 = 58.0;
const GAP: f32 = 8.0;

/// Which palettes the list shows.
#[derive(Clone, Copy, PartialEq)]
enum Show {
    All,
    Dark,
    Light,
}

fn font(size: f32) -> egui::FontId {
    rivet_look::fonts::ui_font(size)
}

impl SettingsApp {
    /// The palettes as tiles, under a filter; a click chooses one.
    pub(super) fn palette_tiles(&mut self, ui: &mut egui::Ui) {
        {
            let id = egui::Id::unique("palette-show");
            let mut show = ui.data(|d| d.get_temp::<Show>(id)).unwrap_or(Show::All);
            segmented(ui, &mut show, &[(Show::All, "All"), (Show::Dark, "Dark"), (Show::Light, "Light")]);
            ui.data_mut(|d| d.insert_temp(id, show));
            ui.add_space(10.0);

            let chosen = Scheme::by_name(&self.s.palette).map(|s| s.name);
            let list: Vec<Option<&Scheme>> = SCHEMES.iter().filter(|s| match show {
                Show::All => true,
                Show::Dark => s.dark,
                Show::Light => !s.dark,
            }).map(Some).collect();
            // Whole tiles across the card, sharing what's left over.
            let room = ui.available_width();
            let cols = (((room + GAP) / (TILE_W + GAP)).floor() as usize).max(1);
            let w = (room - GAP * (cols as f32 - 1.0)) / cols as f32;
            let rows = list.len().div_ceil(cols);
            let (area, _) = ui.allocate_exact_size(Vec2::new(room, rows as f32 * (TILE_H + GAP) - GAP), Sense::hover());
            let v = ui.visuals().clone();
            let accent = v.selection.bg_fill;
            let mut pick: Option<&str> = None;
            for (i, sc) in list.iter().enumerate() {
                let r = Rect::from_min_size(area.min + Vec2::new((i % cols) as f32 * (w + GAP), (i / cols) as f32 * (TILE_H + GAP)), Vec2::new(w, TILE_H));
                if !ui.is_rect_visible(r) {
                    continue;
                }
                let name = sc.map(|s| s.name).unwrap_or("None");
                let resp = ui.interact(r, egui::Id::unique(("palette", name)), Sense::click());
                let (bg, fg) = match sc {
                    Some(s) => (s.bg(), s.fg()),
                    None => (v.panel_fill, v.text_color()),
                };
                let on = chosen == sc.map(|s| s.name);
                let p = ui.painter();
                p.rect_filled(r, r_md(), bg);
                let edge = if on { Stroke::new(2.0, accent) } else { Stroke::new(1.0, mix(bg, fg, if resp.hovered() { 0.45 } else { 0.16 })) };
                p.rect_stroke(r.shrink(edge.width / 2.0), r_md(), edge, egui::StrokeKind::Middle);
                let g = crate::ui::one_line(p, name, font(12.5), fg, w - 22.0);
                let cut = g.elided;
                p.galley(r.min + Vec2::new(11.0, 9.0), g, fg);
                match sc {
                    Some(s) => {
                        for (k, c) in s.swatch().into_iter().enumerate() {
                            p.circle_filled(Pos2::new(r.left() + 17.0 + k as f32 * 15.0, r.bottom() - 16.0), 5.0, c);
                        }
                    }
                    None => {
                        p.text(Pos2::new(r.left() + 11.0, r.bottom() - 16.0), egui::Align2::LEFT_CENTER, "The style and accent above", font(11.0), mix(fg, bg, 0.4));
                    }
                }
                if on {
                    let c = Pos2::new(r.right() - 16.0, r.bottom() - 16.0);
                    p.circle_filled(c, 7.5, accent);
                    let ink = if rivet_look::palette::luminance(accent) > 0.6 { Color32::BLACK } else { Color32::WHITE };
                    p.line(vec![c + Vec2::new(-3.2, 0.2), c + Vec2::new(-1.0, 2.4), c + Vec2::new(3.3, -2.4)], Stroke::new(1.6, ink));
                }
                let resp = resp.role(Role::RadioButton, name).selected(on).on_hover_cursor(egui::CursorIcon::PointingHand);
                let resp = if cut { resp.on_hover_text(name) } else { resp };
                if resp.clicked() {
                    pick = Some(sc.map(|s| s.name).unwrap_or(""));
                }
            }
            if let Some(name) = pick {
                self.s.choose_palette(name);
            }
        }
    }
}
