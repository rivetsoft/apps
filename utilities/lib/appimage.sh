#!/bin/sh
# Build a single-file AppImage for an app: dist/Rivet_Files-VERSION-ARCH.AppImage (RIVET_APP or
# the first argument picks the app: files by default, ftp, notes, edit, terminal, welcome, snip or color).
#
# Needs mksquashfs (squashfs-tools). The AppImage "type 2 runtime" (the small
# program at the front of every AppImage) is downloaded once from the AppImage
# project and cached in ~/.cache/rivet-build/.
set -e
cd "$(dirname "$0")/../.."
. ./utilities/lib/apps.sh
app_select "${1:-${RIVET_APP:-files}}"

VERSION=$(app_version)
ARCH=$(uname -m)
# "Rivet Files" -> Rivet_Files
OUT="dist/$(echo "$TITLE" | tr ' ' _)-$VERSION-$ARCH.AppImage"
CACHE="${XDG_CACHE_HOME:-$HOME/.cache}/rivet-build"
RUNTIME="$CACHE/runtime-$ARCH"

command -v mksquashfs >/dev/null || { echo "Please install squashfs-tools (mksquashfs)." >&2; exit 1; }

if [ ! -s "$RUNTIME" ]; then
    mkdir -p "$CACHE"
    echo "Downloading the AppImage runtime for $ARCH…"
    curl -fL --proto '=https' -o "$RUNTIME.part" \
        "https://github.com/AppImage/type2-runtime/releases/download/continuous/runtime-$ARCH"
    # Sanity check: it must be an ELF program.
    head -c 4 "$RUNTIME.part" | od -An -c | grep -q 'E   L   F' || { echo "Download is not an ELF file." >&2; rm -f "$RUNTIME.part"; exit 1; }
    mv "$RUNTIME.part" "$RUNTIME"
fi

# Linked for glibc 2.31, not whatever this machine's libm exports.
RIVET_PKG=$PKG ./utilities/lib/portable-build.sh >/dev/null
RIVET_BIN=$(RIVET_PKG=$PKG ./utilities/lib/portable-bin.sh)
APPDIR="dist/AppDir-$DIST_NAME"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$APPDIR/usr/share/applications" "$APPDIR/usr/share/icons"
strip -o "$APPDIR/usr/bin/$BIN" "$RIVET_BIN"
# winit dlopens these; ldd on rivet does not see them. Ubuntu 22.04 builds,
# so the bundle itself does not need glibc 2.43.
./utilities/lib/bundle-runtime-libs.sh "$APPDIR/usr/lib"
cp "$APP/data/$DIST_NAME.desktop" "$APPDIR/$DIST_NAME.desktop"
cp "$APP/data/$DIST_NAME.desktop" "$APPDIR/usr/share/applications/"
cp "$APP/data/$DIST_NAME.svg" "$APPDIR/$DIST_NAME.svg"
# The icon in every size the app has (Rivet Files), else its SVG as the scalable one.
if [ -d "$APP/data/icons/hicolor" ]; then
    cp -r "$APP/data/icons/hicolor" "$APPDIR/usr/share/icons/"
else
    mkdir -p "$APPDIR/usr/share/icons/hicolor/scalable/apps"
    cp "$APP/data/$DIST_NAME.svg" "$APPDIR/usr/share/icons/hicolor/scalable/apps/"
fi
# .DirIcon, what file managers show for the AppImage: a 256 px PNG.
if [ -f "$APP/data/icons/hicolor/256x256/apps/$DIST_NAME.png" ]; then
    cp "$APP/data/icons/hicolor/256x256/apps/$DIST_NAME.png" "$APPDIR/.DirIcon"
elif command -v rsvg-convert >/dev/null; then
    rsvg-convert -w 256 -h 256 -o "$APPDIR/.DirIcon" "$APP/data/$DIST_NAME.svg"
elif command -v magick >/dev/null; then
    magick -background none -density 384 "$APP/data/$DIST_NAME.svg" -resize 256x256 "png:$APPDIR/.DirIcon"
else
    cp "$APP/data/$DIST_NAME.svg" "$APPDIR/.DirIcon"
fi
cat > "$APPDIR/AppRun" <<EOF
#!/bin/sh
HERE="\$(dirname "\$(readlink -f "\$0")")"
# Bundled libxkbcommon, libxkbcommon-x11 and libxcb-xkb. The system provides
# libc, libX11, Wayland and OpenGL so they match the running desktop.
export LD_LIBRARY_PATH="\$HERE/usr/lib\${LD_LIBRARY_PATH:+:\$LD_LIBRARY_PATH}"
exec "\$HERE/usr/bin/$BIN" "\$@"
EOF
chmod +x "$APPDIR/AppRun"

# The file we ship, plus every bundled library, must stay under the ceiling.
ROOT=$(pwd)
# shellcheck disable=SC1091
. ./utilities/lib/ensure-zigbuild.sh
set -- "$APPDIR/usr/bin/$BIN"
for lib in "$APPDIR"/usr/lib/lib*.so.*; do
    [ -f "$lib" ] || continue
    set -- "$@" "$lib"
done
./utilities/lib/check-glibc.sh --max "$GLIBC_MAX_VERSION" --record dist/glibc-highest "$@"

SQUASH="$APPDIR.squashfs"
rm -f "$OUT" "$SQUASH"
mksquashfs "$APPDIR" "$SQUASH" -root-owned -noappend -comp zstd -quiet
cat "$RUNTIME" "$SQUASH" > "$OUT"
chmod +x "$OUT"
rm -rf "$APPDIR" "$SQUASH"
(cd dist && sha256sum "$(basename "$OUT")" > "$(basename "$OUT").sha256")
echo "Built $OUT"
