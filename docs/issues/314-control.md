# Issue 314: Control Panel

## Goal

Implement a control panel for webview panes in ts3, matching the appearance and
behavior of ts2. The control panel displays the URL and provides visual feedback
about the browser mode.

## Background

### TermSurf 1.x (Ghostty + WKWebView)

The original control panel was implemented in Swift using NSTextField. It had a
modal system with Control mode (terminal keybindings) and Browse mode (browser
input).

### TermSurf 2.0 (WezTerm + in-process CEF)

ts2 ported the control panel to WezTerm's Rust/wgpu rendering. Key files:

- `ts2/wezterm-gui/src/cef_browser/mod.rs` — `BrowserState` and `BrowserMode`
- `ts2/wezterm-gui/src/termwindow/render/pane.rs` — Rendering logic
- `docs/issues/205-cef-mvp4.md` — Specification document

The ts2 control panel:

- **Position**: Top of the webview pane
- **Height**: 2 cell heights (half-cell top margin + text + half-cell bottom)
- **Margins**: Half-cell on left, top, and bottom
- **Content**: URL in Browse mode, instructions in Control mode

### TermSurf 3.0 (WezTerm + out-of-process CEF)

ts3 uses XPC to communicate with a separate profile server process. The URL is
not stored in-process — it must be received via XPC from termsurf-profile.

## Requirements

### Phase 1: Visual Appearance (This Issue)

The control panel should look exactly like ts2:

```
┌──────────────────────────────────────────────────────────────────┐
│                                                                  │  ← half-cell top margin
│  https://google.com                                              │  ← URL text
│                                                                  │  ← half-cell bottom margin
├──────────────────────────────────────────────────────────────────┤
│                                                                  │
│                    Webview content here                          │
│                                                                  │
└──────────────────────────────────────────────────────────────────┘
```

**Dimensions:**

- Control bar height: `cell_height * 2.0` (exactly 2 cell heights)
- Left margin: `cell_width / 2.0`
- Top margin: `cell_height / 2.0`
- Bottom margin: `cell_height / 2.0`
- Right margin: 0 (text truncates with ellipsis)

**Colors:**

- Background: Terminal palette background
- Text: Terminal palette foreground

**Font:**

- Default monospace font (same as terminal)

### Phase 2: Mode Switching (Future)

After the control panel renders, add:

- Control mode / Browse mode switching
- `Enter` to enter Browse mode
- `Ctrl+C` to exit browser
- Visual dimming in Control mode

### Phase 3: Input Forwarding (Future)

- Forward keyboard input to CEF in Browse mode
- Forward mouse input to CEF in Browse mode

## Technical Approach

### Differences from ts2

| Aspect         | ts2                                             | ts3                                  |
| -------------- | ----------------------------------------------- | ------------------------------------ |
| CEF process    | In-process                                      | Out-of-process (XPC)                 |
| URL source     | `BrowserState.url` field                        | XPC message from profile             |
| Texture source | Direct CEF callback                             | XPC Mach port transfer               |
| State storage  | `browser_states: HashMap<PaneId, BrowserState>` | `WebviewOverlayState` + `XpcManager` |

### URL Transmission

Currently, ts3 transmits texture data (Mach port, dimensions) via XPC but not
the URL. Two options:

**Option A: Add URL to XPC protocol**

1. Profile server sends URL with each texture update
2. XPC manager stores URL alongside texture data
3. Render code retrieves URL from XPC manager

**Option B: Store URL locally from web command**

