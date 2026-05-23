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

"Border-box model" means the border is part of the pane's allocated visual area:
the border consumes space first, and the terminal content grid is computed from
what remains. This is different from the failed Issue 777 presentation model,
where the terminal grid kept the full pane size and the border tried to shift
painted pixels afterward.

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

   The likely sizing pipeline to inspect is:

   ```text
   termwindow resize/layout event
     -> mux::tab::Tab::resize(...)
     -> split tree computes each PositionedPane
     -> pane resize / renderable dimensions are assigned
     -> PTY receives rows/cols through the normal resize path
   ```

   Insert the border reservation between "pane visual pixels are known" and
   "rows/cols are derived from those pixels." Do not insert it inside paint.

2. **Define one bordered content rect.**

   When `split_border_width > 0`, more than one pane is visible, and the pane is
   not zoomed:
   - convert `split_border_width` from logical pixels to physical pixels using
     the current DPI:

     ```text
     border_width_physical = (split_border_width * dpi / 96.0).round()
     ```

   - use physical pixels throughout this calculation. `cell_width` and
     `cell_height` are already physical pixels for the current DPI;
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

   The PTY/grid recompute must happen on every transition that changes the
   bordered content rect:
   - window resize;
   - split add/remove;
   - zoom/unzoom;
   - config reload that changes `split_border_width` or border enablement;
   - DPI/display-scale change.

   Each transition that changes whether border space is reserved must trigger a
   fresh content-rect calculation and normal PTY resize.

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

   After this change, `pane_render_geometry()` should not own grid sizing.
   Either delete it if the new sizing path subsumes it, or simplify it so it
   only returns the content origin / border drawing geometry needed by paint.
   Remove `content_pixel_width` and `content_pixel_height` from its return value
   unless they are purely descriptive and never used to derive temporary
   `RenderableDimensions`.

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

   Pay particular attention to browser overlays. The render pass currently feeds
   pane pixel coordinates into `set_overlay_frame()` and
   `create_pending_ca_layer_host()` through
   `wezboard/wezboard-gui/src/termsurf/conn.rs`. Those calls must receive the
   bordered content origin, not the outer border rect, so CALayerHost browser
   overlays align with terminal content.

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

   Rollback procedure if needed: revert `61ff8e625d0f0` and any later commits
   that only build on that split-border implementation, then verify split
   borders no longer hide terminal rows. Do not keep a known row-clipping fix in
   the tree while searching for a better border model.

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
   - verify the existing pane receives a resize and `stty size` changes if the
     border reduces its visible grid;
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
   - `stty size` grows when zoom removes the border reservation;
   - unzooming restores borders and the bordered content grid;
   - `stty size` shrinks back to the bordered visible grid;
   - test in both windowed and fullscreen modes;
   - test a small split pane, around 3-5 rows tall, and confirm it still has a
     coherent visible grid.

10. Split and config transitions:
    - start with one pane, record `stty size`, then open a split and verify the
      original pane resizes to the bordered visible grid;
    - close the split and verify the remaining pane returns to the single-pane
      grid;
    - with splits active, reload config after changing `split_border_width` from
      `4` to `8`, then back to `4`; all visible panes should resize their grid
      and repaint without requiring a restart.

11. DPI/display-scale transition:
    - if multiple displays are available, move the window between displays with
      different scale factors;
    - verify the physical border width, visible grid, overlay position, and
      mouse mapping recompute together.

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

**Result:** Won't implement

Experiment 1 was cancelled before implementation.

The analysis surfaced the real root cause: Wezboard's mux split tree uses
terminal cell dimensions as its layout currency, while the desired split-border
model requires a pixel-first border-box contract. A correct implementation would
need to distinguish pane outer rectangles from pane content rectangles before
PTY/grid sizing, and then thread that distinction through split layout, resize
events, zoom/unzoom, config reload, DPI changes, browser overlays, mouse
mapping, and split hit regions.

