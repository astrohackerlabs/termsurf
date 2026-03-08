# Issue 728: Complete remaining TermSurf protocol in Wezboard

## Goal

Implement the remaining unhandled TermSurf protocol messages in Wezboard so that
the `web` TUI works identically whether connected to Ghostboard or Wezboard.

## Background

Issues 715–727 built Wezboard from scratch — fork, rename, build cleanup, socket
server, protocol scaffolding, state management, process spawning, message
forwarding, CALayerHost rendering, overlay positioning, overlay lifecycle,
per-window overlays, and pane borders. Wezboard now handles 14 of 30 TermSurf
protocol messages (47%).

### What works

The full browser overlay pipeline is functional:

- **Socket server** (Issue 715) — Listens on
  `$TMPDIR/termsurf/wezboard-{pid}.sock`, sets `TERMSURF_SOCKET`, detects
  connection type (TUI vs Chromium), parses length-prefixed protobuf.
- **State management** (Issue 724) — Pane registry, server registry, tab-to-pane
  mappings, last-browser-pane tracking, server pane counting.
- **Process spawning** (Issue 724) — Spawns Roamium with `--ipc-socket`, tracks
  process lifecycle, reuses servers for same profile.
- **Tab lifecycle** (Issues 724, 726) — CreateTab, TabReady, CloseTab with
  proper cleanup on TUI disconnect.
- **Message forwarding** (Issue 724) — Navigate, UrlChanged, LoadingState,
  TitleChanged, SetColorScheme, ModeChanged, Resize.
- **CALayerHost rendering** (Issues 724, 725) — Transparent overlay NSView with
  layer-hosting, three-layer hierarchy (flipped → positioning → host), zero-copy
  GPU compositing.
- **Overlay positioning** (Issues 725–727) — Cell metrics bridge, per-pane grid
  offset from mux PositionedPane, contentsScale fix for Retina, TUI viewport
  offset (col/row), padding + border metrics, per-window overlay views.
- **Overlay lifecycle** (Issue 726) — Tab switching visibility sync, resize with
  Chromium Resize messages.
- **Query handlers** (Issue 726) — QueryLastRequest, QueryDevtoolsRequest,
  QueryTabsRequest with proper replies.
- **Pane borders** (Issue 723) — Configurable focused/unfocused colors, width,
  content inset.

### Messages currently handled (14 of 30)

| #   | Message              | Direction        | Handler                |
| --- | -------------------- | ---------------- | ---------------------- |
| 1   | ServerRegister       | Chromium → Board | handle_server_register |
| 2   | SetOverlay           | TUI → Board      | handle_set_overlay     |
| 3   | TabReady             | Chromium → Board | handle_tab_ready       |
| 4   | HelloRequest         | TUI → Board      | inline reply           |
| 5   | UrlChanged           | Chromium → Board | forward_to_tui         |
| 6   | LoadingState         | Chromium → Board | forward_to_tui         |
| 7   | TitleChanged         | Chromium → Board | forward_to_tui         |
| 8   | Navigate             | TUI → Board      | forward_to_chromium    |
| 9   | SetColorScheme       | TUI → Board      | forward_to_chromium    |
| 10  | ModeChanged          | TUI → Board      | update pane state      |
| 11  | CaContext            | Chromium → Board | handle_ca_context      |
| 12  | QueryLastRequest     | TUI → Board      | inline reply           |
| 13  | QueryDevtoolsRequest | TUI → Board      | inline reply           |
| 14  | QueryTabsRequest     | TUI → Board      | inline reply           |

### Messages NOT handled (16 of 30)

**Reply-only messages (6) — sent by the board, never received:**

These are outbound-only messages that the board sends in response to requests or
as state updates. The board never receives them. They are already "handled" in
the sense that their send paths exist (e.g., `HelloReply` is sent in response to
`HelloRequest`). No additional handler code is needed:

| Message            | Direction     | Status       |
| ------------------ | ------------- | ------------ |
| HelloReply         | Board → TUI   | Already sent |
| QueryLastReply     | Board → TUI   | Already sent |
| QueryDevtoolsReply | Board → TUI   | Already sent |
| QueryTabsReply     | Board → TUI   | Already sent |
| CreateTab          | Board → Chrom | Already sent |
| CloseTab           | Board → Chrom | Already sent |

