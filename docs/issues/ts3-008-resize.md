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

- **XPC** — GUI ↔ Launcher, Launcher → Profile, Profile → GUI (IOSurface
  transfer)
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

| File                                            | Role                             |
| ----------------------------------------------- | -------------------------------- |
| `ts3/termsurf-profile/src/main.rs`              | Command listener, resize handler |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` | Store command connection         |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Detect size change, debounce     |
| `ts2/wezterm-gui/src/cef_browser/mod.rs`        | Reference implementation         |
| `ts2/wezterm-gui/src/termwindow/render/pane.rs` | Reference debounce logic         |

## Experiments

### Experiment 1: Implement Dynamic Resize via Bidirectional XPC

**Status:** FAILED

**Goal:** Enable the GUI to send resize commands to the profile server so that
webviews re-render at the correct size when panes are resized.

#### Key Insight: XPC Connections Are Bidirectional

XPC connections are inherently bidirectional. When the profile server connects
to the GUI via `gui_endpoint`, that same connection can be used by the GUI to
send commands back. No separate command channel is needed.

Current state:

- Profile server connects to GUI → sends `display_surface`
- GUI stores connection in a `Vec` (index only, no pane mapping)
- Profile server's event handler only logs errors

To enable resize:

- GUI stores connection by `pane_id` in a `HashMap`
- Profile server's event handler processes incoming commands
- GUI sends `resize_browser` on the existing connection

#### Architecture

```
GUI                                     Profile Server
┌─────────────────────┐                 ┌─────────────────────────┐
│                     │                 │                         │
│  Pane 0 (google)    │                 │  Browser 0 (google)     │
│    ┌───────────────┐│                 │    ┌───────────────────┐│
│    │ peer_conn[0] ◄┼┼──display_surface┼────┤ gui connection    ││
│    │               ├┼┼──resize_browser─────►                   ││
│    └───────────────┘│                 │    └───────────────────┘│
│                     │                 │                         │
│  Pane 1 (github)    │                 │  Browser 1 (github)     │
│    ┌───────────────┐│                 │    ┌───────────────────┐│
│    │ peer_conn[1] ◄┼┼──display_surface┼────┤ gui connection    ││
│    │               ├┼┼──resize_browser─────►                   ││
│    └───────────────┘│                 │    └───────────────────┘│
│                     │                 │                         │
└─────────────────────┘                 └─────────────────────────┘
```

Each pane has its own bidirectional connection. No session_id routing needed—
the connection itself identifies the browser.

#### Why No Handshake Is Needed

The GUI creates **one listener per pane**. When a connection arrives on that
listener, the GUI already knows the `pane_id` — it's captured in the closure
from `spawn_profile_for_pane`. The current code already looks up `pane_id` from
`session_id` via `pending_sessions`. We just need to store the connection by
`pane_id` instead of pushing to a Vec.

No `register_browser` message needed. No session routing. The listener itself
provides the context.

#### Changes

**1. GUI: Store peer connections by pane_id**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

Change `connections: Vec` to `peer_connections: HashMap`:

```rust
pub struct XpcManager {
    // ... existing fields ...

    /// Peer connections from profile servers, keyed by pane_id
    /// Used to send commands (resize, input) back to the browser
    peer_connections: Mutex<HashMap<u64, Arc<XpcConnection>>>,
}
```

In `set_new_connection_handler` (around line 149), look up `pane_id` from the
captured `session_id` and store the connection:

```rust
set_new_connection_handler(&listener, move |conn| {
    log::info!("[XPC Manager] New connection for session {}", session_id_clone);

    let conn = Arc::new(conn);
    let session_id = session_id_clone.clone();
    let manager = Arc::clone(&self_clone);

    // Look up pane_id from session BEFORE setting event handler
    // This works because pending_sessions.insert() happens before spawn_profile
    let pane_id = manager.pending_sessions.lock().unwrap()
        .get(&session_id).copied();

    // Store connection by pane_id (replaces the old Vec push at line 227)
    if let Some(pane_id) = pane_id {
        manager.peer_connections.lock().unwrap()
            .insert(pane_id, Arc::clone(&conn));
        log::info!("[XPC] Stored peer connection for pane {}", pane_id);
    }

    // ... existing set_event_handler code ...

    conn.resume();
});
```

The `pane_id` lookup works because
`pending_sessions.insert(session_id, pane_id)` happens on line 237-240, BEFORE
the spawn message is sent. When the profile server connects back, the mapping is
already present.

**2. GUI: Add method to send commands**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

```rust
impl XpcManager {
    /// Send a command to the browser in the given pane
    pub fn send_command(&self, pane_id: u64, msg: &XpcDictionary) -> bool {
        let connections = self.peer_connections.lock().unwrap();
        if let Some(conn) = connections.get(&pane_id) {
            conn.send(msg);
            true
        } else {
            log::warn!("[XPC] No connection for pane {}", pane_id);
            false
        }
    }

    /// Send a resize command to the browser in the given pane
    pub fn send_resize(&self, pane_id: u64, width: u32, height: u32) -> bool {
        let msg = XpcDictionary::new();
        msg.set_string("action", "resize_browser");
        msg.set_i64("width", width as i64);
        msg.set_i64("height", height as i64);

        if self.send_command(pane_id, &msg) {
            log::info!("[XPC] Sent resize to pane {}: {}x{}", pane_id, width, height);
            true
        } else {
            false
        }
    }
}
```

**3. Profile Server: Add event handler for commands**

**File:** `ts3/termsurf-profile/src/main.rs`

Currently, the GUI connection has no event handler (or only logs errors). Add a
handler to process incoming commands:

```rust
// In create_browser_on_ui_thread, after creating gui connection:

let browser_state_clone = Arc::clone(&browser_state);
set_event_handler(&gui, move |event| {
    match event {
        Ok(msg) => {
            let action = msg.get_string("action").unwrap_or_default();
            match action.as_str() {
                "resize_browser" => {
                    let width = msg.get_i64("width") as u32;
                    let height = msg.get_i64("height") as u32;
                    println!("Profile: resize_browser {}x{}", width, height);

                    let bs = Arc::clone(&browser_state_clone);
                    cef::post_task(cef::ThreadId::UI, move || {
                        resize_browser(&bs, width, height);
                    });
                }
                _ => {}
            }
        }
        Err(e) => {
            eprintln!("Profile: GUI connection error: {}", e);
        }
    }
});
```

**4. Profile Server: Store Browser reference**

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
}
```

In `create_browser_on_ui_thread`, store the browser after creation:

```rust
if let Some(b) = browser {
    *browser_state.browser.lock().unwrap() = Some(b);
    // ... existing code ...
}
```

**5. Profile Server: Resize function**

**File:** `ts3/termsurf-profile/src/main.rs`

```rust
fn resize_browser(state: &BrowserState, width: u32, height: u32) {
    // Update stored dimensions (used by get_view_rect)
    state.width.store(width, Ordering::Relaxed);
    state.height.store(height, Ordering::Relaxed);

    // Notify CEF of size change
    if let Some(ref browser) = *state.browser.lock().unwrap() {
        if let Some(host) = browser.host() {
            println!("Profile: was_resized {}x{}", width, height);
            host.was_resized();
            host.invalidate(cef::PaintElementType::View);
        }
    }
}
```

**6. GUI: Add debounce state**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

Add debounce tracking to `WebviewOverlay`:

```rust
pub struct WebviewOverlay {
    pub mach_port: u32,
    pub width: u32,
    pub height: u32,
    // Debounce state for resize:
    pub last_sent_size: Option<(u32, u32)>,
    pub pending_resize: Option<(u32, u32, std::time::Instant)>,
}
```

**7. GUI: Detect size change and send resize**

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

In `render_webview_overlays_webgpu`, after calculating viewport dimensions:

```rust
const SETTLE_DELAY: Duration = Duration::from_millis(30);

let current_size = (viewport_w as u32, viewport_h as u32);

// Track pending resize with timestamp
if overlay.last_sent_size != Some(current_size) {
    // Size changed - record pending resize
    if overlay.pending_resize.map(|(w, h, _)| (w, h)) != Some(current_size) {
        overlay.pending_resize = Some((current_size.0, current_size.1, Instant::now()));
    }
}

// Send resize after settle delay
if let Some((w, h, time)) = overlay.pending_resize {
    if time.elapsed() >= SETTLE_DELAY {
        if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
            xpc_manager.send_resize(*pane_id, w, h);
        }
        overlay.last_sent_size = Some((w, h));
        overlay.pending_resize = None;
    }
}
```

**Note:** Requires mutable access to overlay. May need to store debounce state
in a separate `HashMap<PaneId, ResizeState>` if overlay iteration is immutable.

#### Files to Modify

| File                                               | Changes                                                                                          |
| -------------------------------------------------- | ------------------------------------------------------------------------------------------------ |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Change `connections` Vec to `peer_connections` HashMap, add `send_command()` and `send_resize()` |
| `ts3/termsurf-profile/src/main.rs`                 | Add event handler on gui connection, store Browser ref, add `resize_browser()`                   |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add debounce fields to WebviewOverlay                                                            |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Detect size change, 30ms debounce, call `send_resize()`                                          |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Open a webview
web google.com

# Check that connection was stored
cat /tmp/termsurf-gui.log | grep "peer connection"
# Should show: "[XPC] Stored peer connection for pane 0"

# Split the pane (Cmd+Shift+D)
# The webview should re-render at the new smaller size

# Check profile logs for resize handling
cat /tmp/termsurf-profile-*.log | grep -i resize
# Should show: "resize_browser 640x768"
# Should show: "was_resized 640x768"

# Check GUI logs for resize commands
cat /tmp/termsurf-gui.log | grep "Sent resize"
# Should show: "[XPC] Sent resize to pane 0: 640x768"

# Open second webview in the new pane
web github.com
# Should get its own connection
cat /tmp/termsurf-gui.log | grep "peer connection"
# Should show: "[XPC] Stored peer connection for pane 1"

# Drag the window edge to resize
# Both webviews should update after 30ms settle delay

# Close one pane (Cmd+W or exit)
# Remaining webview should expand to full size and re-render
cat /tmp/termsurf-gui.log | grep "Sent resize"
# Should show resize to larger dimensions
```

#### Success Criteria

- [ ] GUI stores peer connection by pane_id in HashMap
- [ ] GUI can send `resize_browser` command via stored connection
- [ ] Profile server receives command on gui connection's event handler
- [ ] Profile server calls `was_resized()` and `invalidate()`
- [ ] CEF re-renders at new size
- [ ] New IOSurface is sent to GUI with correct dimensions
- [ ] Splitting a pane triggers resize command after 30ms
- [ ] Dragging window edge triggers resize after 30ms settle
- [ ] Text remains crisp after resize (not stretched)
- [ ] Multiple webviews each have independent connections
- [ ] Each webview resizes independently

#### Forwards Compatibility

This architecture supports future input handling with no changes:

```rust
// Keyboard (future)
xpc_manager.send_command(pane_id, &key_event_msg);

