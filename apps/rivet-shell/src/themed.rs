//! Other programs in the theme's colours: terminals and editors that read a colour file of
//! their own kind. For each one turned on, Rivet keeps one file of its own in that program's
//! settings folder, written again whenever the theme's colours change, and tells the running
//! program to read it where there is a way to. Rivet never rewrites a program's own settings
//! file; the one line that points at Rivet's file is added only when the person asks.

use std::path::PathBuf;

use eframe::egui::Color32;
use rivet_look::palette::{luminance, mix};

use crate::config::Settings;

/// The theme as these programs need it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Colors {
    pub dark: bool,
    pub bg: Color32,
    pub fg: Color32,
    pub accent: Color32,
    pub secondary: Color32,
    pub tertiary: Color32,
    /// Black, red, green, yellow, blue, magenta, cyan, white, then their bright forms.
    pub ansi: [Color32; 16],
    /// Code: comments, keywords, strings, numbers, functions, types.
    pub code: [Color32; 6],
}

const fn rgb(c: u32) -> Color32 {
    Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8)
}

/// Red, green, yellow, blue, magenta, cyan before the theme turns them: for dark backgrounds,
/// then for light ones. The same six Rivet Terminal starts from.
const SIX: [[Color32; 6]; 2] = [[rgb(0xe8706a), rgb(0x9fcf7c), rgb(0xe9c46a), rgb(0x6faee8), rgb(0xc792e0), rgb(0x5fc6c0)], [rgb(0xc0392b), rgb(0x3d8a28), rgb(0x9a6b00), rgb(0x1f64b8), rgb(0x8b3fb8), rgb(0x0f8079)]];

impl Colors {
    /// From the window's colours and the theme's three. Each of the six hues leans toward the
    /// nearest theme colour, as Rivet Terminal's do, so red stays reddish and a new wallpaper
    /// still shows.
    pub fn new(bg: Color32, fg: Color32, trio: [Color32; 3]) -> Colors {
        let dark = luminance(bg) < 0.5;
        let far = if dark { Color32::WHITE } else { Color32::BLACK };
        // Away from the background until it reads on it.
        let readable = |mut c: Color32| {
            for _ in 0..4 {
                if (luminance(c) - luminance(bg)).abs() >= 0.3 {
                    break;
                }
                c = mix(c, far, 0.25);
            }
            c
        };
        let mut ansi = [Color32::BLACK; 16];
        for (i, base) in SIX[usize::from(!dark)].into_iter().enumerate() {
            // Red turns least, so errors stay red.
            let c = readable(rivet_look::highlight::lean(base, trio, if i == 0 { 14.0 } else { 34.0 }));
            ansi[i + 1] = c;
            ansi[i + 9] = mix(c, far, 0.2);
        }
        ansi[0] = mix(mix(bg, fg, if dark { 0.14 } else { 0.85 }), trio[0], 0.1);
        ansi[8] = mix(mix(fg, bg, 0.45), trio[0], 0.1);
        ansi[7] = mix(mix(fg, bg, if dark { 0.1 } else { 0.4 }), trio[0], 0.1);
        ansi[15] = if dark { mix(fg, Color32::WHITE, 0.3) } else { mix(bg, fg, 0.18) };
        let c = rivet_look::highlight::colors(fg, bg, trio[0], trio[1], trio[2]);
        Colors { dark, bg, fg, accent: trio[0], secondary: trio[1], tertiary: trio[2], ansi, code: [c.comment, c.keyword, c.string, c.number, c.function, c.type_] }
    }

    /// As the shell looks now.
    pub fn now(s: &Settings) -> Colors {
        let live = rivet_look::live::LiveLook::new();
        let pal = rivet_look::palette::shown(live.palette(), s.follow_desktop_colors);
        let dark = pal.map(|p| p.is_dark()).unwrap_or(s.theme != rivet_look::theme::Mode::Light);
        let bg = pal.and_then(|p| p.window_bg).unwrap_or(if dark { rgb(0x242424) } else { rgb(0xf6f5f4) });
        let fg = pal.and_then(|p| p.window_fg).unwrap_or(if dark { rgb(0xf2f2f2) } else { rgb(0x2a2a2a) });
        let roles = crate::style::Roles::new(&live, s);
        Colors::new(bg, fg, [roles.primary, roles.secondary, roles.tertiary])
    }

