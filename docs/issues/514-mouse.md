# Issue 514: Mouse Input Forwarding

## Goal

Click on links, scroll up and down on a page, and see the cursor change when
hovering over links. Full mouse control in browse mode.

## Background

Issues 509–512 built a complete Chromium streaming pipeline: webpages render at
120fps in terminal panes via IOSurface overlay. Issue 513 added bidirectional
mode synchronization — the window knows when a pane is in browse mode and can
intercept input. But there is no input forwarding yet. The browser renders but
cannot be interacted with.

Mouse input is the highest-impact next step. Clicking links, scrolling content,
and seeing cursor changes are the minimum interactions needed for a usable
browser pane.

## Architecture

### Event flow

```
NSEvent (mouse click/move/scroll in browse mode)
    ↓
CompositorXPC local event monitor (main thread)
    ├─ Pane in browse mode → forward via XPC
    └─ Not browsing → pass through to terminal
    ↓
XPC message to Chromium Profile Server
    ↓
shell_browser_main_parts.cc (XPC handler on control_connection_)
    ↓
RenderWidgetHost::ForwardMouseEvent() / ForwardWheelEvent()
    ↓
Chromium renderer process
    ↓
Blink handles the event (click, hover, scroll)
```

### Coordinate transformation

The overlay occupies a grid region of the terminal pane. Mouse coordinates from
NSEvent are in window coordinates (pixels, Retina-scaled). These must be
transformed to coordinates relative to the overlay's origin:

```
overlay_x = event.locationInWindow.x - overlay_pixel_origin_x
overlay_y = overlay_pixel_height - (event.locationInWindow.y - overlay_pixel_origin_y)
```

The Y-axis is flipped: macOS has origin at bottom-left, Chromium has origin at
top-left. The overlay's pixel origin and size are derived from its grid
coordinates and cell size (already computed in `handleSetOverlay`).

Hit testing: if the transformed coordinates fall outside (0, 0) to
(overlay_width, overlay_height), the event is outside the overlay and should
pass through to the terminal.

### XPC message format

Mouse events are sent as XPC dictionaries from CompositorXPC to the Chromium
Profile Server on the existing control connection:

```
{
    action: "mouse_event",
    pane_id: "<uuid>",
    type: "move" | "down" | "up" | "entered" | "exited",
    x: <float>,           // pixels, relative to overlay origin
    y: <float>,           // pixels, relative to overlay origin
    button: "left" | "right" | "middle" | "none",
    click_count: <int>,   // 1 = single, 2 = double, 3 = triple
    modifiers: <uint64>,  // bitmask: shift=1, ctrl=2, alt=4, meta=8
}

{
    action: "scroll_event",
    pane_id: "<uuid>",
    x: <float>,           // cursor position, overlay-relative
    y: <float>,
    delta_x: <float>,     // scroll amount (pixels, continuous)
    delta_y: <float>,
    modifiers: <uint64>,
}
```

### Cursor feedback

Chromium changes the cursor based on what's under the mouse (pointer for links,
text cursor for input fields, etc.). The cursor change needs to propagate back
from the Chromium Profile Server to the window.

Chromium's `RenderWidgetHostViewMac` normally handles this via `UpdateCursor()`.
Since we don't have a real view, we need to observe cursor changes and send them
back via XPC so CompositorXPC can set the cursor on the NSWindow.

### Chromium-side injection

The Chromium Profile Server receives mouse events via XPC and injects them into
the rendering pipeline:

```cpp
#include "third_party/blink/public/common/input/web_mouse_event.h"

// In the XPC handler:
blink::WebMouseEvent mouse_event(
    blink::WebInputEvent::Type::kMouseDown,  // or kMouseMove, kMouseUp
    gfx::PointF(x, y),                       // position in widget
    gfx::PointF(screen_x, screen_y),         // position on screen
    blink::WebPointerProperties::Button::kLeft,
    click_count,
    modifiers,
    base::TimeTicks::Now());

shell->web_contents()
    ->GetRenderWidgetHostView()
    ->GetRenderWidgetHost()
    ->ForwardMouseEvent(mouse_event);
```

For scroll events:

```cpp
#include "third_party/blink/public/common/input/web_mouse_wheel_event.h"

blink::WebMouseWheelEvent wheel_event(
    blink::WebInputEvent::Type::kMouseWheel,
    modifiers,
    base::TimeTicks::Now());
wheel_event.SetPositionInWidget(x, y);
wheel_event.delta_x = delta_x;
wheel_event.delta_y = delta_y;
wheel_event.phase = blink::WebMouseWheelEvent::kPhaseBegan;

shell->web_contents()
    ->GetRenderWidgetHostView()
    ->GetRenderWidgetHost()
    ->ForwardWheelEvent(wheel_event);
```

### XPC performance

XPC Mach port transfers are sub-millisecond (proven across Issues 403–414). The
current pipeline already delivers 120 IOSurface frames per second over XPC
(Issue 512). Mouse events are tiny dictionaries (~100 bytes) compared to Mach
port transfers — delivery latency is negligible.

Issue 345 measured mouse move events traversing GUI → XPC → CEF profile server.
The p50 frame interval was identical with and without mouse input (18.7ms vs
17.4ms). The p95 spike (34ms → 79ms) was caused by CEF message loop contention
in the old ts3 architecture, not XPC latency. The Chromium Content API used in
ts5 does not have this contention — `ForwardMouseEvent` is a simple IPC call to
the renderer process, not a synchronous block on the browser message loop.

Mouse moves at display refresh rate (60–120Hz) generate 60–120 XPC messages per
second — far less traffic than the 120fps IOSurface stream already running.

## Ideas for Experiments

The capabilities needed to satisfy the goal — roughly in order of impact:

- **Clicks** — The simplest mouse event. A single point, no continuous tracking.
  Forward left/right mouse down/up from CompositorXPC to the Chromium Profile
  Server via XPC, inject as `blink::WebMouseEvent`, call `ForwardMouseEvent()`.
- **Movement and hover** — Mouse tracking enables CSS `:hover`, tooltips, and
  cursor feedback. Forward `mouseMoved` and track enter/exit for the overlay
  region.
- **Scrolling** — Forward scroll wheel events (`scrollingDeltaX`/`deltaY`) as
  `blink::WebMouseWheelEvent`. Handle both discrete (mouse wheel) and continuous
  (trackpad) scrolling with momentum phases.
- **Cursor feedback** — Propagate cursor type changes (pointer, I-beam, arrow)
  from the Chromium renderer back to the window via XPC so `NSCursor` updates.

## Experiments

### Experiment 1: Mouse clicks

Make left-click work end-to-end. A click on a link in the rendered page should
navigate. This is the simplest mouse event — a single point, no continuous
tracking, no coordinate-sensitive feedback.

#### Mode gating

Mouse events are only intercepted when the target pane is in browse mode. If the
pane under the cursor is not browsing (or has no overlay), the event passes
through to the terminal unchanged. Mouse behavior is identical to stock Ghostty
in control mode — clicks select text, interact with the terminal, etc.

#### Click routing

Unlike Ctrl+Esc (which uses `firstResponder` to identify the focused pane),
mouse clicks can land in any pane — not just the focused one. The monitor cannot
use the first-responder check. Instead, it hit-tests the click coordinates
against all browsing panes:

1. The monitor fires on the main thread. It iterates `paneSurfaceViews`
   (typically 1–3 entries).
2. For each pane, convert `event.locationInWindow` to the SurfaceView's local
   coordinate space via
   `surfaceView.convert(event.locationInWindow, from: nil)`.
3. Check if the point falls within `surfaceView.bounds`. If not, skip.
4. Check `paneBrowsing[uuid] == true` (read on XPC queue via sync dispatch). If
   not browsing, skip.
