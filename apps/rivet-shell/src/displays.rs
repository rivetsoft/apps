//! Displays: each screen's resolution, refresh rate, scale, rotation and place, read from the
//! compositor and written, with the window look, to the overrides file its config includes
//! (see comp.rs). niri (`niri msg --json outputs`, `output "NAME" { … }`) and Hyprland
//! (`hyprctl monitors all -j`, `monitor = …` in hyprland.conf). Softcore answers as niri does
//! (`softcore msg --json outputs`) and takes `output.NAME=…` lines.

use crate::comp::Format;
use crate::sys::Compositor;

/// A resolution and refresh rate (in thousandths of a hertz).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mode {
    pub w: u32,
    pub h: u32,
    pub mhz: u32,
}

impl Mode {
    pub fn hz(&self) -> f32 {
        self.mhz as f32 / 1000.0
    }
}

/// A screen as it's set (in the settings as `display.NAME=on=true;mode=2560x1440@59951;…`).
#[derive(Clone, Debug, PartialEq)]
pub struct Display {
    pub name: String,
    pub on: bool,
    /// None: the screen's own preferred mode.
    pub mode: Option<Mode>,
    pub scale: f32,
    /// Its top left corner among the screens, in points (pixels over the scale).
    pub pos: (i32, i32),
    /// Turned clockwise: 0, 90, 180 or 270; and mirrored.
    pub rotate: u32,
    pub flip: bool,
    /// Variable refresh rate.
    pub vrr: bool,
}

impl Display {
    pub fn text(&self) -> String {
        let mode = self.mode.map(|m| format!("{}x{}@{}", m.w, m.h, m.mhz)).unwrap_or_default();
        format!("on={};mode={mode};scale={};pos={},{};rotate={};flip={};vrr={}", self.on, self.scale, self.pos.0, self.pos.1, self.rotate, self.flip, self.vrr)
    }

    pub fn parse(name: &str, v: &str) -> Display {
        let mut d = Display { name: name.to_string(), on: true, mode: None, scale: 1.0, pos: (0, 0), rotate: 0, flip: false, vrr: false };
        for part in v.split(';') {
            let Some((k, val)) = part.split_once('=') else { continue };
            let val = val.trim();
            match k.trim() {
                "on" => d.on = val != "false",
                "mode" => d.mode = parse_mode(val),
                "scale" => d.scale = val.parse().unwrap_or(1.0f32).clamp(0.25, 10.0),
                "pos" => d.pos = val.split_once(',').and_then(|(x, y)| Some((x.trim().parse().ok()?, y.trim().parse().ok()?))).unwrap_or((0, 0)),
                "rotate" => d.rotate = val.parse::<u32>().ok().filter(|r| [0, 90, 180, 270].contains(r)).unwrap_or(0),
                "flip" => d.flip = val == "true",
                "vrr" => d.vrr = val == "true",
                _ => {}
            }
        }
        d
    }

    /// Its size among the screens, in points: the mode over the scale, turned.
    pub fn size(&self, fallback: Mode) -> (f32, f32) {
        let m = self.mode.unwrap_or(fallback);
        let (w, h) = (m.w as f32 / self.scale.max(0.25), m.h as f32 / self.scale.max(0.25));
        if self.rotate % 180 == 90 { (h, w) } else { (w, h) }
    }
}

/// "2560x1440@59951".
fn parse_mode(v: &str) -> Option<Mode> {
    let (size, mhz) = v.split_once('@')?;
    let (w, h) = size.split_once('x')?;
    Some(Mode { w: w.trim().parse().ok()?, h: h.trim().parse().ok()?, mhz: mhz.trim().parse().ok()? })
}

/// A connected screen, as the compositor tells it.
#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    /// "Dell Inc. AW2725DM".
    pub what: String,
    pub modes: Vec<Mode>,
    pub preferred: Mode,
    pub vrr_supported: bool,
    /// How it's set now.
    pub now: Display,
}

/// Whether displays can be set for this compositor's config.
pub fn supported(format: Format) -> bool {
    matches!(format, Format::Kdl | Format::HyprConf | Format::Softcore)
}

