# Issue 408: Two Profiles at 60fps

## Goal

Render two Chromium `BrowserContext` instances side by side in a single macOS
window at 60fps or higher. Issue 407 proved that multiple profiles coexist in
one process and that content_shell renders a single WebContents at 60fps. But
placing two WebContents in one window dropped both to 2-3fps because manual
NSView manipulation broke Chromium's internal visibility tracking. This issue
solves the framerate problem.

## Background

Issue 407 established:

- **Multi-profile works.** Two `ShellBrowserContext` instances with different
  storage paths run in the same process with full isolation (separate cookies,
  localStorage, cache).
- **Single WebContents renders at 60fps.** content_shell's windowed rendering
  path has no framerate ceiling.
- **Two WebContents in one window renders at 2-3fps.** The
  `RenderWidgetHostViewCocoa` has NSView-level visibility tracking that
  overrides explicit `WasShown()` calls. Manually reparenting and resizing views
  causes it to misreport visibility, triggering Chromium's background tab
  throttle.

The throttling chain:

```
RenderWidgetHostViewCocoa (NSView visibility)
  -> RenderWidgetHostImpl::WasShown() / WasHidden()
  -> Blink PageSchedulerImpl::SetPageVisible()
  -> CC SchedulerStateMachine::visible_
  -> ShouldSubscribeToBeginFrames()
  -> vsync subscription ON (60fps) or OFF (~1fps)
```

## Success Criteria

- Two panes in one window, each showing a different localStorage string.
- Both panes render the spinning blue square at 60fps or higher.
- Strings persist across app restarts.
- No custom IPC protocol between the embedder and Chromium.

## Approaches to Investigate

### Approach 1: Chromium `views` framework

content_shell uses raw NSWindows, bypassing Chromium's `views` layer. Chrome
itself uses `views::WebView` to embed WebContents into `views::Widget` windows.
The `views` framework handles visibility, layout, occlusion, and resize
notifications through proper Chromium channels.

**Idea:** Create a `views::Widget` with two `views::WebView` children, each
backed by a different `BrowserContext`. The `views` framework manages visibility
through `NativeWidgetMacNSWindowHost` and `windowDidChangeOcclusionState:`,
which should keep both views at full framerate.

**Risk:** The `views` framework is large and tightly coupled to Chrome's UI.
Using it from a minimal embedder may pull in unwanted dependencies. It may also
assume Chrome-specific infrastructure (like `BrowserView`, `TabStripModel`) that
doesn't exist in our app.

### Approach 2: Off-screen compositing via CopyFromSurface

Use `RenderWidgetHostView::CopyFromSurface()` to capture each WebContents'
rendered output as a bitmap or GPU texture, then composite both into a single
Metal render pass in the host window.

**Idea:** Each WebContents renders into its own off-screen surface. The host app
reads these surfaces on a display-link timer and composites them into the
window. This is conceptually similar to CEF's off-screen rendering but uses
Chromium's Content API directly.

**Risk:** `CopyFromSurface()` may involve GPU readback (GPU -> CPU -> GPU),
which is slow. Need to verify whether there's a zero-copy path that yields an
IOSurface or Metal texture directly. If this is just a glorified screenshot API,
it will have the same throughput ceiling as CEF's OSR.

### Approach 3: Patch RenderWidgetHostViewCocoa visibility

Override or patch `RenderWidgetHostViewCocoa`'s NSView-level visibility
detection so that explicit `WasShown()` calls are respected regardless of the
view's position in the NSView hierarchy.

**Idea:** Find the specific code in `RenderWidgetHostViewCocoa` (or its backing
`RenderWidgetHostViewMac`) that checks NSView/NSWindow visibility and either
disable it or make it configurable. This is the most surgical fix — if the only
problem is the visibility misdetection, patching it should restore 60fps.

**Risk:** The visibility detection may exist for good reason (power savings,
correctness). Disabling it could cause subtle rendering bugs, excessive GPU
usage, or break other Chromium features that depend on accurate visibility
state.

### Approach 4: Two Shell windows, shared parent

Create two full `Shell` instances through the proper `Shell::CreateNewWindow()`
pipeline (each with its own NSWindow, proper platform delegate setup, correct
visibility tracking). Then reparent their content views into a shared parent
NSWindow.

**Idea:** Each Shell gets its own invisible NSWindow for Chromium's internal
bookkeeping, but the actual `RenderWidgetHostViewCocoa` NSViews are moved to a
visible parent window for display. Chromium thinks each view is in its own
window (preserving the one-WebContents-per-window assumption), while the user
sees a single window.

**Risk:** Reparenting NSViews between windows may trigger the same visibility
issues as Issue 407. NSViews removed from a key window may be treated as hidden.
Need to verify whether Chromium tracks the NSView's window or the Shell's
original window for visibility purposes.

### Approach 5: DelegatedFrameHost / IOSurface extraction

Intercept the compositor output at the `DelegatedFrameHost` or `ui::Compositor`
level to get each WebContents' rendered frames as IOSurfaces before they reach
the NSView, then composite both IOSurfaces into a single Metal render pass.

**Idea:** This is the "proper" version of off-screen compositing — instead of
screenshotting via `CopyFromSurface()`, tap into the compositor's frame
submission pipeline to get zero-copy access to the rendered surfaces. This is
how Chrome's tab preview thumbnails and picture-in-picture work internally.

