+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 626: CALayerHost X/Y Positioning

## Goal

Fix the remaining ~10px Y and ~3px X offset in the CALayerHost browser overlay
so that web content aligns pixel-perfectly with the TUI viewport.

## Background

[Issue 625](625-calayerhost.md) replaced the `FrameSinkVideoCapturer` pipeline
with `CALayerHost`. Instead of capturing IOSurface frames at 120fps and
transferring Mach ports over XPC every frame, Chromium now sends a
`ca_context_id` (uint32) once per tab. The GUI creates a `CALayerHost` sublayer
on the IOSurfaceLayer, and Window Server composites the remote content directly
from GPU VRAM. Zero per-frame IPC, zero pixel copies, zero Metal shader
compositing.

### The positioning saga

The initial CALayerHost implementation (Issue 625 Experiment 2) had the web
content positioned catastrophically wrong — pushed ~400px to the bottom of the
screen with only the top ~10% visible. Six experiments were needed to diagnose
and fix the positioning:

- **Experiment 2:** Core CALayerHost pipeline works, but content is ~400px off.
- **Experiment 3:** Tried flipping Y (`flipped_y = screen_height - y - h`). Zero
  visible effect — the Y flip had no impact at all.
- **Experiment 4:** Added diagnostic logging. Confirmed
  `setProperty("frame",
  frame)` works (readback matches), the function is
  called with valid data, and the parent IOSurfaceLayer is 800×600 points with
  `contentsScale=2.0`. The hardcoded frame test proved the frame property
  controls positioning. Also discovered cell dimensions are in physical pixels
  while CALayer frames use logical points.
- **Experiment 5:** Researched the Chromium source. Found three root causes:
  1. **Missing `geometryFlipped`.** Chromium's CAContext root layer uses
     `geometryFlipped = YES` (Y=0 at top). Chromium's browser process hosts it
     in a `geometryFlipped` layer too. Our IOSurfaceLayer had no
     `geometryFlipped`, causing a full Y-axis inversion.
  2. **Shell window chrome.** The Chromium Profile Server's NSWindow had a title
     bar (~28px) and toolbar (24px) that created phantom offsets in the
     CAContext layer tree, even though the window was hidden.
  3. **Physical pixels vs logical points.** Cell dimensions and screen height
     were in physical pixels (2x on Retina), but CALayer frames use points.
- **Experiment 6:** Applied all three fixes. `geometryFlipped = YES` and
  `anchorPoint = CGPointZero` on the CALayerHost, borderless window with hidden
  toolbar on the Chromium side, pixel-to-point conversion by dividing by
  `contentsScale`. The ~400px offset is gone, but a small residual offset
  remains.

### Current state

The web content renders near the correct position but is offset by approximately
**10px too high** (Y) and **3px too far left** (X). This is close enough to see
that the CALayerHost pipeline works, but the misalignment prevents thorough
testing of:

- Scrolling responsiveness and latency
- Text selection tracking
- Pane resize behavior
- Multiple panes with different profiles
- CALayerHost cleanup on pane close
- Input latency comparison with native Chrome

The offset must be fixed before any of those can be verified.

### How positioning currently works

The TUI (`web` command) sends grid coordinates over XPC:

```
set_overlay: col=1 row=4 w=120 h=35
```

The GUI converts these to a CALayerHost frame in logical points:

```zig
scale = parent.contentsScale   // 2.0 on Retina
x = grid_col * cell_width / scale
y = grid_row * cell_height / scale
w = grid_width * cell_width / scale
h = grid_height * cell_height / scale
host.setProperty("frame", { origin: {x, y}, size: {w, h} })
```

The CALayerHost has `geometryFlipped = YES`, so Y=0 is at the top — matching the
terminal grid's coordinate system. No Y flip is applied.

### Possible causes of the residual offset

- **Grid padding.** The terminal grid may not start at pixel (0, 0) in the
  surface. There could be padding between the surface edge and the first cell.
  If `grid_col=1` maps to a pixel position that assumes no padding, the
  CALayerHost would be offset by the padding amount.
- **Chromium view insets.** Even with a borderless window and hidden toolbar,
  the Chromium content view may have a small inset within the window frame. The
  `RenderWidgetHostViewMac` frame might not start at exactly (0, 0) in the
  contentView.
