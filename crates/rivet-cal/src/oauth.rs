//! Signing in to Google and Microsoft: OAuth 2.0 for a program on the desktop (RFC 8252), in the
//! browser. A port on this computer's loopback address hears the browser come back with a code,
//! the code is swapped for tokens, with PKCE so a code caught on the way is no use to anyone else.
//! The long-lived token is kept in the keyring; the short one that requests carry is kept only in
//! memory, and asked for again as it runs out.
//!
//! Rivet signs in as its own registered app, whose client IDs a release build is given. Someone
//! can use their own instead (their own Google Cloud project or Microsoft Entra registration),
//! kept with the account.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::caldav::{Login, request};

/// Who an account signs in with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Provider {
    Google,
    Microsoft,
}

/// An app registered with a provider: its client ID, and for Google the secret that goes with it
/// (Google gives desktop apps one too, though it can't be kept secret in a program anyone can
/// read; PKCE is what protects the sign-in).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Client {
    pub id: String,
    pub secret: String,
}

/// Rivet's own registrations, set when a release is built:
/// `RIVET_GOOGLE_CLIENT_ID`, `RIVET_GOOGLE_CLIENT_SECRET` and `RIVET_MICROSOFT_CLIENT_ID`.
/// `docs/calendar-accounts.md` says how they're registered.
const GOOGLE_ID: Option<&str> = option_env!("RIVET_GOOGLE_CLIENT_ID");
const GOOGLE_SECRET: Option<&str> = option_env!("RIVET_GOOGLE_CLIENT_SECRET");
const MICROSOFT_ID: Option<&str> = option_env!("RIVET_MICROSOFT_CLIENT_ID");

impl Provider {
    pub fn name(self) -> &'static str {
        match self {
            Provider::Google => "Google",
            Provider::Microsoft => "Microsoft",
        }
    }

    /// As the accounts file writes it.
    pub fn key(self) -> &'static str {
        match self {
            Provider::Google => "google",
            Provider::Microsoft => "microsoft",
        }
    }

    pub fn from_key(k: &str) -> Option<Provider> {
        match k {
            "google" => Some(Provider::Google),
            "microsoft" => Some(Provider::Microsoft),
            _ => None,
        }
    }

    /// Rivet's own registration, when this build has one.
    pub fn builtin(self) -> Option<Client> {
        let (id, secret) = match self {
            Provider::Google => (GOOGLE_ID, GOOGLE_SECRET),
            Provider::Microsoft => (MICROSOFT_ID, None),
        };
        let id = id.map(str::trim).filter(|i| !i.is_empty())?;
        Some(Client { id: id.to_string(), secret: secret.unwrap_or("").trim().to_string() })
    }

    /// Whether this provider's apps need a client secret as well as the ID.
    pub fn wants_secret(self) -> bool {
        self == Provider::Google
    }

    fn auth_url(self) -> &'static str {
        match self {
            Provider::Google => "https://accounts.google.com/o/oauth2/v2/auth",
            Provider::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/authorize",
        }
    }

    fn token_url(self) -> &'static str {
        match self {
            Provider::Google => "https://oauth2.googleapis.com/token",
            Provider::Microsoft => "https://login.microsoftonline.com/common/oauth2/v2.0/token",
        }
    }

    fn scope(self) -> &'static str {
        match self {
            Provider::Google => "https://www.googleapis.com/auth/calendar",
            Provider::Microsoft => "offline_access Calendars.ReadWrite User.Read",
        }
    }

    /// Where the browser is sent back to. Microsoft takes `localhost` with any port for a
    /// desktop app; Google takes the loopback address with any port.
    fn redirect(self, port: u16) -> String {
        match self {
            Provider::Google => format!("http://127.0.0.1:{port}"),
            Provider::Microsoft => format!("http://localhost:{port}"),
        }
    }
}

/// What a sign-in gives.
#[derive(Clone, Debug, Default)]
pub struct Tokens {
    pub access: String,
    pub refresh: String,
    pub expires: u64,
}

/// Percent-encoding for a query or a form.
pub fn encode(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{}={}", encode(k), encode(v))).collect::<Vec<_>>().join("&")
}

/// Bytes from the system's random source.
fn random(n: usize) -> Vec<u8> {
    let mut buf = vec![0u8; n];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut buf);
    }
    buf
}

