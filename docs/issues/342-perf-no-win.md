# Issue 342: 60fps Without a Hidden Window

## Problem Statement

The profile server (`termsurf-profile`) renders webpages via CEF off-screen
rendering and sends IOSurface Mach ports to the GUI. Currently it achieves
~28.5fps with a simple `sleep(1ms)` + `do_message_loop_work()` polling loop.

Issue 341 ran 18 experiments trying to reach 60fps. The only approach that
worked was a hidden 1x1 window, which provided the macOS window server's vsync
signal to CEF's compositor. But the hidden window steals focus from the GUI, and
every attempt to prevent that either destroyed the vsync signal or broke CEF.
The hidden window approach is rejected.

**Goal:** Achieve smooth, consistent 60fps frame delivery from the profile
server to the GUI without creating any window — hidden or otherwise.

## Why We Know This Is Possible

1. **The cef-rs OSR example achieves 60fps.** It uses winit with a _visible_
   window, but CEF's off-screen renderer doesn't paint to that window — it
   renders to an IOSurface. The window merely provides the process environment
   that CEF expects.

2. **Chrome and Chromium achieve 60fps in headless mode.** Chromium's headless
   mode uses a `SyntheticBeginFrameSource` — a timer-based replacement for
   hardware vsync that ticks at the configured frame rate. No window is
   involved.

3. **OBS Browser Source achieves 60fps.** OBS embeds CEF for browser sources and
   renders at configurable frame rates using shared textures.

4. **CEF's architecture doesn't require a window.** CEF's
   `windowless_frame_rate` setting configures a `SyntheticBeginFrameSource` that
   fires `BeginFrame` signals at the specified interval. The compositor responds
   to these signals whether or not a window exists. Something in our process
   environment is preventing this from working at full speed.

## Current Baseline

From Issue 341, Experiment 18 (simple polling loop, no `external_message_pump`):

| Metric                    | Value |
| ------------------------- | ----- |
| Average FPS               | 28.5  |
| Frames at 60fps (14-20ms) | 40%   |
| Frames at 30fps (30-36ms) | 23%   |
| Dropped frames (>50ms)    | 18%   |
| Max consecutive 60fps     | 11    |

The frame timing is bimodal: some frames arrive at 16ms, others at 33ms, with
frequent ~80ms drops. CEF's internal `SyntheticBeginFrameSource` should be
ticking at 16.67ms intervals (60fps), but something is causing every other tick
to be missed or delayed.

## Key Difference: cef-rs Example vs Profile Server

| Aspect                  | cef-rs OSR example                                 | termsurf-profile                        |
| ----------------------- | -------------------------------------------------- | --------------------------------------- |
| Window                  | Visible winit window                               | None                                    |
| `external_message_pump` | `true`                                             | `false` (current baseline)              |
| `windowless_frame_rate` | `60`                                               | `60`                                    |
| Event loop              | winit `pump_app_events` + `do_message_loop_work()` | `sleep(1ms)` + `do_message_loop_work()` |
| NSApplication           | Initialized by winit                               | Never initialized                       |
| CFRunLoop               | Driven by winit event pumping                      | Not driven                              |
| Frame rate              | 60fps sustained                                    | 28fps bimodal                           |

The critical differences are:

1. **NSApplication is never initialized** in the profile server
2. **CFRunLoop is never pumped** in the profile server
3. **No `external_message_pump`** means CEF runs its own internal message loop
   scheduling, which may interact poorly with the tight sleep loop

## Ideas to Investigate

### Category A: Fix CEF's Message Loop Scheduling

These ideas address the possibility that CEF's internal scheduler isn't getting
the CPU time it needs because our polling loop is fighting it.

#### Idea 1: `OnScheduleMessagePumpWork` Callback (Cooperative Scheduling)

**Priority: HIGH**

The `external_message_pump` setting enables a callback
`BrowserProcessHandler::on_schedule_message_pump_work(delay_ms)`. Instead of
blindly polling every 1ms, CEF tells us _exactly_ when it needs work done:

- `delay_ms <= 0`: call `do_message_loop_work()` immediately
- `delay_ms > 0`: schedule the call after that delay

