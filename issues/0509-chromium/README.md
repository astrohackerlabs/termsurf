+++
status = "closed"
opened = "2026-02-16"
closed = "2026-02-16"
+++

# Issue 509: Chromium Streaming (Retry)

## Background

Issue 507 proved the full Chromium pipeline works end-to-end. In Experiment 4,
the box-demo (blue spinning square at `http://localhost:9407`) rendered inside a
TermSurf pane at 58-60fps for approximately three seconds before crashing. Every
piece of the pipeline ran successfully:

1. **`web` TUI** sent viewport grid coordinates and URL to the app via XPC.
2. **TermSurf app** spawned the Chromium Profile Server and forwarded the URL.
3. **Chromium Profile Server** connected to the app via the xpc-gateway's
   two-step endpoint handoff, navigated to the URL, and captured frames with
   `FrameSinkVideoCapturer` at 1600x1200 (Retina).
4. **IOSurface Mach ports** streamed at 60fps from the server to the app over
   the direct XPC connection.
5. **Metal renderer** created MTLTextures from the IOSurfaces and composited
   them at the correct grid coordinates inside the terminal pane.

The crash was an IOSurface use-after-free across the Swift/Zig boundary. When a
new frame arrived (every ~16ms), Swift replaced `currentSurfaces[uuid]` with the
new IOSurface. ARC released the previous one while the Zig renderer still held a
raw pointer to it.

Issue 508 solved this. Five experiments traced the path from misdiagnosis
(blamed a race condition) through lost logs (stderr discarded by `open`) to the
actual root cause: **Metal requires `bytesPerRow` to be a multiple of 16 for
IOSurface-backed textures.** The fix was one line:
`(pixelWidth * 4 + 15) & ~15`. Along the way, Issue 508 also established:

- **CFRetain/CFRelease on the Zig side** — when the renderer receives a new
  IOSurface pointer, it calls `CFRetain` on the new one and `CFRelease` on the
  old one. This gives Zig its own ownership stake independent of Swift ARC.
- **`open --stderr <path>`** as the standard debugging approach for TermSurf (no
  source code changes needed to capture logs).
- **`Texture.fromIOSurface()`** — zero-copy MTLTexture creation from an
  IOSurface reference, created per-frame (correct pattern for streaming
  content).

### What exists now

The codebase has all the infrastructure needed:

| Component                                   | Location                          | State               |
| ------------------------------------------- | --------------------------------- | ------------------- |
| Metal overlay pipeline (`overlay`)          | `shaders.zig`, `shaders.metal`    | Working (Issue 508) |
| Pink overlay fallback (`pink_overlay`)      | `shaders.zig`, `shaders.metal`    | Working (Issue 505) |
| IOSurface texture import                    | `Texture.zig:fromIOSurface`       | Working (Issue 508) |
| CFRetain/CFRelease lifetime                 | `Surface.zig:setOverlayIOSurface` | Working (Issue 508) |
| `ghostty_surface_set_overlay` (grid coords) | C API                             | Working (Issue 505) |
| `ghostty_surface_set_overlay_iosurface`     | C API                             | Working (Issue 508) |
| `ghostty_surface_get_cell_size`             | C API                             | Working (Issue 508) |
| XPC gateway (rendezvous)                    | `xpc-gateway/`                    | Working (Issue 506) |
| App XPC client + anonymous listener         | `CompositorXPC.swift`             | Working (Issue 506) |
| `web` TUI (URL bar, viewport, modes)        | `web/src/main.rs`                 | Working (Issue 504) |
| `web` XPC client (two-step connect)         | `web/src/xpc.rs`                  | Working (Issue 506) |
| Pane ID propagation (`TERMSURF_PANE_ID`)    | `SurfaceView_AppKit.swift`        | Working (Issue 505) |
| Checkerboard test IOSurface                 | `CompositorXPC.swift`             | Working (Issue 508) |

### What Issue 507 built but reverted

Issue 507 Experiments 3-4 added Chromium-specific code to three components. This
code was reverted along with all of Issue 507's overlay changes when that issue
concluded. Issue 508 reimplemented the overlay infrastructure (shaders,
pipeline, `Texture.fromIOSurface`, CFRetain/CFRelease, cell size API) but did
NOT reimplement the Chromium-specific pieces:

1. **CompositorXPC.swift — server lifecycle management.** Spawning the Chromium
   Profile Server process, handling `server_register` and `display_surface`
   messages, forwarding `create_tab` commands, killing the server on `web`
   disconnect. Currently the Swift code only creates a static checkerboard
   IOSurface.

2. **Chromium Profile Server — two-step gateway connect.** The server
   (`shell_browser_main_parts.cc`) needs to connect to the app via the
   xpc-gateway's endpoint relay instead of connecting to a named Mach service
   directly. The Chromium branch `146.0.7650.0-issue-507` has this code but it
   was never merged into the main Chromium branch.

3. **`web` TUI — URL in `set_overlay`.** The `web` process needs to include the
   URL in its `set_overlay` messages so the app knows what page to load. The
   current `send_set_overlay` doesn't include a `url` field.

### What Issue 507 learned

- **`base::CommandLine` requires `=` syntax.** Chromium's command-line parser
  only supports `--flag=value`, not `--flag value`. Process arguments must be
  joined with `=`.
- **The full pipeline runs at 60fps.** Before the crash,
  `FrameSinkVideoCapturer` sustained 58-60fps with 1600x1200 Retina frames
  streaming over XPC.
- __`freopen` only redirects C FILE_ stderr._* Zig writes to fd 2 directly via
  `std.debug.print`, bypassing the C FILE layer. `open --stderr <path>`
  redirects fd 2 at the OS level, capturing everything.

### What Issue 508 fixed

- **bytesPerRow alignment:** `(pixelWidth * 4 + 15) & ~15` for IOSurface-backed
  Metal textures.
- **IOSurface lifetime:** CFRetain on the Zig side when receiving a new
  IOSurface, CFRelease on the old one, all under `draw_mutex`.
- **Resize stability:** The checkerboard IOSurface survives terminal resize
  without crashing.

## Goal

`cargo run -p web -- http://localhost:9407` renders the box-demo (blue spinning
square with FPS counter) inside a TermSurf pane at 60fps. The page renders at
the viewport's grid coordinates, composited by the Metal renderer using the
IOSurface overlay pipeline.