5. First match wins (no overlapping SurfaceViews in Ghostty's split layout).

If no browsing pane matched, return `event` unchanged.

#### Coordinate transformation

Once the target pane is identified, compute overlay-relative coordinates:

- SurfaceView is not flipped (`isFlipped` defaults to `false`), so Y=0 is at the
  bottom. Flip Y to top-left origin:
  `flippedY = surfaceView.bounds.height - mouseInView.y`.
- Scale to physical pixels: multiply by
  `surfaceView.window?.backingScaleFactor ?? 2.0`.
- Compute overlay-relative physical coordinates: `relX = physX - col * cellW`,
  `relY = physY - row * cellH`.
- Hit test against overlay: if `relX < 0` or `relY < 0` or
  `relX >= width * cellW` or `relY >= height * cellH`, the click is inside the
  pane but outside the overlay — pass through (return `event`).
- Convert back to logical pixels for Chromium: `chromiumX = relX / scale`,
  `chromiumY = relY / scale`.

#### URL synchronization

When a click navigates the page, the URL changes. The `web` TUI needs to know
the new URL so it can update the URL bar. `ShellVideoConsumer` already inherits
from `WebContentsObserver` and already has the per-tab XPC connection and
pane_id. Override `DidFinishNavigation` to detect committed main-frame
navigations and send the new URL back through the GUI to the TUI.

Flow:

```
Chromium renderer commits navigation
    ↓
ShellVideoConsumer::DidFinishNavigation()
    ↓
url_changed XPC message → CompositorXPC (on tab connection)
    ↓
CompositorXPC looks up webPeersForPane[uuid]
    ↓
url_changed XPC message → web TUI (on web peer connection)
    ↓
web TUI updates url variable, URL bar redraws
```

#### XPC messages

Three messages are needed for this experiment:

**1. `mouse_event`** — CompositorXPC → Chromium server (on control connection)

Sent when the user clicks inside the overlay. The `pane_id` field tells the
server which tab to target (one server may host multiple tabs for the same
profile).

- NSEvent type: `.leftMouseDown` → `"down"`, `.leftMouseUp` → `"up"`,
  `.rightMouseDown` → `"down"`, `.rightMouseUp` → `"up"`.
- NSEvent button: left events → `"left"`, right events → `"right"`.
- Modifier flags: `.shift` → 1, `.control` → 2, `.option` → 4, `.command` → 8.
  These match `blink::WebInputEvent::Modifiers` exactly.

```
{
    action: "mouse_event",
    pane_id: "<uuid>",
    type: "down" | "up",
    x: <double>,        // logical pixels, overlay-relative
    y: <double>,
    button: "left" | "right",
    click_count: <int>,  // event.clickCount
    modifiers: <uint64>
}
```

Return `nil` to consume the NSEvent (prevent terminal from receiving it).

**2. `url_changed`** — Chromium server → CompositorXPC (on tab connection)

Sent by `ShellVideoConsumer` when a main-frame navigation commits. Only fires
for committed, primary main-frame navigations (not subframes, not aborted).

```
{
    action: "url_changed",
    pane_id: "<uuid>",
    url: "<new url>"
}
```

**3. `url_changed`** — CompositorXPC → web TUI (on web peer connection)

CompositorXPC receives `url_changed` from the server, looks up the web peer for
the pane, and forwards it.

```
{
    action: "url_changed",
    url: "<new url>"
}
```

#### Changes

##### CompositorXPC.swift

New state properties:

```swift
private var overlayGeometry: [UUID: (col: UInt32, row: UInt32,
    width: UInt32, height: UInt32, cellW: UInt32, cellH: UInt32)] = [:]
private var paneSurfaceViews: [UUID: Ghostty.SurfaceView] = [:]
```

Populate `overlayGeometry` in `handleSetOverlay` after calling
`ghostty_surface_get_cell_size`, alongside the existing `cachedCSurfaces`
assignment. Populate `paneSurfaceViews` in the same `DispatchQueue.main.sync`
block where `findSurface` already runs. Clean both up in `handleDisconnect`.

Add a second local event monitor:

```swift
NSEvent.addLocalMonitorForEvents(matching: [
    .leftMouseDown, .leftMouseUp,
    .rightMouseDown, .rightMouseUp
]) { [weak self] event in ... }
```

The monitor performs click routing, mode gating, coordinate transformation, and
XPC message sending as described above.

Handle incoming `url_changed` messages from the server (add case to
`handleMessage`). Look up the pane UUID from the message's `pane_id`, find the
web peer via `webPeersForPane[uuid]`, and forward the `url_changed` message.

##### shell_video_consumer.cc

Override `DidFinishNavigation` (already a `WebContentsObserver`, already has
`xpc_connection_` and `pane_id_`):

```cpp
void ShellVideoConsumer::DidFinishNavigation(
    NavigationHandle* navigation_handle) {
    if (!navigation_handle->HasCommitted())
        return;
    if (!navigation_handle->IsInPrimaryMainFrame())
        return;

    const std::string& url = navigation_handle->GetURL().spec();
    xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
    xpc_dictionary_set_string(msg, "action", "url_changed");
    xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str());
    xpc_dictionary_set_string(msg, "url", url.c_str());
    xpc_connection_send_message(xpc_connection_, msg);
    xpc_release(msg);
}
```

##### shell_video_consumer.h

Add override declaration:

```cpp
void DidFinishNavigation(NavigationHandle* navigation_handle) override;
```

Add include for `NavigationHandle`:

```cpp
#include "content/public/browser/navigation_handle.h"
```

##### shell_browser_main_parts.cc

Add includes:

```cpp
#include "content/public/browser/render_widget_host.h"
#include "third_party/blink/public/common/input/web_mouse_event.h"
```

Add `"mouse_event"` case to the XPC event handler in `StartDynamicMode` (after
the existing `"resize"` case):

```cpp
} else if (action && std::string_view(action) == "mouse_event") {
    const char* pane = xpc_dictionary_get_string(event, "pane_id");
    const char* type_str = xpc_dictionary_get_string(event, "type");
    const char* button_str = xpc_dictionary_get_string(event, "button");
    double x = xpc_dictionary_get_double(event, "x");
    double y = xpc_dictionary_get_double(event, "y");
    int click_count = (int)xpc_dictionary_get_int64(event, "click_count");
    uint64_t modifiers = xpc_dictionary_get_uint64(event, "modifiers");
    std::string s_pane(pane ? pane : "");
    std::string s_type(type_str ? type_str : "");
    std::string s_button(button_str ? button_str : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::HandleMouseEvent,
                       base::Unretained(self), s_pane, s_type, x, y,
                       s_button, click_count, modifiers));
}
```

New method `HandleMouseEvent`. The server may host multiple tabs (one per pane
sharing this profile). The `pane_id` field from the XPC message selects the
correct tab — same linear scan used by `ResizeCapture`:

```cpp
void ShellBrowserMainParts::HandleMouseEvent(
    const std::string& pane_id, const std::string& type,
    double x, double y, const std::string& button,
    int click_count, uint64_t modifiers) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    // Find tab by pane_id (same lookup as ResizeCapture).
    TabState* tab = nullptr;
    for (auto& t : tabs_) {
        if (t->pane_id == pane_id) { tab = t.get(); break; }
    }
    if (!tab) return;

    // Map type string to WebInputEvent::Type.
    blink::WebInputEvent::Type event_type;
    if (type == "down")
        event_type = blink::WebInputEvent::Type::kMouseDown;
    else if (type == "up")
        event_type = blink::WebInputEvent::Type::kMouseUp;
    else
        return;  // Only clicks in this experiment.

    // Map button string.
    auto btn = blink::WebPointerProperties::Button::kLeft;
    if (button == "right")
        btn = blink::WebPointerProperties::Button::kRight;

    // Map modifiers (Swift bitmask matches WebInputEvent::Modifiers).
    int web_modifiers = static_cast<int>(modifiers & 0xF);
    // Add button-down modifier for mouseDown.
    if (type == "down") {
        if (button == "left")
            web_modifiers |= blink::WebInputEvent::kLeftButtonDown;
        else if (button == "right")
            web_modifiers |= blink::WebInputEvent::kRightButtonDown;
    }

    blink::WebMouseEvent mouse_event(
        event_type,
        gfx::PointF(x, y),
        gfx::PointF(x, y),  // screen position (approximate)
        btn, click_count, web_modifiers,
        base::TimeTicks::Now());

    auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
    if (view)
        view->GetRenderWidgetHost()->ForwardMouseEvent(mouse_event);
}
```

##### shell_browser_main_parts.h

Add inside the `#if BUILDFLAG(IS_MAC)` block after `CloseTab`:

```cpp
void HandleMouseEvent(const std::string& pane_id,
                      const std::string& type,
                      double x, double y,
                      const std::string& button,
                      int click_count, uint64_t modifiers);
```

##### web/src/xpc.rs

Add `UrlChanged` variant to `CompositorMessage`:

```rust
pub enum CompositorMessage {
    ModeChanged { browsing: bool },
    UrlChanged { url: String },
}
```

Add `"url_changed"` parsing to the XPC event handler (after the existing
`"mode_changed"` branch):

```rust
} else if action == "url_changed" {
    let url_key = CString::new("url").unwrap();
    let url_ptr = unsafe { xpc_dictionary_get_string(event, url_key.as_ptr()) };
    if !url_ptr.is_null() {
        let url = unsafe { std::ffi::CStr::from_ptr(url_ptr) }
            .to_str()
            .unwrap_or("")
            .to_string();
        let _ = tx.send(CompositorMessage::UrlChanged { url });
    }
}
```

##### web/src/main.rs

Make the `url` variable mutable. Add a match arm to the message drain loop:

```rust
xpc::CompositorMessage::UrlChanged { url: new_url } => {
    url = new_url;
}
```

The UI already renders `&url` in the URL bar every frame — no rendering changes
needed.

##### Chromium branch

Create `146.0.7650.0-issue-514` from `146.0.7650.0-issue-512` (the last Chromium
branch, from Issue 512 vsync). Add the `mouse_event` and `url_changed` handlers.
Build with `autoninja -C out/Default chromium_profile_server`.

**Verification:**

```bash
cd ts4/box-demo && bun run server.ts &
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- http://localhost:9407
```

Click on a link in the box-demo page. The page should navigate, and the URL bar
in the `web` TUI should update to show the new URL.

Pass: clicking a link navigates the page and the URL bar updates.

**Result:** Pass

Tested with news.ycombinator.com. Clicked a link in browse mode — the page
navigated to the new URL and the `web` TUI URL bar updated to reflect the
change. The full pipeline works end-to-end: NSEvent monitor → hit-test →
coordinate transform → XPC mouse_event → Chromium ForwardMouseEvent → navigation
→ DidFinishNavigation → url_changed XPC → CompositorXPC forward → web TUI URL
bar update.

#### Conclusion

Mouse clicks work. The hit-testing, coordinate transformation, and XPC message
pipeline are all functional. URL synchronization via `DidFinishNavigation` keeps
the TUI URL bar in sync with Chromium's actual navigation state. This is the
first interactive input flowing from TermSurf into the browser — prior to this,
the browser pane was view-only.

### Experiment 2: Re-apply view size after navigation

After clicking a link, the new page renders at the wrong size — narrower than
the viewport, with black bars on the left and right. Resizing the window fixes
it because the `resize` XPC handler calls `view->SetSize()` on the current
`RenderWidgetHostView`.

The root cause is that `view->SetSize()` is only called once in `CreateTab`.
When Chromium navigates, content_shell can reassert the Shell window's default
size on the `RenderWidgetHostView`, overriding our custom dimensions. The
capturer's IOSurface stays the correct size, but the content renders smaller
within it.

#### Fix

`ShellVideoConsumer` already has `initial_width_` and `initial_height_` (the
physical pixel dimensions from `SetInitialSize`), and `DidFinishNavigation`
already fires on every committed main-frame navigation. Add a `SetSize` call
inside `DidFinishNavigation` to re-apply the correct view dimensions after each
navigation.

#### Changes

##### shell_video_consumer.cc

In `DidFinishNavigation`, after the `HasCommitted` / `IsInPrimaryMainFrame`
guards and before the `url_changed` XPC message, add:

```cpp
// Re-apply view size — content_shell may reset it after navigation.
if (initial_width_ > 0 && initial_height_ > 0) {
    RenderWidgetHostView* view = web_contents()->GetRenderWidgetHostView();
    if (view) {
        float scale = view->GetDeviceScaleFactor();
        gfx::Size logical(
            static_cast<int>(std::ceil(initial_width_ / scale)),
            static_cast<int>(std::ceil(initial_height_ / scale)));
        view->SetSize(logical);
    }
}
```

Also update `SetResolution` to store the new dimensions in `initial_width_` /
`initial_height_` so that navigations after a resize use the latest size:

```cpp
void ShellVideoConsumer::SetResolution(int width, int height) {
  if (capturer_ && width > 0 && height > 0) {
    initial_width_ = width;
    initial_height_ = height;
    // ... existing capturer resize code ...
  }
}
```

No other files need changes. No new XPC messages.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://news.ycombinator.com
```

Click a link. The new page should fill the full viewport with no black bars.
Resize the window, then click another link — should still fill correctly.

Pass: no black bars after navigation, at any window size.

**Result:** Pass

Clicked links on news.ycombinator.com. Pages fill the full viewport after
navigation with no black bars.

#### Conclusion

Storing the pixel dimensions in `SetResolution` and re-applying `view->SetSize`
in `DidFinishNavigation` fixes the post-navigation size reset. Two lines in
`SetResolution`, ten lines in `DidFinishNavigation`.

### Experiment 3: Scroll wheel forwarding

Forward trackpad and mouse wheel scroll events from CompositorXPC to the
Chromium Profile Server. After this, pages can be scrolled in browse mode.

#### Gating

Scroll events are only forwarded to Chromium when both conditions are met:

1. **Browse mode** — the pane must have `paneBrowsing[uuid] == true`. In control
   mode, scroll events pass through to the terminal (e.g. for scrollback).
2. **Mouse over viewport** — the cursor must be inside the overlay bounds (the
   Chromium content area), not in the URL bar, status bar, or outside the pane.

Both checks are performed by the shared `hitTestOverlay` helper (extracted from
the mouse click monitor). If either condition fails, `hitTestOverlay` returns
`nil` and the monitor returns the event unchanged — the terminal handles it.

#### Phase mapping

NSEvent scroll phases and Chromium's `WebMouseWheelEvent::Phase` use identical
bitmask values — no translation needed:

| Name       | NSEvent raw | Chromium enum      | Value |
| ---------- | ----------- | ------------------ | ----- |
| None       | 0           | `kPhaseNone`       | 0     |
| Began      | 1           | `kPhaseBegan`      | 1     |
| Stationary | 2           | `kPhaseStationary` | 2     |
| Changed    | 4           | `kPhaseChanged`    | 4     |
| Ended      | 8           | `kPhaseEnded`      | 8     |
| Cancelled  | 16          | `kPhaseCancelled`  | 16    |
| May begin  | 32          | `kPhaseMayBegin`   | 32    |

Trackpad scrolls produce a full lifecycle: `began → changed → ended`, optionally
followed by momentum events (`momentumPhase: began → changed → ended`). Mouse
wheel events have `phase = none` and `momentumPhase = none`.

#### Delta units

`NSEvent.hasPreciseScrollingDeltas` distinguishes trackpad (points) from mouse
wheel (lines). Pass this as a boolean; the Chromium side sets `delta_units`:

- `true` → `ui::ScrollGranularity::kScrollByPrecisePixel`
- `false` → `ui::ScrollGranularity::kScrollByLine`

#### XPC message

```
{
    action: "scroll_event",
    pane_id: "<uuid>",
    x: <double>,              // cursor position, overlay-relative logical pixels
    y: <double>,
    delta_x: <double>,        // scrollingDeltaX (points or lines)
    delta_y: <double>,        // scrollingDeltaY
    phase: <uint64>,          // NSEvent.phase.rawValue
    momentum_phase: <uint64>, // NSEvent.momentumPhase.rawValue
    precise: <bool>,          // hasPreciseScrollingDeltas
    modifiers: <uint64>
}
```

#### Changes

##### CompositorXPC.swift

Extract the hit-test + coordinate transform logic from the mouse click monitor
into a private helper method. Both monitors use the same hit-test:

```swift
private struct OverlayHit {
    let uuid: UUID
    let x: Double  // logical pixels, overlay-relative
    let y: Double
}

private func hitTestOverlay(windowLocation: CGPoint) -> OverlayHit? {
    // Same logic currently in the mouse monitor:
    // iterate paneSurfaceViews, check paneBrowsing, convert coordinates,
    // flip Y, scale to physical, subtract overlay origin, hit test, convert
    // back to logical.
}
```

Call this from both the existing mouse click monitor and a new scroll monitor.
Must be called on the XPC queue (the monitors dispatch to `xpcQueue.sync`
already).

Add a second local event monitor for scroll events:

```swift
NSEvent.addLocalMonitorForEvents(matching: [.scrollWheel]) { [weak self] event in
    guard let self = self else { return event }
    let hit = self.xpcQueue.sync { self.hitTestOverlay(...) }
    guard let hit = hit else { return event }

    // Send scroll_event via XPC.
    self.xpcQueue.async {
        guard let profile = self.paneProfiles[hit.uuid],
              let controlConn = self.serverControlConnections[profile] else { return }

        let msg = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(msg, "action", "scroll_event")
        xpc_dictionary_set_string(msg, "pane_id", hit.uuid.uuidString)
        xpc_dictionary_set_double(msg, "x", hit.x)
        xpc_dictionary_set_double(msg, "y", hit.y)
        xpc_dictionary_set_double(msg, "delta_x", event.scrollingDeltaX)
        xpc_dictionary_set_double(msg, "delta_y", event.scrollingDeltaY)
        xpc_dictionary_set_uint64(msg, "phase", UInt64(event.phase.rawValue))
        xpc_dictionary_set_uint64(msg, "momentum_phase",
            UInt64(event.momentumPhase.rawValue))
        xpc_dictionary_set_bool(msg, "precise", event.hasPreciseScrollingDeltas)
        // modifiers same as mouse clicks
        xpc_dictionary_set_uint64(msg, "modifiers", mods)
        xpc_connection_send_message(controlConn, msg)
    }
    return nil  // consume
}
```

##### shell_browser_main_parts.cc

Add include:

```cpp
#include "third_party/blink/public/common/input/web_mouse_wheel_event.h"
#include "ui/events/types/scroll_types.h"
```

Add `"scroll_event"` case to the XPC handler in `StartDynamicMode` (after the
`mouse_event` case):

```cpp
} else if (action && std::string_view(action) == "scroll_event") {
    const char* pane = xpc_dictionary_get_string(event, "pane_id");
    double x = xpc_dictionary_get_double(event, "x");
    double y = xpc_dictionary_get_double(event, "y");
    double dx = xpc_dictionary_get_double(event, "delta_x");
    double dy = xpc_dictionary_get_double(event, "delta_y");
    uint64_t phase = xpc_dictionary_get_uint64(event, "phase");
    uint64_t momentum = xpc_dictionary_get_uint64(event, "momentum_phase");
    bool precise = xpc_dictionary_get_bool(event, "precise");
    uint64_t modifiers = xpc_dictionary_get_uint64(event, "modifiers");
    std::string s_pane(pane ? pane : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::HandleScrollEvent,
                       base::Unretained(self), s_pane, x, y, dx, dy,
                       phase, momentum, precise, modifiers));
}
```

New method `HandleScrollEvent`:

```cpp
void ShellBrowserMainParts::HandleScrollEvent(
    const std::string& pane_id, double x, double y,
    double delta_x, double delta_y,
    uint64_t phase, uint64_t momentum_phase,
    bool precise, uint64_t modifiers) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    TabState* tab = nullptr;
    for (auto& t : tabs_) {
        if (t->pane_id == pane_id) { tab = t.get(); break; }
    }
    if (!tab) return;

    int web_modifiers = static_cast<int>(modifiers & 0xF);

    blink::WebMouseWheelEvent wheel_event(
        blink::WebInputEvent::Type::kMouseWheel,
        web_modifiers,
        base::TimeTicks::Now());

    wheel_event.SetPositionInWidget(gfx::PointF(x, y));
    wheel_event.SetPositionInScreen(gfx::PointF(x, y));
    wheel_event.delta_x = static_cast<float>(delta_x);
    wheel_event.delta_y = static_cast<float>(delta_y);
    wheel_event.phase =
        static_cast<blink::WebMouseWheelEvent::Phase>(phase);
    wheel_event.momentum_phase =
        static_cast<blink::WebMouseWheelEvent::Phase>(momentum_phase);
    wheel_event.delta_units = precise
        ? ui::ScrollGranularity::kScrollByPrecisePixel
        : ui::ScrollGranularity::kScrollByLine;

    auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
    if (view)
        view->GetRenderWidgetHost()->ForwardWheelEvent(wheel_event);
}
```

##### shell_browser_main_parts.h

Add inside the `#if BUILDFLAG(IS_MAC)` block after `HandleMouseEvent`:

```cpp
void HandleScrollEvent(const std::string& pane_id,
                       double x, double y,
                       double delta_x, double delta_y,
                       uint64_t phase, uint64_t momentum_phase,
                       bool precise, uint64_t modifiers);
```

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://news.ycombinator.com
```

Scroll down with trackpad or mouse wheel in browse mode. The page should scroll.
Trackpad momentum (flick and release) should also work.

Pass: page scrolls smoothly with both trackpad and mouse wheel.

**Result:** Pass

Scrolling works on news.ycombinator.com in browse mode. Trackpad scrolling is
smooth with proper momentum (flick and release coasts naturally). The
`hitTestOverlay` refactor simplified the mouse click monitor and gave the scroll
monitor identical gating for free — both check browse mode and overlay bounds
before forwarding.

#### Conclusion

Scroll wheel forwarding completes the basic input trilogy: clicks (Experiment
1), post-navigation sizing (Experiment 2), and scrolling (Experiment 3). The
phase mapping required zero translation — NSEvent and Chromium use identical
bitmask values. The `hitTestOverlay` helper extracted from the click monitor
made adding the scroll monitor trivial (same hit-test, different XPC payload).
Pages are now navigable and scrollable in browse mode.

### Experiment 4: Mouse move / hover events

Forward mouse movement to Chromium so hover states work: links highlight on
hover, cursor changes to a pointer over clickable elements, tooltips appear, and
CSS `:hover` states fire.

#### Gating

Same as clicks and scroll — only forward when both conditions are met:

1. **Browse mode** — `paneBrowsing[uuid] == true`.
2. **Mouse over viewport** — cursor is inside the overlay bounds.

Both checked by the existing `hitTestOverlay` helper.

#### Event types

Three NSEvent types map to Chromium mouse move:

| NSEvent type         | When it fires                  | Chromium type |
| -------------------- | ------------------------------ | ------------- |
| `.mouseMoved`        | Mouse moves, no button held    | `kMouseMove`  |
| `.leftMouseDragged`  | Mouse moves, left button held  | `kMouseMove`  |
| `.rightMouseDragged` | Mouse moves, right button held | `kMouseMove`  |

All three map to `kMouseMove` — Chromium distinguishes drags by the button-down
modifiers (`kLeftButtonDown`, `kRightButtonDown`), not by event type.

`.mouseMoved` requires a tracking area on the view. SurfaceView already has one
(with `.mouseMoved`, `.activeAlways`, `.inVisibleRect`), so the local event
monitor will receive these events.

#### Performance

Mouse move events fire at 120+ Hz. Each one crosses XPC to the Chromium server.
For a first pass this should be fine — the existing scroll pipeline already
handles similar frequency. If it becomes an issue, we can throttle later (e.g.
coalesce to one event per frame).

#### XPC message

```
{
    action: "mouse_move",
    pane_id: "<uuid>",
    x: <double>,          // logical pixels, overlay-relative
    y: <double>,
    modifiers: <uint64>   // shift=1, ctrl=2, alt=4, cmd=8 + button-down flags
}
```

Button-down flags in modifiers (same bitmask as Chromium):

- Left button held: `modifiers |= 32` (`kLeftButtonDown`)
- Right button held: `modifiers |= 512` (`kRightButtonDown`)

No `click_count`, `button`, or `type` fields — this is always a move, never a
click.

#### Changes

##### CompositorXPC.swift

Add a third local event monitor for mouse move and drag events:

```swift
NSEvent.addLocalMonitorForEvents(matching: [
    .mouseMoved, .leftMouseDragged, .rightMouseDragged
]) { [weak self] event in
    guard let self = self else { return event }

    let hit = self.xpcQueue.sync { self.hitTestOverlay(event: event) }
    guard let hit = hit else { return event }

    // Map modifier flags (shift=1, ctrl=2, alt=4, cmd=8).
    var mods: UInt64 = 0
    if event.modifierFlags.contains(.shift)   { mods |= 1 }
    if event.modifierFlags.contains(.control)  { mods |= 2 }
    if event.modifierFlags.contains(.option)   { mods |= 4 }
    if event.modifierFlags.contains(.command)  { mods |= 8 }
    // Add button-down flags for drag events.
    if event.type == .leftMouseDragged  { mods |= 32 }   // kLeftButtonDown
    if event.type == .rightMouseDragged { mods |= 512 }   // kRightButtonDown

    // Send mouse_move via XPC to the Chromium server.
    self.xpcQueue.async {
        guard let profile = self.paneProfiles[hit.uuid],
              let controlConn = self.serverControlConnections[profile] else { return }

        let msg = xpc_dictionary_create(nil, nil, 0)
        xpc_dictionary_set_string(msg, "action", "mouse_move")
        xpc_dictionary_set_string(msg, "pane_id", hit.uuid.uuidString)
        xpc_dictionary_set_double(msg, "x", hit.x)
        xpc_dictionary_set_double(msg, "y", hit.y)
        xpc_dictionary_set_uint64(msg, "modifiers", mods)
        xpc_connection_send_message(controlConn, msg)
    }

    return nil  // consume
}
```

##### shell_browser_main_parts.cc

Add `"mouse_move"` case to the XPC handler in `StartDynamicMode` (after the
`scroll_event` case):

```cpp
} else if (action && std::string_view(action) == "mouse_move") {
    const char* pane = xpc_dictionary_get_string(event, "pane_id");
    double x = xpc_dictionary_get_double(event, "x");
    double y = xpc_dictionary_get_double(event, "y");
    uint64_t modifiers = xpc_dictionary_get_uint64(event, "modifiers");
    std::string s_pane(pane ? pane : "");
    content::GetUIThreadTaskRunner({})->PostTask(
        FROM_HERE,
        base::BindOnce(&ShellBrowserMainParts::HandleMouseMove,
                       base::Unretained(self), s_pane, x, y, modifiers));
}
```

New method `HandleMouseMove`:

```cpp
void ShellBrowserMainParts::HandleMouseMove(
    const std::string& pane_id, double x, double y,
    uint64_t modifiers) {
    DCHECK_CURRENTLY_ON(BrowserThread::UI);

    TabState* tab = nullptr;
    for (auto& t : tabs_) {
        if (t->pane_id == pane_id) { tab = t.get(); break; }
    }
    if (!tab) return;

    int web_modifiers = static_cast<int>(modifiers);

    blink::WebMouseEvent mouse_event(
        blink::WebInputEvent::Type::kMouseMove,
        gfx::PointF(x, y),
        gfx::PointF(x, y),
        blink::WebPointerProperties::Button::kNoButton,
        0,  // click_count
        web_modifiers,
        base::TimeTicks::Now());

    auto* view = tab->shell->web_contents()->GetRenderWidgetHostView();
    if (view)
        view->GetRenderWidgetHost()->ForwardMouseEvent(mouse_event);
}
```

##### shell_browser_main_parts.h

Add inside the `#if BUILDFLAG(IS_MAC)` block after `HandleScrollEvent`:

```cpp
void HandleMouseMove(const std::string& pane_id,
                     double x, double y,
                     uint64_t modifiers);
```

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://news.ycombinator.com
```

Move the mouse over links in browse mode. Links should highlight on hover and
the cursor should change to a pointer. Drag to select text (left-click hold +
move).

Pass: links highlight on hover.

**Result:** Pass

Links highlight on hover on news.ycombinator.com. CSS `:hover` states fire
correctly as the mouse moves over the page. Drag events (`.leftMouseDragged`,
`.rightMouseDragged`) are also forwarded with button-down modifiers.

**Not working:** Cursor appearance does not change. The cursor stays as an
I-beam (from the terminal's SurfaceView) regardless of what Chromium thinks the
cursor should be. Mouse move forwarding tells Chromium _where_ the cursor is,
but cursor _appearance_ requires a reverse channel — Chromium sending cursor
type changes back through XPC so the app can call `NSCursor.set()`. This is a
separate experiment.

#### Conclusion

Mouse move forwarding enables hover states, completing the mouse input pipeline:
clicks (Experiment 1), post-nav sizing (Experiment 2), scrolling (Experiment 3),
and hover (Experiment 4). Cursor appearance is a follow-up — it requires a new
`cursor_changed` message from Chromium back to the app, which is a different
architectural pattern (server → app notification) than the input forwarding
built so far (app → server commands).

### Experiment 5: Cursor appearance sync

When hovering over a link, Chromium knows the cursor should be a pointing hand.
But this information is discarded — the Shell window is hidden, and macOS shows
the terminal's I-beam instead. This experiment adds a reverse XPC channel:
Chromium sends cursor type changes back to the app, which sets `NSCursor`
accordingly.

#### Cursor change path in Chromium

```
Blink renderer detects hover over <a> tag
    ↓
