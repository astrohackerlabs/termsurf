+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 622: JavaScript Is Slow

## Goal

Identify and fix the Chromium mechanism that throttles JavaScript-driven
rendering to 2fps when two BrowserContexts coexist in a single process. The fix
must allow two profiles, each running `requestAnimationFrame` loops, to both
render at 60fps.

## Background

Two prior issues (620, 621) systematically narrowed a 2fps rendering degradation
across 20 experiments. The result: **JavaScript execution on the Blink main
thread is the sole trigger.** Everything else — the compositor, the GPU
pipeline, the viz frame delivery system — is clean.

### What's fast

Two BrowserContexts with **CSS-only animations** both render at 60fps. CSS
`@keyframes` animations run in the compositor thread. They generate continuous
compositor damage every vsync — new CompositorFrames, new draw calls, new GPU
commands — yet two profiles handle this without any degradation.

Two BrowserContexts both loading **lite.duckduckgo.com** (a static HTML form
with virtually no JavaScript) also render at 60fps.

This proves the compositor thread, GPU command serialization, paint layer
complexity, and compositor damage frequency are all fine.

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| CSS animation       | CSS animation       | 60    | 60    | 621.4      |
| lite.duckduckgo.com | lite.duckduckgo.com | 60    | 60    | 621.3      |

### What's slow

Two BrowserContexts both running **any JavaScript animation** degrade to 2fps.
This includes google.com (heavyweight: analytics, autocomplete, service workers,
ad scripts) and the ts4 box demo (lightweight: a 30-line `requestAnimationFrame`
loop drawing one rectangle on a 300x300 canvas). The degradation is identical
regardless of JavaScript complexity — even the most trivial rAF loop triggers
it.

| Profile A         | Profile B         | A fps | B fps | Experiment |
| ----------------- | ----------------- | ----- | ----- | ---------- |
| google.com        | google.com        | 2     | 2     | 621.2      |
| JS box demo (rAF) | JS box demo (rAF) | 2     | 2     | 621.5      |

### What's mixed

When one profile runs JavaScript and the other doesn't, only the JavaScript
profile degrades. The non-JavaScript profile is unaffected.

google.com (continuous JS) paired with lite.duckduckgo.com (no JS): google drops
to 2fps, DDG stays at 60fps. Reversing the profile order reverses which window
is slow — it's always the one running JavaScript, regardless of which
BrowserContext it belongs to.

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| google.com          | lite.duckduckgo.com | 2     | 60    | 620.14     |
| lite.duckduckgo.com | google.com          | 60    | 2     | 620.15     |

### What the viz pipeline research eliminated

Issue 620 Experiments 12–15 instrumented the entire viz/compositor pipeline.
BeginFrames arrive at 60fps to both profiles. The renderer receives them but
only produces CompositorFrames at ~3fps for JavaScript-heavy pages. Every
throttle mechanism in the viz pipeline was checked and either never triggered or
confirmed as a symptom rather than a root cause:

- StopObservingBeginFrames — symptom, fixed in 620 Exp 13
- ShouldDraw() gate — healthy except `needs_draw_`
- CVDisplayLink thrashing — observed but not causal
- BeginFrameTracker throttle — never triggered
- kUndrawnFrameLimit — never triggered
- root_frame_missing() — reinforces the stall but doesn't cause it

### The unexplored layer

The bottleneck is between the compositor thread (which receives BeginFrames at
60fps) and the Blink main thread (which executes `requestAnimationFrame`
callbacks). This interface — **BeginMainFrame dispatch** — is where the
compositor tells the main thread "start your frame work now." When two
BrowserContexts both have active rAF loops, something in this layer serializes
or throttles the callbacks.

Key areas to investigate:

- **Renderer process allocation**
  (`content/browser/renderer_host/render_process_host_impl.cc`) — do two
  BrowserContexts get separate renderer processes, or share one? If they share a
  process, there's literally one Blink main thread running both rAF loops.
- **Blink's main thread scheduler**
  (`third_party/blink/renderer/platform/scheduler/`) — how it prioritizes and
  dispatches tasks across multiple renderer contexts
- **BeginMainFrame** — the compositor-to-main-thread signal that triggers rAF
  callbacks, style recalc, layout, and paint
- **ProxyMain / ThreadProxy** (`cc/trees/`) — the cc-layer interface between the
  compositor thread and the main thread

## Approach

Research the Chromium source code first, guided by the precise signal from
Issues 620–621. Previous searches were blind — now we know the bottleneck is
JavaScript on the Blink main thread, not the compositor or GPU. Start by
answering the critical architectural question: do two BrowserContexts share a
renderer process? The answer determines the entire investigation direction.

If a likely culprit is identified, design experiments to confirm and fix it.

## Experiments

### Experiment 1: Research renderer process allocation and rAF scheduling

A source code research experiment — no code changes, no builds. Read the
Chromium source to answer three questions that determine the investigation
direction.

