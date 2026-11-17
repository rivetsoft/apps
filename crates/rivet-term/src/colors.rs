//! The terminal's colours: the background, text, cursor, selection and the sixteen ANSI
//! colours, from the app's theme or from one of the shared colour schemes.
//!
//! Both start from Rivet's own sixteen colours and lean them toward the theme (the way code
//! colours do): programs still get a red that reads as red, but it sits in the desktop's
//! colours. A scheme gives its own colours to the hues it has (its keywords, strings, numbers,
//! functions and types) and the rest are tinted toward it.

use alacritty_terminal::vte::ansi::{Color, NamedColor, Rgb};
use eframe::egui::Color32;
use rivet_look::palette::{luminance, mix};
use rivet_look::schemes::Scheme;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TermColors {
    pub bg: Color32,
    pub fg: Color32,
    pub cursor: Color32,
    pub selection: Color32,
    /// Black, red, green, yellow, blue, magenta, cyan, white, then their bright forms.
    pub ansi: [Color32; 16],
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Rivet's own sixteen, for dark backgrounds.
const BASE_DARK: [Color32; 16] = [
    rgb(0x2a2d33),
    rgb(0xe8706a),
    rgb(0x9fcf7c),
    rgb(0xe9c46a),
    rgb(0x6faee8),
    rgb(0xc792e0),
    rgb(0x5fc6c0),
    rgb(0xd5d8de),
    rgb(0x5d636e),
    rgb(0xf59490),
    rgb(0xb7e29a),
    rgb(0xf4d68c),
    rgb(0x92c4f2),
    rgb(0xdab1ee),
    rgb(0x86dcd6),
    rgb(0xf4f5f7),
];

/// The same for light backgrounds (deeper, so they read on white).
const BASE_LIGHT: [Color32; 16] = [
    rgb(0x2e3138),
    rgb(0xc0392b),
    rgb(0x3d8a28),
    rgb(0x9a6b00),
    rgb(0x1f64b8),
    rgb(0x8b3fb8),
    rgb(0x0f8079),
    rgb(0xa9adb5),
    rgb(0x6b7079),
    rgb(0xd9493b),
    rgb(0x4ea333),
    rgb(0xb78400),
    rgb(0x2f7bd6),
    rgb(0xa354d1),
    rgb(0x16978f),
    rgb(0xe6e8ec),
];

fn hue_of(c: Color32) -> Option<f32> {
    let (r, g, b) = (c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0);
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let d = max - min;
    if d < 0.12 {
        return None;
    }
    let h = if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    Some((h + 360.0) % 360.0)
}

fn hue_gap(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 360.0;
    d.min(360.0 - d)
}

/// Push a colour away from the background until it reads well.
fn readable(c: Color32, bg: Color32) -> Color32 {
    let dark = luminance(bg) < 0.5;
    let mut c = c;
    for _ in 0..4 {
        if (luminance(c) - luminance(bg)).abs() >= 0.3 {
            break;
        }
        c = mix(c, if dark { Color32::WHITE } else { Color32::BLACK }, 0.25);
    }
    c
}

impl TermColors {
    /// From the app's theme, the way Rivet Edit colours code by default: Rivet's default code
    /// palette tinted toward the accent (`rivet_look::highlight::colors`, shared by every Rivet
    /// app that shows code), so the terminal's colours match the editor's.
    pub fn from_theme(bg: Color32, fg: Color32, accent: Color32) -> TermColors {
        use rivet_look::builtin_icons::{derive_secondary, derive_tertiary};
        Self::from_theme_trio(bg, fg, [accent, derive_secondary(accent), derive_tertiary(accent)])
    }

    /// The same from the theme's three colours (the desktop palette's primary, secondary and
    /// tertiary when it has them). Each of the six hues then leans toward whichever theme colour
    /// is nearest to it, so a new wallpaper or accent shows in the terminal, while red stays
    /// reddish, blue bluish and so on.
    pub fn from_theme_trio(bg: Color32, fg: Color32, trio: [Color32; 3]) -> TermColors {
        let [p, s, t3] = trio;
        let code = rivet_look::highlight::colors(fg, bg, p, s, t3);
        let dark = luminance(bg) < 0.5;
        let mut t = Self::from_code(bg, fg, dark, &code);
        for i in 1..7 {
            let Some(h) = hue_of(t.ansi[i]) else { continue };
            // The nearest theme colour by hue, and how near: the nearer, the stronger the lean.
            let near = trio.iter().filter_map(|c| hue_of(*c).map(|th| (*c, hue_gap(h, th)))).min_by(|a, b| a.1.total_cmp(&b.1));
            if let Some((c, gap)) = near {
                // Near theme colours pull hard; far ones only tint (magenta mustn't turn red).
                let pull = 0.42 * (1.0 - (gap / 60.0).min(1.0)) + 0.08;
                t.ansi[i] = readable(mix(t.ansi[i], c, pull), bg);
                t.ansi[i + 8] = mix(t.ansi[i], if dark { Color32::WHITE } else { Color32::BLACK }, 0.18);
            }
        }
        t.cursor = p;
        t.selection = mix(bg, p, if dark { 0.38 } else { 0.3 });
        t
    }

    /// From a colour scheme: its background, text and syntax colours.
    pub fn from_scheme(s: &Scheme) -> TermColors {
        Self::from_code(s.bg(), s.fg(), s.dark, &s.colors())
    }

    /// The sixteen from a set of code colours: each ANSI hue takes the code colour nearest it
    /// (keywords, strings, numbers, functions, types); the hues it lacks come from Rivet's
    /// sixteen, leaning toward the code colours.
    fn from_code(bg: Color32, fg: Color32, dark: bool, c: &rivet_look::highlight::Colors) -> TermColors {
        let own = [c.keyword, c.string, c.number, c.function, c.type_];
        let base = if dark { BASE_DARK } else { BASE_LIGHT };
        // The scheme's average colour, to tint what it doesn't have.
        let avg = own.iter().fold((0u32, 0u32, 0u32), |a, c| (a.0 + c.r() as u32, a.1 + c.g() as u32, a.2 + c.b() as u32));
        let avg = Color32::from_rgb((avg.0 / 5) as u8, (avg.1 / 5) as u8, (avg.2 / 5) as u8);
        let mut ansi = base;
        // Red, yellow, green, cyan, blue, magenta: each takes the nearest code colour, the
        // closest pairs first (so a yellowish green still goes to green when yellow has gold).
        let slots = [(1usize, 0.0f32), (3, 55.0), (2, 115.0), (6, 180.0), (4, 220.0), (5, 290.0)];
        let mut pairs: Vec<(usize, usize, f32)> = Vec::new();
        for (si, (_, want)) in slots.iter().enumerate() {
            for (k, c) in own.iter().enumerate() {
                if let Some(h) = hue_of(*c) {
                    pairs.push((si, k, hue_gap(h, *want)));
                }
            }
        }
        pairs.sort_by(|a, b| a.2.total_cmp(&b.2));
        let (mut slot_got, mut colour_used) = ([None; 6], [false; 5]);
        for (si, k, gap) in pairs {
            // Red must stay red (errors), so only a near red takes its place.
            let limit = if si == 0 { 18.0 } else { 40.0 };
            if gap <= limit && slot_got[si].is_none() && !colour_used[k] {
                slot_got[si] = Some(own[k]);
                colour_used[k] = true;
            }
        }
        for (si, (i, _)) in slots.iter().enumerate() {
            ansi[*i] = slot_got[si].unwrap_or_else(|| readable(mix(base[*i], avg, 0.22), bg));
            ansi[i + 8] = mix(ansi[*i], if dark { Color32::WHITE } else { Color32::BLACK }, 0.18);
        }
        ansi[0] = mix(bg, fg, if dark { 0.14 } else { 0.85 });
        ansi[8] = c.comment;
        ansi[7] = mix(fg, bg, if dark { 0.1 } else { 0.4 });
        ansi[15] = if dark { mix(fg, Color32::WHITE, 0.3) } else { mix(bg, fg, 0.18) };
        let accent = c.function;
        TermColors { bg, fg, cursor: accent, selection: mix(bg, accent, if dark { 0.35 } else { 0.28 }), ansi }
    }

    /// A colour index 0..256: the sixteen, the 6×6×6 cube, then 24 greys.
    pub fn indexed(&self, i: u8) -> Color32 {
        match i {
            0..=15 => self.ansi[i as usize],
            16..=231 => {
                let n = i - 16;
                let level = |v: u8| if v == 0 { 0 } else { 55 + v * 40 };
                Color32::from_rgb(level(n / 36), level((n / 6) % 6), level(n % 6))
            }
            _ => {
                let v = 8 + (i - 232) * 10;
                Color32::from_rgb(v, v, v)
            }
        }
    }

    /// A cell's colour, with the colours a program set (OSC 4/10/11) taking over.
    pub fn resolve(&self, c: Color, set: &dyn Fn(usize) -> Option<Rgb>) -> Color32 {
        let from = |r: Rgb| Color32::from_rgb(r.r, r.g, r.b);
        match c {
            Color::Spec(r) => from(r),
            Color::Indexed(i) => set(i as usize).map(from).unwrap_or_else(|| self.indexed(i)),
            Color::Named(n) => {
                if let Some(r) = set(n as usize) {
                    return from(r);
                }
                let i = n as usize;
                match n {
                    NamedColor::Foreground | NamedColor::BrightForeground => self.fg,
                    NamedColor::Background => self.bg,
                    NamedColor::Cursor => self.cursor,
                    NamedColor::DimForeground => mix(self.fg, self.bg, 0.35),
                    _ if i < 16 => self.ansi[i],
                    // The dim ones: the normal colour, toward the background.
                    _ => {
                        let k = (i.saturating_sub(NamedColor::DimBlack as usize)) % 8;
                        mix(self.ansi[k], self.bg, 0.35)
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;


    #[test]
    fn colours_from_theme_and_scheme() {
        let t = TermColors::from_theme(rgb(0x1e1e1e), rgb(0xeeeeee), rgb(0xe66100));
        // Red stays red, green stays green.
        assert!(t.ansi[1].r() > t.ansi[1].g() && t.ansi[2].g() > t.ansi[2].r());
        // Red stays a true red even when the theme's orange numbers are near it.
        assert!(t.ansi[1].r() > 180 && t.ansi[1].g() < 140, "{:?}", t.ansi[1]);
        // The theme shows: a blue accent makes blue bluer than an orange one does, and the
        // colours differ between themes (they used to stay all but the same).
        let b = TermColors::from_theme(rgb(0x1e1e1e), rgb(0xeeeeee), rgb(0x3584e4));
        assert!(b.ansi[4].b() > t.ansi[4].b());
        let moved: u32 = (1..7).map(|i| (t.ansi[i].r() as i32 - b.ansi[i].r() as i32).unsigned_abs() + (t.ansi[i].b() as i32 - b.ansi[i].b() as i32).unsigned_abs()).sum();
        assert!(moved > 150, "{moved}");
        // Each hue keeps its family: magenta stays bluer than red.
        assert!(t.ansi[5].b() > t.ansi[1].b() + 40, "{:?} {:?}", t.ansi[5], t.ansi[1]);
        assert_eq!(t.indexed(16), Color32::from_rgb(0, 0, 0));
        assert_eq!(t.indexed(231), Color32::from_rgb(255, 255, 255));
        assert_eq!(t.indexed(232), Color32::from_rgb(8, 8, 8));
        let s = Scheme::by_name("Dracula").unwrap();
        let d = TermColors::from_scheme(s);
        assert_eq!(d.bg, s.bg());
        // Dracula's yellow string colour is its yellow.
        assert_eq!(d.ansi[3], rgb(0xf1fa8c));
        let none = |_| None;
        assert_eq!(d.resolve(Color::Named(NamedColor::Background), &none), s.bg());
        assert_eq!(d.resolve(Color::Spec(Rgb { r: 1, g: 2, b: 3 }), &none), Color32::from_rgb(1, 2, 3));
    }
}
