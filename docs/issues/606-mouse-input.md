# Issue 606: Mouse Input

## Goal

Click on links and buttons in a Chromium overlay. Scroll web pages. Select text
by dragging. Cursor changes to a pointer hand over links. Focus state enables
selection highlights and carets.

## Background

Issues 603-605 proved the full visual pipeline: Ghost renders live Chromium
frames as Metal overlays, routes `display_surface` by pane ID, reuses servers by
profile, and handles multi-pane/multi-profile correctly. But there is no input —
the overlays are display-only.

ts5 solved mouse input in Issues 514-515 with four NSEvent local monitors in
CompositorXPC.swift. The monitors intercepted clicks, scrolls, moves, and
Ctrl+Esc before the responder chain, hit-tested against overlay bounds,
transformed coordinates, and forwarded events to the Chromium server via XPC.
The server injected them as `blink::WebMouseEvent` / `WebMouseWheelEvent` via
`ForwardMouseEvent()` / `ForwardWheelEvent()`.

Ghost's architecture is different: browser integration lives in Zig, not Swift.
But the mouse event pipeline maps cleanly to Zig because:

- `Surface.mouseButtonCallback()` already receives all mouse clicks with
  coordinates and modifiers
- `Surface.scrollCallback()` receives all scroll events
- `Surface.cursorPosCallback()` receives mouse moves (called from `mouseMoved`
  and `mouseDragged`)
- The overlay grid coordinates and cell size are already stored on the surface
- XPC message construction is already proven in `xpc.zig`

The key insight from ts5 Issue 515: left mouse events must NOT be consumed —
they must propagate through the responder chain so macOS generates drag events
for text selection. ts5 solved this with a `suppressMouseForOverlay` flag on
SurfaceView that skips terminal processing while preserving drag tracking.

### What we have

**Ghost (from Issues 601-605):**

- `Surface.mouseButtonCallback()` — receives clicks with action, button, mods
- `Surface.scrollCallback()` — receives scroll events with x/y offsets
- `Surface.cursorPosCallback()` — receives mouse moves with position
- `Surface.setOverlay()` — stores overlay grid coordinates on the renderer
- `Surface.getCellSize()` — returns cell dimensions in physical pixels
- `renderer.pink_overlay` — has `grid_col`, `grid_row`, `grid_width`,
  `grid_height`
- XPC serial queue and HashMap-based multi-pane state in `xpc.zig`

**Chromium Profile Server (from ts5 Issues 514-515):**

- `HandleMouseEvent` — maps XPC message to `blink::WebMouseEvent`, calls
  `ForwardMouseEvent()`
- `HandleScrollEvent` — maps to `blink::WebMouseWheelEvent`, calls
  `ForwardWheelEvent()`
- `HandleMouseMove` — mouse move with button state for drag selection
- `HandleFocusChanged` — `view->Focus()` + `view->SetActive(true)` enables
  selection highlights
- `OnCursorChanged` callback — sends `cursor_changed` back via XPC
- `DidFinishNavigation` — sends `url_changed` back via XPC

**XPC message formats (proven in ts5):**

```
mouse_event:  { action, pane_id, type, x, y, button, click_count, modifiers }
scroll_event: { action, pane_id, x, y, delta_x, delta_y, phase, momentum_phase, precise, modifiers }
mouse_move:   { action, pane_id, x, y, modifiers }
focus_changed: { action, pane_id, focused }
```

Coordinates are overlay-relative logical pixels (physical pixels / scale
factor).

### What needs to change

**1. Hit-testing in Zig.**

When `mouseButtonCallback` fires, check if the click position falls within the
overlay bounds. The overlay is at `(grid_col, grid_row)` with size
`(grid_width, grid_height)` in grid cells. Multiply by cell size (physical
pixels) to get the overlay rectangle. Compare against the mouse position
(physical pixels). If inside, forward to Chromium instead of the terminal.

**2. Coordinate transformation.**

Transform from surface coordinates (physical pixels, origin top-left) to
overlay-relative logical pixels for Chromium:

1. Compute overlay origin: `overlay_x = grid_col * cell_w`,
   `overlay_y = grid_row * cell_h` (physical pixels)
2. Subtract: `rel_x = mouse_x - overlay_x`, `rel_y = mouse_y - overlay_y`
3. Bounds check: `0 <= rel_x < grid_width * cell_w` (same for y)
4. Convert to logical: `chromium_x = rel_x / scale`,
   `chromium_y = rel_y / scale`

**3. XPC forwarding from Zig.**

Construct XPC messages for `mouse_event`, `scroll_event`, `mouse_move` and send
on the server's control connection. The server routes by `pane_id` to the
correct tab's `RenderWidgetHost`.

**4. Suppress terminal processing for overlay clicks.**

When a click is in the overlay, `mouseButtonCallback` must return early (not
process for terminal). But left mouse events need macOS drag tracking — the
Swift SurfaceView must still propagate the NSEvent while skipping its own
terminal handling. This requires a `suppressMouseForOverlay` flag, same pattern
as ts5 Issue 515.

**5. Focus lifecycle.**

Chromium needs `Focus()` + `SetActive(true)` to paint selection highlights. Send
`focus_changed` when entering browse mode and when the pane gains/loses focus.

**6. Cursor synchronization.**

The Chromium server sends `cursor_changed` messages with cursor type (pointer,
hand, ibeam, etc.). Ghost needs to apply the corresponding macOS cursor when the
mouse is over the overlay and restore the terminal cursor when it leaves.

### What should work without changes

- **Chromium server** — all mouse/scroll/focus/cursor handlers already exist
  from Issues 514-515
- **XPC message format** — same protocol, Ghost just needs to construct the
  messages
- **`web` TUI** — no mouse awareness needed; overlay hit-testing happens in
  Ghost

## Experiment 1: Mouse clicks

### Goal

Click a link in the Chromium overlay and see the page navigate. Left click and
right click both work. Coordinates are correct (clicking a specific link hits
that link, not an adjacent one).

### Design

**Phase 1: Hit-test and forward clicks from Zig.**

Add a `hitTestOverlay` function to `Surface.zig` that checks if a mouse position
(in physical pixels) falls within the overlay rectangle:

```zig
/// Check if a point (physical pixels) is inside the overlay.
/// Returns overlay-relative logical coordinates, or null if outside.
pub fn hitTestOverlay(self: *Surface, phys_x: f64, phys_y: f64) ?struct { x: f64, y: f64 } {
    self.renderer.draw_mutex.lock();
    defer self.renderer.draw_mutex.unlock();

    const overlay = self.renderer.pink_overlay;
    if (overlay.grid_width == 0) return null;

    const cell_w: f64 = @floatFromInt(self.renderer.grid_metrics.cell_width);
    const cell_h: f64 = @floatFromInt(self.renderer.grid_metrics.cell_height);
    const ox = overlay.grid_col * cell_w;
    const oy = overlay.grid_row * cell_h;
    const ow = overlay.grid_width * cell_w;
    const oh = overlay.grid_height * cell_h;

    const rel_x = phys_x - ox;
    const rel_y = phys_y - oy;
    if (rel_x < 0 or rel_y < 0 or rel_x >= ow or rel_y >= oh) return null;

    const scale: f64 = @floatFromInt(self.size.screen.dpi_x) / 72.0;
    return .{ .x = rel_x / scale, .y = rel_y / scale };
}
```

**Phase 2: Intercept clicks in `mouseButtonCallback`.**

At the top of `mouseButtonCallback`, before any terminal processing:

```zig
// Check if click is in a browser overlay.
if (self.hitTestOverlay(self.mouse.point.x, self.mouse.point.y)) |overlay_pos| {
    // Forward to Chromium via XPC.
    xpc.sendMouseEvent(self, action, button, mods, overlay_pos.x, overlay_pos.y);
    return true; // consumed, don't process for terminal
}
```

**Phase 3: XPC message construction in `xpc.zig`.**

