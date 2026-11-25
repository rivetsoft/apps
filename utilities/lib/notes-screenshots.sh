#!/bin/sh
# Regenerate Rivet Notes' screenshots (docs/screenshots/notes-*.png) from a throwaway demo
# vault, so they never show anyone's real notes, settings or desktop colors.
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-notes
OUT="docs/screenshots"
mkdir -p "$OUT"

T="$(mktemp -d /tmp/rivet-notes-docs-XXXXXX)"
V="$T/Notes"
mkdir -p "$V/Projects" "$V/Journal" "$V/Reading" "$V/Attachments"
d() { date -d "$1" +%Y-%m-%d; }
TODAY=$(d today)

cat > "$V/Welcome.md" <<EOF
---
tags: [start]
created: $TODAY 09:00
---
# Welcome to your vault

A vault is just a folder of **Markdown** files: nothing to import, nothing locked in.

## What's here

- Browse by folder or by tag, or jump anywhere with \`Ctrl+P\`
- Link notes with [[Launch plan]] and see who links back in Peek
- Type \`/\` for headings, todos, code and more

> Notes are plain files, so any app can read them.
EOF

cat > "$V/Projects/Launch plan.md" <<EOF
---
title: Launch plan
tags: [work, launch]
status: active
due: $(d '+9 days')
owner: Sam
repo: https://example.org/rivet
---
## Goals

Ship the first version with **live editing**, *links between notes* and super todos.
See [[Design notes]] for the look.

## Tasks

- [x] Pick a name and an icon #task #design
- [/] Write the release notes #task #writing due:$(d tomorrow) priority:medium
- [ ] Record the screenshots #task #docs due:$(d '+3 days') priority:high
- [ ] Check the Flatpak build #task #build due:$(d '-2 days') priority:high
- [ ] Say thanks to the testers

\`\`\`rust
fn main() {
    println!("Hello from Rivet Notes");
}
\`\`\`

![Release poster](../Attachments/Poster.png)
EOF

cat > "$V/Projects/Design notes.md" <<EOF
---
tags: [design]
---
## Colors

Follow the desktop's palette; fall back to a calm dark theme.

- [ ] Try a lighter sidebar #task #design due:$(d '+6 days')

Back to the [[Launch plan]].
EOF

cat > "$V/Journal/$TODAY.md" <<EOF
---
tags: [daily]
---
Long walk, then wrote the launch notes. #journal

- [/] Reply to the newsletter #task #people due:$TODAY
EOF

cat > "$V/Reading/Books.md" <<EOF
---
tags: [reading, lists]
---
1. A Pattern Language
2. The Design of Everyday Things
3. Thinking in Systems
EOF

cat > "$V/Inbox.md" <<EOF
Ideas to sort later. #inbox

- Look at sync options
- [ ] Back up the vault #task #chores priority:low
EOF

python3 -I - "$V/Attachments/Poster.png" <<'EOF'
import sys
from PIL import Image, ImageDraw
w, h = 900, 300
im = Image.new("RGB", (w, h))
px = im.load()
for y in range(h):
    for x in range(w):
        t = x / w
        px[x, y] = (int(120 + 110 * t), int(60 + 40 * (1 - t)), int(170 - 60 * t))
dr = ImageDraw.Draw(im)
for i in range(7):
    dr.ellipse((60 + i * 120, 170 - (i % 3) * 25, 140 + i * 120, 250 - (i % 3) * 25), fill=(255, 205, 120))
im.save(sys.argv[1])
EOF

export HOME="$T" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache"
export RIVET_SHOT_SIZE=1280x800 RIVET_NOTES_TODOS=1 RIVET_NOTES_DAILY=1
BIN=./target/debug/rivet-notes
shot() { name=$1; shift; env "$@" "$BIN" "$V" --screenshot "$OUT/notes-$name-wide.png"; }

shot live RIVET_NOTES_THEME=dark RIVET_NOTES_LIVE=1 RIVET_NOTES_PEEK=1 "RIVET_NOTES_OPEN=Projects/Launch plan.md"
shot source RIVET_NOTES_THEME=light "RIVET_NOTES_OPEN=Projects/Launch plan.md"
shot reading RIVET_NOTES_THEME=dark RIVET_NOTES_READ=1 "RIVET_NOTES_OPEN=Projects/Launch plan.md"
shot todos-board RIVET_NOTES_THEME=dark RIVET_NOTES_TODOS=board RIVET_NOTES_OPEN=Welcome.md
shot todos-list RIVET_NOTES_THEME=light RIVET_NOTES_TODOS=page RIVET_NOTES_OPEN=Welcome.md
shot quick-open RIVET_NOTES_THEME=dark RIVET_NOTES_LIVE=1 RIVET_NOTES_OPEN=Welcome.md "RIVET_NOTES_QUICK=open:la"
shot slash RIVET_NOTES_THEME=light RIVET_NOTES_LIVE=1 RIVET_NOTES_OPEN=Inbox.md "RIVET_NOTES_APPEND=\n/" RIVET_NOTES_SELECT=99999,99999
shot preferences RIVET_NOTES_THEME=dark RIVET_NOTES_PREFS=modules RIVET_NOTES_OPEN=Welcome.md
echo "Saved notes-*-wide.png in $OUT"
