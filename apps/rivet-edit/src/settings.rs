//! Rivet Edit's settings, kept in `~/.config/rivet-edit/settings` as plain `key=value` lines.
//! Checks (screenshot runs) never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Interface text size in points; 0 follows the desktop's.
    pub text_size: f32,
    pub builtin_icons: bool,
    /// Files and folders get Material Design icons in a colour per type (else the icon
    /// theme's).
    pub typed_icons: bool,
    pub show_menubar: bool,
    pub show_sidebar: bool,
    pub show_statusbar: bool,
    /// The code font (a monospace family; empty: the system's).
    pub code_font: String,
    pub code_size: f32,
    /// The space each line of code takes, as a multiple of the font's own (1.0: as the font
    /// sets it).
    pub line_height: f32,
    /// Spaces a Tab key puts in (or a tab's width, with `use_tabs`), for new files and files
    /// whose indentation can't be told.
    pub tab_width: usize,
    /// The Tab key puts in a tab rather than spaces (for new files and undetected ones).
    pub use_tabs: bool,
    /// Follow each file's own indentation (tabs or spaces, and how many).
    pub detect_indent: bool,
    /// Suggestions open as you type (Ctrl+Space always opens them).
    pub autocomplete: bool,
    /// Use the language's server (completion and errors) when one is installed.
    pub lsp: bool,
    /// Vim keys in the editor (Normal, Insert and Visual modes).
    pub vim_keys: bool,
    pub word_wrap: bool,
    pub line_numbers: bool,
    /// The editor's color scheme by name (empty: follow the app's colors).
    pub code_scheme: String,
    /// Folders and files opened before, newest first.
    pub recent: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0xe6, 0x61, 0x00],
            follow_desktop_colors: true,
            text_size: 0.0,
            builtin_icons: true,
            typed_icons: true,
            show_menubar: true,
            show_sidebar: true,
            show_statusbar: true,
            code_font: String::new(),
            code_size: 14.0,
            line_height: 1.0,
            tab_width: 4,
            use_tabs: false,
            detect_indent: true,
            autocomplete: true,
            lsp: true,
            vim_keys: false,
            word_wrap: false,
            line_numbers: true,
            code_scheme: String::new(),
            recent: Vec::new(),
        }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-edit/settings")
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
                "builtin_icons" => s.builtin_icons = b,
                "typed_icons" => s.typed_icons = b,
                "show_menubar" => s.show_menubar = b,
                "show_sidebar" => s.show_sidebar = b,
                "show_statusbar" => s.show_statusbar = b,
                "code_font" => s.code_font = v.to_string(),
                "code_size" => s.code_size = v.parse::<f32>().unwrap_or(14.0).clamp(8.0, 32.0),
                "line_height" => s.line_height = v.parse::<f32>().unwrap_or(1.0).clamp(1.0, 2.0),
                "tab_width" => s.tab_width = v.parse::<usize>().unwrap_or(4).clamp(1, 8),
                "use_tabs" => s.use_tabs = b,
                "detect_indent" => s.detect_indent = b,
                "autocomplete" => s.autocomplete = b,
                "lsp" => s.lsp = b,
                "vim_keys" => s.vim_keys = b,
                "word_wrap" => s.word_wrap = b,
                "line_numbers" => s.line_numbers = b,
                "code_scheme" => s.code_scheme = v.to_string(),
                "recent" => s.recent = v.split(SEP).filter(|x| !x.is_empty()).map(PathBuf::from).collect(),
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
        let recent = self.recent.iter().map(|p| p.to_string_lossy().replace(['\n', SEP], " ")).collect::<Vec<_>>().join(&SEP.to_string());
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nbuiltin_icons={}\ntyped_icons={}\nshow_menubar={}\nshow_sidebar={}\nshow_statusbar={}\ncode_font={}\ncode_size={}\nline_height={}\ntab_width={}\nuse_tabs={}\ndetect_indent={}\nautocomplete={}\nlsp={}\nvim_keys={}\nword_wrap={}\nline_numbers={}\ncode_scheme={}\nrecent={recent}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.builtin_icons,
            self.typed_icons,
            self.show_menubar,
            self.show_sidebar,
            self.show_statusbar,
            self.code_font.replace('\n', ""),
            self.code_size,
            self.line_height,
            self.tab_width,
            self.use_tabs,
            self.detect_indent,
            self.autocomplete,
            self.lsp,
            self.vim_keys,
            self.word_wrap,
            self.line_numbers,
            self.code_scheme.replace('\n', ""),
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

    /// Remember a folder or file as opened most recently.
    pub fn remember(&mut self, p: PathBuf) {
        self.recent.retain(|x| *x != p);
        self.recent.insert(0, p);
        self.recent.truncate(12);
    }
}
