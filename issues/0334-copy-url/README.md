+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# Issue 334: Copy URL with Cmd+C in control mode

## Goal

Allow users to copy the current webview URL to clipboard using Cmd+C when in
control mode. Show a brief confirmation message.

## Requirements

1. **Trigger**: Cmd+C while in control mode
2. **Action**: Copy current URL to system clipboard
3. **Feedback**: Briefly display "url copied" in the control panel
4. **Duration**: Message visible for ~1-2 seconds, then revert to normal display

## Behavior

### Before (Control mode)

```
┌─────────────────────────────────────────────────────┐
│ Enter to browse. Ctrl+C to exit.            default │
└─────────────────────────────────────────────────────┘
```

### After Cmd+C (briefly)

```
┌─────────────────────────────────────────────────────┐
│ url copied                                  default │
└─────────────────────────────────────────────────────┘
```

### Then reverts to normal

```
┌─────────────────────────────────────────────────────┐
│ Enter to browse. Ctrl+C to exit.            default │
└─────────────────────────────────────────────────────┘
```

## Notes

- Only works in control mode (not browse mode where Cmd+C goes to the browser)
- Ctrl+C (without Cmd) still exits the webview as before

## Files Involved

- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` - Overlay state (add
  feedback timestamp)
- `ts3/wezterm-gui/src/termwindow/keyevent.rs` - Key handling (add Cmd+C in
  Control mode)
- `ts3/wezterm-gui/src/termwindow/render/pane.rs` - Control panel text display

---

## Experiment 1: Copy URL with feedback message

**Status: Failed**

Add Cmd+C handling in Control mode to copy the URL and show brief feedback.

### Step 1: Add feedback state to WebviewOverlay

In `webview_socket.rs`, add a field to track when the "url copied" message
should be displayed (line 338, in `WebviewOverlay` struct):

```rust
pub struct WebviewOverlay {
    pub session_id: String,
    pub tab_id: TabId,
    pub mode: WebviewMode,
    pub profile: String,
    /// When set, show "url copied" feedback until this instant
    pub copy_feedback_until: Option<std::time::Instant>,
}
```

Update overlay creation (lines 539-544 and 595-600) to initialize the new field:

```rust
let overlay = WebviewOverlay {
    session_id: session_id.clone(),
    tab_id,
    mode: WebviewMode::default(),
    profile: profile.to_string(),
    copy_feedback_until: None,
};
```

### Step 2: Handle Cmd+C in Control mode

In `keyevent.rs`, in the `handle_webview_key_event` function, add Cmd+C handling
in the `WebviewMode::Control` match arm (after line 1082, before `Some(false)`):

```rust
WebviewMode::Control => {
    if is_enter {
        // ... existing Enter handling ...
    }
    if is_ctrl_c {
        // ... existing Ctrl+C handling ...
    }

    // Handle Cmd+C (copy URL to clipboard)
    let is_cmd_c = window_key.key_is_down
        && window_key.modifiers.contains(Modifiers::SUPER)
        && matches!(&window_key.key, KeyCode::Char('c') | KeyCode::Char('C'));

    if is_cmd_c {
        log::info!("[Webview] Cmd+C in Control mode → Copy URL");

        // Get URL from XpcManager
        if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
            if let Some(surface) = xpc_manager.get_received_surface(pane_id) {
                let url = surface.url.clone();
                log::info!("[Webview] Copying URL to clipboard: {}", url);

                // Copy to system clipboard
                if let Some(window) = self.window.as_ref() {
                    window.set_clipboard(::window::Clipboard::Clipboard, url);
                }

                // Set feedback timestamp (show "url copied" for 1.5 seconds)
                overlay.copy_feedback_until = Some(
                    std::time::Instant::now() + std::time::Duration::from_millis(1500)
                );
            }
        }

        drop(overlays);
        if let Some(ref w) = self.window {
            w.invalidate();
        }
        return Some(true);
    }

    // In Control mode, return Some(false) to allow keybindings
    Some(false)
}
```

### Step 3: Modify control panel rendering

In `render/pane.rs`, update the `display_text` calculation (around line 950) to
check the feedback state:

```rust
let display_text = match overlay.mode {
    WebviewMode::Browse => {
        // ... existing URL truncation logic ...
    }
    WebviewMode::Control => {
        // Check for copy feedback
        if let Some(until) = overlay.copy_feedback_until {
            if std::time::Instant::now() < until {
                "url copied".to_string()
            } else {
                "Enter to browse. Ctrl+C to exit.".to_string()
            }
        } else {
            "Enter to browse. Ctrl+C to exit.".to_string()
        }
    }
};
```

### Step 4: Auto-invalidate for timeout

The feedback message needs to disappear after 1.5 seconds. Since WezTerm
continuously redraws, the message will naturally disappear when the timestamp
expires. However, if the terminal is idle, we need to ensure a redraw happens.

In the Control mode rendering block, after computing `display_text`, schedule an
invalidation if we're showing feedback:

```rust
// Schedule invalidation for feedback timeout
if overlay.copy_feedback_until.is_some() {
    if let Some(ref w) = self.window {
        // Request redraw after a short delay to clear the feedback
        w.invalidate();
    }
}
```

This causes continuous redraws while feedback is active, which isn't ideal but
is simple. A more elegant solution would use a timer, but this works for now.

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com                    # Opens webview
# Press Ctrl+C to enter control mode
# Press Cmd+C
# Expected: "url copied" appears briefly, then reverts to normal
# Check clipboard: pbpaste should show the URL
```

