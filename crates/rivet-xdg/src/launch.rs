//! Opening files: default applications (mimeapps.list), "Open With" choices from
//! installed .desktop entries, and opening a terminal in a folder.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crate::places;

#[derive(Clone, Debug)]
pub struct DesktopApp {
    pub id: String,
    pub name: String,
    pub exec: String,
    pub icon: String,
    pub mimes: Vec<String>,
    pub terminal: bool,
}

/// Where an app came from, when it isn't the system's own package: a Flatpak, a Snap or an
/// AppImage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Flatpak,
    Snap,
    AppImage,
}

impl Source {
    /// Its name, as each calls itself.
    pub fn label(self) -> &'static str {
        match self {
            Source::Flatpak => "Flatpak",
            Source::Snap => "Snap",
            Source::AppImage => "AppImage",
        }
    }
}

impl DesktopApp {
    /// Where it came from, from what its entry runs: `flatpak run`, a program in Snap's
    /// `snap/bin` (`/snap/bin`, or `/var/lib/snapd/snap/bin` where `/snap` isn't used), or an
    /// `.AppImage` file. None for the rest.
    pub fn source(&self) -> Option<Source> {
        let exec = self.exec.to_lowercase();
        if exec.split_whitespace().take(2).any(|w| w.ends_with("flatpak")) && exec.contains(" run ") {
            Some(Source::Flatpak)
        } else if exec.contains("/snap/bin/") {
            Some(Source::Snap)
        } else if exec.contains(".appimage") {
            Some(Source::AppImage)
        } else {
            None
        }
    }
}

fn app_dirs() -> Vec<PathBuf> {
    let mut v = vec![places::data_home().join("applications")];
    let dd = std::env::var("XDG_DATA_DIRS").ok().filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for d in dd.split(':') {
        let p = PathBuf::from(d).join("applications");
        if !v.contains(&p) {
            v.push(p);
        }
    }
    v
}

fn parse_desktop(path: &Path, id: String) -> Option<DesktopApp> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut in_entry = false;
    let (mut name, mut exec, mut icon, mut mimes) = (None, None, String::new(), Vec::new());
    let mut hidden = false;
    let mut terminal = false;
    let mut is_app = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_entry = line == "[Desktop Entry]";
            continue;
        }
        if !in_entry {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        match k.trim() {
            "Name" => name = Some(v.trim().to_string()),
            "Exec" => exec = Some(v.trim().to_string()),
            "Icon" => icon = v.trim().to_string(),
            "MimeType" => mimes = v.split(';').filter(|s| !s.is_empty()).map(|s| s.to_string()).collect(),
            "NoDisplay" | "Hidden" => hidden |= v.trim() == "true",
            "Terminal" => terminal = v.trim() == "true",
            "Type" => is_app = v.trim() == "Application",
            _ => {}
        }
    }
    if hidden || !is_app {
        return None;
    }
    Some(DesktopApp { id, name: name?, exec: exec?, icon, mimes, terminal })
}

pub fn all_apps() -> Vec<DesktopApp> {
    let mut v: Vec<DesktopApp> = Vec::new();
    for d in app_dirs() {
        let mut stack = vec![(d.clone(), String::new())];
        while let Some((dir, prefix)) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&dir) else { continue };
            for e in rd.flatten() {
                let p = e.path();
                let fname = e.file_name().to_string_lossy().to_string();
                if p.is_dir() {
                    stack.push((p, format!("{prefix}{fname}-")));
                } else if fname.ends_with(".desktop") {
                    let id = format!("{prefix}{fname}");
                    if v.iter().any(|a| a.id == id) {
                        continue;
                    }
                    if let Some(a) = parse_desktop(&p, id) {
                        v.push(a);
                    }
                }
            }
        }
    }
    v.sort_by_key(|a| a.name.to_lowercase());
    v
}

