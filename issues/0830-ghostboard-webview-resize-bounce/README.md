+++
status = "open"
opened = "2026-06-19"
+++

# Issue 830: Ghostboard Webview Resize Bounce

## Goal

Eliminate the visible webview resize bounce when a Ghostboard pane containing a
browser overlay is resized.

When the pane changes size, the webview should resize directly to the final pane
geometry instead of briefly shrinking to a fallback/default size and then
expanding back to the correct size.

## Background

On an installed production build, resizing a pane with an active webview causes
the browser content to momentarily resize to a small default-looking size before
resizing back to fill the pane. The double resize is visually glitchy.

Code analysis found a likely cause in Ghostboard's current resize path:

- `handleSetOverlay` updates an existing pane and immediately sends a browser
  `Resize` through `snapshotResize`.
- `snapshotResize` computes browser pixels as `pane.width * fallback_cell_width`
  and `pane.height * fallback_cell_height`, where the fallback constants are
  `10` and `20`.
- AppKit later computes the actual CALayerHost frame, reports presented pixels
  through `termsurf_overlay_presented_pixels`, and Zig sends a corrective resize
  to the real AppKit pixel size.

That produces the exact visible sequence: fallback-size browser resize followed
by real-size browser resize.

Ghostboard Legacy did not use this fallback resize path for active overlays. It
computed resize pixels from the active surface's real cell metrics:

- `ghostboard-legacy/src/apprt/xpc.zig` called `surface.core().getCellSize()`
  when processing `set_overlay`.
- It sent a browser resize using `width * cell.width` and
  `height * cell.height`.
- The CALayerHost frame lived in the renderer path and was updated from the same
  renderer cell metrics.

Current Ghostboard no longer has direct surface access in the Zig socket hub
when `SetOverlay` arrives, because presentation crosses into Swift through
`termsurf_present_overlay`. The final pixel truth is therefore the AppKit
presented-pixel callback, not the hard-coded fallback.

## Analysis

The fix should remove the fallback `10x20` browser resize from the active
existing-pane resize path. The browser should receive a resize only when
Ghostboard knows the real pixel size for the presented overlay.

The likely implementation direction is:

- keep `presentOverlay` updating the Swift CALayer geometry immediately;
- keep AppKit reporting actual presented pixels via
  `termsurf_overlay_presented_pixels`;
- send the browser `Resize` from `overlayPresentedPixels` when the reported
  pixel size differs from the last resize sent;
- do not send a `Resize` from `handleSetOverlay` if the only available size is
  `fallback_cell_width`/`fallback_cell_height`;
- preserve initial tab creation behavior so a browser tab still receives usable
  initial dimensions.

This issue needs manual visual confirmation because the primary defect is a
short-lived visual bounce. Automated checks can prove that the fallback resize
message is gone and that the final AppKit pixel resize is still delivered, but
the final acceptance requires a human watching the installed or development app.

## Acceptance Criteria

- Resizing a browser pane no longer visibly shrinks the webview to a
  default/small size before returning to the pane size.
- Browser resize messages after pane geometry changes use AppKit-presented pixel
  sizes, not hard-coded `10x20` fallback-derived sizes.
- Existing browser open, window resize, split resize, and devtools overlay
  geometry behavior still work.
- The fix does not require Chromium changes.
- The result is manually verified by resizing an active browser pane in
  Ghostboard.

## Experiments

- [Experiment 1: Remove fallback resize from active overlay updates](01-remove-fallback-resize-from-active-overlay-updates.md)
  — **Designed**