    fn selection(&self) -> Color32 {
        mix(self.bg, self.accent, if self.dark { 0.38 } else { 0.3 })
    }
}

fn hex(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// How the program comes to read Rivet's file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Hook {
    /// A line in its settings file (a path in its folder), which Rivet can add at the end.
    Line { file: &'static str, line: &'static str },
    /// The same, where the line only counts at the top, before the file's first section.
    FirstLine { file: &'static str, line: &'static str },
    /// Something the person does in the program or its settings: said, not done.
    Tell(&'static str),
}

/// How a running one is told the colours changed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reload {
    /// It watches the file itself.
    Watches,
    /// A signal to every process of this name.
    Signal(&'static str, &'static str),
    /// Not until it starts again, or its next window.
    Later,
}

pub struct App {
    /// What the settings file calls it.
    pub id: &'static str,
    pub name: &'static str,
    /// Any of these on the path means it is installed.
    commands: &'static [&'static str],
    /// Its settings folder, under the config folder.
    folder: &'static str,
    /// Rivet's file in that folder.
    file: &'static str,
    pub hook: Hook,
    pub reload: Reload,
    make: fn(&Colors) -> String,
}

pub const APPS: &[App] = &[
    App { id: "kitty", name: "kitty", commands: &["kitty"], folder: "kitty", file: "rivet-colors.conf", hook: Hook::Line { file: "kitty.conf", line: "include rivet-colors.conf" }, reload: Reload::Signal("kitty", "USR1"), make: kitty },
    App { id: "ghostty", name: "Ghostty", commands: &["ghostty"], folder: "ghostty", file: "themes/rivet", hook: Hook::Line { file: "config", line: "theme = rivet" }, reload: Reload::Signal("ghostty", "USR2"), make: ghostty },
    App { id: "alacritty", name: "Alacritty", commands: &["alacritty"], folder: "alacritty", file: "rivet-colors.toml", hook: Hook::Tell("In alacritty.toml, under [general], add: import = [\"rivet-colors.toml\"]"), reload: Reload::Watches, make: alacritty },
    App { id: "foot", name: "foot", commands: &["foot"], folder: "foot", file: "rivet-colors.ini", hook: Hook::FirstLine { file: "foot.ini", line: "include=~/.config/foot/rivet-colors.ini" }, reload: Reload::Later, make: foot },
    App { id: "zed", name: "Zed", commands: &["zed", "zeditor", "zedit"], folder: "zed", file: "themes/rivet.json", hook: Hook::Tell("In Zed, choose the theme named Rivet."), reload: Reload::Watches, make: zed },
    App { id: "btop", name: "btop", commands: &["btop"], folder: "btop", file: "themes/rivet.theme", hook: Hook::Tell("In btop, press Esc, open Options and set Color theme to rivet."), reload: Reload::Later, make: btop },
];

impl App {
    pub fn installed(&self) -> bool {
        self.commands.iter().any(|c| crate::browser::on_path(c).is_some())
    }

    fn dir(&self) -> PathBuf {
        rivet_xdg::places::config_home().join(self.folder)
    }

    fn path(&self) -> PathBuf {
        self.dir().join(self.file)
    }

    /// The program's own settings file and the line it needs, when it needs one.
    fn line(&self) -> Option<(PathBuf, &'static str, bool)> {
        match self.hook {
            Hook::Line { file, line } => Some((self.dir().join(file), line, false)),
            Hook::FirstLine { file, line } => Some((self.dir().join(file), line, true)),
            Hook::Tell(_) => None,
        }
    }

    /// Whether the program's settings already point at Rivet's file (always, where there is
    /// only something to tell).
    pub fn hooked(&self) -> bool {
        match self.line() {
            Some((file, line, _)) => std::fs::read_to_string(file).is_ok_and(|t| has_line(&t, line)),
            None => true,
        }
    }

    /// Add the line to the program's settings file: at its end, or at its top where that is
    /// where it counts. Everything already in the file stays as it is.
    pub fn hook_up(&self) -> Result<(), String> {
        let Some((file, line, first)) = self.line() else { return Ok(()) };
        // Through a link, so settings kept elsewhere (a dotfiles folder) stay linked.
        let file = std::fs::canonicalize(&file).unwrap_or(file);
        let old = std::fs::read_to_string(&file).unwrap_or_default();
        if has_line(&old, line) {
            return Ok(());
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, with_line(&old, line, first)).map_err(|e| format!("{}: {e}", file.display()))
    }
}

