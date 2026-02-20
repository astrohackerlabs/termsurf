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

2. **Control panel clicks don't trigger mode changes.** Clicks outside the
   overlay reach the terminal and the `web` TUI, but the TUI doesn't handle
   mouse events for mode switching — it only uses Enter/Esc.

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
