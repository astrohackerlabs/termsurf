# Issue 746: Fix webview overlay positioning

## Goal

The webview overlay must always appear at the correct position and size, even
after tab switches and window resizes. The overlay's pixel coordinates should
come from the same calculation that positions terminal pane content — not from a
separate, duplicated formula.

## Background

When a webview is open in one pane, switching to another tab, resizing the
window, and switching back causes the webview to be wrongly positioned. Terminal
pane content is fine — only the webview overlay is broken.

### How terminal pane content is positioned (correct)

The render loop computes pane positions fresh every frame:

1. `paint_pass()` calls `tab.iter_panes()`, which walks the split tree and
   returns `PositionedPane` structs with `left`, `top`, `width`, `height` in
   cells.
2. `paint_pane()` converts cell positions to pixel positions using
   `padding_left`, `border.left`, `top_bar_height`, `padding_top`, `border.top`,
   and `cell_width`/`cell_height` from `render_metrics`.
3. Edge cases are handled: left-most panes start at `x=0`, top-most panes
   account for the tab bar, internal panes add half-cell offsets for split
   dividers.

This runs every frame, so positions are always correct — including after tab
switches and window resizes.

### How the webview overlay is positioned (broken)

The overlay position is computed in `update_ca_layer_frame()`
(`wezboard-gui/src/termsurf/conn.rs`):

```rust
let (cell_w, cell_h, origin_x, origin_y, border_left, border_top) = metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x_backing = origin_x + border_left + (pane_left + pane.col) * cell_w;
let y_backing = origin_y + border_top + (pane_top + pane.row) * cell_h;
```

This has three bugs:

**Bug 1: `get_pane_cell_position()` only searches the active tab.** It calls
`w.get_active()` and iterates only that tab's panes. When you're on tab B and
resize the window, `reposition_all_overlays()` tries to look up the tab A pane's
position — but can't find it, so it returns `(0, 0)`.

**Bug 2: The formula doesn't match `paint_pane()`.** The terminal renderer has
edge-case handling (left-most panes start at x=0, half-cell offsets for split
dividers). The overlay code has none of this, so even when it finds the right
pane, the position is slightly wrong.

**Bug 3: No reposition on tab switch.** `reposition_all_overlays()` is only
called from `resize()`. Tab switches don't trigger it, so the stale (wrong)
position persists when switching back.

### Root cause

The overlay code duplicates the pane positioning logic instead of using the same
calculation that `paint_pane()` uses. The terminal rendering knows exactly where
each pane goes (via `PositionedPane` + `paint_pane()`), but this information
never reaches the overlay code.

### Why the duplication exists

The overlay code runs on the TermSurf IPC thread and uses CALayer frames (Core
Animation), while `paint_pane()` runs on the render thread and draws GPU quads.
They're in different parts of the code with different APIs. The overlay code
can't call `paint_pane()` directly.

### Proposed solution

Compute the overlay's pixel position during the render pass — where
`PositionedPane` and all padding/border/tab-bar values are already available —
then update the CALayer frame from those coordinates. This eliminates all three
bugs:

- No separate formula (uses the same calculation as terminal content).
- No active-tab-only lookup (the render pass already has the right
  `PositionedPane`).
- Updates every frame (including tab switches).

The render pass could either update the CALayer directly (if on the main thread)
or write the computed pixel rect to a shared location that the TermSurf code
reads.

### References

- `wezboard/wezboard-gui/src/termwindow/render/pane.rs` — `paint_pane()`,
  background rect calculation (lines 111-153)
- `wezboard/wezboard-gui/src/termwindow/render/paint.rs` — `paint_pass()`,
  iterates panes
- `wezboard/wezboard-gui/src/termsurf/conn.rs` — `update_ca_layer_frame()`,
  `reposition_all_overlays()`, `get_pane_cell_position()`
- `wezboard/wezboard-gui/src/termsurf/metrics.rs` — Global atomic metrics
- `wezboard/wezboard-gui/src/termsurf/state.rs` — `Pane` struct with overlay
  state
- `wezboard/wezboard-gui/src/termwindow/resize.rs` — Resize handler, calls
  `metrics::set()` and `reposition_all_overlays()`
- `wezboard/mux/src/tab.rs` — `iter_panes_impl()`, split tree traversal

## Experiments

