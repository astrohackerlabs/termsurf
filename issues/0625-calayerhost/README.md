+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 625: CALayerHost

## Goal

Replace the `FrameSinkVideoCapturer` with `CALayerHost` so that browser panes
display with the same latency as native Chrome — zero per-frame IPC, zero
application-side compositing, Window Server composites directly from GPU VRAM.

## Chromium Branch

`146.0.7650.0-issue-625` (forked from `146.0.7650.0-issue-616`)

## Background

### The current pipeline

TermSurf runs Chromium out-of-process. A Chromium Profile Server renders web
content and streams frames to the GUI (a Ghostty fork) over XPC. The current
frame delivery path:

```
Chromium renders → compositor composites → FrameSinkVideoCapturer (timer) →
CopyOutputRequest → IOSurface → Mach port via XPC → GUI imports IOSurface →
Metal shader composites → CVDisplayLink vsync → screen
```

This adds 15–25ms of latency versus native Chrome
([Issue 619](619-input-latency.md)). The single biggest contributor is the
`FrameSinkVideoCapturer` — a recording API that runs on its own timer, adding
~5-7ms per frame from timer wait and GPU readback. On top of that, every frame
requires a Mach port transfer over XPC.

### What we learned in Issue 624

[Issue 624](624-chromium-ipc.md) mapped Chromium's full IPC architecture across
three experiments:

1. **Chrome's normal display path uses `CALayerHost`.** The GPU process creates
   a `CAContext`, sends a `ca_context_id` (uint32) once, and the browser process
   creates a `CALayerHost` pointing to that ID. Window Server composites the GPU
   process's CALayer tree directly from VRAM. Zero per-frame IPC, zero pixel
   copies.

2. **Our Chromium Profile Server already produces `CALayerParams` every frame.**
   The capturer is purely observational — the normal display path runs alongside
   it. We've been ignoring `CALayerParams` output at
   `RenderWidgetHostViewMac::AcceleratedWidgetCALayerParamsUpdated()`.

3. **Electron validates this approach.** Electron's normal `BrowserWindow` uses
   stock Chromium — CALayerHost, unmodified, zero custom display code.
   Electron's off-screen rendering mode uses the same `FrameSinkVideoCapturer`
   that TermSurf currently uses, with the same latency penalty. CALayerHost is
   the architecturally correct way to display Chromium content.

### Why CALayerHost

The `ca_context_id` is a uint32 that identifies a `CAContext` in the GPU
process. `CALayerHost` is a `CALayer` subclass that displays a remote
`CAContext` from another process — Window Server handles the compositing. This
is the same mechanism Chrome uses between its own GPU process and browser
process. Adding one more process boundary (Chromium server → TermSurf GUI) is
the same pattern.

**What it eliminates:**

- `FrameSinkVideoCapturer` and `ShellVideoConsumer` (~460 lines)
- Per-frame `CopyOutputRequest` GPU readback
- Per-frame IOSurface Mach port transfer over XPC
- Per-frame Metal texture import and shader compositing in the GUI
- The ~5-7ms capturer timer latency

**What it adds:**

- One XPC message per tab containing a uint32 `ca_context_id` (sent once, not
  per frame)
- A `CALayerHost` sublayer in the GUI, positioned at browser pane coordinates
- Dimming of inactive browser panes via a sibling `CALayer` with
  semi-transparent background

**Architectural change:** Browser pane content moves out of the Metal shader
pipeline. Terminal panes render via Metal (unchanged). Browser panes render via
`CALayerHost` — Window Server composites them. Both coexist as sibling
`CALayer`s in the same NSView layer tree. The GUI still controls positioning and
z-order, but does not touch browser pixels.

### What needs to change

Two sides:

**Chromium Profile Server** (in `chromium/src/`):

- Intercept `CALayerParams` at
  `RenderWidgetHostViewMac::AcceleratedWidgetCALayerParamsUpdated()`
- Extract `ca_context_id` from the params
- Send it over XPC to the GUI (once per tab, re-send on context change)
- Remove `ShellVideoConsumer` and all capturer setup

**TermSurf GUI** (in `gui/`):

- Receive `ca_context_id` over XPC
- Create `CALayerHost` with that `contextId`
- Add as sublayer of the window's content view, positioned at the browser pane's
  pixel coordinates
- Update position/size on pane resize and split changes
- Add dimming overlay `CALayer` for inactive browser panes
- Remove the IOSurface overlay pipeline from the Metal renderer (the pink
  texture proof-of-concept from [Issue 602](602-pink-texture.md), the IOSurface
  import from [Issue 603](603-box-demo.md))

### Open questions

Before implementing, we need to research the TermSurf codebase to understand:

1. **Where does the GUI currently receive IOSurface frames?** Trace the XPC
   message path from reception to Metal rendering. What code handles the Mach
   port, imports the IOSurface, and passes it to the renderer?

2. **How does the Metal renderer currently composite browser content?** What
   shaders, pipeline state, and draw calls are involved? What gets deleted?

3. **Where in the Zig/Swift layer hierarchy should `CALayerHost` live?** The GUI
   uses a `CAMetalLayer` for terminal rendering. `CALayerHost` needs to be a
   sibling or sublayer. What's the current NSView/CALayer structure?

4. **How does pane positioning work?** When a split pane resizes, how do pixel
   coordinates propagate? The `CALayerHost` needs to track these coordinates.

5. **Can `CALayerHost` be created from Zig?** Zig already calls Objective-C
   runtime APIs for Metal. `CALayerHost` is another `CALayer` subclass — same
   pattern. But we need to verify the specific API calls.

## Experiments

### Experiment 1: Map the current pipeline

A source code research experiment — no code changes, no builds. Read the
TermSurf GUI and Chromium Profile Server source to trace the complete frame
delivery pipeline from end to end. The goal is to know exactly what code to
change before writing a single line.

#### Q1: XPC frame reception in the GUI

Trace the path from XPC message arrival to Metal rendering. Where does the GUI
receive IOSurface Mach ports, and how do they reach the renderer?

**Where to look:**

- `gui/src/apprt/embedded.zig` — C API exports. Search for XPC-related functions
  and `overlay` / `iosurface` / `mach_port` references.
- `gui/src/Surface.zig` — the core surface. Search for overlay state, IOSurface
  fields, and any XPC callback handling.
- `gui/src/renderer/Metal.zig` — the Metal renderer. Search for IOSurface
  import, overlay texture, and how the browser frame gets composited.
- `gui/macos/` — Swift code. Search for XPC message handling, Mach port
  extraction, and calls into the Zig C API.

**Deliverable:** A sequence diagram from XPC message arrival to Metal draw call,
with every function, file, and line number labeled. Identify exactly what gets
deleted.

#### Q2: Metal overlay pipeline

Map the Metal shader pipeline that composites browser content. What render pass,
pipeline state, vertex buffer, and fragment shader are involved?

**Where to look:**

- `gui/src/renderer/Metal.zig` — search for `overlay`, `iosurface`, `browser`,
  or any render pass that handles non-terminal content.
- `gui/src/renderer/metal/` — shader files, pipeline definitions. What shaders
  draw the browser overlay?
- Search for `IOSurface` across all of `gui/src/` — every reference needs to be
  cataloged for removal.

