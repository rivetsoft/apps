//! Syntax highlighting. Tree-sitter parses the languages it has a grammar for here (Rust,
//! Python, JavaScript, TypeScript and TSX, JSON, shell, C, Go, HTML, CSS, TOML, YAML); every
//! other file gets the shared highlighter Rivet Files' Peek and Rivet Notes use. Colors come
//! from the theme (`rivet_look::highlight::Colors`), so they follow the desktop's palette.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock, mpsc};

use eframe::egui::{Color32, FontId, Galley, Ui, text::LayoutJob};
use rivet_look::highlight::{self, Colors};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

/// The capture names the queries use, in the order `configure` numbers them.
const NAMES: &[&str] = &[
    "attribute",
    "boolean",
    "character",
    "comment",
    "constant",
    "constant.builtin",
    "constructor",
    "embedded",
    "escape",
    "float",
    "function",
    "function.builtin",
    "function.macro",
    "function.method",
    "keyword",
    "label",
    "module",
    "number",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "punctuation.special",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
];

/// A language tree-sitter knows here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Lang {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Tsx,
    Json,
    Bash,
    C,
    Go,
    Html,
    Css,
    Toml,
    Yaml,
}

/// The language a tab is shown as: told from the file (the default), plain text, or one
/// chosen by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Pick {
    #[default]
    Auto,
    Plain,
    Lang(Lang),
}

/// The language of a file: by its name, else by its first line (a `#!` line naming python,
/// node or a shell; `<!DOCTYPE html>`).
pub fn detect(name: &str, text: &str) -> Option<Lang> {
    if let Some(l) = Lang::for_file(name) {
        return Some(l);
    }
    let first = text.lines().next().unwrap_or("").trim().to_lowercase();
    if let Some(bang) = first.strip_prefix("#!") {
        let prog = bang.split_whitespace().flat_map(|w| w.rsplit('/').next()).find(|w| *w != "env" && !w.starts_with('-')).unwrap_or("");
        return match prog.trim_end_matches(|c: char| c.is_ascii_digit() || c == '.') {
            "python" | "pypy" => Some(Lang::Python),
            "node" | "deno" | "bun" | "nodejs" => Some(Lang::JavaScript),
            "sh" | "bash" | "zsh" | "dash" | "ksh" => Some(Lang::Bash),
            _ => None,
        };
    }
    if first.starts_with("<!doctype html") || first.starts_with("<html") {
        return Some(Lang::Html);
    }
    None
}

/// What the status bar calls a tab's language.
pub fn shown_name(name: &str, text: &str, pick: Pick) -> &'static str {
    match pick {
        Pick::Plain => "Plain Text",
        Pick::Lang(l) => l.name(),
        Pick::Auto => match detect(name, text) {
            Some(l) => l.name(),
            None => language_name(name),
        },
    }
}