This is the same goal as Issue 507 — but now the IOSurface lifetime management
and bytesPerRow alignment are solved. The crash that ended Issue 507 Experiment
4 after three seconds should not recur.

### Scope

- Single default profile only.
- No resize of the Chromium viewport (the server captures at a fixed size; the
  overlay stretches to fit).
- No URL editing or re-navigation.
- No keyboard/mouse input forwarding to the browser.

### Not in scope

- Multiple browser profiles.
- Retina size matching (viewport pixel dimensions sent to server for
  `SetResolutionConstraints`).
- Dynamic resize (updating capture resolution when the terminal resizes).
- Input forwarding (keyboard, mouse, scroll).

## Process Topology

```
web TUI ────direct XPC────▶ TermSurf app ──spawns──▶ Chromium Profile Server
              (via xpc-gateway               (connects back to app
               endpoint handoff)              via xpc-gateway)

xpc-gateway (com.termsurf.xpc-gateway)
  - Pure rendezvous, no ongoing traffic
  - App registers anonymous listener endpoint
  - web + server both claim endpoint, connect directly to app
```

## Connection Flow

```
1. App starts:     registers endpoint with xpc-gateway

2. web starts:     connects to xpc-gateway → gets endpoint → connects to app
                   sends set_overlay (grid coords + URL) on direct connection

3. App receives:   stores overlay grid coords
                   spawns Chromium Profile Server for the pane

4. Server starts:  connects to xpc-gateway → gets endpoint → connects to app
                   sends server_register (pane_id)
                   app sends create_tab (URL) to server
                   server navigates to URL
                   FrameSinkVideoCapturer captures frames

5. Frame arrives:  server sends display_surface (IOSurface Mach port) to app
                   app imports IOSurface via IOSurfaceLookupFromMachPort
                   app calls ghostty_surface_set_overlay_iosurface
                   Zig renderer CFRetains new, CFReleases old
                   drawFrame() creates MTLTexture from IOSurface, renders quad

6. web exits:      drops XPC connection
                   app kills server, clears overlay, CFReleases IOSurface
```

## XPC Protocol

### `web` to app (direct connection)

**Set overlay (extended with URL):**

```
{ action: "set_overlay", pane_id: "<uuid>",
  col: N, row: N, width: N, height: N,
  url: "http://localhost:9407" }
```

### App to Chromium Profile Server (direct connection)

**Create tab:**

```
{ action: "create_tab", url: "http://localhost:9407", tab_id: "<uuid>" }
```

### Chromium Profile Server to app (direct connection)

**Server register:**

```
{ action: "server_register", pane_id: "<uuid>" }
```

**Display surface (at 60fps):**

```
{ action: "display_surface", pane_id: "<uuid>",
  iosurface_port: <mach_send_right> }
```

## Components to Change

### 1. CompositorXPC.swift — Server lifecycle + display_surface handler

The current `set_overlay` handler creates a static checkerboard IOSurface. This
must be extended to:

- Detect the `url` field in `set_overlay`
- Spawn a Chromium Profile Server process (with `=`-joined flags)
- Handle `server_register` from the server (store the control connection)
- Forward `create_tab` with the URL
- Handle `display_surface` (import IOSurface from Mach port, pass to renderer)
- Kill the server and clean up on `web` disconnect

The checkerboard code can remain as a fallback or be replaced entirely.

### 2. Chromium Profile Server — Two-step gateway connect

The server's `StartDynamicMode` in `shell_browser_main_parts.cc` currently
connects to a named Mach service. It must be changed to the two-step gateway
connect (same pattern as `web/src/xpc.rs`):

1. Connect to `com.termsurf.xpc-gateway`
2. Send `{ action: "connect" }`, receive endpoint in reply
3. Create connection from endpoint
4. Send `server_register` on the direct connection
5. Wait for `create_tab` commands from the app

The `--pane-id` flag must be added so the server can identify itself.

### 3. `web` TUI — Include URL in set_overlay

The `send_set_overlay` function in `web/src/xpc.rs` must include the URL string.
The `web/src/main.rs` call site must pass the URL from the command-line
argument.

### 4. Chromium branch

The `146.0.7650.0-issue-507` branch in the Chromium fork has the two-step
gateway connect and `--pane-id` code from Issue 507 Experiment 3. This can be
used as a starting point (or a new branch can be created from
`146.0.7650.0-issue-503`).

## Build

```bash
# Build Chromium Profile Server
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# Build xpc-gateway
cd ts5/xpc-gateway && swift build

# Build TermSurf
cd ts5 && zig build

# Build web
cargo build -p web
```

## Verification

```bash
# Start the box-demo server
cd ts4/box-demo && bun run server.ts &

# Launch the app
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://localhost:9407

# Expected:
# - Blue spinning square visible in viewport
# - FPS counter on the page shows ~60fps
# - No crash (the IOSurface lifetime fix from Issue 508 prevents it)
# - Quitting web (q or Ctrl+C) clears the overlay and kills the server
#
# Debugging:
# open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
```

## File Summary

| File                                            | Action                                                             |
| ----------------------------------------------- | ------------------------------------------------------------------ |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Add server lifecycle, `server_register`, `display_surface` handler |
| `web/src/xpc.rs`                                | Add `url` parameter to `send_set_overlay`                          |
| `web/src/main.rs`                               | Pass URL to `send_set_overlay`                                     |
| `chromium/src/.../shell_browser_main_parts.cc`  | Two-step gateway connect, `server_register`                        |
| `chromium/src/.../shell_browser_main_parts.h`   | Add `app_endpoint_`, `pane_id_`                                    |
| `chromium/src/.../shell_switches.h`             | Add `--pane-id` flag                                               |
| `chromium/src/.../shell_video_consumer.cc`      | Include `pane_id` in `display_surface`                             |
| `chromium/src/.../shell_video_consumer.h`       | Add `SetPaneId()` method                                           |
| `docs/chromium.md`                              | Add new branch                                                     |

## Ideas for Experiments

### Idea 1: End-to-end streaming

Reimplement the three Chromium-specific pieces that were reverted from Issue 507
on top of the Issue 508 infrastructure: server lifecycle in CompositorXPC.swift,
two-step gateway connect in the Chromium Profile Server, and URL in `web`'s
`set_overlay`. This is essentially Issue 507 Experiments 3+4 redone with the
bytesPerRow alignment fix and CFRetain/CFRelease already in place. The box-demo
should render at 60fps without crashing.

