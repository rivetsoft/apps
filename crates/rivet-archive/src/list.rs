//! What is in an archive, and taking only some of it out.

use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::{archive_ext, has, run, seven_zip, stem};

/// One file or folder in an archive.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Entry {
    /// Where it is in the archive, with `/` between folders and none at either end.
    pub path: String,
    pub dir: bool,
    /// Its size unpacked, in bytes.
    pub size: u64,
    /// When it was last changed: seconds since 1970 by the clock the archive tool printed
    /// (local time, taken as it reads), or 0 when the tool doesn't say.
    pub modified: i64,
}

impl Entry {
    /// Its own name, without the folders it is in.
    pub fn name(&self) -> &str {
        self.path.rsplit('/').next().unwrap_or(&self.path)
    }

    /// The folder it is in ("" at the top).
    pub fn parent(&self) -> &str {
        self.path.rsplit_once('/').map_or("", |(p, _)| p)
    }
}

/// Which tool reads this archive.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Tool {
    Tar,
    SevenZ(&'static str),
    Unzip,
    Bsdtar,
    /// One compressed file, not an archive of several.
    Single,
}

fn tool_for(archive: &Path) -> Result<Tool, String> {
    let name = archive.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
    let ext = archive_ext(&name).ok_or("This is not an archive Rivet knows")?;
    if matches!(ext, ".gz" | ".xz" | ".bz2" | ".zst" | ".lz" | ".lzma") {
        return Ok(Tool::Single);
    }
    let is_tar = ext.starts_with(".tar") || matches!(ext, ".tgz" | ".txz" | ".tbz" | ".tbz2" | ".tzst");
    if is_tar && has("tar") {
        Ok(Tool::Tar)
    } else if let Some(z) = seven_zip() {
        Ok(Tool::SevenZ(z))
    } else if matches!(ext, ".zip" | ".jar") && has("unzip") {
        Ok(Tool::Unzip)
    } else if has("bsdtar") {
        Ok(Tool::Bsdtar)
    } else {
        Err("No tool to open this kind of archive is installed (try installing 7-Zip)".into())
    }
}

/// Run a tool and give what it printed, stopping it when asked to.
fn output(mut cmd: Command, cancel: &AtomicBool) -> Result<String, String> {
    use std::io::Read;
    // Dates and sizes as plain numbers, whatever language the desktop is in.
    cmd.env("LC_ALL", "C").stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("Could not start the archive tool: {e}"))?;
    let (out, err) = (child.stdout.take(), child.stderr.take());
    let read = |pipe: Option<Box<dyn Read + Send>>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut p) = pipe {
                let _ = p.read_to_end(&mut bytes);
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let out = read(out.map(|o| Box::new(o) as Box<dyn Read + Send>));
    let err = read(err.map(|e| Box::new(e) as Box<dyn Read + Send>));
    loop {
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            let (text, said) = (out.join().unwrap_or_default(), err.join().unwrap_or_default());
            return if status.success() { Ok(text) } else { Err(said.lines().map(str::trim).rfind(|l| !l.is_empty()).unwrap_or("The archive tool failed").to_string()) };
        }
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Cancelled".into());
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}

/// Everything in `archive`: files and folders, every folder a file is in included, sorted by
/// path.
pub fn list(archive: &Path, cancel: &AtomicBool) -> Result<Vec<Entry>, String> {
    let mut entries = match tool_for(archive)? {
        Tool::Single => vec![Entry { path: stem(archive), ..Default::default() }],
        Tool::Tar => {
            let mut cmd = Command::new("tar");
            cmd.args(["--full-time", "-tvf"]).arg(archive);
            output(cmd, cancel)?.lines().filter_map(tar_line).collect()
        }
        Tool::SevenZ(z) => {
            let mut cmd = Command::new(z);
            cmd.args(["l", "-slt", "-ba", "-sccUTF-8"]).arg(archive);
            seven_zip_entries(&output(cmd, cancel)?)
        }
        Tool::Unzip => {
            let mut cmd = Command::new("unzip");
            cmd.arg("-l").arg(archive);
            output(cmd, cancel)?.lines().filter_map(unzip_line).collect()
        }
        Tool::Bsdtar => {
            let mut cmd = Command::new("bsdtar");
            cmd.arg("-tvf").arg(archive);
            output(cmd, cancel)?.lines().filter_map(ls_line).collect()
        }
    };
    Ok(tidy(&mut entries))
}

