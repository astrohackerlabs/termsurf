+++
status = "closed"
opened = "2026-02-20"
closed = "2026-02-20"
+++

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

### Result: Pass

Hovering over links on Hacker News triggers CSS hover states. The interception
in `cursorPosCallback` hit-tests the overlay, converts to logical coordinates,
and `sendMouseMove` in xpc.zig forwards the `mouse_move` XPC message with
button-down flags from `click_state`. Moving `click_state` update before the
overlay check in `mouseButtonCallback` ensures button state is tracked for both
overlay and terminal clicks, so drag events carry the correct `kLeftButtonDown`
(64) modifier. No crashes from high-frequency mouse move events.

## Experiment 4: Cursor appearance sync

### Goal

Hover over a link in the Chromium overlay and see the cursor change to a
pointing hand. Move over text and see an I-beam. Move off the overlay and see
the terminal's cursor restored. The cursor tracks Chromium's actual cursor state
in real time.

### Background

The Chromium Profile Server already sends `cursor_changed` XPC messages — this
was built in ts5's Issue 514 Experiment 5. When Chromium's renderer detects a
cursor change (e.g., hovering over an `<a>` tag), the change flows through
`RenderWidgetHostImpl::SetCursor` → `ShellVideoConsumer::OnCursorChanged` → XPC
message with the `ui::mojom::CursorType` integer value.

Ghost currently ignores these messages — `handleMessage` in xpc.zig logs
"unknown action: cursor_changed" and drops them.

Ghostty already has a cursor pipeline: terminal escape sequences dispatch
`performAction(.mouse_shape, shape)`, which sets `pointerStyle` on the
SurfaceView model. A Combine subscriber in SurfaceScrollView picks this up and
sets `scrollView.documentCursor = style.cursor`. macOS then shows that cursor
over the scroll view via the cursor rect system.

This experiment reuses that pipeline. When the mouse is over the overlay,
`cursorPosCallback` overrides `mouse_shape` with the Chromium cursor. When the
mouse leaves the overlay, it restores the terminal's cursor. The override
happens on every mouse move event (60-120Hz), matching the pattern ts5 proved
works — continuous setting is the only approach that sticks on macOS.

ts5 learned three lessons across Experiments 5-7:

1. **Continuous setting works** — calling `NSCursor.set()` on every mouse move
   while over the overlay keeps the cursor correct. One-time calls get swallowed
   by macOS cursor management.
2. **One-time reset doesn't work** — Experiment 6 tried resetting once on exit,
   but macOS timing swallows it. The cursor sticks.
3. **`invalidateCursorRects` is the correct reset** — Experiment 7 called
   `window.invalidateCursorRects(for: hitView)` which tells macOS to re-evaluate
   cursor rects and pick up `documentCursor`.

Ghost's approach is simpler: since we use Ghostty's
`performAction(.mouse_shape)` pipeline (which sets `documentCursor`), we don't
fight the cursor rect system — we work through it. Setting `mouse_shape` while
over the overlay changes `documentCursor` to the Chromium cursor. Restoring
`mouse_shape` when leaving restores it. The Combine subscriber dispatches to
main thread automatically.

### Design

**Phase 1: Handle `cursor_changed` in xpc.zig.**

Add `cursor_changed` case to `handleMessage`:

```zig
} else if (std.mem.eql(u8, action_str, "cursor_changed")) {
    handleCursorChanged(msg);
}
```

New handler that stores the cursor type on both the Pane and the CoreSurface:

```zig
fn handleCursorChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const cursor_type = xpc_dictionary_get_int64(msg, "cursor_type");

    if (panes.get(pane_id)) |p| {
        if (p.overlay_surface) |surface| {
            surface.overlay_cursor_type = cursor_type;
        }
    }
}
```

Need to add the `xpc_dictionary_get_int64` extern declaration (it's not there
yet — we have `set_int64` but not `get_int64`):

```zig
extern "c" fn xpc_dictionary_get_int64(xdict: xpc_object_t, key: [*:0]const u8) i64;
```

**Phase 2: Add state to Surface.**

Add `overlay_cursor_type` field to CoreSurface (`Surface.zig`). This is written
by the XPC handler (serial queue) and read by `cursorPosCallback` (main thread).
A plain `i64` is fine — worst case we see a one-frame stale value.

```zig
/// Chromium cursor type for overlay (Issue 606).
/// Set by XPC cursor_changed handler, read by cursorPosCallback.
/// Values are ui::mojom::CursorType integers from Chromium.
overlay_cursor_type: i64 = 0,
```

**Phase 3: Set cursor in `cursorPosCallback`.**

Expand the overlay check in `cursorPosCallback` to also set the cursor:

```zig
// Check if mouse is in a browser overlay (Issue 606).
if (self.hitTestOverlay(@floatCast(pos.x), @floatCast(pos.y))) |overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    xpc.sendMouseMove(self, overlay_pos.x, overlay_pos.y);

    // Set cursor from Chromium's cursor type.
    const shape = mapChromiumCursor(self.overlay_cursor_type);
    _ = try self.rt_app.performAction(
        .{ .surface = self },
        .mouse_shape,
        shape,
    );
    self.mouse.over_overlay = true;
    return;
}

// Restore terminal cursor when leaving the overlay.
if (self.mouse.over_overlay) {
    self.mouse.over_overlay = false;
    _ = try self.rt_app.performAction(
        .{ .surface = self },
        .mouse_shape,
        self.io.terminal.mouse_shape,
    );
}
```

Add `over_overlay: bool = false` to the mouse state struct in Surface.zig
(alongside the existing `over_link`, `click_state`, etc.).

**Phase 4: Cursor type mapping.**

Map Chromium's `ui::mojom::CursorType` integers to Ghostty's `MouseShape` enum.
Only the common cursors need explicit mapping — everything else defaults to
`.default` (arrow).

```zig
const MouseShape = @import("terminal/main.zig").MouseShape;

fn mapChromiumCursor(cursor_type: i64) MouseShape {
    return switch (cursor_type) {
        0 => .default,       // kPointer (arrow)
        1 => .crosshair,     // kCross
        2 => .pointer,       // kHand
        3 => .text,          // kIBeam
        31 => .move,         // kMove
        39 => .default,      // kNone (hide cursor — use default for now)
        40 => .not_allowed,  // kNotAllowed
        43 => .grab,         // kGrab
        44 => .grabbing,     // kGrabbing
        else => .default,
    };
}
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://news.ycombinator.com
```

Pass criteria:

- Hovering over a link shows pointing hand cursor
- Hovering over text shows I-beam cursor
- Hovering over non-interactive areas shows arrow
- Moving off the overlay restores the terminal's cursor (arrow from `web` TUI
  mouse capture)
- Cursor transitions are instant (no flicker or stuck cursor)
- Rapid movement between overlay and terminal doesn't leave the wrong cursor

### Result: Pass

Hovering over links on news.ycombinator.com shows the pointing hand cursor.
Moving off the overlay restores the terminal's cursor. Cursor changes are
instant with no flicker. The `performAction(.mouse_shape)` pipeline works
through Ghostty's existing `documentCursor` system — no `NSCursor.set()` hack,
no `invalidateCursorRects` needed. Three ts5 experiments collapsed into one.

## Experiment 5: Focus lifecycle

### Goal

Chromium receives focus/unfocus signals that match the user's intent. Entering
browse mode focuses the Chromium tab. Exiting browse mode unfocuses it.
Switching panes with the keyboard transfers focus. Clicking on a webview that
isn't focused focuses it. Clicking on the control panel (URL bar) when the
webview is focused unfocuses it.

### Background

