//! Custom right-click actions: people's own scripts in the menu.
//!
//! One file per action, ending in `.action`, in `~/.local/share/rivet/actions`:
//!
//! ```text
//! [Action]
//! Name=Make Executable
//! Comment=Let these files run as programs
//! Icon=application-x-executable
//! Exec=chmod +x %F
//! For=some
//! Types=files
//! ```
//!
//! * `Exec`: a shell command line. `%F` the selected paths, `%f` the first one, `%U` / `%u`
//!   the same as `file://` addresses, `%n` the first one's name, `%d` the folder you're in,
//!   `%%` a percent sign. Every path is quoted.
//! * `For`: `one`, `many` (two or more), `some` (one or more), `any`, or `none` (right-click on
//!   empty space). Default: `some`.
//! * `Types`: `files`, `folders`, `all`, file types (`image/png`, `image/*`) or endings
//!   (`.png`); every selected item must match one. Default: `all`.
//! * `Each=true` runs the command once for each item; `Terminal=true` runs it in a terminal
//!   window; `Requires=` lists programs that must be installed; `Menu=` groups it in a submenu;
//!   `Enabled=false` hides it.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
enum For {
    One,
    Many,
    Some,
    Any,
    /// Right-click on empty space (no items).
    None,
}

#[derive(Clone, Debug)]
pub struct CustomAction {
    pub name: String,
    pub comment: String,
    pub icon: Option<String>,
    /// The submenu it goes in, if any.
    pub submenu: Option<String>,
    exec: String,
    terminal: bool,
    each: bool,
    count: For,
    /// "files", "folders", "all", MIME types (maybe "image/*") and endings (".png").
    types: Vec<String>,
}

/// One selected item, as far as matching goes.
pub struct Item<'a> {
    pub path: &'a Path,
    pub is_dir: bool,
    pub mime: &'a str,
}

/// Where actions live.
pub fn own_folder() -> PathBuf {
    crate::places::data_home().join("rivet/actions")
}

/// All actions, re-read every few seconds (so new files show up without a restart).
pub fn all() -> Vec<CustomAction> {
    static CACHE: Mutex<Option<(Instant, Vec<CustomAction>)>> = Mutex::new(None);
    let mut c = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((t, v)) = c.as_ref()
        && t.elapsed() < Duration::from_secs(3)
    {
        return v.clone();
    }
    let v = load();
    *c = Some((Instant::now(), v.clone()));
    v
}

fn load() -> Vec<CustomAction> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(own_folder())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "action"))
        .collect();
    files.sort();
    files.iter().filter_map(|f| std::fs::read_to_string(f).ok()).filter_map(|t| parse(&t)).collect()
}

/// The `key=value` lines of the `[Action]` section.
fn keys(text: &str) -> Option<Vec<(String, String)>> {
    let mut in_action = false;
    let mut seen = false;
    let mut kv = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if l.starts_with('[') {
            in_action = l == "[Action]";
            seen |= in_action;
        } else if in_action && let Some((k, v)) = l.split_once('=') {
            kv.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    seen.then_some(kv)
}

fn installed(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

fn parse(text: &str) -> Option<CustomAction> {
    let kv = keys(text)?;
    let get = |k: &str| kv.iter().find(|(key, _)| key == k).map(|(_, v)| v.as_str());
    let list = |k: &str| -> Vec<String> { get(k).unwrap_or("").split([';', ',']).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect() };
    let yes = |k: &str| get(k).is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
    if get("Enabled").is_some_and(|v| v.eq_ignore_ascii_case("false") || v == "0") {
        return None;
    }
    if !list("Requires").iter().all(|p| installed(p)) {
        return None;
    }
    let count = match get("For").map(|s| s.to_lowercase()).as_deref() {
        Some("one") => For::One,
        Some("many") => For::Many,
        Some("any") => For::Any,
        Some("none") => For::None,
        _ => For::Some,
    };
    let mut types: Vec<String> = list("Types").into_iter().map(|t| t.to_lowercase()).collect();
    if types.is_empty() {
        types.push("all".into());
    }
    Some(CustomAction {
        name: get("Name")?.to_string(),
        comment: get("Comment").unwrap_or("").to_string(),
        icon: get("Icon").map(str::to_string),
        submenu: get("Menu").filter(|m| !m.is_empty()).map(str::to_string),
        exec: get("Exec")?.to_string(),
        terminal: yes("Terminal"),
        each: yes("Each"),
        count,
        types,
    })
}

fn type_matches(t: &str, item: &Item) -> bool {
    match t {
        "all" => true,
        "files" => !item.is_dir,
        "folders" => item.is_dir,
        ending if ending.starts_with('.') => {
            !item.is_dir && item.path.file_name().is_some_and(|n| n.to_string_lossy().to_lowercase().ends_with(ending))
        }
        mime => match mime.strip_suffix("/*") {
            Some(top) => !item.is_dir && item.mime.split('/').next() == Some(top),
            None => item.mime == mime || (mime == "inode/directory" && item.is_dir),
        },
    }
}

impl CustomAction {
    /// Does this action belong in the menu for these items (empty = right-click on empty space)?
    pub fn applies(&self, items: &[Item]) -> bool {
        let n = items.len();
        let count_ok = match self.count {
            For::One => n == 1,
            For::Many => n >= 2,
            For::Some => n >= 1,
            For::Any => true,
            For::None => n == 0,
        };
        count_ok && items.iter().all(|it| self.types.iter().any(|t| type_matches(t, it)))
    }

    /// Run it for these items in this folder.
    pub fn run(&self, items: &[PathBuf], folder: Option<&Path>) -> Result<(), String> {
        for cmd in self.commands(items, folder) {
            crate::launch::run_shell(&cmd, folder, self.terminal)?;
        }
        Ok(())
    }

    /// The shell command line(s) it runs (one per item with `Each=true`).
    fn commands(&self, items: &[PathBuf], folder: Option<&Path>) -> Vec<String> {
        let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let uri = |p: &Path| format!("file://{}", crate::places::percent_encode_path(p));
        let fill = |items: &[PathBuf]| -> String {
            let first = items.first().map(|p| p.as_path());
            let mut out = String::new();
            let mut chars = self.exec.chars();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('F') => out.push_str(&items.iter().map(|p| q(&p.to_string_lossy())).collect::<Vec<_>>().join(" ")),
                    Some('f') => out.push_str(&first.map(|p| q(&p.to_string_lossy())).unwrap_or_default()),
                    Some('U') => out.push_str(&items.iter().map(|p| q(&uri(p))).collect::<Vec<_>>().join(" ")),
                    Some('u') => out.push_str(&first.map(|p| q(&uri(p))).unwrap_or_default()),
                    Some('n') => out.push_str(&first.and_then(|p| p.file_name()).map(|n| q(&n.to_string_lossy())).unwrap_or_default()),
                    Some('d') => out.push_str(&folder.map(|d| q(&d.to_string_lossy())).unwrap_or_default()),
                    Some('%') => out.push('%'),
                    Some(other) => {
                        out.push('%');
                        out.push(other);
                    }
                    None => out.push('%'),
                }
            }
            out
        };
        if self.each && items.len() > 1 {
            items.iter().map(|p| fill(std::slice::from_ref(p))).collect()
        } else {
            vec![fill(items)]
        }
    }
}

