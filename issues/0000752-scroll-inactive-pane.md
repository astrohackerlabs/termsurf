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