The cef-rs repo includes a complete reference implementation in
`examples/tests_shared/src/browser/main_message_loop_external_pump/mac.rs`. That
implementation uses `NSTimer` on a `CFRunLoop` to schedule work at precise
intervals. This is how CEF is _designed_ to be driven when you own the message
loop.

**Why it might work:** The blind `sleep(1ms)` + poll approach may call
`do_message_loop_work()` at the wrong times — too early (CEF has no work), too
late (CEF missed its compositor deadline), or out of phase with CEF's internal
timers. Cooperative scheduling ensures work happens exactly when CEF requests
it.

**Why it might fail:** The reference implementation uses `NSApp(mtm).run()`
which runs an `NSApplication` run loop. We'd need to adapt this to work without
`NSApplication` — perhaps using `CFRunLoop` directly.

#### Idea 2: CFRunLoop-Based Message Pump (Without NSApplication)

**Priority: HIGH**

The reference `external_message_pump` implementation on macOS calls
`NSApp(mtm).run()`, which runs the AppKit run loop. But `CFRunLoop` is the
underlying primitive — `NSApplication.run()` is just a wrapper around
`CFRunLoopRun()` with AppKit event handling.

We can create a `CFRunLoop`-based pump:

1. Enable `external_message_pump`
2. Implement `on_schedule_message_pump_work` to add `CFRunLoopTimer` entries
3. Run `CFRunLoopRun()` on the main thread
4. When a timer fires, call `do_message_loop_work()`

This gives CEF's compositor the run loop integration it expects without any
AppKit, any NSApplication, or any window.

**Why it might work:** CEF's internal timers (including the
`SyntheticBeginFrameSource`) may rely on `CFRunLoop` sources being properly
serviced. A tight `sleep` + poll loop doesn't run the `CFRunLoop`, so these
sources never fire.

#### Idea 3: `CefRunMessageLoop()` (Let CEF Own the Loop)

**Priority: MEDIUM**

The profile server is a dedicated CEF process. It doesn't need to own the main
thread for anything else — XPC messages arrive on background threads. Let CEF
run its own message loop with `cef::run_message_loop()`.

Issue 341, Experiment 6 tried this and got 18fps. But that experiment also had
`external_message_pump: 1` enabled, which is _incompatible_ with
`run_message_loop()`. The combination may have confused CEF's scheduler. This
deserves a clean test with the correct configuration: `external_message_pump`
disabled (or absent) and `run_message_loop()` as the sole loop driver.

**Why it might work:** CEF knows best how to schedule its own work.
`run_message_loop()` runs a proper `CFRunLoop`/`NSRunLoop` internally, which
services all the timer sources that the `sleep` + poll approach misses.

**Why it might fail:** Experiment 6 already tried this (though possibly
misconfigured). If CEF's own run loop still doesn't achieve 60fps without a
window, the problem is deeper than message loop scheduling.

### Category B: Provide Display Timing Without a Window

These ideas give the process a vsync-aligned timing signal without creating any
window.

#### Idea 4: CVDisplayLink from CGDirectDisplayID

**Priority: HIGH**

`CVDisplayLink` does NOT require a window. You can create one directly from a
display ID:

```c
CVDisplayLinkRef displayLink;
CVDisplayLinkCreateWithCGDisplay(CGMainDisplayID(), &displayLink);
CVDisplayLinkSetOutputCallback(displayLink, myCallback, NULL);
CVDisplayLinkStart(displayLink);
```

The callback fires on a high-priority background thread at vsync intervals
(~16.67ms for 60Hz). We can use this to time `do_message_loop_work()` calls.

Issue 341, Experiment 8 tried `CVDisplayLink` but got only 30% at 60fps.
However, that experiment used the display link _instead_ of winit's event loop,
without `external_message_pump` cooperative scheduling. The display link alone
doesn't help if CEF's internal timers aren't being serviced properly.

**New approach:** Combine `CVDisplayLink` + `external_message_pump` +
`on_schedule_message_pump_work` on a `CFRunLoop`. The display link provides
timing alignment while the cooperative scheduler ensures CEF's internal work
happens on schedule.

**Note:** `CVDisplayLink` is deprecated in macOS 15 but still functional.

#### Idea 5: CADisplayLink via NSScreen (macOS 14+)

**Priority: MEDIUM**

