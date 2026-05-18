+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

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

## Ideas Checklist

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

### Experiment 1: Drain CFRunLoop Fully

**Status:** Not started

**Goal:** Service all pending CFRunLoop sources per loop iteration instead of
just one, eliminating timing gaps where CEF's internal timers fire but their
callbacks are deferred to the next iteration.

**Hypotheses tested:** H2 (timeout too short), H9 (`return_after_source_handled`
flag)

#### Problem

The current code calls `CFRunLoopRunInMode` once per loop iteration with
`return_after_source_handled: true` (1). This means:

1. If a timer source fires → it's handled → function returns immediately
2. If no source fires within 1ms → function returns on timeout
3. **Either way, at most one source is processed per call**

`CFRunLoopRunInMode` returns one of four values:

| Return value                 | Int | Meaning                             |
| ---------------------------- | --- | ----------------------------------- |
| `kCFRunLoopRunFinished`      | 1   | No sources or timers in this mode   |
| `kCFRunLoopRunStopped`       | 2   | Stopped via `CFRunLoopStop()`       |
| `kCFRunLoopRunTimedOut`      | 3   | Timeout expired, no source handled  |
| `kCFRunLoopRunHandledSource` | 4   | A source was handled (early return) |

When the return value is 4, there may be additional sources ready to fire. The
current code ignores this and proceeds to the next `do_message_loop_work()` +
`CFRunLoopRunInMode` cycle. If CEF has multiple run loop sources that need to
fire within a single 16.67ms compositor window (e.g., SyntheticBeginFrameSource
tick + compositor dispatch + IPC callback), the second and third sources are
delayed by one full loop iteration (~1-2ms). Over several cycles this drift
accumulates, eventually causing a missed compositor beat — which shows up as a
33ms frame (30fps) instead of 16ms (60fps).

#### Changes

Two modifications to `ts3/termsurf-profile/src/main.rs`:

**1. Add a `drain()` function to the `cfrunloop` module:**

Replace the single `run_for()` function with a `drain()` that loops until the
run loop has no more sources to handle:

```rust
/// Drain all pending CFRunLoop sources. Calls CFRunLoopRunInMode in a
/// loop until it returns kCFRunLoopRunTimedOut (3), meaning no more
/// sources are ready. Uses a minimal timeout (0.001s = 1ms) per call
/// to avoid blocking indefinitely.
pub fn drain() {
    const TIMED_OUT: i32 = 3;
    loop {
        let result = unsafe {
            CFRunLoopRunInMode(kCFRunLoopDefaultMode, 0.001, 1)
        };
        if result != 4 {
            // Not kCFRunLoopRunHandledSource — either timed out,
            // finished, or stopped. No more sources to drain.
            break;
        }
        // A source was handled. There may be more — loop again
        // with a fresh timeout.
    }
}
```

The key insight: when `CFRunLoopRunInMode` returns 4
(`kCFRunLoopRunHandledSource`), we immediately call it again with a fresh 1ms
timeout. This continues until it returns 3 (`kCFRunLoopRunTimedOut`), meaning
all pending sources have been serviced.

Safety: the loop cannot spin forever because each call either handles a source
(finite number pending) or times out after 1ms. In the worst case (no sources),
this behaves identically to the current single call.

**2. Replace `cfrunloop::run_for(0.001)` with `cfrunloop::drain()` in the
polling loop:**

```rust
while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
    cef::do_message_loop_work();
    #[cfg(target_os = "macos")]
    cfrunloop::drain();
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_millis(1));
}
```

#### What Stays the Same

- `do_message_loop_work()` is still called every iteration
- CEF settings unchanged (no `external_message_pump`)
- No new dependencies
- Non-macOS fallback still uses `sleep(1ms)`
- All XPC, shutdown, and browser creation code unchanged

#### Expected Outcomes

| Result                       | Meaning                                                                                  |
| ---------------------------- | ---------------------------------------------------------------------------------------- |
| >80% at 60fps, higher streak | Multiple sources were being starved. H2/H9 confirmed.                                    |
| ~71% at 60fps (unchanged)    | Only one source fires per cycle anyway. H2/H9 ruled out. Investigate H1/H3 next.         |
| Performance regression       | Draining too aggressively delays `do_message_loop_work()`. Try capping drain iterations. |

#### Risk

Low. The drain loop adds at most a few microseconds per extra source handled. If
only one source ever fires (the common case today), the behavior is identical to
the current code — one call returns 4, the next returns 3, loop exits after two
calls instead of one.

#### Results

**Status: FAILED** — Performance regressed across every metric.

**Overall stats:** 495 frames over 16.1s = **30.6 fps average**

| Interval        | Count | Percentage |
| --------------- | ----- | ---------- |
| Burst (0-5ms)   | 18    | 3.6%       |
| 60fps (6-20ms)  | 303   | **61.3%**  |
| 30fps (21-40ms) | 108   | 21.9%      |
| Mid (41-70ms)   | 25    | 5.1%       |
| Low (>70ms)     | 40    | 8.1%       |

**Dominant intervals:** 17ms (79), 16ms (76), 18ms (70) — still vsync-aligned,
but weaker than before. Secondary peak at 33-35ms (87 combined).

**Max consecutive 60fps frames:** **62** (top streaks: 62, 21, 20, 19, 18)

CEF debug log histograms:

- `Viz.ExternalBeginFrameSourceMac.DisplayLink`: 3 samples (unchanged)
- `Viz.ExternalBeginFrameSource.Interval`: **11 samples**, mean 16ms (down from
  19 in Issue 342 Exp 5 — SyntheticBeginFrameSource firing less often)
- `Viz.FrameSinkVideoCapturer.CaptureDuration`: 494 samples, mean 7.6ms
- `Graphics.Smoothness.PercentDroppedFrames3.AllSequences`: **22.2%** (up from
  19% — more frames dropped)

#### Comparison

| Metric                    | Issue 342 Exp 5 (baseline) | **Exp 1 (drain)** |
| ------------------------- | -------------------------- | ----------------- |
| Average FPS               | 38.2                       | **30.6**          |
| Frames at 60fps           | 71%                        | **61.3%**         |
| Frames at 30fps           | ~15%                       | **21.9%**         |
| Max consecutive 60fps     | 424                        | **62**            |
| SyntheticBeginFrame fires | 19                         | **11**            |
| PercentDroppedFrames      | 19%                        | **22.2%**         |

#### Conclusion

Draining all CFRunLoop sources made everything worse. The
SyntheticBeginFrameSource dropped from 19 to 11 samples — we starved it more,
not less. The max streak collapsed from 424 to 62.

The cause: draining multiple sources delays the return to
`do_message_loop_work()`. CEF's compositor posts tasks to its internal queue
that require `do_message_loop_work()` to process. By spending extra time in the
CFRunLoop drain loop, we defer that processing, causing missed compositor beats.

This rules out **H2** (timeout too short) and **H9**
(`return_after_source_handled` flag). The single-source-per-call behavior in
Issue 342 Exp 5 was actually optimal — it provides the tightest interleaving
between CFRunLoop source handling and `do_message_loop_work()` task processing.

The result supports **H3** (the two systems fighting): `do_message_loop_work()`
and CFRunLoop need tight, alternating interleaving. More aggressive draining of
either side hurts the other. The next experiment should investigate this
relationship — either by removing `do_message_loop_work()` entirely (letting
CFRunLoop drive everything) or by tuning the balance between the two calls.

### Experiment 2: Set Thread QoS to USER_INTERACTIVE

**Status:** Not started

**Goal:** Eliminate macOS timer coalescing as a source of missed compositor
beats by setting the main thread's Quality of Service class to the highest
level.

**Hypothesis tested:** H10 (process priority and QoS)

#### Problem

macOS assigns a QoS class to every thread. The QoS class determines:

- **Scheduling priority** — how quickly the thread gets CPU time after becoming
  runnable
- **Timer precision** — whether timers fire at their exact deadline or get
  coalesced with nearby timers to save power
- **Timer leeway** — the system-imposed tolerance on timer firing times

The five QoS classes, from lowest to highest:

