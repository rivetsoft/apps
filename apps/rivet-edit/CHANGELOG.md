# Changelog

## Unreleased

- **A new icon**: a blue card of code with a cursor, drawn in the website's colors with a plum outline and a hard shadow.
- **Return in a question presses the highlighted button**: Save, Replace, Discard and Move to
  Trash answer to Return. Tab moves between the buttons, and Return on Cancel cancels.

## 0.3.0

- A Rivet color theme, in Rivet's own pink, blue and yellow.
- Rivet Shell's Edit plugin: your five most recent folders on the panel and in the launcher
  (`@edit`), each opening in a new window (turn it on in the shell's Settings, Plugins).
- A new icon in Rivet's pink, to match the rest of the apps and the website.

## 0.2.0

- Drag a tab to move it: it lifts and follows the pointer with a glow, the
  tabs it passes slide aside, and it glides into place when let go. Dragging near either end of
  a full strip scrolls it.
- Several windows, each its own workspace: New Window (Shift+Ctrl+N), Open Folder in New Window,
  and Open in New Window on a folder in the sidebar; also from the app launcher.
- Large files scroll and repaint smoothly: the laid-out text is kept between frames instead of
  being rebuilt and looked up again each time, and only the line numbers on screen are drawn.
- Typing in a large file no longer waits for its colors: they're worked out on another thread,
  and until then the old ones move along with the edit. A large file opens at once, colored a
  moment later.
- Undo and redo of its own (Ctrl+Z, Shift+Ctrl+Z or Ctrl+Y, Vim's u and Ctrl+R, and in the Edit
  menu): typing undoes a word at a time, every command (moving lines, commenting, several
  cursors, replace, Vim's changes) is a step of its own, and undo brings every cursor back.
  Each step keeps only what changed, not a copy of the file.
- Line height for the code (Preferences → Editor → Text), from 1.00× (as the font sets it) to
  2.00×. The extra space is shared above and below each line.
- Git, in the spirit of VS Code's and Zed's:
  - A Git panel in the sidebar (Shift+Ctrl+G) lists merge conflicts, staged changes and changes
    not staged yet. Hover a file to stage, unstage or discard it, or a section to do all of its
    files.
  - Write a message and commit (Ctrl+Enter); with nothing staged, Commit All stages everything
    first. You can also amend the last commit or undo it.
  - The branch shows in the panel and the status bar, with how far it is ahead of and behind
    its upstream. Click it to switch branch, or type a new name to make one.
  - Sync (pull, then push), fetch, pull, push and publishing a new branch are in the panel's
    more menu, the Git menu and Commands. A folder that isn't a repository gets an Initialize
    Repository button.
  - The tree colors changed files and shows their letter (M, A, U, D), with a dot on folders
    that hold changes.
  - The gutter marks lines added, changed and deleted since staged, including edits not saved
    yet. Click a mark to see the old lines, revert that change, stage just it, or step to the
    next one (Alt+F5, Shift+Alt+F5).
  - The git work is the new shared `rivet-git` crate, which has no user interface, so a Git app
    can be built on it later. It runs the `git` program, so your own config, hooks, credentials
    and signing all apply.

## 0.1.0

The first version.

- Open a folder as the workspace, or single files; a tree with faint guides, and tabs.
- Tree-sitter highlighting for Rust, Python, JavaScript, TypeScript, TSX, JSON, shell, C, Go,
  HTML, CSS, TOML and YAML, in the theme's colors; the shared highlighter for other files.
- Go to File (Ctrl+P, fuzzy), Commands (Shift+Ctrl+P) and Go to Line (Ctrl+G), on Rivet's quick panel.
- Line numbers, word wrap, Tab as spaces, code font and size, a status bar with line, column and language.
- Safe saving, reloading files changed elsewhere, and questions before losing changes.
- 39 editor color themes (One Dark, Dracula, Catppuccin, Gruvbox, Solarized, Tokyo Night, GitHub,
  Rosé Pine…), previewed live from Commands → Color Theme, or the app's own colors.
- A right-click menu in the sidebar: new file or folder, rename, move to the Trash, copy the path,
  search in a folder, open in the file manager or a terminal, collapse all, reload.
- A right-click menu in the editor: Go to Definition, Find References, Back / Forward, cut, copy,
  paste, select all, matches, comment out, find and search the folder for the word.
- Go to Definition (F12, Ctrl+click), Find References (Shift+F12), what a name is when the
  pointer rests on it, and Back / Forward (Alt+Left / Alt+Right), from the language's server;
  without one, Go to Definition finds `fn` / `def` / `class` / `struct`… by name in the folder.
- Vim keys (Preferences > Editor): Normal, Insert, Visual and Visual Line modes, counts,
  operators with motions and text objects, registers, `.` repeat, search, and `:` commands
  (w, q, wq, x, a line number, s///). A block cursor, and the mode in the status bar.
- Esc no longer takes the keyboard away from the editor.
- File and folder icons by type (Material Design Icons), each kind in its own colour leaning a
  little toward the accent; Preferences > Appearance > Icons can switch back to the icon theme.
- Suggestions as you type (Ctrl+Space any time) and errors underlined, from the language's
  server when one is installed (rust-analyzer, clangd, pyright, typescript-language-server,
  gopls…), else from the file's own words.
- The language and the indentation (tabs or spaces, and how many) are told from each file and
  shown in the status bar; click either to change it, or to convert the indentation.
- A new start page (Start and Recent side by side, with icons and keys) and Preferences laid
  out like the other Rivet apps: pages for Appearance, Editor, Window, Keyboard and About.
- Faster folder search: several threads at once, a newer query stops the old one, pictures and
  other binaries skipped, and only the result rows on screen drawn.
- Find and replace (Ctrl+F, Ctrl+H; case, whole words, regular expressions), and search and
  replace across the folder (Shift+Ctrl+F) in the sidebar.
- Multiple cursors (Ctrl+D, Shift+Ctrl+L, Alt+click, Ctrl+Alt+Up/Down) and VS Code's line keys:
  move, copy, delete, comment, indent, new line above/below, select line.
- Preferences for the look, the editor and the window; `--install-desktop-entry`.
