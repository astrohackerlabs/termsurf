# 311: Resize Accuracy Issues

## Summary

After implementing debounce-based resize in 309-resize.md and fixing the tab
leak in 310-tabs.md, two resize-related issues remain:

1. **Borders appear after resizing**: When resizing the window, black borders
   often appear at the top/bottom or left/right of the webview. This indicates
   the texture size doesn't match the viewport size.

2. **Webview doesn't fill exact pixel bounds**: The webview matches the nearest
   grid (rows/columns) rather than the exact pixel dimensions of the pane. Extra
   space appears as blank areas because the pane's pixel dimensions rarely align
   perfectly with cell boundaries.

Both issues exist in ts3 but not in ts2. The hypothesis is that ts3's sizing
logic diverged from ts2's during the XPC architecture migration.

## Prior Work

- **309-resize.md**: Implemented debounce pattern with `pending_size`,
  `pending_since`, `last_sent_size` and invalidate callbacks
- **310-tabs.md**: Fixed browser overlay leaking across tabs by filtering by
  `tab_id`

## Research: ts2 vs ts3 Comparison

### ts2: How It Works (Correctly)

**Spawn-time sizing** (`ts2/wezterm-gui/src/termwindow/mod.rs`):

```rust
// Uses render_metrics.cell_size (DPI-aware, always current)
let physical_width = dims.cols * render_metrics.cell_size.width;
let physical_height = dims.viewport_rows * render_metrics.cell_size.height;
let logical_width = physical_width / scale;
let logical_height = physical_height / scale;
```

**Render-time sizing** (`ts2/wezterm-gui/src/termwindow/render/pane.rs`):

```rust
// Called EVERY frame in paint_browser_overlay()
let (x, y, w, h) = calculate_pane_pixel_bounds();  // Exact pixels
let logical_w = w / scale;
let logical_h = h / scale;

// Debounce logic
if target_size != pending_size {
    set_pending_size(target_size);
    mark_resize_time();
}
if time_since_last_resize() >= 30ms {
    browser.resize(logical_w, logical_h);
    // Synchronous CEF update:
    host.was_resized();
    host.invalidate(PaintElementType::default());
    cef::do_message_loop_work();  // Pump message loop
}
```

**Key patterns:**

1. Uses `render_metrics.cell_size` (updated every render, DPI-aware)
2. Calls `calculate_pane_pixel_bounds()` for exact pixel dimensions
3. After resize, pumps CEF message loop synchronously
4. Texture updates immediately (same process)

### ts3: How It Works (Buggy)

**Spawn-time sizing** (`ts3/wezterm-gui/src/termwindow/webview_socket.rs`):

```rust
// Uses GLOBAL STATIC cell size (may be stale!)
let (cell_width, cell_height) = get_cell_size();  // From CELL_WIDTH/CELL_HEIGHT
let physical_width = dims.cols as f32 * cell_width as f32;
let physical_height = dims.viewport_rows as f32 * cell_height as f32;
let logical_w = (physical_width / scale) as u32;
let logical_h = (physical_height / scale) as u32;

xpc_manager.request_profile_spawn(pane_id, url, profile, logical_w, logical_h, scale);
```

**Render-time sizing** (`ts3/wezterm-gui/src/termwindow/render/draw.rs`):

```rust
// Uses pos.pixel_width/height (correct, exact pixels)
let viewport_w = pos.pixel_width as f32;
let viewport_h = pos.pixel_height as f32;
let logical_w = (viewport_w / scale) as u32;
let logical_h = (viewport_h / scale) as u32;

// Debounce logic (similar to ts2)
if state.last_sent_size == Some(target_size) {
    // Fast path - already sent
} else if target_size changed {
    pending_size = target_size;
    pending_since = now;
} else if elapsed >= 30ms {
    xpc_manager.send_resize(logical_w, logical_h);  // Async!
    last_sent_size = target_size;
}
```

**Problems identified:**

| Issue            | ts2                                 | ts3                           |
| ---------------- | ----------------------------------- | ----------------------------- |
| Cell size source | `render_metrics.cell_size` (fresh)  | Global static (stale)         |
| Resize delivery  | Synchronous (same process)          | Async (XPC to profile server) |
| Post-resize sync | `was_resized()` + message loop pump | None (fire and forget)        |
| Texture update   | Immediate                           | Waits for IOSurface via XPC   |

## Root Cause Analysis

### Issue 1: Borders After Resizing

**Timeline of the bug:**

