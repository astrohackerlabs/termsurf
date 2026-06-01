# Experiment 129: Port Basic Print Mode Effects

## Description

Experiment 128 made SM/RM mode state reachable from real stream input, but it
deliberately left several runtime mode effects state-only. This experiment ports
the basic print-path effects that the current Roastty core can represent
honestly:

- insert mode (`CSI 4 h/l`) changes printable single-cell ASCII writes from
  overwrite to insert-before-write when the cursor is not at the right edge;
- linefeed mode (`CSI 20 h/l`) makes LF perform an automatic carriage return
  after moving down, matching Ghostty's `linefeed()` ordering;
- wraparound mode (`CSI ? 7 h/l`) controls whether a pending-wrap printable
  single-cell ASCII character wraps to the next row or overwrites the right-edge
  cell in place.

This is still a basic-cell experiment. Roastty does not yet support wide
characters, grapheme clustering, current-SGR blank-cell coloring, or full styled
printing from stream input. Do not port those subsystems here. The goal is to
make the existing ASCII/basic-cell print path respect the modes that Experiment
128 now toggles through real terminal input.

Do not implement alternate screen, save/restore cursor, DECCOLM resize, mouse
encoding, keypad behavior, SGR parsing, OSC, DCS, public ABI, or non-macOS
behavior in this experiment.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Terminal.zig::print()` for insert and
     wraparound behavior.
   - Use `vendor/ghostty/src/terminal/Terminal.zig::linefeed()` for linefeed
     mode ordering.
   - Use upstream tests:
     - `Terminal: linefeed mode automatic carriage return`;
     - `Terminal: insert mode with space`;
     - `Terminal: insert mode doesn't wrap pushed characters`;
     - `Terminal: insert mode does nothing at the end of the line`;
     - `Terminal: input with basic wraparound`;
     - disabled-wraparound narrow-cell behavior inferred from `print()` because
       upstream's explicit disabled-wraparound tests focus on wide characters,
       which remain out of scope for Roastty here.
   - Do not modify `vendor/ghostty/`.

2. Thread basic print-mode inputs into the screen print path.
   - Keep `TerminalStreamHandler::print()` as the terminal-level owner of mode
     decisions.
   - Pass the current `Mode::Insert` and `Mode::Wraparound` values into the
     screen/basic print helper, or add an equivalent helper that keeps the
     decision in one place.
   - Preserve existing unsupported-codepoint behavior: non-ASCII printable input
     and non-replacement unsupported codepoints still return
     `TerminalStreamError::UnsupportedCodepoint`.

3. Implement basic insert-mode printing.
   - Before writing a single-cell printable ASCII codepoint, if insert mode is
     enabled and the cursor is not at the right edge, shift cells right by one
     using the existing insert-characters primitive, then write the codepoint.
   - Preserve the existing `insert_chars_basic()` margin semantics:
     - pending wrap is cleared before margin checks;
     - if the cursor is outside the horizontal margins, no cells shift;
     - if the cursor is inside the horizontal margins, only cells through the
       current right margin shift;
     - content shifted beyond the right margin is discarded.
   - Insert mode must not wrap pushed characters. Characters shifted past the
     right edge are discarded, matching the existing `CSI @` insert-character
     behavior.
   - Insert mode at the right edge does not shift. If wraparound is enabled and
     there is a pending wrap, the print path may wrap first; after the wrap,
     insert mode applies at the new position if it is not the right edge.
   - Keep the implementation scoped to single-cell ASCII. Wide-character insert
     behavior belongs to the later Unicode-width/grapheme subsystem.

4. Implement linefeed-mode runtime behavior.
   - `Action::LineFeed` covers LF, VT, and FF in Roastty's stream parser, as in
     upstream Ghostty. All three must honor linefeed mode.
   - `Action::Index` (`ESC D` / IND) remains index-only and must not apply
     linefeed-mode carriage return behavior.
   - When `Mode::Linefeed` is enabled for `Action::LineFeed`, perform carriage
     return after the linefeed movement, matching Ghostty's
     `try self.index(); if linefeed carriageReturn();` ordering.
   - `Escape E` / NEL remains linefeed plus carriage return by definition. If
     linefeed mode also performs a carriage return before NEL's explicit
     carriage return, the second carriage return must be observationally
     harmless.

5. Implement wraparound-mode runtime behavior for single-cell ASCII.
   - When wraparound is enabled, preserve current pending-wrap behavior:
     printing after a pending wrap moves to the next row and writes there.
   - When wraparound is disabled and `cursor.pending_wrap` is true, do not move
     to the next row and do not scroll. Write the new single-cell codepoint at
     the current right-edge cell and leave the cursor at the right edge.
   - After a disabled-wraparound right-edge overwrite, leave
     `cursor.pending_wrap` set, matching Ghostty's narrow-cell `print()` path.
     Repeated printable bytes should keep overwriting the same right-edge cell
     without moving. If wraparound is re-enabled while pending wrap is still
     set, the next printable byte should wrap normally.
   - Keep bottom-row behavior explicit: disabled wraparound at the bottom-right
     cell must not scroll.
   - Do not implement the wide-character disabled-wraparound cases from upstream
     yet.