| QoS Class                    | Value | Timer behavior                           |
| ---------------------------- | ----- | ---------------------------------------- |
| `QOS_CLASS_BACKGROUND`       | 0x09  | Aggressive coalescing, low priority      |
| `QOS_CLASS_UTILITY`          | 0x11  | Moderate coalescing                      |
| `QOS_CLASS_DEFAULT`          | 0x15  | Standard behavior                        |
| `QOS_CLASS_USER_INITIATED`   | 0x19  | Reduced coalescing                       |
| `QOS_CLASS_USER_INTERACTIVE` | 0x21  | **Minimal coalescing, highest priority** |

The profile server is a windowless background process. macOS likely assigns it
`QOS_CLASS_DEFAULT` or lower. At this level, the system is permitted to coalesce
CFRunLoop timer firings by several milliseconds — grouping them with other
timers to reduce CPU wake-ups and save power.

CEF's `SyntheticBeginFrameSource` is a CFRunLoop timer set to fire every
16.67ms. If macOS coalesces this timer even slightly (e.g., delays it by 2-3ms),
the callback fires at 19ms instead of 16.67ms. The compositor misses its
deadline for the current vsync beat and the frame slips to the next one —
producing a 33ms interval (30fps) instead of 16ms (60fps).

This would explain the bimodal pattern: most frames hit 16-17ms (timer fires on
time), but ~20% land at 33-35ms (timer coalesced past the deadline). The pattern
is not random jitter — it's the exact missed-beat signature of timer coalescing.

#### Changes

One addition to `ts3/termsurf-profile/src/main.rs`:

**Add a `pthread_set_qos_class_self_np` call before the polling loop:**

```rust
// Issue 343, Experiment 2: Set highest QoS for maximum timer precision.
#[cfg(target_os = "macos")]
unsafe {
    extern "C" {
        fn pthread_set_qos_class_self_np(qos_class: u32, relative_priority: i32) -> i32;
    }
    let QOS_CLASS_USER_INTERACTIVE: u32 = 0x21;
    let ret = pthread_set_qos_class_self_np(QOS_CLASS_USER_INTERACTIVE, 0);
    println!("Profile: Set QoS to USER_INTERACTIVE: {}", if ret == 0 { "ok" } else { "failed" });
}
```

This goes just before the `while !QUIT_FLAG` loop, after Ctrl+C handler setup.
The call sets the current thread (main thread) to the highest QoS class, telling
the kernel this thread is doing user-interactive work that requires maximum
responsiveness.

#### What Stays the Same

- Polling loop structure unchanged (`do_message_loop_work()` + `run_for(0.001)`)
- CEF settings unchanged
- No new dependencies (uses raw FFI to a single POSIX function)
- All other code unchanged

#### Expected Outcomes

| Result                          | Meaning                                                        |
| ------------------------------- | -------------------------------------------------------------- |
| >80% at 60fps, fewer 33ms drops | Timer coalescing was the cause. H10 confirmed.                 |
| ~71% at 60fps (unchanged)       | Timer precision is already adequate. H10 ruled out.            |
| Performance regression          | Extremely unlikely — higher QoS cannot reduce timer precision. |

#### Risk

Effectively zero. `QOS_CLASS_USER_INTERACTIVE` is what every GUI app's main
thread runs at. It increases CPU priority and timer precision — it cannot make
timers less accurate. The only cost is slightly higher power consumption, which
is irrelevant for a process that's already polling at 1ms intervals.

#### Results

**Status: FAILED** — Performance regressed despite QoS being set successfully.

QoS was confirmed set: `Profile: Set QoS to USER_INTERACTIVE: ok`

**Overall stats:** 298 frames over 10.2s = **28.9 fps average**

| Interval        | Count | Percentage |
| --------------- | ----- | ---------- |
| Burst (0-5ms)   | 18    | 6.1%       |
| 60fps (6-20ms)  | 192   | **64.6%**  |
| 30fps (21-40ms) | 23    | 7.7%       |
| Mid (41-70ms)   | 42    | **14.1%**  |
| Low (>70ms)     | 22    | 7.4%       |

**Dominant intervals:** 17ms (126), 16ms (62) — vsync-aligned peak intact. New
cluster at 50ms (25 occurrences) and 66-67ms (10 combined) that did not exist in
the baseline.

**Max consecutive 60fps frames:** **61** (top streaks: 61, 35, 18, 7, 7)

CEF debug log histograms:

- `Viz.ExternalBeginFrameSourceMac.DisplayLink`: 3 samples (unchanged)
- `Viz.ExternalBeginFrameSource.Interval`: 17 samples, mean 16ms (down from 19)
- `Viz.FrameSinkVideoCapturer.CaptureDuration`: 297 samples, mean 8.4ms
- `Graphics.Smoothness.PercentDroppedFrames3.AllSequences`: **22.5%** (up from
  19%)

#### Comparison

| Metric                    | Issue 342 Exp 5 (baseline) | **Exp 2 (QoS)** |
| ------------------------- | -------------------------- | --------------- |
| Average FPS               | 38.2                       | **28.9**        |
| Frames at 60fps           | 71%                        | **64.6%**       |
| Frames at 30fps           | ~15%                       | **7.7%**        |
| Mid (41-70ms)             | —                          | **14.1%**       |
| Max consecutive 60fps     | 424                        | **61**          |
| SyntheticBeginFrame fires | 19                         | **17**          |
| PercentDroppedFrames      | 19%                        | **22.5%**       |

#### Conclusion

QoS made things worse. While the 30fps bucket improved (7.7% vs ~15%), a new
41-70ms cluster appeared (14.1%), centered on 50ms (3 vsync beats) and 66ms (4
vsync beats). These multi-beat misses didn't exist in the baseline and dragged
the average FPS down from 38.2 to 28.9.

The higher scheduling priority may have changed how macOS interleaves the main
thread with CEF's internal threads (GPU process communication, IPC handlers).
With `QOS_CLASS_USER_INTERACTIVE`, the main thread gets more aggressive
scheduling, potentially starving CEF's background threads that feed work into
the compositor pipeline.

**H10 is ruled out.** Timer coalescing was not the cause of the 30fps drops. The
problem is not scheduling precision — it's something in the interaction between
`do_message_loop_work()`, CFRunLoop, and CEF's internal task pipeline.

### Experiment 3: Measure Loop Iteration Timing

**Status:** Not started

**Goal:** Instrument the polling loop with microsecond-precision timing to
understand the actual behavior of `do_message_loop_work()` and
`CFRunLoopRunInMode` — how long each call takes, how they vary, and whether
spikes correlate with dropped frames.

**Hypothesis tested:** H1 (polling loop timing mismatch), plus general
diagnostics to guide future experiments.

#### Motivation

Two interventions have failed (Exp 1: drain, Exp 2: QoS). Both changed timing
behavior without understanding it, and both made things worse. Before trying
another intervention, we need to see what the loop is actually doing.

Key questions:

1. **`do_message_loop_work()` duration** — Is it consistently fast (<0.1ms)? Or
   does it spike when processing compositor tasks (5-10ms)? Spikes would
   directly explain missed beats.
2. **`CFRunLoopRunInMode` duration** — Does it always block for the full 1ms
   timeout? Or does it sometimes return instantly (source handled in
   microseconds)? This reveals how often CFRunLoop sources actually fire.
3. **Total loop iteration time** — How consistent is the cadence? If most
   iterations take 1.2ms but some take 8ms, timing drift is the cause.
4. **Correlation with frame drops** — Do `do_message_loop_work()` spikes precede
   33ms frame intervals? If yes, the spike is the cause. If no, the problem is
   elsewhere (CEF internal, GPU process, IPC).

#### Changes

One modification to `ts3/termsurf-profile/src/main.rs`:

**Replace the polling loop with an instrumented version:**

