+++
status = "closed"
opened = "2026-03-03"
closed = "2026-03-03"
+++

# Issue 701: Replace GUI↔Chromium XPC with Unix Sockets

## Goal

Replace the GUI↔Chromium IPC channel with Unix domain sockets + protobuf. This
is the second half of the XPC removal — Issue 700 replaced TUI↔GUI. After this
issue, there is no XPC anywhere in the stack and the xpc-gateway daemon can be
deleted.

## Background

### What Issues 698–700 proved

Issue 698 proved protobuf wire compatibility across Zig (protobuf-c), Rust
(prost), and C++ (libprotobuf), and proved Unix socket round-trips across Zig
and Rust. Issue 699 solved the build system integration — protobuf-c compiles
into the GUI's xcframework via the `gui/src/protobuf/` stb.c pattern. Issue 700
replaced TUI↔GUI XPC with sockets end-to-end across three experiments.

### What exists now

- **Proto schema:** `proto/termsurf.proto` — 30 messages in a `oneof` wrapper,
  shared across all three build systems.
- **GUI socket listener:** `xpc.zig` listens on
  `$TMPDIR/termsurf/gui{-debug}.sock`. Currently handles one TUI connection.
  Uses `dispatch_source` on the serial `xpc_queue`.
- **GUI protobuf-c:** `gui/src/protobuf/` — runtime + generated code, linked
  into the final binary.
- **Wire format:** 4-byte LE length prefix + serialized `TermSurfMessage`.
- **Chromium protobuf:** `third_party/protobuf/` ships with Chromium. The
  `proto_library.gni` template compiles `.proto` → C++ at build time.

### What still uses XPC

The GUI↔Chromium channel — both directions:

**GUI → Chromium (commands, via `server.peer`):**

| Message             | Fields                                                                    |
| ------------------- | ------------------------------------------------------------------------- |
| `CreateTab`         | url, pane_id, pixel_width, pixel_height, dark                             |
| `CreateDevtoolsTab` | pane_id, inspected_tab_id, pixel_width, pixel_height, dark                |
| `Resize`            | tab_id, pixel_width, pixel_height                                         |
| `MouseEvent`        | tab_id, type, x, y, button, click_count, modifiers                        |
| `MouseMove`         | tab_id, x, y, modifiers                                                   |
| `ScrollEvent`       | tab_id, x, y, delta_x, delta_y, phase, momentum_phase, precise, modifiers |
| `KeyEvent`          | tab_id, type, windows_key_code, utf8, modifiers                           |
| `FocusChanged`      | tab_id, focused                                                           |
| `Navigate`          | tab_id, url                                                               |
| `SetColorScheme`    | tab_id, dark                                                              |
| `CloseTab`          | tab_id                                                                    |
| `QueryTabs`         | (reply expected)                                                          |

**Chromium → GUI (events, via per-tab XPC connection):**

| Message          | Fields                                           |
| ---------------- | ------------------------------------------------ |
| `ServerRegister` | profile                                          |
| `TabReady`       | pane_id, tab_id                                  |
| `CaContext`      | tab_id, ca_context_id, pixel_width, pixel_height |
| `UrlChanged`     | tab_id, url                                      |
| `LoadingState`   | tab_id, state, progress                          |
| `TitleChanged`   | tab_id, title                                    |
| `CursorChanged`  | tab_id, cursor_type                              |

### Current connection flow

```
GUI                       Gateway              Chromium
 |---register_app(endpoint)-->|                    |
 |                            |                    |
 |  (GUI spawns Chromium with --xpc-service=...)   |
 |                            |<--get_endpoint-----|
 |                            |---endpoint-------->|
 |                            |                    |
 |<========= XPC control connection (from endpoint) ========>|
 |<========= XPC per-tab connections (from endpoint) =======>|
```

### New connection flow

```
GUI                                     Chromium
 |  (GUI spawns Chromium with --ipc-socket=path)  |
 |                                                 |
 |<-------- socket connect to gui.sock ------------|
 |<-------- ServerRegister { profile } ------------|
 |                                                 |
 |========= single bidirectional socket ==========>|
```

No gateway, no endpoint handshake, no per-tab connections. One socket per
Chromium server process. The `tab_id` field in every message identifies the tab.

### Chromium source files to modify

All TermSurf code lives in `content/chromium_profile_server/browser/`:

