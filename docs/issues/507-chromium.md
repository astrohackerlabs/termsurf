# Issue 507: Chromium Integration

## Background

This is the culmination of five generations of work. Every previous issue built
a piece of the puzzle:

- **ts1-ts3** proved IOSurface Mach port transfer via XPC works at 60fps, but
  CEF's headless off-screen rendering caps at ~31fps on macOS (26 experiments,
  Issues 325-350).
- **ts4** proved the Content API eliminates CEF's framerate ceiling. Multiple
  WebContents within one `BrowserContext` render at 60fps (Issue 413). Multiple
  profiles in one process causes 2fps (Issue 413, Experiment 4), so each profile
  gets its own process (Issue 414).
- **Issue 414** proved two profile processes sending IOSurface Mach ports to a
  receiver at 60fps. Reimplemented in Swift (Issue 415) and Rust (Issue 416).
- **Issue 501** renamed the embedder to Chromium Profile Server and proved
  headless rendering with no Dock icon.
- **Issue 502** replaced the hardcoded 2-second capturer attach delay with
  event-driven `RenderViewReady()` (67% faster startup).
- **Issue 503** proved dynamic multi-tab protocol: the compositor sends
  `create_tab` commands to the profile server, each tab gets its own XPC
  connection and `FrameSinkVideoCapturer`.
- **Issue 504** built the `web` TUI chrome (URL bar, viewport border, status
  bar) and sends viewport grid coordinates to the app via XPC.
- **Issue 505** proved GPU overlay compositing: a pink quad renders at exact
  grid coordinates inside a Ghostty pane, driven by XPC messages from `web`.
- **Issue 506 (xpc-gateway)** freed the app from launchd constraints. The
  xpc-gateway daemon owns the Mach service; the app launches normally via
  `open`.

Every piece is proven. The Chromium Profile Server renders to IOSurface at
60fps. The Metal renderer composites overlays at grid coordinates. The
xpc-gateway provides rendezvous. The `web` TUI manages browser chrome. Now we
connect them.

## Goal

`cargo run -p web -- http://localhost:9407` renders the box-demo (blue spinning
square with FPS counter) inside a TermSurf pane at 60fps, full Retina
resolution, composited by the Metal renderer at the exact grid coordinates of
the viewport.

The box-demo (`ts4/box-demo/`) is a Canvas 2D page with a rotating blue square,
localStorage identity string, and a built-in FPS counter — making 60fps
verification trivial. Served by Bun on port 9407.

Single default profile only. No multiple profiles for this issue.

## Architecture

### Process Topology

```
                      xpc-gateway
                  (com.termsurf.xpc-gateway)
                   /                    \
                  /                      \
TermSurf app ───┘                        └─── Chromium Profile Server
(registers endpoint)                      (claims endpoint,
                                           connects directly)
       ▲                                          │
       │ direct XPC                               │ direct XPC
       │ (set_overlay)                            │ (display_surface)
       │                                          │
     web TUI ─────────spawns──────────────────────┘
(browser chrome,
 viewport coords)
```

Four processes:

1. **xpc-gateway** — Owns the Mach service. Pure rendezvous. Already built
   (Issue 506).

2. **TermSurf app** — Registers anonymous listener with gateway. Receives
   `set_overlay` from `web` (grid coordinates) and `display_surface` from
   Chromium Profile Server (IOSurface Mach ports). Composites the IOSurface
   texture at the grid coordinates using Metal.

3. **`web` TUI** — Draws browser chrome. Sends viewport coordinates to app.
   Spawns the Chromium Profile Server as a child process. Kills it on exit.

4. **Chromium Profile Server** — Renders the webpage. Connects to app via
   gateway. Sends IOSurface Mach ports at 60fps on the direct connection.

### Connection Flow

```
1. App starts:     registers endpoint with gateway

2. web starts:     connects to gateway, gets endpoint, connects to app
                   sends set_overlay (grid coords) on direct connection
                   spawns Chromium Profile Server

3. Server starts:  connects to gateway, gets endpoint, connects to app
                   navigates to URL
                   sends display_surface (IOSurface Mach ports) at 60fps

4. App renders:    imports IOSurface from Mach port
                   creates MTLTexture
                   renders textured quad at grid coordinates

5. web exits:      kills server, drops connection
                   app clears overlay
```

### IOSurface Frame Delivery

The Chromium Profile Server already captures frames via
`viz::ClientFrameSinkVideoCapturer` (Issue 414). On each captured frame:

1. `OnFrameCaptured()` receives a `gpu_memory_buffer_handle` containing an
   `IOSurfaceRef`.
2. `IOSurfaceCreateMachPort(io_surface)` creates a Mach port handle.
3. The port is sent via XPC:
   `xpc_dictionary_set_mach_send(msg, "iosurface_port",
   port)`.
4. The app imports it: `IOSurfaceLookupFromMachPort(port)`.
5. The renderer creates a `MTLTexture` from the IOSurface for the current frame.

This is the same pipeline proven in Issues 414-416 at 60fps. The only new part
is connecting it to the ts5 Metal renderer instead of a standalone receiver.

### Retina Handling

The `FrameSinkVideoCapturer` already captures at physical pixel resolution
(Issue 502, `shell_video_consumer.cc` line 72-80):

```cpp
float scale = view->GetDeviceScaleFactor();
gfx::Size physical_size(
    static_cast<int>(std::ceil(view_size.width() * scale)),
    static_cast<int>(std::ceil(view_size.height() * scale)));
capturer_->SetResolutionConstraints(physical_size, physical_size, false);
```

The challenge is matching the capture resolution to the overlay viewport size.
The overlay viewport is defined in grid cells. The physical pixel size is:

```
pixel_width  = grid_width  * cell_width  * scale_factor
pixel_height = grid_height * cell_height * scale_factor
```

The app knows `cell_width`, `cell_height`, and `scale_factor` from the renderer.
The Chromium Profile Server needs this information to set its capture
resolution. This can flow through `web` (which already bridges both) or via a
reply on the direct XPC connection.

## XPC Protocol

### Existing Messages (Unchanged)

**`web` to app (direct connection):**

```
{ action: "set_overlay", pane_id: "<uuid>",
  col: N, row: N, width: N, height: N }
```

### New Messages

**Chromium Profile Server to app (direct connection):**

```
{ action: "display_surface", pane_id: "<uuid>",
  iosurface_port: <mach_send_right>,
  width: N, height: N }
```

The app maps `pane_id` to the correct surface and updates the overlay texture.
`width` and `height` are the IOSurface physical pixel dimensions.

**`web` to Chromium Profile Server (command-line args, not XPC):**

```
Chromium\ Profile\ Server \
  --url http://localhost:9407 \
  --pane-id <uuid> \
  --xpc-service com.termsurf.xpc-gateway \
  --hidden \
  --user-data-dir ~/.config/termsurf/profiles/default \
  --content-shell-host-window-size 800x600
```

The `--content-shell-host-window-size` sets the initial WebContents size.
Approximate is fine for the first experiment; proper size matching comes later.

## Components

### 1. Metal Shader: IOSurface Texture Overlay

Replace the `pink_overlay` solid-color shader with a texture-sampling shader.
The vertex shader is identical (positions a quad at grid coordinates). The
fragment shader samples from an IOSurface-backed `MTLTexture`.

**Files:**

- `ts5/src/renderer/shaders/shaders.metal` — New `overlay_vertex` and
  `overlay_fragment` shaders.
- `ts5/src/renderer/metal/shaders.zig` — Replace `pink_overlay` pipeline with
  `overlay` pipeline (or rename).

**Vertex shader** — Same as `pink_overlay_vertex` but outputs texture
coordinates:

```metal
struct OverlayVertexOut {
    float4 position [[position]];
    float2 texcoord;
};

vertex OverlayVertexOut overlay_vertex(
    uint vid [[vertex_id]],
    constant OverlayIn &params [[buffer(0)]],
    constant Uniforms &uniforms [[buffer(1)]]
) {
    float2 cell_size = uniforms.cell_size;
    float2 padding = float2(uniforms.grid_padding[0], uniforms.grid_padding[2]);
    float2 origin = float2(params.grid_col, params.grid_row) * cell_size + padding;
    float2 size = float2(params.grid_width, params.grid_height) * cell_size;

    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

    OverlayVertexOut out;
    out.position = uniforms.projection_matrix * float4(origin + size * corner, 0.0, 1.0);
    out.texcoord = corner;
    return out;
}
```

**Fragment shader** — Samples the IOSurface texture:

```metal
fragment float4 overlay_fragment(
    OverlayVertexOut in [[stage_in]],
    texture2d<float> tex [[texture(0)]]
) {
    constexpr sampler s(mag_filter::linear, min_filter::linear);
    return tex.sample(s, in.texcoord);
}
```

