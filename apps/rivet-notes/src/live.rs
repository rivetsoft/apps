//! Live mode: the note is still Markdown text, but every line except the one being edited
//! shows rendered. This works out, for each byte of the text, how it looks: which Markdown
//! marks hide (or show faintly, on the line being edited), which text is bold, a heading, a
//! link, code… plus the things drawn over the text (bullets, check boxes, quote bars, rules).

use std::ops::Range;

/// How a Markdown mark is hidden when its line isn't being edited.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Hide {
    #[default]
    No,
    /// Squeezed to almost nothing.
    Tiny,
    /// Invisible but keeping its width (something is drawn in its place).
    Clear,
}

/// How one byte of the text looks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct St {
    /// Heading level (1-6) of the line, 0 for none.
    pub heading: u8,
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    /// `inline code`.
    pub code: bool,
    /// Inside a fenced code block.
    pub block: bool,
    pub link: bool,
    pub tag: bool,
    pub quote: bool,
    /// A finished task's text.
    pub done: bool,
    /// A Markdown mark shown faintly (on the line being edited).
    pub marker: bool,
    pub hide: Hide,
    /// The whole line folds away (a code fence off the line being edited).
    pub collapse: bool,
}

/// Something drawn over the text; positions are character indices.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Deco {
    /// A bullet in place of the list mark at this character.
    Bullet(usize),
    /// A check box over `[ ]` starting at this character; `byte` is the space or x inside.
    Check { at: usize, done: bool, byte: usize },
    /// A quote bar at this character (the `>`).
    Quote(usize),
    /// A horizontal rule on the line starting at this character.
    Rule(usize),
}

/// A fenced code block's text and language, for coloring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub inner: Range<usize>,
    pub lang: String,
}

pub struct Live {
    pub st: Vec<St>,
    pub decos: Vec<Deco>,
    pub blocks: Vec<Block>,
}

/// Work out how `text` looks. `raw` is the bytes being edited (the cursor or selection): the
/// lines it touches keep their marks (shown faintly), and a code block it touches keeps its
/// fences.
/// A line's heading level (1-6), or 0.
pub fn heading_level(line: &str) -> u8 {
    let t = line.trim_start();
    let n = t.bytes().take_while(|b| *b == b'#').count();
    if (1..=6).contains(&n) && t[n..].starts_with(' ') { n as u8 } else { 0 }
}