// Mouse (future)
xpc_manager.send_command(pane_id, &mouse_event_msg);
```

Same connection, same pattern. The `send_command()` method is generic.

#### Connection Cleanup

When a webview pane is closed, remove the stale connection:

```rust
impl XpcManager {
    pub fn remove_connection(&self, pane_id: u64) {
        self.peer_connections.lock().unwrap().remove(&pane_id);
        log::info!("[XPC] Removed connection for pane {}", pane_id);
    }
}
```

Call this from wherever pane close is handled (e.g., when the overlay is removed
from the overlays map).

#### Risks and Mitigations

1. **Debounce timing** — 30ms may be too short or too long. Start with 30ms (ts2
   default), adjust based on feel.

2. **Race conditions** — Resize commands may arrive while CEF is already
   rendering. CEF should handle this gracefully.

3. **Browser reference lifetime** — Storing `Browser` in `BrowserState` requires
   the browser to outlive the state. CEF manages browser lifetime internally.

4. **Mutable overlay access** — Debounce state requires mutable access to
   overlays during render. If problematic, store debounce state in a separate
   `HashMap<PaneId, ResizeState>` in XpcManager.

#### Conclusion

**Result:** FAILED — CEF helper subprocess crashes during browser creation.

**Symptoms:**

- `web google.com` times out after 5 seconds waiting for IOSurface
- Profile server logs show browser creation started but never completed
- CEF helper subprocess crashes with:
  ```
  bootstrap_look_up org.wezfurlong.wezterm.MachPortRendezvousServer.1: Unknown service name (1102)
  No rendezvous client, terminating process (parent died?)
  ```

**What Changed:**

In `create_browser_on_ui_thread`, the original code was:

```rust
set_event_handler(&*gui, |event| { /* log errors */ });
gui.resume();
```

The implementation changed this to:

```rust
gui.resume();
// ... create browser_state ...
set_event_handler(&*gui, move |event| { /* handle resize + log errors */ });
```

**Hypothesis:**

The order change — calling `gui.resume()` before setting the event handler — may
have caused the failure. XPC connections should have their event handler set
before being resumed to avoid race conditions or undefined behavior.

However, this doesn't fully explain why CEF's internal Mach port rendezvous
fails. The GUI XPC connection is separate from CEF's internal process
communication. Possible explanations:

1. **XPC event handler order matters** — Setting the handler after resume may
   cause the connection to behave unexpectedly, possibly dropping or corrupting
   messages that affect CEF initialization.

2. **Timing/race condition** — The delayed event handler setup may have changed
   timing in a way that affects CEF subprocess spawning.

3. **Unrelated environmental issue** — The CEF helper crash may be coincidental
   (code signing, sandbox, or CEF state corruption from previous runs).

**Proposed Fix for Experiment 2:**

1. Restore the original order: set event handler BEFORE `gui.resume()`
2. Capture `browser_state` in the closure after it's created, but ensure the
   handler is set before resume
3. This may require creating a placeholder `Arc<Mutex<Option<BrowserState>>>`
   that gets populated after browser creation

### Experiment 2: Fix XPC Event Handler Order

**Status:** FAILED

**Goal:** Fix the XPC event handler ordering bug from Experiment 1. The event
handler must be set BEFORE `gui.resume()` to avoid undefined behavior.

#### The Problem

In Experiment 1, the code in `create_browser_on_ui_thread` was:

```rust
let gui = XpcConnection::from_endpoint(gui_endpoint)?;
gui.resume();  // WRONG: resume before handler

let browser_state = Arc::new(BrowserState { ... });

set_event_handler(&*gui, move |event| {
    // Handler set AFTER resume - too late!
    ...
});
```

XPC connections should have their event handler set before being resumed.
Setting the handler after resume may cause race conditions where messages arrive
before the handler is ready.

#### The Challenge

The event handler needs access to `browser_state` for resize operations, but
`browser_state` doesn't exist until after the connection is created. We can't
set the handler before creating browser_state, and we can't create browser_state
before we have the gui connection.

#### Solution: Deferred State Wrapper

Use an `Arc<Mutex<Option<BrowserState>>>` wrapper that starts as `None` and gets
populated after browser creation:

```rust
// 1. Create connection (don't resume yet)
let gui = XpcConnection::from_endpoint(gui_endpoint)?;
let gui = Arc::new(gui);

// 2. Create deferred state wrapper (empty for now)
let deferred_state: Arc<Mutex<Option<Arc<BrowserState>>>> =
    Arc::new(Mutex::new(None));

// 3. Set event handler BEFORE resume (with deferred state)
let deferred_for_handler = Arc::clone(&deferred_state);
set_event_handler(&*gui, move |event| {
    // Get state from wrapper (may be None early in lifecycle)
    let state_guard = deferred_for_handler.lock().unwrap();
    let Some(state) = state_guard.as_ref() else {
        return;  // State not ready yet, ignore message
    };
    // ... handle resize ...
});

// 4. NOW resume the connection
gui.resume();

// 5. Create browser_state (now that gui is ready)
let browser_state = Arc::new(BrowserState { ... });

