//! A small picture of the desktop with notification cards on it, drawn by the shell's own card
//! code, so the Notifications page shows the corner, width and look as they'll be.

use eframe::egui::{self, Id, LayerId, Pos2, Rect, Sense, Vec2};

use super::SettingsApp;
use super::panel_editor::backdrop;
use crate::config::{Edge, NotePlace};

/// The screen the preview draws, in points, before it's shrunk to fit the page.
const SCREEN: Vec2 = Vec2::new(1000.0, 380.0);

impl SettingsApp {
    pub(super) fn notes_preview(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        super::controls::preview_head(ui);
        let w = ui.available_width();
        let scale = w / SCREEN.x;
        let (r, _) = ui.allocate_exact_size(SCREEN * scale, Sense::hover());
        super::controls::preview_edge(ui, r);
        backdrop(ui, r);
        let kit = self.preview_kit.get_or_insert_with(|| crate::ui::Kit::new(self.shot.is_none()));
        kit.begin(&ctx, &self.s);
        let status = crate::shot::demo_status();
        let notes = crate::shot::demo_notes();
        let cards: Vec<_> = notes.iter().take(self.s.note_max.min(2)).cloned().collect();
        let extra = crate::ui::Extra { notes: notes.clone(), cards: cards.clone(), notifying: true, user: String::new(), ..Default::default() };
        let mut acts = Vec::new();
        let s = self.s.clone();
        let mut m = crate::ui::Model { s: &s, status: &status, extra: &extra, apps: &self.apps, compositor: crate::sys::Compositor::Niri, output: "", acts: &mut acts };
        // Everything on one layer in the screen's own points, scaled down onto the preview.
        let layer = LayerId::new(ui.layer_id().order, Id::unique("shell-notes-preview"));
        let full = Rect::from_min_size(Pos2::ZERO, SCREEN);
        let shown = r.intersect(ui.clip_rect());
        let clip = Rect::from_min_max(((shown.min - r.min) / scale).to_pos2(), ((shown.max - r.min) / scale).to_pos2()).intersect(full.shrink(2.0 / scale));
        let mut at = |rect: Rect, f: &mut dyn FnMut(&mut egui::Ui, &mut crate::ui::Model, &mut crate::ui::Kit)| {
            let mut child = ui.new_child(egui::UiBuilder::new().layer_id(layer).max_rect(rect));
            child.set_clip_rect(clip);
            f(&mut child, &mut m, kit);
        };
        // The first panel, when it runs along the top or bottom: "By the Panel" follows it.
        let panel = s.panels.first().filter(|p| !p.vertical());
        if let Some(p) = panel {
            let h = p.height();
            let reach = crate::panels::reach(&s.panels, 0, "");
            let pr = if p.edge == Edge::Bottom { Rect::from_min_size(Pos2::new(0.0, SCREEN.y - reach), Vec2::new(SCREEN.x, h)) } else { Rect::from_min_size(Pos2::new(0.0, reach - h), Vec2::new(SCREEN.x, h)) };
            at(pr, &mut |ui, m, k| crate::bar::panel(ui, m, k, 0));
        }
        let room = |edge: Edge| panel.filter(|p| p.edge == edge).map_or(0.0, |_| crate::panels::reach(&s.panels, 0, ""));
        let (cw, ch) = (crate::cc::cards_width(&s), crate::cc::cards_height(&cards));
        let (top, left) = match s.note_place {
            NotePlace::ByPanel => (panel.is_none_or(|p| p.edge == Edge::Top), false),
            NotePlace::TopLeft => (true, true),
            NotePlace::TopRight => (true, false),
            NotePlace::BottomLeft => (false, true),
            NotePlace::BottomRight => (false, false),
        };
        let y = if top { room(Edge::Top) + 2.0 } else { SCREEN.y - room(Edge::Bottom) - 2.0 - ch };
        let x = if left { 4.0 } else { SCREEN.x - 4.0 - cw };
        at(Rect::from_min_size(Pos2::new(x, y), Vec2::new(cw, ch)), &mut |ui, m, k| crate::cc::cards(ui, m, k));
        ctx.set_transform_layer(layer, egui::emath::TSTransform::new(r.min.to_vec2(), scale));
        ctx.set_sublayer(ui.layer_id(), layer);
    }
}
