+++
status = "open"
opened = "2026-05-23"
+++

# Issue 785: Split border hides bottom pane row

## Goal

Fix the split border regression where the bottom row of a split pane is not
visible when `split_border_width` is enabled. The last terminal row must remain
visible because it often contains the shell prompt, Codex status, Neovim status
line, command input, or other critical UI.

This issue is urgent: if the bug cannot be fixed narrowly, the Issue 777 split
border implementation should be considered for immediate rollback.

## Background

[Issue 777](../0777-split-border-overlap/README.md) fixed split pane borders so
they behave like a real margin instead of painting directly over pane content.
The passing implementation came from Issue 777 Experiment 5, committed as:

```text
61ff8e625d0f0 Restore presentation split borders
```

That solution intentionally chose a presentation-layer model:

- do not change mux split layout;
- do not resize PTYs from paint;
- do not mutate pane cell coordinates;
- shift terminal rendering, browser overlays, mouse mapping, border drawing, and
  split hit regions consistently around the existing WezTerm pane geometry.

Manual testing later found a deal-breaking regression: when borders are enabled,
the bottom row of the pane can be cut off. Anything rendered on the final row is
not visible.

## Analysis

The most plausible cause is that the current code both shifts content downward
and reduces the effective rendered row count.

In `wezboard/wezboard-gui/src/termwindow/render/pane.rs`,
`pane_render_geometry()` computes:

```rust
let content_origin_y = top_pixel_y + pos.top as f32 * cell_height + border_width;
let content_pixel_height =
    (pos.height as f32 * cell_height - (border_width * 2.0)).max(0.0);
```

Then `paint_pane()` derives a smaller renderable height from that value:

```rust
let content_rows = ((geometry.content_pixel_height / cell_height).floor() as usize)
    .min(dims.viewport_rows);

let render_dims = RenderableDimensions {
    viewport_rows: content_rows,
    pixel_height: (content_rows as f32 * cell_height) as usize,
    ..dims
};
```

On a Retina display, `split_border_width = 4` becomes roughly `8` physical
pixels. If a pane is `N` rows tall:

```text
content_pixel_height = N * cell_height - 16
content_rows = floor((N * cell_height - 16) / cell_height)
```

For normal cell heights, that floors to `N - 1`. The implementation therefore:

1. moves the first row down by `border_width`;
2. renders one fewer row;
3. leaves the terminal/mux believing the pane still has the original row count.

That explains why bottom-row UI disappears.

This contradicts the intended Issue 777 Experiment 5 model. The conclusion said
to keep the existing pane grid stable and apply a presentation inset. The actual
code still performs a render-time layout shrink by constructing a smaller
`RenderableDimensions`.

## Proposed Fix

Make the Issue 777 fix truly presentation-only for terminal row/column count.

The narrow fix should:

1. Keep shifting the content origin by the split border width.
2. Stop reducing `RenderableDimensions.viewport_rows` from
   `content_pixel_height`.
3. Stop reducing `RenderableDimensions.cols` from `content_pixel_width` unless a
   separate horizontal clipping bug proves that is required.
4. Preserve the existing mux/PTY pane dimensions.
5. Keep browser overlay origin, mouse mapping, border drawing, and split hit
   regions aligned with the shifted content origin.

The candidate code direction is:

```rust
let render_dims = dims;
```

or equivalently preserve:

```rust
cols: dims.cols,
viewport_rows: dims.viewport_rows,
pixel_width: dims.pixel_width,
pixel_height: dims.pixel_height,
```

while still using `geometry.content_origin_x` and `geometry.content_origin_y`
for placement.

If horizontal edge clipping appears after this fix, address it separately and
explicitly. Do not hide terminal rows or columns by silently shrinking
`RenderableDimensions` in paint.

## Verification Requirements

The fix is not acceptable unless all of these pass:

1. With `split_border_width = 4`, open a split pane and run a shell prompt on
   the last visible row. The prompt must remain visible.
2. Run Codex or another TUI with a bottom status line. The bottom status line
   must remain visible.
3. Run Neovim in a split pane. The status line and command line must remain
   visible.
4. Print text to the bottom row and rightmost column. The bottom row must not be
   clipped.
5. The border still visually insets content from the pane edge.
6. Browser overlays still align with shifted terminal content.
7. Mouse clicks and selection still map to the expected terminal cells.
8. Split resize hit regions remain hoverable and draggable.
9. Single-pane and zoomed-pane behavior remain unchanged.

## Notes

This issue should be treated as a regression from Issue 777, not a new border
feature request. The first attempt should be a narrow code audit and fix around
`content_rows`, `content_cols`, and the temporary `RenderableDimensions` created
inside `paint_pane()`.
