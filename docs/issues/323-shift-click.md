# 323: Shift-Click to Extend Selection

Extend text selection by Shift-clicking.

## Status

Not started.

## Product Requirements

Users need to extend existing selections using Shift-click:

1. **Extend selection** — After making an initial selection (via double-click,
   triple-click, or drag), Shift-clicking elsewhere should extend the selection
   from the original anchor point to the new click position.

2. **Works with all selection types** — Should work whether the initial
   selection was made by double-click (word), triple-click (line), or drag.

3. **Bidirectional extension** — Should extend forwards or backwards from the
   anchor point depending on where the Shift-click occurs.

4. **Standard browser behavior** — Should match how Chrome, Safari, and other
   browsers handle Shift-click selection extension.

## Background

### What Works (from Issues 317-322)

Previous issues established input infrastructure for ts3 webviews:

| Feature | Status | Issue |
|---------|--------|-------|
| Keyboard input | Working | 317 |
| Mouse move | Working | 319 |
| Left click | Working | 319 |
| Hover effects | Working | 319 |
| Double-click (word select) | Working | 320 |
| Triple-click (line select) | Working | 320 |
| Scroll (trackpad) | Working | 321 |
| Drag selection | Working | 322 |

### Current Click Implementation

The current click handler doesn't pass keyboard modifiers:

```rust
// From mouseevent.rs
xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, false, click_count as i32, 0);
//                                                                              ^^^
//                                                                          modifiers=0
```

The `send_mouse_click` signature already accepts modifiers:

```rust
pub fn send_mouse_click(
    &self,
    pane_id: PaneId,
    x: i32,
    y: i32,
    button: u32,
    is_up: bool,
    click_count: i32,
    modifiers: u32,  // Already supported, just not used
) -> bool
```

### CEF Modifier Flags

CEF uses these flags for keyboard modifiers:

| Flag | Value | Meaning |
|------|-------|---------|
| `EVENTFLAG_SHIFT_DOWN` | 1 << 1 | Shift key is pressed |
| `EVENTFLAG_CONTROL_DOWN` | 1 << 2 | Control key is pressed |
| `EVENTFLAG_ALT_DOWN` | 1 << 3 | Alt/Option key is pressed |
| `EVENTFLAG_COMMAND_DOWN` | 1 << 7 | Command key is pressed (macOS) |

For Shift-click selection extension, we need `0x02` (`EVENTFLAG_SHIFT_DOWN`).

### WezTerm Modifier Access

The mouse event includes modifier state:

```rust
// MouseEvent structure
pub struct MouseEvent {
    pub kind: MouseEventKind,
    pub coords: Point,
    pub screen_coords: ScreenPoint,
    pub modifiers: Modifiers,  // <-- Contains Shift, Ctrl, Alt, etc.
    pub mouse_buttons: MouseButtons,
}
```

The `Modifiers` type has methods like `contains(Modifiers::SHIFT)`.

### Architecture Reference

```
Shift-Click Flow:

User holds Shift + clicks
    │
    ▼
MouseEvent with modifiers.contains(SHIFT)
    │
    ▼
handle_webview_mouse_event()
    │
    ├─ [NEEDED] Convert WezTerm Modifiers to CEF flags
    │
    └─ send_mouse_click(..., modifiers=0x02)
            │
            ▼
        XPC to Profile Server
            │
            ▼
        CEF send_mouse_click_event(modifiers with SHIFT)
            │
            ▼
        CEF extends selection to click point
```

## Implementation Approach

### 1. Convert Modifiers

Create a helper to convert WezTerm modifiers to CEF flags:

```rust
fn wezterm_modifiers_to_cef(mods: &Modifiers) -> u32 {
    let mut flags = 0u32;
    if mods.contains(Modifiers::SHIFT) {
        flags |= 0x02; // EVENTFLAG_SHIFT_DOWN
    }
    if mods.contains(Modifiers::CTRL) {
        flags |= 0x04; // EVENTFLAG_CONTROL_DOWN
    }
    if mods.contains(Modifiers::ALT) {
        flags |= 0x08; // EVENTFLAG_ALT_DOWN
    }
    if mods.contains(Modifiers::SUPER) {
        flags |= 0x80; // EVENTFLAG_COMMAND_DOWN
    }
    flags
}
```

### 2. Pass Modifiers to Click Events

Update the click handlers to include keyboard modifiers:

```rust
WMEK::Press(MousePress::Left) => {
    let kb_modifiers = wezterm_modifiers_to_cef(&event.modifiers);
    // Combine with mouse button state if needed
    let modifiers = kb_modifiers | mouse_button_flags;
    xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, false, click_count, modifiers as i32);
    true
}
```

## Success Criteria

- [ ] Shift-click extends existing selection
- [ ] Extension works forwards and backwards
- [ ] Works after double-click word selection
- [ ] Works after triple-click line selection
- [ ] Works after drag selection

## Next Steps (Other Mouse Input)

After Shift-click, these features remain:

| Feature | Priority | Notes |
|---------|----------|-------|
| Cmd-click | Medium | Open links in new tab (may need special handling) |
| Right-click | Medium | Context menus |
| Middle-click | Low | Paste or open in new tab |
| Cursor feedback | Low | Change cursor shape over links, text |

## Experiments

*No experiments yet.*

## References

- `docs/issues/322-drag-selection.md` — Drag selection (completed)
- WezTerm `Modifiers` type in `window` crate
- CEF event flags: `cef_event_flags_t` in cef-rs bindings
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
