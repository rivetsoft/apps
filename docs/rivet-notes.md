# Rivet Notes

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

- **Frontmatter**: the `---` block at the top of a note is shown as **Properties**: text
  fields, lists, and tags as chips. Nested values are kept exactly as written and shown read-only.
  Saving writes back only what changed in the block; the rest of the file is untouched.
- **Tags** come from the frontmatter (`tags: [a, b]` or a `- item` list) and from `#tags` in
  the text (not inside code).
- **Browse** by folder (a tree that folds) or by tag (each tag with its notes, plus Untagged).
  Search finds titles, paths, the first line and tags; `#name` searches tags only.
- Notes save a second after you stop typing, and with Ctrl+S. A note changed by another program
  is reloaded, or you're asked if you had unsaved changes.
- New notes get `title`, `created` and `tags`. Delete moves notes to the desktop's Trash.

## Keys

| Key | |
| --- | --- |
| Ctrl+N / Shift+Ctrl+N | New note / new folder (in the current folder) |
| Ctrl+S | Save now |
| Ctrl+F | Search |
| Ctrl+T | Folders / tags |
| Ctrl+P | Show or hide properties |
| F9, Ctrl+M | Show or hide the sidebar, the menu bar |
| Ctrl+, | Preferences |

## Command line

```sh
rivet-notes ~/Documents/Notes     # open that folder as a vault
rivet-notes tags ~/Documents/Notes  # the tags in a vault, most used first
```

## Next

Rendered preview (Markdown to formatted text), links between notes (`[[Note]]`) and backlinks,
templates, and the full-text search in the shared file views.
