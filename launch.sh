#!/bin/sh
# Start splash-monitor against the Splash server that oMLX runs.
# The port and the cache folder are read from the running splash process.
#
# Usage: ./launch.sh [-d] [--no-log] [-- <monitor options>]
#   -d, --detach   run in a background screen session named splashmon
#   --no-log       do not write the CSV log
#
# You can override the defaults with these variables:
#   SPLASH_URL          full base URL, for example http://127.0.0.1:50502
#   SPLASH_API_KEY      bearer token, when your server needs one
#   SPLASH_CACHE_DIR    volume to measure for device-wide disk I/O
#   MONITOR_BIN         path to the binary
#   MONITOR_LOG         path to the CSV log

set -eu

REPO=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
BIN=${MONITOR_BIN:-$REPO/target/release/splash-monitor}
LOG=${MONITOR_LOG:-$REPO/telemetry.csv}

if [ ! -x "$BIN" ]; then
    echo "No binary at $BIN. Run: cargo build --release" >&2
    exit 1
fi

# Find the running "splash serve" line. oMLX picks a new port at each start.
SERVE=$(ps -Ao args= | grep -m1 '[s]plash serve --model' || true)

URL=${SPLASH_URL:-}
if [ -z "$URL" ] && [ -n "$SERVE" ]; then
    PORT=$(printf '%s' "$SERVE" | sed -nE 's/.* --port ([0-9]+).*/\1/p')
    [ -n "$PORT" ] && URL="http://127.0.0.1:$PORT"
fi
if [ -z "$URL" ]; then
    URL="http://127.0.0.1:18000"
    echo "No running splash server found. Using $URL" >&2
fi

CACHE=${SPLASH_CACHE_DIR:-}
if [ -z "$CACHE" ] && [ -n "$SERVE" ]; then
    CACHE=$(printf '%s' "$SERVE" | sed -nE 's/.* --cache-dir (.*)$/\1/p')
fi
if [ -n "$CACHE" ] && [ ! -d "$CACHE" ]; then
    echo "Cache volume is not mounted: $CACHE" >&2
    CACHE=
fi

detach=""
log=yes
extra=""
for arg in "$@"; do
    case $arg in
        -d | --detach) detach=yes ;;
        --no-log) log=no ;;
        -h | --help) sed -n '2,15p' "$0"; exit 0 ;;
        *) extra="$extra $arg" ;;
    esac
done

set --
if [ "$log" = yes ]; then set -- "$@" --log "$LOG"; fi
if [ -n "$CACHE" ]; then set -- "$@" --cache-dir "$CACHE"; fi
if [ -n "$extra" ]; then
    # shellcheck disable=SC2086  # word split of extra args is intended
    set -- "$@" $extra
fi

echo "url:       $URL"
echo "cache dir: ${CACHE:-none}"
echo "key:       ${SPLASH_API_KEY:+set}${SPLASH_API_KEY:-none}"
echo "log:       ${log}"

if [ "$detach" = yes ]; then
    if screen -ls 2>/dev/null | grep -q '\.splashmon'; then
        echo "splashmon is already running. Watch it with: screen -r splashmon" >&2
        exit 1
    fi
    screen -dmS splashmon env TERM=xterm-256color "$BIN" --url "$URL" "$@"
    echo "Running in the background. Watch it with: screen -r splashmon"
    exit 0
fi

exec "$BIN" --url "$URL" "$@"
