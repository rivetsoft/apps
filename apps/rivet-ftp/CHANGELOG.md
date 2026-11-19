# Changelog

## 0.3.0

- Rivet Shell's FTP plugin: your saved connections on the panel and in the launcher (`@ftp`),
  each opening a new window connected to it (turn it on in the shell's Settings, Plugins).
- Ctrl+N makes a new folder and Ctrl+Shift+N opens a new window (File → New Window); a new
  connection is Ctrl+Shift+K. Ctrl+Shift+Tab goes to the previous tab again (Ctrl+Tab used to
  catch it).

## 0.2.0

- Code in the preview takes Rivet's own code colors (a color for keywords, strings, numbers,
  functions and types), leaning toward your desktop's colors and accent, the same as Rivet Edit
  and Rivet Terminal: distinct, familiar colors that still sit in your theme.

## 0.1.0

The first version. SFTP only; FTP and FTPS come later.

- Two panes (this computer and the server) with Rivet's path bar, sortable columns and previews.
- Saved connections in groups, tabs for several at once, and a quick-connect bar.
- Transfers by drag and drop, double-click or Upload / Download, with a queue; files already
  on the other side are compared (dates and sizes) before anything is replaced.
- Edit server files in any app; each save is sent back, with a check for changes made meanwhile.
- Info and permissions (chmod), rename, new folder and delete on the server.
- SSH keys: list, make, copy and add to servers; a key per connection; passwords and
  passphrases in the desktop keyring when asked to.
- A command line for saved connections (`list`, `add`, `ssh`, `sftp`, `url`, `keys`), and an
  ssh_config so plain `ssh`, `scp`, `rsync` and `git` reach them by name.
- Preferences for the look, the menu bar and toolbar, transfers and connections.
- `--install-desktop-entry` adds Rivet FTP to the app launcher.