```rust
// Issue 343, Experiment 3: Instrumented polling loop.
println!("Profile: Running message loop (instrumented)...");
let mut loop_count: u64 = 0;
let mut max_mlw_us: u128 = 0;  // max do_message_loop_work duration
let mut max_cfl_us: u128 = 0;  // max CFRunLoopRunInMode duration
let mut max_total_us: u128 = 0; // max total iteration
let mut mlw_spike_count: u64 = 0; // iterations where mlw > 1ms
let mut cfl_instant_count: u64 = 0; // iterations where cfl < 0.1ms

while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
    let t0 = std::time::Instant::now();

    cef::do_message_loop_work();
    let t1 = std::time::Instant::now();

    #[cfg(target_os = "macos")]
    cfrunloop::run_for(0.001);
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_millis(1));
    let t2 = std::time::Instant::now();

    let mlw_us = (t1 - t0).as_micros();
    let cfl_us = (t2 - t1).as_micros();
    let total_us = (t2 - t0).as_micros();

    if mlw_us > max_mlw_us { max_mlw_us = mlw_us; }
    if cfl_us > max_cfl_us { max_cfl_us = cfl_us; }
    if total_us > max_total_us { max_total_us = total_us; }
    if mlw_us > 1000 { mlw_spike_count += 1; }
    if cfl_us < 100 { cfl_instant_count += 1; }

    loop_count += 1;

    // Log every 1000 iterations (~1 second)
    if loop_count % 1000 == 0 {
        println!(
            "[LOOP-TIMING] iter={} max_mlw={}us max_cfl={}us max_total={}us mlw_spikes={} cfl_instant={}",
            loop_count, max_mlw_us, max_cfl_us, max_total_us, mlw_spike_count, cfl_instant_count
        );
    }
}

// Final summary
println!(
    "[LOOP-TIMING] FINAL iter={} max_mlw={}us max_cfl={}us max_total={}us mlw_spikes={} cfl_instant={}",
    loop_count, max_mlw_us, max_cfl_us, max_total_us, mlw_spike_count, cfl_instant_count
);
```

The instrumentation adds three `Instant::now()` calls per iteration (each ~20ns
on Apple Silicon) — negligible overhead relative to the 1ms+ loop cadence.

#### What This Measures

| Metric        | What it tells us                                                                                                           |
| ------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `max_mlw`     | Worst-case `do_message_loop_work()` duration. If >5ms, it's eating into the frame budget.                                  |
| `max_cfl`     | Worst-case `CFRunLoopRunInMode` duration. Should be ~1ms (timeout). If much longer, macOS is blocking us.                  |
| `max_total`   | Worst-case loop iteration. If >16ms, we're guaranteed to miss a beat.                                                      |
| `mlw_spikes`  | How often `do_message_loop_work()` takes >1ms. Frequent spikes = H1 confirmed.                                             |
| `cfl_instant` | How often CFRunLoop returns instantly (<0.1ms). High count = sources rarely fire (most iterations are just timeout waits). |

#### What Stays the Same

- Loop behavior is identical — same `do_message_loop_work()` + `run_for(0.001)`
- CEF settings unchanged
- No new dependencies
- Frame production and XPC communication unaffected

#### Expected Outcomes

| Pattern                                     | Meaning                                                        | Next step                                                          |
| ------------------------------------------- | -------------------------------------------------------------- | ------------------------------------------------------------------ |
| `mlw_spikes` is high, correlates with drops | `do_message_loop_work()` occasionally blocks. H1 confirmed.    | Try `external_message_pump` (idea 7) for cooperative scheduling    |
| `cfl_instant` is very high (>90%)           | CFRunLoop sources rarely fire — most iterations are idle waits | The 1ms timeout is mostly wasted. Try shorter timeout or busy-poll |
| `max_total` stays under 2ms                 | Loop cadence is rock-solid. The problem is not in the loop.    | Investigate CEF internals (idea 9) or GUI side (idea 10)           |
| `max_mlw` or `max_total` occasionally >10ms | Rare but large spikes cause the 33ms drops                     | Try yielding differently after spikes, or cap iteration time       |

#### Risk

None. This is purely diagnostic — additive logging with negligible overhead. The
loop behavior is bit-for-bit identical to the baseline.

#### Results

**Status: DIAGNOSTIC COMPLETE** — Reveals `do_message_loop_work()` is the
dominant cost, not CFRunLoop.

**Loop timing data:**

```
[LOOP-TIMING] FINAL iter=865 max_mlw=33293us max_cfl=747us max_total=33326us mlw_spikes=865 cfl_instant=831
```

| Metric                 | Value          | Meaning                                         |
| ---------------------- | -------------- | ----------------------------------------------- |
| Total iterations       | 865            | ~65 iter/sec (not ~1000 as 1ms sleep implies)   |
| `max_mlw`              | **33,293us**   | `do_message_loop_work()` blocks up to 33ms      |
| `max_cfl`              | 747us          | `CFRunLoopRunInMode` well-behaved, always <1ms  |
| `max_total`            | 33,326us       | Worst iteration = 33ms, entirely from mlw       |
| `mlw_spikes` (>1ms)    | **865 of 865** | mlw takes >1ms on **every single call**         |
| `cfl_instant` (<0.1ms) | **831 of 865** | CFRunLoop returns instantly **96% of the time** |

**Frame performance:**

453 frames over 13.4s = **33.7 fps average**

| Interval        | Count | Percentage |
| --------------- | ----- | ---------- |
| Burst (0-5ms)   | 19    | 4.2%       |
| 60fps (6-20ms)  | 374   | **82.7%**  |
| 30fps (21-40ms) | 23    | 5.1%       |
| Mid (41-70ms)   | 8     | 1.8%       |
| Low (>70ms)     | 28    | 6.2%       |

**Dominant intervals:** 17ms (236), 16ms (128) — strong vsync-aligned peak.

**Max consecutive 60fps frames:** **89** (top streaks: 89, 45, 34, 26, 18)

CEF debug log histograms:

- `Viz.ExternalBeginFrameSource.Interval`: 13 samples, mean 16ms
- `Viz.FrameSinkVideoCapturer.CaptureDuration`: 453 samples, mean 8.0ms
- `Graphics.Smoothness.PercentDroppedFrames3.AllSequences`: 21.0%

#### Key Findings

1. **`do_message_loop_work()` takes >1ms on every call.** This is not a fast
   function with occasional spikes — it consistently consumes 1-33ms per call.
   It is the dominant cost in the loop, accounting for >95% of iteration time.

2. **CFRunLoop sources almost never fire.** 96% of iterations, CFRunLoop returns
   in <0.1ms with nothing to do. The run loop is not what's driving frame
   production — `do_message_loop_work()` is. CFRunLoop's role is limited to the
   4% of iterations where it actually services a source.

3. **Only 865 iterations in 13.4 seconds.** At ~65 iterations/sec, the loop runs
   far slower than the theoretical 1000/sec that a 1ms sleep would produce.
   `do_message_loop_work()` consumes the time budget, leaving almost nothing for
   CFRunLoop.

4. **The worst-case 33ms spike matches the 30fps drops exactly.** When
   `do_message_loop_work()` blocks for 33ms, it consumes two entire vsync
   periods in a single call, producing the characteristic 33ms frame interval.

5. **82.7% at 60fps — higher than baseline (71%).** The `Instant::now()` calls
   add ~60ns of overhead per iteration, which may subtly alter the interleaving
   rhythm. This is noise, not a real improvement, but it suggests the system is
   sensitive to tiny timing changes.

#### Conclusion

**H1 is confirmed** — the polling loop timing mismatch is real, but it's not
drift or jitter. It's that `do_message_loop_work()` itself is the bottleneck. It
blocks for variable durations (1-33ms), consuming the entire frame budget and
leaving CFRunLoop with almost no time to service its sources.

This reframes the problem: the Issue 342 CFRunLoop fix helped not because
CFRunLoop sources needed to fire frequently, but because the 4% of iterations
where a source fires are critical — they're the SyntheticBeginFrameSource timer
ticks that trigger compositor cycles. Without CFRunLoop servicing, those 4% of
critical moments never happen at all.

**Implications for next experiments:**

