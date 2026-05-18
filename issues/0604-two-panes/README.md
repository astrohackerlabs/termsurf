+++
status = "closed"
opened = "2026-02-20"
closed = "2026-02-20"
+++

# Issue 604: Two Panes

## Goal

Two terminal panes, each running `web`, both rendering live Chromium frames
simultaneously. Same profile, same server process, two independent browser tabs
streaming at 60fps to two different grid overlays.

## Background

Issue 603 proved the full Ghost pipeline works for a single pane: `web` sends
`set_overlay`, Ghost spawns a Chromium Profile Server, receives IOSurface Mach
ports at 60fps, and composites them as a Metal overlay at the correct grid
coordinates. Dynamic resize works. Clean exit kills the server.

But `xpc.zig` has a single `var pane: Pane = .{}`. A second `web` process
connecting to the same Ghost instance overwrites the first pane's state. Two
panes cannot coexist.

### What we have

**Ghost (from Issues 601–603):**

- XPC gateway connection, anonymous listener, endpoint registration
- `Pane` struct with per-pane mutex, web/server peer tracking (mutex replaced by
  serial queue in this issue)
- XPC connections use default concurrent dispatch queue (no target queue set)
- Server lifecycle: spawn, `server_register` → `create_tab` → `display_surface`
- IOSurface → Metal texture pipeline (zero-copy)
- Dynamic resize via `sendResize`
- `setOverlay()` / `setOverlayIOSurface()` / `clearOverlay()` with `draw_mutex`
- Surface lookup by pane ID (`app.findSurfaceByPaneId`)

**Chromium Profile Server (from ts5 Issues 503–515):**

- Multi-tab support: one process hosts N WebContents, each with an independent
  `FrameSinkVideoCapturer` streaming at 60fps
- `CreateTab` adds a new Shell + VideoConsumer + per-tab XPC connection
- `CloseTab` fires when a tab's connection drops (server continues if tabs
  remain)
- `display_surface` messages carry `pane_id` — the server stamps each tab's
  frames with the pane UUID from `create_tab`
- `resize` routes by `pane_id` to the correct tab
- Server auto-exits when the last tab closes
- Profile lock on `--user-data-dir`: a second server with the same path will
  crash. Server reuse is a correctness requirement, not an optimization.

**`web` TUI:**

- Already sends `pane_id`, `url`, `profile` in `set_overlay`
- Each `web` instance gets its own `TERMSURF_PANE_ID` from the shell environment
- No changes needed to `web`

### What needs to change

**1. Serial dispatch queue for XPC state.**

Currently XPC connections use the default concurrent dispatch queue, so handlers
can fire on different threads simultaneously. Issue 603 worked around this with
a per-pane mutex, but multi-pane requires coordinating state across panes (e.g.
server reuse, disconnect cleanup). Per-pane mutexes don't cover cross-pane
operations; a global mutex would bottleneck everything.

The proven solution (ts5 Issue 511): create a serial dispatch queue and set it
as the target queue for all XPC connections. All handlers run serially — no
mutexes needed, no deadlocks possible, no lock ordering concerns. ts5 ran three
panes at 60fps each (180 `display_surface` messages/second) plus mouse, scroll,
and keyboard events on one serial queue with no bottleneck — each handler is
microseconds of work.

Zig calls `dispatch_queue_create` and `xpc_connection_set_target_queue` directly
(C APIs via `@cImport` or manual extern declarations).

**2. Multi-pane state in `xpc.zig`.**

Replace `var pane: Pane = .{}` with a data structure that maps pane UUIDs to
`Pane` structs. When `set_overlay` arrives with a new `pane_id`, create a new
`Pane`. When a web peer disconnects, clean up only that pane. No mutex on `Pane`
— the serial queue serializes all access.

**3. Server reuse by profile.**

Currently Ghost spawns a new server for every `set_overlay` with a URL. With two
panes on the same profile, the second pane must reuse the first pane's server.
Need a profile → server mapping so that `handleSetOverlay` can detect an
existing server and send `create_tab` on its control connection instead of
spawning.

**4. `display_surface` routing.**

