#!/bin/sh
# Regenerate Rivet Play's screenshots (docs/screenshots/rivet-play-*.png) from sample media made
# by play-samples.sh, in the Rivet scheme, in a throwaway home, so they never show anyone's
# files or desktop colors. RIVET_PLAY_CLICK=X,Y clicks there first (to open a menu).
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-play
OUT="$(pwd)/docs/screenshots"
BIN="$(pwd)/target/debug"
SAMPLES=$(sh utilities/lib/play-samples.sh)
mkdir -p "$OUT"
T="$(mktemp -d /tmp/rivet-play-docs-XXXXXX)"
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
export RIVET_SHOT_SIZE=1280x800 RIVET_PLAY_SAMPLE="$SAMPLES"

# The first picture has the Subtitles menu open (PLAY_MENU_CLICK moves the click).
RIVET_PLAY_SCENE=video RIVET_PLAY_CLICK=${PLAY_MENU_CLICK:-934,757} "$BIN/rivet-play" --screenshot "$OUT/rivet-play-wide.png"
RIVET_PLAY_SCENE=audio "$BIN/rivet-play" --screenshot "$OUT/rivet-play-music-wide.png"
RIVET_PLAY_SCENE=seek "$BIN/rivet-play" --screenshot "$OUT/rivet-play-seek-wide.png"
RIVET_PLAY_SCENE=queue "$BIN/rivet-play" --screenshot "$OUT/rivet-play-queue-wide.png"
rm -rf "$T"
echo "Saved rivet-play-{,music-,seek-,queue-}wide.png in $OUT"
