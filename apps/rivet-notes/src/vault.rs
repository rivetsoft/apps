//! A vault: a folder of Markdown notes. Each note may start with a frontmatter block
//! (`---` … `---`, YAML) whose fields are kept in order; `tags` come from the frontmatter
//! (`tags: [a, b]` or a `- item` list) and from `#tags` in the text.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// One frontmatter field's value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `key: text` (quotes removed).
    Text(String),
    /// `key: [a, b]` or a `- item` list under the key.
    List(Vec<String>),
    /// Anything else (nested maps, block text): kept exactly as written, shown read-only.
    Raw(String),
}

impl Value {
    pub fn as_text(&self) -> String {
        match self {
            Value::Text(t) => t.clone(),
            Value::List(v) => v.join(", "),
            Value::Raw(r) => r.trim().to_string(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frontmatter {
    pub fields: Vec<(String, Value)>,
}

impl Frontmatter {
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.fields.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v)
    }

    pub fn set(&mut self, key: &str, v: Value) {
        match self.fields.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
            Some((_, old)) => *old = v,
            None => self.fields.push((key.to_string(), v)),
        }
    }

    /// The frontmatter's own tags (`tags` or `tag`), without a leading `#`.
    pub fn tags(&self) -> Vec<String> {
        let v = self.get("tags").or_else(|| self.get("tag"));
        let raw: Vec<String> = match v {
            Some(Value::List(l)) => l.clone(),
            Some(Value::Text(t)) => t.split([',', ' ']).map(str::to_string).collect(),
            _ => Vec::new(),
        };
        raw.into_iter().map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect()
    }

    /// Written back as YAML: text as is (quoted when it needs to be), lists in brackets,
    /// raw values exactly as they were.
    pub fn to_yaml(&self) -> String {
        let mut s = String::new();
        for (k, v) in &self.fields {
            match v {
                Value::Text(t) => s.push_str(&format!("{k}: {}\n", quote(t))),
                Value::List(l) => s.push_str(&format!("{k}: [{}]\n", l.iter().map(|x| quote(x)).collect::<Vec<_>>().join(", "))),
                Value::Raw(r) => {
                    s.push_str(&format!("{k}:"));
                    s.push_str(r);
                    if !r.ends_with('\n') {
                        s.push('\n');
                    }
                }
            }
        }
        s
    }
}

fn quote(t: &str) -> String {
    let needs = t.starts_with(['[', '{', '"', '\'', '#', '&', '*', '!', '|', '>', '%', '@', '`', '-', '?', ' '])
        || t.contains(": ")
        || t.contains(" #")
        || t.ends_with(':')
        || t.ends_with(' ');
    if needs { format!("\"{}\"", t.replace('\\', "\\\\").replace('"', "\\\"")) } else { t.to_string() }
}

fn unquote(v: &str) -> String {
    let v = v.trim();
    if v.len() >= 2 && ((v.starts_with('"') && v.ends_with('"')) || (v.starts_with('\'') && v.ends_with('\''))) {
        let inner = &v[1..v.len() - 1];
        if v.starts_with('"') { inner.replace("\\\"", "\"").replace("\\\\", "\\") } else { inner.replace("''", "'") }
    } else {
        v.to_string()
    }
}

/// Split a note into (frontmatter, body). Without a frontmatter block, the whole text is body.
pub fn split(text: &str) -> (Option<Frontmatter>, &str) {
    let t = text.strip_prefix('\u{feff}').unwrap_or(text);
    let Some(rest) = t.strip_prefix("---\n").or_else(|| t.strip_prefix("---\r\n")) else { return (None, text) };
    // The block ends at a line that is just `---` (or `...`).
    let mut pos = 0;
    for line in rest.split_inclusive('\n') {
        let l = line.trim_end_matches(['\n', '\r']);
        if l == "---" || l == "..." {
            let yaml = &rest[..pos];
            let body = &rest[pos + line.len()..];
            return (Some(parse_yaml(yaml)), body);
        }
        pos += line.len();
    }
    if rest.trim_end() == "---" {
        return (Some(Frontmatter::default()), "");
    }
    (None, text)
}

fn parse_yaml(yaml: &str) -> Frontmatter {
    let mut fm = Frontmatter::default();
    let lines: Vec<&str> = yaml.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        i += 1;
        if line.trim().is_empty() || line.trim_start().starts_with('#') || line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((k, v)) = line.split_once(':') else { continue };
        let key = k.trim().to_string();
        let v = v.trim();
        // The indented lines that belong to this key.
        let start = i;
        while i < lines.len() && (lines[i].starts_with([' ', '\t']) || lines[i].trim().is_empty()) {
            i += 1;
        }
        let child: Vec<&str> = lines[start..i].iter().copied().filter(|l| !l.trim().is_empty()).collect();
        let value = if v.starts_with('[') && v.ends_with(']') {
            Value::List(v[1..v.len() - 1].split(',').map(unquote).filter(|s| !s.is_empty()).collect())
        } else if v.is_empty() && !child.is_empty() && child.iter().all(|l| l.trim_start().starts_with("- ") || l.trim() == "-") {
            Value::List(child.iter().map(|l| unquote(l.trim_start().trim_start_matches('-'))).filter(|s| !s.is_empty()).collect())
        } else if v.is_empty() && child.is_empty() {
            Value::Text(String::new())
        } else if child.is_empty() && !v.starts_with(['|', '>', '{', '&', '*']) {
            Value::Text(unquote(v))
        } else {
            // Kept as written: the rest of the key's line, and its indented lines.
            let mut raw = if v.is_empty() { String::from("\n") } else { format!(" {v}\n") };
            for l in &lines[start..i] {
                raw.push_str(l);
                raw.push('\n');
            }
            Value::Raw(raw)
        };
        fm.fields.push((key, value));
    }
    fm
}