1. The `web` command already includes the URL
2. Store it in `WebviewOverlay` when the command is received
3. Use stored URL for display (won't update on navigation)

**Decision: Option A.** We want the URL to update when the user navigates, so
we'll add URL to the XPC protocol from the start.

### Rendering Architecture

ts2 uses a two-phase rendering pattern to avoid wgpu buffer conflicts:

1. **Phase 1 (during `paint_pane`)**: Render control bar background via
   `filled_rectangle` while the layers buffer is mapped
2. **Phase 2 (after layers dropped)**: Render control bar text via
   `render_element` in a separate pass

ts3 doesn't use WezTerm's pane rendering for webviews — it renders directly in
`render_webview_overlays_webgpu`. We need to integrate control bar rendering
into this flow.

**Approach for ts3:**

1. In `render_webview_overlays_webgpu`, before rendering the webview texture:
   - Calculate control bar bounds (top 2 cell heights)
   - Render control bar background as a filled quad
   - Adjust webview viewport to start below control bar

2. After webview texture rendering:
   - Render control bar text using WezTerm's text rendering system

### Viewport Adjustment

Currently, the webview fills the entire pane. With the control bar, we need to:

1. Reserve 2 cell heights at the top for the control bar
2. Reduce the webview height by 2 cell heights
3. Send adjusted size to CEF via XPC resize command

```rust
// Current (fills entire pane)
let (viewport_x, viewport_y, viewport_w, viewport_h) = calculate_pane_bounds();

// With control bar
let control_bar_height = cell_height * 2.0;
let webview_y = viewport_y + control_bar_height;
let webview_h = viewport_h - control_bar_height;
```

## Implementation Plan

### Step 1: Store URL in WebviewOverlay

**File: `ts3/wezterm-gui/src/termwindow/webview_socket.rs`**

Add `url` field to `WebviewOverlay`:

```rust
pub struct WebviewOverlay {
    pub session_id: String,
    pub tab_id: TabId,
    pub url: String,  // NEW: URL for control bar display
}
```

Update `handle_open_webview` to store the URL when the command is received.

### Step 2: Calculate Control Bar Bounds

**File: `ts3/wezterm-gui/src/termwindow/render/draw.rs`**

In `render_webview_overlays_webgpu`, after calculating pane bounds:

```rust
// Control bar dimensions (matching ts2 exactly)
let cell_height = self.render_metrics.cell_size.height as f32;
let cell_width = self.render_metrics.cell_size.width as f32;
let control_bar_height = cell_height * 2.0;

// Control bar rect
let control_bar_x = viewport_x;
let control_bar_y = viewport_y;
let control_bar_w = viewport_w;
let control_bar_h = control_bar_height;

// Adjusted webview rect (below control bar)
let webview_y = viewport_y + control_bar_height;
let webview_h = viewport_h - control_bar_height;
```

### Step 3: Render Control Bar Background

Render a filled rectangle for the control bar background. This can be done:

**Option A: Separate render pass**

Create a simple quad and render it before the webview texture.

**Option B: Use WezTerm's filled_rectangle**

If we can access the layers buffer, use the existing infrastructure.

For Phase 1, Option A is likely simpler since we're already in a separate
rendering path.

### Step 4: Render Control Bar Text

Use WezTerm's `render_element` or similar text rendering to display the URL.

Key considerations:

- Text must be rendered AFTER the background
- May need separate command buffer submission
- Follow ts2's pattern from `paint_browser_control_bars`

### Step 5: Adjust CEF Viewport

Update the resize command sent to CEF to exclude control bar height:

```rust
// Send resize with reduced height
xpc_manager.send_resize(
    *pane_id,
    logical_w,
    logical_h - (control_bar_height / scale) as u32,
);
```

## Files to Modify

| File                                               | Changes                             |
| -------------------------------------------------- | ----------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add `url` field to `WebviewOverlay` |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Add control bar rendering           |
| `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`    | Adjust resize commands              |

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open a webview
web google.com

# 2. Verify control bar appears
# - Should see 2-cell-height bar at top of webview pane
# - Should display "https://google.com" with half-cell margins
# - Webview content should start below control bar

# 3. Test with long URL
web https://example.com/some/very/long/path/that/should/truncate

# 4. Test resize
# - Drag window edge
# - Control bar should resize with pane
# - No rendering artifacts

# 5. Test splits
split-pane
web github.com
# Each webview pane should have its own control bar
```

## Success Criteria

### Phase 1 (This Issue)

1. [ ] Control bar renders at top of webview pane
2. [ ] Control bar height is exactly 2 cell heights
3. [ ] Background uses terminal palette background color
4. [ ] URL text displays with half-cell margins (left, top, bottom)
5. [ ] URL truncates with ellipsis if too long
6. [ ] Webview content renders below control bar (not overlapping)
7. [ ] CEF receives correct viewport size (excluding control bar)
8. [ ] Control bar resizes correctly when pane resizes
9. [ ] Multiple webview panes each have their own control bar

### Phase 2 (Future)

- [ ] Control mode / Browse mode switching works
- [ ] Visual dimming in Control mode
- [ ] Click handling for mode switching

### Phase 3 (Future)

- [ ] Keyboard input forwarded to CEF in Browse mode
- [ ] Mouse input forwarded to CEF in Browse mode

## References

- `docs/issues/205-cef-mvp4.md` — ts2 control panel specification
- `ts2/wezterm-gui/src/termwindow/render/pane.rs:813-966` — ts2 rendering code
- `ts2/wezterm-gui/src/cef_browser/mod.rs:28-57` — ts2 BrowserState struct

---

## Experiments

### Experiment 1: Add URL to XPC Protocol

**Goal:** Transmit the current URL from the profile server to the GUI via XPC,
so the control panel can display it. The URL should update when pages navigate.

**Background:**

Currently, the `display_surface` XPC message contains:

```rust
msg.set_string("action", "display_surface");
msg.set_mach_send("iosurface_port", port);
msg.set_i64("width", width as i64);
msg.set_i64("height", height as i64);
```

We need to add the URL to this message. The URL is available in CEF via the
`Browser::get_main_frame().get_url()` method.

#### Changes

**Step 1: Store URL in BrowserState (profile server)**

**File: `ts3/termsurf-profile/src/main.rs`**

Add `url` field to `BrowserState`:

```rust
struct BrowserState {
    session_id: String,
    gui: Arc<XpcConnection>,
    width: AtomicU32,
    height: AtomicU32,
    last_handle: AtomicPtr<c_void>,
    browser: Mutex<Option<cef::Browser>>,
    url: Mutex<String>,  // NEW: Current URL for this browser
}
```

Initialize with the URL from browser creation:

```rust
let browser_state = Arc::new(BrowserState {
    session_id: session_id.to_string(),
    gui: gui_connection,
    width: AtomicU32::new(width),
    height: AtomicU32::new(height),
    last_handle: AtomicPtr::new(std::ptr::null_mut()),
    browser: Mutex::new(None),
    url: Mutex::new(url.to_string()),  // NEW
});
```

**Step 2: Update URL on navigation (profile server)**

CEF provides `on_address_change` callback via the `DisplayHandler`. We need to
implement this to update the stored URL when navigation occurs.

Add to `RenderHandler` (or create separate `DisplayHandler`):

```rust
impl DisplayHandler for RenderHandler {
    fn on_address_change(
        &self,
        _browser: &Browser,
        _frame: &Frame,
        url: &CefString,
    ) {
        let url_str = url.to_string();
        *self.inner.state.url.lock().unwrap() = url_str.clone();
        println!("Profile: URL changed to '{}'", url_str);
    }
}
```

**Step 3: Add URL to display_surface message (profile server)**

**File: `ts3/termsurf-profile/src/main.rs`** (in `on_paint`)

Add URL to the XPC message:

```rust
let msg = XpcDictionary::new();
msg.set_string("action", "display_surface");
msg.set_mach_send("iosurface_port", port);
msg.set_i64("width", width as i64);
msg.set_i64("height", height as i64);
msg.set_string("url", &self.inner.state.url.lock().unwrap());  // NEW
self.inner.state.gui.send(&msg);
```

**Step 4: Add URL to ReceivedSurface (GUI)**

**File: `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`**

Add `url` field to `ReceivedSurface`:

```rust
pub struct ReceivedSurface {
    pub mach_port: u32,
    pub width: u32,
    pub height: u32,
    pub url: String,  // NEW: Current URL from profile server
}
```

**Step 5: Extract URL from XPC message (GUI)**

**File: `ts3/wezterm-gui/src/termwindow/webview_xpc.rs`** (in message handler)

Extract URL when receiving `display_surface`:

```rust
"display_surface" => {
    let port = msg.copy_mach_send("iosurface_port");
    let width = msg.get_i64("width").unwrap_or(0) as u32;
    let height = msg.get_i64("height").unwrap_or(0) as u32;
    let url = msg.get_string("url").unwrap_or_default();  // NEW

    // ... existing port handling ...

    let surface = ReceivedSurface {
        mach_port: port,
        width,
        height,
        url,  // NEW
    };

    manager.received_surfaces.lock().unwrap().insert(pane_id, surface);
}
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open a webview
web google.com

# 2. Check profile log for URL storage
grep "URL changed" /tmp/termsurf-profile-default.log
# Expected: "Profile: URL changed to 'https://www.google.com/'"

# 3. Check GUI log for URL receipt
grep "url=" /tmp/termsurf-gui.log
# Expected: URL appears in display_surface message handling

# 4. (Future) Verify URL updates on navigation
# Once input forwarding is implemented, clicking links should update the URL
```

#### Success Criteria

1. [x] `BrowserState` in profile server stores URL
2. [x] URL initialized from browser creation arguments
3. [x] `on_address_change` callback updates URL on navigation
4. [x] `display_surface` XPC message includes URL
5. [x] `ReceivedSurface` in GUI stores URL
6. [x] URL extracted from XPC message correctly
7. [~] URL updates when page navigates (requires input forwarding to test fully)

#### Result

**Success.** URL transmission via XPC is working correctly.

#### Conclusion

**What was accomplished:**

The URL now flows from CEF through XPC to the GUI with every texture update:

1. **Profile server stores URL** — Added `url: Mutex<String>` to `BrowserState`,
   initialized from the browser creation arguments.

2. **DisplayHandler tracks navigation** — Implemented `ProfileDisplayHandler`
   with `on_address_change` callback that updates the stored URL whenever CEF
   navigates to a new page.

3. **URL included in XPC message** — The `display_surface` message now includes
   the current URL alongside the Mach port and dimensions.

4. **GUI receives and stores URL** — `ReceivedSurface` now has a `url` field,
   extracted from each incoming XPC message.

**Verification from logs:**

```
Profile: URL changed to 'https://www.google.com/'
[XPC Manager] Surface URL: 'https://www.google.com/'
```

**What this enables:**

The control panel rendering code (Experiment 2) can now access the current URL
via `xpc_manager.get_received_surface(pane_id).url` to display it in the
control bar.

**Files modified:**

- `ts3/termsurf-profile/src/main.rs` — Added URL to BrowserState, DisplayHandler,
  and XPC message
- `ts3/wezterm-gui/src/termwindow/webview_xpc.rs` — Added URL to ReceivedSurface

**Next step:**

Experiment 2 will implement the actual control panel rendering (background,
text, viewport adjustment).

---

### Experiment 2: Render Control Panel

**Goal:** Render a control panel at the top of each webview pane, displaying
the URL. The webview content should render below the control panel, not
overlapping it.

**Background:**

ts2 renders the control panel in two phases (see `pane.rs:813-966`):

1. **Phase 1 (paint_browser_overlay)**: Render background via `filled_rectangle`
   while layers buffer is mapped
2. **Phase 2 (paint_browser_control_bars)**: Render text via `render_element`
   after layers are dropped

ts3's webview rendering is different — it happens in a separate function
(`render_webview_overlays_webgpu`) that uses its own wgpu render passes. We need
to adapt the approach.

#### Approach

**Part A: Viewport adjustment and background**

1. Calculate control bar dimensions (2 cell heights)
2. Adjust webview viewport to start below control bar
3. Adjust CEF resize commands to use reduced height
4. Render control bar background as a filled quad

**Part B: Text rendering**

1. Access URL from `ReceivedSurface`
2. Use WezTerm's text rendering to display URL with half-cell margins

For this experiment, we'll implement Part A first. Part B (text) may require
additional investigation into WezTerm's text rendering from our render path.

#### Changes

**Step 1: Calculate control bar height**

In `render_webview_overlays_webgpu`, after getting cell dimensions:

```rust
let cell_height = self.render_metrics.cell_size.height as f32;
let cell_width = self.render_metrics.cell_size.width as f32;
let control_bar_height = cell_height * 2.0;
```

**Step 2: Adjust viewport for webview texture**

Currently the webview fills the entire pane. Change to:

```rust
// Control bar occupies top 2 cell heights
let webview_y = viewport_y + control_bar_height;
let webview_h = viewport_h - control_bar_height;

// Use adjusted viewport for webview texture
render_pass.set_viewport(viewport_x, webview_y, viewport_w, webview_h, 0.0, 1.0);
```

**Step 3: Adjust CEF resize to match**

The resize command should use the reduced height:

```rust
// Send resize with control bar height subtracted
let logical_h = ((viewport_h - control_bar_height) / scale) as u32;
xpc_manager.send_resize(*pane_id, logical_w, logical_h);
```

**Step 4: Render control bar background**

Before the webview render pass, create a separate render pass for the control
bar background:

```rust
// Render control bar background
{
    let mut control_bar_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("Control Bar Background"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &output_view,
            resolve_target: None,
            ops: wgpu::Operations {
                load: wgpu::LoadOp::Load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    });

    // Set viewport to control bar area
    control_bar_pass.set_viewport(
        viewport_x,
        viewport_y,
        viewport_w,
        control_bar_height,
        0.0,
        1.0,
    );

    // Use a simple solid color pipeline (or filled_rectangle equivalent)
    // TODO: Need to create or reuse a solid color shader/pipeline
}
```

**Step 5: Render URL text (Part B - may be separate experiment)**

After webview rendering, use WezTerm's Element system to render the URL:

```rust
// Get URL from received surface
let url = surface.url.clone();

// Create text element (similar to ts2's paint_browser_control_bars)
let element = Element::new(&font, ElementContent::Text(url))
    .colors(ElementColors { ... })
    .padding(BoxDimension {
        left: Dimension::Pixels(half_cell_width),
        top: Dimension::Pixels(half_cell_height),
        ...
    });

// Compute and render
let computed = self.compute_element(&layout_context, &element)?;
computed.translate(euclid::vec2(viewport_x, viewport_y));
self.render_element(&computed, gl_state, None)?;
```

#### Files to Modify

| File | Changes |
|------|---------|
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Viewport adjustment, control bar rendering |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open webview
web google.com

# 2. Verify control bar space
# - Top 2 cell heights should be reserved (may be empty or have background)
# - Webview content should start below this area

# 3. Verify no overlap
# - Webview should not cover the control bar area
# - No visual glitches at the boundary

# 4. Test resize
# - Drag window edge
# - Control bar area should maintain 2 cell height
# - Webview should resize correctly below it

# 5. Check logs for adjusted viewport
grep "viewport" /tmp/termsurf-gui.log
# Expected: Viewport Y should be offset by control bar height
```

#### Success Criteria

**Part A (viewport adjustment):**

1. [ ] Control bar height calculated as 2 cell heights
2. [ ] Webview viewport starts below control bar (Y offset)
3. [ ] Webview viewport height reduced by control bar height
4. [ ] CEF receives correct reduced height in resize commands
5. [ ] Control bar area visible (even if just terminal background showing through)
6. [ ] No visual overlap between control bar area and webview

**Part B (text rendering):**

1. [ ] URL retrieved from ReceivedSurface
2. [ ] URL rendered with half-cell margins
3. [ ] Text uses terminal palette colors
4. [ ] Long URLs truncate gracefully

#### Result

_Pending_
