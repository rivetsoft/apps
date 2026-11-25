//! Drawing the weather. Everything here is made of parts that can go anywhere: [`icon`] draws a
//! sky at any size, and [`view`] stacks whichever [`Part`]s are asked for into a rectangle, with
//! [`height`] saying how tall that will be. The desktop widget, the panel item's popup and
//! anything else that wants the weather (another widget, a sub panel) are different choices of
//! parts over the same reading.

use eframe::egui::{self, Align2, Color32, Pos2, Rect, Sense, Shape, Stroke, Vec2};
use rivet_look::palette::mix;

use crate::glyph;
use crate::quick::tool_button;
use crate::ui::{Act, Kit, Model, one_line, ui_font};
use crate::weather::{Sky, Units, Weather, words};

/// A piece of the weather, as tall as [`Part::height`] and as wide as it's given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Part {
    /// The place's name, on one line.
    Place,
    /// Now: the sky, the temperature, and in words with what it feels like.
    Now,
    /// One line: wind, humidity and today's chance of rain.
    Details,
    /// The next hours, side by side (how many).
    Hours(usize),
    /// The coming days, a row each (how many, today first).
    Days(usize),
}

const GAP: f32 = 10.0;
const DAY_ROW: f32 = 28.0;
const MESSAGE: f32 = 44.0;

impl Part {
    fn height(self, w: &Weather) -> f32 {
        match self {
            Part::Place => 18.0,
            Part::Now => 56.0,
            Part::Details => 18.0,
            Part::Hours(_) if w.hours.is_empty() => 0.0,
            Part::Hours(_) => 64.0,
            Part::Days(n) => n.min(w.days.len()) as f32 * DAY_ROW,
        }
    }
}

/// The colours a view is drawn in.
#[derive(Clone, Copy)]
pub struct Tones {
    pub strong: Color32,
    pub text: Color32,
    pub weak: Color32,
    /// The sun, and lightning.
    pub accent: Color32,
    /// The hairlines between parts.
    pub line: Color32,
}

impl Tones {
    /// For drawing on a card or popup in the theme's colours.
    pub fn of(v: &egui::Visuals, accent: Color32) -> Tones {
        Tones { strong: v.strong_text_color(), text: v.text_color(), weak: v.weak_text_color(), accent, line: mix(v.panel_fill, v.text_color(), 0.08) }
    }
}

/// How tall [`view`] draws `parts` (with no reading yet: one line saying why).
pub fn height(w: &Weather, parts: &[Part]) -> f32 {
    if w.message().is_some() {
        return MESSAGE;
    }
    let hs: Vec<f32> = parts.iter().map(|p| p.height(w)).filter(|h| *h > 0.0).collect();
    hs.iter().sum::<f32>() + GAP * hs.len().saturating_sub(1) as f32
}

