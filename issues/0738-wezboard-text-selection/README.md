+++
status = "closed"
opened = "2026-03-10"
closed = "2026-03-10"
+++

# Issue 738: Wezboard text selection

## Goal

Click+drag text selection should work in Wezboard browser overlays — single
click to place cursor, drag to select, double-click for word, triple-click for
line.

## Background

Text selection works in Ghostboard but not in Wezboard. The GUI is responsible
for forwarding mouse events to Chromium via the TermSurf protocol (MouseEvent
and MouseMove messages). The TUI (webtui) is not involved in mouse forwarding —
it intentionally drops all mouse events.

### How Ghostboard does it

Ghostboard's working implementation (`Surface.zig` + `xpc.zig`) has three key
mechanisms:

1. **Click count tracking.** `mouseButtonCallback` (Surface.zig:4021–4080)
   tracks `left_click_count` by measuring time and distance between clicks. If
   the next click is within the timing window and close enough, the count
   increments (1→2→3→1). The count is sent in `MouseEvent.click_count`, enabling
   Chromium's double-click (word) and triple-click (line) selection.

2. **Button-down flags in MouseMove.** `sendMouseMove` (xpc.zig:1272–1304) reads
   `click_state[LEFT]` and sets `modifiers |= 64` when the left button is held.
   This lets Chromium distinguish drag (selecting text) from hover (just moving
   the cursor).

3. **Persistent click state.** `click_state` is updated in `mouseButtonCallback`
   _before_ the overlay hit-test, so it persists across move events. A press
   sets `.press`, a release sets `.release`. Move events read this state to
   encode button-down flags.

### What Wezboard gets wrong

Three bugs in `wezboard/wezboard-gui/src/termsurf/input.rs`:

1. **Click count always 1.** Lines 131, 152, and 173 hardcode `click_count: 1`.
   No timing or distance tracking exists. Double-click and triple-click
   selection are impossible.

2. **No button-down flags in MouseMove.** Lines 179–191 send MouseMove with only
   keyboard modifiers (shift/ctrl/alt/super). The `modifiers_to_termsurf`
   function (lines 252–271) doesn't encode button state. Chromium receives move
   events but can't tell if a button is held, so it treats every move as a hover
   — no drag selection.

3. **MouseMove stops at overlay boundary.** The `hit_test_overlay` check at line
   109 gates all event forwarding. If the user clicks inside the overlay and
   drags outside it, MouseMove events stop. Selection freezes mid-drag.

### Fix approach

Add click state tracking to `input.rs` (or `state.rs`):

- Track which buttons are currently pressed.
- Track left-click timestamp and position for click count calculation.
- On MouseMove, encode button-down flags in modifiers (bit 6 = left, bit 8 =
  right), matching Ghostboard's convention.
- On MouseMove outside the overlay while a button is held, clamp coordinates to
  the overlay bounds and continue sending events.

## Experiments

### Experiment 1: Mouse state tracking and button-down flags

#### Description

Fix all three bugs in `input.rs` by adding a `MouseState` struct that tracks
button presses and click timing. WezTerm's `MouseEvent` already carries
`mouse_buttons: MouseButtons` (a bitflag with `LEFT`, `RIGHT`, `MIDDLE`), so we
don't need to manually track press/release — we can read button state directly
from the event.

#### Changes

**`wezboard/wezboard-gui/src/termsurf/input.rs`**

1. Add a module-level `MouseState` struct with `OnceLock<Mutex<MouseState>>`:

   ```rust
   use std::sync::{Mutex, OnceLock};
   use std::time::Instant;

   struct MouseState {
       left_click_count: i64,
       last_click_time: Option<Instant>,
       last_click_x: f64,
       last_click_y: f64,
       /// The pane where a drag started (if any button is held).
       drag_pane: Option<String>,
   }

   static MOUSE_STATE: OnceLock<Mutex<MouseState>> = OnceLock::new();

   fn mouse_state() -> &'static Mutex<MouseState> {
       MOUSE_STATE.get_or_init(|| Mutex::new(MouseState {
           left_click_count: 0,
           last_click_time: None,
           last_click_x: 0.0,
           last_click_y: 0.0,
           drag_pane: None,
       }))
   }
   ```

2. **Fix 1 — Click count.** In both `Press(Left)` branches (lines 123–135 and
   137–156), replace the hardcoded `click_count: 1` with a call to a new
   `compute_click_count` function:

   ```rust
   fn compute_click_count(x: f64, y: f64) -> i64 {
       let mut ms = mouse_state().lock().unwrap();
       let now = Instant::now();
       // Reset if too far or too long since last click.
       // 5.0 logical pixels distance threshold, 500ms timing threshold.
       let dist = ((x - ms.last_click_x).powi(2) + (y - ms.last_click_y).powi(2)).sqrt();
       let expired = ms.last_click_time
           .map(|t| now.duration_since(t).as_millis() > 500)
           .unwrap_or(true);
       if dist > 5.0 || expired {
           ms.left_click_count = 0;
       }
       ms.left_click_count = (ms.left_click_count % 3) + 1; // cycles 1→2→3→1
       ms.last_click_time = Some(now);
       ms.last_click_x = x;
       ms.last_click_y = y;
       ms.left_click_count
   }
   ```

   Call `compute_click_count(rel_x, rel_y)` for left presses and pass the result
   as `click_count`. Keep `click_count: 1` for right/middle presses and all
   releases.

