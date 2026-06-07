+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 815: Renderer Frame Rebuild Plan

## Description

Add the first value-level frame rebuild planner for Roastty's renderer. Upstream
`renderer/generic.zig` decides, before drawing, whether the terminal grid
changed, whether cells need a full rebuild or row-level rebuild, which rows are
eligible for CPU-side cell reconstruction, and whether IME preedit text should
mask cells on the cursor row. Roastty already has `Contents`, preedit values,
and Metal frame presentation primitives, but it does not yet have the tested
decision boundary that connects terminal dirty state to `Contents` rebuilding.

This experiment does not port the full `terminal.RenderState` update, row
formatting, link highlighting, search highlighting, cursor glyph emission,
images, overlays, custom shaders, renderer thread, or draw pacing. It adds a
small pure Rust planner that later `rebuildCells` integration can call before
mutating `Contents`.

## Changes

- `roastty/src/renderer/frame_rebuild.rs`
  - Add a new renderer module for the upstream `rebuildCells` decision layer.
  - Add a `RenderDirty` enum with `Clean`, `Partial`, and `Full`, matching the
    upstream terminal render-state dirty modes.
  - Add a `FrameRebuildInput` value carrying:
    - the current `Contents` grid size,
    - the terminal render-state grid size,
    - the terminal render dirty mode,
    - per-row dirty flags,
    - optional preedit text, and
    - optional cursor viewport coordinate.
  - Add a `FrameRebuildPlan` value carrying:
    - `grid_changed`,
    - optional `resize_to` grid size,
    - the effective post-resize grid size,
    - `full_rebuild`,
    - the viewport row count to process,
    - the ordered row indexes to rebuild, and
    - side-effect metadata for later integration: `reset_contents`,
      `clear_rows`, and `rows_to_mark_clean`,
    - optional preedit row/range metadata.
  - Add a small `FrameRebuildPlanError` for invalid input that would otherwise
    make the upstream indexing assumptions unsafe, especially short row-dirty
    slices.
  - Add `FrameRebuildPlan::build(input)` that mirrors the front half of upstream
    `rebuildCells`:
    - `grid_changed` is true when rows or columns differ.
    - grid changes are planned as `resize_to = terminal_grid`, and all row
      decisions use the effective post-resize grid size, matching upstream's
      `self.cells.resize` before `row_len` calculation.
    - `full_rebuild` is true when dirty mode is `Full` or the grid changed.
    - full rebuild processes every row that fits in both the render-state
      viewport and the effective post-resize contents grid.
    - non-full rebuild processes every dirty row, even when the dirty enum is
      `Clean`, because upstream row dirty flags are authoritative after
      highlight/link updates.
    - preedit range is present only when preedit exists, cursor viewport exists,
      the cursor row is inside the processed row range, and that row will be
      rebuilt.
    - preedit width/range calculation reuses `renderer::state::Preedit::range`.
    - zero-row or zero-column grids are accepted and produce no rows/preedit
      range, avoiding max-column underflow.
    - cursor coordinates outside the effective viewport skip preedit range
      planning.
    - row-dirty slices shorter than the terminal render-state row count return
      `FrameRebuildPlanError::DirtyRowsTooShort`; extra dirty flags are ignored.
  - Add focused tests for full rebuild, partial rebuild, clean frames, grid
    growth/shrink after resize, row-count clamping, clean dirty-row processing,
    side-effect metadata for resize/reset/clear/mark-clean actions, short dirty
    slices, zero-sized grids, and preedit range inclusion/exclusion.
- `roastty/src/renderer/mod.rs`
  - Add the `frame_rebuild` module.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - After implementation, update the renderer tracker to note that a frame
    rebuild dirty planner exists while keeping actual terminal-state
    reconstruction, row formatting, renderer-thread integration, and live frame
    orchestration open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/generic.zig` `updateFrame`
  - `vendor/ghostty/src/renderer/generic.zig` `rebuildCells`
  - `roastty/src/renderer/cell.rs`
  - `roastty/src/renderer/state.rs`
  - `roastty/src/renderer/size.rs`
- Run:
  - `cargo fmt -p roastty`
  - `cargo test -p roastty renderer::frame_rebuild -- --nocapture`
  - `cargo test -p roastty renderer::state -- --nocapture`
  - `cargo test -p roastty renderer::cell::tests::resize -- --nocapture`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/815-renderer-frame-rebuild-plan.md`
- Run:
  - `git diff --check`

The planner consumes already-finalized row dirty flags after the future
terminal-state, search-highlight, and link-highlight update steps. It does not
perform those updates in this experiment.

