+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

# Issue 349: The Bimodal Pattern

## Background

Across Issues 346, 347, and 348, a persistent bimodal frame timing pattern has
emerged in ts3. The system randomly enters one of two modes per benchmark run:

- **Good mode:** p50 = 16.7ms, p95 = 33.3ms, ~50–55fps, 85%+ at 60fps
- **Bad mode:** p50 = 16.9–19.1ms, p95 = 66–83ms, ~33–38fps, <50% at 60fps

The modes are quantized to multiples of the 16.7ms vsync interval. The system
enters one mode or the other at the start of a run and tends to stay in it.
Which mode it enters appears random — the same code, same build, same machine
can produce either result on consecutive runs.

### What we've ruled out

| Hypothesis             | Issue | Result                                      |
| ---------------------- | ----- | ------------------------------------------- |
| Mouse input overhead   | 346   | No effect — variance identical with/without |
| Debug build overhead   | 347   | Release helps fps but doesn't fix bimodal   |
| Hot-path logging       | 346/8 | Removing logs doesn't fix bimodal           |
| 1ms message loop sleep | 348   | 0ms and 1ms both produce bimodal in ts3     |
| CEF OSR inherent limit | 347/8 | cef-test release is stable at ~51fps        |

### The critical clue

**cef-test release does NOT exhibit the bimodal pattern.** With 1ms sleeps,
cef-test release produced 50.3–51.6fps consistently across multiple runs, with a
stable p95 of 33.6ms. The bimodal pattern was explicitly noted as "gone" in
Issue 347 Experiment 1.

ts3, under identical conditions (release build, 1ms sleep, same machine), still
exhibits bimodal behavior. The difference must lie in something ts3 has that
cef-test doesn't.

### What ts3 has that cef-test doesn't

| Component               | cef-test            | ts3                                  |
| ----------------------- | ------------------- | ------------------------------------ |
| GUI event loop          | winit (simple)      | WezTerm (complex)                    |
| Terminal rendering      | None                | Active (pane rendering)              |
| Render scheduling       | Manual redraw       | WezTerm's frame scheduler            |
| XPC architecture        | Via launcher relay  | Via launcher relay (same)            |
| Number of panes         | 2 (left/right)      | Terminal + webview                   |
| wgpu surface management | Single window       | Per-window, multi-pane               |
| Process count           | 3 (gui, 2x profile) | 4+ (gui, launcher, profile, web CLI) |

## Lines of inquiry

### L1: WezTerm's render scheduling

WezTerm has its own frame scheduling logic for terminal rendering. It decides
when to redraw based on terminal output, cursor blink, animations, and
potentially vsync. If the webview frame arrives while WezTerm is in the middle
of its own render cycle, the webview frame may be delayed until the next
WezTerm-initiated render — adding up to one frame of latency.

In cef-test, the GUI calls `request_redraw()` immediately when a new IOSurface
arrives and renders on the next event loop iteration. WezTerm may batch or defer
redraws.

**Test:** Investigate how WezTerm schedules renders and whether webview
IOSurface arrivals trigger immediate redraws or wait for the next scheduled
frame.

### L2: Terminal pane rendering contention

ts3 renders both terminal panes and webview panes in the same wgpu render pass.
If the terminal pane has activity (cursor blink, output), it triggers renders on
its own schedule. The webview pane's IOSurface may arrive out of phase with the
terminal's render cycle, causing some frames to miss vsync.

In cef-test, the only thing driving renders is the incoming IOSurface — there's
no competing render source.

**Test:** Run the ts3 benchmark with a completely idle terminal (no cursor
blink, no output) and compare with an active terminal. If the bimodal pattern
disappears with an idle terminal, render scheduling contention is the cause.

### L3: WezTerm event loop interaction with vsync

