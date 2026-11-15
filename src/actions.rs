//! Custom right-click actions: people's own scripts in the menu.
//!
//! Two formats other file managers use are read as they are, so existing actions just work:
//!
//! * **Nemo actions** (`*.nemo_action`): `[Nemo Action]` with `Name`, `Exec`, `Icon-Name`,
//!   `Comment`, `Selection` (s, m, any, none, notnone or a number), `Extensions` (dir, none,
//!   nodirs, any or a list), `Mimetypes`, `Dependencies`, `Terminal`, `Active`. Exec codes:
//!   `%F` paths, `%U` URIs, `%f`/`%N` the first item's name, `%P` the folder, `%p` its name.
//! * **Dolphin service menus** (`.desktop`): `MimeType` (`all/all`, `all/allfiles`,
//!   `inode/directory`, `image/*`…), `Actions` and `[Desktop Action …]` groups with `Name`,
//!   `Icon`, `Exec` (`%f`/`%u` once per item, `%F`/`%U` all at once), `X-KDE-Submenu`,
//!   `X-KDE-MinNumberOfUrls` / `X-KDE-MaxNumberOfUrls`.
//!
//! Read from Rivet's own folder (`~/.local/share/rivet/actions`, either format), the person's
//! Nemo actions (`~/.local/share/nemo/actions`) and service menus (`kio/servicemenus` in the
//! data folders). Commands run through `sh -c`, with every path quoted.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
enum Count {
    Any,
    /// Right-click on empty space (no items).
    None,
    NotNone,
    Exactly(usize),
    Range(usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
enum Style {
    Nemo,
    Kde,
}

#[derive(Clone, Debug)]
pub struct CustomAction {
    pub name: String,
    pub comment: String,
    pub icon: Option<String>,
    /// KDE's submenu name.
    pub submenu: Option<String>,
    exec: String,
    terminal: bool,
    count: Count,
    /// Extensions (lower-case, no dot), or the specials "dir", "none", "nodirs", "any".
    extensions: Vec<String>,
    mimes: Vec<String>,
    style: Style,
}

/// One selected item, as far as matching goes.
pub struct Item<'a> {
    pub path: &'a Path,
    pub is_dir: bool,
    pub mime: &'a str,
}

/// The folders actions are read from (Rivet's own first).
pub fn folders() -> Vec<PathBuf> {
    let data = crate::places::data_home();
    let mut v = vec![own_folder(), data.join("nemo/actions"), data.join("kio/servicemenus")];
    let sys = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    v.extend(sys.split(':').map(|d| PathBuf::from(d).join("kio/servicemenus")));
    v
}

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
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for dir in folders() {
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()).collect();
        files.sort();
        for f in files {
            // The same file name in a later folder is a system copy of one already read.
            let Some(name) = f.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            if !seen.insert(name.clone()) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            if name.ends_with(".nemo_action") {
                out.extend(parse_nemo(&text));
            } else if name.ends_with(".desktop") {
                out.extend(parse_kde(&text));
            }
        }
    }
    out
}

/// key=value pairs of one [group].
fn groups(text: &str) -> Vec<(String, Vec<(String, String)>)> {
    let mut out: Vec<(String, Vec<(String, String)>)> = Vec::new();
    for line in text.lines() {
        let l = line.trim();
        if l.is_empty() || l.starts_with('#') {
            continue;
        }
        if let Some(g) = l.strip_prefix('[').and_then(|g| g.strip_suffix(']')) {
            out.push((g.to_string(), Vec::new()));
        } else if let (Some((_, kv)), Some((k, v))) = (out.last_mut(), l.split_once('=')) {
            kv.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    out
}

fn get<'a>(kv: &'a [(String, String)], key: &str) -> Option<&'a str> {
    kv.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str())
}

fn list(v: Option<&str>) -> Vec<String> {
    v.unwrap_or("").split(';').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

fn installed(program: &str) -> bool {
    if program.contains('/') {
        return Path::new(program).exists();
    }
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(program).is_file()))
}

fn parse_nemo(text: &str) -> Option<CustomAction> {
    let g = groups(text);
    let (_, kv) = g.iter().find(|(n, _)| n == "Nemo Action")?;
    if get(kv, "Active").is_some_and(|a| a.eq_ignore_ascii_case("false")) {
        return None;
    }
    // Every dependency must be there ("!name": must not be).
    for dep in list(get(kv, "Dependencies")) {
        let (want, prog) = match dep.strip_prefix('!') {
            Some(p) => (false, p.to_string()),
            None => (true, dep),
        };
        if installed(&prog) != want {
            return None;
        }
    }
    let count = match get(kv, "Selection").map(|s| s.to_lowercase()).as_deref() {
        Some("s") => Count::Exactly(1),
        Some("m") => Count::Range(2, usize::MAX),
        Some("none") => Count::None,
        Some("notnone") => Count::NotNone,
        Some(n) if n.parse::<usize>().is_ok() => Count::Exactly(n.parse().unwrap_or(1)),
        _ => Count::Any,
    };
    let mut extensions: Vec<String> = list(get(kv, "Extensions")).into_iter().map(|e| e.trim_start_matches('.').to_lowercase()).collect();
    let mimes = list(get(kv, "Mimetypes"));
    if extensions.is_empty() && mimes.is_empty() {
        extensions.push("any".into());
    }
    Some(CustomAction {
        name: get(kv, "Name").unwrap_or("Action").replace('_', ""),
        comment: get(kv, "Comment").unwrap_or("").to_string(),
        icon: get(kv, "Icon-Name").map(str::to_string),
        submenu: None,
        exec: get(kv, "Exec")?.trim_start_matches('<').trim_end_matches('>').to_string(),
        terminal: get(kv, "Terminal").is_some_and(|t| t.eq_ignore_ascii_case("true")),
        count,
        extensions,
        mimes,
        style: Style::Nemo,
    })
}

