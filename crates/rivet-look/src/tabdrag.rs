//! Reordering tabs by dragging them, for the apps' tab strips (Rivet Edit, Rivet Terminal).
//!
//! The strip draws its tabs with `Sense::click_and_drag` and hands each one to
//! [`TabDrag::add`]. [`TabDrag::finish`] then says which two tabs to swap. A tab trades places
//! with its neighbor as soon as the pointer passes that neighbor's middle, as in a browser.
//!
//! It also gives the drag some life. The tab being dragged lifts off the strip with a shadow and
//! follows the pointer with a glowing edge. The tab it passes slides
//! into its new place, and on release the dragged tab glides back into its slot. The strip draws
//! each tab at [`TabDrag::shown`], with [`TabDrag::painter`], between
//! [`TabDrag::under`] and [`TabDrag::over`].
//!
//! The dragged tab is remembered by its place in the strip, not by its widget, so the strip can
//! keep giving its tabs ids by position.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};

/// How long a tab takes to slide into place (seconds).
const SLIDE: f64 = 0.16;

/// What's kept between frames.
#[derive(Clone, Default)]
struct State {
    /// The tab being dragged, and where on it the pointer took hold (from its left edge).
    held: Option<(usize, f32)>,
    /// Tabs still sliding into place: which, how far off they started, and when.
    slides: Vec<(usize, f32, f64)>,
}

pub struct TabDrag {
    id: egui::Id,
    rects: Vec<Rect>,
    started: Option<(usize, f32)>,
    state: State,
    now: f64,
    pointer: Option<Pos2>,
    bounds: Option<Rect>,
    loaded: bool,
}

impl TabDrag {
    /// Start a strip; `id` names it (one drag per strip).
    pub fn new(id: &'static str) -> TabDrag {
        TabDrag { id: egui::Id::unique(("rivet-tab-drag", id)), rects: Vec::new(), started: None, state: State::default(), now: 0.0, pointer: None, bounds: None, loaded: false }
    }

    /// Keep the dragged tab inside `r` (the strip's own area).
    pub fn within(mut self, r: Rect) -> TabDrag {
        self.bounds = Some(r);
        self
    }

    /// Read what's kept from last frame. Call once, before the tabs.
    pub fn load(mut self, ctx: &egui::Context) -> TabDrag {
        self.state = ctx.data(|d| d.get_temp::<State>(self.id)).unwrap_or_default();
        (self.now, self.pointer) = ctx.input(|i| (i.time, i.pointer.interact_pos()));
        self.state.slides.retain(|s| self.now - s.2 < SLIDE);
        self.loaded = true;
        if !self.state.slides.is_empty() || self.state.held.is_some() {
            ctx.request_repaint();
        }
        self
    }

    /// The tab being dragged now, if any.
    pub fn dragging(&self, ctx: &egui::Context) -> Option<usize> {
        match self.state.held {
            Some((i, _)) => Some(i),
            None => ctx.data(|d| d.get_temp::<State>(self.id)).and_then(|s| s.held).map(|h| h.0),
        }
    }

    /// Where to draw tab `i`, whose place in the strip is `r`: under the pointer while it's
    /// dragged, part way back while it slides into place, else `r`.
    pub fn shown(&self, i: usize, r: Rect) -> Rect {
        if let (Some((k, grab)), Some(p)) = (self.state.held, self.pointer)
            && k == i
        {
            let mut left = p.x - grab;
            if let Some(b) = self.bounds {
                left = left.clamp(b.left(), (b.right() - r.width()).max(b.left()));
            }
            return Rect::from_min_size(Pos2::new(left, r.top() - 2.0), r.size());
        }
        match self.state.slides.iter().rev().find(|s| s.0 == i) {
            Some(&(_, dx, t0)) => {
                let t = ((self.now - t0) / SLIDE).clamp(0.0, 1.0) as f32;
                let ease = 1.0 - (1.0 - t).powi(3);
                r.translate(Vec2::new(dx * (1.0 - ease), 0.0))
            }
            None => r,
        }
    }

    /// The painter for tab `i`: the dragged one is drawn above the others.
    pub fn painter(&self, ui: &egui::Ui, i: usize) -> egui::Painter {
        if self.state.held.is_some_and(|h| h.0 == i) {
            ui.ctx().layer_painter(egui::LayerId::new(egui::Order::Foreground, self.id))
        } else {
            ui.painter().clone()
        }
    }

    /// Whether tab `i` is the one lifted.
    pub fn lifted(&self, i: usize) -> bool {
        self.state.held.is_some_and(|h| h.0 == i)
    }

    /// Under a lifted tab: its shadow and a solid fill (`fill`), so it hides what it passes over.
    pub fn under(&self, p: &egui::Painter, i: usize, r: Rect, fill: Color32, radius: u8) {
        if !self.lifted(i) {
            return;
        }
        let shadow = egui::epaint::Shadow { offset: [0, 5], blur: 16, spread: 0, color: Color32::from_black_alpha(70) };
        p.add(shadow.as_shape(r, radius));
        p.rect_filled(r, radius, fill);
    }

    /// Over a lifted tab: a glowing edge in `accent`.
    pub fn over(&self, p: &egui::Painter, i: usize, r: Rect, accent: Color32, radius: u8) {
        if !self.lifted(i) {
            return;
        }
        p.rect_stroke(r.expand(2.0), radius.saturating_add(2), Stroke::new(3.0, accent.gamma_multiply(0.18)), egui::StrokeKind::Outside);
        p.rect_stroke(r, radius, Stroke::new(1.5, accent), egui::StrokeKind::Inside);
    }