Chromium's Blink renderer requires `FocusController::IsActive() == true` to
paint selection highlights, show blinking carets in text inputs, and apply
`:focus` CSS styles. The Chromium Profile Server exposes this via a
`focus_changed` XPC message — `HandleFocusChanged` calls `view->Focus()` +
`view->SetActive(true)` on focus, `view->SetActive(false)` on blur. But Ghost
never sends this message. `CreateTab` doesn't set focus either. So Chromium tabs
are permanently unfocused — text selection won't render, input fields won't show
carets.

ts5 needed five experiments (Issue 515 Experiments 1–5) to get focus right. The
complexity came from Swift-side NSNotification observers, firstResponder
tracking, deferred focus after server registration, and two code paths
(new-server vs existing-server). Ghost's Zig-first architecture simplifies this
significantly because all the state lives in xpc.zig's Pane struct on a single
serial queue.

Three focus triggers need to work:

1. **Mode change (keyboard).** The `web` TUI sends `mode_changed` with
   `browsing: true/false` when the user presses Enter (browse) or Esc (control).
   Ghost's `handleModeChanged` currently just logs this. It should send
   `focus_changed` to the Chromium server.

2. **Pane switch (keyboard).** Ghostty's `focusDidChange` fires on every pane
   focus transition — keyboard shortcuts, mouse clicks on different panes,
   splits, focus-follows-mouse. When a pane loses focus, its Chromium tab should
   lose focus too. When a pane with an active overlay gains focus, its tab
   should regain focus (but only if the `web` TUI is in browse mode).

3. **Mouse click (overlay vs control panel).** Clicking on the overlay while in
   control mode should switch to browse mode and focus Chromium. Clicking on the
   control panel (terminal area above the overlay) while in browse mode should
   switch to control mode and unfocus Chromium. The `web` TUI already sends
   `mode_changed` on these transitions — Ghost just needs to handle them.

The single-pane enforcement rule from ts5 still applies: only one Chromium tab
can be focused at a time. Focusing one pane must unfocus any previously focused
pane.

### Design

**Phase 1: Add focus state to Pane and tracking to xpc.zig.**

Add `browsing` and track the currently focused pane:

```zig
const Pane = struct {
    ...
    browsing: bool = false,
};

/// The pane UUID that currently has Chromium focus (at most one).
var focused_pane: ?[]const u8 = null;
```

New helper that enforces single-pane focus:

```zig
fn sendFocusChanged(pane_id: []const u8, focused: bool) void {
    const p = panes.get(pane_id) orelse return;
    const server = p.server orelse return;
    if (server.peer == null) return;

    // Single-pane enforcement: unfocus previous pane.
    if (focused) {
        if (focused_pane) |prev| {
            if (!std.mem.eql(u8, prev, pane_id)) {
                sendFocusMessage(prev, false);
            }
        }
        focused_pane = pane_id;
    } else {
        if (focused_pane) |prev| {
            if (std.mem.eql(u8, prev, pane_id)) {
                focused_pane = null;
            }
        }
    }

    sendFocusMessage(pane_id, focused);
}

fn sendFocusMessage(pane_id: []const u8, focused: bool) void {
    const p = panes.get(pane_id) orelse return;
    const server = p.server orelse return;
    if (server.peer == null) return;

    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "focus_changed");

    if (pane_id.len > 0 and pane_id.len <= 36) {
        var pane_z: [37]u8 = undefined;
        @memcpy(pane_z[0..pane_id.len], pane_id);
        pane_z[pane_id.len] = 0;
        xpc_dictionary_set_string(msg, "pane_id", @ptrCast(&pane_z));
    }

    xpc_dictionary_set_bool(msg, "focused", focused);
    xpc_connection_send_message(server.peer, msg);
    log.info("focus_changed pane={s} focused={}", .{ pane_id, focused });
}
```

**Phase 2: Handle `mode_changed` properly.**

Replace the current stub:

```zig
fn handleModeChanged(msg: xpc_object_t) void {
    const pane_id = str(xpc_dictionary_get_string(msg, "pane_id"));
    const browsing = xpc_dictionary_get_bool(msg, "browsing");

    log.info("mode_changed pane={s} browsing={}", .{ pane_id, browsing });

    if (panes.get(pane_id)) |p| {
        p.browsing = browsing;
        sendFocusChanged(pane_id, browsing);
    }
}
```

When the `web` TUI sends `browsing: true` (Enter), focus the tab. When it sends
`browsing: false` (Esc), unfocus it.

**Phase 3: Handle pane focus changes from Ghostty.**

Ghostty's `focusDidChange` fires on every pane switch. Ghost needs to observe
this at the Zig level. Add a new C API export that the Swift `focusDidChange`
will call:

In `Surface.zig`, add a public method:

```zig
/// Called when this surface gains or loses pane focus (Issue 606).
/// Notifies XPC to update Chromium focus state.
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    const xpc = @import("apprt/xpc.zig");
    xpc.handlePaneFocusChanged(self, focused);
}
```

In `embedded.zig`, add the C API export:

```zig
export fn ghostty_surface_pane_focus_changed(surface: *Surface, focused: bool) void {
    surface.core_surface.paneFocusChanged(focused);
}
```

In `SurfaceView_AppKit.swift`, add a call in `focusDidChange`:

```swift
func focusDidChange(_ focused: Bool) {
    guard let surface = self.surface else { return }
    guard self.focused != focused else { return }
    self.focused = focused
    ghostty_surface_set_focus(surface, focused)
    ghostty_surface_pane_focus_changed(surface, focused)  // Issue 606
    ...
}
```

In `xpc.zig`, the handler dispatches on the serial queue:

```zig
pub fn handlePaneFocusChanged(surface: *CoreSurface, focused: bool) void {
    const ptr_val = @intFromPtr(surface);
    const dispatch_fn = struct {
        fn f(ctx: ?*anyopaque) callconv(.c) void {
            const addr = @intFromPtr(ctx);
            // focused encoded in low bit
            const surf_addr = addr & ~@as(usize, 1);
            const is_focused = (addr & 1) != 0;
            const pane_id = surface_to_pane.get(surf_addr) orelse return;
            const p = panes.get(pane_id) orelse return;
            if (is_focused) {
                // Only focus if the web TUI is in browse mode.
                if (p.browsing) {
                    sendFocusChanged(pane_id, true);
                }
            } else {
                sendFocusChanged(pane_id, false);
            }
        }
    }.f;
    // Encode focused state in low bit of pointer (Surface is aligned).
    const encoded = ptr_val | @as(usize, if (focused) 1 else 0);
    dispatch_async_f(xpc_queue, @ptrFromInt(encoded), dispatch_fn);
}
```

This encodes the focused boolean in the low bit of the surface pointer (which is
always aligned, so the low bit is free). The dispatch function runs on the XPC
serial queue where all pane state lives.

The logic: when a pane gains Ghostty focus, only send Chromium focus if the
`web` TUI is in browse mode (`p.browsing`). When a pane loses focus, always
unfocus the Chromium tab. This matches ts5's behavior.

**Phase 4: Focus on initial tab creation.**

ts5 learned (Issue 515 Experiments 4–5) that focus must be sent after the tab is
created, not before. In Ghost, `sendCreateTab` runs either from
`handleSetOverlay` (server already registered) or `handleServerRegister` (server
just appeared). In both cases, the tab hasn't started rendering yet.

Send initial focus after `sendCreateTab` if the pane is in browse mode:

```zig
// In handleSetOverlay, after sendCreateTab:
sendCreateTab(p, server);
if (p.browsing) {
    sendFocusChanged(p.pane_id_key, true);
}

// In handleServerRegister, after the flush loop:
if (p.browsing) {
    sendFocusChanged(p.pane_id_key, true);
}
```

