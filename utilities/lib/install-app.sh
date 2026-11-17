#!/bin/sh
# Install @TITLE@ for the current user (no root needed), from a release archive or from source:
#   program       -> ~/.local/bin/@BIN@
#   app launcher  -> ~/.local/share/applications (via @BIN@ --install-desktop-entry)
# In the release archive @BIN@ and @TITLE@ are filled in; from a checkout, run
#   utilities/rivet-dev install ftp     (or notes)
set -e
cd "$(dirname "$0")"
BIN_NAME=${RIVET_INSTALL_BIN:-@BIN@}
if [ -x "./$BIN_NAME" ]; then
    SRC="./$BIN_NAME"                               # release download
else
    cd ../..                                         # a source checkout (utilities/lib)
    cargo build --release -p "$BIN_NAME"
    SRC="target/release/$BIN_NAME"
fi
DEST="${XDG_BIN_HOME:-$HOME/.local/bin}"
install -Dm755 "$SRC" "$DEST/$BIN_NAME"
"$DEST/$BIN_NAME" --install-desktop-entry
echo "Installed $BIN_NAME in $DEST. It is now in your app launcher."
