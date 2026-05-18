+++
status = "closed"
opened = "2026-02-11"
closed = "2026-02-11"
+++

# Issue 410.2: Two Profiles at 60fps

## Goal

Get the Two Profiles app rendering both panes at 60fps with profile isolation.
The app exists and builds. The throttling patches are applied. The bypass calls
just aren't reaching the right objects at the right time.

## Background

### The Two Profiles app

A minimal Content API embedder at `content/two_profiles/` in the Chromium fork.
Creates two `ShellBrowserContext` instances with different storage paths and
displays two `WebContents` side by side in one NSWindow. Each profile gets
isolated cookies, localStorage, and cache. Built by `autoninja` as a macOS
`.app` bundle.

### The throttling problem

Issue 407 built the first version. Profile isolation worked — each pane showed a
different localStorage string. But the second pane (and sometimes both) rendered
at 2-3fps instead of 60fps.

Issue 408 traced this to three independent throttling layers in Chromium's
rendering pipeline:

1. **RenderWidgetHost visibility.** `RenderWidgetHostImpl::WasHidden()` is
   called when macOS occlusion detection marks a WebContents as hidden. In our
   side-by-side layout, the second WebContents doesn't own a top-level NSWindow,
   so it gets marked hidden.

2. **Blink scheduler throttling.** `PageSchedulerImpl` independently tracks page
   visibility. Even if Layer 1 is bypassed, Blink throttles
   `requestAnimationFrame` to ~1fps for pages it considers background.

3. **Compositor vsync.** `ui::Compositor` unsubscribes from vsync for hidden
   views, stopping `BeginFrame` signals entirely.

### What Issue 410 accomplished

Applied three Electron patches to Chromium `146.0.7650.0` that add bypass APIs
for each throttling layer:

- `disable_hidden_` flag on `RenderWidgetHostImpl` (Layer 1)
- `SetSchedulerThrottling(bool)` on `RenderViewHost` / `WebViewImpl` (Layer 2)
- `SetBackgroundThrottling(bool)` on `ui::Compositor` (Layer 3)

Content Shell builds and runs at 60fps on the patched Chromium. The Two Profiles
app builds and launches. But the bypass calls have no effect — both panes still
render at 2-3fps.

### Why the bypass didn't work

The bypass calls run during `InitializeMessageLoopContext()`, immediately after
creating the WebContents. Three problems:

1. **Wrong RenderWidgetHostImpl.** Navigation is asynchronous. The
   `RenderWidgetHostImpl` that exists at creation time is a placeholder. When
   navigation commits, a new renderer process is created with a new
   `RenderWidgetHostImpl`. The `disable_hidden_ = true` we set on the original
   instance is lost.

2. **Mojo messages lost.** `SetSchedulerThrottling(false)` sends a Mojo IPC to
   the renderer's `WebViewImpl`. If the renderer process hasn't started yet, the
   Mojo pipe doesn't exist and the message has nowhere to go.

3. **Layer 3 never called.** The compositor patch adds `SetBackgroundThrottling`
   to `ui::Compositor`, but our code never calls it. Additionally, content_shell
   on macOS uses native Cocoa views (NSView), not the aura/views compositor — so
   this API may not be relevant on macOS at all.

## Architecture Notes

### macOS rendering path

Content_shell on macOS uses native Cocoa views. Each `WebContents` has an
`RenderWidgetHostViewMac` backed by an NSView. This is different from Linux and
ChromeOS, which use aura (`RenderWidgetHostViewAura`) and the `ui::Compositor`.

This distinction matters because:

- The Layer 1 patch modifies both `RenderWidgetHostImpl` (cross-platform) and
  `RenderWidgetHostViewAura::HideImpl()` (aura-only). On macOS, the aura code
  path is not used. The cross-platform `WasHidden()` bypass should still work,
  but there may be macOS-specific visibility code in `RenderWidgetHostViewMac`
  that also needs attention.

- The Layer 3 patch modifies `ui::Compositor`, which is part of the aura/views
  system. On macOS with native views, there is no `ui::Compositor` per view.
  This entire layer may be irrelevant for our case.

