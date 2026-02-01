# Issue 319: Mouse Input for Webview

## Goal

Enable mouse interaction with webviews in ts3. Users should be able to click,
drag, and scroll within browser panes using their mouse or trackpad.

## Status

Not started.

## Requirements

### Clicking

| Requirement  | Description                                       |
| ------------ | ------------------------------------------------- |
| Left click   | Click links, buttons, form elements               |
| Right click  | No action (context menu deferred to future issue) |
| Middle click | No action for now                                 |
| Double-click | Select word                                       |
| Triple-click | Select line/paragraph                             |

### Dragging

| Requirement    | Description                                |
| -------------- | ------------------------------------------ |
| Text selection | Click and drag to select text              |
| Drag handles   | Drag slider controls, resize handles, etc. |
| Scroll by drag | Drag scrollbars to scroll content          |

### Scrolling

| Requirement       | Description                                               |
| ----------------- | --------------------------------------------------------- |
| Scroll wheel      | Vertical scrolling with mouse wheel                       |
| Horizontal scroll | Shift+scroll or horizontal wheel for horizontal scrolling |
| Trackpad scroll   | Two-finger scroll gesture                                 |
| Smooth scrolling  | Scrolling should feel native and smooth                   |

### Hover

| Requirement    | Description                                                                     |
| -------------- | ------------------------------------------------------------------------------- |
| Hover effects  | CSS :hover states should activate on mouse over                                 |
| Tooltips       | Native browser tooltips should appear                                           |
| Cursor changes | Cursor should change based on element (pointer for links, text for input, etc.) |

## Out of Scope

- Context menus (right-click menu) — separate issue
- Drag and drop between applications
- Pinch to zoom
- Force touch / pressure sensitivity

## Research: ts2 Mouse Input

ts2 handles mouse input in-process. ts3 must forward events via XPC, but the
CEF API calls and coordinate transformations are the same.

### Key Files (ts2)

| File | Purpose |
|------|---------|
| `ts2/wezterm-gui/src/cef_browser/mod.rs` | CEF browser API wrappers |
| `ts2/wezterm-gui/src/termwindow/mouseevent.rs` | Mouse event routing |

### Event Flow

1. Window system → `mouse_event_impl()` → `mouse_event_browser()`
2. Transform coordinates: physical window → browser-relative → logical (DIP)
3. Call CEF host methods

### Coordinate Transformation

```rust
// Physical to browser-relative
let rel_x = event.coords.x - pane_x;
let rel_y = event.coords.y - pane_y;

// Physical to logical (CEF expects DIP)
let scale = dpi / 72.0;  // macOS base DPI = 72
let cef_x = (rel_x / scale) as i32;
let cef_y = (rel_y / scale) as i32;
```

### CEF APIs

| Method | Purpose |
|--------|---------|
| `host.send_mouse_move_event()` | Mouse movement, hover |
| `host.send_mouse_click_event()` | Press/release, click count |
| `host.send_mouse_wheel_event()` | Scroll (delta × 120) |

### Modifier Flags

CEF uses a bitmask for modifiers and button state:

```rust
EVENTFLAG_SHIFT_DOWN: u32 = 1 << 1;
EVENTFLAG_CONTROL_DOWN: u32 = 1 << 2;
EVENTFLAG_ALT_DOWN: u32 = 1 << 3;
EVENTFLAG_LEFT_MOUSE_BUTTON: u32 = 1 << 4;
EVENTFLAG_MIDDLE_MOUSE_BUTTON: u32 = 1 << 5;
EVENTFLAG_RIGHT_MOUSE_BUTTON: u32 = 1 << 6;
EVENTFLAG_COMMAND_DOWN: u32 = 1 << 7;
```

### Key Patterns

1. **Button state tracking** — Track which buttons are pressed across events
2. **Modifier composition** — Combine keyboard modifiers + button state
3. **Wheel delta** — Multiply by 120 (Windows scroll standard)
4. **Mode switching** — Click on browser area switches Control → Browse mode

### ts3 Adaptation