fn has_line(text: &str, line: &str) -> bool {
    let squash = |l: &str| l.split_whitespace().collect::<String>();
    text.lines().any(|l| squash(l) == squash(line))
}

fn with_line(old: &str, line: &str, first: bool) -> String {
    const WHY: &str = "# Colors from Rivet (Settings, Applications).";
    if first {
        format!("{WHY}\n{line}\n\n{old}")
    } else {
        let gap = if old.is_empty() || old.ends_with("\n\n") { "" } else if old.ends_with('\n') { "\n" } else { "\n\n" };
        format!("{old}{gap}{WHY}\n{line}\n")
    }
}

/// Bring each chosen program's file up to date with the theme, and tell the running ones.
/// A program turned off has Rivet's file taken away.
pub fn apply(s: &Settings) {
    let colors = Colors::now(s);
    for a in APPS {
        let path = a.path();
        if !s.themed_apps.iter().any(|id| id == a.id) {
            if path.exists() && std::fs::remove_file(&path).is_ok() {
                reload(a);
            }
            continue;
        }
        if !a.installed() {
            continue;
        }
        let text = (a.make)(&colors);
        if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
            continue;
        }
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match std::fs::write(&path, text) {
            Ok(()) => reload(a),
            Err(e) => eprintln!("rivet-shell: {}'s colors: {}: {e}", a.name, path.display()),
        }
    }
}

fn reload(a: &App) {
    if let Reload::Signal(process, signal) = a.reload {
        crate::sys::spawn("pkill", &[&format!("-{signal}"), "-x", process]);
    }
}

// ------------------------------------------------------------------ each program's file

const HEAD: &str = "Rivet writes this file from the theme (Settings, Applications). Changes here are lost.";

fn kitty(c: &Colors) -> String {
    let mut t = format!("# {HEAD}\nforeground {}\nbackground {}\ncursor {}\ncursor_text_color {}\nselection_background {}\nselection_foreground {}\nurl_color {}\nactive_border_color {}\ninactive_border_color {}\nactive_tab_background {}\nactive_tab_foreground {}\ninactive_tab_background {}\ninactive_tab_foreground {}\n", hex(c.fg), hex(c.bg), hex(c.accent), hex(c.bg), hex(c.selection()), hex(c.fg), hex(c.secondary), hex(c.accent), hex(c.ansi[8]), hex(c.selection()), hex(c.fg), hex(mix(c.bg, c.fg, 0.06)), hex(mix(c.fg, c.bg, 0.4)));
    for (i, a) in c.ansi.iter().enumerate() {
        t.push_str(&format!("color{i} {}\n", hex(*a)));
    }
    t
}

fn ghostty(c: &Colors) -> String {
    let mut t = format!("# {HEAD}\nbackground = {}\nforeground = {}\ncursor-color = {}\ncursor-text = {}\nselection-background = {}\nselection-foreground = {}\n", hex(c.bg), hex(c.fg), hex(c.accent), hex(c.bg), hex(c.selection()), hex(c.fg));
    for (i, a) in c.ansi.iter().enumerate() {
        t.push_str(&format!("palette = {i}={}\n", hex(*a)));
    }
    t
}

fn alacritty(c: &Colors) -> String {
    const NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    let mut t = format!("# {HEAD}\n[colors.primary]\nbackground = \"{}\"\nforeground = \"{}\"\n\n[colors.cursor]\ncursor = \"{}\"\ntext = \"{}\"\n\n[colors.selection]\nbackground = \"{}\"\ntext = \"{}\"\n", hex(c.bg), hex(c.fg), hex(c.accent), hex(c.bg), hex(c.selection()), hex(c.fg));
    for (table, from) in [("normal", 0), ("bright", 8)] {
        t.push_str(&format!("\n[colors.{table}]\n"));
        for (i, name) in NAMES.iter().enumerate() {
            t.push_str(&format!("{name} = \"{}\"\n", hex(c.ansi[from + i])));
        }
    }
    t
}

/// Whether the foot installed is new enough to want its colours under `[colors-dark]`: it
/// says so about plain `[colors]` each time it starts, from 1.23 on. Older ones only know
/// `[colors]`.
fn foot_is_new() -> bool {
    let said = std::process::Command::new("foot").arg("--version").stdin(std::process::Stdio::null()).output().map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    foot_version(&said).is_some_and(|v| v >= (1, 23))
}

