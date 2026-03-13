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
