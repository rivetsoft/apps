# Rivet Shell plugins and hooks

A plugin is a folder with a `plugin.conf` and a program. The shell runs the program, shows what
it says as an item on a panel (with a menu when it gives one), lists its entries in the launcher,
shows its settings in Settings, and tells it what happens.
Plugins are separate programs in any language: one that crashes or hangs can't take the shell
with it.

**Hooks** are the same events without a plugin: a command of your own, run when something
happens (Settings, Plugins, Hooks).

## Where plugins live

| Folder | What |
| --- | --- |
| `PREFIX/share/rivet-shell/plugins/NAME/` (beside the program, and each of `$XDG_DATA_DIRS`) | Official: they come with the shell |
| `~/.local/share/rivet-shell/plugins/NAME/` | Third-party: yours, or ones you installed |
| `$RIVET_SHELL_PLUGINS/NAME/` | While you write one |

The folder's name is the plugin's id. A plugin does nothing until it's turned on in Settings,
Plugins; then its item (`plugin:NAME`) can go on any panel (Settings, Panels, Add Item), in a
group with its own colours like any other item. A plugin is a program that runs as you: turn on
only ones you trust.

## plugin.conf

```ini
name=Disk Space
description=Free space on your disk
author=You
version=1
exec=./disk.sh /        # from the plugin's folder, with its arguments; or a command on PATH
mode=stream             # poll (the default) or stream
interval=30             # poll: seconds between runs
events=unlock,wallpaper # the shell's events it's told of, besides its own clicks
keyword=disk            # @disk in the launcher shows only its entries
```

## What a plugin says

A JSON object; every key is optional.

| Key | What |
| --- | --- |
| `text` | The item's text |
| `icon` | An icon's name in the icon theme (`drive-harddisk`), or a picture's path |
| `tooltip` | Shown on hover |
| `state` | `normal`, `accent`, `warning` or `danger`: the text's colour |
| `hidden` | `true` takes the item off the panel for now |
| `badge` | `true` puts a dot on the item: something wants attention |
| `menu` | Rows shown when the item is clicked: `[{"id":"…","label":"…","sub":"…"}]` (more below) |
| `launcher` | Entries in the launcher: `[{"id":"…","label":"…","icon":"…","query":true}]` (more below) |
| `notify` | A notification: `{"summary":"…","body":"…"}` |
| `run` | A `rivet-shell msg` command for the shell: `"launcher"`, `"volume up"`, `"lock"`… |

### Menu rows

A row with an `id` can be clicked (the plugin is told: `menu`, with the `id`); one without is
only shown. Besides `id`, `label` and `sub`:

| Key | What |
| --- | --- |
| `heading` | `true`: a small title over the rows after it, or a line when it has no `label` |
| `icon` | An icon before the label |
| `mark` | A task's box before the label: `" "` to do, `"/"` started, `"x"` done |
| `state` | The colour of `sub`: `normal`, `accent`, `warning`, `danger` |
| `quiet` | `true`: in weaker text |

### Launcher entries

An entry shows in the launcher once something is typed that matches its `label`. With
`"query": true` it also shows, last, for anything typed, as `label “the words”`. Choosing one
tells the plugin: `launch`, with the entry's `id` and, for the second kind, the words as `query`.
An entry has a `sub` (shown after its name in a list) and an `icon` too.

**Commands.** With `keyword=ftp` in its `plugin.conf`, a plugin has a word in the launcher:
`@ftp` shows only its entries, all of them, and words after it search them by `label` and `sub`
(`@ftp work`). `@` alone lists the words there are; choosing one types it. An entry with
`"scoped": true` shows only under the plugin's word, never in an ordinary search: for long
lists (every note, every bookmark) that shouldn't crowd the apps.

## Settings of its own

A plugin asks for settings in its `plugin.conf`; they show under it in Settings, Plugins, while
it's on:

```ini
setting.vault=folder|Vault|The folder of notes to show
setting.label=text|Label|What to call it
setting.all_tasks=toggle|Show every task
```

