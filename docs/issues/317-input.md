# Issue 317: Input Forwarding to Webview

## Goal

Forward keyboard and mouse input from the terminal to the webview so users can
interact with web pages — click links, scroll, type in forms, and use standard
browser navigation.

## Background

### Current Behavior

When a webview pane is visible in Browse mode:

- The webview renders correctly
- All keyboard input is intercepted and consumed (315-mode)
- No input reaches the browser
- Users cannot interact with the page

The webview is effectively display-only. This is the biggest gap in
functionality.

### Desired Behavior

In Browse mode:

- Keyboard input is forwarded to CEF
- Mouse clicks are forwarded to CEF
- Mouse movement is forwarded to CEF (for hover effects)
- Scroll events are forwarded to CEF
- Users can interact with web pages naturally

### Reference Implementation (ts2)

ts2 implements input forwarding in `ts2/wezterm-gui/src/cef_browser/mod.rs`. Key
patterns:

1. **Keyboard events** — Converted to CEF `KeyEvent` struct with:

   - `key_type`: Press, Release, or Char
   - `modifiers`: Shift, Ctrl, Alt, Meta
   - `windows_key_code`: Virtual key code
   - `native_key_code`: Platform-specific code
   - `character`: Unicode character (for Char events)

2. **Mouse events** — Converted to CEF mouse API with:

   - `x`, `y`: Logical coordinates relative to browser view
   - `button`: Left, Middle, Right
   - `modifiers`: Shift, Ctrl, Alt
   - `click_count`: 1 for click, 2 for double-click

3. **Scroll events** — Converted to CEF scroll with:
   - `x`, `y`: Cursor position
   - `delta_x`, `delta_y`: Scroll amount

### XPC Command Infrastructure

Issue 308 established bidirectional XPC for resize commands:

```rust
// GUI sends command to profile
xpc_manager.send_resize(pane_id, width, height);

// Profile receives via event handler
set_event_handler(&*gui, move |event| {
    let action = msg.get_string("action");
    match action.as_str() {
        "resize_browser" => { ... }
    }
});
```

Input forwarding uses the same pattern with new action types.

## Product Requirements

### Keyboard Input

**In Browse mode**, all keyboard input (except mode-switching keys) should be
forwarded to the browser:

| Input                      | Action                                 |
| -------------------------- | -------------------------------------- |
| Regular keys (a-z, 0-9)    | Forward to browser                     |
| Arrow keys                 | Forward to browser (navigation)        |
| Enter                      | Forward to browser (form submit)       |
| Tab                        | Forward to browser (focus next)        |
| Backspace, Delete          | Forward to browser                     |
| Ctrl+C (in text field)     | Forward to browser (copy)              |
| Ctrl+V                     | Forward to browser (paste)             |
| Ctrl+A                     | Forward to browser (select all)        |
| Ctrl+C (not in text field) | Switch to Control mode (315 rule)      |
| WezTerm keybindings        | Execute WezTerm action (not forwarded) |

**Key insight:** Ctrl+C behavior depends on context. If the browser has a text
selection or is in a text field, Ctrl+C should copy. Otherwise, it switches to
Control mode. For Phase 1, we can simplify: always switch to Control mode on
Ctrl+C. Phase 2 can add context-aware behavior.

### Mouse Input

**In Browse mode**, all mouse input within the webview bounds should be
forwarded:

| Input        | Action                              |
| ------------ | ----------------------------------- |
| Left click   | Forward to browser                  |
| Right click  | Forward to browser (context menu)   |
| Middle click | Forward to browser                  |
| Mouse move   | Forward to browser (hover)          |
| Scroll       | Forward to browser                  |
| Drag         | Forward to browser (text selection) |

**Coordinate translation:** Mouse coordinates from WezTerm are in physical
pixels relative to the window. CEF expects logical pixels relative to the
browser view. Translation required:

```
cef_x = (window_x - pane_left) / scale_factor
cef_y = (window_y - pane_top - control_bar_height) / scale_factor
```

### Control Mode

In Control mode, input is NOT forwarded. This is already implemented in 315:

- Keys are consumed or passed to WezTerm keybindings
- Enter returns to Browse mode
- Ctrl+C exits the browser

### Focus Indication

Users need visual feedback about which mode is active:

- **Browse mode**: Webview at full brightness (current)
- **Control mode**: Webview dimmed (implemented in 316)

No additional visual changes needed for input forwarding.

## Technical Approach

### Phase 1: Keyboard Forwarding

**Goal:** Forward keyboard input from Browse mode to CEF.

#### 1. Modify key interception (keyevent.rs)

Currently `handle_webview_key_event` consumes all keys in Browse mode:

```rust
WebviewMode::Browse => {
    if is_ctrl_c { /* switch to Control mode */ }
    // In Browse mode, consume all keys (future: forward to CEF)
    Some(true)
}
```

Change to forward keys:

```rust
WebviewMode::Browse => {
    if is_ctrl_c {
        overlay.mode = WebviewMode::Control;
        return Some(true);
    }

    // Forward to CEF via XPC
    if window_key.key_is_down {
        if let Some(xpc_manager) = get_xpc_manager() {
            xpc_manager.send_key_event(pane_id, window_key);
        }
    }
    Some(true) // Still consume (don't send to terminal)
}
```

#### 2. Add send_key_event to XpcManager (webview_xpc.rs)

```rust
pub fn send_key_event(&self, pane_id: PaneId, key: &KeyEvent) {
    let msg = XpcDictionary::new();
    msg.set_string("action", "key_event");
    msg.set_bool("key_is_down", key.key_is_down);

    // Serialize key code
    let key_code = match &key.key {
        KeyCode::Char(c) => format!("char:{}", c),
        KeyCode::Function(n) => format!("f:{}", n),
        KeyCode::LeftArrow => "left".to_string(),
        KeyCode::RightArrow => "right".to_string(),
        KeyCode::UpArrow => "up".to_string(),
        KeyCode::DownArrow => "down".to_string(),
        KeyCode::Enter => "enter".to_string(),
        KeyCode::Backspace => "backspace".to_string(),
        KeyCode::Tab => "tab".to_string(),
        KeyCode::Escape => "escape".to_string(),
        // ... other keys
        _ => format!("raw:{:?}", key.key),
    };
    msg.set_string("key_code", &key_code);

    // Serialize modifiers
    let mods = key.modifiers;
    msg.set_bool("shift", mods.contains(Modifiers::SHIFT));
    msg.set_bool("ctrl", mods.contains(Modifiers::CTRL));
    msg.set_bool("alt", mods.contains(Modifiers::ALT));
    msg.set_bool("meta", mods.contains(Modifiers::SUPER));

    self.send_command(pane_id, &msg);
}
```

#### 3. Handle key_event in profile server (main.rs)

```rust
"key_event" => {
    let key_is_down = msg.get_bool("key_is_down");
    let key_code = msg.get_string("key_code").unwrap_or_default();
    let shift = msg.get_bool("shift");
    let ctrl = msg.get_bool("ctrl");
    let alt = msg.get_bool("alt");
    let meta = msg.get_bool("meta");

    // Convert to CEF KeyEvent
    let cef_event = convert_to_cef_key_event(
        key_is_down, &key_code, shift, ctrl, alt, meta
    );

    // Send to browser on UI thread
    let bs = Arc::clone(browser_state);
    cef::post_task(cef::ThreadId::UI, move || {
        if let Some(browser) = bs.browser.lock().unwrap().as_ref() {
            if let Some(host) = browser.host() {
                host.send_key_event(&cef_event);
            }
        }
    });
}
```

#### 4. CEF key event conversion

Reference ts2 for the conversion logic. Key points:

- CEF uses Windows virtual key codes even on macOS
- Need to handle both key down/up and character events
- Character events are separate from key events in CEF

### Phase 2: Mouse Forwarding

**Goal:** Forward mouse clicks and movement to CEF.

#### 1. Intercept mouse events (mod.rs or new file)

WezTerm handles mouse events in `termwindow/mod.rs`. Add webview check:

