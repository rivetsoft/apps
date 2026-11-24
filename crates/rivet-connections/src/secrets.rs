//! Passwords and key passphrases in the desktop keyring (GNOME Keyring, KWallet, KeePassXC…),
//! through `rivet_xdg::keyring`. Secrets travel over stdin and stdout, never on a command line,
//! and are never written to Rivet FTP's own files.

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
    rivet_xdg::keyring::available()
}

/// The stored secret for a connection, if there is one.
pub fn lookup(connection: &str, kind: Kind) -> Option<String> {
    rivet_xdg::keyring::lookup(&[("service", "rivet-ftp"), ("connection", connection), ("kind", kind.attr())])
}

/// Store (or replace) a connection's secret.
pub fn store(connection: &str, kind: Kind, secret: &str) -> Result<(), String> {
    let what = match kind {
        Kind::Password => "password",
        Kind::Passphrase => "key passphrase",
    };
    rivet_xdg::keyring::store(&format!("Rivet FTP {what} for {connection}"), &[("service", "rivet-ftp"), ("connection", connection), ("kind", kind.attr())], secret)
}

/// Forget a connection's secrets (both kinds).
pub fn clear(connection: &str) {
    for kind in [Kind::Password, Kind::Passphrase] {
        rivet_xdg::keyring::clear(&[("service", "rivet-ftp"), ("connection", connection), ("kind", kind.attr())]);
    }
}

/// Move a connection's secrets to a new name (after a rename).
pub fn rename(old: &str, new: &str) {
    for kind in [Kind::Password, Kind::Passphrase] {
        if let Some(s) = lookup(old, kind)
            && store(new, kind, &s).is_ok()
        {
            rivet_xdg::keyring::clear(&[("service", "rivet-ftp"), ("connection", old), ("kind", kind.attr())]);
        }
    }
}
