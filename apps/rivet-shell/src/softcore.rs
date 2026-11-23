//! Softcore's settings, read and changed by asking Softcore itself.
//!
//! Softcore keeps one hand-editable file and owns its format. It describes every plain
//! setting (`softcore settings describe`: the key, its kind, range, default and label),
//! lists what each is now (`softcore settings`), and changes one line at a time
//! (`softcore settings set KEY VALUE`, `unset KEY`), leaving comments, order and everything
//! typed by hand as they were. Softcore reads the file again by itself when it changes. So
//! the Settings window draws its controls from the description and never writes the file:
//! a newer Softcore's settings appear with no change here, and nothing in this module runs
//! unless Softcore is the compositor.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde_json::Value;

/// What a setting holds, which decides the control drawn for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64, step: f64 },
    Bool,
    /// `#rrggbb`.
    Color,
    Choice(Vec<String>),
    /// Words, and any kind this version doesn't know: typed as Softcore reads them.
    Text,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Setting {
    pub key: String,
    /// The card it is under: "Windows", "Keyboard"…
    pub group: String,
    pub label: String,
    pub help: String,
    pub default: String,
    pub kind: Kind,
    /// Empty means "follow": the edges follow the gaps, colours follow Rivet.
    pub optional: bool,
}

/// A key binding written in the settings file (Softcore's own aren't listed: they stay
/// unless its "own key bindings" setting is off).
#[derive(Clone, Debug, PartialEq)]
pub struct Bind {
    pub keys: String,
    pub action: String,
}

/// What a setting is now, and whether the file says so (else it is the default, or follows).
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Now {
    pub text: String,
    pub in_file: bool,
}

#[derive(Clone, Debug, Default)]
pub struct State {
    /// The settings file, and files it may include, watched for changes made by hand.
    pub files: Vec<PathBuf>,
    pub settings: Vec<Setting>,
    pub values: HashMap<String, Now>,
    pub binds: Vec<Bind>,
}

/// What a setting is about, from its key: `shadow_size` and `shadows` are both `shadow`.
fn about(key: &str) -> &str {
    let word = key.split('_').next().unwrap_or(key);
    word.strip_suffix('s').unwrap_or(word)
}

/// A card's settings in the parts they are drawn in: the ones about one thing together (the
/// borders, the shadows, the blur), in the order Softcore first names each, and the ones
/// that stand alone as one part. That part comes first when the card starts with one of
/// them, and last otherwise. Softcore's order is kept inside each part.
pub fn parts<'a>(list: &[&'a Setting]) -> Vec<Vec<&'a Setting>> {
    let family = |s: &Setting| list.iter().filter(|x| about(&x.key) == about(&s.key)).count() > 1;
    let mut out: Vec<Vec<&Setting>> = Vec::new();
    let mut alone: Vec<&Setting> = Vec::new();
    for s in list {
        if !family(s) {
            alone.push(s);
        } else if let Some(part) = out.iter_mut().find(|p| about(&p[0].key) == about(&s.key)) {
            part.push(s);
        } else {
            out.push(vec![s]);
        }
    }
    if !alone.is_empty() {
        let first = list.first().is_some_and(|s| !family(s));
        out.insert(if first { 0 } else { out.len() }, alone);
    }
    out
}

impl State {
    /// The groups in the order Softcore lists them.
    pub fn groups(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for s in &self.settings {
            if !out.contains(&s.group) {
                out.push(s.group.clone());
            }
        }
        out
    }

    #[cfg(test)]
    pub fn value(&self, key: &str) -> &str {
        self.values.get(key).map(|n| n.text.as_str()).unwrap_or("")
    }
}

/// The table `softcore settings describe` prints, and the file it is about.
pub fn parse_table(json: &str) -> Result<(Vec<Setting>, Option<PathBuf>), String> {
    let v: Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let text = |o: &Value, k: &str| o.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let list = v.get("settings").and_then(Value::as_array).ok_or("no settings in the description")?;
    let settings = list
        .iter()
        .filter_map(|o| {
            let key = text(o, "key");
            let kind = o.get("kind")?;
            let num = |k: &str| kind.get(k).and_then(Value::as_f64);
            let parsed = match kind.get("type").and_then(Value::as_str)? {
                "int" => Kind::Int { min: num("min")? as i64, max: num("max")? as i64 },
                "float" => Kind::Float { min: num("min")?, max: num("max")?, step: num("step").filter(|s| *s > 0.0).unwrap_or(0.05) },
                "bool" => Kind::Bool,
                "color" => Kind::Color,
                "choice" => Kind::Choice(kind.get("choices")?.as_array()?.iter().filter_map(|c| c.as_str().map(String::from)).collect()),
                _ => Kind::Text,
            };
            (!key.is_empty()).then(|| Setting { key, group: text(o, "group"), label: text(o, "label"), help: text(o, "help"), default: text(o, "default"), kind: parsed, optional: kind.get("optional").and_then(Value::as_bool).unwrap_or(false) })
        })
        .collect();
    Ok((settings, v.get("file").and_then(Value::as_str).filter(|f| !f.is_empty()).map(PathBuf::from)))
}