/// The weather's `parts`, stacked from the top of `r` and as wide as it.
pub fn view(p: &egui::Painter, r: Rect, w: &Weather, units: Units, parts: &[Part], t: Tones) {
    if let Some(msg) = w.message() {
        let g = one_line(p, msg, ui_font(12.5), t.weak, r.width());
        p.galley(Pos2::new(r.left(), r.top() + (MESSAGE - g.size().y) / 2.0), g, t.weak);
        return;
    }
    let Some(now) = &w.now else { return };
    let mut y = r.top();
    for part in parts {
        let h = part.height(w);
        if h <= 0.0 {
            continue;
        }
        let row = Rect::from_min_size(Pos2::new(r.left(), y), Vec2::new(r.width(), h));
        match *part {
            Part::Place => {
                let g = one_line(p, &w.place, ui_font(12.0), t.weak, row.width());
                p.galley(Pos2::new(row.left(), row.center().y - g.size().y / 2.0), g, t.weak);
            }
            Part::Now => {
                icon(p, Pos2::new(row.left() + 24.0, row.center().y), 44.0, Sky::of(now.code), now.day, t.text, t.accent);
                let g = p.layout_no_wrap(units.temp(now.temp), egui::FontId::proportional(38.0), t.strong);
                let tx = row.left() + 58.0;
                p.galley(Pos2::new(tx, row.center().y - g.size().y / 2.0), g.clone(), t.strong);
                // Beside the temperature: what it is, and what it feels like.
                let left = tx + g.size().x + 14.0;
                let room = row.right() - left;
                let a = one_line(p, words(now.code), ui_font(13.5), t.text, room);
                p.galley(Pos2::new(left, row.center().y - a.size().y - 1.0), a, t.text);
                let feels = if now.feels.is_finite() { format!("Feels like {}", units.temp(now.feels)) } else { String::new() };
                let b = one_line(p, &feels, ui_font(12.0), t.weak, room);
                p.galley(Pos2::new(left, row.center().y + 2.0), b, t.weak);
            }
            Part::Details => {
                let mut bits = vec![format!("Wind {}", units.wind(now.wind)), format!("Humidity {}%", now.humidity)];
                if let Some(d) = w.days.first() {
                    bits.push(format!("Rain {}%", d.wet));
                }
                let g = one_line(p, &bits.join("  ·  "), ui_font(12.0), t.weak, row.width());
                p.galley(Pos2::new(row.left(), row.center().y - g.size().y / 2.0), g, t.weak);
            }
            Part::Hours(n) => {
                let n = n.min(w.hours.len()).max(1);
                let each = row.width() / n as f32;
                for (i, h) in w.hours.iter().take(n).enumerate() {
                    let cx = row.left() + each * (i as f32 + 0.5);
                    p.text(Pos2::new(cx, row.top() + 8.0), Align2::CENTER_CENTER, if i == 0 { "Now".to_string() } else { format!("{:02}", h.hour) }, ui_font(11.5), t.weak);
                    icon(p, Pos2::new(cx, row.top() + 31.0), 22.0, Sky::of(h.code), h.day, t.text, t.accent);
                    p.text(Pos2::new(cx, row.bottom() - 8.0), Align2::CENTER_CENTER, units.temp(h.temp), ui_font(12.5), t.text);
                }
            }
            Part::Days(n) => {
                for (i, d) in w.days.iter().take(n).enumerate() {
                    let cy = row.top() + DAY_ROW * (i as f32 + 0.5);
                    p.text(Pos2::new(row.left(), cy), Align2::LEFT_CENTER, d.name(i == 0), ui_font(12.5), t.text);
                    icon(p, Pos2::new(row.left() + 66.0, cy), 20.0, Sky::of(d.code), true, t.text, t.accent);
                    if d.wet >= 20 {
                        p.text(Pos2::new(row.left() + 84.0, cy), Align2::LEFT_CENTER, format!("{}%", d.wet), ui_font(11.5), t.weak);
                    }
                    p.text(Pos2::new(row.right(), cy), Align2::RIGHT_CENTER, units.temp(d.high), ui_font(12.5), t.text);
                    p.text(Pos2::new(row.right() - 40.0, cy), Align2::RIGHT_CENTER, units.temp(d.low), ui_font(12.5), t.weak);
                }
            }
        }
        y += h + GAP;
        if y < r.bottom() - 1.0 && !matches!(part, Part::Place) {
            let ly = y - GAP / 2.0;
            p.line_segment([Pos2::new(r.left(), ly), Pos2::new(r.right(), ly)], Stroke::new(1.0, t.line));
        }
    }
}

// ------------------------------------------------------------------ the sky

/// A sky centred on `c` in a square of `size`: clouds and what falls from them in `col`, the
/// sun and lightning in `sun`; at night the moon in `col`.
pub fn icon(p: &egui::Painter, c: Pos2, size: f32, sky: Sky, day: bool, col: Color32, sun: Color32) {
    let u = size / 24.0;
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * u;
    let light = |c: Pos2, k: f32| if day { sun_at(p, c, u * k, sun) } else { moon_at(p, c, u * k, col) };
    let fall = Stroke::new((1.6 * u).max(1.0), col);
    match sky {
        Sky::Clear => light(c, 1.0),
        Sky::PartlyCloudy => {
            light(at(4.5, -4.0), 0.68);
            cloud(p, at(-1.5, 2.5), u * 0.9, col);
        }
        Sky::Cloudy => cloud(p, at(0.0, -0.5), u * 1.2, col),
        Sky::Fog => {
            cloud(p, at(0.0, -3.5), u, col);
            for (y, half) in [(6.0, 8.0), (9.5, 5.5)] {
                p.line_segment([at(-half, y), at(half, y)], fall);
            }
        }
        Sky::Drizzle | Sky::Rain => {
            cloud(p, at(0.0, -3.5), u, col);
            let drops: &[f32] = if sky == Sky::Rain { &[-4.5, 0.5, 5.5] } else { &[-2.0, 3.5] };
            for x in drops {
                p.line_segment([at(*x, 5.5), at(*x - 1.8, 10.0)], fall);
            }
        }
        Sky::Snow => {
            cloud(p, at(0.0, -3.5), u, col);
            for (x, y) in [(-4.5, 6.5), (0.5, 9.0), (5.0, 6.5)] {
                p.circle_filled(at(x, y), (1.4 * u).max(1.0), col);
            }
        }
        Sky::Thunder => {
            cloud(p, at(0.0, -3.5), u, col);
            p.add(Shape::line(vec![at(2.0, 4.0), at(-1.6, 8.0), at(1.6, 8.0), at(-1.0, 11.8)], Stroke::new((1.8 * u).max(1.0), sun)));
        }
    }
}

fn sun_at(p: &egui::Painter, c: Pos2, u: f32, col: Color32) {
    p.circle_filled(c, 4.6 * u, col);
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::FRAC_PI_4;
        let d = Vec2::new(a.cos(), a.sin());
        p.line_segment([c + d * 7.0 * u, c + d * 9.6 * u], Stroke::new((1.7 * u).max(1.0), col));
    }
}

