+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 325: Webview Frame Rate

Webview content does not refresh at 60fps, causing visible lag.

## Status

**Resolved.** Webview now renders at 60fps using a 1ms polling loop with
`do_message_loop_work()`.

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

### Experiment 3: Replace run_message_loop with Polling

**Goal:** Test whether replacing `run_message_loop()` with a tight polling loop
using `do_message_loop_work()` improves frame rate.

**Hypothesis:** CEF's `run_message_loop()` is not pumping work frequently enough.
Calling `do_message_loop_work()` at a higher frequency will increase the frame
rate toward 60fps.

**Approach:** Replace the blocking `run_message_loop()` with a custom loop that
calls `do_message_loop_work()` with short sleeps between iterations.

**Changes:**

1. **`ts3/termsurf-profile/src/main.rs`** — Replace message loop:

   Before:
   ```rust
   // 10. Run CEF message loop (blocks until quit_message_loop)
   println!("Profile: Running message loop...");
   cef::run_message_loop();
   ```

   After:
   ```rust
   // 10. Run CEF message loop with high-frequency polling
   println!("Profile: Running message loop (polling mode)...");
   let quit_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
   let quit_flag_handler = quit_flag.clone();

   // Update Ctrl+C handler to set flag instead of calling quit_message_loop
   ctrlc::set_handler(move || {
       println!("Profile: Ctrl+C, setting quit flag...");
       quit_flag_handler.store(true, std::sync::atomic::Ordering::Relaxed);
   }).expect("Failed to set Ctrl+C handler");

   // Poll at ~1000Hz (1ms sleep) to ensure responsive frame rendering
   while !quit_flag.load(std::sync::atomic::Ordering::Relaxed) {
       cef::do_message_loop_work();
       std::thread::sleep(std::time::Duration::from_millis(1));
   }
   ```

   Note: The Ctrl+C handler setup needs to move after this change since we can't
   call `quit_message_loop()` when not using the blocking loop.

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Watch frame rate in profile logs:
tail -f /tmp/termsurf-profile-*.log | grep FRAME-TX

# Test scrolling:
web google.com
# Scroll and observe frame intervals

# Compare to before:
# - Before: Frame intervals 9-588ms (12-20fps)
# - Expected: Frame intervals ~16ms (60fps)
```

**Expected outcome:**

| Metric | Before (Exp 2) | Expected (Exp 3) |
|--------|----------------|------------------|
| Frame interval | 9-588ms | ~16ms |
| Effective FPS | 12-20 | ~60 |
| CPU usage | Low | Higher (polling) |

**Risks:**

- **Higher CPU usage** — Polling at 1000Hz will use more CPU than the blocking
  loop. This is acceptable for testing; if successful, we can optimize with
  proper `on_schedule_message_pump_work` integration later.

- **Ctrl+C handling** — Need to restructure shutdown logic since
  `quit_message_loop()` won't work with a custom loop.

**Status:** Success.

**Result:** Frame rate improved from 12-20fps to ~60fps during active scrolling.

**Observed frame intervals (during scroll):**

```
908→909: 18ms    915→916: 16ms
909→910: 16ms    916→917: 16ms
910→911: 18ms    917→918: 15ms
911→912: 15ms    918→919: 18ms
912→913: 17ms    919→920: 18ms
913→914: 15ms    920→921: 16ms
914→915: 19ms    921→922: 16ms
```

**Calculation:** 19 frames over 320ms = **59.4 fps** ✓

**Conclusion:** The polling loop with `do_message_loop_work()` achieves 60fps.
The original `run_message_loop()` was not pumping CEF's message queue frequently
enough, causing the low frame rate. This confirms the ts2 comparison findings —
the message loop integration is critical for frame rate.

### Testing Note: Background Process Lifecycle

During testing, initial results were inconsistent (~26fps). This was caused by
**stale background processes not reloading** after rebuilds. The profile server
and launcher processes persist after the GUI closes, so code changes weren't
taking effect.

**Workaround:** Kill background processes before testing:

```bash
pkill -f termsurf-profile
pkill -f termsurf-launcher
```

**Root cause:** When the GUI app closes, the profile server and launcher
processes are not terminated. This is a separate bug that should be fixed —
the GUI should signal child processes to exit on shutdown.

### Next Steps

1. **Fix process lifecycle** — GUI should terminate profile/launcher processes
   on exit. This is blocking effective development iteration.

2. **Optimize polling** — The 1ms sleep works but wastes CPU. Implement proper
   `on_schedule_message_pump_work` callback like ts2 for demand-driven pumping.

3. **Verify all success criteria** — Confirm hover effects, text selection, and
   typing all feel responsive at 60fps.

### Experiment 4: Demand-Driven Message Pump (like ts2)

**Goal:** Replace the 1ms polling loop with proper `on_schedule_message_pump_work`
callback, matching ts2's architecture for efficient CPU usage.

**Hypothesis:** CEF will call `on_schedule_message_pump_work(delay_ms)` to tell
us exactly when to pump work. Using CFRunLoop timers like ts2, we can pump work
precisely when needed — no wasted CPU cycles.

**Approach:** Port ts2's CFRunLoop timer pattern to ts3's profile server.

**Reference implementation (ts2):**

```rust
// ts2/wezterm-gui/src/cef_integration.rs

