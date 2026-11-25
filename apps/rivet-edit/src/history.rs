//! Undo and redo for a tab.
//!
//! The text is changed in many places (the text box, multiple cursors, line commands, Vim,
//! completions, replace), so rather than each of them reporting what it did, the history looks
//! at the text once or twice a frame and records what changed since it last looked: where, what
//! went and what came, and the cursors before and after.
//!
//! Typing or deleting one character after another, without moving the cursor in between, is one
//! step (a new word starts a new step, and so does switching from typing to deleting). Commands
//! (moving lines, commenting, replacing…) are a step each.

use crate::ops::Sel;

/// One change: at byte `at`, `removed` gave way to `inserted`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edit {
    at: usize,
    removed: String,
    inserted: String,
}

/// What one undo takes back.
#[derive(Debug)]
struct Step {
    /// In the order they happened.
    edits: Vec<Edit>,
    before: Vec<Sel>,
    after: Vec<Sel>,
    /// Typing (true) or deleting (false).
    grows: bool,
    /// Whether more changes may join it (not after a command, an undo or a redo).
    open: bool,
}

impl Step {
    fn bytes(&self) -> usize {
        self.edits.iter().map(|e| e.removed.len() + e.inserted.len()).sum()
    }
}

/// How many steps, and how much changed text, a tab keeps (the oldest go first).
const MAX_STEPS: usize = 1000;
const MAX_BYTES: usize = 64 << 20;

#[derive(Default)]
pub struct History {
    /// The text as last seen (None until the first look).
    seen: Option<String>,
    /// The cursors as last seen: the ones before the next change.
    sels: Vec<Sel>,
    undo: Vec<Step>,
    redo: Vec<Step>,
    /// The next change is a command: a step of its own.
    command: bool,
}

impl History {
    /// Record what changed since the last look, and remember the cursors (`sels`, character
    /// positions, the main one last).
    pub fn observe(&mut self, text: &str, sels: &[Sel]) {
        let Some(seen) = self.seen.as_mut() else {
            self.seen = Some(text.to_string());
            self.sels = sels.to_vec();
            return;
        };
        if seen.as_str() == text {
            self.sels = sels.to_vec();
            return;
        }
        // Where the change starts is at or before the first cursor: say so to the diff, so typing
        // "b" in "ab|b" is put after the "a" where it was typed, not after the "b".
        let hint = self.sels.iter().map(Sel::lo).min().map(|c| seen.char_indices().nth(c).map_or(seen.len(), |(i, _)| i)).unwrap_or(usize::MAX);
        let edit = diff(seen, text, hint);
        seen.replace_range(edit.at..edit.at + edit.removed.len(), &edit.inserted);
        debug_assert_eq!(seen.as_str(), text);
        let before = std::mem::replace(&mut self.sels, sels.to_vec());
        self.record(edit, before, sels.to_vec());
    }

    /// The cursors moved, but the text is as last seen.
    pub fn look(&mut self, sels: &[Sel]) {
        self.sels = sels.to_vec();
    }

    /// The next change is a command, an undo step of its own.
    pub fn command(&mut self) {
        self.command = true;
    }

    /// Take the last step back; the cursors to put back. `text` must be as last seen.
    pub fn undo(&mut self, text: &mut String) -> Option<Vec<Sel>> {
        if self.seen.as_deref() != Some(text.as_str()) {
            return None;
        }
        let mut step = self.undo.pop()?;
        for e in step.edits.iter().rev() {
            text.replace_range(e.at..e.at + e.inserted.len(), &e.removed);
        }
        self.after_jump(text, &step.before);
        step.open = false;
        let sels = step.before.clone();
        self.redo.push(step);
        Some(sels)
    }

    /// Do the last step taken back again; the cursors to put back.
    pub fn redo(&mut self, text: &mut String) -> Option<Vec<Sel>> {
        if self.seen.as_deref() != Some(text.as_str()) {
            return None;
        }
        let mut step = self.redo.pop()?;
        for e in &step.edits {
            text.replace_range(e.at..e.at + e.removed.len(), &e.inserted);
        }
        self.after_jump(text, &step.after);
        step.open = false;
        let sels = step.after.clone();
        self.undo.push(step);
        Some(sels)
    }

    fn after_jump(&mut self, text: &str, sels: &[Sel]) {
        self.seen = Some(text.to_string());
        self.sels = sels.to_vec();
        if let Some(top) = self.undo.last_mut() {
            top.open = false;
        }
    }

