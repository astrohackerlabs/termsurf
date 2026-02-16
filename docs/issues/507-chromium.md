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
web TUI ──── TermSurf app ──── Chromium Profile Server
              (central hub)       (managed by app)
                   │
               xpc-gateway
           (rendezvous only)
```

Four processes:

1. **xpc-gateway** — Owns the Mach service. Pure rendezvous. Already built
   (Issue 506).

2. **TermSurf app** — The central hub. Registers anonymous listener with
   gateway. Receives `set_overlay` and `navigate` from `web`. Manages Chromium
   Profile Server lifecycle (spawns on first `navigate`, kills on `web`
   disconnect). Forwards URLs to the server. Receives `display_surface`
   (IOSurface Mach ports) from the server. Composites the IOSurface texture at
   the grid coordinates using Metal.

3. **`web` TUI** — Pure browser chrome. Sends viewport coordinates and URL to
   the app. Does not know about the Chromium Profile Server. Communicates only
   with the app via XPC.

4. **Chromium Profile Server** — Renders the webpage. Connects to app via
   gateway. Receives `navigate` commands from the app. Sends IOSurface Mach
   ports at 60fps on the direct connection.

### Connection Flow

```
1. App starts:     registers endpoint with gateway

2. web starts:     connects to gateway, gets endpoint, connects to app
                   sends set_overlay (grid coords + URL) on direct connection

3. App receives:   stores overlay grid coords
                   spawns Chromium Profile Server for the pane

4. Server starts:  connects to gateway, gets endpoint, connects to app
                   app sends navigate (URL) to server
                   server navigates to URL
                   sends display_surface (IOSurface Mach ports) at 60fps

5. App renders:    imports IOSurface from Mach port
                   creates MTLTexture
                   renders textured quad at grid coordinates

6. URL changes:    web sends navigate (new URL) to app
                   app forwards navigate to server

7. web exits:      drops connection
                   app kills server, clears overlay
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
Since the app manages the profile server directly, it can send this information
to the server on the direct XPC connection (e.g., alongside the `navigate`
command or as a separate `resize` message).

## XPC Protocol

### `web` to app (direct connection)

**Set overlay (existing, extended with URL):**

```
{ action: "set_overlay", pane_id: "<uuid>",
  col: N, row: N, width: N, height: N,
  url: "http://localhost:9407" }
```

The `url` field is new. On first receipt, the app spawns a Chromium Profile
Server for this pane and sends it the URL. On subsequent receipts (resize, URL
change), the app updates the overlay coordinates and/or forwards the new URL.

**Navigate (URL change only):**

```
{ action: "navigate", pane_id: "<uuid>",
  url: "http://localhost:9407/other-page" }
```

Sent when the user changes the URL in the `web` TUI. The app forwards it to the
profile server managing this pane.

### App to Chromium Profile Server (direct connection)

**Navigate:**

```
{ action: "navigate", url: "http://localhost:9407" }
```

Sent when the app receives a URL from `web` (either on initial `set_overlay` or
a subsequent `navigate`). The server navigates its WebContents to the URL.

### Chromium Profile Server to app (direct connection)

```
{ action: "display_surface", pane_id: "<uuid>",
  iosurface_port: <mach_send_right>,
  width: N, height: N }
```

The app maps `pane_id` to the correct surface and updates the overlay texture.
`width` and `height` are the IOSurface physical pixel dimensions.

### App spawns Chromium Profile Server (command-line args)

```
Chromium\ Profile\ Server \
  --pane-id <uuid> \
  --xpc-service com.termsurf.xpc-gateway \
  --hidden \
  --user-data-dir ~/.config/termsurf/profiles/default \
  --content-shell-host-window-size 800x600
```

Note: `--url` is no longer a command-line arg. The URL is sent via XPC after the
server connects. The `--content-shell-host-window-size` sets the initial
WebContents size. Approximate is fine for the first experiment; proper size
matching comes later.

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

### 4. CompositorXPC.swift: Profile Server Management

**Files:**

- `ts5/macos/Sources/Ghostty/CompositorXPC.swift` — Add profile server lifecycle
  management and new message handlers.

**New responsibilities:**

1. **Spawn profile server.** When `set_overlay` arrives with a `url` and no
   server is running for this pane, spawn a Chromium Profile Server process
   (`Process` / `NSTask`). Track it by pane ID. The server binary path is
   hardcoded or configured (e.g., `TERMSURF_CHROMIUM_PATH` env var, defaulting
   to
   `chromium/src/out/Default/Chromium Profile Server.app/Contents/MacOS/
   Chromium Profile Server`).