/// What `softcore settings` prints: a line for each setting, starred when the file sets it,
/// with its label after it, then the lines there are many of (`bind.KEYS=ACTION`…).
pub fn parse_listing(text: &str, settings: &[Setting]) -> (HashMap<String, Now>, Vec<Bind>) {
    let mut values = HashMap::new();
    let mut binds = Vec::new();
    for line in text.lines() {
        let (in_file, rest) = match line.strip_prefix('*') {
            Some(r) => (true, r),
            None => (false, line.trim_start()),
        };
        let Some((key, value)) = rest.split_once('=') else { continue };
        let key = key.trim();
        if key.starts_with('#') {
            continue;
        }
        if let Some(keys) = key.strip_prefix("bind.") {
            binds.push(Bind { keys: keys.to_string(), action: value.trim().to_string() });
        } else if let Some(s) = settings.iter().find(|s| s.key == key) {
            // The label follows the value as a comment; a colour's `#` is part of the value.
            let value = value.trim_end();
            let value = value.strip_suffix(s.label.as_str()).and_then(|v| v.trim_end().strip_suffix('#')).unwrap_or(value);
            values.insert(key.to_string(), Now { text: value.trim().to_string(), in_file });
        }
    }
    (values, binds)
}

fn ask(args: &[&str]) -> Result<String, String> {
    let out = Command::new("softcore").args(args).stdin(Stdio::null()).output().map_err(|e| format!("Softcore's program couldn't be run ({e})"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).into_owned());
    }
    let said = String::from_utf8_lossy(&out.stderr);
    let said = said.lines().next().unwrap_or("").trim();
    Err(said.strip_prefix("softcore settings:").unwrap_or(said).trim().to_string())
}

/// Everything the page shows. Runs Softcore twice: call it from a thread.
pub fn read() -> Result<State, String> {
    let (settings, file) = parse_table(&ask(&["settings", "describe"])?)?;
    let (values, binds) = parse_listing(&ask(&["settings"])?, &settings);
    // The window-look file sits beside the settings and is included by them.
    let files = file.into_iter().flat_map(|f| [f.with_file_name("rivet-shell.settings"), f]).collect();
    Ok(State { files, settings, values, binds })
}

/// Change one setting or binding (`bind.Mod+T`). Softcore checks the value first and says
/// why when it won't do.
pub fn set(key: &str, value: &str) -> Result<(), String> {
    ask(&["settings", "set", key, value]).map(|_| ())
}

/// Take a line out of the file: a setting goes back to its default, a binding goes.
pub fn unset(key: &str) -> Result<(), String> {
    ask(&["settings", "unset", key]).map(|_| ())
}

/// A table and a listing as Softcore prints them, for checks and tests.
pub fn sample() -> State {
    let (settings, _) = parse_table(SAMPLE_TABLE).unwrap_or_default();
    let (values, binds) = parse_listing(SAMPLE_LISTING, &settings);
    State { files: Vec::new(), settings, values, binds }
}

