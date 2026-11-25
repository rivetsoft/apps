# Rivet Color

Pick a color from anywhere on the screen, copy it in the format you need, see the colors that
go with it, and keep colors in palettes.

- **Pick from Screen** (or P) takes the color under the next click and copies it.
- The color in **hex, RGB, HSL and OKLCH**: click one to copy it. **Copy as** sets the format
  copies use everywhere (the window, the panel, `rivet-color pick`).
- **Shades**: five tints and five shades, even steps in how light they look.
- **Harmonies**: complementary, analogous, triadic, split complementary and square, turned in
  OKLCH so the colors look as light as the one they start from.
- **Contrast** on white and black, with the WCAG grade.
- **History** of the colors picked and copied, and **palettes**. Right-click any swatch to
  copy it, add it to a palette or remove it.

Unreleased; built from source.

## Picking

Where the screen can be captured (niri, Hyprland, Sway and other compositors with layer-shell,
with `grim` installed), Rivet Color shows its own picker: the screen held still, with a loupe
beside the pointer showing the pixels around it enlarged and the color's code under them.
Click or Enter picks; Escape or a right-click stops; the wheel zooms; arrow keys move one
pixel, ten with Shift.

Elsewhere it asks what can:

| Where | What picks |
| --- | --- |
| niri without grim | niri's own picker (`niri msg pick-color`) |
| anywhere hyprpicker is installed | `hyprpicker` |
| compositors that let programs read the screen, with `slurp` | `slurp` chooses the point, `grim` reads it |
| GNOME, KDE | the desktop's picker (xdg-desktop-portal) |

Copying from the panel or `rivet-color pick` uses `wl-copy` (wl-clipboard).

## In Rivet Shell

Turn on **Color** in Settings, Plugins, then add its item to a panel. The item shows the color
picked last. Click it for Pick a Color and the recent colors (click one to copy it);
middle-click picks straight away; right-click opens Rivet Color. In the launcher, `@color`
lists the history and palettes, and `@color #d53f8c` (or `rgb(…)`, `hsl(…)`, `oklch(…)`)
copies that color in the format chosen.

For a key binding, bind `rivet-color pick`: it picks, copies, keeps the color in the history
and prints it.

## Files

| File | What |
| --- | --- |
| `~/.local/share/rivet-color/history` | The history, a `#rrggbb` a line, the latest first |
| `~/.local/share/rivet-color/palettes/*.gpl` | Palettes, in GIMP's format |
| `~/.config/rivet-color/settings` | `format=hex`, `rgb`, `hsl` or `oklch` |

## Building

```sh
cargo build -p rivet-color
target/debug/rivet-color --install-desktop-entry
target/debug/rivet-color --screenshot out.png   # RIVET_COLOR_SCENE=empty|desktop|copied|loupe, RIVET_COLOR_THEME=light|dark
```
