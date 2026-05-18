+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 627: CALayerHost Resize

## Goal

Make the browser overlay resize correctly when the user resizes the window or
pane. The overlay should track the TUI viewport at all times, matching the
behavior that worked flawlessly before the CALayerHost migration.

## Background

### How we got here

[Issue 625](625-calayerhost.md) replaced the `FrameSinkVideoCapturer` pipeline
with `CALayerHost`. Instead of capturing IOSurface frames at 120fps and
transferring Mach ports over XPC every frame, Chromium now sends a
`ca_context_id` (uint32) once per tab. The GUI creates a `CALayerHost` sublayer
on the IOSurfaceLayer, and Window Server composites the remote content directly
from GPU VRAM. Zero per-frame IPC, zero pixel copies.

[Issue 626](626-x-y-calayerhost.md) fixed the X/Y positioning of the CALayerHost
overlay. Six experiments revealed two coordinate system bugs: a missing
intermediate flipped layer and a bottom-origin Y coordinate system on the
IOSurfaceLayer. The overlay is now pixel-perfect at initial placement.

### What broke

Before CALayerHost, resize worked flawlessly. The full chain was in place:

1. User resizes window/pane
2. TUI detects the new terminal dimensions
3. TUI sends `set_overlay` XPC with updated grid coordinates and dimensions
4. GUI receives `set_overlay`, stores new grid coordinates, computes pixel
   dimensions, and forwards the resize to Chromium
5. Chromium renders at the new size
6. The Metal shader composited the IOSurface at the new grid coordinates every
   frame in `drawFrame()`

Step 6 is what changed. The old IOSurface pipeline re-read the overlay grid
coordinates every frame and rendered the Metal shader at the current position
and size. Resize was automatic — update the coordinates and the next frame
renders correctly.

With CALayerHost, there is no per-frame rendering. The `flipped_layer` has a
static frame set once by `updateCALayerHostFrame()`. When the pane resizes, the
XPC messages still flow (steps 1–5 still work), but the `flipped_layer` frame is
not updated to reflect the new dimensions.

### The resize chain today

```
TUI sends set_overlay (new grid coords)
    │
    ▼
xpc.zig handleSetOverlay() — stores grid coords, computes pixel dims
    │
    ▼
Surface.setOverlay() — updates renderer overlay_grid_* fields
    │
    ▼
renderer.updateCALayerHostFrame() — sets flipped_layer.frame
    │
    ▼
Metal.updateCALayerHostFrame() — converts grid→points, Y-flip, sets frame
```

The chain looks correct on paper — `setOverlay()` calls
`updateCALayerHostFrame()` which should update the `flipped_layer` frame. But
something in this path fails on resize. Either the `flipped_layer` pointer is
null at that point, the parent bounds are stale, or the new coordinates don't
produce the correct frame.

### Current layer tree (from Issue 626)

```
IOSurfaceLayer (geometryFlipped=false, Y=0 at bottom)
└─ flipped_layer (geometryFlipped=YES, anchorPoint=zero, auto-fill mask,
│                  explicit frame at overlay rect)
   └─ CALayerHost (anchorPoint=zero, pinned top-left via mask, at origin)
```

The `flipped_layer` frame is set via:

```zig
y_from_top = grid_row * cell_height / scale + padding_top / scale
y = parent_bounds.height - y_from_top - h
```

On resize, `parent_bounds.height` changes (the IOSurfaceLayer resizes with the
window). The grid coordinates, cell dimensions, and padding may also change. All
of these must be current when `updateCALayerHostFrame()` runs.

### What was accidentally removed

The Issue 625 CALayerHost commit (`3313dd352688e`) removed two pieces of the
resize pipeline that were working:

**GUI side (`gui/src/apprt/xpc.zig`):**

- `sendResize()` — sent `"resize"` XPC action with `pixel_width`/`pixel_height`
  to Chromium when an existing pane's overlay dimensions changed.
- The call to `sendResize()` in the existing-pane path of `handleSetOverlay()`.

**Chromium side (`shell_browser_main_parts.cc`):**

- The `"resize"` XPC action handler that dispatched to `ResizeCapture()`.
- `ResizeCapture()` — looked up the tab by pane_id, called
  `view->SetSize(logical)` to resize the WebContents, and resized the capturer
  via `SetResolution()`.

