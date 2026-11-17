//! Live mode in the editor: the text box lays the note out with [`crate::live`]'s styles, so
//! Markdown marks fold away off the line being edited, and bullets, check boxes, quote bars and
//! rules are drawn over the text afterwards.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use eframe::egui::text::{CCursor, LayoutJob, TextFormat};
use rivet_look::highlight;

use crate::live::{self, Deco, Hide};

/// Something in the rendered text that a press acts on.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Hit {
    /// A check box: the byte of its space or x.
    Check(usize),
    /// A link to a note, by name.
    Link(String),
    /// A code block's Copy button: the code.
    Copy(String),
    /// A super todo's button: its line, and where its popover hangs.
    Task(usize, Pos2),
}

/// The colors and sizes live mode draws with.
pub(super) struct LiveLook {
    pub size: f32,
    pub line_spacing: f32,
    pub text: Color32,
    pub strong: Color32,
    pub weak: Color32,
    pub link: Color32,
    pub tag: Color32,
    pub accent: Color32,
    pub code_bg: Color32,
    pub code: Color32,
    /// The tags that make a task a super todo.
    pub todo_tags: Vec<String>,
}

/// How much bigger each heading level is.
fn heading_scale(level: u8) -> f32 {
    match level {
        1 => 1.75,
        2 => 1.45,
        3 => 1.25,
        4 => 1.12,
        _ => 1.0,
    }
}

/// The bytes being edited: the cursor, or the selection, while the text has the keyboard.
pub(super) fn raw_range(ctx: &egui::Context, id: egui::Id, text: &str) -> Option<(usize, usize)> {
    if !ctx.memory(|m| m.has_focus(id)) {
        return None;
    }
    let r = egui::TextEdit::load_state(ctx, id)?.cursor.char_range()?;
    let to_byte = |c: usize| text.char_indices().nth(c).map(|(b, _)| b).unwrap_or(text.len());
    let (a, b) = (r.primary.index.0.min(r.secondary.index.0), r.primary.index.0.max(r.secondary.index.0));
    Some((to_byte(a), to_byte(b)))
}

/// The edited range moved to fit text that changed since it was read: the cursor was read
/// before this frame's typing, so text typed (or deleted) at it shifts it along.
pub(super) fn shift_raw(old: &str, new: &str, raw: Option<(usize, usize)>) -> Option<(usize, usize)> {
    let (a, b) = raw?;
    if old.len() == new.len() && old == new {
        return Some((a, b));
    }
    // Most edits happen at the cursor: typing or pasting there, or Backspace / Delete.
    if a == b && a <= old.len() && old.is_char_boundary(a) {
        let d = new.len() as isize - old.len() as isize;
        if d > 0 {
            let k = d as usize;
            if new.get(..a) == old.get(..a) && new.get(a + k..) == old.get(a..) {
                return Some((a + k, a + k));
            }
        } else if d < 0 {
            let k = (-d) as usize;
            if a >= k && new.get(..a - k) == old.get(..a - k) && new.get(a - k..) == old.get(a..) {
                return Some((a - k, a - k));
            }
            if new.get(..a) == old.get(..a) && new.get(a..) == old.get(a + k..) {
                return Some((a, a));
            }
        }
    }
    // Otherwise: where the two texts first differ.
    let p = old.char_indices().zip(new.chars()).find(|((_, x), y)| x != y).map(|((i, _), _)| i).unwrap_or(old.len().min(new.len()));
    let delta = new.len() as isize - old.len() as isize;
    let fix = |x: usize| {
        let mut y = if x >= p { ((x as isize + delta).max(p as isize)) as usize } else { x };
        y = y.min(new.len());
        while !new.is_char_boundary(y) {
            y += 1;
        }
        y
    };
    Some((fix(a), fix(b)))
}

