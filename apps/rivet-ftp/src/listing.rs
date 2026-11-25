//! What both panes list: a name, folder or not, size and date. Local folders are read directly;
//! server folders come from `sftp`'s `ls -la`.

use std::path::Path;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    /// As shown: "Nov 15 21:49", or "Nov 15  2024" for older files.
    pub modified: String,
    /// Seconds since 1970 (for sorting; to the minute for server files).
    pub mtime: i64,
    /// Permission bits (0o755), owner and group names, where known.
    pub mode: u32,
    pub owner: String,
    pub group: String,
}

/// Which column the list is sorted by.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortCol {
    #[default]
    Name,
    Size,
    Modified,
}

/// Sort column and direction (descending when true).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Sort {
    pub col: SortCol,
    pub desc: bool,
}

/// A local folder, sorted as asked. Hidden files only when asked.
pub fn read_local(dir: &Path, hidden: bool, order: Sort) -> Result<Vec<Entry>, String> {
    use std::collections::HashMap;
    use std::os::unix::fs::MetadataExt;
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let now = chrono::Local::now();
    // A folder's files nearly all share one owner: look each one up once, not once a file.
    let mut users: HashMap<u32, String> = HashMap::new();
    let mut groups: HashMap<u32, String> = HashMap::new();
    let mut out = Vec::new();
    for d in rd.flatten() {
        let name = d.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        let link = d.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let md = std::fs::metadata(d.path()).ok();
        let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
        let t = md.as_ref().and_then(|m| m.modified().ok());
        let modified = t.map(|t| fmt_time(t, now)).unwrap_or_default();
        let mtime = t.and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        let (mode, owner, group) = md
            .as_ref()
            .map(|m| {
                let owner = users.entry(m.uid()).or_insert_with(|| name_from("/etc/passwd", m.uid())).clone();
                let group = groups.entry(m.gid()).or_insert_with(|| name_from("/etc/group", m.gid())).clone();
                (m.mode() & 0o7777, owner, group)
            })
            .unwrap_or_default();
        out.push(Entry { name, is_dir, is_link: link, size: if is_dir { 0 } else { md.map(|m| m.len()).unwrap_or(0) }, modified, mtime, mode, owner, group });
    }
    sort_by(&mut out, order);
    Ok(out)
}

/// Folders first, then by the chosen column (name breaks ties).
pub fn sort_by(v: &mut [Entry], s: Sort) {
    v.sort_by(|a, b| {
        let by = match s.col {
            SortCol::Name => natural(&a.name, &b.name),
            SortCol::Size => a.size.cmp(&b.size).then_with(|| natural(&a.name, &b.name)),
            SortCol::Modified => a.mtime.cmp(&b.mtime).then_with(|| natural(&a.name, &b.name)),
        };
        b.is_dir.cmp(&a.is_dir).then(if s.desc { by.reverse() } else { by })
    });
}

/// "file2" before "file10", ignoring case.
fn natural(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut x, mut y) = (a.chars().peekable(), b.chars().peekable());
    loop {
        match (x.peek(), y.peek()) {
            (None, None) => return a.cmp(b),
            (None, _) => return std::cmp::Ordering::Less,
            (_, None) => return std::cmp::Ordering::Greater,
            (Some(c), Some(d)) if c.is_ascii_digit() && d.is_ascii_digit() => {
                let mut n = String::new();
                while let Some(c) = x.peek().filter(|c| c.is_ascii_digit()) {
                    n.push(*c);
                    x.next();
                }
                let mut m = String::new();
                while let Some(d) = y.peek().filter(|d| d.is_ascii_digit()) {
                    m.push(*d);
                    y.next();
                }
                let (n, m) = (n.trim_start_matches('0'), m.trim_start_matches('0'));
                let o = n.len().cmp(&m.len()).then_with(|| n.cmp(m));
                if o.is_ne() {
                    return o;
                }
            }
            (Some(c), Some(d)) => {
                let o = c.to_lowercase().cmp(d.to_lowercase());
                if o.is_ne() {
                    return o;
                }
                x.next();
                y.next();
            }
        }
    }
}

