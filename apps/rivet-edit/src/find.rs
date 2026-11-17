//! Find and replace in a file (Ctrl+F, Ctrl+H) and search across the workspace
//! (Shift+Ctrl+F): plain text, whole words, matching case, or regular expressions.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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

/// One match in a workspace search, ready to show: the line around it, cut short.
#[derive(Clone, Debug)]
pub struct Hit {
    /// Line number, from 1.
    pub line: usize,
    /// The text before the match (at most a few words, "…" when cut).
    pub pre: String,
    pub hit: String,
    /// The rest of the line (cut short).
    pub post: String,
    /// The match in the file, as characters.
    pub at: (usize, usize),
}

/// One file's matches in a workspace search.
#[derive(Clone, Debug)]
pub struct FileHits {
    pub path: PathBuf,
    pub rel: String,
    pub hits: Vec<Hit>,
}

/// Files never searched (pictures, archives, fonts and other binaries), by extension.
const SKIP: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "icns", "tif", "tiff", "psd", "avif", "heic", "pdf", "zip", "gz", "tgz", "xz", "bz2", "zst", "7z", "rar", "jar", "war", "so", "dll", "dylib", "exe", "o", "a", "lib", "rlib", "rmeta", "class", "pyc", "wasm", "bin", "dat", "db",
    "sqlite", "mdb", "woff", "woff2", "ttf", "otf", "eot", "mp3", "mp4", "m4a", "ogg", "wav", "flac", "mov", "avi", "mkv", "webm", "glb", "gltf", "fbx", "blend", "iso", "dmg", "deb", "rpm", "flatpak", "lock",
];

fn skipped(rel: &str) -> bool {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    match name.rsplit_once('.') {
        Some((_, ext)) => SKIP.iter().any(|s| ext.eq_ignore_ascii_case(s)) && !name.eq_ignore_ascii_case("Cargo.lock"),
        None => false,
    }
}

/// The matches in one file's text, each with its line cut short around it.
fn file_hits(text: &str, re: &regex::Regex, room: usize) -> Vec<Hit> {
    let ranges: Vec<Range<usize>> = re.find_iter(text).filter(|m| !m.is_empty()).take(room).map(|m| m.range()).collect();
    if ranges.is_empty() {
        return Vec::new();
    }
    let chars = char_ranges(text, &ranges);
    let bytes = text.as_bytes();
    let (mut line, mut line_start, mut seen) = (1usize, 0usize, 0usize);
    let mut out = Vec::with_capacity(ranges.len());
    for (r, at) in ranges.into_iter().zip(chars) {
        // Count lines once, walking forward (not from the top for every match).
        for (i, &b) in bytes[seen..r.start].iter().enumerate() {
            if b == b'\n' {
                line += 1;
                line_start = seen + i + 1;
            }
        }
        seen = r.start;
        // The end of the line, looking only a little way past the match (long lines).
        let mut window = (r.end + 400).min(text.len());
        while !text.is_char_boundary(window) {
            window -= 1;
        }
        let line_end = text[r.start..window].find('\n').map(|i| r.start + i).unwrap_or(window);
        let hit_end = r.end.min(line_end);
        let lead = text[line_start..r.start].trim_start();
        let pre = match lead.char_indices().rev().nth(17) {
            Some((i, _)) => format!("…{}", &lead[i..]),
            None => lead.to_string(),
        };
        let post: String = text[hit_end..line_end].chars().take(80).collect();
        out.push(Hit { line, pre, hit: text[r.start..hit_end].to_string(), post, at });
    }
    out
}

/// Search the workspace's files (text ones under 2 MB) on several threads, at most `limit`
/// matches in all, in the files' order. Stops early when `stop` is set (a newer search).
pub fn search_files(root: &Path, files: &[String], q: &Query, limit: usize, stop: &AtomicBool) -> Result<(Vec<FileHits>, bool), String> {
    let Some(re) = q.compile()? else { return Ok((Vec::new(), false)) };
    let next = AtomicUsize::new(0);
    let total = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).clamp(1, 8);
    let mut found: Vec<(usize, FileHits)> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut mine = Vec::new();
                    loop {
                        let i = next.fetch_add(1, Ordering::Relaxed);
                        if i >= files.len() || stop.load(Ordering::Relaxed) || total.load(Ordering::Relaxed) >= limit {
                            break;
                        }
                        let rel = &files[i];
                        if skipped(rel) {
                            continue;
                        }
                        let path = root.join(rel);
                        if std::fs::metadata(&path).map(|m| m.len() > 2 * 1024 * 1024).unwrap_or(true) {
                            continue;
                        }
                        let Ok(text) = crate::workspace::read_text(&path) else { continue };
                        let room = limit.saturating_sub(total.load(Ordering::Relaxed));
                        let hits = file_hits(&text, &re, room.max(1));
                        if !hits.is_empty() {
                            total.fetch_add(hits.len(), Ordering::Relaxed);
                            mine.push((i, FileHits { path, rel: rel.clone(), hits }));
                        }
                    }
                    mine
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap_or_default()).collect()
    });
    if stop.load(Ordering::Relaxed) {
        return Err("stopped".into());
    }
    found.sort_by_key(|(i, _)| *i);
    // Keep the first `limit` matches in the files' order.
    let mut left = limit;
    let mut out = Vec::new();
    let mut cut = false;
    for (_, mut f) in found {
        if left == 0 {
            cut = true;
            break;
        }
        if f.hits.len() >= left {
            cut = cut || f.hits.len() > left || total.load(Ordering::Relaxed) > limit;
            f.hits.truncate(left);
        }
        left -= f.hits.len();
        out.push(f);
    }
    if total.load(Ordering::Relaxed) >= limit {
        cut = true;
    }
    Ok((out, cut))
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

    #[test]
    fn hits_show_their_line() {
        let re = q("fox", false, false, false).compile().unwrap().unwrap();
        let text = "one\n\n    the quick brown fox jumps\nfox\n";
        let h = file_hits(text, &re, 100);
        assert_eq!(h.len(), 2);
        assert_eq!((h[0].line, h[0].pre.as_str(), h[0].hit.as_str(), h[0].post.as_str()), (3, "the quick brown ", "fox", " jumps"));
        assert_eq!((h[1].line, h[1].pre.as_str(), h[1].post.as_str()), (4, "", ""));
        assert_eq!(h[1].at, (35, 38));
        assert!(skipped("img/a.PNG") && !skipped("Cargo.lock") && !skipped("src/main.rs"));
    }
}
