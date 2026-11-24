//! The screens: their current modes from the compositor when there is a Wayland session, and
//! their names and sizes from the kernel's copy of each monitor's EDID.

use std::path::{Path, PathBuf};
use wayland_client::protocol::{wl_output, wl_registry};
use wayland_client::{Connection, Dispatch, QueueHandle, WEnum};

#[derive(Default, Debug, Clone)]
pub struct Screen {
    /// The connector, such as `DP-1` or `eDP-1`.
    pub connector: String,
    /// The monitor's own name, from its EDID, else make and model.
    pub name: String,
    pub width: i32,
    pub height: i32,
    /// Refresh rate in mHz, 0 when not known.
    pub refresh: i32,
    /// Diagonal in inches, 0 when not known.
    pub inches: u32,
    pub builtin: bool,
}

pub fn screens() -> Vec<Screen> {
    let mut list = wayland().unwrap_or_default();
    if list.is_empty() {
        list = from_drm();
    }
    for s in &mut list {
        s.builtin = ["eDP", "LVDS", "DSI"].iter().any(|p| s.connector.starts_with(p));
        if let Some(edid) = connector_dir(&s.connector).and_then(|d| std::fs::read(d.join("edid")).ok()) {
            if let Some(n) = edid_name(&edid) {
                s.name = n;
            }
            if s.inches == 0 && edid.len() > 22 {
                s.inches = diagonal(edid[21] as f32 * 10.0, edid[22] as f32 * 10.0);
            }
        }
    }
    list
}

fn diagonal(w_mm: f32, h_mm: f32) -> u32 {
    if w_mm <= 0.0 || h_mm <= 0.0 {
        return 0;
    }
    ((w_mm * w_mm + h_mm * h_mm).sqrt() / 25.4).round() as u32
}

/// The kernel's folder for a connector name, such as /sys/class/drm/card1-DP-1.
fn connector_dir(connector: &str) -> Option<PathBuf> {
    if connector.is_empty() {
        return None;
    }
    std::fs::read_dir("/sys/class/drm").ok()?.flatten().map(|e| e.path()).find(|p| {
        p.file_name().and_then(|n| n.to_str()).and_then(|n| n.split_once('-')).is_some_and(|(card, c)| card.starts_with("card") && c == connector)
    })
}

/// The monitor name descriptor (tag 0xFC) of an EDID block.
pub fn edid_name(edid: &[u8]) -> Option<String> {
    (0..4).find_map(|i| {
        let d = edid.get(54 + i * 18..54 + (i + 1) * 18)?;
        if d[..3] != [0, 0, 0] || d[3] != 0xFC {
            return None;
        }
        let text: String = d[5..].iter().take_while(|&&b| b != b'\n').map(|&b| b as char).collect();
        let text = text.trim().to_string();
        (!text.is_empty()).then_some(text)
    })
}

/// Without a compositor to ask: every connected connector and its preferred mode.
fn from_drm() -> Vec<Screen> {
    let Ok(rd) = std::fs::read_dir("/sys/class/drm") else { return Vec::new() };
    let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    dirs.sort();
    dirs.iter()
        .filter(|d| read(d, "status").as_deref() == Some("connected"))
        .filter_map(|d| {
            let connector = d.file_name()?.to_str()?.split_once('-')?.1.to_string();
            let mode = read(d, "modes")?.lines().next()?.to_string();
            let (w, h) = mode.split_once('x')?;
            Some(Screen { connector, width: w.parse().ok()?, height: h.trim_end_matches(|c: char| !c.is_ascii_digit()).parse().ok()?, ..Default::default() })
        })
        .collect()
}

fn read(dir: &Path, file: &str) -> Option<String> {
    std::fs::read_to_string(dir.join(file)).ok().map(|s| s.trim().to_string())
}

struct State {
    screens: Vec<Screen>,
}

fn wayland() -> Option<Vec<Screen>> {
    std::env::var_os("WAYLAND_DISPLAY")?;
    let conn = Connection::connect_to_env().ok()?;
    let mut queue = conn.new_event_queue();
    conn.display().get_registry(&queue.handle(), ());
    let mut state = State { screens: Vec::new() };
    // The first round trip lists the outputs, the second brings their details.
    queue.roundtrip(&mut state).ok()?;
    queue.roundtrip(&mut state).ok()?;
    Some(state.screens)
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event
            && interface == "wl_output"
        {
            let i = state.screens.len();
            state.screens.push(Screen::default());
            registry.bind::<wl_output::WlOutput, usize, Self>(name, version.min(4), qh, i);
        }
    }
}

impl Dispatch<wl_output::WlOutput, usize> for State {
    fn event(state: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, i: &usize, _: &Connection, _: &QueueHandle<Self>) {
        let s = &mut state.screens[*i];
        match event {
            wl_output::Event::Geometry { physical_width, physical_height, make, model, .. } => {
                s.inches = diagonal(physical_width as f32, physical_height as f32);
                if s.name.is_empty() {
                    s.name = format!("{make} {model}").trim().to_string();
                }
            }
            wl_output::Event::Mode { flags: WEnum::Value(flags), width, height, refresh } if flags.contains(wl_output::Mode::Current) => {
                s.width = width;
                s.height = height;
                s.refresh = refresh;
            }
            wl_output::Event::Name { name } => s.connector = name,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_monitor_name() {
        let mut edid = vec![0u8; 128];
        edid[54 + 18..54 + 18 + 5].copy_from_slice(&[0, 0, 0, 0xFC, 0]);
        edid[54 + 18 + 5..54 + 18 + 13].copy_from_slice(b"LG TV\n  ");
        assert_eq!(edid_name(&edid).as_deref(), Some("LG TV"));
        assert_eq!(diagonal(600.0, 340.0), 27);
    }
}