/// Clean paths, one entry each, and the folders nothing listed but a file is in.
fn tidy(entries: &mut Vec<Entry>) -> Vec<Entry> {
    use std::collections::BTreeMap;
    let mut all: BTreeMap<String, Entry> = BTreeMap::new();
    for mut e in entries.drain(..) {
        let ends_as_folder = e.path.ends_with('/');
        e.path = e.path.split('/').filter(|p| !p.is_empty() && *p != ".").collect::<Vec<_>>().join("/");
        if e.path.is_empty() {
            continue;
        }
        e.dir |= ends_as_folder;
        let mut parent = e.parent().to_string();
        while !parent.is_empty() {
            all.entry(parent.clone()).or_insert_with(|| Entry { path: parent.clone(), dir: true, ..Default::default() });
            parent = parent.rsplit_once('/').map_or(String::new(), |(p, _)| p.to_string());
        }
        // A folder made up from a file's path gives way to the archive's own word on it.
        let made_up = all.get(&e.path).is_some_and(|old| old.dir && old.modified == 0);
        if made_up || !all.contains_key(&e.path) {
            all.insert(e.path.clone(), e);
        }
    }
    all.into_values().collect()
}

/// The first `n` words of a line, and what follows them (which may hold spaces of its own).
fn words(line: &str, n: usize) -> Option<(Vec<&str>, &str)> {
    let mut rest = line.trim_start();
    let mut out = Vec::with_capacity(n);
    for _ in 0..n {
        let end = rest.find(char::is_whitespace)?;
        out.push(&rest[..end]);
        rest = rest[end..].trim_start();
    }
    (!rest.is_empty()).then_some((out, rest))
}

/// "2026-11-24" and "00:10:22" (or "00:10") as seconds since 1970, reading the clock as it is.
fn stamp(date: &str, time: &str) -> i64 {
    let mut d = date.split(['-', '/']).filter_map(|p| p.parse::<i64>().ok());
    let (Some(a), Some(b), Some(c)) = (d.next(), d.next(), d.next()) else { return 0 };
    // Year first, as most tools print it; some older unzip builds put it last.
    let (y, m, day) = if a > 31 { (a, b, c) } else { (c, a, b) };
    let mut t = time.split([':', '.']).filter_map(|p| p.parse::<i64>().ok());
    let (h, min, s) = (t.next().unwrap_or(0), t.next().unwrap_or(0), t.next().unwrap_or(0));
    // Days since 1970 for a date in the calendar in use everywhere now.
    let (y2, m2) = if m <= 2 { (y - 1, m + 9) } else { (y, m - 3) };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * m2 + 2) / 5 + day - 1;
    let days = era * 146097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719468;
    days * 86400 + h * 3600 + min * 60 + s
}

/// GNU tar's `--full-time -tv`: "-rw-r--r-- sam/sam 5 2026-11-24 00:10:22 Folder/a.txt".
fn tar_line(line: &str) -> Option<Entry> {
    let (w, name) = words(line, 5)?;
    let kind = w[0].chars().next()?;
    // A link's line ends with where it points.
    let name = match kind {
        'l' => name.split(" -> ").next().unwrap_or(name),
        'h' => name.split(" link to ").next().unwrap_or(name),
        _ => name,
    };
    Some(Entry { path: name.to_string(), dir: kind == 'd', size: w[2].parse().unwrap_or(0), modified: stamp(w[3], w[4]) })
}

/// 7-Zip's `l -slt`: a block of "Key = value" lines for each entry, a blank line between.
fn seven_zip_entries(text: &str) -> Vec<Entry> {
    let mut out = Vec::new();
    let mut cur: Option<Entry> = None;
    for line in text.lines().chain([""]) {
        let Some((key, value)) = line.split_once(" = ") else {
            if line.trim().is_empty() {
                out.extend(cur.take());
            }
            continue;
        };
        match key.trim() {
            // A new block starts at its path.
            "Path" => {
                out.extend(cur.take());
                cur = Some(Entry { path: value.to_string(), ..Default::default() });
            }
            "Size" => {
                if let Some(e) = cur.as_mut() {
                    e.size = value.trim().parse().unwrap_or(0);
                }
            }
            "Modified" => {
                if let (Some(e), Some((d, t))) = (cur.as_mut(), value.trim().split_once(' ')) {
                    e.modified = stamp(d, t);
                }
            }
            "Folder" if value.trim() == "+" => {
                if let Some(e) = cur.as_mut() {
                    e.dir = true;
                }
            }
            "Attributes" if value.trim_start().starts_with('D') => {
                if let Some(e) = cur.as_mut() {
                    e.dir = true;
                }
            }
            _ => {}
        }
    }
    out
}

/// unzip's `-l`: "        5  2026-11-24 00:10   Folder/a.txt", between a heading and a total.
fn unzip_line(line: &str) -> Option<Entry> {
    let (w, name) = words(line, 3)?;
    let size = w[0].parse::<u64>().ok()?;
    if !w[1].contains(['-', '/']) || !w[2].contains(':') {
        return None;
    }
    Some(Entry { path: name.to_string(), dir: name.ends_with('/'), size, modified: stamp(w[1], w[2]) })
}