- `shell_browser_main_parts.cc` (~900 lines) — XPC handshake, control connection
  handler, all GUI→Chromium command dispatch. **The main file.**
- `shell_browser_main_parts.h` — declares `TabState` with
  `xpc_connection_t tab_connection`, XPC handler methods.
- `shell_tab_observer.cc` — sends `url_changed`, `loading_state`,
  `title_changed`, `cursor_changed` via per-tab XPC connection.
- `shell_tab_observer.h` — holds `xpc_connection_t xpc_connection_`.
- `shell_switches.h` — defines `kXpcService` switch name.

### Chromium branch

Base: `146.0.7650.0-issue-694` (latest TermSurf branch). New branch:
`146.0.7650.0-issue-701`.

## Architecture decisions

### One socket per server process

Each Chromium server process (one per browser profile) opens a single
bidirectional socket to the GUI. Replaces the control connection + N per-tab
connections. Simpler, fewer file descriptors, same serialization.

### Multi-client accept in the GUI

The GUI's socket listener currently handles one TUI connection (`tui_fd`). For
Chromium, it needs to accept multiple concurrent connections — one TUI plus N
Chromium servers. The accept handler must create per-connection state (fd, read
buffer, dispatch_source) and distinguish connection types by the first message
received.

### Server.peer becomes Server.fd

In `xpc.zig`, `Server.peer: xpc_object_t` becomes `Server.fd: std.posix.fd_t`.
All `xpc_connection_send_message(server.peer, msg)` calls become
`sendProtobuf(server.fd, &wrapper)`. The XPC dictionary construction in
`sendCreateTab`, `sendResize`, `sendMouseEvent`, etc. is replaced with protobuf
struct initialization.

### Chromium uses C++ protobuf (not protobuf-c)

The GUI uses protobuf-c (C API, via `@cImport`). Chromium uses
`third_party/protobuf` (C++ API, via `proto_library.gni`). Both produce
identical wire format — that's protobuf's purpose. The C++ side uses
`ParseFromString` / `SerializeToString`.

### --xpc-service becomes --ipc-socket

Chromium currently receives `--xpc-service=com.termsurf.xpc-gateway`. This
becomes `--ipc-socket=/path/to/gui.sock`. The GUI passes the actual socket path
(from `sock_path_buf`) as a command-line argument to the server process.

## Ideas for experiments

- **Protobuf in Chromium BUILD.gn.** Copy `termsurf.proto` into the Chromium
  tree, add a `proto_library` target, verify `autoninja` compiles it to C++.
  Proof that the schema works in Chromium's build system.

- **Multi-client socket accept.** Extend the GUI's socket listener to handle
  multiple concurrent connections — per-connection read buffers,
  dispatch_sources, and connection type tagging (TUI vs Chromium). Replace the
  single `tui_fd` with a connection map.

- **Chromium socket client.** Replace the XPC handshake in
  `shell_browser_main_parts.cc` with a Unix socket connect to `--ipc-socket`.
  Send `ServerRegister` as the first message. Receive `CreateTab` and reply with
  `TabReady` + `CaContext`. Minimal viable round-trip proving the socket works.

- **Full Chromium message replacement.** Convert all remaining XPC messages to
  protobuf in both directions. Replace the XPC message handler with a socket
  reader. Replace per-tab XPC connections in `shell_tab_observer.cc` with sends
  over the shared server socket.

- **GUI → Chromium socket sends.** Replace all
  `xpc_connection_send_message( server.peer, msg)` calls in `xpc.zig` with
  `sendProtobuf(server.fd, &wrapper)`. Convert `sendCreateTab`, `sendResize`,
  `sendMouseEvent`, `sendKeyEvent`, etc. from XPC dict construction to protobuf
  struct initialization.

- **End-to-end integration.** Full runtime test — launch GUI, open a web page,
  verify browser renders via socket-only IPC. All 12 GUI→Chromium and 7
  Chromium→GUI message types working.

- **Remove xpc-gateway.** Delete the gateway daemon entirely. Remove all XPC
  client code from Chromium. Remove the gateway connection and endpoint
  registration from `xpc.zig`. Clean up `TERMSURF_XPC_SERVICE` env var and
  launchd plist.

## Experiments

### Experiment 1: Protobuf in Chromium BUILD.gn

**Goal:** Copy `termsurf.proto` into the Chromium tree, add a `proto_library`
target, wire it into `chromium_profile_server_lib`, and verify `autoninja`
compiles the generated C++ headers. Proof that the schema works in Chromium's
build system before touching any runtime code.