/// `held` is the line being edited (by index) and the heading level it had when the cursor came
/// to it: that line shows as plain Markdown at that size until the cursor leaves it.
pub fn analyze(text: &str, raw: Option<(usize, usize)>, held: Option<(usize, u8)>) -> Live {
    let mut st = vec![St::default(); text.len()];
    let mut decos = Vec::new();
    let mut blocks = Vec::new();
    let touches = |r: &Range<usize>| raw.is_some_and(|(a, b)| r.start <= b && a <= r.end.saturating_sub(1).max(r.start));
    // Lines with their byte ranges (the newline included).
    let mut lines: Vec<Range<usize>> = Vec::new();
    let mut pos = 0;
    for l in text.split_inclusive('\n') {
        lines.push(pos..pos + l.len());
        pos += l.len();
    }
    // Character index of every byte, for decorations.
    let char_at = |b: usize| text[..b].chars().count();
    let mut i = 0;
    while i < lines.len() {
        let r = lines[i].clone();
        let line = &text[r.clone()];
        let body = line.trim_end_matches(['\n', '\r']);
        let indent = body.len() - body.trim_start().len();
        let t = body.trim_start();
        // ---- fenced code: find the closing fence, then the whole block at once
        if t.starts_with("```") || t.starts_with("~~~") {
            let fence = &t[..3];
            let lang = t[3..].trim().to_string();
            let mut j = i + 1;
            while j < lines.len() && !text[lines[j].clone()].trim_start().starts_with(fence) {
                j += 1;
            }
            let end = if j < lines.len() { j } else { lines.len() - 1 };
            let whole = r.start..lines[end].end;
            let editing = touches(&whole);
            for (k, lr) in lines.iter().enumerate().take(end + 1).skip(i) {
                let is_fence = k == i || (k == end && j < lines.len());
                for s in &mut st[lr.clone()] {
                    s.block = true;
                    if is_fence {
                        if editing {
                            s.marker = true;
                        } else {
                            s.hide = Hide::Tiny;
                            s.collapse = true;
                        }
                    }
                }
            }
            let inner_start = lines.get(i + 1).map(|l| l.start).unwrap_or(r.end).min(lines[end].end);
            let inner_end = if j < lines.len() { lines[end].start } else { lines[end].end };
            if inner_start < inner_end {
                blocks.push(Block { inner: inner_start..inner_end, lang });
            }
            i = end + 1;
            continue;
        }
        let editing = touches(&r);
        // The line being edited: plain Markdown, at the size it had (nothing re-renders while
        // typing on it).
        if editing {
            let level = match held {
                Some((li, h)) if li == i => h,
                _ => heading_level(body),
            };
            for s in &mut st[r.clone()] {
                s.heading = level;
            }
            i += 1;
            continue;
        }
        let at = r.start + indent;
        let mark = |st: &mut [St], range: Range<usize>, hide: Hide| {
            for s in &mut st[range] {
                if editing {
                    s.marker = true;
                } else {
                    s.hide = hide;
                }
            }
        };
        let mut rest = at; // where the line's own text starts
        // ---- horizontal rule
        let rule = t.len() >= 3 && ["-", "*", "_"].iter().any(|c| t.chars().all(|x| x.to_string() == *c || x == ' ') && t.matches(c).count() >= 3);
        if rule && !editing {
            for s in &mut st[r.clone()] {
                s.hide = Hide::Clear;
            }
            decos.push(Deco::Rule(char_at(r.start)));
            i += 1;
            continue;
        }
        // ---- heading
        let hashes = t.bytes().take_while(|b| *b == b'#').count();
        if (1..=6).contains(&hashes) && t[hashes..].starts_with(' ') {
            for s in &mut st[r.clone()] {
                s.heading = hashes as u8;
            }
            mark(&mut st, at..at + hashes + 1, Hide::Tiny);
            rest = at + hashes + 1;
        } else if let Some(q) = t.strip_prefix('>') {
            // ---- quote
            for s in &mut st[r.clone()] {
                s.quote = true;
            }
            let n = if q.starts_with(' ') { 2 } else { 1 };
            mark(&mut st, at..at + 1, Hide::Clear);
            if n == 2 {
                mark(&mut st, at + 1..at + 2, Hide::No);
            }
            if !editing {
                decos.push(Deco::Quote(char_at(at)));
            }
            rest = at + n;
        } else if t.len() >= 2 && matches!(t.as_bytes()[0], b'-' | b'*' | b'+') && t.as_bytes()[1] == b' ' {
            // ---- list item, maybe a task
            let after = &t[2..];
            let task = after.len() >= 3 && after.starts_with('[') && matches!(after.as_bytes()[1], b' ' | b'x' | b'X') && after.as_bytes()[2] == b']' && (after.len() == 3 || after.as_bytes()[3] == b' ');
            if task {
                let done = after.as_bytes()[1] != b' ';
                mark(&mut st, at..at + 2, Hide::Tiny);
                mark(&mut st, at + 2..at + 5, Hide::Clear);
                if !editing {
                    decos.push(Deco::Check { at: char_at(at + 2), done, byte: at + 3 });
                }
                rest = (at + 6).min(r.start + body.len());
                if done {
                    for s in &mut st[rest..r.start + body.len()] {
                        s.done = true;
                    }
                }
            } else {
                mark(&mut st, at..at + 1, Hide::Clear);
                if !editing {
                    decos.push(Deco::Bullet(char_at(at)));
                }
                rest = at + 2;
            }
        }
        inline(text, &mut st, rest..r.start + body.len(), editing);
        i += 1;
    }
    Live { st, decos, blocks }
}