#### Question 1: Do two BrowserContexts share a renderer process?

This is the most important question. If two BrowserContexts loading different
origins share a single renderer process, there is literally one Blink main
thread running both `requestAnimationFrame` loops. The 2fps would be explained
by a single thread alternating between two rAF callbacks with scheduling
overhead.

If they get separate renderer processes (each with their own Blink main thread),
the contention must be in a shared resource outside the renderer — the browser
process, GPU process, or inter-process scheduling.

**Where to look:**

- `content/browser/renderer_host/render_process_host_impl.cc` —
  `GetProcessHostForSiteInstance()` or similar method that decides process
  allocation
- `content/browser/site_instance_impl.cc` — how SiteInstances map to processes
- `content/browser/renderer_host/render_process_host_impl.cc` —
  `GetProcessCount()` or process limit logic
- Content Shell's process model — does it use `--single-process`,
  `--process-per-site`, or default multi-process?

**Expected outcome:** Two BrowserContexts with different origins should get
separate renderer processes by default. But Content Shell might override this.

#### Question 2: How does BeginMainFrame reach the Blink main thread?

When the compositor thread decides it's time for a new frame, it sends a
BeginMainFrame signal to the Blink main thread. This triggers rAF callbacks,
style recalc, layout, and paint. If this dispatch mechanism has any
serialization or throttling across multiple contexts, it would explain the 2fps.

**Where to look:**

- `cc/trees/proxy_main.cc` — `BeginMainFrame()` method
- `cc/trees/single_thread_proxy.cc` — single-threaded alternative (Content Shell
  might use this)
- `third_party/blink/renderer/platform/widget/compositing/layer_tree_view.cc` —
  Blink's interface to cc
- `third_party/blink/renderer/core/frame/local_frame_view.cc` —
  `ServiceScriptedAnimations()` which runs rAF callbacks

**Expected outcome:** Each renderer process has its own compositor thread and
main thread. BeginMainFrame should be per-renderer-process. But if Content Shell
uses single-threaded compositing, both contexts might share one thread.

#### Question 3: Does Content Shell use single-threaded compositing?

Content Shell is a minimal embedder. It might use `--single-process` mode or
single-threaded compositing by default, which would put both BrowserContexts'
compositor and main thread work on the same thread.

**Where to look:**

- `content/shell/browser/shell_content_browser_client.cc` — process model
  overrides
- `content/shell/app/shell_main_delegate.cc` — command line flags
- `content/shell/common/shell_switches.cc` — Content Shell-specific switches
- The Zig Content Shell launch to see if `--single-process` is passed

**Expected outcome:** Content Shell likely uses multi-process by default (it's a
testing tool for the Content API). But this must be verified.

#### Verification

Research is complete when all three questions have clear answers with specific
file paths and line numbers from the Chromium source. The answers will determine
what Experiment 2 should be:

- If processes are shared → Experiment 2 forces separate processes
- If processes are separate but compositing is single-threaded → Experiment 2
  enables threaded compositing
- If processes are separate and compositing is threaded → the contention is
  deeper and Experiment 2 instruments the Blink scheduler

**Result:** All three questions answered. The architecture is fully isolated —
and the bottleneck is not where we expected.

#### Answer 1: Separate renderer processes (guaranteed)

Two BrowserContexts **always** get separate renderer processes. This is a hard
architectural constraint, not a configuration option.

`IsSuitableHost()` in `render_process_host_impl.cc:4696-4697` performs the
BrowserContext check as the **first** filter:

```cpp
if (host->GetBrowserContext() != browser_context)
    return false;
```

If the BrowserContexts don't match, the process is immediately unsuitable. All
process tracking data structures (`SiteProcessCountTracker`,
`GetSiteProcessMapForBrowserContext()`) are keyed per-BrowserContext. Content
Shell does not override this. Every reuse path — `kProcessPerSite`, reusable
subframe, empty background host, embedder preference — checks BrowserContext
first.

This means each profile has its own Blink main thread. Two rAF loops are NOT
fighting for one thread.

#### Answer 2: BeginMainFrame dispatch is per-process, no cross-process serialization

The BeginMainFrame path:

1. `Scheduler` fires on the compositor thread
2. `ProxyImpl::ScheduledActionSendBeginMainFrame` (`cc/trees/proxy_impl.cc:741`)
   builds a `BeginMainFrameAndCommitState` and PostTasks to the main thread
3. `ProxyMain::BeginMainFrame` (`cc/trees/proxy_main.cc:139`) runs on the main
   thread, calls `LayerTreeHost::BeginMainFrame`
4. `Page::Animate` (`page.cc:1532`) → `PageAnimator::ServiceScriptedAnimations`
   (`page_animator.cc:66`) → rAF callbacks execute

Architecture: one compositor thread per renderer process
(`render_thread_impl.cc:736`). Each WebContents gets its own `LayerTreeHost`
with its own `Scheduler`. Within a renderer process, multiple schedulers post to
the same main thread task queue (FIFO). But since two BrowserContexts get
separate renderer processes, this intra-process serialization is irrelevant.

