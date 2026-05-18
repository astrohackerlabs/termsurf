+++
status = "closed"
opened = "2026-01-31"
closed = "2026-01-31"
+++

# Issue 316: Unified Dimming with WezTerm Config

## Goal

Use WezTerm's existing `inactive_pane_hsb` configuration to control dimming for
webview panes. This ensures consistent visual treatment across:

1. **Inactive terminal panes** — Already handled by WezTerm
2. **Webview in Control mode** — Browser dimmed when not receiving input
3. **Control panel in Browse mode** — Panel dimmed when browser is active
4. **Entire webview pane when inactive** — Both panel and browser dimmed

## Background

### Current Behavior

Issue 315 implemented dimming for Control mode with a hardcoded 50% brightness
reduction in the webview shader. This works but has two problems:

1. **Inconsistent with WezTerm config** — Users can't customize the dimming
2. **Control panel not dimmed** — In Browse mode, the control panel should be
   dimmed to indicate it's not active

### WezTerm Config

WezTerm already has a config for inactive pane appearance:

```lua
inactive_pane_hsb = {
  hue = 1.0,        -- Keep the hue the same
  saturation = 0.7, -- Reduce saturation for inactive panes
  brightness = 0.5, -- Make inactive panes slightly dimmer
}
```

This config uses HSB (Hue, Saturation, Brightness) adjustments rather than
simple alpha/brightness multiplication. We should use the exact same values.

### Desired Behavior

| Pane State | Mode    | Control Panel | Webview      |
| ---------- | ------- | ------------- | ------------ |
| Active     | Browse  | Dimmed (HSB)  | Normal       |
| Active     | Control | Normal        | Dimmed (HSB) |
| Inactive   | Browse  | Dimmed (HSB)  | Dimmed (HSB) |
| Inactive   | Control | Dimmed (HSB)  | Dimmed (HSB) |

When dimmed, both components use `inactive_pane_hsb` values from the config.

## Product Requirements

### HSB Dimming

Apply the same HSB transformation that WezTerm uses for inactive terminal panes:

- **Hue**: Multiplier for color hue (1.0 = unchanged)
- **Saturation**: Multiplier for color saturation (0.7 = 70% saturation)
- **Brightness**: Multiplier for brightness (0.5 = 50% brightness)

### Component Dimming Rules

**Control Panel:**

- Dimmed in Browse mode (browser is active, panel is not)
- Dimmed when pane is inactive
- Normal in Control mode of active pane

**Webview:**

- Dimmed in Control mode (panel is active, browser is not)
- Dimmed when pane is inactive
- Normal in Browse mode of active pane

### Config Integration

Read `inactive_pane_hsb` from the WezTerm config and pass the values to:

- The webview shader (for webview dimming)
- The control panel rendering (for panel dimming)

## Technical Considerations

### Webview Shader

Currently the shader has a simple `dim_factor` uniform. This needs to be
extended to support HSB transformation:

```wgsl
struct DimUniforms {
    hue: f32,
    saturation: f32,
    brightness: f32,
}
```

The fragment shader would need to:

1. Convert RGB to HSB
2. Multiply by the uniform values
3. Convert back to RGB

### Control Panel

The control panel is rendered using WezTerm's UI element system. We need to
apply the same HSB transformation to the text and background colors when the
panel should be dimmed.

### Pane Active State

Need to determine if the current pane is active. WezTerm already tracks this for
terminal panes — we need to access the same state for webview panes.

## Implementation Plan

### Step 1: Research

Understand how WezTerm applies `inactive_pane_hsb` to terminal panes. Find:

- Where the config is read
- How HSB transformation is applied
- How pane active state is determined

### Step 2: Update Webview Shader

Replace simple `dim_factor` with HSB uniforms. Implement RGB↔HSB conversion.

### Step 3: Update Control Panel

Apply HSB dimming to control panel colors based on mode and pane state.

### Step 4: Wire Up Config

Pass `inactive_pane_hsb` values from config to both shader and panel rendering.

## Files to Investigate

| File                                       | Purpose            |
| ------------------------------------------ | ------------------ |
| `ts3/config/src/lib.rs`                    | Config definitions |
| `ts3/wezterm-gui/src/termwindow/render/`   | Pane rendering     |
| `ts3/wezterm-gui/src/termwindow/webgpu.rs` | Shader setup       |
| `ts3/wezterm-gui/src/webview_shader.wgsl`  | Webview shader     |

## Success Criteria

1. [x] Webview uses `inactive_pane_hsb` config for dimming
2. [x] Control panel uses `inactive_pane_hsb` config for dimming
3. [x] Browse mode: control panel dimmed, webview normal
4. [x] Control mode: webview dimmed, control panel normal
5. [x] Inactive pane: both panel and webview dimmed
6. [x] Visual consistency with inactive terminal panes

## References

- `docs/issues/0315-mode.md` — Current dimming implementation
- WezTerm docs on `inactive_pane_hsb`

---

## Experiments

### Experiment 1: Unified HSB Dimming

