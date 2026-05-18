+++
status = "closed"
opened = "2026-03-03"
closed = "2026-03-03"
+++

# Issue 700: Replace TUI↔GUI XPC with Unix Sockets

## Goal

Replace the TUI↔GUI IPC channel with Unix domain sockets + protobuf. The
GUI↔Chromium channel stays on XPC (a separate issue). This eliminates the
xpc-gateway daemon for TUI connections and removes all ObjC FFI from the TUI.

## Background

Issue 698 proved the full stack across four experiments:

1. Protobuf works in all three languages (Experiment 1)
2. The full 30-message schema compiles and round-trips (Experiment 2)
3. Unix sockets work across Zig and Rust (Experiment 3)
4. Production integration was blocked only by build system (Experiment 4)

Issue 699 solved the build blocker: protobuf-c compiles and links into the GUI's
final macOS binary using the stb.c pattern (`gui/src/protobuf/`).

### What changes

**TUI (`tui/src/xpc.rs` → `tui/src/ipc.rs`):** Replace 710 lines of unsafe XPC
FFI with pure Rust sockets + prost. No more `block2`, no more `extern "C"`, no
more `CString`. The public API stays identical — `CompositorConnection` with the
same methods.

**GUI (`gui/src/apprt/xpc.zig`):** Add a Unix socket listener alongside the
existing XPC listener. TUI messages arrive over the socket; Chromium messages
still arrive over XPC. Both feed into the same handler functions. The TUI's
`web_peer` (currently an `xpc_object_t`) becomes a socket fd for sending replies
and events back.

**Gateway:** No longer needed for TUI connections. Still needed for Chromium
(until that channel is also migrated). The gateway can be removed entirely once
both channels use sockets.

### Architecture

```
Before:  TUI --XPC--> Gateway --endpoint--> GUI
After:   TUI --socket+protobuf--> GUI (direct)
```

The GUI listens on a Unix domain socket. The TUI connects by path. No
intermediary, no endpoint handshake, no launchd dependency.

### Socket path

```
$TMPDIR/termsurf/gui.sock          (macOS release)
$TMPDIR/termsurf/gui-debug.sock   (macOS debug)
$XDG_RUNTIME_DIR/termsurf/gui.sock (Linux)
```

The GUI creates the socket on startup, removes it on shutdown. Stale sockets
(from crashes) are detected by attempting connect — if refused, the file is
stale and can be replaced.

### Wire format

Same as Issue 698 Experiment 3: 4-byte little-endian length prefix before each
serialized `TermSurfMessage`.

```
[4 bytes: message length (u32 LE)] [N bytes: serialized TermSurfMessage]
```

### Message inventory

TUI → GUI (commands and queries):

| Message                | Type        | Fields                                                      |
| ---------------------- | ----------- | ----------------------------------------------------------- |
| `SetOverlay`           | fire-forget | pane_id, col, row, width, height, url, profile, browsing    |
| `SetDevtoolsOverlay`   | fire-forget | pane_id, col, row, width, height, inspected_tab_id, profile |
| `Navigate`             | fire-forget | pane_id, url                                                |
| `SetColorScheme`       | fire-forget | pane_id, dark                                               |
| `ModeChanged`          | fire-forget | pane_id, browsing                                           |
| `OpenSplit`            | fire-forget | pane_id, direction, command                                 |
| `HelloRequest`         | sync query  | pane_id                                                     |
| `QueryLastRequest`     | sync query  | pane_id, profile                                            |
| `QueryDevtoolsRequest` | sync query  | pane_id, inspected_tab_id, profile                          |
| `QueryTabsRequest`     | sync query  | pane_id, profile                                            |

GUI → TUI (replies and events):

| Message              | Type        | Fields                                                                     |
| -------------------- | ----------- | -------------------------------------------------------------------------- |
| `HelloReply`         | sync reply  | homepage                                                                   |
| `QueryLastReply`     | sync reply  | pane_id, tab_id, profile, error                                            |
| `QueryDevtoolsReply` | sync reply  | tab_id, error                                                              |
| `QueryTabsReply`     | sync reply  | gui_panes, chromium_tabs, chromium_browser, chromium_devtools, tabs, error |
| `ModeChanged`        | async event | browsing                                                                   |
| `UrlChanged`         | async event | tab_id (ignored by TUI), url                                               |
| `LoadingState`       | async event | tab_id (ignored by TUI), state, progress                                   |
| `TitleChanged`       | async event | tab_id (ignored by TUI), title                                             |

