+++
status = "closed"
opened = "2026-03-02"
closed = "2026-03-02"
+++

# Issue 696: Double Click Suppression

Clicking an unfocused browser pane requires three clicks to interact: one to
focus, one that gets eaten, and one that finally goes through. Should only
require two (focus + interact).

## Why

Two independent click suppression flags — `pane_activation` (Issue 670) and
`overlay_activation` (Issue 606 Experiment 10) — both fire on the same click
when refocusing a pane that's in browse mode. Each flag eats one click, so two
clicks are consumed instead of one.

## How It Happens

Clicking an unfocused pane that's already in browse mode (e.g. clicked away to
another pane, now clicking back):

```
1. becomeFirstResponder() → focusCallback(true)
     → pane_activation = true

2. paneFocusChanged(true) → isOverlayBrowsing? YES
     → overlay_activation = true          ← BOTH flags now set

3. mouseButtonCallback(press)
     → pane_activation TRUE → consumed    ← first click eaten

4. mouseButtonCallback(release)
     → pane_activation TRUE → consumed, cleared

5. mouseButtonCallback(press)             ← user's SECOND click
     → pane_activation FALSE, continue
     → hit-test overlay → isOverlayForwarding → YES
     → overlay_activation TRUE → consumed  ← second click ALSO eaten

6. mouseButtonCallback(release)
     → overlay_activation TRUE → consumed, cleared

7. mouseButtonCallback(press)             ← THIRD click finally goes through
```

## Root Cause

`paneFocusChanged` (Surface.zig:3499) sets `overlay_activation = true` when a
pane gains focus while in browse mode. This was added in Issue 606 Experiment
10, before `pane_activation` existed. Issue 670 later added `pane_activation`,
which runs first in `mouseButtonCallback` and already suppresses the focus click
for all cases (terminal and overlay). The `overlay_activation` set in
`paneFocusChanged` is now redundant — it stacks on top of `pane_activation`,
consuming a second click.

The other place `overlay_activation` is set — in `notifyOverlayClicked()` for
control→browse mode transitions — is correct and unrelated. That path handles
activating browse mode, not refocusing.

## Experiment 1: Remove overlay_activation from paneFocusChanged

### Hypothesis

If we remove the `overlay_activation = true` set in `paneFocusChanged`, the
double-suppression disappears. `pane_activation` (set in `focusCallback`)
already handles focus-change click suppression for all cases. The
`overlay_activation` set in `notifyOverlayClicked()` remains — that covers the
separate control→browse activation path.

### Changes

One file, one deletion.

#### Surface.zig — remove overlay_activation from paneFocusChanged

Current code (line 3499):

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    const xpc = @import("apprt/xpc.zig");
    if (focused) {
        if (xpc.isOverlayBrowsing(self)) {
            self.mouse.overlay_activation = true;
        }
    }
    xpc.handlePaneFocusChanged(self, focused);
}
```

After:

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    const xpc = @import("apprt/xpc.zig");
    xpc.handlePaneFocusChanged(self, focused);
}
```

The `if (focused)` block is removed entirely. `pane_activation` (set in
`focusCallback` on the line above) already suppresses the activation click.

### What stays the same

- `pane_activation` in `focusCallback` (Issue 670) — unchanged
- `overlay_activation` in `notifyOverlayClicked` (Issue 606) — unchanged
- `cursorPosCallback` drag suppression (Issue 695) — unchanged

### Test

1. Open two split panes, both with browser overlays in browse mode
2. Click the unfocused pane → focuses (first click consumed, correct)
3. Click again → click goes through to Chromium (not consumed)
4. Verify: control→browse activation still works (click overlay in control mode,
   first click activates, second interacts)
5. Verify: terminal pane click-to-focus still works (Issue 670)

### Result: Success

Refocusing a browser pane in browse mode now requires two clicks (focus +
interact) instead of three. Control→browse activation and terminal
click-to-focus still work correctly.

## Experiment 2: Clear pane_activation on keypress

### Problem

Experiment 1 fixed the mouse-refocus path. But keyboard-driven focus still
over-suppresses:

```
1. Esc → control mode
2. Keyboard switch to another pane
3. Keyboard switch back to browser pane
     → focusCallback(true) → pane_activation = true
4. Enter → browse mode
     → keyCallback handles Enter, but pane_activation stays true
5. Mouse click → pane_activation TRUE → consumed  ← BUG
6. Second click → finally goes through
```

`pane_activation` is set when the pane gains focus (step 3), which is correct —
it protects against accidental clicks. But keyboard interaction (step 4) proves
the user is intentionally engaged with the pane. The flag should be cleared so
the next mouse click goes through.

### Hypothesis

If we clear `pane_activation` early in `keyCallback`, any keypress on a focused
pane will cancel click suppression. This covers Enter-to-browse and any other
keyboard interaction that proves intentional engagement.

### Changes

One file, one line.

#### Surface.zig — clear pane_activation in keyCallback

At the top of `keyCallback` (~line 2737), after crash metadata but before any
key processing:

```zig
// Any keypress proves intentional engagement — cancel click suppression (Issue 696).
self.mouse.pane_activation = false;
```

### What stays the same

- `pane_activation` set in `focusCallback` (Issue 670) — unchanged
- `pane_activation` consumed in `mouseButtonCallback` (Issue 670) — unchanged
- `pane_activation` guard in `cursorPosCallback` (Issue 695) — unchanged
- `overlay_activation` in `notifyOverlayClicked` (Issue 606) — unchanged

### Test

1. Focus a browser pane via keyboard, press Enter to browse, click → click goes
   through (not consumed)
2. Focus a browser pane via mouse click → first click consumed (correct), second
   goes through
3. Focus a terminal pane via keyboard, type → no click suppression on next mouse
   use
4. Focus a terminal pane via mouse click → first click consumed (correct)
5. Control→browse activation via overlay click still works (click overlay in
   control mode, first click activates, second interacts)

### Result: Success

All tests pass. Keyboard-driven focus followed by Enter to browse mode no longer
eats the first mouse click.

## Conclusion

Two experiments, two one-line fixes:

1. **Experiment 1:** Removed redundant `overlay_activation` set in
   `paneFocusChanged` — it stacked on top of `pane_activation`, consuming two
   clicks instead of one when refocusing via mouse.
2. **Experiment 2:** Clear `pane_activation` in `keyCallback` — any keypress
   proves intentional engagement, so click suppression is no longer needed when
   focus was gained via keyboard.

Both fixes are in `Surface.zig`. No new flags, no new structs. The
`pane_activation` flag (Issue 670) now correctly handles all focus paths: mouse
clicks consume the activation click, keyboard interaction cancels suppression.
