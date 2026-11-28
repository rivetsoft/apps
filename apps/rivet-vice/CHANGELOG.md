# Changelog

## Unreleased

- **Screen readers and the keyboard**: Orca and other screen readers can read the window's
  buttons, menus, check boxes and text fields, and Tab shows a ring around the control that has
  the keyboard. Shift+F10 opens the right-click menu of the control that has the keyboard.
  With higher contrast or less motion asked for in the desktop's settings, edges and secondary
  text get stronger and animations stop.

## 0.1.1

- An idle window follows desktop color changes right away.

## 0.1.0

- **Rivet Vice, a new app**: open an archive and see what is in it, folder by folder, with
  each thing's size and when it was last changed. Zip, tar of every kind, 7z and more, using
  the archive tools the computer already has.
- **Extract All** takes everything out into a folder you choose. Select some rows and
  **Extract Selected** takes out only those. Nothing already there is written over.
- **Make an archive**: Compress a Folder, or drop files on the window, then choose its name
  and kind (zip, tar.gz, tar.xz, tar.zst or 7z).
- **With no window**: `rivet-vice --extract-here`, `--extract-to FOLDER` and `--compress` do
  the same from a terminal or another program.
