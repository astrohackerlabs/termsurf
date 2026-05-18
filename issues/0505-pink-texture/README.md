+++
status = "closed"
opened = "2026-02-15"
closed = "2026-02-15"
+++

# Issue 505: Pink Texture Overlay

## Background

Issue 504 built the `web` TUI chrome — a ratatui-based terminal application that
draws a URL bar, viewport border, and status bar inside a Ghostty pane. The
viewport is the region where the browser content will eventually render. The
`web` TUI knows the exact pixel coordinates and size of its viewport (it prints
them inside the viewport itself).

This issue is the next step: **render a solid pink GPU texture where the browser
viewport is supposed to be.** No browser, no Chromium, no IPC — just a pink
rectangle rendered by Ghostty's Metal pipeline at the correct position inside
the terminal pane. Pink because it's unmistakably visible.

This is the foundational experiment for browser pane rendering. If we can
overlay a texture at arbitrary pixel coordinates inside a Ghostty pane, we can
overlay anything — including Chromium's IOSurface output.

## Prior Art

### What Previous Generations Taught Us

#### ts1 (Ghostty + WKWebView)

ts1 used WKWebView as a native macOS NSView overlaid on the terminal pane. No
GPU texture compositing was needed — WKWebView handled its own rendering. The
overlay was positioned using NSView frame coordinates. This approach only worked
on macOS and is not applicable to ts5's in-process Chromium strategy.

#### ts3 (WezTerm + out-of-process CEF via XPC)

ts3 used wgpu to composite CEF-rendered IOSurfaces into WezTerm's terminal
panes. Key lessons:

- **Viewport calculation was the hardest part.** Grid cells → physical pixels →
  logical DIP → CEF dimensions. Getting this chain right took many experiments
  (Issues 306, 308, 309, 311).
- **sRGB double-correction was a major bug.** CEF outputs sRGB pixel data. If
  the texture view is declared as linear (`Bgra8Unorm`), the GPU applies gamma
  correction again, washing out colors. Fix: use `Bgra8UnormSrgb` so the GPU
  knows the data is already sRGB-encoded.
- **Textures bounced and stretched during resize.** The root cause (Issue 309):
  during continuous window drag, the old texture was rendered into the resized
  viewport. The debounce timer delayed resize commands by 30ms, but the viewport
  changed every frame. During that gap, the old texture (e.g., 2820×2130) was
  stretched into the new viewport (e.g., 1599×1590), breaking aspect ratio.
- **Removing debounce fixed the bouncing.** Sending resize on every frame (no
  debounce) eliminated the mismatch. CEF handled rapid resize commands fine.
- **Invalidation was critical.** When a new texture arrived via XPC, nothing
  triggered the render loop (Issue 309, Experiment 4). The texture sat in a
  buffer for up to 887ms until an incidental event (mouse move, keystroke)
  forced a render. Fix: call `window.invalidate()` immediately when a new XPC
  message arrives, so the render loop picks up the new data.
- **Scale factor source mismatch.** Initial spawn used pane DPI; resize used
  window DPI. If these differed, the logical dimensions were wrong. Fix: use the
  same DPI source for both (Issue 309).
- **Stale cell size during resize.** Global static cell size values could lag
  behind actual font rendering (Issue 311). Fix: always use current
  `render_metrics.cell_size`, never cached globals.
- **Send physical pixels, not logical.** Converting physical to logical on the
  GUI side introduced truncation errors (Issue 311). Sending physical pixels
  directly and letting the renderer convert eliminated double-truncation.

#### ts4 (Chromium Content API experiments)

ts4 proved in-process Chromium rendering at 60fps. Key lessons:

- **Metal IOSurface textures are zero-copy.**
  `device.makeTexture(descriptor:
  iosurface: plane:)` creates an MTLTexture
  that directly references IOSurface GPU memory. No pixel copying.
- **Retina requires three coordinated fixes (Issue 414).** All three must work
  together for crisp output:
  1. Capture at physical pixel resolution (not logical).
  2. Set `CAMetalLayer.contentsScale = backingScaleFactor`.
  3. Match `drawableSize` to physical pixel dimensions. Without all three,
     textures are blurry. Issue 414 Experiment 5 produced a 640×360 IOSurface on
     a Retina screen instead of 1280×720 — the logical size leaked through.
- **Metal bytesPerRow alignment.** IOSurface-backed textures require 16-byte row
  alignment: `(width * 4 + 15) & ~15`. Odd widths crash without this.
- **Resize was immediate, no debounce.** `windowDidEndLiveResize` sent physical
  dimensions via XPC. Child processes created a new IOSurface at the new size
  and sent it back. Old IOSurface released atomically when the new one replaced
  it — no flicker between frames.
- **Texture creation validates dimensions.** `MTLTexture` descriptor
  width/height must match the IOSurface. If they don't, Metal returns nil. This
  acts as implicit mismatch detection — the old texture stays visible until a
  correctly sized one arrives.

