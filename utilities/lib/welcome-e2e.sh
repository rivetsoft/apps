#!/bin/sh
# Rivet Welcome, end to end, against a stand-in for GitHub (welcome-e2e-server.py) in a
# throwaway home folder: nothing outside it is touched, and nothing is published.
#
#   utilities/rivet-dev welcome-e2e [--old FILE.tar.gz]
#
# 1. The released Rivet Welcome before this one (from GitHub, read only, or --old) is installed
#    the way a person does it, and updates itself to this build.
# 2. This build installs two apps, updates one (over a connection that drops halfway), finds
#    pre-releases only when asked, follows the Edge channel's nightly builds by commit and goes
#    back to the stable release, refuses a damaged download and one with no checksum, puts an
#    app back when its installer fails, says when GitHub's limit is used up, leaves an app
#    installed elsewhere alone, waits its turn behind another Rivet Welcome, and removes an app
#    and then itself.
# After each step it checks what --version says and the launcher entries.
#
# RIVET_WELCOME_BIN=PATH tests that program instead of building target/debug/rivet-welcome.
# KEEP=1 keeps the throwaway folder to look at afterwards.
set -eu
cd "$(dirname "$0")/../.."
REPO=$(pwd)
. ./utilities/lib/apps.sh
ARCH=$(uname -m)

OLD_TARBALL=
while [ $# -gt 0 ]; do
    case "$1" in
        --old) OLD_TARBALL=$(realpath "$2"); shift 2 ;;
        *) echo "Usage: utilities/rivet-dev welcome-e2e [--old FILE.tar.gz]" >&2; exit 1 ;;
    esac
done

NEW=${RIVET_WELCOME_BIN:-}
if [ -z "$NEW" ]; then
    cargo build --locked -p rivet-welcome
    NEW=$REPO/target/debug/rivet-welcome
fi
NEW=$(realpath "$NEW")
NEWV=$("$NEW" --version | awk '{print $2}')

WORK=$(mktemp -d "${TMPDIR:-/tmp}/rivet-welcome-e2e.XXXXXX")
SERVER_PID=
cleanup() {
    [ -n "$SERVER_PID" ] && kill "$SERVER_PID" 2>/dev/null || true
    if [ "${KEEP:-}" = 1 ]; then echo "Kept $WORK"; else rm -rf "$WORK"; fi
}
trap cleanup EXIT INT TERM

# A home folder of its own: every XDG folder in it, its ~/.local/bin first on PATH.
export HOME="$WORK/home"
export XDG_CONFIG_HOME="$HOME/.config" XDG_DATA_HOME="$HOME/.local/share" XDG_CACHE_HOME="$HOME/.cache"
unset XDG_BIN_HOME RIVET_WELCOME_RELEASES RIVET_WELCOME_REPO RIVET_WELCOME_INSTALLED RIVET_WELCOME_API 2>/dev/null || true
BIN="$HOME/.local/bin"
APPS="$XDG_DATA_HOME/applications"
SYS="$WORK/elsewhere"
mkdir -p "$HOME" "$BIN" "$SYS"
# Only the system's own programs besides: an app installed in the real home folder mustn't count.
export PATH="$BIN:$SYS:/usr/local/bin:/usr/bin:/bin"
SRV="$WORK/server"
mkdir -p "$SRV/dl" "$SRV/flaky"
: > "$SRV/releases.txt"
: > "$SRV/log"

step=0
step() { step=$((step + 1)); printf '\n== %s. %s\n' "$step" "$*"; }
fail() { echo "FAILED (step $step): $*" >&2; exit 1; }
ok() { echo "  ok: $*"; }

# --version of an installed program, or "none".
version() { if [ -x "$BIN/$1" ]; then "$BIN/$1" --version | awk '{print $NF}'; else echo none; fi; }
expect_version() {
    got=$(version "$1")
    [ "$got" = "$2" ] || fail "$1 --version says $got, expected $2"
    ok "$1 is $2"
}
# The launcher entry runs the program in ~/.local/bin.
expect_entry() {
    f="$APPS/$1.desktop"
    [ -f "$f" ] || fail "no launcher entry $f"
    grep -q "^Exec=\"$BIN/$2\"" "$f" || fail "$f doesn't run $BIN/$2: $(grep ^Exec= "$f")"
    ok "launcher entry $1.desktop runs $BIN/$2"
}
expect_no_entry() {
    [ ! -e "$APPS/$1.desktop" ] || fail "$APPS/$1.desktop is still there"
    ok "no launcher entry $1.desktop"
}
# Run a command that should fail, and check what it says.
expect_fail() {
    want=$1; shift
    if out=$("$@" 2>&1); then fail "'$*' worked, and should have failed: $out"; fi
    echo "$out" | sed 's/^/  | /'
    echo "$out" | grep -qF -- "$want" || fail "'$*' didn't say '$want'"
    ok "refused: $want"
}

