//! Editing with one or more selections (multiple cursors), and line commands, the way VS Code
//! does them: Ctrl+D adds the next occurrence, Shift+Ctrl+L selects them all, Ctrl+Alt+Up/Down
//! adds a cursor above or below, Alt+Up/Down moves lines, Shift+Alt+Up/Down copies them,
//! Shift+Ctrl+K deletes them, Ctrl+/ comments them, Ctrl+] and Ctrl+[ indent them.
//!
//! Positions are character indexes (what the text box's cursor uses). The text is worked on as
//! characters, which is plenty fast for files people edit by hand.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Sel {
    pub anchor: usize,
    pub head: usize,
}

impl Sel {
    pub fn caret(p: usize) -> Sel {
        Sel { anchor: p, head: p }
    }
    pub fn new(anchor: usize, head: usize) -> Sel {
        Sel { anchor, head }
    }
    pub fn lo(&self) -> usize {
        self.anchor.min(self.head)
    }
    pub fn hi(&self) -> usize {
        self.anchor.max(self.head)
    }
    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }
}

/// Sorted, without overlaps (overlapping ones become one). The last one stays the main one
/// when it survives.
pub fn normalize(sels: &mut Vec<Sel>) {
    let main = sels.last().copied();
    sels.sort_by_key(|s| (s.lo(), s.hi()));
    let mut out: Vec<Sel> = Vec::with_capacity(sels.len());
    for s in sels.drain(..) {
        match out.last_mut() {
            Some(l) if s.lo() < l.hi() || (s.lo() == l.hi() && (s.is_empty() || l.is_empty()) && s.lo() == l.lo()) || *l == s => {
                let (lo, hi) = (l.lo().min(s.lo()), l.hi().max(s.hi()));
                *l = if l.head >= l.anchor { Sel::new(lo, hi) } else { Sel::new(hi, lo) };
            }
            _ => out.push(s),
        }
    }
    // Keep the main selection last.
    if let Some(m) = main
        && let Some(i) = out.iter().position(|s| s.lo() <= m.lo() && m.hi() <= s.hi())
    {
        let s = out.remove(i);
        out.push(s);
    }
    *sels = out;
}

fn sorted(sels: &[Sel]) -> Vec<(usize, Sel)> {
    let mut v: Vec<(usize, Sel)> = sels.iter().copied().enumerate().collect();
    v.sort_by_key(|(_, s)| s.lo());
    v
}

/// Replace every selection with the text `f` gives for it (it gets the characters and the
/// selection), leaving a caret after each insert.
pub fn insert_each(chars: &mut Vec<char>, sels: &mut Vec<Sel>, f: impl Fn(&[char], Sel) -> String) {
    normalize(sels);
    let order = sorted(sels);
    let mut out: Vec<char> = Vec::with_capacity(chars.len() + 16);
    let mut pos = 0;
    let mut new = vec![Sel::default(); sels.len()];
    for (i, s) in order {
        let (lo, hi) = (s.lo().min(chars.len()), s.hi().min(chars.len()));
        out.extend_from_slice(&chars[pos..lo.max(pos)]);
        out.extend(f(chars, s).chars());
        new[i] = Sel::caret(out.len());
        pos = hi.max(pos);
    }
    out.extend_from_slice(&chars[pos.min(chars.len())..]);
    *chars = out;
    *sels = new;
}

pub fn insert(chars: &mut Vec<char>, sels: &mut Vec<Sel>, s: &str) {
    insert_each(chars, sels, |_, _| s.to_string());
}

/// Backspace (or Delete, `forward`): empty selections take one character.
pub fn delete(chars: &mut Vec<char>, sels: &mut Vec<Sel>, forward: bool) {
    let n = chars.len();
    for s in sels.iter_mut() {
        if s.is_empty() {
            *s = if forward { Sel::new(s.head, (s.head + 1).min(n)) } else { Sel::new(s.head.saturating_sub(1), s.head) };
        }
    }
    insert(chars, sels, "");
}

