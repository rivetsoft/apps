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
    /// The bar along the bottom: the vault, how many notes, the open note's words.
    pub show_statusbar: bool,
    pub show_sidebar: bool,
    /// The sidebar's panels: which slot each is in, tabs, and the split.
    pub layout: crate::layout::Layout,
    /// Show the frontmatter as editable properties above the text.
    pub show_properties: bool,
    /// Rivet's own icon kit, colored from the theme (else the desktop's icon theme).
    pub builtin_icons: bool,
    /// Font families (empty: the interface follows the desktop, the editor the interface,
    /// monospace the system's).
    pub ui_font: String,
    pub editor_font: String,
    pub mono_font: String,
    /// The editor's text size in points.
    pub editor_size: f32,
    /// Live mode: Markdown shows rendered except on the line being edited (else plain
    /// Markdown with colors).
    pub live_mode: bool,
    /// Line height as a multiple of the text size.
    pub line_spacing: f32,
    /// The note's text in a centered column (else the full width of the window), for notes
    /// without their own choice.
    pub centered: bool,
    /// Notes whose width was chosen in Peek: the file, and whether it's centered.
    pub note_widths: Vec<(PathBuf, bool)>,
    /// Peek, the panel on the right about the open note.
    pub show_peek: bool,
    /// The bar above a note's title showing where it is (vault › folders › file).
    pub show_breadcrumbs: bool,
    /// Module: daily notes (Ctrl+D makes or opens today's note).
    pub daily_notes: bool,
    /// Where daily notes live, inside the vault (empty: the top of the vault).
    pub daily_folder: String,
    /// Where dropped files go, inside the vault.
    pub attachments_folder: String,
    /// Module: super todos — tasks tagged with one of `todo_tags` get details and the Todos page.
    pub super_todos: bool,
    /// The tags that make a task a super todo (without `#`).
    pub todo_tags: Vec<String>,
    /// The Todos page as a board (columns by progress) rather than a list.
    pub todo_board: bool,
    /// The Todos page in a centered column (its own width, apart from the notes').
    pub todo_centered: bool,
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
            show_statusbar: true,
            show_sidebar: true,
            layout: Default::default(),
            show_properties: true,
            builtin_icons: true,
            ui_font: String::new(),
            editor_font: String::new(),
            mono_font: String::new(),
            editor_size: 15.0,
            line_spacing: 1.6,
            live_mode: false,
            centered: false,
            note_widths: Vec::new(),
            daily_notes: false,
            daily_folder: "Daily".into(),
            attachments_folder: "Attachments".into(),
            super_todos: false,
            todo_tags: vec!["task".into()],
            todo_board: false,
            todo_centered: false,
            show_peek: false,
            show_breadcrumbs: true,
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
                "show_statusbar" => s.show_statusbar = b,
                "show_sidebar" => s.show_sidebar = b,
                // Older settings: the two fixed arrangements.
                "browse" if v == "tags" => s.layout.show(Browse::Tags),
                "browse" => {}
                "show_properties" => s.show_properties = b,
                "builtin_icons" => s.builtin_icons = b,
                "ui_font" => s.ui_font = v.to_string(),
                "editor_font" => s.editor_font = v.to_string(),
                "mono_font" => s.mono_font = v.to_string(),
                "line_spacing" => s.line_spacing = v.parse::<f32>().unwrap_or(1.6).clamp(1.1, 2.4),
                "editor_size" => s.editor_size = v.parse::<f32>().unwrap_or(15.0).clamp(10.0, 28.0),
                "tags_below" if b => s.layout = crate::layout::Layout::stacked(),
                "tags_below" => {}
                "panels" => {
                    let split = s.layout.split;
                    s.layout = crate::layout::Layout::from_text(v, split);
                }
                "split" => s.layout.split = v.parse::<f32>().unwrap_or(0.4).clamp(0.15, 0.85),
                "note_width" => s.centered = v == "centered",
                "show_peek" => s.show_peek = b,
                "edit_mode" => s.live_mode = v == "live",
                "show_breadcrumbs" => s.show_breadcrumbs = b,
                "daily_notes" => s.daily_notes = b,
                "super_todos" => s.super_todos = b,
                "todo_view" => s.todo_board = v == "board",
                "todo_width" => s.todo_centered = v == "centered",
                "todo_tags" => s.todo_tags = v.split(',').map(|t| t.trim().trim_start_matches('#').to_string()).filter(|t| !t.is_empty()).collect(),
                "daily_folder" => s.daily_folder = v.trim_matches('/').to_string(),
                "attachments_folder" => s.attachments_folder = v.trim_matches('/').to_string(),
                "note_widths" => {
                    s.note_widths = v
                        .split(SEP)
                        .filter_map(|e| match e.split_at_checked(1)? {
                            ("c", p) if !p.is_empty() => Some((PathBuf::from(p), true)),
                            ("f", p) if !p.is_empty() => Some((PathBuf::from(p), false)),
                            _ => None,
                        })
                        .collect()
                }
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
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nshow_menubar={}\nshow_statusbar={}\nshow_sidebar={}\npanels={}\nsplit={}\nshow_properties={}\nbuiltin_icons={}\nui_font={}\neditor_font={}\nmono_font={}\neditor_size={}\nline_spacing={}\nnote_width={}\nshow_peek={}\nedit_mode={}\nshow_breadcrumbs={}\ndaily_notes={}\nsuper_todos={}\ntodo_view={}\ntodo_width={}\ntodo_tags={}\ndaily_folder={}\nattachments_folder={}\nnote_widths={}\nvaults={}\ncollapsed={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.show_menubar,
            self.show_statusbar,
            self.show_sidebar,
            self.layout.to_text(),
            self.layout.split,
            self.show_properties,
            self.builtin_icons,
            self.ui_font.replace('\n', ""),
            self.editor_font.replace('\n', ""),
            self.mono_font.replace('\n', ""),
            self.editor_size,
            self.line_spacing,
            if self.centered { "centered" } else { "full" },
            self.show_peek,
            if self.live_mode { "live" } else { "source" },
            self.show_breadcrumbs,
            self.daily_notes,
            self.super_todos,
            if self.todo_board { "board" } else { "list" },
            if self.todo_centered { "centered" } else { "full" },
            self.todo_tags.join(",").replace('\n', ""),
            self.daily_folder.replace('\n', ""),
            self.attachments_folder.replace('\n', ""),
            join(self.note_widths.iter().map(|(p, c)| format!("{}{}", if *c { 'c' } else { 'f' }, p.to_string_lossy())).collect()),
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

    /// This note's own width choice, if it has one.
    pub fn note_width(&self, note: &std::path::Path) -> Option<bool> {
        self.note_widths.iter().find(|(p, _)| p == note).map(|(_, c)| *c)
    }

    /// Whether this note's text is centered: its own choice, else the default.
    pub fn centered_for(&self, note: &std::path::Path) -> bool {
        self.note_width(note).unwrap_or(self.centered)
    }

    /// Give a note its own width (`None`: follow the default again).
    pub fn set_note_width(&mut self, note: &std::path::Path, centered: Option<bool>) {
        self.note_widths.retain(|(p, _)| p != note);
        if let Some(c) = centered {
            self.note_widths.push((note.to_path_buf(), c));
        }
    }

    /// A note or folder was renamed or moved: its notes keep their widths.
    pub fn moved(&mut self, from: &std::path::Path, to: &std::path::Path) {
        for (p, _) in &mut self.note_widths {
            if let Ok(rest) = p.strip_prefix(from) {
                *p = if rest.as_os_str().is_empty() { to.to_path_buf() } else { to.join(rest) };
            }
        }
    }

    /// Make this vault the current one (first in the list).
    pub fn use_vault(&mut self, p: PathBuf) {
        self.vaults.retain(|v| *v != p);
        self.vaults.insert(0, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn note_widths_override_and_follow_moves() {
        let mut s = Settings { centered: false, ..Default::default() };
        let a = Path::new("/v/Projects/a.md");
        assert!(!s.centered_for(a));
        s.set_note_width(a, Some(true));
        assert!(s.centered_for(a));
        assert!(!s.centered_for(Path::new("/v/b.md")), "other notes keep the default");
        s.moved(Path::new("/v/Projects"), Path::new("/v/Work"));
        assert_eq!(s.note_width(Path::new("/v/Work/a.md")), Some(true));
        s.set_note_width(Path::new("/v/Work/a.md"), None);
        assert!(s.note_widths.is_empty());
    }
}
