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

# Experiment 40: Phase D — named live A/B recipes

## Description

Experiment 39 proved the mechanics of launching Ghostty and Roastty, driving one
shared marker command, capturing both windows, and diffing the images. The next
Phase-D step is to turn that one-off smoke command into named, repeatable
recipes that can grow into the feature-by-feature conformance matrix.

This experiment adds a small recipe layer to `live-ab-smoke.sh`, starting with
the existing `smoke` recipe and one deterministic `ascii-grid` recipe. The new
recipe should render stable ASCII content while the shell command is still
sleeping, so captures do not include a returned shell prompt as part of the
oracle. The recipe layer must stay conservative: it should not claim visual
parity while strict diffs still fail, and it should not try to solve all Phase-D
feature coverage in one step.

## Changes

- `scripts/roastty-app/live-ab-smoke.sh`
  - Add `--recipe <name>` with at least:
    - `smoke`: the existing `clear; echo ISSUE802_AB_SMOKE_<timestamp>` command.
    - `ascii-grid`: a deterministic ASCII command that clears the screen, prints
      a recipe marker plus several fixed rows of letters, digits, and
      punctuation, then sleeps long enough for the harness to capture before the
      prompt returns.
  - Add `--list-recipes` so future experiments can discover supported recipes
    without reading the script.
  - Include `recipe` in the JSON summary.
  - Keep the default recipe as `smoke` so Experiment 39 behavior stays
    compatible.
  - Keep screenshots outside the repo, retain the IOSurface-safe Roastty
    full-screen-plus-crop path, invoke `pngdiff.swift` through `swift`, and keep
    exact launched-PID-tree cleanup.
- `scripts/roastty-app/README.md`
  - Document `--recipe`, `--list-recipes`, and the initial recipes.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, record the recipe usage under Operating notes if the
    live run succeeds.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
- Run recipe discovery:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
  - Confirm it exits `0`, prints the supported recipe names, and does not launch
    either app.
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/40-live-ab-recipes.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- If both debug apps are built, run the default smoke recipe with no `--recipe`
  argument:
  - `scripts/roastty-app/live-ab-smoke.sh --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm the harness exits `0`, prints one JSON summary object, includes
    `"recipe":"smoke"`, and preserves Experiment 39's default behavior.
- If both debug apps are built, run the ASCII recipe with permissive thresholds:
  - `scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid --max-mismatch-ratio 1 --max-mean-channel-delta 255`
  - Confirm the harness exits `0`, prints one JSON summary object, includes
    `"recipe":"ascii-grid"`, includes same-sized captures, and cleans up only
    the launched PID trees.
- Run the ASCII recipe with strict thresholds:
  - `bash -lc 'scripts/roastty-app/live-ab-smoke.sh --recipe ascii-grid; rc=$?; echo strict_exit=$rc; exit 0'`
  - Record the current strict verdict and metrics. Strict visual parity is not
    required for this experiment unless the current app state already achieves
    it.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = named recipes work without regressing the default smoke behavior,
`ascii-grid` can run live through the A/B harness, JSON identifies the recipe,
screenshots remain outside the repo, and launched app processes are cleaned up.

**Partial** = the recipe layer is syntax-checked and documented, but a local
app-build, accessibility, screen-recording, or live-window condition prevents a
full live run; the blocker and next command are recorded.

**Fail** = the recipe layer makes the harness unreliable or cannot be added
without a larger rewrite.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED after fixes.**

The first review returned `CHANGES REQUIRED` with one Required finding: the
design promised default `smoke` compatibility but only verified the new
`ascii-grid` recipe. Fixed by adding a permissive live run with no `--recipe`
argument and requiring the JSON summary to include `"recipe":"smoke"`.

The focused re-review approved the fix and found no new Required issues.
