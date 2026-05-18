+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 621: Single Process Multi-Profile Performance

## Goal

Make two browser profiles (BrowserContext instances) render at 60fps in a single
Chromium process. Issue 620 proved the architecture works — a thin C++ shim
drives the Content API through a C function boundary — but complex pages
(google.com) degrade to 2fps when a second BrowserContext exists. This issue
investigates and fixes that degradation.

## Background

Issue 620 built a minimal Chromium embedder (3 files, ~190 lines) that loads web
pages through the Content API via a C function boundary. 15 experiments
systematically narrowed a 2fps rendering throttle:

### What works

| Configuration        | FPS   | Issue 620 Experiments |
| -------------------- | ----- | --------------------- |
| 1 profile, 1 window  | 60fps | 1, 10                 |
| 1 profile, 2 windows | 60fps | 11                    |

### What doesn't work

| Configuration                | FPS   | Issue 620 Experiments |
| ---------------------------- | ----- | --------------------- |
| 2 profiles, any window count | 2fps  | 2–9, 12               |
| 2 profiles, DDG + google.com | mixed | 14–15                 |

Experiments 14–15 revealed that the 2fps is page-dependent: lite.duckduckgo.com
renders at 60fps while google.com renders at 2fps, regardless of which profile
is created first. This was consistent across 6 runs in both URL orderings.

### What was eliminated in Issue 620

- **C API wrappers** — identical 2fps with and without them (Exp 8 vs 9)
- **Callback/lifecycle machinery** — identical 2fps (Exp 7 vs 8)
- **Direct WebContents vs Shell::CreateNewWindow** — identical (Exp 5–7)
- **Custom launcher vs stock shell_main_mac.cc** — identical (Exp 8)
- **Creation order** — 5 runs, always google slow, DDG fast (Exp 15)
- **ExternalBeginFrameSourceMac thrashing** — found in Exp 12, fixed in Exp 13,
  but 2fps persisted
- **BeginFrameTracker throttle/stop** — never fired (0 events in Exp 14)
- **ShouldSendBeginFrame gate** — 9 rejections in 90s, all during init (Exp 14)
- **DisplayScheduler ShouldDraw conditions** — output_surface_lost, visible,
  root_frame_missing all healthy; only `needs_draw=0` fails (Exp 14)

### Prior art: Issue 413

Issue 413 (ts4 era) found the same boundary: two WebContents from different
BrowserContexts = 2fps, two WebContents from the same BrowserContext = 60fps.
The conclusion at the time was that multi-profile-in-one-process doesn't work
and each profile needs its own process. This issue challenges that conclusion by
investigating whether the degradation can be fixed.

## Chromium internals research

The following architecture was mapped during Issue 620's investigation.

### Vsync and frame delivery chain

```
CADisplayLinkMac / CVDisplayLinkMac (macOS vsync driver)
  → ExternalBeginFrameSourceMac (per-compositor begin frame source)
    → ExternalBeginFrameSource::AddObserver/RemoveObserver
      → DisplayScheduler::OnBeginFrame (schedules deadline)
        → DisplayScheduler::AttemptDrawAndSwap
          → ShouldDraw() gate (needs_draw_, visible, output_surface_lost,
                               root_frame_missing)
          → DrawAndSwap() → actual pixel output
      → CompositorFrameSinkSupport::OnBeginFrame (forwards to renderer)
        → ShouldSendBeginFrame() gate (multiple throttle checks)
        → client_->OnBeginFrame() (Mojo IPC to renderer process)
```

### Throttle mechanisms investigated

| Mechanism                         | Location                         | Threshold            | Status in Issue 620      |
| --------------------------------- | -------------------------------- | -------------------- | ------------------------ |
| StopObservingBeginFrames          | display_scheduler.cc:603         | ShouldDraw()=false   | Fixed (Exp 13), not root |
| BeginFrameTracker::ShouldThrottle | begin_frame_tracker.cc           | outstanding >= 10    | Never triggered          |
| BeginFrameTracker::ShouldStop     | begin_frame_tracker.cc           | outstanding >= 100   | Never triggered          |
| kUndrawnFrameLimit                | compositor_frame_sink_support.cc | undrawn > 3          | Never triggered          |
| SetIsGpuBusy                      | display_scheduler.cc:644         | pending_swaps >= max | Not investigated         |
| client_needs_begin_frame_         | compositor_frame_sink_support.cc | client stopped       | 9 events (init only)     |

### The observer–renderer deadlock (found and partially fixed)

When `DisplayScheduler::AttemptDrawAndSwap()` calls `StopObservingBeginFrames()`
after `ShouldDraw()` returns false, the renderer loses BeginFrames and can't
produce new CompositorFrames. Without new frames, no damage arrives, and the
compositor stays stopped. This creates a deadlock that manifests as ~3fps
(register → draw 1–2 frames → unregister → wait ~300ms → repeat).