impl Lang {
    /// The line comment (Ctrl+/), or none.
    pub fn comment(self) -> Option<&'static str> {
        match self {
            Lang::Python | Lang::Bash | Lang::Toml | Lang::Yaml => Some("#"),
            Lang::Json | Lang::Html | Lang::Css => None,
            _ => Some("//"),
        }
    }

    pub const ALL: [Lang; 13] = [Lang::Rust, Lang::Python, Lang::JavaScript, Lang::TypeScript, Lang::Tsx, Lang::Json, Lang::Bash, Lang::C, Lang::Go, Lang::Html, Lang::Css, Lang::Toml, Lang::Yaml];

    pub fn name(self) -> &'static str {
        match self {
            Lang::Rust => "Rust",
            Lang::Python => "Python",
            Lang::JavaScript => "JavaScript",
            Lang::TypeScript => "TypeScript",
            Lang::Tsx => "TSX",
            Lang::Json => "JSON",
            Lang::Bash => "Shell",
            Lang::C => "C",
            Lang::Go => "Go",
            Lang::Html => "HTML",
            Lang::Css => "CSS",
            Lang::Toml => "TOML",
            Lang::Yaml => "YAML",
        }
    }

    /// The language a file is, by its name.
    pub fn for_file(name: &str) -> Option<Lang> {
        let lower = name.to_lowercase();
        let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("");
        Some(match ext {
            "rs" => Lang::Rust,
            "py" | "pyw" | "pyi" => Lang::Python,
            "js" | "mjs" | "cjs" | "jsx" => Lang::JavaScript,
            "ts" | "mts" | "cts" => Lang::TypeScript,
            "tsx" => Lang::Tsx,
            "json" | "jsonc" | "geojson" => Lang::Json,
            "sh" | "bash" | "zsh" | "ksh" => Lang::Bash,
            "c" | "h" => Lang::C,
            "go" => Lang::Go,
            "html" | "htm" | "xhtml" => Lang::Html,
            "css" => Lang::Css,
            "toml" => Lang::Toml,
            "yaml" | "yml" => Lang::Yaml,
            _ => match lower.as_str() {
                ".bashrc" | ".zshrc" | ".profile" | "pkgbuild" => Lang::Bash,
                "cargo.lock" => Lang::Toml,
                _ => return None,
            },
        })
    }

    fn config(self) -> Option<HighlightConfiguration> {
        let (lang, name, hl): (tree_sitter_language::LanguageFn, &str, String) = match self {
            Lang::Rust => (tree_sitter_rust::LANGUAGE, "rust", tree_sitter_rust::HIGHLIGHTS_QUERY.into()),
            Lang::Python => (tree_sitter_python::LANGUAGE, "python", tree_sitter_python::HIGHLIGHTS_QUERY.into()),
            Lang::JavaScript => (tree_sitter_javascript::LANGUAGE, "javascript", format!("{}\n{}", tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY)),
            // TypeScript's queries add to JavaScript's.
            Lang::TypeScript => (tree_sitter_typescript::LANGUAGE_TYPESCRIPT, "typescript", format!("{}\n{}", tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY)),
            Lang::Tsx => (tree_sitter_typescript::LANGUAGE_TSX, "tsx", format!("{}\n{}\n{}", tree_sitter_typescript::HIGHLIGHTS_QUERY, tree_sitter_javascript::HIGHLIGHT_QUERY, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY)),
            Lang::Json => (tree_sitter_json::LANGUAGE, "json", tree_sitter_json::HIGHLIGHTS_QUERY.into()),
            Lang::Bash => (tree_sitter_bash::LANGUAGE, "bash", tree_sitter_bash::HIGHLIGHT_QUERY.into()),
            Lang::C => (tree_sitter_c::LANGUAGE, "c", tree_sitter_c::HIGHLIGHT_QUERY.into()),
            Lang::Go => (tree_sitter_go::LANGUAGE, "go", tree_sitter_go::HIGHLIGHTS_QUERY.into()),
            Lang::Html => (tree_sitter_html::LANGUAGE, "html", tree_sitter_html::HIGHLIGHTS_QUERY.into()),
            Lang::Css => (tree_sitter_css::LANGUAGE, "css", tree_sitter_css::HIGHLIGHTS_QUERY.into()),
            Lang::Toml => (tree_sitter_toml_ng::LANGUAGE, "toml", tree_sitter_toml_ng::HIGHLIGHTS_QUERY.into()),
            Lang::Yaml => (tree_sitter_yaml::LANGUAGE, "yaml", tree_sitter_yaml::HIGHLIGHTS_QUERY.into()),
        };
        let mut c = HighlightConfiguration::new(lang.into(), name, &hl, "", "").ok()?;
        c.configure(NAMES);
        Some(c)
    }
}

/// The grammars, set up once (on first use) and kept.
fn configs() -> &'static HashMap<Lang, HighlightConfiguration> {
    static C: OnceLock<HashMap<Lang, HighlightConfiguration>> = OnceLock::new();
    C.get_or_init(|| Lang::ALL.iter().filter_map(|l| l.config().map(|c| (*l, c))).collect())
}

/// The color of a capture.
fn color(name: &str, c: &Colors) -> Color32 {
    let head = name.split('.').next().unwrap_or(name);
    match (head, name) {
        ("comment", _) => c.comment,
        (_, "variable.builtin") | ("keyword" | "label" | "tag", _) => c.keyword,
        ("string" | "escape" | "character", _) => c.string,
        ("number" | "float" | "boolean" | "constant", _) => c.number,
        ("function" | "constructor", _) => c.function,
        ("type" | "attribute" | "module", _) => c.type_,
        ("punctuation" | "operator", _) => c.punct,
        _ => c.text,
    }
}

/// Colored byte ranges of `text`, by tree-sitter (None when it has no grammar for `lang`).
#[cfg(test)]
pub fn spans(text: &str, lang: Lang, colors: &Colors) -> Option<Vec<(Range<usize>, Color32)>> {
    spans_unless(text, lang, colors, None)
}