Currently `handleDisplaySurface` writes to `pane.overlay_surface` — a single
surface. With multiple panes, the handler must read `pane_id` from each
`display_surface` message and route the IOSurface to the correct pane's surface.

**5. Per-pane disconnect.**

Currently `handleDisconnect` kills the server and cleans up everything. With
multiple panes sharing a server, disconnecting one pane should only remove that
pane's tab. The server should be killed only when all panes for that profile
have disconnected (or the server auto-exits when its last tab closes).

**6. Per-pane resize.**

Currently `sendResize` sends on `pane.server_peer`. With multiple panes sharing
one server, resize messages go on the shared control connection but must include
`pane_id` so the server routes to the correct tab. This already works — the
`resize` message already includes `pane_id`.

### What should work without changes

- **xpc-gateway** — Stateless rendezvous. No pane or profile awareness.
- **Metal renderer** — Each surface independently receives IOSurface frames.
- **`web` TUI** — Already sends unique `pane_id` per instance.
- **Chromium server multi-tab** — Proven in Issue 503 Experiment 3, used in
  Issue 511.
- **Overlay shader** — Reads pixel dimensions per IOSurface.

## Experiment 1: Multi-pane XPC routing

### Goal

Two terminal panes, each running `web http://localhost:9407`, both rendering the
box demo simultaneously at 60fps. Same profile, one Chromium server process.

### Design

**Serial dispatch queue** in `xpc.zig`:

Create a serial dispatch queue and set it as the target queue for the gateway
connection, the anonymous listener, and every peer connection. All XPC event
handlers then run serially on this queue. No mutexes needed — state access is
inherently serialized.

```zig
extern "c" fn dispatch_queue_create(label: [*:0]const u8, attr: ?*anyopaque) ?*anyopaque;
extern const _dispatch_queue_attr_concurrent: anyopaque; // not used — null = serial
extern "c" fn xpc_connection_set_target_queue(conn: xpc_object_t, queue: ?*anyopaque) void;

var xpc_queue: ?*anyopaque = null;
```

In `init()`, before creating connections:

```zig
xpc_queue = dispatch_queue_create("com.termsurf.ghost.xpc", null); // null = serial
```

Then set on every connection:

```zig
gateway = xpc_connection_create_mach_service("com.termsurf.xpc-gateway", null, 0);
xpc_connection_set_target_queue(gateway, xpc_queue);
// ... same for listener and each peer in listenerHandler
```

**Data structures** in `xpc.zig`:

Replace the single `var pane: Pane = .{}` with:

```zig
/// Active panes, keyed by pane UUID string.
var panes: std.StringHashMap(*Pane) = undefined;

/// Active servers, keyed by profile name.
var servers: std.StringHashMap(*Server) = undefined;

/// Reverse lookup: connection pointer → pane UUID string.
var peer_to_pane: std.AutoHashMap(usize, []const u8) = undefined;

/// Reverse lookup: connection pointer → profile name (for server peers).
var peer_to_profile: std.AutoHashMap(usize, []const u8) = undefined;
```

Where `Server` holds the shared server state:

```zig
const Server = struct {
    process: std.process.Child,
    peer: xpc_object_t = null,
    profile: []const u8,
    pane_count: usize = 0,
};
```

And `Pane` drops the mutex (serial queue handles serialization):

```zig
const Pane = struct {
    web_peer: xpc_object_t = null,
    overlay_surface: ?*CoreSurface = null,
    server: ?*Server = null,
    pane_id: [36]u8 = undefined,
    profile: []const u8 = "",
    pending_url_buf: [2048]u8 = undefined,
    pending_url_len: usize = 0,
    pending_pixel_w: u64 = 0,
    pending_pixel_h: u64 = 0,
};
```

No mutexes on any of these. All access happens on the serial `xpc_queue`.

Note: `setOverlayIOSurface` and `setOverlay` still use `draw_mutex` internally —
that's the renderer thread lock, separate from XPC state. The serial queue
protects XPC state; `draw_mutex` protects renderer state. They don't interact.

**`handleSetOverlay` flow:**

