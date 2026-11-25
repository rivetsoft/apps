//! Colors and the arithmetic on them: reading a color written any of the usual ways, writing it
//! back out, and the colors that go with it. Harmonies turn the hue in OKLCH and shades mix in
//! OKLab, the spaces made so that equal steps look equal; HSL's steps don't (its yellow is far
//! lighter than its blue at the same lightness).

/// A color on screen: red, green and blue, 0 to 255.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rgb(pub u8, pub u8, pub u8);

/// The ways a color can be written out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Hex,
    Rgb,
    Hsl,
    Oklch,
}

impl Format {
    pub const ALL: [Format; 4] = [Format::Hex, Format::Rgb, Format::Hsl, Format::Oklch];

    pub fn name(self) -> &'static str {
        match self {
            Format::Hex => "Hex",
            Format::Rgb => "RGB",
            Format::Hsl => "HSL",
            Format::Oklch => "OKLCH",
        }
    }

    /// Its word in the settings file.
    pub fn id(self) -> &'static str {
        match self {
            Format::Hex => "hex",
            Format::Rgb => "rgb",
            Format::Hsl => "hsl",
            Format::Oklch => "oklch",
        }
    }

    pub fn from_id(id: &str) -> Option<Format> {
        Format::ALL.into_iter().find(|f| f.id() == id)
    }
}

/// The colors that go with one, each a turn of the hue.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Harmony {
    Complementary,
    Analogous,
    Triadic,
    Split,
    Square,
}

impl Harmony {
    pub const ALL: [Harmony; 5] = [Harmony::Complementary, Harmony::Analogous, Harmony::Triadic, Harmony::Split, Harmony::Square];

    pub fn name(self) -> &'static str {
        match self {
            Harmony::Complementary => "Complementary",
            Harmony::Analogous => "Analogous",
            Harmony::Triadic => "Triadic",
            Harmony::Split => "Split complementary",
            Harmony::Square => "Square",
        }
    }

    /// The turns, in degrees, of the colors besides the one it starts from.
    fn turns(self) -> &'static [f32] {
        match self {
            Harmony::Complementary => &[180.0],
            Harmony::Analogous => &[-30.0, 30.0],
            Harmony::Triadic => &[120.0, 240.0],
            Harmony::Split => &[150.0, 210.0],
            Harmony::Square => &[90.0, 180.0, 270.0],
        }
    }
}

fn to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) }
}

