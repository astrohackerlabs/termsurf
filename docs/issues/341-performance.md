# TermSurf 3.0: Performance Investigation

## Goal

Achieve 60fps browser rendering in ts3 by fixing the XPC/profile server
implementation, without abandoning CEF or rewriting in C++.

## Background

Issue 340 (Research 3) discovered that the cef-rs OSR example achieves smooth
60fps rendering with the same CEF version and settings as ts3. This proves CEF
is not the bottleneck—something in ts3's implementation is.

| Metric                   | ts3    | cef-rs OSR example |
| ------------------------ | ------ | ------------------ |
| Frame rate               | ~20fps | ~60fps             |
| CEF version              | 143    | 143                |
| `shared_texture_enabled` | true   | true               |
| `windowless_frame_rate`  | 60     | 60                 |
| `on_accelerated_paint`   | Yes    | Yes                |

The difference is architectural: ts3 runs CEF in a separate profile server
process, while the cef-rs example runs CEF in the same process as the GUI.

## Hypothesis: Missing Event Loop Integration

The profile server lacks the event loop integration that CEF needs to produce
frames at full speed.

### cef-rs OSR example (fast)

```rust
let ret = loop {
    do_message_loop_work();
    let timeout = Some(Duration::from_millis(1));
    let status = event_loop.pump_app_events(timeout, &mut app);  // winit event loop
    // ...
};
```

The example integrates CEF with a **winit event loop** that:

- Has a visible window connected to the display
- Handles window focus, resize, and other events
- Receives lightweight `UserEvent::FrameReady` signals
- Pumps both winit and CEF message queues together

### ts3 profile server (slow)

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));  // just sleep
}
```

The profile server runs CEF in **isolation**:

- No window, no display connection
- No event loop—just a sleep loop
- No vsync or display link signals
- CEF may think nothing is visible

### Why This Could Cause Throttling

CEF's compositor is designed for normal browser windows. It may:

1. **Throttle when "invisible"** — No window means CEF thinks nothing needs
   rendering urgently
2. **Wait for vsync it never receives** — Windowless mode may expect external
   timing signals
3. **Batch frames conservatively** — Without display connection, CEF may render
   less frequently

### Supporting Evidence

- Issue 338 measured CEF frame production at ~20fps (not XPC delay—CEF itself
  was slow)
- `external_begin_frame_enabled` had partial effect (it affects timing)
- Same CEF version and settings produce different results based on environment

## Ideas for Experiments

### Idea 1: Add Event Loop to Profile Server

**Hypothesis:** Adding a minimal event loop (even headless) to the profile
server will increase CEF's frame production rate.

**Approach:**

1. Add winit as a dependency to termsurf-profile
2. Create a headless event loop (no visible window)
3. Integrate `do_message_loop_work()` with winit's `pump_app_events()`
4. Measure frame rate

**Success criteria:** Frame production increases from ~20fps toward 60fps.

### Idea 2: Test with Visible Window

**Hypothesis:** If the profile server creates a visible (but hidden/offscreen)
window, CEF's compositor will behave normally.

**Approach:**

1. Create a 1x1 pixel window in the profile server
2. Hide it or move it offscreen
3. Measure frame rate

**Success criteria:** Frame production matches the cef-rs OSR example (~60fps).

### Idea 3: Profile CEF's Internal Timing

**Hypothesis:** CEF has internal throttling that activates in windowless mode.

**Approach:**

1. Add timing instrumentation to `on_accelerated_paint`
2. Log intervals between CEF's internal render calls
3. Compare against cef-rs example timing
4. Identify where CEF decides to skip/delay frames

**Success criteria:** Identify the specific CEF behavior causing throttling.

## Experiments

### Experiment 1: Document the Process Architecture

**Goal:** Understand the architectural difference between cef-rs OSR (fast) and
ts3 (slow) to identify where the bottleneck occurs.

#### cef-rs OSR Example (2 processes)

```
┌─────────────────────────────────────────┐
│ App Process (Browser Process)           │
│ - CEF initialization                    │
│ - winit event loop                      │
│ - on_accelerated_paint receives texture │
│ - wgpu renders to window                │
└─────────────────────────────────────────┘
              ▲
              │ IOSurface (Chromium internal IPC)
              │
┌─────────────────────────────────────────┐
│ GPU Process (spawned by CEF)            │
│ - Compositor                            │
│ - Creates IOSurface                     │
└─────────────────────────────────────────┘
```

The GPU process is spawned automatically by CEF/Chromium. It handles all GPU
operations and creates IOSurfaces that are passed back to the browser process
via Chromium's internal IPC.

#### ts3 (3 processes)

```
┌─────────────────────────────────────────┐
│ GUI Process (WezTerm)                   │
│ - wgpu renders to window                │
│ - Receives IOSurface via Mach port      │
└─────────────────────────────────────────┘
              ▲
              │ IOSurface (ts3 XPC protocol)
              │
