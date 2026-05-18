+++
status = "closed"
opened = "2026-02-01"
closed = "2026-02-01"
+++

# 327: Scroll Speed

Increase webview scroll speed for faster navigation.

## Status

**Resolved.** Scroll multiplier increased from 2x to 5x.

## Problem

Webview scrolling felt too slow. Users had to scroll excessively to navigate
long pages. The default `* 2` multiplier from Issue 321 provided smooth
scrolling but required too much trackpad movement.

## Background

Issue 321 established scroll support with a `* 2` multiplier for trackpad
input. This was chosen because:

- `* 120` (CEF default for line-based scroll) was blocky
- `* 1` (raw values) was too slow
- `* 2` (cef-rs OSR example default) was smooth

While `* 2` felt smooth, it was slower than native browser scrolling in Chrome
or Safari.

## Solution

Increased the scroll multiplier from `* 2` to `* 5` for both vertical and
horizontal scrolling. This provides faster navigation while maintaining smooth
feel.

## Changes

**File:** `ts3/wezterm-gui/src/termwindow/mouseevent.rs`

| Handler | Before | After |
|---------|--------|-------|
| `VertWheel` (line 1318) | `* 2` | `* 5` |
| `HorzWheel` (line 1328) | `* 2` | `* 5` |

```rust
// Vertical scroll
let delta_y = (*amount as i32) * 5;

// Horizontal scroll
let delta_x = (*amount as i32) * 5;
```

## Verification

```bash
cd ts3 && ./scripts/build-debug.sh --open
web google.com
# Two-finger scroll gesture
# Expected: Faster scrolling, ~2.5x previous speed
```

## References

- Issue 321 — Original scroll implementation with `* 2` multiplier
- `ts3/wezterm-gui/src/termwindow/mouseevent.rs` — Scroll event handling