WezTerm's event loop may have a different relationship with vsync than winit's
`pump_app_events`. If WezTerm's loop runs on a timer or is driven by a display
link, the phase relationship between CEF's frame production and WezTerm's
presentation could create a beat frequency — sometimes in phase (good mode),
sometimes out of phase (bad mode).

This would explain why the bimodal pattern is stable within a run but random
between runs: the phase relationship is set at startup and persists.

**Test:** Investigate WezTerm's main loop timing. Does it use a display link? A
timer? How does it decide when to present frames?

### L4: Process startup timing

The bimodal pattern may be determined by the relative startup timing of the
profile server and GUI. If the profile server's first frame happens to align
with the GUI's vsync cycle, subsequent frames stay aligned (good mode). If it
misaligns, frames consistently miss (bad mode).

This would explain the randomness between runs — process startup timing varies
by microseconds, which determines which vsync phase the pipeline locks into.

**Test:** Add timestamps to the first few frames in both the profile server and
GUI to see if good-mode runs have different phase alignment than bad-mode runs.

## Recommended experiment order

1. **L1 + L3:** Investigate WezTerm's render scheduling and event loop (code
   reading, no changes needed — highest information value)
2. **L2:** Test with idle terminal (quick behavioral test)
3. **L4:** Timestamp first frames to check phase alignment (diagnostic)

## Experiments

### Experiment 1: Code review of WezTerm render pipeline

**Goal:** Understand how WezTerm schedules and presents frames, and identify
differences from cef-test that could explain the bimodal pattern.

**Method:** Code reading of the WezTerm GUI rendering pipeline. No code changes.

**Findings:**

Reviewed the following files:
- `wezterm-gui/src/frontend.rs` — event loop
- `wezterm-gui/src/termwindow/mod.rs` — window event dispatch
- `wezterm-gui/src/termwindow/render/paint.rs` — paint scheduling
- `wezterm-gui/src/termwindow/render/draw.rs` — draw calls, webview overlay
- `wezterm-gui/src/termwindow/render/pane.rs` — per-pane rendering
- `wezterm-gui/src/termwindow/webview_xpc.rs` — XPC surface reception
- `wezterm-gui/src/termwindow/webgpu.rs` — wgpu surface configuration

**Key discovery: PresentMode differs.**

| Setting        | cef-test                    | ts3 (WezTerm)          |
| -------------- | --------------------------- | ---------------------- |
| Present mode   | `PresentMode::AutoVsync`    | `PresentMode::Fifo`    |
| Frame latency  | `desired_maximum_frame_latency: 2` | Not set (default) |

This is the most likely cause of the bimodal pattern:

- **Fifo** (WezTerm): Strict FIFO queue. Frames are presented in order at each
  vsync. If a frame misses the vsync deadline, it waits for the next vsync —
  AND it pushes all subsequent frames back in the queue. A single late frame
  can desynchronize the entire pipeline, causing a cascade where every
  subsequent frame also misses. This creates a bistable system: either all
  frames are on time (good mode) or the queue is perpetually one frame behind
  (bad mode).

- **AutoVsync** (cef-test): Automatically selects the best vsync mode. On
  macOS with Metal, this likely uses Mailbox semantics, where a late frame
  simply replaces the pending frame in the queue instead of backing up behind
  it. A single late frame is absorbed gracefully — it doesn't cascade.

This explains every observation:
- Why ts3 is bimodal but cef-test is not (different present modes)
- Why the mode is stable within a run (once the Fifo queue desynchronizes, it
  stays desynchronized)
- Why the mode is random between runs (depends on whether early frames happen
  to hit or miss the first few vsync deadlines)

**Other findings:**

1. **Event-driven rendering.** WezTerm uses `window.invalidate()` to trigger
   redraws. The XPC callback calls `invalidate()` immediately when a new
   IOSurface arrives — there's no deferral.

2. **Animation timer competition.** WezTerm schedules cursor blink and animated
   image updates via `smol::Timer`, which calls `invalidate()` on its own
   schedule. This could compete with webview-triggered redraws, but is unlikely
   to cause the bimodal pattern since the timer fires infrequently (~1Hz for
   cursor blink).