pub fn line_start(chars: &[char], p: usize) -> usize {
    let p = p.min(chars.len());
    chars[..p].iter().rposition(|c| *c == '\n').map(|i| i + 1).unwrap_or(0)
}

pub fn line_end(chars: &[char], p: usize) -> usize {
    let p = p.min(chars.len());
    chars[p..].iter().position(|c| *c == '\n').map(|i| p + i).unwrap_or(chars.len())
}

/// The leading spaces and tabs of the line at `p`.
pub fn indent_of(chars: &[char], p: usize) -> String {
    let a = line_start(chars, p);
    chars[a..].iter().take_while(|c| **c == ' ' || **c == '\t').collect()
}

/// Enter: a new line with the same indent at every cursor.
pub fn newline(chars: &mut Vec<char>, sels: &mut Vec<Sel>) {
    insert_each(chars, sels, |c, s| format!("\n{}", indent_of(c, s.lo())));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    WordLeft,
    WordRight,
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Move every cursor (`extend`: keep the anchor, growing the selection).
pub fn move_sels(chars: &[char], sels: &mut Vec<Sel>, m: Move, extend: bool) {
    let n = chars.len();
    for s in sels.iter_mut() {
        // Left / Right without Shift on a selection: to its edge.
        if !extend && !s.is_empty() && matches!(m, Move::Left | Move::Right) {
            *s = Sel::caret(if m == Move::Left { s.lo() } else { s.hi() });
            continue;
        }
        let h = s.head;
        let to = match m {
            Move::Left => h.saturating_sub(1),
            Move::Right => (h + 1).min(n),
            Move::Home => {
                // First to the indent's end, then to the line's start.
                let a = line_start(chars, h);
                let ind = a + indent_of(chars, h).chars().count();
                if h == ind { a } else { ind }
            }
            Move::End => line_end(chars, h),
            Move::Up | Move::Down => {
                let a = line_start(chars, h);
                let col = h - a;
                if m == Move::Up {
                    if a == 0 {
                        0
                    } else {
                        let pa = line_start(chars, a - 1);
                        (pa + col).min(a - 1)
                    }
                } else {
                    let e = line_end(chars, h);
                    if e >= n {
                        n
                    } else {
                        let na = e + 1;
                        (na + col).min(line_end(chars, na))
                    }
                }
            }
            Move::WordLeft => {
                let mut p = h;
                while p > 0 && !is_word(chars[p - 1]) {
                    p -= 1;
                }
                while p > 0 && is_word(chars[p - 1]) {
                    p -= 1;
                }
                p
            }
            Move::WordRight => {
                let mut p = h;
                while p < n && !is_word(chars[p]) {
                    p += 1;
                }
                while p < n && is_word(chars[p]) {
                    p += 1;
                }
                p
            }
        };
        *s = if extend { Sel::new(s.anchor, to) } else { Sel::caret(to) };
    }
    normalize(sels);
}

/// The word at (or just before) `p`.
pub fn word_at(chars: &[char], p: usize) -> Option<(usize, usize)> {
    let p = p.min(chars.len());
    let mut a = p;
    while a > 0 && is_word(chars[a - 1]) {
        a -= 1;
    }
    let mut b = p;
    while b < chars.len() && is_word(chars[b]) {
        b += 1;
    }
    (a < b).then_some((a, b))
}

/// Is `chars[a..b]` a whole word (word characters, with none just around it)?
fn whole_word(chars: &[char], a: usize, b: usize) -> bool {
    a < b && chars[a..b].iter().all(|c| is_word(*c)) && (a == 0 || !is_word(chars[a - 1])) && (b >= chars.len() || !is_word(chars[b]))
}

/// The next place `needle` appears from `from` (wrapping round); `word`: only as a whole word.
fn find_from(chars: &[char], needle: &[char], from: usize, word: bool) -> Option<usize> {
    if needle.is_empty() || needle.len() > chars.len() {
        return None;
    }
    let last = chars.len() - needle.len();
    (from.min(last + 1)..=last).chain(0..from.min(last + 1)).find(|&i| chars[i..i + needle.len()] == *needle && (!word || whole_word(chars, i, i + needle.len())))
}

/// Ctrl+D: the word at the cursor first; then the next place the selected text appears.
pub fn add_next_occurrence(chars: &[char], sels: &mut Vec<Sel>) {
    let Some(last) = sels.last().copied() else { return };
    if last.is_empty() {
        if let Some((a, b)) = word_at(chars, last.head) {
            *sels.last_mut().unwrap() = Sel::new(a, b);
        }
        return;
    }
    let needle: Vec<char> = chars[last.lo()..last.hi()].to_vec();
    // A whole word selected: whole words only (as VS Code does after Ctrl+D picks a word).
    let word = whole_word(chars, last.lo(), last.hi());
    let mut from = last.hi();
    for _ in 0..chars.len() {
        let Some(i) = find_from(chars, &needle, from, word) else { return };
        let cand = Sel::new(i, i + needle.len());
        if !sels.iter().any(|s| s.lo() == cand.lo()) {
            sels.push(cand);
            return;
        }
        if i + 1 == from || sels.len() * needle.len() >= chars.len() {
            return;
        }
        from = i + 1;
        if from == last.hi() {
            return;
        }
    }
}

/// Shift+Ctrl+L: every place the selected text (or the word at the cursor) appears.
pub fn select_all_occurrences(chars: &[char], sels: &mut Vec<Sel>) {
    let Some(mut last) = sels.last().copied() else { return };
    if last.is_empty() {
        let Some((a, b)) = word_at(chars, last.head) else { return };
        last = Sel::new(a, b);
    }
    let needle: Vec<char> = chars[last.lo()..last.hi()].to_vec();
    let word = whole_word(chars, last.lo(), last.hi());
    let mut out = Vec::new();
    let mut i = 0;
    while i + needle.len() <= chars.len() {
        if chars[i..i + needle.len()] == *needle && (!word || whole_word(chars, i, i + needle.len())) {
            out.push(Sel::new(i, i + needle.len()));
            i += needle.len();
        } else {
            i += 1;
        }
    }
    // The one the cursor was on stays the main one.
    if let Some(k) = out.iter().position(|s| s.lo() == last.lo()) {
        let m = out.remove(k);
        out.push(m);
    }
    if !out.is_empty() {
        *sels = out;
    }
}

/// Ctrl+Alt+Up / Down: another cursor on the line above or below the main one.
pub fn add_cursor_vertical(chars: &[char], sels: &mut Vec<Sel>, down: bool) {
    let Some(last) = sels.last().copied() else { return };
    let mut s = vec![Sel::caret(last.head)];
    move_sels(chars, &mut s, if down { Move::Down } else { Move::Up }, false);
    if s[0].head != last.head && !sels.iter().any(|x| x.head == s[0].head) {
        sels.push(s[0]);
    }
}

// ---------------------------------------------------------------- whole lines

/// (line, column) of a position.
fn to_lc(chars: &[char], p: usize) -> (usize, usize) {
    let p = p.min(chars.len());
    let line = chars[..p].iter().filter(|c| **c == '\n').count();
    (line, p - line_start(chars, p))
}

fn from_lc(lines: &[Vec<char>], line: usize, col: usize) -> usize {
    let line = line.min(lines.len().saturating_sub(1));
    lines[..line].iter().map(|l| l.len() + 1).sum::<usize>() + col.min(lines.get(line).map_or(0, Vec::len))
}

fn split_lines(chars: &[char]) -> Vec<Vec<char>> {
    chars.split(|c| *c == '\n').map(<[char]>::to_vec).collect()
}

fn join_lines(lines: &[Vec<char>]) -> Vec<char> {
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.extend_from_slice(l);
    }
    out
}

