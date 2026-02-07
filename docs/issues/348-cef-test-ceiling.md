# Issue 348: cef-test Performance Ceiling

## Background

Issue 347 decomposed the gap between Chrome (60fps) and TermSurf (~48fps) into
three layers. The middle layer — a ~9fps gap in cef-test itself — is the focus
here. cef-test is a minimal CEF off-screen rendering harness with no TermSurf
code. If it can't hit 60fps, nothing built on top of it can either.

### Current cef-test release performance

| Metric | Value     |
| ------ | --------- |
| FPS    | 50.3–51.6 |
| p50    | 16.7ms    |
| p95    | 33.6ms    |
| 60fps% | 81–85%    |

The p50 of 16.7ms means ~85% of frames land on the first vsync — they're
perfect. The remaining ~15% miss by just enough to wait for the next vsync at
33.3ms. The question is: what causes those 15% to miss?

### cef-test architecture

cef-test has two processes:

**cef-test-profile** (CEF process):

```
cef::do_message_loop_work()       // process CEF events (non-blocking)
cfrunloop::run_for(0.001)         // CFRunLoop sleep, up to 1ms
→ simulated scroll input at 125Hz
→ CEF renders off-screen to IOSurface
→ on_accelerated_paint() callback:
    IOSurfaceCreateMachPort(handle)
    XPC send mach_port to GUI
```

**cef-test-gui** (window process):

```
event_loop.pump_app_events(Duration::from_millis(1))  // winit pump, up to 1ms
→ process_pending_surfaces():
    IOSurfaceLookupFromMachPort(mach_port)
    import_to_wgpu() → creates wgpu::Texture
    create sRGB TextureView + BindGroup
    request_redraw()
→ render():
    surface.get_current_texture()
    draw two quads (left + right)
    surface_texture.present()       // AutoVsync
```

### Key settings

| Setting                      | Value           | Location                         |
| ---------------------------- | --------------- | -------------------------------- |
| CEF `windowless_frame_rate`  | 60              | cef-test-profile/src/main.rs:593 |
| CEF `shared_texture_enabled` | true            | cef-test-profile/src/main.rs:588 |
| Profile message loop sleep   | 1ms (CFRunLoop) | cef-test-profile/src/main.rs:249 |
| GUI event pump timeout       | 1ms (winit)     | cef-test-gui/src/main.rs:859     |
| wgpu present mode            | AutoVsync       | cef-test-gui/src/main.rs:361     |
| wgpu max frame latency       | 2               | cef-test-gui/src/main.rs:360     |
| GUI event loop control flow  | Poll            | cef-test-gui/src/main.rs:836     |

## Lines of inquiry

### L1: Double 1ms sleep

There are two 1ms sleeps in the pipeline — one in the profile server
(`cfrunloop::run_for(0.001)`) and one in the GUI
(`pump_app_events(Duration::from_millis(1))`). In the worst case, a frame
rendered by CEF sits through up to 2ms of idle sleep before reaching the screen:
1ms waiting for the profile loop to wake and send the Mach port, then 1ms
waiting for the GUI loop to wake and import it.

At 60fps each frame has 16.7ms. If the render itself takes ~14ms and the two
sleeps add up to ~2ms, that's 16ms — right on the edge. Any jitter pushes the
frame past the deadline.

**Test:** Reduce or eliminate the sleep durations and measure the effect on fps
and CPU usage.

### L2: CFRunLoop sleep variance

`CFRunLoopRunInMode` with a 1ms timeout and `return_after_source_handled = 1`
should return in <=1ms. But CFRunLoop is a macOS system primitive — it may
overshoot, especially under load. If the sleep occasionally takes 2–3ms instead
of 1ms, that's enough to miss vsync on frames that were already close to the
deadline.

**Test:** Instrument the CFRunLoop sleep duration (already have `cfl_us` timing
in the loop) and check for outliers.

### L3: IOSurface import cost

Every frame does a full IOSurface round-trip:

1. Profile: `IOSurfaceCreateMachPort(handle)` — creates a Mach port
2. XPC send (mach_port as `set_mach_send`)
3. GUI: `copy_mach_send` to extract port from XPC message
4. GUI: `IOSurfaceLookupFromMachPort(port)` — imports the surface
5. GUI: `import_to_wgpu()` — creates a new wgpu::Texture from the IOSurface

Steps 1 and 4 involve kernel calls (Mach port creation and lookup). Step 5
creates a new GPU texture each frame. If any of these occasionally spike, that
frame misses vsync.

**Test:** Check whether CEF reuses the same IOSurface handle across frames. If
so, the Mach port could be sent once and reused, eliminating per-frame kernel
calls.

