# Issue 632: Navigation Flicker — CALayerHost Swap Artifact

## Goal

Eliminate the brief flicker that occurs on every page navigation. The browser
overlay should transition from old page to new page with no visible blank frame.

## Background

### CALayerHost issue history

This is the eighth issue in the CALayerHost series:

- [Issue 625](625-calayerhost.md) — **CALayerHost migration.** Replaced
  `FrameSinkVideoCapturer` with `CALayerHost`. Chromium sends a `ca_context_id`
  once per tab; the GUI creates a `CALayerHost` sublayer and Window Server
  composites the remote content directly from GPU VRAM.

- [Issue 626](626-x-y-calayerhost.md) — **X/Y positioning.** Fixed ~10px Y and
  ~3px X offset by adding a positioning layer inside a geometry-flipped layer.

- [Issue 627](627-resize-calayerhost.md) — **Resize.** Fixed overlay resize by
  propagating resize events through XPC to the Chromium capturer.

- [Issue 628](628-navigation-calayerhost.md) — **Navigation (first attempt).**
  Eight experiments, all failed. Key finding: the new `ca_context_id` arrives
  quickly but the new host shows nothing for ~10 seconds.

- [Issue 629](629-understand-nav-calayerhost.md) — **Navigation (diagnosis).**
  Five research experiments. Produced the primary hypothesis and confirmed two
  latent bugs.

- [Issue 630](630-nav-calayerhost-6.md) — **Navigation (fix).** Seven
  coordinated fixes across GUI and Chromium resolved the permanent overlay
  disappearance. A brief flicker remained on every navigation.

- [Issue 631](631-continue-nav-calayerhost.md) — **Navigation flicker
  (investigation).** Five experiments attempting to eliminate the flicker. All
  failed, but produced critical understanding of the problem.

### What Issue 631 established

Issue 631 ran five experiments:

1. **Code smell audit** (Experiment 1) — identified 15 potential causes, 11
   confirmed. Primary suspect: unnecessary CALayerHost swap on same ID.

2. **Skip swap when ID unchanged** (Experiment 2) — the `ca_context_id` changes
   on every navigation (confirmed via Chromium server logs), so the skip never
   triggered.

3. **Delay old host removal** (Experiment 3) — the old CAContext's content is
   already destroyed by Chromium when navigation creates a new one. Keeping the
   old host around just keeps a pointer to a dead context.

4. **Research Chromium/Electron** (Experiment 4) — discovered Chromium's
   `DelegatedFrameHost` fallback surface mechanism and Electron's compositor
   recycling patch. Found that `CALayerTreeCoordinator` owns the `CAContext` and
   is recreated per compositor.

5. **Disable compositor recycling** (Experiment 5) — applied Electron's patch.
   Caused white screen on back navigation. The `ca_context_id` changes because
   of renderer/RenderViewHost swaps, not occlusion-triggered compositor
   recycling.

### The critical realization

The flicker was initially estimated at ~100ms. It is actually much shorter —
visually it appears to be roughly one frame (~16ms), though this has not been
precisely measured. This changes the entire analysis.

If the gap were 100ms, it would mean the new CAContext genuinely has no content
for a significant period. But a very brief gap is likely **the inherent cost of
the CALayerHost swap itself**, not a content gap. The new CAContext probably
already has content when we swap — Window Server just needs at least one vsync
cycle to composite the new host's layer tree after the CATransaction commits.

### Current swap mechanics

The swap in `Metal.zig`'s `setCALayerHostContextId()` is an atomic operation
inside a single `CATransaction`:

1. `CATransaction.begin()` + `setDisableActions:YES`
2. Create new `CALayerHost` with new `contextId`
3. `addSublayer:` new host to positioning layer
4. `removeFromSuperlayer` on old host
5. `CATransaction.commit()`

Window Server processes the entire transaction atomically at the next vsync: old
host removed + new host added = at least one frame where the new host hasn't
been composited yet. The result is a brief blank flash (visually estimated at ~1
frame, not precisely measured).

### Why previous experiments missed this

- **Experiment 2** (skip swap): The ID changes every navigation, so the swap is
  unavoidable. But the swap itself is the problem — not the ID change.
