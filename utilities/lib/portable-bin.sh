#!/bin/sh
# Path of the glibc-pinned release binary (after utilities/lib/portable-build.sh).
set -eu
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
triple=$("$here/gnu-triple.sh")
# RIVET_BIN is the program (rivetfetch), else the package's name (RIVET_PKG); rivet by default.
printf '%s\n' "$root/target/$triple/release/${RIVET_BIN:-${RIVET_PKG:-rivet}}"