**Board-initiated messages (4) — board generates and sends, never receives:**

These are messages the board originates in response to user input events or
window state changes. The board never receives them on the socket — it creates
and sends them. They require hooking into WezTerm's event system:

| Message     | Direction        | What it does                       |
| ----------- | ---------------- | ---------------------------------- |
| KeyEvent    | Board → Chromium | Forward keyboard events to browser |
| MouseEvent  | Board → Chromium | Forward mouse clicks to browser    |
| MouseMove   | Board → Chromium | Forward mouse movement to browser  |
| ScrollEvent | Board → Chromium | Forward scroll wheel to browser    |

**Received but unhandled (6) — arrive on socket, currently ignored:**

| Message            | Direction        | What it does                           |
| ------------------ | ---------------- | -------------------------------------- |
| SetDevtoolsOverlay | TUI → Board      | Create DevTools pane linked to tab     |
| OpenSplit          | TUI → Board      | Create a split pane in the terminal    |
| CursorChanged      | Chromium → Board | Update system cursor over overlay      |
| FocusChanged       | Board → Chromium | Notify browser of focus change         |
| Resize             | Board → Chromium | Already partially handled (SetOverlay) |
| CreateDevtoolsTab  | Board → Chromium | Send DevTools tab creation to Chromium |

## Approach

Group the remaining work into experiments by functional area, ordered by user
impact:

1. **Input forwarding** — KeyEvent, MouseEvent, MouseMove, ScrollEvent. This is
   the highest-impact missing feature. Without input, the browser overlay is
   view-only. Ghostboard hooks into Surface.keyCallback() and
   mouseButtonCallback() to intercept events when in browse mode. Wezboard needs
   equivalent hooks in the WezTerm event path, translating WezTerm key/mouse
   events into TermSurf proto messages and sending them to Chromium via the
   server's tx channel.

2. **Cursor changes** — CursorChanged. When the browser changes the cursor
   (pointer, text, hand, etc.), the board should update the system cursor.
   Ghostboard handles this in `handleCursorChanged`. The proto sends a cursor
   type integer that maps to macOS NSCursor types.

3. **Focus management** — FocusChanged. When a pane gains or loses focus, the
   board should notify Chromium so it can update its internal focus state
   (affects text selection, form focus, etc.). Ghostboard sends FocusChanged
   when the active pane changes.

4. **DevTools support** — SetDevtoolsOverlay and CreateDevtoolsTab. The TUI
   sends SetDevtoolsOverlay to open DevTools for a specific tab. The board
   creates a pane with `inspected_tab_id` set, then sends CreateDevtoolsTab to
   Chromium instead of CreateTab. Ghostboard implements this in
   `handleSetDevtoolsOverlay`.

5. **Split management** — OpenSplit. The TUI sends OpenSplit to create a new
   terminal split pane. The board should call WezTerm's split pane API to create
   a new pane in the specified direction.

## Reference: Ghostboard implementations

### Input forwarding (Ghostboard)

Ghostboard routes input in `Surface.zig`:

- `keyCallback()` — In browse mode, converts key events to TermSurf KeyEvent
  proto and sends via socket. Maps Ghostty key codes to Windows virtual key
  codes. Handles Cmd+key bypass (Cmd+C/V/A/L pass to the TUI, not the browser).
- `mouseButtonCallback()` — Converts mouse events to TermSurf MouseEvent proto.
  Computes overlay-relative coordinates from window-absolute position.
- `mouseMotion()` — Sends MouseMove with overlay-relative coords.
- `scrollCallback()` — Sends ScrollEvent with delta values and phase info.

### Modifier translation

WezTerm and TermSurf use different modifier bit positions:

| Modifier | WezTerm  | TermSurf |
| -------- | -------- | -------- |
| Shift    | `1 << 1` | `1 << 0` |
| Ctrl     | `1 << 3` | `1 << 1` |
| Alt      | `1 << 2` | `1 << 2` |
| Super    | `1 << 4` | `1 << 3` |

### Key code translation

Ghostboard maps its internal key codes to Windows virtual key codes (VK\_\*) for
the TermSurf KeyEvent proto. WezTerm uses its own `KeyCode` enum. The mapping
needs to convert WezTerm KeyCode variants to Windows VK codes.

