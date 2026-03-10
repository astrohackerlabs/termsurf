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

**Result:** Pass

Launched Wezboard (PID 42486) with a browser pane using the dev build
(`--browser /Users/ryan/dev/termsurf/chromium/src/out/Default/roamium`).
Force-killed Wezboard with `kill -9 42486`. The dev Roamium exited cleanly — no
orphan processes for that socket. Old orphans from an earlier test using the
installed binary (without the fix) confirmed the leak still exists without the
change.

#### Conclusion

Two lines of `ts_post_task(Some(quit_trampoline), ...)` in `reader_loop` plus a
small trampoline function are sufficient. EOF and read errors now trigger a
clean Chromium shutdown via the UI thread, matching the graceful `Shutdown`
message path.

### Experiment 2: Remove Shutdown message from protocol

#### Description

Now that Roamium exits on socket EOF (Experiment 1), the explicit `Shutdown`
protobuf message is redundant. When the last pane closes, the GUI can simply
close the socket — Roamium sees EOF and quits. Removing `Shutdown` simplifies
the protocol and eliminates a message that now serves no purpose.

#### Changes

**`proto/termsurf.proto`**

1. Remove `Shutdown shutdown = 31;` from the `oneof msg` block (line 52).
2. Remove `message Shutdown {}` definition (line 279).

**`ghostboard/src/apprt/xpc.zig`**

1. Delete the `sendShutdown()` function (lines 1051–1060).
2. At the call site in `handleClientDisconnect()` (line 1749), remove the
   `sendShutdown(server)` call. The existing code already calls `proc.wait()`
   and cleans up the server afterward — closing the socket fd naturally produces
   EOF for Roamium.

**`ghostboard/src/protobuf/termsurf.pb-c.h` and `termsurf.pb-c.c`**

Regenerate from the updated proto. These are generated files — remove all
`Termsurf__Shutdown` struct definitions, function declarations
(`termsurf__shutdown__init`, `__pack`, `__unpack`, `__free_unpacked`,
`__get_packed_size`), the `TERMSURF__TERM_SURF_MESSAGE__MSG_SHUTDOWN` enum
value, and the `shutdown` union member.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

1. Remove the Shutdown message construction and send (lines 887–890). The server
   removal on line 891 (`servers_to_remove.push`) already drops the `server_tx`
   channel, closing the socket and producing EOF.

**`roamium/src/dispatch.rs`**

1. Remove the `Msg::Shutdown(_)` match arm (lines 194–196). Roamium no longer
   receives this message — it exits via EOF instead.

#### Verification

1. Regenerate protobuf-c files:
   `chromium/src/out/Default/protoc --c_out=ghostboard/src/protobuf proto/termsurf.proto`
2. Build all components:
   `./scripts/build.sh roamium && ./scripts/build.sh wezboard && ./scripts/build.sh ghostboard`
3. Launch Wezboard with a browser pane, close the pane — Roamium should exit
   (EOF from socket close).
4. Launch Wezboard with a browser pane, `kill -9` the GUI — Roamium should exit
   (EOF from crash).
5. Repeat steps 3–4 with Ghostboard.

**Result:** Pass

Removed the `Shutdown` protobuf message from the protocol, all send sites
(Ghostboard and Wezboard), and the receive site (Roamium). Regenerated
protobuf-c files. In Ghostboard, reordered the cleanup block so the client fd
closes before `proc.wait()`, ensuring Roamium sees EOF and exits instead of
blocking. Also fixed `install.sh` to use `sudo` for Ghostboard's system
directory writes.

#### Conclusion

The `Shutdown` message is gone. Socket EOF is now the sole shutdown mechanism
for Roamium — both for graceful pane close and GUI crash. The protocol drops
from 31 to 30 message types.

## Conclusion

Roamium process leaks are fixed. Experiment 1 made Roamium detect socket EOF and
quit cleanly via `ts_post_task(ts_quit)`. Experiment 2 removed the now-redundant
`Shutdown` protobuf message, simplifying the protocol. Socket EOF is the single
shutdown path — it handles both graceful close and GUI crash uniformly.
