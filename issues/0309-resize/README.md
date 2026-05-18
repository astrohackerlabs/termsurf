+++
status = "closed"
opened = "2026-01-29"
closed = "2026-01-29"
+++

# TS3-9: Dynamic Webview Resize (Continued)

Continuation of resize work from [308-resize.md](./308-resize.md). The basic
resize pipeline is now functional but exhibits inconsistent behavior.

## Goal

When the user resizes a terminal window or splits a pane, the webview should
dynamically resize to match the new pane dimensions. The resized content should:

1. Fill the pane completely (no gaps, no overflow)
2. Render at correct resolution (not stretched or squished)
3. Maintain crisp text (proper Retina scaling)
4. Respond reliably to all resize events

## Progress Made

### Working

1. **Single source of truth for textures** — The render loop now reads directly
   from `XpcManager::received_surfaces` instead of the disconnected
   `WebviewOverlayState::overlays`. New textures from resize are immediately
   available for rendering.

2. **Bidirectional XPC communication** — GUI can send `resize_browser` commands
   to the profile server. Profile server handles resize and sends new textures
   back.

3. **Resize command pathway** — The full command flow works:
   ```
   GUI detects resize → sends resize_browser via XPC →
   profile calls was_resized() + invalidate() → CEF re-renders →
   profile sends display_surface → GUI receives new texture
   ```

4. **Debounce logic** — 30ms settle delay prevents flooding the profile with
   resize commands during rapid window dragging.

5. **Scale factor conversion** — Logical dimensions (physical / scale) are sent
   to CEF, which expects DIP coordinates.

### Commits

- `de932e372` — Implement webview resize via bidirectional XPC
- `579039b2b` — Design experiment 4: single source of truth
- `e09a93628` — Design experiment 3: fix scale division
- `deaa638bd` — Mark experiment 2 as failed: wrong scale
- `3b5c99d27` — Design experiment 2: fix XPC handler order
- `f5f53aff8` — Mark experiment 1 as failed: XPC order bug

## What Has Failed

Despite the pipeline being functional, resize behavior is **inconsistent**:

1. **Resize doesn't always trigger** — Sometimes changing the window size causes
   no resize. No predictable pattern for when it works vs doesn't.

2. **Stretched appearance** — Sometimes the webview appears visibly stretched,
   indicating the texture dimensions don't match the viewport dimensions.

3. **Doesn't fill pane** — Sometimes the webview doesn't fill the pane
   dimensions correctly, leaving visible gaps or overflowing the bounds.

4. **Unpredictable** — The same resize action might work correctly one time and
   fail the next.

## Top Hypothesis: Scale Factor Inconsistency

**Initial spawn** uses the pane's DPI from the Mux:

```rust
// webview_socket.rs - spawn_browser handler
let scale = dims.dpi as f32 / 72.0;
```

**Resize** uses the window's DPI from TermWindow:

```rust
// draw.rs - render loop
let scale = self.dimensions.dpi as f32 / 72.0;
```

If `dims.dpi` (pane) differs from `self.dimensions.dpi` (window), the logical
dimensions sent for resize won't match what was used for initial spawn. This
causes the profile to create an IOSurface at the wrong size.

**Evidence:** The stretching and incorrect fill suggest dimension mismatches
rather than complete pipeline failure.

## Other Hypotheses

### 1. Texture/Viewport Transition Period

When resize occurs, there's a window where the old texture is rendered at the
new viewport size:

```
t0: Pane resizes → viewport changes immediately
t1: Resize command sent (after 30ms debounce)
t2: Profile receives, browser resizes
t3: CEF re-renders
t4: New texture sent via XPC
t5: GUI receives and renders new texture
```

During t0-t4, the old texture is stretched to fit the new viewport. If any step
fails or is delayed, stretching persists.

### 2. Debounce Logic Not Triggering

The debounce in `check_and_send_resize` requires:

1. The render loop to run
2. 30ms to elapse since the size changed
3. The size to be different from `last_sent_size`

Problems:

- If terminal content is static (no cursor blink, no animations), the render
  loop might not run frequently
- `last_sent_size` is updated optimistically before confirming the resize
  succeeded
- If resize fails, we don't retry

### 3. Render Loop Timing

Resize detection only happens inside `render_webview_overlays_webgpu`. This
function is only called when the window is being painted. If:

- Terminal content is static
- No cursor blinking
- No animations or updates

...the render loop might not run at the right time to detect size changes.

### 4. Viewport Position Calculation Drift

The viewport is calculated from:

```rust
let x = pos.left as f32 * cell_width + border.left;
let y = pos.top as f32 * cell_height + tab_bar_height + border.top;
let w = pos.pixel_width as f32;
let h = pos.pixel_height as f32;
```

If any of these change without triggering a recalculation:

- Tab bar visibility changes
- Cell size changes
- Border dimensions change

...the viewport won't align with the actual pane position.

### 5. Race Condition in Texture Updates

The XPC handler updates `received_surfaces` while the render loop might be
mid-render using the old texture. While this shouldn't cause stretching (just a
one-frame delay), there might be edge cases where dimensions are read
inconsistently.

### 6. CEF Not Actually Resizing

The profile server calls:

```rust
host.was_resized();
host.invalidate(PaintElementType::default());
```

But we haven't verified that CEF actually re-renders at the new size. Possible
issues:

- `view_rect()` might return stale dimensions
- The browser might not honor the resize immediately
- There might be a minimum time between resize calls

## Files Involved