1. Extract `pane_id` from message
2. Look up or create `Pane` in `panes` for this pane ID
3. Store overlay surface, URL, pixel dimensions on the pane
4. Register `connection_ptr → pane_id` in `peer_to_pane`
5. Look up `servers` by profile:
   - **No server:** spawn one, store URL as pending, increment `pane_count`
   - **Server exists, peer not yet connected:** store URL as pending (server is
     still starting up), increment `pane_count`
   - **Server exists, peer connected:** send `create_tab` immediately, increment
     `pane_count`
6. If server already running and dimensions changed: send `resize`

**`handleServerRegister` flow:**

1. Extract `profile` from message, look up `Server`
2. Store the peer connection on the `Server`
3. Register `connection_ptr → profile` in `peer_to_profile`
4. Iterate all panes whose server matches and have pending URLs → send
   `create_tab` for each

**`handleDisplaySurface` flow:**

1. Extract `pane_id` from message
2. Look up `Pane` by pane ID
3. Import IOSurface, route to that pane's surface

**Disconnect flow:**

The `listenerHandler` now passes `peer` to the peer event handler via a closure
or by capturing the connection pointer. On disconnect, the handler receives the
connection object directly (same object that was passed to
`xpc_connection_set_event_handler`).

When a web peer disconnects:

1. Look up `peer_to_pane[connection_ptr]` → pane ID
2. Look up pane, get its profile and server
3. Clear overlay on the pane's surface
4. Decrement server's `pane_count`
5. If `pane_count == 0`: kill server, remove from `servers` and
   `peer_to_profile`
6. Remove pane from `panes` and `peer_to_pane`

When a server peer disconnects (server crashed or exited):

1. Look up `peer_to_profile[connection_ptr]` → profile
2. Find all panes that reference this server, clear their overlays
3. Remove server from `servers`, panes from `panes`
4. Clean up reverse lookup maps

All of this runs on the serial queue — no concurrent access, no races.

### Verification

```bash
cd box-demo && bun run server.ts &
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
# In pane 1:
cargo run -p web -- http://localhost:9407
# Split pane (Cmd+D or equivalent), in pane 2:
cargo run -p web -- http://localhost:9407
```

Pass criteria:

- Both panes render the box demo simultaneously at 60fps
- Only one `chromium_profile_server` process running (same profile)
- Closing one pane doesn't affect the other
- Closing the last pane kills the server
- Resize works independently per pane

### Result: Pass

Tested with three panes on the same profile, all rendering the box demo
simultaneously. One Chromium server process (PID 29095) hosted all three tabs.

**Timeline (from logs):**

1. First `web` connects → Ghost spawns server → `create_tab` → 60fps streaming
2. Split pane → second `web` connects → Ghost reuses server → `create_tab` →
   `2 tab(s) active` → 60fps on both
3. Third pane → Ghost reuses server again → `create_tab` → `3 tab(s) active` →
   60fps on all three
4. Resize one pane → only that tab's capturer resizes, others unaffected
5. Close panes → server continues until last tab closes

**Observations:**

- **Three tabs at 60fps each** — 180 `display_surface` messages/second on the
  serial queue with no frame drops or bottleneck.
- **Server reuse worked first try** — the `servers` HashMap keyed by profile
  correctly shared one Chromium process across all three panes.
- **Independent resize** — resizing pane 1 (780→377→780) while panes 2 and 3
  continued streaming unaffected.
- **First-tab latency ~1s, subsequent tabs ~0.2s** — the server is already warm
  for the second and third tabs, so `create_tab` → first frame is much faster.
- **`[ProfileServer] Gateway error`** appears in every session — the server's
  gateway connection errors after registration because the gateway drops the
  connection once the endpoint is claimed. Harmless but noisy; could suppress in
  a future issue.

## Conclusion

Issue 604 is complete. The multi-pane architecture works: serial dispatch queue,
per-pane state maps, server reuse by profile, `display_surface` routing by
`pane_id`, and per-pane disconnect cleanup. Three panes at 60fps each on one
server process — matching the throughput ts5 achieved in Issue 511, now in Zig.

No changes were needed to `web`, xpc-gateway, the Chromium server, or the Metal
renderer. The entire issue was a rewrite of `xpc.zig` — replacing the single
`var pane: Pane` with `StringHashMap`-based multi-pane state and a GCD serial
queue for thread safety.
