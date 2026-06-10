# Roastty App Automation Helpers

Helpers in this directory drive or inspect the copied, renamed Roastty macOS app
for Issue 802 experiments.

## Screenshot Policy

Screenshots are never committed. The screenshot wrapper writes captures outside
the repo to `${TERMSURF_SHOT_DIR:-$HOME/.cache/termsurf/shots}` and prints the
PNG path. Keep retained images outside the working tree.

## PNG Diff

`pngdiff.swift` compares two PNG captures and writes one JSON object to stdout.
Diagnostics and usage errors go to stderr.

```bash
swift scripts/roastty-app/pngdiff.swift expected.png actual.png
swift scripts/roastty-app/pngdiff.swift expected.png actual.png \
  --max-mismatch-ratio 0.01 \
  --max-mean-channel-delta 2.0
```

The helper exits `0` when the metrics are within the supplied thresholds and
nonzero on threshold failure, dimension mismatch, invalid input, or unreadable
images.

## Live A/B Smoke

`live-ab-smoke.sh` launches the debug Ghostty and Roastty app binaries directly
with per-run shell bootstrap config, captures both apps through the
IOSurface-safe full-screen-plus-crop path, and compares the captures with
`pngdiff.swift`.

```bash
scripts/roastty-app/live-ab-smoke.sh \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255

scripts/roastty-app/live-ab-smoke.sh --list-recipes
scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255
scripts/roastty-app/live-ab-smoke.sh --recipe color-grid \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255
scripts/roastty-app/live-ab-smoke.sh --recipe clear-after \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255
scripts/roastty-app/live-ab-smoke.sh --recipe alt-screen \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255
scripts/roastty-app/live-ab-smoke.sh --recipe scroll-output \
  --max-mismatch-ratio 1 \
  --max-mean-channel-delta 255
```

The script prints one JSON summary object to stdout and diagnostics to stderr.
It traps cleanup by killing only the launched Ghostty/Roastty PID trees after
expected debug app path verification. Run `scripts/ghostty-app/stop-app.sh` and
`scripts/roastty-app/stop-app.sh` after manual debugging if a run is interrupted
externally.

Recipe commands run from a per-run `ZDOTDIR` bootstrap. The harness launches
each app binary directly with generated zsh and Nushell startup files, so
recipes execute at shell startup instead of relying on paste or synthetic UI
typing. Recipe payloads use data arguments rather than `printf` format strings
so literal `%`, backslashes, and ANSI escapes do not corrupt the shell command.

Recipes:

- `smoke` — default. Clears the terminal and prints one timestamped marker.
- `ascii-grid` — clears the terminal, prints a timestamped marker plus fixed
  ASCII rows, and sleeps so capture happens before the prompt returns.
- `color-grid` — clears the terminal, prints ANSI palette rows, bold/bright
  rows, truecolor samples, and sleeps so capture happens before the prompt
  returns.
- `clear-after` — prints pre-clear rows, runs the full clear sequence, prints
  post-clear rows, and sleeps so capture happens before the prompt returns.
- `alt-screen` — enters alternate screen mode, draws fixed text at cursor
  addressed positions, and sleeps so capture happens while the alt screen is
  active.
- `scroll-output` — clears the terminal, prints a timestamped marker plus 80
  numbered rows, and sleeps so capture happens after the viewport scrolls to the
  bottom.

## Live A/B Matrix

`live-ab-matrix.sh` runs one or more recipes through `live-ab-smoke.sh` and
prints one JSON Lines summary per recipe.

```bash
scripts/roastty-app/live-ab-matrix.sh --recipe smoke
scripts/roastty-app/live-ab-matrix.sh \
  --recipe ascii-grid \
  --recipe clear-after
```

If no `--recipe` is supplied, it runs every recipe reported by
`live-ab-smoke.sh --list-recipes`. Defaults are permissive
(`--max-mismatch-ratio 1 --max-mean-channel-delta 255`); pass strict thresholds
when intentionally recording current visual mismatches.
