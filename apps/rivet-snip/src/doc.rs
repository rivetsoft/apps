//! The picture being worked on: where it is clipped, and the marks drawn over it. Everything
//! is in the picture's own pixels, so it means the same at any zoom and in the saved file.

use eframe::egui::{Color32, Pos2, Rect, Vec2};

/// What a mark is. Also the tools that draw them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Arrow,
    Line,
    Rect,
    Ellipse,
    Pen,
    /// A wide see-through stroke, for highlighting.
    Marker,
    Text,
    /// A numbered circle, for steps.
    Step,
    /// Squares of averaged colour, so what is under them can't be read.
    Blur,
}

/// One mark. `a` and `b` are its two ends or opposite corners; a pen or marker stroke keeps
/// every point in `pts`; text and steps sit at `a`.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub kind: Kind,
    pub a: Pos2,
    pub b: Pos2,
    pub pts: Vec<Pos2>,
    pub color: Color32,
    /// The line's thickness in pixels; text and steps take their size from it.
    pub width: f32,
    pub text: String,
    /// A step's number.
    pub n: u32,
}

impl Mark {
    pub fn new(kind: Kind, at: Pos2, color: Color32, width: f32) -> Mark {
        Mark { kind, a: at, b: at, pts: vec![at], color, width, text: String::new(), n: 0 }
    }

    /// The height of text, and of a step's number, in pixels.
    pub fn text_size(&self) -> f32 {
        10.0 + self.width * 3.5
    }

    /// A step's circle.
    pub fn step_radius(&self) -> f32 {
        self.text_size() * 0.85
    }

    /// The box around it. Text is measured where it is drawn, so `text` gives its size.
    pub fn bounds(&self, text: Vec2) -> Rect {
        let pad = self.width / 2.0;
        match self.kind {
            Kind::Pen | Kind::Marker => Rect::from_points(&self.pts).expand(pad),
            Kind::Text => Rect::from_min_size(self.a, text),
            Kind::Step => Rect::from_center_size(self.a, Vec2::splat(self.step_radius() * 2.0)),
            Kind::Blur => Rect::from_two_pos(self.a, self.b),
            _ => Rect::from_two_pos(self.a, self.b).expand(pad),
        }
    }

    /// Is `p` on the mark, within `slack` pixels?
    pub fn hit(&self, p: Pos2, slack: f32, text: Vec2) -> bool {
        let near = self.width / 2.0 + slack;
        match self.kind {
            Kind::Arrow | Kind::Line => seg_dist(p, self.a, self.b) <= near,
            Kind::Pen | Kind::Marker => self.pts.windows(2).any(|w| seg_dist(p, w[0], w[1]) <= near) || (self.pts.len() == 1 && self.pts[0].distance(p) <= near),
            Kind::Rect => {
                let r = Rect::from_two_pos(self.a, self.b);
                r.expand(near).contains(p) && !r.shrink(near).contains(p)
            }
            Kind::Ellipse => {
                let r = Rect::from_two_pos(self.a, self.b);
                let (rx, ry) = (r.width() / 2.0, r.height() / 2.0);
                if rx < 1.0 || ry < 1.0 {
                    return r.expand(near).contains(p);
                }
                // How far along the way from the middle to the edge: 1 is on the outline.
                let d = ((p.x - r.center().x) / rx).hypot((p.y - r.center().y) / ry);
                (d - 1.0).abs() * rx.min(ry) <= near
            }
            Kind::Text | Kind::Step | Kind::Blur => self.bounds(text).expand(slack).contains(p),
        }
    }

    pub fn shift(&mut self, d: Vec2) {
        self.a += d;
        self.b += d;
        for p in &mut self.pts {
            *p += d;
        }
    }

    /// Too small to have been meant (a click with a tool that wants a drag).
    pub fn is_slip(&self) -> bool {
        match self.kind {
            Kind::Text | Kind::Step => false,
            Kind::Pen | Kind::Marker => false,
            _ => self.a.distance(self.b) < 4.0,
        }
    }
}