The main difference for ts3: events must be serialized and sent via XPC to the
profile server, which then calls the CEF host methods. The XPC message would
include:

- Event type (move, click, wheel)
- Coordinates (already transformed to logical pixels)
- Button type and state (for clicks)
- Modifiers bitmask
- Click count (for double/triple click)
- Wheel deltas (for scroll)

## Hypothesis: Forward Transformed Events via XPC

Do coordinate transformation in the GUI (where we have pane bounds and DPI),
then send logical coordinates via XPC. The profile server just calls CEF
methods — no layout knowledge needed.

### GUI Side (wezterm-gui)

**1. Intercept in `mouse_event_impl()` (mouseevent.rs)**

Check if mouse is over a webview pane in Browse mode. If so, intercept instead
of normal handling.

**2. Transform coordinates**

```rust
// Get pane bounds (already available from render state)
let rel_x = event.coords.x - pane_x;
let rel_y = event.coords.y - pane_y;

// Convert to logical pixels
let scale = dpi / 72.0;
let cef_x = (rel_x / scale) as i32;
let cef_y = (rel_y / scale) as i32;
```

**3. Send via XPC (webview_xpc.rs)**

One method for each event type:

```rust
pub fn send_mouse_move(&self, pane_id, x, y, modifiers);
pub fn send_mouse_click(&self, pane_id, x, y, button, is_up, click_count, modifiers);
pub fn send_mouse_wheel(&self, pane_id, x, y, delta_x, delta_y, modifiers);
```

**4. Track button state on GUI side**

Maintain `mouse_buttons: u32` flag field, update on press/release, combine with
keyboard modifiers.

### Profile Server Side (termsurf-profile)

**1. Handle XPC messages**

```rust
"mouse_move" => { post MouseMoveTask }
"mouse_click" => { post MouseClickTask }
"mouse_wheel" => { post MouseWheelTask }
```

**2. Tasks call CEF host methods**

```rust
host.send_mouse_move_event(Some(&mouse_event), mouse_leave);
host.send_mouse_click_event(Some(&mouse_event), button, mouse_up, click_count);
host.send_mouse_wheel_event(Some(&mouse_event), delta_x, delta_y);
```

### Why This Should Work

1. **Same pattern as keyboard** — We already do XPC message → post_task → CEF
2. **GUI has all layout info** — Pane bounds, DPI, mode state already available
3. **Profile server stays simple** — Just receives coordinates and calls CEF
4. **No new architecture** — Extends existing XpcManager methods

### Potential Complications

1. **Click counting** — Double/triple click detection needs timeout logic on GUI
2. **Mouse leave events** — Need to detect when mouse exits pane bounds
3. **Cursor changes** — CEF may need to send cursor type back to GUI (reverse XPC)
4. **Latency** — XPC round-trip for every mouse move could feel sluggish

### Suggested First Experiment

Start with just `send_mouse_move` and `send_mouse_click` for left button. Verify
clicking links works before adding wheel, modifiers, and click counting.

## Success Criteria

- [ ] Can click links to navigate
- [ ] Can click buttons and form elements
- [ ] Can double-click to select words
- [ ] Can click and drag to select text
- [ ] Can scroll with mouse wheel
- [ ] Can scroll with trackpad gestures
- [ ] Hover effects work (CSS :hover, tooltips)
- [ ] Cursor changes appropriately (pointer, text, etc.)

---

## Experiment 1: Mouse Move and Left Click

**Status: FAILED**

Start with the minimal implementation: mouse movement and left-button clicks. Verify
that clicking links works before adding scrolling, modifiers, or click counting.

### Goal

- Mouse hover over webview pane triggers CEF hover effects
- Left-click on links navigates to the link target

### Files to Modify

