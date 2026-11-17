# Changelog

## 0.1.0 (unreleased)

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