**Deliverable:** A list of every Metal resource (pipeline state, shader
function, vertex buffer, texture, render pass) used for the browser overlay.
Each item should be annotated with: keep, delete, or modify.

#### Q3: NSView and CALayer hierarchy

Map the current layer tree. Where does `CAMetalLayer` live? What is the NSView
structure? Where should `CALayerHost` be inserted?

**Where to look:**

- `gui/macos/Sources/` — Swift app code. Search for `NSView`, `CALayer`,
  `CAMetalLayer`, `contentView`, `wantsLayer`, `makeBackingLayer`.
- `gui/src/apprt/embedded.zig` — how the Zig side references the view/layer.
- Look at how Ghostty's Metal renderer gets its `CAMetalLayer` — this is the
  layer that `CALayerHost` needs to sit on top of.

**Deliverable:** A layer tree diagram showing the current NSView/CALayer
hierarchy, with the proposed insertion point for `CALayerHost` marked.

#### Q4: Pane coordinate propagation

Understand how the GUI tracks browser pane pixel coordinates. The `CALayerHost`
frame (position + size) must update when panes resize.

**Where to look:**

- `gui/src/Surface.zig` — how does the surface know its pixel bounds within the
  window? Search for `screen`, `padding`, `grid`, `size`, `viewport`.
- `gui/src/apprt/embedded.zig` — how do resize events propagate?
- The current overlay pipeline — how does it know where to position the browser
  content? Whatever coordinate system it uses is what `CALayerHost` needs.

**Deliverable:** The exact fields and functions that provide browser pane pixel
coordinates (origin x, origin y, width, height) relative to the window.

#### Q5: Chromium server side — CALayerParams interception

Map the Chromium Profile Server's current frame sending code and identify where
to intercept `CALayerParams` instead.

**Where to look:**

- `chromium/src/content/shell/browser/shell_video_consumer.cc` and `.h` — the
  current capturer. Trace how it sends IOSurface Mach ports over XPC.
- `chromium/src/content/shell/browser/shell_browser_main_parts.cc` — where the
  capturer is created and connected to XPC. This is where `CALayerParams`
  interception would go instead.
- `chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm` —
  `AcceleratedWidgetCALayerParamsUpdated()` at line 156. How to hook into this
  callback for our Content Shell.

**Deliverable:** A before/after outline showing: (1) what currently sends frames
over XPC, (2) what replaces it (CALayerParams interception), and (3) what gets
deleted.

#### Verification

Research is complete when we can draw the full before/after picture:

1. **Before:** Complete trace from Chromium capturer → XPC → GUI → Metal draw
   call, with every file and function labeled.
2. **After:** Complete trace from Chromium `CALayerParams` callback → XPC
   (ca_context_id once) → GUI → `CALayerHost` sublayer, with proposed code
   changes for each step.
3. A definitive layer tree diagram showing where `CALayerHost` sits relative to
   `CAMetalLayer`.
4. The coordinate system for positioning `CALayerHost` at browser pane bounds.

#### Results

##### A1: XPC frame reception — the current pipeline

The entire pipeline is Zig — no Swift intermediary. XPC messages arrive on a
serial GCD queue, IOSurface references are stored on the renderer struct behind
a mutex, and `drawFrame` creates a transient Metal texture from the IOSurface
each frame.

```
Chromium Profile Server
  │ XPC: { action: "display_surface", iosurface_port: <mach_port>, pane_id }
  ▼
GCD serial queue "com.termsurf.ghost.xpc"
  │
  ▼
xpc.zig:handleDisplaySurface()                           line 415-436
  ├─ xpc_dictionary_copy_mach_send(msg, "iosurface_port")  → Mach port
  ├─ IOSurfaceLookupFromMachPort(port)                      → IOSurfaceRef
  ├─ mach_port_deallocate(port)
  └─ surface.setOverlayIOSurface(iosurface)
       │
       ▼
Surface.zig:setOverlayIOSurface()                        line 2528-2538
  ├─ draw_mutex.lock()
  ├─ CFRelease(old), CFRetain(new)
  ├─ renderer.overlay_iosurface = iosurface
  ├─ renderer.overlay_surface_changed = true
  └─ queueRender()
       │
       ▼ [renderer thread]
generic.zig:drawFrame()                                  line 1406
  ├─ draw_mutex.lock()                                   line 1422
  ├─ (terminal cells, backgrounds, text, images)
  └─ IOSurface overlay step                              line 1661-1688
       ├─ Texture.fromIOSurface(device, iosurface)       Texture.zig:88-117
       │    (zero-copy MTLTexture from IOSurface GPU memory)
       ├─ Buffer(PinkOverlay).initFill(overlay_params)
       └─ pass.step(pipeline=overlay, textures={tex}, vertex_count=4)
            │
            ▼ [GPU]
shaders.metal:overlay_vertex()                           line 894-909
  origin = (grid_col, grid_row) * cell_size
  size = (pixel_width, pixel_height) from IOSurface
shaders.metal:overlay_fragment()                         line 912-918
  sample IOSurface texture → BGRA color
```

The overlay position is set separately via `set_overlay` XPC message →
`xpc.zig:handleSetOverlay()` (line 267) → `Surface.zig:setOverlay()` (line
2499), which stores grid coordinates in the `PinkOverlay` struct.

##### A2: Metal overlay pipeline — what gets deleted

Every Metal resource for the browser overlay is cleanly isolated from terminal
rendering. Nothing shared needs modification.

**DELETE — browser overlay only:**

| Resource                                        | File                | Lines            |
| ----------------------------------------------- | ------------------- | ---------------- |
| Pipeline `pink_overlay`                         | `metal/shaders.zig` | 45-49            |
| Pipeline `overlay`                              | `metal/shaders.zig` | 50-54            |
| Struct `PinkOverlay`                            | `metal/shaders.zig` | 346-353          |
| Shader `pink_overlay_vertex`                    | `shaders.metal`     | 866-880          |
| Shader `pink_overlay_fragment`                  | `shaders.metal`     | 882-884          |
| Shader `overlay_vertex`                         | `shaders.metal`     | 894-909          |
| Shader `overlay_fragment`                       | `shaders.metal`     | 912-918          |
| MSL structs `PinkOverlayIn`, `OverlayVertexOut` | `shaders.metal`     | 857-864, 889-892 |
| `Texture.fromIOSurface()`                       | `metal/Texture.zig` | 85-117           |
| IOSurface draw call block                       | `generic.zig`       | 1661-1688        |
| Field `pink_overlay`                            | `generic.zig`       | 151              |
| Field `overlay_iosurface`                       | `generic.zig`       | 157              |
| Field `overlay_surface_changed`                 | `generic.zig`       | 160              |
| `setOverlay()`                                  | `Surface.zig`       | 2499-2507        |
| `setOverlayIOSurface()`                         | `Surface.zig`       | 2528-2538        |
| `clearOverlay()`                                | `Surface.zig`       | 2542-2549        |
| `hitTestOverlay()`                              | `Surface.zig`       | 2459-2476        |
| `mapChromiumCursor()`                           | `Surface.zig`       | 2479-2495        |
| `overlay_cursor_type` field                     | `Surface.zig`       | 83               |
| `handleDisplaySurface()`                        | `xpc.zig`           | 415-436          |
| IOSurface externs                               | `xpc.zig`           | 58-64            |
| `ghostty_surface_is_overlay_forwarding`         | `embedded.zig`      | 1749-1752        |

