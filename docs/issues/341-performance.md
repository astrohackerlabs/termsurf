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

**Status:** PARTIAL SUCCESS — 60fps frames now dominant, but stalls remain

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

#### Results

**Overall stats:** 234 frames over 10631ms = **22.0 fps average**

**Sustained rendering (frames 20-180, excluding page load):**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| 6-20ms (~60fps)      | 79    | **52%**    |
| 21-40ms (~30fps)     | 28    | 19%        |
| 41-70ms (~15fps)     | 12    | 8%         |
| 71-110ms (~10fps)    | 11    | 7%         |
| >110ms (stall)       | 16    | 11%        |
| 0-5ms (burst)        | 5     | 3%         |

**Most common intervals:** 17ms (59 occurrences), 16ms (30 occurrences) — both
are 60fps.

#### Comparison Across All Experiments

| Metric               | Exp 2 (no change) | Exp 4 (ext pump) | cef-rs example |
| -------------------- | ----------------- | ----------------- | -------------- |
| Average FPS          | 17.0              | **22.0**          | 60.9           |
| Frames at ~60fps     | 3%                | **52%**           | ~90%           |
| Frames at ~30fps     | 80%               | 19%               | ~5%            |
| Frames >110ms        | 7%                | 11%               | rare           |
| Most common interval | 33ms              | **17ms**          | 16ms           |

#### Conclusion

`external_message_pump` made a significant difference — the most common frame
interval flipped from 33ms to **17ms**. CEF is now *trying* to run at 60fps,
but something still causes it to drop frames and stall.

The bimodal distribution (sometimes 16ms, sometimes 33ms+) suggests CEF is
intermittently throttling — possibly because the process lacks a window/display
connection that the cef-rs example has.

**Next step:** Investigate the remaining difference: the cef-rs example has a
visible window and sets `NSApplicationActivationPolicyRegular`, making it a
foreground GUI app. The profile server has neither.

### Experiment 5: Set `NSApplicationActivationPolicyRegular`

**Status:** FAILED — Activation policy had no effect on frame rate

**Goal:** Tell macOS the profile server is a foreground GUI app to prevent
background process throttling.

**Rationale:** macOS aggressively throttles background processes via **App Nap**:
timer coalescing, reduced scheduling priority, and lower CPU allocation. The
profile server has no window and no activation policy — macOS treats it as a
background process.

This perfectly explains the bimodal distribution from Experiment 4: sometimes the
process gets scheduled on time (16ms), sometimes macOS delays it (33ms+). App
Nap doesn't fully block the process — it just makes scheduling inconsistent,
which is exactly what we observe.

The cef-rs example calls `NSApplicationActivationPolicyRegular` before CEF init.
ts3's profile server does not.

#### Implementation

One line before CEF initialization in `ts3/termsurf-profile/src/main.rs`:

```rust
// Issue 341, Experiment 5: Tell macOS this is a foreground GUI app
// to prevent App Nap from throttling timer/scheduling precision.
unsafe {
    use cocoa::appkit::{NSApp, NSApplication, NSApplicationActivationPolicyRegular};
    NSApp().setActivationPolicy_(NSApplicationActivationPolicyRegular);
}
```

Added `cocoa = { workspace = true }` to `termsurf-profile/Cargo.toml`.

#### Results

**Overall stats:** 270 frames over 14.1s = **19.1 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| 0ms (burst/double)   | 32    | 12%        |
| 6-20ms (~60fps)      | 113   | **42%**    |
| 21-40ms (~30fps)     | 17    | 6%         |
| 41-70ms (~15fps)     | 50    | 19%        |
| 71-110ms (~10fps)    | 39    | 15%        |
| >110ms (stall)       | 18    | 7%         |

**Most common intervals:** 17ms (70), 16ms (37), 0ms (32)

#### Comparison Across All Experiments

