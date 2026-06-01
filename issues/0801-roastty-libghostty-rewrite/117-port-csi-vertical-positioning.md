# Experiment 117: Port CSI Vertical Positioning

## Description

Continue the stream/action port by adding Ghostty's single-axis vertical cursor
positioning forms:

- `CSI d` -> vertical position absolute (`VPA`)
- `CSI e` -> vertical position relative (`VPR`)

Upstream Ghostty parses these in `vendor/ghostty/src/terminal/stream.zig`:

- final `d` emits `.cursor_row`;
- final `e` emits `.cursor_row_relative`;
- no params means row/value `1`;
- one param is passed through;
- more than one param is invalid and dispatches no action;
- intermediates dispatch no action.

Upstream routing in `vendor/ghostty/src/terminal/stream_terminal.zig` maps:

- `.cursor_row` to `Terminal.setCursorPos(value, current_x + 1)`;
- `.cursor_row_relative` to
  `Terminal.setCursorPos(current_y + 1 +| value, current_x + 1)`.

That means both forms preserve the current column and use `setCursorPos` for
final clamping and pending-wrap clearing. `CSI d` is absolute and 1-indexed:
explicit row `0` is passed to the terminal and resolved to the top edge. `CSI e`
is relative: explicit value `0` is passed through as zero relative movement, so
it should not behave like `CSI B` / cursor-down, whose explicit zero means one
row.

Roastty does not yet have origin mode, scrolling-region-aware positioning, or
full `setCursorPos` parity. This experiment ports the parser boundary and basic
full-screen terminal behavior only:

- `CSI d` sets the row absolutely and preserves the current column;
- `CSI e` moves down by the requested relative value and preserves the current
  column;
- missing params resolve to `1`;
- explicit zero is preserved by the parser;
- oversized values clamp to the bottom edge;
- pending wrap is cleared;
- no cells are written, no rows are dirtied, and no scrolling occurs.

This experiment is intentionally narrow. It does not implement `CSI H` / `CSI f`
cursor positioning, two-parameter CSI parsing beyond rejecting it, origin mode,
scrolling-region-aware positioning, reverse index, direct C1 CSI bytes, public
API, or ABI changes.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for CSI dispatch:
     - final `d` emits `.cursor_row`;
     - final `e` emits `.cursor_row_relative`;
     - no params means value `1`;
     - one param is passed through;
     - more than one param is invalid and dispatches no action;
     - intermediates dispatch no action.
   - Use `vendor/ghostty/src/terminal/stream_terminal.zig` for routing.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for basic row semantics:
     row/value `0` is resolved by `setCursorPos`, rows are 1-indexed, oversized
     rows clamp, and positioning clears pending wrap.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action surface.
   - Add private actions:
     - `Action::CursorRow { row: u16 }`
     - `Action::CursorRowRelative { rows: u16 }`
   - Keep them internal to the terminal module.
   - Do not add public API or ABI surface.

3. Extend CSI dispatch.
   - Reuse the existing numeric CSI parser storage, but add a helper for
     absolute/relative positioning params that preserves explicit zero.
   - Do not reuse the relative-movement `movement_count()` helper. Relative
     movement treats zero as one in the parser, but row positioning must
     preserve explicit zero until terminal positioning resolves it.
   - Dispatch `CSI d` and `CSI e` only when there is no private marker, no
     intermediate byte, and zero or one numeric param.
   - Missing param should dispatch value `1`.
   - Explicit `0` should dispatch value `0`.
   - Numeric overflow should continue saturating at `u16::MAX`, then terminal
     positioning clamps to the bottom edge.
   - Private, intermediate-bearing, semicolon-param, colon-param, and other
     invalid forms should dispatch no action and should not leak printable final
     bytes.
   - Keep the parser transition to ground before invoking the handler, so a
     handler error cannot leave it stuck in CSI state.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by `CSI d` or `CSI e`, dispatch `U+FFFD` before the
     cursor-row action.
   - Preserve all existing `CSI A/B/C/D/E/F/G/k/a/j/backtick` and `CSI W`
     behavior.