That is broader than an urgent regression fix. Attempting to patch the behavior
in the GUI layer would risk preserving the same architectural mismatch in a
different form: the mux would still allocate panes in grid cells first, while
the renderer would retrofit pixel border space afterward.

#### Conclusion

Do not implement Experiment 1 as a narrow fix. The root cause is architectural:
split layout is grid-first, while true margin-style split borders require a
pixel-first content-rect sizing model.

For this issue, the safer path is rollback of the Issue 777 split-border
implementation. A future border-box implementation should be designed as a
larger layout architecture change, not as an urgent regression patch.

### Experiment 2: Manually Restore Pre-Issue-777 Split Borders

#### Description

Fully and precisely remove the code behavior introduced by Issue 777 Experiment
5, commit `61ff8e625d0f0 Restore presentation split borders`.

This is a rollback of code behavior, not a `git revert` operation. Do not run
`git revert`. Do not apply a reverse patch blindly. Do not modify the closed
Issue 777 document. Instead, manually restore the affected code paths to the
pre-`61ff8e625d0f0` behavior, reviewing every hunk so later unrelated work is
not accidentally removed.

The goal is to return split rendering to the old grid-consistent model where the
terminal pane grid, PTY dimensions, `RenderableDimensions`, mouse mapping, and
browser overlay positioning all agree. This may bring back the visual
border-overlap behavior that Issue 777 tried to improve, but it must eliminate
the deal-breaking hidden bottom row.

#### Non-Negotiable Invariants

- Do not run `git revert`.
- Do not modify closed issue documents, including Issue 777.
- Remove the Issue 777 presentation-inset behavior completely from code.
- Do not leave a hybrid state where content origin is shifted but rows/columns
  are also clipped.
- The bottom row must be visible in split panes.
- `stty size` must match the visible terminal grid.
- Single-pane and zoomed-pane behavior must remain unchanged.
- Browser overlays must keep their pre-Issue-777 alignment behavior.
- Mouse clicks, text selection, terminal mouse forwarding, and split dragging
  must use the same cell coordinate model as the visible grid.

#### Changes

1. **Use the commit diff only as a map.**

   Inspect the code-only portion of commit `61ff8e625d0f0`:

   ```bash
   git show --stat --oneline 61ff8e625d0f0
   git show -- wezboard/wezboard-gui/src/termwindow/mouseevent.rs \
     wezboard/wezboard-gui/src/termwindow/render/paint.rs \
     wezboard/wezboard-gui/src/termwindow/render/pane.rs \
     wezboard/wezboard-gui/src/termwindow/render/split.rs \
     61ff8e625d0f0
   ```

   Use that diff to identify the exact Issue 777 hunks. Then edit files
   manually. The implementation should not restore whole files from the parent
   commit unless a review confirms there have been no later unrelated changes in
   that file.

2. **Restore `render/pane.rs` to pre-presentation-inset behavior.**

   In `wezboard/wezboard-gui/src/termwindow/render/pane.rs`:
   - remove `PaneRenderGeometry` and `pane_render_geometry()` if they are only
     serving the Issue 777 presentation-inset model;
   - remove `split_border_width_physical()` if it has no remaining legitimate
     caller after the rollback;
   - remove any calculation of `content_origin_x`, `content_origin_y`,
     `content_pixel_width`, or `content_pixel_height` that shifts or shrinks
     terminal rendering for split borders;
   - remove the temporary `RenderableDimensions` shrink in `paint_pane()`;
   - restore line rendering width to the pane's actual dimensions instead of the
     Issue 777 content width;
   - restore the `paint_pane()` return value to the pre-Issue-777 pane origin
     used by browser overlays.

   After this step, `paint_pane()` must not reduce row or column count based on
   `split_border_width`.

3. **Restore split drawing and hit regions in `render/split.rs`.**

   In `wezboard/wezboard-gui/src/termwindow/render/split.rs`:
   - restore `paint_split()` to the pre-Issue-777 signature;
   - remove `draw_divider` and `hit_thickness` parameters;
   - restore the old divider drawing behavior;
   - restore split `UIItem` hit regions to the cell-based coordinates used
     before the presentation-border experiment.

