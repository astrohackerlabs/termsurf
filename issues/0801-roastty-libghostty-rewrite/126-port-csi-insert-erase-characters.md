# Experiment 126: Port CSI Insert and Erase Characters

## Description

Continue the row-mutation subsystem by porting Ghostty's remaining basic
same-row character mutation commands:

- `CSI @` / ICH inserts blank characters at the cursor and shifts existing cells
  right toward the right scrolling margin;
- `CSI X` / ECH erases characters at the cursor without shifting surrounding
  cells.

`CSI P` delete characters is already implemented from Experiment 122, and the
line/scroll primitives from Experiments 123-125 are now in place. Grouping
`CSI @` and `CSI X` is the right granularity because both operate on a single
active row and share count parsing, clear-cell behavior, pending-wrap clearing,
dirty-row handling, and managed-cell cleanup. They do not share every boundary
rule: `CSI @` is constrained by the left/right scrolling margins, while `CSI X`
uses the screen edge. This is larger than a one-command experiment but still
narrow enough to diagnose failures cleanly.

Upstream Ghostty parses these in `vendor/ghostty/src/terminal/stream.zig`:

- final `@` emits `.insert_blanks`;
- final `X` emits `.erase_chars`;
- no params means count `1`;
- for `CSI @`, one param is clamped to a minimum of `1`, so `CSI 0 @` and
  `CSI ; @` insert one blank;
- for `CSI X`, one param emits exactly the parsed value, but
  `Terminal.eraseChars()` treats zero as one with `@max(count_req, 1)`;
- more than one param is invalid and dispatches no action;
- any private/intermediate form is invalid for these commands.

Upstream Ghostty executes these in
`vendor/ghostty/src/terminal/Terminal.zig::insertBlanks` and
`Terminal.zig::eraseChars`:

- both commands operate only on the current active row;
- both commands preserve the absolute cursor position;
- both commands clear `pending_wrap`;
- `insertBlanks()` clears pending wrap before checking horizontal margins;
- `insertBlanks()` is a no-op for cells when the cursor is outside the
  left/right margins, but still clears pending wrap;
- `insertBlanks()` clamps the insertion count to the remaining columns from the
  cursor through the right margin, shifts cells right, and clears the inserted
  range;
- `insertBlanks()` does not reset row soft-wrap metadata; it only clears
  `pending_wrap` and marks the row dirty;
- `eraseChars()` treats count zero as count one, clamps to the remaining screen
  width from the cursor to the right edge, resets wrap metadata for the row, and
  clears the erased range;
- `eraseChars()` only respects protected cells when Ghostty's active protected
  mode is ISO. Roastty does not yet have that protected-mode stream state, and
  plain `CSI X` has no protected-marker form like `CSI ? J` / `CSI ? K`.
  Therefore this experiment must implement plain `CSI X` as an unprotected clear
  that clears cells even if their stored per-cell protection bit is set, and
  must document the deferred ISO protected-mode gap.

This experiment intentionally does not solve current-SGR blank-cell coloring or
Unicode-width boundary repair. Those gaps are already deferred in recent row
mutation experiments and require broader styled printing / wide-character work.
The implementation should not add fake or partial wide-character handling just
to satisfy upstream tests that Roastty cannot yet represent correctly.

Do not implement `CSI Z`, SGR, OSC, DCS, alternate-screen semantics, Kitty
graphics, public ABI, or non-macOS behavior in this experiment.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for `CSI @` and `CSI X`
     parsing.
   - Use `vendor/ghostty/src/terminal/Terminal.zig::insertBlanks` and
     `Terminal.zig::eraseChars` for execution semantics.
   - Use upstream tests around `Terminal: insertBlanks` / `Terminal: eraseChars`
     as the behavior checklist, especially count-zero behavior, cursor
     preservation, pending-wrap clearing, horizontal margins, protected cells,
     styled/managed metadata cleanup, and row wrap metadata.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action.
   - Add `Action::InsertChars { count }` for `CSI @`.
   - Add `Action::EraseChars { count }` for `CSI X`.
   - Keep both actions internal to the terminal module.
   - Do not add public API or ABI surface.

