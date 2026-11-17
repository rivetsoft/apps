//! Editor color schemes: the well-known palettes, mapped onto the editor's color roles (the
//! background, the text, comments, keywords, strings, numbers and constants, functions, types
//! and punctuation). The window around the editor keeps the app's own look; "Follow the app"
//! (no scheme) colors code from the desktop's palette instead.

use eframe::egui::{self, Color32};
use rivet_look::highlight::Colors;
use rivet_look::palette::mix;

pub struct Scheme {
    pub name: &'static str,
    pub dark: bool,
    bg: u32,
    fg: u32,
    comment: u32,
    keyword: u32,
    string: u32,
    number: u32,
    function: u32,
    type_: u32,
    punct: u32,
}

const fn s(name: &'static str, dark: bool, c: [u32; 9]) -> Scheme {
    Scheme { name, dark, bg: c[0], fg: c[1], comment: c[2], keyword: c[3], string: c[4], number: c[5], function: c[6], type_: c[7], punct: c[8] }
}

//                  background  text      comment   keyword   string    number    function  type      punct
pub const SCHEMES: [Scheme; 39] = [
    s("One Dark", true, [0x282c34, 0xabb2bf, 0x5c6370, 0xc678dd, 0x98c379, 0xd19a66, 0x61afef, 0xe5c07b, 0xabb2bf]),
    s("One Light", false, [0xfafafa, 0x383a42, 0xa0a1a7, 0xa626a4, 0x50a14f, 0x986801, 0x4078f2, 0xc18401, 0x383a42]),
    s("Dracula", true, [0x282a36, 0xf8f8f2, 0x6272a4, 0xff79c6, 0xf1fa8c, 0xbd93f9, 0x50fa7b, 0x8be9fd, 0xf8f8f2]),
    s("Monokai", true, [0x272822, 0xf8f8f2, 0x75715e, 0xf92672, 0xe6db74, 0xae81ff, 0xa6e22e, 0x66d9ef, 0xf8f8f2]),
    s("Solarized Dark", true, [0x002b36, 0x839496, 0x586e75, 0x859900, 0x2aa198, 0xd33682, 0x268bd2, 0xb58900, 0x839496]),
    s("Solarized Light", false, [0xfdf6e3, 0x657b83, 0x93a1a1, 0x859900, 0x2aa198, 0xd33682, 0x268bd2, 0xb58900, 0x657b83]),
    s("Nord", true, [0x2e3440, 0xd8dee9, 0x616e88, 0x81a1c1, 0xa3be8c, 0xb48ead, 0x88c0d0, 0x8fbcbb, 0xeceff4]),
    s("Gruvbox Dark", true, [0x282828, 0xebdbb2, 0x928374, 0xfb4934, 0xb8bb26, 0xd3869b, 0xfabd2f, 0x83a598, 0xebdbb2]),
    s("Gruvbox Light", false, [0xfbf1c7, 0x3c3836, 0x928374, 0x9d0006, 0x79740e, 0x8f3f71, 0xb57614, 0x076678, 0x3c3836]),
    s("Catppuccin Latte", false, [0xeff1f5, 0x4c4f69, 0x9ca0b0, 0x8839ef, 0x40a02b, 0xfe640b, 0x1e66f5, 0xdf8e1d, 0x7c7f93]),
    s("Catppuccin Frappé", true, [0x303446, 0xc6d0f5, 0x737994, 0xca9ee6, 0xa6d189, 0xef9f76, 0x8caaee, 0xe5c890, 0x949cbb]),
    s("Catppuccin Macchiato", true, [0x24273a, 0xcad3f5, 0x6e738d, 0xc6a0f6, 0xa6da95, 0xf5a97f, 0x8aadf4, 0xeed49f, 0x939ab7]),
    s("Catppuccin Mocha", true, [0x1e1e2e, 0xcdd6f4, 0x6c7086, 0xcba6f7, 0xa6e3a1, 0xfab387, 0x89b4fa, 0xf9e2af, 0x9399b2]),
    s("Tokyo Night", true, [0x1a1b26, 0xa9b1d6, 0x565f89, 0xbb9af7, 0x9ece6a, 0xff9e64, 0x7aa2f7, 0x2ac3de, 0x89ddff]),
    s("GitHub Light", false, [0xffffff, 0x1f2328, 0x6e7781, 0xcf222e, 0x0a3069, 0x0550ae, 0x8250df, 0x953800, 0x1f2328]),
    s("GitHub Dark", true, [0x0d1117, 0xe6edf3, 0x8b949e, 0xff7b72, 0xa5d6ff, 0x79c0ff, 0xd2a8ff, 0xffa657, 0xe6edf3]),
    s("GitHub Dark Dimmed", true, [0x22272e, 0xadbac7, 0x768390, 0xf47067, 0x96d0ff, 0x6cb6ff, 0xdcbdfb, 0xf69d50, 0xadbac7]),
    s("Rosé Pine", true, [0x191724, 0xe0def4, 0x6e6a86, 0x31748f, 0xf6c177, 0xc4a7e7, 0xebbcba, 0x9ccfd8, 0x908caa]),
    s("Rosé Pine Moon", true, [0x232136, 0xe0def4, 0x6e6a86, 0x3e8fb0, 0xf6c177, 0xc4a7e7, 0xea9a97, 0x9ccfd8, 0x908caa]),
    s("Rosé Pine Dawn", false, [0xfaf4ed, 0x575279, 0x9893a5, 0x286983, 0xea9d34, 0x907aa9, 0xd7827e, 0x56949f, 0x797593]),
    s("Everforest", true, [0x2d353b, 0xd3c6aa, 0x859289, 0xe67e80, 0xa7c080, 0xd699b6, 0x83c092, 0xdbbc7f, 0x9da9a0]),
    s("Ayu Dark", true, [0x0b0e14, 0xbfbdb6, 0x565b66, 0xff8f40, 0xaad94c, 0xd2a6ff, 0xffb454, 0x59c2ff, 0xbfbdb6]),
    s("Ayu Mirage", true, [0x1f2430, 0xcccac2, 0x707a8c, 0xffad66, 0xd5ff80, 0xdfbfff, 0xffd173, 0x73d0ff, 0xcccac2]),
    s("Ayu Light", false, [0xfcfcfc, 0x5c6166, 0xadaeb1, 0xfa8d3e, 0x86b300, 0xa37acc, 0xf2ae49, 0x399ee6, 0x5c6166]),
    s("Material", true, [0x263238, 0xeeffff, 0x546e7a, 0xc792ea, 0xc3e88d, 0xf78c6c, 0x82aaff, 0xffcb6b, 0x89ddff]),
    s("Palenight", true, [0x292d3e, 0xa6accd, 0x676e95, 0xc792ea, 0xc3e88d, 0xf78c6c, 0x82aaff, 0xffcb6b, 0x89ddff]),
    s("Kanagawa", true, [0x1f1f28, 0xdcd7ba, 0x727169, 0x957fb8, 0x98bb6c, 0xd27e99, 0x7e9cd8, 0x7aa89f, 0x9cabca]),
    s("Night Owl", true, [0x011627, 0xd6deeb, 0x637777, 0xc792ea, 0xecc48d, 0xf78c6c, 0x82aaff, 0xffcb8b, 0x7fdbca]),
    s("Light Owl", false, [0xfbfbfb, 0x403f53, 0x989fb1, 0x994cc3, 0xc96765, 0xaa0982, 0x4876d6, 0x0c969b, 0x403f53]),
    s("VS Code Dark Modern", true, [0x1f1f1f, 0xcccccc, 0x6a9955, 0x569cd6, 0xce9178, 0xb5cea8, 0xdcdcaa, 0x4ec9b0, 0xd4d4d4]),
    s("VS Code Light Modern", false, [0xffffff, 0x3b3b3b, 0x008000, 0x0000ff, 0xa31515, 0x098658, 0x795e26, 0x267f99, 0x3b3b3b]),
    s("JetBrains Darcula", true, [0x2b2b2b, 0xa9b7c6, 0x808080, 0xcc7832, 0x6a8759, 0x6897bb, 0xffc66d, 0x9876aa, 0xa9b7c6]),
    s("Xcode Light", false, [0xffffff, 0x000000, 0x5d6c79, 0x9b2393, 0xc41a16, 0x1c00cf, 0x326d74, 0x0b4f79, 0x000000]),
    s("Xcode Dark", true, [0x1f1f24, 0xffffff, 0x6c7986, 0xfc5fa3, 0xfc6a5d, 0xd0bf69, 0x67b7a4, 0x5dd8ff, 0xffffff]),
    s("High Contrast Dark", true, [0x000000, 0xffffff, 0x7ca668, 0x569cd6, 0xce9178, 0xb5cea8, 0xdcdcaa, 0x4ec9b0, 0xffffff]),
    s("High Contrast Light", false, [0xffffff, 0x000000, 0x515151, 0x0f4a85, 0xa31515, 0x096d48, 0x5e2cbc, 0x185e73, 0x000000]),
    s("Monokai Pro", true, [0x2d2a2e, 0xfcfcfa, 0x727072, 0xff6188, 0xffd866, 0xab9df2, 0xa9dc76, 0x78dce8, 0x939293]),
    s("Tokyo Night Day", false, [0xe1e2e7, 0x3760bf, 0x848cb5, 0x9854f1, 0x587539, 0xb15c00, 0x2e7de9, 0x007197, 0x3760bf]),
    s("Everforest Light", false, [0xfdf6e3, 0x5c6a72, 0x939f91, 0xf85552, 0x8da101, 0xdf69ba, 0x35a77c, 0xdfa000, 0x829181]),
];

fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

impl Scheme {
    pub fn by_name(name: &str) -> Option<&'static Scheme> {
        SCHEMES.iter().find(|s| s.name.eq_ignore_ascii_case(name))
    }

    pub fn bg(&self) -> Color32 {
        rgb(self.bg)
    }

    pub fn fg(&self) -> Color32 {
        rgb(self.fg)
    }

    /// A few of its colors, for swatches.
    pub fn swatch(&self) -> [Color32; 4] {
        [rgb(self.keyword), rgb(self.string), rgb(self.function), rgb(self.type_)]
    }

    /// The code colors.
    pub fn colors(&self) -> Colors {
        Colors {
            text: rgb(self.fg),
            keyword: rgb(self.keyword),
            string: rgb(self.string),
            number: rgb(self.number),
            comment: rgb(self.comment),
            function: rgb(self.function),
            type_: rgb(self.type_),
            punct: rgb(self.punct),
        }
    }

    /// The look of the editor's area: its background, text, faint text, selection and cursor.
    pub fn visuals(&self, base: &egui::Visuals) -> egui::Visuals {
        let mut v = if self.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
        // Keep the app's shapes (corners, spacing), only the colors change.
        v.window_corner_radius = base.window_corner_radius;
        v.menu_corner_radius = base.menu_corner_radius;
        let (bg, fg) = (self.bg(), self.fg());
        v.extreme_bg_color = bg;
        v.panel_fill = bg;
        v.window_fill = mix(bg, fg, 0.04);
        v.faint_bg_color = mix(bg, fg, 0.05);
        v.override_text_color = Some(fg);
        v.weak_text_color = Some(mix(bg, fg, 0.5));
        v.hyperlink_color = rgb(self.function);
        let accent = rgb(self.function);
        v.selection.bg_fill = mix(bg, accent, if self.dark { 0.35 } else { 0.28 });
        v.selection.stroke.color = fg;
        v.text_cursor.stroke.color = accent;
        v.widgets.hovered.weak_bg_fill = mix(bg, fg, 0.08);
        v.widgets.noninteractive.bg_stroke.color = mix(bg, fg, 0.12);
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asked_for_scheme_is_here() {
        for n in [
            "One Dark", "One Light", "Dracula", "Monokai", "Solarized Dark", "Solarized Light", "Nord", "Gruvbox Dark", "Gruvbox Light",
            "Catppuccin Latte", "Catppuccin Frappé", "Catppuccin Macchiato", "Catppuccin Mocha", "Tokyo Night", "GitHub Light", "GitHub Dark",
            "GitHub Dark Dimmed", "Rosé Pine", "Rosé Pine Moon", "Rosé Pine Dawn", "Everforest", "Ayu Dark", "Ayu Mirage", "Ayu Light",
            "Material", "Palenight", "Kanagawa", "Night Owl", "Light Owl", "VS Code Dark Modern", "VS Code Light Modern", "JetBrains Darcula",
            "Xcode Light", "Xcode Dark", "High Contrast Dark", "High Contrast Light",
        ] {
            assert!(Scheme::by_name(n).is_some(), "{n}");
        }
        // Text reads on its background: dark schemes have light text and the other way round.
        for s in &SCHEMES {
            let l = |c: Color32| c.r() as u32 + c.g() as u32 + c.b() as u32;
            assert_eq!(l(s.fg()) > l(s.bg()), s.dark, "{}", s.name);
        }
        let names: std::collections::HashSet<&str> = SCHEMES.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), SCHEMES.len());
    }
}