Experiment 13 fixed this by commenting out `StopObservingBeginFrames()`. The
vsync pipeline stayed alive, but rendering was still 2fps. The deadlock was a
symptom, not the root cause.

### The real bottleneck: renderer not submitting frames

After fixing the observation deadlock, the DisplayScheduler receives BeginFrames
at 60fps but `needs_draw_` stays false — no damage arrives from the renderer.
The renderer receives BeginFrames (via
`CompositorFrameSinkSupport::OnBeginFrame`) but only produces CompositorFrames
at ~3fps for complex pages. Lightweight pages are unaffected.

### Key source files

| File                               | Role                                                         |
| ---------------------------------- | ------------------------------------------------------------ |
| cv_display_link_mac.mm             | macOS CVDisplayLink vsync driver                             |
| ca_display_link_mac.mm             | macOS CADisplayLink vsync driver (newer macOS)               |
| external_begin_frame_source_mac.cc | Per-compositor vsync source, registers/unregisters callbacks |
| begin_frame_source.cc              | Observer management, OnNeedsBeginFrames dispatch             |
| display_scheduler.cc               | Draw timing, ShouldDraw gate, AttemptDrawAndSwap             |
| display_damage_tracker.cc          | Tracks root_frame_missing, surface damage                    |
| compositor_frame_sink_support.cc   | Forwards BeginFrames to renderer, throttle gates             |
| begin_frame_tracker.cc             | Tracks outstanding BeginFrames for throttle/stop             |
| root_compositor_frame_sink_impl.cc | Creates ExternalBeginFrameSourceMac per profile              |
| layer_tree_host_impl.cc            | Renderer-side compositor, WillBeginImplFrame, damage check   |
| scheduler_state_machine.h          | Renderer scheduler state, throttle_frame_rate_               |
| shared_image_manager.h             | GPU process SharedImageManager, serializes GPU access        |
| render_process_host_impl.cc        | Process allocation, process-per-site logic                   |
| page_scheduler_impl.cc             | Blink page scheduler, visibility/background throttling       |
| frame_rate_throttling.cc           | Browser-side frame sink throttling                           |

### Uninvestigated mechanisms

These are candidate throttle points that Issue 620 did not instrument:

**1. GPU channel serialization.** The GPU process has a single
`SharedImageManager` (gpu/command_buffer/service/shared_image/) that serializes
access to GPU resources across all contexts. When two renderers actively submit
GPU commands, they're serialized through lock-protected critical sections. This
could halve effective frame rate.

**2. Renderer-side early damage check.** `LayerTreeHostImpl::WillBeginImplFrame`
(cc/trees/layer_tree_host_impl.cc:3873) performs an early damage check that can
skip the entire frame:

```cpp
bool recent_frame_had_no_damage =
    consecutive_frame_with_damage_count_ < settings_.damaged_frame_limit;
if (settings_.enable_early_damage_check && recent_frame_had_no_damage &&
    CanDraw()) {
    // ... check for damage, skip frame if none found
}
```

Complex pages generate frequent damage, so
`consecutive_frame_with_damage_count_` stays high and the early check always
runs. Lightweight pages generate less damage, causing the counter to stay low
and the check to be skipped (allowing frames through without checking).

**3. Scheduler state machine throttling.**
`cc/scheduler/scheduler_state_machine.h` has `throttle_frame_rate_` and
`main_frame_throttled_interval_` that can explicitly throttle BeginMainFrame
delivery. If one renderer's scheduler enters throttled mode due to contention,
it would produce fewer frames.

**4. `SetIsGpuBusy` backpressure.** When `pending_swaps_ >= MaxPendingSwaps()`
(display_scheduler.cc:644), the DisplayScheduler calls
`begin_frame_source_->SetIsGpuBusy(true)`, which can throttle BeginFrame
delivery at the source level. With two profiles sharing the GPU, swap buffers
may back up past the threshold.

**5. Renderer process priority.** `RenderProcessHostPriorityClient`
(content/public/browser/render_process_host_priority_client.h) tracks
`is_hidden`, `frame_depth`, `intersects_viewport`. The OS scheduler distributes
CPU time according to process importance. If a renderer process is
deprioritized, its frame production drops.

**6. Browser-side frame rate throttling.**
`content/browser/performance_manager/frame_rate_throttling.cc` provides
`StartThrottlingAllFrameSinks()` and `UpdateThrottlingFrameSinks()`. If a
performance manager policy throttles frame sinks globally (e.g., for power
savings), all profiles would be affected.

**7. Blink main thread starvation.** The Blink main thread's `TaskQueueSelector`
(base/task/sequence_manager/task_queue_selector.h) uses priority-based
scheduling. In multi-context scenarios, if both renderers' main threads queue
BeginFrame-related tasks (style recalc, layout, paint), they compete for
execution time. The priority mechanism can cause one context's main thread work
to back up, delaying CompositorFrame submission.