/// A crescent: an outer arc, and a smaller one closing its open side.
fn moon_at(p: &egui::Painter, c: Pos2, u: f32, col: Color32) {
    use std::f32::consts::PI;
    let s = Stroke::new((1.7 * u).max(1.0), col);
    let arc = |c: Pos2, r: f32, from: f32, to: f32| {
        let pts: Vec<Pos2> = (0..=18).map(|i| from + (to - from) * i as f32 / 18.0).map(|a| c + Vec2::new(a.cos(), a.sin()) * r).collect();
        p.add(Shape::line(pts, s));
    };
    arc(c, 8.0 * u, PI * 0.15, PI * 1.85);
    arc(c + Vec2::new(4.1, -2.7) * u, 6.0 * u, PI * 0.42, PI * 1.25);
}

/// A cloud, filled, centred on `c`: three puffs over a flat bottom.
fn cloud(p: &egui::Painter, c: Pos2, u: f32, col: Color32) {
    let at = |x: f32, y: f32| c + Vec2::new(x, y) * u;
    p.circle_filled(at(-4.6, 1.6), 4.1 * u, col);
    p.circle_filled(at(0.4, -1.8), 5.6 * u, col);
    p.circle_filled(at(5.2, 1.9), 3.8 * u, col);
    p.rect_filled(Rect::from_min_max(at(-4.6, 0.5), at(5.2, 5.7)), 0.0, col);
}

// ------------------------------------------------------------------ the popup

const HEAD: f32 = 46.0;
const FOOT: f32 = 24.0;
const POPUP: [Part; 4] = [Part::Now, Part::Details, Part::Hours(6), Part::Days(7)];

pub fn popup_height(m: &Model) -> f32 {
    28.0 + HEAD + 8.0 + height(&m.extra.weather, &POPUP) + FOOT
}

/// The panel item's popup: the place, now, the next hours and the week.
pub fn popup(ui: &mut egui::Ui, m: &mut Model, kit: &mut Kit) {
    let v = ui.visuals().clone();
    let t = Tones::of(&v, kit.accent(m.s));
    let w = m.extra.weather.clone();
    let wide = ui.available_width();
    ui.spacing_mut().item_spacing.y = 0.0;

    let (r, _) = ui.allocate_exact_size(Vec2::new(wide, HEAD), Sense::hover());
    let p = ui.painter().clone();
    p.text(Pos2::new(r.left() + 2.0, r.top() + 13.0), Align2::LEFT_CENTER, "Weather", ui_font(14.5), t.strong);
    let g = one_line(&p, if w.place.is_empty() { "No place chosen" } else { &w.place }, ui_font(12.0), t.weak, wide - 60.0);
    p.galley(Pos2::new(r.left() + 2.0, r.top() + 26.0), g, t.weak);
    let tool = Rect::from_center_size(Pos2::new(r.right() - 16.0, r.top() + 18.0), Vec2::splat(30.0));
    if tool_button(ui, tool, "weather-refresh", "Refresh now", &|p, c, col| glyph::refresh(p, c, col)) {
        m.acts.push(Act::WeatherRefresh);
    }
    p.line_segment([Pos2::new(r.left(), r.bottom() - 1.0), Pos2::new(r.right(), r.bottom() - 1.0)], Stroke::new(1.0, t.line));
    ui.add_space(8.0);

    let (r, _) = ui.allocate_exact_size(Vec2::new(wide, height(&w, &POPUP)), Sense::hover());
    view(&p, r.shrink2(Vec2::new(2.0, 0.0)), &w, m.s.weather_units, &POPUP, t);

    let (r, _) = ui.allocate_exact_size(Vec2::new(wide, FOOT), Sense::hover());
    let foot = match (&w.state, w.at) {
        (_, 0) => "Weather from Open-Meteo".to_string(),
        (crate::weather::State::Failed(_), at) => format!("Couldn't update · last read {}", crate::ai::ago(at)),
        (_, at) => format!("Read {} · Open-Meteo", crate::ai::ago(at)),
    };
    p.text(Pos2::new(r.left() + 2.0, r.bottom() - 7.0), Align2::LEFT_CENTER, foot, ui_font(11.5), t.weak);
}

// ------------------------------------------------------------------ the desktop widget

const CARD_PAD: f32 = 16.0;
const CARD: [Part; 3] = [Part::Place, Part::Now, Part::Days(4)];

/// How tall the desktop widget's card is.
pub fn card_height(w: &Weather) -> f32 {
    height(w, &CARD) + CARD_PAD * 2.0
}

/// The desktop widget, inside its card `r`.
pub fn card(p: &egui::Painter, r: Rect, w: &Weather, units: Units, t: Tones) {
    view(p, r.shrink(CARD_PAD), w, units, &CARD, t);
}
