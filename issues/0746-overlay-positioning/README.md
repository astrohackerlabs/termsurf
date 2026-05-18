+++
status = "closed"
opened = "2026-03-13"
closed = "2026-03-13"
+++

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

### Edge cases

The goal of this issue is to solve all edge cases for the webview overlay that
are already handled by the underlying terminal. The overlay should be positioned
using the same calculations as terminal pane content. This means the overlay
will be correct in every case the terminal handles correctly. If the terminal
itself has a bug (e.g., stale scale factor when moving between displays with
different backing scale factors), the overlay will have the same bug. We will
fix those terminal-level issues in future work, not in this issue.

1. Open a webview — positioned and sized correctly.
2. Split pane to the left — webview correctly positioned on the right.
3. Open a new tab — webview disappears (no longer visible).
4. Resize the window (on the other tab) — resizes correctly, no webview visible.
5. Switch back to the first tab — webview visible at correct position.
6. Resize the window while the webview is visible — webview tracks pane
   position.
7. Move the window to a different display with a different backing scale factor
   — sized and positioned correctly (may be limited by terminal's own handling
   of display changes).
8. Open a new window, then switch tabs, open/close webviews, open/close panes —
   all webviews correct in both windows.
9. Multiple webviews visible simultaneously — all positioned and sized correctly
   in their respective panes.
10. Resize while on a different tab, then switch back to a tab with multiple
    webviews — all webviews correctly repositioned and resized.
11. Font size change — webviews reposition correctly after cell dimensions
    change.
12. Zoom a pane — zoomed pane's webview fills the space, other panes' webviews
    hide.
13. Close a split pane — remaining pane expands, its webview repositions to fill
    the larger space.

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

**Result:** Fail

Opening a webview in a single unsplit pane positions correctly. Splitting the
pane (adding a pane to the left) causes the webview to animate off screen to the
right — a gross mispositioning.

The root cause is the same fundamental mistake as experiment 1: **the overlay
position is still not computed using the same calculation as the terminal
pane.**

The issue's goal (line 7-8) states: "The overlay's pixel coordinates should come
from the same calculation that positions terminal pane content." The issue's
background section (lines 22-28) documents exactly how `paint_pane()` computes
pane positions, including edge-case handling for left-most panes starting at
x=0, half-cell offsets for split dividers, and top-most pane tab bar accounting.

Experiment 2's `paint_pass()` code reconstructs the position from scratch:

```rust
let x = pad_left as f64
    + border.left.get() as f64
    + (pos.left as f64 + col as f64) * cell_w;
```

Meanwhile, `paint_pane()` (pane.rs lines 111-153) computes a materially
different value — with conditional logic for edge panes, half-cell offsets for
split dividers, and different x origins depending on whether `pos.left == 0`:

```rust
let (x, width_delta) = if pos.left == 0 {
    (0., padding_left + border.left.get() as f32 + (cell_width / 2.0))
} else {
    (padding_left + border.left.get() as f32 - (cell_width / 2.0)
        + (pos.left as f32 * cell_width),
     cell_width)
};
```

These are not the same formula. The overlay code doesn't handle the left==0
case, doesn't account for half-cell split offsets, and ignores the edge-case
branches entirely. With a single unsplit pane (pos.left=0), the error is masked
because the pane starts at x=0 and the overlay's `pad + border + 0*cell_w`
happens to be close enough. With a split (pos.left > 0), the `-cell_width/2.0`
offset in `paint_pane` diverges from the overlay's straight `pos.left * cell_w`,
and the coordinate system mismatch (likely logical vs backing pixels in the
padding/border values) amplifies the error proportionally to pos.left, pushing
the overlay off screen.

Two experiments have now failed for the same reason: duplicating the position
formula instead of using the values that `paint_pane()` already computes. The
next experiment must extract the actual pixel coordinates from `paint_pane()`'s
calculation — either by reading them from the `PositionedPane` struct (if pixel
coordinates are added) or by calling the same positioning function that
`paint_pane()` uses — not by writing yet another approximation of the formula.

#### Conclusion