/// `#tags` in the text (not inside code, not headings, not `#123` numbers or URLs' anchors).
pub fn inline_tags(body: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fence = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("```") || t.starts_with("~~~") {
            fence = !fence;
            continue;
        }
        if fence || t.starts_with("    ") {
            continue;
        }
        let mut in_code = false;
        let chars: Vec<char> = line.chars().collect();
        let mut k = 0;
        while k < chars.len() {
            let c = chars[k];
            if c == '`' {
                in_code = !in_code;
            } else if c == '#' && !in_code && (k == 0 || chars[k - 1].is_whitespace() || chars[k - 1] == '(') {
                let tag: String = chars[k + 1..].iter().take_while(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '/')).collect();
                if !tag.is_empty() && !tag.chars().all(|c| c.is_ascii_digit()) && !(k == 0 && chars.get(1) == Some(&' ')) {
                    out.push(tag.trim_end_matches('/').to_string());
                }
                k += tag.len();
            }
            k += 1;
        }
    }
    out
}

#[derive(Clone, Debug)]
pub struct Note {
    pub path: PathBuf,
    /// Path inside the vault ("Projects/Rivet.md").
    pub rel: String,
    /// `title` from the frontmatter, else the first `# heading`, else the file name.
    pub title: String,
    /// Frontmatter and inline tags, without `#`, each once.
    pub tags: Vec<String>,
    pub modified: Option<SystemTime>,
    /// The first line of text (for the list).
    pub snippet: String,
}

impl Note {
    /// The folder it's in, inside the vault ("" for the top).
    pub fn folder(&self) -> &str {
        self.rel.rsplit_once('/').map(|(d, _)| d).unwrap_or("")
    }
}

pub fn read_note(root: &Path, path: &Path) -> Option<Note> {
    let text = std::fs::read_to_string(path).ok()?;
    let rel = path.strip_prefix(root).ok()?.to_string_lossy().replace('\\', "/");
    let (fm, body) = split(&text);
    let mut tags = fm.as_ref().map(|f| f.tags()).unwrap_or_default();
    for t in inline_tags(body) {
        if !tags.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
            tags.push(t);
        }
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let heading = body.lines().find_map(|l| l.strip_prefix("# ").map(|h| h.trim().to_string()));
    let title = fm.as_ref().and_then(|f| f.get("title")).map(|v| v.as_text()).filter(|t| !t.is_empty()).or(heading).unwrap_or(stem);
    let snippet = body.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("```")).unwrap_or("").chars().take(120).collect();
    Some(Note { path: path.to_path_buf(), rel, title, tags, modified: std::fs::metadata(path).and_then(|m| m.modified()).ok(), snippet })
}

/// Every note in a vault (`.md` and `.markdown`), skipping hidden folders such as `.git`,
/// sorted by path.
pub fn scan(root: &Path) -> Vec<Note> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let p = e.path();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() && depth < 24 {
                stack.push((p, depth + 1));
            } else if (t.is_file() || t.is_symlink()) && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("md") || x.eq_ignore_ascii_case("markdown"))
                && let Some(n) = read_note(root, &p)
            {
                out.push(n);
            }
        }
    }
    out.sort_by_key(|n| n.rel.to_lowercase());
    out
}

