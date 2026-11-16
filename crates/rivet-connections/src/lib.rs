//! Saved server connections, shared by Rivet FTP's window and its command line.
//!
//! They live in one plain, hand-editable file, `~/.config/rivet-ftp/connections.conf`:
//!
//! ```text
//! [Work site]
//! protocol=sftp
//! host=example.com
//! port=2222
//! user=me
//! key=~/.ssh/id_ed25519
//! remote=/var/www
//! local=~/Sites/work
//! ```
//!
//! Passwords are never written here; the window keeps them in the desktop's keyring. SSH
//! connections use keys or the ssh agent.
//!
//! Each save also writes `~/.config/rivet-ftp/ssh_config` with one `Host` block per SFTP
//! connection. With `Include ~/.config/rivet-ftp/ssh_config` in `~/.ssh/config`, plain `ssh`,
//! `scp`, `rsync` and `git` reach saved connections by name too.

pub mod keys;

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// SSH file transfer (also used for `ssh` itself).
    Sftp,
    /// Plain FTP (nothing encrypted).
    Ftp,
    /// FTP over TLS (explicit, "FTPES"; the usual kind).
    Ftps,
}

impl Protocol {
    pub fn parse(s: &str) -> Option<Protocol> {
        match s.to_ascii_lowercase().as_str() {
            "sftp" | "ssh" | "scp" => Some(Protocol::Sftp),
            "ftp" => Some(Protocol::Ftp),
            "ftps" | "ftpes" => Some(Protocol::Ftps),
            _ => None,
        }
    }
    pub fn scheme(self) -> &'static str {
        match self {
            Protocol::Sftp => "sftp",
            Protocol::Ftp => "ftp",
            Protocol::Ftps => "ftps",
        }
    }
    pub fn default_port(self) -> u16 {
        match self {
            Protocol::Sftp => 22,
            Protocol::Ftp | Protocol::Ftps => 21,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Connection {
    /// What the person calls it ("Work site"); unique, compared without case.
    pub name: String,
    pub protocol: Protocol,
    pub host: String,
    pub port: Option<u16>,
    pub user: Option<String>,
    /// SSH private key (SFTP only); `~` is allowed.
    pub key: Option<String>,
    /// Folder to open on the server.
    pub remote: Option<String>,
    /// Folder to open on this computer, in the other pane.
    pub local: Option<String>,
    pub note: Option<String>,
}

impl Connection {
    /// A connection from an address such as `sftp://me@example.com:2222/var/www`. A bare
    /// `me@example.com` means SFTP. Passwords in the address are refused: they don't belong
    /// in a plain file.
    pub fn from_url(name: &str, url: &str) -> Result<Connection, String> {
        let name = name.trim();
        if name.is_empty() || name.contains(['[', ']', '\n']) {
            return Err("give the connection a name (without [ or ])".into());
        }
        let (protocol, rest) = match url.split_once("://") {
            Some((scheme, rest)) => (Protocol::parse(scheme).ok_or_else(|| format!("unknown protocol \"{scheme}\" (use sftp, ftp or ftps)"))?, rest),
            None => (Protocol::Sftp, url),
        };
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], Some(&rest[i..])),
            None => (rest, None),
        };
        let (user, hostport) = match authority.rsplit_once('@') {
            Some((u, h)) => (Some(u), h),
            None => (None, authority),
        };
        if let Some(u) = user
            && u.contains(':')
        {
            return Err("leave the password out of the address; Rivet FTP keeps passwords in your keyring, and SSH uses keys".into());
        }
        let (host, port) = split_host_port(hostport)?;
        if host.is_empty() {
            return Err(format!("no server in \"{url}\""));
        }
        Ok(Connection {
            name: name.to_string(),
            protocol,
            host,
            port,
            user: user.map(percent_decode).filter(|u| !u.is_empty()),
            key: None,
            remote: path.map(percent_decode).filter(|p| p != "/" && !p.is_empty()),
            local: None,
            note: None,
        })
    }

    pub fn port(&self) -> u16 {
        self.port.unwrap_or(self.protocol.default_port())
    }

    /// `sftp://me@example.com:2222/var/www`
    pub fn url(&self) -> String {
        let mut s = format!("{}://", self.protocol.scheme());
        if let Some(u) = &self.user {
            s.push_str(u);
            s.push('@');
        }
        if self.host.contains(':') {
            let _ = write!(s, "[{}]", self.host);
        } else {
            s.push_str(&self.host);
        }
        if let Some(p) = self.port.filter(|p| *p != self.protocol.default_port()) {
            let _ = write!(s, ":{p}");
        }
        if let Some(r) = &self.remote {
            if !r.starts_with('/') {
                s.push('/');
            }
            s.push_str(r);
        }
        s
    }

    /// `me@example.com`
    pub fn target(&self) -> String {
        match &self.user {
            Some(u) => format!("{u}@{}", self.host),
            None => self.host.clone(),
        }
    }

    /// The name as an ssh `Host` alias: lower case, spaces and odd characters as dashes.
    pub fn alias(&self) -> String {
        slug(&self.name)
    }

    fn need_ssh(&self, tool: &str) -> Result<(), String> {
        if self.protocol == Protocol::Sftp {
            Ok(())
        } else {
            Err(format!("\"{}\" is an {} connection; {tool} needs an SFTP (SSH) one", self.name, self.protocol.scheme().to_uppercase()))
        }
    }

    /// Arguments for `ssh`: log in and start in the saved remote folder, or run `command`
    /// there.
    pub fn ssh_args(&self, command: &[String]) -> Result<Vec<String>, String> {
        self.need_ssh("ssh")?;
        let mut a = Vec::new();
        if let Some(p) = self.port.filter(|p| *p != 22) {
            a.extend(["-p".into(), p.to_string()]);
        }
        if let Some(k) = &self.key {
            a.extend(["-i".into(), expand_home(k)]);
        }
        let remote_dir = self.remote.as_deref().filter(|r| !r.is_empty());
        match (remote_dir, command.is_empty()) {
            (None, true) => a.push(self.target()),
            (None, false) => {
                a.push(self.target());
                a.push("--".into());
                a.extend(command.iter().cloned());
            }
            (Some(dir), true) => {
                // A login shell, started in the folder.
                a.extend(["-t".into(), self.target(), format!("cd -- {} && exec \"$SHELL\" -l", shell_quote(dir))]);
            }
            (Some(dir), false) => {
                a.extend([self.target(), format!("cd -- {} && {}", shell_quote(dir), command.join(" "))]);
            }
        }
        Ok(a)
    }

    /// Arguments for the `sftp` command line, starting in the saved remote folder.
    pub fn sftp_args(&self) -> Result<Vec<String>, String> {
        self.need_ssh("sftp")?;
        let mut a = Vec::new();
        if let Some(p) = self.port.filter(|p| *p != 22) {
            a.extend(["-P".into(), p.to_string()]);
        }
        if let Some(k) = &self.key {
            a.extend(["-i".into(), expand_home(k)]);
        }
        let host = if self.host.contains(':') { format!("[{}]", self.host) } else { self.host.clone() };
        let target = match &self.user {
            Some(u) => format!("{u}@{host}"),
            None => host,
        };
        a.push(match &self.remote {
            Some(r) => format!("{target}:{r}"),
            None => target,
        });
        Ok(a)
    }

    /// This connection as an ssh `Host` block (SFTP only).
    pub fn ssh_config_block(&self) -> Option<String> {
        if self.protocol != Protocol::Sftp {
            return None;
        }
        let mut s = format!("# {}\nHost {}\n    HostName {}\n", self.name, self.alias(), self.host);
        if let Some(u) = &self.user {
            let _ = writeln!(s, "    User {u}");
        }
        if let Some(p) = self.port.filter(|p| *p != 22) {
            let _ = writeln!(s, "    Port {p}");
        }
        if let Some(k) = &self.key {
            let _ = writeln!(s, "    IdentityFile {k}");
        }
        Some(s)
    }
}

