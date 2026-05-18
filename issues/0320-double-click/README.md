+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 320: Double-Click Support

Double-click to select words in webview panes.

## Status

**Complete.** Double-click and triple-click working. See Conclusion for details.

## Product Requirements

Users expect standard text selection behavior in web content:

1. **Double-click selects a word** — Clicking twice quickly on a word should
   highlight the entire word, matching browser behavior.

2. **Triple-click selects a line/paragraph** — Three rapid clicks should select
   the entire line or paragraph, depending on the element.

3. **Selection is visible** — Selected text should display with the standard
   highlight color.

4. **Selection can be extended** — After double-click selection, Shift-click
   should extend the selection to the clicked position.

## Background

### What Works (from Issue 319)

Issue 319 established basic mouse input for ts3 webviews:

| Feature                 | Status  | Implementation                          |
| ----------------------- | ------- | --------------------------------------- |
| Mouse move              | Working | `send_mouse_move()` via XPC             |
| Left click              | Working | `send_mouse_click()` via XPC            |
| Hover effects           | Working | CSS :hover triggers correctly           |
| Coordinate transform    | Working | Physical → logical with DPI scaling     |
| Control panel exclusion | Working | Clicks above webview handled separately |

### Current Click Implementation

The existing click handler in `mouseevent.rs` sends a single click with
`click_count: 1`:

```rust
// From handle_webview_mouse_event()
xpc_manager.send_mouse_click(
    pane_id,
    cef_x,
    cef_y,
    scale,
    true,  // is_press
);
```

CEF's `send_mouse_click_event` accepts a `click_count` parameter that determines
selection behavior:

| click_count | CEF Behavior                  |
| ----------- | ----------------------------- |
| 1           | Position cursor, no selection |
| 2           | Select word under cursor      |
| 3           | Select line/paragraph         |

### Architecture Reference

```
Mouse Click Flow:

User double-clicks
    │
    ▼
Window System (two rapid MousePress events)
    │
    ▼
mouse_event_impl() in mouseevent.rs
    │
    ▼
handle_webview_mouse_event()
    │
    ├─ [NEEDED] Track click timing
    ├─ [NEEDED] Count rapid clicks (1, 2, or 3)
    │
    └─ xpc_manager.send_mouse_click(... click_count)
            │
            ▼
        XPC to Profile Server
            │
            ▼
        CEF send_mouse_click_event(click_count)
            │
            ▼
        Word/line selection based on count
```

## Implementation Approach

### Click Counting Logic

Track recent clicks to detect double/triple clicks:

1. **Store last click info** — Position (x, y) and timestamp
2. **On new click** — Check if within time threshold (~500ms) and position
   threshold (~5 pixels)
3. **Increment or reset** — If thresholds met, increment count (max 3);
   otherwise reset to 1
4. **Send to CEF** — Pass computed click_count with the click event

### State Requirements

Need to track per-pane:

- Last click timestamp
- Last click position (x, y)
- Current click count (1, 2, or 3)

### Threshold Values

Standard double-click thresholds:

- **Time**: 500ms (typical OS default)
- **Distance**: 5 pixels (allow slight movement between clicks)

## Success Criteria

- [x] Double-click selects word
- [x] Triple-click selects line/paragraph
- [x] Click count resets after timeout
- [x] Click count resets if mouse moves too far
- [x] Selection highlight is visible

## Next Steps (Other Mouse Input)

After double-click, these features remain for full mouse support:

| Feature         | Priority | Notes                                      |
| --------------- | -------- | ------------------------------------------ |
| Scroll wheel    | High     | `send_mouse_wheel()`, delta × 120 for CEF  |
| Trackpad scroll | High     | Same as wheel, may need gesture handling   |
| Drag selection  | Medium   | Track button state across moves            |
| Modifier keys   | Medium   | Shift-click, Cmd-click, Ctrl-click         |
| Right-click     | Medium   | Context menu or forward to CEF             |
| Middle-click    | Low      | Paste or open in new tab                   |
| Cursor feedback | Low      | CEF → GUI reverse channel for cursor shape |

## Experiments

### Experiment 1: Click Counting State

**Status:** SUCCESS

**Hypothesis:** Adding click timing/position tracking to TermWindow and
computing click_count based on thresholds will enable CEF to receive
double/triple clicks.

**Approach:** The XPC infrastructure already passes `click_count` to CEF — it's
just hardcoded to 1. Add per-pane click state and compute the count dynamically.

#### 1a. Add Click State Struct

In `mouseevent.rs`, add a struct to track click history:

```rust
/// State for tracking multi-click sequences (double-click, triple-click)
#[derive(Debug, Clone)]
pub struct ClickState {
    /// Timestamp of last click
    pub last_time: std::time::Instant,
    /// Position of last click (CEF coordinates)
    pub last_pos: (i32, i32),
    /// Current click count (1, 2, or 3)
    pub count: u32,
}

impl Default for ClickState {
    fn default() -> Self {
        Self {
            last_time: std::time::Instant::now(),
            last_pos: (0, 0),
            count: 0,
        }
    }
}
```

#### 1b. Add State to TermWindow

In `mod.rs`, add a field to TermWindow:

```rust
/// Per-pane click state for double/triple-click detection
click_state: RefCell<HashMap<PaneId, ClickState>>,
```

Initialize in `new_window()`:

```rust
click_state: RefCell::new(HashMap::new()),
```

#### 1c. Implement Click Counting Function

In `mouseevent.rs`, add a method to compute click count:

```rust
impl super::TermWindow {
    /// Compute click count based on timing and position.
    /// Returns 1, 2, or 3 depending on rapid successive clicks.
    fn compute_click_count(&self, pane_id: PaneId, x: i32, y: i32) -> u32 {
        use std::time::{Duration, Instant};

        const DOUBLE_CLICK_TIME: Duration = Duration::from_millis(500);
        const DOUBLE_CLICK_DISTANCE: i32 = 5;

        let mut states = self.click_state.borrow_mut();
        let state = states.entry(pane_id).or_default();

        let now = Instant::now();
        let elapsed = now.duration_since(state.last_time);
        let dx = (x - state.last_pos.0).abs();
        let dy = (y - state.last_pos.1).abs();

        let new_count = if elapsed < DOUBLE_CLICK_TIME
            && dx <= DOUBLE_CLICK_DISTANCE
            && dy <= DOUBLE_CLICK_DISTANCE
        {
            // Rapid click near same position: increment (max 3)
            (state.count + 1).min(3)
        } else {
            // Too slow or too far: reset to 1
            1
        };

        // Update state for next click
        state.last_time = now;
        state.last_pos = (x, y);
        state.count = new_count;

        log::info!(
            "[CLICK] pane={} pos=({},{}) elapsed={:?} distance=({},{}) count={}",
            pane_id, x, y, elapsed, dx, dy, new_count
        );

        new_count
    }
}
```

#### 1d. Use Click Count in Handler

Modify `handle_webview_mouse_event()` to use computed count:

```rust
WMEK::Press(MousePress::Left) => {
    let click_count = self.compute_click_count(pane_id, cef_x, cef_y);
    log::info!(
        "[MOUSE] Press LEFT pane={} cef=({}, {}) click_count={}",
        pane_id, cef_x, cef_y, click_count
    );
    xpc_manager.send_mouse_click(pane_id, cef_x, cef_y, 0, false, click_count as i32, 0);
    true
}
WMEK::Release(MousePress::Left) => {
    // Use same count as press (don't re-compute on release)
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

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Single click
web google.com
# Click once on text
# Expected log: "[CLICK] ... count=1"

# Test 2: Double-click
# Double-click on a word
# Expected log: "[CLICK] ... count=2"
# Expected result: Word is selected

# Test 3: Triple-click
# Triple-click on a line
# Expected log: "[CLICK] ... count=3"
# Expected result: Line/paragraph is selected

# Test 4: Slow clicks (should reset)
# Click, wait 1 second, click again
# Expected: count=1 both times

# Test 5: Distant clicks (should reset)
# Click at one position, quickly click far away
# Expected: count=1 for second click

tail -f /tmp/termsurf-gui.log | grep "\[CLICK\]"
```

#### Success Criteria

- [x] Log shows count=2 for rapid double-clicks
- [x] Log shows count=3 for rapid triple-clicks
- [x] Log shows count=1 for slow or distant clicks
- [x] Double-click selects word in webview
- [x] Triple-click selects line/paragraph in webview

---

## Conclusion

### What Was Accomplished

Double-click and triple-click text selection now works in ts3 webviews:

1. **Double-click selects word** — Two rapid clicks on a word highlight the
   entire word, matching standard browser behavior.

2. **Triple-click selects line/paragraph** — Three rapid clicks select the full
   line or paragraph element.

3. **Automatic reset** — Click count resets to 1 if more than 500ms passes
   between clicks or if the mouse moves more than 5 pixels.

### What We Learned

1. **The XPC infrastructure was already complete** — The `send_mouse_click()`
   method already accepted a `click_count` parameter; it was simply hardcoded
   to 1. No changes to XPC or CEF-side code were needed.

2. **Per-pane state is essential** — Click tracking must be per-pane because
   users may click in different webview panes. Using a global click state would
   incorrectly detect double-clicks across panes.

3. **Press computes, Release reuses** — The click count is computed on Press and
   stored. Release reuses the same count rather than recomputing, ensuring CEF
   receives matching Press/Release click counts.

### Implementation Summary

```
Click Counting Flow:

MousePress event arrives
    │
    ▼
compute_click_count(pane_id, x, y)
    │
    ├─ Lookup ClickState for pane (or create default)
    │
    ├─ Check elapsed time since last click
    │   └─ If > 500ms: reset to count=1
    │
    ├─ Check distance from last click
    │   └─ If > 5px: reset to count=1
    │
    ├─ Otherwise: increment count (max 3)
    │
    └─ Store new timestamp, position, count
           │
           ▼
    Return count (1, 2, or 3)
           │
           ▼
    send_mouse_click(..., click_count)
           │
           ▼
    CEF selects nothing / word / line
```

### Files Modified

| File            | Changes                                                         |
| --------------- | --------------------------------------------------------------- |
| `mouseevent.rs` | Added `ClickState` struct, `compute_click_count()` method       |
| `mod.rs`        | Added `click_state: RefCell<HashMap<PaneId, ClickState>>` field |

### What's Next

With double-click complete, these mouse features remain:

| Feature         | Priority | Notes                                      |
| --------------- | -------- | ------------------------------------------ |
| Scroll wheel    | High     | Most requested after click/selection       |
| Drag selection  | Medium   | Click-and-drag to select text ranges       |
| Modifier keys   | Medium   | Shift-click to extend selection            |
| Right-click     | Medium   | Context menus                              |
| Cursor feedback | Low      | Change cursor shape over links, text, etc. |

Recommended next issue: **321-scroll** for scroll wheel support.

## References

- `docs/issues/0319-mouse.md` — Basic mouse input (completed)
- `docs/issues/0317-input.md` — Keyboard input (completed)
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Mouse event handling
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — XPC mouse methods
- `ts3/termsurf-profile/src/main.rs` — CEF mouse event handlers