┌─────────────────────────────────────────┐
│ Profile Server (Browser Process)        │
│ - CEF initialization                    │
│ - on_accelerated_paint receives texture │
│ - Forwards IOSurface via Mach port      │
└─────────────────────────────────────────┘
              ▲
              │ IOSurface (Chromium internal IPC)
              │
┌─────────────────────────────────────────┐
│ GPU Process (spawned by CEF)            │
│ - Compositor                            │
│ - Creates IOSurface                     │
└─────────────────────────────────────────┘
```

ts3 adds an extra process (Profile Server) between the GPU and the GUI. This
creates an additional IPC hop for texture sharing.

#### IPC Hop Comparison

| Path                  | cef-rs                | ts3                   |
| --------------------- | --------------------- | --------------------- |
| GPU → Browser Process | Chromium internal IPC | Chromium internal IPC |
| Browser Process → GUI | Same process (direct) | ts3 XPC (Mach port)   |
| **Total hops**        | 1                     | 2                     |

#### Key Finding: The Extra Hop Is Not the Bottleneck

Issue 338's measurements showed that the Profile Server → GUI hop was fast
(frames arrived within milliseconds of production). The bottleneck was CEF
producing frames at ~20fps in the profile server, not the XPC transport.

This means the problem is not the extra IPC hop—it's something about the profile
server's environment that causes CEF to throttle frame production.

#### Conclusion

The architecture difference (2 vs 3 processes) is not directly causing the
slowdown. The profile server receives IOSurfaces from the GPU process at the
same rate as the cef-rs example would—but CEF isn't producing them at 60fps.

The root cause is likely in how the profile server runs CEF (no event loop, no
window, no display connection), not in the texture forwarding to the GUI.

### Experiment 2: Add Event Loop to Profile Server

**Status:** FAILED — Event loop integration did not improve frame rate

**Goal:** Test whether adding a winit event loop to the profile server (without
a visible window) improves CEF's frame production rate.

**Hypothesis:** CEF's compositor needs event loop integration to produce frames
at full speed. The profile server's simple sleep loop doesn't provide this.

#### Before (slow)

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}
```

#### After (implemented)

```rust
// Minimal ApplicationHandler that does nothing (no window needed)
struct MinimalApp;

impl ApplicationHandler for MinimalApp {
    fn resumed(&mut self, _event_loop: &ActiveEventLoop) {}
    fn window_event(&mut self, _event_loop: &ActiveEventLoop, _window_id: WindowId, _event: WindowEvent) {}
}

// Main loop with winit event pump
let mut event_loop = EventLoop::<()>::with_user_event().build().unwrap();
event_loop.set_control_flow(ControlFlow::Poll);
let mut app = MinimalApp;

loop {
    if QUIT_FLAG.load(Ordering::Relaxed) {
        break;
    }
    cef::do_message_loop_work();
    let timeout = Some(Duration::from_millis(1));
    let status = event_loop.pump_app_events(timeout, &mut app);

    if let PumpStatus::Exit(_) = status {
        break;
    }
}
```

#### Why This Respects the Architecture

- Profile server remains a separate process (required for multi-profile support)
- One CEF instance per profile server (hard constraint)
- GUI process unchanged
- XPC communication unchanged
- Only the message loop implementation changes

#### Implementation Steps

1. ✅ Add `winit` dependency to `termsurf-profile/Cargo.toml`
2. ✅ Create a minimal `ApplicationHandler` implementation (empty)
3. ✅ Replace the sleep loop with `pump_app_events`
4. ✅ Measure frame rate with existing instrumentation

#### Results

**Status: FAILED** — The winit event loop did NOT improve frame rate.

| Metric                     | Value       |
| -------------------------- | ----------- |
| **Average FPS**            | **17.0**    |
| Frames at ~30fps (21-40ms) | 443 (80%)   |
| Frames at ~15fps (41-70ms) | 35 (6%)     |
| Frames at ~10fps (71-110ms)| 21 (4%)     |
| Stalls (>110ms)            | 37 (7%)     |
| Fast frames (0-20ms)       | 17 (3%)     |

Frame interval distribution shows CEF is producing frames at roughly **30fps**
when active (most intervals are 33-34ms), but frequent stalls bring the overall
rate down to ~17fps.

#### Conclusion

The winit event loop hypothesis was **wrong**. CEF appears to have an internal
~30fps cap that isn't affected by event loop integration. The
`windowless_frame_rate: 60` setting isn't being honored.

This disproves the theory that CEF needed event loop integration to produce
frames at full speed. The profile server's simple sleep loop was not the
bottleneck.

### Experiment 3: Measure cef-rs Example Frame Rate

**Status:** CONFIRMED — cef-rs achieves 60fps, proving CEF can deliver high frame rates

**Goal:** Measure the actual frame rate in the cef-rs OSR example to determine
whether it truly achieves 60fps or just feels smoother for other reasons.

