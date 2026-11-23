//! Browser colours: Chromium, Chrome, Brave and Edge take the theme's colour.
//!
//! These browsers read "managed policies": JSON files in a folder under `/etc`. Two of the
//! policies set the colour the browser makes its whole theme from, and light or dark. Rivet
//! keeps one file of its own in each installed browser's folder, writes it again when the
//! theme's colours change, and tells a running browser to read its policies again, so it
//! recolours without a restart. The tabs and toolbar stay the browser's own.
//!
//! The folders belong to root. Turning the feature on asks for the password once (`pkexec`)
//! to make Rivet's file and hand it to the person; after that the shell writes it with no
//! prompt. Nothing here runs or writes unless that file is there.

use std::path::{Path, PathBuf};

use crate::config::Settings;

/// Rivet's file in a policy folder. No other program's policies are read or changed.
const FILE: &str = "rivet-color.json";

/// A family of browsers: its name, the commands it is started by, and where it reads policies.
const KINDS: [(&str, &[&str], &str); 4] = [
    ("Chromium", &["chromium", "chromium-browser"], "etc/chromium/policies/managed"),
    ("Chrome", &["google-chrome-stable", "google-chrome"], "etc/opt/chrome/policies/managed"),
    ("Brave", &["brave-browser", "brave", "brave-origin"], "etc/brave/policies/managed"),
    ("Edge", &["microsoft-edge-stable", "microsoft-edge"], "etc/opt/edge/policies/managed"),
];

/// A browser that is installed.
#[derive(Clone, Debug, PartialEq)]
pub struct Browser {
    pub name: &'static str,
    /// The command that starts it, as found on the path.
    command: PathBuf,
    /// Where the program itself is, which is how its running copies are told from another's.
    home: PathBuf,
    /// Rivet's file in its policy folder.
    file: PathBuf,
}

fn on_path(command: &str) -> Option<PathBuf> {
    std::env::split_paths(&std::env::var_os("PATH")?).map(|d| d.join(command)).find(|p| p.is_file())
}

/// The browsers installed, each once (a command and its `-stable` twin are one browser).
pub fn found() -> Vec<Browser> {
    let mut out: Vec<Browser> = Vec::new();
    for (name, commands, dir) in KINDS {
        for c in commands {
            let Some(command) = on_path(c) else { continue };
            let real = std::fs::canonicalize(&command).unwrap_or_else(|_| command.clone());
            let home = real.parent().map(Path::to_path_buf).unwrap_or_default();
            if out.iter().all(|b| b.home != home) {
                out.push(Browser { name, command, home, file: Path::new("/").join(dir).join(FILE) });
            }
        }
    }
    out
}

impl Browser {
    /// Rivet's file is there and Rivet may write it: the one-time step has been done.
    pub fn ready(&self) -> bool {
        std::fs::OpenOptions::new().append(true).open(&self.file).is_ok()
    }

    fn running(&self) -> bool {
        let Ok(dir) = std::fs::read_dir("/proc") else { return false };
        dir.flatten().filter(|e| e.file_name().to_string_lossy().bytes().all(|b| b.is_ascii_digit())).any(|e| std::fs::read_link(e.path().join("exe")).is_ok_and(|exe| exe.starts_with(&self.home)))
    }
}

