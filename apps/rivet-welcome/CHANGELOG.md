# Changelog

## Unreleased

- **Screen readers and the keyboard**: Orca and other screen readers can read the window's
  buttons, menus, check boxes and text fields, and Tab shows a ring around the control that has
  the keyboard. Shift+F10 opens the right-click menu of the control that has the keyboard. With higher contrast or less motion asked for in the desktop's settings, edges
  and secondary text get stronger and animations stop.
- **Edge versions.** Choose Stable, Alpha or Edge under Versions in Settings. Edge installs a
  build of each app's newest code, made every night, and shows which commit and day it's from.
  Going back to Stable or Alpha replaces the edge builds with the releases, even though their
  version numbers are lower. "Offer alpha versions of stable apps" from 0.4.0 carries over as
  Alpha.
- **No false "busy" message.** Installing, updating or removing an app right after the last job
  on it finished no longer sometimes says another Rivet Welcome is working on it.

## 0.4.0

- **Stable versions first.** An app with a stable release is updated to its next stable release,
  and its alpha versions are left out. Turn on "Offer alpha versions of stable apps" in Settings
  to get them too. An app with no stable release yet still gets its alpha versions.
- **Every release is checked.** A download with no published SHA-256 sum is refused instead of
  installed unchecked, and only the archive with the app's exact name is used.
- **Downloads survive a bad connection.** A download that stops picks up where it left off,
  with up to four tries, and one that stalls is stopped instead of hanging.
- **Nothing is half-installed.** Before downloading, Rivet Welcome checks there's room on the
  disk and that it can write to the folder the programs go in. If an app's installer fails, the
  version you had before is put back.
- **Clearer messages when something goes wrong**: a full disk, a release with a file missing,
  or GitHub's limit on checks, with the time you can check again.
- **Checks are lighter.** Every release is read, not only the newest hundred, and an unchanged
  list costs nothing against GitHub's limit. Set `GITHUB_TOKEN` to raise the limit.
- **Flatpak and system installs are labeled** with where they came from, and left for that
  package manager to update.
- Two Rivet Welcomes (the window and the tray, or two terminals) no longer work on the same app
  at once.
- After Rivet Welcome updates itself, the tray restarts as the new version, and launchers never
  point at the old, deleted program. Removing Rivet Welcome also removes the tray's sign-in
  entry.

## 0.3.1

- An idle window follows desktop color changes right away.

## 0.3.0

- **A new icon**: the box the apps come in, with the apps on a shelf behind it, drawn in the website's colors with a plum outline and a hard shadow.
- **Rivet Snip is listed**, as coming soon until its first release. Then it installs and updates
  like the other apps.
- **Rivet Color is listed**, and installs and updates like the other apps.
- **Every Rivet app is listed**: Rivet Shell, Vice, Calendar and Fetch install and update here
  like the others. Rivet Fetch is a program for the terminal, so it has no Open button: run
  `rivetfetch` in a terminal.

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