ChromeClientImpl::SetCursor(ui::Cursor{kHand})
    ↓
WidgetBase::SetCursor() → Mojo IPC to browser process
    ↓
RenderWidgetHostImpl::SetCursor(cursor)   ← hook point
    ↓
view_->UpdateCursor(cursor)
    ↓
RenderWidgetHostViewMac::DisplayCursor()  (discarded — window hidden)
```

The hook point is `RenderWidgetHostImpl::SetCursor()`. This is where the browser
process receives cursor changes from the renderer. We add a callback here that
notifies our `ShellVideoConsumer`, which sends the cursor type via XPC.

#### Cursor type mapping

`ui::mojom::CursorType` enum values map to integer IDs. We send the raw integer
over XPC and map to `NSCursor` on the app side. Only the common cursors need
explicit mapping — everything else defaults to arrow.

| CursorType      | Value | NSCursor               |
| --------------- | ----- | ---------------------- |
| `kPointer`      | 0     | `.arrow`               |
| `kHand`         | 2     | `.pointingHand`        |
| `kIBeam`        | 3     | `.iBeam`               |
| `kCross`        | 1     | `.crosshair`           |
| `kMove`         | 31    | `.openHand`            |
| `kGrab`         | 43    | `.openHand`            |
| `kGrabbing`     | 44    | `.closedHand`          |
| `kNotAllowed`   | 40    | `.operationNotAllowed` |
| `kNone`         | 39    | hide cursor            |
| Everything else | —     | `.arrow`               |

#### Gating and state

CompositorXPC stores the last cursor type per pane:

```swift
private var paneCursorTypes: [UUID: Int64] = [:]
```

Cursor is applied when ALL of these are true:

1. Pane is in browse mode (`paneBrowsing[uuid] == true`)
2. Mouse is over the overlay (hitTestOverlay succeeds)
3. A cursor type has been received from Chromium

The cursor is set in two places:

- **On `cursor_changed` receipt** — if the mouse is currently over this pane's
  overlay (check via stored last-hit pane UUID).
- **On mouse move** — when entering an overlay, apply the stored cursor type.

When the mouse leaves the overlay or mode switches to control, stop overriding.
The terminal's tracking area reasserts the I-beam automatically. If this turns
out not to be reliable (cursor sticks after leaving the overlay), the fix is to
explicitly call `NSCursor.arrow.set()` when `hitTestOverlay` returns nil and
`lastHitPaneUUID` was previously set.

#### XPC message

```
{
    action: "cursor_changed",
    pane_id: "<uuid>",
    cursor_type: <int64>    // ui::mojom::CursorType raw value
}
```

#### Changes

##### render_widget_host_impl.h (upstream file, minimal change)

Add a callback member and setter inside the class (after the existing
`SetCursor` override declaration):

```cpp
// Cursor change callback for out-of-process embedders (TermSurf).
using CursorChangedCallback =
    base::RepeatingCallback<void(const ui::Cursor&)>;
