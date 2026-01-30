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

## Experiment 1: Filter Overlays by Active Tab

### Hypothesis

The render loop iterates over ALL webview overlays globally, regardless of which
tab is active. When a pane from Tab A is rendered while Tab B is active, the
pane isn't found in Tab B's layout, triggering a fallback to full-window
coordinates that covers the tab bar.

**If we store tab_id in WebviewOverlay and filter by active tab during render,
overlays will only appear in their owning tab.**

### Technical Analysis

#### Current Flow (Broken)

1. `webview_panes.overlays` is a global `HashMap<PaneId, WebviewOverlay>`
2. Render loop at `draw.rs:221` iterates ALL overlays without tab filtering:
   ```rust
   for (pane_id, _overlay) in webview_panes.overlays.iter() {
   ```
3. `positioned_panes` contains only the ACTIVE tab's panes (from
   `get_panes_to_render()`)
4. When Tab A's pane isn't found in Tab B's layout, the fallback kicks in:
   ```rust
   None => {
       log::warn!("[Render] Pane {} not found in layout, using full window", pane_id);
       (0.0, 0.0, window_width, window_height)  // Covers tab bar!
   }
   ```

#### Fix: Store and Filter by Tab ID

**Step 1: Add tab_id to WebviewOverlay** (`webview_socket.rs`)

```rust
pub struct WebviewOverlay {
    pub session_id: String,
    pub tab_id: TabId,  // NEW
}
```

When creating the overlay in `handle_request` (open_webview action), capture the
tab_id from the pane's containing tab.

**Step 2: Filter overlays in render loop** (`draw.rs`)

Before iterating overlays, get the active tab and skip overlays from other tabs:

```rust
let active_tab_id = match mux.get_active_tab_for_window(self.mux_window_id) {
    Some(tab) => tab.tab_id(),
    None => return Ok(()),
};

for (pane_id, overlay) in webview_panes.overlays.iter() {
    if overlay.tab_id != active_tab_id {
        continue;  // Skip overlays from other tabs
    }
    // ... rest of render logic ...
}
```

**Step 3: Clean up overlays when tabs close** (future consideration)

Add cleanup logic to remove overlays when their tab is closed. This prevents
stale overlays from accumulating.

### Files to Modify

| File | Changes |
|------|---------|
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add `tab_id` field to `WebviewOverlay`, populate when creating overlay |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Filter overlay iteration by active tab_id |

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Multi-tab isolation
web google.com        # Tab 1 shows browser
Cmd+T                 # Tab 2 should be clean terminal
# Expected: No browser overlay in Tab 2

# Test 2: Tab switching
# Switch back to Tab 1
# Expected: Browser overlay reappears correctly

# Test 3: Multiple browser tabs
# In Tab 2: web github.com
# Expected: Each tab shows its own browser, no cross-contamination
```

### Success Criteria

- [x] New tabs do not show browser overlays from other tabs
- [x] Tab bar remains visible when switching tabs
- [x] Browser overlay reappears when switching back to its owning tab
- [x] Multiple browser panes in different tabs work independently

### Result: Success

Experiment 1 fixed the tab leak issue. The fix stores `tab_id` in each
`WebviewOverlay` when created, then filters overlays during render to only
display those belonging to the active tab.
