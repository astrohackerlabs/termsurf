# Issue 343: Optimal Performance — Perfect 60fps

## Goal

Achieve perfect, uncompromising 60fps frame delivery from the profile server to
the GUI. Not 38fps. Not 71% of frames at 60fps. Every frame, every time.

The current implementation delivers 38.2fps average with 71% of frames at 60fps
cadence and a max streak of 424 consecutive good frames. This is a dramatic
improvement over where we started (17fps), but it is visibly inferior to native
60fps. Scrolling still stutters. Animations still hitch. The remaining 29% of
dropped or delayed frames are perceptible and unacceptable.

We will not stop until the profile server delivers a sustained, unbroken 60fps
stream — matching or exceeding the cef-rs OSR example's performance.

## How We Got Here

### Issue 338: Discovery

[Issue 338](./338-lag.md) identified the problem: TermSurf's browser rendering
was noticeably laggy compared to native Chrome. Scrolling felt sluggish,
animations stuttered, and hover effects were jerky. Investigation revealed the
bottleneck was not XPC transport latency but CEF itself — the profile server was
producing frames at only ~20fps.

Key findings from 338:

- IOSurface import is fast (~0.37ms), not the bottleneck
- Mouse move events are unthrottled (100+ XPC messages/sec during movement)
- The GUI holds a read lock during the entire render loop
- Mach ports are recreated every frame even when the IOSurface handle hasn't
  changed

### Issue 340: The cef-rs Breakthrough

[Issue 340](./340-architecture.md) made the critical discovery: the cef-rs OSR
example achieves a sustained 60fps with the same CEF version. This disproved the
theory that CEF had a hard 30fps cap and proved the problem was in our process
environment, not CEF itself.

### Issue 341: 18 Experiments, One Dead End

[Issue 341](./341-performance.md) ran 18 systematic experiments to close the gap
between the cef-rs example (60fps) and the profile server (~20fps):

| #  | Experiment                               | Result  | 60fps% | Streak |
| -- | ---------------------------------------- | ------- | ------ | ------ |
| 1  | Document process architecture            | Diag    | —      | —      |
| 2  | Add winit event loop (no window)         | Failed  | 3%     | —      |
| 3  | Measure cef-rs example frame rate        | Diag    | ~90%   | —      |
| 4  | Enable `external_message_pump`           | Partial | 52%    | 5      |
| 5  | `NSApplicationActivationPolicyRegular`   | Failed  | 42%    | —      |
| 6  | Use `run_message_loop()`                 | Failed  | 24%    | 5      |
| 7  | Hidden 1x1 window                        | Success | 78%    | 57     |
| 8  | CVDisplayLink without window             | Failed  | 30%    | 4      |
| 9  | Restore hidden window baseline           | Success | 61%    | 35     |
| 10 | `NSApplicationActivationPolicyAccessory` | Failed  | —      | —      |
| 11 | Native NSWindow, `canBecomeKey: NO`      | Partial | 34%    | 4      |
| 12 | `orderFront` instead of `orderBack`      | Partial | 34%    | 4      |
| 13 | Layer-backed content view                | Failed  | 36%    | 3      |
| 14 | NSApplication event pumping              | Failed  | 33%    | 4      |
| 15 | Swizzle `canBecomeKey` on winit          | Failed  | —      | —      |
| 16 | GUI-side focus reclaim                   | Partial | 20%    | 16     |
| 17 | External begin frame at 60Hz             | Failed  | 10%    | 2      |
| 18 | Revert to baseline                       | Diag    | 40%    | 11     |

The only approach that worked was a hidden 1x1 window (Exp 7: 78% at 60fps), but
it steals focus from the GUI. Every attempt to fix focus stealing destroyed the
vsync signal. The hidden window approach was abandoned as an architectural dead
end — focus and vsync are fundamentally coupled through the macOS window server.

### Issue 342: The CFRunLoop Breakthrough

[Issue 342](./342-perf-no-win.md) took a different approach: instead of
providing CEF with an external vsync signal, understand why its internal frame
scheduling was failing. Five experiments:

| # | Experiment                | Result  | FPS  | 60fps% | Streak |
| - | ------------------------- | ------- | ---- | ------ | ------ |
| 1 | CEF debug logging         | Diag    | —    | —      | —      |
| 2 | NSApplication init        | Failed  | 28.5 | 40%    | 11     |
| 3 | `run_message_loop()`      | Failed  | 19.2 | —      | —      |
| 4 | CFRunLoop + external pump | Failed  | 0    | 0%     | 0      |
| 5 | `CFRunLoopRunInMode` swap | Success | 38.2 | 71%    | 424    |

**Root cause found:** CEF's `SyntheticBeginFrameSource` — the timer-based frame
scheduler for windowless mode — schedules work via CFRunLoop timer sources.
`thread::sleep()` suspends the thread without servicing the run loop, starving
these sources. Replacing `sleep(1ms)` with `CFRunLoopRunInMode(0.001)` services
pending timer callbacks, allowing CEF's internal scheduling to function.

**Current state after Issue 342:**

| Metric                    | Before (Issue 341) | After (Issue 342) |
| ------------------------- | ------------------ | ----------------- |
| Average FPS               | 28.5               | 38.2              |
| Frames at 60fps           | 40%                | 71%               |
| Max consecutive 60fps     | 11                 | 424               |
| Display link samples      | —                  | 3 (not working)   |
| SyntheticBeginFrame fires | 3                  | 19                |

## Current Code

