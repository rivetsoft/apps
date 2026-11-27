#!/bin/sh
# Regenerate the README pictures of Rivet Snip, Vice, Calendar, Color, View, Fetch and Docs
# (docs/screenshots/rivet-APP-wide.png) in the Rivet scheme, in a throwaway home with each app's
# sample data, so they never show anyone's real files, events or desktop colors.
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-snip -p rivet-vice -p rivet-calendar -p rivet-color -p rivet-view -p rivet-fetch -p rivet-terminal -p rivet-docs
# Rivet Docs draws PDFs with PDFium, kept beside the program in target/lib.
[ -f target/lib/rivet-docs/libpdfium.so ] || sh utilities/lib/pdfium.sh target/lib/rivet-docs
OUT="$(pwd)/docs/screenshots"
BIN="$(pwd)/target/debug"
mkdir -p "$OUT"
T="$(mktemp -d /tmp/rivet-app-docs-XXXXXX)"
mkdir -p "$T/home" "$T/config/rivet"
# The Rivet scheme as the chosen palette (what Rivet Shell's settings write), so every app
# draws in the website's pink.
cat > "$T/config/rivet/palette.css" <<'EOF'
/* Rivet colors: Rivet */
@define-color accent_bg_color #d53f8c;
@define-color accent_fg_color #ffffff;
@define-color window_bg_color #281d24;
@define-color window_fg_color #e8dde3;
@define-color headerbar_bg_color #281d24;
@define-color sidebar_bg_color #241920;
@define-color view_bg_color #1e131a;
@define-color view_fg_color #e8dde3;
@define-color popover_bg_color #32272e;
@define-color secondary #0084b8;
@define-color tertiary #fff270;
EOF
export HOME="$T/home" TMPDIR="$T" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache"
export RIVET_SHOT_SIZE=1280x800

"$BIN/rivet-snip" --screenshot "$OUT/rivet-snip-wide.png"
"$BIN/rivet-vice" --screenshot "$OUT/rivet-vice-wide.png"
RIVET_CALENDAR_VIEW=month "$BIN/rivet-calendar" --screenshot "$OUT/rivet-calendar-wide.png"
"$BIN/rivet-color" --screenshot "$OUT/rivet-color-wide.png"
# Rivet View makes its own sample pictures (in the throwaway home's Pictures) when given none.
RIVET_VIEW_DETAILS=1 RIVET_VIEW_PICK=0 "$BIN/rivet-view" --screenshot "$OUT/rivet-view-wide.png"

# Rivet Docs with the sample field guide and comic (utilities/lib/docs-samples.py).
python3 utilities/lib/docs-samples.py "$T/docs" >/dev/null
GUIDE="$T/docs/Rivet Field Guide.pdf"
RIVET_DOCS_FILE="$GUIDE" RIVET_DOCS_PAGE=3 RIVET_DOCS_SIDEBAR=pages RIVET_DOCS_DARK=off "$BIN/rivet-docs" --screenshot "$OUT/rivet-docs-wide.png"
RIVET_DOCS_FILE="$GUIDE" RIVET_DOCS_PAGE=3 RIVET_DOCS_SIDEBAR=outline RIVET_DOCS_DARK=off RIVET_DOCS_FIND=panel "$BIN/rivet-docs" --screenshot "$OUT/rivet-docs-find-wide.png"
RIVET_DOCS_FILE="$GUIDE" RIVET_DOCS_PAGE=8 RIVET_DOCS_SIDEBAR=pages RIVET_DOCS_DARK=on "$BIN/rivet-docs" --screenshot "$OUT/rivet-docs-dark-wide.png"
RIVET_DOCS_FILE="$T/docs/Rivet at the Market.cbz" RIVET_DOCS_PAGE=4 RIVET_DOCS_VIEW=single RIVET_DOCS_SIDEBAR=pages "$BIN/rivet-docs" --screenshot "$OUT/rivet-docs-comic-wide.png"

# rivetfetch in Rivet Terminal, with RIVET_FETCH_DEMO's made-up computer instead of this one.
cat > "$T/fetch.sh" <<'EOF'
#!/bin/bash
e=$(printf '\033')
p() { printf '%s[1;32mrivet%s[0m@%s[1;34mdemo%s[0m %s[36m~%s[0m %s[1;35m>%s[0m %s' "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$e" "$1"; }
p 'rivetfetch'; echo
rivetfetch --color always --modules title,separator,os,host,kernel,uptime,packages,shell,wm,terminal,cpu,gpu,memory,disk,rivet,break,colors
p ''
exec sleep 600
EOF
PATH="$BIN:$PATH" RIVET_FETCH_DEMO=1 RIVET_SHOT_AFTER=200 "$BIN/rivet-terminal" --screenshot "$OUT/rivet-fetch-wide.png" -e bash "$T/fetch.sh"
rm -rf "$T"
echo "Saved rivet-{snip,vice,calendar,color,view,fetch,docs,docs-find,docs-dark,docs-comic}-wide.png in $OUT"
