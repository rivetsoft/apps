#!/bin/sh
# Put PDFium, the PDF engine Rivet Reader loads when it opens a PDF, in DEST: DEST/libpdfium.so,
# and its licenses in DEST/licenses/pdfium/ (or LICENSES_DEST when that is set).
#
#   utilities/lib/pdfium.sh DEST [LICENSES_DEST]
#
# The library is Google's PDFium as built by bblanchon/pdfium-binaries, pinned to one build and
# checked against its sha256. It is the build pdfium-render's `pdfium_8125` bindings (in
# crates/rivet-doc) are written for: move both together. Downloads are cached in
# ~/.cache/rivet-build/ like the keyboard libraries.
set -eu

if [ $# -lt 1 ] || [ $# -gt 2 ]; then
    echo "usage: $0 DEST [LICENSES_DEST]" >&2
    exit 2
fi
dest=$1
licenses=${2:-$dest/licenses/pdfium}

BUILD=8125
case "$(uname -m)" in
    x86_64) asset=pdfium-linux-x64.tgz sum=e1c3ce6e270cb5e0c20816b379ad9f2c1309972697c505f508de92bf5e7d6659 ;;
    aarch64 | arm64) asset=pdfium-linux-arm64.tgz sum=8b8818f2ec57d1b213d952ca6d7f0ffeca9d0110487d0e20ad83ba918bc92efa ;;
    *) echo "No PDFium build is pinned for $(uname -m)." >&2; exit 1 ;;
esac

cache=${XDG_CACHE_HOME:-$HOME/.cache}/rivet-build
mkdir -p "$cache"
tgz=$cache/pdfium-$BUILD-$asset
if ! echo "$sum  $tgz" | sha256sum -c - >/dev/null 2>&1; then
    echo "Downloading PDFium $BUILD ($asset)"
    curl -fL --proto '=https' --retry 3 -o "$tgz.part" "https://github.com/bblanchon/pdfium-binaries/releases/download/chromium/$BUILD/$asset"
    echo "$sum  $tgz.part" | sha256sum -c -
    mv "$tgz.part" "$tgz"
fi

stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
tar -C "$stage" -xzf "$tgz"
[ -f "$stage/lib/libpdfium.so" ] || { echo "$asset has no lib/libpdfium.so" >&2; exit 1; }
[ -f "$stage/LICENSE" ] && [ -f "$stage/licenses/pdfium.txt" ] || { echo "$asset is missing its license files" >&2; exit 1; }
mkdir -p "$dest" "$licenses"
cp "$stage/lib/libpdfium.so" "$dest/libpdfium.so"
cp "$stage/licenses/"* "$licenses/"
cp "$stage/LICENSE" "$licenses/pdfium-binaries.txt"
echo "PDFium $BUILD in $dest"
