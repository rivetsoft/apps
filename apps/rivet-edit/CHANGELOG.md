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
