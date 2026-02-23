# Issue 628: CALayerHost Navigation

## Goal

Fix browser navigation so that clicking a link does not cause the web overlay to
vanish. The overlay should persist across same-site and cross-site navigations,
updating seamlessly as Chromium produces new compositor surfaces.

## Background

### How we got here

[Issue 625](625-calayerhost.md) replaced the `FrameSinkVideoCapturer` pipeline
with `CALayerHost`. Instead of capturing IOSurface frames at 120fps and
transferring Mach ports over XPC every frame, Chromium now sends a
`ca_context_id` (uint32) once per tab. The GUI creates a `CALayerHost` sublayer,
and Window Server composites the remote content directly from GPU VRAM. Zero
per-frame IPC, zero pixel copies.

[Issue 626](626-x-y-calayerhost.md) fixed X/Y positioning of the CALayerHost
overlay. Six experiments revealed two coordinate system bugs: a missing
intermediate flipped layer and a bottom-origin Y coordinate on the
IOSurfaceLayer. The overlay is now pixel-perfect.

[Issue 627](627-resize-calayerhost.md) restored resize behavior. The Issue 625
commit had accidentally removed `sendResize()` and Chromium's `"resize"`
handler. A second experiment introduced a 3-layer architecture
(`flipped_layer → positioning_layer → CALayerHost`) to anchor the overlay to the
top edge during resize instead of the bottom.

Before the CALayerHost migration, navigation worked flawlessly. The old
IOSurface pipeline received a new Mach port on every frame — including after
navigation. The Metal shader re-read the IOSurface texture every frame in
`drawFrame()`, so navigation was invisible to the overlay pipeline. The new
surface just showed up on the next frame.

### What broke

With CALayerHost, there is no per-frame surface update. The `ca_context_id` is
set once on the `CALayerHost` layer, and Window Server composites from that
context. When the user clicks a link, Chromium navigates to a new page. This may
trigger a renderer process swap (site isolation) or a new compositor surface,
either of which produces a new `CAContext` with a new `ca_context_id`. The old
context becomes invalid, and the overlay vanishes.

### The ca_context_id lifecycle

The `ca_context_id` is produced by Chromium's GPU process and delivered to our
code via the `CALayerParams` callback chain:

```
GPU Process creates CAContext
    ↓
RenderWidgetHostViewMac receives CALayerParams
    ↓
TermSurf callback (registered via SetCALayerParamsCallbackOnView)
    ↓
XPC message: action="ca_context", ca_context_id=N, pane_id=...
    ↓
GUI xpc.zig handleCAContext() → Surface.setCAContextId()
    ↓
Metal.setCALayerHostContextId() — creates or updates CALayerHost
```

The callback is a `BindRepeating` with deduplication — it only sends when
`ca_context_id != 0` and `ca_context_id != last_id`:

```cpp
if (params.ca_context_id == 0 || params.ca_context_id == *last_id)
    return;
*last_id = params.ca_context_id;
// send XPC message...
```

### Two potential failure points

**1. The callback may not survive navigation.**

The callback is registered on a specific `RenderWidgetHostView`. On cross-site
navigation, Chromium may destroy the old `RenderWidgetHostView` and create a new
one (renderer process swap). The callback would be lost with the old view. The
new view would never send `ca_context_id` updates because no callback was
registered on it.

**2. Updating `contextId` on an existing `CALayerHost` may not work.**

Even if the callback survives and the GUI receives the new `ca_context_id`, our
code updates the property on the existing `CALayerHost`:

```zig
if (ca_layer_host_ptr.*) |existing| {
    const host = objc.Object.fromId(existing);
    host.setProperty("contextId", @as(u32, context_id));
}
```

But Chromium's own `DisplayCALayerTree::GotCALayerFrame()` does NOT do this. It
creates a **brand new** `CALayerHost` every time the `ca_context_id` changes:

```cpp
void DisplayCALayerTree::GotCALayerFrame(uint32_t ca_context_id) {
    if (remote_layer_.contextId == ca_context_id)
        return;

    CALayerHost* new_remote_layer = [[CALayerHost alloc] init];
    new_remote_layer.anchorPoint = CGPointZero;
    new_remote_layer.contextId = ca_context_id;
    new_remote_layer.autoresizingMask = kCALayerMaxXMargin | kCALayerMaxYMargin;

    [maybe_flipped_layer_ addSublayer:new_remote_layer];
    [remote_layer_ removeFromSuperlayer];
    remote_layer_ = new_remote_layer;
}
```

This suggests that Window Server may not re-bind to a new remote context when
`contextId` is changed on an existing `CALayerHost`. The safe approach is to
match Chromium's pattern: destroy the old `CALayerHost` and create a new one.

### Current layer tree (from Issue 627)

```
IOSurfaceLayer (Y=0 at bottom)
└─ flipped_layer (geometryFlipped=YES, auto-fills parent)
   └─ positioning_layer (explicit frame at overlay rect, top-origin Y)
      └─ CALayerHost (at origin, contextId set once)
```

The `flipped_layer` and `positioning_layer` should survive navigation — they are
independent of the `ca_context_id`. Only the `CALayerHost` needs to be replaced.

### Chromium branch

`146.0.7650.0-issue-628`, branched from `146.0.7650.0-issue-627`.

## Experiments

### Experiment 1: Re-register callback on view swap, replace CALayerHost on context change

Fix both failure points identified in the analysis.

