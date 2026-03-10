# Issue 736: Roamium process leak on GUI crash

## Goal

When a GUI (Ghostboard or Wezboard) crashes, Roamium processes should detect the
dead connection and exit automatically instead of living forever as orphans.

## Background

Both Ghostboard and Wezboard manage Roamium's lifecycle. They spawn Roamium
processes, track them via `Child` process handles, and send a `Shutdown`
protobuf message when the last pane using a server closes (`pane_count == 0`).
The GUI then calls `wait()` on the child process.

If a GUI crashes, this graceful shutdown never happens. Here's the sequence:

1. The OS closes the GUI's socket file descriptors automatically.
2. Roamium's reader thread (`roamium/src/ipc.rs:48`) sees EOF and returns.
3. The Chromium main loop (`ts_content_main`) keeps running indefinitely.
4. Roamium becomes an orphaned process with no parent connection.

The reader thread exiting does nothing to stop the Chromium event loop. There is
no heartbeat, no parent PID monitoring, and no signal handling that would cause
Roamium to exit when the GUI disappears.

### Current shutdown paths

| Path       | Trigger                        | Result                                       |
| ---------- | ------------------------------ | -------------------------------------------- |
| Graceful   | GUI sends `Shutdown` message   | `ts_quit()` called, clean exit               |
| Socket EOF | GUI closes socket (or crashes) | Reader thread exits, Chromium loop continues |
| Manual     | `killall roamium`              | Process killed externally                    |

The socket EOF path is the gap — it detects disconnection but doesn't act on it.

### Proposed solution

When Roamium's reader thread detects EOF (socket closed), it should call
`ts_quit()` to shut down the Chromium event loop, just like the `Shutdown`
message handler does. This way, any socket disconnection — whether from a
graceful close or a GUI crash — triggers a clean Roamium exit.

This is the simplest fix: no new protocol messages, no heartbeat timers, no
parent PID polling. The detection mechanism already exists (`Ok(0)` in
`reader_loop`); it just needs to call `ts_quit()` instead of silently returning.

## Experiments

### Experiment 1: Call ts_quit on socket EOF

#### Description

When the reader thread detects EOF or a read error, it currently returns
silently. Instead, it should trigger `ts_quit()` to shut down the Chromium event
loop — the same thing the `Shutdown` message handler does.

One subtlety: the reader thread runs on a background thread, but `ts_quit()`
should be called from the UI thread (the `Shutdown` handler in `dispatch.rs`
calls it from the UI thread via `ts_post_task`). So the reader thread should use
`ts_post_task` to schedule a `ts_quit()` call on the UI thread, rather than
calling it directly.

#### Changes

**`roamium/src/ipc.rs`**

In `reader_loop`, replace the two silent returns (`Ok(0)` and `Err(_)`) with a
call to `ts_post_task` that schedules `ts_quit()` on the UI thread:

```rust
loop {
    let n = match stream.read(&mut tmp) {
        Ok(0) => {
            // Socket closed (GUI exited or crashed). Shut down cleanly.
            unsafe { ffi::ts_post_task(Some(quit_trampoline), std::ptr::null_mut()); }
            return;
        }
        Ok(n) => n,
        Err(_) => {
            unsafe { ffi::ts_post_task(Some(quit_trampoline), std::ptr::null_mut()); }
            return;
        }
    };
    // ... rest unchanged
}
```

Add a trampoline function at the bottom of `ipc.rs`:

```rust
/// Trampoline for ts_quit, called on the UI thread via ts_post_task.
unsafe extern "C" fn quit_trampoline(_data: *mut c_void) {
    ffi::ts_quit();
}
```

#### Verification

1. Build Roamium: `./scripts/build.sh roamium`
2. Launch Ghostboard or Wezboard with a browser pane open
3. Force-kill the GUI process: `kill -9 <pid>`
4. Check that the Roamium process exits within a few seconds:
   `ps aux | grep roamium` — should show no running processes
5. Compare with current behavior (before the fix): Roamium would remain as an
   orphan indefinitely
