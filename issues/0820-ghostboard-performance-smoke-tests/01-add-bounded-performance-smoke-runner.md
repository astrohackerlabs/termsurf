# Experiment 1: Add Bounded Performance Smoke Runner

## Description

Issue 820 needs durable performance smoke coverage without turning ordinary
testing into a benchmark suite. Existing Ghostboard automation already proves
many functional paths through `scripts/ghostboard-geometry-matrix.sh`, and
`scripts/bounded-run.sh` already provides a no-hang wrapper. This experiment
will add a small performance smoke runner that reuses those pieces and records
wall-clock evidence for obvious regressions.

The runner should not claim stable microbenchmarks. It should catch hangs,
startup explosions, and major interaction regressions while keeping thresholds
generous enough for this macOS-on-macOS VM.

## Changes

Planned source/script changes:

- Add `scripts/ghostboard-performance-smoke.sh`.
  - Run each scenario through `scripts/bounded-run.sh`.
  - Write a timestamped summary log under `logs/`.
  - Record scenario name, bounded-run log path, status, exit code, and elapsed
    seconds.
  - Fail if any bounded run reports `HARD_TIMEOUT`, `IDLE_KILL`, a nonzero
    scenario exit code, or an elapsed time above that scenario's generous smoke
    ceiling.
  - Support a default fast profile and an explicit slower diagnostic profile.
  - Allow threshold tuning through environment variables, but keep defaults in
    the script so the smoke is runnable without extra setup.
- Fast profile coverage:
  - repeated browser startup by running `initial-open` three times;
  - one resize responsiveness row with `window-resize`;
  - one mouse/input hot-path proxy row with `mouse-after-geometry-change`.
- Diagnostic profile coverage:
  - all fast rows;
  - `terminal-scrollback-movement` for terminal scroll responsiveness;
  - `browser-input-granularity` for keyboard/mouse interaction density.

Planned issue-document changes:

- Add `## Result` and `## Conclusion` after verification.
- Update the Issue 820 README experiment status after verification.
- If the fast profile is too slow or flaky in this VM, record the exact failing
  scenario, log paths, and whether the next experiment should tune the runner or
  fix a specific app/harness problem.

Explicitly out of scope:

- App, Roamium, webtui, or protocol source changes.
- Precise FPS, frame-time, CPU, or memory benchmarking.
- Adding the smoke to CI or release scripts.
- Committing generated logs or screenshots.

## Verification

Formatting actions:

```bash
prettier --write --prose-wrap always --print-width 80 \
  issues/0820-ghostboard-performance-smoke-tests/README.md \
  issues/0820-ghostboard-performance-smoke-tests/01-add-bounded-performance-smoke-runner.md
```

Static checks:

```bash
bash -n scripts/ghostboard-performance-smoke.sh
git diff --check
```

Runtime checks:

```bash
scripts/ghostboard-performance-smoke.sh --fast
```

If the fast profile passes and there is enough time to run slower diagnostics:

```bash
scripts/ghostboard-performance-smoke.sh --diagnostic
```

Pass criteria:

- The runner exists, is executable, and passes `bash -n`.
- `--fast` runs three repeated startup attempts plus resize and mouse/input rows
  under bounded-run protection.
- The summary log records status, exit code, elapsed seconds, and per-scenario
  log paths.
- The fast profile either passes within the generous smoke ceilings or fails
  with a precise scenario/log path that identifies the next experiment.
- No generated logs or screenshots are staged.

Partial criteria:

- The runner is implemented and static checks pass, but one or more runtime rows
  fail due to an app or harness issue that needs a focused follow-up.
- The fast profile passes, but the diagnostic profile exposes a slower
  non-blocking issue that should become a later experiment.

Fail criteria:

- The runner cannot launch Ghostboard, webtui, or Roamium at all.
- The runner cannot distinguish bounded-run timeout, scenario failure, and smoke
  threshold failure.
- The runner's default profile is too slow for repeated local use.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, and commit
the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Goodall the 2nd`:

- **Verdict:** Approved.
- **Required findings:** None.
- **Evidence checked:** The reviewer confirmed the README links Experiment 1 as
  `Designed`, the experiment contains the required sections, the scope is
  narrow, `scripts/bounded-run.sh` emits distinguishable `HARD_TIMEOUT`,
  `IDLE_KILL`, and `COMPLETED rc=... elapsed=...` statuses, the planned geometry
  scenarios exist, and `git diff --check` passed.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 820 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
