//! Rivet Notes as a Rivet Shell plugin (`rivet-notes shell-plugin`, started by the shell's
//! Notes plugin and kept running): a panel item whose menu shows the vault's open tasks and its
//! latest notes, a New Note entry in the launcher, and a notice when something is due today.
//!
//! The shell's side of it is in its PLUGINS.md: a JSON line out for what to show, a JSON line
//! in for each thing that happens. The vault is the one chosen in the shell's Settings, Plugins
//! (`RIVET_SET_VAULT`), else the vault Rivet Notes had open last.

use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::{Duration, SystemTime};

use chrono::NaiveDate;
use serde_json::{Value, json};

use crate::settings::Settings;
use crate::vault::{self, Note};

/// How often the vault is read again.
const EVERY: Duration = Duration::from_secs(20);
/// Tasks and notes listed at most.
const TASKS: usize = 8;
const NOTES: usize = 5;
/// Notes the launcher can find (`@notes`), the latest first.
const FIND: usize = 300;

/// A setting the shell passes (`RIVET_SET_NAME`).
fn setting(name: &str) -> String {
    std::env::var(format!("RIVET_SET_{name}")).unwrap_or_default().trim().to_string()
}

/// The vault to show: the plugin's setting, else Notes' own latest.
fn vault_dir() -> Option<PathBuf> {
    let set = setting("VAULT");
    let dir = match set.strip_prefix("~/") {
        Some(rest) => Some(rivet_xdg::places::home().join(rest)),
        None if !set.is_empty() => Some(PathBuf::from(set)),
        None => Settings::load().vaults.into_iter().next(),
    };
    dir.filter(|d| d.is_dir())
}

/// An open task and the note it's in.
struct Task<'a> {
    note: &'a Note,
    item: &'a crate::tasks::Item,
}

/// The vault's open tasks, most pressing first: late, due today, due later, then the rest by
/// priority. `all`: every task, else only super todos (the ones Notes' Todos page shows).
fn open_tasks<'a>(notes: &'a [Note], triggers: &[String], all: bool) -> Vec<Task<'a>> {
    let mut out: Vec<Task> = notes.iter().flat_map(|n| n.tasks.iter().filter(|t| !t.done && (all || t.is_super(triggers))).map(move |item| Task { note: n, item })).collect();
    out.sort_by(|a, b| {
        let key = |t: &Task| (t.item.due.is_none(), t.item.due, std::cmp::Reverse(t.item.priority), !t.item.doing);
        key(a).cmp(&key(b))
    });
    out
}

/// When a note was last changed, in a few words.
fn ago(t: Option<SystemTime>) -> String {
    let secs = t.and_then(|t| t.elapsed().ok()).map(|d| d.as_secs()).unwrap_or(0);
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        86_400..172_800 => "yesterday".into(),
        _ => format!("{} days ago", secs / 86_400),
    }
}

fn due_words(d: NaiveDate, today: NaiveDate) -> String {
    match (d - today).num_days() {
        0 => "Today".into(),
        1 => "Tomorrow".into(),
        -1 => "Yesterday".into(),
        n if n < 0 => format!("{} days late", -n),
        n if n < 7 => d.format("%A").to_string(),
        _ => d.format("%-d %b").to_string(),
    }
}

