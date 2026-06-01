# Experiment 114: Port Basic CSI Cursor Movement

## Description

Continue the stream/action port by adding Ghostty's basic CSI cardinal cursor
movement actions:

- `CSI A` and `CSI k` -> cursor up (`CUU` / alias)
- `CSI B` -> cursor down (`CUD`)
- `CSI C` and `CSI a` -> cursor right (`CUF` / `HPR` alias)
- `CSI D` and `CSI j` -> cursor left (`CUB` / alias)

Upstream Ghostty parses these in `vendor/ghostty/src/terminal/stream.zig` and
routes them through `cursor_up`, `cursor_down`, `cursor_right`, and
`cursor_left`. The terminal implementation in
`vendor/ghostty/src/terminal/Terminal.zig` clamps movement to the screen or
scrolling region, treats a missing or zero count as one, and clears pending
wrap.

Roastty does not yet have scrolling-region-aware movement, origin mode, reverse
wrap, or margin-aware movement. This experiment is therefore a deliberate
partial semantic port: it implements basic full-screen cursor movement with the
same action split and count semantics, but clamps only to the current
full-screen bounds. More advanced Ghostty movement semantics remain separate
future ports.

This experiment is intentionally narrow. It does not implement `CSI E` / `CSI F`
next/previous line, `CSI G` / `CSI H` absolute positioning, scrolling regions,
origin mode, left/right margins, reverse-wrap cursor-left behavior, C1 CSI
bytes, public API, or ABI changes.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for CSI dispatch:
     - `A` and `k` emit `.cursor_up`;
     - `B` emits `.cursor_down`;
     - `C` and `a` emit `.cursor_right`;
     - `D` and `j` emit `.cursor_left`;
     - no params means count `1`;
     - one param is passed through;
     - more than one param is invalid and dispatches no action;
     - intermediates dispatch no action.
   - Use `vendor/ghostty/src/terminal/stream_terminal.zig` for terminal routing.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for basic count behavior:
     missing and zero counts move by one, oversized counts clamp, and movement
     clears pending wrap.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action surface.
   - Add private actions:
     - `Action::CursorUp { count: u16 }`
     - `Action::CursorDown { count: u16 }`
     - `Action::CursorRight { count: u16 }`
     - `Action::CursorLeft { count: u16 }`
   - Keep these internal to the terminal module.
   - Do not add public API or ABI surface.

3. Extend CSI parsing without regressing tab-clear parsing.
   - Keep the existing `CSI W` family behavior from Experiments 111 and 112:
     - `CSI W` and `CSI 0 W` -> tab set;
     - `CSI 2 W` -> clear current tab;
     - `CSI 5 W` -> clear all tabs;
     - `CSI ? 5 W` -> tab reset;
     - unsupported `W` variants dispatch no tab action.
   - Add a small shared CSI parameter parser that can represent:
     - no params;
     - a single numeric param;
     - a leading private marker such as `?`;
     - invalid input such as semicolons or unsupported private markers.
   - Match Ghostty's numeric overflow behavior: saturate overflowing numeric
     params at `u16::MAX`, then let terminal movement clamp to the screen edge.
     Do not treat numeric overflow as an invalid cursor command.
   - Dispatch cursor movement only when the CSI has no private marker, no
     intermediates, and zero or one numeric param.
   - Treat missing param and explicit `0` as movement count `1`, matching
     Ghostty terminal behavior.
   - Treat multiple params as invalid and dispatch no action.
   - Set the stream parser back to ground before invoking the handler, so a
     handler error cannot leave it stuck in CSI state.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by a cursor CSI, dispatch `U+FFFD` before the
     cursor action.

4. Add basic terminal movement helpers.
   - Add private full-screen helpers on `Screen` or `Terminal` for:
     - cursor up;
     - cursor down;
     - cursor right;
     - cursor left.
   - Missing or zero count should move one cell/row.
   - Oversized counts should clamp to the full-screen edge.
   - Movement should clear pending wrap.
   - Movement should not write cells, scroll, or dirty rows.
   - Cursor-left should use no-wrap behavior only: clamp at column zero. Do not
     implement Ghostty's reverse-wrap or reverse-wrap-extended behavior in this
     experiment.
   - Cursor-up/down should clamp to the full-screen top/bottom. Do not implement
     scrolling-region or origin-mode behavior in this experiment.

