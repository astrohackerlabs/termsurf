# Issue 731: Wezboard scroll crashes Roamium

## Goal

Fix scrolling inside Wezboard's browser overlay — currently any scroll event
causes Roamium (the Chromium browser engine) to crash, making the webview
vanish.

## Background

### Symptom

In Wezboard, loading a page works fine. As soon as the user scrolls (trackpad or
mouse wheel), the webview disappears. The Roamium process crashes.

### How Ghostboard handles scroll (working)

Ghostboard captures raw macOS `NSEvent` scroll data in Swift
(`SurfaceView_AppKit.swift:1002-1017`), stores it in the Zig `CoreSurface`
struct (`Surface.zig:70-78`), and forwards it via protobuf
(`xpc.zig:1236-1268`).

Key details from the working implementation:

- **Raw NSEvent phases** are passed through. `NSEventPhase` values are bitmask
  flags: `none=0`, `began=1`, `changed=2`, `stationary=4`, `ended=8`,
  `cancelled=16`, `mayBegin=32`.
- **`precise`** is set from `NSEvent.hasPreciseScrollingDeltas` — true for
  trackpad, false for mouse wheel.
- **`delta_x`/`delta_y`** are raw `NSEvent.scrollingDeltaX/Y` values (pixels for
  trackpad, lines for mouse wheel).

### How Wezboard handles scroll (broken)

Wezboard's `input.rs:192-208` handles `VertWheel(delta)`:

```rust
WMEK::VertWheel(delta) => {
    send_to_chromium(
        &pane_id_str,
        Msg::ScrollEvent(proto::ScrollEvent {
            tab_id: 0,
            x: rel_x,
            y: rel_y,
            delta_x: 0.0,
            delta_y: *delta as f64,
            phase: 4,
            momentum_phase: 0,
            precise: false,
            modifiers: mods,
        }),
    );
}
```

### The phase value mismatch

The C API header documents phase values as
`0=none, 1=began, 2=changed, 3=ended`. The Chromium implementation
(`ForwardScrollEvent` in `ts_browser_main_parts.cc`) casts phase directly to
`blink::WebMouseWheelEvent::Phase`:

```cpp
wheel_event.phase =
    static_cast<blink::WebMouseWheelEvent::Phase>(phase);
```

Blink's `WebMouseWheelEvent::Phase` enum:

| Value | Name             |
| ----- | ---------------- |
| 0     | kPhaseNone       |
| 1     | kPhaseBegan      |
| 2     | kPhaseStationary |
| 4     | kPhaseChanged    |
| 8     | kPhaseEnded      |
| 16    | kPhaseCancelled  |
| 32    | kPhaseMayBegin   |

These are **bitmask values**, matching macOS `NSEventPhase` exactly. So the C
API header comment (`0=none, 1=began, 2=changed, 3=ended`) is **wrong** — the
implementation accepts raw NSEventPhase bitmask values.

Ghostboard passes raw `NSEventPhase` values and it works. Wezboard sends
`phase: 4` which is `kPhaseChanged` — not inherently wrong, but sending a single
`kPhaseChanged` without a preceding `kPhaseBegan` may confuse Chromium's scroll
state machine.

### Root cause: WezTerm's input abstraction

The real problem runs deeper than a wrong phase constant. WezTerm's input
pipeline in `window/src/os/macos/window.rs` strips raw NSEvent data before it
reaches application code. The `scroll_wheel()` handler (line ~2532):

1. Reads `hasPreciseScrollingDeltas`, `scrollingDeltaY`, `scrollingDeltaX`
2. **Does NOT read** `phase`, `momentumPhase`
3. Accumulates fractional deltas and truncates to integer lines
4. Emits `VertWheel(i16)` / `HorzWheel(i16)` — a single integer

By the time the event reaches Wezboard's termsurf code, all trackpad gesture
information is gone. Smooth scrolling becomes stepped. Momentum scrolling is
invisible. This is a terminal-centric design — scroll phases don't matter for
paging a terminal, but they're critical for browser forwarding.

The same data loss affects other input types:

