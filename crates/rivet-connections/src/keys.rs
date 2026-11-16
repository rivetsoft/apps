//! SSH keys in `~/.ssh`: find them, describe them, and make new ones.
//!
//! Everything here is local. A private key is opened only to see whether it has a passphrase
//! (the format says so in plain text); fingerprints and new keys come from OpenSSH's own
//! `ssh-keygen`, and the agent's keys from `ssh-add -l`.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq)]
pub struct SshKey {
    /// The private key file.
    pub path: PathBuf,
    /// Its file name ("id_ed25519").
    pub name: String,
    /// "ED25519", "RSA", "ECDSA"…
    pub kind: String,
    /// Key size in bits, as ssh-keygen reports it.
    pub bits: u32,
    /// The comment at the end of the public key (often user@host).
    pub comment: String,
    /// `SHA256:…`, or empty when it couldn't be read.
    pub fingerprint: String,
    /// The public key line, from the `.pub` file beside it.
    pub public: Option<String>,
    /// Protected by a passphrase (None: couldn't tell).
    pub passphrase: Option<bool>,
    /// Loaded in the running ssh agent.
    pub in_agent: bool,
}

impl SshKey {
    /// `~/.ssh/id_ed25519`, the way connections store it.
    pub fn short_path(&self) -> String {
        tilde(&self.path)
    }

    /// "ED25519" or "RSA 4096".
    pub fn kind_label(&self) -> String {
        if self.kind == "RSA" && self.bits > 0 { format!("RSA {}", self.bits) } else { self.kind.clone() }
    }
}

/// `~/.ssh`
pub fn ssh_dir() -> PathBuf {
    rivet_xdg::places::home().join(".ssh")
}

/// The keys in `~/.ssh`, sorted by name.
pub fn list() -> Vec<SshKey> {
    let agent = agent_fingerprints();
    list_in(&ssh_dir(), &agent)
}

/// The private keys in a folder, sorted by name.
pub fn list_in(dir: &Path, agent: &[String]) -> Vec<SshKey> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut keys: Vec<SshKey> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_none_or(|x| x != "pub"))
        .filter_map(|p| read_key(&p, agent))
        .collect();
    keys.sort_by(|a, b| a.name.cmp(&b.name));
    keys
}

fn read_key(path: &Path, agent: &[String]) -> Option<SshKey> {
    // Private keys start with a PEM header; read just enough to see it.
    let text = read_head(path, 64 * 1024)?;
    if !(text.starts_with("-----BEGIN") && text.lines().next().is_some_and(|l| l.contains("PRIVATE KEY"))) {
        return None;
    }
    let pub_path = PathBuf::from(format!("{}.pub", path.display()));
    let public = std::fs::read_to_string(&pub_path).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let mut key = SshKey {
        path: path.to_path_buf(),
        name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
        kind: String::new(),
        bits: 0,
        comment: String::new(),
        fingerprint: String::new(),
        public: public.clone(),
        passphrase: has_passphrase(&text),
        in_agent: false,
    };
    // The comment and type are in the public key line: "ssh-ed25519 AAAA… me@host".
    if let Some(p) = &public {
        let mut parts = p.splitn(3, ' ');
        key.kind = kind_from_algo(parts.next().unwrap_or(""));
        parts.next();
        key.comment = parts.next().unwrap_or("").to_string();
    }
    // Fingerprint and size from ssh-keygen: "256 SHA256:abc… comment (ED25519)". The public
    // file is enough; without one, ssh-keygen reads the private key (no passphrase needed).
    let source = if public.is_some() { pub_path } else { path.to_path_buf() };
    if let Some(line) = run_text("ssh-keygen", &["-l", "-f", &source.to_string_lossy()]) {
        let line = line.trim();
        let mut it = line.split_whitespace();
        key.bits = it.next().and_then(|b| b.parse().ok()).unwrap_or(0);
        key.fingerprint = it.next().unwrap_or("").to_string();
        if let (Some(open), true) = (line.rfind('('), line.ends_with(')')) {
            key.kind = line[open + 1..line.len() - 1].to_string();
        }
    }
    if key.kind.is_empty() {
        key.kind = "Key".into();
    }
    key.in_agent = !key.fingerprint.is_empty() && agent.contains(&key.fingerprint);
    Some(key)
}

fn read_head(path: &Path, max: u64) -> Option<String> {
    use std::io::Read;
    let f = std::fs::File::open(path).ok()?;
    if f.metadata().ok()?.len() > max {
        return None;
    }
    let mut s = String::new();
    f.take(max).read_to_string(&mut s).ok()?;
    Some(s)
}

