# 322: Drag Selection

Click-and-drag to select text in webview panes.

## Status

**Complete.** Click-and-drag text selection working.

## Product Requirements

Users need to select arbitrary text ranges by clicking and dragging:

1. **Click-and-drag selection** — Press mouse button, drag across text, release
   to select. The selection should highlight as the user drags.

2. **Visual feedback** — Selected text should display with standard highlight
   color during and after the drag operation.

3. **Selection persistence** — After releasing the mouse, the selection should
   remain until the user clicks elsewhere or performs another action.

4. **Copy support** — Selected text should be copyable via Cmd+C (already
   working from keyboard input issue 317).

5. **Extend selection** — Shift-click after an initial selection should extend
   the selection to the new click position (requires modifier key support).

## Background

### What Works (from Issues 317, 319, 320, 321)

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

### How Drag Selection Works in Browsers

Drag selection requires tracking mouse button state across move events:

1. **Mouse down** — Start potential selection at click position
2. **Mouse move (while button held)** — Extend selection to current position
3. **Mouse up** — Finalize selection

CEF receives these as separate events and handles selection internally. The key
is that mouse move events must indicate when the button is pressed.

### Current Mouse Move Implementation

The current `send_mouse_move()` doesn't track button state:

```rust
// From webview_xpc.rs
pub fn send_mouse_move(
    &self,
    pane_id: PaneId,
    x: i32,
    y: i32,
    modifiers: u32,  // Currently always 0
) -> bool
```

CEF's `send_mouse_move_event` signature:

```c
void send_mouse_move_event(
    const cef_mouse_event_t* event,  // x, y, modifiers
    int mouse_leave                   // 1 if mouse left the view
);
```

The `modifiers` field in `cef_mouse_event_t` can include mouse button flags:

| Flag                            | Value  | Meaning                  |
| ------------------------------- | ------ | ------------------------ |
| `EVENTFLAG_LEFT_MOUSE_BUTTON`   | 1 << 4 | Left button is pressed   |
| `EVENTFLAG_MIDDLE_MOUSE_BUTTON` | 1 << 5 | Middle button is pressed |
| `EVENTFLAG_RIGHT_MOUSE_BUTTON`  | 1 << 6 | Right button is pressed  |

### Architecture Reference

```
Drag Selection Flow:

1. Mouse Press (start selection)
    │
    ▼
send_mouse_click(is_up=false)
    │
    ├─ [NEEDED] Track that left button is now held
    │
    ▼
CEF positions cursor at click point

2. Mouse Move (extend selection)
    │
    ▼
send_mouse_move(modifiers with LEFT_MOUSE_BUTTON flag)
    │
    ▼
CEF extends selection to new position

3. Mouse Release (end selection)
    │
    ▼
send_mouse_click(is_up=true)
    │
    ├─ [NEEDED] Clear left button held state
    │
    ▼
CEF finalizes selection
```

## Implementation Approach

### 1. Track Button State

Add state to track which mouse buttons are currently pressed:

```rust
// In TermWindow or as part of webview state
mouse_buttons_held: RefCell<HashSet<MousePress>>,
```

Update on press/release events.

### 2. Include Button State in Move Events

When sending mouse move, check if buttons are held and set modifiers:

```rust
let mut modifiers = 0u32;
if self.mouse_buttons_held.borrow().contains(&MousePress::Left) {
    modifiers |= 0x10; // EVENTFLAG_LEFT_MOUSE_BUTTON
}
xpc_manager.send_mouse_move(pane_id, cef_x, cef_y, modifiers);
```

### 3. Verify CEF Receives Button State

The profile server's `MouseMoveTask` already passes modifiers to CEF:

```rust
let mouse_event = cef::MouseEvent {
    x: self.x,
    y: self.y,
    modifiers: self.modifiers,  // Just need to populate this
};
host.send_mouse_move_event(Some(&mouse_event), 0);
```

## Success Criteria

- [x] Click-and-drag selects text
- [x] Selection highlights during drag
- [x] Selection persists after mouse release
- [x] Can copy selection with Cmd+C
- [x] Multiple drag selections work (new drag clears old selection)

## Next Steps (Other Mouse Input)

After drag selection, these features remain:

| Feature         | Priority | Notes                                                |
| --------------- | -------- | ---------------------------------------------------- |
| Modifier keys   | Medium   | Shift-click to extend selection, Cmd-click for links |
| Right-click     | Medium   | Context menus                                        |
| Middle-click    | Low      | Paste or open in new tab                             |
| Cursor feedback | Low      | Change cursor shape over links, text                 |

## Experiments

### Experiment 1: Track Button State in Move Events

**Status:** SUCCESS

**Hypothesis:** Including the left mouse button flag in mouse move events when
the button is held will enable CEF to perform drag selection.

**Approach:** Track button state per-pane and include it in the modifiers field
when sending mouse move events.

#### 1a. Add Button State Tracking

In `mod.rs`, add a field to TermWindow (similar to click_state):

```rust
/// Per-pane mouse button state for drag detection (issue 322)
#[cfg(target_os = "macos")]
webview_mouse_buttons: RefCell<HashMap<PaneId, u32>>,
```

Initialize in `new_window()`:

```rust
#[cfg(target_os = "macos")]
webview_mouse_buttons: RefCell::new(HashMap::new()),
```

The value is a bitmask of held buttons using CEF's event flags:
- `0x10` = `EVENTFLAG_LEFT_MOUSE_BUTTON`
- `0x20` = `EVENTFLAG_MIDDLE_MOUSE_BUTTON`
- `0x40` = `EVENTFLAG_RIGHT_MOUSE_BUTTON`

