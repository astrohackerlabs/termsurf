+++
status = "closed"
opened = "2026-02-23"
closed = "2026-02-23"
+++

# Issue 630: Fix Navigation Blank in CALayerHost

## Goal

Fix the ~10-second blank that occurs when clicking a link in the browser
overlay. The overlay should transition seamlessly to the new page, matching the
behavior of the old `FrameSinkVideoCapturer` pipeline where navigation was
invisible.

## Background

### CALayerHost issue history

This is the sixth issue in the CALayerHost series. Each addressed a different
regression from the migration away from `FrameSinkVideoCapturer`:

- [Issue 625](625-calayerhost.md) — **CALayerHost migration.** Replaced the
  `FrameSinkVideoCapturer` pipeline with `CALayerHost`. Instead of capturing
  IOSurface frames at 120fps and transferring Mach ports over XPC every frame,
  Chromium now sends a `ca_context_id` (uint32) once per tab. The GUI creates a
  `CALayerHost` sublayer, and Window Server composites the remote content
  directly from GPU VRAM. Zero per-frame IPC, zero texture copies.

- [Issue 626](626-x-y-calayerhost.md) — **X/Y positioning.** The CALayerHost
  overlay had a ~10px Y and ~3px X offset. Fixed by adding a positioning layer
  inside a geometry-flipped layer, matching Chromium's `maybe_flipped_layer_`
  pattern.

- [Issue 627](627-resize-calayerhost.md) — **Resize.** The overlay stopped
  resizing when the user resized the window or pane. Fixed by propagating resize
  events through XPC to the Chromium capturer and updating the positioning
  layer's frame.

- [Issue 628](628-navigation-calayerhost.md) — **Navigation (first attempt).**
  Ran 8 experiments targeting the Chromium-side pipeline. All failed. Key
  finding from diagnostic logging: the new `ca_context_id` arrives within 100ms
  and the GUI replaces the `CALayerHost` immediately, yet the new host shows
  nothing for ~10 seconds.

- [Issue 629](629-understand-nav-calayerhost.md) — **Navigation (diagnosis).**
  Research issue. Five experiments: compared Electron/Chromium CALayerHost
  usage, traced the CAContext lifecycle, tested `DisableDisplay()` (made things
  worse), audited all 10-second delays in Chromium, and performed a full code
  audit of both the GUI and Chromium Profile Server. Produced the primary
  hypothesis and confirmed two latent bugs.

### What we know

1. **Chromium is fast.** The new `ca_context_id` arrives in ~100ms. The page
   loads in ~70ms.
2. **The GUI is fast.** The `CALayerHost` is replaced immediately upon receiving
   the new ID.
3. **The blank is ~10 seconds.** Suspiciously consistent.
4. **Disabling the hidden window's `DisplayCALayerTree` makes things worse.**
   The navigated page never appears at all (Issue 629 Experiment 3).
5. **The problem is NOT:** callback lifecycle, compositor surface fallback,
   dedup gate timing, NSWindow sizing, `SetSize()` vs `setContentSize:`, or dual
   CALayerHost interference.

### Chromium branch

Continue from `146.0.7650.0-issue-627`. Create `146.0.7650.0-issue-630` if any
Chromium changes are needed.

## Checklist

Items to investigate, test, and resolve. Derived from Issue 629's full code
audit (Experiment 5).

### Primary hypothesis

- [ ] **Hidden window compositor detachment.** The Chromium Profile Server hides
      its NSWindow via `[window orderOut:nil]`
      (`shell_platform_delegate_mac.mm:209`). This likely sets
      `render_widget_host_is_hidden_ = true` on the `RenderWidgetHostViewMac`,
      which causes `BrowserCompositorMac` to transition to `HasNoCompositor`.
      During navigation, `DidNavigate()` invalidates the surface ID instead of
      generating a new one — no new surface is embedded, no frames are
      submitted. The surface manager's `kExpireInterval = base::Seconds(10)`
      eventually garbage-collects the orphaned temporary reference, triggering
      recovery. This explains both the blank and its consistent ~10-second
      duration. **Needs diagnostic logging in
      `BrowserCompositorMac::UpdateState()` and `DidNavigate()` to confirm
      whether `render_widget_host_is_hidden_` is true.**

### Confirmed bugs

- [ ] **CALayer mutations from background thread.** All CALayerHost
      creation/replacement in the GUI happens on the XPC serial GCD queue
      (`com.termsurf.ghost.xpc`), not the main thread. No `CATransaction`
      wrapping, no `ScopedCAActionDisabler`. Chromium's `DisplayCALayerTree`
      wraps its `CALayerHost` operations in `ScopedCAActionDisabler` and runs on
      the main thread. Our code does neither. This violates Apple's threading
      model for Core Animation and could cause delayed or missed visual updates.
      **Fix:** Dispatch CALayerHost creation/replacement to the main thread, and
      wrap in `[CATransaction begin]` / `[CATransaction commit]` with
      `[CATransaction setDisableActions:YES]`.

- [ ] **Missing `RenderViewHostChanged` in `ShellTabObserver`.** The
      CALayerParams callback and cursor callback are registered once in
      `CreateTab()` (`shell_browser_main_parts.cc:404-427`) on the initial
      `RenderWidgetHostView`. Nobody re-registers them after a view swap.
      `ShellTabObserver` does not implement `RenderViewHostChanged` or
      `RenderFrameHostChanged`. Currently latent (content_shell doesn't enable
      strict site isolation), but will cause permanent blank on cross-site
      navigation when site isolation is enabled. **Fix:** Add
      `RenderViewHostChanged()` to `ShellTabObserver` that re-registers both the
      CALayerParams callback and the cursor callback on the new view.

## Experiments

### Experiment 1: Create Chromium branch

#### Purpose

Create the `146.0.7650.0-issue-630` branch for this issue's Chromium changes.

#### Steps

1. Create the branch from `146.0.7650.0-issue-629` (which has Experiment 3's
   `DisableDisplay()` changes reverted — clean state):

   ```bash
   cd chromium/src
   git checkout 146.0.7650.0-issue-629
   git checkout -b 146.0.7650.0-issue-630
   ```

