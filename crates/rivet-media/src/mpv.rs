//! Playback through libmpv, the library behind the mpv player, loaded when it is first asked
//! for. Nothing of mpv is linked or shipped: `libmpv.so.2` (or `.so.1`) is opened at run time,
//! so a computer without it still runs the app, which says what to install instead.
//!
//! The declarations below are our own, written from mpv's documented C API (its `client.h`,
//! `render.h` and `render_gl.h` describe it; they are ISC-licensed so programs under any
//! license can use them). Only what Rivet uses is declared.
//!
//! [`Player`] is one mpv instance: commands, properties, and a thread that turns mpv's events
//! into [`Event`]s. [`Render`] draws its video into an OpenGL framebuffer the app owns, so the
//! picture is a texture like any other and the app's controls draw over it.

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, OnceLock};

/// What to tell a person when libmpv isn't there.
pub const MISSING: &str = "Install mpv-libs (Fedora), libmpv2 (Debian, Ubuntu) or mpv (Arch) to play video and audio.";

const FORMAT_NONE: c_int = 0;
const FORMAT_STRING: c_int = 1;
const FORMAT_FLAG: c_int = 3;
const FORMAT_INT64: c_int = 4;
const FORMAT_DOUBLE: c_int = 5;
const FORMAT_NODE: c_int = 6;
const FORMAT_NODE_ARRAY: c_int = 7;
const FORMAT_NODE_MAP: c_int = 8;

const EVENT_SHUTDOWN: c_int = 1;
const EVENT_START_FILE: c_int = 6;
const EVENT_END_FILE: c_int = 7;
const EVENT_FILE_LOADED: c_int = 8;
const EVENT_SEEK: c_int = 20;
const EVENT_PLAYBACK_RESTART: c_int = 21;
const EVENT_PROPERTY_CHANGE: c_int = 22;

const END_EOF: c_int = 0;
const END_ERROR: c_int = 4;

const RENDER_PARAM_INVALID: c_int = 0;
const RENDER_PARAM_API_TYPE: c_int = 1;
const RENDER_PARAM_OPENGL_INIT_PARAMS: c_int = 2;
const RENDER_PARAM_OPENGL_FBO: c_int = 3;
const RENDER_PARAM_FLIP_Y: c_int = 4;
const RENDER_PARAM_X11_DISPLAY: c_int = 8;
const RENDER_PARAM_WL_DISPLAY: c_int = 9;
const RENDER_UPDATE_FRAME: u64 = 1;

/// `GL_RGBA8`, the framebuffer's format as mpv is told it.
const GL_RGBA8: c_int = 0x8058;

// Some fields are only read by libmpv.
#[allow(dead_code)]
#[repr(C)]
struct RawEvent {
    event_id: c_int,
    error: c_int,
    reply_userdata: u64,
    data: *mut c_void,
}

// Some fields are only read by libmpv.
#[allow(dead_code)]
#[repr(C)]
struct RawProperty {
    name: *const c_char,
    format: c_int,
    data: *mut c_void,
}

#[repr(C)]
struct RawEndFile {
    reason: c_int,
    error: c_int,
}

#[repr(C)]
union RawNodeValue {
    string: *mut c_char,
    flag: c_int,
    int64: i64,
    double: f64,
    list: *mut RawNodeList,
}

#[repr(C)]
struct RawNode {
    u: RawNodeValue,
    format: c_int,
}

#[repr(C)]
struct RawNodeList {
    num: c_int,
    values: *mut RawNode,
    keys: *mut *mut c_char,
}

// Some fields are only read by libmpv.
#[allow(dead_code)]
#[repr(C)]
struct RenderParam {
    kind: c_int,
    data: *mut c_void,
}

// Some fields are only read by libmpv.
#[allow(dead_code)]
#[repr(C)]
struct GlInitParams {
    get_proc_address: Option<unsafe extern "C" fn(*mut c_void, *const c_char) -> *mut c_void>,
    get_proc_address_ctx: *mut c_void,
    /// Gone from newer headers; a trailing null keeps the struct right for older ones too.
    extra_exts: *const c_char,
}

// Some fields are only read by libmpv.
#[allow(dead_code)]
#[repr(C)]
struct GlFbo {
    fbo: c_int,
    w: c_int,
    h: c_int,
    internal_format: c_int,
}

type MpvHandle = c_void;
type MpvRender = c_void;