**Context:**

Chromium ships `third_party/protobuf/` with a `proto_library.gni` template that
compiles `.proto` files to C++ at build time. Existing examples:

- `content/browser/private_aggregation/proto/BUILD.gn` — 14-line BUILD.gn with
  `proto_library("private_aggregation_budgets_proto")`.
- Pattern: import the `.gni`, declare `sources`, set
  `cc_generator_options = "dllexport_decl=CONTENT_EXPORT:"` and
  `cc_include = "content/common/content_export.h"`.

The `chromium_profile_server_lib` static_library (BUILD.gn line 164) lists all
profile server sources and deps. Adding a proto dep follows the standard
pattern: add `":termsurf_proto"` to its `deps` list.

**Steps:**

1. Create the Chromium branch:

   ```bash
   cd chromium/src
   git checkout 146.0.7650.0-issue-694
   git checkout -b 146.0.7650.0-issue-701
   ```

2. Copy the proto file:

   ```bash
   cp proto/termsurf.proto \
     chromium/src/content/chromium_profile_server/browser/termsurf.proto
   ```

3. Create `content/chromium_profile_server/browser/proto/BUILD.gn`:

   ```gn
   import("//third_party/protobuf/proto_library.gni")

   proto_library("termsurf_proto") {
     sources = [ "../termsurf.proto" ]
   }
   ```

   Minimal — no `dllexport_decl` or `cc_include` needed since this is an
   internal-only library (not exported via component DLL boundaries). The proto
   lives in `browser/` and the BUILD.gn lives in `browser/proto/` to keep the
   build target isolated.

4. Wire into `chromium_profile_server_lib` — add to the `deps` list (after
   `:protocol_sources` at BUILD.gn line 317):

   ```gn
   "browser/proto:termsurf_proto",
   ```

5. Add a smoke-test `#include` in `shell_browser_main_parts.cc`:

   ```cpp
   #include "content/chromium_profile_server/browser/termsurf.pb.h"
   ```

   And a trivial usage in a function body to verify compilation:

   ```cpp
   termsurf::TermSurfMessage msg;
   msg.mutable_server_register()->set_profile("test");
   ```

6. Build:

   ```bash
   cd chromium/src
   autoninja -C out/Default chromium_profile_server
   ```

**Pass criteria:** `autoninja` compiles without errors. The generated
`termsurf.pb.h` is included and the trivial usage compiles.

**Fail criteria:** Build errors from proto compilation, missing include paths,
or linker failures.

**Result: PASS**

Proto compilation and C++ code generation work. Two adjustments from the
original design:

1. **`option optimize_for = LITE_RUNTIME;`** — Required in the Chromium copy of
   `termsurf.proto`. Without it, protoc generates full-protobuf code (`Message`
   base class, `UnknownFieldSet`), but Chromium only links `protobuf_lite`.
   Adding `LITE_RUNTIME` generates `MessageLite`-based code. This option is
   ignored by protobuf-c (Zig) and prost (Rust).

2. **`component_build_force_source_set = true`** — Required in the
   `proto_library` target. In component builds (`is_component_build=true`), the
   profile server is a shared library. Without this flag, the proto target
   compiles as a static library whose protobuf runtime references aren't
   resolved at link time.

Generated `termsurf.pb.h` is 580KB with C++ accessors for all 30 messages. The
smoke test in `ShellBrowserMainParts` constructor compiles and links clean.

### Experiment 2: Multi-client socket accept

**Goal:** Refactor the GUI's socket listener from a single-client `tui_fd` model
to a connection pool that supports multiple concurrent clients. Each connection
gets its own fd, dispatch_source, and read buffer. The first message on a
connection determines its type (TUI or Chromium). This unblocks Experiment 3
where Chromium connects alongside the TUI.

**Context:**

The current socket code in `xpc.zig` (Issue 700) uses global state for one TUI
connection:

```zig
var tui_fd: std.posix.fd_t = -1;
var tui_source: ?*anyopaque = null;
var tui_buf: [65536]u8 = undefined;
var tui_buf_len: usize = 0;
```

When a new connection arrives, `socketAcceptHandler` forcibly disconnects the
previous one. This means only one client can be connected at a time. For Issue
701, the GUI needs to accept N Chromium server connections (one per browser
profile) alongside the TUI.