### Renderer process lifecycle

When a `WebContents` is created and navigated:

1. `WebContents::Create()` creates the WebContents with a placeholder
   `RenderWidgetHost`.
2. `LoadURLWithParams()` starts the navigation.
3. Chromium creates a speculative `RenderFrameHost` with a new
   `RenderWidgetHost` for the navigation.
4. When the navigation commits, the speculative frame becomes the active frame.
   The old `RenderWidgetHost` is discarded.

Any flags set on the pre-navigation `RenderWidgetHost` are lost at step 4.

### How Electron uses these APIs

Electron's patches add the bypass APIs to Chromium. Electron's own code (in
`electron/shell/browser/`) calls them. Understanding Electron's call sites will
reveal the correct lifecycle hooks and timing. Key questions:

- When does Electron set `disable_hidden_`? On what object, at what lifecycle
  point?
- When does Electron call `SetSchedulerThrottling(false)`? Does it use a
  `WebContentsObserver` or some other mechanism?
- Does Electron use `SetBackgroundThrottling` on macOS, or is it Linux/Windows
  only?

### macOS occlusion detection

macOS tracks window and view visibility through `NSWindow` occlusion state.
Chromium's macOS integration responds to these signals to throttle hidden
content. Relevant code paths:

- `NativeWidgetMacNSWindowHost` observes
  `NSWindowDidChangeOcclusionStateNotification`
- `RenderWidgetHostViewMac` may have its own visibility tracking
- The `WebContents` visibility API (`WasShown()` / `WasHidden()`) is called from
  these macOS-specific observers

The Electron patches bypass throttling at the Chromium level, but we need to
understand which macOS-specific code paths trigger the throttling in the first
place.

## Ideas

### 1. Set bypass flags after renderer is ready

Use a `WebContentsObserver` to hook `RenderViewReady()` or
`RenderFrameCreated()`. Set `disable_hidden_` and call
`SetSchedulerThrottling(false)` in the callback, when the renderer process
exists and the correct `RenderWidgetHostImpl` is active.

### 2. Study Electron's call sites

Read Electron's source code in `shell/browser/` to find where it calls
`disable_hidden_`, `SetSchedulerThrottling`, and `SetBackgroundThrottling`.
Replicate the same lifecycle hooks and timing.

### 3. Test each layer independently

Isolate each throttling layer to determine which ones are actually responsible
for the 2-3fps on macOS. The three layers are independent — we can
enable/disable each bypass individually and measure the framerate to see which
ones matter.

### 4. Investigate RenderWidgetHostViewMac

Read the macOS-specific `RenderWidgetHostViewMac` code to understand how it
handles visibility. There may be macOS-specific throttling that the three
Electron patches don't address, or there may be a simpler place to intervene.

### 5. Force visibility at the WebContents level

Instead of bypassing individual throttling layers, force the `WebContents` to
report itself as visible. `WebContents::WasShown()` is a public API that tells
all layers the content is visible. If we can call it at the right time and
prevent it from being overridden, all three layers might be bypassed at once.

### 6. Bypass macOS occlusion detection

If macOS occlusion detection is the root trigger that tells Chromium to
throttle, intercepting or overriding the occlusion state for our window might
solve the problem at the source rather than patching each downstream throttling
layer.

## Research Plan

### Question 1: What triggers the throttling?

We don't know what calls `Hide()` or `WasOccluded()` on our second WebContents.
Add logging to `RenderWidgetHostViewMac::Hide()`,
`RenderWidgetHostViewMac::WasOccluded()`, and
`RenderWidgetHostImpl::WasHidden()` to capture stack traces when they're called.
This reveals the exact trigger.

**Where to look:**

- `content/browser/renderer_host/render_widget_host_view_mac.mm` — `Hide()` and
  `WasOccluded()`
- `content/browser/renderer_host/render_widget_host_impl.cc` — `WasHidden()`

### Question 2: Does the RenderWidgetHostImpl get replaced?