/// What the shell shows for this vault: the panel item, its menu, the launcher's entry.
fn status(dir: Option<&Path>, notes: &[Note], triggers: &[String], all: bool, today: NaiveDate) -> Value {
    let Some(dir) = dir else {
        return json!({
            "icon": "rivet-notes", "text": "", "badge": false, "state": "normal", "tooltip": "No vault chosen",
            "menu": [{"id": "settings", "label": "Choose a vault…", "sub": "In Settings, Plugins, under Notes"}],
            "launcher": [],
        });
    };
    let name = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let tasks = open_tasks(notes, triggers, all);
    let due_now = tasks.iter().filter(|t| t.item.due.is_some_and(|d| d <= today)).count();
    let mut menu: Vec<Value> = Vec::new();
    menu.push(json!({"heading": true, "label": if tasks.is_empty() { "Tasks".to_string() } else { format!("Tasks · {}", tasks.len()) }}));
    for t in tasks.iter().take(TASKS) {
        let mut sub = Vec::new();
        if let Some(d) = t.item.due {
            sub.push(due_words(d, today));
        }
        sub.push(t.note.title.clone());
        let state = match t.item.due {
            Some(d) if d < today => "danger",
            Some(d) if d == today => "accent",
            _ => "normal",
        };
        menu.push(json!({"id": format!("note:{}", t.note.rel), "label": t.item.title, "sub": sub.join(" · "), "mark": t.item.mark().to_string(), "state": state}));
    }
    if tasks.is_empty() {
        menu.push(json!({"label": "Nothing to do", "quiet": true}));
    } else if tasks.len() > TASKS {
        menu.push(json!({"id": "open", "label": format!("{} more in Notes…", tasks.len() - TASKS), "quiet": true}));
    }
    menu.push(json!({"heading": true, "label": "Latest notes"}));
    let mut latest: Vec<&Note> = notes.iter().collect();
    latest.sort_by_key(|n| std::cmp::Reverse(n.modified));
    for n in latest.iter().take(NOTES) {
        let sub = if n.folder().is_empty() { ago(n.modified) } else { format!("{} · {}", n.folder(), ago(n.modified)) };
        menu.push(json!({"id": format!("note:{}", n.rel), "label": n.title, "sub": sub}));
    }
    if notes.is_empty() {
        menu.push(json!({"label": "No notes yet", "quiet": true}));
    }
    menu.push(json!({"heading": true, "label": ""}));
    menu.push(json!({"id": "new", "label": "New Note", "icon": "list-add-symbolic"}));
    menu.push(json!({"id": "open", "label": format!("Open {name}"), "icon": "rivet-notes"}));
    let tooltip = match due_now {
        0 => name.clone(),
        1 => format!("{name} · 1 task due"),
        n => format!("{name} · {n} tasks due"),
    };
    // In the launcher: New Note for anything typed, and under `@notes` every note by name.
    let mut launcher = vec![json!({"id": "new", "label": "New Note", "sub": format!("In {name}"), "icon": "rivet-notes", "query": true})];
    launcher.extend(latest.iter().take(FIND).map(|n| json!({"id": format!("note:{}", n.rel), "label": n.title, "sub": if n.folder().is_empty() { ago(n.modified) } else { format!("{} · {}", n.folder(), ago(n.modified)) }, "icon": "rivet-notes", "scoped": true})));
    json!({
        "icon": "rivet-notes",
        "text": if due_now > 0 { due_now.to_string() } else { String::new() },
        "badge": due_now > 0,
        "state": if due_now > 0 { "accent" } else { "normal" },
        "tooltip": tooltip,
        "menu": menu,
        "launcher": launcher,
    })
}

/// The notice for what's due today: once a day, and again when more comes due.
fn notice(notes: &[Note], triggers: &[String], all: bool, today: NaiveDate) -> Option<Value> {
    let due: Vec<String> = open_tasks(notes, triggers, all).iter().filter(|t| t.item.due == Some(today)).map(|t| t.item.title.clone()).collect();
    if due.is_empty() {
        return None;
    }
    // Remembered between runs, so the shell starting again doesn't say it twice.
    let stamp = rivet_xdg::places::state_home().join("rivet-notes/shell-due");
    let now = format!("{today} {}", due.len());
    let before = std::fs::read_to_string(&stamp).unwrap_or_default();
    let said = before.split_once(' ').filter(|(d, _)| *d == today.to_string()).and_then(|(_, n)| n.trim().parse::<usize>().ok()).unwrap_or(0);
    if due.len() <= said {
        return None;
    }
    if let Some(d) = stamp.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(&stamp, now);
    let summary = if due.len() == 1 { "1 task is due today".to_string() } else { format!("{} tasks are due today", due.len()) };
    Some(json!({"summary": summary, "body": due.iter().take(5).map(|t| format!("• {t}")).collect::<Vec<_>>().join("\n")}))
}

/// Open the Notes window on the vault (and a note in it).
fn open(dir: &Path, note: Option<&str>) {
    let Ok(exe) = std::env::current_exe() else { return };
    let mut c = std::process::Command::new(exe);
    c.arg(dir);
    if let Some(rel) = note {
        c.args(["--note", rel]);
    }
    c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null());
    if let Ok(mut child) = c.spawn() {
        std::thread::spawn(move || {
            let _ = child.wait();
        });
    }
}