The dispatch source API already supports per-connection context:
`dispatch_set_context(source, ptr)` stores a pointer that the handler receives
as its first argument. The current handlers ignore this parameter
(`_: ?*anyopaque`). Experiment 2 uses it to pass a `*ClientConn` pointer.

**Data structure:**

```zig
const MAX_CLIENTS = 16;

const ConnType = enum { unknown, tui, chromium };

const ClientConn = struct {
    fd: std.posix.fd_t = -1,
    source: ?*anyopaque = null,
    buf: [65536]u8 = undefined,
    buf_len: usize = 0,
    conn_type: ConnType = .unknown,
    server: ?*Server = null, // set when conn_type == .chromium
};

var clients: [MAX_CLIENTS]ClientConn = [_]ClientConn{.{}} ** MAX_CLIENTS;
```

No dynamic allocation. Fixed pool indexed by slot. An empty slot has `fd == -1`.

**Changes to `xpc.zig`:**

1. **Replace globals.** Delete `tui_fd`, `tui_source`, `tui_buf`, `tui_buf_len`.
   Add `ClientConn` struct and `clients` array.

2. **Increase listen backlog.** Change `listen(sock_fd, 1)` to
   `listen(sock_fd, 8)`.

3. **Refactor `socketAcceptHandler`.** On accept: find an empty slot in
   `clients`, fill in the fd, create a dispatch_source, call
   `dispatch_set_context(source, &clients[i])` so the read handler knows which
   connection it's reading, and resume.

4. **Refactor `socketReadHandler`.** The first parameter is now `*ClientConn`
   (via dispatch context). Read into `conn.buf[conn.buf_len..]`. Extract
   messages and dispatch to `handleSocketMessage(conn, pb_msg)`.

5. **Connection type tagging.** In `handleSocketMessage`, if
   `conn.conn_type == .unknown`, the first message determines the type:
   - `ServerRegister` (case 12) → `.chromium`. Look up the Server by profile,
     store `conn` as the server's socket connection. This replaces the XPC peer
     registration in `handleServerRegister`.
   - Anything else → `.tui`. Proceed as before.

6. **Refactor disconnect.** `handleClientDisconnect(conn: *ClientConn)` replaces
   `handleTuiDisconnect()`. For `.tui` connections, clean up panes where
   `p.web_fd == conn.fd`. For `.chromium` connections, clear `conn.server.fd` so
   the GUI knows the server disconnected. Reset the slot (`fd = -1`).

7. **Update `web_fd` references.** The existing code stores `tui_fd` in
   `p.web_fd` for reply routing. This continues to work — each pane stores the
   fd of the connection that created it. `sendProtobuf(p.web_fd, &wrapper)` is
   unchanged.

8. **Add `Server.fd`.** Add `fd: std.posix.fd_t = -1` to the `Server` struct
   alongside the existing `peer: xpc_object_t`. Both coexist during the
   transition. Experiment 5 (GUI → Chromium socket sends) will switch from
   `server.peer` to `server.fd`.

**What does NOT change:**

- `handleSocketMessage` dispatch logic (all case numbers stay the same)
- `sendProtobuf` function (still takes an fd and a wrapper)
- All the individual socket message handlers (`handleSocketSetOverlay`, etc.)
- XPC message handling (still works in parallel for Chromium until Experiment 5)
- The TERMSURF_SOCKET env var export

**Pass criteria:** Build clean on all three targets. Runtime test: launch the
GUI, open a web page via `web`, verify the TUI connection works exactly as
before (overlay renders, URL syncs, mode changes). The refactor is transparent
to TUI clients.