- **Idea 3 (remove `do_message_loop_work()`) is now the most interesting.** If
  `do_message_loop_work()` is consuming 95% of the time and CFRunLoop sources
  drive the actual frame scheduling, what happens if we let CFRunLoop run longer
  and call `do_message_loop_work()` less frequently — or not at all?
- **Idea 7 (two-phase `external_message_pump`)** is also motivated: cooperative
  scheduling via `on_schedule_message_pump_work` would let CEF tell us exactly
  when it needs `do_message_loop_work()`, instead of calling it blindly every
  iteration.
- **Idea 4 (increase CFRunLoop timeout)** deserves revisiting. With mlw taking
  1-33ms anyway, increasing the CFRunLoop timeout from 1ms to 16ms wouldn't
  change the loop cadence much — but it would give CFRunLoop sources much more
  opportunity to fire during the 96% of idle iterations.

### Experiment 4: Instrument cef-rs OSR Example Loop

**Status:** Not started

**Goal:** Add the same microsecond-precision timing instrumentation from
Experiment 3 to the cef-rs OSR example's main loop, so we can compare the two
processes side-by-side and understand what's different about the 60fps example.

**Target:** `cef-rs/examples/osr/src/main.rs` (not `termsurf-profile`)

#### Motivation

Experiment 3 revealed that `do_message_loop_work()` takes >1ms on every call in
the profile server, consuming 95% of the loop's time budget. But the cef-rs OSR
example calls the same function in the same loop pattern and achieves 60fps.

Either:

1. **`do_message_loop_work()` is faster in the example** — perhaps because the
   visible window and event loop provide a different execution context that
   makes CEF process tasks more efficiently.
2. **`do_message_loop_work()` is equally slow, but `pump_app_events`
   compensates** — winit's event pump does something that our
   `CFRunLoopRunInMode(0.001)` does not, and that something is what actually
   drives frame production.
3. **`do_message_loop_work()` is equally slow, and the example's overall FPS is
   closer to ours than we thought** — Issue 341 Exp 3 measured 36.8fps overall
   (60fps only during sustained active rendering). Perhaps the gap is smaller
   than assumed.

This experiment answers which of these three is true.

#### Changes

One modification to `cef-rs/examples/osr/src/main.rs`:

**Instrument the main loop with the same timing as Experiment 3:**

```rust
let mut loop_count: u64 = 0;
let mut max_mlw_us: u128 = 0;
let mut max_pae_us: u128 = 0;  // pump_app_events instead of CFRunLoop
let mut max_total_us: u128 = 0;
let mut mlw_spike_count: u64 = 0;
let mut pae_instant_count: u64 = 0;

let ret = loop {
    let t0 = std::time::Instant::now();

    do_message_loop_work();
    let t1 = std::time::Instant::now();

    let timeout = Some(Duration::from_millis(1));
    let status = event_loop.pump_app_events(timeout, &mut app);
    let t2 = std::time::Instant::now();

    let mlw_us = (t1 - t0).as_micros();
    let pae_us = (t2 - t1).as_micros();
    let total_us = (t2 - t0).as_micros();

    if mlw_us > max_mlw_us { max_mlw_us = mlw_us; }
    if pae_us > max_pae_us { max_pae_us = pae_us; }
    if total_us > max_total_us { max_total_us = total_us; }
    if mlw_us > 1000 { mlw_spike_count += 1; }
    if pae_us < 100 { pae_instant_count += 1; }

    loop_count += 1;

    if loop_count % 1000 == 0 {
        println!(
            "[LOOP-TIMING] iter={} max_mlw={}us max_pae={}us max_total={}us mlw_spikes={} pae_instant={}",
            loop_count, max_mlw_us, max_pae_us, max_total_us, mlw_spike_count, pae_instant_count
        );
    }

    if let PumpStatus::Exit(exit_code) = status {
        break ExitCode::from(exit_code as u8);
    }
};

println!(
    "[LOOP-TIMING] FINAL iter={} max_mlw={}us max_pae={}us max_total={}us mlw_spikes={} pae_instant={}",
    loop_count, max_mlw_us, max_pae_us, max_total_us, mlw_spike_count, pae_instant_count
);
```

#### What We're Comparing

| Metric                            | Profile server (Exp 3)    | cef-rs OSR (this exp) |
| --------------------------------- | ------------------------- | --------------------- |
| `do_message_loop_work()` duration | >1ms every call, max 33ms | ?                     |
| Event pump duration               | <0.1ms 96% of the time    | ?                     |
| Total iteration                   | ~65 iter/sec              | ?                     |
| `mlw_spikes` (>1ms)               | 100%                      | ?                     |
| Event pump instant (<0.1ms)       | 96%                       | ?                     |

If `do_message_loop_work()` behaves identically in both processes, then the
difference is entirely in `pump_app_events` vs `CFRunLoopRunInMode`. If mlw is
faster in the example, then the visible window changes CEF's internal behavior.

#### Build and Run

```bash
cd cef-rs && ./scripts/build-osr.sh --open
```

Interact with the app for 10-15 seconds (scroll, click, navigate), then close
it. The timing data will print to stdout.

#### Risk

None. Purely diagnostic. The cef-rs example is a standalone test app — no impact
on the profile server or GUI.

#### Results

**Status: DIAGNOSTIC COMPLETE** — Reveals the root cause of the performance gap.

**Loop timing data (at 1000 iterations):**

```
[LOOP-TIMING] iter=1000 max_mlw=12702us max_pae=570855us max_total=570861us mlw_spikes=57 pae_instant=0
```

| Metric                 | Value          | Meaning                                       |
| ---------------------- | -------------- | --------------------------------------------- |
| Total iterations       | 1000           | Sample at the 1000th iteration checkpoint     |
| `max_mlw`              | **12,702us**   | `do_message_loop_work()` max is 12.7ms        |
| `max_pae`              | **570,855us**  | `pump_app_events` blocks up to 570ms          |
| `max_total`            | 570,861us      | Worst iteration dominated by pae              |
| `mlw_spikes` (>1ms)    | **57 of 1000** | mlw takes >1ms on only **5.7%** of calls      |
| `pae_instant` (<0.1ms) | **0 of 1000**  | `pump_app_events` **never** returns instantly |

#### Comparison

| Metric              | Profile server (Exp 3) | **cef-rs OSR (Exp 4)** |
| ------------------- | ---------------------- | ---------------------- |
| `mlw_spikes` (>1ms) | 865 of 865 (100%)      | **57 of 1000 (5.7%)**  |
| `max_mlw`           | 33,293us (33ms)        | **12,702us (12.7ms)**  |
| Event pump instant  | 831 of 865 (96%)       | **0 of 1000 (0%)**     |
| `max` event pump    | 747us                  | **570,855us (570ms)**  |

#### Conclusion

**The root cause of the performance gap is identified.**

`do_message_loop_work()` is **17x less likely to spike** in the cef-rs example
(5.7% vs 100%). The same function, the same CEF version — but radically
different behavior depending on what happens between calls.

The explanation: **`pump_app_events` offloads work that would otherwise
accumulate in CEF's internal task queue.** Winit's `pump_app_events` runs the
full macOS `NSApplication` event loop — processing CFRunLoop sources, window
server events, display link callbacks, and Core Animation commits. When the
event pump handles these tasks, `do_message_loop_work()` finds an almost-empty
queue and returns in microseconds.

In the profile server, `CFRunLoopRunInMode(0.001)` returns instantly 96% of the
time with nothing to do. All the work accumulates in CEF's internal task queue,
and `do_message_loop_work()` has to process the entire backlog on every call —
taking 1-33ms.

The fix is not about timer precision (Exp 2), source draining (Exp 1), or thread
priority. It's that our event pump equivalent is too weak. We need to run the
macOS event loop more thoroughly between `do_message_loop_work()` calls, so that
system-level tasks get processed by the OS instead of piling up in CEF's queue.

**Three directions this points to:**

1. **Longer CFRunLoop timeout.** Increase from 1ms to 16ms so CFRunLoop has time
   to process pending sources instead of timing out instantly. With mlw
   averaging several milliseconds anyway, the extra timeout won't slow the loop.
