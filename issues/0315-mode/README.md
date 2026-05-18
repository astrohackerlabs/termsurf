+++
status = "closed"
opened = "2026-01-31"
closed = "2026-01-31"
+++

# Issue 315: Control Mode

## Goal

Implement mode switching for webview panes. When focused on a webview pane, the
user is in one of two modes:

- **Browse mode** — Browser is focused, receiving input (future)
- **Control mode** — Control panel is focused, browser dimmed

This issue implements the mode state machine and key interception. Actual input
forwarding to the browser is deferred to a future issue.

## Background

### Current Behavior

When a webview pane is visible:

- The control panel displays the URL
- The webview renders below it
- All keyboard input goes to the terminal underneath
- Ctrl+C sends SIGINT to the terminal process

### Desired Behavior

When a webview pane is visible:

- No keyboard input reaches the terminal underneath
- Keys are intercepted by the control panel, webview, or WezTerm GUI
- Mode determines which component receives input

## Product Requirements

### Mode State Machine

```
                ┌─────────────┐
                │             │
     ┌──────────│ Browse Mode │◄─────────┐
     │          │  (default)  │          │
     │          │             │          │
     │          └─────────────┘          │
     │                                   │
Ctrl+C                               Enter
     │                                   │
     │          ┌─────────────┐          │
     │          │             │          │
     └─────────►│Control Mode │──────────┘
                │             │
                └──────┬──────┘
                       │
                  Ctrl+C
                       │
                       ▼
                ┌─────────────┐
                │             │
                │ Exit Browser│
                │             │
                └─────────────┘
```

### Browse Mode

**Default mode** when entering a webview pane.

| Input               | Action                                     |
| ------------------- | ------------------------------------------ |
| Ctrl+C              | Switch to Control mode                     |
| WezTerm keybindings | Execute (e.g., Ctrl+Shift+T for new tab)   |
| All other keys      | No-op for now (future: forward to browser) |
| Mouse input         | No-op for now (future: forward to browser) |

**Visual appearance:**

- Control panel shows URL
- Webview renders normally (full brightness)

### Control Mode

**Activated** by pressing Ctrl+C in Browse mode.

| Input               | Action                                           |
| ------------------- | ------------------------------------------------ |
| Enter               | Switch to Browse mode                            |
| Ctrl+C              | Exit browser (close webview, return to terminal) |
| WezTerm keybindings | Execute (e.g., Ctrl+Shift+T for new tab)         |
| All other keys      | No-op                                            |
| Mouse input         | No-op                                            |

**Visual appearance:**

- Control panel shows instructions: "Enter to browse. Ctrl+C to exit."
- Webview renders dimmed (reduced opacity or overlay)

### Key Interception

**Critical requirement:** While a webview is visible, NO keys should reach the
terminal process underneath. This prevents:

- Accidental input to shell while browsing
- Ctrl+C sending SIGINT to terminal process
- Any keystrokes appearing in terminal

Keys are handled in this priority order:

1. **WezTerm keybindings** — Ctrl+Shift+T, Ctrl+Tab, etc.
2. **Mode-specific actions** — Ctrl+C, Enter (as defined above)
3. **Browser input** — Future: forwarded to CEF in Browse mode
4. **Dropped** — All remaining keys are discarded

### Exit Behavior

When exiting the browser (Ctrl+C in Control mode):

1. Close the webview overlay
2. Remove the control panel
3. Return focus to the terminal pane underneath
4. Terminal resumes normal operation

This matches the current Ctrl+C behavior, but only triggers from Control mode.

## Technical Approach

### Mode State Storage

Store the current mode per webview pane:

```rust
pub enum WebviewMode {
    Browse,
    Control,
}

// In WebviewOverlay or separate state
pub struct WebviewModeState {
    mode: WebviewMode,
}
```

### Key Event Interception

Intercept key events before they reach the terminal:

1. Check if the focused pane has a webview overlay
2. If yes, route the key through the mode state machine
3. Only WezTerm keybindings and mode actions are processed
4. All other keys are consumed (not forwarded)

Location in WezTerm: `termwindow/mod.rs` key event handling.

