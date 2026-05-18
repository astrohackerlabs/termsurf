+++
status = "closed"
opened = "2026-02-02"
closed = "2026-02-02"
+++

# Issue 335: Browser Navigation (Back/Forward)

## Product Requirements

### User Story

As a user browsing the web in TermSurf, I want to navigate back and forward
through my browsing history using familiar keyboard shortcuts, so that I can
revisit pages I've already viewed without retyping URLs.

### Acceptance Criteria

1. **Cmd+[** navigates back in browser history (like Safari/Chrome)
2. **Cmd+]** navigates forward in browser history (like Safari/Chrome)
3. Navigation only works when a webview pane is focused
4. Navigation works in both Browse mode and Control mode
5. If there's no history to navigate (e.g., can't go back on first page), the
   command does nothing silently

### Keybindings

| Shortcut | Action           | Notes                           |
| -------- | ---------------- | ------------------------------- |
| Cmd+[    | Navigate back    | Standard macOS browser shortcut |
| Cmd+]    | Navigate forward | Standard macOS browser shortcut |

### Non-Requirements (Out of Scope)

- History list/menu (showing all visited pages)
- Visual feedback when navigation occurs (URL will change naturally)
- Configurable keybindings (hardcoded for now, like ts2)
- Mouse back/forward buttons (future issue)

## Technical Context

### ts2 Implementation

In ts2, CEF runs in-process. Navigation is handled by:

1. Intercepting Cmd+[/] in `keyevent.rs` (lines 480-500) as special cases in
   Browse mode
2. Calling `browser.go_back()` / `browser.go_forward()` directly on the CEF
   browser object

### ts3 Challenge

In ts3, CEF runs **out-of-process** in `termsurf-profile`. The GUI cannot call
CEF methods directly — it must send a message to the profile server via IPC.

### IPC Options

**Option A: Unix socket protocol**

Extend the existing socket protocol (used for `open_webview`) with new commands:

```json
{"action": "go_back", "pane_id": 123}
{"action": "go_forward", "pane_id": 123}
```

**Option B: XPC messages**

Send navigation commands via the direct XPC connection between GUI and profile
server.

## Files Involved

### GUI Side

- `ts3/wezterm-gui/src/termwindow/keyevent.rs` — Intercept Cmd+[/] and send
  navigation command
- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` — Add protocol support for
  navigation commands (if using socket)
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — Add XPC message types (if
  using XPC)

### Profile Server Side

- `ts3/termsurf-web/src/main.rs` — Handle navigation commands, call CEF methods

### CEF Bindings

- `cef-rs/cef/src/bindings/` — Already has `go_back()` and `go_forward()` on
  Browser object

---

## Experiments

### Experiment 1: XPC-based navigation commands

**Status: Success**

Use XPC messages (Option B) to send navigation commands from GUI to profile
server. This follows the established pattern used for copy/cut/select_all
(issue 318).

#### Step 1: Add XPC methods in GUI (webview_xpc.rs)

Add `send_go_back()` and `send_go_forward()` methods after `send_select_all()`
(around line 521):

```rust
/// Send go_back command to the browser (issue 335)
pub fn send_go_back(&self, pane_id: PaneId) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "go_back");

    if self.send_command(pane_id, &msg) {
        log::info!("[XPC] Sent go_back to pane {}", pane_id);
        true
    } else {
        false
    }
}

/// Send go_forward command to the browser (issue 335)
pub fn send_go_forward(&self, pane_id: PaneId) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "go_forward");

    if self.send_command(pane_id, &msg) {
        log::info!("[XPC] Sent go_forward to pane {}", pane_id);
        true
    } else {
        false
    }
}
```

#### Step 2: Intercept Cmd+[/] in Browse mode (keyevent.rs)

In `handle_webview_key_event`, add handlers in the `WebviewMode::Browse` arm
after the Cmd+A handler (around line 1038):

```rust
// Handle Cmd+[ (go back) - issue 335
let is_cmd_bracket_left = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && matches!(&window_key.key, KeyCode::Char('['));

if is_cmd_bracket_left {
    log::info!("[NAV] Cmd+[ detected, sending go_back to browser");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_go_back(pane_id);
    }
    return Some(true);
}

