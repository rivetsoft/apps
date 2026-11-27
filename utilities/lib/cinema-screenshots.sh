#!/bin/sh
# Regenerate Rivet Cinema's screenshots (docs/screenshots/rivet-cinema-*.png) from sample media made
# by cinema-samples.sh, in the Rivet scheme, in a throwaway home, so they never show anyone's
# files or desktop colors. RIVET_CINEMA_CLICK=X,Y clicks there first (to open a menu).
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-cinema
OUT="$(pwd)/docs/screenshots"
BIN="$(pwd)/target/debug"
SAMPLES=$(sh utilities/lib/cinema-samples.sh)
mkdir -p "$OUT"
T="$(mktemp -d /tmp/rivet-cinema-docs-XXXXXX)"
mkdir -p "$T/home" "$T/config/rivet"
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
export RIVET_SHOT_SIZE=1280x800 RIVET_CINEMA_SAMPLE="$SAMPLES"

# The first picture has the Subtitles menu open (CINEMA_MENU_CLICK moves the click).
RIVET_CINEMA_SCENE=video RIVET_CINEMA_CLICK=${CINEMA_MENU_CLICK:-934,757} "$BIN/rivet-cinema" --screenshot "$OUT/rivet-cinema-wide.png"
RIVET_CINEMA_SCENE=audio "$BIN/rivet-cinema" --screenshot "$OUT/rivet-cinema-music-wide.png"
RIVET_CINEMA_SCENE=seek "$BIN/rivet-cinema" --screenshot "$OUT/rivet-cinema-seek-wide.png"
RIVET_CINEMA_SCENE=queue "$BIN/rivet-cinema" --screenshot "$OUT/rivet-cinema-queue-wide.png"
rm -rf "$T"
echo "Saved rivet-cinema-{,music-,seek-,queue-}wide.png in $OUT"
