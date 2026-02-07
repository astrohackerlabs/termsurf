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