### 2. Renderer State: IOSurface Storage

Add an IOSurface reference to the renderer state, protected by `draw_mutex`.

**Files:**

- `ts5/src/renderer/generic.zig` — Add `overlay_surface` field (IOSurfaceRef as
  opaque pointer) alongside the existing `pink_overlay` grid coordinates.
- `ts5/src/renderer/generic.zig` — In `drawFrame()`, when `overlay_surface` is
  non-null, create a `MTLTexture` from it and render with the texture shader
  instead of the pink shader.

The `MTLTexture` creation from IOSurface uses:

```objc
[device newTextureWithDescriptor:descriptor
                       iosurface:surface
                           plane:0]
```

In Zig, this is an Objective-C message send via the existing Metal interop
layer.

### 3. C API Bridge

**Files:**

- `ts5/include/ghostty.h` — Add
  `ghostty_surface_set_overlay_surface(surface, port, width, height)`.
- `ts5/src/apprt/embedded.zig` — Export the function.
- `ts5/src/Surface.zig` — Implement `setOverlaySurface()`: import IOSurface from
  Mach port, store on renderer, queue redraw.

The Mach port is received as `mach_port_t` (`u32` in Zig). The import uses
`IOSurfaceLookupFromMachPort()`. The renderer stores the resulting
`IOSurfaceRef`.

### 4. CompositorXPC.swift: Handle `display_surface`

**Files:**

- `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — Add `display_surface` case
  to `handleMessage()`.

When a `display_surface` message arrives:

1. Extract `pane_id` (UUID string) and look up the surface.
2. Extract `iosurface_port` via
   `xpc_dictionary_copy_mach_send(msg,
   "iosurface_port")`.
3. Extract `width` and `height`.
4. Call `ghostty_surface_set_overlay_surface(surface, port, width, height)`.

The Mach port is passed through to the C API. The Zig side imports the
IOSurface. This keeps Swift minimal — no IOSurface framework import needed.

### 5. Chromium Profile Server: Gateway Connect

**Files:**

- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — Modify XPC connection code for two-step gateway connect.
- `chromium/src/content/chromium_profile_server/common/shell_switches.h` — Add
  `--pane-id` flag.

Currently the server connects directly to a named Mach service:

```cpp
xpc_connection_create_mach_service(service_name, queue, 0);
```

Change to two-step connect (same pattern as `web/src/xpc.rs`):

1. Connect to `com.termsurf.xpc-gateway`.
2. Send `{ action: "connect" }` with
   `xpc_connection_send_message_with_reply_sync`.
3. Extract endpoint from reply.
4. Create connection from endpoint via `xpc_connection_create_from_endpoint`.
5. Send `display_surface` frames on the direct connection, including `pane_id`.

### 6. `web`: Spawn Chromium Profile Server

**Files:**

- `web/src/main.rs` — After connecting to the app and sending `set_overlay`,
  spawn the Chromium Profile Server as a child process.

```rust
let server_path = std::env::var("TERMSURF_CHROMIUM_PATH")
    .unwrap_or_else(|_| "chromium/src/out/Default/Chromium Profile Server.app/\
        Contents/MacOS/Chromium Profile Server".to_string());

let child = Command::new(&server_path)
    .args([
        "--url", &url,
        "--pane-id", &pane_id,
        "--xpc-service", "com.termsurf.xpc-gateway",
        "--hidden",
        "--user-data-dir",
        &format!("{}/.config/termsurf/profiles/default",
                 std::env::var("HOME").unwrap()),
        "--content-shell-host-window-size", "800x600",
    ])
    .spawn()
    .expect("Failed to spawn Chromium Profile Server");