```
Frame 1: Pane is 800px wide, texture is 800px
Frame 2-5: User drags to resize to 700px
  - Viewport immediately becomes 700px (from pos.pixel_width)
  - Debounce timer starts
  - Texture still 800px (waiting for XPC)
  - Render: 800px texture in 700px viewport → clipped, but no border yet
Frame 6-8: Debounce waiting (< 30ms)
  - Viewport: 700px
  - Texture: 800px
  - Still rendering old texture
Frame 9: 30ms elapsed, debounce fires
  - Sends XPC resize command for 700px
  - Texture: STILL 800px (profile server hasn't responded)
  - Viewport: 700px
Frame 10-15: Waiting for profile server...
  - Texture: 800px (stale)
  - Viewport: 700px
  - Border visible if texture doesn't cover viewport
Frame 16: Profile server sends new 700px IOSurface
  - Finally matches!
```

**Root cause:** Async XPC communication means texture lags behind viewport.
During the lag, size mismatch creates visible borders.

### Issue 2: Blank Space at Edges

**The mismatch:**

```
Spawn time (webview_socket.rs):
  cell_width = 8 (from global static, maybe stale)
  cols = 100
  physical = 100 * 8 = 800px

Render time (draw.rs):
  pos.pixel_width = 803px (actual pane dimensions from layout)

Result: Texture is 800px, viewport is 803px → 3px blank on right
```

**Root cause:** Spawn uses grid-based calculation with potentially stale cell
size. Render uses exact pixel dimensions. These diverge.

## Hypotheses

### Hypothesis 1: Use Exact Pixels at Spawn Time

Instead of calculating `cols × cell_width`, use `pos.pixel_width` directly at
spawn time. This requires access to the positioned pane layout from the socket
handler.

**Challenge:** The socket handler runs in a background thread and may not have
access to the current layout. Need to either:

- Pass pixel dimensions from the `web` command caller
- Query the layout from the socket handler
- Accept grid-based sizing at spawn but immediately resize to exact pixels

### Hypothesis 2: Add Post-Debounce Synchronization

After sending resize via XPC, continue invalidating until the received texture
size matches the expected size.

```rust
// Track what we've received, not just what we've sent
if received_surface.width == expected_width
   && received_surface.height == expected_height {
    // Sizes match, stop invalidating
} else {
    // Keep invalidating until texture catches up
    window.invalidate();
}
```

### Hypothesis 3: Send Exact Pixel Dimensions in Resize

Currently, resize sends logical dimensions calculated from viewport:

```rust
let logical_w = (viewport_w / scale) as u32;
```

The `as u32` truncation may lose precision. If viewport is 803px and scale is
2.0, logical becomes 401 (truncated from 401.5). CEF renders at 401 logical =
802 physical. Result: 1px border.

**Fix:** Use consistent rounding or send physical pixels and let profile server
handle scaling.

### Hypothesis 4: Match ts2's Exact Sizing Flow

Port ts2's sizing logic more faithfully:

1. At spawn: Calculate size the same way ts2 does in `handle_web_open()`
2. At resize: Use `render_metrics.cell_size` instead of global static
3. After resize: Add acknowledgment from profile server before clearing
   `pending_size`

## Proposed Experiments

### Experiment 1: Diagnostic Logging

Add detailed logging to understand the exact mismatch:

- Log spawn-time dimensions (what we request)
- Log received texture dimensions (what profile server sends)
- Log viewport dimensions (what we render to)
- Log any mismatches

### Experiment 2: Use Exact Pixels at Spawn

Modify `webview_socket.rs` to use exact pixel dimensions instead of grid-based:

- Get `pos.pixel_width/height` from the positioned pane
- Send physical pixels to profile server
- Profile server converts to logical using scale

### Experiment 3: Continue Invalidating Until Match

Modify debounce logic to track received size:

- Add `last_received_size` to state
- Update it when new IOSurface arrives via XPC
- Only stop invalidating when received matches expected

### Experiment 4: Synchronous Resize Acknowledgment

Add XPC response from profile server after resize completes:

- Profile server sends "resize_complete" with actual dimensions
- GUI waits for acknowledgment before rendering at new size
- Eliminates the async lag problem

## Files to Modify

| File                                               | Changes                                |
| -------------------------------------------------- | -------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Fix spawn-time sizing                  |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Add synchronization logic              |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Track received vs sent sizes           |
| `ts3/termsurf-profile/src/main.rs`                 | Send resize acknowledgment (if needed) |

## Success Criteria

- [ ] No black borders appear during or after window resize
- [ ] Webview fills exact pixel bounds of pane (no blank space at edges)
- [ ] Resize behavior matches ts2's accuracy
- [ ] No regression in debounce functionality (still batches rapid resizes)

---

## Experiments

### Experiment 1: Diagnostic Logging

Add detailed logging to understand the exact size mismatches at each stage.

#### Goal

Capture concrete data showing:

1. What size we request at spawn time
2. What size the profile server actually renders
3. What viewport size we render to
4. Where and why mismatches occur