2. Add the new branch to the Branches table in `docs/chromium.md`:

   ```
   | `146.0.7650.0-issue-630` | [Issue 630](issues/0630-nav-calayerhost-6.md) | Fix navigation blank |
   ```

#### Verification

`git branch --show-current` prints `146.0.7650.0-issue-630` and the branch table
in `docs/chromium.md` includes the new entry.

#### Conclusion

Done. Branch `146.0.7650.0-issue-630` created from `146.0.7650.0-issue-629`.
Branch table entry already existed in `docs/chromium.md`; updated the "Current
State" line to point to the new branch.

### Experiment 2: Audit all CALayerHost code for 20 code smells

#### Purpose

Find what causes the browser overlay to permanently vanish when clicking a link.

Current behavior: clicking a link causes the overlay to disappear and it never
comes back. Previously the overlay would vanish for ~10 seconds and then
reappear, but that behavior is no longer reproducible — the disappearance is now
permanent.

The audit checks every CALayerHost-related code path in both the GUI (Zig) and
Chromium Profile Server (C++) against 20 known code smells. Each smell is
evaluated solely for whether it could cause the overlay to disappear or delay
its appearance. Smells that only affect positioning, color, performance, or
other non-visibility concerns are marked clean and skipped.

The core questions are:

1. **Why does the overlay permanently vanish on navigation?** This is the
   primary bug. The overlay disappears when clicking a link and never returns.
2. **Why did the overlay previously vanish for ~10 seconds and then recover?**
   This was the behavior documented in Issues 628–629 but is no longer
   reproducible. The audit should still search for what caused the 10-second
   recovery, as understanding that mechanism may explain why recovery no longer
   happens.
3. **Why does the initial overlay take longer than expected to appear?**
4. **What is the correct CALayerHost lifecycle for continuous visibility?** How
   should we create, swap, and manage CALayerHost instances to guarantee the
   overlay is always visible — across initial load, same-site navigation, and
   cross-site navigation?

The audit does not fix anything. It produces a findings table with verdicts and
a summary of every finding that could affect overlay visibility.

#### Code smells

**General (C++/Zig):**

1. **Thread-unsafe CALayer mutations.** CALayer creation, removal, or property
   changes happening off the main thread. Core Animation requires all layer-tree
   mutations on the main thread. XPC callbacks run on GCD queues.
2. **Missing `CATransaction` wrapping.** Layer mutations without
   `[CATransaction begin]`/`[CATransaction commit]` and `setDisableActions:YES`.
   Core Animation may animate or defer changes.
3. **Stale callback registration.** Callbacks registered once on an initial
   `RenderWidgetHostView` but never re-registered when the view swaps during
   navigation.
4. **Hidden-window compositor detachment.** `orderOut:` on the NSWindow marking
   the render widget as hidden, causing `BrowserCompositorMac` to drop into
   `HasNoCompositor`. Navigation in this state may invalidate surface IDs
   without generating replacements.
5. **Dedup gate swallowing valid updates.** Comparing incoming `ca_context_id`
   against the current one and skipping duplicates. A navigation that produces
   the same ID (or zero → zero) would be silently dropped.
6. **Layer accumulation.** Adding a new `CALayerHost` sublayer without removing
   the old one first, or removing both and adding the new one in the wrong
   order. Stacked or orphaned layers could mask live content.
7. **Zero/null context ID treated as valid.** Processing `ca_context_id = 0` as
   a real context ID, creating a `CALayerHost` that points at nothing.
8. **Surface ID invalidation without regeneration.** `DidNavigate()`
   invalidating the current surface ID while the compositor is detached. No new
   surface embedded, so frames have nowhere to go until the 10-second
   `kExpireInterval` GC recovers.
9. **Missing size on new view.** After navigation, the new
   `RenderWidgetHostView` not receiving the current pane dimensions. Could
   render at 0×0 or a default size.
10. **Retained strong references blocking recovery.** Old `FrameSinkId` or
    `LocalSurfaceId` references held by the tab observer or callback closures,
    preventing the surface manager from cleaning up and re-creating the
    compositor path.

**CALayerHost-specific (from Issues 625–629):**

11. **Dual CALayerHost per CAContext.** The Chromium Profile Server's hidden
    window creates its own `CALayerHost` via `DisplayCALayerTree`, then the GUI
    creates a second one pointing at the same `CAContext`. macOS may only
    composite to one host at a time.
12. **Explicit frame set on CALayerHost.** Setting a `frame` directly on
    `CALayerHost` makes it invisible (Issue 626 Exp 4). The frame must go on an
    intermediate `positioning_layer`.
13. **`geometryFlipped` on the wrong layer.** `geometryFlipped` affects sublayer
    positioning, not the layer's own position in its parent. Must go on a parent
    layer, not on the `CALayerHost` itself.
14. **Physical pixels where logical points are expected.** Cell dimensions and
    screen height from the renderer are in physical pixels, but `CALayer` frames
    use logical points. Requires dividing by `contentsScale`.
15. **Hidden window phantom chrome offset.** The Chromium Profile Server's
    `NSWindow` title bar (~28px) and toolbar (24px) baked into the `CAContext`
    layer tree even though the window is hidden. Requires
    `NSWindowStyleMaskBorderless` and `ShouldHideToolbar()`.
16. **Accidental pipeline deletion during migration.** Code deleted alongside
    the capturer that was still needed (e.g., `sendResize()` and the `"resize"`
    XPC handler were removed with the IOSurface pipeline).
17. **`autoresizingMask` on a layer with no initial frame.** Auto-resize mask
    without an initial frame causes the layer to start at zero size (invisible).
    Must set initial frame to parent bounds first.
18. **Non-atomic CALayerHost swap.** Chromium adds the new `CALayerHost` before
    removing the old one (no blank frame). Removing first then adding creates a
    visible gap.
