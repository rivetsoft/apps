//! Markdown formatting from the keyboard: wrap (or unwrap) the selection, or the word under
//! the cursor, in `**`, `*`, `~~` or backticks; make links; set headings; turn lines into
//! lists, tasks or quotes and back. Positions are in characters, like egui's text cursor.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fmt {
    Bold,
    Italic,
    Strike,
    Code,
    Link,
    /// 1–6, or 0 for plain text.
    Heading(u8),
    Bullet,
    Numbered,
    Task,
    Quote,
    /// Tick or untick the tasks on the selected lines.
    ToggleDone,
    CodeBlock,
}

impl Fmt {
    pub const ALL: [(Fmt, &'static str, &'static str); 15] = [
        (Fmt::Bold, "Bold", "Ctrl+B"),
        (Fmt::Italic, "Italic", "Ctrl+I"),
        (Fmt::Strike, "Strikethrough", "Shift+Ctrl+X"),
        (Fmt::Code, "Inline Code", "Ctrl+E"),
        (Fmt::Link, "Link", "Ctrl+K"),
        (Fmt::Heading(1), "Heading 1", "Ctrl+1"),
        (Fmt::Heading(2), "Heading 2", "Ctrl+2"),
        (Fmt::Heading(3), "Heading 3", "Ctrl+3"),
        (Fmt::Heading(0), "Plain Text", "Ctrl+0"),
        (Fmt::Bullet, "Bulleted List", "Shift+Ctrl+8"),
        (Fmt::Numbered, "Numbered List", "Shift+Ctrl+7"),
        (Fmt::Task, "Task List", "Shift+Ctrl+9"),
        (Fmt::ToggleDone, "Tick Task", "Ctrl+Enter"),
        (Fmt::Quote, "Quote", "Shift+Ctrl+Q"),
        (Fmt::CodeBlock, "Code Block", "Shift+Ctrl+E"),
    ];
}

/// The text with the change made, and the new selection (start, end), in characters.
pub fn apply(text: &str, sel: (usize, usize), f: Fmt) -> (String, (usize, usize)) {
    let chars: Vec<char> = text.chars().collect();
    let (a, b) = (sel.0.min(sel.1).min(chars.len()), sel.0.max(sel.1).min(chars.len()));
    match f {
        Fmt::Bold => wrap(&chars, a, b, "**"),
        Fmt::Italic => wrap(&chars, a, b, "*"),
        Fmt::Strike => wrap(&chars, a, b, "~~"),
        Fmt::Code => wrap(&chars, a, b, "`"),
        Fmt::Link => link(&chars, a, b),
        Fmt::CodeBlock => code_block(&chars, a, b),
        _ => lines(&chars, a, b, f),
    }
}

fn s(c: &[char]) -> String {
    c.iter().collect()
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_' || c == '\'' || c == '-'
}

/// The word around a cursor, if it's in one.
fn word_at(c: &[char], at: usize) -> Option<(usize, usize)> {
    let mut a = at;
    while a > 0 && is_word(c[a - 1]) {
        a -= 1;
    }
    let mut b = at;
    while b < c.len() && is_word(c[b]) {
        b += 1;
    }
    (a < b).then_some((a, b))
}

/// How many of `ch` run up to `at` (backwards) and from `at` (forwards).
fn run(c: &[char], at: usize, ch: char, back: bool) -> usize {
    if back { c[..at].iter().rev().take_while(|x| **x == ch).count() } else { c[at..].iter().take_while(|x| **x == ch).count() }
}

fn wrap(c: &[char], mut a: usize, mut b: usize, m: &str) -> (String, (usize, usize)) {
    let ml = m.chars().count();
    let mc = m.chars().next().unwrap_or('*');
    if a == b {
        match word_at(c, a) {
            Some((wa, wb)) => (a, b) = (wa, wb),
            None => {
                // Nothing to wrap: put the markers in, with the cursor between them.
                let mut out = s(&c[..a]);
                out.push_str(m);
                out.push_str(m);
                out.push_str(&s(&c[a..]));
                return (out, (a + ml, a + ml));
            }
        }
    }
    // Markdown wants the markers against the text: leave outer spaces outside.
    while a < b && c[a].is_whitespace() {
        a += 1;
    }
    while b > a && c[b - 1].is_whitespace() {
        b -= 1;
    }
    // Already wrapped (markers just outside, or the selection includes them)? Unwrap.
    // For * and _ the run's length tells italic (1, 3) from bold (2, 3).
    let has = |n: usize| match (m, n) {
        ("*", 1 | 3) | ("**", 2 | 3) => true,
        ("*" | "**", _) => false,
        (_, n) => n >= ml,
    };
    let (before, after) = (run(c, a, mc, true), run(c, b, mc, false));
    if before == after && before >= ml && has(before) {
        let mut out = s(&c[..a - ml]);
        out.push_str(&s(&c[a..b]));
        out.push_str(&s(&c[b + ml..]));
        return (out, (a - ml, b - ml));
    }
    let inner = &c[a..b];
    let (ib, ia) = (run(inner, 0, mc, false), run(inner, inner.len(), mc, true));
    if inner.len() >= 2 * ml && ib == ia && has(ib) {
        let mut out = s(&c[..a]);
        out.push_str(&s(&c[a + ml..b - ml]));
        out.push_str(&s(&c[b..]));
        return (out, (a, b - 2 * ml));
    }
    let mut out = s(&c[..a]);
    out.push_str(m);
    out.push_str(&s(&c[a..b]));
    out.push_str(m);
    out.push_str(&s(&c[b..]));
    (out, (a + ml, b + ml))
}

