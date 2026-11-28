//! The power menu, in the style chosen for the Power item: a list (Menu), a row of symbols
//! under the item (Row), or large tiles in the middle of the screen (Grid). Lock and suspend
//! happen at once; the rest ask for a second click, unless that's turned off.

use rivet_look::a11y::{Access, Role};
use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rivet_look::palette::mix;
use rivet_look::theme::{r_lg, r_md};

use crate::config::{PowerStyle, Settings};
use crate::sys::Power;
use crate::ui::{Act, Model, hover_fill, ui_font};

/// Each action: its name in the settings file, its label, and whether it asks first.
pub const ACTIONS: [(Power, &str, &str, bool); 5] = [(Power::Lock, "lock", "Lock", false), (Power::Suspend, "suspend", "Suspend", false), (Power::LogOut, "logout", "Log Out", true), (Power::Restart, "restart", "Restart", true), (Power::ShutDown, "shutdown", "Shut Down", true)];

const PAD: f32 = 14.0;
const ROW_H: f32 = 38.0;
const TILE: f32 = 46.0;
const BIG: Vec2 = Vec2::new(116.0, 104.0);
const GAP: f32 = 10.0;
const HEAD: f32 = 30.0;
const CAPTION: f32 = 24.0;

/// The actions that show (all of them, should every one be hidden).
fn shown(s: &Settings) -> Vec<(Power, &'static str, bool)> {
    let all = |hide: &[String]| ACTIONS.iter().filter(|a| !hide.iter().any(|h| h == a.1)).map(|a| (a.0, a.2, a.3)).collect::<Vec<_>>();
    let v = all(&s.power_hide);
    if v.is_empty() { all(&[]) } else { v }
}

fn cols(n: usize) -> usize {
    if n == 4 { 2 } else { n.min(3) }
}

/// Whether the menu opens in the middle of the screen, over a dim.
pub fn centred(s: &Settings) -> bool {
    s.power_style == PowerStyle::Grid
}

pub fn size(s: &Settings) -> Vec2 {
    let n = shown(s).len();
    match s.power_style {
        PowerStyle::Menu => Vec2::new(240.0, n as f32 * (ROW_H + 4.0) - 4.0 + PAD * 2.0),
        PowerStyle::Row => Vec2::new(row_w(n) + PAD * 2.0, TILE + CAPTION + PAD * 2.0 - 4.0),
        PowerStyle::Grid => {
            let (c, r) = (cols(n), n.div_ceil(cols(n)));
            Vec2::new(c as f32 * (BIG.x + GAP) - GAP + PAD * 2.0, HEAD + r as f32 * (BIG.y + GAP) - GAP + PAD * 2.0)
        }
    }
}

/// The row's inner width: its tiles, and room for the line under them.
fn row_w(n: usize) -> f32 {
    (n as f32 * (TILE + 6.0) - 6.0).max(164.0)
}

