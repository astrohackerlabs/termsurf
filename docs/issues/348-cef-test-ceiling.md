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
