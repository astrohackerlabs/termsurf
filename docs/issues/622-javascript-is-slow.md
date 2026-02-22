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