fn parse_kde(text: &str) -> Vec<CustomAction> {
    let g = groups(text);
    let Some((_, entry)) = g.iter().find(|(n, _)| n == "Desktop Entry") else { return Vec::new() };
    // Only file manager context-menu entries.
    let service_types = format!("{} {}", get(entry, "ServiceTypes").unwrap_or(""), get(entry, "X-KDE-ServiceTypes").unwrap_or(""));
    if !service_types.contains("KonqPopupMenu") && get(entry, "Type") != Some("Service") {
        return Vec::new();
    }
    let mimes = list(get(entry, "MimeType"));
    let min = get(entry, "X-KDE-MinNumberOfUrls").and_then(|v| v.parse().ok()).unwrap_or(1);
    let max = get(entry, "X-KDE-MaxNumberOfUrls").and_then(|v| v.parse().ok()).unwrap_or(usize::MAX);
    let submenu = get(entry, "X-KDE-Submenu").map(str::to_string);
    list(get(entry, "Actions"))
        .into_iter()
        .filter_map(|id| {
            let (_, a) = g.iter().find(|(n, _)| *n == format!("Desktop Action {id}"))?;
            Some(CustomAction {
                name: get(a, "Name")?.to_string(),
                comment: String::new(),
                icon: get(a, "Icon").map(str::to_string),
                submenu: submenu.clone(),
                exec: get(a, "Exec")?.to_string(),
                terminal: false,
                count: Count::Range(min, max),
                extensions: Vec::new(),
                mimes: mimes.clone(),
                style: Style::Kde,
            })
        })
        .collect()
}

fn mime_matches(pattern: &str, item: &Item) -> bool {
    match pattern {
        "all/all" | "*" | "*/*" => true,
        "all/allfiles" | "application/octet-stream" if !item.is_dir => true,
        "inode/directory" => item.is_dir,
        p => match p.strip_suffix("/*") {
            Some(top) => !item.is_dir && item.mime.split('/').next() == Some(top),
            None => item.mime == p,
        },
    }
}

impl CustomAction {
    /// Does this action belong in the menu for these items (empty = right-click on empty space)?
    pub fn applies(&self, items: &[Item]) -> bool {
        let n = items.len();
        let count_ok = match self.count {
            Count::Any => true,
            Count::None => n == 0,
            Count::NotNone => n > 0,
            Count::Exactly(k) => n == k,
            Count::Range(lo, hi) => n >= lo && n <= hi,
        };
        if !count_ok {
            return false;
        }
        if n == 0 {
            return true;
        }
        items.iter().all(|it| {
            let ext_ok = self.extensions.iter().any(|e| match e.as_str() {
                "any" => true,
                "dir" => it.is_dir,
                "nodirs" => !it.is_dir,
                "none" => !it.is_dir && it.path.extension().is_none(),
                ext => !it.is_dir && it.path.extension().is_some_and(|x| x.to_string_lossy().to_lowercase() == ext),
            });
            let mime_ok = self.mimes.iter().any(|m| mime_matches(m, it));
            match self.style {
                // Nemo: either list may match.
                Style::Nemo => ext_ok || mime_ok,
                Style::Kde => mime_ok,
            }
        })
    }

    /// Run it for these items in this folder.
    pub fn run(&self, items: &[PathBuf], folder: Option<&Path>) -> Result<(), String> {
        for cmd in self.commands(items, folder) {
            crate::launch::run_shell(&cmd, folder, self.terminal)?;
        }
        Ok(())
    }

