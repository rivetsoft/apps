# Changelog

## Unreleased

- **A new icon**: the box the apps come in, with the apps on a shelf behind it, drawn in the website's colors with a plum outline and a hard shadow.
- **Rivet Snip is listed**, as coming soon until its first release. Then it installs and updates
  like the other apps.
- **Every Rivet app is listed**: Rivet Shell, Snip, Vice, Calendar, Color and Fetch install and
  update here like the others. Rivet Fetch is a program for the terminal, so it has no Open
  button: run \`rivetfetch\`.

## 0.2.0

- A new icon in Rivet's pink, to match the rest of the apps and the website.
- Clicking the tray icon again brings the open window forward instead of opening another one.
- Opened from an AppImage, Rivet Welcome is listed as running instead of offering to install
  itself. Install on This Computer is still in its menu if you want it in your app launcher.

## 0.1.0

The first version.

- Every Rivet app as a card: what's installed, what's released, and Install, Update, Open and
  Remove. Apps released after this version are found in the releases and offered too.
- Installs from the GitHub release archives for this computer, checked against their SHA-256
  sums, with each archive's own installer: programs in ~/.local/bin and the apps in the
  launcher, no password. Apps installed another way (a package, AUR) are shown and left alone.
- Looks for releases as soon as it opens, and from a tray icon that starts when you sign in:
  it checks then and every few hours, shows a dot and sends a notification when there are
  updates, and can install them all from its menu.
- `rivet-welcome check`, `list`, `install APP`, `update` and `remove APP` from a terminal.
