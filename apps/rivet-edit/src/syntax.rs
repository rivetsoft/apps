//! Syntax highlighting. Tree-sitter parses the languages it has a grammar for here (Rust,
//! Python, JavaScript, TypeScript and TSX, JSON, shell, C, Go, HTML, CSS, TOML, YAML); every
//! other file gets the shared highlighter Rivet Files' Peek and Rivet Notes use. Colors come
//! from the theme (`rivet_look::highlight::Colors`), so they follow the desktop's palette.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;
use std::sync::OnceLock;

use eframe::egui::{Color32, FontId, text::LayoutJob};
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
pub fn spans(text: &str, lang: Lang, colors: &Colors) -> Option<Vec<(Range<usize>, Color32)>> {
    let config = configs().get(&lang)?;
    let mut hl = Highlighter::new();
    let events = hl.highlight(config, text.as_bytes(), None, None, |_| None).ok()?;
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

/// What the editor shows a file's text with: the layout, cached until the text, the colors or
/// the font change.
#[derive(Default)]
pub struct Cache {
    key: u64,
    job: Option<LayoutJob>,
}

impl Cache {
    /// The laid-out (not yet wrapped) text of `name` in `font`.
    pub fn job(&mut self, text: &str, name: &str, pick: Pick, colors: &Colors, font: &FontId) -> LayoutJob {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        text.hash(&mut h);
        name.hash(&mut h);
        pick.hash(&mut h);
        font.size.to_bits().hash(&mut h);
        colors.hash(&mut h);
        let key = h.finish();
        if self.key == key
            && let Some(j) = &self.job
        {
            return j.clone();
        }
        let job = build(text, name, pick, colors, font);
        self.key = key;
        self.job = Some(job.clone());
        job
    }
}

/// Very large files are shown plain (highlighting them would slow typing down).
const MAX_HIGHLIGHT: usize = 1_500_000;

fn build(text: &str, name: &str, pick: Pick, colors: &Colors, font: &FontId) -> LayoutJob {
    let plain = || {
        let mut j = LayoutJob::default();
        j.append(text, 0.0, eframe::egui::TextFormat { font_id: font.clone(), color: colors.text, ..Default::default() });
        j
    };
    if text.len() > MAX_HIGHLIGHT || pick == Pick::Plain {
        return plain();
    }
    let lang = match pick {
        Pick::Lang(l) => Some(l),
        _ => detect(name, text),
    };
    if let Some(lang) = lang
        && let Some(sp) = spans(text, lang, colors)
    {
        let mut j = LayoutJob { text: text.to_string(), ..Default::default() };
        let mut covered = 0;
        for (r, c) in sp {
            if r.start < covered || r.end > text.len() || !text.is_char_boundary(r.start) || !text.is_char_boundary(r.end) {
                continue;
            }
            if r.start > covered {
                j.sections.push(section(covered..r.start, colors.text, font));
            }
            j.sections.push(section(r.clone(), c, font));
            covered = r.end;
        }
        if covered < text.len() {
            j.sections.push(section(covered..text.len(), colors.text, font));
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
    fn languages_by_file_name() {
        assert_eq!(Lang::for_file("main.RS"), Some(Lang::Rust));
        assert_eq!(Lang::for_file("Cargo.lock"), Some(Lang::Toml));
        assert_eq!(Lang::for_file("README.md"), None);
        assert_eq!(language_name("README.md"), "Markdown");
    }
}