### Experiment 1: Position overlay from the render pass

#### Description

Move overlay positioning into `paint_pass()`, where `PositionedPane` and all
layout values are already computed. Add a new function
`termsurf::set_overlay_frame()` that takes backing-pixel coordinates and a scale
factor, converts to points, and updates the CALayer. Remove the old
metrics-based positioning system.

#### Coordinate systems

All values in the render pass are in **backing pixels** (device pixels):

- `dimensions.pixel_width/height` — from `convertRectToBacking` in the macOS
  window layer
- `render_metrics.cell_size` — font rasterized at the backing DPI
- `padding_left`, `border.left`, `top_pixel_y` — derived from the above

CALayer `setFrame:` expects **points**. The conversion is:

```
scale = dimensions.dpi / default_dpi()
points = backing_pixels / scale
```

`default_dpi()` is platform-specific: 72.0 on macOS, 96.0 on Linux/Windows
(`wezboard_window::default_dpi()`, backed by the `DEFAULT_DPI` constant in
`wezboard/window/src/lib.rs`). Wayland already uses this same formula
(`self.dpi / DEFAULT_DPI`). We must not hardcode 72.

This is consistent with how the rest of Wezboard handles scale. The render pass
trusts `self.dimensions.dpi` for all scale-dependent calculations (cell sizes,
font metrics, pixel coordinates). The DPI is guaranteed fresh: on display
changes, `draw_rect()` detects the `screen_changed` flag, calls `did_resize()`
(which reads `backingScaleFactor` from the NSWindow), and skips painting until
the next frame. By the time `paint_pass()` runs, `self.dimensions.dpi` is
current.

#### Overlay position formula

The cell grid for a pane at `(pos.left, pos.top)` starts at:

```
pane_x = padding_left + border_left + pos.left * cell_width    [backing px]
pane_y = top_pixel_y + pos.top * cell_height                   [backing px]
```

Where `top_pixel_y = tab_bar_height + padding_top + border_top`.

The overlay starts at cell `(col, row)` within the pane:

```
overlay_x = pane_x + col * cell_width     [backing px]
overlay_y = pane_y + row * cell_height     [backing px]
overlay_w = pixel_width                    [backing px, from SetOverlay]
overlay_h = pixel_height                   [backing px, from SetOverlay]
```

Convert to points for `setFrame:`:

```
frame = CGRect(
    overlay_x / scale,
    overlay_y / scale,
    overlay_w / scale,
    overlay_h / scale,
)
```

#### Changes

**Add `set_overlay_frame()` to `wezboard-gui/src/termsurf/conn.rs`:**

Takes backing-pixel coordinates and DPI. Computes scale internally using
`wezboard_window::default_dpi()`, then converts to points. The caller passes
`self.dimensions.dpi` — no hardcoded constants.

```rust
#[cfg(target_os = "macos")]
pub fn set_overlay_frame(
    pane_id: usize,
    x_backing: f64,
    y_backing: f64,
    w_backing: f64,
    h_backing: f64,
    dpi: usize,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let Some(state) = super::state::global() else {
        return;
    };
    let st = state.lock().unwrap();
    let id = pane_id.to_string();
    let Some(pane) = st.panes.get(&id) else {
        return;
    };
    if pane.ca_layer_positioning == 0 {
        return;
    }
    let scale = dpi as f64 / wezboard_window::default_dpi();
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let x = x_backing / scale;
    let y = y_backing / scale;
    let w = w_backing / scale;
    let h = h_backing / scale;
    unsafe {
        let layer = pane.ca_layer_positioning as *mut AnyObject;
        let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
        let _: () = msg_send![layer, setFrame: frame];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_overlay_frame(
    _pane_id: usize,
    _x: f64,
    _y: f64,
    _w: f64,
    _h: f64,
    _dpi: usize,
) {}
```

**Call from `paint_pass()` in `wezboard-gui/src/termwindow/render/paint.rs`:**

After the existing `paint_pane()` and `paint_pane_border()` calls (lines
258-260), update the overlay position for each pane:

