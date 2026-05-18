+++
status = "closed"
opened = "2026-02-23"
closed = "2026-02-23"
+++

# Issue 629: Understand Navigation Blank in CALayerHost

## Goal

Understand **why** the browser overlay disappears for ~10 seconds when the user
clicks a link. This is a research issue — the goal is diagnosis, not a fix.

## Background

### The CALayerHost migration

[Issue 625](625-calayerhost.md) replaced the `FrameSinkVideoCapturer` pipeline
with `CALayerHost`. Instead of capturing IOSurface frames at 120fps and
transferring Mach ports over XPC every frame, Chromium now sends a
`ca_context_id` (uint32) once per tab. The GUI creates a `CALayerHost` sublayer,
and Window Server composites the remote content directly from GPU VRAM.

This migration broke several things that worked under the old pipeline:

- [Issue 626](626-x-y-calayerhost.md) — X/Y positioning was offset. Fixed.
- [Issue 627](627-resize-calayerhost.md) — Resize stopped working. Fixed.
- [Issue 628](628-navigation-calayerhost.md) — Navigation causes a ~10s blank.
  **Unresolved after 8 experiments.**

Under the old IOSurface pipeline, navigation was invisible — every frame
delivered a new Mach port, and the Metal shader re-read the texture every frame
in `drawFrame()`. The new surface just showed up. With CALayerHost, there is no
per-frame update. The `ca_context_id` is set once, and Window Server composites
from that context. When navigation produces a new `CAContext`, the old one
becomes invalid and the overlay goes blank.

There may be additional regressions from the CALayerHost migration that haven't
been tested yet.

### What Issue 628 tried and failed

Issue 628 ran 8 experiments targeting the Chromium-side pipeline. All failed to
fix the ~10-second blank:

| Exp | Approach                                               | Result             |
| --- | ------------------------------------------------------ | ------------------ |
| 1   | Re-register callback on view swap, replace CALayerHost | No effect on blank |
| 2   | Re-apply size in `RenderViewHostChanged`               | Fail               |
| 3   | Research: Electron/Chromium sizing                     | Research only      |
| 4   | Resize NSWindow instead of `view->SetSize()`           | No effect on blank |
| 5   | Research: navigation transitions, dedup gate           | Research only      |
| 6   | Set fallback surface before navigation                 | Fail               |
| 7   | Diagnostic logging                                     | Research only      |
| 8   | Reduce dedup gate to 100ms                             | Fail               |

Key finding from Experiment 7's diagnostic logging: Chromium sends the new
`ca_context_id` within 100ms of the click. The page loads in ~70ms. The GUI
receives the ID and replaces the `CALayerHost` immediately. Yet the new
`CALayerHost` shows nothing for ~10 seconds.

All code changes from Issue 628 should be reverted — none had any effect.

### What we know

1. **Chromium is fast.** The new `ca_context_id` arrives in ~100ms. The page
   loads in ~70ms.
2. **The GUI is fast.** The `CALayerHost` is replaced immediately upon receiving
   the new ID.
3. **The blank is ~10 seconds.** Suspiciously consistent.
4. **The problem is NOT:** callback lifecycle, compositor surface fallback,
   dedup gate timing, NSWindow sizing, or `SetSize()` vs `setContentSize:`.
5. **The problem likely IS:** something in the Window Server's handling of
   cross-process CAContext/CALayerHost connections, or something about how the
   hidden NSWindow interacts with CAContext compositing.

### What we don't know

- Does the blank happen with a **visible** Chromium window? If not, the hidden
  window is the cause.
- Is the new `CAContext` actually producing content when the `CALayerHost`
  connects to it? Or is it empty?
- Does macOS have an internal timeout for establishing cross-process CAContext
  connections (`CARemoteLayerServer` / `CARemoteLayerClient`)?
- Does `CALayerHost` need an explicit trigger (e.g., `setNeedsDisplay`,
  `CATransaction`) to start displaying a newly-connected remote context?
- Is the old `CAContext` torn down before the new one is ready, creating a gap
  where no context has content?

### Chromium branch

Start from `146.0.7650.0-issue-627` (discarding Issue 628's branch). Create
`146.0.7650.0-issue-629` if any Chromium changes are needed for diagnostics.

### GUI revert

Revert `a73f3e1` (`gui/src/renderer/Metal.zig` — CALayerHost replacement logic
from Issue 628 Experiment 1) before starting experiments.

## Experiments

### Experiment 1: Compare Electron and Chromium CALayerHost usage to TermSurf

#### Problem

We don't understand why the overlay goes blank for ~10 seconds during
navigation. Issue 628 spent 8 experiments modifying the Chromium pipeline with
no effect. Before trying more fixes, we need to understand how our CALayerHost
usage differs from the working implementations.

#### Research questions

**R1: Does Electron use CALayerHost at all?**

How does Electron's off-screen rendering display compositor output on macOS?
Does it create a `CALayerHost` with a `ca_context_id`, or does it use a
completely different mechanism?

**R2: How does normal Chromium use CALayerHost?**

In a standard Chrome window, how does `DisplayCALayerTree` manage the
`CALayerHost`? What is the layer tree structure? What happens during navigation
when the `ca_context_id` changes?

**R3: How does TermSurf use CALayerHost?**

Trace the full pipeline: Chromium Profile Server sends `ca_context_id` over XPC
→ GUI creates `CALayerHost`. What layer tree structure do we use? How does it
differ from Chromium's `DisplayCALayerTree`?

**R4: What are the architectural differences?**

Compare the three approaches side by side. Identify anything TermSurf does
differently that could explain the 10-second blank.

#### Results

**R1: Electron does NOT use CALayerHost.**

Electron's off-screen rendering on macOS intercepts `CALayerParams` at the
`HostDisplayClient` level and extracts the IOSurface directly — it never creates
a `CALayerHost`:

```cpp
// vendor/electron/shell/browser/osr/osr_host_display_client_mac.mm
void OffScreenHostDisplayClient::OnDisplayReceivedCALayerParams(
    const gfx::CALayerParams& ca_layer_params) {
  if (!ca_layer_params.is_empty) {
    IOSurfaceRef io_surface = IOSurfaceLookupFromMachPort(
        ca_layer_params.io_surface_mach_port.get());
    void* pixels = IOSurfaceGetBaseAddress(io_surface);
    SkBitmap bitmap;
    bitmap.installPixels(..., pixels, stride);
    callback_.Run(ca_layer_params.damage, bitmap, {});
  }
}
```

Electron reads pixels from the IOSurface Mach port on every frame. It has two
rendering paths:

1. **Hardware accelerated:** `FrameSinkVideoCapturer` (the same pipeline
   TermSurf used before Issue 625).
2. **Software:** `CALayerParams` → extract IOSurface → read pixels → SkBitmap.

Neither path involves `CALayerHost`. Electron sidesteps the entire
CAContext/CALayerHost mechanism. This means **Electron cannot tell us anything
about CALayerHost navigation behavior** — they don't use it.

**R2: Normal Chromium uses CALayerHost inside a visible window.**

In stock Chrome, `DisplayCALayerTree` (in the browser process) creates a
`CALayerHost` inside the window's NSView layer tree:

```
RenderWidgetHostViewCocoa (NSView, wantsLayer=YES)
└─ background_layer_ (CALayer, view's backing layer)
   └─ maybe_flipped_layer_ (CALayer, geometryFlipped=YES)
      └─ remote_layer_ (CALayerHost, contextId = ca_context_id)
```

Key details from `display_ca_layer_tree.mm`:

- `GotCALayerFrame()` creates a **new** `CALayerHost` when `ca_context_id`
  changes (never updates `contextId` on an existing host).
- Uses `ScopedCAActionDisabler` to suppress CALayer animations during the swap.
- Adds the new host **before** removing the old one — atomic visual swap.
- The NSView is in a **visible** window on screen.

When `SetCALayerParams()` is called on the NSView, it calls
`DisplayCALayerTree::UpdateCALayerTree()` which calls `GotCALayerFrame()`. This
happens inside `AcceleratedWidgetCALayerParamsUpdated()`.

**R3: TermSurf uses CALayerHost cross-process with a hidden intermediary.**

TermSurf's pipeline:

1. Chromium Profile Server runs with a **hidden** NSWindow
   (`[window orderOut:nil]`).
2. Inside that hidden window, the standard Chromium pipeline runs:
   `RenderWidgetHostViewCocoa` → `DisplayCALayerTree` → `CALayerHost`. This
   `CALayerHost` lives inside the hidden window and points at the GPU process's
   `CAContext`.
3. We hook `SetCALayerParamsCallback` on the `RenderWidgetHostViewMac` to
   intercept the `ca_context_id`.
4. We send the `ca_context_id` over XPC to the TermSurf GUI (a completely
   separate process).
5. The GUI creates its **own** `CALayerHost` in its Metal renderer's layer tree:

```
IOSurfaceLayer (Metal renderer)
└─ flipped_layer (geometryFlipped=YES)
   └─ positioning_layer (explicit frame at overlay grid rect)
      └─ CALayerHost (contextId = ca_context_id from XPC)
```

So there are **two CALayerHosts** pointing at the same `CAContext`:

1. One inside the Chromium Profile Server's hidden window (created by
   `DisplayCALayerTree`, standard Chromium behavior).