- **Experiment 3** (delay removal): Kept the old host, but the old CAContext was
  dead. The right idea (keep something visible during the swap) but the wrong
  mechanism (the old host has no content to show).
- **Experiment 5** (prevent recycling): Addressed the wrong cause. The ID
  changes because of renderer swaps, not compositor recycling. But the flicker
  would exist even if the ID stayed the same — it's a swap artifact, not a
  content gap.

## Possible approaches

### Two-phase swap

Split the atomic swap into two CATransactions across two frames:

1. **Frame N**: Add the new CALayerHost on top of the old one. Both hosts are in
   the layer tree. Commit. Window Server composites the new host for the first
   time. The old host is still visible underneath (even if its content is dead —
   it doesn't matter because the new host is on top).
2. **Frame N+1**: Remove the old CALayerHost. Commit. By now, the new host has
   been composited for one full frame and is visible.

This requires a short delay between add and remove — at least one frame.
Implementable via `dispatch_after_f` with a delay of ~16ms, or by deferring the
removal to the next `drawFrame()` call. The exact delay needed is unknown until
tested.

### Pre-warm the CALayerHost

Create the new CALayerHost and add it to the layer tree as a hidden sublayer
(e.g., with `opacity: 0` or outside the visible bounds) before the actual swap.
Window Server starts compositing it immediately. When the `ca_context_id`
arrives, move the pre-warmed host into position and remove the old one. The
pre-warmed host has already been composited, so no blank frame.

Challenge: we don't know the new `ca_context_id` until the XPC message arrives.
We would need to create the host, add it to the tree, then set its `contextId` —
and hope that setting the property triggers Window Server to start compositing
before the next vsync.

### Crossfade via opacity

Instead of an instant swap, briefly overlap both hosts:

1. Add new host with `opacity: 0`
2. Animate old host opacity to 0 and new host opacity to 1 over 1-2 frames
3. Remove old host

Even with `setDisableActions:YES`, we could manually set opacity values across
multiple frames. This turns the blank flash into a crossfade.

### Accept and mask

If the brief blank is truly unavoidable with CALayerHost, mask it:

- Set the positioning layer's `backgroundColor` to white (or the page's
  background color). During the one-frame gap, the user sees white instead of
  the terminal background, which is far less jarring.
- Or set it to the previous page's dominant color, extracted before navigation.

## Experiment 1: Two-phase swap via deferred removal

### Hypothesis

The flicker occurs because the old CALayerHost is removed in the same
`CATransaction` as the new one is added. Window Server processes the transaction
atomically at the next vsync, but the new host hasn't been composited yet — so
there's a brief blank. If we leave the old host in the layer tree (underneath
the new one) and defer its removal, the old host covers the gap while Window
Server composites the new one.

### Design

**Change:** Split the atomic swap in `Metal.zig`'s `setCALayerHostContextId()`
into two phases:

1. **Phase 1 (immediate):** Create the new `CALayerHost`, add it to the
   positioning layer. Do NOT remove the old host. Store the old host pointer in
   a new field `ca_layer_host_pending_removal: ?*anyopaque` on the renderer
   struct. Commit the `CATransaction`.

2. **Phase 2 (deferred to next `drawFrame`):** At the start of `drawFrame()`,
   check if `ca_layer_host_pending_removal` is non-null. If so, remove it from
   the superlayer, release it, and set the field to null. This runs inside the
   `draw_mutex` lock that `drawFrame` already holds.

**Why `drawFrame` and not `dispatch_after_f`:** Both `setCALayerHostContextId`
and `drawFrame` run under `draw_mutex`, so the pending removal field is
thread-safe without additional synchronization. Using `drawFrame` also
guarantees the removal happens after at least one render pass, not after an
arbitrary timer that might fire too early or too late.

### Code changes

**`generic.zig` — add field:**

```zig
/// Old CALayerHost pending removal (Issue 632 Experiment 1).
/// Set during two-phase swap; cleared at next drawFrame.
ca_layer_host_pending_removal: ?*anyopaque = null,
```

**`Metal.zig` — modify the existing-host branch of
`setCALayerHostContextId()`:**

Replace lines 220–223 (the immediate removal):

```zig
// Now remove old host.
const old_host = objc.Object.fromId(existing);
old_host.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
old_host.release();
```

With deferred removal:

```zig
// Defer old host removal to next drawFrame (Issue 632 Experiment 1).
// The old host stays in the layer tree underneath the new one,
// covering the gap while Window Server composites the new host.
ca_layer_host_pending_removal.* = existing;
```

And update the function signature to accept the new pointer:

```zig
pub fn setCALayerHostContextId(
    self: *Metal,
    context_id: u32,
    ca_layer_host_ptr: *?*anyopaque,
    ca_layer_flipped_ptr: *?*anyopaque,
    ca_layer_positioning_ptr: *?*anyopaque,
    ca_layer_host_pending_removal: *?*anyopaque,
) void {
```

**`generic.zig` — update the wrapper to pass the new field:**

```zig
pub fn setCALayerHostContextId(self: *Self, context_id: u32) void {
    if (comptime @hasDecl(GraphicsAPI, "setCALayerHostContextId")) {
        self.api.setCALayerHostContextId(
            context_id,
            &self.ca_layer_host,
            &self.ca_layer_flipped,
            &self.ca_layer_positioning,
            &self.ca_layer_host_pending_removal,
        );
    }
}
```

**`generic.zig` — add deferred removal in `drawFrame()`:**

Insert near the top of `drawFrame()`, after `draw_mutex` is acquired (after line
1468):

```zig
// Phase 2 of two-phase CALayerHost swap (Issue 632 Experiment 1).
// Remove the old host that was left in the layer tree during the swap.
if (self.ca_layer_host_pending_removal) |old_ptr| {
    const CATx = objc.getClass("CATransaction");
    if (CATx) |tx| {
        tx.msgSend(void, objc.sel("begin"), .{});
        tx.msgSend(void, objc.sel("setDisableActions:"), .{true});
        const old_host = objc.Object.fromId(old_ptr);
        old_host.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
        old_host.release();
        tx.msgSend(void, objc.sel("commit"), .{});
    }
    self.ca_layer_host_pending_removal = null;
}
```

**`generic.zig` — update `removeCALayerHost` to clean up pending removal:**

Add to the existing `removeCALayerHost()` function:

```zig
// Also clean up any pending removal (Issue 632).
if (self.ca_layer_host_pending_removal) |old_ptr| {
    const old_host_obj = objc.Object.fromId(old_ptr);
    old_host_obj.msgSend(void, objc.sel("removeFromSuperlayer"), .{});
    old_host_obj.release();
    self.ca_layer_host_pending_removal = null;
}
```

### Test

1. Build: `cd gui && zig build`
2. Launch: `open gui/zig-out/TermSurf.app`
3. Open `web` TUI, navigate to any page
4. Click a link — observe whether the flicker is gone, reduced, or unchanged
5. Click browser back/forward — same observation
6. Try multiple rapid navigations in sequence

### Success criteria

Navigation between pages has no visible blank flash. The old page remains
visible until the new page appears.

### Failure modes

- **Flicker unchanged:** The gap is not caused by the `removeFromSuperlayer`
  timing. The new host genuinely takes longer than one `drawFrame` cycle to be
  composited. Would need to increase the delay or try a different approach.
- **Stale frame visible:** The old host's dead CAContext shows corruption or a
  stale frame briefly. Visually worse than a blank. Would need the opacity
  crossfade approach instead.
- **Memory leak:** If `drawFrame` never runs after the swap (e.g., window is
  occluded), the old host is never removed. Mitigated by the cleanup in
  `removeCALayerHost()`, but worth verifying.

### Result: FAIL

The flicker is still visible. Deferring old host removal to the next `drawFrame`
did not help — the flash of content persists on every navigation.

### Analysis

The hypothesis was wrong. The flicker is not caused by removing the old host in
the same transaction as adding the new one. Even with the old host kept in the
layer tree underneath the new one, the flash still occurs. This means:

1. **The old host is not covering the gap.** Either its dead CAContext renders
   as transparent (not as the last frame), or Window Server doesn't composite a
   host whose CAContext has been destroyed — it just shows nothing regardless of
   whether the host is in the layer tree.

2. **The new host genuinely has no content for at least one frame.** The flash
   is the new CAContext before Chromium's GPU process has composited its first
   frame into it. No amount of layer tree manipulation on the GUI side can fix
   this — the content simply doesn't exist yet.