| Input    | Captured                                    | Lost                                              | Severity     |
| -------- | ------------------------------------------- | ------------------------------------------------- | ------------ |
| Scroll   | deltaY/X, precision flag, modifiers         | phase, momentumPhase, raw float deltas, timestamp | **Critical** |
| Mouse    | location, buttons, modifiers                | clickCount, pressure, timestamp                   | Moderate     |
| Keyboard | characters, keyCode, modifiers, repeat flag | timestamp                                         | Low          |

### The correct approach

Ghostboard works flawlessly because it passes raw NSEvent data unaltered to
Chromium. Wezboard must do the same. Rather than working around WezTerm's
abstractions, we need to extract raw NSEvent properties at the handler level
(where the native event is still available) and pass them through to the
termsurf code.

### WezTerm input pipeline architecture

```
NSEvent (from Cocoa runtime)
    ↓
extern "C" fn handlers (window/src/os/macos/window.rs)
    scroll_wheel(), mouse_down(), key_down(), etc.
    ↓ (extract via objc2::msg_send![])
WindowEvent enum (window/src/lib.rs)
    MouseEvent { kind: VertWheel(i16), coords, modifiers }
    ↓
WindowEventSender::dispatch() callback
    ↓
TermWindow event handlers (wezboard-gui/src/termwindow/)
    mouse_event_impl(), key_event_impl(), raw_key_event_impl()
    ↓
termsurf input forwarding (wezboard-gui/src/termsurf/input.rs)
    try_forward_mouse(), try_forward_key()
```

The interception point is at the top — the `extern "C" fn` handlers in
`window/src/os/macos/window.rs`. The raw `NSEvent` is available there via
`objc2::msg_send![]`, the same mechanism already used for other properties.

## Experiments

### Experiment 1: Raw scroll events from NSEvent

#### Goal

Extract raw scroll data (phase, momentumPhase, float deltas) from NSEvents at
the handler level and pass them through to the termsurf code for Chromium
forwarding.

#### Design

**1. Add `RawScrollEvent` to `WindowEvent` enum**

File: `wezboard/window/src/lib.rs`

Add a new variant to `WindowEvent` that carries raw macOS scroll data:

```rust
pub enum WindowEvent {
    // ... existing variants ...

    /// Raw scroll event with full macOS NSEvent data, for browser forwarding.
    RawScrollEvent {
        /// Overlay-relative position (same as MouseEvent coords).
        coords: Point,
        screen_coords: ScreenPoint,
        /// Raw NSEvent.scrollingDeltaX/Y (pixels for trackpad, lines for wheel).
        delta_x: f64,
        delta_y: f64,
        /// NSEventPhase bitmask (0=none, 1=began, 4=changed, 8=ended, etc.).
        phase: u32,
        /// NSEventMomentumPhase bitmask.
        momentum_phase: u32,
        /// True if trackpad (hasPreciseScrollingDeltas).
        precise: bool,
        /// Key modifiers.
        modifiers: Modifiers,
        /// Current mouse button state.
        mouse_buttons: MouseButtons,
    },
}
```

**2. Extract raw properties in `scroll_wheel()` handler**

File: `wezboard/window/src/os/macos/window.rs`, in `scroll_wheel()` (~line 2532)

Before the existing delta accumulation logic, extract the additional NSEvent
properties and dispatch a `RawScrollEvent`:

```rust
// Extract raw scroll properties (existing code already reads these):
let precise: BOOL = unsafe { msg_send![nsevent, hasPreciseScrollingDeltas] };
let delta_y: CGFloat = unsafe { msg_send![nsevent, scrollingDeltaY] };
let delta_x: CGFloat = unsafe { msg_send![nsevent, scrollingDeltaX] };

// NEW: extract phase and momentum_phase
let phase: u32 = unsafe { msg_send![nsevent, phase] };
let momentum_phase: u32 = unsafe { msg_send![nsevent, momentumPhase] };

// Dispatch raw scroll event before any accumulation/truncation
inner.events.dispatch(WindowEvent::RawScrollEvent {
    coords: Point::new(mouse_x as isize, mouse_y as isize),
    screen_coords: ScreenPoint::new(screen_x as isize, screen_y as isize),
    delta_x: delta_x as f64,
    delta_y: delta_y as f64,
    phase,
    momentum_phase,
    precise: precise != NO,
    modifiers,
    mouse_buttons,
});

// ... existing VertWheel/HorzWheel logic continues for terminal use ...
```

