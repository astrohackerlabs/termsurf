+++
status = "closed"
opened = "2026-02-15"
closed = "2026-02-15"
+++

# Issue 506: XPC Gateway Daemon

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

### Prior Art: Launcher Patterns Across Generations

#### ts1: No XPC

ts1 used in-process WKWebView — no inter-process rendering. CLI-to-app
communication used Unix domain sockets with JSON messages. No launcher, no Mach
services, no IOSurface transfer.

#### ts3: `termsurf-launcher` (Rust, ~300 lines)

The most complex launcher. Written in Rust using the `termsurf-xpc` crate (~900
lines of safe wrappers over the XPC C API). Owned `com.termsurf.launcher` Mach
service. Handled four actions:

| Action               | Purpose                                                                                                                         |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| `spawn_profile`      | GUI requests a browser profile. Launcher stores GUI's endpoint, spawns `termsurf-profile` process (or forwards to existing one) |
| `register_profile`   | Profile server registers after starting. Launcher stores connection for reuse + crash detection                                 |
| `claim_session`      | Profile server claims GUI's endpoint. Launcher returns it in reply. Profile connects directly to GUI                            |
| `unregister_profile` | Profile server gracefully unregisters before exit                                                                               |

Beyond rendezvous, the ts3 launcher also:

- **Spawned child processes** (`Command::new(&profile_bin_path).spawn()`)
- **Tracked running profiles** in a `HashMap<String, Arc<XpcConnection>>`
- **Detected crashed profiles** via XPC error handlers, cleaned up registry
- **Implemented graceful shutdown** (exited when all GUI connections
  disconnected, tracked via `AtomicUsize` counter)
- **Routed multi-webview commands** (`create_browser`) to existing profile
  processes sharing the same CEF context

Endpoint relay pattern: GUI creates anonymous listener → sends endpoint to
launcher → launcher stores → profile claims → profile connects directly to GUI
for IOSurface Mach port transfer at 60fps.

Deployment: bundled as an XPC service inside the app at
`Contents/XPCServices/com.termsurf.launcher.xpc/`. The launchd plist was
dynamically generated at build time with absolute paths.

Key files: `ts3/termsurf-launcher/src/main.rs`,
`ts3/termsurf-xpc/src/{lib,ffi,connection,listener,dictionary,block,iosurface}.rs`.

#### ts4: No Launcher

ts4's `two-profiles-receiver` experiments had no separate launcher. The receiver
(GUI) itself claimed the Mach service directly — the same architecture ts5 had
before this issue. It works, but forces `launchctl kickstart` instead of `open`.

Implemented three times (C++, Swift, Rust) to compare ergonomics. All three
shared the same critical patterns: store peers in global arrays to prevent
ARC/Drop release, lock IOSurface access between XPC and render threads, use
`IOSurfaceCreateMachPort` / `IOSurfaceLookupFromMachPort` for cross-process
texture sharing.

Key files: `ts4/two-profiles-receiver/main.mm`,
`ts4/two-profiles-swift/Sources/Receiver/main.swift`,
`ts4/two-profiles-rust/src/main.rs`.

#### Comparison

| Aspect           | ts3 launcher                    | ts4 (none)               | ts5 xpc-gateway                      |
| ---------------- | ------------------------------- | ------------------------ | ------------------------------------ |
| Language         | Rust + termsurf-xpc             | N/A                      | Swift (raw C API)                    |
| Size             | ~300 lines + ~900 line library  | N/A                      | ~80 lines                            |
| Actions          | 4                               | N/A                      | 2                                    |
| Spawns processes | Yes                             | No                       | No                                   |
| Profile tracking | Yes (HashMap + crash detection) | No                       | No                                   |
| `open` works     | Yes                             | No (launchctl)           | Yes                                  |
| Ongoing traffic  | Relays commands to profiles     | Receives IOSurface ports | None                                 |
| Deployment       | XPC service in app bundle       | launchd plist            | launchd plist (SMAppService planned) |

#### Design Lessons

The ts5 xpc-gateway is dramatically simpler than ts3's launcher because it
separates concerns. ts3's launcher was both a rendezvous point and a process
manager. ts5 splits these: the gateway is pure rendezvous, and process
management is external.

What ts5 could learn from ts3:

- **Crash recovery.** ts3 detected profile crashes via XPC error handlers and
  cleaned up the registry. ts5's gateway stores one endpoint — if the app
  crashes and restarts, it re-registers (which works). But stale endpoints could
  matter if the app dies without re-registering.
