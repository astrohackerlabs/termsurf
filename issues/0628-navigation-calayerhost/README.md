+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

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

### Experiment 2: Re-apply size to new RenderWidgetHostView after navigation

#### Problem

After navigation triggers a `RenderViewHostChanged`, the new
`RenderWidgetHostView` has its default size, not the size that was applied via
`ResizeTab()`. If the pane was resized between tab creation and navigation, the
new page renders at the original creation size.

`ResizeTab()` calls `view->SetSize(logical)` on the current RWHV, but that view
is destroyed on cross-site navigation. The new RWHV knows nothing about the
previous resize.

#### Solution

Store the last pixel dimensions in `ShellTabObserver`. When `ResizeTab()` is
called, update the stored dimensions on the observer (via a new setter). When
`RenderViewHostChanged()` fires, re-apply the stored dimensions to the new view
— the same `view->SetSize(logical)` call that `ResizeTab()` uses.

This is a Chromium-only change. No GUI changes needed.

#### Changes

**`chromium/.../shell_tab_observer.h`:**

- Add `void SetLastPixelSize(int width, int height)`.
- Add `int last_pixel_width_ = 0` and `int last_pixel_height_ = 0` members.

**`chromium/.../shell_tab_observer.cc`:**

- Implement `SetLastPixelSize`: store width and height.
- In `RenderViewHostChanged`: after `RegisterCALayerParamsCallback()`, get the
  new `RenderWidgetHostView`, compute logical size from stored pixel dimensions
  and `view->GetDeviceScaleFactor()`, call `view->SetSize(logical)`.

**`chromium/.../shell_browser_main_parts.cc`:**

- In `CreateTab()`: after the initial `view->SetSize()`, call
  `tab_observer->SetLastPixelSize(pixel_width, pixel_height)`.
- In `ResizeTab()`: after `view->SetSize()`, call
  `tab->tab_observer->SetLastPixelSize(pixel_width, pixel_height)`.

#### Verification

Run the app, open a browser overlay, resize the window, then click a link. The
new page should render at the current pane size, not the original creation size.

#### Results

**Fail.** The new page still renders at the original creation size, not the
resized size. The fix had no effect.

#### Conclusion

`RenderViewHostChanged` does not fire for the navigations being tested.
Same-site navigations (e.g., clicking a link on Google) don't swap the
`RenderViewHost` — the same view persists. Cross-site navigations may also not
trigger it if site isolation isn't fully enabled in the Profile Server
configuration. The re-apply code never runs because the hook never fires.

The fix needs a different hook point — one that fires on every navigation, not
just view swaps. The CALayerParams callback itself is a candidate: when a new
`ca_context_id` arrives (meaning a new compositor surface was created), re-apply
the stored size to the current view.

### Experiment 3: Research how Electron handles resize across navigation

#### Problem

After navigation, the new page renders at the original creation size instead of
the current resized size. Experiment 2 tried re-applying the size in
`RenderViewHostChanged`, but that hook doesn't fire for same-site navigations.
We need a different approach.

Electron's `BrowserWindow` handles this correctly — you can resize a window and
navigate, and the new page renders at the current size. Electron uses the same
Chromium Content API that we do. Understanding how Electron maintains size
across navigations will reveal the correct approach.

#### Research questions

**R1: Does Electron call `view->SetSize()` at all?**

Electron's `BrowserWindow` has a fixed window frame. Does it ever explicitly
call `SetSize()` on the `RenderWidgetHostView`, or does the view automatically
inherit the window/NSView size? If the view auto-sizes to its parent NSView, the
resize problem doesn't exist for Electron — the view always matches the window.

Look in `vendor/electron/shell/browser/` for calls to `SetSize`, `Resize`, or
`SetBounds` on `RenderWidgetHostView`.

**R2: How does Chromium's normal display path handle view sizing?**

In stock Chromium (not content_shell), how does the `RenderWidgetHostView` get
its size? Is it set explicitly, or does it follow the NSView frame? When the
window resizes, does something call `SetSize()`, or does the view observe its
parent's bounds change?

Look in
`chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm` for
sizing logic — `setFrameSize`, `viewDidChangeBackingProperties`,
`boundsDidChange`, or similar NSView layout methods.

**R3: Why does content_shell need explicit `SetSize()` calls?**

Our Profile Server is based on content_shell. Content_shell creates a hidden
NSWindow with a `WebContentsViewCocoa`. The `RenderWidgetHostView` lives inside
that view hierarchy. When we call `view->SetSize()`, we're setting the view's
frame explicitly because the hidden window doesn't have normal window
management.

Is there an alternative? Could we resize the hidden NSWindow instead, letting
the normal NSView layout propagate the size to the `RenderWidgetHostView`? Would
that survive navigation automatically?

Look in
`chromium/src/content/chromium_profile_server/browser/shell_platform_delegate_mac.mm`
and `chromium/src/content/shell/browser/shell_platform_delegate_mac.mm` for how
the window and web contents view are set up.

**R4: Does the `RenderWidgetHostView` survive same-site navigation?**

