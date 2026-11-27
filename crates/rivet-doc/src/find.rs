//! Finding text on a page: where a phrase is, as ranges of characters. Case is ignored unless
//! asked for, and any run of spaces or line ends in the page matches any in the phrase, so a
//! phrase broken over two lines is still found.

use crate::PageText;

/// Where `query` is on the page, as [start, end) character ranges, in order.
pub fn matches(text: &PageText, query: &str, match_case: bool) -> Vec<(usize, usize)> {
    let fold = |c: char| if match_case { c } else { c.to_lowercase().next().unwrap_or(c) };
    let q: Vec<char> = squash(query.trim().chars().map(fold)).collect();
    if q.is_empty() {
        return Vec::new();
    }
    // The page squashed the same way, remembering where each character came from.
    let mut page: Vec<(char, usize)> = Vec::with_capacity(text.chars.len());
    for (i, c) in text.chars.iter().enumerate() {
        let c = if c.is_whitespace() { ' ' } else { fold(*c) };
        if c == ' ' && page.last().is_some_and(|(p, _)| *p == ' ') {
            continue;
        }
        page.push((c, i));
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i + q.len() <= page.len() {
        if page[i..i + q.len()].iter().zip(&q).all(|((c, _), w)| c == w) {
            let start = page[i].1;
            let end = page[i + q.len() - 1].1 + 1;
            out.push((start, end));
            i += q.len();
        } else {
            i += 1;
        }
    }
    out
}

fn squash(chars: impl Iterator<Item = char>) -> impl Iterator<Item = char> {
    let mut last_space = true;
    chars.filter_map(move |c| {
        let c = if c.is_whitespace() { ' ' } else { c };
        if c == ' ' && last_space {
            return None;
        }
        last_space = c == ' ';
        Some(c)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(s: &str) -> PageText {
        PageText { chars: s.chars().collect(), boxes: vec![[0.0; 4]; s.chars().count()] }
    }

    #[test]
    fn finds_across_case_and_lines() {
        let t = page("The Panel shows the panel\r\nitems. PANEL");
        assert_eq!(matches(&t, "panel", false), vec![(4, 9), (20, 25), (34, 39)]);
        assert_eq!(matches(&t, "Panel", true), vec![(4, 9)]);
        assert_eq!(matches(&t, "panel items", false), vec![(20, 32)]);
        assert_eq!(matches(&t, "  ", false), vec![]);
        assert_eq!(matches(&t, "panel ", false).len(), 3);
    }
}
