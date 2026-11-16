//! Rivet Notes' settings and the list of vaults, kept in `~/.config/rivet-notes/settings` as
//! plain `key=value` lines. Checks (screenshot runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// How the sidebar lists notes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Browse {
    Folders,
    Tags,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    pub text_size: f32,
    pub show_menubar: bool,
    pub show_sidebar: bool,
    /// The sidebar's panels: which slot each is in, tabs, and the split.
    pub layout: crate::layout::Layout,
    /// Show the frontmatter as editable properties above the text.
    pub show_properties: bool,
    /// The vaults, most recently used first.
    pub vaults: Vec<PathBuf>,
    /// Folders (vault-relative) and tags folded up in the sidebar.
    pub collapsed: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0x91, 0x41, 0xac],
            follow_desktop_colors: true,
            text_size: 0.0,
            show_menubar: true,
            show_sidebar: true,
            layout: Default::default(),
            show_properties: true,
            vaults: Vec::new(),
            collapsed: Vec::new(),
        }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-notes/settings")
}

const SEP: char = '\u{1f}';

impl Settings {
    pub fn load() -> Settings {
        let mut s = Settings::default();
        let Ok(text) = std::fs::read_to_string(path()) else { return s };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            let b = v == "true";
            match k.trim() {
                "theme" => s.theme = match v { "light" => Mode::Light, "dark" => Mode::Dark, _ => Mode::System },
                "accent" => {
                    if let Some(h) = v.strip_prefix('#').filter(|h| h.len() == 6)
                        && let Ok(n) = u32::from_str_radix(h, 16)
                    {
                        s.accent = [(n >> 16) as u8, (n >> 8) as u8, n as u8];
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "show_menubar" => s.show_menubar = b,
                "show_sidebar" => s.show_sidebar = b,
                // Older settings: the two fixed arrangements.
                "browse" if v == "tags" => s.layout.show(Browse::Tags),
                "browse" => {}
                "show_properties" => s.show_properties = b,
                "tags_below" if b => s.layout = crate::layout::Layout::stacked(),
                "tags_below" => {}
                "panels" => {
                    let split = s.layout.split;
                    s.layout = crate::layout::Layout::from_text(v, split);
                }
                "split" => s.layout.split = v.parse::<f32>().unwrap_or(0.4).clamp(0.15, 0.85),
                "vaults" => s.vaults = v.split(SEP).filter(|x| !x.is_empty()).map(PathBuf::from).collect(),
                "collapsed" => s.collapsed = v.split(SEP).filter(|x| !x.is_empty()).map(str::to_string).collect(),
                _ => {}
            }
        }
        s
    }

    pub fn save(&self) {
        if NO_SAVE.load(Ordering::Relaxed) {
            return;
        }
        let theme = match self.theme {
            Mode::System => "system",
            Mode::Light => "light",
            Mode::Dark => "dark",
        };
        let [r, g, b] = self.accent;
        let join = |v: Vec<String>| v.into_iter().map(|x| x.replace(['\n', SEP], " ")).collect::<Vec<_>>().join(&SEP.to_string());
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nshow_menubar={}\nshow_sidebar={}\npanels={}\nsplit={}\nshow_properties={}\nvaults={}\ncollapsed={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.show_menubar,
            self.show_sidebar,
            self.layout.to_text(),
            self.layout.split,
            self.show_properties,
            join(self.vaults.iter().map(|p| p.to_string_lossy().into_owned()).collect()),
            join(self.collapsed.clone()),
        );
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }

    /// Make this vault the current one (first in the list).
    pub fn use_vault(&mut self, p: PathBuf) {
        self.vaults.retain(|v| *v != p);
        self.vaults.insert(0, p);
    }
}
