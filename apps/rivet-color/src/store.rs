//! What Rivet Color keeps: the colors picked and copied lately, the palettes saved, and the
//! one setting (the way a copied color is written). The window and the shell plugin are
//! separate programs sharing these files, so each write replaces its file whole, and each
//! reader looks again when a file changes.
//!
//! - `~/.local/share/rivet-color/history`: one `#rrggbb` per line, the latest first.
//! - `~/.local/share/rivet-color/palettes/NAME.gpl`: GIMP's palette format, which GIMP,
//!   Inkscape, Krita and Aseprite read as they are.
//! - `~/.config/rivet-color/settings`: `format=hex|rgb|hsl|oklch`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use crate::color::{Format, Rgb, parse};

/// Checks (screenshot runs) never write.
pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// Colors kept in the history at most.
pub const HISTORY: usize = 60;

pub fn data_dir() -> PathBuf {
    rivet_xdg::places::data_home().join("rivet-color")
}

pub fn history_path() -> PathBuf {
    data_dir().join("history")
}

pub fn palettes_dir() -> PathBuf {
    data_dir().join("palettes")
}

fn settings_path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-color/settings")
}

/// When a file last changed, to notice another program writing it.
pub fn changed(p: &Path) -> Option<SystemTime> {
    std::fs::metadata(p).and_then(|m| m.modified()).ok()
}

/// Write a file whole: to a new file beside it, then over it, so a reader never sees half.
fn write(path: &Path, text: &str) -> Result<(), String> {
    if NO_SAVE.load(Ordering::Relaxed) {
        return Ok(());
    }
    if let Some(d) = path.parent() {
        std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
    }
    let tmp = path.with_extension(format!("new{}", std::process::id()));
    std::fs::write(&tmp, text).and_then(|_| std::fs::rename(&tmp, path)).map_err(|e| e.to_string())
}

pub fn load_format() -> Format {
    let text = std::fs::read_to_string(settings_path()).unwrap_or_default();
    text.lines().filter_map(|l| l.split_once('=')).find(|(k, _)| k.trim() == "format").and_then(|(_, v)| Format::from_id(v.trim())).unwrap_or(Format::Hex)
}

pub fn save_format(f: Format) {
    let _ = write(&settings_path(), &format!("format={}\n", f.id()));
}

