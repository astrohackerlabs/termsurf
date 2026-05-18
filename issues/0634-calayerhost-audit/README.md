+++
status = "closed"
opened = "2026-02-24"
closed = "2026-02-24"
+++

# Issue 634: CALayerHost Feature Audit

## Goal

Systematically test every TermSurf feature that touches the browser overlay
pipeline after the CALayerHost migration (Issues 625–633). Confirm everything
works end-to-end with the persistent compositor architecture before moving on to
new features.

## Background

Issues 625–633 replaced the `FrameSinkVideoCapturer` pipeline with zero-copy
`CALayerHost` compositing and fixed every bug that surfaced along the way:

- [Issue 625](625-calayerhost.md) — **CALayerHost migration.** Replaced
  FrameSinkVideoCapturer (120fps IOSurface Mach port transfer, 15-25ms latency)
  with CALayerHost. GPU process sends `ca_context_id` once per tab; Window
  Server composites directly from VRAM. Zero per-frame IPC.

- [Issue 626](626-x-y-calayerhost.md) — **X/Y positioning.** Fixed ~10px Y and
  ~3px X offset. Added intermediate `flipped_layer` with `geometryFlipped=YES`
  matching Chromium's `DisplayCALayerTree` pattern. Applied Y-flip formula for
  `IOSurfaceLayer` coordinates.

- [Issue 627](627-resize-calayerhost.md) — **Resize tracking.** Restored resize
  pipeline (accidentally removed in Issue 625). Introduced 3-layer architecture:
  `flipped_layer` (auto-fill) → `positioning_layer` (explicit frame) →
  `CALayerHost`. Overlay stays pinned to top-left, grows downward.

- [Issue 628](628-navigation-calayerhost.md) — **Navigation fix (first
  attempt).** Eight experiments, all failed. Overlay vanished for ~10 seconds on
  link click. Identified that new `ca_context_id` arrives within 100ms but
  something prevents rendering.

- [Issue 629](629-understand-nav-calayerhost.md) — **Navigation blank
  diagnosis.** Pure research. Root cause: new `ca_context_id` is created per
  navigation, but CALayerHost is created before GPU renders first frame into new
  CAContext. Window Server has no content to composite.

- [Issue 630](630-nav-calayerhost-6.md) — **Navigation fix (coordinated).**
  Seven coordinated fixes across GUI and Chromium resolved permanent overlay
  disappearance. Overlay reappears after navigation, but ~100ms flicker remains.

- [Issue 631](631-continue-nav-calayerhost.md) — **Flicker investigation.** Five
  experiments, all failed. Flicker is inherent to CALayerHost swap — per-
  navigation `CAContext` creation forces GUI to swap CALayerHosts.

- [Issue 632](632-nav-flicker-calayerhost.md) — **Flicker diagnosis.** Four
  diagnostic experiments. Confirmed `UseParentLayerCompositor` mode can be
  adopted without Chrome's `ui/views` framework. All required types available to
  content embedders.

- [Issue 633](633-persistent-compositor.md) — **Persistent compositor.**
  Switched profile server from `HasOwnCompositor` to `UseParentLayerCompositor`
  mode. `PersistentCompositorBridge` implements `AcceleratedWidgetMacNSView` to
  receive stable `ca_context_id`. Zero flicker. Navigation seamless.

## Test plan

Each test is manual. Mark PASS or FAIL with notes.

Build: release (`./build-release.sh`)

### T1: Basic page load

Open TermSurf, run `web`, navigate to a URL. Content renders in the browser
pane.

**Result: PASS**

### T2: Link navigation

Click a link on a page. Content transitions without flicker. New page renders
correctly.

**Result: PASS**

Bug: refocusing the window eats the first click even though the browser pane is
already focused. The focus-eating logic incorrectly treats a window-refocus
click as a pane-focus click. Link navigation itself works correctly with no
flicker.

### T3: Back/forward navigation

