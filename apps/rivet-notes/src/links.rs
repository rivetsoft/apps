//! Links between notes: `[[Note]]`, `[[Note|shown text]]`, `[[Note#Heading]]`, and Markdown
//! links to `.md` files. Each note's outgoing links are read when the vault is scanned; a
//! note's backlinks are the other notes whose links lead to it.

use std::ops::Range;

/// One link in a note's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Link {
    /// The note it names: a title, a file name, or a path (without `|alias` or `#heading`).
    pub target: String,
    /// The line it's on, trimmed (shown under backlinks).
    pub context: String,
    /// Where the whole link is in the text.
    pub range: Range<usize>,
}

/// Every link in `body`, skipping code (fenced blocks and `inline code`).
pub fn outgoing(body: &str) -> Vec<Link> {
    let mut out = Vec::new();
    let mut fence = false;
    let mut pos = 0;
    for line in body.split_inclusive('\n') {
        let start = pos;
        pos += line.len();
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence {
            continue;
        }
        let code = code_spans(line);
        let context: String = line.trim().chars().take(140).collect();
        let mut i = 0;
        let b = line.as_bytes();
        while i < line.len() {
            if code.iter().any(|r| r.contains(&i)) {
                i += 1;
                continue;
            }
            // `i` walks bytes and can stand inside a letter of more than one byte, so the mark
            // is looked for in the bytes; the text is only cut just after one.
            if b[i..].starts_with(b"[[")
                && let Some(off) = line[i + 2..].find("]]")
            {
                let inner = &line[i + 2..i + 2 + off];
                let target = inner.split(['|', '#']).next().unwrap_or("").trim();
                if !target.is_empty() && !inner.contains('\n') {
                    out.push(Link { target: target.to_string(), context: context.clone(), range: start + i..start + i + 4 + off });
                }
                i += 4 + off;
                continue;
            }
            // [label](path.md) — not images, not web addresses.
            if b[i] == b'['
                && (i == 0 || b[i - 1] != b'!')
                && let Some(off) = line[i + 1..].find("](")
                && let Some(coff) = line[i + 1 + off + 2..].find(')')
            {
                let url = line[i + 1 + off + 2..i + 1 + off + 2 + coff].trim();
                let path = url.split('#').next().unwrap_or("");
                if !url.contains("://") && !url.starts_with("mailto:") && (path.ends_with(".md") || path.ends_with(".markdown")) {
                    let target = path.replace("%20", " ");
                    let target = target.trim_start_matches("./").trim_end_matches(".markdown").trim_end_matches(".md").to_string();
                    out.push(Link { target, context: context.clone(), range: start + i..start + i + 1 + off + 3 + coff });
                }
                i += 1 + off + 3 + coff;
                continue;
            }
            i += 1;
        }
    }
    out
}

/// The `inline code` spans of one line.
fn code_spans(line: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    let b = line.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'`'
            && let Some(q) = (i + 1..b.len()).find(|&q| b[q] == b'`')
        {
            out.push(i..q + 1);
            i = q + 1;
            continue;
        }
        i += 1;
    }
    out
}

/// The link whose text covers this byte, if any.
pub fn link_at(body: &str, byte: usize) -> Option<Link> {
    outgoing(body).into_iter().find(|l| l.range.start <= byte && byte < l.range.end)
}

/// A `[[` being typed at the cursor: where its name starts, and the name so far.
pub fn typing_link(body: &str, cursor: usize) -> Option<(usize, String)> {
    let cursor = cursor.min(body.len());
    if !body.is_char_boundary(cursor) {
        return None;
    }
    let line_start = body[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before = &body[line_start..cursor];
    let open = before.rfind("[[")?;
    let typed = &before[open + 2..];
    if typed.contains("]]") || typed.contains('|') || typed.contains('#') || typed.len() > 80 {
        return None;
    }
    Some((line_start + open + 2, typed.to_string()))
}

/// `body` with links to any of `old` (titles or file names, any case) pointed at `new`,
/// keeping their `|shown text` and `#heading`. None when nothing changed.
pub fn rename_links(body: &str, old: &[String], new: &str) -> Option<String> {
    let olds: Vec<String> = old.iter().map(|o| o.trim().to_lowercase()).filter(|o| !o.is_empty()).collect();
    let mut out = String::with_capacity(body.len());
    let mut last = 0;
    let mut changed = false;
    for l in outgoing(body) {
        let text = &body[l.range.clone()];
        if !text.starts_with("[[") || !olds.contains(&l.target.to_lowercase()) {
            continue;
        }
        // Keep everything after the name: `|alias`, `#heading`, `]]`.
        let inner = &text[2..text.len() - 2];
        let name_end = inner.find(['|', '#']).unwrap_or(inner.len());
        out.push_str(&body[last..l.range.start]);
        out.push_str("[[");
        out.push_str(new);
        out.push_str(&inner[name_end..]);
        out.push_str("]]");
        last = l.range.end;
        changed = true;
    }
    if !changed {
        return None;
    }
    out.push_str(&body[last..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_links_but_not_in_code() {
        let t = "See [[Rivet FTP]] and [[Books|my books]] and [[Inbox#Ideas]].\nAlso [the plan](Projects/Plan.md) and [web](https://x.org/a.md).\n`[[not this]]`\n```\n[[nor this]]\n```\n";
        let names: Vec<String> = outgoing(t).into_iter().map(|l| l.target).collect();
        assert_eq!(names, ["Rivet FTP", "Books", "Inbox", "Projects/Plan"]);
        let l = &outgoing(t)[0];
        assert_eq!(&t[l.range.clone()], "[[Rivet FTP]]");
        assert!(l.context.starts_with("See [[Rivet FTP]]"));
        assert_eq!(link_at(t, 8).map(|l| l.target), Some("Rivet FTP".into()));
        assert_eq!(link_at(t, 1), None);
    }

    #[test]
    fn letters_of_more_than_one_byte() {
        let t = "Wait… see [[Nöte|é]] and [ñ…](Pläne/Übung.md) — `[[…]]` 🙂\n";
        let l = outgoing(t);
        assert_eq!(l.iter().map(|l| l.target.as_str()).collect::<Vec<_>>(), ["Nöte", "Pläne/Übung"]);
        assert_eq!(&t[l[0].range.clone()], "[[Nöte|é]]");
        // Asked about a byte inside a letter, as a caret position can be.
        assert!((0..t.len()).all(|b| link_at(t, b).is_none_or(|l| l.range.contains(&b))));
    }

    #[test]
    fn suggests_while_typing_and_renames() {
        assert_eq!(typing_link("Go to [[Riv", 11), Some((8, "Riv".into())));
        assert_eq!(typing_link("Done [[Riv]] x", 14), None);
        let t = "[[Old]] and [[old|alias]] and [[Old#Part]] but [[Older]].";
        assert_eq!(rename_links(t, &["Old".into()], "New").as_deref(), Some("[[New]] and [[New|alias]] and [[New#Part]] but [[Older]]."));
        assert_eq!(rename_links("nothing", &["Old".into()], "New"), None);
    }
}
