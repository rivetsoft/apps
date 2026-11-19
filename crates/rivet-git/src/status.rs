//! `git status --porcelain=v2 --branch -z`, read into types.

/// How a file changed, on one side (staged, or on disk).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Change {
    Unchanged,
    Modified,
    /// Its kind changed (a file became a link, say).
    TypeChanged,
    Added,
    Deleted,
    Renamed,
    Copied,
    /// Not tracked yet.
    Untracked,
    /// Both sides changed it in a merge, and it isn't settled.
    Conflicted,
}

impl Change {
    fn from_code(c: u8) -> Change {
        match c {
            b'M' => Change::Modified,
            b'T' => Change::TypeChanged,
            b'A' => Change::Added,
            b'D' => Change::Deleted,
            b'R' => Change::Renamed,
            b'C' => Change::Copied,
            b'U' => Change::Conflicted,
            _ => Change::Unchanged,
        }
    }

    /// The letter shown beside the file, as VS Code and Zed show it.
    pub fn letter(self) -> &'static str {
        match self {
            Change::Unchanged => "",
            Change::Modified | Change::TypeChanged => "M",
            Change::Added => "A",
            Change::Deleted => "D",
            Change::Renamed => "R",
            Change::Copied => "C",
            Change::Untracked => "U",
            Change::Conflicted => "!",
        }
    }

    /// In words, for a tooltip.
    pub fn words(self) -> &'static str {
        match self {
            Change::Unchanged => "Unchanged",
            Change::Modified => "Modified",
            Change::TypeChanged => "Type changed",
            Change::Added => "Added",
            Change::Deleted => "Deleted",
            Change::Renamed => "Renamed",
            Change::Copied => "Copied",
            Change::Untracked => "New, not tracked yet",
            Change::Conflicted => "Conflict",
        }
    }
}

/// A file git has something to say about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Inside the repository, with `/`.
    pub path: String,
    /// The old path of a rename or copy.
    pub from: Option<String>,
    /// What's staged.
    pub index: Change,
    /// What's changed on disk since then.
    pub worktree: Change,
}

impl Entry {
    pub fn untracked(&self) -> bool {
        self.worktree == Change::Untracked
    }

    pub fn conflicted(&self) -> bool {
        self.index == Change::Conflicted
    }

    /// Something is staged.
    pub fn staged(&self) -> bool {
        !self.conflicted() && !self.untracked() && self.index != Change::Unchanged
    }

    /// Something isn't staged yet (a change on disk, or a new file).
    pub fn unstaged(&self) -> bool {
        !self.conflicted() && self.worktree != Change::Unchanged
    }

    /// The one change to show for the file as a whole: a conflict, else what's on disk, else
    /// what's staged.
    pub fn overall(&self) -> Change {
        if self.conflicted() {
            Change::Conflicted
        } else if self.worktree != Change::Unchanged {
            // A file added and then changed again is still new.
            if self.index == Change::Added && self.worktree == Change::Modified { Change::Added } else { self.worktree }
        } else {
            self.index
        }
    }
}

/// The branch checked out.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Branch {
    /// Its name; None when HEAD is detached.
    pub head: Option<String>,
    /// The commit; None before the first commit.
    pub oid: Option<String>,
    /// The branch it follows ("origin/main").
    pub upstream: Option<String>,
    /// Commits here and not upstream, and the other way round.
    pub ahead: usize,
    pub behind: usize,
}

impl Branch {
    /// The name to show: the branch, or the commit when detached.
    pub fn label(&self) -> String {
        match (&self.head, &self.oid) {
            (Some(h), _) => h.clone(),
            (None, Some(o)) => format!("{} (detached)", &o[..o.len().min(8)]),
            (None, None) => "No commits".into(),
        }
    }
}

/// A repository's status.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Status {
    pub branch: Branch,
    /// Sorted by path.
    pub entries: Vec<Entry>,
}

impl Status {
    pub fn staged(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.staged())
    }

    pub fn unstaged(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.unstaged())
    }

    pub fn conflicted(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter().filter(|e| e.conflicted())
    }

    pub fn get(&self, path: &str) -> Option<&Entry> {
        self.entries.binary_search_by(|e| e.path.as_str().cmp(path)).ok().map(|i| &self.entries[i])
    }
}