- **The `termsurf-xpc` crate.** ts3 built a full safe Rust XPC library. ts5's
  `web/src/xpc.rs` uses raw FFI. If XPC usage grows, the safe wrapper pattern
  may be worth reviving.

## Goal

Launch TermSurf with `open ts5/zig-out/TermSurf.app` and have the XPC overlay
pipeline work exactly as it does today.

## Architecture

### Current (Issue 505)

```
web ──XPC──▶ TermSurf app (com.termsurf.xpc-gateway)
              ├── XPC listener (Mach service)
              └── Renderer (set_overlay)
```

The app is both the Mach service and the renderer. Must be launched via
`launchctl kickstart`.

### Proposed

```
                     ┌──────────────────────┐
                     │    XPC Gateway       │
                     │  (com.termsurf.      │
                     │   xpc-gateway)       │
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

1. **XPC gateway** — Tiny binary. Owns the `com.termsurf.xpc-gateway` Mach
   service. Managed by launchd. Its only job is rendezvous: the app registers
   its anonymous endpoint, and `web` processes claim it.

2. **TermSurf app** — Launched normally via `open`. On startup, connects to
   `com.termsurf.xpc-gateway` as a client and sends an anonymous XPC listener
   endpoint. Handles `set_overlay` messages from `web` processes on the direct
   connection.

3. **`web` TUI** — Connects to `com.termsurf.xpc-gateway`, requests the app's
   endpoint, then connects directly to the app. All overlay messages flow on the
   direct connection — no relay hop through the gateway.

### Why Direct Connection (Not Relay)

The gateway could relay every message from `web` to the app, but direct
connection is better:

- **No per-message relay hop.** Overlay coordinates are sent every 250ms today.
  IOSurface Mach ports will be sent at 60fps in the future. A relay hop adds
  latency and CPU overhead for every frame.
- **Proven pattern.** ts3 used exactly this approach — the launcher relayed
  endpoints, then profile servers connected directly to the GUI for IOSurface
  Mach port transfer.
- **Simpler gateway.** The gateway handles two message types (`register_app`,
  `connect`) and no ongoing traffic. It could crash and restart without
  interrupting active `web` sessions (they already have direct connections).

### Why Not Eliminate the Gateway Entirely

The gateway exists solely because of a macOS constraint: a Mach service can only
be claimed by the process launchd launched for that job. Without a gateway, the
app must be launched by launchd. With a gateway, the app launches normally and
the gateway provides the well-known rendezvous point.

There is no alternative IPC mechanism that avoids this. XPC is the only way to
transfer IOSurface Mach ports between processes on macOS. See CLAUDE.md "Settled
Architectural Decisions".

## XPC Protocol

### Gateway Messages

The gateway handles two actions:

**`register_app`** — Sent by the TermSurf app on startup.

```
→ { action: "register_app", endpoint: <anonymous_listener_endpoint> }
```

The gateway stores the endpoint. If a previous endpoint exists (app restarted),
it replaces it.

**`connect`** — Sent by `web` processes.

```
→ { action: "connect", pane_id: "<uuid>" }
← { endpoint: <app_anonymous_listener_endpoint> }
```

The gateway returns the app's endpoint. The `web` process uses it to establish a
direct connection to the app.

If the app hasn't registered yet (gateway started before app), the gateway can
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
                  ├── connect_mach_service("com.termsurf.xpc-gateway")
                  │   └── launchd auto-starts gateway if not running
                  ├── create anonymous XPC listener
                  ├── send { action: "register_app", endpoint: <listener> }
                  └── set event handler on anonymous listener
                      (handles web connections)

3. User types:    cargo run -p web -- https://example.com

4. web starts:    read TERMSURF_PANE_ID from env
                  ├── connect_mach_service("com.termsurf.xpc-gateway")
                  ├── send { action: "connect", pane_id: "<uuid>" }
                  ├── receive reply with app endpoint
                  ├── connect to app via endpoint
                  └── send set_overlay on direct connection each frame

5. web exits:     direct connection closes
                  └── app detects disconnect, clears overlay
```

## Components

### XPC Gateway

A standalone binary, ~50–100 lines. Written in Swift or Rust — either works
since the XPC C API is the same. Swift may be simpler because
`xpc_connection_create_mach_service` and `xpc_connection_set_event_handler` are
more ergonomic with closures.

