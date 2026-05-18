+++
status = "closed"
opened = "2026-03-14"
closed = "2026-03-14"
+++

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

There should be exactly one place that computes overlay position:
`set_overlay_frame()`. Having a second function that duplicates the calculation
is a maintenance hazard — the next time the formula changes, someone has to
remember to update both.

Remove `update_ca_layer_frame()` entirely. At first creation time, don't set a
frame at all — the CALayerHost defaults to zero-size at origin, which is
invisible. On the very next frame, `paint_pass()` calls `set_overlay_frame()`
and places it correctly. This is how terminal panes work — they don't
pre-compute position at creation time, they get rendered at the correct position
on the next paint pass.

The zero-size initial frame won't affect the browser. The CALayer frame is
purely a display property — it controls where the composited GPU layer appears
in the window. The browser's viewport size is controlled by the `Resize`
protobuf message over the Unix socket, which is independent. Chromium is already
rendering at its correct size before the `CaContext` message arrives.

## Experiments

### Experiment 1: Remove update_ca_layer_frame

#### Description

Delete `update_ca_layer_frame()` and its call site. The CALayerHost will be
created with no explicit frame (defaults to zero-rect), and
`set_overlay_frame()` from `paint_pass()` will place it correctly on the next
frame.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

1. Remove the call to `update_ca_layer_frame(pane, root_layer)` at ~line 1286
   (inside the first-creation branch of `handle_ca_context()`).

2. Delete the entire `update_ca_layer_frame()` function (~lines 1330-1369).

No other changes needed. `set_overlay_frame()` and the `paint_pass()` call site
remain untouched.

#### Verification

```bash
scripts/build.sh wezboard
```

| # | Test                    | Steps                                          | Expected                              |
| - | ----------------------- | ---------------------------------------------- | ------------------------------------- |
| 1 | No flash in right split | Split pane, run `web google.com` in right pane | Webview appears on right, no flash    |
| 2 | No flash in left split  | Split pane, run `web google.com` in left pane  | Webview appears on left, no flash     |
| 3 | No flash without split  | Single pane, run `web google.com`              | Webview appears normally              |
| 4 | Resize after split      | Open webview in right split, resize window     | Webview repositions correctly         |
| 5 | Split after webview     | Open webview, then split pane                  | Webview resizes/repositions correctly |

**Result:** Fail

The overlay still flashes, but now at (0, 0) — the top-left corner of the window
— instead of at the correct coordinates on the wrong pane. Removing
`update_ca_layer_frame()` eliminated the initial positioning entirely, so the
CALayerHost renders at its default zero-rect origin until `set_overlay_frame()`
fires on the next paint pass. The flash is still one frame long, just in a
different (worse) location.

#### Conclusion

The hypothesis was wrong. The flash is not caused by having two positioning
authorities — it's caused by the gap between CALayerHost creation and the first
`paint_pass()` call. Removing the initial positioning made the flash worse (0,0
instead of approximately correct). The fix needs to either (a) make
`update_ca_layer_frame()` split-aware so the first frame is correct, or (b)
defer CALayerHost visibility until `set_overlay_frame()` has run at least once.

### Experiment 2: Defer CALayerHost creation to the render pass

#### Description

Split `handle_ca_context()` into two phases: (1) store the `context_id` on the
pane when the socket message arrives, (2) create the CALayerHost in
`paint_pass()` where the correct split-aware position is already known. The
CALayerHost is created already positioned — no flash, no hidden/unhidden dance.

This is the right approach because:

- The CALayerHost is never visible at the wrong position — it's created at the
  correct position on its first frame
- It keeps positioning in one place (the render pass)
- It works for future multiple-webviews-per-pane where each webview's position
  is only known at render time
- `update_ca_layer_frame()` can be deleted

#### Changes

**`wezboard/wezboard-gui/src/termsurf/state.rs`**

1. Add `pub pending_context_id: Option<u32>` to the `Pane` struct (default
   `None`). This holds a context ID that has arrived but whose CALayerHost has
   not yet been created.

**`wezboard/wezboard-gui/src/termsurf/conn.rs`**

2. In `handle_ca_context()`, for the first-creation case (`ca_layer_host == 0`):
   instead of creating the 3-layer hierarchy and calling
   `update_ca_layer_frame()`, just store the context ID in
   `pane.pending_context_id = Some(context_id)` and return. Still call
   `get_or_create_overlay()` so the overlay NSView and root layer exist.

3. For the swap case (`ca_layer_host != 0`): keep as-is. The layers already
   exist and are positioned, so swapping the CALayerHost in place is fine — no
   flash because the positioning layer is already at the correct location.

4. Delete `update_ca_layer_frame()`.

5. Add a new public function `create_pending_ca_layer_host()` that takes
   `pane_id`, `root_layer` pointer, and the frame coordinates. It checks
   `pane.pending_context_id`, and if `Some`, creates the 3-layer hierarchy
   (flipped → positioning → host) with the positioning layer already set to the
   correct frame. Clears `pending_context_id` to `None` after creation.

**`wezboard/wezboard-gui/src/termwindow/render/paint.rs`**

6. In the overlay positioning block (~line 264), after computing `x`, `y`, `pw`,
   `ph`: check if the pane has a `pending_context_id`. If so, call
   `create_pending_ca_layer_host()` with the position. Otherwise call
   `set_overlay_frame()` as before.

#### Verification

```bash
scripts/build.sh wezboard
```

| # | Test                    | Steps                                          | Expected                              |
| - | ----------------------- | ---------------------------------------------- | ------------------------------------- |
| 1 | No flash in right split | Split pane, run `web google.com` in right pane | Webview appears on right, no flash    |
| 2 | No flash in left split  | Split pane, run `web google.com` in left pane  | Webview appears on left, no flash     |
| 3 | No flash without split  | Single pane, run `web google.com`              | Webview appears normally              |
| 4 | Resize after split      | Open webview in right split, resize window     | Webview repositions correctly         |
| 5 | Split after webview     | Open webview, then split pane                  | Webview resizes/repositions correctly |

**Result:** Pass

All five tests pass. No flash on any split configuration.

#### Conclusion

Deferring CALayerHost creation to the render pass eliminates the flash entirely.
The socket handler now stores the `context_id` as `pending_context_id`, and the
render pass creates the 3-layer hierarchy already positioned at the correct
split-aware coordinates. The layer is never visible at the wrong location
because it doesn't exist until the first frame that knows where to put it.

## Conclusion

The initial overlay flash was caused by creating the CALayerHost in
`handle_ca_context()` (the socket handler), which had no knowledge of the split
tree layout. The fix splits the work: the socket handler stores the context ID,
and the render pass creates the CALayerHost at the correct position on the next
frame. `update_ca_layer_frame()` was deleted — `set_overlay_frame()` and
`create_pending_ca_layer_host()` are now the only two positioning code paths,
both called from `paint_pass()` with split-aware coordinates.
