# CEF MVP2: Web-Open Command

This document details the implementation plan for MVP2 - adding a `web-open` CLI command that displays a CEF browser overlay on a terminal pane.

## Implementation Status

**Completed:**
1. MuxNotification variants (`WebOpen`, `WebClosed`) added to `mux/src/lib.rs`
2. Session handler updated to publish `WebOpen` notification
3. Pattern matching updated in `dispatch.rs`, `frontend.rs`, `termwindow/mod.rs`
4. CEF browser module created at `wezterm-gui/src/cef_browser/mod.rs` (renamed from `cef` to avoid shadowing crate)
5. Browser state field added to TermWindow
6. `handle_web_open()` method implemented
7. `close_browser_for_pane()` method implemented
8. Keyboard handling (Ctrl+C to close) added to `keyevent.rs`
9. CEF import issues fixed (renamed local module to `cef_browser`)
10. Browser overlay placeholder rendering in `paint_browser_overlay()`

**Remaining:**
1. ~~Actual CEF texture rendering~~ - DONE (commit `247ba3b1c`)
2. Fix macOS terminal launch issue - see [Known Issues in cef.md](cef.md#macos-multiple-browsers-fail-when-launched-from-terminal)
3. Fix HiDPI/Retina resolution (content appears small on high-DPI displays)
4. Testing and validation

## Goal

```bash
wezterm cli web-open https://example.com
```

Opens a CEF browser displaying the URL in the current terminal pane. Press Ctrl+C to close the webview and return to the terminal.

## Current State

### What Already Exists (from MVP1)

1. **CEF initialization** - `wezterm-gui/src/cef_integration.rs`
   - `BrowserProcessHandler` with `on_schedule_message_pump_work` callback
   - Message pump properly integrated with macOS run loop
   - CEF loads and initializes successfully

2. **CLI plumbing** - Never reverted, still functional:
   - `wezterm/src/cli/web_open.rs` - CLI command implementation
   - `codec/src/lib.rs` - `WebOpen`, `WebOpenResponse`, `WebClosed` PDUs
   - `wezterm-client/src/client.rs` - `client.web_open()` method
   - `wezterm-mux-server-impl/src/sessionhandler.rs` - Handles `Pdu::WebOpen`

### What Was Reverted (commit 700fc3822) - Now Re-implemented

The following was reverted and has been re-implemented:

1. ✅ **MuxNotification variants** - `WebOpen` and `WebClosed` in `mux/src/lib.rs`
2. ✅ **CEF browser module** - `wezterm-gui/src/cef_browser/mod.rs` (~580 lines, renamed from `cef` to avoid shadowing)
3. ✅ **TermWindow integration** - `browser_states` field, `handle_web_open()`, `close_browser_for_pane()`
4. ✅ **Keyboard handling** - Ctrl+C detection in `keyevent.rs`
5. 🔄 **Browser rendering** - Placeholder implemented in `paint_browser_overlay()`, actual texture compositing pending

## Implementation Plan

### Step 1: Add MuxNotification Variants

**File:** `mux/src/lib.rs`

Add to the `MuxNotification` enum:

```rust
WebOpen {
    pane_id: PaneId,
    url: String,
},
WebClosed {
    pane_id: PaneId,
},
```

**File:** `wezterm-mux-server-impl/src/sessionhandler.rs`

In the `Pdu::WebOpen` handler, after the success response, publish the notification:

```rust
Pdu::WebOpen(WebOpen { pane_id, url }) => {
    // ... existing validation ...

    // Notify GUI to open browser
    mux.notify(MuxNotification::WebOpen {
        pane_id,
        url: url.clone(),
    });

    Pdu::WebOpenResponse(WebOpenResponse {
        message: format!("Opening {} in pane {}", url, pane_id),
    })
}
```

### Step 2: Create CEF Browser Module

**File:** `wezterm-gui/src/cef_browser/mod.rs` (new file, ~400 lines)

Port selectively from the reverted code, removing the old `set_message_pump_hook` approach since we now use callback-based integration.

Key components:

```rust
pub struct BrowserState {
    pub browser: cef::browser::Browser,
    pub pane_id: PaneId,
    pub url: String,
    pub texture_holder: Arc<Mutex<Option<TextureHolder>>>,
    render_handler: RenderHandler,
}

impl BrowserState {
    pub fn new(pane_id: PaneId, url: &str, width: u32, height: u32) -> Result<Self> {
        // Create browser with OSR settings
        // Set up render handler for texture capture
    }

    pub fn send_key_event(&self, event: &KeyEvent) {
        // Convert wezterm key event to CEF key event
        // Send to browser
    }

    pub fn resize(&self, width: u32, height: u32) {
        // Notify browser of size change
    }

    pub fn get_texture(&self) -> Option<wgpu::Texture> {
        // Return current texture for rendering
    }
}
```

**Key differences from reverted code:**
- Remove `init_cef()` - already handled in `main.rs`
- Remove `set_message_pump_hook` - we use `BrowserProcessHandler` callback
- Keep `BrowserState`, `RenderHandler`, key event conversion
- Keep IOSurface texture import logic

### Step 3: Add Browser State to TermWindow

**File:** `wezterm-gui/src/termwindow/mod.rs`

Add fields to `TermWindow`:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
browser_states: RefCell<HashMap<PaneId, cef::BrowserState>>,

#[cfg(all(target_os = "macos", feature = "cef"))]
browser_render_targets: RefCell<Vec<BrowserRenderTarget>>,
```

Add struct for render targets:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
struct BrowserRenderTarget {
    pane_id: PaneId,
    texture: wgpu::Texture,
    bind_group: wgpu::BindGroup,
}
```

Initialize in `TermWindow::new()`:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
browser_states: RefCell::new(HashMap::new()),
#[cfg(all(target_os = "macos", feature = "cef"))]
browser_render_targets: RefCell::new(Vec::new()),
```

### Step 4: Handle WebOpen Notification

**File:** `wezterm-gui/src/termwindow/mod.rs`

Add method to handle the notification:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn handle_web_open(&self, pane_id: PaneId, url: String) {
    log::info!("[CEF] Opening browser for pane {} with URL: {}", pane_id, url);

    // Get pane dimensions
    let dims = self.get_pane_dimensions(pane_id);

    // Create browser state
    match cef::BrowserState::new(pane_id, &url, dims.width, dims.height) {
        Ok(state) => {
            self.browser_states.borrow_mut().insert(pane_id, state);
            self.window.as_ref().map(|w| w.invalidate());
        }
        Err(e) => {
            log::error!("[CEF] Failed to create browser: {}", e);
        }
    }
}
```

Wire up in the notification dispatch (in `TermWindow::dispatch_mux_notification`):

```rust
MuxNotification::WebOpen { pane_id, url } => {
    #[cfg(all(target_os = "macos", feature = "cef"))]
    self.handle_web_open(pane_id, url);
}
```

### Step 5: Add Keyboard Handling

**File:** `wezterm-gui/src/termwindow/keyevent.rs`

In the key event handler, before normal terminal processing:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
{
    let pane_id = self.get_active_pane_id();
    if let Some(pane_id) = pane_id {
        let has_browser = self.browser_states.borrow().contains_key(&pane_id);
        if has_browser {
            // Check for Ctrl+C to close browser
            let is_ctrl_c = window_key.key_is_down
                && window_key.modifiers.contains(Modifiers::CTRL)
                && matches!(window_key.key, KeyCode::Char('c') | KeyCode::Char('C'));

            if is_ctrl_c {
                self.close_browser_for_pane(pane_id);
                return;
            }

            // Forward other keys to browser
            if let Some(browser) = self.browser_states.borrow().get(&pane_id) {
                browser.send_key_event(&window_key);
            }
            return;
        }
    }
}
```

Add close method:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn close_browser_for_pane(&self, pane_id: PaneId) {
    log::info!("[CEF] Closing browser for pane {}", pane_id);

    // Remove browser state (this will close the browser)
    self.browser_states.borrow_mut().remove(&pane_id);

    // Remove render target
    self.browser_render_targets.borrow_mut().retain(|t| t.pane_id != pane_id);

    // Notify mux that browser closed
    let mux = Mux::get();
    mux.notify(MuxNotification::WebClosed { pane_id });

    // Redraw
    self.window.as_ref().map(|w| w.invalidate());
}
```

### Step 6: Add Browser Rendering

**File:** `wezterm-gui/src/termwindow/render/pane.rs`

In the pane rendering code, check for browser overlay:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
{
    if let Some(browser) = self.browser_states.borrow().get(&pane_id) {
        if let Some(texture) = browser.get_texture() {
            self.paint_browser_overlay(pane_id, &texture, &pos);
            return Ok((
                Some(cursor_name),
                None, // composited
            ));
        }
    }
}
```

**File:** `wezterm-gui/src/termwindow/render/draw.rs`

Add browser texture rendering method:

```rust
#[cfg(all(target_os = "macos", feature = "cef"))]
fn paint_browser_overlay(
    &self,
    pane_id: PaneId,
    texture: &wgpu::Texture,
    pos: &PositionedPane,
) {
    // Create texture view and bind group if needed
    // Render browser texture over pane area
    // Similar to existing texture compositing code
}
```

### Step 7: Wire Up WebClosed Dispatch

**File:** `wezterm-gui/src/termwindow/mod.rs`

In `dispatch_mux_notification`:

```rust
MuxNotification::WebClosed { pane_id } => {
    // Already handled locally, but log for debugging
    log::debug!("[CEF] WebClosed notification for pane {}", pane_id);
}
```

## File Summary

| File | Changes |
|------|---------|
| `mux/src/lib.rs` | Add `WebOpen`, `WebClosed` to `MuxNotification` |
| `wezterm-mux-server-impl/src/sessionhandler.rs` | Publish `WebOpen` notification |
| `wezterm-gui/src/cef_browser/mod.rs` | New file - browser state, render handler |
| `wezterm-gui/src/cef_integration.rs` | Existing - no changes needed |
| `wezterm-gui/src/termwindow/mod.rs` | Add browser state fields, handle methods |
| `wezterm-gui/src/termwindow/keyevent.rs` | Add Ctrl+C handling, key routing |
| `wezterm-gui/src/termwindow/render/pane.rs` | Check for browser overlay |
| `wezterm-gui/src/termwindow/render/draw.rs` | Add browser texture rendering |
| `wezterm-gui/src/main.rs` | Add `mod cef_browser;` |
| `wezterm-gui/src/termwindow/render/pane.rs` | Add `paint_browser_overlay()` method |

## Estimated Lines of Code

- Step 1 (MuxNotification): ~20 lines
- Step 2 (CEF module): ~400 lines
- Step 3 (TermWindow state): ~30 lines
- Step 4 (WebOpen handler): ~50 lines
- Step 5 (Keyboard handling): ~60 lines
- Step 6 (Browser rendering): ~150 lines
- Step 7 (WebClosed dispatch): ~10 lines

**Total:** ~720 lines

## Testing Plan

1. **Build:** `./scripts/build-debug.sh`
2. **Run:** `./target/debug/WezTerm.app/Contents/MacOS/wezterm-gui`
3. **Open terminal:** Let WezTerm start normally
4. **Test command:** In another terminal: `./target/debug/WezTerm.app/Contents/MacOS/wezterm cli web-open https://example.com`
5. **Verify:** Browser should appear in the pane
6. **Close:** Press Ctrl+C in the WezTerm window
7. **Verify:** Terminal should return to normal

## Dependencies

- MVP1 must be complete (CEF initialization working)
- `cef-rs` must be available at `../../cef-rs`
- `cef-osr.app` must be built (provides CEF framework)

## Notes

- The old reverted code used `set_message_pump_hook` for CEF polling. We now use the callback-based `BrowserProcessHandler::on_schedule_message_pump_work` from `cef_integration.rs`. The new CEF module should NOT include any message pump setup.

- Mouse events are deferred to a future MVP. This MVP focuses on keyboard-only interaction.

- Browser resize on pane resize is included but may need refinement.

## References

- [cef-wezterm.md](cef-wezterm.md) - CEF + WezTerm integration overview
- [cef-mvp.md](cef-mvp.md) - MVP1 execution log
- Reverted commit: `700fc3822 Revert flawed CEF MVP integration`