/// All saved connections, in the order they were added.
#[derive(Debug, Default)]
pub struct Store {
    pub path: PathBuf,
    pub items: Vec<Connection>,
}

impl Store {
    /// `~/.config/rivet-ftp/connections.conf`
    pub fn default_path() -> PathBuf {
        rivet_xdg::places::config_home().join("rivet-ftp/connections.conf")
    }

    pub fn load() -> Store {
        Store::load_from(&Store::default_path())
    }

    /// Read a connections file; a missing file is an empty list, and lines it doesn't
    /// understand are skipped (so a newer version's file still loads).
    pub fn load_from(path: &Path) -> Store {
        let text = std::fs::read_to_string(path).unwrap_or_default();
        Store { path: path.to_path_buf(), items: parse(&text) }
    }

    /// Find by name or ssh alias, ignoring case.
    pub fn find(&self, name: &str) -> Option<&Connection> {
        let n = name.trim();
        self.items.iter().find(|c| c.name.eq_ignore_ascii_case(n)).or_else(|| self.items.iter().find(|c| c.alias() == slug(n)))
    }

    /// Add a connection, or replace the one with the same name when `replace` is set.
    pub fn add(&mut self, c: Connection, replace: bool) -> Result<(), String> {
        if let Some(i) = self.items.iter().position(|o| o.name.eq_ignore_ascii_case(&c.name)) {
            if !replace {
                return Err(format!("there is already a connection called \"{}\"", self.items[i].name));
            }
            self.items[i] = c;
            return Ok(());
        }
        if let Some(o) = self.items.iter().find(|o| o.alias() == c.alias()) {
            return Err(format!("\"{}\" would have the same ssh name ({}) as \"{}\"", c.name, c.alias(), o.name));
        }
        self.items.push(c);
        Ok(())
    }

