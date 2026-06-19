# Experiment 1: Use Renderer Padding for AppKit Overlay Frames

## Description

Make current Ghostboard's AppKit overlay frame math match Ghostboard Legacy's
renderer-owned CALayerHost frame math.

Legacy placed the browser host layer in surface coordinates by adding renderer
grid padding to the grid rectangle:

- `x = grid_col * cell_width / scale + padding_left / scale`
- `y = grid_row * cell_height / scale + padding_top / scale`
- `width = grid_width * cell_width / scale`
- `height = grid_height * cell_height / scale`

Current Ghostboard computes the frame in Swift using only `col * cellWidth` and
`row * cellHeight`, which ignores the renderer's current padding. This
experiment will expose the exact renderer padding through the existing
`ghostty_surface_size_s` bridge and use that padding in
`presentTermSurfOverlay`.

## Changes

Planned code changes:

1. Extend the existing surface-size bridge.

   - Update `ghostboard/src/apprt/embedded.zig`'s exported `SurfaceSize` extern
     struct to include:
     - `padding_top_px`
     - `padding_bottom_px`
     - `padding_right_px`
     - `padding_left_px`
   - Populate those fields from `surface.core_surface.size.padding`.
   - Update the matching C declaration in `ghostboard/include/ghostty.h`.

2. Update AppKit overlay frame math.

   - In
     `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift`,
     derive logical-point padding from `surfaceSize`:
     - `paddingLeft = padding_left_px / backingScale`
     - `paddingTop = padding_top_px / backingScale`
   - Compute the overlay frame as:
     - `x = paddingLeft + col * cellWidth`
     - `y = paddingTop + row * cellHeight`
     - `width = width * cellWidth`
     - `height = height * cellHeight`
   - Keep `host.frame = CGRect(origin: .zero, size: frame.size)`.
   - Keep the Issue 830 AppKit-presented-pixel resize flow unchanged.
   - Keep hit testing based on `termsurfOverlayFrame`, so input follows the
     corrected visible frame.

3. Add focused geometry trace data only if needed.

   - Prefer using existing `overlay_frame`, `bounds`, `cell`, and `grid` log
     fields.
   - If the existing logs are insufficient to prove padding use, add the minimum
     trace fields needed to record the padding used for the frame.

Non-goals:

- Do not change `webtui` viewport rectangle generation. It correctly sends the
  inner rect of the ratatui viewport border.
- Do not change Roamium or Chromium.
- Do not change browser resize message routing beyond any pixel-size effect
  caused by the corrected AppKit frame.
- Do not change devtools behavior beyond sharing the same corrected generic
  overlay frame function if it already uses `presentTermSurfOverlay`.

## Verification

Static verification:

```bash
zig fmt ghostboard/src/apprt/embedded.zig
git diff --check
```

Build verification:

```bash
./scripts/build.sh ghostboard
```

Automated geometry verification:

```bash
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh split-down
```

Log verification:

- The geometry run must prove the scenario has non-zero renderer padding, either
  through an explicit padding trace field or through an equivalent calculation
  from logged surface size, grid, cell size, and backing scale.
- With non-zero renderer padding proven, AppKit `overlay_frame.minX` should
  equal:
  - `padding_left_px / backing_scale + col * cell_width`
- AppKit `overlay_frame.minY` should equal:
  - `padding_top_px / backing_scale + row * cell_height`
- AppKit `host_frame` should still start at `{0, 0}` with the overlay frame's
  size.
- AppKit `presented_pixels` should still match the visible overlay frame size
  multiplied by backing scale.
- Roamium should still receive `ts_set_view_size` for the AppKit-presented pixel
  dimensions.
- Browser hit-test logs should still report webview-relative points inside the
  corrected `overlay_frame`.

Manual verification:

1. Build Ghostboard with `./scripts/build.sh ghostboard`.
2. Run Ghostboard from the repo.
3. Inside Ghostboard, run the debug `web` binary with the debug Roamium binary:

   ```bash
   /Users/astrohacker/dev/termsurf/target/debug/web \
     --browser /Users/astrohacker/dev/termsurf/chromium/src/out/Default/roamium \
     https://astrohacker.com/
   ```