**Rationale:** We assumed the cef-rs example achieves 60fps because it "looks
smooth." But we never actually measured it. This assumption drove our hypothesis
that the profile server's environment was causing throttling. Before pursuing
more complex experiments, we should verify this assumption.

#### Expected Outcomes

| Result                      | Implication                                                                                                      |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| **cef-rs achieves ~60fps**  | CEF *can* do 60fps. Something about ts3's configuration or environment is different. Continue investigating ts3. |
| **cef-rs also gets ~30fps** | The "smoothness" isn't frame rate—it's latency or frame pacing. The 30fps cap may be fundamental to CEF's OSR.   |

#### Implementation Steps

1. ✅ Add frame timing to `cef-rs/examples/osr/src/webrender.rs` in the
   `on_accelerated_paint` handler (similar to ts3's `[FRAME-TX]` logging)
2. ✅ Run the example, scroll around on a content-heavy page
3. ✅ Analyze the frame intervals using the same methodology as Experiment 2

#### Results

**Overall stats:** 305 frames over 8279ms = **36.8 fps average** (includes idle
periods and stalls during page load).

**Sustained rendering (frames 10-80):** 70 frames over 1149ms = **60.9 fps**

**Frame interval analysis:**

```
frame 13→14: 163-146 = 17ms
frame 14→15: 179-163 = 16ms
frame 15→16: 196-179 = 17ms
frame 16→17: 212-196 = 16ms
```

The intervals are consistently **16-17ms** during active rendering. This is
**60fps**.

**Note:** The example runs two browser windows (github.com and google.com).
Duplicate timestamps (e.g., frames 116-117 both at 1846ms) show both browsers
sometimes paint simultaneously.

#### Comparison

| Metric         | cef-rs example | ts3     |
| -------------- | -------------- | ------- |
| Frame interval | 16-17ms        | 33-34ms |
| Active FPS     | ~60fps         | ~30fps  |
| With stalls    | ~37fps         | ~17fps  |

#### Conclusion

**CEF can deliver 60fps.** The cef-rs example proves it. The problem is specific
to ts3's profile server environment, not CEF's fundamental capability.

This invalidates the conclusions from Issues 338 and 339 which claimed CEF had a
hard-coded 30fps cap. The cap exists somewhere in ts3's implementation, not in
CEF itself.

**Next step:** Identify what ts3's profile server does differently from the
cef-rs example that causes the 30fps throttling

### Experiment 4: Enable `external_message_pump`

**Status:** Not started

**Goal:** Fix the configuration mismatch between ts3 and the cef-rs example by
enabling `external_message_pump` in CEF settings.

**Rationale:** The cef-rs example (60fps) sets `external_message_pump: true`. ts3
(17fps) does not. This is the most obvious configuration difference between the
two.

ts3 is in a **contradictory state**: it calls `do_message_loop_work()` in a
manual polling loop (the external message pump pattern), but doesn't tell CEF
it's using an external message pump. CEF's internal scheduler doesn't know the
app is manually driving the loop, so it may be fighting the manual pumping or
throttling frame production.

#### Configuration Comparison

| Setting                  | cef-rs example (60fps) | ts3 (17fps)            |
| ------------------------ | ---------------------- | ---------------------- |
| `external_message_pump`  | `true`                 | **not set (false)**    |
| `windowless_rendering`   | `true`                 | `true`                 |
| Message loop             | `do_message_loop_work` | `do_message_loop_work` |

#### Why Previous Attempts Failed

Issue 325, Experiments 4-5 tried adding `external_message_pump: 1` but combined
it with **CFRunLoop timers**, which broke for unrelated reasons (timers stopped
firing after initial setup). Nobody has tried the simple combination of
`external_message_pump: 1` with the **existing polling loop** that already works.

#### Implementation

One-line change to CEF settings in `ts3/termsurf-profile/src/main.rs`:

```rust
let settings = cef::Settings {
    windowless_rendering_enabled: 1,
    no_sandbox: 1,
    external_message_pump: 1,  // NEW: Tell CEF we're driving the loop
    root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
    browser_subprocess_path: cef::CefString::from(helper_path.to_str().unwrap()),
    persist_session_cookies: 1,
    ..Default::default()
};
```

The existing polling loop stays unchanged:

```rust
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}
```

#### Success Criteria

| Result       | Conclusion                                                              |
| ------------ | ----------------------------------------------------------------------- |
| ~60fps       | `external_message_pump` was the missing setting. Keep this fix.         |
| Still ~30fps | The setting alone isn't enough. Investigate other differences (window). |

#### Notes

- This is intentionally minimal — one setting change, no other modifications
- If this fails, a secondary difference to investigate is the cef-rs example's
  `NSApplicationActivationPolicyRegular` call, which makes the process a
  foreground GUI app and may affect macOS frame scheduling

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  investigation
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Research
  that led to this hypothesis
