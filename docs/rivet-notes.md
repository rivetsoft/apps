# Rivet Notes

> **Early alpha.** Rivet Notes is not ready for daily use: things are missing, change without
> notice, or break. Try it on a copy of your notes you don't mind losing, and keep backups.

Markdown notes kept in plain folders, built on the same shared parts as Rivet Files and Rivet FTP
(theme and desktop colors, icons, fonts, in-app windows, Markdown coloring). It's an original app
in the same family as other folder-based note apps.

**Status:** first version. Open the window with `rivet-notes` (or `rivet-notes FOLDER`).

## Vaults

A vault is any folder. Its `.md` files are the notes, and its sub-folders are their folders.
Nothing is added to the folder: no database, no hidden settings. Folders and files whose names
start with a dot (`.git`, `.obsidian`) are left out. Switch vaults from the button at the left of
the toolbar, or with File → Switch Vault. Preferences lists your vaults (Forget takes one off
the list and leaves the folder alone).

## Notes

- **Frontmatter**: the `---` block at the top of a note is shown as **Properties**, each
  with a type: Text, Number, Checkbox, Date (with a calendar), List, Tags (chips) or Link (with an
  open button). The type is worked out from the value; click a property's name (or right-click it)
  to change its type, rename or remove it. *+ Add property* asks for a name and a type. The
  block folds from its header. `title` (the heading) and `created` (in Peek's dates) stay in
  the file but aren't listed. The file stays plain YAML. Nested values are kept exactly as
  written and shown read-only.
  Saving writes back only what changed in the block; the rest of the file is untouched.
- **Tags** come from the frontmatter (`tags: [a, b]` or a `- item` list) and from `#tags` in
  the text (not inside code).
- **Browse** by folder (a tree that folds) or by tag (each tag with its notes, plus Untagged).
  The sidebar holds panels (Folders, Tags). Drag a panel's title onto another panel to make
  them tabs, or to its top or bottom edge to stack them, each in its own pane with a divider to
  drag. The ⋯ menu, View and Preferences → Sidebar have the same choices; Ctrl+T switches tabs
  (or folds the last pane when they're stacked).
  Search finds titles, paths, the first line and tags; `#name` searches tags only.
- Notes save a second after you stop typing, and with Ctrl+S. A note changed by another program
  is reloaded, or you're asked if you had unsaved changes.
- Drag a note (or a folder) onto a folder to move it there, or onto the empty space below the
  folders to move it to the top of the vault.
- New notes get `title`, `created` and `tags`. Delete moves notes to the desktop's Trash.

- **The title** at the top of a note can be edited in place: Enter or clicking away keeps it,
  Esc puts it back. A note's title is its `title` property, else its first `# heading`, else
  its file name (the same in the sidebar and the editor). Changing it updates where it came
  from and renames the file to match.
- **Breadcrumbs** above the title show the vault, folders and file. View → Breadcrumbs (or Preferences → Appearance → Window)
  hides the bar.

## Modules

Optional features, turned on in Preferences → Modules.

- **Daily notes**: a note for each day, named by its date (`2026-11-16.md`) in a folder you
  pick (Daily by default; it's made when first needed). Ctrl+D, File → Today's Daily Note, or
  the + button (which then offers New Note or Today's Daily Note) opens today's note, making it
  with a `daily` tag when it isn't there yet.

## Formatting

Keys that turn the selection (or the word under the cursor) into Markdown, and back again when
pressed a second time: Ctrl+B bold, Ctrl+I italic, Shift+Ctrl+X strikethrough, Ctrl+E inline
code, Ctrl+K link (a web address becomes the link's target), Ctrl+1/2/3 headings and Ctrl+0
plain text, Shift+Ctrl+8/7/9 bulleted, numbered and task lists, Ctrl+Enter ticks a task,
Shift+Ctrl+Q quote, Shift+Ctrl+E code block. They're also in the Format menu, the text's
right-click menu and Preferences → Keyboard. Selecting text brings up a small bar above it with
the common ones (bold, italic, strikethrough, code, link, headings, lists, quote).

## Reading view

Ctrl+Space (or View → Reading View, or the eye on the toolbar) shows the open note as a
rendered page: headings, bold and italic, lists and tasks, quotes, code blocks with colors and a
Copy button, tables, rules, links, `#tags` and pictures stored beside the note. Links to other
notes (`[[Name]]` or a relative `.md` path) open them in place, and Backspace goes back. Arrow
keys, Page Up/Down, Space, Home and End scroll. Esc or Ctrl+Space closes it.

## Peek

The right-sidebar button at the far right of the toolbar (or F11, or View → Peek) opens Peek, a panel on the
right about the open note: how it's shown (full width or a centered column, and whether the
properties sit above the text), the file (name, folder, size, words, characters, lines, reading
time), its dates (edited, created, saved) and its properties. A width chosen in Peek is that note's
own (it follows the note when it's renamed or moved; its Reset button drops it). Preferences →
Appearance → Default note width sets it for every other note.

## Fonts

Preferences → Editor sets three fonts, the editor's size and its line spacing: **Interface** (menus, sidebar;
like the desktop by default), **Editor** (the note's text; the interface font by default) and
**Monospace** (code blocks, `inline code` and other embedded text; the system's by default). A
sample shows the editor's fonts as you choose.

## Keys

| Key | |
| --- | --- |
| Ctrl+N / Shift+Ctrl+N | New note / new folder (in the current folder) |
| Ctrl+S | Save now |
| Ctrl+Space | Reading view (Esc closes it) |
| F11 | Peek |
| Ctrl+F | Search |
| Ctrl+T | Switch folders / tags, or fold the tags pane when it's below |
| Ctrl+P | Show or hide properties |
| F9, Ctrl+M | Show or hide the sidebar, the menu bar |
| Ctrl+, | Preferences |

## Command line

```sh
rivet-notes ~/Documents/Notes     # open that folder as a vault
rivet-notes tags ~/Documents/Notes  # the tags in a vault, most used first
```

## Next

Links between notes (`[[Note]]`) and backlinks,
templates, and the full-text search in the shared file views.