The capturer resize (`SetResolution()`) is no longer needed — there's no
capturer with CALayerHost. But `view->SetSize()` is still essential. It tells
Chromium to re-render at the new dimensions, and the CAContext automatically
reflects the new size via Window Server compositing.

## Experiments

### Experiment 1: Restore resize pipeline and update flipped_layer on size change

Two things are broken:

1. **Chromium doesn't know about resizes.** The `sendResize()` function and
   Chromium's `"resize"` handler were removed in the CALayerHost commit. The GUI
   stores updated pixel dimensions but never sends them to Chromium.

2. **The flipped_layer frame doesn't update on window resize.** The
   `drawFrame()` loop detects `size_changed` and updates screen uniforms, but
   never calls `updateCALayerHostFrame()`. The `setOverlay()` XPC path does call
   it, but the IOSurfaceLayer's parent bounds may be stale at that point due to
   timing.

#### Changes

**Chromium branch:** Create `146.0.7650.0-issue-627` from
`146.0.7650.0-issue-625`. Update `docs/chromium.md`.

**`chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc`:**

- Restore the `"resize"` XPC action handler in `StartDynamicMode()`. Dispatch to
  `ResizeTab()` (renamed from `ResizeCapture` — no capturer to resize).
- Add `ResizeTab()`: look up the tab by pane_id, call `view->SetSize(logical)`.
  No capturer resize needed.

**`gui/src/apprt/xpc.zig`:**

- Restore `sendResize()`: send `"resize"` action with pane_id, pixel_width,
  pixel_height to the Chromium server.
- Restore the call in `handleSetOverlay()`'s existing-pane path: if the tab has
  been sent and dimensions changed, call `sendResize()`.

**`gui/src/renderer/generic.zig`:**

- In `drawFrame()`, after the `size_changed` block (after
  `self.updateScreenSizeUniforms()`), add `self.updateCALayerHostFrame()`. The
  method guards on `ca_layer_flipped` being non-null, so it's a no-op without an
  overlay.

#### Verification

Run the app, open a browser overlay, then resize the window. The web content
should resize and reposition to match the TUI viewport border. Chromium should
log `Resized pane ... to ...` on each resize. Test horizontal resize, vertical
resize, and window maximize/restore.

#### Results

**Pass.** Resize works. The browser overlay tracks the TUI viewport on window
resize.

#### Conclusion

Two things were broken:

1. The `sendResize()` function and Chromium's `"resize"` handler had been
   accidentally removed in the CALayerHost commit. Restoring them lets Chromium
   re-render at the new dimensions.

2. The `flipped_layer` frame was never updated when the surface size changed.
   Adding `updateCALayerHostFrame()` to the `size_changed` path in `drawFrame()`
   ensures the layer repositions on every resize.

### Experiment 2: Anchor overlay to top edge during resize

#### Problem

Resize works, but the overlay is anchored to the bottom of the window. When
dragging the bottom edge down, the web content slides down with it, then the top
edge catches up. This looks wrong — the overlay should stay pinned to the
top-left corner and grow downward.

The cause is the Y-flip formula from Issue 626 Experiment 6:

```zig
y = parent_bounds.height - y_from_top - h
```

This positions the `flipped_layer` relative to the **bottom** of the
IOSurfaceLayer. During a drag-resize, `parent_bounds.height` changes every
frame, so the layer's distance from the bottom stays constant while its distance
from the top changes — making it slide with the bottom edge.

#### Solution

Stop setting the frame on `flipped_layer`. Let it auto-fill the parent via its
existing `autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable`.
Since `flipped_layer` has `geometryFlipped = YES`, its sublayer coordinate
system has Y=0 at the **top**. Add a third CALayer (`positioning_layer`) between
`flipped_layer` and the CALayerHost. Set the explicit frame on
`positioning_layer` using top-origin Y directly — no flip formula needed. The
CALayerHost sits inside `positioning_layer` at origin.

**Current layer tree:**

```
IOSurfaceLayer (Y=0 at bottom)
└─ flipped_layer (geometryFlipped=YES, auto-fill mask, explicit frame + Y-flip)
   └─ CALayerHost (at origin)
```

**Target layer tree:**

```
IOSurfaceLayer (Y=0 at bottom)
└─ flipped_layer (geometryFlipped=YES, auto-fills parent, NO explicit frame)
   └─ positioning_layer (explicit frame at overlay rect, top-origin Y)
      └─ CALayerHost (at origin)
```

