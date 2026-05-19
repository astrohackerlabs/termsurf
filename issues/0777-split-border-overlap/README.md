+++
status = "open"
opened = "2026-04-11"
+++

# Issue 777: Split border overlaps pane content and blocks mouse resize

## Goal

The `split_border_width` border must not overlap pane content or block
mouse-driven pane resizing.

## Background

Wezboard has a config option `split_border_width = 4` that draws a border around
each terminal pane. This border has two problems:

### 1. Border overlaps pane content

The border is drawn on top of the pane's content area rather than outside it.
With `split_border_width = 4`, the outermost 4 pixels of terminal content are
hidden behind the border. The pane needs padding or margin equal to the border
width so content is inset and fully visible.

### 2. Border covers the mouse resize handle

WezTerm uses a thin invisible hit region between panes for mouse-driven resizing
(click and drag to resize splits). The border is drawn on top of this region,
visually covering it and — more critically — intercepting or blocking mouse
events. With the border enabled, it is impossible to resize panes with the
mouse.

## Analysis

This issue is a regression from the behavior that worked in the archived Ghostty
fork. Ghostboard's final border behavior is documented in Issue 672: the border
was an overlay, but the rendered surface was reduced by the border width and
offset inward. That made the border behave like CSS `box-sizing: border-box`:
the pane owns a full outer rectangle, the border occupies the edge of that
rectangle, and terminal content renders inside the remaining inner rectangle.

The intended behavior is:

1. A single tab with one pane has no split border.
2. Creating a split makes borders appear on both panes.
3. The focused pane uses `focused_split_border_color`.
4. Unfocused panes use `unfocused_split_border_color`.
5. The border occupies real pane space. Content is inset by exactly
   `split_border_width` logical pixels on all four sides.
6. The split divider remains visible and mouse-draggable, even when pane borders
   replace the old thin divider drawing.

### What Wezboard currently does

Wezboard has the first half of this behavior:

- `paint.rs` iterates visible panes, calls `paint_pane`, then calls
  `paint_pane_border`.
- `paint_pane_border` draws four layer-2 rectangles around each pane when there
  is more than one pane and `split_border_width > 0`.
- `paint_pane_border` selects the focused/unfocused color from
  `focused_split_border_color` and `unfocused_split_border_color`, falling back
  to the palette split color.
- `paint.rs` skips the old `paint_split` divider when `split_border_width > 0`,
  so the old thin divider is not drawn on top of the new pane borders.

That is why the visual state is partly correct: no border for a single pane,
border appears after splitting, and focused/unfocused colors can work.

### Why the content inset is wrong

The border is currently just paint. It does not participate in pane layout or
line rendering.

In `paint_pane`, the `num_panes` parameter is named `_num_panes` and is not
used. The line-rendering origin is still computed as:

```rust
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32);
```

The line-rendering top position is likewise based on the pane's unmodified
`top_pixel_y`, and `render_screen_line` still receives:

```rust
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32,
```

None of these values account for `split_border_width`. As a result, terminal
text and background fills still begin at the original pane edge and span the
original pane width. The border is then drawn later on layer 2, directly over
the outer pixels of that content.

There is also a unit bug: the border width is currently treated as physical
framebuffer pixels. Today, `split_border_width` flows through
`Dimension::evaluate_as_pixels`, where `Dimension::Pixels(n)` returns
`n.floor()` with no DPI scaling. Users configure border widths in UI terms, so
`split_border_width = 4` should mean 4 logical pixels, not 4 physical pixels. On
a 2x Retina display, that should draw and reserve 8 physical pixels. On a 1x
display, it should draw and reserve 4 physical pixels. The conversion from
logical pixels to physical pixels must happen once in split-border-specific
geometry, using the window's current scale/DPI, before painting, content inset,
and hit-region geometry are computed.

The historical failed attempts in Issue 723 explain why this must be a per-pane
content inset, not a global resize:

- Reducing only `pixel_width` does not move the left/top origin.
- Shifting only selected "interior" edges misses the edges where the border is
  still drawn.
- Subtracting border pixels from the global window size in `resize.rs` changes
  the terminal cell count but does not allocate padding inside each pane.

The successful Ghostboard-style model is simpler: every bordered pane gets
uniform inner padding on all four sides, equal to the physical-pixel value
computed from `split_border_width` logical pixels for the current display.

### Why mouse resize is wrong

Mouse resizing depends on UI hit regions, not just pixels on screen.

`paint_split` does two things:

1. It draws the old split divider line.
2. It pushes a `UIItemType::Split(split.clone())` into `self.ui_items`.

`mouseevent.rs` later uses that UI item to set the resize cursor, start split
dragging, and call `tab.resize_split_by(...)`.

When `split_border_width > 0`, `paint.rs` skips `paint_split`. That removes the
old visual divider, which is correct, but it also removes the only code path
that registers the split resize hit region. `paint_pane_border` draws border
rectangles but does not register a `UIItemType::Split`, so the mouse has no
split target to hover, click, or drag.

### Fix direction

The fix should restore the Ghostboard model in Wezboard:

1. **Compute a per-pane border inset.** In `paint_pane`, resolve
   `split_border_width` into physical pixels only when `num_panes > 1` and the
   pane is not zoomed. Otherwise the inset is zero. Do not use the current
   `Dimension::Pixels` behavior directly for this, because it returns raw
   physical pixels. The configured value should be interpreted as logical
   pixels; convert it to physical pixels using the current window scale/DPI
   before using it for geometry.
2. **Apply that inset to content on all four sides.** Add the inset to
   `left_pixel_x`, add it to the top coordinate used by `LineRender`, and reduce
   the width passed to `render_screen_line` by `2 * inset`. This makes text and
   line backgrounds render inside the border.
3. **Keep the pane background consistent.** Inset the pane background fill by
   the same amount, or explicitly decide that background may extend under the
   border while text and line fills do not. The Ghostboard behavior was cleaner:
   content, progress overlays, and surface rendering all lived inside the
   border.
4. **Register split resize hit regions even when borders replace dividers.**
   Either keep a non-drawing `paint_split` path that only pushes
   `UIItemType::Split`, or move split hit-region registration into a separate
   helper called regardless of whether the old divider is drawn. The hit region
   should use the same logical-to-physical border width conversion as painting,
   cover the visible border/divider area, and remain large enough to drag
   comfortably with the mouse.
5. **Do not solve this in `resize.rs`.** The terminal cell count may stay the
   same; the issue is pixel placement inside each pane. Global resize math does
   not create per-pane padding.

The implementation should prove both requirements together: border padding must
move content inward by exactly `split_border_width` logical pixels, converted to
the correct physical pixel count for the current display, and the split
divider/hit region must remain visible and clickable for mouse resizing.

## Experiments

### Experiment 1: Restore Border-Box Pane Geometry

#### Description

Fix split pane borders by restoring the Ghostboard border-box model in Wezboard.
When multiple panes are visible, `split_border_width` should be interpreted as
logical pixels, converted to physical pixels for the current display, and used
as uniform inner padding for each bordered pane. The same physical border width
must also define the split resize hit region so mouse dragging still works when
the old thin divider is hidden.

This experiment should define explicit outer and inner pane geometry. The outer
pane rect is the full area assigned to the pane. The border occupies the edge of
that outer rect. The inner content rect is the outer rect inset by the converted
border width on all four sides. Rendering, browser overlay positioning, and
mouse-to-cell mapping must all use the same inner content rect.

#### Changes