/// The functions used, looked up once.
struct Lib {
    create: unsafe extern "C" fn() -> *mut MpvHandle,
    initialize: unsafe extern "C" fn(*mut MpvHandle) -> c_int,
    terminate_destroy: unsafe extern "C" fn(*mut MpvHandle),
    set_option_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    command: unsafe extern "C" fn(*mut MpvHandle, *mut *const c_char) -> c_int,
    set_property: unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int,
    set_property_string: unsafe extern "C" fn(*mut MpvHandle, *const c_char, *const c_char) -> c_int,
    get_property: unsafe extern "C" fn(*mut MpvHandle, *const c_char, c_int, *mut c_void) -> c_int,
    observe_property: unsafe extern "C" fn(*mut MpvHandle, u64, *const c_char, c_int) -> c_int,
    wait_event: unsafe extern "C" fn(*mut MpvHandle, f64) -> *mut RawEvent,
    error_string: unsafe extern "C" fn(c_int) -> *const c_char,
    render_create: unsafe extern "C" fn(*mut *mut MpvRender, *mut MpvHandle, *mut RenderParam) -> c_int,
    render_set_update_callback: unsafe extern "C" fn(*mut MpvRender, Option<unsafe extern "C" fn(*mut c_void)>, *mut c_void),
    render_update: unsafe extern "C" fn(*mut MpvRender) -> u64,
    render_render: unsafe extern "C" fn(*mut MpvRender, *mut RenderParam) -> c_int,
    render_report_swap: unsafe extern "C" fn(*mut MpvRender),
    render_free: unsafe extern "C" fn(*mut MpvRender),
}

/// A function from the library, or None when it isn't there.
///
/// # Safety
/// `T` must be the function pointer type the symbol really has.
unsafe fn sym<T: Copy>(lib: *mut c_void, name: &CStr) -> Option<T> {
    let p = unsafe { libc::dlsym(lib, name.as_ptr()) };
    (!p.is_null()).then(|| unsafe { std::mem::transmute_copy::<*mut c_void, T>(&p) })
}

fn load() -> Option<Lib> {
    // RIVET_CINEMA_NO_MPV=1 behaves as if it weren't installed, to see that message.
    if std::env::var_os("RIVET_CINEMA_NO_MPV").is_some() {
        return None;
    }
    let lib = [c"libmpv.so.2", c"libmpv.so.1", c"libmpv.so"].into_iter().map(|n| unsafe { libc::dlopen(n.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) }).find(|h| !h.is_null())?;
    unsafe {
        Some(Lib {
            create: sym(lib, c"mpv_create")?,
            initialize: sym(lib, c"mpv_initialize")?,
            terminate_destroy: sym(lib, c"mpv_terminate_destroy")?,
            set_option_string: sym(lib, c"mpv_set_option_string")?,
            command: sym(lib, c"mpv_command")?,
            set_property: sym(lib, c"mpv_set_property")?,
            set_property_string: sym(lib, c"mpv_set_property_string")?,
            get_property: sym(lib, c"mpv_get_property")?,
            observe_property: sym(lib, c"mpv_observe_property")?,
            wait_event: sym(lib, c"mpv_wait_event")?,
            error_string: sym(lib, c"mpv_error_string")?,
            render_create: sym(lib, c"mpv_render_context_create")?,
            render_set_update_callback: sym(lib, c"mpv_render_context_set_update_callback")?,
            render_update: sym(lib, c"mpv_render_context_update")?,
            render_render: sym(lib, c"mpv_render_context_render")?,
            render_report_swap: sym(lib, c"mpv_render_context_report_swap")?,
            render_free: sym(lib, c"mpv_render_context_free")?,
        })
    }
}

fn lib() -> Option<&'static Lib> {
    static LIB: OnceLock<Option<Lib>> = OnceLock::new();
    LIB.get_or_init(load).as_ref()
}

/// Whether libmpv could be loaded on this computer.
pub fn available() -> bool {
    lib().is_some()
}

fn error_text(lib: &Lib, code: c_int) -> String {
    let p = unsafe { (lib.error_string)(code) };
    if p.is_null() { format!("error {code}") } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

fn cstring(s: &str) -> CString {
    CString::new(s.replace('\0', "")).unwrap_or_default()
}

/// A value from mpv: what a property holds, or a piece of a list or map.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Node {
    #[default]
    None,
    Str(String),
    Flag(bool),
    Int(i64),
    Double(f64),
    List(Vec<Node>),
    Map(Vec<(String, Node)>),
}