#### 1b. Update Press Handler

Set the button flag on press:

```rust
WMEK::Press(MousePress::Left) => {
    // Issue 322: Track button state for drag selection
    {
        let mut buttons = self.webview_mouse_buttons.borrow_mut();
        let state = buttons.entry(pane_id).or_insert(0);
        *state |= 0x10; // EVENTFLAG_LEFT_MOUSE_BUTTON
    }

    let click_count = self.compute_click_count(pane_id, cef_x, cef_y);
    log::info!(
        "[MOUSE] Press LEFT pane={} cef=({}, {}) click_count={}",
        pane_id, cef_x, cef_y, click_count
    );
    xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, false, click_count as i32, 0);
    true
}
```

#### 1c. Update Release Handler

Clear the button flag on release:

```rust
WMEK::Release(MousePress::Left) => {
    // Issue 322: Clear button state
    {
        let mut buttons = self.webview_mouse_buttons.borrow_mut();
        if let Some(state) = buttons.get_mut(&pane_id) {
            *state &= !0x10; // Clear EVENTFLAG_LEFT_MOUSE_BUTTON
        }
    }

    let click_count = {
        let states = self.click_state.borrow();
        states.get(&pane_id).map(|s| s.count).unwrap_or(1)
    };
    log::info!(
        "[MOUSE] Release LEFT pane={} cef=({}, {}) click_count={}",
        pane_id, cef_x, cef_y, click_count
    );
    xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, true, click_count as i32, 0);
    true
}
```

#### 1d. Update Move Handler

Include button state in modifiers:

```rust
WMEK::Move => {
    // Issue 322: Include button state for drag selection
    let modifiers = {
        let buttons = self.webview_mouse_buttons.borrow();
        *buttons.get(&pane_id).unwrap_or(&0)
    };

    log::info!(
        "[MOUSE] Move pane={} cef=({}, {}) modifiers=0x{:x}",
        pane_id, cef_x, cef_y, modifiers
    );
    xpc_manager.send_mouse_move(pane_id, cef_x, cef_y, modifiers);
    true
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Drag selection
web google.com
# Click and drag across text
# Expected: Text highlights as you drag

# Test 2: Check logs
tail -f /tmp/termsurf-gui.log | grep "\[MOUSE\]"
# During drag: modifiers=0x10
# After release: modifiers=0x0

# Test 3: Copy selection
# Select text with drag, press Cmd+C
# Paste in terminal to verify
```

#### Success Criteria

- [x] Log shows modifiers=0x10 during drag
- [x] Log shows modifiers=0x0 after release
- [x] Text highlights as user drags
- [x] Selection persists after mouse release
- [x] Can copy selection with Cmd+C

---

## Conclusion

### What Was Accomplished

Drag selection for ts3 webviews is complete:

1. **Click-and-drag selection** — Press left button, drag across text, release
   to select. Text highlights in real-time as the user drags.

2. **Button state tracking** — Per-pane `webview_mouse_buttons` HashMap stores
   which buttons are currently held using CEF's event flag bitmask.

3. **Modifier propagation** — Mouse move events include the button state in
   the modifiers field, telling CEF when to extend selection.

4. **Copy support** — Selected text can be copied with Cmd+C (from issue 317).

### What We Learned

1. **CEF needs button state in move events** — Unlike some APIs that track drag
   state internally, CEF requires the `EVENTFLAG_LEFT_MOUSE_BUTTON` flag (0x10)
   in every mouse move event during a drag operation.

2. **Simple bitmask approach works** — Using a u32 bitmask per pane is simpler
   than tracking individual button states. The same pattern can support middle
   and right button drags if needed.

3. **Existing infrastructure was ready** — The XPC `send_mouse_move()` already
   accepted a modifiers parameter; we just weren't populating it.

### Implementation Summary

```
Drag Selection Flow:

Press LEFT
    │
    ├─ webview_mouse_buttons[pane] |= 0x10
    │
    └─ send_mouse_click(is_up=false)

Move (during drag)
    │
    ├─ modifiers = webview_mouse_buttons[pane]  // 0x10
    │
    └─ send_mouse_move(modifiers=0x10)
           │
           ▼
       CEF extends selection

Release LEFT
    │
    ├─ webview_mouse_buttons[pane] &= !0x10
    │
    └─ send_mouse_click(is_up=true)
           │
           ▼
       CEF finalizes selection
```

### Files Modified

| File | Changes |
|------|---------|
| `mod.rs` | Added `webview_mouse_buttons: RefCell<HashMap<PaneId, u32>>` |
| `mouseevent.rs` | Updated Press/Release/Move handlers for button tracking |

### What's Next

With drag selection complete, these mouse features remain:

| Feature | Priority | Notes |
|---------|----------|-------|
| Modifier keys | Medium | Shift-click to extend, Cmd-click for links |
| Right-click | Medium | Context menus |
| Middle-click | Low | Paste or open in new tab |
| Cursor feedback | Low | Change cursor shape over links, text |

Recommended next issue: **323-modifier-keys** for Shift-click selection extension
and Cmd-click link handling.

---

## References

- `docs/issues/319-mouse.md` — Basic mouse input (completed)
- `docs/issues/320-double-click.md` — Double/triple click (completed)
- `docs/issues/321-scroll.md` — Scroll support (completed)
- CEF event flags: `cef_event_flags_t` in cef-rs bindings
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
- `ts3/termsurf-profile/src/main.rs` — CEF event handlers
