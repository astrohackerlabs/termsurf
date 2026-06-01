# Experiment 112: Port CSI Tab Clear and Reset

## Description

Continue the tabstop stream/action port by adding Ghostty's remaining CSI cursor
tabulation control forms:

- `CSI 2 W` → `tab_clear_current`
- `CSI 5 W` → `tab_clear_all`
- `CSI ? 5 W` → `tab_reset`

Experiment 111 added `CSI W` and `CSI 0 W` as tab-set forms while explicitly
leaving these neighboring actions unsupported. Roastty already has the tabstop
storage operations needed for the terminal side:

- `Tabstops::unset(col)` mirrors upstream `Tabstops.unset()`;
- `Tabstops::reset(0)` clears all tabstops;
- `Tabstops::reset(TABSTOP_INTERVAL)` restores default tabstops.

This experiment wires those existing operations through private stream actions.
It does not add a public CSI parser API, public terminal API, or public ABI.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for the
     `Cursor Tabulation Control` branch:
     - `CSI 2 W` emits `.tab_clear_current`;
     - `CSI 5 W` emits `.tab_clear_all`;
     - `CSI ? 5 W` emits `.tab_reset`;
     - other `W` forms remain ignored for this experiment.
   - Use `vendor/ghostty/src/termio/stream_handler.zig` and
     `vendor/ghostty/src/terminal/stream_terminal.zig` for routing those actions
     to terminal tabstop mutation.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for `tabClear()` and
     `tabReset()`:
     - current clear calls `tabstops.unset(cursor.x)`;
     - all clear calls `tabstops.reset(0)`;
     - reset calls `tabstops.reset(TABSTOP_INTERVAL)`.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action surface.
   - Add private actions:
     - `Action::TabClearCurrent`
     - `Action::TabClearAll`
     - `Action::TabReset`
   - Keep `Action::TabSet` unchanged.
   - Do not expose these actions outside the terminal module.

3. Extend CSI `W` parsing without generalizing CSI.
   - Extend the private CSI state from Experiment 111 so it can identify:
     - no params → `TabSet`;
     - single numeric param `0` → `TabSet`;
     - single numeric param `2` → `TabClearCurrent`;
     - single numeric param `5` → `TabClearAll`;
     - private `?` plus single numeric param `5` → `TabReset`.
   - Treat `>` private-marker forms, non-`?` private-marker forms, semicolons,
     multiple params, intermediates, overflowing numeric params, and unsupported
     params as invalid/no-op.
   - Keep unsupported non-`W` CSI finals consumed and ignored.
   - Set the stream parser back to ground before invoking the handler for every
     CSI `W` action, so handler errors cannot leave the parser in CSI state.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by any accepted CSI `W` form, dispatch `U+FFFD`
     before the CSI action.
   - Do not add tab clear/reset through any non-CSI path in this experiment.

4. Add terminal tabstop behavior.
   - `TabClearCurrent` unsets the tabstop at the active cursor column.
   - `TabClearAll` clears all tabstops with `Tabstops::reset(0)`.
   - `TabReset` restores default tabstops with `TABSTOP_INTERVAL`.
   - Leave cursor position unchanged.
   - Leave pending wrap unchanged.
   - Do not modify cells.
   - Do not dirty rows.
   - Do not change `Tabstops::unset()` semantics in this experiment; it already
     mirrors upstream's XOR behavior.

5. Add tests.
   - Stream parser tests:
     - `A\x1b[2WB` dispatches print, tab-clear-current, print;
     - `A\x1b[5WB` dispatches print, tab-clear-all, print;
     - `A\x1b[?5WB` dispatches print, tab-reset, print;
     - split-feed versions of all three accepted forms dispatch the same
       actions;
     - pending invalid UTF-8 dispatches `U+FFFD` before each accepted CSI `W`
       action;
     - handler errors for tab-clear-current, tab-clear-all, and tab-reset leave
       the parser in ground state for the next byte;
     - invalid/deferred forms such as `CSI ? 2 W`, `CSI > 5 W`, `CSI ? 1 W`,
       `CSI 1 W`, `CSI 99 W`, `CSI 0 ; 5 W`, and overflowing params dispatch no
       new action and recover for the next byte;
     - `CSI W` and `CSI 0 W` keep dispatching `TabSet`.
   - Terminal tests:
     - after moving to the default tabstop at column 8, `CSI 2 W` clears that
       stop so the next `HT` from column 0 moves to column 16;
     - `CSI 5 W` clears all stops so the next `HT` from column 0 clamps to the
       right edge;
     - after `CSI 5 W`, `CSI ? 5 W` restores defaults so the next `HT` from
       column 0 moves to column 8;
     - each action leaves cursor position unchanged;
     - each action preserves pending wrap at the right edge;
     - each action does not dirty rows or modify cells by itself.
   - Existing stream, `ESC H`, CSI tab-set, horizontal-tab, formatter, PageList,
     and ABI tests must keep passing.

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
     - accepted and rejected CSI `W` forms;
     - stream parser state behavior on handler errors;
     - terminal tabstop mutation behavior;
     - pending-wrap, dirty-row, and cell behavior;
     - what remains deferred;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `CSI 2 W` dispatches `Action::TabClearCurrent`;
- `CSI 5 W` dispatches `Action::TabClearAll`;
- `CSI ? 5 W` dispatches `Action::TabReset`;
- split-feed versions of all three dispatch the same actions;
- `CSI W` and `CSI 0 W` keep dispatching `Action::TabSet`;
- invalid forms such as `CSI ? 2 W`, `CSI > 5 W`, `CSI ? 1 W`, `CSI 1 W`,
  `CSI 99 W`, `CSI 0 ; 5 W`, and overflowing params dispatch no tab action and
  recover for the next byte;
- pending invalid UTF-8 emits `U+FFFD` before each accepted CSI `W` action;
- handler errors from all accepted CSI `W` actions leave the parser in ground
  state for the next byte;
- current clear removes the active cursor column from tabstop navigation;
- all clear removes all tabstops;
- reset restores the default tabstop interval;
- all three terminal actions leave cursor position and pending wrap unchanged;
- all three terminal actions do not dirty rows or modify cells by themselves;
- no horizontal-tab-back, margins, origin mode, no-scrollback rotation, styles,
  hyperlinks, wide/Unicode handling, public API, or public ABI are added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream parsing dispatches the new actions correctly, but terminal tabstop
  mutation exposes a mismatch in the already-ported `Tabstops` storage that
  requires a separate storage-focused experiment.

The experiment fails if:

- any accepted CSI `W` form remains silently ignored;
- invalid CSI `W` forms dispatch a tab action;
- CSI bytes leak as printable text;
- handler errors leave the parser stuck in CSI state;
- terminal tabstop mutation targets the wrong column or reset interval;
- any tab clear/reset action moves the cursor, clears pending wrap, dirties
  rows, or modifies cells;
- public API or ABI changes are added.

## Design Review

Codex reviewed this design before implementation.

Review artifacts:

- Prompt: `logs/codex-review/20260601-022752-425781-prompt.md`
- Result: `logs/codex-review/20260601-022752-425781-last-message.md`

The helper reported that the stored Codex session was full and automatically
started a fresh session, as required by the review skill.

Codex found no design blockers. It confirmed the accepted CSI `W` forms,
terminal tabstop mutations, scope boundaries, and test plan match upstream
Ghostty and Roastty's current shape. It noted that the existing unsupported CSI
`W` variant test from Experiment 111 will need to move `CSI 2 W`, `CSI 5 W`, and
`CSI ? 5 W` into accepted-action coverage during implementation.
