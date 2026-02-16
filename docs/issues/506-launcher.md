# Issue 506: Compositor Launcher Daemon

## Background

Issue 505 proved GPU overlay compositing works: a pink quad renders at exact
grid coordinates inside a Ghostty pane, driven by XPC messages from the `web`
TUI. But the main app must be launched via `launchctl kickstart` instead of
`open` because the app IS the XPC Mach service. A process can only claim a Mach
service if its launchd identity matches the plist's job — launching via `open`
gives the app a different identity.

In ts3, a separate launcher daemon (`termsurf-launcher`) owned the Mach service.
The main app (WezTerm) launched normally via `open` and connected to the
launcher as a client. This issue restores that pattern for ts5.

## Goal

Launch TermSurf with `open ts5/zig-out/TermSurf.app` and have the XPC overlay
pipeline work exactly as it does today.

## Architecture

### Current (Issue 505)

```
web ──XPC──▶ TermSurf app (com.termsurf.compositor)
              ├── XPC listener (Mach service)
              └── Renderer (set_overlay)
```

The app is both the Mach service and the renderer. Must be launched via
`launchctl kickstart`.

### Proposed

```
                     ┌──────────────────────┐
                     │  Compositor Daemon   │
                     │  (com.termsurf.      │
                     │   compositor)        │
                     │                      │
  TermSurf app ─────▶│  Stores app endpoint │◀───── web
  (launched via open)│                      │  (connects, claims
  sends anonymous    │  Returns endpoint    │   endpoint, then
  listener endpoint  │  to web processes    │   connects directly
                     └──────────────────────┘   to app)

  web ──direct XPC──▶ TermSurf app (anonymous listener)
                       └── set_overlay, clear_overlay
```

Three processes:

1. **Compositor daemon** — Tiny binary. Owns the `com.termsurf.compositor` Mach
   service. Managed by launchd. Its only job is rendezvous: the app registers
   its anonymous endpoint, and `web` processes claim it.

2. **TermSurf app** — Launched normally via `open`. On startup, connects to
   `com.termsurf.compositor` as a client and sends an anonymous XPC listener
   endpoint. Handles `set_overlay` messages from `web` processes on the direct
   connection.

3. **`web` TUI** — Connects to `com.termsurf.compositor`, requests the app's
   endpoint, then connects directly to the app. All overlay messages flow on the
   direct connection — no relay hop through the daemon.

### Why Direct Connection (Not Relay)

The daemon could relay every message from `web` to the app, but direct
connection is better:

- **No per-message relay hop.** Overlay coordinates are sent every 250ms today.
  IOSurface Mach ports will be sent at 60fps in the future. A relay hop adds
  latency and CPU overhead for every frame.
- **Proven pattern.** ts3 used exactly this approach — the launcher relayed
  endpoints, then profile servers connected directly to the GUI for IOSurface
  Mach port transfer.
- **Simpler daemon.** The daemon handles two message types (`register_app`,
  `connect`) and no ongoing traffic. It could crash and restart without
  interrupting active `web` sessions (they already have direct connections).

### Why Not Eliminate the Daemon Entirely

The daemon exists solely because of a macOS constraint: a Mach service can only
be claimed by the process launchd launched for that job. Without a daemon, the
app must be launched by launchd. With a daemon, the app launches normally and
the daemon provides the well-known rendezvous point.

There is no alternative IPC mechanism that avoids this. XPC is the only way to
transfer IOSurface Mach ports between processes on macOS. See CLAUDE.md "Settled
Architectural Decisions".

## XPC Protocol

### Daemon Messages

The daemon handles two actions:

**`register_app`** — Sent by the TermSurf app on startup.

```
→ { action: "register_app", endpoint: <anonymous_listener_endpoint> }
```

The daemon stores the endpoint. If a previous endpoint exists (app restarted),
it replaces it.

**`connect`** — Sent by `web` processes.

```
→ { action: "connect", pane_id: "<uuid>" }
← { endpoint: <app_anonymous_listener_endpoint> }
```

The daemon returns the app's endpoint. The `web` process uses it to establish a
direct connection to the app.

If the app hasn't registered yet (daemon started before app), the daemon can
either return an error or hold the request until the app registers. Returning an
error is simpler — `web` can retry.

### Direct Connection Messages

Once `web` has a direct connection to the app, it sends the same messages as
today:

```
→ { action: "set_overlay", pane_id: "<uuid>",
    col: N, row: N, width: N, height: N }
```

On disconnect, the app clears the overlay for that pane (same as today).

## Startup Sequence