Use browser navigation keybindings (Cmd+[ and Cmd+]) to go back and forward.
Pages render without flicker.

**Result: PASS**

### T4: Page reload

Reload the current page (Cmd+R). Content re-renders without blank frame.

### T5: Resize (window)

Drag the window edge to resize. Browser overlay tracks the TUI viewport
correctly. No misalignment, no stale frame.

**Result: PASS**

### T6: Resize (pane split)

With multiple panes, drag the split divider. Browser overlay resizes to match
the new pane dimensions.

**Result: PASS**

### T7: Multi-pane

Open two or more browser panes in splits. Each renders independently. Both
display content simultaneously.

**Result: PASS**

### T8: Multi-profile

Open browser panes with different profiles. Each gets its own Chromium Profile
Server process. Content renders independently in each.

### T9: Mouse clicks

Click on links, buttons, and form fields in web content. Events reach Chromium
and trigger expected behavior.

**Result: PASS**

### T10: Mouse drag and text selection

Click and drag to select text on a web page. Selection highlighting appears.
Cmd+C copies selected text.

**Result: PASS**

### T11: Mouse scroll

Scroll a page with the trackpad or mouse wheel. Page scrolls smoothly. Scroll
events reach Chromium.

**Result: PASS**

### T12: Cursor changes

Hover over links (should show pointer cursor), text (should show I-beam), and
default areas. Cursor changes correctly.

**Result: PASS**

### T13: Keyboard input (typing)

Click a text input or search field. Type characters. Text appears in the field.

### T14: Keyboard input (Cmd+key bypass)

Cmd+C, Cmd+V, Cmd+A, Cmd+X bypass the browser and work as expected (copy, paste,
select all, cut).

### T15: Keyboard input (Tab)

Press Tab to move between form fields. Focus advances correctly.

### T16: Focus lifecycle

Click between a terminal pane and a browser pane. Focus follows correctly.
Keyboard input routes to the focused pane.

### T17: Loading indicator

Navigate to a page. Progress indicator appears during load and disappears when
the page finishes loading.

### T18: URL sync

Navigate to a new page (by clicking a link or entering a URL). The URL bar in
the TUI updates to reflect the current page.

### T19: Tab creation

Open a new `web` tab. Browser pane appears and renders content.

### T20: Tab close

Close a browser tab. Overlay disappears. If it was the last tab for a profile,
the Chromium Profile Server exits.

### T21: Retina rendering

On a Retina display, content renders at physical pixel resolution (not blurry).
Text is sharp. Compare with native Chrome side-by-side if needed.

### T22: Overlay positioning (pixel-perfect)

Compare the browser overlay position with the TUI viewport border. Content
should align to the pixel — no gap, no overlap, no offset.

## Conclusion

The audit passed 11 of the first 12 tests (T1–T7, T9–T12) before uncovering a
severe regression: opening a second pane with the same browser profile causes
both webviews to navigate to the new URL. The profile server is supposed to
manage independent tabs — each pane gets its own `WebContents`, its own URL, its
own navigation history. Instead, the persistent compositor changes from Issue
633 appear to have conflated the two tabs, routing all navigation through a
single shared path.

This is a fundamental correctness issue. Multi-tab isolation within a single
profile is the core architecture of the profile server (established in Issues
503–511). A broken multi-tab mode means the CALayerHost migration has regressed
a feature that worked correctly under the old `FrameSinkVideoCapturer` pipeline.

The remaining tests (T4, T8, T13–T22) are deferred. There is no point testing
keyboard input, multi-profile, or tab lifecycle when multi-tab within a single
profile is broken. The next issue will diagnose and fix this regression, then
the audit will resume.

### Minor bug noted

T2 found that refocusing the TermSurf window eats the first click even though
the browser pane is already focused. The focus-eating logic incorrectly treats a
window-refocus click as a pane-focus click. This is cosmetic — not blocking.
