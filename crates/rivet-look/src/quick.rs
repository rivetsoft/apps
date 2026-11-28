//! The quick panel: a search box over a list, floating near the top of the window over a dimmed
//! backdrop (Rivet Files' Quick Open and Quick Actions, and the same in the other apps). The app
//! ranks the rows (see [`fuzzy_score`]); the panel draws them and handles the keys.

use eframe::egui::{self, Align2, Color32, Id, Key, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};

use crate::a11y::{self, Access, Role};
use crate::fonts::ui_font;
use crate::theme::{paint_tex, r_lg, r_sm, selected_tile};

/// One row of the panel.
pub enum QuickRow {
    /// Something to open: an icon, a name, and where it is underneath.
    Item { icon: Option<egui::TextureHandle>, name: String, sub: String },
    /// A command: its group, its name, its keys, and a check box for on / off options.
    Command { group: String, label: String, keys: String, state: Option<bool> },
}

/// What happened in the panel this frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuickEvent {
    /// This row was chosen (Enter or a click).
    Pick(usize),
    /// Esc, or a click outside.
    Close,
}

/// The panel's state: what's typed, the highlighted row.
#[derive(Clone, Debug, Default)]
pub struct QuickPanel {
    pub query: String,
    pub sel: usize,
    /// Put the keyboard in the search box (on opening).
    pub focus: bool,
}

/// The words around the panel.
pub struct QuickText<'a> {
    pub hint: &'a str,
    pub empty: &'a str,
    pub legend: &'a str,
}

impl QuickPanel {
    pub fn new() -> Self {
        QuickPanel { query: String::new(), sel: 0, focus: true }
    }