fn fmt_time(t: std::time::SystemTime, now: chrono::DateTime<chrono::Local>) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    if (now - dt).num_days() < 180 && dt <= now + chrono::Duration::days(1) {
        dt.format("%b %e %H:%M").to_string()
    } else {
        dt.format("%b %e  %Y").to_string()
    }
}

/// One line of `ls -la` from sftp, e.g.
/// `-rw-r--r--    ? me    me      1234 Nov 15 21:49 /srv/www/index.html`.
/// The name is given relative to `dir`.
pub fn parse_ls_line(line: &str, dir: &str) -> Option<Entry> {
    let mut rest = line;
    let mut fields = Vec::with_capacity(8);
    for _ in 0..8 {
        let t = rest.trim_start();
        let end = t.find(char::is_whitespace)?;
        fields.push(&t[..end]);
        rest = &t[end..];
    }
    // The name keeps its own spaces; only the single separator before it goes.
    let name = rest.strip_prefix(' ').unwrap_or(rest);
    let perms = fields[0];
    if perms.len() < 10 || !"-dlcbps".contains(&perms[..1]) {
        return None;
    }
    let size: u64 = fields[4].parse().ok()?;
    let prefix = if dir.ends_with('/') { dir.to_string() } else { format!("{dir}/") };
    let name = name.strip_prefix(&prefix).unwrap_or(name);
    let name = name.rsplit('/').next().unwrap_or(name).to_string();
    if name.is_empty() {
        return None;
    }
    let is_dir = perms.starts_with('d');
    let mtime = ls_time(fields[5], fields[6], fields[7]).unwrap_or(0);
    Some(Entry {
        name,
        is_dir,
        is_link: perms.starts_with('l'),
        size: if is_dir { 0 } else { size },
        modified: format!("{} {:>2} {:>5}", fields[5], fields[6], fields[7]),
        mtime,
        mode: mode_from(perms),
        owner: fields[2].to_string(),
        group: fields[3].to_string(),
    })
}

/// "rwxr-xr--" (after the type letter) → 0o754, with setuid/setgid/sticky.
pub fn mode_from(perms: &str) -> u32 {
    let b = perms.as_bytes();
    let at = |i: usize| b.get(i).copied().unwrap_or(b'-');
    let mut m = 0;
    for (i, bit) in [0o400, 0o200, 0o100, 0o040, 0o020, 0o010, 0o004, 0o002, 0o001].iter().enumerate() {
        // s and t also mean x; S and T mean the special bit without x.
        if matches!(at(i + 1), b'r' | b'w' | b'x' | b's' | b't') {
            m |= bit;
        }
    }
    for (i, bit) in [(3, 0o4000), (6, 0o2000), (9, 0o1000)] {
        if matches!(at(i), b's' | b'S' | b't' | b'T') {
            m |= bit;
        }
    }
    m
}

/// A user's or group's name from /etc/passwd or /etc/group (or the number).
fn name_from(file: &str, id: u32) -> String {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|t| t.lines().find_map(|l| {
            let mut f = l.split(':');
            let name = f.next()?;
            f.next();
            (f.next()?.parse::<u32>().ok()? == id).then(|| name.to_string())
        }))
        .unwrap_or_else(|| id.to_string())
}

/// `ls` dates: "Nov 15 21:49" (within the last half year) or "Nov 15 2024".
fn ls_time(month: &str, day: &str, time_or_year: &str) -> Option<i64> {
    use chrono::{Datelike, NaiveDate, TimeZone};
    const M: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let m = M.iter().position(|x| *x == month)? as u32 + 1;
    let d: u32 = day.parse().ok()?;
    let now = chrono::Local::now();
    let (year, h, min) = match time_or_year.split_once(':') {
        Some((h, mi)) => {
            let (h, mi): (u32, u32) = (h.parse().ok()?, mi.parse().ok()?);
            // Without a year it's within the last six months: a date ahead of today is last year's.
            let this = NaiveDate::from_ymd_opt(now.year(), m, d)?;
            let y = if this > now.date_naive() + chrono::Duration::days(1) { now.year() - 1 } else { now.year() };
            (y, h, mi)
        }
        None => (time_or_year.parse().ok()?, 0, 0),
    };
    let dt = NaiveDate::from_ymd_opt(year, m, d)?.and_hms_opt(h, min, 0)?;
    chrono::Local.from_local_datetime(&dt).earliest().map(|t| t.timestamp())
}