4. **Restore split rendering call sites in `render/paint.rs`.**

   In `wezboard/wezboard-gui/src/termwindow/render/paint.rs`:
   - remove the Issue 777 logic that computes `split_border_width` and changes
     split drawing or hit-region thickness;
   - restore calls to `paint_split()` to the old argument list;
   - keep browser overlay calls wired to the pane origin returned by
     `paint_pane()`, but that origin should now be the pre-Issue-777 origin.

5. **Restore mouse coordinate handling in `mouseevent.rs`.**

   In `wezboard/wezboard-gui/src/termwindow/mouseevent.rs`:
   - remove the Issue 777 helper path that uses `pane_render_geometry()` to
     remap mouse coordinates to inset content;
   - restore the original window-cell coordinate calculation for terminal mouse
     events;
   - restore split dragging to use the pre-Issue-777 cell coordinate arguments;
   - preserve any later unrelated mouse handling fixes if review finds them.

6. **Audit for leftovers.**

   After manual edits, search for Issue 777 presentation-inset leftovers:

   ```bash
   rg "pane_render_geometry|PaneRenderGeometry|split_border_width_physical|content_pixel_width|content_pixel_height|draw_divider|hit_thickness" \
     wezboard/wezboard-gui/src/termwindow
   ```

   Any remaining match must be intentionally justified in the result. The
   expected outcome is that these Issue 777 rollback-target symbols are gone
   from the split rendering path.

7. **Preserve configuration compatibility.**

   Do not remove the `split_border_width` config option in this experiment. If
   the option becomes unused after the rollback, leave the config surface in
   place and record that it no longer affects split rendering. Removing or
   redefining the option is a separate compatibility decision.

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

3. Split pane bottom row:
   - open a split pane;
   - run `stty size`;
   - move the shell prompt to the last visible row;
   - confirm the prompt/input line is visible;
   - print text on the last row and confirm it remains visible.

4. TUI status lines:
   - run Codex or another TUI with a bottom status line;
   - run Neovim in a split pane;
   - confirm bottom status/command lines are visible.

5. Edge cells:
   - print text reaching the rightmost column and bottom row;
   - confirm no row or column silently disappears.

6. Split behavior:
   - open horizontal and vertical splits;
   - drag split dividers;
   - confirm hit regions work and pane resizing still behaves normally.

7. Mouse behavior:
   - click cells near split boundaries;
   - select text across lines;
   - test terminal mouse forwarding in an app that receives mouse input;
   - confirm clicks target the visible cells.

8. Browser overlay behavior:
   - open a browser pane with `web`;
   - verify the browser overlay aligns with the terminal pane;
   - resize splits and verify the overlay follows the pane.

9. Single-pane and zoomed-pane behavior:
   - verify a single pane renders normally;
   - zoom and unzoom a split pane;
   - confirm bottom-row visibility and overlay/mouse behavior remain correct.

10. Regression acceptance:
    - visually inspect split borders;
    - if the old overlap behavior returns, record it as the accepted rollback
      tradeoff;
    - do not treat border overlap as a failure for this experiment unless it
      hides terminal content.

#### Pass Criteria

The experiment passes if the bottom row is visible in split panes, the terminal
grid and `stty size` agree with what is visible, and the four affected code
paths have been manually restored to the pre-Issue-777 split behavior without
using `git revert`.

#### Partial Criteria

The experiment is Partial if the bottom row is visible but one non-critical
secondary behavior needs a follow-up, such as a visual border artifact or a
minor split-hit-region issue. Partial is not acceptable if any terminal row or
column is still hidden.

#### Failure Criteria

The experiment fails if:

- `git revert` or a blind reverse patch is used;
- the bottom row is still clipped;
- a different row or column becomes hidden;
- mouse mapping, selection, terminal mouse forwarding, split dragging, or
  browser overlay positioning regress;
- closed Issue 777 documentation is modified;
- the code remains in a hybrid state with both pre-Issue-777 and Issue 777
  presentation-inset behavior active.
