//! CalDAV accounts (Nextcloud, Fastmail, iCloud, Radicale…), synced both ways. Each calendar on
//! the server is a calendar folder here like any other; `.rivet-caldav` in it says where on the
//! server it is and, for each of its files, the server's version of it (its ETag) and what the
//! file was when they last matched. A sync brings down what changed on the server, sends up what
//! changed here (new, changed and deleted events), and when both changed the same event, the
//! server's is kept.
//!
//! Google accounts are CalDAV too, signed in to in the browser (`oauth`); Microsoft's calendars
//! aren't on CalDAV, and are synced through Microsoft Graph instead (`graph`), with the same
//! `.rivet-caldav` record and the same rules.
//!
//! Requests are made with curl. The password (or the token a sign-in gives) is in the desktop's
//! keyring and goes to curl on its input, never on a command line or in a file.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::oauth::{Client, Provider};
use crate::xml::{self, Node};

/// An account: a server and who to sign in as. The password, or for an account signed in to in
/// the browser its token, is kept in the keyring.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Account {
    pub id: String,
    pub url: String,
    pub user: String,
    pub name: String,
    /// Google or Microsoft, signed in to in the browser; None for a CalDAV server and a password.
    pub provider: Option<Provider>,
    /// The client ID it signs in with when it's someone's own (its secret is in the keyring);
    /// empty for Rivet's.
    pub client: String,
}

fn accounts_path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-calendar/accounts")
}

/// The accounts, from `~/.config/rivet-calendar/accounts` (`[id]` then `url=`, `user=`, `name=`).
pub fn accounts() -> Vec<Account> {
    let text = std::fs::read_to_string(accounts_path()).unwrap_or_default();
    let mut out: Vec<Account> = Vec::new();
    for line in text.lines().map(str::trim) {
        if let Some(id) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            out.push(Account { id: id.to_string(), name: id.to_string(), ..Default::default() });
        } else if let (Some(a), Some((k, v))) = (out.last_mut(), line.split_once('=')) {
            match k.trim() {
                "url" => a.url = v.trim().to_string(),
                "user" => a.user = v.trim().to_string(),
                "name" => a.name = v.trim().to_string(),
                "kind" => a.provider = Provider::from_key(v.trim()),
                "client" => a.client = v.trim().to_string(),
                _ => {}
            }
        }
    }
    out
}

pub fn save_accounts(list: &[Account]) -> std::io::Result<()> {
    let text: String = list
        .iter()
        .map(|a| {
            let kind = a.provider.map(|p| format!("kind={}\n", p.key())).unwrap_or_default();
            let client = if a.client.is_empty() { String::new() } else { format!("client={}\n", a.client) };
            format!("[{}]\n{kind}url={}\nuser={}\nname={}\n{client}\n", a.id, a.url, a.user, a.name)
        })
        .collect();
    let p = accounts_path();
    if let Some(d) = p.parent() {
        std::fs::create_dir_all(d)?;
    }
    std::fs::write(p, text)
}

fn keyring_attrs(id: &str) -> [(&str, &str); 2] {
    [("service", "rivet-calendar"), ("account", id)]
}

pub fn password(id: &str) -> Option<String> {
    rivet_xdg::keyring::lookup(&keyring_attrs(id))
}

/// A signed-in account's secrets: its refresh token, and its own client's secret.
fn token_attrs(id: &str) -> [(&str, &str); 3] {
    [("service", "rivet-calendar"), ("account", id), ("kind", "token")]
}

fn client_secret_attrs(id: &str) -> [(&str, &str); 3] {
    [("service", "rivet-calendar"), ("account", id), ("kind", "client-secret")]
}

/// The app an account signs in as: its own, or Rivet's.
fn client_of(a: &Account, p: Provider) -> Result<Client, String> {
    if a.client.is_empty() {
        return p.builtin().ok_or_else(|| format!("This build has no {} client ID: remove the account and add it again with your own.", p.name()));
    }
    Ok(Client { id: a.client.clone(), secret: rivet_xdg::keyring::lookup(&client_secret_attrs(&a.id)).unwrap_or_default() })
}