```rust
fn mouse_event(&mut self, event: MouseEvent, window: &Window) {
    // Check if mouse is over a webview pane
    if let Some((pane_id, local_x, local_y)) = self.webview_hit_test(&event) {
        self.forward_mouse_to_webview(pane_id, local_x, local_y, &event);
        return; // Don't process as terminal event
    }

    // ... existing terminal mouse handling
}
```

#### 2. Hit test for webview panes

```rust
fn webview_hit_test(&self, event: &MouseEvent) -> Option<(PaneId, f32, f32)> {
    let server = get_server()?;
    let overlays = server.state().read().unwrap();

    for (pane_id, _) in overlays.overlays.iter() {
        // Get pane bounds
        let pos = self.get_panes_to_render()
            .iter()
            .find(|p| p.pane.pane_id() == *pane_id)?;

        // Check if mouse is within pane bounds
        let pane_x = pos.left as f32 * self.render_metrics.cell_size.width as f32;
        let pane_y = pos.top as f32 * self.render_metrics.cell_size.height as f32;
        let pane_w = pos.pixel_width as f32;
        let pane_h = pos.pixel_height as f32;

        if event.x >= pane_x && event.x < pane_x + pane_w
            && event.y >= pane_y && event.y < pane_y + pane_h
        {
            // Translate to local coordinates
            let local_x = (event.x - pane_x) / scale;
            let local_y = (event.y - pane_y - control_bar_height) / scale;
            return Some((*pane_id, local_x, local_y));
        }
    }
    None
}
```

#### 3. Add send_mouse_event to XpcManager

```rust
pub fn send_mouse_event(
    &self,
    pane_id: PaneId,
    x: f32,
    y: f32,
    event_type: &str, // "move", "down", "up", "scroll"
    button: Option<MouseButton>,
    scroll_delta: Option<(f32, f32)>,
    modifiers: Modifiers,
) {
    let msg = XpcDictionary::new();
    msg.set_string("action", "mouse_event");
    msg.set_string("event_type", event_type);
    // XPC has no set_f32, use string encoding
    msg.set_string("x", &x.to_string());
    msg.set_string("y", &y.to_string());

    if let Some(btn) = button {
        msg.set_string("button", match btn {
            MouseButton::Left => "left",
            MouseButton::Right => "right",
            MouseButton::Middle => "middle",
            _ => "none",
        });
    }

    if let Some((dx, dy)) = scroll_delta {
        msg.set_string("scroll_x", &dx.to_string());
        msg.set_string("scroll_y", &dy.to_string());
    }

    msg.set_bool("shift", modifiers.contains(Modifiers::SHIFT));
    msg.set_bool("ctrl", modifiers.contains(Modifiers::CTRL));
    msg.set_bool("alt", modifiers.contains(Modifiers::ALT));

    self.send_command(pane_id, &msg);
}
```

#### 4. Handle mouse_event in profile server

```rust
"mouse_event" => {
    let event_type = msg.get_string("event_type").unwrap_or_default();
    let x: f32 = msg.get_string("x").unwrap_or_default().parse().unwrap_or(0.0);
    let y: f32 = msg.get_string("y").unwrap_or_default().parse().unwrap_or(0.0);

    match event_type.as_str() {
        "move" => {
            host.send_mouse_move_event(x as i32, y as i32, false);
        }
        "down" => {
            let button = parse_button(&msg);
            host.send_mouse_click_event(x as i32, y as i32, button, false, 1);
        }
        "up" => {
            let button = parse_button(&msg);
            host.send_mouse_click_event(x as i32, y as i32, button, true, 1);
        }
        "scroll" => {
            let dx: f32 = msg.get_string("scroll_x")...;
            let dy: f32 = msg.get_string("scroll_y")...;
            host.send_mouse_wheel_event(x as i32, y as i32, dx as i32, dy as i32);
        }
        _ => {}
    }
}
```

## Files to Modify

| File                                            | Changes                                    |
| ----------------------------------------------- | ------------------------------------------ |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`    | Forward keys in Browse mode                |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add send_key_event, send_mouse_event       |
| `ts3/wezterm-gui/src/termwindow/mod.rs`         | Intercept mouse events for webview panes   |
| `ts3/termsurf-profile/src/main.rs`              | Handle key_event, mouse_event XPC commands |

## Success Criteria

### Phase 1: Keyboard

- [ ] Keys forwarded to browser in Browse mode
- [ ] Can type in Google search box
- [ ] Can press Enter to submit form
- [ ] Arrow keys navigate (scroll page, move cursor)
- [ ] Tab key moves focus between elements
- [ ] Ctrl+C still switches to Control mode
- [ ] WezTerm keybindings still work

### Phase 2: Mouse

- [ ] Click on links navigates to new page
- [ ] Click in text field focuses it
- [ ] Mouse hover shows hover effects (underlines, tooltips)
- [ ] Scroll wheel scrolls the page
- [ ] Right-click shows context menu
- [ ] Drag to select text works
- [ ] Double-click selects word

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Keyboard input
web google.com
# Type "hello" in search box
# Expected: Text appears in search box

# Test 2: Form submission
# Press Enter
# Expected: Google search results appear

# Test 3: Arrow key navigation
# Press Tab to focus a link, Enter to click
# Expected: Navigation works

# Test 4: Mode switching still works
# Press Ctrl+C
# Expected: Switch to Control mode, webview dims

# Test 5: Mouse click (Phase 2)
# Click on a link
# Expected: Navigates to new page

# Test 6: Scroll (Phase 2)
# Scroll with mouse wheel
# Expected: Page scrolls smoothly
```

## References

- `docs/issues/315-mode.md` — Key interception implementation
- `ts2/wezterm-gui/src/cef_browser/mod.rs` — ts2 input handling
- `ts2/wezterm-gui/src/termwindow/keyevent.rs` — ts2 key forwarding
- CEF documentation on `send_key_event`, `send_mouse_click_event`

---

## Experiments

### Experiment 1: Basic Keyboard Forwarding

**Goal:** Forward keyboard input from Browse mode to CEF so users can type in
text fields and navigate with the keyboard.

#### Background

ts2 handles keyboard forwarding in `keyevent.rs:720-815`. Key insights:

1. **CEF uses Windows virtual key codes** — Even on macOS, CEF expects Windows
   VK codes. Requires a conversion table from macOS native keycodes.

2. **Three event types** — KEYDOWN, KEYUP, and CHAR. For printable characters,
   both KEYDOWN and CHAR must be sent on key press.

3. **Skip KEYUP for action keys** — Keys like Tab, Enter, arrows trigger actions
   on both down and up. Sending both causes double actions. Only send KEYDOWN.

4. **Modifiers tracked separately** — Shift, Ctrl, Alt, Meta are bitflags.

The existing XPC infrastructure from resize (issue 308) provides the command
pathway. We add a new `key_event` action.

#### Approach

**Part A: Add send_key_event to XpcManager (webview_xpc.rs)**

```rust
/// Send a key event to the browser in the given pane
pub fn send_key_event(&self, pane_id: PaneId, key: &KeyEvent) {
    let msg = XpcDictionary::new();
    msg.set_string("action", "key_event");
    msg.set_bool("key_is_down", key.key_is_down);

    // Include raw keycode if available (for accurate VK conversion)
    if let Some(raw) = &key.raw {
        msg.set_i64("raw_code", raw.raw_code as i64);
    }

    // Serialize the key for character extraction
    match &key.key {
        KeyCode::Char(c) => {
            msg.set_string("key_type", "char");
            msg.set_i64("char_code", *c as i64);
        }
        KeyCode::LeftArrow => msg.set_string("key_type", "left"),
        KeyCode::RightArrow => msg.set_string("key_type", "right"),
        KeyCode::UpArrow => msg.set_string("key_type", "up"),
        KeyCode::DownArrow => msg.set_string("key_type", "down"),
        KeyCode::Home => msg.set_string("key_type", "home"),
        KeyCode::End => msg.set_string("key_type", "end"),
        KeyCode::PageUp => msg.set_string("key_type", "pageup"),
        KeyCode::PageDown => msg.set_string("key_type", "pagedown"),
        KeyCode::Insert => msg.set_string("key_type", "insert"),
        KeyCode::Function(n) => {
            msg.set_string("key_type", "function");
            msg.set_i64("function_num", *n as i64);
        }
        _ => msg.set_string("key_type", "unknown"),
    }

    // Serialize modifiers
    let mods = key.modifiers;
    msg.set_bool("shift", mods.contains(Modifiers::SHIFT));
    msg.set_bool("ctrl", mods.contains(Modifiers::CTRL));
    msg.set_bool("alt", mods.contains(Modifiers::ALT));
    msg.set_bool("meta", mods.contains(Modifiers::SUPER));

    if self.send_command(pane_id, &msg) {
        log::debug!(
            "[XPC] Sent key_event to pane {}: {:?} down={}",
            pane_id, key.key, key.key_is_down
        );
    }
}
```