### Visual Feedback

**Control mode text** (matching ts2):

```
"Enter to browse. Ctrl+C to exit."
```

**Dimming in Control mode:**

- Option A: Reduce webview opacity
- Option B: Overlay semi-transparent layer
- Option C: Apply CSS filter via CEF (future)

For Phase 1, Option B is simplest — render a semi-transparent overlay on top of
the webview texture.

## Implementation Plan

### Step 1: Add Mode State

Add `WebviewMode` enum and storage to track current mode per pane.

### Step 2: Intercept Key Events

Modify key handling to check for webview overlay and route through mode logic.

### Step 3: Implement Mode Transitions

- Ctrl+C in Browse mode → Control mode
- Enter in Control mode → Browse mode
- Ctrl+C in Control mode → Exit browser

### Step 4: Update Control Panel Text

Show different text based on mode:

- Browse mode: URL
- Control mode: "Enter to browse. Ctrl+C to exit."

### Step 5: Add Visual Dimming

Render semi-transparent overlay on webview in Control mode.

## Files to Modify

| File                                               | Changes                         |
| -------------------------------------------------- | ------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add WebviewMode enum and state  |
| `ts3/wezterm-gui/src/termwindow/mod.rs`            | Key event interception          |
| `ts3/wezterm-gui/src/termwindow/render/pane.rs`    | Mode-aware control panel text   |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs`    | Dimming overlay in Control mode |

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open webview (starts in Browse mode)
web google.com

# 2. Verify Browse mode
# - Control panel shows URL
# - Type random keys — nothing appears in terminal
# - Webview is full brightness

# 3. Press Ctrl+C — switch to Control mode
# - Control panel shows "Enter to browse. Ctrl+C to exit."
# - Webview is dimmed
# - Type random keys — nothing appears in terminal

# 4. Press Enter — switch back to Browse mode
# - Control panel shows URL again
# - Webview is full brightness

# 5. Press Ctrl+C twice — exit browser
# - First Ctrl+C: Control mode
# - Second Ctrl+C: Browser closes, terminal visible

# 6. Verify WezTerm keybindings work in both modes
# - Ctrl+Shift+T opens new tab
# - Ctrl+Tab switches tabs
```

## Success Criteria

1. [x] `WebviewMode` enum exists (Browse, Control)
2. [x] Mode state stored per webview pane
3. [x] Keys intercepted when webview is visible
4. [x] No keys reach terminal underneath
5. [x] Ctrl+C in Browse mode → Control mode
6. [x] Enter in Control mode → Browse mode
7. [x] Ctrl+C in Control mode → Exit browser
8. [x] Control panel text changes based on mode
9. [x] Visual dimming in Control mode
10. [x] WezTerm keybindings work in both modes

## References

- `docs/issues/0314-control.md` — Control panel implementation
- `ts2/wezterm-gui/src/cef_browser/mod.rs` — ts2 BrowserMode enum
- `ts1/src/apprt/surface.zig` — ts1 mode implementation

---

## Experiments

### Experiment 1: Mode State and Key Interception

**Goal:** Add mode state (Browse/Control) and intercept key events so that no
keys reach the terminal underneath while a webview is visible.

**Background:**

ts2 handles key interception in `keyevent.rs:660-845`. The key insight is that
the browser mode check happens early in `key_event_impl`, before keys are
processed for terminal input.

```rust
// ts2 approach (simplified)
fn key_event_impl(&mut self, window_key: KeyEvent, ...) {
    // 1. Check for browser mode FIRST
    if let Some(mode) = get_browser_mode(pane_id) {
        match mode {
            Browse => {
                if is_ctrl_c { set_mode(Control); return; }
                // Forward to CEF (future)
                return; // Consume all keys
            }
            Control => {
                if is_enter { set_mode(Browse); return; }
                if is_ctrl_c { close_browser(); return; }
                // Fall through to keybindings only
            }
        }
    }

    // 2. Process WezTerm keybindings
    if self.process_key(...) { return; }

    // 3. Send to terminal (we must prevent this for webview panes!)
    pane.writer().write_all(...)
}
```