#### Log Points

**1. Spawn-time sizing** (`webview_socket.rs`, in `open_webview` handler):

```rust
log::info!(
    "[SPAWN-SIZE] pane={} grid={}x{} cell={}x{} physical={}x{} scale={:.2} logical={}x{}",
    pane_id, dims.cols, dims.viewport_rows,
    cell_width, cell_height,
    physical_width, physical_height,
    scale, logical_w, logical_h
);
```

**2. Received texture** (`webview_xpc.rs`, when IOSurface arrives):

```rust
log::info!(
    "[TEXTURE-SIZE] pane={} received={}x{} (from mach_port={})",
    pane_id, surface.width, surface.height, surface.mach_port
);
```

**3. Viewport dimensions** (`draw.rs`, in render loop):

```rust
log::info!(
    "[VIEWPORT-SIZE] pane={} viewport={}x{} logical={}x{} scale={:.2}",
    pane_id, viewport_w, viewport_h, logical_w, logical_h, scale
);
```

**4. Size mismatch detection** (`draw.rs`, after getting texture and viewport):

```rust
let texture_physical_w = (surface.width as f32 * scale) as u32;
let texture_physical_h = (surface.height as f32 * scale) as u32;
if texture_physical_w != viewport_w as u32 || texture_physical_h != viewport_h as u32 {
    log::warn!(
        "[SIZE-MISMATCH] pane={} texture_physical={}x{} viewport={}x{} diff=({}, {})",
        pane_id,
        texture_physical_w, texture_physical_h,
        viewport_w as u32, viewport_h as u32,
        texture_physical_w as i32 - viewport_w as i32,
        texture_physical_h as i32 - viewport_h as i32
    );
}
```

**5. Resize command sent** (`draw.rs`, when debounce fires):

```rust
log::info!(
    "[RESIZE-SEND] pane={} logical={}x{} (physical={}x{} at scale={:.2})",
    pane_id, logical_w, logical_h,
    (logical_w as f32 * scale) as u32,
    (logical_h as f32 * scale) as u32,
    scale
);
```

**6. Profile server resize received** (`termsurf-profile/src/main.rs`):

```rust
log::info!(
    "[RESIZE-RECV] logical={}x{} scale={:.2} -> physical={}x{}",
    width, height, scale,
    (width as f32 * scale) as u32,
    (height as f32 * scale) as u32
);
```

#### Files to Modify

| File                                               | Log Points                              |
| -------------------------------------------------- | --------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | SPAWN-SIZE                              |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | TEXTURE-SIZE                            |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | VIEWPORT-SIZE, SIZE-MISMATCH, RESIZE-SEND |
| `ts3/termsurf-profile/src/main.rs`                 | RESIZE-RECV                             |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# In terminal:
web google.com

# Resize the window by dragging edges
# Check logs:
tail -f /tmp/termsurf-gui.log | grep -E "\[(SPAWN|TEXTURE|VIEWPORT|SIZE|RESIZE)"
tail -f /tmp/termsurf-profile-*.log | grep "RESIZE-RECV"
```

#### Expected Findings

1. **If Issue 1 (borders)**: SIZE-MISMATCH logs will show texture lagging behind
   viewport during resize. Texture catches up after XPC round-trip.

2. **If Issue 2 (blank space)**: SPAWN-SIZE will show grid-based calculation
   (cols × cell_width) differs from VIEWPORT-SIZE (exact pixels from layout).

3. **Precision loss**: Compare RESIZE-SEND logical dimensions with TEXTURE-SIZE.
   If they differ, truncation during `as u32` conversion is the culprit.

#### Success Criteria

- [x] Logs clearly show the source of size mismatches
- [ ] Can correlate texture/viewport divergence with visible borders
- [ ] Can measure latency between RESIZE-SEND and TEXTURE-SIZE update

#### Result: FAILED

The experiment failed to meet 2 of 3 success criteria.

#### Conclusion

**Bug in the diagnostic code itself:**

The SIZE-MISMATCH calculation assumes `surface.width/height` are logical pixels
and multiplies by scale:

```rust
let texture_physical_w = (surface.width as f32 * scale) as u32;
```

However, the texture dimensions from IOSurface are already in **physical**
pixels. Multiplying by scale (2.0 on Retina) incorrectly doubles them.

Evidence from logs:

```
[TEXTURE-SIZE] pane=1 received=1872x2190 (mach_port=...)
[SIZE-MISMATCH] pane=1 texture_physical=3744x4380 viewport=1547x1950 diff=(2197, 2430)
```

The diagnostic computes 1872 × 2 = 3744, but 1872 is already the physical size.

**Actual mismatch (correcting for the bug):**

| Dimension | Texture (physical) | Viewport (physical) | Diff   |
| --------- | ------------------ | ------------------- | ------ |
| Width     | 1872               | 1547                | +325   |
| Height    | 2190               | 1950                | +240   |

The texture is **larger** than the viewport, not smaller. This confirms Issue 2
from the Summary: the texture is sized based on grid dimensions (cols ×
cell_width) while the viewport uses exact pixel bounds from the pane layout.

**Key insight:**

The SIZE-MISMATCH comparison should be direct physical-to-physical:

```rust
if surface.width != viewport_w as u32 || surface.height != viewport_h as u32
```

Not `surface.width * scale` vs `viewport_w`.

**Next steps:**

1. Fix the SIZE-MISMATCH diagnostic to compare physical-to-physical
2. Investigate why spawn-time sizing produces larger dimensions than render-time
   viewport (grid-based calculation vs exact pixel bounds)

### Experiment 2: Fix Diagnostic and Measure Async Lag

Fix the bugs from Experiment 1 and add timestamp-based latency measurement.

#### Goal

1. Correct the SIZE-MISMATCH comparison to physical-to-physical
2. Add timestamps to measure latency between RESIZE-SEND and TEXTURE-SIZE
3. Determine if borders appear due to async lag during resize transitions

#### Changes

**1. Fix SIZE-MISMATCH in `draw.rs`:**

The texture dimensions from IOSurface are already physical pixels. Compare
directly without multiplying by scale:

```rust
// BEFORE (wrong):
let texture_physical_w = (surface.width as f32 * scale) as u32;
let texture_physical_h = (surface.height as f32 * scale) as u32;
if texture_physical_w != viewport_w as u32 ...