**Part B: Forward keys in handle_webview_key_event (keyevent.rs)**

Modify the Browse mode handling to forward keys instead of just consuming them:

```rust
WebviewMode::Browse => {
    if is_ctrl_c {
        log::info!("[Webview] Ctrl+C in Browse mode → Control mode");
        overlay.mode = WebviewMode::Control;
        drop(overlays);
        if let Some(ref w) = self.window {
            w.invalidate();
        }
        return Some(true);
    }

    // Forward key to browser via XPC
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_key_event(pane_id, window_key);
    }

    // Consume the key (don't send to terminal)
    Some(true)
}
```

**Part C: Handle key_event in profile server (main.rs)**

Add handler in the XPC event handler (after resize_browser):

```rust
"key_event" => {
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: key_event ignored (state not ready)");
        return;
    };

    let key_is_down = msg.get_bool("key_is_down");
    let key_type = msg.get_string("key_type").unwrap_or_default();
    let raw_code = msg.get_i64("raw_code") as u32;
    let char_code = msg.get_i64("char_code") as u32;

    let shift = msg.get_bool("shift");
    let ctrl = msg.get_bool("ctrl");
    let alt = msg.get_bool("alt");
    let meta = msg.get_bool("meta");

    let bs = Arc::clone(bs);
    drop(state_guard);

    // Post to CEF UI thread
    cef::post_task(cef::ThreadId::UI, move || {
        send_key_event_to_cef(
            &bs, key_is_down, &key_type, raw_code, char_code,
            shift, ctrl, alt, meta
        );
    });
}
```

**Part D: CEF key event sending (main.rs)**

Add the key event conversion and sending:

```rust
fn send_key_event_to_cef(
    state: &BrowserState,
    key_is_down: bool,
    key_type: &str,
    raw_code: u32,
    char_code: u32,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) {
    use cef::{KeyEvent, KeyEventType};

    let browser = match state.browser.lock().unwrap().as_ref() {
        Some(b) => b.clone(),
        None => return,
    };
    let host = match browser.host() {
        Some(h) => h,
        None => return,
    };

    // Build CEF modifiers
    let mut modifiers = 0u32;
    if shift { modifiers |= cef::EVENTFLAG_SHIFT_DOWN; }
    if ctrl { modifiers |= cef::EVENTFLAG_CONTROL_DOWN; }
    if alt { modifiers |= cef::EVENTFLAG_ALT_DOWN; }
    if meta { modifiers |= cef::EVENTFLAG_COMMAND_DOWN; }

    // Convert to Windows VK code
    let windows_vk = macos_keycode_to_windows_vk(raw_code);
    let native_code = raw_code as i32;

    // Determine if this is an action key (skip KEYUP to avoid double actions)
    let is_action_key = matches!(
        key_type,
        "left" | "right" | "up" | "down" | "home" | "end" |
        "pageup" | "pagedown" | "insert"
    ) || (key_type == "char" && matches!(
        char_code,
        0x08 | 0x7f | 0x09 | 0x1b | 0x0d | 0x20  // BS, DEL, TAB, ESC, ENTER, SPACE
    ));

    if is_action_key && !key_is_down {
        return; // Skip KEYUP for action keys
    }

    // Send KEYDOWN or KEYUP
    let event_type = if key_is_down {
        KeyEventType::KEYDOWN
    } else {
        KeyEventType::KEYUP
    };

    let key_event = KeyEvent {
        size: std::mem::size_of::<KeyEvent>(),
        type_: event_type,
        modifiers,
        windows_key_code: windows_vk,
        native_key_code: native_code,
        is_system_key: 0,
        character: 0,
        unmodified_character: 0,
        focus_on_editable_field: 0,
    };
    host.send_key_event(Some(&key_event));

    // For key-down of printable characters, also send CHAR event
    if key_is_down && key_type == "char" && char_code > 0 && char_code < 0x10000 {
        let char_event = KeyEvent {
            size: std::mem::size_of::<KeyEvent>(),
            type_: KeyEventType::CHAR,
            modifiers,
            windows_key_code: char_code as i32,
            native_key_code: 0,
            is_system_key: 0,
            character: char_code as u16,
            unmodified_character: char_code as u16,
            focus_on_editable_field: 0,
        };
        host.send_key_event(Some(&char_event));
    }

    println!(
        "Profile: key_event {:?} vk={} native={} char={}",
        event_type, windows_vk, native_code, char_code
    );
}

/// Convert macOS keycode to Windows virtual key code (from ts2)
fn macos_keycode_to_windows_vk(code: u32) -> i32 {
    match code {
        // Letters
        0x00 => 0x41, // A
        0x0B => 0x42, // B
        0x08 => 0x43, // C
        0x02 => 0x44, // D
        0x0E => 0x45, // E
        0x03 => 0x46, // F
        0x05 => 0x47, // G
        0x04 => 0x48, // H
        0x22 => 0x49, // I
        0x26 => 0x4A, // J
        0x28 => 0x4B, // K
        0x25 => 0x4C, // L
        0x2E => 0x4D, // M
        0x2D => 0x4E, // N
        0x1F => 0x4F, // O
        0x23 => 0x50, // P
        0x0C => 0x51, // Q
        0x0F => 0x52, // R
        0x01 => 0x53, // S
        0x11 => 0x54, // T
        0x20 => 0x55, // U
        0x09 => 0x56, // V
        0x0D => 0x57, // W
        0x07 => 0x58, // X
        0x10 => 0x59, // Y
        0x06 => 0x5A, // Z
        // Numbers
        0x1D => 0x30, // 0
        0x12 => 0x31, // 1
        0x13 => 0x32, // 2
        0x14 => 0x33, // 3
        0x15 => 0x34, // 4
        0x17 => 0x35, // 5
        0x16 => 0x36, // 6
        0x1A => 0x37, // 7
        0x1C => 0x38, // 8
        0x19 => 0x39, // 9
        // Special keys
        0x24 => 0x0D, // Return -> VK_RETURN
        0x30 => 0x09, // Tab -> VK_TAB
        0x31 => 0x20, // Space -> VK_SPACE
        0x33 => 0x08, // Delete (backspace) -> VK_BACK
        0x35 => 0x1B, // Escape -> VK_ESCAPE
        0x75 => 0x2E, // Forward Delete -> VK_DELETE
        // Arrow keys
        0x7B => 0x25, // Left -> VK_LEFT
        0x7C => 0x27, // Right -> VK_RIGHT
        0x7E => 0x26, // Up -> VK_UP
        0x7D => 0x28, // Down -> VK_DOWN
        // Navigation
        0x73 => 0x24, // Home -> VK_HOME
        0x77 => 0x23, // End -> VK_END
        0x74 => 0x21, // PageUp -> VK_PRIOR
        0x79 => 0x22, // PageDown -> VK_NEXT
        // Function keys
        0x7A => 0x70, // F1
        0x78 => 0x71, // F2
        0x63 => 0x72, // F3
        0x76 => 0x73, // F4
        0x60 => 0x74, // F5
        0x61 => 0x75, // F6
        0x62 => 0x76, // F7
        0x64 => 0x77, // F8
        0x65 => 0x78, // F9
        0x6D => 0x79, // F10
        0x67 => 0x7A, // F11
        0x6F => 0x7B, // F12
        // Punctuation (common ones)
        0x27 => 0xBA, // ; -> VK_OEM_1
        0x18 => 0xBB, // = -> VK_OEM_PLUS
        0x2B => 0xBC, // , -> VK_OEM_COMMA
        0x1B => 0xBD, // - -> VK_OEM_MINUS
        0x2F => 0xBE, // . -> VK_OEM_PERIOD
        0x2C => 0xBF, // / -> VK_OEM_2
        0x32 => 0xC0, // ` -> VK_OEM_3
        0x21 => 0xDB, // [ -> VK_OEM_4
        0x2A => 0xDC, // \ -> VK_OEM_5
        0x1E => 0xDD, // ] -> VK_OEM_6
        0x27 => 0xDE, // ' -> VK_OEM_7
        _ => 0,
    }
}
```

#### Files to Modify

| File                                            | Changes                                      |
| ----------------------------------------------- | -------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add `send_key_event()` method                |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`    | Forward keys in Browse mode                  |
| `ts3/termsurf-profile/src/main.rs`              | Handle `key_event`, CEF key conversion       |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Type in search box
web google.com
# Click in search box (not yet implemented - use Tab to focus)
# Type "hello"
# Expected: Letters appear in search box