2. **Use `NSApplication` event pumping.** Issue 341 Exp 14 tried this with a
   native NSWindow and it didn't help — but that was before the CFRunLoop fix
   and without `external_message_pump`. Combining NSApp event pumping with our
   current setup may produce different results.
3. **Investigate what `pump_app_events` actually does internally.** Winit's
   macOS backend runs `nextEventMatchingMask:untilDate:inMode:dequeue:` which
   drives the full `NSRunLoop`. There may be a specific run loop mode or
   configuration that we need to match.

### Experiment 5: Increase CFRunLoop Timeout to 16ms

**Status:** Not started

**Goal:** Give CFRunLoop enough time to catch pending timer sources by
increasing the timeout from 1ms to 16ms — one full vsync period.

**Hypothesis tested:** H1 (timing mismatch), informed by Exp 3 and Exp 4 data.

#### Motivation

Experiments 3 and 4 revealed that:

- `CFRunLoopRunInMode(0.001)` returns instantly 96% of the time (Exp 3)
- `pump_app_events` in the cef-rs example **never** returns instantly (Exp 4)
- `do_message_loop_work()` takes >1ms on 100% of calls in our process but only
  5.7% in the example (Exp 3 vs Exp 4)

The 96% instant return rate could mean CFRunLoop sources are scheduled to fire
_after_ the 1ms timeout expires. The SyntheticBeginFrameSource fires every
16.67ms. If it's scheduled to fire in 2ms at the moment we call
`CFRunLoopRunInMode(0.001)`, we timeout after 1ms and miss it. The source fires
unhandled, and `do_message_loop_work()` has to process the resulting backlog.

With a 16ms timeout, we'd wait long enough to catch any source scheduled within
the current frame period. The SyntheticBeginFrameSource at 16.67ms would almost
always fire within our timeout window.

#### Why This Is Different from Experiment 1

Experiment 1 (drain) failed because it made **many rapid-fire short calls** to
CFRunLoop, delaying the return to `do_message_loop_work()`. This experiment
makes **one longer call**, giving the run loop continuous time to process
sources naturally. The distinction:

| Approach         | CFRunLoop calls per iteration | Time in CFRunLoop                           | Time before next mlw               |
| ---------------- | ----------------------------- | ------------------------------------------- | ---------------------------------- |
| Baseline (1ms)   | 1 × 1ms                       | ~0.1ms (96% instant)                        | ~0.1ms                             |
| Exp 1 (drain)    | N × 1ms (loop until timeout)  | Variable, delays mlw                        | Variable                           |
| **Exp 5 (16ms)** | **1 × 16ms**                  | **Up to 16ms, but returns early on source** | **Immediate after source handled** |

With `return_after_source_handled: true`, CFRunLoop returns as soon as it
handles one source — it doesn't block for the full 16ms. The 16ms is a maximum
wait, not a minimum. If a source fires after 3ms, we get it and return to
`do_message_loop_work()` immediately.

#### Changes

One change to `ts3/termsurf-profile/src/main.rs`:

**Change the CFRunLoop timeout from 0.001 to 0.016:**

```rust
#[cfg(target_os = "macos")]
cfrunloop::run_for(0.016); // 16ms — one vsync period
```

That's it. One number change.

#### What Stays the Same

- `do_message_loop_work()` still called every iteration
- `return_after_source_handled` still true (returns early on source)
- CEF settings unchanged
- No new dependencies
- All other code unchanged

#### Expected Outcomes

| Result                             | Meaning                                                                                                                                                   |
| ---------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| >80% at 60fps, `cfl_instant` drops | Sources are firing within the 16ms window that the 1ms timeout was missing. The timing mismatch was the cause.                                            |
| ~71% at 60fps (unchanged)          | Sources aren't pending — the 96% instant rate means there truly are no sources to process, not that we're timing out too early. The problem is elsewhere. |
| Performance regression             | The 16ms timeout delays `do_message_loop_work()` on iterations where no source fires, starving CEF's task queue.                                          |

#### Risk

Low. With `return_after_source_handled: true`, the function returns immediately
when a source fires — the 16ms is only the maximum wait. On iterations where a
source fires quickly, the behavior is identical to the 1ms timeout. The only
difference is on the 96% of iterations where no source fires: those will now
wait up to 16ms instead of 1ms. But since `do_message_loop_work()` already takes
1-33ms per call, an extra 15ms of waiting on empty iterations may not change the
effective cadence much — or it may give CEF's internal threads time to post work
that `do_message_loop_work()` can then process more efficiently.

#### Results

**Status: FAILED — regression across all metrics.**

| Metric                 | Baseline | Exp 5 | Delta  |
| ---------------------- | -------- | ----- | ------ |
| Total frames           | —        | 390   | —      |
| Duration               | —        | 12.4s | —      |
| Average FPS            | 38.2     | 31.3  | -18%   |
| 60fps range (excl dup) | 71.0%    | 65.8% | -5.2pp |
| Max streak             | 424      | 24    | -94%   |

**Loop timing (instrumentation from Exp 3):**

```
FINAL iter=810 max_mlw=33244us max_cfl=953us max_total=33278us mlw_spikes=810 cfl_instant=466
```

- 810 iterations in 12.4s (~65/sec) — same iteration rate as Exp 3
- `mlw_spikes=810` — 100% of `do_message_loop_work()` calls >1ms, unchanged
- `cfl_instant=466` — 57.5% instant returns (down from 96% at 1ms timeout)
- `max_cfl=953us` — CFRunLoop never blocks for the full 16ms, caps at ~1ms

**Frame interval distribution (389 intervals):**

| Bucket                  | Count | Percent |
| ----------------------- | ----- | ------- |
| 0-1ms (batch/duplicate) | 56    | 14.4%   |
| 13-20ms (60fps, good)   | 219   | 56.3%   |
| 21-34ms (30-47fps)      | 50    | 12.9%   |
| 35-50ms (missed)        | 14    | 3.6%    |
| 51-100ms (stall)        | 38    | 9.8%    |
| 101-200ms (very bad)    | 5     | 1.3%    |
| 200+ms (terrible)       | 7     | 1.8%    |

#### Conclusion

The 16ms timeout matched the "Performance regression" expected outcome. The
longer timeout did not catch any additional CFRunLoop sources — `max_cfl` was
only 953us, meaning CFRunLoop never waited anywhere close to 16ms. The instant
return rate dropped from 96% to 57.5%, but that's because the longer timeout let
some sub-1ms sources fire that previously timed out — not because we caught any
new 2-15ms sources.

The max streak collapse from 424 to 24 is the most telling result. The longer
timeout introduced unpredictable delays that disrupted the frame cadence without
providing any offsetting benefit. The rendering pipeline can hit 60fps but
cannot sustain it past ~24 frames before a stall interrupts.

**Hypotheses ruled out:**

- **H1 (timing mismatch) — partially ruled out.** The specific mechanism
  proposed (1ms timeout missing sources scheduled 2-16ms in the future) is
  disproven. CFRunLoop sources are not pending at those timescales. However, H1
  in the broader sense (our event pump is inadequate compared to
  `pump_app_events`) remains open — the problem may not be _when_ we pump but
  _what_ we pump.

**Key insight:** The difference between our `CFRunLoopRunInMode` and the cef-rs
example's `pump_app_events` is not about timeout duration. It's about what each
function processes. `pump_app_events` runs the full `NSApplication` event loop
including `nextEventMatchingMask:untilDate:inMode:dequeue:`, which processes
window server events, display link callbacks, and Core Animation commits.
`CFRunLoopRunInMode` only processes CFRunLoop sources. The work that makes
`do_message_loop_work()` spike to >1ms on every call may be work that should
have been handled by the NSApplication event loop instead.

### Experiment 6: Remove `do_message_loop_work()`

**Status:** Not started

**Goal:** Determine whether `do_message_loop_work()` is redundant when CFRunLoop
is being serviced — and whether removing it improves frame delivery.

**Hypothesis tested:** H3 (`do_message_loop_work()` and CFRunLoop fighting).

#### Motivation

Five experiments have established a clear picture:

- `do_message_loop_work()` takes >1ms on **100% of calls** (Exp 3), up to 33ms
- `CFRunLoopRunInMode` returns instantly **96% of the time** (Exp 3)
- In the cef-rs example, `do_message_loop_work()` spikes only **5.7%** of calls
  because `pump_app_events` handles most of the work (Exp 4)
- Increasing CFRunLoop timeout to 16ms didn't help — no sources are pending at
  longer timescales (Exp 5)

The consistent finding is that `do_message_loop_work()` is doing _all_ the work
while CFRunLoop does _nothing_. But in the cef-rs example, the opposite is true
— the event pump does the heavy lifting. This suggests the two systems may be
fighting: `do_message_loop_work()` drains the task queue before CFRunLoop
sources have a chance to fire, making CFRunLoop perpetually empty.

If we remove `do_message_loop_work()`, CEF's internal timers and sources on the
CFRunLoop may process the same work organically — the way they were designed to
when a run loop is active. The question is whether CEF posts all its work as
CFRunLoop sources, or whether some work only lives in the internal task queue
that `do_message_loop_work()` drains.

#### Changes

One change to `ts3/termsurf-profile/src/main.rs`:

**Remove the `do_message_loop_work()` call and increase CFRunLoop timeout to
16ms:**

```rust
while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
    #[cfg(target_os = "macos")]
    cfrunloop::run_for(0.016); // 16ms — let CFRunLoop drive everything
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_millis(16));
}
```

The Exp 3 instrumentation is removed since there's no `do_message_loop_work()`
to time. Frame delivery is still measured by the existing `[FRAME-TX]` logging.

We use 16ms (one vsync period) so CFRunLoop has enough time to catch any pending
sources per frame.

#### What Stays the Same

- CEF settings unchanged (`external_message_pump` is false)
- `cfrunloop` module unchanged
- XPC, IOSurface, render handler all unchanged
- `[FRAME-TX]` frame logging still active

#### Expected Outcomes

| Result                                           | Meaning                                                                                                                                                       |
| ------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Frames still delivered, quality improves         | CFRunLoop sources handle the work that `do_message_loop_work()` was doing. The explicit call was redundant and its overhead was causing stalls. H3 confirmed. |
| Frames still delivered, quality similar or worse | CFRunLoop can drive CEF but not efficiently. The work isn't fought over — it's just slow either way. Need to explore NSApplication event loop (idea 7).       |
| No frames delivered (OnPaint stops firing)       | `do_message_loop_work()` is essential — CFRunLoop sources alone cannot drive CEF's rendering pipeline. H3 ruled out. Need a different approach entirely.      |

#### Risk

Medium. If `do_message_loop_work()` is the only thing driving CEF's rendering,
removing it will produce zero frames. But the app won't crash — CEF will simply
be idle. The `[FRAME-TX]` log will show immediately whether frames are arriving
or not, and we can kill the process and revert.

#### Results

**Status: FAILED — no frames delivered. Webview did not open at all.**

The app launched but the webview never rendered. No `[FRAME-TX]` entries were
logged. CEF's `OnPaint` callback never fired. The log file was not updated
(still contained Exp 5 data with "Running message loop (instrumented)..."
header).

This matches the third expected outcome: "No frames delivered (OnPaint stops
firing)."

#### Conclusion

`do_message_loop_work()` is **essential**. CFRunLoop sources alone cannot drive
CEF's rendering pipeline. Without the explicit call, CEF never processes its
internal task queue, never triggers layout/paint, and never calls `OnPaint`.

**H3 ruled out.** The two systems are not fighting — `do_message_loop_work()` is
doing work that CFRunLoop sources simply don't cover. CEF does not post its core
rendering tasks as CFRunLoop sources. The internal task queue that
`do_message_loop_work()` drains is the only path to rendering.

This also reframes the Exp 3/4 comparison: the reason `pump_app_events` reduces
`do_message_loop_work()` spike rate in the cef-rs example isn't because it
handles the _same_ work — it's because it handles _complementary_ work
(NSApplication events, display link, Core Animation) that reduces the total
workload CEF needs to process internally. Our CFRunLoop call doesn't process
that complementary work, so `do_message_loop_work()` has to do everything.

### Experiment 7: NSApplication Event Pump

**Status:** Not started

**Goal:** Replace `CFRunLoopRunInMode` with an NSApplication event pump — the
same mechanism that winit's `pump_app_events` uses internally — to process the
complementary work that reduces `do_message_loop_work()` overhead.

**Hypothesis tested:** Direct consequence of Exp 3, 4, 5, 6 findings.

#### Motivation

Six experiments have converged on a single conclusion:

1. `do_message_loop_work()` is essential and cannot be removed (Exp 6)
2. `do_message_loop_work()` takes >1ms on 100% of calls in our process but only
   5.7% in the cef-rs example (Exp 3 vs Exp 4)
3. The difference is `pump_app_events` — it handles complementary work
   (NSApplication events, display link, Core Animation) that our
   `CFRunLoopRunInMode` doesn't touch (Exp 4, 5, 6 conclusions)
4. `CFRunLoopRunInMode` returns instantly 96% of the time regardless of timeout
   (Exp 3, 5) — it's not processing the right kind of work

The cef-rs example achieves low mlw spike rates because winit's
`pump_app_events` internally calls:

```objc
[NSApp nextEventMatchingMask:NSEventMaskAny
                   untilDate:[NSDate distantPast]
                      inMode:NSDefaultRunLoopMode
                     dequeue:YES];
[NSApp sendEvent:event];
```

This processes the macOS application event queue: window server messages,
display link callbacks, Core Animation layer commits, and other system events
that CEF's internal threads post. Without processing these, the work accumulates
and `do_message_loop_work()` has to handle it all.

#### Changes

Two changes to `ts3/termsurf-profile/src/main.rs`:

**1. Add an `nsapp` module alongside the existing `cfrunloop` module:**

```rust
#[cfg(target_os = "macos")]
mod nsapp {
    use std::ffi::c_void;

    type Id = *mut c_void;

    #[link(name = "AppKit", kind = "framework")]
    extern "C" {
        // NSApp global
        static NSApp: Id;
    }

    extern "C" {
        // NSDate
        fn objc_msgSend(receiver: Id, sel: *const c_void, ...) -> Id;
        fn sel_registerName(name: *const u8) -> *const c_void;
    }

    /// Drain pending NSApplication events without blocking.
    pub fn pump_events() {
        unsafe {
            let distant_past: Id = objc_msgSend(
                objc_msgSend(
                    class(b"NSDate\0"),
                    sel_registerName(b"distantPast\0" as *const u8),
                ),
                sel_registerName(b"self\0" as *const u8), // identity, just to get the value
            );

            // NSEventMaskAny = NSUIntegerMax
            let mask: u64 = u64::MAX;
            let mode = kCFRunLoopDefaultMode; // NSDefaultRunLoopMode == kCFRunLoopDefaultMode

            loop {
                let event: Id = objc_msgSend(
                    NSApp,
                    sel_registerName(
                        b"nextEventMatchingMask:untilDate:inMode:dequeue:\0" as *const u8,
                    ),
                    mask,
                    distant_past,
                    mode,
                    true as i8, // YES = dequeue
                );
                if event.is_null() {
                    break;
                }
                objc_msgSend(
                    NSApp,
                    sel_registerName(b"sendEvent:\0" as *const u8),
                    event,
                );
            }
        }
    }

    unsafe fn class(name: &[u8]) -> Id {
        extern "C" {
            fn objc_getClass(name: *const u8) -> Id;
        }
        objc_getClass(name.as_ptr())
    }

    extern "C" {
        static kCFRunLoopDefaultMode: *const c_void;
    }
}
```

This uses raw FFI to call the same Objective-C methods that winit calls
internally. No new crate dependencies — just `extern "C"` bindings to AppKit,
the Objective-C runtime, and CoreFoundation.

**2. Replace `cfrunloop::run_for(0.001)` with `nsapp::pump_events()` in the
loop:**

```rust
while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
    cef::do_message_loop_work();
    #[cfg(target_os = "macos")]
    nsapp::pump_events();
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_millis(1));
}
```

