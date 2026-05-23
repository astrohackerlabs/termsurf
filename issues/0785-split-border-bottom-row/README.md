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

## Proposed Solution

Adopt the border-box model directly: the split border consumes real pixel space,
and the terminal grid is derived from the remaining content area.

The previous Issue 777 implementation tried to keep the mux/PTY grid unchanged
while shifting rendered pixels inward. That creates an impossible contract: the
terminal still believes it owns `N` rows, while the bordered content rect only
has room for `N - 1` rows. Paint then silently hides a row.

The correct model is:

1. Determine the pane's outer visual area.
2. Reserve `split_border_width` on all four sides when split borders are active.
3. Compute the content rect inside that reserved border.
4. Derive visible rows and columns from that content rect.
5. Ensure the PTY/renderable dimensions, terminal rendering, browser overlays,
   mouse mapping, border drawing, and split resize hit regions all use that same
   content rect.

Enabling split borders may reduce a pane's visible row or column count. That is
acceptable. Hiding a row that the PTY still thinks exists is not acceptable.

## Experiments

### Experiment 1: Derive Grid Size From Bordered Content Rect

#### Description

Fix the bottom-row regression by moving the split-border reservation earlier in
the pane sizing/rendering contract. Instead of painting fewer rows from a pane
whose PTY still has the old size, make the pane's visible terminal grid match
the content area that remains after the border is reserved.

This experiment intentionally adopts the user's model:

```text
outer pane area
  - split border on all sides
    = content rect
      -> rows/columns are derived from this rect
      -> PTY/rendering/mouse/overlay all agree on this rect
```

The end result should be that border space is real. If a border consumes enough
space to reduce a pane from 30 rows to 29 rows, the PTY should report 29 rows
and apps should render only 29 rows. No terminal app should be able to draw into
a hidden 30th row.

#### Non-Negotiable Invariants

- The bottom visible terminal row must never be clipped or hidden.
- The PTY size, `RenderableDimensions`, visible rows/columns, and painted
  terminal content must agree.
- The fix must not silently drop rows or columns in `paint_pane()`.
- Single-pane behavior remains unchanged: no split border, no border inset.
- Zoomed-pane behavior remains unchanged: no split border, no border inset.
- Split borders still appear and visually reserve space.
- Browser overlays align with the bordered content rect.
- Mouse clicks, selection, and terminal mouse forwarding map to the visible
  content grid.
- Split resize hit regions remain hoverable and draggable.

#### Changes

1. **Move border reservation out of paint-only row reduction.**

   Audit the current Issue 777 code path in:
   - `wezboard/wezboard-gui/src/termwindow/render/pane.rs`
   - `wezboard/wezboard-gui/src/termwindow/render/paint.rs`
   - `wezboard/wezboard-gui/src/termwindow/render/split.rs`
   - `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`
   - the relevant pane sizing path in `wezboard/wezboard-gui/src/termwindow/`
     and `wezboard/mux/src/tab.rs`

   Identify where Wezboard determines pane pixel area, pane cell dimensions, and
   PTY resize dimensions. The border reservation belongs in that sizing
   contract, not as a late `RenderableDimensions` shrink inside `paint_pane()`.

2. **Define one bordered content rect.**

   When `split_border_width > 0`, more than one pane is visible, and the pane is
   not zoomed:
   - convert `split_border_width` from logical pixels to physical pixels using
     the current DPI;
   - reserve that physical width on all four sides of the pane's outer visual
     area;
   - compute `content_rect = outer_rect.inset(border_width)`;
   - derive `visible_cols = floor(content_rect.width / cell_width)`;
   - derive `visible_rows = floor(content_rect.height / cell_height)`;
   - clamp tiny panes so the visible grid never becomes negative.

   When split borders are inactive, keep the existing sizing behavior.