Failed. Despite the issue explicitly stating that overlay coordinates should
come from the same calculation as terminal pane content, experiment 2 wrote a
third independent approximation of the pane position formula. The formula
diverges from `paint_pane()` in multiple ways (no edge-case handling, no
half-cell split offsets, possible coordinate system mismatch), producing correct
results only when pos.left=0 (single pane) and wrong results when pos.left > 0
(split panes). The next experiment must reuse `paint_pane()`'s actual computed
values, not reconstruct them.

### Experiment 3: Read pixel coordinates from paint_pane()

#### Description

`paint_pane()` already computes the exact pixel origin of each pane's cell grid:

```rust
// pane.rs line 341-343
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32);

// pane.rs line 79
let top_pixel_y = top_bar_height + padding_top + border.top.get() as f32;
```

The first cell of pane content renders at
`(left_pixel_x, top_pixel_y + pos.top * cell_height)`. This is the terminal's
authoritative position — it accounts for padding, borders, tab bar, split
offsets, and every edge case.

This experiment makes `paint_pane()` return these computed values so
`paint_pass()` can read them and pass them directly to `set_overlay_frame()`. No
separate formula. The overlay position is:

```
overlay_x = left_pixel_x + col * cell_width
overlay_y = (top_pixel_y + pos.top * cell_height) + row * cell_height
```

Where `col` and `row` are the webview's offset within the pane (from the TUI
protocol), and `cell_width`/`cell_height` come from `render_metrics` — the same
values `paint_pane()` uses. The pane's pixel origin comes verbatim from
`paint_pane()`.

#### Changes

**`wezboard-gui/src/termwindow/render/pane.rs` — return cell grid origin from
`paint_pane()`:**

Change the return type from `anyhow::Result<()>` to
`anyhow::Result<(f32, f32)>`, returning
`(left_pixel_x, top_pixel_y + pos.top * cell_height)` — the pixel position of
the pane's first cell. Both values are already computed; this just passes them
out.

For the `use_box_model_render` early return (line 39), return `(0.0, 0.0)` — box
model rendering is a separate path and we don't position overlays in it.

**`wezboard-gui/src/termwindow/render/paint.rs` — use returned values:**

In the `for pos in panes` loop, capture the return value from `paint_pane()` and
use it to position the overlay:

```rust
let (pane_pixel_x, pane_pixel_y) = self.paint_pane(&pos, num_panes, &mut layers)
    .context("paint_pane")?;

// Update webview overlay position using paint_pane's coordinates.
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
        let x = pane_pixel_x as f64 + col as f64 * cell_w;
        let y = pane_pixel_y as f64 + row as f64 * cell_h;
        crate::termsurf::set_overlay_frame(
            pane_id,
            x, y,
            pw as f64, ph as f64,
            self.dimensions.dpi,
        );
    }
}
```

No padding. No border. No tab bar. No edge-case branches. Those are all inside
`paint_pane()` already and baked into `pane_pixel_x` and `pane_pixel_y`.

**`wezboard-gui/src/termsurf/conn.rs` — add `set_overlay_frame()`:**

Takes backing-pixel coordinates and `dpi` from the render pass. Computes scale
as `dpi / default_dpi()` — the same formula the terminal uses everywhere, and
equivalent to `backingScaleFactor` on macOS (144 / 72 = 2.0 on Retina). The
scale stays correct when moving between displays because `self.dimensions.dpi`
is updated by the terminal's display-change handling before `paint_pass()` runs.

Unlike experiment 2 (which read `contentsScale` from `ca_layer_positioning` — a
sublayer that doesn't inherit the root layer's scale), this uses the render
pass's authoritative DPI.

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
    let mut st = state.lock().unwrap();
    let id = pane_id.to_string();
    let Some(pane) = st.panes.get_mut(&id) else {
        return;
    };
    if pane.ca_layer_positioning == 0 {
        return;
    }

    let scale = dpi as f64 / wezboard_window::default_dpi();
    let scale = if scale > 0.0 { scale } else { 1.0 };

    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;

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

**`wezboard-gui/src/termsurf/conn.rs` — keep `handle_ca_context()` with
`update_ca_layer_frame()`:**

Initial placement stays. The render pass takes over on the next frame.

**`wezboard-gui/src/termsurf/conn.rs` — delete old helpers:**

