+++
status = "closed"
opened = "2026-02-22"
closed = "2026-02-22"
+++

# Issue 623: Viz Display Serialization

## Goal

Confirm or rule out Viz Display serialization as the root cause of the 2fps
degradation when two BrowserContexts both run `requestAnimationFrame` loops. If
confirmed, find a fix. If ruled out, identify what's actually causing it.

## Background

### The problem

Two BrowserContexts in one Chromium process with JavaScript animations degrade
to 2fps. This requires BOTH conditions — neither alone triggers it:

| Configuration                | rAF | FPS     | Experiment |
| ---------------------------- | --- | ------- | ---------- |
| 2 BrowserContexts, 2 windows | yes | 2 + 2   | 621.5      |
| 1 BrowserContext, 2 windows  | yes | 60 + 60 | 622.3      |
| 2 BrowserContexts, CSS only  | no  | 60 + 60 | 621.4      |
| 1 BrowserContext, 1 window   | yes | 60      | 621.1      |

### What 24 experiments have eliminated

Across Issues 620 (15 experiments), 621 (5 experiments), and 622 (4
experiments):

- **Renderer process contention** — each BrowserContext gets its own renderer
  process with its own Blink main thread, compositor thread, and scheduler. No
  cross-process serialization in the renderer layer.
- **BrowserContext-specific frame logic** — "BrowserContext" appears in zero
  files across all of `components/viz/` and `cc/`. The frame pipeline has no
  concept of browser profiles.
- **Compositor/GPU pipeline** — CSS animations generate continuous compositor
  damage through the full viz pipeline at 60fps across two profiles.
- **BeginFrame delivery** — BeginFrames arrive at 60fps to both renderers. The
  renderer receives them but only produces CompositorFrames at ~3fps.
- **Scheduler state machine** — `--enable-main-frame-before-activation` is
  enabled on all renderers, so pipelining should be active.
- **All viz-side throttles** — `kUndrawnFrameLimit` never triggered,
  `BeginFrameTracker` throttle/stop never triggered, `ShouldSendBeginFrame`
  rejections only during init.
- **ExternalBeginFrameSourceMac thrashing** — found and fixed in 620 Exp 13
  (commented out `StopObservingBeginFrames`). Vsync stayed alive but 2fps
  persisted. Symptom, not cause.

### The hypothesis: Viz Display serialization

The `DisplayScheduler` waits for all pending surfaces before drawing
(`HasPendingSurfaces` in `display_damage_tracker.cc:135-170`). Each renderer
blocks on `kMaxPendingSubmitFrames = 1` (`scheduler_state_machine.cc:32`) until
Viz acknowledges the previous frame. The ack arrives only after the Display
draws.

The proposed chain reaction:

1. Both renderers receive BeginFrame simultaneously
2. Both start the BeginMainFrame → rAF → commit → activate → draw cycle
3. Renderer A finishes first, submits CompositorFrame, blocks on ack
4. Display waits for Renderer B (still in its main-thread round-trip)
5. Renderer B finishes, submits, Display draws both, sends acks
6. Both renderers unblock, start next frame — but a vsync was wasted waiting

CSS animations bypass this because the compositor responds to BeginFrame
immediately — no main-thread round-trip, no pending tree, no slow commit cycle.
The Display never waits.

Same-BrowserContext bypasses this because both WebContents share one renderer
process. Their compositor batches both into a single submission.

### The magnitude problem

The hypothesis qualitatively explains all three experimental cases. But there is
a gap: simple cross-process waiting should produce ~30fps (miss every other
frame), not 2fps (miss 29 out of 30). The 2fps implies a ~500ms frame cycle, far
beyond what "wait for the slower renderer" alone can produce.

This suggests either:

1. **A cascading feedback loop** — the initial delay triggers secondary
   throttling mechanisms that amplify the degradation. Missed deadlines cause
   the renderer to be treated as unresponsive, which causes more missed
   deadlines, which triggers more throttling.
2. **The hypothesis is wrong** — Viz Display serialization is not the cause, and
   the real mechanism is something else entirely that happens to correlate with
   separate renderer processes + main-thread involvement.

