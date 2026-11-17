//! Vim keys for the editor: Normal, Insert, Visual and Visual Line modes, counts, operators
//! (d c y > <) with motions and text objects, registers, `.` to repeat, `/` `?` `n` `N` `*` `#`
//! search, and `:` commands (w, q, wq, x, a line number, s///).
//!
//! It works on the text as characters and a cursor (a character index), the way `ops` does;
//! Insert mode's typing is left to the text box. Keys come in as characters, with the special
//! keys below standing for Esc, Enter, Backspace and the Ctrl ones.

pub const ESC: char = '\u{1b}';
pub const ENTER: char = '\r';
pub const BS: char = '\u{8}';
pub const CTRL_D: char = '\u{4}';
pub const CTRL_U: char = '\u{15}';
pub const CTRL_R: char = '\u{12}';
pub const CTRL_F: char = '\u{6}';
pub const CTRL_B: char = '\u{2}';
pub const HOME: char = '\u{1}';

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Normal,
    Insert,
    Visual,
    VisualLine,
}

/// What the editor around it should do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    Save,
    SaveAll,
    Close { force: bool },
    SaveClose,
    QuitAll { force: bool },
    Undo(usize),
    Redo(usize),
    /// Words for the status bar (`bad` ones in red).
    Message(String, bool),
    /// Bring the cursor into view (it moved a long way).
    Reveal,
    /// Copied: put on the clipboard as well.
    Yanked(String),
}

#[derive(Clone, Debug, Default)]
struct Register {
    text: String,
    lines: bool,
}

#[derive(Clone, Debug)]
struct Change {
    keys: Vec<char>,
    insert: Option<String>,
}

#[derive(Default)]
pub struct Vim {
    pub mode: Mode,
    /// Keys typed toward a command not finished yet ("2d", "f").
    keys: Vec<char>,
    /// The `:` / `/` / `?` line being typed: (its first character, the rest).
    pub cmdline: Option<(char, String)>,
    reg: Register,
    /// Last f / t / F / T and its character.
    find: Option<(char, char)>,
    /// Last search: (pattern, forward).
    search: Option<(String, bool)>,
    /// The column j and k aim for.
    want_col: Option<usize>,
    /// Where Visual mode started.
    pub anchor: usize,
    /// The cursor, as Vim sees it (in Visual mode the text box's is at the selection's edge).
    pub head: usize,
    last_change: Option<Change>,
    /// A change that went into Insert mode: its keys, and where typing started.
    open_change: Option<(Vec<char>, usize, usize)>,
    replaying: bool,
}

// ---------------------------------------------------------------- lines and words

fn ls(t: &[char], i: usize) -> usize {
    let mut j = i.min(t.len());
    while j > 0 && t[j - 1] != '\n' {
        j -= 1;
    }
    j
}

fn le(t: &[char], i: usize) -> usize {
    let mut j = i.min(t.len());
    while j < t.len() && t[j] != '\n' {
        j += 1;
    }
    j
}

fn line_no(t: &[char], i: usize) -> usize {
    t[..i.min(t.len())].iter().filter(|c| **c == '\n').count()
}

/// Lines, as Vim counts them (a final line break doesn't start another).
fn line_count(t: &[char]) -> usize {
    let n = t.iter().filter(|c| **c == '\n').count() + 1;
    if t.last() == Some(&'\n') { n - 1 } else { n }
}

/// Where line `n` (from 0) starts, the last line when past the end.
fn line_start(t: &[char], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut seen = 0;
    for (i, c) in t.iter().enumerate() {
        if *c == '\n' {
            seen += 1;
            if seen == n {
                return i + 1;
            }
        }
    }
    ls(t, t.len())
}

fn first_nb(t: &[char], i: usize) -> usize {
    let mut j = ls(t, i);
    let e = le(t, i);
    while j < e && (t[j] == ' ' || t[j] == '\t') {
        j += 1;
    }
    j
}

/// A Normal-mode cursor sits on a character, never past the line's last one.
pub fn clamp(t: &[char], i: usize) -> usize {
    let i = i.min(t.len());
    let (s, e) = (ls(t, i), le(t, i));
    if e > s { i.min(e - 1) } else { s }
}

fn cls(c: char, big: bool) -> u8 {
    if c.is_whitespace() {
        0
    } else if big || c.is_alphanumeric() || c == '_' {
        1
    } else {
        2
    }
}

fn next_word(t: &[char], i: usize, big: bool) -> usize {
    let n = t.len();
    if i >= n {
        return n;
    }
    let mut j = i;
    let c = cls(t[i], big);
    if c != 0 {
        while j < n && cls(t[j], big) == c {
            j += 1;
        }
    }
    while j < n && cls(t[j], big) == 0 {
        // An empty line counts as a word.
        if t[j] == '\n' && j + 1 < n && t[j + 1] == '\n' && j >= i {
            return j + 1;
        }
        j += 1;
    }
    j
}

fn prev_word(t: &[char], i: usize, big: bool) -> usize {
    if i == 0 {
        return 0;
    }
    let mut j = i - 1;
    while j > 0 && cls(t[j], big) == 0 {
        if t[j] == '\n' && t[j - 1] == '\n' {
            return j;
        }
        j -= 1;
    }
    let c = cls(t[j], big);
    while j > 0 && c != 0 && cls(t[j - 1], big) == c {
        j -= 1;
    }
    j
}

fn word_end(t: &[char], i: usize, big: bool) -> usize {
    let n = t.len();
    let mut j = i + 1;
    while j < n && cls(t[j], big) == 0 {
        j += 1;
    }
    if j >= n {
        return n.saturating_sub(1);
    }
    let c = cls(t[j], big);
    while j + 1 < n && cls(t[j + 1], big) == c {
        j += 1;
    }
    j
}

fn is_blank_line(t: &[char], s: usize) -> bool {
    le(t, s) == s
}

fn para_fwd(t: &[char], i: usize) -> usize {
    let mut seen = !is_blank_line(t, ls(t, i));
    let mut s = le(t, i) + 1;
    while s < t.len() {
        if is_blank_line(t, s) {
            if seen {
                return s;
            }
        } else {
            seen = true;
        }
        s = le(t, s) + 1;
    }
    t.len().saturating_sub(1)
}

