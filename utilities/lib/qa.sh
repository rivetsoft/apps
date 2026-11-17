#!/bin/sh
# QA pass: open every dialog, page and mode looking for crashes, then drive the common file
# workflows with the keyboard and check the results on disk. Everything happens in a throwaway
# home folder; your own files, settings, Trash and clipboard are never touched.
set -u
cd "$(dirname "$0")/../.."
cargo build -q || exit 1
BIN="$(pwd)/target/debug/rivet"

T="$(mktemp -d /tmp/rivet-qa-XXXXXX)"
H="$T/home"
mkdir -p "$H/Files" "$T/shots"
for f in a.txt b.txt c.md; do : > "$H/Files/$f"; done
export HOME="$H" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache" XDG_STATE_HOME="$T/state"
export RIVET_THEME=dark

crashes=0
pass=0
fail=0

# ---------------------------------------------------------------- crashes
smoke() {
    name=$1; shift
    out=$(env "$@" "$BIN" "$H/Files" --screenshot "$T/shots/$name.png" </dev/null 2>&1)
    code=$?
    if [ $code -ne 0 ] || printf '%s' "$out" | grep -q panicked; then
        echo "CRASH $name (exit $code)"
        printf '%s\n' "$out" | grep -A3 panicked | head -5
        crashes=$((crashes + 1))
    else
        echo "ok    $name"
    fi
}
for d in about shortcuts openwith delete compress connect preview rename bulkrename convert resize newfolder newfile; do
    smoke "dialog-$d" RIVET_DIALOG=$d RIVET_SELECT=0
done
for t in views behavior menu modules system about; do smoke "prefs-$t" RIVET_PREFS_TAB=$t; done
smoke terminal RIVET_TERMINAL=1 RIVET_PEEK=1 RIVET_SELECT=0
smoke zen RIVET_ZEN=1 RIVET_ZEN_SHOW=sidebar,toolbar,peek
smoke split RIVET_SPLIT=1 RIVET_PEEK=1 RIVET_SELECT=1
smoke quick-open RIVET_SPOTLIGHT=fil
smoke quick-actions RIVET_ACTIONS=split
smoke vim RIVET_VIM=1 RIVET_KEYBOARD=1 RIVET_CLICKS="type:jjGggV@6;key:escape@9"
for r in 'mode=open\nmultiple=1' 'mode=save\nname=x.txt' 'mode=open\ndirectory=1' 'mode=savefiles\nfile=a.txt'; do
    out=$(RIVET_PICK_REQUEST="$r" "$BIN" --pick --screenshot "$T/shots/picker.png" </dev/null 2>&1)
    # A picker closed without a choice exits with 1.
    if [ $? -gt 1 ] || printf '%s' "$out" | grep -q panicked; then
        echo "CRASH picker ($r)"; crashes=$((crashes + 1))
    else
        echo "ok    picker $(printf '%s' "$r" | cut -d'\' -f1)"
    fi
done

# ---------------------------------------------------------------- workflows
check() { if eval "$2"; then echo "PASS  $1"; pass=$((pass + 1)); else echo "FAIL  $1"; fail=$((fail + 1)); fi; }
fresh() { d="$T/work/$1"; mkdir -p "$d"; for f in a.txt b.txt c.md; do : > "$d/$f"; done; echo "$d"; }
run() { dir=$1; shift; env RIVET_KEYBOARD=1 "$@" "$BIN" "$dir" --view list --screenshot "$T/shots/run.png" </dev/null >/dev/null 2>&1; }

D=$(fresh new);    run "$D" RIVET_CLICKS="key:ctrl+n@6;type:Alpha@10;key:enter@13"
check "new folder (Ctrl+N, type, Enter)" "[ -d '$D/Alpha' ]"
D=$(fresh rename); run "$D" RIVET_SELECT=0 RIVET_CLICKS="key:f2@6;type:renamed@10;key:enter@13"
check "rename (F2 keeps the extension)" "[ -f '$D/renamed.txt' ] && [ ! -f '$D/a.txt' ]"
D=$(fresh trash);  run "$D" RIVET_SELECT=1 RIVET_CLICKS="key:delete@6;key:enter@10"
check "Delete, Enter: moves to Trash" "[ ! -f '$D/b.txt' ] && ls '$XDG_DATA_HOME/Trash/files' | grep -q b"
D=$(fresh undo);   run "$D" RIVET_SELECT=1 RIVET_CLICKS="key:delete@6;key:enter@10;key:ctrl+z@30"
check "Ctrl+Z brings it back" "[ -f '$D/b.txt' ]"
D=$(fresh split);  mkdir -p "$D-other"; run "$D" RIVET_SPLIT="$D-other" RIVET_SELECT=2 RIVET_CLICKS="copy@6;key:f6@9;paste@12"
check "split view: copy, F6, paste" "[ -f '$D-other/c.md' ] && [ -f '$D/c.md' ]"
D=$(fresh zip);    run "$D" RIVET_SELECT=0 RIVET_DIALOG=compress RIVET_CLICKS="key:enter@8"
check "compress" "ls '$D' | grep -q 'a.*\.zip'"
D=$(fresh bulk);   run "$D" RIVET_SELECT=0,1 RIVET_DIALOG=bulkrename RIVET_CLICKS="type:x_@20;key:enter@24"
check "bulk rename" "[ -f '$D/x_a.txt' ] && [ -f '$D/x_b.txt' ]"
D=$(fresh nav);    mkdir -p "$D/sub/deeper"; run "$D/sub/deeper" RIVET_CLICKS="key:alt+ArrowUp@6;key:alt+ArrowUp@9;key:ctrl+n@12;type:Here@16;key:enter@19"
check "Alt+Up twice, new folder lands there" "[ -d '$D/Here' ]"

echo
echo "crashes: $crashes · workflows passed: $pass · failed: $fail"
case "$T" in /tmp/rivet-qa-*) rm -rf "$T" ;; esac
[ $crashes -eq 0 ] && [ $fail -eq 0 ]
