# Rivet Notes

Part of [Rivet Apps](../../README.md).

> **Early alpha.** Rivet Notes is not ready for daily use: things are missing, change without
> notice, or break. Try it on a copy of your notes you don't mind losing, and keep backups.

Markdown notes in plain folders. A vault is any folder of `.md` files: switch between vaults
from the menu, browse notes by folder or by tag, and edit frontmatter as properties. Ctrl+Space
shows the rendered note. More in [Rivet Notes](../../docs/rivet-notes.md).

```sh
cargo build -p rivet-notes
./target/debug/rivet-notes [VAULT]       # the window
./target/debug/rivet-notes tags [VAULT]  # list the vault's tags
```
