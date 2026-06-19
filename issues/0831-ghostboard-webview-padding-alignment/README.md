+++
status = "open"
opened = "2026-06-19"
+++

# Issue 831: Ghostboard Webview Padding Alignment

## Goal

Fix Ghostboard webview overlay positioning so the browser content aligns
symmetrically inside the TUI viewport border.

The webview should use the same terminal grid coordinate system as the rendered
TUI. It should not sit visibly closer to the left border than the right border.

## Background

A manual screenshot taken after Issue 830 showed a browser webview inside the
`web` TUI viewport with asymmetric horizontal spacing. The webview starts close
to the left border, while a larger gap remains before the right border. The
symptom is visible with the split pane border enabled and a browser page loaded.

Ghostboard Legacy did not show this alignment problem.

The visible viewport is produced by `webtui`:

- `webtui` draws a ratatui `Block` around the browser viewport.
- It sends `viewport_block.inner(viewport_area)` to Ghostboard as the overlay
  rectangle.
- Therefore `col=1` and `row=1` are expected for a normal bordered viewport;
  those coordinates describe the inside of the TUI border, not the outer border.

## Analysis

Code analysis points to a coordinate conversion regression introduced by moving
browser overlay presentation out of the renderer and into Swift/AppKit.

Current Ghostboard positions the overlay in Swift:

- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`
  computes the overlay frame as:
  - `x = col * cellWidth`
  - `y = row * cellHeight`
  - `width = width * cellWidth`
  - `height = height * cellHeight`
- The Swift path knows `cellSize`, but it does not add the renderer's terminal
  grid padding or balanced leftover space.

Ghostboard Legacy positioned the overlay in the renderer:

- `ghostboard-legacy/src/Surface.zig` stored the overlay grid rectangle on the
  renderer.
- `ghostboard-legacy/src/renderer/generic.zig` passed `self.size.padding.left`
  and `self.size.padding.top` to the Metal backend.
- `ghostboard-legacy/src/renderer/Metal.zig` converted grid coordinates to the
  CALayerHost frame with:
  - `x = grid_col * cell_width / scale + padding_left / scale`
  - `y = grid_row * cell_height / scale + padding_top / scale`
  - `width = grid_width * cell_width / scale`
  - `height = grid_height * cell_height / scale`

The renderer coordinate model also confirms that grid coordinates are not raw
surface coordinates. `ghostboard/src/renderer/size.zig` converts grid
coordinates to surface coordinates by adding `size.padding.left` and
`size.padding.top`.

That difference explains the screenshot: current Swift anchors the webview to
the raw cell rectangle, while the rendered terminal grid may be shifted by
balanced padding or leftover surface space. The result is a browser overlay that
does not line up with the same grid/border geometry the user sees.

## Proposed Direction

Restore padding-aware overlay positioning in current Ghostboard.

Likely approaches:

1. Expose the current surface padding and grid metrics to Swift, then make
   `presentTermSurfOverlay` use the same grid-to-surface conversion as the
   renderer.
2. Have the Zig/AppKit bridge pass an AppKit-ready overlay frame instead of only
   grid coordinates.
3. Move overlay frame ownership back toward the renderer if that is cleaner and
   does not conflict with the current AppKit CALayerHost presentation path.

The preferred fix should keep the current AppKit-presented-pixel resize flow
from Issue 830 and only change overlay placement math.

## Acceptance Criteria

- The webview aligns symmetrically inside the `web` TUI viewport border.
- The overlay still starts inside the TUI viewport border, not on top of it.
- Browser input hit testing still matches the visible webview after the
  alignment fix.
- Window resize and split resize still keep the webview aligned.
- Existing geometry tests continue to pass or are updated to assert the
  padding-aware frame.
- Ghostboard Legacy's padding-aware coordinate behavior is used as the
  reference.

## Experiments

- [Experiment 1: Use renderer padding for AppKit overlay frames](01-use-renderer-padding-for-appkit-overlay-frames.md)
  — **Pass**