# A release archive as utilities/lib/release-app.sh lays one out: the program and the
# install.sh made from install-app.sh, then its .sha256.
pack() { # APP VERSION PROGRAM-FILE [INSTALLER-FILE]
    (
        app_select "$1"
        name="$DIST_NAME-$2-$ARCH-linux"
        stage="$WORK/stage/$name"
        rm -rf "$stage"
        mkdir -p "$stage"
        cp "$3" "$stage/$BIN"
        chmod +x "$stage/$BIN"
        if [ -n "${4:-}" ]; then
            cp "$4" "$stage/install.sh"
        else
            sed "s/@BIN@/$BIN/g; s/@TITLE@/$TITLE/g; s/@LAUNCHER@/$LAUNCHER/g" utilities/lib/install-app.sh > "$stage/install.sh"
        fi
        chmod +x "$stage/install.sh"
        tar -C "$WORK/stage" -czf "$SRV/dl/$name.tar.gz" "$name"
        (cd "$SRV/dl" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
    )
}
# A stand-in app: says its version and adds its launcher entry, as the real ones do.
fake() { # APP VERSION -> path of the program
    (
        app_select "$1"
        f="$WORK/fake/$BIN-$2"
        mkdir -p "$WORK/fake"
        cat > "$f" <<EOF
#!/bin/sh
case "\$1" in
    --version) echo "$BIN $2" ;;
    --install-desktop-entry)
        d="\${XDG_DATA_HOME:-\$HOME/.local/share}/applications"
        mkdir -p "\$d"
        printf '[Desktop Entry]\nType=Application\nName=$TITLE\nExec="%s"\nIcon=$DIST_NAME\n' "\$(readlink -f "\$0")" > "\$d/$DIST_NAME.desktop" ;;
esac
EOF
        chmod +x "$f"
        echo "$f"
    )
}
# Put a release on the stand-in GitHub, newest first: publish APP VERSION pre|stable [no-sum]
publish() {
    (
        app_select "$1"
        name="$DIST_NAME-$2-$ARCH-linux.tar.gz"
        assets="$name"
        [ "${4:-}" = no-sum ] || assets="$assets $name.sha256"
        { echo "$TAG_PREFIX$2 $3 $assets"; cat "$SRV/releases.txt"; } > "$SRV/releases.txt.new"
        mv "$SRV/releases.txt.new" "$SRV/releases.txt"
    )
}
# The rolling edge release of an app, holding one build: publish_edge APP VERSION
publish_edge() {
    (
        app_select "$1"
        name="$DIST_NAME-$2-$ARCH-linux.tar.gz"
        { echo "$DIST_NAME-edge pre $name $name.sha256"; grep -v "^$DIST_NAME-edge " "$SRV/releases.txt" || true; } > "$SRV/releases.txt.new"
        mv "$SRV/releases.txt.new" "$SRV/releases.txt"
    )
}
setting() { mkdir -p "$XDG_CONFIG_HOME/rivet-welcome"; echo "$1" > "$XDG_CONFIG_HOME/rivet-welcome/settings"; }
count() { grep -cF -- "$1" "$SRV/log" || true; }

python3 utilities/lib/welcome-e2e-server.py "$SRV" "$WORK/port" &
SERVER_PID=$!
i=0
while [ ! -f "$WORK/port" ]; do
    i=$((i + 1)); [ $i -lt 100 ] || fail "the stand-in server didn't start"; sleep 0.1
done
BASE="http://127.0.0.1:$(cat "$WORK/port")"
echo "Stand-in GitHub at $BASE, home folder $HOME, testing Rivet Welcome $NEWV ($NEW)"