**Responsibilities:**

- Listen on `com.termsurf.xpc-gateway` (LISTENER flag)
- Accept connections from the app (stores endpoint)
- Accept connections from `web` processes (returns endpoint)
- No ongoing traffic once connections are established

**Lifecycle:**

- Launched on-demand by launchd when first client connects
- Stays running while any client is connected
- Can exit when all clients disconnect (optional — launchd restarts on next
  connection anyway)

### launchd Plist

Same as today but points to the gateway binary instead of the app:

```xml
<key>ProgramArguments</key>
<array>
    <string>/path/to/xpc-gateway</string>
</array>
```

### TermSurf App Changes

Replace `CompositorXPC.swift`'s listener with a client connection:

**Before (Issue 505):**

```swift
// App IS the listener
let conn = xpc_connection_create_mach_service(
    "com.termsurf.xpc-gateway", queue,
    UInt64(XPC_CONNECTION_MACH_SERVICE_LISTENER))
```

**After:**

```swift
// App connects as client, sends anonymous listener
let gateway = xpc_connection_create_mach_service(
    "com.termsurf.xpc-gateway", queue, 0)  // no LISTENER flag

let listener = xpc_connection_create(nil, queue)  // anonymous
// ... set up handler for web connections on listener ...

let msg = xpc_dictionary_create(nil, nil, 0)
xpc_dictionary_set_string(msg, "action", "register_app")
xpc_dictionary_set_value(msg, "endpoint",
    xpc_endpoint_create(listener))
xpc_connection_send_message(gateway, msg)
```

The handler on the anonymous listener processes `set_overlay` messages exactly
as `CompositorXPC.swift` does today.

### `web` TUI Changes

Replace the direct Mach service connection with a two-step connect:

1. Connect to `com.termsurf.xpc-gateway` (the gateway)
2. Send `{ action: "connect", pane_id: "<uuid>" }`
3. Receive reply with endpoint
4. Connect to app via endpoint
5. Send `set_overlay` on the direct connection

This requires `xpc_connection_send_message_with_reply` (or the sync variant) to
get the endpoint back from the gateway.

## Verification

1. `open ts5/zig-out/TermSurf.app` launches the app normally.
2. In a TermSurf pane: `cargo run -p web -- https://example.com` shows the pink
   overlay.
3. Resizing works. Quitting `web` clears the overlay.
4. Killing and relaunching the app works (gateway stays running, `web`
   reconnects on next launch).
5. The gateway is invisible to the user — no manual `launchctl` commands needed
   after initial plist registration.

## Experiments

### Experiment 1: XPC Gateway with Endpoint Relay

Implement the three-process architecture: a standalone Swift gateway owns the
Mach service, the app registers an anonymous endpoint, and `web` processes claim
the endpoint to connect directly to the app.

#### Changes

##### Part 1: XPC Gateway Binary

###### `ts5/xpc-gateway/Package.swift`

Minimal Swift Package Manager executable:

```swift
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "XPCGateway",
    platforms: [.macOS(.v14)],
    targets: [
        .executableTarget(name: "xpc-gateway",
                          path: "Sources")
    ]
)
```

###### `ts5/xpc-gateway/Sources/main.swift`

~60 lines. The gateway:

1. Creates a Mach service listener on `com.termsurf.xpc-gateway`.
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

###### `ts5/macos/com.termsurf.xpc-gateway.plist`

New plist replacing `com.termsurf.compositor.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.xpc-gateway</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.xpc-gateway</key>
        <true/>
    </dict>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/ryan/dev/termsurf/ts5/xpc-gateway/.build/debug/xpc-gateway</string>
    </array>
    <key>StandardOutPath</key>
    <string>/Users/ryan/dev/termsurf/logs/xpc-gateway.log</string>
    <key>StandardErrorPath</key>
    <string>/Users/ryan/dev/termsurf/logs/xpc-gateway.log</string>
</dict>
</plist>
```

##### Part 3: App Connects as Client

###### `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

Replace the Mach service listener with:

1. **Client connection to gateway.** Connect to `com.termsurf.xpc-gateway`
   without the LISTENER flag. Resume the connection.

2. **Anonymous XPC listener.** Create with `xpc_connection_create(nil, queue)`.
   Set its event handler to accept peer connections from `web` processes. Each
   peer's message handler calls `handleMessage` (same as today). On disconnect,
   calls `handleDisconnect` (same as today). Resume the listener.

