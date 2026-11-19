#!/bin/sh
# Regenerate Rivet Shell's README picture (docs/screenshots/shell-wide.jpg): the demo desktop
# over a wallpaper, in the colors matugen makes from that wallpaper, at two pixels to a point.
#
#   utilities/rivet-dev shell-screenshots WALLPAPER
#
# The wallpaper isn't kept in the repository. The README's is "Sunset View" by Vittorio
# Staffolani: https://www.pexels.com/photo/sunset-view-1606327/
# Nothing of yours is read or changed: the colors are made in a throwaway folder.
set -e
WALL=$(realpath "${1:?usage: rivet-dev shell-screenshots WALLPAPER}")
cd "$(dirname "$0")/../.."
command -v matugen >/dev/null || { echo "shell-screenshots: matugen is needed for the colors" >&2; exit 1; }
cargo build -q -p rivet-shell
OUT="docs/screenshots"
T="$(mktemp -d /tmp/rivet-shell-docs-XXXXXX)"
trap 'rm -rf "$T"' EXIT
mkdir -p "$T/matugen/templates" "$T/rivet"

# Rivet's colors from the picture, as the shell has matugen make them.
XDG_CONFIG_HOME="$T" target/debug/rivet-shell matugen-template > "$T/matugen/templates/rivet-colors.css"
cat > "$T/matugen/config.toml" <<CONF
[config]

[templates.rivet]
input_path = '$T/matugen/templates/rivet-colors.css'
output_path = '$T/rivet/colors.css'
CONF
matugen image "$WALL" -m dark -c "$T/matugen/config.toml" -q

XDG_CONFIG_HOME="$T" RIVET_SHELL_WALL="$WALL" RIVET_SHOT_SIZE=1440x810 RIVET_SHOT_SCALE=2 \
    target/debug/rivet-shell --screenshot "$T/shell-wide.png"
# A photograph: far smaller as a JPEG than as a PNG.
if command -v magick >/dev/null; then
    magick "$T/shell-wide.png" -quality 90 "$OUT/shell-wide.jpg"
else
    python3 -c "import sys; from PIL import Image; Image.open(sys.argv[1]).convert('RGB').save(sys.argv[2], quality=90, optimize=True)" "$T/shell-wide.png" "$OUT/shell-wide.jpg"
fi
echo "Wrote $OUT/shell-wide.jpg"
