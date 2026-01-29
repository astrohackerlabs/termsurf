# ts3-10: Browser Overlay Leaks Across Tabs

## Summary

When a browser pane exists in one tab and the user opens a new tab, the browser
overlay incorrectly appears in the new tab and fills the entire window, covering
the tab bar. This is a critical bug that makes multi-tab usage broken.

## Prior Work: ts3-9 Resize Solution

Before tackling this issue, we completed the resize implementation in ts3-9:

### What We Built

1. **Debounce pattern** (from ts2): State on TermWindow tracks `pending_size`,
   `pending_since`, and `last_sent_size` to avoid flooding the browser with
   resize commands during rapid window resizing.

2. **Invalidate callback pattern**: XPC manager stores per-pane callbacks that
   trigger window redraws when new textures arrive from the profile server. This
   solved the issue where debounced resizes would complete but the window
   wouldn't redraw to show the new texture.

### Key Files Modified

- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` - Added `invalidate_callbacks`
  HashMap and methods to register/invoke callbacks when textures arrive
- `ts3/wezterm-gui/src/termwindow/render/draw.rs` - Added debounce logic and
  callback registration during first render
- `ts3/wezterm-gui/src/termwindow/mod.rs` - Added `WebviewResizeState` struct

### Experiments Completed

| Experiment | Description              | Result                                  |
| ---------- | ------------------------ | --------------------------------------- |
| 1          | Diagnostic logging       | Identified timing issues                |
| 2          | Remove debounce          | Confirmed debounce was working          |
| 3          | Correct debounce pattern | Failed - window didn't redraw after XPC |
| 4          | More diagnostic logging  | Found root cause - no redraw trigger    |
| 5          | Invalidate callback      | **Success** - resize works correctly    |

## Current Issue: Browser Overlay Leaks Across Tabs

### Steps to Reproduce

1. Open terminal, run `web google.com` in the first pane
2. Browser renders correctly in that pane
3. Press `Cmd+T` to open a new tab
4. **Bug**: The browser overlay appears in the new tab and fills the entire
   window, covering the tab bar

### Expected Behavior

1. New tab should open with a fresh terminal pane
2. No browser overlay should be visible (the `web` command was not run)
3. Tab bar should remain visible and functional

### Actual Behavior

1. New tab opens
2. Browser overlay from the previous tab appears
3. Overlay fills the entire window (not just pane area)
4. Tab bar is covered and inaccessible

### Problems Identified

1. **Wrong pane association**: The browser overlay is rendering in a tab/pane
   where no `web` command was issued. The overlay should only appear for panes
   that have an active webview session.

2. **Wrong size calculation**: The overlay is filling the entire window instead
   of being constrained to the pane's viewport. This suggests the size/position
   calculation is using window dimensions instead of pane dimensions.

## Next Steps

- [ ] Investigate how pane IDs are mapped to webview overlays
- [ ] Understand when/where overlay visibility is determined
- [ ] Trace the render path to find where the incorrect size is computed
- [ ] Fix pane association so overlays only render for their owning pane
- [ ] Fix size calculation to use pane viewport, not window size
