//! The AI Usage popup, opened from the panel's AI item: each provider's plan and limits, with a
//! bar for each and when it resets, and whether Claude Code is running.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Align2, Pos2, Rect, Sense, Stroke, Vec2};
use rivet_look::palette::mix;

use crate::ai::{Provider, State, ago, name, pin_key, until};
use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, one_line, ui_font};

/// The providers' logos (see data/ai/README.md).
fn logo_svg(id: &str) -> &'static str {
    match id {
        "claude" => include_str!("../data/ai/claude.svg"),
        "codex" => include_str!("../data/ai/codex.svg"),
        "grok" => include_str!("../data/ai/grok.svg"),
        "openrouter" => include_str!("../data/ai/openrouter.svg"),
        "deepseek" => include_str!("../data/ai/deepseek.svg"),
        "hermes" => include_str!("../data/ai/hermes.svg"),
        _ => include_str!("../data/ai/claude.svg"),
    }
}

/// A provider's logo in `r` (square). Logos without colours of their own take `col`. Each is
/// made once per size and colour, at the screen's scale.
pub fn logo(ui: &egui::Ui, p: &egui::Painter, id: &str, r: Rect, col: egui::Color32) {
    let svg = logo_svg(id);
    let mono = svg.contains("currentColor");
    let px = (r.width() * ui.ctx().pixels_per_point()).round().max(1.0) as u32;
    let key = egui::Id::unique(("ai-logo", id, px, if mono { col.to_array() } else { [0; 4] }));
    let tex = ui.ctx().data(|d| d.get_temp::<egui::TextureHandle>(key)).or_else(|| {
        let data = if mono { svg.replace("currentColor", &format!("#{:02x}{:02x}{:02x}", col.r(), col.g(), col.b())) } else { svg.to_string() };
        let img = rivet_look::icons::render_svg(data.as_bytes(), px)?;
        let t = ui.ctx().load_texture(format!("ai-logo-{id}-{px}"), img, egui::TextureOptions::LINEAR);
        ui.ctx().data_mut(|d| d.insert_temp(key, t.clone()));
        Some(t)
    });
    if let Some(t) = tex {
        let tint = if mono { egui::Color32::WHITE.gamma_multiply(col.a() as f32 / 255.0) } else { egui::Color32::WHITE };
        p.image(t.id(), r, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), tint);
    }
}

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
            State::Ok => p.windows.len() as f32 * WROW + if p.balance.is_some() { LINE } else { 0.0 } + p.facts.len().div_ceil(2) as f32 * LINE + if p.windows.is_empty() && p.balance.is_none() && p.facts.is_empty() { LINE } else { 0.0 },
            _ => LINE,
        }
        + GAP
}

pub fn height(m: &Model) -> f32 {
    let (shown, missing) = order(&m.extra.ai);
    let mut h = 28.0 + HEAD + 6.0 + shown.iter().map(|p| block(p)).sum::<f32>();
    // Providers that aren't set up aren't listed: only a line when none is.
    if shown.is_empty() {
        h += LINE * 2.0;
    }
    let _ = missing;
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
    if tool_button(ui, tool, "ai-refresh", "Refresh now", &|p, c, col| glyph::refresh(p, c, col)) {
        m.acts.push(Act::AiRefresh);
    }
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, mix(v.panel_fill, v.text_color(), 0.08)));
    ui.add_space(6.0);

    let (shown, missing) = order(&q);
    if shown.is_empty() {
        let (r, _) = ui.allocate_exact_size(Vec2::new(w, LINE * 2.0), Sense::hover());
        let text = if missing.is_empty() { "No providers turned on. Choose them in Settings › Panels." } else { "None of the providers turned on is set up on this computer." };
        p.text(Pos2::new(r.left() + 2.0, r.center().y), Align2::LEFT_CENTER, text, ui_font(12.5), v.weak_text_color());
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
        logo(ui, &p, pv.id, Rect::from_center_size(Pos2::new(r.left() + 10.0, r.center().y), Vec2::splat(16.0)), v.strong_text_color());
        let g = p.layout_no_wrap(name(pv.id).to_string(), ui_font(13.5), v.strong_text_color());
        let nw = g.size().x + 24.0;
        p.galley(Pos2::new(r.left() + 26.0, r.center().y - g.size().y / 2.0), g, v.strong_text_color());
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
                    // Pin to the panel: on the right, shown while pinned or while the row is pointed at.
                    let key = pin_key(pv.id, &win.name);
                    let pinned = m.s.ai_pins.contains(&key);
                    let pr = Rect::from_center_size(Pos2::new(r.right() - 11.0, r.center().y - 1.0), Vec2::splat(22.0));
                    let presp = ui.interact(pr, egui::Id::unique(("ai-pin", &key)), Sense::click()).role(Role::Button, "Pin to the panel").toggled(pinned);
                    if presp.hovered() {
                        p.rect_filled(pr, 5.0, mix(v.panel_fill, v.text_color(), 0.1));
                    }
                    if pinned || ui.rect_contains_pointer(r) {
                        glyph::pin(&p, pr.center(), if pinned { accent } else if presp.hovered() { v.text_color() } else { v.weak_text_color() }, pinned);
                    }
                    if presp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(if pinned { "Take off the panel" } else { "Pin to the panel" }).clicked() {
                        m.acts.push(Act::AiPin(key));
                    }
                    let r = Rect::from_min_max(r.min, Pos2::new(r.right() - 28.0, r.bottom()));
                    let w = r.width() + 2.0;
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
                // Figures that aren't limits, two to a row: the label faint, the value after it.
                for pair in pv.facts.chunks(2) {
                    let (r, _) = ui.allocate_exact_size(Vec2::new(w, LINE), Sense::hover());
                    for (i, (k, val)) in pair.iter().enumerate() {
                        let x = r.left() + 2.0 + i as f32 * (w / 2.0);
                        let kg = p.layout_no_wrap(k.clone(), ui_font(12.0), v.weak_text_color());
                        let kw = kg.size().x;
                        p.galley(Pos2::new(x, r.center().y - kg.size().y / 2.0), kg, v.weak_text_color());
                        let vg = one_line(&p, val, ui_font(12.0), v.text_color(), w / 2.0 - kw - 14.0);
                        p.galley(Pos2::new(x + kw + 6.0, r.center().y - vg.size().y / 2.0), vg, v.text_color());
                    }
                }
                if pv.windows.is_empty() && pv.balance.is_none() && pv.facts.is_empty() {
                    line(ui, if pv.id == crate::ai::HERMES.0 { "No sessions in the last 7 days." } else { "No limits to show." }, v.weak_text_color());
                }
            }
            State::Error(e) => line(ui, e.as_str(), sc.danger),
            State::Unknown => line(ui, "Reading…", v.weak_text_color()),
            State::Missing(_) => {}
        }
        ui.add_space(GAP);
    }
}