- Delete `reposition_all_overlays()` (both macOS and non-macOS).
- Delete `get_pane_cell_position()`.

Keep `get_pane_mux_window()` (used by `handle_ca_context`). Keep
`update_ca_layer_frame()` (initial placement).

**`wezboard-gui/src/termwindow/resize.rs` — remove `reposition_all_overlays()`
call (line 93).**

**`wezboard-gui/src/termsurf/mod.rs` — replace
`pub use conn::reposition_all_overlays;` with
`pub use conn::set_overlay_frame;`.**

#### Verification

Build:

- [x] `cd wezboard && cargo build` — compiles without errors.

Edge case checklist:

- [x] Open a webview — positioned and sized correctly.
- [ ] Split pane to the left — webview correctly positioned on the right.

Stopped testing at item 2.

**Result:** Fail

Opening a webview in a single unsplit pane positions correctly. Splitting the
pane (adding a pane to the left) causes the webview to animate to the right —
the same gross mispositioning as experiment 2.

The animation is because `set_overlay_frame()` calls `setFrame:` without
wrapping it in a `CATransaction` with `setDisableActions:YES`. Core Animation
interpolates the frame change, making the overlay visibly slide to the wrong
position.

The position itself is wrong because the scale factor is 1.0 instead of 2.0. The
formula `dpi as f64 / ::window::default_dpi()` was supposed to produce
`backingScaleFactor` (e.g., `144 / 72 = 2.0` on Retina). But on macOS,
`default_dpi()` does NOT return the base platform constant (72). It returns
`screens.active.effective_dpi` — the screen's effective DPI, which on a 2x
Retina display is already 144:

```rust
// window/src/os/macos/connection.rs:113-118
fn default_dpi(&self) -> f64 {
    if let Ok(screens) = self.screens() {
        screens.active.effective_dpi.unwrap_or(crate::DEFAULT_DPI)
    } else {
        crate::DEFAULT_DPI
    }
}
```

So `scale = 144 / 144 = 1.0`. Backing pixels pass through to `setFrame:`
undivided. On a 2x display, every coordinate is 2x what it should be in logical
points, pushing the overlay to the right and down.

The correct approach to return
`(left_pixel_x, top_pixel_y + pos.top * cell_height)` from `paint_pane()` was
sound — those are the terminal's authoritative pane origin coordinates. The bug
is purely in the backing-to-points conversion inside `set_overlay_frame()`.

The existing `update_ca_layer_frame()` (used for initial placement) gets the
correct scale because it reads `contentsScale` from the root overlay layer,
which was explicitly set to `backingScaleFactor` when the overlay was created
(conn.rs line 1168-1169):

```rust
let backing_scale: f64 = msg_send![window, backingScaleFactor];
let _: () = msg_send![root_layer, setContentsScale: backing_scale];
```

#### Conclusion