```rust
for pos in panes {
    // ... existing paint_pane / paint_pane_border calls ...

    // Update webview overlay position from the render pass.
    // All values are in backing pixels, consistent with the rest
    // of the renderer. set_overlay_frame converts to points.
    let pane_id = pos.pane.pane_id();
    let overlay_info = crate::termsurf::state::global().and_then(|state| {
        let st = state.lock().unwrap();
        let id = pane_id.to_string();
        st.panes
            .get(&id)
            .filter(|p| p.ca_layer_positioning != 0)
            .map(|p| (p.col, p.row, p.pixel_width, p.pixel_height))
    });
    if let Some((col, row, pw, ph)) = overlay_info {
        let cell_w = self.render_metrics.cell_size.width as f64;
        let cell_h = self.render_metrics.cell_size.height as f64;
        let (pad_left, pad_top) = self.padding_left_top();
        let border = self.get_os_border();
        let tab_bar_h = if self.show_tab_bar
            && !self.config.tab_bar_at_bottom
        {
            self.tab_bar_pixel_height().unwrap_or(0.) as f64
        } else {
            0.0
        };
        let top_y = tab_bar_h + pad_top as f64 + border.top.get() as f64;
        let x = pad_left as f64
            + border.left.get() as f64
            + (pos.left as f64 + col as f64) * cell_w;
        let y = top_y + (pos.top as f64 + row as f64) * cell_h;
        crate::termsurf::set_overlay_frame(
            pane_id,
            x, y,
            pw as f64, ph as f64,
            self.dimensions.dpi,
        );
    }
}
```

**Keep `metrics::set()` for cell-to-pixel conversion only:**

`handle_set_overlay()` in `conn.rs` (lines 440-450) uses `metrics::get()` to
convert overlay cell dimensions to `pixel_width`/`pixel_height` for the `Resize`
message sent to Chromium. This stays — it's about sizing, not positioning.

**Remove old positioning code from `conn.rs`:**

- Delete `update_ca_layer_frame()` (lines 1363-1407).
- Delete `reposition_all_overlays()` (lines 1412-1440).
- Delete `get_pane_cell_position()` (lines 1332-1360).
- Delete `get_pane_mux_window()` (lines 1303-1328) — only used by
  `reposition_all_overlays()`.
- Remove `update_ca_layer_frame()` calls from `handle_ca_context()`.

**Remove `reposition_all_overlays()` call from `resize.rs`:**

Delete line 93 (`crate::termsurf::reposition_all_overlays();`). The render pass
now handles repositioning every frame.

**Clean up `state.rs` `Pane` struct:**

Remove fields no longer needed:

- `overlay_origin_x: f64` — was cached position, now computed every frame
- `overlay_origin_y: f64` — same
- `overlay_scale: f64` — same

Remove all assignments to these fields (in `handle_set_overlay`,
`handle_ca_context`, `update_ca_layer_frame`).

#### Verification

1. Open a webview in a pane. It displays at the correct position.
2. Split the pane. The webview stays correctly positioned in its pane.
3. Switch to a different tab, resize the window, switch back. The webview is at
   the correct position and size.
4. Resize the window while the webview tab is active. The webview tracks the
   pane position correctly.
5. Open a second webview in a split pane. Both overlays are correctly
   positioned.

**Result:** Fail

Two problems observed:

**Problem 1: Overlay starts at (0,0).** When a webview first opens, the overlay
appears at the window origin (0,0) — ignoring all padding, borders, and tab bar.
It stays there until something triggers a repaint (like a keypress).

Root cause: We removed the `update_ca_layer_frame()` call from
`handle_ca_context()`, relying on `paint_pass()` to position it on the next
frame. But `paint_pass()` only runs when something triggers a repaint. After
`handle_ca_context` adds the CALayer to the window's layer tree, there is no
guaranteed repaint — the terminal content hasn't changed, so nothing invalidates
the frame. The CALayer sits at its default (0,0) position until an external
event (keypress, mouse move) forces a repaint.

The old code positioned the overlay immediately inside `handle_ca_context`
because it couldn't wait — there's no guarantee the render loop will run again
soon. Deferring to the render pass created a race condition where the CALayer
exists but hasn't been positioned yet.

**Problem 2: After keypress, overlay moves but to the wrong position (too far
down and to the right).** When a repaint finally happens and `paint_pass()`
runs, the computed position overshoots.

Root cause: The formula double-counts the pane's cell offset. `pos.left` and
`pos.top` from `PositionedPane` already represent the pane's absolute cell
position within the tab grid. But we then add `col` and `row` (the overlay's
cell offset within the pane) using the same multiplication:

```rust
let x = pad_left + border_left + (pos.left + col) * cell_w;
let y = top_y + (pos.top + row) * cell_h;
```

This is correct for the _content start_ of the overlay within the pane. But
`paint_pane()` positions text at
`padding_left + border.left + pos.left * cell_width` — the pane origin, not the
overlay offset. The overlay's `(col, row)` offset is an additional displacement
within the pane. So the formula itself is arithmetically correct, but the
position it computes doesn't match what `paint_pane()` would give for the same
pane corner, because `paint_pane()` starts at `(pos.left, pos.top)` and the
overlay adds `(col, row)` on top.

The more likely cause: `padding_left_top()` and `get_os_border()` return values
in backing pixels (f32), and all the render pass arithmetic is in backing
pixels. But the old code used the `metrics::get()` values (stored as u32 from
the resize handler), which may have been computed differently or at a different
time. The values from `padding_left_top()` might not exactly match what
`metrics::set()` stored, leading to a systematic offset.

Additionally, `tab_bar_pixel_height()` returns `Result<f32>` and the old code
path through `metrics::set()` in `resize.rs` computed `top_bar_height` slightly
differently (as a single `f32` passed to `metrics::set`). Any floating-point
discrepancy between the two paths would show up as a position error.

#### Conclusion

The approach of deferring overlay positioning entirely to the render pass has
two fundamental problems:

1. **Timing gap.** The CALayer must be positioned when it's created, not on the
   next repaint. There's no guarantee a repaint happens promptly after
   `handle_ca_context`. The old code positioned immediately because it had to.

2. **Value mismatch.** Even when the render pass does run, computing the
   position from `padding_left_top()`, `get_os_border()`, and
   `tab_bar_pixel_height()` in `paint_pass()` doesn't produce the same result as
   the old metrics-based path. The render pass values and the metrics values may
   differ due to when they're computed and how they're rounded.

The next experiment should keep immediate positioning in `handle_ca_context` (so
the overlay is never at 0,0) and focus on fixing the three original bugs
(active-tab-only lookup, formula mismatch, no reposition on tab switch) without
removing the initial positioning step. The render pass can _update_ the position
every frame, but it must not be the _only_ place that sets it.

### Experiment 2: Render-pass positioning with initial placement

#### Description

Same goal as experiment 1 — compute overlay position from the render pass where
`PositionedPane` and all layout values are already correct — but fix the two
problems that caused experiment 1 to fail:

1. **Keep initial positioning.** `handle_ca_context()` still calls
   `update_ca_layer_frame()` when the CALayer is first created. The render pass
   updates the position every frame after that. The overlay is never at (0,0).

