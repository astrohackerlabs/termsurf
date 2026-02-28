# Issue 668: TUI doesn't resize

## Problem

When resizing the window or opening a split pane, the TUI (Rust/ratatui browser
chrome) does not resize. The terminal panes resize correctly — only the TUI is
broken. Typing a key causes the TUI to redraw at the correct size, confirming
it's a missing event, not a layout bug.

## Root Cause

Issue 666 ("Slay the 250ms poll dragon") moved crossterm event reading into a
dedicated thread that only forwards `Event::Key` to the unified channel:

```rust
std::thread::spawn(move || loop {
    if let Ok(Event::Key(key)) = event::read() {
        if key_tx.send(LoopEvent::Key(key)).is_err() {
            break;
        }
    }
});
```

`Event::Resize` events are silently dropped by the `Event::Key` pattern match.
The old code used `event::poll` + `event::read` in the main loop where all event
types — including `Event::Resize` — were processed. The main loop called
`terminal.draw()` on every iteration, so resize events naturally triggered a
redraw at the new dimensions.

After Issue 666, the main loop blocks on `rx.recv()` and only wakes for
`LoopEvent::Key` or `LoopEvent::Xpc`. There is no `LoopEvent::Resize` variant,
so terminal resize signals (SIGWINCH → crossterm `Event::Resize`) are consumed
by the reader thread and discarded.

## Fix

Forward all crossterm events through the channel instead of filtering for
`Event::Key`. This avoids silently dropping any event type — resize, mouse,
focus, paste, or future crossterm additions.

Replace `LoopEvent::Key(KeyEvent)` with `LoopEvent::Terminal(Event)`:

```rust
enum LoopEvent {
    Terminal(Event),
    Xpc(xpc::CompositorMessage),
}
```

The reader thread forwards everything:

```rust
std::thread::spawn(move || loop {
    match event::read() {
        Ok(ev) => {
            if key_tx.send(LoopEvent::Terminal(ev)).is_err() {
                break;
            }
        }
        Err(_) => break,
    }
});
```

In the main loop, match on the inner `Event` variant. Key handling stays the
same (match `Event::Key`). All other events — including `Event::Resize` — fall
through to the next `terminal.draw()` call, which picks up the new dimensions
automatically.

## Test

1. `cd tui && cargo build` — compiles without errors.
2. Open TermSurf, run `web google.com`.
3. Resize the window — TUI redraws immediately at the new size.
4. Open a split pane — TUI in the original pane resizes correctly.
5. Verify keyboard input still works.
6. Verify XPC messages (mode changes, URL updates, loading state) still work.
