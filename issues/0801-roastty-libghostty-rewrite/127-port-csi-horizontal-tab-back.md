# Experiment 127: Port CSI Horizontal Tab Back

## Description

Port Ghostty's `CSI Z` / CBT command:

- `CSI Z` / `CSI 1 Z` moves the cursor to the previous horizontal tab stop;
- `CSI n Z` repeats that movement `n` times;
- if no earlier tab stop exists, the cursor clamps to the left limit.

Roastty already implements ordinary horizontal tab (`HT`) and `CSI I` from
Experiment 119. This experiment completes the paired tab-stop cursor movement
surface by adding the backwards direction. This is intentionally a narrow CSI
slice: it shares the existing tabstop data structure and cursor-only behavior,
does not mutate cells, and does not add mode-setting parser work.

Upstream Ghostty parses this in `vendor/ghostty/src/terminal/stream.zig`:

- final `Z` emits `.horizontal_tab_back`;
- no params means count `1`;
- one param emits that exact count, including `0`;
- more than one param is invalid and dispatches no action;
- private/intermediate forms are invalid.

Upstream Ghostty executes this in
`vendor/ghostty/src/terminal/Terminal.zig::horizontalTabBack`:

- it moves left one cell at a time until it reaches a previous tab stop;
- starting on a tab stop moves to the previous tab stop, not the current one;
- without origin mode, the left limit is column `0`;
- with origin mode, the left limit is the active scrolling-region left margin;
- if the cursor is already at or left of the left limit, it does nothing;
- it does not dirty rows or modify cells.

Roastty already has mode storage and test helpers for `Mode::Origin`, and
scrolling-region test helpers can set left/right margins. This experiment may
use those helpers to prove origin-mode left-limit behavior, but it must not add
general mode parser support or left/right margin parser support.

Pending-wrap behavior should match the existing `horizontal_tab_basic()` model:
moving by tab stops should not explicitly clear pending wrap. Add tests for the
edge cases where this matters rather than assuming cursor movement helpers will
handle it.