// 6. Populate the deferred wrapper
*deferred_state.lock().unwrap() = Some(Arc::clone(&browser_state));
```

This ensures:

- Event handler is set before resume
- Handler can safely access browser_state once it's populated
- Early messages (before state is ready) are ignored harmlessly

#### Changes

**File:** `ts3/termsurf-profile/src/main.rs`

Replace the browser creation code in `create_browser_on_ui_thread`:

```rust
pub fn create_browser_on_ui_thread(
    url: &str,
    session_id: &str,
    gui_endpoint: XpcEndpoint,
    width: u32,
    height: u32,
    state: &Arc<ProfileState>,
) {
    use std::sync::atomic::AtomicPtr;

    // 1. Connect to GUI (don't resume yet)
    let gui = match XpcConnection::from_endpoint(gui_endpoint) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("Profile: Failed to connect to GUI: {}", e);
            return;
        }
    };

    // 2. Create deferred state wrapper (will be populated after browser creation)
    let deferred_state: Arc<Mutex<Option<Arc<BrowserState>>>> =
        Arc::new(Mutex::new(None));

    // 3. Set event handler BEFORE resume
    let deferred_for_handler = Arc::clone(&deferred_state);
    set_event_handler(&*gui, move |event| {
        match event {
            Ok(msg) => {
                let action = msg.get_string("action").unwrap_or_default();
                match action.as_str() {
                    "resize_browser" => {
                        // Get state from deferred wrapper
                        let state_guard = deferred_for_handler.lock().unwrap();
                        let Some(bs) = state_guard.as_ref() else {
                            println!("Profile: resize_browser ignored (state not ready)");
                            return;
                        };

                        let width = msg.get_i64("width") as u32;
                        let height = msg.get_i64("height") as u32;
                        println!("Profile: resize_browser {}x{}", width, height);

                        let bs = Arc::clone(bs);
                        drop(state_guard);  // Release lock before post_task

                        let mut task = ResizeBrowserTask::new(bs, width, height);
                        cef::post_task(cef::ThreadId::UI, Some(&mut task));
                    }
                    _ => {}
                }
            }
            Err(e) => {
                eprintln!("Profile: GUI connection error: {}", e);
            }
        }
    });

    // 4. NOW resume the connection (handler is ready)
    gui.resume();

    // 5. Create per-browser state
    let browser_state = Arc::new(BrowserState {
        session_id: session_id.to_string(),
        gui: Arc::clone(&gui),
        width: std::sync::atomic::AtomicU32::new(width),
        height: std::sync::atomic::AtomicU32::new(height),
        last_handle: AtomicPtr::new(std::ptr::null_mut()),
        browser: Mutex::new(None),
    });

    // ... create render handler, client, browser ...

    match browser {
        Some(b) => {
            let browser_id = b.identifier();
            println!(
                "Profile: Browser {} created for '{}' (session='{}')",
                browser_id, url, session_id
            );

            // Store browser reference for resize operations
            *browser_state.browser.lock().unwrap() = Some(b);

            // 6. Populate deferred state (handler can now access it)
            *deferred_state.lock().unwrap() = Some(Arc::clone(&browser_state));

            // Store browser state by ID
            state.browsers.lock().unwrap().insert(browser_id, browser_state);
        }
        None => eprintln!("Profile: Failed to create browser for '{}'", url),
    }
}
```

#### Why This Works

1. **Handler before resume** — The event handler is set before `gui.resume()`,
   following XPC best practices.

2. **Safe state access** — The handler checks if state is available before using
   it. Early messages (unlikely but possible) are safely ignored.

3. **No race condition** — The browser is created synchronously on the UI
   thread. By the time the browser is ready to receive resize commands, the
   state wrapper is already populated.

4. **Lock discipline** — The lock is dropped before `post_task` to avoid holding
   it during CEF operations.

#### Files to Modify

| File                               | Changes                                                         |
| ---------------------------------- | --------------------------------------------------------------- |
| `ts3/termsurf-profile/src/main.rs` | Reorder event handler before resume, use deferred state wrapper |

No changes needed to the GUI side — the XPC connection storage and resize
detection from Experiment 1 are correct.

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Open a webview - should work now (no crash)
web google.com

# Check profile logs for successful browser creation
cat /tmp/termsurf-profile-*.log | grep -E "(Browser.*created|resize)"
# Should show: "Browser 1 created for 'https://google.com'"

# Split the pane (Cmd+Shift+D)
# Webview should re-render at new smaller size

# Check resize was sent and handled
cat /tmp/termsurf-gui.log | grep "Sent resize"
cat /tmp/termsurf-profile-*.log | grep -E "(resize_browser|was_resized)"
# Should show resize commands being received and processed

# Test edge case: very fast resize (drag window corner rapidly)
# Should debounce and not overwhelm CEF
```

#### Success Criteria

- [ ] `web google.com` works (no crash, page renders)
- [ ] Profile logs show browser creation completed
- [ ] GUI logs show peer connection stored
- [ ] Split pane triggers resize command
- [ ] Profile receives resize and calls was_resized()
- [ ] Page re-renders at new size (text remains crisp)
- [ ] Multiple webviews work independently

#### Risks

1. **Deferred state overhead** — Extra `Arc<Mutex<Option<>>>` wrapper adds
   indirection. Performance impact should be negligible since resize events are
   infrequent.

2. **Dropped early messages** — If the GUI sends a resize before state is
   populated, it's ignored. This is acceptable since the browser hasn't rendered
   yet anyway.

3. **Lock contention** — The mutex is only held briefly during message handling.
   Not a concern at normal resize frequencies.

#### Conclusion

**Result:** FAILED — The XPC handler order fix worked (no crash), but resizing
produces wrong dimensions and only works once.

**What Worked:**

- The deferred state wrapper approach correctly fixed the XPC handler ordering
- Browser creation now succeeds (no more CEF subprocess crash)
- Resize commands are sent from GUI and received by profile server
- Profile server calls `was_resized()` and `invalidate()` correctly

**What Failed:**

1. **Wrong dimensions sent — physical instead of logical pixels**

   The resize code in `draw.rs` sends viewport dimensions directly:
   ```rust
   xpc_manager.check_and_send_resize(*pane_id, viewport_w as u32, viewport_h as u32);
   ```

   But `viewport_w` and `viewport_h` are in **physical pixels**. CEF expects
   **logical pixels** (which it multiplies by `device_scale_factor`).

   Evidence from logs:
   - Initial spawn: logical 1033×1050 → physical IOSurface 2066×2100 ✓
   - Resize sends: 2067×2100 (physical, should be ~1033 logical)
   - CEF applies 2× scale → IOSurface becomes 4134×4200 ✗

   The initial spawn in `webview_socket.rs` correctly divides by scale:
   ```rust
   let scale = dims.dpi as f32 / 72.0;
   let lw = (physical_width / scale) as u32;
   let lh = (physical_height / scale) as u32;
   ```

   But the resize code omits this division.

2. **Resolution change instead of resize**

   Because physical pixels were sent as logical, CEF doubled the size. The
   webview appeared to "change resolution" rather than resize — the content
   rendered at 2× the expected size, making everything appear zoomed out or
   lower resolution.