3. Extend CSI dispatch.
   - Add `insert_chars_action()` for final `@`.
   - Add `erase_chars_action()` for final `X`.
   - Accept:
     - `CSI @` and `CSI X`;
     - `CSI 0 @`, `CSI ; @`, `CSI 0 X`, and `CSI ; X`;
     - `CSI 1 @`, `CSI 1 ; @`, `CSI 1 X`, and `CSI 1 ; X`;
     - larger single numeric params, clamped to `u16::MAX` by the current parser
       accumulator behavior.
   - For `CSI @`, clamp the action count to at least `1` in the parser, matching
     upstream's `@max(1, input.params[0])`.
   - For `CSI X`, the parser may emit count `0`, but execution must treat it as
     one erased cell.
   - Reject and dispatch no action for:
     - private forms such as `CSI ? @`, `CSI > @`, `CSI ? X`, and `CSI > X`;
     - real multi-param forms such as `CSI 1 ; 2 @` and `CSI 1 ; 2 X`;
     - colon/mixed separators;
     - any byte that the current parser treats as invalid;
     - direct C1 CSI byte `0x9b`, which remains out of scope and follows the
       current UTF-8 replacement behavior.
   - Preserve parser ground-state behavior on handler errors.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by `CSI @` or `CSI X`, dispatch `U+FFFD` before the
     row mutation action.

4. Add Page/PageList insertion support.
   - Add a Page-level row helper that shifts a bounded same-row range right and
     clears the inserted span, mirroring the current `delete_chars_in_row()`
     shape.
   - Add a PageList active-row wrapper around that helper.
   - The helper must preserve PageList integrity and correctly move or release
     managed style, grapheme, and hyperlink metadata for the shifted and cleared
     cells.
   - The helper should mark the affected row dirty.
   - Reuse existing `swap_cells()` / `clear_cells()` primitives where possible
     instead of duplicating managed-memory logic.

5. Add `Screen::insert_chars_basic()`.
   - Clear `cursor.pending_wrap` before checking horizontal margins.
   - If count is zero at this layer, return after the pending-wrap clear. Real
     `CSI @` input should already be clamped to at least one, but this keeps the
     primitive robust.
   - If the cursor is outside the left/right margins, mutate no cells but keep
     the pending-wrap clear.
   - Clamp the count to the remaining columns from the cursor through
     `right_margin`.
   - Shift cells right within the bounded row region and clear the inserted
     range.
   - Preserve cursor `x` and `y`.
   - Do not reset row soft-wrap metadata. Upstream `insertBlanks()` marks the
     row dirty but does not call `cursorResetWrap()`.
   - Current-SGR blank-cell coloring and wide-character split repair remain
     deferred.

6. Add `Screen::erase_chars_basic()`.
   - Treat count `0` as count `1`, matching upstream execution semantics.
   - Clamp the count to the remaining screen width from cursor through the right
     edge. Do not use the horizontal scrolling margin as the right bound unless
     upstream inspection proves that `eraseChars()` is margin-constrained.
   - Clear `pending_wrap`.
   - Reset row wrap metadata consistently with erase-line behavior.
   - Clear the target range as an unprotected clear. Plain `CSI X` has no
     protected request marker in the current parser, and Roastty does not yet
     model Ghostty's global ISO protected mode for this command.
   - Preserve cursor `x` and `y`.
   - Current-SGR blank-cell coloring and wide-character split repair remain
     deferred.

7. Route terminal actions.
   - In `TerminalStreamHandler`, route `Action::InsertChars` and
     `Action::EraseChars` to the new `Screen` helpers.
   - Pass terminal size and horizontal scrolling-region bounds where needed.
   - Existing print, linefeed, cursor, positioning, tab, erase-display,
     erase-line, delete-character, insert-line, delete-line, scroll, formatter,
     PageList, and ABI behavior must keep passing unchanged.

