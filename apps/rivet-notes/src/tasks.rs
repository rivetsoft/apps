//! Super todos: Markdown tasks (`- [ ] …`) tagged with one of the todo tags (`#task` by
//! default). Their details stay in the line as plain text, so any app can read them:
//!
//! `- [ ] Send the invoice #task #work due:2026-12-01 priority:high`

use chrono::NaiveDate;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    #[default]
    None,
    Low,
    Medium,
    High,
}

impl Priority {
    pub const ALL: [Priority; 4] = [Priority::None, Priority::Low, Priority::Medium, Priority::High];

    pub fn label(self) -> &'static str {
        match self {
            Priority::None => "None",
            Priority::Low => "Low",
            Priority::Medium => "Medium",
            Priority::High => "High",
        }
    }

    fn word(self) -> Option<&'static str> {
        match self {
            Priority::None => None,
            Priority::Low => Some("low"),
            Priority::Medium => Some("medium"),
            Priority::High => Some("high"),
        }
    }

    fn parse(w: &str) -> Option<Priority> {
        match w.to_ascii_lowercase().as_str() {
            "high" | "h" | "1" => Some(Priority::High),
            "medium" | "med" | "m" | "2" => Some(Priority::Medium),
            "low" | "l" | "3" => Some(Priority::Low),
            _ => None,
        }
    }
}

/// One Markdown task line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    /// Its line in the note's text (counting from 0).
    pub line: usize,
    /// The line as written (to make sure it's still the same before changing it).
    pub raw: String,
    pub done: bool,
    /// Started but not finished (`- [/]`).
    pub doing: bool,
    /// The words, without tags and details.
    pub title: String,
    /// Its #tags (without `#`), the todo tags included.
    pub tags: Vec<String>,
    pub due: Option<NaiveDate>,
    pub priority: Priority,
}

impl Item {
    /// The character in its box: ` `, `/` or `x`.
    pub fn mark(&self) -> char {
        if self.done {
            'x'
        } else if self.doing {
            '/'
        } else {
            ' '
        }
    }

    /// Whether it's a super todo: it carries one of `triggers`.
    pub fn is_super(&self, triggers: &[String]) -> bool {
        self.tags.iter().any(|t| triggers.iter().any(|g| g.eq_ignore_ascii_case(t)))
    }
}

/// Where the task's checkbox starts and how long the `- [ ] ` mark is, or None.
fn task_mark(line: &str) -> Option<(usize, usize, bool)> {
    let indent = line.len() - line.trim_start().len();
    let t = &line[indent..];
    let b = t.as_bytes();
    if b.len() >= 5 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ' && b[2] == b'[' && matches!(b[3], b' ' | b'x' | b'X' | b'/') && b[4] == b']' && (b.len() == 5 || b[5] == b' ') {
        Some((indent, if b.len() > 5 { 6 } else { 5 }, b[3] != b' '))
    } else {
        None
    }
}

/// Read one line as a task.
pub fn parse_line(line_no: usize, line: &str) -> Option<Item> {
    let line = line.trim_end_matches(['\n', '\r']);
    let (indent, mark, _) = task_mark(line)?;
    let state = line.as_bytes()[indent + 3];
    let done = matches!(state, b'x' | b'X');
    let doing = state == b'/';
    let rest = &line[indent + mark..];
    let mut words = Vec::new();
    let mut tags = Vec::new();
    let mut due = None;
    let mut priority = Priority::None;
    for w in rest.split_whitespace() {
        if let Some(t) = w.strip_prefix('#')
            && !t.is_empty()
            && t.chars().all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/'))
            && !t.chars().all(|c| c.is_ascii_digit())
        {
            if !tags.iter().any(|x: &String| x.eq_ignore_ascii_case(t)) {
                tags.push(t.to_string());
            }
        } else if let Some(d) = w.strip_prefix("due:").and_then(|d| NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()) {
            due = Some(d);
        } else if let Some(p) = w.strip_prefix("priority:").and_then(Priority::parse) {
            priority = p;
        } else {
            words.push(w);
        }
    }
    Some(Item { line: line_no, raw: line.to_string(), done, doing, title: words.join(" "), tags, due, priority })
}

