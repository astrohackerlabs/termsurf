+++
status = "closed"
opened = "2026-02-05"
closed = "2026-02-05"
+++

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

**Status:** SUCCESS — Confirms hidden window approach is the correct foundation

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

#### Results

**Overall stats:** 514 frames over 23.0s = **22.3 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 33    | 6%         |
| 60fps (6-20ms)       | 314   | **61%**    |
| 30fps (21-40ms)      | 31    | 6%         |
| Mid (41-70ms)        | 75    | 14%        |
| Low (>70ms)          | 60    | 11%        |

**Dominant intervals:** 17ms (167 times), 16ms (98 times) — a strong
vsync-aligned peak at 16-17ms, exactly what a 60Hz display produces.

**Max consecutive 60fps frames:** **35**

#### Comparison Across Key Experiments

| Metric                | Exp 4 (ext pump) | Exp 7 (window) | Exp 8 (CVDisplayLink) | **Exp 9 (restore)** |
| --------------------- | ---------------- | -------------- | --------------------- | ------------------- |
| Average FPS           | 22.0             | 25.7           | 18.2                  | **22.3**            |
| Frames at ~60fps      | 52%              | 78%            | 30%                   | **61%**             |
| Max consecutive 60fps | 5                | 57             | 4                     | **35**              |
| Dominant interval     | scattered        | 16-17ms        | scattered             | **16-17ms (265)**   |

#### Conclusion

The hidden window approach is confirmed as the correct foundation. The strong
16-17ms vsync peak is back and dominant (265 out of 513 intervals), matching
Experiment 7's pattern. The slightly lower percentages compared to Exp 7 (61% vs
78%) likely reflect different browsing activity during the test rather than a
code difference.

**Next step:** Fix the focus-stealing problem with the hidden window.

### Experiment 10: NSApplicationActivationPolicyAccessory

**Status:** Not started

**Goal:** Prevent the hidden window from stealing focus by setting the profile
process's activation policy to `NSApplicationActivationPolicyAccessory`.

**Problem:** When `termsurf-profile` creates a winit window (for vsync), macOS
activates that process and pulls focus from the WezTerm GUI. The user has to
click back on the terminal window to interact with the webview. We already tried
winit's `with_active(false)` — it didn't work.

**Hypothesis:** `NSApplicationActivationPolicyAccessory` (value `1`) tells macOS
that this process is a background helper. It can own windows but won't appear in
the Dock and won't steal focus from the foreground app. This is different from
`NSApplicationActivationPolicyRegular` (value `0`, used in Exp 5 for App Nap),
which makes the app fully foreground-capable.

The three activation policies:

| Policy        | Value | Dock icon | Can own windows | Steals focus |
| ------------- | ----- | --------- | --------------- | ------------ |
| Regular       | 0     | Yes       | Yes             | Yes          |
| **Accessory** | **1** | **No**    | **Yes**         | **No**       |
| Prohibited    | 2     | No        | No              | No           |

`Prohibited` won't work because it prevents window creation entirely. `Accessory`
is the sweet spot — windows are allowed (for the vsync signal) but the process
stays in the background.

#### Changes

1. **Add `cocoa` dependency** to `Cargo.toml` (for `objc` FFI)
2. **Set activation policy before window creation** — call
   `[NSApplication sharedApplication] setActivationPolicy:1` before the winit
   event loop creates the hidden window
3. **No other changes** — same hidden window, same pump loop, same CEF settings

#### Implementation

```rust
// Set before EventLoop::new()
unsafe {
    let app: id = msg_send![class!(NSApplication), sharedApplication];
    let _: () = msg_send![app, setActivationPolicy: 1i64];
}
```

#### Expected Outcome

| Result                    | Meaning                                                    |
| ------------------------- | ---------------------------------------------------------- |
| Focus stays on WezTerm    | Fix works. Hidden window provides vsync without side effects. |
| Focus still stolen        | Accessory policy doesn't prevent winit's window activation. Need native NSWindow approach. |
| Performance regression    | Accessory policy interferes with vsync notifications. Unlikely but possible. |

#### Why This Should Work

macOS decides which app to activate based on activation policy. When a `Regular`
app creates a window, the window server activates it. An `Accessory` app can
create windows that participate in the window server (receiving vsync
notifications) but the app itself won't be activated — focus stays wherever it
was.

#### Results

**FAILED** — The main window still lost focus when opening a webview. The
Accessory activation policy does not prevent winit from stealing focus. Winit
internally calls `makeKeyAndOrderFront:` (or similar) when creating the window,
which activates the process regardless of the activation policy.

#### Conclusion

