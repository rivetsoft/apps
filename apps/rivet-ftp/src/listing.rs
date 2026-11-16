//! What both panes list: a name, folder or not, size and date. Local folders are read directly;
//! server folders come from `sftp`'s `ls -la`.

use std::path::Path;

#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub is_dir: bool,
    pub is_link: bool,
    pub size: u64,
    /// As shown: "Nov 15 21:49", or "Nov 15  2024" for older files.
    pub modified: String,
}

/// A local folder, folders first then by name (natural order, ignoring case). Hidden files
/// only when asked.
pub fn read_local(dir: &Path, hidden: bool) -> Result<Vec<Entry>, String> {
    let rd = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    let mut out = Vec::new();
    for d in rd.flatten() {
        let name = d.file_name().to_string_lossy().into_owned();
        if !hidden && name.starts_with('.') {
            continue;
        }
        let link = d.file_type().map(|t| t.is_symlink()).unwrap_or(false);
        let md = std::fs::metadata(d.path()).ok();
        let is_dir = md.as_ref().is_some_and(|m| m.is_dir());
        let modified = md.as_ref().and_then(|m| m.modified().ok()).map(fmt_time).unwrap_or_default();
        out.push(Entry { name, is_dir, is_link: link, size: if is_dir { 0 } else { md.map(|m| m.len()).unwrap_or(0) }, modified });
    }
    sort(&mut out);
    Ok(out)
}

pub fn sort(v: &mut [Entry]) {
    v.sort_by(|a, b| b.is_dir.cmp(&a.is_dir).then_with(|| natural(&a.name, &b.name)));
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

fn fmt_time(t: std::time::SystemTime) -> String {
    let dt: chrono::DateTime<chrono::Local> = t.into();
    let now = chrono::Local::now();
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
    Some(Entry { name, is_dir, is_link: perms.starts_with('l'), size: if is_dir { 0 } else { size }, modified: format!("{} {:>2} {:>5}", fields[5], fields[6], fields[7]) })
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
        let mut v: Vec<Entry> = ["file10", "File2", "dir"].iter().map(|n| Entry { name: n.to_string(), is_dir: *n == "dir", is_link: false, size: 0, modified: String::new() }).collect();
        sort(&mut v);
        assert_eq!(v.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["dir", "File2", "file10"]);
        assert_eq!(human_size(1), "1 byte");
        assert_eq!(human_size(1_234_567), "1.2 MB");
    }
}
