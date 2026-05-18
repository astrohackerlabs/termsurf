+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 329: Focus Lifecycle

The webview caret blinks even when the webview should not have focus.

## Status

**Resolved.** Focus commands now sent on mode change and pane switch.

## Problem

After implementing issue 328 (focus toggle on first paint), the caret now
appears correctly. However, it continues blinking in situations where the
webview should not have focus:

1. **Control mode** — When switching from Browse mode to Control mode (Ctrl+C),
   the caret continues blinking. The webview should be unfocused.

2. **Pane switching** — When switching to another pane, the webview caret keeps
   blinking. Only the active pane's webview should have focus.

**Expected behavior:**

| Scenario                    | Caret State  |
| --------------------------- | ------------ |
| Browse mode, active pane    | Blinking     |
| Browse mode, inactive pane  | Not blinking |
| Control mode, active pane   | Not blinking |
| Control mode, inactive pane | Not blinking |

## Background

Issue 328 added focus initialization on first paint:

```rust
// on_accelerated_paint
if !self.inner.state.initial_focus_set.load(Ordering::Relaxed) {
    host.set_focus(0);
    host.set_focus(1);
    self.inner.state.initial_focus_set.store(true, Ordering::Relaxed);
}
```

This correctly enables the caret on initial load, but there's no mechanism to
update focus state when:

- Mode changes (Browse ↔ Control)
- Active pane changes

## Proposed Solution

Send focus commands to the profile server via XPC when focus state should
change.

### Focus Events

| Event                             | Action                                |
| --------------------------------- | ------------------------------------- |
| Enter Browse mode                 | Send `set_focus(1)` to profile server |
| Enter Control mode                | Send `set_focus(0)` to profile server |
| Pane becomes active (Browse mode) | Send `set_focus(1)` to profile server |
| Pane becomes inactive             | Send `set_focus(0)` to profile server |

### Implementation Approach

**1. Add `focus` XPC command to profile server**

In `ts3/termsurf-profile/src/main.rs`, handle a new `focus` action:

```rust
"focus" => {
    let focused = msg.get_bool("focused");
    if let Some(browser) = browser_state.browser.lock().unwrap().as_ref() {
        if let Some(host) = browser.host() {
            println!("[FOCUS] Setting focus to {}", focused);
            host.set_focus(if focused { 1 } else { 0 });
        }
    }
}
```

**2. Add `send_focus` to XpcManager**

In `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`:

```rust
pub fn send_focus(&self, pane_id: PaneId, focused: bool) {
    let msg = XpcDictionary::new();
    msg.set_string("action", "focus");
    msg.set_bool("focused", focused);
    self.send_command(pane_id, &msg);
}
```

**3. Send focus on mode change**

In `ts3/wezterm-gui/src/termwindow/keyevent.rs`, when mode changes:

```rust
// Ctrl+C in Browse mode → Control mode
WebviewMode::Browse => {
    if is_ctrl_c {
        overlay.mode = WebviewMode::Control;
        // Unfocus webview
        if let Some(xpc) = get_xpc_manager() {
            xpc.send_focus(pane_id, false);
        }
        // ... existing code ...
    }
}

// Enter in Control mode → Browse mode
WebviewMode::Control => {
    if is_enter {
        overlay.mode = WebviewMode::Browse;
        // Refocus webview
        if let Some(xpc) = get_xpc_manager() {
            xpc.send_focus(pane_id, true);
        }
        // ... existing code ...
    }
}
```

**4. Send focus on pane change**

This is more complex. Need to detect when the active pane changes and send
focus/unfocus commands accordingly. Possible locations:

- `MuxNotification::PaneFocused` handler in `termwindow/mod.rs`
- Mouse click handler when clicking on a different pane
- Keyboard navigation between panes

For each webview pane:

- If it becomes active AND is in Browse mode → `send_focus(true)`
- If it becomes inactive → `send_focus(false)`

## Files to Modify

| File                                            | Changes                   |
| ----------------------------------------------- | ------------------------- |
| `ts3/termsurf-profile/src/main.rs`              | Handle `focus` XPC action |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add `send_focus()` method |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`    | Send focus on mode change |
| `ts3/wezterm-gui/src/termwindow/mod.rs`         | Send focus on pane change |

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Control mode unfocuses
web google.com
# Caret should blink in search box
# Press Ctrl+C to enter Control mode
# Expected: Caret stops blinking, webview dims

# Test 2: Browse mode refocuses
# Press Enter to return to Browse mode
# Expected: Caret resumes blinking

# Test 3: Pane switch unfocuses
# Open a second terminal pane (Ctrl+Shift+E or similar)
# Click on the terminal pane
# Expected: Webview caret stops blinking

# Test 4: Return to webview pane refocuses
# Click on the webview pane
# Expected: Caret resumes blinking (if in Browse mode)

# Test 5: Multiple webviews
# Open two webview panes
# Switch between them
# Expected: Only the active webview has a blinking caret
```