On resize, `flipped_layer` auto-resizes with the IOSurfaceLayer.
`positioning_layer` stays at its position relative to the top. No Y-flip, no
bottom-anchored sliding.

#### Changes

**`gui/src/renderer/Metal.zig`:**

- In `setCALayerHostContextId`: Create `positioning_layer` (plain CALayer) as a
  sublayer of `flipped_layer`. Move the CALayerHost to be a sublayer of
  `positioning_layer` instead of `flipped_layer`. Set `anchorPoint = zero` on
  `positioning_layer`. No autoresizingMask on `positioning_layer` — it gets an
  explicit frame.
- In `updateCALayerHostFrame`: Set the frame on `positioning_layer` instead of
  `flipped_layer`. Use top-origin Y directly
  (`y = grid_row * cell_height /
  scale + padding_top / scale`). Remove the
  Y-flip formula and the `parent_bounds` read.
- In `removeCALayerHost`: Also remove and release `positioning_layer`.
- Store the `positioning_layer` pointer.

**`gui/src/renderer/generic.zig`:**

- Add `ca_layer_positioning: ?*anyopaque = null` field.
- Pass it to `setCALayerHostContextId` and `removeCALayerHost`.
- In `updateCALayerHostFrame`: Pass `ca_layer_positioning` instead of
  `ca_layer_flipped`.

#### Verification

Run the app, open a browser overlay, drag the bottom edge of the window down.
The web content should stay pinned to the top-left and grow downward — no
sliding. Test dragging all four edges and corners.

#### Results

**Pass.** The overlay stays pinned to the top-left on resize. Dragging the
bottom edge grows the content downward — no sliding, no bottom-anchoring.

The initial implementation failed because `flipped_layer` had no explicit frame
and `autoresizingMask` does not set the initial size — the layer started at zero
size, making the overlay invisible. The fix was to set the initial frame on
`flipped_layer` to the parent's bounds at creation time. After that,
`autoresizingMask` handles all subsequent resizes.

#### Conclusion

The 3-layer architecture works:
`IOSurfaceLayer → flipped_layer → positioning_layer → CALayerHost`. The
`flipped_layer` auto-fills the parent and provides a top-left-origin coordinate
system via `geometryFlipped=YES`. The `positioning_layer` holds the explicit
frame at the overlay grid rectangle using simple top-origin Y — no Y-flip
formula needed. The CALayerHost sits at origin inside the positioning layer.

This eliminates the bottom-anchored sliding from Experiment 1. The Y-flip
formula (`y = parent_height - y_from_top - h`) positioned the overlay relative
to the bottom of the IOSurfaceLayer, so during resize the layer stayed fixed
relative to the bottom edge. With the 3-layer architecture, the positioning
layer is placed relative to the top of the flipped layer, which tracks the top
of the window automatically.

## Conclusion

Issue 627 restored resize behavior that was broken during the CALayerHost
migration (Issue 625). Two problems were fixed across two experiments.

**Experiment 1** restored the resize pipeline. The Issue 625 commit accidentally
removed `sendResize()` from the GUI and the `"resize"` XPC handler from
Chromium. Without these, Chromium never learned about size changes — the
WebContents stayed at its initial dimensions. Restoring `sendResize()` (now
dispatching to `ResizeTab()` instead of `ResizeCapture()`, since there is no
capturer) and adding `updateCALayerHostFrame()` to the `size_changed` path in
`drawFrame()` got resize working again.

**Experiment 2** fixed bottom-anchored sliding during resize. The Y-flip formula
from Issue 626 (`y = parent_height - y_from_top - h`) positioned the overlay
relative to the bottom of the IOSurfaceLayer. During a drag-resize,
`parent_height` changes every frame, so the overlay slid with the bottom edge.
The fix introduced a 3-layer architecture:

```
IOSurfaceLayer (Y=0 at bottom)
└─ flipped_layer (geometryFlipped=YES, auto-fills parent)
   └─ positioning_layer (explicit frame, top-origin Y)
      └─ CALayerHost (at origin)
```

The `flipped_layer` auto-resizes with the window and provides a top-origin
coordinate system. The `positioning_layer` uses simple top-origin Y — no flip
formula, no dependency on parent height. The overlay stays pinned to the
top-left and grows downward on resize.

The CALayerHost integration is now functionally complete: pixel-perfect
positioning (Issue 626), resize tracking (Issue 627), and zero per-frame IPC
(Issue 625).