### Ghostty's Renderer (ts5)

ts5 is a lightly modified Ghostty fork. The renderer is a multi-pass Metal
pipeline:

```
IOSurface-backed Target (MTLTexture)
  ↓
Background (solid color or image)
  ↓
Kitty images (below text)
  ↓
Cell backgrounds (opaque)
  ↓
Kitty images (below text, above bg)
  ↓
Text (instanced: 4 vertices × N cells)
  ↓
Kitty images (above text)
  ↓
Debug overlay
  ↓
Custom shader passes (optional, ping-pong textures)
  ↓
Present (IOSurface → CALayer.contents)
```

**Key files:**

| File                                     | Purpose                                       |
| ---------------------------------------- | --------------------------------------------- |
| `ts5/src/renderer/generic.zig`           | Main render logic, `drawFrame()` at line 1393 |
| `ts5/src/renderer/metal/Target.zig`      | IOSurface-backed MTLTexture render target     |
| `ts5/src/renderer/metal/Frame.zig`       | Command buffer and completion handler         |
| `ts5/src/renderer/metal/RenderPass.zig`  | Render pass descriptor and encoder            |
| `ts5/src/renderer/metal/Pipeline.zig`    | MTLRenderPipelineState wrapper                |
| `ts5/src/renderer/metal/shaders.zig`     | Pipeline definitions and shader params        |
| `ts5/src/renderer/shaders/shaders.metal` | Metal shader source                           |
| `ts5/src/renderer/size.zig`              | Coordinate systems and size conversions       |
| `ts5/src/renderer/Metal.zig`             | Metal API wrapper, surface size, presentation |

**Coordinate systems** (from `size.zig`):

- **Surface coordinates:** (0,0) = top-left of window, units = physical pixels
  (after DPI scaling).
- **Terminal coordinates:** (0,0) = top-left of grid (padding removed), units =
  physical pixels.
- **Grid coordinates:** (0,0) = top-left of grid, units = cells (column, row).

**Existing pipelines** (from `metal/shaders.zig`):

- `bg_color` — Solid background fill
- `bg_image` — Background image
- `cell_bg` — Cell background colors
- `cell_text` — Text rendering (instanced)
- `image` — Kitty image protocol

Each pipeline has a vertex function, fragment function, and optional vertex
attributes. Adding a new pipeline for the pink overlay follows the same pattern.

## Architecture

The pink texture overlay is a new render pass step inserted into Ghostty's
`drawFrame()` function, after text rendering and before custom shaders. It draws
a solid-color rectangle at specific pixel coordinates within the terminal pane.

```
... existing render steps ...
  ↓
Text (instanced)
  ↓
Kitty images (above text)
  ↓
★ Pink overlay (NEW) — quad rendered at exact pixel coordinates of browser region
  ↓
Custom shader passes (if any)
  ↓
Present
```

### Why After Text

The pink texture must be drawn **on top of** the terminal content. The `web` TUI
renders its chrome (URL bar, status bar, borders) as terminal text. The browser
viewport area contains terminal text too (the coordinates display). The pink
overlay covers the viewport area, obscuring the terminal text beneath it — which
is exactly what a real browser texture would do.

### Resize Timing

When the terminal window resizes, there is an unavoidable gap between the
compositor knowing the new size and `web` sending updated coordinates:

```
t0    Compositor resizes pane (new grid dimensions, new padding)
t1    Shell receives SIGWINCH
t2    crossterm detects resize event
t3    web's event loop wakes, calls terminal.draw()
t4    ratatui recomputes layout with new dimensions
t5    web sends set_overlay via XPC with new grid coordinates
t6    Compositor stores new overlay rect
t7    Next drawFrame() renders pink quad at new position/size
```

During `t0`–`t6`, the compositor has the old grid coordinates but the new cell
size and padding. Because cell size is determined by font metrics (not terminal
dimensions), it does **not** change on resize. What changes is the grid
dimensions (columns/rows) and balanced padding.

**Impact:** Between `t0` and `t6`, the overlay is drawn with the old grid
width/height but the current padding. Since `col` and `row` are typically small
values (e.g., col=1, row=3) and cell size hasn't changed, the overlay **position
stays correct** — only the **width/height are stale** (the overlay is too narrow
or too wide for a few frames).

**Why this is acceptable for the pink overlay:**