// Handle Cmd+] (go forward) - issue 335
let is_cmd_bracket_right = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && matches!(&window_key.key, KeyCode::Char(']'));

if is_cmd_bracket_right {
    log::info!("[NAV] Cmd+] detected, sending go_forward to browser");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_go_forward(pane_id);
    }
    return Some(true);
}
```

#### Step 3: Intercept Cmd+[/] in Control mode (keyevent.rs)

In the `WebviewMode::Control` arm, add handlers before the final `Some(false)`
return (around line 1083):

```rust
// Handle Cmd+[ (go back) in Control mode - issue 335
let is_cmd_bracket_left = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && matches!(&window_key.key, KeyCode::Char('['));

if is_cmd_bracket_left {
    log::info!("[NAV] Cmd+[ in Control mode, sending go_back");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_go_back(pane_id);
    }
    return Some(true);
}

// Handle Cmd+] (go forward) in Control mode - issue 335
let is_cmd_bracket_right = window_key.key_is_down
    && window_key.modifiers.contains(Modifiers::SUPER)
    && matches!(&window_key.key, KeyCode::Char(']'));

if is_cmd_bracket_right {
    log::info!("[NAV] Cmd+] in Control mode, sending go_forward");
    drop(overlays);
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.send_go_forward(pane_id);
    }
    return Some(true);
}
```

#### Step 4: Add navigation tasks in profile server (termsurf-profile/src/main.rs)

Add GoBackTask and GoForwardTask after SelectAllTask (around line 1340):

```rust
// ====== Go Back Task ======
//
// Task for navigating back in browser history via CEF's browser.go_back().
// Issue 335: Browser navigation.

wrap_task! {
    pub struct GoBackTask {
        state: Arc<BrowserState>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                println!("[NAV] Calling browser.go_back()");
                browser.go_back();
            } else {
                println!("[NAV] GoBackTask: no browser");
            }
        }
    }
}

// ====== Go Forward Task ======
//
// Task for navigating forward in browser history via CEF's browser.go_forward().
// Issue 335: Browser navigation.

wrap_task! {
    pub struct GoForwardTask {
        state: Arc<BrowserState>,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                println!("[NAV] Calling browser.go_forward()");
                browser.go_forward();
            } else {
                println!("[NAV] GoForwardTask: no browser");
            }
        }
    }
}
```

#### Step 5: Handle XPC actions in profile server (termsurf-profile/src/main.rs)

Add handlers for "go_back" and "go_forward" actions in the XPC message handler,
after the "do_select_all" case (around line 924):

```rust
"go_back" => {
    // Issue 335: Navigate back in browser history
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: go_back ignored (state not ready)");
        return;
    };

    println!("[NAV] Received go_back command");

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = GoBackTask::new(bs);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
"go_forward" => {
    // Issue 335: Navigate forward in browser history
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("Profile: go_forward ignored (state not ready)");
        return;
    };

    println!("[NAV] Received go_forward command");

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = GoForwardTask::new(bs);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com                    # Opens webview
# Click a link to navigate to another page
# Press Cmd+[ → should go back to google.com
# Press Cmd+] → should go forward to the linked page
# Press Ctrl+C to enter Control mode
# Press Cmd+[ → should still navigate back
# Press Cmd+] → should still navigate forward
```

Check logs:
```bash
tail -f /tmp/termsurf-gui.log | grep NAV
tail -f /tmp/termsurf-profile-*.log | grep NAV
```

---

## Conclusion

Browser navigation with Cmd+[ (back) and Cmd+] (forward) is now implemented.
The feature works in both Browse mode and Control mode, following the
established XPC pattern used for clipboard operations (issue 318).

### Implementation Summary

| Component       | Change                                                    |
| --------------- | --------------------------------------------------------- |
| webview_xpc.rs  | Added `send_go_back()` and `send_go_forward()` methods    |
| keyevent.rs     | Intercept Cmd+[/] in Browse and Control modes             |
| termsurf-profile| Added `GoBackTask`, `GoForwardTask`, and XPC handlers     |

### User Experience

- **Cmd+[** navigates back in browser history
- **Cmd+]** navigates forward in browser history
- Works in both Browse mode (typing in page) and Control mode (terminal focus)
- Silent no-op if no history exists (matches native browser behavior)