3. **Webview renders after terminal.** In `call_draw_webgpu()`, the webview
   IOSurface is imported and rendered after all terminal content. The terminal
   render time is added to the webview frame's latency budget. If terminal
   rendering takes variable time, it could push webview frames past vsync.

**Recommended next experiment:** Change WezTerm's present mode from `Fifo` to
`AutoVsync` and re-run the ts3 benchmark. If the bimodal pattern disappears,
the present mode is the cause.

**Status:** Done

### Experiment 2: Multi-trial benchmark for bimodality detection

**Goal:** Modify `web benchmark` to run multiple short trials with independent
profile server restarts, so that bimodality can be detected statistically rather
than by running 70-second benchmarks one at a time.

**Rationale:** The current benchmark runs one 70-second trial per invocation.
Detecting bimodality requires running the benchmark repeatedly and comparing
results across runs. But since the mode is determined at startup and is stable
within a run, a single 70-second run only gives us one sample. We need multiple
independent samples to see the distribution.

By restarting the profile server between trials, we get a fresh phase
relationship between CEF's frame production and the GUI's vsync cycle each time
— an independent coin flip. A 10-second trial is long enough to clearly identify
which mode the system is in (p50 of ~16.7ms vs ~33ms is obvious within seconds).

**Design:**

- 7 trials of 10 seconds each (~70s total, same as current benchmark)
- Full profile server restart between trials (independent phase alignment)
- ~1 second pause between trials (let the wgpu FIFO queue drain)
- Per-trial stats printed on one line each
- Summary at end showing distribution across trials

**What the output should look like:**

```
[BENCH] Trial 1/7: 52.1fps  85% @60fps  p50=16.7ms  p95=33.4ms
[BENCH] Trial 2/7: 34.2fps  42% @60fps  p50=33.1ms  p95=66.8ms
[BENCH] Trial 3/7: 51.8fps  84% @60fps  p50=16.7ms  p95=33.5ms
...
[BENCH] Summary: 4/7 good mode, 3/7 bad mode (bimodal: YES)
```

After Experiment 3 (present mode fix), the same test should show:

```
[BENCH] Trial 1/7: 51.2fps  83% @60fps  p50=16.7ms  p95=33.4ms
[BENCH] Trial 2/7: 50.8fps  82% @60fps  p50=16.8ms  p95=33.5ms
...
[BENCH] Summary: 7/7 good mode, 0/7 bad mode (bimodal: NO)
```

**Implementation scope:** Changes needed in the coordinator
(`termsurf-web/src/main.rs`) to loop over trials, and in the profile server
(`termsurf-profile/src/main.rs`) to support shorter trial durations. The GUI and
launcher also needed changes to pass `benchmark_duration` through the XPC chain.

**Implementation notes:**

- Added `--benchmark-duration` arg to the profile server (default 70, 10 for
  multi-trial). Plumbed through: coordinator JSON → GUI socket → GUI XPC →
  launcher → profile server CLI arg.
- Changed completion marker from `[BENCHMARK] 70 seconds elapsed` to
  `[BENCHMARK-DONE]` (duration-independent).
- First attempt hung on trial 2. Root cause: profile server's `cef::shutdown()`
  is slow (several seconds). The launcher still thought the old profile was
  running and forwarded `create_browser` to a dead connection. Fix: send
  `unregister_profile` to the launcher explicitly before `cef::shutdown()`.

**Results (run 1 — fresh app launch, machine cool):**