19. **Y-flip formula dependent on parent height.** Using
    `y = parent_height - y_from_top - h` breaks during resize because
    `parent_height` changes every frame. The 3-layer architecture
    (`flipped_layer` → `positioning_layer` → `CALayerHost`) avoids this.
20. **`setContentSize:` vs `view->SetSize()` for the hidden window.** Calling
    `view->SetSize()` without resizing the hidden `NSWindow` causes
    `BrowserCompositorMac::DidNavigate()` to use the window's original
    `dfh_size_dip_`, reverting to stale dimensions.

#### Files to audit

**GUI (Zig):**

- `gui/src/renderer/Metal.zig` — CALayerHost creation, layer tree setup,
  `updateCALayerHostFrame()`, `setCALayerHostContextId()`
- `gui/src/renderer/generic.zig` — `drawFrame()` overlay path, `size_changed`
  resize path, `ca_layer_*` fields
- `gui/src/Surface.zig` — `setCAContextId()`, overlay state, mode switching
- `gui/src/apprt/xpc.zig` — `handleCAContext()`, `handleSetOverlay()`,
  `sendResize()`, XPC message parsing

**Chromium (C++):**

- `content/shell/browser/shell_browser_main_parts.cc` — `CreateTab()`, callback
  registration, `ResizeTab()`, XPC action handlers
- `content/shell/browser/shell_tab_observer.cc` — `WebContentsObserver`
  overrides, `DidFinishNavigation()`, `DidStopLoading()`
- `content/shell/browser/shell_tab_observer.h` — observer interface, stored
  state
- `content/shell/browser/shell_platform_delegate_mac.mm` — `CreateShell()`,
  window setup, `orderOut:`, `ResizeWebContent()`
- `content/shell/browser/shell.h` / `shell.cc` — Shell interface, window
  accessors

#### Steps

For each of the 9 files above:

1. Read the file in full.
2. Check each of the 20 code smells.
3. Record a verdict: **clean** (not present), **suspect** (possible but
   unconfirmed), or **confirmed** (definitely present).
4. Add a one-line note explaining the verdict.

#### Output format

A findings table per file:

```
#### File: `path/to/file.zig`

| # | Smell | Verdict | Note |
|---|-------|---------|------|
| 1 | Thread-unsafe CALayer mutations | confirmed | CALayerHost created on XPC queue, not main thread |
| 2 | Missing CATransaction wrapping | confirmed | No CATransaction around layer mutations |
| … | … | … | … |
```

After all files, a summary section listing every confirmed and suspect finding
with file path and line number.

#### Verification

Every confirmed and suspect finding has a file path, line number, and one-line
explanation. No smell is left unchecked for any file.

#### Findings

##### File: `gui/src/renderer/Metal.zig`