| File                                               | Role                                                      |
| -------------------------------------------------- | --------------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Resize detection, viewport calculation, texture rendering |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | XPC manager, debounce logic, send_resize                  |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Initial spawn, pane dimension lookup                      |
| `ts3/termsurf-profile/src/main.rs`                 | Resize handler, CEF browser resize, texture sending       |

## Next Steps

1. **Add diagnostic logging** — Compare scale factors between initial spawn and
   resize to verify hypothesis #1

2. **Force render loop** — Ensure the render loop runs after resize events by
   calling `window.invalidate()`

3. **Verify CEF resize** — Add logging in profile server to confirm
   `view_rect()` returns updated dimensions after `was_resized()`

4. **Check debounce state** — Log when debounce prevents resize and when it
   allows resize through

5. **Unify scale factor source** — Use the same DPI source for both initial
   spawn and resize

---

## Experiments

### Experiment 1: Diagnostic Logging

**Status:** SUCCESS

**Goal:** Add comprehensive logging throughout the resize pipeline to pinpoint
the true cause of inconsistent resize behavior.

#### The Problem

Resize behavior is unpredictable:

- Sometimes resize triggers, sometimes it doesn't
- Sometimes the webview appears stretched
- Sometimes it doesn't fill the pane correctly
- The same action may work one time and fail the next

We have multiple hypotheses but no data to confirm which is correct. Before
attempting fixes, we need visibility into what's actually happening.

#### Logging Strategy

Add logging at 8 key points in the resize pipeline:

| # | Location               | Purpose                            |
| - | ---------------------- | ---------------------------------- |
| 1 | Initial spawn          | Capture baseline dimensions        |
| 2 | Render loop layout     | See pane dimensions from layout    |
| 3 | Debounce logic         | See when resize is sent vs blocked |
| 4 | Profile resize handler | Confirm receipt of resize command  |
| 5 | CEF view_rect          | What dimensions CEF thinks it has  |
| 6 | Texture sending        | Actual IOSurface dimensions        |
| 7 | Texture receiving      | What GUI receives via XPC          |
| 8 | Texture rendering      | Compare texture vs viewport        |

#### Changes

**1. Initial Spawn (webview_socket.rs)**

After calculating dimensions in `spawn_browser` handler:

```rust
log::info!(
    "[SPAWN] pane={} cols={} rows={} cell={}x{} physical={}x{} dpi={} scale={:.2} logical={}x{}",
    pane_id,
    dims.cols,
    dims.viewport_rows,
    cell_width,
    cell_height,
    physical_width,
    physical_height,
    dims.dpi,
    scale,
    lw,
    lh
);
```

**2. Render Loop Layout (draw.rs)**

After finding pane position, before resize check:

```rust
log::info!(
    "[LAYOUT] pane={} pos.left={} pos.top={} pos.pixel={}x{} cell={}x{} window.dpi={}",
    pane_id,
    pos.left,
    pos.top,
    pos.pixel_width,
    pos.pixel_height,
    self.render_metrics.cell_size.width,
    self.render_metrics.cell_size.height,
    self.dimensions.dpi
);
```

**3. Debounce Logic (webview_xpc.rs)**

At the start of `check_and_send_resize`:

```rust
log::info!(
    "[DEBOUNCE] pane={} current={}x{} last_sent={:?} pending={:?}",
    pane_id,
    width,
    height,
    state.last_sent_size,
    state.pending_resize.map(|(w, h, t)| (w, h, t.elapsed().as_millis()))
);
```

When resize is actually sent:

```rust
log::info!("[DEBOUNCE] pane={} SENDING {}x{}", pane_id, w, h);
```

When resize is skipped (add new log):

```rust
// If size unchanged from last_sent
log::info!("[DEBOUNCE] pane={} SKIP size unchanged", pane_id);

// If still waiting for settle delay
log::info!(
    "[DEBOUNCE] pane={} WAIT {}ms remaining",
    pane_id,
    (SETTLE_DELAY - time.elapsed()).as_millis()
);
```

**4. Profile Resize Handler (termsurf-profile/src/main.rs)**

When resize command is received:

```rust
println!(
    "[RESIZE-RX] width={} height={} prev_state={}x{}",
    width,
    height,
    bs.width.load(Ordering::Relaxed),
    bs.height.load(Ordering::Relaxed)
);
```

After calling `was_resized()`:

```rust
println!("[RESIZE-RX] called was_resized() and invalidate()");
```

**5. CEF view_rect (termsurf-profile/src/main.rs)**

In the `view_rect` callback:

```rust
println!(
    "[VIEW_RECT] returning {}x{}",
    w,
    h
);
```

**6. Texture Sending (termsurf-profile/src/main.rs)**

In `on_accelerated_paint` when sending:

```rust
println!(
    "[TEXTURE-TX] handle={:p} iosurface={}x{} view_rect={}x{}",
    handle,
    info.width,
    info.height,
    self.state.width.load(Ordering::Relaxed),
    self.state.height.load(Ordering::Relaxed)
);
```

**7. Texture Receiving (webview_xpc.rs)**

In the XPC handler for `display_surface`:

```rust
log::info!(
    "[TEXTURE-RX] pane={} mach_port={} size={}x{}",
    pane_id,
    port,
    width,
    height
);
```

**8. Texture Rendering (draw.rs)**

Before importing the texture:

```rust
log::info!(
    "[RENDER] pane={} texture={}x{} viewport={}x{} match={}",
    pane_id,
    surface.width,
    surface.height,
    viewport_w as u32,
    viewport_h as u32,
    surface.width == viewport_w as u32 && surface.height == viewport_h as u32
);
```

#### Files to Modify

