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
cd ts4/box-demo && bun run server.ts &
cd ghost && zig build
open ghost/zig-out/Ghostty.app --stderr ~/dev/termsurf/logs/ghost.log
cargo run -p web -- http://localhost:9407
```

Pass criteria:

- Clicking a link in the box demo navigates to the target page
- Click position is accurate (correct link is hit, not neighbors)
- Right click works (context menu or right-click handler fires)
- Clicks outside the overlay go to the terminal as normal
- No crash on rapid clicking