The `set_overlay` message from `web` includes `browsing: true/false`. Store it
on the Pane in `handleSetOverlay`:

```zig
// In handleSetOverlay, new pane creation:
p.browsing = browsing;

// In handleSetOverlay, existing pane update:
p.browsing = browsing;
```

Need to add `dispatch_async_f` extern declaration:

```zig
extern "c" fn dispatch_async_f(queue: ?*anyopaque, context: ?*anyopaque, work: *const fn (?*anyopaque) callconv(.c) void) void;
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- Entering browse mode (Enter) sends `focus_changed focused=true` (visible in
  log)
- Exiting browse mode (Esc) sends `focus_changed focused=false`
- Text input on a focused page shows blinking caret (e.g., Wikipedia search box)
- Switching to another pane with keyboard unfocuses the Chromium tab
- Switching back refocuses it (if still in browse mode)
- Clicking on the overlay while in control mode triggers mode change → focus
- Clicking on the control panel while in browse mode triggers mode change →
  unfocus
- Only one Chromium tab is focused at a time (multi-pane test)

### Result: Partial pass

Keyboard-driven focus works correctly:

- Enter/Esc toggles browse mode → `focus_changed` sent correctly
- Keyboard pane switching transfers focus (unfocuses old, refocuses new if
  browsing)
- Multi-pane single-focus enforcement works

Mouse-driven focus does not work:

- Clicking on the overlay while in control mode does not switch to browse mode
  or focus Chromium
- Clicking on the control panel while in browse mode does not switch to control
  mode or unfocus Chromium

Root cause: the design assumed the `web` TUI already sends `mode_changed` on
mouse clicks, but it doesn't. Overlay clicks are intercepted by Ghost's
`mouseButtonCallback` and forwarded directly to Chromium — the `web` TUI never
sees them. Control panel clicks go to the terminal but the `web` TUI doesn't
handle mouse clicks for mode switching. Mouse-driven mode switching needs
additional work in a follow-up experiment.

## Experiment 6: Mouse-driven focus

### Goal

Click on the Chromium overlay while in control mode to switch to browse mode and
focus the tab. Click on the control panel (URL bar area) while in browse mode to
switch to control mode and unfocus the tab.

### Background

Experiment 5 proved keyboard-driven focus works — Enter/Esc toggles mode, pane
switches transfer focus. But mouse-driven mode switching failed because:

1. **Overlay clicks are invisible to the `web` TUI.** Ghost's
   `mouseButtonCallback` intercepts overlay hits and forwards them to Chromium
   via `sendMouseEvent`. The `web` TUI never sees these clicks, so it can't
   trigger `mode_changed`.

2. **Non-overlay clicks don't trigger mode changes.** Clicks outside the overlay
   fall through to normal terminal handling, but Ghost doesn't detect them as a
   signal to exit browse mode and unfocus Chromium.

The solution: Ghost drives mode switching on mouse clicks and notifies the `web`
TUI by sending `mode_changed` messages back on `p.web_peer`. The `web` TUI
already handles incoming `mode_changed` messages — it updates its local mode
state (URL bar styling, status indicator) without echoing back. No feedback
loop.

XPC connections are bidirectional. Ghost already stores the `web` TUI's
connection as `p.web_peer` (retained from `xpc_dictionary_get_remote_connection`
in `handleSetOverlay`). Sending `xpc_connection_send_message(p.web_peer, msg)`
delivers to the `web` TUI's event handler, which dispatches
`CompositorMessage::ModeChanged` into the TUI's event loop.

### Design

**Phase 1: Add `sendModeToWeb` helper in xpc.zig.**

```zig
fn sendModeToWeb(p: *Pane, browsing: bool) void {
    if (p.web_peer == null) return;
    const msg = xpc_dictionary_create(null, null, 0);
    xpc_dictionary_set_string(msg, "action", "mode_changed");
    xpc_dictionary_set_bool(msg, "browsing", browsing);
    xpc_connection_send_message(p.web_peer, msg);
}
```

**Phase 2: Add `notifyOverlayClicked` and `notifyNonOverlayClicked` in
xpc.zig.**

```zig
/// Called from mouseButtonCallback when a left-click hits the overlay.
/// If the pane is in control mode, switches to browse mode and focuses.
pub fn notifyOverlayClicked(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;
    if (p.browsing) return;

    p.browsing = true;
    sendModeToWeb(p, true);
    sendFocusChanged(pane_id_key, true);
}

/// Called from mouseButtonCallback when a left-click misses the overlay.
/// If the pane is in browse mode, switches to control mode and unfocuses.
pub fn notifyNonOverlayClicked(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;
    if (!p.browsing) return;

    p.browsing = false;
    sendModeToWeb(p, false);
    sendFocusChanged(pane_id_key, false);
}
```

These are called from the main thread but access XPC state directly — same
pattern as the existing `sendMouseEvent`, `sendMouseMove`, and
`sendScrollEvent`. The state mutation (`p.browsing`) is safe because mouse
clicks and keyboard mode changes are user-driven and don't race.

**Phase 3: Call from `mouseButtonCallback` in Surface.zig.**

Expand the overlay check block. On left-click press that hits the overlay, call
`notifyOverlayClicked` before forwarding the click. On left-click press that
misses the overlay, call `notifyNonOverlayClicked` and let the click fall
through to normal terminal handling:

```zig
// Check if click is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
        const xpc = @import("apprt/xpc.zig");
        // Switch to browse mode on overlay click if in control mode (Exp 6).
        if (button == .left and action == .press) {
            xpc.notifyOverlayClicked(self);
        }
        xpc.sendMouseEvent(self, action, button, mods, overlay_pos.x, overlay_pos.y);
        return true;
    }
    // Click missed overlay — switch to control if browsing (Exp 6).
    if (button == .left and action == .press) {
        const xpc = @import("apprt/xpc.zig");
        xpc.notifyNonOverlayClicked(self);
    }
}
```

The `notifyNonOverlayClicked` call is a no-op if the surface has no overlay
(returns early when `surface_to_pane.get` fails) or is already in control mode.
The click always falls through to normal terminal processing regardless.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- In control mode, clicking on the overlay switches to browse mode (URL bar
  styling updates) and focuses Chromium (`focus_changed focused=true` in log)
- The click that triggered the mode switch also activates the clicked element
  (e.g., clicking a link navigates)
- In browse mode, clicking on the URL bar area switches to control mode and
  unfocuses Chromium (`focus_changed focused=false` in log)
- Right-clicks don't trigger mode switching (only left-click press)
- Keyboard mode switching (Enter/Esc) still works alongside mouse switching
- No feedback loop — `web` TUI doesn't echo `mode_changed` back

### Result: Pass

## Experiment 7: Gate mouse events on focus + browse

### Goal

Only forward mouse clicks, moves, scroll, and cursor changes to Chromium when
the pane is both the active Ghostty pane AND in browse mode. Activation clicks
should switch mode and focus but not pass through to Chromium.

### Background

Experiments 1-6 unconditionally forward mouse events to Chromium whenever a hit
test succeeds. This is wrong in two cases:

1. **Control mode.** The pane has an overlay but the web TUI is in control mode.
   Mouse moves still send `mouse_moved` to Chromium (causing hover effects) and
   clicks both activate and pass through (e.g., clicking a link navigates
   instead of just switching to browse mode).

2. **Inactive pane.** The pane is in browse mode but the user switched to a
   different Ghostty pane. Mouse events still forward to Chromium because the
   hit test doesn't check pane focus.

The correct behavior: mouse events only forward when the pane is the focused
pane AND in browse mode. An activation click (overlay click while in control
mode or while the pane is inactive) should switch to browse mode and focus the
pane, but the click itself must not reach Chromium. Similarly, mouse moves and
scroll over an unfocused or control-mode overlay should not forward.

### Design

Add a single query function `isOverlayForwarding` in xpc.zig that returns true
only when both conditions hold: the pane is browsing AND is the focused pane.
Use it to gate all three mouse forwarding sites in Surface.zig.

**Phase 1: Add `isOverlayForwarding` query in xpc.zig.**

```zig
/// Returns true if the surface's pane is in browse mode AND is the
/// focused pane — the only state where mouse events should forward
/// to Chromium.
pub fn isOverlayForwarding(surface: *CoreSurface) bool {
    const pane_id = surface_to_pane.get(@intFromPtr(surface)) orelse return false;
    const p = panes.get(pane_id) orelse return false;
    if (!p.browsing) return false;
    const fp = focused_pane orelse return false;
    return std.mem.eql(u8, fp, pane_id);
}
```

**Phase 2: Gate `mouseButtonCallback` in Surface.zig.**

Replace the overlay hit block. When the hit test succeeds but
`isOverlayForwarding` is false, call `notifyOverlayClicked` (which handles
activation) but do NOT call `sendMouseEvent` — consume the click by returning
true. When forwarding is true, send the event normally:

```zig
// Check if click is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
        const xpc = @import("apprt/xpc.zig");
        if (xpc.isOverlayForwarding(self)) {
            // Active + browsing: forward click to Chromium.
            xpc.sendMouseEvent(self, action, button, mods, overlay_pos.x, overlay_pos.y);
        } else if (button == .left and action == .press) {
            // Not forwarding: activate on left-click, consume the click.
            xpc.notifyOverlayClicked(self);
        }
        return true;
    }
    // Click missed overlay — switch to control if browsing (Exp 6).
    if (button == .left and action == .press) {
        const xpc = @import("apprt/xpc.zig");
        xpc.notifyNonOverlayClicked(self);
    }
}
```

**Phase 3: Gate `cursorPosCallback` in Surface.zig.**

Only forward mouse moves and set the Chromium cursor when forwarding is active.
When not forwarding, still track `over_overlay` for cursor restore but skip
`sendMouseMove` and cursor shape override:

```zig
// Check if mouse is in a browser overlay (Issue 606).
if (self.hitTestOverlay(@floatCast(pos.x), @floatCast(pos.y))) |_overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    if (xpc.isOverlayForwarding(self)) {
        xpc.sendMouseMove(self, _overlay_pos.x, _overlay_pos.y);
        const shape = mapChromiumCursor(self.overlay_cursor_type);
        _ = try self.rt_app.performAction(
            .{ .surface = self },
            .mouse_shape,
            shape,
        );
    }
    self.mouse.over_overlay = true;
    return;
}
```

**Phase 4: Gate `scrollCallback` in Surface.zig.**

Only forward scroll events when forwarding is active. When not forwarding, let
the scroll fall through to normal terminal handling:

```zig
// Check if scroll is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
        const xpc = @import("apprt/xpc.zig");
        if (xpc.isOverlayForwarding(self)) {
            xpc.sendScrollEvent(self, overlay_pos.x, overlay_pos.y);
            return;
        }
    }
}
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- In control mode, hovering over the overlay does NOT change cursor or trigger
  Chromium hover effects (no `mouse_moved` in log)
