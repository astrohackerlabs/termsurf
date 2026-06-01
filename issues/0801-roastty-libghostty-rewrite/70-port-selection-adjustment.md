# Experiment 70: Port Selection Adjustment

## Description

Port the `Selection.Adjustment` enum and `Selection.adjust` behavior from
upstream `Selection.zig`.

Upstream adjustment mutates only the selection's end pin. The start pin is
preserved because the end pin represents the active drag/keyboard endpoint,
regardless of visual order. Roastty already has the required local foundation:
selection value mutation, PageList screen pin conversion, row and cell
iterators, pin up/down movement, and cell text detection.

This experiment should add the adjustment behavior as a private PageList-owned
helper because adjustment depends on PageList coordinates, cells, rows, and
screen size. It must not add word selection, line selection, formatting,
selection gestures, C ABI, Screen, search, renderer, parser, app, or unrelated
terminal mutation behavior.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Selection.zig` for:
     - `Adjustment`;
     - `adjust`;
     - upstream tests from `Selection: adjust right` through
       `Selection: adjust end of line`.
   - Use existing Roastty code:
     - `roastty/src/terminal/selection.rs`;
     - `roastty/src/terminal/page_list.rs::pin_up`;
     - `roastty/src/terminal/page_list.rs::pin_down`;
     - `roastty/src/terminal/page_list.rs::cell_iterator`;
     - `roastty/src/terminal/page_list.rs::row_iterator`;
     - `roastty/src/terminal/page_list.rs::pin_cell`;
     - `roastty/src/terminal/page.rs::Cell::has_text`;
     - `roastty/src/terminal/page.rs::Cell::has_text_any`.
   - Do not modify `vendor/ghostty/`.

2. Add the adjustment enum.
   - In `roastty/src/terminal/selection.rs`, add a terminal-internal enum:

     ```rust
     pub(super) enum Adjustment {
         Left,
         Right,
         Up,
         Down,
         Home,
         End,
         PageUp,
         PageDown,
         BeginningOfLine,
         EndOfLine,
     }
     ```

   - Derive `Debug`, `Clone`, `Copy`, `PartialEq`, and `Eq`.
   - Do not expose `Adjustment` through crate-public API or C ABI.

3. Add PageList-owned adjustment.
   - In `roastty/src/terminal/page_list.rs`, add a private helper such as:

     ```rust
     fn selection_adjust(
         &self,
         selection: &mut selection::Selection,
         adjustment: selection::Adjustment,
     ) -> Option<()>
     ```

   - Mutate only `selection.end_mut()`, matching upstream.
   - Preserve `selection.start()` exactly for every adjustment.
   - Return `None` instead of panicking if the current end pin is invalid,
     missing, garbage, or cannot map through the PageList operations needed for
     the adjustment.
   - Return `Some(())` when the adjustment is valid but leaves the end pin
     unchanged, matching upstream no-op behavior at screen edges.

4. Preserve upstream adjustment semantics.
   - `Up`:
     - move the end pin up one row with the same x if possible;
     - if not possible, fall back to `BeginningOfLine`.
   - `Down`:
     - move downward until the next row whose cells contain text;
     - preserve the original end-pin x coordinate when landing on the next
       nonblank row;
     - if no nonblank row exists below, fall back to `EndOfLine`.
   - `Left`:
     - scan left/up from the current end pin, skipping the current cell;
     - move to the next cell with text;
     - leave unchanged if no prior text cell exists.
   - `Right`:
     - scan right/down from the current end pin, skipping the current cell;
     - move to the next cell with text;
     - leave unchanged if no later text cell exists.
   - `PageUp`:
     - move up by `self.rows`;
     - if not possible, fall back to `Home`.
   - `PageDown`:
     - move down by `self.rows`;
     - if not possible, fall back to `End`.
   - `Home`:
     - move the end pin to screen coordinate `(0, 0)`.
   - `End`:
     - scan rows upward from the screen bottom and move the end pin to the last
       column of the first row that contains text;
     - leave unchanged if no text row exists.
   - `BeginningOfLine`:
     - set the end pin x to `0`.
   - `EndOfLine`:
     - set the end pin x to the current row's last column.

5. Add small local helpers only if they remove real duplication.
   - Acceptable examples:
     - helper to set only the selection end pin;
     - helper to find whether a row contains any text;
     - helper to find the current row's last column.
   - Keep helpers private to `PageList`.
   - Do not introduce a public selection adjustment API.

6. Add tests.
   - Port upstream tests for:
     - adjust right: simple movement, end-of-line wrap, no-op at final text
       cell;
     - adjust left: simple movement, wrap to prior row;
     - adjust left skips blanks: same-line blanks and prior-row blanks;
     - adjust up: normal move and fallback to beginning of line;
     - adjust down: normal move and fallback to end of line;
     - adjust down with a not-full screen;
     - adjust page up: successful move by `self.rows` and fallback to `Home`;
     - adjust page down: successful move by `self.rows` and fallback to `End`;
     - adjust home;
     - adjust end with a not-full screen;
     - adjust beginning of line;
     - adjust end of line.
   - Add tests proving:
     - every adjustment preserves `selection.start()`;
     - normal `Down` movement preserves the original end-pin x coordinate when
       it lands on a later nonblank row;
     - adjustments mutate tracked end-pin storage when the selection is tracked;
     - invalid or garbage end pins return `None` without mutating the start pin;
     - a valid adjustment that cannot find a text target returns `Some(())` and
       leaves the end pin unchanged where upstream no-ops.
   - Existing selection ordering, containment, contained-row, highlight,
     PageList, and full Roastty tests must continue passing.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal::page_list::tests::selection
     cargo test -p roastty terminal::selection
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get an independent agent review of this experiment
     design.
   - After implementation and verification, get an independent result review.
   - Fix all real findings before proceeding.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - adjustment enum added;
     - adjustment helper behavior;
     - fallback behavior;
     - invalid/no-op behavior;
     - tracked selection behavior;
     - tests added;
     - verification command output summary;
     - independent result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `selection::Adjustment` exists with the upstream adjustment variants;
- `PageList` can adjust only the selection end pin;
- all upstream adjustment behaviors are ported;
- fallback behavior for up, down, page-up, and page-down matches upstream;
- left/right scanning skips blank cells and wraps rows like upstream;
- home/end and beginning/end-of-line match upstream;
- tracked selections mutate tracked end-pin storage;
- invalid or garbage end pins return `None` instead of panicking;
- valid no-op edge cases return `Some(())` and preserve the end pin;
- no word/line selection, formatting, gestures, C ABI, Screen, search, renderer,
  parser, app, tracking ownership changes, or unrelated terminal mutation
  behavior is introduced;
- `cargo fmt`, targeted selection/PageList tests, and full
  `cargo test -p roastty` pass;
- independent design and result reviews approve the experiment, or all real
  findings are fixed before proceeding.

The experiment is partial if:

- line-boundary and vertical adjustments work, but left/right text scanning
  needs a follow-up.

The experiment fails if:

- adjustment mutates the selection start pin;
- adjustment uses visual order instead of mutating the stored end pin;
- left/right scanning lands on blank cells or fails to wrap rows as upstream
  does;
- down/end fail to skip blank rows as upstream does;
- invalid end pins panic instead of returning `None`;
- word/line selection, formatting, gestures, ABI, Screen, search, renderer,
  parser, app, tracking ownership changes, or unrelated behavior is added
  prematurely;
- tests or formatting fail.

## Result

**Result:** Pass

Implemented selection adjustment in `roastty/src/terminal/selection.rs` and
`roastty/src/terminal/page_list.rs`.

The change adds the terminal-internal `selection::Adjustment` enum with the
upstream variants:

- `Left`;
- `Right`;
- `Up`;
- `Down`;
- `Home`;
- `End`;
- `PageUp`;
- `PageDown`;
- `BeginningOfLine`;
- `EndOfLine`.

`PageList` now owns a private
`selection_adjust(&mut selection::Selection, selection::Adjustment) -> Option<()>`
helper. The helper validates the current end pin, mutates only
`selection.end_mut()`, and preserves `selection.start()` for every adjustment.

The implementation ports upstream behavior:

- `Up` moves one row or falls back to `BeginningOfLine`;
- `Down` scans to the next nonblank row while preserving x, or falls back to
  `EndOfLine`;
- `Left` and `Right` scan through cells, skip the current cell, skip blanks, and
  wrap rows;
- `PageUp` and `PageDown` move by `self.rows` or fall back to `Home` / `End`;
- `Home` moves to screen `(0, 0)`;
- `End` scans upward from the screen bottom to the last row containing text and
  moves to that row's last column;
- `BeginningOfLine` and `EndOfLine` update only the end pin's x coordinate.

Invalid or garbage end pins return `None` instead of panicking. Valid no-op edge
cases return `Some(())` and leave the end pin unchanged.

Added tests for:

- upstream right adjustment cases;
- upstream left adjustment and blank-skipping cases;
- upstream up/down behavior and fallback behavior;
- down preserving x on normal movement and handling not-full screens;
- page-up and page-down successful moves and fallbacks;
- home, end, beginning-of-line, and end-of-line;
- tracked selection end mutation;
- start preservation;
- invalid and garbage end-pin behavior;
- valid no-op edge behavior.

Verification:

```bash
cargo fmt
cargo test -p roastty terminal::page_list::tests::selection
cargo test -p roastty terminal::selection
cargo test -p roastty
```

Results:

- `cargo fmt` completed successfully.
- `cargo test -p roastty terminal::page_list::tests::selection`: 32 passed.
- `cargo test -p roastty terminal::selection`: 12 passed.
- `cargo test -p roastty`: 609 unit tests passed, ABI harness passed, doctests
  passed.

Independent result review approved the implementation as a Pass. The reviewer
found no blocking issues, confirmed the upstream adjustment shape, confirmed the
test coverage, and found no scope drift.

## Conclusion

Roastty now has the upstream selection adjustment behavior represented as a
PageList-owned private helper. This completes the core `Selection.zig` selection
value, ordering, containment, row extraction, and adjustment behavior ported so
far while keeping higher-level word/line selection, formatting, gestures, ABI,
and Screen integration for later experiments.