```
1. User runs:     open ts5/zig-out/TermSurf.app

2. App starts:    applicationDidFinishLaunching()
                  ├── connect_mach_service("com.termsurf.compositor")
                  │   └── launchd auto-starts daemon if not running
                  ├── create anonymous XPC listener
                  ├── send { action: "register_app", endpoint: <listener> }
                  └── set event handler on anonymous listener
                      (handles web connections)

3. User types:    cargo run -p web -- https://example.com

4. web starts:    read TERMSURF_PANE_ID from env
                  ├── connect_mach_service("com.termsurf.compositor")
                  ├── send { action: "connect", pane_id: "<uuid>" }
                  ├── receive reply with app endpoint
                  ├── connect to app via endpoint
                  └── send set_overlay on direct connection each frame

5. web exits:     direct connection closes
                  └── app detects disconnect, clears overlay
```

## Components

### Compositor Daemon

A standalone binary, ~50–100 lines. Written in Swift or Rust — either works
since the XPC C API is the same. Swift may be simpler because
`xpc_connection_create_mach_service` and `xpc_connection_set_event_handler` are
more ergonomic with closures.

**Responsibilities:**

- Listen on `com.termsurf.compositor` (LISTENER flag)
- Accept connections from the app (stores endpoint)
- Accept connections from `web` processes (returns endpoint)
- No ongoing traffic once connections are established

**Lifecycle:**

- Launched on-demand by launchd when first client connects
- Stays running while any client is connected
- Can exit when all clients disconnect (optional — launchd restarts on next
  connection anyway)

### launchd Plist

Same as today but points to the daemon binary instead of the app:

```xml
<key>ProgramArguments</key>
<array>
    <string>/path/to/termsurf-compositor</string>
</array>
```

### TermSurf App Changes

Replace `CompositorXPC.swift`'s listener with a client connection:

**Before (Issue 505):**

```swift
// App IS the listener
let conn = xpc_connection_create_mach_service(
    "com.termsurf.compositor", queue,
    UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER))
```

**After:**

```swift
// App connects as client, sends anonymous listener
let daemon = xpc_connection_create_mach_service(
    "com.termsurf.compositor", queue, 0)  // no LISTENER flag

let listener = xpc_connection_create(nil, queue)  // anonymous
// ... set up handler for web connections on listener ...

let msg = xpc_dictionary_create(nil, nil, 0)
xpc_dictionary_set_string(msg, "action", "register_app")
xpc_dictionary_set_value(msg, "endpoint",
    xpc_endpoint_create(listener))
xpc_connection_send_message(daemon, msg)
```

The handler on the anonymous listener processes `set_overlay` messages exactly
as `CompositorXPC.swift` does today.

### `web` TUI Changes

Replace the direct Mach service connection with a two-step connect:

1. Connect to `com.termsurf.compositor` (the daemon)
2. Send `{ action: "connect", pane_id: "<uuid>" }`
3. Receive reply with endpoint
4. Connect to app via endpoint
5. Send `set_overlay` on the direct connection

This requires `xpc_connection_send_message_with_reply` (or the sync variant) to
get the endpoint back from the daemon.

## Verification

1. `open ts5/zig-out/TermSurf.app` launches the app normally.
2. In a TermSurf pane: `cargo run -p web -- https://example.com` shows the pink
   overlay.
3. Resizing works. Quitting `web` clears the overlay.
4. Killing and relaunching the app works (daemon stays running, `web` reconnects
   on next launch).
5. The daemon is invisible to the user — no manual `launchctl` commands needed
   after initial plist registration.

## Experiments

### Experiment 1: Compositor Daemon with Endpoint Relay

Implement the three-process architecture: a standalone Swift daemon owns the
Mach service, the app registers an anonymous endpoint, and `web` processes claim
the endpoint to connect directly to the app.

#### Changes

##### Part 1: Compositor Daemon Binary

###### `ts5/compositor-daemon/Package.swift`

Minimal Swift Package Manager executable:

```swift
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "CompositorDaemon",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "compositor-daemon",
                          path: "Sources")
    ]
)
```

###### `ts5/compositor-daemon/Sources/main.swift`

~60 lines. The daemon:

1. Creates a Mach service listener on `com.termsurf.compositor`.
2. For each incoming peer connection, sets a message handler.
3. On `register_app`: stores the endpoint from the app (replacing any previous
   one).
4. On `connect`: sends a reply containing the stored endpoint. If no app has
   registered yet, sends an error string in the reply.
5. Enters `dispatchMain()` (never returns — launchd manages lifecycle).

Key details:

- Peers and the listener must be stored in global variables to prevent ARC
  release.
