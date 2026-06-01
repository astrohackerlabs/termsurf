# Experiment 113: Port Escape Index and Next Line

## Description

Continue the stream/action port by adding the simple escape-sequence movement
actions:

- `ESC D` → Index (`IND`)
- `ESC E` → Next Line (`NEL`)

Upstream Ghostty maps `ESC D` to `.index` and `ESC E` to `.next_line`.
`next_line` is implemented as `index()` followed by `carriageReturn()`.

Roastty already has basic linefeed/index-style downward movement from
Experiments 106 and 109. LF, VT, and FF currently call the same internal
linefeed path, which preserves column, clears pending wrap, scrolls at the
bottom of the active screen, and dirties the affected rows. This experiment
reuses that existing basic movement for `ESC D`, then adds `ESC E` as that same
movement followed by carriage return.

This is a deliberate partial semantic port of Ghostty `IND` / `NEL`, not a full
port of Ghostty's `Terminal.index()`. Upstream `index()` is
scrolling-region-aware, left/right-margin-aware, semantic-prompt-aware, and
no-scrollback-aware. Roastty does not have those movement semantics yet, so this
experiment only connects the simple escape parser to the already-ported basic
full-screen movement model.

This experiment is intentionally narrow. It does not implement reverse index
(`ESC M`), direct C1 IND/NEL bytes, scrolling-region-aware index behavior,
left/right margin behavior, semantic prompt continuation behavior, no-scrollback
rotation, or a general escape parser.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for simple escape dispatch:
     - `ESC D` with no intermediates emits `.index`;
     - `ESC E` with no intermediates emits `.next_line`.
   - Use `vendor/ghostty/src/termio/stream_handler.zig` and
     `vendor/ghostty/src/terminal/stream_terminal.zig` for terminal routing:
     - `.index` calls `terminal.index()`;
     - `.next_line` calls `terminal.index()` and then
       `terminal.carriageReturn()`.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` to confirm that `index()`
     clears pending wrap and preserves the current column, while `next_line`
     resets the column through carriage return.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action surface.
   - Add private actions:
     - `Action::Index`
     - `Action::NextLine`
   - Keep `Action::LineFeed` unchanged. Stream LF/VT/FF should keep using the
     existing `LineFeed` action in this experiment, even though upstream routes
     linefeed through the same terminal `index()` implementation.
   - Do not expose these actions outside the terminal module.

3. Extend simple escape parsing.
   - Dispatch `ESC D` as `Action::Index`.
   - Dispatch `ESC E` as `Action::NextLine`.
   - Set the stream parser back to ground before invoking the handler for both
     actions, so a handler error cannot leave it stuck in escape state.
   - Add a minimal invalid-intermediate escape state for `ESC` followed by
     intermediate bytes (`0x20..=0x2f`). This state consumes bytes until the
     next escape final byte (`0x30..=0x7e`) and dispatches no action. This
     prevents invalid forms such as `ESC ( D` and `ESC # E` from leaking `D` or
     `E` as printable text, matching upstream's "invalid command, no action"
     behavior without implementing charset designation or `DECALN`.
   - Preserve unsupported direct escape finals: they remain consumed and ignored
     without leaking printable bytes.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by `ESC D` or `ESC E`, dispatch `U+FFFD` before the
     escape action.
   - Do not implement `ESC M` in this experiment.

4. Add terminal behavior.
   - Route `Action::Index` to the existing basic linefeed/index helper.
   - Route `Action::NextLine` to the existing basic linefeed/index helper and
     then the existing carriage-return helper.
   - `ESC D` should:
     - move down one row when not at the bottom;
     - preserve the current column;
     - clear pending wrap;
     - scroll at the active screen bottom using the existing basic scroll path.
   - `ESC E` should:
     - perform the same downward movement;
     - then set the cursor column to zero;
     - clear pending wrap;
     - scroll at the active screen bottom using the existing basic scroll path.
   - Keep the behavior scoped to the currently-ported full-screen basic movement
     model. Do not add scrolling-region-specific index behavior in this
     experiment.

5. Add tests.
   - Stream parser tests:
     - `A\x1bDB` dispatches print, index, print;
     - `A\x1bEB` dispatches print, next-line, print;
     - split-feed `ESC D` and `ESC E` dispatch the same actions;
     - pending invalid UTF-8 dispatches `U+FFFD` before same-slice and
       split-feed `ESC D` / `ESC E`;
     - handler errors from `Index` and `NextLine` leave the parser in ground
       state for the next byte;
     - invalid intermediate-bearing forms such as `ESC ( D` and `ESC # E`
       consume the final and dispatch no action or printable `D`/`E`;
     - unsupported direct escape finals still do not leak printable bytes;
     - `ESC M` remains unsupported in this experiment and does not dispatch an
       action.
     - direct C1 IND/NEL bytes (`0x84` and `0x85`) remain out of scope and do
       not dispatch `Index` / `NextLine`.
   - Terminal tests:
     - `ESC D` moves from row 0 to row 1 and preserves column;
     - `ESC E` moves from row 0 to row 1 and resets column to 0;
     - `ESC E` bypasses linefeed mode: with `Mode::Linefeed = true`, it still
       performs exactly index plus carriage return, with no extra linefeed-mode
       behavior;
     - both actions clear pending wrap at the right edge;
     - both actions scroll at the bottom row using the same behavior as LF;
     - both actions mark the same affected rows dirty as the reused basic
       linefeed path;
     - split-feed `ESC D` and `ESC E` mutate terminal state correctly.
   - Existing stream, LF/VT/FF, CR, backspace, tabstop, formatter, PageList, and
     ABI tests must keep passing.

6. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream
     cargo test -p roastty terminal_formatter
     cargo test -p roastty terminal::terminal
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
     - stream action changes;
     - exact accepted and deferred escape forms;
     - parser state behavior on handler errors;
     - terminal movement behavior;
     - pending-wrap, dirty-row, and scroll behavior;
     - what remains deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `ESC D` dispatches `Action::Index`;
- `ESC E` dispatches `Action::NextLine`;
- split-feed `ESC D` and `ESC E` dispatch the same actions;
- pending invalid UTF-8 emits `U+FFFD` before `ESC D` / `ESC E`;
- handler errors from `Index` and `NextLine` leave the parser in ground state
  for the next byte;
- invalid intermediate-bearing forms such as `ESC ( D` and `ESC # E` consume
  their finals without dispatching an action or leaking printable text;
- unsupported direct escape finals still do not leak printable bytes;
- `ESC M` remains unsupported;
- direct C1 IND/NEL bytes (`0x84` and `0x85`) remain out of scope and do not
  dispatch `Index` / `NextLine`;
- terminal `ESC D` moves down one row, preserves column, and clears pending wrap
  using the current basic full-screen movement path;
- terminal `ESC E` moves down one row, resets column to zero, and clears pending
  wrap using the current basic full-screen movement path;
- terminal `ESC E` bypasses linefeed mode and behaves as index plus carriage
  return when `Mode::Linefeed = true`;
- bottom-row `ESC D` and `ESC E` scroll like the existing LF path;
- dirty-row behavior matches the reused basic linefeed path;
- no reverse index, direct C1 IND/NEL bytes, scrolling-region-aware index,
  left/right margins, semantic prompt continuation, no-scrollback rotation,
  public API, or public ABI are added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream parsing dispatches `Index` and `NextLine`, but terminal behavior
  exposes that the existing basic linefeed/index helper must be refactored
  before both actions can reuse it cleanly.

The experiment fails if:

- `ESC D` or `ESC E` remains silently ignored;
- `D` or `E` leaks as printable text;
- handler errors leave the parser stuck in escape state;
- invalid intermediate-bearing `ESC D` / `ESC E` variants leak `D` or `E` as
  printable text;
- direct C1 IND/NEL bytes dispatch these new actions;
- `ESC D` resets the column;
- `ESC E` preserves the nonzero column;
- `ESC E` linefeed-mode behavior differs from index plus carriage return;
- either action fails to clear pending wrap;
- bottom-row behavior diverges from the existing LF path;
- `ESC M` or scrolling-region-specific behavior is accidentally implemented;
- public API or ABI changes are added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-023652-264442-prompt.md`
- Result: `logs/codex-review/20260601-023652-264442-last-message.md`

The helper reported that the stored Codex session was full and automatically
started a fresh session, as required by the review skill.

Codex found four real design gaps: intermediate-bearing invalid escape forms
needed a no-leak plan, direct C1 IND/NEL bytes needed explicit negative
verification, the result criteria needed to call this a partial semantic port of
Ghostty `index()` rather than a full port, and `ESC E` needed a linefeed-mode
interaction test. The design was updated for all four findings.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-023908-334281-prompt.md`
- Result: `logs/codex-review/20260601-023908-334281-last-message.md`

Codex found no remaining blockers and approved implementation.