8. Add tests.
   - Stream parser tests:
     - `A\x1b[@B` dispatches print `A`, insert-chars count `1`, print `B`;
     - `A\x1b[XB` dispatches print `A`, erase-chars count `1`, print `B`;
     - `CSI @` / `CSI X` dispatch count `1`;
     - `CSI 0 @` and `CSI ; @` dispatch count `1`;
     - `CSI 0 X` and `CSI ; X` dispatch count `0`;
     - `CSI 1 @`, `CSI 1 ; @`, `CSI 1 X`, and `CSI 1 ; X` dispatch count `1`;
     - larger single params dispatch their parsed/clamped value;
     - real multi-param, colon-param, mixed-separator, and invalid/private forms
       dispatch no action and do not leak printable final bytes;
     - split-feed `CSI @`, `CSI 3 @`, `CSI X`, and `CSI 3 X` dispatch correctly;
     - pending invalid UTF-8 emits `U+FFFD` before same-slice and split-feed
       `CSI @` / `CSI X`;
     - direct C1 CSI byte `0x9b` followed by `@` or `X` remains out of scope and
       dispatches `U+FFFD` plus printable final byte;
     - handler errors from insert/erase chars leave the parser in ground state.
   - Page/PageList tests:
     - inserting one and multiple blanks shifts bounded row content right;
     - insertion clamps to the remaining bounded row region;
     - inserted cells are default blanks under the current basic model;
     - managed style, grapheme, and hyperlink metadata shifts with cells and
       cleared cells release ownership exactly once;
     - insertion preserves integrity across page boundaries if the active row is
       located on a non-first PageList page;
     - erase chars clears the expected range and releases managed metadata;
     - plain `CSI X` / unprotected erase chars clears cells even when their
       stored per-cell protection bit is set, documenting that ISO protected
       mode for this command remains deferred.
   - Terminal tests:
     - `CSI @` count one shifts suffix right and inserts one blank;
     - `CSI @` count zero behaves as one inserted blank;
     - `CSI @` count larger than the remaining margin clamps;
     - `CSI @` preserves cursor position and clears pending wrap;
     - `CSI @` preserves row soft-wrap metadata when it mutates cells;
     - `CSI @` outside horizontal margins mutates no cells but clears pending
       wrap;
     - `CSI @` honors left/right margins;
     - `CSI @` preserves scrollback and other rows;
     - `CSI X` count one clears one cell without shifting suffix content;
     - `CSI X` count zero behaves as one erased cell;
     - `CSI X` oversized count clamps to the remaining screen width;
     - `CSI X` preserves cursor position and clears pending wrap;
     - `CSI X` ignores active left/right margins and clears through the screen
       edge, including when the cursor starts outside the horizontal margins;
     - `CSI X` resets row wrap metadata;
     - plain `CSI X` clears stored protected cells under Roastty's current lack
       of global ISO protected-mode state;
     - unsupported/invalid `CSI @` / `CSI X` forms do not mutate terminal state;
     - split-feed `CSI @` / `CSI X` mutates terminal state correctly.
   - Existing stream, cursor movement, positioning, tabstop, erase-display,
     erase-line, delete-character, insert-line, delete-line, scroll, formatter,
     PageList, and ABI tests must keep passing.

9. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream
     cargo test -p roastty terminal::terminal
     cargo test -p roastty terminal::page
     cargo test -p roastty terminal::page_list
     cargo test -p roastty terminal_formatter
     cargo test -p roastty screen_formatter
     cargo test -p roastty page_string
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

10. Independent review.
    - Before implementation, get Codex review of this experiment design.
    - Fix all real design findings before implementation.
    - Record the design-review outcome in this experiment file before
      implementation.
    - Commit the approved design before implementation.
    - After implementation and verification, get Codex review of the completed
      result.
    - Fix all real result findings before proceeding.
    - Commit the approved result separately from the design commit.

11. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - accepted and rejected `CSI @` / `CSI X` forms;
      - terminal behavior for count `0`, count `1`, larger counts, clamping,
        margins, pending-wrap clearing, and cursor restoration;
      - proof that insert-chars shifts cells and managed metadata correctly;
      - proof that erase-chars clears cells and releases managed metadata
        correctly;
      - PageList integrity and managed-memory safety evidence;
      - dirty-row and wrap-metadata behavior;
      - protected-cell behavior under Roastty's current unprotected `CSI X`
        model and the deferred ISO protected-mode gap;
      - deferred SGR blank-cell coloring and wide-boundary behavior, if still
        deferred;
      - confirmation that existing raw print, cursor, positioning, line, tab,
        erase-display, erase-line, delete-character, insert-line, delete-line,
        scroll, formatter, PageList, and ABI behavior did not regress;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Design Review

Codex reviewed the initial design and found three real issues:
`logs/codex-review/20260601-055259-376287-last-message.md`.

- `CSI @` was incorrectly specified as resetting row soft-wrap metadata.
  Upstream `insertBlanks()` clears pending wrap and marks the row dirty, but
  does not call `cursorResetWrap()`.
- `CSI X` horizontal-margin behavior was contradictory and under-tested.
  Upstream `eraseChars()` uses the screen edge, not active left/right margins.