`NSApplicationActivationPolicyAccessory` is not sufficient. The focus stealing
happens at the window level (winit's internal NSWindow creation), not at the
application activation policy level. Need to bypass winit entirely and create
the NSWindow directly with `canBecomeKey: NO`.

### Experiment 11: Native NSWindow with canBecomeKey: NO

**Status:** PARTIAL — Focus fix works, but lost vsync benefit

**Goal:** Replace winit with a native NSWindow created via AppKit FFI, subclassed
to override `canBecomeKey` returning `NO`. This prevents the window from ever
becoming the key window, which is what causes focus stealing.

**Problem:** Experiments 7, 9, and 10 proved that a window is needed for vsync,
but winit's window creation steals focus. `NSApplicationActivationPolicyAccessory`
(Exp 10) didn't help because winit internally activates the window during
creation. We need full control over the window's behavior.

**Hypothesis:** When an NSWindow's `canBecomeKey` returns `NO`, it cannot become
the key window, so it cannot steal keyboard focus. Combined with `orderBack:`
instead of `makeKeyAndOrderFront:`, the window will exist in the window server
(providing vsync notifications) without any focus side effects.

#### Changes

1. **Remove `winit` dependency** from `Cargo.toml` (keep `cocoa`)
2. **Remove `MinimalApp`** struct and winit event loop entirely
3. **Register a custom NSWindow subclass** via the Objective-C runtime that
   overrides `canBecomeKey` to return `NO`
4. **Create a 1×1 borderless NSWindow** using the custom subclass
5. **Call `orderBack:`** instead of `makeKeyAndOrderFront:` to place the window
   in the window server without activating it
6. **Keep `NSApplicationActivationPolicyAccessory`** as belt-and-suspenders
7. **Restore simple sleep+pump loop** (like Exp 8, without CVDisplayLink)

#### Implementation

```rust
use cocoa::appkit::{NSWindow, NSWindowStyleMask, NSBackingStoreType};
use cocoa::base::{id, nil, NO};
use cocoa::foundation::{NSPoint, NSRect, NSSize};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel, BOOL};

// 1. Register subclass with canBecomeKey -> NO
extern "C" fn can_become_key(_this: &Object, _sel: Sel) -> BOOL { NO }

let superclass = Class::get("NSWindow").unwrap();
let mut decl = ClassDecl::new("CEFHostWindow", superclass).unwrap();
unsafe { decl.add_method(sel!(canBecomeKey), can_become_key as extern "C" fn(&Object, Sel) -> BOOL); }
let host_class = decl.register();

// 2. Create window using custom class
let window: id = unsafe {
    let alloc: id = msg_send![host_class, alloc];
    let rect = NSRect::new(NSPoint::new(0., 0.), NSSize::new(1., 1.));
    let window: id = msg_send![alloc,
        initWithContentRect:rect
        styleMask:NSWindowStyleMask::NSBorderlessWindowMask
        backing:NSBackingStoreType::NSBackingStoreBuffered
        defer:NO
    ];
    let _: () = msg_send![window, orderBack:nil];
    window
};
```

#### Expected Outcome

| Result                    | Meaning                                                       |
| ------------------------- | ------------------------------------------------------------- |
| Focus stays on WezTerm    | Fix works. Native window provides vsync without focus issues. |
| Focus still stolen        | Something else is activating the process. Investigate further. |
| Performance regression    | Custom NSWindow doesn't get vsync. Unlikely — it's still an NSWindow. |

#### Why This Should Work

macOS focus stealing requires two things: the app must be activated, and the
window must become key. By subclassing NSWindow to return `NO` from
`canBecomeKey`, the window can never become key — it simply exists in the window
server's window list. The window server still sends vsync notifications to all
windows, regardless of key status. Combined with `orderBack:` (which places the
window without activating it) and `NSApplicationActivationPolicyAccessory` (which
prevents app-level activation), there are three layers of protection against
focus stealing.

#### Results

**Focus:** SUCCESS — The main window retained focus when opening a webview.

**Performance:** REGRESSION — Lost the vsync benefit entirely.

**Overall stats:** 407 frames over 21.2s = **19.1 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 20    | 4%         |
| 60fps (6-20ms)       | 140   | **34%**    |
| 30fps (21-40ms)      | 50    | 12%        |
| Mid (41-70ms)        | 90    | 22%        |
| Low (>70ms)          | 106   | 26%        |

**Dominant intervals:** Scattered (10ms, 13ms, 12ms) — no 16-17ms vsync peak.

**Max consecutive 60fps frames:** Only **4**

#### Comparison Across Key Experiments

| Metric                | Exp 4 (baseline) | Exp 9 (winit window) | **Exp 11 (native NSWindow)** |
| --------------------- | ---------------- | -------------------- | ---------------------------- |
| Average FPS           | 22.0             | 22.3                 | **19.1**                     |
| Frames at ~60fps      | 52%              | 61%                  | **34%**                      |
| Max consecutive 60fps | 5                | 35                   | **4**                        |
| Dominant interval     | scattered        | 16-17ms              | **scattered**                |

#### Conclusion

The native NSWindow fixed focus stealing but **lost the vsync signal**.
Performance is worse than even the Exp 4 baseline. The bare `orderBack:`
NSWindow doesn't get the same window server vsync notifications that winit's
window was getting.

Winit likely configures something extra — a CALayer, a content view, or a
display link tied to the window — that our raw NSWindow doesn't have. A bare
NSWindow created with `orderBack:` may not be fully registered with the window
server's compositing pipeline.

**Next step:** Try `orderFront:` instead of `orderBack:` — the window may need
to be fully on-screen for the window server to send vsync notifications.

### Experiment 12: orderFront instead of orderBack

**Status:** PARTIAL — Focus retained, but vsync still missing

**Goal:** Fix the vsync regression from Experiment 11 by using `orderFront:nil`
instead of `orderBack:nil`. Keep `canBecomeKey: NO` to prevent focus stealing.

**Problem:** Experiment 11 proved that a native NSWindow with `canBecomeKey: NO`
prevents focus stealing, but `orderBack:` caused a vsync regression. The window
was not fully registered with the window server's compositing pipeline.

**Hypothesis:** `orderBack:` puts the window in a partially-registered state
where the window server doesn't include it in its vsync notification list.
`orderFront:nil` fully registers the window with the compositing pipeline.
Since `canBecomeKey` returns `NO`, the window cannot become the key window even
when ordered front — keyboard focus should stay with WezTerm.

The macOS window server tracks "on-screen" windows for compositing and vsync.
`orderBack:` may place the window below everything (possibly off-screen to the
compositor), while `orderFront:` ensures the window is actively composited.

#### Changes

1. **One-line change:** Replace `orderBack:nil` with `orderFront:nil` in the
   native NSWindow creation code
2. **Everything else stays the same:** `canBecomeKey: NO` subclass,
   `NSApplicationActivationPolicyAccessory`, simple sleep+pump loop

#### Expected Outcome

| Result                       | Meaning                                                          |
| ---------------------------- | ---------------------------------------------------------------- |
| Focus stays + vsync restored | Both problems solved. Ship it.                                   |
| Focus stolen                 | `orderFront:` activates the app despite Accessory policy and canBecomeKey: NO. Need another approach. |
| Vsync still missing          | The issue isn't orderBack vs orderFront. Winit does something else (CALayer, content view, etc.). |

#### Why This Should Work

`canBecomeKey` and `orderFront:` are orthogonal:

- `orderFront:` controls window **visibility and compositing** — it tells the
  window server "this window is on-screen, include it in your rendering pipeline"
- `canBecomeKey` controls **keyboard focus** — it determines whether the window
  can receive keyboard events

A window can be ordered front (visible, composited, receiving vsync) without
being key (receiving keyboard input). This is exactly how utility/palette windows
work in macOS — they're visible and rendered but don't steal the text cursor.

#### Results

**Focus:** SUCCESS — The main window retained focus.

**Performance:** Still regressed — essentially identical to Experiment 11.

**Overall stats:** 415 frames over 20.0s = **20.7 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 13    | 3%         |
| 60fps (6-20ms)       | 141   | **34%**    |
| 30fps (21-40ms)      | 58    | 14%        |
| Mid (41-70ms)        | 103   | 24%        |
| Low (>70ms)          | 99    | 23%        |

**Dominant intervals:** Scattered (11ms, 10ms, 13ms) — no 16-17ms vsync peak.

**Max consecutive 60fps frames:** Only **4** (same as Exp 11)

#### Comparison

| Metric                | Exp 9 (winit window) | Exp 11 (orderBack) | **Exp 12 (orderFront)** |
| --------------------- | -------------------- | ------------------- | ----------------------- |
| Average FPS           | 22.3                 | 19.1                | **20.7**                |
| Frames at ~60fps      | 61%                  | 34%                 | **34%**                 |
| Max consecutive 60fps | 35                   | 4                   | **4**                   |
| Dominant interval     | 16-17ms              | scattered           | **scattered**           |

#### Conclusion

`orderFront:` did not restore vsync. The issue is not `orderBack` vs
`orderFront` — our bare NSWindow is missing something that winit's window has.

The most likely culprit is that winit creates a **content view with a CALayer
backing**. On macOS, the window server's compositing pipeline is driven by Core
Animation layers, not the NSWindow itself. A bare NSWindow without a layer-backed
content view may not participate in the vsync-driven compositing cycle. Winit
creates an `NSView` as the content view and enables layer backing
(`wantsLayer = YES`), which is what actually hooks into the display's refresh
cycle.

**Next step:** Add a layer-backed NSView as the content view to the native
NSWindow.

### Experiment 13: Add Layer-Backed Content View

**Status:** FAILED — CALayer did not restore vsync

**Goal:** Restore vsync by adding an NSView with `wantsLayer = YES` as the
content view of our native NSWindow. Keep `canBecomeKey: NO` to prevent focus
stealing.

**Problem:** Experiments 11 and 12 proved that a native NSWindow with
`canBecomeKey: NO` prevents focus stealing, but the window doesn't get vsync
notifications — regardless of `orderBack:` vs `orderFront:`. Winit's window
gets vsync but steals focus. The difference must be in how winit configures the
window beyond creation and ordering.

**Hypothesis:** On macOS, the window server's compositing pipeline is driven by
Core Animation (CA) layers, not NSWindow objects directly. The vsync cycle is:

1. Window server sends a display link callback to each compositing layer
2. CA layers that need redrawing are composited
3. The result is displayed on the next vsync

A bare NSWindow without a layer-backed content view doesn't participate in this
pipeline. Winit creates an NSView with `wantsLayer = YES` as the content view,
which creates a backing CALayer. This CALayer is what registers the window with
the window server's compositing cycle, enabling vsync notifications.

#### Changes

1. **Add three lines of objc FFI** after window creation:
   - Create an `NSView` with the window's frame
   - Set `wantsLayer = YES` on the view
   - Set it as the window's content view
2. **Everything else stays the same:** `canBecomeKey: NO` subclass,
   `NSApplicationActivationPolicyAccessory`, `orderFront:nil`, simple
   sleep+pump loop

#### Implementation

```rust
// After creating the window...
let view: id = msg_send![class!(NSView), alloc];
let view: id = msg_send![view, initWithFrame:rect];
let _: () = msg_send![view, setWantsLayer:YES];
let _: () = msg_send![window, setContentView:view];
```

#### Expected Outcome

| Result                       | Meaning                                                          |
| ---------------------------- | ---------------------------------------------------------------- |
| Vsync restored + no focus    | CALayer was the missing piece. Problem fully solved.             |
| Vsync still missing          | CALayer is not sufficient. Need to investigate winit's source code for other setup (e.g., display link, NSOpenGLContext). |
| Focus stolen                 | The content view somehow triggers activation. Unlikely since canBecomeKey is still NO. |

#### Why This Should Work

Core Animation layers are the fundamental unit of compositing on macOS. The
window server doesn't directly composite NSWindows — it composites CALayers
within those windows. When an NSView has `wantsLayer = YES`:

1. AppKit creates a backing CALayer for the view
2. The layer is registered with the window server's compositing tree
3. The window server includes this layer in its vsync-driven render pass

Without a layer-backed view, the window exists in the window list but has
nothing to composite — so the window server has no reason to include it in the
vsync cycle.

#### Results

**Focus:** SUCCESS — The main window retained focus.

**Performance:** Still no vsync — identical to Experiments 11 and 12.

**Overall stats:** 194 frames over 9.0s = **21.5 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 11    | 5%         |
| 60fps (6-20ms)       | 70    | **36%**    |
| 30fps (21-40ms)      | 9     | 4%         |
| Mid (41-70ms)        | 48    | 24%        |
| Low (>70ms)          | 55    | 28%        |

**Dominant intervals:** Scattered (10ms, 11ms, 5ms, 76ms) — no 16-17ms vsync
peak.

**Max consecutive 60fps frames:** Only **3**

#### Comparison Across Native Window Experiments

| Metric                | Exp 9 (winit) | Exp 11 (orderBack) | Exp 12 (orderFront) | **Exp 13 (CALayer)** |
| --------------------- | ------------- | ------------------- | -------------------- | -------------------- |
| Frames at ~60fps      | 61%           | 34%                 | 34%                  | **36%**              |
| Max consecutive 60fps | 35            | 4                   | 4                    | **3**                |
| Dominant interval     | 16-17ms       | scattered           | scattered            | **scattered**        |

#### Conclusion

The CALayer hypothesis was wrong. All three native NSWindow experiments (11, 12,
13) produce the same ~34-36% at 60fps with max streak of 3-4, regardless of
ordering method or layer backing. Winit's window produces 61-78% with streaks
of 35-57.

The difference isn't the window configuration — it's the **event loop**. Winit's
`pump_app_events` integrates with the macOS `NSRunLoop`, which is what drives
the window server's display refresh callbacks. Our simple
`sleep(1ms) + do_message_loop_work()` loop doesn't run the `NSRunLoop` at all.

The two remaining candidates:

1. **Winit's NSRunLoop integration** — `pump_app_events` calls
   `NSApplication.nextEventMatchingMask:` which runs the macOS run loop,
   triggering display refresh callbacks tied to the window.
2. **NSApplication event pumping** — The macOS window server delivers vsync
   notifications as events through `NSApplication`, not directly to windows.
   Without pumping `NSApplication`, the events never arrive.

**Next step:** Add `NSApplication` event pumping to the native window loop.

### Experiment 14: NSApplication Event Pumping

**Status:** FAILED — NSApp event pumping did not restore vsync

**Goal:** Restore vsync by pumping `NSApplication` events in the main loop,
replacing the naive `sleep(1ms)` polling. Keep the native NSWindow with
`canBecomeKey: NO` from Experiments 11-13.

**Problem:** Experiments 11-13 proved that a native NSWindow (with various
configurations) prevents focus stealing but doesn't get vsync. The window
itself isn't the issue — all three configurations produced identical ~34-36%
at 60fps. Winit's `pump_app_events` produces 61-78%. The difference must be
the event loop, not the window.

**Hypothesis:** Winit's `pump_app_events` internally calls
`NSApplication.nextEventMatchingMask:untilDate:inMode:dequeue:`, which runs
the macOS run loop. The macOS window server delivers display refresh
notifications as events through `NSApplication`. Without pumping these events,
the window server's vsync callbacks never fire, and CEF's compositor doesn't
get the timing signal it needs.

Evidence supporting this:

| Configuration                     | Event loop          | 60fps %  | Max streak |
| --------------------------------- | ------------------- | -------- | ---------- |
| No window, sleep+pump             | None                | 52%      | 5          |
| Native window, sleep+pump         | None                | 34-36%   | 3-4        |
| Winit window, `pump_app_events`   | NSApp event pump    | 61-78%   | 35-57      |
| CEF `run_message_loop`            | CEF's own loop      | 24%      | —          |

The only configuration that achieves vsync is the one that pumps NSApplication
events. The window alone actually makes things *worse* (34% vs 52%).

#### Changes

1. **Replace `sleep(1ms)` loop** with `NSApplication` event pumping:
   - Call `nextEventMatchingMask:untilDate:inMode:dequeue:` to drain pending
     macOS events
   - Dispatch each event with `sendEvent:`
   - Continue calling `do_message_loop_work()` after draining events
   - Keep the 1ms sleep as a yield point
2. **Everything else stays the same:** `canBecomeKey: NO` subclass,
   `NSApplicationActivationPolicyAccessory`, layer-backed content view,
   `orderFront:nil`

#### Implementation

```rust
let ns_app = cocoa::appkit::NSApp();
loop {
    if QUIT_FLAG.load(Ordering::Relaxed) { break; }

    // Pump all pending macOS events
    unsafe {
        loop {
            let event: id = msg_send![ns_app,
                nextEventMatchingMask:NSUIntegerMax
                untilDate:nil
                inMode:NSDefaultRunLoopMode
                dequeue:YES
            ];
            if event == nil { break; }
            let _: () = msg_send![ns_app, sendEvent:event];
        }
    }

    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}
```

#### Expected Outcome

| Result                       | Meaning                                                          |
| ---------------------------- | ---------------------------------------------------------------- |
| Vsync restored + no focus    | Event pumping was the missing piece. Problem fully solved.       |
| Vsync still missing          | NSApp event pumping alone isn't enough. Winit does something else (CFRunLoop integration, timer sources, etc.). |
| Focus stolen                 | Event pumping triggers app activation. Unlikely with Accessory policy + canBecomeKey: NO. |

#### Why This Should Work

The macOS window server communicates with applications through the
`NSApplication` event queue. Display refresh notifications, window
update events, and compositing signals are all delivered as events.
`NSApplication.nextEventMatchingMask:` is the standard way to receive
these — it runs the underlying `CFRunLoop`, which processes:

- Display link callbacks (per-window vsync)
- Window server notifications (damage, expose, update)
- Core Animation commit triggers

Without this event pumping, the window exists but the process never
processes the vsync events the window server sends to it.

#### Results

**Focus:** SUCCESS — The main window retained focus.

**Performance:** Still no vsync — same as Experiments 11-13.

**Overall stats:** 278 frames over 11.4s = **24.3 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 19    | 6%         |
| 60fps (6-20ms)       | 92    | **33%**    |
| 30fps (21-40ms)      | 55    | 19%        |
| Mid (41-70ms)        | 54    | 19%        |
| Low (>70ms)          | 57    | 20%        |

**Dominant intervals:** Scattered (10ms, 11ms, 14ms, 31ms) — no 16-17ms vsync
peak.

**Max consecutive 60fps frames:** Only **4**

#### Comparison Across All Native Window Experiments

| Metric                | Exp 11 (orderBack) | Exp 12 (orderFront) | Exp 13 (CALayer) | **Exp 14 (NSApp pump)** |
| --------------------- | ------------------- | -------------------- | ---------------- | ----------------------- |
| Frames at ~60fps      | 34%                 | 34%                  | 36%              | **33%**                 |
| Max consecutive 60fps | 4                   | 4                    | 3                | **4**                   |
| Dominant interval     | scattered           | scattered            | scattered        | **scattered**           |

#### Conclusion

NSApplication event pumping made no difference. Every attempt to replicate
winit's vsync benefit with a native window has failed (Exp 11-14):

- `orderBack:` — no vsync
- `orderFront:` — no vsync
- Layer-backed content view — no vsync
- NSApp event pumping — no vsync

Whatever winit does to enable vsync, it's something deeper than window
configuration or event pumping — possibly a `CFRunLoop` observer,
`CADisplayLink`, or its own `CVDisplayLink` tied to the window's display
connection. The mechanism is internal to winit's macOS backend.

**Next step:** Stop trying to replicate winit's internals. Go back to winit
and prevent focus stealing via method swizzling.

### Experiment 15: Swizzle canBecomeKey on Winit's Window

**Status:** FAILED — Global swizzle broke CEF entirely

**Goal:** Go back to winit (for vsync) and prevent focus stealing by swizzling
NSWindow's `canBecomeKey` method to return `NO` before winit creates its window.

**Problem:** Four experiments (11-14) tried to replicate winit's vsync with a
native NSWindow. All failed identically at ~34% at 60fps. Winit's internal
setup — whatever it is — cannot be replicated by creating an NSWindow manually.
Meanwhile, winit produces 61-78% at 60fps but steals focus.

Previous focus-fix attempts with winit:
- `with_active(false)` — didn't work
- `NSApplicationActivationPolicyAccessory` (Exp 10) — didn't work

Both failed because winit internally calls `makeKeyAndOrderFront:`, which makes
the window key and activates the application regardless of these settings.

**Hypothesis:** If we swizzle `canBecomeKey` on the `NSWindow` class itself
(via the ObjC runtime) *before* winit creates its EventLoop and window, then
when winit calls `makeKeyAndOrderFront:`, the window will be ordered front
(preserving vsync registration) but *cannot* become key (preserving focus).

This is the reverse of Experiment 11's approach: instead of building a focus-safe
window and trying to add vsync, we take winit's vsync-capable window and remove
its ability to steal focus.

#### Changes

1. **Remove native NSWindow code** (CEFHostWindow subclass, orderFront, content
   view, NSApp event pump — all of Exp 11-14)
2. **Restore winit** dependency and `MinimalApp` event loop from Exp 9
3. **Before `EventLoop::new()`**, swizzle `canBecomeKey` on `NSWindow`:
   - Get the `NSWindow` class via ObjC runtime
   - Replace the `canBecomeKey` method with one that returns `NO`
4. **Keep `NSApplicationActivationPolicyAccessory`** as belt-and-suspenders

#### Implementation

```rust
use objc::runtime::{Class, Method, Sel, BOOL};
use std::ffi::c_void;

// Replacement: canBecomeKey -> NO
extern "C" fn can_become_key_no(_this: &objc::runtime::Object, _sel: Sel) -> BOOL {
    cocoa::base::NO
}

// Swizzle NSWindow.canBecomeKey before winit touches it
unsafe {
    let class = Class::get("NSWindow").unwrap();
    let sel = sel!(canBecomeKey);
    let method = class.instance_method(sel).unwrap();
    let new_imp = can_become_key_no as extern "C" fn(&objc::runtime::Object, Sel) -> BOOL;
    // method_setImplementation replaces the IMP for this method
    let _: *mut c_void = msg_send![method, setImplementation: new_imp as *mut c_void];
}
```

Note: The actual swizzling API may need `objc::runtime::method_setImplementation`
or similar. The exact implementation will be determined at build time.

#### Expected Outcome

| Result                       | Meaning                                                          |
| ---------------------------- | ---------------------------------------------------------------- |
| Vsync + no focus steal       | Problem fully solved. Winit provides vsync, swizzle prevents focus. |
| Vsync but focus still stolen | Winit calls `[NSApp activateIgnoringOtherApps:]` separately. Need to swizzle that too. |
| Vsync lost                   | `canBecomeKey: NO` prevents winit from fully setting up the window. Unlikely — key status and window registration are orthogonal. |

#### Why This Should Work

Method swizzling replaces the implementation of an Objective-C method at runtime.
By replacing `canBecomeKey` on the `NSWindow` class *before* winit creates any
windows, every NSWindow in this process will return `NO` from `canBecomeKey`.

When winit calls `makeKeyAndOrderFront:`:
1. The window is ordered front (**vsync registration preserved**)
2. The runtime checks `canBecomeKey` → returns `NO`
3. The window is **not** made key (**no focus stealing**)

This is safe because `termsurf-profile` has no other NSWindows — CEF does
off-screen rendering and doesn't create macOS windows. The swizzle only affects
this process, not the system.

#### Why Previous Focus Fixes Failed

- `with_active(false)` — winit window attribute, but winit may ignore it or
  call `makeKeyAndOrderFront:` regardless
- `NSApplicationActivationPolicyAccessory` — prevents app-level activation, but
  `makeKeyAndOrderFront:` can still make the window key *within* the app,
  triggering macOS to switch focus to the process
- Swizzling `canBecomeKey` works at a deeper level — it prevents the window from
  ever becoming key, which is the root cause of focus stealing

#### Notes

- This flips the approach: instead of replicating winit's vsync (Exp 11-14),
  we disable winit's focus stealing
- Restores `winit` dependency, removes `cocoa` NSWindow creation code
- Keeps `cocoa` and `objc` for the swizzle and activation policy
- Method swizzling is a standard Objective-C pattern used by many macOS apps
  for customizing framework behavior
- If `canBecomeKey` alone isn't enough, we may also need to swizzle
  `canBecomeMain` and/or `[NSApp activateIgnoringOtherApps:]`

#### Results

**FAILED** — The webview did not open at all. The global `canBecomeKey` swizzle
on the `NSWindow` class broke CEF entirely.

Swizzling at the class level affects *every* NSWindow instance in the process —
including internal windows that CEF/Chromium creates during initialization. Even
in off-screen rendering mode, CEF creates hidden helper windows that require
`canBecomeKey` to return `YES` to function properly. The global override
prevented CEF from initializing or creating browsers.

#### Conclusion

Global method swizzling on NSWindow is not viable. CEF depends on its internal
windows being able to become key. We cannot distinguish between winit's window
(which we want to prevent from becoming key) and CEF's internal windows (which
must be allowed to become key) when swizzling at the class level.