## Success Criteria

- [x] Caret stops blinking when entering Control mode
- [x] Caret resumes blinking when entering Browse mode
- [x] Caret stops blinking when switching to another pane
- [x] Caret resumes blinking when switching back to webview in Browse mode
- [x] Multiple webviews: only active one has blinking caret
- [x] No regression in keyboard/mouse input

## Experiments

### Experiment 1: Focus on Mode Change

**Goal:** Stop the caret from blinking when entering Control mode, and resume
blinking when returning to Browse mode.

**Hypothesis:** Sending `host.set_focus(0)` when entering Control mode will
unfocus the browser, stopping the caret. Sending `host.set_focus(1)` when
returning to Browse mode will refocus the browser, resuming the caret.

**Scope:** Mode changes only. Pane switching is deferred to experiment 2.

**Changes:**

1. **Add `focus` action handler in profile server**
   (`ts3/termsurf-profile/src/main.rs`, in the action match block after
   `mouse_wheel`)

   ```rust
   "focus" => {
       // Issue 329: Focus/unfocus browser for caret control
       let state_guard = deferred_for_handler.lock().unwrap();
       let Some(bs) = state_guard.as_ref() else {
           println!("Profile: focus ignored (state not ready)");
           return;
       };

       let focused = msg.get_bool("focused");
       println!("[FOCUS] Received focus command: {}", focused);

       let bs = Arc::clone(bs);
       drop(state_guard);

       let mut task = FocusTask::new(bs, focused);
       cef::post_task(cef::ThreadId::UI, Some(&mut task));
   }
   ```

2. **Add `FocusTask` struct** (`ts3/termsurf-profile/src/main.rs`, near other
   task structs like `ResizeBrowserTask`)

   ```rust
   /// Issue 329: Task to set browser focus state
   struct FocusTask {
       browser_state: Arc<BrowserState>,
       focused: bool,
   }

   impl FocusTask {
       fn new(browser_state: Arc<BrowserState>, focused: bool) -> Self {
           Self { browser_state, focused }
       }
   }

   impl cef::Task for FocusTask {
       fn execute(&mut self) {
           if let Some(browser) = self.browser_state.browser.lock().unwrap().as_ref() {
               if let Some(host) = browser.host() {
                   println!("[FOCUS] Setting focus to {}", self.focused);
                   host.set_focus(if self.focused { 1 } else { 0 });
               }
           }
       }
   }
   ```

3. **Add `send_focus` method to XpcManager**
   (`ts3/wezterm-gui/src/termwindow/webview_xpc.rs`, after `send_select_all`)

   ```rust
   /// Issue 329: Send focus command to the browser
   pub fn send_focus(&self, pane_id: PaneId, focused: bool) -> bool {
       let msg = XpcDictionary::new();
       msg.set_string("action", "focus");
       msg.set_bool("focused", focused);

       if self.send_command(pane_id, &msg) {
           log::info!("[XPC] Sent focus to pane {}: {}", pane_id, focused);
           true
       } else {
           false
       }
   }
   ```

4. **Send focus on mode change** (`ts3/wezterm-gui/src/termwindow/keyevent.rs`,
   in `handle_webview_overlay_key`)

   In the `WebviewMode::Browse` branch, after setting `overlay.mode`:
   ```rust
   if is_ctrl_c {
       log::info!("[Webview] Ctrl+C in Browse mode → Control mode");
       overlay.mode = WebviewMode::Control;
       // Issue 329: Unfocus webview to stop caret blinking
       drop(overlays);
       if let Some(xpc) = crate::termwindow::webview_xpc::get_xpc_manager() {
           xpc.send_focus(pane_id, false);
       }
       if let Some(ref w) = self.window {
           w.invalidate();
       }
       return Some(true);
   }
   ```

   In the `WebviewMode::Control` branch, after setting `overlay.mode`:
   ```rust
   if is_enter {
       log::info!("[Webview] Enter in Control mode → Browse mode");
       overlay.mode = WebviewMode::Browse;
       // Issue 329: Refocus webview to resume caret blinking
       drop(overlays);
       if let Some(xpc) = crate::termwindow::webview_xpc::get_xpc_manager() {
           xpc.send_focus(pane_id, true);
       }
       if let Some(ref w) = self.window {
           w.invalidate();
       }
       return Some(true);
   }
   ```