The difference from ts2: we want NO keys to reach the terminal in either mode.
In Control mode, ts2 lets non-keybinding keys fall through. We will consume
them.

#### Approach

**Part A: Add WebviewMode enum and state**

Add mode enum to `webview_socket.rs`:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WebviewMode {
    Browse,  // Browser focused, receiving input (future)
    Control, // Control panel focused, browser dimmed
}

impl Default for WebviewMode {
    fn default() -> Self {
        WebviewMode::Browse
    }
}
```

Add mode field to `WebviewOverlay`:

```rust
pub struct WebviewOverlay {
    pub session_id: String,
    pub tab_id: TabId,
    pub mode: WebviewMode,
}
```

**Part B: Key interception in key_event_impl**

Add early check in `keyevent.rs` `key_event_impl`:

```rust
pub fn key_event_impl(&mut self, window_key: KeyEvent, context: &dyn WindowOps) {
    let pane = match self.get_active_pane_or_overlay() {
        Some(pane) => pane,
        None => return,
    };

    // Check for webview overlay and handle mode-specific input
    #[cfg(target_os = "macos")]
    if let Some(handled) = self.handle_webview_key_event(&pane, &window_key) {
        if handled {
            return; // Key was consumed by webview handling
        }
        // If not handled, continue to keybindings but NOT terminal
    }

    // ... rest of existing key handling ...
}
```

**Part C: Implement handle_webview_key_event**

New helper function:

```rust
#[cfg(target_os = "macos")]
fn handle_webview_key_event(
    &mut self,
    pane: &Arc<dyn Pane>,
    window_key: &KeyEvent,
) -> Option<bool> {
    use crate::termwindow::webview_socket::{get_server, WebviewMode};

    let pane_id = pane.pane_id();

    // Check if this pane has a webview overlay
    let server = get_server()?;
    let state = server.state();
    let mut overlays = state.write().unwrap();
    let overlay = overlays.overlays.get_mut(&pane_id)?;

    // Check for Ctrl+C
    let is_ctrl_c = window_key.key_is_down
        && window_key.modifiers.contains(::window::Modifiers::CTRL)
        && matches!(
            &window_key.key,
            ::window::KeyCode::Char('c') | ::window::KeyCode::Char('C')
        );

    // Check for Enter
    let is_enter = window_key.key_is_down
        && window_key.modifiers.is_empty()
        && matches!(&window_key.key, ::window::KeyCode::Char('\r'));

    match overlay.mode {
        WebviewMode::Browse => {
            if is_ctrl_c {
                log::info!("[Webview] Ctrl+C in Browse mode → Control mode");
                overlay.mode = WebviewMode::Control;
                // Trigger redraw for visual feedback
                drop(overlays);
                if let Some(ref w) = self.window {
                    w.invalidate();
                }
                return Some(true);
            }
            // In Browse mode, consume all keys (future: forward to CEF)
            if window_key.key_is_down {
                log::debug!("[Webview] Consuming key in Browse mode");
            }
            Some(true)
        }
        WebviewMode::Control => {
            if is_enter {
                log::info!("[Webview] Enter in Control mode → Browse mode");
                overlay.mode = WebviewMode::Browse;
                drop(overlays);
                if let Some(ref w) = self.window {
                    w.invalidate();
                }
                return Some(true);
            }
            if is_ctrl_c {
                log::info!("[Webview] Ctrl+C in Control mode → Exit browser");
                drop(overlays);
                self.close_webview_for_pane(pane_id);
                return Some(true);
            }
            // In Control mode, return None to allow keybindings
            // but we'll block terminal input separately
            None
        }
    }
}
```

**Part D: Block terminal input in Control mode**

After `process_key` in `key_event_impl`, check if we should block terminal
input:

```rust
// After process_key returns false (no keybinding matched)
#[cfg(target_os = "macos")]
{
    // If this pane has a webview, consume the key instead of sending to terminal
    if self.pane_has_webview_overlay(pane.pane_id()) {
        log::debug!("[Webview] Consuming unbound key in Control mode");
        return;
    }
}

