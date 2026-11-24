//! rivetfetch: the computer at a glance in the terminal, beside Rivet the mascot, in the
//! desktop's accent. Every fact is gathered on its own thread, so the slowest one sets the time.

mod accent;
mod info;
mod logo;
mod outputs;

use info::Fact;

const HELP: &str = "rivetfetch: this computer at a glance, beside Rivet.

Usage: rivetfetch [OPTIONS]

  -l, --logo NAME|FILE   The picture: mascot, none, or a logo file
                         (fastfetch logo files work, with $1 to $9 for colours)
  -m, --modules LIST     What to show and in what order, as a comma-separated list
      --color WHEN       Colour the output: auto, always or never
      --pipe             Plain text with no colour, for scripts
      --list-modules     List the modules
      --list-logos       List the built-in logos
  -h, --help             Show this help
  -V, --version          Show the version

Settings in ~/.config/rivet-fetch/settings, one per line:
  logo=none
  modules=title,separator,os,kernel,uptime,memory,rivet
  color=auto";

#[derive(Debug, Clone, PartialEq)]
struct Options {
    logo: String,
    modules: Vec<String>,
    color: Option<bool>,
}

impl Default for Options {
    fn default() -> Self {
        Options { logo: "mascot".into(), modules: split_modules(info::DEFAULT), color: None }
    }
}

fn split_modules(list: &str) -> Vec<String> {
    // Commas, or colons as fastfetch's --structure takes them.
    list.split([',', ':']).map(|m| m.trim().to_lowercase()).filter(|m| !m.is_empty()).collect()
}

fn parse_color(v: &str) -> Result<Option<bool>, String> {
    match v {
        "auto" => Ok(None),
        "always" | "yes" | "true" => Ok(Some(true)),
        "never" | "no" | "false" => Ok(Some(false)),
        _ => Err(format!("--color takes auto, always or never, not \"{v}\".")),
    }
}

/// The settings file's lines over the defaults; unknown keys are skipped so an older
/// rivetfetch still reads a newer file.
fn apply_settings(o: &mut Options, text: &str) {
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let v = v.trim();
        match k.trim() {
            "logo" => o.logo = v.to_string(),
            "modules" => o.modules = split_modules(v),
            "color" => o.color = parse_color(v).unwrap_or(None),
            _ => {}
        }
    }
}

enum Run {
    Show(Options),
    Print(String),
}

fn parse_args(mut o: Options, args: &[String]) -> Result<Run, String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (flag, inline) = match a.split_once('=') {
            Some((f, v)) if f.starts_with("--") => (f, Some(v.to_string())),
            _ => (a.as_str(), None),
        };
        let mut value = |name: &str| inline.clone().or_else(|| it.next().cloned()).ok_or(format!("{name} needs a value. Run rivetfetch --help to see them."));
        match flag {
            "-l" | "--logo" => o.logo = value("--logo")?,
            "-m" | "--modules" | "-s" | "--structure" => o.modules = split_modules(&value("--modules")?),
            "--color" => o.color = parse_color(&value("--color")?)?,
            "--pipe" => o.color = Some(false),
            "--list-modules" => {
                let w = info::MODULES.iter().map(|(n, _)| n.len()).max().unwrap_or(0);
                let mut s: Vec<String> = info::MODULES.iter().map(|(n, d)| format!("{n:w$}  {d}")).collect();
                s.push(format!("{:w$}  an empty line", "break"));
                return Ok(Run::Print(s.join("\n")));
            }
            "--list-logos" => return Ok(Run::Print(logo::NAMES.join("\n"))),
            "-h" | "--help" => return Ok(Run::Print(HELP.into())),
            "-V" | "--version" => return Ok(Run::Print(format!("rivetfetch {}", env!("CARGO_PKG_VERSION")))),
            other => return Err(format!("rivetfetch doesn't know \"{other}\". Run rivetfetch --help to see what it takes.")),
        }
    }
    Ok(Run::Show(o))
}

/// The escape codes output is drawn with; all empty when colour is off.
struct Paint {
    on: bool,
    truecolor: bool,
    accent: String,
    cheeks: String,
}

impl Paint {
    fn new(on: bool) -> Paint {
        let truecolor = std::env::var("COLORTERM").is_ok_and(|c| c == "truecolor" || c == "24bit");
        let rgb = |c: logo::Rgb| colour_code(38, c, truecolor);
        // The desktop's accent, as every Rivet app shows it; Rivet's own pink without one.
        let accent = accent::load().unwrap_or((0xe0, 0x7a, 0xab));
        Paint { on, truecolor, accent: rgb(accent), cheeks: rgb((0xe0, 0x7a, 0xab)) }
    }

