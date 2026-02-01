# 325: Webview Frame Rate

Webview content does not refresh at 60fps, causing visible lag.

## Status

Experiment 1 designed.

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
    ├─ If handle == last_handle: RETURN (dedup)
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

**Status:** Implemented, awaiting test.

## References

- `ts3/termsurf-profile/src/main.rs` — `on_accelerated_paint` with dedup logic
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC handler, invalidate
  callbacks
- `ts3/wezterm-gui/src/termwindow/render/draw.rs` — Webview texture rendering
- CEF `windowless_frame_rate` setting
- IOSurface/Metal texture sharing architecture