4. Add basic terminal positioning behavior.
   - Add private full-screen helpers on `Screen` or `Terminal` for:
     - vertical absolute positioning;
     - vertical relative positioning.
   - `CSI d` behavior should:
     - keep the current column unchanged;
     - treat row `0` as row `1`;
     - convert the 1-indexed row to zero-indexed cursor `y`;
     - clamp oversized rows to the bottom edge;
     - clear pending wrap;
     - avoid writing cells, dirtying rows, or scrolling.
   - `CSI e` behavior should:
     - keep the current column unchanged;
     - treat missing value as `1`;
     - treat explicit value `0` as zero relative movement;
     - move down by the relative value;
     - clamp oversized movement to the bottom edge;
     - clear pending wrap;
     - avoid writing cells, dirtying rows, or scrolling.
   - Do not implement origin-mode or scrolling-region-aware behavior in this
     experiment.

5. Add tests.
   - Stream parser tests:
     - `A\x1b[dB` dispatches print `A`, `CursorRow { row: 1 }`, print `B`;
     - `A\x1b[eB` dispatches print `A`, `CursorRowRelative { rows: 1 }`, print
       `B`;
     - explicit values such as `CSI 5 d` and `CSI 6 e` dispatch those values;
     - explicit zero dispatches value `0` for both forms;
     - overflowing values dispatch `u16::MAX`;
     - split-feed `CSI d` and `CSI e` dispatch correctly;
     - pending invalid UTF-8 emits `U+FFFD` before same-slice and split-feed
       cursor-row actions;
     - invalid private, intermediate-bearing, semicolon-param, colon-param, and
       multi-param forms dispatch no action and do not leak printable final
       bytes;
     - direct C1 CSI byte `0x9b` followed by `d` or `e` remains out of scope and
       dispatches `U+FFFD` plus printable `d` / `e`, not cursor positioning;
     - handler errors from `CursorRow` / `CursorRowRelative` leave the parser in
       ground state;
     - existing cursor and tab CSI behavior still behaves as before.
   - Terminal tests:
     - `CSI d` moves to the top row with default param and keeps the column
       unchanged;
     - `CSI e` moves down by one with default param and keeps the column
       unchanged;
     - explicit absolute rows move to the requested 1-indexed row converted to
       zero-indexed cursor `y`;
     - explicit relative values move down by that value;
     - explicit zero for `CSI d` moves to the top edge;
     - explicit zero for `CSI e` leaves the row unchanged but clears pending
       wrap;
     - oversized absolute and relative values clamp to the bottom edge;
     - positioning does not modify cells, dirty rows, or scroll;
     - split-feed `CSI d` / `CSI e` mutates terminal state correctly.
   - Existing stream, movement, tabstop, formatter, PageList, and ABI tests must
     keep passing.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream
     cargo test -p roastty terminal::terminal
     cargo test -p roastty terminal_formatter
     cargo test -p roastty screen_formatter
     cargo test -p roastty page_string
     cargo test -p roastty terminal::page_list
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

7. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - Commit the approved design before implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.
   - Commit the approved result separately from the design commit.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - accepted `CSI d` / `CSI e` forms;
     - rejected `CSI d` / `CSI e` forms;
     - parser/terminal split for explicit zero;
     - count parsing and clamping behavior;
     - parser state behavior on handler errors;
     - terminal pending-wrap, dirty-row, no-scroll, and column-preservation
       behavior;
     - confirmation that existing cursor and tab CSI behavior did not regress;
     - what remains deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `CSI d` dispatches `CursorRow`;
- `CSI e` dispatches `CursorRowRelative`;
- missing params dispatch value `1`;
- explicit zero dispatches value `0`;
- `CSI d` explicit zero moves to the top edge;
- `CSI e` explicit zero preserves the current row and clears pending wrap;
- one numeric param dispatches that value;
- overflowing numeric params saturate to `u16::MAX` and clamp at the terminal
  bottom edge;
- invalid private, intermediate, semicolon-param, colon-param, and multi-param
  forms dispatch no cursor-row action and do not leak printable bytes;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior instead of dispatching cursor actions;
- pending invalid UTF-8 emits `U+FFFD` before cursor-row actions;
- handler errors leave the parser in ground state;
- existing `CSI A/B/C/D/E/F/G/k/a/j/backtick` cursor behavior remains unchanged;
- existing `CSI W` tab behavior remains unchanged;
- terminal behavior keeps the column unchanged, clamps to full-screen vertical
  bounds, clears pending wrap, and does not scroll, dirty rows, or write cells;
- no `CSI H` / `CSI f` / two-parameter cursor positioning, origin-mode,
  scrolling-region, direct C1 CSI, public API, or ABI behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream parsing works, but the current screen cursor helpers need a small
  prerequisite refactor before vertical positioning can be routed cleanly
  without mutating cells or dirty rows.

