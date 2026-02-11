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