- Protected-cell behavior was too vague. The design now states that plain
  `CSI X` is an unprotected clear under Roastty's current model and documents
  Ghostty's ISO protected-mode behavior as deferred.

Codex re-reviewed the updated design and found no remaining blocking issues:
`logs/codex-review/20260601-055539-822019-last-message.md`. The design is
approved for implementation.

## Verification

The experiment passes if:

- `CSI @` and `CSI X` dispatch counts exactly as upstream requires, including
  `CSI @` parser clamping to at least one and `CSI X` execution treating zero as
  one;
- invalid/private/colon/mixed/multi-param forms dispatch no row mutation action
  and do not leak printable bytes;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior;
- pending invalid UTF-8 emits `U+FFFD` before the row mutation action;
- handler errors leave the parser in ground state;
- `CSI @` shifts cells right only inside the intended row region, inserts blank
  cells, preserves cursor position, and clears pending wrap;
- `CSI @` outside horizontal margins mutates no cells but still clears pending
  wrap;
- `CSI @` preserves row soft-wrap metadata;
- `CSI X` clears cells without shifting suffix content, preserves cursor
  position, and clears pending wrap;
- `CSI X` count zero erases one cell;
- `CSI X` ignores active left/right margins and clears to the screen edge,
  including when the cursor starts outside the horizontal margins;
- `CSI X` resets row soft-wrap metadata;
- managed style/grapheme/hyperlink movement and cleanup remains integrity-safe;
- plain `CSI X` clears stored protected cells, with Ghostty's ISO protected-mode
  behavior documented as deferred;
- vacated blank cells are default cells with default style under the current
  basic model;
- existing raw print, linefeed, cursor, positioning, tabstop, erase-display,
  erase-line, delete-character, insert-line, delete-line, scroll, PageList,
  formatter, and ABI behavior remains unchanged;
- no unrelated CSI, SGR, OSC, DCS, public API, ABI, or non-macOS behavior is
  added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream dispatch is correct but insert-char managed metadata movement needs a
  dedicated PageList primitive before it can be integrity-safe;
- `CSI @` works for plain cells but cannot yet prove managed metadata ownership
  after shifts;
- plain `CSI X` works as an unprotected clear, but implementing Ghostty's ISO
  protected-mode behavior for `eraseChars()` proves to require a separate
  protection-mode experiment;
- visual row mutation is correct but dirty tracking or wrap metadata requires an
  additional narrow primitive before the result can be called complete.

The experiment fails if:

- it treats `CSI 0 @` as a no-op instead of one inserted blank;
- it treats `CSI 0 X` as a no-op instead of one erased cell;
- it mutates cells outside the intended active row region;
- it corrupts PageList integrity or leaks managed memory;
- it changes cursor position after either command;
- it leaves pending wrap set after either command;
- it changes unrelated cursor, tab, print, erase-display, erase-line,
  delete-character, insert-line, delete-line, scroll, formatter, or ABI
  behavior;
- it adds unrelated CSI, SGR, OSC, DCS, public API, ABI, or non-macOS behavior.

## Result

**Result:** Pass

Experiment 126 ports Ghostty's `CSI @` / ICH and `CSI X` / ECH commands across
the current private Roastty terminal stack.

The stream parser now dispatches:

- `Action::InsertChars { count }` for final `@`;
- `Action::EraseChars { count }` for final `X`;
- default count `1` for `CSI @` and `CSI X`;
- count `1` for `CSI 0 @` and `CSI ; @`, matching Ghostty's parser-level minimum
  for insert blanks;
- count `0` for `CSI 0 X` and `CSI ; X`, with execution treating that as one
  erased cell;
- count `1` for `CSI 1 @`, `CSI 1 ; @`, `CSI 1 X`, and `CSI 1 ; X`;
- larger single numeric params using the existing `u16::MAX` parser clamp.

The parser rejects the invalid forms required by the design:

- private forms such as `CSI ? @`, `CSI > @`, `CSI ? X`, and `CSI > X`;
- real multi-param forms such as `CSI 1 ; 2 @` and `CSI 1 ; 2 X`;
- colon and mixed-separator forms;
- direct raw C1 CSI byte `0x9b`, which remains out of scope and follows the
  current UTF-8 replacement behavior;
- handler errors leave the parser in ground state before returning the error;
- pending invalid UTF-8 dispatches `U+FFFD` before same-slice and split-feed
  insert/erase character actions.

The execution path now implements both commands:

- `CSI @` clears pending wrap before margin checks;
- `CSI @` preserves cursor position, shifts cells right inside the current
  left/right scrolling margins, inserts default blank cells, and clamps to the
  remaining bounded row width;
- `CSI @` outside the horizontal margins mutates no cells but still clears
  pending wrap;
- `CSI @` preserves row soft-wrap metadata, matching Ghostty's `insertBlanks()`
  behavior;
- `CSI X` treats count `0` as count `1`;
- `CSI X` preserves cursor position, clears pending wrap, clears cells without
  shifting suffix content, and clamps to the screen edge;
- `CSI X` ignores active left/right margins, including when the cursor starts
  outside those margins;
- `CSI X` resets row soft-wrap metadata;
- plain `CSI X` is implemented as an unprotected clear under Roastty's current
  model, so it clears stored per-cell protection bits. Ghostty's ISO
  protected-mode behavior for `eraseChars()` remains deferred until Roastty has
  the corresponding protected-mode stream state.

The implementation adds:

- `Page::insert_chars_in_row()`, mirroring the existing `delete_chars_in_row()`
  shape but shifting bounded same-row cells right;
- `PageList::insert_active_chars()`;
- `Screen::insert_chars_basic()`;
- `Screen::erase_chars_basic()`;
- `Action::InsertChars` and `Action::EraseChars` routing in
  `TerminalStreamHandler`.

Managed metadata coverage now includes:

- a Page test proving insert-character shifts style, grapheme, hyperlink, and
  protected metadata with the moved cell while releasing the inserted blank
  range exactly once;
- a PageList page-boundary test proving the active-row insert helper keeps
  PageList integrity when the active row lives on a non-first backing page;
- a PageList erase-path test proving the clear primitive used by `CSI X`
  releases style, grapheme, and hyperlink metadata and leaves no managed-memory
  row flags behind;
- terminal tests proving `CSI @` shifts stored protected-cell metadata and
  `CSI X` clears stored protected cells under the current unprotected model;
- a terminal test proving `CSI X` clears pending wrap directly.

Current-SGR blank-cell coloring, Unicode-width boundary repair, wide-character
rendering, ISO protected-mode stream state for `eraseChars()`, alternate-screen
semantics, Kitty graphics, and broader public ABI work remain deferred. None of
those were added or required for this slice.

Verification commands:

```bash
cargo fmt
cargo test -p roastty stream
cargo test -p roastty terminal::terminal
cargo test -p roastty terminal::page
cargo test -p roastty terminal::page_list
cargo test -p roastty terminal_formatter
cargo test -p roastty screen_formatter
cargo test -p roastty page_string
cargo test -p roastty
```

All commands passed. The full `cargo test -p roastty` run reported `1339`
library tests passing, the ABI harness test passing, and doc-tests passing.

Codex design review found three real issues in the initial design: `CSI @` row
wrap metadata semantics were wrong, `CSI X` horizontal-margin behavior needed
explicit coverage, and protected-cell behavior needed a concrete current-scope
expectation. The design was updated with those requirements and re-reviewed
successfully before implementation:

- initial design review:
  `logs/codex-review/20260601-055259-376287-last-message.md`;
- approved design re-review:
  `logs/codex-review/20260601-055539-822019-last-message.md`.

Codex result review found two real coverage gaps in the first completed result:
`logs/codex-review/20260601-060313-567267-last-message.md`.

- `CSI X` managed-metadata release was only implied through
  `clear_active_cells()` and needed a targeted test.
- `CSI X` pending-wrap clearing was implemented indirectly through
  `cursor_reset_wrap_basic()` but lacked an explicit terminal test.

Both findings were fixed by adding the PageList erase-path metadata test and the
direct `CSI X` pending-wrap terminal test, then rerunning the full verification
chain above. Codex re-reviewed the updated result and found no remaining
blocking issues: `logs/codex-review/20260601-060707-399009-last-message.md`. The
re-review explicitly confirmed both prior blockers were resolved and that this
experiment is good enough to commit as `Pass`.

## Conclusion

`CSI @` and `CSI X` are now implemented for Roastty's current terminal model.
Together with `CSI P`, insert/delete/erase character mutation now has a coherent
single-row foundation with parser coverage, Page/PageList integrity checks, and
terminal behavior tests.

The next experiment can continue to a neighboring CSI command such as horizontal
tabulation back (`CSI Z`) or move into a larger coherent slice of remaining CSI
settings/reports if that surface is now ready to group safely.