| File                                               | Changes                                         |
| -------------------------------------------------- | ----------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add [SPAWN] log                                 |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Add [LAYOUT] and [RENDER] logs                  |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Add [DEBOUNCE] and [TEXTURE-RX] logs            |
| `ts3/termsurf-profile/src/main.rs`                 | Add [RESIZE-RX], [VIEW_RECT], [TEXTURE-TX] logs |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Initial spawn
web google.com
cat /tmp/termsurf-gui.log | grep -E "\[SPAWN\]|\[LAYOUT\]|\[RENDER\]"
cat /tmp/termsurf-profile-*.log | grep -E "\[VIEW_RECT\]|\[TEXTURE-TX\]"
# Expected: See baseline dimensions, verify texture matches viewport

# Test 2: Split pane (Cmd+Shift+D)
cat /tmp/termsurf-gui.log | grep -E "\[DEBOUNCE\]|\[TEXTURE-RX\]"
cat /tmp/termsurf-profile-*.log | grep -E "\[RESIZE-RX\]|\[VIEW_RECT\]"
# Expected: See resize command sent, received, and new texture

# Test 3: Drag window edge
# Watch logs in real-time:
tail -f /tmp/termsurf-gui.log | grep -E "\[DEBOUNCE\]|\[RENDER\]"
# Expected: See debounce behavior, texture/viewport comparison
```

#### What Each Log Reveals

| Issue                  | Log to Check        | What to Look For                    |
| ---------------------- | ------------------- | ----------------------------------- |
| Resize doesn't trigger | [DEBOUNCE]          | SKIP or WAIT instead of SENDING     |
| Stretched appearance   | [RENDER]            | texture size != viewport size       |
| Doesn't fill pane      | [LAYOUT] vs [SPAWN] | pixel dimensions differ             |
| Profile not resizing   | [RESIZE-RX]         | Missing or wrong dimensions         |
| CEF not updating       | [VIEW_RECT]         | Returns old dimensions after resize |
| Wrong texture sent     | [TEXTURE-TX]        | iosurface size != view_rect size    |

#### Success Criteria

- [x] All 8 logging points are implemented
- [x] Logs are parseable with grep patterns
- [x] Can trace a complete resize from detection to render
- [x] Can identify WHERE in the pipeline failures occur
- [x] Have data to inform Experiment 2 (the actual fix)

#### Findings

Logging revealed the true causes of inconsistent resize behavior:

**1. Viewport updates correctly, texture lags far behind**

The pane dimensions (`pos.pixel`) update on every frame during resize:

```
pos.pixel=2054x1920 → 2002x1890 → 1898x1800 → ... → 1599x1590
```

But the texture only updates after: (a) user stops resizing for 30ms, (b) resize
command sent via XPC, (c) CEF re-renders, (d) new IOSurface sent back. This
takes **1-1.5 seconds** total.

**2. Debounce resets continuously during active resize**

Every frame during resize changes the viewport size, which resets the 30ms
debounce timer. Resize commands only send after the user **completely stops**
resizing for 30ms:

```
[DEBOUNCE] pane=0 current=1040x1080 last_sent=None pending=Some((1040, 1080, 1515))
[DEBOUNCE] pane=0 SENDING 1040x1080  ← only after 30ms of no changes
```

This is correct debounce behavior, but means texture is always wrong during
active resize.

**3. Texture/viewport mismatch causes stretching**

The `[RENDER]` logs show `match=false` almost continuously during resize:

```
[RENDER] pane=0 texture=2820x2130 viewport=1599x1590 match=false  ← stretched!
[RENDER] pane=0 texture=1598x1590 viewport=2990x2190 match=false  ← stretched!
[RENDER] pane=0 texture=2990x2190 viewport=2990x2190 match=true   ← finally correct
```

The texture is rendered at its native size into a differently-sized viewport,
causing the stretched appearance.

**4. CEF pipeline is working correctly**

The profile logs show CEF is doing its job:

```
[RESIZE-RX] session=pane-0-2208 width=799 height=795 prev=1040x1080
[VIEW_RECT] session=pane-0-2208 returning 799x795
[TEXTURE-TX] session=pane-0-2208 iosurface=1598x1590 view_rect=799x795
```

CEF receives resize, updates view_rect, and sends correctly-sized IOSurface
(1598×1590 = 799×2 × 795×2 for Retina). The issue is timing, not CEF.

#### Root Cause Summary

| Symptom                   | Actual Cause                                                                       |
| ------------------------- | ---------------------------------------------------------------------------------- |
| "Usually doesn't resize"  | Debounce keeps resetting during active resize; only sends when user stops for 30ms |
| "Wrong size when it does" | By the time new texture arrives (1+ second), user has already changed size again   |
| "Stretched appearance"    | Texture rendered 1:1 but viewport size has changed, causing aspect ratio mismatch  |

#### Conclusion

The resize **pipeline** works correctly. The problem is the **rendering
strategy**: we render the texture at its native size, but the viewport changes
continuously during resize. This causes a persistent mismatch.

**Potential fixes for Experiment 2:**

1. **Scale texture to fit viewport** — Instead of rendering texture 1:1, scale
   it to fill the viewport. Will be slightly blurry but not stretched.

2. **Send resize more aggressively** — Send interim resizes during active drag
   (every 100ms?) instead of only after stopping.

3. **Speed up CEF response** — Investigate why it takes 1+ seconds for CEF to
   produce a new texture after resize command.

---

### Experiment 2: Remove Debounce

**Status:** SUCCESS

**Goal:** Remove the 30ms debounce logic entirely and send resize commands on
every frame where the size changes. This tests whether CEF can keep up with
rapid resize commands and whether removing the debounce improves perceived
responsiveness.

#### Hypothesis

The 30ms debounce was added to prevent "flooding" the profile server with resize
commands. But Experiment 1 showed that the debounce causes resize commands to
only fire after the user completely stops dragging, resulting in a 1+ second
delay before the correct texture appears.

If we remove the debounce:

- **Best case:** CEF keeps up, textures arrive faster, resize feels responsive
- **Worst case:** XPC/CEF gets overwhelmed, performance degrades, but we learn
  the actual bottleneck

Either outcome provides valuable data.

#### Changes

**webview_xpc.rs — Remove debounce logic**

Replace `check_and_send_resize` with a simpler version that sends immediately
when size changes:

```rust
/// Send resize command immediately if size changed from last sent.
/// No debouncing — send on every frame where size differs.
pub fn check_and_send_resize(&self, pane_id: PaneId, width: u32, height: u32) -> bool {
    let current_size = (width, height);

    let mut debounce = self.resize_debounce.lock().unwrap();
    let state = debounce.entry(pane_id).or_insert(ResizeDebounceState {
        last_sent_size: None,
        pending_resize: None,
    });

    // Only send if size actually changed
    if state.last_sent_size == Some(current_size) {
        return false;
    }

    log::info!(
        "[RESIZE] pane={} SENDING {}x{} (was {:?})",
        pane_id,
        width,
        height,
        state.last_sent_size
    );

    state.last_sent_size = Some(current_size);
    state.pending_resize = None;
    drop(debounce);

    self.send_resize(pane_id, width, height)
}
```

#### Files to Modify

| File                                            | Changes                              |
| ----------------------------------------------- | ------------------------------------ |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Replace debounce with immediate send |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Slow drag
web google.com
# Slowly drag window edge
# Watch: Does webview resize more frequently?

# Test 2: Fast drag
# Rapidly drag window edge back and forth
# Watch: Does performance degrade? Does app lag?

# Test 3: Check XPC traffic
cat /tmp/termsurf-gui.log | grep "\[RESIZE\]" | wc -l
# Expected: Many more resize commands than before

cat /tmp/termsurf-profile-*.log | grep "\[RESIZE-RX\]" | wc -l
# Expected: Should match GUI send count (no lost commands)
```