fn para_back(t: &[char], i: usize) -> usize {
    let mut seen = !is_blank_line(t, ls(t, i));
    let mut s = ls(t, i);
    while s > 0 {
        s = ls(t, s - 1);
        if is_blank_line(t, s) {
            if seen {
                return s;
            }
        } else {
            seen = true;
        }
    }
    0
}

fn match_bracket(t: &[char], i: usize) -> Option<usize> {
    let e = le(t, i);
    let start = (i..e).find(|j| "()[]{}".contains(t[*j]))?;
    let c = t[start];
    let (open, close, fwd) = match c {
        '(' => ('(', ')', true),
        '[' => ('[', ']', true),
        '{' => ('{', '}', true),
        ')' => ('(', ')', false),
        ']' => ('[', ']', false),
        _ => ('{', '}', false),
    };
    let mut depth = 0i32;
    if fwd {
        for (j, ch) in t.iter().enumerate().skip(start) {
            if *ch == open {
                depth += 1;
            } else if *ch == close {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
        }
    } else {
        for j in (0..=start).rev() {
            if t[j] == close {
                depth += 1;
            } else if t[j] == open {
                depth -= 1;
                if depth == 0 {
                    return Some(j);
                }
            }
        }
    }
    None
}

/// An enclosing pair (open, close) around `i`.
fn enclosing(t: &[char], i: usize, open: char, close: char) -> Option<(usize, usize)> {
    let mut depth = 0i32;
    let mut o = None;
    let from = if i < t.len() && t[i] == close { i.checked_sub(1) } else { Some(i.min(t.len().saturating_sub(1))) };
    if i < t.len() && t[i] == open {
        o = Some(i);
    } else if let Some(from) = from {
        for j in (0..=from).rev() {
            if t[j] == close && j != i {
                depth += 1;
            } else if t[j] == open {
                if depth == 0 {
                    o = Some(j);
                    break;
                }
                depth -= 1;
            }
        }
    }
    let o = o?;
    let mut depth = 0;
    for (j, c) in t.iter().enumerate().skip(o) {
        if *c == open {
            depth += 1;
        } else if *c == close {
            depth -= 1;
            if depth == 0 {
                return Some((o, j));
            }
        }
    }
    None
}

// ---------------------------------------------------------------- what keys mean

#[derive(Clone, Copy, Debug, PartialEq)]
enum Motion {
    Left,
    Right,
    Up,
    Down,
    Word(bool),
    Back(bool),
    End(bool),
    LineStart,
    FirstNonBlank,
    LineEnd,
    FileStart,
    FileEnd,
    ParaFwd,
    ParaBack,
    Match,
    Find(char, char),
    Repeat(bool),
    NextLine,
    PrevLine,
    HalfDown,
    HalfUp,
    PageDown,
    PageUp,
    SearchNext(bool),
}

impl Motion {
    fn linewise(self) -> bool {
        matches!(self, Motion::Up | Motion::Down | Motion::FileStart | Motion::FileEnd | Motion::NextLine | Motion::PrevLine | Motion::HalfDown | Motion::HalfUp | Motion::PageDown | Motion::PageUp)
    }
}

enum Target {
    Motion(Motion),
    /// dd, cc, yy, >>, <<.
    Lines,
    /// i / a and the kind (w, ", (…).
    Object(char, char),
}

enum Parsed {
    More,
    Bad,
    Move(usize, Motion),
    Op(usize, char, Target),
    Simple(usize, char, Option<char>),
}

fn parse_motion(k: &[char]) -> Result<Option<(Motion, usize)>, ()> {
    let Some(&c) = k.first() else { return Ok(None) };
    let m = match c {
        'h' | BS => Motion::Left,
        'l' | ' ' => Motion::Right,
        'k' => Motion::Up,
        'j' => Motion::Down,
        'w' => Motion::Word(false),
        'W' => Motion::Word(true),
        'b' => Motion::Back(false),
        'B' => Motion::Back(true),
        'e' => Motion::End(false),
        'E' => Motion::End(true),
        '0' | HOME => Motion::LineStart,
        '^' | '_' => Motion::FirstNonBlank,
        '$' => Motion::LineEnd,
        'G' => Motion::FileEnd,
        '}' => Motion::ParaFwd,
        '{' => Motion::ParaBack,
        '%' => Motion::Match,
        ';' => Motion::Repeat(false),
        ',' => Motion::Repeat(true),
        '+' | ENTER => Motion::NextLine,
        '-' => Motion::PrevLine,
        CTRL_D => Motion::HalfDown,
        CTRL_U => Motion::HalfUp,
        CTRL_F => Motion::PageDown,
        CTRL_B => Motion::PageUp,
        'n' => Motion::SearchNext(false),
        'N' => Motion::SearchNext(true),
        'f' | 't' | 'F' | 'T' => {
            return match k.get(1) {
                None => Ok(None),
                Some(&ESC) => Err(()),
                Some(&ch) => Ok(Some((Motion::Find(c, ch), 2))),
            };
        }
        'g' => {
            return match k.get(1) {
                None => Ok(None),
                Some('g') => Ok(Some((Motion::FileStart, 2))),
                Some('e') => Ok(Some((Motion::Back(false), 2))),
                _ => Err(()),
            };
        }
        _ => return Err(()),
    };
    Ok(Some((m, 1)))
}

fn count_at(k: &[char], i: &mut usize) -> usize {
    let mut n = 0usize;
    while *i < k.len() && k[*i].is_ascii_digit() && !(k[*i] == '0' && n == 0) {
        n = (n * 10 + k[*i].to_digit(10).unwrap_or(0) as usize).min(99_999);
        *i += 1;
    }
    n
}

fn parse(k: &[char], visual: bool) -> Parsed {
    let mut i = 0;
    let n1 = count_at(k, &mut i);
    let Some(&c) = k.get(i) else { return Parsed::More };
    let count = n1.max(1);
    let ops: &[char] = if visual { &[] } else { &['d', 'c', 'y', '>', '<'] };
    if ops.contains(&c) {
        i += 1;
        let n2 = count_at(k, &mut i);
        let count = count * n2.max(1);
        let Some(&m) = k.get(i) else { return Parsed::More };
        if m == ESC {
            return Parsed::Bad;
        }
        if m == c {
            return Parsed::Op(count, c, Target::Lines);
        }
        if m == 'i' || m == 'a' {
            return match k.get(i + 1) {
                None => Parsed::More,
                Some(&o) => Parsed::Op(count, c, Target::Object(m, o)),
            };
        }
        return match parse_motion(&k[i..]) {
            Ok(None) => Parsed::More,
            Err(()) => Parsed::Bad,
            Ok(Some((mo, _))) => Parsed::Op(count, c, Target::Motion(mo)),
        };
    }
    if visual && (c == 'i' || c == 'a') {
        return match k.get(i + 1) {
            None => Parsed::More,
            Some(&o) => Parsed::Op(count, 'v', Target::Object(c, o)),
        };
    }
    // `gg` with a count is a line number.
    if c == 'g' && k.get(i + 1) == Some(&'g') {
        return Parsed::Move(n1, Motion::FileStart);
    }
    if c == 'G' {
        return Parsed::Move(n1, Motion::FileEnd);
    }
    match parse_motion(&k[i..]) {
        Ok(Some((m, _))) => return Parsed::Move(count, m),
        Ok(None) => return Parsed::More,
        Err(()) => {}
    }
    match c {
        'r' => match k.get(i + 1) {
            None => Parsed::More,
            Some(&ESC) => Parsed::Bad,
            Some(&ch) => Parsed::Simple(count, 'r', Some(ch)),
        },
        'z' => match k.get(i + 1) {
            None => Parsed::More,
            Some(&ch) => Parsed::Simple(count, 'z', Some(ch)),
        },
        'g' => match k.get(i + 1) {
            None => Parsed::More,
            Some('J') => Parsed::Simple(count, 'J', Some('g')),
            _ => Parsed::Bad,
        },
        _ => Parsed::Simple(count, c, None),
    }
}

/// Keys that change the text (and so can be repeated with `.`).
fn is_change(k: &[char]) -> bool {
    let mut i = 0;
    count_at(k, &mut i);
    matches!(k.get(i), Some('d' | 'c' | '>' | '<' | 'x' | 'X' | 's' | 'S' | 'C' | 'D' | 'p' | 'P' | 'J' | 'r' | 'i' | 'a' | 'I' | 'A' | 'o' | 'O' | '~'))
}

// ---------------------------------------------------------------- doing it

impl Vim {
    /// The mode's name for the status bar.
    pub fn mode_name(&self) -> &'static str {
        match self.mode {
            Mode::Normal => "NORMAL",
            Mode::Insert => "INSERT",
            Mode::Visual => "VISUAL",
            Mode::VisualLine => "V-LINE",
        }
    }

    /// Keys typed toward a command not finished yet ("2d", "f").
    pub fn typed(&self) -> String {
        self.keys.iter().filter(|c| !c.is_control()).collect()
    }

    /// The selection Visual mode shows: (from, to) as a range of characters.
    pub fn visual_range(&self, t: &[char], cur: usize) -> Option<(usize, usize)> {
        let (a, b) = (self.anchor.min(cur), self.anchor.max(cur));
        match self.mode {
            Mode::Visual => Some((a, (b + 1).min(t.len()))),
            Mode::VisualLine => Some((ls(t, a), (le(t, b) + 1).min(t.len()))),
            _ => None,
        }
    }

    fn motion(&mut self, t: &[char], cur: usize, m: Motion, count: usize, raw_count: usize) -> Option<usize> {
        let n = t.len();
        let line = line_no(t, cur);
        let lines = line_count(t);
        let col = cur - ls(t, cur);
        let vertical = |app: &mut Vim, to_line: usize| -> usize {
            let want = *app.want_col.get_or_insert(col);
            let s = line_start(t, to_line.min(lines - 1));
            let e = le(t, s);
            (s + want).min(if e > s { e - 1 } else { s })
        };
        let p = match m {
            Motion::Left => cur.saturating_sub(count).max(ls(t, cur)),
            Motion::Right => (cur + count).min(le(t, cur)),
            Motion::Up => vertical(self, line.saturating_sub(count)),
            Motion::Down => vertical(self, line + count),
            Motion::HalfDown => vertical(self, line + 15 * count),
            Motion::HalfUp => vertical(self, line.saturating_sub(15 * count)),
            Motion::PageDown => vertical(self, line + 30 * count),
            Motion::PageUp => vertical(self, line.saturating_sub(30 * count)),
            Motion::Word(big) => (0..count).fold(cur, |p, _| next_word(t, p, big)),
            Motion::Back(big) => (0..count).fold(cur, |p, _| prev_word(t, p, big)),
            Motion::End(big) => (0..count).fold(cur, |p, _| word_end(t, p, big)),
            Motion::LineStart => ls(t, cur),
            Motion::FirstNonBlank => first_nb(t, cur),
            Motion::LineEnd => {
                let s = line_start(t, line + count - 1);
                le(t, s).saturating_sub(1).max(s)
            }
            Motion::FileStart => first_nb(t, line_start(t, raw_count.saturating_sub(1))),
            Motion::FileEnd => first_nb(t, line_start(t, if raw_count == 0 { lines - 1 } else { raw_count - 1 })),
            Motion::ParaFwd => (0..count).fold(cur, |p, _| para_fwd(t, p)),
            Motion::ParaBack => (0..count).fold(cur, |p, _| para_back(t, p)),
            Motion::Match => match_bracket(t, cur)?,
            Motion::NextLine => first_nb(t, line_start(t, (line + count).min(lines - 1))),
            Motion::PrevLine => first_nb(t, line_start(t, line.saturating_sub(count))),
            Motion::Find(kind, ch) => {
                self.find = Some((kind, ch));
                self.find_char(t, cur, kind, ch, count)?
            }
            Motion::Repeat(rev) => {
                let (kind, ch) = self.find?;
                let kind = if rev {
                    match kind {
                        'f' => 'F',
                        'F' => 'f',
                        't' => 'T',
                        _ => 't',
                    }
                } else {
                    kind
                };
                // Repeating t / T steps past the character it stopped before.
                let from = match kind {
                    't' => cur + 1,
                    'T' => cur.saturating_sub(1),
                    _ => cur,
                };
                self.find_char(t, from, kind, ch, count)?
            }
            Motion::SearchNext(rev) => {
                let (pat, fwd) = self.search.clone()?;
                let mut p = cur;
                for _ in 0..count {
                    p = search(t, p, &pat, fwd != rev)?;
                }
                p
            }
        };
        if !matches!(m, Motion::Up | Motion::Down | Motion::HalfDown | Motion::HalfUp | Motion::PageDown | Motion::PageUp) {
            self.want_col = None;
        }
        Some(p.min(n))
    }

    fn find_char(&self, t: &[char], cur: usize, kind: char, ch: char, count: usize) -> Option<usize> {
        let (s, e) = (ls(t, cur), le(t, cur));
        let mut p = cur;
        for _ in 0..count {
            p = match kind {
                'f' | 't' => (p + 1..e).find(|j| t[*j] == ch)?,
                _ => (s..p).rev().find(|j| t[*j] == ch)?,
            };
        }
        Some(match kind {
            't' => p - 1,
            'T' => p + 1,
            _ => p,
        })
    }

    /// A text object's range (from, to) around `cur`.
    fn object(&self, t: &[char], cur: usize, around: bool, kind: char) -> Option<(usize, usize)> {
        match kind {
            'w' | 'W' => {
                let big = kind == 'W';
                if t.is_empty() {
                    return None;
                }
                let cur = cur.min(t.len() - 1);
                let c = cls(t[cur], big);
                let (mut a, mut b) = (cur, cur + 1);
                while a > 0 && cls(t[a - 1], big) == c && t[a - 1] != '\n' {
                    a -= 1;
                }
                while b < t.len() && cls(t[b], big) == c && t[b] != '\n' {
                    b += 1;
                }
                if around {
                    let b0 = b;
                    while b < t.len() && (t[b] == ' ' || t[b] == '\t') {
                        b += 1;
                    }
                    if b == b0 {
                        while a > ls(t, a) && (t[a - 1] == ' ' || t[a - 1] == '\t') {
                            a -= 1;
                        }
                    }
                }
                Some((a, b))
            }
            '"' | '\'' | '`' => {
                let (s, e) = (ls(t, cur), le(t, cur));
                let qs: Vec<usize> = (s..e).filter(|j| t[*j] == kind && (*j == 0 || t[*j - 1] != '\\')).collect();
                // The pair around the cursor, else the next pair on the line.
                let pair = qs.chunks(2).filter(|p| p.len() == 2).find(|p| p[0] <= cur && cur <= p[1]).or_else(|| qs.chunks(2).filter(|p| p.len() == 2).find(|p| p[0] > cur))?;
                let (o, c) = (pair[0], pair[1]);
                Some(if around { (o, c + 1) } else { (o + 1, c) })
            }
            '(' | ')' | 'b' | '[' | ']' | '{' | '}' | 'B' | '<' | '>' => {
                let (open, close) = match kind {
                    '(' | ')' | 'b' => ('(', ')'),
                    '[' | ']' => ('[', ']'),
                    '<' | '>' => ('<', '>'),
                    _ => ('{', '}'),
                };
                let (o, c) = enclosing(t, cur, open, close)?;
                if around {
                    return Some((o, c + 1));
                }
                // A block over several lines: the lines inside, not the brackets' lines.
                let (mut a, mut b) = (o + 1, c);
                if a < t.len() && t[a] == '\n' && ls(t, c) > a {
                    a += 1;
                    b = ls(t, c);
                }
                Some((a, b))
            }
            _ => None,
        }
    }

    /// Feed one key; the text and cursor change in place.
    pub fn key(&mut self, k: char, t: &mut Vec<char>, cur: &mut usize, unit: &str, width: usize) -> Vec<Effect> {
        let mut fx = Vec::new();
        if let Some((kind, mut line)) = self.cmdline.take() {
            match k {
                ESC => {}
                ENTER => self.run_cmdline(kind, &line, t, cur, &mut fx),
                BS => {
                    if line.pop().is_some() {
                        self.cmdline = Some((kind, line));
                    }
                }
                c if !c.is_control() => {
                    line.push(c);
                    self.cmdline = Some((kind, line));
                }
                _ => self.cmdline = Some((kind, line)),
            }
            return fx;
        }
        match self.mode {
            Mode::Insert => {
                if k == ESC {
                    self.leave_insert(t, cur);
                }
            }
            Mode::Normal | Mode::Visual | Mode::VisualLine => {
                if k == ESC {
                    if self.keys.is_empty() {
                        self.mode = Mode::Normal;
                    }
                    self.keys.clear();
                    *cur = clamp(t, *cur);
                    return fx;
                }
                self.keys.push(k);
                let visual = self.mode != Mode::Normal;
                let keys = self.keys.clone();
                match parse(&keys, visual) {
                    Parsed::More => return fx,
                    Parsed::Bad => self.keys.clear(),
                    p => {
                        self.keys.clear();
                        let entered_insert_before = self.mode == Mode::Insert;
                        let before = t.clone();
                        self.run(p, t, cur, unit, width, &mut fx);
                        if !visual && !self.replaying && is_change(&keys) && !entered_insert_before {
                            if self.mode == Mode::Insert {
                                self.open_change = Some((keys, *cur, t.len()));
                            } else if *t != before {
                                self.last_change = Some(Change { keys, insert: None });
                            }
                        }
                    }
                }
                if self.mode == Mode::Normal {
                    *cur = clamp(t, *cur);
                }
            }
        }
        fx
    }

    fn leave_insert(&mut self, t: &[char], cur: &mut usize) {
        self.mode = Mode::Normal;
        if let Some((keys, from, len)) = self.open_change.take() {
            let typed = *cur >= from && t.len() >= len && t.len() - len == *cur - from;
            let insert = typed.then(|| t[from..*cur].iter().collect());
            self.last_change = Some(Change { keys, insert });
        }
        if *cur > ls(t, *cur) {
            *cur -= 1;
        }
        *cur = clamp(t, *cur);
    }

    /// Typing done by the text box in Insert mode is part of the change `.` repeats; when
    /// Esc comes from there the caller says so with `insert_done`.
    pub fn insert_done(&mut self, t: &[char], cur: &mut usize) {
        self.leave_insert(t, cur);
    }

    fn yank(&mut self, text: String, lines: bool, fx: &mut Vec<Effect>) {
        fx.push(Effect::Yanked(text.clone()));
        self.reg = Register { text, lines };
    }

    fn run(&mut self, p: Parsed, t: &mut Vec<char>, cur: &mut usize, unit: &str, width: usize, fx: &mut Vec<Effect>) {
        let visual = self.mode == Mode::Visual || self.mode == Mode::VisualLine;
        match p {
            Parsed::Move(count, m) => {
                let raw = count;
                let count = count.max(1);
                if let Some(p) = self.motion(t, *cur, m, count, raw) {
                    if matches!(m, Motion::FileStart | Motion::FileEnd | Motion::HalfDown | Motion::HalfUp | Motion::PageDown | Motion::PageUp | Motion::SearchNext(_) | Motion::ParaFwd | Motion::ParaBack) {
                        fx.push(Effect::Reveal);
                    }
                    *cur = p;
                } else if matches!(m, Motion::SearchNext(_)) {
                    fx.push(Effect::Message(match &self.search {
                        Some((pat, _)) => format!("Pattern not found: {pat}"),
                        None => "No previous search".into(),
                    }, true));
                }
            }
            Parsed::Op(count, op, target) => {
                // The range it covers, and whether whole lines.
                let range = match target {
                    Target::Lines => {
                        let l = line_no(t, *cur);
                        Some((line_start(t, l), line_start(t, l + count - 1), true))
                    }
                    Target::Object(ai, kind) => self.object(t, *cur, ai == 'a', kind).map(|(a, b)| (a, b, false)),
                    Target::Motion(m) => {
                        // cw changes to the end of the word it's on (not to the next word), like Vim.
                        if let (Motion::Word(big), 'c') = (m, op)
                            && *cur < t.len()
                            && cls(t[*cur], big) != 0
                        {
                            let c0 = cls(t[*cur], big);
                            let mut b = *cur;
                            while b < t.len() && cls(t[b], big) == c0 {
                                b += 1;
                            }
                            for _ in 1..count {
                                b = word_end(t, b, big) + 1;
                            }
                            let b = b.min(t.len());
                            return self.apply('c', *cur, b, false, t, cur, unit, width, fx);
                        }
                        self.motion(t, *cur, m, count, count).map(|p| {
                            let inclusive = matches!(m, Motion::End(_) | Motion::LineEnd | Motion::Find(..) | Motion::Repeat(_) | Motion::Match);
                            let (mut a, mut b) = ((*cur).min(p), (*cur).max(p));
                            if m.linewise() {
                                return (a, b, true);
                            }
                            if inclusive {
                                b = (b + 1).min(t.len());
                            }
                            // dw at a line's end doesn't take the line break.
                            if matches!(m, Motion::Word(_)) && b > le(t, a) && le(t, a) > a {
                                b = le(t, a);
                            }
                            if a > b {
                                std::mem::swap(&mut a, &mut b);
                            }
                            (a, b, false)
                        })
                    }
                };
                let Some((a, b, lines)) = range else { return };
                if op == 'v' {
                    // viw, vi( …: select it.
                    self.anchor = a;
                    *cur = b.saturating_sub(1).max(a);
                    return;
                }
                self.apply(op, a, b, lines, t, cur, unit, width, fx);
            }
            Parsed::Simple(count, c, extra) => self.simple(count, c, extra, visual, t, cur, unit, width, fx),
            _ => {}
        }
    }

    /// An operator over (a, b): characters, or the whole lines from a's to b's.
    #[allow(clippy::too_many_arguments)]
    fn apply(&mut self, op: char, a: usize, b: usize, lines: bool, t: &mut Vec<char>, cur: &mut usize, unit: &str, width: usize, fx: &mut Vec<Effect>) {
        let (s, e) = if lines { (ls(t, a), (le(t, b) + 1).min(t.len())) } else { (a, b) };
        match op {
            'y' => {
                let mut text: String = t[s..e].iter().collect();
                if lines && !text.ends_with('\n') {
                    text.push('\n');
                }
                self.yank(text, lines, fx);
                *cur = if lines { (*cur).min(s).max(s) } else { s };
            }
            'd' | 'c' => {
                let mut text: String = t[s..e].iter().collect();
                if lines && !text.ends_with('\n') {
                    text.push('\n');
                }
                self.yank(text, lines, fx);
                if lines && op == 'c' {
                    // cc keeps the first line's indentation and the line itself.
                    let indent: Vec<char> = t[s..first_nb(t, s)].to_vec();
                    let end = le(t, b);
                    t.splice(s..end, indent.iter().copied());
                    *cur = s + indent.len();
                    self.mode = Mode::Insert;
                    return;
                }
                if lines {
                    // The last line: take the break before it instead.
                    let (s2, e2) = if e == t.len() && s > 0 && (e == s || t[e - 1] != '\n') { (s - 1, e) } else { (s, e) };
                    t.drain(s2..e2);
                    *cur = first_nb(t, s2.min(t.len()));
                    if s2 < s {
                        *cur = first_nb(t, ls(t, s2));
                    }
                } else {
                    t.drain(s..e);
                    *cur = s;
                }
                if op == 'c' {
                    self.mode = Mode::Insert;
                } else {
                    self.mode = Mode::Normal;
                }
            }
            '>' | '<' => {
                let (l0, l1) = (line_no(t, s), line_no(t, if lines { e.saturating_sub(1).max(s) } else { b }));
                for l in l0..=l1 {
                    let st = line_start(t, l);
                    if op == '>' {
                        if le(t, st) > st {
                            t.splice(st..st, unit.chars());
                        }
                    } else {
                        let n = if t.get(st) == Some(&'\t') { 1 } else { t[st..].iter().take(width.max(1)).take_while(|c| **c == ' ').count() };
                        t.drain(st..st + n);
                    }
                }
                *cur = first_nb(t, line_start(t, l0));
                self.mode = Mode::Normal;
            }
            '~' | 'u' | 'U' => {
                let e = e.min(t.len());
                for c in t[s..e].iter_mut() {
                    *c = match op {
                        'u' => c.to_lowercase().next().unwrap_or(*c),
                        'U' => c.to_uppercase().next().unwrap_or(*c),
                        _ if c.is_lowercase() => c.to_uppercase().next().unwrap_or(*c),
                        _ => c.to_lowercase().next().unwrap_or(*c),
                    };
                }
                *cur = s;
                self.mode = Mode::Normal;
            }
            'J' => {
                let (l0, l1) = (line_no(t, s), line_no(t, b).max(line_no(t, s) + 1));
                for _ in l0..l1 {
                    let e = le(t, line_start(t, l0));
                    if e >= t.len() {
                        break;
                    }
                    let mut k = e + 1;
                    while k < t.len() && (t[k] == ' ' || t[k] == '\t') {
                        k += 1;
                    }
                    let space = !(k >= t.len() || t[k] == '\n' || t[k] == ')' || e == ls(t, e) || t[e - 1] == ' ');
                    t.splice(e..k, if space { vec![' '] } else { vec![] });
                    *cur = e;
                }
                self.mode = Mode::Normal;
            }
            _ => {}
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn simple(&mut self, count: usize, c: char, extra: Option<char>, visual: bool, t: &mut Vec<char>, cur: &mut usize, unit: &str, width: usize, fx: &mut Vec<Effect>) {
        // Visual mode: the operators act on the selection.
        if visual {
            let Some((a, b)) = self.visual_range(t, *cur) else { return };
            let lines = self.mode == Mode::VisualLine;
            let (va, vb) = (self.anchor.min(*cur), self.anchor.max(*cur));
            match c {
                'd' | 'x' | 'X' | 'D' => return self.apply('d', if lines { va } else { a }, if lines { vb } else { b }, lines, t, cur, unit, width, fx),
                'y' | 'Y' => {
                    self.apply('y', if lines { va } else { a }, if lines { vb } else { b }, lines, t, cur, unit, width, fx);
                    self.mode = Mode::Normal;
                    return;
                }
                'c' | 's' | 'S' | 'C' => return self.apply('c', if lines { va } else { a }, if lines { vb } else { b }, lines, t, cur, unit, width, fx),
                '>' | '<' => return self.apply(c, va, vb, true, t, cur, unit, width, fx),
                '~' | 'u' | 'U' => return self.apply(c, a, b, false, t, cur, unit, width, fx),
                'J' => return self.apply('J', va, vb, true, t, cur, unit, width, fx),
                'o' => {
                    std::mem::swap(&mut self.anchor, cur);
                    return;
                }
                'v' => {
                    self.mode = if self.mode == Mode::Visual { Mode::Normal } else { Mode::Visual };
                    return;
                }
                'V' => {
                    self.mode = if self.mode == Mode::VisualLine { Mode::Normal } else { Mode::VisualLine };
                    return;
                }
                'p' | 'P' => {
                    let reg = self.reg.clone();
                    t.drain(a..b);
                    let ins: Vec<char> = reg.text.chars().collect();
                    let n = ins.len();
                    t.splice(a..a, ins);
                    *cur = (a + n).saturating_sub(1);
                    self.mode = Mode::Normal;
                    return;
                }
                ':' => {
                    self.mode = Mode::Normal;
                    self.cmdline = Some((':', String::new()));
                    return;
                }
                _ => return,
            }
        }
        match c {
            'i' => self.mode = Mode::Insert,
            'a' => {
                if le(t, *cur) > *cur {
                    *cur += 1;
                }
                self.mode = Mode::Insert;
            }
            'I' => {
                *cur = first_nb(t, *cur);
                self.mode = Mode::Insert;
            }
            'A' => {
                *cur = le(t, *cur);
                self.mode = Mode::Insert;
            }
            'o' | 'O' => {
                let indent: Vec<char> = t[ls(t, *cur)..first_nb(t, *cur)].to_vec();
                if c == 'o' {
                    let e = le(t, *cur);
                    let mut ins = vec!['\n'];
                    ins.extend(&indent);
                    t.splice(e..e, ins);
                    *cur = e + 1 + indent.len();
                } else {
                    let s = ls(t, *cur);
                    let mut ins = indent.clone();
                    ins.push('\n');
                    t.splice(s..s, ins);
                    *cur = s + indent.len();
                }
                self.mode = Mode::Insert;
            }
            'x' => {
                let e = le(t, *cur);
                if e > *cur {
                    self.apply('d', *cur, (*cur + count).min(e), false, t, cur, unit, width, fx);
                }
            }
            'X' => {
                let s = ls(t, *cur);
                if *cur > s {
                    let a = cur.saturating_sub(count).max(s);
                    self.apply('d', a, *cur, false, t, cur, unit, width, fx);
                }
            }
            's' => {
                let e = le(t, *cur);
                self.apply('c', *cur, (*cur + count).min(e), false, t, cur, unit, width, fx);
            }
            'S' => self.apply('c', *cur, *cur, true, t, cur, unit, width, fx),
            'C' | 'D' => {
                let e = le(t, line_start(t, line_no(t, *cur) + count - 1));
                self.apply(if c == 'C' { 'c' } else { 'd' }, *cur, e, false, t, cur, unit, width, fx);
            }
            'Y' => {
                let l = line_no(t, *cur);
                let (a, b) = (line_start(t, l), line_start(t, l + count - 1));
                let keep = *cur;
                self.apply('y', a, b, true, t, cur, unit, width, fx);
                *cur = keep;
            }
            'p' | 'P' => {
                let reg = self.reg.clone();
                if reg.text.is_empty() {
                    return;
                }
                let one: String = reg.text.clone();
                let text: String = one.repeat(count);
                if reg.lines {
                    let body: Vec<char> = text.chars().collect();
                    let mut pushed = false;
                    let at = if c == 'p' {
                        let e = le(t, *cur);
                        if e < t.len() {
                            e + 1
                        } else {
                            // After the last line, which has no line break.
                            t.push('\n');
                            pushed = true;
                            t.len()
                        }
                    } else {
                        ls(t, *cur)
                    };
                    let mut body = body;
                    if pushed && body.last() == Some(&'\n') {
                        body.pop();
                    }
                    t.splice(at..at, body);
                    *cur = first_nb(t, at);
                } else {
                    let at = if c == 'p' && le(t, *cur) > *cur { *cur + 1 } else { *cur };
                    let body: Vec<char> = text.chars().collect();
                    let n = body.len();
                    t.splice(at..at, body);
                    *cur = (at + n).saturating_sub(1);
                }
            }
            'J' => {
                let l = line_no(t, *cur);
                let b = line_start(t, l + count.max(2) - 1);
                let keep_space = extra != Some('g');
                if keep_space {
                    self.apply('J', *cur, b, true, t, cur, unit, width, fx);
                } else {
                    // gJ: join without a space.
                    for _ in 0..count.max(2) - 1 {
                        let e = le(t, *cur);
                        if e < t.len() {
                            t.remove(e);
                            *cur = e;
                        }
                    }
                }
            }
            '~' => {
                let e = le(t, *cur);
                let b = (*cur + count).min(e);
                if b > *cur {
                    self.apply('~', *cur, b, false, t, cur, unit, width, fx);
                    *cur = b.min(e.saturating_sub(1).max(ls(t, b)));
                }
            }
            'r' => {
                let Some(ch) = extra else { return };
                let e = le(t, *cur);
                if *cur + count <= e {
                    for k in 0..count {
                        t[*cur + k] = ch;
                    }
                    *cur += count - 1;
                }
            }
            'u' => fx.push(Effect::Undo(count)),
            CTRL_R => fx.push(Effect::Redo(count)),
            'v' => {
                self.mode = Mode::Visual;
                self.anchor = *cur;
            }
            'V' => {
                self.mode = Mode::VisualLine;
                self.anchor = *cur;
            }
            '.' => {
                if let Some(ch) = self.last_change.clone() {
                    self.replaying = true;
                    for _ in 0..count {
                        for k in &ch.keys {
                            let more = self.key(*k, t, cur, unit, width);
                            fx.extend(more);
                        }
                        if self.mode == Mode::Insert {
                            if let Some(ins) = &ch.insert {
                                let body: Vec<char> = ins.chars().collect();
                                let n = body.len();
                                t.splice(*cur..*cur, body);
                                *cur += n;
                            }
                            self.mode = Mode::Normal;
                            if *cur > ls(t, *cur) {
                                *cur -= 1;
                            }
                        }
                    }
                    self.replaying = false;
                }
            }
            '*' | '#' => {
                let Some((a, b)) = self.object(t, *cur, false, 'w') else { return };
                let word: String = t[a..b].iter().collect();
                if word.trim().is_empty() {
                    return;
                }
                let pat = format!(r"\b{}\b", regex::escape(&word));
                self.search = Some((pat.clone(), c == '*'));
                if let Some(p) = search(t, if c == '*' { b.saturating_sub(1) } else { a }, &pat, c == '*') {
                    *cur = p;
                    fx.push(Effect::Reveal);
                }
            }
            ':' | '/' | '?' => self.cmdline = Some((c, String::new())),
            'z' => fx.push(Effect::Reveal),
            ENTER => {}
            _ => {}
        }
    }

    fn run_cmdline(&mut self, kind: char, line: &str, t: &mut Vec<char>, cur: &mut usize, fx: &mut Vec<Effect>) {
        if kind == '/' || kind == '?' {
            let pat = if line.is_empty() { self.search.as_ref().map(|s| s.0.clone()).unwrap_or_default() } else { smart_case(line) };
            if pat.is_empty() {
                return;
            }
            self.search = Some((pat.clone(), kind == '/'));
            match search(t, *cur, &pat, kind == '/') {
                Some(p) => {
                    *cur = p;
                    fx.push(Effect::Reveal);
                }
                None => fx.push(Effect::Message(format!("Pattern not found: {line}"), true)),
            }
            return;
        }
        let cmd = line.trim();
        if let Ok(n) = cmd.parse::<usize>() {
            *cur = first_nb(t, line_start(t, n.saturating_sub(1)));
            fx.push(Effect::Reveal);
            return;
        }
        if let Some(rest) = cmd.strip_prefix("%s").or_else(|| cmd.strip_prefix("s")).filter(|r| r.starts_with('/')) {
            let all = cmd.starts_with('%');
            match substitute(t, *cur, rest, all) {
                Ok(0) => fx.push(Effect::Message("Pattern not found".into(), true)),
                Ok(n) => {
                    fx.push(Effect::Message(format!("{n} substitution{}", if n == 1 { "" } else { "s" }), false));
                    *cur = clamp(t, (*cur).min(t.len()));
                }
                Err(e) => fx.push(Effect::Message(e, true)),
            }
            return;
        }
        match cmd {
            "w" | "write" | "up" | "update" => fx.push(Effect::Save),
            "wa" | "wall" => fx.push(Effect::SaveAll),
            "q" | "quit" | "clo" | "close" | "bd" => fx.push(Effect::Close { force: false }),
            "q!" | "quit!" | "bd!" => fx.push(Effect::Close { force: true }),
            "wq" | "x" | "xit" | "wq!" | "x!" => fx.push(Effect::SaveClose),
            "qa" | "qall" | "qa!" | "qall!" => fx.push(Effect::QuitAll { force: cmd.ends_with('!') }),
            "wqa" | "xa" => {
                fx.push(Effect::SaveAll);
                fx.push(Effect::QuitAll { force: false });
            }
            "noh" | "nohlsearch" | "" => {}
            "$" => {
                *cur = first_nb(t, line_start(t, line_count(t) - 1));
                fx.push(Effect::Reveal);
            }
            _ => fx.push(Effect::Message(format!("Not an editor command: {cmd}"), true)),
        }
    }
}

/// Lower-case patterns match any case (Vim's smartcase).
fn smart_case(p: &str) -> String {
    if p.chars().any(char::is_uppercase) { p.to_string() } else { format!("(?i){p}") }
}

fn compile(pat: &str) -> Option<regex::Regex> {
    regex::Regex::new(pat).ok().or_else(|| {
        let (flag, rest) = pat.strip_prefix("(?i)").map(|r| ("(?i)", r)).unwrap_or(("", pat));
        regex::Regex::new(&format!("{flag}{}", regex::escape(rest))).ok()
    })
}

/// The next match of `pat` after `cur` (before it, going back), wrapping round.
fn search(t: &[char], cur: usize, pat: &str, fwd: bool) -> Option<usize> {
    let re = compile(pat)?;
    let s: String = t.iter().collect();
    let starts: Vec<usize> = re.find_iter(&s).filter(|m| !m.is_empty()).map(|m| s[..m.start()].chars().count()).collect();
    if fwd {
        starts.iter().copied().find(|p| *p > cur).or(starts.first().copied())
    } else {
        starts.iter().rev().copied().find(|p| *p < cur).or(starts.last().copied())
    }
}

/// `:s/pat/with/flags` on the cursor's line (or every line with `%`). How many were replaced.
fn substitute(t: &mut Vec<char>, cur: usize, spec: &str, all_lines: bool) -> Result<usize, String> {
    let body = &spec[1..];
    let mut parts = Vec::new();
    let mut part = String::new();
    let mut esc = false;
    for c in body.chars() {
        if esc {
            if c != '/' {
                part.push('\\');
            }
            part.push(c);
            esc = false;
        } else if c == '\\' {
            esc = true;
        } else if c == '/' {
            parts.push(std::mem::take(&mut part));
        } else {
            part.push(c);
        }
    }
    parts.push(part);
    let pat = parts.first().cloned().unwrap_or_default();
    let with = parts.get(1).cloned().unwrap_or_default().replace('&', "$0").replace("\\0", "$0");
    let flags = parts.get(2).cloned().unwrap_or_default();
    let mut pat = if flags.contains('i') { format!("(?i){pat}") } else { smart_case(&pat) };
    if flags.contains('I') {
        pat = pat.trim_start_matches("(?i)").to_string();
    }
    let re = compile(&pat).ok_or_else(|| format!("Bad pattern: {pat}"))?;
    let global = flags.contains('g');
    let (a, b) = if all_lines { (0, t.len()) } else { (ls(t, cur), le(t, cur)) };
    let region: String = t[a..b].iter().collect();
    let mut n = 0;
    let out: String = region
        .split('\n')
        .map(|line| {
            let hits = re.find_iter(line).count();
            if hits == 0 {
                return line.to_string();
            }
            if global {
                n += hits;
                re.replace_all(line, with.as_str()).into_owned()
            } else {
                n += 1;
                re.replace(line, with.as_str()).into_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    t.splice(a..b, out.chars());
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run keys over text with the cursor at `at`; the text and cursor after.
    fn run(text: &str, at: usize, keys: &str) -> (String, usize, Vim) {
        let mut v = Vim::default();
        let mut t: Vec<char> = text.chars().collect();
        let mut cur = at;
        for k in keys.chars() {
            let k = match k {
                '⎋' => ESC,
                '⏎' => ENTER,
                c => c,
            };
            // Insert mode's typing is the text box's: do it here for the tests.
            if v.mode == Mode::Insert && k != ESC {
                t.insert(cur, k);
                cur += 1;
                continue;
            }
            v.key(k, &mut t, &mut cur, "    ", 4);
        }
        (t.into_iter().collect(), cur, v)
    }

    #[test]
    fn motions() {
        let t = "fn main() {\n    let x = 1;\n}\n";
        assert_eq!(run(t, 0, "w").1, 3);
        assert_eq!(run(t, 0, "2w").1, 7);
        assert_eq!(run(t, 0, "3w").1, 10);
        assert_eq!(run(t, 0, "e").1, 1);
        assert_eq!(run(t, 0, "$").1, 10);
        assert_eq!(run(t, 0, "j").1, 12);
        assert_eq!(run(t, 0, "j^").1, 16);
        assert_eq!(run(t, 0, "G").1, 27);
        assert_eq!(run(t, 20, "gg").1, 0);
        assert_eq!(run(t, 0, "2G").1, 16);
        assert_eq!(run(t, 0, "f(").1, 7);
        assert_eq!(run(t, 0, "t(").1, 6);
        assert_eq!(run(t, 0, "f{%").1, 27);
        assert_eq!(run(t, 0, "fn;").1, 6);
    }

    #[test]
    fn operators_and_objects() {
        assert_eq!(run("one two three", 0, "dw").0, "two three");
        assert_eq!(run("one two three", 4, "cwxx⎋").0, "one xx three");
        assert_eq!(run("one two three", 5, "ciwhey⎋").0, "one hey three");
        assert_eq!(run("one two three", 5, "daw").0, "one three");
        assert_eq!(run("a\nb\nc\n", 2, "dd").0, "a\nc\n");
        assert_eq!(run("a\nb\nc", 4, "dd").0, "a\nb");
        assert_eq!(run("a\nb\nc\n", 0, "yyjp").0, "a\nb\na\nc\n");
        assert_eq!(run("a\nb\n", 0, "ddp").0, "b\na\n");
        assert_eq!(run("f(a, b)", 3, "di(").0, "f()");
        assert_eq!(run("say \"hi there\" ok", 6, "ci\"yo⎋").0, "say \"yo\" ok");
        assert_eq!(run("abc", 0, "x").0, "bc");
        assert_eq!(run("abc", 0, "2x").0, "c");
        assert_eq!(run("abc def", 1, "D").0, "a");
        assert_eq!(run("abc def", 1, "C!⎋").0, "a!");
        assert_eq!(run("a\n  b\n", 0, "J").0, "a b\n");
        assert_eq!(run("x\n", 0, ">>").0, "    x\n");
        assert_eq!(run("    x\n", 0, "<<").0, "x\n");
        assert_eq!(run("abc", 0, "rz").0, "zbc");
        assert_eq!(run("abc", 0, "~~").0, "ABc");
        assert_eq!(run("  a\n", 2, "otwo⎋").0, "  a\n  two\n");
        assert_eq!(run("a\n", 0, "Oz⎋").0, "z\na\n");
        assert_eq!(run("one two", 0, "A!⎋").0, "one two!");
        assert_eq!(run("one two", 4, "I-⎋").0, "-one two");
    }

    #[test]
    fn visual_repeat_search_and_commands() {
        assert_eq!(run("one two three", 0, "ved").0, " two three");
        assert_eq!(run("a\nb\nc\n", 0, "Vjd").0, "c\n");
        assert_eq!(run("a b c d", 0, "dw..").0, "d");
        assert_eq!(run("a b c d", 0, "cwX⎋w.").0, "X X c d");
        let (_, cur, _) = run("foo bar foo baz foo", 0, "/foo⏎");
        assert_eq!(cur, 8);
        let (_, cur, _) = run("foo bar foo baz foo", 0, "/foo⏎n");
        assert_eq!(cur, 16);
        assert_eq!(run("foo bar foo baz foo", 0, "*").1, 8);
        assert_eq!(run("a-a a-a\nb-a", 0, ":%s/a/X/g⏎").0, "X-X X-X\nb-X");
        assert_eq!(run("aa\naa", 0, ":s/a/b/⏎").0, "ba\naa");
        let mut v = Vim::default();
        let mut t: Vec<char> = "x".chars().collect();
        let mut c = 0;
        for k in ":wq".chars() {
            v.key(k, &mut t, &mut c, "    ", 4);
        }
        assert_eq!(v.key(ENTER, &mut t, &mut c, "    ", 4), vec![Effect::SaveClose]);
        assert_eq!(run("a\nb\nc\nd\n", 0, ":3⏎").1, 4);
        assert_eq!(run("abc", 0, "u").2.mode, Mode::Normal);
    }
}