**Files to modify:**

| File                                            | Changes                                     |
| ----------------------------------------------- | ------------------------------------------- |
| `ts3/termsurf-profile/src/main.rs`              | Add `FocusTask`, add `focus` action handler |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Add `send_focus()` method                   |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`    | Send focus on mode change                   |

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Control mode unfocuses
web google.com
# Click in search box, caret should blink
# Press Ctrl+C to enter Control mode
# Expected: Caret stops blinking

# Check logs
cat /tmp/termsurf-profile-*.log | grep "\[FOCUS\]"
# Expected: "[FOCUS] Received focus command: false"
#           "[FOCUS] Setting focus to false"

# Test 2: Browse mode refocuses
# Press Enter to return to Browse mode
# Expected: Caret resumes blinking

# Check logs
cat /tmp/termsurf-profile-*.log | grep "\[FOCUS\]"
# Expected: "[FOCUS] Received focus command: true"
#           "[FOCUS] Setting focus to true"

# Test 3: Keyboard input still works in Browse mode
# Type "hello" in the search box
# Expected: Text appears, caret follows

# Test 4: Ctrl+C in Control mode still closes webview
# Press Ctrl+C (now in Browse mode)
# Press Ctrl+C again (now in Control mode)
# Expected: Webview closes
```

**Success criteria:**

- [x] Caret stops blinking when pressing Ctrl+C (Browse → Control)
- [x] Caret resumes blinking when pressing Enter (Control → Browse)
- [x] Keyboard input still works in Browse mode
- [x] Ctrl+C in Control mode still closes the webview
- [x] Logs show focus commands being sent and received

**Status:** Success.

**Risks:**

1. **CEF thread safety** — `set_focus` must be called on the CEF UI thread. The
   `FocusTask` pattern (same as other XPC handlers) ensures this.

2. **Focus state mismatch** — If user rapidly toggles modes, focus state could
   get out of sync. This is acceptable for now; experiment 2 (pane switching)
   may need more robust state tracking.

### Experiment 2: Focus on Pane Change

**Goal:** Stop the caret from blinking when switching to another pane, and
resume blinking when switching back to a webview pane in Browse mode.

**Hypothesis:** When the active pane changes (via click or keyboard navigation),
we can detect the change in `MuxNotification::PaneFocused` handler and send
focus commands to the affected webviews.

**Scope:** Pane switching via mouse click and keyboard navigation.

**Background:**

When a user clicks on a different pane:

1. `mouse_event_terminal` calls `tab.set_active_idx(pos.index)`
   (mouseevent.rs:712)
2. This triggers `MuxNotification::PaneFocused(pane_id)`
3. Handler at mod.rs:1341 currently just calls `update_title_post_status()`

We need to:

- Track which pane was previously focused
- When focus changes, unfocus the old webview (if any)
- Focus the new webview if it's in Browse mode

**Changes:**

1. **Add `last_focused_pane` field to TermWindow**
   (`ts3/wezterm-gui/src/termwindow/mod.rs`, in the struct definition)

   ```rust
   /// Issue 329: Last focused pane for webview focus tracking
   #[cfg(target_os = "macos")]
   last_focused_pane: Option<PaneId>,
   ```

2. **Initialize field in `new_window`**
   (`ts3/wezterm-gui/src/termwindow/mod.rs`)

   ```rust
   #[cfg(target_os = "macos")]
   last_focused_pane: None,
   ```

3. **Handle focus change in `MuxNotification::PaneFocused`**
   (`ts3/wezterm-gui/src/termwindow/mod.rs`, around line 1341)

   ```rust
   MuxNotification::PaneFocused(new_pane_id) => {
       // Issue 329: Handle webview focus on pane change
       #[cfg(target_os = "macos")]
       {
           use crate::termwindow::webview_socket::{get_server, WebviewMode};
           use crate::termwindow::webview_xpc::get_xpc_manager;

           let old_pane_id = self.last_focused_pane;
           self.last_focused_pane = Some(new_pane_id);

           if let (Some(xpc), Some(server)) = (get_xpc_manager(), get_server()) {
               let state = server.state();
               let overlays = state.read().unwrap();

               // Unfocus old webview (if it had one)
               if let Some(old_id) = old_pane_id {
                   if old_id != new_pane_id {
                       if overlays.overlays.contains_key(&old_id) {
                           log::info!("[FOCUS] Pane change: unfocusing old pane {}", old_id);
                           xpc.send_focus(old_id, false);
                       }
                   }
               }

               // Focus new webview if in Browse mode
               if let Some(overlay) = overlays.overlays.get(&new_pane_id) {
                   if overlay.mode == WebviewMode::Browse {
                       log::info!("[FOCUS] Pane change: focusing new pane {} (Browse mode)", new_pane_id);
                       xpc.send_focus(new_pane_id, true);
                   }
               }
           }
       }

       // Existing code
       self.update_title_post_status();
   }
   ```