### Key observations that must be explained

Any correct theory must account for all of these:

1. **Both windows degrade equally** — not just the slower one
2. **The renderer stops producing frames** — `needs_draw=0` at the Display,
   meaning no CompositorFrames arrive (620 Exp 14)
3. **BeginFrames arrive at 60fps** — the Viz→renderer path is healthy
4. **The magnitude is ~2fps** — not 30fps, not 15fps
5. **CSS animations are immune** — despite going through the same Viz ack
   pipeline
6. **Same-BrowserContext JS is immune** — despite using the same GPU/Viz process
7. **Trivial JS triggers it** — a 30-line rAF loop is enough (621 Exp 5)
8. **Pipelining flag is enabled** — `--enable-main-frame-before-activation`
   should allow overlapped work

### Key code locations

| Component                      | File                                                                            | What it does                                                  |
| ------------------------------ | ------------------------------------------------------------------------------- | ------------------------------------------------------------- |
| Pending surface wait           | `components/viz/service/display/display_damage_tracker.cc:135-170`              | `HasPendingSurfaces` — Display waits for all surfaces         |
| Display draw decision          | `components/viz/service/display/display_scheduler.cc`                           | `ShouldDraw()`, `OnBeginFrameDeadline()`                      |
| Frame submission backpressure  | `cc/scheduler/scheduler_state_machine.cc:32`                                    | `kMaxPendingSubmitFrames = 1`                                 |
| Draw throttle gate             | `cc/scheduler/scheduler_state_machine.cc:1602-1607`                             | `IsDrawThrottled()` blocks draws, commits, and BeginMainFrame |
| BeginMainFrame gate            | `cc/scheduler/scheduler_state_machine.cc:670-679`                               | `ShouldSendBeginMainFrame()` blocked when draw-throttled      |
| Undrawn frame throttle         | `components/viz/service/frame_sinks/compositor_frame_sink_support.h:81`         | `kUndrawnFrameLimit = 3`                                      |
| Unresponsive client throttle   | `components/viz/service/frame_sinks/begin_frame_tracker.h:23-24`                | `kLimitThrottle = 10`, `kLimitStop = 100`                     |
| CompositorFrame ack            | `components/viz/service/frame_sinks/compositor_frame_sink_support.cc`           | `DidReceiveCompositorFrameAck` flow                           |
| Surface activation             | `components/viz/service/surfaces/surface.cc:702`                                | Ack sent when next frame activates                            |
| Frame sink BeginFrame delivery | `components/viz/service/frame_sinks/compositor_frame_sink_support.cc:1138-1233` | `OnBeginFrame()` with throttle checks                         |
| BeginFrame source fan-out      | `components/viz/common/frame_sinks/begin_frame_source.cc:558-597`               | Flat loop over all observers                                  |

## Approach

Two tracks: empirical experiments to test the hypothesis directly, and source
code research to trace the exact frame lifecycle and find the amplification
mechanism (or the real cause). Start with research to understand the
`HasPendingSurfaces` → ack → unblock chain in detail before modifying code.

## Experiments

### Experiment 1: Trace the full frame lifecycle across two renderers

A source code research experiment — no code changes, no builds. Before
instrumenting or modifying Chromium, we need to understand the exact sequence of
events in a single frame cycle when two renderer processes both have active rAF
loops. The goal is to map the complete chain from BeginFrame to ack and identify
where the ~500ms delay accumulates.

#### Question 1: What happens when the Display has pending surfaces?

The `DisplayScheduler` checks `HasPendingSurfaces` before drawing. Trace the
exact behavior when one renderer has submitted a CompositorFrame but the other
has not yet responded to its BeginFrame.

**Where to look:**

- `components/viz/service/display/display_scheduler.cc` —
  `OnBeginFrameDeadline()`, `ShouldDraw()`, and how the deadline timer is set.
  What happens when the deadline fires and a surface is still pending? Does the
  Display draw anyway (without the slow surface), or does it skip the entire
  draw?