macOS 14 (Sonoma) introduced `NSScreen.displayLink(target:selector:)` — a
display link that does not require a window or view:

```objc
CADisplayLink *link = [[NSScreen mainScreen]
    displayLinkWithTarget:self selector:@selector(displayLinkFired:)];
[link addToRunLoop:[NSRunLoop currentRunLoop] forMode:NSRunLoopCommonModes];
```

This is Apple's official replacement for `CVDisplayLink`. It provides
vsync-aligned callbacks on any run loop thread, supports ProMotion displays, and
works without any window.

**Why it might work:** Same timing benefit as CVDisplayLink but with modern API
and proper run loop integration.

**Caveat:** Requires macOS 14+. TermSurf may need to support older versions. The
process needs access to `NSScreen`, which requires linking against AppKit (but
not creating any windows).

#### Idea 6: High-Resolution Timer (dispatch_source)

**Priority: MEDIUM**

If the problem is just timer precision, a `dispatch_source` timer on a
`QOS_CLASS_USER_INTERACTIVE` queue provides high-resolution callbacks:

```c
dispatch_source_t timer = dispatch_source_create(
    DISPATCH_SOURCE_TYPE_TIMER, 0, 0,
    dispatch_get_global_queue(QOS_CLASS_USER_INTERACTIVE, 0));
dispatch_source_set_timer(timer,
    DISPATCH_TIME_NOW, 16666666, 1000000); // 16.67ms, 1ms leeway
dispatch_resume(timer);
```

This is not vsync-aligned but provides consistent 60Hz timing. Combined with
`external_message_pump` and `on_schedule_message_pump_work`, it could provide
the pacing that the `sleep(1ms)` loop lacks.

**Why it might work:** The `sleep(1ms)` loop calls `do_message_loop_work()` up
to 1000 times per second, but at random phases relative to CEF's compositor
deadlines. A 60Hz timer would call it at exactly the right frequency.

**Why it might fail:** If the issue is CFRunLoop integration rather than timer
precision, this won't help.

### Category C: Architectural Changes

These ideas change how frames flow between the profile server and the GUI.

#### Idea 7: GUI-Driven Frame Requests

**Priority: HIGH**

Instead of the profile server pushing frames as fast as it can, have the GUI
pull frames at its own vsync rate:

1. GUI has windows and display links — it knows exactly when the next vsync is
2. GUI sends a "request frame" message to the profile server via XPC before each
   vsync
3. Profile server calls `do_message_loop_work()` in response
4. CEF renders and sends the IOSurface back

This decouples frame production from the profile server's timer quality. The GUI
always has perfect vsync timing because it has windows. The profile server
doesn't need to know anything about display timing.

**Why it might work:** The GUI already renders at 60fps for terminal content. If
it drives the frame requests, the timing is naturally perfect.

**Challenges:** Adds a round-trip XPC message per frame. XPC latency is
sub-millisecond for Mach port transfers, but the round trip (request → CEF
render → IOSurface send) may take longer than one frame period. Would need
pipelining — request frame N+1 while displaying frame N.

#### Idea 8: Double Buffering / Frame Interpolation

**Priority: LOW**

Decouple frame production from frame display entirely:

1. Profile server produces frames at whatever rate it can (28fps, 60fps, etc.)
2. GUI maintains a double buffer — the last two IOSurfaces received
3. GUI renders at 60fps regardless, displaying the most recent IOSurface

If CEF only produces 30fps, the GUI would display each frame twice (hold for
33ms). This eliminates visual stuttering from irregular frame delivery — the GUI
always has something to show at vsync.

**Why it might work:** Converts the frame rate problem into a latency problem,
which is perceptually better. A consistent 30fps with no stuttering looks
smoother than an irregular 40fps with judder.

**Why it's low priority:** This doesn't actually solve the 60fps problem — it
papers over it. But it's a useful fallback if nothing else achieves true 60fps.

### Category D: Investigate the Root Cause

These ideas focus on understanding _why_ CEF doesn't hit 60fps without a window,
rather than working around it.

#### Idea 9: Instrument CEF's SyntheticBeginFrameSource

**Priority: HIGH**