/// The lines the selections cover, as merged (first, last) blocks. A selection ending at a
/// line's very start doesn't take that line.
fn blocks(chars: &[char], sels: &[Sel]) -> Vec<(usize, usize)> {
    let mut v: Vec<(usize, usize)> = sels
        .iter()
        .map(|s| {
            let (a, _) = to_lc(chars, s.lo());
            let (mut b, bc) = to_lc(chars, s.hi());
            if !s.is_empty() && bc == 0 && b > a {
                b -= 1;
            }
            (a, b)
        })
        .collect();
    v.sort();
    let mut out: Vec<(usize, usize)> = Vec::new();
    for (a, b) in v {
        match out.last_mut() {
            Some(l) if a <= l.1 + 1 => l.1 = l.1.max(b),
            _ => out.push((a, b)),
        }
    }
    out
}

type Lc = ((usize, usize), (usize, usize));

/// Run a line command: `f` changes the lines and moves each selection's (line, column) ends.
fn with_lines(chars: &mut Vec<char>, sels: &mut Vec<Sel>, f: impl FnOnce(&mut Vec<Vec<char>>, &[(usize, usize)], &mut Vec<Lc>)) {
    let bl = blocks(chars, sels);
    let mut lcs: Vec<Lc> = sels.iter().map(|s| (to_lc(chars, s.anchor), to_lc(chars, s.head))).collect();
    let mut lines = split_lines(chars);
    f(&mut lines, &bl, &mut lcs);
    *chars = join_lines(&lines);
    *sels = lcs.into_iter().map(|(a, h)| Sel::new(from_lc(&lines, a.0, a.1), from_lc(&lines, h.0, h.1))).collect();
    normalize(sels);
}