**KEEP — terminal rendering (unaffected):**

- Pipelines: `bg_color`, `cell_bg`, `cell_text`, `image`, `bg_image`
- `Uniforms` struct (read-only by overlay shaders, unchanged by removal)
- `IOSurfaceLayer` — the terminal's presentation layer, NOT the browser overlay
- `Overlay.zig` — Ghostty's inspector debug overlay (unrelated to browser)

##### A3: NSView/CALayer hierarchy and CALayerHost insertion point

The current layer tree:

```
NSWindow (TerminalWindow)
  └─ contentView → TerminalViewContainer (NSView)
       └─ NSHostingView (hosts SwiftUI)
            └─ TerminalSplitTreeView → SplitView(s)
                 └─ [per split pane]
                      └─ SurfaceRepresentable (NSViewRepresentable)
                           └─ SurfaceScrollView (NSView)
                                └─ NSScrollView → NSClipView → documentView
                                     └─ SurfaceView (NSView, layer-hosting)
                                          └─ layer: IOSurfaceLayer (CALayer subclass)
                                               contents = terminal IOSurface
                                               (no sublayers)
```

**Critical detail:** SurfaceView is a **layer-hosting view** — Zig sets `layer`
BEFORE `wantsLayer = true` (`Metal.zig:124-125`), giving the application full
control over the layer tree. AppKit does not create a default backing layer.

**Proposed insertion — CALayerHost as sublayer of IOSurfaceLayer:**

```
SurfaceView (layer-hosting)
  └─ layer: IOSurfaceLayer (terminal content)
       ├─ sublayer: CALayerHost (browser content)    ← NEW
       │    contextId = Chromium's ca_context_id
       │    frame = overlay pixel rect
       └─ sublayer: CALayer (dimming overlay)         ← NEW (inactive panes)
            backgroundColor = (0, 0, 0, 0.4)
            frame = same as CALayerHost
```

This works because:

- Layer-hosting means we control the layer tree from Zig
- `CALayerHost` as a sublayer composites ON TOP of terminal content
- Window Server handles compositing — no Metal draw calls for browser content
- The dimming layer sits above the CALayerHost for inactive pane dimming

Key files for the insertion point:

| What                    | File                       | Line    |
| ----------------------- | -------------------------- | ------- |
| IOSurfaceLayer creation | `Metal.zig`                | 111     |
| Layer-hosting setup     | `Metal.zig`                | 124-125 |
| IOSurfaceLayer subclass | `metal/IOSurfaceLayer.zig` | 138-183 |
| SurfaceView class       | `SurfaceView_AppKit.swift` | 10      |
| NSView pointer to Zig   | `SurfaceView.swift`        | 694-695 |

##### A4: Pane coordinate system

The overlay uses **grid coordinates** (column/row in terminal cells) as source
of truth, converted to pixels at render time via `cell_size`.

**Current conversion (shader-side):**

```
origin_px = (grid_col, grid_row) * cell_size
size_px = (pixel_width, pixel_height)  // from IOSurface dimensions
```

**For CALayerHost, the same conversion applies but in Zig:**

```zig
const x = pink_overlay.grid_col * cell_width;
const y = pink_overlay.grid_row * cell_height;
const w = pixel_width;  // from Chromium's reported size
const h = pixel_height;
ca_layer_host.setFrame(.{ .origin = .{ x, y }, .size = .{ w, h } });
```

**Critical finding: each surface has its own coordinate space starting at (0,
0).** The `SurfaceView` has no knowledge of its position within the window.
Overlay grid coordinates are relative to the surface's own grid. Since
`CALayerHost` will be a sublayer of IOSurfaceLayer (which IS the surface's root
layer), its frame is also relative to the surface — no window-relative
coordinates needed.

**Resize propagation:**

1. SwiftUI detects size change → `sizeDidChange()` →
   `ghostty_surface_set_size()` → renderer updates uniforms
2. `web` TUI detects terminal resize → sends new `set_overlay` with updated grid
   coordinates → XPC handler recomputes pixel dimensions → sends `resize` to
   Chromium
3. CALayerHost frame updates when new grid coordinates arrive (same path as
   current overlay)

**Key fields:**

| What                   | How                                      | Location           |
| ---------------------- | ---------------------------------------- | ------------------ |
| Grid coordinates       | `pink_overlay.grid_col/row/width/height` | `generic.zig:151`  |
| Cell size (pixels)     | `grid_metrics.cell_width/height`         | `generic.zig:110`  |
| Surface size (pixels)  | `self.size.screen.width/height`          | `Surface.zig:150`  |
| Content scale (Retina) | `rt_surface.getContentScale()`           | `Surface.zig:2474` |

##### A5: Chromium server side — before and after

**BEFORE — capturer path:**

`ShellVideoConsumer` (`shell_video_consumer.cc/h`) creates a
`ClientFrameSinkVideoCapturer` at 120fps. On every captured frame:

```
OnFrameCaptured()                                        line 258-332
  ├─ Extract IOSurfaceRef from gpu_memory_buffer_handle  line 270
  ├─ IOSurfaceCreateMachPort(io_surface)                 line 277
  ├─ Build XPC dict: { action: "display_surface",
  │    iosurface_port: <mach_port>, pane_id }
  ├─ xpc_connection_send_message(tab_connection, msg)    line 285
  └─ mach_port_deallocate(port)                          line 287
```

Created in `shell_browser_main_parts.cc:CreateTab()` (line 354-405): creates
consumer, sets pane ID and size, opens per-tab XPC connection, sends
`tab_ready`, hands connection to consumer.

**AFTER — CALayerParams interception:**

Replace the capturer with a callback on
`AcceleratedWidgetCALayerParamsUpdated()`. Two options:

- **Option A:** Add a callback on `RenderWidgetHostViewMac` (similar to existing
  `SetCursorChangedCallback` pattern at line 394) that fires with
  `ca_context_id` when `AcceleratedWidgetCALayerParamsUpdated()` is called.
- **Option B:** After `RenderViewReady()`, query
  `browser_compositor_->GetLastCALayerParams()->ca_context_id` and send it once.

New XPC message format:

```
Sent once per tab (and on ca_context_id change):
{
  "action": "ca_context",
  "ca_context_id": <uint32>,
  "pane_id": "<uuid>",
  "pixel_width": <uint64>,
  "pixel_height": <uint64>,
  "scale_factor": "<string>"    // XPC has no float type
}
```

**DELETE:**

| What                               | File                               | Lines              |
| ---------------------------------- | ---------------------------------- | ------------------ |
| `shell_video_consumer.cc`          | `chromium_profile_server/browser/` | entire (347 lines) |
| `shell_video_consumer.h`           | `chromium_profile_server/browser/` | entire (113 lines) |
| BUILD.gn entries                   | `chromium_profile_server/BUILD.gn` | 201-202            |
| Consumer creation in `CreateTab()` | `shell_browser_main_parts.cc`      | 354-358, 387-388   |
| `ResizeCapture()`                  | `shell_browser_main_parts.cc`      | 411-446            |
| `resize` XPC handler               | `shell_browser_main_parts.cc`      | 219-227            |