1. **Add a split border width helper.**

   In `wezboard/wezboard-gui/src/termwindow/render/pane.rs` or another nearby
   render helper module, add a small helper that returns the active split border
   width in physical pixels:
   - Return `0.0` when `num_panes <= 1`.
   - Return `0.0` when the pane is zoomed.
   - Interpret `split_border_width` as logical pixels.
   - Do not rely on `Dimension::evaluate_as_pixels` for pixel units here;
     current `Dimension::Pixels(n)` returns `n.floor()` as physical pixels.
   - Convert logical pixels to physical pixels using the current window
     scale/DPI. With the current available `dpi`, use
     `physical = logical * dpi / 96.0`, rounded consistently for drawing and hit
     testing.
   - Use this helper everywhere split border geometry is computed.

   Do not change global `Dimension::Pixels` semantics, since other config values
   may already depend on physical-pixel behavior.

2. **Introduce shared pane geometry.**

   In `wezboard/wezboard-gui/src/termwindow/render/pane.rs`, compute a shared
   per-pane geometry struct or helper return value with:
   - `outer_rect` — the current pane background rectangle.
   - `border_width` — the active physical border width.
   - `inner_rect` — `outer_rect` inset by `border_width` on all four sides.
   - `content_origin` — the pixel origin used for terminal line rendering.
   - `content_pixel_width` — the horizontal physical pixel span available to
     line rendering inside the border.

   Clamp inner width/height and content width to zero or another safe minimum so
   narrow panes cannot produce negative geometry.

3. **Inset pane content by using the inner rect.**

   In `wezboard/wezboard-gui/src/termwindow/render/pane.rs`, update `paint_pane`
   so the existing `num_panes` parameter is used. When borders are active:
   - Use the inner content origin for `left_pixel_x`.
   - Use the inner content origin for the `top_pixel_y` passed into
     `LineRender`.
   - Pass `content_pixel_width` to `render_screen_line`.
   - Inset pane background fills so they align with the inner content area, or
     document and verify if the outer pane background intentionally remains
     under the border.

   The implementation must not simply draw the same cell grid into a narrower
   clip if that visibly chops the rightmost glyphs or bottom row. If the
   existing terminal cell count cannot fit inside the inner content rect, reduce
   the renderable cell grid for the pane or adjust the pane's effective
   renderable dimensions so cells fit the inner rect. This may require touching
   pane sizing or renderable-dimension plumbing; clipping edge cells is not an
   acceptable pass result.

4. **Update mouse-to-cell mapping.**

   Any mouse coordinate path that maps window pixels to pane cells must subtract
   the inner content origin before computing row/column. This includes
   click-to-focus/pass-through, selection, and any terminal mouse forwarding.
   The same helper used for rendering should supply the inset/origin so mouse
   behavior and drawing cannot drift apart.

5. **Update browser overlay coordinates.**

   In `wezboard/wezboard-gui/src/termwindow/render/paint.rs`, overlay frames are
   currently derived from the `pane_pixel_x` and `pane_pixel_y` returned by
   `paint_pane`. After the content origin moves inward, return the inner content
   origin and use it for `set_overlay_frame` and `create_pending_ca_layer_host`.
   Browser overlays must align with terminal content, not the outer border rect.

6. **Keep border drawing aligned with shared geometry.**

   Update `paint_pane_border` to use the shared `outer_rect` and `border_width`.
   The drawn rectangles and content inset must agree exactly. Be careful around
   the existing half-cell expansion used for pane backgrounds at interior split
   edges; border drawing and content inset should share one geometry source so
   they do not produce gaps or overlaps.

7. **Preserve split resize hit regions.**

   In `wezboard/wezboard-gui/src/termwindow/render/split.rs` and/or
   `wezboard/wezboard-gui/src/termwindow/render/paint.rs`, separate split
   hit-region registration from old divider drawing:
   - Keep drawing the old thin divider only when `split_border_width == 0`.
   - Always register a `UIItemType::Split` for each split when multiple panes
     are visible.
   - When borders are enabled, make the hit region cover the visible
     border/divider area and use the same logical-to-physical border conversion
     as the border drawing.
   - Do not make the mouse target only as thin as the visible border. Use a
     practical minimum hit thickness, such as the old cell-sized split hit
     region or `max(border_width, cell_width / 2.0)` for vertical dividers and
     `max(border_width, cell_height / 2.0)` for horizontal dividers.