/// The folders of a vault (including empty ones), relative, sorted.
pub fn folders(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || !e.file_type().is_ok_and(|t| t.is_dir()) || depth >= 24 {
                continue;
            }
            let p = e.path();
            if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
            stack.push((p, depth + 1));
        }
    }
    out.sort_by_key(|s| s.to_lowercase());
    out
}

/// Put frontmatter and body back together.
pub fn join(fm: Option<&Frontmatter>, body: &str) -> String {
    match fm {
        Some(f) => format!("---\n{}---\n{}", f.to_yaml(), body),
        None => body.to_string(),
    }
}

/// A file name for a new note from its title ("Plans: 2027?" → "Plans 2027.md"), not taken yet.
pub fn note_file(dir: &Path, title: &str) -> PathBuf {
    let mut base: String = title.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '#' | '^' | '[' | ']') { ' ' } else { c }).collect();
    base = base.split_whitespace().collect::<Vec<_>>().join(" ");
    if base.is_empty() {
        base = "Untitled".into();
    }
    let mut p = dir.join(format!("{base}.md"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{base} {n}.md"));
        n += 1;
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frontmatter_round_trip() {
        let text = "---\ntitle: \"Plans: 2027\"\ntags: [work, ideas]\naliases:\n  - plans\n  - roadmap\nmeta:\n  owner: me\n  level: 2\ndraft: true\n---\n# Heading\nBody with #inline and `#not` a tag.\n";
        let (fm, body) = split(text);
        let fm = fm.unwrap();
        assert_eq!(fm.get("title"), Some(&Value::Text("Plans: 2027".into())));
        assert_eq!(fm.get("aliases"), Some(&Value::List(vec!["plans".into(), "roadmap".into()])));
        assert!(matches!(fm.get("meta"), Some(Value::Raw(_))));
        assert_eq!(fm.tags(), ["work", "ideas"]);
        assert_eq!(body, "# Heading\nBody with #inline and `#not` a tag.\n");
        assert_eq!(inline_tags(body), ["inline"]);
        // Written back, it reads the same (the nested map exactly as it was).
        let again = join(Some(&fm), body);
        let (fm2, body2) = split(&again);
        assert_eq!(fm2.unwrap(), fm);
        assert_eq!(body2, body);
        assert!(again.contains("meta:\n  owner: me\n  level: 2\n"));
    }

    #[test]
    fn no_frontmatter_and_tags() {
        let (fm, body) = split("Just text\n---\nnot a block");
        assert!(fm.is_none());
        assert!(body.starts_with("Just text"));
        assert_eq!(inline_tags("# Heading\n#tag at start, issue #123, nested #area/sub\n```\n#code\n```"), ["tag", "area/sub"]);
        let (fm, _) = split("---\ntags: one two\n---\n");
        assert_eq!(fm.unwrap().tags(), ["one", "two"]);
    }

    #[test]
    fn scans_a_vault() {
        let dir = std::env::temp_dir().join(format!("rivet-notes-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(dir.join("Projects/.git"));
        let _ = std::fs::create_dir_all(dir.join(".obsidian"));
        std::fs::write(dir.join("Inbox.md"), "Remember #todo\n").unwrap();
        std::fs::write(dir.join("Projects/Rivet.md"), "---\ntitle: Rivet\ntags: [work]\n---\nText\n").unwrap();
        std::fs::write(dir.join(".obsidian/hidden.md"), "x").unwrap();
        std::fs::write(dir.join("Projects/.git/x.md"), "x").unwrap();
        let notes = scan(&dir);
        assert_eq!(notes.iter().map(|n| n.rel.as_str()).collect::<Vec<_>>(), ["Inbox.md", "Projects/Rivet.md"]);
        assert_eq!(notes[1].title, "Rivet");
        assert_eq!(notes[1].folder(), "Projects");
        assert_eq!(notes[0].tags, ["todo"]);
        assert_eq!(folders(&dir), ["Projects"]);
        assert_eq!(note_file(&dir, "Inbox").file_name().unwrap(), "Inbox 2.md");
        for f in ["Inbox.md", "Projects/Rivet.md", ".obsidian/hidden.md", "Projects/.git/x.md"] {
            let _ = std::fs::remove_file(dir.join(f));
        }
        for d in ["Projects/.git", "Projects", ".obsidian", ""] {
            let _ = std::fs::remove_dir(dir.join(d));
        }
    }
}
