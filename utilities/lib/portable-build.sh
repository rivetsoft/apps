#!/bin/sh
# Build the release binary linked for an old glibc, regardless of the host.
#
# Host linkers bind atan2f/acosf to whatever libm they have. On glibc 2.43 that
# is GLIBC_2.43, and the binary then refuses to start on Debian 13 and older.
# cargo-zigbuild links those symbols against glibc 2.31 instead.
set -eu
ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
cd "$ROOT"
# shellcheck disable=SC1091
. "$ROOT/utilities/lib/ensure-zigbuild.sh"

triple=$("$ROOT/utilities/lib/gnu-triple.sh")
# RIVET_GLIBC_LINK: a newer glibc for a program that needs one (LINK_GLIBC in apps.sh).
link=${RIVET_GLIBC_LINK:-$GLIBC_LINK_VERSION}
target=$triple.$link
# Which program: RIVET_PKG (a cargo package; rivet, the file manager, by default).
pkg=${RIVET_PKG:-rivet}
# A program that links libxkbcommon itself (Rivet Shell) links Ubuntu 22.04's copy, the one the
# AppImage bundles, not the build machine's, which may need a newer glibc than the target's.
libs=$CACHE/link-libs
if [ ! -e "$libs/libxkbcommon.so" ]; then
    "$ROOT/utilities/lib/bundle-runtime-libs.sh" "$libs" >/dev/null
    ln -sf libxkbcommon.so.0 "$libs/libxkbcommon.so"
fi
echo "Linking $pkg for glibc $link ($target) with $($CARGO_ZIGBUILD_ZIG_PATH version)"
RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-L native=$libs" cargo zigbuild --release --locked --target "$target" -p "$pkg"
bin=$("$ROOT/utilities/lib/portable-bin.sh")
[ -x "$bin" ] || { echo "expected $bin" >&2; exit 1; }
"$ROOT/utilities/lib/check-glibc.sh" --max "$GLIBC_MAX_VERSION" "$bin"
echo "$bin"