- **Cell size rounding.** The cell dimensions are integers (physical pixels).
  Dividing by `contentsScale` may introduce fractional point values that don't
  align with the actual grid rendering.

## Experiments

### Experiment 1: Research Electron's CALayerHost handling

Electron embeds Chromium in a normal `BrowserWindow` using stock CALayerHost —
no custom display code. Their CALayerHost positioning works perfectly. This
experiment studies how Electron sets up its NSView/CALayer hierarchy and
positions the CALayerHost to understand what we're doing differently.

#### Research questions

**R1: Electron's NSView hierarchy.**

How does Electron's `BrowserWindow` set up its NSView tree? What view hosts the
web content? Is it a layer-hosting view or a layer-backed view? What is the view
hierarchy from NSWindow down to the CALayerHost?

Look in `vendor/electron/shell/browser/native_window_mac.mm` and related files.

**R2: Electron's CALayerHost setup.**

Does Electron create the CALayerHost itself, or does it inherit Chromium's
default `DisplayCALayerTree` behavior? Does it set `geometryFlipped`,
`anchorPoint`, or any other properties on the host layer? How does the host
layer relate to the content view?

Look in `vendor/electron/` for any CALayerHost references, and in Chromium's
`ui/accelerated_widget_mac/display_ca_layer_tree.mm` for the default behavior.

**R3: How does Chromium's default `DisplayCALayerTree` position the
CALayerHost?**

In stock Chromium (which Electron uses), `DisplayCALayerTree::GotCALayerFrame()`
creates the CALayerHost and adds it to a `maybe_flipped_layer_`. What is
`maybe_flipped_layer_`? How is it configured? What is its relationship to the
NSView's layer? This is the reference implementation that works — we need to
match it.

Look in `chromium/src/ui/accelerated_widget_mac/display_ca_layer_tree.mm` and
`display_ca_layer_tree.h`.

**R4: How does the content view's frame relate to the CAContext geometry?**

In Chromium's normal display path, the content view fills the window (minus any
chrome). The CALayerHost fills the content view. The CAContext from the GPU
process is sized to match. How does this size agreement happen? Is the
CALayerHost frame explicitly set, or does it auto-fill via autoresizing masks?

**R5: What would "no offset" look like for us?**

Given what we learn from R1–R4, what would we need to change so that the
CALayerHost content starts at exactly (0, 0) in the CAContext — with no title
bar, toolbar, or view inset shifting the content? Is it a Chromium-side fix
(make the content view fill the window at origin), a GUI-side fix (account for
the offset), or both?

#### Verification

Research is complete when we can draw a side-by-side comparison:

1. **Electron/Chrome:** Full layer tree from NSWindow → NSView → CALayer →
   CALayerHost, with every `geometryFlipped`, `anchorPoint`, `frame`, and
   autoresizing mask documented.
2. **TermSurf:** Our current layer tree from SurfaceView → IOSurfaceLayer →
   CALayerHost, with the same properties documented.
3. A concrete list of differences that could explain the ~10px Y / ~3px X
   offset.

#### Results

##### R1: Electron's NSView hierarchy

Electron does NOT create or manage CALayerHost directly. It relies entirely on
Chromium's `DisplayCALayerTree` for layer hosting. The view hierarchy:

```
NSWindow (ElectronNSWindow)
└─ contentView (layer-backed via setWantsLayer:YES)
   └─ RootViewMac (views::View)
      └─ Content View (web content, Chromium's render view)
```

In `AddContentViewLayers()` (`native_window_mac.mm:1776–1792`), Electron sets:

- `setWantsLayer:YES` on the contentView
- For framed windows: creates an explicit `CALayer` with
  `autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable`

Electron sets no `geometryFlipped`, no `anchorPoint`, no `contentsScale` — all
of that is handled by Chromium's `DisplayCALayerTree`.

##### R2: Electron's CALayerHost setup

No CALayerHost references exist in the Electron source. Electron inherits
Chromium's default `DisplayCALayerTree` behavior entirely.

##### R3: Chromium's `DisplayCALayerTree`

This is the key finding. From `display_ca_layer_tree.mm` (lines 33–52):

