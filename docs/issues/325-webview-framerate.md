# 325: Webview Frame Rate

Webview content does not refresh at 60fps, causing visible lag.

## Status

Experiment 2 complete. CEF confirmed as bottleneck — producing ~12-20fps instead
of 60fps. XPC and WezTerm rendering are not the issue.

## Product Requirements

Webview rendering should match native browser performance:

1. **60fps rendering** — Webview content should update at display refresh rate
   (60fps on standard displays, higher on ProMotion).

2. **Smooth scrolling** — Scrolling through web content should feel as smooth as
   Chrome or Safari.

3. **Responsive hover** — Hover effects (link highlights, button states) should
   appear immediately.

4. **Smooth selections** — Drag selections should highlight text in real-time
   without visible lag.

5. **Responsive typing** — Text input in web forms should feel instant.

## Background

### Observed Symptoms

All webview interactions feel slightly laggy compared to native Chrome:

- Scrolling is choppy, not smooth
- Mouse hover effects are delayed
- Text selection highlighting lags behind cursor
- Typing in form fields has noticeable latency

The display refreshes at 60fps and Chrome looks perfectly smooth. The webview
does not.

### Current Architecture

```
CEF renders frame
    │
    ▼
on_accelerated_paint(IOSurface handle)
    │
    ├─ Create Mach port from IOSurface
    │
    └─ Send XPC: { action: "display_surface", port, width, height }
            │
            ▼
        GUI receives message
            │
            ├─ Store surface info
            │
            └─ Call invalidate callback → Window repaints
```

Note: Dedup logic was removed in Experiment 1. Every frame now triggers XPC.

### The Deduplication Problem

The profile server has this logic in `on_accelerated_paint`:

```rust
// Dedup: only send when IOSurface handle changes.
// CEF calls on_accelerated_paint every frame (cursor blinks, etc.)
// but reuses the same IOSurface buffer. We only need to send a new
// Mach port when the buffer changes (double-buffering swap).
let handle = info.shared_texture_io_surface as *mut c_void;
let prev = self.inner.state.last_handle.swap(handle, Ordering::Relaxed);
if handle == prev {
    return;  // <-- BLOCKS ALL SUBSEQUENT FRAMES WITH SAME HANDLE
}
```

**The assumption was wrong.** The comment says "We only need to send a new Mach
port when the buffer changes" — but the GUI also needs to know when the buffer
_content_ changes, even if the handle stays the same.

### Why This Causes Lag

1. CEF renders frame 1 to IOSurface A
2. Profile sends Mach port for A → GUI imports and displays
3. CEF renders frame 2 to IOSurface A (same surface, new content)
4. Profile sees `handle == prev` → **returns early, sends nothing**
5. GUI doesn't know there's new content → **doesn't repaint**
6. User sees stale frame until something else triggers a repaint

The Metal texture IS backed by the IOSurface and DOES see the new content. But
the GUI doesn't know to repaint because the invalidate callback is never called.

### CEF Frame Rate Setting

CEF is configured for 60fps:

```rust
// termsurf-profile/src/main.rs
BrowserSettings {
    windowless_frame_rate: 60,
    ...
}
```

So CEF is producing 60fps, but the GUI isn't displaying them.

### WezTerm's Render Model

WezTerm renders on-demand, not continuously:

- Terminal output triggers repaint
- Cursor blink triggers repaint
- Window resize triggers repaint
- Invalidate callback triggers repaint

Without explicit invalidation, the window just sits with stale content.

## Hypothesis

**Primary hypothesis:** The deduplication logic prevents frame update
notifications from reaching the GUI. Sending a notification on every
`on_accelerated_paint` call will restore 60fps rendering.

**Secondary consideration:** Even without dedup, XPC message latency or GUI
processing time might limit frame rate. May need to measure actual throughput.

## Implementation Approach

### Option A: Lightweight "frame_ready" Notification

