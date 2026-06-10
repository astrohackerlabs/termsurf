+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "pending"
+++

# Experiment 48: Phase D — hold live A/B recipe frames through capture

## Description

Experiment 47 made live A/B recipe delivery reliable by running recipes from
launch-time shell bootstrap files. The current captures now show each recipe
executing in both apps, but they also show a harness-induced mismatch after the
recipe finishes: Ghostty and Roastty return to different shell prompts/cwds (`~`
vs `termsurf`) and render different cursor/prompt state. That prompt difference
is not the recipe behavior under test, and it pollutes the screenshot diff
before we can reason about stricter visual thresholds.

This experiment keeps each recipe's final test frame active through capture.
Instead of letting the startup shell return to its prompt before screenshots are
taken, each recipe should sleep long enough after drawing the intended frame.
The harness still kills the launched app PID trees at the end, so the held shell
does not need to return naturally.

## Changes

- `scripts/roastty-app/live-ab-smoke.sh`
  - Add one configurable hold duration for live A/B recipes, defaulting to a
    value long enough for the current launch, sizing, and capture flow.
  - Replace per-recipe short sleeps with that hold duration.
  - Preserve recipe names, marker generation, launch-time bootstrap delivery,
    full-screen-plus-crop capture, JSON output, and exact launched-PID-tree
    cleanup.
  - Do not hide or crop out app chrome in this experiment; only remove
    post-recipe shell prompt/cwd noise.
- `scripts/roastty-app/README.md`
  - Document the held-frame behavior and the optional hold-duration environment
    variable.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, update Operating notes with the held-frame result and
    any improved diff metrics.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
  - `bash -n scripts/roastty-app/live-ab-matrix.sh`
- Run non-GUI recipe discovery:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
- Run representative live A/B recipes:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe smoke --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - `scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm both captures visibly contain the marker/expected rows and do not
    show the returned shell prompt.
  - Record the new diff metrics and compare them to the pre-Experiment-48
    permissive metrics from Experiment 47.
- Run the full default matrix:
  - `scripts/roastty-app/live-ab-matrix.sh`
  - Confirm it exits `0`, emits one JSON Lines object for every recipe, and
    every recipe's captures have direct execution evidence without returned
    prompt/cwd noise.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/48-live-ab-held-recipe-frame.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `find /tmp -maxdepth 1 -name 'termsurf-ab-bootstrap.*' -print` and verify
  no bootstrap temp dirs remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = representative and matrix recipes still execute in both apps, the
captured frames no longer include returned shell prompts, diff metrics improve
or the remaining differences are attributable to app/rendering rather than
post-recipe prompt/cwd noise, screenshots remain outside the repo, and no app
processes or bootstrap temp dirs remain.

**Partial** = held frames work for representative recipes but the full matrix is
blocked by local app/window/screen-recording conditions; record the exact
blocker and next command.

**Fail** = the harness cannot reliably capture before prompts return without a
larger recipe/capture redesign.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED.**

The reviewer found no Required issues. It noted one Optional concern: prompt
absence is still manually verified by visual inspection, which is acceptable for
this visual harness but may need an automated cue or OCR/check helper if prompt
contamination recurs. It also noted a documentation nit: the README update
should be unconditional because the design adds a configurable hold duration;
fixed before the plan commit.
