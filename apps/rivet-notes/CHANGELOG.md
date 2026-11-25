# Changelog

## Unreleased

- **A new icon**: a yellow notepad on spiral rings, in place of the folder, drawn in the website's colors with a plum outline and a hard shadow.
- **Return in a question presses the highlighted button**: Move to Trash and Keep Mine answer
  to Return. Tab moves between the buttons, and Return on Cancel cancels.
- Rivet Notes no longer closes with a crash report when a note has an ellipsis (…), an
  accented letter, an emoji or any other letter outside plain English.

## 0.4.0

- **Paste a picture into a note**: a picture copied from a screenshot tool or a browser is saved
  in the attachments folder and shown where the cursor is. Picture files copied in a file manager
  are added the same way.
- **Crash reports**: if Rivet Notes closes unexpectedly, a report is saved on your computer and
  shown the next time it starts, with personal details left out. Nothing is sent unless you
  choose Report on GitHub, which opens an issue for you to check and send.
- A new icon in Rivet's pink, to match the rest of the apps and the website.
- **Open Folder as Vault uses your desktop's folder picker**, so you choose a folder that
  exists instead of typing its path. Typing a path is still there for desktops without a picker.

## 0.3.0

- Rivet Shell's Notes plugin: open tasks and the latest notes on the panel, New Note in the
  launcher (and `@notes` to find a note by name), and a notice when something is due today (turn it on in the shell's Settings,
  Plugins).
- `rivet-notes FOLDER --note PATH` opens the window on a note, and `rivet-notes new [--vault
  FOLDER] [TITLE]` makes a note and opens it.

## 0.2.0

- Code blocks take Rivet's own code colors (a color for keywords, strings, numbers, functions
  and types), leaning toward your desktop's colors and accent, the same as Rivet Edit and Rivet
  Terminal: distinct, familiar colors that still sit in your theme.

## 0.1.0

The first version.

- Vaults: any folder of Markdown notes; switch between them from the menu.
- Live and Source editing modes, and a reading view (Ctrl+Space).
- Typed properties from the frontmatter, with a calendar for dates.
- Links between notes (`[[Note]]`), backlinks in Peek, links kept up to date on rename.
- Folders and tags in the sidebar (tabs or stacked), with faint tree guides; drag notes between folders.
- Slash menu, line dragging, a formatting bar over selections, Markdown shortcuts (Ctrl+B and more).
- Quick Open (Ctrl+P) and Quick Actions (Shift+Ctrl+P).
- Attachments: files dropped on a note are copied into the vault and linked; pictures show in place.
- Modules: Daily notes (Ctrl+D) and Super todos, with a Todos page as a list or a board.
- Peek (F11): display, links, file details and dates; the Todos page has its own.
- Follows the desktop's colors, fonts and icon theme; Rivet's own icon kit by default.
- `--install-desktop-entry` adds Rivet Notes to the app launcher.
