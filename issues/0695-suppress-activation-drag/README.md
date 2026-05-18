+++
status = "closed"
opened = "2026-03-02"
closed = "2026-03-02"
+++

# Issue 695: Suppress Activation Drag

When clicking an unfocused pane to focus it, the first click is correctly
suppressed (Issue 670). But if the mouse moves slightly during the click — which
happens naturally when clicking quickly — the drag leaks through to the browser
overlay.

## Why

The `pane_activation` flag suppresses press and release in
`mouseButtonCallback`, but `cursorPosCallback` has no corresponding check. Mouse
moves during the suppressed click are forwarded to Chromium as drags because
`click_state[left]` is set to `.press` before the suppression check consumes the
click.

## How It Leaks

```
1. focusCallback(true) → pane_activation = true
2. mouseButtonCallback(press) → click_state[left] = .press (line 4055)
                               → pane_activation check consumes the click (line 4058)
3. cursorPosCallback(move) → hitTestOverlay → isOverlayForwarding
                           → sendMouseMove reads click_state[left] == .press
                           → Chromium receives a drag event  ← BUG
4. mouseButtonCallback(release) → consumed, pane_activation = false
```

The move in step 3 has no `pane_activation` guard, so it slips through.

## Experiment 1: Guard cursorPosCallback with pane_activation

### Hypothesis

If we add a `pane_activation` early-return to `cursorPosCallback`, mouse moves
during the activation click will be suppressed — preventing accidental drags
from reaching Chromium or the terminal.

### Changes

One file, one line.

#### Surface.zig — guard `cursorPosCallback`

After the crash metadata setup (~line 4864), before the overlay hit-test:

```zig
// Suppress drag during activation — same as click suppression (Issue 670).
if (self.mouse.pane_activation) return;
```

No new flags, no new structs. Reuses the existing `pane_activation` flag from
Issue 670.

### What stays the same

- `mouseButtonCallback` suppression logic (Issue 670) — unchanged
- `overlay_activation` suppression logic (Issue 606) — unchanged
- All mouse behavior after the activation click+release — unchanged

### Test

1. Open two split panes, both with browser overlays
2. Click the unfocused pane quickly with slight mouse movement
3. The pane focuses but no drag is sent to Chromium (no text selection, no
   accidental link hover change)
4. Second click interacts normally — clicks, drags, selections all work
5. Terminal panes: same behavior, no accidental terminal text selection on focus
   click

### Result: Success

All tests pass. Clicking an unfocused pane with slight mouse movement no longer
sends accidental drags to Chromium. Second click interacts normally.

## Conclusion

One-line fix. `cursorPosCallback` now checks `pane_activation` and returns
early, matching the existing suppression in `mouseButtonCallback` (Issue 670).
No new flags — reuses the same `pane_activation` bool that already gates clicks.