```
RenderWidgetHostViewCocoa (NSView, layer-hosting)
└─ background_layer_ (root CALayer, NSView.layer)
   └─ maybe_flipped_layer_ (CALayer)
      └─ remote_layer_ (CALayerHost)
```

**`maybe_flipped_layer_` properties:**

| Property           | Value                                           |
| ------------------ | ----------------------------------------------- |
| `geometryFlipped`  | `YES` (macOS only)                              |
| `autoresizingMask` | `kCALayerWidthSizable \| kCALayerHeightSizable` |
| `anchorPoint`      | `CGPointZero`                                   |

**`remote_layer_` (CALayerHost) properties:**

| Property           | Value                                      |
| ------------------ | ------------------------------------------ |
| `anchorPoint`      | `CGPointZero`                              |
| `contextId`        | GPU process ca_context_id                  |
| `autoresizingMask` | `kCALayerMaxXMargin \| kCALayerMaxYMargin` |

The critical pattern: Chromium uses a **two-layer setup**. The
`maybe_flipped_layer_` handles the coordinate flip and auto-resizes to fill the
parent. The CALayerHost sits inside it at origin with margins that keep it
pinned to the top-left.

The NSView connection happens in `RenderWidgetHostNSViewBridge` (line 41–60):

```cpp
background_layer_ = [[CALayer alloc] init];
display_ca_layer_tree_ = new DisplayCALayerTree(background_layer_);
cocoa_view_.layer = background_layer_;
cocoa_view_.wantsLayer = YES;
```

##### R4: Size agreement

The CALayerHost frame is NEVER explicitly set. Instead:

- `maybe_flipped_layer_` auto-resizes to fill `background_layer_` (via
  `kCALayerWidthSizable | kCALayerHeightSizable`)
- The CALayerHost has
  `autoresizingMask = kCALayerMaxXMargin |
  kCALayerMaxYMargin` — it stays at
  origin, does NOT resize with parent
- The remote CAContext content renders at its own intrinsic size
- Size agreement happens because both the NSView and the Chromium compositor are
  told the same window size

##### R5: Differences that explain the offset

**Chromium/Electron layer tree:**

```
NSView.layer (background_layer_, no special properties)
└─ maybe_flipped_layer_ (geometryFlipped=YES, anchorPoint=zero, auto-resizes)
   └─ CALayerHost (anchorPoint=zero, pinned to top-left)
```

**TermSurf layer tree:**

```
IOSurfaceLayer (no geometryFlipped, has contentsScale=2.0)
└─ CALayerHost (geometryFlipped=YES, anchorPoint=zero, frame set explicitly)
```

**Key differences:**

1. **Missing intermediate layer.** Chromium uses a dedicated
   `maybe_flipped_layer_` between the root and the CALayerHost. We put the
   CALayerHost directly on the IOSurfaceLayer. The `geometryFlipped` should be
   on the intermediate layer, not on the CALayerHost itself.

