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
  Esc puts it back. A note's title is its `title` property, else its file name (the same in
  the sidebar and the editor); headings in the text are just headings. Changing it updates where it came
  from and renames the file to match.
- **Breadcrumbs** above the title show the vault, folders and file. View → Breadcrumbs (or Preferences → Appearance → Window)
  hides the bar.

## Links between notes

Link a note with `[[Note name]]` (also `[[Note|shown text]]` and `[[Note#Heading]]`); Markdown
links to `.md` files count too. A link finds its note by path, file name or title.

- **Typing `[[`** lists matching notes; Up/Down choose, Enter or Tab puts the link in (with
  its `]]`), Esc hides the list.
- **Following a link**: Ctrl+click it in either mode, or just click it in live mode (where it
  shows rendered). A link to a note that isn't there yet makes it, beside the current note.
- **Backlinks**: Peek lists the notes linking to the open one, with the line each link is on,
  and the open note's own links (faint when there's no note yet; clicking makes it). Each shows
  only when there are some.
- **Renaming** a note (its title, or Rename) changes the `[[links]]` to it in every note, keeping
  their `|shown text` and `#heading`.

## Quick Open and Quick Actions

Ctrl+P opens Quick Open: type part of a note's title or folder (letters in order are enough,
like `rvft` for Rivet FTP) and press Enter. With nothing typed it lists the most recently edited
notes. Shift+Ctrl+P opens Quick Actions: every command and option by name, with on / off options
ticked. Both are the same panel as Rivet Files' (shared in `rivet_look::quick`).

The status bar shows the vault (muted, with a vault icon) and the open note's words, plus any
problem (in red); View → Status Bar hides it.

## Modules

Optional features, turned on in Preferences → Modules.

- **Daily notes**: a note for each day, named by its date (`2026-11-16.md`) in a folder you
  pick (Daily by default; it's made when first needed). Ctrl+D, File → Daily Note, or
  the + button (which then offers New Note or Daily Note) opens today's note, making it
  with a `daily` tag when it isn't there yet.

## Live mode

Two ways to edit, switched with the button left of the reading view's eye at the top right (a
bolt in live mode, brackets in source mode), or Preferences → Editor, View → Live Mode or Quick
Actions. The reading view (the eye, Ctrl+Space) works in either:

- **Source**: the Markdown as written, colored.
- **Live**: you still write Markdown, but every line except the one you're on shows rendered (the line you're on stays
  plain Markdown until you leave it; a heading takes its size as soon as its `#` marks are typed):
  the marks (`**`, `#`, backticks, link addresses…) fold away, headings show at their size, lists get bullets, tasks get check boxes
  you can click to tick or untick (the cursor stays where it was), quotes get a bar, `---` becomes a rule, and code blocks are colored for their
  language with their fences tucked away until you move into the block. With the keyboard
  elsewhere, the whole note shows rendered.

## Super todos (module)

Turned on in Preferences → Modules, where the todo tags are set (`#task` by default). A plain
`- [ ]` stays an ordinary todo; one carrying a todo tag is a **super todo**, and Shift+Ctrl+T
makes the line at the cursor one (adding the tag, and the `- [ ]` if needed).

- Its details are plain text in the line: `- [ ] Send the invoice #task #work due:2026-12-01 priority:high`.
  Progress is the box: `[ ]` to do, `[/]` in progress, `[x]` done.
- In live mode the details don't show as text: the todo tag folds away, the due date becomes a
  pill (Today, Tomorrow, Friday, 3 days late… colored for how soon it is) and the priority a
  colored flag; an in-progress task's box is half filled.
- In live mode, clicking a super todo's words opens its panel (the line stays rendered; *Edit
  text* in the panel puts the cursor at the end of the line, and *Plain todo* turns it back
  into an ordinary todo, dropping its todo tag, due date and priority).
- On a super todo's line (cursor or pointer) a ⋯ button also opens the panel for its progress (To do, In
  progress, Done), due date (Today, Tomorrow, Next week or a calendar), priority (flags) and tags.
- **Todos page** (the check box button at the top right, File → Todos, or Quick Actions): every
  super todo in the vault. Filter by text, Open / In progress / Done / All, when it's due (overdue, today, next
  7 days, no date) and tag; sort by due date, priority or note. Tick a box to finish a task (its
  note changes on disk); click a row to open its note. Overdue dates are red.

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
| Ctrl+P | Quick Open: go to any note by typing part of its title or folder |
| Shift+Ctrl+P | Quick Actions: every command and option by name |
| Ctrl+D | Daily note (with the Daily notes module on) |
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