/// Color the code inside fences for its language (Source mode: the rest is Markdown).
pub(super) fn color_blocks(job: &mut LayoutJob, text: &str, colors: &highlight::Colors, size: f32) {
    let l = live::analyze(text, None, None);
    if l.blocks.is_empty() {
        return;
    }
    let mut hl: Vec<Option<Color32>> = vec![None; text.len()];
    for b in l.blocks.iter().filter(|b| !b.inner.is_empty()) {
        let name = if b.lang.is_empty() { "code.txt".to_string() } else { format!("code.{}", b.lang) };
        let j = highlight::layout(&text[b.inner.clone()], &name, colors, egui::FontId::monospace(size));
        for s in &j.sections {
            for k in s.byte_range.start.0..s.byte_range.end.0 {
                if let Some(h) = hl.get_mut(b.inner.start + k) {
                    *h = Some(s.format.color);
                }
            }
        }
    }
    let mut out = Vec::with_capacity(job.sections.len());
    for sec in std::mem::take(&mut job.sections) {
        let (a, e) = (sec.byte_range.start.0, sec.byte_range.end.0);
        let mut k = a;
        while k < e {
            let c = hl[k];
            let mut m = k + 1;
            while m < e && (hl[m] == c || !text.is_char_boundary(m)) {
                m += 1;
            }
            let mut part = sec.clone();
            part.byte_range = egui::text::ByteIndex(k)..egui::text::ByteIndex(m);
            if k > a {
                part.leading_space = 0.0;
            }
            if let Some(c) = c {
                part.format.color = c;
            }
            out.push(part);
            k = m;
        }
    }
    job.sections = out;
}