const SAMPLE_TABLE: &str = r##"{"file":"/home/rivet/.config/softcore/settings","keyed":[],"lists":["spawn"],"settings":[
{"default":"12","group":"Windows","help":"In pixels.","key":"gaps","kind":{"max":200,"min":0,"type":"int"},"label":"Gaps between windows"},
{"default":"","group":"Windows","help":"In pixels. Empty: the same as between windows.","key":"gaps_out","kind":{"max":400,"min":0,"optional":true,"type":"int"},"label":"Gaps at the screen's edges"},
{"default":"","group":"Windows","help":"Empty: Rivet's accent.","key":"border_active","kind":{"optional":true,"type":"color"},"label":"Focused window's border"},
{"default":"","group":"Windows","help":"A second colour makes the border a gradient from the first to this one. Empty: one colour.","key":"border_active_to","kind":{"optional":true,"type":"color"},"label":"Focused window's border runs to"},
{"default":"","group":"Windows","help":"Empty: the Rivet theme's outline.","key":"border_inactive","kind":{"optional":true,"type":"color"},"label":"Other windows' border"},
{"default":"","group":"Windows","help":"Empty: one colour.","key":"border_inactive_to","kind":{"optional":true,"type":"color"},"label":"Other windows' border runs to"},
{"default":"0.4","group":"Windows","help":"","key":"shadow_opacity","kind":{"max":1.0,"min":0.0,"step":0.05,"type":"float"},"label":"Shadow strength"},
{"default":"#000000","group":"Windows","help":"","key":"shadow_color","kind":{"type":"color"},"label":"Shadow colour"},
{"default":"true","group":"Windows","help":"","key":"blur","kind":{"type":"bool"},"label":"Blur behind see-through windows"},
{"default":"5","group":"Windows","help":"","key":"blur_strength","kind":{"max":10,"min":1,"type":"int"},"label":"Blur strength"},
{"default":"horizontal","group":"Columns","help":"horizontal: columns side by side, scrolling left and right. vertical: rows one under the other, scrolling up and down. auto: vertical on a display taller than it is wide.","key":"scroll","kind":{"choices":["horizontal","vertical","auto"],"type":"choice"},"label":"Windows scroll"},
{"default":"0.3333,0.5,0.6667","group":"Columns","help":"The widths the next-width key steps through, as parts of the screen.","key":"column_widths","kind":{"type":"widths"},"label":"Column widths"},
{"default":"0.5","group":"Columns","help":"","key":"default_width","kind":{"max":1.0,"min":0.1,"step":0.05,"type":"float"},"label":"A new column's width"},
{"default":"true","group":"Motion","help":"","key":"animations","kind":{"type":"bool"},"label":"Animations"},
{"default":"1","group":"Motion","help":"","key":"animation_speed","kind":{"max":4.0,"min":0.25,"step":0.25,"type":"float"},"label":"Animation speed"},
{"default":"auto","group":"Keyboard","help":"auto: Super on displays of its own, Alt in a window of another desktop.","key":"mod","kind":{"choices":["auto","super","alt"],"type":"choice"},"label":"The Mod key"},
{"default":"true","group":"Keyboard","help":"","key":"default_binds","kind":{"type":"bool"},"label":"Softcore's own key bindings"},
{"default":"","group":"Keyboard","help":"As xkb names it: us, de, us,ru. Empty: the system's.","key":"xkb_layout","kind":{"type":"text"},"label":"Layout"},
{"default":"600","group":"Keyboard","help":"In milliseconds.","key":"repeat_delay","kind":{"max":5000,"min":50,"type":"int"},"label":"Delay before a held key repeats"},
{"default":"true","group":"Pointer","help":"","key":"tap_to_click","kind":{"type":"bool"},"label":"Tap to click"},
{"default":"false","group":"Pointer","help":"","key":"natural_scroll","kind":{"type":"bool"},"label":"Natural scrolling"},
{"default":"24","group":"Pointer","help":"","key":"cursor_size","kind":{"max":256,"min":8,"type":"int"},"label":"Cursor size"}
]}"##;

const SAMPLE_LISTING: &str = "# /home/rivet/.config/softcore/settings

# Windows
*gaps=14                                  # Gaps between windows
 gaps_out=                                # Gaps at the screen's edges
 border_active=                           # Focused window's border
*border_active_to=#e5bd9b                 # Focused window's border runs to
*border_inactive=#45454a                  # Other windows' border
 border_inactive_to=                      # Other windows' border runs to
 shadow_opacity=0.4                       # Shadow strength
*shadow_color=#1a1030                     # Shadow colour
 blur=true                                # Blur behind see-through windows
 blur_strength=5                          # Blur strength

# Columns
*scroll=auto                              # Windows scroll
 column_widths=0.3333,0.5,0.6667          # Column widths
 default_width=0.5                        # A new column's width

# Motion
 animations=true                          # Animations
*animation_speed=1.25                     # Animation speed

# Keyboard
 mod=auto                                 # The Mod key
 default_binds=true                       # Softcore's own key bindings
*xkb_layout=us,de                         # Layout
 repeat_delay=600                         # Delay before a held key repeats

# Pointer
 tap_to_click=true                        # Tap to click
*natural_scroll=true                      # Natural scrolling
 cursor_size=24                           # Cursor size