3. **Register endpoint.** Create an endpoint from the anonymous listener with
   `xpc_endpoint_create(listener)`. Send `{ action: "register_app", endpoint }`
   to the gateway.

The `handleMessage` and `handleDisconnect` methods stay unchanged — they already
handle `set_overlay`, surface lookup, and cleanup.

Key details:

- The anonymous listener must be stored in a strong property (not just a local
  variable) to prevent ARC release.
- `xpc_endpoint_create` takes a listener connection and returns a serializable
  endpoint object that can be sent over XPC to other processes.
- The gateway connection only carries the `register_app` message at startup. All
  subsequent traffic flows on the anonymous listener's direct connections.

##### Part 4: `web` Two-Step Connect

###### `web/src/xpc.rs`

Replace `CompositorConnection::connect()` with a two-step flow:

1. Connect to `com.termsurf.xpc-gateway` (the gateway).
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

The gateway connection can be dropped after the endpoint is received — it's not
needed for ongoing communication.

#### Build & Test

```bash
# Build the gateway
cd ts5/xpc-gateway && swift build

# Build ts5
cd ts5 && zig build

# Build web TUI
cargo build -p web

# Clear old launchd registration and re-register with gateway
launchctl bootout gui/$(id -u)/com.termsurf.compositor
launchctl bootstrap gui/$(id -u) ts5/macos/com.termsurf.xpc-gateway.plist

# Launch the app normally
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- https://example.com
```

#### Pass Criteria

1. `open ts5/zig-out/TermSurf.app` launches the app and the pink overlay works.
2. No `launchctl kickstart` needed — the gateway auto-starts when the app
   connects.
3. The pink overlay appears, resizes correctly, and clears on `web` exit.
4. Relaunching the app (quit and `open` again) works — the gateway stays running
   and the app re-registers its endpoint.
5. The gateway process is visible in Activity Monitor as `xpc-gateway` but
   requires no user interaction.

#### Result: PASS

All five pass criteria met. `open ts5/zig-out/TermSurf.app` launches the app
normally, the gateway auto-starts via launchd, the pink overlay appears and
resizes correctly, and quitting `web` clears it. No `launchctl kickstart`
needed.

##### Conclusion

###### What Was Built

| File                                            | Role                                                           |
| ----------------------------------------------- | -------------------------------------------------------------- |
| `ts5/xpc-gateway/Package.swift`                 | SwiftPM package for the gateway daemon                         |
| `ts5/xpc-gateway/Sources/main.swift`            | Gateway: owns Mach service, stores/returns app endpoint        |
| `ts5/macos/com.termsurf.xpc-gateway.plist`      | launchd plist for the gateway                                  |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | App: connects as client, registers anonymous listener endpoint |
| `web/src/xpc.rs`                                | `web`: two-step connect (gateway → endpoint → direct to app)   |

###### What Was Proven

1. **The three-process endpoint relay works.** The gateway owns the Mach
   service, the app registers an anonymous listener endpoint, and `web`
   processes claim the endpoint to connect directly to the app. No relay hop for
   ongoing overlay messages.

2. **`open` works.** The app launches normally via `open` instead of
   `launchctl
   kickstart`. The gateway handles Mach service ownership, freeing
   the app from launchd identity constraints.

3. **Anonymous XPC listeners work for direct connections.** The app creates an
   anonymous listener with `xpc_connection_create(nil, queue)`, extracts an
   endpoint with `xpc_endpoint_create`, and sends it through the gateway. `web`
   processes use `xpc_connection_create_from_endpoint` to connect directly. This
   is the same pattern ts3 used for IOSurface Mach port transfer.

4. **The gateway is ~80 lines of Swift.** It handles two message types
   (`register_app`, `connect`) and carries no ongoing traffic. It could crash
   and restart without interrupting active `web` sessions.

###### What Remains

- **Automatic plist registration.** The gateway plist must be registered with
  launchd via `launchctl bootstrap` (one-time setup). Apple's `SMAppService` API
  (macOS 13+) can automate this — the plist ships inside the app bundle at
  `Contents/Library/LaunchAgents/` and the app calls
  `SMAppService.agent
  (plistName:).register()` on first launch.

- **Chromium texture swap.** The pink overlay will be replaced with a real
  IOSurface texture from Chromium. The direct connection established here is the
  same channel that will carry IOSurface Mach ports at 60fps.

### Experiment 2: Automatic Plist Registration via SMAppService

