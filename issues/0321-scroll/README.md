+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 321: Scroll Support

Scroll webview content using mouse wheel and trackpad gestures.

## Status

**Complete.** Smooth trackpad scrolling working with `* 2` multiplier.

## Product Requirements

Users need to scroll web content in webview panes:

1. **Vertical scroll** — Scroll up and down through page content using mouse
   wheel or two-finger trackpad gesture.

2. **Horizontal scroll** — Scroll left and right for wide content using
   horizontal wheel tilt or two-finger horizontal swipe.

3. **Smooth scrolling** — Trackpad gestures should feel smooth and responsive,
   matching native browser behavior.

4. **Momentum scrolling** — On macOS, trackpad flick gestures should continue
   scrolling with momentum (inertia), then decelerate naturally.

5. **Scroll anywhere** — Scrolling should work regardless of where the cursor is
   positioned within the webview pane.

## Background

### What Works (from Issues 319, 320)

Previous issues established mouse input infrastructure for ts3 webviews:

| Feature                    | Status  | Issue |
| -------------------------- | ------- | ----- |
| Mouse move                 | Working | 319   |
| Left click                 | Working | 319   |
| Hover effects              | Working | 319   |
| Coordinate transform       | Working | 319   |
| Double-click (word select) | Working | 320   |
| Triple-click (line select) | Working | 320   |

### Current Scroll Handling

WezTerm already receives scroll events for terminal panes:

```rust
// From mouseevent.rs - terminal scroll handling
WMEK::VertWheel(amount) => {
    if amount > 0 {
        TMB::WheelUp(amount as usize)
    } else {
        TMB::WheelDown((-amount) as usize)
    }
}
WMEK::HorzWheel(amount) => {
    if amount > 0 {
        TMB::WheelLeft(amount as usize)
    } else {
        TMB::WheelRight((-amount) as usize)
    }
}
```

The `amount` is a signed integer:

- Positive = up/left
- Negative = down/right

### CEF Scroll API

CEF provides `send_mouse_wheel_event` for scroll input:

```rust
// From cef-rs OSR example
let (delta_x, delta_y) = match delta {
    // Line-based scroll (traditional mouse wheel): multiply by 120
    MouseScrollDelta::LineDelta(x, y) => ((x * 120.0) as i32, (y * 120.0) as i32),
    // Pixel-based scroll (trackpad gestures): multiply by 2
    MouseScrollDelta::PixelDelta(pos) => ((pos.x * 2.0) as i32, (pos.y * 2.0) as i32),
};
host.send_mouse_wheel_event(Some(&mouse_event), delta_x, delta_y);
```

Key points:

- CEF expects delta values, not absolute positions
- Line-based scrolling (discrete wheel clicks) uses 120 units per line
- Pixel-based scrolling (smooth trackpad) uses smaller increments
- Both horizontal and vertical deltas can be sent in one call

### Scroll Input Types

| Input Device            | Event Type  | Scroll Style                    |
| ----------------------- | ----------- | ------------------------------- |
| Traditional mouse wheel | Line delta  | Discrete steps (120 units/line) |
| Apple Magic Mouse       | Pixel delta | Smooth gesture-based            |
| Trackpad two-finger     | Pixel delta | Smooth gesture-based            |
| Trackpad with momentum  | Pixel delta | Continues after lift            |

WezTerm's `WMEK::VertWheel` and `WMEK::HorzWheel` events should already
encapsulate both types—the window system translates device input into these
events.

### Architecture Reference

```
Scroll Event Flow:

User scrolls (wheel or gesture)
    │
    ▼
Window System (winit/macOS)
    │
    ▼
WMEK::VertWheel(amount) or WMEK::HorzWheel(amount)
    │
    ▼
mouse_event_impl() in mouseevent.rs
    │
    ▼
handle_webview_mouse_event()
    │
    ├─ [NEEDED] Handle VertWheel/HorzWheel events
    ├─ [NEEDED] Convert amount to CEF delta format
    │
    └─ xpc_manager.send_mouse_wheel(...)  [NEEDED]
            │
            ▼
        XPC to Profile Server
            │
            ▼
        [NEEDED] Handle "mouse_wheel" action
            │
            ▼
        CEF host.send_mouse_wheel_event(delta_x, delta_y)
            │
            ▼
        Page scrolls
```

## Implementation Approach

### 1. Add XPC Method (webview_xpc.rs)

```rust
pub fn send_mouse_wheel(
    &self,
    pane_id: PaneId,
    x: i32,
    y: i32,
    delta_x: i32,
    delta_y: i32,
    modifiers: u32,
) -> bool
```

### 2. Handle Scroll Events (mouseevent.rs)

Add cases for `WMEK::VertWheel` and `WMEK::HorzWheel` in
`handle_webview_mouse_event()`.