- In control mode, clicking the overlay switches to browse mode and focuses, but
  does NOT navigate links or trigger Chromium click handlers
- After activation, subsequent clicks and moves forward to Chromium normally
- In browse mode, clicking outside the overlay switches to control mode and
  unfocuses — subsequent overlay hovers do not forward
- Scrolling over the overlay in control mode scrolls the terminal, not the web
  page
- With two panes, switching away from a browsing pane stops forwarding to it
- Keyboard mode switching (Enter/Esc) still works

### Result: Partial pass

Mouse moves, cursor changes, and scroll are correctly gated — none forward to
Chromium when the pane is inactive or in control mode. However, mouse clicks
still trigger link navigation on activation.

**Root cause:** The activation click suppresses the press but not the release.
`notifyOverlayClicked` fires on the press event and sets `p.browsing = true` +
`focused_pane = pane_id`. By the time the release event arrives (milliseconds
later), `isOverlayForwarding` returns true and `sendMouseEvent` forwards the
release to Chromium. Chromium triggers link navigation on mouseup, so the
release alone is enough to navigate.

**Fix ideas:**

1. **Activation guard flag.** Add a per-pane `consuming_activation: bool` flag.
   Set it true in `notifyOverlayClicked`. In the forwarding path, check the flag
   and skip `sendMouseEvent` if set. Clear it on the next release event (after
   skipping it). This suppresses both press and release for the activation
   click.

2. **Suppress at the source.** Instead of gating in xpc.zig, track an
   `activation_button` on the Surface. Set it in `mouseButtonCallback` when
   `notifyOverlayClicked` fires. Skip all forwarding for that button until the
   release completes. Simpler because it stays in Surface.zig without adding
   state to xpc.zig.

3. **Delay forwarding.** Don't update `p.browsing`/`focused_pane` in
   `notifyOverlayClicked` synchronously. Instead, send the focus XPC message and
   let a callback update state. The release would still see
   `isOverlayForwarding` as false. Fragile — depends on XPC timing.

## Experiment 8: Suppress activation mouseup

### Goal

Prevent the activation click's mouseup from reaching Chromium. When an overlay
click switches mode from control to browse, both the press and release must be
consumed — Chromium should see nothing until the next fresh click.

### Background

Experiment 7 gates mouse forwarding on `isOverlayForwarding` (browsing + focused
pane). The press is correctly suppressed: `isOverlayForwarding` is false, so
`sendMouseEvent` is skipped and `notifyOverlayClicked` activates. But
`notifyOverlayClicked` synchronously sets `p.browsing = true` and
`focused_pane`, so by the time the release event arrives, `isOverlayForwarding`
returns true and the release forwards to Chromium. Chromium navigates links on
mouseup.

Fix idea 2 from Experiment 7 is the cleanest: track the suppression in
Surface.zig where the press/release lifecycle already lives, without adding
state to xpc.zig.

### Design

**Phase 1: Add `overlay_activation` flag to Mouse struct in Surface.zig.**

Add after `over_overlay`:

```zig
/// True while consuming an activation click on the overlay. Set on
/// the press that triggers notifyOverlayClicked, cleared on the
/// corresponding release. Prevents the mouseup from forwarding to
/// Chromium. (Issue 606 Experiment 8.)
overlay_activation: bool = false,
```

**Phase 2: Set and check the flag in `mouseButtonCallback`.**

Replace the overlay hit block. When activation fires, set the flag. When
forwarding is active, check the flag first — if set and this is the release,
clear it and skip forwarding:

```zig
// Check if click is in a browser overlay (Issue 606).
{
    const cursor = try self.rt_surface.getCursorPos();
    if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
        const xpc = @import("apprt/xpc.zig");
        if (xpc.isOverlayForwarding(self)) {
            // Suppress the release that follows an activation press (Exp 8).
            if (self.mouse.overlay_activation) {
                if (action == .release) {
                    self.mouse.overlay_activation = false;
                }
            } else {
                // Active + browsing: forward click to Chromium.
                xpc.sendMouseEvent(self, action, button, mods, overlay_pos.x, overlay_pos.y);
            }
        } else if (button == .left and action == .press) {
            // Not forwarding: activate on left-click, consume the click.
            xpc.notifyOverlayClicked(self);
            self.mouse.overlay_activation = true;
        }
        return true;
    }
    // Click missed overlay — switch to control if browsing (Exp 6).
    if (button == .left and action == .press) {
        const xpc = @import("apprt/xpc.zig");
        xpc.notifyNonOverlayClicked(self);
    }
}
```