Our hypothesis says the `RenderWidgetHostImpl` we set `disable_hidden_` on is a
placeholder that gets replaced during navigation. Verify this by logging the
pointer address of the `RenderWidgetHostImpl` at two points: (a) when we set
`disable_hidden_` in `InitializeMessageLoopContext()`, and (b) after navigation
completes (via a `WebContentsObserver::DidFinishNavigation()` hook). If the
addresses differ, the hypothesis is confirmed.

### Question 3: Does RenderFrameCreated fire at the right time?

Electron uses `RenderFrameCreated()` as its hook. Add a `WebContentsObserver` to
both WebContents that logs when `RenderFrameCreated()` fires and what
`RenderWidgetHostImpl` is active at that point. If the instance is different
from what we set during initialization, we've found the fix.

### Question 4: Does WasShown() bypass everything?

Electron's `SetBackgroundThrottling()` calls `rwh_impl->WasShown({})` if the
widget is currently hidden. This is a brute-force approach: instead of
preventing individual throttling layers, just tell the widget it's visible
again. Test whether calling `WasShown({})` from a `RenderFrameCreated` observer
restores 60fps.

### Question 5: Does BrowserCompositorMac need its own bypass?

The `disable_hidden_` flag prevents `RenderWidgetHostImpl::WasHidden()` but
doesn't prevent `BrowserCompositorMac::SetRenderWidgetHostIsHidden(true)`, which
is called by `RenderWidgetHostViewMac::WasOccluded()`. If the compositor
transitions to `HasNoCompositor`, no frames are produced regardless of the
renderer state. Check whether the BrowserCompositorMac is in `HasNoCompositor`
state for the throttled WebContents.

**Where to look:**

- `content/browser/renderer_host/browser_compositor_view_mac.mm` —
  `SetRenderWidgetHostIsHidden()` and `UpdateState()`

## Research Findings

### How Electron actually calls the bypass APIs

Electron's call sites are in `shell/browser/api/electron_api_web_contents.cc`.
The critical pattern is `HandleNewRenderFrame()`, called from the
`RenderFrameCreated()` WebContentsObserver hook:

```cpp
void WebContents::HandleNewRenderFrame(
    content::RenderFrameHost* render_frame_host) {
  auto* rwhv = render_frame_host->GetView();
  if (!rwhv)
    return;

  if (!background_throttling_)
    render_frame_host->GetRenderViewHost()->SetSchedulerThrottling(false);

  auto* rwh_impl =
      static_cast<content::RenderWidgetHostImpl*>(rwhv->GetRenderWidgetHost());
  if (rwh_impl)
    rwh_impl->disable_hidden_ = !background_throttling_;
}
```

This is called **after** the renderer process exists and the correct
`RenderWidgetHostImpl` is active — not during initialization. This confirms the
timing hypothesis: our bypass calls run too early and hit placeholder objects
that get replaced when navigation commits.

Electron also has `SetBackgroundThrottling(bool)` which can be called at any
time. When called, it:

1. Sets `disable_hidden_` on the current `RenderWidgetHostImpl`
2. Calls `SetSchedulerThrottling(allowed)` on the current `RenderViewHost`
3. If the widget is currently hidden, calls `WasShown({})` to force it visible
4. Calls `owner_window_->UpdateBackgroundThrottlingState()` which reaches the
   compositor

### Layer 3 is irrelevant on macOS

The Layer 3 patch modifies `ui::Compositor`, which is part of the aura/views
system used on Linux and Windows. On macOS, content_shell uses
`BrowserCompositorMac` instead, which has its own state machine:

- `BrowserCompositorMac::SetRenderWidgetHostIsHidden(true)` transitions to
  `HasNoCompositor` — completely stops frame production
- `BrowserCompositorMac::SetRenderWidgetHostIsHidden(false)` transitions to
  `HasOwnCompositor` — full compositor running

This state machine is controlled by `RenderWidgetHostViewMac::WasOccluded()`
(which calls `browser_compositor_->SetRenderWidgetHostIsHidden(true)`) and
`ShowWithVisibility()` (which calls
`browser_compositor_->SetViewVisible(true)`).

