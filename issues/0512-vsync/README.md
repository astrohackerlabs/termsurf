+++
status = "closed"
opened = "2026-02-16"
closed = "2026-02-16"
+++

# Issue 512: Vsync

## Background

Issues 509–511 built a complete streaming pipeline: Chromium renders webpages,
streams IOSurface frames at 60fps over XPC, and the Metal renderer composites
them as overlays inside terminal panes. The pipeline supports multiple profiles,
multiple panes, server reuse, resize, and clean lifecycle management.

Both Chromium and the renderer report 60fps. But when comparing TermSurf
side-by-side with native Chromium, TermSurf looks noticeably choppier. The
average framerate is correct, but the visual cadence is uneven — micro-stutters
that the human eye detects even when the numbers look fine.

## Problem

The pipeline has two independent 60fps clocks that are not synchronized.

### Clock 1: Chromium's FrameSinkVideoCapturer

The capturer runs on its own timer, configured in `shell_video_consumer.cc`:

```cpp
capturer_->SetMinCapturePeriod(base::Milliseconds(16));
capturer_->SetAutoThrottlingEnabled(false);
```

This produces frames at ~60fps on Chromium's internal clock. Frames are pushed
to the compositor unconditionally — `OnFrameCaptured` calls `Done()` immediately
to return the buffer to Chromium's pool, with no backpressure from the renderer.

### Clock 2: Ghostty's Metal renderer

The renderer runs on a CVDisplayLink (macOS display vsync). When vsync is
enabled, only display-link-driven `drawFrame(true)` calls actually render. All
other `drawFrame(false)` calls are gated by `hasVsync()` and return immediately.

### The desynchronization

Even though both clocks average 60fps, their ticks drift relative to each other:

- Sometimes two captured frames arrive between two display refreshes. One frame
  is dropped (the old IOSurface is CFReleased, the new one CFRetained). Wasted
  work.
- Sometimes no captured frame arrives between two display refreshes. The
  renderer composites the same IOSurface again. Duplicate frame.

The result is micro-stutter — uneven frame intervals even at a correct average
framerate. XPC message delivery adds further jitter from scheduling latency.

## Additional issues found during analysis

### `needs_redraw` does not account for IOSurface changes

This may be the larger problem. In `generic.zig`, the display-link-driven
`drawFrame` checks:

```zig
const needs_redraw =
    size_changed or
    self.cells_rebuilt or
    self.hasAnimations() or
    sync;
```

When a new IOSurface arrives from Chromium:

- `size_changed` — false (the terminal didn't resize)
- `cells_rebuilt` — false (no terminal cell content changed)
- `hasAnimations()` — false (no custom shaders)
- `sync` — false (display link frames pass `sync=false`)

So `needs_redraw` is false, and the renderer calls `presentLastTarget()` — it
re-presents the previous frame instead of compositing the new IOSurface.

The `setOverlayIOSurface` call does invoke `queueRender()`, which notifies the
wakeup async. But when vsync is active, the wakeup path calls
`drawFrame(false)`, which is a no-op because `hasVsync()` returns true.

In practice, the overlay still renders because the `web` TUI is actively running
in the pane (ratatui draws the URL bar, status bar, cursor blink), which keeps
`cells_rebuilt` true on most frames. But if the TUI reaches a steady state, new
IOSurface frames would be silently missed until something else triggers a
redraw. This explains the uneven visual cadence — some frames render the new
overlay, some don't, depending on whether the terminal happened to have cell
changes at that moment.

### No backpressure from renderer to capturer

Chromium's `OnFrameCaptured` calls `Done()` immediately, returning the IOSurface
buffer to Chromium's pool. If Chromium reuses that buffer before the next
`drawFrame`, the IOSurface content may be mid-write when the Metal renderer
samples it — a potential tearing source.

### Single IOSurface pointer, no buffering

The overlay uses a single `overlay_iosurface` pointer. When Chromium sends
frames faster than the renderer consumes them, intermediate frames are silently
dropped (CFRelease old, CFRetain new in `setOverlayIOSurface`). There is no
double or triple buffering for the overlay — only the terminal's swap chain has
triple buffering.

## How Chromium solves this internally

Chromium's fundamental solution is to **never have two independent clocks**.
There is exactly one authoritative timing source — the display's vsync — and
every frame producer in the entire pipeline is a slave to that single clock.

### The BeginFrame architecture

The architecture has three layers:

**BeginFrameSource** — wraps `CADisplayLink`/`CVDisplayLink` on macOS
(`ExternalBeginFrameSourceMac`). On each vsync callback it generates a
`BeginFrameArgs` containing `frame_time`, `deadline`, and `interval`. This is
the _only_ entity that talks to the OS vsync mechanism. Everything else receives
timing information from it.

**Display Compositor (Viz)** — the `DisplayScheduler` observes the
`BeginFrameSource` and orchestrates the pipeline. On each vsync it enters a
"begin frame deadline interval", waits for all client frame sinks to submit
their `CompositorFrame`s (or ack that they have no damage), then at a computed
deadline triggers `DrawAndSwap()`. The 16.67ms frame budget at 60Hz is split
into thirds:

- 0–5.5ms: main thread (Blink layout, paint, JavaScript)
- 5.5–11ms: compositor thread (raster, commit, activate, draw)
- 11–16.67ms: display compositor (aggregate, GPU draw, swap)

**cc::Scheduler (per renderer)** — each renderer process has a scheduler that is
a `BeginFrameObserver`. It does not have its own timer. It receives
`BeginFrameArgs` from Viz via Mojo IPC. When a BeginFrame arrives, it does
layout/paint/commit, then submits a `CompositorFrame` back to Viz before the
deadline. If the main thread is too slow, the compositor draws from the previous
active tree (animations and scrolls still update smoothly).

The `FrameSinkManagerImpl` propagates the single `BeginFrameSource` to all child
frame sinks via `RecursivelyAttachBeginFrameSource()`. Every renderer's
compositor gets the same clock. No independent timing anywhere.

### Design patterns

- **Pull, don't push.** Frame producers never push on their own schedule. The
  display clock pulls work by sending BeginFrame signals. Producers respond to
  those signals and submit frames within the allotted deadline.
- **Single authoritative clock.** One `BeginFrameSource` per display, derived
  from the OS vsync. All producers are observers of this single source. No
  independent 60fps timers anywhere.
- **Deadline-based scheduling.** Each BeginFrame carries a deadline. Miss it and
  you skip the frame. The display compositor waits until the deadline (or until
  all surfaces are ready), then draws.
- **Damage-driven drawing.** No damage = no draw = no wasted work.

### Why this doesn't help us (yet)

The `FrameSinkVideoCapturer` (what we use) works differently from normal frame
sinks. It is an async observer that issues `CopyOutputRequest`s on its own
schedule. It does not participate in the BeginFrame protocol the same way. This
is exactly why our two-clock problem exists — the capturer's output timing is
decoupled from the display's vsync.

To benefit from Chromium's one-clock architecture, we would need to either:

1. Participate in Chromium's BeginFrame protocol — have the CVDisplayLink feed a
   `BeginFrameSource` that Chromium's renderers observe.
2. Use Chromium's own display path — render output surfaces directly as Metal
   textures in the same pipeline CVDisplayLink drives.

Both require in-process embedding via the Content API (idea #4 below).

## How Electron handles this

Electron has two rendering modes with very different answers.

### Normal BrowserWindow (windowed rendering)

No synchronization problem exists. Chromium renders into IOSurface-backed
CALayers, sends `CAContextID`s cross-process, and the browser uses `CALayerHost`
to composite them. CoreAnimation handles vsync synchronization atomically at the
window server level. One clock. No drift.

This is the architecture you get with in-process Chromium embedding.

### Off-Screen Rendering (OSR)

Structurally identical to what TermSurf does. `FrameSinkVideoCapturer` produces
frames on Chromium's internal clock. The consumer receives them asynchronously.
Two independent clocks, same drift, same micro-stutter. **Electron does not
solve this.**

What Electron does to make the drift tolerable:

- **Frame pool backpressure.** A 10-frame `GpuMemoryBufferVideoFramePool`. If
  the consumer doesn't call `release()`, the pool fills and capture stops.
  Prevents unbounded accumulation but does not synchronize timing.
- **Damage-driven capture.** Captures on `OnFrameDamaged` events rather than a
  fixed timer. Reduces wasted work but does not align timing.
- **Unlimited frame rate for shared textures.** Electron PR #45669 removed the
  fps cap when using GPU-backed shared textures, since the handle copy is
  near-zero overhead. Always have a fresh frame available. This is idea #2
  below.

The three throttling patches we already applied (from Issues 408–410) solve a
different problem — they ensure Chromium produces frames at all for off-screen
content. That is about frame _production_, not frame _synchronization_.

### Summary

| Approach            | How it solves vsync                                                   |
| ------------------- | --------------------------------------------------------------------- |
| Chromium (internal) | Single `BeginFrameSource` from vsync; all producers observe one clock |
| Electron (windowed) | CALayer + CAContextID + CoreAnimation; OS handles sync atomically     |
| Electron (OSR)      | Doesn't solve it. Same two-clock drift we have. Tolerates it.         |
| TermSurf (current)  | Same as Electron OSR — two independent clocks                         |

## Ideas for fixing

### 1. Fix `needs_redraw` for overlay changes

The simplest fix with the highest impact. Add a flag like
`overlay_surface_changed` that is set in `setOverlayIOSurface` and checked in
the `needs_redraw` condition. This ensures every new IOSurface triggers a redraw
on the next display link tick.

This alone would fix the frame-skipping problem. It does not fix the two-clock
desynchronization, but it ensures every captured frame that arrives before a
vsync is composited on that vsync.

### 2. Increase capture rate to 120fps

Change `SetMinCapturePeriod` from 16ms to 8ms (120fps). With the capturer
producing frames at 2x the display rate, there is always a fresh frame available
at every vsync. The maximum age of the displayed frame drops from ~16ms to ~8ms,
smoothing out the visual cadence.

Cost: doubled capture work and XPC traffic (120 IOSurface Mach port transfers
per second per pane instead of 60). The frames themselves are zero-copy GPU
memory, so the overhead is in the capture scheduling and XPC messaging, not in
pixel copying.

This pairs well with fix #1. Without #1, increasing the capture rate just
produces more frames that get silently skipped.

### 3. Demand-driven capture (pull model)

Instead of the capturer pushing frames on its own timer, the renderer requests a
new frame after each vsync. Chromium's `FrameSinkVideoCapturer` supports
`RequestRefreshFrame()` — it forces the capturer to produce a frame on demand.
The consumer would set `SetMinCapturePeriod(0)` (unlimited rate) and call
`RequestRefreshFrame()` after each display refresh.

The challenge: the signaling path is renderer → Zig → Swift → XPC → Chromium
server → capturer. This round-trip adds latency that might make per-frame pull
impractical for 60fps. Each request needs to complete in under 16ms to stay
ahead of the next vsync.

This is the theoretically correct solution but the most complex to implement.

### 4. In-process Chromium (the endgame)

When Chromium is embedded directly via the Content API, the renderer can use the
same display link to drive both terminal and browser rendering. No XPC latency,
no independent clocks, no wasted frames. The streaming architecture was always a
stepping stone — this is what eliminates the vsync problem permanently.

## Experiments

### Experiment 1: 120fps oversampling

Combine fixes #1 and #2: ensure every IOSurface change triggers a redraw, then
double the capture rate so there is always a fresh frame at every vsync. Goal:
visually indistinguishable from native Chromium.

#### Change 1: `overlay_surface_changed` flag (generic.zig)

Add a boolean field to the renderer state:

```zig
/// Set when a new overlay IOSurface arrives. Cleared after each drawFrame.
overlay_surface_changed: bool = false,
```

Add it after the existing `overlay_iosurface` field (line 197 of `generic.zig`).

In `setOverlayIOSurface` (line 2443 of `Surface.zig`), set the flag before
calling `queueRender`:

```zig
self.renderer.overlay_iosurface = iosurface;
self.renderer.overlay_surface_changed = true;
self.queueRender() catch {};
```

In `drawFrame` (line 1454 of `generic.zig`), include the flag in `needs_redraw`
and clear it:

```zig
const overlay_changed = self.overlay_surface_changed;
self.overlay_surface_changed = false;

const needs_redraw =
    size_changed or
    self.cells_rebuilt or
    self.hasAnimations() or
    overlay_changed or
    sync;
```

Read the flag and clear it before the `needs_redraw` check, so it is always
consumed regardless of whether other conditions are true. The field is protected
by `draw_mutex` (the same lock that guards `setOverlayIOSurface`).

#### Change 2: 120fps capture (shell_video_consumer.cc)

In `Attach()` (line 87 of `shell_video_consumer.cc`), change:

```cpp
capturer_->SetMinCapturePeriod(base::Milliseconds(16));
```

to:

```cpp
capturer_->SetMinCapturePeriod(base::Milliseconds(8));
```

This doubles the capture rate from 60fps to 120fps. At 2x the display rate,
there is always a frame no older than ~8ms at every vsync.

#### Chromium branch

Create `146.0.7650.0-issue-512` from `146.0.7650.0-issue-511` (inherits per-tab
pane routing and auto-exit). The only Chromium change is the one-line capture
period.

```bash
cd chromium/src
git checkout -b 146.0.7650.0-issue-512 146.0.7650.0-issue-511
# Edit shell_video_consumer.cc
autoninja -C out/Default chromium_profile_server
```

#### Verification

```bash
cd ts4/box-demo && bun run server.ts &
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- http://localhost:9407
```

Open native Chromium side by side with TermSurf, both showing the box-demo
spinning square. Compare visual smoothness for 30+ seconds.

Pass: TermSurf looks visually indistinguishable from native Chromium. No
perceptible micro-stutter or uneven cadence.

#### Result: Pass

Side-by-side with native Chromium, TermSurf is visually identical. The
micro-stutter is gone. Two changes, both necessary:

1. **`overlay_surface_changed`** fixed the frame-skipping bug. Without this, new
   IOSurface frames were silently missed whenever the terminal had no cell
   changes — the `needs_redraw` check returned false and `presentLastTarget()`
   re-presented the old frame. This was likely the larger contributor to the
   choppiness.

2. **120fps capture** fixed the two-clock drift. With the capturer producing
   frames at 2x the display rate, the newest IOSurface at any vsync is at most
   ~8ms old. The temporal aliasing from phase drift between the capturer timer
   and the CVDisplayLink is halved, dropping below the perceptible threshold.

The cost is doubled capture GPU blits and XPC traffic (120 Mach port transfers
per second per pane instead of 60). For a handful of panes this is negligible.
If it becomes a concern at scale, the demand-driven pull model (idea #3) or
in-process Chromium (idea #4) would eliminate the overhead. For now, 120fps
oversampling is good enough.

## Conclusion

The correct long-term solution is Chromium's own approach: a single vsync source
of truth owned by the main app (TermSurf's CVDisplayLink), with Chromium's frame
production driven by BeginFrame signals originating from that clock. This would
require rearchitecting the communication protocol between TermSurf and the
Chromium Profile Server — and possibly making significant changes inside
Chromium itself to wire the capturer into the BeginFrame pipeline. That work
will happen eventually.

But the visual difference would be marginal. The 120fps oversampling solution
already looks identical to native Chromium. The single-clock approach would
reduce wasted GPU blits and XPC traffic, not improve perceived smoothness. It is
an efficiency optimization, not a quality one.

Meanwhile, there are far more fundamental problems to solve: keyboard and mouse
input forwarding, scrolling, navigation, tab management, DevTools, downloads,
permissions, and every other feature a browser needs. Perfecting the vsync
solution can wait until those are in place. This issue is resolved for now.