**Goal:** Use WezTerm's existing `inactive_pane_hsb` config and HSB
infrastructure for all webview-related dimming.

**Background:**

WezTerm's existing infrastructure:

1. **`shader.wgsl`** has `rgb2hsv()`, `hsv2rgb()`, `apply_hsv()` functions
2. Each quad vertex has an `hsv: vec3<f32>` attribute
3. `quad.set_hsv(Some(config.inactive_pane_hsb))` applies dimming
4. The fragment shader applies `apply_hsv(color, hsv)` at the end

The control panel uses `render_element()` which calls `quad.set_hsv(None)`. The
webview uses a separate shader with a simple `dim_factor` uniform.

#### Approach

**Part A: Webview Shader — Add HSB Support**

Copy the HSV functions from `shader.wgsl` to `webview_shader.wgsl`:

```wgsl
fn rgb2hsv(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(0.0, -1.0 / 3.0, 2.0 / 3.0, -1.0);
    let p = mix(vec4<f32>(c.bg, K.wz), vec4<f32>(c.gb, K.xy), step(c.b, c.g));
    let q = mix(vec4<f32>(p.xyw, c.r), vec4<f32>(c.r, p.yzx), step(p.x, c.r));
    let d = q.x - min(q.w, q.y);
    let e = 1.0e-10;
    return vec3<f32>(abs(q.z + (q.w - q.y) / (6.0 * d + e)), d / (q.x + e), q.x);
}

fn hsv2rgb(c: vec3<f32>) -> vec3<f32> {
    let K = vec4<f32>(1.0, 2.0 / 3.0, 1.0 / 3.0, 3.0);
    let p = abs(fract(c.xxx + K.xyz) * 6.0 - K.www);
    return c.z * mix(K.xxx, clamp(p - K.xxx, vec3(0.0), vec3(1.0)), c.y);
}

fn apply_hsv(c: vec4<f32>, transform: vec3<f32>) -> vec4<f32> {
    let hsv = rgb2hsv(c.rgb) * transform;
    return vec4<f32>(hsv2rgb(hsv).rgb, c.a);
}
```

Change `DimUniforms` from single `dim_factor` to HSB values:

```wgsl
struct DimUniforms {
    hue: f32,
    saturation: f32,
    brightness: f32,
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let color = textureSample(webview_texture, webview_sampler, input.tex_coord);
    let hsv_transform = vec3<f32>(
        dim_uniforms.hue,
        dim_uniforms.saturation,
        dim_uniforms.brightness
    );
    return apply_hsv(color, hsv_transform);
}
```

**Part B: Update Webview Render Code**

In `draw.rs`, pass HSB values from config instead of hardcoded dim_factor:

```rust
// Determine if webview should be dimmed
// Dimmed when: Control mode OR pane is inactive
let webview_should_dim = match overlay.mode {
    WebviewMode::Browse => !pane_is_active,  // Only dim if pane inactive
    WebviewMode::Control => true,            // Always dim in Control mode
};

let (hue, saturation, brightness) = if webview_should_dim {
    let hsb = self.config.inactive_pane_hsb;
    (hsb.hue, hsb.saturation, hsb.brightness)
} else {
    (1.0, 1.0, 1.0)  // No transformation
};
```

Update uniform buffer to hold three f32 values instead of one.

**Part C: Control Panel HSB Dimming**

Add optional HSV parameter to `render_element`:

```rust
pub fn render_element_with_hsv(
    &self,
    element: &ComputedElement,
    gl_state: &RenderState,
    inherited_colors: Option<&ElementColors>,
    hsv: Option<HsbTransform>,
) -> anyhow::Result<()>
```

Inside, pass `hsv` to all `quad.set_hsv()` calls instead of `None`.

In `paint_webview_control_bars`, determine if panel should be dimmed:

```rust
// Determine if control panel should be dimmed
// Dimmed when: Browse mode OR pane is inactive
let panel_should_dim = match overlay.mode {
    WebviewMode::Browse => true,             // Always dim in Browse mode
    WebviewMode::Control => !pane_is_active, // Only dim if pane inactive
};

let hsv = if panel_should_dim {
    Some(self.config.inactive_pane_hsb)
} else {
    None
};

self.render_element_with_hsv(&computed, gl_state, None, hsv)?;
```

**Part D: Get Pane Active State**

In both render functions, determine if the pane is active:

```rust
// Find if this pane is active
let pane_is_active = positioned_panes
    .iter()
    .find(|p| p.pane.pane_id() == *pane_id)
    .map(|p| p.is_active)
    .unwrap_or(false);
```

#### Files to Modify

| File                                            | Changes                                       |
| ----------------------------------------------- | --------------------------------------------- |
| `ts3/wezterm-gui/src/webview_shader.wgsl`       | Add HSV functions, change DimUniforms to HSB  |
| `ts3/wezterm-gui/src/termwindow/webgpu.rs`      | Update uniform buffer size (3 × f32)          |
| `ts3/wezterm-gui/src/termwindow/render/draw.rs` | Pass HSB values, check pane active state      |
| `ts3/wezterm-gui/src/termwindow/box_model.rs`   | Add `render_element_with_hsv` function        |
| `ts3/wezterm-gui/src/termwindow/render/pane.rs` | Use HSV for control panel, check active state |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Active pane, Browse mode
web google.com
# Expected: Control panel dimmed, webview normal