The experiment passes if the planner produces the same rebuild decisions as the
front half of upstream `rebuildCells` for clean, partial, full, and grid-changed
frames, including resize-before-row-selection behavior, dirty rows even when the
dirty enum is clean, the side-effect metadata needed by later `Contents`
integration, and the preedit row masking conditions. It is Partial if the
planner lands but needs follow-up to match a missed upstream edge case. It fails
if the decision logic cannot be separated cleanly from the later terminal
row-formatting work.

## Design Review

Codex reviewed the initial design and found real issues before implementation.
The original plan computed rows against the current pre-resize contents grid,
which would miss newly added rows on grid growth even though upstream resizes
`self.cells` before calculating `row_len`. It also treated `RenderDirty::Clean`
as a no-work frame, but upstream still rebuilds rows whose row dirty flags are
set; this matters because search/highlight updates can dirty rows while the
outer dirty enum is clean. Codex also noted that the planner needed either
side-effect metadata or an explicit deferral for resize/reset/clear/mark-clean
actions, plus input invariants for short dirty slices, zero grids, and
out-of-viewport cursors.

The design was amended to plan against the effective post-resize grid, process
dirty row flags in every non-full frame regardless of the outer dirty enum,
return explicit side-effect metadata (`resize_to`, `reset_contents`,
`clear_rows`, and `rows_to_mark_clean`), define validation behavior, and expand
tests for grid growth/shrink, clean-plus-dirty rows, short dirty slices,
zero-sized grids, and preedit inclusion/exclusion.

Codex re-reviewed the amended design and approved it with no remaining blocking
findings. The follow-up review confirmed the post-resize row semantics,
clean-plus-dirty-row behavior, side-effect metadata, and input invariants. The
only non-blocking suggestion was to make the planned tests explicitly assert the
side-effect metadata, which was added before the plan commit.

## Result

**Result:** Pass

Roastty now has a value-level renderer frame rebuild planner:

- `roastty/src/renderer/frame_rebuild.rs` adds `RenderDirty`,
  `FrameRebuildInput`, `FrameRebuildPlan`, `FramePreeditRange`, and
  `FrameRebuildPlanError`.
- `FrameRebuildPlan::build` plans the same front-half rebuild decisions as
  upstream `rebuildCells`: grid change detection, resize-to-terminal-grid
  semantics, post-resize row selection, full rebuild versus row-dirty rebuild,
  reset/clear/mark-clean metadata, and optional preedit range planning.
- Non-full frames process dirty rows even when the outer dirty enum is `Clean`,
  matching upstream's row-dirty gate after terminal/search/link state updates.
- Short row-dirty slices return `DirtyRowsTooShort`; extra dirty flags are
  ignored; zero-row/zero-column grids and out-of-viewport cursors produce no
  preedit range rather than underflowing.
- `roastty/src/renderer/mod.rs` exposes the new `frame_rebuild` module.

Verification:

- Inspected `vendor/ghostty/src/renderer/generic.zig` `updateFrame`.
- Inspected `vendor/ghostty/src/renderer/generic.zig` `rebuildCells`.
- Inspected `roastty/src/renderer/cell.rs`.
- Inspected `roastty/src/renderer/state.rs`.
- Inspected `roastty/src/renderer/size.rs`.
- `cargo fmt -p roastty` — passed.
- `cargo test -p roastty renderer::frame_rebuild -- --nocapture` — passed, 13
  tests.
- `cargo test -p roastty renderer::state -- --nocapture` — passed, 8 tests.
- `cargo test -p roastty renderer::cell::tests::resize -- --nocapture` — matched
  0 tests, so it was replaced with the actual resize test filter.
- `cargo test -p roastty renderer::cell::tests::contents_resize -- --nocapture`
  — passed, 4 tests.
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/815-renderer-frame-rebuild-plan.md`
  — passed.
- `git diff --check` — passed.

## Conclusion

Experiment 815 completes the tested decision boundary between future terminal
render-state updates and `Contents` row rebuilding. The next renderer slices can
apply this plan to real `Contents` mutation and row formatting, then connect the
prepared contents into the existing Metal frame compositor. This does not yet
implement the live render loop, renderer thread, glyph upload orchestration,
images, custom shaders, overlays, or pacing.

## Completion Review

Codex reviewed the completed implementation and approved it with no blocking
code findings. The review confirmed that `FrameRebuildPlan::build` matches the
approved scope and upstream front-half behavior: resize before row selection,
full rebuild on dirty full/grid change, row-dirty rebuild in non-full frames,
reset versus clear metadata, mark-clean rows, and preedit gating against rebuilt
cursor rows. The only finding was that the result verification record initially
omitted the successful Prettier and `git diff --check` commands; those bullets
were added before the result commit.