### Synchronous queries

The TUI has 4 synchronous queries (`hello`, `query_last`, `query_devtools`,
`query_tabs`). With XPC, these use
`xpc_connection_send_message_with_reply_sync`.

With sockets, the TUI spawns a reader thread that reads all incoming messages.
Reply messages route to a pending-reply channel; event messages route to the
existing `LoopEvent` mpsc channel. Since the TUI sends queries one at a time
(single-threaded main loop), there is at most one pending query. The query
method writes the request, then blocks on the reply channel.

### Thread safety

The GUI already uses a serial GCD queue (`xpc_queue`) for all XPC handlers. The
socket reader uses `dispatch_source` (`DISPATCH_SOURCE_TYPE_READ`) targeting the
same queue. Same serialization guarantee — no additional mutexes needed.

### SetColorScheme field mapping

The current XPC `set_color_scheme` sends a `scheme` string (`"dark"`, `"light"`,
`"system"`). The protobuf `SetColorScheme` message has a `dark` bool. The TUI
will convert: `"dark"` → `dark=true`, `"light"` → `dark=false`, `"system"` →
`dark=false` (let the GUI decide). The GUI handler already converts the scheme
string to a bool internally, so this simplifies the path.

### ModeChanged field mapping

The current XPC `mode_changed` from TUI→GUI sends `pane_id` + `browsing`. The
protobuf `ModeChanged` message only has `browsing`. The `pane_id` needs to be
added to the protobuf message — update `proto/termsurf.proto` to add
`string pane_id = 2` to `ModeChanged`.

Similarly, the GUI→TUI `ModeChanged` currently only sends `browsing` (no
pane_id), which matches the protobuf schema. No change needed for that
direction.

## Ideas for experiments

- **TUI socket client.** Create `tui/src/ipc.rs` with the same public API as
  `xpc.rs` but using Unix sockets + prost. Replace `block2` ObjC FFI with pure
  Rust. Switch `main.rs` from `mod xpc` to `mod ipc`.

- **GUI socket listener.** Add a Unix domain socket listener to `xpc.zig`.
  Accept TUI connections, read length-prefixed protobuf messages, dispatch to
  the existing handler functions. Send replies and events back over the socket.
  Add `web_fd` to `Pane` alongside `web_peer`.

- **Proto schema update.** Add `string pane_id = 2` to `ModeChanged` (needed for
  TUI→GUI direction). Regenerate GUI and TUI protobuf files.

- **End-to-end integration.** Both sides compiled and working together. Full
  verification of all 10 TUI→GUI and 8 GUI→TUI message types.

## Experiments

### Experiment 1: TUI socket client

Replace `tui/src/xpc.rs` (710 lines of unsafe XPC FFI) with `tui/src/ipc.rs`
(pure Rust sockets + prost). Same public API — `CompositorConnection` with
identical methods. Verify it compiles.

#### Changes

**1. Update `proto/termsurf.proto`**

Add `string pane_id = 2` to `ModeChanged`. The TUI→GUI direction needs `pane_id`
to identify which pane changed mode. The GUI→TUI direction ignores it (the TUI
only has one pane).

**2. Regenerate protobuf files**

Run `proto/generate.sh` to update `gui/src/protobuf/termsurf.pb-c.{c,h}`.

**3. Update `tui/Cargo.toml`**

Replace `block2` with `prost`. Add `prost-build` to build-dependencies.

```toml
[dependencies]
prost = "0.14"

[build-dependencies]
prost-build = "0.14"
```

**4. Create `tui/build.rs`**

Prost code generation — compiles `proto/termsurf.proto` at build time.

```rust
fn main() {
    prost_build::Config::new()
        .compile_protos(&["../proto/termsurf.proto"], &["../proto/"])
        .unwrap();
}
```

**5. Create `tui/src/ipc.rs`**

Same public types and methods as `xpc.rs`:

- `CompositorMessage` enum (4 variants: `ModeChanged`, `UrlChanged`,
  `LoadingState`, `TitleChanged`)
- `CompositorConnection` struct
- `connect(tx)` → builds socket path from `$TMPDIR`, connects via `UnixStream`,
  spawns reader thread
- 6 fire-and-forget methods: `send_set_overlay`, `send_set_devtools_overlay`,
  `send_navigate`, `send_set_color_scheme`, `send_open_split`,
  `send_mode_changed`
- 4 sync query methods: `send_hello`, `send_query_last`, `send_query_devtools`,
  `send_query_tabs`

Internals:

- Socket path: `$TMPDIR/termsurf/gui.sock`
- Wire format: 4-byte LE length prefix + serialized `TermSurfMessage`
- Reader thread: reads length-prefixed messages in a loop, routes reply messages
  to a `reply_tx` channel and event messages to the `LoopEvent` mpsc channel
- Sync queries: write request → block on `reply_rx` → return result
- `send_set_color_scheme`: converts scheme string (`"dark"`, `"light"`,
  `"system"`) to `dark` bool before sending
- `Drop`: closes the `UnixStream` (reader thread exits on EOF)

**6. Switch `main.rs` from `xpc` to `ipc`**

- `mod xpc` → `mod ipc`
- `xpc::CompositorMessage` → `ipc::CompositorMessage`
- `xpc::CompositorConnection` → `ipc::CompositorConnection`
- `LoopEvent::Xpc` → `LoopEvent::Ipc`

#### Verification

```bash
cd tui && cargo build
```

**Pass criterion:** Compiles with zero errors. No runtime test — the GUI socket
listener doesn't exist yet.

#### Result: PASS

`cargo build` compiles with zero errors and zero warnings. The TUI is now 100%
pure Rust — no `block2`, no `extern "C"`, no `CString`, no `unsafe`. The
`ipc.rs` module (265 lines) replaces `xpc.rs` (710 lines) with the same public
API.

The GUI also rebuilds cleanly with the regenerated protobuf files (adding
`pane_id` to `ModeChanged`).

### Experiment 2: GUI socket listener

Add a Unix domain socket listener to `xpc.zig`. Accept TUI connections, read
length-prefixed protobuf messages, dispatch to handler functions. Send replies
and events back over the socket. XPC remains for Chromium — both transports
coexist.

#### Changes

**1. Add socket state to `xpc.zig`**

New module-level variables:

```zig
var sock_fd: std.posix.fd_t = -1;
var tui_fd: std.posix.fd_t = -1;  // single TUI connection (only one TUI per GUI)
var tui_buf: [65536]u8 = undefined;
var tui_buf_len: usize = 0;
```

Add `web_fd: std.posix.fd_t = -1` to the `Pane` struct. When `web_fd >= 0`, the
pane's TUI is connected via socket (not XPC). When `web_peer != null`, it's XPC.

**2. Add POSIX and dispatch_source externs**

Zig's `std.posix` provides `socket`, `bind`, `listen`, `close`, `unlink`. For
`accept`, `read`, and `write` on the socket, use `std.posix` as well.

For async I/O on the `xpc_queue`, add dispatch_source externs:

```zig
extern "c" fn dispatch_source_create(
    type: *const anyopaque, handle: usize, mask: u64, queue: ?*anyopaque
) ?*anyopaque;
extern "c" fn dispatch_source_set_event_handler_f(
    source: ?*anyopaque, handler: *const fn (?*anyopaque) callconv(.c) void
) void;
extern "c" fn dispatch_source_set_cancel_handler_f(
    source: ?*anyopaque, handler: ?*const fn (?*anyopaque) callconv(.c) void
) void;
extern "c" fn dispatch_resume(object: ?*anyopaque) void;
extern "c" fn dispatch_source_cancel(source: ?*anyopaque) void;
extern "c" fn dispatch_set_context(object: ?*anyopaque, context: ?*anyopaque) void;
extern const _dispatch_source_type_read: anyopaque;
```

**3. Socket creation in `init()`**

After XPC setup, create the Unix socket listener:

- Build path: `$TMPDIR/termsurf/gui.sock` (release) or
  `$TMPDIR/termsurf/gui-debug.sock` (debug)
