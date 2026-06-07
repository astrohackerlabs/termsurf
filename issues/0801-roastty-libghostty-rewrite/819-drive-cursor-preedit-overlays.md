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

# Experiment 819: Drive Cursor and Preedit Overlays

## Description

Connect the frame rebuild plan to Roastty's existing cursor and IME/preedit
glyph helpers. Experiment 818 can format planned text rows, including the
background and text masking needed when preedit occupies the cursor row. The
remaining post-row step from upstream `rebuildCells` is to clear the old cursor
glyph, draw the selected cursor glyph when appropriate, and draw preedit glyphs
over the planned preedit range.

This experiment keeps the input prepared and value-level. It does not collect
live terminal render state, calculate the final cursor style from
`RenderStateScalar`, update Metal shader cursor uniforms, upload cell buffers,
submit draw calls, pace redraws, or add the live renderer thread.

## Changes

- `roastty/src/renderer/frame_rebuild.rs`
  - Add `FrameTextOverlayInput<'a>` with prepared overlay data:
    - `preedit: Option<&'a Preedit>`,
    - `cursor: Option<FrameCursorOverlay>`,
    - `screen_fg: Rgb`, and
    - `alpha: u8`.
  - Add `FrameCursorOverlay` with:
    - `grid_pos: [u16; 2]`,
    - `style: renderer::cursor::Style`,
    - `wide: bool`, and
    - `color: Rgb`.
  - Add `FrameTextOverlayValidationError` for:
    - `Contents` grid mismatches against the plan's effective grid,
    - cursor positions outside the effective grid,
    - wide cursor overlays that would extend past the effective grid,
    - invalid preedit ranges where `start > end` or `end` exceeds the effective
      grid width,
    - preedit ranges that point outside the effective grid,
    - missing preedit payload when the plan has a preedit range, and
    - mismatched preedit payload when the payload cannot satisfy the plan's
      `cp_offset` or the codepoint widths from `cp_offset` do not fit the
      planned range.
  - Add `FrameTextOverlayRenderError` for glyph/font render errors from the
    existing overlay helpers.
  - Add `FrameTextOverlayApplication` recording whether the prior cursor was
    cleared, which cursor style was drawn, and whether preedit glyphs were
    attempted.
  - Add
    `FrameRebuildPlan::draw_text_overlays(&self, contents: &mut Contents, grid: &mut SharedGrid, input: FrameTextOverlayInput<'_>) -> Result<FrameTextOverlayApplication, FrameTextOverlayError>`.
  - Validate all prepared overlay inputs before mutation, including
    `contents.size() == self.effective_grid`, so malformed callers cannot clear
    a cursor or write preedit glyphs into a mismatched `Contents` grid.
  - Always clear the previous cursor first with
    `Contents::set_cursor(None, None)`, matching upstream's per-frame cursor
    refresh semantics.
  - If `input.preedit.is_some()`, do not draw a cursor glyph. This matches
    upstream `rebuildCells`, which suppresses cursor drawing for active preedit
    before checking whether a preedit range exists.
  - If `input.preedit` and the plan's preedit range are both present, draw the
    plan-owned preedit glyphs with `cell::add_preedit`. The row formatter has
    already used the same plan-owned range as the mask source.
  - If `input.preedit.is_some()` and the plan has no preedit range, leave the
    cursor cleared and draw no preedit glyphs.
  - If `input.preedit.is_none()` and a cursor overlay is provided, draw it with
    `cell::add_cursor`.
  - If no cursor overlay is provided and preedit is inactive, leave the cursor
    cleared.
  - Convert `renderer::cursor::Style` to the existing `cell::CursorStyle` cursor
    glyph style without reimplementing cursor-style selection.
  - Add tests proving:
    - a stale cursor is cleared when no overlay is provided,
    - a non-preedit cursor overlay draws the expected cursor glyph position and
      style,
    - block cursor overlays use the reserved first cursor row and non-block
      cursors use the reserved last cursor row through `Contents::set_cursor`,
    - a plan-owned preedit range suppresses cursor drawing and adds preedit text
      and underline glyphs,
    - active preedit without a planned range still suppresses cursor drawing,
    - cursor/preedit validation errors abort before mutation, and
    - `Contents` grid mismatches abort before mutation, before the stale cursor
      is cleared,
    - missing or invalid preedit payloads abort before mutation.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - After implementation, update the renderer tracker to mention that prepared
    cursor/preedit overlays can be emitted after planned row formatting, while
    live terminal-state collection, cursor uniform updates, glyph upload/draw
    calls, pacing, and renderer-thread integration remain open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/generic.zig` `rebuildCells` cursor/preedit
    section
  - `roastty/src/renderer/frame_rebuild.rs`
  - `roastty/src/renderer/cell.rs`
  - `roastty/src/renderer/cursor.rs`
- Run:
  - `cargo fmt -p roastty`
  - `cargo test -p roastty renderer::frame_rebuild -- --nocapture`
  - `cargo test -p roastty renderer::cell::tests::add_cursor -- --nocapture`
  - `cargo test -p roastty renderer::cell::tests::add_preedit -- --nocapture`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/819-drive-cursor-preedit-overlays.md`
- Run:
  - `git diff --check`

The experiment passes if prepared cursor and preedit overlay inputs can be
validated and emitted after planned row formatting using the existing cell
helpers, without duplicating cursor-style selection or live terminal-state
collection. It is Partial if the driver lands but one overlay path requires a
follow-up wrapper. It fails if cursor/preedit overlay emission cannot be cleanly
separated from the live renderer state.

## Design Review

Codex reviewed the initial design and found two blocking correctness issues and
two verification gaps. First, upstream suppresses cursor drawing whenever
preedit is active, even if no preedit range is planned; the original design only
suppressed the cursor when the plan had a preedit range. Second, the wrapper
must reject `Contents` grids that do not match the plan's effective grid before
clearing the cursor or drawing glyphs. Codex also noted that preedit and cursor
extent validation was underspecified, and that the test plan needed explicit
wrapper-level coverage for active preedit without a planned range, contents-grid
mismatch, and malformed prepared payload extents.

The design was amended to suppress cursor drawing whenever
`input.preedit.is_some()`, validate `contents.size()` before mutation, define
preedit/cursor extent validation before mutation, and add wrapper-level tests
for those cases.

Codex re-reviewed the amended design and approved it for implementation with no
remaining blockers. The re-review confirmed that the prior preedit suppression,
contents-grid validation, extent-validation, and wrapper-test findings were
resolved. The only note was wording-level: `cell::CursorStyle` is effectively
the renderer cursor style used by the cell helper, so the implementation should
use the existing local type path rather than introduce a duplicate style.