The Exp 3 instrumentation is kept to compare mlw spike rates directly.

#### What Stays the Same

- `do_message_loop_work()` still called every iteration
- CEF settings unchanged
- No new crate dependencies
- XPC, IOSurface, render handler all unchanged
- `[FRAME-TX]` frame logging still active

#### Expected Outcomes

| Result                                                   | Meaning                                                                                                                                                                                                            |
| -------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| mlw spike rate drops from 100% toward 5.7%, fps improves | NSApplication events were the missing complementary work. The event pump offloads work from CEF's internal queue, matching the cef-rs example's behavior.                                                          |
| mlw spike rate unchanged, fps unchanged                  | NSApplication events aren't relevant in a headless (no window) process. The cef-rs example is fast because it _has a window_, not because of the event pump itself. Need to investigate window-dependent pathways. |
| Crash or hang                                            | NSApp may not be initialized in our process since CEF doesn't create one for headless/windowless mode. Would need to call `[NSApplication sharedApplication]` first.                                               |

#### Risk

Medium. The raw `objc_msgSend` FFI is unsafe and calling conventions must be
exact. However, the pattern is well-established (winit, cocoa crate, and the
cef-rs cefsimple example all do the same thing). If NSApp is null because CEF
never initialized it, the first `objc_msgSend` will crash — but we'll see that
immediately in the log and can add `[NSApplication sharedApplication]`
initialization. The `cfrunloop` module is kept intact for easy revert.

#### Results

**Status: FAILED — webview did not open. Same behavior as Exp 6.**

Replacing `CFRunLoopRunInMode` with the NSApplication event pump broke the app
completely. The webview never rendered. This matches the "Crash or hang"
expected outcome — though rather than crashing, the app simply never produced
frames.

#### Conclusion

The NSApplication event pump **replaced** CFRunLoop servicing rather than
**complementing** it. The problem is that `nsapp::pump_events()` drains the
NSApplication event queue but does not service CFRunLoop sources — and Exp 6
already proved that CFRunLoop servicing is necessary for
`do_message_loop_work()` to produce frames.

The failure reveals a misconception: we assumed `pump_app_events` was a superset
of `CFRunLoopRunInMode` (processing NSApplication events _and_ CFRunLoop
sources). In reality, they may process different layers of the macOS event
system. Our NSApplication pump only calls
`nextEventMatchingMask:untilDate:inMode:dequeue:` + `sendEvent:`, which handles
the NSApplication event queue. But `CFRunLoopRunInMode` services CFRunLoop
sources (timers, ports, observers) that CEF depends on for frame scheduling.

**The fix for a future experiment:** call **both** — `nsapp::pump_events()` for
NSApplication events _and_ `cfrunloop::run_for()` for CFRunLoop sources. The
cef-rs example's `pump_app_events` likely does both internally via winit's macOS
backend, which runs a full `NSRunLoop` iteration that encompasses both layers.

### Experiment 8: NSApplication Event Pump + CFRunLoop Together

**Status:** Not started

**Goal:** Call both `nsapp::pump_events()` and `cfrunloop::run_for()` in each
loop iteration, combining the NSApplication event processing that the cef-rs
example benefits from with the CFRunLoop servicing that CEF requires.

**Hypothesis tested:** Same as Exp 7, corrected for the two-layer insight.

#### Motivation

Experiments 6 and 7 each removed one half of the equation and both failed:

- Exp 6: Removed `do_message_loop_work()`, kept CFRunLoop → no frames
- Exp 7: Replaced CFRunLoop with NSApplication pump → no frames

Both failures confirm that CFRunLoop servicing is essential — CEF's internal
timers (SyntheticBeginFrameSource) are CFRunLoop sources, not NSApplication
events. But the Exp 4 comparison showed that `pump_app_events` processes
_complementary_ work that reduces `do_message_loop_work()` overhead from 100%
spike rate to 5.7%.

The hypothesis: `pump_app_events` works because it does **both** — it runs a
full NSRunLoop iteration that services CFRunLoop sources _and_ processes
NSApplication events. Our previous experiments tried one or the other. This
experiment tries both together.

#### Changes

Two changes to `ts3/termsurf-profile/src/main.rs`:

**1. Add the `nsapp` module from Exp 7** (same raw FFI code).

**2. Call both in the loop — NSApplication pump first, then CFRunLoop:**

```rust
while !QUIT_FLAG.load(std::sync::atomic::Ordering::Relaxed) {
    cef::do_message_loop_work();
    #[cfg(target_os = "macos")]
    {
        nsapp::pump_events();
        cfrunloop::run_for(0.001);
    }
    #[cfg(not(target_os = "macos"))]
    std::thread::sleep(std::time::Duration::from_millis(1));
}
```

NSApplication pump runs first to drain any pending system events, then CFRunLoop
services CEF's timer sources. The Exp 3 instrumentation is kept to compare mlw
spike rates.

#### What Stays the Same

- `do_message_loop_work()` still called every iteration
- CFRunLoop timeout unchanged at 1ms
- CEF settings unchanged
- No new crate dependencies
- `[FRAME-TX]` frame logging still active

#### Expected Outcomes

| Result                             | Meaning                                                                                                                       |
| ---------------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| mlw spike rate drops, fps improves | NSApplication events are the complementary work. Adding them alongside CFRunLoop matches what `pump_app_events` does.         |
| No change from baseline            | NSApplication events don't exist in a headless process. The cef-rs example is fast because of its window, not the event type. |
| Regression or broken               | The two pumps interfere with each other, or the NSApplication pump disrupts CEF's internal state.                             |

#### Risk

Low. This is additive — we keep the proven baseline (`do_message_loop_work()` +
`cfrunloop::run_for(0.001)`) and add `nsapp::pump_events()` before it. If the
NSApplication pump has no events to process (likely in a headless process), it
returns immediately and the behavior is identical to baseline. Worst case is the
same performance as before.

#### Results

**Status: FAILED — webview did not open. Same total failure as Exp 7.**

Adding `nsapp::pump_events()` alongside `cfrunloop::run_for()` broke the app
just as completely as replacing it did in Exp 7. The webview never rendered.

#### Conclusion

The NSApplication event pump via raw `objc_msgSend` FFI is fundamentally
incompatible with CEF's process in its current form. Three possible
explanations:

1. **NSApp is null.** CEF in windowless/off-screen mode may never call
   `[NSApplication sharedApplication]`, so the `NSApp` global is nil. Calling
   `objc_msgSend` on nil returns nil (no crash), but the pump loop may interact
   badly with CEF's internal state or thread expectations.

2. **The `objc_msgSend` variadic FFI is incorrect.** The calling convention for
   `nextEventMatchingMask:untilDate:inMode:dequeue:` involves mixed parameter
   types (u64, pointer, pointer, int) through a C variadic function. If the ABI
   is wrong, it could corrupt the stack or CEF's state silently.

3. **CEF's internal NSApplication subclass conflicts.** CEF may install its own
   NSApplication delegate or subclass (like `CefAppProtocol` seen in the
   cefsimple example). Pumping events outside CEF's control may bypass its event
   routing and break its internal assumptions.

**The NSApplication event pump approach is abandoned.** Experiments 7 and 8 both
produced total failures. The raw FFI approach to NSApplication event pumping is
either incorrect or fundamentally incompatible with CEF's headless process
model. A different direction is needed.

### Experiment 9: Question the Assumption — cef-rs OSR Without a Window

**Status:** Not tried — changing direction

**Goal:** Determine whether the cef-rs OSR example's 60fps performance comes
from `pump_app_events` or from having a window with a Metal layer (which creates
a CVDisplayLink under the hood).

**Hypothesis tested:** Is the window the key, not the event pump?

#### Motivation

Experiments 6, 7, and 8 all failed when touching the event pump. Every attempt
to replicate what `pump_app_events` does via raw FFI broke the app completely.
This suggests we may be chasing the wrong cause.

The cef-rs OSR example has three things our profile server doesn't:

1. `NSApplicationActivationPolicyRegular` — makes it a full GUI process
2. A winit window with a wgpu/Metal surface — creates a CVDisplayLink
3. `pump_app_events` processing the winit event loop

We assumed #3 was the cause of 60fps (Exp 4 conclusion). But what if it's #2? A
Metal layer implicitly creates a CVDisplayLink, which provides a hardware 60Hz
vsync signal. This signal drives Core Animation and may also drive CEF's
`SyntheticBeginFrameSource` timer scheduling. Without a display link, macOS may
coalesce timers and deliver them at a degraded cadence — explaining why
CFRunLoop returns instantly 96% of the time (no sources pending because the
display link never fires).

This experiment tests this directly: remove the window from the cef-rs OSR
example and see if it still gets 60fps.

#### Changes

One change to `cef-rs/examples/osr/src/main.rs`:

**Skip window creation in `create_browser_window`.** Replace the window + wgpu
setup with a minimal headless browser that still calls `on_accelerated_paint`
but doesn't render to a window. The simplest approach: skip
`create_browser_window` entirely and create the browser directly with a minimal
render handler, similar to how the profile server does it.

However, since the example's architecture is deeply coupled to the window (wgpu
State, BrowserInstance, window events), the cleaner approach is to **comment out
the window creation and wgpu setup** while keeping:

- The winit event loop and `pump_app_events` (the control variable)
- CEF initialization with the same settings
- Browser creation with `windowless_rendering_enabled: true`
- The `on_accelerated_paint` callback (just log, don't render)

Specifically in `create_browser_window`:

```rust
fn create_browser_window(&mut self, event_loop: &ActiveEventLoop, url: &str, ...) {
    // Skip window and wgpu State creation
    // Create browser with no window, minimal render handler
    let window_info = WindowInfo {
        windowless_rendering_enabled: true as _,
        shared_texture_enabled: true as _,
        external_begin_frame_enabled: false as _,
        ..Default::default()
    };
    let browser_settings = BrowserSettings {
        windowless_frame_rate: 60,
        ..Default::default()
    };
    // ... create browser with a logging-only render handler
}
```

Add frame counting with timestamps in the render handler to measure delivery
rate, similar to the profile server's `[FRAME-TX]` logging.

#### What Stays the Same

- winit event loop created and `pump_app_events` called (the thing we're
  testing)
- `NSApplicationActivationPolicyRegular` still set
- `do_message_loop_work()` still called every iteration
- `external_message_pump: true` in CEF settings
- Same CEF initialization

#### Expected Outcomes

| Result                                      | Meaning                                                                                                                                                                                               |
| ------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| fps drops to ~38 (matches profile server)   | The **window/display link** is the key, not `pump_app_events`. Our headless process can never match 60fps without a display link. Next step: install a CVDisplayLink or create a hidden CAMetalLayer. |
| fps stays at ~60                            | `pump_app_events` really is the key. Our raw FFI attempts (Exp 7, 8) failed due to implementation bugs, not because the approach is wrong. Next step: use winit properly or fix the FFI.              |
| fps drops to an intermediate value (~45-55) | Both the window and `pump_app_events` contribute. The event pump helps but the display link provides additional timing precision.                                                                     |

#### Risk

Low. This only modifies the cef-rs example, not the profile server. The example
is a test harness — no production impact. If the modification is too invasive,
we can create a separate minimal example instead.

## Conclusion (After 8 Experiments)

### What We Learned

**The core bottleneck is clear:** `do_message_loop_work()` takes >1ms on 100% of
calls in the profile server (Exp 3), versus only 5.7% in the cef-rs example (Exp
4). This single function consuming 1-33ms per call is why we can't sustain
60fps.

**Why it's slow:** The cef-rs example has `pump_app_events` (winit) which
processes macOS system events — window server messages, display link callbacks,
Core Animation commits — as complementary work. This offloads tasks from CEF's
internal queue, so `do_message_loop_work()` finds an almost-empty queue and
returns in microseconds. Our profile server has no equivalent — all the work
piles up in CEF's internal queue and `do_message_loop_work()` processes
everything.

**What we can't do about it:** Three experiments (6, 7, 8) tried to replicate
`pump_app_events` via raw FFI. All three produced total failures — the webview
stopped rendering entirely. The NSApplication event pump approach via
`objc_msgSend` is either fundamentally incompatible with CEF's headless process
model, or our FFI implementation was wrong.

### Hypotheses Resolved

| Hypothesis                             | Status        | Evidence                                                         |
| -------------------------------------- | ------------- | ---------------------------------------------------------------- |
| H1: Polling loop timing mismatch       | **Confirmed** | Exp 3: mlw dominates loop at 1-33ms per call                     |
| H2: CFRunLoop 1ms timeout too short    | **Ruled out** | Exp 1, 5: longer timeouts and draining both made things worse    |
| H3: mlw and CFRunLoop fighting         | **Ruled out** | Exp 6: removing mlw produced zero frames — they're not redundant |
| H9: `return_after_source_handled` flag | **Ruled out** | Exp 1: draining all sources hurt performance                     |
| H10: Process QoS too low               | **Ruled out** | Exp 2: USER_INTERACTIVE QoS caused regressions                   |
| H4-H8: remaining                       | **Untested**  | —                                                                |

### What Made Things Better

Nothing. Every intervention either regressed or broke the app:

| Experiment               | Change                     | Result                 |
| ------------------------ | -------------------------- | ---------------------- |
| Exp 1: Drain CFRunLoop   | Loop until timed-out       | -20% fps, -85% streak  |
| Exp 2: QoS               | USER_INTERACTIVE           | -24% fps, -86% streak  |
| Exp 3: Instrument loop   | Diagnostic                 | Baseline (no change)   |
| Exp 4: Instrument cef-rs | Diagnostic                 | Baseline (no change)   |
| Exp 5: 16ms timeout      | Increase CFRunLoop timeout | -18% fps, -94% streak  |
| Exp 6: Remove mlw        | CFRunLoop only             | **Broken** — no frames |
| Exp 7: NSApp pump        | Replace CFRunLoop          | **Broken** — no frames |
| Exp 8: NSApp + CFRunLoop | Add NSApp alongside        | **Broken** — no frames |

The baseline from Issue 342 Exp 5 (38.2fps, 71% at 60fps, max streak 424)
remains the best result achieved. No experiment in this issue improved on it.

### What's Left to Try

The untested hypotheses and ideas that remain viable:

1. **H4: `external_message_pump` with deadlock workaround** — Issue 342 Exp 4
   failed due to a chicken-and-egg deadlock during init. A two-phase approach
   (poll during init, switch to cooperative scheduling after
   `on_context_initialized`) might solve this. The cef-rs example uses
   `external_message_pump: true` and achieves 60fps.

2. **H6: CVDisplayLink + CFRunLoop** — Issue 341 Exp 8 tried CVDisplayLink alone
   (failed), but that was before the CFRunLoop fix. Combining hardware vsync
   timing with CFRunLoop servicing is untested.

3. **H8: Mach port caching** — Eliminating redundant kernel syscalls on every
   frame. Likely marginal but could reduce per-frame overhead.

4. **H5/H7: Diagnostic experiments** — Correlating SyntheticBeginFrameSource
   timing with frame drops (H5), or measuring GUI-side frame pacing (H7).

5. **The inverse test** — Instead of removing the window from the cef-rs example
   (Exp 9, abandoned as impractical), replace `pump_app_events` in the cef-rs
   example with `CFRunLoopRunInMode`. If fps drops to ~38, it confirms
   `pump_app_events` is the key. If fps stays at ~60, the window is what
   matters.

6. **Use winit in the profile server** — Instead of raw FFI, add winit as a
   dependency and use `pump_app_events` directly. This would replicate the
   cef-rs example's architecture exactly, but adds a significant dependency to a
   headless process.

### Status

**Issue 343 is unresolved.** The goal of perfect 60fps has not been achieved.
The baseline remains 38.2fps / 71% at 60fps. The root cause is understood
(inadequate event pump in a headless process) but no solution has been found
within the constraints (no hidden windows, no focus stealing, no winit
dependency).