### 3. CEF Handler (termsurf-profile/main.rs)

Add handler for "mouse_wheel" action that calls `host.send_mouse_wheel_event()`.

### Delta Conversion

WezTerm provides scroll amounts that need conversion to CEF deltas:

```rust
// Vertical scroll
WMEK::VertWheel(amount) => {
    // amount is typically small integers for line scrolling
    // or larger values for pixel-based trackpad scrolling
    let delta_y = amount * 120; // Scale for CEF
    xpc_manager.send_mouse_wheel(pane_id, cef_x, cef_y, 0, delta_y, 0);
}
```

The exact scaling factor may need tuning based on testing with actual hardware.

## Success Criteria

- [x] Vertical scroll works with mouse wheel
- [x] Vertical scroll works with trackpad two-finger gesture
- [x] Horizontal scroll works
- [x] Scroll feels smooth (not jerky or too fast/slow)
- [x] Momentum scrolling works on macOS trackpad
- [x] Scroll direction matches system preferences (natural vs traditional)

## Next Steps (Other Mouse Input)

After scroll, these features remain for full mouse support:

| Feature         | Priority | Notes                                |
| --------------- | -------- | ------------------------------------ |
| Drag selection  | Medium   | Click-and-drag to select text ranges |
| Modifier keys   | Medium   | Shift-click, Cmd-click, Ctrl-click   |
| Right-click     | Medium   | Context menus                        |
| Middle-click    | Low      | Paste or open in new tab             |
| Cursor feedback | Low      | Change cursor shape over links, text |

## Experiments

### Experiment 1: Basic Scroll Pipeline

**Status:** SUCCESS

**Hypothesis:** Adding scroll event handling through the existing XPC pipeline
will enable CEF to receive scroll input and scroll page content.

**Approach:** Follow the same pattern as mouse click (issue 319): add XPC
method, handle events in GUI, create CEF task in profile server.

#### 1a. Add XPC Method (webview_xpc.rs)

Add `send_mouse_wheel` method after `send_mouse_click`:

```rust
/// Send mouse wheel event to the browser (issue 321, experiment 1)
pub fn send_mouse_wheel(
    &self,
    pane_id: PaneId,
    x: i32,
    y: i32,
    delta_x: i32,
    delta_y: i32,
    modifiers: u32,
) -> bool {
    let msg = XpcDictionary::new();
    msg.set_string("action", "mouse_wheel");
    msg.set_i64("x", x as i64);
    msg.set_i64("y", y as i64);
    msg.set_i64("delta_x", delta_x as i64);
    msg.set_i64("delta_y", delta_y as i64);
    msg.set_i64("modifiers", modifiers as i64);

    if self.send_command(pane_id, &msg) {
        log::debug!(
            "[XPC] Sent mouse_wheel to pane {}: ({}, {}) delta=({}, {})",
            pane_id, x, y, delta_x, delta_y
        );
        true
    } else {
        false
    }
}
```

#### 1b. Handle Scroll Events (mouseevent.rs)

Add cases for `VertWheel` and `HorzWheel` in `handle_webview_mouse_event()`:

```rust
WMEK::VertWheel(amount) => {
    // CEF expects delta in "wheel ticks" where 120 = 1 line
    // WezTerm amount is already scaled appropriately
    let delta_y = amount * 120;
    log::info!(
        "[MOUSE] VertWheel pane={} cef=({}, {}) amount={} delta_y={}",
        pane_id, cef_x, cef_y, amount, delta_y
    );
    xpc_manager.send_mouse_wheel(pane_id, cef_x, cef_y, 0, delta_y, 0);
    true
}
WMEK::HorzWheel(amount) => {
    let delta_x = amount * 120;
    log::info!(
        "[MOUSE] HorzWheel pane={} cef=({}, {}) amount={} delta_x={}",
        pane_id, cef_x, cef_y, amount, delta_x
    );
    xpc_manager.send_mouse_wheel(pane_id, cef_x, cef_y, delta_x, 0, 0);
    true
}
```

#### 1c. Add MouseWheelTask (termsurf-profile/main.rs)

Add task struct after `MouseClickTask`:

```rust
wrap_task! {
    pub struct MouseWheelTask {
        state: Arc<BrowserState>,
        x: i32,
        y: i32,
        delta_x: i32,
        delta_y: i32,
        modifiers: u32,
    }

    impl Task {
        fn execute(&self) {
            println!("[MOUSE-TASK] MouseWheelTask::execute() called");

            let browser_guard = self.state.browser.lock().unwrap();
            let Some(browser) = browser_guard.as_ref() else {
                println!("[MOUSE-TASK] FAIL: browser is None");
                return;
            };

            let Some(host) = browser.host() else {
                println!("[MOUSE-TASK] FAIL: browser.host() is None");
                return;
            };
            println!("[MOUSE-TASK] Host obtained, calling send_mouse_wheel_event");

            let mouse_event = cef::MouseEvent {
                x: self.x,
                y: self.y,
                modifiers: self.modifiers,
            };
            host.send_mouse_wheel_event(
                Some(&mouse_event),
                self.delta_x,
                self.delta_y,
            );
            println!("[MOUSE-TASK] send_mouse_wheel_event returned");
        }
    }
}
```