# Test 2: Arrow keys
# Press Tab several times to navigate
# Press Enter to activate focused link
# Expected: Navigation works

# Test 3: Backspace
# Type "helloo", then Backspace
# Expected: Last character deleted

# Test 4: Mode switching still works
# Press Ctrl+C
# Expected: Switch to Control mode

# Test 5: WezTerm keybindings
# Press Ctrl+Shift+T
# Expected: New tab opens (not sent to browser)

# Check logs
cat /tmp/termsurf-gui.log | grep "key_event"
cat /tmp/termsurf-profile-*.log | grep "key_event"
```

#### Success Criteria

- [x] `send_key_event` sends XPC messages with key data
- [x] Profile server receives and processes `key_event` action
- [x] CEF `send_key_event` is called with correct KeyEvent
- [x] Printable characters appear when typing in text fields
- [x] Backspace deletes characters
- [x] Tab moves focus between elements
- [x] Enter submits forms / activates links
- [x] Arrow keys work for navigation
- [x] Ctrl+C still switches to Control mode
- [x] WezTerm keybindings still work (not forwarded)

#### Known Limitations (Phase 1)

1. **Focus** — Without mouse click support, the only way to focus a text field
   is Tab navigation. Phase 2 (mouse) will fix this.

2. **Ctrl+C ambiguity** — Always switches to Control mode, even in text fields
   where it should copy. Acceptable for Phase 1.

3. **Complex input** — IME (Chinese, Japanese input) not supported. Standard
   keyboard input only.

#### Result: SUCCESS

Keyboard input forwarding works. Users can type in text fields and navigate with
keyboard.

#### Conclusion

**What worked:**

- Typing printable characters in focused text fields (Google search box)
- Shift+Left/Right for text selection
- Tab for focus navigation
- Enter for form submission
- Backspace for deletion
- Ctrl+C to switch to Control mode

**Issues discovered:**

1. **Copy/paste does not work in the browser**

   When pressing Cmd+V (paste) in Browse mode, the paste action does NOT go to
   the browser. Instead, the pasted text appears in the terminal behind the
   webview. This reveals a fundamental issue: **WezTerm keybindings are being
   processed before keys are forwarded to the browser.**

   **Root cause:** WezTerm has TWO key event handlers:

   1. `raw_key_event_impl` — handles `RawKeyEvent`, processes keybindings FIRST
   2. `key_event_impl` — handles `KeyEvent`, where our webview interception lives

   The actual flow is:
   ```
   RawKeyEvent → raw_key_event_impl → process_key (keybindings execute here!)
   KeyEvent → key_event_impl → handle_webview_key_event → forwards to CEF
   ```

   Cmd+V is matched as a keybinding in `raw_key_event_impl` and pastes to the
   terminal BEFORE `key_event_impl` even runs. Our webview handler only
   intercepts the second event, by which time the damage is done.

2. **Cmd+C also affected**

   Similarly, Cmd+C (copy) likely goes to WezTerm instead of the browser. This
   is separate from Ctrl+C which correctly switches to Control mode.

**Fix required:**

Add webview interception to `raw_key_event_impl` that:
1. Checks if pane has webview in Browse mode
2. Calls `key.set_handled()` to prevent keybinding processing
3. Does NOT forward to CEF (that happens in `key_event_impl`)

This will be addressed in Experiment 2.

---

### Experiment 2: Block Keybindings in Browse Mode

**Goal:** Prevent WezTerm keybindings (like Cmd+V paste) from executing when a
webview pane is active in Browse mode. Keys should go to the browser, not the
terminal.

#### Background

Experiment 1 discovered that WezTerm processes keybindings in `raw_key_event_impl`
BEFORE our webview handler in `key_event_impl` runs. This causes Cmd+V to paste
into the terminal instead of the browser.

The fix is to add an early check in `raw_key_event_impl` that marks the key as
handled when a webview is active in Browse mode.

#### Approach

**Add webview check to raw_key_event_impl (keyevent.rs)**

Insert at the beginning of `raw_key_event_impl`, right after getting the pane:

```rust
pub fn raw_key_event_impl(&mut self, key: RawKeyEvent, context: &dyn WindowOps) {
    // ... leader key handling ...

    let pane = match self.get_active_pane_or_overlay() {
        Some(pane) => pane,
        None => return,
    };

    // Block keybindings when webview is active in Browse mode
    #[cfg(target_os = "macos")]
    {
        use crate::termwindow::webview_socket::{get_server, WebviewMode};

        let pane_id = pane.pane_id();
        if let Some(server) = get_server() {
            let state = server.state();
            let overlays = state.read().unwrap();
            if let Some(overlay) = overlays.overlays.get(&pane_id) {
                if overlay.mode == WebviewMode::Browse {
                    // In Browse mode: block all keybindings except explicit allowlist
                    // Keys will be forwarded to CEF via key_event_impl
                    if key.key_is_down {
                        log::debug!(
                            "[Webview] Blocking keybinding in Browse mode: {:?}",
                            key.key
                        );
                    }
                    key.set_handled();
                    return;
                }
            }
        }
    }

    // ... rest of raw_key_event_impl ...
}
```

#### Allowlist Consideration

Some keybindings should still work even in Browse mode:

| Keybinding     | Action                    | Allow in Browse? |
| -------------- | ------------------------- | ---------------- |
| Ctrl+Shift+T   | New tab                   | Yes              |
| Ctrl+Shift+W   | Close tab                 | Yes              |
| Ctrl+Shift+N   | New window                | Yes              |
| Cmd+Q          | Quit app                  | Yes              |
| Cmd+,          | Open preferences          | Yes              |
| Cmd+V          | Paste                     | **No** (→ browser) |
| Cmd+C          | Copy                      | **No** (→ browser) |
| Cmd+A          | Select all                | **No** (→ browser) |

For Phase 1, we'll block ALL keybindings in Browse mode. This is the simplest
approach and matches user expectations (Browse mode = browser has focus). Users
can press Ctrl+C to enter Control mode if they need WezTerm keybindings.

If specific keybindings are needed in Browse mode, we can add an allowlist in a
future experiment.

#### Files to Modify

| File                                         | Changes                                  |
| -------------------------------------------- | ---------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs` | Add Browse mode check to `raw_key_event_impl` |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Cmd+V in Browse mode
web google.com
# Type "hello" in search box
# Copy text from elsewhere (e.g., another app)
# Press Cmd+V in the browser
# Expected: Text pastes into browser search box, NOT terminal

# Test 2: Cmd+C in Browse mode
# Select text in browser with Shift+Arrow keys
# Press Cmd+C
# Expected: Text copied from browser (test by pasting elsewhere)

# Test 3: Ctrl+C still works
# Press Ctrl+C
# Expected: Switch to Control mode (webview dims)

# Test 4: Control mode keybindings work
# In Control mode, press Ctrl+Shift+T
# Expected: New tab opens

