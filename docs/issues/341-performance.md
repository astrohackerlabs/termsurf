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

## Experiments

### Experiment 1: Add Event Loop to Profile Server

**Status:** Not started

**Hypothesis:** Adding a minimal event loop (even headless) to the profile
server will increase CEF's frame production rate.

**Approach:**

1. Add winit as a dependency to termsurf-profile
2. Create a headless event loop (no visible window)
3. Integrate `do_message_loop_work()` with winit's `pump_app_events()`
4. Measure frame rate

**Success criteria:** Frame production increases from ~20fps toward 60fps.

### Experiment 2: Test with Visible Window

**Status:** Not started

**Hypothesis:** If the profile server creates a visible (but hidden/offscreen)
window, CEF's compositor will behave normally.

**Approach:**

1. Create a 1x1 pixel window in the profile server
2. Hide it or move it offscreen
3. Measure frame rate

**Success criteria:** Frame production matches the cef-rs OSR example (~60fps).

### Experiment 3: Profile CEF's Internal Timing

**Status:** Not started

**Hypothesis:** CEF has internal throttling that activates in windowless mode.

**Approach:**

1. Add timing instrumentation to `on_accelerated_paint`
2. Log intervals between CEF's internal render calls
3. Compare against cef-rs example timing
4. Identify where CEF decides to skip/delay frames

**Success criteria:** Identify the specific CEF behavior causing throttling.

## Related Issues

- [Issue 338: Browser lag investigation](./338-lag.md) — Original performance
  investigation
- [Issue 340: Architecture reconsideration](./340-architecture.md) — Research
  that led to this hypothesis