## Chromium branch

`146.0.7650.0-issue-621` — branched from the vanilla `146.0.7650.0` tag. Fresh
start with no Issue 620 instrumentation. Code patterns from Issue 620's branch
(`146.0.7650.0-issue-620`) can be referenced but are not carried forward.

## Approach

Same as Issue 620: the Zig Content Shell is a thin C++ shim
(`content/zig_content_shell/`) that subclasses Content Shell's classes and
exports `ContentMain()` as `extern "C"`. The shim is built with GN/autoninja
inside `chromium/src/` and produces a macOS `.app` bundle.

The shim code (BUILD.gn, content_api_shim.h, content_api_shim.mm, plist files,
ts_main.mm) is copied from Issue 620's final state. The only change between
experiments will be `content_api_shim.mm` (URLs, profiles) and Chromium source
modifications (instrumentation, fixes).

## Ideas for experiments

### 1. Reproduce the baseline: single profile, google.com

Copy the Zig Content Shell files from Issue 620's branch to the new branch.
Configure for a single profile loading google.com. Verify 60fps rendering. This
establishes the baseline and confirms the build works on the fresh branch.

### 2. Reproduce the degradation: two profiles, google.com + DDG

Add a second BrowserContext (Profile B) loading lite.duckduckgo.com. Verify that
google.com degrades to 2fps while DDG stays at 60fps. This reproduces the Issue
620 finding and gives us a stable reproduction case. The two pages are easily
distinguishable (both have search inputs for typing tests).

### 3. Instrument renderer-side frame production

The viz compositor pipeline is clean (Issue 620 Exp 14 proved this). The
bottleneck is the renderer not submitting CompositorFrames. Instrument the
renderer side:

- **`cc/trees/layer_tree_host_impl.cc:WillBeginImplFrame`** — log whether the
  early damage check skips the frame, and what
  `consecutive_frame_with_damage_count_` is
- **`cc/scheduler/scheduler.cc`** — log when `SetShouldThrottleFrameRate()` is
  called and whether `throttle_frame_rate_` is true
- **`cc/trees/layer_tree_host_impl.cc:PrepareToDraw`** — log when the renderer
  actually prepares a draw vs skips

This would reveal whether the renderer skips frames at the cc layer.

### 4. Instrument GPU channel contention

If the renderer produces frames but they're slow to reach the GPU:

- **`gpu/command_buffer/service/shared_image/shared_image_manager.cc`** — log
  lock acquisition/contention
- **`display_scheduler.cc:DidSwapBuffers`** — log `pending_swaps_` count and
  whether `SetIsGpuBusy(true)` fires

This would reveal whether GPU serialization is the bottleneck.

### 5. Test with two google.com profiles

Load google.com on both profiles. If both degrade to 2fps, the problem is
complexity-dependent (any complex page triggers it). If only one degrades, the
problem is asymmetric contention (first or second profile specifically).

### 6. Test with two lite.duckduckgo.com profiles

Load DDG on both profiles. If both render at 60fps, the problem is
page-complexity-specific. If one degrades, even lightweight pages are affected
under certain conditions.

### 7. Disable renderer-side early damage check

Set `settings_.enable_early_damage_check = false` in the renderer's
LayerTreeSettings. If the early damage check
(`layer_tree_host_impl.cc:3941–3952`) is causing frame skips for the complex
page when contention exists, disabling it would let all frames through.

### 8. Separate GPU channels per BrowserContext

Investigate whether two BrowserContexts can use separate GPU channels or command
buffer streams. If GPU serialization is the bottleneck, this would eliminate the
contention. Look at `GpuProcessHost::EstablishGpuChannel()` in
`content/browser/gpu/gpu_process_host.cc`.

### 9. Adjust MaxPendingSwaps threshold

`DisplayScheduler::MaxPendingSwaps()` (display_scheduler.cc:357) returns
different values based on display refresh rate. If
`pending_swaps_ >=
MaxPendingSwaps()`, the GPU is marked busy and BeginFrame
delivery throttles. Increasing the threshold might prevent premature GPU busy
signals when two profiles share the same GPU.

### 10. Disable frame rate throttling in the scheduler state machine

The renderer's `SchedulerStateMachine` has `throttle_frame_rate_` and
`main_frame_throttled_interval_`. If these are being set due to multi-context
contention, disabling them would force the renderer to produce frames at full
speed.

### 11. Process-per-BrowserContext investigation

Check whether two BrowserContexts get separate renderer processes or share one.
`RenderProcessHostImpl::GetProcessHostForSiteInstance()` in
`content/browser/renderer_host/render_process_host_impl.cc` decides process
allocation. If they share a renderer, all frame production competes on one
process's threads. If they're separate, the contention is in the browser or GPU
process.

### 12. Profile with Chrome tracing