# Test 2: Active pane, Control mode (Ctrl+C)
# Expected: Control panel normal, webview dimmed

# Test 3: Inactive pane
# Split pane (Ctrl+Shift+%), click other pane
# Expected: Both control panel AND webview dimmed

# Test 4: Visual consistency
# Compare dimming to inactive terminal pane
# Expected: Same HSB transformation applied
```

#### Success Criteria

1. [x] Webview uses `inactive_pane_hsb` config values
2. [ ] Control panel uses `inactive_pane_hsb` config values
3. [ ] Browse mode (active): panel dimmed, webview normal
4. [ ] Control mode (active): panel normal, webview dimmed
5. [ ] Inactive pane: both panel and webview dimmed
6. [ ] Dimming matches inactive terminal pane appearance

#### Result: FAILED

**What worked:**

- Webview dimming works correctly using HSB from config
- Webview dims when switching to Control mode
- Webview dims when pane becomes inactive

**What failed:**

- Control panel never dims in any state

#### Conclusion

The control panel has **two separate rendering paths**:

1. **Background** — Rendered in `paint_webview_overlay_background()`
   (pane.rs:718-786) during the main pane painting pass. Calls
   `filled_rectangle()` at line 783 but **discards the returned quad** without
   calling `set_hsv()`.

2. **Text** — Rendered in `paint_webview_control_bars()` (pane.rs:791+) after
   layers are dropped. Uses `render_element_with_hsv()` which **does** apply HSV
   correctly.

The text receives the HSV transformation, but the bright background behind it
overwhelms the visual effect. The background quad is never given an HSV
transform.

**Hypothesis for fix:**

Modify `paint_webview_overlay_background()` to:

1. Get the overlay mode from `get_server()` (like `paint_webview_control_bars`
   does)
2. Determine if background should be dimmed (same logic as text)
3. Capture the quad returned by `filled_rectangle()` and call `set_hsv()` on it

---

### Experiment 2: Apply HSV to Control Panel Background

**Goal:** Fix experiment 1 by applying HSV to the control panel background quad.

**Problem:** `paint_webview_overlay_background()` renders the background but
discards the quad without calling `set_hsv()`.

#### Approach

Modify `paint_webview_overlay_background()` to:

1. Get the webview overlay from `get_server()` to access the mode
2. Calculate whether the background should be dimmed (same logic as text)
3. Capture the quad from `filled_rectangle()` and call `set_hsv()` on it

**Current code (pane.rs:779-783):**

```rust
// Render control bar background
let palette = self.palette().clone();
let bg_color = palette.background.to_linear();
let control_bar_rect = euclid::rect(x, y, width, control_bar_height);
self.filled_rectangle(layers, 0, control_bar_rect, bg_color)?;
```

**New code:**

```rust
// Render control bar background with HSV dimming
let palette = self.palette().clone();
let bg_color = palette.background.to_linear();
let control_bar_rect = euclid::rect(x, y, width, control_bar_height);
let mut quad = self.filled_rectangle(layers, 0, control_bar_rect, bg_color)?;

// Get overlay mode to determine if background should be dimmed
use crate::termwindow::webview_socket::{get_server, WebviewMode};
let pane_id = pos.pane.pane_id();
let panel_hsv = if let Some(server) = get_server() {
    let state = server.state();
    let overlays = state.read().unwrap();
    if let Some(overlay) = overlays.overlays.get(&pane_id) {
        // Dimmed when: Browse mode OR pane is inactive
        let should_dim = match overlay.mode {
            WebviewMode::Browse => true,
            WebviewMode::Control => !pos.is_active,
        };
        if should_dim {
            Some(self.config.inactive_pane_hsb)
        } else {
            None
        }
    } else {
        None
    }
} else {
    None
};

quad.set_hsv(panel_hsv);
```

#### Files to Modify

| File                                            | Changes                    |
| ----------------------------------------------- | -------------------------- |
| `ts3/wezterm-gui/src/termwindow/render/pane.rs` | Add HSV to background quad |

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open

# Test 1: Active pane, Browse mode
web google.com
# Expected: Control panel background AND text dimmed, webview normal

# Test 2: Active pane, Control mode (Ctrl+C)
# Expected: Control panel normal, webview dimmed

# Test 3: Inactive pane
# Split pane, click other pane
# Expected: Both control panel AND webview dimmed
```

#### Success Criteria

1. [x] Control panel background uses HSV dimming
2. [x] Browse mode (active): entire panel dimmed (background + text)
3. [x] Control mode (active): entire panel normal
4. [x] Inactive pane: entire panel dimmed
5. [x] Visual match between background and text dimming

#### Result: SUCCESS

The control panel background now dims correctly alongside the text.