/// A listing like `ls -l` (bsdtar): the size is the fifth word and the name follows three
/// words of date, whose form depends on how old the file is, so the date is left out.
fn ls_line(line: &str) -> Option<Entry> {
    let (w, name) = words(line, 8)?;
    let name = if w[0].starts_with('l') { name.split(" -> ").next().unwrap_or(name) } else { name };
    Some(Entry { path: name.to_string(), dir: w[0].starts_with('d'), size: w[4].parse().unwrap_or(0), modified: 0 })
}

/// Take only these out of `archive` (paths as `list` gives them; a folder brings what is in
/// it), into the folder `into`, keeping the folders they are in.
pub fn extract_some(archive: &Path, paths: &[String], into: &Path, cancel: &AtomicBool) -> Result<(), String> {
    if paths.is_empty() {
        return Ok(());
    }
    std::fs::create_dir_all(into).map_err(|e| format!("Could not write here: {e}"))?;
    let mut cmd;
    match tool_for(archive)? {
        Tool::Single => return Err("This file holds one thing: extract all of it".into()),
        Tool::Tar => {
            cmd = Command::new("tar");
            // No wildcards: a name with a star in it means itself.
            cmd.arg("-xf").arg(archive).arg("-C").arg(into).arg("--no-wildcards").arg("--").args(paths);
        }
        Tool::SevenZ(z) => {
            cmd = Command::new(z);
            cmd.args(["x", "-bd", "-y", "-spd"]).arg(format!("-o{}", into.display())).arg(archive).arg("--").args(paths);
        }
        Tool::Unzip => {
            cmd = Command::new("unzip");
            cmd.args(["-q", "-o"]).arg(archive);
            // unzip takes patterns: a folder is itself and everything under it.
            for p in paths {
                let plain: String = p.chars().flat_map(|c| if "*?[]\\".contains(c) { vec!['\\', c] } else { vec![c] }).collect();
                cmd.arg(&plain).arg(format!("{plain}/*"));
            }
            cmd.arg("-d").arg(into);
        }
        Tool::Bsdtar => {
            cmd = Command::new("bsdtar");
            cmd.arg("-xf").arg(archive).arg("-C").arg(into).arg("--").args(paths);
        }
    }
    run(cmd, cancel)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_tars_lines() {
        let e = tar_line("-rw-r--r-- sam/sam          5 2026-11-24 00:10:22 Folder/two words.txt").unwrap();
        assert_eq!((e.path.as_str(), e.size, e.dir), ("Folder/two words.txt", 5, false));
        assert_eq!(e.modified, stamp("2026-11-24", "00:10:22"));
        assert!(tar_line("drwxr-xr-x sam/sam          0 2026-11-24 00:10:22 Folder/").unwrap().dir);
        assert_eq!(tar_line("lrwxrwxrwx sam/sam          0 2026-11-24 00:10:22 a -> b/c").unwrap().path, "a");
        assert_eq!(stamp("1970-01-02", "00:00:01"), 86401);
        assert_eq!(stamp("2026-11-24", "00:10:22"), 1795479022);
        // An older unzip's month-day-year reads the same.
        assert_eq!(stamp("11-24-2026", "00:10"), stamp("2026-11-24", "00:10:00"));
    }

    #[test]
    fn reads_seven_zips_blocks() {
        let l = seven_zip_entries("Path = Folder\nFolder = +\nSize = 0\nModified = 2026-11-24 00:10:22\nAttributes = D drwxr-xr-x\n\nPath = Folder/a = b.txt\nFolder = -\nSize = 5\nModified = 2026-11-24 00:10:22.1234567\nAttributes = A -rw-r--r--\n");
        assert_eq!(l.len(), 2);
        assert!(l[0].dir && !l[1].dir);
        assert_eq!((l[1].path.as_str(), l[1].size), ("Folder/a = b.txt", 5));
        assert_eq!(l[1].modified, stamp("2026-11-24", "00:10:22"));
    }

    #[test]
    fn reads_unzips_lines() {
        assert!(unzip_line("  Length      Date    Time    Name").is_none());
        assert!(unzip_line("---------  ---------- -----   ----").is_none());
        assert!(unzip_line("       11                     2 files").is_none());
        let e = unzip_line("        5  2026-11-24 00:10   Folder/two words.txt").unwrap();
        assert_eq!((e.path.as_str(), e.size), ("Folder/two words.txt", 5));
    }

    #[test]
    fn folders_are_filled_in() {
        let mut raw = vec![Entry { path: "./a/b/c.txt".into(), size: 3, modified: 9, ..Default::default() }, Entry { path: "a/".into(), modified: 7, ..Default::default() }];
        let l = tidy(&mut raw);
        assert_eq!(l.iter().map(|e| (e.path.as_str(), e.dir)).collect::<Vec<_>>(), [("a", true), ("a/b", true), ("a/b/c.txt", false)]);
        // The archive's own entry for a folder wins over the one made up from a path.
        assert_eq!(l[0].modified, 7);
        assert_eq!((l[2].name(), l[2].parent()), ("c.txt", "a/b"));
    }
}
