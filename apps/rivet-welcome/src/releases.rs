//! The Rivet apps' releases, from GitHub: every app is released from the one repository and told
//! apart by its tag (`v0.7.0` for Rivet Files, `rivet-notes-v0.2.0`…). The list is fetched with
//! `curl` (no TLS stack in this program) and kept in the cache folder, so the window and the
//! tray share one answer and the window has something to show before the network does.

use std::cmp::Ordering;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::SystemTime;

use serde_json::Value;

/// Where the apps are released.
pub const REPO: &str = "rivetsoft/apps";

/// The repository to look in (`RIVET_WELCOME_REPO=owner/name` for testing a fork).
pub fn repo() -> String {
    std::env::var("RIVET_WELCOME_REPO")
        .ok()
        .filter(|r| r.contains('/'))
        .unwrap_or_else(|| REPO.to_string())
}

/// A version number: numbers separated by dots, and an optional pre-release suffix
/// (`0.2.0-beta.1`), which comes before the plain number.
#[derive(Clone, Debug, Default)]
pub struct Version {
    pub nums: Vec<u64>,
    pub pre: Option<String>,
}

impl Version {
    /// `0.7.0`, `v0.7.0`, `0.2.0-beta.1`. None for anything else.
    pub fn parse(s: &str) -> Option<Version> {
        let s = s.trim().trim_start_matches('v');
        // Build metadata (`+abc`) doesn't change which version it is.
        let s = s.split_once('+').map_or(s, |(v, _)| v);
        let (main, pre) = match s.split_once('-') {
            Some((_, "")) => return None,
            Some((m, p)) => (m, Some(p.to_string())),
            None => (s, None),
        };
        if main.is_empty() {
            return None;
        }
        let nums = main
            .split('.')
            .map(|n| n.parse::<u64>().ok())
            .collect::<Option<Vec<_>>>()?;
        Some(Version { nums, pre })
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let main: Vec<String> = self.nums.iter().map(u64::to_string).collect();
        write!(f, "{}", main.join("."))?;
        if let Some(p) = &self.pre {
            write!(f, "-{p}")?;
        }
        Ok(())
    }
}

impl Ord for Version {
    fn cmp(&self, other: &Self) -> Ordering {
        let n = self.nums.len().max(other.nums.len());
        for i in 0..n {
            let (a, b) = (
                self.nums.get(i).copied().unwrap_or(0),
                other.nums.get(i).copied().unwrap_or(0),
            );
            if a != b {
                return a.cmp(&b);
            }
        }
        match (&self.pre, &other.pre) {
            (None, None) => Ordering::Equal,
            (None, Some(_)) => Ordering::Greater,
            (Some(_), None) => Ordering::Less,
            (Some(a), Some(b)) => pre_cmp(a, b),
        }
    }
}

/// Pre-release suffixes compare part by part, as semver does: `beta.10` after `beta.2`, and a
/// number before a word (`1` before `alpha`).
fn pre_cmp(a: &str, b: &str) -> Ordering {
    let (mut x, mut y) = (a.split('.'), b.split('.'));
    loop {
        match (x.next(), y.next()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(p), Some(q)) => {
                let o = match (p.parse::<u64>(), q.parse::<u64>()) {
                    (Ok(m), Ok(n)) => m.cmp(&n),
                    (Ok(_), Err(_)) => Ordering::Less,
                    (Err(_), Ok(_)) => Ordering::Greater,
                    (Err(_), Err(_)) => p.cmp(q),
                };
                if o != Ordering::Equal {
                    return o;
                }
            }
        }
    }
}