- `std.fs.cwd().makePath()` the directory
- `std.posix.unlink()` any stale socket
- `std.posix.socket(AF.UNIX, SOCK.STREAM, 0)` → `sock_fd`
- `std.posix.bind()` with `sockaddr_un`
- `std.posix.listen(sock_fd, 1)` — backlog 1 (only one TUI)
- Create `dispatch_source(DISPATCH_SOURCE_TYPE_READ, sock_fd)` on `xpc_queue`
- Set event handler → `acceptHandler`
- `dispatch_resume`

**4. Accept handler**

When the listener socket is readable:

```
fn acceptHandler() {
    tui_fd = std.posix.accept(sock_fd, ...)
    // Create dispatch_source(READ, tui_fd) on xpc_queue
    // Set event handler → clientReadHandler
    // dispatch_resume
}
```

Only one TUI connection at a time. If `tui_fd >= 0` already, close the old one.

**5. Client read handler**

When the client socket is readable:

```
fn clientReadHandler() {
    n = std.posix.read(tui_fd, tui_buf[tui_buf_len..])
    if n == 0: // EOF — TUI disconnected
        handleTuiDisconnect()
        return
    tui_buf_len += n
    // Extract complete length-prefixed messages
    while tui_buf_len >= 4:
        msg_len = read_u32_le(tui_buf[0..4])
        if tui_buf_len < 4 + msg_len: break
        pb_msg = termsurf__term_surf_message__unpack(null, msg_len, tui_buf[4..])
        if pb_msg != null:
            handleSocketMessage(tui_fd, pb_msg)
            termsurf__term_surf_message__free_unpacked(pb_msg, null)
        shift tui_buf left by 4 + msg_len
}
```

**6. `handleSocketMessage` — protobuf dispatcher**

Switch on `pb_msg.msg_case`:

- `MSG_SET_OVERLAY` → extract fields from `pb_msg.set_overlay`, call existing
  `handleSetOverlay` logic. Register `web_fd = client_fd` on the new pane
  (instead of `web_peer` from XPC).
- `MSG_SET_DEVTOOLS_OVERLAY` → same pattern, register `web_fd`.
- `MSG_NAVIGATE` → extract `pane_id` + `url`, call existing navigate logic.
- `MSG_SET_COLOR_SCHEME` → extract `pane_id` + `dark`, forward to Chromium. The
  TUI already converts scheme→bool (Experiment 1), so the GUI receives a bool
  directly — skip the scheme string resolution.
- `MSG_MODE_CHANGED` → extract `pane_id` + `browsing`, call existing logic.
- `MSG_OPEN_SPLIT` → extract fields, dispatch to main thread.
- `MSG_HELLO_REQUEST` → look up surface config, build `HelloReply` protobuf,
  send back over socket.
- `MSG_QUERY_LAST_REQUEST` → same logic as `handleQueryLast`, build
  `QueryLastReply` protobuf, send back.
- `MSG_QUERY_DEVTOOLS_REQUEST` → same logic, build `QueryDevtoolsReply`, send
  back.
- `MSG_QUERY_TABS_REQUEST` → same logic, forward to Chromium via XPC, build
  `QueryTabsReply` protobuf, send back.

For the 6 fire-and-forget handlers, the logic is identical to the existing XPC
handlers — the only difference is where `web_fd` comes from (socket fd instead
of XPC peer). To avoid duplicating all the handler code, the implementation will
construct XPC dictionaries from protobuf fields and feed them into the existing
`handleMessage()`. This reuses all handler logic unchanged. The key difference:
when creating a new pane from a socket connection, set `web_fd = client_fd`
instead of retaining an XPC peer.

For the 4 sync queries, the implementation will add a `socket_reply_fd` thread-
local that the existing handlers check. If `>= 0`, they build a protobuf reply
and send it over the socket instead of using `xpc_dictionary_create_reply`.

Actually, simpler: `handleSocketMessage` handles queries directly (doesn't go
through `handleMessage`), while fire-and-forget messages go through the XPC-dict
adapter.

**7. `sendProtobuf` helper**