| File                                                 | Changes                                    |
| ---------------------------------------------------- | ------------------------------------------ |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`      | Add `send_mouse_move`, `send_mouse_click`  |
| `ts3/wezterm-gui/src/termwindow/mouseevent.rs`       | Intercept mouse events for webview panes   |
| `ts3/termsurf-profile/src/main.rs`                   | Handle XPC messages, call CEF host methods |

### Part 1: XPC Methods (webview_xpc.rs)

Add two methods to XpcManager after the existing `send_select_all` method:

```rust
/// Send mouse move event to the browser (issue 319, experiment 1)
pub fn send_mouse_move(&self, pane_id: PaneId, x: i32, y: i32, modifiers: u32) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "mouse_move");
    msg.set_i64("x", x as i64);
    msg.set_i64("y", y as i64);
    msg.set_i64("modifiers", modifiers as i64);

    if self.send_command(pane_id, &msg) {
        log::trace!("[XPC] Sent mouse_move to pane {}: ({}, {})", pane_id, x, y);
        true
    } else {
        false
    }
}

/// Send mouse click event to the browser (issue 319, experiment 1)
pub fn send_mouse_click(
    &self,
    pane_id: PaneId,
    x: i32,
    y: i32,
    button: u32,
    is_up: bool,
    click_count: i32,
    modifiers: u32,
) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "mouse_click");
    msg.set_i64("x", x as i64);
    msg.set_i64("y", y as i64);
    msg.set_i64("button", button as i64);
    msg.set_bool("is_up", is_up);
    msg.set_i64("click_count", click_count as i64);
    msg.set_i64("modifiers", modifiers as i64);

    if self.send_command(pane_id, &msg) {
        log::debug!(
            "[XPC] Sent mouse_click to pane {}: ({}, {}) btn={} up={} count={}",
            pane_id, x, y, button, is_up, click_count
        );
        true
    } else {
        false
    }
}
```

### Part 2: Intercept Mouse Events (mouseevent.rs)

Add a new method to TermWindow and call it early in `mouse_event_impl`:

**2a. Add helper method to check webview pane bounds**

```rust
/// Check if mouse event is over a webview pane in Browse mode.
/// Returns Some((pane_id, rel_x, rel_y, scale)) if so, None otherwise.
#[cfg(target_os = "macos")]
fn mouse_over_webview(&self, event: &MouseEvent) -> Option<(mux::pane::PaneId, f32, f32, f32)> {
    use crate::termwindow::webview_socket::{get_server, WebviewMode};

    let server = get_server()?;
    let state = server.state();
    let overlays = state.read().unwrap();

    // Check each pane to find if mouse is over a webview
    for pos in self.get_panes_to_render() {
        let pane_id = pos.pane.pane_id();

        // Only consider panes with webview overlays in Browse mode
        let overlay = overlays.overlays.get(&pane_id)?;
        if overlay.mode != WebviewMode::Browse {
            continue;
        }

        // Calculate viewport bounds (same logic as render_webview_overlays_webgpu)
        let border = self.get_os_border();
        let tab_bar_height = if self.show_tab_bar && !self.config.tab_bar_at_bottom {
            self.tab_bar_pixel_height().unwrap_or(0.)
        } else {
            0.
        };

        let pane_x = pos.left as f32 * self.render_metrics.cell_size.width as f32
            + border.left.get() as f32;
        let pane_y = pos.top as f32 * self.render_metrics.cell_size.height as f32
            + border.top.get() as f32
            + tab_bar_height;
        let pane_w = pos.width as f32 * self.render_metrics.cell_size.width as f32;
        let pane_h = pos.height as f32 * self.render_metrics.cell_size.height as f32;

        // Check if mouse is within pane bounds
        let mx = event.coords.x as f32;
        let my = event.coords.y as f32;

        if mx >= pane_x && mx < pane_x + pane_w && my >= pane_y && my < pane_y + pane_h {
            // Calculate relative position within pane
            let rel_x = mx - pane_x;
            let rel_y = my - pane_y;

            // Get scale factor
            let scale = self.dimensions.dpi as f32 / 72.0;
            let scale = if scale <= 0.0 { 2.0 } else { scale };

            return Some((pane_id, rel_x, rel_y, scale));
        }
    }

    None
}
```

**2b. Add method to handle webview mouse events**

```rust
/// Handle mouse events for webview panes in Browse mode.
/// Returns true if the event was consumed.
#[cfg(target_os = "macos")]
fn handle_webview_mouse_event(&mut self, event: &MouseEvent) -> bool {
    use ::window::MouseEventKind as WMEK;
    use ::window::MousePress;

    let (pane_id, rel_x, rel_y, scale) = match self.mouse_over_webview(event) {
        Some(info) => info,
        None => return false,
    };

    // Convert to logical (CEF DIP) coordinates
    let cef_x = (rel_x / scale) as i32;
    let cef_y = (rel_y / scale) as i32;

    let xpc_manager = match crate::termwindow::webview_xpc::get_xpc_manager() {
        Some(m) => m,
        None => return false,
    };

    match &event.kind {
        WMEK::Move => {
            xpc_manager.send_mouse_move(pane_id, cef_x, cef_y, 0);
            true
        }
        WMEK::Press(MousePress::Left) => {
            xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, false, 1, 0);
            true
        }
        WMEK::Release(MousePress::Left) => {
            xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, true, 1, 0);
            true
        }
        _ => false, // Let other events pass through for now
    }
}
```

**2c. Add early intercept in mouse_event_impl**

At the start of `mouse_event_impl`, after getting the pane:

```rust
pub fn mouse_event_impl(&mut self, event: MouseEvent, context: &dyn WindowOps) {
    log::trace!("{:?}", event);
    let pane = match self.get_active_pane_or_overlay() {
        Some(pane) => pane,
        None => return,
    };

    // Check for webview mouse event (issue 319)
    #[cfg(target_os = "macos")]
    if self.handle_webview_mouse_event(&event) {
        return; // Event consumed by webview
    }

    self.current_mouse_event.replace(event.clone());
    // ... rest of existing code
```

### Part 3: CEF Mouse Event Handling (main.rs)

**3a. Add XPC message handlers in the event handler**

In `create_browser_on_ui_thread`, add cases for mouse events:

```rust
"mouse_move" => {
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        return;
    };

    let x = msg.get_i64("x") as i32;
    let y = msg.get_i64("y") as i32;
    let modifiers = msg.get_i64("modifiers") as u32;

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = MouseMoveTask::new(bs, x, y, modifiers);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
"mouse_click" => {
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        return;
    };

    let x = msg.get_i64("x") as i32;
    let y = msg.get_i64("y") as i32;
    let button = msg.get_i64("button") as u32;
    let is_up = msg.get_bool("is_up");
    let click_count = msg.get_i64("click_count") as i32;
    let modifiers = msg.get_i64("modifiers") as u32;

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = MouseClickTask::new(bs, x, y, button, is_up, click_count, modifiers);
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
```

**3b. Add MouseMoveTask**

```rust
// ====== Mouse Move Task ======
//
// Task for sending mouse move events to CEF on the UI thread.
// Issue 319, experiment 1.

wrap_task! {
    pub struct MouseMoveTask {
        state: Arc<BrowserState>,
        x: i32,
        y: i32,
        modifiers: u32,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                if let Some(host) = browser.host() {
                    let mouse_event = cef::MouseEvent {
                        x: self.x,
                        y: self.y,
                        modifiers: self.modifiers,
                    };
                    // mouse_leave = false (mouse is over the view)
                    host.send_mouse_move_event(Some(&mouse_event), 0);
                }
            }
        }
    }
}
```

**3c. Add MouseClickTask**

```rust
// ====== Mouse Click Task ======
//
// Task for sending mouse click events to CEF on the UI thread.
// Issue 319, experiment 1.