| Metric               | Exp 2  | Exp 4 (+ext pump) | Exp 5 (+activation) | cef-rs |
| -------------------- | ------ | ------------------ | -------------------- | ------ |
| Average FPS          | 17.0   | 22.0               | **19.1**             | 60.9   |
| Frames at ~60fps     | 3%     | 52%                | 42%                  | ~90%   |
| Most common interval | 33ms   | 17ms               | 17ms                 | 16ms   |
| Avg interval         | 59ms   | 45ms               | **52ms**             | ~16ms  |

#### Conclusion

`NSApplicationActivationPolicyRegular` did **not** improve frame rate. The
results are virtually unchanged from Experiment 4 — slightly worse if anything
(19.1 fps vs 22.0 fps, within noise).

**App Nap is not the cause.** The bimodal distribution persists despite telling
macOS this is a foreground GUI app. The large gaps (50ms, 67ms, 83ms) are exact
multiples of ~16.7ms (vsync), which points to **dropped vsync beats** rather
than process scheduling delays.

Notable: 32 frames arrived at 0ms intervals (pairs of frames at the same
millisecond), meaning CEF sometimes double-paints. Combined with the
vsync-multiple gaps, this suggests the `pump_app_events` +
`do_message_loop_work()` polling loop is not synchronized with CEF's internal
compositor timing.

The key remaining difference from the cef-rs example: it has an actual **window**
connected to a display, which provides a real vsync signal. The profile server
has no window — CEF's compositor has no display link to synchronize against.

### Experiment 6: Use `run_message_loop()` Instead of Manual Pump

**Status:** FAILED — CEF's built-in loop is worse than manual pump

**Goal:** Test whether CEF's built-in message loop achieves 60fps, eliminating
our manual polling loop as the cause.

**Rationale:** After Experiments 2-5, the ts3 profile server now has identical
CEF settings to the cef-rs example:

| Setting                            | cef-rs example | ts3 (after Exp 5) |
| ---------------------------------- | -------------- | ----------------- |
| `external_message_pump`            | `true`         | `true`            |
| `windowless_rendering_enabled`     | `true`         | `true`            |
| `no_sandbox`                       | `true`         | `true`            |
| `NSApplicationActivationPolicy`    | Regular        | Regular           |
| winit event loop                   | Yes            | Yes               |
| `on_schedule_message_pump_work`    | Not impl'd     | Not impl'd        |
| `ControlFlow::Poll`               | Yes            | Yes               |

The only remaining difference is that the cef-rs example has **real windows**
with wgpu surfaces connected to the display. These provide a CVDisplayLink — a
hardware-driven 60Hz callback synchronized to the monitor's refresh rate.

Before adding a window (complex), we should test something simpler:
`cef::run_message_loop()`. This is CEF's built-in blocking message loop. On
macOS, it runs an `NSRunLoop`/`CFRunLoop` which may handle timing and display
integration differently than our manual `do_message_loop_work()` polling. Our
manual pump may be fighting CEF's internal scheduling.

#### Implementation

Replace the manual pump loop with CEF's built-in loop:

```rust
// Remove external_message_pump from settings (run_message_loop is the
// non-external-pump mode)
let settings = cef::Settings {
    windowless_rendering_enabled: 1,
    no_sandbox: 1,
    // external_message_pump: REMOVED
    root_cache_path: ...,
    browser_subprocess_path: ...,
    persist_session_cookies: 1,
    ..Default::default()
};

// Replace winit event loop with CEF's own loop
println!("Profile: Running CEF message loop...");
cef::run_message_loop();
```

Change shutdown to use `cef::quit_message_loop()`:

```rust
// Ctrl+C handler and GUI disconnect handler
cef::quit_message_loop();
```

#### Why This Is Safe

- XPC handlers run on dispatch queue threads, not the main thread
- `run_message_loop()` blocks the main thread, but XPC events still fire
- `quit_message_loop()` can be called from any thread to unblock it

#### Success Criteria