pub fn mimeapps_default(mime: &str) -> Option<String> {
    let mut files = vec![places::config_home().join("mimeapps.list")];
    if let Ok(d) = std::env::var("XDG_CURRENT_DESKTOP") {
        for de in d.split(':') {
            files.insert(0, places::config_home().join(format!("{}-mimeapps.list", de.to_lowercase())));
        }
    }
    for dd in app_dirs() {
        files.push(dd.join("mimeapps.list"));
        files.push(dd.join("defaults.list"));
    }
    // Each list read so far: its modification time and size when read, and its defaults.
    // Peek and the context menu ask every frame, so a list is read again only when it changes.
    type Lists = HashMap<PathBuf, ((Option<SystemTime>, u64), HashMap<String, String>)>;
    static LISTS: OnceLock<Mutex<Lists>> = OnceLock::new();
    let mut lists = LISTS.get_or_init(Default::default).lock().unwrap_or_else(|e| e.into_inner());
    for f in files {
        let Ok(meta) = std::fs::metadata(&f) else {
            lists.remove(&f);
            continue;
        };
        let stamp = (meta.modified().ok(), meta.len());
        if lists.get(&f).is_none_or(|(s, _)| *s != stamp) {
            let Ok(text) = std::fs::read_to_string(&f) else { continue };
            lists.insert(f.clone(), (stamp, parse_defaults(&text)));
        }
        if let Some(app) = lists.get(&f).and_then(|(_, d)| d.get(mime)) {
            return Some(app.clone());
        }
    }
    None
}

/// A mimeapps.list's [Default Applications]: each type's first app, the first line for a type winning.
fn parse_defaults(text: &str) -> HashMap<String, String> {
    let mut defaults = HashMap::new();
    let mut in_default = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_default = line == "[Default Applications]";
            continue;
        }
        if in_default
            && let Some((k, v)) = line.split_once('=')
            && let Some(first) = v.split(';').find(|s| !s.is_empty())
        {
            defaults.entry(k.trim().to_string()).or_insert_with(|| first.trim().to_string());
        }
    }
    defaults
}

/// Make an app the default for a file type (writes ~/.config/mimeapps.list via xdg-mime).
pub fn set_default_app(app_id: &str, mime: &str) -> Result<(), String> {
    let ok = Command::new("xdg-mime")
        .args(["default", app_id, mime])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false);
    if ok { Ok(()) } else { Err("Could not change the default application.".into()) }
}

pub fn default_app<'a>(apps: &'a [DesktopApp], mime: &str) -> Option<&'a DesktopApp> {
    if let Some(id) = mimeapps_default(mime)
        && let Some(a) = apps.iter().find(|a| a.id == id) {
            return Some(a);
        }
    apps.iter().find(|a| a.mimes.iter().any(|m| m == mime))
}

pub fn apps_for<'a>(apps: &'a [DesktopApp], mime: &str) -> Vec<&'a DesktopApp> {
    let media = format!("{}/*", mime.split('/').next().unwrap_or(""));
    let mut v: Vec<&DesktopApp> = apps
        .iter()
        .filter(|a| a.mimes.iter().any(|m| m == mime || *m == media))
        .collect();
    if mime.starts_with("text/") || mime == "application/x-shellscript" {
        for a in apps.iter().filter(|a| a.mimes.iter().any(|m| m == "text/plain")) {
            if !v.iter().any(|x| x.id == a.id) {
                v.push(a);
            }
        }
    }
    v
}

fn split_exec(exec: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut chars = exec.chars().peekable();
    let mut has = false;
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                in_q = !in_q;
                has = true;
            }
            '\\' if in_q => {
                if let Some(n) = chars.next() {
                    cur.push(n);
                }
            }
            ' ' if !in_q => {
                if has || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    has = false;
                }
            }
            _ => cur.push(c),
        }
    }
    if has || !cur.is_empty() {
        out.push(cur);
    }
    out
}