### L4: wgpu texture creation per frame

`import_to_wgpu()` creates a new `wgpu::Texture` from the IOSurface on every
frame. This involves Metal API calls to wrap the IOSurface as a Metal texture,
then wrap that as a wgpu texture. Creating GPU resources per frame is generally
expensive.

If the IOSurface is reused (same handle, contents updated in place), the wgpu
texture could also be reused — just re-render from the same texture after CEF
signals a new frame.

**Test:** Log the IOSurface handle value across frames. If stable, restructure
to create the texture once and reuse it.

### L5: AutoVsync presentation semantics

`PresentMode::AutoVsync` lets wgpu pick the best vsync mode. On macOS with
Metal, this likely maps to `CAMetalLayer`'s default presentation, which uses a
display link. The `desired_maximum_frame_latency: 2` allows up to 2 frames in
the presentation queue.

If the queue is full (2 frames already pending), `get_current_texture()` blocks
until a frame is presented. This could introduce variable latency if frames
arrive in bursts.

**Test:** Try `PresentMode::Mailbox` (if supported) or reduce
`desired_maximum_frame_latency` to 1, and measure the effect.

## Recommended experiment order

1. **L1:** Reduce/eliminate the 1ms sleeps (cheapest, most likely culprit)
2. **L3 + L4:** Check IOSurface handle reuse (quick log check, high optimization
   potential)
3. **L2:** Instrument CFRunLoop sleep variance (diagnostic)
4. **L5:** Try different wgpu presentation modes (easy toggle)

## Experiments

### Experiment 1: Eliminate the 1ms sleeps

**Goal:** Determine whether the two 1ms sleeps (profile CFRunLoop + GUI winit
pump) are responsible for the ~15% of frames that miss vsync.

**Hypothesis:** Each sleep adds up to 1ms of idle wait. Together they can add up
to 2ms per frame in the pipeline. At 60fps the entire frame budget is 16.7ms.
If CEF finishes rendering at, say, 15ms into the frame, the profile sleep adds
1ms (now 16ms), and the GUI sleep adds another 1ms (now 17ms) — past the
16.7ms vsync deadline. Eliminating the sleeps should push these marginal frames
back under the deadline.

**What needs to change:**

Two lines, one in each process:

1. **Profile server** — `cef-test-profile/src/main.rs:249`:
   ```rust
   // Before:
   cfrunloop::run_for(0.001);  // 1ms sleep
   // After:
   cfrunloop::run_for(0.0);    // no sleep, return immediately
   ```
   We keep the `cfrunloop::run_for` call (with 0 duration) rather than removing
   it entirely, because CEF on macOS needs the CFRunLoop to be serviced for
   internal event delivery. A zero-duration call still processes any pending
   CFRunLoop sources — it just doesn't wait for new ones.

2. **GUI** — `cef-test-gui/src/main.rs:859`:
   ```rust
   // Before:
   let status = event_loop.pump_app_events(Some(Duration::from_millis(1)), &mut app);
   // After:
   let status = event_loop.pump_app_events(Some(Duration::ZERO), &mut app);
   ```
   Same rationale: process any pending events immediately, don't wait.

**Risk:** Higher CPU usage. Without the 1ms sleeps, both processes will spin
their loops as fast as possible. This is acceptable for a benchmark — we're
measuring the ceiling, not optimizing power consumption. If fps improves, we
can later find a smarter sleep strategy (e.g., sleep only when no frame is
pending).

**How to test:**

1. Make the changes above
2. `cd ts3 && ./cef-test-scripts/benchmark.sh --release`
3. Run 3 times, record fps/p50/p95/60fps%
4. Compare against baseline (50.3–51.6fps, p50=16.7ms, 81–85% at 60fps)
5. Note CPU usage during the run (Activity Monitor or `top`)

**What the results tell us:**

- If fps reaches ~60 and 60fps% reaches ~95%+: the sleeps were the bottleneck.
  The path forward is a smarter sleep strategy that doesn't add latency on the
  hot path.
- If fps stays at ~51: the sleeps aren't the problem. Move to L3/L4 (IOSurface
  handle reuse).
- If fps improves partially (e.g., ~55fps): the sleeps contribute but aren't
  the only factor. Combine with other optimizations.

**Result:**

| Condition       | FPS  | 60fps% | Streak | p50    | p95    | p99    |
| --------------- | ---- | ------ | ------ | ------ | ------ | ------ |
| Baseline LEFT   | ~51  | 81–85% | 69–109 | 16.7ms | 33.6ms | 33.9ms |
| Baseline RIGHT  | ~51  | 81–85% | 69–109 | 16.7ms | 33.6ms | 33.9ms |
| No sleep LEFT   | 55.7 | 93.2%  | 196    | 16.7ms | 33.3ms | 33.8ms |
| No sleep RIGHT  | 53.2 | 87.9%  | 54     | 16.7ms | 33.6ms | 33.9ms |