2. **Handle `display_surface`.** When an IOSurface frame arrives from the
   server:
   - Extract `pane_id` and look up the surface.
   - Extract `iosurface_port` via
     `xpc_dictionary_copy_mach_send(msg,
     "iosurface_port")`.
   - Extract `width` and `height`.
   - Call `ghostty_surface_set_overlay_surface(surface, port, width, height)`.

3. **Forward `navigate`.** When `navigate` arrives from `web`, forward the URL
   to the profile server's XPC connection for this pane.

4. **Kill server on disconnect.** When a `web` peer disconnects, kill the
   profile server process for that pane and clear the overlay.

The Mach port is passed through to the C API. The Zig side imports the
IOSurface. This keeps Swift minimal — no IOSurface framework import needed.

### 5. Chromium Profile Server: Gateway Connect + Navigate Handler

**Files:**

- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`
  — Modify XPC connection code for two-step gateway connect. Add incoming
  `navigate` message handler.
- `chromium/src/content/chromium_profile_server/common/shell_switches.h` — Add
  `--pane-id` flag. Remove `--url` flag (URL now comes via XPC).

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
5. Set up incoming message handler for `navigate` commands from the app.
6. Send `display_surface` frames on the direct connection, including `pane_id`.

When a `navigate` message arrives from the app, the server calls
`LoadURLForDocument()` on its WebContents to navigate to the new URL.

### 6. `web`: Send URL to App

**Files:**

- `web/src/main.rs` — Include the URL in `set_overlay` messages. Send `navigate`
  when the user changes the URL.

`web` no longer spawns or manages the Chromium Profile Server. It sends the URL
to the app and the app handles the rest.

```rust
// On startup / each draw: include url in set_overlay
let msg = XpcDictionary::new();
msg.set_string("action", "set_overlay");
msg.set_string("pane_id", &pane_id);
msg.set_u64("col", inner_rect.x as u64);
msg.set_u64("row", inner_rect.y as u64);
msg.set_u64("width", inner_rect.width as u64);
msg.set_u64("height", inner_rect.height as u64);
msg.set_string("url", &url);
conn.send(&msg);

// On URL change:
let msg = XpcDictionary::new();
msg.set_string("action", "navigate");
msg.set_string("pane_id", &pane_id);
msg.set_string("url", &new_url);
conn.send(&msg);
```

On `web` exit (Ctrl+C, `q`, or signal), the XPC connection drops. The app
detects the disconnect, kills the profile server, and clears the overlay.

### 7. Profile Data

Default profile storage: `~/.config/termsurf/profiles/default/`

This is the Chromium user data directory. It stores cookies, localStorage,
cache, and all other browser state. One directory per profile. Issue 507 uses
only the `default` profile.

## Ideas for Experiments

### Experiment Idea 1: IOSurface Texture Pipeline

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

### Experiment Idea 2: Live Chromium Frames

Connect the Chromium Profile Server and render live web content.

**Changes:**

1. Modify Chromium Profile Server for two-step gateway connect + `navigate`
   handler (Component 5).
2. Add `--pane-id` flag to the server. Remove `--url` (URL comes via XPC).
3. Add profile server management + `display_surface` handler to
   `CompositorXPC.swift` (Component 4).
4. Add `url` field to `web`'s `set_overlay` message (Component 6).
5. The renderer uses the IOSurface from the server instead of the test surface.

**Pass criteria:**

1. `cargo run -p web -- http://localhost:9407` shows the box-demo in the
   viewport — blue square rotating on a dark background.