fn kind_from_algo(a: &str) -> String {
    match a {
        "ssh-ed25519" => "ED25519".into(),
        "ssh-rsa" => "RSA".into(),
        "ecdsa-sha2-nistp256" | "ecdsa-sha2-nistp384" | "ecdsa-sha2-nistp521" => "ECDSA".into(),
        "sk-ssh-ed25519@openssh.com" => "ED25519-SK".into(),
        "sk-ecdsa-sha2-nistp256@openssh.com" => "ECDSA-SK".into(),
        _ => String::new(),
    }
}

/// Whether a private key file is protected by a passphrase. Old PEM keys say so in a header;
/// OpenSSH keys name their cipher ("none" when unprotected) right after the magic bytes.
pub fn has_passphrase(text: &str) -> Option<bool> {
    if text.contains("Proc-Type: 4,ENCRYPTED") || text.contains("BEGIN ENCRYPTED PRIVATE KEY") {
        return Some(true);
    }
    if !text.contains("BEGIN OPENSSH PRIVATE KEY") {
        // Unencrypted PEM (RSA, EC, PKCS#8).
        return Some(false);
    }
    let body: String = text.lines().filter(|l| !l.starts_with("-----")).collect();
    let bytes = base64_decode(&body)?;
    let magic = b"openssh-key-v1\0";
    let rest = bytes.strip_prefix(magic.as_slice())?;
    let len = u32::from_be_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let cipher = rest.get(4..4 + len)?;
    Some(cipher != b"none")
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    let val = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let clean: Vec<u8> = s.bytes().filter(|c| !c.is_ascii_whitespace() && *c != b'=').collect();
    let mut out = Vec::with_capacity(clean.len() * 3 / 4);
    for chunk in clean.chunks(4) {
        let mut n = 0u32;
        for (i, c) in chunk.iter().enumerate() {
            n |= val(*c)? << (18 - 6 * i);
        }
        let bytes = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        out.extend_from_slice(&bytes[..chunk.len().saturating_sub(1)]);
    }
    Some(out)
}

/// Fingerprints of the keys loaded in the ssh agent (empty without an agent).
pub fn agent_fingerprints() -> Vec<String> {
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Vec::new();
    }
    run_text("ssh-add", &["-l"]).map(|t| t.lines().filter_map(|l| l.split_whitespace().nth(1).map(str::to_string)).filter(|f| f.starts_with("SHA256:")).collect()).unwrap_or_default()
}

fn run_text(cmd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new(cmd).args(args).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyType {
    Ed25519,
    Rsa4096,
}

/// A key name that's safe as a file name in `~/.ssh` (letters, digits, `._-`).
pub fn valid_name(name: &str) -> Result<(), String> {
    if name.is_empty() {
        return Err("Give the key a file name.".into());
    }
    if name.starts_with('.') || name.ends_with(".pub") || !name.chars().all(|c| c.is_ascii_alphanumeric() || "._-".contains(c)) {
        return Err("Use letters, numbers, dots, dashes and underscores for the file name.".into());
    }
    if ["config", "known_hosts", "authorized_keys", "environment"].contains(&name) {
        return Err(format!("\"{name}\" is one of ssh's own files."));
    }
    Ok(())
}

/// Make a new key pair in `dir` with ssh-keygen. Refuses to replace an existing file. An
/// empty passphrase leaves the key unprotected.
pub fn generate(dir: &Path, name: &str, kind: KeyType, comment: &str, passphrase: &str) -> Result<PathBuf, String> {
    valid_name(name)?;
    let path = dir.join(name);
    if path.exists() || PathBuf::from(format!("{}.pub", path.display())).exists() {
        return Err(format!("There's already a key called \"{name}\"."));
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // ~/.ssh must not be readable by others, or ssh refuses to use it.
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(m) = std::fs::metadata(dir)
            && m.permissions().mode() & 0o077 != 0
            && dir == ssh_dir()
        {
            let _ = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700));
        }
    }
    let mut args: Vec<String> = vec!["-q".into(), "-f".into(), path.to_string_lossy().into_owned(), "-C".into(), comment.to_string()];
    match kind {
        KeyType::Ed25519 => args.extend(["-t".into(), "ed25519".into()]),
        KeyType::Rsa4096 => args.extend(["-t".into(), "rsa".into(), "-b".into(), "4096".into()]),
    }
    // ssh-keygen only takes a new passphrase on its command line (-N); it's gone again in
    // well under a second.
    args.extend(["-N".into(), passphrase.to_string()]);
    let out = Command::new("ssh-keygen").args(&args).stdin(Stdio::null()).output().map_err(|e| format!("couldn't run ssh-keygen ({e}); is OpenSSH installed?"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(err.lines().last().unwrap_or("ssh-keygen failed").to_string());
    }
    Ok(path)
}