/// [`spans`], giving up (None) once `cancel` is set.
fn spans_unless(text: &str, lang: Lang, colors: &Colors, cancel: Option<&AtomicUsize>) -> Option<Vec<(Range<usize>, Color32)>> {
    let config = configs().get(&lang)?;
    let mut hl = Highlighter::new();
    let events = hl.highlight(config, text.as_bytes(), None, cancel, |_| None).ok()?;
    let mut out = Vec::new();
    let mut stack: Vec<Color32> = Vec::new();
    for ev in events {
        match ev.ok()? {
            HighlightEvent::HighlightStart(h) => stack.push(color(NAMES.get(h.0).copied().unwrap_or(""), colors)),
            HighlightEvent::HighlightEnd => {
                stack.pop();
            }
            HighlightEvent::Source { start, end } => out.push((start..end, stack.last().copied().unwrap_or(colors.text))),
        }
    }
    Some(out)
}

/// What the editor shows a file's text with: the laid-out text, kept until the text, the colors,
/// the font, the wrap width or the font atlas change.
///
/// The text box asks for its layout every frame. Building the colored job again, or even handing
/// egui the same job to look up in its own cache, costs time in proportion to the file (tens of
/// milliseconds a frame for a large one), so the galley is kept here and handed back as it is.
///
/// Coloring a large file takes longer than a frame (tree-sitter manages about 1.5 MB a second),
/// so after an edit it keeps the colors it had, moved along past the change, and colors the new
/// text on another thread.
#[derive(Default)]
pub struct Cache {
    /// The file name, language, colors and font it was built with (not the text: the galley
    /// holds that, and comparing it is far quicker than hashing it).
    style: u64,
    wrap: f32,
    pixels_per_point: f32,
    /// How full egui's glyph atlas was after laying it out. The atlas only fills up until egui
    /// starts it over, which leaves this galley pointing at glyphs that are no longer there.
    atlas_fill: f32,
    galley: Option<Arc<Galley>>,
    /// The galley's colors are the old ones moved along, until the worker's arrive.
    approx: bool,
    worker: Option<Worker>,
}

/// Files bigger than this are colored on another thread (smaller ones take under ~20 ms).
const SYNC_HIGHLIGHT: usize = 32 * 1024;

/// A thread coloring the text in one style; dropping it stops the thread.
struct Worker {
    style: u64,
    done: mpsc::Receiver<LayoutJob>,
    cancel: Arc<AtomicUsize>,
}

impl Drop for Worker {
    fn drop(&mut self) {
        self.cancel.store(1, Ordering::Relaxed);
    }
}