Confirm or refute the Experiment 2 hypothesis. After a same-site navigation
(e.g., clicking a link on google.com that stays on google.com), is the
`RenderWidgetHostView` the same object? If so, the `SetSize()` from
`ResizeTab()` should still be in effect, and the problem is elsewhere — perhaps
the compositor creates a new surface at a default size before the view's size
takes effect.

Look at `RenderWidgetHostViewMac` lifecycle during navigation.

#### Verification

Research is complete when we can answer all four questions and propose a
concrete fix based on how Electron/Chromium maintains view size across
navigation.

#### Results

**Pass.** All four questions answered.

**R1: Does Electron call `view->SetSize()` at all?**

Yes. Electron explicitly calls `SetSize()` on its off-screen
`RenderWidgetHostView` whenever the window resizes. It does NOT rely on NSView
auto-layout. The call chain: `NativeWindow::NotifyWindowResize()` →
`OffScreenWebContentsView::OnWindowResize()` → `view->SetSize(GetSize())`. The
macOS NSView is a dummy placeholder — all sizing is explicit and programmatic.

Key files:

- `vendor/electron/shell/browser/osr/osr_web_contents_view.cc:62-66`
- `vendor/electron/shell/browser/osr/osr_render_widget_host_view.cc:289-296`

**R2: How does Chromium's normal display path handle view sizing?**

The RWHV gets its initial size from `SetParentWebContentsNSView()`, which copies
the parent's bounds and sets
`autoresizingMask = NSViewWidthSizable |
NSViewHeightSizable` on the Cocoa view.
After that, autoresizing keeps the view sized to its parent automatically. When
the NSView frame changes (from autoresizing or explicit `SetBounds`),
`setFrameSize:` fires → `sendViewBoundsInWindowToHost` →
`OnBoundsInWindowChanged()` → `UpdateScreenInfo()` →
`UpdateSurfaceFromNSView()`. This updates `dfh_size_dip_` in
`BrowserCompositorMac`, which is the size used when creating new compositor
surfaces.

Key files:

- `chromium/src/content/app_shim_remote_cocoa/render_widget_host_ns_view_bridge.mm:94-108`
- `chromium/src/content/app_shim_remote_cocoa/render_widget_host_view_cocoa.mm:1763-1766`
- `chromium/src/content/browser/renderer_host/render_widget_host_view_mac.mm:894-942`

**R3: Why does content_shell need explicit `SetSize()` calls?**

Our `SetContents` in `shell_platform_delegate_mac.mm` adds the web view to the
hidden NSWindow's contentView with
`autoresizingMask = NSViewWidthSizable |
NSViewHeightSizable`. But `ResizeTab()`
only calls `view->SetSize()` — it never resizes the hidden NSWindow itself. This
creates a mismatch: the NSView frame is set to the new size via `SetBounds()`,
but the NSWindow remains at its creation size. The autoresizing mask ties the
web view to the window's contentView bounds, so the NSView frame and the window
are inconsistent.

Resizing the NSWindow instead of (or in addition to) calling `view->SetSize()`
would let the normal autoresizing chain propagate size correctly: NSWindow
resize → contentView resize → web_view autoresize → `setFrameSize:` →
`sendViewBoundsInWindowToHost` → `OnBoundsInWindowChanged` → `UpdateScreenInfo`
→ `UpdateSurfaceFromNSView`.

Key files:

- `chromium/src/content/chromium_profile_server/browser/shell_platform_delegate_mac.mm:229-245`
- `chromium/src/content/chromium_profile_server/browser/shell_browser_main_parts.cc:417-452`

**R4: Does the `RenderWidgetHostView` survive same-site navigation?**

Yes. Same-site navigations reuse the current `RenderFrameHost` and its
`RenderWidgetHostView`. The view is the same object — it is not destroyed and
recreated. The `ca_context_id` changes because
`BrowserCompositorMac::DidNavigate()` generates a new `LocalSurfaceId`, which
creates a new compositor surface. The new surface is created with
`dfh_size_dip_` — whatever size `BrowserCompositorMac` thinks the view is.

This is the root cause: `DidNavigate()` calls `EmbedSurface()` with
`dfh_size_dip_`. If `dfh_size_dip_` reflects the NSWindow's contentView bounds
(from autoresizing) rather than the explicitly-set view size, the new surface
will be created at the wrong size.

Key files:

- `chromium/src/content/browser/renderer_host/browser_compositor_view_mac.mm:341-361`
- `chromium/src/content/browser/renderer_host/render_frame_host_manager.cc:1865-2021`

#### Conclusion

The stale-size-after-navigation bug has a clear root cause.

`ResizeTab()` calls `view->SetSize()` which sets the NSView frame and updates
`dfh_size_dip_` via the bounds-change callback chain. This works initially — the
page renders at the correct size. But the hidden NSWindow remains at its
creation size. The web view has `autoresizingMask` tying it to the window's
contentView, creating tension between the explicitly-set frame and the
autoresizing constraint.

