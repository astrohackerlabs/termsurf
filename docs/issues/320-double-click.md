# 320: Double-Click Support

Double-click to select words in webview panes.

## Status

Not started.

## Product Requirements

Users expect standard text selection behavior in web content:

1. **Double-click selects a word** — Clicking twice quickly on a word should
   highlight the entire word, matching browser behavior.

2. **Triple-click selects a line/paragraph** — Three rapid clicks should select
   the entire line or paragraph, depending on the element.

3. **Selection is visible** — Selected text should display with the standard
   highlight color.

4. **Selection can be extended** — After double-click selection, Shift-click
   should extend the selection to the clicked position.

## Background

### What Works (from Issue 319)

Issue 319 established basic mouse input for ts3 webviews:

| Feature | Status | Implementation |
|---------|--------|----------------|
| Mouse move | Working | `send_mouse_move()` via XPC |
| Left click | Working | `send_mouse_click()` via XPC |
| Hover effects | Working | CSS :hover triggers correctly |
| Coordinate transform | Working | Physical → logical with DPI scaling |
| Control panel exclusion | Working | Clicks above webview handled separately |

### Current Click Implementation

The existing click handler in `mouseevent.rs` sends a single click with
`click_count: 1`:

```rust
// From handle_webview_mouse_event()
xpc_manager.send_mouse_click(
    pane_id,
    cef_x,
    cef_y,
    scale,
    true,  // is_press
);
```

CEF's `send_mouse_click_event` accepts a `click_count` parameter that determines
selection behavior:

| click_count | CEF Behavior |
|-------------|--------------|
| 1 | Position cursor, no selection |
| 2 | Select word under cursor |
| 3 | Select line/paragraph |

### Architecture Reference

```
Mouse Click Flow:

User double-clicks
    │
    ▼
Window System (two rapid MousePress events)
    │
    ▼
mouse_event_impl() in mouseevent.rs
    │
    ▼
handle_webview_mouse_event()
    │
    ├─ [NEEDED] Track click timing
    ├─ [NEEDED] Count rapid clicks (1, 2, or 3)
    │
    └─ xpc_manager.send_mouse_click(... click_count)
            │
            ▼
        XPC to Profile Server
            │
            ▼
        CEF send_mouse_click_event(click_count)
            │
            ▼
        Word/line selection based on count
```

## Implementation Approach

### Click Counting Logic

Track recent clicks to detect double/triple clicks:

1. **Store last click info** — Position (x, y) and timestamp
2. **On new click** — Check if within time threshold (~500ms) and position
   threshold (~5 pixels)
3. **Increment or reset** — If thresholds met, increment count (max 3); otherwise
   reset to 1
4. **Send to CEF** — Pass computed click_count with the click event

### State Requirements

Need to track per-pane:
- Last click timestamp
- Last click position (x, y)
- Current click count (1, 2, or 3)

### Threshold Values

Standard double-click thresholds:
- **Time**: 500ms (typical OS default)
- **Distance**: 5 pixels (allow slight movement between clicks)

## Success Criteria

- [ ] Double-click selects word
- [ ] Triple-click selects line/paragraph
- [ ] Click count resets after timeout
- [ ] Click count resets if mouse moves too far
- [ ] Selection highlight is visible

## Next Steps (Other Mouse Input)

After double-click, these features remain for full mouse support:

| Feature | Priority | Notes |
|---------|----------|-------|
| Scroll wheel | High | `send_mouse_wheel()`, delta × 120 for CEF |
| Trackpad scroll | High | Same as wheel, may need gesture handling |
| Drag selection | Medium | Track button state across moves |
| Modifier keys | Medium | Shift-click, Cmd-click, Ctrl-click |
| Right-click | Medium | Context menu or forward to CEF |
| Middle-click | Low | Paste or open in new tab |
| Cursor feedback | Low | CEF → GUI reverse channel for cursor shape |

## Experiments

*No experiments yet.*

## References

- `docs/issues/319-mouse.md` — Basic mouse input (completed)
- `docs/issues/317-input.md` — Keyboard input (completed)
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
- `ts3/termsurf-profile/src/main.rs` — CEF mouse event handlers
