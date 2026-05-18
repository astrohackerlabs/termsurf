+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

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

## Experiment 1: Forward all terminal events

### Changes

Three edits in `tui/src/main.rs`:

#### 1. Replace `LoopEvent::Key` with `LoopEvent::Terminal` (line 54)

```rust
enum LoopEvent {
    Terminal(Event),
    Xpc(xpc::CompositorMessage),
}
```

The `KeyEvent` import can be removed — it's no longer used in the enum (the main
loop destructures `Event::Key(key)` directly).

#### 2. Forward all events from the reader thread (line 189)

Replace:

```rust
std::thread::spawn(move || loop {
    if let Ok(Event::Key(key)) = event::read() {
        if key_tx.send(LoopEvent::Key(key)).is_err() {
            break;
        }
    }
});
```

With:

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

#### 3. Unwrap `LoopEvent::Terminal` in the main loop (line 266)

Replace:

```rust
Ok(LoopEvent::Key(key)) => {
```

With:

```rust
Ok(LoopEvent::Terminal(Event::Key(key))) => {
```

Add a catch-all arm before `Err(_)` for non-key terminal events:

```rust
Ok(LoopEvent::Terminal(_)) => {
    // Resize, mouse, focus, paste, etc. — just redraw.
}
```

No other changes needed. The existing key handling code is untouched — only the
outer match arm pattern changes from `LoopEvent::Key(key)` to
`LoopEvent::Terminal(Event::Key(key))`.

### Refinement: filter mouse events

The initial implementation forwarded all crossterm events, which would cause a
full redraw on every pixel of mouse movement. The reader thread now filters by
event type — only `Key`, `Resize`, and `Paste` are forwarded. `Mouse`,
`FocusGained`, and `FocusLost` are dropped silently.

```rust
Ok(ev @ (Event::Key(_) | Event::Resize(_, _) | Event::Paste(_))) => {
    if key_tx.send(LoopEvent::Terminal(ev)).is_err() {
        break;
    }
}
Ok(_) => {} // Mouse, FocusGained, FocusLost — drop silently.
```

### Result: PASS

Resize works immediately on window drag and split pane creation. Keyboard input
and XPC messages are unaffected.

## Conclusion

Issue 666 moved crossterm event reading into a dedicated thread but only
forwarded `Event::Key`, silently dropping `Event::Resize` and all other event
types. The TUI never saw resize signals and only redrew when a keystroke
happened to arrive.

The fix widens `LoopEvent` from `Key(KeyEvent)` to `Terminal(Event)` and
forwards `Key`, `Resize`, and `Paste` from the reader thread. Mouse and focus
events are filtered out to avoid unnecessary redraws. The main loop's key
handling is unchanged — only the outer match arm pattern changed from
`LoopEvent::Key(key)` to `LoopEvent::Terminal(Event::Key(key))`.

Changes in `tui/src/main.rs`:

- `LoopEvent` enum: `Key(KeyEvent)` → `Terminal(Event)`
- Reader thread: `if let Ok(Event::Key(key))` → filtered `match` forwarding
  `Key`, `Resize`, `Paste`
- Main loop: `Ok(LoopEvent::Key(key))` →
  `Ok(LoopEvent::Terminal(Event::Key(key)))`, plus `Ok(LoopEvent::Terminal(_))`
  catch-all for resize/paste
