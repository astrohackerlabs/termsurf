+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 782: Terminal Clipboard OSC Events

## Description

Retain parsed terminal clipboard OSC actions as drainable terminal events.

The terminal parser already recognizes OSC 52 clipboard commands and Kitty
clipboard OSC 5522 commands, but `Terminal::osc` currently drops those actions.
That means the surface/termio layer has no authoritative terminal event stream
from which it can allocate OSC 52 read/write clipboard requests.

This experiment adds a narrow terminal-side event queue for clipboard OSC
actions. It does not initiate runtime clipboard callbacks, allocate
`Surface`-owned request states, write clipboard data, or send OSC replies. Those
surface/termio behaviors remain later experiments once the terminal event bridge
exists.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add an owned `TerminalClipboardEvent` enum for clipboard OSC actions.
  - Store pending clipboard OSC events on `Terminal`.
  - On `stream::OscAction::ClipboardContents`, push an event containing the OSC
    52 clipboard kind byte and raw data bytes.
  - On `stream::OscAction::KittyClipboard`, push an event containing metadata
    bytes, optional payload bytes, and the terminator.
  - Add a drain method so callers can take pending clipboard OSC events without
    disturbing normal terminal screen/title/pwd state.
  - Clear pending clipboard OSC events on both direct terminal reset and
    RIS/full reset (`ESC c`).
  - Replace the current "clipboard protocols are ignored" test with coverage
    proving the protocols are retained as events while still not mutating the
    terminal screen, title, pwd, hyperlink, cursor, modes, colors, dirty rows,
    or PTY response.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Update checklist wording only if the implementation proves this narrow
    terminal event bridge is complete.
  - Use scoped wording: terminal clipboard OSC event retention done, while OSC
    52 surface request allocation/handling stays missing.

## Verification

- Run focused tests:
  - `cargo test -p roastty terminal_clipboard -- --nocapture --test-threads=1`
  - `cargo test -p roastty terminal_stream_clipboard -- --nocapture --test-threads=1`
- New or updated assertions must cover:
  - OSC 52 events retain the clipboard kind byte and raw data bytes.
  - Kitty clipboard events retain metadata, optional payload, and terminator.
  - Draining returns events in parse order and clears the pending queue.
  - Draining an empty queue returns no events.
  - Direct terminal reset clears pending clipboard events.
  - RIS/full reset (`ESC c`) clears pending clipboard events.
  - Clipboard OSC input still does not mutate screen text, title, pwd,
    hyperlink, cursor, modes, colors, dirty rows, or PTY response.
- Run:
  - `cargo fmt -p roastty`
  - `cargo fmt -p roastty -- --check`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/782-terminal-clipboard-osc-events.md`
- Run:
  - `git diff --check`

The experiment passes if terminal clipboard OSC actions are retained and
drainable without changing existing terminal rendering/effect behavior, and all
focused verification passes. It is Partial if the event queue works only for OSC
52 or only for Kitty clipboard. It fails if retaining these actions requires
surface request allocation or runtime callback wiring in the same experiment.

## Design Review

Codex reviewed the initial design and approved the terminal-owned event queue
shape, but found one real gap: reset coverage only mentioned terminal reset and
did not explicitly include the full-reset/RIS path (`ESC c`) dispatched as
`Action::FullReset`.

The design was updated to require both direct reset and RIS/full reset to clear
pending clipboard OSC events, with explicit verification coverage for each.

## Result

**Result:** Pass

The terminal now owns a pending clipboard OSC event queue. `Terminal::osc`
retains OSC 52 clipboard actions as `TerminalClipboardEvent::Osc52` values with
the kind byte and raw data bytes, and retains Kitty clipboard OSC 5522 actions
as `TerminalClipboardEvent::Kitty` values with metadata bytes, optional payload
bytes, and the OSC terminator.

`Terminal::drain_clipboard_events` returns pending events in parse order and
clears the queue. Direct terminal reset and RIS/full reset (`ESC c`) both clear
pending clipboard OSC events. The retained events do not mutate normal terminal
screen text, title, pwd, hyperlink, cursor, modes, colors, dirty rows, or PTY
response.

This experiment intentionally stops at the terminal bridge. Termio worker event
emission, surface request allocation, runtime clipboard callbacks, OSC replies,
and Kitty clipboard semantics remain later work.

Verification passed:

- `cargo test -p roastty terminal_clipboard -- --nocapture --test-threads=1` — 3
  passed
- `cargo test -p roastty terminal_stream_clipboard -- --nocapture --test-threads=1`
  — 1 passed
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/782-terminal-clipboard-osc-events.md`
- `git diff --check`

## Conclusion

Roastty no longer drops parsed terminal clipboard OSC actions inside
`Terminal::osc`. There is now a tested, drainable terminal-side event stream
that later termio/surface experiments can use to allocate OSC 52 read/write
requests and route runtime clipboard callbacks.
