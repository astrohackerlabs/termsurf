+++
status = "closed"
opened = "2026-03-04"
closed = "2026-03-04"
+++

# Issue 703: Remove click-to-activate suppression

## Goal

Mouse clicks, drags, and scrolls should always propagate to content immediately,
without requiring a separate "activation" click first. Clicking a browser pane
in control mode should switch to browse mode AND forward the click. Clicking an
unfocused pane should focus it AND forward the click. Scrolling over a browser
pane in control mode should forward the scroll to the webview — being able to
scroll without switching modes is very useful. The current behavior — requiring
one click to activate, then another to interact — is annoying in practice.

Keyboard focus still follows the click (the clicked pane becomes the active pane
for keyboard input), but mouse events should never be swallowed.

## Background

Three issues built the current suppression system:

- **Issue 670** — Added `pane_activation` flag. When a pane gains focus, the
  first click is consumed (press and release swallowed). Rationale: panes should
  behave like OS windows, where click-to-focus doesn't pass through.
- **Issue 695** — Extended suppression to drags. Added early return in
  `cursorPosCallback` when `pane_activation` is set, so mouse movement during
  the suppressed click doesn't leak to Chromium.
- **Issue 696** — Fixed double-suppression bug where both `pane_activation` and
  `overlay_activation` fired on the same click, requiring three clicks to
  interact. Removed the redundant `overlay_activation` set from
  `paneFocusChanged()` and added keyboard-clears-suppression in `keyCallback()`.

The overlay_activation flag (from Issue 606 Experiment 8) is now dead code — it
is never set to `true` anywhere. Issue 696 removed the only place that set it.

## Analysis

There are two suppression mechanisms to remove:

### 1. Pane activation suppression (`pane_activation` flag)

When a pane gains focus via `focusCallback()`, `pane_activation` is set to
`true`. This causes:

- `mouseButtonCallback()` (line ~4055): returns early, swallowing press+release.
- `cursorPosCallback()` (line ~4867): returns early, swallowing drag.
- `keyCallback()` (line ~2740): clears the flag (keyboard engagement bypass).

**To remove:** Delete the flag declaration, all places that set it, and all
guards that check it.

### 2. Overlay activation suppression (`overlay_activation` flag)

This flag was designed to suppress the click that transitions from control mode
to browse mode. It is now dead code — never set to `true` — but the guards still
exist:

- `mouseButtonCallback()` (line ~4069): checks `overlay_activation` to suppress
  press+release on the overlay.
- `mouseButtonCallback()` (line ~4120): clears `overlay_activation` when click
  misses overlay.

**To remove:** Delete the flag declaration and all guards.

### 3. Control→browse click forwarding

Currently, when a browser pane is in control mode and the user clicks the
overlay, `notifyOverlayClicked()` switches to browse mode but does NOT forward
the click to Chromium. The click is consumed by the mode switch.

After removing suppression, this click should both switch to browse mode AND
forward to Chromium, so the user's click lands where they intended.

### 4. Scroll forwarding in control mode

Currently, `scrollCallback()` (line ~3549) checks `isOverlayForwarding()` before
forwarding scrolls to Chromium. `isOverlayForwarding()` requires `p.browsing`
(browse mode) AND focused pane. So scrolls over a browser pane in control mode
fall through to the terminal instead of scrolling the webpage.

Scrolls should forward to Chromium whenever the cursor is over a browser
overlay, regardless of mode. This lets users scroll webpages without switching
to browse mode.

## Code locations

All in `gui/src/Surface.zig` unless noted:

| Location                         | What                                                          | Action                               |
| -------------------------------- | ------------------------------------------------------------- | ------------------------------------ |
| Line ~283                        | `overlay_activation: bool = false` declaration                | Delete                               |
| Line ~287                        | `pane_activation: bool = false` declaration                   | Delete                               |
| Line ~2740                       | `self.mouse.pane_activation = false` in `keyCallback()`       | Delete                               |
| Line ~3417-3420                  | Set `pane_activation = true` in `focusCallback()`             | Delete                               |
| Line ~3554                       | `isOverlayForwarding` check in `scrollCallback()`             | Change to forward regardless of mode |
| Line ~4055-4061                  | `pane_activation` guard in `mouseButtonCallback()`            | Delete                               |
| Line ~4069-4074                  | `overlay_activation` guard in `mouseButtonCallback()`         | Delete                               |
| Line ~4120                       | `overlay_activation = false` clear in `mouseButtonCallback()` | Delete                               |
| Line ~4867                       | `pane_activation` guard in `cursorPosCallback()`              | Delete                               |
| `gui/src/apprt/xpc.zig` ~801-809 | `notifyOverlayClicked()`                                      | Modify to also forward the click     |

