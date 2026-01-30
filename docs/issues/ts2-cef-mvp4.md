# CEF MVP4: Control Panel and Modes

## Goal

Restore the control panel UI and modal keybinding system from TermSurf 1.x,
adapted for CEF and WezTerm.

## Background

TermSurf 1.x has a modal system for browser panes:

- **Control mode**: Terminal keybindings work, browser is inactive
- **Browse mode**: Browser has focus, receives all input

This provides a clear UX where users always know whether their keystrokes go to
the terminal or browser.

## Requirements

### Modes

1. **Control Mode** (default when switching to browser pane)

   - Terminal keybindings work (pane navigation, splits, etc.)
   - Browser is visually dimmed
   - No input goes to the browser
   - Keybindings:
     - `Enter` → browse mode
     - `Ctrl+C` → close browser
   - Control bar displays: "Enter to browse. Ctrl+C to exit."

2. **Browse Mode**

   - Browser receives all input (keys, mouse, touch)
   - If browser doesn't use a keybinding, it passes through to WezTerm
   - Control bar displays: current URL only

### Control Bar

A horizontal bar at the top of the browser pane:

**Browse mode:**

```
┌─────────────────────────────────────────────────────────────┐
│ https://example.com/path/to/page                            │
└─────────────────────────────────────────────────────────────┘
```

**Control mode:**

```
┌─────────────────────────────────────────────────────────────┐
│ Enter to browse. Ctrl+C to exit.                            │
└─────────────────────────────────────────────────────────────┘
```

- URL truncates with ellipsis if too long
- Height: 2 cell heights (half-cell top margin + text + half-cell bottom margin)

### No Stacking

Only one browser per pane is supported. Attempting to open a second browser in
the same pane returns an error. This simplifies the implementation and avoids
confusion about which browser is active.

### Visual Feedback

- **Control mode**: Browser area is dimmed
- **Browse mode**: Control bar is dimmed (optional, may skip for MVP)
- Clicking browser area switches to browse mode
- Clicking control bar switches to control mode

## Technical Approach

### 1. Control Bar Rendering

The control bar is rendered in two phases to avoid wgpu buffer conflicts:

1. **Background**: Rendered via `filled_rectangle` during `paint_pane` (while
   layers buffer is mapped)
2. **Text**: Rendered via `render_element` in `paint_browser_control_bars`
   (after layers buffer is dropped)

This matches the pattern used by `paint_modal`.

### 2. Mode State

Mode tracking in `BrowserState`:

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum BrowserMode {
    Control,
    Browse,
}

pub struct BrowserState {
    // ... existing fields ...
    mode: RefCell<BrowserMode>,
    url: RefCell<String>,
}
```

### 3. Key Event Routing

In `keyevent.rs`, the CEF browser key handling section needs mode-aware logic:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
{
    let pane_id = pane.pane_id();
    if let Some(browser) = self.browser_states.borrow().get(&pane_id) {
        match browser.get_mode() {
            BrowserMode::Control => {
                // Handle Enter → browse mode
                // Handle Ctrl+C → close browser
                // Other keys: process as terminal keybindings
            }
            BrowserMode::Browse => {
                // Forward all input to CEF
                // If CEF doesn't handle it, fall through to WezTerm
            }
        }
    }
}
```

### 4. CEF Input Forwarding

In browse mode, forward all input to CEF:

```rust
impl BrowserState {
    pub fn forward_key_event(&self, event: &KeyEvent) -> bool {
        // Convert WezTerm KeyEvent to CEF KeyEvent
        // Send via browser.host().send_key_event()
        // Return true if CEF handled it, false to pass through
    }
}
```

### 5. Click Handling

Mouse clicks need mode-aware routing:

- Click on browser area → switch to browse mode, forward click to CEF
- Click on control bar → switch to control mode

### 6. Single Browser Enforcement

When opening a browser in a pane that already has one:

```rust
pub fn open_browser_in_pane(&mut self, pane_id: PaneId, url: &str) -> Result<()> {
    if self.browser_states.borrow().contains_key(&pane_id) {
        return Err(anyhow!("Pane already has a browser open"));
    }
    // ... create browser ...
}
```

## Implementation Plan

### Phase 1: Mode State and Basic Switching (Done)

1. Add `BrowserMode` enum and state to `BrowserState`
2. Update key handling in `keyevent.rs` for mode switching
3. Add mode-based key routing (control: Enter/Ctrl+C)

### Phase 2: Control Bar Rendering (Done)

1. Calculate control bar bounds (top of pane, 2 cell heights)
2. Render control bar background (during paint_pane)
3. Render control bar text (after layers dropped)
4. Adjust browser viewport to exclude control bar height

### Phase 3: Control Bar Text Content

1. In browse mode: display current URL
2. In control mode: display "Enter to browse. Ctrl+C to exit."

### Phase 4: Visual Feedback

1. Render dim overlay on browser area in control mode
2. Implement click detection for mode switching

### Phase 5: CEF Input Forwarding

1. Forward key events to CEF in browse mode
2. Forward mouse events to CEF in browse mode
3. Handle pass-through for unhandled keys

## Key Differences from TS1

| Aspect          | TS1 (Swift/WKWebView)            | TS2 (Rust/CEF)                     |
| --------------- | -------------------------------- | ---------------------------------- |
| Event monitor   | NSEvent.addLocalMonitorForEvents | WezTerm key_event_impl             |
| First responder | NSView responder chain           | WezTerm pane focus                 |
| Text rendering  | NSTextField                      | WezTerm box model (render_element) |
| Dim overlay     | NSView with alpha                | wgpu quad with alpha               |

## Non-Goals for MVP4

- Insert mode (URL editing)
- Multiple browser stack
- Navigation controls (back/forward buttons)
- Tab completion for URLs

## Success Criteria

The implementation is complete when:

- [x] Control bar renders with background and text
- [x] Browse mode displays current URL
- [ ] Control mode displays "Enter to browse. Ctrl+C to exit."
- [ ] Pressing Enter in control mode switches to browse mode
- [ ] Pressing Ctrl+C in control mode closes the browser
- [ ] In browse mode, input goes to browser
- [ ] Unhandled browser keys pass through to WezTerm
- [ ] Clicking browser area switches to browse mode
- [ ] Clicking control bar switches to control mode
- [ ] Browser area is dimmed in control mode
- [ ] Opening second browser in same pane returns error
