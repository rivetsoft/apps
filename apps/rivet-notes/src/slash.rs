//! The slash menu: `/` at the start of a line (or after a space) offers blocks to insert —
//! headings, lists, a task, a quote, code, a divider, a link, today's date — filtered by what's
//! typed after it.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    H1,
    H2,
    H3,
    Bullet,
    Numbered,
    Task,
    SuperTodo,
    Quote,
    Code,
    Divider,
    Link,
    Date,
}

pub struct Item {
    pub kind: Kind,
    pub label: &'static str,
    /// Other words it's found by.
    pub words: &'static str,
    /// A short mark for its tile.
    pub mark: &'static str,
    pub hint: &'static str,
}

pub const ITEMS: [Item; 12] = [
    Item { kind: Kind::H1, label: "Heading 1", words: "h1 title big", mark: "H1", hint: "Big section heading" },
    Item { kind: Kind::H2, label: "Heading 2", words: "h2 subtitle", mark: "H2", hint: "Medium heading" },
    Item { kind: Kind::H3, label: "Heading 3", words: "h3", mark: "H3", hint: "Small heading" },
    Item { kind: Kind::Bullet, label: "Bulleted list", words: "ul bullet unordered list", mark: "•", hint: "A simple list" },
    Item { kind: Kind::Numbered, label: "Numbered list", words: "ol ordered number list", mark: "1.", hint: "A list with numbers" },
    Item { kind: Kind::Task, label: "Todo", words: "task checkbox check todo", mark: "[ ]", hint: "A box to tick" },
    Item { kind: Kind::SuperTodo, label: "Super todo", words: "task due priority todo", mark: "[!]", hint: "A todo with a due date and priority" },
    Item { kind: Kind::Quote, label: "Quote", words: "blockquote cite", mark: "\u{201c}", hint: "Set a passage apart" },
    Item { kind: Kind::Code, label: "Code block", words: "code fence snippet pre", mark: "</>", hint: "Code, colored for its language" },
    Item { kind: Kind::Divider, label: "Divider", words: "hr rule line separator", mark: "\u{2014}", hint: "A line across the page" },
    Item { kind: Kind::Link, label: "Link to note", words: "wiki link note [[", mark: "[[", hint: "Link another note" },
    Item { kind: Kind::Date, label: "Today's date", words: "date today now", mark: "D", hint: "Insert today's date" },
];

/// A `/` being typed at the cursor: where the `/` is, and what's typed after it.
pub fn typing(body: &str, cursor: usize) -> Option<(usize, String)> {
    let cursor = cursor.min(body.len());
    if !body.is_char_boundary(cursor) {
        return None;
    }
    let line_start = body[..cursor].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let before = &body[line_start..cursor];
    let slash = before.rfind('/')?;
    let query = &before[slash + 1..];
    // At the line's start or after a space; a short word or two after it.
    let ok_before = slash == 0 || before[..slash].ends_with([' ', '\t']);
    if !ok_before || query.len() > 20 || query.starts_with(' ') || query.contains("  ") || !query.chars().all(|c| c.is_alphanumeric() || c == ' ') {
        return None;
    }
    // Not inside a code block.
    let fences = body[..line_start].lines().filter(|l| l.trim_start().starts_with("```")).count();
    if fences % 2 == 1 {
        return None;
    }
    Some((line_start + slash, query.to_string()))
}

/// The items matching `query`, best first (Super todo only with that module on).
pub fn matches(query: &str, super_todos: bool) -> Vec<&'static Item> {
    let q = query.trim().to_lowercase();
    let mut scored: Vec<(i32, &Item)> = ITEMS
        .iter()
        .filter(|i| super_todos || i.kind != Kind::SuperTodo)
        .filter_map(|i| {
            if q.is_empty() {
                return Some((0, i));
            }
            let l = i.label.to_lowercase();
            if l.starts_with(&q) {
                Some((300, i))
            } else if l.split(' ').any(|w| w.starts_with(&q)) || i.words.split(' ').any(|w| w.starts_with(&q)) {
                Some((200, i))
            } else if l.contains(&q) {
                Some((100, i))
            } else {
                None
            }
        })
        .collect();
    scored.sort_by_key(|s| std::cmp::Reverse(s.0));
    scored.into_iter().map(|(_, i)| i).collect()
}

/// `body` with the `/query` at `slash..end` turned into `kind`, and where the cursor goes (a
/// character index).
pub fn apply(body: &str, slash: usize, end: usize, kind: Kind, todo_tag: &str, today: &str) -> (String, usize) {
    let line_start = body[..slash].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let at_start = body[line_start..slash].trim().is_empty();
    // Line kinds go on a line of their own.
    let nl = if at_start { "" } else { "\n" };
    let (insert, back): (String, usize) = match kind {
        Kind::H1 => (format!("{nl}# "), 0),
        Kind::H2 => (format!("{nl}## "), 0),
        Kind::H3 => (format!("{nl}### "), 0),
        Kind::Bullet => (format!("{nl}- "), 0),
        Kind::Numbered => (format!("{nl}1. "), 0),
        Kind::Task => (format!("{nl}- [ ] "), 0),
        Kind::SuperTodo => {
            let tail = format!(" #{todo_tag}");
            let n = tail.chars().count();
            (format!("{nl}- [ ] {tail}"), n)
        }
        Kind::Quote => (format!("{nl}> "), 0),
        Kind::Code => (format!("{nl}```\n\n```"), 4),
        Kind::Divider => (format!("{nl}---\n"), 0),
        Kind::Link => ("[[".to_string(), 0),
        Kind::Date => (today.to_string(), 0),
    };
    let mut out = String::with_capacity(body.len() + insert.len());
    out.push_str(&body[..slash]);
    out.push_str(&insert);
    out.push_str(&body[end..]);
    let cursor = body[..slash].chars().count() + insert.chars().count() - back;
    (out, cursor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_only_where_it_should() {
        assert_eq!(typing("/he", 3), Some((0, "he".into())));
        assert_eq!(typing("text /co", 8), Some((5, "co".into())));
        assert_eq!(typing("a/b", 3), None);
        assert_eq!(typing("http://x", 8), None);
        assert_eq!(typing("```\n/x", 6), None);
        assert_eq!(typing("/heading 2", 10), Some((0, "heading 2".into())));
    }

    #[test]
    fn matches_and_applies() {
        assert_eq!(matches("h2", false)[0].kind, Kind::H2);
        assert_eq!(matches("todo", true)[0].kind, Kind::Task);
        assert!(matches("", false).iter().all(|i| i.kind != Kind::SuperTodo));
        assert_eq!(apply("/h1", 0, 3, Kind::H1, "task", "x"), ("# ".into(), 2));
        assert_eq!(apply("a\nsome /co", 7, 10, Kind::Code, "task", "x"), ("a\nsome \n```\n\n```".into(), 12));
        let (t, c) = apply("/su", 0, 3, Kind::SuperTodo, "task", "x");
        assert_eq!(t, "- [ ]  #task");
        assert_eq!(c, 6);
    }
}