```zig
fn sendProtobuf(fd: std.posix.fd_t, wrapper: *pb.Termsurf__TermSurfMessage) void {
    const size = pb.termsurf__term_surf_message__get_packed_size(wrapper);
    // Stack buffer for small messages, heap for large.
    var buf: [4096]u8 = undefined;
    const len_bytes = @as([4]u8, @bitCast(@as(u32, @intCast(size))));
    // Write 4-byte LE length + packed protobuf.
    _ = std.posix.write(fd, &len_bytes);
    pb.termsurf__term_surf_message__pack(wrapper, buf[0..]);
    _ = std.posix.write(fd, buf[0..size]);
}
```

**8. Modify event forwarders**

In `handleLoadingState`, `handleUrlChanged`, `handleTitleChanged`, and
`sendModeToWeb`: check `p.web_fd >= 0` first. If so, build the corresponding
protobuf event message and call `sendProtobuf`. Otherwise, use the existing XPC
path.

**9. TUI disconnect handler**

When the TUI socket closes (read returns 0):

- Find all panes where `web_fd == tui_fd`
- For each: clean up overlay, close Chromium tab, decrement server pane count
- Reset `tui_fd = -1`, `tui_buf_len = 0`
- Cancel the client dispatch_source

**10. Socket cleanup in `deinit()`**

- Close `tui_fd` if open
- Close `sock_fd` if open
- Unlink the socket file
- Cancel dispatch_sources

#### Verification

```bash
cd gui && zig build
```

**Pass criterion:** Compiles with zero errors. No runtime test — the TUI and GUI
have not been tested end-to-end together yet. That is Experiment 3.

#### Result: PASS

`zig build` compiles with zero errors across all three targets (macOS, iOS,
iOS-simulator).

Key implementation details:

- **Socket infrastructure:** `initSocket()` creates `$TMPDIR/termsurf/gui.sock`
  (or `gui-debug.sock` in debug), binds, listens(1). `dispatch_source` on the
  serial `xpc_queue` handles accept and read — same serialization as XPC.
- **Wire format:** 4-byte LE length prefix + serialized protobuf-c, matching the
  TUI's Rust side exactly.
- **XPC-dict adapter:** Fire-and-forget messages (set_overlay, navigate,
  set_color_scheme, mode_changed, open_split, set_devtools_overlay) construct an
  XPC dictionary from protobuf fields and call the existing `handleMessage()`.
  Zero handler duplication.
- **Direct protobuf replies:** Sync queries (hello, query_last, query_devtools,
  query_tabs) build protobuf reply structs directly and send them back over the
  socket via `sendProtobuf()`.
- **Event forwarders:** `handleLoadingState`, `handleUrlChanged`,
  `handleTitleChanged`, `sendModeToWeb` check `p.web_fd >= 0` and send protobuf
  when a socket client is connected, falling back to XPC otherwise.
- **Cleanup:** `handleTuiDisconnect()` closes all socket-connected panes, closes
  their Chromium tabs, and kills idle servers. `deinit()` cleans up the socket
  file and dispatch sources.

Zig ↔ protobuf-c quirks resolved during build:

1. `error` is a Zig keyword → use `reply.@"error"` for the error field.
2. Anonymous C unions in protobuf-c → access via `.unnamed_0.field_name`.
3. `msg_case` is `c_uint`, not an enum → use directly, no `@intFromEnum`.
4. Union field pointers are `[*c]T` → add explicit `*T` type annotation on
   `orelse` unwrap to enable `.field` access.

### Experiment 3: End-to-end integration

Both sides compile (Experiments 1–2). Now wire them together and verify the full
round-trip at runtime: TUI connects via socket, browser overlay appears, events
flow back.

#### Problem: socket path mismatch in debug

The GUI creates `gui-debug.sock` in debug builds, but the TUI always connects to
`gui.sock`. The same pattern as `TERMSURF_XPC_SERVICE` (Issue 653) — debug
builds need an env var so child processes discover the correct socket path.

#### Changes

**1. GUI: set `TERMSURF_SOCKET` in debug builds (`xpc.zig`)**

In `init()`, after `initSocket()`, set the env var pointing to the actual socket
path. Debug and release both benefit — the TUI gets the exact path the GUI
created.

```zig
// After initSocket():
if (sock_fd >= 0) {
    _ = internal_os.setenv("TERMSURF_SOCKET", sock_path_buf[0..sock_path_len :0]);
}
```