6. Add tests.
   - Insert mode:
     - real input `CSI 4 h` inserts before writing in the middle of a row;
     - `CSI 4 l` returns to overwrite behavior;
     - pushed characters do not wrap and are discarded at the edge;
     - insert mode at the right edge does not shift before the write/wrap path;
     - insert mode inside horizontal margins shifts only through the right
       margin;
     - insert mode outside horizontal margins clears pending wrap but does not
       shift cells;
     - insert mode with a narrowed right margin discards content shifted beyond
       that right margin;
     - split-feed mode-setting plus print behaves the same as same-slice input.
   - Linefeed mode:
     - `CSI 20 h` makes LF move down and return to column 0;
     - `CSI 20 l` returns LF to column-preserving behavior;
     - bottom-row LF with linefeed mode scrolls then returns to column 0;
     - VT and FF honor linefeed mode because they dispatch `Action::LineFeed`;
     - `ESC D` / IND remains index-only and ignores linefeed-mode carriage
       return behavior;
     - NEL / `ESC E` remains linefeed plus carriage return by definition.
   - Wraparound mode:
     - default wraparound behavior still wraps after pending wrap;
     - `CSI ? 7 l` disables pending-wrap movement for the next printable
       single-cell ASCII character;
     - disabled wraparound overwrites the right-edge cell and does not dirty the
       next row;
     - disabled wraparound leaves pending wrap set after overwrite;
     - repeated disabled-wraparound writes keep overwriting the right-edge cell;
     - disabled wraparound at the bottom-right cell does not scroll;
     - `CSI ? 7 h` re-enables normal wraparound behavior, including wrapping the
       next printable byte if pending wrap is still set.
   - Dirty state:
     - insert-mode shifts dirty only the affected row;
     - disabled-wraparound right-edge overwrite dirties the current row;
     - disabled bottom-right overwrite does not create scrollback or dirty rows
       from a scroll that should not happen.
   - Existing print, pending-wrap, CR/LF, index/NEL, tab, cursor, CSI mutation,
     formatter, PageList, and ABI tests must keep passing.

7. Verify.
   - Run:

     ```bash
     cargo fmt
     cargo test -p roastty terminal_stream_csi_mode
     cargo test -p roastty terminal_stream_lf
     cargo test -p roastty terminal_stream_pending_wrap
     cargo test -p roastty terminal_stream_right_edge
     cargo test -p roastty terminal::terminal
     cargo test -p roastty stream
     cargo test -p roastty terminal_formatter
     cargo test -p roastty screen_formatter
     cargo test -p roastty page_string
     cargo test -p roastty
     ```

   - `cargo fmt` output must be accepted as-is.

8. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - Commit the approved design before implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.
   - Commit the approved result separately from the design commit.

9. Record the result.
   - Append `## Result` and `## Conclusion` to this file.
   - Include:
     - exact insert-mode behavior;
     - exact linefeed-mode behavior and ordering;
     - exact disabled-wraparound behavior;
     - intentionally deferred wide/grapheme/styled behavior;
     - dirty-row and scroll behavior for affected paths;
     - verification command output summary;
     - Codex design-review outcome;
     - Codex result-review outcome.
   - Update the Issue 801 README experiment index from `Designed` to `Pass`,
     `Partial`, or `Fail`.

## Verification

The experiment passes if:

- real `CSI 4 h/l`, `CSI 20 h/l`, and `CSI ? 7 h/l` input changes the
  corresponding runtime behavior, not only stored mode state;
- insert mode shifts existing single-cell ASCII content right before writing
  when there is room and discards shifted-off content instead of wrapping it;
- insert mode obeys horizontal margins and does not shift outside them;
- linefeed mode performs linefeed movement before carriage return, matching
  Ghostty's ordering;
- disabled wraparound prevents pending-wrap movement and bottom-row scroll for
  single-cell ASCII printing;
- disabled wraparound leaves pending wrap set after right-edge overwrite, and
  re-enabled wraparound restores the existing wrap path;
- no wide-character, grapheme, SGR, alternate-screen, DECCOLM, mouse, keypad,
  OSC, DCS, public ABI, or non-macOS behavior is added;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- linefeed and wraparound behavior can land but insert mode requires a broader
  screen primitive;
- disabled wraparound exposes existing pending-wrap assumptions that need a
  narrower preparatory experiment.

The experiment fails if:

- mode state changes but runtime behavior remains unchanged;
- insert mode ignores horizontal margins or shifts outside the margin region;
- insert mode wraps pushed characters;
- disabled wraparound still moves to the next row or scrolls;
- disabled wraparound clears pending wrap after a right-edge overwrite;
- linefeed mode applies carriage return in the wrong order;
- VT/FF fail to honor linefeed mode or IND incorrectly honors linefeed mode;
- it fakes or partially implements wide/grapheme/styled behavior outside this
  experiment's scope;
- it changes unrelated parser, formatter, PageList, ABI, or non-macOS behavior.

## Design Review

Codex reviewed the initial design and found four real issues:
`logs/codex-review/20260601-064245-945736-last-message.md`.

The design was updated to:

- require LF, VT, and FF to honor linefeed mode through `Action::LineFeed`,
  while keeping `ESC D` / IND index-only;
- make insert-mode horizontal-margin semantics part of the required scope,
  including pending-wrap clearing before margin checks, no shifting outside
  margins, shifting only through the right margin, and discarding shifted-off
  content;
- require disabled wraparound to leave pending wrap set after right-edge
  overwrite, keep repeated writes on the right-edge cell, and wrap normally once
  wraparound is re-enabled;
- add dirty-row and no-scroll expectations for the affected paths.

Codex re-reviewed the updated design and found no blocking design findings:
`logs/codex-review/20260601-064703-396868-last-message.md`.

The design is approved for implementation.