/// Something happened in the shell: a menu row or the launcher's entry was chosen. Returns
/// what to tell the shell, if anything.
fn handle(ev: &Value, dir: Option<&Path>) -> Option<Value> {
    let s = |k: &str| ev.get(k).and_then(Value::as_str).unwrap_or("");
    let id = match s("event") {
        "menu" | "launch" => s("id"),
        _ => return None,
    };
    let Some(dir) = dir else {
        return (id == "settings").then(|| json!({"run": "settings"}));
    };
    match id {
        "new" => {
            let title = s("query").trim();
            match vault::create_note(dir, if title.is_empty() { "Untitled" } else { title }) {
                Ok(p) => open(dir, p.strip_prefix(dir).ok().and_then(Path::to_str)),
                Err(e) => return Some(json!({"notify": {"summary": "Couldn't make the note", "body": e.to_string()}})),
            }
        }
        "open" => open(dir, None),
        _ => {
            if let Some(rel) = id.strip_prefix("note:") {
                open(dir, Some(rel));
            }
        }
    }
    None
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
    let all = setting("ALL_TASKS") == "true";
    let mut last = String::new();
    loop {
        let dir = vault_dir();
        let triggers = Settings::load().todo_tags;
        let notes = dir.as_deref().map(vault::scan).unwrap_or_default();
        let today = chrono::Local::now().date_naive();
        let mut say = status(dir.as_deref(), &notes, &triggers, all, today);
        let text = say.to_string();
        // Only what changed is said again; a notice rides along with it, or on its own.
        let news = notice(&notes, &triggers, all, today);
        if text != last || news.is_some() {
            if let Some(n) = news {
                say["notify"] = n;
            }
            println!("{say}");
            last = text;
        }
        match rx.recv_timeout(EVERY) {
            Ok(Some(line)) => {
                if let Ok(ev) = serde_json::from_str::<Value>(&line)
                    && let Some(answer) = handle(&ev, dir.as_deref())
                {
                    println!("{answer}");
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
    fn rolls_up_tasks_and_latest_notes() {
        let dir = std::env::temp_dir().join(format!("rivet-notes-shell-{}", std::process::id())).join("Vault");
        std::fs::create_dir_all(dir.join("Work")).unwrap();
        std::fs::write(dir.join("Inbox.md"), "- [ ] Plain one\n- [ ] Later #task due:2026-12-01\n- [x] Done #task due:2026-11-18\n").unwrap();
        std::fs::write(dir.join("Work/Plan.md"), "- [/] Reply to Sam #task due:2026-11-18\n- [ ] Late one #task due:2026-11-10 priority:high\n- [ ] Someday #task\n").unwrap();
        let notes = vault::scan(&dir);
        let today = NaiveDate::from_ymd_opt(2026, 11, 18).unwrap();
        let triggers = vec!["task".to_string()];
        let titles = |all| open_tasks(&notes, &triggers, all).iter().map(|t| t.item.title.clone()).collect::<Vec<_>>();
        assert_eq!(titles(false), ["Late one", "Reply to Sam", "Later", "Someday"]);
        assert_eq!(titles(true).len(), 5);
        let s = status(Some(&dir), &notes, &triggers, false, today);
        assert_eq!((s["text"].as_str(), s["badge"].as_bool(), s["state"].as_str()), (Some("2"), Some(true), Some("accent")));
        let menu = s["menu"].as_array().unwrap();
        assert_eq!(menu[0]["label"], "Tasks · 4");
        assert_eq!((menu[1]["label"].as_str(), menu[1]["state"].as_str(), menu[1]["sub"].as_str()), (Some("Late one"), Some("danger"), Some("8 days late · Plan")));
        assert_eq!((menu[2]["mark"].as_str(), menu[2]["state"].as_str(), menu[2]["id"].as_str()), (Some("/"), Some("accent"), Some("note:Work/Plan.md")));
        assert!(menu.iter().any(|r| r["label"] == "Latest notes"));
        assert_eq!(s["launcher"][0]["query"], true);
        assert_eq!((s["launcher"].as_array().unwrap().len(), s["launcher"][1]["scoped"].as_bool()), (3, Some(true)));
        // No vault: only the way to choose one.
        let none = status(None, &[], &triggers, false, today);
        assert_eq!(none["menu"][0]["id"], "settings");
        // The launcher's New Note makes the file, with its title.
        let made = vault::create_note(&dir, "Shopping: list").unwrap();
        assert_eq!(made.file_name().unwrap(), "Shopping list.md");
        assert!(std::fs::read_to_string(&made).unwrap().contains("title: "));
        std::fs::remove_dir_all(dir.parent().unwrap()).ok();
    }
}