    /// A bitmap cell: its top pixel in the foreground of an upper half block, its bottom pixel
    /// in the background (or a lower half block when only the bottom is drawn).
    fn cell(&self, (top, bottom): logo::Cell) -> String {
        let fg = |c| colour_code(38, c, self.truecolor);
        let bg = |c| colour_code(48, c, self.truecolor);
        match (top, bottom) {
            (Some(t), Some(b)) if t == b => format!("\x1b[49;{}m\u{2588}", fg(t)),
            (Some(t), Some(b)) => format!("\x1b[{};{}m\u{2580}", fg(t), bg(b)),
            (Some(t), None) => format!("\x1b[49;{}m\u{2580}", fg(t)),
            (None, Some(b)) => format!("\x1b[49;{}m\u{2584}", fg(b)),
            (None, None) => "\x1b[0m ".into(),
        }
    }

    fn sgr(&self, code: &str) -> String {
        if self.on { format!("\x1b[{code}m") } else { String::new() }
    }

    fn reset(&self) -> String {
        self.sgr("0")
    }

    /// The colour of a logo slot (`$1`…`$9`).
    fn slot(&self, n: u8) -> String {
        let code = match n {
            1 | 6 => "0;1".to_string(),
            3 | 7 => format!("0;1;{}", self.accent),
            4 => format!("0;{}", self.cheeks),
            5 => "0;1;93".to_string(),
            8 => "0;36".to_string(),
            9 => "0;34".to_string(),
            _ => "0".to_string(),
        };
        self.sgr(&code)
    }

    fn key(&self, k: &str) -> String {
        format!("{}{k}{}", self.sgr(&format!("1;{}", self.accent)), self.reset())
    }

    /// Green while there's room, yellow past half, red past four fifths (the other way round
    /// for a battery).
    fn percent(&self, p: f64, fuller_is_better: bool) -> String {
        let level = if fuller_is_better { 100.0 - p } else { p };
        let c = if level < 50.0 {
            "32"
        } else if level < 80.0 {
            "33"
        } else {
            "31"
        };
        format!("{}{p:.0}%{}", self.sgr(c), self.reset())
    }
}

/// A foreground (38) or background (48) colour, exact with truecolor, else the nearest of the
/// 256 colours nearly every terminal has.
fn colour_code(base: u8, (r, g, b): logo::Rgb, truecolor: bool) -> String {
    if truecolor { format!("{base};2;{r};{g};{b}") } else { format!("{base};5;{}", cube(r, g, b)) }
}

/// The nearest colour in the 6×6×6 cube of the 256-colour palette.
fn cube(r: u8, g: u8, b: u8) -> u8 {
    const STEPS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let near = |v: u8| (0..6).min_by_key(|&i| (STEPS[i] as i32 - v as i32).abs()).unwrap() as u8;
    16 + 36 * near(r) + 6 * near(g) + near(b)
}

/// One module's lines, drawn.
fn draw(module: &str, facts: Vec<Fact>, title_width: usize, paint: &Paint) -> Vec<String> {
    match module {
        "break" => vec![String::new()],
        "separator" => vec!["-".repeat(title_width.max(1))],
        "colors" => {
            let row = |base: u8| (0..8).map(|i| format!("{}   ", paint.sgr(&(base + i).to_string()))).collect::<String>() + &paint.reset();
            if paint.on { vec![row(40), row(100)] } else { Vec::new() }
        }
        "title" => facts
            .into_iter()
            .map(|f| match f.value.split_once('@') {
                Some((u, h)) => format!("{}@{}", paint.key(u), paint.key(h)),
                None => paint.key(&f.value),
            })
            .collect(),
        _ => facts
            .into_iter()
            .map(|f| {
                let mut v = f.value;
                if let Some(p) = f.percent {
                    let p = paint.percent(p, f.key.starts_with("Battery"));
                    v = if v.is_empty() { p } else if f.key.starts_with("Battery") { format!("{p} {v}") } else { format!("{v} ({p})") };
                }
                v.push_str(f.after.as_deref().unwrap_or(""));
                if f.key.is_empty() { v } else { format!("{}: {v}", paint.key(&f.key)) }
            })
            .collect(),
    }
}

/// The logo's lines drawn, each with the columns it takes.
fn draw_logo(logo: &logo::Logo, paint: &Paint) -> Vec<(String, usize)> {
    match logo {
        logo::Logo::Text(text) => text
            .lines()
            .map(|l| {
                let mut row: String = logo::runs(l).into_iter().map(|(slot, t)| paint.slot(slot) + &t).collect();
                row.push_str(&paint.reset());
                (row, logo::width(l))
            })
            .collect(),
        // A bitmap needs colour to be him; plain output goes without.
        logo::Logo::Bitmap(_) if !paint.on => Vec::new(),
        logo::Logo::Bitmap(rows) => rows
            .iter()
            .map(|cells| {
                let mut row: String = cells.iter().map(|&c| paint.cell(c)).collect();
                row.push_str(&paint.reset());
                (row, cells.len())
            })
            .collect(),
    }
}