/// 1234567 → "1.2 MB"
pub fn human_size(n: u64) -> String {
    const U: [&str; 5] = ["bytes", "kB", "MB", "GB", "TB"];
    if n < 1000 {
        return format!("{n} {}", if n == 1 { "byte" } else { "bytes" });
    }
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1000.0 && i < U.len() - 1 {
        v /= 1000.0;
        i += 1;
    }
    format!("{v:.1} {}", U[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_local_folder_in_order() {
        let d = std::env::temp_dir().join(format!("rivet-ftp-listing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        for (name, len) in [("b10.txt", 1), ("b2.txt", 30), (".hidden", 5)] {
            std::fs::write(d.join(name), vec![b'x'; len]).unwrap();
        }
        let names = |v: &[Entry]| v.iter().map(|e| e.name.clone()).collect::<Vec<_>>();
        let v = read_local(&d, false, Sort::default()).unwrap();
        assert_eq!(names(&v), ["sub", "b2.txt", "b10.txt"]);
        // Every file has its owner's name, looked up once for the folder.
        assert!(v.iter().all(|e| !e.owner.is_empty() && e.owner == v[0].owner && !e.group.is_empty()));
        let v = read_local(&d, true, Sort { col: SortCol::Size, desc: true }).unwrap();
        assert_eq!(names(&v), ["sub", "b2.txt", ".hidden", "b10.txt"]);
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn parses_sftp_ls() {
        let e = parse_ls_line("-rw-r--r--    ? me       me              2 Nov 15 21:49 /srv/www/odd name [1].txt", "/srv/www").unwrap();
        assert_eq!((e.name.as_str(), e.is_dir, e.size), ("odd name [1].txt", false, 2));
        let d = parse_ls_line("drwxr-xr-x    ? me       me             60 Nov 15  2024 /srv/www/sub dir", "/srv/www/").unwrap();
        assert!(d.is_dir && d.name == "sub dir" && d.modified == "Nov 15  2024");
        assert_eq!(parse_ls_line("drwxr-xr-x  ? me me 100 Nov 15 21:49 /srv/www/.", "/srv/www").unwrap().name, ".");
        assert!(parse_ls_line("Can't ls: \"/x\" not found", "/").is_none());
        assert!(parse_ls_line("lrwxrwxrwx 1 me me 7 Jan 1 10:00 /a/link", "/a").unwrap().is_link);
    }

    #[test]
    fn natural_order_and_sizes() {
        let mut v: Vec<Entry> = ["file10", "File2", "dir"]
            .iter()
            .enumerate()
            .map(|(i, n)| Entry { name: n.to_string(), is_dir: *n == "dir", size: i as u64, mtime: 10 - i as i64, ..Default::default() })
            .collect();
        sort_by(&mut v, Sort::default());
        assert_eq!(v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["dir", "File2", "file10"]);
        sort_by(&mut v, Sort { col: SortCol::Size, desc: true });
        assert_eq!(v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["dir", "File2", "file10"], "folders stay first");
        sort_by(&mut v, Sort { col: SortCol::Modified, desc: false });
        assert_eq!(v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["dir", "File2", "file10"]);
        assert!(ls_time("Nov", "15", "2024").is_some() && ls_time("Foo", "1", "2024").is_none());
        assert_eq!(mode_from("-rw-r--r--"), 0o644);
        assert_eq!(mode_from("drwxr-x--x"), 0o751);
        assert_eq!(mode_from("-rwsr-xr-x"), 0o4755);
        assert_eq!(mode_from("drwxrwxrwt"), 0o1777);
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(1_234_567), "1.2 MB");
    }
}