2. The page renders at 60fps (verify via the box-demo's built-in FPS counter and
   the server's fps logging).
3. Quitting `web` kills the server and clears the overlay.

### Experiment Idea 3: Retina Resolution and Resize

Match the capture resolution to the viewport's physical pixel size.

**Changes:**

1. The app computes the viewport's physical pixel size from the grid coordinates
   (`cell_size * scale_factor`) and sends it to the profile server (e.g., as a
   `resize` message or alongside `navigate`).
2. The server sets `SetResolutionConstraints()` to the viewport's physical pixel
   size.
3. On terminal resize, `web` sends updated grid coordinates to the app. The app
   recomputes the pixel size and sends a `resize` to the server.

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
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Manage profile server, handle `display_surface`  |
| `chromium/src/.../shell_browser_main_parts.cc`  | Two-step gateway connect, `navigate` handler     |
| `chromium/src/.../shell_switches.h`             | Add `--pane-id`, remove `--url`                  |
| `web/src/main.rs`                               | Send URL to app (no longer spawns server)        |

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

# In a TermSurf pane:
cargo run -p web -- http://localhost:9407

# Expected:
# - URL bar shows "http://localhost:9407"
# - Viewport renders the blue spinning square on a dark background
# - Box-demo's built-in FPS counter shows 60fps
# - Server logs 60fps to stderr
# - Resizing the terminal updates the rendered content
# - Quitting web (q or Ctrl+C) clears the overlay and kills the server
```

## Experiments

### Experiment 1: IOSurface Texture Overlay

Prove the Metal renderer can display an IOSurface-backed texture at grid
coordinates. Uses a test IOSurface created in Swift (no Chromium yet).

#### Why This First

The pink overlay (Issue 505) renders a solid-color quad. The final pipeline
renders an IOSurface texture. This experiment bridges the gap — same grid
coordinates, but sampling from a real IOSurface instead of returning a constant
color. If this works, Experiment 2 just swaps the test surface for live Chromium
frames.

#### Changes

**1. Metal shaders — `ts5/src/renderer/shaders/shaders.metal`**

Add `overlay_vertex` and `overlay_fragment`. The vertex shader is the same
grid-to-clip-space math as `pink_overlay_vertex`, but also outputs texture
coordinates (0→1). The fragment shader samples the texture.

```metal
struct OverlayVertexOut {
    float4 position [[position]];
    float2 texcoord;
};

vertex OverlayVertexOut overlay_vertex(
    uint vid [[vertex_id]],
    constant PinkOverlayIn &params [[buffer(0)]],
    constant Uniforms &uniforms [[buffer(1)]]
) {
    float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
    float2 size = float2(params.grid_width, params.grid_height) * uniforms.cell_size;

    float2 corner;
    corner.x = float(vid == 1 || vid == 3);
    corner.y = float(vid == 2 || vid == 3);

    OverlayVertexOut out;
    out.position = uniforms.projection_matrix * float4(origin + size * corner, 0.0, 1.0);
    out.texcoord = corner;
    return out;
}

fragment float4 overlay_fragment(
    OverlayVertexOut in [[stage_in]],
    texture2d<float> tex [[texture(0)]]
) {
    constexpr sampler s(mag_filter::linear, min_filter::linear);
    return tex.sample(s, in.texcoord);
}
```

Reuses `PinkOverlayIn` for the grid coordinate params (same struct, same
layout). Keep the existing pink shaders for now.

**2. Pipeline — `ts5/src/renderer/metal/shaders.zig`**

Add `overlay` pipeline alongside `pink_overlay`:

```zig
.{ "overlay", .{
    .vertex_fn = "overlay_vertex",
    .fragment_fn = "overlay_fragment",
    .blending_enabled = true,
} },
```

Blending enabled so the texture can composite over terminal content.

**3. Renderer state — `ts5/src/renderer/generic.zig`**

Add field alongside `pink_overlay`:

```zig
/// IOSurfaceRef for the overlay texture. Set from Swift via C API.
/// When non-null, drawFrame() creates an MTLTexture from it and
/// renders with the overlay pipeline instead of pink_overlay.
overlay_iosurface: ?*anyopaque = null,
```

In `drawFrame()`, replace the pink overlay render block with:

```zig
if (self.pink_overlay.grid_width > 0 and
    self.pink_overlay.grid_height > 0)
{
    if (self.overlay_iosurface) |iosurface| {
        // Create MTLTexture from IOSurface (zero-copy, cheap per-frame).
        const desc = // MTLTextureDescriptor for BGRA8Unorm_sRGB
        const tex = device.msgSend(
            ?objc.Object,
            objc.sel("newTextureWithDescriptor:iosurface:plane:"),
            .{ desc, iosurface, @as(c_ulong, 0) },
        );
        if (tex) |t| {
            // Render textured quad with overlay pipeline.
            pass.step(.{
                .pipeline = self.shaders.pipelines.overlay,
                .uniforms = frame.uniforms.buffer,
                .buffers = &.{buf.buffer},
                .textures = &.{Texture.fromNative(t)},
                .draw = .{ .type = .triangle_strip, .vertex_count = 4 },
            });
        }
    } else {
        // Fallback: pink overlay (no IOSurface yet).
        // ... existing pink_overlay render code ...
    }
}
```

The MTLTexture creation from IOSurface is zero-copy — it's just a view into the
same GPU memory. Creating it per-frame is fine. For streaming Chromium frames
(Experiment 2), each new IOSurface produces a different texture, so per-frame
creation is the correct pattern anyway.

**4. C API — `ts5/include/ghostty.h`**

```c
void ghostty_surface_set_overlay_iosurface(ghostty_surface_t, void* iosurface_ref);
```

**5. Export — `ts5/src/apprt/embedded.zig`**

```zig
export fn ghostty_surface_set_overlay_iosurface(
    surface: *Surface,
    iosurface: ?*anyopaque,
) void {
    surface.core_surface.setOverlayIOSurface(iosurface);
}
```

**6. Surface method — `ts5/src/Surface.zig`**

```zig
pub fn setOverlayIOSurface(self: *Surface, iosurface: ?*anyopaque) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    self.renderer.overlay_iosurface = iosurface;
    self.queueRender() catch {};
}
```

**7. Test IOSurface — `ts5/macos/Sources/Ghostty/CompositorXPC.swift`**

In `handleMessage` for `set_overlay`, after calling
`ghostty_surface_set_overlay()`, create a test IOSurface and pass it:

```swift
import IOSurface