### Cursor type mapping

The CursorChanged proto sends an integer cursor type. Ghostboard maps these to
Ghostty cursor shapes in `handleCursorChanged`. Wezboard needs to map them to
WezTerm's `MouseCursor` enum or directly to macOS NSCursor types.

## Experiment 1: Mode-aware input forwarding

### Goal

Forward keyboard, mouse, and scroll events to Chromium when the active pane is
in browse mode. This is the highest-impact missing feature — without it, the
browser overlay is view-only.

### Design

#### How Ghostboard does it

Ghostboard intercepts input at three points in `Surface.zig`:

1. **`keyCallback()` (line 2723)** — Checks `xpc.isOverlayForwarding(self)`
   (browse mode + focused pane). If true, sends `KeyEvent` to Chromium and
   returns `.consumed`. Special-cases Esc to exit browse mode.
2. **`mouseButtonCallback()` (line 4021)** — Calls `hitTestOverlay()` to check
   if the click is inside the overlay rectangle. If yes, forwards `MouseEvent`.
   Left-click on overlay auto-switches to browse mode; left-click off overlay
   switches to control mode.
3. **`scrollCallback()` (line 3519)** — Calls `hitTestOverlay()`. If the cursor
   is over the overlay, forwards `ScrollEvent` regardless of mode.

Mouse move is sent from `cursorPosCallback()` when the cursor is over the
overlay.

Coordinates are overlay-relative: `hitTestOverlay()` computes the overlay's
pixel rectangle from cell grid position, subtracts the origin, and divides by
content scale for Retina.

#### Wezboard interception points

WezTerm's event flow in `termwindow/`:

- **Keyboard**: `raw_key_event_impl()` and `key_event_impl()` both call
  `process_key()` (keyevent.rs:239). This is the single chokepoint for all
  keyboard input.
- **Mouse**: `mouse_event_impl()` (mouseevent.rs:61) dispatches to
  `mouse_event_terminal()` (mouseevent.rs:648) for pane-targeted events.
- **Scroll**: Handled as `WMEK::VertWheel` / `WMEK::HorzWheel` variants inside
  `mouse_event_terminal()`.

The active pane is available as `pane.pane_id()` (a `usize`). TermSurf state
uses string pane IDs. The bridge is `pane_id.to_string()` to look up
`state.panes`.

#### What to build

**1. Helper module: `termsurf/input.rs`** (new file)

Public functions callable from `termwindow/` that check TermSurf state and
forward to Chromium:

```rust
/// Check if a pane is in browse mode and has an active browser tab.
pub fn is_browsing(pane_id: usize) -> bool

/// Forward a key event to Chromium. Returns true if consumed.
pub fn forward_key_event(
    pane_id: usize,
    key: &KeyCode,
    modifiers: Modifiers,
    is_down: bool,
    utf8: &str,
) -> bool

/// Forward a mouse event to Chromium. Returns true if consumed.
pub fn forward_mouse_event(
    pane_id: usize,
    event_type: &str,     // "down" or "up"
    button: &str,         // "left", "right", "middle"
    x: f64,               // overlay-relative pixel X
    y: f64,               // overlay-relative pixel Y
    click_count: i64,
    modifiers: Modifiers,
) -> bool

/// Forward a mouse move to Chromium. Returns true if consumed.
pub fn forward_mouse_move(
    pane_id: usize,
    x: f64,
    y: f64,
    left_button_down: bool,
    right_button_down: bool,
) -> bool

/// Forward a scroll event to Chromium. Returns true if consumed.
pub fn forward_scroll_event(
    pane_id: usize,
    x: f64,
    y: f64,
    delta_x: f64,
    delta_y: f64,
) -> bool
```

Each function: lock TermSurf global state → look up pane by
`pane_id.to_string()` → check `pane.browsing` (for key events) or overlay bounds
(for mouse/scroll) → build protobuf message → send via server tx channel.

**2. Key code translation: `keycode_to_windows_vk()`**

Map WezTerm `KeyCode` variants to Windows virtual key codes, matching
Ghostboard's `keyToWindowsVK()` (xpc.zig:1315):