    fn record(&mut self, edit: Edit, before: Vec<Sel>, after: Vec<Sel>) {
        self.redo.clear();
        let command = std::mem::take(&mut self.command);
        let grows = edit.inserted.len() > edit.removed.len();
        let cursors = before.len().max(1);
        // A keystroke: a character or so typed or deleted at each cursor, and with one cursor
        // (where the diff is exactly what was typed) not a line break.
        let delta = edit.inserted.chars().count().abs_diff(edit.removed.chars().count());
        let keystroke = !command && delta >= 1 && delta <= cursors && (cursors > 1 || (!edit.inserted.contains('\n') && (edit.removed.is_empty() || edit.inserted.is_empty())));
        if keystroke
            && let Some(last) = self.undo.last_mut()
            && last.open
            && last.grows == grows
            && last.after == before
            && !(cursors == 1 && grows && starts_word(last, &edit))
        {
            join(&mut last.edits, edit);
            last.after = after;
            return;
        }
        self.undo.push(Step { edits: vec![edit], before, after, grows, open: keystroke });
        // Keep it bounded.
        let mut bytes: usize = self.undo.iter().map(Step::bytes).sum();
        let mut drop = 0;
        while self.undo.len() - drop > 1 && (self.undo.len() - drop > MAX_STEPS || bytes > MAX_BYTES) {
            bytes -= self.undo[drop].bytes();
            drop += 1;
        }
        self.undo.drain(..drop);
    }

    /// How many steps there are to undo and to redo.
    pub fn steps(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }
}

/// Typing a word's first character after a space.
fn starts_word(last: &Step, e: &Edit) -> bool {
    let prev = last.edits.last().and_then(|p| p.inserted.chars().last());
    let next = e.inserted.chars().next();
    matches!((prev, next), (Some(p), Some(n)) if p.is_whitespace() && !n.is_whitespace())
}

/// Add `e` to a step's edits, into the last one when it carries straight on from it.
fn join(edits: &mut Vec<Edit>, e: Edit) {
    if let Some(p) = edits.last_mut() {
        // Typing on.
        if e.removed.is_empty() && e.at == p.at + p.inserted.len() {
            p.inserted.push_str(&e.inserted);
            return;
        }
        if e.inserted.is_empty() {
            // Backspace over what was just typed.
            if e.at + e.removed.len() == p.at + p.inserted.len() && e.at >= p.at && p.inserted.ends_with(&e.removed) {
                p.inserted.truncate(e.at - p.at);
                return;
            }
            // More backspaces, or Delete.
            if p.inserted.is_empty() && e.at + e.removed.len() == p.at {
                p.removed.insert_str(0, &e.removed);
                p.at = e.at;
                return;
            }
            if p.inserted.is_empty() && e.at == p.at {
                p.removed.push_str(&e.removed);
                return;
            }
        }
    }
    edits.push(e);
}

