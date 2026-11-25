# Changelog

## Unreleased

- **A new icon**: two panes with files going each way between them, in place of the folder, drawn in the website's colors with a plum outline and a hard shadow.
- **Return in a question presses the highlighted button**: Delete, Replace, the changed-on-the-
  server question and Info answer to Return. Tab moves between the buttons, and Return on
  Cancel cancels.
- Tab moves through the New Key window, where it used to switch panes behind it.

## 0.4.0

- A new icon in Rivet's pink, to match the rest of the apps and the website.
- **Smoother with big folders and long transfer lists**: scrolling and redrawing no longer
  slow down as a folder or the transfer list grows (thousands of files draw as quickly as a few).
- **This computer's folders open without a pause**: they are read in the background, about three
  times faster, and the window stays responsive while a slow drive answers.
- **Opens sooner**: the first frame is ready about 100 ms earlier.
- Going up a folder scrolls to the folder you came from, even in a long list.
- **A cleaner transfer list**: it sits in a card under the panes, with a line that fills as the
  batch goes. Transfers still to go are listed first. Each one shows its file's icon and a mark
  for done or failed. Retry Failed sends every failed one again, and right-clicking a finished
  transfer removes it from the list.

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