/// The connected screens.
pub fn read(comp: Compositor) -> Result<Vec<Output>, String> {
    let (cmd, args): (&str, &[&str]) = match comp {
        Compositor::Niri | Compositor::Softcore => (comp.msg(), &["msg", "--json", "outputs"]),
        Compositor::Hyprland => ("hyprctl", &["monitors", "all", "-j"]),
        _ => return Err(format!("Displays can't be read from {}.", comp.name())),
    };
    let out = std::process::Command::new(cmd).args(args).stdin(std::process::Stdio::null()).output().map_err(|e| format!("couldn't run {cmd}: {e}"))?;
    if !out.status.success() {
        return Err(format!("{cmd} failed: {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut list = if comp == Compositor::Hyprland { from_hyprland(&text) } else { from_niri(&text) }?;
    list.sort_by(|a, b| (a.now.pos, &a.name).cmp(&(b.now.pos, &b.name)));
    // A screen that's off has no place: after the others, so turning it on overlaps nothing.
    let mut right = list.iter().filter(|o| o.now.on).map(|o| o.now.pos.0 + o.now.size(o.preferred).0.round() as i32).max().unwrap_or(0);
    for o in list.iter_mut().filter(|o| !o.now.on) {
        o.now.pos = (right, 0);
        right += o.now.size(o.preferred).0.round() as i32;
    }
    Ok(list)
}

fn turn(name: &str) -> (u32, bool) {
    let n = name.to_lowercase();
    let deg = [270, 180, 90].into_iter().find(|d| n.contains(&d.to_string())).unwrap_or(0);
    (deg, n.contains("flipped"))
}

fn from_niri(text: &str) -> Result<Vec<Output>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("niri's outputs couldn't be read: {e}"))?;
    let outs = v.as_object().ok_or("niri's outputs couldn't be read")?;
    let num = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let text_of = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Ok(outs
        .values()
        .filter_map(|o| {
            let name = text_of(o, "name");
            let modes: Vec<Mode> = o.get("modes")?.as_array()?.iter().map(|m| Mode { w: num(m, "width") as u32, h: num(m, "height") as u32, mhz: num(m, "refresh_rate") as u32 }).collect();
            let preferred = o.get("modes")?.as_array()?.iter().position(|m| m.get("is_preferred").and_then(|p| p.as_bool()) == Some(true)).and_then(|i| modes.get(i).copied()).or(modes.first().copied())?;
            let current = o.get("current_mode").and_then(|c| c.as_u64()).and_then(|i| modes.get(i as usize).copied());
            let logical = o.get("logical").filter(|l| l.is_object());
            let (rotate, flip) = logical.map(|l| turn(&text_of(l, "transform"))).unwrap_or((0, false));
            let now = Display {
                name: name.clone(),
                on: logical.is_some() && current.is_some(),
                mode: current,
                scale: logical.map(|l| num(l, "scale") as f32).filter(|s| *s > 0.0).unwrap_or(1.0),
                pos: logical.map(|l| (num(l, "x") as i32, num(l, "y") as i32)).unwrap_or((0, 0)),
                rotate,
                flip,
                vrr: o.get("vrr_enabled").and_then(|x| x.as_bool()).unwrap_or(false),
            };
            Some(Output { name, what: format!("{} {}", text_of(o, "make"), text_of(o, "model")).trim().to_string(), modes, preferred, vrr_supported: o.get("vrr_supported").and_then(|x| x.as_bool()).unwrap_or(false), now })
        })
        .collect())
}

fn from_hyprland(text: &str) -> Result<Vec<Output>, String> {
    let v: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Hyprland's monitors couldn't be read: {e}"))?;
    let num = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_f64()).unwrap_or(0.0);
    let text_of = |v: &serde_json::Value, k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
    Ok(v.as_array()
        .ok_or("Hyprland's monitors couldn't be read")?
        .iter()
        .map(|o| {
            let name = text_of(o, "name");
            let current = Mode { w: num(o, "width") as u32, h: num(o, "height") as u32, mhz: (num(o, "refreshRate") * 1000.0).round() as u32 };
            // "2560x1440@59.95Hz".
            let mut modes: Vec<Mode> = o.get("availableModes").and_then(|m| m.as_array()).map(|a| a.iter().filter_map(|m| m.as_str()).filter_map(|m| {
                let (size, hz) = m.trim_end_matches("Hz").split_once('@')?;
                let (w, h) = size.split_once('x')?;
                Some(Mode { w: w.parse().ok()?, h: h.parse().ok()?, mhz: (hz.parse::<f64>().ok()? * 1000.0).round() as u32 })
            }).collect()).unwrap_or_default();
            if modes.is_empty() {
                modes.push(current);
            }
            let t = num(o, "transform") as u32;
            let on = o.get("disabled").and_then(|d| d.as_bool()) != Some(true) && current.w > 0;
            // The nearest listed mode to the one running (Hyprland rounds the rate).
            let mode = modes.iter().copied().filter(|m| (m.w, m.h) == (current.w, current.h)).min_by_key(|m| m.mhz.abs_diff(current.mhz));
            let now = Display { name: name.clone(), on, mode: mode.filter(|_| on), scale: (num(o, "scale") as f32).max(0.25), pos: (num(o, "x") as i32, num(o, "y") as i32), rotate: (t % 4) * 90, flip: t >= 4, vrr: o.get("vrr").and_then(|x| x.as_bool()).unwrap_or(false) };
            Output { name, what: format!("{} {}", text_of(o, "make"), text_of(o, "model")).trim().to_string(), preferred: modes[0], modes, vrr_supported: true, now }
        })
        .collect())
}