3. **Resize only works once**

   After the first incorrect resize, subsequent resizes appeared to stop
   working. This may be related to the dimension mismatch confusing the debounce
   logic, or it may have been coincidental with closing the webview.

**Hypothesis for Fix (Experiment 3):**

Divide viewport dimensions by scale factor before sending resize:

```rust
// In draw.rs, around line 364:
let scale = self.dimensions.dpi as f32 / 72.0;
let logical_w = (viewport_w / scale) as u32;
let logical_h = (viewport_h / scale) as u32;
xpc_manager.check_and_send_resize(*pane_id, logical_w, logical_h);
```

This matches how the initial dimensions are calculated in `webview_socket.rs`
and should produce correctly-sized IOSurfaces after resize.

### Experiment 3: Fix Scale Factor for Resize

**Status:** FAILED

**Goal:** Fix the scale factor bug from Experiment 2. Send logical dimensions
(physical / scale) instead of physical dimensions when resizing.

#### The Problem

Experiment 2 fixed the XPC handler ordering but sent wrong dimensions:

```rust
// draw.rs line 364 - WRONG: sends physical pixels
xpc_manager.check_and_send_resize(*pane_id, viewport_w as u32, viewport_h as u32);
```

CEF expects logical dimensions and multiplies by `device_scale_factor`:

- GUI sends: 2067×2100 (physical pixels)
- CEF interprets as: 2067×2100 logical
- CEF renders at: 4134×4200 physical (2× scale applied)

The initial spawn in `webview_socket.rs` correctly divides by scale:

```rust
let scale = dims.dpi as f32 / 72.0;
let lw = (physical_width / scale) as u32;
let lh = (physical_height / scale) as u32;
```

#### Solution

Apply the same scale division in `draw.rs`:

```rust
// Get scale factor (macOS base DPI = 72)
let scale = self.dimensions.dpi as f32 / 72.0;
let scale = if scale <= 0.0 { 2.0 } else { scale };

// Convert physical pixels to logical for CEF
let logical_w = (viewport_w / scale) as u32;
let logical_h = (viewport_h / scale) as u32;

xpc_manager.check_and_send_resize(*pane_id, logical_w, logical_h);
```

#### Changes

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

Replace the resize call (around line 362-365):

```rust
// Check if we need to send a resize command (with 30ms debounce)
if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
    // Convert physical pixels to logical (CEF expects DIP coordinates)
    // Scale factor: macOS base DPI is 72, Retina is 144 (scale = 2.0)
    let scale = self.dimensions.dpi as f32 / 72.0;
    let scale = if scale <= 0.0 { 2.0 } else { scale };
    let logical_w = (viewport_w / scale) as u32;
    let logical_h = (viewport_h / scale) as u32;

    xpc_manager.check_and_send_resize(*pane_id, logical_w, logical_h);
}
```

#### Files to Modify

| File                                            | Changes                                            |
| ----------------------------------------------- | -------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Divide viewport dimensions by scale before sending |

No changes needed to profile server — it already handles logical dimensions
correctly.

#### Expected Behavior

With the fix:

- Initial spawn: logical 1033×1050 → physical IOSurface 2066×2100 ✓
- Resize sends: 1034×1050 logical (was 2067×2100 physical)
- CEF renders at: 2068×2100 physical ✓

The IOSurface size should remain consistent with the initial render, just at the
new dimensions.

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Basic webview works
web google.com
# Expected: Page renders correctly

# Test 2: Check initial dimensions
cat /tmp/termsurf-profile-*.log | grep "Sending IOSurface" | head -3
# Expected: ~2066x2100 (physical, 2× logical)

# Test 3: Split pane and verify resize
# Press Cmd+Shift+D to split
cat /tmp/termsurf-gui.log | grep "Sent resize"
# Expected: Dimensions should be ~1000x1050 (logical, not ~2000x2100)

cat /tmp/termsurf-profile-*.log | grep "Sending IOSurface" | tail -5
# Expected: ~2000x2100 (physical, matching 2× the logical resize)

# Test 4: Visual verification
# After split, webview should re-render at smaller size
# Text should remain crisp (same resolution density)
# Content should reflow to fit narrower width

