+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 666: Devil's Esc — Browse Mode Escape Latency

Eliminate the ~250ms delay when pressing `Esc` to exit Browse mode.

## Problem

Pressing `Esc` in Browse mode has a noticeable delay before the TUI switches to
Control mode. In Edit and Command modes, `Esc` is instant. The difference is the
input path:

- **Edit/Command modes**: `Esc` flows through the terminal as a normal
  keystroke. `event::poll()` returns immediately when the key arrives via
  crossterm.
- **Browse mode**: Zig intercepts `Esc` in `Surface.zig` (Issue 665), consumes
  it, and sends an XPC `ModeChanged` message to the TUI. The key never reaches
  the terminal.

The TUI event loop (`tui/src/main.rs`, line ~249) blocks inside
`event::poll(Duration::from_millis(250))` waiting for a terminal key event. XPC
messages are only drained after the poll returns (line ~402 via
`conn.try_recv()`). When `Esc` is consumed by Zig, no terminal event arrives, so
the poll blocks for up to 250ms before the TUI checks XPC and sees the mode
change.

The 250ms value was never a deliberate choice — it came from the original TUI
scaffold commit, copied from a standard crossterm/ratatui tutorial example. At
that time there was no XPC communication, just a simple `q`-to-quit loop.

## Solution

Unify crossterm key events and XPC messages into a single `mpsc::channel` so the
main loop blocks on one source and wakes instantly on either event type. No
polling, no arbitrary timeout, zero CPU when idle.

### Architecture

```
Thread 1 (crossterm):  event::read() → tx.send(LoopEvent::Key(key))
XPC callback (xpc.rs): → tx.send(LoopEvent::Xpc(msg))
Main loop:             rx.recv() → handle immediately
```

Currently, `CompositorConnection` creates its own internal
`mpsc::channel<CompositorMessage>` (xpc.rs line ~176) and the XPC event handler
pushes messages into it. The main loop calls `conn.try_recv()` to non-blockingly
drain these messages after the crossterm poll returns.

The change: instead of creating an internal channel, `CompositorConnection`
accepts an external `Sender<LoopEvent>` so XPC messages go directly to the
unified channel. A dedicated thread blocks on `crossterm::event::read()` and
sends key events to the same channel. The main loop does `rx.recv()` — blocking
with zero CPU until either source fires.

## Experiment 1: Unified event channel

### Hypothesis

Replacing the two-source poll-then-drain loop with a single unified
`mpsc::channel` will eliminate Browse mode Esc latency, making it feel as
instant as Edit/Command mode Esc.

### Changes

1. **Define `LoopEvent` enum** in `tui/src/main.rs`:

   ```rust
   enum LoopEvent {
       Key(crossterm::event::KeyEvent),
       Xpc(xpc::CompositorMessage),
   }
   ```

2. **Modify `CompositorConnection`** in `tui/src/xpc.rs`:
   - Add a `connect_with_sender(tx: Sender<LoopEvent>)` method (or modify
     `connect()` to accept `Option<Sender<LoopEvent>>`) so the XPC event handler
     sends `LoopEvent::Xpc(msg)` directly to the unified channel instead of the
     internal one.
   - Remove the internal `mpsc::channel` and `try_recv()` method since XPC
     messages now go directly to the unified channel.

3. **Spawn crossterm reader thread** in `tui/src/main.rs`:

   ```rust
   let (tx, rx) = std::sync::mpsc::channel();
   let key_tx = tx.clone();
   std::thread::spawn(move || {
       loop {
           if let Ok(Event::Key(key)) = crossterm::event::read() {
               if key_tx.send(LoopEvent::Key(key)).is_err() {
                   break;
               }
           }
       }
   });
   ```

4. **Replace the event loop** in `tui/src/main.rs`:

   Before:

   ```rust
   if event::poll(Duration::from_millis(250))? {
       if let Event::Key(key) = event::read()? {
           // handle key...
       }
   }
   // ...later...
   while let Some(msg) = conn.try_recv() {
       // handle XPC message...
   }
   ```

   After:

   ```rust
   // Non-blocking: drain all pending events, then block for the next one.
   loop {
       let event = match rx.try_recv() {
           Ok(e) => e,
           Err(_) => match rx.recv() {
               Ok(e) => e,
               Err(_) => break,
           },
       };
       match event {
           LoopEvent::Key(key) => { /* handle key */ }
           LoopEvent::Xpc(msg) => { /* handle XPC message */ }
       }
       // After handling, drain any remaining queued events before redrawing.
   }
   ```

5. **Pass `tx` to `CompositorConnection`** — the `connect()` call passes
   `tx.clone()` so XPC callbacks send directly to the unified channel.

### Test

1. `cd tui && cargo build` — compiles without errors
2. In Browse mode, press `Esc` — exits to Control instantly (no perceptible
   delay)
3. In Edit mode (Insert), press `Esc` — enters Normal mode instantly. Press
   `Esc` again — exits to Control instantly
4. In Command mode, same as Edit — both Esc transitions are instant
5. XPC messages (URL changes, loading state, title changes) still work correctly
6. Idle CPU usage is ~0% (no busy polling)