Add `sendMouseEvent` that looks up the pane by surface pointer, constructs an
XPC dictionary with `action=mouse_event`, `pane_id`, `type=down|up`,
`button=left|right`, `x`, `y`, `click_count`, `modifiers`, and sends on the
server's control connection.

Need a reverse lookup from surface pointer to pane, or pass pane ID through. The
simplest approach: add a `surface_to_pane` map (`AutoHashMap(usize, *Pane)`)
keyed by `CoreSurface` pointer address, populated in `handleSetOverlay`.

**Phase 4: Modifier mapping.**

Map Ghost's `input.Mods` to the Chromium modifier bitmask:

```
shift = 1, ctrl = 2, alt = 4, cmd = 8
```

For mouse down events, also set button-down flags:

```
left_button_down = 64 (1 << 6)
right_button_down = 256 (1 << 8)
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://news.ycombinator.com
```

Pass criteria:

- Clicking a Hacker News story link navigates to the target page
- Click position is accurate (correct link is hit, not neighbors)
- Right click works (context menu or right-click handler fires)
- Clicks outside the overlay go to the terminal as normal
- No crash on rapid clicking

### Result: Pass

Clicking links on Hacker News navigates correctly. The hit-test and coordinate
transformation pipeline works: `hitTestOverlay` on the Surface checks overlay
bounds in physical pixels, converts to logical pixels via content scale, and
`sendMouseEvent` in xpc.zig forwards the XPC `mouse_event` to the Chromium
server's control connection. Clicks outside the overlay go to the terminal as
normal.

## Experiment 2: Scroll forwarding

### Goal

Scroll a web page in the Chromium overlay. Trackpad two-finger scroll and mouse
wheel both work. Momentum scrolling (inertial flick) works. Scrolling outside
the overlay scrolls the terminal as normal.

### Background

Ghostty's `scrollWheel` handler in `SurfaceView_AppKit.swift` modifies raw
NSEvent scroll data before passing it to Zig:

1. **Delta values are doubled** for precision (trackpad) scrolls — a "feels
   better" UX choice for terminal scrolling speed
2. **Momentum phase is remapped** from NSEvent bitmask values (1,2,4,8,16,32) to
   a compact sequential enum (1,2,3,4,5,6) for cross-platform abstraction
3. **Gesture phase (`NSEvent.phase`) is dropped** entirely — terminals don't
   need it

Chromium needs the raw values. ts5 solved this trivially because the scroll
monitor had direct NSEvent access. Ghost's architecture routes scroll events
through the Zig C API, which loses the raw data.

The solution: a new `termsurf_macos_surface_mouse_scroll` C API function that
carries both the processed values (for terminal) and the raw NSEvent values (for
Chromium). The `termsurf_` prefix provides a namespace separate from Ghostty's
`ghostty_` APIs, guaranteeing compatibility with upstream changes. The `_macos_`
infix makes the platform specificity explicit — Linux will get
`termsurf_linux_surface_mouse_scroll` with its own raw values.

### Design

**Phase 1: New C API — `termsurf_macos_surface_mouse_scroll`.**

Add to `embedded.zig`:

```zig
export fn termsurf_macos_surface_mouse_scroll(
    surface: *Surface,
    x: f64,           // processed delta (2x multiplied for trackpad)
    y: f64,           // processed delta
    scroll_mods: c_int, // existing ScrollMods (precision + momentum enum)
    raw_delta_x: f64,   // NSEvent.scrollingDeltaX (unmodified)
    raw_delta_y: f64,   // NSEvent.scrollingDeltaY (unmodified)
    raw_phase: u64,      // NSEvent.phase.rawValue (bitmask)
    raw_momentum_phase: u64, // NSEvent.momentumPhase.rawValue (bitmask)
) void
```

This function stores the raw values on the surface, then calls the existing
`scrollCallback(x, y, scroll_mods)`. The terminal path runs unchanged with the
processed values. The browser path (hit-test in `scrollCallback`) reads the
stored raw values.