pub fn launch_app(app: &DesktopApp, files: &[PathBuf]) -> Result<(), String> {
    let mut args = exec_args(app, files);
    if args.is_empty() {
        return Err("Nothing to run".into());
    }
    if app.terminal {
        let mut t = terminal_command();
        t.push("-e".into());
        t.extend(args);
        args = t;
    }
    spawn_detached(&args, files.first().and_then(|f| f.parent()))
}

/// Expand a desktop entry's Exec= line into a command line for these files.
pub fn exec_args(app: &DesktopApp, files: &[PathBuf]) -> Vec<String> {
    let mut args: Vec<String> = Vec::new();
    let mut used = false;
    for tok in split_exec(&app.exec) {
        match tok.as_str() {
            "%f" | "%u" => {
                if let Some(f) = files.first() {
                    args.push(f.to_string_lossy().to_string());
                }
                used = true;
            }
            "%F" | "%U" => {
                args.extend(files.iter().map(|f| f.to_string_lossy().to_string()));
                used = true;
            }
            "%i" => {
                if !app.icon.is_empty() {
                    args.push("--icon".into());
                    args.push(app.icon.clone());
                }
            }
            "%c" => args.push(app.name.clone()),
            "%k" | "%d" | "%D" | "%n" | "%N" | "%v" | "%m" => {}
            t => args.push(t.replace("%%", "%")),
        }
    }
    if !used {
        args.extend(files.iter().map(|f| f.to_string_lossy().to_string()));
    }
    args
}

/// Variables the apps launched from here get on top of this program's own: a desktop shell
/// sets them for the session it runs (Rivet Shell names Qt's platform theme).
static ENV: std::sync::RwLock<Vec<(String, String)>> = std::sync::RwLock::new(Vec::new());

/// Give apps launched from now on this variable, or stop giving it (None).
pub fn set_env(key: &str, value: Option<&str>) {
    let mut env = ENV.write().unwrap_or_else(|e| e.into_inner());
    env.retain(|(k, _)| k != key);
    if let Some(v) = value {
        env.push((key.to_string(), v.to_string()));
    }
}

