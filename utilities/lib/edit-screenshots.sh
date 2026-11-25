#!/bin/sh
# Regenerate Rivet Edit's screenshots (docs/screenshots/edit-*.png) from its demo project, in a
# throwaway home, so they never show anyone's real files, folders or settings.
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-edit
OUT="docs/screenshots"
mkdir -p "$OUT"
T="$(mktemp -d /tmp/rivet-edit-docs-XXXXXX)"
mkdir -p "$T/home"
export HOME="$T/home" TMPDIR="$T" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache"
export RIVET_SHOT_SIZE=1280x800
BIN=./target/debug/rivet-edit
shot() { name=$1; shift; env "$@" "$BIN" --screenshot "$OUT/edit-$name-wide.png"; }

shot editor RIVET_EDIT_THEME=dark RIVET_EDIT_OPEN=web/app.js,Cargo.toml,src/main.rs RIVET_EDIT_LINE=9
shot light RIVET_EDIT_THEME=light RIVET_EDIT_OPEN=src/main.rs,web/app.js
shot go-to-file RIVET_EDIT_THEME=dark RIVET_EDIT_OPEN=src/main.rs RIVET_EDIT_QUICK=files:wds
shot commands RIVET_EDIT_THEME=dark RIVET_EDIT_OPEN=src/main.rs RIVET_EDIT_QUICK=commands
shot preferences RIVET_EDIT_THEME=light RIVET_EDIT_OPEN=src/main.rs RIVET_EDIT_PREFS=1
echo "Saved edit-*-wide.png in $OUT"