The Electron Layer 3 patch (`ui::Compositor::SetBackgroundThrottling`) has no
effect on macOS content_shell. The macOS equivalent would be to prevent
`BrowserCompositorMac` from transitioning to `HasNoCompositor`.

### The full macOS throttling chain

When a WebContents is hidden on macOS, the chain is:

1. `RenderWidgetHostViewMac::Hide()` sets `is_visible_ = false`
2. → Calls `WasOccluded()`
3. → Calls `host()->WasHidden()` — sets `is_hidden_ = true` on
   `RenderWidgetHostImpl`, sends Mojo `WasHidden()` to renderer
4. → Calls `browser_compositor_->SetRenderWidgetHostIsHidden(true)`
5. → `UpdateState()` → `TransitionToState(HasNoCompositor)`
6. → `DelegatedFrameHost::DetachFromCompositor()` and `WasHidden()`
7. → Renderer stops receiving `BeginFrame` messages
8. → No compositor running in browser process

The `disable_hidden_` flag (Layer 1) intercepts step 3 by making
`RenderWidgetHostImpl::WasHidden()` a no-op. But steps 4-6 happen in
`RenderWidgetHostViewMac::WasOccluded()`, which is called directly by
`RenderWidgetHostViewMac::Hide()` — independent of `WasHidden()`. So even with
`disable_hidden_` set, the BrowserCompositorMac may still shut down.

### Who calls Hide() on our second WebContents?

This is still unknown. Candidates:

- `WebContentsImpl` calling `Hide()` during view attachment or navigation
- macOS occlusion detection marking the view as hidden
- The NSView not being properly added to the window's view hierarchy before
  visibility is evaluated

## Experiments

### Experiment 1: RenderFrameCreated observer with force-show

#### Hypothesis

The bypass calls in Phase 4 failed because they ran during
`InitializeMessageLoopContext()`, before renderer processes exist. The
`RenderWidgetHostImpl` set at that point is a placeholder that gets replaced
when navigation commits. Electron solves this by setting bypass flags in a
`RenderFrameCreated()` WebContentsObserver hook, which fires after the renderer
is alive and the correct RenderWidgetHostImpl is active.

#### Design

1. Create a `ThrottleBypassObserver` class that implements `WebContentsObserver`
2. Override `RenderFrameCreated(RenderFrameHost*)`:
   - Get the `RenderWidgetHostView` from the render frame host
   - Cast to `RenderWidgetHostImpl` and set `disable_hidden_ = true`
   - Call `SetSchedulerThrottling(false)` on the render view host
   - If the RenderWidgetHostImpl is currently hidden (`IsHidden()`), call
     `WasShown({})` to force it back to visible state
3. Override `RenderFrameHostChanged(old_host, new_host)` to repeat the bypass on
   the new host (handles cross-origin navigations)
4. Attach one observer to each WebContents in `InitializeMessageLoopContext()`
5. Remove the existing bypass calls from `InitializeMessageLoopContext()` — the
   observer handles timing

#### What this tests

- Whether `RenderFrameCreated` fires at the right lifecycle point (Q3)
- Whether `WasShown({})` reverses the hiding done by `Hide()` (Q4)
- Whether `disable_hidden_` prevents re-hiding once set on the correct instance

#### What this does NOT test

- Whether the `BrowserCompositorMac` also needs to be re-shown (Q5). The
  `WasShown({})` call on `RenderWidgetHostImpl` does not touch the compositor.
  If the compositor has transitioned to `HasNoCompositor`, frames still won't be
  produced. If this experiment fails, the next step is to also call
  `ShowWithVisibility()` on the `RenderWidgetHostViewMac` to re-show the
  compositor.

#### Files to modify

- `content/two_profiles/two_profiles_main_parts.h` — add
  `ThrottleBypassObserver` class declaration and observer member pointers
- `content/two_profiles/two_profiles_main_parts.mm` — implement the observer,
  attach to both WebContents, remove old bypass calls

#### Expected result

Both panes at 60fps. If only one pane improves, or if fps increases but doesn't
reach 60, the BrowserCompositorMac bypass is likely needed as a follow-up.

#### Result: FAILED

