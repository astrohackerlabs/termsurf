# Issue 727: Wezboard second webview positioning

## Goal

Make two browser overlays visible simultaneously in split panes, each positioned
correctly over its own terminal pane.

## Background

Issue 726 proved the second overlay's full pipeline works — SetOverlay →
CreateTab → TabReady → CaContext → CALayerHost all complete successfully. But
both overlays render at the same screen position because `update_ca_layer_frame`
uses global metrics that don't distinguish between panes.

### The positioning formula

Ghostboard's correct formula (Metal.zig:301–302):

```zig
const x: f64 = @as(f64, grid_col) * cw / scale + pl / scale;
const y: f64 = @as(f64, grid_row) * ch / scale + pt / scale;
```

Where `grid_col`/`grid_row` are per-surface grid coordinates, `cw`/`ch` are cell
dimensions, and `pl`/`pt` are padding_left/padding_top. Each Surface owns its
own positioning layer, so coordinates are relative to the Surface's own area.

Wezboard's current formula (conn.rs:986–987, from Issue 726 Exp 7):

```rust
let x = (origin_x as u64 + pane.col * cell_w as u64) as f64 / scale;
let y = (origin_y as u64 + pane.row * cell_h as u64) as f64 / scale;
```

Where `origin_x` = `padding_left` and `origin_y` = `top_bar_height` (set in
resize.rs:84–89), and `col`/`row` come from the TUI's `viewport_rect.x/y`
(ratatui grid coords relative to the terminal pane, e.g., col=0, row=1).

### Why both formulas look equivalent but aren't

Ghostboard: each Surface has its own layer tree. The positioning layer is a
sublayer of the Surface's own view/layer, so `(0, 0)` means "top-left of this
Surface's rendering area." Adding `padding_left` offsets from the Surface edge
to the grid origin.

Wezboard: ONE shared overlay NSView covers the entire window. All panes'
positioning layers are siblings under the same root layer. So `(0, 0)` means
"top-left of the window." The `origin_x`/`origin_y` values from `metrics::get()`
provide the global offset from the window origin to the content area, but they
say nothing about which pane within the content area.

### What the TUI sends

The TUI sends `viewport_rect.x` and `viewport_rect.y` (main.rs:420–421) as `col`
and `row` in SetOverlay. These are ratatui grid coordinates relative to the
terminal pane:

- Single pane: col=0, row=1 (row 0 is the URL bar)
- The TUI has no knowledge of where its pane sits within the terminal window

### The missing piece

The TUI's col/row are pane-relative grid coordinates, but Wezboard needs
window-relative pixel coordinates to position the CALayer. The conversion
requires knowing where each pane sits in the window — information that WezTerm's
mux/layout system has but the TermSurf connection code currently doesn't access.

### Issue 726 Exp 7 failure

Adding `col * cell_w` to `origin_x` doubled the first overlay's margins (pushed
it down and to the right). The second overlay still didn't appear. This happened
because col=0 and row=1 for the first pane, so:

- x = origin_x + 0 \* cell_w = origin_x (correct, unchanged)
- y = origin_y + 1 \* cell_h = origin_y + cell_h (wrong — shifted down by one
  cell, URL bar offset shouldn't be added to the window origin)

The `origin_y` already positions at the content area top. Adding `row * cell_h`
adds the URL bar row offset again, which is wrong because `origin_y` is the
window-level offset (tab bar height), not the pane-level offset.

### Secondary bug: white flash

When a split opens, the first pane resizes. Chromium re-sends CaContext with the
same context ID. `handle_ca_context` swaps the CALayerHost (remove old, add
new), showing a blank frame briefly. This is cosmetic but worth fixing.

## Approach

The core problem is converting pane-relative grid coordinates to window-absolute
pixel coordinates. There are two possible approaches:

### Approach A: Query WezTerm's pane layout

WezTerm knows exactly where each pane sits in the window. The mux/tab system
tracks pane positions. If we can query a pane's pixel origin from the mux, we
can position the overlay precisely.

Pros: Uses WezTerm's own layout system, naturally correct for splits and
resizes. Cons: Requires finding the right API in WezTerm's codebase.

### Approach B: Per-pane metrics

Instead of global metrics, store per-pane pixel origin. When WezTerm computes
pane layout (during resize or split), update each pane's origin in the shared
state. The positioning formula becomes:

```rust
let x = (pane_origin_x + pane.col * cell_w) as f64 / scale;
let y = (pane_origin_y + pane.row * cell_h) as f64 / scale;
```

Where `pane_origin_x`/`pane_origin_y` are the pixel coordinates of this specific
pane's top-left corner within the window.

Pros: Clean separation, doesn't depend on querying WezTerm APIs at render time.
Cons: Requires hooking into WezTerm's layout recalculation.

### Recommended path

Start with Approach A — find where WezTerm computes per-pane pixel positions and
expose that to the TermSurf connection code. This is the more reliable approach
since WezTerm already calculates these positions for its own rendering.

## Experiment 1: Investigate WezTerm's pane position data

### Hypothesis

WezTerm's mux/tab system tracks per-pane positions in pixel coordinates. If we
can access a pane's pixel origin given its pane_id, we can fix the positioning
formula without adding new state.

### Design

1. Search WezTerm's codebase for how it computes pane positions — look for the
   split layout engine, `PositionedPane`, or similar structures that map pane_id
   to pixel coordinates
2. Trace how the rendering code knows where to draw each pane's content
3. Determine if this information is accessible from the TermSurf connection code
   (which runs on the main thread via `promise::spawn::spawn_into_main_thread`)
4. Document findings and propose the specific code change

### Verification

Research only — no code changes. Success = we know exactly which API to call and
what coordinates it returns.

### Result: Success

Found the complete data path. WezTerm already computes per-pane positions.

**`PositionedPane` struct** (mux/src/tab.rs:58–79):

```rust
pub struct PositionedPane {
    pub index: usize,
    pub is_active: bool,
    pub is_zoomed: bool,
    pub left: usize,        // cell offset from tab top-left
    pub top: usize,         // cell offset from tab top-left
    pub width: usize,       // in cells
    pub pixel_width: usize,
    pub height: usize,      // in cells
    pub pixel_height: usize,
    pub pane: Arc<dyn Pane>,
}
```

`left` and `top` are in **cells**, relative to the tab's top-left corner. For a
vertical split, the right pane has `left = left_pane_width + 1` (1 cell for the
divider). `pixel_width`/`pixel_height` are the pane's pixel dimensions.

**How to get it:** `tab.iter_panes()` returns `Vec<PositionedPane>` for all
panes in the active tab. This is already used at termwindow/mod.rs:1308 inside
the `WindowInvalidated` handler for `sync_overlay_visibility`:

```rust
for positioned in tab.iter_panes() {
    active_ids.insert(positioned.pane.pane_id().to_string());
}
```

**PaneId mapping:** WezTerm's `PaneId` is `usize` (mux/src/pane.rs:25). The TUI
gets its pane_id from `TERMSURF_PANE_ID` env var, which is set as
`pane_id.to_string()` in domain.rs:483. So our String pane_id parses back to
usize for matching against `positioned.pane.pane_id()`.

**Pixel conversion formula** (render/pane.rs:109–136):

```rust
let cell_width = self.render_metrics.cell_size.width as f32;
let cell_height = self.render_metrics.cell_size.height as f32;
let top_pixel_y = top_bar_height + padding_top + border.top;

// For pane at pos.left, pos.top (in cells):
let x = padding_left + border.left + (pos.left as f32 * cell_width);
let y = top_pixel_y + (pos.top as f32 * cell_height);
```

The render code has special cases for `pos.left == 0` and `pos.top == 0`
(extends to window edge), but the overlay doesn't need those — we want the
grid-aligned position.

**Accessibility from TermSurf code:** The TermSurf connection handler runs on
the main thread via `promise::spawn::spawn_into_main_thread`. The Mux is
accessible from the main thread via `Mux::get()`. So we can call
`tab.iter_panes()` from `handle_ca_context` to look up the pane's cell position,
then convert to pixels using the same formula as the render code.

**Proposed formula for `update_ca_layer_frame`:**

```rust
let (cell_w, cell_h, padding_left, top_bar_height) = super::metrics::get();
// Look up pane's cell position from mux
let (pane_left, pane_top) = get_pane_cell_position(pane_id);
// Window-absolute pixel position:
let x = (padding_left + border_left + (pane_left + col) * cell_w) / scale;
let y = (top_bar_height + padding_top + border_top + (pane_top + row) * cell_h) / scale;
```

Where `pane_left`/`pane_top` come from `PositionedPane.left`/`.top`, and
`col`/`row` are the TUI's pane-relative grid offset (0, 1 for URL bar skip).

**Key insight:** `padding_top` and `border.top` are not currently stored in
metrics. We need to either add them to the metrics bridge or access them from
the Mux/TermWindow at query time. The simplest approach: expand `metrics::set()`
to include `padding_top` and `border_width`, since these are already computed in
resize.rs where `metrics::set()` is called.

## Experiment 2: Implement per-pane overlay positioning

### Hypothesis

Using `PositionedPane.left`/`.top` from the mux plus the TUI's col/row, we can
position each overlay correctly over its own pane.

### Design

Three changes:

**1. Expand metrics bridge** (metrics.rs + termwindow/resize.rs)

Add `padding_top` and `border_left`/`border_top` to `metrics::set()` so the
positioning formula has all the values it needs. Currently metrics stores
`(cell_w, cell_h, padding_left, top_bar_height)`. Add `padding_top` and the
border values.

**2. Add pane position lookup** (conn.rs)

Add a helper function that queries the mux for a pane's cell position:

```rust
fn get_pane_cell_position(pane_id: &str) -> (usize, usize) {
    let numeric_id: usize = pane_id.parse().unwrap_or(0);
    let mux = Mux::get();
    for window_id in mux.iter_windows() {
        if let Some(w) = mux.get_window(window_id) {
            if let Some(tab) = w.get_active() {
                for pos in tab.iter_panes() {
                    if pos.pane.pane_id() == numeric_id {
                        return (pos.left, pos.top);
                    }
                }
            }
        }
    }
    (0, 0) // fallback: single pane at origin
}
```

**3. Fix positioning formula** (conn.rs `update_ca_layer_frame`)

```rust
let (cell_w, cell_h, pad_left, top_bar_h, pad_top, border_left, border_top)
    = super::metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x = (pad_left + border_left + (pane_left as u32 + pane.col as u32) * cell_w)
    as f64 / scale;
let y = (top_bar_h + pad_top + border_top + (pane_top as u32 + pane.row as u32) * cell_h)
    as f64 / scale;
```

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch Wezboard, run `web google.com`
3. Single pane overlay renders in correct position (no regression)
4. Open a vertical split, run `web google.com` in the second pane
5. Both overlays visible, each over its own pane
6. Close one pane — remaining overlay repositions correctly

### Result: Failure

Same symptoms as Issue 726 Exp 7. The first pane's overlay is positioned
incorrectly (shifted from its correct location), and the second pane's overlay
still doesn't appear. Adding `pane.col * cell_w` and `pane.row * cell_h` to the
grid origin displaces the first overlay because the TUI sends row=1 (URL bar
offset), which adds an extra `cell_h` to the y position on top of the grid
origin that already accounts for the content area.

The fundamental problem remains: the TUI's col/row are pane-relative viewport
offsets (row=1 means "skip the URL bar row"), NOT window-relative grid
coordinates. Adding them to the window-level grid origin double-counts the
offset. For the single-pane case, col=0 and row=1 — so x stays correct but y
gets pushed down by one cell height.

Additionally, the metrics change (adding `border_left` and
`padding_top + border_top` to the stored origin values) may have further shifted
the first overlay compared to the pre-experiment baseline, since the original
formula only used `padding_left` and `top_bar_height`.

The `get_pane_cell_position` lookup from the mux is architecturally correct, but
the formula for combining grid origin + pane cell offset + TUI viewport offset
needs rethinking. The TUI's row=1 should offset within the pane's area, not be
added to the window-level grid origin.

## Experiment 3: Minimal pane offset (mux lookup only)

### Hypothesis

The current baseline formula positions the overlay correctly for a single pane
(or the left pane in a split). It fails only because it doesn't account for the
pane's cell offset within the tab. Adding ONLY `PositionedPane.left * cell_w`
and `PositionedPane.top * cell_h` to the existing formula — without changing
metrics, without adding the TUI's col/row — will position the overlay correctly
for any pane.

For the left pane, `PositionedPane.left = 0` and `.top = 0`, so the formula
reduces to the baseline (no regression). For the right pane in a vertical split,
`.left > 0` shifts the overlay rightward by the correct amount.

### Why Exp 2 failed

Two unnecessary changes broke it:

1. **Changed metrics values** — added `border_left` and
   `padding_top + border_top` to the stored origin, shifting the first overlay
   from its correct baseline position.
2. **Added TUI's col/row** — the TUI sends `row=1` (skip URL bar), which added
   an extra `cell_height` to y on top of the grid origin. Double-counted.

### Design

One file changed: `conn.rs`. Two modifications:

**1. Add `get_pane_cell_position` helper**

```rust
fn get_pane_cell_position(pane_id: &str) -> (usize, usize) {
    let numeric_id: usize = match pane_id.parse() {
        Ok(id) => id,
        Err(_) => return (0, 0),
    };
    let mux = mux::Mux::get();
    for window_id in mux.iter_windows() {
        if let Some(w) = mux.get_window(window_id) {
            if let Some(tab) = w.get_active() {
                for pos in tab.iter_panes() {
                    if pos.pane.pane_id() == numeric_id {
                        return (pos.left, pos.top);
                    }
                }
            }
        }
    }
    (0, 0)
}
```

**2. Add pane offset to `update_ca_layer_frame`**

Change from:

```rust
let (_, _, origin_x, origin_y) = super::metrics::get();
let x = origin_x as f64 / scale;
let y = origin_y as f64 / scale;
```

To:

```rust
let (cell_w, cell_h, origin_x, origin_y) = super::metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x = (origin_x as u64 + pane_left as u64 * cell_w as u64) as f64 / scale;
let y = (origin_y as u64 + pane_top as u64 * cell_h as u64) as f64 / scale;
```

No changes to metrics.rs, resize.rs, or state.rs. No col/row fields on Pane.

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Single pane: `web google.com` — overlay in correct position (no regression)
3. Split pane, open from RIGHT side — overlay appears over right pane
4. Split pane, open from LEFT side — overlay appears over left pane
5. Close the TUI — overlay removed cleanly

### Result: Failure

The overlay for the right pane is positioned far beyond the visible window —
stretching the window to the right reveals it. The offset is approximately the
full window width, not the left pane's width. This means
`PositionedPane.left * cell_w` is producing a value equal to the full window
width in pixels rather than the left pane's pixel width.

The `PositionedPane.left` value from `iter_panes()` is in cells relative to the
tab, and `cell_w` is the cell width from metrics. If the right pane's `left` is,
say, 80 cells in a 160-column window, then `80 * cell_w` should be half the
window. But the overlay ends up at the full window width, suggesting either
`pane_left` or `cell_w` is wrong, or the pane_id lookup is matching the wrong
pane. Debug logging of the actual `pane_left`, `cell_w`, and resulting `x`
values is needed to diagnose.

## Experiment 4: Debug log pane positions

### Hypothesis

The `get_pane_cell_position` lookup returns wrong values — either the pane_id
isn't matching, or `PositionedPane.left` is larger than expected. Logging the
actual values will reveal why the overlay ends up at the full window width.

### Design

Add logging to two functions in `conn.rs`:

**1. In `get_pane_cell_position`** — log ALL panes returned by `iter_panes()`,
not just the matching one. This shows every pane's id, left, top, width, and
height, revealing what WezTerm thinks the layout is:

```rust
for pos in tab.iter_panes() {
    log::info!(
        "  pane id={} left={} top={} width={} height={} pixel={}x{}",
        pos.pane.pane_id(), pos.left, pos.top,
        pos.width, pos.height, pos.pixel_width, pos.pixel_height
    );
    if pos.pane.pane_id() == numeric_id {
        return (pos.left, pos.top);
    }
}
```

**2. In `update_ca_layer_frame`** — log the inputs and computed output:

```rust
log::info!(
    "update_ca_layer_frame: pane_id={} cell=({},{}) origin=({},{}) "
    "pane_cell=({},{}) → pixel=({:.1},{:.1}) size=({:.1},{:.1}) scale={}",
    pane.pane_id, cell_w, cell_h, origin_x, origin_y,
    pane_left, pane_top, x, y, w, h, scale
);
```

### Verification

1. Build and launch Wezboard
2. Open a split pane, run `web ryanxcharles.com` from the right side
3. Read the logs — check what `pane_left` value is, what `cell_w` is, and
   whether `pane_left * cell_w` matches the expected left pane pixel width

### Result: Success (diagnostics)

Logs confirmed the mux data is correct and revealed the root cause:

```
mux pane id=0 left=0 top=0 width=79 height=72 pixel=1027x2160
mux pane id=1 left=80 top=0 width=80 height=72 pixel=1040x2160
pane_id=1 metrics cell=(13,30) origin=(13,50) scale=1
frame=(13.0,50.0,1014.0,1980.0)
```

The mux correctly reports pane 1 at left=80 cells. But `scale=1` is wrong — this
is a Retina display where `contentsScale` should be 2.0. The overlay root
layer's `contentsScale` was never set (defaults to 1.0).

All metrics (cell_w=13, origin_x=13, origin_y=50) are in **physical pixels**.
CALayer frames use **logical points** (points = pixels / scale). Since
scale=1.0, no pixel→point conversion happens. For origin_x=13, the error is only
6.5 points (imperceptible). But for
`pane_left * cell_w = 80 * 13 = 1040 pixels`, it should be 520 points —
rendering at 1040 points puts it past the window boundary. This is exactly the
Exp 3 failure.

## Experiment 5: Fix contentsScale and add pane offset

### Hypothesis

Setting the overlay root layer's `contentsScale` to the actual screen scale
factor (2.0 on Retina) will make the `/ scale` division correctly convert
physical pixels to logical points. Combined with the pane offset from
`PositionedPane.left/top`, the overlay will position correctly over any pane.

### Design

Two changes in `conn.rs`:

**1. Set `contentsScale` on the root layer** (in `get_or_create_overlay`)

After creating the root layer, query the window's `backingScaleFactor` and set
it on the layer:

```rust
// Get the backing scale factor from the window
let window: *mut AnyObject = msg_send![superview, window];
let backing_scale: f64 = msg_send![window, backingScaleFactor];
let _: () = msg_send![root_layer, setContentsScale: backing_scale];
```

**2. Add pane offset to `update_ca_layer_frame`**

Same `get_pane_cell_position` helper from Exp 3, same formula. Now that `scale`
will be 2.0 on Retina, the division works correctly:

```rust
let (cell_w, cell_h, origin_x, origin_y) = super::metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x = (origin_x as u64 + pane_left as u64 * cell_w as u64) as f64 / scale;
let y = (origin_y as u64 + pane_top as u64 * cell_h as u64) as f64 / scale;
```

With correct scale=2.0 on Retina:

- Left pane (pane_left=0): x = (13 + 0) / 2 = 6.5 points
- Right pane (pane_left=80): x = (13 + 1040) / 2 = 526.5 points

Keep the debug logging from Exp 4 to verify the values.

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Single pane: `web google.com` — overlay in correct position
3. Split pane, open from RIGHT side — overlay appears over right pane
4. Split pane, open from LEFT side — overlay appears over left pane
5. Check logs: `scale=2` on Retina, computed x/y values are reasonable

### Result: Partial success

The right-pane positioning is fixed — opening a webview from the right pane now
places the overlay over the right pane. The `contentsScale` fix works: scale=2.0
on Retina, and `pane_left * cell_w / scale` produces the correct point offset.

**Regression:** The overlay now renders at the exact top-left corner of the
pane, ignoring the TUI's viewport offset. Previously, the overlay appeared below
the URL bar row (correct). Now it covers the URL bar.

**Why:** Before the Retina fix, `origin_x=13` and `origin_y=50` (physical
pixels) were divided by scale=1.0, producing 13 and 50 points. These values
accidentally provided enough offset to position the overlay correctly below the
tab bar and URL bar. With the correct scale=2.0, they become 6.5 and 25 points —
half the previous offset. The overlay now starts closer to the true pane origin,
which is too high (covers the URL bar) and too far left (less padding).

The TUI sends `col=0, row=1` in SetOverlay (row 1 = skip the URL bar). The
current formula doesn't use these values. Before the fix, the wrong scale
accidentally compensated. Now that scale is correct, the TUI's col/row viewport
offset needs to be explicitly added to the positioning formula. With scale=2.0,
adding `row * cell_h / scale` would give the correct offset in points.

## Experiment 6: Add TUI viewport offset (col/row)

### Hypothesis

The TUI sends `col`, `row`, `width`, `height` in SetOverlay. We already use
`width` and `height` for overlay size (`pixel_w = width * cell_w`,
`pixel_h = height * cell_h`). Adding `col` and `row` to the positioning formula
will offset the overlay below the URL bar (row=1) and position it at the correct
viewport origin within the pane. With scale=2.0 from Exp 5, the pixel→point
conversion will be correct.

### Design

Two files changed: `state.rs` and `conn.rs`.

**1. Add col/row to Pane** (state.rs)

```rust
pub col: u64,
pub row: u64,
```

**2. Store col/row in handle_set_overlay** (conn.rs)

In the new-pane construction and the resize branch, store `overlay.col` and
`overlay.row` on the pane.

**3. Use col/row in update_ca_layer_frame** (conn.rs)

```rust
let x = (origin_x as u64 + (pane_left as u64 + pane.col) * cell_w as u64)
    as f64 / scale;
let y = (origin_y as u64 + (pane_top as u64 + pane.row) * cell_h as u64)
    as f64 / scale;
```

For a single pane (pane_left=0, pane_top=0, col=0, row=1):

- x = (13 + 0 \* 13) / 2 = 6.5 points
- y = (50 + 1 \* 30) / 2 = 40 points (below tab bar + URL bar row)

For the right pane (pane_left=80, pane_top=0, col=0, row=1):

- x = (13 + 80 \* 13) / 2 = 526.5 points
- y = (50 + 1 \* 30) / 2 = 40 points

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Single pane: overlay positioned below URL bar (no regression from pre-Exp 5)
3. Split pane, open from RIGHT side — overlay over right pane, below URL bar
4. Split pane, open from LEFT side — overlay over left pane, below URL bar

### Result: Partial success

Two webviews open side by side without crashes. Each overlay appears inside its
own pane — the mux lookup and col/row offset are working. However, two issues
remain:

**Issue 1: Y position off by ~half a cell height.** The overlay renders slightly
too high — approximately half a cell height (roughly 10 pixels) above where it
should be. Something in the y calculation is producing a value that's slightly
wrong. The col/row offset logic is directionally correct (the overlay is below
the URL bar, not covering it), but the final pixel position is shifted upward by
a small amount. This could be a rounding issue in the integer arithmetic, or one
of the origin/padding values not accounting for something correctly.

**Issue 2: Pane borders not accounted for.** When a second pane is created,
WezTerm adds borders around both panes (the border/padding added in the Wezboard
pane borders work, Issue 723). These borders shift the terminal content inward,
but the overlay positioning formula doesn't include border offsets. The terminal
text moves with the border but the webview overlay stays at the pre-border
position. This means both overlays are misaligned by the border width on each
affected side. The fix requires adding `border.left` and `border.top` to the
positioning formula — these values are available in the render code
(render/pane.rs:109–136) but are not currently passed through the metrics
bridge.

#### Conclusion

The col/row viewport offset works correctly — the overlay is positioned below
the URL bar in the right location relative to the pane. Two remaining problems
need separate fixes:

1. The mysterious ~half-cell y offset — needs debug logging of the exact values
   to diagnose whether it's a rounding issue, an off-by-one in origin_y, or
   something else.
2. The missing border offset — requires expanding the metrics bridge to include
   `border.left` and `border.top` (and possibly `padding_top`), then adding them
   to the positioning formula. This is the same issue identified in Exp 1's
   research (the render code formula includes `border.left + padding_left` and
   `border.top + padding_top + top_bar_height`) but was deferred in Exp 3–5 to
   keep changes minimal.

## Experiment 7: Add padding_top and border offsets to metrics bridge

### Hypothesis

The half-cell y offset is the default `window_padding.top = 0.5 cells` not being
included in `origin_y`. The pane border misalignment is `border.left` and
`border.top` not being included either.

WezTerm's render code (render/pane.rs:79) positions pane content at:

```rust
let top_pixel_y = top_bar_height + padding_top + border.top.get() as f32;
```

And for x (render/pane.rs:120–121, non-leftmost pane):

```rust
padding_left + border.left.get() as f32 + (pos.left as f32 * cell_width)
```

But the metrics bridge only stores `origin_x = padding_left` and
`origin_y = top_bar_height`. The missing `padding_top` (default: 0.5 cells) and
`border.top`/`border.left` cause the overlay to be mispositioned.

### Design

Three files changed: `metrics.rs`, `resize.rs`, `mod.rs`.

**1. Expand metrics bridge** (metrics.rs)

Add `border_left` and `border_top` to the stored values. Change `origin_y` to
include `padding_top`. The metrics become:

- `cell_width` — unchanged
- `cell_height` — unchanged
- `origin_x` — `padding_left` (unchanged — border_left is separate)
- `origin_y` — `top_bar_height + padding_top` (was just `top_bar_height`)
- `border_left` — new
- `border_top` — new

```rust
static BORDER_LEFT: AtomicU32 = AtomicU32::new(0);
static BORDER_TOP: AtomicU32 = AtomicU32::new(0);

pub fn set(cell_width: u32, cell_height: u32, origin_x: u32, origin_y: u32,
           border_left: u32, border_top: u32) {
    // ... store all 6 values
}

pub fn get() -> (u32, u32, u32, u32, u32, u32) {
    // ... load all 6 values
}
```

**2. Update resize path** (resize.rs:72–89)

Currently:

```rust
let (pad_left, _) = self.padding_left_top();
```

Change to capture `pad_top` and compute border:

```rust
let (pad_left, pad_top) = self.padding_left_top();
let border = self.get_os_border();
```

Update `metrics::set` call to pass 6 values:

```rust
crate::termsurf::metrics::set(
    self.render_metrics.cell_size.width as u32,
    self.render_metrics.cell_size.height as u32,
    pad_left as u32,
    (top_bar_height + pad_top) as u32,
    border.left.get() as u32,
    border.top.get() as u32,
);
```

**3. Update init path** (mod.rs:653–662)

The init path already has `padding_top` and `border` computed. Update the
`metrics::set` call:

```rust
crate::termsurf::metrics::set(
    render_metrics.cell_size.width as u32,
    render_metrics.cell_size.height as u32,
    padding_left as u32,
    (if config.tab_bar_at_bottom { 0 } else { tab_bar_height }
        + padding_top) as u32,
    border.left.get() as u32,
    border.top.get() as u32,
);
```

**4. Update positioning formula** (conn.rs `update_ca_layer_frame`)

Change from:

```rust
let (cell_w, cell_h, origin_x, origin_y) = super::metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x = (origin_x as u64 + (pane_left as u64 + pane.col) * cell_w as u64) as f64 / scale;
let y = (origin_y as u64 + (pane_top as u64 + pane.row) * cell_h as u64) as f64 / scale;
```

To:

```rust
let (cell_w, cell_h, origin_x, origin_y, border_left, border_top) = super::metrics::get();
let (pane_left, pane_top) = get_pane_cell_position(&pane.pane_id);
let x = (origin_x as u64 + border_left as u64
    + (pane_left as u64 + pane.col) * cell_w as u64) as f64 / scale;
let y = (origin_y as u64 + border_top as u64
    + (pane_top as u64 + pane.row) * cell_h as u64) as f64 / scale;
```

### Why this fixes both issues

**Half-cell y offset:** `origin_y` changes from `top_bar_height` to
`top_bar_height + padding_top`. With default `padding_top = 0.5 cells`, this
adds ~15 physical pixels (half of cell_height=30), shifting the overlay down by
exactly the amount it was too high.

**Pane border misalignment:** When splits create borders, `border.left` and
`border.top` become non-zero. Adding them to the formula shifts the overlay
inward to match the terminal content. For a single pane with no splits, the
window_frame border defaults are 0, so the formula reduces to the baseline.

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Single pane: `web google.com` — overlay aligned with terminal content (URL
   bar row visible, no half-cell gap)
3. Split pane, open from RIGHT side — overlay over right pane, aligned with
   terminal content
4. Split pane, open from LEFT side — overlay over left pane, aligned with
   terminal content
5. Both overlays shift correctly when borders appear (no misalignment)

### Result: Success

Build compiles with zero errors. The metrics bridge now stores 6 values
(`cell_width`, `cell_height`, `origin_x`, `origin_y`, `border_left`,
`border_top`) instead of 4. `origin_y` includes `padding_top` (default 0.5
cells), fixing the half-cell gap. `border_left` and `border_top` are added to
the positioning formula, fixing the misalignment when pane borders appear.

Files changed:

- `metrics.rs` — Added `BORDER_LEFT`/`BORDER_TOP` atomics, expanded `set`/`get`
  to 6 parameters.
- `resize.rs` — Captures `pad_top` and `border`, passes
  `(top_bar_height + pad_top)` as `origin_y` plus border values.
- `mod.rs` — Moved `metrics::set` after `border` computation, added
  `padding_top` to `origin_y` and passes border values.
- `conn.rs` — Destructures 6-tuple, adds `border_left`/`border_top` to x/y in
  `update_ca_layer_frame`.