// ... existing terminal input code ...
```

**Part E: Add close_webview_for_pane helper**

```rust
#[cfg(target_os = "macos")]
fn close_webview_for_pane(&mut self, pane_id: PaneId) {
    use crate::termwindow::webview_socket::get_server;

    if let Some(server) = get_server() {
        let state = server.state();
        let mut overlays = state.write().unwrap();
        overlays.overlays.remove(&pane_id);
    }

    // Also clean up XPC resources
    if let Some(xpc_manager) = crate::termwindow::webview_xpc::get_xpc_manager() {
        xpc_manager.remove_surface(pane_id);
        xpc_manager.remove_connection(pane_id);
        xpc_manager.remove_invalidate_callback(pane_id);
    }

    // Trigger redraw
    if let Some(ref w) = self.window {
        w.invalidate();
    }
}
```

#### Files to Modify

| File                                               | Changes                                  |
| -------------------------------------------------- | ---------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/webview_socket.rs` | Add `WebviewMode` enum, add `mode` field |
| `ts3/wezterm-gui/src/termwindow/keyevent.rs`       | Add key interception, mode transitions   |
| `ts3/wezterm-gui/src/termwindow/mod.rs`            | Add helper methods if needed             |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open webview (starts in Browse mode)
web google.com

# 2. Type random keys
# Expected: Nothing appears in terminal

# 3. Press Ctrl+C
# Expected: Log shows "Ctrl+C in Browse mode → Control mode"

# 4. Type random keys
# Expected: Nothing appears in terminal

# 5. Press Enter
# Expected: Log shows "Enter in Control mode → Browse mode"

# 6. Press Ctrl+C twice
# First: → Control mode
# Second: Browser closes, terminal visible

# 7. Verify WezTerm keybindings work
# - In Browse mode: Ctrl+Shift+T should open new tab
# - In Control mode: Ctrl+Shift+T should open new tab

# Check logs
grep "\[Webview\]" /tmp/termsurf-gui.log
```

#### Success Criteria

1. [x] `WebviewMode` enum exists with Browse and Control variants
2. [x] `WebviewOverlay` has `mode` field, defaults to Browse
3. [x] Key events intercepted for webview panes
4. [x] No keys reach terminal in Browse mode
5. [x] No keys reach terminal in Control mode
6. [x] Ctrl+C in Browse mode → Control mode
7. [x] Enter in Control mode → Browse mode
8. [x] Ctrl+C in Control mode → closes webview
9. [x] WezTerm keybindings work in both modes

#### Result

**Success.** Mode switching and key interception working correctly.

#### Conclusion

**What was accomplished:**

Key interception and mode switching now work for webview panes:

1. **WebviewMode enum** added with Browse (default) and Control variants
2. **Key interception** happens early in `key_event_impl`, before terminal input
3. **Browse mode**: All keys consumed (Ctrl+C switches to Control mode)
4. **Control mode**: Enter → Browse, Ctrl+C → exit, other keys → keybindings
   only
5. **Terminal protected**: No keys reach the terminal in either mode

**Implementation details:**

- `handle_webview_key_event()` routes keys based on current mode
- Early return in `key_event_impl` for Browse mode (consume all keys)
- Post-keybinding block for Control mode (allow keybindings, block terminal)
- `close_webview_for_pane()` cleans up overlay state and XPC resources

**Files modified:**

- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` — WebviewMode enum, mode
  field
- `ts3/wezterm-gui/src/termwindow/keyevent.rs` — Key interception and mode
  handling

**Next steps:**

- Experiment 2: Update control panel text based on mode
- Experiment 3: Add visual dimming in Control mode

### Experiment 2: Mode-Aware Control Panel Text

**Goal:** Update the control panel to show different text based on the current
mode:

- Browse mode: Show the URL (current behavior)
- Control mode: Show "Enter to browse. Ctrl+C to exit."

**Background:**

The `paint_webview_control_bars` function in `pane.rs` already iterates over
webview overlays and has access to `overlay.mode`. Currently it always displays
the URL:

```rust
// Line 842-845: Get URL
let url = match xpc_manager.get_received_surface(*pane_id) {
    Some(surface) => surface.url.clone(),
    None => continue,
};

// Line 904: Create text element with URL
let element = Element::new(&font, ElementContent::Text(url))
```

#### Approach

After getting the URL, check the overlay mode and choose the appropriate text:

```rust
// Get URL from received surface
let url = match xpc_manager.get_received_surface(*pane_id) {
    Some(surface) => surface.url.clone(),
    None => continue,
};

// Choose text based on mode
use crate::termwindow::webview_socket::WebviewMode;
let display_text = match overlay.mode {
    WebviewMode::Browse => url,
    WebviewMode::Control => "Enter to browse. Ctrl+C to exit.".to_string(),
};
```

Then use `display_text` instead of `url` when creating the element:

```rust
let element = Element::new(&font, ElementContent::Text(display_text))
```

#### Files to Modify

| File                                            | Changes                                       |
| ----------------------------------------------- | --------------------------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/pane.rs` | Mode-aware text in paint_webview_control_bars |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open webview (starts in Browse mode)
web google.com

# 2. Verify Browse mode
# - Control panel shows URL (e.g., "https://www.google.com/")

# 3. Press Ctrl+C — switch to Control mode
# - Control panel shows "Enter to browse. Ctrl+C to exit."

# 4. Press Enter — switch back to Browse mode
# - Control panel shows URL again

# 5. Press Ctrl+C, then Ctrl+C again — exit browser
# - Browser closes, terminal visible
```

#### Success Criteria

1. [x] Browse mode shows URL in control panel
2. [x] Control mode shows "Enter to browse. Ctrl+C to exit."
3. [x] Text updates immediately on mode switch

#### Result

**Success.** Control panel text now changes based on mode.

#### Conclusion

**What was accomplished:**

The control panel now displays context-appropriate text based on the current
webview mode:

- **Browse mode**: Shows the URL (e.g., "https://www.google.com/")
- **Control mode**: Shows "Enter to browse. Ctrl+C to exit."

**Implementation details:**

- Added mode check in `paint_webview_control_bars()` after fetching the URL
- Uses `overlay.mode` to determine which text to display
- Text updates immediately when mode transitions (invalidate already triggered)

**Files modified:**

- `ts3/wezterm-gui/src/termwindow/render/pane.rs` — Mode-aware display text

**Next steps:**

- Experiment 3: Add visual dimming in Control mode

### Experiment 3: Visual Dimming in Control Mode

**Goal:** Dim the webview when in Control mode to provide visual feedback that
the browser is not active.

**Background:**

The issue requirements specify Option B: "Overlay semi-transparent layer." After
analyzing the render pipeline, a cleaner approach is to pass a dimming factor to
the webview shader via a uniform buffer. This avoids adding a second render pass
and keeps the code simple.

Current render flow:

1. Terminal/UI layers rendered via `call_draw_webgpu`
2. Webview texture rendered via `render_webview_overlays_webgpu`
3. Webview shader samples texture and outputs color directly

The webview shader is simple (lines 41-47 of `webview_shader.wgsl`):

```wgsl
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(webview_texture, webview_sampler, input.tex_coord);
    return color;
}
```

#### Approach

Add a uniform buffer to pass a `dim_factor` value to the shader. In Control
mode, multiply the RGB values by `(1.0 - dim_factor)` to darken the output.

**Part A: Update webview shader**

Add uniform struct and modify fragment shader:

```wgsl
// Add after texture/sampler bindings
struct DimUniforms {
    dim_factor: f32,
}
@group(1) @binding(0) var<uniform> dim_uniforms: DimUniforms;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(webview_texture, webview_sampler, input.tex_coord);
    // Apply dimming: reduce brightness in Control mode
    let brightness = 1.0 - dim_uniforms.dim_factor;
    return vec4<f32>(color.rgb * brightness, color.a);
}
```

**Part B: Add bind group layout in webgpu.rs**

Create a new bind group layout for the dim uniforms:

```rust
let webview_dim_bind_group_layout =
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Webview Dim Bind Group Layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: Some(std::num::NonZeroU64::new(4).unwrap()), // f32
            },
            count: None,
        }],
    });
```

Update the pipeline layout to include both bind groups:

```rust
let webview_pipeline_layout =
    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Webview Pipeline Layout"),
        bind_group_layouts: &[&webview_bind_group_layout, &webview_dim_bind_group_layout],
        push_constant_ranges: &[],
    });
```

Store the layout on WebGpuState:

```rust
pub webview_dim_bind_group_layout: wgpu::BindGroupLayout,
```

**Part C: Pass dim_factor in render_webview_overlays_webgpu**

After getting the overlay, check the mode and create a uniform buffer:

```rust
// Get dim factor based on mode
use crate::termwindow::webview_socket::WebviewMode;
let dim_factor: f32 = match overlay.mode {
    WebviewMode::Browse => 0.0,
    WebviewMode::Control => 0.5, // 50% dimming
};

// Create uniform buffer for dim factor
let dim_buffer = webgpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
    label: Some("Webview Dim Buffer"),
    contents: bytemuck::cast_slice(&[dim_factor]),
    usage: wgpu::BufferUsages::UNIFORM,
});

let dim_bind_group = webgpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
    label: Some("Webview Dim Bind Group"),
    layout: &webgpu.webview_dim_bind_group_layout,
    entries: &[wgpu::BindGroupEntry {
        binding: 0,
        resource: dim_buffer.as_entire_binding(),
    }],
});

// In the render pass, set both bind groups
render_pass.set_bind_group(0, &bind_group, &[]);
render_pass.set_bind_group(1, &dim_bind_group, &[]);
```

#### Files to Modify

| File                                            | Changes                                    |
| ----------------------------------------------- | ------------------------------------------ |
| `ts3/wezterm-gui/src/webview_shader.wgsl`       | Add DimUniforms struct, apply dimming      |
| `ts3/wezterm-gui/src/termwindow/webgpu.rs`      | Add dim bind group layout, update pipeline |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Pass dim_factor based on mode              |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# 1. Open webview (starts in Browse mode)
web google.com

# 2. Verify Browse mode
# - Webview renders at full brightness
# - Control panel shows URL

# 3. Press Ctrl+C — switch to Control mode
# - Webview is visibly dimmed (50% darker)
# - Control panel shows "Enter to browse. Ctrl+C to exit."

# 4. Press Enter — switch back to Browse mode
# - Webview returns to full brightness
# - Control panel shows URL again

# 5. Toggle back and forth
# - Dimming should be immediate on each transition
```

#### Success Criteria

1. [x] Webview renders at full brightness in Browse mode
2. [x] Webview is visibly dimmed in Control mode
3. [x] Dimming transitions immediately on mode switch
4. [x] No visual artifacts or flickering

#### Result

**Success.** Visual dimming now provides clear feedback when in Control mode.

#### Conclusion

**What was accomplished:**

The webview now dims to 50% brightness when entering Control mode, providing
clear visual feedback that the browser is not receiving input:

- **Browse mode**: Full brightness (dim_factor = 0.0)
- **Control mode**: 50% brightness (dim_factor = 0.5)

**Implementation details:**

- Added `DimUniforms` struct to webview shader with `dim_factor` uniform
- Fragment shader multiplies RGB by `(1.0 - dim_factor)` for dimming effect
- Created new bind group layout for dim uniforms in WebGpuState
- Updated pipeline layout to include both texture and dim bind groups
- Render code checks `overlay.mode` and creates appropriate dim buffer

**Files modified:**

- `ts3/wezterm-gui/src/webview_shader.wgsl` — Added dim uniform and dimming
  logic
- `ts3/wezterm-gui/src/termwindow/webgpu.rs` — Added dim bind group layout
- `ts3/wezterm-gui/src/termwindow/render/draw.rs` — Pass dim_factor based on
  mode

**Issue 315 complete.** All three experiments succeeded:

1. **Experiment 1**: Mode state and key interception
2. **Experiment 2**: Mode-aware control panel text
3. **Experiment 3**: Visual dimming in Control mode
