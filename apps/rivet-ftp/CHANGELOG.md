# Changelog

## 0.2.0

- Release 0.7.0
- rivet-dev release-all: release the apps one at a time
- Ready to publish: README version, app status lists Rivet Terminal
- Rivet Terminal: wider tabs
- Code colours lean toward the theme the same way as the terminal's
- Rivet Terminal: the theme shapes the terminal colours more
- Rivet Terminal: its colours follow the theme visibly
- Rivet Terminal: theme colours the same as Rivet Edit's defaults
- Rivet Terminal, and rivet-term: a terminal for Rivet apps
- Rivet Edit: a clear gap between the tabs and the code card
- Rivet Edit: a right-click menu in the sidebar
- Rivet Edit: a right-click menu in the editor
- Rivet Edit: Go to Definition, Find References, hover, Back / Forward
- Code colours: the default palette leads, the theme only tints it
- Code colours: Rivet's default palette mixed with the theme
- Rivet Edit: Normal-mode commands in the middle of the status bar too
- Rivet Edit: Vim modes as coloured tags, the : line in the middle
- Rivet Edit: Vim keys
- Rivet Edit: file and folder icons by type, coloured to the theme
- Rivet Edit: suggestions as you type, and errors from language servers
- Rivet Edit: change a file's language and its tabs or spaces
- Rivet Edit: a new start page, and Preferences like the other Rivet apps
- Rivet Edit: tabs on the window, the code card below them
- Rivet Edit: make Search in Folder fast
- Rivet Edit: multiple cursors, VS Code's editing keys, find/replace and folder search
- Rivet Edit: preview tabs; tabs keep the app's look under a color theme; a leaner toolbar
- Rivet Edit: built-in editor color themes
- Rivet Edit: tabs without a scroll bar over them; < > when they overflow
- Rivet Edit: Go to File without the wait; Tab fills in when opening a folder
- Rivet Edit: a fourth app, a lightweight code editor

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
