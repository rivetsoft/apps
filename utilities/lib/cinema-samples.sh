#!/bin/sh
# Make Rivet Cinema's sample media, for screenshots and trying it out, from ffmpeg's own test
# sources only: nothing recorded, nothing anyone owns.
#
#   utilities/lib/cinema-samples.sh [FOLDER]     (default: ~/.cache/rivet-cinema-samples)
#
# FOLDER/Mandelbrot Zoom.webm   2 minutes of a fractal zoom, two audio tracks (named English
#                               and Français), three chapters, and English subtitles beside it
# FOLDER/Cellular Life.webm     30 seconds of the Game of Life
# FOLDER/Music/                 three short pieces made of tones, by "Rivet Sample Band", two
#                               of them FLAC with a cover drawn here, one Opus with none
# Files already made are kept; delete the folder to make them again.
set -eu
OUT=${1:-${XDG_CACHE_HOME:-$HOME/.cache}/rivet-cinema-samples}
command -v ffmpeg >/dev/null || { echo "Install ffmpeg to make the samples." >&2; exit 1; }
mkdir -p "$OUT/Music"
T=$(mktemp -d)
trap 'rm -rf "$T"' EXIT
ff() { ffmpeg -v error -y -nostdin "$@"; }

V="$OUT/Mandelbrot Zoom.webm"
if [ ! -f "$V" ]; then
    cat > "$T/chapters.txt" <<'EOF'
;FFMETADATA1
title=Mandelbrot Zoom
[CHAPTER]
TIMEBASE=1/1000
START=0
END=40000
title=Wide
[CHAPTER]
TIMEBASE=1/1000
START=40000
END=85000
title=Closer
[CHAPTER]
TIMEBASE=1/1000
START=85000
END=120000
title=Deep
EOF
    ff -f lavfi -i "mandelbrot=size=640x360:rate=10:maxiter=400:start_scale=3:end_scale=0.01:end_pts=1200,scale=1280:720:flags=lanczos" \
        -f lavfi -i "aevalsrc=0.15*sin(2*PI*220*t)+0.1*sin(2*PI*330*t)*(0.6+0.4*sin(2*PI*0.25*t)):s=48000:d=120" \
        -f lavfi -i "aevalsrc=0.15*sin(2*PI*247*t)+0.1*sin(2*PI*370*t)*(0.6+0.4*sin(2*PI*0.2*t)):s=48000:d=120" \
        -i "$T/chapters.txt" -t 120 -map 0:v -map 1:a -map 2:a -map_metadata 3 -map_chapters 3 \
        -c:v libvpx-vp9 -deadline realtime -cpu-used 8 -row-mt 1 -b:v 2M -pix_fmt yuv420p -c:a libopus -b:a 96k \
        -metadata:s:a:0 language=eng -metadata:s:a:0 title=English \
        -metadata:s:a:1 language=fre -metadata:s:a:1 title=Français "$V"
    cat > "$OUT/Mandelbrot Zoom.en.srt" <<'EOF'
1
00:00:02,000 --> 00:00:06,000
Every edge of the set holds more of it.

2
00:00:40,000 --> 00:00:45,000
Closer now. The shapes repeat, never quite the same.

3
00:01:25,000 --> 00:01:31,000
This deep, the numbers need more care than the eye.
EOF
fi

L="$OUT/Cellular Life.webm"
[ -f "$L" ] || ff -f lavfi -i "life=size=320x180:rate=12:mold=10:ratio=0.1:death_color=#24121b:life_color=#d53f8c,scale=1280:720:flags=neighbor" \
    -f lavfi -i "aevalsrc=0.08*sin(2*PI*196*t)*lt(t-floor(t)\,0.15):s=48000:d=30" -t 30 \
    -c:v libvpx-vp9 -deadline realtime -cpu-used 8 -b:v 1M -pix_fmt yuv420p -c:a libopus -b:a 64k "$L"

# A cover: ripples over a corner-to-corner blend of two Rivet colors (red green blue, twice).
cover() {
    c() { echo "lerp($1\\,$2\\,(X+Y)/(W+H))*(0.82+0.18*sin(hypot(X-W*0.72\\,Y-H*0.28)/28))"; }
    ff -f lavfi -i "color=black:s=600x600,format=rgb24,geq=r='$(c "$1" "$4")':g='$(c "$2" "$5")':b='$(c "$3" "$6")'" -frames:v 1 "$7"
}
song() {
    # title, file, track number, seconds, expression, cover colors (or none)
    out="$OUT/Music/$2"
    [ -f "$out" ] && return 0
    if [ "$6" = none ]; then
        ff -f lavfi -i "aevalsrc=$5:s=48000:d=$4" -c:a libopus -b:a 96k \
            -metadata title="$1" -metadata artist="Rivet Sample Band" -metadata album="Tide Tables" -metadata track="$3" "$out"
    else
        # shellcheck disable=SC2086
        cover $6 "$T/cover.png"
        ff -f lavfi -i "aevalsrc=$5:s=48000:d=$4" -i "$T/cover.png" -map 0:a -map 1:v -c:a flac -c:v png -disposition:v attached_pic \
            -metadata title="$1" -metadata artist="Rivet Sample Band" -metadata album="Tide Tables" -metadata track="$3" "$out"
    fi
}
song "Low Tide" "01 Low Tide.flac" 1 154 "0.12*sin(2*PI*196*t)*(0.5+0.5*sin(2*PI*0.1*t))+0.08*sin(2*PI*294*t)+0.05*sin(2*PI*392*t)*lt(t-floor(t)\,0.3)" "213 63 140 0 132 184"
song "Harbor Lights" "02 Harbor Lights.flac" 2 96 "0.1*sin(2*PI*262*t)+0.08*sin(2*PI*330*t)*(0.5+0.5*sin(2*PI*0.5*t))+0.06*sin(2*PI*392*t)" "0 132 184 36 18 27"
song "Morning Bell" "03 Morning Bell.opus" 3 64 "0.2*sin(2*PI*523*t)*exp(-3*(t-2*floor(t/2)))" none
echo "$OUT"