- `components/viz/service/display/display_damage_tracker.cc:135-170` —
  `HasPendingSurfaces()` implementation. What counts as "pending"? Is it any
  surface that received a BeginFrame but hasn't submitted a CompositorFrame?
- `components/viz/service/display/display_scheduler.cc` — how the deadline
  duration is calculated. Is it a fixed offset from vsync, or adaptive? Does it
  grow when surfaces are slow?

**Key question:** Does the Display **skip the draw entirely** when surfaces are
pending at the deadline, or does it **draw with stale data** for the missing
surface? If it skips, that's a complete pipeline stall. If it draws with stale
data, the fast renderer should still get acked.

#### Question 2: How does the ack flow back to the renderer?

When the Display draws and the frame is presented, trace the exact path of the
ack back to the renderer's `pending_submit_frames_` counter.

**Where to look:**

- `components/viz/service/display/display.cc` — after `DrawAndSwap()`, how does
  it signal completion?
- `components/viz/service/frame_sinks/compositor_frame_sink_support.cc` —
  `DidReceiveCompositorFrameAck()`. When is this called relative to the draw? Is
  it immediate, or deferred to the next frame?
- `components/viz/service/surfaces/surface.cc:702` — the comment says "ack sent
  when next frame activates." Does this mean a renderer must wait for its OWN
  next frame to activate before receiving the ack for the current one? If so,
  that's a built-in one-frame delay that compounds with cross-surface waiting.
- `cc/scheduler/scheduler.cc` — how does receiving the ack unblock the renderer?
  Does it immediately allow a new BeginMainFrame, or does it wait for the next
  BeginFrame?

**Key question:** Is the ack latency one frame (sent at next draw), two frames
(sent at next surface activation), or variable? If it's two frames and the draw
itself is delayed by `HasPendingSurfaces`, the compounding could explain 2fps.

#### Question 3: What is the renderer doing between BeginFrame and CompositorFrame submission?

When the renderer receives a BeginFrame and `IsDrawThrottled()` is true (because
`pending_submit_frames_ >= 1`), what exactly happens? Does it silently drop the
BeginFrame? Does it send `DidNotProduceFrame`? Does it queue the work?

**Where to look:**

- `cc/scheduler/scheduler.cc` — `OnBeginFrameSourcePausedChanged()`,
  `BeginFrame()` — what does the scheduler do with a BeginFrame when it can't
  send BeginMainFrame?
- `cc/scheduler/scheduler_state_machine.cc` — trace through the full
  `NextAction()` decision tree when `IsDrawThrottled()` is true. What state does
  the scheduler settle into?
- `components/viz/service/frame_sinks/compositor_frame_sink_support.cc` — when
  the renderer responds with `DidNotProduceFrame`, does Viz still count it as
  "pending" for `HasPendingSurfaces`?