void SetCursorChangedCallback(CursorChangedCallback callback);

CursorChangedCallback cursor_changed_callback_;
```

##### render_widget_host_impl.cc (upstream file, minimal change)

Add the setter method and invoke the callback in `SetCursor`:

```cpp
void RenderWidgetHostImpl::SetCursorChangedCallback(
    CursorChangedCallback callback) {
  cursor_changed_callback_ = std::move(callback);
}

void RenderWidgetHostImpl::SetCursor(const ui::Cursor& cursor) {
  if (cursor_changed_callback_)
    cursor_changed_callback_.Run(cursor);
  if (view_) {
    view_->UpdateCursor(cursor);
  }
}
```

##### shell_video_consumer.h

Add cursor change handler and include:

```cpp
#include "ui/base/cursor/cursor.h"

void OnCursorChanged(const ui::Cursor& cursor);
```

##### shell_video_consumer.cc

Send cursor type via XPC:

```cpp
void ShellVideoConsumer::OnCursorChanged(const ui::Cursor& cursor) {
#if BUILDFLAG(IS_MAC)
  if (!xpc_connection_)
    return;
  xpc_object_t msg = xpc_dictionary_create(NULL, NULL, 0);
  xpc_dictionary_set_string(msg, "action", "cursor_changed");
  xpc_dictionary_set_string(msg, "pane_id", pane_id_.c_str());
  xpc_dictionary_set_int64(msg, "cursor_type",
                           static_cast<int64_t>(cursor.type()));
  xpc_connection_send_message(xpc_connection_, msg);
  xpc_release(msg);
#endif
}
```

##### shell_browser_main_parts.cc

In `CreateTab`, after `video_consumer->ObserveContents()`, register the cursor
callback on the RenderWidgetHost:

```cpp
#include "content/browser/renderer_host/render_widget_host_impl.h"