```rust
fn keycode_to_windows_vk(key: &KeyCode) -> i64 {
    match key {
        KeyCode::Char(c) => match c.to_ascii_uppercase() {
            'A'..='Z' => *c as i64,  // 0x41-0x5A
            '0'..='9' => *c as i64,  // 0x30-0x39
            _ => 0,
        },
        KeyCode::Function(n) => 0x70 + (*n as i64 - 1),  // F1=0x70
        KeyCode::Enter => 0x0D,
        KeyCode::Tab => 0x09,
        KeyCode::Backspace => 0x08,
        KeyCode::Escape => 0x1B,
        KeyCode::Delete => 0x2E,
        KeyCode::UpArrow => 0x26,
        KeyCode::DownArrow => 0x28,
        KeyCode::LeftArrow => 0x25,
        KeyCode::RightArrow => 0x27,
        KeyCode::Home => 0x24,
        KeyCode::End => 0x23,
        KeyCode::PageUp => 0x21,
        KeyCode::PageDown => 0x22,
        KeyCode::Insert => 0x2D,
        _ => 0,
    }
}
```

**3. Modifier translation: `modifiers_to_termsurf()`**

WezTerm and TermSurf use different bit positions:

```rust
fn modifiers_to_termsurf(mods: Modifiers) -> u64 {
    let mut result: u64 = 0;
    if mods.contains(Modifiers::SHIFT)   { result |= 1; }      // 1 << 0
    if mods.contains(Modifiers::CTRL)    { result |= 2; }      // 1 << 1
    if mods.contains(Modifiers::ALT)     { result |= 4; }      // 1 << 2
    if mods.contains(Modifiers::SUPER)   { result |= 8; }      // 1 << 3
    result
}
```

**4. Overlay hit testing: `hit_test_overlay()`**

Compute whether a pixel coordinate falls inside the overlay rectangle, and
return overlay-relative coordinates if so. Uses pane state (`col`, `row`,
`pixel_width`, `pixel_height`) and the cell metrics bridge:

```rust
fn hit_test_overlay(
    pane_id: usize,
    window_x: f64,
    window_y: f64,
) -> Option<(f64, f64)>
```

The overlay's pixel origin is `(col * cell_width, row * cell_height)` plus
padding and border offsets. This mirrors Ghostboard's `hitTestOverlay()`
(Surface.zig:2455).

**5. Keyboard interception in `process_key()`**

Add an early check at the top of `process_key()` (keyevent.rs:239), before
leader key and keybinding processing:

```rust
// Forward to browser overlay if in browse mode (TermSurf).
if let Some(result) = crate::termsurf::input::try_forward_key(
    pane.pane_id(),
    keycode,
    raw_modifiers,
    is_down,
    key_event,
) {
    return result;
}
```

The `try_forward_key` function handles:

- Look up TermSurf pane state for `pane.pane_id()`
- If not browsing, return `None` (let WezTerm handle normally)
- If Esc press: set `pane.browsing = false`, send `ModeChanged(false)` to TUI,
  send `FocusChanged(false)` to Chromium, return `Some(true)` (consumed)
- Otherwise: translate key code and modifiers, send `KeyEvent` to Chromium,
  return `Some(true)` (consumed)

**6. Mouse interception in `mouse_event_terminal()`**

Add an early check at the top of `mouse_event_terminal()` (mouseevent.rs:648),
before pane resolution:

```rust
// Forward to browser overlay if click hits overlay (TermSurf).
if crate::termsurf::input::try_forward_mouse(
    pane.pane_id(),
    &event,
    &self.render_metrics,
    // pass padding/border offsets for coordinate translation
) {
    return;
}
```

The `try_forward_mouse` function handles:

- Hit test: is the mouse position inside the overlay rectangle?
- If yes and left-click press: auto-switch to browse mode (set
  `pane.browsing = true`, send `ModeChanged(true)` to TUI)
- If yes: forward `MouseEvent`, `MouseMove`, or `ScrollEvent` depending on
  `event.kind`
- If no and left-click press and was browsing: switch to control mode
- Mouse move forwarding when cursor is over overlay (regardless of browse mode,
  for hover effects)

**7. Mode change notifications**

When the board auto-switches mode (click on/off overlay, Esc), it must notify
both sides:

- **TUI**: Send `ModeChanged { browsing, pane_id }` so the TUI updates its
  status bar