impl Node {
    pub fn get(&self, key: &str) -> Option<&Node> {
        match self {
            Node::Map(m) => m.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn str(&self) -> Option<&str> {
        match self {
            Node::Str(s) => Some(s),
            _ => None,
        }
    }

    pub fn flag(&self) -> Option<bool> {
        match self {
            Node::Flag(b) => Some(*b),
            _ => None,
        }
    }

    pub fn int(&self) -> Option<i64> {
        match self {
            Node::Int(i) => Some(*i),
            Node::Double(d) => Some(*d as i64),
            _ => None,
        }
    }

    pub fn double(&self) -> Option<f64> {
        match self {
            Node::Double(d) => Some(*d),
            Node::Int(i) => Some(*i as f64),
            _ => None,
        }
    }

    /// # Safety
    /// `n` must point to a node mpv handed over, still alive.
    unsafe fn from_raw(n: *const RawNode) -> Node {
        let n = unsafe { &*n };
        unsafe {
            match n.format {
                FORMAT_STRING => Node::Str(text(n.u.string)),
                FORMAT_FLAG => Node::Flag(n.u.flag != 0),
                FORMAT_INT64 => Node::Int(n.u.int64),
                FORMAT_DOUBLE => Node::Double(n.u.double),
                FORMAT_NODE_ARRAY | FORMAT_NODE_MAP if !n.u.list.is_null() => {
                    let l = &*n.u.list;
                    let count = l.num.max(0) as usize;
                    if n.format == FORMAT_NODE_ARRAY {
                        Node::List((0..count).map(|i| Node::from_raw(l.values.add(i))).collect())
                    } else {
                        Node::Map((0..count).map(|i| (text(*l.keys.add(i)), Node::from_raw(l.values.add(i)))).collect())
                    }
                }
                _ => Node::None,
            }
        }
    }
}

/// # Safety
/// `p` is null or a C string mpv handed over.
unsafe fn text(p: *const c_char) -> String {
    if p.is_null() { String::new() } else { unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned() }
}

/// What mpv reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A watched property has a new value (`Node::None` when it has none now).
    Changed(&'static str, Node),
    StartFile,
    /// The file is open; its tracks and length are known.
    FileLoaded,
    /// A file stopped: it played to the end, or it couldn't be played (and why).
    EndFile { finished: bool, error: Option<String> },
    /// A seek has started.
    Seeking,
    /// Playback picked up again after a seek or the start of a file.
    Restarted,
    Shutdown,
}

/// The properties every player watches, and how mpv should hand each over.
const WATCHED: [(&str, c_int); 15] = [
    ("time-pos", FORMAT_DOUBLE),
    ("duration", FORMAT_DOUBLE),
    ("pause", FORMAT_FLAG),
    ("volume", FORMAT_DOUBLE),
    ("mute", FORMAT_FLAG),
    ("speed", FORMAT_DOUBLE),
    ("track-list", FORMAT_NODE),
    ("metadata", FORMAT_NODE),
    ("media-title", FORMAT_STRING),
    ("dwidth", FORMAT_INT64),
    ("dheight", FORMAT_INT64),
    ("sub-delay", FORMAT_DOUBLE),
    ("idle-active", FORMAT_FLAG),
    ("paused-for-cache", FORMAT_FLAG),
    ("chapter-list", FORMAT_NODE),
];

/// The mpv instance, destroyed when the last of the player, its event thread and its renderer
/// lets go of it.
struct Handle {
    lib: &'static Lib,
    ptr: *mut MpvHandle,
}

// mpv's client API may be called from any thread.
unsafe impl Send for Handle {}
unsafe impl Sync for Handle {}

impl Drop for Handle {
    fn drop(&mut self) {
        unsafe { (self.lib.terminate_destroy)(self.ptr) };
    }
}

/// One mpv instance, with its events read on a thread of its own.
pub struct Player {
    h: Arc<Handle>,
    pub events: Receiver<Event>,
}

impl Player {
    /// Start mpv with these options (set before it starts; see mpv's manual), calling `wake`
    /// after each event so the window looks at it.
    pub fn new(options: &[(&str, &str)], wake: impl Fn() + Send + 'static) -> Result<Player, String> {
        let lib = lib().ok_or_else(|| MISSING.to_string())?;
        let ptr = unsafe { (lib.create)() };
        if ptr.is_null() {
            return Err("mpv couldn't start".into());
        }
        let h = Arc::new(Handle { lib, ptr });
        for (k, v) in options {
            let (k, v) = (cstring(k), cstring(v));
            let r = unsafe { (lib.set_option_string)(ptr, k.as_ptr(), v.as_ptr()) };
            if r < 0 {
                eprintln!("rivet-media: mpv option {}: {}", k.to_string_lossy(), error_text(lib, r));
            }
        }
        let r = unsafe { (lib.initialize)(ptr) };
        if r < 0 {
            return Err(format!("mpv couldn't start: {}", error_text(lib, r)));
        }
        for (i, (name, format)) in WATCHED.iter().enumerate() {
            let n = cstring(name);
            unsafe { (lib.observe_property)(ptr, i as u64, n.as_ptr(), *format) };
        }
        let (tx, events) = channel();
        let reader = h.clone();
        std::thread::Builder::new()
            .name("mpv events".into())
            .spawn(move || {
                loop {
                    let e = unsafe { &*(reader.lib.wait_event)(reader.ptr, -1.0) };
                    let event = unsafe { convert(reader.lib, e) };
                    let done = event == Some(Event::Shutdown);
                    if let Some(event) = event
                        && tx.send(event).is_err()
                    {
                        break;
                    }
                    wake();
                    if done {
                        break;
                    }
                }
            })
            .map_err(|e| e.to_string())?;
        Ok(Player { h, events })
    }

    /// Run an mpv command, such as `["loadfile", path, "replace"]` or `["seek", "5"]`.
    pub fn command(&self, args: &[&str]) -> Result<(), String> {
        let owned: Vec<CString> = args.iter().map(|a| cstring(a)).collect();
        let mut ptrs: Vec<*const c_char> = owned.iter().map(|c| c.as_ptr()).collect();
        ptrs.push(std::ptr::null());
        let r = unsafe { (self.h.lib.command)(self.h.ptr, ptrs.as_mut_ptr()) };
        if r < 0 { Err(error_text(self.h.lib, r)) } else { Ok(()) }
    }

    pub fn set_str(&self, name: &str, value: &str) {
        let (n, v) = (cstring(name), cstring(value));
        unsafe { (self.h.lib.set_property_string)(self.h.ptr, n.as_ptr(), v.as_ptr()) };
    }

    pub fn set_flag(&self, name: &str, on: bool) {
        let n = cstring(name);
        let mut v: c_int = on.into();
        unsafe { (self.h.lib.set_property)(self.h.ptr, n.as_ptr(), FORMAT_FLAG, (&raw mut v).cast()) };
    }

    pub fn set_double(&self, name: &str, value: f64) {
        let n = cstring(name);
        let mut v = value;
        unsafe { (self.h.lib.set_property)(self.h.ptr, n.as_ptr(), FORMAT_DOUBLE, (&raw mut v).cast()) };
    }

    pub fn get_double(&self, name: &str) -> Option<f64> {
        let n = cstring(name);
        let mut v = 0f64;
        let r = unsafe { (self.h.lib.get_property)(self.h.ptr, n.as_ptr(), FORMAT_DOUBLE, (&raw mut v).cast()) };
        (r >= 0).then_some(v)
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        // Ends the event thread (it sees the shutdown); mpv itself goes when the last handle does.
        let _ = self.command(&["quit"]);
    }
}

/// # Safety
/// `e` is the event mpv just returned, read before the next wait.
unsafe fn convert(lib: &Lib, e: &RawEvent) -> Option<Event> {
    Some(match e.event_id {
        EVENT_SHUTDOWN => Event::Shutdown,
        EVENT_START_FILE => Event::StartFile,
        EVENT_FILE_LOADED => Event::FileLoaded,
        EVENT_SEEK => Event::Seeking,
        EVENT_PLAYBACK_RESTART => Event::Restarted,
        EVENT_END_FILE if !e.data.is_null() => {
            let end = unsafe { &*(e.data as *const RawEndFile) };
            Event::EndFile { finished: end.reason == END_EOF, error: (end.reason == END_ERROR).then(|| error_text(lib, end.error)) }
        }
        EVENT_PROPERTY_CHANGE if !e.data.is_null() => {
            let p = unsafe { &*(e.data as *const RawProperty) };
            let name = WATCHED.get(e.reply_userdata as usize)?.0;
            let value = if p.data.is_null() {
                Node::None
            } else {
                unsafe {
                    match p.format {
                        FORMAT_NONE => Node::None,
                        FORMAT_FLAG => Node::Flag(*(p.data as *const c_int) != 0),
                        FORMAT_INT64 => Node::Int(*(p.data as *const i64)),
                        FORMAT_DOUBLE => Node::Double(*(p.data as *const f64)),
                        FORMAT_STRING => Node::Str(text(*(p.data as *const *const c_char))),
                        FORMAT_NODE => Node::from_raw(p.data as *const RawNode),
                        _ => Node::None,
                    }
                }
            };
            Event::Changed(name, value)
        }
        _ => return None,
    })
}

/// One audio, video or subtitle track of the file playing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Track {
    pub id: i64,
    /// "audio", "video" or "sub".
    pub kind: String,
    pub title: Option<String>,
    pub lang: Option<String>,
    pub codec: Option<String>,
    pub selected: bool,
    /// A picture stored in the file (an album cover), not a video.
    pub albumart: bool,
    /// Loaded from a file of its own (a subtitle file).
    pub external: bool,
    pub default: bool,
    pub forced: bool,
}

/// The tracks in mpv's `track-list`.
pub fn tracks(list: &Node) -> Vec<Track> {
    let Node::List(items) = list else { return Vec::new() };
    items
        .iter()
        .filter_map(|t| {
            let s = |k: &str| t.get(k).and_then(Node::str).filter(|v| !v.is_empty()).map(str::to_string);
            let b = |k: &str| t.get(k).and_then(Node::flag).unwrap_or(false);
            Some(Track { id: t.get("id")?.int()?, kind: s("type")?, title: s("title"), lang: s("lang"), codec: s("codec"), selected: b("selected"), albumart: b("albumart"), external: b("external"), default: b("default"), forced: b("forced") })
        })
        .collect()
}

/// The text values of mpv's `metadata` map, with lower-case keys ("title", "artist"…).
pub fn metadata(map: &Node) -> Vec<(String, String)> {
    let Node::Map(m) = map else { return Vec::new() };
    m.iter().filter_map(|(k, v)| Some((k.to_lowercase(), v.str()?.to_string()))).collect()
}

/// Where a renderer finds OpenGL's functions (eframe hands one over).
pub type ProcAddress = Arc<dyn Fn(&CStr) -> *const c_void + Send + Sync>;

unsafe extern "C" fn proc_trampoline(ctx: *mut c_void, name: *const c_char) -> *mut c_void {
    let f = unsafe { &*(ctx as *const ProcAddress) };
    f(unsafe { CStr::from_ptr(name) }) as *mut c_void
}

type Wake = Box<dyn Fn() + Send + Sync>;

unsafe extern "C" fn wake_trampoline(ctx: *mut c_void) {
    let f = unsafe { &*(ctx as *const Wake) };
    f();
}

/// The display a renderer shares with mpv, for hardware decoding's sake.
#[derive(Clone, Copy, Debug)]
pub enum Display {
    Wayland(*mut c_void),
    X11(*mut c_void),
    Unknown,
}

/// Draws a player's video with OpenGL. Make it, draw with it and drop it only while the
/// window's OpenGL context is current.
pub struct Render {
    ctx: *mut MpvRender,
    h: Arc<Handle>,
    _proc: Box<ProcAddress>,
    _wake: Box<Wake>,
}

impl Render {
    /// # Safety
    /// The OpenGL context `proc` belongs to must be current on this thread.
    pub unsafe fn new(player: &Player, proc: ProcAddress, display: Display, wake: impl Fn() + Send + Sync + 'static) -> Result<Render, String> {
        let h = player.h.clone();
        let proc = Box::new(proc);
        let mut init = GlInitParams { get_proc_address: Some(proc_trampoline), get_proc_address_ctx: (&*proc as *const ProcAddress).cast_mut().cast(), extra_exts: std::ptr::null() };
        let mut params = vec![RenderParam { kind: RENDER_PARAM_API_TYPE, data: c"opengl".as_ptr().cast_mut().cast() }, RenderParam { kind: RENDER_PARAM_OPENGL_INIT_PARAMS, data: (&raw mut init).cast() }];
        match display {
            Display::Wayland(d) if !d.is_null() => params.push(RenderParam { kind: RENDER_PARAM_WL_DISPLAY, data: d }),
            Display::X11(d) if !d.is_null() => params.push(RenderParam { kind: RENDER_PARAM_X11_DISPLAY, data: d }),
            _ => {}
        }
        params.push(RenderParam { kind: RENDER_PARAM_INVALID, data: std::ptr::null_mut() });
        let mut ctx = std::ptr::null_mut();
        let r = unsafe { (h.lib.render_create)(&mut ctx, h.ptr, params.as_mut_ptr()) };
        if r < 0 || ctx.is_null() {
            return Err(format!("mpv couldn't draw the video: {}", error_text(h.lib, r)));
        }
        let wake: Box<Wake> = Box::new(Box::new(wake) as Wake);
        unsafe { (h.lib.render_set_update_callback)(ctx, Some(wake_trampoline), (&*wake as *const Wake).cast_mut().cast()) };
        Ok(Render { ctx, h, _proc: proc, _wake: wake })
    }

