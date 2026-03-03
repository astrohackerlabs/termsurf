# Issue 702: Socket IPC Cleanup

## Goal

Remove all dead XPC code from both the GUI and Chromium, and remove the
fixed-size client connection limit in the GUI. Issues 698–701 replaced all IPC
with Unix sockets + protobuf. This issue cleans up the remnants.

## Background

- [Issue 698](698-unix-sockets.md) — Proved Unix socket + protobuf round-trips
  across Zig, Rust, and C++.
- [Issue 699](699-protobuf-build.md) — Built protobuf-c into the GUI.
- [Issue 700](700-tui-gui-sockets.md) — Replaced TUI↔GUI XPC with sockets.
- [Issue 701](701-chromium-sockets.md) — Replaced GUI↔Chromium XPC with sockets.

After Issue 701, no XPC messages flow at runtime. All IPC uses Unix domain
sockets with length-prefixed protobuf. But the XPC code is still in the
codebase, and the GUI's socket listener uses a fixed 16-slot connection pool.

## Part 1: Dead XPC Code Removal

### Chromium (`chromium/src/content/chromium_profile_server/`)

- `shell_browser_main_parts.cc`:
  - `StartDynamicMode()` — XPC gateway handshake. Dead.
  - `control_connection_` and `app_endpoint_` — XPC connection/endpoint storage.
    Dead.
  - Per-tab XPC connection creation in `CreateTab()` and `CreateDevToolsTab()` —
    the `else` branches that call `xpc_connection_create_from_endpoint`. Dead.
  - XPC message handler for the control connection. Dead.
  - `HandleQueryTabs()` XPC reply path. Dead.
- `shell_browser_main_parts.h`:
  - `xpc_connection_t control_connection_`, `xpc_object_t app_endpoint_`
    declarations. Dead.
  - `TabState::tab_connection` (per-tab XPC connection). Dead.
  - XPC handler method declarations. Dead.
- `shell_tab_observer.cc`:
  - XPC fallback branches in `OnCursorChanged`, `DidFinishNavigation`,
    `SendLoadingState`, `TitleWasSet` — the `else if (xpc_connection_)` paths.
    Dead.
- `shell_tab_observer.h`:
  - `xpc_connection_t xpc_connection_` member. Dead.
  - `SetConnection(xpc_connection_t)` method. Dead.
- `shell_switches.h`:
  - `kXpcService` switch. Dead.

### GUI (`gui/src/apprt/xpc.zig`)

- XPC gateway connection and anonymous listener — the `register_app(endpoint)`
  handshake. Dead.
- `server.peer` field and all `xpc_connection_send_message(server.peer, ...)`
  calls — the `else` branches in every send function. Dead.
- XPC fallback branches in all 10 GUI→Chromium send functions (`sendCreateTab`,
  `sendCreateDevToolsTab`, `sendResize`, `sendFocusMessage`, `sendMouseEvent`,
  `sendScrollEvent`, `sendMouseMove`, `sendKeyEvent`, `handleNavigate`,
  `handleSetColorScheme`). Dead.
- XPC fallback in close-tab sends in `handleDisconnect` and
  `handleClientDisconnect`. Dead.
- `peer_to_profile` and `peer_to_pane` maps (keyed by XPC peer address). Dead.
- `Server.peer` field. Dead.
- `Pane.web_peer` field. Dead.
- `handleServerRegister` XPC path (the non-socket branch). Dead.
- `TERMSURF_XPC_SERVICE` env var and launchd plist references. Dead.

### XPC Gateway Daemon

The entire gateway daemon can be deleted once all XPC code is removed. It was
the intermediary that brokered XPC connections between GUI and Chromium.

## Part 2: Unlimited Client Connections

The GUI's socket listener uses a fixed-size array:

```zig
const MAX_CLIENTS = 16;
var clients: [MAX_CLIENTS]ClientConn = [_]ClientConn{.{}} ** MAX_CLIENTS;
```

Each `ClientConn` has a 65KB read buffer, so 16 slots = 1MB pre-allocated. This
caps the number of simultaneous TUI + Chromium connections at 16.

Replace with heap-allocated `ClientConn`s (same pattern as `Pane` and `Server`)
so there is no fixed limit. Each connection is allocated on accept and freed on
disconnect.

## Experiments

### Experiment 1: Remove dead XPC code from GUI

**Result: Success.** Commit `6fba7c7`.

Removed all dead XPC code from `gui/src/apprt/xpc.zig`. Net change: -1054 lines,
+255 lines (rewritten comments and simplified control flow).

#### Removed

- **Extern declarations (15):** `xpc_connection_create_mach_service`,
  `xpc_connection_set_event_handler`, `xpc_connection_resume`,
  `xpc_connection_cancel`, `xpc_connection_send_message`,
  `xpc_connection_send_message_with_reply_sync`, `xpc_connection_create`,
  `xpc_endpoint_create`, `xpc_dictionary_set_value`,
  `xpc_dictionary_get_remote_connection`, `xpc_dictionary_create_reply`,
  `xpc_get_type`, `xpc_retain`, `xpc_release`,
  `xpc_connection_set_target_queue`.
- **Extern consts (3):** `_xpc_type_connection`, `_xpc_type_error`,
  `_xpc_error_connection_invalid`.
- **Types (3):** `EventBlock`, `PeerContext`, `PeerBlock`.
- **Helper:** `xpcPtr` function.
- **Import:** `objc` (only used for block types).
- **Struct fields:** `Server.peer`, `Pane.web_peer`.
- **Variables:** `gateway`, `listener`, `peer_to_pane`, `peer_to_profile`.
- **Functions (10):** `gatewayHandler`, `listenerHandler`, `peerHandler`,
  `handleServerRegister`, `handleHello`, `handleQueryLast`,
  `handleQueryDevtools`, `handleQueryTabs`, `handleDisconnect`.
- **Dispatch entries (5):** `"server_register"`, `"hello"`, `"query_last"`,
  `"query_devtools"`, `"query_tabs"` in `handleMessage()`.
- **XPC fallback branches** in all 10 GUI→Chromium send functions and 4 handler
  functions (`handleLoadingState`, `handleUrlChanged`, `handleTitleChanged`,
  `sendModeToWeb`).
- **XPC close-tab fallback** in `handleClientDisconnect`.
- **XPC forward** in `handleSocketQueryTabs`.
- **Init/deinit:** Gateway/listener setup, dead map inits,
  `TERMSURF_XPC_SERVICE` env var, web_peer/peer cleanup, gateway/listener
  cancel.

#### Kept

- `xpc_dictionary_*` extern declarations — still used by socket adapter
  functions that build XPC dicts for `handleMessage()` dispatch.
- `xpc_object_t` type alias — still used throughout.
- `_xpc_type_dictionary` — still used for type checking.
- All socket infrastructure (`ClientConn`, `clients`, `socketAcceptHandler`,
  etc.).

#### Renames

- `xpc_queue` → `ipc_queue` (all occurrences + queue label string).
- `log = std.log.scoped(.xpc)` → `log = std.log.scoped(.ipc)`.
- Updated file header comment to reference IPC and Issues 698–701.

#### Simplified guards

- `(server.peer != null or server.fd >= 0)` → `(server.fd >= 0)`.
- `if (server.peer == null and server.fd < 0) return` →
  `if (server.fd < 0) return`.

#### Verified

- `zig build` compiles clean.
- Manual test: launch GUI, `web google.com`, browse, navigate, exit TUI — all
  working.