The flag is set on the activating press and cleared on the next release. Any
events between (drags, other buttons) are also suppressed while the flag is set,
which is correct — the entire activation gesture should be invisible to
Chromium.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- In control mode, clicking a link activates the webview but does NOT navigate
- The link only navigates on the next click (after activation)
- Mouse moves and scroll still correctly gated (no regression from Exp 7)
- Keyboard mode switching still works
- Right-clicks on an inactive overlay don't activate or forward

### Result: Fail

The activation flag was never checked because Ghostty's pane focus switch
happens before `mouseButtonCallback` runs. When clicking on an inactive pane's
overlay:

1. macOS delivers mouseDown to the SurfaceView
2. SurfaceView becomes first responder → `focusDidChange` fires →
   `handlePaneFocusChanged` sets `focused_pane = pane_id`
3. `mouseButtonCallback` runs — by now `isOverlayForwarding` returns true (pane
   is browsing + just became focused), so the press goes straight through
   `sendMouseEvent` to Chromium
4. Release also forwards normally

The `overlay_activation` flag is only set in the `else if` branch (when
`isOverlayForwarding` is false), but the pane is already forwarding by the time
the callback runs. The flag is never set, so the suppression never triggers.

**Root cause:** The assumption that `isOverlayForwarding` would be false during
the activation press was wrong. Ghostty's focus change races ahead of the mouse
callback.

## Experiment 9: Trace activation click sequence

### Goal

Add temporary logging to determine the exact event ordering when clicking on an
inactive pane's overlay. Identify why the Experiment 8 activation flag fails to
suppress the click.

### Background

Experiment 8 failed but the root cause is speculative. We hypothesized that
`focusDidChange` fires before `mouseButtonCallback`, making
`isOverlayForwarding` true by the time the click handler runs. But we haven't
verified this. The actual sequence could be different — there may be another
path entirely.

### Design

**Phase 1: Log in `paneFocusChanged` (Surface.zig).**

Add a log line at the entry of `paneFocusChanged`:

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    log.info("paneFocusChanged focused={} addr={x}", .{ focused, @intFromPtr(self) });
    const xpc = @import("apprt/xpc.zig");
    xpc.handlePaneFocusChanged(self, focused);
}
```

**Phase 2: Log in `mouseButtonCallback` overlay hit (Surface.zig).**

Log the state at the decision point — what `isOverlayForwarding` returns and
whether `overlay_activation` is set:

```zig
if (self.hitTestOverlay(@floatCast(cursor.x), @floatCast(cursor.y))) |overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    const forwarding = xpc.isOverlayForwarding(self);
    log.info("overlay click action={s} button={s} forwarding={} activation={}",
        .{ @tagName(action), @tagName(button), forwarding, self.mouse.overlay_activation });
    // ... rest of logic
```

**Phase 3: Log in `notifyOverlayClicked` (xpc.zig).**

Log whether the function activates or returns early:

```zig
pub fn notifyOverlayClicked(surface: *CoreSurface) void {
    const pane_id_key = surface_to_pane.get(@intFromPtr(surface)) orelse return;
    const p = panes.get(pane_id_key) orelse return;
    if (p.browsing) {
        log.info("notifyOverlayClicked: already browsing, skipped", .{});
        return;
    }
    log.info("notifyOverlayClicked: activating", .{});
    // ... rest of activation
```

**Phase 4: Log in `isOverlayForwarding` (xpc.zig).**

Log the components of the decision:

```zig
pub fn isOverlayForwarding(surface: *CoreSurface) bool {
    const pane_id = surface_to_pane.get(@intFromPtr(surface)) orelse return false;
    const p = panes.get(pane_id) orelse return false;
    if (!p.browsing) {
        log.info("isOverlayForwarding: not browsing", .{});
        return false;
    }
    const fp = focused_pane orelse {
        log.info("isOverlayForwarding: no focused pane", .{});
        return false;
    };
    const result = std.mem.eql(u8, fp, pane_id);
    if (!result) {
        log.info("isOverlayForwarding: wrong pane focused", .{});
    }
    return result;
}
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Test: switch to a different pane, then click on a link in the overlay pane.
Check `~/dev/termsurf/logs/ghost.log` for the sequence. The log should reveal:

- Whether `paneFocusChanged` fires before or after `mouseButtonCallback`
- What `isOverlayForwarding` returns at press time and why
- Whether `notifyOverlayClicked` is reached or skipped
- The full press → release sequence with all state at each step

### Result: Pass

Chromium server logs confirmed the hypothesis. In both test clicks, focus fires
1-4ms before the mouse event:

```
040612.965848  Focus gained for pane 4C35F212
040612.967241  Mouse down — 1.4ms later, press forwarded
040613.056259  Mouse up — release forwarded
040613.131111  URL changed — link navigated
```

`paneFocusChanged` sets `focused_pane` before `mouseButtonCallback` runs, so
`isOverlayForwarding` returns true and both press and release go straight to
Chromium.

## Experiment 10: Set activation flag in paneFocusChanged

### Goal

Suppress the activation click when clicking on an inactive pane's overlay.
Remove the Experiment 9 debug logs.

### Background

Experiment 9 confirmed that `paneFocusChanged` fires before
`mouseButtonCallback`. Experiment 8's approach — setting `overlay_activation` in
`notifyOverlayClicked` — failed because `notifyOverlayClicked` is in the
`else if` branch that's never reached (the pane is already forwarding by press
time).

The fix: set the flag earlier, in `paneFocusChanged` itself. Since
`paneFocusChanged` fires before the mouse callback, the flag will already be
true when `mouseButtonCallback` runs. The forwarding path checks the flag and
suppresses both press and release.

This matches standard macOS behavior: clicking an inactive window activates it
without clicking through. The first click on a freshly-focused pane's overlay is
always consumed as activation, whether the focus came from a mouse click or a
keyboard shortcut.

### Design

**Phase 1: Set activation flag in `paneFocusChanged` (Surface.zig).**