- **Chromium**: Send `FocusChanged { tab_id, focused }` so Chromium updates
  internal focus state (text selection, form focus, etc.)

This requires a new helper in `conn.rs`:

```rust
pub fn send_mode_changed(pane_id: &str, browsing: bool, state: &SharedState)
pub fn send_focus_changed(pane_id: &str, focused: bool, state: &SharedState)
```

### Files to modify

| File                       | Changes                                       |
| -------------------------- | --------------------------------------------- |
| `termsurf/input.rs` (new)  | Input forwarding module with all helpers      |
| `termsurf/mod.rs`          | Add `pub mod input;`                          |
| `termsurf/conn.rs`         | Add `send_mode_changed`, `send_focus_changed` |
| `termwindow/keyevent.rs`   | Early return in `process_key()` for browse    |
| `termwindow/mouseevent.rs` | Early return in `mouse_event_terminal()`      |

### Coordinate system

The trickiest part is translating mouse coordinates from WezTerm's window pixel
space to overlay-relative pixel space:

1. **Window pixel coords** — `event.coords.x`, `event.coords.y` (from
   `mouse_event_impl`)
2. **Subtract padding + border + tab bar** — same offsets already computed in
   `mouse_event_impl()` for cell coordinate conversion
3. **Subtract overlay origin** — `col * cell_width`, `row * cell_height` (from
   TermSurf pane state × cell metrics)
4. **Divide by content scale** — for Retina displays (matches Ghostboard's
   approach)

Cell metrics are available via the global atomics bridge (`termsurf::metrics`),
already used for overlay positioning in Issue 725.

### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Open `web google.com` — overlay renders (existing behavior)
3. Type in the Google search box — keystrokes appear in the browser
4. Click a link — navigates to the target page
5. Scroll on a page — page scrolls
6. Press Esc — returns to control mode, keys go to terminal
7. Click on overlay — auto-switches to browse mode
8. Click outside overlay — returns to control mode

**Result:** Fail

Build succeeded with zero errors and keyboard input partially worked — key
events reached Chromium and the browser responded to them. However, mouse input
had two critical bugs:

1. **Mouse coordinates were wrong.** Hovered links highlighted significantly
   lower than the actual cursor position. The `hit_test_overlay` function stored
   the overlay origin in backing pixels (pre-Retina-scale values from
   `update_ca_layer_frame`) and compared them correctly against `event.coords`
   (also backing pixels), so hit testing worked. But the overlay-relative
   coordinates sent to Chromium were in backing pixels, while Chromium expects
   logical/CSS pixels (points). On a 2× Retina display, the y offset sent to
   Chromium was double the correct value, making hovers land far below the
   cursor.

2. **Scroll events crashed Chromium.** WezTerm's `WMEK::VertWheel(i16)` is a
   discrete wheel delta with no scroll phase information. The implementation
   sent `phase=0` and `momentum_phase=0`, but Chromium's
   `MouseWheelEventQueue::TryForwardNextEventToRenderer()` has a DCHECK
   requiring at least one of phase or momentum_phase to be non-zero
   (`kPhaseNone = 0` is invalid for both). Ghostboard avoids this because it
   passes through the raw macOS `NSEvent.phase` and `NSEvent.momentumPhase`
   values directly from the Swift layer — discrete trackpad scrolls always have
   a real phase. WezTerm's event model strips this information.

#### Conclusion

The architecture is sound — the module structure, interception points, key
translation, modifier remapping, and mode toggling all worked correctly. The two
failures are coordinate-space and protocol-detail bugs:

- **Fix 1: Divide by scale.** Store the Retina scale factor in pane state
  (already available in `update_ca_layer_frame` as the `scale` variable). In
  `hit_test_overlay`, divide the overlay-relative coordinates by scale before
  returning them. This converts backing pixels → logical pixels for Chromium.

- **Fix 2: Set scroll phase.** For discrete wheel events, set `phase = 4`
  (`kPhaseChanged`, which is `1 << 2` in Chromium's bit-flag enum) instead of 0.
  This satisfies the DCHECK. Ghostboard doesn't need this because it forwards
  raw macOS phases, but Wezboard must synthesize them since WezTerm doesn't
  expose scroll phases.