/// The usual comment for a new key: user@host.
pub fn default_comment() -> String {
    let user = std::env::var("USER").unwrap_or_default();
    let host = ["/etc/hostname", "/proc/sys/kernel/hostname"].iter().find_map(|p| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())).unwrap_or_default();
    match (user.is_empty(), host.is_empty()) {
        (false, false) => format!("{user}@{host}"),
        (false, true) => user,
        _ => String::new(),
    }
}

/// A free file name for a new key: id_ed25519, then id_ed25519_2, …
pub fn suggest_name(dir: &Path, kind: KeyType) -> String {
    let base = match kind {
        KeyType::Ed25519 => "id_ed25519",
        KeyType::Rsa4096 => "id_rsa",
    };
    let free = |n: &str| !dir.join(n).exists() && !dir.join(format!("{n}.pub")).exists();
    if free(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{base}_{i}")).find(|n| free(n)).unwrap_or_default()
}

/// The command (for a terminal) that adds a public key to a server's authorized_keys.
pub fn copy_id_command(key: &SshKey, c: &crate::Connection) -> String {
    let q = |s: &str| format!("'{}'", s.replace('\'', r"'\''"));
    let public = format!("{}.pub", key.path.display());
    let mut cmd = format!("ssh-copy-id -i {}", q(&public));
    if let Some(p) = c.port.filter(|p| *p != 22) {
        cmd.push_str(&format!(" -p {p}"));
    }
    cmd.push(' ');
    cmd.push_str(&q(&c.target()));
    cmd
}

fn tilde(p: &Path) -> String {
    let home = rivet_xdg::places::home();
    match p.strip_prefix(&home) {
        Ok(rest) if !home.as_os_str().is_empty() => format!("~/{}", rest.display()),
        _ => p.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn makes_and_reads_keys() {
        let dir = std::env::temp_dir().join(format!("rivet-keys-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        // Not keys: ssh's own files and random text are skipped.
        std::fs::write(dir.join("config"), "Host x\n").unwrap();
        std::fs::write(dir.join("notes"), "hello\n").unwrap();
        let a = generate(&dir, "plain", KeyType::Ed25519, "me@test", "").unwrap();
        let b = generate(&dir, "locked", KeyType::Ed25519, "me@test", "correct horse").unwrap();
        assert!(generate(&dir, "plain", KeyType::Ed25519, "", "").is_err(), "never replaces a key");
        assert!(generate(&dir, "../escape", KeyType::Ed25519, "", "").is_err());
        let keys = list_in(&dir, &[]);
        assert_eq!(keys.iter().map(|k| k.name.as_str()).collect::<Vec<_>>(), ["locked", "plain"]);
        let plain = keys.iter().find(|k| k.path == a).unwrap();
        let locked = keys.iter().find(|k| k.path == b).unwrap();
        assert_eq!((plain.passphrase, locked.passphrase), (Some(false), Some(true)));
        assert_eq!(plain.kind, "ED25519");
        assert_eq!(plain.comment, "me@test");
        assert!(plain.fingerprint.starts_with("SHA256:"));
        assert!(plain.public.as_deref().is_some_and(|p| p.starts_with("ssh-ed25519 ")));
        // The agent check matches by fingerprint.
        let with_agent = list_in(&dir, std::slice::from_ref(&plain.fingerprint));
        assert!(with_agent.iter().find(|k| k.name == "plain").unwrap().in_agent);
        assert_eq!(suggest_name(&dir, KeyType::Ed25519), "id_ed25519");
        for f in ["plain", "plain.pub", "locked", "locked.pub", "config", "notes"] {
            let _ = std::fs::remove_file(dir.join(f));
        }
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn names_and_commands() {
        assert!(valid_name("id_work").is_ok());
        assert!(valid_name("config").is_err() && valid_name("a b").is_err() && valid_name(".x").is_err() && valid_name("k.pub").is_err());
        let key = SshKey { path: PathBuf::from("/h/.ssh/id_x"), name: "id_x".into(), kind: "ED25519".into(), bits: 256, comment: String::new(), fingerprint: String::new(), public: None, passphrase: None, in_agent: false };
        let c = crate::Connection::from_url("w", "sftp://me@example.com:2222/var").unwrap();
        assert_eq!(copy_id_command(&key, &c), "ssh-copy-id -i '/h/.ssh/id_x.pub' -p 2222 'me@example.com'");
        assert_eq!(base64_decode("aGVsbG8=").unwrap(), b"hello");
    }
}