# Test 5: Multiple resizes
# Drag window edge to resize
# Webview should update smoothly after 30ms debounce
# Check logs show multiple resize commands with correct logical dimensions
```

#### Success Criteria

- [ ] `web google.com` renders correctly (no change from Experiment 2)
- [ ] Split pane sends logical dimensions (half of previous physical / scale)
- [ ] IOSurface size after resize matches 2× logical dimensions
- [ ] Text remains crisp after resize (same pixel density)
- [ ] Content reflows naturally to new width
- [ ] Multiple consecutive resizes all work correctly
- [ ] Window drag resize works with 30ms debounce

#### Conclusion

**Result:** FAILED — Resize does not work most of the time. When it does work
(rarely), the dimensions are completely wrong, causing black bands and distorted
content.

**Symptoms:**

1. **Resize rarely triggers** — Changing pane size usually has no effect on the
   webview. The browser continues displaying at its original size.

2. **When resize does occur, dimensions are wrong** — The one time it did
   resize, large black bands appeared at top and bottom, and the content was
   squeezed horizontally. This indicates a fundamental mismatch between the sent
   dimensions and the expected dimensions.

3. **No consistent behavior** — There's no predictable pattern for when resize
   will or won't work.

**What the Logs Show:**

GUI logs confirm resize commands ARE being sent:

```
[XPC] Sent resize to pane 0: 799x795
[XPC] Sent resize to pane 0: 1716x1215
```

These are logical dimensions (physical / scale), which appears correct.

**Hypotheses for Why It's Failing:**

1. **XPC connection may be broken or stale**

   The peer connection stored in `peer_connections` HashMap might not be the
   same connection the profile server is listening on. The GUI thinks it's
   sending, but the messages may not be reaching the profile.

   The deferred state wrapper in the profile might be interfering with the
   connection setup. Setting the event handler before `gui.resume()` with an
   empty state wrapper, then populating it later, might cause issues with how
   XPC handles message delivery.

2. **Dimension source mismatch between initial spawn and resize**

   Initial spawn uses: `cols * cell_width / scale` Resize uses:
   `pos.pixel_width / scale`

   These are fundamentally different dimension sources. The pane layout
   dimensions (`pos.pixel_width`) may not match the terminal cell-based
   dimensions (`cols * cell_width`). This could explain why the aspect ratio is
   wrong when resize does occur.

3. **The resize render path may not be triggered**

   The webview overlay rendering only happens when there's a valid overlay in
   `self.webview_overlays`. If the overlay's dimensions aren't being updated
   when the pane resizes, or if the render function isn't being called when pane
   sizes change, the resize detection wouldn't run.

4. **Profile server may not be processing resize commands**

   The deferred state wrapper (`Arc<Mutex<Option<Arc<BrowserState>>>>`) might
   not be populated by the time resize commands arrive. If the handler checks
   `state_guard.as_ref()` and finds `None`, it silently ignores the command.

**Root Cause Analysis:**

After detailed code analysis, the root cause was identified: **there are two
separate data stores that are disconnected**.

1. **`XpcManager::received_surfaces`** (webview_xpc.rs:93) — where XPC stores
   incoming textures when `display_surface` messages arrive

2. **`WebviewOverlayState::overlays`** (webview_socket.rs:337) — where the
   render loop reads textures for drawing

These are **completely different data structures**. When the profile sends a new
texture after resize, it goes into `received_surfaces`, but the render loop
reads from `overlays`.

**Why Initial Spawn Works:**

```
CLI sends spawn_browser →
  socket handler spawns profile →
  socket handler POLLS get_received_surface() in a loop (line 476) →
  socket handler COPIES surface to WebviewOverlayState::overlays (line 504) →
  render loop reads from overlays ✓
```

**Why Resize Fails:**

```
render loop calls check_and_send_resize →
  sends resize_browser via XPC →
  profile resizes, calls on_accelerated_paint →
  profile sends display_surface via XPC →
  XPC handler stores in received_surfaces (webview_xpc.rs:230) →
  ❌ NOTHING copies to overlays →
  render loop still reads OLD texture from overlays
```

The new texture sits unused in `received_surfaces`. The render loop keeps
drawing the old texture because `overlays` is never updated.

**The Fix:**

Eliminate the dual state stores. The render loop should read directly from
`XpcManager::received_surfaces` instead of `WebviewOverlayState::overlays`. This
creates a single source of truth for received textures.

### Experiment 4: Single Source of Truth for Textures

**Status:** FAILED (partial success)

**Goal:** Fix the disconnect between where XPC stores textures and where the
render loop reads them. Make the render loop read directly from
`XpcManager::received_surfaces` so that resized textures are immediately
available for rendering.

#### The Problem

There are two separate state stores for webview textures:

| Store                           | Location              | Purpose                |
| ------------------------------- | --------------------- | ---------------------- |
| `XpcManager::received_surfaces` | webview_xpc.rs:93     | XPC writes here        |
| `WebviewOverlayState::overlays` | webview_socket.rs:337 | Render loop reads here |

The initial spawn includes explicit code to copy from one to the other:

```rust
// webview_socket.rs:476 - polling loop during spawn_browser
let surface = loop {
    if let Some(s) = xpc_manager.get_received_surface(pane_id) {
        break s;
    }
    // ... timeout handling ...
};

