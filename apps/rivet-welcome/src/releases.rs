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
        let (main, pre) = match s.split_once('-') {
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
            (Some(a), Some(b)) => a.cmp(b),
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

/// The JSON, from GitHub (or from `RIVET_WELCOME_RELEASES=FILE`, for testing).
fn fetch() -> Result<String, String> {
    if let Ok(f) = std::env::var("RIVET_WELCOME_RELEASES") {
        return std::fs::read_to_string(&f).map_err(|e| format!("couldn't read {f}: {e}"));
    }
    let url = format!(
        "https://api.github.com/repos/{}/releases?per_page=100",
        repo()
    );
    let out = Command::new("curl")
        .args([
            "-fsSL",
            "--max-time",
            "30",
            "--retry",
            "2",
            "-H",
            "Accept: application/vnd.github+json",
            "-A",
            concat!("rivet-welcome/", env!("CARGO_PKG_VERSION")),
            &url,
        ])
        .stdin(Stdio::null())
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "curl isn't installed (it's used to download)".to_string()
            } else {
                format!("couldn't run curl: {e}")
            }
        })?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        let line = err
            .lines()
            .last()
            .unwrap_or("")
            .trim()
            .trim_start_matches("curl: ")
            .to_string();
        return Err(if line.contains("403") || line.contains("429") {
            "GitHub is busy just now (too many checks); try again in a while".to_string()
        } else if line.is_empty() {
            "couldn't reach GitHub".to_string()
        } else {
            format!("couldn't reach GitHub ({line})")
        });
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Look for releases now (blocking: run it off the UI thread), and keep the answer.
pub fn check() -> Result<Vec<Release>, String> {
    let json = fetch()?;
    let list = parse(&json)?;
    let p = cache_file();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let tmp = p.with_extension("json.part");
    if std::fs::write(&tmp, &json).is_ok() {
        let _ = std::fs::rename(&tmp, &p);
    }
    Ok(list)
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