# ---------------------------------------------------------------------------------------------
step "Install the Rivet Welcome released before $NEWV, as a person would"
if [ -z "$OLD_TARBALL" ]; then
    tok=${GITHUB_TOKEN:-${GH_TOKEN:-}}
    if [ -n "$tok" ]; then
        curl -fsSL -H "Authorization: Bearer $tok" "https://api.github.com/repos/rivetsoft/apps/releases?per_page=100" > "$WORK/github.json"
    else
        curl -fsSL "https://api.github.com/repos/rivetsoft/apps/releases?per_page=100" > "$WORK/github.json"
    fi
    OLD_URL=$(NEWV="$NEWV" ARCH="$ARCH" python3 - "$WORK/github.json" <<'PY'
import json, os, re, sys
def key(v):
    main, _, pre = v.partition("-")
    return [int(n) for n in main.split(".")], pre == "", pre
new = key(os.environ["NEWV"])
best = None
for r in json.load(open(sys.argv[1])):
    m = re.fullmatch(r"rivet-welcome-v(.+)", r["tag_name"])
    if r["draft"] or not m or key(m.group(1)) >= new:
        continue
    for a in r["assets"]:
        if a["name"] == f"rivet-welcome-{m.group(1)}-{os.environ['ARCH']}-linux.tar.gz":
            if best is None or key(m.group(1)) > best[0]:
                best = (key(m.group(1)), a["browser_download_url"])
print(best[1] if best else "")
PY
)
    [ -n "$OLD_URL" ] || fail "no released Rivet Welcome older than $NEWV on GitHub"
    OLD_TARBALL="$WORK/$(basename "$OLD_URL")"
    curl -fsSL -o "$OLD_TARBALL" "$OLD_URL"
    curl -fsSL -o "$OLD_TARBALL.sha256" "$OLD_URL.sha256"
    (cd "$WORK" && sha256sum -c "$(basename "$OLD_TARBALL").sha256" >/dev/null) || fail "$OLD_URL doesn't match its checksum"
