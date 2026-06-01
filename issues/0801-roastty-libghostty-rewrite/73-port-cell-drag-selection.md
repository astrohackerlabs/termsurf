# Experiment 73: Port Cell Drag Selection

## Description

Port the pure cell-granular part of upstream `SelectionGesture.dragSelection`.

Experiment 72 added the PageList-local pin ordering, clamp, and wrap helpers
that `dragSelection` needs. This experiment should now port the calculation that
turns a click pin, drag pin, click pixel x, drag pixel x, rectangle flag, and
grid geometry into an optional untracked `Selection`.

This is not the full `SelectionGesture` port. It must not add press/release
state, double/triple-click handling, autoscroll, deep press, word selection,
line selection, semantic-output selection, Screen, Terminal, public C ABI,
renderer, parser, app, or platform input behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/SelectionGesture.zig` for:
     - `dragSelection`;
     - `test "SelectionGesture drag selection logic"`;
     - `test "SelectionGesture rectangle drag selection logic"`;
     - the local `testDragSelection` and `testDragSelectionIsNull` helpers.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs` pin-navigation helpers from
       Experiment 72;
     - `roastty/src/terminal/selection.rs`.
   - Do not modify `vendor/ghostty/`.

2. Add a narrow cell-drag selection helper.
   - Preferred shape:

     ```rust
     struct DragGeometry {
         columns: u32,
         cell_width: u32,
         padding_left: u32,
         screen_height: u32,
     }

     fn drag_selection(
         &self,
         click_pin: Pin,
         drag_pin: Pin,
         click_x: u32,
         drag_x: u32,
         rectangle_selection: bool,
         geometry: DragGeometry,
     ) -> Option<selection::Selection>
     ```

   - The helper may live in `page_list.rs` as a private PageList method, or in a
     new `selection_gesture.rs` module if the PageList pin-navigation helpers
     are exposed only as `pub(super)`.
   - Keep it crate-internal/private. Do not add public C ABI.
   - Use the upstream threshold calculation:
     - threshold is `round(cell_width * 0.6)`;
     - subtract `padding_left` with saturating subtraction;
     - clamp pixel x to `columns * cell_width - 1`;
     - modulo by `cell_width` to get fractional x in the clicked/dragged cell.
   - Return `None` if geometry is invalid enough to make the calculation
     meaningless, such as `columns == 0`, `cell_width == 0`, or
     `columns * cell_width` overflowing `u32`.
   - Use checked arithmetic for `columns * cell_width - 1`, e.g.
     `columns.checked_mul(cell_width).and_then(|v| v.checked_sub(1))`, returning
     `None` on overflow.
   - Use `pin_before` for regular selections and x-only comparison for rectangle
     selections, matching upstream.
   - Use clamp helpers for rectangle selection and wrap helpers for regular
     selection, matching upstream.
   - Preserve upstream wrap-failure fallback: when regular selection asks for
     `leftWrap(1) orelse original_pin` or `rightWrap(1) orelse original_pin`,
     Roastty must use `pin_left_wrap(...).unwrap_or(original_pin)` or
     `pin_right_wrap(...).unwrap_or(original_pin)`. A failed wrap helper must
     not automatically make `drag_selection` return `None`.
   - Return an untracked
     `Selection::new(start_pin, end_pin, rectangle_selection)` on success.

3. Preserve upstream behavior.
   - Same-cell selections should return `None` unless the pointer crosses the
     threshold far enough to include the cell.
   - Adjacent-cell threshold cases that include neither side should return
     `None`.
   - Regular selections may wrap rows.
   - Rectangle selections must not wrap rows horizontally; they clamp columns.
   - Invalid or garbage click/drag pins should return `None`.

4. Add tests.
   - Port the upstream regular selection table:
     - LTR single-cell selection;
     - LTR include both click and drag cells;
     - LTR include click only;
     - LTR include drag only;
     - LTR include neither endpoint but keep middle cell;
     - LTR empty same-cell and adjacent threshold cases;
     - RTL equivalents;
     - regular wrapping cases.
     - edge cases where regular selection wrap fallback reaches a PageList edge
       and still follows upstream `orelse original_pin` behavior instead of
       propagating `None` from the helper.
   - Port the upstream rectangle selection table:
     - LTR single-column selection;
     - LTR include both columns;
     - LTR include click only;
     - LTR include drag only;
     - LTR include neither endpoint but keep middle column;
     - LTR empty threshold cases;
     - RTL equivalents;
     - rectangle non-wrapping cases.
   - Add Roastty-specific tests for:
     - `padding_left` saturating subtraction;
     - invalid `columns == 0`;
     - invalid `cell_width == 0`;
     - overflowing `columns * cell_width`;
     - garbage click or drag pins returning `None`.
   - Existing PageList and selection tests must continue passing.

5. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty drag_selection
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

6. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real findings before proceeding.

7. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - helper shape and location;
     - upstream table coverage;
     - Roastty-specific edge tests;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- Roastty has a private helper equivalent to upstream
  `SelectionGesture.dragSelection`;
- regular and rectangular drag-selection tests match the upstream tables;
- regular selections wrap rows, while rectangle selections clamp columns and do
  not wrap;
- same-cell and adjacent threshold empty-selection cases return `None`;
- invalid geometry and invalid/garbage pins return `None`;
- regular-selection wrap-helper failure falls back to the original pin, matching
  upstream `orelse original_pin` behavior;
- no press/release state, double/triple-click handling, autoscroll, deep press,
  word selection, line selection, semantic-output selection, Screen, Terminal,
  public ABI, renderer, parser, app, or platform input behavior is added;
- `cargo fmt`, targeted drag-selection tests, PageList tests, and full
  `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- regular cell drag selection works, but rectangle mode exposes a mismatch that
  needs its own follow-up.

The experiment fails if:

- the helper depends on full `SelectionGesture`, Screen, Terminal, public ABI,
  renderer, parser, app, or platform input behavior;
- the threshold math differs from upstream without a documented reason;
- wrap-helper failure is propagated as `None` instead of using the upstream
  original-pin fallback;
- rectangle selection wraps horizontally instead of clamping columns;
- invalid geometry panics instead of returning `None`;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and required two changes before
implementation:

- preserve upstream regular-selection wrap failure fallback to the original pin,
  instead of propagating `None` from the PageList wrap helper;
- use checked arithmetic for `columns * cell_width - 1` and test overflow.

Those changes are incorporated above. A follow-up Codex review must approve this
updated design before implementation begins.

Follow-up Codex review approved the updated design for implementation. No
remaining blockers were found.
