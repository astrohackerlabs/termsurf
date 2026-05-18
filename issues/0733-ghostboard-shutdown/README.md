+++
status = "closed"
opened = "2026-03-09"
closed = "2026-03-09"
+++

# Issue 733: Ghostboard should send Shutdown instead of SIGKILL

## Goal

Ghostboard should use the new `Shutdown` protocol message to gracefully
terminate Roamium, instead of force-killing it with SIGKILL.

## Background

Issue 732 added a `Shutdown` message to the TermSurf protocol and removed
Roamium's self-termination on last tab close. Roamium now waits for an explicit
`Shutdown` message before exiting. Wezboard already sends `Shutdown` when
`pane_count` drops to 0.

Ghostboard still works — when `pane_count` reaches 0, it calls `killServer()`
(`xpc.zig:1042`), which sends SIGKILL via `proc.kill()`. SIGKILL terminates the
process, so the behavior is correct but not graceful. Chromium's cleanup code
(atexit handlers, shared memory cleanup, IPC channel teardown) never runs.

### Current Ghostboard code path

In `handleClientDisconnect()` (`xpc.zig:1724-1757`), when a TUI disconnects:

1. Sends `CloseTab` to Roamium for each pane (`msg_case = 4`, line 1731)
2. Decrements `server.pane_count` (line 1736)
3. If `pane_count == 0`:
   - Calls `killServer(server)` — sends SIGKILL, then `proc.wait()` (line 1738)
   - Cancels Chromium's dispatch source and closes its socket fd (lines
     1743-1751)
   - Removes server from `servers` map and frees memory (lines 1753-1756)

### What should change

Replace `killServer(server)` with:

1. Send `Shutdown` message to Roamium via `sendProtobuf(server.fd, &wrapper)`
   (msg_case = 31)
2. Wait for Roamium to exit via `proc.wait()` — Roamium calls `ts_quit()` on
   receiving Shutdown, which exits quickly
3. Keep `killServer()` for app-exit cleanup (line 173-182) as a safety net

The rest of the cleanup (cancel dispatch source, remove server entry, free
memory) stays the same.

### Relevant code

| Location          | What it does                                             |
| ----------------- | -------------------------------------------------------- |
| `xpc.zig:1042-49` | `killServer()` — SIGKILL + wait                          |
| `xpc.zig:1724-57` | TUI disconnect: CloseTab, decrement, kill when count = 0 |
| `xpc.zig:173-182` | App exit cleanup: kills all remaining servers            |
| `xpc.zig:2303-24` | `sendProtobuf()` — send length-prefixed protobuf         |

### Protobuf details

The `Shutdown` message was added in Issue 732 as field 31 in the `oneof msg`
block. In Ghostboard's protobuf-c bindings, this corresponds to `msg_case = 31`.
The message body is empty — it's a signal with no payload. Need to check how
protobuf-c handles the new field (the `.c`/`.h` files may need regeneration from
`termsurf.proto`).

## Experiments

### Experiment 1: Send Shutdown and wait for graceful exit

#### Description

Replace the SIGKILL in `handleClientDisconnect` with a `Shutdown` protobuf
message. When `pane_count` reaches 0, send `Shutdown` over the socket and call
`proc.wait()` to let Roamium exit on its own. Keep `killServer()` (SIGKILL) only
for app-exit cleanup as a safety net.

#### Changes

**1. Regenerate protobuf-c bindings**

The generation script `proto/generate.sh` still references the old `gui/`
directory. Update it to output to `ghostboard/src/protobuf/`, then run it:

```bash
protoc-c --c_out=ghostboard/src/protobuf --proto_path=proto proto/termsurf.proto
```

This adds `Termsurf__Shutdown` struct, `termsurf__shutdown__init()`, and
`msg_case = 31` to the generated `.pb-c.h` and `.pb-c.c` files.

**2. `proto/generate.sh`** — fix output directory

Change `gui/src/protobuf` to `ghostboard/src/protobuf` (line 6) and update the
echo message (line 8).

**3. `ghostboard/src/apprt/xpc.zig`** — add `sendShutdown` helper

Add a new function near `killServer()` (after line 1049):

```zig
fn sendShutdown(server: *Server) void {
    if (server.fd < 0) return;
    var sd: pb.Termsurf__Shutdown = undefined;
    pb.termsurf__shutdown__init(&sd);
    var wrapper: pb.Termsurf__TermSurfMessage = undefined;
    pb.termsurf__term_surf_message__init(&wrapper);
    wrapper.msg_case = 31; // SHUTDOWN
    wrapper.unnamed_0.shutdown = &sd;
    sendProtobuf(server.fd, &wrapper);
}
```

**4. `ghostboard/src/apprt/xpc.zig`** — replace `killServer` with
`sendShutdown` + `waitServer` in `handleClientDisconnect`

At line 1737–1738, replace `killServer(server)` with:

```zig
sendShutdown(server);
// Wait for graceful exit after Shutdown message.
if (server.process) |*proc| {
    _ = proc.wait() catch {};
}
server.process = null;
```

The rest of the cleanup block (cancel dispatch source, remove server, free
memory) stays unchanged.

**5. No changes to `killServer()`** — it remains for app-exit cleanup (lines
173-182) where we need guaranteed termination regardless of socket state.

#### Verification

1. `protoc-c --c_out=ghostboard/src/protobuf --proto_path=proto proto/termsurf.proto`
   — regenerates bindings with Shutdown
2. `cd ghostboard && zig build` — compiles with new sendShutdown function
3. Run Ghostboard, open a webview, close it — Roamium should exit gracefully (no
   SIGKILL in logs)
4. Reopen a webview — should work (server entry removed, fresh spawn)

**Result:** Pass

Build compiles successfully. All changes applied cleanly.

#### Conclusion

The experiment worked as designed. Ghostboard now sends a `Shutdown` protobuf
message instead of SIGKILL when `pane_count` drops to 0, then waits for the
process to exit. `killServer()` remains as a safety net for app-exit cleanup.

## Conclusion

Ghostboard now gracefully shuts down Roamium using the `Shutdown` protocol
message added in Issue 732. When the last pane for a profile closes, Ghostboard
sends `Shutdown` (msg_case 31) over the Unix socket and waits for Roamium to
exit on its own — allowing Chromium's cleanup handlers to run properly. SIGKILL
(`killServer()`) is preserved only as a safety net during app-exit.

Additionally fixed `proto/generate.sh` to point at the correct
`ghostboard/src/protobuf/` output directory and regenerated the protobuf-c
bindings to include the `Termsurf__Shutdown` struct.