# Test 5: Return to Browse mode
# Press Enter
# Expected: Return to Browse mode, keybindings blocked again
```

#### Success Criteria

- [x] `raw_key_event_impl` checks for webview Browse mode
- [x] Keybindings are blocked in Browse mode (`key.set_handled()`)
- [ ] Cmd+V pastes into browser, not terminal
- [ ] Cmd+C copies from browser
- [ ] Ctrl+C still switches to Control mode
- [ ] Control mode keybindings still work
- [ ] Browse mode keybindings blocked (all of them)

#### Result: FAILURE

Keyboard completely broken. Ctrl+C stopped working. User had to force-quit the
application.

#### Conclusion

**What went wrong:**

Calling `key.set_handled()` on the `RawKeyEvent` signals to the underlying
windowing system that the key was consumed. This prevents the corresponding
`KeyEvent` from ever being generated or dispatched.

The result:
1. `raw_key_event_impl` marks ALL keys as handled in Browse mode
2. `KeyEvent` is never dispatched to `key_event_impl`
3. Our webview handler in `key_event_impl` never runs
4. Ctrl+C handling never runs (it's in `key_event_impl`)
5. CEF key forwarding never runs (also in `key_event_impl`)
6. Keyboard completely dead, no way to exit Browse mode

**Hypothesis:**

The `RawKeyEvent` and `KeyEvent` are not independent events. The `RawKeyEvent`
is the low-level event from the OS, and calling `set_handled()` on it tells the
windowing system "I consumed this, don't generate the higher-level KeyEvent."

**Fix for Experiment 3:**

Instead of calling `key.set_handled()`, we should:

1. Return early from `raw_key_event_impl` WITHOUT calling `set_handled()`
2. This skips keybinding processing in `raw_key_event_impl`
3. But still allows `KeyEvent` to be generated and dispatched
4. `key_event_impl` then handles webview forwarding and Ctrl+C as before

The key insight is: we want to skip the keybinding processing, not consume the
key entirely.

---

### Experiment 3: Skip Keybindings Without Consuming Key

**Goal:** Same as experiment 2 (prevent WezTerm keybindings in Browse mode), but
without breaking keyboard input entirely.

#### Background

Experiment 2 failed because `key.set_handled()` on `RawKeyEvent` prevents the
corresponding `KeyEvent` from being dispatched. This broke all keyboard input.

The fix is simple: return early WITHOUT calling `set_handled()`. This:
- Skips keybinding processing in `raw_key_event_impl`
- Allows `KeyEvent` to still be generated and dispatched
- `key_event_impl` handles webview forwarding and Ctrl+C as normal

#### Approach

**Modify raw_key_event_impl (keyevent.rs)**

Same location as experiment 2, but WITHOUT `set_handled()`:

```rust
let pane = match self.get_active_pane_or_overlay() {
    Some(pane) => pane,
    None => return,
};

// Skip keybinding processing when webview is active in Browse mode
// Do NOT call set_handled() - we want KeyEvent to still be dispatched
// so key_event_impl can forward to CEF and handle Ctrl+C
#[cfg(target_os = "macos")]
{
    use crate::termwindow::webview_socket::{get_server, WebviewMode};

    let pane_id = pane.pane_id();
    if let Some(server) = get_server() {
        let state = server.state();
        let overlays = state.read().unwrap();
        if let Some(overlay) = overlays.overlays.get(&pane_id) {
            if overlay.mode == WebviewMode::Browse {
                // In Browse mode: skip keybinding processing
                // but let KeyEvent flow through to key_event_impl
                if key.key_is_down {
                    log::debug!(
                        "[Webview] Skipping keybindings in Browse mode: {:?}",
                        key.key
                    );
                }
                return; // Early return, NO set_handled()
            }
        }
    }
}

// First, try to match raw physical key
// ... rest of function
```

The only difference from experiment 2: `return` instead of `key.set_handled(); return`.

#### Files to Modify

| File                                         | Changes                                  |
| -------------------------------------------- | ---------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs` | Add Browse mode check, return without set_handled |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Basic keyboard still works
web google.com
# Type "hello" in search box
# Expected: Text appears (same as experiment 1)

# Test 2: Ctrl+C still works
# Press Ctrl+C
# Expected: Switch to Control mode (CRITICAL - was broken in exp 2)

# Test 3: Cmd+V in Browse mode
# Copy text from another app
# Press Cmd+V in browser
# Expected: Pastes into browser, NOT terminal

# Test 4: Cmd+C in Browse mode
# Select text with Shift+arrows
# Press Cmd+C
# Expected: Copies from browser