**PRESERVE (move out of ShellVideoConsumer):**

The video consumer currently doubles as a `WebContentsObserver` for navigation
and loading state. These notifications must move to a simpler observer:

- `DidFinishNavigation` → URL change notifications (lines 90-144)
- `DidStartLoading`/`DidStopLoading`/`LoadProgressChanged` (lines 161-193)
- Cursor change callback (via `SetCursorChangedCallback`, line 394)

#### Conclusion

The full before/after picture is clear:

**Before:** Chromium capturer (120fps timer) → IOSurface → Mach port → XPC (per
frame) → Zig imports IOSurface → Metal texture → shader composites → screen.
~460 lines of capturer code, per-frame GPU readback, per-frame Mach port
transfer.

**After:** Chromium `AcceleratedWidgetCALayerParamsUpdated()` → extract
`ca_context_id` (uint32) → XPC (once) → Zig creates `CALayerHost` as sublayer of
IOSurfaceLayer → Window Server composites from GPU VRAM → screen. ~50 lines of
interception code, zero per-frame IPC.

The layer hierarchy is clean: `IOSurfaceLayer` (terminal) with `CALayerHost`
(browser) as sublayer. Each surface has its own coordinate space starting at (0,
0), so CALayerHost frame coordinates match the current grid-to-pixel conversion.
No window-relative math needed.

The Chromium side needs: delete `ShellVideoConsumer`, add a `CALayerParams`
callback, move `WebContentsObserver` notifications to a simpler class. The GUI
side needs: delete the entire Metal overlay pipeline, add `CALayerHost` creation
and positioning in Zig via Objective-C runtime calls.

### Experiment 2: Implement CALayerHost

Replace the `FrameSinkVideoCapturer` pipeline with `CALayerHost`. Two sides:
Chromium Profile Server sends `ca_context_id` instead of IOSurface Mach ports,
GUI creates a `CALayerHost` sublayer instead of Metal-compositing an IOSurface
texture.

#### Step 1: Chromium — intercept CALayerParams and send ca_context_id

**In `shell_browser_main_parts.cc`:**

1. Add a `CALayerParams` callback on `RenderWidgetHostViewMac`. Follow the
   existing `SetCursorChangedCallback` pattern (line 394). When
   `AcceleratedWidgetCALayerParamsUpdated()` fires, extract `ca_context_id` from
   `GetLastCALayerParams()`. If it differs from the last sent value, send an XPC
   message:

   ```
   { "action": "ca_context", "ca_context_id": <uint32>, "pane_id": "<uuid>" }
   ```

2. In `CreateTab()`, replace the `ShellVideoConsumer` creation (lines 354-405)
   with the new callback registration. Keep the per-tab XPC connection and
   `tab_ready` message — those are still needed.

3. Remove the `resize` XPC handler (lines 219-227) and `ResizeCapture()` (lines
   411-446). Resize is handled automatically — the compositor produces new
   `CALayerParams` at the new size, and the GUI updates the `CALayerHost` frame
   when `set_overlay` arrives with new grid coordinates.

**Move `WebContentsObserver` notifications out of `ShellVideoConsumer`:**

4. Create a lightweight `ShellTabObserver` class (or add observer methods
   directly to `ShellBrowserMainParts`) that observes the `WebContents` for:
   - `DidFinishNavigation` → send `url_changed` over XPC
   - `DidStartLoading` / `DidStopLoading` / `LoadProgressChanged` → send
     `loading_state` over XPC
   - `DidFailLoad` → send `loading_state` with error

   The cursor change callback (`SetCursorChangedCallback`) stays on
   `RenderWidgetHostImpl` — it's independent of the video consumer.

**Delete `ShellVideoConsumer`:**

5. Delete `shell_video_consumer.cc` (347 lines) and `shell_video_consumer.h`
   (113 lines). Remove their entries from `BUILD.gn` (lines 201-202). Remove the
   `#include` and forward declaration from `shell_browser_main_parts`.

#### Step 2: GUI — receive ca_context_id and create CALayerHost

**In `xpc.zig`:**

6. Add a `handleCAContext()` handler for the `"ca_context"` action. Extract
   `ca_context_id` (uint32) from the XPC message. Look up the pane by `pane_id`.
   Call a new method on the surface to set the CALayerHost.

**In `Surface.zig`:**

7. Add `setCAContextId(context_id: u32)` method. This replaces
   `setOverlayIOSurface()`. Behind `draw_mutex`:
   - Store the `context_id` on the renderer
   - Call into the renderer to create/update the `CALayerHost`

**In `Metal.zig` or a new `CALayerHost.zig`:**

8. Create the `CALayerHost` via Objective-C runtime calls:
   ```
   objc_getClass("CALayerHost")
   objc_msgSend(class, "alloc")
   objc_msgSend(instance, "init")
   objc_msgSend(instance, "setContextId:", context_id)
   ```
   Add it as a sublayer of the IOSurfaceLayer:
   ```
   objc_msgSend(iosurface_layer, "addSublayer:", ca_layer_host)
   ```

9. Set the `CALayerHost` frame to the browser pane pixel coordinates:
   ```
   frame.origin.x = grid_col * cell_width
   frame.origin.y = grid_row * cell_height
   frame.size.width = pixel_width
   frame.size.height = pixel_height
   ```
   This frame updates whenever `set_overlay` arrives with new grid coordinates.

#### Step 3: GUI — delete the Metal overlay pipeline

10. Delete from `shaders.metal`: `PinkOverlayIn` struct, `OverlayVertexOut`
    struct, `pink_overlay_vertex`, `pink_overlay_fragment`, `overlay_vertex`,
    `overlay_fragment`.

11. Delete from `metal/shaders.zig`: `pink_overlay` and `overlay` pipeline
    definitions, `PinkOverlay` struct.

12. Delete from `metal/Texture.zig`: `fromIOSurface()` and IOSurface extern
    declarations.

13. Delete from `generic.zig`: the IOSurface overlay draw call block (lines
    1661-1688), fields `pink_overlay`, `overlay_iosurface`,
    `overlay_surface_changed`.

14. Delete from `Surface.zig`: `setOverlay()`, `setOverlayIOSurface()`,
    `clearOverlay()`, `hitTestOverlay()`, `mapChromiumCursor()`,
    `overlay_cursor_type` field.

15. Delete from `xpc.zig`: `handleDisplaySurface()`, IOSurface externs.

16. Delete from `embedded.zig`: `ghostty_surface_is_overlay_forwarding`.

#### Step 4: GUI — update overlay positioning for CALayerHost

17. Modify `handleSetOverlay()` in `xpc.zig` to update the `CALayerHost` frame
    instead of storing `PinkOverlay` grid coordinates on the renderer. The
    conversion is the same (grid × cell_size = pixels), but the target is
    `CALayerHost.frame` instead of a shader uniform buffer.

18. Update `hitTestOverlay()` replacement — hit testing against the
    `CALayerHost` frame rect instead of the `PinkOverlay` grid coordinates. The
    logic is the same, just reading from the layer frame.

