# TermSurf 3.0 Resize Support

## Background

This document continues from [ts3-5-profile.md](./ts3-5-profile.md), which fixed
profile isolation so each `--profile` value creates a separate CEF data
directory at `~/.config/termsurf/cef/<profile>/`.

### What We Accomplished (ts3-5)

One experiment fixed the profile path from `~/Library/Application Support/` to
`~/.config/termsurf/cef/` by replacing `dirs_next::config_dir()` with
`$HOME/.config/termsurf/cef/`. Verified that `default`, `test1`, and `test2`
profiles each create separate directories.

### Current Limitations

The webview renders at a hardcoded 800x600 logical pixels with a hardcoded 2.0
device scale factor. This means:

1. The rendered page doesn't match the pane size -- it's always 800x600
   regardless of how large or small the pane is.
2. Resizing the window or splitting panes doesn't re-render the page at the new
   size.
3. The scale factor assumes Retina (2x) on all displays.

### New Goal

Make the webview render at the correct size and re-render when the pane resizes.

**Success looks like:**

- Opening `web google.com` renders at the actual pane size, not 800x600
- Resizing the window re-renders the page at the new size
- Splitting panes (reducing space) re-renders at the smaller size
- Retina displays render at 2x, non-Retina at 1x (determined dynamically)

## Research: How ts2 Solved This

ts2 had CEF in-process, so the render handler had direct access to pane
dimensions via `Rc<RefCell<(u32, u32)>>`. ts3 has CEF in a separate process, so
dimensions must be communicated via XPC. But the CEF-side logic is the same.

### Retina / Device Scale Factor

ts2 converts physical pixels to logical pixels for CEF:

```rust
// ts2/wezterm-gui/src/termwindow/render/pane.rs (lines 845-848)
const MACOS_BASE_DPI: f32 = 72.0;
let device_scale_factor = self.dimensions.dpi as f32 / MACOS_BASE_DPI;
let logical_width = (width / device_scale_factor) as u32;
let logical_height = (browser_height / device_scale_factor) as u32;
```

CEF works in logical pixels. `view_rect()` returns the logical size, and
`screen_info()` returns the device scale factor. CEF internally multiplies
logical size by scale factor to get the physical IOSurface size.

For example, on a Retina display (DPI 144, scale factor 2.0):

- Pane is 1600x1200 physical pixels
- `view_rect()` reports 800x600 logical
- `screen_info()` reports `device_scale_factor = 2.0`
- CEF renders a 1600x1200 IOSurface

ts2's `screen_info` reads from a stored `device_scale_factor` field:

```rust
// ts2/wezterm-gui/src/cef_browser/mod.rs (lines 584-592)
fn screen_info(&self, _browser: Option<&mut Browser>,
               screen_info: Option<&mut ScreenInfo>) -> i32 {
    if let Some(screen_info) = screen_info {
        screen_info.device_scale_factor = self.handler.device_scale_factor;
        return 1;
    }
    0
}
```

### Dynamic view_rect

ts2 reads size from a shared `Rc<RefCell<(u32, u32)>>`:

```rust
// ts2/wezterm-gui/src/cef_browser/mod.rs (lines 567-579)
fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
    let (width, height) = *self.handler.size.borrow();
    if let Some(rect) = rect {
        if width > 0 && height > 0 {
            rect.width = width as i32;
            rect.height = height as i32;
        }
    }
}
```

### Resize Flow

When the pane size changes, ts2 calls `browser.resize()`:

```rust
// ts2/wezterm-gui/src/cef_browser/mod.rs (lines 262-291)
pub fn resize(&self, width: u32, height: u32) {
    *self.size.borrow_mut() = (width, height);
    if let Some(host) = self.host() {
        host.was_resized();        // Tell CEF viewport changed
        host.invalidate(...);      // Force repaint
        do_message_loop_work();    // Process immediately (ts2 pumps manually)
    }
}
```

`host.was_resized()` causes CEF to call `view_rect()` again to get the new size,
then re-render at that size. The next `on_accelerated_paint` delivers a new
IOSurface at the updated dimensions.