/// Bold, italics, strikethrough, `code`, links and #tags inside one line.
fn inline(text: &str, st: &mut [St], r: Range<usize>, editing: bool) {
    if r.is_empty() {
        return;
    }
    let b = text.as_bytes();
    let mut used = vec![false; r.end - r.start];
    let idx = |p: usize| p - r.start;
    let mark = |st: &mut [St], used: &mut [bool], range: Range<usize>| {
        for p in range {
            used[p - r.start] = true;
            if editing {
                st[p].marker = true;
            } else {
                st[p].hide = Hide::Tiny;
            }
        }
    };
    // `code` first: nothing inside it is Markdown.
    let mut p = r.start;
    while p < r.end {
        if b[p] == b'`'
            && let Some(q) = (p + 1..r.end).find(|&q| b[q] == b'`')
            && q > p + 1
        {
            mark(st, &mut used, p..p + 1);
            mark(st, &mut used, q..q + 1);
            for k in p + 1..q {
                st[k].code = true;
                used[idx(k)] = true;
            }
            p = q + 1;
            continue;
        }
        p += 1;
    }
    // [[wiki links]] and [label](url) (and ![images]).
    let mut p = r.start;
    while p < r.end {
        if used[idx(p)] {
            p += 1;
            continue;
        }
        if text[p..r.end].starts_with("[[")
            && let Some(off) = text[p + 2..r.end].find("]]")
            && off > 0
        {
            let q = p + 2 + off;
            mark(st, &mut used, p..p + 2);
            mark(st, &mut used, q..q + 2);
            for s in &mut st[p + 2..q] {
                s.link = true;
            }
            p = q + 2;
            continue;
        }
        let img = b[p] == b'!' && p + 1 < r.end && b[p + 1] == b'[';
        let open = if img { p + 1 } else { p };
        if b[open] == b'['
            && let Some(off) = text[open + 1..r.end].find("](")
            && !text[open + 1..open + 1 + off].contains(']')
            && let Some(coff) = text[open + 1 + off + 2..r.end].find(')')
        {
            let label_end = open + 1 + off;
            let close = label_end + 2 + coff;
            mark(st, &mut used, p..open + 1);
            mark(st, &mut used, label_end..close + 1);
            for s in &mut st[open + 1..label_end] {
                s.link = true;
            }
            p = close + 1;
            continue;
        }
        // Bare web addresses.
        if (text[p..r.end].starts_with("https://") || text[p..r.end].starts_with("http://")) && (p == r.start || !b[p - 1].is_ascii_alphanumeric()) {
            let end = text[p..r.end].find(char::is_whitespace).map(|o| p + o).unwrap_or(r.end);
            for k in p..end {
                st[k].link = true;
                used[idx(k)] = true;
            }
            p = end;
            continue;
        }
        p += 1;
    }
    // Pairs: **bold**, __bold__, ~~strike~~, then *italic* and _italic_.
    let pairs = |st: &mut [St], used: &mut Vec<bool>, m: &str, apply: &dyn Fn(&mut St)| {
        let n = m.len();
        let mut p = r.start;
        while p + n <= r.end {
            let free = |a: usize, used: &Vec<bool>| (a..a + n).all(|k| !used[idx(k)]);
            if &text[p..p + n] == m && free(p, used) && p + n < r.end && !b[p + n].is_ascii_whitespace() {
                // `_` only at word edges (snake_case stays as it is).
                if m.starts_with('_') && p > r.start && b[p - 1].is_ascii_alphanumeric() {
                    p += 1;
                    continue;
                }
                let mut q = p + n + 1;
                let mut found = None;
                while q + n <= r.end {
                    if &text[q..q + n] == m && free(q, used) && !b[q - 1].is_ascii_whitespace() && (n > 1 || q + 1 >= r.end || b[q + 1] != m.as_bytes()[0]) {
                        if m.starts_with('_') && q + n < r.end && b[q + n].is_ascii_alphanumeric() {
                            q += 1;
                            continue;
                        }
                        found = Some(q);
                        break;
                    }
                    q += 1;
                }
                if let Some(q) = found {
                    for x in &mut st[p + n..q] {
                        apply(x);
                    }
                    for k in (p..p + n).chain(q..q + n) {
                        used[idx(k)] = true;
                        if editing {
                            st[k].marker = true;
                        } else {
                            st[k].hide = Hide::Tiny;
                        }
                    }
                    p = q + n;
                    continue;
                }
            }
            p += 1;
        }
    };
    pairs(st, &mut used, "**", &|s| s.bold = true);
    pairs(st, &mut used, "__", &|s| s.bold = true);
    pairs(st, &mut used, "~~", &|s| s.strike = true);
    pairs(st, &mut used, "*", &|s| s.italic = true);
    pairs(st, &mut used, "_", &|s| s.italic = true);
    // #tags (shown as they are, colored).
    let mut p = r.start;
    while p < r.end {
        if b[p] == b'#' && !used[idx(p)] && (p == r.start || b[p - 1].is_ascii_whitespace() || b[p - 1] == b'(') {
            let end = text[p + 1..r.end].find(|c: char| !(c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))).map(|o| p + 1 + o).unwrap_or(r.end);
            if end > p + 1 && !text[p + 1..end].chars().all(|c| c.is_ascii_digit()) {
                for s in &mut st[p..end] {
                    s.tag = true;
                }
                p = end;
                continue;
            }
        }
        p += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(text: &str, raw: Option<(usize, usize)>) -> String {
        let l = analyze(text, raw, None);
        text.char_indices().filter(|(i, _)| l.st[*i].hide == Hide::No).map(|(_, c)| c).collect()
    }

    #[test]
    fn hides_marks_off_the_edited_line() {
        let t = "# Title\nSome **bold** and *it* and `code`.\n";
        assert_eq!(shown(t, None), "Title\nSome bold and it and code.\n");
        // Editing the second line: its marks stay (the heading's still hidden).
        assert_eq!(shown(t, Some((10, 10))), "Title\nSome **bold** and *it* and `code`.\n");
        // …and nothing on it is styled while it's being edited.
        let l = analyze(t, Some((10, 10)), None);
        assert!(!l.st[t.find("bold").unwrap()].bold);
        // A line made a heading while editing it keeps the size it had until the cursor leaves.
        let t2 = "# Title\n## new\n";
        assert_eq!(analyze(t2, Some((10, 10)), Some((1, 0))).st[10].heading, 0);
        assert_eq!(analyze(t2, None, None).st[12].heading, 2);
        let l = analyze(t, None, None);
        assert_eq!(l.st[2].heading, 1);
        assert!(l.st[t.find("bold").unwrap()].bold);
        assert!(l.st[t.find("it*").unwrap()].italic);
        assert!(l.st[t.find("code`").unwrap()].code);
    }

    #[test]
    fn links_lists_tasks_and_rules() {
        let t = "See [docs](https://x.org) and [[Note]].\n- item\n- [x] done\n---\n> quote\n";
        assert_eq!(shown(t, None), "See docs and Note.\n item\n done\n quote\n");
        let l = analyze(t, None, None);
        assert!(l.decos.contains(&Deco::Bullet(40)));
        assert!(l.decos.iter().any(|d| matches!(d, Deco::Check { done: true, .. })));
        assert!(l.decos.iter().any(|d| matches!(d, Deco::Rule(_))));
        assert!(l.decos.iter().any(|d| matches!(d, Deco::Quote(_))));
    }

    #[test]
    fn code_blocks_fold_their_fences() {
        let t = "a\n```rust\nlet x = 1;\n```\nb\n";
        let l = analyze(t, None, None);
        assert!(l.st[2].collapse && l.st[t.find("let").unwrap()].block);
        assert_eq!(l.blocks, vec![Block { inner: 10..21, lang: "rust".into() }]);
        // Inside the block, the fences show.
        let l = analyze(t, Some((12, 12)), None);
        assert!(!l.st[2].collapse && l.st[2].marker);
        // snake_case isn't italic.
        assert!(!analyze("a_b_c\n", None, None).st[2].italic);
    }
}
