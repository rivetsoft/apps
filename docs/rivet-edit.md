# Rivet Edit

A lightweight code editor, the fourth Rivet app. It reuses the shared crates: `rivet-look` (theme,
desktop colors, fonts, icons, in-app windows, the quick panel and its fuzzy matching, the
highlighter's colors) and `rivet-kit` (screenshots for checks).

**Status:** just started (0.1.0, unreleased). Screenshots: `utilities/rivet-dev edit-screenshots`.

## Highlighting

Tree-sitter (`tree-sitter-highlight`) with the grammars' own highlight queries, for Rust, Python,
JavaScript, TypeScript, TSX, JSON, Bash, C, Go, HTML, CSS, TOML and YAML. Captures map onto the
theme's code colors (`rivet_look::highlight::Colors`): keywords, strings, numbers and constants,
comments, functions, types, punctuation. Other files use the shared highlighter. The colored
layout is cached until the text, colors or font change, and files over 1.5 MB are shown plain.

Adding a language: its grammar crate in `apps/rivet-edit/Cargo.toml`, a `Lang` in `syntax.rs`
(file endings, grammar, query), then `utilities/rivet-dev flatpak-sources` (the Flatpak build
fetches the whole lockfile offline).

## Workspaces

A folder's files are listed (up to 20,000), skipping `.git`, `.hg`, `.svn`, `node_modules`,
`target`, `__pycache__`, `.venv`, `venv` and `.cache`. The list refreshes every two seconds, and
open files changed elsewhere reload when they have no changes here.

## Next

Find and replace, a project-wide search, multiple cursors, more languages, an outline from
tree-sitter, Git status in the tree, and a terminal panel.