The profile server process cannot prevent focus stealing from within itself
without breaking CEF. The fix must come from the **other side** — the GUI
process should reclaim focus after spawning the profile server.

### Experiment 16: GUI-Side Focus Reclaim

**Status:** Not started

**Goal:** Accept that the profile server will momentarily steal focus, and have
the WezTerm GUI immediately reclaim it.

**Problem:** Eight experiments (10-15) tried to prevent the profile server from
stealing focus:

| Experiment | Approach                            | Result                        |
| ---------- | ----------------------------------- | ----------------------------- |
| Exp 10     | NSApplicationActivationPolicyAccessory | Focus still stolen         |
| Exp 11     | Native NSWindow, canBecomeKey: NO   | Focus fixed, lost vsync       |
| Exp 12     | orderFront instead of orderBack     | Focus fixed, lost vsync       |
| Exp 13     | Layer-backed content view           | Focus fixed, lost vsync       |
| Exp 14     | NSApp event pumping                 | Focus fixed, lost vsync       |
| Exp 15     | Global canBecomeKey swizzle + winit | Broke CEF entirely            |

The fundamental conflict: winit's window is needed for vsync (Exp 7/9 proved
this), but winit's window creation steals focus. Every attempt to prevent focus
stealing either breaks vsync (Exp 11-14) or breaks CEF (Exp 15).