pub fn menu(ui: &mut egui::Ui, confirm: &mut Option<Power>, m: &mut Model) {
    let v = ui.visuals().clone();
    let danger = rivet_look::theme::status(ui.ctx()).danger;
    let acts = shown(m.s);
    let area = ui.max_rect();
    let p = ui.painter().clone();
    let (style, ask, name) = (m.s.power_style, m.s.power_confirm, who(m));
    let mut click = |ui: &mut egui::Ui, r: Rect, pw: Power, label: &str, sure: bool| -> (bool, bool) {
        let resp = ui.interact(r, egui::Id::unique(("power", label)), Sense::click()).role(Role::Button, label);
        let hot = resp.hovered();
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if sure && ask && *confirm != Some(pw) {
                *confirm = Some(pw);
            } else {
                m.acts.push(Act::Power(pw));
            }
        }
        (hot, *confirm == Some(pw))
    };
    match style {
        PowerStyle::Menu => {
            for (i, &(pw, label, sure)) in acts.iter().enumerate() {
                let r = Rect::from_min_size(Pos2::new(area.left(), area.top() + i as f32 * (ROW_H + 4.0)), Vec2::new(area.width(), ROW_H));
                let (hot, asking) = click(ui, r, pw, label, sure);
                if asking {
                    p.rect_filled(r, r_md(), danger.gamma_multiply(0.2));
                } else if hot {
                    hover_fill(&p, r, &v);
                }
                let col = if asking { danger } else { v.text_color() };
                icon(&p, Pos2::new(r.left() + 20.0, r.center().y), 1.0, pw, col);
                let text = if asking { format!("{label}? Click again") } else { label.to_string() };
                p.text(Pos2::new(r.left() + 42.0, r.center().y), Align2::LEFT_CENTER, text, ui_font(14.0), col);
            }
        }
        PowerStyle::Row => {
            let n = acts.len() as f32;
            let mut x = (area.center().x - (n * (TILE + 6.0) - 6.0) / 2.0).round();
            // Under the tiles: what the pointer is on, or what's being asked.
            let mut caption: Option<(String, Color32)> = None;
            for &(pw, label, sure) in &acts {
                let r = Rect::from_min_size(Pos2::new(x, area.top()), Vec2::splat(TILE));
                x += TILE + 6.0;
                let (hot, asking) = click(ui, r, pw, label, sure);
                let fill = if asking { danger.gamma_multiply(0.22) } else { mix(v.panel_fill, v.text_color(), if hot { 0.14 } else { 0.06 }) };
                p.rect_filled(r, r_md(), fill);
                icon(&p, r.center(), 1.15, pw, if asking { danger } else { v.text_color() });
                if asking {
                    caption = Some((format!("{label}? Click again"), danger));
                } else if hot && caption.is_none() {
                    caption = Some((label.to_string(), v.text_color()));
                }
            }
            let (text, col) = caption.unwrap_or((name, v.weak_text_color()));
            p.text(Pos2::new(area.center().x, area.top() + TILE + CAPTION / 2.0 + 2.0), Align2::CENTER_CENTER, text, ui_font(12.5), col);
        }
        PowerStyle::Grid => {
            // Who's signed in, and how long the computer has been up.
            p.text(Pos2::new(area.left() + 4.0, area.top() + 10.0), Align2::LEFT_CENTER, name, ui_font(14.0), v.strong_text_color());
            let up = crate::sys::uptime();
            p.text(Pos2::new(area.right() - 4.0, area.top() + 10.0), Align2::RIGHT_CENTER, format!("Up {} h {} min", up / 3600, up / 60 % 60), ui_font(12.5), v.weak_text_color());
            let c = cols(acts.len());
            for (i, &(pw, label, sure)) in acts.iter().enumerate() {
                // A last row that isn't full sits in the middle.
                let (row, in_row) = (i / c, (acts.len() - i / c * c).min(c));
                let left = (area.center().x - (in_row as f32 * (BIG.x + GAP) - GAP) / 2.0).round();
                let r = Rect::from_min_size(Pos2::new(left + (i % c) as f32 * (BIG.x + GAP), area.top() + HEAD + row as f32 * (BIG.y + GAP)), BIG);
                let (hot, asking) = click(ui, r, pw, label, sure);
                let fill = if asking { danger.gamma_multiply(0.22) } else { mix(v.panel_fill, v.text_color(), if hot { 0.14 } else { 0.06 }) };
                p.rect_filled(r, r_lg(), fill);
                let col = if asking { danger } else { v.text_color() };
                icon(&p, Pos2::new(r.center().x, r.top() + 42.0), 2.1, pw, col);
                p.text(Pos2::new(r.center().x, r.bottom() - 22.0), Align2::CENTER_CENTER, if asking { "Click again" } else { label }, ui_font(13.5), col);
            }
        }
    }
}

fn who(m: &Model) -> String {
    if m.extra.user.is_empty() { "Power".to_string() } else { m.extra.user.clone() }
}

/// An action's symbol around `c`, `k` times the panel's size.
pub fn icon(p: &egui::Painter, c: Pos2, k: f32, pw: Power, col: Color32) {
    use std::f32::consts::PI;
    let s = Stroke::new(1.1 + 0.55 * k, col);
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * k;
    let arc = |cx: f32, cy: f32, r: f32, from: f32, to: f32| {
        let pts: Vec<Pos2> = (0..=20).map(|i| from + (to - from) * i as f32 / 20.0).map(|a| at(cx + a.cos() * r, cy + a.sin() * r)).collect();
        p.add(Shape::line(pts, s));
    };
    match pw {
        Power::Lock => {
            p.rect_stroke(Rect::from_min_max(at(-5.5, -1.0), at(5.5, 7.0)), 2.0 * k, s, egui::StrokeKind::Middle);
            arc(0.0, -3.0, 3.5, PI, 2.0 * PI);
            p.line_segment([at(-3.5, -3.0), at(-3.5, -1.0)], s);
            p.line_segment([at(3.5, -3.0), at(3.5, -1.0)], s);
            p.circle_filled(at(0.0, 3.0), 1.3 * k, col);
        }
        Power::Suspend => {
            arc(0.0, 0.0, 7.0, PI * 0.15, PI * 1.85);
            arc(3.6, -2.4, 5.2, PI * 0.42, PI * 1.25);
        }
        Power::LogOut => {
            p.add(Shape::line(vec![at(0.5, -7.0), at(-6.5, -7.0), at(-6.5, 7.0), at(0.5, 7.0)], s));
            p.line_segment([at(-2.0, 0.0), at(7.5, 0.0)], s);
            p.add(Shape::line(vec![at(4.0, -3.5), at(7.5, 0.0), at(4.0, 3.5)], s));
        }
        Power::Restart => {
            arc(0.0, 0.0, 6.5, -PI * 0.25, PI * 1.45);
            let tip = Vec2::new((-PI * 0.25).cos(), (-PI * 0.25).sin()) * 6.5;
            p.add(Shape::convex_polygon(vec![at(tip.x - 4.0, tip.y - 2.4), at(tip.x + 1.8, tip.y - 4.0), at(tip.x + 0.8, tip.y + 2.4)], col, Stroke::NONE));
        }
        Power::ShutDown => {
            arc(0.0, 1.0, 6.5, -PI / 2.0 + 0.7, 3.0 * PI / 2.0 - 0.7);
            p.line_segment([at(0.0, -7.0), at(0.0, 0.5)], s);
        }
    }
}
