+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# Issue 333: Display profile name in control panel

## Goal

Show the current browser profile name in the webview control panel,
right-aligned.

## Requirements

1. **Position**: Right-aligned in the control panel
2. **Overflow handling**: When the URL is long enough to overlap with the
   profile name:
   - Profile name renders above the tail end of the URL (z-order)
   - URL truncates with ellipsis (`...`) where it would overlap

## Visual Examples

### Normal case (short URL)

```
┌─────────────────────────────────────────────────────┐
│ https://google.com                          default │
└─────────────────────────────────────────────────────┘
```

### Long URL (truncated with ellipsis)

```
┌─────────────────────────────────────────────────────┐
│ https://example.com/very/long/path/to/...   default │
└─────────────────────────────────────────────────────┘
```

## Files Involved

- `ts3/wezterm-gui/src/termwindow/webview_socket.rs` - WebviewOverlay struct and
  creation
- `ts3/wezterm-gui/src/termwindow/render/pane.rs` - Control panel rendering
  (lines 850-995)

---

## Experiment 1: Display profile name right-aligned

**Status: Success**

Add the profile name to the control panel, right-aligned, with URL truncation
when they would overlap.

### Step 1: Add profile field to WebviewOverlay

In `webview_socket.rs`, add profile to the struct (line 338):

```rust
pub struct WebviewOverlay {
    pub session_id: String,
    pub tab_id: TabId,
    pub mode: WebviewMode,
    pub profile: String,  // NEW
}
```

Update overlay creation (line 537 and 592) to include the profile:

```rust
let overlay = WebviewOverlay {
    session_id: session_id.clone(),
    tab_id,
    mode: WebviewMode::default(),
    profile: profile.to_string(),  // NEW
};
```

### Step 2: Calculate available space and truncate URL

In `render/pane.rs`, after getting the display_text (line 884), calculate space
for profile name:

```rust
let profile_name = &overlay.profile;
let profile_padding = cell_width; // Space between URL and profile

// Measure profile name width
let profile_width = profile_name.len() as f32 * cell_width;
let profile_reserved = profile_width + profile_padding;

// Available width for URL (total width minus right padding minus profile)
let url_max_width = width - half_cell_width - profile_reserved;

// Truncate URL if needed
let display_text = match overlay.mode {
    WebviewMode::Browse => {
        truncate_with_ellipsis(&url, url_max_width, cell_width)
    }
    WebviewMode::Control => "Enter to browse. Ctrl+C to exit.".to_string(),
};
```

Add helper function:

```rust
fn truncate_with_ellipsis(text: &str, max_width: f32, cell_width: f32) -> String {
    let max_chars = (max_width / cell_width) as usize;
    if text.len() <= max_chars {
        text.to_string()
    } else if max_chars > 3 {
        format!("{}...", &text[..max_chars - 3])
    } else {
        "...".to_string()
    }
}
```

### Step 3: Render profile name element

After rendering the URL element (line 991), add a second element for the profile
name:

```rust
// Render profile name (right-aligned)
let profile_element = Element::new(&font, ElementContent::Text(profile_name.clone()))
    .colors(ElementColors {
        border: BorderColor::default(),
        bg: palette.background.to_linear().into(),
        text: palette.foreground.to_linear().into(),
    })
    .padding(BoxDimension {
        left: Dimension::Pixels(0.),
        top: Dimension::Pixels(half_cell_height),
        right: Dimension::Pixels(half_cell_width),
        bottom: Dimension::Pixels(half_cell_height),
    });

let mut profile_computed = self.compute_element(
    &LayoutContext { /* same as above */ },
    &profile_element,
)?;

// Position at right edge
let profile_x = x + width - profile_width - half_cell_width;
profile_computed.translate(euclid::vec2(profile_x, y));

self.render_element_with_hsv(&profile_computed, gl_state, None, panel_hsv)?;
```

### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com                    # Short URL - profile visible on right
web google.com/very/long/path...  # Long URL - truncated with ellipsis
```

---

## Conclusion

The control panel now displays the browser profile name right-aligned, making it
easy to identify which profile is active for each webview.

### Implementation Summary

| Component | Change |
|-----------|--------|
| `WebviewOverlay` struct | Added `profile: String` field |
| Overlay creation | Profile name captured from `web` command (defaults to "default") |
| URL rendering | Truncated with `...` when space is limited |
| Profile rendering | Separate element positioned at right edge |

### Key Design Decisions

1. **Two separate elements** - URL and profile are rendered as independent
   elements rather than a single formatted string. This allows precise
   positioning and independent styling if needed later.

2. **Character-based truncation** - URL truncation is calculated based on
   available character width, accounting for the profile name, padding, and
   margins.

3. **Profile on top (z-order)** - The profile element is rendered after the URL
   element, so if they somehow overlap, the profile name remains visible.

This is new functionality not present in ts2, which only displayed the URL.