19. Update `clearOverlay()` replacement — remove the `CALayerHost` sublayer when
    the browser pane closes.

#### Verification

1. Build Chromium Profile Server
   (`autoninja -C out/Default
   chromium_profile_server`).
2. Build TermSurf GUI (`cd gui && zig build`).
3. Launch the app, open a terminal, type `web google.com`.
4. **Pass criteria:**
   - Web page renders in the browser pane at the correct position
   - No visible lag increase compared to the capturer path (should be noticeably
     better)
   - Text selection tracks the cursor without visible delay
   - Scrolling feels responsive
   - Pane resize works — browser content resizes with the pane
   - Multiple panes with different profiles work
   - Closing a browser pane cleans up the CALayerHost
5. **Bonus verification:**
   - Compare text selection latency side-by-side with native Chrome
   - Verify no per-frame XPC messages in Console.app / log stream

**Result:** Fail

Both builds succeed — Chromium Profile Server and TermSurf GUI compile without
errors. The app launches, a terminal opens, and `web news.ycombinator.com` loads
the page. The CALayerHost receives the `ca_context_id` and the web content is
visible. So the core pipeline works: Chromium → XPC (ca_context_id once) →
CALayerHost → Window Server compositing.

However, the CALayerHost is positioned catastrophically wrong. Instead of
appearing at the overlay viewport position (near the top of the surface), the
web content is pushed to the bottom of the screen with only the top ~10%
visible. The error is so severe that no other functionality could be tested.

**Root cause:** CALayer on macOS uses Y=0 at the **bottom** (Y increases
upward), but the terminal grid has row 0 at the **top** (Y increases downward).
`updateCALayerHostFrame()` in `Metal.zig` naively sets
`frame.origin.y = grid_row * cell_height`, which places a small grid row (like
row 1–2) at a small Y value — near the bottom in CALayer coordinates. The fix is
to flip Y: `flipped_y = parent_layer_height - y - h`.

#### Conclusion

The architecture is sound — CALayerHost works for displaying Chromium content
from another process, the XPC one-shot `ca_context_id` delivery works, and
Window Server composites the content. The implementation has a coordinate system
bug that prevents testing any other functionality. Experiment 3 should fix the Y
coordinate flip and re-verify.

### Experiment 3: Fix CALayerHost Y coordinate

Fix the Y-axis inversion bug from Experiment 2 and re-verify the full
CALayerHost pipeline.

#### Problem

CALayer on macOS uses a bottom-left origin (Y=0 at the bottom, Y increases
upward). The terminal grid uses a top-left origin (row 0 at the top, row
increases downward). `updateCALayerHostFrame()` in `Metal.zig` sets
`frame.origin.y = grid_row * cell_height` without flipping, placing the
CALayerHost near the bottom of the parent layer instead of near the top.

#### Fix

Two options:

- **Option A — flip Y manually.** Read the parent layer's bounds height and
  compute `flipped_y = parent_height - y - h`. Requires passing the surface
  height into `updateCALayerHostFrame()`.

- **Option B — set `geometryFlipped = true` on the IOSurfaceLayer.** This tells
  Core Animation that sublayers use a top-left coordinate system (Y=0 at top, Y
  increases downward). Only affects sublayer geometry, not the layer's own
  `contents` rendering. No Y math needed —
  `frame.origin.y = grid_row *
  cell_height` works as-is.

Option B is cleaner (one property, no math), but it could have side effects on
terminal rendering if any other code assumes the default bottom-left geometry.
Since no existing code adds sublayers to the IOSurfaceLayer (CALayerHost is the
first), Option B should be safe. However, Option A is more conservative and
explicit.

#### Changes

**In `Metal.zig` — `updateCALayerHostFrame()`:**

1. Add the surface screen height as a parameter (from `self.size.screen.height`
   on the GenericRenderer).
2. Flip Y: `flipped_y = parent_height - y - h`.
3. Remove the incorrect comment claiming the coordinate system is already
   top-left.

**In `generic.zig` — `updateCALayerHostFrame()`:**

4. Pass `self.size.screen.height` to the Metal call.

#### Verification

Same as Experiment 2:

1. Build TermSurf GUI (`cd gui && zig build`).
2. Launch the app, open a terminal, type `web news.ycombinator.com`.
3. **Pass criteria:**
   - Web page renders at the correct position (aligned with the TUI viewport)
   - Scrolling feels responsive
   - Text selection tracks the cursor
   - Pane resize works — browser content resizes with the pane
   - Multiple panes with different profiles work
   - Closing a browser pane cleans up the CALayerHost
4. **Bonus verification:**
   - Compare text selection latency side-by-side with native Chrome
   - Verify no per-frame XPC messages in Console.app / log stream

**Result:** Fail

The Y flip (`flipped_y = screen_height - y - h`) had zero visible effect — the
CALayerHost is in the exact same wrong position as Experiment 2. The
`screen_height` value from the renderer may not match the parent layer's actual
bounds height, or the coordinate system assumption is wrong in a different way
than expected. The problem needs deeper investigation — either by logging the
actual values being set, inspecting the layer hierarchy with Xcode's view
debugger, or testing with hardcoded frame values to understand how the parent
layer's coordinate system actually works.

#### Conclusion

The naive Y flip doesn't work. The coordinate system issue is not a simple
top-vs-bottom inversion, or the values being passed are incorrect. Experiment 4
should add diagnostic logging and/or use a different approach to understand what
coordinates the CALayerHost actually needs.

### Experiment 4: Diagnose CALayerHost positioning

The Y flip in Experiment 3 had **zero visible effect**. That's the key clue —
not "slightly wrong" or "flipped the other way," but zero change. This narrows
the possible causes significantly. This experiment adds diagnostic logging to
identify which hypothesis is correct, then fixes the root cause.

#### Hypotheses

Ranked by how well they explain "zero effect from Y flip":

1. **`setProperty("frame", frame)` silently fails for CGRect.** CGRect is a
   32-byte struct. The zig-objc `setProperty` method uses `objc_msgSend` under
   the hood. On ARM64, `objc_msgSend` can pass structs up to a certain size in
   registers, but CGRect (4 doubles = 32 bytes) may exceed that limit and need a
   different calling convention. If the struct data arrives as garbage, the
   frame is never actually set. This would explain zero effect from any value
   change.

2. **The function isn't being called or values are all zero.** If
   `overlay_grid_*` fields are still zero when `updateCALayerHostFrame` runs
   (race condition: `ca_context_id` arrives before overlay coordinates), the
   computed frame is a zero-size rect. Flipping a zero rect produces a zero
   rect.

3. **CALayerHost ignores the host-side `frame`.** CALayerHost displays a remote
   `CAContext`'s layer tree. The remote layer may impose its own size. Setting
   `frame` on the host might only set a clip rect (or be ignored entirely), with
   the actual positioning controlled by the remote layer's geometry.

4. **Physical pixels vs logical points.** CALayer frames use points (logical
   coordinates). If `cell_width`, `cell_height`, and `screen_height` are in
   physical pixels (2x on Retina), everything is doubled — the frame is 2x too
   large and 2x offset. The Y flip math would produce a different number, but if
   the frame is already larger than the parent, both positions overflow the
   visible area in the same way.