Run with
`--trace-startup=cc,viz,gpu --trace-startup-duration=10
--trace-startup-file=/tmp/trace.json`.
Load the trace in `chrome://tracing` or Perfetto UI. The trace will show exactly
where time is spent per frame for each compositor, revealing whether the
bottleneck is in cc scheduling, GPU command submission, or Blink main thread
work.

## Experiments

### Experiment 1: Reproduce the baseline — single profile, google.com

Establish the baseline on a fresh Chromium branch. Copy the Zig Content Shell
files from Issue 620's branch (`146.0.7650.0-issue-620`, commit `e69a931` — the
clean single-profile state from Experiment 10). Load google.com in a single
profile. Verify 60fps rendering.

This confirms the build works on the fresh branch with zero Issue 620
instrumentation (no `[TS-DIAG]` logging in display_scheduler.cc,
begin_frame_tracker.cc, compositor_frame_sink_support.cc, or
external_begin_frame_source_mac.cc).

#### Setup

1. Create the new branch from the vanilla tag:

   ```bash
   cd ~/dev/termsurf/chromium/src
   git checkout -b 146.0.7650.0-issue-621 146.0.7650.0
   ```

2. Copy the `content/zig_content_shell/` directory from Issue 620's Experiment
   10 commit. The directory contains 7 files:

   - `BUILD.gn` — GN build target (app bundle + framework + helpers)
   - `content_api_shim.h` — C header exporting `ContentMain`
   - `content_api_shim.mm` — C++ shim with three subclasses
   - `app-Info.plist` — App bundle plist
   - `framework-Info.plist` — Framework plist
   - `helper-Info.plist` — Helper app plist
   - `ts_main.mm` — Custom launcher (not compiled in this experiment, but
     present in the tree from Issue 620)

   ```bash
   git checkout 146.0.7650.0-issue-620 -- content/zig_content_shell/
   ```

3. The `content_api_shim.mm` from commit `e69a931` is the clean single-profile
   version. It may need to be restored to that state since the 620 branch HEAD
   has the two-profile Experiment 15 code:

   ```bash
   git show e69a931:content/zig_content_shell/content_api_shim.mm \
     > content/zig_content_shell/content_api_shim.mm
   ```

   Update the comment block at the top to reference Issue 621 instead of 620.

#### content_api_shim.mm (expected state)

Single profile, single window, only `InitializeMessageLoopContext` overridden.
Parent `ShellBrowserMainParts` handles `InitializeBrowserContexts` and
`PostMainMessageLoopRun` — no profile path override, no second BrowserContext.

```cpp
const char* kInitialUrl = "https://google.com";

class TsBrowserMainParts : public content::ShellBrowserMainParts {
 protected:
  void InitializeMessageLoopContext() override {
    content::Shell::CreateNewWindow(browser_context(), GURL(kInitialUrl),
                                    nullptr, gfx::Size());
  }
};
```

The three-class chain (`TsMainDelegate` → `TsContentBrowserClient` →
`TsBrowserMainParts`) is identical to Issue 620 Experiment 1/10.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
gn gen out/Default
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Launch the app:
   ```bash
   open out/Default/Zig\ Content\ Shell.app
   ```
2. One Content Shell window appears showing google.com
3. Page is interactive — type into the search box, scroll, click links
4. Rendering is smooth (60fps) — no stuttering or freezing
5. Close the window — process exits cleanly

If 60fps: baseline established. Proceed to Experiment 2 (add second profile).

If 2fps: something is different about this branch. Compare with Issue 620's
branch to find the discrepancy.

**Result:** Pass

google.com rendered at 60fps — smooth, interactive, no stuttering. Baseline
established on the fresh `146.0.7650.0-issue-621` branch with zero
instrumentation.

#### Conclusion

The clean branch works. Single profile, single window, full speed. Ready for
Experiment 2.

### Experiment 2: Two profiles, both google.com

Add a second BrowserContext loading google.com. Both profiles load the same
page. This eliminates page complexity as a variable — if one profile degrades to
2fps while the other stays at 60fps, the problem is purely multi-BrowserContext
contention, not page-specific behavior.

Issue 620 Experiments 14–15 used google.com + lite.duckduckgo.com and found that
google.com was always the slow one. That left open the question: is google.com
specifically triggering the throttle, or does any complex page trigger it? By
loading the same page on both profiles, we answer that question.

Possible outcomes:

- **Both 60fps**: The problem was specific to the google.com + DDG combination
  (unlikely but would change the investigation direction entirely)
- **Both 2fps**: Both profiles degrade equally — the contention is symmetric and
  page complexity is not a factor
- **One 60fps, one 2fps**: The contention is asymmetric — one profile "wins"
  regardless of page content. This would confirm that the problem is in
  BrowserContext resource ordering, not page behavior

#### Changes

**`content_api_shim.mm`** — add second BrowserContext with same URL:

1. Add includes for `shell_browser_context.h` and `shell_paths.h`
2. Replace `kInitialUrl` with two URL constants (both google.com):
   ```cpp
   const char* kProfileAUrl = "https://google.com";
   const char* kProfileBUrl = "https://google.com";
   ```
3. Override `InitializeBrowserContexts()` to create two profiles with separate
   storage paths (`profile-a/`, `profile-b/`)
4. Override `InitializeMessageLoopContext()` to open two windows
5. Override `PostMainMessageLoopRun()` to destroy profile B before parent
   cleanup
6. Add `browser_context_b_` member

This is the same two-profile pattern from Issue 620 Experiment 2/9, but with
both URLs set to google.com.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Launch the app:
   ```bash
   open out/Default/Zig\ Content\ Shell.app
   ```
2. Two Content Shell windows appear, both showing google.com
3. Focus each window individually and type into the search box
4. Observe rendering performance in each window:
   - Are both smooth (60fps)?
   - Are both sluggish (2fps)?
   - Is one smooth and the other sluggish?
5. Close both windows — process exits cleanly

**Result:** Both 2fps

Both google.com windows rendered at 2fps. The contention is symmetric — neither
profile "wins." This eliminates page complexity as a variable: the Issue 620
finding that DDG was fast while google was slow was because DDG is lightweight
enough to avoid the bottleneck, not because google specifically triggers it.

#### Conclusion

Two BrowserContexts with the same complex page both degrade equally to 2fps. The
problem is purely multi-BrowserContext contention in Chromium's rendering
pipeline — not page-specific behavior, not asymmetric resource ordering. Any
sufficiently complex page will trigger it.

### Experiment 3: Two profiles, both lite.duckduckgo.com

The inverse of Experiment 2. Both profiles load lite.duckduckgo.com — a
lightweight page that rendered at 60fps alongside google.com in Issue 620
Experiments 14–15. If both DDG instances also render at 60fps here, the
degradation is strictly complexity-dependent: lightweight pages escape the
contention, heavyweight pages don't. If both degrade to 2fps, then even
lightweight pages suffer when both profiles are under equal load.

#### Changes

**`content_api_shim.mm`** — change both URLs to lite.duckduckgo.com:

```cpp
const char* kProfileAUrl = "https://lite.duckduckgo.com";
const char* kProfileBUrl = "https://lite.duckduckgo.com";
```

No other changes. Same two-profile structure as Experiment 2.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Launch the app:
   ```bash
   open out/Default/Zig\ Content\ Shell.app
   ```
2. Two Content Shell windows appear, both showing lite.duckduckgo.com
3. Focus each window individually and type into the search box
4. Observe rendering performance in each window:
   - Are both smooth (60fps)?
   - Are both sluggish (2fps)?
   - Is one smooth and the other sluggish?
5. Close both windows — process exits cleanly

If both 60fps: the contention only affects complex pages. Lightweight pages
avoid whatever bottleneck exists (likely less GPU/main-thread work means the
contention window is too short to cause visible degradation).

If both 2fps: the contention affects all pages regardless of complexity. The
Issue 620 DDG result was a fluke or edge case.

**Result:** Both 60fps

Both lite.duckduckgo.com windows rendered at 60fps — smooth, interactive, no
degradation. Combined with Experiment 2 (both google.com = both 2fps), this
completes the picture:

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| google.com          | —                   | 60    | —     | 621.1      |
| google.com          | google.com          | 2     | 2     | 621.2      |
| lite.duckduckgo.com | lite.duckduckgo.com | 60    | 60    | 621.3      |
| google.com          | lite.duckduckgo.com | 2     | 60    | 620.14     |
| lite.duckduckgo.com | google.com          | 60    | 2     | 620.15     |

The multi-BrowserContext contention exists regardless of page complexity, but
lightweight pages are fast enough to avoid visible degradation. The bottleneck
is a shared resource that both profiles contend for — when pages finish their
work quickly (DDG), the contention window is too short to matter. When pages
have sustained work (google.com), the contention becomes a persistent stall.

#### Hypotheses: why DDG escapes

**1. Blink main thread utilization.** google.com runs continuous JavaScript
(autocomplete, analytics, ad scripts, service workers). This keeps the Blink
main thread busy across multiple task queue cycles. DDG's lite page has
virtually no JavaScript — a static HTML form. When two profiles contend for
shared resources, the profile whose Blink main thread finishes quickly releases
the contention before the next vsync. google.com's main thread is still working
when the next BeginFrame arrives, causing frame drops.

**2. Compositor damage frequency.** google.com generates continuous compositor
damage — CSS animations (the blinking cursor, suggestion dropdown transitions),
layout shifts from async content loading, and JavaScript-driven DOM mutations.
Each damage event triggers a new CompositorFrame submission attempt. With two
profiles both generating continuous damage, the shared GPU channel or compositor
thread is saturated. DDG generates damage only on user input (typing), so the
contention window is brief and intermittent.

