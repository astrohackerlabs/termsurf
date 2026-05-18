+++
status = "closed"
opened = "2026-02-28"
closed = "2026-02-28"
+++

# Issue 670: Click-to-Focus Without Pass-Through

When you click an unfocused split pane, the click both activates the pane and
passes through to the content. This causes accidental link clicks, text
selections, and other unintended interactions. macOS windows don't work this way
— clicking an inactive window only activates it; you must click again to
interact.

## Current Behavior

Clicking an unfocused split pane:

1. `becomeFirstResponder()` fires → pane gains focus
2. `mouseDown` fires → `termsurf_surface_mouse_button` → `mouseButtonCallback`
3. The click is forwarded to content (terminal or Chromium)

All three happen on the same click. The user wanted to focus the pane but
accidentally clicked a link, started a selection, or triggered some other
action.

## Desired Behavior

macOS-style click-to-focus:

1. First click on unfocused pane → only activates it, click is consumed
2. Second click → interacts with content normally

## Analysis

### Click chain

```
NSEvent mouseDown
  → SurfaceView_AppKit.swift mouseDown() (line 844)
    → termsurf_surface_mouse_button (C API)
      → Surface.zig mouseButtonCallback() (line 4030)
        → hit-test overlay → forward to Chromium (xpc.sendMouseEvent)
        → or: forward to terminal (mouse reporting / selection)
```

Focus happens separately via `becomeFirstResponder()` (line 769), which sets
`self.focused = true` and calls `focusDidChange(true)` → Zig's `focusCallback()`
(Surface.zig line 3399) → sets `self.focused = true`.

### Existing precedent

Issue 606 Experiment 10 added `overlay_activation` — a flag that suppresses the
activation click on Chromium overlays. When the overlay transitions from
not-forwarding to forwarding (i.e. activating browse mode), the press and
release are consumed. This proves the pattern works for overlays. But it doesn't
cover terminal content clicks, and it only applies to the overlay activation
case, not split pane focus changes.

### Where to fix

The Zig layer (`mouseButtonCallback`) is the right place. It already has
`self.focused` which tracks focus state. The fix:

1. When `mouseButtonCallback` receives a press and `self.focused` was `false`
   just before this click, consume the click.
2. The focus change (`focusCallback`) happens before or concurrently with the
   mouse event, so we need a flag that persists across the focus→click sequence.

The tricky part: `focusCallback` sets `self.focused = true` before
`mouseButtonCallback` runs (because `becomeFirstResponder` fires first). So by
the time the click arrives, `self.focused` is already `true`. We need a separate
flag.

## Experiment 1: Activation click suppression

### Hypothesis

A `pane_activation` flag on `Surface.mouse` — set by `focusCallback` when
gaining focus, cleared by the subsequent mouse release — will suppress the
pass-through click without affecting normal interactions.

### Changes

#### 1. Surface.zig — add `pane_activation` flag

In the `mouse` struct (near `overlay_activation`):

```zig
/// Set when this surface just gained focus. The next press+release
/// is consumed (click-to-focus without pass-through).
pane_activation: bool = false,
```

#### 2. Surface.zig — set flag in `focusCallback`

In `focusCallback()`, when gaining focus:

```zig
if (focused) {
    self.mouse.pane_activation = true;
}
```

#### 3. Surface.zig — consume click in `mouseButtonCallback`

At the top of `mouseButtonCallback`, after recording click state but before any
forwarding logic:

```zig
// Suppress activation click — click-to-focus without pass-through (Issue 670).
if (self.mouse.pane_activation) {
    if (action == .release) {
        self.mouse.pane_activation = false;
    }
    return true; // Consume the event.
}
```

This goes before the overlay hit-test block (line 4046), so it applies to both
terminal and browser clicks.

### Result: PASS

Click-to-focus works. Clicking an unfocused pane activates it without passing
the click through to content. A second click interacts normally. No regressions
with resize or other mouse behavior.

## Conclusion

macOS-style click-to-focus for split panes. One new flag (`pane_activation`) in
the mouse struct, three small edits in `Surface.zig`:

1. Flag declaration in the `mouse` struct
2. Set flag in `focusCallback` when gaining focus
3. Consume press+release in `mouseButtonCallback` when flag is set

Zig-only change — no Swift modifications needed. The fix applies to both
terminal and browser pane clicks because it sits above the overlay hit-test in
the click handling chain.