/// Alt+Up / Down: move the lines up or down by one.
pub fn move_lines(chars: &mut Vec<char>, sels: &mut Vec<Sel>, down: bool) {
    with_lines(chars, sels, |lines, bl, lcs| {
        let n = lines.len();
        // Blocks that can move (not at the top / bottom); moved from the far end inwards.
        let mut moved: Vec<(usize, usize)> = Vec::new();
        let order: Vec<&(usize, usize)> = if down { bl.iter().rev().collect() } else { bl.iter().collect() };
        for &(a, b) in order {
            if (!down && a == 0) || (down && b + 1 >= n) {
                continue;
            }
            if down {
                let l = lines.remove(b + 1);
                lines.insert(a, l);
            } else {
                let l = lines.remove(a - 1);
                lines.insert(b, l);
            }
            moved.push((a, b));
        }
        for (an, hd) in lcs.iter_mut() {
            for &(a, b) in &moved {
                if (a..=b).contains(&hd.0) {
                    let d = |x: usize| if down { x + 1 } else { x - 1 };
                    an.0 = d(an.0);
                    hd.0 = d(hd.0);
                    break;
                }
            }
        }
    });
}

/// Shift+Alt+Up / Down: copy the lines above or below; the cursors go with the copy below.
pub fn duplicate_lines(chars: &mut Vec<char>, sels: &mut Vec<Sel>, down: bool) {
    with_lines(chars, sels, |lines, bl, lcs| {
        let mut added = 0;
        let mut shift: Vec<(usize, usize, usize)> = Vec::new(); // (a, b, lines added before it)
        for &(a, b) in bl {
            let (a2, b2) = (a + added, b + added);
            let copy: Vec<Vec<char>> = lines[a2..=b2].to_vec();
            for (k, l) in copy.into_iter().enumerate() {
                lines.insert(b2 + 1 + k, l);
            }
            shift.push((a, b, added));
            added += b - a + 1;
        }
        for (an, hd) in lcs.iter_mut() {
            let line = hd.0;
            let (before, own) = shift.iter().fold((0, 0), |(before, own), &(a, b, add)| {
                if line > b { (add + (b - a + 1), own) } else if (a..=b).contains(&line) { (add, b - a + 1) } else { (before, own) }
            });
            let d = before + if down { own } else { 0 };
            an.0 += d;
            hd.0 += d;
        }
    });
}