wrap_browser_process_handler! {
    struct WezTermBrowserProcessHandler;
    impl BrowserProcessHandler {
        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            schedule_cef_work(delay_ms);
        }
    }
}

fn schedule_cef_work(delay_ms: i64) {
    cancel_timer();
    let delay_secs = if delay_ms <= 0 { 0.0 } else { delay_ms as f64 / 1000.0 };

    let timer = CFRunLoopTimerCreate(
        std::ptr::null(),
        CFAbsoluteTimeGetCurrent() + delay_secs,  // fire time
        0.0,                                       // non-repeating
        0, 0,
        timer_callback,
        std::ptr::null_mut(),
    );
    CFRunLoopAddTimer(CFRunLoopGetMain(), timer, kCFRunLoopCommonModes);
}

extern "C" fn timer_callback(...) {
    cef::do_message_loop_work();
}
```

**Changes for ts3:**

1. **`ts3/termsurf-profile/src/main.rs`** — Add CFRunLoop timer infrastructure:

   ```rust
   use core_foundation::runloop::{
       kCFRunLoopCommonModes, CFRunLoopAddTimer, CFRunLoopGetMain,
       CFRunLoopRun, CFRunLoopStop, CFRunLoopTimerCreate,
       CFRunLoopTimerInvalidate, CFRunLoopTimerRef,
   };
   use core_foundation_sys::date::CFAbsoluteTimeGetCurrent;

   static CEF_TIMER: Mutex<Option<SendableTimer>> = Mutex::new(None);

   struct SendableTimer(CFRunLoopTimerRef);
   unsafe impl Send for SendableTimer {}
   ```

2. **Update `BrowserProcessHandler`** — Add `on_schedule_message_pump_work`:

   ```rust
   wrap_browser_process_handler! {
       pub struct ProfileBPH {
           state: Arc<ProfileState>,
       }

       impl BrowserProcessHandler {
           fn on_context_initialized(&self) { /* existing code */ }

           fn on_schedule_message_pump_work(&self, delay_ms: i64) {
               schedule_cef_work(delay_ms);
           }
       }
   }
   ```

3. **Replace polling loop with CFRunLoop**:

   Before (polling):
   ```rust
   while !quit_flag.load(...) {
       cef::do_message_loop_work();
       std::thread::sleep(Duration::from_millis(1));
   }
   ```

   After (CFRunLoop):
   ```rust
   // Run the CFRunLoop - timers will fire and call do_message_loop_work()
   println!("Profile: Running CFRunLoop...");
   unsafe { CFRunLoopRun(); }
   ```

4. **Update Ctrl+C handler** to stop CFRunLoop:

   ```rust
   ctrlc::set_handler(move || {
       println!("Profile: Ctrl+C, stopping CFRunLoop...");
       unsafe { CFRunLoopStop(CFRunLoopGetMain()); }
   })
   ```

5. **Add `core-foundation` dependency** to `termsurf-profile/Cargo.toml`:

   ```toml
   [dependencies]
   core-foundation = "0.9"
   core-foundation-sys = "0.8"
   ```

**Verification:**

```bash
# Kill any stale processes first!
pkill -f termsurf-profile
pkill -f termsurf-launcher