**3. Renderer process weight.** Each BrowserContext gets its own renderer
process. google.com's renderer process is heavyweight: large DOM, many layout
objects, complex paint layers, multiple JavaScript contexts (iframes, ads). The
renderer takes longer to produce each CompositorFrame. DDG's renderer is
lightweight: small DOM, simple layout, one paint layer. It produces frames
faster, clearing the shared resource before the next frame deadline.

**4. GPU command buffer volume.** google.com submits more GPU commands per frame
(more draw calls, more texture uploads, more shader switches) due to its complex
visual composition. With two profiles sharing a single GPU process, the command
buffer serialization takes longer. DDG submits far fewer GPU commands per frame,
so the serialization overhead is negligible.

**The common thread:** all four hypotheses point to the same root cause — a
shared resource (likely GPU channel, compositor thread, or Blink main thread
scheduling) that becomes a bottleneck proportional to per-frame work. The fix
must either eliminate the shared resource or reduce the per-frame contention.

#### Conclusion

The 2fps degradation is complexity-dependent, not inherent to multi-
BrowserContext. Lightweight pages coexist at 60fps. The contention is
proportional to per-frame work — when both profiles have heavy pages, neither
can finish within the frame budget. The next experiments should instrument the
renderer side to identify which specific shared resource is the bottleneck.

### Experiment 4: Two profiles, CSS animation only (no JavaScript)

google.com has two things DDG doesn't: JavaScript and CSS animations. Either
could be the source of contention. This experiment isolates CSS animations by
loading a page with continuous `@keyframes` animations and zero JavaScript.

CSS animations run in the compositor thread — they generate continuous
compositor damage (new CompositorFrames every vsync) without touching the Blink
main thread or executing any JavaScript. If two profiles both loading this page
degrade to 2fps, the bottleneck is in the compositor/GPU pipeline (damage
frequency, paint layers, GPU command serialization). If they stay at 60fps, the
bottleneck is JavaScript-driven.

#### Changes

**`test-html/css-animation/index.html`** — new test page with:

- A spinning box (`transform: rotate`, continuous)
- A color-cycling background (`background-color` transition, continuous)
- No `<script>` tags, no JavaScript, no event handlers

```html
<!DOCTYPE html>
<html>
<head>
<style>
body { margin: 0; overflow: hidden; background: #111; }
@keyframes spin {
  from { transform: rotate(0deg) }
  to { transform: rotate(360deg) }
}
@keyframes pulse {
  0% { background: #e44 }
  33% { background: #4e4 }
  66% { background: #44e }
  100% { background: #e44 }
}
.box {
  width: 200px;
  height: 200px;
  margin: 100px auto;
  animation: spin 2s linear infinite, pulse 3s linear infinite;
  border-radius: 20px;
}
</style>
</head>
<body>
<div class="box"></div>
</body>
</html>
```

**`test-html/css-animation/server.ts`** — Bun server on port 9621:

```typescript
const file = Bun.file(import.meta.dir + "/index.html");

Bun.serve({
  port: 9621,
  fetch() {
    return new Response(file, {
      headers: { "Content-Type": "text/html" },
    });
  },
});

console.log("http://localhost:9621");
```

**`content_api_shim.mm`** — point both URLs to localhost:

```cpp
const char* kProfileAUrl = "http://localhost:9621";
const char* kProfileBUrl = "http://localhost:9621";
```

Same two-profile structure as Experiments 2–3.

#### Build

```bash
cd ~/dev/termsurf/chromium/src
export PATH="$HOME/dev/termsurf/chromium/depot_tools:$PATH"
autoninja -C out/Default zig_content_shell
```

#### Verification

1. Start the Bun server:
   ```bash
   cd ~/dev/termsurf/test-html/css-animation && bun run server.ts
   ```
2. Launch the app:
   ```bash
   open out/Default/Zig\ Content\ Shell.app
   ```
3. Two Content Shell windows appear, each showing a spinning color-cycling box
   on a dark background
4. Observe the animation in each window:
   - Are both spinning smoothly (60fps)?
   - Are both stuttering (~2fps)?
   - Is one smooth and the other stuttering?
5. Close both windows — process exits cleanly
6. Stop the Bun server (Ctrl+C)

If both 60fps: CSS animations alone don't trigger the contention. The bottleneck
is JavaScript-driven (Blink main thread work, not compositor work).

If both 2fps: the compositor/GPU pipeline itself is the bottleneck. Continuous
compositor damage from any source (CSS or JS) triggers the contention when two
BrowserContexts are active.

**Result:** Both 60fps

Both windows rendered the spinning color-cycling box at full speed — smooth,
continuous animation with no stuttering. CSS `@keyframes` animations generate
continuous compositor damage every vsync, yet two BrowserContexts handle this
without degradation.

