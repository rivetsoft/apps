#!/bin/sh
# Regenerate Rivet FTP's screenshots (docs/screenshots/ftp-*.png) without a real server or
# network: a throwaway home with saved connections, and a demo "server" — a folder served by the
# local sftp-server program (RIVET_FTP_DEMO_SERVER), which stands for the server's "/".
set -e
cd "$(dirname "$0")/../.."
cargo build -q -p rivet-ftp
OUT="docs/screenshots"
mkdir -p "$OUT"

T="$(mktemp -d /tmp/rivet-ftp-docs-XXXXXX)"
H="$T/home"
S="$T/server"
SITE="$S/var/www/example.com"
mkdir -p "$H/Sites/example.com/assets" "$H/Sites/example.com/blog" "$H/.ssh" \
    "$SITE/assets/img" "$SITE/blog" "$SITE/cgi-bin" "$S/home/me"

page() { printf '<!doctype html>\n<title>%s</title>\n<h1>%s</h1>\n' "$2" "$2" > "$1"; }
for d in "$SITE" "$H/Sites/example.com"; do
    page "$d/index.html" "Example"
    page "$d/about.html" "About us"
    page "$d/contact.html" "Contact"
    printf 'body { font: 16px/1.5 sans-serif; color: #223; }\n' > "$d/assets/style.css"
    printf 'User-agent: *\nAllow: /\n' > "$d/robots.txt"
done
page "$SITE/blog/hello-world.html" "Hello, world"
page "$H/Sites/example.com/blog/hello-world.html" "Hello, world"
page "$H/Sites/example.com/blog/second-post.html" "A second post"
page "$H/Sites/example.com/pricing.html" "Pricing"
printf '#!/bin/sh\necho ok\n' > "$SITE/cgi-bin/status.sh"
head -c 48000 /dev/zero > "$SITE/assets/img/hero.jpg"
head -c 12000 /dev/zero > "$SITE/favicon.ico"
printf 'deploy: main\n' > "$H/Sites/example.com/.rivet-deploy"
# Some files a few days old on the server, so dates differ.
touch -d '-3 days' "$SITE"/*.html "$SITE/robots.txt"
touch -d '-9 days' "$SITE/cgi-bin/status.sh"
# A key to show on the SSH Keys page (made here, used nowhere).
ssh-keygen -q -t ed25519 -N '' -C "me@laptop" -f "$H/.ssh/id_ed25519"

export HOME="$H" XDG_CONFIG_HOME="$T/config" XDG_DATA_HOME="$T/data" XDG_CACHE_HOME="$T/cache"
BIN=./target/debug/rivet-ftp
"$BIN" add "Example site" sftp://me@example.com/var/www/example.com --key id_ed25519 --local "$H/Sites/example.com" --category Clients >/dev/null
"$BIN" add "Staging" sftp://deploy@staging.example.com:2222/var/www --category Clients >/dev/null
"$BIN" add "Home server" me@192.168.1.20 --category Home >/dev/null
"$BIN" add "Raspberry Pi" pi@raspberrypi.local --category Home >/dev/null
"$BIN" add "Public mirror" ftps://files.example.org/pub >/dev/null

export RIVET_SHOT_SIZE=1280x800 RIVET_FTP_DEMO_SERVER="$S"
shot() { name=$1; shift; env "$@" "$BIN" open "Example site" --screenshot "$OUT/ftp-$name-wide.png"; }

shot connected RIVET_FTP_THEME=dark "RIVET_FTP_SEND=up:pricing.html;down:favicon.ico,cgi-bin"
shot light RIVET_FTP_THEME=light
shot info RIVET_FTP_THEME=dark RIVET_FTP_INFO=index.html
shot preferences RIVET_FTP_THEME=dark RIVET_FTP_PREFS=transfers
shot keys RIVET_FTP_THEME=light RIVET_FTP_PREFS=keys
env RIVET_FTP_THEME=dark RIVET_FTP_FORM=1 "$BIN" --screenshot "$OUT/ftp-new-connection-wide.png"
# Sending files the server already has: Rivet FTP compares them first.
shot replace RIVET_FTP_THEME=dark "RIVET_FTP_SEND=up:about.html,blog"
echo "Saved ftp-*-wide.png in $OUT"
