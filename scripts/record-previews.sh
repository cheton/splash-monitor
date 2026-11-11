#!/usr/bin/env bash
# Record the README preview from a live dashboard session.
#
#   ./scripts/record-previews.sh gif   # record footage, then cut previews/demo.gif
#   ./scripts/record-previews.sh cut   # re-cut previews/demo.gif from existing footage
#
# Requirements: cargo (gif mode), vhs (gif mode: brew install vhs), ffmpeg, and a
# live Splash server at 127.0.0.1:18000. Raw footage lands in tmp/ and is never
# committed, so a different edit is a filter change, not a new recording.
#
# Tunables (environment): WARMUP, CUT_AT, SPEEDUP, GIF_WIDTH, GIF_FPS,
# FAST_FPS. Recorded length is the Sleep line in previews/demo.tape.
set -euo pipefail

WARMUP=${WARMUP:-70}              # footage seconds dropped before the GIF opens
CUT_AT=${CUT_AT:-45}              # seconds of footage kept at 1x speed
SPEEDUP=${SPEEDUP:-9}             # the rest of the footage plays this much faster
GIF_WIDTH=${GIF_WIDTH:-1080}
GIF_FPS=${GIF_FPS:-10}
FAST_FPS=${FAST_FPS:-8}

cd "$(dirname "$0")/.."
mkdir -p tmp

FOOTAGE=tmp/demo.raw.mp4

# NO_COLOR and TERM=dumb in the calling shell would record a colorless frame.
record() {
  cargo build --release
  nc -z 127.0.0.1 18000 || { echo "Splash is not reachable at 127.0.0.1:18000" >&2; exit 1; }
  env -u NO_COLOR TERM=xterm-256color vhs previews/demo.tape >/dev/null
}

# Storyboard cut: skip the warm-up (rates need two samples, histories start
# empty), hold a readable opening, then compress the rest so the graphs show
# minutes of progress.
cut() {
  [ -f "$FOOTAGE" ] || { echo "no footage: run '$0 gif' first" >&2; exit 1; }
  local start=$WARMUP
  # -nostdin: ffmpeg otherwise reads the terminal for interactive keys, which
  # blocks when the script runs with an open pipe (no TTY to close).
  # Trim before splitting: two branches that each trim the whole stream make
  # ffmpeg decode the footage twice and buffer both copies.
  ffmpeg -nostdin -y -loglevel error -i "$FOOTAGE" -filter_complex \
    "[0:v]trim=start=${start},setpts=PTS-STARTPTS,split=2[a][b];
     [a]trim=0:${CUT_AT},setpts=PTS-STARTPTS,scale=${GIF_WIDTH}:-2,fps=${GIF_FPS}[n];
     [b]trim=${CUT_AT},setpts=(PTS-STARTPTS)/${SPEEDUP},scale=${GIF_WIDTH}:-2,fps=${FAST_FPS}[f];
     [n][f]concat=n=2:v=1[o];
     [o]split=2[po][pv];
     [po]palettegen=max_colors=256:stats_mode=full[pal];
     [pv][pal]paletteuse=dither=none" \
    previews/demo.gif
  echo "previews/demo.gif  $(du -h previews/demo.gif | cut -f1)"
}

case "${1:-gif}" in
  gif) record && cut ;;
  cut) cut ;;
  *) echo "usage: $0 [gif|cut]" >&2; exit 2 ;;
esac
