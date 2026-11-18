# Rivet Shell plugins and hooks

A plugin is a folder with a `plugin.conf` and a program. The shell runs the program, shows what
it says as an item on a panel (with a menu when it gives one), and tells it what happens.
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
| `menu` | Rows shown when the item is clicked: `[{"id":"…","label":"…","sub":"…"}]` |
| `notify` | A notification: `{"summary":"…","body":"…"}` |
| `run` | A `rivet-shell msg` command for the shell: `"launcher"`, `"volume up"`, `"lock"`… |

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

## An example

`data/plugins/disk` (Disk Space) is a stream plugin in bash, about fifty lines: free space on
the panel, turning orange and red as the disk fills; a menu of every disk, which opens the one
clicked; one notification when it's nearly full; and a refresh when the screen is unlocked.
Copy its folder into `~/.local/share/rivet-shell/plugins/` under another name to start your own.
