+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 323: Shift-Click to Extend Selection

Extend text selection by Shift-clicking.

## Status

**Complete.** Shift-click selection extension working.

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

| Feature                    | Status  | Issue |
| -------------------------- | ------- | ----- |
| Keyboard input             | Working | 317   |
| Mouse move                 | Working | 319   |
| Left click                 | Working | 319   |
| Hover effects              | Working | 319   |
| Double-click (word select) | Working | 320   |
| Triple-click (line select) | Working | 320   |
| Scroll (trackpad)          | Working | 321   |
| Drag selection             | Working | 322   |

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

| Flag                     | Value  | Meaning                        |
| ------------------------ | ------ | ------------------------------ |
| `EVENTFLAG_SHIFT_DOWN`   | 1 << 1 | Shift key is pressed           |
| `EVENTFLAG_CONTROL_DOWN` | 1 << 2 | Control key is pressed         |
| `EVENTFLAG_ALT_DOWN`     | 1 << 3 | Alt/Option key is pressed      |
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

- [x] Shift-click extends existing selection
- [x] Extension works forwards and backwards
- [x] Works after double-click word selection
- [x] Works after triple-click line selection
- [x] Works after drag selection

## Next Steps (Other Mouse Input)

After Shift-click, these features remain:

| Feature         | Priority | Notes                                             |
| --------------- | -------- | ------------------------------------------------- |
| Cmd-click       | Medium   | Open links in new tab (may need special handling) |
| Right-click     | Medium   | Context menus                                     |
| Middle-click    | Low      | Paste or open in new tab                          |
| Cursor feedback | Low      | Change cursor shape over links, text              |

## Experiments

### Experiment 1: Pass Keyboard Modifiers to Click Events

**Status:** SUCCESS

**Hypothesis:** Converting WezTerm's keyboard modifiers to CEF flags and passing
them with click events will enable Shift-click selection extension.

**Approach:** The infrastructure already exists — `send_mouse_click` accepts
modifiers, we just pass 0. Add a conversion function and use it.

#### 1a. Add Modifier Conversion Function

In `mouseevent.rs`, add a helper function to convert WezTerm modifiers to CEF:

```rust
/// Convert WezTerm keyboard modifiers to CEF event flags (issue 323)
#[cfg(target_os = "macos")]
fn modifiers_to_cef_flags(mods: ::window::Modifiers) -> u32 {
    use ::window::Modifiers;
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

#### 1b. Update Press Handler

Include keyboard modifiers in the click event:

```rust
WMEK::Press(MousePress::Left) => {
    // Issue 322: Track button state for drag selection
    {
        let mut buttons = self.webview_mouse_buttons.borrow_mut();
        let state = buttons.entry(pane_id).or_insert(0);
        *state |= 0x10; // EVENTFLAG_LEFT_MOUSE_BUTTON
    }

    // Issue 323: Include keyboard modifiers for Shift-click
    let kb_modifiers = modifiers_to_cef_flags(event.modifiers);

    let click_count = self.compute_click_count(pane_id, cef_x, cef_y);
    log::info!(
        "[MOUSE] Press LEFT pane={} cef=({}, {}) click_count={} modifiers=0x{:x}",
        pane_id, cef_x, cef_y, click_count, kb_modifiers
    );
    xpc_manager.send_mouse_click(
        pane_id, cef_x, cef_y, 0, false, click_count as i32, kb_modifiers as i32
    );
    true
}
```

#### 1c. Update Release Handler

Include keyboard modifiers in the release event too:

```rust
WMEK::Release(MousePress::Left) => {
    // Issue 322: Clear button state
    {
        let mut buttons = self.webview_mouse_buttons.borrow_mut();
        if let Some(state) = buttons.get_mut(&pane_id) {
            *state &= !0x10;
        }
    }

    // Issue 323: Include keyboard modifiers
    let kb_modifiers = modifiers_to_cef_flags(event.modifiers);

    let click_count = {
        let states = self.click_state.borrow();
        states.get(&pane_id).map(|s| s.count).unwrap_or(1)
    };
    log::info!(
        "[MOUSE] Release LEFT pane={} cef=({}, {}) click_count={} modifiers=0x{:x}",
        pane_id, cef_x, cef_y, click_count, kb_modifiers
    );
    xpc_manager.send_mouse_click(
        pane_id, cef_x, cef_y, 0, true, click_count as i32, kb_modifiers as i32
    );
    true
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Shift-click extends selection
web google.com
# Double-click a word to select it
# Hold Shift, click on another word
# Expected: Selection extends from first word to second

# Test 2: Check logs
tail -f /tmp/termsurf-gui.log | grep "\[MOUSE\]"
# Shift-click should show modifiers=0x2

# Test 3: Backwards extension
# Select a word, Shift-click BEFORE it
# Expected: Selection extends backwards
```

#### Success Criteria

- [x] Log shows modifiers=0x2 when Shift is held during click
- [x] Shift-click after double-click extends selection
- [x] Shift-click after drag extends selection
- [x] Extension works forwards (click after selection)
- [x] Extension works backwards (click before selection)

---

## Conclusion

### What Was Accomplished

Shift-click selection extension for ts3 webviews is complete:

1. **Modifier conversion** — Added `modifiers_to_cef_flags()` helper that
   converts WezTerm's `Modifiers` bitmask to CEF event flags (Shift=0x02,
   Ctrl=0x04, Alt=0x08, Cmd=0x80).

2. **Click event modifiers** — Updated Press and Release handlers to include
   keyboard modifiers in click events sent to CEF.

3. **Selection extension** — Users can now Shift-click to extend selections made
   by double-click, triple-click, or drag, in either direction.

### What We Learned

1. **Infrastructure was already there** — The `send_mouse_click` method already
   accepted a modifiers parameter; we were just passing 0. A simple conversion
   function was all that was needed.

2. **CEF handles the logic** — Once CEF receives the Shift modifier with the
   click event, it handles selection extension automatically. No anchor tracking
   or range calculation needed on our side.

3. **All modifiers now work** — By converting the full modifier set, we also
   enabled Ctrl-click, Alt-click, and Cmd-click for future features like link
   opening or context menus.

### Implementation Summary

```
Shift-Click Flow:

User holds Shift + clicks
    │
    ▼
MouseEvent with modifiers.contains(SHIFT)
    │
    ▼
modifiers_to_cef_flags() → 0x02
    │
    ▼
send_mouse_click(..., modifiers=0x02)
    │
    ▼
CEF extends selection to click point
```

### Files Modified

| File            | Changes                                                          |
| --------------- | ---------------------------------------------------------------- |
| `mouseevent.rs` | Added `modifiers_to_cef_flags()`, updated Press/Release handlers |

### What's Next

With Shift-click complete, the remaining mouse input features are:

| Feature         | Priority | Notes                                                             |
| --------------- | -------- | ----------------------------------------------------------------- |
| Cmd-click       | Medium   | Open links in new tab (modifier now passed, needs link detection) |
| Right-click     | Medium   | Context menus                                                     |
| Middle-click    | Low      | Paste or open in new tab                                          |
| Cursor feedback | Low      | Change cursor shape over links, text                              |

Recommended next issue: **324-right-click** for context menu support, or
**Cmd-click link handling** if link detection is already working.

---

## References

- `docs/issues/0322-drag-selection.md` — Drag selection (completed)
- WezTerm `Modifiers` type in `window` crate
- CEF event flags: `cef_event_flags_t` in cef-rs bindings
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
