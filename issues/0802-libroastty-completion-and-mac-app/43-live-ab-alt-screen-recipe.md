+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex-adversarial"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 43: Phase D — alt-screen live A/B recipe

## Description

The live A/B harness now has recipes for text, colors, and clear-screen
behavior. The next feature from Experiment 20's conformance map is the alternate
screen plus cursor addressing: enter the alt screen, clear it, draw fixed text
at specific cursor positions, and capture while the command is sleeping.

This experiment adds an `alt-screen` recipe. It is self-terminating in the same
sense as the earlier Experiment 20 probe: the shell command enters the alternate
screen and sleeps so the harness can capture the alt-screen content; the harness
then kills the launched app PID trees, which tears down the sleeping command. As
with the other Phase-D recipes, strict visual parity is recorded but not
required yet.

## Changes

- `scripts/roastty-app/live-ab-smoke.sh`
  - Add `alt-screen` to `--list-recipes`.
  - Add `--recipe alt-screen`.
  - Update the `--help` / usage text to include `alt-screen`.
  - The recipe command:
    - enters alternate screen mode with `DECSET 1049`,
    - clears the screen,
    - prints a timestamped marker at a fixed row/column,
    - prints additional fixed text at at least two other cursor-addressed
      positions,
    - sleeps before the prompt returns so the capture sees the alt screen.
  - Include the existing `recipe` JSON field with value `alt-screen`.
  - Preserve `smoke`, `ascii-grid`, `color-grid`, and `clear-after`; screenshot
    policy; IOSurface-safe Roastty capture; `swift pngdiff.swift`; and exact
    launched-PID-tree cleanup.
- `scripts/roastty-app/README.md`
  - Document `alt-screen`.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, record `alt-screen` under Operating notes if the live
    run succeeds.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
- Run recipe discovery:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
  - Confirm it exits `0`, prints `smoke`, `ascii-grid`, `color-grid`,
    `clear-after`, and `alt-screen`, and does not launch either app.
- Run help:
  - `scripts/roastty-app/live-ab-smoke.sh --help`
  - Confirm it exits `0` and usage includes `alt-screen`.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/43-live-ab-alt-screen-recipe.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- If both debug apps are built, run the alt-screen recipe with permissive
  thresholds:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe alt-screen --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm the harness exits `0`, prints one JSON summary object, includes
    `"recipe":"alt-screen"`, includes same-sized captures, and cleans up only
    the launched PID trees.
- Run the alt-screen recipe with strict thresholds:
  - `bash -lc 'scripts/roastty-app/live-ab-smoke.sh --recipe alt-screen; rc=$?; echo strict_exit=$rc; exit 0'`
  - Record the current strict verdict and metrics. Strict visual parity is not
    required for this experiment unless the current app state already achieves
    it.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = `alt-screen` is discoverable, runs live through the A/B harness, JSON
identifies the recipe, screenshots stay outside the repo, strict metrics are
recorded without overclaiming parity, and launched app processes are cleaned up.

**Partial** = the recipe is syntax-checked and documented, but a local
app-build, accessibility, screen-recording, or live-window condition prevents a
full live run; the blocker and next command are recorded.

**Fail** = the recipe makes the harness unreliable or cannot be added without a
larger rewrite.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED with no findings.**
