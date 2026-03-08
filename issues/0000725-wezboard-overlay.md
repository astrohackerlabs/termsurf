# Issue 725: Wezboard browser overlay rendering

## Goal

Make browser content visible in the Wezboard terminal window. The TermSurf
protocol plumbing works end-to-end (Issue 724), but the CALayerHost overlay
renders nothing on screen.

## Background

Issue 724 implemented the first three layers of the TermSurf protocol in
Wezboard across three experiments:

1. **State management** (Exp 1) — `Pane`, `Server`, and `TermSurfState` structs
   with pane registry, server registry, tab-to-pane mappings. Browser process
   spawning with `--ipc-socket`. Tab lifecycle (`CreateTab`, `TabReady`,
   `CloseTab`).
2. **Message forwarding** (Exp 2) — Board routes messages between TUI and
   Chromium. Navigate, UrlChanged, LoadingState, TitleChanged, SetColorScheme,
   ModeChanged, and Resize forwarding. Disconnect cleanup with server pane
   counting.
3. **CALayerHost rendering** (Exp 3) — CaContext message handling, three-layer
   CALayerHost hierarchy (flipped -> positioning -> host), cleanup on
   disconnect.

Everything works except visibility. Logs confirm:

- `CaContext: tab_id=1 context_id=4223629142` (valid nonzero context ID)
- `created CALayerHost contextId=4223629142`
- TUI disconnect cleans up layers without crash

But no browser content appears on screen. Not mispositioned, not half-sized —
completely invisible.

## The problem

The CALayerHost is added as a sublayer of the terminal view's backing layer
(`[ns_view layer]`). The backing layer is a `CAMetalLayer` created by
`make_backing_layer()` in `window.rs`. ANGLE (OpenGL ES via Metal) also creates
its own `CAMetalLayer` as a sublayer of this same backing layer for terminal
rendering.

The layer tree looks like:

```
NSView [layer-backed, wantsLayer=YES]
  └─ CAMetalLayer [backing layer from make_backing_layer()]
       ├─ CAMetalLayer [ANGLE's sublayer, renders terminal]
       └─ flipped_layer [our code]
            └─ positioning_layer
                 └─ CALayerHost [contextId set correctly]
```

## Hypotheses considered

### 1. Z-order: CALayerHost behind ANGLE's content

**Status: excluded.**

Core Animation draws sublayers on top of the parent layer's content. Sibling
sublayers are composited in insertion order — later additions go on top. ANGLE's
sublayer is added during EGL init (window creation). Our flipped_layer is added
later (when CaContext arrives). Our CALayerHost is on top.

### 2. ANGLE's opaque rendering covers the CALayerHost

**Status: excluded.**

Even if ANGLE renders a fully opaque terminal background (it does — `glClear`
then a full-window filled rectangle), our CALayerHost is above it in z-order, so
it would paint on top of the opaque content.

### 3. contentsScale mismatch (1.0 vs 2.0)

**Status: excluded as sole cause.**

WezTerm hardcodes `contentsScale = 1.0` on the backing layer. Ghostboard sets
`contentsScale = scaleFactor` (2.0 on Retina). A scale mismatch could cause
incorrect sizing or positioning, but not complete invisibility. A factor of 2
error would make the overlay half-sized or doubled, not zero-sized.

### 4. Zero-sized frames

**Status: excluded.**

The flipped_layer frame is set to `backing_layer.bounds` (the full view size).
The positioning_layer frame is set to `pixel_width / contentsScale` by
`pixel_height / contentsScale` — with placeholder values of ~800x700 points.
Both are non-zero.

### 5. CALayerHost not receiving content from Chromium

**Status: excluded.**

The same Roamium binary with the same CAContext mechanism works in Ghostboard.
The context ID is valid and nonzero. CALayerHost doesn't need special
entitlements — Window Server handles cross-process compositing natively.

### 6. Wrong view or wrong layer

**Status: excluded.**

`first_ns_view()` gets the NSView via `HasWindowHandle` ->
`RawWindowHandle::AppKit` -> `ns_view`. This is the same view that ANGLE renders
into. `[ns_view layer]` returns its backing CAMetalLayer.

### 7. Layer-backed view doesn't composite manual sublayers

**Status: current best hypothesis.**

WezTerm creates a **layer-backed** view: `setWantsLayer: true` is called without
first assigning a layer (window.rs line 627). In a layer-backed view, AppKit
owns the layer tree. Apple's documentation states: "In a layer-backed view, you
should never interact directly with the layer." Manually added sublayers may not
be composited.

Ghostboard creates a **layer-hosting** view: it assigns a custom
`IOSurfaceLayer` to `view.layer` _before_ setting `view.wantsLayer = true`
(Metal.zig lines 124-125). In a layer-hosting view, the app owns the layer tree
and manually added sublayers composite correctly.

This is the only hypothesis that explains complete invisibility with correct
z-order, correct context ID, non-zero frames, and working protocol plumbing.

## Proposed solution

Create a transparent **overlay NSView** as a subview on top of the terminal
view. Make the overlay view layer-hosting (assign its layer before setting
`wantsLayer`). Put the CALayerHost in the overlay view's layer tree:

```
NSWindow
  └─ contentView
       ├─ terminalView (layer-backed, ANGLE renders here — unchanged)
       └─ overlayView (new, layer-hosting, transparent)
            └─ CALayer [root, assigned before wantsLayer]
                 └─ flipped_layer (geometryFlipped=YES, auto-fills parent)
                      └─ positioning_layer (explicit frame)
                           └─ CALayerHost (contextId from Chromium)
```

This sidesteps the layer-backed restriction without modifying WezTerm's ANGLE
rendering pipeline. The overlay NSView is a sibling subview composited by AppKit
on top of the terminal view.

The overlay NSView would be:

- Created once on the main thread (when first CaContext arrives, or at window
  init)
- Same frame as the terminal view, with autoresizing mask to follow resizes
- Layer-hosting: `view.layer = CALayer.new(); view.wantsLayer = true` (in that
  order)
- Transparent: no background color, `layer.opaque = false`
- Non-interactive: `hitTest:` returns nil so all input passes through to the
  terminal view beneath
