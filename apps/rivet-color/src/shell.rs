//! Rivet Color as a Rivet Shell plugin (`rivet-color shell-plugin`, started by the shell's
//! Color plugin and kept running): a panel item showing the color picked last, whose menu
//! picks a new one and copies recent ones, and `@color` in the launcher, which lists the
//! history and palettes and copies a color typed in any format.
//!
//! The shell shows an icon from a picture's path, so each color shown gets a small square
//! drawn into the cache, named for the color so the shell's own cache of pictures stays right.

use std::io::BufRead;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use serde_json::{Value, json};

use crate::color::{Format, Rgb, parse};
use crate::store;

/// How often the history and palettes are looked at for changes from the window.
const EVERY: Duration = Duration::from_secs(2);
/// Recent colors in the item's menu.
const RECENT: usize = 8;

/// A small square of the color, for the shell to show as an icon.
pub fn swatch(c: Rgb) -> String {
    let dir = rivet_xdg::places::cache_home().join("rivet-color/swatches");
    let path = dir.join(format!("{}.svg", &c.hex()[1..]));
    if !path.exists() {
        let edge = if c.luminance() > 0.6 { "#00000055" } else { "#ffffff55" };
        let svg = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 16 16\"><rect x=\"1.5\" y=\"1.5\" width=\"13\" height=\"13\" rx=\"3\" fill=\"{}\" stroke=\"{edge}\"/></svg>", c.hex());
        let _ = std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, svg));
    }
    path.to_string_lossy().into_owned()
}

/// What the shell shows: the item, its menu and the launcher's entries.
fn status(history: &[Rgb], palettes: &[store::Palette], format: Format, show_code: bool, icon: &dyn Fn(Rgb) -> String) -> Value {
    let last = history.first().copied();
    let mut menu = vec![json!({"id": "pick", "label": "Pick a Color", "icon": "color-select-symbolic"}), json!({"id": "open", "label": "Open Rivet Color", "icon": "rivet-color"})];
    menu.push(json!({"heading": true, "label": "Recent"}));
    if history.is_empty() {
        menu.push(json!({"label": "Colors you pick show here", "quiet": true}));
    }
    for c in history.iter().take(RECENT) {
        menu.push(json!({"id": format!("copy:{}", c.hex()), "label": c.format(format), "sub": "Copy", "icon": icon(*c)}));
    }
    let mut launcher = vec![json!({"id": "pick", "label": "Pick a Color", "icon": "rivet-color"}), json!({"id": "convert", "label": format!("Copy as {}", format.name()), "icon": "rivet-color", "query": true, "scoped": true})];
    for c in history {
        launcher.push(json!({"id": format!("copy:{}", c.hex()), "label": c.format(format), "sub": "Recent", "icon": icon(*c), "scoped": true}));
    }
    for p in palettes {
        for (c, name) in &p.colors {
            let label = if name.is_empty() || name.starts_with('#') { c.format(format) } else { format!("{name} {}", c.format(format)) };
            launcher.push(json!({"id": format!("copy:{}", c.hex()), "label": label, "sub": p.name, "icon": icon(*c), "scoped": true}));
        }
    }
    json!({
        "icon": last.map(icon).unwrap_or_else(|| "rivet-color".into()),
        "text": if show_code { last.map(|c| c.format(format)).unwrap_or_default() } else { String::new() },
        "tooltip": match last { Some(c) => format!("Last picked: {}. Middle-click to pick.", c.format(format)), None => "Pick a color from the screen".into() },
        "menu": menu,
        "launcher": launcher,
    })
}

/// Copy a color as the settings say, put it first in the history, and say so.
fn copy(c: Rgb, format: Format) -> Value {
    let text = c.format(format);
    store::remember(c);
    if crate::pick::copy(&text) {
        json!({"notify": {"summary": format!("Copied {text}")}})
    } else {
        json!({"notify": {"summary": format!("Couldn't copy {text}"), "body": "Install wl-clipboard to copy from the panel."}})
    }
}

fn open_window(color: Option<Rgb>) {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("rivet-color"));
    let mut cmd = std::process::Command::new(exe);
    if let Some(c) = color {
        cmd.arg(c.hex());
    }
    let _ = cmd.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
}

