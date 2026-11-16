//! Rivet FTP's preferences, kept in `~/.config/rivet-ftp/settings` as plain `key=value` lines.
//! Checks (screenshot runs) start from the defaults and never save.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use rivet_look::theme::Mode;

/// Set for screenshot runs: settings are never written.
pub static NO_SAVE: AtomicBool = AtomicBool::new(false);

/// One thing on the toolbar. The quick-connect field and the menu button can be moved but
/// not removed; the menu button shows only while the menu bar is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Menu,
    Connections,
    NewConnection,
    QuickConnect,
    Status,
    Disconnect,
    Up,
    Refresh,
    Home,
    ServerHome,
    NewFolder,
    Upload,
    Download,
    EditRemote,
    Properties,
    ShowHidden,
    Transfers,
    Preferences,
    Separator,
}

impl Tool {
    pub const ALL: [Tool; 19] = [
        Tool::Menu,
        Tool::Connections,
        Tool::NewConnection,
        Tool::QuickConnect,
        Tool::Status,
        Tool::Disconnect,
        Tool::Up,
        Tool::Refresh,
        Tool::Home,
        Tool::ServerHome,
        Tool::NewFolder,
        Tool::Upload,
        Tool::Download,
        Tool::EditRemote,
        Tool::Properties,
        Tool::ShowHidden,
        Tool::Transfers,
        Tool::Preferences,
        Tool::Separator,
    ];

    pub fn default_layout() -> Vec<Tool> {
        use Tool::*;
        vec![Menu, Connections, NewConnection, Separator, QuickConnect, Status, Disconnect, Separator, Upload, Download]
    }

    /// Can't be taken off the toolbar.
    pub fn fixed(self) -> bool {
        matches!(self, Tool::Menu | Tool::QuickConnect)
    }

    pub fn id(self) -> &'static str {
        use Tool::*;
        match self {
            Menu => "menu",
            Connections => "connections",
            NewConnection => "new-connection",
            QuickConnect => "quick-connect",
            Status => "status",
            Disconnect => "disconnect",
            Up => "up",
            Refresh => "refresh",
            Home => "home",
            ServerHome => "server-home",
            NewFolder => "new-folder",
            Upload => "upload",
            Download => "download",
            EditRemote => "edit",
            Properties => "properties",
            ShowHidden => "show-hidden",
            Transfers => "transfers",
            Preferences => "preferences",
            Separator => "separator",
        }
    }

    pub fn label(self) -> &'static str {
        use Tool::*;
        match self {
            Menu => "Menu (when the menu bar is hidden)",
            Connections => "Saved Connections",
            NewConnection => "New Connection",
            QuickConnect => "Quick Connect",
            Status => "Connection Status",
            Disconnect => "Disconnect",
            Up => "Up",
            Refresh => "Reload",
            Home => "Home Folder",
            ServerHome => "Server Home",
            NewFolder => "New Folder",
            Upload => "Upload",
            Download => "Download",
            EditRemote => "Open in Editor",
            Properties => "Properties",
            ShowHidden => "Show Hidden Files",
            Transfers => "Transfers",
            Preferences => "Preferences",
            Separator => "Separator",
        }
    }

    /// Icon names to try (symbolic first), and a short text when none is found.
    pub fn icons(self) -> (&'static [&'static str], &'static str) {
        use Tool::*;
        match self {
            Menu => (&[], "≡"),
            Connections => (&["network-server-symbolic", "network-workgroup-symbolic", "folder-remote-symbolic"], "Sites"),
            NewConnection => (&["list-add-symbolic", "bookmark-new-symbolic", "list-add"], "+"),
            QuickConnect => (&["network-server-symbolic", "network-workgroup-symbolic"], "…"),
            Status => (&["network-wired-symbolic", "network-idle-symbolic"], "•"),
            Disconnect => (&["network-offline-symbolic", "process-stop-symbolic", "window-close-symbolic"], "×"),
            Up => (&["go-up-symbolic", "go-up"], "Up"),
            Refresh => (&["view-refresh-symbolic", "view-refresh"], "⟳"),
            Home => (&["user-home-symbolic", "go-home-symbolic", "user-home"], "Home"),
            ServerHome => (&["folder-remote-symbolic", "network-server-symbolic"], "Srv"),
            NewFolder => (&["folder-new-symbolic", "folder-new"], "+"),
            Upload => (&["document-send-symbolic", "send-to-symbolic", "go-up-symbolic"], "Up"),
            Download => (&["folder-download-symbolic", "document-save-symbolic", "go-down-symbolic"], "Down"),
            EditRemote => (&["document-edit-symbolic", "accessories-text-editor-symbolic", "text-editor-symbolic"], "Edit"),
            Properties => (&["document-properties-symbolic", "dialog-information-symbolic", "document-properties"], "i"),
            ShowHidden => (&["view-reveal-symbolic", "view-visible-symbolic", "eye-open-symbolic"], "Hid"),
            Transfers => (&["network-transmit-receive-symbolic", "view-list-symbolic"], "Q"),
            Preferences => (&["preferences-system-symbolic", "emblem-system-symbolic", "preferences-system"], "Prefs"),
            Separator => (&[], "|"),
        }
    }

    pub fn from_id(s: &str) -> Option<Tool> {
        // "info" was the old name of Properties.
        let s = if s == "info" { "properties" } else { s };
        Tool::ALL.into_iter().find(|t| t.id() == s)
    }
}

