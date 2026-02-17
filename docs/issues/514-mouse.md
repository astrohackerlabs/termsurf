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