fn from_linear(c: f32) -> f32 {
    if c <= 0.003_130_8 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

fn byte(c: f32) -> u8 {
    (c.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// OKLab from linear red, green and blue (Björn Ottosson's matrices).
fn oklab_of(r: f32, g: f32, b: f32) -> [f32; 3] {
    let l = (0.412_221_46 * r + 0.536_332_55 * g + 0.051_445_995 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    [0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s, 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s, 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s]
}

/// Linear red, green and blue from OKLab; outside 0 to 1 when the screen can't show it.
fn linear_of(lab: [f32; 3]) -> [f32; 3] {
    let [ll, a, b] = lab;
    let l = (ll + 0.396_337_78 * a + 0.215_803_76 * b).powi(3);
    let m = (ll - 0.105_561_346 * a - 0.063_854_17 * b).powi(3);
    let s = (ll - 0.089_484_18 * a - 1.291_485_5 * b).powi(3);
    [4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s, -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s, -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s]
}

impl Rgb {
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Hue in degrees, saturation and lightness 0 to 1.
    pub fn hsl(self) -> [f32; 3] {
        let (r, g, b) = (self.0 as f32 / 255.0, self.1 as f32 / 255.0, self.2 as f32 / 255.0);
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let l = (max + min) / 2.0;
        let d = max - min;
        if d == 0.0 {
            return [0.0, 0.0, l];
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        let h = if max == r {
            ((g - b) / d).rem_euclid(6.0)
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        [h * 60.0, s, l]
    }

    pub fn from_hsl([h, s, l]: [f32; 3]) -> Rgb {
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h = h.rem_euclid(360.0) / 60.0;
        let x = c * (1.0 - (h % 2.0 - 1.0).abs());
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        let m = l - c / 2.0;
        Rgb(byte(r + m), byte(g + m), byte(b + m))
    }

    pub fn oklab(self) -> [f32; 3] {
        oklab_of(to_linear(self.0), to_linear(self.1), to_linear(self.2))
    }

    /// The nearest color the screen can show to an OKLab one.
    pub fn from_oklab(lab: [f32; 3]) -> Rgb {
        let [r, g, b] = linear_of(lab);
        Rgb(byte(from_linear(r)), byte(from_linear(g)), byte(from_linear(b)))
    }

    /// Lightness 0 to 1, chroma (0 for a grey, about 0.37 at most), hue in degrees.
    pub fn oklch(self) -> [f32; 3] {
        let [l, a, b] = self.oklab();
        let c = (a * a + b * b).sqrt();
        let h = if c < 1e-4 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
        [l, c, h]
    }

    /// An OKLCH color, with its chroma lowered until the screen can show it: the same
    /// lightness and hue, a little less vivid, rather than a different hue cut off at an edge.
    pub fn from_oklch([l, c, h]: [f32; 3]) -> Rgb {
        let lab = |c: f32| [l, c * h.to_radians().cos(), c * h.to_radians().sin()];
        let fits = |c: f32| linear_of(lab(c)).iter().all(|v| (-1e-4..=1.0001).contains(v));
        if fits(c) {
            return Rgb::from_oklab(lab(c));
        }
        let (mut lo, mut hi) = (0.0, c);
        for _ in 0..20 {
            let mid = (lo + hi) / 2.0;
            if fits(mid) { lo = mid } else { hi = mid }
        }
        Rgb::from_oklab(lab(lo))
    }

    pub fn format(self, f: Format) -> String {
        match f {
            Format::Hex => self.hex(),
            Format::Rgb => format!("rgb({}, {}, {})", self.0, self.1, self.2),
            Format::Hsl => {
                let [h, s, l] = self.hsl();
                format!("hsl({:.0}, {:.0}%, {:.0}%)", h.round() % 360.0, s * 100.0, l * 100.0)
            }
            Format::Oklch => {
                let [l, c, h] = self.oklch();
                format!("oklch({:.1}% {:.3} {:.1})", l * 100.0, c, h)
            }
        }
    }

    /// How bright it looks, 0 to 1 (WCAG's relative luminance).
    pub fn luminance(self) -> f32 {
        0.2126 * to_linear(self.0) + 0.7152 * to_linear(self.1) + 0.0722 * to_linear(self.2)
    }

    /// The contrast of text in one color on the other, 1 to 21 (WCAG).
    pub fn contrast(self, other: Rgb) -> f32 {
        let (a, b) = (self.luminance(), other.luminance());
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    /// Black or white, whichever reads better on this color.
    pub fn ink(self) -> Rgb {
        if self.contrast(Rgb(0, 0, 0)) >= self.contrast(Rgb(255, 255, 255)) { Rgb(0, 0, 0) } else { Rgb(255, 255, 255) }
    }

    /// Part of the way to another color, in OKLab: 0 is this color, 1 the other.
    pub fn mix(self, other: Rgb, t: f32) -> Rgb {
        let (a, b) = (self.oklab(), other.oklab());
        Rgb::from_oklab([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t])
    }

    /// This color and the ones that go with it, this one first.
    pub fn harmony(self, h: Harmony) -> Vec<Rgb> {
        let [l, c, hue] = self.oklch();
        std::iter::once(self).chain(h.turns().iter().map(|t| Rgb::from_oklch([l, c, hue + t]))).collect()
    }

    /// Tints, the color, then shades: `n` steps towards white, and `n` towards black, each an
    /// even step in lightness, stopping short of white and black themselves.
    pub fn shades(self, n: usize) -> Vec<Rgb> {
        let step = |i: usize| 0.85 * i as f32 / n as f32;
        let tints = (1..=n).rev().map(|i| self.mix(Rgb(255, 255, 255), step(i)));
        let shades = (1..=n).map(|i| self.mix(Rgb(0, 0, 0), step(i)));
        tints.chain(std::iter::once(self)).chain(shades).collect()
    }
}

/// Numbers in a CSS function, split on commas, spaces and slashes, with `%` and `deg` kept off.
fn numbers(inside: &str) -> Vec<(f32, bool)> {
    inside
        .split(|c: char| c == ',' || c == '/' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let pct = p.ends_with('%');
            let p = p.trim_end_matches('%').trim_end_matches("deg");
            p.parse::<f32>().ok().map(|v| (v, pct))
        })
        .collect()
}

/// A color written any of the usual ways: `#d53f8c`, `d53f8c`, `#d38`, `#d53f8cff`,
/// `rgb(213, 63, 140)`, `hsl(329 64% 54%)` or `oklch(60% 0.18 350)`.
pub fn parse(text: &str) -> Option<Rgb> {
    let t = text.trim().to_ascii_lowercase();
    if let Some((func, rest)) = t.split_once('(') {
        let n = numbers(rest.trim_end_matches(')'));
        if n.len() < 3 {
            return None;
        }
        let part = |(v, pct): (f32, bool), whole: f32| if pct { v / 100.0 } else { v / whole };
        return match func.trim() {
            "rgb" | "rgba" => Some(Rgb(byte(part(n[0], 255.0)), byte(part(n[1], 255.0)), byte(part(n[2], 255.0)))),
            "hsl" | "hsla" => Some(Rgb::from_hsl([n[0].0, part(n[1], 100.0), part(n[2], 100.0)])),
            // OKLCH's lightness is 0 to 1 bare, its chroma 0.4 at 100%.
            "oklch" => Some(Rgb::from_oklch([part(n[0], 1.0), if n[1].1 { n[1].0 / 100.0 * 0.4 } else { n[1].0 }, n[2].0])),
            _ => None,
        };
    }
    let h = t.strip_prefix('#').unwrap_or(&t);
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |s: &str| u8::from_str_radix(s, 16).ok();
    match h.len() {
        3 | 4 => {
            let d = |i: usize| v(&h[i..=i]).map(|x| x * 17);
            Some(Rgb(d(0)?, d(1)?, d(2)?))
        }
        6 | 8 => Some(Rgb(v(&h[0..2])?, v(&h[2..4])?, v(&h[4..6])?)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PINK: Rgb = Rgb(0xd5, 0x3f, 0x8c);

    #[test]
    fn reads_the_usual_ways() {
        for s in ["#d53f8c", "D53F8C", "#d53f8cff", "rgb(213, 63, 140)", "rgb(213 63 140 / 50%)", " #D53f8C "] {
            assert_eq!(parse(s), Some(PINK), "{s}");
        }
        assert_eq!(parse("#fff"), Some(Rgb(255, 255, 255)));
        assert_eq!(parse("hsl(0, 100%, 50%)"), Some(Rgb(255, 0, 0)));
        assert_eq!(parse("hsl(120deg 100% 25%)"), Some(Rgb(0, 128, 0)));
        assert_eq!(parse("oklch(100% 0 0)"), Some(Rgb(255, 255, 255)));
        for bad in ["", "#12", "#ggg", "rgb(1, 2)", "pink", "lab(50 1 1)"] {
            assert_eq!(parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn writes_back_what_it_reads() {
        for f in Format::ALL {
            let text = PINK.format(f);
            let back = parse(&text).unwrap_or_else(|| panic!("{text}"));
            let off = (back.0 as i32 - PINK.0 as i32).abs().max((back.1 as i32 - PINK.1 as i32).abs()).max((back.2 as i32 - PINK.2 as i32).abs());
            assert!(off <= 2, "{text} came back as {back:?}");
        }
        assert_eq!(PINK.format(Format::Rgb), "rgb(213, 63, 140)");
        assert_eq!(Rgb(255, 0, 0).format(Format::Hsl), "hsl(0, 100%, 50%)");
        assert_eq!(Rgb(255, 255, 255).format(Format::Oklch), "oklch(100.0% 0.000 0.0)");
    }

    #[test]
    fn oklab_goes_there_and_back() {
        for c in [PINK, Rgb(0, 0, 0), Rgb(255, 255, 255), Rgb(0, 0x84, 0xb8), Rgb(0xff, 0xf2, 0x70)] {
            assert_eq!(Rgb::from_oklch(c.oklch()), c);
            assert_eq!(Rgb::from_hsl(c.hsl()), c);
        }
    }

    #[test]
    fn harmonies_turn_the_hue() {
        let h = PINK.harmony(Harmony::Complementary);
        assert_eq!(h.len(), 2);
        assert_eq!(h[0], PINK);
        // Pink's opposite is a green, about as light.
        let [l, _, hue] = h[1].oklch();
        assert!((l - PINK.oklch()[0]).abs() < 0.03, "{l}");
        assert!((hue - (PINK.oklch()[2] + 180.0).rem_euclid(360.0)).abs() < 3.0, "{hue}");
        assert_eq!(PINK.harmony(Harmony::Square).len(), 4);
        // A grey has no hue to turn.
        assert!(Rgb(128, 128, 128).harmony(Harmony::Triadic).iter().all(|c| *c == Rgb(128, 128, 128)));
    }

    #[test]
    fn shades_run_light_to_dark() {
        let s = PINK.shades(4);
        assert_eq!(s.len(), 9);
        assert_eq!(s[4], PINK);
        assert!(s.windows(2).all(|w| w[0].luminance() > w[1].luminance()));
        assert_ne!(s[0], Rgb(255, 255, 255));
        assert_ne!(s[8], Rgb(0, 0, 0));
    }

    #[test]
    fn contrast_is_wcags() {
        assert!((Rgb(0, 0, 0).contrast(Rgb(255, 255, 255)) - 21.0).abs() < 0.01);
        assert!((PINK.contrast(Rgb(255, 255, 255)) - 4.27).abs() < 0.01, "{}", PINK.contrast(Rgb(255, 255, 255)));
        assert_eq!(Rgb(0xff, 0xf2, 0x70).ink(), Rgb(0, 0, 0));
        assert_eq!(Rgb(0x24, 0x12, 0x1b).ink(), Rgb(255, 255, 255));
    }
}