fn link(c: &[char], mut a: usize, mut b: usize) -> (String, (usize, usize)) {
    if a == b
        && let Some((wa, wb)) = {
            // A web address under the cursor counts as one word.
            let mut x = a;
            while x > 0 && !c[x - 1].is_whitespace() {
                x -= 1;
            }
            let mut y = a;
            while y < c.len() && !c[y].is_whitespace() {
                y += 1;
            }
            (x < y && s(&c[x..y]).contains("://")).then_some((x, y)).or_else(|| word_at(c, a))
        }
    {
        (a, b) = (wa, wb);
    }
    let picked = s(&c[a..b]);
    let mut out = s(&c[..a]);
    let sel = if picked.contains("://") {
        // A URL: it goes in the brackets' address; the cursor waits for the text.
        out.push_str(&format!("[]({picked})"));
        (a + 1, a + 1)
    } else {
        out.push_str(&format!("[{picked}]()"));
        let at = a + 1 + picked.chars().count() + 2;
        if picked.is_empty() { (a + 1, a + 1) } else { (at, at) }
    };
    out.push_str(&s(&c[b..]));
    (out, sel)
}

fn code_block(c: &[char], a: usize, b: usize) -> (String, (usize, usize)) {
    let ls = c[..a].iter().rposition(|x| *x == '\n').map(|i| i + 1).unwrap_or(0);
    let le = c[b..].iter().position(|x| *x == '\n').map(|i| b + i).unwrap_or(c.len());
    let mut out = s(&c[..ls]);
    out.push_str("```\n");
    out.push_str(&s(&c[ls..le]));
    out.push_str("\n```");
    out.push_str(&s(&c[le..]));
    // The cursor goes after the opening fence, to type the language.
    (out, (ls + 3, ls + 3))
}

/// A line's list or quote marker: (indent, the marker's length after the indent, what it is).
fn marker(line: &[char]) -> (usize, usize, Option<Fmt>) {
    let indent = line.iter().take_while(|c| **c == ' ' || **c == '\t').count();
    let t = s(&line[indent..]);
    for (p, f) in [("- [ ] ", Fmt::Task), ("- [x] ", Fmt::Task), ("- [X] ", Fmt::Task), ("- ", Fmt::Bullet), ("* ", Fmt::Bullet), ("+ ", Fmt::Bullet), ("> ", Fmt::Quote)] {
        if t.starts_with(p) {
            return (indent, p.len(), Some(f));
        }
    }
    let digits = t.chars().take_while(|c| c.is_ascii_digit()).count();
    if digits > 0 && (t[digits..].starts_with(". ") || t[digits..].starts_with(") ")) {
        return (indent, digits + 2, Some(Fmt::Numbered));
    }
    (indent, 0, None)
}

fn heading_len(line: &[char]) -> (usize, u8) {
    let n = line.iter().take_while(|c| **c == '#').count();
    if (1..=6).contains(&n) && line.get(n) == Some(&' ') { (n + 1, n as u8) } else { (0, 0) }
}