*bind.Mod+D=title=\"Launcher\" spawn rivet-shell msg launcher
*bind.Mod+Return=spawn rivet-terminal
*bind.Mod+Shift+S=screenshot
*bind.Mod+E=
*rule.pip=title=\"Picture-in-Picture\" floating
*spawn=rivet-shell
";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_cards_settings_fall_into_parts() {
        let set = |key: &str| Setting { key: key.into(), group: String::new(), label: String::new(), help: String::new(), default: String::new(), kind: Kind::Text, optional: false };
        let keys = |list: &[&str]| {
            let all: Vec<Setting> = list.iter().map(|k| set(k)).collect();
            parts(&all.iter().collect::<Vec<_>>()).iter().map(|p| p.iter().map(|s| s.key.clone()).collect::<Vec<_>>().join(" ")).collect::<Vec<_>>()
        };
        assert_eq!(
            keys(&["gaps", "gaps_out", "border", "radius", "border_active", "shadows", "shadow_size", "xwayland", "blur", "blur_strength"]),
            ["gaps gaps_out", "border border_active", "shadows shadow_size", "blur blur_strength", "radius xwayland"]
        );
        // Starting with ones that stand alone: they stay first.
        assert_eq!(keys(&["theme", "accent", "screenshot_dir", "background", "screenshot_edit"]), ["theme accent background", "screenshot_dir screenshot_edit"]);
        assert_eq!(keys(&["scroll", "tab_bar"]), ["scroll tab_bar"]);
    }

    #[test]
    fn reads_softcores_table_and_listing() {
        let s = sample();
        assert_eq!(s.groups(), ["Windows", "Columns", "Motion", "Keyboard", "Pointer"]);
        let get = |k: &str| s.settings.iter().find(|x| x.key == k).unwrap();
        assert_eq!(get("gaps").kind, Kind::Int { min: 0, max: 200 });
        assert!(get("gaps_out").optional && !get("gaps").optional);
        assert_eq!(get("animation_speed").kind, Kind::Float { min: 0.25, max: 4.0, step: 0.25 });
        assert_eq!(get("scroll").kind, Kind::Choice(vec!["horizontal".into(), "vertical".into(), "auto".into()]));
        // A kind this version doesn't know is still shown, as words.
        assert_eq!(get("column_widths").kind, Kind::Text);
        assert_eq!(s.values["gaps"], Now { text: "14".into(), in_file: true });
        assert_eq!(s.values["gaps_out"], Now { text: String::new(), in_file: false });
        // A colour's `#` is the value's, not the start of the label after it.
        assert_eq!(s.value("shadow_color"), "#1a1030");
        assert_eq!(s.value("border_active"), "");
        assert_eq!(s.value("xkb_layout"), "us,de");
        assert_eq!(s.value("column_widths"), "0.3333,0.5,0.6667");
        // Bindings keep everything after their first `=`; other named lines aren't bindings.
        assert_eq!(s.binds.len(), 4);
        assert_eq!(s.binds[0], Bind { keys: "Mod+D".into(), action: "title=\"Launcher\" spawn rivet-shell msg launcher".into() });
        assert_eq!(s.binds[3], Bind { keys: "Mod+E".into(), action: String::new() });
    }

    #[test]
    fn a_table_that_isnt_one_is_refused() {
        assert!(parse_table("not json").is_err());
        assert!(parse_table("{}").is_err());
        // A setting with no kind is left out; the rest still load.
        let (list, file) = parse_table(r#"{"settings":[{"key":"a","label":"A"},{"key":"b","label":"B","kind":{"type":"bool"}}]}"#).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].key, "b");
        assert!(file.is_none());
    }

    /// With Softcore installed: its real table and listing are read without loss.
    /// `cargo test -p rivet-shell softcore_table_live -- --ignored`
    #[test]
    #[ignore]
    fn softcore_table_live() {
        let s = read().unwrap();
        assert!(s.settings.len() > 20);
        for x in &s.settings {
            assert!(s.values.contains_key(&x.key), "{} has no value in the listing", x.key);
            if let (Kind::Int { .. } | Kind::Float { .. }, false) = (&x.kind, s.value(&x.key).is_empty()) {
                assert!(s.value(&x.key).parse::<f64>().is_ok(), "{}={:?} isn't a number", x.key, s.value(&x.key));
            }
            if x.kind == Kind::Color && !s.value(&x.key).is_empty() {
                assert!(s.value(&x.key).starts_with('#'), "{}={:?}", x.key, s.value(&x.key));
            }
        }
    }
}