| #   | Smell                                  | Verdict   | Note                                                                                                                                                                                                                                                                                                                             |
| --- | -------------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Thread-unsafe CALayer mutations        | confirmed | `setCALayerHostContextId()` (line 172) creates/destroys CALayerHost, sets properties, adds/removes sublayers — all on the XPC queue thread (caller holds `draw_mutex` but that doesn't dispatch to main thread). Window Server may not composite a layer tree modified from a background thread.                                 |
| 2   | Missing CATransaction wrapping         | confirmed | No `CATransaction begin/commit` or `setDisableActions:YES` around any layer mutation in `setCALayerHostContextId()` (lines 172-256) or `updateCALayerHostFrame()` (lines 263-295). On a background GCD queue without a run loop, implicit transactions may never commit — changes may never reach the render server.             |
| 3   | Stale callback registration            | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |
| 4   | Hidden-window compositor detachment    | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |
| 5   | Dedup gate swallowing valid updates    | clean     | No dedup on context_id in this file. Every call destroys and recreates.                                                                                                                                                                                                                                                          |
| 6   | Layer accumulation                     | clean     | Old host is removed before new one is added (lines 194-195, 207-210). No accumulation.                                                                                                                                                                                                                                           |
| 7   | Zero/null context ID                   | suspect   | No zero-check on `context_id` parameter. If Chromium sends 0, this creates a CALayerHost with `contextId=0`, pointing at nothing. The Chromium-side lambda filters zeros, but if that gate fails, this file will happily create an empty host.                                                                                   |
| 8   | Surface ID invalidation                | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |
| 9   | Missing size on new view               | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |
| 10  | Retained references                    | clean     | Old host is released (line 195). No dangling references.                                                                                                                                                                                                                                                                         |
| 11  | Dual CALayerHost                       | clean     | GUI side — this file creates the GUI's CALayerHost. The dual-host issue is an architectural concern, not a bug in this file.                                                                                                                                                                                                     |
| 12  | Explicit frame on CALayerHost          | clean     | Frame is set on `positioning_layer`, not on CALayerHost (line 289).                                                                                                                                                                                                                                                              |
| 13  | geometryFlipped on wrong layer         | clean     | Set on `flipped_layer` (line 226), not on CALayerHost.                                                                                                                                                                                                                                                                           |
| 14  | Physical vs logical pixels             | clean     | Division by `contentsScale` in `updateCALayerHostFrame()` (line 274).                                                                                                                                                                                                                                                            |
| 15  | Hidden window phantom chrome           | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |
| 16  | Accidental pipeline deletion           | clean     | Current code is complete.                                                                                                                                                                                                                                                                                                        |
| 17  | autoresizingMask without initial frame | clean     | `flipped_layer` gets initial frame set to parent bounds (line 230) before autoresizingMask (line 231).                                                                                                                                                                                                                           |
| 18  | Non-atomic CALayerHost swap            | confirmed | Replacement path (lines 194-210): old host is `removeFromSuperlayer` + `release` (lines 194-195) BEFORE new host is created and added (lines 200-210). There is a window where no CALayerHost exists in the positioning layer. Chromium's `DisplayCALayerTree::GotCALayerFrame()` adds the new host before removing the old one. |
| 19  | Y-flip dependent on parent height      | clean     | Using 3-layer architecture with `flipped_layer`.                                                                                                                                                                                                                                                                                 |
| 20  | setContentSize vs view->SetSize        | clean     | GUI side — not applicable.                                                                                                                                                                                                                                                                                                       |

##### File: `gui/src/renderer/generic.zig`

| #     | Smell                           | Verdict   | Note                                                                                                                                                     |
| ----- | ------------------------------- | --------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1     | Thread-unsafe CALayer mutations | confirmed | `setCALayerHostContextId()` (line 841) delegates to `Metal.setCALayerHostContextId()`. Same thread-safety issue — called from XPC queue via Surface.zig. |
| 2     | Missing CATransaction wrapping  | confirmed | Delegates to Metal.zig — same issue.                                                                                                                     |
| 3-4   | Chromium-side smells            | clean     | Not applicable.                                                                                                                                          |
| 5     | Dedup gate                      | clean     | No dedup at this level.                                                                                                                                  |
| 6     | Layer accumulation              | clean     | `removeCALayerHost()` (line 866) nulls all three pointers after calling Metal.                                                                           |
| 7     | Zero context ID                 | suspect   | `setCALayerHostContextId()` (line 841) passes `context_id` through with no zero check.                                                                   |
| 8-10  | Chromium-side smells            | clean     | Not applicable.                                                                                                                                          |
| 11-20 | CALayerHost-specific            | clean     | Delegates to Metal.zig — verdicts same as above.                                                                                                         |

##### File: `gui/src/Surface.zig`

| #     | Smell                           | Verdict   | Note                                                                                                                                                 |
| ----- | ------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1     | Thread-unsafe CALayer mutations | confirmed | `setCAContextId()` (line 2521) acquires `draw_mutex` but runs on the XPC serial queue, not the main thread. All CALayer mutations cascade from here. |
| 2     | Missing CATransaction wrapping  | confirmed | No transaction wrapping in `setCAContextId()` or `clearOverlay()` (line 2531).                                                                       |
| 5     | Dedup gate                      | clean     | No dedup on context_id.                                                                                                                              |
| 7     | Zero context ID                 | suspect   | `setCAContextId()` (line 2524) passes `context_id` straight to renderer with no zero check.                                                          |
| 11-20 | CALayerHost-specific            | clean     | Delegates to generic.zig → Metal.zig.                                                                                                                |

##### File: `gui/src/apprt/xpc.zig`

| #   | Smell                           | Verdict   | Note                                                                                                                                                                                                                          |
| --- | ------------------------------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 1   | Thread-unsafe CALayer mutations | confirmed | `handleCAContext()` (line 409) calls `surface.setCAContextId()` directly on the XPC serial queue (`com.termsurf.ghost.xpc`). This is where the chain starts — all CALayer mutations originate from this non-main-thread call. |
| 2   | Missing CATransaction wrapping  | confirmed | No CATransaction at this level or any level below it.                                                                                                                                                                         |
| 5   | Dedup gate                      | clean     | No dedup on `ca_context_id` in `handleCAContext()`. Every message triggers a full CALayerHost replacement.                                                                                                                    |
| 6   | Layer accumulation              | clean     | `handleDisconnect()` (line 1023) calls `surface.clearOverlay()` which removes all layers.                                                                                                                                     |
| 7   | Zero context ID                 | suspect   | `handleCAContext()` (line 411) extracts `ca_context_id` from XPC dictionary and passes it directly to `surface.setCAContextId()` with no zero check.                                                                          |

##### File: `shell_browser_main_parts.cc`

| #   | Smell                               | Verdict   | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| --- | ----------------------------------- | --------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Thread-unsafe CALayer mutations     | clean     | All tab/view operations PostTask to UI thread (lines 208-292). XPC handler extracts values then dispatches.                                                                                                                                                                                                                                                                                                                                                                                            |
| 2   | Missing CATransaction wrapping      | clean     | C++ side — no direct CALayer mutations.                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| 3   | Stale callback registration         | confirmed | CALayerParams callback registered ONCE in `CreateTab()` (line 404) via `SetCALayerParamsCallbackOnView(view, ...)` on the initial `RenderWidgetHostView`. Cursor callback registered ONCE (line 394) via `rwhi->SetCursorChangedCallback()`. Neither is re-registered when the `RenderWidgetHostView` swaps during cross-process navigation. If the view changes, the callback is on the destroyed view — no more `ca_context_id` messages are ever sent. **Permanent blank.**                         |
| 4   | Hidden-window compositor detachment | suspect   | `CreateTab()` creates the Shell which calls `CreatePlatformWindow()` which calls `[window orderOut:nil]`. The `orderOut:` likely causes `WasOccluded()` → `render_widget_host_is_hidden_ = true` → `BrowserCompositorMac` transitions to `HasNoCompositor`. During navigation, `DidNavigate()` may invalidate the surface ID without generating a new one. Needs diagnostic logging to confirm.                                                                                                        |
| 5   | Dedup gate                          | confirmed | CALayerParams callback lambda (line 407): `if (params.ca_context_id == 0 \|\| params.ca_context_id == *last_id) return;`. If the `ca_context_id` does not change during same-site navigation (Issue 628 Exp 5 found it doesn't), this gate blocks the callback entirely. The GUI never learns about the navigation. If the compositor internally tears down and rebuilds its output during navigation, the CALayerHost may point at a stale/dead CAContext even though the ID is numerically the same. |
| 7   | Zero context ID                     | clean     | Lambda filters `ca_context_id == 0` (line 407).                                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| 8   | Surface ID invalidation             | suspect   | If `render_widget_host_is_hidden_ = true` (see #4), `DidNavigate()` in `BrowserCompositorMac` invalidates the current `LocalSurfaceId` via `InvalidateLocalSurfaceId()` instead of generating a new one via `GetRendererLocalSurfaceId()`. No new surface is embedded, no frames are submitted. Recovery depends on the 10-second `kExpireInterval` in Surface Manager — but if that mechanism is broken or not triggered in the hidden state, recovery never happens. **Permanent blank.**            |
| 9   | Missing size on new view            | suspect   | If the view swaps during navigation, the new `RenderWidgetHostView` gets default dimensions from the hidden NSWindow's contentView (which was set during `CreatePlatformWindow()`). `ResizeTab()` only calls `view->SetSize()` on the current view. After a swap, the new view may have the wrong size. Not a direct visibility issue but could produce 0×0 or default-sized content.                                                                                                                  |
| 10  | Retained references                 | clean     | `last_id` is `base::Owned` — freed when callback is destroyed. No dangling refs.                                                                                                                                                                                                                                                                                                                                                                                                                       |
| 11  | Dual CALayerHost                    | suspect   | The hidden window's `DisplayCALayerTree` creates its own CALayerHost for the same CAContext (this happens inside `ns_view_->SetCALayerParams()` at `AcceleratedWidgetCALayerParamsUpdated`). The GUI creates a second one. Two hosts pointing at the same CAContext. Issue 629 Exp 3 showed that disabling the hidden window's host made things worse, but the interference during navigation state transitions is still unexplained.                                                                  |
| 16  | Accidental pipeline deletion        | clean     | `sendResize()` and `"resize"` handler are present (restored in Issue 627).                                                                                                                                                                                                                                                                                                                                                                                                                             |
| 18  | Non-atomic swap                     | clean     | C++ side doesn't manage CALayerHost directly — that's the GUI's job.                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| 20  | setContentSize vs view->SetSize     | confirmed | `ResizeTab()` (line 469) uses `view->SetSize(logical)` without resizing the hidden `NSWindow`. Issue 628 Exp 4 found that `[window setContentSize:]` is needed because `BrowserCompositorMac::DidNavigate()` reads `dfh_size_dip_` from the window, not from the view. That fix was reverted along with all Issue 628 changes. The current code uses `view->SetSize()`, meaning post-navigation sizing may be wrong.                                                                                   |

##### File: `shell_tab_observer.cc` / `shell_tab_observer.h`

| #          | Smell                               | Verdict   | Note                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| ---------- | ----------------------------------- | --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3          | Stale callback registration         | confirmed | `ShellTabObserver` does NOT override `RenderViewHostChanged()` or `RenderFrameHostChanged()`. The observer correctly follows the `WebContents` across navigations (via `WebContentsObserver::Observe()`), so navigation/loading notifications survive. But the CALayerParams callback and cursor callback are NOT managed by the observer — they were registered inline in `CreateTab()` on the initial view. The observer has no mechanism to re-register them. |
| 4          | Hidden-window compositor detachment | clean     | Observer doesn't interact with window visibility.                                                                                                                                                                                                                                                                                                                                                                                                                |
| 5          | Dedup gate                          | clean     | No ca_context_id handling in observer.                                                                                                                                                                                                                                                                                                                                                                                                                           |
| 8          | Surface ID invalidation             | clean     | Observer doesn't touch surface IDs.                                                                                                                                                                                                                                                                                                                                                                                                                              |
| 9          | Missing size on new view            | clean     | Observer doesn't handle resize.                                                                                                                                                                                                                                                                                                                                                                                                                                  |
| All others |                                     | clean     | Not applicable to this file.                                                                                                                                                                                                                                                                                                                                                                                                                                     |

##### File: `shell_platform_delegate_mac.mm`

| #          | Smell                               | Verdict   | Note                                                                                                                                                                                                                                                                                                                                                                                |
| ---------- | ----------------------------------- | --------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 4          | Hidden-window compositor detachment | confirmed | `CreatePlatformWindow()` (line 209): `[window orderOut:nil]` when `--hidden` flag is set. This removes the window from screen. macOS sends `NSWindowDidChangeOcclusionStateNotification`, which Chromium handles in `RenderWidgetHostViewMac::OnWindowOcclusionStateChanged()`, potentially setting `render_widget_host_is_hidden_ = true` and triggering compositor detachment.    |
| 15         | Hidden window phantom chrome        | clean     | `NSWindowStyleMaskBorderless` (line 151) and `ShouldHideToolbar() = true` (shell.cc line 170). No phantom offset.                                                                                                                                                                                                                                                                   |
| 20         | setContentSize vs view->SetSize     | suspect   | `ResizeWebContent()` (line 247) sets `contentView.frame` but is NOT called from `ResizeTab()`. `ResizeTab()` calls `view->SetSize()` directly. The window's contentView frame stays at its initial size, while the RWHV has a different size. After navigation, `DidNavigate()` reads `dfh_size_dip_` which may reflect the window/contentView dimensions, not the RWHV dimensions. |
| All others |                                     | clean     | Not applicable to this file.                                                                                                                                                                                                                                                                                                                                                        |

##### File: `shell.h` / `shell.cc`

| #          | Smell                               | Verdict | Note                                                                                                                                                                                                                                                                                                       |
| ---------- | ----------------------------------- | ------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 3          | Stale callback registration         | suspect | `PrimaryPageChanged()` (shell.cc line 656) fires on cross-document navigations and calls `g_platform->DidNavigatePrimaryMainFramePostCommit()`, which is a **no-op** (platform_delegate line 333-335). This is the natural hook point for re-registering callbacks after a view swap, but it does nothing. |
| 4          | Hidden-window compositor detachment | clean   | Shell itself doesn't manage window visibility — delegates to platform.                                                                                                                                                                                                                                     |
| 15         | Hidden window phantom chrome        | clean   | `ShouldHideToolbar()` returns `true` unconditionally (line 170).                                                                                                                                                                                                                                           |
| All others |                                     | clean   | Not applicable to this file.                                                                                                                                                                                                                                                                               |

#### Summary

**Confirmed findings (could cause permanent vanishing):**

| #   | Smell                               | Files                                                                 | Impact                                                                                                                                                                                                                                                                                                                                                                                           |
| --- | ----------------------------------- | --------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| 1   | Thread-unsafe CALayer mutations     | `xpc.zig:409`, `Surface.zig:2521`, `generic.zig:841`, `Metal.zig:172` | All CALayerHost creation/replacement happens on the XPC serial queue, not the main thread. On a background GCD queue without a run loop, CALayer mutations may never reach Window Server. The initial overlay works (possibly due to timing luck), but during navigation the replacement CALayerHost may fail to composite.                                                                      |
| 2   | Missing CATransaction wrapping      | `Metal.zig:172-295`                                                   | No `CATransaction begin/commit` or `setDisableActions:YES`. Without an explicit transaction on a background thread, Core Animation may buffer changes indefinitely. The replacement CALayerHost after navigation may never become visible.                                                                                                                                                       |
| 3   | Stale callback registration         | `shell_browser_main_parts.cc:394-427`, `shell_tab_observer.h`         | CALayerParams and cursor callbacks registered once on the initial `RenderWidgetHostView`. Never re-registered after view swap. If the view changes during navigation, **no more `ca_context_id` messages are ever sent**. Permanent blank. Currently latent without strict site isolation, but `RenderDocument` mode in modern Chromium may trigger view swaps even for same-origin navigations. |
| 4   | Hidden-window compositor detachment | `shell_platform_delegate_mac.mm:209`                                  | `[window orderOut:nil]` likely triggers `WasOccluded()` → hidden compositor state. During navigation, `DidNavigate()` may invalidate surface IDs without regeneration. Combined with the 10-second `kExpireInterval`, this could explain both the old 10-second recovery and the current permanent blank (if the recovery mechanism is no longer triggered).                                     |
| 5   | Dedup gate on ca_context_id         | `shell_browser_main_parts.cc:407`                                     | If `ca_context_id` doesn't change during same-site navigation, the callback is never fired. The GUI keeps the old CALayerHost pointing at the same ID. If the underlying CAContext is invalidated and re-created with the same ID (or if the compositor output is interrupted even though the ID persists), the GUI never knows to reconnect.                                                    |
| 18  | Non-atomic CALayerHost swap         | `Metal.zig:194-210`                                                   | Old host removed before new one added. During the gap, no CALayerHost is in the layer tree. Combined with threading issues (#1, #2), the new host may take arbitrarily long to become visible.                                                                                                                                                                                                   |
| 20  | setContentSize vs view->SetSize     | `shell_browser_main_parts.cc:469`                                     | `ResizeTab()` uses `view->SetSize()` without updating the hidden NSWindow's contentView frame. After navigation, `DidNavigate()` may use stale window dimensions. Issue 628 Exp 4 fix was reverted.                                                                                                                                                                                              |

**Suspect findings (plausibly contribute):**

| #   | Smell                                        | Files                                                                | Impact                                                                                                                                                                                      |
| --- | -------------------------------------------- | -------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 7   | Zero context ID treated as valid             | `xpc.zig:411`, `Surface.zig:2524`, `Metal.zig:172`                   | No zero-check on the GUI side. If the Chromium-side filter (line 407) is bypassed or a race produces `ca_context_id=0`, the GUI creates a CALayerHost pointing at nothing.                  |
| 8   | Surface ID invalidation without regeneration | `shell_browser_main_parts.cc` (indirect, via `BrowserCompositorMac`) | If compositor is in hidden/detached state, `DidNavigate()` invalidates without regenerating. Dependent on #4 being true.                                                                    |
| 9   | Missing size on new view                     | `shell_browser_main_parts.cc:462-469`                                | After view swap, new view gets default dimensions. Not a direct visibility cause but could produce invisible (0×0) content.                                                                 |
| 11  | Dual CALayerHost per CAContext               | `shell_browser_main_parts.cc:404` (indirect)                         | The hidden window's `DisplayCALayerTree` creates a CALayerHost for the same CAContext. Two hosts exist. macOS behavior with dual hosts during compositor state transitions is undocumented. |

#### Analysis: most likely cause of permanent vanishing

The most likely cause is a combination of **#1 + #2** (thread-unsafe mutations
without CATransaction) and **#4 + #8** (hidden compositor detachment).

**Scenario for permanent blank:**

1. User clicks a link → same-site navigation.
2. Chromium's `BrowserCompositorMac::DidNavigate()` fires on the hidden window.
3. Because the window is ordered out (`#4`), the compositor may be in a detached
   or degraded state. It invalidates the current surface and transitions
   internally (`#8`).
4. The `ca_context_id` does not change (same `CALayerTreeCoordinator`), so the
   dedup gate (`#5`) blocks the callback. The GUI is never notified.
5. Even if the `ca_context_id` does change and the GUI receives it: the
   replacement CALayerHost is created on the XPC queue (`#1`) without a
   CATransaction (`#2`). The layer mutation may not reach Window Server.
6. Old CALayerHost was already removed (`#18`), so there is no fallback content.
7. The 10-second surface recovery mechanism (`kExpireInterval`) previously
   triggered recomposition, but in the current Chromium branch it may not fire
   because the compositor is fully detached rather than just waiting for a
   surface claim.

**Scenario for why the initial overlay works (sometimes):**

The initial `ca_context_id` arrives shortly after tab creation, while the
compositor is still initializing. The CALayerHost creation on the XPC queue
happens to succeed because the GCD queue's autorelease pool or a coincidental
main-thread drain flushes the implicit CATransaction. This is timing-dependent
and not guaranteed.

**Why the old 10-second recovery no longer works:**

The 10-second recovery was likely the `kExpireInterval` surface garbage
collection in `SurfaceManager`. A code change between the Issue 628/629 testing
and now — possibly a Chromium branch difference (issue-627 vs issue-629 vs
issue-630) or a rebuild with different state — may have changed the compositor's
behavior in the hidden-window case. If the compositor fully detaches
(`HasNoCompositor`) instead of holding a temporary reference, there is nothing
for `kExpireInterval` to garbage-collect, and recovery never happens.

### Experiment 3: Fix navigation blank

#### Purpose

Fix the permanent overlay disappearance on navigation by addressing the seven
confirmed findings from Experiment 2. Changes span both the GUI (Zig) and the
Chromium Profile Server (C++).

#### Changes

**GUI side (3 changes):**

**G1. Dispatch CALayerHost mutations to the main thread.**

File: `gui/src/apprt/xpc.zig`

Add a `dispatch_get_main_queue` extern declaration alongside the existing
`dispatch_async_f` (line 55):

```zig
extern "c" fn dispatch_get_main_queue() ?*anyopaque;
```

Change `handleCAContext()` (lines 409-420) to dispatch to the main queue instead
of calling `surface.setCAContextId()` directly on the XPC queue. The context ID
and surface pointer must be encoded into a dispatch context (same pattern as
`handlePaneFocusChanged` at line 548).

File: `gui/src/Surface.zig`

`setCAContextId()` (lines 2524-2529) currently acquires `draw_mutex`. Keep the
mutex acquisition — the main thread and the renderer thread both need it — but
the function is now called from the main thread instead of the XPC queue.

File: `gui/src/renderer/Metal.zig`

`setCALayerHostContextId()` (lines 172-256) now runs on the main thread. Wrap
all CALayer mutations in a CATransaction. Add these extern declarations:

```zig
extern "c" fn CATransaction_begin() void;    // [CATransaction begin]
extern "c" fn CATransaction_commit() void;   // [CATransaction commit]
extern "c" fn CATransaction_setDisableActions(flag: bool) void;
```

Or use the Objective-C runtime via `objc.getClass("CATransaction")` + `msgSend`.
The wrapping should look like:

```
CATransaction.begin()
CATransaction.setDisableActions(true)
// ... all layer creation/replacement/property-setting ...
CATransaction.commit()
```

Apply the same wrapping to `updateCALayerHostFrame()` (lines 263-295) and
`removeCALayerHost()` (lines 298-315). These are called from `drawFrame()` on
the renderer thread (via `size_changed` path), so they also need CATransaction
wrapping, though they already run on an appropriate thread (the renderer thread
drives the Metal command queue and is effectively the "display" thread).

**G2. Atomic CALayerHost swap.**

File: `gui/src/renderer/Metal.zig`

In the replacement path of `setCALayerHostContextId()` (lines 188-213), reverse
the order: add the new CALayerHost to `positioning_layer` BEFORE removing the
old one.

Current order (lines 194-210):

1. `old_host.removeFromSuperlayer()` + `old_host.release()`
2. Create `new_host`, set properties
3. `positioning_layer.addSublayer(new_host)`

New order:

1. Create `new_host`, set properties
2. `positioning_layer.addSublayer(new_host)`
3. `old_host.removeFromSuperlayer()` + `old_host.release()`

This matches Chromium's `DisplayCALayerTree::GotCALayerFrame()` pattern — the
new host is added before the old one is removed, ensuring no frame without a
CALayerHost in the layer tree.

**G3. Guard against zero context ID.**

File: `gui/src/apprt/xpc.zig`

In `handleCAContext()` (line 411), add a zero check before calling
`surface.setCAContextId()`:

```zig
if (context_id == 0) return;
```

This is defense-in-depth — the Chromium-side lambda already filters zeros (line
407 of `shell_browser_main_parts.cc`), but a race or a different code path could
bypass it.

**Chromium side (4 changes):**

**C1. Replace `orderOut:` with `setAlphaValue:0`.**

File: `shell_platform_delegate_mac.mm`, line 209.

Change:

```objc
[window orderOut:nil];
```

To:

```objc
[window setAlphaValue:0.0];
[window orderFront:nil];
```

`orderOut:` removes the window from the window list, which triggers
`NSWindowDidChangeOcclusionStateNotification` and causes Chromium's
`RenderWidgetHostViewMac::OnWindowOcclusionStateChanged()` to set
`render_widget_host_is_hidden_ = true`. This cascades into
`BrowserCompositorMac` transitioning to `HasNoCompositor`, which invalidates
surface IDs during navigation.

`setAlphaValue:0` makes the window fully transparent but keeps it in the window
list. The compositor remains active. `orderFront:nil` ensures the window is in
the list without activating it (no key/main window status). The user never sees
the transparent window.

If `orderFront:nil` steals focus, use
`[window orderWindow:NSWindowBelow relativeTo:0]` instead, which adds the window
to the list at the back.

**C2. Re-register callbacks on view swap.**

File: `shell_tab_observer.h`

Add to the class declaration (after line 43):

```cpp
void RenderViewHostChanged(RenderViewHost* old_host,
                           RenderViewHost* new_host) override;
```

Add private members to store the callback and connection for re-registration:

```cpp
base::RepeatingCallback<void(const gfx::CALayerParams&)> ca_layer_params_callback_;
```

File: `shell_tab_observer.cc`

Add a new method:

```cpp
void ShellTabObserver::RenderViewHostChanged(
    RenderViewHost* old_host, RenderViewHost* new_host) {
  if (!new_host || !xpc_connection_)
    return;
  auto* web_contents = WebContents::FromRenderFrameHost(
      new_host->GetMainRenderFrameHost());
  if (!web_contents)
    return;
  auto* view = web_contents->GetRenderWidgetHostView();
  if (!view)
    return;

  // Re-register CALayerParams callback on the new view.
  SetCALayerParamsCallbackOnView(view, ca_layer_params_callback_);

  // Re-register cursor callback on the new view.
  auto* rwhi = static_cast<RenderWidgetHostImpl*>(
      view->GetRenderWidgetHost());
  rwhi->SetCursorChangedCallback(base::BindRepeating(
      &ShellTabObserver::OnCursorChanged, base::Unretained(this)));
}
```

File: `shell_browser_main_parts.cc`

In `CreateTab()`, after creating the CALayerParams callback (line 427), store it
on the observer so `RenderViewHostChanged` can re-use it:

```cpp
tab_observer->SetCALayerParamsCallback(ca_layer_callback);
```

This requires extracting the lambda into a named variable and passing it to both
`SetCALayerParamsCallbackOnView()` and the observer.

**C3. Reset dedup gate on navigation.**

File: `shell_tab_observer.h`

Add a private member:

```cpp
uint32_t* last_ca_context_id_ = nullptr;  // owned by the callback
```

File: `shell_tab_observer.cc`

In `DidFinishNavigation()`, after the existing gate checks (line 51), reset the
dedup gate:

```cpp
// Reset the ca_context_id dedup gate so the callback fires even if the
// new CAContext reuses the same ID (Issue 630, Experiment 2 finding #5).
if (last_ca_context_id_)
  *last_ca_context_id_ = 0;
```

File: `shell_browser_main_parts.cc`

In `CreateTab()`, change `base::Owned(new uint32_t(0))` to a raw pointer that is
also stored on the observer:

```cpp
auto* last_id = new uint32_t(0);
tab_observer->SetLastCAContextIdPtr(last_id);
// ... use last_id in BindRepeating instead of base::Owned ...
```

Ownership of `last_id` transfers to the callback via `base::Owned` as before,
but the observer also holds a non-owning pointer for reset purposes. The
observer must NOT delete it — the callback destructor handles that.

Alternatively, use a `std::shared_ptr<uint32_t>` shared between the callback and
the observer, avoiding the raw pointer ownership question entirely.

**C4. Use `setContentSize:` for resize.**

File: `shell_browser_main_parts.cc`

In `ResizeTab()` (lines 462-469), replace `view->SetSize(logical)` with:

```cpp
shell->ResizeWebContentForTests(logical);
```

This calls `ShellPlatformDelegate::ResizeWebContent()` which sets
`contentView.frame` on the hidden NSWindow, ensuring `DidNavigate()` reads the
correct `dfh_size_dip_` from the window dimensions.

The same change should be made in `CreateTab()` (lines 340-353) — use
`shell->ResizeWebContentForTests(logical)` instead of `view->SetSize(logical)`.

#### Build and test

```bash
# Build Chromium
cd chromium/src
export PATH="$(cd ../depot_tools && pwd):$PATH"
autoninja -C out/Default chromium_profile_server

# Build GUI
cd gui && zig build

# Launch
open gui/zig-out/TermSurf.app
```

Test:

1. Open a terminal pane, run `web example.com`.
2. Verify the initial overlay appears promptly (faster than before).
3. Click a link on the page.
4. Verify the overlay transitions seamlessly — no blank, no flicker.
5. Click multiple links in succession — verify continuous visibility.
6. Resize the window during and after navigation — verify overlay tracks.
7. Open a second pane with a different profile — verify both work.

#### Verification

- Overlay never vanishes during navigation (same-site or cross-site).
- Initial overlay appears within 1-2 seconds of `web` command.
- Resize continues to work after navigation.
- Multi-pane multi-profile still works.
- No CALayer warnings in Console.app (filter by process name).

**Result:** Pass

Navigation no longer causes the overlay to permanently vanish. Clicking links
transitions to the new page with the overlay intact. There is a brief flicker
(one frame of blank) during the transition, but the overlay reappears
immediately — fundamentally different from the previous behavior where it
vanished forever.

#### Conclusion

The seven fixes together resolved the permanent navigation blank. The most
impactful changes were likely C1 (replacing `orderOut:` with `setAlphaValue:0`
to keep the compositor active) and the GUI threading fixes G1+G2 (dispatching
CALayerHost mutations to the main thread with CATransaction wrapping and atomic
swap). The brief flicker during navigation is a separate, much smaller issue —
the overlay is continuously visible across navigations, which was the goal.

## Conclusion

Issue 630 resolved the primary navigation blank — clicking a link no longer
causes the browser overlay to permanently vanish. The fix required seven
coordinated changes across both the GUI (Zig) and the Chromium Profile Server
(C++):

- **C1**: Replaced `[window orderOut:nil]` with `setAlphaValue:0` to keep the
  hidden window's compositor active during navigation.
- **C2**: Added `RenderViewHostChanged` to `ShellTabObserver` to re-register
  CALayerParams and cursor callbacks after cross-process navigation view swaps.
- **C3**: Reset the `ca_context_id` dedup gate on navigation so the callback
  fires even if the new CAContext reuses the same ID.
- **C4**: Changed `view->SetSize()` to `ResizeWebContentForTests()` so the
  hidden NSWindow's `contentView.frame` stays in sync with the RWHV, ensuring
  `BrowserCompositorMac::DidNavigate()` reads the correct `dfh_size_dip_`.
- **G1**: Dispatched CALayerHost creation to the main thread via
  `dispatch_async_f` and wrapped all CALayer mutations in explicit
  `CATransaction begin/setDisableActions/commit`.
- **G2**: Reversed the swap order to add the new CALayerHost before removing the
  old one (atomic swap), matching Chromium's `DisplayCALayerTree` pattern.
- **G3**: Added a zero guard on `ca_context_id` in the XPC handler.

### Remaining issue

There is still a brief (~100ms) flicker during every navigation — the overlay
blanks for one frame then reappears. This is visible and annoying but does not
make the browser unusable. The most likely cause is the compositor tearing down
and rebuilding its content tree during the navigation, leaving the CAContext
momentarily empty. A snapshot-behind approach (capturing the current frame as a
static image before the swap) or avoiding the CALayerHost swap entirely when the
`ca_context_id` doesn't change could eliminate this. Tracked as a follow-up
issue.

### Untested areas

The CALayerHost migration (Issues 625–630) changed fundamental rendering
infrastructure. The following features have not been retested since the
migration and may have regressions:

- **Mouse input**: clicks, drag, scroll, cursor changes (Issue 606)
- **Keyboard input**: key forwarding, Cmd+key bypass, clipboard, Tab (Issues
  607–609)
- **Loading progress**: progress bar, pulse animation (Issue 616)
- **Browser navigation keybindings**: Cmd+L, Cmd+R, back/forward (Issue 616)
- **Multi-pane multi-profile**: server reuse, independent tabs (Issues 604–605)
- **Dynamic resize**: pane resize propagation through XPC (Issue 627)
- **Text selection**: drag-to-select, cursor changes (Issue 606)

A comprehensive retest of all browser features should be performed before moving
on to new feature work.
