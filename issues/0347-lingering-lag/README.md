+++
status = "closed"
opened = "2026-02-07"
closed = "2026-02-07"
+++

# Issue 347: Lingering Lag

## Background

Issue 346 investigated a suspected mouse performance problem. After three
experiments, we concluded the mouse was not the cause — the real finding was
that TermSurf's rendering pipeline runs at 35–47fps with high variance, while
Chrome renders at a smooth 60fps on the same hardware.

### Benchmark results to date

| Source   | Condition | FPS       | p50    | p95    |
| -------- | --------- | --------- | ------ | ------ |
| ts3      | Best run  | 46.6      | 16.9ms | 49.7ms |
| ts3      | Worst run | 33.7      | 19.1ms | 83.3ms |
| cef-test | LEFT      | 37.8      | 16.9ms | 50.0ms |
| cef-test | RIGHT     | 39.5      | 16.8ms | 49.9ms |
| Chrome   | Native    | 60 (est.) | 16.7ms | 16.7ms |

All ts3 and cef-test benchmarks were run as **debug builds**.

### The bimodal pattern

Issue 346 discovered that frame intervals are bimodal, not continuous:

- **Good mode:** p50 = 16.8–16.9ms, p95 = 49.7–50.0ms
- **Bad mode:** p50 = 18.9–19.1ms, p95 = 83.0–83.3ms

These values are exact multiples of the 60Hz display refresh interval (16.7ms).
The system enters one mode or the other per run, seemingly at random. This
suggests frames either consistently hit the vsync deadline or consistently miss
it by a small margin.

### The rendering pipeline

```
CEF renders page (off-screen)
    │
    ▼
on_accelerated_paint callback (IOSurface handle)
    │
    ▼
IOSurfaceCreateMachPort (create Mach port from handle)
    │
    ▼
XPC send (Mach port to GUI process)
    │
    ▼
IOSurfaceLookupFromMachPort (GUI imports IOSurface)
    │
    ▼
wgpu texture from IOSurface
    │
    ▼
wgpu render to screen
```

Chrome skips this entire pipeline — it composites directly to its own window via
GPU. TermSurf adds: off-screen render → IOSurface → Mach port → IPC → texture
import → second composite. Each step adds latency.

## Lines of inquiry

### L1: Debug vs release build

All benchmarks to date have been run as debug builds (`target/debug/`). Debug
builds disable all compiler optimizations, include bounds checks, overflow
checks, and debug assertions. This affects every function call in the hot path:
CEF message loop processing, IOSurface handling, XPC serialization, wgpu
rendering.

A release build could recover significant performance. This is the cheapest test
and should be done first.

**Test:** Run `web benchmark` with a release build. Compare fps, p50, p95.

### L2: Message loop cadence

The profile server's message loop runs:

```rust
cef::do_message_loop_work();
cfrunloop::run_for(0.001);  // 1ms sleep
```

This means the loop iterates at ~1000Hz, but CEF only gets one
`do_message_loop_work()` call per iteration. If CEF internally needs multiple
loop iterations to advance its rendering pipeline, we may be throttling it.

Chrome uses its own tightly integrated message loop with no artificial sleep.
The 1ms `cfrunloop` sleep was added to service macOS event delivery, but it may
be too long or too short.

**Test:** Try different sleep durations (0.5ms, 0.1ms, 0ms) and measure the
effect on fps and CPU usage.

### L3: Frame pacing and vsync alignment

The bimodal pattern (p50 = 16.8ms vs 19.1ms) suggests the pipeline is on the
edge of the vsync deadline. Frames that arrive slightly late wait a full 16.7ms
for the next vsync — a cliff-edge effect.

The pipeline has multiple asynchronous steps (CEF render → IPC → GUI present).
If any step adds variable latency, frames oscillate between hitting and missing
vsync.

**Test:** Add timestamps at each stage of the pipeline (CEF paint callback, XPC
send, XPC receive, wgpu present) to identify where the latency accumulates.

### L4: IOSurface transfer cost per frame

Every frame creates a new Mach port from the IOSurface handle via
`IOSurfaceCreateMachPort`, sends it over XPC, and the GUI does
`IOSurfaceLookupFromMachPort` to import it. This is per-frame overhead.

Questions:

- Does CEF reuse the same IOSurface (updating contents in place), or allocate a
  new one per frame?
- If the IOSurface is reused, can we send the Mach port once and skip the
  per-frame transfer?
- What is the actual cost of `IOSurfaceCreateMachPort` +
  `IOSurfaceLookupFromMachPort` per call?

**Test:** Log the IOSurface handle value across frames. If it's the same handle,
we can optimize to send the Mach port once and just signal "new frame"
afterward.

### L5: cef-test as the OSR ceiling

cef-test is a minimal CEF off-screen rendering app with no TermSurf code. It
scored 37.8–39.5fps — in the same range as TermSurf. If cef-test cannot reach
60fps, the bottleneck is in CEF's off-screen rendering itself, not our pipeline.

This would mean the path to 60fps requires either:

- Switching from OSR to a windowed CEF mode (requires architectural change)
- Optimizing CEF's OSR pipeline (upstream contribution)
- Accepting that OSR has an inherent fps ceiling

**Test:** Run cef-test as a release build and measure whether it reaches 60fps.
If cef-test release hits 60fps but ts3 release doesn't, the bottleneck is in our
pipeline. If neither hits 60fps, the bottleneck is CEF OSR.

### L6: GUI-side presentation timing

We measure frame intervals in `on_accelerated_paint` on the profile server side.
We don't know how quickly the GUI actually presents frames after receiving them
over XPC. The GUI could be:

- Batching frames and presenting on its own schedule
- Blocking on wgpu texture import
- Missing vsync deadlines due to its own event loop

**Test:** Add frame timing on the GUI side (in `webview_xpc.rs`) to measure the
interval between receiving a Mach port and presenting the frame.

## Recommended experiment order

1. **L1 + L5:** Release build of both ts3 and cef-test (cheapest, highest
   information value)
2. **L4:** Check if IOSurface handle is reused (quick log check)
3. **L3 + L6:** Pipeline timestamp instrumentation (more involved)
4. **L2:** Message loop tuning (only if L1 doesn't explain the gap)

## Experiments

### Experiment 1: cef-test release build benchmark

**Goal:** Determine whether the ~38fps ceiling is a debug build artifact or an
inherent limit of CEF off-screen rendering. cef-test is the minimal reference
app with no TermSurf code — if it can't hit 60fps in release mode, CEF OSR is
the bottleneck.

**What needs to change:**

The build script (`cef-test-scripts/build.sh`) and benchmark script
(`cef-test-scripts/benchmark.sh`) are hardcoded for debug builds:

1. `build.sh` line 51: `cargo build` → needs `--release`
2. `build.sh` line 62: `target/debug/cef-test-gui` → `target/release/`
3. `build.sh` line 65: `target/debug/cef-test-profile` → `target/release/`
4. `build.sh` line 90: `target/debug/cef-test-launcher` → `target/release/`

**Implementation plan:**

Add a `--release` flag to both scripts:

1. In `build.sh`:
   - Parse `--release` flag alongside existing `--clean` and `--open`
   - Set `PROFILE=release` or `PROFILE=debug` based on flag
   - Use `cargo build --release` when flag is present
   - Use `target/$PROFILE/` for all binary paths

2. In `benchmark.sh`:
   - Parse `--release` flag
   - Pass it through to `build.sh`

**How to test:**

1. `cd ts3 && ./cef-test-scripts/benchmark.sh` — debug build (existing behavior)
2. `cd ts3 && ./cef-test-scripts/benchmark.sh --release` — release build

Run each 3 times and compare.

**What the results tell us:**

- If release cef-test hits ~60fps: debug overhead is the bottleneck. The path to
  60fps for ts3 is simply building in release mode.
- If release cef-test stays at ~38fps: CEF OSR has an inherent fps ceiling. The
  gap between us and Chrome is the cost of off-screen rendering + IPC, not our
  code.
- If release cef-test improves but doesn't reach 60fps (e.g., ~50fps): debug
  overhead accounts for some of the gap, but other factors (L2–L4) also
  contribute.

**Result:**

Release build dramatically improves cef-test performance:

| Build   | FPS        | p50    | p95    | p99    | 60fps% | Streak |
| ------- | ---------- | ------ | ------ | ------ | ------ | ------ |
| Debug   | 37.8–39.5  | 16.8ms | 50.0ms | 83.3ms | 51–59% | 59–67  |
| Release | 50.3–51.6  | 16.7ms | 33.6ms | 33.9ms | 81–85% | 69–109 |

**Findings:**

1. **Debug overhead accounts for ~12fps.** Release jumps from ~38 to ~51fps — a
   33% improvement from compiler optimizations alone.

2. **The bimodal pattern is gone.** Debug had random "good mode" (p95=50ms) vs
   "bad mode" (p95=83ms). Release is consistently p95=33.6ms. The system no
   longer randomly enters a bad state.

3. **The p95 is now exactly 2 vsync intervals (33.3ms).** The p50 is a perfect
   16.7ms (1/60s). About 85% of frames land on the first vsync, and the
   remaining ~15% miss by just enough to wait for the next one — costing exactly
   one extra frame interval.

4. **CEF OSR is NOT the ceiling.** The ~38fps was debug overhead, not an inherent
   OSR limitation. At ~51fps with 85% of frames at 60fps, there's a clear path
   to smoother rendering.

5. **The remaining gap to 60fps is small.** The ~15% of frames that slip past
   the vsync deadline are likely caused by IOSurface → Mach port → IPC transfer
   time occasionally pushing a frame just past the 16.7ms boundary. This is what
   L3/L4/L6 would investigate.

**Next step:** Test ts3 in release mode to confirm it sees the same improvement.

**Status:** Done

### Experiment 2: ts3 release build benchmark

**Goal:** Confirm that ts3 sees the same ~12fps improvement from release mode
that cef-test did. If ts3 release matches cef-test release (~51fps), the
TermSurf pipeline adds no overhead beyond CEF OSR itself.

**Implementation:** No code changes needed. ts3 already has `build-release.sh`.

**How to test:**

1. Build release: `cd ts3 && ./scripts/build-release.sh`
2. Run benchmark: `./target/release/termsurf-gui.app/Contents/MacOS/web benchmark`
3. Run 3 times with no mouse movement, record fps/p50/p95

**What the results tell us:**

- If ts3 release matches cef-test release (~51fps, p95=33.6ms): the TermSurf
  pipeline (XPC, launcher, profile server, GUI) adds no measurable overhead
  beyond what cef-test already incurs. The remaining gap to 60fps is in CEF OSR
  or the IOSurface transfer, shared by both.
- If ts3 release is significantly worse than cef-test release (e.g., ~45fps):
  the TermSurf pipeline adds overhead that cef-test avoids. Investigate L3/L4/L6
  to find where.
- If ts3 release reaches ~60fps: we're done. Ship release builds.

**Result:**

ts3 release improves over debug but falls short of cef-test release:

| Source   | Build   | FPS        | p50    | p95    | 60fps% | Streak |
| -------- | ------- | ---------- | ------ | ------ | ------ | ------ |
| cef-test | Release | 50.3–51.6  | 16.7ms | 33.6ms | 81–85% | 69–109 |
| ts3      | Release | 45.8–51.8  | 18.9ms | 34.1ms | 47–55% | 20–43  |
| ts3      | Debug   | 33.7–46.6  | 16.9ms | 49.7ms | —      | —      |

**Findings:**

1. **The TermSurf pipeline adds ~2ms per frame.** cef-test release has
   p50=16.7ms (exactly one vsync). ts3 release has p50=18.9–19.6ms —
   consistently 2–3ms above the vsync deadline. This extra latency is in the
   TermSurf-specific path: IOSurface → Mach port → XPC → GUI import → wgpu
   render.

2. **The 2ms cliff-edge effect is devastating.** That ~2ms pushes the majority
   of frames past the 16.7ms vsync boundary. cef-test lands 81–85% of frames on
   the first vsync; ts3 lands only 47–55%. A small absolute latency causes a
   large fps drop because of the quantized vsync deadline.

3. **Run-to-run variance persists in ts3.** cef-test release eliminated the
   bimodal pattern entirely (consistent p95=33.6ms across runs). ts3 release
   still shows it: Run 1 had p95=34.1ms, Run 2 had p95=52.8ms. Something in the
   TermSurf pipeline introduces variable latency that cef-test avoids.

4. **Release mode still helps ts3.** ts3 debug was 33.7–46.6fps; release is
   45.8–51.8fps. The floor is higher. But the improvement is less dramatic than
   for cef-test, because the pipeline overhead masks some of the compiler
   optimization gains.

**Next step:** The ~2ms pipeline overhead is now the primary target. L3
(pipeline timestamps), L4 (IOSurface handle reuse), and L6 (GUI-side
presentation timing) are the highest-value investigations.

**Status:** Done

## Conclusion

Two experiments isolated the layers of the performance problem:

| Source   | Build   | FPS       | p50    | p95    | 60fps% |
| -------- | ------- | --------- | ------ | ------ | ------ |
| Chrome   | —       | 60 (est.) | 16.7ms | 16.7ms | 100%   |
| cef-test | Release | 50.3–51.6 | 16.7ms | 33.6ms | 81–85% |
| ts3      | Release | 45.8–51.8 | 18.9ms | 34.1ms | 47–55% |
| ts3      | Debug   | 33.7–46.6 | 16.9ms | 49.7ms | —      |

The gap between Chrome (60fps) and TermSurf (~48fps) breaks down into three
distinct layers:

1. **Debug build overhead (~12fps).** Eliminated by compiling with `--release`.
   This was the single largest factor. No further action needed — just ship
   release builds.

2. **CEF OSR baseline gap (~9fps).** cef-test is a minimal app with no TermSurf
   code, yet it only reaches ~51fps in release mode. About 15% of frames miss
   the vsync deadline. This gap exists before any TermSurf pipeline code runs.
   The cause is unknown — candidates include the 1ms message loop sleep (L2),
   CEF's `windowless_frame_rate` setting, or inherent cost of off-screen
   rendering. This is the next area to investigate.

3. **TermSurf pipeline overhead (~2ms per frame).** ts3 release has
   p50=18.9ms vs cef-test's 16.7ms. The extra ~2ms comes from the IPC path:
   IOSurface → Mach port → XPC → GUI import → wgpu render. This pushes ts3's
   60fps hit rate from ~83% down to ~51%. However, this layer only matters after
   the cef-test baseline is understood — if cef-test can be brought to 60fps,
   the pipeline overhead may shrink or become irrelevant.

**Recommended next step:** Investigate why cef-test tops out at ~51fps before
diving into ts3 pipeline details. cef-test is a smaller codebase, changes are
cheaper to test, and any improvement directly benefits ts3. The most likely
candidates are message loop cadence (L2) and CEF's frame rate configuration.
