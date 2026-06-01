# Experiment 103: Port Basic Stream Print Mutation

## Description

Connect Experiment 102's private stream parser to the terminal state for the
first mutation slice: printable narrow ASCII bytes should flow through
`Stream::next_slice()`, arrive as `Action::Print`, write to the active screen at
the cursor, and advance the cursor.

Upstream Ghostty's `Terminal.print()` is much larger than this slice. It handles
status display, pending wrap, scrolling, insert mode, Unicode width, grapheme
clusters, wide spacers, style references, hyperlinks, charsets, Kitty graphics,
semantic prompt state, and dirty tracking. This experiment must not pretend to
port all of that. It ports only the simplest upstream behavior proven by the
first `Terminal: input with no control characters` test: writing ordinary text
into an empty screen before the right edge.

The purpose is to establish the runtime action-to-terminal boundary without
turning the next experiment into a full terminal parser rewrite.

## Changes

1. Re-read the upstream source of truth.
   - Use `vendor/ghostty/src/terminal/Terminal.zig` for:
     - `vtHandler()`;
     - `printString()`;
     - `print()`;
     - the first upstream test, `Terminal: input with no control characters`.
   - Use `vendor/ghostty/src/terminal/stream.zig` for `Action.print` dispatch.
   - Do not modify `vendor/ghostty/`.

2. Add a private terminal stream entrypoint.
   - Add a private, non-ABI method on `Terminal` for feeding VT bytes through
     the stream parser. A name such as `next_slice(&mut self, input: &[u8])` or
     `vt_next_slice(&mut self, input: &[u8])` is acceptable.
   - This method is internal to the Rust terminal implementation. It must not be
     exported through `roastty.h`, public C ABI, app API, renderer API, or
     workspace-level public API.
   - The method should construct/use a stream handler that dispatches
     `Action::Print` back into `Terminal`.
   - Preserve the parser/handler boundary introduced in Experiment 102.
   - Parser state must survive separate terminal feed calls. A fresh parser per
     `Terminal::next_slice()` call is a failure because it loses split UTF-8 and
     split escape state from Experiment 102.
   - To make persistent parser state possible without self-referential borrows,
     refactor `stream::Stream` if needed so it owns parser state only and
     receives a mutable handler per `next_slice()` call, or otherwise store a
     reusable parser state inside `Terminal` without unsafe aliasing. Do not add
     unsafe Rust to solve this borrow problem.
   - Add tests that feed split UTF-8 and split `ESC [ C` through two separate
     terminal feed calls and prove parser state survives the boundary.

3. Add a private terminal stream handler.
   - Implement `stream::Handler` for a small private handler wrapper, not by
     exposing `stream::Action` outside the terminal subsystem.
   - The handler should match `Action::Print { cp }` and call the basic print
     mutation helper described below.
   - Change the private handler boundary to propagate errors concretely if
     needed. For example, `Handler::vt()` may return a private `Result`, and
     `Stream::next_slice()` may return that same error type. The exact Rust
     shape is flexible, but unsupported mutation paths must not be silently
     swallowed.
   - Do not add handling for CSI, OSC, DCS, APC, modes, cursor movement
     sequences, tab mutation, PWD mutation, keyboard mutation, PTY IO, or app
     events.

4. Add the smallest safe screen write helper.
   - Add a private helper on `Screen` or `PageList` that writes a single
     codepoint into the active screen at the current cursor position.
   - The helper must mark the row/page dirty in the same spirit as upstream's
     `cursorMarkDirty()` before/when changing a cell.
   - The helper must be deliberately scoped to unmanaged/default cells for this
     experiment. If the target cell has grapheme, style, hyperlink, wide spacer,
     or other managed state, return a small private error rather than silently
     leaking references or corrupting state. Full `printCell()` cleanup belongs
     in a later experiment.
   - Prefer reusing existing `PageList` pin/page/cell helpers over duplicating
     raw offset arithmetic.
   - Add a narrow `#[cfg(test)]` dirty-state observer if one is not already
     available. Dirty-state verification is mandatory for this experiment.

5. Add basic cursor advancement.
   - After writing a supported one-cell printable character before the right
     edge, advance `cursor.x` by one.
   - Keep `cursor.y` unchanged in this slice.
   - Do not implement wrapping, scrolling, insert mode, margins, origin mode, or
     pending-wrap execution.
   - If the cursor is already at the right edge, reject this path with a private
     `RightEdgeUnsupported`-style error. Do not write the cell. Pending-wrap and
     edge printing belong in the next print experiment.
   - Test the right-edge rejection. Do not silently overwrite the last cell on
     the next print and call it correct.

