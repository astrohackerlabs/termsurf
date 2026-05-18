+++
status = "closed"
opened = "2026-03-13"
closed = "2026-03-13"
+++

# Issue 747: Overlay doesn't reposition on split (second screen)

## Goal

When a pane is split, the webview overlay must reposition immediately — not just
resize. This must work on all screens, not just the primary display.

## Background

Issue 746 replaced the overlay's broken positioning system with one that
piggybacks on the terminal's own render pass. The old system
(`reposition_all_overlays`, `get_pane_cell_position`, global `metrics` atomics)
computed overlay coordinates independently and was wrong in multiple ways. The
new system returns `(left_pixel_x, top_pixel_y)` from `paint_pane()` and passes
those coordinates directly to `set_overlay_frame()`, which converts backing
pixels to logical points (`dpi / 72.0`) and sets the CALayer frame inside a
`CATransaction` (to suppress animation).

Issue 746 also added a `MuxNotification::WindowInvalidated` notification after
the async split completes (spawn.rs), ensuring the GUI repaints with the updated
split tree.

This works on the primary screen. On a secondary screen, it doesn't.

### What works

- Single window, single screen: open, split, resize, tab switch — all correct.
- Scale correct on Retina (`dpi / 72.0` = 2.0).
- No animation artifacts (CATransaction suppresses implicit animations).
- Split triggers immediate overlay reposition via `WindowInvalidated`.

### The bug

Steps to reproduce:

1. Open a new window.
2. Move the window to the second screen.
3. Open a webview. It positions correctly.
4. Split pane to the left.
5. The webview **resizes** (width shrinks correctly) but does **not reposition**
   (x stays at 0, left edge, instead of moving to the right pane).
6. Press any key — the overlay snaps to the correct position.

This does not happen on the primary screen. The same window on the primary
screen repositions correctly on split. The bug is specific to a window on a
secondary display.

### What this tells us

The overlay's width and x position are both set in the same
`set_overlay_frame()` call from `paint_pass()`. The width comes from
`pane.pixel_width` (updated by the TUI's protocol resize message after
SIGWINCH). The x comes from `pane_pixel_x` (returned by `paint_pane()`, which
computes `padding_left + border.left + pos.left * cell_width`).

Since the width updates but x doesn't, the paint pass is running — but with
`pos.left` still at 0 (the pre-split value). Then a keypress triggers another
paint which sees the correct `pos.left`.

The `WindowInvalidated` notification travels through a 4-hop async chain:

1. `mux.notify()` → subscriber calls `spawn_into_main_thread`
2. `mux_pane_output_event_callback` → `window.notify()` →
   `Connection::with_window_inner` → `spawn_into_main_thread`
3. Event handler dispatches `WindowInvalidated` → `window.invalidate()` →
   `Connection::with_window_inner` → `spawn_into_main_thread`
4. Inner invalidate runs `setNeedsDisplay: true`

On macOS, the spawn queue processes one task per `CFRunLoopObserver` invocation.
Each display has its own `CVDisplayLink` at its own phase. The hypothesis is
that on the second screen, display timing causes a repaint between the tree
update and the `setNeedsDisplay: true` arriving — so the repaint sees old
positions but new sizes (since the TUI's resize message updates
`pane.pixel_width` through a different path).

However, `get_panes_to_render()` reads directly from `tab.iter_panes()`, which
walks the live split tree. The tree is updated before the notification is sent.
So any paint after the split should see the correct `pos.left`. This contradicts
the observed behavior and suggests the root cause may be elsewhere.

### Key files

- `wezboard/wezboard-gui/src/termwindow/render/paint.rs` — `paint_pass()`,
  overlay positioning block (lines 263–288)
- `wezboard/wezboard-gui/src/termwindow/render/pane.rs` — `paint_pane()`,
  returns `(left_pixel_x, top_pixel_y)`
- `wezboard/wezboard-gui/src/termsurf/conn.rs` — `set_overlay_frame()`,
  `update_ca_layer_frame()` (initial placement)
- `wezboard/wezboard-gui/src/spawn.rs` — `WindowInvalidated` notification after
  split
- `wezboard/wezboard-gui/src/termwindow/mod.rs` — `WindowInvalidated` handler
  (line 1328), notification filter (line 1552)
- `wezboard/window/src/os/macos/window.rs` — `invalidate()` →
  `setNeedsDisplay: true`
- `wezboard/window/src/os/macos/connection.rs` — `with_window_inner` →
  `spawn_into_main_thread`

### Root cause

The initial analysis in the "What this tells us" section above was wrong. It
assumed the resize and reposition both come from `set_overlay_frame` in
`paint_pass`. They don't. The resize is visible because **Chromium itself
renders smaller content**, not because the CALayer frame changed.