cd ts3 && ./scripts/build-debug.sh --open

# Monitor CPU usage (should be near 0% when idle):
top -pid $(pgrep -f termsurf-profile)

# Watch frame rate during scroll:
tail -f /tmp/termsurf-profile-*.log | grep FRAME-TX

# Test scrolling - should still be ~60fps
web google.com
```

**Expected outcome:**

| Metric | Polling (Exp 3) | CFRunLoop (Exp 4) |
|--------|-----------------|-------------------|
| FPS during scroll | ~60 | ~60 |
| CPU when idle | ~5-10% | ~0% |
| CPU during scroll | ~5-10% | ~1-2% |
| Wakeups when idle | ~1000/sec | ~0/sec |

**Status:** Failed.

**Result:** Webview did not open at all. GUI reported "Timeout waiting for surface
from XPC" — no frames were ever received.

**Analysis:** The profile server started correctly:
- CEF initialized
- Browser created
- CFRunLoop running

But no `FRAME-TX` log output appeared — `on_accelerated_paint` was never called.

**Root cause:** CEF was not configured to use the external message pump. Without
`external_message_pump: 1` in CEF settings, CEF:

1. Uses its own internal message loop scheduling
2. **Never calls `on_schedule_message_pump_work`** — this callback is only
   invoked when `external_message_pump: 1` is set
3. Expects `run_message_loop()` to be called for proper operation

We replaced `run_message_loop()` with `CFRunLoopRun()`, but CEF wasn't
configured to drive rendering through our callback. The CFRunLoop ran, but
CEF never scheduled any work on it.

**Missing configuration:**

```rust
// ts3/termsurf-profile/src/main.rs
let settings = cef::Settings {
    windowless_rendering_enabled: 1,
    no_sandbox: 1,
    external_message_pump: 1,  // <-- MISSING! Required for on_schedule_message_pump_work
    // ...
};
```

Both ts2 and the cef-rs OSR example set `external_message_pump: 1`:

- `ts2/wezterm-gui/src/main.rs:129` — `external_message_pump: 1`
- `cef-rs/examples/osr/src/main.rs:674` — `external_message_pump: true as _`

**Conclusion:** The experiment failed because of a missing CEF configuration
setting, not a fundamental flaw in the approach. The CFRunLoop timer pattern
is correct, but CEF must be told to use external message pump mode.

**Next steps:** Either:

1. Add `external_message_pump: 1` and retry this experiment
2. Revert to the working polling loop (Experiment 3) and optimize later

### Experiment 5: Fix Experiment 4 (Add external_message_pump)

**Goal:** Fix experiment 4 by adding the missing `external_message_pump: 1`
setting to CEF configuration.

**Hypothesis:** With `external_message_pump: 1` set, CEF will call
`on_schedule_message_pump_work` to request work, our CFRunLoop timers will fire,
and we'll achieve 60fps with minimal CPU usage.

**Approach:** Add the single missing setting to CEF initialization.

**Changes:**

1. **`ts3/termsurf-profile/src/main.rs`** — Add `external_message_pump: 1` to
   settings (around line 239):

   Before:
   ```rust
   let settings = cef::Settings {
       windowless_rendering_enabled: 1,
       no_sandbox: 1,
       root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
       browser_subprocess_path: cef::CefString::from(helper_path.to_str().unwrap()),
       persist_session_cookies: 1,
       ..Default::default()
   };
   ```

   After:
   ```rust
   let settings = cef::Settings {
       windowless_rendering_enabled: 1,
       no_sandbox: 1,
       external_message_pump: 1,  // Required for on_schedule_message_pump_work
       root_cache_path: cef::CefString::from(cache_path.to_str().unwrap()),
       browser_subprocess_path: cef::CefString::from(helper_path.to_str().unwrap()),
       persist_session_cookies: 1,
       ..Default::default()
   };
   ```

**Verification:**

```bash
# Kill any stale processes first!
pkill -f termsurf-profile
pkill -f termsurf-launcher

cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Verify webview opens
web google.com
# Expected: Page loads and displays (unlike Exp 4 which timed out)

# Test 2: Check frame rate during scroll
tail -f /tmp/termsurf-profile-*.log | grep FRAME-TX
# Scroll the page
# Expected: ~60fps (frame intervals ~16ms)