2. One inside the TermSurf GUI (created by `Metal.zig`).

**R4: Architectural differences.**

| Aspect               | Normal Chrome            | Electron OSR            | TermSurf                          |
| -------------------- | ------------------------ | ----------------------- | --------------------------------- |
| Mechanism            | CALayerHost              | IOSurface pixel read    | CALayerHost                       |
| CALayerHost location | Browser window (visible) | N/A                     | GUI process (visible)             |
| Intermediate window  | None                     | Placeholder NSView      | Hidden NSWindow                   |
| CALayerHost count    | 1 per CAContext          | 0                       | **2 per CAContext**               |
| Window visibility    | Visible                  | N/A                     | Hidden                            |
| Process topology     | GPU → Browser            | GPU → Browser (extract) | GPU → Server (hidden) → XPC → GUI |

Three critical differences in TermSurf:

1. **Two CALayerHosts per CAContext.** The hidden window's `DisplayCALayerTree`
   creates one, and our GUI creates another. Both point at the same `CAContext`.
   macOS may not support multiple `CALayerHost` instances for the same
   `CAContext`, or the hidden window's host may interfere with the GUI's host.

2. **Hidden intermediary window.** The Chromium Profile Server's NSWindow is
   hidden via `[window orderOut:nil]`. The `DisplayCALayerTree` inside that
   window still runs and manages its own `CALayerHost`. The Window Server may
   deprioritize or defer compositing for off-screen windows, affecting the
   `CAContext` that both hosts share.

3. **No `ScopedCAActionDisabler`.** Chromium wraps CALayerHost creation in
   `ScopedCAActionDisabler` to suppress Core Animation's implicit animations
   (fade-in, position interpolation). Our GUI code does not do this. A 0.25s
   fade-in animation wouldn't explain a 10-second blank, but other implicit
   animation behaviors might.

#### Conclusion

Electron is irrelevant — they don't use CALayerHost at all. The comparison that
matters is TermSurf vs. normal Chrome.

The most suspicious difference is **two CALayerHosts pointing at the same
CAContext**. In normal Chrome, there is exactly one `CALayerHost` per
`CAContext`. In TermSurf, the hidden window's `DisplayCALayerTree` creates one,
and the GUI creates a second. This is an untested configuration — macOS may not
properly handle it.

The next experiment should test whether eliminating the hidden window's
`CALayerHost` (by disabling `DisplayCALayerTree` or by not calling
`SetCALayerParams` on the hidden NSView) resolves the blank. Alternatively, test
whether making the hidden window visible fixes the problem — this would confirm
that window visibility affects `CAContext` compositing.

#### Verification

Research is complete when all four questions are answered and we have a concrete
hypothesis for the next experiment.

### Experiment 2: Deep dive into the CAContext lifecycle during navigation

#### Problem

Experiment 1 identified three critical differences between TermSurf and stock
Chrome, but it didn't explain _why_ the blank lasts ~10 seconds. We need to
trace the exact lifecycle of CAContext creation and destruction during
navigation, understand whether the hidden window's `DisplayCALayerTree`
interferes with our GUI's `CALayerHost`, and determine whether
`DisableDisplay()` is the key mechanism Chrome uses to avoid the problem.

#### Research questions

**R1: What is the exact CAContext lifecycle during navigation?**

When a user clicks a link, the renderer process may swap (cross-site navigation)
or the compositor may recreate its output surface (same-site). In either case, a
new `CALayerTreeCoordinator` is created in the GPU process, which allocates a
new `CAContext` via `+[CAContext contextWithCGSConnection:options:]`. This gives
a new `ca_context_id`.

Trace the full sequence:

1. Navigation commits.
2. `RenderViewHostChanged` fires (if cross-site) — or the compositor swaps the
   output surface (same-site).
3. Old `CALayerTreeCoordinator` is destroyed → old `CAContext` is released.
4. New `CALayerTreeCoordinator` is created → new `CAContext` → new
   `ca_context_id`.
5. `AcceleratedWidgetCALayerParamsUpdated()` fires with new params.
6. `ns_view_->SetCALayerParams()` → hidden window's
   `DisplayCALayerTree::GotCALayerFrame()` creates a `CALayerHost` with the new
   `ca_context_id`.
7. `ca_layer_params_callback_` sends the `ca_context_id` over XPC.
8. GUI receives it and creates its own `CALayerHost`.

Questions: Does step 6 happen before step 8? (Yes — they're in the same
function, and `SetCALayerParams` is called first.) Does the hidden window's host
"claim" the CAContext before the GUI can connect? What is the timing gap between
step 6 and step 8?

**R2: Is `DisableDisplay()` ever called in the Chromium Profile Server?**

`DisableDisplay()` on `RenderWidgetHostNSViewBridge` destroys the
`DisplayCALayerTree` and sets `display_disabled_ = true`. After that,
`SetCALayerParams()` returns early — no `CALayerHost` is created in the hidden
window.

The only call site is `RenderWidgetHostViewMac::SetParentUiLayer()`, which fires
only when a `parent_ui_layer` (Views/Aura compositor layer) is provided. The
comment at line 362 explicitly notes: "not all code has been updated to use
`ui::Views` (e.g, `content_shell`)".

Since the Chromium Profile Server is based on `content_shell`, and
`content_shell` does not use `ui::Views` / `ui::Layer` compositing,
`SetParentUiLayer()` is never called with a non-null layer. Therefore
`DisableDisplay()` is **never called** in our pipeline.

This means the hidden window's `DisplayCALayerTree` is **always active**. Every
frame, it receives `SetCALayerParams()` and maintains its own `CALayerHost`
inside the hidden window's NSView.

**R3: How does Chrome's Views UI eliminate the dual-host problem?**

In stock Chrome (not `content_shell`), the browser uses `ui::Views` for
rendering. When `SetParentUiLayer()` is called with a valid layer:

1. `DisableDisplay()` is called on the `RenderWidgetHostNSViewBridge`.
2. The `DisplayCALayerTree` is destroyed (`display_ca_layer_tree_.reset()`).
3. `display_disabled_ = true` prevents future `SetCALayerParams()` calls from
   creating any `CALayerHost` in the NSView.