#### What We're Testing

| Question                        | How to Measure                         |
| ------------------------------- | -------------------------------------- |
| Can CEF handle rapid resize?    | Watch for lag, dropped frames, crashes |
| Does removing debounce help UX? | Subjective: does resize feel smoother? |
| What's the actual bottleneck?   | Compare send count vs receive count    |
| Is XPC the limiting factor?     | Check if commands are lost or delayed  |

#### Success Criteria

- [x] Resize commands sent on every size change (no 30ms wait)
- [x] App remains stable during rapid resize
- [x] Determine if debounce removal improves or worsens UX
- [x] Identify actual bottleneck if UX doesn't improve

#### Risks

- **Performance degradation** — Flooding XPC/CEF with resize commands could
  cause lag or dropped frames
- **Resource exhaustion** — Rapid IOSurface creation could exhaust GPU memory
- **Race conditions** — More resize commands in flight increases chance of
  ordering issues

If performance degrades significantly, we'll know the debounce was necessary and
can explore middle-ground solutions (e.g., 10ms debounce, or throttle to 60fps).

#### Outcome

Removing debounce entirely fixed the resize behavior. Resize commands now fire
immediately when size changes, and the webview updates responsively during drag.
Some remaining issues exist but are unrelated to debounce timing.

---

### Experiment 3: Correct Debounce Pattern (from ts2)

**Status:** FAILED

**Goal:** Implement the correct debounce pattern from ts2, which properly waits
for size to "settle" before sending resize commands. This provides the best of
both worlds: responsive resize during drag AND reduced XPC traffic.

#### Background

Experiment 2 removed debounce entirely and worked, but sends many resize
commands during drag (potentially wasteful). ts2 has a debounce that works
perfectly. The difference:

**ts3's broken debounce (Experiment 1):**

```rust
// BUG: Timer reset EVERY frame because we compared to last_sent_size
if state.last_sent_size != Some(current_size) {
    if state.pending_resize.map(|(w, h, _)| (w, h)) != Some(current_size) {
        state.pending_resize = Some((width, height, now));  // Timer reset!
    }
}
```

Since `last_sent_size` is `None` until we actually send, every frame saw a
"change" and reset the timer. The 30ms never elapsed until dragging stopped.

**ts2's correct debounce:**

```rust
// Only reset timer when TARGET changes (not when it differs from sent)
if browser.get_pending_size() != Some(target_size) {
    browser.set_pending_size(logical_width, logical_height);
    browser.mark_resize_time();  // Timer resets ONLY here
}

// Check if we've waited long enough since target last changed
if let Some(elapsed) = browser.time_since_last_resize() {
    if elapsed >= SETTLE_DELAY {
        browser.resize(logical_width, logical_height);
        browser.clear_resize_time();
        browser.clear_pending_size();
    } else {
        // CRITICAL: Ensure render loop runs again to check timer
        self.window.invalidate();
    }
}
```

The key insight: **reset timer only when the target size changes, not when it
differs from what we last sent.**

#### The Pattern

ts2 puts the debounce logic **inline in the render function**, not in a separate
method. This gives direct access to `self.window` for invalidation.

Three pieces of state per pane:

| State            | Purpose                                    |
| ---------------- | ------------------------------------------ |
| `pending_size`   | The target size we want to resize to       |
| `pending_since`  | When the target last changed (timer start) |
| `last_sent_size` | What we last sent via XPC (for dedup)      |

**Why ts3 needs `last_sent_size` but ts2 doesn't:**

- ts2's `browser.resize()` is in-process and synchronous — resizing to the same
  size is a cheap no-op
- ts3's resize goes over XPC and triggers IOSurface recreation — redundant
  resizes would flood the profile server

Without `last_sent_size`, after sending we'd clear `pending_size`, then on the
next frame see `None != Some(current)` and start the timer again, creating an
infinite loop of resizes every 30ms.

Flow:

1. Fast path: if `last_sent_size == current`, skip (already correct)
2. If `pending_size != current`: update `pending_size`, reset timer
3. If `pending_size == current`: check if 30ms elapsed
4. If elapsed: send resize, record in `last_sent_size`, clear pending
5. If not elapsed: call `window.invalidate()` to ensure we check again

This means:

- During fast drag: timer keeps resetting, no resize sent (debounced)
- When drag slows/stops: timer runs out, resize sent
- After send: `last_sent_size` prevents re-triggering until size changes
- **Critical:** `window.invalidate()` ensures render loop runs to check timer

#### Changes

**mod.rs — Add state to TermWindow (like ts2's BrowserState)**

Add the debounce state directly to `TermWindow`, using `RefCell` like ts2:

```rust
use std::cell::RefCell;
use std::time::Instant;

/// State for debouncing resize commands (ts2 pattern + last_sent for XPC dedup).
pub struct WebviewResizeState {
    /// The target size we want to resize to
    pub pending_size: Option<(u32, u32)>,
    /// When pending_size last changed (timer start)
    pub pending_since: Option<Instant>,
    /// What we last sent via XPC (prevents infinite loop after send)
    /// Note: ts2 doesn't need this because in-process resize is a cheap no-op.
    /// ts3 needs it because XPC resize triggers IOSurface recreation.
    pub last_sent_size: Option<(u32, u32)>,
}

pub struct TermWindow {
    // ... existing fields ...

    /// Per-pane resize debounce state (like ts2's BrowserState fields)
    webview_resize_state: RefCell<HashMap<PaneId, WebviewResizeState>>,
}
```

**webview_xpc.rs — Keep only send_resize**

Remove `resize_debounce` field and `get_resize_state` method. Keep only
`send_resize`:

```rust
impl XpcManager {
    /// Send resize command to profile server.
    pub fn send_resize(&self, pane_id: PaneId, width: u32, height: u32) -> bool {
        // ... existing send_resize implementation ...
    }
}
```

**draw.rs — Inline debounce logic using self (like ts2)**

In `render_webview_overlays_webgpu`, access state via `self` instead of
`xpc_manager`:

```rust
// Inside render_webview_overlays_webgpu, after calculating logical_w/logical_h:

use std::time::{Duration, Instant};
const SETTLE_DELAY: Duration = Duration::from_millis(30);

let target_size = (logical_w, logical_h);

// Access state via self (like ts2 accesses browser.get_pending_size())
let mut resize_state = self.webview_resize_state.borrow_mut();
let state = resize_state.entry(*pane_id).or_insert(WebviewResizeState {
    pending_size: None,
    pending_since: None,
    last_sent_size: None,
});

// Fast path: size unchanged from last sent, nothing to do
// This prevents infinite loop: send → clear pending → set pending → send...
if state.last_sent_size == Some(target_size) {
    state.pending_size = None;
    state.pending_since = None;
    drop(resize_state);  // Release borrow for consistency
    // Already at correct size, skip debounce logic
} else {
    // Check if target size changed (compare against pending, not sent)
    if state.pending_size != Some(target_size) {
        state.pending_size = Some(target_size);
        state.pending_since = Some(Instant::now());
        log::debug!(
            "[DEBOUNCE] pane={} target changed to {}x{}",
            pane_id, logical_w, logical_h
        );
    }

    // Settle-and-send logic
    if let Some(since) = state.pending_since {
        let elapsed = since.elapsed();
        if elapsed >= SETTLE_DELAY {
            log::info!(
                "[DEBOUNCE] pane={} SENDING {}x{} (settled {:?})",
                pane_id, logical_w, logical_h, elapsed
            );
            // Record what we sent and clear pending state
            state.last_sent_size = Some(target_size);
            state.pending_size = None;
            state.pending_since = None;
            drop(resize_state);  // Release borrow before XPC call

            if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
                xpc_manager.send_resize(*pane_id, logical_w, logical_h);
            }
        } else {
            // Not settled yet — ensure render loop runs again
            drop(resize_state);  // Release borrow before window call
            if let Some(ref w) = self.window {
                w.invalidate();
            }
        }
    }
}
```

#### Structural Comparison with ts2

| Aspect            | ts2                               | Experiment 3 (updated)                     |
| ----------------- | --------------------------------- | ------------------------------------------ |
| State location    | `BrowserState` on TermWindow      | `WebviewResizeState` on TermWindow         |
| State access      | `browser.get_pending_size()`      | `self.webview_resize_state.borrow_mut()`   |
| Concurrency       | `RefCell`                         | `RefCell`                                  |
| Debounce logic    | Inline in `paint_browser_overlay` | Inline in `render_webview_overlays_webgpu` |
| Window invalidate | `self.window.invalidate()`        | `self.window.invalidate()`                 |

Only unavoidable XPC differences remain:

- `last_sent_size` field (XPC resizes are expensive)
- `xpc_manager.send_resize()` instead of `browser.resize()`

#### Files to Modify

| File                                            | Changes                                           |
| ----------------------------------------------- | ------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/mod.rs`         | Add `WebviewResizeState` and field on TermWindow  |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Inline debounce logic using `self`                |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Remove `resize_debounce`, keep only `send_resize` |

#### Why Inline Matters

The previous approach put debounce logic in `XpcManager::check_and_send_resize`,
which had no access to `self.window`. Without `window.invalidate()`:

- If terminal content is static (no cursor blink, no animations)
- The render loop might not run frequently
- The 30ms timer could expire but never get checked

By inlining the logic in `draw.rs` (like ts2), we have direct access to
`self.window` and can ensure the render loop keeps running while waiting.

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Fast drag (debounce should activate)
web google.com
# Rapidly drag window edge
# Watch logs: should see "target changed" but few "SENDING"
tail -f /tmp/termsurf-gui.log | grep "\[DEBOUNCE\]"

# Test 2: Slow drag (should send on each pause)
# Slowly drag, pausing occasionally
# Watch logs: should see "SENDING" after each 30ms pause

# Test 3: Compare with Experiment 2
cat /tmp/termsurf-gui.log | grep "SENDING" | wc -l
# Expected: Fewer sends than Experiment 2, but still responsive

# Test 4: Static terminal (cursor blink disabled)
# Resize window, then stop
# Verify resize still fires after 30ms (invalidate working)
```

#### Expected Behavior

| Scenario             | Experiment 1 (broken) | Experiment 2 (no debounce) | Experiment 3 (fixed)     |
| -------------------- | --------------------- | -------------------------- | ------------------------ |
| Fast continuous drag | No sends until stop   | Send every frame           | No sends until pause     |
| Drag with pauses     | Send after each stop  | Send every frame           | Send after 30ms pause    |
| Stable size          | No sends              | No sends                   | No sends                 |
| XPC traffic          | Low but delayed       | High                       | Moderate                 |
| Static terminal      | Timer never fires     | N/A                        | Timer fires (invalidate) |

#### Success Criteria

- [x] Debounce logic is inline in draw.rs (like ts2)
- [x] `window.invalidate()` called when waiting for timer
- [x] Timer only resets when target size changes
- [ ] Resize sends after 30ms of stable target
- [ ] Fewer resize commands than Experiment 2
- [ ] Works even with static terminal content

#### Outcome

The debouncing does not work reliably. Sometimes the browser will re-render
after the user stops dragging the window. Other times, it simply won't re-render
at all. If it doesn't re-render, clicking on the webview "fixes" it — for some
reason that causes it to re-render.

This suggests `window.invalidate()` is not reliably triggering render loop
iterations, or there's a race condition where the debounce state is being
checked at the wrong time relative to when the timer expires.

#### Hypothesis: Why Experiment 3 Failed

**The core problem:** The debounce logic depends on `window.invalidate()` to
schedule another render pass so the timer expiry can be checked. But WezTerm may
be optimizing away these invalidations when "nothing has changed" from its
perspective.

**Evidence supporting this:**

1. **Clicking "fixes" it** — Input events definitively trigger render cycles.
   This proves the debounce logic works when a render pass actually occurs. The
   problem is getting render passes to happen.

2. **Inconsistent behavior** — Sometimes it works (render loop happens to run),
   sometimes it doesn't (no render scheduled). This suggests `invalidate()` is
   unreliable, not that the logic itself is broken.

3. **ts2 works, ts3 doesn't** — ts2 has CEF running in-process. CEF's `OnPaint`
   callback fires whenever the browser renders new content, which would
   naturally trigger render cycles. In ts3, the webview is just a static texture
   overlay with no in-process engine keeping the loop active.

**Why `invalidate()` might fail:**

- Window systems often coalesce multiple invalidate requests
- WezTerm might have optimization logic that says "terminal content unchanged,
  skip redraw"
- The invalidate might be happening but the render pass completes before the
  30ms timer expires, then nothing schedules the next check

**Potential fixes to explore in future experiments:**

1. Use `schedule_callback` or a timer mechanism instead of relying on
   render-loop invalidation
2. Track "debounce pending" as explicit state that forces renders until resolved
3. Investigate WezTerm's existing mechanisms for scheduled re-renders (cursor
   blink, etc.) and hook into those