    /// Draw the panel with these rows (already ranked for `self.query`).
    pub fn show(&mut self, ctx: &egui::Context, id: &str, rows: &[QuickRow], text: &QuickText, search_icon: Option<&egui::TextureHandle>) -> Option<QuickEvent> {
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        let n = rows.len();
        if self.sel >= n {
            self.sel = 0;
        }
        if n > 0 {
            if down {
                self.sel = (self.sel + 1) % n;
            }
            if up {
                self.sel = (self.sel + n - 1) % n;
            }
        }
        let mut out = if esc { Some(QuickEvent::Close) } else if enter && n > 0 { Some(QuickEvent::Pick(self.sel)) } else { None };

        let screen = ctx.content_rect();
        let w = 600f32.min(screen.width() - 40.0);
        let top = (screen.height() * 0.16).max(30.0);
        let id = Id::unique(id);
        let frame = egui::Frame::popup(&ctx.global_style()).inner_margin(egui::Margin::same(8)).corner_radius(r_lg());
        let area = egui::Modal::default_area(id).anchor(Align2::CENTER_TOP, Vec2::new(0.0, top));
        let backdrop = Color32::from_black_alpha(if ctx.global_style().visuals.dark_mode { 90 } else { 50 });
        let before = self.query.clone();
        let tall = rows.iter().any(|r| matches!(r, QuickRow::Item { .. }));
        let mut search: Option<egui::Response> = None;
        let mut chosen: Option<egui::Id> = None;
        let r = egui::Modal::new(id).area(area).frame(frame).backdrop_color(backdrop).show(ctx, |ui| {
            ui.set_width(w);
            let v = ui.visuals().clone();
            let weak = v.weak_text_color();
            // The search box.
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                if let Some(t) = search_icon {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(22.0, 36.0), Sense::hover());
                    paint_tex(ui.painter(), t, Rect::from_center_size(r.center(), Vec2::splat(18.0)), weak);
                }
                let edit = egui::TextEdit::singleline(&mut self.query)
                    .frame(egui::Frame::NONE)
                    .font(egui::FontId::proportional(18.0))
                    .align(Align2::LEFT_CENTER)
                    .min_size(Vec2::new(0.0, 36.0))
                    .desired_width(f32::INFINITY)
                    .hint_text(text.hint);
                let o = edit.show(ui);
                search = Some(egui::Response::clone(&o.response).role(Role::SearchInput, text.hint));
                if self.focus {
                    o.response.request_focus();
                    // The cursor at the end (after text was filled in, e.g. by Tab).
                    let mut st = o.state.clone();
                    let end = egui::text::CCursor::new(self.query.chars().count());
                    st.cursor.set_char_range(Some(egui::text::CCursorRange::one(end)));
                    st.store(ui.ctx(), o.response.id);
                    self.focus = false;
                }
            });
            if !rows.is_empty() {
                ui.add_space(4.0);
                let (sep, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
                ui.painter().hline(sep.x_range(), sep.center().y, Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color));
                ui.add_space(4.0);
            }
            let row_h = if tall { 44.0 } else { 34.0 };
            let max_rows = if tall { 8.0 } else { 11.0 };
            egui::ScrollArea::vertical().max_height(row_h * max_rows + 4.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = if tall { 2.0 } else { 1.0 };
                ui.scope_builder(a11y::area(Role::ListBox, text.hint), |ui| {
                for (i, row) in rows.iter().enumerate() {
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), row_h), Sense::click());
                    let resp = match row {
                        QuickRow::Item { name, sub, .. } => resp.role(Role::ListBoxOption, name.as_str()).described(sub.as_str()),
                        QuickRow::Command { group, label, keys, state } => {
                            let r = resp.role(Role::ListBoxOption, format!("{group}: {label}"));
                            let r = if keys.is_empty() { r } else { r.described(keys.as_str()) };
                            match state {
                                Some(on) => r.toggled(*on),
                                None => r,
                            }
                        }
                    }
                    .selected(i == self.sel);
                    if i == self.sel {
                        chosen = Some(resp.id);
                    }
                    if resp.hovered() && ui.input(|i| i.pointer.delta() != Vec2::ZERO) {
                        self.sel = i;
                    }
                    if i == self.sel {
                        let (fill, stroke) = selected_tile(&v, v.window_fill);
                        ui.painter().rect_filled(rect, r_sm(), fill);
                        ui.painter().rect_stroke(rect, r_sm(), stroke, StrokeKind::Inside);
                        if up || down {
                            ui.scroll_to_rect(rect, None);
                        }
                    }
                    let cy = rect.center().y;
                    match row {
                        QuickRow::Item { icon, name, sub } => {
                            if let Some(t) = icon {
                                paint_tex(ui.painter(), t, Rect::from_center_size(Pos2::new(rect.left() + 22.0, cy), Vec2::splat(24.0)), Color32::WHITE);
                            }
                            let x = rect.left() + 44.0;
                            let maxw = rect.right() - x - 10.0;
                            let mut job = egui::text::LayoutJob::simple_singleline(name.clone(), ui_font(14.0), v.strong_text_color());
                            job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                            let g = ui.painter().layout_job(job);
                            ui.painter().galley(Pos2::new(x, rect.top() + 5.0), g, v.strong_text_color());
                            let mut job = egui::text::LayoutJob::simple_singleline(sub.clone(), ui_font(12.0), weak);
                            job.wrap = egui::text::TextWrapping::truncate_at_width(maxw);
                            let g = ui.painter().layout_job(job);
                            ui.painter().galley(Pos2::new(x, rect.bottom() - 6.0 - g.size().y), g, weak);
                        }
                        QuickRow::Command { group, label, keys, state } => {
                            // On / off options show a check box like the menus do.
                            if let Some(on) = state {
                                let b = Rect::from_center_size(Pos2::new(rect.left() + 18.0, cy), Vec2::splat(14.0));
                                if *on {
                                    ui.painter().rect_filled(b, 3.0, v.selection.bg_fill);
                                    let c = b.center();
                                    let s = Stroke::new(1.6, v.selection.stroke.color);
                                    ui.painter().line_segment([c + Vec2::new(-3.5, 0.0), c + Vec2::new(-1.0, 2.8)], s);
                                    ui.painter().line_segment([c + Vec2::new(-1.0, 2.8), c + Vec2::new(3.8, -2.8)], s);
                                } else {
                                    ui.painter().rect_stroke(b, 3.0, Stroke::new(1.2, weak), StrokeKind::Inside);
                                }
                            }
                            let x = rect.left() + 36.0;
                            let g = ui.painter().layout_no_wrap(format!("{group}  "), ui_font(12.5), weak);
                            let gw = g.size().x;
                            ui.painter().galley(Pos2::new(x, cy - g.size().y / 2.0), g, weak);
                            let g = ui.painter().layout_no_wrap(label.clone(), ui_font(14.0), v.strong_text_color());
                            ui.painter().galley(Pos2::new(x + gw.max(52.0), cy - g.size().y / 2.0), g, v.strong_text_color());
                            if !keys.is_empty() {
                                let g = ui.painter().layout_no_wrap(keys.clone(), ui_font(12.0), weak);
                                let kr = Rect::from_min_size(Pos2::new(rect.right() - 12.0 - g.size().x - 12.0, cy - 10.0), Vec2::new(g.size().x + 12.0, 20.0));
                                ui.painter().rect_stroke(kr, 5.0, Stroke::new(1.0, v.widgets.noninteractive.bg_stroke.color), StrokeKind::Inside);
                                ui.painter().galley(Pos2::new(kr.left() + 6.0, cy - g.size().y / 2.0), g, weak);
                            }
                        }
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        out = Some(QuickEvent::Pick(i));
                    }
                }
                });
            });
            if rows.is_empty() && !self.query.trim().is_empty() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(egui::RichText::new(text.empty).color(weak));
                });
                ui.add_space(4.0);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(text.legend).size(11.5).color(weak));
            });
        });
        // The keyboard stays in the search box while the arrows move the highlight, so the
        // highlighted row is the box's active descendant: a screen reader says each row reached.
        if let (Some(s), Some(row)) = (&search, chosen) {
            ctx.accesskit_node_builder(s.id, |n| n.set_active_descendant(row.accesskit_id()));
        }
        if self.query != before {
            self.sel = 0;
        }
        if r.backdrop_response.clicked() {
            out = Some(QuickEvent::Close);
        }
        out
    }
}

/// How well `name` matches what was typed (higher is better), or None.
/// Starts-with beats contains, which beats the letters appearing in order.
pub fn fuzzy_score(name: &str, typed: &str) -> Option<i32> {
    if typed.is_empty() {
        return Some(0);
    }
    let n = name.to_lowercase();
    let t = typed.to_lowercase();
    if n == t {
        return Some(3000);
    }
    if n.starts_with(&t) {
        return Some(2000 - n.len() as i32);
    }
    if let Some(pos) = n.find(&t) {
        return Some(1000 - pos as i32 * 4 - n.len() as i32);
    }
    // Letters in order, with as few gaps as possible.
    let mut gaps = 0;
    let mut last: Option<usize> = None;
    let mut chars = n.char_indices();
    for tc in t.chars() {
        let (i, _) = chars.find(|(_, c)| *c == tc)?;
        if let Some(l) = last {
            gaps += (i - l - 1) as i32;
        }
        last = Some(i);
    }
    Some(500 - gaps * 8 - n.len() as i32)
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn fuzzy_order() {
        let s = |n| fuzzy_score(n, "doc");
        assert!(s("Documents") > s("my-docs"));
        assert!(s("my-docs") > s("Downloads-cache"));
        assert!(fuzzy_score("Downloads", "dwn").is_some());
        assert!(fuzzy_score("Music", "xyz").is_none());
    }
}