5. **Superlayer coordinate weirdness.** The parent IOSurfaceLayer may have
   transforms, bounds offsets, or `geometryFlipped = YES` already set. If it's
   already flipped, our flip double-flips back to the original wrong value. But
   "zero change" is harder to explain this way unless both values happen to
   overflow identically.

#### Diagnostic changes

Add `log.info` calls to print every value at every stage. This tells us exactly
which hypothesis is correct.

**In `Metal.zig` — `setCALayerHostContextId()`:**

1. After creating or updating the CALayerHost, log the host's actual `frame` as
   read back from the object (to confirm `setProperty("frame", ...)` works).

**In `Metal.zig` — `updateCALayerHostFrame()`:**

2. Log all input values: `grid_col`, `grid_row`, `grid_width`, `grid_height`,
   `cell_width`, `cell_height`, `screen_height`.
3. Log the computed frame: `x`, `y` (before flip), `flipped_y`, `w`, `h`.
4. After calling `setProperty("frame", frame)`, read the frame back from the
   host object and log it. If the read-back frame differs from what we set,
   hypothesis #1 (setProperty fails for CGRect) is confirmed.
5. Also log the parent IOSurfaceLayer's `bounds` and `frame` to check for
   unexpected transforms or sizes.

**In `generic.zig` — `updateCALayerHostFrame()`:**

6. Log whether `ca_layer_host` is non-null (to confirm the function is actually
   reached).

**In `generic.zig` — `setCALayerHostContextId()`:**

7. Log the order of operations — whether `ca_context_id` arrives before or after
   `set_overlay` grid coordinates.

#### Hardcoded sanity check

As a parallel diagnostic, temporarily hardcode a known-good frame value:

8. In `updateCALayerHostFrame()`, before the normal logic, add a hardcoded
   override:
   ```
   frame = { origin: { x: 50, y: 50 }, size: { width: 400, height: 300 } }
   ```
   If the hardcoded frame also has no effect, the problem is #1 (setProperty
   fails) or #3 (CALayerHost ignores frame). If the hardcoded frame works
   correctly, the problem is in the computed values (#2 or #4).

#### Verification

1. Build TermSurf GUI (`cd gui && zig build`).
2. Launch the app, open a terminal, type `web news.ycombinator.com`.
3. Check TermSurf's log output for all the diagnostic values.
4. **Diagnosis complete when we can answer:**
   - Are the input values (grid coords, cell size, screen height) reasonable?
   - Does the frame read-back match what we set? (Tests hypothesis #1)
   - Does the hardcoded frame produce correct positioning? (Tests #1 vs #2/#4)
   - What is the parent layer's bounds? (Tests #5)
   - Is `updateCALayerHostFrame` called at all? (Tests #2)
5. Once the root cause is identified, fix it and verify the CALayerHost appears
   at the correct position.

**Result:** Fail

Diagnostic logging eliminated hypotheses #1 and #2, but did not identify the
root cause of the ~400px Y offset.

**What the logs showed:**

```
set_overlay: col=1 row=4 w=120 h=35
setCALayerHostContextId=3551625741 grid=(1.0,4.0,120.0,35.0) host=null
created CALayerHost contextId=3551625741
CALayerHost frame after setContextId: x=0.0 y=0.0 w=0.0 h=0.0
updateCALayerHostFrame inputs: grid=(1.0,4.0,120.0,35.0) cell=(13,29) screen_h=1200
computed frame: x=13.0 y=116.0 flipped_y=69.0 w=1560.0 h=1015.0
parent IOSurfaceLayer bounds: x=0.0 y=0.0 w=800.0 h=600.0
parent IOSurfaceLayer frame: x=0.0 y=0.0 w=800.0 h=600.0 scale=2.0
CALayerHost frame readback: x=50.0 y=50.0 w=400.0 h=300.0
```

**Hypotheses eliminated:**

- **#1 (`setProperty` fails for CGRect):** Eliminated. The frame readback
  matches what was set. `setProperty("frame", frame)` works correctly for CGRect
  structs.
- **#2 (function not called / values zero):** Eliminated. The function is called
  with valid grid coordinates `(1, 4, 120, 35)` and non-null host pointer. The
  first `updateCALayerHostFrame` call (from `set_overlay`) has
  `ca_layer_host=
  null` because `ca_context_id` hasn't arrived yet, but the
  second call (from `setCALayerHostContextId`) succeeds with valid data.

**Key finding: the frame DOES control positioning, but there's a ~400px
unexplained Y offset.** Adding 500 to the Y value pushed the content up by
approximately that amount. So the `frame` property works — the computed values
just don't account for a large offset introduced by the remote CAContext's layer
tree. The Chromium GPU process positions its layers within the CAContext at some
offset (likely from the "window" geometry it thinks it's rendering to), and the
CALayerHost frame is relative to the parent IOSurfaceLayer, not to the remote
content's coordinate space.

**Additional issue: physical pixels vs logical points.** The cell dimensions
(`cell_width=13`, `cell_height=29`) and `screen_height=1200` are in physical
pixels, but CALayer frames use logical points. The parent IOSurfaceLayer is
800x600 points with `contentsScale=2.0` (so 1600x1200 physical). The computed
frame of 1560x1015 is nearly 2x the parent's 800x600 point dimensions. This
needs to be divided by the scale factor.

**What remains unsolved:** The ~400px Y offset. This is far larger than a 2x
scale factor error. The remote CAContext's layer tree has a built-in offset that
we don't understand yet. Changing the Y value on the host frame does move the
content, proving the frame works, but we don't know what offset to apply or
where it comes from.

#### Conclusion

The diagnostic confirmed that `setProperty("frame", frame)` works and that the
function is called with valid data. The frame does control positioning — adding
Y offset moves the content. But there are two problems: (1) physical pixel
values are being used where logical points are needed (2x error), and (2) an
unexplained ~400px Y offset from the remote CAContext that dwarfs the scale
issue. The pixel/point fix is straightforward. The ~400px offset needs further
investigation — either on the Chromium side (why the GPU process positions
content at that offset) or by probing the remote layer tree's geometry from the
GUI side.

### Experiment 5: Find the source of the CAContext offset

The CALayerHost frame works — changing Y moves the content — but the remote
CAContext from Chromium's GPU process positions web content at a ~400px Y offset
and a smaller X offset. This experiment investigates the Chromium side to find
where these offsets come from.

#### Theory

In normal Chrome, the GPU process creates a CAContext for the entire window. The
web content is not at (0, 0) in that CAContext — it's offset below the browser
chrome (tab strip 36px, toolbar 40px, bookmarks bar 28px, etc.). The CALayerHost
fills the entire NSView, so these internal offsets are correct because the
NSView IS the window.

The Chromium Profile Server uses content_shell, which creates its own NSWindow
with a shell toolbar (URL bar). Even though we don't display this window to the
user, the GPU process still creates a CAContext with the full window geometry.
The web content sits below the shell toolbar and window title bar within the
CAContext's layer tree. That's the 400px offset.

The X offset is likely from window padding or the shell's view insets.

#### Research

Search the Chromium source for how content_shell sets up its window, views, and
web content positioning. The goal is to understand exactly what creates the
offset, and whether we can eliminate it.

**R1: Shell window creation.**

How does content_shell create its NSWindow? What size is it? Does it have a
title bar? Look at `Shell::CreateShell()` and platform-specific
`Shell::PlatformCreateWindow()` in `content/shell/browser/shell_mac.mm`.

**R2: Shell view hierarchy.**

What NSViews exist inside the shell window? Is there a toolbar view, URL bar, or
status bar that offsets the web content view? Look at
`Shell::PlatformSetContents()` and how the `WebContents` view is added to the
window.

**R3: RenderWidgetHostViewMac positioning.**

How is the `RenderWidgetHostViewMac` (the view that hosts web content)
positioned within the shell window? What is its frame relative to the window?
This offset is what the GPU process uses when building the CAContext layer tree.

**R4: CAContext layer tree structure.**

How does the GPU process build the CAContext? Does it use the window's full
frame or just the content view's frame? Look at `ui::CARendererLayerTree` and
`BrowserCompositorMac` to understand what coordinates the CAContext's layers
use.

**R5: Can we eliminate the offset?**

Options to investigate:

- Make the shell window have no title bar and no toolbar (so the web content
  view is at (0, 0) in the window)
- Create a minimal NSWindow/NSView just for the RenderWidgetHostViewMac with no
  parent chrome
- Set the RenderWidgetHostViewMac's frame origin to (0, 0) in its superview
- Intercept the CAContext layer tree and reposition the content layer

#### Verification

Research is complete when we can answer:

1. What is the exact shell window size and configuration?
2. What views exist between the window and the web content view, and what are
   their frames?
3. Where in the code are these views and offsets created?
4. Which of the R5 options is the most practical fix?

#### Results

##### R1: Shell window creation

**File:**
`chromium/src/content/chromium_profile_server/browser/shell_platform_delegate_mac.mm`
(lines 134–214)

The shell creates a standard macOS window with title bar and toolbar:

```objc
NSUInteger style_mask = NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                        NSWindowStyleMaskMiniaturizable |
                        NSWindowStyleMaskResizable;
NSWindow* window =
    [[NSWindow alloc] initWithContentRect:content_rect
                                styleMask:style_mask
                                  backing:NSBackingStoreBuffered
                                    defer:NO];
```

- **Title:** `"Chromium Profile Server"` (line 86)
- **Title bar:** Yes (`NSWindowStyleMaskTitled`) — adds ~28px
- **Toolbar:** 24px (`kURLBarHeight`) with Back/Forward/Reload/Stop buttons and
  a URL text field (lines 177–204). Only hidden if
  `--chromium-profile-server-hide-toolbar` switch is set.
- **Default size:** 800×600 content area + 24px toolbar = 800×624 total content
  view
- **Visibility:** Hidden if `--hidden` flag is set (lines 207–210), otherwise
  made key and ordered front

##### R2: Shell view hierarchy

```
NSWindow (800×624, titled)
  └── contentView (NSView, auto-created by NSWindow)
       ├── NSButton (Back, 72×24)
       ├── NSButton (Forward, 72×24)
       ├── NSButton (Reload, 72×24)
       ├── NSButton (Stop, 72×24)
       ├── NSTextField (URL bar, remaining width × 24)
       └── web_contents view (800×600, added in SetContents())
```

**File:** `shell_platform_delegate_mac.mm`, `SetContents()` (lines 228–244):

```objc
NSView* web_view = shell->web_contents()->GetNativeView().GetNativeNSView();
web_view.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
NSRect frame = window.contentView.bounds;
if (!Shell::ShouldHideToolbar()) {
    frame.size.height -= kURLBarHeight;  // 24px subtracted
}
web_view.frame = frame;
```

The web content view sits at origin `(0, 0)` in the contentView (bottom-left in
AppKit coordinates), with the toolbar 24px above it. The title bar adds another
~28px above that.

##### R3: RenderWidgetHostViewMac positioning

The `RenderWidgetHostViewMac` is the web content view (`web_view` above). Its
frame is set to `contentView.bounds` minus the 24px toolbar height. It's
positioned at `(0, 0)` in the contentView — the bottom of the window in AppKit's
bottom-left coordinate system.

The GPU process receives the view's bounds through the compositor pipeline. The
quads rendered by the compositor use **render-target-relative coordinates**, not
window-relative. But the render target's size includes the full window geometry
that the compositor is aware of.

##### R4: CAContext layer tree structure

**File:** `chromium/src/ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm`
(lines 40–67)

The GPU process creates the CAContext with a `geometryFlipped` root layer:

```objc
root_ca_layer_ = [[CALayer alloc] init];
root_ca_layer_.geometryFlipped = YES;  // Key: Y=0 at top
root_ca_layer_.opaque = YES;

CGSConnectionID connection_id = CGSMainConnectionID();
ca_context_ = [CAContext contextWithCGSConnection:connection_id options:@{}];
ca_context_.layer = root_ca_layer_;
```

The `geometryFlipped = YES` means the CAContext's coordinate system has Y=0 at
the **top**, matching standard display coordinates. Content layers are
positioned using pixel coordinates converted to DIPs (divided by scale factor).

**File:** `chromium/src/ui/accelerated_widget_mac/ca_renderer_layer_tree.mm`
(lines 1220–1225)

Content layers are positioned in DIPs:

```cpp
gfx::RectF dip_rect = gfx::RectF(rect_);
dip_rect.Scale(1 / tree()->scale_factor_);
ca_layer_.position = CGPointMake(dip_rect.x(), dip_rect.y());
ca_layer_.bounds = CGRectMake(0, 0, dip_rect.width(), dip_rect.height());
```

**Chromium's browser process also hosts the CALayerHost in a `geometryFlipped`
layer.** From `display_ca_layer_tree.mm` (lines 123–153):

```objc
CALayerHost* new_remote_layer = [[CALayerHost alloc] init];
new_remote_layer.anchorPoint = CGPointZero;
new_remote_layer.contextId = ca_context_id;
[maybe_flipped_layer_ addSublayer:new_remote_layer];
```

The `maybe_flipped_layer_` has `geometryFlipped = YES`. So in normal Chrome, the
full chain is: GPU process (`geometryFlipped` root) → CAContext → browser
process (`geometryFlipped` host layer) → CALayerHost. Both sides agree on Y=0 at
top.

##### R5: Source of the offset

**Two causes identified:**

1. **Missing `geometryFlipped`.** Our IOSurfaceLayer does NOT have
   `geometryFlipped = YES`. Chromium's CAContext root layer has
   `geometryFlipped = YES` (Y=0 at top). In normal Chrome, the browser-side host
   layer also has `geometryFlipped = YES`, so both sides agree. In TermSurf, the
   CALayerHost sits in a non-flipped parent (Y=0 at bottom), causing the entire
   content to render with inverted Y positioning. This is the dominant cause of
   the ~400px offset.

2. **Shell window chrome.** The title bar (~28px) and toolbar (24px) push the
   web content view down by ~52px in the window. The GPU process's compositor
   includes this offset in the CAContext layer tree. Even after fixing the
   `geometryFlipped` issue, there will be a residual ~52px offset from the
   phantom window chrome.

3. **Physical pixels vs logical points.** Cell dimensions and screen height are
   passed in physical pixels but CALayer frames use points. This is a 2x error
   on Retina displays, separate from the offset issue.

**Chromium's own CALayerHost setup also sets `anchorPoint = CGPointZero`** — we
should do the same to match their behavior.

#### Conclusion

The ~400px Y offset has two root causes: (1) our parent layer lacks
`geometryFlipped = YES`, causing a full Y-axis inversion relative to what
Chromium expects, and (2) the shell window's title bar + toolbar add ~52px of
phantom offset in the CAContext. The X offset is likely from the window frame or
content view padding. The fix is: set `geometryFlipped = YES` on the parent
layer (or the CALayerHost), hide or eliminate the shell toolbar, set
`anchorPoint = CGPointZero` on the CALayerHost, and convert pixel values to
points by dividing by the scale factor.

### Experiment 6: Fix CALayerHost positioning

Apply the three fixes identified in Experiment 5: `geometryFlipped`, shell
window chrome elimination, and pixel-to-point conversion.

#### Changes

**Fix 1: `geometryFlipped` on CALayerHost (GUI side)**

In `Metal.zig` — `setCALayerHostContextId()`:

1. After creating the CALayerHost, set `geometryFlipped = YES` on it. This makes
   the CALayerHost's coordinate system match Chromium's CAContext (Y=0 at top).
   Do NOT set it on the IOSurfaceLayer itself — that could break terminal
   rendering. Setting it on the CALayerHost only affects the remote content's
   sublayer geometry.

2. Set `anchorPoint = CGPointZero` on the CALayerHost, matching Chromium's own
   `DisplayCALayerTree::GotCALayerFrame()` pattern.

**Fix 2: Remove shell window chrome (Chromium side)**

In `shell_platform_delegate_mac.mm` — `CreatePlatformWindow()`:

3. Change the window style mask to `NSWindowStyleMaskBorderless`. This removes
   the title bar entirely — no ~28px offset.

4. Force-hide the toolbar by making `ShouldHideToolbar()` return `true` for the
   Chromium Profile Server, or pass `--chromium-profile-server-hide-toolbar`.
   This removes the 24px toolbar offset and the nav buttons/URL bar views.

   If the toolbar is already hidden via the `--hidden` flag or command-line
   switch, confirm this and note it. If not, add the switch to the server launch
   arguments.

**Fix 3: Pixel-to-point conversion (GUI side)**

In `Metal.zig` — `updateCALayerHostFrame()`:

5. Read the parent IOSurfaceLayer's `contentsScale` and divide all pixel values
   by it. The grid coordinates multiplied by cell size produce physical pixels,
   but CALayer frames use logical points.

   ```
   scale = parent.contentsScale  // 2.0 on Retina
   x = (grid_col * cell_width) / scale
   y = (grid_row * cell_height) / scale
   w = (grid_width * cell_width) / scale
   h = (grid_height * cell_height) / scale
   ```

6. Remove the Y flip (`flipped_y = sh - y - h`). With `geometryFlipped = YES` on
   the CALayerHost, Y=0 is at the top — matching the terminal grid's coordinate
   system. The naive `y = grid_row * cell_height / scale` is correct as-is.

**Fix 4: Remove diagnostic logging (GUI side)**

7. Remove all `diag:` log lines from `Metal.zig` and `generic.zig`. Keep the
   existing non-diagnostic log lines (`created CALayerHost`, etc.).

#### Verification

1. Build Chromium Profile Server
   (`autoninja -C out/Default chromium_profile_server`).
2. Build TermSurf GUI (`cd gui && zig build`).
3. Launch the app, open a terminal, type `web news.ycombinator.com`.
4. **Pass criteria:**
   - Web page renders at the correct position (aligned with the TUI viewport, ~1
     row down and ~1 column in from the top-left of the surface)
   - No visible Y offset — content top edge aligns with the TUI viewport top
   - No visible X offset — content left edge aligns with the TUI viewport left
   - Scrolling feels responsive
   - Text selection tracks the cursor
   - Pane resize works — browser content resizes with the pane
   - Multiple panes with different profiles work
   - Closing a browser pane cleans up the CALayerHost

**Result:** Fail (partial success)

The three fixes dramatically improved positioning. The ~400px Y offset and the
large X offset are gone. The web content now renders near the correct position —
close enough that the browser is usable. However, a small residual offset
remains: approximately **10px too high** (Y) and **3px too far left** (X).

**What worked:**

- `geometryFlipped = YES` on the CALayerHost fixed the dominant Y offset. The
  content is now right-side-up and near the top of the surface where it belongs.
- `NSWindowStyleMaskBorderless` eliminated the ~28px title bar offset.
- `ShouldHideToolbar() = true` eliminated the 24px toolbar offset.
- Dividing by `contentsScale` fixed the 2x pixel-vs-point scaling error.
- `anchorPoint = CGPointZero` matches Chromium's own pattern.

**What remains:**

A small residual offset: ~10px too high, ~3px too far left. This is likely from
padding, border, or inset values in either the terminal grid coordinate system
or the Chromium compositor's view positioning. The grid coordinates from the TUI
(`col=1, row=4`) may not account for some padding between the surface edge and
the first grid cell, or the Chromium content view may have a small inset within
the borderless window.

#### Conclusion

The major positioning bugs are fixed. The CALayerHost content renders near the
correct position — a massive improvement from the ~400px offset. A small ~10px Y
and ~3px X residual offset remains, likely from grid padding or view insets.

## Conclusion

Issue 625 replaced `FrameSinkVideoCapturer` with `CALayerHost`. The core
pipeline works: Chromium sends a `ca_context_id` once over XPC, the GUI creates
a `CALayerHost` sublayer, and Window Server composites directly from GPU VRAM.
No per-frame IPC, no pixel copies, no Metal shader compositing.

**What was accomplished:**

- Deleted the entire `FrameSinkVideoCapturer` pipeline (~460 lines of capturer
  code, IOSurface Mach port transfer, Metal overlay shaders)
- Chromium side: added `CALayerParams` callback on `RenderWidgetHostViewMac`,
  sends `ca_context_id` once per tab over XPC
- GUI side: creates `CALayerHost` as sublayer of IOSurfaceLayer with
  `geometryFlipped = YES` and `anchorPoint = CGPointZero`
- GUI side: converts grid coordinates to logical points (divides by
  `contentsScale`) for the CALayerHost frame
- Chromium side: borderless window with hidden toolbar to eliminate phantom
  offsets in the CAContext layer tree
- Moved `WebContentsObserver` notifications to a lightweight `ShellTabObserver`

**What was NOT tested:**

The positioning bug consumed all six experiments (2–6 were positioning fixes,
plus Experiment 1 was research). The CALayerHost content now renders near the
correct position (~10px Y, ~3px X residual offset), but this offset prevented
thorough testing of:

- Scrolling responsiveness and latency improvement vs the capturer
- Text selection tracking
- Pane resize behavior
- Multiple panes with different profiles
- CALayerHost cleanup on pane close
- Input latency comparison with native Chrome

**Follow-up issues needed:**

1. Fix the remaining ~10px Y / ~3px X positioning offset (likely grid padding or
   Chromium view insets)
2. Verify the full CALayerHost pipeline once positioning is pixel-perfect
3. Measure input latency improvement vs the old capturer pipeline