There is no cross-process serialization in the BeginMainFrame path.

#### Answer 3: Content Shell uses full multi-process, threaded compositing

Content Shell uses:

- **Multi-process** — no `--single-process` flag, default process model
- **Out-of-process compositing** via the Viz process
  (`viz_process_transport_factory.cc`)
- **GPU-accelerated compositing** with dedicated compositor threads
- **Threaded compositing** — `LayerTreeHost::CreateThreaded()` in production
  (`layer_tree_view.cc:114-120`)

Content Shell does not override `ShouldUseProcessPerSite()`, does not disable
GPU compositing, does not enable single-threaded compositing. It inherits full
Chromium defaults.

#### Conclusion

The architecture is fully isolated:

| Resource          | Shared? | Evidence                                 |
| ----------------- | ------- | ---------------------------------------- |
| Renderer process  | No      | `IsSuitableHost()` checks BrowserContext |
| Blink main thread | No      | One per renderer process                 |
| Compositor thread | No      | One per renderer process                 |
| Scheduler         | No      | One per LayerTreeHost                    |
| BeginMainFrame    | No      | PostTask within each renderer process    |
| GPU/Viz process   | **Yes** | Single Viz process for all compositors   |

Two BrowserContexts get separate renderer processes, separate main threads,
separate compositor threads, separate schedulers. Yet a trivial rAF loop in both
degrades to 2fps. **The contention is in a shared resource outside the renderer
processes** — most likely the GPU/Viz process, which is the only shared
component in the pipeline.

This changes the investigation direction. The Blink main thread scheduler is not
the culprit. The next experiment should investigate the GPU/Viz process: how it
serializes frame submissions from multiple renderer processes, and whether GPU
command buffer contention or swap chain scheduling explains the 2fps
degradation. The key question is why CSS animations (which also go through the
Viz process) are unaffected while JavaScript animations are not — the difference
must be in what the renderer submits, not how the Viz process handles it.

### Experiment 2: Verify process isolation empirically

Experiment 1 was a code analysis — it showed two BrowserContexts _should_ get
separate renderer processes. But we haven't verified this empirically. If our
Zig Content Shell shim is accidentally running in single-process mode (e.g., a
command line flag, an initialization order issue, or Content Shell defaulting
differently than expected), the code analysis is irrelevant and two rAF loops
would be fighting for one main thread.

This experiment has two parts: verify the process architecture, then investigate
the actual difference between CSS and JS frame submission.

#### Part A: Count processes

While the two-profile rAF box demo is running (Experiment 5 from Issue 621),
count the running processes.

1. Start the Bun server and launch the app with two profiles loading the JS box
   demo
2. Run `ps aux | grep "Zig Content Shell"` to list all processes
3. Count:
   - How many "Zig Content Shell Helper" processes exist? (these are renderer
     processes)
   - Is there a GPU process?
   - Is there a utility/network process?

**Expected if multi-process:** At least 4 processes — 1 browser, 2 renderers
(one per BrowserContext), 1 GPU. Possibly more (network, utility).

**Expected if single-process:** Only 1 process (or 1 browser + 1 GPU, no
separate renderers).

If single-process: the Experiment 1 code analysis was wrong for our case, and
the fix is to ensure multi-process mode. Design Experiment 3 to force
`--no-single-process` or fix the shim.

#### Part B: Research CSS vs JS frame submission

If Part A confirms multi-process, the mystery deepens. The next question is:
what does a CSS-animation renderer submit to Viz vs what does a JS-rAF renderer
submit?

CSS animations run entirely in the compositor thread. The compositor can produce
new CompositorFrames without waiting for the main thread — it interpolates
transform/opacity values on its own and submits directly to Viz.

JS rAF requires a **BeginMainFrame → main thread work → commit** cycle. The
compositor sends BeginMainFrame to the main thread, waits for JS to execute and
layout/paint to complete, then the main thread commits the result back to the
compositor, which then produces a CompositorFrame for Viz.

The commit step is the key difference. Research:

- `cc/trees/proxy_main.cc` — the commit path after BeginMainFrame completes.
  Does it block the compositor thread?
- `cc/trees/proxy_impl.cc` — how the compositor handles pending commits. Does it
  stop requesting BeginFrames while waiting for a commit?
- `cc/scheduler/scheduler.cc` — the state machine that decides when to send
  BeginMainFrame vs when to draw. Does `COMMIT_STATE_WAITING_FOR_FIRST_DRAW` or
  similar block new BeginFrames?

The hypothesis: when the main thread is involved, the compositor-to-Viz
submission rate drops because the compositor blocks waiting for commits. With
two renderers both in this state, some shared resource (Viz frame scheduling,
BeginFrame source) sees both as "slow" and throttles them. With CSS-only, the
compositor never blocks, so the shared resource never sees contention.

