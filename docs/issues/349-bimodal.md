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

**Results:**

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

**Analysis:**

- Bimodal pattern confirmed: 1/7 trials entered bad mode.
- Bad mode (trial 1): p50=16.7ms (perfect single vsync) but p95=83.3ms (5
  vsync intervals). Most frames on time, but occasional cascading delays — fits
  the FIFO queue hypothesis.
- Good mode (trials 2-7): tightly clustered at 49.9–51.3fps, p50=18.1–18.8ms,
  p95=33.2–33.6ms. Extremely stable once entered.
- The p95 values quantize cleanly to vsync multiples: 33ms (2×), 83ms (5×).
- This provides a clean baseline for testing the PresentMode fix in Experiment 3.

**Status:** Done

### Experiment 3: Change WezTerm present mode to AutoVsync

**Goal:** Test whether changing WezTerm's wgpu present mode from `Fifo` to
`AutoVsync` eliminates the bimodal pattern.

**Hypothesis:** `PresentMode::Fifo` creates a strict FIFO queue where one late
frame desynchronizes all subsequent frames. `AutoVsync` (likely Mailbox on
macOS) absorbs late frames without cascading. Switching should eliminate the
bimodal pattern and stabilize ts3 at ~50fps, matching cef-test.

**What needs to change:**

One line in `wezterm-gui/src/termwindow/webgpu.rs`:

```rust
// Before:
present_mode: wgpu::PresentMode::Fifo,

// After:
present_mode: wgpu::PresentMode::AutoVsync,
```

**How to test:**

1. Make the change
2. `cd ts3 && ./scripts/build-release.sh --open`
3. Run `web benchmark` (multi-trial mode from Experiment 2)
4. Check for bimodal pattern: are all trials in good mode, or still split?

**What the results tell us:**

- If results are stable (~50fps, no bimodal): Fifo was the cause. Ship with
  AutoVsync.
- If bimodal persists: the present mode isn't the cause. Investigate L2
  (terminal rendering contention) next.
- If fps changes but bimodal persists: the present mode affects performance
  but doesn't explain the bistable behavior.

**Status:** Not started