5. Add tests.
   - Stream parser tests:
     - `A\x1b[AB`, `A\x1b[BB`, `A\x1b[CB`, and `A\x1b[DB` dispatch print, cursor
       action with count `1`, print;
     - aliases `CSI k`, `CSI a`, and `CSI j` dispatch the same actions as
       `CSI A`, `CSI C`, and `CSI D`;
     - explicit counts such as `CSI 5 C` dispatch count `5`;
     - explicit zero counts dispatch count `1`;
     - split-feed CSI cursor movement dispatches correctly;
     - pending invalid UTF-8 emits `U+FFFD` before same-slice and split-feed CSI
       cursor actions;
     - handler errors from all four cursor actions leave the parser in ground
       state for the next byte;
     - private, intermediate-bearing, multi-param, and overflowing cursor CSI
       sequences do not leak printable bytes;
     - overflowing cursor CSI params saturate to `u16::MAX` and then clamp at
       the terminal edge;
     - direct C1 CSI byte `0x9b` remains out of scope and follows the current
       raw-C1 UTF-8 replacement behavior; for example, `0x9b A` should dispatch
       `U+FFFD` and printable `A`, not `CursorUp`;
     - existing `CSI W` tab actions and unsupported `W` variants still behave as
       before.
   - Terminal tests:
     - cursor up/down/right/left move by one with default count;
     - explicit counts move the requested amount;
     - count zero moves by one;
     - oversized counts clamp to the full-screen edge;
     - each direction clears pending wrap;
     - movement does not modify cells or dirty rows;
     - cursor-left clamps at column zero without reverse wrap;
     - cursor-up/down clamp to top/bottom without scrolling;
     - split-feed CSI movement mutates terminal state correctly.
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
     - accepted cursor CSI forms;
     - rejected cursor CSI forms;
     - count parsing and clamping behavior;
     - parser state behavior on handler errors;
     - terminal pending-wrap, dirty-row, and no-scroll behavior;
     - confirmation that `CSI W` tab behavior did not regress;
     - what remains deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `CSI A` / `CSI k` dispatch `CursorUp`;
- `CSI B` dispatches `CursorDown`;
- `CSI C` / `CSI a` dispatch `CursorRight`;
- `CSI D` / `CSI j` dispatch `CursorLeft`;
- missing and zero params move by count `1`;
- one numeric param dispatches that count;
- invalid private, intermediate, and multi-param cursor CSI forms dispatch no
  cursor action and do not leak printable bytes;
- overflowing cursor CSI params saturate to `u16::MAX` and clamp at the terminal
  edge instead of becoming invalid;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior instead of dispatching cursor actions;
- handler errors leave the parser in ground state;
- pending invalid UTF-8 emits `U+FFFD` before cursor CSI actions;
- existing `CSI W` tab behavior remains unchanged;
- terminal movement clamps to full-screen bounds, clears pending wrap, does not
  scroll, and does not dirty rows;
- cursor-left does not implement reverse wrap;
- cursor-up/down do not implement scrolling regions or origin mode;
- no `CSI E` / `CSI F` / `CSI G` / `CSI H`, direct C1 CSI bytes, public API, or
  ABI changes are added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream parsing works, but terminal movement exposes that the current cursor or
  dirty-row helpers need a separate prerequisite refactor before all four
  directions can be routed cleanly.

The experiment fails if:

- any accepted cursor CSI form remains silently ignored;
- cursor CSI finals leak as printable text;
- invalid cursor CSI variants dispatch cursor actions;
- handler errors leave the parser stuck in CSI state;
- `CSI W` tab behavior regresses;
- movement fails to clear pending wrap;
- movement writes cells, dirties rows, or scrolls;
- reverse wrap, scrolling-region, origin-mode, CNL/CPL/CUP, public API, or ABI
  behavior is accidentally added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-025053-813792-prompt.md`
- Result: `logs/codex-review/20260601-025053-813792-last-message.md`

Codex found two real design blockers: cursor CSI numeric overflow should
saturate like Ghostty instead of becoming invalid, and direct C1 CSI byte `0x9b`
needed explicit negative coverage. The design was updated for both findings.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-025227-293874-prompt.md`
- Result: `logs/codex-review/20260601-025227-293874-last-message.md`

The helper reported that the stored Codex session was full and automatically
started a fresh session, as required by the review skill.

Codex found no remaining design blockers and approved implementation. It also
suggested making the `0x9b` negative assertion concrete; the design now states
that `0x9b A` should follow raw-C1 UTF-8 replacement behavior and dispatch
`U+FFFD` plus printable `A`, not `CursorUp`.