**3. Handle `RawScrollEvent` in TermWindow**

File: `wezboard/wezboard-gui/src/termwindow/mod.rs`

Add a match arm in the event handler:

```rust
WindowEvent::RawScrollEvent {
    coords,
    screen_coords,
    delta_x,
    delta_y,
    phase,
    momentum_phase,
    precise,
    modifiers,
    mouse_buttons,
} => {
    // Forward to termsurf if over browser overlay
    // If consumed, skip the subsequent VertWheel/HorzWheel processing
}
```

**4. Update termsurf input forwarding**

File: `wezboard/wezboard-gui/src/termsurf/input.rs`

Add a new function `try_forward_raw_scroll()` that builds the protobuf
`ScrollEvent` from raw data — matching exactly what Ghostboard sends:

```rust
pub fn try_forward_raw_scroll(
    pane_id: usize,
    coords: Point,
    delta_x: f64,
    delta_y: f64,
    phase: u32,
    momentum_phase: u32,
    precise: bool,
    modifiers: Modifiers,
) -> bool {
    let pane_id_str = pane_id.to_string();
    if let Some((rel_x, rel_y)) = hit_test_overlay(&pane_id_str, coords) {
        let mods = modifiers_to_termsurf(modifiers);
        send_to_chromium(
            &pane_id_str,
            Msg::ScrollEvent(proto::ScrollEvent {
                tab_id: 0,
                x: rel_x,
                y: rel_y,
                delta_x,
                delta_y,
                phase: phase as u64,
                momentum_phase: momentum_phase as u64,
                precise,
                modifiers: mods,
            }),
        );
        return true;
    }
    false
}
```

**5. Skip old VertWheel/HorzWheel for browser overlay**

When `RawScrollEvent` is consumed by the browser overlay, the subsequent
`VertWheel`/`HorzWheel` event (dispatched by the same `scroll_wheel()` handler)
must be suppressed. Options:

- Set a flag on the TermWindow that `mouse_event_impl` checks
- Or dispatch `RawScrollEvent` first, and only dispatch `VertWheel`/`HorzWheel`
  if it wasn't consumed (requires the handler to signal back)

The simplest approach: dispatch `RawScrollEvent` **instead of** the existing
`MouseEvent` when over a browser overlay. Check overlay hit-test at the
`scroll_wheel()` level. If not over an overlay, fall through to the existing
`VertWheel`/`HorzWheel` path.

However, this requires knowing overlay state at the window layer, which
currently only exists in the termsurf module. A cleaner approach: always
dispatch both events. The `RawScrollEvent` handler sets a flag. The subsequent
`MouseEvent` handler checks the flag and skips if already consumed.

#### Files modified

| File                                          | Change                                     |
| --------------------------------------------- | ------------------------------------------ |
| `wezboard/window/src/lib.rs`                  | Add `RawScrollEvent` to `WindowEvent` enum |
| `wezboard/window/src/os/macos/window.rs`      | Extract phase/momentumPhase, dispatch raw  |
| `wezboard/wezboard-gui/src/termwindow/mod.rs` | Handle `RawScrollEvent`                    |
| `wezboard/wezboard-gui/src/termsurf/input.rs` | Add `try_forward_raw_scroll()`             |

#### Verification

1. Wezboard builds without errors
2. Load a page — trackpad scrolling works smoothly (phase lifecycle: began →
   changed → ended)
3. Mouse wheel scrolling works (phase: none, discrete ticks)
4. Momentum scrolling works (momentum_phase lifecycle after finger lifts)
5. Terminal scrolling still works when no browser overlay is active
6. Compare scroll behavior with Ghostboard as reference