This runs for all builds (not just debug). The TUI checks the env var first,
falls back to the default path. Release builds get the same path either way, so
no behavior change. Debug builds get the correct `gui-debug.sock` path.

**2. TUI: check `TERMSURF_SOCKET` env var (`ipc.rs`)**

In `CompositorConnection::connect()`, check `TERMSURF_SOCKET` first. If not set,
build the default path from `$TMPDIR`.

```rust
let sock_path = std::env::var("TERMSURF_SOCKET").unwrap_or_else(|_| {
    let tmpdir = std::env::var("TMPDIR").unwrap_or_else(|_| "/tmp".to_string());
    format!("{}/termsurf/gui.sock", tmpdir.trim_end_matches('/'))
});
```

**3. TUI: rebuild with `cargo build`**

Verify Rust side still compiles after the path change.

**4. GUI: rebuild with `zig build`**

Verify Zig side still compiles after adding the env var.

#### Verification

1. Build both:
   ```bash
   cd gui && zig build
   cd tui && cargo build
   ```
2. Launch the GUI: `open gui/zig-out/TermSurf.app`
3. Open a terminal pane in TermSurf
4. Verify the socket exists: `ls $TMPDIR/termsurf/gui-debug.sock`
5. Run `web google.com` in the terminal pane
6. Check GUI logs for `socket accept` and `socket set_overlay` messages
7. Verify: browser overlay appears, page loads, URL bar updates

**Pass criterion:** The TUI connects to the GUI via Unix socket, sends
`SetOverlay`, and a Chromium browser overlay renders in the terminal pane.
Events (LoadingState, UrlChanged, TitleChanged) flow back to the TUI and update
the status bar. If runtime bugs surface, fix them and document.

#### Result: PASS

Full end-to-end round-trip works. The GUI creates `gui-debug.sock` at
`$TMPDIR/termsurf/`, the TUI discovers it via `TERMSURF_SOCKET`, connects, and
web pages render in the terminal pane.

Code changes:

- **GUI (`xpc.zig`):** Set `TERMSURF_SOCKET` env var after `initSocket()` so
  child processes discover the correct socket path.
- **TUI (`ipc.rs`):** Check `TERMSURF_SOCKET` first, fall back to default
  `$TMPDIR/termsurf/gui.sock`.

### Experiment 4: Dead code cleanup

Delete `tui/src/xpc.rs` — the 710-line ObjC FFI module replaced by `ipc.rs` in
Experiment 1. It's completely dead: `main.rs` uses `mod ipc`, `Cargo.toml` has
no `block2` dependency, and nothing imports it.

#### Changes

**1. Delete `tui/src/xpc.rs`**

Remove the file. 710 lines of unsafe ObjC FFI gone.

**2. Verify build**

```bash
cd tui && cargo build
```

#### Verification

```bash
cd tui && cargo build
```

**Pass criterion:** Compiles with zero errors. No references to xpc remain in
the TUI source (aside from the comment in `ipc.rs` noting it replaced xpc.rs).

#### Result: PASS

`cargo build` succeeds. `tui/src/xpc.rs` deleted — 710 lines of unsafe ObjC FFI
removed.

## Conclusion

Issue 700 replaces TUI↔GUI XPC with Unix domain sockets + protobuf across four
experiments:

1. **TUI socket client** — `ipc.rs` (265 lines, pure Rust) replaces `xpc.rs`
   (710 lines, unsafe ObjC FFI). Same public API, no `block2`, no `extern "C"`.
2. **GUI socket listener** — `xpc.zig` gains a Unix socket listener alongside
   XPC. Fire-and-forget messages use the XPC-dict adapter (zero handler
   duplication). Sync queries reply directly with protobuf.
3. **End-to-end integration** — `TERMSURF_SOCKET` env var bridges the debug path
   mismatch. Full round-trip verified at runtime.
4. **Dead code cleanup** — Delete the old `xpc.rs`.

The TUI is now 100% pure Rust with zero platform-specific FFI. The GUI accepts
both socket (TUI) and XPC (Chromium) connections on the same serial queue. The
xpc-gateway daemon is no longer needed for TUI connections.
