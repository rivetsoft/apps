//! Where the pages go: their sizes at the zoom, one under another (continuous) or one at a time,
//! and the turning of points on a page to points on the screen and back, for any rotation.

use eframe::egui::{Pos2, Rect, Vec2};

/// The space around the pages, and between them.
pub const MARGIN: f32 = 16.0;
pub const GAP: f32 = 12.0;
pub const MIN_SCALE: f32 = 0.1;
pub const MAX_SCALE: f32 = 8.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Zoom {
    /// The widest page fills the width.
    Width,
    /// The whole page shows.
    Page,
    /// Screen points to a page point: 1.0 is actual size.
    Scale(f32),
}

impl Zoom {
    pub fn parse(s: &str) -> Option<Zoom> {
        match s {
            "width" => Some(Zoom::Width),
            "page" => Some(Zoom::Page),
            n => n.parse::<f32>().ok().filter(|v| v.is_finite()).map(|v| Zoom::Scale(v.clamp(MIN_SCALE, MAX_SCALE))),
        }
    }

    pub fn text(self) -> String {
        match self {
            Zoom::Width => "width".into(),
            Zoom::Page => "page".into(),
            Zoom::Scale(s) => format!("{s:.3}"),
        }
    }
}

/// A page's size once turned \`rot\` quarter turns clockwise.
pub fn turned(size: [f32; 2], rot: u8) -> [f32; 2] {
    if rot % 2 == 1 { [size[1], size[0]] } else { size }
}

/// The scale a zoom comes to, for pages shown in a view of this size. \`current\` is the page
/// Fit Page fits.
pub fn scale_for(zoom: Zoom, sizes: &[[f32; 2]], rot: u8, view: Vec2, current: usize) -> f32 {
    let room = Vec2::new((view.x - 2.0 * MARGIN).max(40.0), (view.y - 2.0 * MARGIN).max(40.0));
    let s = match zoom {
        Zoom::Scale(s) => s,
        Zoom::Width => {
            let widest = sizes.iter().map(|s| turned(*s, rot)[0]).fold(1.0f32, f32::max);
            room.x / widest
        }
        Zoom::Page => {
            let [w, h] = turned(sizes.get(current).copied().unwrap_or([612.0, 792.0]), rot);
            (room.x / w.max(1.0)).min(room.y / h.max(1.0))
        }
    };
    s.clamp(MIN_SCALE, MAX_SCALE)
}

/// The pages laid out: each page's place in the whole, in screen points from its top-left.
pub struct Layout {
    pub rects: Vec<Rect>,
    pub size: Vec2,
}

/// Every page one under another, centered; or (\`single\`) only \`current\`, and the others
/// placed nowhere (empty rectangles).
pub fn layout(sizes: &[[f32; 2]], rot: u8, scale: f32, single: Option<usize>, view: Vec2) -> Layout {
    let widest = sizes.iter().map(|s| turned(*s, rot)[0] * scale).fold(0.0f32, f32::max);
    let width = (widest + 2.0 * MARGIN).max(view.x);
    let mut rects = vec![Rect::NOTHING; sizes.len()];
    let mut y = MARGIN;
    for (i, s) in sizes.iter().enumerate() {
        if single.is_some_and(|c| c != i) {
            continue;
        }
        let [w, h] = turned(*s, rot).map(|v| v * scale);
        rects[i] = Rect::from_min_size(Pos2::new((width - w) / 2.0, y), Vec2::new(w, h));
        y += h + GAP;
    }
    let height = (y - GAP + MARGIN).max(view.y);
    // A single page shorter than the view sits in the middle of it.
    if let Some(c) = single
        && let Some(r) = rects.get_mut(c)
        && r.height() + 2.0 * MARGIN < view.y
    {
        *r = r.translate(Vec2::new(0.0, (view.y - r.height()) / 2.0 - MARGIN));
    }
    Layout { rects, size: Vec2::new(width, height) }
}

impl Layout {
    /// The page at a height in the whole (the nearest one, between pages).
    pub fn page_at(&self, y: f32) -> usize {
        let mut best = 0;
        for (i, r) in self.rects.iter().enumerate() {
            if r.is_negative() {
                continue;
            }
            if r.top() - GAP <= y {
                best = i;
            } else {
                break;
            }
        }
        best
    }