## Experiments

### Experiment 1: Remove all click/scroll suppression

Remove both suppression flags, make overlay clicks in control mode forward to
Chromium, and forward scrolls regardless of mode.

#### Changes

**`gui/src/Surface.zig`:**

1. **Delete `overlay_activation` field** (line ~283) and `pane_activation` field
   (line ~287) from the mouse struct.

2. **Delete `pane_activation` clear in `keyCallback()`** (line ~2740):

   ```zig
   // Any keypress proves intentional engagement — cancel click suppression (Issue 696).
   self.mouse.pane_activation = false;
   ```

3. **Delete `pane_activation` set in `focusCallback()`** (lines ~3417-3420):

   ```zig
   // Set activation flag so the next click is consumed (Issue 670).
   if (focused) {
       self.mouse.pane_activation = true;
   }
   ```

4. **Change `scrollCallback()`** (line ~3554): Replace `isOverlayForwarding`
   with `hasOverlayPane` so scrolls forward to Chromium whenever the cursor is
   over the overlay, regardless of mode:

   ```zig
   // before:
   if (xpc.isOverlayForwarding(self)) {
   // after:
   if (xpc.hasOverlayPane(self)) {
   ```

5. **Delete `pane_activation` guard in `mouseButtonCallback()`** (lines
   ~4055-4061):

   ```zig
   // Suppress activation click — click-to-focus without pass-through (Issue 670).
   if (self.mouse.pane_activation) {
       if (action == .release) {
           self.mouse.pane_activation = false;
       }
       return true;
   }
   ```

6. **Restructure overlay click handling in `mouseButtonCallback()`** (lines
   ~4068-4111). Currently the logic is:

   ```
   if isOverlayForwarding → forward click (with overlay_activation guard)
   else if left press → notifyOverlayClicked (consume click, switch mode)
   ```

   Replace with:

   ```
   if hasOverlayPane:
       if not browsing → notifyOverlayClicked (switch to browse mode)
       forward click to Chromium (always)
   ```

   This means: if the overlay exists, always forward the click. If we were in
   control mode, also switch to browse mode first. Delete the entire
   `overlay_activation` guard block.

7. **Delete `overlay_activation` clear** (line ~4120):

   ```zig
   // Clear activation flag — click landed outside overlay (Exp 10).
   self.mouse.overlay_activation = false;
   ```

8. **Delete `pane_activation` guard in `cursorPosCallback()`** (line ~4867):

   ```zig
   // Suppress drag during activation — same as click suppression (Issue 670).
   if (self.mouse.pane_activation) return;
   ```

**`gui/src/apprt/xpc.zig`:**

9. **No change to `notifyOverlayClicked()`** — it still switches to browse mode
   and sends focus. The click forwarding happens in `mouseButtonCallback()`
   after calling it.

#### Verification

1. `cd gui && zig build` — must compile clean.
2. Open GUI with two split panes, one terminal and one browser.
3. Click the terminal pane, then click the browser pane — click should both
   focus and interact (no double-click needed).
4. While in control mode, scroll over the browser pane — page should scroll.
5. While in control mode, click the browser pane — should switch to browse mode
   AND the click should land on the page.
6. Drag on the browser pane from control mode — should select text.

#### Results

**Result: Success.** Net -35 lines.

Deleted both `pane_activation` and `overlay_activation` flags and all their
guards from `mouseButtonCallback()`, `cursorPosCallback()`, `focusCallback()`,
and `keyCallback()`. Restructured overlay click handling to always forward
clicks to Chromium — if in control mode, switches to browse mode first, then
forwards. Changed `scrollCallback()` to use `hasOverlayPane()` instead of
`isOverlayForwarding()`, so scrolls reach the webview regardless of mode or
focus.

All six verification steps pass. Clicks, drags, and scrolls propagate
immediately — no activation click needed.

## Conclusion

Removed the click-to-activate suppression system built across Issues 670, 695,
and 696. Deleted both `pane_activation` and `overlay_activation` flags and all
their guards. Mouse clicks, drags, and scrolls now propagate to content
immediately — no activation click required. Scrolls forward to the webview
regardless of mode or focus. Clicking a browser pane in control mode switches to
browse mode and forwards the click in the same action. Net -35 lines.