8. **Keep single-pane and zoomed behavior unchanged.**

   A single pane must have no border, no content inset, and no split hit region.
   A zoomed pane must also have no split border or inset.

#### Verification

1. Build Wezboard:

   ```bash
   scripts/build.sh wezboard
   ```

2. Configure:

   ```lua
   config.focused_split_border_color = "#7dcfff"
   config.unfocused_split_border_color = "#565f89"
   config.split_border_width = 4
   ```

3. Single pane:
   - No border is drawn.
   - Content starts at the same position as before.
   - No layout space is lost.

4. Split panes:
   - Borders appear on all panes.
   - The focused pane has the focused border color.
   - Unfocused panes have the unfocused border color.
   - Terminal content is inset from the border on all four sides by exactly
     `split_border_width` logical pixels.
   - On a 2x Retina display, `split_border_width = 4` occupies 8 physical
     pixels; on a 1x display, it occupies 4 physical pixels.
   - Rightmost glyphs and the bottom row are not clipped by the border inset.

5. Mouse resizing:
   - Hovering the divider/border region shows the resize cursor.
   - Dragging the divider resizes panes.
   - The old thin divider is not drawn when borders are enabled.
   - Removing `split_border_width` restores the old thin divider and its mouse
     resize behavior.
   - Clicking, selecting text, and terminal mouse forwarding still hit the
     correct cells after the content origin moves inward.

6. Zoom:
   - Zooming a pane hides borders and removes the inset.
   - Unzooming restores borders and inset.

7. Overlay sanity:
   - Browser overlays still align with terminal content after the pane content
     origin moves inward.

**Result:** Partial

Implemented the shared split-border geometry in Wezboard and verified that the
debug Wezboard build completes with `scripts/build.sh wezboard`.

The implementation now resolves `split_border_width` as logical pixels for
split-border geometry, converts it to physical pixels with the current DPI,
insets pane content and browser overlay origins by that amount, keeps border
drawing aligned with the shared outer pane rectangle, and registers split resize
hit regions even when the old thin divider is not drawn.

Manual GUI verification is still pending for the exact visual inset, Retina
pixel scaling, mouse resize behavior, selection/click cell mapping, and zoom
transitions.

#### Conclusion

The core border-box model is implemented and buildable. The next experiment
should run the manual split-pane checks, tighten any visual or mouse-positioning
drift found at runtime, and then decide whether the issue can be closed.

### Experiment 2: Make Border Geometry Match Pane Sizing

#### Description

Experiment 1 proved the basic direction, but it only changed local rendering
geometry. That avoids drawing the border directly over glyphs, but it does not
make the pane's visible cell grid agree with the terminal and mux dimensions. If
a pane still has 80 columns while only 79 columns fit inside the bordered
content rect, the rightmost cell can become invisible instead of clipped. That
is not acceptable.

This experiment should make the whole pane contract coherent: the mux pane
position, terminal renderable dimensions, pane background, split border, browser
overlay origin, and mouse coordinate mapping must all agree on the same visible
content grid.

#### Changes

1. **Plumb border inset into pane sizing.**

   In the tab/pane sizing path, account for the active split border inset before
   assigning the visible cell dimensions to each pane. Start by tracing
   `wezboard/wezboard-gui/src/termwindow/resize.rs` for window-to-tab sizing and
   `wezboard/mux/src/tab.rs` for split layout and per-pane resize. When multiple
   panes are visible and the pane is not zoomed:
   - Convert `split_border_width` from logical pixels to physical pixels using
     the same helper added in Experiment 1.
   - Compute how many full terminal cells remain visible after reserving
     `2 * border_width` horizontally and vertically.
   - Set the pane's effective/renderable cell width and height to that visible
     grid.

   The PTY, `RenderableDimensions`, `PositionedPane.width`,
   `PositionedPane.height`, line rendering, selection, and mouse mapping must
   agree on this visible grid. Do not merely shrink `RenderScreenLineParams` at
   paint time.

   If borders would consume too much space in a tiny pane, clamp the visible
   grid to at least one column by one row and reduce or suppress the effective
   border for that pane so geometry never goes negative.