During navigation, `BrowserCompositorMac::DidNavigate()` creates a new
compositor surface using `dfh_size_dip_`. If the autoresizing constraint has
pulled the NSView back to the window size, or if `dfh_size_dip_` was reset
during the navigation lifecycle, the new surface renders at the wrong (original)
size.

The fix is to resize the hidden NSWindow in `ResizeTab()` instead of calling
`view->SetSize()` directly. When the NSWindow resizes, the autoresizing chain
propagates naturally: contentView → web_view → RWHV Cocoa view → `setFrameSize:`
→ `sendViewBoundsInWindowToHost` → `OnBoundsInWindowChanged` →
`UpdateScreenInfo` → `UpdateSurfaceFromNSView`. This updates `dfh_size_dip_`
through the standard Chromium path, and the size survives navigation because the
NSWindow stays at the correct size.

### Experiment 4: Resize the NSWindow instead of calling view->SetSize()

#### Problem

`ResizeTab()` calls `view->SetSize(logical)` to resize the
`RenderWidgetHostView` directly. This sets the NSView frame but never resizes
the hidden NSWindow. The web view has
`autoresizingMask = NSViewWidthSizable |
NSViewHeightSizable`, tying it to the
window's contentView. When navigation triggers
`BrowserCompositorMac::DidNavigate()`, the new compositor surface is created
with `dfh_size_dip_`, which may reflect the window's original size rather than
the explicitly-set view size.

#### Solution

Resize the NSWindow instead of calling `view->SetSize()`. The
`Shell::ResizeWebContentForTests()` method already calls
`ShellPlatformDelegate::ResizeWebContent()`, which has access to the NSWindow in
the `.mm` file. Fix `ResizeWebContent` to set the window's content size (not
just `contentView.frame`), then use `ResizeWebContentForTests` from both
`CreateTab()` and `ResizeTab()`.

The autoresizing chain handles the rest: window resize → contentView resize →
web_view autoresize → RWHV Cocoa view `setFrameSize:` →
`sendViewBoundsInWindowToHost` → `OnBoundsInWindowChanged` → `UpdateScreenInfo`
→ `UpdateSurfaceFromNSView` → `dfh_size_dip_` updated.

#### Changes

**`chromium/.../shell_platform_delegate_mac.mm`:**

- In `ResizeWebContent()`, replace
  `shell_data.delegate.window.contentView.frame = frame` with
  `[shell_data.delegate.window setContentSize:NSMakeSize(width, height)]`. This
  resizes the window itself, letting autoresizing propagate to all subviews.

**`chromium/.../shell_browser_main_parts.cc`:**

- In `CreateTab()`, replace `view->SetSize(logical)` with
  `shell->ResizeWebContentForTests(logical)`.
- In `ResizeTab()`, replace `view->SetSize(logical)` with
  `tab->shell->ResizeWebContentForTests(logical)`.

#### Verification

Run the app, open a browser overlay, resize the window, then click a link. The
new page should render at the current pane size, not the original creation size.
Test same-site navigation (link within google.com) and cross-site navigation
(link from Google to Wikipedia). Both should preserve the resized dimensions.

#### Results

**Pass.** Navigation preserves the resized dimensions. The new page renders at
the current pane size after both same-site and cross-site navigation.

#### Conclusion

The root cause was confirmed: `view->SetSize()` set the RWHV's NSView frame but
left the hidden NSWindow at its creation size. The autoresizing mask created
tension between the two, and `BrowserCompositorMac::DidNavigate()` used the
wrong size when creating the new compositor surface.

Resizing the NSWindow via `[window setContentSize:]` lets the standard
autoresizing chain propagate the size through the entire view hierarchy. The
`dfh_size_dip_` in `BrowserCompositorMac` stays correct across navigations
because it derives from the actual NSView bounds, which follow the window.

### Experiment 5: Research the vanishing overlay during navigation

#### Problem

When the user clicks a link, the overlay vanishes immediately and stays blank
for ~10 seconds before the new page appears. This is far longer than the actual
page load time. A normal browser keeps the old page visible until the new one is
ready — the transition should be nearly seamless.

The current code replaces the `CALayerHost` when a new `ca_context_id` arrives
(Experiment 1). But there are two timing issues:

1. **The old CALayerHost is destroyed too early.** The old `ca_context_id` may
   become invalid as soon as the GPU process tears down the old compositor
   surface, which happens before the new page has rendered. The overlay goes
   blank.

2. **The new `ca_context_id` arrives late.** The CALayerParams callback fires
   only when the GPU process has a fully composited frame for the new page. If
   the page takes time to load/render, the callback is delayed.

The result: the old content vanishes immediately, and there's a long gap before
the new content appears. The ~10 second delay suggests something beyond normal
page load — possibly the new `ca_context_id` is not being sent at all until some
timeout or fallback triggers.

#### Research questions

**R1: How does Chromium's normal browser handle the transition between old and
new content during navigation?**

In stock Chromium (not content_shell), when the user clicks a link, the old page
stays visible until the new page is ready. How is this achieved? Is there a
"deadline" mechanism where the old surface is kept alive until the new one
produces its first frame?

Look at:

- `BrowserCompositorMac::DidNavigate()` — the `DeadlinePolicy` parameter in
  `EmbedSurface()`.
- `DelegatedFrameHost` — how it manages the transition between old and new
  surfaces.
- `DisplayCALayerTree::GotCALayerFrame()` — does it keep the old remote layer
  until the new one has content?

**R2: How does Electron handle navigation transitions in off-screen rendering?**

Electron renders off-screen like we do. When navigating, does Electron show the
old frame until the new one arrives? Does it have a "hold" mechanism?

Look at:

- `vendor/electron/shell/browser/osr/osr_render_widget_host_view.cc` — search
  for `DidNavigate`, `DidNavigateMainFrame`, `SurfaceId`, `EmbedSurface`,
  `deadline`, or `fallback`.
- `vendor/electron/shell/browser/osr/osr_video_consumer.cc` — does the video
  consumer handle navigation transitions?

**R3: What is the `DeadlinePolicy` in `EmbedSurface`, and how does it control
the old-to-new surface transition?**

`BrowserCompositorMac::DidNavigate()` calls:

```cpp
delegated_frame_host_->EmbedSurface(
    ..., dfh_size_dip_, cc::DeadlinePolicy::UseExistingDeadline());
```

What does `UseExistingDeadline` mean? What other policies are available? Does
the deadline control how long the old surface stays visible? If the deadline
expires before the new surface produces a frame, does the compositor show a
blank?

Look at:

- `chromium/src/cc/trees/deadline_policy.h`
- `chromium/src/components/viz/common/surfaces/` — surface lifecycle
- `DelegatedFrameHost::EmbedSurface()` — how it applies the deadline

**R4: Is our CALayerParams callback firing correctly during navigation?**

The ~10 second delay suggests the callback may not be firing when expected. Add
logging to understand the timeline:

- When does the old `ca_context_id` become invalid?
- When does the new `ca_context_id` arrive?
- Is there a gap where no callback fires at all?

Check the `RegisterCALayerParamsCallback` implementation: is the callback still
registered on the correct view after navigation? Does same-site navigation
preserve the callback? Does the deduplication logic (`last_ca_context_id_`)
accidentally filter out the new ID?

#### Verification

Research is complete when we can answer all four questions and propose a
concrete fix that keeps the old content visible during navigation until the new
content is ready.

#### Results

**Pass.** All four questions answered. The root cause is identified.

**R1: How does Chromium's normal browser keep old content visible?**

Chromium uses a **SurfaceRange fallback mechanism** in the viz compositor:

1. `DelegatedFrameHost::DidNavigateMainFramePreCommit()` invalidates the primary
   surface but preserves the old surface as a fallback via
   `SurfaceRange(fallback, primary)`.
2. `EmbedSurface()` sets the new primary surface with a `DeadlinePolicy`. The
   compositor renders the fallback (old content) while waiting for the primary
   (new content) to produce its first frame.
3. `SurfaceManager::GetLatestInFlightSurface()` tries the primary first; if no
   active frame, it falls back to the old surface.
4. `DisplayCALayerTree::GotCALayerFrame()` adds the new CALayerHost before
   removing the old one — an atomic visual swap.
5. For cross-site navigation, `TakeFallbackContentFrom()` transfers the old
   view's surface to the new view as a fallback.

The key insight: this mechanism operates **inside** the viz compositor pipeline.
The CAContext is the final output — whatever the compositor renders (primary or
fallback) goes into the CAContext's layer tree.

**R2: How does Electron handle navigation transitions?**

Electron delegates entirely to the same `DelegatedFrameHost` mechanism:

```cpp
void OffScreenRenderWidgetHostView::DidNavigate() {
  ResizeRootLayer(true);
  if (delegated_frame_host())
    delegated_frame_host()->DidNavigate();
}
```

Electron also uses `HoldResize()` / `ReleaseResize()` to freeze layout during
transitions, preventing intermediate states.

**R3: What is DeadlinePolicy?**

Four types: `UseExistingDeadline` (preserve current countdown),
`UseDefaultDeadline` (4 frames), `UseSpecifiedDeadline(N)` (N frames),
`UseInfiniteDeadline` (wait forever).

`BrowserCompositorMac::DidNavigate()` uses `UseExistingDeadline`, meaning "don't
change the deadline — render immediately with whatever is available." Combined
with the SurfaceRange fallback, this means: render the fallback (old content)
immediately while waiting for the new primary surface.

The fallback surface mechanism is entirely within viz. `SurfaceLayer` maintains
a `SurfaceRange(start, end)` where `start` is the fallback and `end` is the
primary. `SetSurfaceId()` preserves the fallback when updating the primary.

**R4: Is the callback firing correctly?**

The `CAContext` is created **once** per `CALayerTreeCoordinator` (in the GPU
process) and never recreated. The `ca_context_id` is `[ca_context_ contextId]` —
it stays the same for the lifetime of the compositor output. During same-site
navigation, the `ca_context_id` does not change.

This means:

- Our deduplication (`params.ca_context_id == observer->last_ca_context_id_`)
  blocks the callback after navigation — the ID hasn't changed.
