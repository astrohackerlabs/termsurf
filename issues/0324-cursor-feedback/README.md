+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 324: Cursor Feedback

Display appropriate cursor shapes when hovering over webview content.

## Status

**Complete.** Cursor feedback working for webviews.

## Product Requirements

Users need visual cursor feedback when interacting with webviews:

1. **Text cursor (I-beam)** — When hovering over selectable text, display the
   text selection cursor to indicate the user can select.

2. **Pointer cursor (hand)** — When hovering over clickable links, display the
   pointer/hand cursor to indicate the element is clickable.

3. **Default cursor (arrow)** — When hovering over non-interactive content,
   display the standard arrow cursor.

4. **Other cursors** — Support additional cursor types as needed (resize
   handles, wait/busy, not-allowed, etc.).

5. **Smooth transitions** — Cursor should change immediately as the mouse moves
   between different element types.

## Background

### What Works (from Issues 317-323)

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
| Shift-click extend         | Working | 323   |

### Current Behavior

Currently, the cursor does not change when hovering over webview content. It
likely stays as the default arrow regardless of what element is under the mouse.

### CEF Cursor Change API

CEF provides cursor feedback through the `CefRenderHandler::OnCursorChange`
callback:

```cpp
void OnCursorChange(
    CefRefPtr<CefBrowser> browser,
    CefCursorHandle cursor,      // Platform-specific cursor handle
    cef_cursor_type_t type,      // Cursor type enum
    const CefCursorInfo& info    // Custom cursor info (if type is CT_CUSTOM)
);
```

The `cef_cursor_type_t` enum includes:

| Type             | Value | Description                    |
| ---------------- | ----- | ------------------------------ |
| `CT_POINTER`     | 0     | Default arrow                  |
| `CT_CROSS`       | 1     | Crosshair                      |
| `CT_HAND`        | 2     | Hand/pointer for links         |
| `CT_IBEAM`       | 3     | I-beam for text                |
| `CT_WAIT`        | 4     | Busy/wait                      |
| `CT_HELP`        | 5     | Help cursor                    |
| `CT_EASTRESIZE`  | 6     | Resize east                    |
| `CT_NORTHRESIZE` | 7     | Resize north                   |
| `CT_MOVE`        | 13    | Move cursor                    |
| `CT_NOTALLOWED`  | 28    | Not allowed                    |
| ...              | ...   | Many more resize/special types |

### Architecture Reference

```
Cursor Feedback Flow:

CEF detects hover over link
    │
    ▼
CefRenderHandler::OnCursorChange(type=CT_HAND)
    │
    ▼
Profile Server receives callback
    │
    ├─ [NEEDED] Send cursor type to GUI via XPC
    │
    ▼
XPC message: { action: "cursor_change", cursor_type: 2 }
    │
    ▼
GUI receives message
    │
    ├─ [NEEDED] Map CEF cursor type to WezTerm/window cursor
    │
    └─ context.set_cursor(MouseCursor::Hand)
            │
            ▼
        Window displays hand cursor
```

### WezTerm Cursor Types

WezTerm's `window` crate provides `MouseCursor` enum:

```rust
pub enum MouseCursor {
    Arrow,
    Hand,
    Text,
    SizeUpDown,
    SizeLeftRight,
    // ... others
}
```

We need to map CEF's `cef_cursor_type_t` to WezTerm's `MouseCursor`.

### Current XPC Infrastructure

The GUI already receives XPC messages from the profile server for IOSurface
updates. We can add a new message type for cursor changes:

```rust
// Profile server sends:
{
    "action": "cursor_change",
    "pane_id": 123,
    "cursor_type": 2  // CT_HAND
}

// GUI receives and applies cursor
```

## Implementation Approach

### 1. Profile Server: Handle OnCursorChange

In `termsurf-profile`, implement the CEF callback to send cursor type to GUI:

```rust
fn on_cursor_change(
    &self,
    _browser: &Browser,
    cursor: cef::CursorHandle,
    type_: cef::CursorType,
    _custom_cursor_info: &cef::CursorInfo,
) {
    // Send cursor type to GUI via XPC
    if let Some(connection) = self.gui_connection.as_ref() {
        let msg = xpc_dictionary_create(null(), null(), 0);
        xpc_dictionary_set_string(msg, "action", "cursor_change");
        xpc_dictionary_set_int64(msg, "cursor_type", type_ as i64);
        xpc_connection_send_message(connection, msg);
    }
}
```

### 2. GUI: Receive Cursor Change Messages

In `webview_xpc.rs`, handle the new message type:

```rust
"cursor_change" => {
    let cursor_type = xpc_dictionary_get_int64(event, "cursor_type") as u32;
    // Store cursor type for this pane
    // Trigger window invalidation to apply cursor
}
```

### 3. GUI: Apply Cursor in Mouse Handler

When processing mouse events over webviews, apply the stored cursor:

```rust
fn handle_webview_mouse_event(&mut self, event: &MouseEvent) -> bool {
    // ... existing code ...

    // Apply cursor feedback
    if let Some(cursor) = self.get_webview_cursor(pane_id) {
        context.set_cursor(Some(cursor));
    }

    // ... rest of handler ...
}
```

### 4. Cursor Type Mapping

Map CEF cursor types to WezTerm cursors:

```rust
fn cef_cursor_to_wezterm(cef_type: u32) -> MouseCursor {
    match cef_type {
        0 => MouseCursor::Arrow,      // CT_POINTER
        2 => MouseCursor::Hand,       // CT_HAND
        3 => MouseCursor::Text,       // CT_IBEAM
        6 | 8 => MouseCursor::SizeLeftRight,  // CT_EASTRESIZE, CT_WESTRESIZE
        7 | 9 => MouseCursor::SizeUpDown,     // CT_NORTHRESIZE, CT_SOUTHRESIZE
        _ => MouseCursor::Arrow,      // Default for unsupported types
    }
}
```

## Success Criteria

- [x] I-beam cursor appears when hovering over selectable text
- [x] Hand/pointer cursor appears when hovering over links
- [x] Arrow cursor appears when hovering over non-interactive areas
- [x] Cursor changes are immediate (no noticeable delay)
- [x] Cursor reverts to arrow when leaving the webview

## Next Steps (Other Mouse Input)

After cursor feedback, these features remain:

| Feature      | Priority | Notes                                                  |
| ------------ | -------- | ------------------------------------------------------ |
| Cmd-click    | Medium   | Open links (modifiers now passed, needs link handling) |
| Right-click  | Medium   | Context menus                                          |
| Middle-click | Low      | Paste or open in new tab                               |

## Experiments

### Experiment 1: Send Cursor Type via XPC

**Status:** SUCCESS

**Hypothesis:** Adding `on_cursor_change` to the DisplayHandler and sending the
cursor type to the GUI via XPC will enable cursor feedback in webviews.

**Approach:** The DisplayHandler already exists in termsurf-profile (for URL
changes). Add the cursor change callback, send XPC messages, handle them in the
GUI, and apply the cursor in mouse event handling.

#### 1a. Profile Server: Add on_cursor_change to DisplayHandler

In `termsurf-profile/src/main.rs`, extend the DisplayHandler to handle cursor
changes. The `on_cursor_change` callback is part of `ImplDisplayHandler`:

```rust
wrap_display_handler! {
    pub struct ProfileDisplayHandler {
        inner: DisplayHandlerInner,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut Browser>,
            _frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            if let Some(url) = url {
                let url_str = url.to_string();
                println!("Profile: URL changed to '{}'", url_str);
                *self.inner.state.url.lock().unwrap() = url_str;
            }
        }

        // Issue 324: Cursor feedback
        fn on_cursor_change(
            &self,
            _browser: Option<&mut Browser>,
            _cursor: cef::CursorHandle,
            type_: cef::CursorType,
            _custom_cursor_info: Option<&cef::CursorInfo>,
        ) {
            // Convert CursorType to i64 for XPC
            let cursor_type: i64 = type_.into();
            println!("Profile: Cursor changed to type {}", cursor_type);

            // Send to GUI via XPC
            let msg = termsurf_xpc::XpcDictionary::new();
            msg.set_string("action", "cursor_change");
            msg.set_i64("cursor_type", cursor_type);
            self.inner.state.gui.send(&msg);
        }
    }
}
```

Note: Need to add `CursorType` to the imports in the `cef_imports!` block.

#### 1b. GUI: Add Cursor Storage to XpcManager

In `webview_xpc.rs`, add a field to store cursor type per pane:

```rust
pub struct XpcManager {
    // ... existing fields ...

    /// Current cursor type per pane (CEF cursor type value)
    /// Issue 324: Cursor feedback
    webview_cursors: Mutex<HashMap<PaneId, i64>>,
}

impl XpcManager {
    pub fn new() -> Self {
        Self {
            // ... existing init ...
            webview_cursors: Mutex::new(HashMap::new()),
        }
    }

    /// Get cursor type for a pane (issue 324)
    pub fn get_cursor(&self, pane_id: PaneId) -> Option<i64> {
        self.webview_cursors.lock().unwrap().get(&pane_id).copied()
    }
}
```