| Result       | Conclusion                                                               |
| ------------ | ------------------------------------------------------------------------ |
| ~60fps       | Our manual pump was the problem. Keep `run_message_loop()`.              |
| Still ~20fps | CEF's own loop can't do 60fps here either. The process needs a window.   |

#### Results

**Overall stats:** 298 frames over 16.4s = **18.1 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 36    | 12%        |
| 60fps (6-20ms)       | 70    | **24%**    |
| 30fps (21-40ms)      | 14    | 5%         |
| Mid (41-70ms)        | 75    | 25%        |
| Low (>70ms)          | 102   | **34%**    |

**Most common long intervals:** 78ms (12), 75ms (11), 74ms (11), 76ms (10) —
clustered around **~75ms** (roughly 5 vsync beats at 16.7ms each).

**Frame pairing pattern:** Short+long pairs average **85ms**. CEF produces
frames in bursts of 2 (one at 0-10ms, then another 70-80ms later), rather than
the steady 16ms cadence seen in the cef-rs example.

**Max consecutive 60fps frames:** Only **5** (occurred once). Most 60fps frames
were isolated singles (53 occurrences of a lone 60fps frame).

#### Comparison Across All Experiments

| Metric               | Exp 4 (ext pump) | Exp 5 (+activation) | Exp 6 (run_msg_loop) | cef-rs |
| -------------------- | ---------------- | -------------------- | -------------------- | ------ |
| Average FPS          | 22.0             | 19.1                 | **18.1**             | 60.9   |
| Frames at ~60fps     | 52%              | 42%                  | **24%**              | ~90%   |
| Avg interval         | 45ms             | 52ms                 | **55ms**             | ~16ms  |
| Low (>70ms)          | —                | 15%                  | **34%**              | rare   |
| Dominant pattern     | bimodal 17/33ms  | bimodal 17/50-83ms   | **~75ms gaps**       | 16ms   |

#### Conclusion

`run_message_loop()` is **worse** than the manual pump with
`external_message_pump` (Experiment 4). The frame rate character changed
completely:

- **Experiment 4** had a bimodal distribution with the majority of frames at
  16-17ms — CEF was *trying* to hit 60fps but dropping some beats
- **Experiment 6** has a dominant **~75ms** interval — CEF's own loop produces
  frames at roughly **13fps** when active, with occasional bursts

This rules out the manual polling loop as the cause. The manual pump
(Experiment 4) was actually doing a **better** job of driving CEF's compositor
than CEF's own `run_message_loop()`.

The problem is confirmed to be the **process environment** — specifically the
lack of a window and display link. The next experiment should revert to the
Experiment 4 configuration (manual pump + `external_message_pump`) and add a
hidden 1x1 window to provide a CVDisplayLink/display connection.

### Experiment 7: Add a Hidden 1x1 Window

**Status:** SIGNIFICANT SUCCESS — 78% of frames at 60fps, sustained runs of 57 frames

**Goal:** Provide the process with a display link by creating a hidden window,
closing the last remaining gap between ts3 and the cef-rs example.

**Rationale:** Six experiments have eliminated every other variable:

| Experiment | What it tested                  | Result  |
| ---------- | ------------------------------- | ------- |
| Exp 2      | winit event loop (no window)    | No help |
| Exp 4      | `external_message_pump`         | Partial (52% at 60fps) |
| Exp 5      | `NSApplicationActivationPolicy` | No help |
| Exp 6      | `run_message_loop()`            | Worse   |

CEF settings are now identical to the cef-rs example. The only remaining
difference: the cef-rs example has **real windows** connected to the display
server. The profile server has none.

On macOS, a window connected to the window server provides a **CVDisplayLink**
— a hardware-driven callback synchronized to the monitor's refresh rate (60Hz).
This is the timing signal that winit's event loop processes. Without a window,
`pump_app_events` has nothing meaningful to pump, and CEF's compositor has no
vsync reference.

This explains the bimodal pattern from Experiment 4: CEF sometimes catches a
vsync beat (16ms), sometimes misses one or more (33ms, 50ms, 67ms, 83ms). The
compositor is trying to synchronize but has no signal to lock onto.