/// An example action, for people to copy and change.
pub const EXAMPLE: &str = "[Action]
# The name in the right-click menu, and a line shown when you point at it.
Name=Make Executable
Comment=Let these files run as programs
Icon=application-x-executable
# The command. %F is the selected files (quoted); %f the first one, %n its name,
# %U / %u the same as file:// addresses, %d the folder you're in.
Exec=chmod +x %F
# When it shows: one, many, some (one or more), any, or none (right-click on empty space).
For=some
# For which items: files, folders, all, file types (image/*) or endings (.png).
Types=files
# Run once for each item instead of once for all of them.
Each=false
# Run in a terminal window, to see what it prints.
Terminal=false
# Only when these programs are installed (e.g. Requires=convert).
Requires=
# Put it in a submenu.
Menu=
";

#[cfg(test)]
mod tests {
    use super::*;

    fn item<'a>(path: &'a Path, mime: &'a str, is_dir: bool) -> Item<'a> {
        Item { path, is_dir, mime }
    }

    #[test]
    fn matches_by_count_and_type() {
        let a = parse("[Action]\nName=Shrink\nExec=convert %F -resize 50% out.png\nFor=one\nTypes=.png;image/jpeg\n").unwrap();
        let png = Path::new("/tmp/a b.png");
        assert!(a.applies(&[item(png, "image/png", false)]));
        assert!(a.applies(&[item(Path::new("/tmp/c.jpg"), "image/jpeg", false)]));
        assert!(!a.applies(&[item(Path::new("/tmp/x.txt"), "text/plain", false)]));
        assert!(!a.applies(&[item(png, "image/png", false), item(png, "image/png", false)]));
        assert!(!a.applies(&[]));
        let here = parse("[Action]\nName=Here\nExec=foo %d\nFor=none\n").unwrap();
        assert!(here.applies(&[]));
        let dirs = parse("[Action]\nName=Dirs\nExec=x\nTypes=folders\n").unwrap();
        assert!(dirs.applies(&[item(Path::new("/tmp"), "inode/directory", true)]));
        assert!(!dirs.applies(&[item(png, "image/png", false)]));
        let pics = parse("[Action]\nName=Pics\nExec=x\nTypes=image/*\nMenu=Pictures\n").unwrap();
        assert_eq!(pics.submenu.as_deref(), Some("Pictures"));
        assert!(pics.applies(&[item(png, "image/png", false)]));
        assert!(parse("[Action]\nName=Off\nExec=x\nEnabled=false\n").is_none());
        assert!(parse("[Action]\nName=Needs\nExec=x\nRequires=surely-not-installed-xyz\n").is_none());
        assert!(parse("[Other]\nName=x\nExec=x\n").is_none());
    }

    #[test]
    fn commands_quote_every_path() {
        let a = parse("[Action]\nName=x\nExec=tool %F --in %d --name %n 100%%\n").unwrap();
        let items = vec![PathBuf::from("/tmp/it's here.txt"), PathBuf::from("/tmp/b.txt")];
        let c = a.commands(&items, Some(Path::new("/tmp/My Folder")));
        assert_eq!(c, vec!["tool '/tmp/it'\\''s here.txt' '/tmp/b.txt' --in '/tmp/My Folder' --name 'it'\\''s here.txt' 100%".to_string()]);
        let each = parse("[Action]\nName=e\nExec=open %f\nEach=true\n").unwrap();
        assert_eq!(each.commands(&items, None), vec!["open '/tmp/it'\\''s here.txt'".to_string(), "open '/tmp/b.txt'".to_string()]);
    }

    #[test]
    fn example_parses() {
        let a = parse(EXAMPLE).unwrap();
        assert_eq!(a.name, "Make Executable");
        assert!(a.applies(&[item(Path::new("/tmp/run.sh"), "application/x-shellscript", false)]));
    }
}
