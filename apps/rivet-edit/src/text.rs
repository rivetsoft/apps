//! An open file's text, with a number that changes on every edit and where each line starts.
//!
//! Everything that used to compare the whole text with a copy to learn whether it changed (undo,
//! the Git gutter, the colors, the language server) compares the number instead, and the status
//! bar and gutter read the line starts rather than counting through the text every frame.

use std::ops::{Deref, Range};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use eframe::egui::{TextBuffer, text::CharIndex};

/// Edit numbers are unique across every text in the program, so a number seen for one tab is
/// never mistaken for another's, even after a tab with the same file is closed and reopened.
static NEXT: AtomicU64 = AtomicU64::new(1);

fn next() -> u64 {
    NEXT.fetch_add(1, Ordering::Relaxed)
}

pub struct Text {
    s: String,
    /// Shared so the layout code can read it while the text box holds the text.
    generation: Arc<AtomicU64>,
    /// The character index each line starts at; the first is always 0.
    starts: Vec<usize>,
    chars: usize,
}

impl Default for Text {
    fn default() -> Self {
        Text::new(String::new())
    }
}

impl Deref for Text {
    type Target = String;
    fn deref(&self) -> &String {
        &self.s
    }
}

impl Text {
    pub fn new(s: String) -> Text {
        let (starts, chars) = line_starts(&s);
        Text { s, generation: Arc::new(AtomicU64::new(next())), starts, chars }
    }

    /// The edit number: different after every change.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// The edit number as it is when read, for code that can't borrow the text then.
    pub fn generation_handle(&self) -> Arc<AtomicU64> {
        self.generation.clone()
    }

    pub fn set(&mut self, s: String) {
        (self.starts, self.chars) = line_starts(&s);
        self.s = s;
        self.bump();
    }

    /// Change the text in place (the line starts are counted again afterwards).
    pub fn edit<R>(&mut self, f: impl FnOnce(&mut String) -> R) -> R {
        let r = f(&mut self.s);
        (self.starts, self.chars) = line_starts(&self.s);
        self.bump();
        r
    }

    pub fn replace_range(&mut self, bytes: Range<usize>, with: &str) {
        self.edit(|s| s.replace_range(bytes, with));
    }

    fn bump(&mut self) {
        self.generation.store(next(), Ordering::Relaxed);
    }

    pub fn char_count(&self) -> usize {
        self.chars
    }

    pub fn line_count(&self) -> usize {
        self.starts.len()
    }

    /// The character index line `n` (from 1) starts at; the end of the text past the last line.
    pub fn line_start(&self, n: usize) -> usize {
        match n.checked_sub(1).and_then(|i| self.starts.get(i)) {
            Some(&c) => c,
            None => self.chars,
        }
    }

    /// The line and column (from 1) of character index `c`.
    pub fn line_col(&self, c: usize) -> (usize, usize) {
        let line = self.starts.partition_point(|&s| s <= c).max(1);
        (line, c - self.starts[line - 1] + 1)
    }
}

/// Where each line starts, and how many characters there are.
fn line_starts(s: &str) -> (Vec<usize>, usize) {
    let mut v = vec![0];
    let mut n = 0;
    for ch in s.chars() {
        n += 1;
        if ch == '\n' {
            v.push(n);
        }
    }
    (v, n)
}

/// The text box types and deletes through these, so its edits keep the line starts up to date
/// without counting the whole text again.
impl TextBuffer for Text {
    fn is_mutable(&self) -> bool {
        true
    }

    fn as_str(&self) -> &str {
        &self.s
    }

    fn insert_text(&mut self, text: &str, char_index: CharIndex) -> usize {
        let at = char_index.0.min(self.chars);
        let n = self.s.insert_text(text, CharIndex(at));
        let line = self.starts.partition_point(|&s| s <= at);
        let mut tail = self.starts.split_off(line);
        tail.iter_mut().for_each(|s| *s += n);
        self.starts.extend(text.chars().enumerate().filter(|(_, ch)| *ch == '\n').map(|(i, _)| at + i + 1));
        self.starts.append(&mut tail);
        self.chars += n;
        self.bump();
        n
    }

    fn delete_char_range(&mut self, char_range: Range<CharIndex>) {
        let (a, b) = (char_range.start.0.min(self.chars), char_range.end.0.min(self.chars));
        self.s.delete_char_range(char_range);
        let gone = b - a;
        self.starts.retain(|&s| s <= a || s > b);
        self.starts.iter_mut().filter(|s| **s > b).for_each(|s| *s -= gone);
        self.chars -= gone;
        self.bump();
    }

    fn clear(&mut self) {
        self.set(String::new());
    }

    fn replace_with(&mut self, text: &str) {
        self.set(text.to_string());
    }

    fn take(&mut self) -> String {
        let s = std::mem::take(&mut self.s);
        self.set(String::new());
        s
    }

    fn type_id(&self) -> std::any::TypeId {
        std::any::TypeId::of::<Self>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typing_and_deleting_keep_the_line_starts() {
        let mut t = Text::new("ab\ncd\né\n".into());
        let check = |t: &Text| assert_eq!((t.starts.clone(), t.chars), line_starts(&t.s), "{:?}", t.s);
        let g = t.generation();
        t.insert_text("x\ny", CharIndex(1));
        assert_ne!(t.generation(), g);
        check(&t);
        t.insert_text("\n\n", CharIndex(0));
        check(&t);
        t.insert_text("z", CharIndex(t.char_count()));
        check(&t);
        t.delete_char_range(CharIndex(0)..CharIndex(3));
        check(&t);
        t.delete_char_range(CharIndex(2)..CharIndex(6));
        check(&t);
        let len = t.char_count();
        t.delete_char_range(CharIndex(0)..CharIndex(len));
        check(&t);
        assert_eq!(t.line_count(), 1);
    }

    #[test]
    fn line_and_column() {
        let t = Text::new("ab\nçd\n".into());
        assert_eq!(t.line_col(0), (1, 1));
        assert_eq!(t.line_col(2), (1, 3));
        assert_eq!(t.line_col(3), (2, 1));
        assert_eq!(t.line_col(5), (2, 3));
        assert_eq!(t.line_col(6), (3, 1));
        assert_eq!(t.line_start(2), 3);
        assert_eq!(t.line_start(9), 6);
        assert_eq!(t.line_count(), 3);
    }

    #[test]
    fn every_change_has_a_new_number() {
        let mut t = Text::new("a".into());
        let mut seen = vec![t.generation()];
        t.set("b".into());
        seen.push(t.generation());
        t.edit(|s| s.push('c'));
        seen.push(t.generation());
        t.replace_range(0..1, "d");
        seen.push(t.generation());
        t.replace_with("e");
        seen.push(t.generation());
        seen.dedup();
        assert_eq!(seen.len(), 5);
        assert_ne!(Text::new("a".into()).generation(), Text::new("a".into()).generation());
    }
}