Keep Mach port dedup (don't re-send same port), but add a separate notification:

```rust
fn on_accelerated_paint(...) {
    // Always notify GUI that new content is available
    let msg = XpcDictionary::new();
    msg.set_string("action", "frame_ready");
    self.inner.state.gui.send(&msg);

    // Only send new Mach port if handle changed
    let prev = self.inner.state.last_handle.swap(handle, Ordering::Relaxed);
    if handle != prev {
        // Send full display_surface message with Mach port
        ...
    }
}
```

GUI handles `frame_ready` by calling invalidate callback.

**Pros:** Minimal XPC traffic (small message vs full surface info) **Cons:** Two
message types to handle

### Option B: Remove Deduplication Entirely

Send `display_surface` on every `on_accelerated_paint`:

```rust
fn on_accelerated_paint(...) {
    let port = create_mach_port(handle);
    let msg = XpcDictionary::new();
    msg.set_string("action", "display_surface");
    msg.set_mach_send("iosurface_port", port);
    // ... send full message every frame
}
```

**Pros:** Simpler, single code path **Cons:** More XPC traffic, repeated Mach
port creation

### Option C: GUI Continuous Invalidation

GUI polls at 60fps when webview overlays exist:

```rust
// In render loop or timer
if has_webview_overlays() {
    window.invalidate();
    schedule_next_frame(16ms);
}
```

**Pros:** No profile server changes needed **Cons:** Wastes CPU when webview
content is static

### Recommendation

**Start with Option B** — it's the simplest. The dedup was a premature
optimization based on the wrong assumption that CEF does double-buffering. In
reality, CEF renders new content to the same IOSurface repeatedly. Removing the
dedup entirely is the right fix. If XPC throughput becomes an issue, we can
explore Option A (lightweight notification) as an optimization.

## Success Criteria

- [ ] Scrolling feels as smooth as Chrome
- [ ] Hover effects appear immediately
- [ ] Text selection highlights in real-time
- [ ] Typing in form fields feels instant
- [ ] Log shows ~60 invalidate callbacks per second during activity

## Diagnostic Steps

Before implementing, verify the hypothesis with logging:

```bash
# Add logging to on_accelerated_paint
println!("[PAINT] frame, handle={:?}, same_as_prev={}", handle, handle == prev);

# Check how often CEF is painting vs how often GUI is invalidating
tail -f /tmp/termsurf-profile-*.log | grep PAINT
tail -f /tmp/termsurf-gui.log | grep invalidate
```

Expected: Many PAINT logs, few invalidate logs (confirming dedup is blocking).

## Experiments

### Experiment 1: Remove Deduplication (Option B)

**Goal:** Verify that removing the dedup logic restores 60fps rendering.

**Approach:** Remove the early return when handle matches previous. Send
`display_surface` on every `on_accelerated_paint` call.

**Changes:**

1. **`ts3/termsurf-profile/src/main.rs`** — In `on_accelerated_paint`, remove
   the dedup check:

   Before:
   ```rust
   let handle = info.shared_texture_io_surface as *mut c_void;
   let prev = self.inner.state.last_handle.swap(handle, Ordering::Relaxed);
   if handle == prev {
       return;
   }
   ```

   After:
   ```rust
   let handle = info.shared_texture_io_surface as *mut c_void;
   // Send every frame — GUI needs to know when content changes,
   // not just when the IOSurface handle changes.
   ```

   Also remove the `last_handle` field from the state struct since it's no
   longer needed.

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Scrolling
web google.com
# Search for something, scroll results
# Expected: Smooth scrolling like Chrome

# Test 2: Hover effects
# Hover over links
# Expected: Immediate highlight, no delay

# Test 3: Text selection
# Click and drag to select text
# Expected: Real-time highlight following cursor

# Test 4: Typing
# Click a text input, type
# Expected: Characters appear instantly
```

**Status:** Partial success.

**Result:** Dedup removed, code kept. However, lag persists — the dedup was not
the primary cause of the frame rate issue.

**Conclusion:** The dedup logic was based on a wrong assumption (that CEF does
double-buffering), so removing it is correct. But the lag has another root
cause. Keeping this change because:

1. It's semantically correct — GUI should be notified on every frame
2. It eliminates one potential source of dropped frames
3. The code is simpler without the dedup state tracking

### Remaining Hypotheses

The lag likely comes from one of these sources:

1. **XPC message latency** — Each `display_surface` message goes through XPC. At
   60fps, messages arrive every 16.7ms. If XPC adds even 5-10ms latency, frames
   could pile up or be dropped.

2. **WezTerm invalidate → render delay** — Calling `window.invalidate()` doesn't
   immediately repaint. WezTerm batches repaints and may not render until the
   next vsync or event loop tick. The delay between invalidate and actual render
   could be 1-2 frames.

3. **Metal texture import overhead** — Each frame requires
   `IOSurfaceLookupFromMachPort` and creating a new wgpu texture. This may be
   expensive if done 60 times per second.

4. **GUI event loop starvation** — If the GUI is busy processing other events
   (terminal output, input handling), XPC messages may queue up.

5. **Double-buffering mismatch** — CEF may be rendering ahead of what the GUI
   displays, causing a perception of lag even if frames arrive on time.

### Next Steps

To find the real bottleneck, add timestamp logging at each stage:

```rust
// Profile server (on_accelerated_paint)
println!("[FRAME-TX] t={:?}", Instant::now());

// GUI (display_surface handler)
println!("[FRAME-RX] t={:?}", Instant::now());

// GUI (invalidate callback)
println!("[INVALIDATE] t={:?}", Instant::now());

// GUI (actual render)
println!("[RENDER] t={:?}", Instant::now());
```

Compare timestamps to see where time is lost.

### Experiment 2: Diagnostic Logging

**Goal:** Identify where time is lost in the frame pipeline by adding timestamps
at each stage.

**Approach:** Add frame counter and timestamp logging to measure latency between
each stage of the pipeline:

```
CEF paint → XPC send → XPC receive → invalidate → render
```

**Changes:**

1. **`ts3/termsurf-profile/src/main.rs`** — Add frame counter and timestamp in
   `on_accelerated_paint`:

   ```rust
   use std::sync::atomic::{AtomicU64, Ordering};
   use std::time::Instant;

   static FRAME_COUNTER: AtomicU64 = AtomicU64::new(0);
   static START_TIME: OnceLock<Instant> = OnceLock::new();

   // In on_accelerated_paint, before sending XPC:
   let frame_id = FRAME_COUNTER.fetch_add(1, Ordering::Relaxed);
   let start = *START_TIME.get_or_init(Instant::now);
   let elapsed_ms = start.elapsed().as_millis();
   println!("[FRAME-TX] frame={} t={}ms", frame_id, elapsed_ms);

   // Add frame_id to XPC message:
   msg.set_i64("frame_id", frame_id as i64);
   msg.set_i64("tx_time_ms", elapsed_ms as i64);
   ```

2. **`ts3/wezterm-gui/src/termwindow/webview_xpc.rs`** — Log receipt time in
   `display_surface` handler:

   ```rust
   // In handle_xpc_message, display_surface case:
   let frame_id = msg.get_i64("frame_id");
   let tx_time = msg.get_i64("tx_time_ms");
   let rx_time = /* current time since GUI start */;
   println!("[FRAME-RX] frame={} tx={}ms rx={}ms delta={}ms",
            frame_id, tx_time, rx_time, rx_time - tx_time);
   ```

   Note: TX and RX times use different clocks (profile vs GUI process), so
   `delta` is only meaningful if both started at similar times. The key metric
   is the _pattern_ — are deltas consistent or do they grow?

3. **`ts3/wezterm-gui/src/termwindow/webview_xpc.rs`** — Log invalidate call:

   ```rust
   // When calling invalidate callback:
   println!("[INVALIDATE] frame={} t={}ms", frame_id, rx_time);
   ```

4. **`ts3/wezterm-gui/src/termwindow/render/draw.rs`** — Log render time in
   `render_webview_overlays_webgpu`:

   ```rust
   // At start of webview render:
   println!("[RENDER] t={}ms (webview overlay rendering)", elapsed_ms);
   ```

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# In one terminal, watch profile logs:
tail -f /tmp/termsurf-profile-*.log | grep FRAME-TX

# In another terminal, watch GUI logs:
tail -f /tmp/termsurf-gui.log | grep -E "(FRAME-RX|INVALIDATE|RENDER)"

# Then interact with webview:
web google.com
# Scroll, hover, click around

# Look for:
# 1. Frame rate: How many FRAME-TX per second? Should be ~60.
# 2. XPC latency: delta between TX and RX times
# 3. Invalidate gap: time between RX and INVALIDATE (should be ~0)
# 4. Render gap: time between INVALIDATE and RENDER
# 5. Dropped frames: frame_ids that appear in TX but not RX
```

**Expected Findings:**

| Symptom                   | Likely Cause                       |
| ------------------------- | ---------------------------------- |
| TX rate < 60fps           | CEF not rendering fast enough      |
| Large TX→RX delta         | XPC message latency                |
| RX without INVALIDATE     | Invalidate callback not firing     |
| INVALIDATE without RENDER | WezTerm batching/dropping repaints |
| Growing delta over time   | Backpressure / queue buildup       |

**Status:** Success.

**Result:** Diagnostic logging confirmed that the bottleneck is CEF itself, not
XPC or WezTerm rendering.

**Findings:**

| Metric                 | Observed | Expected |
| ---------------------- | -------- | -------- |
| Scroll events received | 57       | —        |
| Frames produced        | 35       | ~57      |
| Frame interval         | 9-588ms  | 16.7ms   |
| Effective FPS          | ~12-20   | 60       |
| XPC latency            | ~10-30ms | —        |
| Invalidate→Render gap  | ~10-30ms | —        |

Key observations:

1. **CEF is the bottleneck** — `on_accelerated_paint` is called at 12-20fps, not
   60fps, despite `windowless_frame_rate: 60` being set.

2. **XPC is fast enough** — Frames arrive at the GUI within milliseconds of
   being sent. No significant queue buildup.

3. **WezTerm renders promptly** — INVALIDATE triggers RENDER within ~10-30ms. No
   frames dropped on the GUI side.

4. **CEF batches input** — 57 scroll events produced only 35 frames. CEF
   combines multiple inputs per render, which is normal, but the render rate
   itself is too low.

**Conclusion:** The lag is caused by CEF's off-screen rendering not hitting
60fps. The `windowless_frame_rate` setting appears to be ignored or overridden.

### ts2 Comparison: Why ts2 Achieves 60fps

Confirmed that ts2 (in-process CEF) does NOT have this problem — it runs at full
60fps. The key difference is **message loop integration**.

**ts2 (60fps) — CFRunLoop Timer Callbacks:**

```rust
// ts2/wezterm-gui/src/cef_integration.rs:79-163

// CEF calls this when it needs work done
fn on_schedule_message_pump_work(&self, delay_ms: i64) {
    schedule_cef_work(delay_ms);  // Creates CFRunLoop timer
}

// Timer fires precisely when CEF needs it
extern "C" fn timer_callback(...) {
    cef::do_message_loop_work();  // Pump work immediately
}
```

ts2 uses `do_message_loop_work()` driven by CFRunLoop timers. CEF tells the GUI
exactly when it needs work pumped via `on_schedule_message_pump_work(delay_ms)`,
and the GUI responds by setting a timer that fires after `delay_ms` and calls
`do_message_loop_work()`.

**ts3 (12-20fps) — Blocking `run_message_loop()`:**

```rust
// ts3/termsurf-profile/src/main.rs:281-285
cef::run_message_loop();  // Blocks in separate process
```

ts3 uses CEF's blocking `run_message_loop()` in a separate process. The GUI has
no control over when work is pumped — CEF runs at its own pace.

**The critical difference:**

| Aspect       | ts2                                      | ts3                           |
| ------------ | ---------------------------------------- | ----------------------------- |
| Message loop | `do_message_loop_work()` on demand       | `run_message_loop()` blocking |
| Scheduling   | `on_schedule_message_pump_work` callback | None (CEF controls timing)    |
| Integration  | CFRunLoop timers, precise timing         | Separate process, no control  |
| Frame rate   | 60fps achieved                           | 12-20fps observed             |

**Root cause:** ts3's `run_message_loop()` is not pumping work fast enough. CEF
internally throttles because nothing is requesting frames at 60fps. The
`windowless_frame_rate: 60` setting only sets the _maximum_ rate — CEF still
needs its message loop pumped frequently to actually render.

**Proposed fix:** Replace `run_message_loop()` with a custom loop using
`do_message_loop_work()`. Either:

1. **Demand-driven:** Implement `on_schedule_message_pump_work` to pump work
   exactly when CEF requests it (like ts2).

2. **Polling:** Call `do_message_loop_work()` in a tight loop with short sleeps
   (simpler but less efficient).

### Next Steps

Investigate CEF's rendering pipeline:

1. **Verify frame rate setting** — Call `host.get_windowless_frame_rate()` to
   confirm CEF actually has 60fps configured internally.

2. **Force invalidate after input** — Call `host.invalidate()` after each scroll
   event to request immediate repaint. This may force CEF to render more
   frequently.

3. **External begin frame mode** — CEF supports `SendExternalBeginFrame` for
   explicit frame scheduling. This gives full control over when frames render
   but requires more integration work.

4. **Message loop investigation** — Check if `run_message_loop()` is the right
   approach. Alternatives:
   - `do_message_loop_work()` in a tighter custom loop
   - Multi-threaded message loop mode
   - External message pump

5. **CEF settings review** — Check for other settings that might affect
   rendering performance:
   - `background_color` (transparency overhead?)
   - `webgl_antialiasing` / other GPU settings
   - Chrome command-line switches for off-screen rendering

6. **Profile CEF internally** — Use Chrome's tracing (`--enable-tracing`) to see
   where time is spent inside CEF's rendering pipeline.

## References

- `ts3/termsurf-profile/src/main.rs` — `on_accelerated_paint` with dedup logic
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC handler, invalidate
  callbacks
- `ts3/wezterm-gui/src/termwindow/render/draw.rs` — Webview texture rendering
- CEF `windowless_frame_rate` setting
- IOSurface/Metal texture sharing architecture