pub fn run() -> ExitCode {
    let (tx, rx) = channel::<Option<String>>();
    let lines = tx.clone();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines().map_while(Result::ok) {
            if lines.send(Some(line)).is_err() {
                return;
            }
        }
        let _ = lines.send(None);
    });
    let show_code = std::env::var("RIVET_SET_SHOW_CODE").is_ok_and(|v| v == "true");
    let mut picking = false;
    let mut last = String::new();
    let mut seen = (None, None);
    loop {
        let now = (store::changed(&store::history_path()), store::changed(&store::palettes_dir()));
        let format = store::load_format();
        if now != seen || last.is_empty() {
            seen = now;
            let say = status(&store::load_history(), &store::load_palettes(), format, show_code, &swatch).to_string();
            if say != last {
                println!("{say}");
                last = say;
            }
        }
        let line = match rx.recv_timeout(EVERY) {
            Ok(Some(line)) => line,
            Ok(None) | Err(RecvTimeoutError::Disconnected) => return ExitCode::SUCCESS,
            Err(RecvTimeoutError::Timeout) => continue,
        };
        // A pick finished, on its thread.
        if let Some(answer) = line.strip_prefix("\u{0}picked:") {
            picking = false;
            match answer.strip_prefix("error:") {
                Some(e) => println!("{}", json!({"notify": {"summary": "Couldn't pick a color", "body": e}})),
                None => {
                    if let Some(c) = parse(answer) {
                        println!("{}", copy(c, format));
                    }
                }
            }
            last.clear();
            continue;
        }
        let Ok(ev) = serde_json::from_str::<Value>(&line) else { continue };
        let s = |k: &str| ev.get(k).and_then(Value::as_str).unwrap_or("");
        let id = match (s("event"), s("button")) {
            ("menu" | "launch", _) => s("id"),
            ("click", "middle") => "pick",
            ("click", "right") => "open",
            _ => continue,
        };
        match id {
            "pick" if !picking => {
                picking = true;
                let tx = tx.clone();
                std::thread::spawn(move || {
                    // A moment for the shell's menu or launcher to close, so it isn't picked.
                    std::thread::sleep(Duration::from_millis(250));
                    let answer = match crate::pick::pick() {
                        Ok(Some(c)) => c.hex(),
                        Ok(None) => "cancelled".into(),
                        Err(e) => format!("error:{e}"),
                    };
                    let _ = tx.send(Some(format!("\u{0}picked:{answer}")));
                });
            }
            "open" => open_window(store::load_history().first().copied()),
            "convert" => match parse(s("query")) {
                Some(c) => println!("{}", copy(c, format)),
                None => println!("{}", json!({"notify": {"summary": "That isn't a color", "body": "Type one like #d53f8c, rgb(213, 63, 140) or hsl(329, 64%, 54%)."}})),
            },
            other => {
                if let Some(c) = other.strip_prefix("copy:").and_then(parse) {
                    println!("{}", copy(c, format));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_and_launcher() {
        let icon = |c: Rgb| format!("/swatch/{}", c.hex());
        let pal = store::Palette { name: "Website".into(), colors: vec![(Rgb(0, 132, 184), "blue".into()), (Rgb(255, 242, 112), "#fff270".into())], path: PathBuf::new() };
        let history: Vec<Rgb> = (0..10).map(|i| Rgb(i * 20, 63, 140)).collect();
        let s = status(&history, &[pal], Format::Rgb, true, &icon);
        assert_eq!(s["icon"], "/swatch/#003f8c");
        assert_eq!(s["text"], "rgb(0, 63, 140)");
        let menu = s["menu"].as_array().unwrap();
        assert_eq!(menu[0]["id"], "pick");
        assert_eq!(menu.len(), 3 + RECENT);
        assert_eq!(menu[3]["id"], "copy:#003f8c");
        let launcher = s["launcher"].as_array().unwrap();
        assert_eq!(launcher.len(), 2 + 10 + 2);
        assert_eq!((launcher[1]["query"].as_bool(), launcher[1]["scoped"].as_bool()), (Some(true), Some(true)));
        assert_eq!(launcher[12]["label"], "blue rgb(0, 132, 184)");
        assert_eq!((launcher[13]["label"].as_str(), launcher[13]["sub"].as_str()), (Some("rgb(255, 242, 112)"), Some("Website")));
        let empty = status(&[], &[], Format::Hex, false, &icon);
        assert_eq!((empty["icon"].as_str(), empty["text"].as_str()), (Some("rivet-color"), Some("")));
        assert_eq!(empty["menu"][3]["quiet"], true);
    }
}