/// How an account's requests sign in: its password, or a current token.
pub fn login_for(a: &Account) -> Result<Login, String> {
    match a.provider {
        None => Ok(Login { user: a.user.clone(), pass: password(&a.id).ok_or("Its password isn't in the keyring: remove the account and add it again.")?, token: String::new() }),
        Some(p) => {
            let client = client_of(a, p)?;
            let refresh = rivet_xdg::keyring::lookup(&token_attrs(&a.id)).ok_or("It isn't signed in: remove the account and add it again.")?;
            let id = a.id.clone();
            let name = a.name.clone();
            let token = crate::oauth::access_token(&a.id, p, &client, &refresh, |new| {
                let _ = rivet_xdg::keyring::store(&format!("Rivet Calendar sign-in for {name}"), &token_attrs(&id), new);
            })?;
            Ok(Login { token, ..Default::default() })
        }
    }
}

/// An answer from the server.
#[derive(Debug, Default)]
pub struct Resp {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Resp {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.as_str())
    }

    fn ok(&self) -> bool {
        (200..300).contains(&self.status)
    }
}

/// A string for curl's config: quoted, with what needs it escaped.
fn quoted(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Who to sign in as, for a request: a user name and password, or a token from a sign-in.
#[derive(Clone, Default)]
pub struct Login {
    pub user: String,
    pub pass: String,
    pub token: String,
}

/// One request. Everything (the password and the body too) goes to curl as its config, on its
/// input. Redirects are followed, keeping the method.
pub fn request(method: &str, url: &str, login: &Login, headers: &[(&str, &str)], body: Option<&str>) -> Result<Resp, String> {
    let mut cfg = format!("url = {}\nrequest = {}\nsilent\nshow-error\nlocation\nmax-time = 60\ndump-header = \"-\"\n", quoted(url), quoted(method));
    if !login.user.is_empty() {
        cfg.push_str(&format!("user = {}\n", quoted(&format!("{}:{}", login.user, login.pass))));
    }
    if !login.token.is_empty() {
        cfg.push_str(&format!("header = {}\n", quoted(&format!("Authorization: Bearer {}", login.token))));
    }
    for (k, v) in headers {
        cfg.push_str(&format!("header = {}\n", quoted(&format!("{k}: {v}"))));
    }
    if let Some(b) = body {
        cfg.push_str(&format!("data-binary = {}\n", quoted(b)));
    }
    let mut child = Command::new("curl")
        .args(["-K", "-"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Syncing needs curl, which isn't installed.".to_string())?;
    child.stdin.take().ok_or("no input")?.write_all(cfg.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if !out.status.success() && out.stdout.is_empty() {
        let why = String::from_utf8_lossy(&out.stderr);
        return Err(format!("The server didn't answer ({}).", why.trim().trim_start_matches("curl: ")));
    }
    Ok(parse_response(&String::from_utf8_lossy(&out.stdout)))
}

/// Headers then body, as curl gives them; after a redirect or a `100 Continue` the last block of
/// headers is the answer's.
fn parse_response(text: &str) -> Resp {
    let mut r = Resp::default();
    let mut rest = text;
    while rest.starts_with("HTTP/") {
        let (block, after) = match rest.find("\r\n\r\n") {
            Some(i) => (&rest[..i], &rest[i + 4..]),
            None => (rest, ""),
        };
        let mut lines = block.lines();
        r.status = lines.next().and_then(|l| l.split_whitespace().nth(1)).and_then(|s| s.parse().ok()).unwrap_or(0);
        r.headers = lines.filter_map(|l| l.split_once(':')).map(|(k, v)| (k.trim().to_string(), v.trim().to_string())).collect();
        rest = after;
    }
    r.body = rest.to_string();
    r
}

/// `href` (a path on the server, or a whole address) made whole against `base`.
pub fn absolute(base: &str, href: &str) -> String {
    if href.starts_with("http://") || href.starts_with("https://") {
        return href.to_string();
    }
    let scheme_end = base.find("://").map(|i| i + 3).unwrap_or(0);
    let host_end = base[scheme_end..].find('/').map(|i| i + scheme_end).unwrap_or(base.len());
    format!("{}{}", &base[..host_end], href)
}

/// An address's path, the way a server writes hrefs: `/remote.php/dav/calendars/me/`.
fn path_of(url: &str) -> String {
    let scheme_end = url.find("://").map(|i| i + 3).unwrap_or(0);
    url[scheme_end..].find('/').map(|i| url[scheme_end + i..].to_string()).unwrap_or_else(|| "/".into())
}

/// Two hrefs for the same thing (one may be percent-encoded, or lack a trailing slash).
fn same_href(a: &str, b: &str) -> bool {
    href_key(a) == href_key(b)
}

/// An href as `same_href` compares it, for looking hrefs up by.
fn href_key(s: &str) -> String {
    rivet_xdg::places::percent_decode(&path_of(s).trim_end_matches('/').to_lowercase())
}

const PRINCIPAL: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:current-user-principal/><c:calendar-home-set/></d:prop></d:propfind>"#;
const HOME: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><c:calendar-home-set/></d:prop></d:propfind>"#;
const LIST: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:a="http://apple.com/ns/ical/"><d:prop><d:resourcetype/><d:displayname/><a:calendar-color/><c:supported-calendar-component-set/></d:prop></d:propfind>"#;
const ETAGS: &str = r#"<?xml version="1.0" encoding="utf-8"?><d:propfind xmlns:d="DAV:"><d:prop><d:getetag/><d:resourcetype/></d:prop></d:propfind>"#;

/// A calendar on the server.
#[derive(Clone, Debug, PartialEq)]
pub struct Remote {
    pub url: String,
    pub name: String,
    pub color: Option<[u8; 3]>,
    /// Shared with the account to see but not change.
    pub read_only: bool,
}

pub(crate) fn multistatus(r: &Resp, what: &str) -> Result<Node, String> {
    match r.status {
        207 => Ok(xml::parse(&r.body)),
        401 => Err("The server didn't take the user name and password.".into()),
        403 => Err(format!("The server wouldn't let this account see {what}.")),
        404 => Err(format!("The server has no {what} at that address.")),
        0 => Err("The server didn't answer.".into()),
        s => Err(format!("The server answered {s} when asked for {what}.")),
    }
}

/// The calendars an account can see: from the address given, the account's own place
/// (current-user-principal), then where its calendars are (calendar-home-set), as the standard
/// has it. An address with no path tries `/.well-known/caldav` first.
pub fn discover(url: &str, login: &Login) -> Result<Vec<Remote>, String> {
    let mut base = url.trim().trim_end_matches('/').to_string();
    if !base.contains("://") {
        base = format!("https://{base}");
    }
    let depth0 = [("Depth", "0"), ("Content-Type", "application/xml; charset=utf-8")];
    let depth1 = [("Depth", "1"), ("Content-Type", "application/xml; charset=utf-8")];
    let mut start = format!("{base}/");
    if path_of(&start) == "/" {
        let wk = format!("{base}/.well-known/caldav");
        if request("PROPFIND", &wk, login, &depth0, Some(PRINCIPAL)).is_ok_and(|r| r.status == 207) {
            start = wk;
        }
    }
    let first = multistatus(&request("PROPFIND", &start, login, &depth0, Some(PRINCIPAL))?, "calendars")?;
    let home = match first.all("calendar-home-set").first().and_then(|h| h.text_of("href")) {
        Some(h) => absolute(&start, &h),
        None => match first.all("current-user-principal").first().and_then(|p| p.text_of("href")) {
            Some(p) => {
                let principal = absolute(&start, &p);
                let found = multistatus(&request("PROPFIND", &principal, login, &depth0, Some(HOME))?, "calendars")?;
                found.all("calendar-home-set").first().and_then(|h| h.text_of("href")).map(|h| absolute(&principal, &h)).unwrap_or(start.clone())
            }
            // No principal: the address may be the calendars' own place.
            None => start.clone(),
        },
    };
    let list = multistatus(&request("PROPFIND", &home, login, &depth1, Some(LIST))?, "calendars")?;
    let mut out = Vec::new();
    for r in list.all("response") {
        let Some(href) = r.text_of("href") else { continue };
        let is_calendar = r.all("resourcetype").iter().any(|t| t.child("calendar").is_some());
        let comps: Vec<&str> = r.all("comp").iter().filter_map(|c| c.attr("name")).collect();
        // Task lists and the like have no events to show.
        if !is_calendar || !(comps.is_empty() || comps.iter().any(|c| c.eq_ignore_ascii_case("VEVENT"))) {
            continue;
        }
        let url = absolute(&home, &href);
        let name = r.text_of("displayname").filter(|n| !n.is_empty()).unwrap_or_else(|| path_of(&url).trim_end_matches('/').rsplit('/').next().unwrap_or("Calendar").to_string());
        out.push(Remote { url: if url.ends_with('/') { url } else { format!("{url}/") }, name, color: r.text_of("calendar-color").and_then(|c| crate::store::parse_color(&c)), read_only: false });
    }
    if out.is_empty() {
        return Err("The server has no calendars for this account.".into());
    }
    Ok(out)
}

/// A calendar folder's link to the server: where, and each file's state when last synced.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    pub account: String,
    pub url: String,
    /// File name → (href, ETag, the file's hash then).
    pub items: HashMap<String, (String, String, String)>,
}

pub fn link(dir: &Path) -> Option<Link> {
    let text = std::fs::read_to_string(dir.join(".rivet-caldav")).ok()?;
    let mut l = Link::default();
    for line in text.lines() {
        match line.split_once('=') {
            Some(("account", v)) => l.account = v.to_string(),
            Some(("url", v)) => l.url = v.to_string(),
            Some(("item", v)) => {
                let p: Vec<&str> = v.split('\t').collect();
                if p.len() == 4 {
                    l.items.insert(p[0].to_string(), (p[1].to_string(), p[2].to_string(), p[3].to_string()));
                }
            }
            _ => {}
        }
    }
    (!l.url.is_empty()).then_some(l)
}

pub(crate) fn save_link(dir: &Path, l: &Link) -> std::io::Result<()> {
    let mut text = format!("account={}\nurl={}\n", l.account, l.url);
    let mut items: Vec<_> = l.items.iter().collect();
    items.sort();
    for (f, (h, e, x)) in items {
        text.push_str(&format!("item={f}\t{h}\t{e}\t{x}\n"));
    }
    std::fs::write(dir.join(".rivet-caldav"), text)
}

pub(crate) fn hash(text: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    text.hash(&mut h);
    format!("{:016x}", h.finish())
}

/// What a sync did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Synced {
    pub down: usize,
    pub up: usize,
    pub removed: usize,
}

/// The file name a server's event is kept under here.
fn local_name(href: &str) -> String {
    let last = rivet_xdg::places::percent_decode(href.trim_end_matches('/').rsplit('/').next().unwrap_or("event"));
    let safe: String = last.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.@".contains(c) { c } else { '_' }).collect();
    if safe.to_ascii_lowercase().ends_with(".ics") { safe } else { format!("{safe}.ics") }
}

/// Sync one calendar folder with its calendar on the server.
pub fn sync(dir: &Path) -> Result<Synced, String> {
    let mut l = link(dir).ok_or("This calendar isn't on a server.")?;
    let acct = accounts().into_iter().find(|a| a.id == l.account).ok_or("Its account is gone: add it again.")?;
    let login = login_for(&acct)?;
    let busy = dir.join(".rivet-sync-busy");
    if std::fs::metadata(&busy).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|a| a.as_secs() > 300) {
        let _ = std::fs::remove_file(&busy);
    }
    if std::fs::OpenOptions::new().write(true).create_new(true).open(&busy).is_err() {
        return Err("It's being synced already.".into());
    }
    let result = match acct.provider {
        Some(Provider::Microsoft) => crate::graph::sync_inner(dir, &mut l, &login),
        _ => sync_inner(dir, &mut l, &login),
    };
    if result.as_ref().is_err_and(|e| e.contains("didn't take")) {
        // A token turned down is asked for again next time.
        crate::oauth::forget(&acct.id);
    }
    let _ = save_link(dir, &l);
    let _ = std::fs::remove_file(&busy);
    result
}

fn sync_inner(dir: &Path, l: &mut Link, login: &Login) -> Result<Synced, String> {
    let mut done = Synced::default();
    let depth1 = [("Depth", "1"), ("Content-Type", "application/xml; charset=utf-8")];
    // ---- what the server has: href → ETag
    let listing = multistatus(&request("PROPFIND", &l.url, login, &depth1, Some(ETAGS))?, "this calendar")?;
    let mut remote: Vec<(String, String)> = Vec::new();
    for r in listing.all("response") {
        let Some(href) = r.text_of("href") else { continue };
        if same_href(&href, &l.url) || r.all("resourcetype").iter().any(|t| t.child("collection").is_some()) {
            continue;
        }
        remote.push((absolute(&l.url, &href), r.text_of("getetag").unwrap_or_default()));
    }
    // ---- what's here: file → its text
    let local: HashMap<String, String> = std::fs::read_dir(dir)
        .map_err(|e| e.to_string())?
        .flatten()
        .filter(|e| e.path().extension().is_some_and(|x| x == "ics"))
        .filter_map(|e| Some((e.file_name().to_string_lossy().into_owned(), std::fs::read_to_string(e.path()).ok()?)))
        .collect();
    // Looked up by href, as a calendar can have thousands.
    let by_href: HashMap<String, String> = l.items.iter().map(|(f, (h, _, _))| (href_key(h), f.clone())).collect();
    let on_server: std::collections::HashSet<String> = remote.iter().map(|(h, _)| href_key(h)).collect();
    let mut fetch: Vec<(String, String)> = Vec::new(); // (href, file)
    for (href, etag) in &remote {
        match by_href.get(&href_key(href)).cloned() {
            Some(file) => {
                let (_, old_etag, old_hash) = l.items.get(&file).cloned().unwrap_or_default();
                let here = local.get(&file);
                if *etag != old_etag || here.is_none() && etag.is_empty() {
                    // Changed on the server: brought down (the server's wins over a change here).
                    if here.is_some() || etag != &old_etag {
                        fetch.push((href.clone(), file));
                    }
                } else if let Some(text) = here {
                    if hash(text) != old_hash {
                        put(dir, l, login, &file, href, text, Some(etag))?;
                        done.up += 1;
                    }
                } else {
                    // Deleted here: deleted there.
                    let r = request("DELETE", href, login, &[("If-Match", etag)], None)?;
                    if r.ok() || r.status == 404 || r.status == 410 {
                        l.items.remove(&file);
                        done.removed += 1;
                    } else if r.status == 412 {
                        fetch.push((href.clone(), file));
                    }
                }
            }
            None => fetch.push((href.clone(), local_name(href))),
        }
    }
    // Gone from the server: gone here, unless changed here since (then it goes back up).
    let gone: Vec<String> = l.items.iter().filter(|(_, (h, _, _))| !on_server.contains(&href_key(h))).map(|(f, _)| f.clone()).collect();
    for file in gone {
        let (_, _, old_hash) = l.items.remove(&file).unwrap_or_default();
        match local.get(&file) {
            Some(text) if hash(text) != old_hash => {
                let href = absolute(&l.url, &format!("{}{}", path_of(&l.url), rivet_xdg::places::percent_encode_path(Path::new(&file))));
                put(dir, l, login, &file, &href, text, None)?;
                done.up += 1;
            }
            Some(_) => {
                let _ = std::fs::remove_file(dir.join(&file));
                done.removed += 1;
            }
            None => {}
        }
    }
    // New here: sent up.
    let fetching: std::collections::HashSet<String> = fetch.iter().map(|(_, f)| f.clone()).collect();
    for (file, text) in &local {
        if !l.items.contains_key(file) && !fetching.contains(file) {
            let href = absolute(&l.url, &format!("{}{}", path_of(&l.url), rivet_xdg::places::percent_encode_path(Path::new(file))));
            put(dir, l, login, file, &href, text, None)?;
            done.up += 1;
        }
    }
    done.down += download(dir, l, login, &fetch)?;
    Ok(done)
}

/// Send a file up: as new (`If-None-Match: *`) or over the server's version it was based on.
fn put(dir: &Path, l: &mut Link, login: &Login, file: &str, href: &str, text: &str, etag: Option<&str>) -> Result<(), String> {
    let cond = match etag {
        Some(e) if !e.is_empty() => ("If-Match", e),
        _ => ("If-None-Match", "*"),
    };
    let r = request("PUT", href, login, &[("Content-Type", "text/calendar; charset=utf-8"), cond], Some(text))?;
    if r.status == 412 {
        // Changed on the server meanwhile: its version is brought down at the next sync.
        l.items.insert(file.to_string(), (href.to_string(), String::new(), String::new()));
        return Ok(());
    }
    if !r.ok() {
        return Err(format!("The server wouldn't take a change ({}).", r.status));
    }
    let etag = match r.header("ETag") {
        Some(e) => e.to_string(),
        None => etag_of(href, login).unwrap_or_default(),
    };
    l.items.insert(file.to_string(), (href.to_string(), etag, hash(text)));
    let _ = dir;
    Ok(())
}

fn etag_of(href: &str, login: &Login) -> Option<String> {
    let r = request("PROPFIND", href, login, &[("Depth", "0"), ("Content-Type", "application/xml; charset=utf-8")], Some(ETAGS)).ok()?;
    xml::parse(&r.body).text_of("getetag")
}

/// Bring events down, many to a request (calendar-multiget), else one at a time.
fn download(dir: &Path, l: &mut Link, login: &Login, want: &[(String, String)]) -> Result<usize, String> {
    let mut n = 0;
    for chunk in want.chunks(40) {
        let hrefs: String = chunk.iter().map(|(h, _)| format!("<d:href>{}</d:href>", xml::escape(&path_of(h)))).collect();
        let body = format!(r#"<?xml version="1.0" encoding="utf-8"?><c:calendar-multiget xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav"><d:prop><d:getetag/><c:calendar-data/></d:prop>{hrefs}</c:calendar-multiget>"#);
        let r = request("REPORT", &l.url, login, &[("Depth", "1"), ("Content-Type", "application/xml; charset=utf-8")], Some(&body))?;
        let mut got: Vec<(String, String, String)> = Vec::new(); // (href, etag, text)
        if r.status == 207 {
            for resp in xml::parse(&r.body).all("response") {
                if let (Some(h), Some(t)) = (resp.text_of("href"), resp.text_of("calendar-data")) {
                    got.push((absolute(&l.url, &h), resp.text_of("getetag").unwrap_or_default(), t));
                }
            }
        }
        for (href, file) in chunk {
            let found = got.iter().find(|(h, _, _)| same_href(h, href)).cloned();
            let (etag, text) = match found {
                Some((_, e, t)) => (e, t),
                None => {
                    let g = request("GET", href, login, &[], None)?;
                    if !g.ok() {
                        continue;
                    }
                    (g.header("ETag").unwrap_or_default().to_string(), g.body)
                }
            };
            // Line ends as the format has them.
            let text = if text.contains("\r\n") { text } else { text.replace('\n', "\r\n") };
            crate::store::write_atomic(&dir.join(file), &text).map_err(|e| e.to_string())?;
            l.items.insert(file.clone(), (href.clone(), etag, hash(&text)));
            n += 1;
        }
    }
    Ok(n)
}

/// Add a CalDAV account: find its calendars, keep its password in the keyring, and make a folder
/// for each calendar (empty until it's synced). Gives the folders.
pub fn add_account(root: &Path, url: &str, user: &str, pass: &str, name: &str) -> Result<Vec<PathBuf>, String> {
    let login = Login { user: user.trim().to_string(), pass: pass.to_string(), token: String::new() };
    let found = discover(url, &login)?;
    let host = url.trim().split("://").last().unwrap_or(url).split('/').next().unwrap_or("server").to_string();
    let name = if name.trim().is_empty() { host } else { name.trim().to_string() };
    let acct = Account { url: url.trim().to_string(), user: login.user.clone(), name, ..Default::default() };
    make_account(root, acct, &found, |id, name| rivet_xdg::keyring::store(&format!("Rivet Calendar password for {name}"), &keyring_attrs(id), pass))
}

/// Add a Google or Microsoft account: sign in in the browser (as `own` client when given, else as
/// Rivet), find its calendars, and make a folder for each (empty until it's synced). `cancel` stops the wait for the browser.
pub fn add_signed_in(root: &Path, p: Provider, own: Option<Client>, name: &str, cancel: &std::sync::atomic::AtomicBool) -> Result<Vec<PathBuf>, String> {
    let client = match &own {
        Some(c) if !c.id.trim().is_empty() => Client { id: c.id.trim().to_string(), secret: c.secret.trim().to_string() },
        _ => p.builtin().ok_or_else(|| format!("This build has no {} client ID: use your own.", p.name()))?,
    };
    let tokens = crate::oauth::sign_in(p, &client, cancel)?;
    let login = Login { token: tokens.access.clone(), ..Default::default() };
    let (user, found) = match p {
        Provider::Google => google_calendars(&login)?,
        Provider::Microsoft => crate::graph::calendars(&login)?,
    };
    // The same account twice would sync every calendar twice.
    if !user.is_empty() && accounts().iter().any(|a| a.provider == Some(p) && a.user.eq_ignore_ascii_case(&user)) {
        return Err(format!("{user} is added already."));
    }
    let name = if name.trim().is_empty() { if user.is_empty() { p.name().to_string() } else { user.clone() } } else { name.trim().to_string() };
    let own = own.is_some_and(|c| !c.id.trim().is_empty());
    let acct = Account { url: String::new(), user, name, provider: Some(p), client: if own { client.id.clone() } else { String::new() }, ..Default::default() };
    make_account(root, acct, &found, |id, name| {
        rivet_xdg::keyring::store(&format!("Rivet Calendar sign-in for {name}"), &token_attrs(id), &tokens.refresh)?;
        crate::oauth::remember(id, &tokens);
        if own && !client.secret.is_empty() {
            rivet_xdg::keyring::store(&format!("Rivet Calendar client secret for {name}"), &client_secret_attrs(id), &client.secret)?;
        }
        Ok(())
    })
}

/// A Google account's calendars, from the Calendar API's list of them, each at its CalDAV address;
/// and the account's address (the main calendar's id).
fn google_calendars(login: &Login) -> Result<(String, Vec<Remote>), String> {
    let mut out = Vec::new();
    let mut user = String::new();
    let mut page = String::new();
    loop {
        let url = format!("https://www.googleapis.com/calendar/v3/users/me/calendarList?maxResults=250{}", if page.is_empty() { String::new() } else { format!("&pageToken={}", crate::oauth::encode(&page)) });
        let r = request("GET", &url, login, &[("Accept", "application/json")], None)?;
        if r.status == 403 {
            return Err("Google turned the calendars down: the client's project needs the Google Calendar API and CalDAV API turned on.".into());
        }
        if r.status != 200 {
            return Err(format!("Google answered {} when asked for the calendars.", r.status));
        }
        let v: serde_json::Value = serde_json::from_str(&r.body).map_err(|e| e.to_string())?;
        for c in v["items"].as_array().into_iter().flatten() {
            let id = c["id"].as_str().unwrap_or_default();
            if id.is_empty() || c["deleted"].as_bool() == Some(true) {
                continue;
            }
            if c["primary"].as_bool() == Some(true) {
                user = id.to_string();
            }
            let name = c["summaryOverride"].as_str().or(c["summary"].as_str()).unwrap_or(id).to_string();
            let role = c["accessRole"].as_str().unwrap_or("");
            out.push(Remote {
                url: format!("https://apidata.googleusercontent.com/caldav/v2/{}/events/", crate::oauth::encode(id)),
                name,
                color: c["backgroundColor"].as_str().and_then(crate::store::parse_color),
                read_only: role != "owner" && role != "writer",
            });
        }
        match v["nextPageToken"].as_str() {
            Some(t) if !t.is_empty() => page = t.to_string(),
            _ => break,
        }
    }
    if out.is_empty() {
        return Err("Google has no calendars for this account.".into());
    }
    Ok((user, out))
}

/// Save an account and make a folder for each of its calendars. `keep` puts its secrets in the
/// keyring, given its id and name. The first sync is left to the caller: a busy calendar's first
/// download can take minutes, and the account is usable before it's done.
fn make_account(root: &Path, mut acct: Account, found: &[Remote], keep: impl FnOnce(&str, &str) -> Result<(), String>) -> Result<Vec<PathBuf>, String> {
    let mut list = accounts();
    let base: String = acct.name.chars().map(|c| if c.is_ascii_alphanumeric() { c.to_ascii_lowercase() } else { '-' }).collect();
    let mut id = base.trim_matches('-').to_string();
    let mut k = 2;
    while list.iter().any(|a| a.id == id) || id.is_empty() {
        id = format!("{}-{k}", base.trim_matches('-'));
        k += 1;
    }
    keep(&id, &acct.name)?;
    acct.id = id.clone();
    list.push(acct);
    save_accounts(&list).map_err(|e| format!("Couldn't save the account: {e}"))?;
    let mut store = crate::Store::open(root);
    let mut dirs = Vec::new();
    for (i, r) in found.iter().enumerate() {
        let color = r.color.unwrap_or([[0x72, 0x87, 0xfd], [0x40, 0xa0, 0x2b], [0xdf, 0x8e, 0x1d], [0x88, 0x39, 0xef]][i % 4]);
        let dir = store.create_calendar(&r.name, color).map_err(|e| format!("Couldn't make the calendar: {e}"))?;
        save_link(&dir, &Link { account: id.clone(), url: r.url.clone(), items: HashMap::new() }).map_err(|e| e.to_string())?;
        if r.read_only {
            let _ = std::fs::write(dir.join(".read-only"), "");
        }
        dirs.push(dir);
    }
    Ok(dirs)
}

/// Remove an account: its calendars here go to the Trash, its password from the keyring. The
/// calendars on the server stay as they are.
pub fn remove_account(root: &Path, id: &str) -> Result<(), String> {
    for d in std::fs::read_dir(root).into_iter().flatten().flatten().map(|e| e.path()) {
        if link(&d).is_some_and(|l| l.account == id) {
            rivet_xdg::trash::trash(&d).map_err(|e| e.to_string())?;
        }
    }
    rivet_xdg::keyring::clear(&keyring_attrs(id));
    let list: Vec<Account> = accounts().into_iter().filter(|a| a.id != id).collect();
    save_accounts(&list).map_err(|e| e.to_string())
}

/// Sync every calendar on a server. Gives what changed in all, and why any couldn't be synced.
pub fn sync_all(root: &Path) -> (Synced, Vec<String>) {
    let mut total = Synced::default();
    let mut problems = Vec::new();
    for d in std::fs::read_dir(root).into_iter().flatten().flatten().map(|e| e.path()).filter(|d| link(d).is_some()) {
        match sync(&d) {
            Ok(s) => {
                total.down += s.down;
                total.up += s.up;
                total.removed += s.removed;
            }
            Err(e) => {
                let name = std::fs::read_to_string(d.join("displayname")).unwrap_or_default();
                problems.push(format!("{}: {e}", name.trim()));
            }
        }
    }
    (total, problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_config_strings_are_escaped() {
        assert_eq!(quoted("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn the_last_header_block_is_the_answer() {
        let r = parse_response("HTTP/1.1 100 Continue\r\n\r\nHTTP/1.1 201 Created\r\nETag: \"42\"\r\n\r\nbody");
        assert_eq!(r.status, 201);
        assert_eq!(r.header("etag"), Some("\"42\""));
        assert_eq!(r.body, "body");
    }

    #[test]
    fn hrefs_are_made_whole_and_compared() {
        assert_eq!(absolute("https://h.org/dav/cal/", "/dav/cal/a.ics"), "https://h.org/dav/cal/a.ics");
        assert!(same_href("https://h.org/dav/my%20cal/", "/dav/my cal"));
        assert_eq!(local_name("/dav/cal/Event%201.ics"), "Event_1.ics");
        assert_eq!(local_name("/dav/cal/abc"), "abc.ics");
    }

    #[test]
    fn links_round_trip() {
        let dir = std::env::temp_dir().join(format!("rivet-caldav-link-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut l = Link { account: "me".into(), url: "https://h.org/cal/".into(), items: HashMap::new() };
        l.items.insert("a.ics".into(), ("https://h.org/cal/a.ics".into(), "\"1\"".into(), "ff".into()));
        save_link(&dir, &l).unwrap();
        assert_eq!(link(&dir), Some(l));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