/// Changes that work on whole lines: headings, lists, tasks, quotes.
fn lines(c: &[char], a: usize, b: usize, f: Fmt) -> (String, (usize, usize)) {
    // The lines the selection touches: (start, end) in characters, end before the newline.
    let mut spans = Vec::new();
    let mut start = 0;
    for (i, ch) in c.iter().enumerate() {
        if *ch == '\n' {
            spans.push((start, i));
            start = i + 1;
        }
    }
    spans.push((start, c.len()));
    let first = spans.iter().position(|(s0, e)| a >= *s0 && a <= *e).unwrap_or(0);
    let last = spans.iter().position(|(s0, e)| b >= *s0 && b <= *e).unwrap_or(first).max(first);
    // Each touched line: how many characters to drop at its start, and what to put there.
    let chosen = &spans[first..=last];
    let all_have = chosen.iter().all(|(s0, e)| {
        let l = &c[*s0..*e];
        match f {
            Fmt::Heading(n) => heading_len(l).1 == n,
            Fmt::ToggleDone => true,
            _ => marker(l).2 == Some(f),
        }
    });
    let mut edits: Vec<(usize, usize, String)> = Vec::new(); // (line start, chars removed, inserted)
    for (k, (s0, e)) in chosen.iter().enumerate() {
        let l = &c[*s0..*e];
        let (indent, mlen, kind) = marker(l);
        let edit = match f {
            Fmt::Heading(n) => {
                // A heading replaces a list or quote marker.
                let hl = if kind.is_some() { indent + mlen } else { heading_len(l).0 };
                let new = if all_have || n == 0 { String::new() } else { format!("{} ", "#".repeat(n as usize)) };
                (hl, new)
            }
            Fmt::ToggleDone => {
                let t = s(&l[indent..]);
                if t.starts_with("- [ ] ") {
                    (indent + 6, format!("{}- [x] ", s(&l[..indent])))
                } else if t.starts_with("- [x] ") || t.starts_with("- [X] ") {
                    (indent + 6, format!("{}- [ ] ", s(&l[..indent])))
                } else {
                    (0, String::new())
                }
            }
            _ if l.is_empty() && chosen.len() > 1 => (0, String::new()),
            _ if all_have => (indent + mlen, s(&l[..indent])),
            _ => {
                let p = match f {
                    Fmt::Bullet => "- ".to_string(),
                    Fmt::Numbered => format!("{}. ", k + 1),
                    Fmt::Task => "- [ ] ".into(),
                    _ => "> ".into(),
                };
                // Replaces any other list marker the line had.
                let drop = if kind.is_some() { mlen } else { 0 };
                (indent + drop, format!("{}{p}", s(&l[..indent])))
            }
        };
        edits.push((*s0, edit.0, edit.1));
    }
    // Build the text and move the cursor with its line.
    let mut out = String::new();
    let mut pos = 0;
    let map = |p: usize, edits: &[(usize, usize, String)]| -> usize {
        let mut shift: isize = 0;
        for (ls, rem, ins) in edits {
            let ins = ins.chars().count();
            if p >= ls + rem {
                shift += ins as isize - *rem as isize;
            } else if p >= *ls {
                return (*ls as isize + shift) as usize + ins;
            }
        }
        (p as isize + shift).max(0) as usize
    };
    for (ls, rem, ins) in &edits {
        out.push_str(&s(&c[pos..*ls]));
        out.push_str(ins);
        pos = ls + rem;
    }
    out.push_str(&s(&c[pos..]));
    (out, (map(a, &edits), map(b, &edits)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, sel: (usize, usize), f: Fmt) -> (String, (usize, usize)) {
        apply(text, sel, f)
    }

    #[test]
    fn wrapping() {
        // The word under the cursor.
        assert_eq!(run("say hello now", (6, 6), Fmt::Bold), ("say **hello** now".into(), (6, 11)));
        // A selection, spaces left outside.
        assert_eq!(run("say hello now", (3, 10), Fmt::Italic).0, "say *hello* now");
        // Again: unwraps (markers outside the selection, or inside it).
        assert_eq!(run("say **hello** now", (6, 11), Fmt::Bold).0, "say hello now");
        assert_eq!(run("say **hello** now", (4, 13), Fmt::Bold).0, "say hello now");
        // Italic on a bold word adds italic; it doesn't strip a * from the bold.
        assert_eq!(run("**hello**", (2, 7), Fmt::Italic).0, "***hello***");
        assert_eq!(run("***hello***", (3, 8), Fmt::Italic).0, "**hello**");
        // Nowhere to wrap: markers with the cursor between.
        assert_eq!(run("a  b", (2, 2), Fmt::Code), ("a `` b".into(), (3, 3)));
        assert_eq!(run("x", (1, 1), Fmt::Strike).0, "~~x~~");
        // Characters, not bytes.
        assert_eq!(run("é ü", (2, 2), Fmt::Bold).0, "é **ü**");
    }

    #[test]
    fn links() {
        assert_eq!(run("see docs here", (4, 4), Fmt::Link), ("see [docs]() here".into(), (11, 11)));
        assert_eq!(run("go https://x.org now", (5, 5), Fmt::Link), ("go [](https://x.org) now".into(), (4, 4)));
        assert_eq!(run("", (0, 0), Fmt::Link), ("[]()".into(), (1, 1)));
    }

    #[test]
    fn line_formats() {
        assert_eq!(run("Title\nbody", (2, 2), Fmt::Heading(2)), ("## Title\nbody".into(), (5, 5)));
        assert_eq!(run("## Title", (5, 5), Fmt::Heading(2)).0, "Title");
        assert_eq!(run("## Title", (5, 5), Fmt::Heading(1)).0, "# Title");
        assert_eq!(run("- item", (3, 3), Fmt::Heading(2)).0, "## item");
        assert_eq!(run("a\nb\nc", (0, 5), Fmt::Bullet).0, "- a\n- b\n- c");
        assert_eq!(run("- a\n- b", (0, 7), Fmt::Bullet).0, "a\nb");
        assert_eq!(run("- a\n- b", (0, 7), Fmt::Numbered).0, "1. a\n2. b");
        assert_eq!(run("  - a", (4, 4), Fmt::Task).0, "  - [ ] a");
        assert_eq!(run("- [ ] a\n- [x] b", (0, 15), Fmt::ToggleDone).0, "- [x] a\n- [ ] b");
        assert_eq!(run("quote me", (0, 0), Fmt::Quote), ("> quote me".into(), (2, 2)));
        assert_eq!(run("a\nfn x()\nb", (3, 3), Fmt::CodeBlock), ("a\n```\nfn x()\n```\nb".into(), (5, 5)));
    }
}