### Conclusion

Cmd+C in Control mode did not trigger the handler. The key event was not
reaching `handle_webview_key_event` because the macOS menu system intercepts
Cmd+C and routes it to the `CopyTo` key assignment handler instead.

---

## Experiment 2: Modify CopyTo handler for webview URL

**Status: Success**

Instead of intercepting the key event, modify the `CopyTo` action handler to
check for webview Control mode and copy the URL.

### Analysis

The `CopyTo` key assignment is handled in `perform_key_assignment`
(`termwindow/mod.rs` line 2789):

```rust
CopyTo(dest) => {
    let text = self.selection_text(pane);
    self.copy_to_clipboard(*dest, text);
}
```

When Cmd+C is pressed, the menu system triggers this handler. We need to:

1. Check if the pane has a webview overlay in Control mode
2. If yes, copy the URL and set feedback timestamp
3. If no, do the normal selection copy

### Step 1: Modify CopyTo handler

In `termwindow/mod.rs`, update the `CopyTo` handler (line 2789):

```rust
CopyTo(dest) => {
    // Issue 334: Check for webview Control mode
    #[cfg(target_os = "macos")]
    {
        use crate::termwindow::webview_socket::{get_server, WebviewMode};

        let pane_id = pane.pane_id();
        if let Some(server) = get_server() {
            let state = server.state();
            let mut overlays = state.write().unwrap();
            if let Some(overlay) = overlays.overlays.get_mut(&pane_id) {
                if overlay.mode == WebviewMode::Control {
                    // Copy URL instead of selection
                    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
                        if let Some(surface) = xpc_manager.get_received_surface(pane_id) {
                            let url = surface.url.clone();
                            log::info!("[Webview] CopyTo in Control mode → Copy URL: {}", url);
                            self.copy_to_clipboard(*dest, url);

                            // Set feedback timestamp
                            overlay.copy_feedback_until = Some(
                                std::time::Instant::now() + std::time::Duration::from_millis(1500)
                            );
                            drop(overlays);
                            if let Some(ref w) = self.window {
                                w.invalidate();
                            }
                            return Ok(PerformAssignmentResult::Handled);
                        }
                    }
                }
            }
        }
    }

    // Normal copy behavior
    let text = self.selection_text(pane);
    self.copy_to_clipboard(*dest, text);
}
```

### Step 2: Remove key event handler (cleanup)

Remove the Cmd+C handling added in Experiment 1 from `keyevent.rs` since it's
now handled in the CopyTo action.

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com                    # Opens webview
# Press Ctrl+C to enter control mode
# Press Cmd+C
# Expected: "url copied" appears briefly, then reverts to normal
# Check clipboard: pbpaste should show the URL
```

---

## Conclusion

Users can now copy the current webview URL to the clipboard using Cmd+C while in
Control mode. A brief "url copied" feedback message appears in the control panel
for 1.5 seconds.

### What We Learned

1. **Menu shortcuts take priority over key events.** Cmd+C is intercepted by the
   macOS menu system and routed to the `CopyTo` key assignment handler, never
   reaching the key event handler. Experiment 1 failed because it tried to
   intercept the key event directly.

2. **Work with the system, not against it.** Instead of fighting the menu
   system, Experiment 2 modified the `CopyTo` handler to check for webview
   Control mode. This integrates naturally with WezTerm's existing architecture.

### Implementation Summary

| Component               | Change                                                         |
| ----------------------- | -------------------------------------------------------------- |
| `WebviewOverlay` struct | Added `copy_feedback_until: Option<Instant>` field             |
| `CopyTo` handler        | Check for webview Control mode; copy URL instead of selection  |
| Control panel rendering | Show "url copied" when feedback timestamp is active            |
| Auto-invalidate         | Continuous redraws while feedback is visible to ensure timeout |

### User Experience

```
Control mode (normal):     "Enter to browse. Ctrl+C to exit."
After Cmd+C (1.5 sec):     "url copied"
Then reverts to:           "Enter to browse. Ctrl+C to exit."
```
