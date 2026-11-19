//! Which lines changed between two texts, as hunks: the gutter's marks, and what staging or
//! reverting one hunk replaces.
//!
//! Lines are compared whole, newline included. The common start and end are skipped first, so
//! an edit in one place of a big file costs little. What's left is compared with Myers'
//! algorithm (what git uses), giving up past [`MAX_EDITS`] differences and marking the rest as
//! one changed block.

use std::collections::HashMap;
use std::ops::Range;

/// Past this many differing lines, what's left is one hunk (the comparison would slow down).
pub const MAX_EDITS: usize = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HunkKind {
    /// Lines that are new.
    Added,
    /// Lines that replaced others.
    Modified,
    /// Lines that are gone; `new` is empty and points where they were.
    Deleted,
}

/// A run of changed lines: `old` lines in the base were replaced by `new` lines (line numbers
/// from 0, end not included).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Hunk {
    pub old: Range<usize>,
    pub new: Range<usize>,
}

impl Hunk {
    pub fn kind(&self) -> HunkKind {
        if self.old.is_empty() {
            HunkKind::Added
        } else if self.new.is_empty() {
            HunkKind::Deleted
        } else {
            HunkKind::Modified
        }
    }
}

/// The lines of a text, each with its newline.
pub fn lines(text: &str) -> Vec<&str> {
    text.split_inclusive('\n').collect()
}

/// The hunks that turn `old` into `new`, in order.
pub fn hunks(old: &str, new: &str) -> Vec<Hunk> {
    if old == new {
        return Vec::new();
    }
    let (a, b) = (lines(old), lines(new));
    // One number per distinct line, so lines compare as numbers.
    let mut ids: HashMap<&str, u32> = HashMap::new();
    let a: Vec<u32> = intern(&a, &mut ids);
    let b: Vec<u32> = intern(&b, &mut ids);
    hunks_of(&a, &b)
}

fn intern<'a>(ls: &[&'a str], ids: &mut HashMap<&'a str, u32>) -> Vec<u32> {
    ls.iter()
        .map(|l| {
            let n = ids.len() as u32;
            *ids.entry(l).or_insert(n)
        })
        .collect()
}

/// The hunks between two sequences of line numbers.
fn hunks_of(a: &[u32], b: &[u32]) -> Vec<Hunk> {
    let pre = a.iter().zip(b).take_while(|(x, y)| x == y).count();
    let suf = a[pre..].iter().rev().zip(b[pre..].iter().rev()).take_while(|(x, y)| x == y).count();
    let (a_mid, b_mid) = (&a[pre..a.len() - suf], &b[pre..b.len() - suf]);
    let mut out = Vec::new();
    match myers(a_mid, b_mid) {
        Some(same) => {
            // Between matching pairs, whatever's left over is a hunk.
            let (mut i, mut j) = (0, 0);
            for (x, y) in same.into_iter().chain(std::iter::once((a_mid.len(), b_mid.len()))) {
                if x > i || y > j {
                    out.push(Hunk { old: pre + i..pre + x, new: pre + j..pre + y });
                }
                (i, j) = (x + 1, y + 1);
            }
        }
        None => out.push(Hunk { old: pre..pre + a_mid.len(), new: pre..pre + b_mid.len() }),
    }
    out
}

/// The matching line pairs (index in `a`, index in `b`) of a shortest edit, in order, or None
/// past [`MAX_EDITS`].
fn myers(a: &[u32], b: &[u32]) -> Option<Vec<(usize, usize)>> {
    let (n, m) = (a.len() as i64, b.len() as i64);
    if n == 0 || m == 0 {
        return Some(Vec::new());
    }
    let max = (n + m).min(MAX_EDITS as i64);
    let off = max + 1;
    // The furthest x reached on each diagonal k (at `off + k`).
    let mut v = vec![0i32; (2 * off + 1) as usize];
    // `v` before each round, only the diagonals that round reads (k from -d-1 to d+1), to walk
    // back through.
    let mut trace: Vec<Vec<i32>> = Vec::new();
    let mut found = None;
    for d in 0..=max {
        trace.push(v[(off - d - 1) as usize..=(off + d + 1) as usize].to_vec());
        for k in (-d..=d).step_by(2) {
            let at = |k: i64| v[(off + k) as usize] as i64;
            let down = k == -d || (k != d && at(k - 1) < at(k + 1));
            let mut x = if down { at(k + 1) } else { at(k - 1) + 1 };
            let mut y = x - k;
            while x < n && y < m && a[x as usize] == b[y as usize] {
                x += 1;
                y += 1;
            }
            v[(off + k) as usize] = x as i32;
            if x >= n && y >= m {
                found = Some(d);
                break;
            }
        }
        if found.is_some() {
            break;
        }
    }
    let end = found?;
    let mut pairs = Vec::new();
    let (mut x, mut y) = (n, m);
    for d in (0..=end).rev() {
        let vs = &trace[d as usize];
        let at = |k: i64| vs[(k + d + 1) as usize] as i64;
        let k = x - y;
        let down = k == -d || (k != d && at(k - 1) < at(k + 1));
        let pk = if down { k + 1 } else { k - 1 };
        let (px, py) = (at(pk), at(pk) - pk);
        // The matches after this round's step.
        while x > px && y > py {
            x -= 1;
            y -= 1;
            pairs.push((x as usize, y as usize));
        }
        if d > 0 {
            (x, y) = (px, py);
        }
    }
    pairs.reverse();
    Some(pairs)
}