// Create a 256x256 BGRA checkerboard IOSurface.
let testSurface = IOSurface(properties: [
    .width: 256,
    .height: 256,
    .bytesPerElement: 4,
    .bytesPerRow: 256 * 4,
    .pixelFormat: 0x42475241  // 'BGRA'
] as [IOSurfacePropertyKey: Any])!

testSurface.lock(options: [], seed: nil)
let base = testSurface.baseAddress
let bpr = testSurface.bytesPerRow
for y in 0..<256 {
    for x in 0..<256 {
        let cellX = x / 32
        let cellY = y / 32
        let isLight = (cellX + cellY) % 2 == 0
        let offset = y * bpr + x * 4
        // BGRA: blue, green, red, alpha
        if isLight {
            base.storeBytes(of: UInt32(0xFF_FF_88_44), toByteOffset: offset, as: UInt32.self)  // #4488FF
        } else {
            base.storeBytes(of: UInt32(0xFF_22_22_22), toByteOffset: offset, as: UInt32.self)  // #222222
        }
    }
}
testSurface.unlock(options: [], seed: nil)

ghostty_surface_set_overlay_iosurface(cSurface, Unmanaged.passUnretained(testSurface).toOpaque())
```

The checkerboard uses 8x8 cells (32px each) in blue (#4488FF) and dark (#222222)
— matching the box-demo's color scheme. This pattern makes texture coordinate
correctness visually obvious: if the mapping is wrong, the squares will stretch
or shift.

The IOSurface must be retained for the lifetime of the overlay. Store it on the
`CompositorXPC` instance alongside the peer tracking.

#### Pass Criteria

1. When `web` runs, a blue/dark checkerboard appears at the viewport coordinates
   (instead of pink).
2. The checkerboard follows the viewport on terminal resize.
3. Quitting `web` clears the overlay.
4. No flickering or tearing.

#### Files

| File                                            | Change                                                         |
| ----------------------------------------------- | -------------------------------------------------------------- |
| `ts5/src/renderer/shaders/shaders.metal`        | Add `overlay_vertex` + `overlay_fragment`                      |
| `ts5/src/renderer/metal/shaders.zig`            | Add `overlay` pipeline                                         |
| `ts5/src/renderer/generic.zig`                  | Add `overlay_iosurface` field, texture render in `drawFrame()` |
| `ts5/include/ghostty.h`                         | Add `ghostty_surface_set_overlay_iosurface`                    |
| `ts5/src/apprt/embedded.zig`                    | Export new C function                                          |
| `ts5/src/Surface.zig`                           | Add `setOverlayIOSurface()` method                             |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Create test IOSurface, call new API                            |

#### Build & Verify

```bash
cd ts5/xpc-gateway && swift build
cd ts5 && zig build
cargo build -p web
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://example.com

# Expected: blue/dark checkerboard at viewport instead of pink
# Resize terminal → checkerboard follows
# Quit web → checkerboard clears
```

### Experiment 2: Retina-Correct Resolution

The Experiment 1 checkerboard is blurry because the IOSurface is a fixed 256x256
pixels, stretched to fill the overlay area. On a Retina display, the overlay
might be 1000x800 physical pixels — the 256x256 texture is magnified 4x, and
linear filtering smooths the edges.

This experiment creates the IOSurface at the exact physical pixel dimensions of
the overlay viewport, so the texture maps 1:1 with screen pixels.

#### Key Insight

Ghostty's `cell_width` and `cell_height` (from font metrics) are already in
physical pixels. When the display scale factor changes, `contentScaleCallback()`
recalculates font metrics at the new DPI (e.g., 72 * 2 = 144 DPI for Retina). So
the conversion is simply:

```
pixel_width  = grid_width  * cell_width
pixel_height = grid_height * cell_height
```

No separate scale factor multiplication needed.

#### Changes

**1. C API: query cell size — `ts5/include/ghostty.h`**

```c
void ghostty_surface_get_cell_size(ghostty_surface_t,
                                    uint32_t* width, uint32_t* height);
