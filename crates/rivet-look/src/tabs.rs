//! The apps' tab strip (Rivet Edit, Rivet Terminal, Rivet Files): one look and one behavior.
//!
//! - **Look:** the open tab is raised off the window with an accent line under it, and the
//!   others light up under the pointer. A tab can carry an icon, italics (a preview), a dot
//!   while it has unsaved changes, and a small boxed count.
//! - **Closing:** with its ×, or a middle-click.
//! - **Moving:** drag a tab to move it ([`crate::tabdrag`]). It lifts and follows the pointer,
//!   with a glow, and the tabs it passes slide aside.
//! - **Too many to fit:** the strip scrolls sideways, with < and > at its end, the mouse wheel,
//!   and faded edges. The open tab is brought into view when it changes, and dragging a tab near
//!   either end scrolls the strip too.
//!
//! The app says what its tabs are ([`Tab`]) and gets back what was asked ([`TabEvent`]). It
//! draws whatever sits after the strip, such as a + button, where the strip ends.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Stroke, Vec2};

use crate::tabdrag::{self, TabDrag};
use crate::theme::{paint_tex, r_md, r_sm};
use crate::{fonts, palette};

/// A tab to show.
#[derive(Clone, Default)]
pub struct Tab {
    pub label: String,
    /// Shown when the pointer rests on it (the label when None).
    pub tip: Option<String>,
    pub icon: Option<egui::TextureHandle>,
    /// In italics (a preview tab).
    pub italic: bool,
    /// A dot instead of the × until the pointer is on it: changes not saved.
    pub dirty: bool,
    /// A small boxed count (Rivet Terminal's panes).
    pub badge: Option<String>,
}

/// What the strip was asked to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabEvent {
    Select(usize),
    Close(usize),
    /// Double-clicked (Rivet Edit keeps a preview tab open).
    Keep(usize),
    /// Dragged: swap these two (the open tab follows; see [`tabdrag::follow`]).
    Swap(usize, usize),
}

pub struct TabStrip {
    id: &'static str,
    accent: Color32,
    min_width: f32,
    max_width: f32,
    height: f32,
    reserve: f32,
}

/// What the strip did this frame.
pub struct TabOutput {
    pub events: Vec<TabEvent>,
    /// Each tab's response, in order (for things particular to an app, like files dropped on
    /// a tab).
    pub responses: Vec<egui::Response>,
}

impl TabOutput {
    /// The open tab after this frame's events: `active` moved along by swaps, then by a select.
    pub fn active(&self, mut active: usize) -> usize {
        for e in &self.events {
            match *e {
                TabEvent::Swap(a, b) => active = tabdrag::follow(active, a, b),
                TabEvent::Select(i) => active = i,
                _ => {}
            }
        }
        active
    }
}

impl TabStrip {
    /// A strip named `id` (its scroll and drag are kept under it), with `accent` for the open
    /// tab's line and the drag's glow.
    pub fn new(id: &'static str, accent: Color32) -> TabStrip {
        TabStrip { id, accent, min_width: 0.0, max_width: 320.0, height: 30.0, reserve: 0.0 }
    }

    /// Each tab's width, at least and at most (labels too long are cut with …).
    pub fn widths(mut self, min: f32, max: f32) -> TabStrip {
        (self.min_width, self.max_width) = (min, max.max(min));
        self
    }

    /// Leave this much room after the strip, for the app's own buttons.
    pub fn reserve(mut self, w: f32) -> TabStrip {
        self.reserve = w;
        self
    }