    /// The pages that cross the band [top, bottom].
    pub fn pages_in(&self, top: f32, bottom: f32) -> Vec<usize> {
        self.rects.iter().enumerate().filter(|(_, r)| !r.is_negative() && r.bottom() >= top && r.top() <= bottom).map(|(i, _)| i).collect()
    }
}

/// A point on a page as a fraction of its width and height (0 to 1 from its top-left, before
/// rotation) to the same as a fraction of the page as shown, turned \`rot\` quarter turns.
pub fn to_shown(rot: u8, [x, y]: [f32; 2]) -> [f32; 2] {
    match rot % 4 {
        1 => [1.0 - y, x],
        2 => [1.0 - x, 1.0 - y],
        3 => [y, 1.0 - x],
        _ => [x, y],
    }
}

/// The other way: a fraction of the page as shown to the page before rotation.
pub fn to_page(rot: u8, [x, y]: [f32; 2]) -> [f32; 2] {
    match rot % 4 {
        1 => [y, 1.0 - x],
        2 => [1.0 - x, 1.0 - y],
        3 => [1.0 - y, x],
        _ => [x, y],
    }
}

/// A point on the page in points (before rotation) to the screen, for a page shown in \`rect\`.
pub fn page_to_screen(rect: Rect, size: [f32; 2], rot: u8, p: [f32; 2]) -> Pos2 {
    let [u, v] = to_shown(rot, [p[0] / size[0].max(1.0), p[1] / size[1].max(1.0)]);
    Pos2::new(rect.left() + u * rect.width(), rect.top() + v * rect.height())
}

/// A point on the screen to the page, in points before rotation.
pub fn screen_to_page(rect: Rect, size: [f32; 2], rot: u8, p: Pos2) -> [f32; 2] {
    let [u, v] = to_page(rot, [(p.x - rect.left()) / rect.width().max(1.0), (p.y - rect.top()) / rect.height().max(1.0)]);
    [u * size[0], v * size[1]]
}

/// An area of the page (points, before rotation) on the screen.
pub fn area_to_screen(rect: Rect, size: [f32; 2], rot: u8, a: [f32; 4]) -> Rect {
    Rect::from_two_pos(page_to_screen(rect, size, rot, [a[0], a[1]]), page_to_screen(rect, size, rot, [a[2], a[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_round_trips() {
        for rot in 0..4 {
            for p in [[0.1, 0.2], [0.9, 0.05], [0.5, 0.5]] {
                let back = to_page(rot, to_shown(rot, p));
                assert!((back[0] - p[0]).abs() < 1e-6 && (back[1] - p[1]).abs() < 1e-6, "{rot} {p:?}");
            }
        }
        // A quarter turn clockwise takes the top-left corner to the top-right.
        assert_eq!(to_shown(1, [0.0, 0.0]), [1.0, 0.0]);
    }

    #[test]
    fn pages_one_under_another() {
        let sizes = [[600.0, 800.0], [800.0, 600.0], [600.0, 800.0]];
        let l = layout(&sizes, 0, 1.0, None, Vec2::new(500.0, 500.0));
        assert_eq!(l.rects[0].top(), MARGIN);
        assert_eq!(l.rects[1].top(), MARGIN + 800.0 + GAP);
        assert_eq!(l.size.x, 800.0 + 2.0 * MARGIN);
        assert_eq!(l.page_at(MARGIN + 900.0), 1);
        assert_eq!(l.pages_in(0.0, 900.0), vec![0, 1]);
        let one = layout(&sizes, 1, 0.5, Some(2), Vec2::new(1000.0, 1000.0));
        assert!(one.rects[0].is_negative());
        assert_eq!(one.rects[2].size(), Vec2::new(400.0, 300.0));
        let w = scale_for(Zoom::Width, &sizes, 0, Vec2::new(832.0, 600.0), 0);
        assert!((w - 1.0).abs() < 1e-6);
        assert_eq!(Zoom::parse(&Zoom::Scale(1.5).text()), Some(Zoom::Scale(1.5)));
    }
}