### Idea 2: Stability soak

Run the box-demo for 5+ minutes. Verify no crash, no memory growth, no fps
degradation. At 60fps, five minutes is ~18,000 IOSurface swaps — enough to
expose any subtle lifetime or resource leak issues. Monitor with Activity
Monitor and the server's fps logging.

### Idea 3: Retina size matching

The server currently captures at a fixed 800x600 (stretched to fill the
viewport). Send the viewport's physical pixel dimensions to the server so it
calls `SetResolutionConstraints` at the exact size. The app computes
`grid_width * cell_width` and `grid_height * cell_height` (already in physical
pixels thanks to DPI-scaled font metrics) and sends them alongside `create_tab`.
The IOSurface should then match the overlay quad 1:1 — no stretching, no blur.

### Idea 4: Dynamic resize

When the terminal resizes, `web` sends updated grid coordinates to the app. The
app recomputes the viewport's physical pixel dimensions and sends a `resize`
message to the server. The server updates `SetResolutionConstraints` and starts
capturing at the new size. New IOSurfaces arrive at the new dimensions. The
overlay pipeline already handles dimension changes (Issue 508 proved this with
the checkerboard).

### Idea 5: Two panes — same profile

Run two `web` commands in different panes, both using the default profile. Each
points at a different URL (or the same URL). Both should render simultaneously
at 60fps. This proves the per-pane tracking in CompositorXPC works with real
Chromium frames — each pane has its own server process, its own IOSurface
stream, and its own overlay quad. Both servers share the same `--user-data-dir`
(default profile), so cookies and localStorage are shared between them.

### Idea 6: Two panes — two profiles

Run two `web` commands in different panes with different `--profile` flags
(e.g., `--profile work` and `--profile personal`). Each profile gets its own
Chromium Profile Server process with its own `--user-data-dir`
(`~/.config/termsurf/profiles/work/` and
`~/.config/termsurf/profiles/personal/`). Both render simultaneously at 60fps
with fully isolated browser state — different cookies, different localStorage,
different cache. This is the core TermSurf differentiator: side-by-side browsing
with different identities in the same terminal window.

### Idea 7: Three panes — two profiles

Run three `web` commands: two panes share the same profile (e.g.,
`--profile
work`) and the third uses a different profile (e.g.,
`--profile personal`). This tests the one-server-per-profile architecture — the
two `work` panes should share a single Chromium Profile Server process (with two
tabs/WebContents inside it), while the `personal` pane gets its own server. This
is the ts3 foundational constraint (one CEF process per profile) applied to
ts5's Content API approach, and validates the multi-tab protocol from Issue 503.

## Experiment 1: End-to-end streaming

### Goal

Box-demo renders at 60fps in a TermSurf pane. This is Idea 1 — reimplementing
the three Chromium-specific pieces (server lifecycle, gateway connect, URL in
`set_overlay`) on top of the Issue 508 overlay infrastructure.

### Key insight

The Chromium Profile Server code on branch `146.0.7650.0-issue-507` already
implements the full two-step gateway connect, `server_register`, `create_tab`,
`tab_ready`, and `display_surface` protocol. This is identical to the
`146.0.7650.0-issue-503` branch (zero diff between them). **No Chromium source
code changes are needed.** Only a build.

The Issue 507 crash was caused by bytesPerRow misalignment and IOSurface
use-after-free — both fixed in Issue 508. The overlay pipeline
(`Texture.fromIOSurface`, CFRetain/CFRelease, `draw_mutex`) is already working
and proven stable with the checkerboard.

### Changes needed

Three files change in the TermSurf repo. Zero files change in Chromium.

#### 1. `web/src/xpc.rs` — Add URL to `send_set_overlay`

Add a `url: &str` parameter and include it in the XPC dictionary.

Current signature (line 169):

```rust
pub fn send_set_overlay(&self, pane_id: &str, col: u16, row: u16, width: u16, height: u16)
```

New signature:

```rust
pub fn send_set_overlay(&self, pane_id: &str, col: u16, row: u16, width: u16, height: u16, url: &str)
```

Add after the existing `xpc_dictionary_set_uint64` calls (line 192):

```rust
let url_key = CString::new("url").unwrap();
let url_c = CString::new(url).unwrap();
xpc_dictionary_set_string(dict, url_key.as_ptr(), url_c.as_ptr());
```

#### 2. `web/src/main.rs` — Pass URL to `send_set_overlay`

The call site at line 82 currently passes five arguments. Add `&url` as the
sixth:

```rust
conn.send_set_overlay(
    pid,
    viewport_rect.x,
    viewport_rect.y,
    viewport_rect.width,
    viewport_rect.height,
    &url,
);
```

#### 3. `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — Server lifecycle

This is the bulk of the work. The current `set_overlay` handler (lines 133–224)
creates a static checkerboard. Replace it with live Chromium server management.

**New state** (add after `currentSurfaces` at line 33):

```swift
/// Maps pane UUID → Chromium Profile Server process.
private var serverProcesses: [UUID: Process] = [:]

/// Maps pane UUID → server control connection (for sending create_tab).
private var serverControlConnections: [UUID: xpc_connection_t] = [:]

/// Maps pane UUID → URL to load (stored until server registers).
private var pendingURLs: [UUID: String] = [:]

/// Maps pane UUID → C surface pointer (cached for display_surface handler).
private var cachedCSurfaces: [UUID: ghostty_surface_t] = [:]
```

**Modified `set_overlay` handler:**

When `set_overlay` arrives with a `url` field:

1. Store the URL in `pendingURLs[uuid]`.
2. Set overlay grid coordinates (same as now — `ghostty_surface_set_overlay`).
3. Cache the `ghostty_surface_t` pointer in `cachedCSurfaces[uuid]`.
4. Spawn a Chromium Profile Server process.

The server path is the built binary:
`chromium/src/out/Default/chromium_profile_server`