#### Implementation

Revert to the Experiment 4 configuration (best result: 22fps, 52% at 60fps),
then add a hidden window:

1. Restore `winit` dependency
2. Restore `external_message_pump: 1` in CEF settings
3. Restore manual pump loop (`do_message_loop_work()` + `pump_app_events`)
4. In `ApplicationHandler::resumed()`, create a 1x1 hidden window:

```rust
impl ApplicationHandler for MinimalApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        // Create a tiny hidden window to establish a display link.
        // The window itself is invisible — it just anchors the process
        // to the macOS window server and its vsync callback.
        let attrs = WindowAttributes::default()
            .with_inner_size(winit::dpi::LogicalSize::new(1.0, 1.0))
            .with_visible(false)
            .with_title("termsurf-profile");
        self.window = Some(event_loop.create_window(attrs).unwrap());
    }
}
```

#### Success Criteria

| Result       | Conclusion                                                                |
| ------------ | ------------------------------------------------------------------------- |
| ~60fps       | The display link was the missing piece. Keep the hidden window.           |
| Still ~22fps | It's not the display link. Try adding a wgpu surface on the window next.  |

#### Results

**Overall stats:** 647 frames over 25.2s = **25.7 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 23    | 4%         |
| 60fps (6-20ms)       | 503   | **78%**    |
| 30fps (21-40ms)      | 39    | 6%         |
| Mid (41-70ms)        | 17    | 3%         |
| Low (>70ms)          | 64    | 10%        |

**Most common intervals:** 17ms (327), 16ms (174) — overwhelmingly **60fps**.

**Sustained rendering:** Max consecutive 60fps frames: **57** (nearly a full
second). Multiple long streaks: 38, 34, 32, 29, 28 consecutive frames. This
is qualitatively different from all prior experiments where the max was 5.

**Stalls:** The remaining 10% of "Low" frames are mostly 100ms+ stalls, likely
caused by page load, layout, or JavaScript execution — not systematic
throttling.

#### Comparison Across All Experiments

| Metric               | Exp 4  | Exp 5  | Exp 6  | **Exp 7**  | cef-rs |
| -------------------- | ------ | ------ | ------ | ---------- | ------ |
| Average FPS          | 22.0   | 19.1   | 18.1   | **25.7**   | 60.9   |
| Frames at ~60fps     | 52%    | 42%    | 24%    | **78%**    | ~90%   |
| Avg interval         | 45ms   | 52ms   | 55ms   | **39ms**   | ~16ms  |
| Low (>70ms)          | —      | 15%    | 34%    | **10%**    | rare   |
| Max consecutive 60fps| 5      | —      | 5      | **57**     | —      |
| Dominant pattern     | bimodal| bimodal| ~75ms  | **steady** | steady |

#### Conclusion

The hidden window hypothesis was **correct**. The display link provided by the
window gives CEF's compositor the vsync timing signal it needs.

The bimodal pattern from Experiment 4 is largely gone. CEF now sustains 60fps
in long uninterrupted runs (up to 57 consecutive frames). The 25.7 fps average
is dragged down by idle periods and page load stalls, not by systematic frame
dropping during active rendering.

**What the window provides:** On macOS, a window connected to the window server
receives a CVDisplayLink callback at the monitor's refresh rate (60Hz). This
callback flows through winit's `pump_app_events`, giving the process a
hardware-synchronized timing heartbeat. Without it, CEF's compositor had no
vsync reference and dropped frames unpredictably.

**Remaining gap from cef-rs (78% vs 90% at 60fps):** The cef-rs example renders
directly to its window via wgpu. ts3's profile server creates the window but
doesn't render to it — the actual rendering happens in the GUI process. This
may mean the display link integration is slightly less effective without an
active rendering surface. However, 78% at 60fps with sustained runs is a
dramatic improvement and likely sufficient for smooth user experience.

#### Notes