6. Restrict the supported print set honestly.
   - This experiment supports ordinary one-cell printable ASCII and `U+FFFD`.
   - Do not implement Unicode width tables, wide characters, zero-width
     combining marks, grapheme clustering, charsets, emoji variation selectors,
     style propagation, hyperlinks, or semantic prompt state.
   - For non-ASCII Unicode other than `U+FFFD`, return a private
     `UnsupportedCodepoint`-style error. Do not claim full Unicode print
     support.

7. Add tests.
   - Add tests for:
     - stream-fed ASCII bytes write to the active screen and format back as
       plain text;
     - cursor x advances after each supported printed character;
     - row/page dirty state is marked for written cells;
     - invalid UTF-8 through the stream writes `U+FFFD`;
     - C0 controls and unsupported ESC/CSI input from Experiment 102 do not
       write cells through the terminal handler;
     - split UTF-8 across two terminal feed calls preserves parser state;
     - split `ESC [ C` across two terminal feed calls preserves parser state and
       does not leak `[` or `C`;
     - right-edge input returns the private unsupported error and does not write
       a cell;
     - valid non-ASCII input other than `U+FFFD` returns the private unsupported
       error;
     - attempting to overwrite a managed target cell returns the private
       unsupported error.
   - Port or mirror the upstream `Terminal: input with no control characters`
     test as closely as this narrow scope allows.

8. Verify.
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

9. Independent review.
   - Before implementation, get Codex review of this experiment design.
   - Fix all real design findings before implementation.
   - Record the design-review outcome in this experiment file before
     implementation.
   - After implementation and verification, get Codex review of the completed
     result.
   - Fix all real result findings before proceeding.

10. Record the result.
    - Append `## Result` and `## Conclusion` to this file.
    - Include:
      - the terminal stream entrypoint shape;
      - the handler wiring;
      - the screen/page write helper shape;
      - the supported print set;
      - the explicit right-edge behavior;
      - what remains deferred from upstream `Terminal.print()`;
      - verification command output summary;
      - Codex design-review outcome;
      - Codex result-review outcome.
    - Update the Issue 801 README experiment index from `Designed` to `Pass`,
      `Partial`, or `Fail`.

## Verification

The experiment passes if:

- `Action::Print` can flow from the private stream parser into `Terminal`
  through a private handler boundary;
- ordinary ASCII input writes cells into the active screen and formats back as
  the same text;
- parser state survives multiple terminal feed calls;
- cursor x advances for supported printed characters before the right edge;
- row/page dirty state is observable and marked for written cells;
- invalid UTF-8 through the stream writes `U+FFFD`;
- unsupported controls and escape sequences remain non-printing;
- right-edge input, unsupported non-ASCII input, and managed-cell overwrite
  attempts return explicit private errors and are tested;
- managed cell cleanup, Unicode width, wide characters, zero-width characters,
  grapheme clustering, charsets, styles, hyperlinks, semantic prompt state,
  wrapping, scrolling, insert mode, margins, CSI, OSC, DCS, APC, PTY IO, public
  API, and public ABI remain deferred;
- `cargo fmt` and the listed tests pass;
- Codex design and result reviews approve the experiment, or all real findings
  are fixed before proceeding.

The experiment is partial if:

- safe cell mutation requires first porting a larger upstream `Screen` helper,
  and that prerequisite is identified precisely;
- cursor advancement cannot be represented safely without first adding upstream
  pending-wrap state, and the next experiment is scoped around that
  prerequisite.

The experiment fails if:

- stream parsing is coupled directly to page storage without the handler
  boundary;
- terminal feed calls construct a fresh parser and lose split UTF-8 or split
  escape state;
- the implementation claims full `Terminal.print()` behavior;
- managed cells can be overwritten in a way that leaks style, grapheme, or
  hyperlink references;
- unsupported print paths are silently ignored instead of returning the private
  error;
- unsupported escape/control syntax writes printable text;
- right-edge input silently overwrites existing content without documented
  explicit rejection behavior;
- public API or ABI changes are added.

## Design Review

Codex reviewed this design before implementation.

Initial review artifacts:

- Prompt: `logs/codex-review/20260601-005538-774733-prompt.md`
- Result: `logs/codex-review/20260601-005538-774733-last-message.md`