Raw values stored on `CoreSurface`:

```zig
/// Raw macOS scroll data for browser forwarding (Issue 606).
/// Set by termsurf_macos_surface_mouse_scroll, read by scrollCallback.
raw_scroll: struct {
    delta_x: f64 = 0,
    delta_y: f64 = 0,
    phase: u64 = 0,
    momentum_phase: u64 = 0,
    precise: bool = false,
} = .{},
```

The `precise` field is copied from `scroll_mods.precision` — this value is
correct (not remapped), just convenient to have alongside the other raw fields.

**Phase 2: Swift — call new API.**

In `SurfaceView_AppKit.swift`'s `scrollWheel(with:)`, replace the call to
`surfaceModel.sendMouseScroll(scrollEvent)` with a direct call to the new C API:

```swift
override func scrollWheel(with event: NSEvent) {
    guard let surface = self.surface else { return }

    var x = event.scrollingDeltaX
    var y = event.scrollingDeltaY
    let precision = event.hasPreciseScrollingDeltas

    if precision {
        x *= 2
        y *= 2
    }

    let scrollMods = Ghostty.Input.ScrollMods(
        precision: precision,
        momentum: .init(event.momentumPhase)
    )

    termsurf_macos_surface_mouse_scroll(
        surface,
        x, y,
        scrollMods.cScrollMods,
        event.scrollingDeltaX,          // raw, unmodified
        event.scrollingDeltaY,          // raw, unmodified
        UInt64(event.phase.rawValue),   // bitmask: 0,1,2,4,8,16,32
        UInt64(event.momentumPhase.rawValue) // bitmask: 0,1,2,4,8,16,32
    )
}
```

The existing `sendMouseScroll` code path is no longer called from `scrollWheel`.
All scroll events go through the new function. Non-overlay scrolls behave
identically — the processed values reach `scrollCallback` the same way they
always did.

**Phase 3: Intercept scrolls in `scrollCallback`.**

At the top of `scrollCallback`, before any terminal processing:

```zig
// Check if scroll is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
        const xpc = @import("apprt/xpc.zig");
        xpc.sendScrollEvent(self, overlay_pos.x, overlay_pos.y);
        return;
    }
}
```

`scrollCallback` returns `!void`, so `return` is enough to suppress terminal
processing. The `sendScrollEvent` reads `self.raw_scroll` for the raw values.

**Phase 4: `sendScrollEvent` in `xpc.zig`.**

Add `sendScrollEvent` that constructs an XPC `scroll_event` dictionary using the
raw values stored on the surface:

```
action:         "scroll_event"
pane_id:        UUID string
x, y:           overlay-relative logical pixels (from hitTestOverlay)
delta_x:        raw_scroll.delta_x (NSEvent.scrollingDeltaX, unmodified)
delta_y:        raw_scroll.delta_y (NSEvent.scrollingDeltaY, unmodified)
phase:          raw_scroll.phase (NSEvent.phase.rawValue bitmask)
momentum_phase: raw_scroll.momentum_phase (NSEvent.momentumPhase.rawValue bitmask)
precise:        raw_scroll.precise (hasPreciseScrollingDeltas)
modifiers:      0 (scroll events rarely carry modifiers)
```