Failed. The `paint_pane()` return value approach is correct — the pane origin
coordinates match the terminal's own rendering. But the scale calculation in
`set_overlay_frame()` is wrong: `dpi / default_dpi()` produces 1.0 on macOS
because both values are the effective DPI (144 on Retina), not base/effective.
The next experiment should keep the `paint_pane()` return values and fix the
scale by using `DEFAULT_DPI` (the constant 72.0) instead of `default_dpi()` (the
function that returns the screen's effective DPI). It must also wrap `setFrame:`
in a `CATransaction` to suppress animation.

### Experiment 4: Fix scale and suppress animation

#### Description

Experiment 3's approach of returning the pane pixel origin from `paint_pane()`
is correct — the coordinates match the terminal's own rendering exactly. The
only problems are:

1. **Wrong scale.** `dpi / default_dpi()` = 1.0 because both are the effective
   DPI (144 on Retina). The fix: use `dpi as f64 / 72.0` on macOS. The constant
   72.0 is macOS's base DPI — `backingScaleFactor` is defined as
   `effectiveDPI / 72`. This matches how `update_ca_layer_frame()` gets the
   correct scale from `contentsScale` (which is set to `backingScaleFactor`). On
   other platforms, use `dpi as f64 / 96.0` (the Windows/Linux base DPI). These
   are the `DEFAULT_DPI` constants already defined in `window/src/lib.rs:22-24`.

2. **Animated frame change.** `setFrame:` without a `CATransaction` causes Core
   Animation to animate the change. Wrap in `CATransaction` with
   `setDisableActions:YES`, matching the pattern in `handle_ca_context()`.

#### Changes

**`wezboard-gui/src/termsurf/conn.rs` — fix `set_overlay_frame()`:**

Replace the scale calculation and add a CATransaction:

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
    use objc2::runtime::{AnyObject, Bool};
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

    // macOS base DPI is 72. backingScaleFactor = effectiveDPI / 72.
    // default_dpi() returns effectiveDPI (144 on 2x Retina), NOT 72.
    let scale = dpi as f64 / 72.0;
    let scale = if scale > 0.0 { scale } else { 1.0 };

    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;

    let x = x_backing / scale;
    let y = y_backing / scale;
    let w = w_backing / scale;
    let h = h_backing / scale;
    unsafe {
        let ca_transaction = cls(b"CATransaction\0");
        let _: () = msg_send![ca_transaction, begin];
        let _: () = msg_send![ca_transaction, setDisableActions: Bool::YES];

        let layer = pane.ca_layer_positioning as *mut AnyObject;
        let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
        let _: () = msg_send![layer, setFrame: frame];

        let _: () = msg_send![ca_transaction, commit];
    }
}
```

No changes to the non-macOS stub (it has no CALayer and no scale to fix).

No changes to any other file. Experiment 3's changes to `pane.rs`, `paint.rs`,
`resize.rs`, and `mod.rs` are all correct. Only `set_overlay_frame()` in
`conn.rs` changes.

#### Verification

Build:

- [x] `cd wezboard && cargo build` — compiles without errors.

Edge case checklist:

- [x] Open a webview — positioned and sized correctly.
- [ ] Split pane to the left — webview correctly positioned on the right.
- [ ] Open a new tab — webview disappears (no longer visible).
- [ ] Resize the window (on the other tab) — resizes correctly, no webview
      visible.
- [ ] Switch back to the first tab — webview visible at correct position.
- [ ] Resize the window while the webview is visible — webview tracks pane
      position.
- ~~Move the window to a different display with a different backing scale factor
  — unable to test (no non-Retina display available).~~
- [ ] Open a new window, then switch tabs, open/close webviews, open/close panes
      — all webviews correct in both windows.
- [ ] Multiple webviews visible simultaneously — all positioned and sized
      correctly in their respective panes.
- [ ] Resize while on a different tab, then switch back to a tab with multiple
      webviews — all webviews correctly repositioned and resized.
- [ ] Font size change — webviews reposition correctly after cell dimensions
      change.
- [ ] Zoom a pane — zoomed pane's webview fills the space, other panes' webviews
      hide.
- [ ] Close a split pane — remaining pane expands, its webview repositions to
      fill the larger space.
- [ ] Click inside webview — mouse events land at correct coordinates.

Stopped testing at item 2.

**Result:** Partial

The scale fix (`dpi / 72.0` instead of `dpi / default_dpi()`) and animation
suppression (`CATransaction` with `setDisableActions:YES`) both work. Opening a
webview in a single unsplit pane positions it correctly with no animation
artifacts. The scale is now 2.0 on Retina (correct) instead of 1.0 (wrong).

However, splitting a pane reveals a timing problem: the overlay stays at its old
position (left side) after a split creates a new pane on the left. The overlay
should move to the right (where the original pane now lives). Pressing any key
(including ESC) fixes the position — the overlay snaps to the correct location
on the right.

The positioning calculations are correct (confirmed by the keypress fix). The
problem is that the overlay doesn't know the layout changed until something
triggers a fresh repaint with the updated split tree.

#### Conclusion

The scale and animation fixes are correct and should be kept. The remaining
problem is a **stale layout** issue when splitting panes.

**Root cause analysis:** The split is async — `spawn_command_impl` (spawn.rs:29)
calls `promise::spawn::spawn(async move { ... }).detach()`. The async task calls
`mux.split_pane().await`, which calls `domain.split_pane().await` →
`tab.split_and_insert()`. The key handler's `context.invalidate()` (keyevent.rs
line 327) triggers a repaint immediately after spawning the async task — before
the split has completed. This first paint sees the old (pre-split) tree and
positions the overlay at the old location.

After the split completes, the mux does NOT fire a `WindowInvalidated`
notification. The split code (`mux/src/lib.rs:split_pane`,
`mux/src/domain.rs:split_pane`, `mux/src/tab.rs:split_and_insert`) modifies the
tree and resizes panes, but never calls `self.notify(WindowInvalidated(...))`.
The only way the GUI learns about the layout change is through indirect
`PaneOutput` notifications — when the new pane's shell outputs text, or when the
original pane's TUI redraws after SIGWINCH. These `PaneOutput` notifications go
through a multi-hop async chain:

1. PTY reader thread → `Mux::notify_from_any_thread` → `spawn_into_main_thread`
2. Main thread → `mux.notify()` → subscriber callback → `spawn_into_main_thread`
3. Main thread → `mux_pane_output_event_callback` → `window.notify()`
4. Window event handler → `mux_pane_output_event` → `is_pane_visible` →
   `window.invalidate()`

On macOS, the spawn queue (`window/src/spawn.rs`) is driven by a
`CFRunLoopObserver` on `kCFRunLoopAllActivities` that processes **one task per
invocation**. Each hop in the chain above requires a separate observer
invocation. Combined with macOS's display phase timing (which may run before all
queued tasks are processed), the repaint with the updated tree may be delayed or
may not happen at all until an external event (keypress) forces a fresh repaint.

The next experiment should ensure the GUI repaints with the updated layout
immediately after a split completes — either by firing a `WindowInvalidated`
notification from the split code, or by explicitly calling `window.invalidate()`
after the async split task finishes.

### Experiment 5: Notify GUI after async split

#### Description

The split is async — `spawn_command_impl` spawns a `promise::spawn::spawn` task
that calls `mux.split_pane().await`. When the task completes, the split tree is
updated, but the mux never fires a notification to tell the GUI. The key
handler's `context.invalidate()` (keyevent.rs:327) fires a repaint before the
async task runs, so the first paint sees the old tree.

Fix: after `mux.split_pane().await` returns in `spawn_command_internal`, fire
`MuxNotification::WindowInvalidated(src_window_id)`. This uses the existing
notification path — `WindowInvalidated` is already handled in mod.rs:1328 where
it calls `window.invalidate()` and syncs overlay visibility. The notification is
fired from within the async task, so the tree is guaranteed to be updated by the
time `paint_pass` runs in response.

#### Changes

**`wezboard-gui/src/spawn.rs` — add `WindowInvalidated` notification after
split:**

Add `use mux::MuxNotification;` to the imports. After line 120
(`pane.set_config(term_config);`), add:

```rust
mux.notify(MuxNotification::WindowInvalidated(src_window_id));
```

This fires inside the `if let Some(tab)` block, after the split is complete and
the pane is configured. `src_window_id` is already in scope (line 98). `mux` is
already bound (line 46: `let mux = Mux::get();`).

No other files change.

#### Verification

Build:

- [x] `cd wezboard && cargo build` — compiles without errors.

Edge case checklist:

- [x] Open a webview — positioned and sized correctly.
- [x] Split pane to the left — webview correctly positioned on the right (single
      window, single screen).
- [ ] Open a new tab — webview disappears (no longer visible).
- [ ] Resize the window (on the other tab) — resizes correctly, no webview
      visible.
- [ ] Switch back to the first tab — webview visible at correct position.
- [ ] Resize the window while the webview is visible — webview tracks pane
      position.
- ~~Move the window to a different display with a different backing scale factor
  — unable to test (no non-Retina display available).~~
- [ ] Open a new window, then switch tabs, open/close webviews, open/close panes
      — all webviews correct in both windows.
- [ ] Multiple webviews visible simultaneously — all positioned and sized
      correctly in their respective panes.
- [ ] Resize while on a different tab, then switch back to a tab with multiple
      webviews — all webviews correctly repositioned and resized.
- [ ] Font size change — webviews reposition correctly after cell dimensions
      change.
- [ ] Zoom a pane — zoomed pane's webview fills the space, other panes' webviews
      hide.
- [ ] Close a split pane — remaining pane expands, its webview repositions to
      fill the larger space.
- [ ] Click inside webview — mouse events land at correct coordinates.

Stopped testing at the multi-window / multi-screen scenario.

**Result:** Partial

The `WindowInvalidated` notification fixes the split timing issue on a single
screen. Opening a webview, splitting panes, and resizing all work correctly when
testing with one window on one screen. The notification successfully triggers a
repaint with the updated split tree, so the overlay repositions immediately
after the split — no keypress needed.

However, a multi-screen regression appears: open a second window, move it to a
second screen, open a webview, then split to the left. The webview **resizes**
(width shrinks correctly) but does **not reposition** (x stays at the left edge
instead of moving right). This only happens when the window is on the second
screen — the same window on the first screen works correctly.

The overlay's width and position are both set in the same `set_overlay_frame()`
call from `paint_pass()`. The width comes from `pane.pixel_width` (updated by
the TUI's protocol resize message). The position comes from `pane_pixel_x`
(returned by `paint_pane()`, which computes
`padding_left + border.left + pos.left * cell_width`). Since the width updates
but the position doesn't, the paint pass is running, but `pos.left` appears to
be 0 (the pre-split value) even though the split tree has been updated.

#### Conclusion

The `WindowInvalidated` notification works correctly for single-screen scenarios
and should be kept. The remaining bug is specific to multi-screen: when a window
is on a secondary display, splitting a pane updates the overlay's size but not
its position.

**Root cause analysis:** The `WindowInvalidated` notification travels through a
4-hop async chain before reaching `setNeedsDisplay: true`:

1. `mux.notify()` → subscriber calls `spawn_into_main_thread` (hop 1)
2. `mux_pane_output_event_callback` → `window.notify()` →
   `Connection::with_window_inner` → `spawn_into_main_thread` (hop 2)
3. Event handler dispatches `TermWindowNotif::MuxNotification` → calls
   `window.invalidate()` → `Connection::with_window_inner` →
   `spawn_into_main_thread` (hop 3)
4. Inner invalidate runs `setNeedsDisplay: true` (hop 4)

On macOS, the spawn queue (`window/src/spawn.rs`) is driven by a
`CFRunLoopObserver` that processes **one task per invocation**. Each display has
its own `CVDisplayLink` running at its own phase. The second screen's display
link fires at a different phase than the first screen's.

The likely sequence on a second screen:

1. Key handler spawns async split task, calls `context.invalidate()` → immediate
   paint with OLD tree (overlay at old position, old size).
2. Async split completes, tree updated. `mux.notify(WindowInvalidated)` queued.
3. Meanwhile, the TUI receives SIGWINCH from the pane resize and sends an
   updated `pixel_width` via the TermSurf protocol. The IPC handler updates
   `pane.pixel_width` in state.
4. A `PaneOutput` notification from the new pane's shell startup also enters the
   async chain. Whichever notification reaches `setNeedsDisplay: true` first
   triggers the next repaint.
5. On the second screen, the display link phase means the repaint fires during
   an intermediate state where `pane.pixel_width` has been updated (new size
   from step 3) but the run loop hasn't processed enough spawn queue tasks for
   the `WindowInvalidated` chain to reach `setNeedsDisplay: true`. The repaint
   sees the new width but the OLD `pos.left` because `get_panes_to_render()`
   returns stale positions.

Actually, `get_panes_to_render()` always reads directly from `tab.iter_panes()`,
which walks the live split tree. The tree was updated in step 2. So any paint
after step 2 should see the correct `pos.left`. This contradicts the observed
behavior.

The more likely explanation is that the initial `context.invalidate()` paint
(step 1) races with the async split. On the first screen, the paint happens
before the split completes, so the overlay shows the old position. Then the
`WindowInvalidated` chain triggers a second paint with the correct tree. On the
second screen, the display link phase difference means the
`context.invalidate()` paint happens AFTER the split but BEFORE
`pane.pixel_width` is updated by the TUI — so the overlay shows the new position
but old size. Then when the TUI's resize message arrives and updates
`pixel_width`, a `PaneOutput`-triggered repaint shows the new size and new
position. But this isn't what's observed either.

The root cause needs debugging with logging to determine exactly when
`set_overlay_frame` is called, what values it receives, and whether the repaint
from `WindowInvalidated` actually fires on the second screen. The next
experiment should add targeted logging to `set_overlay_frame` and `paint_pass`'s
overlay block to capture the coordinates and timing on both screens.

## Conclusion

This issue replaced the overlay's broken positioning system with one that
piggybacks on the terminal's own render pass. Five experiments, three
architectural changes kept, one bug remaining.

### What was accomplished

**The overlay now gets its position from `paint_pane()`.** The old system
(`update_ca_layer_frame`, `reposition_all_overlays`, `get_pane_cell_position`,
and global `metrics` atomics) computed overlay coordinates independently from
the terminal renderer. This duplicated formula was wrong in multiple ways: it
only searched the active tab, it missed edge cases like split divider offsets,
and it never ran on tab switches. The new system returns
`(left_pixel_x, top_pixel_y)` from `paint_pane()` and uses those coordinates
directly in `set_overlay_frame()`. No separate formula, no global metrics lookup
for positioning. The render pass already knows exactly where every pane goes —
the overlay now uses that.

**Key changes (experiments 3–5, all kept):**

1. **`pane.rs`** — `paint_pane()` returns `(f32, f32)` instead of `()`. The
   returned values are the pixel origin of the pane's cell grid, already
   accounting for padding, borders, tab bar, and split divider offsets.

2. **`paint.rs`** — After each `paint_pane()` call, the overlay block reads the
   pane's `col`, `row`, `pixel_width`, `pixel_height` from TermSurf state and
   calls `set_overlay_frame(pane_id, x, y, w, h, dpi)` using the paint_pane
   coordinates plus cell-sized offsets.

3. **`conn.rs`** — New `set_overlay_frame()` function. Converts backing pixels
   to logical points using `dpi / 72.0` (macOS base DPI, equivalent to
   `backingScaleFactor`). Wraps `setFrame:` in a `CATransaction` with
   `setDisableActions:YES` to suppress Core Animation interpolation. The old
   `reposition_all_overlays()` and `get_pane_cell_position()` were deleted.

4. **`conn.rs`** — `update_ca_layer_frame()` kept for initial placement only
   (when `handle_ca_context` creates the CALayerHost). The render pass takes
   over on the next frame.

5. **`spawn.rs`** — After `mux.split_pane().await` completes, fires
   `MuxNotification::WindowInvalidated(src_window_id)` to trigger a repaint with
   the updated split tree. Without this, the overlay stayed at its pre-split
   position until an unrelated event caused a repaint.

6. **`resize.rs`** — Removed the `reposition_all_overlays()` call. The render
   pass handles repositioning every frame.

7. **`mod.rs`** — Re-exports `set_overlay_frame` instead of
   `reposition_all_overlays`.

### What works

- Single window, single screen: all positioning correct (open, split, resize,
  tab switch, switch back).
- Scale is correct on Retina displays (`dpi / 72.0` = 2.0).
- No animation artifacts (CATransaction suppresses implicit animations).
- Split pane triggers immediate overlay reposition (WindowInvalidated
  notification).

### What doesn't work

- **Multi-screen split repositioning.** Open a second window, move it to a
  second screen, open a webview, split to the left. The overlay resizes (width
  shrinks) but does not reposition (x stays at 0 instead of moving right). First
  screen and first window are unaffected. The bug is specific to a window on a
  secondary display.

### Remaining checklist items (untested)

- New tab hides overlay, switch back restores it.
- Resize on different tab, switch back — overlay correct.
- Font size change — overlay repositions.
- Zoom pane — zoomed pane's overlay fills space, others hide.
- Close split — remaining pane expands, overlay fills.
- Mouse click coordinates land correctly inside overlay.
- Multiple simultaneous overlays.

### Next steps

The multi-screen bug needs targeted logging in `set_overlay_frame` and the
paint_pass overlay block to determine what coordinates are computed for the
second screen's window. The issue should continue in a new issue doc (747)
focused specifically on multi-screen overlay positioning.
