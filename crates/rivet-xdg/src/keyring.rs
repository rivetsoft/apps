//! Secrets in the desktop's keyring (GNOME Keyring, KWallet, KeePassXC…), through the Secret
//! Service and libsecret's `secret-tool`. Secrets travel over stdin and stdout, never on a command
//! line, and are never written to an app's own files. An app names its secrets by attributes
//! (`service=rivet-ftp connection=home kind=password`).

use std::io::Write;
use std::process::{Command, Stdio};

/// Whether a keyring can be used here (`secret-tool` is installed).
pub fn available() -> bool {
    Command::new("secret-tool").arg("--version").stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok()
}

fn flat<'a>(attrs: &'a [(&'a str, &'a str)]) -> impl Iterator<Item = &'a str> {
    attrs.iter().flat_map(|(k, v)| [*k, *v])
}

/// The secret with these attributes, if there is one.
pub fn lookup(attrs: &[(&str, &str)]) -> Option<String> {
    let out = Command::new("secret-tool").arg("lookup").args(flat(attrs)).stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8(out.stdout).ok()?;
    let s = s.strip_suffix('\n').unwrap_or(&s).to_string();
    (!s.is_empty()).then_some(s)
}

/// Store (or replace) a secret, shown in the keyring as `label`.
pub fn store(label: &str, attrs: &[(&str, &str)], secret: &str) -> Result<(), String> {
    let mut child = Command::new("secret-tool")
        .args(["store", "--label", label])
        .args(flat(attrs))
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

/// Forget the secret with these attributes.
pub fn clear(attrs: &[(&str, &str)]) {
    let _ = Command::new("secret-tool").arg("clear").args(flat(attrs)).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
}
