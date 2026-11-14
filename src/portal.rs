//! `rivet --portal`: Rivet as the desktop's file picker.
//!
//! Apps ask the desktop portal (xdg-desktop-portal) to show a file picker; the portal passes the
//! request to the FileChooser backend set in `portals.conf`. This process is that backend: it
//! serves `org.freedesktop.impl.portal.FileChooser` on the session bus, starts a `rivet --pick`
//! window for each request (see `picker.rs`), and answers with what was chosen. A request's
//! `Close` (the app gave up) closes its window. Turning it on and off is `portal_setup.rs`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

use crate::picker::{Answer, Mode, Request, TypeFilter};

pub const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.rivet";
const OBJECT_PATH: &str = "/org/freedesktop/portal/desktop";
/// Stop when nothing has been asked for this long (the bus starts it again when needed).
const IDLE_EXIT: Duration = Duration::from_secs(15 * 60);

const RESPONSE_OK: u32 = 0;
const RESPONSE_CANCELLED: u32 = 1;
const RESPONSE_OTHER: u32 = 2;

type Options = HashMap<String, OwnedValue>;
type Results = HashMap<String, OwnedValue>;

#[derive(Clone, Default)]
struct Shared {
    /// Open requests: handle path -> picker process id.
    open: Arc<Mutex<HashMap<String, u32>>>,
    last_used: Arc<Mutex<Option<Instant>>>,
}

struct Chooser {
    shared: Shared,
}

/// The per-request object the portal calls `Close` on when the app cancels.
struct RequestObject {
    shared: Shared,
    handle: String,
}

#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl RequestObject {
    async fn close(&self) {
        if let Some(pid) = self.shared.open.lock().unwrap_or_else(|e| e.into_inner()).get(&self.handle) {
            // SAFETY: plain kill(2) on the picker process this service started.
            unsafe {
                libc::kill(*pid as i32, libc::SIGTERM);
            }
        }
    }
}

#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl Chooser {
    async fn open_file(
        &self,
        #[zbus(connection)] conn: &zbus::Connection,
        handle: OwnedObjectPath,
        app_id: String,
        parent_window: String,
        title: String,
        options: Options,
    ) -> (u32, Results) {
        let _ = parent_window;
        let mut r = request(Mode::Open, title, &options);
        r.app = app_id;
        r.multiple = get::<bool>(&options, "multiple").unwrap_or(false);
        r.directory = get::<bool>(&options, "directory").unwrap_or(false);
        self.serve(conn, handle, r).await
    }

    async fn save_file(
        &self,
        #[zbus(connection)] conn: &zbus::Connection,
        handle: OwnedObjectPath,
        app_id: String,
        parent_window: String,
        title: String,
        options: Options,
    ) -> (u32, Results) {
        let _ = parent_window;
        let mut r = request(Mode::Save, title, &options);
        r.app = app_id;
        r.name = get::<String>(&options, "current_name").unwrap_or_default().replace('/', "_");
        if let Some(file) = bytes_path(&options, "current_file") {
            r.folder = file.parent().map(|p| p.to_path_buf()).or(r.folder);
            if r.name.is_empty() {
                r.name = file.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            }
        }
        self.serve(conn, handle, r).await
    }

    async fn save_files(
        &self,
        #[zbus(connection)] conn: &zbus::Connection,
        handle: OwnedObjectPath,
        app_id: String,
        parent_window: String,
        title: String,
        options: Options,
    ) -> (u32, Results) {
        let _ = parent_window;
        let mut r = request(Mode::SaveFiles, title, &options);
        r.app = app_id;
        r.files = get::<Vec<Vec<u8>>>(&options, "files")
            .unwrap_or_default()
            .into_iter()
            .map(|b| String::from_utf8_lossy(b.strip_suffix(&[0]).unwrap_or(&b)).replace('/', "_"))
            .filter(|n| !n.is_empty())
            .collect();
        self.serve(conn, handle, r).await
    }

    /// Version of the backend interface.
    #[zbus(property)]
    async fn version(&self) -> u32 {
        4
    }
}

impl Chooser {
    /// Show a picker for this request and wait for it.
    async fn serve(&self, conn: &zbus::Connection, handle: OwnedObjectPath, r: Request) -> (u32, Results) {
        *self.shared.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        let key = handle.as_str().to_string();
        let server = conn.object_server();
        let _ = server.at(handle.clone(), RequestObject { shared: self.shared.clone(), handle: key.clone() }).await;
        let result = self.run_picker(&key, &r).await;
        let _ = server.remove::<RequestObject, _>(handle).await;
        self.shared.open.lock().unwrap_or_else(|e| e.into_inner()).remove(&key);
        *self.shared.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
        match result {
            Some(Ok(a)) => (RESPONSE_OK, results(&r, &a)),
            Some(Err(())) => (RESPONSE_CANCELLED, Results::new()),
            None => (RESPONSE_OTHER, Results::new()),
        }
    }

    /// Run `rivet --pick`; Some(Ok) chosen, Some(Err) cancelled, None failed to start.
    async fn run_picker(&self, key: &str, r: &Request) -> Option<Result<Answer, ()>> {
        use std::io::{Read, Write};
        use std::process::{Command, Stdio};
        let mut child = Command::new(picker_exe()).arg("--pick").stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::inherit()).spawn().ok()?;
        self.shared.open.lock().unwrap_or_else(|e| e.into_inner()).insert(key.to_string(), child.id());
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(r.to_text().as_bytes());
        }
        // Wait without blocking the bus (other requests and Close keep working).
        let status = loop {
            match child.try_wait() {
                Ok(Some(s)) => break s,
                Ok(None) => async_io::Timer::after(Duration::from_millis(80)).await,
                Err(_) => return None,
            };
        };
        let mut out = String::new();
        if let Some(mut so) = child.stdout.take() {
            let _ = so.read_to_string(&mut out);
        }
        let a = Answer::parse(&out);
        Some(if status.success() && !a.paths.is_empty() { Ok(a) } else { Err(()) })
    }
}