/// How far `p` is from the line between `a` and `b`.
fn seg_dist(p: Pos2, a: Pos2, b: Pos2) -> f32 {
    let ab = b - a;
    let len = ab.length_sq();
    if len < 1e-6 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The second corner of a drag held to a square, or to a line at a multiple of 45°.
pub fn held(kind: Kind, a: Pos2, b: Pos2) -> Pos2 {
    let d = b - a;
    match kind {
        Kind::Arrow | Kind::Line => {
            let step = std::f32::consts::FRAC_PI_4;
            let angle = (d.angle() / step).round() * step;
            a + Vec2::angled(angle) * d.length()
        }
        Kind::Rect | Kind::Ellipse | Kind::Blur => {
            let side = d.x.abs().max(d.y.abs());
            a + Vec2::new(side.copysign(d.x), side.copysign(d.y))
        }
        _ => b,
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Snap {
    crop: Rect,
    marks: Vec<Mark>,
}

#[derive(Clone, Debug)]
pub struct Doc {
    /// The whole picture's size in pixels.
    pub size: [usize; 2],
    /// The part that is kept, in whole pixels.
    pub crop: Rect,
    pub marks: Vec<Mark>,
    undo: Vec<Snap>,
    redo: Vec<Snap>,
    /// Counts every change, so the window knows when what it copied or saved is out of date.
    pub edits: u64,
}

impl Doc {
    pub fn new(size: [usize; 2]) -> Doc {
        Doc { size, crop: Rect::from_min_size(Pos2::ZERO, Vec2::new(size[0] as f32, size[1] as f32)), marks: Vec::new(), undo: Vec::new(), redo: Vec::new(), edits: 0 }
    }

    pub fn full(&self) -> Rect {
        Rect::from_min_size(Pos2::ZERO, Vec2::new(self.size[0] as f32, self.size[1] as f32))
    }

    pub fn is_cropped(&self) -> bool {
        self.crop != self.full()
    }

    /// Call before a change, so Undo can go back to how things are now.
    pub fn checkpoint(&mut self) {
        self.undo.push(Snap { crop: self.crop, marks: self.marks.clone() });
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.edits += 1;
    }

    /// Take back the last checkpoint when nothing came of it (a drag that was a slip).
    pub fn forget(&mut self) {
        if let Some(s) = self.undo.pop() {
            (self.crop, self.marks) = (s.crop, s.marks);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snap { crop: self.crop, marks: std::mem::take(&mut self.marks) });
            (self.crop, self.marks) = (s.crop, s.marks);
            self.edits += 1;
        }
    }

    pub fn redo(&mut self) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snap { crop: self.crop, marks: std::mem::take(&mut self.marks) });
            (self.crop, self.marks) = (s.crop, s.marks);
            self.edits += 1;
        }
    }

    /// Keep only `r` (clamped to the picture, in whole pixels). False when nothing is left.
    pub fn set_crop(&mut self, r: Rect) -> bool {
        let r = Rect::from_min_max(r.min.floor(), r.max.ceil()).intersect(self.full());
        if r.width() < 2.0 || r.height() < 2.0 || r == self.crop {
            return false;
        }
        self.checkpoint();
        self.crop = r;
        true
    }

    /// The number the next step gets: one more than the highest there is.
    pub fn next_step(&self) -> u32 {
        self.marks.iter().filter(|m| m.kind == Kind::Step).map(|m| m.n).max().unwrap_or(0) + 1
    }

    /// The crop as `[x0, y0, x1, y1]` in pixels.
    pub fn crop_px(&self) -> [usize; 4] {
        [self.crop.min.x as usize, self.crop.min.y as usize, self.crop.max.x as usize, self.crop.max.y as usize]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(x: f32, y: f32) -> Pos2 {
        Pos2::new(x, y)
    }

    #[test]
    fn undo_and_redo_walk_the_changes() {
        let mut d = Doc::new([400, 300]);
        d.checkpoint();
        d.marks.push(Mark::new(Kind::Step, p(10.0, 10.0), Color32::RED, 4.0));
        assert!(d.set_crop(Rect::from_min_max(p(10.4, 20.6), p(900.0, 100.2))));
        assert_eq!(d.crop, Rect::from_min_max(p(10.0, 20.0), p(400.0, 101.0)));
        assert_eq!(d.crop_px(), [10, 20, 400, 101]);
        d.undo();
        assert!(!d.is_cropped() && d.marks.len() == 1);
        d.undo();
        assert!(d.marks.is_empty() && !d.can_undo());
        d.redo();
        d.redo();
        assert!(d.is_cropped() && d.marks.len() == 1 && !d.can_redo());
        // A crop to nothing, or to what it already is, changes nothing.
        let edits = d.edits;
        assert!(!d.set_crop(Rect::from_min_max(p(50.0, 50.0), p(50.5, 90.0))));
        assert!(!d.set_crop(d.crop));
        assert_eq!(d.edits, edits);
    }

    #[test]
    fn steps_count_up_from_the_highest() {
        let mut d = Doc::new([100, 100]);
        assert_eq!(d.next_step(), 1);
        for n in [1, 2, 3] {
            let mut m = Mark::new(Kind::Step, p(0.0, 0.0), Color32::RED, 4.0);
            m.n = n;
            d.marks.push(m);
        }
        d.marks.remove(1);
        assert_eq!(d.next_step(), 4);
    }

    #[test]
    fn marks_are_hit_on_their_lines() {
        let z = Vec2::ZERO;
        let mut line = Mark::new(Kind::Arrow, p(0.0, 0.0), Color32::RED, 4.0);
        line.b = p(100.0, 0.0);
        assert!(line.hit(p(50.0, 3.0), 2.0, z));
        assert!(!line.hit(p(50.0, 9.0), 2.0, z));
        assert!(!line.hit(p(120.0, 0.0), 2.0, z));
        // A box is hit on its outline, not in its middle, so what is under it can be reached.
        let mut rect = Mark::new(Kind::Rect, p(0.0, 0.0), Color32::RED, 4.0);
        rect.b = p(100.0, 80.0);
        assert!(rect.hit(p(1.0, 40.0), 2.0, z));
        assert!(!rect.hit(p(50.0, 40.0), 2.0, z));
        let mut oval = Mark::new(Kind::Ellipse, p(0.0, 0.0), Color32::RED, 4.0);
        oval.b = p(100.0, 60.0);
        assert!(oval.hit(p(50.0, 1.0), 2.0, z));
        assert!(oval.hit(p(99.0, 30.0), 2.0, z));
        assert!(!oval.hit(p(50.0, 30.0), 2.0, z));
        assert!(!oval.hit(p(2.0, 2.0), 2.0, z));
        // A blurred patch is hit anywhere on it.
        let mut blur = Mark::new(Kind::Blur, p(0.0, 0.0), Color32::RED, 4.0);
        blur.b = p(40.0, 40.0);
        assert!(blur.hit(p(20.0, 20.0), 2.0, z));
        let mut pen = Mark::new(Kind::Pen, p(0.0, 0.0), Color32::RED, 4.0);
        pen.pts = vec![p(0.0, 0.0), p(10.0, 10.0), p(20.0, 0.0)];
        assert!(pen.hit(p(10.0, 9.0), 2.0, z));
        pen.shift(Vec2::new(100.0, 0.0));
        assert!(!pen.hit(p(10.0, 9.0), 2.0, z) && pen.hit(p(110.0, 9.0), 2.0, z));
    }

    #[test]
    fn shift_holds_a_drag_square_or_straight() {
        assert_eq!(held(Kind::Rect, p(0.0, 0.0), p(30.0, -80.0)), p(80.0, -80.0));
        let b = held(Kind::Line, p(0.0, 0.0), p(100.0, 8.0));
        assert!(b.y.abs() < 0.01 && (b.x - p(100.0, 8.0).to_vec2().length()).abs() < 0.01);
        let b = held(Kind::Arrow, p(0.0, 0.0), p(50.0, 60.0));
        assert!((b.x - b.y).abs() < 0.01);
    }
}