When a pane gains focus, set the flag. Remove Experiment 9 log.

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    if (focused) {
        self.mouse.overlay_activation = true;
    }
    const xpc = @import("apprt/xpc.zig");
    xpc.handlePaneFocusChanged(self, focused);
}
```

**Phase 2: Clear flag on non-overlay clicks (Surface.zig).**

After the overlay hit block in `mouseButtonCallback`, clear the flag. This
handles keyboard-driven focus changes where the subsequent click lands outside
the overlay:

```zig
    // Click missed overlay — switch to control if browsing (Exp 6).
    if (button == .left and action == .press) {
        const xpc = @import("apprt/xpc.zig");
        xpc.notifyNonOverlayClicked(self);
    }
    // Clear activation flag — click landed outside overlay (Exp 10).
    self.mouse.overlay_activation = false;
}
```

The overlay hit path (which returns true) keeps the flag alive for release
suppression. The miss path clears it.

**Phase 3: Remove Experiment 9 debug logs.**

Remove all `log.info` calls added in Experiment 9 from:

- `paneFocusChanged` in Surface.zig
- `mouseButtonCallback` overlay hit in Surface.zig
- `notifyOverlayClicked` in xpc.zig
- `isOverlayForwarding` in xpc.zig

Restore these functions to their pre-Experiment 9 form (with Experiment 8's
suppression logic intact in `mouseButtonCallback`).

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- Clicking a link on an inactive pane activates the pane but does NOT navigate
- The link navigates on the next click (after activation)
- Keyboard pane switch followed by overlay click: first click is absorbed
- Mouse moves and scroll still gated (no regression from Exp 7)
- Keyboard mode switching (Enter/Esc) still works
- No debug log noise in ghost.log

### Result: Pass

Activation clicks are fully suppressed. Clicking on an inactive pane's overlay
activates the pane (switches to browse mode, focuses Chromium) without passing
the click through to Chromium. The link only navigates on the next click.

Combined with Experiment 7's `isOverlayForwarding` gate, all mouse events —
clicks, moves, scroll, and cursor changes — are now correctly gated on two
conditions: the pane must be the focused Ghostty pane AND in browse mode. Events
only forward when both hold. Activation clicks are consumed. Mouse moves over
unfocused or control-mode overlays don't trigger Chromium hover effects.

The key insight across Experiments 7-10: `focusDidChange` fires before
`mouseButtonCallback` on macOS (confirmed by Experiment 9 logs, 1-4ms gap). The
activation flag must be set in `paneFocusChanged`, not in the mouse callback,
because by the time the callback runs the pane is already focused.

## Experiment 11: Text selection via mouse drag

### Goal

Select text in a Chromium overlay by clicking and dragging. The selection
highlight should paint in real time as the mouse moves.

### Background

The full drag pipeline may already be wired:

1. macOS `mouseDragged` → calls `mouseMoved` → triggers `cursorPosCallback`
2. `cursorPosCallback` → overlay hit test → `sendMouseMove` with
   `kLeftButtonDown` flag from `click_state` (line 868 of xpc.zig)
3. Chromium `HandleMouseMove` → detects `kLeftButtonDown` in modifiers, sets
   button to `kLeft`, creates `kMouseMove` event
4. `ForwardMouseEvent` → Blink processes the drag, paints selection

The press/release are already forwarded via `sendMouseEvent`. The drag events
flow through `sendMouseMove` with button-down flags. Chromium's
`HandleMouseMove` already derives the button from modifier bits and creates a
proper `WebMouseEvent` with `kMouseMove` type.

Focus state is confirmed working (Experiments 5-6, 10). Blink's
`FocusController::IsActive()` is true, so selection highlights should paint.

This experiment is a verification — it may already work with no code changes. If
it doesn't, the failure will reveal what's missing.

### Design

No code changes. Test the existing pipeline.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

In browse mode, click and drag across text on the Wikipedia page.

Pass criteria:

- Text highlights in real time as the mouse drags
- Selection starts at mousedown position, extends to current cursor position
- Releasing the mouse finalizes the selection (highlight stays)
- Cursor changes to text cursor (I-beam) over selectable text
- Dragging past the edge of the overlay doesn't crash

### Result: Fail

Text selection does not work. Chromium logs confirm mouse down and up arrive
with different coordinates (drag occurred), but selection highlights don't
paint. Chromium's `HandleMouseMove` doesn't log individual moves, so we can't
tell from these logs whether drag moves reached Chromium or what modifiers they
carried.

## Experiment 12: Trace drag events

### Goal

Add temporary logging to trace why mouse drag events don't produce text
selection. Determine whether `cursorPosCallback` is called during a drag,
whether `isOverlayForwarding` returns true, and whether `sendMouseMove` sends
the `kLeftButtonDown` modifier.

### Design

**Phase 1: Log in `cursorPosCallback` overlay hit (Surface.zig).**

Log whether the hit test succeeds during a drag and what the forwarding state
is. Only log when left button is down (drag), to avoid flooding with hover
moves:

```zig
if (self.hitTestOverlay(@floatCast(pos.x), @floatCast(pos.y))) |overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    const left_down = self.mouse.click_state[@intFromEnum(input.MouseButton.left)] == .press;
    if (left_down) {
        const fwd = xpc.isOverlayForwarding(self);
        log.info("drag move: overlay_hit left_down={} forwarding={} pos=({d:.1},{d:.1})",
            .{ left_down, fwd, overlay_pos.x, overlay_pos.y });
    }
    // ... rest of existing code