    /// Remove by name or alias; returns the removed connection.
    pub fn remove(&mut self, name: &str) -> Option<Connection> {
        let target = self.find(name)?.name.clone();
        let i = self.items.iter().position(|c| c.name == target)?;
        Some(self.items.remove(i))
    }

    /// Write the connections file (readable only by you) and the ssh_config beside it.
    pub fn save(&self) -> std::io::Result<()> {
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        write_private(&self.path, &render(&self.items))?;
        write_private(&self.ssh_config_path(), &self.ssh_config())
    }

    /// `ssh_config` beside the connections file.
    pub fn ssh_config_path(&self) -> PathBuf {
        self.path.with_file_name("ssh_config")
    }

    /// Host blocks for every SFTP connection.
    pub fn ssh_config(&self) -> String {
        let mut s = String::from(
            "# Written by Rivet FTP from its saved connections; changes here are replaced.\n# Use them from plain ssh by adding this line near the top of ~/.ssh/config:\n#   Include ~/.config/rivet-ftp/ssh_config\n",
        );
        for b in self.items.iter().filter_map(|c| c.ssh_config_block()) {
            s.push('\n');
            s.push_str(&b);
        }
        s
    }
}

// ---------------------------------------------------------------- file format

fn parse(text: &str) -> Vec<Connection> {
    let mut out: Vec<Connection> = Vec::new();
    let mut cur: Option<Connection> = None;
    let finish = |c: Option<Connection>, out: &mut Vec<Connection>| {
        if let Some(c) = c
            && !c.host.is_empty()
            && !out.iter().any(|o| o.name.eq_ignore_ascii_case(&c.name))
        {
            out.push(c);
        }
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            finish(cur.take(), &mut out);
            cur = Some(Connection {
                name: name.trim().to_string(),
                protocol: Protocol::Sftp,
                host: String::new(),
                port: None,
                user: None,
                key: None,
                remote: None,
                local: None,
                note: None,
            });
            continue;
        }
        let (Some(c), Some((k, v))) = (cur.as_mut(), line.split_once('=')) else { continue };
        let v = v.trim();
        let opt = || Some(v.to_string()).filter(|s| !s.is_empty());
        match k.trim() {
            "protocol" => c.protocol = Protocol::parse(v).unwrap_or(c.protocol),
            "host" => c.host = v.to_string(),
            "port" => c.port = v.parse().ok(),
            "user" => c.user = opt(),
            "key" => c.key = opt(),
            "remote" => c.remote = opt(),
            "local" => c.local = opt(),
            "note" => c.note = opt(),
            _ => {}
        }
    }
    finish(cur, &mut out);
    out
}

fn render(items: &[Connection]) -> String {
    let mut s = String::from("# Rivet FTP saved connections. Edit freely; passwords are never stored here.\n");
    for c in items {
        let _ = write!(s, "\n[{}]\nprotocol={}\nhost={}\n", c.name, c.protocol.scheme(), c.host);
        if let Some(p) = c.port {
            let _ = writeln!(s, "port={p}");
        }
        for (k, v) in [("user", &c.user), ("key", &c.key), ("remote", &c.remote), ("local", &c.local), ("note", &c.note)] {
            if let Some(v) = v {
                let _ = writeln!(s, "{k}={}", v.replace('\n', " "));
            }
        }
    }
    s
}

/// Write via a temporary file and rename, with permissions 0600.
fn write_private(path: &Path, text: &str) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension("tmp");
    {
        let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
        f.write_all(text.as_bytes())?;
        f.sync_all()?;
    }
    std::fs::rename(&tmp, path)
}

// ---------------------------------------------------------------- helpers