    /// The next tab, in order: where it is in the strip, and its response (sensing drags).
    pub fn add(&mut self, rect: Rect, resp: &egui::Response) {
        if resp.drag_started_by(egui::PointerButton::Primary) {
            let grab = resp.interact_pointer_pos().map(|p| p.x - rect.left()).unwrap_or(rect.width() / 2.0);
            self.started = Some((self.rects.len(), grab));
        }
        self.rects.push(rect);
    }

    /// After the last tab: the tabs to swap this frame, `(from, to)`. Swap them, and move the
    /// open tab along if it's one of them.
    pub fn finish(mut self, ctx: &egui::Context) -> Option<(usize, usize)> {
        if !self.loaded {
            self = self.load(ctx);
        }
        if self.state.held.is_none() && self.started.is_none() {
            // Nothing held (maybe tabs still sliding): just keep the slides.
            if !self.state.slides.is_empty() {
                ctx.data_mut(|d| d.insert_temp(self.id, self.state));
            } else {
                ctx.data_mut(|d| d.remove::<State>(self.id));
            }
            return None;
        }
        if let Some(s) = self.started {
            self.state.held = Some(s);
        }
        let (k, grab) = self.state.held.unwrap_or_default();
        let down = ctx.input(|i| i.pointer.primary_down());
        if !down || k >= self.rects.len() {
            // Let go: glide from where it was drawn back into its slot.
            if let Some(r) = self.rects.get(k).copied() {
                let shown = self.shown(k, r);
                self.state.slides.push((k, shown.left() - r.left(), self.now));
            }
            self.state.held = None;
            ctx.data_mut(|d| d.insert_temp(self.id, self.state));
            ctx.request_repaint();
            return None;
        }
        ctx.set_cursor_icon(egui::CursorIcon::Grabbing);
        ctx.request_repaint();
        let x = self.pointer.map(|p| p.x).unwrap_or(self.rects[k].center().x);
        let to = if k + 1 < self.rects.len() && x > self.rects[k + 1].center().x {
            k + 1
        } else if k > 0 && x < self.rects[k - 1].center().x {
            k - 1
        } else {
            k
        };
        if to != k {
            // The tab passed over slides from its old place to its new one.
            let (old, moved) = (self.rects[to], self.rects[k]);
            let new_left = if k < to { moved.left() } else { moved.right() - old.width() };
            self.state.slides.retain(|s| s.0 != k && s.0 != to);
            self.state.slides.push((k, old.left() - new_left, self.now));
        }
        self.state.held = Some((to, grab));
        ctx.data_mut(|d| d.insert_temp(self.id, self.state));
        (to != k).then_some((k, to))
    }
}

/// The open tab's place after `from` and `to` swapped.
pub fn follow(active: usize, from: usize, to: usize) -> usize {
    if active == from {
        to
    } else if active == to {
        from
    } else {
        active
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Three 100-point tabs in a row; drag the first by its middle to `to_x` over a few frames,
    /// applying each swap as an app would. The order at the end.
    fn drag_first_to(to_x: f32) -> Vec<&'static str> {
        let ctx = egui::Context::default();
        let mut tabs = vec!["a", "b", "c"];
        let y = 15.0;
        let mut steps: Vec<Vec<egui::Event>> = vec![vec![egui::Event::PointerMoved(egui::pos2(50.0, y))]];
        steps.push(vec![egui::Event::PointerButton { pos: egui::pos2(50.0, y), button: egui::PointerButton::Primary, pressed: true, modifiers: Default::default() }]);
        for k in 1..=10 {
            steps.push(vec![egui::Event::PointerMoved(egui::pos2(50.0 + (to_x - 50.0) * k as f32 / 10.0, y))]);
        }
        for _ in 0..4 {
            steps.push(Vec::new());
        }
        steps.push(vec![egui::Event::PointerButton { pos: egui::pos2(to_x, y), button: egui::PointerButton::Primary, pressed: false, modifiers: Default::default() }]);
        for events in steps {
            let input = egui::RawInput { events, screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 100.0))), ..Default::default() };
            let mut out = ctx.run_ui(input, |ui| {
                let mut drag = TabDrag::new("test-tabs");
                for (i, _) in tabs.iter().enumerate() {
                    let r = Rect::from_min_size(egui::pos2(i as f32 * 100.0, 0.0), egui::vec2(100.0, 30.0));
                    let resp = ui.interact(r, egui::Id::unique(("tab", i)), egui::Sense::click_and_drag());
                    drag.add(r, &resp);
                }
                if let Some((a, b)) = drag.finish(ui.ctx()) {
                    tabs.swap(a, b);
                }
            });
            out.textures_delta.clear();
        }
        tabs
    }

    #[test]
    fn dragging_moves_a_tab_along() {
        assert_eq!(drag_first_to(260.0), ["b", "c", "a"]);
        assert_eq!(drag_first_to(160.0), ["b", "a", "c"]);
        // Not past the neighbor's middle: stays.
        assert_eq!(drag_first_to(120.0), ["a", "b", "c"]);
    }

    #[test]
    fn open_tab_follows_a_swap() {
        assert_eq!(follow(2, 2, 3), 3);
        assert_eq!(follow(3, 2, 3), 2);
        assert_eq!(follow(0, 2, 3), 0);
    }
}
