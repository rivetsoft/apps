//! Find and replace in a file (Ctrl+F, Ctrl+H) and search across the workspace
//! (Shift+Ctrl+F): plain text, whole words, matching case, or regular expressions.

use std::ops::Range;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
    pub text: String,
    pub case: bool,
    pub word: bool,
    pub regex: bool,
}

/// At most this many matches are found in one file.
pub const MAX_MATCHES: usize = 20_000;

impl Query {
    pub fn compile(&self) -> Result<Option<regex::Regex>, String> {
        if self.text.is_empty() {
            return Ok(None);
        }
        let mut p = if self.regex { self.text.clone() } else { regex::escape(&self.text) };
        if self.word {
            p = format!(r"\b(?:{p})\b");
        }
        if !self.case {
            p = format!("(?i){p}");
        }
        regex::RegexBuilder::new(&p).multi_line(true).build().map(Some).map_err(|e| e.to_string().lines().last().unwrap_or("not a valid expression").trim().to_string())
    }
}

/// The matches in `text`, as byte ranges (empty matches skipped).
pub fn find(text: &str, q: &Query) -> Result<Vec<Range<usize>>, String> {
    let Some(re) = q.compile()? else { return Ok(Vec::new()) };
    Ok(re.find_iter(text).filter(|m| !m.is_empty()).take(MAX_MATCHES).map(|m| m.range()).collect())
}

/// The text replacing one match (with `$1` and the like for regular expressions).
pub fn replacement(text: &str, range: Range<usize>, q: &Query, with: &str) -> String {
    match (q.regex, q.compile()) {
        (true, Ok(Some(re))) => match re.captures(&text[range.clone()]) {
            Some(c) => {
                let mut out = String::new();
                c.expand(with, &mut out);
                out
            }
            None => with.to_string(),
        },
        _ => with.to_string(),
    }
}

/// Replace every match; the new text and how many were replaced.
pub fn replace_all(text: &str, q: &Query, with: &str) -> Result<(String, usize), String> {
    let Some(re) = q.compile()? else { return Ok((text.to_string(), 0)) };
    let n = re.find_iter(text).filter(|m| !m.is_empty()).count();
    let out = if q.regex { re.replace_all(text, with).into_owned() } else { re.replace_all(text, regex::NoExpand(with)).into_owned() };
    Ok((out, n))
}

/// Matches as character ranges (for the text box), walking the text once.
pub fn char_ranges(text: &str, ranges: &[Range<usize>]) -> Vec<(usize, usize)> {
    let mut out = Vec::with_capacity(ranges.len());
    let (mut byte, mut ch) = (0usize, 0usize);
    let adv = |to: usize, byte: &mut usize, ch: &mut usize| {
        *ch += text[*byte..to].chars().count();
        *byte = to;
    };
    for r in ranges {
        adv(r.start, &mut byte, &mut ch);
        let a = ch;
        adv(r.end, &mut byte, &mut ch);
        out.push((a, ch));
    }
    out
}

/// One match: (line number from 1, the line, the match within the line as characters, the
/// match in the file as characters).
pub type Hit = (usize, String, (usize, usize), (usize, usize));

/// One file's matches in a workspace search.
#[derive(Clone, Debug)]
pub struct FileHits {
    pub path: PathBuf,
    pub rel: String,
    /// (line number from 1, the line, the match within the line as characters, the match in
    /// the file as characters).
    pub hits: Vec<Hit>,
}

/// Search the workspace's files (text ones under 2 MB), at most `limit` matches in all.
pub fn search_files(root: &Path, files: &[String], q: &Query, limit: usize) -> Result<(Vec<FileHits>, bool), String> {
    let Some(re) = q.compile()? else { return Ok((Vec::new(), false)) };
    let mut out = Vec::new();
    let mut total = 0;
    for rel in files {
        let path = root.join(rel);
        if std::fs::metadata(&path).map(|m| m.len() > 2 * 1024 * 1024).unwrap_or(true) {
            continue;
        }
        let Ok(text) = crate::workspace::read_text(&path) else { continue };
        let mut hits = Vec::new();
        let ranges: Vec<Range<usize>> = re.find_iter(&text).filter(|m| !m.is_empty()).map(|m| m.range()).collect();
        if ranges.is_empty() {
            continue;
        }
        let chars = char_ranges(&text, &ranges);
        for (r, (ca, cb)) in ranges.iter().zip(chars) {
            let ls = text[..r.start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let le = text[r.start..].find('\n').map(|i| r.start + i).unwrap_or(text.len());
            let line_no = text[..r.start].matches('\n').count() + 1;
            let line = &text[ls..le];
            let a = text[ls..r.start].chars().count();
            let b = a + text[r.start..r.end.min(le)].chars().count();
            hits.push((line_no, line.to_string(), (a, b), (ca, cb)));
            total += 1;
            if total >= limit {
                out.push(FileHits { path, rel: rel.clone(), hits });
                return Ok((out, true));
            }
        }
        out.push(FileHits { path, rel: rel.clone(), hits });
    }
    Ok((out, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(t: &str, case: bool, word: bool, regex: bool) -> Query {
        Query { text: t.into(), case, word, regex }
    }

    #[test]
    fn finds_and_replaces() {
        let t = "Foo foo food (foo)";
        assert_eq!(find(t, &q("foo", false, false, false)).unwrap().len(), 4);
        assert_eq!(find(t, &q("foo", true, false, false)).unwrap().len(), 3);
        assert_eq!(find(t, &q("foo", false, true, false)).unwrap().len(), 3);
        assert_eq!(find(t, &q("(foo)", false, false, false)).unwrap(), vec![13..18]);
        assert!(find(t, &q("(", false, false, true)).is_err());
        assert_eq!(replace_all(t, &q("fo(o)", true, false, true), "x$1").unwrap(), ("Foo xo xod (xo)".to_string(), 3));
        assert_eq!(replace_all("a.b", &q(".", false, false, false), "$0").unwrap().0, "a$0b");
        assert_eq!(replacement("ab12", 2..4, &q(r"(\d)(\d)", false, false, true), "$2$1"), "21");
        assert_eq!(char_ranges("ää x ä", &[2..4, 7..9]), vec![(1, 2), (5, 6)]);
    }
}
