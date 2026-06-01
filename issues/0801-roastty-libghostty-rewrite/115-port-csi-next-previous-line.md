# Experiment 115: Port CSI Next and Previous Line

## Description

Continue the stream/action port by adding Ghostty's CSI next-line and
previous-line cursor movement forms:

- `CSI E` -> cursor next line (`CNL`)
- `CSI F` -> cursor previous line (`CPL`)

Upstream Ghostty parses these in `vendor/ghostty/src/terminal/stream.zig`.
Unlike `CSI A` / `CSI B`, each sequence emits two handler actions in order:

1. `CSI E` emits `.cursor_down` with the requested count, then
   `.carriage_return`.
2. `CSI F` emits `.cursor_up` with the requested count, then `.carriage_return`.

Roastty already has the basic full-screen cursor up/down helpers from Experiment
114 and the basic carriage-return helper from Experiment 106. This experiment
should connect the parser to those existing behaviors without adding new
cursor-positioning semantics.

This is a parser-boundary and basic-terminal-behavior slice. It intentionally
inherits Experiment 114's simplified full-screen cursor movement. Upstream
Ghostty's `Terminal.cursorUp` and `Terminal.cursorDown` are scrolling-region
aware when the cursor is inside the region; Roastty does not yet have that
semantic layer. Scrolling-region and origin-mode parity remain deferred to the
future experiment that ports those movement semantics for all cursor-up/down
callers, not just `CSI E` / `CSI F`.

This experiment is intentionally narrow. It does not implement `CSI G` and
`CSI \`` column positioning, `CSI H`and`CSI f`cursor positioning,`CSI d`and`CSI
e` row movement, scrolling-region-aware movement, origin mode, left/right
margins, direct C1 CSI bytes, public API, or ABI changes.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for CSI dispatch:
     - final `E` with no intermediates emits `.cursor_down` and then
       `.carriage_return`;
     - final `F` with no intermediates emits `.cursor_up` and then
       `.carriage_return`;
     - no params means count `1`;
     - one param is passed through;
     - more than one param is invalid and dispatches no action;
     - intermediates dispatch no action.
   - Use `vendor/ghostty/src/terminal/stream_terminal.zig` for terminal routing.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for basic count behavior:
     missing and zero counts move by one, oversized counts clamp, and cursor
     movement clears pending wrap.
   - Do not claim full `Terminal.zig` movement parity in this experiment.
     Ghostty's scrolling-region-aware up/down behavior is real, but Roastty's
     current up/down movement is still full-screen only by design.
   - Do not modify `vendor/ghostty/`.

2. Extend CSI dispatch to support an ordered action pair.
   - Keep the existing private `Action` variants from previous experiments.
   - Do not add a combined `CursorNextLine` or `CursorPreviousLine` action.
     Ghostty dispatches `CNL` and `CPL` as two handler actions, and Roastty
     should preserve that ordering at the parser boundary.
   - Replace the current single-action CSI helper with a small internal dispatch
     representation that can express:
     - no action;
     - one action;
     - two ordered actions.
   - `CSI E` should dispatch:
     - `Action::CursorDown { count }`
     - `Action::CarriageReturn`
   - `CSI F` should dispatch:
     - `Action::CursorUp { count }`
     - `Action::CarriageReturn`
   - Keep the stream parser transition to ground before invoking either handler
     action.
   - If the first handler action returns an error, return that error and do not
     invoke the second action.
   - If the second handler action returns an error, return that error.
   - In both error cases, the parser must already be back in ground state.

3. Preserve existing CSI behavior.
   - Keep Experiment 114 cursor movement behavior unchanged:
     - `CSI A` / `CSI k` -> cursor up;
     - `CSI B` -> cursor down;
     - `CSI C` / `CSI a` -> cursor right;
     - `CSI D` / `CSI j` -> cursor left.
   - Keep Experiments 111 and 112 tab behavior unchanged:
     - `CSI W` and `CSI 0 W` -> tab set;
     - `CSI 2 W` -> clear current tab;
     - `CSI 5 W` -> clear all tabs;
     - `CSI ? 5 W` -> tab reset.
   - Keep numeric overflow behavior from Experiment 114: params saturate at
     `u16::MAX`, then terminal movement clamps to the screen edge.
   - Keep direct C1 CSI byte `0x9b` out of scope. It should continue to follow
     Roastty's current raw-C1 UTF-8 replacement behavior.

