//! Live mode in the editor: the text box lays the note out with [`crate::live`]'s styles, so
//! Markdown marks fold away off the line being edited, and bullets, check boxes, quote bars and
//! rules are drawn over the text afterwards.

use eframe::egui::{self, Color32, Pos2, Rect, Stroke, Vec2};
use eframe::egui::text::{CCursor, LayoutJob, TextFormat};
use rivet_look::highlight;

use crate::live::{self, Deco, Hide};

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

/// The note laid out live.
pub(super) fn job(text: &str, raw: Option<(usize, usize)>, held: Option<(usize, u8)>, look: &LiveLook, colors: &highlight::Colors) -> LayoutJob {
    let l = live::analyze(text, raw, held);
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
    let line_height = if s.collapse { Some(3.0) } else { Some(line_size * look.line_spacing) };
    TextFormat {
        font_id: egui::FontId::new(size, family),
        line_height,
        color,
        background: if (s.code || s.block) && s.hide == Hide::No && !s.collapse { look.code_bg } else { Color32::TRANSPARENT },
        italics: s.italic || s.quote,
        underline: if s.link && s.hide == Hide::No { Stroke::new(1.0, look.link.gamma_multiply(0.45)) } else { Stroke::NONE },
        strikethrough: if (s.strike || s.done) && s.hide == Hide::No { Stroke::new(1.0, color) } else { Stroke::NONE },
        ..Default::default()
    }
}

/// Draw bullets, check boxes, quote bars and rules over the laid-out text. Returns the byte of
/// a check box's mark when one was clicked (to tick or untick it).
pub(super) fn decorate(ui: &egui::Ui, text: &str, raw: Option<(usize, usize)>, held: Option<(usize, u8)>, out: &egui::text_edit::TextEditOutput, look: &LiveLook) -> Option<usize> {
    let l = live::analyze(text, raw, held);
    let p = ui.painter().with_clip_rect(out.text_clip_rect);
    let at = |c: usize| out.galley.pos_from_cursor(CCursor::new(c)).translate(out.galley_pos.to_vec2());
    let clicked = out.response.response.clicked();
    let pointer = ui.ctx().pointer_interact_pos();
    let mut toggle = None;
    for d in &l.decos {
        match *d {
            Deco::Bullet(c) => {
                let (a, b) = (at(c), at(c + 1));
                let y = a.center().y;
                p.circle_filled(Pos2::new((a.left() + b.left()) / 2.0, y), (look.size * 0.17).max(2.2), look.accent);
            }
            Deco::Check { at: c, done, byte } => {
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
                } else {
                    p.rect_stroke(r, 3.0, Stroke::new(1.4, look.weak), egui::StrokeKind::Inside);
                }
                if clicked && pointer.is_some_and(|q| r.expand(3.0).contains(q)) {
                    toggle = Some(byte);
                }
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
    toggle
}
