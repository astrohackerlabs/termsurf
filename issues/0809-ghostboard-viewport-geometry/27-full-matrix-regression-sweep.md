# Experiment 27: Full Matrix Regression Sweep

## Description

Experiments 1-26 tested and fixed each row of the viewport matrix one at a time.
The issue README requires a final experiment that re-tests the complete matrix
and proves the behaviors work together, not just individually.

The goal of this experiment is to run the complete Ghostboard geometry matrix
against the current worktree, collect the per-row runtime evidence, and close
Issue 809 only if the final evidence satisfies the issue acceptance criteria.

This experiment should not add new feature behavior. If a matrix row fails, the
result should be recorded as `Fail` or `Partial`, the issue should remain open,
and the next experiment should localize and fix that row. If the only remaining
limitation is the already documented single-display VM constraint for display
move/backing-scale, record that as a known environment-limited partial and do
not pretend a multi-display move was verified.

## Changes

Planned files:

- `issues/0809-ghostboard-viewport-geometry/27-full-matrix-regression-sweep.md`
  - record the full-matrix scenario list;
  - record the design review, verification, per-row result table, completion
    review, and conclusion.
- `issues/0809-ghostboard-viewport-geometry/README.md`
  - link Experiment 27 in the experiment index;
  - if and only if the final sweep satisfies the issue goal, add the issue
    conclusion and close the issue.
- `issues/README.md`
  - regenerate only if Issue 809 is closed.
- `scripts/ghostboard-geometry-matrix.sh`
  - change only if the final sweep exposes a harness bug that prevents a valid
    matrix row from running.
- Product code
  - change only if the final sweep exposes a real Ghostboard/Roamium/webtui
    regression. Any product fix should stop this final sweep and become a new
    focused experiment instead of being hidden inside the closure run.

## Verification

Pass criteria:

- Markdown is formatted:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0809-ghostboard-viewport-geometry/README.md \
    issues/0809-ghostboard-viewport-geometry/27-full-matrix-regression-sweep.md
  ```

- Shell syntax and whitespace checks pass:

  ```bash
  bash -n scripts/ghostboard-geometry-matrix.sh
  git diff --check
  ```

- The full matrix is run sequentially, one scenario at a time:

  ```bash
  scripts/ghostboard-geometry-matrix.sh initial-open
  scripts/ghostboard-geometry-matrix.sh window-resize
  scripts/ghostboard-geometry-matrix.sh split-right
  scripts/ghostboard-geometry-matrix.sh split-down
  scripts/ghostboard-geometry-matrix.sh split-right-resize
  scripts/ghostboard-geometry-matrix.sh split-right-equalize
  scripts/ghostboard-geometry-matrix.sh split-right-zoom
  scripts/ghostboard-geometry-matrix.sh split-right-close-sibling
  scripts/ghostboard-geometry-matrix.sh split-right-close-browser-pane
  scripts/ghostboard-geometry-matrix.sh split-right-focus-switch
  scripts/ghostboard-geometry-matrix.sh new-terminal-tab-visibility
  scripts/ghostboard-geometry-matrix.sh open-browser-in-new-tab
  scripts/ghostboard-geometry-matrix.sh close-browser-tab
  scripts/ghostboard-geometry-matrix.sh open-browser-in-new-window
  scripts/ghostboard-geometry-matrix.sh multiple-windows-with-browsers
  scripts/ghostboard-geometry-matrix.sh display-move-backing-scale
  scripts/ghostboard-geometry-matrix.sh fullscreen-unfullscreen
  scripts/ghostboard-geometry-matrix.sh minimize-hide-restore
  scripts/ghostboard-geometry-matrix.sh font-size-cell-metrics
  scripts/ghostboard-geometry-matrix.sh tui-overlay-resize-command
  scripts/ghostboard-geometry-matrix.sh terminal-scrollback-movement
  scripts/ghostboard-geometry-matrix.sh browser-navigation-geometry
  scripts/ghostboard-geometry-matrix.sh devtools-split-geometry
  scripts/ghostboard-geometry-matrix.sh mouse-after-geometry-change
  scripts/ghostboard-geometry-matrix.sh keyboard-after-tab-window-switch
  ```

- The result records a per-row table with:
  - viewport matrix row;
  - harness scenario that covers that row;
  - status;
  - screenshot path or `n/a`;
  - harness log path;
  - app log path;
  - Roamium trace path;
  - identity tuple evidence;
  - rect/backing-scale/input notes;
  - pass/fail notes.
- If one harness scenario covers multiple README matrix rows, the table must
  list those README rows separately or explicitly map the combined scenario to
  every row it covers. Examples include focus away/back, tab switch away/back,
  and window open/switch behavior.
- The final conclusion explicitly says whether Issue 809 can close.
- The fresh-context design review is recorded in this experiment file, and the
  Experiment 27 plan is committed before the matrix sweep begins.
- A fresh-context completion review approves the result before the result
  commit.
- If closing the issue:
  - README frontmatter changes to `status = "closed"` with `closed` set to the
    current date;
  - `## Conclusion` is added to the issue README;
  - `scripts/build-issues-index.sh` is run;
  - the issue close is committed separately after the Experiment 27 result
    commit if that creates a clearer history.

Fail criteria:

- Any required matrix scenario fails and the issue is closed anyway.
- The final table omits evidence paths or collapses multiple scenarios into a
  vague summary.
- The display-move/backing-scale row is claimed as a full multi-display pass
  without actual multi-display evidence.
- Product changes are made inside this final sweep instead of being split into a
  new focused experiment.

## Design Review

Fresh-context adversarial design review initially returned **CHANGES REQUIRED**.

Required finding:

- The design did not explicitly require recording the design review and
  committing the plan before running the matrix sweep.

Optional finding:

- The planned final table listed harness scenarios but did not explicitly map
  combined harness scenarios to the README viewport matrix rows they cover.

Fixes:

- Added a pass criterion requiring design-review recording and plan commit
  before running the sweep.
- Added final-table requirements for README viewport matrix row mapping,
  including combined-scenario mappings.

Re-review verdict: **APPROVED**.

The reviewer confirmed the required design/plan commit gate and the viewport
matrix row mapping requirement are now resolved, with no new required findings.