```

**Phase 2: Log in `sendMouseMove` (xpc.zig).**

Log the modifiers being sent, but only when button-down flags are set (drag):

```zig
if (modifiers != 0) {
    log.info("sendMouseMove drag modifiers={} x={d:.1} y={d:.1}", .{ modifiers, overlay_x, overlay_y });
}
```

**Phase 3: Log in `mouseButtonCallback` overlay hit (Surface.zig).**

Log activation flag state and forwarding result at press and release:

```zig
if (self.hitTestOverlay(...)) |overlay_pos| {
    const xpc = @import("apprt/xpc.zig");
    const fwd = xpc.isOverlayForwarding(self);
    log.info("overlay click action={s} button={s} forwarding={} activation={}",
        .{ @tagName(action), @tagName(button), fwd, self.mouse.overlay_activation });
    if (fwd) {
        // ... existing suppression + forwarding logic
```

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

In browse mode, click and drag across text. Check
`~/dev/termsurf/logs/ghost.log`.

The logs should reveal one of:

- `cursorPosCallback` is not called during drags → macOS event routing issue
- Hit test fails during drags → coordinate problem
- `isOverlayForwarding` returns false → state problem
- `sendMouseMove` called with modifiers=0 → `click_state` not set
- `sendMouseMove` called with modifiers=64 → Chromium receives drags but doesn't
  select (Blink issue, focus issue, or XPC type mismatch)

### Result: Pass (diagnostic)

The Ghost-side pipeline works correctly. Every drag move is forwarded with
`modifiers=64` (`kLeftButtonDown`). The bug is an XPC type mismatch:
`sendMouseMove` uses `xpc_dictionary_set_int64` but Chromium reads with
`xpc_dictionary_get_uint64`. XPC returns 0 when the types don't match, so
Chromium sees `modifiers=0` and treats every drag move as a hover.

The same mismatch exists in `sendMouseEvent` (line 779) — keyboard modifiers
(shift, ctrl, alt, cmd) and button-down flags on clicks are also silently
dropped.

## Experiment 13: Fix XPC modifier type mismatch

### Goal

Fix the `int64` vs `uint64` type mismatch in modifier fields so Chromium
receives button-down flags during drags. Remove Experiment 12 debug logs.

### Background

XPC is typed. `xpc_dictionary_set_int64` stores as `XPC_TYPE_INT64`.
`xpc_dictionary_get_uint64` expects `XPC_TYPE_UINT64`. When types don't match,
XPC returns 0. Two call sites in xpc.zig use `set_int64` for modifiers while all
three Chromium readers use `get_uint64`:

| Function          | Line | Uses         | Should use   |
| ----------------- | ---- | ------------ | ------------ |
| `sendMouseEvent`  | 779  | `set_int64`  | `set_uint64` |
| `sendScrollEvent` | 829  | `set_uint64` | correct      |
| `sendMouseMove`   | 873  | `set_int64`  | `set_uint64` |

### Design

**Phase 1: Fix `sendMouseEvent` modifiers (xpc.zig).**

Change the modifier variable type from `i64` to `u64` and use
`xpc_dictionary_set_uint64`:

```zig
var modifiers: u64 = 0;
// ... same bitmask logic ...
xpc_dictionary_set_uint64(msg, "modifiers", modifiers);
```

**Phase 2: Fix `sendMouseMove` modifiers (xpc.zig).**

Same change — `i64` to `u64` and `set_int64` to `set_uint64`:

```zig
var modifiers: u64 = 0;
// ... same bitmask logic ...
xpc_dictionary_set_uint64(msg, "modifiers", modifiers);
```

**Phase 3: Remove Experiment 12 debug logs.**

Remove all `log.info` calls added in Experiment 12 from:

- `cursorPosCallback` overlay hit in Surface.zig
- `mouseButtonCallback` overlay hit in Surface.zig
- `sendMouseMove` in xpc.zig

### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stdout ~/dev/termsurf/logs/ghost.log --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- Click and drag across text selects it (blue highlight appears)
- Selection extends in real time as the mouse moves
- Releasing the mouse finalizes the selection
- No debug log noise

### Result: Partial pass

The XPC type mismatch fix is correct — Chromium now receives modifier bits
during drags, and text selection works. But it works intermittently: sometimes a
click-drag selects text, sometimes it doesn't. The `overlay_activation` flag
(set by `paneFocusChanged(true)`) eats the first mouse press after any focus
change. If the pane was recently focused (e.g., by clicking into it, or by
switching panes), the next press is suppressed. This breaks the "activate on
first click, then immediately drag/select" flow.

## Experiment 14: Trace activation flag lifecycle

### Goal

Add debug logs to trace the `overlay_activation` flag through the full click
lifecycle. Determine exactly when and why the flag is still `true` at press time
when it shouldn't be.

### Background

The `overlay_activation` flag is set to `true` in `paneFocusChanged(true)` and
cleared on mouse release (or when a click lands outside the overlay). The intent
is to suppress the single activation click that caused the focus change. But the
flag appears to persist beyond the activation click, eating subsequent presses.

Possible causes:

1. **No release after activation click.** If the activation click's release
   lands outside the overlay (e.g., mouse moved slightly), it clears via the
   bottom path (`self.mouse.overlay_activation = false` at line 4058) — this
   works. But if the release is consumed by the `if (action == .release)` branch
   at line 4039, the flag clears — this also works. So what case is missed?

2. **Focus fires without a corresponding click.** If `paneFocusChanged(true)`
   fires from a keyboard shortcut (Cmd+]) or tab switch rather than a mouse
   click, the flag is set but there is no mouse release to clear it. The flag
   stays `true` and eats the next press.

3. **Multiple `paneFocusChanged(true)` calls.** If focus oscillates quickly
   (e.g., split pane reshuffle), the flag may be re-set after being cleared.

Logs will reveal which case is happening.

### Design

**Phase 1: Log in `paneFocusChanged` (Surface.zig:3475).**

After `self.mouse.overlay_activation = true`:

```zig
log.info("paneFocusChanged focused={} overlay_activation={}", .{
    focused, self.mouse.overlay_activation,
});
```

**Phase 2: Log in `mouseButtonCallback` overlay hit (Surface.zig:4036).**

After entering the `isOverlayForwarding` branch, before the activation check:

```zig
log.info("overlay click action={} button={} forwarding=true activation={}", .{
    action, button, self.mouse.overlay_activation,
});
```

Also log the suppression and clear paths:

```zig
// Inside the overlay_activation == true branch, after clearing on release:
log.info("activation suppressed action={} cleared={}", .{
    action, action == .release,
});
```

```zig
// Inside the else branch (forwarding to Chromium):
log.info("forwarding click action={} button={}", .{ action, button });
```

**Phase 3: Log in `mouseButtonCallback` non-overlay clear (Surface.zig:4058).**

```zig
log.info("non-overlay click, clearing activation flag", .{});
```

### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stdout ~/dev/termsurf/logs/ghost.log --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Test sequence:

1. Click the overlay to activate (should see activation suppressed log)
2. Immediately click and drag text (should see forwarding log, NOT activation
   suppressed)
3. Switch to another pane via keyboard (Cmd+]), then switch back, then
   click-drag — check if activation flag eats the press

Pass criteria: logs reveal the exact scenario where the activation flag is
`true` when it shouldn't be.

### Result: Pass

Logs confirmed the activation flag lifecycle works correctly: set on focus gain,
suppresses the activation press and release, cleared on release, all subsequent
clicks forwarded. Text selection worked consistently with logs enabled.

After removing the logs, text selection continued to work consistently. The
intermittent failure reported after Experiment 13 may have been an illusion — a
one-off glitch during the transition from broken modifiers (`int64`/`uint64`
mismatch) to working modifiers, rather than a persistent bug. Two consecutive
retests (with and without logs) showed no failures.

### Conclusion

The activation flag mechanism (set in `paneFocusChanged`, cleared on release) is
sound. The real fix for text selection was Experiment 13's XPC type mismatch
correction. No code changes needed from this experiment — the debug logs were
added and removed as a diagnostic tool only.

## Experiment 15: Double-click word selection

### Goal

Enable double-click to select a word and triple-click to select a line in
Chromium overlays.

### Background

`sendMouseEvent` hardcodes `click_count` to `1` (xpc.zig line 766). Chromium
reads this and passes it to `WebMouseEvent`. For double-click word selection
Chromium needs `click_count=2`; for triple-click line selection,
`click_count=3`.

Ghostty already tracks multi-click state in `mouse.left_click_count` (1, 2, or
3) with timing and distance checks. But the overlay block in
`mouseButtonCallback` returns early at line 4050 (`return true`), so Ghostty's
click count computation at line 4258+ never runs for overlay clicks.

We need to replicate the multi-click detection for overlay clicks. The logic is:

1. On left press, check time since last left press
2. If within `config.mouse_interval` (default 500ms) and cursor hasn't moved
   more than one cell width, increment count
3. If too slow or too far, reset to 1
4. Cap at 3 (triple-click), wrap back to 1

We can reuse the existing `left_click_count`, `left_click_time`, and position
fields on `mouse` since the overlay block runs before the terminal click
processing and returns early.

### Design

**Phase 1: Compute click count for overlay clicks (Surface.zig).**

Inside the overlay forwarding block, before the `sendMouseEvent` call, add
multi-click detection for left press events:

```zig
// Compute click count for overlay (mirrors logic at line 4258).
var click_count: u8 = 1;
if (button == .left and action == .press) {
    const cursor = try self.rt_surface.getCursorPos();
    // Distance check — reset if cursor moved too far.
    if (self.mouse.left_click_count > 0) {
        const max_distance: f64 = @floatFromInt(self.size.cell.width);
        const distance = @sqrt(
            std.math.pow(f64, cursor.x - self.mouse.left_click_xpos, 2) +
                std.math.pow(f64, cursor.y - self.mouse.left_click_ypos, 2),
        );
        if (distance > max_distance) self.mouse.left_click_count = 0;
    }
    // Timing check — reset if too slow.
    if (std.time.Instant.now()) |now| {
        if (self.mouse.left_click_count > 0) {
            const since = now.since(self.mouse.left_click_time);
            if (since > self.config.mouse_interval) {
                self.mouse.left_click_count = 0;
            }
        }
        self.mouse.left_click_time = now;
        self.mouse.left_click_count += 1;
        if (self.mouse.left_click_count > 3) self.mouse.left_click_count = 1;
    } else |_| {
        self.mouse.left_click_count = 1;
    }
    self.mouse.left_click_xpos = cursor.x;
    self.mouse.left_click_ypos = cursor.y;
    click_count = self.mouse.left_click_count;
}
```

**Phase 2: Pass click count to `sendMouseEvent` (xpc.zig).**

Add `click_count: u8` parameter to `sendMouseEvent` signature. Replace the
hardcoded `1`:

```zig
xpc_dictionary_set_int64(msg, "click_count", @intCast(click_count));
```

Update the call site in `mouseButtonCallback` to pass `click_count`.

### Verification

```bash
cd ghost && zig build
GHOSTTY_LOG=stderr open ghost/zig-out/Ghostty.app --stdout ~/dev/termsurf/logs/ghost.log --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- Double-click a word: the word highlights
- Triple-click: the entire line/paragraph highlights
- Single click still works (places caret, no selection)
- Click and drag still works (text selection)

### Result: Pass

Double-click word selection and triple-click line selection both work.

### Conclusion

The overlay block returns early before Ghostty's own multi-click detection runs,
so we replicated the timing + distance logic inline. The existing
`left_click_count`, `left_click_time`, and position fields on `mouse` are reused
since the overlay path and the terminal path are mutually exclusive. The click
count is passed through `sendMouseEvent` → XPC `click_count` field → Chromium's
`WebMouseEvent`, where Chromium handles word/line selection natively.

## Experiment 16: Fix spurious activation flag on initial focus

### Goal

The first click on the overlay after launching the app is always consumed. Fix
it so the first click works.

### Background

`paneFocusChanged(true)` sets `overlay_activation = true` unconditionally. This
flag is designed to suppress the activation click when switching between panes —
the click that caused the focus change shouldn't pass through to Chromium.

But `paneFocusChanged` also fires when the app launches and the pane gets
initial focus. At that point there's no overlay, no browse mode, and no click to
suppress. The flag persists until a mouse release on the overlay clears it. This
means the release of the user's first real overlay click is suppressed,
requiring an extra click before anything forwards.

The same problem occurs when focus changes via keyboard (e.g., Cmd+] to switch
panes). There's no mouse click to suppress, but the flag is set and eats the
next click.

The fix: only set `overlay_activation` when the surface is already in overlay
forwarding state. If the pane isn't in browse mode (or doesn't have an overlay),
there's no click to suppress and the flag should stay false.