```

On `web` exit (Ctrl+C, `q`, or signal), kill the child process. The XPC
connection drops, and the app clears the overlay.

### 7. Profile Data

Default profile storage: `~/.config/termsurf/profiles/default/`

This is the Chromium user data directory. It stores cookies, localStorage,
cache, and all other browser state. One directory per profile. Issue 507 uses
only the `default` profile.

## Experiments

### Experiment 1: IOSurface Texture Pipeline

Prove the Metal renderer can display an IOSurface texture at grid coordinates.
Use a programmatically created test IOSurface (solid color or gradient) instead
of live Chromium frames.

**Changes:**

1. Add `overlay_vertex` / `overlay_fragment` shaders to `shaders.metal`.
2. Replace `pink_overlay` pipeline with `overlay` pipeline in `shaders.zig`.
3. Add `overlay_surface` field to renderer state in `generic.zig`.
4. Add `ghostty_surface_set_overlay_surface(surface, port, width, height)` to C
   API.
5. In `CompositorXPC.swift`, create a test IOSurface (e.g., 100x100 solid blue),
   pass its Mach port through the C API.
6. Render the textured quad at the overlay grid coordinates.

**Pass criteria:**

1. A solid-colored rectangle (from the test IOSurface) appears at the viewport
   coordinates.
2. The rectangle resizes correctly when the terminal resizes.
3. Quitting `web` clears the overlay.
4. No flickering or tearing.

### Experiment 2: Live Chromium Frames

Connect the Chromium Profile Server and render live web content.

**Changes:**

1. Modify Chromium Profile Server for two-step gateway connect (Component 5).
2. Add `--pane-id` and `--url` flags to the server.
3. Add `display_surface` handler to `CompositorXPC.swift` (Component 4).
4. Modify `web` to spawn the server (Component 6).
5. The renderer uses the IOSurface from the server instead of the test surface.

**Pass criteria:**

1. `cargo run -p web -- http://localhost:9407` shows the box-demo in the
   viewport — blue square rotating on a dark background.
2. The page renders at 60fps (verify via the box-demo's built-in FPS counter and
   the server's fps logging).
3. Quitting `web` kills the server and clears the overlay.

### Experiment 3: Retina Resolution and Resize

Match the capture resolution to the viewport's physical pixel size.

**Changes:**

1. `web` queries the app for viewport pixel dimensions (via XPC reply to
   `set_overlay`, or a separate query). Or the app computes and tells the server
   directly.
2. The server sets `SetResolutionConstraints()` to the viewport's physical pixel
   size.
3. On terminal resize, `web` sends updated coordinates, the app recomputes pixel
   size, and the server updates its capturer.

**Pass criteria:**

1. The blue square is crisp and sharp at native Retina resolution.
2. Resizing the terminal window updates the rendered content to match the new
   viewport size.
3. No stretching or blurriness — the IOSurface dimensions match the overlay quad
   dimensions exactly.

## File Summary

| File                                            | Action                                           |
| ----------------------------------------------- | ------------------------------------------------ |
| `ts5/src/renderer/shaders/shaders.metal`        | Add `overlay_vertex` + `overlay_fragment`        |
| `ts5/src/renderer/metal/shaders.zig`            | Replace `pink_overlay` with `overlay` pipeline   |
| `ts5/src/renderer/generic.zig`                  | Add `overlay_surface` field, texture render step |
| `ts5/include/ghostty.h`                         | Add `ghostty_surface_set_overlay_surface`        |
| `ts5/src/apprt/embedded.zig`                    | Export the new C API function                    |
| `ts5/src/Surface.zig`                           | Add `setOverlaySurface()` method                 |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Handle `display_surface` action                  |
| `chromium/src/.../shell_browser_main_parts.cc`  | Two-step gateway connect                         |
| `chromium/src/.../shell_switches.h`             | Add `--pane-id` flag                             |
| `web/src/main.rs`                               | Spawn Chromium Profile Server                    |

## Chromium Branch

Create `146.0.7650.0-issue-507` from `146.0.7650.0-issue-503` (which has the
latest Chromium Profile Server code including dynamic tabs and
`FrameSinkVideoCapturer`).

## Build

```bash
# Build Chromium Profile Server (once, ~1.5h; incremental ~20s)
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# Build xpc-gateway
cd ts5/xpc-gateway && swift build

# Build TermSurf
cd ts5 && zig build

# Build web
cd web && cargo build

# Install box-demo deps (if needed)
cd ts4/box-demo && bun install
```

## Verification

```bash
# Start the box-demo server
cd ts4/box-demo && bun run server.ts &

# Launch the app
open ts5/zig-out/TermSurf.app

# In a TermSurf pane (set TERMSURF_CHROMIUM_PATH if not using default):
cargo run -p web -- http://localhost:9407

# Expected:
# - URL bar shows "http://localhost:9407"
# - Viewport renders the blue spinning square on a dark background
# - Box-demo's built-in FPS counter shows 60fps
# - Server logs 60fps to stderr
# - Resizing the terminal updates the rendered content
# - Quitting web (q or Ctrl+C) clears the overlay and kills the server
```