// AFTER (correct):
if surface.width != viewport_w as u32 || surface.height != viewport_h as u32 {
    log::warn!(
        "[SIZE-MISMATCH] pane={} texture={}x{} viewport={}x{} diff=({}, {})",
        pane_id,
        surface.width, surface.height,
        viewport_w as u32, viewport_h as u32,
        surface.width as i32 - viewport_w as i32,
        surface.height as i32 - viewport_h as i32
    );
}
```

**2. Add timestamp to RESIZE-SEND in `draw.rs`:**

Record when resize commands are sent so we can correlate with texture arrival:

```rust
use std::time::SystemTime;

log::info!(
    "[RESIZE-SEND] pane={} logical={}x{} physical={}x{} timestamp={:?}",
    pane_id, logical_w, logical_h,
    (logical_w as f32 * scale) as u32,
    (logical_h as f32 * scale) as u32,
    SystemTime::now()
);
```

**3. Add timestamp to TEXTURE-SIZE in `webview_xpc.rs`:**

Record when textures arrive to measure round-trip latency:

```rust
use std::time::SystemTime;

log::info!(
    "[TEXTURE-SIZE] pane={} size={}x{} timestamp={:?}",
    pid, width, height,
    SystemTime::now()
);
```

**4. Add transition logging in `draw.rs`:**

Log when texture size doesn't match viewport during render (the moment borders
would be visible):

```rust
if surface.width < viewport_w as u32 || surface.height < viewport_h as u32 {
    log::warn!(
        "[BORDER-VISIBLE] pane={} texture={}x{} < viewport={}x{} gap=({}, {})",
        pane_id,
        surface.width, surface.height,
        viewport_w as u32, viewport_h as u32,
        viewport_w as i32 - surface.width as i32,
        viewport_h as i32 - surface.height as i32
    );
}
```

#### Files to Modify

| File                                            | Changes                           |
| ----------------------------------------------- | --------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Fix SIZE-MISMATCH, add timestamps |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add timestamp to TEXTURE-SIZE     |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# In terminal:
web google.com

# Resize window rapidly, then stop
# Check logs for latency:
grep -E "\[(RESIZE-SEND|TEXTURE-SIZE)\]" /tmp/termsurf-gui.log | tail -20

# Check for border-visible moments:
grep "BORDER-VISIBLE" /tmp/termsurf-gui.log

# Calculate latency by comparing timestamps between RESIZE-SEND and TEXTURE-SIZE
```

#### Expected Findings

1. **SIZE-MISMATCH** will show actual physical-to-physical comparison (texture
   larger than viewport by ~325×240 on steady state)

2. **BORDER-VISIBLE** will fire during resize transitions when old texture
   hasn't caught up to new larger viewport

3. **Latency measurement** will show time between RESIZE-SEND and corresponding
   TEXTURE-SIZE (expected: 10-50ms for XPC round-trip + CEF render)

#### Success Criteria

- [ ] SIZE-MISMATCH shows correct physical-to-physical comparison
- [ ] Can measure latency between RESIZE-SEND and TEXTURE-SIZE
- [ ] BORDER-VISIBLE logs correlate with visible borders during resize