/// Shift+Ctrl+K: delete the lines; the cursors go to where they were, one line down.
pub fn delete_lines(chars: &mut Vec<char>, sels: &mut Vec<Sel>) {
    with_lines(chars, sels, |lines, bl, lcs| {
        let mut gone = 0;
        let mut spans: Vec<(usize, usize, usize)> = Vec::new();
        for &(a, b) in bl {
            let (a2, b2) = (a - gone, b - gone);
            lines.drain(a2..=b2);
            spans.push((a, b, gone));
            gone += b - a + 1;
        }
        if lines.is_empty() {
            lines.push(Vec::new());
        }
        for (an, hd) in lcs.iter_mut() {
            let line = hd.0;
            let mut nl = line;
            for &(a, b, g) in &spans {
                if line > b {
                    nl = line - (g + b - a + 1);
                } else if (a..=b).contains(&line) {
                    nl = a - g;
                }
            }
            let nl = nl.min(lines.len() - 1);
            *an = (nl, hd.1);
            *hd = (nl, hd.1);
        }
    });
}

/// Ctrl+] and Ctrl+[: indent by `width` spaces, or take an indent away.
pub fn indent_lines(chars: &mut Vec<char>, sels: &mut Vec<Sel>, width: usize, out: bool) {
    with_lines(chars, sels, |lines, bl, lcs| {
        let mut delta: Vec<(usize, isize)> = Vec::new();
        for &(a, b) in bl {
            for (i, l) in lines.iter_mut().enumerate().take(b + 1).skip(a) {
                if out {
                    let n = l.iter().take(width).take_while(|c| **c == ' ').count();
                    let n = if n == 0 && l.first() == Some(&'\t') { 1 } else { n };
                    l.drain(..n);
                    delta.push((i, -(n as isize)));
                } else if !l.is_empty() {
                    for _ in 0..width {
                        l.insert(0, ' ');
                    }
                    delta.push((i, width as isize));
                }
            }
        }
        for (an, hd) in lcs.iter_mut() {
            for p in [an, hd] {
                if let Some((_, d)) = delta.iter().find(|(i, _)| *i == p.0) {
                    p.1 = (p.1 as isize + d).max(0) as usize;
                }
            }
        }
    });
}

/// Ctrl+/: comment the lines out with `token` (or back in when they all are).
pub fn toggle_comment(chars: &mut Vec<char>, sels: &mut Vec<Sel>, token: &str) {
    let tok: Vec<char> = token.chars().collect();
    with_lines(chars, sels, |lines, bl, lcs| {
        let mut delta: Vec<(usize, usize, isize)> = Vec::new(); // (line, from column, change)
        for &(a, b) in bl {
            let busy = |l: &Vec<char>| l.iter().any(|c| !c.is_whitespace());
            let ind = |l: &Vec<char>| l.iter().take_while(|c| c.is_whitespace()).count();
            let all = (a..=b).filter(|i| busy(&lines[*i])).all(|i| {
                let k = ind(&lines[i]);
                lines[i][k..].starts_with(&tok)
            });
            let min_ind = (a..=b).filter(|i| busy(&lines[*i])).map(|i| ind(&lines[i])).min().unwrap_or(0);
            for (i, l) in lines.iter_mut().enumerate().take(b + 1).skip(a) {
                if !busy(l) {
                    continue;
                }
                if all {
                    let k = ind(l);
                    let mut n = tok.len();
                    if l.get(k + n) == Some(&' ') {
                        n += 1;
                    }
                    l.drain(k..k + n);
                    delta.push((i, k, -(n as isize)));
                } else {
                    let ins: Vec<char> = tok.iter().copied().chain([' ']).collect();
                    for (j, c) in ins.iter().enumerate() {
                        l.insert(min_ind + j, *c);
                    }
                    delta.push((i, min_ind, ins.len() as isize));
                }
            }
        }
        for (an, hd) in lcs.iter_mut() {
            for p in [an, hd] {
                if let Some((_, from, d)) = delta.iter().find(|(i, _, _)| *i == p.0)
                    && p.1 >= *from
                {
                    p.1 = (p.1 as isize + d).max(*from as isize) as usize;
                }
            }
        }
    });
}