Eliminate manual `launchctl bootstrap` by using Apple's `SMAppService` API. The
gateway plist and binary ship inside the app bundle. The app registers the
LaunchAgent on startup — zero user setup.

#### Background

`SMAppService` (macOS 13+) lets an app register LaunchAgents from within its own
bundle. The plist lives at `Contents/Library/LaunchAgents/` and references the
agent binary via `BundleProgram` — a bundle-relative path. No absolute paths, no
`launchctl` commands, no copying files to `~/Library/LaunchAgents/`.

Key behaviors:

- `register()` is **not** idempotent — throws if already registered. Must check
  `.status` first.
- The binary referenced by `BundleProgram` must be inside the app bundle
  (typically `Contents/MacOS/`).
- No special entitlements needed for non-sandboxed apps (TermSurf is not
  sandboxed).
- The agent appears in System Settings > General > Login Items, giving users
  control.

#### Changes

##### Part 1: Move Gateway Binary into App Bundle

The `xpc-gateway` binary must live inside `TermSurf.app/Contents/MacOS/`. The
simplest approach: add a post-build copy step in `GhosttyXcodebuild.zig` that
copies the pre-built binary into the bundle after xcodebuild finishes.

###### `ts5/src/build/GhosttyXcodebuild.zig`

After the existing `copy` step (line 176), add a new step that copies the
gateway binary into the app bundle's `Contents/MacOS/` directory:

```zig
const copy_gateway = RunStep.create(b, "copy xpc-gateway into bundle");
copy_gateway.addArgs(&.{
    "cp",
    b.pathFromRoot("xpc-gateway/.build/debug/xpc-gateway"),
    b.fmt("{s}/Contents/MacOS/xpc-gateway", .{app_path}),
});
copy_gateway.step.dependOn(&build.step);
```

Make the `copy` step depend on `copy_gateway` so the final bundle includes the
binary.

This requires `swift build` to have been run in `ts5/xpc-gateway/` beforehand.
The Zig build does not build Swift packages — that's a manual step for now (same
as the Zig build doesn't build `web/`).

##### Part 2: Bundle the LaunchAgent Plist

###### `ts5/macos/com.termsurf.xpc-gateway.bundle.plist`

New plist designed for SMAppService. Uses `BundleProgram` instead of
`ProgramArguments`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.termsurf.xpc-gateway</string>
    <key>BundleProgram</key>
    <string>Contents/MacOS/xpc-gateway</string>
    <key>MachServices</key>
    <dict>
        <key>com.termsurf.xpc-gateway</key>
        <true/>
    </dict>
</dict>
</plist>
```

Key differences from the Experiment 1 plist:

- **`BundleProgram`** replaces `ProgramArguments`. The path is relative to the
  app bundle root — no hardcoded absolute paths. The app bundle can live
  anywhere on disk.
- **No `StandardOutPath`/`StandardErrorPath`.** These used hardcoded paths into
  the repo. For bundled agents, logging should use `os_log` or stderr (which
  launchd captures to the unified log). We can add log paths back later if
  needed for debugging.

Add a post-build step in `GhosttyXcodebuild.zig` to copy this plist into the
bundle:

```zig
const mkdir_la = RunStep.create(b, "mkdir LaunchAgents in bundle");
mkdir_la.addArgs(&.{
    "mkdir", "-p",
    b.fmt("{s}/Contents/Library/LaunchAgents", .{app_path}),
});
mkdir_la.step.dependOn(&build.step);

const copy_plist = RunStep.create(b, "copy xpc-gateway plist into bundle");
copy_plist.addArgs(&.{
    "cp",
    b.pathFromRoot("macos/com.termsurf.xpc-gateway.bundle.plist"),
    b.fmt("{s}/Contents/Library/LaunchAgents/com.termsurf.xpc-gateway.plist",
          .{app_path}),
});
copy_plist.step.dependOn(&mkdir_la.step);
```

##### Part 3: Register on App Startup

###### `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

Before connecting to the gateway, register the LaunchAgent via SMAppService. Add
this at the top of `start()`:

```swift
import ServiceManagement

// Register the xpc-gateway LaunchAgent if not already registered.
let gatewayService = SMAppService.agent(
    plistName: "com.termsurf.xpc-gateway.plist")
switch gatewayService.status {
case .notRegistered, .notFound:
    do {
        try gatewayService.register()
        fputs("[Compositor] Registered xpc-gateway LaunchAgent\n", stderr)
    } catch {
        fputs("[Compositor] Failed to register xpc-gateway: \(error)\n", stderr)
    }
case .enabled:
    fputs("[Compositor] xpc-gateway LaunchAgent already registered\n", stderr)
case .requiresApproval:
    fputs("[Compositor] xpc-gateway requires user approval in System Settings\n",
          stderr)
@unknown default:
    break
}
```

The `register()` call tells launchd about the agent. When the app then calls
`xpc_connection_create_mach_service("com.termsurf.xpc-gateway", ...)`, launchd
auto-starts the gateway if it isn't already running.

##### Part 4: Keep Development Plist for Convenience

The existing `com.termsurf.xpc-gateway.plist` (with absolute paths) stays for
development — it's useful when running the gateway standalone outside the app
bundle. The new `com.termsurf.xpc-gateway.bundle.plist` is the bundled version.

#### Build & Test

```bash
# Build the gateway (must be done before zig build)
cd ts5/xpc-gateway && swift build

# Build ts5 (copies gateway binary + plist into bundle)
cd ts5 && zig build

# Unregister old launchd services (one-time cleanup)
launchctl bootout gui/$(id -u)/com.termsurf.xpc-gateway 2>/dev/null
launchctl bootout gui/$(id -u)/com.termsurf.compositor 2>/dev/null

# Launch the app — no launchctl bootstrap needed!
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- https://example.com
```

#### Pass Criteria

1. `open ts5/zig-out/TermSurf.app` launches the app with zero prior setup — no
   `launchctl bootstrap` needed.
2. The xpc-gateway LaunchAgent registers automatically on first launch.
3. The pink overlay works: appears, resizes, clears on `web` exit.
4. Quitting and relaunching the app works — the gateway stays registered and
   auto-starts.
5. The gateway binary inside the bundle is found correctly via `BundleProgram`
   (no absolute paths).
6. `xpc-gateway` appears in System Settings > General > Login Items under
   TermSurf.

#### Result: PASS

All six pass criteria met. The app launches via `open`, the SMAppService
registration happens automatically, the pink overlay works, and no
`launchctl
bootstrap` is needed.

#### Gotcha: Stale App Process

The initial test appeared to fail with `[web] Gateway error: "no_app"`. The
cause was a stale TermSurf process from before the experiment. macOS keeps the
app in the Dock after closing the window — the process is still running. The
stale process had connected to the gateway and registered its endpoint under the
old launchd registration, but after booting out and re-registering via
SMAppService, the gateway restarted with no stored endpoint.

The fix: right-click the Dock icon and quit, then relaunch with `open`. The
fresh app registers its endpoint with the new gateway, and `web` connects
successfully. This is not a bug in the SMAppService approach — it's a one-time
migration issue when switching from manual `launchctl bootstrap` to
SMAppService.

#### Conclusion

##### What Was Built

| File                                              | Role                                                        |
| ------------------------------------------------- | ----------------------------------------------------------- |
| `ts5/macos/com.termsurf.xpc-gateway.bundle.plist` | SMAppService plist with `BundleProgram` (no absolute paths) |
| `ts5/src/build/GhosttyXcodebuild.zig`             | Post-build steps to copy gateway binary + plist into bundle |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift`   | SMAppService registration on app startup                    |

##### What Was Proven

1. **SMAppService works for XPC gateway registration.** The app calls
   `SMAppService.agent(plistName:).register()` on startup, launchd picks up the
   bundled plist, and the gateway auto-starts when `web` connects to
   `com.termsurf.xpc-gateway`. Zero manual `launchctl` commands.

2. **BundleProgram resolves correctly.** The plist uses
   `Contents/MacOS/xpc-gateway` (bundle-relative path) and launchd finds the
   binary inside the app bundle. No absolute paths anywhere.

3. **The build system bundles everything.** `zig build` copies the pre-built
   gateway binary into `Contents/MacOS/` and the plist into
   `Contents/Library/LaunchAgents/`. The app bundle is self-contained.

4. **Status checking prevents duplicate registration.** Checking
   `gatewayService.status` before calling `register()` avoids the not-idempotent
   trap — subsequent launches skip registration when the agent is already
   enabled.

##### What Remains

- **Chromium texture swap.** The pink overlay will be replaced with a real
  IOSurface texture from Chromium. The direct XPC connection established in
  Experiment 1 is the same channel that will carry IOSurface Mach ports at
  60fps.
