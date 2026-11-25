//! Rivet Edit as a Rivet Shell plugin (`rivet-edit shell-plugin`, started by the shell's Edit
//! plugin and kept running): a panel item whose menu lists the folders opened most recently,
//! and an entry for each in the launcher (`@edit` there shows only these). Choosing one opens
//! it in a new Rivet Edit window.
//!
//! The shell's side of it is in its PLUGINS.md: a JSON line out for what to show, a JSON line
//! in for each thing that happens.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use serde_json::{Value, json};

use crate::settings::Settings;

/// How often the settings file is looked at again.
const EVERY: Duration = Duration::from_secs(5);
/// How many folders are listed.
const SHOWN: usize = 5;

/// The folders opened most recently that are still there, newest first. (The list also holds
/// files opened by themselves; those aren't projects.)
fn folders(recent: &[PathBuf]) -> Vec<PathBuf> {
    recent.iter().filter(|p| p.is_dir()).take(SHOWN).cloned().collect()
}

/// A folder's own name, and where it is with the home folder as `~`.
fn names(p: &Path, home: &Path) -> (String, String) {
    let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.display().to_string());
    let at = match p.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => p.display().to_string(),
    };
    (name, at)
}

/// What the shell shows: the panel item, its menu and the launcher's entries.
fn status(folders: &[PathBuf], home: &Path) -> Value {
    let row = |p: &PathBuf, icon: bool| {
        let (name, at) = names(p, home);
        let mut r = json!({"id": format!("open:{}", p.display()), "label": name, "sub": at});
        if icon {
            r["icon"] = json!("rivet-edit");
        }
        r
    };
    let mut menu: Vec<Value> = folders.iter().map(|p| row(p, false)).collect();
    if folders.is_empty() {
        menu.push(json!({"label": "No recent folders yet: open one in Rivet Edit", "quiet": true}));
    }
    menu.push(json!({"heading": true, "label": ""}));
    menu.push(json!({"id": "window", "label": "New Window", "icon": "rivet-edit"}));
    let launcher: Vec<Value> = folders.iter().map(|p| row(p, true)).collect();
    let tooltip = match folders.len() {
        0 => "No recent folders".to_string(),
        1 => "1 recent folder".to_string(),
        n => format!("{n} recent folders"),
    };
    json!({"icon": "rivet-edit", "text": "", "tooltip": tooltip, "menu": menu, "launcher": launcher})
}

/// A new Rivet Edit window on `folder` (or an empty one).
fn open(folder: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut c = std::process::Command::new(exe);
    c.arg(folder.unwrap_or("--new-window"));
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    if let Ok(mut child) = c.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// What a line from the shell asks for: Some(None) an empty window, Some(Some(folder)) a folder.
fn wanted(line: &str) -> Option<Option<String>> {
    let ev: Value = serde_json::from_str(line).ok()?;
    let s = |k: &str| ev.get(k).and_then(Value::as_str).unwrap_or("");
    if !matches!(s("event"), "menu" | "launch") {
        return None;
    }
    match s("id") {
        "window" => Some(None),
        id => id.strip_prefix("open:").map(|n| Some(n.to_string())),
    }
}

pub fn run() -> ExitCode {
    let (tx, rx) = channel::<Option<String>>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if tx.send(Some(line)).is_err() {
                return;
            }
        }
        // The shell closed our input: it's done with us.
        let _ = tx.send(None);
    });
    let home = rivet_xdg::places::home();
    let mut last = String::new();
    loop {
        let text = status(&folders(&Settings::load().recent), &home).to_string();
        if text != last {
            println!("{text}");
            last = text;
        }
        match rx.recv_timeout(EVERY) {
            Ok(Some(line)) => {
                // Only a folder that's in the list: never whatever path a line names.
                match wanted(&line) {
                    Some(None) => open(None),
                    Some(Some(f)) if folders(&Settings::load().recent).iter().any(|p| p.display().to_string() == f) => open(Some(&f)),
                    _ => {}
                }
            }
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_the_five_newest_folders() {
        let root = std::env::temp_dir().join(format!("rivet-edit-shell-{}", std::process::id()));
        let mut recent = Vec::new();
        for i in 0..7 {
            let d = root.join(format!("project-{i}"));
            std::fs::create_dir_all(&d).unwrap();
            recent.push(d);
        }
        // A file opened by itself and a folder since deleted are both passed over.
        std::fs::write(root.join("notes.txt"), "").unwrap();
        recent.insert(1, root.join("notes.txt"));
        recent.insert(2, root.join("gone"));
        let f = folders(&recent);
        assert_eq!(f.len(), 5);
        assert_eq!(f[1], root.join("project-1"));

        let s = status(&f, &root);
        let menu = s["menu"].as_array().unwrap();
        let labels: Vec<&str> = menu.iter().map(|r| r["label"].as_str().unwrap_or("")).collect();
        assert_eq!(labels, ["project-0", "project-1", "project-2", "project-3", "project-4", "", "New Window"]);
        assert_eq!(menu[0]["sub"], "~/project-0");
        assert_eq!(menu[0]["id"], format!("open:{}", root.join("project-0").display()));
        assert_eq!(s["launcher"].as_array().unwrap().len(), 5);
        assert_eq!(s["tooltip"], "5 recent folders");
        assert_eq!(status(&[], &root)["menu"][0]["label"], "No recent folders yet: open one in Rivet Edit");
        assert_eq!(wanted(r#"{"event":"launch","id":"open:/a/b c","query":""}"#), Some(Some("/a/b c".into())));
        assert_eq!(wanted(r#"{"event":"menu","id":"window"}"#), Some(None));
        assert_eq!(wanted(r#"{"event":"click","button":"left"}"#), None);
        let _ = std::fs::remove_dir_all(&root);
    }
}