**Insight:** The focus steal is a **momentary** problem. The profile server
starts, winit creates a window, focus briefly moves to that process, then
nothing else happens — the profile server has no UI, no keyboard handling, and
the user is already interacting with WezTerm. The fix doesn't need to prevent
the steal — it just needs to **undo** it immediately.

**Hypothesis:** After the GUI spawns a profile server via XPC, it should
immediately reclaim focus. On macOS, this is done with
`[NSApp activateIgnoringOtherApps:YES]` or the modern
`NSApplication.activate()`. The timing is:

1. GUI sends `spawn_profile` to launcher via XPC
2. Launcher spawns `termsurf-profile` process
3. Profile server creates winit window → **focus steals to profile server**
4. GUI calls `[NSApp activate]` → **focus returns to GUI**

The delay between steps 3 and 4 depends on how quickly the GUI detects the
profile server has started. Options:

- **Timer-based:** GUI sets a short timer (e.g., 500ms after spawn) to reclaim
  focus. Simple but fragile — the profile server may not have started yet, or
  may have already started long before.
- **XPC-triggered:** When the profile server connects to the GUI via the
  anonymous XPC endpoint (step 4 in the current topology), the GUI reclaims
  focus in the connection handler. This is precise — focus is reclaimed at
  exactly the right moment.