#### Verification

Part A is complete when the process count is known. Part B is complete when the
commit-blocking behavior is understood with file paths and line numbers.
Together they determine whether Experiment 3 should fix the process model, fix a
commit bottleneck, or investigate the Viz process further.

#### Part A result: Multi-process confirmed (8 processes)

```
PID    Type              Notes
72083  Browser           Zig Content Shell (main process)
72136  GPU               --type=gpu-process
72138  Network utility   --type=utility (network.mojom.NetworkService)
72140  Renderer          --type=renderer, client-id=5
72142  Renderer          --type=renderer, client-id=7
72146  Renderer          --type=renderer, client-id=11
72147  Renderer          --type=renderer, client-id=12
72151  Storage utility   --type=utility (storage.mojom.StorageService)
```

**4 renderer processes** for 2 tabs. 2 are active (client IDs 5 and 7, one per
BrowserContext). 2 are spare/prewarmed renderers created by Chromium's
`SpareRenderProcessHostManager` (`content/common/features.cc:479` —
`kMultipleSpareRPHs` enabled by default). Spares are created proactively after
tabs load and when the browser goes idle.

Multi-process is confirmed. Each BrowserContext has its own renderer process
with its own Blink main thread and compositor thread. The code analysis from
Experiment 1 matches reality.

Notable flag on all renderers: `--enable-main-frame-before-activation`.

#### Part B result: Root cause identified

**The scheduler state machine blocks BeginMainFrame while a pending tree
exists.**

The critical code is in `scheduler_state_machine.cc:627-633`
(`ShouldSendBeginMainFrame()`):

```cpp
bool can_send_main_frame_with_pending_tree =
    settings_.main_frame_before_activation_enabled ||
    current_pending_tree_is_impl_side_;
if (has_pending_tree_ && !can_send_main_frame_with_pending_tree)
  return false;
```

When JS rAF fires, the main thread commits a new layer tree. This sets
`has_pending_tree_ = true` (`scheduler_state_machine.cc:1042`). Until the
pending tree activates, the scheduler **blocks the next BeginMainFrame**. This
means the compositor cannot start a new frame cycle — it must wait for the
current commit to fully activate before requesting main thread work again.

**CSS animations bypass this entirely.** They run in the compositor thread via
property trees (`cc::AnimationTimeline`). No BeginMainFrame is sent, no commit
is created, no pending tree exists. The compositor produces new CompositorFrames
directly from the active tree on each BeginFrame.

**The `--enable-main-frame-before-activation` flag** is the pipelining control:

- `content/browser/gpu/compositor_util.cc` —
  `IsMainFrameBeforeActivationEnabled()` checks
  `base::SysInfo::NumberOfProcessors() < 4` and returns `false` on machines with
  fewer than 4 cores
- When enabled (≥4 processors), `main_frame_before_activation_enabled = true`
  allows the scheduler to send BeginMainFrame while a pending tree exists
- When disabled, each context can only process every _other_ BeginFrame

**But wait — `--enable-main-frame-before-activation` IS present on all renderer
processes.** The flag is on the command line. This means
`main_frame_before_activation_enabled` should be `true`, and the check at line
632 should pass. If pipelining is active, the pending tree should not block the
next BeginMainFrame.

This means the scheduler state machine is NOT the blocking point — the flag is
enabled. The bottleneck is elsewhere. But the research identified the exact
architecture: JS rAF creates pending trees and CSS animations don't. Something
downstream of the pending tree — the activation step, the draw step, or the
frame submission to Viz — is where the contention lies.

#### Conclusion

Multi-process confirmed. The `--enable-main-frame-before-activation` flag is
active, so the scheduler state machine should allow pipelined commits. Yet 2fps
persists. The pending tree / activation pipeline is the right area but the
blocking point is not the `ShouldSendBeginMainFrame()` gate.

The remaining suspects:

1. **Activation itself** — even with pipelining enabled, activation may be slow
   when the Viz process is shared. The pending tree activates only after the
   previous frame is drawn and the Viz process acknowledges it.
2. **Draw throttling** — `SchedulerStateMachine::ShouldDraw()` or
   `ShouldAbortCurrentFrame()` may block draws when pending swaps accumulate
   across two renderers sharing one GPU process.
3. **The `has_pending_tree_` flag may still be true despite the pipelining
   flag** — if `current_pending_tree_is_impl_side_` is true, it bypasses the
   check differently. Need to verify the actual runtime state.

The next experiment should instrument the scheduler state machine to trace why
frames are being dropped — specifically `ShouldSendBeginMainFrame()`,
`ShouldDraw()`, and `has_pending_tree_` state at each BeginFrame.

### Experiment 3: Two windows, same profile, JS box demo

Before instrumenting the scheduler, there's a simpler experiment that could
change the entire investigation. Issue 620 Experiment 11 tested two windows in
the **same** BrowserContext with google.com and got 60fps. But that was with
google.com — we never tested same-profile with the JS box demo specifically.