#### Chromium side: re-register callback after navigation

The `SetCALayerParamsCallbackOnView` callback is registered once in
`CreateTab()` on the initial `RenderWidgetHostView`. If navigation causes a
renderer process swap, the old view is destroyed and the callback is lost.

`ShellTabObserver` already extends `WebContentsObserver` and has
`xpc_connection_` and `pane_id_`. `WebContentsObserver` provides
`RenderViewHostChanged(old_host,
new_host)`, which fires when the `WebContents`
swaps its `RenderViewHost` — exactly when the `RenderWidgetHostView` changes.

Move the CALayerParams callback registration into `ShellTabObserver` so it can
re-register after a view swap:

1. Add a `RegisterCALayerParamsCallback()` method to `ShellTabObserver` that
   gets the current `RenderWidgetHostView` from `web_contents()`, and calls
   `SetCALayerParamsCallbackOnView` with the same callback logic currently in
   `CreateTab()`. Track the `last_id` as a member variable (not `base::Owned`)
   so it persists across re-registrations.
2. Override `RenderViewHostChanged()` in `ShellTabObserver`. In the override,
   call `RegisterCALayerParamsCallback()` to re-register on the new view.
3. In `CreateTab()`, replace the inline callback registration with a call to
   `tab_observer->RegisterCALayerParamsCallback()`.

#### GUI side: replace CALayerHost instead of updating contextId

Match Chromium's `DisplayCALayerTree::GotCALayerFrame()` pattern. When the
`ca_context_id` changes on an existing overlay, destroy the old `CALayerHost`
and create a new one inside the existing `positioning_layer`.

In `Metal.setCALayerHostContextId`, change the existing-host branch: instead of
`host.setProperty("contextId", context_id)`, remove the old `CALayerHost` from
its superlayer and release it, then create a new `CALayerHost` with the new
`contextId`, set `anchorPoint = zero` and
`autoresizingMask = kCALayerMaxXMargin | kCALayerMaxYMargin`, add it as a
sublayer of the `positioning_layer`, and update `ca_layer_host_ptr`.

The `positioning_layer` pointer is needed in `setCALayerHostContextId` for this.
Pass it as a parameter (it's already stored in `generic.zig`).

#### Changes

**`chromium/.../shell_tab_observer.h`:**

- Add `void RegisterCALayerParamsCallback()` declaration.
- Add `void RenderViewHostChanged(RenderViewHost*, RenderViewHost*)` override.
- Add `uint32_t last_ca_context_id_ = 0` member.

**`chromium/.../shell_tab_observer.cc`:**

- Implement `RegisterCALayerParamsCallback()`: get
  `web_contents()->GetRenderWidgetHostView()`, guard on null, call
  `SetCALayerParamsCallbackOnView` with a lambda that sends the `"ca_context"`
  XPC message. Use `&last_ca_context_id_` for deduplication (member, not owned).
- Implement `RenderViewHostChanged()`: log the swap, call
  `RegisterCALayerParamsCallback()`.
- Add `#include "shell_ca_layer_bridge_mac.h"` for the bridge function.

**`chromium/.../shell_browser_main_parts.cc`:**

- In `CreateTab()`, remove the inline `SetCALayerParamsCallbackOnView` block.
  Replace with `tab_observer->RegisterCALayerParamsCallback()`.

**`gui/src/renderer/Metal.zig`:**

- Change `setCALayerHostContextId` to accept the `positioning_layer` pointer.
- In the existing-host branch: remove old CALayerHost from superlayer, release
  it, create a new one, add to `positioning_layer`, update `ca_layer_host_ptr`.

**`gui/src/renderer/generic.zig`:**

- Pass `self.ca_layer_positioning` to `setCALayerHostContextId`.

#### Verification

Run the app, open a browser overlay at `google.com`, search for something. The
search results page should render — the overlay should not vanish. Test clicking
links on the results page. Test navigating back with Cmd+[. Test cross-site
navigation (e.g., clicking a link from Google to Wikipedia).

#### Results

**Partial.** Navigation works — clicking a link no longer permanently kills the
overlay. The new page renders after navigation. But two issues remain:

1. **New page renders at stale size.** If the pane was resized before clicking a
   link, the new page renders at the original size (from tab creation), not the
   current size. Resizing the window again fixes it. The likely cause: when
   `RenderViewHostChanged` fires and the callback re-registers on the new view,
   the new `RenderWidgetHostView` has its default size, not the size that was
   set on the old view. The resize needs to be re-applied to the new view.

2. **Blank gap between pages.** When navigating, the old page vanishes
   completely (the old CALayerHost is destroyed when the new `ca_context_id`
   arrives), then there is a visible blank period before the new page loads. A
   normal browser keeps the old page visible until the new one is ready. The old
   `CALayerHost` should be kept alive until the new `ca_context_id` arrives,
   rather than being destroyed as soon as the `RenderViewHost` swaps.

#### Conclusion

The core fix works: `RenderViewHostChanged` re-registers the CALayerParams
callback on the new view, and the GUI replaces the `CALayerHost` when the new
`ca_context_id` arrives. Navigation no longer kills the overlay permanently.

The two remaining issues need separate fixes:

- **Stale size:** Re-apply the current pixel dimensions to the new
  `RenderWidgetHostView` in `RenderViewHostChanged`.
- **Blank gap:** Defer destruction of the old `CALayerHost` until the
  replacement `ca_context_id` arrives, rather than destroying it eagerly.