---

### Experiment 4: Diagnose invalidate() Behavior

**Status:** SUCCESS

**Goal:** Add diagnostic logging to determine whether `window.invalidate()` is
being called and whether it's actually triggering render passes. This will
pinpoint why Experiment 3's debounce fails intermittently.

#### Background

Experiment 3 implemented the correct debounce pattern from ts2, but it fails
intermittently. The hypothesis is that `window.invalidate()` isn't reliably
triggering render passes. Before attempting fixes, we need to confirm this
hypothesis with data.

#### Key Questions

1. Is the render loop running at all after the user stops dragging?
2. Is `invalidate()` being called?
3. Is `self.window` ever `None` (causing silent skip)?
4. What's the time gap between renders? (reveals if invalidate works)
5. Which debounce branch are we taking?

#### Log Points

**1. Render loop entry (draw.rs)**

At the very start of `render_webview_overlays_webgpu`, before any pane
iteration:

```rust
log::info!(
    "[RENDER-LOOP] render_webview_overlays_webgpu called at {:?}",
    std::time::Instant::now()
);
```

**2. Debounce branch: Fast path (draw.rs)**

When `last_sent_size == target_size`:

```rust
log::info!(
    "[DEBOUNCE] pane={} FAST_PATH size={}x{} (already sent)",
    pane_id, logical_w, logical_h
);
```