### The Bouncing Problem and Settle Delay

ts2 had a "bouncing" problem: during continuous window resize (dragging the
edge), the texture would bounce around and often settle in the wrong position.
The root cause was never fully diagnosed, but rapid resize events caused texture
mismatches -- CEF may still be rendering a previous frame when the next resize
arrives.

ts2 solved this with a **settle-and-rerender** pattern: wait for the resize to
stop, then do one final resize at the settled size. The actual delay was
**30ms** (the documentation said 10ms but the code used 30ms).

```rust
// ts2/wezterm-gui/src/termwindow/render/pane.rs (line 820)
const SETTLE_DELAY: Duration = Duration::from_millis(30);
```

The logic, called every paint frame:

```rust
// ts2/wezterm-gui/src/termwindow/render/pane.rs (lines 852-876)

// 1. If target size changed, mark the time
if browser.get_pending_size() != Some(target_size) {
    browser.set_pending_size(logical_width, logical_height);
    browser.mark_resize_time();
}

// 2. If timer is running, check if we've settled
if let Some(elapsed) = browser.time_since_last_resize() {
    if elapsed >= SETTLE_DELAY {
        // 30ms with no size change -- do the resize
        browser.resize(logical_width, logical_height);
        browser.clear_resize_time();
        browser.clear_pending_size();
    } else {
        // Still settling -- keep painting to check again next frame
        window.invalidate();
    }
}
```

**For ts3:** We prefer continuous resize (no delay). The settle delay is a
fallback if the bouncing problem recurs.

### Key Difference: ts3 Doesn't Need do_message_loop_work()

ts2 used `do_message_loop_work()` to pump CEF's event queue from the GUI's event
loop. ts3 doesn't need this -- the profile server runs `cef::run_message_loop()`
in its own process, so CEF processes events (including resize) automatically.

## Architecture

### XPC Connection Topology

```
GUI Process                           Profile Server Process
    |                                         |
    |-- XpcListener (anonymous) ----+         |
    |                               |         |
    |   [launcher relays endpoint]  |         |
    |                               v         |
    |                    Profile connects ---->|
    |                               |         |
    |<-- profile_conn (accepted) ---+         |
    |                                         |
    |--- send("resize", w, h) ------------->  |  (NEW: GUI to profile)
    |                                         |
    |<-- send("display_surface", port) -----  |  (existing: profile to GUI)
```

The profile server already connects directly to the GUI (via the launcher's
endpoint relay). The GUI accepts this connection in its
`new_connection_handler`. Currently the GUI only **receives** on this
connection. To send resize messages, the GUI needs to **store a reference to the
accepted connection** and use it to send messages back.

On the profile side, the event handler on the GUI connection currently only
sends -- it doesn't listen for incoming messages. Adding a `set_event_handler`
on the profile's GUI connection would let it receive resize messages.

### Data Flow for Initial Size

```
web CLI --("open_webview", url, profile)--> GUI
    GUI computes pane pixel dimensions and DPI
    GUI sends spawn_profile to launcher with width, height, scale
    Launcher spawns: termsurf-profile --width W --height H --scale S ...
    Profile uses W, H, S in view_rect() and screen_info()
```

### Data Flow for Resize

```
Window resize / pane split detected by GUI
    GUI computes new pane pixel dimensions
    GUI converts to logical: logical = physical / scale_factor
    GUI sends XPC message: { action: "resize", width: W, height: H }
    Profile receives, updates stored size
    Profile calls host.was_resized()
    CEF calls view_rect() -> returns new size
    CEF re-renders at new size
    on_accelerated_paint fires with new IOSurface
    Profile sends new IOSurface to GUI
    GUI renders updated texture
```

## Tasks

- [x] Pass initial pane size from GUI through launcher to profile server
- [x] Make view_rect() and screen_info() read from dynamic shared state
- [ ] Add bidirectional XPC: GUI sends resize messages to profile server
- [ ] Profile server receives resize messages and calls `was_resized()`
- [ ] GUI detects pane resize and sends new dimensions to profile
- [ ] Verify resize works when expanding/shrinking the window
- [ ] Verify resize works when splitting panes
- [ ] (If needed) Add settle delay to prevent bouncing