2. **Keep split coordinates in the same grid.**

   `PositionedSplit.left`, `PositionedSplit.top`, and `PositionedSplit.size` are
   cell coordinates. When borders reduce the visible pane grid, split positions
   must be recomputed or derived in the same coordinate system as
   `PositionedPane.left`, `PositionedPane.top`, `PositionedPane.width`, and
   `PositionedPane.height`.

   Do not leave dividers at pre-inset cell positions while pane content uses a
   bordered visible grid. The divider pixel position, split hit region, pane
   content origin, and pane dimensions must all derive from one coherent split
   layout.

3. **Use one rect model for border, background, and content.**

   Rework `PaneRenderGeometry` so it exposes:
   - `outer_rect` — the pane area including the border.
   - `border_rect` — the area painted by the border.
   - `content_rect` — the cell-aligned area where terminal content begins.
   - `background_rect` — the pane background fill area.

   When borders are active, drop the old half-cell expansion on interior pane
   edges. Adjacent pane outer rects should meet pixel-perfectly, and
   `content_rect` should be `outer_rect` inset by `border_width` on all four
   sides. The distance from the inner edge of the visible border to the start of
   terminal content must be exactly `border_width` on every edge, including
   interior split edges.

4. **Define shared-edge border precedence.**

   Adjacent panes share interior border pixels. If the focused and unfocused
   border colors differ, the focused pane's border should win on shared edges so
   focus highlighting remains visible and deterministic.

5. **Eliminate unpainted interior strips.**

   Ensure layer 0 background fills every pixel inside the pane that is not
   intentionally transparent:
   - The border area should be painted by the border.
   - The content area should be painted by the pane background and line
     backgrounds.
   - If rounding leaves any non-content pixel inside the border, paint it with
     the pane background.

   Interior split edges must not show the window background as a seam between a
   border and the pane content.

6. **Restore captured mouse offset preservation.**

   In `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`, keep the Experiment
   1 content-origin correction, but restore the old behavior for captured mouse
   drags outside the pane bounds:
   - Negative horizontal drift past the left edge must preserve `x_pixel_offset`
     instead of clamping everything to column 0.
   - Negative vertical drift past the top edge must preserve `y_pixel_offset`
     instead of clamping everything to row 0.
   - Drag-selection and terminal mouse forwarding should continue to receive
     stable out-of-bounds offsets.

7. **Keep resize hit regions easy to grab.**

   Use the old full-cell split hit region as the minimum mouse target when
   borders are enabled:
   - Vertical divider target width: `max(border_width, cell_width)`.
   - Horizontal divider target height: `max(border_width, cell_height)`.

   The visible old divider should still be suppressed when borders are enabled.

8. **Remove unrelated formatting churn.**

   Keep the diff limited to the files required for this experiment. Avoid
   import-only churn unless the edited code requires it.

#### Verification

1. Build Wezboard:

   ```bash
   scripts/build.sh wezboard
   ```

2. Configure:

   ```lua
   config.focused_split_border_color = "#7dcfff"
   config.unfocused_split_border_color = "#565f89"
   config.split_border_width = 4
   ```

3. Single pane:
   - No border is drawn.
   - The pane has the same cell dimensions and content origin as before.
   - Mouse clicks and selection behave as before.

4. Split panes:
   - Borders appear on all panes.
   - Focused and unfocused panes use the correct border colors.
   - The focused border wins on shared interior edges.
   - Content starts exactly one converted `split_border_width` inside the
     visible border on every edge, including interior split edges.
   - No window-background seam appears between the border and pane background.
   - Rightmost glyphs and the bottom row remain visible.
   - Dividers sit exactly between the panes in the same coordinate system as the
     visible cell grid.
   - The pane's reported terminal dimensions match the visible cell grid. Check
     this with `stty size` or `tput cols` inside the split pane.
   - Very small panes still show at least a 1x1 visible cell grid and do not
     produce negative geometry or paint artifacts.

