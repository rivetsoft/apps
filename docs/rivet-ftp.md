# Rivet FTP

A dual-pane file-transfer app for Linux (SFTP, FTP and FTPS), built on Rivet's core and
written in Rust and egui, with no GTK or Qt. It's in the same family as Transmit and FileZilla
and works the way people expect from those, but it has its own name, icon and look.

**Status:** saved connections and the command line work. The window doesn't exist yet.

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
  Quick Open and Quick Actions, theme colours, terminal mode and Vim keys.

## Where things are kept

| What | Where |
| --- | --- |
| Saved connections | `~/.config/rivet-ftp/connections.conf`: plain text you can edit, readable only by you |
| SSH aliases for them | `~/.config/rivet-ftp/ssh_config`: rewritten on every save |
| Passwords | Never in those files. The window will keep them in the desktop keyring (Secret Service). SSH logins use keys or the ssh agent. |

## Plan

1. ✅ Shared crates in this repository: `rivet-xdg` and `rivet-connections`.
2. ✅ The `rivet-ftp` command line for saved connections, `ssh` and `sftp`.
3. Move the look into a shared crate (theme, icons, fonts, the path bar), so the window looks
   like a Rivet app from its first build. See [shared crates](shared-crates.md).
4. A window with the connection list and two local panes, reusing Rivet's split view.
5. Remote browsing over SFTP first (the most common protocol), then FTP and FTPS.
6. The transfer queue.
7. Passwords in the keyring, host-key checks (known_hosts), and reconnecting after a drop.
8. Packaging (Flatpak, AUR, AppImage) and releases, tagged `rivet-ftp-vX.Y.Z` so they stay
   separate from Rivet's.