Process arguments (all `=`-joined per Issue 507's lesson):

```
--xpc-service=com.termsurf.xpc-gateway
--pane-id=<uuid>
--user-data-dir=<profile-path>
--hidden
```

The `--user-data-dir` for the default profile is
`~/.config/termsurf/profiles/default/`.

If `set_overlay` arrives WITHOUT a `url` field, fall back to the existing
checkerboard behavior. This preserves Issue 508's test path.

**New `server_register` handler:**

When the server connects back to the app through the gateway and sends
`server_register`:

1. Extract `pane_id` from the message.
2. Store the peer connection as the control connection:
   `serverControlConnections[uuid] = peer`.
3. Look up `pendingURLs[uuid]` to get the URL.
4. Generate a `tab_id` (new UUID string).
5. Send `create_tab` on the control connection:
   ```
   { action: "create_tab", url: "<url>", tab_id: "<tab_id>" }
   ```
6. Remove the URL from `pendingURLs`.

**New `tab_ready` handler:**

When the server creates a new tab, it opens a separate per-tab connection to the
app and sends `tab_ready` with the `tab_id`. Nothing to do here except log it —
the `display_surface` messages will arrive on this same connection.

**New `display_surface` handler:**

This arrives at 60fps on the per-tab connection. For each frame:

1. Extract `pane_id` from the message.
2. Extract the Mach send right:
   `xpc_dictionary_copy_mach_send(msg, "iosurface_port")`.
3. Import the IOSurface: `IOSurfaceLookupFromMachPort(port)`.
4. Deallocate the Mach port: `mach_port_deallocate(mach_task_self_, port)`.
5. Store in `currentSurfaces[uuid]` (ARC retains new, releases old).
6. Get the raw pointer: `Unmanaged.passUnretained(surface).toOpaque()`.
7. Call `ghostty_surface_set_overlay_iosurface(cSurface, ptr)` using the cached
   C surface pointer from `cachedCSurfaces[uuid]`.

This runs on the XPC serial queue — no dispatch to main needed.
`ghostty_surface_set_overlay_iosurface` is thread-safe (protected by
`draw_mutex` on the Zig side). `currentSurfaces` is only accessed from the XPC
queue.

**Modified `handleDisconnect`:**

When a `web` peer disconnects (the existing handler at line 227):

1. Look up the pane UUID (existing behavior).
2. Kill the server process: `serverProcesses[uuid]?.terminate()`.
3. Clean up all state: remove from `serverProcesses`,
   `serverControlConnections`, `pendingURLs`, `cachedCSurfaces`,
   `currentSurfaces`.
4. Clear the overlay (existing behavior — `ghostty_surface_clear_overlay`).

When a server peer disconnects (control or tab connection): log it. The server
may crash or be killed — the overlay should remain until the `web` process also
disconnects.

#### 4. Chromium branch

Create `146.0.7650.0-issue-509` from `146.0.7650.0-issue-507`. No source code
changes — the branch exists for traceability. Build `chromium_profile_server` on
the new branch.

Update `docs/chromium.md` to add the new branch to the Branches table.

### XPC message flow

```
web ──set_overlay(coords+url)──▶ App
                                  │
                                  ├─ stores coords + URL
                                  ├─ spawns server --pane-id=UUID
                                  │
Server ──(gateway connect)──▶ App
Server ──server_register──▶ App
                                  │
                           App ──create_tab(url)──▶ Server
                                  │
Server ──(new connection)──▶ App
Server ──tab_ready──▶ App
                                  │
Server ──display_surface(60fps)──▶ App
                                  │  ├─ IOSurfaceLookupFromMachPort
                                  │  ├─ currentSurfaces[uuid] = surface
                                  │  └─ ghostty_surface_set_overlay_iosurface
                                  │
web disconnects ──▶ App
                     ├─ server.terminate()
                     ├─ clean up all state
                     └─ ghostty_surface_clear_overlay
```

### Build

```bash
# 1. Create issue-509 branch and build Chromium Profile Server
cd chromium/src
git checkout -b 146.0.7650.0-issue-509 146.0.7650.0-issue-507
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# 2. Build TermSurf (after CompositorXPC.swift changes)
cd ts5 && zig build

# 3. Build web (after xpc.rs + main.rs changes)
cargo build -p web
```

### Verification

```bash
# Start the box-demo server
cd ts4/box-demo && bun run server.ts &

# Launch TermSurf with stderr logging
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log

# In a TermSurf pane:
cargo run -p web -- http://localhost:9407

# Expected log output:
# [Compositor] set_overlay with URL http://localhost:9407
# [Compositor] Spawning server for pane <uuid>
# [Compositor] server_register from pane <uuid>
# [Compositor] Sending create_tab url=http://localhost:9407
# [Compositor] tab_ready for tab <tab_id>
# [Compositor] display_surface <width>x<height> for pane <uuid>
# (repeated at ~60fps)

# Expected visual:
# - Blue spinning square visible in the viewport area
# - FPS counter on the page
# - Stable rendering (no crash after 3 seconds — the Issue 508 fix)

# Quit:
# Press Esc then q (or Ctrl+C)
# Expected: overlay clears, server process terminates
```

### Pass criteria

1. Box-demo renders inside the viewport at any frame rate above 30fps.
2. No crash for at least 30 seconds (10 seconds would already exceed Issue 507's
   3-second crash window, but 30 seconds provides confidence).
3. Quitting `web` clears the overlay and kills the server process.
4. No orphaned `chromium_profile_server` processes after exit.

### File summary

| File                                            | Action                                             |
| ----------------------------------------------- | -------------------------------------------------- |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Server lifecycle + `display_surface` handler       |
| `web/src/xpc.rs`                                | Add `url` parameter to `send_set_overlay`          |
| `web/src/main.rs`                               | Pass `&url` to `send_set_overlay`                  |
| `docs/chromium.md`                              | Add `146.0.7650.0-issue-509` to Branches table     |
| Chromium source                                 | No changes (new branch from issue-507, build only) |

### Result

**Failed.** Pink overlay only — no Chromium frames rendered. Crash after
closing.

**Root cause:** `web` sends `set_overlay` on every draw cycle (every 250ms), and
`handleSetOverlay` spawns a new Chromium Profile Server on every call with no
guard. Dozens of server processes were spawned for a single pane.

**Why no blue spinning square:** All servers competed for the same LevelDB
profile directory (LOCK errors). Chromium sub-processes failed with
`bootstrap_look_up ... Permission denied (1100)` — the Mach port rendezvous
breaks when multiple servers collide. Multiple servers sent `server_register`,
but only the first found a `pendingURL` (the rest got "no pending URL" because
`removeValue` already consumed it). No server successfully streamed
`display_surface` frames while `web` was connected.

**Why crash:** `handleDisconnect` kills only `serverProcesses[uuid]` — the last
server stored. All previously spawned servers became orphans (each with renderer
and GPU sub-processes). Resource exhaustion from dozens of orphaned Chromium
process trees caused the crash.

**Fix needed (for Experiment 2):**

1. Guard `spawnServer` — skip if `serverProcesses[uuid]` already exists.
2. Optionally, only send `set_overlay` when the viewport rect changes.

---

## Experiment 2: Guard server spawn

### Goal

Same as Experiment 1 — box-demo renders at 60fps in a TermSurf pane. This
experiment fixes the server storm bug that caused Experiment 1 to fail.

### Root cause recap

`web` sends `set_overlay` every 250ms (on every draw cycle). `handleSetOverlay`
calls `spawnServer` unconditionally. Both sides need guards.

### Changes

Two files change. No Chromium changes. No build changes (same binaries).

#### 1. `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — Guard server spawn

In the URL branch of `handleSetOverlay`, skip everything after the first call.
The grid coordinates and server are already set — repeated `set_overlay`
messages for the same pane should be no-ops.

Replace the unconditional spawn with a guard:

```swift
// Check for URL field — if present, spawn Chromium server.
let urlPtr = xpc_dictionary_get_string(msg, "url")
if let urlPtr = urlPtr {
    let url = String(cString: urlPtr)

    // Skip if server already running for this pane.
    if serverProcesses[uuid] != nil {
        // Update grid coordinates only (server already running).
        if let cSurface = cachedCSurfaces[uuid] {
            ghostty_surface_set_overlay(cSurface, col, row, width, height)
        }
        return
    }

    fputs("[Compositor] set_overlay with URL \(url) for pane \(paneIdStr)\n", stderr)
    // ... rest of spawn logic unchanged ...
```

When `serverProcesses[uuid]` already exists, the handler updates the grid
coordinates (in case the viewport moved) and returns. No second server spawned.

#### 2. `web/src/main.rs` — Only send when viewport changes

Track the previous viewport rect. Skip `send_set_overlay` if the rect hasn't
changed since the last send.

Before the event loop, add:

```rust
let mut last_viewport = Rect::default();
```

Replace the unconditional send with:

```rust
// Send overlay coordinates to compositor (only when changed).
if viewport_rect != last_viewport {
    if let (Some(ref conn), Some(ref pid)) = (&compositor, &pane_id) {
        conn.send_set_overlay(
            pid,
            viewport_rect.x,
            viewport_rect.y,
            viewport_rect.width,
            viewport_rect.height,
            &url,
        );
    }
    last_viewport = viewport_rect;
}
```

This reduces XPC traffic from 4 messages/second to only on resize. The server
guard in Swift is the essential fix (prevents the storm even if `web` sends
duplicates), but the client-side dedup is good hygiene.

### Why both fixes

The Swift guard is sufficient on its own — even if `web` floods `set_overlay`,
only one server spawns. But sending identical messages every 250ms wastes XPC
bandwidth and CPU. The client-side dedup is cheap (one `Rect` comparison) and
eliminates the noise. Either fix alone prevents the server storm; both together
are cleaner.

### Pass criteria

Same as Experiment 1:

1. Box-demo renders inside the viewport at any frame rate above 30fps.
2. No crash for at least 30 seconds.
3. Quitting `web` clears the overlay and kills the server process.
4. No orphaned `chromium_profile_server` processes after exit.
5. Logs show exactly ONE `Spawned server` message per pane (not dozens).

### File summary

| File                                            | Action                                |
| ----------------------------------------------- | ------------------------------------- |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Guard `spawnServer` with exists check |
| `web/src/main.rs`                               | Only send `set_overlay` when changed  |

### Result

**Passed.** Box-demo rendered live Chromium frames inside the TermSurf pane.
Exactly one server spawned per pane. No crash. Clean exit killed the server and
cleared the overlay. Resize not tested (out of scope for this experiment).

Two visual issues remain: incorrect resolution (expected — server captures at a
fixed size, stretched to fit) and incorrect colors (darks too dark, blues too
blue — sRGB mismatch, see Experiment 3).

---

## Experiment 3: Correct overlay colors

### Goal

Fix the "too bold" colors in the Chromium overlay. Darks should match the
original webpage, not appear crushed. Blues should match the original, not
appear oversaturated.

### Root cause

`Texture.fromIOSurface()` in `ts5/src/renderer/metal/Texture.zig` line 100
creates the MTLTexture with `bgra8unorm_srgb`. This tells Metal to apply
sRGB→linear decoding on texture read. But Ghostty's render target (the
CAMetalLayer drawable) uses `bgra8unorm` (non-sRGB). The decoded linear values
are written directly to the non-sRGB target without a linear→sRGB re-encode,
making darks darker and colors more saturated.