pub fn load_history() -> Vec<Rgb> {
    let text = std::fs::read_to_string(history_path()).unwrap_or_default();
    let mut out: Vec<Rgb> = Vec::new();
    for c in text.lines().filter_map(parse) {
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out.truncate(HISTORY);
    out
}

pub fn save_history(list: &[Rgb]) {
    let text: String = list.iter().take(HISTORY).map(|c| format!("{}\n", c.hex())).collect();
    let _ = write(&history_path(), &text);
}

/// Put a color at the front of the history (moving it there when it's already in it).
pub fn remember(c: Rgb) -> Vec<Rgb> {
    let mut list = load_history();
    list.retain(|x| *x != c);
    list.insert(0, c);
    list.truncate(HISTORY);
    save_history(&list);
    list
}

/// A saved palette: its name and its colors, each with a name of its own (often empty).
#[derive(Clone, Debug, PartialEq)]
pub struct Palette {
    pub name: String,
    pub colors: Vec<(Rgb, String)>,
    /// Its file; empty until it's saved.
    pub path: PathBuf,
}

impl Palette {
    /// Read a GIMP palette: a `GIMP Palette` line, `Name:` and `Columns:` lines, `#` comments,
    /// then a color a line as three numbers and an optional name.
    pub fn parse(text: &str, path: PathBuf) -> Option<Palette> {
        let mut lines = text.lines();
        if lines.next()?.trim() != "GIMP Palette" {
            return None;
        }
        let mut name = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        let mut colors = Vec::new();
        for line in lines {
            let t = line.trim();
            if let Some(n) = t.strip_prefix("Name:") {
                name = n.trim().to_string();
                continue;
            }
            if t.is_empty() || t.starts_with('#') || t.starts_with("Columns:") {
                continue;
            }
            let mut parts = t.split_whitespace();
            let mut n = || parts.next().and_then(|p| p.parse::<u8>().ok());
            let (Some(r), Some(g), Some(b)) = (n(), n(), n()) else { continue };
            // The name is the words after the third number.
            colors.push((Rgb(r, g, b), t.split_whitespace().skip(3).collect::<Vec<_>>().join(" ")));
        }
        Some(Palette { name, colors, path })
    }

    pub fn text(&self) -> String {
        let mut out = format!("GIMP Palette\nName: {}\nColumns: 0\n#\n", self.name);
        for (c, n) in &self.colors {
            out.push_str(&format!("{:3} {:3} {:3}\t{}\n", c.0, c.1, c.2, if n.is_empty() { c.hex() } else { n.clone() }));
        }
        out
    }

    pub fn save(&mut self) -> Result<(), String> {
        if self.path.as_os_str().is_empty() {
            self.path = free_path(&self.name);
        }
        write(&self.path, &self.text())
    }

    /// Give it another name, and the file a name to match.
    pub fn rename(&mut self, name: &str) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() || name == self.name {
            return Ok(());
        }
        let old = std::mem::take(&mut self.path);
        self.name = name.to_string();
        self.save()?;
        if !old.as_os_str().is_empty() && old != self.path && !NO_SAVE.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(old);
        }
        Ok(())
    }

    pub fn delete(&self) {
        if !NO_SAVE.load(Ordering::Relaxed) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// A file for a palette of this name that isn't taken.
fn free_path(name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c == '/' || c.is_control() { '-' } else { c }).collect();
    let safe = if safe.trim().is_empty() || safe.starts_with('.') { "Palette".to_string() } else { safe.trim().to_string() };
    let dir = palettes_dir();
    let mut p = dir.join(format!("{safe}.gpl"));
    let mut n = 2;
    while p.exists() {
        p = dir.join(format!("{safe} {n}.gpl"));
        n += 1;
    }
    p
}

/// The saved palettes, by name.
pub fn load_palettes() -> Vec<Palette> {
    let mut out: Vec<Palette> = std::fs::read_dir(palettes_dir())
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "gpl"))
        .filter_map(|p| Palette::parse(&std::fs::read_to_string(&p).ok()?, p))
        .collect();
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// A name for a new palette that no saved one has: "Palette", "Palette 2"…
pub fn new_name(taken: &[Palette]) -> String {
    (1..).map(|n| if n == 1 { "Palette".to_string() } else { format!("Palette {n}") }).find(|n| !taken.iter().any(|p| p.name.eq_ignore_ascii_case(n))).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_gimp_palettes() {
        let text = "GIMP Palette\nName: Website\nColumns: 4\n# made by hand\n213  63 140\tpink\n  0 132 184 deep sea blue\n255 242 112\nnot a color\n";
        let p = Palette::parse(text, PathBuf::from("/x/web.gpl")).unwrap();
        assert_eq!(p.name, "Website");
        assert_eq!(p.colors, vec![(Rgb(213, 63, 140), "pink".into()), (Rgb(0, 132, 184), "deep sea blue".into()), (Rgb(255, 242, 112), String::new())]);
        let again = Palette::parse(&p.text(), p.path.clone()).unwrap();
        assert_eq!(again.name, "Website");
        assert_eq!(again.colors[0], p.colors[0]);
        // A color with no name is written with its code, as GIMP does.
        assert_eq!(again.colors[2].1, "#fff270");
        assert!(Palette::parse("not a palette", PathBuf::new()).is_none());
    }

    #[test]
    fn new_names_are_free() {
        let p = |n: &str| Palette { name: n.into(), colors: vec![], path: PathBuf::new() };
        assert_eq!(new_name(&[]), "Palette");
        assert_eq!(new_name(&[p("palette"), p("Palette 2")]), "Palette 3");
    }
}