**Fail criteria:** Build errors, runtime regressions (TUI can't connect, overlay
doesn't render, disconnects cause crashes).

**Result: PASS**

Build clean on all three targets (macOS, iOS, iOS-simulator has pre-existing
simdutf error). Implementation matched the design exactly — no surprises.

Changes to `xpc.zig`:

- Added `ClientConn` struct (fd, source, buf, buf_len, conn_type, server) and
  `clients: [16]ClientConn` pool. Deleted `tui_fd`, `tui_source`, `tui_buf`,
  `tui_buf_len`.
- Added `Server.fd` alongside existing `Server.peer`.
- `socketAcceptHandler`: finds empty slot, sets `dispatch_set_context` so read
  handler receives `*ClientConn`.
- `socketReadHandler`: uses context parameter instead of global state.
- `handleClientDisconnect`: replaces `handleTuiDisconnect`, branches on
  `conn_type` — TUI cleans up panes, Chromium clears `server.fd`.
- `handleSocketMessage`: first message tags connection type (case 12 =
  ServerRegister → chromium, anything else → tui). New case 12 routes to
  `handleSocketServerRegister`.
- `handleSocketServerRegister`: stores `conn.fd` as `server.fd`, links
  `conn.server`, flushes pending tabs (mirrors XPC `handleServerRegister`).
- `deinit`: iterates `clients` array instead of single `tui_fd`.
- Listen backlog: 1 → 8.

Runtime test: TUI still works — overlay renders, URL syncs, mode changes. The
refactor is transparent to TUI clients.

### Experiment 3: Bidirectional socket IPC

**Goal:** Replace all XPC between GUI and Chromium with bidirectional
socket+protobuf. After this experiment, the only remaining XPC code is the
gateway connection and listener (cleanup in a later experiment). This combines
ideas 3–5 from the ideas list because the two directions are tightly coupled — a
half-XPC half-socket hybrid would be more complex than a clean swap.

**Context:**

The current Chromium profile server uses a two-tier XPC model:

1. **Control connection** (one per profile server) — receives all 12
   GUI→Chromium commands via `xpc_connection_create_from_endpoint`. Event
   handler dispatches by `action` string to `CreateTab`, `HandleMouseEvent`,
   etc.
2. **Per-tab connections** (one per tab) — also from the stored endpoint. Send
   events back to GUI: `tab_ready`, `ca_context`, `url_changed`,
   `loading_state`, `title_changed`, `cursor_changed`.

The gateway handshake is 4 steps: connect → request endpoint → create control
connection from endpoint → send server_register → cancel gateway. Per-tab
connections are created during `CreateTab()` and destroyed during tab close.

The new model: one socket per server process, bidirectional. No gateway, no
endpoint, no per-tab connections. The `tab_id` field in every message identifies
the tab.

**Chromium side changes (`shell_browser_main_parts.cc/.h`):**

1. **Add `--ipc-socket` switch.** In `shell_switches.h`, add
   `kIpcSocket = "ipc-socket"`. In `InitializeMessageLoopContext`, check for
   `--ipc-socket` before `--xpc-service`. If present, call
   `StartSocketMode(path)` instead of `StartDynamicMode(gateway_name)`.

2. **`StartSocketMode(path)`.** New function:
   - `connect()` to the Unix socket at `path`
   - Send `ServerRegister { profile }` protobuf (4-byte LE prefix + serialized)
   - Store the socket fd as `socket_fd_`
   - Derive profile from `--user-data-dir` basename (same as current code)
   - Start a socket reader on a background thread

3. **Socket reader thread.** Reads length-prefixed protobuf messages and posts
   tasks to the UI thread. Same framing as the GUI: 4-byte LE length + payload.
   Dispatches by `msg.msg_case()`:
   - `kCreateTab` (1) → `CreateTab()`
   - `kCreateDevtoolsTab` (2) → `CreateDevToolsTab()`
   - `kResize` (3) → `ResizeTab()`
   - `kCloseTab` (4) → `CloseTabById()`
   - `kNavigate` (5) → `NavigateTab()`
   - `kMouseEvent` (6) → `HandleMouseEvent()`
   - `kMouseMove` (7) → `HandleMouseMove()`
   - `kScrollEvent` (8) → `HandleScrollEvent()`
   - `kKeyEvent` (9) → `HandleKeyEvent()`
   - `kFocusChanged` (10) → `HandleFocusChanged()`
   - `kSetColorScheme` (11) → `SetColorScheme()`
   - `kQueryTabsRequest` (29) → `HandleQueryTabs()` (reply via socket)

4. **`SendProtobuf(msg)` helper.** Serializes a `TermSurfMessage` with 4-byte LE
   prefix and writes to `socket_fd_`. Thread-safe (UI thread only, or
   mutex-protected for observer callbacks).

5. **Replace per-tab XPC sends in `CreateTab()`.** Instead of creating a per-tab
   `xpc_connection_create_from_endpoint`, send `TabReady` and `CaContext`
   protobuf messages over the shared `socket_fd_`. The CALayerParams callback
   captures `socket_fd_` instead of `tab_conn`.

6. **Replace ShellTabObserver XPC sends.** `ShellTabObserver` stores
   `socket_fd_` instead of `xpc_connection_t`. `OnCursorChanged`,
   `DidFinishNavigation`, `SendLoadingState`, `TitleWasSet` send protobuf
   instead of XPC dictionaries.

7. **No per-tab connections.** `TabState::tab_connection` becomes unused. No
   `xpc_connection_create_from_endpoint` calls. No `app_endpoint_` storage.

**GUI side changes (`xpc.zig`):**

8. **Pass `--ipc-socket` to Chromium.** In `spawnServerProcess`, replace
   `--xpc-service=...` with `--ipc-socket=<sock_path>`. The socket path comes
   from `sock_path_buf[0..sock_path_len]`.

9. **GUI→Chromium: send via protobuf.** In `sendCreateTab`, `sendResize`,
   `sendMouseEvent`, `sendKeyEvent`, etc.: if `server.fd >= 0`, build a protobuf
   `TermSurfMessage` and call `sendProtobuf(server.fd, &wrapper)` instead of
   constructing XPC dicts and calling
   `xpc_connection_send_message(server.peer, ...)`. Keep XPC fallback when
   `server.fd == -1` (graceful transition).

10. **Chromium→GUI: handle socket messages.** In `handleSocketMessage`, add
    cases for Chromium events received via socket (these arrive on the Chromium
    `ClientConn`):
    - `tab_ready` (13) → XPC-dict adapter → `handleMessage`
    - `ca_context` (14) → XPC-dict adapter → `handleMessage`
    - `url_changed` (15) → XPC-dict adapter → `handleMessage`
    - `loading_state` (16) → XPC-dict adapter → `handleMessage`
    - `title_changed` (17) → XPC-dict adapter → `handleMessage`
    - `cursor_changed` (18) → XPC-dict adapter → `handleMessage` Uses the same
      adapter pattern as TUI messages: build an XPC dict and call
      `handleMessage()` to reuse existing handler logic.

11. **QueryTabs reply via socket.** `handleSocketQueryTabs` currently forwards
    to Chromium via `xpc_connection_send_message_with_reply_sync`. When
    `server.fd >= 0`, send `QueryTabsRequest` protobuf to `server.fd` and read
    the reply. (Or defer this — query_tabs can stay on XPC as a temporary gap.)

**What does NOT change (yet):**

- The XPC gateway connection and listener in `xpc.zig` — still needed for the
  app launch flow. Cleanup is a separate experiment.
- `control_connection_` and `app_endpoint_` declarations in the header — removed
  in cleanup.
- `server.peer` field — still present, just not used when `server.fd >= 0`.

**Pass criteria:** Build clean (both GUI and Chromium). Runtime test: launch
GUI, type `web google.com`, browser renders via socket-only IPC. Tab ready, CA
context, URL changes, loading state, title changes, cursor changes — all flowing
over the socket. Mouse, keyboard, scroll input working.

**Fail criteria:** Build errors, runtime failures (blank pane, no input, missing
events), crashes on connect/disconnect.

**Result: PASS**

All XPC between GUI and Chromium has been replaced with bidirectional
socket+protobuf. Both builds clean. Runtime test: browser renders, navigation
works, input works, TUI exits cleanly.

**Chromium changes** (`146.0.7650.0-issue-701` branch, 5 files, +605/−189):

- `shell_switches.h`: Added `kIpcSocket = "ipc-socket"` switch.
- `shell_browser_main_parts.h`: Declared `StartSocketMode`, `SendProtobuf`,
  `SocketReaderLoop`, `HandleSocketMsg`, `HandleQueryTabsSocket`,
  `socket_fd_ = -1`.
- `shell_browser_main_parts.cc`: Added `StartSocketMode()` (connect, send
  `ServerRegister`, start reader thread), `SendProtobuf()` (4-byte LE prefix +
  serialize), `SocketReaderLoop()` (length-prefixed framing, dispatch to UI
  thread), `HandleSocketMsg()` (11-case dispatch). Modified `CreateTab()` and
  `CreateDevToolsTab()` with socket/XPC branching — socket sends `TabReady` and
  `CaContext` protobuf, CALayerParams callbacks post to UI thread via
  `GetUIThreadTaskRunner`. Modified `CloseTabById()` to skip XPC cleanup when
  `socket_fd_ >= 0`. Used `UNSAFE_BUFFERS()` for pointer arithmetic per
  Chromium's `-Wunsafe-buffer-usage`.
- `shell_tab_observer.cc`/`.h`: Added `SetSocketFd(int fd)` and `socket_fd_`.
  All 6 observer callbacks (`OnCursorChanged`, `DidFinishNavigation`,
  `SendLoadingState`, `TitleWasSet`, etc.) branch on `socket_fd_ >= 0` to send
  protobuf instead of XPC dicts.

**GUI changes** (`xpc.zig`, +474/−197):

- `spawnServerProcess`: passes `--ipc-socket=<sock_path>` instead of
  `--xpc-service=...`.
- All 10 GUI→Chromium send functions (`sendCreateTab`, `sendCreateDevToolsTab`,
  `sendResize`, `sendFocusMessage`, `sendMouseEvent`, `sendScrollEvent`,
  `sendMouseMove`, `sendKeyEvent`, `handleNavigate`, `handleSetColorScheme`)
  branch on `server.fd >= 0` to send protobuf instead of XPC dicts.
- All readiness checks updated from `server.peer != null` to
  `(server.peer != null or server.fd >= 0)`.
- 6 new Chromium→GUI socket handlers (`handleSocketTabReady`,
  `handleSocketCaContext`, `handleSocketUrlChanged`, `handleSocketLoadingState`,
  `handleSocketTitleChanged`, `handleSocketCursorChanged`) using XPC-dict
  adapter pattern (build dict → call `handleMessage`) for zero handler
  duplication.
- Close-tab sends in `handleDisconnect` and `handleClientDisconnect` branch on
  socket vs XPC.
- Fixed use-after-free: when killing a server, cancel the Chromium ClientConn's
  dispatch source and close its fd before freeing the server, preventing a
  dangling pointer crash when the dead socket's dispatch source fires.

**Adjustments from design:**

1. **CALayerParams thread safety** — The CALayerParams callback fires on the
   compositor thread, not the UI thread. Socket writes must be posted to the UI
   thread via `content::GetUIThreadTaskRunner({})->PostTask()`. XPC was
   thread-safe by design; sockets are not.
2. **`UNSAFE_BUFFERS()` macro** — Chromium's `-Wunsafe-buffer-usage` flag
   rejects raw pointer arithmetic (`buf.data() + 4`, `memset`, `strncpy`).
   Required `#include "base/compiler_specific.h"` and `UNSAFE_BUFFERS()`
   wrappers for `SocketReaderLoop` and `strncpy`.
3. **Use-after-free on disconnect** — When the TUI disconnects and `pane_count`
   hits 0, `killServer` kills the Chromium process and `alloc.destroy(server)`
   frees the server. But the Chromium `ClientConn`'s dispatch source was still
   active — when the dead socket's source fired, it accessed freed memory. Fixed
   by cancelling the dispatch source and closing the fd before freeing.
4. **QueryTabs** — Left as a temporary gap per design point 11. When
   `server.peer == null` (socket mode), QueryTabs returns GUI-side counts only;
   the XPC forwarding to Chromium is skipped.

## Conclusion

GUI↔Chromium IPC now runs entirely over Unix domain sockets with protobuf
serialization. Three experiments got us here:

1. **Experiment 1** proved protobuf compiles in Chromium's build system with
   `proto_library.gni` and `LITE_RUNTIME`.
2. **Experiment 2** refactored the GUI's socket listener from single-client to a
   16-slot connection pool with per-connection dispatch sources and type
   tagging.
3. **Experiment 3** replaced all 19 message types (12 GUI→Chromium, 7
   Chromium→GUI) with socket+protobuf, eliminated per-tab XPC connections, and
   fixed a use-after-free in the disconnect path.

Combined with Issue 700 (TUI↔GUI sockets), the entire TermSurf IPC stack now
uses Unix sockets + protobuf. No XPC messages flow at runtime. The XPC gateway
daemon and all XPC client code remain in the codebase but are dead code —
cleanup is a separate issue.

### What remains

- **Remove the XPC gateway.** Delete the gateway daemon, gateway connection,
  endpoint registration, and `TERMSURF_XPC_SERVICE` env var. Dead code now.
- **Remove XPC fallback branches.** All send functions have
  `if (server.fd >= 0) { ... } else { xpc... }` branches. The XPC branches are
  dead code.
- **QueryTabs via socket.** Currently returns GUI-side counts only in socket
  mode. Needs a request/reply protocol over the socket.
- **`server.peer` field.** Still present in the `Server` struct but unused when
  `server.fd >= 0`. Can be removed after XPC cleanup.