This experiment creates two windows in one BrowserContext (no second profile),
both loading the JS box demo. It answers: **is the 2fps caused by multiple
BrowserContexts, or by multiple windows with rAF?**

The distinction matters because:

- **Two BrowserContexts** → two renderer processes (separate main threads,
  separate compositor threads). The contention would be in a shared resource
  outside the renderers (GPU/Viz process).
- **One BrowserContext, two windows** → the two windows may share a renderer
  process (same origin, same profile). The contention would be intra-process
  (scheduler state machine, main thread serialization).

If same-profile is 60fps: the problem is specifically multi-BrowserContext.
Something about having two separate renderer processes causes contention in the
shared GPU/Viz process that doesn't happen when windows share a renderer.

If same-profile is 2fps: the problem is multi-window with rAF regardless of
profile. BrowserContext is irrelevant and the bottleneck is in how Chromium
handles two concurrent rAF loops (possibly intra-process scheduler contention).

#### Changes

**`content_api_shim.mm`** — revert to single BrowserContext, open two windows:

```cpp
// One profile, two windows, JS box demo (requestAnimationFrame).
const char* kWindowAUrl = "http://localhost:9616/test-box-demo.html";
const char* kWindowBUrl = "http://localhost:9616/test-box-demo.html";

class TsBrowserMainParts : public content::ShellBrowserMainParts {
 protected:
  void InitializeMessageLoopContext() override {
    content::Shell::CreateNewWindow(browser_context(), GURL(kWindowAUrl),
                                    nullptr, gfx::Size());
    content::Shell::CreateNewWindow(browser_context(), GURL(kWindowBUrl),
                                    nullptr, gfx::Size());
  }
};
```

No `InitializeBrowserContexts` override, no second BrowserContext, no
`PostMainMessageLoopRun` override. Parent class handles all profile setup. Two
windows share the default profile.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Start the Bun server (if not already running):
   ```bash
   cd ~/dev/termsurf/test-html && bun run server.ts
   ```
2. Launch the app:
   ```bash
   open out/Default/Zig\ Content\ Shell.app
   ```
3. Two Content Shell windows appear, each showing a spinning blue square with
   FPS counter
4. Observe:
   - Are both spinning smoothly (FPS counter shows ~60)?
   - Are both stuttering (FPS counter shows ~2)?
   - Do the identity strings match (confirming same profile)?
5. Also count processes: `ps aux | grep "Zig Content Shell"` — how many
   renderers?
6. Close both windows — process exits cleanly

If both ~60fps: multi-BrowserContext is the trigger. Two separate renderer
processes contend in the GPU/Viz process in a way that one shared renderer does
not.

If both ~2fps: rAF + multiple windows is sufficient. BrowserContext is
irrelevant and the problem is in the scheduler or compositor handling of
multiple concurrent rAF loops within one renderer process.

**Result:** Both 60fps

Both windows rendered the spinning blue square at 60fps. On first launch,
identity strings differed due to a race condition — both windows loaded
simultaneously and called `localStorage.getItem()` before either wrote. On
second launch, both windows displayed the same identity string, confirming they
share the same BrowserContext and localStorage. 4 renderer processes total (2
active + 2 spare). Same profile, same origin, same rAF loop — both smooth.

This is the definitive fork:

| Configuration                | rAF   | FPS     | Experiment |
| ---------------------------- | ----- | ------- | ---------- |
| 2 BrowserContexts, 2 windows | yes   | 2 + 2   | 621.5      |
| 1 BrowserContext, 2 windows  | yes   | 60 + 60 | 622.3      |
| 2 BrowserContexts, CSS only  | no JS | 60 + 60 | 621.4      |

**The 2fps requires BOTH conditions: multiple BrowserContexts AND JavaScript.**
Multiple windows with rAF in the same BrowserContext is fine. Multiple
BrowserContexts with CSS-only is fine. Only the combination triggers the
degradation.

This eliminates the GPU/Viz process as the sole bottleneck — if GPU contention
were the cause, same-profile two-window rAF would also degrade (both
configurations have separate renderer processes submitting frames to the same
GPU process). The problem is specific to the multi-BrowserContext code path.

#### Conclusion

The 2fps degradation requires multi-BrowserContext + JavaScript. Same-profile
rAF is 60fps. The bottleneck is not in the renderer process (each has its own
threads), not in the GPU/Viz process (both configurations share it), and not in
the scheduler state machine (same behavior in both cases). It must be in
**browser-process-level coordination** that differs between
single-BrowserContext and multi-BrowserContext — something in how the browser
process manages frame scheduling, BeginFrame distribution, or compositor frame
sink setup when multiple BrowserContexts exist.

### Experiment 4: Research browser-process BrowserContext coordination