2. **Use `contentsScale` for scale, not `dpi / default_dpi()`.** The old code
   reads `contentsScale` from the actual CALayer — this is the authoritative
   backing scale factor set from `backingScaleFactor` when the overlay is
   created. The new `set_overlay_frame()` reads `contentsScale` from
   `ca_layer_positioning` (which inherits the root layer's scale). No indirect
   DPI calculation.

After the render-pass path is working, remove the old positioning functions that
are no longer needed (`get_pane_cell_position`, `reposition_all_overlays`,
`metrics` module usage for positioning). `update_ca_layer_frame` stays for
initial placement only.

#### Changes

**`wezboard-gui/src/termsurf/conn.rs` — add `set_overlay_frame()`:**

New public function. Takes backing-pixel coordinates, reads `contentsScale` from
the pane's `ca_layer_positioning` layer, converts to points, updates
`overlay_origin_x/y/scale` for input.rs, and sets the CALayer frame.

```rust
#[cfg(target_os = "macos")]
pub fn set_overlay_frame(
    pane_id: usize,
    x_backing: f64,
    y_backing: f64,
    w_backing: f64,
    h_backing: f64,
) {
    use objc2::msg_send;
    use objc2::runtime::AnyObject;
    use objc2_core_foundation::{CGPoint, CGRect, CGSize};

    let Some(state) = super::state::global() else {
        return;
    };
    let mut st = state.lock().unwrap();
    let id = pane_id.to_string();
    let Some(pane) = st.panes.get_mut(&id) else {
        return;
    };
    if pane.ca_layer_positioning == 0 {
        return;
    }

    let layer = pane.ca_layer_positioning as *mut AnyObject;
    let scale: f64 = unsafe { msg_send![layer, contentsScale] };
    let scale = if scale > 0.0 { scale } else { 1.0 };

    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;

    let x = x_backing / scale;
    let y = y_backing / scale;
    let w = w_backing / scale;
    let h = h_backing / scale;
    unsafe {
        let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
        let _: () = msg_send![layer, setFrame: frame];
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_overlay_frame(
    _pane_id: usize,
    _x: f64,
    _y: f64,
    _w: f64,
    _h: f64,
) {}
```

Key difference from experiment 1: scale comes from `contentsScale` on the actual
layer, not from `dpi / default_dpi()`.

**`wezboard-gui/src/termwindow/render/paint.rs` — call from `paint_pass()`:**

After `paint_pane()` and `paint_pane_border()` in the pane loop (after line
260), add the overlay position update. Same formula as experiment 1 — uses
`padding_left_top()`, `get_os_border()`, `tab_bar_pixel_height()`,
`render_metrics.cell_size`, and `PositionedPane.left/top`:

```rust
// Update webview overlay position from the render pass.
{
    let pane_id = pos.pane.pane_id();
    let overlay_info = crate::termsurf::state::global().and_then(|state| {
        let st = state.lock().unwrap();
        let id = pane_id.to_string();
        st.panes
            .get(&id)
            .filter(|p| p.ca_layer_positioning != 0)
            .map(|p| (p.col, p.row, p.pixel_width, p.pixel_height))
    });
    if let Some((col, row, pw, ph)) = overlay_info {
        let cell_w = self.render_metrics.cell_size.width as f64;
        let cell_h = self.render_metrics.cell_size.height as f64;
        let (pad_left, pad_top) = self.padding_left_top();
        let border = self.get_os_border();
        let tab_bar_h = if self.show_tab_bar && !self.config.tab_bar_at_bottom {
            self.tab_bar_pixel_height().unwrap_or(0.) as f64
        } else {
            0.0
        };
        let top_y = tab_bar_h + pad_top as f64 + border.top.get() as f64;
        let x = pad_left as f64
            + border.left.get() as f64
            + (pos.left as f64 + col as f64) * cell_w;
        let y = top_y + (pos.top as f64 + row as f64) * cell_h;
        crate::termsurf::set_overlay_frame(
            pane_id,
            x, y,
            pw as f64, ph as f64,
        );
    }
}
```

**`wezboard-gui/src/termsurf/conn.rs` — keep `handle_ca_context()` unchanged:**

The `update_ca_layer_frame(pane, root_layer)` call at line 1307 stays. This
provides the initial position when the CALayer is first created. The render pass
takes over on the next frame.

**`wezboard-gui/src/termsurf/conn.rs` — delete old positioning helpers:**

After the render-pass path works, delete:

- `get_pane_cell_position()` (lines 1332-1360) — no longer needed; the render
  pass gets pane position from `PositionedPane`.
- `reposition_all_overlays()` (lines 1412-1443) — no longer needed; the render
  pass repositions every frame.

Keep `get_pane_mux_window()` — still used by `handle_ca_context()`. Keep
`update_ca_layer_frame()` — still used for initial placement.

**`wezboard-gui/src/termwindow/resize.rs` — remove `reposition_all_overlays()`
call:**

Delete line 93. The render pass handles repositioning every frame.

**`wezboard-gui/src/termsurf/mod.rs` — update re-exports:**

Replace `pub use conn::reposition_all_overlays;` with
`pub use conn::set_overlay_frame;`.

#### Verification

1. `cd wezboard && cargo build` — compiles without errors.
2. Open a webview — correct position immediately (initial placement from
   `handle_ca_context`).
3. Split the pane — webview stays positioned correctly (render pass updates
   every frame).
4. Switch to a different tab, resize the window, switch back — webview is at the
   correct position (render pass computes from `PositionedPane`, not active-tab
   lookup).
5. Resize while webview tab is active — webview tracks pane position (render
   pass updates every frame, no need for `reposition_all_overlays`).
6. Click inside webview — mouse events land correctly (`overlay_origin_x/y` and
   `overlay_scale` updated every frame by `set_overlay_frame`).
