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
   | `146.0.7650.0-issue-630` | [Issue 630](issues/630-nav-calayerhost-6.md) | Fix navigation blank |
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

Find what causes the browser overlay to vanish for ~10 seconds when clicking a
link, and what causes the initial page to take longer than expected to appear.

The audit checks every CALayerHost-related code path in both the GUI (Zig) and
Chromium Profile Server (C++) against 20 known code smells. Each smell is
evaluated solely for whether it could cause the overlay to disappear or delay
its appearance. Smells that only affect positioning, color, performance, or
other non-visibility concerns are marked clean and skipped.

The core questions are:

1. **Why does the overlay vanish on navigation and reappear ~10 seconds later?**
2. **Why does the initial overlay take longer than expected to appear?**
3. **What is the correct CALayerHost lifecycle for continuous visibility?** How
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
