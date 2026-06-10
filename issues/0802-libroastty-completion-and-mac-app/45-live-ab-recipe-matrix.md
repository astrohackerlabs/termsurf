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

# Experiment 45: Phase D — live A/B recipe matrix runner

## Description

Experiments 40–44 added individual live A/B recipes, but Phase D still needs a
repeatable in-session run wired so later work can regression-test the current
feature surface without manually invoking each recipe. This experiment adds a
small matrix runner over the existing `live-ab-smoke.sh` recipes.

The runner should execute a selected set of recipes, keep the permissive
thresholds configurable, print one machine-readable JSON Lines summary per
recipe, and exit nonzero if any selected recipe fails under the supplied
thresholds. It should not introduce new screenshot storage rules or new visual
judgment logic; it composes the existing harness and `pngdiff.swift` outputs.
Strict visual parity remains a separate per-recipe metric, not a pass
requirement for this matrix runner.

## Changes

- `scripts/roastty-app/live-ab-matrix.sh`
  - Add a Bash runner around `scripts/roastty-app/live-ab-smoke.sh`.
  - Default to running every recipe reported by
    `live-ab-smoke.sh --list-recipes`.
  - Support selecting a subset with repeated `--recipe <name>`.
  - Support threshold passthrough:
    - `--max-mismatch-ratio <N>`
    - `--max-mean-channel-delta <N>`
  - Default thresholds should be permissive (`1` and `255`) so the matrix proves
    harness mechanics and current coverage rather than strict parity.
  - For each recipe, run `live-ab-smoke.sh`, capture its single JSON summary,
    and print one JSON Lines object containing at least:
    - `recipe`,
    - `status` (`PASS` / `FAIL`),
    - child exit status,
    - nested harness JSON summary.
  - Continue running remaining recipes after a recipe fails, then exit nonzero
    if any recipe failed.
  - Preserve the existing screenshot policy: no screenshots or generated
    artifacts in the repo.
- `scripts/roastty-app/README.md`
  - Document the matrix runner and a one-recipe smoke invocation.
- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Add this experiment to the index as `Designed`.
  - After implementation, record the matrix command under Operating notes if the
    live run succeeds.

## Verification

- Run shell syntax checks:
  - `bash -n scripts/roastty-app/live-ab-matrix.sh`
  - `bash -n scripts/roastty-app/live-ab-smoke.sh`
- Run a non-GUI recipe discovery check:
  - `scripts/roastty-app/live-ab-smoke.sh --list-recipes`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/45-live-ab-recipe-matrix.md scripts/roastty-app/README.md`
- Run `git diff --check`.
- If both debug apps are built, run a one-recipe matrix smoke:
  - `scripts/roastty-app/live-ab-matrix.sh --recipe smoke`
  - Confirm it exits `0`, prints one JSON Lines object, includes
    `"recipe":"smoke"`, includes nested harness JSON, and cleans up only the
    launched PID trees.
- If the one-recipe smoke passes, run a two-recipe matrix:
  - `scripts/roastty-app/live-ab-matrix.sh --recipe ascii-grid --recipe clear-after`
  - Confirm it exits `0` with permissive defaults and prints exactly two JSON
    Lines objects.
- Run an intentional failure aggregation check with strict thresholds:
  - `bash -lc 'scripts/roastty-app/live-ab-matrix.sh --recipe ascii-grid --recipe clear-after --max-mismatch-ratio 0 --max-mean-channel-delta 0; rc=$?; echo matrix_exit=$rc; exit 0'`
  - Confirm the matrix prints one JSON Lines object for `ascii-grid` and one for
    `clear-after`, at least one has `status:"FAIL"`, the later recipe still ran
    after the first failure, and the wrapper prints a nonzero `matrix_exit`.
- Run
  `pgrep -fl '[G]hostty.app/Contents/MacOS/ghostty|[R]oastty.app/Contents/MacOS/roastty' || true`
  and verify no launched app processes remain.
- Run `git status --short` and verify no screenshots or generated artifacts are
  in the repo.

**Pass** = the matrix runner can execute selected recipes, emit JSON Lines
summaries, continue after failures, report aggregate failure by exit status,
preserve screenshot hygiene, and leave no app processes running.

**Partial** = the runner is syntax-checked and documented, but a local
app-build, accessibility, screen-recording, or live-window condition prevents a
full live run; the blocker and next command are recorded.

**Fail** = composing recipes into a reliable runner requires a larger harness
rewrite.

## Design Review

**Reviewer:** Codex-native adversarial subagent (`multi_agent_v1.spawn_agent`,
fresh context, read-only). **Verdict: APPROVED after fixes.**

The first review returned `CHANGES REQUIRED` with one Required finding: the
design promised continue-after-failure and aggregate nonzero exit behavior, but
only verified permissive passing runs. Fixed by adding an intentional
strict-threshold failure aggregation check that must emit JSON Lines for both
selected recipes, show at least one `FAIL`, prove the later recipe still ran,
and report nonzero `matrix_exit`.

The focused re-review approved the fix and found no new Required issues.