- But that's fine: the CALayerHost is already bound to the correct CAContext.
  Window Server composites whatever the GPU renders into that context.

The 10-second gate in
`RootCompositorFrameSinkImpl::DisplayDidReceiveCALayerParams` (line 901)
compares the entire `CALayerParams` struct including `ca_context_id`. Since
`ca_context_id` doesn't change during same-site navigation, and the pixel size
likely stays the same (Experiment 4 fixed that), the params ARE identical, and
the gate blocks the callback. But this shouldn't matter — the CALayerHost
doesn't need updating.

The `CALayerParams::operator==` comparison includes `is_empty`, `ca_context_id`,
`io_surface_mach_port`, `pixel_size`, and `scale_factor`.

#### Conclusion

The overlay vanishes during navigation not because of a callback or CALayerHost
issue, but because **the viz compositor clears its output** during the surface
transition. The SurfaceRange fallback mechanism should prevent this, but it may
not be working correctly in our hidden-window configuration.

The `CAContext` is the final output of the viz compositor pipeline. The
`ca_context_id` stays the same across navigation. Our `CALayerHost` stays bound
to the correct context. The content visible through the CALayerHost is whatever
the compositor renders into that context. If the compositor outputs nothing
during the transition (empty frame, cleared layer tree), the CALayerHost shows
blank.

In Chromium's normal browser, the SurfaceRange fallback ensures the compositor
always outputs the old content until the new content is ready. But our setup
uses a hidden, borderless NSWindow. The `DelegatedFrameHost::EmbedSurface()`
method has an early return:

```cpp
if (!client_->DelegatedFrameHostIsVisible()) {
    return;  // Don't embed if hidden
}
```

If our hidden window causes `DelegatedFrameHostIsVisible()` to return false
during or after navigation, `EmbedSurface()` would skip embedding entirely. The
compositor would have no surface to render, and the CAContext output would go
blank.

The next experiment should add logging to confirm this hypothesis: check whether
`DelegatedFrameHostIsVisible()` returns false during navigation, and whether the
fallback surface is properly set up.

Further research after Experiment 5 refined the hypothesis.
`DelegatedFrameHostIsVisible()` returns `state_ != HasNoCompositor`, which
checks compositor state, not window visibility. Since the page renders
initially, the compositor is running and the state is `HasOwnCompositor`. The
visibility check is not the issue.

The real issue is the **missing fallback surface**. During same-site navigation,
`BrowserCompositorMac::DidNavigate()` generates a new `LocalSurfaceId` and calls
`EmbedSurface()` with the new primary surface. But `SurfaceRange.start()` (the
fallback) is never set. The compositor tries to render the new primary (no frame
yet) and has no fallback to show, so it outputs blank content into the
CAContext. The CALayerHost shows the blank.

`SetOldestAcceptableFallback()` sets `SurfaceRange.start()` — exactly the
fallback the compositor needs. Chromium calls it for BFCache restoration and
cross-site view swaps, but never for normal same-site navigation. This is the
gap.

### Experiment 6: Set fallback surface before navigation

#### Problem

When `BrowserCompositorMac::DidNavigate()` generates a new `LocalSurfaceId` and
embeds the new primary surface, the viz compositor has no fallback. The
`SurfaceRange` is `(nullopt, new_primary)`. The compositor tries the primary (no
frame yet), finds nothing, and outputs blank content. The CAContext goes blank,
and the CALayerHost shows nothing until the renderer produces the new page's
first frame.

In Chromium's normal browser, this blank may be imperceptible (the compositor
runs at vsync rate and the renderer produces frames quickly). But in our setup,
the blank is visible through the CALayerHost overlay and persists until the new
page renders.

#### Solution

Set the current primary surface as the fallback before generating the new
`LocalSurfaceId`. This makes the `SurfaceRange` be `(old_primary, new_primary)`.
The compositor renders the old content while waiting for the new primary to
produce its first frame — exactly how a normal browser keeps the old page
visible during navigation.

#### Changes

**`chromium/src/content/browser/renderer_host/browser_compositor_view_mac.mm`:**

In `BrowserCompositorMac::DidNavigate()`, before the
`dfh_local_surface_id_allocator_.GenerateId()` call, add:

```cpp
// Set the current surface as fallback so the compositor shows old content
// while waiting for the new page's first frame (Issue 628).
const viz::SurfaceId* current_primary =
    DelegatedFrameHostGetLayer()->GetSurfaceId();
if (current_primary) {
    DelegatedFrameHostGetLayer()->SetOldestAcceptableFallback(
        *current_primary);
}
```

This is the same pattern Chromium uses for BFCache restoration
(`DelegatedFrameHost::EmbedSurface`, line 336) and cross-site view swaps
(`TakeFallbackContentFrom`, line 696). We're applying it to same-site
navigation, where it was missing.

#### Verification

Run the app, open a browser overlay at `google.com`, click a search result. The
old page should stay visible until the new page renders — no blank gap. Test
same-site navigation (staying on google.com) and cross-site navigation (Google
to Wikipedia). Both should show the old content during the transition.