4. Use existing terminal movement behavior.
   - Do not add new screen helpers unless the implementation clearly needs a
     tiny shared helper to avoid duplicated test-only plumbing.
   - Terminal behavior for `CSI E` should be equivalent to:
     - cursor down by count, clamped to the full-screen bottom;
     - clear pending wrap through cursor-down behavior;
     - set cursor column to zero through carriage return.
   - Terminal behavior for `CSI F` should be equivalent to:
     - cursor up by count, clamped to the full-screen top;
     - clear pending wrap through cursor-up behavior;
     - set cursor column to zero through carriage return.
   - Do not dirty rows, write cells, scroll, or implement scroll-region/origin
     semantics in this experiment.
   - No-dirty-row behavior is an invariant of cursor movement here; it is not a
     consequence of full-screen clamping. A later scrolling-region-aware cursor
     movement port should preserve the same no-scroll/no-dirty-row property for
     `CSI A/B/E/F`.

5. Add tests.
   - Stream parser tests:
     - `A\x1b[EB` dispatches print `A`, `CursorDown { count: 1 }`,
       `CarriageReturn`, print `B`;
     - `A\x1b[FB` dispatches print `A`, `CursorUp { count: 1 }`,
       `CarriageReturn`, print `B`;
     - explicit counts such as `CSI 5 E` and `CSI 3 F` dispatch the requested
       count followed by carriage return;
     - explicit zero counts dispatch count `1` followed by carriage return;
     - overflowing counts saturate to `u16::MAX` followed by carriage return;
     - split-feed `CSI E` and `CSI F` dispatch both actions correctly;
     - pending invalid UTF-8 emits `U+FFFD` before the cursor action pair for
       same-slice and split-feed cases;
     - invalid private, intermediate-bearing, and multi-param `CSI E` / `CSI F`
       forms dispatch no action and do not leak printable final bytes;
     - invalid semicolon and colon parameter forms such as `CSI 1 ; 2 E`,
       `CSI 1 ; 2 F`, `CSI 1 : 2 E`, and `CSI 1 : 2 F` dispatch no action and do
       not leak printable final bytes;
     - direct C1 CSI byte `0x9b` followed by `E` or `F` remains out of scope and
       dispatches `U+FFFD` plus printable `E` / `F`, not cursor movement;
     - handler errors from the first action leave the parser in ground state and
       do not invoke the second action;
     - handler errors from the second action leave the parser in ground state;
     - existing `CSI A/B/C/D/k/a/j` cursor movement and `CSI W` tab actions
       still behave as before.
   - Terminal tests:
     - `CSI E` moves down by one and sets column to zero;
     - `CSI F` moves up by one and sets column to zero;
     - explicit and zero counts use the same count semantics as Experiment 114;
     - oversized counts clamp to the top or bottom edge and still set column to
       zero;
     - both sequences clear pending wrap;
     - both sequences do not modify cells, dirty rows, or scroll;
     - `CSI E` at the bottom row clamps and returns to column zero without
       behaving like `ESC E` / next-line scrolling;
     - `CSI F` at the top row clamps and returns to column zero without behaving
       like reverse index;
     - split-feed `CSI E` / `CSI F` mutate terminal state correctly.
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
     - accepted `CSI E` / `CSI F` forms;
     - rejected `CSI E` / `CSI F` forms;
     - action ordering;
     - count parsing and clamping behavior;
     - parser state behavior on first-action and second-action handler errors;
     - terminal pending-wrap, dirty-row, no-scroll, and carriage-return
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

- `CSI E` dispatches `CursorDown` followed by `CarriageReturn`;
- `CSI F` dispatches `CursorUp` followed by `CarriageReturn`;
- missing and zero params move by count `1`;
- one numeric param dispatches that count;
- overflowing numeric params saturate to `u16::MAX` and clamp at the terminal
  edge;
- invalid private, intermediate, semicolon-param, colon-param, and multi-param
  `CSI E` / `CSI F` forms dispatch no cursor action and do not leak printable
  bytes;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior instead of dispatching cursor actions;
- pending invalid UTF-8 emits `U+FFFD` before the action pair;
- handler errors from either action leave the parser in ground state;
- a first-action handler error prevents the second action from running;
- existing `CSI A/B/C/D/k/a/j` cursor behavior remains unchanged;
- existing `CSI W` tab behavior remains unchanged;
- terminal behavior moves vertically, clamps to full-screen bounds, clears
  pending wrap, and returns to column zero;
- terminal behavior does not scroll, dirty rows, or write cells;
- bottom-row `CSI E` clamps and returns to column zero without behaving like
  `ESC E` / next-line scrolling;
- top-row `CSI F` clamps and returns to column zero without behaving like
  reverse index;
- scrolling-region-aware Ghostty movement parity remains explicitly deferred as
  part of the broader cursor-up/down semantic port;
- no `CSI G` / `CSI H` / absolute positioning, scrolling-region, origin-mode,
  direct C1 CSI, public API, or ABI behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream parsing works, but the current single-action parser plumbing requires a
  larger refactor before ordered two-action CSI dispatch can be implemented
  cleanly.

The experiment fails if:

- `CSI E` or `CSI F` remains silently ignored;
- either final leaks as printable text;
- invalid variants dispatch cursor actions;
- handler errors leave the parser stuck in CSI state;
- a first-action handler error still invokes carriage return;
- existing cursor or tab CSI behavior regresses;
- terminal movement fails to return to column zero;
- movement writes cells, dirties rows, or scrolls;
- absolute positioning, scroll-region, origin-mode, direct C1 CSI, public API,
  or ABI behavior is accidentally added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-030415-847085-prompt.md`
- Result: `logs/codex-review/20260601-030415-847085-last-message.md`

The helper reported that the stored Codex session was full and automatically
started a fresh session, as required by the review skill.

Codex approved `CSI E` / `CSI F` as the right next parser slice and agreed that
two ordered handler actions match upstream Ghostty's dispatch boundary. It found
one real ambiguity: the design cited Ghostty terminal behavior while also
intentionally inheriting Experiment 114's simplified full-screen movement. The
design now states that this experiment is a parser-boundary and
basic-terminal-behavior slice, and that scrolling-region-aware cursor movement
parity is deferred to a future broader movement experiment.

Codex also recommended explicit regression coverage that `CSI E` is not
implemented as `ESC E` / next-line scrolling, plus explicit semicolon and colon
invalid-parameter examples. The design was updated for both findings.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-030602-941958-prompt.md`
- Result: `logs/codex-review/20260601-030602-941958-last-message.md`

Codex found no remaining blockers and approved implementation. It noted one
minor spacing typo in the non-goals sentence; that typo was fixed before the
design commit.

## Result

**Result:** Pass

Implemented the `CSI E` and `CSI F` parser slice:

- `CSI E` dispatches `Action::CursorDown { count }` followed by
  `Action::CarriageReturn`.
- `CSI F` dispatches `Action::CursorUp { count }` followed by
  `Action::CarriageReturn`.

The stream parser now uses a small internal `CsiDispatch` representation for
zero, one, or two ordered actions. This preserves the existing single-action
paths for `CSI A/B/C/D/k/a/j` cursor movement and the `CSI W` tab family, while
allowing `CSI E/F` to mirror Ghostty's two-handler-action dispatch boundary. The
parser transitions to ground before invoking either action. If the first action
errors, the second action is skipped; if the second action errors, the error is
returned; in both cases the next byte is parsed from ground state.

Accepted forms:

- `CSI E` / `CSI F`;
- `CSI 0 E` / `CSI 0 F`, treated as count `1`;
- `CSI n E` / `CSI n F`, with numeric params saturating at `u16::MAX` before
  terminal movement clamps to the screen edge.

Rejected forms:

- private forms such as `CSI ? 3 E` and `CSI ? 3 F`;
- unsupported private markers such as `CSI > 3 E` and `CSI > 3 F`;
- semicolon params such as `CSI 5 ; 4 E` and `CSI 5 ; 4 F`;
- colon params such as `CSI 1 : 2 E` and `CSI 1 : 2 F`;
- intermediate-bearing forms such as `CSI SP E` and `CSI SP F`.

Rejected forms dispatch no cursor action and do not leak printable final bytes.
Direct C1 CSI byte `0x9b` remains out of scope and follows the current raw-C1
UTF-8 replacement behavior.

Terminal behavior uses the existing basic movement helpers from Experiment 114:

- `CSI E` moves down, clamps to the full-screen bottom, clears pending wrap, and
  returns to column zero.
- `CSI F` moves up, clamps to the full-screen top, clears pending wrap, and
  returns to column zero.
- Neither sequence writes cells, dirties rows, or scrolls.
- Bottom-row `CSI E` does not behave like `ESC E` / next-line scrolling.
- Top-row `CSI F` does not behave like reverse index.

Existing `CSI A/B/C/D/k/a/j` cursor behavior and `CSI W` tab behavior did not
regress. Scrolling-region-aware cursor movement, origin mode, absolute
positioning, direct C1 CSI, public API, and ABI behavior remain deferred.

Verification:

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

All commands passed. The full `cargo test -p roastty` run reported 1114 unit
tests passed, the ABI harness passed, and doc-tests had zero tests.

Codex result review artifacts:

- Prompt: `logs/codex-review/20260601-030935-070184-prompt.md`
- Result: `logs/codex-review/20260601-030935-070184-last-message.md`

Codex found no implementation blockers and approved recording a Pass.

## Conclusion

Experiment 115 completed the next adjacent CSI cursor-movement slice. Roastty
now parses `CSI E` and `CSI F` with Ghostty's ordered handler-action boundary
while intentionally preserving the current simplified full-screen movement
semantics. The next cursor parser slice can move to absolute/row/column
positioning (`CSI G`, `CSI H`, `CSI f`, `CSI d`, `CSI e`) or first broaden
cursor movement to Ghostty's scrolling-region-aware semantics.