CEF's `SyntheticBeginFrameSource` is a timer that should fire at 16.67ms
intervals. If it's firing correctly but frames aren't being produced, the
problem is downstream. If it's not firing correctly, the problem is the timer
itself.

Add logging to determine:

- Is `SyntheticBeginFrameSource` actually ticking at 60Hz?
- When it ticks, does the compositor produce a frame?
- If not, why? Is `DisplayScheduler::ShouldDraw()` returning false?
- Is `root_frame_missing_` stuck on true?

This requires either building CEF from source with instrumentation or using
`CHROMIUM_LOG` environment variables to enable internal logging.

**Why this matters:** Every other idea is a guess about what's wrong. This would
tell us definitively.

#### Idea 10: Run Profile Server with CEF Debug Logging

**Priority: HIGH**

CEF supports `--enable-logging --v=1` (or higher verbosity) which outputs
Chromium's internal logging. This may reveal why the compositor drops frames or
what timer sources it's using.

Additionally, `--log-file=/tmp/cef-debug.log` directs output to a file for
analysis.

**Specific things to look for:**

- Messages about `BeginFrame` delivery timing
- Messages about compositor frame submission
- Any throttling or scheduling warnings
- Timer source creation and configuration

#### Idea 11: Compare Process Environments

**Priority: MEDIUM**

The cef-rs example and the profile server use the same CEF library but achieve
different frame rates. Beyond the obvious window difference, compare:

- What `CFRunLoopSource` entries exist in each process?
- What Mach port rights does each process hold?
- Does winit's initialization register any system-level callbacks that affect
  CEF's timer infrastructure?
- Does `NSApplication` initialization (even without a window) change the
  process's relationship with the window server?

Tools: `CFRunLoopGetCurrent()` introspection, `lsmp` (list Mach ports), `sample`
(sampling profiler to see where time is spent).

#### Idea 12: Initialize NSApplication Without Creating a Window

**Priority: MEDIUM**

`NSApplication` initialization may register the process with the window server
and enable timer sources that CEF relies on, even without creating any window:

```rust
let _: () = msg_send![class!(NSApplication), sharedApplication];
```

or via the cocoa crate:

```rust
let _ = NSApp(); // Initializes NSApplication singleton
```

This is different from creating a window. It just creates the `NSApplication`
object and establishes the process's connection to the window server. If CEF's
`SyntheticBeginFrameSource` relies on run loop sources that only exist after
`NSApplication` initialization, this single line could be the fix.

**Why it might work:** The difference between the cef-rs example (60fps) and the
profile server (28fps) is that the example has `NSApplication` initialized (via
winit). If CEF's timer infrastructure routes through AppKit's run loop
integration, initializing `NSApplication` without a window could unlock it.

**Issue 341 note:** Experiment 5 tried `NSApplicationActivationPolicyRegular`
(which implies NSApplication initialization) and got 19fps. But that was without
`external_message_pump`. The combination of NSApplication init +
`external_message_pump` + proper `CFRunLoop` pumping has never been tested.

### Category E: Alternative Frame Production

#### Idea 13: CEF `Invalidate()` at 60Hz

**Priority: LOW**

`CefBrowserHost::Invalidate(PET_VIEW)` forces CEF to repaint the entire view.
Called at 60Hz from a timer, this might force frame production.

**Why it's low priority:** This is a hack. It forces unnecessary repaints and
doesn't address the underlying timer/compositor issue. But it's simple to test.

#### Idea 14: Correct Use of `external_begin_frame_enabled`

**Priority: LOW**

Issue 341, Experiment 17 tried `send_external_begin_frame()` and got the worst
results of any experiment (14.8fps). The research suggests this API is buggy and
poorly documented, with known issues:

- No completion callback (can't know when rendering finishes)
- Overlapping calls crash (`pending_frame_callback_` assertion)
- `root_frame_missing_` flag can get stuck

However, projects like `cef-mixer` and `cef-spout` use it successfully on
Windows with D3D11 shared textures. It may work if used correctly:

1. Call `send_external_begin_frame()` at 60Hz
2. Wait for `on_accelerated_paint` callback before calling again
3. Use a state machine to avoid overlapping calls

**Why it's low priority:** The API is fragile and our Experiment 17 results were
catastrophic. Only revisit if all other approaches fail.

## Experiment Checklist

Ordered by likelihood of success and implementation effort:

- [ ] **1. CEF debug logging** (Idea 10) — Zero code changes, may reveal root
      cause
- [ ] **2. `run_message_loop()` clean test** (Idea 3) — Simplest code change,
      may have been misconfigured in Exp 6
- [ ] **3. NSApplication init without window** (Idea 12) — One line of code, may
      unlock CEF timers
- [ ] **4. CFRunLoop-based pump** (Idea 2) — Addresses the likely root cause
      (timer infrastructure)
- [ ] **5. OnScheduleMessagePumpWork** (Idea 1) — The "correct" way to drive CEF
      externally
- [ ] **6. CVDisplayLink from display ID** (Idea 4) — Proven timing source,
      needs proper integration
- [ ] **7. GUI-driven frame requests** (Idea 7) — Architectural change, but
      fundamentally sound
- [ ] **8. Instrument SyntheticBeginFrameSource** (Idea 9) — Deep investigation
      if above ideas fail
- [ ] **9. Compare process environments** (Idea 11) — Diagnostic, helps guide
      further experiments
- [ ] **10. CADisplayLink via NSScreen** (Idea 5) — Modern API, requires macOS
      14+
- [ ] **11. dispatch_source timer** (Idea 6) — Simple fallback, not
      vsync-aligned
- [ ] **12. Double buffering** (Idea 8) — Doesn't solve 60fps, papers over the
      problem
- [ ] **13. Invalidate() at 60Hz** (Idea 13) — Hack, unlikely to work
- [ ] **14. external_begin_frame correct usage** (Idea 14) — Fragile API, last
      resort

## Constraints

- **No hidden windows.** Not even 1x1 pixels. Not even invisible ones.
- **No focus stealing.** The GUI must retain keyboard focus at all times.
- **macOS first.** Linux and Windows support is future work. macOS-specific APIs
  (CVDisplayLink, CADisplayLink, CFRunLoop) are acceptable.
- **Profile server is a separate process.** We cannot move CEF into the GUI
  process due to the one-profile-per-process constraint.

## Related Issues

- [Issue 341: Performance investigation](./341-performance.md) — 18 experiments,
  hidden window approach rejected
- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  investigation
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Research
  that led to the performance hypothesis

## Experiments

### Experiment 1: CEF Debug Logging

**Status:** Not started

**Goal:** Enable Chromium's internal logging to see what CEF's compositor and
frame scheduler are doing. Understand _why_ frames are being dropped or delayed.

**Changes:** Two modifications to `ts3/termsurf-profile/src/main.rs`:

1. **Add `log_severity` and `log_file` to CEF Settings** (~line 222):

```rust
let settings = cef::Settings {
    windowless_rendering_enabled: 1,
    no_sandbox: 1,
    log_severity: cef::LogSeverity::VERBOSE,
    log_file: cef::CefString::from("/tmp/cef-debug.log"),
    root_cache_path: ...,
    ...
};
```

2. **Add `--enable-logging` and `--v=1` to command-line args** (~line 683):

```rust
fn on_before_command_line_processing(&self, ..., command_line: ...) {
    if let Some(command_line) = command_line {
        command_line.append_switch(Some(&"no-startup-window".into()));
        command_line.append_switch(Some(&"enable-logging".into()));
        command_line.append_switch_with_value(
            Some(&"v".into()),
            Some(&"1".into()),
        );
    }
}
```

**No functional code changes.** The rendering loop stays the same. This is pure
diagnostics.

**What to look for in `/tmp/cef-debug.log`:**

- `BeginFrame` / `SyntheticBeginFrameSource` — is the frame timer ticking at
  60Hz?
- `Compositor` / `Draw` / `SubmitCompositorFrame` — is the compositor producing
  frames?
- `DisplayScheduler` / `ShouldDraw` — is the scheduler blocking frames?
- Any throttling, timer, or scheduling warnings
- Messages about `CFRunLoop`, `NSRunLoop`, or timer sources

**Expected output:** A log file at `/tmp/cef-debug.log` plus the normal
`[FRAME-TX]` entries in `/tmp/termsurf-profile-default.log`. Cross-referencing
these will reveal what happens between CEF's internal frame scheduling and our
`on_accelerated_paint` callback.