4. Verify that the webview aligns symmetrically inside the viewport border.
5. Resize the window and split panes, then verify alignment remains stable.

Pass criteria:

- The automated geometry scenarios pass.
- Logs prove the AppKit overlay frame uses renderer padding.
- The browser remains interactive after the corrected frame is applied.
- Manual verification confirms the visible left/right viewport spacing is
  symmetric.

If automated geometry passes but manual verification still shows asymmetry, the
result should be **Partial** and the next experiment should capture a screenshot
plus AppKit geometry trace from that exact manual reproduction.

## Design Review

Fresh-context adversarial design review returned **APPROVED** with no Required
findings.

Optional finding:

- The original log verification said "in a geometry run with non-zero renderer
  padding" but did not explicitly require proving that the run actually had
  non-zero padding. That could make the padding assertion vacuous if defaults or
  harness configuration change.

Fix applied:

- Updated log verification to require proving non-zero renderer padding, either
  through an explicit padding trace field or through an equivalent calculation
  from logged surface size, grid, cell size, and backing scale.

## Result

**Result:** Pass

Implementation changed the current AppKit overlay path to use the same
padding-aware grid-to-surface conversion that Ghostboard Legacy used in the
renderer path.

Code changes:

- `ghostboard/src/apprt/embedded.zig` now exports renderer padding through the
  existing `SurfaceSize` bridge.
- `ghostboard/include/ghostty.h` now exposes the matching
  `ghostty_surface_size_s` padding fields to Swift.
- `ghostboard/macos/Sources/Ghostty/Surface View/SurfaceView_AppKit.swift` now
  adds `padding_left_px / backing_scale` and `padding_top_px / backing_scale` to
  the AppKit overlay frame origin, while preserving the Issue 830
  AppKit-presented-pixel resize path.
- The AppKit geometry trace now logs renderer padding in pixels and logical
  points so future geometry runs can prove whether the padding is actually in
  use.

Verification run:

```bash
zig fmt ghostboard/src/apprt/embedded.zig
git diff --check
./scripts/build.sh ghostboard
scripts/ghostboard-geometry-matrix.sh window-resize
scripts/ghostboard-geometry-matrix.sh split-right
scripts/ghostboard-geometry-matrix.sh split-down
```

`./scripts/build.sh ghostboard` passed. The build still emitted existing ImGui
symbol warnings from `ghostty-internal.a(ext.o)`, but the macOS app build
completed successfully.

Geometry scenario logs:

- `logs/ghostboard-geometry-window-resize-app-20260619-183521.log`
- `logs/ghostboard-geometry-split-right-app-20260619-183538.log`
- `logs/ghostboard-geometry-split-down-app-20260619-183623.log`

All three geometry matrix scenarios passed.

The padding audit proved non-zero renderer padding and exact frame alignment:

```text
window:      grid=139x45+1+1 frame={{10,19},{1112,765}} padding=4px/2pt expected={{10,19},{1112,765}} PASS
window grow: grid=179x55+1+1 frame={{10,19},{1432,935}} padding=4px/2pt expected={{10,19},{1432,935}} PASS
window shrink: grid=149x49+1+1 frame={{10,19},{1192,833}} padding=4px/2pt expected={{10,19},{1192,833}} PASS
split right: grid=73x49+1+1 frame={{10,19},{584,833}} padding=4px/2pt expected={{10,19},{584,833}} PASS
split down:  grid=149x21+1+1 frame={{10,19},{1192,357}} padding=4px/2pt expected={{10,19},{1192,357}} PASS
```

Manual visual inspection of
`logs/ghostboard-geometry-window-resize-screenshot-20260619-183521.png`
confirmed the webview starts inside the viewport border and is no longer
anchored to the raw view edge.

## Conclusion

The asymmetry came from Swift converting terminal grid coordinates directly to
AppKit points without adding the renderer's terminal-grid padding. Current
Ghostboard now follows Ghostboard Legacy's coordinate model: the webview frame
origin is renderer padding plus the grid cell offset, while the size remains the
grid cell extent.

## Completion Review

Fresh-context adversarial completion review returned **APPROVED** with no
findings.