/// The logo and the lines side by side, the lines starting three columns past the logo's
/// widest line.
fn compose(logo_lines: &[(String, usize)], lines: &[String], paint: &Paint) -> String {
    let width = logo_lines.iter().map(|(_, w)| *w).max().unwrap_or(0);
    let gap = if width > 0 { 3 } else { 0 };
    let rows = logo_lines.len().max(lines.len());
    let mut out = String::new();
    for i in 0..rows {
        let (art, w) = logo_lines.get(i).map(|(a, w)| (a.as_str(), *w)).unwrap_or(("", 0));
        let mut row = art.to_string();
        if let Some(info) = lines.get(i) {
            row.push_str(&" ".repeat(width - w + gap));
            row.push_str(info);
        }
        out.push_str(row.trim_end());
        out.push_str(&paint.reset());
        out.push('\n');
    }
    out
}

fn main() {
    let mut options = Options::default();
    if let Ok(text) = std::fs::read_to_string(rivet_xdg::places::config_home().join("rivet-fetch/settings")) {
        apply_settings(&mut options, &text);
    }
    let args: Vec<String> = std::env::args().skip(1).collect();
    let options = match parse_args(options, &args) {
        Ok(Run::Show(o)) => o,
        Ok(Run::Print(text)) => {
            println!("{text}");
            return;
        }
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let unknown: Vec<&String> = options.modules.iter().filter(|m| !info::known(m)).collect();
    if !unknown.is_empty() {
        let names: Vec<&str> = unknown.iter().map(|s| s.as_str()).collect();
        eprintln!("rivetfetch has no module called {}. Run rivetfetch --list-modules to see them.", names.join(", "));
        std::process::exit(2);
    }
    let logo = match logo::load(&options.logo) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    let tty = unsafe { libc::isatty(1) } == 1;
    let color = options.color.unwrap_or(tty && std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty()));

    let needs_gtk = options.modules.iter().any(|m| ["theme", "icons", "font", "cursor"].contains(&m.as_str()));
    let empty = info::Gtk::default();
    let (paint, facts) = std::thread::scope(|s| {
        let gtk = s.spawn(move || if needs_gtk { info::Gtk::read() } else { info::Gtk::default() });
        let paint = s.spawn(move || Paint::new(color));
        let handles: Vec<_> = options
            .modules
            .iter()
            .map(|m| {
                let wait = ["theme", "icons", "font", "cursor"].contains(&m.as_str());
                let empty = &empty;
                (wait, s.spawn(move || if wait { Vec::new() } else { info::gather(m, empty) }))
            })
            .collect();
        let gtk = gtk.join().unwrap_or_default();
        let facts: Vec<Vec<Fact>> = handles
            .into_iter()
            .zip(&options.modules)
            .map(|((wait, h), m)| if wait { info::gather(m, &gtk) } else { h.join().unwrap_or_default() })
            .collect();
        (paint.join().unwrap_or_else(|_| Paint::new(false)), facts)
    });

    let title_width = info::user_name().chars().count() + 1 + info::host_name().chars().count();
    let lines: Vec<String> = options.modules.iter().zip(facts).flat_map(|(m, f)| draw(m, f, title_width, &paint)).collect();
    print!("{}", compose(&draw_logo(&logo, &paint), &lines, &paint));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain() -> Paint {
        Paint { on: false, truecolor: false, accent: String::new(), cheeks: String::new() }
    }

    #[test]
    fn settings_and_flags_combine() {
        let mut o = Options::default();
        apply_settings(&mut o, "logo=none\nmodules=os, kernel\nnew-key=1\n");
        assert_eq!(o.logo, "none");
        assert_eq!(o.modules, ["os", "kernel"]);
        let args: Vec<String> = ["--logo=none", "-s", "os:cpu", "--pipe"].iter().map(|s| s.to_string()).collect();
        let Ok(Run::Show(o)) = parse_args(o, &args) else { panic!() };
        assert_eq!((o.logo.as_str(), o.modules.len(), o.color), ("none", 2, Some(false)));
        assert!(parse_args(Options::default(), &["--nope".into()]).is_err());
    }

    #[test]
    fn lines_start_past_the_logo() {
        let logo = draw_logo(&logo::Logo::Text("$1ab\n$1abcd".into()), &plain());
        let out = compose(&logo, &["one".into(), "two".into(), "three".into()], &plain());
        assert_eq!(out, "ab     one\nabcd   two\n       three\n");
        assert_eq!(compose(&[], &["one".into()], &plain()), "one\n");
    }

    #[test]
    fn facts_draw_with_keys_and_percentages() {
        let f = vec![Fact { key: "Memory".into(), value: "1 / 2".into(), percent: Some(50.0), after: None }];
        assert_eq!(draw("memory", f, 0, &plain()), ["Memory: 1 / 2 (50%)"]);
        let f = vec![Fact { key: "".into(), value: "me@box".into(), percent: None, after: None }];
        assert_eq!(draw("title", f, 0, &plain()), ["me@box"]);
        assert_eq!(draw("separator", Vec::new(), 6, &plain()), ["------"]);
    }
}
