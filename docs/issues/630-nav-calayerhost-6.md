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
