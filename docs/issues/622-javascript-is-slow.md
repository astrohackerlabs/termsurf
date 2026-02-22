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