/// Ctrl+Enter / Shift+Ctrl+Enter: a new line below / above, with the same indent, and the
/// cursor on it.
pub fn insert_line(chars: &mut Vec<char>, sels: &mut Vec<Sel>, above: bool) {
    with_lines(chars, sels, |lines, _bl, lcs| {
        let mut rows: Vec<usize> = lcs.iter().map(|(_, h)| h.0).collect();
        rows.sort();
        rows.dedup();
        let mut at: Vec<(usize, usize)> = Vec::new(); // (old line, new line index)
        for (added, r) in rows.into_iter().enumerate() {
            let r2 = r + added;
            let ind: Vec<char> = lines[r2].iter().take_while(|c| **c == ' ' || **c == '\t').copied().collect();
            let pos = if above { r2 } else { r2 + 1 };
            lines.insert(pos, ind.clone());
            at.push((r, pos));
        }
        for (an, hd) in lcs.iter_mut() {
            if let Some((_, p)) = at.iter().find(|(r, _)| *r == hd.0) {
                let col = lines[*p].len();
                *an = (*p, col);
                *hd = (*p, col);
            }
        }
    });
}

/// Ctrl+L: select the whole lines (again: one more line).
pub fn select_lines(chars: &[char], sels: &mut [Sel]) {
    for s in sels.iter_mut() {
        let a = line_start(chars, s.lo());
        let mut b = line_end(chars, s.hi());
        // Already whole lines: take the next one too.
        if s.lo() == a && s.hi() == line_start(chars, s.hi()) && s.hi() > s.lo() {
            b = line_end(chars, s.hi());
        }
        let b = if b < chars.len() { b + 1 } else { b };
        *s = Sel::new(a, b);
    }
}

/// The line comment for a file, by its name.
pub fn comment_token(name: &str) -> Option<&'static str> {
    let lower = name.to_lowercase();
    let ext = lower.rsplit_once('.').map(|(_, e)| e).unwrap_or(lower.as_str());
    Some(match ext {
        "py" | "pyw" | "sh" | "bash" | "zsh" | "fish" | "toml" | "yaml" | "yml" | "rb" | "pl" | "r" | "conf" | "cfg" | "ini" | "nix" | "mk" | "makefile" | "dockerfile" | "cmake" => "#",
        "lua" | "sql" | "hs" | "elm" => "--",
        "lisp" | "el" | "clj" | "scm" | "asm" => ";",
        "vim" => "\"",
        "md" | "markdown" | "txt" | "json" | "html" | "htm" | "xml" | "svg" | "css" => return None,
        _ => "//",
    })
}