#### Results

**Fail.** The overlay still vanishes on navigation and reappears several seconds
later. Setting the fallback surface had no effect.

#### Conclusion

The missing fallback surface hypothesis was wrong, or the fallback mechanism
does not prevent the blank in our setup. The viz compositor may be outputting
blank frames for a different reason — possibly the CAContext's layer tree is
being cleared during the surface switch regardless of the fallback, or the
compositor is not running at all during the transition.

The ~seconds-long delay before reappearance is too long for normal page
rendering. Something is fundamentally blocking or delaying the compositor output
pipeline during navigation. The next experiment needs empirical data — add
logging to trace exactly what happens: when does the compositor stop producing
frames, when does it resume, and what triggers the resume.

### Experiment 7: Add logging to trace the navigation blank

#### Problem

Six experiments of analysis have not identified the root cause. Hypotheses about
callback timing, fallback surfaces, and visibility checks have all failed to
explain the blank. We need empirical data from the running system to understand
what actually happens during navigation.

#### Approach

Add logging at every point in the pipeline to build a timeline of what happens
when the user clicks a link. The logs will answer:

1. When does the CALayerParams callback stop firing?
2. When does it resume?
3. Does the `ca_context_id` change?
4. What is the compositor state during navigation?
5. Is `render_widget_host_is_hidden_` true or false?
6. How long is the gap between the last old-page callback and the first new-page
   callback?

#### Changes

**`chromium/.../shell_tab_observer.cc` — CALayerParams callback:**

Log ALL invocations, not just new `ca_context_id` values. Currently the callback
silently returns when `ca_context_id == 0` or `ca_context_id == last_id`. Add
logging before the early return so we can see every callback, including filtered
ones. Include the `ca_context_id`, `pixel_size`, `is_empty`, and whether the
callback was filtered.

```cpp
[](ShellTabObserver* observer, const gfx::CALayerParams& params) {
    bool filtered = (params.ca_context_id == 0 ||
                     params.ca_context_id == observer->last_ca_context_id_);
    LOG(INFO) << "[CALayerParams] pane=" << observer->pane_id_
              << " ca_context_id=" << params.ca_context_id
              << " size=" << params.pixel_size.width()
              << "x" << params.pixel_size.height()
              << " empty=" << params.is_empty
              << (filtered ? " FILTERED" : " NEW");
    if (filtered)
        return;
    // ... existing send logic ...
}
```

**`chromium/.../browser_compositor_view_mac.mm` — DidNavigate():**

Log the compositor state when navigation fires:

```cpp
LOG(INFO) << "[BrowserCompositorMac] DidNavigate"
          << " hidden=" << render_widget_host_is_hidden_
          << " state=" << state_
          << " first_nav=" << is_first_navigation_;
```

**`chromium/.../render_widget_host_view_mac.mm` —
AcceleratedWidgetCALayerParamsUpdated():**

Log when params are delivered to our callback, and whether params exist:

```cpp
LOG(INFO) << "[RWHVM] AcceleratedWidgetCALayerParamsUpdated"
          << " has_params=" << (ca_layer_params != nullptr)
          << " has_callback=" << (!!ca_layer_params_callback_);
```

This will fire at vsync rate, so it will be verbose. But during the blank period
we need to see whether it stops firing entirely or continues with empty params.

#### Verification

Run the app, open a browser overlay, click a link. Examine the logs for the
timeline between navigation start and the overlay reappearing. The logs should
reveal the exact gap — when the compositor stops delivering params, whether the
callback is still registered, and what triggers the resumption.

#### Results

**Pass.** The logs reveal the root cause.

**Timeline of the navigation event:**

```
04:25:30.552  AcceleratedWidgetCALayerParamsUpdated: ca_context_id=3867294868
              → NEW, sent to GUI → GUI creates CALayerHost → visible ✓
04:25:30.593  DidNavigate: hidden=0 state=0 first_nav=1 (initial load)
04:25:30.723  DidStopLoading (initial page loaded)

04:25:34.128  Mouse down (user clicks link)
04:25:34.192  DidStartLoading
04:25:34.241  RenderViewHostChanged → new RWHV → new callback registered
04:25:34.242  DidNavigate: hidden=0 state=0 first_nav=1 (new compositor)
04:25:34.264  DidStopLoading (new page loaded in 70ms!)
04:25:34.284  AcceleratedWidgetCALayerParamsUpdated: ca_context_id=9392910
              → NEW, sent to GUI → GUI replaces CALayerHost → BLANK

              ~~~ 13.5 seconds of ZERO AcceleratedWidgetCALayerParamsUpdated ~~~

04:25:47.808  Key down (user gives up, exits)
```

**Key findings:**

1. **Chromium is fast.** The new `ca_context_id` (9392910) is sent 100ms after
   the click. The page loads in 70ms. There is no server-side delay.

2. **The ca_context_id changes** from 3867294868 to 9392910 because
   `RenderViewHostChanged` creates a new RWHV → new `BrowserCompositorMac` → new
   `CALayerTreeCoordinator` → new `CAContext`.