3. **Make PTY/renderable dimensions match the content rect.**

   Ensure the pane's effective terminal dimensions use `visible_cols` and
   `visible_rows` from the bordered content rect. The PTY should receive those
   dimensions through the normal pane resize/layout path, not from paint.

   Remove the current paint-time workaround that creates a smaller temporary
   `RenderableDimensions` from `content_pixel_width` and `content_pixel_height`.
   `paint_pane()` should render the rows and columns that the pane actually
   owns.

4. **Render at the bordered content origin.**

   `paint_pane()` should still use the content rect origin for:
   - `left_pixel_x`;
   - `top_pixel_y`;
   - returned browser overlay origin.

   But it should not decide that fewer rows are visible than the pane's real
   dimensions. Any row/column reduction must happen before the pane dimensions
   reach paint.

5. **Keep border, overlay, mouse, and split hit regions on the same geometry.**

   Use the same bordered content rect for:
   - border drawing;
   - browser overlay positioning;
   - mouse-to-cell mapping;
   - selection;
   - terminal mouse forwarding;
   - split resize hit-region placement.

   Avoid duplicated math that could make the visible content, mouse coordinates,
   and overlay origin drift apart.

6. **Do not accept a hidden-row workaround.**

   Do not fix this by:
   - sacrificing the top row instead of the bottom row;
   - rendering all rows into a smaller clipped area;
   - squishing row height;
   - drawing rows under the border and relying on paint order;
   - silently shrinking `RenderableDimensions` only inside `paint_pane()`.

7. **Define rollback if the model cannot be implemented safely.**

   If making the PTY/grid dimensions match the bordered content rect proves too
   invasive or regresses basic pane behavior, rollback the Issue 777
   split-border implementation rather than shipping a terminal that hides
   bottom-row content.

   The rollback target is the behavior before commit
   `61ff8e625d0f0 Restore presentation split borders`: split borders may lose
   their real-margin behavior, but terminal content must remain fully visible.

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
   - no split border is drawn;
   - `stty size` matches the visible grid;
   - content starts exactly where it did before.

4. Split pane bottom row:
   - open a split pane;
   - run a shell prompt on the last visible row;
   - the prompt remains visible;
   - `stty size` reports the visible row/column count, not the pre-border count;
   - printing text on the last row does not disappear under the border.

5. TUI status lines:
   - run Codex or another TUI with a bottom status line;
   - run Neovim in a split pane;
   - bottom status/command lines remain visible.

6. Edge cells:
   - print text reaching the rightmost column and bottom row;
   - the rightmost column and bottom row remain visible;
   - no row or column is silently hidden by paint.

7. Border appearance:
   - borders appear when a split is opened;
   - content visibly starts inside the border;
   - focused and unfocused border colors still work;
   - no unpainted seam appears between border and pane content.

8. Mouse and overlays:
   - browser overlays align with the bordered content rect;
   - clicking and selecting text hit the expected cells;
   - terminal mouse forwarding targets the expected cells;
   - split resize regions remain hoverable and draggable.

9. Zoom, window modes, and small panes:
   - zooming a pane hides borders and restores the full-pane grid;
   - unzooming restores borders and the bordered content grid;
   - test in both windowed and fullscreen modes;
   - test a small split pane, around 3-5 rows tall, and confirm it still has a
     coherent visible grid.

#### Pass Criteria

The experiment passes if all verification scenarios pass and the PTY/renderable
row and column count always matches the visible bordered content grid.

#### Partial Criteria

The experiment is Partial if the bottom-row regression is fixed but one
secondary behavior needs follow-up, such as a minor border painting seam,
horizontal edge-cell mismatch, or browser overlay offset. Partial is not
acceptable if terminal bottom-row content can still be hidden.

#### Failure Criteria

The experiment fails if:

- the bottom row can still disappear;
- the fix hides a different row or column instead;
- the PTY reports more rows/columns than are visible;
- split borders stop reserving visible space;
- browser overlays, mouse mapping, selection, or split resizing regress;
- the implementation requires unsafe layout churn that is worse than rolling
  back Issue 777.