The profile server's polling loop (`ts3/termsurf-profile/src/main.rs`):

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    #[cfg(target_os = "macos")]
    cfrunloop::run_for(0.001); // CFRunLoopRunInMode, 1ms
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(Duration::from_millis(1));
}
```

CEF settings: `windowless_rendering_enabled: 1`, `no_sandbox: 1`,
`log_severity: VERBOSE`. No `external_message_pump`. No window. No event loop.

## What We Know

1. **CEF can deliver 60fps** — the cef-rs OSR example proves it (Issue 341,
   Exp 3)
2. **The display link is broken** — `ExternalBeginFrameSourceMac.DisplayLink`
   fires only 3 times; it needs a window server connection we don't have
3. **SyntheticBeginFrameSource works but is inconsistent** — it fires at the
   correct 16ms interval but only produced 19 samples across the session (Issue
   342, Exp 5)
4. **71% of frames hit 60fps cadence** — something causes the other 29% to miss
5. **CFRunLoop servicing is necessary** — without it, CEF's timers starve
   completely
6. **The polling loop structure matters** — `do_message_loop_work()` +
   `CFRunLoopRunInMode(1ms)` is the best combination found so far

## The Remaining Gap: 71% → 100%

The 29% of non-60fps frames fall into several categories based on Issue 342's
Experiment 5 data:

- **10-19ms bucket:** 426 intervals (72%) — these are the good frames
- **30-36ms bucket:** ~50 intervals — frames arriving at exactly 2x the vsync
  period (30fps), suggesting a missed compositor beat
- **50-80ms bucket:** scattered — multi-beat misses, likely from longer stalls
- **>100ms:** rare — page load, layout, or JavaScript execution pauses

The 30fps frames (33ms intervals) are the primary target. These are not random
jitter — they are exactly one missed vsync beat, suggesting a systematic timing
issue where the compositor occasionally skips a cycle.

## Hypotheses: Why 29% of Frames Miss

### H1: Polling Loop Timing Mismatch

The current loop calls `do_message_loop_work()` then `CFRunLoopRunInMode(1ms)`.
The total loop iteration time is `do_message_loop_work()` latency + up to 1ms.
If `do_message_loop_work()` takes variable time, the loop cadence drifts
relative to CEF's internal 16.67ms compositor cycle.

When the loop call happens to align with CEF's timer — frame produced. When it
drifts out of alignment — timer fires but no `do_message_loop_work()` processes
it until the next iteration, missing the beat.

**Test:** Measure actual loop iteration times. If they cluster around 1.5-2ms
with occasional spikes, timing drift is the likely cause.

### H2: CFRunLoop 1ms Timeout Too Short

`CFRunLoopRunInMode` with a 1ms timeout returns after either handling one source
or timing out. If CEF has multiple timer sources that need servicing in a single
iteration (e.g., SyntheticBeginFrameSource + compositor task), only the first
one fires per call. The second waits for the next loop iteration.

**Test:** Try longer CFRunLoop timeouts (2ms, 5ms, 10ms) or call
`CFRunLoopRunInMode` multiple times per loop iteration until it returns "timed
out" (no more sources to handle).

### H3: `do_message_loop_work()` and CFRunLoop Fighting

`do_message_loop_work()` processes CEF's internal task queue.
`CFRunLoopRunInMode` processes CFRunLoop sources (which include CEF's timers).
These may interfere: `do_message_loop_work()` might partially process a task
that a CFRunLoop timer was about to trigger, or vice versa. The two systems are
not designed to be interleaved this way.

**Test:** Try calling only `CFRunLoopRunInMode` with a longer timeout (16ms) and
no explicit `do_message_loop_work()`. If CEF's timers internally call
`do_message_loop_work()` when they fire, the explicit call may be redundant — or
harmful.

### H4: `external_message_pump` Would Help (If Deadlock Is Fixed)

Issue 342 Experiment 4 failed because of a chicken-and-egg deadlock: CEF needs
`do_message_loop_work()` during initialization, but the cooperative timer that
calls it only fires after the run loop starts. However, the cef-rs OSR example
uses `external_message_pump: true` and achieves 60fps. The reference
implementation in `cef-rs/examples/tests_shared/` uses `NSApp().run()` (not
`CFRunLoopRun()`) to avoid this deadlock.

If we can correctly initialize CEF with `external_message_pump` — perhaps by
running a brief polling phase during init before switching to cooperative
scheduling — the `on_schedule_message_pump_work` callback would let CEF tell us
exactly when to call `do_message_loop_work()`, eliminating all timing guesswork.

**Test:** Two-phase approach: (1) poll during CEF init, (2) switch to
`on_schedule_message_pump_work`-driven scheduling after `on_context_initialized`
fires.

### H5: SyntheticBeginFrameSource Only Fires 19 Times

Issue 342 Exp 5 showed only 19 `Viz.ExternalBeginFrameSource.Interval` histogram
samples across a 15-second session with 593 frames. If SyntheticBeginFrameSource
is the frame clock, 19 fires across 593 frames means most frames are produced by
some other mechanism — possibly `Invalidate()` calls from content changes or
`do_message_loop_work()` directly triggering compositor runs.

The 19 fires could be the "seed" that kicks CEF into rendering, after which the
compositor runs on momentum for a while before stalling. The stalls would
correspond to the 30fps drops.

**Test:** Correlate SyntheticBeginFrameSource fire times with frame timing logs
to see if the 19 fires correspond to the starts of 60fps streaks.

### H6: Missing Vsync Signal Causes Frame Pacing Jitter

The display link (`ExternalBeginFrameSourceMac.DisplayLink`) fires only 3 times.
Without a real vsync signal, CEF relies on SyntheticBeginFrameSource — a
software timer. Software timers on macOS are subject to:

- Timer coalescing (macOS groups timers to save power)
- Thread scheduling jitter
- CFRunLoop iteration timing

A CVDisplayLink or CADisplayLink tied to the actual display refresh would
provide a hardware-accurate frame clock. Issue 341 Exp 8 tried CVDisplayLink
alone (it didn't help), but that was before the CFRunLoop fix. Combining a
CVDisplayLink with CFRunLoop servicing might provide both the timing signal and
the run loop integration.

**Test:** Create a CVDisplayLink and use its callback to trigger
`do_message_loop_work()` at precise vsync times, replacing the blind 1ms
polling.

### H7: GUI-Side Frame Pacing

The profile server may produce frames at 60fps, but the GUI may not consume them
at that rate. WezTerm's render loop has its own cadence. If the GUI renders at a
different rate or has variable frame times, it could drop profile server frames
or introduce jitter.

The GUI currently uses `PresentMode::Fifo` with
`desired_maximum_frame_latency:
2`, which means 2 frames in the GPU queue. If the
GUI's render loop takes variable time, frames from the profile server may sit in
the queue waiting.

**Test:** Add frame timing instrumentation to the GUI side to measure when
IOSurface frames arrive vs when they're actually rendered to screen.

### H8: Mach Port Creation Overhead

Every `on_accelerated_paint` callback creates a new Mach port from the IOSurface
handle, even when the handle hasn't changed. This is a kernel syscall on every
frame. CEF uses double/triple buffering so the handle changes on ~82% of frames,
but the 18% of redundant Mach port creations add kernel overhead in the hot
path.

More importantly, the XPC message to send the Mach port to the GUI adds latency.
Issue 338 measured XPC send at 10-30ms — a significant chunk of the 16.67ms
frame budget.

**Test:** Cache Mach ports by IOSurface handle. Only create and send a new Mach
port when the handle actually changes.

### H9: `return_after_source_handled` Flag

`CFRunLoopRunInMode` is called with `return_after_source_handled: true` (1).
This means it returns after processing a single source. If multiple CFRunLoop
sources need servicing in one frame cycle (e.g., a timer source + an input
source + a compositor callback), only the first is handled per call. The others
wait for the next loop iteration, introducing a 1ms+ delay.

**Test:** Try `return_after_source_handled: false` (0) so CFRunLoop processes
all pending sources before returning, or loop until CFRunLoop returns
`kCFRunLoopRunTimedOut` (no more sources to handle).

### H10: CEF Process Priority and QoS

macOS assigns Quality of Service (QoS) classes to threads and processes. A
process without a window may be assigned a lower QoS class, leading to:

- Lower scheduling priority
- Timer coalescing (reduced timer precision)
- Reduced CPU allocation

Even though `NSApplicationActivationPolicyRegular` didn't help frame rate in
Issue 341 Exp 5, thread-level QoS might matter. The main thread's QoS class
determines timer precision. Setting it to `QOS_CLASS_USER_INTERACTIVE` (highest)
would give maximum timer precision.

**Test:** Set the main thread's QoS class to `QOS_CLASS_USER_INTERACTIVE` before
entering the polling loop.

## Experiment Checklist

Ordered by likelihood of impact and implementation simplicity:

- [ ] **1. Drain CFRunLoop fully** (H2, H9) — Loop `CFRunLoopRunInMode` until it
      returns timed-out, processing all pending sources per iteration
- [ ] **2. Measure loop iteration timing** (H1) — Add microsecond-precision
      timing to the polling loop to understand actual cadence
- [ ] **3. Remove explicit `do_message_loop_work()`** (H3) — Test whether
      CFRunLoop alone drives CEF, without the explicit call
- [ ] **4. Increase CFRunLoop timeout** (H2) — Try 2ms, 5ms, 16ms timeouts to
      give run loop sources more time to fire
- [ ] **5. Set thread QoS to USER_INTERACTIVE** (H10) — Maximize timer precision
      and scheduling priority
- [ ] **6. Cache Mach ports by IOSurface handle** (H8) — Eliminate redundant
      kernel syscalls and XPC messages
- [ ] **7. Two-phase `external_message_pump`** (H4) — Poll during init, switch
      to cooperative scheduling after context initialized
- [ ] **8. CVDisplayLink + CFRunLoop** (H6) — Hardware vsync-driven frame timing
      combined with run loop servicing
- [ ] **9. Correlate SyntheticBeginFrameSource with frame timing** (H5) —
      Diagnostic to understand the 19-sample mystery
- [ ] **10. GUI-side frame timing instrumentation** (H7) — Measure arrival vs
      render time on the GUI side

## Constraints

- **No hidden windows.** Proven to be an architectural dead end (Issue 341).
- **No focus stealing.** The GUI must retain keyboard focus at all times.
- **macOS first.** macOS-specific APIs are acceptable; Linux/Windows are future
  work.
- **Profile server is a separate process.** One CEF instance per profile is a
  hard constraint.
- **No regressions.** Each experiment must match or exceed 38.2fps / 71% at
  60fps.

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  discovery, input path and output path analysis
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Discovery
  that cef-rs OSR example achieves 60fps
- [Issue 341: Performance investigation](./341-performance.md) — 18 experiments,
  hidden window approach discovered and abandoned
- [Issue 342: 60fps without a hidden window](./342-perf-no-win.md) — CFRunLoop
  breakthrough, current 38.2fps baseline established

## Experiments