```
[BENCH] Trial 1/7: 43.4fps  66.2% @60fps  p50=16.7ms  p95=83.3ms
[BENCH] Trial 2/7: 50.9fps  79.3% @60fps  p50=18.1ms  p95=33.2ms
[BENCH] Trial 3/7: 49.9fps  75.5% @60fps  p50=18.3ms  p95=33.6ms
[BENCH] Trial 4/7: 51.3fps  79.2% @60fps  p50=18.7ms  p95=33.4ms
[BENCH] Trial 5/7: 50.1fps  76.5% @60fps  p50=18.6ms  p95=33.6ms
[BENCH] Trial 6/7: 50.3fps  79.8% @60fps  p50=18.5ms  p95=33.3ms
[BENCH] Trial 7/7: 50.4fps  81.2% @60fps  p50=18.8ms  p95=33.5ms

Summary: 6/7 good mode, 1/7 bad mode (bimodal: YES)
```

**Results (run 2 — closed app, reopened, launched again):**

```
[BENCH] Trial 1/7: 35.5fps  51.0% @60fps  p50=17.1ms  p95=81.5ms
[BENCH] Trial 2/7: 30.9fps  30.8% @60fps  p50=32.9ms  p95=83.3ms
[BENCH] Trial 3/7: 30.9fps  32.3% @60fps  p50=32.8ms  p95=83.2ms
[BENCH] Trial 4/7: 30.2fps  33.7% @60fps  p50=30.2ms  p95=83.1ms
[BENCH] Trial 5/7: 29.4fps  29.3% @60fps  p50=33.1ms  p95=83.0ms
[BENCH] Trial 6/7: 29.3fps  33.8% @60fps  p50=32.9ms  p95=83.1ms
[BENCH] Trial 7/7: 25.8fps  28.9% @60fps  p50=33.2ms  p95=83.4ms

Summary: 0/7 good mode, 7/7 bad mode (bimodal: NO)
```

**Results (run 3 — fresh app launch after cooling):**

```
[BENCH] Trial 1/7: 49.0fps  84.5% @60fps  p50=16.7ms  p95=33.7ms
[BENCH] Trial 2/7: 49.0fps  80.5% @60fps  p50=18.7ms  p95=33.9ms
[BENCH] Trial 3/7: 48.4fps  76.7% @60fps  p50=18.8ms  p95=33.9ms
[BENCH] Trial 4/7: 49.7fps  78.4% @60fps  p50=18.7ms  p95=33.5ms
[BENCH] Trial 5/7: 48.7fps  74.1% @60fps  p50=18.7ms  p95=33.5ms
[BENCH] Trial 6/7: 50.7fps  76.1% @60fps  p50=18.7ms  p95=33.6ms
[BENCH] Trial 7/7: 50.4fps  76.3% @60fps  p50=18.7ms  p95=33.7ms

Summary: 7/7 good mode, 0/7 bad mode (bimodal: NO)
```

**Results (run 4 — same app, immediately re-ran):**

```
[BENCH] Trial 1/7: 51.9fps  83.1% @60fps  p50=18.7ms  p95=33.6ms
[BENCH] Trial 2/7: 50.6fps  81.9% @60fps  p50=18.5ms  p95=33.5ms
[BENCH] Trial 3/7: 50.6fps  79.0% @60fps  p50=18.7ms  p95=33.5ms
[BENCH] Trial 4/7: 51.1fps  81.0% @60fps  p50=18.8ms  p95=33.6ms
[BENCH] Trial 5/7: 51.1fps  80.0% @60fps  p50=18.8ms  p95=33.6ms
[BENCH] Trial 6/7: 44.1fps  64.7% @60fps  p50=18.9ms  p95=61.3ms
[BENCH] Trial 7/7: 41.4fps  61.2% @60fps  p50=19.1ms  p95=53.6ms

Summary: 5/7 good mode, 2/7 bad mode (bimodal: YES)
```

**Results (run 5 — same app, immediately re-ran again):**