#### Changes

1. **In `webview_xpc.rs`** (GUI-side XPC handler): When the profile server's
   XPC connection is established, call `[NSApp activateIgnoringOtherApps:YES]`
   to reclaim focus
2. **No changes to `termsurf-profile`** — keep the winit window as-is (Exp 9
   configuration for vsync)
3. **Revert Exp 15 code changes** in `termsurf-profile` — remove the swizzle,
   remove the Accessory policy, restore clean Exp 9 state

#### Implementation

In `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`, in the XPC connection
handler:

```rust
// When profile server connects, reclaim focus
unsafe {
    let app: id = msg_send![class!(NSApplication), sharedApplication];
    let _: () = msg_send![app, activateIgnoringOtherApps:YES];
}
```

#### Expected Outcome

| Result                              | Meaning                                                  |
| ----------------------------------- | -------------------------------------------------------- |
| Brief flicker, then focus returns   | Fix works. The focus steal is imperceptible to the user.  |
| No flicker at all                   | Even better — the reclaim happens before the user notices. |
| Focus doesn't return                | The timing is wrong. May need a longer delay or different trigger point. |
| Performance regression              | Unlikely — no changes to the profile server's event loop. |

#### Why This Should Work

This is how most macOS apps handle subprocess focus issues. Apps like Spotlight,
Alfred, and iTerm2 all manage focus across multiple processes. The standard
pattern is: let the subprocess do what it needs to do, then the main app
reclaims focus when it's ready.