# Test 5: Control mode keybindings still work
# In Control mode, press Ctrl+Shift+T
# Expected: New tab opens
```

#### Success Criteria

- [x] Keyboard input works (typing in text fields)
- [x] Ctrl+C switches to Control mode (CRITICAL)
- [x] Keybindings skipped in Browse mode (no terminal paste)
- [ ] Cmd+V pastes into browser
- [ ] Cmd+C copies from browser
- [x] Control mode keybindings still work

#### Result: PARTIAL SUCCESS

The primary goal was achieved: keybindings are now skipped in Browse mode, so
Cmd+V no longer pastes into the terminal. Ctrl+C works correctly (fixed from
experiment 2's complete failure).

However, copy/paste does not work IN the browser — Cmd+V doesn't paste, Cmd+C
doesn't copy.

#### Conclusion

**What worked:**

- Keyboard input (typing, arrows, Tab, Enter, Backspace)
- Ctrl+C switches to Control mode
- WezTerm keybindings are skipped (no accidental terminal paste)
- Control mode keybindings still work

**What didn't work:**

- Cmd+V does not paste into browser text fields
- Cmd+C does not copy selected text from browser

**Hypothesis:**

The keys ARE being forwarded to CEF (we can verify via logs), but copy/paste
isn't working. Possible reasons:

1. **macOS clipboard integration** — CEF's clipboard access may require proper
   NSApplication integration. Since CEF is running in an off-screen/headless
   context in a separate process, it may not have access to the system
   pasteboard.

2. **Focus state** — CEF may need to believe it has system focus to handle
   clipboard operations. Our off-screen browser may not have proper focus state.

3. **Key event format** — macOS system shortcuts may need to go through the
   responder chain rather than being synthesized as key events. CEF might
   receive our Cmd+V key event but not trigger the actual paste action.

4. **Separate process isolation** — The profile server process runs CEF. System
   clipboard access may be restricted or require entitlements we haven't
   configured.

**Next steps:**

This is a deeper CEF integration issue that may require:
- Investigating CEF's clipboard handler APIs
- Checking if off-screen browsers have clipboard limitations
- Possibly implementing explicit clipboard commands via XPC rather than relying
  on key events

---

### Experiment 4: Diagnose Clipboard Behavior

**Goal:** Add diagnostic logging to understand why Cmd+V doesn't paste into the
browser. Before implementing a fix, we need to know if the issue is:

1. Key event not arriving at profile server
2. Key event arriving but with wrong modifiers/VK code
3. CEF receiving the event but not having "focus"
4. CEF off-screen browser lacking clipboard access
5. Something else entirely

#### Diagnostic Points

| Location | What to Log |
|----------|-------------|
| GUI (keyevent.rs) | When Cmd+V is forwarded to XPC |
| GUI (webview_xpc.rs) | The exact XPC message being sent |
| Profile (main.rs) | When key_event action is received |
| Profile (main.rs) | Modifiers being set (especially `meta`) |
| Profile (main.rs) | Windows VK code for the key |
| Profile (main.rs) | CEF focus state (if queryable) |
| Profile (main.rs) | After `send_key_event` call |

#### Approach

**Part A: Enhanced logging in GUI (keyevent.rs)**

Add log when forwarding Cmd+V specifically:

```rust
// In handle_webview_key_event, Browse mode section:
if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
    // Log Cmd+V specifically for debugging
    if window_key.modifiers.contains(Modifiers::SUPER) {
        if let KeyCode::Char(c) = &window_key.key {
            if *c == 'v' || *c == 'V' {
                log::info!(
                    "[CLIPBOARD-DEBUG] Forwarding Cmd+V to pane {}: down={}, raw={:?}",
                    pane_id,
                    window_key.key_is_down,
                    window_key.raw.as_ref().map(|r| r.raw_code)
                );
            }
        }
    }
    xpc_manager.send_key_event(pane_id, window_key);
}
```

**Part B: Enhanced logging in profile server (main.rs)**

Add detailed logging in `send_key_event_to_cef`:

```rust
fn send_key_event_to_cef(
    state: &BrowserState,
    key_is_down: bool,
    key_type: &str,
    raw_code: u32,
    char_code: u32,
    shift: bool,
    ctrl: bool,
    alt: bool,
    meta: bool,
) {
    use cef::{KeyEvent, KeyEventType};

    // Detailed logging for clipboard shortcuts
    let is_potential_paste = meta && (char_code == 'v' as u32 || char_code == 'V' as u32);
    let is_potential_copy = meta && (char_code == 'c' as u32 || char_code == 'C' as u32);

    if is_potential_paste || is_potential_copy {
        println!(
            "[CLIPBOARD-DEBUG] {} received: key_is_down={}, raw_code={}, char_code={}, modifiers=[shift={}, ctrl={}, alt={}, meta={}]",
            if is_potential_paste { "Cmd+V" } else { "Cmd+C" },
            key_is_down,
            raw_code,
            char_code,
            shift, ctrl, alt, meta
        );
    }

    let browser = match state.browser.lock().unwrap().as_ref() {
        Some(b) => b.clone(),
        None => {
            if is_potential_paste || is_potential_copy {
                println!("[CLIPBOARD-DEBUG] ERROR: No browser instance!");
            }
            return;
        }
    };
    let host = match browser.host() {
        Some(h) => h,
        None => {
            if is_potential_paste || is_potential_copy {
                println!("[CLIPBOARD-DEBUG] ERROR: No browser host!");
            }
            return;
        }
    };

    // Build CEF modifiers
    let mut modifiers = 0u32;
    if shift { modifiers |= EVENTFLAG_SHIFT_DOWN; }
    if ctrl { modifiers |= EVENTFLAG_CONTROL_DOWN; }
    if alt { modifiers |= EVENTFLAG_ALT_DOWN; }
    if meta { modifiers |= EVENTFLAG_COMMAND_DOWN; }

    // Convert to Windows VK code
    let windows_vk = macos_keycode_to_windows_vk(raw_code);
    let native_code = raw_code as i32;

    if is_potential_paste || is_potential_copy {
        println!(
            "[CLIPBOARD-DEBUG] CEF event: windows_vk={:#x} (expected V={:#x}), native={}, modifiers={:#x} (COMMAND_DOWN={:#x})",
            windows_vk,
            0x56, // VK_V
            native_code,
            modifiers,
            EVENTFLAG_COMMAND_DOWN
        );
    }

    // ... rest of function, add logging after send_key_event:
    host.send_key_event(Some(&key_event));

    if is_potential_paste || is_potential_copy {
        println!(
            "[CLIPBOARD-DEBUG] send_key_event called for {:?}",
            event_type
        );
    }

    // Also try sending focus event to ensure CEF thinks it has focus
    if is_potential_paste && key_is_down {
        println!("[CLIPBOARD-DEBUG] Calling set_focus(true) before paste");
        host.set_focus(1); // 1 = focused
    }
}
```

**Part C: Check if focus is the issue**

Add a one-time focus event when the browser is first created:

```rust
// In create_browser_on_ui_thread, after browser creation:
match browser {
    Some(b) => {
        let browser_id = b.identifier();
        println!(
            "Profile: Browser {} created for '{}' (session='{}')",
            browser_id, url, session_id
        );

        // Ensure browser has focus for clipboard operations
        if let Some(host) = b.host() {
            println!("[FOCUS-DEBUG] Sending initial focus event to browser");
            host.set_focus(1);
        }

        // Store browser reference...
    }
}
```

#### Files to Modify

| File | Changes |
|------|---------|
| `ts3/wezterm-gui/src/termwindow/keyevent.rs` | Log when Cmd+V/C forwarded |
| `ts3/termsurf-profile/src/main.rs` | Detailed clipboard logging, focus event |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test: Cmd+V paste attempt
web google.com
# Copy text in another app
# Return to TermSurf, press Cmd+V

# Check logs for diagnostic output
cat /tmp/termsurf-gui.log | grep "CLIPBOARD-DEBUG"
cat /tmp/termsurf-profile-*.log | grep -E "(CLIPBOARD-DEBUG|FOCUS-DEBUG)"
```

Expected log output to analyze:
```
# GUI side:
[CLIPBOARD-DEBUG] Forwarding Cmd+V to pane 0: down=true, raw=Some(9)

# Profile side:
[CLIPBOARD-DEBUG] Cmd+V received: key_is_down=true, raw_code=9, char_code=118, modifiers=[shift=false, ctrl=false, alt=false, meta=true]
[CLIPBOARD-DEBUG] CEF event: windows_vk=0x56 (expected V=0x56), native=9, modifiers=0x80 (COMMAND_DOWN=0x80)
[CLIPBOARD-DEBUG] Calling set_focus(true) before paste
[CLIPBOARD-DEBUG] send_key_event called for KEYDOWN
```

#### What We're Looking For

1. **If no GUI log appears**: Key not reaching forwarding code
2. **If no Profile log appears**: XPC message not arriving
3. **If modifiers wrong**: Need to fix modifier handling
4. **If VK code wrong**: Need to fix keycode conversion (macOS 0x09 → Windows 0x56)
5. **If everything looks correct**: Issue is likely focus or CEF off-screen limitation

#### Success Criteria

- [ ] Diagnostic logging added for Cmd+V and Cmd+C
- [ ] Logs show the full event path from GUI to CEF
- [ ] Focus event sent to CEF when clipboard shortcut detected
- [ ] Analysis of logs reveals root cause

#### Result: SUCCESS (Root Cause Found)

The diagnostic logging revealed the root cause of the clipboard issue.

#### Conclusion

**Findings from logs:**

1. GUI log shows NO `[CLIPBOARD-DEBUG]` entries for Cmd+V/C
2. Regular keys (a, s, d, f, arrows) DO reach the profile server
3. Ctrl+C correctly triggers mode switching
4. Profile server only shows `[FOCUS-DEBUG] Sending initial focus event to browser 1`

**Root cause:**

Cmd+C/V/X are **not generating KeyEvents** on macOS. This is standard Cocoa behavior:

1. When you press Cmd+V, macOS interprets it as a "key equivalent"
2. The `performKeyEquivalent:` method on the NSView is called
3. WezTerm's implementation returns `NO` for Cmd+V (see `window/src/os/macos/window.rs:2829`)
4. macOS then routes it to the menu system as a `paste:` action
5. Since there's no `paste:` handler, nothing happens
6. **No KeyEvent is ever generated**

**Why ts2 works but ts3 doesn't:**

| Aspect | ts2 | ts3 |
|--------|-----|-----|
| CEF location | In-process | Out-of-process |
| First responder | CEF browser view | WezTerm GUI window |
| Cmd+V handling | CEF's native Cocoa integration | Lost to menu system |

In ts2, CEF runs in the same process and has native Cocoa clipboard integration.
In ts3, CEF runs in a separate process and never sees Cmd+V.

**The fix:**

Modify `perform_key_equivalent` in `ts3/window/src/os/macos/window.rs` to:
1. Intercept Cmd+C/V/X (and Cmd+A for select all)
2. Synthesize key events via `Self::key_common()` (like it already does for Cmd+.)
3. Return `YES` to prevent macOS from handling it

This will generate KeyEvents for clipboard shortcuts, allowing our webview key handler
to forward them to CEF via XPC.

**Next experiment:** Implement the `perform_key_equivalent` fix.

---

### Experiment 5: Synthesize KeyEvents for Cmd+C/V/X/A

**Goal:** Make Cmd+C/V/X/A generate KeyEvents so they can be forwarded to CEF.

#### Background

Experiment 4 revealed that `performKeyEquivalent:` returns `NO` for Cmd+C/V/X,
causing macOS to route them to the menu system instead of generating KeyEvents.

WezTerm already handles this pattern for other shortcuts. In
`window/src/os/macos/window.rs:2840-2851`:

```rust
if (chars == "." && modifiers == Modifiers::SUPER)
    || (chars == "\u{1b}" && modifiers == Modifiers::CTRL)
    || (chars == "\t" && modifiers == Modifiers::CTRL)
    || (chars == "\x19"/* Shift-Tab */)
{
    // Synthesize a key down event for this
    Self::key_common(this, nsevent, true);
    YES
} else {
    NO
}
```