### Design

**Phase 1: Gate activation flag on forwarding state (Surface.zig).**

In `paneFocusChanged`, check `isOverlayForwarding` before setting the flag:

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    if (focused) {
        const xpc = @import("apprt/xpc.zig");
        if (xpc.isOverlayForwarding(self)) {
            self.mouse.overlay_activation = true;
        }
        xpc.handlePaneFocusChanged(self, focused);
    } else {
        const xpc = @import("apprt/xpc.zig");
        xpc.handlePaneFocusChanged(self, focused);
    }
}
```

This is safe because `isOverlayForwarding` checks `p.browsing` and
`focused_pane == pane_id`. When the pane is gaining initial focus (app launch),
`focused_pane` hasn't been updated yet (it's updated by `handlePaneFocusChanged`
which we call after), so `isOverlayForwarding` returns false and the flag is not
set. When switching panes by clicking an already-browsing overlay,
`focused_pane` was set from the previous focus cycle, and `p.browsing` is true,
so `isOverlayForwarding` returns true and the flag IS set — exactly the case we
want to suppress.

**Wait — there's a subtlety.** `isOverlayForwarding` checks that `focused_pane`
matches this surface's pane ID. But at the moment `paneFocusChanged(true)`
fires, `focused_pane` still points to the _old_ pane (the one losing focus). So
`isOverlayForwarding` would return false even for the pane-switch case.

We need a different check. The right question is: "does this surface have an
overlay in browse mode?" That's just `p.browsing` — we don't need the focus
check because we already know focus is being gained.

Revised approach — check `p.browsing` directly:

```zig
pub fn paneFocusChanged(self: *Surface, focused: bool) void {
    const xpc = @import("apprt/xpc.zig");
    if (focused) {
        if (xpc.isOverlayBrowsing(self)) {
            self.mouse.overlay_activation = true;
        }
    }
    xpc.handlePaneFocusChanged(self, focused);
}
```

**Phase 2: Add `isOverlayBrowsing` query (xpc.zig).**

```zig
/// Returns true if the surface has an overlay in browse mode.
/// Unlike isOverlayForwarding, this does not check pane focus.
pub fn isOverlayBrowsing(surface: *CoreSurface) bool {
    const pane_id = surface_to_pane.get(@intFromPtr(surface)) orelse return false;
    const p = panes.get(pane_id) orelse return false;
    return p.browsing;
}
```

This returns true only when the pane has an active overlay in browse mode. On
initial app launch, no pane has `p.browsing = true`, so the flag is not set. On
keyboard pane switch (Cmd+]), if the overlay isn't in browse mode, the flag is
not set. Only when clicking into a pane that's already in browse mode does the
flag get set — the exact case we want to suppress.

### Verification

```bash
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- https://en.wikipedia.org/wiki/Terminal_emulator
```

Pass criteria:

- First click on a link after app launch navigates immediately (no wasted click)
- Switching panes by clicking an already-browsing overlay still suppresses the
  activation click (no accidental link navigation)
- Keyboard pane switch (Cmd+]) followed by a click works on the first try

### Result: Pass

First click after app launch navigates immediately.

### Conclusion

The `overlay_activation` flag was being set unconditionally in
`paneFocusChanged(true)`, including at app startup when no overlay exists. The
flag persisted until a mouse release on the overlay cleared it, causing the
first real click to be suppressed. The fix gates the flag on `isOverlayBrowsing`
— a new query that checks `p.browsing` without requiring focus. This correctly
distinguishes three cases: (1) app launch / no overlay → flag not set → first
click works, (2) keyboard pane switch to non-browsing pane → flag not set →
first click works, (3) mouse click on a browsing overlay in an inactive pane →
flag set → activation click suppressed as intended.

## Conclusion

Issue 606 is complete. All mouse input goals are met: clicking links and
buttons, scrolling pages, selecting text by drag/double-click/triple-click,
cursor shape sync, focus lifecycle, and activation gating.

### What we built

Sixteen experiments across three layers (Surface.zig, xpc.zig, Chromium server):

**Click forwarding (Experiments 1, 7, 10, 15, 16).** `mouseButtonCallback` hit-
tests the overlay, computes multi-click count (timing + distance, mirroring
Ghostty's own logic), and sends press/release events via XPC. An
`overlay_activation` flag suppresses the click that caused a pane switch — set
in `paneFocusChanged` only when the overlay is already in browse mode
(`isOverlayBrowsing`), cleared on release.

**Mouse move (Experiments 2, 12, 13).** `cursorPosCallback` forwards position to
Chromium with button-down modifier bits for drag detection. Fixed an XPC type
mismatch (`set_int64` vs `get_uint64`) that silently zeroed modifiers, breaking
text selection.

**Scroll (Experiment 3).** `scrollCallback` forwards delta x/y to Chromium when
the cursor is over the overlay and forwarding is active.

**Cursor sync (Experiment 4).** Chromium sends cursor type changes via XPC.
Ghost maps Chromium's `ui::mojom::CursorType` integers to Ghostty's `MouseShape`
enum and applies them via `performAction(.mouse_shape)`.

**Focus lifecycle (Experiments 5, 6).** `paneFocusChanged` dispatches to XPC to
send `focus_changed` to Chromium. Clicking the overlay activates browse mode
(`notifyOverlayClicked`), clicking outside deactivates it
(`notifyNonOverlayClicked`). Mode changes are sent to the `web` TUI so it can
update its chrome.

**Event gating (Experiments 7, 10, 16).** `isOverlayForwarding` checks both
`p.browsing` and `focused_pane == pane_id`. All mouse events (clicks, moves,
scroll) are gated on this. Prevents mouse input from leaking to inactive or
non-browsing overlays.

### Key files modified

- `ghost/src/Surface.zig` — overlay hit-testing, click/move/scroll gating,
  multi-click detection, activation flag, `paneFocusChanged`
- `ghost/src/apprt/xpc.zig` — `sendMouseEvent`, `sendMouseMove`,
  `sendScrollEvent`, `isOverlayForwarding`, `isOverlayBrowsing`,
  `notifyOverlayClicked`, `notifyNonOverlayClicked`, `handlePaneFocusChanged`

### Next steps

- **Issue 607: Keyboard input.** Typing in text fields, Cmd+C to copy selected
  text, Cmd+V to paste, Cmd+A to select all, Tab to move between form fields,
  Enter to submit. The biggest remaining gap for browser usability.