The same approach was used in ts3's cef-rs (`iosurface.rs:186`) — non-sRGB
`BGRA8Unorm` for IOSurface-backed textures, letting the sRGB-encoded bytes pass
through to the display unchanged.

### Change

One file, one line.

#### `ts5/src/renderer/metal/Texture.zig` line 100

Change:

```zig
desc.setProperty("pixelFormat", @intFromEnum(mtl.MTLPixelFormat.bgra8unorm_srgb));
```

To:

```zig
desc.setProperty("pixelFormat", @intFromEnum(mtl.MTLPixelFormat.bgra8unorm));
```

The sRGB-encoded bytes from Chromium's IOSurface pass through the shader
unchanged and are written to the non-sRGB render target. The display's inherent
sRGB curve renders them correctly.

### Pass criteria

1. Box-demo colors match the original webpage when viewed in a regular browser.
2. No visual regression on the checkerboard test path (Issue 508).
3. No crash.

### File summary

| File                                 | Action                           |
| ------------------------------------ | -------------------------------- |
| `ts5/src/renderer/metal/Texture.zig` | `bgra8unorm_srgb` → `bgra8unorm` |

### Result

**Passed.** Colors now match the original webpage. The sRGB-encoded bytes from
Chromium pass through the shader unchanged to the non-sRGB render target, and
the display's inherent sRGB curve renders them correctly. Same approach as ts3's
cef-rs fix.

---

## Experiment 4: Retina resolution + dynamic resize

### Goal

The Chromium overlay renders at the exact physical pixel dimensions of the
viewport — no stretching, no blur. When the terminal resizes, the server updates
its capture resolution and new frames arrive at the new size.

### Problem

