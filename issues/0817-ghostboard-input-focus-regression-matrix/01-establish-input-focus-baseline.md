# Experiment 1: Establish Input/Focus Baseline Matrix

## Description

Issue 817 needs a focused input/focus regression matrix before adding new
automation or fixing app code. Current evidence is spread across Issue 809
geometry scenarios, Issue 811 cursor feedback, Issue 812 GUI active state, and
Issue 816 browser-state/copy behavior. This experiment will turn that scattered
evidence into an explicit baseline for Issue 817 and run a small representative
set of existing scenarios to prove the current automation still works.

The experiment is intentionally evidence-first. It should classify each
requested input/focus behavior as:

- `Covered`: current issue docs plus a current rerun prove it well enough for
  the fast regression matrix;
- `Partially covered`: current evidence exists, but the behavior needs a focused
  follow-up or a slower/manual bucket;
- `Uncovered`: no useful current Ghostboard runtime proof exists;
- `Blocked`: automation cannot test the behavior yet and the blocker is
  concrete.

## Changes

Planned issue-document changes:

- Add an Issue 817 baseline matrix section recording each requested behavior:
  keyboard text input and special keys, Cmd/menu shortcuts, clipboard behavior,
  mode transitions, focus stealing and pane focus, inactive visual feedback,
  caret visibility, mouse click/hover/scroll/double-click/triple-click/
  modifier-click, drag selection and terminal-selection suppression, and mouse
  hot-path performance.
- Link every baseline row to current evidence where it exists:
  - Issue 809 geometry/input scenarios;
  - Issue 811 cursor feedback;
  - Issue 812 GUI active state;
  - Issue 816 browser state and copy-current-URL;
  - current `scripts/ghostboard-geometry-matrix.sh` scenario names.
- Record which rows should become fast automated smokes, which rows should be
  slower screenshot/manual checks, and which rows require new harness support.

Planned runtime checks:

- Run a compact current baseline using existing scenarios rather than the full
  slow matrix:
  1. `scripts/ghostboard-geometry-matrix.sh mouse-after-geometry-change`;
  2. `scripts/ghostboard-geometry-matrix.sh keyboard-after-tab-window-switch`;
  3. `scripts/ghostboard-geometry-matrix.sh gui-active-multi-tab`;
  4. `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`.
- If one of those scenarios fails, stop and record the failing row, owner, logs,
  and next experiment recommendation instead of masking it with additional
  scenarios.

Planned source changes:

- None unless the baseline run proves the existing harness cannot distinguish a
  required pass/fail condition. If that happens, limit implementation changes to
  this issue's docs and, at most, `scripts/ghostboard-geometry-matrix.sh`
  assertions/logging needed to make the baseline trustworthy, then rerun the
  affected scenario. If Ghostboard, webtui, Roamium, or protocol source changes
  are needed, record `Partial` or `Fail` and make the source fix the next
  experiment.

## Verification

Formatting actions:

1. `prettier --write --prose-wrap always --print-width 80 issues/0817-ghostboard-input-focus-regression-matrix/README.md issues/0817-ghostboard-input-focus-regression-matrix/01-establish-input-focus-baseline.md`.

Static checks:

1. `git diff --check`.
2. If `scripts/ghostboard-geometry-matrix.sh` changes, run
   `bash -n scripts/ghostboard-geometry-matrix.sh`.
3. If Rust files change, run `cargo fmt` and `cargo check` for the affected
   package.
4. If Ghostboard Zig or Swift files change, run the relevant `zig build` or
   `macos/build.nu` command before runtime testing.

Runtime checks:

1. `scripts/ghostboard-geometry-matrix.sh mouse-after-geometry-change`.
2. `scripts/ghostboard-geometry-matrix.sh keyboard-after-tab-window-switch`.
3. `scripts/ghostboard-geometry-matrix.sh gui-active-multi-tab`.
4. `scripts/ghostboard-geometry-matrix.sh copy-current-url-smoke`.

Pass criteria:

- The Issue 817 baseline matrix exists and maps every requested behavior from
  the issue README to `Covered`, `Partially covered`, `Uncovered`, or `Blocked`.
- Every `Covered` row cites concrete current evidence.
- The compact runtime baseline passes, or failures are classified with log paths
  and a specific next experiment.
- The result recommends the smallest next experiment based on the weakest
  uncovered or failing row.

Partial criteria:

- The matrix exists, but one or more existing scenarios fail for reasons that
  require a focused fix experiment.
- Runtime automation is available for the main keyboard/mouse/focus paths, but
  slower behaviors such as triple-click, drag selection, caret visibility, or
  hot-path performance remain only classified and not yet implemented.

Fail criteria:

- The experiment cannot map the Issue 817 requested behaviors to concrete rows.
- The runtime baseline cannot launch Ghostboard, webtui, or Roamium.
- The harness cannot produce logs specific enough to identify the owner of a
  failure.

## Design Review

This experiment is plan-only until a fresh-context adversarial design review
approves it. Record the reviewer verdict here, fix all real findings, then
commit the approved plan before implementation begins.

Fresh-context adversarial design review by Codex subagent `Boole`:

- **Initial verdict:** Changes required.
- **Finding 1:** The Rust formatting check incorrectly narrowed formatting to
  changed files. Fixed by requiring `cargo fmt` after any Rust edit.
- **Finding 2:** The experiment was missing an explicit completion/result gate.
  Fixed by adding the Completion Gate section below.
- **Optional finding:** The source-change escape hatch was wider than the
  baseline experiment needed. Fixed by constraining implementation changes to
  docs and, at most, harness assertions/logging; Ghostboard, webtui, Roamium, or
  protocol source fixes must become a follow-up experiment.
- **Final verdict:** Approved. The reviewer confirmed the prior findings were
  resolved and no Required findings remained.

## Completion Gate

After implementation and verification:

- add `## Result` and `## Conclusion` to this experiment file;
- update the Issue 817 README experiment status from `Designed` to `Pass`,
  `Partial`, or `Fail`;
- request a fresh-context completion review;
- fix all real completion-review findings and record the final verdict in this
  file; and
- commit the reviewed result separately before designing or implementing the
  next experiment.