5. DPI:
   - On a 2x Retina display, `split_border_width = 4` reserves 8 physical
     pixels.
   - On a 1x display, `split_border_width = 4` reserves 4 physical pixels.
   - Moving the window between displays with different scale/DPI recomputes the
     border width and visible pane grid.
   - Reloading config after changing `split_border_width` recomputes pane sizes,
     border geometry, hit regions, and overlay positions.

6. Mouse behavior:
   - Hovering over the border/divider region shows the resize cursor.
   - Dragging the divider resizes panes.
   - The old thin divider is not drawn when borders are enabled.
   - Removing `split_border_width` restores the old divider and old resize
     target.
   - Clicking and selecting text hit the expected cells after the inset.
   - Drag-selection that leaves a pane past the left or top edge preserves
     negative pixel offsets and does not snap incorrectly.

7. Overlay and zoom:
   - Browser overlays align with terminal content inside the inset pane.
   - Zooming a pane hides borders, removes the inset, and restores the full pane
     grid.
   - Unzooming restores borders, inset, overlay alignment, and resize hit
     regions.

**Result:** Fail

Experiment 2 did not produce a correct solution. The implementation attempted to
mix WezTerm's overlapping split-gutter geometry with Ghostboard-style pane
borders. That made the coordinate system inconsistent: some attempts hid edge
cells, some produced split gaps, some removed normal pane padding, and none
implemented the captured-mouse offset fix reliably.

#### Conclusion

The whole implementation approach was wrong. The next experiment should revert
the Issue 777 code changes back to the pre-implementation state so a future
attempt can start from clean geometry instead of patching a broken model.

### Experiment 3: Revert Failed Split Border Implementation

#### Description

Fully revert the code changes from the failed Issue 777 implementation path. The
previous attempts were based on the wrong model: they tried to graft thick pane
borders onto WezTerm's existing shared split gutter instead of defining a single
coherent pane geometry.

This experiment should restore the code to the pre-implementation behavior so
future work can redesign the feature from a clean baseline.

#### Changes

1. Revert the Wezboard code changes from the failed implementation commits:
   - `2aac155a176b8` — Restore split border geometry.
   - `0bc8211f85248` — Align bordered pane grids.
   - `3b166a2d98b25` — Fix bordered pane padding.
   - `3d6e108460e61` — Render full bordered pane grid.
2. Leave the issue document in place and record why the implementation was
   reverted.
3. Do not close the issue.

#### Verification

1. Confirm the Wezboard source files touched by the failed implementation match
   their pre-implementation state:
   - `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`
   - `wezboard/wezboard-gui/src/termwindow/render/paint.rs`
   - `wezboard/wezboard-gui/src/termwindow/render/pane.rs`
   - `wezboard/wezboard-gui/src/termwindow/render/split.rs`
2. Build Wezboard:

   ```bash
   scripts/build.sh wezboard
   ```

**Result:** Pass

The failed Wezboard implementation changes from Issue 777 were reverted. The
touched source files now match their pre-implementation state, and the debug
Wezboard build completes with `scripts/build.sh wezboard`.

#### Conclusion

The codebase is back at the clean pre-implementation baseline for split pane
borders. Issue 777 remains open; the next implementation attempt must start from
a fresh geometry design instead of patching the reverted approach.

### Experiment 4: Implement Ghostboard Border Boxes

#### Description

Implement the split border behavior with one explicit model: when
`split_border_width > 0` and more than one pane is visible, Wezboard should use
Ghostboard-style pane border boxes. Do not mix this with WezTerm's old half-cell
split gutter model.

In this model:

- Each pane has a non-overlapping `outer_rect`.
- Adjacent pane `outer_rect`s abut exactly at split boundaries.
- The border is drawn inside the pane's `outer_rect`.
- Terminal content starts at `outer_rect + border_width`.
- The pane's PTY/renderable cell grid is the content grid inside the border.
  Border space is reserved before pane and split positions are exposed to
  rendering, so the rightmost column and bottom row remain visible without
  paint-time clipping.
- Split resize hit regions straddle the shared boundary between adjacent
  `outer_rect`s.
- Single-pane and zoomed-pane rendering keep the old behavior: no split border,
  no content inset, no split-border hit region.

This experiment must not resize PTYs from paint, must not mutate mux pane
dimensions locally in render code, and must not use the old half-cell-expanded
background rect as the bordered pane's outer rect.

#### Changes

1. **Reserve border space before rendering.**

   Add a layout-level bordered pane mapping before `paint_pane` sees pane
   positions. This should be near the GUI's tab/pane positioning path, not in
   line rendering and not by resizing panes from paint.

   When `split_border_width > 0`, `num_panes > 1`, and the pane is not zoomed:
   - Convert `split_border_width` from logical pixels to physical pixels using
     `physical = logical * dpi / 96.0`.
   - Treat each pane's terminal cell grid as the content grid.
   - Build a visual `outer_rect` around that content grid by adding
     `border_width` on each side.
   - Recompute adjacent pane visual positions so neighboring `outer_rect`s abut
     exactly. Do not leave later panes at pre-border positions if earlier panes
     reserve border space.
   - Recompute split hit-region positions from those same abutting visual
     boundaries.

   The important invariant is: `PositionedPane` content origin, visual
   `outer_rect`, split hit region, browser overlay origin, and mouse-to-cell
   mapping all derive from the same bordered layout. Do not shrink only
   `RenderScreenLineParams`, and do not resize PTYs from paint.

2. **Create a bordered pane geometry helper.**

   In `wezboard/wezboard-gui/src/termwindow/render/pane.rs`, add a helper used
   only when `split_border_width > 0`, `num_panes > 1`, and the pane is not
   zoomed. The helper should compute:
   - `outer_rect` from the bordered visual layout, without half-cell expansion.
   - `border_width` from the layout-level converted physical pixel value.
   - `content_origin` as `outer_rect.origin + border_width`.
   - `content_rect` as the pane's full PTY/renderable cell span starting at
     `content_origin`.
   - `background_rect` as `outer_rect`.

   The old half-cell-expanded background rect remains only for the no-border
   path.

3. **Render content from the bordered content origin.**

   In `paint_pane`, when bordered geometry is active:
   - Use the bordered `content_origin` for `left_pixel_x` and `top_pixel_y`.
   - Pass `pixel_width = dims.cols * cell_width` to `render_screen_line`.
   - Keep `RenderableDimensions.cols` and `viewport_rows` unchanged.
   - Do not subtract `2 * border_width` from render width or height.

   The pane dimensions reaching this point must already be the visible content
   grid selected by the bordered layout. Rendering must not hide columns or rows
   to compensate for border pixels.

4. **Draw borders inside non-overlapping pane boxes.**

   Update `paint_pane_border` to use the bordered `outer_rect`. Since adjacent
   `outer_rect`s abut instead of overlapping, interior borders should not create
   a cell-wide gutter or cover neighboring content.

   Mechanical correctness check: for two horizontally adjacent panes,
   `left_pane.outer_rect.max_x() == right_pane.outer_rect.origin.x`. The two
   borders touch at that boundary; they do not overlap content and they do not
   leave an unpainted seam.

   Paint unfocused pane borders first and focused pane borders last so focused
   edges win on shared boundaries.