/// The program to run for each picker window: this one (or the AppImage it came from).
///
/// The command the service was started with comes first (`/usr/bin/rivet`): when Rivet is
/// updated while this service runs, its own program file is replaced and `current_exe` points
/// at the old, deleted one, which can't be started.
fn picker_exe() -> PathBuf {
    if let (Some(img), Some(dir)) = (std::env::var_os("APPIMAGE"), std::env::var_os("APPDIR"))
        && std::env::current_exe().is_ok_and(|e| e.starts_with(&dir))
    {
        return PathBuf::from(img);
    }
    if let Some(arg0) = std::env::args_os().next().map(PathBuf::from)
        && arg0.is_absolute()
        && arg0.is_file()
    {
        return arg0;
    }
    match std::env::current_exe() {
        Ok(e) => {
            let s = e.to_string_lossy();
            PathBuf::from(s.strip_suffix(" (deleted)").unwrap_or(&s).to_string())
        }
        Err(_) => PathBuf::from("rivet"),
    }
}

fn get<T>(o: &Options, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    o.get(key)?.try_clone().ok()?.try_into().ok()
}

/// A path sent as bytes with a trailing NUL (`current_folder`, `current_file`).
fn bytes_path(o: &Options, key: &str) -> Option<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let b = get::<Vec<u8>>(o, key)?;
    let b = b.strip_suffix(&[0]).unwrap_or(&b);
    (!b.is_empty()).then(|| PathBuf::from(std::ffi::OsStr::from_bytes(b))).filter(|p| p.is_absolute())
}

/// What every request shares: title, accept label, folder and the file-type list.
fn request(mode: Mode, title: String, o: &Options) -> Request {
    let raw: Vec<(String, Vec<(u32, String)>)> = get(o, "filters").unwrap_or_default();
    let filters: Vec<TypeFilter> = raw
        .into_iter()
        .take(256)
        .map(|(name, pats)| {
            let mut f = TypeFilter { name, ..Default::default() };
            for (kind, p) in pats.into_iter().take(256) {
                if kind == 1 {
                    f.mimes.push(p.to_lowercase());
                } else {
                    f.globs.push(p.to_lowercase());
                }
            }
            f
        })
        .collect();
    // The app's preferred type, matched to the list by name.
    let current: Option<(String, Vec<(u32, String)>)> = get(o, "current_filter");
    let current_filter = current.and_then(|(name, _)| filters.iter().position(|f| f.name == name));
    Request {
        mode,
        title: title.chars().take(256).collect(),
        accept: get::<String>(o, "accept_label").unwrap_or_default().replace('_', ""),
        folder: bytes_path(o, "current_folder"),
        filters,
        current_filter,
        ..Default::default()
    }
}

/// The answer in the portal's terms: `uris`, and the type that was chosen.
fn results(r: &Request, a: &Answer) -> Results {
    let mut out = Results::new();
    let paths: Vec<PathBuf> = match r.mode {
        // Several files go into the chosen folder under the names the app asked for.
        Mode::SaveFiles => {
            let dir = a.paths.first().cloned().unwrap_or_default();
            r.files.iter().map(|n| dir.join(n)).collect()
        }
        Mode::Save => a.paths.iter().take(1).cloned().collect(),
        Mode::Open => a.paths.clone(),
    };
    let uris: Vec<String> = paths.iter().map(|p| format!("file://{}", crate::places::percent_encode_path(p))).collect();
    if let Ok(v) = OwnedValue::try_from(Value::from(uris)) {
        out.insert("uris".into(), v);
    }
    if let Some(f) = a.filter.and_then(|i| r.filters.get(i)) {
        let mut pats: Vec<(u32, String)> = f.globs.iter().map(|g| (0u32, g.clone())).collect();
        pats.extend(f.mimes.iter().map(|m| (1u32, m.clone())));
        if let Ok(v) = OwnedValue::try_from(Value::from((f.name.clone(), pats))) {
            out.insert("current_filter".into(), v);
        }
    }
    out
}

/// Serve requests until idle for a while.
pub fn run() -> Result<(), String> {
    let shared = Shared::default();
    let conn = zbus::blocking::connection::Builder::session()
        .and_then(|b| b.name(BUS_NAME))
        .and_then(|b| b.serve_at(OBJECT_PATH, Chooser { shared: shared.clone() }))
        .and_then(|b| b.build())
        .map_err(|e| format!("Could not start the file picker service: {e}"))?;
    *shared.last_used.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    let me = conn.unique_name().map(|n| n.to_string());
    loop {
        std::thread::sleep(Duration::from_secs(20));
        let busy = !shared.open.lock().unwrap_or_else(|e| e.into_inner()).is_empty();
        let idle = shared.last_used.lock().unwrap_or_else(|e| e.into_inner()).is_some_and(|t| t.elapsed() > IDLE_EXIT);
        if idle && !busy {
            return Ok(());
        }
        // A newer copy took over the name (the bus started it after an update): leave to it.
        let owner = zbus::blocking::fdo::DBusProxy::new(&conn)
            .ok()
            .and_then(|p| p.get_name_owner(BUS_NAME.try_into().ok()?).ok())
            .map(|o| o.to_string());
        if !busy && owner.is_some() && owner != me {
            return Ok(());
        }
    }
}
