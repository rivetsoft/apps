//! Signing in through greetd: its protocol over $GREETD_SOCK, JSON messages each after its
//! length (four bytes, in this computer's byte order).

use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;

use serde_json::json;

fn send(sock: &mut UnixStream, msg: &serde_json::Value) -> Result<serde_json::Value, String> {
    let body = msg.to_string();
    sock.write_all(&(body.len() as u32).to_ne_bytes()).and_then(|_| sock.write_all(body.as_bytes())).map_err(|e| format!("greetd stopped answering ({e})"))?;
    let mut len = [0u8; 4];
    sock.read_exact(&mut len).map_err(|e| format!("greetd stopped answering ({e})"))?;
    let mut buf = vec![0u8; u32::from_ne_bytes(len) as usize];
    sock.read_exact(&mut buf).map_err(|e| format!("greetd stopped answering ({e})"))?;
    serde_json::from_slice(&buf).map_err(|e| format!("greetd's answer didn't make sense ({e})"))
}

/// Sign in as `login` with `password`, then have greetd start `session`. On success greetd
/// starts it when the greeter leaves; on failure the attempt is cancelled, ready for another.
pub fn sign_in(login: &str, password: &str, session: &[String]) -> Result<(), String> {
    let path = std::env::var("GREETD_SOCK").map_err(|_| "Not started by greetd, so there's no one to sign in with".to_string())?;
    sign_in_at(Path::new(&path), login, password, session)
}

fn sign_in_at(path: &Path, login: &str, password: &str, session: &[String]) -> Result<(), String> {
    let mut sock = UnixStream::connect(path).map_err(|e| format!("Can't reach greetd ({e})"))?;
    let mut reply = send(&mut sock, &json!({ "type": "create_session", "username": login }))?;
    let mut answered = false;
    loop {
        match reply["type"].as_str() {
            Some("success") => break,
            Some("auth_message") => {
                // The password once; information and errors are acknowledged with no answer.
                let secret = matches!(reply["auth_message_type"].as_str(), Some("secret" | "visible"));
                let response = if secret && !answered {
                    answered = true;
                    json!(password)
                } else if secret {
                    // A second question (a code, say) this screen can't ask.
                    let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
                    return Err("This account asks for more than a password".into());
                } else {
                    serde_json::Value::Null
                };
                reply = send(&mut sock, &json!({ "type": "post_auth_message_response", "response": response }))?;
            }
            _ => {
                let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
                return Err(if reply["error_type"].as_str() == Some("auth_error") { "Wrong password".into() } else { reply["description"].as_str().unwrap_or("greetd refused").to_string() });
            }
        }
    }
    if session.is_empty() {
        let _ = send(&mut sock, &json!({ "type": "cancel_session" }));
        return Err("No session to start: install a Wayland desktop".into());
    }
    let reply = send(&mut sock, &json!({ "type": "start_session", "cmd": session, "env": [] }))?;
    if reply["type"].as_str() == Some("success") { Ok(()) } else { Err(reply["description"].as_str().unwrap_or("greetd couldn't start the session").to_string()) }
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    /// A greetd that wants `password` (and, with `code`, a one-time code after it), and
    /// answers each message as greetd would. Gives back the kinds of message it was sent.
    pub fn fake_greetd(path: &Path, password: &'static str, code: bool) -> std::thread::JoinHandle<Vec<String>> {
        let listener = UnixListener::bind(path).unwrap();
        std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut seen = Vec::new();
            let mut asked = 0;
            loop {
                let mut len = [0u8; 4];
                if s.read_exact(&mut len).is_err() {
                    return seen;
                }
                let mut buf = vec![0u8; u32::from_ne_bytes(len) as usize];
                s.read_exact(&mut buf).unwrap();
                let m: serde_json::Value = serde_json::from_slice(&buf).unwrap();
                let kind = m["type"].as_str().unwrap().to_string();
                let reply = match kind.as_str() {
                    "create_session" => json!({ "type": "auth_message", "auth_message_type": "info", "auth_message": "Welcome" }),
                    "post_auth_message_response" if asked == 0 => {
                        asked += 1;
                        json!({ "type": "auth_message", "auth_message_type": "secret", "auth_message": "Password:" })
                    }
                    "post_auth_message_response" if m["response"] != password && asked == 1 => json!({ "type": "error", "error_type": "auth_error", "description": "pam" }),
                    "post_auth_message_response" if code && asked == 1 => {
                        asked += 1;
                        json!({ "type": "auth_message", "auth_message_type": "secret", "auth_message": "Code:" })
                    }
                    "start_session" if m["cmd"].as_array().is_none_or(|c| c.is_empty()) => json!({ "type": "error", "error_type": "error", "description": "no command" }),
                    _ => json!({ "type": "success" }),
                };
                seen.push(kind);
                let body = reply.to_string();
                s.write_all(&(body.len() as u32).to_ne_bytes()).unwrap();
                s.write_all(body.as_bytes()).unwrap();
            }
        })
    }

    fn sock(name: &str) -> (std::path::PathBuf, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("rivet-greetd-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (dir.join("sock"), dir)
    }

    #[test]
    fn signs_in_and_starts_the_session() {
        let (sock, dir) = sock("ok");
        let server = fake_greetd(&sock, "right", false);
        assert_eq!(sign_in_at(&sock, "sam", "right", &["niri-session".into()]), Ok(()));
        assert_eq!(server.join().unwrap(), ["create_session", "post_auth_message_response", "post_auth_message_response", "start_session"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_wrong_password_cancels() {
        let (sock, dir) = sock("bad");
        let server = fake_greetd(&sock, "right", false);
        assert_eq!(sign_in_at(&sock, "sam", "wrong", &["niri-session".into()]), Err("Wrong password".into()));
        assert_eq!(server.join().unwrap(), ["create_session", "post_auth_message_response", "post_auth_message_response", "cancel_session"]);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_second_question_cancels() {
        let (sock, dir) = sock("code");
        let server = fake_greetd(&sock, "right", true);
        assert_eq!(sign_in_at(&sock, "sam", "right", &["niri-session".into()]), Err("This account asks for more than a password".into()));
        assert_eq!(server.join().unwrap().last().map(String::as_str), Some("cancel_session"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_session_cancels() {
        let (sock, dir) = sock("none");
        let server = fake_greetd(&sock, "right", false);
        assert_eq!(sign_in_at(&sock, "sam", "right", &[]), Err("No session to start: install a Wayland desktop".into()));
        assert_eq!(server.join().unwrap().last().map(String::as_str), Some("cancel_session"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
