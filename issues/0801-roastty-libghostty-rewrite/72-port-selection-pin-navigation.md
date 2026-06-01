# Experiment 72: Port Selection Pin Navigation

## Description

Port the PageList-local pin navigation helpers needed by upstream
`SelectionGesture.dragSelection`.

Upstream `SelectionGesture.dragSelection` depends on `Pin.before`,
`Pin.leftClamp`, `Pin.rightClamp`, `Pin.leftWrap`, and `Pin.rightWrap`.
Roastty's `Pin` is a value that stores a `NonNull<Node>`, while `PageList` owns
the page order and can safely validate node membership. Therefore this
experiment should adapt those helpers as private `PageList` methods instead of
adding traversal logic directly to `Pin`.

This experiment is a prerequisite for the next selection-gesture slice. It must
not port `SelectionGesture`, mouse event state, press/release handling,
autoscroll, word selection, line selection, semantic-output selection, Screen,
Terminal, public C ABI, renderer, parser, app, or platform input behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/PageList.zig` for:
     - `Pin.before`;
     - `Pin.leftClamp`;
     - `Pin.rightClamp`;
     - `Pin.leftWrap`;
     - `Pin.rightWrap`.
   - Use `vendor/ghostty/src/terminal/SelectionGesture.zig::dragSelection` to
     confirm which helper behavior is needed by the next slice.
   - Use existing Roastty code:
     - `roastty/src/terminal/page_list.rs::pin_up`;
     - `roastty/src/terminal/page_list.rs::pin_down`;
     - `roastty/src/terminal/page_list.rs::pin_is_valid`;
     - `roastty/src/terminal/page_list.rs::node_index`.
   - Do not modify `vendor/ghostty/`.

2. Add private `PageList` pin-navigation helpers.
   - Add methods such as:

     ```rust
     fn pin_before(&self, pin: Pin, other: Pin) -> Option<bool>
     fn pin_left_clamp(&self, pin: Pin, cells: CellCountInt) -> Option<Pin>
     fn pin_right_clamp(&self, pin: Pin, cells: CellCountInt) -> Option<Pin>
     fn pin_left_wrap(&self, pin: Pin, cells: usize) -> Option<Pin>
     fn pin_right_wrap(&self, pin: Pin, cells: usize) -> Option<Pin>
     ```

   - The exact names may follow local style, but the behavior must be
     PageList-local and private.
   - The movement parameter is the number of cells to move, matching upstream's
     `n`; it is not the terminal width. Row width must come from the current or
     target PageList row, not from the caller.
   - `pin_before` should:
     - return `None` if either pin is garbage or not valid in this PageList;
     - compare `(node order, y, x)` using the PageList's page order;
     - return `Some(false)` for equal pins.
   - Clamp helpers should:
     - return `None` for garbage or invalid pins;
     - preserve the pin's node, row, and garbage state;
     - move left/right within the same row, saturating at column `0` or the
       row's last column.
   - Wrap helpers should:
     - return `None` for garbage or invalid pins;
     - support the one-cell movement needed by `dragSelection`;
     - move to the adjacent row when crossing a row boundary;
     - return `None` when wrapping would move before the first row or after the
       last row;
     - use `pin_up` / `pin_down` internally if useful, but apply horizontal-wrap
       x semantics explicitly: left wrap sets `x` to the previous row's last
       column, and right wrap sets `x` to `0` when crossing rows.

3. Keep scope narrow.
   - Do not add `selection_gesture.rs` yet.
   - Do not add a public API.
   - Do not add Screen or Terminal.
   - Do not implement word, line, output, autoscroll, press/release, deep-press,
     or mouse-input behavior.
   - Do not alter existing selection tracking, ordering, containment,
     contained-row, or adjustment behavior except where tests prove a helper
     must reuse it.

4. Add tests.
   - Add focused `PageList` tests proving:
     - `pin_before` works within one row;
     - `pin_before` works across rows in one page;
     - `pin_before` works across pages;
     - equal pins are not before each other;
     - invalid or garbage pins return `None`;
     - left clamp moves one cell within a row;
     - left clamp saturates at column `0`;
     - right clamp moves one cell within a row;
     - right clamp saturates at the row's last column;
     - left wrap moves one cell within a row when `x > 0`;
     - left wrap from column `0` moves to the previous row's last column;
     - right wrap moves one cell within a row when `x < last_column`;
     - right wrap from the last column moves to the next row's column `0`;
     - left wrap at the first cell returns `None`;
     - right wrap at the last cell returns `None`;
     - wrap helpers work across page boundaries.
   - Existing PageList and selection tests must continue passing.

5. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty pin_navigation
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
     - helper behavior implemented;
     - tests added;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- PageList has private helpers equivalent to the upstream pin ordering,
  clamping, and one-cell wrapping behavior needed by
  `SelectionGesture.dragSelection`;
- invalid or garbage pins return `None`;
- helpers work within a row, across rows, and across PageList page boundaries;
- wrapping returns `None` at the top-left and bottom-right edges;
- no `SelectionGesture`, Screen, Terminal, public ABI, renderer, parser, app,
  word selection, line selection, semantic-output selection, autoscroll,
  press/release, deep-press, or platform input behavior is added;
- `cargo fmt`, targeted pin-navigation tests, PageList tests, and full
  `cargo test -p roastty` pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- ordering and clamping work, but wrapping needs a follow-up because it exposes
  a deeper PageList coordinate issue.

The experiment fails if:

- helpers are added to `Pin` in a way that assumes direct linked-list traversal
  unavailable in Roastty's storage model;
- helpers silently accept garbage or missing-node pins;
- wrapping skips rows or mishandles page boundaries;
- the experiment drifts into full `SelectionGesture`, Screen, Terminal, public
  ABI, renderer, parser, app, or mouse-input behavior;
- tests or formatting fail.

## Design Review

Codex reviewed the initial design and required three changes before
implementation:

- rename the movement parameter from `cols` to `cells` or make the helpers
  explicit one-cell helpers, because upstream's argument is a movement count,
  not terminal width;
- add ordinary non-boundary movement tests for clamp and wrap helpers;
- clarify that wrap helpers may reuse `pin_up` / `pin_down`, but must set `x`
  according to horizontal wrapping semantics when crossing rows.

Those changes are incorporated above. A follow-up Codex review must approve this
updated design before implementation begins.

Follow-up Codex review approved the updated design for implementation. No
remaining blockers were found.