/// The one-time step: make Rivet's file in each browser's policy folder and give it to the
/// person, as root. Asks for the password; call it from a thread.
pub fn allow() -> Result<(), String> {
    let mut files: Vec<PathBuf> = found().into_iter().filter(|b| !b.ready()).map(|b| b.file).collect();
    files.dedup();
    if files.is_empty() {
        return Ok(());
    }
    // The paths are Rivet's own, with nothing in them a shell would read as more than a name.
    let uid = unsafe { libc::getuid() };
    let script: String = files.iter().map(|f| format!("mkdir -p '{}' && touch '{}' && chown {uid} '{}' && chmod 644 '{}'", f.parent().unwrap_or(Path::new("/")).display(), f.display(), f.display(), f.display())).collect::<Vec<_>>().join(" && ");
    let out = std::process::Command::new("pkexec").args(["sh", "-c", &script]).output().map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { "pkexec isn't installed".to_string() } else { e.to_string() })?;
    match out.status.code() {
        Some(0) => Ok(()),
        // pkexec's own: the password window was closed, or the password was refused.
        Some(126) | Some(127) => Err("the password wasn't given".to_string()),
        _ => Err(String::from_utf8_lossy(&out.stderr).trim().lines().last().unwrap_or("it didn't work").to_string()),
    }
}

/// The policy file: the colour the browser makes its theme from, and light or dark. With
/// nothing to set, an empty one (the folder is root's, so the file can't be taken away).
fn text(look: Option<([u8; 3], bool)>) -> String {
    match look {
        Some(([r, g, b], dark)) => format!("{{\"BrowserThemeColor\": \"#{r:02x}{g:02x}{b:02x}\", \"BrowserColorScheme\": \"{}\"}}\n", if dark { "dark" } else { "light" }),
        None => "{}\n".to_string(),
    }
}

/// The colour and light or dark the browsers get, as the shell looks now (None: turned off).
fn look(s: &Settings) -> Option<([u8; 3], bool)> {
    if !s.browser_colors {
        return None;
    }
    let live = rivet_look::live::LiveLook::new();
    let pal = rivet_look::palette::shown(live.palette(), s.follow_desktop_colors);
    let dark = pal.map(|p| p.is_dark()).unwrap_or(s.theme != rivet_look::theme::Mode::Light);
    let accent = live.accent(&s.choice());
    let surface = pal.and_then(|p| p.window_bg).unwrap_or(if dark { eframe::egui::Color32::from_rgb(0x24, 0x24, 0x24) } else { eframe::egui::Color32::from_rgb(0xf6, 0xf5, 0xf4) });
    let c = if s.browser_surface { surface } else { accent };
    Some(([c.r(), c.g(), c.b()], dark))
}

/// Bring each browser's file up to date with the settings and the theme, and have the ones
/// running read it. Does nothing for a browser whose file was never made.
pub fn apply(s: &Settings) {
    let all: Vec<Browser> = found().into_iter().filter(Browser::ready).collect();
    if all.is_empty() {
        return;
    }
    let text = text(look(s));
    for b in &all {
        if std::fs::read_to_string(&b.file).is_ok_and(|old| old == text) {
            continue;
        }
        if let Err(e) = std::fs::write(&b.file, &text) {
            eprintln!("rivet-shell: browser colors: {}: {e}", b.file.display());
        }
    }
    // Every running one, also those whose file was already right: two can share a folder.
    for b in all.iter().filter(|b| b.running()) {
        crate::sys::spawn(&b.command.to_string_lossy(), &["--refresh-platform-policy", "--no-startup-window"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_policy_file() {
        assert_eq!(text(Some(([0xe6, 0x61, 0x00], true))), "{\"BrowserThemeColor\": \"#e66100\", \"BrowserColorScheme\": \"dark\"}\n");
        assert_eq!(text(Some(([1, 2, 3], false))), "{\"BrowserThemeColor\": \"#010203\", \"BrowserColorScheme\": \"light\"}\n");
        assert_eq!(text(None), "{}\n");
    }

    /// What is on this machine: `cargo test -p rivet-shell browsers_here -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn browsers_here() {
        for b in found() {
            println!("{} {:?} home {:?} file {:?} ready {} running {}", b.name, b.command, b.home, b.file, b.ready(), b.running());
        }
    }

    #[test]
    fn rivets_file_is_in_each_browsers_own_folder() {
        for (_, _, dir) in KINDS {
            assert!(dir.ends_with("policies/managed") && !dir.starts_with('/'));
        }
    }
}