fi
mkdir -p "$WORK/old"
tar -C "$WORK/old" -xzf "$OLD_TARBALL"
sh "$WORK"/old/*/install.sh >/dev/null
OLDV=$(version rivet-welcome)
[ "$OLDV" != none ] || fail "the old Rivet Welcome didn't install"
ok "Rivet Welcome $OLDV is installed from $(basename "$OLD_TARBALL")"
expect_entry rivet-welcome rivet-welcome

# ---------------------------------------------------------------------------------------------
step "The old Rivet Welcome updates itself to $NEWV"
pack welcome "$NEWV" "$NEW"
publish welcome "$NEWV" stable
# An edge build of Rivet Welcome is up too, numbered above this one: older versions, which know
# nothing of edge builds, must pass it by.
WELCOME_EDGE="$NEWV.1-edge.20261127.gfeed123"
pack welcome "$WELCOME_EDGE" "$(fake welcome "$WELCOME_EDGE")"; publish_edge welcome "$WELCOME_EDGE"
# Older versions read the list from a file (RIVET_WELCOME_RELEASES); the downloads still come
# from the stand-in server.
curl -fsS "$BASE/all" > "$WORK/releases.json"
RIVET_WELCOME_RELEASES="$WORK/releases.json" "$BIN/rivet-welcome" install welcome
expect_version rivet-welcome "$NEWV"
expect_entry rivet-welcome rivet-welcome

# From here on, the new one asks the stand-in GitHub itself, a few releases a page.
export RIVET_WELCOME_API="$BASE"
echo 3 > "$SRV/per_page"

# ---------------------------------------------------------------------------------------------
step "Install two apps"
pack notes 0.1.0 "$(fake notes 0.1.0)"; publish notes 0.1.0 stable
pack edit 0.1.0 "$(fake edit 0.1.0)"; publish edit 0.1.0 stable
pack terminal 0.1.0 "$(fake terminal 0.1.0)"; publish terminal 0.1.0 stable
rivet-welcome install notes edit
expect_version rivet-notes 0.1.0
expect_version rivet-edit 0.1.0
expect_entry rivet-notes rivet-notes
expect_entry rivet-edit rivet-edit
[ "$(count 'releases?per_page=3&page=2')" -ge 1 ] || fail "only the first page of releases was read"
ok "read every page of releases"

# ---------------------------------------------------------------------------------------------
step "Nothing new: asking again costs nothing (ETag)"
before=$(count ' 304 ')
rivet-welcome check
[ "$(count ' 304 ')" -gt "$before" ] || fail "GitHub was asked again without the ETag"
ok "the unchanged list came back 304"

# ---------------------------------------------------------------------------------------------
step "Update one app over a connection that drops halfway"
pack notes 0.2.0 "$(fake notes 0.2.0)"; publish notes 0.2.0 stable
touch "$SRV/flaky/rivet-notes-0.2.0-$ARCH-linux.tar.gz"
set +e; rivet-welcome check; code=$?; set -e
[ $code = 2 ] || fail "check should exit 2 with an update waiting (it said $code)"
rivet-welcome update
expect_version rivet-notes 0.2.0
expect_version rivet-edit 0.1.0
expect_entry rivet-notes rivet-notes
grep -q "rivet-notes-0.2.0-$ARCH-linux.tar.gz 200-dropped" "$SRV/log" || fail "the download wasn't dropped"
grep -q "rivet-notes-0.2.0-$ARCH-linux.tar.gz 206 bytes=" "$SRV/log" || fail "the second try didn't carry on from where it stopped"
ok "the dropped download carried on from where it stopped"

# ---------------------------------------------------------------------------------------------
step "Pre-releases are offered only when asked for"
pack notes 0.3.0-beta.1 "$(fake notes 0.3.0-beta.1)"; publish notes 0.3.0-beta.1 pre
rivet-welcome check || fail "a pre-release was offered without asking"
ok "stable only: up to date"
mkdir -p "$XDG_CONFIG_HOME/rivet-welcome"
echo prereleases=true > "$XDG_CONFIG_HOME/rivet-welcome/settings"
set +e; rivet-welcome check; code=$?; set -e
[ $code = 2 ] || fail "with pre-releases on, 0.3.0-beta.1 should be offered (check said $code)"
ok "with pre-releases on: 0.3.0-beta.1 is offered"
echo prereleases=false > "$XDG_CONFIG_HOME/rivet-welcome/settings"

# ---------------------------------------------------------------------------------------------
step "Edge: nightly builds by commit, then back to Stable"
EDGE1=0.3.1-edge.20261126.g1111111
EDGE2=0.3.1-edge.20261127.g2222222
pack notes "$EDGE1" "$(fake notes "$EDGE1")"; publish_edge notes "$EDGE1"
setting channel=edge
set +e; rivet-welcome check; code=$?; set -e
[ $code = 2 ] || fail "on Edge, the edge build should be offered (check said $code)"
rivet-welcome install notes
expect_version rivet-notes "$EDGE1"
expect_entry rivet-notes rivet-notes
# The next night's build: an update, told apart by its commit.
pack notes "$EDGE2" "$(fake notes "$EDGE2")"; publish_edge notes "$EDGE2"
rivet-welcome check | grep -q "Rivet Notes: edge build 1111111 from 2026-11-26 → edge build 2222222 from 2026-11-27" \
    || fail "check didn't offer the next edge build by its commit and day"
rivet-welcome install notes
expect_version rivet-notes "$EDGE2"
rivet-welcome install notes | grep -q "already installed" || fail "the same edge build was installed again"
ok "the same commit isn't installed twice"
# Rivet Edit has no edge build: Edge gives it its newest release.
expect_version rivet-edit 0.1.0
rivet-welcome list | grep "^edit " | grep -q "released 0.1.0$" || fail "Rivet Edit, with no edge build, wasn't offered its release"
ok "an app with no edge build follows its releases"
# Back to Stable: the release replaces the edge build, though its number is lower.
setting channel=stable
set +e; rivet-welcome check; code=$?; set -e
[ $code = 2 ] || fail "back on Stable, the release should replace the edge build (check said $code)"
rivet-welcome install notes
expect_version rivet-notes 0.2.0
expect_entry rivet-notes rivet-notes
rivet-welcome check || fail "after going back to Stable there's still something to update"
ok "back on Stable at 0.2.0, up to date"

# ---------------------------------------------------------------------------------------------
step "An installer that fails puts the previous version back"
cat > "$WORK/broken-install.sh" <<'EOF'
#!/bin/sh
set -e
cd "$(dirname "$0")"
install -Dm755 rivet-notes "${XDG_BIN_HOME:-$HOME/.local/bin}/rivet-notes"
rm -f "${XDG_DATA_HOME:-$HOME/.local/share}/applications/rivet-notes.desktop"
echo "couldn't add Rivet Notes to the launcher" >&2
exit 1
EOF
pack notes 0.3.0 "$(fake notes 0.3.0)" "$WORK/broken-install.sh"; publish notes 0.3.0 stable
expect_fail "Rivet Notes 0.2.0 is back as it was" rivet-welcome install notes
expect_version rivet-notes 0.2.0
expect_entry rivet-notes rivet-notes

# ---------------------------------------------------------------------------------------------
step "A download that doesn't match its checksum is refused"
pack edit 0.2.0 "$(fake edit 0.2.0)"
echo "0000000000000000000000000000000000000000000000000000000000000000  rivet-edit-0.2.0-$ARCH-linux.tar.gz" > "$SRV/dl/rivet-edit-0.2.0-$ARCH-linux.tar.gz.sha256"
publish edit 0.2.0 stable
expect_fail "doesn't match its checksum" rivet-welcome install edit
expect_version rivet-edit 0.1.0
[ -z "$(ls -A "$XDG_CACHE_HOME/rivet-welcome/downloads/edit" 2>/dev/null)" ] || fail "the damaged download was kept"
ok "the damaged download was thrown away"

step "A release with no checksum is refused"
pack edit 0.2.1 "$(fake edit 0.2.1)"; publish edit 0.2.1 stable no-sum
expect_fail "has no checksum" rivet-welcome install edit
expect_version rivet-edit 0.1.0

step "A release whose download is missing is refused"
pack edit 0.2.2 "$(fake edit 0.2.2)"; publish edit 0.2.2 stable
rm "$SRV/dl/rivet-edit-0.2.2-$ARCH-linux.tar.gz"
expect_fail "missing from the release" rivet-welcome install edit
expect_version rivet-edit 0.1.0

# ---------------------------------------------------------------------------------------------
step "GitHub's limit used up: said plainly, and the last list still works"
touch "$SRV/ratelimit"
expect_fail "limit on checks from this computer is used up" rivet-welcome check
rivet-welcome list | grep -q "^notes " || fail "list doesn't work from the cached releases"
ok "list still works from the last answer"
rm "$SRV/ratelimit"

# ---------------------------------------------------------------------------------------------
step "An app installed some other way is left alone"
fake terminal 0.0.9 > /dev/null
cp "$WORK/fake/rivet-terminal-0.0.9" "$SYS/rivet-terminal"
out=$(rivet-welcome install terminal 2>&1) || true
echo "$out" | sed 's/^/  | /'
echo "$out" | grep -q "not updated here" || fail "it didn't say why it left Rivet Terminal alone"
[ ! -e "$BIN/rivet-terminal" ] || fail "it installed Rivet Terminal over the other one"
ok "Rivet Terminal in $SYS is left alone"

# ---------------------------------------------------------------------------------------------
step "Two Rivet Welcomes don't install the same app at once"
mkdir -p "$XDG_CACHE_HOME/rivet-welcome/locks"
# Holds Rivet Edit's lock as another Rivet Welcome would, until it's stopped.
python3 -c 'import fcntl, sys, time; f = open(sys.argv[1], "w"); fcntl.flock(f, fcntl.LOCK_EX); time.sleep(30)'     "$XDG_CACHE_HOME/rivet-welcome/locks/edit.lock" &
LOCKER=$!
sleep 1
pack edit 0.3.0 "$(fake edit 0.3.0)"; publish edit 0.3.0 stable
expect_fail "another Rivet Welcome is installing" rivet-welcome install edit
kill $LOCKER 2>/dev/null || true
wait $LOCKER 2>/dev/null || true
rivet-welcome install edit
expect_version rivet-edit 0.3.0
expect_entry rivet-edit rivet-edit

# ---------------------------------------------------------------------------------------------
step "A folder that isn't writable is found before anything is downloaded"
if [ "$(id -u)" = 0 ]; then
    echo "  skipped: root can write anywhere"
else
    chmod a-w "$BIN"
    expect_fail "isn't yours to change" rivet-welcome install notes
    chmod u+w "$BIN"
    expect_version rivet-notes 0.2.0
fi

# ---------------------------------------------------------------------------------------------
step "Remove one app"
rivet-welcome remove edit
[ ! -e "$BIN/rivet-edit" ] || fail "rivet-edit is still in $BIN"
expect_no_entry rivet-edit
expect_version rivet-notes 0.2.0
expect_entry rivet-notes rivet-notes

step "Rivet Welcome removes itself"
rivet-welcome remove welcome
[ ! -e "$BIN/rivet-welcome" ] || fail "rivet-welcome is still in $BIN"
expect_no_entry rivet-welcome
[ ! -e "$XDG_CONFIG_HOME/autostart/rivet-welcome-tray.desktop" ] || fail "the tray still starts at sign-in"
ok "the tray no longer starts at sign-in"

printf '\nRivet Welcome %s passed every step (%s to %s).\n' "$NEWV" "$OLDV" "$NEWV"