    /// Whether mpv has a new frame to draw.
    pub fn frame_due(&self) -> bool {
        unsafe { (self.h.lib.render_update)(self.ctx) & RENDER_UPDATE_FRAME != 0 }
    }

    /// Draw the current frame into framebuffer `fbo`, `w`×`h` pixels, letterboxed.
    /// `flip` turns it upside down (for a texture read top row first).
    ///
    /// # Safety
    /// The OpenGL context must be current, and `fbo` a complete framebuffer of that size.
    pub unsafe fn draw(&self, fbo: u32, w: i32, h: i32, flip: bool) -> Result<(), String> {
        let mut target = GlFbo { fbo: fbo as c_int, w, h, internal_format: GL_RGBA8 };
        let mut flip: c_int = flip.into();
        let mut params = [
            RenderParam { kind: RENDER_PARAM_OPENGL_FBO, data: (&raw mut target).cast() },
            RenderParam { kind: RENDER_PARAM_FLIP_Y, data: (&raw mut flip).cast() },
            RenderParam { kind: RENDER_PARAM_INVALID, data: std::ptr::null_mut() },
        ];
        let r = unsafe { (self.h.lib.render_render)(self.ctx, params.as_mut_ptr()) };
        if r < 0 { Err(error_text(self.h.lib, r)) } else { Ok(()) }
    }