There are **two independent code paths** that set the CALayer frame:

1. **`set_overlay_frame()`** (conn.rs:1372) — called from `paint_pass()` every
   frame. Uses `paint_pane()` coordinates which include `pos.left` from the
   split tree. **Correct.**

2. **`update_ca_layer_frame()`** (conn.rs:1331) — called from
   `handle_ca_context()` whenever Chromium sends a `CaContext` message. Uses
   global `metrics` atomics and `pane.col` — has **no knowledge of `pos.left`**.
   Always positions the overlay as if the pane is at the left edge. **Wrong
   after a split.**

The `on_ca_context_id` callback fires on **every GPU frame swap** (traced
through Chromium: `DidSwapBuffersComplete` → `DidReceiveCALayerParams` →
`AcceleratedWidgetCALayerParamsUpdated` → `ca_layer_params_callback_` →
`TsNotifyCAContextId`). This means every time Chromium renders a frame —
including after a resize — it sends a `CaContext` message, which triggers
`update_ca_layer_frame`.

The sequence on split:

1. Split happens. Tree updated. `pos.left` is now correct.
2. `paint_pass` runs (terminal panes reposition correctly). `set_overlay_frame`
   sets the overlay to the **correct** position and size.
3. TUI sends SetOverlay resize → Roamium sends Resize to Chromium.
4. Chromium resizes, renders a new frame at the smaller size. The user sees
   smaller content because Chromium's CALayerHost displays the new smaller
   frame.
5. Chromium fires `on_ca_context_id` → Roamium sends `CaContext` to GUI.
6. `handle_ca_context` → `update_ca_layer_frame` → **overwrites** the frame with
   `origin_x + border_left + pane.col * cell_w`. No `pos.left`. The overlay
   snaps back to the left edge with the correct width.

Step 6 clobbers step 2. The user sees the overlay at the left edge with the
correct (smaller) size.

