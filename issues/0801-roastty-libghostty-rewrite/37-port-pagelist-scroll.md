# Experiment 37: Port PageList Viewport Scrolling

## Description

Port the bounded, non-mutating PageList viewport scroll commands from upstream
Ghostty:

- `Scroll.active`
- `Scroll.top`
- `Scroll.row`
- `Scroll.delta_row`
- `Scroll.pin`

Experiment 36 added `Scrollbar`, viewport row offsets, pin-active/top
predicates, and pin offset cache integrity. This experiment should use those
pieces to move the viewport over already-existing rows. It must not create
scrollback, allocate pages, prune pages, erase rows, resize, reflow, or port
prompt-jump scrolling.

This is still PageList-local state. It should not touch parser, screen,
renderer, app integration, or public ABI.

## Changes

1. Re-read upstream source.
   - Use `vendor/ghostty/src/terminal/PageList.zig` as the source of truth for:
     - `Scroll`;
     - `scroll`, excluding `delta_prompt`;
     - related upstream tests from `PageList scroll with max_size 0 no history`
       through `PageList scroll to row with cache fast path up`.
   - Stop before `PageList scroll clear` and any prompt-jump tests.
   - Do not modify `vendor/ghostty/`.

2. Add a Rust scroll command type.
   - Add an internal enum shaped for current needs:

     ```rust
     enum Scroll {
         Active,
         Top,
         Row(usize),
         DeltaRow(isize),
         Pin(Pin),
     }
     ```

   - Do not add `DeltaPrompt` yet. Upstream `delta_prompt` depends on prompt row
     metadata and prompt iteration that Roastty has not ported.

3. Port `scroll`.
   - Shape it as:

     ```rust
     fn scroll(&mut self, behavior: Scroll)
     ```

   - Always finish by satisfying PageList integrity, matching upstream's
     `defer self.assertIntegrity()`.
   - If `explicit_max_size == 0`, force `Viewport::Active` and return. This is
     upstream's no-scrollback behavior.

4. Port `Scroll::Active` and `Scroll::Top`.
   - `Active` sets `viewport = Viewport::Active`.
   - `Top` sets `viewport = Viewport::Top`.
   - Preserve existing cached pin offset if harmless, but do not let it affect
     non-pin viewport calculations.

5. Port `Scroll::Pin`.
   - If the target pin is active, set `Viewport::Active`.
   - Else if the target pin is top, set `Viewport::Top`.
   - Else copy the pin into `viewport_pin`, set `viewport = Viewport::Pin`, and
     invalidate `viewport_pin_row_offset`.
   - Preserve upstream behavior that the pin's `x` is ignored for viewport
     positioning after the pin is copied.

6. Port `Scroll::Row`.
   - Row `0` sets `Viewport::Top`.
   - Rows at or below `total_rows - rows` clamp to `Viewport::Active`.
   - If the current viewport is a pin with a cached row offset, use the upstream
     fast path by converting the row target to `DeltaRow`.
   - Otherwise set `viewport_pin_row_offset = Some(row)`, set
     `viewport = Viewport::Pin`, and find the matching page/row by direct
     traversal.
   - Use the same forward/backward traversal choice as upstream when practical:
     forward from the first page for rows before the midpoint, backward from the
     last page for rows at or after the midpoint.
   - If a row cannot be found, clamp to `Viewport::Active`, matching upstream's
     defensive fallback.

7. Port `Scroll::DeltaRow`.
   - Preserve upstream edge cases:
     - top + negative/no-op stays top;
     - active + positive/no-op stays active;
     - active + negative moves upward if there is scrollback;
     - moving above the first row clamps to top;
     - moving into or past the active area clamps to active.
   - Fast-path existing pin viewports with `pin_up` / `pin_down` and update the
     cached row offset when it exists.
   - Slow-path non-pin viewports from `get_top_left(Tag::Viewport)`.
   - Invalidate `viewport_pin_row_offset` when the final pinned viewport was
     produced by a slow path and no exact cache update is available.

8. Add tests.
   - No-scrollback mode rejects `Top`, `Pin`, `Row`, and `DeltaRow` by staying
     active and reporting `rows/0/rows`; also verify the viewport top-left did
     not move.
   - `Scroll::Top` moves the viewport to the top and scrollbar offset zero;
     verify the viewport top-left row is zero.
   - `Scroll::Active` returns from top/pin to active and reports
     `total_rows - rows`; verify the viewport top-left row equals the active
     start row.
   - `DeltaRow(-1)` from active with simulated history moves one row up and
     reports the expected offset; verify the viewport top-left row.
   - `DeltaRow` above the first row clamps to top and verifies top-left row
     zero.
   - `DeltaRow` forward from top creates a pin at the expected row and verifies
     top-left row.
   - `DeltaRow` forward into active clamps to active and verifies top-left row.
   - `Pin` to a scrollback row creates a pin viewport and ignores `x` for the
     visible top-left.
   - `Pin` to the active area clamps to active.
   - `Pin` to row zero clamps to top.
   - `Row(0)` clamps to top and verifies top-left row zero.
   - `Row` in scrollback creates a pin viewport, sets the cache to that row, and
     verifies top-left row.
   - `Row` in the middle of a larger scrollback range verifies the midpoint
     traversal case and top-left row.
   - `Row` at the active boundary clamps to active and verifies top-left row.
   - `Row` beyond active clamps to active and verifies top-left row.
   - `Row` without scrollback preserves active and verifies top-left row.
   - `Row` followed by `DeltaRow` moves from the cached pin and verifies
     top-left row after each move.
   - Cached-pin `Row` fast path down verifies the same final scrollbar state and
     top-left row as direct row traversal.
   - Cached-pin `Row` fast path up verifies the same final scrollbar state and
     top-left row as direct row traversal.

9. Preserve scope.
   - Do not implement:
     - `DeltaPrompt`;
     - prompt iterators or prompt semantic row behavior;
     - grow/prune/erase/reset/resize/reflow;
     - mutation-time viewport fixups;
     - row creation or allocation;
     - screen/parser integration;
     - renderer or app integration;
     - public C ABI additions.
   - Do not add `ghostty` names except when citing upstream paths or test
     provenance.

10. Verify.
    - Run:

      ```bash
      cargo fmt
      cargo test -p roastty terminal::page_list
      cargo test -p roastty
      ```

    - `cargo fmt` output must be accepted as-is.

11. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - scroll enum variants added;
      - scroll behaviors implemented;
      - prompt scrolling deferred rationale;
      - tests added;
      - verification command output summary.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Scroll::Active`, `Top`, `Row`, `DeltaRow`, and `Pin` match upstream behavior
  for already-existing PageList rows;
- no-scrollback mode cannot scroll away from active;
- row and delta scrolling clamp to top/active at the same boundaries as
  upstream;
- pin and row scrolling correctly invalidate or update
  `viewport_pin_row_offset`;
- no prompt scrolling, mutation behavior, allocation behavior, public ABI, or
  app integration is introduced;
- `cargo fmt`, targeted PageList tests, and full `cargo test -p roastty` pass;
- an independent agent reviews the experiment design and completed result and
  approves them, or all real findings are fixed.

The experiment is partial if:

- active/top/pin scrolling works, but row or delta scrolling exposes a missing
  PageList primitive that should be ported before completing viewport scrolling.

The experiment fails if:

- any scroll command creates rows, allocates pages, prunes pages, or mutates
  page contents;
- no-scrollback mode can leave active;
- row or delta scrolling uses active-relative coordinates instead of absolute
  rows from the top of the PageList;
- prompt scrolling is accidentally introduced;
- tests or formatting fail.