/// Base64 for addresses, with no padding (RFC 4648 §5).
fn b64url(data: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk.iter().enumerate().fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

/// SHA-256 (FIPS 180-4), for the PKCE challenge.
fn sha256(data: &[u8]) -> [u8; 32] {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
    ];
    let mut h: [u32; 8] = [0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19];
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend_from_slice(&((data.len() as u64) * 8).to_be_bytes());
    for block in msg.chunks(64) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = u32::from_be_bytes([block[4 * i], block[4 * i + 1], block[4 * i + 2], block[4 * i + 3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut hh] = h;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ (!e & g);
            let t1 = hh.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            hh = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d, e, f, g, hh]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 32];
    for (i, x) in h.iter().enumerate() {
        out[4 * i..4 * i + 4].copy_from_slice(&x.to_be_bytes());
    }
    out
}

/// A query string's values (`a=1&b=x%20y`).
fn query(q: &str) -> HashMap<String, String> {
    q.split('&').filter_map(|p| p.split_once('=')).map(|(k, v)| (k.to_string(), rivet_xdg::places::percent_decode(&v.replace('+', " ")))).collect()
}

/// The page the browser shows after coming back.
fn answer(stream: &mut TcpStream, ok: bool) {
    let (title, line) = if ok { ("Signed in", "Rivet Calendar is signed in. You can close this tab.") } else { ("Not signed in", "Rivet Calendar wasn't signed in. You can close this tab and try again.") };
    let body = format!("<!doctype html><meta charset=utf-8><title>{title}</title><body style=\"font:16px system-ui,sans-serif;display:grid;place-items:center;height:90vh;margin:0\"><div><h1 style=\"font-size:22px\">{title}</h1><p>{line}</p></div>");
    let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

/// The token endpoint's answer, or why there's none.
fn token_call(p: Provider, pairs: &[(&str, &str)]) -> Result<Value, String> {
    let r = request("POST", p.token_url(), &Login::default(), &[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")], Some(&form(pairs)))?;
    let v: Value = serde_json::from_str(&r.body).unwrap_or(Value::Null);
    if let Some(e) = v.get("error").and_then(Value::as_str) {
        if e == "invalid_grant" {
            return Err(format!("{} signed this account out: remove it and add it again.", p.name()));
        }
        let why = v.get("error_description").and_then(Value::as_str).unwrap_or(e);
        return Err(format!("{} didn't sign in: {}", p.name(), why.lines().next().unwrap_or(e)));
    }
    if r.status != 200 {
        return Err(format!("{} answered {} when signing in.", p.name(), r.status));
    }
    Ok(v)
}

fn tokens_of(v: &Value) -> Tokens {
    let s = |k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    Tokens { access: s("access_token"), refresh: s("refresh_token"), expires: v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600) }
}

/// Sign in in the browser: open the provider's page and wait (up to 5 minutes, or until `cancel`)
/// for it to send the browser back here.
pub fn sign_in(p: Provider, client: &Client, cancel: &AtomicBool) -> Result<Tokens, String> {
    if client.id.trim().is_empty() {
        return Err(format!("This build has no {} client ID: use your own.", p.name()));
    }
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Couldn't wait for the browser: {e}"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    // `localhost` may be looked up as ::1 first; hear that too when the port is free there.
    let v6 = TcpListener::bind(("::1", port)).ok();
    for l in std::iter::once(&listener).chain(v6.as_ref()) {
        l.set_nonblocking(true).map_err(|e| e.to_string())?;
    }
    let verifier = b64url(&random(32));
    let challenge = b64url(&sha256(verifier.as_bytes()));
    let state = b64url(&random(16));
    let redirect = p.redirect(port);
    let mut pairs = vec![("client_id", client.id.as_str()), ("redirect_uri", redirect.as_str()), ("response_type", "code"), ("scope", p.scope()), ("state", state.as_str()), ("code_challenge", challenge.as_str()), ("code_challenge_method", "S256")];
    if p == Provider::Google {
        // A refresh token every time, so a second sign-in still gets one.
        pairs.extend([("access_type", "offline"), ("prompt", "consent")]);
    } else {
        pairs.push(("prompt", "select_account"));
    }
    let url = format!("{}?{}", p.auth_url(), form(&pairs));
    rivet_xdg::launch::open_url(&url).map_err(|e| format!("Couldn't open the browser: {e}"))?;
    let until = Instant::now() + Duration::from_secs(300);
    let code = loop {
        if cancel.load(Ordering::Relaxed) {
            return Err("Signing in was stopped.".into());
        }
        if Instant::now() > until {
            return Err("Signing in took too long: try again.".into());
        }
        let got = std::iter::once(&listener).chain(v6.as_ref()).find_map(|l| l.accept().ok());
        let Some((mut stream, _)) = got else {
            std::thread::sleep(Duration::from_millis(150));
            continue;
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut line = String::new();
        let _ = BufReader::new(&stream).read_line(&mut line);
        // `GET /?code=…&state=… HTTP/1.1`; anything else (a favicon) is turned away.
        let target = line.split_whitespace().nth(1).unwrap_or("");
        let Some((_, q)) = target.split_once('?') else {
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        };
        let q = query(q);
        if q.get("state") != Some(&state) {
            answer(&mut stream, false);
            continue;
        }
        match (q.get("code"), q.get("error")) {
            (Some(c), _) => {
                answer(&mut stream, true);
                break c.clone();
            }
            (None, Some(e)) => {
                answer(&mut stream, false);
                return Err(if e == "access_denied" { "Signing in was turned down.".into() } else { format!("{} didn't sign in: {e}", p.name()) });
            }
            (None, None) => answer(&mut stream, false),
        }
    };
    let v = token_call(p, &[("client_id", &client.id), ("client_secret", &client.secret), ("code", &code), ("code_verifier", &verifier), ("grant_type", "authorization_code"), ("redirect_uri", &redirect)])?;
    let t = tokens_of(&v);
    if t.refresh.is_empty() {
        return Err(format!("{} didn't give Rivet a way to stay signed in.", p.name()));
    }
    Ok(t)
}

/// Access tokens in this process, by account: the token and when it runs out.
fn cache() -> &'static Mutex<HashMap<String, (String, Instant)>> {
    static C: OnceLock<Mutex<HashMap<String, (String, Instant)>>> = OnceLock::new();
    C.get_or_init(Default::default)
}

/// Keep a token just got for an account (as a sign-in gives one).
pub fn remember(account: &str, t: &Tokens) {
    if let Ok(mut c) = cache().lock() {
        c.insert(account.to_string(), (t.access.clone(), Instant::now() + Duration::from_secs(t.expires.saturating_sub(60))));
    }
}

/// A current access token for an account, from its refresh token. A provider that hands out a
/// new refresh token with it (Microsoft) has that one kept instead.
pub fn access_token(account: &str, p: Provider, client: &Client, refresh: &str, keep: impl FnOnce(&str)) -> Result<String, String> {
    if let Ok(c) = cache().lock()
        && let Some((t, until)) = c.get(account)
        && Instant::now() < *until
    {
        return Ok(t.clone());
    }
    let mut pairs = vec![("client_id", client.id.as_str()), ("client_secret", client.secret.as_str()), ("refresh_token", refresh), ("grant_type", "refresh_token")];
    if p == Provider::Microsoft {
        pairs.push(("scope", p.scope()));
    }
    let t = tokens_of(&token_call(p, &pairs)?);
    if !t.refresh.is_empty() && t.refresh != refresh {
        keep(&t.refresh);
    }
    remember(account, &t);
    Ok(t.access)
}

/// Forget an account's access token (it was turned down).
pub fn forget(account: &str) {
    if let Ok(mut c) = cache().lock() {
        c.remove(account);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_and_base64_match_the_standards() {
        // RFC 7636's own example.
        assert_eq!(b64url(&sha256(b"dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk")), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
        assert_eq!(b64url(&sha256(b"")), "47DEQpj8HBSa-_TImW-5JCeuQeRkm5NMpJWZG3hSuFU");
        assert_eq!(b64url(b"f"), "Zg");
        assert_eq!(b64url(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn forms_and_queries_round_trip() {
        assert_eq!(form(&[("a", "x y&z"), ("empty", ""), ("b", "1")]), "a=x%20y%26z&b=1");
        let q = query("code=4%2F0A&state=abc");
        assert_eq!(q["code"], "4/0A");
        assert_eq!(q["state"], "abc");
    }
}