2. **We set `geometryFlipped` on the wrong layer.** Chromium sets
   `geometryFlipped = YES` on `maybe_flipped_layer_` (the parent of the
   CALayerHost). We set it on the CALayerHost itself. `geometryFlipped` affects
   **sublayer** geometry, not the layer's own position. Setting it on the
   CALayerHost flips its sublayers (the remote content's internal layers), but
   does NOT flip where the CALayerHost itself sits in the parent.

3. **We explicitly set frame; Chromium never does.** Chromium relies on
   autoresizing masks to position the CALayerHost. The CALayerHost has no
   explicit frame — it sits at (0, 0) in the `maybe_flipped_layer_`, which
   itself fills the root layer. We set an explicit `frame` with computed grid
   coordinates.

4. **`autoresizingMask` mismatch.** Chromium's CALayerHost uses
   `kCALayerMaxXMargin | kCALayerMaxYMargin` (pin to top-left). We don't set any
   autoresizing mask.

5. **The ~10px/~3px offset** is likely from `geometryFlipped` being on the wrong
   layer. When `geometryFlipped = YES` is on the CALayerHost, it doesn't affect
   the CALayerHost's own position in the parent IOSurfaceLayer. The
   IOSurfaceLayer uses default CALayer coordinates (Y=0 at bottom). Our explicit
   frame positions the CALayerHost in those un-flipped coordinates, but the
   remote content inside is rendered in flipped coordinates. This mismatch
   creates a small offset that depends on the difference between the CALayerHost
   frame and the remote content size.

#### Conclusion

The root cause is architectural: we're missing the intermediate flipped layer
that Chromium uses. Chromium's pattern is
`root → maybe_flipped_layer_
(geometryFlipped) → CALayerHost`, not
`root → CALayerHost (geometryFlipped)`. The fix should either: (a) add an
intermediate layer matching Chromium's pattern, or (b) move `geometryFlipped` to
the IOSurfaceLayer (risky — could break terminal rendering) and position the
CALayerHost without explicit frame math. The `geometryFlipped` on the wrong
layer explains the residual offset.

### Experiment 2: Diagnostic logging to find the residual offset

The Experiment 1 research identified architectural differences (missing
intermediate layer, `geometryFlipped` on the wrong layer). But before changing
the layer architecture, we need to understand the exact source of the ~10px Y /
~3px X offset. There's a simpler hypothesis: grid padding.

The terminal grid doesn't start at pixel (0, 0) in the surface — it starts at
`(padding.left, padding.top)`. The current `updateCALayerHostFrame` computes
position as `grid_col * cell_width / scale`, relative to the surface origin, not
the grid origin. If `padding.top / scale ≈ 10` and `padding.left / scale ≈ 3`,
this entirely explains the residual offset.

#### What to log

**GUI side (in `updateCALayerHostFrame`):**

1. Grid padding: `self.size.padding.top`, `self.size.padding.left` (physical
   pixels)
2. Grid padding in points: padding / contentsScale
3. Computed frame values (x, y, w, h) in logical points
4. Overlay grid coordinates (col, row, width, height)
5. Cell dimensions (cell_width, cell_height)
6. contentsScale

**Chromium side (in `SetContents`):**

7. `web_view.frame` — content view frame within the window
8. `window.contentView.bounds` — window content area bounds

#### Changes

- `gui/src/renderer/generic.zig`: Pass `self.size.padding.top` and
  `self.size.padding.left` to `Metal.updateCALayerHostFrame`.
- `gui/src/renderer/Metal.zig`: Accept padding parameters. Log all values
  including padding, padding-in-points, computed frame, grid coordinates, cell
  dimensions, and contentsScale.
- `chromium/src/content/chromium_profile_server/browser/shell_platform_delegate_mac.mm`:
  Add `NSLog` in `SetContents` to log `web_view.frame` and
  `window.contentView.bounds`.

#### Verification

Run the app, trigger a browser overlay, and read the logs. If
`padding.top / scale ≈ 10` and `padding.left / scale ≈ 3`, the padding is the
cause and the fix is to add padding to the frame calculation. If not, the
Chromium-side logs will show whether the content view has a non-zero origin.

#### Results

**GUI side:**

```
padding_top=4 padding_left=4 padding_top_pts=2.0 padding_left_pts=2.0
scale=2.0 grid=(1,4,120,32) cell=(13,29) frame=(6.5,58.0,780.0,464.0)
```

- Padding is only 2.0 pts in each direction — far too small to explain a ~10px Y
  or ~3px X offset.

**Chromium side:**

```
web_view.frame: (0, 0, 800, 600)
contentView.bounds: (0, 0, 800, 600)
```

- The web content view starts at exactly (0, 0) and fills the entire borderless
  window. No inset, no offset.

#### Conclusion

Both hypotheses are ruled out:

- **Grid padding** is only 2pt — not the cause.
- **Chromium view inset** is zero — not the cause.

The remaining explanation is the coordinate system mismatch identified in
Experiment 1. The IOSurfaceLayer uses standard CALayer coordinates (Y=0 at
bottom). We set `frame.origin.y = 58.0` as if Y=0 is at the top, but the
CALayerHost is positioned 58pt from the _bottom_ of the IOSurfaceLayer, not from
the top. The `geometryFlipped = YES` on the CALayerHost only flips its sublayer
coordinate system — it does not change how the CALayerHost itself is positioned
in its parent. The fix requires restructuring the layer tree to match Chromium's
`DisplayCALayerTree` pattern: add an intermediate layer with
`geometryFlipped = YES` so that Y=0-at-top applies to the CALayerHost's
position.

### Experiment 3: Add intermediate flipped layer

Match Chromium's `DisplayCALayerTree` layer tree architecture. Instead of
putting `geometryFlipped` on the CALayerHost, create an intermediate CALayer
between the IOSurfaceLayer and the CALayerHost that handles the coordinate flip.

**Current layer tree:**

```
IOSurfaceLayer (Y=0 at bottom, contentsScale=2.0)
└─ CALayerHost (geometryFlipped=YES, anchorPoint=zero, frame set explicitly)
```

**Target layer tree:**

```
IOSurfaceLayer (Y=0 at bottom, contentsScale=2.0)
└─ flipped_layer (geometryFlipped=YES, anchorPoint=zero,
│                  autoresizingMask=widthSizable|heightSizable)
   └─ CALayerHost (anchorPoint=zero,
                    autoresizingMask=maxXMargin|maxYMargin)
```

This matches Chromium's `root_layer_ → maybe_flipped_layer_ → remote_layer_`
pattern exactly. The `flipped_layer` auto-resizes to fill the IOSurfaceLayer and
provides a top-left-origin coordinate system. The CALayerHost sits at (0, 0)
inside it, pinned to the top-left.

#### Changes

**`gui/src/renderer/Metal.zig`:**

- In `setCALayerHostContextId`: Create the intermediate `flipped_layer` as a
  sublayer of `self.layer.layer`. Set `geometryFlipped = YES`,
  `anchorPoint = CGPointZero`,
  `autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable` on it.
  Create the CALayerHost as a sublayer of `flipped_layer` (not IOSurfaceLayer
  directly). Set `anchorPoint = CGPointZero`,
  `autoresizingMask = kCALayerMaxXMargin | kCALayerMaxYMargin` on the
  CALayerHost. Do NOT set `geometryFlipped` on the CALayerHost.
- In `updateCALayerHostFrame`: Set the frame on the `flipped_layer`, not the
  CALayerHost. The flipped layer is sized and positioned at the overlay grid
  rectangle (in the flipped coordinate system, so Y=0 is at top). The
  CALayerHost stays at (0, 0) inside it with no explicit frame.
- In `removeCALayerHost`: Remove the `flipped_layer` (which removes the
  CALayerHost with it).
- Store the `flipped_layer` pointer alongside (or instead of) the CALayerHost
  pointer, or store both.
- Remove diagnostic logging from Experiment 2.

**`gui/src/renderer/generic.zig`:**

- Add a `ca_layer_flipped: ?*anyopaque = null` field to store the intermediate
  layer pointer.
- Pass it to `Metal.setCALayerHostContextId` and `Metal.removeCALayerHost`.
- Keep passing padding to `updateCALayerHostFrame` — the padding should be added
  to the frame position so the overlay aligns with the grid, not the surface
  edge.

**`chromium/src/content/chromium_profile_server/browser/shell_platform_delegate_mac.mm`:**

- Remove diagnostic logging from Experiment 2.

#### Verification

Run the app. The web content should align pixel-perfectly with the TUI viewport
border — no visible gap at the top or left edge. Compare the top-left corner of
the web content with the TUI viewport border drawn by ratatui. If the offset is
gone, proceed to test scrolling, text selection, and pane resize.

#### Results

**Partial success.** The X offset is fixed — web content aligns horizontally
with the TUI viewport. The Y offset remains at approximately ~10px too high.
Resize is broken (the flipped layer does not update on resize), but that is
expected and deferred.

#### Conclusion

The intermediate flipped layer architecture fixed the X offset. Adding grid
padding to the frame calculation (`+ padding_left / scale`) and restructuring
the layer tree (`IOSurfaceLayer → flipped_layer → CALayerHost`) were both
necessary for the X fix.

The Y offset persists because of a conflict between the `flipped_layer`'s
`autoresizingMask` and its explicit frame. The `flipped_layer` has
`autoresizingMask = kCALayerWidthSizable | kCALayerHeightSizable` — the same
mask Chromium uses on `maybe_flipped_layer_` to fill the parent. But Chromium's
`maybe_flipped_layer_` has no explicit frame — it fills the entire parent. We
set an explicit frame on the `flipped_layer` to position it at the overlay
rectangle, which conflicts with the auto-fill mask. The mask may be adjusting
the layer's position during layout, causing the ~10px Y shift.

The fix for Experiment 4 should follow Chromium's pattern more closely: let the
`flipped_layer` fill the entire IOSurfaceLayer (no explicit frame, keep the
auto-fill mask), and set the frame on the CALayerHost instead. The CALayerHost's
position would then be in the flipped layer's coordinate system (Y=0 at top),
which is the correct coordinate system for grid-based positioning.

### Experiment 4: Position CALayerHost inside full-size flipped layer

Follow Chromium's `DisplayCALayerTree` pattern exactly: the `flipped_layer`
fills the entire IOSurfaceLayer (no explicit frame, auto-resizes via mask). The
CALayerHost is positioned inside the `flipped_layer` at the overlay grid
rectangle. This ensures the CALayerHost's frame coordinates are always in the
flipped coordinate system (Y=0 at top), regardless of the IOSurfaceLayer's
coordinate system.

**Current architecture (Experiment 3):**

```
IOSurfaceLayer
└─ flipped_layer (geometryFlipped=YES, auto-fill mask, explicit frame at overlay rect)
   └─ CALayerHost (at origin, pinned top-left via mask)
```

Problem: the auto-fill mask conflicts with the explicit frame on
`flipped_layer`, causing the ~10px Y shift.

**Target architecture (Experiment 4):**

```
IOSurfaceLayer
└─ flipped_layer (geometryFlipped=YES, auto-fill mask, fills parent)
   └─ CALayerHost (explicit frame at overlay rect, no auto-resize mask)
```

This matches Chromium: `maybe_flipped_layer_` fills the parent, `remote_layer_`
sits inside it.

#### Changes

**`gui/src/renderer/Metal.zig`:**

- In `setCALayerHostContextId`: Remove `autoresizingMask` from the CALayerHost
  (we will set an explicit frame on it instead of relying on pinning).
- In `updateCALayerHostFrame`: Set the frame on the CALayerHost (not the flipped
  layer). The flipped layer fills the parent and is never repositioned.

**`gui/src/renderer/generic.zig`:**

- In `updateCALayerHostFrame`: Pass the `ca_layer_host` pointer instead of
  `ca_layer_flipped`.

#### Verification

Run the app. The web content should align pixel-perfectly with the TUI viewport
in both X and Y. The ~10px Y offset should be gone.

#### Results

**Fail.** The web browser does not appear at all. The CALayerHost is completely
invisible — no web content renders anywhere on screen. This is a catastrophic
regression from Experiment 3, which at least showed the content with correct X
and only ~10px Y offset.

#### Conclusion

Setting an explicit frame on the CALayerHost does not work. In Chromium's
`DisplayCALayerTree`, the CALayerHost never has an explicit frame — it sits at
(0, 0) and renders the remote CAContext at its intrinsic size. CALayerHost is
not a normal CALayer: it mirrors a remote layer tree from another process via
Window Server. Setting a frame on it likely clips or displaces the remote
content in a way that makes it invisible.

This means Experiment 3's architecture was closer to correct: the
`flipped_layer` must be the one with the explicit frame, and the CALayerHost
must sit at (0, 0) inside it with no explicit frame. The ~10px Y offset in
Experiment 3 was NOT caused by the `autoresizingMask` conflict — that hypothesis
was wrong. The actual cause of the Y offset must be something else.

Experiment 5 should revert to Experiment 3's architecture (frame on the
`flipped_layer`, CALayerHost at origin) and investigate the Y offset through
other means.