/// The selected text of each selection, joined by new lines (for copying).
pub fn selected_text(chars: &[char], sels: &[Sel]) -> String {
    let mut v: Vec<Sel> = sels.to_vec();
    v.sort_by_key(Sel::lo);
    v.iter().map(|s| chars[s.lo().min(chars.len())..s.hi().min(chars.len())].iter().collect::<String>()).collect::<Vec<_>>().join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn c(s: &str) -> Vec<char> {
        s.chars().collect()
    }
    fn s(v: &[char]) -> String {
        v.iter().collect()
    }

    #[test]
    fn types_at_every_cursor() {
        let mut t = c("ab\ncd\nef");
        let mut sels = vec![Sel::caret(1), Sel::caret(4), Sel::caret(7)];
        insert(&mut t, &mut sels, "X");
        assert_eq!(s(&t), "aXb\ncXd\neXf");
        assert_eq!(sels, vec![Sel::caret(2), Sel::caret(6), Sel::caret(10)]);
        delete(&mut t, &mut sels, false);
        assert_eq!(s(&t), "ab\ncd\nef");
        newline(&mut c("  x").clone(), &mut vec![Sel::caret(3)]);
        let mut t = c("  x");
        let mut sels = vec![Sel::caret(3)];
        newline(&mut t, &mut sels);
        assert_eq!(s(&t), "  x\n  ");
    }

    #[test]
    fn ctrl_d_and_select_all() {
        let t = c("foo bar foo baz foo");
        let mut sels = vec![Sel::caret(1)];
        add_next_occurrence(&t, &mut sels);
        assert_eq!(sels, vec![Sel::new(0, 3)]);
        add_next_occurrence(&t, &mut sels);
        add_next_occurrence(&t, &mut sels);
        assert_eq!(sels.len(), 3);
        add_next_occurrence(&t, &mut sels);
        assert_eq!(sels.len(), 3, "wraps around without repeating");
        // Whole words: "foo" doesn't pick up "food".
        let t3 = c("foo food foo");
        let mut w = vec![Sel::caret(1)];
        select_all_occurrences(&t3, &mut w);
        assert_eq!(w.len(), 2);
        let mut sels = vec![Sel::caret(9)];
        select_all_occurrences(&t, &mut sels);
        assert_eq!(sels.len(), 3);
        assert_eq!(sels.last(), Some(&Sel::new(8, 11)));
        let mut t2 = t.clone();
        insert(&mut t2, &mut sels, "qux");
        assert_eq!(s(&t2), "qux bar qux baz qux");
    }

    #[test]
    fn cursors_above_and_moving() {
        let t = c("abc\nde\nfghij");
        let mut sels = vec![Sel::caret(2)];
        add_cursor_vertical(&t, &mut sels, true);
        add_cursor_vertical(&t, &mut sels, true);
        assert_eq!(sels.iter().map(|x| x.head).collect::<Vec<_>>(), [2, 6, 9]);
        move_sels(&t, &mut sels, Move::End, false);
        assert_eq!(sels.iter().map(|x| x.head).collect::<Vec<_>>(), [3, 6, 12]);
    }

    #[test]
    fn line_commands() {
        let mut t = c("a\nb\nc");
        let mut sels = vec![Sel::caret(2)];
        move_lines(&mut t, &mut sels, false);
        assert_eq!(s(&t), "b\na\nc");
        assert_eq!(sels, vec![Sel::caret(0)]);
        move_lines(&mut t, &mut sels, true);
        move_lines(&mut t, &mut sels, true);
        assert_eq!(s(&t), "a\nc\nb");
        let mut t = c("a\nb");
        let mut sels = vec![Sel::caret(0)];
        duplicate_lines(&mut t, &mut sels, true);
        assert_eq!(s(&t), "a\na\nb");
        assert_eq!(sels, vec![Sel::caret(2)]);
        delete_lines(&mut t, &mut sels);
        assert_eq!(s(&t), "a\nb");
        let mut t = c("fn x() {\n    y();\n}");
        let mut sels = vec![Sel::new(0, 15)];
        toggle_comment(&mut t, &mut sels, "//");
        assert_eq!(s(&t), "// fn x() {\n//     y();\n}");
        toggle_comment(&mut t, &mut sels, "//");
        assert_eq!(s(&t), "fn x() {\n    y();\n}");
        let mut t = c("a\nb");
        let mut sels = vec![Sel::new(0, 3)];
        indent_lines(&mut t, &mut sels, 4, false);
        assert_eq!(s(&t), "    a\n    b");
        indent_lines(&mut t, &mut sels, 4, true);
        assert_eq!(s(&t), "a\nb");
        let mut t = c("  a\nb");
        let mut sels = vec![Sel::caret(1)];
        insert_line(&mut t, &mut sels, false);
        assert_eq!(s(&t), "  a\n  \nb");
        assert_eq!(sels, vec![Sel::caret(6)]);
        let t = c("one\ntwo\nthree");
        let mut sels = vec![Sel::caret(5)];
        select_lines(&t, &mut sels);
        assert_eq!(sels, vec![Sel::new(4, 8)]);
        select_lines(&t, &mut sels);
        assert_eq!(sels, vec![Sel::new(4, 13)]);
    }
}