Updated results table:

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| google.com          | —                   | 60    | —     | 621.1      |
| google.com          | google.com          | 2     | 2     | 621.2      |
| lite.duckduckgo.com | lite.duckduckgo.com | 60    | 60    | 621.3      |
| CSS animation       | CSS animation       | 60    | 60    | 621.4      |
| google.com          | lite.duckduckgo.com | 2     | 60    | 620.14     |
| lite.duckduckgo.com | google.com          | 60    | 2     | 620.15     |

This is a critical finding. CSS animations run in the compositor thread and
generate continuous compositor damage — the same kind of per-frame work that
google.com produces. Yet they don't trigger the 2fps contention. This eliminates
the compositor/GPU pipeline as the bottleneck:

- **Eliminated:** compositor damage frequency (CSS animations damage every frame
  — no degradation)
- **Eliminated:** GPU command buffer serialization (two profiles both submitting
  draw calls every frame — no degradation)
- **Eliminated:** paint layer complexity (the spinning box has transforms and
  color changes — no degradation)
- **Confirmed:** the bottleneck is JavaScript-driven. google.com's continuous
  JavaScript execution (analytics, autocomplete, service workers, ad scripts) is
  what triggers the contention. The Blink main thread is the shared resource.

#### Conclusion

CSS animations at 60fps across two profiles proves the compositor and GPU
pipelines are not the bottleneck. The 2fps degradation is caused by JavaScript
contention on the Blink main thread. The next experiment should confirm this by
loading a page with continuous JavaScript animation (e.g.,
`requestAnimationFrame` loop) and zero CSS animations.

### Experiment 5: Two profiles, JavaScript animation (requestAnimationFrame)

The inverse of Experiment 4. Both profiles load the ts4 box demo — a page with a
continuous `requestAnimationFrame` loop drawing a spinning blue square on a
canvas. Zero CSS animations, all rendering driven by JavaScript. The page also
has a built-in FPS counter and a localStorage identity string (different per
profile, confirming profile isolation).

If both degrade to 2fps: JavaScript-driven animation confirms the Blink main
thread as the bottleneck. Combined with Experiment 4 (CSS = 60fps), this
pinpoints JavaScript execution as the trigger.

If both stay at 60fps: the box demo's JavaScript is too lightweight to trigger
the contention, and google.com's degradation comes from something heavier
(multiple scripts, iframes, service workers, etc.).

#### Changes

**`content_api_shim.mm`** — point both URLs to the box demo:

```cpp
const char* kProfileAUrl = "http://localhost:9616/test-box-demo.html";
const char* kProfileBUrl = "http://localhost:9616/test-box-demo.html";
```

Same two-profile structure as Experiments 2–4. The test page already exists at
`test-html/public/test-box-demo.html`.

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
3. Two Content Shell windows appear, each showing a spinning blue square with an
   FPS counter and a localStorage identity string
4. Observe:
   - Are both spinning smoothly (FPS counter shows ~60)?
   - Are both stuttering (FPS counter shows ~2)?
   - Do the identity strings differ (confirming profile isolation)?
5. Close both windows — process exits cleanly

If both ~60fps: lightweight JavaScript animation escapes the contention, just
like CSS animations and DDG. The bottleneck requires heavyweight JS (google.com
scale).

If both ~2fps: any continuous JavaScript execution triggers the contention when
two BrowserContexts are active. This would definitively confirm the Blink main
thread as the shared resource.

**Result:** Both 2fps

Both windows started slightly above 2fps then rapidly settled to a solid 2fps.
The box demo is a minimal `requestAnimationFrame` loop — roughly 30 lines of
JavaScript drawing one rectangle on a 300x300 canvas. This is orders of
magnitude simpler than google.com, yet it triggers the exact same degradation.

Updated results table:

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| google.com          | —                   | 60    | —     | 621.1      |
| google.com          | google.com          | 2     | 2     | 621.2      |
| lite.duckduckgo.com | lite.duckduckgo.com | 60    | 60    | 621.3      |
| CSS animation       | CSS animation       | 60    | 60    | 621.4      |
| JS box demo (rAF)   | JS box demo (rAF)   | 2     | 2     | 621.5      |
| google.com          | lite.duckduckgo.com | 2     | 60    | 620.14     |
| lite.duckduckgo.com | google.com          | 60    | 2     | 620.15     |

Combined with Experiment 4, this is definitive:

- **CSS animations (compositor thread):** 60fps — no contention
- **JavaScript animations (Blink main thread):** 2fps — immediate contention

The bottleneck is not page complexity, DOM size, network activity, or GPU load.
It is specifically **any continuous JavaScript execution** across two
BrowserContexts. Even a trivial `requestAnimationFrame` loop with one canvas
draw call is enough to trigger it.

