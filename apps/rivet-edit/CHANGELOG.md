# Changelog

## Unreleased

The first version.

- Open a folder as the workspace, or single files; a tree with faint guides, and tabs.
- Tree-sitter highlighting for Rust, Python, JavaScript, TypeScript, TSX, JSON, shell, C, Go,
  HTML, CSS, TOML and YAML, in the theme's colors; the shared highlighter for other files.
- Go to File (Ctrl+P, fuzzy), Commands (Shift+Ctrl+P) and Go to Line (Ctrl+G), on Rivet's quick panel.
- Line numbers, word wrap, Tab as spaces, code font and size, a status bar with line, column and language.
- Safe saving, reloading files changed elsewhere, and questions before losing changes.
- 39 editor color themes (One Dark, Dracula, Catppuccin, Gruvbox, Solarized, Tokyo Night, GitHub,
  Rosé Pine…), previewed live from Commands → Color Theme, or the app's own colors.
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
