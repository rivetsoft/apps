#!/bin/sh
# Path of the glibc-pinned release binary (after utilities/lib/portable-build.sh).
set -eu
here=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
root=$(CDPATH= cd -- "$here/../.." && pwd)
triple=$("$here/gnu-triple.sh")
# RIVET_PKG picks the program (its binary has the package's name); rivet by default.
printf '%s\n' "$root/target/$triple/release/${RIVET_PKG:-rivet}"