A source code research experiment — no code changes, no builds. Experiment 3
proved the bottleneck is in the browser process: same-profile rAF is 60fps,
multi-profile rAF is 2fps, yet both configurations share the same GPU/Viz
process and both have separate renderer processes. Something in the browser
process treats multiple BrowserContexts differently from one.

This experiment searches the Chromium source for browser-process code paths that
diverge based on BrowserContext count or identity — specifically in frame
scheduling, BeginFrame distribution, and compositor frame sink management.

#### Question 1: How does the browser process distribute BeginFrames?

The Viz process generates BeginFrame signals at vsync. These reach renderer
processes through the browser process. If the browser process serializes or
throttles BeginFrame delivery per BrowserContext, JS-heavy renderers (which need
the full BeginMainFrame → commit → activate → draw cycle) would stall while
CSS-only renderers (which draw immediately on BeginFrame) would be unaffected.

**Where to look:**

- `components/viz/host/host_frame_sink_manager.cc` — the browser-process
  interface to the Viz frame sink system. Does it track BrowserContext?
- `content/browser/compositor/viz_process_transport_factory.cc` — how the
  browser creates and configures frame sinks
- `content/browser/renderer_host/render_widget_host_impl.cc` — the browser-side
  proxy for each renderer's compositor. Does it throttle BeginFrames?
- `content/browser/renderer_host/render_widget_host_view_mac.mm` — macOS
  platform layer. Does it manage its own display link per view?
- `components/viz/service/frame_sinks/frame_sink_manager_impl.cc` — the Viz-side
  frame sink manager. Search for anything keyed by client/context identity

**Key signal:** Look for any code that groups, batches, or serializes frame
sinks by BrowserContext or StoragePartition. If frame sinks from different
BrowserContexts are processed in separate batches while same-BrowserContext
sinks are processed together, that would explain the asymmetry.

#### Question 2: Does RenderWidgetHostImpl throttle pending frames per BrowserContext?

`RenderWidgetHostImpl` sits between the renderer and the Viz process in the
browser. It may limit the number of in-flight (pending) frames per renderer.
When JS rAF is active, each frame requires a full main-thread round-trip — the
renderer is slower to acknowledge frames. If `RenderWidgetHostImpl` has a
per-BrowserContext pending frame limit, two slow renderers from different
BrowserContexts could starve each other in a way that two renderers from the
same BrowserContext do not.

**Where to look:**

- `content/browser/renderer_host/render_widget_host_impl.cc` — search for
  `pending`, `throttle`, `max_pending`, `DidReceiveCompositorFrame`,
  `SubmitCompositorFrame`
- `content/browser/renderer_host/render_widget_host_delegate.cc` — delegation
  that might differ per BrowserContext
- `content/browser/renderer_host/frame_token_message_queue.cc` — frame token
  management and acknowledgment flow

#### Question 3: What browser-process infrastructure is per-BrowserContext vs global?

Map out which browser-process objects are created per BrowserContext vs shared
globally. This is the structural question — if something that should be
per-context is actually global (or vice versa), that's a candidate for the
contention point.

**Where to look:**

- `content/browser/browser_context.cc` — what does BrowserContext own?
- `content/browser/storage_partition_impl.cc` — StoragePartition is
  per-BrowserContext; does it own any compositor infrastructure?
- `content/browser/gpu/gpu_process_host.cc` — is the GPU process host global or
  per-context? How are GPU channels allocated?
- `content/browser/gpu/compositor_util.cc` — compositor configuration that might
  differ per context
- Search for `GetBrowserContext()` calls in `renderer_host/` — any code that
  branches on BrowserContext identity in the frame submission path

**Key signal:** Find any frame-related resource that is allocated per
BrowserContext in multi-context but shared in single-context. That structural
difference is likely the bottleneck.

#### Verification

Research is complete when all three questions have answers with file paths and
line numbers. The answers should identify either:

1. A specific throttling mechanism in the browser process that activates with
   multiple BrowserContexts — leading to Experiment 5 that disables or fixes it
2. A structural difference in how frame sinks or BeginFrame sources are
   allocated per BrowserContext — leading to Experiment 5 that changes the
   allocation
3. No BrowserContext-specific logic found — meaning the contention is in the Viz
   process itself (not the browser process), and Experiment 5 should instrument
   Viz frame scheduling

**Result:** All three questions answered. No BrowserContext-specific frame logic
exists — but two strong candidate mechanisms identified.

#### Answer 1: BrowserContext is invisible to the BeginFrame pipeline

"BrowserContext" appears in **zero files** across all of `components/viz/` and
`cc/`. The entire BeginFrame delivery system — from display link to
`ExternalBeginFrameSource` to `FrameSinkManagerImpl` to
`CompositorFrameSinkSupport` to renderer `cc::Scheduler` — has no concept of
BrowserContext, StoragePartition, or browser-level profile identity.

The BeginFrame flow on macOS:

1. A single `DisplayLinkMacMojo` (`ui/compositor/display_link_mac_mojo.mm:209`)
   runs on a dedicated "VSyncThread" in the browser process, issuing VSync to
   Viz via Mojo
