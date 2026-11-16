//! Passwords and key passphrases in the desktop keyring (GNOME Keyring, KWallet, KeePassXC…),
//! through the Secret Service and libsecret's `secret-tool`. Secrets travel over stdin and stdout,
//! never on a command line, and are never written to Rivet FTP's own files.

use std::io::Write;
use std::process::{Command, Stdio};

/// What a stored secret unlocks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The server login password.
    Password,
    /// The passphrase of the connection's SSH key.
    Passphrase,
}

impl Kind {
    fn attr(self) -> &'static str {
        match self {
            Kind::Password => "password",
            Kind::Passphrase => "passphrase",
        }
    }
}

/// Whether a keyring can be used here (`secret-tool` is installed).
pub fn available() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

/// The stored secret for a connection, if there is one.
pub fn lookup(connection: &str, kind: Kind) -> Option<String> {
    let out = Command::new("secret-tool")
        .args(["lookup", "service", "rivet-ftp", "connection", connection, "kind", kind.attr()])
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.strip_suffix('\n').unwrap_or(&s).to_string();
    (!s.is_empty()).then_some(s)
}

/// Store (or replace) a connection's secret.
pub fn store(connection: &str, kind: Kind, secret: &str) -> Result<(), String> {
    let what = match kind {
        Kind::Password => "password",
        Kind::Passphrase => "key passphrase",
    };
    let label = format!("Rivet FTP {what} for {connection}");
    let mut child = Command::new("secret-tool")
        .args(["store", "--label", &label, "service", "rivet-ftp", "connection", connection, "kind", kind.attr()])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| "Saving passwords needs secret-tool (from libsecret) and a keyring.".to_string())?;
    child.stdin.take().ok_or("no stdin")?.write_all(secret.as_bytes()).map_err(|e| e.to_string())?;
    let out = child.wait_with_output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&out.stderr);
        Err(err.lines().next().map(str::to_string).filter(|l| !l.is_empty()).unwrap_or_else(|| "The keyring didn't accept it.".into()))
    }
}

/// Forget a connection's secrets (both kinds).
pub fn clear(connection: &str) {
    for kind in [Kind::Password, Kind::Passphrase] {
        let _ = Command::new("secret-tool")
            .args(["clear", "service", "rivet-ftp", "connection", connection, "kind", kind.attr()])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

/// Move a connection's secrets to a new name (after a rename).
pub fn rename(old: &str, new: &str) {
    for kind in [Kind::Password, Kind::Passphrase] {
        if let Some(s) = lookup(old, kind)
            && store(new, kind, &s).is_ok()
        {
            let _ = Command::new("secret-tool")
                .args(["clear", "service", "rivet-ftp", "connection", old, "kind", kind.attr()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}
