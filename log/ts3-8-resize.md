# TermSurf 3.0 Dynamic Browser Resize

## Background

### Progress So Far

ts3-7 completed the one-process-per-profile architecture:

- Multiple webviews can share a single CEF process
- Each webview renders within its pane bounds
- The launcher routes requests to existing profile processes
- IOSurface Mach ports are correctly sent to the right GUI panes

The webview rendering pipeline now works for the **initial render**. When a user
runs `web google.com`, the page renders at the correct pane size and displays
within the pane bounds.

### The Problem

When the user resizes the window or splits panes, the webview does not re-render
at the new size. Instead, the existing texture is stretched or compressed to fit
the new viewport dimensions. This produces a blurry, distorted image that does
not match what a user expects from a browser.

**Current behavior:**

1. User runs `web google.com` in a full-width pane
2. Page renders correctly at 1200x800
3. User splits the pane vertically (now 600x800)
4. The 1200x800 texture is squished into 600x800 viewport
5. Text becomes unreadable, images distorted

**Expected behavior:**

1. User runs `web google.com` in a full-width pane
2. Page renders correctly at 1200x800
3. User splits the pane vertically (now 600x800)
4. CEF re-renders the page at 600x800
5. Page reflows naturally, text remains crisp

This is how every browser works. When you resize a browser window, the page
reflows to fit the new width. TermSurf must do the same.

## Goal

When a pane containing a webview is resized, the browser must re-render at the
new dimensions so the page content reflows naturally.

## Product Requirements

### Core Requirement

**When a webview pane changes size, the browser must resize to match.**

This applies to all resize scenarios:

1. **Window resize** — User drags the window edge to make it larger or smaller
2. **Pane split** — User splits a pane, reducing the original pane's size
3. **Pane close** — User closes an adjacent pane, expanding the remaining pane
4. **Divider drag** — User drags the divider between panes to adjust sizes

In all cases, the webview must re-render at the new size, not stretch the old
texture.

### User Experience

**Resize should feel responsive.** The page should update quickly enough that
the user perceives it as "live" resizing, similar to resizing a Chrome or Safari
window.

**Content should reflow naturally.** Text should wrap to fit the new width.
Images should maintain aspect ratio. Responsive layouts should adapt.

**No visual artifacts.** During resize, it's acceptable to briefly show the
stretched old texture while waiting for the new render. However, the final state
must always be a correctly-sized render.

### Edge Cases

1. **Rapid resizing** — User drags window edge continuously. The system should
   debounce or throttle resize events to avoid overwhelming CEF with resize
   requests. A brief delay (e.g., 30-50ms settle time) before triggering
   re-render is acceptable.

2. **Multiple webviews** — When a window resize affects multiple webview panes,
   all of them must resize. Each webview is independent; they may complete their
   re-renders at different times.

3. **Minimum size** — There may be a minimum practical size for webviews. If a
   pane becomes too small (e.g., < 100px in either dimension), the webview
   behavior is undefined. It's acceptable to show a placeholder or simply not
   render.

4. **Profile process crash** — If the profile process crashes during resize, the
   GUI should handle this gracefully (e.g., show an error state in the pane).

### Non-Requirements (Deferred)

The following are explicitly **not** part of this task:

- **Zoom level** — Changing the page zoom (Cmd+/Cmd-) is separate from resize
- **DPI changes** — Moving window between Retina and non-Retina displays
- **Scroll position preservation** — Maintaining scroll position across resize
  (nice to have, but not required)

## Success Criteria

- [ ] Resize window → webview re-renders at new size
- [ ] Split pane → webview re-renders at new size
- [ ] Close adjacent pane → webview re-renders at new size
- [ ] Drag pane divider → webview re-renders at new size
- [ ] Text remains crisp and readable after resize
- [ ] Page content reflows naturally (responsive layouts work)
- [ ] Multiple webviews in same window all resize correctly
- [ ] Resize feels responsive (not sluggish)

## Tasks

- [ ] Detect pane resize events in the GUI
- [ ] Send new dimensions to the profile server
- [ ] Profile server calls CEF resize API
- [ ] CEF re-renders at new size
- [ ] New IOSurface sent to GUI
- [ ] GUI updates viewport to match new size
- [ ] Implement debounce/throttle for rapid resize events

## Research

### IPC Decision: XPC over Unix Sockets

ts3 uses two IPC mechanisms:

- **XPC** — GUI ↔ Launcher, Launcher → Profile, Profile → GUI (IOSurface transfer)
- **Unix domain sockets** — CLI → GUI (the `web` command)

For GUI → Profile resize communication, **XPC is the correct choice**:

1. Profile server already has an XPC command listener (used by launcher for
   `create_browser` commands)
2. XPC is already used for the Profile → GUI direction
3. The architecture is macOS-specific anyway (IOSurface, Mach ports)
4. Adding Unix sockets would require profile server to manage two IPC mechanisms

### Current Communication Gap

The current XPC flow is **one-way**:

```
Profile Server ──display_surface──▶ GUI (working)
GUI ──???──▶ Profile Server (not implemented)
```

The profile server creates a command listener in `main.rs:216-250` and registers
it with the launcher. The launcher connects to send `create_browser` commands.
But the GUI never receives this endpoint—it can only receive surfaces, not send
commands back.

**Solution:** Profile server must share its command endpoint with the GUI. The
simplest approach is to include it in the first `display_surface` message.

### How ts2 Handles Resize

ts2 implements resize in `ts2/wezterm-gui/src/cef_browser/mod.rs:262-291` and
`ts2/wezterm-gui/src/termwindow/render/pane.rs:813-880`:

1. **Debounce with 30ms settle delay** — Every frame, check if size changed. If
   so, record the pending size and mark the time. Only send resize after 30ms of
   no further changes.

2. **CEF resize API** — Call `host.was_resized()` to notify CEF that dimensions
   changed, then `host.invalidate()` to force a repaint.

3. **Message loop pump** — ts2 calls `cef::do_message_loop_work()` after resize
   because CEF runs in-process and shares the event loop. ts3 does NOT need this
   because the profile server has its own process and event loop.

Key code from ts2:

```rust
const SETTLE_DELAY: Duration = Duration::from_millis(30);

// In BrowserState::resize()
host.was_resized();
host.invalidate(PaintElementType::default());
```

### CEF Automatic Re-rendering

CEF automatically handles animation and content updates without explicit resize:

- `windowless_frame_rate: 60` causes CEF to render at 60 FPS
- When content changes (animations, scrolling, DOM updates), CEF renders a new
  frame and calls `on_accelerated_paint`
- The profile server's dedup logic detects when the IOSurface buffer pointer
  changes and sends the new Mach port to GUI

**This already works.** Animations and dynamic content automatically flow to the
GUI. The only missing piece is triggering a re-render when the **size** changes.

### ts3 Cell-Based Sizing

Unlike ts2 which uses exact pixel dimensions, ts3 sizes browsers to cell
boundaries (`cols × cell_width`, `rows × cell_height`). This means:

- Fewer resize events (size only changes when grid dimensions change)
- More predictable dimensions
- Slightly less precise fit, but acceptable for terminal integration

The 30ms debounce is still valuable for rapid window resizing, but cell-based
sizing naturally reduces resize frequency.

### CEF Thread Safety

XPC callbacks run on libdispatch queues, not the CEF UI thread. Browser
operations (including resize) must be marshalled to the CEF UI thread:

```rust
cef::post_task(cef::ThreadId::UI, move || {
    // Safe to call host.was_resized() here
});
```

### Key Files

| File | Role |
|------|------|
| `ts3/termsurf-profile/src/main.rs` | Command listener, resize handler |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Store command connection |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Detect size change, debounce |
| `ts2/wezterm-gui/src/cef_browser/mod.rs` | Reference implementation |
| `ts2/wezterm-gui/src/termwindow/render/pane.rs` | Reference debounce logic |

## Experiments

### Experiment 1: Implement Dynamic Resize via XPC

**Status:** PLANNED

**Goal:** Enable the GUI to send resize commands to the profile server so that
webviews re-render at the correct size when panes are resized.

#### Overview

Currently, the GUI can receive IOSurface frames from the profile server but
cannot send commands back. This experiment establishes bidirectional
communication by:

1. Having the profile server share its command endpoint with the GUI
2. GUI stores a connection to send resize commands
3. GUI detects pane size changes with 30ms debounce
4. Profile server handles resize commands and triggers CEF re-render

#### Changes

**1. Profile Server: Include command endpoint in display_surface**

**File:** `ts3/termsurf-profile/src/main.rs`

In `on_accelerated_paint`, include the command endpoint in the message. The
endpoint only needs to be sent once per browser session (first frame).

Add to `BrowserState`:

```rust
struct BrowserState {
    // ... existing fields ...
    command_endpoint_sent: AtomicBool,  // Track if we've sent endpoint
}
```

Add to `ProfileState`:

```rust
struct ProfileState {
    // ... existing fields ...
    command_endpoint: Mutex<Option<XpcEndpoint>>,  // Cloneable endpoint
}
```

In `on_accelerated_paint`, when sending `display_surface`:

```rust
// Send command endpoint with first frame only
if !self.inner.state.command_endpoint_sent.swap(true, Ordering::Relaxed) {
    if let Some(endpoint) = profile_state.command_endpoint.lock().unwrap().clone() {
        msg.set_endpoint("command_endpoint", endpoint);
    }
}
```

Note: XPC endpoints can only be sent once. The profile server must create the
endpoint in a way that allows cloning, or create a new endpoint per browser.
Alternative: send a separate `register_commands` message before the first
surface.

**2. Profile Server: Handle resize_browser command**

**File:** `ts3/termsurf-profile/src/main.rs`

In the command listener handler (alongside `create_browser`):

```rust
"resize_browser" => {
    let session_id = msg.get_string("session_id").unwrap_or_default();
    let width = msg.get_i64("width") as u32;
    let height = msg.get_i64("height") as u32;

    println!(
        "Profile: resize_browser session={}, size={}x{}",
        session_id, width, height
    );

    // Store pending resize and post to UI thread
    state.pending_resizes.lock().unwrap().push((
        session_id.to_string(),
        width,
        height,
    ));

    let mut task = ResizeBrowserTask::new(Arc::clone(&state));
    cef::post_task(cef::ThreadId::UI, Some(&mut task));
}
```

**3. Profile Server: Store Browser reference for resize**

**File:** `ts3/termsurf-profile/src/main.rs`

`BrowserState` needs to hold the CEF `Browser` to call `was_resized()`:

```rust
struct BrowserState {
    session_id: String,
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    last_handle: AtomicPtr<c_void>,
    browser: Mutex<Option<Browser>>,  // NEW: for resize
    command_endpoint_sent: AtomicBool,
}
```

In `create_browser_on_ui_thread`, store the browser:

```rust
match browser {
    Some(b) => {
        let browser_id = b.identifier();
        *browser_state.browser.lock().unwrap() = Some(b);
        // ... rest of existing code ...
    }
    None => { /* ... */ }
}
```

**4. Profile Server: Resize task for UI thread**

**File:** `ts3/termsurf-profile/src/main.rs`

```rust
wrap_task! {
    pub struct ResizeBrowserTask {
        state: Arc<ProfileState>,
    }

    impl Task {
        fn execute(&self) {
            let pending: Vec<_> = self.state.pending_resizes
                .lock().unwrap().drain(..).collect();

            for (session_id, width, height) in pending {
                resize_browser_on_ui_thread(&session_id, width, height, &self.state);
            }
        }
    }
}

fn resize_browser_on_ui_thread(
    session_id: &str,
    width: u32,
    height: u32,
    state: &Arc<ProfileState>,
) {
    let browsers = state.browsers.lock().unwrap();

    // Find browser by session_id
    let browser_state = browsers.values()
        .find(|b| b.session_id == session_id);

    if let Some(bs) = browser_state {
        // Update stored dimensions
        bs.width.store(width, Ordering::Relaxed);
        bs.height.store(height, Ordering::Relaxed);

        // Notify CEF
        if let Some(ref browser) = *bs.browser.lock().unwrap() {
            if let Some(host) = browser.host() {
                println!(
                    "Profile: Calling was_resized for session {} ({}x{})",
                    session_id, width, height
                );
                host.was_resized();
                host.invalidate(cef::PaintElementType::View);
            }
        }
    } else {
        eprintln!("Profile: resize_browser - session {} not found", session_id);
    }
}
```

**5. GUI: Store command connection per overlay**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

Extend the overlay state to include command connection and debounce state:

```rust
pub struct WebviewOverlay {
    pub mach_port: u32,
    pub width: u32,
    pub height: u32,
    pub session_id: String,
    // NEW fields for resize:
    pub command_conn: Option<Arc<XpcConnection>>,
    pub last_sent_size: Option<(u32, u32)>,
    pub pending_size: Option<(u32, u32)>,
    pub last_resize_time: Option<std::time::Instant>,
}
```

When receiving `display_surface`, check for command endpoint:

```rust
if let Some(endpoint) = msg.get_endpoint("command_endpoint") {
    match XpcConnection::from_endpoint(endpoint) {
        Ok(conn) => {
            set_event_handler(&conn, |event| {
                if let Err(e) = event {
                    eprintln!("[XPC] Command connection error: {}", e);
                }
            });
            conn.resume();
            overlay.command_conn = Some(Arc::new(conn));
            log::info!("[XPC] Stored command connection for pane {}", pane_id);
        }
        Err(e) => {
            log::error!("[XPC] Failed to connect to command endpoint: {}", e);
        }
    }
}
```

**6. GUI: Detect size change and debounce**

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

In `render_webview_overlays_webgpu`, after calculating viewport dimensions:

```rust
use std::time::{Duration, Instant};

const SETTLE_DELAY: Duration = Duration::from_millis(30);

// After calculating (viewport_x, viewport_y, viewport_w, viewport_h):

let current_size = (viewport_w as u32, viewport_h as u32);

// Check if size changed from what we last sent
let size_changed = overlay.last_sent_size != Some(current_size);

if size_changed {
    // Size changed - update pending and mark time
    if overlay.pending_size != Some(current_size) {
        overlay.pending_size = Some(current_size);
        overlay.last_resize_time = Some(Instant::now());
    }
}

// Check if we should send resize command
if let Some(pending) = overlay.pending_size {
    let should_send = overlay.last_resize_time
        .map(|t| t.elapsed() >= SETTLE_DELAY)
        .unwrap_or(false);

    if should_send {
        if let Some(ref conn) = overlay.command_conn {
            let msg = termsurf_xpc::XpcDictionary::new();
            msg.set_string("action", "resize_browser");
            msg.set_string("session_id", &overlay.session_id);
            msg.set_i64("width", pending.0 as i64);
            msg.set_i64("height", pending.1 as i64);
            conn.send(&msg);

            log::info!(
                "[Render] Sent resize for pane {}: {}x{}",
                pane_id, pending.0, pending.1
            );
        }

        overlay.last_sent_size = Some(pending);
        overlay.pending_size = None;
        overlay.last_resize_time = None;
    }
}
```

**7. GUI: Pass session_id to overlay**

The overlay needs the `session_id` to include in resize commands. This should
already be available from the XPC message flow—verify it's stored in the overlay
struct.

#### Files to Modify

| File | Changes |
|------|---------|
| `ts3/termsurf-profile/src/main.rs` | Send command endpoint, handle `resize_browser`, store Browser ref |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Store command connection and debounce state |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Detect size change, 30ms debounce, send resize |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Open a webview
web google.com

# Split the pane (Cmd+Shift+D)
# The webview should re-render at the new smaller size

# Check profile logs for resize handling
cat /tmp/termsurf-profile-*.log | grep resize
# Should show: "resize_browser session=..., size=..."
# Should show: "Calling was_resized for session..."

# Check GUI logs for resize commands
cat /tmp/termsurf-gui.log | grep "Sent resize"
# Should show: "[Render] Sent resize for pane 0: 640x768"

# Drag the window edge to resize
# Webview should update after 30ms settle delay

# Close the split pane
# Remaining webview should expand and re-render at full size
```

#### Success Criteria

- [ ] Profile server sends command endpoint with first `display_surface`
- [ ] GUI stores command connection per overlay
- [ ] Splitting a pane triggers resize command after 30ms
- [ ] Profile server receives `resize_browser` and calls `was_resized()`
- [ ] CEF re-renders at new size
- [ ] New IOSurface is sent to GUI with correct dimensions
- [ ] Dragging window edge triggers resize after 30ms settle
- [ ] Text remains crisp after resize (not stretched)
- [ ] Multiple webviews in same window each resize independently

#### Risks and Mitigations

1. **XPC endpoint cloning** — XPC endpoints may only be usable once. If so,
   profile server must create a fresh endpoint per browser or send endpoint via
   a separate message before the first surface.

2. **Debounce timing** — 30ms may be too short or too long. Start with 30ms (ts2
   default), adjust if needed based on feel.

3. **Race conditions** — Resize commands may arrive while CEF is already
   rendering. CEF should handle this gracefully, but watch for crashes or
   hangs.

4. **Browser reference lifetime** — Storing `Browser` in `BrowserState` requires
   careful lifetime management. The browser must outlive the state, or we need
   weak references.