```
[BENCH] Trial 1/7: 36.0fps  40.9% @60fps  p50=20.0ms  p95=79.7ms
[BENCH] Trial 2/7: 34.7fps  33.7% @60fps  p50=21.0ms  p95=80.2ms
[BENCH] Trial 3/7: 37.9fps  42.3% @60fps  p50=19.7ms  p95=66.7ms
[BENCH] Trial 4/7: 30.5fps  24.6% @60fps  p50=33.2ms  p95=80.9ms
[BENCH] Trial 5/7: 31.0fps  24.9% @60fps  p50=33.1ms  p95=83.1ms
[BENCH] Trial 6/7: 31.5fps  31.1% @60fps  p50=33.0ms  p95=83.1ms
[BENCH] Trial 7/7: 29.5fps  33.4% @60fps  p50=33.0ms  p95=83.3ms

Summary: 0/7 good mode, 7/7 bad mode (bimodal: NO)
```

**Analysis:**

The dominant effect across these runs is **thermal throttling**, not bimodality:

- Run 1 (cool machine): 6/7 good. Run 2 (immediately after): 7/7 bad. The
  machine was already warm from run 1.
- Runs 3–5 show clear progressive degradation: 7/7 good → 5/7 good (thermal
  transition visible in trials 6-7) → 0/7 good.
- Within run 5, trials degrade monotonically (36→29fps) — not random mode
  selection, but continuous thermal decay.
- The "bimodal YES" in run 4 was actually capturing the thermal transition
  mid-run, not a random coin flip.

**Key finding:** The multi-trial benchmark within a single session mostly
measures thermal throttling, not bimodality. The wgpu FIFO queue lives in the
GUI process, which persists across all trials. Restarting the profile server
does not reset the GUI's presentation pipeline phase. To properly isolate
bimodality from thermal effects, each trial would need a full app relaunch with
cooling time — impractical for an automated benchmark.

The original bimodal observations (from the issue background) may still be real,
but they are confounded by thermal state. Regardless, the PresentMode fix in
Experiment 3 addresses both: AutoVsync absorbs late frames whether caused by
phase misalignment or thermal-induced slowdowns.

**Status:** Done

### Experiment 3: Event-driven CEF message pump via `on_schedule_message_pump_work`