// webview_socket.rs:504 - copy to overlays
state.write().unwrap().add_overlay(pane_id, overlay);
```

But when resize sends a new texture, this copy never happens. The XPC handler
stores to `received_surfaces`, but nothing propagates it to `overlays`.

#### Solution: Read Directly from XpcManager

Instead of maintaining two state stores, have the render loop read directly from
`XpcManager::received_surfaces`. This makes XpcManager the single source of
truth for texture data.

**Before (dual state stores):**

```
XPC → received_surfaces → (manual copy) → overlays → render loop
```

**After (single source of truth):**

```
XPC → received_surfaces → render loop
```

#### Changes

**1. Modify render loop to read from XpcManager**

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

Change `render_webview_overlays_webgpu` to read from XpcManager instead of
WebviewOverlayState:

```rust
fn render_webview_overlays_webgpu(
    &self,
    webgpu: &crate::termwindow::webgpu::WebGpuState,
    output_texture: &wgpu::Texture,
) -> anyhow::Result<()> {
    use crate::termwindow::webview_xpc::get_xpc_manager;
    use cef::osr_texture_import::iosurface::IOSurfaceImporter;
    use cef::osr_texture_import::TextureImporter;
    use cef::sys::cef_color_type_t;

    // Get XPC manager (single source of truth for textures)
    let xpc_manager = match get_xpc_manager() {
        Some(m) => m,
        None => return Ok(()),
    };

    // Get positioned panes for viewport calculation
    let positioned_panes = self.get_panes_to_render();

    // Check if tab bar is visible (needed for offset calculation)
    let tab_bar_height = if self.show_tab_bar {
        self.tab_bar_pixel_height().unwrap_or(0.)
    } else {
        0.0
    };
    let border = self.get_os_border();

    let output_view = output_texture.create_view(&wgpu::TextureViewDescriptor::default());

    // Iterate over positioned panes and check if each has a webview texture
    for pos in positioned_panes.iter() {
        let pane_id = pos.pane.pane_id();

        // Check if this pane has a received surface from XPC
        let surface = match xpc_manager.get_received_surface(pane_id) {
            Some(s) => s,
            None => continue, // No webview for this pane
        };

        if surface.mach_port == 0 {
            continue;
        }

        log::info!(
            "[Render] Importing IOSurface for pane {} from mach_port={}, size={}x{}",
            pane_id,
            surface.mach_port,
            surface.width,
            surface.height
        );

        // Import IOSurface from Mach port
        let importer = match IOSurfaceImporter::from_mach_port(
            surface.mach_port,
            cef_color_type_t::CEF_COLOR_TYPE_BGRA_8888,
            surface.width,
            surface.height,
        ) {
            Some(imp) => imp,
            None => {
                log::warn!(
                    "[Render] Failed to import IOSurface from mach_port={}",
                    surface.mach_port
                );
                continue;
            }
        };

        // Import to wgpu texture
        let texture = match importer.import_to_wgpu(&webgpu.device) {
            Ok(tex) => tex,
            Err(e) => {
                log::warn!("[Render] Failed to import IOSurface to wgpu: {}", e);
                continue;
            }
        };

        // ... rest of rendering code (texture view, sampler, bind group, render pass) ...

        // Calculate viewport from pane position
        let cell_width = self.render_metrics.cell_size.width as f32;
        let cell_height = self.render_metrics.cell_size.height as f32;

        let viewport_x = pos.left as f32 * cell_width + border.left.get() as f32;
        let viewport_y = pos.top as f32 * cell_height + tab_bar_height + border.top.get() as f32;
        let viewport_w = pos.pixel_width as f32;
        let viewport_h = pos.pixel_height as f32;

        log::info!(
            "[Render] Pane {} viewport: ({}, {}) {}x{}",
            pane_id, viewport_x, viewport_y, viewport_w, viewport_h
        );

        // Check if we need to send resize command (with 30ms debounce)
        let scale = self.dimensions.dpi as f32 / 72.0;
        let scale = if scale <= 0.0 { 2.0 } else { scale };
        let logical_w = (viewport_w / scale) as u32;
        let logical_h = (viewport_h / scale) as u32;

        xpc_manager.check_and_send_resize(pane_id, logical_w, logical_h);

        // ... render pass code ...
    }

    Ok(())
}
```

**Key Changes:**

1. Remove dependency on `get_server()` and `WebviewOverlayState`
2. Iterate over `positioned_panes` (all panes in the window)
3. For each pane, check `xpc_manager.get_received_surface(pane_id)`
4. If a surface exists, render it; if not, skip (not a webview pane)

**2. Track which panes have webviews**

The above approach has one problem: we're iterating all panes and checking
XpcManager for each. But `received_surfaces` only contains panes that have
received at least one texture. We also need to know which panes are webview
panes BEFORE the first texture arrives (for the 5-second timeout case).

**Solution:** Keep `WebviewOverlayState` for tracking which panes are webview
panes, but DON'T store texture data there. Use it only as a "this pane has a
webview" registry.

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

Simplify `WebviewOverlay` to remove texture data:

```rust
/// Marker that a pane has an active webview (texture data is in XpcManager)
#[derive(Debug, Clone)]
pub struct WebviewOverlay {
    /// Session ID for this webview (for cleanup/debugging)
    pub session_id: String,
}

impl WebviewOverlayState {
    pub fn add_overlay(&mut self, pane_id: PaneId, session_id: String) {
        log::info!("Marking pane {} as webview (session={})", pane_id, session_id);
        self.overlays.insert(pane_id, WebviewOverlay { session_id });
    }

    // Remove mach_port, width, height from WebviewOverlay
    // These now come exclusively from XpcManager::received_surfaces
}
```

**3. Update spawn_browser handler to not store texture data**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

In the `spawn_browser` handler, after waiting for the surface, mark the pane as
a webview but don't copy texture data:

```rust
// Wait for surface from XPC (existing polling loop)
let surface = loop {
    if let Some(s) = xpc_manager.get_received_surface(pane_id) {
        break s;
    }
    if start.elapsed() > max_wait {
        return Response::error(&request.id, "Timeout waiting for surface");
    }
    thread::sleep(poll_interval);
};

// Mark pane as webview (texture data stays in XpcManager)
state.write().unwrap().add_overlay(pane_id, session_id.clone());

Response::ok(...)
```

**4. Update render loop to use registry + XpcManager**

**File:** `ts3/wezterm-gui/src/termwindow/render/draw.rs`

```rust
fn render_webview_overlays_webgpu(...) -> anyhow::Result<()> {
    // Get webview registry (which panes have webviews)
    let server = match get_server() {
        Some(s) => s,
        None => return Ok(()),
    };
    let state = server.state();
    let webview_panes = state.read().unwrap();

    if webview_panes.overlays.is_empty() {
        return Ok(());
    }

    // Get XPC manager (texture data source)
    let xpc_manager = match get_xpc_manager() {
        Some(m) => m,
        None => return Ok(()),
    };

    let positioned_panes = self.get_panes_to_render();
    // ...

    // For each webview pane, get texture from XpcManager
    for (pane_id, _overlay) in webview_panes.overlays.iter() {
        // Get CURRENT texture from XpcManager (may have been updated by resize)
        let surface = match xpc_manager.get_received_surface(*pane_id) {
            Some(s) => s,
            None => {
                log::warn!("[Render] Webview pane {} has no surface yet", pane_id);
                continue;
            }
        };

        // Find pane position
        let pos = match positioned_panes.iter().find(|p| p.pane.pane_id() == *pane_id) {
            Some(p) => p,
            None => {
                log::warn!("[Render] Pane {} not in layout", pane_id);
                continue;
            }
        };

        // Import and render using surface from XpcManager
        // ... existing import and render code ...

        // Check if resize needed
        let scale = self.dimensions.dpi as f32 / 72.0;
        let scale = if scale <= 0.0 { 2.0 } else { scale };
        let logical_w = (pos.pixel_width as f32 / scale) as u32;
        let logical_h = (pos.pixel_height as f32 / scale) as u32;

        xpc_manager.check_and_send_resize(*pane_id, logical_w, logical_h);

        // ... render ...
    }

    Ok(())
}
```

#### Why This Fixes Resize

**Before:** Render loop reads `overlays.mach_port` which was set during initial
spawn and never updated.

**After:** Render loop reads `xpc_manager.get_received_surface(pane_id)` which
returns the LATEST texture received via XPC. When profile sends a resized
texture, it immediately appears in `received_surfaces` and the next render frame
uses it.

The flow becomes:

```
1. GUI detects resize in render loop
2. GUI sends resize_browser via XPC
3. Profile resizes browser, CEF calls on_accelerated_paint
4. Profile sends display_surface via XPC
5. XPC handler stores NEW texture in received_surfaces
6. Next render frame reads NEW texture from received_surfaces ✓
7. GUI renders at new size
```

No manual copying between state stores. The texture flows directly from XPC to
the render loop.

#### Files to Modify

| File                                               | Changes                                                         |
| -------------------------------------------------- | --------------------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Read texture from XpcManager instead of overlays                |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Remove texture data from WebviewOverlay (keep only as registry) |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Initial render still works
web google.com
# Expected: Page renders correctly

# Test 2: Split pane triggers resize
# Press Cmd+Shift+D
cat /tmp/termsurf-gui.log | grep "Sent resize"
# Expected: Resize command sent with logical dimensions

cat /tmp/termsurf-profile-*.log | grep "Sending IOSurface"
# Expected: New IOSurface with smaller dimensions

# CRITICAL TEST: Verify render loop uses NEW texture
# After split, the webview should show at SMALLER size
# If it still shows at original size, texture propagation is broken

# Test 3: Drag window edge
# Expected: Webview resizes smoothly after 30ms debounce
# Multiple resize commands should appear in logs

# Test 4: Visual verification
# Text should remain crisp after resize (not stretched)
# Content should reflow to fit new width
```