These are the exact same values ts5 sent. No remapping, no scaling corrections.
The Chromium server's `HandleScrollEvent` receives identical input to what it
received from ts5's NSEvent monitor.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://news.ycombinator.com
```

Pass criteria:

- Two-finger trackpad scroll moves the page up and down smoothly
- Momentum scrolling works (flick and release, page keeps scrolling)
- Mouse wheel scrolling works
- Scrolling outside the overlay scrolls the terminal as normal
- No jitter or reverse-direction scroll

### Result: Pass

Scrolling works perfectly. The `termsurf_macos_surface_mouse_scroll` C API
carries raw NSEvent values (unmodified deltas, bitmask phases) alongside
Ghostty's processed values. Zig stores the raw data on the surface, hit-tests
the overlay in `scrollCallback`, and forwards the raw values to Chromium via
XPC. The Chromium server receives identical input to what ts5 sent from its
NSEvent monitor — no remapping, no scaling corrections. Trackpad smooth scroll,
momentum scrolling, and terminal fallback all work correctly.

## Experiment 3: Mouse move forwarding

### Goal

Hover over a link in the Chromium overlay and see it highlight. Move the mouse
across the page and see Chromium's hover states respond. Drag with left button
held and see text selection begin (visual feedback only — full selection support
is a later experiment).

### Background

Unlike scrolling, mouse move has no raw-vs-processed data issue. The coordinates
Ghost receives in `cursorPosCallback` are straightforward — already in physical
pixels via `cursorPosToPixels`, same coordinate space as `hitTestOverlay`. No
`termsurf_macos_` API needed.

The one subtlety: **button state during drag.** ts5 inferred button state from
`NSEvent.type` — `.leftMouseDragged` sets `kLeftButtonDown` (bit 6 = 64) in the
modifier bitmask. Chromium's `HandleMouseMove` reads this to distinguish hover
from drag-select. Ghost routes `mouseMoved`, `mouseDragged`, and
`rightMouseDragged` through the same `cursorPosCallback` with no distinction.

The fix: track overlay button state on the Surface. When `mouseButtonCallback`
forwards a press to the overlay, record which button is down. When it forwards a
release, clear it. `cursorPosCallback` reads this to set the button-down
modifier bits in the `mouse_move` XPC message.

This also requires moving the `click_state` update in `mouseButtonCallback` to
before the overlay check, so the standard button tracking still works for
overlay clicks.

### Design

**Phase 1: Track overlay button state.**

In `mouseButtonCallback`, move the `click_state` update to before the overlay
check:

```zig
// Always record our latest mouse state (moved up for overlay tracking).
self.mouse.click_state[@intCast(@intFromEnum(button))] = action;

// Check if click is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(...)) |overlay_pos| {
        xpc.sendMouseEvent(self, action, button, mods, overlay_pos.x, overlay_pos.y);
        return true;
    }
}
```

Now `click_state` reflects button state regardless of whether the click was in
the overlay or the terminal.

**Phase 2: Intercept mouse moves in `cursorPosCallback`.**

At the top of `cursorPosCallback`, before any terminal processing:

```zig
// Check if mouse is in a browser overlay (Issue 606).
if (self.hitTestOverlay(@floatCast(pos.x), @floatCast(pos.y))) |overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    xpc.sendMouseMove(self, overlay_pos.x, overlay_pos.y);
    return;
}
```

`cursorPosCallback` returns `!void`, so `return` suppresses terminal processing.

**Phase 3: `sendMouseMove` in `xpc.zig`.**

Add `sendMouseMove` that constructs an XPC `mouse_move` dictionary:

```
action:    "mouse_move"
pane_id:   UUID string
x, y:      overlay-relative logical pixels (from hitTestOverlay)
modifiers: bitmask with button-down flags from click_state
```

The modifier bitmask includes button-down flags read from
`surface.mouse.click_state`:

```zig
var modifiers: i64 = 0;
const left_idx = @intFromEnum(input.MouseButton.left);
const right_idx = @intFromEnum(input.MouseButton.right);
if (surface.mouse.click_state[left_idx] == .press) modifiers |= 64;   // kLeftButtonDown
if (surface.mouse.click_state[right_idx] == .press) modifiers |= 256; // kRightButtonDown
```

This is how Chromium distinguishes hover (no button-down flags) from drag (left
button-down flag set). The Chromium server's `HandleMouseMove` reads these bits
to set `blink::WebPointerProperties::Button`.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://news.ycombinator.com
```

Pass criteria:

- Hovering over a link shows a highlight/underline (Chromium CSS hover state)
- Moving mouse across the page triggers hover states on different elements
- Mouse moves outside the overlay still work for terminal (selection, etc.)
- Left-click drag in the overlay sends button-down flag (visible in logs)
- No crash or lag from high-frequency mouse move events