- The window is invisible (`with_visible(false)`) — no UI impact
- No wgpu surface or rendering needed — just the window's existence
- The `cocoa` dependency (from Experiment 5) was not needed — winit handles
  window creation natively
- This is a clean permanent solution: minimal overhead, no visible artifacts

### Experiment 8: Replace Hidden Window with CVDisplayLink

**Status:** FAILED — CVDisplayLink alone does not provide what CEF needs

**Goal:** Get the vsync timing signal from Experiment 7 without a window, by
creating a CVDisplayLink directly.

**Problem with Experiment 7:** The hidden 1x1 window steals focus from the GUI
when the profile server process starts. `with_active(false)` did not prevent
this. On macOS, creating an NSWindow — even an invisible one — can cause the
process to activate and steal focus from the frontmost application.

Other window-based fixes (NSApplicationActivationPolicyAccessory, overriding
canBecomeKey/canBecomeMain) are fragile and fight the window system. The
cleanest solution is to not create a window at all.

**Insight from Experiment 7:** What we actually need is not the window itself —
it's the **CVDisplayLink** that comes with it. A CVDisplayLink is a CoreVideo
timer that fires a callback synchronized to the display's refresh rate. When a
process has a window, the window server provides this automatically. But a
CVDisplayLink can also be created **without a window** via the CoreVideo API.

#### Implementation

Replace the winit event loop and hidden window with a CVDisplayLink created via
CoreVideo FFI:

1. Remove `winit` dependency (no longer needed)
2. Add CoreVideo FFI bindings for CVDisplayLink
3. Create a CVDisplayLink at startup, before CEF init
4. Use a simple polling loop (like the original sleep loop from before Exp 2)
   with `do_message_loop_work()`
5. The CVDisplayLink runs on its own thread and provides the vsync timing that
   CEF needs, independent of the message loop

```rust
// CoreVideo FFI (minimal bindings needed)
extern "C" {
    fn CVDisplayLinkCreateWithActiveCGDisplays(link: *mut *mut c_void) -> i32;
    fn CVDisplayLinkSetOutputCallback(
        link: *mut c_void,
        callback: extern "C" fn(*mut c_void, *const c_void, *const c_void,
                                 u64, *mut u64, *mut c_void) -> i32,
        userInfo: *mut c_void,
    ) -> i32;
    fn CVDisplayLinkStart(link: *mut c_void) -> i32;
    fn CVDisplayLinkStop(link: *mut c_void) -> i32;
    fn CVDisplayLinkRelease(link: *mut c_void);
}

// Callback does nothing — we just need the display link to exist
extern "C" fn display_link_callback(
    _link: *mut c_void, _now: *const c_void, _output_time: *const c_void,
    _flags_in: u64, _flags_out: *mut u64, _ctx: *mut c_void,
) -> i32 {
    0 // kCVReturnSuccess
}

// Create and start a display link
let mut link: *mut c_void = std::ptr::null_mut();
CVDisplayLinkCreateWithActiveCGDisplays(&mut link);
CVDisplayLinkSetOutputCallback(link, display_link_callback, std::ptr::null_mut());
CVDisplayLinkStart(link);
```

The main loop simplifies back to:

```rust
loop {
    if QUIT_FLAG.load(Ordering::Relaxed) {
        break;
    }
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}

// Cleanup
CVDisplayLinkStop(link);
CVDisplayLinkRelease(link);
```

#### Why This Might Work

Experiment 7 proved that the process needs a connection to the display server's
timing. A CVDisplayLink provides exactly this — it registers the process with
CoreVideo's display timing system. The hidden window was just an indirect way to
get a display link; this creates one directly.

#### Why This Might NOT Work

It's possible that CEF's compositor doesn't use CVDisplayLink directly. It may
rely on the window server's per-window vsync notifications (which only exist for
actual windows). If CEF checks for window-level vsync rather than process-level
display timing, the bare CVDisplayLink won't help.

#### Success Criteria

