# Issue 749: Initial overlay flash on wrong side of split

## Goal

When opening a browser overlay in a split pane, the webview appears at its
correct position immediately — no flash on the wrong side.

## Background

### Two code paths set the overlay frame

There are two functions that set the CALayer frame for browser overlays:

1. **`update_ca_layer_frame()`** (conn.rs ~1331) — runs once when the
   CALayerHost is first created inside `handle_ca_context()`. Computes position
   using `origin_x + border_left + pane.col * cell_w`. This formula has no
   knowledge of the split tree — it doesn't know `pos.left`, so it always
   positions relative to the window's left edge.

2. **`set_overlay_frame()`** (conn.rs ~1372) — runs every frame from
   `paint_pass()`. Receives coordinates from `paint_pane()` which includes
   `pos.left` and `pos.top` from the live split tree. This is the correct,
   authoritative position.

### The flash

When opening `web google.com` in a right-side split pane:

1. Chromium starts rendering and sends a `CaContext` message
2. `handle_ca_context()` creates the CALayerHost and calls
   `update_ca_layer_frame()`, which places the overlay at the LEFT side of the
   window (no split tree awareness)
3. On the next frame, `paint_pass()` calls `set_overlay_frame()` with the
   correct right-side coordinates
4. The overlay jumps from left to right — visible as a brief flash

### Prior work

Issue 746 established the render-pass-based positioning system
(`set_overlay_frame()` called from `paint_pass()`). Issue 747 fixed a bug where
`update_ca_layer_frame()` was being called on EVERY `CaContext` message (not
just first creation), which caused overlays to snap back to the wrong position
after splits on secondary screens. The fix moved the call inside the
first-creation branch only.

Issue 747's fix was correct — `update_ca_layer_frame()` should not run on every
frame swap. But it left the first-creation call in place, which is what causes
this initial flash.

### The fix

`update_ca_layer_frame()` is wrong for split panes and redundant —
`set_overlay_frame()` will correct the position on the very next paint pass. The
simplest fix: don't set the frame in `update_ca_layer_frame()` at all. Instead,
position the CALayerHost offscreen initially (e.g., at a negative coordinate
like `-10000`) so there's no visible flash, and let the first
`set_overlay_frame()` from `paint_pass()` place it correctly.

Alternatively, `update_ca_layer_frame()` could be removed entirely and the
initial frame could be set to zero-size or offscreen in `handle_ca_context()`
directly.