/// "menu,new-connection,quick-connect" → the tools; unknown names are skipped, and the fixed
/// ones are put back if a hand-edited file lost them.
pub fn parse_toolbar(v: &str) -> Vec<Tool> {
    let mut t: Vec<Tool> = v.split(',').filter_map(|s| Tool::from_id(s.trim())).collect();
    if !t.contains(&Tool::QuickConnect) {
        t.push(Tool::QuickConnect);
    }
    if !t.contains(&Tool::Menu) {
        t.insert(0, Tool::Menu);
    }
    t
}

/// What to do when a file being sent is already there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnExists {
    Ask,
    Replace,
    Skip,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub theme: Mode,
    pub accent: [u8; 3],
    pub follow_desktop_colors: bool,
    /// Text size in points; 0 follows the desktop's.
    pub text_size: f32,
    pub show_menubar: bool,
    pub show_sidebar: bool,
    pub show_queue: bool,
    pub show_hidden: bool,
    pub on_exists: OnExists,
    /// Double-clicking a file sends it to the other side.
    pub double_click_sends: bool,
    pub confirm_delete: bool,
    pub clear_finished: bool,
    /// Start the local pane where it was last time (else the home folder).
    pub remember_local: bool,
    pub last_local: String,
    /// Desktop app id of the editor for server files; empty: the default app for each type.
    pub editor: String,
    pub toolbar: Vec<Tool>,
    /// Sidebar categories shown folded up.
    pub collapsed: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: Mode::System,
            accent: [0x35, 0x84, 0xe4],
            follow_desktop_colors: true,
            text_size: 0.0,
            show_menubar: true,
            show_sidebar: true,
            show_queue: true,
            show_hidden: false,
            on_exists: OnExists::Ask,
            double_click_sends: true,
            confirm_delete: true,
            clear_finished: false,
            remember_local: true,
            last_local: String::new(),
            editor: String::new(),
            toolbar: Tool::default_layout(),
            collapsed: Vec::new(),
        }
    }
}

fn path() -> PathBuf {
    rivet_xdg::places::config_home().join("rivet-ftp/settings")
}

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
                    if let Some(c) = parse_hex(v) {
                        s.accent = c;
                    }
                }
                "follow_desktop_colors" => s.follow_desktop_colors = b,
                "text_size" => s.text_size = v.parse::<f32>().unwrap_or(0.0).clamp(0.0, 24.0),
                "show_menubar" => s.show_menubar = b,
                "show_sidebar" => s.show_sidebar = b,
                "show_queue" => s.show_queue = b,
                "show_hidden" => s.show_hidden = b,
                "on_exists" => s.on_exists = match v { "replace" => OnExists::Replace, "skip" => OnExists::Skip, _ => OnExists::Ask },
                "double_click_sends" => s.double_click_sends = b,
                "confirm_delete" => s.confirm_delete = b,
                "clear_finished" => s.clear_finished = b,
                "remember_local" => s.remember_local = b,
                "last_local" => s.last_local = v.to_string(),
                "editor" => s.editor = v.to_string(),
                "toolbar" => s.toolbar = parse_toolbar(v),
                // Names are kept apart by the unit separator, which no one types.
                "collapsed" => s.collapsed = v.split('\u{1f}').filter(|x| !x.is_empty()).map(str::to_string).collect(),
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
        let on_exists = match self.on_exists {
            OnExists::Ask => "ask",
            OnExists::Replace => "replace",
            OnExists::Skip => "skip",
        };
        let [r, g, b] = self.accent;
        let text = format!(
            "theme={theme}\naccent=#{r:02x}{g:02x}{b:02x}\nfollow_desktop_colors={}\ntext_size={}\nshow_menubar={}\nshow_sidebar={}\nshow_queue={}\nshow_hidden={}\non_exists={on_exists}\ndouble_click_sends={}\nconfirm_delete={}\nclear_finished={}\nremember_local={}\nlast_local={}\neditor={}\ntoolbar={}\ncollapsed={}\n",
            self.follow_desktop_colors,
            self.text_size,
            self.show_menubar,
            self.show_sidebar,
            self.show_queue,
            self.show_hidden,
            self.double_click_sends,
            self.confirm_delete,
            self.clear_finished,
            self.remember_local,
            self.last_local.replace('\n', ""),
            self.editor.replace('\n', ""),
            self.toolbar.iter().map(|t| t.id()).collect::<Vec<_>>().join(","),
            self.collapsed.iter().map(|c| c.replace(['\n', '\u{1f}'], " ")).collect::<Vec<_>>().join("\u{1f}"),
        );
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

fn parse_hex(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(h, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toolbar_round_trip() {
        let t = Tool::default_layout();
        let text: Vec<&str> = t.iter().map(|x| x.id()).collect();
        assert_eq!(parse_toolbar(&text.join(",")), t);
        // Fixed tools come back; unknown names are skipped.
        assert_eq!(parse_toolbar("upload,nonsense"), vec![Tool::Menu, Tool::Upload, Tool::QuickConnect]);
    }

    #[test]
    fn hex_colors() {
        assert_eq!(parse_hex("#3584e4"), Some([0x35, 0x84, 0xe4]));
        assert_eq!(parse_hex("3584e4"), None);
        assert_eq!(parse_hex("#zzz"), None);
    }
}