3. **The GUI receives and replaces the CALayerHost immediately.** The log shows
   `replaced CALayerHost contextId=9392910` right after the XPC message.

4. **After the single callback at 34.284, there are ZERO more
   `AcceleratedWidgetCALayerParamsUpdated` calls for 13.5 seconds.** This is the
   10-second dedup gate in `RootCompositorFrameSinkImpl`.

#### Conclusion

The 10-second dedup gate in
`RootCompositorFrameSinkImpl::DisplayDidReceiveCALayerParams()` starves the new
CAContext of frame updates.

The sequence:

1. New RWHV creates a new `CALayerTreeCoordinator` with a new `CAContext`.
2. The GPU process reports the new `ca_context_id` via `CALayerParams`.
3. `DisplayDidReceiveCALayerParams` passes the dedup gate (new ID ≠ old ID).
4. `AcceleratedWidgetCALayerParamsUpdated` fires → `SetCALayerParams` is called
   on the NSView → our callback sends the new ID to the GUI via XPC.
5. The GUI receives the XPC message and creates a new `CALayerHost` with the new
   `contextId`.
6. But `SetCALayerParams` was called on the NSView **before** the GUI's
   `CALayerHost` was connected to the new `CAContext`. The Window Server
   composited the new `CAContext`'s content at step 4, but no `CALayerHost` was
   watching yet.
7. Subsequent compositor frames produce identical `CALayerParams` (same
   `ca_context_id`, same `pixel_size`). The dedup gate blocks them all for 10
   seconds.
8. Without `SetCALayerParams` being called again, no new composite cycle is
   triggered. The Window Server doesn't know to re-composite the CAContext for
   the newly-connected `CALayerHost`.
9. After 10 seconds, the gate expires, `SetCALayerParams` is called, a
   recomposite occurs, and the content appears.

The fix: reduce the dedup gate duration so `SetCALayerParams` is called again
soon after the GUI connects the `CALayerHost`.

### Experiment 8: Reduce the 10-second dedup gate

#### Problem

`RootCompositorFrameSinkImpl::DisplayDidReceiveCALayerParams()` has a 10-second
dedup gate that blocks identical `CALayerParams` from being forwarded for 10
seconds after the last unique update. After navigation produces a new
`ca_context_id`, the first callback fires and the GUI creates a new
`CALayerHost`. But the `SetCALayerParams` call (which triggers a Window Server
recomposite) happened _before_ the `CALayerHost` was connected — the XPC
round-trip to the GUI is asynchronous. The dedup gate then blocks all subsequent
`SetCALayerParams` calls for 10 seconds, preventing the Window Server from
re-compositing the new `CAContext` for the newly-connected `CALayerHost`.

The gate exists to avoid redundant vsync parameter updates. The comment says:

```cpp
// OnDisplayReceivedCALayerParams() is ultimately responsible for triggering
// updates to vsync. VSync may change dynamically. To ensure the value is
// updated correctly, OnDisplayReceivedCALayerParams() is periodically called,
// even if the params haven't changed. The value here matches that of
// DisplayLinkMac, which is responsible for querying for vsync updates.
next_forced_ca_layer_params_update_time_ =
    base::TimeTicks::Now() + base::Seconds(10);
```

10 seconds is far too long. At 100ms, the blank would be imperceptible —
`SetCALayerParams` would be called again within 100ms of the GUI connecting the
`CALayerHost`, triggering the Window Server to recomposite.

#### Changes

**`chromium/src/components/viz/service/frame_sinks/root_compositor_frame_sink_impl.cc`:**

Change the dedup gate duration from 10 seconds to 100ms:

```cpp
next_forced_ca_layer_params_update_time_ =
    base::TimeTicks::Now() + base::Milliseconds(100);
```

Also remove the Experiment 7 verbose logging from all three files (the
diagnostic task is complete):

**`chromium/.../shell_tab_observer.cc`:**

Remove the `LOG(INFO) << "[CALayerParams]"` diagnostic logging from the callback
lambda. Keep the existing `LOG(INFO) << "[ShellTabObserver] Sent ca_context_id"`
log that was there before Experiment 7.

**`chromium/.../browser_compositor_view_mac.mm`:**

Remove the `LOG(INFO) << "[BrowserCompositorMac::DidNavigate]"` line.

**`chromium/.../render_widget_host_view_mac.mm`:**

Remove the `LOG(INFO) << "[AcceleratedWidgetCALayerParamsUpdated]"` block.

#### Verification

Run the app, open a browser overlay at `news.ycombinator.com`, click a link. The
new page should appear within ~200ms — no visible blank gap. Test multiple
navigations in sequence. Test cross-site navigation (e.g., a link from HN to an
external site).

#### Results

**Fail.** The overlay still vanishes for ~10 seconds on navigation. Reducing the
dedup gate from 10 seconds to 100ms had no effect.

#### Conclusion