/// Every task in a note's text (outside code blocks).
pub fn scan(body: &str) -> Vec<Item> {
    let mut out = Vec::new();
    let mut fence = false;
    for (i, l) in body.lines().enumerate() {
        let t = l.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if !fence && let Some(it) = parse_line(i, l) {
            out.push(it);
        }
    }
    out
}

/// The line written back with these details: the words, then the tags, the due date and the
/// priority. The indent and the check mark are kept.
pub fn write_line(line: &str, mark: char, tags: &[String], due: Option<NaiveDate>, priority: Priority) -> String {
    let Some(it) = parse_line(0, line) else { return line.to_string() };
    let (indent, _, _) = task_mark(line).unwrap_or((0, 6, false));
    let bullet = &line[indent..indent + 1];
    let mut s = format!("{}{} [{}] {}", &line[..indent], bullet, mark, it.title);
    for t in tags {
        s.push_str(&format!(" #{t}"));
    }
    if let Some(d) = due {
        s.push_str(&format!(" due:{}", d.format("%Y-%m-%d")));
    }
    if let Some(w) = priority.word() {
        s.push_str(&format!(" priority:{w}"));
    }
    s
}

/// Back to a plain todo: no todo tags, due date or priority (its words, other tags and box stay).
pub fn downgrade(it: &mut Item, triggers: &[String]) {
    it.tags.retain(|t| !triggers.iter().any(|g| g.eq_ignore_ascii_case(t)));
    it.due = None;
    it.priority = Priority::None;
}

/// `body` with line `n` replaced (keeping its line ending).
pub fn replace_line(body: &str, n: usize, new: &str) -> Option<String> {
    let mut out = String::with_capacity(body.len() + new.len());
    let mut found = false;
    for (i, l) in body.split_inclusive('\n').enumerate() {
        if i == n {
            let end = &l[l.trim_end_matches(['\n', '\r']).len()..];
            out.push_str(new);
            out.push_str(end);
            found = true;
        } else {
            out.push_str(l);
        }
    }
    found.then_some(out)
}

/// Line `n` of `body`, without its line ending.
pub fn line_of(body: &str, n: usize) -> Option<&str> {
    body.lines().nth(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_details_and_writes_them_back() {
        let it = parse_line(3, "  - [ ] Send the invoice #task #work due:2026-12-01 priority:high").unwrap();
        assert_eq!(it.title, "Send the invoice");
        assert_eq!(it.tags, ["task", "work"]);
        assert_eq!(it.due, NaiveDate::from_ymd_opt(2026, 12, 1));
        assert_eq!(it.priority, Priority::High);
        assert!(!it.done && it.is_super(&["Task".into()]) && !it.is_super(&["todo".into()]));
        let d = NaiveDate::from_ymd_opt(2027, 1, 2);
        assert_eq!(write_line(&it.raw, 'x', &["task".into()], d, Priority::Low), "  - [x] Send the invoice #task due:2027-01-02 priority:low");
        assert_eq!(parse_line(0, "- [x] done"), Some(Item { line: 0, raw: "- [x] done".into(), done: true, doing: false, title: "done".into(), tags: vec![], due: None, priority: Priority::None }));
        assert!(parse_line(0, "- not a task").is_none());
        let it = parse_line(0, "- [/] half way #task").unwrap();
        assert!(it.doing && !it.done && it.mark() == '/');
    }

    #[test]
    fn downgrades_to_a_plain_todo() {
        let mut it = parse_line(0, "- [/] Ship it #task #work due:2026-12-01 priority:high").unwrap();
        downgrade(&mut it, &["task".into()]);
        assert_eq!(write_line(&it.raw, it.mark(), &it.tags, it.due, it.priority), "- [/] Ship it #work");
    }

    #[test]
    fn scans_and_replaces_lines() {
        let body = "# T\n- [ ] a #task\n```\n- [ ] in code #task\n```\n- [x] b\n";
        let all = scan(body);
        assert_eq!(all.iter().map(|i| i.line).collect::<Vec<_>>(), [1, 5]);
        assert_eq!(replace_line(body, 1, "- [x] a #task").unwrap().lines().nth(1), Some("- [x] a #task"));
        assert_eq!(line_of(body, 5), Some("- [x] b"));
    }
}