// In CreateTab, after ObserveContents:
if (auto* rwh = shell->web_contents()->GetRenderWidgetHostView()
                    ->GetRenderWidgetHost()) {
  auto* rwhi = static_cast<RenderWidgetHostImpl*>(rwh);
  rwhi->SetCursorChangedCallback(base::BindRepeating(
      &ShellVideoConsumer::OnCursorChanged,
      base::Unretained(video_consumer.get())));
}
```

Note: `RenderWidgetHostImpl` is internal to `content/` but accessible from our
`content/chromium_profile_server/` code since it's in the same module.

##### CompositorXPC.swift

New state:

```swift
private var paneCursorTypes: [UUID: Int64] = [:]
private var lastHitPaneUUID: UUID? = nil
```

Add `cursor_changed` case to `handleMessage` switch. New handler:

```swift
private func handleCursorChanged(_ msg: xpc_object_t, from peer: xpc_connection_t) {
    guard let paneIdPtr = xpc_dictionary_get_string(msg, "pane_id") else { return }
    let paneIdStr = String(cString: paneIdPtr)
    guard let uuid = UUID(uuidString: paneIdStr) else { return }

    let cursorType = xpc_dictionary_get_int64(msg, "cursor_type")
    paneCursorTypes[uuid] = cursorType

    // If this pane is currently under the mouse, apply immediately.
    if lastHitPaneUUID == uuid {
        DispatchQueue.main.async {
            Self.applyCursor(cursorType)
        }
    }
}

private static func applyCursor(_ cursorType: Int64) {
    let cursor: NSCursor
    switch cursorType {
    case 0:  cursor = .arrow              // kPointer
    case 1:  cursor = .crosshair          // kCross
    case 2:  cursor = .pointingHand       // kHand
    case 3:  cursor = .iBeam              // kIBeam
    case 31: cursor = .openHand           // kMove
    case 39: NSCursor.hide(); return      // kNone
    case 40: cursor = .operationNotAllowed // kNotAllowed
    case 43: cursor = .openHand           // kGrab
    case 44: cursor = .closedHand         // kGrabbing
    default: cursor = .arrow
    }
    NSCursor.unhide()
    cursor.set()
}
```

Update the mouse move monitor to track `lastHitPaneUUID` and apply cursor:

```swift
// In the mouseMoved/drag monitor, after hitTestOverlay:
self.xpcQueue.async {
    self.lastHitPaneUUID = hit.uuid
}
// On main thread, apply stored cursor:
if let cursorType = self.xpcQueue.sync(execute: {
    self.paneCursorTypes[hit.uuid]
}) {
    DispatchQueue.main.async { Self.applyCursor(cursorType) }
}
```

When `hitTestOverlay` returns nil (mouse left overlay), clear `lastHitPaneUUID`:

```swift
// In the mouseMoved/drag monitor, when hit is nil:
self.xpcQueue.async { self.lastHitPaneUUID = nil }
```

Clean up `paneCursorTypes` in `handleDisconnect`.

#### Verification

```bash
open ts5/zig-out/TermSurf.app --stderr ~/dev/termsurf/logs/overlay.log
# In a TermSurf pane:
cargo run -p web -- https://news.ycombinator.com
```

Hover over a link in browse mode. The cursor should change from arrow to
pointing hand. Hover over text — cursor should become I-beam. Move off the
overlay into the URL bar or terminal — cursor should revert to terminal I-beam.

Pass: cursor changes to pointing hand over links, arrow over non-interactive
elements, and reverts when leaving the overlay.
