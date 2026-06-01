# Experiment 106: Port Basic LF and CR

## Description

Continue the `Terminal.print()` path by adding the first C0 control actions that
real terminal text needs: line feed (`LF`, `0x0A`) and carriage return (`CR`,
`0x0D`).

Experiments 103-105 made printable bytes mutate the active screen, wrap, and
scroll. The stream parser still ignores all C0 controls, so input like
`hello\r\nworld` cannot create a normal two-line terminal display. Upstream
Ghostty treats these as parser execute actions handled by `StreamHandler`: `LF`
calls `Terminal.index()` and `CR` calls `Terminal.carriageReturn()`.

This experiment ports only the active full-width subset of that behavior:

- `LF` moves the cursor down one active row, scrolling/growing at the active
  bottom row using the same scrollback path as Experiment 105;
- `CR` moves the cursor to active column 0;
- both clear pending wrap;
- `LF` marks old and new visible rows dirty due to cursor movement;
- `CR` does not mark cells dirty because it does not change rendered content.

This experiment does not implement backspace, tabs, next-line (`NEL`), reverse
index (`RI`), parser C1 controls, linefeed mode configuration, origin mode,
left/right margins, vertical scroll regions, no-scrollback row rotation, styles,
hyperlinks, wide characters, Unicode width, public API, or public ABI.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Parser.zig` for C0 `execute` action shape.
   - Use `vendor/ghostty/src/termio/stream_handler.zig` for:
     - `.linefeed`;
     - `.carriage_return`;
     - `linefeed()`;
     - `index()`;
     - `nextLine()` only as a non-goal reference.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for:
     - `linefeed()`;
     - `carriageReturn()`;
     - `index()`;
     - `Terminal: linefeed and carriage return`;
     - `Terminal: linefeed unsets pending wrap`;
     - `Terminal: carriage return unsets pending wrap`.
   - Do not modify `vendor/ghostty/`.

2. Extend the stream action surface privately.
   - Add private `Action::LineFeed` and `Action::CarriageReturn` variants.
   - In ground state, dispatch:
     - `0x0A` as `LineFeed`;
     - `0x0D` as `CarriageReturn`.
   - Keep other C0 controls ignored in this experiment.
   - Preserve Experiment 102 behavior: if a pending invalid UTF-8 sequence is
     interrupted by `LF` or `CR`, dispatch `U+FFFD` before dispatching the
     control action.

3. Add active full-width cursor movement helpers.
   - Add a private `Screen` helper for line feed / index on the active
     full-width screen:
     - if `cursor.y < rows - 1`, mark the old active row dirty, move down one
       row, clear `pending_wrap`, and mark the new active row dirty;
     - if `cursor.y == rows - 1`, grow/scroll the `PageList` by one row before
       mutating cursor, pending-wrap, or dirty state; after successful growth,
       preserve active `cursor.y == rows - 1`, clear `pending_wrap`, and mark
       the visible active rows dirty;
     - keep `cursor.x` unchanged.
   - Keep bottom-row grow failure transactional: if grow fails, return the
     private allocation error without changing cursor position, `pending_wrap`,
     row dirty state, or cell contents.
   - Add a private `Screen` helper for carriage return:
     - clear `pending_wrap`;
     - set `cursor.x = 0`;
     - do not mark rows dirty.
   - Do not add public API or ABI.

4. Wire terminal handling.
   - In `TerminalStreamHandler::vt`, route `Action::LineFeed` to the line-feed
     helper and `Action::CarriageReturn` to the carriage-return helper.
   - If the existing private error type needs a grow/allocation error variant,
     reuse the `PageAlloc` shape from Experiment 105.
   - Do not implement linefeed mode's automatic CR unless the existing
     `ModeState` already exposes the mode cleanly to the handler without
     widening the experiment. If implemented, add a direct unit test; otherwise
     explicitly record it as deferred in the result.

5. Add tests.
   - Stream parser tests:
     - `A\nB\rC` dispatches print, linefeed, print, carriage-return, print in
       order;
     - other C0 controls besides `LF` and `CR` remain ignored;
     - pending invalid UTF-8 dispatches `U+FFFD` before `LF` / `CR`.
   - Terminal tests:
     - `hello\r\nworld` formats as `hello\nworld`;
     - bare `LF` preserves the column, so `A\nB` formats as `A\n B`;
     - `LF` clears pending wrap without first soft-wrapping;
     - non-bottom `LF` dirties both the old and new active rows after the test
       clears prior dirt;
     - `CR` clears pending wrap and moves to column 0 without dirtying rows,
       verified by clearing dirty state before issuing `CR`;
     - `LF` on the bottom active row grows/scrolls and preserves history, using
       the test-only full-screen dump from Experiment 105;
     - bottom-row `LF` preserves a nonzero cursor column across the scroll;
     - bottom-row `LF` dirties the visible active rows after the test clears
       prior dirt;
     - split-feed `CRLF` works when `\r` and `\n` arrive in separate
       `next_slice` calls.
   - Existing printable, pending-wrap, wrap-scroll, formatter, PageList, and
     stream tests must keep passing.

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
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.

8. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - stream action changes;
     - line-feed/index behavior;
     - carriage-return behavior;
     - pending-wrap behavior;
     - bottom-row scroll behavior;
     - what remains deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- the stream parser dispatches `LF` and `CR` actions in order with printable
  actions;
- other C0 controls remain ignored unless explicitly included in this design;
- pending invalid UTF-8 emits `U+FFFD` before an interrupting `LF` or `CR`;
- `hello\r\nworld` formats as `hello\nworld`;
- bare `LF` moves down while preserving cursor column;
- `LF` clears pending wrap without soft-wrapping first;
- `CR` clears pending wrap, moves to column 0, and does not dirty rows;
- `LF` on the active bottom row grows/scrolls, keeps the cursor on the active
  bottom row, and preserves history;
- bottom-row `LF` preserves the cursor column, including a nonzero column;
- bottom-row grow failure is transactional: cursor position, pending-wrap state,
  dirty state, and cells are unchanged if growth fails;
- LF dirty behavior is proven by tests that clear prior dirt first;
- split-feed `CRLF` behaves the same as same-slice `CRLF`;
- no backspace, tabs, NEL, RI, C1 controls, origin mode, margins, scroll
  regions, no-scrollback rotation, styles, hyperlinks, wide/Unicode handling,
  public API, or public ABI are added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- `LF` and `CR` work in terminal methods but the stream parser needs a broader
  execute-action refactor before dispatching them safely; or
- bottom-row `LF` requires a reusable index helper that should be split into a
  prerequisite experiment.

The experiment fails if:

- C0 controls still silently drop `LF` or `CR`;
- `LF` behaves like `CRLF` by default;
- `CR` dirties rows despite only moving the cursor;
- pending wrap soft-wraps before `LF` / `CR` clears it;
- bottom-row `LF` loses history or writes into the wrong active row;
- unsupported controls beyond the experiment scope are added without design
  review;
- public API or ABI changes are added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-013310-043205-prompt.md`
- Result: `logs/codex-review/20260601-013310-043205-last-message.md`

Codex found four real design issues:

- bottom-row `LF` grow-failure semantics were unspecified and could partially
  mutate state;
- LF dirty-state behavior was required but not directly tested;
- CR non-dirty behavior needed an explicit clear-dirty setup before issuing
  `CR`;
- bottom-row `LF` needed an explicit nonzero-column preservation test.

All four findings were applied. The design now requires transactional bottom-row
grow failure, LF dirty tests after clearing prior dirt for both non-bottom and
bottom-row cases, a CR non-dirty test after clearing dirt, and a bottom-row
nonzero-column preservation test.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-013454-220110-prompt.md`
- Result: `logs/codex-review/20260601-013454-220110-last-message.md`

Codex found no remaining design blockers and approved implementation.