We extend this to include Cmd+C/V/X/A (clipboard and select-all shortcuts).

#### Approach

**Modify perform_key_equivalent (window/src/os/macos/window.rs)**

Add clipboard shortcuts to the condition:

```rust
extern "C" fn perform_key_equivalent(this: &mut Object, _sel: Sel, nsevent: id) -> BOOL {
    let chars = unsafe { nsstring_to_str(nsevent.characters()) };
    let modifier_flags = unsafe { nsevent.modifierFlags() };
    let modifiers = key_modifiers(modifier_flags);

    log::trace!(
        "perform_key_equivalent: chars=`{}` modifiers=`{:?}`",
        chars.escape_debug(),
        modifiers,
    );

    // Shortcuts that need KeyEvent synthesis (macOS won't generate them otherwise)
    let dominated_by_super = modifiers == Modifiers::SUPER
        || modifiers == (Modifiers::SUPER | Modifiers::SHIFT);

    if (chars == "." && modifiers == Modifiers::SUPER)
        || (chars == "\u{1b}" && modifiers == Modifiers::CTRL)
        || (chars == "\t" && modifiers == Modifiers::CTRL)
        || (chars == "\x19"/* Shift-Tab: See issue #1902 */)
        // Clipboard shortcuts - synthesize KeyEvents so webviews can handle them
        || (chars == "c" && dominated_by_super)  // Cmd+C (copy)
        || (chars == "v" && dominated_by_super)  // Cmd+V (paste)
        || (chars == "x" && dominated_by_super)  // Cmd+X (cut)
        || (chars == "a" && dominated_by_super)  // Cmd+A (select all)
    {
        // Synthesize a key down event for this, because macOS will
        // not do that, even though we tell it that we handled this event.
        Self::key_common(this, nsevent, true);

        // Prevent macOS from handling this as a menu action
        YES
    } else {
        // Allow macOS to process built-in shortcuts like CMD-`
        NO
    }
}
```

**Note on `dominated_by_super`:** This allows both `Cmd+C` and `Cmd+Shift+C` to be
captured. Some apps use Cmd+Shift+C for "copy as..." variants.

#### Impact Analysis

This change affects ALL Cmd+C/V/X/A keypresses in WezTerm, not just webview mode.

**When webview is active (Browse mode):**
- KeyEvent is generated → forwarded to CEF via XPC → browser handles clipboard

**When webview is NOT active (normal terminal):**
- KeyEvent is generated → goes through normal key processing
- WezTerm's keybindings still work (Cmd+V = PasteFrom clipboard)
- No behavior change for terminal users

**Potential concern:** WezTerm already has keybindings for Cmd+C/V. Will this
cause double-handling?

No, because:
1. `perform_key_equivalent` is called BEFORE keybinding processing
2. We synthesize a KeyEvent via `key_common()`
3. The KeyEvent flows through normal processing (`raw_key_event_impl` → `key_event_impl`)
4. Our Browse mode check in `raw_key_event_impl` skips keybindings when appropriate
5. If not in Browse mode, normal keybinding processing handles it

The key insight: we're not changing WHAT happens, just ensuring the KeyEvent EXISTS.

#### Files to Modify

| File | Changes |
|------|---------|
| `ts3/window/src/os/macos/window.rs` | Add Cmd+C/V/X/A to `perform_key_equivalent` |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Cmd+V in Browse mode
web google.com
# Copy text from another app
# Press Cmd+V in the browser search box
# Expected: Text pastes into browser (not terminal)

# Test 2: Cmd+C in Browse mode
# Select text in browser with Shift+arrows
# Press Cmd+C
# Paste in another app
# Expected: Browser text was copied

# Test 3: Cmd+A in Browse mode
# Press Cmd+A
# Expected: All text selected in browser

# Test 4: Cmd+V in terminal (no webview)
# Close browser (Ctrl+C twice)
# Press Cmd+V
# Expected: Normal terminal paste works

# Test 5: Cmd+C in terminal
# Select text in terminal
# Press Cmd+C
# Expected: Normal terminal copy works

# Check logs
cat /tmp/termsurf-gui.log | grep "CLIPBOARD-DEBUG"
cat /tmp/termsurf-profile-*.log | grep "CLIPBOARD-DEBUG"
```

#### Success Criteria

- [ ] `perform_key_equivalent` intercepts Cmd+C/V/X/A
- [ ] KeyEvents are generated for these shortcuts
- [ ] Cmd+V pastes into browser in Browse mode
- [ ] Cmd+C copies from browser in Browse mode
- [ ] Cmd+A selects all in browser
- [ ] Normal terminal Cmd+C/V still works when no webview active
- [ ] No regressions in terminal keybinding behavior

#### Result: PARTIAL SUCCESS (KeyEvents generated, but clipboard still doesn't work)

Experiment 5 successfully made Cmd+C/V/X/A generate KeyEvents. The logs confirm
the full pipeline is working:

```
GUI: [CLIPBOARD-DEBUG] Forwarding Cmd+v to pane 0: down=true, raw=Some(9)

Profile: [CLIPBOARD-DEBUG] Cmd+V received: key_is_down=true, raw_code=0x9, char_code=118 ('v'), modifiers=[...meta=true]
Profile: [CLIPBOARD-DEBUG] CEF event: windows_vk=0x56 (expected V=0x56), modifiers=0x80 (COMMAND_DOWN=0x80)
Profile: [CLIPBOARD-DEBUG] Calling set_focus(true) before clipboard operation
Profile: [CLIPBOARD-DEBUG] send_key_event called for KEYDOWN
Profile: [CLIPBOARD-DEBUG] CHAR event also sent
```

However, the clipboard operations still don't work in the browser.

#### Conclusion

**What worked:**

- `perform_key_equivalent` now intercepts Cmd+C/V/X/A
- KeyEvents are synthesized via `key_common()`
- Events flow through the full pipeline: GUI → XPC → profile server → CEF
- CEF receives correct VK codes (0x43 for C, 0x56 for V)
- CEF receives correct modifiers (0x80 = COMMAND_DOWN)
- `set_focus(1)` is called before clipboard operations

**What didn't work:**

CEF receives the Cmd+V key event but does not perform the paste operation.

**Root cause hypothesis: macOS process-level clipboard restrictions**

macOS restricts clipboard access based on application state:

1. **Active application**: Only the frontmost app has full clipboard access
2. **Process isolation**: Background processes have limited clipboard capabilities

The `termsurf-profile` process:
- Runs CEF in off-screen, headless mode
- Is NOT the active/frontmost application (wezterm-gui is)
- May be denied clipboard read/write by macOS security policies
- Even with correct key events, CEF cannot access the system clipboard

**Why ts2 works:** CEF runs in-process with the GUI, so it inherits the GUI's
clipboard access as the active application.

**Why ts3 fails:** CEF runs in a separate background process that lacks clipboard
privileges, regardless of whether it receives the correct key events.

**Potential solutions for future experiments:**

1. **Proxy clipboard via XPC**: GUI reads clipboard (it has access), sends
   contents to profile server via XPC, profile server injects text into CEF
   programmatically (not via key events)

2. **Custom CefClipboardHandler**: Implement CEF's clipboard handler interface
   to proxy clipboard operations back to the GUI process

3. **JavaScript injection**: For paste, inject clipboard contents via JavaScript
   (`document.execCommand('insertText', ...)` or Clipboard API)

The key insight is that **key events alone cannot solve this** — the profile
server process fundamentally lacks clipboard access on macOS.

---

### Experiment 6: Proxy Clipboard via XPC

**Goal:** Implement clipboard paste by proxying clipboard contents from the GUI
process to the profile server, bypassing macOS's process-level clipboard
restrictions.

#### Background

Experiment 5 proved that key events reach CEF correctly, but clipboard operations
fail because the profile server process lacks clipboard access. The GUI process
(as the active application) has full clipboard access.

Solution: When Cmd+V is pressed in Browse mode, the GUI reads the clipboard and
sends the contents to the profile server via XPC. The profile server injects the
text into the browser using JavaScript.

#### Approach

**Phase 1: Paste (Cmd+V) — GUI → Profile**