The kind (`folder`, `text` or `toggle`), then the label, then a hint. The program finds each in
its environment as `RIVET_SET_` and the key in capitals (`RIVET_SET_VAULT=/home/me/Notes`; a
toggle that's on is `true`; one never set is missing), and is started again when one changes.
They're kept in the shell's settings file as `plugin.ID.KEY=value`.

## The two modes

**poll**: the program is run every `interval` seconds, and again for each event. Its whole
output is its answer (keys left out are empty). Plain text works too: the first line is the
text, the second the tooltip, the third the state. What happened is in its environment:
`RIVET_EVENT=click`, `RIVET_BUTTON=left`. A whole plugin:

```sh
#!/bin/sh
# plugin.conf: name=Load / exec=./load.sh / interval=10
cut -d' ' -f1 /proc/loadavg
echo "Load average"
```

**stream**: the program is started once and kept running (started again if it stops, up to five
times a minute). Each line it prints is an update (keys left out keep what they were), and what
happens is written to its input, a JSON object per line: `{"event":"click","button":"left"}`.
When its input closes, it should exit.

## What a plugin is told

Its own item's events, always:

| Event | With |
| --- | --- |
| `start` | (the first run) |
| `poll` | (poll mode: a run because `interval` passed) |
| `click` | `button`: `left`, `middle`, `right` |
| `scroll` | `direction`: `up`, `down` |
| `menu` | `id`: the row clicked |
| `launch` | `id`: its launcher entry chosen; `query`: the words typed, or empty |

The shell's events, when listed in `events=` (and for hooks):

| Event | When | With |
| --- | --- | --- |
| `start` | The shell started | |
| `lock`, `unlock` | The screen was locked, unlocked | |
| `idle` | A step of idle was reached | `stage`: `dim`, `lock`, `screen-off`, `suspend` |
| `active` | Someone is back | `stage` |
| `wallpaper` | The wallpaper changed | `path` |
| `colors` | The desktop's colors were made again (matugen) | |
| `style` | Dark or light was chosen | `mode`: `dark`, `light` |
| `notification` | A notification arrived | `app`, `summary`, `body`, `urgency` |
| `dnd` | Do Not Disturb turned on or off | `on` |
| `workspace` | Another workspace is in front | `key`, `name` |
| `media` | What's playing changed | `title`, `artist`, `player`, `playing` |

In the environment (poll plugins, hooks) each comes as `RIVET_` and its name in capitals:
`RIVET_EVENT=idle RIVET_STAGE=screen-off`. Every plugin also gets `RIVET_PLUGIN` (its id) and
`RIVET_PLUGIN_DIR` (its folder).

## Hooks

In Settings, Plugins, Hooks, or the settings file (`~/.config/rivet-shell/settings`):

```ini
hook.lock=playerctl pause
hook.wallpaper=notify-send "New wallpaper" "$RIVET_PATH"
hook.idle=[ "$RIVET_STAGE" = screen-off ] && openrgb --profile off
```

Each is a shell line, run when its event happens.

## Examples

`data/plugins/disk` (Disk Space) is a stream plugin in bash, about fifty lines: free space on
the panel, turning orange and red as the disk fills; a menu of every disk, which opens the one
clicked; one notification when it's nearly full; and a refresh when the screen is unlocked.
Copy its folder into `~/.local/share/rivet-shell/plugins/` under another name to start your own.

`data/plugins/notes` (Notes) is only a `plugin.conf`: its program is another app, Rivet Notes
(`exec=rivet-notes shell-plugin`; a program beside the shell's own is found before the ones on
`PATH`). It uses the rest of what's here: a setting for the vault, a menu with headings and
tasks' boxes, a badge and a notification when something is due today, and a New Note entry in
the launcher (and under `@notes`, every note by name). Its source is
`apps/rivet-notes/src/shell.rs`.

`data/plugins/ftp` (FTP) is Rivet FTP's (`exec=rivet-ftp shell-plugin`,
`apps/rivet-ftp/src/shell.rs`), about a hundred lines: the saved connections in the item's menu
under their groups and in the launcher (`@ftp`), each opening a new Rivet FTP window connected
to it.