fn split_host_port(s: &str) -> Result<(String, Option<u16>), String> {
    // [::1]:22
    if let Some(rest) = s.strip_prefix('[') {
        let (h, after) = rest.split_once(']').ok_or("unclosed [ in the address")?;
        let port = match after.strip_prefix(':') {
            Some(p) => Some(p.parse().map_err(|_| format!("\"{p}\" isn't a port number"))?),
            None => None,
        };
        return Ok((h.to_string(), port));
    }
    match s.rsplit_once(':') {
        Some((h, p)) if !h.contains(':') => Ok((h.to_string(), Some(p.parse().map_err(|_| format!("\"{p}\" isn't a port number"))?))),
        _ => Ok((s.to_string(), None)),
    }
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(hex) = std::str::from_utf8(&b[i + 1..i + 3])
            && let Ok(v) = u8::from_str_radix(hex, 16)
        {
            out.push(v);
            i += 3;
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// "Work site" → "work-site"
pub fn slug(name: &str) -> String {
    let mut s = String::new();
    for ch in name.trim().chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') {
            s.push(ch.to_ascii_lowercase());
        } else if !s.ends_with('-') {
            s.push('-');
        }
    }
    s.trim_matches('-').to_string()
}

/// `'it'\''s'`: safe to pass through the remote shell.
fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn expand_home(p: &str) -> String {
    match p.strip_prefix("~/") {
        Some(rest) => rivet_xdg::places::home().join(rest).to_string_lossy().into_owned(),
        None => p.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_addresses() {
        let c = Connection::from_url("Work site", "sftp://me@example.com:2222/var/www").unwrap();
        assert_eq!((c.protocol, c.host.as_str(), c.port, c.user.as_deref(), c.remote.as_deref()), (Protocol::Sftp, "example.com", Some(2222), Some("me"), Some("/var/www")));
        assert_eq!(c.url(), "sftp://me@example.com:2222/var/www");
        assert_eq!(c.alias(), "work-site");
        let bare = Connection::from_url("pi", "pi@192.168.1.4").unwrap();
        assert_eq!((bare.protocol, bare.port()), (Protocol::Sftp, 22));
        let v6 = Connection::from_url("v6", "ftps://[::1]:990").unwrap();
        assert_eq!((v6.host.as_str(), v6.port), ("::1", Some(990)));
        assert_eq!(v6.url(), "ftps://[::1]:990");
        assert!(Connection::from_url("x", "ftp://me:secret@host").is_err());
        assert!(Connection::from_url("x", "gopher://host").is_err());
    }

    #[test]
    fn builds_ssh_and_sftp_commands() {
        let mut c = Connection::from_url("w", "sftp://me@example.com:2222/var/my site").unwrap();
        c.key = Some("/k/id".into());
        assert_eq!(c.ssh_args(&[]).unwrap(), ["-p", "2222", "-i", "/k/id", "-t", "me@example.com", "cd -- '/var/my site' && exec \"$SHELL\" -l"]);
        assert_eq!(c.ssh_args(&["ls".into(), "-la".into()]).unwrap(), ["-p", "2222", "-i", "/k/id", "me@example.com", "cd -- '/var/my site' && ls -la"]);
        assert_eq!(c.sftp_args().unwrap(), ["-P", "2222", "-i", "/k/id", "me@example.com:/var/my site"]);
        let plain = Connection::from_url("p", "pi@host").unwrap();
        assert_eq!(plain.ssh_args(&["uptime".into()]).unwrap(), ["pi@host", "--", "uptime"]);
        assert!(Connection::from_url("f", "ftp://host").unwrap().ssh_args(&[]).is_err());
    }

    #[test]
    fn saves_and_loads_with_ssh_config() {
        let dir = std::env::temp_dir().join(format!("rivet-connections-test-{}", std::process::id()));
        let path = dir.join("connections.conf");
        let mut s = Store::load_from(&path);
        assert!(s.items.is_empty());
        let mut a = Connection::from_url("Work site", "sftp://me@example.com:2222/var/www").unwrap();
        a.local = Some("~/Sites/work".into());
        s.add(a.clone(), false).unwrap();
        s.add(Connection::from_url("Old FTP", "ftp://files.example.org").unwrap(), false).unwrap();
        assert!(s.add(Connection::from_url("work SITE", "x@y").unwrap(), false).is_err());
        assert!(s.add(Connection::from_url("Work-Site", "x@y").unwrap(), false).is_err(), "same ssh alias");
        s.save().unwrap();
        let back = Store::load_from(&path);
        assert_eq!(back.items, s.items);
        assert_eq!(back.find("work-site").map(|c| c.name.as_str()), Some("Work site"));
        let cfg = std::fs::read_to_string(back.ssh_config_path()).unwrap();
        assert!(cfg.contains("Host work-site\n    HostName example.com\n    User me\n    Port 2222\n"));
        assert!(!cfg.contains("files.example.org"), "FTP isn't ssh");
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        let mut back = back;
        assert!(back.remove("old ftp").is_some());
        assert_eq!(back.items.len(), 1);
        let _ = std::fs::remove_file(back.ssh_config_path());
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("  My Pi (home) "), "my-pi-home");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }
}
