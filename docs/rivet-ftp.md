# Rivet FTP

> **Early alpha.** Rivet FTP is not ready for daily use: things are missing, change without
> notice, or break. Try it on servers and files you don't mind losing, and keep backups.

A dual-pane file-transfer app for Linux (SFTP, FTP and FTPS), built on Rivet's core and
written in Rust and egui, with no GTK or Qt. It's in the same family as Transmit and FileZilla
and works the way people expect from those, but it has its own name, icon and look.

**Status:** getting ready for a first version (0.1.0, SFTP only). Screenshots: `utilities/rivet-dev ftp-screenshots`, using a demo server made of a throwaway folder (`RIVET_FTP_DEMO_SERVER`, no network). The window works over SFTP, with Rivet's look: a menu bar (or a menu button), Preferences (style, accent, desktop colors, text size, transfer behavior), sortable columns, and Rivet's stepped path bar. It has the connection list, both panes, transfers
both ways (files and whole folders) with a queue, new folder, rename and delete on the
server, and a quick-connect field. The command line works too. FTP and FTPS can be saved but
not opened yet. Open the window with `rivet-ftp`, or `rivet-ftp open NAME` to connect straight
away.

## How it should work

These come from the owner's guidance and set the direction.

- **Two panes.** This computer on the left, the server on the right, and both can browse on
  their own. Rivet's split view already provides this layout. Files are dragged between the
  panes, or sent across with one key.
- **Saved connections (a site manager).** Each connection has a name, the server, protocol,
  user, port, key, and the folders to open on each side. Opening one connects and puts both
  panes in place.
- **Saved connections on the command line too.** The window and `rivet-ftp` read the same
  file, so someone who wants plain SSH can use their saved connections there:
  `rivet-ftp ssh NAME` logs in and starts in that connection's remote folder. With one Include
  line in `~/.ssh/config`, `ssh NAME`, `scp`, `rsync` and `git` reach it by name too. See the
  [command line](../apps/rivet-ftp/README.md).
- **A transfer queue.** Uploads and downloads go into a queue with per-file progress, speed,
  pause, retry, and a choice when a file already exists (overwrite, skip, rename, or overwrite if
  newer).
- **Everything else from Rivet:** the path bar with fuzzy suggestions, the three views, Peek,
  Quick Open and Quick Actions, theme colors, terminal mode and Vim keys.

## Keys

All of these are also in the menus, and in Help → Keyboard Shortcuts.

| Key | |
| --- | --- |
| Tab | Switch between the panes |
| Enter / double-click | Open a folder, or send a file to the other side |
| Backspace, Alt+Up | Up a folder |
| Alt+Home | Home folder |
| Up / Down, Shift to extend | Move through the list |
| Ctrl+A | Select everything |
| F4 | Open a server file in an editor (saves go back to the server) |
| Ctrl+U / Ctrl+D | Upload / download the selection |
| F2 | Rename |
| Delete | Delete on the server (asks first) |
| Ctrl+Shift+N | New folder |
| Ctrl+L | Type a path |
| Ctrl+N / Ctrl+K | New connection / quick connect |
| Ctrl+Shift+D | Disconnect |
| Ctrl+H | Show hidden files |
| Ctrl+M, F9, F8 | Show or hide the menu bar, connections, transfers |
| Ctrl+, | Preferences |
| F5 | Refresh |
| Ctrl+T / Ctrl+W | New tab / close the tab |
| Ctrl+Tab, Ctrl+PageDown / PageUp | Next / previous tab |

## Where things are kept

| What | Where |
| --- | --- |
| Saved connections | `~/.config/rivet-ftp/connections.conf`: plain text you can edit, readable only by you |
| SSH aliases for them | `~/.config/rivet-ftp/ssh_config`: rewritten on every save |
| Passwords | Never in those files. The window will keep them in the desktop keyring (Secret Service). SSH logins use keys or the ssh agent. |

## Plan

1. ✅ Shared crates in this repository: `rivet-xdg`, `rivet-connections`, `rivet-look` (theme,
   colors, icons, fonts) and `rivet-kit` (screenshots for checks).
2. ✅ The `rivet-ftp` command line for saved connections, `ssh` and `sftp`.
3. ✅ The window: the connection list (add, edit, remove, quick connect), two panes, a path bar,
   keyboard control, drag and drop between the panes, and the transfer queue.
4. ✅ SFTP through the system's own `sftp` program. Your keys, agent, `~/.ssh/config` and
   known_hosts all apply. A new server is trusted on first use, and a changed server key is
   refused.
5. Next: Rivet's own file views and path bar in the panes (after the widget refactor in
   [shared crates](shared-crates.md)), sorting by column, and upload progress.
6. FTP and FTPS, with passwords kept in the keyring.
7. Resuming, retrying and pausing transfers; reconnecting after a drop; folder compare and sync.
8. A launcher entry, packaging (Flatpak, AUR, AppImage) and releases, tagged
   `rivet-ftp-vX.Y.Z` so they stay separate from Rivet's.