**3. Debounce branch: Target changed (draw.rs)**

When `pending_size != target_size`:

```rust
log::info!(
    "[DEBOUNCE] pane={} TARGET_CHANGED to {}x{} (timer reset)",
    pane_id, logical_w, logical_h
);
```

**4. Debounce branch: Waiting (draw.rs)**

When timer hasn't expired, right before calling invalidate:

```rust
let remaining = SETTLE_DELAY.saturating_sub(elapsed);
log::info!(
    "[DEBOUNCE] pane={} WAITING elapsed={:?} remaining={:?}",
    pane_id, elapsed, remaining
);
```

**5. Before invalidate() call (draw.rs)**

```rust
log::info!("[DEBOUNCE] pane={} calling window.invalidate()", pane_id);
if let Some(ref w) = self.window {
    w.invalidate();
    log::info!("[DEBOUNCE] pane={} invalidate() completed", pane_id);
} else {
    log::warn!("[DEBOUNCE] pane={} self.window is None!", pane_id);
}
```

**6. Debounce branch: Sending (draw.rs)**

When timer expired and we're sending:

```rust
log::info!(
    "[DEBOUNCE] pane={} SENDING {}x{} (elapsed={:?})",
    pane_id, logical_w, logical_h, elapsed
);
```

#### Files to Modify

| File                                            | Changes                                |
| ----------------------------------------------- | -------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Add all 6 log points in debounce logic |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Normal resize
web google.com
# Drag window slowly, then stop
tail -f /tmp/termsurf-gui.log | grep "\[DEBOUNCE\]\|\[RENDER-LOOP\]"

# Expected if invalidate works:
# [RENDER-LOOP] ... (continuous stream)
# [DEBOUNCE] WAITING elapsed=5ms remaining=25ms
# [DEBOUNCE] calling window.invalidate()
# [DEBOUNCE] invalidate() completed
# [RENDER-LOOP] ... (another render quickly follows)
# ... repeats until timer expires ...
# [DEBOUNCE] SENDING

# Expected if invalidate fails:
# [RENDER-LOOP] ... (during drag)
# [DEBOUNCE] WAITING ...
# [DEBOUNCE] calling window.invalidate()
# [DEBOUNCE] invalidate() completed
# ... long gap, no more [RENDER-LOOP] logs ...
# (user clicks)
# [RENDER-LOOP] ... (render finally happens)
# [DEBOUNCE] SENDING (timer has long since expired)
```

#### What Each Pattern Reveals

| Log Pattern                             | Diagnosis                            |
| --------------------------------------- | ------------------------------------ |
| Continuous RENDER-LOOP after drag stops | invalidate() works, bug is elsewhere |
| RENDER-LOOP stops after drag stops      | invalidate() not triggering renders  |
| "self.window is None" logged            | Window reference issue               |
| WAITING logs but never SENDING          | Timer logic bug                      |
| FAST_PATH when shouldn't be             | last_sent_size incorrectly set       |
| Long gap between RENDER-LOOP entries    | invalidate() being throttled         |

#### Success Criteria

- [x] All 6 log points implemented
- [x] Can observe render loop frequency via timestamps
- [x] Can trace exact debounce state transitions
- [x] Can confirm whether invalidate() is called
- [x] Can confirm whether invalidate() triggers another render
- [x] Have data to design Experiment 5 (the fix)

#### Findings

The logs revealed that **the debounce logic is working correctly**:

1. `invalidate()` IS being called and IS triggering renders during the WAITING
   phase
2. SENDING fires correctly after 30ms of stable size
3. Resize commands are received by profile server
4. Profile server sends new IOSurface back (~30-40ms after resize)
5. GUI receives the new texture via XPC

**The real bug**: After SENDING, we hit FAST_PATH which does NOT call
`invalidate()`. The render loop goes idle. When the new texture arrives via XPC,
nothing triggers a render to display it:

```
08:04:17.907  SENDING 2060x1245
08:04:17.944  TEXTURE-RX 4120x2490  ← New texture arrives via XPC
08:04:18.831  RENDER-LOOP           ← 887ms gap! No render to display new texture
```

The texture sits in `received_surfaces` until something else triggers a render
(mouse move, click, etc.). This explains the "sometimes works, sometimes
doesn't" behavior — it depends on whether incidental events trigger renders.

**Root cause**: Unlike ts2 where CEF is in-process and calls
`invalidate_callback()` on every paint, ts3's XPC handler has no way to notify
the window that a new texture has arrived and needs to be displayed.

---

### Experiment 5: Invalidate Callback for XPC Texture Receipt

**Status:** SUCCESS

**Goal:** When the XPC handler receives a new texture from the profile server,
trigger a window invalidate so the render loop runs and displays the new
texture. This mirrors ts2's `invalidate_callback` pattern.

#### Background

Experiment 4 revealed that the debounce logic works correctly, but there's a gap
in the notification chain:

| ts2 (in-process CEF)                                  | ts3 (out-of-process CEF)                              |
| ----------------------------------------------------- | ----------------------------------------------------- |
| CEF paints → `invalidate_callback()` → window redraws | Profile sends IOSurface → XPC handler stores it → ??? |

ts2's `invalidate_callback` is passed to BrowserState when creating the browser.
CEF's `on_accelerated_paint` calls it directly, triggering a window redraw.

ts3 needs the same pattern: when the XPC handler receives a `display_surface`
message, it should trigger a window invalidate.

#### The Challenge

The XPC event handler runs on a background thread (XPC dispatch queue) and has
no direct access to TermWindow. We need to bridge this gap safely.

#### Solution Design

**1. Add callback storage to XpcManager**

```rust
// In XpcManager struct
invalidate_callbacks: Mutex<HashMap<PaneId, Arc<dyn Fn() + Send + Sync>>>,
```

**2. Register callback when spawning browser**

The spawn request originates from TermWindow (via webview_socket), where we have
access to `self.window`. Pass an invalidate callback:

```rust
// In request_profile_spawn signature
pub fn request_profile_spawn(
    self: &Arc<Self>,
    pane_id: PaneId,
    url: &str,
    profile: &str,
    width: u32,
    height: u32,
    scale: f32,
    invalidate_callback: Arc<dyn Fn() + Send + Sync>,  // NEW
) -> anyhow::Result<String>
```

**3. Create callback in webview_socket.rs**

When handling the spawn_browser command:

```rust
// Create invalidate callback that captures window reference
let window = term_window.window.clone();
let invalidate_callback = Arc::new(move || {
    if let Some(ref w) = window {
        w.invalidate();
    }
});