impl Cache {
    /// The laid-out text of `name` in `font`, wrapped at `wrap_width`. `line_height` is the
    /// space each line takes, as a multiple of the font's own.
    #[allow(clippy::too_many_arguments)]
    pub fn galley(&mut self, ui: &Ui, text: &str, name: &str, pick: Pick, colors: &Colors, font: &FontId, line_height: f32, wrap_width: f32) -> Arc<Galley> {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut h);
        pick.hash(&mut h);
        font.hash(&mut h);
        line_height.to_bits().hash(&mut h);
        colors.hash(&mut h);
        let style = h.finish();
        // The worker's colors, when they're in (and still in this style).
        let mut fresh = None;
        if let Some(w) = &self.worker {
            match w.done.try_recv() {
                Ok(job) => {
                    fresh = (w.style == style).then_some(job);
                    self.worker = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.worker = None,
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        let pixels_per_point = ui.ctx().pixels_per_point();
        let atlas_fill = ui.fonts_mut(|f| f.font_atlas_fill_ratio());
        let same_job = self.style == style && self.galley.as_ref().is_some_and(|g| g.text() == text);
        if fresh.is_none()
            && same_job
            && self.wrap == wrap_width
            && self.pixels_per_point == pixels_per_point
            && atlas_fill >= self.atlas_fill
            && let Some(g) = &self.galley
        {
            return g.clone();
        }
        let (mut job, approx) = match fresh {
            Some(f) if f.text == text => (f, false),
            fresh => match &self.galley {
                // Only rewrapped or laid out again: the same colors.
                Some(g) if same_job && fresh.is_none() => ((*g.job).clone(), self.approx),
                _ if text.len() > SYNC_HIGHLIGHT && text.len() <= MAX_HIGHLIGHT && pick != Pick::Plain => {
                    // The newest real colors to move along: the worker's, else what's shown.
                    let base = fresh.as_ref().or(self.galley.as_ref().map(|g| &*g.job));
                    (base.map(|b| shifted(b, text, font, colors.text)).unwrap_or_else(|| plain(text, font, colors.text)), true)
                }
                _ => (build(text, name, pick, colors, font, None), false),
            },
        };
        if !approx {
            self.worker = None;
        } else if self.worker.as_ref().is_none_or(|w| w.style != style) {
            // One worker at a time: when it finishes on text that has changed since, its
            // colors are moved along and another starts.
            self.worker = spawn(ui.ctx(), style, text, name, pick, colors, font);
        }
        job.wrap.max_width = wrap_width;
        // Taller lines: the extra space is shared above and below the text.
        let row = (line_height > 1.0).then(|| (ui.fonts_mut(|f| f.row_height(font)) * line_height).round());
        if job.sections.first().is_some_and(|s| s.format.line_height != row) {
            for s in &mut job.sections {
                s.format.line_height = row;
            }
        }
        let galley = ui.fonts_mut(|f| f.layout_job(job));
        self.style = style;
        self.wrap = wrap_width;
        self.pixels_per_point = pixels_per_point;
        self.atlas_fill = ui.fonts_mut(|f| f.font_atlas_fill_ratio());
        self.galley = Some(galley.clone());
        self.approx = approx;
        galley
    }
}

/// Color `text` on another thread, asking for a repaint when it's done.
fn spawn(ctx: &eframe::egui::Context, style: u64, text: &str, name: &str, pick: Pick, colors: &Colors, font: &FontId) -> Option<Worker> {
    let (tx, done) = mpsc::channel();
    let cancel = Arc::new(AtomicUsize::new(0));
    let (ctx, text, name, colors, font, stop) = (ctx.clone(), text.to_string(), name.to_string(), *colors, font.clone(), cancel.clone());
    std::thread::Builder::new()
        .name("rivet-edit-colors".into())
        .spawn(move || {
            let job = build(&text, &name, pick, &colors, &font, Some(&stop));
            if stop.load(Ordering::Relaxed) == 0 && tx.send(job).is_ok() {
                ctx.request_repaint();
            }
        })
        .ok()?;
    Some(Worker { style, done, cancel })
}

/// `base`'s colors on `text`, an edit of `base`'s text: runs before the change stay, runs after
/// it move along with it, and what was typed takes the color of the run it carries on from.
fn shifted(base: &LayoutJob, text: &str, font: &FontId, plain: Color32) -> LayoutJob {
    let (old, new) = (base.text.as_bytes(), text.as_bytes());
    // What's the same at the start and at the end, in whole characters (the same bytes end a
    // character in both texts).
    let mut pre = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    while !text.is_char_boundary(pre) {
        pre -= 1;
    }
    let room = old.len().min(new.len()) - pre;
    let mut suf = old.iter().rev().zip(new.iter().rev()).take(room).take_while(|(a, b)| a == b).count();
    while !text.is_char_boundary(new.len() - suf) {
        suf -= 1;
    }
    let (old_end, new_end) = (old.len() - suf, new.len() - suf);
    let mut j = LayoutJob { text: text.to_string(), sections: Vec::with_capacity(base.sections.len() + 2), ..Default::default() };
    let mut covered = 0;
    // The old runs are joined already: only same-colored neighbours (around the change) join.
    let mut add = |j: &mut LayoutJob, r: Range<usize>, c: Color32| {
        let r = r.start.max(covered)..r.end.min(text.len());
        if r.start >= r.end {
            return;
        }
        for (r, c) in [(covered..r.start, plain), (r.clone(), c)] {
            if r.is_empty() {
                continue;
            }
            match j.sections.last_mut() {
                Some(last) if last.format.color == c => last.byte_range.end.0 = r.end,
                _ => j.sections.push(section(r, c, font)),
            }
        }
        covered = r.end;
    };
    let mut typed = plain;
    for s in &base.sections {
        let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
        if a < pre {
            add(&mut j, a..b.min(pre), s.format.color);
            if b >= pre {
                typed = s.format.color;
            }
        }
    }
    add(&mut j, pre..new_end, typed);
    for s in &base.sections {
        let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
        if b > old_end {
            add(&mut j, a.max(old_end) - old_end + new_end..b - old_end + new_end, s.format.color);
        }
    }
    // Anything the old runs didn't reach.
    add(&mut j, 0..text.len(), plain);
    j
}

fn plain(text: &str, font: &FontId, color: Color32) -> LayoutJob {
    let mut j = LayoutJob::default();
    j.append(text, 0.0, eframe::egui::TextFormat { font_id: font.clone(), color, ..Default::default() });
    j
}

/// Very large files are shown plain (highlighting them would slow typing down).
const MAX_HIGHLIGHT: usize = 1_500_000;

fn build(text: &str, name: &str, pick: Pick, colors: &Colors, font: &FontId, cancel: Option<&AtomicUsize>) -> LayoutJob {
    let plain = || plain(text, font, colors.text);
    if text.len() > MAX_HIGHLIGHT || pick == Pick::Plain {
        return plain();
    }
    let lang = match pick {
        Pick::Lang(l) => Some(l),
        _ => detect(name, text),
    };
    if let Some(lang) = lang
        && let Some(sp) = spans_unless(text, lang, colors, cancel)
    {
        let mut j = LayoutJob { text: text.to_string(), ..Default::default() };
        let mut covered = 0;
        for (r, c) in sp {
            if r.start < covered || r.end > text.len() || !text.is_char_boundary(r.start) || !text.is_char_boundary(r.end) {
                continue;
            }
            if r.start > covered {
                push(&mut j, text, covered..r.start, colors.text, font);
            }
            push(&mut j, text, r.clone(), c, font);
            covered = r.end;
        }
        if covered < text.len() {
            push(&mut j, text, covered..text.len(), colors.text, font);
        }
        return j;
    }
    if lang.is_some() {
        return plain();
    }
    // Everything else: the shared highlighter.
    let mut j = highlight::layout(text, name, colors, font.clone());
    j.wrap = Default::default();
    j
}

/// Add a colored run, joined to the one before when it can be: the same color, or either of
/// them only spaces and line breaks (whose color doesn't show). Fewer, longer runs lay out and
/// compare quicker; a source file has about one run per token otherwise.
fn push(j: &mut LayoutJob, text: &str, r: Range<usize>, color: Color32, font: &FontId) {
    let blank = |r: Range<usize>| text.as_bytes()[r].iter().all(u8::is_ascii_whitespace);
    if let Some(last) = j.sections.last_mut()
        && last.byte_range.end.0 == r.start
    {
        if last.format.color == color || blank(r.clone()) {
            last.byte_range.end.0 = r.end;
            return;
        }
        if blank(last.byte_range.start.0..last.byte_range.end.0) {
            last.format.color = color;
            last.byte_range.end.0 = r.end;
            return;
        }
    }
    j.sections.push(section(r, color, font));
}

fn section(r: Range<usize>, color: Color32, font: &FontId) -> eframe::egui::text::LayoutSection {
    eframe::egui::text::LayoutSection {
        leading_space: 0.0,
        byte_range: eframe::egui::text::ByteIndex(r.start)..eframe::egui::text::ByteIndex(r.end),
        format: eframe::egui::TextFormat { font_id: font.clone(), color, ..Default::default() },
    }
}

/// The language shown in the status bar.
pub fn language_name(name: &str) -> &'static str {
    if let Some(l) = Lang::for_file(name) {
        return l.name();
    }
    let lower = name.to_lowercase();
    match lower.rsplit_once('.').map(|(_, e)| e).unwrap_or("") {
        "md" | "markdown" => "Markdown",
        "java" => "Java",
        "kt" | "kts" => "Kotlin",
        "cpp" | "cc" | "cxx" | "hpp" => "C++",
        "cs" => "C#",
        "rb" => "Ruby",
        "php" => "PHP",
        "lua" => "Lua",
        "sql" => "SQL",
        "xml" | "svg" => "XML",
        "ini" | "conf" | "cfg" => "Config",
        "txt" | "" => "Plain text",
        _ => "Text",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn colors() -> Colors {
        highlight::colors(Color32::WHITE, Color32::BLACK, Color32::from_rgb(200, 100, 255), Color32::from_rgb(100, 200, 255), Color32::from_rgb(255, 200, 100))
    }

    #[test]
    fn every_grammar_loads_and_colors_code() {
        let c = colors();
        for (lang, src) in [
            (Lang::Rust, "fn main() { let x = 1; // hi\n}"),
            (Lang::Python, "def f(x):\n    return 'a'  # hi\n"),
            (Lang::JavaScript, "const x = 'a'; // hi\nfunction f() {}"),
            (Lang::TypeScript, "let x: number = 1; // hi"),
            (Lang::Tsx, "const x = <div>hi</div>;"),
            (Lang::Json, "{\"a\": 1, \"b\": true}"),
            (Lang::Bash, "echo \"hi\" # hi\n"),
            (Lang::C, "int main(void) { return 0; } /* hi */"),
            (Lang::Go, "package main\nfunc main() {} // hi"),
            (Lang::Html, "<p class=\"a\">hi</p>"),
            (Lang::Css, "a { color: red; }"),
            (Lang::Toml, "[a]\nb = 1 # hi\n"),
            (Lang::Yaml, "a: 1 # hi\n"),
        ] {
            let s = spans(src, lang, &c).unwrap_or_else(|| panic!("{} didn't load", lang.name()));
            assert!(s.iter().any(|(_, col)| *col != c.text), "{} colored nothing", lang.name());
            assert_eq!(s.last().unwrap().0.end, src.len(), "{} didn't cover the text", lang.name());
        }
        let r = spans("fn main() {} // note", Lang::Rust, &c).unwrap();
        assert!(r.iter().any(|(rg, col)| &"fn main() {} // note"[rg.clone()] == "fn" && *col == c.keyword));
        assert!(r.iter().any(|(rg, col)| "fn main() {} // note"[rg.clone()].contains("note") && *col == c.comment));
    }

    #[test]
    fn runs_join_across_spaces_and_keep_their_colors() {
        let c = colors();
        let text = "fn main() {\n    let x = 1; // hi\n}\n";
        let j = build(text, "a.rs", Pick::Auto, &c, &FontId::monospace(14.0), None);
        // Every byte is in exactly one run, in order.
        let mut at = 0;
        for s in &j.sections {
            assert_eq!(s.byte_range.start.0, at);
            at = s.byte_range.end.0;
        }
        assert_eq!(at, text.len());
        let color_of = |word: &str| {
            let i = text.find(word).unwrap();
            j.sections.iter().find(|s| s.byte_range.start.0 <= i && i < s.byte_range.end.0).unwrap().format.color
        };
        assert_eq!(color_of("fn"), c.keyword);
        assert_eq!(color_of("let"), c.keyword);
        assert_eq!(color_of("// hi"), c.comment);
        // Spaces never start a run of their own.
        assert!(j.sections.iter().all(|s| !text[s.byte_range.start.0..s.byte_range.end.0].trim().is_empty()));
    }

    /// Every byte in exactly one run, in order.
    fn covers(j: &LayoutJob) {
        let mut at = 0;
        for s in &j.sections {
            assert_eq!(s.byte_range.start.0, at, "runs of {:?}", j.text);
            assert!(s.byte_range.end.0 > at);
            at = s.byte_range.end.0;
        }
        assert_eq!(at, j.text.len());
    }

    fn color_at(j: &LayoutJob, word: &str) -> Color32 {
        let i = j.text.find(word).unwrap();
        j.sections.iter().find(|s| s.byte_range.start.0 <= i && i < s.byte_range.end.0).unwrap().format.color
    }

    #[test]
    fn colors_move_along_past_an_edit() {
        let c = colors();
        let font = FontId::monospace(14.0);
        let before = "fn main() {\n    let x = \"héllo\"; // hi\n}\n";
        let base = build(before, "a.rs", Pick::Auto, &c, &font, None);
        let edits = [
            // Typing in the middle, deleting, a multibyte character, at either end, all of it.
            before.replace("let x", "let xyz"),
            before.replace("let x = ", ""),
            before.replace("héllo", "hé€llo"),
            format!("// top\n{before}"),
            format!("{before}fn more() {{}}\n"),
            String::new(),
            "something else entirely".to_string(),
        ];
        for after in &edits {
            let j = shifted(&base, after, &font, c.text);
            assert_eq!(&j.text, after);
            if !after.is_empty() {
                covers(&j);
            }
            for (word, col) in [("fn main", c.keyword), ("// hi", c.comment), ("}", c.punct)] {
                if after.contains(word) {
                    assert_eq!(color_at(&j, word), col, "{word:?} in {after:?}");
                }
            }
        }
        // What's typed carries on the run before it.
        let j = shifted(&base, &before.replace("// hi", "// hi there"), &font, c.text);
        assert_eq!(color_at(&j, "there"), c.comment);
    }

    #[test]
    fn languages_by_file_name() {
        assert_eq!(Lang::for_file("main.RS"), Some(Lang::Rust));
        assert_eq!(Lang::for_file("Cargo.lock"), Some(Lang::Toml));
        assert_eq!(Lang::for_file("README.md"), None);
        assert_eq!(language_name("README.md"), "Markdown");
    }
}