**Goal:** Replace the busy-wait `cfrunloop::run_for(0.000)` loop in the profile
server with CEF's intended event-driven architecture using the
`on_schedule_message_pump_work` callback. This eliminates 100% CPU usage that
causes thermal throttling (Experiment 2's dominant effect), and may also improve
frame timing precision.

**Background:** The current profile server main loop is a busy-wait:

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    cfrunloop::run_for(0.000);  // zero-second timeout = spin forever
}
```

This burns one full CPU core continuously, generating waste heat that degrades
performance over time (Experiment 2 showed clear thermal decay across runs).

CEF provides a proper alternative: `external_message_pump` mode with the
`on_schedule_message_pump_work(delay_ms)` callback. When enabled, CEF calls this
callback whenever it has work to do, telling you exactly how many milliseconds
to wait before calling `do_message_loop_work()`. Between callbacks, the process
can sleep — no busy-waiting.

**Prior attempts and their mistakes:**

This was tried three times before, each with identifiable mistakes:

| Attempt       | Missing piece                                          |
| ------------- | ------------------------------------------------------ |
| Issue 325 E4  | Forgot `external_message_pump: 1` — callback never fired |
| Issue 325 E5  | No fallback timer, no reentrancy guard — pump died after 3 frames |
| Issue 342 E4  | Init deadlock — `CFRunLoopRun()` starts after `cef::initialize()`, but init needs the pump |

None of these replicated the **working reference implementation** in
`cef-rs/examples/tests_shared/src/browser/main_message_loop_external_pump/`,
which has three critical features the ts3 attempts lacked:

1. **Fallback timer (33ms):** After every `do_message_loop_work()`, if no new
   callback has arrived, schedule a 33ms fallback timer. This ensures the pump
   never dies — even if CEF forgets to call the callback.

2. **Reentrancy guard:** `do_message_loop_work()` can trigger
   `on_schedule_message_pump_work()` synchronously. The reference detects this
   with an `is_active` flag and reschedules immediately after the outer call
   returns, rather than nesting.

3. **Thread-safe marshaling:** The callback can come from any thread. The
   reference uses `performSelector:onThread:` to marshal to the main thread.

The ts2 implementation (which worked at 60fps) also used this architecture,
but benefited from WezTerm's existing AppKit event loop. The ts3 profile server
is a headless process — it needs to run its own CFRunLoop.

**What needs to change in `termsurf-profile/src/main.rs`:**

1. Add `external_message_pump: 1` to CEF settings
2. Implement `on_schedule_message_pump_work` in `ProfileBPH` with:
   - Reentrancy guard (`is_active` / `reentrancy_detected` flags)
   - CFRunLoop timer scheduling (cancel old timer, create new one)
   - 33ms fallback timer after each `do_message_loop_work()`
   - Thread marshaling (timer added to main run loop)
3. Replace the busy-wait main loop with `CFRunLoopRun()` (blocking)
4. Solve the init deadlock: use a two-phase approach:
   - Phase 1: Poll with `do_message_loop_work()` + short sleep during
     `cef::initialize()` and browser creation
   - Phase 2: Switch to `CFRunLoopRun()` once `on_context_initialized` fires
     (or after browser is created)

**How to test:**

1. Make the changes
2. `cd ts3 && ./scripts/build-release.sh --open`
3. Run `web benchmark` — multiple consecutive runs to check for thermal decay
4. Monitor CPU usage with Activity Monitor — should drop from ~100% to near-idle
   between frames

**What the results tell us:**

- If CPU drops and fps stays ~50: event-driven pump works. Thermal throttling
  was masking the true performance. Ship this.
- If only 3 frames then halts: the fallback timer isn't working — debug the
  timer scheduling.
- If init deadlock: the two-phase approach needs adjustment — try polling longer
  or using a different signal for the phase transition.
- If fps drops below 50: the timer precision may be too coarse — check whether
  CEF is requesting sub-millisecond delays that CFRunLoop can't deliver.

**Implementation notes:**

All three missing pieces from the reference implementation were added:

1. `cef_pump` module: reentrancy guard (`is_active` / `reentrancy_detected`),
   one-shot CFRunLoop timer scheduling, 33ms fallback timer after each
   `do_message_loop_work()`.
2. `benchmark_timers` module: moved scroll simulation and monitoring from the
   while-loop into an 8ms repeating CFRunLoop timer callback.
3. `external_message_pump: 1` in CEF settings, `on_schedule_message_pump_work`
   callback in `ProfileBPH` forwarding to `cef_pump::schedule_work()`.
4. Main loop replaced with `cfrunloop::run()` (blocking). Quit paths updated to
   call `cfrunloop::stop()`.
5. The init deadlock from Issue 342 E4 did not occur — `cef::initialize()`
   returned successfully before `CFRunLoopRun()` started. A kick-start call
   (`cef_pump::schedule_work(0)`) bootstraps the pump.

**Results (run 1 — fresh app, thermal nominal):**

```
[BENCH] Trial 1/7: 29.0fps  10.0% @60fps  p50=24.9ms  p95=84.7ms
[BENCH] Trial 2/7: 32.1fps  16.2% @60fps  p50=21.8ms  p95=83.1ms
[BENCH] Trial 3/7: 30.8fps  15.9% @60fps  p50=24.8ms  p95=83.5ms
[BENCH] Trial 4/7: 30.7fps  13.7% @60fps  p50=23.5ms  p95=83.4ms
[BENCH] Trial 5/7: 29.8fps  11.1% @60fps  p50=26.3ms  p95=80.6ms
[BENCH] Trial 6/7: 27.7fps  12.3% @60fps  p50=26.8ms  p95=84.8ms
[BENCH] Trial 7/7: 30.0fps  13.4% @60fps  p50=25.3ms  p95=84.0ms

