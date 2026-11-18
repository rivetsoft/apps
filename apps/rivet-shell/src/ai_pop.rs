//! The AI Usage popup, opened from the panel's AI item: each provider's plan and limits, with a
//! bar for each and when it resets, and whether Claude Code is running.

use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;

use crate::ai::{Provider, State, ago, name, until};
use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, one_line, ui_font};

const HEAD: f32 = 46.0;
const PHEAD: f32 = 30.0;
const WROW: f32 = 36.0;
const LINE: f32 = 24.0;
const GAP: f32 = 8.0;

/// The providers with something to show first, then the ones not set up.
fn order(q: &crate::ai::Quotas) -> (Vec<&Provider>, Vec<&Provider>) {
    q.providers.iter().partition(|p| !matches!(p.state, State::Missing(_)))
}

fn block(p: &Provider) -> f32 {
    PHEAD
        + match &p.state {
            State::Ok => p.windows.len() as f32 * WROW + if p.balance.is_some() { LINE } else { 0.0 } + if p.windows.is_empty() && p.balance.is_none() { LINE } else { 0.0 },
            _ => LINE,
        }
        + GAP
}

pub fn height(m: &Model) -> f32 {
    let (shown, missing) = order(&m.extra.ai);
    let mut h = 28.0 + HEAD + 6.0 + shown.iter().map(|p| block(p)).sum::<f32>();
    if !missing.is_empty() {
        h += PHEAD + missing.len() as f32 * LINE;
    }
    if shown.is_empty() && missing.is_empty() {
        h += LINE * 2.0;
    }
    h
}

pub fn popup(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let accent = kit.accent(m.s);
    let v = ui.visuals().clone();
    let q = m.extra.ai.clone();
    let left = m.s.ai_left;
    let w = ui.available_width();
    // Rows sit flush, so height() adds up.
    ui.spacing_mut().item_spacing.y = 0.0;

    // Title, Claude Code's state, refresh.
    let (r, _) = ui.allocate_exact_size(Vec2::new(w, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 13.0), Align2::LEFT_CENTER, "AI Usage", ui_font(14.5), v.strong_text_color());
    let sub = match (q.claude_code, q.claude_sessions, q.claude_busy) {
        (_, 0, _) if q.claude_code => "Claude Code: no sessions running".to_string(),
        (_, 0, _) => "Claude Code isn't installed".to_string(),
        (_, n, busy) => format!("Claude Code: {} running{}", if n == 1 { "1 session".to_string() } else { format!("{n} sessions") }, if busy { " · working now" } else { "" }),
    };
    if q.claude_busy {
        p.circle_filled(Pos2::new(r.left() + 5.0, r.top() + 33.0), 3.5, accent);
    }
    let sx = if q.claude_busy { 14.0 } else { 2.0 };
    let g = one_line(&p, &sub, ui_font(12.0), v.weak_text_color(), w - 60.0);
    p.galley(Pos2::new(r.left() + sx, r.top() + 26.0), g, v.weak_text_color());
    let tool = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.top() + 18.0), Vec2::splat(30.0));
    if tool_button(ui, tool, "ai-refresh", "Read again now", &|p, c, col| glyph::refresh(p, c, col)) {
        m.acts.push(Act::AiRefresh);
    }
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08)));
    ui.add_space(6.0);

    let (shown, missing) = order(&q);
    if shown.is_empty() && missing.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, LINE * 2.0), Sense::hover());
        p.text(Pos2::new(r.left() + 2.0, r.center().y), Align2::LEFT_CENTER, "No providers are turned on (Settings, Panels).", ui_font(12.5), v.weak_text_color());
        return;
    }
    let sc = rivet_look::theme::status(ui.ctx());
    let line = |ui: &mut egui::Ui, text: &str, col: egui::Color32| {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, LINE), Sense::hover());
        let g = one_line(ui.painter(), text, ui_font(12.0), col, w - 4.0);
        ui.painter().galley(Pos2::new(r.left() + 2.0, r.center().y - g.size().y / 2.0), g, col);
    };
    for pv in &shown {
        // Name and plan; when it was read, on the right.
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, PHEAD), Sense::hover());
        let g = p.layout_no_wrap(name(pv.id).to_string(), ui_font(13.5), v.strong_text_color());
        let nw = g.size().x;
        p.galley(Pos2::new(r.left() + 2.0, r.center().y - g.size().y / 2.0), g, v.strong_text_color());
        if !pv.plan.is_empty() {
            let pg = p.layout_no_wrap(pv.plan.clone(), ui_font(11.0), v.text_color());
            let pr = Rect::from_min_size(Pos2::new(r.left() + nw + 10.0, r.center().y - 9.0), Vec2::new(pg.size().x + 12.0, 18.0));
            p.rect_filled(pr, 9.0, mix(v.panel_fill, v.text_color(), 0.09));
            p.galley(pr.center() - pg.size() / 2.0, pg, v.text_color());
        }
        if pv.state == State::Ok && pv.at > 0 {
            p.text(Pos2::new(r.right() - 2.0, r.center().y), Align2::RIGHT_CENTER, ago(pv.at), ui_font(11.5), v.weak_text_color());
        }
        match &pv.state {
            State::Ok => {
                for win in &pv.windows {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(w, WROW), Sense::hover());
                    let y = r.top() + 9.0;
                    let label = if win.reset > 0 { format!("{}  ·  resets {}", win.name, until(win.reset)) } else { win.name.clone() };
                    let g = one_line(&p, &label, ui_font(12.0), v.text_color(), w - 90.0);
                    p.galley(Pos2::new(r.left() + 2.0, y - g.size().y / 2.0), g, v.text_color());
                    let shown_pct = if left { 100.0 - win.used } else { win.used };
                    let col = if win.used >= 90.0 { sc.danger } else if win.used >= 75.0 { sc.warning } else { accent };
                    p.text(Pos2::new(r.right() - 2.0, y), Align2::RIGHT_CENTER, format!("{shown_pct:.0}% {}", if left { "left" } else { "used" }), ui_font(12.0), if win.used >= 75.0 { col } else { v.text_color() });
                    let bar = Rect::from_min_size(Pos2::new(r.left() + 2.0, r.top() + 20.0), Vec2::new(w - 4.0, 6.0));
                    p.rect_filled(bar, 3.0, mix(v.panel_fill, v.text_color(), 0.12));
                    let frac = (shown_pct / 100.0).clamp(0.0, 1.0);
                    if frac > 0.0 {
                        p.rect_filled(Rect::from_min_size(bar.min, Vec2::new((bar.width() * frac).max(6.0), bar.height())), 3.0, col);
                    }
                }
                if let Some(b) = &pv.balance {
                    line(ui, b.as_str(), v.text_color());
                }
                if pv.windows.is_empty() && pv.balance.is_none() {
                    line(ui, "No limits to show.", v.weak_text_color());
                }
            }
            State::Error(e) => line(ui, e.as_str(), sc.danger),
            State::Unknown => line(ui, "Reading…", v.weak_text_color()),
            State::Missing(_) => {}
        }
        ui.add_space(GAP);
    }
    if !missing.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, PHEAD), Sense::hover());
        p.text(Pos2::new(r.left() + 2.0, r.center().y), Align2::LEFT_CENTER, "Not set up", ui_font(12.0), v.weak_text_color());
        for pv in &missing {
            let why = if let State::Missing(w) = &pv.state { w.as_str() } else { "" };
            line(ui, &format!("{}: {why}", name(pv.id)), v.weak_text_color());
        }
    }
}