Do not implement SGR, OSC, DCS, alternate-screen semantics, Kitty graphics,
public ABI, mode parser commands, left/right margin parser commands, or
non-macOS behavior in this experiment.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/stream.zig` for `CSI Z` parsing.
   - Use `vendor/ghostty/src/terminal/Terminal.zig::horizontalTabBack` for
     execution semantics.
   - Use upstream tests around `Terminal: horizontal tabs back`,
     `Terminal: horizontal tabs back starting on tabstop`, and origin-mode left
     margin cases as the behavior checklist.
   - Do not modify `vendor/ghostty/`.

2. Extend the private stream action.
   - Add `Action::HorizontalTabBack { count }` in
     `roastty/src/terminal/stream.rs`.
   - Keep the action internal to the terminal module.
   - Do not add public API or ABI surface.

3. Extend CSI dispatch for final `Z`.
   - Add `horizontal_tab_back_action()`.
   - Accept:
     - `CSI Z`;
     - `CSI 0 Z`;
     - `CSI ; Z`;
     - `CSI 1 Z`;
     - `CSI 1 ; Z`;
     - larger single numeric params, clamped to `u16::MAX` by the current parser
       accumulator behavior.
   - Reject and dispatch no action for:
     - private forms such as `CSI ? Z` and `CSI > Z`;
     - real multi-param forms such as `CSI 1 ; 2 Z`;
     - colon/mixed separators;
     - any byte that the current parser treats as invalid;
     - direct C1 CSI byte `0x9b`, which remains out of scope and follows the
       current UTF-8 replacement behavior.
   - Preserve parser ground-state behavior on handler errors.
   - Preserve pending invalid UTF-8 behavior: if an incomplete invalid UTF-8
     sequence is interrupted by `CSI Z`, dispatch `U+FFFD` before the tab-back
     action.

4. Add `Screen::horizontal_tab_back_basic()`.
   - Use the current cursor x, terminal width, tabstop set, and left limit.
   - Move left until reaching the previous tab stop.
   - Starting on a tab stop must skip the current position and move to the
     previous tab stop.
   - Clamp to `left_limit` if no earlier tab stop exists.
   - If the cursor is already at or left of `left_limit`, do nothing.
   - Preserve cursor y.
   - Do not modify cells or dirty rows.
   - Do not explicitly clear pending wrap.

5. Add `Screen::horizontal_tab_back_count_basic()`.
   - Reuse `horizontal_tab_back_basic()` in a loop.
   - Count `0` is a no-op.
   - Stop early if a step does not move the cursor.

6. Route terminal actions.
   - In `TerminalStreamHandler`, route `Action::HorizontalTabBack`.
   - The left limit is:
     - `0` when `Mode::Origin` is not set;
     - `scrolling_region.left` when `Mode::Origin` is set.
   - Pass the current tabstop set.
   - Existing print, linefeed, cursor, positioning, tab, erase-display,
     erase-line, delete-character, insert/erase-character, insert-line,
     delete-line, scroll, formatter, PageList, and ABI behavior must keep
     passing unchanged.

7. Add tests.
   - Stream parser tests:
     - `A\x1b[ZB` dispatches print `A`, horizontal-tab-back count `1`, print
       `B`;
     - `CSI Z` dispatches count `1`;
     - `CSI 0 Z` and `CSI ; Z` dispatch count `0`;
     - `CSI 1 Z` and `CSI 1 ; Z` dispatch count `1`;
     - larger single params dispatch their parsed/clamped value;
     - real multi-param, colon-param, mixed-separator, and invalid/private forms
       dispatch no action and do not leak printable final bytes;
     - split-feed `CSI Z` and `CSI 3 Z` dispatch correctly;
     - pending invalid UTF-8 emits `U+FFFD` before same-slice and split-feed
       `CSI Z`;
     - direct C1 CSI byte `0x9b` followed by `Z` remains out of scope and
       dispatches `U+FFFD` plus printable `Z`;
     - handler errors from horizontal-tab-back leave the parser in ground state.
   - Terminal tests:
     - default tab stops from the right edge move `19 -> 16 -> 8 -> 0` in a
       20-column terminal, matching upstream;
     - starting on a tab stop moves to the previous tab stop;
     - `CSI 0 Z` and `CSI ; Z` are no-ops;
     - larger counts stop at column `0`;
     - custom tab stops are used;
     - without origin mode, active left/right margins do not change the left
       limit;
     - with origin mode and a left margin, tab-back clamps to the left margin;
     - when the cursor starts before or at the origin-mode left margin, it does
       not move;
     - pending wrap is preserved when tab-back does not move, and the chosen
       moved-case pending-wrap behavior is documented by an explicit test;
     - rows are not dirtied and cells are not modified;
     - split-feed `CSI Z` mutates cursor state correctly;
     - unsupported/invalid `CSI Z` forms do not mutate terminal state.
   - Existing stream, cursor movement, positioning, tabstop, erase-display,
     erase-line, delete-character, insert/erase-character, insert-line,
     delete-line, scroll, formatter, PageList, and ABI tests must keep passing.

8. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty stream
     cargo test -p roastty terminal::terminal
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
      - accepted and rejected `CSI Z` forms;
      - terminal behavior for count `0`, count `1`, larger counts, clamping,
        custom tab stops, origin-mode left limit, pending-wrap behavior, and
        cursor-y / unintended-state preservation;
      - confirmation that no cells or dirty rows are changed;
      - confirmation that existing raw print, cursor, positioning, line, tab,
        erase-display, erase-line, delete-character, insert/erase-character,
        insert-line, delete-line, scroll, formatter, PageList, and ABI behavior
        did not regress;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Design Review

Codex reviewed the design and found no blocking issues:
`logs/codex-review/20260601-061119-674532-last-message.md`.

Codex noted one non-blocking wording nit: the result checklist said “cursor
preservation,” but `CSI Z` intentionally changes `cursor.x`. The checklist now
uses “cursor-y / unintended-state preservation” instead. The design is approved
for implementation.

## Verification

The experiment passes if:

- `CSI Z` dispatches counts exactly as upstream's one-param/default-count parser
  shape requires;
- invalid/private/colon/mixed/multi-param forms dispatch no tab-back action and
  do not leak printable bytes;
- direct C1 CSI byte `0x9b` remains out of scope and follows current raw-C1
  UTF-8 replacement behavior;
- pending invalid UTF-8 emits `U+FFFD` before the tab-back action;
- handler errors leave the parser in ground state;
- `CSI Z` moves to previous tab stops, not the current tab stop;
- count `0` is a no-op;
- oversized counts clamp at the left limit;
- origin mode uses the scrolling-region left margin as the left limit;
- non-origin mode ignores the scrolling-region left margin for this command;
- pending-wrap behavior is explicitly tested and documented;
- rows are not dirtied and cells are not modified;
- existing raw print, linefeed, cursor, positioning, tabstop, erase-display,
  erase-line, delete-character, insert/erase-character, insert-line,
  delete-line, scroll, PageList, formatter, and ABI behavior remains unchanged;
- no unrelated CSI, SGR, OSC, DCS, public API, ABI, mode parser, margin parser,
  or non-macOS behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- stream dispatch is correct but origin-mode left-limit behavior needs a
  separate mode plumbing fix before execution can be called complete;
- cursor movement works but pending-wrap behavior conflicts with upstream and
  needs a narrower follow-up after investigation;
- execution works for default tab stops but custom tabstop behavior exposes a
  bug in the existing tabstop model.

The experiment fails if:

- it treats `CSI 0 Z` as count `1`;
- it stops on the current tab stop instead of moving to the previous one;
- it ignores origin-mode left limits;
- it mutates cells or dirties rows;
- it changes unrelated cursor, tab, print, erase, row mutation, scroll,
  formatter, PageList, or ABI behavior;
- it adds unrelated CSI, SGR, OSC, DCS, public API, ABI, mode parser, margin
  parser, or non-macOS behavior.