The 10-second dedup gate in `RootCompositorFrameSinkImpl` is not the cause. The
correlation between the 10-second gate duration and the 10-second blank was
coincidental. The `SetCALayerParams` call frequency does not control when the
CALayerHost's content becomes visible.

The blank persists for ~10 seconds regardless of how frequently
`AcceleratedWidgetCALayerParamsUpdated` fires. The root cause is elsewhere —
possibly in the Window Server's handling of cross-process CAContext/CALayerHost
connections, or in how the hidden NSWindow's layer tree interacts with the
compositor output.

## Conclusion

Issue 628 is **unresolved**. Eight experiments failed to fix the ~10-second
blank that appears when clicking a link. None of the code changes had any
observable effect on the problem. All changes from this issue should be
reverted.

### What we learned

The overlay vanishes for ~10 seconds when clicking a link. Diagnostic logging
(Experiment 7) confirmed that Chromium sends the new `ca_context_id` within
100ms of the click and the page loads in ~70ms. The GUI receives the ID and
replaces the `CALayerHost` immediately. Yet the `CALayerHost` shows nothing for
~10 seconds.

The problem is not in the callback lifecycle, not in the compositor surface
fallback mechanism, and not in the dedup gate timing. All eight experiments
targeted the Chromium-side pipeline, and all failed. The root cause is likely
outside Chromium — in the Window Server's handling of cross-process
CAContext/CALayerHost connections.

### Experiment summary

| Exp | Approach                                               | Result                       |
| --- | ------------------------------------------------------ | ---------------------------- |
| 1   | Re-register callback on view swap, replace CALayerHost | Partial (no effect on blank) |
| 2   | Re-apply size in `RenderViewHostChanged`               | Fail                         |
| 3   | Research: Electron/Chromium sizing                     | Pass (research only)         |
| 4   | Resize NSWindow instead of `view->SetSize()`           | Pass (no effect on blank)    |
| 5   | Research: navigation transitions, dedup gate           | Pass (research only)         |
| 6   | Set fallback surface before navigation                 | Fail                         |
| 7   | Diagnostic logging                                     | Pass (research only)         |
| 8   | Reduce dedup gate to 100ms                             | Fail                         |

### Ideas for the next issue

The problem is likely in the **Window Server's handling of cross-process
CAContext/CALayerHost connections** when the CAContext belongs to a hidden
window. Possible directions:

1. **Test with a visible window.** Does the blank disappear if the Chromium
   Profile Server's NSWindow is visible (remove `[window orderOut:nil]`)? If so,
   the hidden window is interfering with CAContext compositing. The Window
   Server may deprioritize or defer compositing for off-screen windows.

2. **Keep the old CALayerHost alive longer.** Currently the GUI destroys the old
   `CALayerHost` immediately when the new `ca_context_id` arrives. If the old
   `CALayerHost` were kept alive (behind the new one) until the new one has
   visible content, the transition might appear smoother. This is a GUI-side
   change.

3. **Force a Window Server recomposite.** After creating the new `CALayerHost`,
   explicitly trigger a display update — e.g., `setNeedsDisplay` on the layer,
   or toggling a property. The Window Server may not know it needs to fetch
   content from the new remote CAContext.

4. **Use `CATransaction` to batch the swap.** Chromium's
   `DisplayCALayerTree::GotCALayerFrame()` adds the new `CALayerHost` before
   removing the old one. Wrapping the swap in a `CATransaction` might make it
   atomic from the Window Server's perspective.

5. **Investigate whether the 10-second delay is actually a macOS CARemoteLayer
   timeout.** The delay is suspiciously consistent. macOS may have an internal
   timeout for establishing cross-process CAContext connections. Research
   `CARemoteLayerServer` / `CARemoteLayerClient` behavior.

### Code changes (all should be reverted)

None of the code changes from this issue had any effect on the blank. All should
be reverted.

#### Chromium (`chromium/src/`, branch `146.0.7650.0-issue-628`)

5 commits on top of `146.0.7650.0-issue-627`:

| Commit    | Experiment | Files                                                           |
| --------- | ---------- | --------------------------------------------------------------- |
| `25fab61` | Exp 1      | `shell_tab_observer.cc/h`, `shell_browser_main_parts.cc`        |
| `f056024` | Exp 2      | `shell_tab_observer.cc/h`, `shell_browser_main_parts.cc`        |
| `66d2b51` | Exp 4      | `shell_platform_delegate_mac.mm`, `shell_browser_main_parts.cc` |
| `7b14b21` | Exp 6      | `browser_compositor_view_mac.mm`                                |
| `a3947ae` | Exp 8      | `root_compositor_frame_sink_impl.cc`                            |

**To revert:** Start the next issue's Chromium branch from
`146.0.7650.0-issue-627`, discarding this branch entirely.

#### GUI (`gui/`, main repo)

| Commit    | Experiment | Files                        |
| --------- | ---------- | ---------------------------- |
| `a73f3e1` | Exp 1      | `gui/src/renderer/Metal.zig` |

This commit changed `Metal.zig` to replace the `CALayerHost` (destroy old,
create new) when the `ca_context_id` changes.

**To revert:** `git revert a73f3e1`
