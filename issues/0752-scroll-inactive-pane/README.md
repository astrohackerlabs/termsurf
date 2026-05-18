+++
status = "closed"
opened = "2026-03-15"
closed = "2026-03-15"
+++

# Issue 752: Scroll webview in inactive pane

## Goal

Scrolling with the mouse (or trackpad) over a browser overlay scrolls that
overlay's web content, even if the overlay's pane is not the active pane. This
matches how terminal panes already work — mouse scroll targets the pane under
the cursor, not the focused pane.

## Background

### Current behavior

Scroll events only reach the browser overlay in the **active** pane. If the
active pane is on the left and a browser overlay is visible on the right,
scrolling over the right overlay does nothing — the scroll event goes to the
active pane's overlay (or is consumed by the terminal).

### How scroll events flow today

1. `WindowEvent::RawScrollEvent` arrives in `termwindow/mod.rs`
2. `get_active_pane_or_overlay()` returns the currently active terminal pane
3. `try_forward_raw_scroll(active_pane_id, coords, ...)` is called with that
   pane's ID
4. `try_forward_raw_scroll` hit-tests the coordinates against that one pane's
   overlay bounds
5. If the hit-test passes, the scroll is forwarded to Chromium

The problem is step 2–3: only the active pane is considered. If the mouse is
over a different pane's overlay, the scroll is lost.

### How mouse events work (for comparison)

`try_forward_mouse()` checks `pane.browsing` and hit-tests the overlay bounds,
but it also only operates on the active pane. However, mouse clicks change
focus, so the active pane is usually the one the user is interacting with.
Scroll doesn't change focus — you expect to scroll whatever is under your
cursor.

### What needs to change

Instead of only checking the active pane, `try_forward_raw_scroll` (or its
caller in `termwindow/mod.rs`) should iterate over all panes that have browser
overlays and hit-test the scroll coordinates against each one. The first overlay
that contains the cursor receives the scroll event.

This is the same behavior terminal panes have — WezTerm already scrolls the pane
under the cursor regardless of focus. We just need to extend that to browser
overlays.

## Experiments

### Experiment 1: Hit-test all overlay panes for scroll events

#### Description

Add a new function `try_forward_scroll_any_pane()` that iterates over all panes
with browser overlays, hit-tests the scroll coordinates against each one, and
forwards the scroll event to the first match. Call this from `termwindow/mod.rs`
instead of the current single-pane path.

The existing `try_forward_raw_scroll()` stays unchanged — the new function calls
it for each candidate pane.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/input.rs`**

1. Add a new public function `try_forward_scroll_any_pane()` with the same
   signature as `try_forward_raw_scroll` but without the `pane_id` parameter:

   ```rust
   pub fn try_forward_scroll_any_pane(
       coords: ::window::Point,
       delta_x: f64,
       delta_y: f64,
       phase: u64,
       momentum_phase: u64,
       precise: bool,
       modifiers: Modifiers,
   ) -> bool
   ```

   This function:
   - Locks the global state
   - Collects all pane IDs that have a browser overlay (`tab_id != 0` and
     `ca_layer_host != 0`)
   - Drops the lock
   - For each candidate pane, calls `try_forward_raw_scroll()` with that pane's
     ID
   - Returns `true` on the first hit, `false` if none match

**`wezboard/wezboard-gui/src/termsurf/mod.rs`**

2. Export the new function: `pub use conn::try_forward_scroll_any_pane;` (or
   from `input` — wherever it lives).

**`wezboard/wezboard-gui/src/termwindow/mod.rs`**

3. In the `RawScrollEvent` handler (~line 1086), replace the current logic:

   Before:

   ```rust
   if let Some(pane) = self.get_active_pane_or_overlay() {
       self.raw_scroll_consumed = crate::termsurf::input::try_forward_raw_scroll(
           pane.pane_id(), coords, ...
       );
   }
   ```

   After:

   ```rust
   self.raw_scroll_consumed = crate::termsurf::input::try_forward_scroll_any_pane(
       coords, delta_x, delta_y, phase, momentum_phase, precise, modifiers,
   );
   ```

   If no overlay consumes the scroll, `raw_scroll_consumed` is `false` and the
   terminal handles it normally.

#### Verification

```bash
scripts/build.sh wezboard
```

| #   | Test                          | Steps                                                         | Expected                           |
| --- | ----------------------------- | ------------------------------------------------------------- | ---------------------------------- |
| 1   | Scroll inactive webview       | Split pane, webview on right, focus left, scroll over right   | Right webview scrolls              |
| 2   | Scroll active webview         | Focus the webview pane, scroll over it                        | Webview scrolls (no regression)    |
| 3   | Scroll terminal pane          | Scroll over a terminal pane with no webview                   | Terminal scrolls normally          |
| 4   | Scroll outside overlay bounds | Scroll over the terminal area of a pane that has a webview    | Terminal scrolls, not the webview  |
| 5   | Two webviews, scroll each     | Two split panes with webviews, scroll over each without focus | Each webview scrolls independently |

**Result:** Pass

All five tests pass.

#### Conclusion

Hit-testing all overlay panes instead of only the active pane allows scroll
events to reach any visible webview. The new `try_forward_scroll_any_pane()`
iterates all panes with browser overlays, and the first geometric hit receives
the scroll.

## Conclusion

Scroll events now target the overlay under the cursor regardless of pane focus.
Added `try_forward_scroll_any_pane()` which iterates all overlay panes and
hit-tests each one, replacing the previous active-pane-only path. The
lower-level `try_forward_raw_scroll()` remains public for targeted use.