The XPC connection event is the ideal trigger because:
- It fires after the profile server has fully initialized (window created, CEF
  running)
- It fires in the GUI process on a dispatch queue thread
- The GUI already has an `NSApplication` instance (WezTerm is a GUI app)
- `activateIgnoringOtherApps:` is designed for exactly this case

#### Notes

- This is the first experiment that modifies the **GUI process** rather than
  the profile server
- The profile server keeps its winit window for vsync — no performance risk
- The focus reclaim should happen within milliseconds of the focus steal,
  making it imperceptible
- If the flicker is noticeable, a follow-up experiment could add a small delay
  before the profile server creates its winit window, giving the GUI time to
  set up a focus-reclaim callback first

#### Results

**Focus:** SUCCESS — The GUI retained focus. `activateIgnoringOtherApps:` fired
on XPC connection and the user never lost keyboard focus.

**Performance:** SEVERE REGRESSION — Frame rate dropped back to ~30fps baseline.

**Overall stats:** 721 frames over 33.9s = **21.3 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 38    | 5%         |
| 60fps (6-20ms)       | 148   | **20%**    |
| 30fps (21-40ms)      | 316   | **43%**    |
| Mid (41-70ms)        | 110   | 15%        |
| Low (>70ms)          | 108   | 15%        |

**Dominant intervals:** 33ms (131), 34ms (98) — back to 30fps, identical to the
pre-hidden-window experiments.

**Max consecutive 60fps frames:** 16

#### Comparison Across All Experiments

| Metric                | Exp 4 (baseline) | Exp 9 (winit window) | **Exp 16 (GUI reclaim)** |
| --------------------- | ---------------- | -------------------- | ------------------------ |
| Average FPS           | 22.0             | 22.3                 | **21.3**                 |
| Frames at ~60fps      | 52%              | 61%                  | **20%**                  |
| Frames at ~30fps      | 19%              | 6%                   | **43%**                  |
| Max consecutive 60fps | 5                | 35                   | **16**                   |
| Dominant interval     | scattered        | 16-17ms              | **33-34ms**              |

#### Conclusion

**FAILED.** Focus reclaim works, but the `activateIgnoringOtherApps:` call
deactivated the profile server process, destroying the vsync signal that the
hidden window was providing.

The root cause is a timing problem: the GUI's XPC connection handler fires
**before** the profile server creates its winit window. The profile server's
startup sequence is: (1) connect to GUI via XPC → *GUI reclaims focus here* →
(2) initialize CEF → (3) create winit window. The window is created while the
process is already deactivated, so it never establishes a proper display link.

More fundamentally, this reveals that the hidden window approach has two
unsolvable problems:

1. **Focus stealing:** Creating any window activates the process
2. **Vsync dependency on activation:** The vsync signal requires the process to
   be (or have been) the active app when the window is created

These problems are in direct conflict. The hidden 1×1 window was always a hack
— it works only when the profile server is allowed to steal focus, which is
unacceptable in production.

**The hidden window approach is abandoned.** Future experiments should explore
fundamentally different approaches to achieving 60fps without requiring a window
in the profile server process.

### Experiment 17: External Begin Frame at 60Hz

**Status:** FAILED — Worst results of any experiment

**Goal:** Achieve flawless 60fps by taking direct control of CEF's frame
production using `send_external_begin_frame`, driven by a high-resolution timer.
No hidden windows, no vsync dependency, no hacks.

**Problem:** All previous experiments tried to coax CEF's internal compositor
into producing frames at 60fps by manipulating the process environment (windows,
event loops, activation policies). This approach is fundamentally flawed — we
were fighting CEF's internal scheduling instead of controlling it.

CEF provides an explicit API for exactly this case: **external begin frame**.
When enabled, CEF's compositor stops deciding when to render and instead waits
for the embedder to call `send_external_begin_frame()`. The embedder becomes
the frame clock.

**Hypothesis:** If we call `send_external_begin_frame()` at exactly 60Hz using
a high-resolution timer, CEF will produce exactly one frame per call, delivered
via `on_accelerated_paint`. No window needed — the timer IS the frame clock.

#### How It Works

The standard CEF rendering cycle:

```
CEF decides when to render → compositor runs → on_accelerated_paint fires
```

With external begin frame:

```
Timer fires at 60Hz → send_external_begin_frame() → compositor runs → on_accelerated_paint fires
```

We replace CEF's internal frame scheduling with our own timer. CEF still does
all the rendering work — we just tell it *when*.

#### API Details

The cef-rs bindings fully expose this feature:

- **`WindowInfo.external_begin_frame_enabled`** — Set to `1` when creating the
  browser. Tells CEF to wait for external begin frame signals instead of using
  its internal compositor timing.
- **`browser.get_host().send_external_begin_frame()`** — Triggers one compositor
  cycle. CEF renders the current page state and delivers the result via
  `on_accelerated_paint`. Takes no arguments.

The cef-rs OSR example explicitly disables this (`external_begin_frame_enabled:
false as _`) because it uses winit's event loop for timing instead.

#### Changes

1. **Remove winit** from `termsurf-profile` — no more hidden window, no event
   loop dependency
2. **Set `external_begin_frame_enabled: 1`** in `WindowInfo` when creating
   browsers
3. **Create a high-resolution timer** at 16.67ms (60Hz) using
   `mach_absolute_time` + `mach_wait_until` (sub-millisecond precision on macOS)
4. **In the timer loop**, call `send_external_begin_frame()` on every browser
   in the profile, then call `do_message_loop_work()`
5. **Keep `external_message_pump: 1`** in CEF settings

#### Implementation

```rust
// WindowInfo change
let window_info = WindowInfo {
    windowless_rendering_enabled: 1,
    shared_texture_enabled: 1,
    external_begin_frame_enabled: 1,  // NEW: we control frame timing
    ..Default::default()
};

// Main loop: high-resolution 60Hz timer
let frame_interval_ns: u64 = 16_666_667; // 16.67ms in nanoseconds
let mut next_frame_time = mach_absolute_time();

loop {
    if QUIT_FLAG.load(Ordering::Relaxed) { break; }

    // Wait until next frame time with sub-ms precision
    mach_wait_until(next_frame_time);
    next_frame_time += frame_interval_ns * timebase_ratio;

    // Tell every browser in this profile to render a frame
    for browser in &browsers {
        browser.get_host().send_external_begin_frame();
    }

    // Process CEF messages
    cef::do_message_loop_work();
}
```

Note: `mach_absolute_time` returns ticks in a hardware-specific timebase.
The conversion ratio from nanoseconds to ticks is obtained from
`mach_timebase_info`.

#### Why This Should Work

This approach is fundamentally different from all 16 previous experiments:

| Previous experiments (1-16)         | Experiment 17                     |
| ----------------------------------- | --------------------------------- |
| CEF decides when to render          | **We** decide when to render      |
| Tried to influence CEF's scheduling | Replaced CEF's scheduling         |
| Needed vsync signal from OS         | Timer provides the signal         |
| Required window/event loop          | No window, no event loop          |
| Inconsistent frame timing           | Deterministic frame timing        |