4. Instead, the `ui::Compositor` (parent layer compositor) handles display. The
   `CALayerHost` lives in the Views compositor's layer tree, not in the NSView's
   `DisplayCALayerTree`.

This means in stock Chrome with Views, there is exactly **one CALayerHost per
CAContext** — the one in the Views compositor. The NSView's `DisplayCALayerTree`
is disabled. This is the normal path for Chrome on macOS.

In our Chromium Profile Server (content_shell-based), `DisableDisplay()` is
never called, so the NSView's `DisplayCALayerTree` stays alive and creates a
**second CALayerHost** for the same CAContext. This is the dual-host problem
identified in Experiment 1.

**R4: What is the exact code flow in
`AcceleratedWidgetCALayerParamsUpdated()`?**

```cpp
void RenderWidgetHostViewMac::AcceleratedWidgetCALayerParamsUpdated() {
  SetBackgroundLayerColor(last_frame_root_background_color_);
  const gfx::CALayerParams* ca_layer_params =
      browser_compositor_->GetLastCALayerParams();
  if (ca_layer_params) {
    ns_view_->SetCALayerParams(*ca_layer_params);    // (A) Hidden window host
    if (ca_layer_params_callback_)
      ca_layer_params_callback_.Run(*ca_layer_params); // (B) Our XPC callback
  }
}
```

Step (A) calls `RenderWidgetHostNSViewBridge::SetCALayerParams()` →
`DisplayCALayerTree::UpdateCALayerTree()` → `GotCALayerFrame()`. This creates a
`CALayerHost` in the hidden window with the new `ca_context_id`. Because
`display_disabled_` is false (see R2), this always runs.

Step (B) sends the `ca_context_id` over XPC to the GUI. The GUI then creates its
own `CALayerHost`.

The hidden window's `CALayerHost` is always created **before** the GUI's, in the
same function call. Both point at the same `CAContext`.

**R5: Where is the CAContext created and what happens to the old one?**

`CAContext` is created in `CALayerTreeCoordinator::CALayerTreeCoordinator()`
(`ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm:29-68`):

```cpp
ca_context_ = [CAContext contextWithCGSConnection:connection_id options:@{}];
ca_context_.layer = root_ca_layer_;
```

Each `ImageTransportSurfaceOverlayMacEGL` (GPU output surface) creates one
`CALayerTreeCoordinator`. During navigation, the old output surface is destroyed
(old `CAContext` released) and a new one is created (new `CAContext` with a new
`ca_context_id`).

The `ca_context_id` is read from `[ca_context_ contextId]` in
`CommitPresentedFrameToCA()` and sent to the browser process via the swap
completion callback.

#### Results

**R1: The CAContext lifecycle during navigation is well-defined.**

The GPU process creates one `CAContext` per output surface
(`CALayerTreeCoordinator`). Navigation destroys the old surface and creates a
new one, producing a new `ca_context_id`. The old `CAContext` is released when
its `CALayerTreeCoordinator` destructor runs.

The exact sequence is:

1. Navigation commits → `RenderViewHostChanged` fires.
2. New `RenderWidgetHostView` is created with a new `BrowserCompositorMac`.
3. GPU process creates a new `ImageTransportSurfaceOverlayMacEGL` → new
   `CALayerTreeCoordinator` → new `CAContext` → new `ca_context_id`.
4. First compositor frame arrives → `AcceleratedWidgetCALayerParamsUpdated()`
   fires.
5. `ns_view_->SetCALayerParams()` → hidden window's `DisplayCALayerTree` creates
   a `CALayerHost` with the new ID (step A).
6. `ca_layer_params_callback_` sends the ID over XPC (step B).
7. GUI receives it and creates its own `CALayerHost` (step C).

Steps A and B happen synchronously in the same function. Step C happens
asynchronously after XPC delivery (microseconds to low milliseconds). The hidden
window's host always beats the GUI's host.

**R2: `DisableDisplay()` is never called in the Chromium Profile Server.**

Confirmed. The Chromium Profile Server is content_shell-based. `content_shell`
does not use `ui::Views` or `ui::Layer` compositing. `SetParentUiLayer()` is
never called with a non-null layer. Therefore `DisableDisplay()` never fires,
and the hidden window's `DisplayCALayerTree` is always active.

This is the root of the dual-host problem: `DisplayCALayerTree` was designed to
be disabled when the Views compositor takes over. In our pipeline, nothing takes
over, so it stays active and creates a competing `CALayerHost` for every
`ca_context_id`.

**R3: Chrome's Views UI eliminates the dual-host problem via
`DisableDisplay()`.**

In stock Chrome, `SetParentUiLayer()` is called early in the view lifecycle.
This calls `DisableDisplay()`, which:

1. Destroys the `DisplayCALayerTree` (`display_ca_layer_tree_.reset()`).
2. Sets `display_disabled_ = true`.
3. Future `SetCALayerParams()` calls return early — no `CALayerHost` created.

The Views compositor handles display through its own layer tree. There is
exactly one `CALayerHost` per `CAContext` in the Views compositor, and zero in
the NSView's `DisplayCALayerTree` (because it's been destroyed).

Our Chromium Profile Server never takes this path. The hidden window's
`DisplayCALayerTree` runs for the entire lifetime of the tab, creating a
`CALayerHost` for every `ca_context_id` it receives. Our GUI creates a second
one. Two `CALayerHost` instances compete for the same `CAContext`.

**R4: The call ordering in `AcceleratedWidgetCALayerParamsUpdated()` is
confirmed.**

```
AcceleratedWidgetCALayerParamsUpdated()
  ├── ns_view_->SetCALayerParams()       // Hidden window: DisplayCALayerTree
  │   └── GotCALayerFrame()              // Creates CALayerHost in hidden window
  └── ca_layer_params_callback_()        // Our hook: sends over XPC
      └── (async) GUI creates CALayerHost // Second host, same CAContext
```

The hidden window's `CALayerHost` is created **synchronously before** our XPC
callback fires. The GUI's `CALayerHost` is created asynchronously after XPC
delivery. Both point at the same `CAContext`.

**R5: Each navigation produces a new CAContext.**

The GPU process creates `CAContext` via `+[CAContext contextWithCGSConnection:]`
in `CALayerTreeCoordinator`'s constructor. Each output surface gets one. During
navigation, old surface → destroyed (old `CAContext` released), new surface →
created (new `CAContext`, new `ca_context_id`).

The `ca_context_id` is sent to the browser in `gfx::CALayerParams` via the swap
completion callback, then forwarded to both the hidden window's
`DisplayCALayerTree` (via `SetCALayerParams`) and our XPC callback.

#### Conclusion

The dual-`CALayerHost` problem is now fully understood. In stock Chrome,
`DisableDisplay()` destroys the NSView's `DisplayCALayerTree` when the Views
compositor takes over, ensuring exactly one `CALayerHost` per `CAContext`. In
our Chromium Profile Server (content_shell-based), `DisableDisplay()` is never
called, so the hidden window's `DisplayCALayerTree` stays active and creates a
competing `CALayerHost` for every `ca_context_id`.

The strongest hypothesis is: **the hidden window's `CALayerHost` interferes with
the GUI's `CALayerHost`** because both point at the same `CAContext`. macOS
Window Server may only composite a `CAContext` to one `CALayerHost` at a time,
or may deprioritize compositing for the GUI's host because the hidden window's
host was created first.

There are two testable hypotheses for the next experiment:

1. **Call `DisableDisplay()` on the NSView bridge** — This would destroy the
   hidden window's `DisplayCALayerTree`, eliminating the competing
   `CALayerHost`. The `SetCALayerParams()` call in
   `AcceleratedWidgetCALayerParamsUpdated()` would become a no-op, and only our
   XPC callback would run. This is the most targeted fix.

