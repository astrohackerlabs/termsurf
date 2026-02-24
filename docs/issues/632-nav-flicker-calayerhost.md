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

## Experiment 2: Delay ca_context_id until content is composited

### Hypothesis

The flash occurs because the GUI swaps to a new CALayerHost whose CAContext
hasn't produced visible content yet. The XPC `ca_context` message is sent on the
**first** CALayerParams callback with the new ID — but that first callback may
arrive before the renderer has painted real web content into the new compositor.
If we delay sending the new `ca_context_id` by several frames, the new CAContext
will have real content when the GUI finally swaps to it.

### Background

The `ca_context_id` is sent over XPC from a `CALayerParams` callback registered
on the `RenderWidgetHostViewMac`. The callback is in
`shell_browser_main_parts.cc` lines 399–431 (inside `CreateTab()`):

```cpp
auto ca_layer_callback = base::BindRepeating(
    [](const std::string& pane_id, xpc_connection_t conn,
       uint32_t* last_id, const gfx::CALayerParams& params) {
      if (params.ca_context_id == 0 || params.ca_context_id == *last_id)
        return;
      *last_id = params.ca_context_id;
      // ... send XPC message immediately ...
    },
    cb_pane_id, cb_conn,
    base::Owned(last_id));
```

When a new `ca_context_id` appears (different from `*last_id`), the XPC message
is sent immediately on that same callback invocation. The GPU process has
committed a frame to the CAContext (the callback only fires when
`CALayerParams.is_empty == false`), but the committed content may be a blank
compositor fallback — not the rendered web page. The renderer needs several more
frames to paint real content.

### Design

**Change:** In the `ca_layer_callback` lambda, when a new `ca_context_id` is
detected, don't send the XPC message immediately. Instead, start a frame
counter. Continue counting callbacks with the same `ca_context_id`. After N
callbacks (N = 3 to start, tunable), send the XPC message.

During the delay:

- The GUI still has the **old** CALayerHost pointing at the old (dead)
  CAContext.
- The old host shows nothing (confirmed by Experiment 1).
- The delay means the flash of blank content is still visible for the same
  duration. But the swap itself — when it happens — should be clean: the new
  CAContext will have N frames of real content by then.

**Wait — this doesn't help.** If the old CAContext is dead and shows nothing
during the delay, then delaying the send just makes the blank period _longer_,
not shorter. The flash still happens. The only way this approach works is if the
blank flash is caused by the GUI swapping to a CAContext with no content — not
by the old CAContext dying.

**Revised hypothesis:** The flash might be caused by BOTH:

1. The old CAContext dying (unavoidable), AND
2. The new CAContext not having content when swapped to

If #2 is a factor, delaying the send would eliminate the second blank — reducing
the total flash duration. If #1 is the sole cause and #2 contributes nothing,
then delaying the send won't help.

**This is worth testing because we don't know the relative contribution of each
factor.** The test will tell us.

### Chromium branch

`146.0.7650.0-issue-632` (forked from `146.0.7650.0-issue-631`)

### Code changes

**`shell_browser_main_parts.cc` — modify the `ca_layer_callback` lambda:**

Add two new shared variables alongside `last_id`:

```cpp
auto* last_id = new uint32_t(0);
auto* pending_id = new uint32_t(0);    // New: ID waiting to be sent
auto* pending_count = new int(0);       // New: frames seen with pending ID
const int kFrameDelay = 3;              // New: frames to wait before sending
```

Replace the lambda body:

```cpp
[](const std::string& pane_id, xpc_connection_t conn,
   uint32_t* last_id, uint32_t* pending_id, int* pending_count,
   int frame_delay, const gfx::CALayerParams& params) {
  if (params.ca_context_id == 0)
    return;

  // New ID detected — start counting frames.
  if (params.ca_context_id != *last_id &&
      params.ca_context_id != *pending_id) {
    *pending_id = params.ca_context_id;
    *pending_count = 0;
  }

  // Increment count if we're waiting on a pending ID.
  if (*pending_id != 0 && params.ca_context_id == *pending_id) {
    (*pending_count)++;
    if (*pending_count >= frame_delay) {
      // Enough frames have been composited — send the ID.
      *last_id = *pending_id;
      *pending_id = 0;
      *pending_count = 0;

      xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
      xpc_dictionary_set_string(msg, "action", "ca_context");
      xpc_dictionary_set_uint64(msg, "ca_context_id",
                                params.ca_context_id);
      xpc_dictionary_set_string(msg, "pane_id", pane_id.c_str());
      xpc_dictionary_set_uint64(msg, "pixel_width",
                                params.pixel_size.width());
      xpc_dictionary_set_uint64(msg, "pixel_height",
                                params.pixel_size.height());
      xpc_connection_send_message(conn, msg);
      xpc_release(msg);
    }
  }
}
```

Update the `base::BindRepeating` to pass the new variables:

```cpp
auto ca_layer_callback = base::BindRepeating(
    [](/* ... */),
    cb_pane_id, cb_conn,
    base::Owned(last_id), base::Owned(pending_id),
    base::Owned(pending_count), kFrameDelay);
```

The `ShellTabObserver::DidFinishNavigation` dedup gate reset
(`*last_ca_context_id_ = 0`) remains unchanged — it still resets `last_id`. The
`pending_id` and `pending_count` handle the delay logic independently.

### Test

1. Create Chromium branch: `146.0.7650.0-issue-632` from
   `146.0.7650.0-issue-631`
2. Apply the code change to `shell_browser_main_parts.cc`
3. Build: `autoninja -C out/Default chromium_profile_server`
4. Build GUI: `cd gui && zig build`
5. Launch: `open gui/zig-out/TermSurf.app`
6. Navigate between pages — observe flash behavior
7. If flash is reduced, try tuning `kFrameDelay` (1, 2, 3, 5)
8. If flash is unchanged, the delay doesn't help

### Success criteria

Navigation between pages has no visible flash, or the flash is significantly
reduced compared to the current behavior.

### Failure modes

- **Flash unchanged:** The flash is entirely caused by the old CAContext dying
  (factor #1), and the new CAContext already has content when it arrives. The
  delay just makes the blank period longer. In this case, the flash cannot be
  fixed from either the GUI or the Chromium callback — it's inherent to the
  CAContext lifecycle.
- **Flash longer:** The delay adds latency to the swap without reducing the
  blank. Worse UX than before. Revert immediately.
- **Navigation feels sluggish:** The N-frame delay adds perceived latency to
  page transitions. May need to tune N down or abandon this approach.

### If this fails

Fall back to **Experiment 3: snapshot fallback** (GUI-side). Before swapping the
CALayerHost, capture the current positioning layer's visible content as a bitmap
via `CALayer.renderInContext:` or the `contents` property. Place the bitmap on a
static `CALayer` that covers the gap while the new CAContext produces its first
frame. This approach doesn't prevent the blank — it masks it with a static image
of the previous page, which is visually seamless.