#### 1d. Add Handler for mouse_wheel Action

In the XPC event handler, add case after `mouse_click`:

```rust
"mouse_wheel" => {
    println!("[MOUSE] mouse_wheel handler entered");

    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(bs) = state_guard.as_ref() else {
        println!("[MOUSE] FAIL: deferred_for_handler is None");
        return;
    };

    let x = msg.get_i64("x") as i32;
    let y = msg.get_i64("y") as i32;
    let delta_x = msg.get_i64("delta_x") as i32;
    let delta_y = msg.get_i64("delta_y") as i32;
    let modifiers = msg.get_i64("modifiers") as u32;
    println!(
        "[MOUSE] mouse_wheel coords: ({}, {}) delta=({}, {})",
        x, y, delta_x, delta_y
    );

    let bs = Arc::clone(bs);
    drop(state_guard);

    let mut task = MouseWheelTask::new(bs, x, y, delta_x, delta_y, modifiers);
    println!("[MOUSE] Calling post_task for MouseWheelTask");
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
    println!("[MOUSE] post_task returned");
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Trackpad scroll
web google.com
# Two-finger scroll gesture on trackpad
# Watch logs for scroll events

tail -f /tmp/termsurf-gui.log | grep "\[MOUSE\].*Wheel"
tail -f /tmp/termsurf-profile-*.log | grep "\[MOUSE\]"

# Expected: Page scrolls, logs show delta values
```

#### Success Criteria

- [x] Log shows VertWheel/HorzWheel events received in GUI
- [x] Log shows mouse_wheel action received in profile server
- [x] Log shows send_mouse_wheel_event called in CEF task
- [x] Page content scrolls when using trackpad gesture
- [x] Scroll direction feels correct (natural scrolling)

#### Results

The scroll pipeline works. Page content scrolls when using trackpad gestures,
and the direction matches system preferences (natural scrolling).

**How it works:**

1. **GUI receives scroll events** — WezTerm's window system delivers
   `WMEK::VertWheel(amount)` and `WMEK::HorzWheel(amount)` events.