**Risk:** The compositor internals (`viz`, `DelegatedFrameHost`,
`FrameSinkManager`) are complex and poorly documented for external use. This
approach requires deep understanding of Chromium's GPU compositing pipeline.

## How Electron Solves This

Electron faces the exact same problem — multiple WebContents in a single
BrowserWindow — and solves it with **three coordinated patches to Chromium**
that disable visibility throttling at every layer of the rendering pipeline.
A single-layer fix (like our Experiment 2's `WasShown()` call) is insufficient
because Chromium has three independent throttling systems that all must be
bypassed.

### Layer 1: RenderWidgetHost — `disable_hidden.patch`

Adds a `disable_hidden_` flag to `RenderWidgetHostImpl`. When set,
`WasHidden()` becomes a no-op — the widget never enters the hidden state:

```cpp
void RenderWidgetHostImpl::WasHidden() {
  if (disable_hidden_)
    return;  // Skip hidden-state processing entirely
  // ...
}
```

This is the layer our Experiment 2 tried to address with `WasShown()`, but
`WasShown()` can be overridden by subsequent `WasHidden()` calls from the
NSView-level visibility tracker. Electron's approach is more robust — it
prevents `WasHidden()` from taking effect at all.

### Layer 2: Blink scheduler — `allow_disabling_blink_scheduler_throttling_per_renderview.patch`

Adds `SetSchedulerThrottling()` to `WebViewImpl`. When disabled, the Blink
page scheduler is forced to treat the page as visible regardless of actual
visibility state:

```cpp
void WebViewImpl::SetPageLifecycleStateInternal(...) {
  if (!scheduler_throttling_allowed_)
    new_state->visibility = PageVisibilityState::kVisible;
  // ...
}
```

Even if Layer 1 failed, this would prevent the Blink scheduler from throttling
`requestAnimationFrame` callbacks and timer wake-ups.

### Layer 3: GPU compositor — `fix_disabling_background_throttling_in_compositor.patch`

Patches `ui::Compositor::SetVisible()` so that `SetDisplayVisible()` always
passes `true` when background throttling is disabled:

```cpp
void Compositor::SetVisible(bool visible) {
  if (display_private_)
    display_private_->SetDisplayVisible(
        background_throttling_ ? visible : true);
}
```

This prevents the viz DisplayScheduler from unsubscribing from vsync — the
mechanism that directly controls BeginFrame delivery.

### How Electron wires it up

Electron exposes a `backgroundThrottling` property on each WebContents. When
set to `false`, all three layers are activated:

```cpp
void WebContents::SetBackgroundThrottling(bool allowed) {
  background_throttling_ = allowed;
  rwh_impl->disable_hidden_ = !allowed;                          // Layer 1
  web_contents()->GetRenderViewHost()->SetSchedulerThrottling(allowed);  // Layer 2
  // Layer 3 handled by compositor patch
  if (rwh_impl->IsHidden())
    rwh_impl->WasShown({});  // Force visible immediately
}
```

### View composition

Electron uses Chromium's **`views` framework** for the actual layout — a
`views::Widget` (the window) with `views::WebView` children (one per
WebContents). But the `views` framework alone does not solve the framerate
problem. The three patches are what prevent throttling. The `views` framework
just handles layout, resize, and compositing of the already-rendering surfaces.

### macOS-specific patches

- **`disable_compositor_recycling.patch`** — Prevents expensive compositor
  destruction when a view is hidden but still attached to the window hierarchy.
  Without this, hiding and reshowing a WebContents causes a full compositor
  rebuild.
- **`revert_code_health_clean_up_stale_macwebcontentsocclusion.patch`** —
  Restores macOS occlusion detection that Chrome removed but Electron needs.

### What this means for us

Our Experiment 2 (`WasShown()` only) addressed one layer out of three, which
is why it failed. Electron's approach confirms that all three throttling systems
must be disabled simultaneously. The patches are surgical and well-defined —
we can apply the same pattern to our Chromium fork.

## Approach Selection

**Approach 3 (patch visibility), informed by Electron's three-layer pattern.**
Apply Electron's three patches to our Chromium fork:

1. Add `disable_hidden_` to `RenderWidgetHostImpl` and set it on our
   WebContents instances.
2. Add `SetSchedulerThrottling()` to `WebViewImpl` and disable it for our
   WebContents.
3. Patch `ui::Compositor::SetVisible()` to bypass `SetDisplayVisible(false)`
   when throttling is disabled.

This is the proven path — Electron ships this to millions of users. The patches
are small, targeted, and don't require restructuring the app.

Additionally, adopt Electron's use of the **`views` framework** for view
composition. Replace the raw NSView manipulation from Issue 407's experiments
with `views::Widget` + `views::WebView` children. This gives proper layout,
resize, and compositing through Chromium's own channels.

If the three-patch approach works (high confidence given Electron's track
record), further approaches are unnecessary.

## Relationship to Other Issues

| Issue   | Relationship                                                                      |
| ------- | --------------------------------------------------------------------------------- |
| 325-350 | Proved CEF's off-screen rendering caps at ~31fps on macOS                         |
| 403     | Proved IOSurface compositing at 60fps with colored rectangles                     |
| 406     | Proved multiple profiles work in one Chromium process                             |
| 407     | Proved multi-profile in practice; identified visibility throttling as the blocker |
| 408     | This issue -- solves the framerate problem for multi-profile rendering            |