```
User presses Cmd+V
    │
    ▼
GUI: handle_webview_key_event detects Cmd+V in Browse mode
    │
    ▼
GUI: Read clipboard contents using window::Clipboard
    │
    ▼
GUI: Send XPC message { action: "paste_text", text: "..." }
    │
    ▼
Profile: Receive paste_text action
    │
    ▼
Profile: Execute JavaScript to insert text at cursor
    │
    ▼
Browser: Text appears in focused input
```

**Part A: Detect Cmd+V and read clipboard (keyevent.rs)**

In `handle_webview_key_event`, intercept Cmd+V before forwarding:

```rust
WebviewMode::Browse => {
    if is_ctrl_c {
        // ... existing Ctrl+C handling
    }

    // Handle Cmd+V (paste) - proxy clipboard contents
    let is_cmd_v = window_key.key_is_down
        && window_key.modifiers.contains(Modifiers::SUPER)
        && matches!(&window_key.key, KeyCode::Char('v') | KeyCode::Char('V'));

    if is_cmd_v {
        drop(overlays); // Release lock before clipboard access
        if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
            // Read clipboard
            if let Some(clipboard) = self.clipboard.as_ref() {
                if let Ok(text) = clipboard.get_contents() {
                    log::info!("[CLIPBOARD] Cmd+V: sending {} chars to browser", text.len());
                    xpc_manager.send_paste_text(pane_id, &text);
                }
            }
        }
        return Some(true); // Consume the key
    }

    // Forward other keys to browser via XPC
    // ...
}
```

**Part B: Add send_paste_text to XpcManager (webview_xpc.rs)**

```rust
/// Send clipboard text to paste into the browser
pub fn send_paste_text(&self, pane_id: PaneId, text: &str) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "paste_text");
    msg.set_string("text", text);

    if self.send_command(pane_id, &msg) {
        log::info!("[XPC] Sent paste_text to pane {} ({} chars)", pane_id, text.len());
        true
    } else {
        false
    }
}
```

**Part C: Handle paste_text in profile server (main.rs)**

Add handler in the XPC event handler:

```rust
"paste_text" => {
    let text = msg.get_string("text").unwrap_or_default();
    println!("[CLIPBOARD] Received paste_text: {} chars", text.len());

    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: paste_text ignored (state not ready)");
        return;
    };

    let bs = Arc::clone(bs);
    let text = text.to_string();
    drop(state_guard);

    // Post to CEF UI thread
    let mut task = PasteTextTask::new(bs, text);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
```

**Part D: Inject text via JavaScript (main.rs)**

```rust
wrap_task! {
    pub struct PasteTextTask {
        state: Arc<BrowserState>,
        text: String,
    }

    impl Task {
        fn execute(&self) {
            paste_text_to_browser(&self.state, &self.text);
        }
    }
}

fn paste_text_to_browser(state: &BrowserState, text: &str) {
    let browser = match state.browser.lock().unwrap().as_ref() {
        Some(b) => b.clone(),
        None => return,
    };

    let frame = match browser.main_frame() {
        Some(f) => f,
        None => return,
    };

    // Escape text for JavaScript string
    let escaped = text
        .replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace('\n', "\\n")
        .replace('\r', "\\r");

    // Insert text at cursor using execCommand (works in contenteditable and inputs)
    let js = format!(
        "document.execCommand('insertText', false, '{}');",
        escaped
    );

    println!("[CLIPBOARD] Executing JS to paste {} chars", text.len());
    frame.execute_java_script(Some(&js.into()), None, 0);
}
```

#### Why JavaScript Injection?

Several options were considered:

| Approach | Pros | Cons |
|----------|------|------|
| `document.execCommand('insertText')` | Works in inputs, textareas, contenteditable | Deprecated but widely supported |
| Clipboard API (`navigator.clipboard`) | Modern standard | Requires async, may need permissions |
| Simulate key events per character | Pure CEF | Slow, doesn't handle special chars |
| CEF IME APIs | Native | Complex, designed for input methods |

`execCommand('insertText')` is chosen because:
- Synchronous and simple
- Works in all editable contexts
- Handles Unicode correctly
- Still supported in all browsers (deprecation is theoretical)

#### Files to Modify

| File | Changes |
|------|---------|
| `ts3/wezterm-gui/src/termwindow/keyevent.rs` | Intercept Cmd+V, read clipboard, call send_paste_text |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add `send_paste_text()` method |
| `ts3/termsurf-profile/src/main.rs` | Handle `paste_text` action, JavaScript injection |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Basic paste
# Copy "hello world" from another app
web google.com
# Click in search box (or Tab to focus)
# Press Cmd+V
# Expected: "hello world" appears in search box

# Test 2: Multi-line paste
# Copy multi-line text from another app
# Press Cmd+V in a textarea (e.g., web github.com, open an issue)
# Expected: Multi-line text pastes correctly

# Test 3: Special characters
# Copy text with quotes, backslashes, unicode
# Press Cmd+V
# Expected: All characters paste correctly

# Test 4: Empty clipboard
# Clear clipboard
# Press Cmd+V
# Expected: Nothing happens, no crash

# Check logs
cat /tmp/termsurf-gui.log | grep "CLIPBOARD"
cat /tmp/termsurf-profile-*.log | grep "CLIPBOARD"
```

#### Success Criteria

- [ ] Cmd+V in Browse mode triggers clipboard read in GUI
- [ ] Clipboard contents sent to profile server via XPC
- [ ] Profile server receives text and executes JavaScript
- [ ] Text appears in focused input/textarea in browser
- [ ] Multi-line text works
- [ ] Special characters (quotes, backslashes, unicode) work
- [ ] Empty clipboard doesn't crash
- [ ] Normal terminal Cmd+V still works when no webview

#### Future Work (Not in This Experiment)

- **Cmd+C (copy)**: Query selected text from browser via JavaScript, send to GUI, write to clipboard
- **Cmd+X (cut)**: Copy + delete selection
- **Cmd+A (select all)**: Could work with key events, or use `document.execCommand('selectAll')`

#### Result: SUCCESS

Cmd+V now pastes clipboard contents into the browser. The GUI reads the clipboard,
sends text via XPC, and the profile server injects it using JavaScript.

#### Conclusion

**What was accomplished in Phase 1 (Keyboard Input):**

| Feature | Status | Experiment |
|---------|--------|------------|
| Basic keyboard input (typing) | ✅ Working | Exp 1 |
| Arrow keys, Tab, Enter, etc. | ✅ Working | Exp 1 |
| Shift+arrow text selection | ✅ Working | Exp 1 |
| Skip WezTerm keybindings in Browse mode | ✅ Working | Exp 3 |
| Cmd+C/V/X/A generate KeyEvents | ✅ Working | Exp 5 |
| Cmd+V paste into browser | ✅ Working | Exp 6 |
| Cmd+C copy from browser | ❌ Not implemented | — |
| Cmd+X cut from browser | ❌ Not implemented | — |
| Cmd+A select all | ❓ Untested (may work via key events) | — |

**Key technical insights:**

1. **macOS key equivalents**: Cmd+C/V/X don't generate KeyEvents by default.
   Fixed by modifying `performKeyEquivalent:` to intercept and synthesize them.

2. **Process clipboard isolation**: The profile server (CEF) cannot access the
   system clipboard because it's not the active application. Solved by proxying
   clipboard contents from GUI via XPC.

3. **JavaScript injection**: `document.execCommand('insertText')` reliably
   inserts text at the cursor in inputs, textareas, and contenteditable elements.

**Next steps for keyboard input:**

1. **Cmd+C (copy)**: Execute JavaScript to get `window.getSelection().toString()`,
   send result back to GUI via XPC, GUI writes to clipboard.

2. **Cmd+X (cut)**: Same as copy, then execute `document.execCommand('delete')`.

3. **Cmd+A (select all)**: May already work via key events. If not, use
   `document.execCommand('selectAll')`.

**Phase 2 (Mouse Input) - Not yet started:**

- Mouse click/move/drag forwarding to CEF
- Scroll wheel support
- Text selection via mouse
- Right-click context menu

The keyboard input foundation is now solid enough to support basic browser
interaction. Users can navigate to pages, type in forms, and paste content.