### Experiment 5: Check IOSurfaceLayer's geometryFlipped

The ~10px Y offset may be caused by a coordinate system mismatch. We set the
`flipped_layer`'s frame origin in the IOSurfaceLayer's sublayer coordinate
system. If the IOSurfaceLayer does NOT have `geometryFlipped` (Y=0 at bottom),
then our `y=60pt` places the flipped layer 60pt from the bottom — not from the
top as intended. For a 600pt parent with a 464pt layer, that puts the top edge
at `600 - 60 - 464 = 76pt` from the top instead of 60pt. The 16pt difference is
approximately one cell (14.5pt).

#### Changes

- `gui/src/renderer/Metal.zig`: In `setCALayerHostContextId`, after creating the
  flipped layer, log `self.layer.layer.getProperty(bool, "geometryFlipped")` and
  the IOSurfaceLayer's bounds.

#### Verification

Run the app and read the log. If `geometryFlipped = false`, the Y offset is
explained and the fix is to Y-flip the flipped layer's position:
`y = parent_height - y_from_top - h`. If `geometryFlipped = true`, the offset
has a different cause.

#### Results

**Pass.** The log confirms:

```
IOSurfaceLayer geometryFlipped=false bounds=(0.0,0.0,800.0,568.0)
```

The IOSurfaceLayer does NOT have `geometryFlipped`. Y=0 is at the bottom. Our
`y = 60pt` positions the flipped layer 60pt from the bottom of the 568pt parent,
placing its top edge at `568 - 60 - 464 = 44pt` from the top — instead of the
intended 60pt. The 16pt difference (≈ one cell height of 14.5pt) matches the
observed ~10px offset.