/// 1.0 and 1.0.0 are the same version.
impl PartialEq for Version {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Version {}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Asset {
    pub name: String,
    pub url: String,
    pub size: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub tag: String,
    /// "Rivet Notes 0.2.0".
    pub name: String,
    pub prerelease: bool,
    /// ISO 8601, as GitHub gives it.
    pub published: String,
    /// The release's page.
    pub url: String,
    pub notes: String,
    pub assets: Vec<Asset>,
}

/// GitHub's releases list (a JSON array) as releases, leaving out drafts.
pub fn parse(json: &str) -> Result<Vec<Release>, String> {
    let v: Value = serde_json::from_str(json)
        .map_err(|e| format!("the releases list didn't make sense ({e})"))?;
    let Some(list) = v.as_array() else {
        // An error from GitHub is an object with a message (the rate limit, mostly).
        let msg = v
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("not a list");
        return Err(format!("GitHub said: {msg}"));
    };
    let s = |r: &Value, k: &str| {
        r.get(k)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    Ok(list
        .iter()
        .filter(|r| !r.get("draft").and_then(Value::as_bool).unwrap_or(false))
        .map(|r| Release {
            tag: s(r, "tag_name"),
            name: s(r, "name"),
            prerelease: r
                .get("prerelease")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            published: s(r, "published_at"),
            url: s(r, "html_url"),
            notes: s(r, "body"),
            assets: r
                .get("assets")
                .and_then(Value::as_array)
                .map(|a| {
                    a.iter()
                        .map(|x| Asset {
                            name: s(x, "name"),
                            url: s(x, "browser_download_url"),
                            size: x.get("size").and_then(Value::as_u64).unwrap_or(0),
                        })
                        .collect()
                })
                .unwrap_or_default(),
        })
        .filter(|r| !r.tag.is_empty())
        .collect())
}

fn cache_file() -> PathBuf {
    rivet_xdg::places::cache_home().join("rivet-welcome/releases.json")
}

/// GitHub's tag for the cached list: asking again with it costs nothing when nothing changed.
fn etag_file() -> PathBuf {
    rivet_xdg::places::cache_home().join("rivet-welcome/releases.etag")
}

/// The list fetched last time, and when.
pub fn load_cached() -> Option<(Vec<Release>, SystemTime)> {
    let p = cache_file();
    let when = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
    let text = std::fs::read_to_string(&p).ok()?;
    Some((parse(&text).ok()?, when))
}

/// When the cached list last changed (the tray watches this for checks made in the window).
pub fn cache_stamp() -> Option<SystemTime> {
    std::fs::metadata(cache_file())
        .and_then(|m| m.modified())
        .ok()
}

/// Where GitHub's API is (`RIVET_WELCOME_API=http://127.0.0.1:8000` points at a test server).
fn api_base() -> String {
    std::env::var("RIVET_WELCOME_API")
        .ok()
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .map(|u| u.trim_end_matches('/').to_string())
        .unwrap_or_else(|| "https://api.github.com".to_string())
}

/// Pages of releases read at most: 100 a page, newest first, and the apps are released in
/// rounds, so the newest release of every app is well inside this.
const MAX_PAGES: usize = 5;

/// One answer from the API.
#[derive(Debug, Default, PartialEq)]
struct Answer {
    status: u16,
    body: String,
    etag: Option<String>,
    /// The next page, from the Link header.
    next: Option<String>,
    /// `x-ratelimit-remaining` and `x-ratelimit-reset` (seconds since 1970).
    remaining: Option<u64>,
    reset: Option<u64>,
}

/// The status, headers and body curl wrote: the headers of the last response (after any
/// redirects), then the body in its own file.
fn read_answer(head: &str, body: String) -> Answer {
    let block = head
        .split("\r\n\r\n")
        .flat_map(|b| b.split("\n\n"))
        .filter(|b| b.trim_start().starts_with("HTTP/"))
        .last()
        .unwrap_or("");
    let mut a = Answer {
        body,
        ..Answer::default()
    };
    for (i, line) in block.lines().enumerate() {
        let line = line.trim_end();
        if i == 0 {
            a.status = line
                .split_whitespace()
                .nth(1)
                .and_then(|c| c.parse().ok())
                .unwrap_or(0);
            continue;
        }
        let Some((k, v)) = line.split_once(':') else {
            continue;
        };
        let v = v.trim();
        match k.trim().to_ascii_lowercase().as_str() {
            "etag" => a.etag = Some(v.to_string()),
            "x-ratelimit-remaining" => a.remaining = v.parse().ok(),
            "x-ratelimit-reset" => a.reset = v.parse().ok(),
            "link" => {
                a.next = v
                    .split(',')
                    .find(|l| l.contains("rel=\"next\""))
                    .and_then(|l| Some(l.split_once('<')?.1.split_once('>')?.0.to_string()))
            }
            _ => {}
        }
    }
    a
}

/// What a failed curl means for someone waiting on it.
pub fn curl_failure(code: Option<i32>, stderr: &str, what: &str) -> String {
    match code {
        Some(6) => format!("couldn't reach {what}: this computer seems to be offline"),
        Some(7) => format!("couldn't connect to {what}; try again in a while"),
        Some(28) => format!("{what} took too long to answer; try again in a while"),
        Some(35 | 60) => format!("couldn't make a secure connection to {what}"),
        _ => {
            let line = stderr
                .lines()
                .last()
                .unwrap_or("")
                .trim()
                .trim_start_matches("curl: ")
                .to_string();
            if line.is_empty() {
                format!("couldn't reach {what}")
            } else {
                format!("couldn't reach {what} ({line})")
            }
        }
    }
}

/// `curl` couldn't be started at all.
pub fn no_curl(e: std::io::Error) -> String {
    if e.kind() == std::io::ErrorKind::NotFound {
        "curl isn't installed, and Rivet Welcome needs it to download: install curl".to_string()
    } else {
        format!("couldn't run curl: {e}")
    }
}

/// Ask the API for one page. With `etag`, an unchanged list comes back as 304 and empty.
fn get(url: &str, etag: Option<&str>) -> Result<Answer, String> {
    let dir = rivet_xdg::places::cache_home().join("rivet-welcome");
    let _ = std::fs::create_dir_all(&dir);
    let body_file = dir.join(format!("answer-{}.part", std::process::id()));
    let mut cmd = Command::new("curl");
    cmd.args([
        "-sSL",
        "--connect-timeout",
        "15",
        "--max-time",
        "30",
        "--retry",
        "2",
        "-D",
        "-",
        "-H",
        "Accept: application/vnd.github+json",
        "-H",
        "X-GitHub-Api-Version: 2022-11-28",
        "-A",
        concat!("rivet-welcome/", env!("CARGO_PKG_VERSION")),
    ]);
    if let Some(t) = etag {
        cmd.arg("-H").arg(format!("If-None-Match: {t}"));
    }
    // A token, when there is one, lifts the limit from 60 checks an hour to 5000. It only ever
    // goes to GitHub itself.
    if url.starts_with("https://api.github.com/")
        && let Some(t) = ["RIVET_WELCOME_GITHUB_TOKEN", "GITHUB_TOKEN"]
            .iter()
            .find_map(|k| std::env::var(k).ok().filter(|t| !t.trim().is_empty()))
    {
        cmd.arg("-H")
            .arg(format!("Authorization: Bearer {}", t.trim()));
    }
    let out = cmd
        .arg("-o")
        .arg(&body_file)
        .arg(url)
        .stdin(Stdio::null())
        .output()
        .map_err(no_curl)?;
    let body = std::fs::read_to_string(&body_file).unwrap_or_default();
    let _ = std::fs::remove_file(&body_file);
    if !out.status.success() {
        return Err(curl_failure(
            out.status.code(),
            &String::from_utf8_lossy(&out.stderr),
            "GitHub",
        ));
    }
    Ok(read_answer(&String::from_utf8_lossy(&out.stdout), body))
}

/// Why GitHub said no, in words, with when to try again.
fn refusal(a: &Answer, now: u64) -> String {
    let limited = a.status == 429 || (a.status == 403 && a.remaining == Some(0));
    if !limited {
        let msg = serde_json::from_str::<Value>(&a.body)
            .ok()
            .and_then(|v| v.get("message").and_then(Value::as_str).map(String::from));
        return match (a.status, msg) {
            (404, _) => format!("GitHub has no releases for {}", repo()),
            (s, Some(m)) => format!("GitHub answered {s}: {m}"),
            (s, None) => format!("GitHub answered {s}; try again in a while"),
        };
    }
    let wait = a
        .reset
        .map(|r| r.saturating_sub(now))
        .map(|s| match s.div_ceil(60) {
            0 | 1 => "in a minute".to_string(),
            m if m < 90 => format!("in {m} minutes"),
            m => format!("in {} hours", m.div_ceil(60)),
        })
        .unwrap_or_else(|| "in an hour".to_string());
    format!("GitHub's limit on checks from this computer is used up for now; try again {wait}")
}

/// Every page of releases (up to `MAX_PAGES`) as one JSON array, or None when the first page
/// hasn't changed since `etag`.
fn fetch_remote(etag: Option<&str>) -> Result<Option<(String, Option<String>)>, String> {
    let mut url = format!("{}/repos/{}/releases?per_page=100", api_base(), repo());
    let mut all: Vec<Value> = Vec::new();
    let mut first_etag = None;
    for page in 0..MAX_PAGES {
        let a = get(&url, if page == 0 { etag } else { None })?;
        if page == 0 && a.status == 304 {
            return Ok(None);
        }
        if a.status != 200 {
            let now = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            return Err(refusal(&a, now));
        }
        // Checked one page at a time, so a page that isn't a list says what GitHub said.
        parse(&a.body)?;
        match serde_json::from_str::<Value>(&a.body) {
            Ok(Value::Array(list)) => all.extend(list),
            _ => return Err("the releases list didn't make sense".into()),
        }
        if page == 0 {
            first_etag = a.etag.clone();
        }
        match a.next {
            Some(n) => url = n,
            None => break,
        }
    }
    Ok(Some((Value::Array(all).to_string(), first_etag)))
}

/// Write a file so a reader never sees half of it, even with two Rivet Welcomes at once.
fn write_whole(p: &std::path::Path, text: &str) {
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = p.with_extension(format!("part-{}", std::process::id()));
    if std::fs::write(&tmp, text).is_ok() && std::fs::rename(&tmp, p).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

/// Look for releases now (blocking: run it off the UI thread), and keep the answer. The list is
/// asked for with the last answer's tag, so a check that finds nothing new is free and quick.
/// `RIVET_WELCOME_RELEASES=FILE` reads the list from a file instead, for testing.
pub fn check() -> Result<Vec<Release>, String> {
    if let Ok(f) = std::env::var("RIVET_WELCOME_RELEASES") {
        let json = std::fs::read_to_string(&f).map_err(|e| format!("couldn't read {f}: {e}"))?;
        let list = parse(&json)?;
        write_whole(&cache_file(), &json);
        return Ok(list);
    }
    let cached = load_cached();
    let etag = cached
        .as_ref()
        .and_then(|_| std::fs::read_to_string(etag_file()).ok())
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty());
    match fetch_remote(etag.as_deref())? {
        None => {
            // Unchanged: the cached list is current as of now.
            if let Ok(f) = std::fs::File::options().append(true).open(cache_file()) {
                let _ = f.set_modified(SystemTime::now());
            }
            Ok(cached.map(|(l, _)| l).unwrap_or_default())
        }
        Some((json, tag)) => {
            let list = parse(&json)?;
            write_whole(&cache_file(), &json);
            match tag {
                Some(t) => write_whole(&etag_file(), &t),
                None => {
                    let _ = std::fs::remove_file(etag_file());
                }
            }
            Ok(list)
        }
    }
}

/// "5 minutes ago".
pub fn ago(when: SystemTime) -> String {
    let secs = SystemTime::now()
        .duration_since(when)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let plural = |n: u64, w: &str| {
        if n == 1 {
            format!("1 {w} ago")
        } else {
            format!("{n} {w}s ago")
        }
    };
    match secs {
        0..60 => "just now".into(),
        60..3600 => plural(secs / 60, "minute"),
        3600..86_400 => plural(secs / 3600, "hour"),
        _ => plural(secs / 86_400, "day"),
    }
}

/// A date from GitHub's time stamp: "2026-11-17".
pub fn day(published: &str) -> &str {
    published.get(..10).unwrap_or(published)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare() {
        let v = |s| Version::parse(s).unwrap();
        assert!(v("0.10.0") > v("0.9.3"));
        assert!(v("v1.0") == v("1.0.0"));
        assert!(v("0.2.0") > v("0.2.0-beta.1"));
        assert!(v("0.2.0-beta.2") > v("0.2.0-beta.1"));
        assert_eq!(v("0.2.0-rc.1").to_string(), "0.2.0-rc.1");
        assert!(Version::parse("x1").is_none());
        assert!(Version::parse("").is_none());
    }

    #[test]
    fn prerelease_order() {
        let v = |s| Version::parse(s).unwrap();
        // Numbers in a suffix compare as numbers, not text.
        assert!(v("0.4.0-beta.10") > v("0.4.0-beta.2"));
        // Fewer parts come first, a number before a word, and words in order.
        assert!(v("1.0.0-alpha") < v("1.0.0-alpha.1"));
        assert!(v("1.0.0-alpha.1") < v("1.0.0-alpha.beta"));
        assert!(v("1.0.0-beta") < v("1.0.0-rc.1"));
        assert!(v("1.0.0-rc.1") < v("1.0.0"));
        assert!(v("0.3.1") < v("0.4.0-beta.1"));
        // Build metadata doesn't change the version; an empty suffix isn't one.
        assert!(v("0.4.0+abc") == v("0.4.0"));
        assert!(Version::parse("0.4.0-").is_none());
        assert!(Version::parse("0.4.x").is_none());
    }

    #[test]
    fn answers_are_read() {
        let head = "HTTP/2 302\r\nlocation: x\r\n\r\nHTTP/2 200\r\nETag: W/\"abc\"\r\nx-ratelimit-remaining: 41\r\nx-ratelimit-reset: 1795746165\r\nlink: <https://api.github.com/repositories/1/releases?per_page=100&page=2>; rel=\"next\", <https://api.github.com/repositories/1/releases?per_page=100&page=3>; rel=\"last\"\r\n\r\n";
        let a = read_answer(head, "[]".into());
        assert_eq!(a.status, 200);
        assert_eq!(a.etag.as_deref(), Some("W/\"abc\""));
        assert_eq!((a.remaining, a.reset), (Some(41), Some(1795746165)));
        assert_eq!(
            a.next.as_deref(),
            Some("https://api.github.com/repositories/1/releases?per_page=100&page=2")
        );
        let a = read_answer("HTTP/1.0 304 Not Modified\n\n", String::new());
        assert_eq!((a.status, a.next), (304, None));
    }

    #[test]
    fn refusals_say_when_to_try_again() {
        let limited = Answer {
            status: 403,
            remaining: Some(0),
            reset: Some(1_000 + 23 * 60),
            body: r#"{"message":"API rate limit exceeded for 1.2.3.4."}"#.into(),
            ..Answer::default()
        };
        assert!(refusal(&limited, 1_000).ends_with("try again in 23 minutes"));
        assert!(
            refusal(
                &Answer {
                    status: 429,
                    ..Answer::default()
                },
                0
            )
            .contains("in an hour")
        );
        // A 403 with checks left is some other refusal, said as GitHub said it.
        let other = Answer {
            status: 403,
            remaining: Some(30),
            body: r#"{"message":"Repository access blocked"}"#.into(),
            ..Answer::default()
        };
        assert_eq!(
            refusal(&other, 0),
            "GitHub answered 403: Repository access blocked"
        );
    }

    #[test]
    fn parses_releases() {
        let list = parse(include_str!("../data/demo-releases.json")).unwrap();
        assert!(list.iter().any(|r| r.tag == "v0.7.0" && !r.prerelease));
        assert!(list.iter().all(|r| r.tag != "rivet-draft-v9.0.0"));
        assert!(
            parse(r#"{"message":"API rate limit exceeded"}"#)
                .unwrap_err()
                .contains("rate limit")
        );
    }
}
