//! Split view (F3): two folders side by side.
//!
//! The active pane is the current tab (`tabs[active]`), so the toolbar, path bar, keyboard,
//! menus and Peek all work on it as usual. The other pane's folder is kept in `split`; it is
//! swapped in only while it is drawn. Clicking a pane (or dropping files on it) makes it the
//! active one. `split_right` says which side the active pane is on.

use eframe::egui::{self, Pos2, Rect, Sense, Stroke, Vec2};

use super::RivetApp;
use crate::model::{Location, Tab};

/// Space between the panes.
const GAP: f32 = 6.0;
const HEADER_H: f32 = 28.0;

impl RivetApp {
    pub(super) fn toggle_split(&mut self) {
        if self.split.take().is_none() {
            // The other pane starts in the same folder (like Nemo and Dolphin).
            let mut t = Tab::new(self.tab().loc.clone());
            t.reload(&self.mime, &self.s);
            self.split = Some(t);
            self.split_right = false;
        }
        self.pane_rects = [None, None];
    }

    /// Make the pane on this side (false = left) the active one.
    pub(super) fn focus_pane(&mut self, right: bool) {
        if right == self.split_right {
            return;
        }
        if let Some(other) = self.split.as_mut() {
            std::mem::swap(&mut self.tabs[self.active], other);
            self.split_right = right;
            self.search_open = false;
            self.path_editing = false;
        }
    }

    /// The other pane's folder (split view only).
    pub(super) fn other_pane_dir(&self) -> Option<std::path::PathBuf> {
        self.split.as_ref().and_then(|t| t.loc.dir().map(|d| d.to_path_buf()))
    }

    /// A press (or a drop) in a pane makes it the active one, before anything reacts to it.
    pub(super) fn split_follow_pointer(&mut self, ctx: &egui::Context) {
        if self.split.is_none() {
            return;
        }
        let (pressed, released, pos) = ctx.input(|i| (i.pointer.any_pressed(), i.pointer.any_released(), i.pointer.interact_pos()));
        let dragging_files = egui::DragAndDrop::has_payload_of_type::<super::DragFiles>(ctx);
        if !(pressed || (released && dragging_files)) {
            return;
        }
        let Some(p) = pos else { return };
        if self.pane_rects[1].is_some_and(|r| r.contains(p)) {
            self.focus_pane(true);
        } else if self.pane_rects[0].is_some_and(|r| r.contains(p)) {
            self.focus_pane(false);
        }
    }

    /// Both panes, side by side, each on its own card with a header naming its folder.
    pub(super) fn split_panes(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, frame: egui::Frame, term: bool) -> usize {
        let full = ui.available_rect_before_wrap();
        let half = (full.width() - GAP) / 2.0;
        let rects = [
            Rect::from_min_size(full.min, Vec2::new(half, full.height())),
            Rect::from_min_size(Pos2::new(full.min.x + half + GAP, full.min.y), Vec2::new(half, full.height())),
        ];
        let mut cols = 1;
        let corner = if term { egui::CornerRadius::ZERO } else { egui::CornerRadius::same(super::r_lg()) };
        for (side, rect) in rects.into_iter().enumerate() {
            let right = side == 1;
            let active = right == self.split_right;
            // The inactive pane's folder is swapped in while it's drawn.
            if !active
                && let Some(other) = self.split.as_mut()
            {
                std::mem::swap(&mut self.tabs[self.active], other);
            }
            self.pane_salt = side as u8;
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect).id_salt(("pane", side)));
            let shown = frame.corner_radius(corner).show(&mut child, |ui| {
                ui.set_min_size(ui.available_size());
                self.pane_header(ui, active);
                let c = self.pane_contents(ui, ctx, active);
                if active {
                    cols = c;
                }
            });
            let card = shown.response.rect;
            self.pane_rects[side] = Some(card);
            if term {
                super::tui_title(ui, card, &self.pane_title());
            }
            if active {
                self.files_rect = Some(card);
                // The active pane's edge, in the accent.
                if !term {
                    let accent = super::accent_color(&self.s);
                    ui.painter().rect_stroke(card, corner, Stroke::new(1.5, accent.gamma_multiply(0.7)), egui::StrokeKind::Inside);
                }
            }
            if !active
                && let Some(other) = self.split.as_mut()
            {
                std::mem::swap(&mut self.tabs[self.active], other);
            }
        }
        self.pane_salt = 0;
        cols
    }

    /// "~/Documents" (or "Trash", "Recent", …) for a pane.
    fn pane_title(&self) -> String {
        match &self.tab().loc {
            Location::Dir(p) => {
                let home = crate::places::home();
                match p.strip_prefix(&home) {
                    Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
                    Ok(rest) => format!("~/{}", rest.display()),
                    Err(_) => p.display().to_string(),
                }
            }
            other => other.title(),
        }
    }

    /// A slim header over each pane: its folder's name, and where it is.
    fn pane_header(&mut self, ui: &mut egui::Ui, active: bool) {
        if self.s.terminal {
            // Terminal mode writes the folder into the box's border instead.
            ui.add_space(2.0);
            return;
        }
        let v = ui.visuals().clone();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), HEADER_H), Sense::hover());
        let name = self.tab().loc.title();
        let path = self.pane_title();
        let strong = if active { v.strong_text_color() } else { v.weak_text_color() };
        let g = ui.painter().layout_no_wrap(name, crate::fonts::ui_font(13.5), strong);
        let x = rect.left() + 8.0;
        let y = rect.center().y - 2.0;
        let name_w = g.size().x;
        ui.painter().galley(Pos2::new(x, y - g.size().y / 2.0), g, strong);
        if path != "~" {
            let mut job = egui::text::LayoutJob::simple_singleline(path, crate::fonts::ui_font(11.5), v.weak_text_color());
            job.wrap = egui::text::TextWrapping::truncate_at_width((rect.width() - name_w - 30.0).max(20.0));
            let g = ui.painter().layout_job(job);
            ui.painter().galley(Pos2::new(x + name_w + 10.0, y - g.size().y / 2.0 + 1.0), g, v.weak_text_color());
        }
    }
}