3. **Fix 2 — Button-down flags in MouseMove.** Replace the `WMEK::Move` handler
   (lines 179–191) to encode button state from `event.mouse_buttons`:

   ```rust
   WMEK::Move => {
       let mut mods = modifiers_to_termsurf(event.modifiers);
       if event.mouse_buttons.contains(MouseButtons::LEFT) {
           mods |= 64;  // bit 6
       }
       if event.mouse_buttons.contains(MouseButtons::RIGHT) {
           mods |= 256; // bit 8
       }
       send_to_chromium(
           &pane_id_str,
           Msg::MouseMove(proto::MouseMove {
               tab_id: 0,
               x: rel_x,
               y: rel_y,
               modifiers: mods,
           }),
       );
       return true;
   }
   ```

   Also add `MouseButtons` to the import line at the top of the file.

4. **Fix 3 — Drag outside overlay.** Track drag state and clamp coordinates.

   In the `Press` handlers, after sending the MouseEvent, record the drag pane:

   ```rust
   mouse_state().lock().unwrap().drag_pane = Some(pane_id_str.clone());
   ```

   In the `Release` handler, clear it:

   ```rust
   mouse_state().lock().unwrap().drag_pane = None;
   ```

   At the top of `try_forward_mouse`, before the `hit_test_overlay` check (line
   109), add a fallthrough for drag-outside-overlay. After the existing
   `if let Some((rel_x, rel_y)) = hit_test_overlay(...)` block (which returns on
   hit), add:

   ```rust
   // Drag outside overlay — clamp to overlay bounds.
   if matches!(&event.kind, WMEK::Move | WMEK::Release(_)) {
       let is_dragging = mouse_state().lock().unwrap().drag_pane.as_deref()
           == Some(&pane_id_str);
       if is_dragging {
           if let Some((rel_x, rel_y)) = clamp_to_overlay(&pane_id_str, event) {
               match &event.kind {
                   WMEK::Move => {
                       let mut mods = modifiers_to_termsurf(event.modifiers);
                       if event.mouse_buttons.contains(MouseButtons::LEFT) {
                           mods |= 64;
                       }
                       if event.mouse_buttons.contains(MouseButtons::RIGHT) {
                           mods |= 256;
                       }
                       send_to_chromium(
                           &pane_id_str,
                           Msg::MouseMove(proto::MouseMove {
                               tab_id: 0,
                               x: rel_x,
                               y: rel_y,
                               modifiers: mods,
                           }),
                       );
                       return true;
                   }
                   WMEK::Release(press) => {
                       let button_str = match press {
                           MousePress::Left => "left",
                           MousePress::Right => "right",
                           MousePress::Middle => "middle",
                       };
                       send_to_chromium(
                           &pane_id_str,
                           Msg::MouseEvent(proto::MouseEvent {
                               tab_id: 0,
                               r#type: "up".to_string(),
                               button: button_str.to_string(),
                               x: rel_x,
                               y: rel_y,
                               click_count: 1,
                               modifiers: modifiers_to_termsurf(event.modifiers),
                           }),
                       );
                       mouse_state().lock().unwrap().drag_pane = None;
                       return true;
                   }
                   _ => {}
               }
           }
       }
   }
   ```

   Add the `clamp_to_overlay` helper:

   ```rust
   fn clamp_to_overlay(pane_id_str: &str, event: &MouseEvent) -> Option<(f64, f64)> {
       let state = super::shared_state()?;
       let st = state.lock().unwrap();
       let pane = st.panes.get(pane_id_str)?;
       let ox = pane.overlay_origin_x;
       let oy = pane.overlay_origin_y;
       let ow = pane.pixel_width as f64;
       let oh = pane.pixel_height as f64;
       let scale = pane.overlay_scale;
       let mx = (event.coords.x as f64).clamp(ox, ox + ow - 1.0);
       let my = (event.coords.y as f64).clamp(oy, oy + oh - 1.0);
       Some(((mx - ox) / scale, (my - oy) / scale))
   }
   ```

#### Verification

1. `./scripts/build.sh wezboard`
2. Launch Wezboard, open a browser pane, navigate to a page with text.
3. **Single click+drag:** Click and drag across text — text should highlight as
   you drag. Release — selection stays.
4. **Drag outside overlay:** Start selecting inside the overlay, drag the mouse
   below/above/beside the overlay boundary — selection should continue
   (clamped), not freeze.
5. **Double-click:** Double-click a word — entire word should be selected.
6. **Triple-click:** Triple-click — entire line/paragraph should be selected.
7. **Right-click:** Right-click — context menu should appear (no regression).

**Result:** Pass

All verification steps confirmed working — click+drag highlights text, dragging
outside the overlay continues the selection clamped to the overlay edge,
double-click selects a word, triple-click selects a line, and right-click shows
the context menu without regression.

#### Conclusion

All three fixes landed cleanly in a single experiment. WezTerm's `MouseEvent`
already carried `mouse_buttons: MouseButtons` bitflags, so we could read button
state directly from the event instead of manually tracking press/release — a
simpler approach than Ghostboard's `click_state` array.

## Conclusion

Wezboard browser overlays now support full text selection. Three bugs in
`input.rs` were fixed in one experiment:

1. **Click count tracking** — A `MouseState` struct with `OnceLock<Mutex<...>>`
   tracks left-click timing and position, cycling `click_count` through 1→2→3
   within a 500ms / 5px window. Double-click selects words, triple-click selects
   lines.

2. **Button-down flags in MouseMove** — Move events now encode `LEFT` (bit 6)
   and `RIGHT` (bit 8) in the modifiers field by reading `event.mouse_buttons`,
   letting Chromium distinguish drag from hover.

3. **Drag outside overlay** — When a drag starts inside the overlay and the
   mouse moves outside, coordinates are clamped to the overlay bounds and events
   continue flowing to Chromium. The `drag_pane` field tracks which pane owns
   the active drag, cleared on release.