wrap_task! {
    pub struct MouseClickTask {
        state: Arc<BrowserState>,
        x: i32,
        y: i32,
        button: u32,
        is_up: bool,
        click_count: i32,
        modifiers: u32,
    }

    impl Task {
        fn execute(&self) {
            if let Some(browser) = self.state.browser.lock().unwrap().as_ref() {
                if let Some(host) = browser.host() {
                    let mouse_event = cef::MouseEvent {
                        x: self.x,
                        y: self.y,
                        modifiers: self.modifiers,
                    };
                    // button: 0=left, 1=middle, 2=right (CEF MouseButtonType)
                    let button_type = match self.button {
                        0 => cef::MouseButtonType::MBT_LEFT,
                        1 => cef::MouseButtonType::MBT_MIDDLE,
                        2 => cef::MouseButtonType::MBT_RIGHT,
                        _ => cef::MouseButtonType::MBT_LEFT,
                    };
                    let mouse_up = if self.is_up { 1 } else { 0 };
                    host.send_mouse_click_event(
                        Some(&mouse_event),
                        button_type,
                        mouse_up,
                        self.click_count,
                    );
                }
            }
        }
    }
}
```

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Hover effects
web google.com
# Move mouse over search button
# Expected: Cursor changes, hover effects visible

# Test 2: Click links
web example.com
# Click the "More information..." link
# Expected: Navigates to IANA page

# Test 3: Click form elements
web google.com
# Click in search box
# Expected: Text cursor appears, can type

# Log verification
tail -f /tmp/termsurf-gui.log | grep -E "\[XPC\] Sent mouse"
tail -f /tmp/termsurf-profile-*.log | grep -i mouse
```