fn trim_num(v: f32) -> String {
    let s = format!("{v:.3}");
    s.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// The displays' part of the overrides file ("" when none are set).
pub fn render(format: Format, displays: &[Display]) -> String {
    if displays.is_empty() {
        return String::new();
    }
    let mut t = String::new();
    match format {
        Format::Kdl => {
            t.push_str("\n// Displays (Settings, Displays).\n");
            for d in displays {
                t.push_str(&format!("output \"{}\" {{\n", d.name.replace(['"', '\\'], "")));
                if !d.on {
                    t.push_str("    off\n}\n");
                    continue;
                }
                if let Some(m) = d.mode {
                    t.push_str(&format!("    mode \"{}x{}@{:.3}\"\n", m.w, m.h, m.hz()));
                }
                let turn = match (d.flip, d.rotate) {
                    (false, 0) => "normal".to_string(),
                    (false, r) => r.to_string(),
                    (true, 0) => "flipped".to_string(),
                    (true, r) => format!("flipped-{r}"),
                };
                t.push_str(&format!("    scale {}\n    transform \"{turn}\"\n    position x={} y={}\n", trim_num(d.scale), d.pos.0, d.pos.1));
                if d.vrr {
                    t.push_str("    variable-refresh-rate\n");
                }
                t.push_str("}\n");
            }
        }
        Format::HyprConf => {
            t.push_str("\n# Displays (Settings, Displays).\n");
            for d in displays {
                if !d.on {
                    t.push_str(&format!("monitor = {}, disable\n", d.name));
                    continue;
                }
                let mode = d.mode.map(|m| format!("{}x{}@{:.3}", m.w, m.h, m.hz())).unwrap_or_else(|| "preferred".into());
                let turn = d.rotate / 90 + if d.flip { 4 } else { 0 };
                t.push_str(&format!("monitor = {}, {mode}, {}x{}, {}, transform, {turn}, vrr, {}\n", d.name, d.pos.0, d.pos.1, trim_num(d.scale), u8::from(d.vrr)));
            }
        }
        Format::Softcore => {
            t.push_str("\n# Displays (Settings, Displays).\n");
            for d in displays {
                if !d.on {
                    t.push_str(&format!("output.{}=off\n", d.name));
                    continue;
                }
                let mode = d.mode.map(|m| format!("mode={}x{}@{:.3} ", m.w, m.h, m.hz())).unwrap_or_default();
                let turn = match (d.flip, d.rotate) {
                    (false, 0) => "normal".to_string(),
                    (false, r) => r.to_string(),
                    (true, 0) => "flipped".to_string(),
                    (true, r) => format!("flipped-{r}"),
                };
                t.push_str(&format!("output.{}={mode}scale={} position={},{} transform={turn}\n", d.name, trim_num(d.scale), d.pos.0, d.pos.1));
            }
        }
        Format::HyprLua => t.push_str("\n-- Displays aren't written for hyprland.lua yet.\n"),
        Format::MangoConf | Format::MangoToml => t.push_str("\n# Displays aren't written for mango yet.\n"),
    }
    t
}

/// The scales offered: quarters from 1 to 3 (what compositors place sharply), and the one set.
pub fn scales(now: f32) -> Vec<f32> {
    let mut v: Vec<f32> = (4..=12).map(|q| q as f32 / 4.0).collect();
    if !v.iter().any(|s| (s - now).abs() < 0.005) {
        v.push(now);
        v.sort_by(|a, b| a.total_cmp(b));
    }
    v
}

/// Whether two screens share any of the desktop (which compositors refuse, or place anew).
pub fn overlap(a: ((i32, i32), (f32, f32)), b: ((i32, i32), (f32, f32))) -> bool {
    let (ax, ay, bx, by) = (a.0.0 as f32, a.0.1 as f32, b.0.0 as f32, b.0.1 as f32);
    ax < bx + b.1.0 - 0.5 && bx < ax + a.1.0 - 0.5 && ay < by + b.1.1 - 0.5 && by < ay + a.1.1 - 0.5
}

#[cfg(test)]
mod tests {
    use super::*;

    const NIRI: &str = r#"{"eDP-1":{"name":"eDP-1","make":"BOE","model":"0x0A59","modes":[{"width":1920,"height":1200,"refresh_rate":60003,"is_preferred":true}],"current_mode":null,"vrr_supported":false,"vrr_enabled":false,"logical":null},
    "DP-5":{"name":"DP-5","make":"LG Electronics","model":"LG ULTRAFINE","modes":[{"width":3840,"height":2160,"refresh_rate":59996,"is_preferred":true},{"width":3840,"height":2160,"refresh_rate":30000,"is_preferred":false}],"current_mode":1,"vrr_supported":true,"vrr_enabled":true,"logical":{"x":0,"y":0,"width":1728,"height":3072,"scale":1.25,"transform":"Flipped90"}}}"#;

    #[test]
    fn reads_niri_and_writes_it_back() {
        let o = from_niri(NIRI).unwrap();
        let big = o.iter().find(|x| x.name == "DP-5").unwrap();
        assert_eq!(big.what, "LG Electronics LG ULTRAFINE");
        assert_eq!((big.now.mode, big.now.scale, big.now.rotate, big.now.flip, big.now.vrr), (Some(Mode { w: 3840, h: 2160, mhz: 30000 }), 1.25, 90, true, true));
        assert_eq!(big.now.size(big.preferred), (1728.0, 3072.0));
        let lap = o.iter().find(|x| x.name == "eDP-1").unwrap();
        assert!(!lap.now.on && lap.preferred.w == 1920);
        // The settings line and back.
        assert_eq!(Display::parse("DP-5", &big.now.text()), big.now);
        let k = render(Format::Kdl, &[big.now.clone(), lap.now.clone()]);
        assert!(k.contains("output \"DP-5\" {\n    mode \"3840x2160@30.000\"\n    scale 1.25\n    transform \"flipped-90\"\n    position x=0 y=0\n    variable-refresh-rate\n}"));
        assert!(k.contains("output \"eDP-1\" {\n    off\n}"));
        let h = render(Format::HyprConf, &[big.now.clone(), lap.now.clone()]);
        assert!(h.contains("monitor = DP-5, 3840x2160@30.000, 0x0, 1.25, transform, 5, vrr, 1") && h.contains("monitor = eDP-1, disable"));
        assert!(render(Format::Kdl, &[]).is_empty());
    }

    #[test]
    fn writes_softcore() {
        let d = |name: &str, on: bool, mode: Option<Mode>, rotate: u32, flip: bool| Display { name: name.into(), on, mode, scale: 1.5, pos: (2560, 0), rotate, flip, vrr: false };
        let t = render(Format::Softcore, &[d("DP-5", true, Some(Mode { w: 2560, h: 1440, mhz: 59951 }), 0, false), d("HDMI-A-1", true, None, 90, true), d("eDP-1", false, None, 0, false)]);
        assert!(t.contains("output.DP-5=mode=2560x1440@59.951 scale=1.5 position=2560,0 transform=normal\n"), "{t}");
        assert!(t.contains("output.HDMI-A-1=scale=1.5 position=2560,0 transform=flipped-90\n") && t.contains("output.eDP-1=off\n"), "{t}");
    }

    #[test]
    fn reads_hyprland() {
        let o = from_hyprland(r#"[{"name":"DP-1","make":"Dell","model":"U27","width":2560,"height":1440,"refreshRate":143.97301,"x":1920,"y":0,"scale":1.0,"transform":1,"disabled":false,"vrr":false,"availableModes":["2560x1440@143.97Hz","2560x1440@59.95Hz"]}]"#).unwrap();
        assert_eq!((o[0].now.mode, o[0].now.pos, o[0].now.rotate), (Some(Mode { w: 2560, h: 1440, mhz: 143970 }), (1920, 0), 90));
    }

    /// Against the running compositor: `cargo test -p rivet-shell live_screens -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_screens() {
        let comp = Compositor::detect();
        let outs = read(comp).unwrap();
        for o in &outs {
            println!("{} ({}) {} modes, now {}", o.name, o.what, o.modes.len(), o.now.text());
        }
        println!("{}", render(Format::Kdl, &outs.iter().map(|o| o.now.clone()).collect::<Vec<_>>()));
    }

    #[test]
    fn overlaps() {
        let a = ((0, 0), (2560.0, 1440.0));
        assert!(!overlap(a, ((2560, 0), (1920.0, 1080.0))));
        assert!(overlap(a, ((2000, 100), (1920.0, 1080.0))));
        assert_eq!(scales(1.1).len(), 10);
    }
}
