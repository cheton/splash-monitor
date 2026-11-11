# Scripts

## record-previews.sh

Re-records `previews/demo.gif` from a live dashboard session.

```text
./scripts/record-previews.sh gif   # record footage, then cut the GIF
./scripts/record-previews.sh cut   # re-cut the GIF from footage already in tmp/
```

Needs `ffmpeg`, and for `gif` mode also `cargo`, `vhs` (`brew install vhs`) and a
live Splash server at `127.0.0.1:18000` — the script checks the port and stops
instead of recording an `● OFFLINE` session. Run it from any directory; it
changes to the repository root.

### Footage is the asset

`gif` mode runs `previews/demo.tape` and writes `tmp/demo.raw.mp4` (150×46,
`--interval 1`, `Sleep 240s` — edit that line for a longer session). `tmp/` is
gitignored, so the footage stays local between runs and `cut` mode re-uses it:
an edit to the preview is a filter change, never a new 4-minute recording.

### The cut

| Variable | Default | Meaning |
|:--|:--|:--|
| `WARMUP` | `70` | footage seconds dropped before the GIF opens |
| `CUT_AT` | `45` | footage seconds played at 1x |
| `SPEEDUP` | `9` | speed applied to the remaining footage |
| `GIF_WIDTH` | `1080` | output width; keeps the file under GitHub's 10 MB limit |
| `GIF_FPS` | `10` | frame rate of the 1x opening |
| `FAST_FPS` | `8` | frame rate of the sped-up tail |

`WARMUP` exists because a preview that opens at second 0 opens empty: a rate
needs two samples and the histories start blank, so the first frames read
`Decode -- tok/s` and `Waiting for samples...`. At 70 s the inference and cache
panels are already live; histories saturate at 120 samples, so `WARMUP=120`
opens with every graph at full width.

```text
WARMUP=120 CUT_AT=25 ./scripts/record-previews.sh cut   # open on saturated graphs
GIF_WIDTH=1200 GIF_FPS=12 SPEEDUP=6 ./scripts/record-previews.sh cut
```

Check the result before committing — the opening frame in particular:

```text
ffmpeg -y -i previews/demo.gif -vf "select=eq(n\,1)" -vframes 1 tmp/gif_open.png
stat -f "%z bytes" previews/demo.gif
```

### Traps already handled

- `NO_COLOR=1` or `TERM=dumb` in your shell records a colorless frame, so `vhs`
  runs under `env -u NO_COLOR TERM=xterm-256color`.
- `Wait /…/` never matches in a tape: the dashboard repaints by cursor addressing
  and parks the cursor on the last row, so the tape uses `Sleep`.
- `vhs` rejects a tape that sets both `Width` and `Columns`; the tape sets
  `Columns` and `Rows`.
- A GIF holds 256 colors, so the palette step uses `max_colors=256` with
  `dither=none`; 32 colors bands the panel hues.