2. **Make the hidden window visible** — If the Window Server deprioritizes
   compositing for off-screen windows, making the window visible might fix the
   blank without eliminating the dual host. This would distinguish between the
   "dual host" and "hidden window" hypotheses.

#### Verification

Research is complete when all five questions are answered and we have testable
hypotheses for the next experiment.

### Experiment 3: Disable the hidden window's DisplayCALayerTree

#### Problem

Experiment 2 found that the hidden window's `DisplayCALayerTree` is never
disabled in the Chromium Profile Server. In stock Chrome, `DisableDisplay()` is
called via `SetParentUiLayer()` to destroy the `DisplayCALayerTree` when the
Views compositor takes over. In our content_shell-based server, this never
happens, so the hidden window creates a competing `CALayerHost` for every
`ca_context_id`.

#### Hypothesis

The hidden window's `CALayerHost` interferes with the GUI's `CALayerHost`.
Calling `DisableDisplay()` on the `RenderWidgetHostNSViewBridge` will destroy
the hidden window's `DisplayCALayerTree`, ensuring only the GUI's `CALayerHost`
connects to each `CAContext`. This should eliminate the ~10-second blank during
navigation.

#### Chromium branch

Create `146.0.7650.0-issue-629` from `146.0.7650.0-issue-627` (discarding Issue
628's branch, which has 5 commits that all failed).

#### Changes

**File 1: `content/browser/renderer_host/render_widget_host_view_mac.h`**

Add a public method to expose `DisableDisplay()`:

```cpp
// Disable the NSView's DisplayCALayerTree so it doesn't create a competing
// CALayerHost. Used by Chromium Profile Server (Issue 629).
void DisableNSViewDisplay();
```

Add near line 110, next to the existing `SetCALayerParamsCallback()` TermSurf
addition.

**File 2: `content/browser/renderer_host/render_widget_host_view_mac.mm`**

Implement the wrapper:

```cpp
void RenderWidgetHostViewMac::DisableNSViewDisplay() {
  ns_view_->DisableDisplay();
}
```

Add near the existing `SetCALayerParamsCallback()` implementation (around line
156).

**File 3:
`content/chromium_profile_server/browser/shell_ca_layer_bridge_mac.h`**

Add a bridge function declaration:

```cpp
// Disable the NSView's DisplayCALayerTree to prevent a competing
// CALayerHost in the hidden window (Issue 629).
void DisableDisplayOnView(RenderWidgetHostView* view);
```

**File 4:
`content/chromium_profile_server/browser/shell_ca_layer_bridge_mac.mm`**

Implement the bridge:

```cpp
void DisableDisplayOnView(RenderWidgetHostView* view) {
  auto* mac_view = static_cast<RenderWidgetHostViewMac*>(view);
  mac_view->DisableNSViewDisplay();
}
```

**File 5: `content/chromium_profile_server/browser/shell_tab_observer.cc`**

Call `DisableDisplayOnView()` at the start of `RegisterCALayerParamsCallback()`,
before setting the callback:

```cpp
void ShellTabObserver::RegisterCALayerParamsCallback() {
  if (!web_contents())
    return;
  auto* view = web_contents()->GetRenderWidgetHostView();
  if (!view)
    return;

  // Disable the hidden window's DisplayCALayerTree so it doesn't create
  // a competing CALayerHost for the same CAContext (Issue 629).
  DisableDisplayOnView(view);

  // Reset deduplication so the new view's first ca_context_id always gets sent.
  last_ca_context_id_ = 0;

  SetCALayerParamsCallbackOnView(view, base::BindRepeating(...));
  // ... rest unchanged
}
```

This is called both at initial tab setup and on `RenderViewHostChanged()`, so
every new `RenderWidgetHostView` gets its `DisplayCALayerTree` disabled before
any `SetCALayerParams()` calls arrive.

#### Expected effect

After the change, `AcceleratedWidgetCALayerParamsUpdated()` becomes:

```
AcceleratedWidgetCALayerParamsUpdated()
  ├── ns_view_->SetCALayerParams()       // Returns early (display_disabled_)
  └── ca_layer_params_callback_()        // Our hook: sends over XPC
      └── (async) GUI creates CALayerHost // ONLY host for this CAContext
```

The hidden window's `DisplayCALayerTree` is destroyed. No `CALayerHost` is
created in the hidden window. Only the GUI's `CALayerHost` connects to the
`CAContext`. This matches how stock Chrome operates when the Views compositor
takes over.

#### Verification

1. Build the Chromium Profile Server
   (`autoninja -C out/Default chromium_profile_server`).
2. Launch TermSurf and open a page (e.g., `web google.com`).
3. Click a link on the page.
4. **Pass:** The new page appears within ~1 second (no 10-second blank).
5. **Fail:** The ~10-second blank persists — the dual-host hypothesis is wrong.

**Result:** Fail

The navigated page never appears at all. Worse than the original ~10-second
blank — now the overlay stays permanently blank after clicking a link. The
initial page loads fine, but after navigation, nothing renders.

`DisableDisplay()` destroys the `DisplayCALayerTree` and sets
`display_disabled_ = true`, which causes `SetCALayerParams()` to return early.
But this also means the `AcceleratedWidgetCALayerParamsUpdated()` callback path
may be disrupted — the compositor may depend on `SetCALayerParams()` completing
to continue producing frames. By disabling it, we may have broken the
compositor's feedback loop, not just the hidden window's `CALayerHost`.

#### Conclusion

The dual-host hypothesis is wrong, or at least `DisableDisplay()` is not the
right way to test it. Disabling the `DisplayCALayerTree` made things worse — the
navigated page never appears instead of appearing after 10 seconds. This
suggests the hidden window's `DisplayCALayerTree` may be necessary for the
compositor pipeline to function, not just a redundant consumer of the
`CAContext`. The code changes from this experiment should be reverted.

### Experiment 4: Audit all 10-second delays and CALayer-related timing in Chromium

#### Problem

The ~10-second blank is suspiciously consistent. Issue 628 Experiment 8 found
and modified one 10-second delay (the CALayerParams dedup gate in
`root_compositor_frame_sink_impl.cc`) but it had no effect. There may be other
10-second delays in Chromium that explain the behavior. Additionally, there may
be CALayer-related delays or hidden-window compositor throttling that contribute
to the problem.

#### Research questions

**R1: Where are ALL the 10-second delays in Chromium?**

Comprehensive search for `base::Seconds(10)`,
`base::TimeDelta::FromSeconds(10)`, and related patterns across the rendering,
compositing, and display pipelines.

**R2: Where are ALL the CALayer-related delays?**

Any delays, timeouts, or throttling in files related to `CALayerHost`,
`CAContext`, `CALayerParams`, `DisplayCALayerTree`, or `AcceleratedWidgetMac`.

**R3: Does the hidden window cause compositor detachment or throttling?**

Trace how a hidden NSWindow (`[window orderOut:nil]`) affects the compositor
pipeline. Does it trigger `SetRenderWidgetHostIsHidden(true)`? Does it detach
the compositor? Does it invalidate surface IDs during navigation?

#### Results

**R1: Two 10-second delays found in the rendering pipeline.**

**Delay 1: CALayerParams dedup gate (ALREADY TESTED — NO EFFECT)**

File:
`components/viz/service/frame_sinks/root_compositor_frame_sink_impl.cc:912`

```cpp
next_forced_ca_layer_params_update_time_ =
    base::TimeTicks::Now() + base::Seconds(10);
```

Suppresses identical `CALayerParams` for 10 seconds. Periodically forces an
update to detect dynamic vsync changes. Tested in Issue 628 Experiment 8 —
reducing to 100ms had no effect on the blank.

**Delay 2: Temporary surface reference expiration (NEW — UNTESTED)**

File: `components/viz/service/surfaces/surface_manager.cc:39`

```cpp
constexpr base::TimeDelta kExpireInterval = base::Seconds(10);
```

The `SurfaceManager` runs `ExpireOldTemporaryReferences()` every 10 seconds.
When a new surface is created, a temporary reference is added. The expectation
is that the parent compositor will claim the surface quickly by replacing the
temporary reference with a proper one. If the parent never claims it:

1. **First timer fire (10s):** If the surface is marked for destruction,
   `marked_as_old = true`.
2. **Second timer fire (20s):** Old references are deleted and surfaces are
   garbage collected.

The comment at line 467 reads: _"The temporary reference has existed for more
than 10 seconds, a surface reference should have replaced it by now. To avoid
permanently leaking memory delete the temporary reference."_

This is suspicious: if the hidden window's compositor doesn't properly claim the
new surface after navigation, the temporary reference could expire, causing the
surface to be garbage collected. However, the timing doesn't perfectly match —
the blank is ~10 seconds, not ~20 seconds. The two-phase system means surfaces
survive 10-20 seconds, depending on when the timer happens to fire relative to
surface creation.

**Other 10-second delays found (less relevant):**

- `viz/host/gpu_host_impl.cc:664` — GPU shutdown timeout
  (`kShutDownTimeout = base::Seconds(10)`). Only affects GPU process shutdown,
  not rendering.
- `gpu/ipc/service/gpu_watchdog_thread.cc:71` — GPU watchdog adjustment.
  Windows-only.
- `viz/service/frame_sinks/video_capture/shared_memory_video_frame_pool.h:86` —
  Logging rate limit.

**R2: CALayer-related delays are all short.**

| Delay                      | File                                                                      | Duration | Purpose                             |
| -------------------------- | ------------------------------------------------------------------------- | -------- | ----------------------------------- |
| CATransaction post-commit  | `ui/accelerated_widget_mac/ca_transaction_observer.mm:34`                 | 50ms     | Wait for post-commit observers      |
| Metal backpressure polling | `ui/accelerated_widget_mac/ca_layer_tree_coordinator.mm:128`              | 1ms/poll | Poll Metal fences between frames    |
| GPU command buffer polling | `gpu/ipc/service/command_buffer_stub.cc:70-74`                            | 1-2ms    | Prevent fast/slow frame alternation |
| Delayed GPU task           | `viz/service/display_embedder/skia_output_surface_dependency_impl.cc:170` | 2ms      | GPU thread task scheduling          |
| Frame interval timeout     | `viz/service/display/frame_interval_matchers.h:116`                       | 100ms    | Avoid blips during rate switching   |
| Overlay reclaim            | `viz/service/display_embedder/skia_output_device_buffer_queue.h:130`      | 1s       | Batch overlay resource reclamation  |
| Gr cache cleanup (macOS)   | `gpu/command_buffer/service/gr_cache_controller.cc:76`                    | 5s       | Free unused GPU resources           |

None of these match the 10-second blank. The longest is the 5-second macOS GPU
resource cleanup, which only frees idle resources and wouldn't cause a blank.

**R3: Hidden window compositor behavior — potentially critical.**

The hidden NSWindow may trigger `render_widget_host_is_hidden_ = true`, which
has severe consequences for the compositor pipeline:

**Compositor detachment** (`browser_compositor_view_mac.mm:191-204`):

```cpp
void BrowserCompositorMac::UpdateState() {
  if (parent_ui_layer_) {
    TransitionToState(UseParentLayerCompositor);
    return;
  }
  if (!render_widget_host_is_hidden_) {
    TransitionToState(HasOwnCompositor);
    return;
  }
  // Otherwise put the compositor up for recycling.
  TransitionToState(HasNoCompositor);
}
```

When `render_widget_host_is_hidden_` is true and there's no parent UI layer
(content_shell doesn't use Views), the compositor transitions to
`HasNoCompositor` — the entire rendering pipeline is torn down.

**Surface ID invalidation on navigation**
(`browser_compositor_view_mac.mm:341-361`):

```cpp
void BrowserCompositorMac::DidNavigate() {
  if (render_widget_host_is_hidden_) {
    dfh_local_surface_id_allocator_.Invalidate();
  } else {
    dfh_local_surface_id_allocator_.GenerateId();
    delegated_frame_host_->EmbedSurface(...);
    client_->OnBrowserCompositorSurfaceIdChanged();
  }
}
```

When `render_widget_host_is_hidden_` is true during navigation, the surface ID
allocator is **invalidated**. No new surface ID is generated. No surface is
embedded. The comment says: _"Navigating while hidden should not allocate a new
LocalSurfaceID. Once sizes are ready, or we begin to Show, we can then allocate
the new LocalSurfaceId."_

**How the hidden flag is set** (`render_widget_host_view_mac.mm:491-581`):

```
Hide() → WasOccluded() → SetRenderWidgetHostIsHidden(true)
ShowWithVisibility() → SetRenderWidgetHostIsHidden(false)
```

`Hide()` is called when the view becomes invisible. `WasOccluded()` is called
when the window is occluded. The hidden NSWindow (`orderOut:nil`) could trigger
either of these paths.

**This is a strong lead.** If the hidden window causes
`render_widget_host_is_hidden_` to be true, then during navigation:

1. The compositor is in `HasNoCompositor` state — no rendering pipeline.
2. `DidNavigate()` invalidates the surface ID — no new surface is created.
3. No frames are submitted for the new page.
4. The only thing that eventually triggers rendering is some timeout or recovery
   mechanism — potentially the 10-second surface expiration or a periodic
   compositor check.

However, this contradicts Issue 628 Experiment 7's finding that the new
`ca_context_id` arrives within 100ms. If the compositor were truly detached, no
`ca_context_id` would be produced. This needs verification: was Experiment 7 run
with the Issue 628 code (which included `RenderViewHostChanged` and may have
triggered `WasShown`)?

#### Conclusion

Two new leads emerged from this research:

1. **Surface Manager temporary reference expiration**
   (`kExpireInterval =
   base::Seconds(10)`). If the hidden window's compositor
   doesn't properly claim new surfaces after navigation, temporary references
   expire after 10 seconds. This is the only untested 10-second delay that could
   affect rendering.

2. **Hidden window compositor detachment.** The `render_widget_host_is_hidden_`
   flag may be true for the hidden window, causing the compositor to be detached
   and surface IDs to be invalidated during navigation. This would explain why
   the new page takes so long to appear — the rendering pipeline is torn down
   and needs to be reconstructed.

The next experiment should determine whether `render_widget_host_is_hidden_` is
true in the Chromium Profile Server. Add diagnostic logging to
`BrowserCompositorMac::DidNavigate()` and `UpdateState()` to confirm whether the
compositor is detached and surface IDs are invalidated during navigation.

### Experiment 5: Full code audit — TermSurf and Chromium Profile Server

#### Problem

Experiments 1–4 focused on Chromium internals: CAContext lifecycle, dual
CALayerHost, DisableDisplay, 10-second timers. None explained the blank. Before
going deeper into Chromium, we should audit **our own code** — both the Chromium
Profile Server (`shell_browser_main_parts.cc`, `shell_tab_observer.cc`,
`shell_ca_layer_bridge_mac.mm`) and the GUI (`Metal.zig`, `generic.zig`,
`xpc.zig`, `Surface.zig`). The goal: is there an obvious oversight, a stale
reference, a missing re-registration, or bad code that could explain the
problem?

This experiment also reverts Experiment 3's Chromium changes (DisableDisplay),
which made things worse.

#### Prerequisites

Revert all Experiment 3 code changes on the `146.0.7650.0-issue-629` Chromium
branch:

- `render_widget_host_view_mac.h` — remove `DisableNSViewDisplay()` declaration
- `render_widget_host_view_mac.mm` — remove `DisableNSViewDisplay()`
  implementation
- `shell_ca_layer_bridge_mac.h` — remove `DisableDisplayOnView()` declaration
- `shell_ca_layer_bridge_mac.mm` — remove `DisableDisplayOnView()`
  implementation
- `shell_browser_main_parts.cc` — remove the `DisableDisplayOnView(view)` call

After reverting, rebuild to confirm clean state:
`autoninja -C out/Default chromium_profile_server`

#### Audit checklist

**Architectural concerns (items 1–10)**

**1. Callback survival across navigation.** The CALayerParams callback is
registered once in `CreateTab()` (`shell_browser_main_parts.cc:408-431`) on the
initial `RenderWidgetHostView`. On cross-origin navigation, Chromium may create
a new `RenderWidgetHostView` (site isolation). Does the callback survive? Or
does the new view have no callback, meaning no `ca_context_id` is ever sent for
the navigated page? Check whether `shell_tab_observer.cc` implements
`RenderViewHostChanged` — if not, nobody re-registers the callback on view swap.

**2. ca_context_id transition handling in the GUI.** When `handleCAContext()`
fires in `xpc.zig:409`, it calls `surface.setCAContextId()` which replaces the
`CALayerHost` in `Metal.zig:188-213`. Check: does the replacement path properly
remove the old host before adding the new one? Is there a frame where neither
host is attached? Does the positioning layer still exist and is it still
connected to the flipped layer?

**3. Hidden window visibility state.** The Chromium Profile Server's NSWindow is
hidden via `[window orderOut:nil]` in `shell_platform_delegate_mac.mm`. Does
this trigger `WasOccluded()` → `SetRenderWidgetHostIsHidden(true)` on the
`RenderWidgetHostViewMac`? If so, `BrowserCompositorMac` transitions to
`HasNoCompositor` and `DidNavigate()` invalidates the surface ID instead of
generating a new one. This is the strongest lead from Experiment 4.

**4. Competing CALayerHost during transition.** The hidden window's
`DisplayCALayerTree` gets the new `ca_context_id` synchronously (before the XPC
callback) and creates a `CALayerHost` in `GotCALayerFrame()`. The GUI creates a
second one asynchronously after XPC delivery. Check: does Window Server
deprioritize or delay compositing when two hosts point at the same CAContext?
Does the hidden window's host "claim" the context?

**5. IOSurface overlay lifecycle on blank.** When the old CAContext dies (old
page) and the new one hasn't produced content yet, what does the GUI's
`CALayerHost` display? Does it go transparent? Black? Does the Zig renderer
notice and respond, or does it assume the host is always valid?

**6. Shell tab observer navigation hooks.** Read `shell_tab_observer.h` and
`.cc` line by line. List every `WebContentsObserver` method it implements. Does
it implement `RenderViewHostChanged`? `RenderFrameHostChanged`? If not, there is
no code that re-registers the CALayerParams callback or cursor callback on the
new view after a cross-origin navigation. This is the single most likely cause.

**7. Same-origin vs cross-origin behavior.** Same-origin navigations reuse the
`RenderWidgetHostView`. Cross-origin ones may create a new one. If the callback
is only lost on cross-origin navigations, the blank should only happen for
cross-site links (e.g., google.com → github.com), not same-site links (e.g., one
GitHub page to another). Consider whether our test navigation is same-site or
cross-site.

**8. Frame production gap.** Between the old page's last frame and the new
page's first frame, how long is the gap? The CALayerParams callback only fires
when `AcceleratedWidgetCALayerParamsUpdated()` is called. If the compositor is
paused or detached during this window (see item 3), no callback fires and no
`ca_context_id` is sent.

**9. XPC message ordering.** `url_changed` is sent from `DidFinishNavigation()`
on the UI thread. `ca_context` is sent from
`AcceleratedWidgetCALayerParamsUpdated()` which may fire from a different thread
context. These go to the same XPC connection. Check: could `ca_context` arrive
before `url_changed`? Could `url_changed` arrive without a corresponding
`ca_context`? Does the GUI assume any ordering?

**10. The 10-second coincidence.** Surface Manager `kExpireInterval` is exactly
10 seconds. The blank is ~10 seconds. Could the page reappearing be triggered by
surface reference expiration causing the compositor to re-embed a surface, which
generates a new `ca_context_id`, which fires the callback?

**Code quality concerns (items 11–20)**

**11. Hardcoded delays or sleeps.** Search all TermSurf code (GUI + Chromium
Profile Server) for `sleep`, `dispatch_after`, `asyncAfter`, `PostDelayedTask`,
`std.time.sleep`, or any timer-based "wait for ready" pattern. Any such delay is
a red flag.

**12. Polling loops.** Search for repeated timer checks or busy-wait patterns —
`setInterval`-style code that polls for a condition instead of reacting to an
event.

**13. Recreating objects that should be reused, or vice versa.** The GUI creates
a new `CALayerHost` on every `ca_context_id` change (`Metal.zig:200`). Is this
correct? Chromium's `DisplayCALayerTree` does the same thing (never updates
`contextId` in place). But check: is the positioning layer recreated when it
shouldn't be? Is anything cached that should be refreshed?

**14. Ignored return values.** Check every XPC call
(`xpc_connection_send_message`, `xpc_dictionary_get_*`), every ObjC `msgSend`,
and every `getClass`/`sel` call. Are return values checked? Can any of these
fail silently and leave the system in a broken state?

**15. Wrong thread / wrong queue.** XPC callbacks arrive on serial dispatch
queues. `setCAContextId()` acquires `draw_mutex`. The Metal renderer runs on the
render thread. Check: is `setCALayerHostContextId()` safe to call from an XPC
dispatch queue? Are ObjC layer operations (addSublayer, removeFromSuperlayer)
safe from a non-main thread? CALayer mutations must happen on the main thread or
within a `CATransaction`.

**16. One-shot setup that should be per-navigation.** The CALayerParams callback
(`shell_browser_main_parts.cc:408-431`) is registered once. The cursor callback
(`shell_browser_main_parts.cc:394-396`) is registered once. If the view is
swapped on navigation, both are lost. Check every `SetFoo()` call in
`CreateTab()` and ask: should this be called again after navigation?

**17. Stale Mach port / pointer references.** After navigation, do any pointers
in `TabState`, `ShellTabObserver`, or the GUI's pane state point to destroyed
objects? Specifically: does `tab->shell->web_contents()` return a different
`WebContents` after navigation? Does `GetRenderWidgetHostView()` return a
different view?

**18. Missing cleanup on old resources.** When `setCALayerHostContextId()`
replaces the old `CALayerHost` (`Metal.zig:193-195`), it calls
`removeFromSuperlayer` + `release`. Is this sufficient? Does the old host hold
any references that prevent the old CAContext from being cleaned up? Does the
positioning layer need to be flushed?

**19. Swallowed errors.** Search for `orelse return`, `catch {}`, early returns
without logging, and `if (!foo) return` patterns. Any of these could silently
swallow a failure that leaves the overlay blank.

**20. Assumptions about ordering or timing.** `setCAContextId()` calls
`setCALayerHostContextId()` then `updateCALayerHostFrame()` synchronously
(`Surface.zig:2527-2528`). Does the frame update assume the host has been
rendered at least once? `setOverlay()` calls `updateCALayerHostFrame()` but the
host may not exist yet. The frame update silently returns if
`ca_layer_positioning` is null (`generic.zig:851`). Is there a path where the
positioning layer exists but the host doesn't, or vice versa?

#### Verification

For each item: record whether it's a confirmed issue, not an issue, or
inconclusive. For confirmed issues, describe what's wrong and what the fix would
be. At the end, rank the findings by likelihood of causing the ~10-second blank.

**Pass:** At least one confirmed issue is identified that plausibly explains the
blank.

**Fail:** All 20 items come back clean — the code is correct and the problem is
entirely in Chromium internals or Window Server behavior.

#### Chromium revert

All Experiment 3 code changes reverted on `146.0.7650.0-issue-629`:

- `render_widget_host_view_mac.h` — removed `DisableNSViewDisplay()` declaration
- `render_widget_host_view_mac.mm` — removed `DisableNSViewDisplay()`
  implementation
- `shell_ca_layer_bridge_mac.h` — removed `DisableDisplayOnView()` declaration
- `shell_ca_layer_bridge_mac.mm` — removed `DisableDisplayOnView()`
  implementation
- `shell_browser_main_parts.cc` — removed the `DisableDisplayOnView(view)` call

#### Results

**Item 1: Callback survival across navigation — CONFIRMED BUG (latent)**

The CALayerParams callback is registered once in `CreateTab()`
(`shell_browser_main_parts.cc:404-427`) on the initial `RenderWidgetHostView`.
`ShellTabObserver` does NOT implement `RenderViewHostChanged` or
`RenderFrameHostChanged`. On cross-site navigation with site isolation, Chromium
creates a new `RenderWidgetHostView` and destroys the old one. The callback is
on the old view and is lost. Nobody re-registers it on the new view.

However, the Chromium Profile Server is content_shell-based and does not enable
strict site isolation. Same-site navigations reuse the `RenderWidgetHostView`.
Most test navigations (clicking a link on google.com results page) are
cross-origin, but without site isolation the view is not swapped. This is a
confirmed latent bug that will surface when site isolation is enabled, but is
**probably not the cause of the current ~10-second blank**.

**Fix:** Add `RenderViewHostChanged()` to `ShellTabObserver` that re-registers
both the CALayerParams callback and the cursor callback on the new view.

**Item 2: ca_context_id transition handling in the GUI — NOT AN ISSUE**

`Metal.zig:188-213` properly handles the replacement path:

1. Removes old host from positioning layer (`removeFromSuperlayer` + `release`)
2. Creates new `CALayerHost` with new `contextId`
3. Adds new host to existing positioning layer
4. Updates the stored pointer

The positioning layer and flipped layer are preserved (not recreated). The
removal-then-addition is synchronous within the same function call. This matches
Chromium's `DisplayCALayerTree::GotCALayerFrame()` pattern.

**Item 3: Hidden window visibility state — INCONCLUSIVE**

The Chromium Profile Server hides its NSWindow via `[window orderOut:nil]`
(`shell_platform_delegate_mac.mm:209`). This could trigger `WasOccluded()` →
`SetRenderWidgetHostIsHidden(true)` on the `RenderWidgetHostViewMac`. If
`render_widget_host_is_hidden_` is true:

1. `BrowserCompositorMac::UpdateState()` transitions to `HasNoCompositor`
2. `DidNavigate()` invalidates the surface ID instead of generating a new one
3. No new surface is embedded, no frames are submitted

This would explain the blank: the compositor pipeline is torn down on
navigation. Recovery might happen when the 10-second `kExpireInterval` fires in
the Surface Manager (item 10).

However, this contradicts Issue 628 Experiment 7's finding that the new
`ca_context_id` arrives within 100ms. If the compositor were truly detached, no
`ca_context_id` would be produced. Either: (a) `[window orderOut:nil]` does NOT
trigger `render_widget_host_is_hidden_` for a borderless window that was never
shown, or (b) the initial tab creation path avoids the hidden flag because the
window already exists when the first tab is created.

**Needs diagnostic logging** in `BrowserCompositorMac::UpdateState()` and
`DidNavigate()` to confirm whether `render_widget_host_is_hidden_` is true.

**Item 4: Competing CALayerHost during transition — NOT AN ISSUE**

Experiment 3 disproved this hypothesis. Removing the hidden window's
`DisplayCALayerTree` (which eliminates the competing `CALayerHost`) made things
worse — the navigated page never appeared at all. The hidden window's host is
necessary for the compositor pipeline to function.

**Item 5: IOSurface overlay lifecycle on blank — NOT AN ISSUE**

When the old `CAContext` dies and the new one hasn't produced content yet, the
GUI's `CALayerHost` shows nothing (transparent). This is expected behavior
during the transition. The renderer doesn't need to detect or respond to this —
the new `CALayerHost` will display content when the new `CAContext` starts
producing frames.

**Item 6: Shell tab observer navigation hooks — CONFIRMED BUG (latent)**

`ShellTabObserver` implements only:

- `DidFinishNavigation`
- `DidStartLoading`
- `DidStopLoading`
- `LoadProgressChanged`
- `DidFailLoad`

It does NOT implement `RenderViewHostChanged` or `RenderFrameHostChanged`.
`Shell` implements `PrimaryPageChanged` but only calls
`DidNavigatePrimaryMainFramePostCommit()` on the platform delegate, which is a
no-op (`shell_platform_delegate_mac.mm:333-335`).

This means there is no code anywhere that re-registers callbacks after a view
swap. Same analysis as item 1 — latent bug, not the immediate cause.

**Fix:** Same as item 1.

**Item 7: Same-origin vs cross-origin behavior — NOT RELEVANT**

Without site isolation in content_shell, the `RenderWidgetHostView` is reused
for both same-site and cross-site navigations. The CALayerParams callback
survives. Whether the test navigation is same-origin or cross-origin doesn't
matter for the current bug.

**Item 8: Frame production gap — NOT AN ISSUE**

Issue 628 Experiment 7 confirmed the new `ca_context_id` arrives within 100ms of
clicking a link. The compositor is not paused during the navigation transition.
The frame production gap is negligible.

**Item 9: XPC message ordering — NOT AN ISSUE**

`url_changed` is sent from `DidFinishNavigation()` on the UI thread.
`ca_context` is sent from `AcceleratedWidgetCALayerParamsUpdated()`. Both go to
the same XPC connection. The GUI's `handleCAContext()` and `handleUrlChanged()`
are independent handlers — neither assumes the other has already fired. No
ordering dependency exists.

**Item 10: 10-second surface reference expiration — INCONCLUSIVE**

`SurfaceManager::kExpireInterval = base::Seconds(10)` matches the ~10-second
blank exactly. If the hidden window's compositor doesn't properly claim new
surfaces after navigation (because `render_widget_host_is_hidden_` causes
`DidNavigate()` to invalidate the surface ID instead of embedding), temporary
references would expire after 10 seconds, triggering surface garbage collection.
This could cause the compositor to re-embed, generating a new `ca_context_id`.

This theory depends on item 3 (hidden window compositor detachment) being true.
If the compositor is NOT detached, this is irrelevant.

**Item 11: Hardcoded delays or sleeps — NOT AN ISSUE**

No sleeps, `dispatch_after`, `PostDelayedTask`, or timer-based delays found in
any TermSurf browser integration code. The only sleeps in the GUI codebase are
in upstream Ghostty code (terminal I/O, fork handling) — unrelated to the
overlay pipeline.

**Item 12: Polling loops — NOT AN ISSUE**

No polling loops found in the browser integration code.

**Item 13: Recreating objects that should be reused — NOT AN ISSUE**

`Metal.zig` creates a new `CALayerHost` on every `ca_context_id` change (line
200). This matches Chromium's `DisplayCALayerTree::GotCALayerFrame()` behavior —
it also creates a new host rather than updating `contextId` on the existing one.
The positioning layer and flipped layer are correctly reused.

**Item 14: Ignored return values — MINOR (not the cause)**

`xpc_connection_send_message` returns void (fire-and-forget). ObjC `msgSend`
calls for `removeFromSuperlayer` and `addSublayer:` return void. The
`getClass`/`sel` calls are checked with `orelse` guards that log warnings and
return early. No dangerous ignored return values.

**Item 15: CALayer mutations from background thread — CONFIRMED ISSUE**

All CALayerHost creation, replacement, and removal happens on the XPC serial
dispatch queue (`com.termsurf.ghost.xpc`), **not the main thread**. The call
chain:

```
XPC serial queue (background)
  → handleCAContext() [xpc.zig:409]
    → surface.setCAContextId() [Surface.zig:2524] (acquires draw_mutex)
      → renderer.setCALayerHostContextId() [generic.zig:841]
        → Metal.setCALayerHostContextId() [Metal.zig:172]
          → removeFromSuperlayer, release, [CALayerHost layer], addSublayer:
```

Apple's documentation states CALayer modifications should happen on the main
thread. On the main thread, implicit `CATransaction`s are committed at the end
of each run loop iteration by Core Animation. On a background GCD queue without
a run loop, implicit transactions may not commit automatically.

The initial `setCAContextId` call also happens from this background queue and
works correctly. The difference during navigation is that the replacement path
removes the old host (causing the overlay to go blank) and adds the new host in
the same call. If the `CATransaction` commit is delayed or deferred until the
main thread's next run loop iteration, the removal takes visual effect
immediately (because the layer is gone from the tree) but the addition might not
take visual effect until the transaction commits.

This is unlikely to cause a 10-SECOND delay on its own. But combined with the
hidden window's `CAContext` lifecycle (item 3), background-thread layer
mutations could exacerbate timing issues.

Chromium's `DisplayCALayerTree` wraps its `CALayerHost` operations in
`ScopedCAActionDisabler` (which disables implicit animations) and runs on the
main thread. Our code does neither.

**Fix:** Dispatch CALayerHost creation/replacement to the main thread, and wrap
in `[CATransaction begin]` / `[CATransaction commit]` with
`[CATransaction setDisableActions:YES]`.

**Item 16: One-shot setup that should be per-navigation — CONFIRMED BUG
(latent)**

Same as items 1 and 6. Both the CALayerParams callback
(`shell_browser_main_parts.cc:404-427`) and the cursor callback
(`shell_browser_main_parts.cc:394-396`) are registered once in `CreateTab()`.
Neither is re-registered after a view swap.

**Item 17: Stale Mach port / pointer references — NOT AN ISSUE**

`tab->shell->web_contents()` returns the same `WebContents` throughout the tab's
lifetime (WebContents survives navigation). In `ResizeTab()`,
`HandleMouseEvent()`, `HandleKeyEvent()`, etc., the code calls
`GetRenderWidgetHostView()` fresh each time — this returns the current (possibly
new) view. No stale pointers.

**Item 18: Missing cleanup on old resources — NOT AN ISSUE**

`Metal.zig:193-195` properly removes the old `CALayerHost`:
`removeFromSuperlayer` detaches it from the layer tree, `release` drops the
retain count. The positioning layer is reused, not leaked. Old `CAContext`
cleanup is handled by Chromium's GPU process when the `CALayerTreeCoordinator`
destructor runs.

**Item 19: Swallowed errors — MINOR (not the cause)**

- `queueRender() catch {}` in `Surface.zig:2507,2543` silently swallows render
  queue failures. These are unlikely to fail in practice.
- `generic.zig:851` returns early if `ca_layer_positioning` is null — correct
  behavior, not an error.
- `xpc.zig:415-419` returns silently if pane not found — correct for
  out-of-order messages.

None of these could cause the 10-second blank.

**Item 20: Assumptions about ordering or timing — NOT AN ISSUE**

`setCAContextId()` calls `setCALayerHostContextId()` then
`updateCALayerHostFrame()` synchronously under `draw_mutex`. The frame update
requires `ca_layer_positioning` to exist (early return if null). In practice,
`setOverlay()` (from `web` TUI) always arrives before `ca_context` (from
Chromium), so the positioning layer exists when `setCAContextId()` is called.
Both handlers run on the same serial XPC queue, so there's no race.

#### Findings ranked by likelihood

1. **Item 3 + Item 10: Hidden window compositor detachment + 10-second surface
   expiration.** If `[window orderOut:nil]` causes
   `render_widget_host_is_hidden_ = true`, the compositor is detached during
   navigation. `DidNavigate()` invalidates the surface ID. No new surface is
   embedded. The surface manager's 10-second `kExpireInterval` triggers cleanup
   and eventual recovery. This perfectly explains both the blank and its
   consistent ~10-second duration. **INCONCLUSIVE — needs diagnostic logging.**

2. **Item 15: CALayer mutations from background thread.** All CALayerHost
   operations happen on a GCD queue that is not the main thread. `CATransaction`
   commits may be delayed. Chromium uses `ScopedCAActionDisabler` and runs on
   the main thread — we do neither. This could contribute to the blank but
   unlikely to cause a 10-second delay on its own. **CONFIRMED ISSUE — should be
   fixed regardless.**

3. **Items 1/6/16: Missing RenderViewHostChanged.** The CALayerParams callback
   and cursor callback are registered once and never re-registered after a view
   swap. This will cause permanent blank (not 10-second blank) on cross-site
   navigation with site isolation. Currently latent because content_shell
   doesn't enable strict site isolation. **CONFIRMED BUG (latent) — should be
   fixed.**

#### Conclusion

**Pass.** Three confirmed issues found. Two are latent bugs (items 1/6/16:
missing callback re-registration). One is an active code quality issue (item 15:
background-thread CALayer mutations). One strong hypothesis remains inconclusive
(items 3+10: hidden window compositor detachment + surface expiration).

The next experiment should add diagnostic logging to confirm whether
`render_widget_host_is_hidden_` is true in the Chromium Profile Server. If
confirmed, the fix is straightforward: call `ShowWithVisibility(VISIBLE)` on the
`RenderWidgetHostViewMac` after `[window orderOut:nil]`, or intercept the
`WasOccluded()` call to prevent it from setting the hidden flag.

## Conclusion

Five experiments investigated the ~10-second navigation blank in CALayerHost.
The first four focused on Chromium internals — CAContext lifecycle, dual
CALayerHost interference, `DisableDisplay()`, and timer audits. None explained
the blank. Experiment 5 turned the audit inward on TermSurf's own code and
produced the strongest leads.

### Primary hypothesis: hidden window compositor detachment

The Chromium Profile Server hides its NSWindow via `[window orderOut:nil]`. This
likely sets `render_widget_host_is_hidden_ = true` on the
`RenderWidgetHostViewMac`, which causes `BrowserCompositorMac` to transition to
`HasNoCompositor`. During navigation, `DidNavigate()` then invalidates the
surface ID instead of generating a new one — no new surface is embedded, no
frames are submitted. The surface manager's
`kExpireInterval = base::Seconds(10)` eventually garbage-collects the orphaned
temporary reference, triggering recovery. This explains both the blank and its
consistent ~10-second duration.

This remains unverified. The next step is diagnostic logging in
`BrowserCompositorMac::UpdateState()` and `DidNavigate()` to confirm whether
`render_widget_host_is_hidden_` is true.

### Confirmed bugs

1. **CALayer mutations from background thread.** All CALayerHost
   creation/replacement happens on the XPC serial GCD queue, not the main
   thread. No `CATransaction` wrapping, no `ScopedCAActionDisabler`. Chromium
   does both. This violates Apple's threading model for Core Animation and could
   cause delayed or missed visual updates.

2. **Missing `RenderViewHostChanged` in `ShellTabObserver`.** The CALayerParams
   callback and cursor callback are registered once in `CreateTab()` on the
   initial `RenderWidgetHostView`. Nobody re-registers them after a view swap.
   Currently latent (content_shell doesn't enable strict site isolation), but
   will cause permanent blank on cross-site navigation when site isolation is
   enabled.
