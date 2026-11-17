//! Moving lines around in a note: a line's "block" is what moves with it when it's dragged —
//! a list item (or task) with the items nested under it, a whole fenced code block, or else
//! just the line.

/// The first and last line (counting from 0) of the block starting at line `n`.
pub fn block_at(body: &str, n: usize) -> (usize, usize) {
    let lines: Vec<&str> = body.split('\n').collect();
    if n >= lines.len() {
        return (n, n);
    }
    let indent = |l: &str| l.len() - l.trim_start().len();
    let t = lines[n].trim_start();
    // A fenced code block: to its closing fence.
    if t.starts_with("```") || t.starts_with("~~~") {
        let fence = &t[..3];
        let end = (n + 1..lines.len()).find(|&k| lines[k].trim_start().starts_with(fence)).unwrap_or(lines.len() - 1);
        return (n, end);
    }
    // A list item: with the lines indented under it.
    let b = t.as_bytes();
    let bullet = b.len() >= 2 && matches!(b[0], b'-' | b'*' | b'+') && b[1] == b' ';
    let numbered = t.find(['.', ')']).is_some_and(|i| i > 0 && t[..i].chars().all(|c| c.is_ascii_digit()) && t[i + 1..].starts_with(' '));
    if bullet || numbered {
        let me = indent(lines[n]);
        let mut end = n;
        while end + 1 < lines.len() && !lines[end + 1].trim().is_empty() && indent(lines[end + 1]) > me {
            end += 1;
        }
        return (n, end);
    }
    (n, n)
}

/// `body` with lines `a..=b` moved to just before line `gap` (`gap` = the number of lines puts
/// them at the end). None when that changes nothing.
pub fn move_block(body: &str, a: usize, b: usize, gap: usize) -> Option<String> {
    let mut lines: Vec<&str> = body.split('\n').collect();
    if a > b || b >= lines.len() || gap > lines.len() || (gap >= a && gap <= b + 1) {
        return None;
    }
    let block: Vec<&str> = lines.drain(a..=b).collect();
    let mut g = if gap > b { gap - block.len() } else { gap };
    // Keep the text's final newline last.
    if g == lines.len() && lines.last() == Some(&"") {
        g -= 1;
    }
    for (k, l) in block.into_iter().enumerate() {
        lines.insert(g + k, l);
    }
    let out = lines.join("\n");
    (out != body).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_take_their_children() {
        let t = "# Head\n- a\n  - a1\n  - a2\n- b\n```\ncode\n```\ntext\n";
        assert_eq!(block_at(t, 0), (0, 0));
        assert_eq!(block_at(t, 1), (1, 3));
        assert_eq!(block_at(t, 4), (4, 4));
        assert_eq!(block_at(t, 5), (5, 7));
        assert_eq!(block_at("1. one\n   more\n2. two", 0), (0, 1));
    }

    #[test]
    fn moves_up_and_down() {
        let t = "a\nb\nc\n";
        assert_eq!(move_block(t, 2, 2, 0).as_deref(), Some("c\na\nb\n"));
        assert_eq!(move_block(t, 0, 0, 3).as_deref(), Some("b\nc\na\n"));
        assert_eq!(move_block(t, 0, 0, 4).as_deref(), Some("b\nc\na\n"));
        assert_eq!(move_block(t, 1, 1, 1), None);
        assert_eq!(move_block(t, 1, 1, 2), None);
        let t = "# H\n- x\n  - y\ntext";
        assert_eq!(move_block(t, 1, 2, 0).as_deref(), Some("- x\n  - y\n# H\ntext"));
    }
}
