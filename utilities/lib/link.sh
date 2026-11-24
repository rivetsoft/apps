#!/bin/sh
# Make a command open the build in this folder (rebuilds are picked up automatically): `rivet`,
# or another app's when one is named. The link is to the debug build, so a system-wide one
# lets programs that don't have your own folders on their path (a display manager, a system
# service) run what you are working on.
#   utilities/rivet-dev link               -> /usr/bin/rivet        (asks for your password)
#   utilities/rivet-dev link shell         -> /usr/bin/rivet-shell
#   utilities/rivet-dev link --user        -> ~/.local/bin/rivet    (no password)
#   utilities/rivet-dev link [APP] --remove [--user]
set -e
cd "$(dirname "$0")/../.."
. utilities/lib/apps.sh
# The app: named before the command's flags (rivet-dev passes it on as RIVET_APP), or Rivet
# Shell, which has no releases and so isn't in the list the other commands share.
BIN=rivet PKG=rivet
[ -n "$RIVET_APP" ] && app_select "$RIVET_APP"
DIR=/usr/bin
SUDO=sudo
REMOVE=
for a in "$@"; do
    case "$a" in
        --user) DIR="${XDG_BIN_HOME:-$HOME/.local/bin}"; SUDO=""; mkdir -p "$DIR" ;;
        --remove) REMOVE=1 ;;
        shell | rivet-shell) BIN=rivet-shell PKG=rivet-shell ;;
        *) echo "Unknown: $a (an app, shell, --user or --remove)" >&2; exit 1 ;;
    esac
done
EXE="$(pwd)/target/debug/$BIN"
DEST="$DIR/$BIN"
if [ -e "$DEST" ] && [ ! -L "$DEST" ]; then
    echo "$DEST is a real file (probably from a package); leaving it alone." >&2
    exit 1
fi
if [ -n "$REMOVE" ]; then
    $SUDO rm -f "$DEST"
    echo "Removed $DEST"
    exit 0
fi
[ -x "$EXE" ] || cargo build -p "$PKG"
$SUDO ln -sfn "$EXE" "$DEST"
echo "$DEST -> $EXE"
