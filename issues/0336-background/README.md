+++
status = "closed"
opened = "2026-02-02"
closed = "2026-02-02"
+++

# Issue 336: Default White Background for Webviews

## Problem

Some web pages render with a transparent background, causing the WezTerm
terminal background color to show through. This looks broken because web pages
assume they're rendered on a white canvas (the browser standard).

### Example

A page with no explicit `background-color` CSS:

- **Expected**: White background (like Chrome/Safari)
- **Actual**: Terminal's dark background bleeds through

## Product Requirements

### User Story

As a user browsing the web in TermSurf, I expect pages to look the same as they
do in Chrome or Safari, with a white default background.

### Acceptance Criteria

1. Webviews render with a white background by default
2. Pages that explicitly set a background color (including dark mode sites)
   display correctly
3. Transparent elements blend over white, not the terminal background

### Non-Requirements (Out of Scope)

- Configurable default background color (future enhancement)
- Dark mode default option (future enhancement)
- Per-profile background settings (future enhancement)

## Technical Context

CEF likely has a setting for the default background color during browser
creation or in the render handler. The fix should set this to white (`#FFFFFF`
or `rgba(255, 255, 255, 1)`).

## Files Involved

- `ts3/termsurf-profile/src/main.rs` — Browser creation and render handler

---

## Experiments

### Experiment 1: Set BrowserSettings.background_color to opaque white

**Status: Success**

CEF's `BrowserSettings` has a `background_color` field (type `cef_color_t` =
`u32`) in ARGB format. Setting it to an opaque value disables transparent
painting and provides a default background.

#### Analysis

From `cef-rs/cef/src/window_info.rs` (line 47-48):

> "Transparent painting is enabled by default but can be disabled by setting
> CefBrowserSettings.background_color to an opaque value."

The current browser creation in `termsurf-profile/src/main.rs` (line 1150):

```rust
let browser_settings = BrowserSettings {
    windowless_frame_rate: 60,
    ..Default::default()
};
```

The `background_color` defaults to 0, which means transparent.

#### Color Format

CEF uses ARGB format: `0xAARRGGBB`

- Opaque white: `0xFFFFFFFF` (A=255, R=255, G=255, B=255)

#### Implementation

Update `BrowserSettings` in `termsurf-profile/src/main.rs` (line 1150):

```rust
let browser_settings = BrowserSettings {
    windowless_frame_rate: 60,
    background_color: 0xFFFFFFFF, // Opaque white (issue 336)
    ..Default::default()
};
```

#### Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web example.com    # Simple page, should have white background
web google.com     # Should still look normal
```

Test pages:
- A page with no background CSS → should be white
- A page with explicit dark background → should show dark
- A page with transparent elements → should blend over white

---

## Conclusion

Webviews now render with an opaque white background by default, matching the
behavior of Chrome, Safari, and other browsers. Pages that don't explicitly set
a background color will display correctly instead of showing the terminal's
dark background bleeding through.

### Implementation Summary

| Component         | Change                                            |
| ----------------- | ------------------------------------------------- |
| BrowserSettings   | Set `background_color: 0xFFFFFFFF` (opaque white) |

### Technical Note

CEF uses ARGB color format. Setting `background_color` to an opaque value
(alpha = 0xFF) disables transparent painting mode, which was causing the
terminal background to show through.