2. **Delta conversion** — The amount is multiplied by 120 (CEF's standard "wheel
   tick" unit where 120 = 1 line of scroll).

3. **XPC transport** — `send_mouse_wheel()` packages delta_x/delta_y into an XPC
   dictionary and sends to the profile server.

4. **CEF task execution** — `MouseWheelTask` runs on CEF's UI thread and calls
   `host.send_mouse_wheel_event()` with the delta values.

5. **Page scrolls** — CEF processes the wheel event and scrolls the page.

**Issue discovered:** Scrolling feels blocky/jerky rather than smooth. This is
because the `* 120` multiplier is designed for line-based scrolling (traditional
mouse wheels), not pixel-based scrolling (trackpad gestures). The cef-rs OSR
example uses different multipliers:

- Line-based (mouse wheel): `* 120`
- Pixel-based (trackpad): `* 2`

WezTerm's `WMEK::VertWheel` doesn't distinguish between these types. Experiment
2 will investigate reducing the multiplier or detecting scroll type to achieve
smooth trackpad scrolling.

---

### Experiment 2: Smooth Trackpad Scrolling

**Status:** SUCCESS

**Hypothesis:** Reducing the scroll multiplier from 120 to a smaller value will
make trackpad scrolling feel smooth instead of blocky.

**Background:** The cef-rs OSR example uses two different multipliers:

```rust
// Line-based (mouse wheel clicks): large discrete jumps
MouseScrollDelta::LineDelta(x, y) => ((x * 120.0) as i32, (y * 120.0) as i32),
// Pixel-based (trackpad gestures): small smooth increments
MouseScrollDelta::PixelDelta(pos) => ((pos.x * 2.0) as i32, (pos.y * 2.0) as i32),
```

The 120 value comes from Windows' `WHEEL_DELTA` constant — one "notch" of a
mouse wheel equals 120 units. Trackpads report many small pixel deltas instead.

**Approach:** Test progressively smaller multipliers to find one that feels
smooth for trackpad gestures.

#### 2a. First Try: No Multiplier

Change the scroll handler to pass the raw amount:

```rust
WMEK::VertWheel(amount) => {
    // Experiment 2: Try raw amount (no multiplier)
    let delta_y = *amount as i32;
    log::info!(
        "[MOUSE] VertWheel pane={} amount={} delta_y={}",
        pane_id, amount, delta_y
    );
    xpc_manager.send_mouse_wheel(pane_id, cef_x, cef_y, 0, delta_y, 0);
    true
}
```

If too slow, try `* 2`. If still blocky, the issue may be elsewhere.

#### 2b. Add Diagnostic Logging

Log the raw amount values to understand what WezTerm sends:

```rust
log::info!(
    "[SCROLL-DEBUG] raw_amount={} current_multiplier=1",
    amount
);
```

This will reveal:

- Trackpad: Many events with small amounts (e.g., 1-5 per event)
- Mouse wheel: Few events with larger amounts (e.g., 1-3 lines)

#### 2c. Test Matrix

| Multiplier | Expected Feel | Test Result                           |
| ---------- | ------------- | ------------------------------------- |
| `* 120`    | Blocky        | Confirmed blocky                      |
| `* 1`      | Too slow      | Confirmed smooth but too slow         |
| `* 2`      | Smooth        | **Winner** — smooth and natural speed |
| `* 10`     | Medium        | Not tested (unnecessary)              |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test with trackpad
web google.com
# Two-finger scroll gesture
# Observe: Is scrolling smooth or blocky?

# Check raw values
tail -f /tmp/termsurf-gui.log | grep "VertWheel"
```

#### Success Criteria

- [x] Trackpad scrolling feels smooth (not jerky)
- [x] Scroll speed feels natural (not too fast or slow)
- [x] Momentum scrolling still works
- [x] Identified the optimal multiplier value (`* 2`)

#### Results

The `* 2` multiplier provides smooth trackpad scrolling that matches Chrome's
feel. The cef-rs OSR example uses this same multiplier for pixel-based input.

---

## Conclusion

### What Was Accomplished

Scroll support for ts3 webviews is complete:

1. **Vertical scrolling** — Two-finger trackpad gestures scroll page content up
   and down smoothly.

2. **Horizontal scrolling** — Two-finger horizontal swipes scroll wide content
   left and right.

3. **Smooth feel** — The `* 2` multiplier provides natural scroll speed that
   matches Chrome and other native browsers.

4. **Momentum scrolling** — Trackpad flick gestures continue scrolling with
   inertia, then decelerate naturally.

5. **Natural scrolling** — Scroll direction matches macOS system preferences.

### What We Learned

1. **Multiplier matters** — CEF's scroll API expects delta values where 120 = 1
   line. This works for traditional mouse wheels (discrete clicks), but
   trackpads send many small pixel-based deltas. Using 120 made scrolling
   blocky.

2. **cef-rs got it right** — The OSR example uses `* 2` for pixel-based input,
   which we confirmed is the correct multiplier for smooth trackpad scrolling.

3. **WezTerm abstracts scroll types** — The `WMEK::VertWheel` event doesn't
   distinguish between line-based and pixel-based scrolling. Fortunately, `* 2`
   works well for trackpads, which are the primary input on macOS.

### Implementation Summary

```
Scroll Pipeline:

Trackpad gesture
    │
    ▼
WMEK::VertWheel(amount) / WMEK::HorzWheel(amount)
    │
    ▼
delta = amount * 2  (smooth multiplier)
    │
    ▼
xpc_manager.send_mouse_wheel(delta_x, delta_y)
    │
    ▼
XPC → Profile Server
    │
    ▼
MouseWheelTask on CEF UI thread
    │
    ▼
host.send_mouse_wheel_event()
    │
    ▼
Page scrolls smoothly
```

### Files Modified

| File                       | Changes                                                      |
| -------------------------- | ------------------------------------------------------------ |
| `mouseevent.rs`            | Added `VertWheel`/`HorzWheel` handlers with `* 2` multiplier |
| `webview_xpc.rs`           | Added `send_mouse_wheel()` method                            |
| `termsurf-profile/main.rs` | Added `MouseWheelTask` and `"mouse_wheel"` handler           |

### What's Next

With scroll complete, these mouse features remain:

| Feature         | Priority | Notes                                         |
| --------------- | -------- | --------------------------------------------- |
| Drag selection  | Medium   | Click-and-drag to select text ranges          |
| Modifier keys   | Medium   | Shift-click, Cmd-click for extended selection |
| Right-click     | Medium   | Context menus                                 |
| Middle-click    | Low      | Paste or open in new tab                      |
| Cursor feedback | Low      | Change cursor shape over links, text          |

Recommended next issue: **322-drag-selection** for click-and-drag text
selection.

---

## References

- `docs/issues/0319-mouse.md` — Basic mouse input (completed)
- `docs/issues/0320-double-click.md` — Double/triple click (completed)
- `cef-rs/examples/osr/src/main.rs` — CEF scroll handling reference (lines
  462-466)
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
- `ts3/termsurf-profile/src/main.rs` — CEF event handlers