Both panes still render at 2-3fps. The `RenderFrameCreated` observer fires and
sets the bypass flags, but the framerate does not improve.

#### Conclusion

Fixing the timing of `disable_hidden_` and `SetSchedulerThrottling(false)` is
not sufficient. The throttling is not caused (only) by the RenderWidgetHostImpl
being hidden — the `BrowserCompositorMac` is the likely bottleneck.

On macOS, `RenderWidgetHostViewMac::Hide()` calls `WasOccluded()`, which does
two things independently:

1. `host()->WasHidden()` — blocked by `disable_hidden_` (Layer 1) ✓
2. `browser_compositor_->SetRenderWidgetHostIsHidden(true)` — NOT blocked ✗

Step 2 transitions the BrowserCompositorMac to `HasNoCompositor`, which detaches
the DelegatedFrameHost and stops all frame production. Even though
`disable_hidden_` prevents the renderer from being told it's hidden, the
compositor on the browser side shuts down entirely. No BeginFrame signals are
sent, so the renderer has nothing to respond to.

The `WasShown({})` call on `RenderWidgetHostImpl` only reverses step 1 — it does
not touch the BrowserCompositorMac. To reverse step 2, we need to call
`ShowWithVisibility()` on the `RenderWidgetHostViewMac`, which calls
`browser_compositor_->SetViewVisible(true)` and restarts the compositor.

Alternatively, `Hide()` itself may be the wrong level to intervene. We still
don't know who calls `Hide()` or when. It may be called before or after
`RenderFrameCreated` fires — and if it's called after, our bypass flags get set
but the compositor is shut down immediately afterward.

#### Ideas for Experiment 2

1. **Call `ShowWithVisibility()` on the RenderWidgetHostViewMac** from the
   observer, after setting bypass flags. This directly re-shows the compositor.
   The `RenderWidgetHostViewMac` is accessible via
   `render_frame_host->GetView()` which returns a `RenderWidgetHostView*` — on
   macOS this is a `RenderWidgetHostViewMac*`.

2. **Add logging to trace the full sequence** — log when `Hide()`,
   `WasOccluded()`, `WasHidden()`, `RenderFrameCreated`, and our `ApplyBypass`
   are called, with timestamps. This reveals whether `Hide()` is called before
   or after our observer fires, and whether it's called repeatedly.

3. **Prevent `Hide()` from calling the compositor shutdown** — modify
   `RenderWidgetHostViewMac::WasOccluded()` to skip
   `browser_compositor_->SetRenderWidgetHostIsHidden(true)` when
   `disable_hidden_` is set. This is a Chromium patch rather than an app-level
   fix.

### Experiment 2: Log the call sequence and re-show the view

#### Hypothesis

Experiment 1 failed because `BrowserCompositorMac` transitions to
`HasNoCompositor` independently of the `disable_hidden_` flag. But we don't know
the exact sequence — whether `Hide()` is called before or after
`RenderFrameCreated`, whether it's called once or repeatedly, or what triggers
it. This experiment adds logging to answer those questions, and simultaneously
tries re-showing the view from the observer to restart the compositor.

#### Design

Two changes in one build:

**Part A: Logging.** Add `LOG(ERROR)` lines to trace the exact call sequence
with timestamps. No stack traces (too noisy) — just method name, pointer
address, and key state.

Add logging to:

- `RenderWidgetHostViewMac::Hide()` — log `this` and `host()` pointers
- `RenderWidgetHostViewMac::WasOccluded()` — log whether `host()->IsHidden()`
- `RenderWidgetHostViewMac::ShowWithVisibility()` — log when the view is shown
- `RenderWidgetHostImpl::WasHidden()` — log whether `disable_hidden_` is set
- `ThrottleBypassObserver::ApplyBypass()` — already has logging from Experiment
  1

This reveals the ordering: does `Hide()` fire before or after
`RenderFrameCreated`? Is it called once or many times? Does our observer's
`WasShown()` call get immediately undone by a subsequent `Hide()`?