    /// Tell mpv a frame went to the screen, which keeps its timing right.
    pub fn swapped(&self) {
        unsafe { (self.h.lib.render_report_swap)(self.ctx) };
    }
}

impl Drop for Render {
    fn drop(&mut self) {
        unsafe { (self.h.lib.render_free)(self.ctx) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_from_a_track_list() {
        let t = |id: i64, kind: &str, extra: Vec<(&str, Node)>| {
            let mut m = vec![("id".to_string(), Node::Int(id)), ("type".to_string(), Node::Str(kind.into()))];
            m.extend(extra.into_iter().map(|(k, v)| (k.to_string(), v)));
            Node::Map(m)
        };
        let list = Node::List(vec![
            t(1, "video", vec![("albumart", Node::Flag(true)), ("selected", Node::Flag(true))]),
            t(1, "audio", vec![("lang", Node::Str("eng".into())), ("title", Node::Str("".into()))]),
            t(2, "sub", vec![("external", Node::Flag(true)), ("title", Node::Str("Signs".into()))]),
            Node::Map(vec![("type".into(), Node::Str("audio".into()))]),
        ]);
        let got = tracks(&list);
        assert_eq!(got.len(), 3);
        assert!(got[0].albumart && got[0].selected);
        assert_eq!(got[1].lang.as_deref(), Some("eng"));
        assert_eq!(got[1].title, None);
        assert!(got[2].external);
        assert_eq!(tracks(&Node::None), Vec::new());
    }

    #[test]
    fn metadata_keys_are_lower_case() {
        let m = Node::Map(vec![("TITLE".into(), Node::Str("Tide".into())), ("Artist".into(), Node::Str("Sample".into())), ("track".into(), Node::Int(3))]);
        assert_eq!(metadata(&m), vec![("title".to_string(), "Tide".to_string()), ("artist".to_string(), "Sample".to_string())]);
    }

    #[test]
    fn missing_library_is_an_error_not_a_crash() {
        // Where libmpv isn't installed, starting a player says what to install.
        if !available() {
            assert_eq!(Player::new(&[], || {}).err().as_deref(), Some(MISSING));
        }
    }
}