**Findings:**

1. **The sleeps cost ~4–5fps.** Removing them pushes LEFT from ~51 to 55.7fps.
   The 60fps hit rate jumps from ~83% to 93%. The longest streak of consecutive
   60fps frames nearly doubled (196 vs 109).

2. **Still not 60fps.** 7–13% of frames still miss vsync. The sleeps explain
   about half the gap from ~51fps to 60fps, but something else accounts for the
   rest.

3. **p50 and p95 are unchanged.** Good frames are still 16.7ms, bad frames
   still 33.3ms. The sleeps didn't change the latency of individual frames —
   they just changed how many frames fell on each side of the vsync cliff.

4. **LEFT outperforms RIGHT.** 93.2% vs 87.9% at 60fps. Processing order may
   matter — LEFT gets imported first in `process_pending_surfaces()`.

**Next step:** Investigate L3/L4 — per-frame IOSurface Mach port creation and
wgpu texture import cost. The remaining ~5fps gap likely comes from per-frame
kernel calls and GPU resource creation.

**Status:** Done

### Experiment 2: Check IOSurface handle reuse

**Goal:** Determine whether CEF reuses the same IOSurface handle across frames
or allocates a new one each time. If the handle is stable, we can send the Mach
port once and skip the per-frame `IOSurfaceCreateMachPort` +
`IOSurfaceLookupFromMachPort` kernel calls — eliminating per-frame overhead that
may account for the remaining ~5fps gap.

**What needs to change:**

One log line in `cef-test-profile/src/main.rs`, inside `on_accelerated_paint`
(around line 442):

```rust
// After extracting the handle:
let handle = info.shared_texture_io_surface as *mut std::ffi::c_void;
println!("[HANDLE] frame={} handle={:?}", frame_id, handle);
```

This logs the raw pointer value of the IOSurface handle. If CEF reuses the same
IOSurface (updating its contents in place), the pointer will be the same across
all frames. If CEF allocates a new IOSurface per frame, the pointer will change.

**How to test:**

1. Add the log line
2. `cd ts3 && ./cef-test-scripts/benchmark.sh --release`
3. After the run, check: `grep '\[HANDLE\]' /tmp/cef-test-gui.log | head -20`
4. Count unique handle values: `grep '\[HANDLE\]' /tmp/cef-test-gui.log | sed 's/.*handle=//' | sort -u | wc -l`

**What the results tell us:**

- If 1 unique handle: CEF reuses the IOSurface. We can send the Mach port once
  at the first frame, then signal "new frame" with a lightweight XPC message
  (no Mach port) for subsequent frames. The GUI reuses the wgpu texture, just
  re-rendering from the same IOSurface. This eliminates per-frame kernel calls
  and GPU resource creation.
- If many unique handles: CEF allocates new IOSurfaces. The per-frame Mach port
  transfer is unavoidable. Look elsewhere for the remaining gap.
- If a small number (2–3): CEF double- or triple-buffers IOSurfaces. We can
  cache the Mach port per handle and only create a new one when the handle
  changes.

**Result:**

CEF allocates a new IOSurface for every frame:

| Side  | Frames | Unique handles |
| ----- | ------ | -------------- |
| LEFT  | 3,304  | 861            |
| RIGHT | ~2,800 | 850            |

Every frame gets a different IOSurface pointer. The handle count (~850) is less
than the frame count (~3,000), so CEF does recycle from a pool — but the
recycling pattern is unpredictable (not a simple double/triple buffer).

**Findings:**

1. **The "send Mach port once" optimization is not possible.** CEF doesn't paint
   into a stable buffer. The per-frame `IOSurfaceCreateMachPort` → XPC send →
   `IOSurfaceLookupFromMachPort` → `import_to_wgpu()` pipeline is unavoidable
   with the current CEF API.

2. **Hot-path logging confirmed devastating.** The single `println!` per frame
   dropped performance from 55.7fps to 23–48fps with massive variance —
   consistent with the Issue 346 finding.

3. **L3/L4 is a dead end.** Cannot eliminate per-frame IOSurface transfer cost.
   The remaining ~5fps gap (55.7 → 60) must come from elsewhere.

**Next step:** Investigate wgpu presentation mode (L5) or instrument the
per-frame IOSurface import to measure its actual cost.

**Status:** Done
