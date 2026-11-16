//! Markdown, read for showing: the common parts of CommonMark and GitHub's additions (headings,
//! paragraphs, emphasis, code, quotes, lists with tasks, tables, rules, links, images), plus
//! `[[wiki links]]` and `#tags`. Anything it doesn't know is shown as plain text.

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(u8, String),
    /// Lines of one paragraph; `true` after a line that ends in a hard break.
    Para(Vec<(String, bool)>),
    Code { lang: String, text: String },
    Quote(Vec<Block>),
    List(Vec<Item>),
    Table { head: Vec<String>, rows: Vec<Vec<String>> },
    Rule,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub depth: usize,
    /// The number for ordered lists.
    pub number: Option<u32>,
    /// `[ ]` / `[x]` tasks.
    pub task: Option<bool>,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strike: bool,
    pub code: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String, Style),
    /// Text, where it goes (a URL, a path, or a note's name for `[[wiki links]]`), style.
    Link(String, String, Style),
    Tag(String),
    Image { alt: String, src: String },
}

fn fence(line: &str) -> Option<&str> {
    let t = line.trim_start();
    t.strip_prefix("```").or_else(|| t.strip_prefix("~~~"))
}

fn heading(line: &str) -> Option<(u8, String)> {
    let t = line.trim_start();
    let n = t.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&n) && (t.len() == n || t[n..].starts_with(' ')) {
        Some((n as u8, t[n..].trim().trim_end_matches('#').trim().to_string()))
    } else {
        None
    }
}

fn is_rule(line: &str) -> bool {
    let t: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    t.len() >= 3 && (t.chars().all(|c| c == '-') || t.chars().all(|c| c == '*') || t.chars().all(|c| c == '_'))
}

/// A list item's marker: (indent, number for ordered lists, the text after it).
fn list_item(line: &str) -> Option<(usize, Option<u32>, &str)> {
    let indent = line.len() - line.trim_start().len();
    let t = line.trim_start();
    for m in ["- ", "* ", "+ "] {
        if let Some(rest) = t.strip_prefix(m) {
            return Some((indent, None, rest));
        }
    }
    let digits: String = t.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() && digits.len() < 10 {
        let rest = &t[digits.len()..];
        if let Some(r) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((indent, digits.parse().ok(), r));
        }
    }
    None
}

fn table_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    t.split('|').map(|c| c.trim().to_string()).collect()
}

fn is_table_rule(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty() && line.contains('-') && cells.iter().all(|c| !c.is_empty() && c.chars().all(|ch| matches!(ch, '-' | ':')))
}

/// The blocks of a Markdown text.
pub fn parse(text: &str) -> Vec<Block> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut para: Vec<(String, bool)> = Vec::new();
    let flush = |para: &mut Vec<(String, bool)>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(std::mem::take(para)));
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        if let Some(lang) = fence(line) {
            flush(&mut para, &mut out);
            let marker = &line.trim_start()[..3];
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].trim_start().starts_with(marker) {
                code.push(lines[i]);
                i += 1;
            }
            out.push(Block::Code { lang: lang.trim().to_string(), text: code.join("\n") });
            i += 1;
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        if let Some((n, t)) = heading(line) {
            flush(&mut para, &mut out);
            out.push(Block::Heading(n, t));
            i += 1;
            continue;
        }
        if is_rule(line) {
            flush(&mut para, &mut out);
            out.push(Block::Rule);
            i += 1;
            continue;
        }
        if line.trim_start().starts_with('>') {
            flush(&mut para, &mut out);
            let mut inner = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let t = lines[i].trim_start().trim_start_matches('>');
                inner.push(t.strip_prefix(' ').unwrap_or(t));
                i += 1;
            }
            out.push(Block::Quote(parse(&inner.join("\n"))));
            continue;
        }
        if list_item(line).is_some() && para.is_empty() {
            let base = line.len() - line.trim_start().len();
            let mut items: Vec<Item> = Vec::new();
            while i < lines.len() {
                let l = lines[i];
                if let Some((indent, number, rest)) = list_item(l) {
                    let (task, text) = match rest.get(..4) {
                        Some("[ ] ") => (Some(false), &rest[4..]),
                        Some("[x] " | "[X] ") => (Some(true), &rest[4..]),
                        _ => (None, rest),
                    };
                    items.push(Item { depth: indent.saturating_sub(base) / 2, number, task, text: text.trim().to_string() });
                } else if !l.trim().is_empty() && l.starts_with([' ', '\t']) && let Some(last) = items.last_mut() {
                    // A continuation line of the item above.
                    last.text.push(' ');
                    last.text.push_str(l.trim());
                } else {
                    break;
                }
                i += 1;
            }
            out.push(Block::List(items));
            continue;
        }
        if line.contains('|') && i + 1 < lines.len() && is_table_rule(lines[i + 1]) && para.is_empty() {
            let head = table_cells(line);
            let mut rows = Vec::new();
            i += 2;
            while i < lines.len() && lines[i].contains('|') && !lines[i].trim().is_empty() {
                rows.push(table_cells(lines[i]));
                i += 1;
            }
            out.push(Block::Table { head, rows });
            continue;
        }
        let hard = line.ends_with("  ") || line.ends_with('\\');
        para.push((line.trim().trim_end_matches('\\').to_string(), hard));
        i += 1;
    }
    flush(&mut para, &mut out);
    out
}