- The gap is a few milliseconds (SIGWINCH → ratatui draw → XPC send is fast).
- The overlay position stays correct (top-left corner doesn't jump).
- The width/height catch up within 1–2 frames.
- There is no aspect-ratio distortion (unlike ts3 where an old texture was
  stretched into a new viewport).

**Lesson from ts3:** The bouncing problem in ts3 was far worse because the old
_texture_ (with wrong pixel dimensions) was stretched into the new viewport.
Here, we're re-deriving pixel coordinates from grid coordinates every frame, so
the position is always consistent with the current cell size and padding. The
only stale data is the grid width/height from `web`, which updates within
milliseconds.

**Invalidation requirement:** When new coordinates arrive via XPC, the
compositor must trigger a redraw. ts3's Issue 309 showed that without explicit
invalidation, new data can sit in a buffer for hundreds of milliseconds. The
compositor must call the equivalent of `window.invalidate()` when it receives a
`set_overlay` message.

### Positioning Strategy: XPC Channel

The `web` TUI knows its viewport in **grid coordinates** (column, row, width in
columns, height in rows). The TermSurf compositor (Ghostty fork) knows how to
convert grid coordinates to physical pixels (cell size × grid position +
padding). They communicate over XPC — the same mechanism that will carry
IOSurface Mach ports for real browser textures.

**Pane identification:**

Each terminal pane sets a `TERMSURF_PANE_ID` environment variable before
spawning its shell. This is a unique identifier (e.g., a UUID or incrementing
integer) that the compositor assigns when creating the pane. Any process running
inside the pane — including `web` — inherits this env var and uses it to
identify itself to the compositor.

**XPC service:**

The compositor registers as `com.termsurf.compositor`, an XPC Mach service. This
is the same pattern ts3 used with `com.termsurf.launcher`. The compositor
listens for connections from `web` processes running inside its panes.

**Flow:**

```
web TUI (ratatui)                          TermSurf compositor
─────────────────                          ────────────────────
Reads TERMSURF_PANE_ID
from environment
        │
        ▼
Connects to
  com.termsurf.compositor  ──XPC──▶  Accepts connection
        │                                    │
        ▼                                    ▼
Sends: set_overlay                   Stores overlay rect
  pane_id: <id>                      for pane <id>
  col: 1, row: 3,                    in grid coordinates
  width: 78, height: 20                     │
        │                                    ▼
        │                              drawFrame() converts
        │                              grid → physical pixels:
        │                                x = col × cell_w + pad_left
        │                                y = row × cell_h + pad_top
        │                                w = cols × cell_w
        │                                h = rows × cell_h
        │                                    │
        │                                    ▼
        │                              Render pink quad at
        │                              computed pixel rect
        │
Terminal resizes → SIGWINCH
        │
        ▼
ratatui recomputes layout
  new rect: col=1, row=3,
  width=118, height=40
        │
        ▼
Sends: set_overlay         ──XPC──▶  Update overlay rect
  pane_id: <id>                              │
  col: 1, row: 3,                           ▼
  width: 118, height: 40            Next drawFrame() uses
                                     new coordinates

web exits or disconnects   ──XPC──▶  Connection closed →
                                     clear overlay for pane
```

**Why grid coordinates, not pixels:**

- Grid coordinates are resolution-independent. No DPI/Retina math in `web`.
- The compositor already knows cell sizes, padding, and scale factor.
- The conversion happens once per frame in `drawFrame()`, using values the
  compositor already has.
- If the font size changes (which changes cell size), the overlay automatically
  adjusts without `web` needing to know.

**Why XPC:**

- **Two-way.** The compositor can send messages back to `web` (resize
  notifications, focus changes, etc.). OSC escape sequences are one-way.
- **Same channel for everything.** Viewport coordinates, IOSurface Mach ports,
  input events, and navigation commands will all flow over one XPC connection.
- **Proven in ts3.** The XPC patterns for Mach port transfer and structured
  messaging are already established.
- **Pane-aware.** The pane ID ties each `web` instance to its pane, so the
  compositor knows exactly where to render.

**Message format (XPC dictionary):**

- `action`: `"set_overlay"` — set or update the overlay rectangle.
- `pane_id`: string — the pane this overlay belongs to.
- `col`, `row`, `width`, `height`: integers — grid coordinates (0-indexed).

To clear the overlay, `web` simply disconnects. The compositor detects the
closed connection and removes the overlay for that pane.

## Sizing and Resize Lessons (Reference)

From four generations of texture overlay experiments, these are the rules. Each
was learned the hard way.

### Coordinate Systems

1. **Always work in physical pixels in the renderer.** Multiply logical
   coordinates by `backingScaleFactor` (typically 2.0 on Retina Macs). Ghostty's
   renderer already operates in physical pixels — the projection matrix and all
   coordinates in `drawFrame()` use physical pixel units.

2. **Use the existing projection matrix.** Ghostty creates an orthographic 2D
   projection in `generic.zig` (`math.ortho2d`). The matrix maps pixel
   coordinates to normalized device coordinates. All existing shaders
   (cell_text, image) follow the pattern:
   `position = projection × float4(pixel_x, pixel_y,
   0, 1)`. The pink overlay
   must do the same.

3. **Grid → pixel conversion is simple.** Ghostty uses one formula everywhere:
   `pixel = grid_pos × cell_size + padding`. Cell size comes from font metrics
   and does not change on terminal resize. Padding changes when the terminal
   resizes (balanced padding centers the grid). The conversion must use current
   values from the renderer's `Size` struct, never cached or stale values.

4. **The image shader is the model.** Kitty images are the closest existing
   analog to the pink overlay. They are positioned at grid coordinates with a
   pixel offset, sized in pixels, and rendered as textured quads. The image
   shader parameters are:
   ```
   grid_pos: [2]f32       // top-left cell
   cell_offset: [2]f32    // pixel offset from cell corner
   dest_size: [2]f32      // rendered size in pixels
   ```
   The pink overlay follows this same pattern.

### Resize

5. **Invalidate on new data.** When new coordinates arrive via XPC, the
   compositor must trigger a redraw immediately. ts3 Issue 309 showed that
   without explicit invalidation, a new texture sat in a buffer for up to 887ms.
   The render loop only ran when an incidental event (mouse move, keystroke)
   happened to trigger it. Fix: call `invalidate()` in the XPC handler.

6. **No debounce for coordinate updates.** ts3's 30ms debounce caused the
   bouncing problem — the timer reset every frame during continuous drag, so
   resize didn't fire until the user stopped dragging. Removing the debounce
   fixed it. For the pink overlay, every `set_overlay` message from `web` should
   be applied immediately. There is no expensive operation to debounce (unlike
   CEF re-rendering).

7. **Use current cell size, never stale values.** ts3 Issue 311 found that
   global static cell size variables lagged behind actual font metrics. The fix:
   always read `cell_size` from the renderer's current `Size` struct in
   `drawFrame()`, which is guaranteed fresh. Ghostty's `drawFrame()` already
   calls `api.surfaceSize()` and detects size changes per-frame — the overlay
   conversion piggybacks on this.

8. **Cell size does not change on terminal resize.** Cell size is determined by
   font metrics, not terminal dimensions. When the terminal window resizes, the
   grid dimensions (columns, rows) and padding change, but cell width and height
   stay constant. This means the overlay position (derived from grid position ×
   cell size) stays correct during the gap between resize and updated
   coordinates. Only the overlay width/height are stale for a few frames.

9. **Atomic replacement, not incremental update.** ts4 replaced IOSurface
   textures atomically — the new texture replaced the old one in a single
   assignment. No frame ever mixed old and new data. The pink overlay should
   follow the same pattern: when new grid coordinates arrive via XPC, replace
   the entire overlay rect in one write.

### Future (Browser Texture)

These lessons don't apply to the pink overlay but will matter when we replace it
with a real browser IOSurface:

10. **IOSurface bytesPerRow must be 16-byte aligned.** Formula:
    `(width * 4 + 15) & ~15`. Discovered in ts4 when odd window widths caused
    Metal texture creation failures.

11. **sRGB double-correction.** Chromium outputs sRGB pixel data. If the texture
    view is declared as linear (`Bgra8Unorm`), the GPU applies gamma correction
    again, washing out colors. Fix: declare the texture view as `Bgra8UnormSrgb`
    (or the equivalent in Ghostty's Display P3 color space).

12. **Retina rendering requires three coordinated fixes (Issue 414).** All three
    must be present:
    - Capture at physical pixel resolution.
    - Set `contentsScale = backingScaleFactor` on the Metal layer.
    - Match `drawableSize` to physical pixel dimensions. Without all three,
      output is blurry.

13. **Stale frames during resize.** When the browser hasn't re-rendered at the
    new size yet, the old IOSurface doesn't match the new viewport. Options:
    - **Scale to fit:** Stretch the old texture into the new viewport
      (introduces blur but avoids black flash). ts3 used this.
    - **Discard mismatched frames:** Show terminal content until a correctly
      sized frame arrives. Ghostty's `IOSurfaceLayer.setSurface()` already
      validates dimensions and discards mismatches.
    - **Hide overlay:** Clear the overlay until new coordinates and a matching
      texture arrive. The right choice depends on the visual tradeoff. For the
      pink overlay this doesn't apply — the shader draws at whatever size it's
      told.

14. **Send physical pixels across IPC, not logical.** ts3 Issue 311 found that
    converting physical → logical on the sender side introduced truncation
    errors. Send physical dimensions and let the receiver convert. For the pink
    overlay this doesn't apply (we send grid coordinates, and the compositor
    converts to physical internally), but it will matter for browser resize
    messages.

## Experiments

### Experiment 1: Dynamic Pink Quad via XPC

Add a new Metal shader pipeline (`pink_overlay`) that draws a solid pink
rectangle. The rectangle's position and size come from an XPC message sent by
the `web` TUI. When the terminal resizes, `web` sends updated coordinates and
the pink overlay follows.

This experiment has three parts: the Metal shader, the XPC listener in the
compositor, and the `web` TUI integration.

#### Changes

##### Part 1: Metal Shader Pipeline

###### `ts5/src/renderer/shaders/shaders.metal`

Add two new shader functions:

**Vertex shader (`pink_overlay_vertex`):**

Takes a uniform buffer with the overlay rectangle (x, y, width, height in
physical pixels) and the projection matrix. Emits 4 vertices (triangle strip)
positioned at the exact corners of the overlay rectangle.

The vertex shader converts pixel coordinates to clip space using the existing
orthographic projection matrix. This is the same approach the `image` shader
uses.

**Fragment shader (`pink_overlay_fragment`):**

Returns a solid pink color: `float4(1.0, 0.41, 0.71, 1.0)` (hot pink,
`#FF69B4`).

###### `ts5/src/renderer/metal/shaders.zig`

Add a new pipeline definition `pink_overlay` alongside the existing pipelines.
Define a `PinkOverlayParams` struct with the overlay rectangle dimensions:

```
x: f32,      // Left edge in physical pixels
y: f32,      // Top edge in physical pixels
width: f32,  // Width in physical pixels
height: f32, // Height in physical pixels
```

###### `ts5/src/renderer/generic.zig`

In `drawFrame()`, after the kitty images (above text) step and before custom
shaders, add a new step:

1. Check if an overlay rect is set (non-zero). If not, skip this step.

2. Convert the stored grid coordinates to physical pixel coordinates:
   ```
   x = overlay_col × cell_width + padding_left
   y = overlay_row × cell_height + padding_top
   w = overlay_cols × cell_width
   h = overlay_rows × cell_height
   ```

3. Populate `PinkOverlayParams` with the computed pixel coordinates.

4. Sync the params buffer to the GPU.

5. Add a render pass step with the `pink_overlay` pipeline.

##### Part 2: XPC Listener (Compositor)

###### Pane ID Environment Variable

When creating a terminal pane, the compositor sets `TERMSURF_PANE_ID=<id>` in
the pane's environment. This is inherited by the shell and all child processes.
The ID must be unique across all panes in the compositor (a UUID or monotonic
counter).

###### XPC Mach Service (`com.termsurf.compositor`)

The compositor registers an XPC Mach service at startup. When a `web` process
connects and sends a `set_overlay` message:

1. Look up the pane by `pane_id`.
2. Store the overlay grid rect (col, row, width, height) on the pane's state.
3. **Trigger a redraw** for the pane's surface. This is critical — ts3 Issue 309
   showed that without explicit invalidation, new data can sit in a buffer for
   hundreds of milliseconds until an incidental event forces a render. The XPC
   handler must call the pane's invalidation function immediately.

When the XPC connection closes (because `web` exited or crashed):

1. Clear the overlay rect for that pane.
2. Trigger a redraw.

The overlay rect must be accessible from the renderer thread (where
`drawFrame()` runs). Use the same thread-safe communication pattern Ghostty uses
for other terminal state (e.g., the surface mailbox or shared state protected by
the draw mutex).

###### Implementation Location

The XPC listener should live in the macOS Swift shell (`ts5/macos/`), since XPC
is a macOS framework. The overlay rect is passed to the Zig renderer via the
existing C API bridge (`ts5/include/`).

##### Part 3: `web` TUI Integration

###### `web/src/main.rs`

On startup, read `TERMSURF_PANE_ID` from the environment and connect to
`com.termsurf.compositor` via XPC. After each `terminal.draw()` call, compute
the viewport inner rect and send the overlay coordinates:

```rust
let pane_id = std::env::var("TERMSURF_PANE_ID")
    .expect("TERMSURF_PANE_ID not set — not running inside TermSurf");

let compositor = connect_to_compositor(); // XPC connection

terminal.draw(|frame| ui(frame, &url, &profile, &mode))?;

// Send overlay coordinates to compositor.
// inner_rect is computed during ui() via Block::inner().
compositor.send_set_overlay(
    &pane_id,
    inner_rect.x, inner_rect.y,
    inner_rect.width, inner_rect.height,
);
```

The `inner_rect` values are already computed by ratatui's layout engine. When
the terminal resizes, ratatui automatically recomputes the layout on the next
`draw()` call, and the new coordinates are sent.

On exit, the XPC connection closes automatically. The compositor detects the
disconnection and clears the overlay for that pane — no explicit "clear" message
needed.

#### Pass Criteria

1. Ghostty builds without errors or warnings.
2. Running `web <url>` inside Ghostty shows a pink rectangle exactly covering
   the viewport area (inside the border, below the URL bar, above the status
   bar).
3. Resizing the terminal causes the pink rectangle to resize and reposition to
   match the new viewport dimensions. No lag, no stale positioning.
4. The pink rectangle is opaque and fully covers the terminal text beneath it.
5. The rest of the terminal (URL bar, border, status bar) renders normally.
6. Quitting `web` (Ctrl+C or `q`) clears the pink overlay — the terminal returns
   to normal with no pink residue.
7. The pink rectangle does not flicker or tear during resize.

#### Result: FAIL

`TERMSURF_PANE_ID` is not set in the shell environment when running inside the
built Ghostty app. The `web` process reports:

```
[web] Not connected to compositor (TERMSURF_PANE_ID not set or service unavailable)
```

**What went wrong:**

The experiment modified `SurfaceView_AppKit.swift` to set
`surface_cfg.environmentVariables["TERMSURF_PANE_ID"]` before creating the
surface. However, Ghostty's `SurfaceConfiguration.environmentVariables` does not
propagate to the shell process in the way we assumed. The environment dictionary
on the Swift side may not reach the Zig core's process spawning code, or it may
be overwritten/ignored during the `withCValue` serialization to the C API.

**What needs investigation for Experiment 2:**

1. **How does Ghostty propagate environment variables to child processes?**
   Trace the path from `SurfaceConfiguration.environmentVariables` through
   `withCValue` → `ghostty_surface_config_s` → Zig `Surface.init()` → shell
   spawn. Identify where the env var is lost.

2. **Is the XPC Mach service reachable?** Even if `TERMSURF_PANE_ID` were set,
   the `com.termsurf.compositor` Mach service may not be registered with
   launchd. The current plist launches a separate Ghostty binary as the service,
   but the XPC listener (`CompositorXPC.swift`) runs inside the app process. The
   app would need to register the Mach service itself (via
   `xpc_connection_create_mach_service` with the listener flag) rather than
   relying on launchd to launch a separate process.

3. **Chicken-and-egg with launchd.** If the Mach service is only available when
   the app is running, launchd cannot start it on-demand. The app must register
   the service at startup and launchd must have a matching `MachServices` entry
   in the plist. Alternatively, skip launchd entirely and use an XPC anonymous
   connection or a different IPC mechanism (Unix socket, named pipe).

### Experiment 2: Diagnose and Fix Env Var Propagation

Experiment 1 failed with `TERMSURF_PANE_ID not set or service unavailable`. But
that error message conflates two independent failures — the env var and the XPC
service. This experiment separates them, diagnoses the actual failure, and fixes
it.

#### Analysis

Code review shows the env var propagation chain is **complete** in ts5:

1. **Swift:** `surface_cfg.environmentVariables["TERMSURF_PANE_ID"]` is set
   before `withCValue` in `SurfaceView_AppKit.swift:382`.
2. **Swift → C:** `withCValue` serializes the dictionary to
   `ghostty_surface_config_s.env_vars` / `.env_var_count` via `withCStrings` →
   `ghostty_env_var_s` array (`SurfaceView.swift:736–751`).
3. **C → Zig:** `embedded.zig:529–540` reads `opts.env_vars` and merges each
   key-value pair into `config.env.map` (a `RepeatableStringMap`).
4. **Zig → PTY:** `Surface.zig:632` passes `config.env` as `.env_override` to
   `termio.Exec.init`. `Exec.zig:801–808` iterates `.env_override` and merges
   entries into the child process environment.

This is the **exact same chain** that works in ts1, which successfully
propagated `TERMSURF_PANE_ID` and `TERMSURF_SOCKET` to child shells. The ts5
code is identical at every stage.

**Hypothesis:** The env var IS being set correctly, but the `web` TUI's error
message doesn't distinguish between "env var missing" and "XPC service
unreachable". The real failure is likely the XPC Mach service, which requires
launchd registration and was not properly tested in Experiment 1.

#### Changes

##### Part 1: Better Diagnostics in `web` TUI

###### `web/src/main.rs`

Split the error message into two distinct checks:

```rust
let pane_id = std::env::var("TERMSURF_PANE_ID").ok();
match &pane_id {
    Some(id) => eprintln!("[web] TERMSURF_PANE_ID = {}", id),
    None => eprintln!("[web] TERMSURF_PANE_ID not set (not running inside TermSurf)"),
}

let compositor = pane_id.as_ref().and_then(|_| xpc::CompositorConnection::connect());
match &compositor {
    Some(_) => eprintln!("[web] Connected to compositor"),
    None if pane_id.is_some() => eprintln!("[web] XPC service unavailable (is launchd plist loaded?)"),
    _ => {}
}
```

This tells us exactly which half fails.

##### Part 2: Ensure Launchd Plist Is Loaded

The XPC Mach service `com.termsurf.compositor` must be registered with launchd
before clients can connect. The `CompositorXPC.swift` listener calls
`xpc_connection_create_mach_service` with
`XPC_CONNECTION_MACH_SERVICE_LISTENER`, which only works if launchd knows about
the service name.

**Verification:** Check if the service is loaded:

```bash
launchctl print gui/$(id -u)/com.termsurf.compositor
```

If not loaded:

```bash
launchctl bootstrap gui/$(id -u) ts5/macos/com.termsurf.compositor.plist
```

**Note:** The plist's `ProgramArguments` points to the Ghostty binary. When a
client connects and the service isn't running, launchd will try to launch this
binary. If the app is already running, this creates a conflict. The `KeepAlive`
key is intentionally absent so launchd doesn't auto-launch.

##### Part 3: Add Startup Log in CompositorXPC

###### `ts5/macos/Sources/Ghostty/CompositorXPC.swift`

The `start()` method already prints to stderr, but the app may not be running
with stderr visible. Add an `os_log` call so the message appears in Console.app:

```swift
import os.log

private let logger = Logger(subsystem: "com.termsurf.compositor", category: "xpc")

// In start():
logger.info("Compositor XPC listener starting on com.termsurf.compositor")
// ... after resume:
logger.info("Compositor XPC listener active")
```

This confirms whether the listener actually starts when the app launches.

#### Test Procedure

1. Build ts5: `cd ts5 && zig build`
2. Quit any running TermSurf/Ghostty instance.
3. Ensure the launchd plist is loaded:
   ```bash
   launchctl bootstrap gui/$(id -u) ts5/macos/com.termsurf.compositor.plist
   ```
4. Launch the app: `open ts5/zig-out/Ghostty.app`
5. In the TermSurf terminal pane, verify the env var:
   ```bash
   echo $TERMSURF_PANE_ID
   ```
   Expected: a UUID string (e.g., `A1B2C3D4-E5F6-...`).
6. Run the web TUI:
   ```bash
   cd web && cargo run -- https://example.com
   ```
7. Check the diagnostic output — it will now show exactly which step fails.
8. Check Console.app for `com.termsurf.compositor` log messages.

#### Pass Criteria

1. `echo $TERMSURF_PANE_ID` prints a UUID inside a TermSurf pane.
2. The `web` TUI prints separate diagnostic lines for env var and XPC status.
3. If the env var is set but XPC fails, the diagnostic identifies that clearly.
4. If both work, the pink overlay appears (Experiment 1 pass criteria apply).

#### Result: PASS

The env var was set correctly all along — the hypothesis was confirmed. The
diagnostics revealed that the real failure was the XPC Mach service
registration:

1. The launchd plist pointed to `zig-out/bin/ghostty` (nonexistent). Updated to
   `zig-out/TermSurf.app/Contents/MacOS/ghostty`.
2. Launching the app via `open` gives it a different launchd identity
   (`application.com.termsurf.debug...`) than the plist's job
   (`com.termsurf.compositor`). The app cannot claim a Mach service owned by a
   different launchd job. Fix: launch via `launchctl kickstart` so the process
   identity matches the plist.

With both fixes, all Experiment 1 pass criteria are met:

- Pink rectangle covers the viewport area exactly.
- Resizing the terminal causes the pink rectangle to follow the viewport with no
  lag or stale positioning.
- URL bar, border, and status bar render normally around it.
- Quitting `web` clears the pink overlay.
- No flicker or tearing during resize.

**Launch commands:**

```bash
# Register (once):
launchctl bootstrap gui/$(id -u) ts5/macos/com.termsurf.compositor.plist

# Launch:
launchctl kickstart gui/$(id -u)/com.termsurf.compositor

# Restart after rebuild:
launchctl kill SIGTERM gui/$(id -u)/com.termsurf.compositor
launchctl kickstart gui/$(id -u)/com.termsurf.compositor
```

### Experiment 3: Fix Right-Edge Overshoot

The pink overlay extends roughly half a cell too far to the right. On the left,
there is a clean gap between the viewport border glyph and the pink rectangle.
On the right, the pink extends almost exactly to the border stroke — visibly
asymmetric.

#### Analysis

The pink overlay vertex shader adds `grid_padding` to the world-space origin:

```metal
float2 padding = float2(uniforms.grid_padding[3], uniforms.grid_padding[0]);
float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size + padding;
```

But the existing shaders (`cell_text_vertex`, `image_vertex`) do NOT add
`grid_padding`. They position at `grid_pos * cell_size` and let the projection
matrix handle the offset:

```metal
// cell_text_vertex (line 563):
float2 cell_pos = uniforms.cell_size * float2(in.grid_pos);

// image_vertex (line 823):
float2 image_pos = (uniforms.cell_size * in.grid_pos) + in.cell_offset;
```

The projection matrix maps world `(0, 0)` to the grid origin. Adding
`grid_padding` shifts the overlay to the right by
`padding.left +
blank_padding.left` pixels in world space — double-counting the
offset.

The visual effect depends on the value of `grid_padding.left`, which equals
`configured_padding + blank_padding`. The blank padding is the leftover pixels
when the terminal width isn't an exact multiple of cell width. If
`grid_padding.left ≈ 0.5 × cell_width`, the overlay shifts half a cell right:

- **Left side:** The gap between the border stroke (center of the border cell)
  and the pink left edge grows by half a cell. Looks like a clean boundary.
- **Right side:** The gap between the pink right edge and the border stroke
  shrinks by half a cell. The pink nearly touches the border.

#### Changes

##### `ts5/src/renderer/shaders/shaders.metal`

Remove the `grid_padding` addition from `pink_overlay_vertex`. Match the
`image_vertex` pattern — position at `grid_pos * cell_size` only:

```metal
vertex float4 pink_overlay_vertex(
  uint vid [[vertex_id]],
  constant PinkOverlayIn& params [[buffer(0)]],
  constant Uniforms& uniforms [[buffer(1)]]
) {
  float2 origin = float2(params.grid_col, params.grid_row) * uniforms.cell_size;
  float2 size = float2(params.grid_width, params.grid_height) * uniforms.cell_size;

  float2 corner;
  corner.x = float(vid == 1 || vid == 3);
  corner.y = float(vid == 2 || vid == 3);

  float2 pos = origin + size * corner;
  return uniforms.projection_matrix * float4(pos, 0.0f, 1.0f);
}
```

Two lines removed (the `padding` variable and its addition to `origin`).

#### Pass Criteria

1. The gap between the pink overlay and the viewport border is visually
   symmetric on both sides (left and right).
2. The gap between the pink overlay and the viewport border is visually
   symmetric on both sides (top and bottom).
3. Resizing the terminal preserves the symmetric gap at all window sizes.
4. All other Experiment 1 pass criteria still hold.

#### Result: PASS

Removing the `grid_padding` addition fixed the asymmetry. The pink overlay
shifted slightly to the left — confirming that the padding had been
double-counted. The projection matrix already maps world `(0, 0)` to the grid
origin; adding `grid_padding` on top pushed the overlay rightward by the total
padding amount (configured + blank).

The gap between the pink overlay and the viewport border is now symmetric on all
four sides. Resizing preserves the symmetry at all window sizes.

## Conclusion

The pink texture overlay works. A solid-color GPU quad renders at exact grid
coordinates inside a Ghostty terminal pane, positioned by XPC messages from the
`web` TUI. Resizing is smooth and the overlay tracks the viewport correctly.

### What Was Built

| Component                              | Location                                                          |
| -------------------------------------- | ----------------------------------------------------------------- |
| Metal shader pipeline (`pink_overlay`) | `ts5/src/renderer/metal/shaders.zig`, `shaders.metal`             |
| Overlay state on renderer              | `ts5/src/renderer/generic.zig`                                    |
| C API bridge                           | `ts5/include/ghostty.h`, `ts5/src/apprt/embedded.zig`             |
| Surface overlay methods                | `ts5/src/Surface.zig`                                             |
| XPC Mach service listener              | `ts5/macos/Sources/Ghostty/CompositorXPC.swift`                   |
| Pane ID env var injection              | `ts5/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` |
| launchd plist                          | `ts5/macos/com.termsurf.compositor.plist`                         |
| XPC client (Rust FFI)                  | `web/src/xpc.rs`                                                  |
| Overlay coordinate sending             | `web/src/main.rs`                                                 |

### What Was Proven

1. **Grid-to-pixel conversion works.** The vertex shader positions the quad at
   `grid_pos * cell_size`, and the projection matrix handles padding. This is
   the same pattern used by `cell_text_vertex` and `image_vertex`.

2. **XPC overlay updates are fast enough.** The `web` TUI sends coordinates
   every frame via XPC. The compositor receives them, updates the renderer state
   under `draw_mutex`, and triggers a redraw. No visible latency.

3. **Resize is correct.** When the terminal resizes, the `web` TUI recomputes
   its layout and sends new coordinates. The overlay follows with no stale
   positioning. Cell size doesn't change on resize (it's font-determined), so
   the position stays correct during the brief gap before updated coordinates
   arrive.

4. **Cleanup on disconnect works.** When `web` exits, the XPC connection closes.
   The compositor detects the disconnection and clears the overlay for that
   pane.

### What Remains

The app currently requires `launchctl kickstart` to launch because the main app
IS the XPC Mach service. In ts3, a separate launcher daemon owned the Mach
service and the main app connected as a client, allowing normal launch via
`open`. The next issue should separate the compositor into a standalone daemon
so the app can be launched normally.

The pink quad will eventually be replaced with a real IOSurface texture from
Chromium. The shader pipeline, XPC protocol, and coordinate system are all ready
for this — the only change will be swapping the solid-color fragment shader for
a texture sampler and passing an IOSurface Mach port alongside the grid
coordinates.
