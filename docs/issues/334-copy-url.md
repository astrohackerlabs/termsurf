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
