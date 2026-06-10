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

# Experiment 42: Phase D — clear-screen live A/B recipe

## Description

The live A/B harness now has recipes for smoke, ASCII text, and colors. The next
known conformance feature from the earlier smoke map is clear-screen behavior:
Experiment 20 found a `clear` gap and Experiment 22 fixed the post-clear
rendering path. Phase D should make that behavior repeatable in the Ghostty vs
Roastty visual harness.

This experiment adds a `clear-after` recipe. The recipe prints deterministic
pre-clear text, clears the screen, prints a timestamped post-clear marker, and
sleeps so the capture happens before the shell prompt returns. The expected
visual fixture is the post-clear state only. As with the other live recipes,
strict visual parity is recorded but not required yet.

## Changes

- `scripts/roastty-app/live-ab-smoke.sh`
  - Add `clear-after` to `--list-recipes`.
  - Add `--recipe clear-after`.
  - Update the `--help` / usage text to include `clear-after`.
  - The recipe command:
    - prints several pre-clear lines,
    - runs a terminal clear sequence,
    - prints a timestamped post-clear marker plus a few fixed rows,
    - sleeps before the shell prompt returns.
  - Include the existing `recipe` JSON field with value `clear-after`.
  - Preserve `smoke`, `ascii-grid`, and `color-grid`; screenshot policy;
    IOSurface-safe Roastty capture; `swift pngdiff.swift`; and exact
    launched-PID-tree cleanup.
- `scripts/roastty-app/README.md`
  - Document `clear-after`.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, record `clear-after` under Operating notes if the live
    run succeeds.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
- Run recipe discovery:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
  - Confirm it exits `0`, prints `smoke`, `ascii-grid`, `color-grid`, and
    `clear-after`, and does not launch either app.
- Run help:
  - `scripts/roastty-app/live-ab-smoke.sh --help`
  - Confirm it exits `0` and usage includes `clear-after`.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/42-live-ab-clear-recipe.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- If both debug apps are built, run the clear recipe with permissive thresholds:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe clear-after --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm the harness exits `0`, prints one JSON summary object, includes
    `"recipe":"clear-after"`, includes same-sized captures, and cleans up only
    the launched PID trees.
- Run the clear recipe with strict thresholds:
  - `bash -lc 'scripts/roastty-app/live-ab-smoke.sh --recipe clear-after; rc=$?; echo strict_exit=$rc; exit 0'`
  - Record the current strict verdict and metrics. Strict visual parity is not
    required for this experiment unless the current app state already achieves
    it.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = `clear-after` is discoverable, runs live through the A/B harness,
JSON identifies the recipe, screenshots stay outside the repo, strict metrics
are recorded without overclaiming parity, and launched app processes are cleaned
up.

**Partial** = the recipe is syntax-checked and documented, but a local
app-build, accessibility, screen-recording, or live-window condition prevents a
full live run; the blocker and next command are recorded.

**Fail** = the recipe makes the harness unreliable or cannot be added without a
larger rewrite.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED with no findings.**