2. `ExternalBeginFrameSourceMojoMac`
   (`components/viz/service/frame_sinks/external_begin_frame_source_mojo_mac.cc:48-53`)
   receives it in the Viz process
3. `VSyncProviderMac` (`ui/display/mac/vsync_provider_mac.cc:109-128`)
   distributes to all registered callbacks for that display
4. Each window's `ExternalBeginFrameSourceMac`
   (`components/viz/service/frame_sinks/external_begin_frame_source_mac.cc:240-318`)
   fires
5. `ExternalBeginFrameSource::OnBeginFrame`
   (`components/viz/common/frame_sinks/begin_frame_source.cc:558-597`) iterates
   a flat set of all observers — no batching, no grouping, no identity checks
6. Each `CompositorFrameSinkSupport::OnBeginFrame`
   (`components/viz/service/frame_sinks/compositor_frame_sink_support.cc:1138-1233`)
   decides whether to forward to its renderer client

All data structures are keyed by `FrameSinkId` (an integer pair). Nothing
carries BrowserContext information.

#### Answer 2: Three throttling mechanisms, none per-BrowserContext

RenderWidgetHostImpl has **no** frame throttling or backpressure. Its only
throttle is `visual_properties_ack_pending_` for resize events
(`render_widget_host_impl.cc:1293-1296`), irrelevant to frame pacing.

The real throttling lives in three places:

**A. Renderer-side: `kMaxPendingSubmitFrames = 1`**
(`cc/scheduler/scheduler_state_machine.cc:32`)

After submitting one CompositorFrame to Viz, the renderer blocks draws, commits,
**and BeginMainFrame** (which triggers rAF) until Viz acknowledges:

```cpp
// scheduler_state_machine.cc:670-679
bool just_submitted_in_deadline =
    begin_impl_frame_state_ == BeginImplFrameState::INSIDE_DEADLINE &&
    did_submit_in_last_frame_;
if (IsDrawThrottled() && !just_submitted_in_deadline)
  return false;  // blocks ShouldSendBeginMainFrame
```

`IsDrawThrottled()` returns true when
`pending_submit_frames_ >=
kMaxPendingSubmitFrames` (line 1602-1607). This is
per-renderer-process.

**B. Viz-side: undrawn frame throttling (`kUndrawnFrameLimit = 3`)**
(`components/viz/service/frame_sinks/compositor_frame_sink_support.h:81`,
`compositor_frame_sink_support.cc:1489-1495`)

If a client submits more than 3 frames that haven't been drawn, BeginFrames are
throttled. Per-FrameSink, not per-BrowserContext.

**C. Viz-side: unresponsive client throttling (`kLimitThrottle = 10`)**
(`components/viz/service/frame_sinks/begin_frame_tracker.h:23-24`,
`compositor_frame_sink_support.cc:1531-1534`)

After 10+ outstanding unanswered BeginFrames, Viz throttles sending more. After
100, it stops entirely. Per-FrameSink.

#### Answer 3: Zero frame-related resources are per-BrowserContext

BrowserContext owns storage infrastructure (StoragePartitionImplMap,
NetworkContext, cookies, IndexedDB, ServiceWorker) — **none** of which are
frame-related. All compositor infrastructure is either global or
per-renderer-process:

| Resource                        | Scope                              | Frame-related? |
| ------------------------------- | ---------------------------------- | -------------- |
| `HostFrameSinkManager`          | Global (one per `BrowserMainLoop`) | Yes            |
| `FrameSinkManagerImpl`          | Global (in Viz process)            | Yes            |
| `GpuProcessHost`                | Global singleton                   | Yes            |
| `ImageTransportFactory`         | Global singleton                   | Yes            |
| `SharedImageManager`            | Global (in GPU process)            | Yes            |
| `GpuClient` / GPU channel       | Per-renderer-process               | Yes            |
| `EmbeddedFrameSinkProviderImpl` | Per-renderer-process               | Yes            |
| `StoragePartitionImplMap`       | Per-BrowserContext                 | No             |
| `NetworkContext`                | Per-StoragePartition               | No             |

The only BrowserContext-specific code in the frame path is renderer process
allocation (`IsSuitableHost()` at `render_process_host_impl.cc:4696`), which
**forces** separate renderer processes for different BrowserContexts.

#### Two candidate mechanisms

**Theory A: Viz Display serialization**

The `DisplayScheduler` waits for pending surfaces before drawing
(`display_damage_tracker.cc:135-170`). When two renderer processes both receive
BeginFrames, the Display waits for **both** to respond before drawing either.
Each renderer has `kMaxPendingSubmitFrames = 1` — it blocks rAF until Viz
acknowledges the previous frame. The ack arrives only after the Display draws.
This creates a cross-process waiting chain:

1. Both renderers receive BeginFrame simultaneously
2. Renderer A finishes rAF, submits frame, blocks waiting for ack
3. Display waits for Renderer B (still running rAF)
4. Renderer B finishes, submits, Display draws both, sends acks
5. Both renderers can now start their next frame — but the entire vsync interval
   was spent waiting

CSS animations bypass this because the compositor responds to BeginFrame
immediately without waiting for the main thread. Same-BrowserContext bypasses
this because both WebContents share one renderer process, so frames are batched
into a single submission.

**Theory B: macOS process backgrounding**

`kMacAllowBackgroundingRenderProcesses` (`render_process_host_impl.cc:5717`)
controls whether renderer processes get backgrounded by macOS. When one
renderer's window doesn't have focus, macOS may background that process,
drastically throttling its main thread:

- CSS animations: immune (compositor thread is not throttled by backgrounding)
- JS rAF: affected (main thread IS throttled)
- Same-BrowserContext: immune (one process, both WebContents in the foreground
  process)
- Multi-BrowserContext: affected (two processes, the unfocused one gets
  backgrounded)

This explains the ~2fps magnitude — macOS App Nap and timer coalescing can
throttle background processes to ~1-2 wakeups per second.

Additionally, renderer process priority is managed per-process
(`UpdateProcessPriority` at `render_process_host_impl.cc:5659`). Each renderer
has its own V8 isolate priority (`SetIsolatePriority` at
`render_thread_impl.cc:1210`) and `MainThreadSchedulerImpl` backgrounding state
(`main_thread_scheduler_impl.cc:1088`). When a renderer process is backgrounded,
its entire Blink scheduler shifts to low-priority mode.

#### Conclusion

No BrowserContext-specific frame logic exists in the browser process. The
BrowserContext boundary manifests solely through renderer process isolation. The
lead candidate is **Viz Display serialization**: the `DisplayScheduler` waits
for all pending surfaces before drawing (`HasPendingSurfaces`), and each
renderer blocks on `kMaxPendingSubmitFrames = 1` until Viz acknowledges the
previous frame. Two separate renderer processes with slow JS round-trips create
a cross-process waiting chain through the shared Display. This would show up as
delayed acks. Testable by disabling surface waiting or increasing
`kMaxPendingSubmitFrames`.

## Conclusion

Issue 622 ran four experiments — one empirical test and three Chromium source
code research sessions — to pinpoint why JavaScript-driven rendering degrades to
2fps when two BrowserContexts coexist in a single Chromium process.

### What we learned

**The 2fps requires BOTH conditions: multiple BrowserContexts AND JavaScript.**

| Configuration                | rAF | FPS     | Experiment |
| ---------------------------- | --- | ------- | ---------- |
| 2 BrowserContexts, 2 windows | yes | 2 + 2   | 621.5      |
| 1 BrowserContext, 2 windows  | yes | 60 + 60 | 622.3      |
| 2 BrowserContexts, CSS only  | no  | 60 + 60 | 621.4      |

This is the definitive fork. Neither condition alone triggers the degradation.

**BrowserContext is invisible to the frame pipeline.** The word "BrowserContext"
appears in zero files across all of `components/viz/` and `cc/`. The entire
BeginFrame delivery system, frame sink management, and compositor scheduling
have no concept of browser profiles. The BrowserContext boundary manifests
solely through one mechanism: renderer process isolation (`IsSuitableHost()`
forces separate processes for different BrowserContexts).

**The architecture is fully isolated.** Each BrowserContext gets its own
renderer process with its own Blink main thread, compositor thread, scheduler,
and BeginMainFrame dispatch. There is no cross-process serialization in the
renderer layer. The only shared resources are the GPU/Viz process and the
browser process — both of which are BrowserContext-unaware.

**Three throttling mechanisms were identified, none per-BrowserContext:**

1. `kMaxPendingSubmitFrames = 1` — per-renderer backpressure that blocks rAF
   until Viz acknowledges the previous frame
2. `kUndrawnFrameLimit = 3` — per-FrameSink throttle if undrawn frames pile up
3. `BeginFrameTracker kLimitThrottle = 10` — per-FrameSink unresponsive client
   detection

### Lead theory for Issue 623

**Viz Display serialization.** The `DisplayScheduler` waits for all pending
surfaces before drawing (`HasPendingSurfaces`). Two separate renderer processes
with slow JS round-trips create a cross-process waiting chain through the shared
Display. Each renderer blocks on `kMaxPendingSubmitFrames = 1` while the Display
waits for the other. CSS animations bypass this because the compositor responds
immediately without a main-thread round-trip. Same-BrowserContext bypasses this
because both WebContents share one renderer and batch their submissions.

This theory qualitatively explains all three experimental cases but has a
magnitude gap: simple cross-process waiting should produce ~30fps, not 2fps.
Either there is a cascading feedback loop that amplifies the initial delay, or
the real mechanism is something else that correlates with separate renderer
processes + main-thread involvement.

Issue 623 will investigate.