The experiment fails if:

- `CSI d` or `CSI e` remains silently ignored;
- either final leaks as printable text;
- invalid variants dispatch cursor-row actions;
- handler errors leave the parser stuck in CSI state;
- existing cursor or tab CSI behavior regresses;
- terminal behavior changes the column, fails to clear pending wrap, fails to
  clamp to the screen, writes cells, dirties rows, or scrolls;
- `CSI H` / `CSI f`, origin-mode, scrolling-region, direct C1 CSI, public API,
  or ABI behavior is accidentally added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-032039-303406-prompt.md`
- Result: `logs/codex-review/20260601-032039-303406-last-message.md`

The helper reported that the stored Codex session was full and automatically
started a fresh session, as required by the review skill.

Codex found no blocking design issues and approved `CSI d` / `CSI e` as the
right next slice. It confirmed the zero semantics:

- `CSI d` explicit zero is passed by the parser and resolved by terminal
  positioning to the top row.
- `CSI e` explicit zero is passed by the parser, preserves the current row, and
  still clears pending wrap.

Codex also recommended a non-blocking implementation naming constraint: use a
helper name such as `position_value()` rather than a movement/count name, so the
future `CSI H` / `CSI f` work can reuse the same explicit-zero semantics without
confusing it with relative cursor movement.

## Result

**Result:** Pass

Experiment 117 ports the basic full-screen forms of `CSI d` / VPA and `CSI e` /
VPR into Roastty.

Accepted forms:

- `CSI d` dispatches `Action::CursorRow { row: 1 }`.
- `CSI e` dispatches `Action::CursorRowRelative { rows: 1 }`.
- `CSI n d` dispatches the explicit row value.
- `CSI n e` dispatches the explicit relative row value.
- `CSI 0 d` preserves explicit zero at the parser boundary and resolves to the
  top row in terminal positioning.
- `CSI 0 e` preserves explicit zero at the parser boundary, leaves the row
  unchanged, and still clears pending wrap.
- Oversized numeric params saturate to `u16::MAX` in the parser and clamp to the
  bottom row in the terminal.

Rejected forms:

- private variants such as `CSI ? 3 d` / `CSI ? 3 e`;
- non-standard private variants such as `CSI > 3 d` / `CSI > 3 e`;
- semicolon and colon parameter forms;
- intermediate-bearing forms;
- multi-parameter forms;
- raw C1 `0x9b` followed by `d` or `e`, which remains out of scope and keeps the
  current replacement-character behavior.

The implementation keeps the parser/terminal split from the design. The parser
uses the explicit-zero-preserving `position_value()` helper for column and row
positioning. The terminal resolves row semantics with private full-screen screen
helpers, preserving the current column, clearing pending wrap, clamping to the
vertical screen bounds, and avoiding cell writes, dirty rows, or scrolling.

Parser error behavior was preserved: if a handler fails on `CursorRow` or
`CursorRowRelative`, the stream is already back in ground state and the next
printable byte is parsed normally. Pending invalid UTF-8 still emits `U+FFFD`
before same-slice and split-feed `CSI d` / `CSI e` actions.

Existing behavior for `CSI A/B/C/D/E/F/G/k/a/j/backtick` and `CSI W` continued
to pass. This experiment did not add `CSI H` / `CSI f`, two-parameter cursor
positioning, origin mode, scrolling-region-aware positioning, direct C1 CSI,
public API, or ABI behavior.

Verification passed:

```text
cargo fmt
cargo test -p roastty stream
cargo test -p roastty terminal::terminal
cargo test -p roastty terminal_formatter
cargo test -p roastty screen_formatter
cargo test -p roastty page_string
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

The full package test run passed with 1143 unit tests and the ABI harness.

Codex reviewed the completed implementation with no findings.

Result review artifacts:

- Prompt: `logs/codex-review/20260601-032809-434700-prompt.md`
- Result: `logs/codex-review/20260601-032809-434700-last-message.md`

## Conclusion

Roastty now has the next single-axis cursor-positioning slice from Ghostty:
absolute vertical positioning and relative vertical positioning. The important
semantic detail is locked in for later cursor-positioning work: row and column
positioning preserve explicit zero through the parser and let terminal
positioning resolve it, rather than treating zero as a movement count.

The next likely stream/action slice is two-parameter cursor positioning (`CSI H`
/ `CSI f`) using the same explicit-zero-preserving parameter semantics.