## Experiments

### Experiment 1: Pass Initial Size at Startup

**Status:** SUCCESS

**Goal:** Remove the hardcoded 800x600 and 2.0 scale factor. Pass the actual
pane dimensions and DPI from the GUI to the profile server at spawn time.

**Result:** The webview now renders at the actual pane size. The GUI looks up
pane pixel dimensions via `Mux::try_get()` → `get_pane()` → `get_dimensions()`,
computes logical size and scale factor, and passes them through the launcher to
the profile server as CLI args. The profile server's `view_rect()` and
`screen_info()` read from shared state instead of hardcoded values.

#### Changes

**1. Add CLI args to profile server**

**File:** `ts3/termsurf-profile/src/main.rs`

Add `--width`, `--height`, and `--scale` to the `Args` struct:

```rust
#[derive(Parser)]
struct Args {
    #[arg(long)]
    profile: String,

    #[arg(long)]
    url: String,

    #[arg(long)]
    session_id: String,

    #[arg(long)]
    width: u32,

    #[arg(long)]
    height: u32,

    #[arg(long)]
    scale: f32,
}
```

Store width, height, and scale in `SharedState`:

```rust
struct SharedState {
    gui: std::sync::Arc<XpcConnection>,
    url: String,
    width: std::sync::atomic::AtomicU32,
    height: std::sync::atomic::AtomicU32,
    scale: f32,  // Immutable after startup (for now)
}
```

Use atomics so the render handler can read without a lock.

Update `view_rect()` to read from shared state:

```rust
fn view_rect(&self, _browser: Option<&mut Browser>, rect: Option<&mut Rect>) {
    if let Some(rect) = rect {
        rect.width = self.inner.state.width.load(Ordering::Relaxed) as i32;
        rect.height = self.inner.state.height.load(Ordering::Relaxed) as i32;
    }
}
```

Update `screen_info()` to read from shared state:

```rust
fn screen_info(&self, _browser: Option<&mut Browser>,
               screen_info: Option<&mut ScreenInfo>) -> i32 {
    if let Some(info) = screen_info {
        info.device_scale_factor = self.inner.state.scale;
        return 1;
    }
    0
}
```

**2. GUI computes pane dimensions and passes them through**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs`

In the `open_webview` handler, compute the pane pixel dimensions and DPI. This
requires access to `TermWindow`'s `dimensions` and `render_metrics`. Pass width,
height, and scale to `request_profile_spawn()`.

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

Add `width`, `height`, `scale` parameters to `request_profile_spawn()`. Include
them in the `spawn_profile` XPC message to the launcher.

**File:** `ts3/termsurf-launcher/src/main.rs`

Read `width`, `height`, `scale` from the `spawn_profile` message and pass them
as CLI args when spawning the profile server.

#### Files to Modify

| File                                               | Changes                                                           |
| -------------------------------------------------- | ----------------------------------------------------------------- |
| `ts3/termsurf-profile/src/main.rs`                 | Add CLI args, store in SharedState, read in view_rect/screen_info |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Compute pane dimensions, pass to spawn                            |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Add width/height/scale to spawn request                           |
| `ts3/termsurf-launcher/src/main.rs`                | Pass width/height/scale as CLI args                               |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# Open webview -- should render at pane size, not 800x600
web google.com

# Check profile log for actual dimensions:
cat /tmp/termsurf-profile-*.log | grep "Cache\|view_rect\|coded_size"
```

#### Success Criteria

- [x] Profile log shows dimensions matching the pane size, not 800x600
- [x] IOSurface dimensions in the log match the pane physical pixel size
- [x] Page renders at the correct size (fills the pane, not too small or large)

---

### Experiment 2: Bidirectional XPC for Resize

**Status:** PLANNED

**Goal:** When the pane resizes, send the new dimensions to the profile server
via XPC so CEF re-renders at the correct size.