CEF's `external_begin_frame` is designed for embedders who have their own
rendering pipeline and need CEF to synchronize with it. This is exactly our
case — the GUI process has its own wgpu rendering loop, and we need CEF to
produce frames on our schedule, not its own.

#### Expected Outcome

| Result                     | Meaning                                                      |
| -------------------------- | ------------------------------------------------------------ |
| ~100% at 60fps             | Perfect solution. No window, no vsync, deterministic frames. |
| 60fps but frames are empty | CEF needs `do_message_loop_work` called more frequently or with different timing relative to `send_external_begin_frame`. |
| No frames at all           | The binding may need additional setup (e.g., frame time argument). Check CEF source. |
| Still ~30fps               | CEF's compositor has additional internal throttling that `external_begin_frame` doesn't override. Investigate `windowless_frame_rate` interaction. |

#### Baseline (Pre-Exp 17, Hidden Window with Focus Stealing)

Before implementing Experiment 17, the code was reverted to the Exp 9
configuration (hidden winit window, `external_message_pump`, focus stealing).
This establishes the performance target to beat.

**Overall stats:** 660 frames over 27.0s = **24.5 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 18    | 2%         |
| 60fps (6-20ms)       | 463   | **70%**    |
| 30fps (21-40ms)      | 38    | 5%         |
| Mid (41-70ms)        | 61    | 9%         |
| Low (>70ms)          | 79    | 11%        |

**Dominant intervals:** 17ms (195), 16ms (120) — strong vsync-aligned peak.

**Max consecutive 60fps frames:** 40

This is the best achievable with the hidden window approach, but it requires
focus stealing. Experiment 17 must match or exceed 70% at 60fps without any
window.

#### Why This Wasn't Tried Earlier

We assumed CEF needed environmental signals (vsync, display link, window server
notifications) to produce frames. Experiments 1-16 tried to provide those
signals. But the real answer was always available — CEF has an API to bypass its
internal scheduling entirely. We were so focused on what the process *environment*
needed that we missed what the CEF *API* offered.

#### Notes

- `windowless_frame_rate: 60` in `BrowserSettings` may be irrelevant when using
  external begin frame (CEF doesn't decide timing). Keep it as a safety net.
- If a page hasn't changed, `on_accelerated_paint` may not fire (CEF may
  optimize away no-op frames). This is fine — the GUI keeps displaying the last
  IOSurface.
- The timer runs independently of page content. A static page still gets 60
  begin-frame calls per second. This is intentionally wasteful for now —
  efficiency can be optimized later.
- No `cocoa`, `objc`, or `winit` dependencies needed. Pure Rust + CEF.
- No focus stealing possible — no window is created.

#### Results

**Overall stats:** 218 frames over 14.8s = **14.8 fps average**

| Interval             | Count | Percentage |
| -------------------- | ----- | ---------- |
| Burst (0-5ms)        | 4     | 1%         |
| 60fps (6-20ms)       | 23    | **10%**    |
| 30fps (21-40ms)      | 46    | 21%        |
| Mid (41-70ms)        | 68    | 31%        |
| Low (>70ms)          | 76    | **35%**    |

**Dominant intervals:** None — completely scattered (23ms, 62ms, 44ms, 65ms,
41ms). No vsync-aligned peak, no consistent cadence at all.

**Max consecutive 60fps frames:** 2

#### Comparison

| Metric                | Baseline (Exp 9) | **Exp 17**  |
| --------------------- | ---------------- | ----------- |
| Average FPS           | 24.5             | **14.8**    |
| Frames at ~60fps      | 70%              | **10%**     |
| Low (>70ms)           | 11%              | **35%**     |
| Max consecutive 60fps | 40               | **2**       |
| Dominant interval     | 16-17ms          | **none**    |

#### Conclusion

**FAILED.** `send_external_begin_frame()` does not work as expected. Despite
being called at a precise 60Hz by a high-resolution timer, CEF produced frames
at only 14.8fps — worse than every prior experiment including the original
sleep loop (Exp 2: 17fps).

The frame intervals are completely scattered with no pattern, suggesting
`send_external_begin_frame()` does not directly trigger frame production. The
call likely posts an internal task to CEF's compositor, which then needs
`do_message_loop_work()` to process it. But the compositor may require multiple
message loop iterations to complete a frame, meaning a single
begin-frame + message-loop-work per tick is insufficient.

The `external_begin_frame` API appears to be designed for a different use case
than ours — likely for embedders with their own compositor that need to
synchronize CEF's rendering with an existing frame pipeline, not for headless
processes trying to drive frame production from scratch.

### Experiment 18: Revert to Pre-Performance-Work Baseline

**Status:** COMPLETE

**Goal:** Revert the profile server code to its original state before any
performance experiments (Issue 338 onward), establishing a clean foundation for
a new line of investigation.

**Rationale:** Seventeen experiments have been conducted since Issue 338. The
changes accumulated in the profile server are:

1. `external_message_pump: 1` added to CEF settings (Exp 4)
2. `winit` dependency and hidden 1×1 window event loop (Exp 7/9)
3. Various comment updates

None of these changes produced an acceptable solution:
- The hidden window achieves 60-70% at 60fps but **steals focus** (Exp 7/9)
- Every attempt to fix focus stealing either broke vsync (Exp 11-14), broke
  CEF (Exp 15), or caused a performance regression (Exp 16)
- `external_begin_frame` was worse than the original code (Exp 17)

The original code before performance work (commit `d3fc03161`) was:

```rust
// CEF settings — no external_message_pump
let settings = cef::Settings {
    windowless_rendering_enabled: 1,
    no_sandbox: 1,
    root_cache_path: ...,
    browser_subprocess_path: ...,
    persist_session_cookies: 1,
    ..Default::default()
};

// Simple polling loop — no winit, no window, no event loop
while !QUIT_FLAG.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}
```

This ran at ~17-20fps, which prompted the investigation. Reverting to this
state provides a clean baseline for future experiments that take a different
approach entirely.

#### Changes

1. **Remove `winit`** from `Cargo.toml`
2. **Remove `external_message_pump: 1`** from CEF settings
3. **Replace the winit event loop** with the original simple polling loop
4. **Restore original comments** on the quit flag and polling loop

#### What This Preserves

All non-performance changes made during Issues 325-337 remain intact:
- Ctrl+C graceful shutdown via `QUIT_FLAG`
- XPC communication and Mach port transfer
- Multi-browser support
- Resize handling
- Input forwarding
- All GUI-side code unchanged

Only the message loop implementation and CEF settings revert.

#### Results

314 frames in 11.0 seconds = **28.5 fps** average.