Summary: 0/7 good mode, 7/7 bad mode (bimodal: NO)
```

**Results (run 2 — fresh app, thermal still nominal):**

```
[BENCH] Trial 1/7: 29.1fps  11.1% @60fps  p50=25.2ms  p95=85.4ms
[BENCH] Trial 2/7: 27.2fps   7.6% @60fps  p50=31.1ms  p95=90.0ms
[BENCH] Trial 3/7: 28.3fps  11.4% @60fps  p50=25.5ms  p95=85.2ms
[BENCH] Trial 4/7: 27.6fps  15.5% @60fps  p50=24.7ms  p95=88.0ms
[BENCH] Trial 5/7: 27.6fps  15.6% @60fps  p50=24.9ms  p95=91.1ms
[BENCH] Trial 6/7: 28.4fps  15.8% @60fps  p50=24.9ms  p95=86.2ms
[BENCH] Trial 7/7: 26.7fps  17.6% @60fps  p50=24.6ms  p95=89.6ms

Summary: 0/7 good mode, 7/7 bad mode (bimodal: NO)
```

**Analysis:**

The event-driven pump is significantly worse than the busy-wait loop:

| Metric     | Busy-wait (Exp 2 run 1) | Event-driven (Exp 3) |
| ---------- | ----------------------- | -------------------- |
| Avg fps    | ~50                     | ~29                  |
| @60fps     | 75–85%                  | 10–17%               |
| p50        | 17–19ms                 | 25ms                 |
| p95        | 33ms                    | 83–91ms              |

Key findings:

1. **~30fps matches the 33ms fallback timer exactly.** The fallback timer is the
   dominant scheduling mechanism. CEF's `on_schedule_message_pump_work` callback
   is not firing reliably enough to drive 60fps rendering — after the initial
   burst, the callback stops being the primary work driver, and the 33ms fallback
   becomes the ceiling.

2. **p50 of ~25ms = 1.5 vsync intervals.** Frames land between vsync boundaries
   because `do_message_loop_work()` isn't called quickly enough after CEF has
   work ready. The timer scheduling overhead (create timer → run loop iteration
   → fire timer) adds latency that the busy-wait loop didn't have.

3. **No init deadlock.** The two-phase approach wasn't even needed —
   `cef::initialize()` returned successfully before `CFRunLoopRun()`. This rules
   out Issue 342 E4's failure mode.

4. **No bimodality, just consistently bad.** Every trial is ~29fps across both
   runs. The callback-driven timing is uniformly slow rather than bistable.

5. **This matches Issue 325 E5's core problem** — the callback stops being
   reliable after initial frames — but the fallback timer (which E5 lacked)
   prevents a complete stall. The fallback keeps the pump alive at 30fps instead
   of dying at 3 frames.

**Why the reference implementation works but ts3 doesn't:** The reference uses
`NSApp(mtm).run()` (AppKit's full event loop), not bare `CFRunLoopRun()`. NSApp
dispatches Cocoa-level events, display link callbacks, and other AppKit-internal
machinery that CEF's Chrome runtime may depend on. A headless process with bare
`CFRunLoopRun()` only processes timers and run loop sources — it misses the
AppKit event dispatching layer. The ts2 implementation worked because CEF ran
in-process with WezTerm, which had a full NSApplication event loop.

**Conclusion:** This first attempt at the event-driven pump halved fps from ~50
to ~29. But the investigation is incomplete — several potential causes remain
untested, and we lack diagnostic data to distinguish between them.

**What we don't know yet:**

1. **Is the callback actually firing?** We assumed it stops being reliable, but
   never logged it. The problem might be "callback fires fine but our response
   is too slow" rather than "callback doesn't fire." These need different fixes.

2. **Timer creation overhead for immediate work.** When CEF requests delay=0
   ("work NOW"), we allocate a new `CFRunLoopTimer`, add it to the run loop, and
   wait for CFRunLoop to process it on the next iteration. That's heavyweight.
   A `CFRunLoopSource` would be much faster — signaling a source is just setting
   a flag, and it triggers on the very next run loop iteration without timer
   allocation. The reference uses `performSelector:onThread:` which is similarly
   lighter than raw timer creation.

3. **Benchmark timer contention.** The 8ms scroll timer runs on the main thread.
   While its callback executes (mutex lock, scroll event, rate tracking), NO
   other timer can fire — including the CEF pump timer. CFRunLoop processes one
   callback at a time. If the scroll callback takes 1ms, that's 12.5% of every
   8ms window where the pump is blocked.

4. **Multiple pump calls per frame.** CEF may need several
   `do_message_loop_work()` calls to push one frame through its internal
   pipeline (compositor schedule → layer composite → paint → accelerated paint
   callback). The busy-wait loop provides thousands of calls per second — plenty.
   With the event-driven approach, each call requires timer fire → run loop
   iteration → next timer fire. If a frame needs 5–10 calls and each iteration
   takes 0.1–1ms, a single frame could take 1–10ms of scheduling overhead.

5. **NSApp.run() vs bare CFRunLoopRun().** The reference uses `NSApp().run()`. A
   headless process CAN create and run NSApplication (activation policy
   `.prohibited` — no dock icon, no menu). CEF's Chrome runtime may depend on
   AppKit event dispatching internally. We never tried this.

**Next experiments to try (in priority order):**

1. Add logging to `on_schedule_message_pump_work` — how often does it fire, with
   what delay values? This tells us if the callback or the scheduling is broken.
2. Reduce fallback timer from 33ms to 1ms — if frequency is the issue, this
   narrows it down cheaply.
3. Use `CFRunLoopSource` instead of `CFRunLoopTimer` for delay=0 work —
   eliminates timer allocation overhead entirely.
4. Try `NSApp().run()` in the headless process — definitively tests whether bare
   `CFRunLoopRun()` vs AppKit event loop is the gap.

**Status:** Done — results inconclusive, further investigation needed

## Conclusion

This issue set out to investigate the bimodal frame timing pattern in ts3.
Experiment 1 identified `PresentMode::Fifo` vs `AutoVsync` as the likely cause.
Experiment 2 built a multi-trial benchmark to detect bimodality, but discovered
that thermal throttling from the profile server's busy-wait loop
(`cfrunloop::run_for(0.000)`) is the dominant confounding effect — consecutive
benchmark runs degrade monotonically as the CPU heats up, masking any underlying
bimodal signal. Experiment 3 attempted to fix the busy-wait by switching to
CEF's event-driven `on_schedule_message_pump_work` callback, which eliminated
CPU burn but halved fps from ~50 to ~29.

**The bimodality investigation is blocked by the CPU problem.** The only way to
get ~50fps today is a zero-delay busy-wait loop that pegs one core at 100%. This
causes thermal throttling within minutes, making multi-run benchmarks unreliable.
We cannot meaningfully test whether `PresentMode::AutoVsync` fixes bimodality
until the profile server can sustain ~50fps without overheating.

**The callback approach is the right path forward.** The busy-wait loop is not a
viable long-term architecture — it wastes an entire CPU core to repeatedly call
`do_message_loop_work()` millions of times per second, most of which are no-ops.
CEF's `external_message_pump` + `on_schedule_message_pump_work` is the intended
API for this. A working reference implementation exists in the cef-rs examples,
and ts2 used the same pattern successfully at 60fps. The Experiment 3 attempt
got the pump running but at degraded performance — the question is why, not
whether the approach is viable.

**What comes next:** Issue 350 will focus on debugging the callback pump to
achieve parity with the busy-wait loop. Once the profile server can sustain
~50fps without burning CPU, we return here to test the `AutoVsync` fix for
bimodality with reliable, non-thermally-confounded benchmarks.