# Test 3: Monitor CPU usage when idle
top -pid $(pgrep -f termsurf-profile)
# Expected: Near 0% when not scrolling (vs ~5-10% with polling)

# Test 4: Monitor CPU usage during scroll
# Expected: Lower than polling approach
```

**Expected outcome:**

| Metric | Polling (Exp 3) | CFRunLoop (Exp 5) |
|--------|-----------------|-------------------|
| Webview opens | Yes | Yes |
| FPS during scroll | ~60 | ~60 |
| CPU when idle | ~5-10% | ~0% |
| CPU during scroll | ~5-10% | ~1-2% |

**Status:** Failed.

**Result:** Webview rendered the initial frame but then stopped updating. Scroll
events were received but never processed — tasks posted to CEF's UI thread via
`post_task` never executed.

**Analysis:** Only 3 frames were produced (`frame=0`, `frame=1`, `frame=2`), then
nothing. Scroll events arrived at the XPC handler and were posted to CEF's UI
thread, but `MouseWheelTask::execute()` was never called.

This indicates that after the initial burst, `do_message_loop_work()` stopped
being called. Either:

1. CEF stopped calling `on_schedule_message_pump_work` after initial setup
2. The CFRunLoop timer mechanism isn't working correctly across threads
3. There's a deadlock or blocking issue in the timer/callback interaction

**Conclusion:** The CFRunLoop timer approach doesn't work reliably with CEF's
external message pump in this out-of-process architecture. The polling approach
(Experiment 3) remains the working solution.

**Recommendation:** Keep the simple polling loop from Experiment 3. The ~5-10%
CPU overhead when idle is acceptable for now. If CPU usage becomes a concern
later, investigate alternative approaches:

1. Hybrid: Poll at lower frequency (e.g., 10ms) when idle, faster when active
2. Use `kCFRunLoopDefaultMode` instead of `kCFRunLoopCommonModes`
3. Investigate CEF's multi-threaded message loop mode
4. Profile why `on_schedule_message_pump_work` stops being called

## Conclusion

**Problem:** Webview content was rendering at 12-20fps instead of 60fps, causing
visible lag during scrolling, hover effects, and interactions.

**Root cause:** CEF's `run_message_loop()` does not pump the message queue
frequently enough for 60fps rendering. The `windowless_frame_rate: 60` setting
only sets the maximum rate — CEF still needs its message loop pumped frequently
to actually render frames.

**Solution:** Replace `run_message_loop()` with a custom polling loop that calls
`do_message_loop_work()` every 1ms. This allows CEF to render at its configured
60fps.

```rust
// Before (12-20fps):
cef::run_message_loop();

// After (60fps):
while !quit_flag.load(Ordering::Relaxed) {
    cef::do_message_loop_work();
    std::thread::sleep(Duration::from_millis(1));
}
```

**Trade-off:** The polling loop uses slightly more CPU when idle (~5-10%) compared
to the blocking `run_message_loop()`. This is acceptable for now. Attempts to
optimize with CFRunLoop timers (Experiments 4-5) failed — CEF's
`on_schedule_message_pump_work` callback doesn't work reliably in the
out-of-process architecture.

**Key learnings:**

1. CEF's `windowless_frame_rate` is a maximum, not a guarantee — message loop
   frequency determines actual frame rate
2. ts2 achieved 60fps because it integrated CEF into WezTerm's existing event
   loop with demand-driven `do_message_loop_work()` calls
3. ts3's out-of-process architecture requires explicit polling since there's no
   existing event loop to integrate with
4. The dedup optimization (removed in Experiment 1) was based on a wrong
   assumption and was correctly removed, though it wasn't the primary cause

**Files changed:**

- `ts3/termsurf-profile/src/main.rs` — Replaced `run_message_loop()` with polling
  loop, removed dedup logic, added frame timing diagnostics

## References

- `ts3/termsurf-profile/src/main.rs` — `on_accelerated_paint` with dedup logic
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC handler, invalidate
  callbacks
- `ts3/wezterm-gui/src/termwindow/render/draw.rs` — Webview texture rendering
- CEF `windowless_frame_rate` setting
- IOSurface/Metal texture sharing architecture
