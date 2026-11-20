# User guide backlog

What the user docs will be written from. When a description in an app is cut down to one
sentence, the full explanation goes here, under the app and the page, with the place it came from.
Once the docs exist, each entry becomes a page or section, and the app gets a "Learn more" link to
it.

## Rivet Shell

### Plugins: hooks

From `settings_app/plugins.rs`, Hooks section.

A hook is a command that runs when something happens in the shell. The command receives
`RIVET_EVENT` (the event's name) and, for some events, more details as `RIVET_` variables:

| Event | Details |
| --- | --- |
| An idle step was reached | `RIVET_STAGE`: dim, lock, screen-off, suspend |
| Someone came back after idle | `RIVET_STAGE` |
| The wallpaper changed | `RIVET_PATH` |
| The style switched between dark and light | `RIVET_MODE`: dark, light |
| A notification arrived | `RIVET_APP`, `RIVET_SUMMARY`, `RIVET_BODY`, `RIVET_URGENCY` |
| Do Not Disturb turned on or off | `RIVET_ON`: true, false |
| The active workspace changed | `RIVET_KEY`, `RIVET_NAME` |
| What's playing changed | `RIVET_TITLE`, `RIVET_ARTIST`, `RIVET_PLAYER`, `RIVET_PLAYING` |

### Plugins: writing your own

From `settings_app/plugins.rs`, the removed "Write a plugin" section. `apps/rivet-shell/PLUGINS.md`
is the full reference.

A plugin is a folder with a `plugin.conf` (name, exec, mode, interval, events) and a program in
any language. The program prints what its panel item shows (plain text, or JSON with text, an
icon, a menu and launcher entries), and it hears about clicks and the hook events. It can also add
settings and a launcher command (`@name`). Disk Space, Notes and FTP come with the shell as
examples to copy.

### Plugins: getting more

From `settings_app/plugins.rs`, the Get Plugins window ("Other people make third-party plugins. A
plugin can do anything you can, so turn on only ones you trust.").

The list is `plugins/registry.json` in Rivet's repository. A plugin is accepted into it once; its
author then publishes updates themselves, in a release file the listing points to, which names the
current version, its archive and the archive's checksum. The shell refuses a download that doesn't
match that checksum, but nobody at Rivet reads an update before it reaches people, which is why
third-party plugins carry the warning. Installing copies the plugin into
`~/.local/share/rivet-shell/plugins/`; it runs only once it's turned on. Remove deletes that folder
and keeps the plugin's settings. View Source opens the plugin's own page. A plugin that isn't in
the list can be installed from a terminal (`rivet-shell plugin install FILE`), with no checksum
to check it against. `apps/rivet-shell/PLUGINS.md` has how to share one.

### Panels: zones

From `settings_app/panel_editor.rs`.

A panel has three zones. On a top or bottom panel, Left starts at the panel's left end, Center sits
in the middle and Right ends at the right end. On a side panel the zones are Top, Middle and Bottom.
On a side panel, numbers sit under their symbols, the clock wraps onto lines and the window title
runs downward. When items don't fit, the ones nearest the middle are hidden first.

### Displays and window look: how Rivet Shell changes the compositor's settings

From `settings_app/displays.rs` and `settings_app/window_look.rs`.

Rivet Shell keeps its screen and window settings in a file of its own next to the compositor's
config (`rivet-shell.kdl` for niri, `rivet-shell.conf` or `rivet-shell.lua` for Hyprland,
`rivet-shell.conf` for mango). The compositor's config needs one include line at the end so these
settings take effect; Settings adds it on request and first saves the original as
`NAME.rivet-backup`. Niri needs version 25.11 or later. "Leave the Screens to …" removes the
screens from Rivet Shell's file so the compositor's own config sets them again.

### Login screen (greetd)

From `greeter.rs` and `rivet-shell greeter --setup`.

The login screen is the lock screen, run by greetd. greetd runs it as its own user, which can't
read anyone's home folder, so each person's Rivet Shell leaves a copy of their wallpaper, their
colors and clock, their name, their picture (`~/.face`) and their session in
`/var/lib/rivet-greeter/LOGIN`, whenever the wallpaper or its colors change. The login screen
shows whoever changed theirs last; Tab (or the arrow keys) goes to the next account. The folder
works like `/tmp`: anyone can add their own folder, nobody can change another's, and the login
screen ignores a folder its owner didn't make. The session started is the one the person was
using (by its desktop name), else the first Wayland session installed. An account that asks for
more than a password (a one-time code) can't sign in from this screen yet.

### Lock screen

From `settings_app/idle_lock.rs`.

Rivet Shell's lock screen checks the password through PAM, using the "rivet-shell" service if
`/etc/pam.d/rivet-shell` exists and the "login" service otherwise. With another
locker, the command must keep running until the screen unlocks, so the shell knows the screen is
locked. That is why swaylock's `-f` (which makes it return at once) has to be left out.