/// "foot version: 1.27.0 -pgo +ime" as (1, 27).
fn foot_version(said: &str) -> Option<(u32, u32)> {
    let v = said.split_whitespace().find(|w| w.starts_with(|c: char| c.is_ascii_digit()))?;
    let mut n = v.split('.').map(|p| p.parse::<u32>().ok());
    Some((n.next()??, n.next()??))
}

fn foot(c: &Colors) -> String {
    foot_under(c, if foot_is_new() { "colors-dark" } else { "colors" })
}

fn foot_under(c: &Colors, section: &str) -> String {
    let bare = |x: Color32| hex(x)[1..].to_string();
    let mut t = format!("# {HEAD}\n[{section}]\nforeground={}\nbackground={}\nselection-foreground={}\nselection-background={}\nurls={}\n", bare(c.fg), bare(c.bg), bare(c.fg), bare(c.selection()), bare(c.secondary));
    for (i, a) in c.ansi.iter().enumerate() {
        t.push_str(&format!("{}{}={}\n", if i < 8 { "regular" } else { "bright" }, i % 8, bare(*a)));
    }
    t
}

fn zed(c: &Colors) -> String {
    use serde_json::json;
    let at = |t: f32| hex(mix(c.bg, c.fg, t));
    let faint = |x: Color32, t: f32| hex(mix(c.bg, x, t));
    let [comment, keyword, string, number, function, type_] = c.code.map(hex);
    let (red, green, yellow, blue) = (c.ansi[1], c.ansi[2], c.ansi[3], c.ansi[4]);
    let mut style = serde_json::Map::new();
    let mut put = |k: &str, v: String| {
        style.insert(k.to_string(), json!(v));
    };
    for (k, v) in [
        ("background", hex(c.bg)),
        ("surface.background", at(0.03)),
        ("elevated_surface.background", at(0.06)),
        ("panel.background", at(0.03)),
        ("status_bar.background", hex(c.bg)),
        ("title_bar.background", hex(c.bg)),
        ("toolbar.background", hex(c.bg)),
        ("tab_bar.background", at(0.03)),
        ("tab.inactive_background", at(0.03)),
        ("tab.active_background", hex(c.bg)),
        ("editor.background", hex(c.bg)),
        ("editor.gutter.background", hex(c.bg)),
        ("editor.foreground", hex(c.fg)),
        ("editor.active_line.background", at(0.05)),
        ("editor.line_number", at(0.35)),
        ("editor.active_line_number", hex(c.fg)),
        ("editor.indent_guide", at(0.08)),
        ("editor.indent_guide_active", at(0.2)),
        ("editor.wrap_guide", at(0.08)),
        ("editor.document_highlight.read_background", faint(c.accent, 0.16)),
        ("editor.document_highlight.write_background", faint(c.accent, 0.26)),
        ("search.match_background", faint(c.tertiary, 0.35)),
        ("terminal.background", hex(c.bg)),
        ("terminal.foreground", hex(c.fg)),
        ("border", at(0.12)),
        ("border.variant", at(0.08)),
        ("border.focused", hex(c.accent)),
        ("border.selected", hex(c.accent)),
        ("border.disabled", at(0.06)),
        ("pane.focused_border", hex(c.accent)),
        ("panel.focused_border", hex(c.accent)),
        ("element.background", at(0.07)),
        ("element.hover", at(0.11)),
        ("element.active", at(0.15)),
        ("element.selected", faint(c.accent, 0.22)),
        ("element.disabled", at(0.04)),
        ("ghost_element.hover", at(0.07)),
        ("ghost_element.active", at(0.12)),
        ("ghost_element.selected", faint(c.accent, 0.18)),
        ("drop_target.background", faint(c.accent, 0.25)),
        ("scrollbar.thumb.background", at(0.2)),
        ("scrollbar.thumb.hover_background", at(0.3)),
        ("scrollbar.track.background", hex(c.bg)),
        ("text", hex(c.fg)),
        ("text.muted", at(0.62)),
        ("text.placeholder", at(0.45)),
        ("text.disabled", at(0.38)),
        ("text.accent", hex(c.accent)),
        ("icon", hex(c.fg)),
        ("icon.muted", at(0.62)),
        ("icon.disabled", at(0.38)),
        ("icon.accent", hex(c.accent)),
        ("link_text.hover", hex(c.accent)),
        ("error", hex(red)),
        ("error.background", faint(red, 0.14)),
        ("error.border", faint(red, 0.4)),
        ("warning", hex(yellow)),
        ("warning.background", faint(yellow, 0.14)),
        ("warning.border", faint(yellow, 0.4)),
        ("success", hex(green)),
        ("info", hex(blue)),
        ("hint", at(0.5)),
        ("created", hex(green)),
        ("modified", hex(yellow)),
        ("deleted", hex(red)),
        ("conflict", hex(c.tertiary)),
        ("ignored", at(0.38)),
        ("hidden", at(0.38)),
    ] {
        put(k, v);
    }
    const NAMES: [&str; 8] = ["black", "red", "green", "yellow", "blue", "magenta", "cyan", "white"];
    for (i, a) in c.ansi.iter().enumerate() {
        put(&format!("terminal.ansi.{}{}", if i < 8 { "" } else { "bright_" }, NAMES[i % 8]), hex(*a));
    }
    let mut style = serde_json::Value::Object(style);
    style["accents"] = json!([hex(c.accent), hex(c.secondary), hex(c.tertiary)]);
    style["players"] = json!([{ "cursor": hex(c.accent), "background": hex(c.accent), "selection": faint(c.accent, 0.3) }]);
    let one = |color: &str| json!({ "color": color });
    let punct = at(0.7);
    style["syntax"] = json!({
        "comment": { "color": comment, "font_style": "italic" }, "comment.doc": { "color": comment, "font_style": "italic" },
        "keyword": one(&keyword), "operator": one(&punct), "punctuation": one(&punct), "punctuation.bracket": one(&punct), "punctuation.delimiter": one(&punct),
        "string": one(&string), "string.escape": one(&number), "string.regex": one(&number), "string.special": one(&number),
        "number": one(&number), "boolean": one(&number), "constant": one(&number),
        "function": one(&function), "constructor": one(&function), "tag": one(&keyword), "attribute": one(&function),
        "type": one(&type_), "enum": one(&type_), "variant": one(&type_),
        "variable": one(&hex(c.fg)), "property": one(&hex(mix(c.fg, c.secondary, 0.5))), "label": one(&hex(c.accent)), "link_uri": one(&hex(c.secondary)), "link_text": one(&hex(c.accent)),
        "title": { "color": hex(c.accent), "font_weight": 700 }, "emphasis": { "font_style": "italic" }, "emphasis.strong": { "font_weight": 700 }
    });
    let family = json!({
        "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
        "name": "Rivet",
        "author": "Rivet",
        "themes": [{ "name": "Rivet", "appearance": if c.dark { "dark" } else { "light" }, "style": style }]
    });
    serde_json::to_string_pretty(&family).unwrap_or_default() + "\n"
}