#### Success Criteria

- [ ] Render loop reads texture from `XpcManager::received_surfaces`
- [ ] Initial spawn still works (no regression)
- [ ] Split pane causes webview to resize and re-render
- [ ] Resized texture is immediately visible (no stale texture)
- [ ] Multiple consecutive resizes work correctly
- [ ] Window drag resize works with debounce
- [ ] Text remains crisp after resize
- [ ] Content reflows naturally

#### Risks

1. **Performance** — Reading from XpcManager on every frame adds a mutex lock.
   However, `received_surfaces` is a small HashMap and the lock is brief. Should
   be negligible.

2. **Race condition** — There's a brief window where `received_surfaces` might
   be updated mid-render. This is acceptable since we're just reading; the worst
   case is rendering the previous frame's texture.

3. **Memory** — The old texture in `received_surfaces` is replaced by the new
   one. CEF manages IOSurface lifetime, so the old surface should be released
   when no longer referenced.

#### Why Previous Experiments Failed

- **Experiment 1** failed due to XPC handler ordering (resume before handler)
- **Experiment 2** fixed the ordering but sent physical instead of logical
  pixels
- **Experiment 3** fixed the scale factor but the ACTUAL root cause was never
  addressed: the new texture was stored in the wrong place

All three experiments correctly implemented the resize COMMAND pathway:

```
GUI detects resize → sends command → profile resizes → CEF re-renders ✓
```

But none of them fixed the TEXTURE pathway:

```
CEF re-renders → sends texture → GUI stores in received_surfaces →
❌ render loop reads from overlays (WRONG PLACE)
```

This experiment fixes the texture pathway by making the render loop read from
the same place XPC writes to.

#### Conclusion

**Result:** FAILED (partial success) — Resize now triggers and new textures are
received, but behavior is inconsistent. Sometimes resize works correctly,
sometimes the webview appears stretched, and sometimes it doesn't fill the pane
dimensions properly.

**What Worked:**

- The single source of truth approach is correct — render loop now reads from
  `XpcManager::received_surfaces`
- Resize commands are being sent to the profile
- Profile is resizing the browser and sending new textures
- New textures are being received and rendered

**What's Still Broken:**

1. **Inconsistent resize triggering** — Resize doesn't always happen when the
   window size changes. No predictable pattern for when it works vs doesn't.

2. **Stretched appearance** — Sometimes the webview appears visibly stretched,
   indicating texture dimensions don't match viewport dimensions.

3. **Doesn't fill pane** — Sometimes the webview doesn't fill the pane
   dimensions correctly, leaving gaps or overflowing.

**Hypotheses for Remaining Issues:**

1. **Texture/Viewport Size Mismatch During Transition**

   When resize occurs, there's a transition period where the old texture is
   rendered at the new viewport size (causing stretching). If the new texture
   never arrives or arrives with wrong dimensions, stretching persists.

2. **Debounce Logic Prevents Resize Commands**

   The 30ms debounce requires the render loop to run after the delay. If the
   render loop doesn't run (static content, no cursor blink), the elapsed time
   check never happens. Also, `last_sent_size` is updated optimistically before
   confirming the resize succeeded.

3. **Scale Factor Inconsistency**

   Initial spawn uses pane's DPI: `dims.dpi as f32 / 72.0`
   Resize uses window's DPI: `self.dimensions.dpi as f32 / 72.0`

   If these differ, the logical dimensions sent for resize won't match what was
   used for initial spawn, causing texture size mismatches.

4. **Render Loop Timing**

   Resize detection only happens when `render_webview_overlays_webgpu` runs. If
   terminal content is static with no animations, the render loop might not run
   frequently enough to detect size changes.

5. **Viewport Position Drift**

   The viewport calculation depends on cell dimensions, tab bar height, and
   border offsets. If any of these change without the viewport being
   recalculated, the webview won't align with the pane.

**Most Likely Root Causes:**

- Scale factor inconsistency between initial spawn and resize
- Transition period where old texture is rendered at new viewport size
- Debounce logic not triggering reliably

**Next Steps:**

- Add logging to compare scale factors used in initial spawn vs resize
- Investigate why resize doesn't trigger consistently
- Consider forcing a render loop iteration after resize events
