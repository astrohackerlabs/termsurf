# Experiment 124: Port CSI Delete Lines

## Description

Continue the row-mutation port by adding Ghostty's delete-line form:

- `CSI M` / `CSI 1 M` -> delete one line at the cursor row;
- `CSI n M` -> delete `n` lines starting at the cursor row;
- lines below the deleted span through the bottom of the active vertical
  scrolling region shift upward;
- vacated rows at the bottom of the active vertical scrolling region are
  blanked;
- mutation is limited to the active horizontal margins;
- the cursor moves to the left scrolling margin on the original cursor row.

Upstream Ghostty parses this in `vendor/ghostty/src/terminal/stream.zig`:

- final `M` emits `.delete_lines`;
- no params means count `1`;
- one param emits that exact count, including `0`;
- more than one param is invalid and dispatches no action;
- any private/intermediate form is invalid for this command.

Upstream Ghostty executes this in
`vendor/ghostty/src/terminal/Terminal.zig::deleteLines`:

- count `0` is a no-op;
- if the cursor is outside the active top/bottom or left/right scrolling
  margins, the command is a no-op;
- the effective count is clamped to the remaining rows from the cursor through
  the bottom margin;
- shifted rows move upward within the vertical scrolling region;
- if the horizontal margins are full width, row wrap and wrap-continuation
  metadata for shifted source/destination rows are reset;
- if the horizontal margins are not full width, only the cells inside the left /
  right margins shift, and row wrap metadata is preserved;
- vacated cells are cleared to blanks;
- all shifted/cleared rows are dirtied;
- pending wrap is always cleared after an actual delete;
- the cursor is restored to `(left_margin, start_y)`.

This experiment ports `CSI M` only. It does not port `CSI S`/`CSI T` scroll up
or scroll down, even though upstream implements those partly in terms of
delete-lines/insert-lines. It also does not implement Unicode-width boundary
repair, SGR-colored blanks, or wide-character rendering; those remain deferred
for the same reasons recorded in Experiment 123.

The main implementation risk is accidentally using the existing PageList
erase-row helpers. Those helpers can delete/grow page-list rows and are useful
for scrollback/active history management, but `CSI M` must preserve scrollback
and active screen height. Prefer a dedicated active-region shift-up primitive
that mirrors Experiment 123's shift-down primitive and copies/clears cells
inside fixed active coordinates.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for `CSI M` parsing:
     - no params -> count `1`;
     - one param -> exact count;
     - multiple params -> invalid;
     - private/intermediate forms -> invalid.
   - Use `vendor/ghostty/src/terminal/Terminal.zig::deleteLines` for terminal
     semantics.
   - Use upstream tests around `Terminal: deleteLines` as the behavior
     checklist, especially simple deletion, zero count, cursor outside region,
     top/bottom margins, high counts, pending wrap, wrap reset, managed
     metadata, left/right margins, and scrollback preservation.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action.
   - Add `Action::DeleteLines { count }` in `roastty/src/terminal/stream.rs`.
   - Keep the action internal to the terminal module.
   - Do not add public API or ABI surface.

3. Extend CSI dispatch for final `M`.
   - Add a `delete_lines_action()` helper.
   - Accept:
     - `CSI M`;
     - `CSI 0 M`;
     - `CSI ; M`;
     - `CSI 1 M`;
     - `CSI 1 ; M`;
     - larger single numeric params, clamped to `u16::MAX` by the current parser
       accumulator behavior.
   - Reject and dispatch no action for:
     - private forms such as `CSI ? M` and `CSI > M`;
     - real multi-param forms such as `CSI 1 ; 2 M` and `CSI ;; M`;
     - colon/mixed separators;
     - any byte that the current parser treats as invalid;
     - direct C1 CSI byte `0x9b`, which remains out of scope and follows the
       current UTF-8 replacement behavior.
   - Preserve parser ground-state behavior on handler errors.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by `CSI M`, dispatch `U+FFFD` before the
     delete-lines action.