This narrows the investigation to the Blink main thread scheduling or the
interface between the Blink main thread and the compositor. Something in
Chromium's architecture serializes or throttles `requestAnimationFrame`
callbacks when two BrowserContexts both have active rAF loops.

#### Conclusion

JavaScript is the trigger. A 30-line rAF loop reproduces the same 2fps as
google.com. The contention is in how Chromium schedules JavaScript execution
across BrowserContexts, not in the compositor or GPU pipeline. The next
experiment should test whether a single BrowserContext with two windows (same
profile) also degrades — this would distinguish BrowserContext-level contention
from renderer-process-level contention.

## Conclusion

### What we proved

Five experiments isolated the 2fps multi-BrowserContext degradation to a single
cause: **JavaScript execution on the Blink main thread.**

| Profile A           | Profile B           | A fps | B fps | Experiment |
| ------------------- | ------------------- | ----- | ----- | ---------- |
| google.com          | —                   | 60    | —     | 621.1      |
| google.com          | google.com          | 2     | 2     | 621.2      |
| lite.duckduckgo.com | lite.duckduckgo.com | 60    | 60    | 621.3      |
| CSS animation       | CSS animation       | 60    | 60    | 621.4      |
| JS box demo (rAF)   | JS box demo (rAF)   | 2     | 2     | 621.5      |
| google.com          | lite.duckduckgo.com | 2     | 60    | 620.14     |
| lite.duckduckgo.com | google.com          | 60    | 2     | 620.15     |

The critical pair is Experiments 4 and 5:

- **CSS `@keyframes` animations** run in the compositor thread, generate
  continuous compositor damage every vsync, and render at **60fps** across two
  BrowserContexts.
- **`requestAnimationFrame`** runs on the Blink main thread, and even a trivial
  30-line loop drawing one rectangle on a 300x300 canvas degrades both profiles
  to **2fps**.

The bottleneck is not page complexity, DOM size, network activity, GPU command
serialization, compositor damage frequency, or paint layer count.

### What Issue 620 eliminated

Issue 620's Chromium source code research (Experiments 12–15) instrumented the
entire viz/compositor pipeline and proved it is not the bottleneck:

| Mechanism                     | File                                 | Finding                                             |
| ----------------------------- | ------------------------------------ | --------------------------------------------------- |
| StopObservingBeginFrames      | `display_scheduler.cc:603`           | Fixed in Exp 13; was a symptom, not root cause      |
| ShouldDraw() gate             | `display_scheduler.cc:448–453`       | All conditions healthy except `needs_draw_`         |
| CVDisplayLink thrashing       | `cv_display_link_mac.mm`             | Register/unregister cycles observed but not causal  |
| OnNeedsBeginFrames thrashing  | `external_begin_frame_source_mac.cc` | Chromium devs' own TODO acknowledges as known issue |
| BeginFrameTracker throttle    | `begin_frame_tracker.cc`             | Never triggered (threshold: outstanding ≥ 10)       |
| kUndrawnFrameLimit            | `compositor_frame_sink_support.cc`   | Never triggered (threshold: undrawn > 3)            |
| SetIsGpuBusy backpressure     | `begin_frame_source.cc:152`          | Not investigated                                    |
| root_frame_missing() deadlock | `display_damage_tracker.cc:250`      | Reinforces ShouldDraw()=false but not root cause    |

The key finding from Experiment 14's instrumentation: **BeginFrames arrive at
60fps** via `CompositorFrameSinkSupport::OnBeginFrame()`, but the renderer only
produces CompositorFrames at ~3fps for complex pages. The viz pipeline is clean.
The renderer is the bottleneck.

### Where the bottleneck is

Issue 621 narrows the renderer-side bottleneck to the Blink main thread. The
compositor thread (which drives CSS animations) is unaffected. The contention is
in how Chromium schedules main-thread work — specifically
`requestAnimationFrame` callbacks — when two BrowserContexts coexist in the same
process.

The unexplored layer is:

- **Blink's main thread scheduler**
  (`third_party/blink/renderer/platform/scheduler/`) — how it prioritizes and
  dispatches tasks across multiple renderer contexts
- **BeginMainFrame dispatch** — the interface between the compositor thread and
  the Blink main thread that triggers rAF callbacks and style/layout/paint
- **Renderer process allocation** — whether two BrowserContexts share a renderer
  process or get separate ones, and how that affects main thread scheduling

### Next steps

1. **Same profile, two windows with rAF** — determine if the contention is
   BrowserContext-level or renderer-process-level
2. **Instrument the Blink main thread scheduler** — trace BeginMainFrame
   dispatch, rAF callback scheduling, and task queue contention across contexts
3. **Check renderer process allocation** — verify whether two BrowserContexts
   get separate renderer processes via
   `RenderProcessHostImpl::GetProcessHostForSiteInstance()`
4. **Profile with Chrome tracing** — `--trace-startup=cc,viz,blink` to see
   exactly where main-thread time is spent per frame per context