fn spawn_detached(args: &[String], cwd: Option<&Path>) -> Result<(), String> {
    let mut cmd = Command::new(&args[0]);
    cmd.args(&args[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    cmd.envs(ENV.read().unwrap_or_else(|e| e.into_inner()).iter().map(|(k, v)| (k, v)));
    if let Some(d) = cwd
        && d.is_dir() {
            cmd.current_dir(d);
        }
    let mut child = cmd.spawn().map_err(|e| format!("Could not start {}: {e}", args[0]))?;
    // Waited for, off to the side: an app that's closed is then gone from the process list,
    // not kept as a leftover until the program that started it quits.
    let _ = std::thread::Builder::new().name("launched app".into()).stack_size(64 * 1024).spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Another Rivet program (`rivet-vice`, `rivet-longshot`), when it is installed: beside this program
/// (the same build or package), else on the path.
pub fn rivet_program(name: &str) -> Option<PathBuf> {
    let beside = std::env::current_exe().ok().and_then(|p| std::fs::canonicalize(p).ok()).and_then(|p| p.parent().map(|d| d.join(name)));
    let on_path = std::env::var_os("PATH").and_then(|p| std::env::split_paths(&p).map(|d| d.join(name)).find(|c| c.is_file()));
    beside.filter(|p| p.is_file()).or(on_path)
}

/// Start a program with these arguments, apart from this one: it isn't waited on here, and its
/// output goes nowhere.
pub fn spawn(program: &Path, args: &[String]) -> Result<(), String> {
    let mut argv = vec![program.to_string_lossy().to_string()];
    argv.extend(args.iter().cloned());
    spawn_detached(&argv, None)
}

/// Open a web address or folder with the desktop's default handler.
pub fn open_url(url: &str) -> Result<(), String> {
    spawn_detached(&["xdg-open".into(), url.to_string()], None)
}

/// Open with the desktop's default application.
pub fn open_default(apps: &[DesktopApp], mime: &str, path: &Path) -> Result<(), String> {
    // Host .desktop files point at binaries outside the sandbox. The runtime's
    // xdg-open asks the desktop portal to open the file with the real default app.
    if places::in_flatpak() {
        return spawn_detached(&["xdg-open".into(), path.to_string_lossy().to_string()], None);
    }
    if let Some(a) = default_app(apps, mime) {
        return launch_app(a, &[path.to_path_buf()]);
    }
    spawn_detached(&["xdg-open".into(), path.to_string_lossy().to_string()], None)
}

fn which(bin: &str) -> bool {
    std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
        .unwrap_or(false)
}

fn terminal_command() -> Vec<String> {
    if let Ok(t) = std::env::var("TERMINAL")
        && !t.is_empty() && which(&t) {
            return vec![t];
        }
    for t in [
        "x-terminal-emulator", "kitty", "alacritty", "foot", "wezterm", "ghostty", "gnome-terminal",
        "kgx", "ptyxis", "konsole", "xfce4-terminal", "tilix", "terminator", "xterm",
    ] {
        if which(t) {
            return vec![t.to_string()];
        }
    }
    vec!["xterm".into()]
}

/// Run a shell command line in this folder (custom actions), maybe in a terminal window that
/// stays open until Enter so its output can be read.
pub fn run_shell(cmd: &str, dir: Option<&Path>, terminal: bool) -> Result<(), String> {
    if !terminal {
        return spawn_detached(&["sh".into(), "-c".into(), cmd.to_string()], dir);
    }
    let t = terminal_command();
    let name = Path::new(&t[0]).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    // How each terminal is told what to run.
    let flag: &[&str] = match name.as_str() {
        "gnome-terminal" | "kgx" | "ptyxis" => &["--"],
        "wezterm" => &["start", "--"],
        "xfce4-terminal" | "terminator" => &["-x"],
        _ => &["-e"],
    };
    let mut args = t;
    args.extend(flag.iter().map(|s| s.to_string()));
    args.extend(["sh".into(), "-c".into(), format!("{cmd}; echo; printf 'Press Enter to close'; read _")]);
    spawn_detached(&args, dir)
}

pub fn open_terminal(dir: &Path) -> Result<(), String> {
    let t = terminal_command();
    spawn_detached(&t, Some(dir))
}

pub fn run_executable(path: &Path) -> Result<(), String> {
    spawn_detached(&[path.to_string_lossy().to_string()], path.parent())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_apps_source_is_read_from_what_it_runs() {
        let app = |exec: &str| DesktopApp { id: String::new(), name: String::new(), exec: exec.into(), icon: String::new(), mimes: Vec::new(), terminal: false };
        assert_eq!(app("/usr/bin/flatpak run --branch=stable --arch=x86_64 --command=run.sh dev.aunetx.deezer @@u %U @@").source(), Some(Source::Flatpak));
        assert_eq!(app("/var/lib/snapd/snap/bin/telegram-desktop -- %U").source(), Some(Source::Snap));
        assert_eq!(app("env BAMF_DESKTOP_FILE_HINT=x /snap/bin/moonlight %U").source(), Some(Source::Snap));
        assert_eq!(app("\"/home/me/Applications/figcraft-0.18.1-linux-x86_64.AppImage\" %F").source(), Some(Source::AppImage));
        assert_eq!(app("firefox %u").source(), None);
        assert_eq!(app("rivet-edit --run flatpak").source(), None);
    }

    #[test]
    fn a_lists_first_default_for_a_type_wins() {
        let d = parse_defaults("[Added Associations]\ntext/plain=added.desktop;\n[Default Applications]\ntext/plain=;edit.desktop;other.desktop\ntext/plain=late.desktop\nimage/png = view.desktop\n");
        assert_eq!(d.get("text/plain").map(String::as_str), Some("edit.desktop"));
        assert_eq!(d.get("image/png").map(String::as_str), Some("view.desktop"));
        assert_eq!(d.len(), 2);
    }
}