    /// Draw the tabs, `active` the open one.
    pub fn show(self, ui: &mut egui::Ui, tabs: &[Tab], active: usize) -> TabOutput {
        let ctx = ui.ctx().clone();
        let v = ui.visuals().clone();
        let mut events = Vec::new();
        let key = |what: &str| egui::Id::unique(("rivet-tabs", self.id, what.to_string()));
        let (over_id, shown_id, goto_id) = (key("overflow"), key("shown"), key("goto"));

        // Each tab's label laid out, and its width.
        let laid: Vec<(std::sync::Arc<egui::Galley>, f32)> = tabs
            .iter()
            .map(|t| {
                let left = if t.icon.is_some() { 30.0 } else { 14.0 };
                let right = 34.0 + if t.badge.is_some() { 24.0 } else { 0.0 };
                let room = (self.max_width - left - right).max(20.0);
                let fmt = egui::TextFormat { font_id: fonts::ui_font(13.0), color: v.text_color(), italics: t.italic, ..Default::default() };
                let mut job = egui::text::LayoutJob::single_section(t.label.clone(), fmt);
                job.wrap = egui::text::TextWrapping::truncate_at_width(room);
                let g = ui.painter().layout_job(job);
                let w = (g.size().x + left + right).clamp(self.min_width, self.max_width);
                (g, w)
            })
            .collect();
        let gap = 2.0;
        let content: f32 = laid.iter().map(|l| l.1).sum::<f32>() + gap * tabs.len().saturating_sub(1) as f32;

        let overflow = ctx.data(|d| d.get_temp::<bool>(over_id)).unwrap_or(false);
        let arrows_w = if overflow { 62.0 } else { 0.0 };
        let room = (ui.available_width() - self.reserve).max(60.0);
        let width = if overflow { room } else { content.min(room) };
        let strip = Rect::from_min_size(ui.cursor().min, Vec2::new(width, self.height));
        let tabs_rect = Rect::from_min_max(strip.min, Pos2::new(strip.max.x - arrows_w, strip.max.y));
        ui.allocate_rect(strip, Sense::hover());

        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(tabs_rect).layout(egui::Layout::left_to_right(egui::Align::Center)));
        let wheel = if ui.rect_contains_pointer(tabs_rect) { ctx.input(|i| i.smooth_scroll_delta.y + i.smooth_scroll_delta.x) } else { 0.0 };
        let bring = ctx.data(|d| d.get_temp::<usize>(shown_id)) != Some(active);
        ctx.data_mut(|d| d.insert_temp(shown_id, active));
        let mut sa = egui::ScrollArea::horizontal()
            .id_salt(("rivet-tabs-scroll", self.id))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            // Dragging a tab moves it, so dragging doesn't scroll the strip.
            .scroll_source(egui::scroll_area::ScrollSource { scroll_bar: false, drag: egui::scroll_area::DragScroll::Never, mouse_wheel: false });
        if let Some(x) = ctx.data_mut(|d| d.remove_temp::<f32>(goto_id)) {
            sa = sa.horizontal_scroll_offset(x);
        }
        let mut drag = TabDrag::new(self.id).within(tabs_rect).load(&ctx);
        let lifted = drag.dragging(&ctx);
        let mut responses = Vec::with_capacity(tabs.len());
        let raised = palette::mix(v.panel_fill, v.text_color(), if v.dark_mode { 0.09 } else { 0.07 });
        let out = sa.show(&mut inner, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (i, (t, (g, w))) in tabs.iter().zip(&laid).enumerate() {
                    let on = i == active;
                    let (slot, resp) = ui.allocate_exact_size(Vec2::new(*w, self.height), Sense::click_and_drag());
                    drag.add(slot, &resp);
                    if resp.drag_started() {
                        events.push(TabEvent::Select(i));
                    }
                    if bring && on {
                        ui.scroll_to_rect(slot, None);
                    }
                    // Drawn where the drag puts it: lifted under the pointer, or sliding into place.
                    let r = drag.shown(i, slot);
                    let p = drag.painter(ui, i);
                    // While a tab is dragged, the others show no hover (the drag holds its slot).
                    let hover = resp.hovered() && lifted.is_none();
                    drag.under(&p, i, r, raised, r_md());
                    if on {
                        p.rect_filled(r, r_md(), raised);
                        p.line_segment([r.left_bottom() + Vec2::new(10.0, -1.0), r.right_bottom() + Vec2::new(-10.0, -1.0)], Stroke::new(2.0, self.accent));
                    } else if hover {
                        p.rect_filled(r, r_md(), palette::mix(v.panel_fill, v.text_color(), 0.05));
                    }
                    let mut x = r.left() + 14.0;
                    if let Some(tex) = &t.icon {
                        paint_tex(&p, tex, Rect::from_center_size(Pos2::new(r.left() + 16.0, r.center().y), Vec2::splat(14.0)), Color32::WHITE);
                        x = r.left() + 30.0;
                    }
                    let fg = if on { v.strong_text_color() } else { v.weak_text_color() };
                    p.galley(Pos2::new(x, r.center().y - g.size().y / 2.0), g.clone(), fg);
                    if let Some(b) = &t.badge {
                        let cx = r.right() - 38.0;
                        p.text(Pos2::new(cx, r.center().y), Align2::CENTER_CENTER, b, fonts::ui_font(10.5), v.weak_text_color());
                        p.rect_stroke(Rect::from_center_size(Pos2::new(cx, r.center().y), Vec2::new(16.0, 14.0)), 3.0, Stroke::new(1.0, v.weak_text_color().gamma_multiply(0.6)), egui::StrokeKind::Inside);
                    }
                    // Close: a dot while it has changes, × under the pointer.
                    let xr = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.center().y), Vec2::splat(18.0));
                    let over_x = ui.rect_contains_pointer(xr) && lifted.is_none();
                    if t.dirty && !over_x {
                        p.circle_filled(xr.center(), 4.0, fg);
                    } else if on || hover {
                        if over_x {
                            p.rect_filled(xr, r_sm(), v.widgets.hovered.weak_bg_fill);
                        }
                        p.text(xr.center(), Align2::CENTER_CENTER, "×", fonts::ui_font(15.0), fg);
                    }
                    drag.over(&p, i, r, self.accent, r_md());
                    if resp.double_clicked() {
                        events.push(TabEvent::Keep(i));
                    }
                    let resp = resp.on_hover_text(t.tip.clone().unwrap_or_else(|| t.label.clone()));
                    if resp.clicked() {
                        events.push(if over_x { TabEvent::Close(i) } else { TabEvent::Select(i) });
                    }
                    if resp.middle_clicked() {
                        events.push(TabEvent::Close(i));
                    }
                    responses.push(resp);
                }
            });
        });
        if let Some((a, b)) = drag.finish(&ctx) {
            events.push(TabEvent::Swap(a, b));
        }

        // Overflow: how far it can scroll, the arrows, the wheel, the edges.
        let max = (out.content_size.x - out.inner_rect.width()).max(0.0);
        let mut off = out.state.clamped_offset().x;
        let over_now = content > room + 0.5;
        if over_now != overflow {
            ctx.data_mut(|d| d.insert_temp(over_id, over_now));
            // The strip's width changed (arrows came or went): show the open tab again.
            ctx.data_mut(|d| d.remove_temp::<usize>(shown_id));
            ctx.request_repaint();
        }
        let mut step = -wheel;
        // Dragging a tab near either end scrolls the strip, to carry it past the tabs hidden there.
        if overflow
            && lifted.is_some()
            && let Some(p) = ctx.pointer_interact_pos()
        {
            if p.x < tabs_rect.left() + 40.0 && off > 0.5 {
                step -= 8.0;
            } else if p.x > tabs_rect.right() - 40.0 && off < max - 0.5 {
                step += 8.0;
            }
        }
        if overflow {
            let arrow = |x: f32, left: bool, on: bool| -> bool {
                let r = Rect::from_center_size(Pos2::new(x, tabs_rect.center().y), Vec2::new(26.0, 26.0));
                let resp = ui.interact(r, key(if left { "left" } else { "right" }), Sense::click());
                if on && resp.hovered() {
                    ui.painter().rect_filled(r, r_sm(), v.widgets.hovered.weak_bg_fill);
                }
                let col = if on { v.text_color() } else { v.weak_text_color().gamma_multiply(0.5) };
                let c = r.center();
                let d = if left { -1.0 } else { 1.0 };
                ui.painter().line_segment([c + Vec2::new(-2.5 * d, -5.0), c + Vec2::new(2.5 * d, 0.0)], Stroke::new(1.6, col));
                ui.painter().line_segment([c + Vec2::new(2.5 * d, 0.0), c + Vec2::new(-2.5 * d, 5.0)], Stroke::new(1.6, col));
                on && resp.on_hover_text(if left { "Scroll the tabs left" } else { "Scroll the tabs right" }).clicked()
            };
            let x = strip.right();
            if arrow(x - 44.0, true, off > 0.5) {
                step -= tabs_rect.width() * 0.6;
            }
            if arrow(x - 16.0, false, off < max - 0.5) {
                step += tabs_rect.width() * 0.6;
            }
        }
        if step != 0.0 {
            off = (off + step).clamp(0.0, max);
            ctx.data_mut(|d| d.insert_temp(goto_id, off));
            if wheel != 0.0 {
                ctx.input_mut(|i| i.smooth_scroll_delta = Vec2::ZERO);
            }
            ctx.request_repaint();
        }
        // Faded edges where more tabs are hidden.
        let bg = v.panel_fill;
        let fade = |x_edge: f32, inward: f32| {
            let r = Rect::from_x_y_ranges(x_edge.min(x_edge + inward)..=x_edge.max(x_edge + inward), tabs_rect.top()..=tabs_rect.bottom());
            let (a, b) = if inward > 0.0 { (bg, Color32::TRANSPARENT) } else { (Color32::TRANSPARENT, bg) };
            let mut mesh = egui::Mesh::default();
            mesh.colored_vertex(r.left_top(), a);
            mesh.colored_vertex(r.right_top(), b);
            mesh.colored_vertex(r.right_bottom(), b);
            mesh.colored_vertex(r.left_bottom(), a);
            mesh.add_triangle(0, 1, 2);
            mesh.add_triangle(0, 2, 3);
            ui.painter().add(egui::Shape::mesh(mesh));
        };
        if off > 0.5 {
            fade(tabs_rect.left(), 28.0);
        }
        if off < max - 0.5 {
            fade(tabs_rect.right(), -28.0);
        }
        TabOutput { events, responses }
    }
}