| Interval Bucket | Count | Percentage | Meaning |
|-----------------|-------|------------|---------|
| 0-5ms (duplicates) | 37 | 12% | Same-timestamp double frames |
| 14-20ms (60fps) | 126 | 40% | Good frames |
| 30-36ms (30fps) | 72 | 23% | Half-rate frames |
| >50ms (drops) | 56 | 18% | Missed frames / gaps |

Max consecutive 60fps streak: **11 frames**.

The simple polling loop produces a bimodal pattern — frames alternate between
16ms (60fps) and 33ms (30fps) intervals, with frequent ~80ms drops. Only 40% of
frames land at 60fps and the longest streak is just 11.

For comparison, the Experiment 3 baseline (with `external_message_pump` and the
hidden window) achieved 70% at 60fps with streaks of 40. The
`external_message_pump` setting was genuinely helping — but it required a window
(NSApplication/winit) to drive the vsync signal, which is the approach that was
rejected.

#### Conclusion

The revert is complete. This is the clean pre-performance baseline: a simple
`sleep(1ms)` + `do_message_loop_work()` polling loop running at ~28fps with
unstable timing. Any future experiments start from this foundation.

## Summary of All Experiments

| # | Experiment | Result | Key Metric |
|---|-----------|--------|------------|
| 1 | Document Process Architecture | PARTIAL | Identified extra IPC hop, but hop wasn't the bottleneck |
| 2 | Add Event Loop to Profile Server | FAILED | 17fps — winit event loop alone didn't help |
| 3 | Measure cef-rs Example Frame Rate | SUCCESS | 60.9fps — proved CEF can deliver 60fps |
| 4 | Enable `external_message_pump` | PARTIAL | 22fps, 52% at 60fps — config mismatch was real |
| 5 | `NSApplicationActivationPolicyRegular` | FAILED | 19fps — App Nap not the cause |
| 6 | Use `run_message_loop()` | FAILED | 18fps — CEF's own loop was worse |
| 7 | Add Hidden 1x1 Window | SUCCESS | 78% at 60fps, streak of 57 — window provides vsync |
| 8 | Replace Window with CVDisplayLink | FAILED | 30% at 60fps — bare display link insufficient |
| 9 | Restore Hidden Window Baseline | SUCCESS | 61% at 60fps — confirmed window is the key |
| 10 | `NSApplicationActivationPolicyAccessory` | FAILED | Focus still stolen — winit calls `makeKeyAndOrderFront:` |
| 11 | Native NSWindow with `canBecomeKey: NO` | PARTIAL | 20.7fps — no focus steal, but lost vsync |
| 12 | `orderFront` instead of `orderBack` | PARTIAL | 34% at 60fps — still no vsync recovery |
| 13 | Add Layer-Backed Content View | FAILED | 34-36% at 60fps — CALayer didn't restore vsync |
| 14 | NSApplication Event Pumping | FAILED | 34-36% at 60fps — NSApp pumping couldn't replicate winit |
| 15 | Swizzle `canBecomeKey` on Winit's Window | FAILED | Broke CEF — global swizzle hit CEF's internal windows |
| 16 | GUI-Side Focus Reclaim | PARTIAL | Focus reclaimed, but vsync destroyed (20% at 60fps) |
| 17 | External Begin Frame at 60Hz | FAILED | 14.8fps — worst result, API doesn't work as expected |
| 18 | Revert to Baseline | COMPLETE | 28.5fps, 40% at 60fps — clean foundation restored |

## Conclusions

### What We Know

1. **CEF can deliver 60fps.** The cef-rs OSR example sustains 60.9fps with
   consistent 16-17ms intervals (Exp 3). The problem is not CEF itself.

2. **A hidden window provides vsync.** Creating a 1x1 window gives the process a
   CVDisplayLink via the window server's compositing pipeline, boosting
   performance from ~20fps to 60-78% at 60fps (Exp 7, 9). This is the only
   approach that achieved near-60fps in 18 experiments.

3. **The hidden window is unacceptable.** It steals focus from the GUI. Every
   attempt to fix focus stealing either destroyed the vsync signal (Exp 11-14,
   16), broke CEF entirely (Exp 15), or had no effect (Exp 10). Focus and vsync
   are fundamentally coupled through the window — you cannot have one without the
   other.

4. **`external_message_pump` helps but isn't enough.** Enabling it raised
   performance from 17fps to 22fps (Exp 4), but consistent 60fps still requires
   a display connection.

5. **CEF's frame scheduling APIs don't work as expected.**
   `send_external_begin_frame()` produced the worst results of any experiment at
   14.8fps (Exp 17). The API does not directly trigger frame production.

6. **The bottleneck is the process environment, not the code.** No amount of
   polling strategy, event loop design, or timer precision can compensate for the
   lack of a window server connection. A headless process on macOS does not
   receive vsync callbacks.

### What Didn't Work

- **Event loop variations** (Exp 2, 6, 14) — polling, `run_message_loop()`, and
  NSApp event pumping all failed equally.
- **Activation policies** (Exp 5, 10) — neither Regular nor Accessory policy
  affected frame timing or focus behavior.
- **Native NSWindow without winit** (Exp 11-13) — creating a raw NSWindow with
  `canBecomeKey: NO` solved focus but lost the vsync signal. Adding a CALayer
  didn't help.
- **Global method swizzling** (Exp 15) — broke CEF's internal windows.
- **Focus reclaim from GUI** (Exp 16) — deactivating the profile server before
  window creation destroyed the vsync signal.
- **External frame timing** (Exp 17) — `send_external_begin_frame()` at 60Hz
  was worse than doing nothing.

### Current Baseline

The profile server is reverted to the pre-performance state: a simple
`sleep(1ms)` + `do_message_loop_work()` polling loop producing ~28.5fps with
bimodal 16/33ms intervals. 40% of frames land at 60fps with a max streak of 11.

### Next Steps

Explore every option to increase frame rate **without launching a hidden
window**. Potential directions:

1. **CADisplayLink (macOS 14+)** — Apple's newer display link API that may work
   without a window, unlike the older CVDisplayLink.
2. **IOSurface frame pacing** — Attach a
   `CFRunLoopObserver`/`CADisplayLink`-style callback on the GUI side and drive
   frame requests from there back to the profile server.
3. **CEF `run_message_loop()` with `OnScheduleMessagePumpWork`** — Let CEF
   manage its own scheduling but intercept the pump work callback to integrate
   with a high-resolution timer.
4. **Move CEF rendering into the GUI process** — Eliminate the separate profile
   server for rendering (requires solving the one-profile-per-process constraint
   differently).
5. **Shared memory / double-buffering** — Decouple frame production from frame
   display so the GUI renders at 60fps independently of CEF's frame rate.
6. **Investigate winit's magic** — Deep-dive into exactly what winit does during
   window creation that provides the vsync signal, and replicate only that
   specific mechanism.

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  investigation
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Research
  that led to this hypothesis