- Uses `xpc_dictionary_get_value(msg, "endpoint")` to extract the endpoint
  object and `xpc_dictionary_set_value(reply, "endpoint", endpoint)` to return
  it. Endpoints are opaque XPC objects — they pass through without
  interpretation.
- The `connect` action uses `xpc_dictionary_create_reply(msg)` to create a reply
  dictionary, which is how XPC request/reply works. The client must use
  `xpc_connection_send_message_with_reply` (or `_sync`) to receive it.

##### Part 2: Update launchd Plist

###### `ts5/macos/com.termsurf.compositor.plist`

Change `ProgramArguments` to point to the daemon binary:

```xml
<key>ProgramArguments</key>
<array>
    <string>/Users/ryan/dev/termsurf/ts5/compositor-daemon/.build/debug/compositor-daemon</string>
</array>
```

##### Part 3: App Connects as Client

###### `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

Replace the Mach service listener with:

1. **Client connection to daemon.** Connect to `com.termsurf.compositor` without
   the LISTENER flag. Resume the connection.

2. **Anonymous XPC listener.** Create with `xpc_connection_create(nil, queue)`.
   Set its event handler to accept peer connections from `web` processes. Each
   peer's message handler calls `handleMessage` (same as today). On disconnect,
   calls `handleDisconnect` (same as today). Resume the listener.

3. **Register endpoint.** Create an endpoint from the anonymous listener with
   `xpc_endpoint_create(listener)`. Send `{ action: "register_app", endpoint }`
   to the daemon.

The `handleMessage` and `handleDisconnect` methods stay unchanged — they already
handle `set_overlay`, surface lookup, and cleanup.

Key details:

- The anonymous listener must be stored in a strong property (not just a local
  variable) to prevent ARC release.
- `xpc_endpoint_create` takes a listener connection and returns a serializable
  endpoint object that can be sent over XPC to other processes.
- The daemon connection only carries the `register_app` message at startup. All
  subsequent traffic flows on the anonymous listener's direct connections.

##### Part 4: `web` Two-Step Connect

###### `web/src/xpc.rs`

Replace `CompositorConnection::connect()` with a two-step flow:

1. Connect to `com.termsurf.compositor` (the daemon).
2. Send `{ action: "connect", pane_id: "<uuid>" }` using
   `xpc_connection_send_message_with_reply_sync` to get a reply.
3. Extract the endpoint from the reply with
   `xpc_dictionary_get_value(reply,
   "endpoint")`.
4. Create a new connection from the endpoint with
   `xpc_connection_create_from_endpoint(endpoint)`.
5. Resume the endpoint connection.
6. Return this connection as the `CompositorConnection`.

The `send_set_overlay` method stays unchanged — it sends on whatever connection
it holds, which is now the direct connection to the app.

New FFI bindings needed:

```rust
extern "C" {
    fn xpc_connection_send_message_with_reply_sync(
        conn: XpcConnectionT, message: XpcObjectT,
    ) -> XpcObjectT;
    fn xpc_dictionary_create_reply(request: XpcObjectT) -> XpcObjectT;
    fn xpc_dictionary_get_value(dict: XpcObjectT, key: *const c_char) -> XpcObjectT;
    fn xpc_dictionary_set_value(dict: XpcObjectT, key: *const c_char, value: XpcObjectT);
    fn xpc_endpoint_create(conn: XpcConnectionT) -> XpcObjectT;
    fn xpc_connection_create_from_endpoint(endpoint: XpcObjectT) -> XpcConnectionT;
    fn xpc_retain(object: XpcObjectT) -> XpcObjectT;
}
```

The daemon connection can be dropped after the endpoint is received — it's not
needed for ongoing communication.

#### Build & Test

```bash
# Build the daemon
cd ts5/compositor-daemon && swift build

# Build ts5
cd ts5 && zig build

# Build web TUI
cargo build -p web

# Clear old launchd registration and re-register with daemon binary
launchctl bootout gui/$(id -u)/com.termsurf.compositor
launchctl bootstrap gui/$(id -u) ts5/macos/com.termsurf.compositor.plist

# Launch the app normally
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- https://example.com
```

#### Pass Criteria

1. `open ts5/zig-out/TermSurf.app` launches the app and the pink overlay works.
2. No `launchctl kickstart` needed — the daemon auto-starts when the app
   connects.
3. The pink overlay appears, resizes correctly, and clears on `web` exit.
4. Relaunching the app (quit and `open` again) works — the daemon stays running
   and the app re-registers its endpoint.
5. The daemon process is visible in Activity Monitor as `compositor-daemon` but
   requires no user interaction.
