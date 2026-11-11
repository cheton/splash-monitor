---
name: record-splash-previews
description: Re-record the README previews (demo GIF, compact and full PNGs) for splash-monitor from raw vhs footage, or re-cut the GIF without re-recording
---

# Record Splash Monitor previews

Use when `previews/` is stale: after a layout change, a palette change, or a
rename that changes the on-screen strings.

## One command

```bash
./scripts/record-previews.sh gif      # record footage, then cut previews/demo.gif
./scripts/record-previews.sh cut      # re-cut previews/demo.gif from existing footage
```

Prerequisites: `cargo`, `vhs` (`brew install vhs`), `ffmpeg`, and a live Splash
server at `127.0.0.1:18000`. Check the port first — `nc -z 127.0.0.1 18000` —
otherwise the footage shows `● OFFLINE` and the graphs stay empty.

## Two-stage rule: record once, cut later

Raw footage is the asset. It stays in `tmp/` and is never committed:
`tmp/demo.raw.mp4`, recorded by `previews/demo.tape` (150×46, `--interval 1`,
`Sleep 240s` — the tape controls the recorded length).

`previews/demo.gif` is a storyboard cut of the raw footage: drop the first
`WARMUP` seconds, hold `CUT_AT` seconds at 1x so a reader can read the panels,
then speed up the remainder by `SPEEDUP` so the Braille graphs show minutes of
progress.

The opening must already contain data. A rate needs two samples and the histories
start empty, so a cut from second 0 opens on `Decode -- tok/s` and
`Waiting for samples...`. `WARMUP=70` is the point where the inference and cache
panels are visibly active; histories saturate at 120 samples, so an opening that
starts at 120 already shows full-width graphs.

A new edit is a filter change on the existing mp4, never a new recording:

```bash
./scripts/record-previews.sh cut                    # default cut of existing footage
WARMUP=120 CUT_AT=30 ./scripts/record-previews.sh cut   # open on saturated graphs
GIF_WIDTH=1200 GIF_FPS=12 SPEEDUP=6 ./scripts/record-previews.sh cut
```

The same filter, if you want to run it by hand (trim first, then split — see
Traps):

```bash
ffmpeg -nostdin -y -i tmp/demo.raw.mp4 -filter_complex \
  "[0:v]trim=start=70,setpts=PTS-STARTPTS,split=2[a][b];
   [a]trim=0:45,setpts=PTS-STARTPTS,scale=1080:-2,fps=10[n];
   [b]trim=45,setpts=(PTS-STARTPTS)/9,scale=1080:-2,fps=8[f];
   [n][f]concat=n=2:v=1[o];
   [o]split=2[po][pv];
   [po]palettegen=max_colors=256:stats_mode=full[pal];
   [pv][pal]paletteuse=dither=none" tmp/demo.cut.gif
```

## Activity rule

Recording only proves the panels work. If Splash is idle during the capture,
`INFERENCE / THROUGHPUT` and `SPLASH CACHE / I/O` read `0.0` and
`Idle - no activity in observed samples`, which looks broken in a preview. Start
the capture while the model is actually serving requests, or record while normal
work is happening on the machine, and check a frame from the middle of the
footage before cutting:

```bash
ffmpeg -y -ss 80 -i tmp/demo.raw.mp4 -frames:v 1 tmp/raw_80.png   # then read the image
```

## Traps

- `NO_COLOR=1` or `TERM=dumb` in the calling shell records a **colorless**
  frame. The script runs `vhs` through `env -u NO_COLOR TERM=xterm-256color`;
  keep that if you rewrite it.
- `Wait /…/` in a tape never matches: the dashboard repaints by cursor
  addressing and parks the cursor on the last row. Use `Sleep`.
- `vhs` rejects a tape that sets both `Width` and `Columns`; set `Columns` and
  `Rows` only.
- GIF holds 256 colors: use `palettegen=max_colors=256` with `dither=none`, or
  panel hues band.
- `split` in front of two `trim`s makes ffmpeg decode the whole source twice and
  buffer both copies: on a busy machine a 240-second cut stalls for minutes and
  can leave a truncated file (`Error writing trailer: Invalid argument`). Trim
  once first, then split — the same cut finishes in seconds.
- Run ffmpeg with `-nostdin`. An interactive terminal's stdin blocks ffmpeg, and
  under `set -e` the key-read line then aborts the script before encoding.
  macOS has no `timeout`, so never guard the cut with it.
- A killed ffmpeg leaves an orphan writer on `previews/demo.gif`. Check
  `pgrep -fl ffmpeg` before re-cutting, and `git checkout -- previews/demo.gif`
  if the file reads `End of file`.
- Keep the GIF under 10 MB (GitHub rejects larger files). `GIF_WIDTH=1080`,
  `GIF_FPS=10`, `FAST_FPS=8` gives ~7 MB for 4 minutes of footage.

## Verify before committing

```bash
stat -f "%z bytes" previews/demo.gif
ffprobe -v error -select_streams v:0 -show_entries stream=width,height,nb_frames \
  -of default=noprint_wrappers=1 previews/demo.gif
ffmpeg -y -i previews/demo.gif -vf "select=eq(n\,1)" -vframes 1 tmp/gif_open.png
ffmpeg -y -i previews/demo.gif -vf "select=eq(n\,300)" -vframes 1 tmp/gif_mid.png
```

Read both frames. The **opening** frame must already show live values — not
`Decode -- tok/s`, not `Waiting for samples...`. Colors present, `C0`…`C{N}`
core rows filled, graphs spanning the full panel width, no label clipped
mid-word. Commit `previews/` only after that check; `tmp/` stays untracked.