**Part B: Re-show the view.** After setting bypass flags in `ApplyBypass()`,
call `ShowWithVisibility(PageVisibilityState::kVisible)` on the
`RenderWidgetHostView`. This is a method on the base class
(`RenderWidgetHostView`), so no macOS-specific cast is needed. It calls through
to `RenderWidgetHostViewMac::ShowWithVisibility()`, which:

1. Sets `is_visible_ = true`
2. Calls `browser_compositor_->SetViewVisible(true)`
3. Calls `OnShowWithPageVisibility(kVisible)` — sends visibility to renderer

This directly restarts the compositor, which Experiment 1's `WasShown({})` on
the RenderWidgetHostImpl did not do.

#### What this tests

- The exact call sequence of `Hide()`, `WasOccluded()`, `RenderFrameCreated`,
  and `ApplyBypass` (Q1, via logging)
- Whether re-showing the view from the observer restarts the compositor and
  restores 60fps (Q5)
- Whether `Hide()` is called again after our re-show, undoing the fix

#### What determines success or failure

- **60fps:** The `ShowWithVisibility` call restarts the compositor. Logging
  reveals the sequence for documentation.
- **Still 2-3fps, Hide() called after ApplyBypass:** `Hide()` is called after
  our observer fires, immediately re-hiding the view. The fix is to prevent
  `Hide()` at the source (Experiment 3: patch `WasOccluded()`).
- **Still 2-3fps, Hide() called before ApplyBypass:** Our re-show works
  momentarily but something else re-hides it. Logging reveals the culprit.

#### Files to modify

- `content/browser/renderer_host/render_widget_host_view_mac.mm` — add logging
  to `Hide()`, `WasOccluded()`, `ShowWithVisibility()`
- `content/browser/renderer_host/render_widget_host_impl.cc` — add logging to
  `WasHidden()`
- `content/two_profiles/two_profiles_main_parts.mm` — add
  `ShowWithVisibility(kVisible)` call in `ApplyBypass()`

#### Result: FAILED

Both panes still render at 2fps.

#### Conclusion

The logging revealed a critical finding: **`Hide()`, `WasOccluded()`, and
`WasHidden()` are never called on either view.** The only log lines that appear
are from `ShowWithVisibility()` (our own bypass calls) — none of the hiding
methods fire at all.

This completely invalidates the hypothesis that throttling is caused by the
`Hide() → WasOccluded() → WasHidden()` chain. The entire line of investigation
from Experiments 1 and 2 — setting `disable_hidden_`, calling
`SetSchedulerThrottling(false)`, calling `WasShown({})`, calling
`ShowWithVisibility(kVisible)` — was targeting a code path that is not being
triggered.

The 2fps framerate is caused by something else entirely. 2fps is not a vsync
divisor (not 30fps, not 15fps) — it looks like a timer-based fallback, such as
`requestAnimationFrame` falling back to ~500ms intervals when the compositor is
not driving frames via vsync/display link.

#### Ideas for Experiment 3

1. **Investigate the CADisplayLink / vsync connection.** On macOS, the
   compositor drives frames via `CADisplayLink` (or `CVDisplayLink` on older
   versions). The second WebContents is created via `WebContents::Create()` and
   manually added as an NSView subview — it may never have been properly
   connected to the window's display link. But Shell A (created via
   `Shell::CreateNewWindow()`) is also at 2fps, so the issue is systemic, not
   specific to the second view.

2. **Check BrowserCompositorMac state directly.** Add logging to
   `BrowserCompositorMac::UpdateState()` to see what state both compositors are
   in. If they're in `HasNoCompositor`, frames can't be produced regardless of
   renderer state — but we need to understand how they got there without
   `Hide()`/`WasOccluded()` being called.

3. **Compare with Content Shell (single profile).** Content Shell runs at 60fps
   on the same Chromium build. Diff the initialization code paths to find what
   Two Profiles does differently that causes both views to run at 2fps.

4. **Check NSWindow/NSView visibility and occlusion.** macOS might be reporting
   the window or views as occluded through a different path than
   `RenderWidgetHostViewMac::Hide()`. Log `NSWindow.occlusionState` and the
   NSView's `isHiddenOrHasHiddenAncestor` to see if macOS thinks the views are
   invisible.