#### 1c. GUI: Handle cursor_change XPC Message

In the XPC event handler, add a case for `cursor_change`:

```rust
if action == "display_surface" {
    // ... existing code ...
} else if action == "cursor_change" {
    // Issue 324: Cursor feedback
    let cursor_type = msg.get_i64("cursor_type");
    log::info!(
        "[XPC Manager] Cursor change: session={} type={}",
        session_id, cursor_type
    );

    // Look up pane_id from session
    let pane_id = {
        let pending = manager.pending_sessions.lock().unwrap();
        pending.get(&session_id).copied()
    };

    if let Some(pane_id) = pane_id {
        manager
            .webview_cursors
            .lock()
            .unwrap()
            .insert(pane_id, cursor_type);

        // Trigger invalidation to update cursor
        if let Some(callback) = manager
            .invalidate_callbacks
            .lock()
            .unwrap()
            .get(&pane_id)
        {
            callback();
        }
    }
}
```

#### 1d. GUI: Apply Cursor in Mouse Handler

In `mouseevent.rs`, add a helper to convert CEF cursor type to WezTerm cursor:

```rust
/// Convert CEF cursor type to WezTerm MouseCursor (issue 324)
#[cfg(target_os = "macos")]
fn cef_cursor_to_mouse_cursor(cef_type: i64) -> MouseCursor {
    match cef_type {
        0 => MouseCursor::Arrow,          // CT_POINTER
        2 => MouseCursor::Hand,           // CT_HAND
        3 => MouseCursor::Text,           // CT_IBEAM
        6 | 8 => MouseCursor::SizeLeftRight,   // CT_EASTRESIZE, CT_WESTRESIZE
        7 | 9 => MouseCursor::SizeUpDown,      // CT_NORTHRESIZE, CT_SOUTHRESIZE
        _ => MouseCursor::Arrow,          // Default for unsupported
    }
}
```

Then update `handle_webview_mouse_event` to apply the cursor on Move events:

```rust
WMEK::Move => {
    // Issue 322: Include button state for drag selection
    let modifiers = {
        let buttons = self.webview_mouse_buttons.borrow();
        *buttons.get(&pane_id).unwrap_or(&0)
    };

    // Issue 324: Apply cursor feedback
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        if let Some(cursor_type) = xpc_manager.get_cursor(pane_id) {
            let cursor = cef_cursor_to_mouse_cursor(cursor_type);
            // Note: Need access to context here - may need to restructure
        }
    }

    log::info!(
        "[MOUSE] Move pane={} cef=({}, {}) modifiers=0x{:x}",
        pane_id, cef_x, cef_y, modifiers
    );
    xpc_manager.send_mouse_move(pane_id, cef_x, cef_y, modifiers);
    true
}
```

**Challenge:** The current `handle_webview_mouse_event` returns `bool` and
doesn't have access to `context` to call `set_cursor`. We need to either:

- Return the cursor to apply from the handler, or
- Store the cursor on TermWindow state and apply it after the handler returns

#### 1e. Alternative: Return Cursor from Handler

Modify `handle_webview_mouse_event` to return `Option<MouseCursor>`:

```rust
/// Handle mouse events for webview panes.
/// Returns Some(cursor) if the event was consumed and cursor should be set.
fn handle_webview_mouse_event(&mut self, event: &MouseEvent) -> Option<MouseCursor> {
    // ... existing code ...

    match &event.kind {
        WMEK::Move => {
            // ... existing move handling ...

            // Issue 324: Return cursor to apply
            let cursor = if let Some(xpc_manager) = get_xpc_manager() {
                xpc_manager
                    .get_cursor(pane_id)
                    .map(cef_cursor_to_mouse_cursor)
            } else {
                None
            };

            xpc_manager.send_mouse_move(pane_id, cef_x, cef_y, modifiers);
            return cursor.or(Some(MouseCursor::Arrow));
        }
        // ... other cases return Some(MouseCursor::Arrow) ...
    }
}
```

Then in `mouse_event_impl`, apply the cursor:

```rust
#[cfg(target_os = "macos")]
if let Some(cursor) = self.handle_webview_mouse_event(&event) {
    context.set_cursor(Some(cursor));
    return;
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Cursor over link
web google.com
# Hover over a link (e.g., "Gmail" in top right)
# Expected: Cursor changes to hand/pointer

# Test 2: Cursor over text
# Hover over regular text on the page
# Expected: Cursor changes to I-beam (text cursor)

# Test 3: Cursor over non-interactive
# Hover over background/images
# Expected: Cursor is arrow

# Test 4: Check logs
tail -f /tmp/termsurf-profile-*.log | grep "Cursor"
# Should see: "Profile: Cursor changed to type X"

tail -f /tmp/termsurf-gui.log | grep "Cursor"
# Should see: "[XPC Manager] Cursor change: session=X type=Y"
```