This experiment depends on Experiment 1 (dynamic size in profile server).

#### Changes

**1. GUI stores reference to profile connection**

**File:** `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`

In the `new_connection_handler` (which fires when the profile connects), store
the accepted `XpcConnection` alongside the surface receiver. This gives the GUI
a channel to send messages back to the profile.

**2. Profile server listens for incoming messages**

**File:** `ts3/termsurf-profile/src/main.rs`

Set an event handler on the GUI connection to receive messages. When a `resize`
message arrives, update the `width` and `height` atomics in `SharedState`, then
tell CEF the viewport changed.

The profile server needs a reference to the browser host to call
`was_resized()`. Store it in `SharedState` after browser creation in
`on_context_initialized`.

```rust
// On receiving resize message:
state.width.store(new_width, Ordering::Relaxed);
state.height.store(new_height, Ordering::Relaxed);
if let Some(host) = state.browser_host.lock().unwrap().as_ref() {
    host.was_resized();
}
```

CEF will then call `view_rect()` (which reads the updated atomics), re-render,
and fire `on_accelerated_paint` with a new IOSurface.

**3. GUI sends resize on pane size change**

**File:** `ts3/wezterm-gui/src/termwindow/webview_socket.rs` (or wherever pane
resize is detected)

When the GUI detects that a pane with an active webview has changed size,
compute the new logical dimensions and send an XPC message:

```rust
let msg = XpcDictionary::new();
msg.set_string("action", "resize");
msg.set_i64("width", logical_width as i64);
msg.set_i64("height", logical_height as i64);
profile_conn.send(&msg);
```

The exact location where pane resize is detected needs investigation. In ts2,
this was `paint_browser_overlay()` which checked size every frame. In ts3, a
similar check would go in the webview rendering path.

#### Files to Modify

| File                                               | Changes                                               |
| -------------------------------------------------- | ----------------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Store profile connection, add send_resize method      |
| `ts3/termsurf-profile/src/main.rs`                 | Event handler for resize messages, store browser host |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Detect pane resize, send new dimensions               |

#### Verification

```bash
cd ts3
./scripts/build-debug.sh --open

# Open webview
web google.com

# Resize the window by dragging the edge
# Page should re-render at the new size

# Split the pane (if supported)
# Page should re-render at the smaller size
```

#### Success Criteria

- [ ] Resizing the window re-renders the page at the new size
- [ ] Profile log shows updated dimensions after resize
- [ ] No crashes during rapid resize
- [ ] (Stretch) Splitting panes re-renders at smaller size

---

### Experiment 3: Settle Delay (If Needed)

**Status:** PLANNED (contingent on bouncing problem)

**Goal:** If Experiment 2 causes bouncing during resize, add a settle delay to
wait for the resize to finish before re-rendering.

This experiment is only needed if the bouncing problem from ts2 recurs. Skip if
continuous resize works correctly.

#### Changes

Add settle delay logic to the GUI's resize detection. Instead of sending a
resize message immediately, wait for the size to stabilize:

```rust
const SETTLE_DELAY: Duration = Duration::from_millis(30);

// On pane resize detected:
if new_size != pending_size {
    pending_size = new_size;
    last_resize_time = Instant::now();
}

if last_resize_time.elapsed() >= SETTLE_DELAY {
    send_resize_to_profile(pending_size);
    last_resize_time = None;
    pending_size = None;
}
```

ts2 used 30ms. Adjust as needed.

#### Success Criteria

- [ ] No bouncing during rapid window resize
- [ ] Page settles at the correct size after resize stops
- [ ] Delay is imperceptible (< 50ms)

---

### Next Steps (After This Document)

Once resize is working:

1. **Keyboard input** -- Type in form fields, use keyboard shortcuts
2. **Mouse input** -- Click links, scroll, hover states
3. **Navigation** -- Back, forward, reload, URL changes
4. **Multiple pages** -- Open multiple webviews simultaneously
5. **Page lifecycle** -- Handle page loads, errors, redirects
6. **DevTools** -- Open Chrome DevTools for debugging