/// `base` with one hunk taken from `new`: what staging that hunk makes the staged copy.
pub fn apply(base: &str, new: &str, h: &Hunk) -> String {
    let (a, b) = (lines(base), lines(new));
    let mut s = String::with_capacity(base.len());
    a[..h.old.start.min(a.len())].iter().for_each(|l| s.push_str(l));
    b[h.new.start.min(b.len())..h.new.end.min(b.len())].iter().for_each(|l| s.push_str(l));
    a[h.old.end.min(a.len())..].iter().for_each(|l| s.push_str(l));
    s
}

/// `new` with one hunk put back as it is in `base`: reverting that hunk.
pub fn revert(base: &str, new: &str, h: &Hunk) -> String {
    let flipped = Hunk { old: h.new.clone(), new: h.old.clone() };
    apply(new, base, &flipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(old: &str, new: &str) -> Vec<(HunkKind, Range<usize>, Range<usize>)> {
        hunks(old, new).into_iter().map(|h| (h.kind(), h.old, h.new)).collect()
    }

    #[test]
    fn added_modified_deleted() {
        assert!(hunks("a\nb\n", "a\nb\n").is_empty());
        assert_eq!(kinds("a\nb\n", "a\nx\nb\n"), [(HunkKind::Added, 1..1, 1..2)]);
        assert_eq!(kinds("a\nb\nc\n", "a\nB\nc\n"), [(HunkKind::Modified, 1..2, 1..2)]);
        assert_eq!(kinds("a\nb\nc\n", "a\nc\n"), [(HunkKind::Deleted, 1..2, 1..1)]);
        assert_eq!(kinds("", "a\n"), [(HunkKind::Added, 0..0, 0..1)]);
        // The last line gaining a newline changes it.
        assert_eq!(kinds("a\nb", "a\nb\n"), [(HunkKind::Modified, 1..2, 1..2)]);
    }

    #[test]
    fn several_hunks_apart() {
        let old = "1\n2\n3\n4\n5\n6\n7\n8\n";
        let new = "1\nX\n3\n4\n5\n6\n8\nY\n";
        assert_eq!(kinds(old, new), [(HunkKind::Modified, 1..2, 1..2), (HunkKind::Deleted, 6..7, 6..6), (HunkKind::Added, 8..8, 7..8)]);
    }

    #[test]
    fn shortest_edit_in_the_middle() {
        // Prefix and suffix trimmed, then a real comparison in between.
        let old = "a\nb\nc\nd\ne\nf\n";
        let new = "a\nc\nX\nd\nf\n";
        let h = hunks(old, new);
        let mut rebuilt = old.to_string();
        for x in h.iter().rev() {
            rebuilt = apply(&rebuilt, new, x);
        }
        assert_eq!(rebuilt, new);
        assert_eq!(kinds(old, new), [(HunkKind::Deleted, 1..2, 1..1), (HunkKind::Added, 3..3, 2..3), (HunkKind::Deleted, 4..5, 4..4)]);
    }

    #[test]
    fn stage_and_revert_one_hunk() {
        let base = "a\nb\nc\nd\n";
        let new = "A\nb\nc\nd\ne\n";
        let h = hunks(base, new);
        assert_eq!(h.len(), 2);
        // Stage the second: the base gains "e" only.
        assert_eq!(apply(base, new, &h[1]), "a\nb\nc\nd\ne\n");
        // Revert the first: the text gets "a" back, keeps "e".
        assert_eq!(revert(base, new, &h[0]), "a\nb\nc\nd\ne\n");
    }

    #[test]
    fn random_texts_rebuild() {
        // Every hunk applied to the base gives the new text back.
        let mut seed = 7u64;
        let mut rnd = |n: u64| {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (seed >> 33) % n
        };
        for _ in 0..300 {
            let mk = |r: &mut dyn FnMut(u64) -> u64| (0..r(12)).map(|_| format!("{}\n", r(4))).collect::<String>();
            let old = mk(&mut rnd);
            let new = mk(&mut rnd);
            let h = hunks(&old, &new);
            let mut s = old.clone();
            for x in h.iter().rev() {
                s = apply(&s, &new, x);
            }
            assert_eq!(s, new, "{old:?} -> {new:?}: {h:?}");
        }
    }
}