/// The change from `old` to `new`: what's the same at the start (no further than `hint`) and at
/// the end is left out.
fn diff(old: &str, new: &str, hint: usize) -> Edit {
    let (a, b) = (old.as_bytes(), new.as_bytes());
    let mut pre = a.iter().zip(b).take_while(|(x, y)| x == y).count().min(hint);
    while !old.is_char_boundary(pre) || !new.is_char_boundary(pre) {
        pre -= 1;
    }
    let room = a.len().min(b.len()) - pre;
    let mut suf = a.iter().rev().zip(b.iter().rev()).take(room).take_while(|(x, y)| x == y).count();
    while !old.is_char_boundary(a.len() - suf) || !new.is_char_boundary(b.len() - suf) {
        suf -= 1;
    }
    Edit { at: pre, removed: old[pre..a.len() - suf].to_string(), inserted: new[pre..b.len() - suf].to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caret(p: usize) -> Vec<Sel> {
        vec![Sel::caret(p)]
    }

    /// Type `s` at `at` one character at a time, looking after each.
    fn type_in(h: &mut History, text: &mut String, at: usize, s: &str) {
        let mut p = at;
        for c in s.chars() {
            let b = text.char_indices().nth(p).map_or(text.len(), |(i, _)| i);
            text.insert(b, c);
            p += 1;
            h.observe(text, &caret(p));
        }
    }

    #[test]
    fn typing_undoes_a_word_at_a_time() {
        let mut h = History::default();
        let mut t = String::from("fn x");
        h.observe(&t, &caret(4));
        type_in(&mut h, &mut t, 4, " hello world");
        assert_eq!(t, "fn x hello world");
        // " ", "hello ", "world".
        assert_eq!(h.steps(), (3, 0));
        assert_eq!(h.undo(&mut t), Some(caret(11)));
        assert_eq!(t, "fn x hello ");
        assert_eq!(h.undo(&mut t), Some(caret(5)));
        assert_eq!(t, "fn x ");
        assert_eq!(h.undo(&mut t), Some(caret(4)));
        assert_eq!(t, "fn x");
        assert_eq!(h.undo(&mut t), None);
        assert_eq!(h.redo(&mut t), Some(caret(5)));
        assert_eq!(h.redo(&mut t), Some(caret(11)));
        assert_eq!(h.redo(&mut t), Some(caret(16)));
        assert_eq!(t, "fn x hello world");
    }

    #[test]
    fn moving_the_cursor_or_deleting_starts_a_new_step() {
        let mut h = History::default();
        let mut t = String::from("abc");
        h.observe(&t, &caret(3));
        type_in(&mut h, &mut t, 3, "de");
        // Backspace twice: one step, apart from the typing.
        t.pop();
        h.observe(&t, &caret(4));
        t.pop();
        h.observe(&t, &caret(3));
        // Somewhere else.
        h.observe(&t, &caret(0));
        type_in(&mut h, &mut t, 0, "xy");
        assert_eq!(t, "xyabc");
        assert_eq!(h.steps(), (3, 0));
        h.undo(&mut t);
        assert_eq!(t, "abc");
        h.undo(&mut t);
        assert_eq!(t, "abcde");
        h.undo(&mut t);
        assert_eq!(t, "abc");
    }

    #[test]
    fn a_command_is_a_step_of_its_own_and_brings_every_cursor_back() {
        let mut h = History::default();
        let mut t = String::from("one\ntwo\n");
        let two = vec![Sel::caret(0), Sel::caret(4)];
        h.observe(&t, &two);
        h.command();
        t = "// one\n// two\n".into();
        h.observe(&t, &[Sel::caret(3), Sel::caret(10)]);
        type_in(&mut h, &mut t, 3, "x");
        assert_eq!(h.steps(), (2, 0));
        h.undo(&mut t);
        assert_eq!(h.undo(&mut t), Some(two));
        assert_eq!(t, "one\ntwo\n");
    }

    #[test]
    fn typing_after_an_undo_clears_redo_and_does_not_join_the_old_step() {
        let mut h = History::default();
        let mut t = String::new();
        h.observe(&t, &caret(0));
        type_in(&mut h, &mut t, 0, "ab");
        type_in(&mut h, &mut t, 2, " cd");
        h.undo(&mut t);
        assert_eq!(t, "ab ");
        type_in(&mut h, &mut t, 3, "e");
        assert_eq!(h.steps(), (2, 0));
        h.undo(&mut t);
        assert_eq!(t, "ab ");
    }

    #[test]
    fn the_diff_puts_a_repeated_character_where_it_was_typed() {
        // "ab|b" + "b": the cursor says it went in after the "a"… and the text says nothing
        // more, so the hint decides.
        let e = diff("abb", "abbb", 2);
        assert_eq!(e, Edit { at: 2, removed: String::new(), inserted: "b".into() });
        let e = diff("héllo", "hllo", 1);
        assert_eq!(e, Edit { at: 1, removed: "é".into(), inserted: String::new() });
        let e = diff("aé", "aè", usize::MAX);
        assert_eq!((e.removed.as_str(), e.inserted.as_str()), ("é", "è"));
    }

    #[test]
    fn several_cursors_typing_is_one_step() {
        let mut h = History::default();
        let mut t = String::from("a\nb\n");
        h.observe(&t, &[Sel::caret(1), Sel::caret(3)]);
        t = "ax\nbx\n".into();
        h.observe(&t, &[Sel::caret(2), Sel::caret(5)]);
        t = "axy\nbxy\n".into();
        h.observe(&t, &[Sel::caret(3), Sel::caret(7)]);
        assert_eq!(h.steps(), (1, 0));
        assert_eq!(h.undo(&mut t), Some(vec![Sel::caret(1), Sel::caret(3)]));
        assert_eq!(t, "a\nb\n");
    }

    #[test]
    fn a_change_it_has_not_seen_blocks_undo() {
        let mut h = History::default();
        let mut t = String::from("a");
        h.observe(&t, &caret(1));
        type_in(&mut h, &mut t, 1, "b");
        t.push('c');
        assert_eq!(h.undo(&mut t), None);
        assert_eq!(t, "abc");
    }
}