Codex found three real design gaps:

- terminal feed calls needed persistent parser state across calls, not a fresh
  `Stream` per call;
- unsupported right-edge, managed-cell, and non-ASCII paths needed concrete
  private error propagation instead of implicit panics or silent drops;
- dirty-state verification needed to be mandatory, not conditional.

All three findings were applied. A clean design re-review will be recorded
before implementation.

Clean design re-review artifacts:

- Prompt: `logs/codex-review/20260601-005802-890041-prompt.md`
- Result: `logs/codex-review/20260601-005802-890041-last-message.md`

Codex found no remaining real design findings and approved implementation.

## Result

**Result:** Pass.

Implemented the first terminal mutation path for stream print actions:

- `stream::Stream` now owns parser state only. It no longer owns the handler, so
  parser state can live in `Terminal` across multiple feed calls while each call
  borrows a fresh private handler.
- `stream::Handler::vt()` now returns a private `Result`, and
  `Stream::next_slice()` propagates handler errors.
- `Terminal` now owns a private `stream::Stream` field and exposes a private
  `next_slice(&mut self, input: &[u8]) -> Result<(), TerminalStreamError>`
  entrypoint inside the terminal subsystem.
- A private `TerminalStreamHandler` maps `Action::Print { cp }` to the basic
  print mutation helper.
- `Screen::print_basic_cell()` writes one supported codepoint at the active
  cursor and advances `cursor.x` before the right edge.
- `PageList::write_basic_screen_cell()` performs the scoped cell mutation and
  marks the row dirty.

The supported print set is intentionally narrow:

- printable ASCII;
- `U+FFFD`, so invalid UTF-8 can visibly round-trip through the terminal;
- no other non-ASCII Unicode yet.

Unsupported paths return explicit private errors:

- right-edge input returns `TerminalStreamError::RightEdgeUnsupported` and does
  not write;
- non-ASCII codepoints other than `U+FFFD` return
  `TerminalStreamError::UnsupportedCodepoint`;
- attempts to overwrite cells with managed state return
  `TerminalStreamError::ManagedCellUnsupported`.

This experiment did not implement full upstream `Terminal.print()` behavior:
pending-wrap execution, wrapping, scrolling, insert mode, margins, origin mode,
Unicode width tables, wide characters, zero-width characters, grapheme
clustering, charset mapping, styles, hyperlinks, semantic prompt state, CSI,
OSC, DCS, APC, PTY IO, public API, and public ABI remain deferred.

Verification run:

```text
cargo fmt
cargo test -p roastty stream
cargo test -p roastty terminal_formatter
cargo test -p roastty terminal::terminal
cargo test -p roastty screen_formatter
cargo test -p roastty page_string
cargo test -p roastty terminal::page_list
cargo test -p roastty
```

Results:

- `cargo fmt` passed.
- `cargo test -p roastty stream` passed 82 tests.
- `cargo test -p roastty terminal_formatter` passed 67 tests.
- `cargo test -p roastty terminal::terminal` passed 77 tests.
- `cargo test -p roastty screen_formatter` passed 55 tests.
- `cargo test -p roastty page_string` passed 12 tests.
- `cargo test -p roastty terminal::page_list` passed 524 tests.
- Full `cargo test -p roastty` passed 983 unit tests, the ABI harness, and
  doc-tests.

Codex design review passed after the three design findings above were fixed.

Result-review artifacts:

- Prompt: `logs/codex-review/20260601-010452-112971-prompt.md`
- Result: `logs/codex-review/20260601-010452-112971-last-message.md`

Codex found no blocking findings. It confirmed that parser state is persistent,
error propagation is concrete, mutation scope is narrow, right-edge rejection is
explicit and non-mutating, managed-cell protection is in place, dirty state is
marked and tested, and the result language does not overclaim full
`Terminal.print()` behavior.

## Conclusion

Roastty now has the first end-to-end runtime path from bytes to terminal state:
VT bytes enter the private stream parser, `Action::Print` crosses the private
handler boundary, supported printable bytes write cells into the active screen,
and cursor/dirty state updates are observable in tests.

The next print experiment should extend this foundation toward upstream
`Terminal.print()` rather than broadening parser syntax. The most direct next
slice is pending-wrap/right-edge behavior and basic wrapping, because this
experiment deliberately rejects the right edge instead of implementing Ghostty's
pending-wrap semantics.