fn btop(c: &Colors) -> String {
    let (red, green, yellow, blue, cyan) = (c.ansi[1], c.ansi[2], c.ansi[3], c.ansi[4], c.ansi[6]);
    let lines: [(&str, Color32); 37] = [
        ("main_bg", c.bg),
        ("main_fg", c.fg),
        ("title", c.fg),
        ("hi_fg", c.accent),
        ("selected_bg", c.selection()),
        ("selected_fg", c.fg),
        ("inactive_fg", mix(c.fg, c.bg, 0.6)),
        ("graph_text", mix(c.fg, c.bg, 0.3)),
        ("meter_bg", mix(c.bg, c.fg, 0.14)),
        ("proc_misc", c.secondary),
        ("cpu_box", mix(c.bg, c.accent, 0.6)),
        ("mem_box", mix(c.bg, c.secondary, 0.6)),
        ("net_box", mix(c.bg, c.tertiary, 0.6)),
        ("proc_box", mix(c.bg, c.accent, 0.6)),
        ("div_line", mix(c.bg, c.fg, 0.2)),
        ("temp_start", green),
        ("temp_mid", yellow),
        ("temp_end", red),
        ("cpu_start", c.accent),
        ("cpu_mid", c.tertiary),
        ("cpu_end", red),
        ("free_start", green),
        ("free_mid", green),
        ("free_end", green),
        ("cached_start", blue),
        ("cached_mid", blue),
        ("cached_end", blue),
        ("available_start", cyan),
        ("available_mid", cyan),
        ("available_end", cyan),
        ("used_start", c.secondary),
        ("used_mid", c.tertiary),
        ("used_end", red),
        ("download_start", c.secondary),
        ("download_end", c.accent),
        ("upload_start", c.tertiary),
        ("upload_end", red),
    ];
    let mut t = format!("# {HEAD}\n");
    for (k, v) in lines {
        t.push_str(&format!("theme[{k}]=\"{}\"\n", hex(v)));
    }
    // The middle and the processes' own gradients, which the table above has no room to say twice.
    t.push_str(&format!("theme[download_mid]=\"{}\"\ntheme[upload_mid]=\"{}\"\ntheme[process_start]=\"{}\"\ntheme[process_mid]=\"{}\"\ntheme[process_end]=\"{}\"\n", hex(mix(c.secondary, c.accent, 0.5)), hex(mix(c.tertiary, red, 0.5)), hex(c.accent), hex(c.tertiary), hex(red)));
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Colors {
        Colors::new(rgb(0x1f1f1f), rgb(0xe6e6e6), [rgb(0xffb2bc), rgb(0xe5bdc1), rgb(0xebbf90)])
    }

    #[test]
    fn sixteen_read_on_the_background() {
        for c in [sample(), Colors::new(rgb(0xf6f5f4), rgb(0x2a2a2a), [rgb(0x3584e4), rgb(0x5a6f8f), rgb(0x8a5fa8)])] {
            for a in &c.ansi[1..7] {
                assert!((luminance(*a) - luminance(c.bg)).abs() >= 0.25, "{} on {}", hex(*a), hex(c.bg));
            }
        }
        assert!(sample().dark);
    }

    #[test]
    fn files() {
        let c = sample();
        let k = kitty(&c);
        assert!(k.contains("background #1f1f1f\n") && k.contains("\ncolor15 #"));
        let f = foot_under(&c, "colors");
        assert_eq!(foot_version("foot version: 1.27.0 -pgo +ime"), Some((1, 27)));
        assert!(f.contains("[colors]\nforeground=e6e6e6\nbackground=1f1f1f\n") && f.contains("\nregular0=") && f.contains("\nbright7="));
        assert!(alacritty(&c).contains("[colors.bright]\nblack = \"#"));
        assert!(ghostty(&c).contains("palette = 15=#"));
        assert!(btop(&c).contains("theme[main_bg]=\"#1f1f1f\"\n"));
        // Zed's is JSON it can read back, with one theme named Rivet.
        let z: serde_json::Value = serde_json::from_str(&zed(&c)).unwrap();
        assert_eq!(z["themes"][0]["name"], "Rivet");
        assert_eq!(z["themes"][0]["appearance"], "dark");
        assert_eq!(z["themes"][0]["style"]["terminal.ansi.bright_white"].as_str().map(str::len), Some(7));
    }

    /// Every program's file as this machine's theme makes it, into the folder RIVET_THEMED_OUT
    /// names: `RIVET_THEMED_OUT=/tmp/x cargo test -p rivet-shell files_here -- --ignored`.
    #[test]
    #[ignore = "writes this machine's colours to a folder"]
    fn files_here() {
        let Ok(out) = std::env::var("RIVET_THEMED_OUT") else { return };
        let c = Colors::now(&Settings::load());
        for a in APPS {
            let path = PathBuf::from(&out).join(a.folder).join(a.file);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, (a.make)(&c)).unwrap();
        }
    }

    #[test]
    fn the_line_is_added_once() {
        assert_eq!(with_line("font_size 12\n", "include rivet-colors.conf", false), "font_size 12\n\n# Colors from Rivet (Settings, Applications).\ninclude rivet-colors.conf\n");
        assert_eq!(with_line("", "theme = rivet", false), "# Colors from Rivet (Settings, Applications).\ntheme = rivet\n");
        assert!(with_line("[main]\nfont=x\n", "include=a", true).starts_with("# Colors from Rivet (Settings, Applications).\ninclude=a\n\n[main]"));
        assert!(has_line("a\n  include   rivet-colors.conf \n", "include rivet-colors.conf"));
        assert!(has_line("theme=rivet\n", "theme = rivet") && !has_line("# include rivet-colors.conf\n", "include rivet-colors.conf"));
    }
}