```

Returns `cell_width` and `cell_height` in physical pixels from the renderer's
font metrics (`renderer.grid_metrics.cell_width/height`).

**2. Export — `ts5/src/apprt/embedded.zig`**

```zig
export fn ghostty_surface_get_cell_size(
    surface: *Surface,
    width: *u32,
    height: *u32,
) void {
    surface.core_surface.getCellSize(width, height);
}
```

**3. Surface method — `ts5/src/Surface.zig`**

```zig
pub fn getCellSize(self: *Surface, width: *u32, height: *u32) void {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();
    width.* = self.renderer.grid_metrics.cell_width;
    height.* = self.renderer.grid_metrics.cell_height;
}
```

**4. Dynamic IOSurface — `ts5/macos/Sources/Ghostty/CompositorXPC.swift`**

Replace the fixed 256x256 test surface with a dynamically sized one:

```swift
// Query cell size from the renderer.
var cellWidth: UInt32 = 0
var cellHeight: UInt32 = 0
ghostty_surface_get_cell_size(cSurface, &cellWidth, &cellHeight)

// Compute physical pixel dimensions.
let pixelWidth = Int(width) * Int(cellWidth)
let pixelHeight = Int(height) * Int(cellHeight)

// Create IOSurface at exact pixel dimensions.
let surface = IOSurface(properties: [
    .width: pixelWidth,
    .height: pixelHeight,
    .bytesPerElement: 4,
    .bytesPerRow: pixelWidth * 4,
    .pixelFormat: 0x42475241  // 'BGRA'
])
```

Draw the same checkerboard pattern but at the native resolution. Each checker
cell is `cellWidth x cellHeight` — exactly one terminal cell — so the grid lines
align perfectly with the terminal grid.

Recreate the IOSurface whenever the overlay size changes (i.e., when `web` sends
a new `set_overlay` with different width/height). Store the current width/height
and compare on each message.

**5. Nearest-neighbor sampling — `ts5/src/renderer/shaders/shaders.metal`**

Change the overlay fragment sampler from linear to nearest:

```metal
constexpr sampler s(mag_filter::nearest, min_filter::nearest);
```

This is correct for 1:1 pixel mapping — there's no magnification, so every texel
maps to exactly one screen pixel. Linear filtering would still blur at sub-pixel
boundaries.

#### Pass Criteria

1. Checkerboard squares have sharp, crisp edges (not blurry).
2. Each checker cell is exactly one terminal cell in size (aligned to the grid).
3. Resizing the terminal recreates the IOSurface at the new dimensions — still
   crisp after resize.
4. No flickering or tearing.

#### Files

| File                                            | Change                                  |
| ----------------------------------------------- | --------------------------------------- |
| `ts5/include/ghostty.h`                         | Add `ghostty_surface_get_cell_size`     |
| `ts5/src/apprt/embedded.zig`                    | Export new C function                   |
| `ts5/src/Surface.zig`                           | Add `getCellSize()` method              |
| `ts5/macos/Sources/Ghostty/CompositorXPC.swift` | Dynamic IOSurface at correct pixel size |
| `ts5/src/renderer/shaders/shaders.metal`        | Change sampler to `nearest` filtering   |

#### Build & Verify

```bash
cd ts5/xpc-gateway && swift build
cd ts5 && zig build
cargo build -p web
open ts5/zig-out/TermSurf.app

# In a TermSurf pane:
cargo run -p web -- http://example.com

# Expected: crisp checkerboard, each square = one terminal cell
# Resize terminal → surface recreated, still crisp
# Quit web → overlay clears
```

#### Result: Pass

The checkerboard renders pixel-perfect at native Retina resolution. Each checker
cell is exactly one terminal cell, with sharp crisp edges — no blurriness. The
`ghostty_surface_get_cell_size` API correctly returns physical pixel dimensions
(cell sizes already include the Retina scale factor via DPI-scaled font
metrics).

**Resize crash:** Resizing the terminal window while the overlay is active
crashes the app. This is expected — the resize path recreates the IOSurface on
the main thread while the renderer may be mid-frame reading the old
`overlay_iosurface` pointer. The current `draw_mutex` protects the pointer
assignment but not the IOSurface lifetime (ARC releases the old surface while
the renderer still holds a raw pointer to it). This needs proper lifetime
management before resize can work, but it's out of scope for this experiment.