5. **Register split resize hit regions from split boundaries.**

   In `wezboard/wezboard-gui/src/termwindow/render/split.rs` and/or `paint.rs`,
   keep suppressing the old thin divider when split borders are enabled, but
   always register `UIItemType::Split`.

   For bordered panes:
   - The vertical resize hit region should straddle the boundary at
     `padding_left + os_border.left + split.left * cell_width`.
   - The horizontal resize hit region should straddle the boundary at
     `top_pixel_y + split.top * cell_height`.
   - Minimum target thickness is the old full-cell hit region:
     `max(border_width, cell_width)` or `max(border_width, cell_height)`.

   The hit region is for mouse interaction only; do not draw the old divider
   when borders are active.

6. **Update overlay and mouse geometry from the same helper.**

   Browser overlay frames should use the bordered content origin returned by
   `paint_pane`.

   In `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`, update
   `mouse_position_for_pane` so mouse-to-cell mapping subtracts the same
   bordered content origin before computing row/column. Split the clamping
   behavior:
   - Clamp row/column to valid cell coordinates when needed.
   - Preserve the raw signed pixel offset for `x_pixel_offset` and
     `y_pixel_offset`, including negative values when a captured drag leaves the
     pane past the left or top edge.

   Do not use `rel_x.max(0)` or `rel_y.max(0)` to compute the stored pixel
   offsets.

7. **Keep old paths unchanged when borders are inactive.**

   With one pane, a zoomed pane, or `split_border_width = 0`, the old WezTerm
   gutter/background/divider behavior should remain unchanged.

8. **Keep the diff scoped.**

   Touch only the files required for pane geometry, split hit regions, overlay
   positioning, mouse mapping, and this issue document. Avoid formatter churn.

#### Verification

1. Build Wezboard:

   ```bash
   scripts/build.sh wezboard
   ```

2. Configure:

   ```lua
   config.focused_split_border_color = "#7dcfff"
   config.unfocused_split_border_color = "#565f89"
   config.split_border_width = 4
   ```

3. Single-pane baseline:
   - Open Wezboard with one pane.
   - No split border is drawn.
   - Existing window padding remains unchanged.
   - `stty size` matches the visible terminal grid.

4. Split-pane border boxes:
   - Open a split pane.
   - Borders appear on both panes.
   - The focused pane uses the focused border color.
   - The unfocused pane uses the unfocused border color.
   - Adjacent pane borders meet at the split boundary without a cell-wide
     gutter, unpainted seam, or overlap over content.
   - Content in every pane starts exactly `split_border_width` logical pixels
     inside that pane's visible border on all four edges.
   - Normal window padding remains visible.
   - For an interior split, verify the left pane's `outer_rect.max_x()` equals
     the right pane's `outer_rect.origin.x` by logging or inspecting the
     computed geometry while testing.

5. Edge-cell visibility:
   - In each split pane, print text that reaches the rightmost column and bottom
     row.
   - The rightmost glyphs and bottom row remain visible and are not painted
     under the border.
   - Take a screenshot or inspect pixels to confirm the right edge of the
     rightmost glyph is separated from the inner border edge by the configured
     border inset, not hidden underneath it.
   - `stty size` or `tput cols` matches the pane's visible content grid.

6. Mouse resize:
   - Hovering the shared border area shows the resize cursor.
   - Dragging the shared border resizes the panes.
   - The old thin divider is not drawn when borders are enabled.
   - Removing `split_border_width` restores the old thin divider and old resize
     behavior.

7. Mouse cell mapping:
   - Clicking, selecting text, and terminal mouse forwarding hit the expected
     cells after the content origin moves inward.
   - Drag-selection that leaves a pane past the left or top edge preserves
     negative pixel offsets and does not snap incorrectly.

8. Overlay and zoom:
   - Browser overlays align with terminal content inside the bordered pane.
   - Zooming a pane hides borders and removes the inset.
   - Unzooming restores borders, inset, overlay alignment, and resize hit
     regions.

9. DPI:
   - On a 2x Retina display, `split_border_width = 4` draws and reserves 8
     physical pixels.
   - On a 1x display, `split_border_width = 4` draws and reserves 4 physical
     pixels.