5. **Try creating both WebContents via Shell::CreateNewWindow().** The current
   approach creates Shell A normally and WebContents B manually. Try creating
   two Shells instead, to see if the Shell infrastructure provides something
   essential that the manual approach misses.

## Conclusion

### What we learned

Experiments 1 and 2 targeted the wrong code path. The three Electron patches we
applied (`disable_hidden_`, `SetSchedulerThrottling`, `SetBackgroundThrottling`)
intercept `Hide()` / `WasOccluded()` / `WasHidden()` — but Experiment 2's
logging proved **none of those methods are ever called** on either view. The
throttling patches are irrelevant to our problem.

The root cause is a **visibility race condition** in the compositor lifecycle.

### The race condition

When a `WebContents` is created and its `WebContentsViewCocoa` is added to a
window, macOS fires `viewDidMoveToWindow`, which triggers
`WebContentsImpl::UpdateWebContentsVisibility(VISIBLE)`. This calls
`WasShown()`, which tries to call `ShowWithVisibility(kVisible)` on the
`RenderWidgetHostViewMac` — the method that transitions the
`BrowserCompositorMac` from `HasNoCompositor` (dead) to `HasOwnCompositor`
(alive).

But in our Two Profiles app, the view is added to the window **before the
renderer process exists**. `GetRenderWidgetHostView()` returns null, so
`ShowWithVisibility` is never called. The visibility signal is consumed
(`did_first_set_visible_ = true`, `visibility_ = VISIBLE`), and when the
renderer is later created, the `BrowserCompositorMac` starts in
`HasNoCompositor` state and nothing transitions it out. No `BeginFrame` signals
are sent, no frames are produced, and `requestAnimationFrame` falls back to a
~2fps timer.

Content Shell avoids this because `Shell::CreateNewWindow` creates the renderer
and adds the view in the right order — by the time `viewDidMoveToWindow` fires,
the `RenderWidgetHostView` exists and receives `ShowWithVisibility`.

### Why the ThrottleBypassObserver can't fix it

Our `ThrottleBypassObserver` calls `ShowWithVisibility(kVisible)` from
`RenderFrameCreated`, which fires after the renderer exists. But
`ShowWithVisibility` calls `OnShowWithPageVisibility`, which only calls
`NotifyHostAndDelegateOnWasShown()` — the method that actually transitions the
compositor — **if `host_->IsHidden()` is true**. Since `WasShown()` was already
called on the `WebContentsImpl` (step consuming the signal), the host may not
report as hidden, so the compositor transition is skipped.

### Additional Electron patches

Electron has at least two more relevant patches beyond the three we applied:

1. **`disable_compositor_recycling.patch`** — Prevents `BrowserCompositorMac`
   from transitioning to `HasNoCompositor` when the view is attached to a
   window. Modifies `RenderWidgetHostViewMac::WasOccluded()` to only shut down
   the compositor for unattached views.

2. **`revert_code_health_clean_up_stale_macwebcontentsocclusion.patch`** —
   Restores legacy macOS occlusion detection using `NSWindow.occlusionState`
   directly, instead of Chromium's newer `WebContentsOcclusionCheckerMac`.

Electron also explicitly manages visibility from its own `BrowserWindow::Show()`
by calling `web_contents()->WasShown()` directly, rather than relying solely on
macOS `viewDidMoveToWindow` notifications.

### What needs to happen next

The fix is not more throttling bypasses — it's ensuring the
`BrowserCompositorMac` transitions to `HasOwnCompositor` for both views.
Possible approaches:

1. Reorder operations so the renderer exists before the view enters the window
   hierarchy.
2. Explicitly call `SetRenderWidgetHostIsHidden(false)` on the
   `BrowserCompositorMac` after the renderer is created.
3. Apply the `disable_compositor_recycling` patch and test whether it changes
   the initial compositor state.
4. Use `Shell::CreateNewWindow` for both profiles to get the correct lifecycle.
5. Force a visibility re-notification after the renderer is ready, bypassing the
   `did_first_set_visible_` guard.