#### Conclusion

Root cause confirmed. The IOSurfaceLayer uses standard CALayer coordinates with
Y=0 at the bottom. Our frame calculation assumes Y=0 at the top. The fix is to
apply a Y-flip when setting the flipped layer's frame:
`y_flipped = parent_height - y_from_top - h`.

### Experiment 6: Apply Y-flip for IOSurfaceLayer coordinates

The IOSurfaceLayer has `geometryFlipped = false` (Y=0 at bottom). Our
`flipped_layer`'s frame is positioned in the IOSurfaceLayer's sublayer
coordinate system, which uses bottom-origin Y. We currently compute
`y = grid_row * cell_height / scale + padding_top / scale` as if Y=0 is at the
top. The fix is to flip: `y_flipped = parent_height - y_from_top - h`.

Also remove the Experiment 5 diagnostic log.

#### Changes

**`gui/src/renderer/Metal.zig`:**

- In `updateCALayerHostFrame`: After computing `y` (top-origin) and `h`, read
  the parent IOSurfaceLayer's bounds height (`self.layer.layer` bounds). Compute
  `y_flipped = parent_height - y - h`. Use `y_flipped` as the frame origin Y.
- In `setCALayerHostContextId`: Remove the Experiment 5 diagnostic log.

#### Verification