### Success Criteria for Experiment 1

- [ ] Mouse movement sends events to CEF (visible in logs)
- [ ] Hover over links shows pointer cursor
- [ ] Click on links navigates to URL
- [ ] Click in text fields focuses them
- [ ] Click on buttons activates them

### Known Limitations (Experiment 1)

These will be addressed in later experiments:

- No scroll wheel support
- No modifiers (Shift-click, Cmd-click)
- No click counting (double/triple click)
- No drag support (text selection)
- No right-click support
- No middle-click support

### Conclusion (Experiment 1)

**Result: Failed.** Mouse events are not being delivered to CEF.

#### What's Broken

The GUI successfully intercepts mouse events, transforms coordinates, and calls
`send_command()` via XPC. The connection appears valid (no "No connection for pane"
warnings). However, the profile server never receives the mouse events — zero
`mouse_move` or `mouse_click` actions appear in the profile logs.

Observed behavior: hover highlights appear briefly then disappear, clicks work once
then stop. This suggests messages may be delivered initially but the connection
enters a broken state where `send()` silently fails.

Confusingly, keyboard input uses the identical `send_command()` path and works
reliably. The XPC connection works in both directions for other message types:

| Direction         | Message Type      | Status  |
| ----------------- | ----------------- | ------- |
| Profile → GUI     | `display_surface` | Works   |
| Launcher → Profile| `create_browser`  | Works   |
| GUI → Profile     | `key_event`       | Works   |
| GUI → Profile     | `mouse_move`      | Broken  |
| GUI → Profile     | `mouse_click`     | Broken  |

The profile logs show repeated "XPC connection interrupted" errors and unexpected
`create_browser` commands, suggesting connection instability that may be related.

#### Ideas for Fixing

1. **Debug XPC connection state**: Add logging to verify the connection stored in
   `peer_connections` is the same object the profile has its event handler on.
   Multiple reconnections may cause GUI to send on a connection the profile isn't
   listening to.

2. **Verify event handler registration**: Confirm the profile's event handler for
   `mouse_move`/`mouse_click` is actually registered. Add a catch-all log in the
   handler's `_ => {}` branch to see if messages arrive with unexpected action names.

3. **Test with synchronous reply**: Use `send_with_reply_sync()` instead of `send()`
   for mouse events temporarily. If this works, the issue is with async message
   delivery. If it fails, we'll get an actual error message.

4. **Compare with keyboard path**: Trace exactly what happens for a keyboard event
   vs a mouse event. Find where the paths diverge.

5. **Check for connection replacement**: The logs show many "New connection for
   session" messages after errors. If the GUI stores a new connection but the
   profile's event handler is on the old one, messages would be lost. May need to
   re-register handlers on reconnection.

6. **Simplify**: Strip mouse handling down to the absolute minimum — send a single
   test message on click and verify it arrives. Remove all the coordinate
   transformation and throttling to isolate the core IPC issue.

## References

- `docs/issues/317-input.md` — Keyboard input (completed)
- `docs/issues/318-cmd.md` — Clipboard keybindings (completed)