/// Panels behind the code blocks (set into `under`, painted before the text), with the
/// language at the top right, and a Copy button there while the pointer is on the block.
pub(super) fn code_panels(ui: &egui::Ui, text: &str, out: &egui::text_edit::TextEditOutput, look: &LiveLook, under: egui::layers::ShapeIdx) -> Vec<(Rect, Hit)> {
    let l = live::analyze(text, None, None);
    let mut shapes = Vec::new();
    let mut hits = Vec::new();
    if l.blocks.is_empty() {
        return hits;
    }
    let at = |b: usize| out.galley.pos_from_cursor(CCursor::new(text[..b].chars().count())).translate(out.galley_pos.to_vec2());
    let pointer = ui.ctx().pointer_hover_pos();
    let p = ui.painter().clone();
    for b in &l.blocks {
        let last = text[b.whole.clone()].trim_end_matches(['\n', '\r']).len();
        let (top, bottom) = (at(b.whole.start), at(b.whole.start + last));
        let left = out.galley_pos.x - 10.0;
        let right = (out.galley_pos.x + out.galley.rect.width() + 10.0).max(out.response.response.rect.right());
        let r = Rect::from_min_max(Pos2::new(left, top.top() - 2.0), Pos2::new(right, bottom.bottom() + 2.0));
        let rounding = rivet_look::theme::r_md();
        shapes.push(egui::Shape::rect_filled(r, rounding, look.code_bg));
        shapes.push(egui::Shape::rect_stroke(r, rounding, Stroke::new(1.0, look.weak.gamma_multiply(0.18)), egui::StrokeKind::Inside));
        let font = egui::FontId::proportional(11.0);
        let hovered = pointer.is_some_and(|q| r.contains(q));
        let code = text[b.inner.clone()].trim_end_matches(['\n', '\r']).to_string();
        if hovered && !code.is_empty() {
            let g = p.layout_no_wrap("Copy".into(), font.clone(), look.text);
            let br = Rect::from_min_size(Pos2::new(r.right() - g.size().x - 22.0, r.top() + 3.0), g.size() + Vec2::new(14.0, 6.0));
            let over = pointer.is_some_and(|q| br.contains(q));
            p.rect_filled(br, rivet_look::theme::r_sm(), if over { look.weak.gamma_multiply(0.35) } else { look.weak.gamma_multiply(0.18) });
            p.galley(br.min + Vec2::new(7.0, 3.0), g, look.text);
            if over {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            hits.push((br, Hit::Copy(code)));
        } else if !b.lang.is_empty() {
            p.text(Pos2::new(r.right() - 10.0, r.top() + 9.0), egui::Align2::RIGHT_CENTER, b.lang.to_lowercase(), font, look.weak);
        }
    }
    ui.painter().set(under, egui::Shape::Vec(shapes));
    hits
}

/// The note laid out live.
pub(super) fn job(text: &str, raw: Option<(usize, usize)>, held: Option<(usize, u8)>, look: &LiveLook, colors: &highlight::Colors) -> LayoutJob {
    let l = live::analyze_with(text, raw, held, &look.todo_tags);
    let mono_size = look.size * 0.92;
    // Code blocks colored for their language.
    let mut hl: Vec<Option<Color32>> = vec![None; text.len()];
    for b in &l.blocks {
        let inner = &text[b.inner.clone()];
        let name = if b.lang.is_empty() { "code.txt".to_string() } else { format!("code.{}", b.lang) };
        let j = highlight::layout(inner, &name, colors, egui::FontId::monospace(mono_size));
        for s in &j.sections {
            for k in s.byte_range.start.0..s.byte_range.end.0 {
                if let Some(h) = hl.get_mut(b.inner.start + k) {
                    *h = Some(s.format.color);
                }
            }
        }
    }
    let mut job = LayoutJob { text: text.to_string(), ..Default::default() };
    let mut start = 0;
    while start < text.len() {
        let key = (l.st[start], hl[start]);
        let mut end = start + 1;
        while end < text.len() && (l.st[end], hl[end]) == key {
            end += 1;
        }
        // Stay on character edges.
        while end < text.len() && !text.is_char_boundary(end) {
            end += 1;
        }
        job.sections.push(egui::text::LayoutSection { leading_space: 0.0, byte_range: egui::text::ByteIndex(start)..egui::text::ByteIndex(end), format: format(&key.0, key.1, look) });
        start = end;
    }
    job
}

fn format(s: &live::St, hl: Option<Color32>, look: &LiveLook) -> TextFormat {
    let line_size = look.size * heading_scale(s.heading);
    let mono = s.code || s.block;
    let family = if mono {
        egui::FontFamily::Monospace
    } else if s.bold || s.heading > 0 {
        rivet_look::live::editor_bold_family()
    } else {
        rivet_look::live::editor_family()
    };
    let mut size = if mono { look.size * 0.92 } else { line_size };
    let mut color = if s.marker {
        look.weak
    } else if s.link {
        look.link
    } else if s.tag {
        look.tag
    } else if s.block {
        hl.unwrap_or(look.text)
    } else if s.code {
        look.code
    } else if s.heading > 0 {
        look.strong
    } else if s.quote || s.done {
        look.weak
    } else {
        look.text
    };
    match s.hide {
        Hide::Tiny => {
            size = 1.0;
            color = Color32::TRANSPARENT;
        }
        Hide::Clear => color = Color32::TRANSPARENT,
        Hide::No => {}
    }
    // Folded fences leave a little room above and below the code (the label sits on top).
    let line_height = if s.collapse { Some(if s.fence_top { 18.0 } else { 8.0 }) } else { Some(line_size * look.line_spacing) };
    TextFormat {
        font_id: egui::FontId::new(size, family),
        line_height,
        color,
        // Inline code gets a background; blocks sit on a panel drawn behind the text.
        background: if s.code && !s.block && s.hide == Hide::No { look.code_bg } else { Color32::TRANSPARENT },
        italics: s.italic || s.quote,
        underline: if s.link && s.hide == Hide::No { Stroke::new(1.0, look.link.gamma_multiply(0.45)) } else { Stroke::NONE },
        strikethrough: if (s.strike || s.done) && s.hide == Hide::No { Stroke::new(1.0, color) } else { Stroke::NONE },
        ..Default::default()
    }
}

/// Draw bullets, check boxes, quote bars and rules over the laid-out text. Returns the byte of
/// a check box's mark when one was clicked (to tick or untick it).
pub(super) fn decorate(ui: &egui::Ui, text: &str, raw: Option<(usize, usize)>, held: Option<(usize, u8)>, out: &egui::text_edit::TextEditOutput, look: &LiveLook) -> Vec<(Rect, Hit)> {
    let l = live::analyze_with(text, raw, held, &look.todo_tags);
    let p = ui.painter().with_clip_rect(ui.clip_rect());
    let at = |c: usize| out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
    let pointer = ui.ctx().pointer_hover_pos();
    let mut boxes = Vec::new();
    for d in &l.decos {
        match *d {
            Deco::Bullet(c) => {
                let (a, b) = (at(c), at(c + 1));
                let y = a.center().y;
                p.circle_filled(Pos2::new((a.left() + b.left()) / 2.0, y), (look.size * 0.17).max(2.2), look.accent);
            }
            Deco::Todo { at: c, due, priority, done } => {
                let a = at(c);
                let mut x = a.left() + 10.0;
                let y = a.center().y;
                if let Some(d) = due {
                    x = super::todos::due_pill(&p, Pos2::new(x, y), d, done, look.size).right() + 6.0;
                }
                if priority != crate::tasks::Priority::None {
                    super::todos::flag(&p, Pos2::new(x + 5.0, y), super::todos::priority_color(ui.ctx(), priority, look.weak));
                }
            }
            Deco::Check { at: c, done, doing, byte } => {
                let (a, b) = (at(c), at(c + 3));
                let side = (look.size * 0.95).min(b.left() - a.left()).max(10.0);
                let r = Rect::from_center_size(Pos2::new((a.left() + b.left()) / 2.0, a.center().y), Vec2::splat(side));
                if done {
                    p.rect_filled(r, 3.0, look.accent);
                    let cc = r.center();
                    let s = Stroke::new(1.8, Color32::WHITE);
                    let k = side / 14.0;
                    p.line_segment([cc + Vec2::new(-3.5, 0.0) * k, cc + Vec2::new(-1.0, 2.8) * k], s);
                    p.line_segment([cc + Vec2::new(-1.0, 2.8) * k, cc + Vec2::new(3.8, -2.8) * k], s);
                } else if doing {
                    p.rect_stroke(r, 3.0, Stroke::new(1.4, look.accent), egui::StrokeKind::Inside);
                    p.rect_filled(Rect::from_min_max(r.min, Pos2::new(r.center().x, r.max.y)).shrink(2.5), 1.5, look.accent);
                } else {
                    p.rect_stroke(r, 3.0, Stroke::new(1.4, look.weak), egui::StrokeKind::Inside);
                }
                // Clickable: the editor ticks it on the next press there.
                let hit = r.expand(3.0).intersect(out.text_clip_rect);
                if pointer.is_some_and(|q| hit.contains(q)) {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                boxes.push((hit, Hit::Check(byte)));
            }
            Deco::Quote(c) => {
                let a = at(c);
                p.rect_filled(Rect::from_min_size(Pos2::new(a.left() + 1.0, a.top() + 2.0), Vec2::new(3.0, (a.height() - 4.0).max(4.0))), 1.5, look.accent.gamma_multiply(0.6));
            }
            Deco::Rule(c) => {
                let a = at(c);
                let y = a.center().y;
                let x0 = out.galley_pos.x;
                let x1 = x0 + out.galley.size().x.max(out.text_clip_rect.width() - 8.0);
                p.line_segment([Pos2::new(x0, y), Pos2::new(x1, y)], Stroke::new(1.0, look.weak.gamma_multiply(0.6)));
            }
        }
    }
    // Rendered links to notes open with a click.
    for link in crate::links::outgoing(text) {
        if l.st.get(link.range.start).is_none_or(|s| s.hide == Hide::No) {
            continue;
        }
        let cs = text[..link.range.start].chars().count();
        let ce = cs + text[link.range.clone()].chars().count();
        let (a, b) = (at(cs), at(ce));
        let r = if (a.center().y - b.center().y).abs() < 1.0 { Rect::from_min_max(a.left_top(), b.left_bottom()) } else { Rect::from_min_max(a.left_top(), Pos2::new(out.text_clip_rect.right(), a.bottom())) };
        let r = r.intersect(out.text_clip_rect);
        if pointer.is_some_and(|q| r.contains(q)) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        boxes.push((r, Hit::Link(link.target)));
    }
    boxes
}

#[cfg(test)]
mod tests {
    use super::shift_raw;

    #[test]
    fn edited_range_follows_typing() {
        // Typing a letter at the cursor moves it along.
        assert_eq!(shift_raw("ab\ncd", "ab\ncdx", Some((5, 5))), Some((6, 6)));
        // Enter at the end of the first line: the cursor is on the new line.
        assert_eq!(shift_raw("# ab\ncd", "# ab\n\ncd", Some((4, 4))), Some((5, 5)));
        // Backspace.
        assert_eq!(shift_raw("abc", "ab", Some((3, 3))), Some((2, 2)));
        // Text changed after the cursor leaves it where it was.
        assert_eq!(shift_raw("ab\ncd", "ab\ncdx", Some((1, 1))), Some((1, 1)));
    }
}