4. Add a safe active-region delete-lines primitive.
   - Add the narrow Page/PageList helper needed to shift active rows upward from
     `cursor_y` through `bottom_margin`.
   - The helper should:
     - operate only inside the active screen domain;
     - clamp count to `bottom_margin - cursor_y + 1`;
     - traverse from top to bottom so source rows are copied before their
       destination rows are later reused/cleared;
     - copy only the `[left_margin, right_margin + 1)` cell range;
     - clear vacated rows inside the same horizontal range;
     - release managed memory for overwritten and cleared cells;
     - preserve moved grapheme/style/hyperlink ownership correctly;
     - preserve non-managed metadata such as protected bits on moved cells;
     - clear vacated cells to default cells with default style under the current
       basic model;
     - mark every shifted or cleared row dirty;
     - preserve scrollback row count and content.
   - Full-width row swapping is optional. Correctness is more important than
     matching upstream's optimized swap path in this slice. Reusing the same
     partial-row copy/clear machinery from Experiment 123 is acceptable if it
     preserves managed-memory integrity.
   - Add PageList tests before relying on the helper from `Screen`.
   - Stop and record Partial if managed-memory movement across page boundaries
     cannot be made integrity-safe in this experiment.

5. Add screen/terminal delete-lines behavior.
   - Add
     `Screen::delete_lines_basic(count, top_margin, bottom_margin, left_margin, right_margin, full_width)`.
   - Pass the existing `Terminal::scrolling_region` bounds into the helper.
   - If `count == 0`, do nothing:
     - no dirty rows;
     - no pending-wrap change;
     - no cursor movement.
   - If the cursor is outside the active top/bottom or left/right scrolling
     margins, do nothing and preserve pending wrap.
   - Clamp count to the remaining rows from cursor through bottom margin.
   - Shift rows upward and blank vacated rows inside the horizontal margins.
   - After an actual delete:
     - clear pending wrap;
     - move the cursor to `(left_margin, original_cursor_y)`;
     - for full-width horizontal margins, clear wrap and wrap-continuation
       metadata on shifted/cleared rows in the affected vertical range;
     - for left/right horizontal margins, preserve row wrap metadata.
   - Do not mutate rows above `cursor_y` or below `bottom_margin`.
   - Do not mutate scrollback.
   - Current-SGR blank-cell coloring remains deferred because Roastty's current
     basic print path does not yet write cells with cursor style. Add tests
     proving default blank behavior now, and document SGR-preserving blanks as
     deferred until the SGR mutation path exists.

6. Route terminal actions.
   - In `TerminalStreamHandler`, route `Action::DeleteLines` to the new helper.
   - Reuse the existing error conversion style.
   - Existing print, linefeed, cursor, positioning, tab, erase-display,
     erase-line, delete-character, insert-line, formatter, PageList, and ABI
     behavior must keep passing unchanged.

7. Add tests.
   - Stream parser tests:
     - `A\x1b[MB` dispatches print `A`, delete-lines count `1`, print `B`;
     - `CSI M` dispatches count `1`;
     - `CSI 0 M` and `CSI ; M` dispatch count `0`;
     - `CSI 1 M` and `CSI 1 ; M` dispatch count `1`;
     - larger single params dispatch their parsed/clamped value;
     - real multi-param, colon-param, mixed-separator, and invalid/private forms
       dispatch no action and do not leak printable final bytes;
     - split-feed `CSI M` and `CSI 3 M` dispatch correctly;
     - pending invalid UTF-8 emits `U+FFFD` before same-slice and split-feed
       `CSI M`;
     - direct C1 CSI byte `0x9b` followed by `M` remains out of scope and
       dispatches `U+FFFD` plus printable `M`;
     - handler errors from delete-lines leave the parser in ground state;
     - existing cursor, positioning, line, tab, erase-display, erase-line,
       delete-character, insert-line, and `CSI I` behavior remains unchanged.
   - PageList tests:
     - full-width single-page delete shifts rows up and clears vacated rows;
     - full-width delete across a page boundary preserves page-list integrity
       and managed metadata;
     - left/right margin delete copies only the bounded cell range and preserves
       cells outside the margins;
     - moved style/grapheme/hyperlink metadata remains owned exactly once;
     - protected bits and other non-managed metadata move with shifted cells;
     - overwritten/cleared cells release managed memory;
     - vacated blank cells are default cells with default style under the
       current basic model;
     - scrollback row count and content are unchanged;
     - all shifted/cleared active rows are dirty, and unrelated rows are not.
   - Terminal tests:
     - simple delete at a middle row produces Ghostty's `ABC\nGHI` shape;
     - count `0` is a no-op and preserves pending wrap;
     - count larger than remaining region clamps;
     - cursor outside top/bottom margins is a no-op;
     - cursor outside left/right margins is a no-op;
     - top/bottom scrolling region constrains the shifted/cleared rows;
     - left/right scrolling region shifts and blanks only bounded columns;
     - oversized count with left/right scrolling region clamps vertically while
       still blanking only bounded columns and preserving cells outside the
       margins;
     - cursor moves to the left margin on the original row after an actual
       delete;
     - actual delete clears pending wrap;
     - full-width delete resets wrap metadata for affected rows;
     - left/right-margin delete preserves wrap metadata;
     - rows above the cursor and below the bottom margin are not mutated;
     - scrollback row count and content are not mutated;
     - vacated blank cells are rendered as ordinary default blanks in current
       formatter output;
     - unsupported/invalid `CSI M` forms do not mutate terminal state;
     - split-feed `CSI M` mutates terminal state correctly.
   - Existing stream, cursor movement, positioning, tabstop, erase-display,
     erase-line, delete-character, insert-line, formatter, PageList, and ABI
     tests must keep passing.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream
     cargo test -p roastty terminal::terminal
     cargo test -p roastty terminal::page_list
     cargo test -p roastty terminal_formatter
     cargo test -p roastty screen_formatter
     cargo test -p roastty page_string
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

9. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - Commit the approved design before implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.
   - Commit the approved result separately from the design commit.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - accepted `CSI M` forms;
      - rejected `CSI M` forms;
      - terminal behavior for count `0`, count `1`, larger counts, clamping,
        margins, pending wrap, cursor restoration, and row metadata;
      - PageList row-shift and page-boundary behavior;
      - managed-memory safety evidence;
      - dirty-row behavior;
      - deferred SGR blank-cell coloring and wide-boundary behavior, if still
        deferred;
      - confirmation that existing raw print, cursor, positioning, line, tab,
        erase-display, erase-line, delete-character, insert-line, formatter,
        PageList, and ABI behavior did not regress;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Design Review

Codex reviewed the initial design and found one real issue:
`logs/codex-review/20260601-051625-041313-last-message.md`.

- The design separately required high-count clamping and left/right-margin
  behavior, but did not require the combined upstream case where an oversized
  count clamps vertically while still clearing only the bounded horizontal
  margin range. The terminal test checklist now requires that combined case.

Codex re-reviewed the updated design and reported no blocking findings:
`logs/codex-review/20260601-051858-366252-last-message.md`.

The design is ready to commit before implementation.

## Verification

The experiment passes if:

- `CSI M` dispatches and performs delete-lines count `1`;
- `CSI 0 M` and `CSI ; M` dispatch count `0` and no-op at terminal level;
- single-count `CSI n M` forms dispatch and perform the corresponding clamped
  active-region delete;
- invalid/private/colon/mixed/multi-param forms dispatch no delete-lines action
  and do not leak printable bytes;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior;
- pending invalid UTF-8 emits `U+FFFD` before the delete-lines action;
- handler errors leave the parser in ground state;
- delete-lines shifts only the intended rows and columns inside the active
  scrolling margins;
- cursor position is restored to the left margin on the original row after an
  actual delete;
- actual deletes clear pending wrap;
- full-width deletes reset affected row wrap metadata;
- left/right-margin deletes preserve row wrap metadata;
- count `0` and outside-margin no-ops preserve pending wrap;
- rows above the cursor, rows below the bottom margin, and scrollback are not
  mutated;
- shifted/cleared rows become dirty and unaffected rows do not;
- managed grapheme/style/hyperlink movement and cleanup remains integrity-safe;
- non-managed cell metadata such as the protected bit moves with shifted cells;
- vacated blank cells are default cells with default style under the current
  basic model;
- existing raw print, linefeed, cursor, positioning, tabstop, erase-display,
  erase-line, delete-character, insert-line, PageList, formatter, and ABI
  behavior remains unchanged;
- no scroll up/down, SGR, Unicode-width, wide-character rendering, public API,
  ABI, or non-macOS behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream dispatch is correct but active-region row shifting needs a separate
  PageList primitive design;
- full-width delete works but left/right-margin behavior needs a separate
  bounded-copy primitive;
- plain-cell delete works but managed grapheme/style/hyperlink movement cannot
  be made integrity-safe in this slice;
- behavior is correct for cells but dirty tracking or row metadata requires an
  additional narrow primitive before the result can be called complete.

The experiment fails if:

- it changes unrelated cursor, tab, print, erase-display, erase-line,
  delete-character, insert-line, formatter, or ABI behavior;
- it treats `CSI 0 M` as count `1`;
- it mutates cells outside the active scrolling margins;
- it mutates scrollback;
- it corrupts PageList integrity or leaks managed memory;
- it silently implements incompatible placeholder delete-line semantics;
- it adds unrelated scroll up/down, public API, ABI, or non-macOS behavior.