/// Read `git status --porcelain=v2 --branch -z`.
pub fn parse(out: &[u8]) -> Status {
    let mut s = Status::default();
    let mut recs = out.split(|b| *b == 0).map(|r| String::from_utf8_lossy(r).into_owned());
    while let Some(r) = recs.next() {
        if let Some(h) = r.strip_prefix("# ") {
            let (k, v) = h.split_once(' ').unwrap_or((h, ""));
            match k {
                "branch.oid" => s.branch.oid = (v != "(initial)").then(|| v.to_string()),
                "branch.head" => s.branch.head = (v != "(detached)").then(|| v.to_string()),
                "branch.upstream" => s.branch.upstream = Some(v.to_string()),
                "branch.ab" => {
                    for n in v.split(' ') {
                        if let Some(a) = n.strip_prefix('+') {
                            s.branch.ahead = a.parse().unwrap_or(0);
                        } else if let Some(b) = n.strip_prefix('-') {
                            s.branch.behind = b.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = r.as_bytes().first().copied();
        match kind {
            // 1 XY sub mH mI mW hH hI path
            Some(b'1') => {
                if let Some((xy, path)) = fields(&r, 8) {
                    s.entries.push(entry(xy, path, None));
                }
            }
            // 2 XY sub mH mI mW hH hI Xscore path, then the old path as its own record.
            Some(b'2') => {
                if let Some((xy, path)) = fields(&r, 9) {
                    let from = recs.next();
                    s.entries.push(entry(xy, path, from));
                }
            }
            // u XY sub m1 m2 m3 mW h1 h2 h3 path
            Some(b'u') => {
                if let Some((_, path)) = fields(&r, 10) {
                    s.entries.push(Entry { path, from: None, index: Change::Conflicted, worktree: Change::Conflicted });
                }
            }
            Some(b'?') => s.entries.push(Entry { path: r[2..].to_string(), from: None, index: Change::Unchanged, worktree: Change::Untracked }),
            _ => {}
        }
    }
    s.entries.sort_by(|a, b| a.path.cmp(&b.path));
    s
}

/// The XY code and the path, after `skip` space-separated fields (the path may hold spaces).
fn fields(r: &str, skip: usize) -> Option<(&str, String)> {
    let mut it = r.splitn(skip + 1, ' ');
    let _kind = it.next()?;
    let xy = it.next()?;
    let path = it.nth(skip - 2)?;
    Some((xy, path.to_string()))
}

fn entry(xy: &str, path: String, from: Option<String>) -> Entry {
    let b = xy.as_bytes();
    Entry { path, from, index: Change::from_code(b[0]), worktree: Change::from_code(*b.get(1).unwrap_or(&b'.')) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_porcelain_v2() {
        let out = b"# branch.oid 1234567890abcdef\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb src/main.rs\0\
1 A. N... 000000 100644 100644 000 ccc new file.txt\0\
2 R. N... 100644 100644 100644 ddd ddd R100 b.rs\0a.rs\0\
u UU N... 100644 100644 100644 100644 e f g both.rs\0\
? notes.md\0";
        let s = parse(out);
        assert_eq!(s.branch.head.as_deref(), Some("main"));
        assert_eq!(s.branch.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.branch.ahead, s.branch.behind), (2, 1));
        let paths: Vec<&str> = s.entries.iter().map(|e| e.path.as_str()).collect();
        assert_eq!(paths, ["b.rs", "both.rs", "new file.txt", "notes.md", "src/main.rs"]);
        let b = s.get("b.rs").unwrap();
        assert_eq!((b.index, b.from.as_deref()), (Change::Renamed, Some("a.rs")));
        assert!(s.get("both.rs").unwrap().conflicted());
        assert_eq!(s.get("new file.txt").unwrap().index, Change::Added);
        assert!(s.get("notes.md").unwrap().untracked());
        let m = s.get("src/main.rs").unwrap();
        assert!(m.unstaged() && !m.staged());
        assert_eq!(s.staged().count(), 2);
        assert_eq!(s.unstaged().count(), 2);
    }

    #[test]
    fn first_commit_and_detached() {
        let s = parse(b"# branch.oid (initial)\0# branch.head main\0");
        assert_eq!(s.branch.oid, None);
        assert_eq!(s.branch.label(), "main");
        let s = parse(b"# branch.oid 0123456789abcdef\0# branch.head (detached)\0");
        assert_eq!(s.branch.label(), "01234567 (detached)");
    }
}
