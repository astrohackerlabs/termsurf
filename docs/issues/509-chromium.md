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
  iosurface_port: <mach_send_right>,
  width: N, height: N }
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

_Not yet run._

---

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