**Key question:** If a throttled renderer sends `DidNotProduceFrame`, does the
Display treat that surface as no longer pending (allowing it to draw the other
surface's frame)? Or does it still wait? If `DidNotProduceFrame` clears the
pending state, then the fast renderer should get its ack promptly and shouldn't
degrade. If it doesn't clear it, that's the amplification mechanism.

#### Verification

Research is complete when the full frame lifecycle is mapped:

```
BeginFrame → [renderer receives] → [throttle check] → [BeginMainFrame or drop]
  → [rAF + commit + activate + draw] → [submit CompositorFrame to Viz]
  → [Display checks HasPendingSurfaces] → [draw or skip]
  → [ack sent back] → [renderer unblocks]
```

Each step should have a file path, line number, and the decision logic. The
mapping should answer whether the magnitude problem (2fps vs expected ~30fps) is
explained by compounding delays in the ack chain, or whether we need to look
elsewhere.

**Result:** The Viz Display serialization hypothesis is **debunked**. The Viz
pipeline is clean. The 2fps cause is not in the Display, not in
HasPendingSurfaces, not in the ack flow.

#### Answer 1: The Display draws at the deadline regardless of pending surfaces

Under default settings (`wait_for_all_surfaces_before_draw_ = false`),
**`ShouldDraw()` does not check `HasPendingSurfaces`**
(`display_scheduler.cc:448-453`). The draw proceeds whenever `needs_draw_` is
true, `output_surface_lost_` is false, `visible_` is true, and
`root_frame_missing()` is false. Pending surfaces are irrelevant to the draw
decision.

The only effect of `has_pending_surfaces_` is on deadline mode selection
(`display_scheduler.cc:488-546`). When surfaces are pending, the Display uses
`kRegular` (draw at vsync minus 1/3 interval, ~5.56ms before vsync at 60Hz)
instead of `kImmediate` (draw now). This delays the draw by up to ~11ms but does
not skip it.

The deadline is a fixed ratio — `kDefaultEstimatedDisplayDrawTimeRatio = 1/3` of
the vsync interval (`begin_frame_args.h:177-189`). It is not adaptive. It does
not grow when surfaces are slow.

The `wait_for_all_surfaces_before_draw_` flag (which WOULD block draws on
pending surfaces) is only enabled by the
`--run-all-compositor-stages-before-draw` command-line switch
(`viz_compositor_thread_runner_impl.cc:242-243`). This is off by default and not
used by Content Shell.

**The fast renderer's frame IS drawn even when the slow renderer hasn't
responded.** The Display draws with stale data for the missing surface. The fast
renderer gets its ack during the draw.

#### Answer 2: Ack for Frame N is sent when Frame N+1 activates

The CompositorFrame ack flows through `Surface::ActivateFrame()`
(`surface.cc:669`). When Frame N+1 activates, it replaces Frame N as the active
frame (line 687-689). `UnrefFrameResourcesAndRunCallbacks` is called on Frame N
(line 702), which calls `SendAckIfNeeded` (line 966), which calls
`SendCompositorFrameAck` (line 494), which calls
`CompositorFrameSinkSupport::DidReceiveCompositorFrameAck` (line 1027-1041).
This decrements `pending_frames_` and sends the ack to the renderer via Mojo.

The ack is sent **synchronously on the Viz thread** during Frame N+1's
activation. It is not deferred to the draw. It arrives at the renderer
asynchronously via Mojo IPC.

On the renderer side, the ack decrements `pending_submit_frames_`
(`scheduler_state_machine.cc:1694`), then `ProcessScheduledActions()` runs
(`scheduler.cc:209`). However, if the renderer is in IDLE state (not inside a
BeginFrame interval), `ShouldSendBeginMainFrame()` returns false
(`scheduler_state_machine.cc:656-658`). **The renderer must wait for the next
BeginFrame from Viz before starting new work.**

The `kNoCompositorFrameAcks` feature (`features.cc:371-373`) is **disabled by
default**. When disabled, standard ack-based throttling via
`kMaxPendingSubmitFrames = 1` applies. When enabled, acks are eliminated and
throttling moves to Viz-side BeginFrame withholding. Not relevant to our case.

In steady state for a single renderer: submit Frame N → Frame N activates → acks
Frame N-1 → renderer receives ack → `pending_submit_frames_` goes 1→0 → but
Frame N was just submitted so it goes back to 1 → wait for next BeginFrame →
produce Frame N+1 → Frame N+1 activates → acks Frame N → cycle continues at
60fps.

#### Answer 3: DidNotProduceFrame DOES clear pending state

When a throttled renderer finishes its BeginFrame interval without submitting,
`Scheduler::FinishImplFrame()` (`scheduler.cc:637-686`) detects the
draw-throttled state and sends `DidNotProduceFrame` with
`FrameSkippedReason::kDrawThrottled` (line 661).

The `DidNotProduceFrame` carries a `BeginFrameAck` with the current `frame_id`
and `has_damage = false`. This flows through:

1. `CompositorFrameSinkSupport::DidNotProduceFrame` (line 661-692) calls
   `SurfaceModified()` with `has_damage = false`
2. `SurfaceManager::SurfaceModified` (line 500-510) notifies
   `DisplayDamageTracker::OnSurfaceDamaged`
3. `ProcessSurfaceDamage` (line 88-124) stores `last_ack = ack` for the surface
4. `HasPendingSurfaces` (line 135-170) checks
   `last_ack.frame_id ==
   begin_frame_args.frame_id` — match — surface is **not
   counted as pending**

Additionally, there is a second escape hatch at line 154-157:
`SurfaceHasUnackedFrame` skips surfaces whose producer is already
CompositorFrameAck-throttled. Even if `DidNotProduceFrame` somehow failed, a
renderer waiting for an ack would still not block the Display.

**The proposed feedback loop does not exist.** `DidNotProduceFrame` properly
clears pending state. The Display does not wait indefinitely for throttled
renderers.

#### The full frame lifecycle

```
BeginFrame (vsync)
  → Viz sends OnBeginFrame to each CompositorFrameSinkSupport
  → Mojo IPC to each renderer process

Renderer (throttled, pending_submit_frames_ >= 1):
  → cc::Scheduler::BeginImplFrame() — enters INSIDE_BEGIN_FRAME
  → NextAction() → ShouldSendBeginMainFrame() → false (IsDrawThrottled)
  → NextAction() → ShouldDraw() → false (IsDrawThrottled)
  → Deadline fires → FinishImplFrame()
  → SendDidNotProduceFrame(frame_id, kDrawThrottled)
  → Viz: DidNotProduceFrame → clears pending state

Renderer (not throttled):
  → cc::Scheduler::BeginImplFrame() — enters INSIDE_BEGIN_FRAME
  → NextAction() → ShouldSendBeginMainFrame() → true
  → BeginMainFrame → rAF callbacks → commit → activate → draw
  → SubmitCompositorFrame to Viz → pending_submit_frames_++

Viz (receives CompositorFrame):
  → Surface::QueueFrame → CommitFrame → ActivateFrame
  → Frame N replaces Frame N-1
  → UnrefFrameResourcesAndRunCallbacks(Frame N-1)
  → SendCompositorFrameAck → Mojo to renderer
  → Renderer: pending_submit_frames_-- → wait for next BeginFrame

Display:
  → has_pending_surfaces_ true → kRegular deadline (not kImmediate)
  → Deadline fires (~11ms into frame)
  → ShouldDraw() → true (does NOT check has_pending_surfaces_)
  → DrawAndSwap() with whatever frames are available
  → Stale data for missing surfaces
```

Every step has a clear code path. No step blocks on other renderers' state. The
Display draws at the deadline regardless. Acks flow synchronously during
activation. `DidNotProduceFrame` clears pending state.

#### Conclusion

**The Viz Display serialization hypothesis is wrong.** The proposed mechanism —
`HasPendingSurfaces` creating a cross-renderer stall that cascades into 2fps —
does not match the code. The Display draws at the deadline regardless of pending
surfaces, `DidNotProduceFrame` properly clears pending state, and acks are sent
synchronously during frame activation without waiting for the draw.

The Viz pipeline is now thoroughly mapped and cleared as a suspect. The 2fps
cause is **not** in:

- `HasPendingSurfaces` (does not block draws)
- `kMaxPendingSubmitFrames = 1` ack chain (works correctly in isolation)
- `DidNotProduceFrame` feedback loop (pending state is properly cleared)
- `DisplayScheduler` draw decisions (draws whenever there's damage)

The bottleneck must be upstream of Viz — somewhere in how the renderer process
produces (or fails to produce) CompositorFrames when a second BrowserContext
exists. The key observation from 620 Exp 14 stands: BeginFrames arrive at 60fps
but the renderer only produces CompositorFrames at ~3fps. The renderer is
receiving the signal to produce frames and choosing not to. The question is why.

## Conclusion

Four issues and 25 experiments over more than half a day failed to find the root
cause of the 2fps degradation when two BrowserContexts run JavaScript animations
in a single Chromium process.

### The investigation arc

[Issue 620](620-zig-content-shell.md) (15 experiments) built the Zig Content
Shell — a minimal Chromium embedder in 3 files and ~190 lines — and discovered
the 2fps problem. Instrumentation of the entire viz/compositor pipeline proved
that BeginFrames arrive at 60fps but the renderer only produces CompositorFrames
at ~3fps. The viz pipeline is clean. The bottleneck is in the renderer.

[Issue 621](621-single-process.md) (5 experiments) isolated the trigger: the
degradation requires JavaScript on the Blink main thread. CSS `@keyframes`
animations run at 60fps across two BrowserContexts because they execute on the
compositor thread. A trivial 30-line `requestAnimationFrame` loop degrades both
profiles to 2fps. The compositor thread is healthy; the main thread is the
bottleneck.

[Issue 622](622-javascript-is-slow.md) (4 experiments) proved the degradation
requires BOTH conditions — multiple BrowserContexts AND JavaScript. One
BrowserContext with two JS windows runs at 60fps. Two BrowserContexts with CSS
runs at 60fps. Only the combination triggers 2fps. Source code research
confirmed that "BrowserContext" appears in zero files across `components/viz/`
and `cc/` — the frame pipeline has no concept of profiles. The BrowserContext
boundary manifests solely through renderer process isolation
(`IsSuitableHost()`).

Issue 623 (this issue, 1 experiment) investigated the leading theory — Viz
Display serialization, where the `DisplayScheduler` waiting for pending surfaces
creates a cross-process stall chain. **The hypothesis was debunked.** The
Display draws at the deadline regardless of pending surfaces,
`DidNotProduceFrame` properly clears pending state, and acks flow synchronously
during frame activation. The entire Viz pipeline is now thoroughly mapped and
cleared as a suspect.

### What we know

| Configuration                | rAF | FPS     | Experiment |
| ---------------------------- | --- | ------- | ---------- |
| 1 BrowserContext, 1 window   | yes | 60      | 621.1      |
| 1 BrowserContext, 2 windows  | yes | 60 + 60 | 622.3      |
| 2 BrowserContexts, CSS only  | no  | 60 + 60 | 621.4      |
| 2 BrowserContexts, 2 windows | yes | 2 + 2   | 621.5      |

The renderer receives BeginFrames at 60fps and chooses not to produce
CompositorFrames. The decision happens upstream of Viz, inside the renderer
process, and is related to the Blink main thread. Every downstream mechanism —
Display scheduling, ack chains, pending surface tracking, draw throttling — has
been investigated and cleared.

### What was eliminated

Across all four issues:

- Viz Display serialization (HasPendingSurfaces, ack delays, draw skipping)
- Renderer process contention (each BrowserContext gets its own process)
- BrowserContext-specific frame logic (zero references in viz/cc)
- Compositor/GPU pipeline (CSS animations work at 60fps)
- All viz-side throttles (kUndrawnFrameLimit, BeginFrameTracker — never fired)
- ExternalBeginFrameSourceMac thrashing (symptom, not cause)
- kMaxPendingSubmitFrames ack chain (works correctly in isolation)
- DidNotProduceFrame feedback loop (properly clears pending state)
- DisplayScheduler draw decisions (draws whenever there's damage)
- Scheduler state machine (pipelining flag enabled)

### Direction change

The root cause of the single-process multi-BrowserContext 2fps degradation
remains unknown. Somewhere in Chromium's renderer internals — likely in Blink's
main thread scheduler, BeginMainFrame dispatch, or an interaction between
multiple renderer processes and the shared GPU process — JavaScript execution
triggers a pathological slowdown that we have not been able to locate.

Rather than continue searching for a needle in Chromium's ~35 million lines of
code, we will pursue a **multi-process architecture**: one Chromium process per
browser profile, each with a single BrowserContext. This is the architecture
TermSurf already uses in production (ts5, gui) — separate Chromium Profile
Server processes streaming IOSurface frames over XPC. The single-process
multi-profile approach was an optimization experiment to reduce IPC overhead.

The next investigation will focus on **speeding up inter-process communication**
— making the existing multi-process architecture faster rather than trying to
make multiple profiles coexist in one process.