3. **One `drawFrame` cycle may not be enough.** The delay between add and
   removal was tied to `drawFrame` (~16ms at 60Hz), but if the new CAContext
   needs multiple frames to produce content, a longer delay would be needed.
   However, since the old host isn't providing cover anyway (point 1), a longer
   delay wouldn't help.

### Next steps

The two-phase swap approach is fundamentally limited because dead CAContexts
don't render anything useful. Future experiments should explore:

- **Snapshot the old content.** Before the swap, capture the current positioning
  layer's contents as a bitmap (via `renderInContext:` or `contents` property)
  and set it as the positioning layer's `contents`. This static snapshot covers
  the gap regardless of the old CAContext's state. Remove it once the new host
  is composited.

- **Chromium-side: reuse the CAContext.** Instead of creating a new CAContext on
  navigation, have Chromium update the existing one. If the `ca_context_id`
  doesn't change, no swap is needed on the GUI side. This requires Chromium
  changes — the ID changes because navigation triggers renderer/compositor
  recreation.

- **Chromium-side: pre-composite before sending ID.** Delay sending the new
  `ca_context_id` until the new CAContext has at least one composited frame. The
  old page stays visible (via the old host) until the new one is ready. This
  requires a way to detect "first frame composited" on the Chromium side.

- **Accept and mask.** Set the positioning layer's `backgroundColor` to white so
  the flash shows white instead of the terminal background. Doesn't eliminate
  the flash but makes it far less jarring.

## Experiment 2: Research how Chromium handles CAContext transitions

### Question

Chromium must display content via CALayerHost internally. When the
`ca_context_id` changes (e.g., GPU process crash, cross-process navigation), how
does Chromium avoid a visible flicker?

### Method

Searched the Chromium source at `chromium/src/` for the browser-side CALayerHost
management, compositor lifecycle, and navigation transition logic.

### Key files examined

- `ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm` — GPU-side CAContext
  owner
- `ui/accelerated_widget_mac/display_ca_layer_tree.mm` — Browser-side
  CALayerHost manager
- `content/app_shim_remote_cocoa/render_widget_host_ns_view_bridge.mm` — NSView
  layer tree setup
- `content/browser/renderer_host/browser_compositor_view_mac.mm` —
  BrowserCompositorMac
- `content/browser/renderer_host/render_widget_host_view_mac.mm` —
  RenderWidgetHostViewMac
- `content/browser/renderer_host/delegated_frame_host.cc` — Surface fallback
  system

### Finding 1: The CAContext does NOT change during normal navigation

In normal Chromium, the `CALayerTreeCoordinator` creates **one** `CAContext` in
its constructor and keeps it for the entire lifetime of the output surface:

```cpp
// ca_layer_tree_coordinator.mm, constructor
CGSConnectionID connection_id = CGSMainConnectionID();
ca_context_ = [CAContext contextWithCGSConnection:connection_id options:@{}];
ca_context_.layer = root_ca_layer_;
```

The `ca_context_id` is read from this permanent object every frame:

```cpp
// ca_layer_tree_coordinator.mm, line 211
params.ca_context_id = [ca_context_ contextId];
```

Content changes happen by swapping **sublayers** of `root_ca_layer_` via
`CommitScheduledCALayers` — the CAContext itself persists. The browser-side
`DisplayCALayerTree::GotCALayerFrame()` has an early-out when the ID hasn't
changed:

```cpp
// display_ca_layer_tree.mm
void DisplayCALayerTree::GotCALayerFrame(uint32_t ca_context_id) {
  if (remote_layer_.contextId == ca_context_id)
    return;  // Common case — no swap needed
  // ...
}
```

**This is the fundamental difference from TermSurf.** In our profile server, the
`CALayerTreeCoordinator` is destroyed and recreated on every navigation,
creating a new `CAContext` each time. Normal Chromium doesn't do this.

### Finding 2: Chromium's layer tree has a background color layer

The NSView layer tree is structured as:

```
NSView.layer
  └── background_layer_ (solid color, set by SetBackgroundColor)
        └── maybe_flipped_layer_ (from DisplayCALayerTree)
              └── remote_layer_ (CALayerHost, displays GPU content)
```

The `background_layer_` acts as a fallback behind the remote layer. Its color is
set **only after** a frame swap completes — not before:

```cpp
// render_widget_host_view_mac.mm
void RenderWidgetHostViewMac::AcceleratedWidgetCALayerParamsUpdated() {
  // Note: background is set only AFTER the swap has completed,
  // so that the background is not set before the frame is up.
  SetBackgroundLayerColor(last_frame_root_background_color_);
  // ...
}
```

### Finding 3: Transparent root layer prevents flash during navigation

The compositor's root layer is explicitly transparent:

```cpp
// browser_compositor_view_mac.mm, lines 59-62
root_layer_ = std::make_unique<ui::Layer>(ui::LAYER_SOLID_COLOR);
// Ensure that this layer draws nothing when it does not have delegated
// content (otherwise this solid color will be flashed during navigation).
root_layer_->SetColor(SK_ColorTRANSPARENT);
```

The comment is telling — Chromium explicitly prevents a solid color from
flashing during the gap between old content being evicted and new content
arriving.

### Finding 4: Cross-process navigation uses TakeFallbackContentFrom

During cross-process navigations (site isolation, process swap), the new
`RenderWidgetHostViewMac` **copies the old view's CALayerParams** so it
immediately displays the same content:

```cpp
// render_widget_host_view_mac.mm
void RenderWidgetHostViewMac::TakeFallbackContentFrom(
    RenderWidgetHostView* view) {
  RenderWidgetHostViewMac* view_mac =
      static_cast<RenderWidgetHostViewMac*>(view);
  ScopedCAActionDisabler disabler;
  // Copy background color from old view.
  std::optional<SkColor> color = view_mac->GetBackgroundColor();
  if (color)
    SetBackgroundColor(*color);
  // Make this view's NSView display the same content as the old view.
  const gfx::CALayerParams* ca_layer_params =
      view_mac->browser_compositor_->GetLastCALayerParams();
  if (ca_layer_params)
    ns_view_->SetCALayerParams(*ca_layer_params);
  browser_compositor_->TakeFallbackContentFrom(
      view_mac->browser_compositor_.get());
}
```

The new view's CALayerHost is set to the **same** `ca_context_id` as the old
view's — both CALayerHosts briefly point to the same CAContext. Window Server
handles this seamlessly. The old CAContext isn't destroyed until the old view is
torn down, by which time the new view already has its own content.

### Finding 5: DelegatedFrameHost maintains surface fallbacks

The `DelegatedFrameHost` has a sophisticated fallback system:

- `pre_navigation_local_surface_id_` — cached before navigation so old content
  can be restored if navigation fails
- `first_local_surface_id_after_navigation_` — first valid surface after
  navigation
- `stale_content_layer_` — a **texture copy** of old content, used when surfaces
  are evicted

### Finding 6: Add-before-remove for the rare ID change case

When the `ca_context_id` does change (GPU process crash/restart), the swap in
`DisplayCALayerTree::GotCALayerFrame()` uses add-before-remove:

```cpp
// display_ca_layer_tree.mm
[maybe_flipped_layer_ addSublayer:new_remote_layer];  // New first
[remote_layer_ removeFromSuperlayer];                   // Old second
remote_layer_ = new_remote_layer;
```

All mutations are wrapped in `ScopedCAActionDisabler` (disables implicit CALayer
animations).

### Finding 7: ScopedCAActionDisabler for atomic updates

Every CALayer mutation in Chromium uses `ScopedCAActionDisabler`, which wraps
`[CATransaction begin]` with `kCATransactionDisableActions`. All changes within
scope are committed atomically. No fade-in/fade-out animations.

### Conclusion

Chromium avoids the flicker because **the CAContext persists across
navigations**. The `ca_context_id` simply doesn't change in the normal case.
Content updates happen within the existing CAContext's sublayer tree.

For TermSurf, the `ca_context_id` changes on every navigation because the
profile server's `CALayerTreeCoordinator` is destroyed and recreated. The root
cause is not the GUI's swap logic — it's the server destroying the CAContext.

The most promising path forward is to investigate **why** the profile server
recreates the `CALayerTreeCoordinator` on navigation and whether it can be made
to persist, matching normal Chromium's behavior. If the `ca_context_id` stops
changing, the flicker disappears entirely — no GUI-side workaround needed.