xpc_manager.request_profile_spawn(
    pane_id,
    &url,
    &profile,
    logical_width,
    logical_height,
    scale,
    invalidate_callback,  // Pass the callback
)?;
```

**4. Store callback in XpcManager**

In `request_profile_spawn`:

```rust
// Store invalidate callback for this pane
self.invalidate_callbacks
    .lock()
    .unwrap()
    .insert(pane_id, invalidate_callback);
```

**5. Call callback when texture arrives**

In the XPC event handler, after storing the received surface:

```rust
if action == "display_surface" {
    // ... existing code to extract port, width, height ...
    // ... store in received_surfaces ...

    // NEW: Trigger window invalidate
    if let Some(callback) = manager.invalidate_callbacks.lock().unwrap().get(&pane_id) {
        callback();
    }
}
```

**6. Clean up callback when pane closes**

In `remove_surface` or a new cleanup method:

```rust
pub fn remove_surface(&self, pane_id: PaneId) {
    self.received_surfaces.lock().unwrap().remove(&pane_id);
    self.invalidate_callbacks.lock().unwrap().remove(&pane_id);  // NEW
    // ... rest of cleanup ...
}
```

#### Why This Works

1. **Thread safety**: `window.invalidate()` on macOS posts to the main thread
   via Cocoa's `setNeedsDisplay`, which is safe to call from any thread.

2. **Ownership**: TermWindow creates and owns the callback. XpcManager just
   holds a reference. When the pane closes, we clean up.

3. **Per-pane**: Each pane has its own callback, correctly handling multiple
   webviews across different windows.

4. **Event-driven**: No polling. Invalidate fires exactly when a new texture
   arrives.

#### Files to Modify

| File                                               | Changes                                                                                                  |
| -------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Add `invalidate_callbacks` field, update `request_profile_spawn` signature, call callback in XPC handler |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Create and pass invalidate callback when spawning                                                        |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Basic resize
web google.com
# Drag window edge, stop
# Expected: Webview re-renders immediately after resize (no click needed)

# Test 2: Check logs
tail -f /tmp/termsurf-gui.log | grep "\[TEXTURE-RX\]\|\[RENDER-LOOP\]"
# Expected: RENDER-LOOP entry shortly after each TEXTURE-RX

# Test 3: Multiple rapid resizes
# Drag continuously, then stop
# Expected: Final resize renders correctly without user interaction

# Test 4: Static terminal (no cursor blink)
# Resize and stop
# Expected: Still works (callback triggers render, not relying on terminal activity)
```

#### Success Criteria

- [x] `invalidate_callbacks` HashMap added to XpcManager
- [x] Callback registered during first render (adjusted from spawn time)
- [x] Callback invoked when `display_surface` received
- [x] Window redraws immediately after new texture arrives
- [x] No user interaction needed to see resized content
- [ ] Cleanup works when pane closes (not yet tested)

---

## Conclusion

**Primary goal accomplished.** Webview panes now resize correctly when the user
drags the window edge or splits panes. The solution combines:

1. **Debounce pattern (Experiment 3)** — Resize commands are sent after 30ms of
   stable target size, preventing XPC flooding during rapid drag.

2. **Invalidate callback (Experiment 5)** — When the profile server sends a new
   IOSurface via XPC, the callback triggers a window redraw so the new texture
   is displayed immediately.

The resize pipeline now works end-to-end:

```
User drags window → debounce waits 30ms → GUI sends resize via XPC →
Profile server resizes CEF → CEF re-renders → Profile sends new IOSurface →
XPC handler calls invalidate callback → Window redraws → New texture displayed
```

## Remaining Issues

### Critical: Browser overlay leaks across tabs

**Symptom:** After opening a browser pane with `web google.com`, opening a new
tab (Cmd+T) causes the browser to:

1. Appear in the new tab (incorrectly — new tabs should have no browser)
2. Cover the tab bar, making mouse navigation impossible

The browser overlay seems to be rendered globally rather than per-pane. When a
new tab is created, the overlay logic doesn't correctly scope the browser to its
original pane, causing it to "leak" into the new tab's rendering and overflow
its intended bounds.

**Impact:** Users cannot navigate back to the original tab using the mouse. Only
keyboard shortcuts work. This is a critical UX bug.

**Next steps:** Investigate how the overlay registry (`WebviewOverlayState`)
interacts with tab/pane creation, and ensure the viewport calculation in
`render_webview_overlays_webgpu` correctly constrains rendering to the specific
pane bounds rather than the full window.