/// The inline parts of a line of text.
pub fn inlines(s: &str) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    let mut st = Style::default();
    let mut buf = String::new();
    let chars: Vec<char> = s.chars().collect();
    let flush = |buf: &mut String, out: &mut Vec<Inline>, st: &Style| {
        if !buf.is_empty() {
            out.push(Inline::Text(std::mem::take(buf), st.clone()));
        }
    };
    let rest = |i: usize| -> String { chars[i..].iter().collect() };
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let prev = if i > 0 { chars[i - 1] } else { ' ' };
        // Escapes.
        if c == '\\' && i + 1 < chars.len() && chars[i + 1].is_ascii_punctuation() {
            buf.push(chars[i + 1]);
            i += 2;
            continue;
        }
        // `code`
        if c == '`'
            && let Some(j) = chars[i + 1..].iter().position(|x| *x == '`')
        {
            flush(&mut buf, &mut out, &st);
            let code: String = chars[i + 1..i + 1 + j].iter().collect();
            out.push(Inline::Text(code, Style { code: true, ..Default::default() }));
            i += j + 2;
            continue;
        }
        // ![alt](src) and [text](url)
        let image = c == '!' && chars.get(i + 1) == Some(&'[');
        if c == '[' || image {
            let r = rest(if image { i + 1 } else { i });
            if let Some(close) = r.find("](")
                && let Some(end) = r[close + 2..].find(')')
                && !r[1..close].contains(['\n', ']'])
                && !r.starts_with("[[")
            {
                let label = &r[1..close];
                let url = r[close + 2..close + 2 + end].trim();
                let url = url.split_once(' ').map(|(u, _)| u).unwrap_or(url).trim_matches(['<', '>']);
                flush(&mut buf, &mut out, &st);
                if image {
                    out.push(Inline::Image { alt: label.to_string(), src: url.to_string() });
                } else {
                    out.push(Inline::Link(label.to_string(), url.to_string(), st.clone()));
                }
                i += (if image { 1 } else { 0 }) + r[..close + 2 + end + 1].chars().count();
                continue;
            }
            // [[wiki link]] or [[target|label]]
            if !image && r.starts_with("[[")
                && let Some(end) = r.find("]]")
            {
                let inner = &r[2..end];
                let (target, label) = inner.split_once('|').unwrap_or((inner, inner));
                flush(&mut buf, &mut out, &st);
                out.push(Inline::Link(label.trim().to_string(), format!("[[{}]]", target.trim()), st.clone()));
                i += r[..end + 2].chars().count();
                continue;
            }
        }
        // <https://…>
        if c == '<' {
            let r = rest(i);
            if (r.starts_with("<http://") || r.starts_with("<https://") || r.starts_with("<mailto:"))
                && let Some(end) = r.find('>')
            {
                flush(&mut buf, &mut out, &st);
                let u = r[1..end].to_string();
                out.push(Inline::Link(u.clone(), u, st.clone()));
                i += r[..end + 1].chars().count();
                continue;
            }
        }
        // Bare links.
        if c == 'h' && (prev.is_whitespace() || prev == '(') {
            let r = rest(i);
            if r.starts_with("http://") || r.starts_with("https://") {
                let u: String = r.chars().take_while(|c| !c.is_whitespace() && *c != ')').collect();
                let u = u.trim_end_matches(['.', ',', ';', ':', '!', '?']).to_string();
                flush(&mut buf, &mut out, &st);
                i += u.chars().count();
                out.push(Inline::Link(u.clone(), u, st.clone()));
                continue;
            }
        }
        // #tags
        if c == '#' && (prev.is_whitespace() || prev == '(' || i == 0) {
            let tag: String = chars[i + 1..].iter().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/')).collect();
            if !tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit()) {
                flush(&mut buf, &mut out, &st);
                i += 1 + tag.chars().count();
                out.push(Inline::Tag(tag.trim_end_matches('/').to_string()));
                continue;
            }
        }
        // Emphasis: ** __ ~~ toggle bold / strike; * _ italic (_ only at word edges).
        let next = chars.get(i + 1).copied().unwrap_or(' ');
        if (c == '*' || c == '_') && next == c {
            let after = chars.get(i + 2).copied().unwrap_or(' ');
            let opens = !st.bold && !after.is_whitespace();
            let closes = st.bold && !prev.is_whitespace();
            if opens || closes {
                flush(&mut buf, &mut out, &st);
                st.bold = !st.bold;
                i += 2;
                continue;
            }
        }
        if c == '~' && next == '~' {
            flush(&mut buf, &mut out, &st);
            st.strike = !st.strike;
            i += 2;
            continue;
        }
        if c == '*' || (c == '_' && (if st.italic { !next.is_alphanumeric() } else { !prev.is_alphanumeric() })) {
            let opens = !st.italic && !next.is_whitespace();
            let closes = st.italic && !prev.is_whitespace();
            if opens || closes {
                flush(&mut buf, &mut out, &st);
                st.italic = !st.italic;
                i += 1;
                continue;
            }
        }
        buf.push(c);
        i += 1;
    }
    flush(&mut buf, &mut out, &st);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks() {
        let md = "# Title\n\nSome *text*  \nnext line\n\n- a\n  more of a\n- [x] done\n  1. inner\n\n> quoted\n> # head\n\n```rust\nfn x() {}\n```\n\n| A | B |\n|---|:-:|\n| 1 | 2 |\n\n---\n";
        let b = parse(md);
        assert_eq!(b[0], Block::Heading(1, "Title".into()));
        assert_eq!(b[1], Block::Para(vec![("Some *text*".into(), true), ("next line".into(), false)]));
        let Block::List(items) = &b[2] else { panic!("{:?}", b[2]) };
        assert_eq!(items.len(), 3);
        assert_eq!(items[0].text, "a more of a");
        assert_eq!((items[1].task, items[2].depth, items[2].number), (Some(true), 1, Some(1)));
        assert_eq!(b[3], Block::Quote(vec![Block::Para(vec![("quoted".into(), false)]), Block::Heading(1, "head".into())]));
        assert_eq!(b[4], Block::Code { lang: "rust".into(), text: "fn x() {}".into() });
        assert_eq!(b[5], Block::Table { head: vec!["A".into(), "B".into()], rows: vec![vec!["1".into(), "2".into()]] });
        assert_eq!(b[6], Block::Rule);
    }

    #[test]
    fn inline_parts() {
        let v = inlines("A **bold** and *it* with `code`, [link](https://x.org), [[Other Note|other]], #tag, ~~gone~~ and ![pic](a.png) snake_case");
        let bold = Style { bold: true, ..Default::default() };
        assert!(v.contains(&Inline::Text("bold".into(), bold)));
        assert!(v.contains(&Inline::Text("it".into(), Style { italic: true, ..Default::default() })));
        assert!(v.contains(&Inline::Text("code".into(), Style { code: true, ..Default::default() })));
        assert!(v.contains(&Inline::Link("link".into(), "https://x.org".into(), Style::default())));
        assert!(v.contains(&Inline::Link("other".into(), "[[Other Note]]".into(), Style::default())));
        assert!(v.contains(&Inline::Tag("tag".into())));
        assert!(v.contains(&Inline::Text("gone".into(), Style { strike: true, ..Default::default() })));
        assert!(v.contains(&Inline::Image { alt: "pic".into(), src: "a.png".into() }));
        // Underscores inside words aren't emphasis.
        assert!(matches!(v.last(), Some(Inline::Text(t, s)) if t.ends_with("snake_case") && !s.italic));
        assert_eq!(inlines("see https://a.b/c."), vec![Inline::Text("see ".into(), Style::default()), Inline::Link("https://a.b/c".into(), "https://a.b/c".into(), Style::default()), Inline::Text(".".into(), Style::default())]);
    }
}