Pressing ESC triggers another `paint_pass` → `set_overlay_frame` with the
correct `pos.left`. No `CaContext` follows (Chromium isn't resizing), so the
correct position sticks.

On the primary screen, the timing works out so that another `paint_pass` runs
after step 6 (from PaneOutput or other activity), correcting it before the user
notices. On the secondary screen, the display link phase means no additional
`paint_pass` runs until the user interacts.

## Experiments

### Experiment 1: Remove update_ca_layer_frame

#### Description

`update_ca_layer_frame` is the sole cause of the bug. It runs on every
`CaContext` message (every Chromium frame swap) and overwrites the correct
position set by `set_overlay_frame` with a position that has no split tree
awareness.

The fix: delete `update_ca_layer_frame` and its call site. The render pass
already calls `set_overlay_frame` every frame in `paint_pass`, which computes
the correct position from `paint_pane()`. The first frame after the CALayerHost
is created will position the overlay correctly — a one-frame delay that is
imperceptible.

The `handle_ca_context` function still needs to create the layer hierarchy
(flipped layer, positioning layer, CALayerHost) and swap CALayerHost on context
ID changes. It just shouldn't set the frame position.

#### Changes

**`wezboard-gui/src/termsurf/conn.rs`:**

1. Delete the `update_ca_layer_frame` function (lines 1330–1369, both the
   `#[cfg(target_os = "macos")]` implementation and any non-macOS stub).

2. In `handle_ca_context`, remove the call to `update_ca_layer_frame` (line
   1307: `update_ca_layer_frame(pane, root_layer);`). The `root_layer` variable
   is still needed for `get_or_create_overlay` and
   `msg_send![root_layer, bounds]` / `msg_send![root_layer, addSublayer]` in the
   layer creation block. No other code references `update_ca_layer_frame`.

No other files change.

#### Verification

Build:

- [ ] `cd wezboard && cargo build` — compiles without errors.

Edge case checklist (test on both screens):

- [ ] Open a webview — positioned and sized correctly.
- [ ] Split pane to the left — webview repositions to the right immediately.
- [ ] Same test on second screen — webview repositions immediately (the bug).
- [ ] Open a new tab — webview disappears.
- [ ] Switch back — webview at correct position.
- [ ] Resize the window — webview tracks pane position.
- [ ] Close the split pane — webview expands to fill.
- [ ] Open a second window on a second screen, open webview — correct position.

**Result:** Fail

Opening a webview positions it at (0,0) — the overlay is completely misplaced
from the start. `update_ca_layer_frame` was providing the initial positioning
that `set_overlay_frame` in `paint_pass` hadn't run yet to set. Without it, the
overlay has no position until the next paint pass happens to run, and that
doesn't happen until user interaction (e.g. pressing ESC). The function isn't
just clobbering — it's also the only thing that positions the overlay on first
creation.

#### Conclusion

`update_ca_layer_frame` cannot simply be deleted. It serves a critical role:
initial overlay positioning when the CALayerHost is first created (before any
`paint_pass` has run with overlay coordinates). The fix needs to either (a) make
`update_ca_layer_frame` split-aware so it positions correctly, or (b) ensure
`set_overlay_frame` runs immediately after layer creation before the user sees
anything.

### Experiment 2: Reuse stored overlay coordinates in update_ca_layer_frame

#### Description

The pane already stores `overlay_origin_x`, `overlay_origin_y`, and
`overlay_scale` — set by `set_overlay_frame` every frame from `paint_pass` with
the correct split-tree-aware coordinates.

`update_ca_layer_frame` currently ignores these and recomputes position from
scratch using `pane.col * cell_w` — a formula with no split tree awareness.

The fix: make `update_ca_layer_frame` check whether `set_overlay_frame` has
already run (indicated by `overlay_scale > 0.0`). If so, reuse the stored
`overlay_origin_x/y` and `overlay_scale` instead of recomputing. If not (first
creation, before any `paint_pass`), fall back to the current formula.

This way:

- **First creation:** reasonable initial positioning (same as today).
- **Every subsequent Chromium frame swap:** reapplies the last known-good
  position from `paint_pass` instead of clobbering it.
- **After a split:** Chromium fires `CaContext`, but `update_ca_layer_frame`
  reuses the correct position that `set_overlay_frame` already computed.

#### Changes

**`wezboard-gui/src/termsurf/conn.rs`** — `update_ca_layer_frame`:

Replace the position computation (lines 1336–1347) with a branch:

```rust
let (x, y, w, h) = if pane.overlay_scale > 0.0 {
    // set_overlay_frame has run — reuse its split-aware coordinates.
    let scale = pane.overlay_scale;
    let x = pane.overlay_origin_x / scale;
    let y = pane.overlay_origin_y / scale;
    let w = pane.pixel_width as f64 / scale;
    let h = pane.pixel_height as f64 / scale;
    (x, y, w, h)
} else {
    // First creation, before any paint_pass. Best-effort initial placement.
    let scale: f64 = msg_send![root_layer, contentsScale];
    let scale = if scale > 0.0 { scale } else { 1.0 };
    let w = pane.pixel_width as f64 / scale;
    let h = pane.pixel_height as f64 / scale;
    let (cell_w, cell_h, origin_x, origin_y, border_left, border_top) = super::metrics::get();
    let x_backing = (origin_x as u64 + border_left as u64 + pane.col * cell_w as u64) as f64;
    let y_backing = (origin_y as u64 + border_top as u64 + pane.row * cell_h as u64) as f64;
    pane.overlay_origin_x = x_backing;
    pane.overlay_origin_y = y_backing;
    pane.overlay_scale = scale;
    let x = x_backing / scale;
    let y = y_backing / scale;
    (x, y, w, h)
};
```

No other files change.

#### Verification

Build:

- [ ] `cd wezboard && cargo build` — compiles without errors.

Functional (test on both screens):

- [ ] Open a webview — positioned and sized correctly.
- [ ] Split pane to the left — webview repositions to the right immediately.
- [ ] Same test on second screen — webview repositions immediately (the bug).
- [ ] Resize the window — webview tracks pane position.
- [ ] Close the split pane — webview expands to fill.
- [ ] Open a new tab, switch back — webview at correct position.

**Result:** Fail

Same behavior as Experiment 1. Opening a webview positions it at (0,0). The
`overlay_scale` is 0.0 on first creation (default), so the `else` branch runs —
which is the same old formula. The branch never helps because
`set_overlay_frame` hasn't run yet when the first `CaContext` arrives. The
overlay starts wrong and stays wrong until user interaction triggers a
`paint_pass`.

#### Conclusion

The stored-coordinates approach doesn't help on first creation because
`overlay_scale` is initialized to `1.0` (not `0.0` as assumed — see state.rs
lines 524, 648), so the `if pane.overlay_scale > 0.0` branch always runs,
reading `overlay_origin_x/y` which are still `0.0` (unset). Result:
`0.0 / 1.0 = (0, 0)`. The fallback never executes. The fix must ensure that the
initial positioning in `handle_ca_context` is correct, or that a `paint_pass`
with correct coordinates runs immediately after layer creation.

### Experiment 3: Call update_ca_layer_frame only on first creation

#### Description

`handle_ca_context` has two branches:

1. **First creation** (`ca_layer_host == 0`) — builds the 3-layer hierarchy
   (flipped → positioning → CALayerHost).
2. **Swap** (layer exists) — atomically replaces the CALayerHost with a new one
   when Chromium sends a new context ID (every GPU frame swap).

Currently, `update_ca_layer_frame` is called **after both branches** (line
1307). This means every Chromium frame swap overwrites the overlay position with
the split-unaware formula.

The fix: move the `update_ca_layer_frame` call inside the first-creation branch
only. After creation, `set_overlay_frame` from `paint_pass` becomes the sole
authority on position. Subsequent frame swaps (the `else` branch) just swap the
CALayerHost — they don't need to reposition because `set_overlay_frame` has
already set the correct frame.

This does not modify `update_ca_layer_frame` itself.

#### Changes

**`wezboard-gui/src/termsurf/conn.rs`** — `handle_ca_context`:

Move lines 1306–1307 (`// Position the overlay` +
`update_ca_layer_frame(pane,
root_layer);`) from after the `if/else` block into
the end of the `if
pane.ca_layer_host == 0` branch (after line 1283, before the
closing `}`).

Before:

```rust
        } // end else (swap)

        // Position the overlay
        update_ca_layer_frame(pane, root_layer);

        let _: () = msg_send![ca_transaction, commit];
```

After:

```rust
            // Position the overlay
            update_ca_layer_frame(pane, root_layer);
        } else {
            // Atomic swap: ...
            ...
        }

        let _: () = msg_send![ca_transaction, commit];
```

No other files change.

#### Verification

Build:

- [x] `cd wezboard && cargo build` — compiles without errors.

Functional (test on both screens):

- [x] Open a webview — positioned and sized correctly (initial positioning
      preserved).
- [x] Split pane to the left — webview repositions to the right immediately.
- [x] Same test on second screen — webview repositions immediately (the bug).
- [x] Resize the window — webview tracks pane position.
- [x] Close the split pane — webview expands to fill.
- [x] Open a new tab, switch back — webview at correct position.

**Result:** Pass

All verification checks passed on both screens. Moving `update_ca_layer_frame`
into the first-creation branch only preserves correct initial positioning while
preventing subsequent Chromium frame swaps from clobbering the split-aware
position set by `set_overlay_frame`.

#### Conclusion

The fix was a one-line move: `update_ca_layer_frame` only needs to run when the
CALayerHost is first created. After that, `set_overlay_frame` from `paint_pass`
is the sole authority on overlay position. The root cause was never the formula
itself — it was that the formula ran on every Chromium frame swap, overwriting
correct positions with split-unaware ones.

## Conclusion

Issues 746 and 747 together replaced the overlay positioning system and fixed
every known positioning bug across all screens.

### What was accomplished

**Issue 746** replaced the overlay's broken positioning system with one that
piggybacks on the terminal's own render pass. The old system
(`reposition_all_overlays`, `get_pane_cell_position`, global `metrics` atomics)
computed overlay coordinates independently from the terminal renderer — a
duplicated formula that was wrong in multiple ways. The new system returns pixel
coordinates from `paint_pane()` and passes them directly to
`set_overlay_frame()`. Five experiments, three architectural changes kept:

1. `paint_pane()` returns pixel origin coordinates accounting for padding,
   borders, tab bar, and split divider offsets.
2. `paint_pass()` calls `set_overlay_frame()` every frame with split-tree-aware
   coordinates.
3. `set_overlay_frame()` converts backing pixels to logical points via
   `dpi / 72.0`, wrapped in a `CATransaction` to suppress animation.
4. `spawn.rs` fires `MuxNotification::WindowInvalidated` after split to trigger
   immediate repaint.
5. Deleted `reposition_all_overlays()` and `get_pane_cell_position()`.

**Issue 747** fixed the remaining multi-screen bug. The root cause:
`update_ca_layer_frame()` ran on every Chromium GPU frame swap (via
`handle_ca_context`), overwriting the correct position set by
`set_overlay_frame()` with a split-unaware formula. On the primary screen,
another `paint_pass` corrected it before the user noticed. On secondary screens,
display link timing meant no corrective repaint occurred until user interaction.
The fix: move the `update_ca_layer_frame()` call inside the first-creation
branch of `handle_ca_context()`. After layer creation, `set_overlay_frame()` is
the sole authority on position.

### What works

- Open, split, resize, tab switch, switch back — all correct on all screens.
- Scale correct on Retina displays (`dpi / 72.0` = 2.0).
- No animation artifacts (CATransaction suppresses implicit animations).
- Split triggers immediate overlay reposition via `WindowInvalidated`.
- Multi-screen: overlays reposition correctly on split on secondary displays.
- New tab hides overlay, switch back restores it.
- Close split — remaining pane expands, overlay fills.

### Key insight

The overlay positioning system now has a clean separation of concerns:
`update_ca_layer_frame()` handles initial placement when the CALayerHost is
first created, and `set_overlay_frame()` from `paint_pass()` handles everything
after that. No code path overwrites another. One authority at each stage of the
overlay lifecycle.