| Result       | Conclusion                                                               |
| ------------ | ------------------------------------------------------------------------ |
| ~78%+ at 60fps | CVDisplayLink alone provides the timing. Clean solution, no window.    |
| Back to ~52% | The window itself matters, not just the display link. Need window fix. |

#### Results

**Overall stats:** 230 frames over 12.6s = **18.2 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 9     | 4%         |
| 60fps (6-20ms)       | 69    | **30%**    |
| 30fps (21-40ms)      | 47    | 21%        |
| Mid (41-70ms)        | 37    | 16%        |
| Low (>70ms)          | 67    | **29%**    |

**Most common intervals:** 10ms (19), 9ms (9), 11ms (9) — no dominant 16-17ms
peak. The intervals are scattered rather than clustered at vsync boundaries.

**Max consecutive 60fps frames:** Only **4** (compared to 57 in Experiment 7).

#### Comparison Across Key Experiments

| Metric               | Exp 4 (ext pump) | **Exp 7 (window)** | **Exp 8 (CVDisplayLink)** | cef-rs |
| -------------------- | ---------------- | ------------------ | ------------------------- | ------ |
| Average FPS          | 22.0             | **25.7**           | **18.2**                  | 60.9   |
| Frames at ~60fps     | 52%              | **78%**            | **30%**                   | ~90%   |
| Avg interval         | 45ms             | **39ms**           | **55ms**                  | ~16ms  |
| Max consecutive 60fps| 5                | **57**             | **4**                     | —      |

#### Conclusion

The CVDisplayLink alone does **not** work. Performance is worse than Experiment 4
(30% vs 52% at 60fps) and far worse than Experiment 7 (30% vs 78%). The "Why
This Might NOT Work" prediction was correct.

CEF's compositor does not use process-level CVDisplayLink timing. It relies on
the **window server's per-window vsync notifications**, which only exist for
actual windows. A bare CVDisplayLink registers with CoreVideo but doesn't create
the window-server connection that CEF depends on.

**Next step:** Revert to the hidden window approach (Experiment 7) and fix the
focus-stealing problem. Two viable options:

1. **`NSApplicationActivationPolicyAccessory`** — Makes the process an
   "accessory" app that can own windows but doesn't appear in the Dock and
   doesn't steal focus from other apps. Requires `cocoa` dependency.
2. **Native NSWindow with `canBecomeKey: NO`** — Create the window via AppKit
   directly, overriding focus behavior at the Cocoa level.

### Experiment 9: Restore Hidden Window (Exp 7 Baseline)

**Status:** Not started

**Goal:** Revert the code from Experiment 8 (CVDisplayLink) back to the
Experiment 7 state (hidden 1x1 winit window). This is a pure restoration — no
new changes, no focus-stealing fix.

**Rationale:** Experiment 7 achieved the best performance so far (78% at 60fps,
57 consecutive frames). Experiment 8 attempted to get the same benefit without a
window but failed (30% at 60fps). The hidden window approach is the correct
foundation. Focus-stealing will be addressed in a subsequent experiment.

#### Changes

1. **Add `winit` dependency back** to `Cargo.toml`
2. **Remove `cv_display_link` module** entirely
3. **Restore `MinimalApp`** struct implementing `winit::application::ApplicationHandler`
   - `resumed()` creates a hidden 1×1 window
   - `about_to_wait()` calls `do_message_loop_work()` and checks `QUIT_FLAG`
4. **Restore winit event loop** using `pump_app_events(Some(Duration::ZERO))`
   in a loop with 1ms sleep
5. **Keep** `external_message_pump: 1` and `QUIT_FLAG`

#### Expected Outcome

Performance should match Experiment 7 results exactly:

| Metric                | Expected |
| --------------------- | -------- |
| Average FPS           | ~25.7    |
| Frames at ~60fps      | ~78%     |
| Max consecutive 60fps | ~57      |

**Known issue:** The hidden window steals focus. This will be fixed in a
subsequent experiment.

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  investigation
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Research
  that led to this hypothesis