    /// The shell command line(s) it runs (one per item for KDE's %f / %u).
    fn commands(&self, items: &[PathBuf], folder: Option<&Path>) -> Vec<String> {
        let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
        let uri = |p: &Path| format!("file://{}", crate::places::percent_encode_path(p));
        let name = |p: &Path| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let fill = |exec: &str, items: &[PathBuf]| -> String {
            let mut out = String::new();
            let mut chars = exec.chars().peekable();
            while let Some(c) = chars.next() {
                if c != '%' {
                    out.push(c);
                    continue;
                }
                let first = items.first().map(|p| p.as_path());
                match chars.next() {
                    Some('F') => out.push_str(&items.iter().map(|p| q(&p.to_string_lossy())).collect::<Vec<_>>().join(" ")),
                    Some('U') => out.push_str(&items.iter().map(|p| q(&uri(p))).collect::<Vec<_>>().join(" ")),
                    Some('f') if self.style == Style::Kde => out.push_str(&first.map(|p| q(&p.to_string_lossy())).unwrap_or_default()),
                    Some('u') => out.push_str(&first.map(|p| q(&uri(p))).unwrap_or_default()),
                    Some('f') | Some('N') => out.push_str(&first.map(|p| q(&name(p))).unwrap_or_default()),
                    Some('P') => out.push_str(&folder.map(|d| q(&d.to_string_lossy())).unwrap_or_default()),
                    Some('p') => out.push_str(&folder.map(|d| q(&name(d))).unwrap_or_default()),
                    Some('%') => out.push('%'),
                    // Codes other formats use (%i icon, %c caption, %k file): nothing to add.
                    _ => {}
                }
            }
            out
        };
        // KDE's single-item codes (%f, %u) mean "once for each item".
        let per_item = self.style == Style::Kde && (self.exec.contains("%f") || self.exec.contains("%u")) && items.len() > 1;
        if per_item {
            items.iter().map(|p| fill(&self.exec, std::slice::from_ref(p))).collect()
        } else {
            vec![fill(&self.exec, items)]
        }
    }
}

/// An example action, for people to copy and change.
pub const EXAMPLE: &str = "[Nemo Action]
# Shown in the right-click menu. Rivet also reads Nemo actions and Dolphin service menus.
Name=Make Executable
Comment=Let these files run as programs
Icon-Name=application-x-executable
# %F is the selected files, quoted. Others: %U their URIs, %f the first one's name,
# %P the folder you're in.
Exec=chmod +x %F
# s = one item, m = several, any, none (empty space), notnone, or a number.
Selection=notnone
# File endings to show it for; dir = folders, nodirs = any file, any = everything.
Extensions=nodirs;
# Run in a terminal window (to see what it prints).
Terminal=false
";

#[cfg(test)]
mod tests {
    use super::*;

    fn item<'a>(path: &'a Path, mime: &'a str, is_dir: bool) -> Item<'a> {
        Item { path, is_dir, mime }
    }

    #[test]
    fn nemo_action_matches_and_fills_codes() {
        let a = parse_nemo("[Nemo Action]\nName=_Shrink\nExec=<convert %F -resize 50% %P/out.png>\nSelection=s\nExtensions=png;jpg;\n").unwrap();
        assert_eq!(a.name, "Shrink");
        let p = Path::new("/tmp/a b.png");
        assert!(a.applies(&[item(p, "image/png", false)]));
        assert!(!a.applies(&[item(Path::new("/tmp/x.txt"), "text/plain", false)]));
        assert!(!a.applies(&[item(p, "image/png", false), item(p, "image/png", false)]));
        assert!(!a.applies(&[]));
        // Folders: "dir" only.
        let d = parse_nemo("[Nemo Action]\nName=Here\nExec=foo %P\nSelection=none\nExtensions=dir;\n").unwrap();
        assert!(d.applies(&[]));
        assert!(parse_nemo("[Nemo Action]\nName=Off\nExec=x\nActive=false\n").is_none());
        assert!(parse_nemo("[Nemo Action]\nName=Needs\nExec=x\nDependencies=surely-not-installed-xyz;\n").is_none());
    }

    #[test]
    fn kde_service_menu_reads_its_actions() {
        let text = "[Desktop Entry]\nType=Service\nX-KDE-ServiceTypes=KonqPopupMenu/Plugin\nMimeType=image/*;\nActions=a;b;\nX-KDE-Submenu=Images\n\n[Desktop Action a]\nName=Rotate\nExec=rotate %f\n\n[Desktop Action b]\nName=Stack\nExec=stack %F\n";
        let v = parse_kde(text);
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].submenu.as_deref(), Some("Images"));
        let p = Path::new("/tmp/a.png");
        assert!(v[0].applies(&[item(p, "image/png", false)]));
        assert!(!v[0].applies(&[item(Path::new("/tmp"), "inode/directory", true)]));
    }

    #[test]
    fn commands_quote_every_path() {
        let a = parse_nemo("[Nemo Action]\nName=x\nExec=tool %F --in %P --name %f 100%%\n").unwrap();
        let items = vec![PathBuf::from("/tmp/it's here.txt"), PathBuf::from("/tmp/b.txt")];
        let c = a.commands(&items, Some(Path::new("/tmp/My Folder")));
        assert_eq!(c, vec!["tool '/tmp/it'\\''s here.txt' '/tmp/b.txt' --in '/tmp/My Folder' --name 'it'\\''s here.txt' 100%".to_string()]);
        // KDE: %f runs once per item.
        let k = &parse_kde("[Desktop Entry]\nType=Service\nMimeType=all/all;\nActions=a;\n[Desktop Action a]\nName=A\nExec=open %f\n")[0];
        assert_eq!(k.commands(&items, None).len(), 2);
    }

    #[test]
    fn example_parses() {
        assert!(parse_nemo(EXAMPLE).is_some());
    }
}
