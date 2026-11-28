//! Checking the person's password with PAM, as the lock screen does (Rivet Greeter signs in
//! through greetd instead). PAM is called directly (libpam), on a thread of the caller's
//! choosing: it can take seconds (a wrong password waits on purpose).
//!
//! The PAM service is `rivet-shell` when /etc/pam.d/rivet-shell exists (for anyone who wants
//! their own rules), else `login`, which every distribution has.

use std::ffi::{CStr, CString, c_char, c_int, c_void};

const PAM_SUCCESS: c_int = 0;
const PAM_BUF_ERR: c_int = 5;
const PAM_CONV_ERR: c_int = 19;
const PAM_PROMPT_ECHO_OFF: c_int = 1;
const PAM_PROMPT_ECHO_ON: c_int = 2;
const PAM_ERROR_MSG: c_int = 3;
const PAM_TEXT_INFO: c_int = 4;

#[repr(C)]
struct PamMessage {
    msg_style: c_int,
    msg: *const c_char,
}

#[repr(C)]
struct PamResponse {
    resp: *mut c_char,
    resp_retcode: c_int,
}

type ConvFn = extern "C" fn(c_int, *mut *const PamMessage, *mut *mut PamResponse, *mut c_void) -> c_int;

#[repr(C)]
struct PamConv {
    conv: ConvFn,
    appdata_ptr: *mut c_void,
}

#[link(name = "pam")]
unsafe extern "C" {
    fn pam_start(service: *const c_char, user: *const c_char, conv: *const PamConv, pamh: *mut *mut c_void) -> c_int;
    fn pam_authenticate(pamh: *mut c_void, flags: c_int) -> c_int;
    fn pam_end(pamh: *mut c_void, status: c_int) -> c_int;
    fn pam_strerror(pamh: *mut c_void, errnum: c_int) -> *const c_char;
}

/// What the conversation answers with, and the messages PAM shows along the way.
struct Talk {
    user: CString,
    password: CString,
    said: Vec<String>,
}

extern "C" fn converse(n: c_int, msgs: *mut *const PamMessage, out: *mut *mut PamResponse, data: *mut c_void) -> c_int {
    if n <= 0 || msgs.is_null() || out.is_null() || data.is_null() {
        return PAM_CONV_ERR;
    }
    // SAFETY: `data` is the Talk that check() handed to pam_start, alive for the whole call;
    // `msgs` holds `n` messages (Linux-PAM's layout: an array of pointers).
    let talk = unsafe { &mut *(data as *mut Talk) };
    let replies = unsafe { libc::calloc(n as usize, std::mem::size_of::<PamResponse>()) } as *mut PamResponse;
    if replies.is_null() {
        return PAM_BUF_ERR;
    }
    for i in 0..n as usize {
        let m = unsafe { &**msgs.add(i) };
        let text = if m.msg.is_null() { String::new() } else { unsafe { CStr::from_ptr(m.msg) }.to_string_lossy().into_owned() };
        let answer: Option<&CStr> = match m.msg_style {
            PAM_PROMPT_ECHO_OFF => Some(&talk.password),
            PAM_PROMPT_ECHO_ON => Some(&talk.user),
            PAM_ERROR_MSG | PAM_TEXT_INFO => {
                if !text.trim().is_empty() {
                    talk.said.push(text.trim().to_string());
                }
                None
            }
            _ => {
                unsafe { free_replies(replies, i) };
                return PAM_CONV_ERR;
            }
        };
        if let Some(a) = answer {
            // PAM frees the answers itself, with free(): they must come from malloc.
            let dup = unsafe { libc::strdup(a.as_ptr()) };
            if dup.is_null() {
                unsafe { free_replies(replies, i) };
                return PAM_BUF_ERR;
            }
            unsafe { (*replies.add(i)).resp = dup };
        }
    }
    unsafe { *out = replies };
    PAM_SUCCESS
}

/// Free the first `n` answers (wiping the password first) and the array.
unsafe fn free_replies(replies: *mut PamResponse, n: usize) {
    for i in 0..n {
        let r = unsafe { (*replies.add(i)).resp };
        if !r.is_null() {
            unsafe {
                let len = libc::strlen(r);
                std::ptr::write_bytes(r, 0, len);
                libc::free(r as *mut c_void);
            }
        }
    }
    unsafe { libc::free(replies as *mut c_void) };
}

/// The PAM service to use.
pub fn service() -> &'static str {
    if std::path::Path::new("/etc/pam.d/rivet-shell").exists() { "rivet-shell" } else { "login" }
}

pub use rivet_lockscreen::login_name;

/// Whether `password` is `user`'s. Err holds what to tell them (PAM's own words when it had
/// any). Blocks: a wrong password takes a couple of seconds.
pub fn check(user: &str, password: &str) -> Result<(), String> {
    let service = CString::new(service()).map_err(|_| "bad service name".to_string())?;
    let user_c = CString::new(user).map_err(|_| "bad user name".to_string())?;
    let mut pw = password.as_bytes().to_vec();
    pw.retain(|b| *b != 0);
    let mut talk = Box::new(Talk { user: user_c.clone(), password: CString::new(pw).unwrap_or_default(), said: Vec::new() });
    let conv = PamConv { conv: converse, appdata_ptr: &mut *talk as *mut Talk as *mut c_void };
    let mut h: *mut c_void = std::ptr::null_mut();
    let r = unsafe { pam_start(service.as_ptr(), user_c.as_ptr(), &conv, &mut h) };
    if r != PAM_SUCCESS || h.is_null() {
        return Err("Couldn't check passwords: PAM didn't start".into());
    }
    let r = unsafe { pam_authenticate(h, 0) };
    let why = if r == PAM_SUCCESS {
        None
    } else {
        let s = unsafe { pam_strerror(h, r) };
        Some(if s.is_null() { String::new() } else { unsafe { CStr::from_ptr(s) }.to_string_lossy().into_owned() })
    };
    unsafe { pam_end(h, r) };
    // The copy of the password in this process: wiped.
    let mut bytes = std::mem::take(&mut talk.password).into_bytes_with_nul();
    bytes.iter_mut().for_each(|b| *b = 0);
    match why {
        None => Ok(()),
        // Authentication failure: the plain words; anything else, what PAM said.
        Some(_) if r == 7 => Err(talk.said.last().cloned().unwrap_or_else(|| "Wrong password".into())),
        Some(w) => Err(talk.said.last().cloned().unwrap_or(w)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs PAM for real (and leaves a failed try in the journal): `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn wrong_password_fails() {
        // Not the password (it waits a couple of seconds, as PAM does on purpose).
        let r = check(&login_name(), "not-the-password-\u{1}x");
        assert!(r.is_err());
        assert!(!login_name().is_empty());
    }
}