#### Success Criteria

- [x] Profile server logs show cursor type changes
- [x] GUI logs show received cursor_change messages
- [x] Hand cursor appears over links
- [x] I-beam cursor appears over selectable text
- [x] Arrow cursor appears over non-interactive areas

---

## Conclusion

### What Was Accomplished

Cursor feedback for ts3 webviews is complete:

1. **CEF callback integration** — Added `on_cursor_change` to the DisplayHandler
   in termsurf-profile. CEF calls this whenever the cursor should change based
   on what element is under the mouse.

2. **XPC cursor messaging** — The profile server sends cursor type to the GUI
   via XPC `cursor_change` action. The GUI stores cursor type per pane in a new
   `webview_cursors` HashMap.

3. **Cursor type mapping** — Added `cef_cursor_to_mouse_cursor()` helper that
   maps CEF's cursor types to WezTerm's `MouseCursor` enum:
   - CT_POINTER (0) → Arrow
   - CT_HAND (2) → Hand
   - CT_IBEAM (3) → Text
   - CT_EASTRESIZE/WESTRESIZE (6/8) → SizeLeftRight
   - CT_NORTHRESIZE/SOUTHRESIZE (7/9) → SizeUpDown

4. **Handler refactoring** — Changed `handle_webview_mouse_event` to return
   `Option<MouseCursor>` instead of `bool`, allowing the caller to apply the
   cursor via `context.set_cursor()`.

### What We Learned

1. **DisplayHandler owns cursor changes** — In CEF, cursor changes come through
   `on_cursor_change` in the DisplayHandler, not the RenderHandler. The
   DisplayHandler was already set up for URL changes, making it easy to extend.

2. **Return type refactoring worked well** — Changing the handler to return
   `Option<MouseCursor>` was cleaner than storing cursor state on TermWindow.
   The caller has access to `context` and can apply the cursor immediately.

3. **Immediate feedback** — The XPC messaging and invalidation callback ensure
   cursor changes appear immediately as the user moves the mouse. No perceptible
   delay.

### Implementation Summary

```
Cursor Feedback Flow:

CEF detects hover element change
    │
    ▼
on_cursor_change(type=CT_HAND)
    │
    ▼
XPC: { action: "cursor_change", cursor_type: 2 }
    │
    ▼
GUI stores cursor in webview_cursors[pane_id]
    │
    ▼
Mouse move event over webview
    │
    ▼
handle_webview_mouse_event() returns Some(Hand)
    │
    ▼
context.set_cursor(Some(MouseCursor::Hand))
```

### Files Modified

| File                           | Changes                                                                |
| ------------------------------ | ---------------------------------------------------------------------- |
| `termsurf-profile/src/main.rs` | Added `on_cursor_change` to DisplayHandler                             |
| `webview_xpc.rs`               | Added `webview_cursors` field, `get_cursor()`, `cursor_change` handler |
| `mouseevent.rs`                | Added `cef_cursor_to_mouse_cursor()`, changed handler return type      |

### What's Next

With cursor feedback complete, the core mouse input features are done:

| Feature             | Status   | Issue |
| ------------------- | -------- | ----- |
| Mouse move/click    | Complete | 319   |
| Double/triple-click | Complete | 320   |
| Scroll              | Complete | 321   |
| Drag selection      | Complete | 322   |
| Shift-click extend  | Complete | 323   |
| Cursor feedback     | Complete | 324   |

Remaining mouse-related features:

| Feature      | Priority | Notes                                                    |
| ------------ | -------- | -------------------------------------------------------- |
| Cmd-click    | Medium   | Open links (modifiers passed, needs link URL extraction) |
| Right-click  | Medium   | Context menus (currently suppressed)                     |
| Middle-click | Low      | Paste or open in new tab                                 |

Recommended next focus: Move beyond mouse input to other webview features like
navigation controls, tab management, or profile switching.

---

## References

- `docs/issues/0323-shift-click.md` — Shift-click (completed)
- CEF `cef_cursor_type_t` enum in cef-rs bindings
- WezTerm `MouseCursor` enum in `window` crate
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC communication
- `ts3/termsurf-profile/src/main.rs` — CEF render handler