**Files to modify:**

| File                                    | Changes                                   |
| --------------------------------------- | ----------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/mod.rs` | Add field, initialize, handle PaneFocused |

**Verification:**

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Click on different pane unfocuses webview
web google.com
# Click in search box, caret should blink
# Split pane: Ctrl+Shift+E (or similar)
# Click on the terminal pane
# Expected: Webview caret stops blinking

# Check logs
cat /tmp/termsurf-gui.log | grep "\[FOCUS\]"
# Expected: "[FOCUS] Pane change: unfocusing old pane X"

# Test 2: Click back on webview refocuses
# Click on the webview pane
# Expected: Caret resumes blinking (if in Browse mode)

# Check logs
cat /tmp/termsurf-gui.log | grep "\[FOCUS\]"
# Expected: "[FOCUS] Pane change: focusing new pane Y (Browse mode)"

# Test 3: Click on webview in Control mode
# Press Ctrl+C to enter Control mode
# Click on terminal pane, then back on webview
# Expected: Caret does NOT resume (still in Control mode)

# Test 4: Keyboard navigation
# Use Ctrl+Shift+Arrow to navigate between panes
# Expected: Same focus behavior as mouse click

# Test 5: Multiple webviews
# Open webview in both panes: web google.com (in each)
# Switch between them
# Expected: Only the active webview has blinking caret
```

**Success criteria:**

- [x] Caret stops blinking when clicking on another pane
- [x] Caret resumes blinking when clicking back on webview (Browse mode)
- [x] Caret does NOT resume if webview is in Control mode
- [x] Keyboard pane navigation triggers same behavior
- [x] Multiple webviews: only active one has blinking caret
- [x] No regression in mode switching (experiment 1)

**Status:** Success.

**Risks:**

1. **Initialization race** — `last_focused_pane` starts as `None`, so the first
   pane switch won't unfocus anything. This is acceptable since there's no prior
   webview to unfocus.

2. **Tab switching** — Switching tabs may also trigger `PaneFocused`. Need to
   verify this works correctly across tabs.

3. **Rapid clicking** — Multiple rapid clicks could cause focus state churn.
   This should be benign since each click will eventually settle.

## Conclusion

Both experiments succeeded. The webview caret now correctly:

- Stops blinking when entering Control mode (Ctrl+C)
- Resumes blinking when returning to Browse mode (Enter)
- Stops blinking when switching to another pane
- Resumes blinking when switching back to a webview in Browse mode

### Unrelated Issue Discovered During Testing

While testing experiment 2 with multiple webviews, we observed that closing one
webview caused the profile server to exit, making the other webview inactive.
Investigation of the logs revealed:

```
Profile: GUI connection established (total: 1)
Profile: GUI connection established (total: 2)
Profile: GUI disconnected (remaining: 1)
Profile: GUI disconnected (remaining: 0)
Profile: No more GUI connections, exiting gracefully
```

Both GUI connections were being disconnected when only one webview was closed.
The GUI-side logs confirmed proper tracking (separate connections for pane 0 and
pane 1), but an "XPC connection invalid" error appeared immediately after
removing one connection.

After reverting the experiment 2 code changes, the issue persisted, confirming
this is a **pre-existing bug** unrelated to the focus lifecycle changes. The bug
likely exists in:

- The XPC connection cleanup logic
- The profile server's handling of multiple browser instances
- Or macOS XPC behavior when closing one of multiple connections

This issue should be tracked separately (see issue 330 when created).

## References

- Issue 328 — Initial caret fix (focus toggle on first paint)
- Issue 315 — Mode system (Browse/Control modes)
- `ts3/wezterm-gui/src/termwindow/keyevent.rs` — Mode switching logic
- `ts3/wezterm-gui/src/termwindow/mod.rs` — Pane focus handling