The Chromium Profile Server calls `SetResolutionConstraints` using
`view->GetVisibleViewportSize()` from the Shell's RenderWidgetHostView. Since
the Shell window is hidden (`--hidden`), this returns a default size unrelated
to the actual viewport. The overlay stretches the small capture to fill the
viewport, producing visible blur.

### Solution

Two changes:

1. **Tell Chromium the correct size.** The app knows the correct pixel
   dimensions: `grid_width * cellWidth` and `grid_height * cellHeight` (from
   `ghostty_surface_get_cell_size`, already Retina-aware). Pass these to the
   server so it captures at the right size. On resize, `web` already sends
   `set_overlay` with updated grid coords (Experiment 2's dedup). The app
   computes new pixel dimensions and tells the server to update
   `SetResolutionConstraints`.

2. **Never stretch the texture.** The overlay quad must be sized to the
   IOSurface's exact pixel dimensions, not to `grid_width * cell_size`. During
   resize, the old frame (at the old size) renders at its actual pixel
   dimensions for ~16ms until the next frame arrives at the new size. This
   avoids stretching entirely — a slight size mismatch for one frame is
   imperceptible, while stretching-induced blur is immediately visible.

   The `PinkOverlay` struct gains `pixel_width` and `pixel_height` fields. The
   Zig renderer reads `tex.width` and `tex.height` from the IOSurface texture
   each frame and writes them into the params buffer. The `overlay_vertex`
   shader uses these pixel dimensions directly for the quad size (no
   multiplication by `cell_size`). The pink fallback shader is unchanged.

### XPC protocol (complete)

This is the full protocol after this experiment. New or modified fields are
marked with **(new)**.

#### `web` → app (direct connection)

**`set_overlay`** — sent once on start, then on every viewport change (resize):

```
{ action: "set_overlay",
  pane_id: "<uuid>",
  col: N,           // grid column (top-left)
  row: N,           // grid row (top-left)
  width: N,         // grid cells wide
  height: N,        // grid cells tall
  url: "http://..." }
```

No changes. The app computes pixel dimensions server-side using cell size.

#### app → server (control connection)

**`create_tab`** — sent after `server_register`, includes initial pixel size:

```
{ action: "create_tab",
  url: "http://...",
  tab_id: "<uuid>",
  pixel_width: N,   // (new) physical pixels wide
  pixel_height: N }  // (new) physical pixels tall
```

**`resize`** **(new)** — sent when viewport grid coords change after initial
setup:

```
{ action: "resize",
  pixel_width: N,   // new physical pixels wide
  pixel_height: N }  // new physical pixels tall
```

#### server → app (control connection)

**`server_register`** — unchanged:

```
{ action: "server_register",
  pane_id: "<uuid>" }
```

#### server → app (per-tab connection)

**`tab_ready`** — unchanged:

```
{ action: "tab_ready",
  tab_id: "<uuid>" }
```

**`display_surface`** — size fields removed (redundant, IOSurface is
self-describing via `IOSurfaceGetWidth`/`IOSurfaceGetHeight`):

```
{ action: "display_surface",
  pane_id: "<uuid>",
  iosurface_port: <mach_send_right> }
```

### Changes

Seven files in the TermSurf repo, three files in the Chromium fork.

#### 1. `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

**In `handleSetOverlay` (first-time path, before `spawnServer`):**

Compute pixel dimensions using `ghostty_surface_get_cell_size` and store them
for inclusion in `create_tab`:

```swift
var cellWidth: UInt32 = 0
var cellHeight: UInt32 = 0
ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
let pixelWidth = UInt64(width) * UInt64(cellWidth)
let pixelHeight = UInt64(height) * UInt64(cellHeight)
pendingPixelSizes[uuid] = (pixelWidth, pixelHeight)
```

New state: `private var pendingPixelSizes: [UUID: (UInt64, UInt64)] = [:]`

**In `handleServerRegister` (when sending `create_tab`):**

Include `pixel_width` and `pixel_height` from `pendingPixelSizes[uuid]`.

**In `handleSetOverlay` (server-already-running path):**

Compute new pixel dimensions and send `resize` to the server's control
connection:

```swift
if serverProcesses[uuid] != nil {
    if let cSurface = cachedCSurfaces[uuid] {
        ghostty_surface_set_overlay(cSurface, col, row, width, height)

        // Send resize to server.
        var cellWidth: UInt32 = 0
        var cellHeight: UInt32 = 0
        ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)
        let pixelWidth = UInt64(width) * UInt64(cellWidth)
        let pixelHeight = UInt64(height) * UInt64(cellHeight)

        if let controlConn = serverControlConnections[uuid] {
            let msg = xpc_dictionary_create(nil, nil, 0)
            xpc_dictionary_set_string(msg, "action", "resize")
            xpc_dictionary_set_uint64(msg, "pixel_width", pixelWidth)
            xpc_dictionary_set_uint64(msg, "pixel_height", pixelHeight)
            xpc_connection_send_message(controlConn, msg)
        }
    }
    return
}
```

#### 2. `ts5/src/renderer/metal/shaders.zig` — Add pixel dimension fields

Add `pixel_width` and `pixel_height` to the `PinkOverlay` struct:

```zig
pub const PinkOverlay = extern struct {
    grid_col: f32 = 0,
    grid_row: f32 = 0,
    grid_width: f32 = 0,
    grid_height: f32 = 0,
    pixel_width: f32 = 0,
    pixel_height: f32 = 0,
};
```

#### 3. `ts5/src/renderer/shaders/shaders.metal` — Match struct + use pixel dims

Update `PinkOverlayIn` to match the Zig struct:

```metal
struct PinkOverlayIn {
  float grid_col;
  float grid_row;
  float grid_width;
  float grid_height;
  float pixel_width;
  float pixel_height;
};
```

Update `overlay_vertex` to use pixel dimensions directly for quad size:

```metal
vertex OverlayVertexOut overlay_vertex(
  uint vid [[vertex_id]],
  constant PinkOverlayIn& params [[buffer(0)]],
  constant Uniforms& uniforms [[buffer(1)]]
) {
  float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
  float2 size = float2(params.pixel_width, params.pixel_height);
  // ...rest unchanged...
}
```

The origin stays grid-based (correct positioning). The size is pixel-exact from
the IOSurface. The `pink_overlay_vertex` shader is unchanged (still uses
`grid_width * cell_size` for the solid color fallback).

#### 4. `ts5/src/renderer/generic.zig` — Write pixel dims from texture

Restructure the overlay draw code. For the IOSurface path, create the texture
first, read its dimensions, then create the params buffer with pixel dimensions
filled in:

```zig
if (self.overlay_iosurface) |iosurface| {
    const tex_result = Texture.fromIOSurface(self.api.device, iosurface);
    if (tex_result) |tex| {
        defer tex.deinit();
        var overlay_params = self.pink_overlay;
        overlay_params.pixel_width = @floatFromInt(tex.width);
        overlay_params.pixel_height = @floatFromInt(tex.height);
        if (Buffer(shaderpkg.PinkOverlay).initFill(
            self.api.imageBufferOptions(),
            &.{overlay_params},
        )) |*buf| {
            defer buf.deinit();
            pass.step(.{
                .pipeline = self.shaders.pipelines.overlay,
                .uniforms = frame.uniforms.buffer,
                .buffers = &.{buf.buffer},
                .textures = &.{tex},
                .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
            });
        }
    }
} else {
    // Pink fallback — pixel_width/height stay 0, shader uses grid coords.
    if (Buffer(shaderpkg.PinkOverlay).initFill(
        self.api.imageBufferOptions(),
        &.{self.pink_overlay},
    )) |*buf| {
        defer buf.deinit();
        pass.step(.{
            .pipeline = self.shaders.pipelines.pink_overlay,
            .uniforms = frame.uniforms.buffer,
            .buffers = &.{buf.buffer},
            .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
        });
    }
}
```

The IOSurface path creates the texture first to read its dimensions, then writes
`pixel_width`/`pixel_height` into a local copy of the params. The pink fallback
path is unchanged — `pixel_width`/`pixel_height` default to 0 and the
`pink_overlay_vertex` shader ignores them.

#### 5. `chromium/.../shell_browser_main_parts.cc`

**Update `CreateTab` to accept pixel dimensions and resize the view:**

```cpp
void ShellBrowserMainParts::CreateTab(const GURL& url,
                                      const std::string& tab_id,
                                      int pixel_width,
                                      int pixel_height) {
  // ... existing Shell creation ...

  // Resize WebContents view to match the terminal viewport.
  // SetSize takes logical pixels; Chromium applies device_scale_factor internally.
  // We send physical pixels over XPC; derive logical here.
  if (pixel_width > 0 && pixel_height > 0) {
    RenderWidgetHostView* view = shell->web_contents()->GetRenderWidgetHostView();
    if (view) {
      float scale = view->GetDeviceScaleFactor();
      gfx::Size logical(static_cast<int>(std::ceil(pixel_width / scale)),
                         static_cast<int>(std::ceil(pixel_height / scale)));
      view->SetSize(logical);
    }
  }

  video_consumer->SetInitialSize(pixel_width, pixel_height);
  // ... rest unchanged ...
}
```

`SetSize` takes logical (CSS) pixels. The view computes
`logical × device_scale_factor` to get the physical compositor size, which
matches our capture resolution. CSS layout, media queries, and responsive
breakpoints all see the correct viewport width.

**Update control connection handler to extract pixel dimensions from
`create_tab`:**

```cpp
if (action && std::string_view(action) == "create_tab") {
    // ... existing url/tab_id extraction ...
    int pw = (int)xpc_dictionary_get_int64(event, "pixel_width");
    int ph = (int)xpc_dictionary_get_int64(event, "pixel_height");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::CreateTab,
                       base::Unretained(self), GURL(url), tab_id, pw, ph));
}
```

**Add `resize` handler in control connection event handler:**

```cpp
} else if (action && std::string_view(action) == "resize") {
    int pw = (int)xpc_dictionary_get_int64(event, "pixel_width");
    int ph = (int)xpc_dictionary_get_int64(event, "pixel_height");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::ResizeCapture,
                       base::Unretained(self), pw, ph));
}
```

**Add `ResizeCapture` method — resizes both view and capturer:**

```cpp
void ShellBrowserMainParts::ResizeCapture(int pixel_width, int pixel_height) {
  if (tabs_.empty() || pixel_width <= 0 || pixel_height <= 0)
    return;

  auto& tab = tabs_[0];

  // Resize the WebContents view (logical pixels).
  RenderWidgetHostView* view =
      tab->shell->web_contents()->GetRenderWidgetHostView();
  if (view) {
    float scale = view->GetDeviceScaleFactor();
    gfx::Size logical(static_cast<int>(std::ceil(pixel_width / scale)),
                       static_cast<int>(std::ceil(pixel_height / scale)));
    view->SetSize(logical);
  }

  // Resize the capturer (physical pixels).
  tab->video_consumer->SetResolution(pixel_width, pixel_height);
}
```

(Single-tab for now — `tabs_[0]` is sufficient for the default-profile case.)

#### 6. `chromium/.../shell_browser_main_parts.h`

Update `CreateTab` signature and add `ResizeCapture`:

```cpp
void CreateTab(const GURL& url, const std::string& tab_id,
               int pixel_width, int pixel_height);
void ResizeCapture(int pixel_width, int pixel_height);
```

#### 7. `chromium/.../shell_video_consumer.cc`

**Remove redundant `width`/`height` from `display_surface` message** in
`OnFrameCaptured`. The IOSurface is self-describing — the app reads dimensions
via `IOSurfaceGetWidth`/`IOSurfaceGetHeight` on the imported surface. Remove
these two lines:

```cpp
// DELETE:
xpc_dictionary_set_int64(msg, "width", (int64_t)width);
xpc_dictionary_set_int64(msg, "height", (int64_t)height);
```

**Add `SetInitialSize` — stores dimensions for use in `Attach`:**

```cpp
void ShellVideoConsumer::SetInitialSize(int width, int height) {
  initial_width_ = width;
  initial_height_ = height;
}
```

**Modify `Attach` — use stored dimensions instead of `GetVisibleViewportSize`:**

```cpp
gfx::Size physical_size;
if (initial_width_ > 0 && initial_height_ > 0) {
  physical_size = gfx::Size(initial_width_, initial_height_);
} else {
  gfx::Size view_size = view->GetVisibleViewportSize();
  float scale = view->GetDeviceScaleFactor();
  physical_size = gfx::Size(
      static_cast<int>(std::ceil(view_size.width() * scale)),
      static_cast<int>(std::ceil(view_size.height() * scale)));
}
capturer_->SetResolutionConstraints(physical_size, physical_size, false);
```

**Add `SetResolution` — updates capture resolution on a running capturer:**

```cpp
void ShellVideoConsumer::SetResolution(int width, int height) {
  if (capturer_ && width > 0 && height > 0) {
    gfx::Size size(width, height);
    capturer_->SetResolutionConstraints(size, size, false);
    LOG(INFO) << "[ShellVideoConsumer] Resized to " << width << "x" << height;
  }
}
```

#### 8. `chromium/.../shell_video_consumer.h`

Add declarations:

```cpp
void SetInitialSize(int width, int height);
void SetResolution(int width, int height);
```

Add members:

```cpp
int initial_width_ = 0;
int initial_height_ = 0;
```

### Message flow

```
Terminal resize
    │
    ▼
web detects viewport change (last_viewport != viewport_rect)
    │
    ▼
web ──set_overlay(new grid coords + url)──▶ App
                                             │
                                             ├─ serverProcesses[uuid] exists
                                             ├─ ghostty_surface_set_overlay(new coords)
                                             ├─ ghostty_surface_get_cell_size → cellW, cellH
                                             ├─ pixelW = gridW * cellW
                                             ├─ pixelH = gridH * cellH
                                             │
                                      App ──resize(pixelW, pixelH)──▶ Server
                                                                       │
                                                                       ├─ SetResolutionConstraints(new size)
                                                                       │
                                                            Next frames arrive at new size
                                                                       │
                                              Server ──display_surface──▶ App
                                                                          │
                                                                          └─ overlay renders at correct resolution
```

### Build

```bash
# 1. Build Chromium Profile Server (source changes)
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# 2. Build TermSurf (CompositorXPC.swift changes)
cd ts5 && zig build

# No web changes needed — existing set_overlay dedup handles resize.
```

### Pass criteria

1. On launch, the overlay renders at the viewport's exact physical pixel
   dimensions — no blur, no stretching. Text on the webpage is crisp at Retina
   resolution.
2. On terminal resize, the overlay updates to the new resolution within ~1
   second. During the transition, the old frame renders at its actual pixel
   dimensions (no stretching) — slightly wrong size for ~16ms, then corrected.
3. No crash during or after resize.
4. The texture is **never** stretched. The overlay quad always matches the
   IOSurface's exact pixel dimensions.

### File summary

| File                                            | Action                                                |
| ----------------------------------------------- | ----------------------------------------------------- |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Compute pixel dims, send in `create_tab` and `resize` |
| `ts5/src/renderer/metal/shaders.zig`            | Add `pixel_width`/`pixel_height` to `PinkOverlay`     |
| `ts5/src/renderer/shaders/shaders.metal`        | Match struct, use pixel dims in `overlay_vertex`      |
| `ts5/src/renderer/generic.zig`                  | Write `tex.width`/`tex.height` into params buffer     |
| `chromium/.../shell_browser_main_parts.cc`      | Extract pixel dims, handle `resize`, pass to consumer |
| `chromium/.../shell_browser_main_parts.h`       | Update `CreateTab` signature, add `ResizeCapture`     |
| `chromium/.../shell_video_consumer.cc`          | `SetInitialSize`, `SetResolution` methods             |
| `chromium/.../shell_video_consumer.h`           | Add declarations and members                          |

### Result

**Pass.** The overlay renders at the viewport's exact physical pixel dimensions
on launch — text is crisp at Retina resolution with no blur or stretching. On
terminal resize, the overlay updates to the new resolution smoothly. The texture
is never stretched; during the ~16ms transition the old frame renders at its
actual pixel dimensions, then the next frame arrives at the correct new size.

## Conclusion

### How we got here

Issue 507 proved the full Chromium pipeline works end-to-end at 60fps but
crashed after 3 seconds. Issue 508 fixed the crash (IOSurface `bytesPerRow`
alignment + CFRetain/CFRelease lifetime management). This issue picked up where
508 left off: reimplement end-to-end streaming on top of 508's stable
infrastructure.

Experiment 1 failed immediately — a server storm bug spawned a new Chromium
process on every `set_overlay` call (every 250ms), flooding the system with
dozens of competing servers. Experiment 2 fixed this with two guards: skip
server spawn if one already exists, and only send `set_overlay` when the
viewport actually changes. Live Chromium frames rendered stably.

With streaming working, two visual issues became apparent. Experiment 3 fixed
colors — the IOSurface texture was declared `bgra8unorm_srgb` but Ghostty's
render target is non-sRGB, causing a decode/encode mismatch that made colors
"too bold." One-line fix: `bgra8unorm`. Experiment 4 fixed resolution and added
dynamic resize — the server had been capturing at a default hidden-window size,
producing blurry stretched output. The app now computes physical pixel
dimensions from grid coords × cell size, sends them to the server at startup and
on every resize, and the overlay quad is always sized to the IOSurface's exact
pixel dimensions (never stretched).

### What we accomplished

The original scope was just "retry end-to-end streaming" — get back to Issue
507's state without the crash. We exceeded that significantly:

| Experiment | Goal                       | Result |
| ---------- | -------------------------- | ------ |
| 1          | End-to-end streaming       | Fail   |
| 2          | Fix server storm           | Pass   |
| 3          | Fix sRGB color mismatch    | Pass   |
| 4          | Retina resolution + resize | Pass   |

The pipeline now delivers:

- **Correct colors.** Chromium's sRGB output composites correctly into Ghostty's
  non-sRGB render target.
- **Pixel-perfect Retina resolution.** Text on webpages is crisp — no blur, no
  scaling artifacts.
- **Dynamic resize.** Terminal resize triggers a new capture resolution within
  ~16ms. The texture is never stretched during the transition.
- **Clean lifecycle.** Server spawns once, resizes on demand, terminates cleanly
  on disconnect.

### What's next

The streaming pipeline is solid. The remaining work to make browser panes
usable:

- **Input forwarding.** Keyboard and mouse events need to flow from the terminal
  pane to the Chromium WebContents. Without this, the webpage is view-only.
- **Scroll support.** Scroll events from the terminal need to reach the page.
- **Multiple tabs/profiles.** The current implementation is single-tab
  (`tabs_[0]`). Supporting multiple browser panes requires routing by tab ID.
- **In-process Chromium.** The current architecture runs Chromium out-of-process
  via the Profile Server. The ts5 endgame is in-process Chromium via the Content
  API (proven in ts4). The XPC streaming approach is a stepping stone — it
  validates the rendering pipeline while the in-process embedding is developed.