Run the app. The web content should align pixel-perfectly with the TUI viewport
in both X and Y. The ~10px Y offset should be gone. If it works, also verify
that the content stays aligned after a browser navigation (page load).

#### Results

**Pass.** The web content aligns pixel-perfectly with the TUI viewport in both X
and Y. The ~10px Y offset is gone.

#### Conclusion

The Y-flip formula `y = parent_height - y_from_top - h` corrects for the
IOSurfaceLayer's bottom-origin coordinate system. Combined with Experiment 3's
intermediate flipped layer (which fixed X), the overlay is now pixel-perfect.

## Conclusion

The CALayerHost X/Y offset had two root causes:

1. **Missing intermediate flipped layer** (fixed in Experiment 3). Chromium's
   `DisplayCALayerTree` uses a `maybe_flipped_layer_` with `geometryFlipped=YES`
   between the root layer and the CALayerHost. We were attaching the CALayerHost
   directly to the IOSurfaceLayer with `geometryFlipped` on the wrong layer.

2. **Bottom-origin Y coordinates** (fixed in Experiment 6). The IOSurfaceLayer
   has `geometryFlipped=false` — Y=0 is at the bottom. Our frame calculation
   assumed Y=0 at the top. The fix is a single Y-flip:
   `y = parent_height - y_from_top - h`.

Six experiments were needed to fix what amounted to two coordinate system bugs.
The actual code changes are trivial — an intermediate CALayer and a one-line
Y-flip. The debugging took longer than building the entire CALayerHost pipeline
in Issue 625.

### Remaining work

Pixel-perfect positioning is done, but the broader CALayerHost integration still
needs testing. These were deferred while the offset blocked all verification:

- **Resize** — The overlay does not update when the pane resizes. Known broken.
- **Scrolling** — Responsiveness and latency untested.
- **Text selection** — Drag tracking untested.
- **Multi-pane** — Multiple overlays in split panes untested.
- **CALayerHost cleanup** — Layer removal on pane close untested.
- **Input latency** — Comparison with native Chrome untested.

These will be tracked in a follow-up issue.
