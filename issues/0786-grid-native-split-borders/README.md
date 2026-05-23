+++
status = "open"
opened = "2026-05-23"
+++

# Issue 786: Grid-Native Split Borders

## Goal

Implement split pane borders that do not overlap terminal content and do not use
pixel-level presentation insets. The active pane should be easy to identify via
a complete border outline, while the terminal grid, PTY size, browser overlays,
mouse mapping, and split dragging remain cell-consistent.

## Background

[Issue 777](../0777-split-border-overlap/README.md) attempted to make split
borders behave like a real margin instead of painting over pane content. That
approach used a presentation-layer pixel inset around pane rendering.

[Issue 785](../0785-split-border-bottom-row/README.md) found that the
presentation-inset model could hide the bottom terminal row. The root cause was
architectural: Wezboard's mux split tree uses terminal cells as its layout
currency, while Issue 777 tried to add pixel border space after the grid had
already been allocated. The rollback in Issue 785 restored the older
grid-consistent behavior, accepting that borders may again sit over pane
content.

The next implementation should work with Wezboard's architecture instead of
fighting it. Borders should be represented in grid space, not as pixel insets
added during rendering.

## Analysis

Wezboard's split layout is cell-based.

In `wezboard/mux/src/tab.rs`, split children are positioned with a shared
one-cell divider:

```rust
fn left_of_second(&self) -> usize {
    match self.direction {
        SplitDirection::Horizontal => self.first.cols as usize + 1,
        SplitDirection::Vertical => 0,
    }
}

fn top_of_second(&self) -> usize {
    match self.direction {
        SplitDirection::Horizontal => 0,
        SplitDirection::Vertical => self.first.rows as usize + 1,
    }
}
```

`iter_splits()` exposes those divider cells as `PositionedSplit`, and
`render/split.rs` already paints and hit-tests them as split UI regions. That
means shared internal dividers already match the mux's native model.

What the current model lacks is an outer border around the visible pane area.
That outer border is important because the active pane needs a complete outline,
not only the internal edges shared with neighboring panes.

Two possible grid-native models were considered:

1. **Full border grid per pane.**

   Each pane owns a top, bottom, left, and right border cell around its content.
   This is conceptually simple, but adjacent panes create double borders unless
   the implementation collapses neighboring borders into a shared divider. Once
   that collapse is added, this approach has effectively reinvented shared
   dividers with more ownership complexity.

2. **Pane outer perimeter plus shared internal dividers.**

   Keep the mux's existing shared one-cell split dividers for internal pane
   boundaries. Add grid-native outer perimeter border cells around the tab or
   visible split area so panes can be outlined all the way around. Pane content
   remains a cell rect that the PTY actually owns.

The second model better matches Wezboard. It extends the split tree's existing
cell-divider model instead of layering per-pane borders over it.

## Proposed Solution

Implement grid-native split borders using:

- existing shared split divider cells between adjacent panes;
- new outer perimeter border cells around the visible split area;
- active-pane-aware coloring for both shared dividers and perimeter edges;
- no pixel inset, no temporary `RenderableDimensions` shrink, and no post-layout
  content clipping.

Conceptually:

```text
tab grid
  perimeter border cells
  pane content cells
  shared divider cells between adjacent panes
```

The PTY should only ever receive the content grid that is actually visible. If a
future design needs border cells to consume additional rows or columns, that
must happen in the mux/layout cell model before PTY dimensions are assigned.
Rendering must not silently hide rows or columns after the fact.

## Constraints

- Do not reintroduce the Issue 777 pixel presentation-inset model.
- Do not shift pane rendering by pixel border widths.
- Do not shrink `RenderableDimensions` inside `paint_pane()`.
- Do not hide terminal rows or columns under border paint.
- Keep split layout, mouse mapping, browser overlays, split hit regions, and PTY
  dimensions in one cell-coordinate system.
- The active pane must have a complete visual outline, including outer edges.
- Shared internal dividers are preferable to duplicated per-pane borders.

## Open Questions

- Should the perimeter border apply to the whole tab grid, each top-level split
  subtree, or each visible pane's exterior edges?
- How should active-pane coloring work for shared dividers between active and
  inactive panes?
- Should `split_border_width` be reinterpreted as a cell-count option, or should
  grid-native borders use a separate configuration option?
- What is the minimum viable implementation that restores a complete active
  outline without changing PTY dimensions unexpectedly?

For Experiment 1, answer these conservatively:

- apply perimeter borders to visible pane exterior edges;
- collapse interior edges into the existing shared `PositionedSplit` dividers;
- prefer the active pane's color on shared dividers so the active outline is
  continuous;
- make grid-native borders one cell thick;
- do not reinterpret `split_border_width` yet.

## Experiments

### Experiment 1: One-Cell Shared-Divider Outline

#### Description

Implement the first grid-native split border model with the smallest behavior
surface:

- keep existing mux split layout and PTY dimensions unchanged;
- keep existing one-cell shared internal dividers;
- add missing one-cell outer edge border segments around visible panes;
- make the active pane visually outlined on all four sides;
- do not reintroduce pixel insets, temporary render-dimension shrinkage, or
  post-layout clipping.

This experiment intentionally ignores `split_border_width` for the new
grid-native behavior. Borders are one cell thick because Wezboard's split layout
already uses cells as its currency. The existing `split_border_width` config
field remains for compatibility and for the old pre-grid-native rendering path,
but it is not the shape control for this experiment.

#### Non-Negotiable Invariants

- Do not use pixel presentation insets.
- Do not change PTY row or column counts in this experiment.
- Do not shrink `RenderableDimensions` inside `paint_pane()`.
- Do not hide terminal rows or columns under border paint.
- Do not break existing shared split divider hit regions or split dragging.
- Browser overlays must remain aligned to pane content.
- Mouse clicks, selection, and terminal mouse forwarding must keep targeting the
  visible terminal cells.
- Single-pane and zoomed-pane behavior remain unchanged: no split outline is
  drawn.

#### Changes

1. **Audit the current split geometry.**

   Confirm the current rollback state:

   ```bash
   rg "pane_render_geometry|PaneRenderGeometry|split_border_width_physical|content_pixel_width|content_pixel_height|content_origin_x|content_origin_y|draw_divider|hit_thickness" \
     wezboard/wezboard-gui
   ```

   Expected: no matches.

   Inspect:
   - `wezboard/mux/src/tab.rs::SplitDirectionAndSize::{left_of_second,top_of_second}`;
   - `wezboard/mux/src/tab.rs::iter_panes()`;
   - `wezboard/mux/src/tab.rs::iter_splits()`;
   - `wezboard/wezboard-gui/src/termwindow/render/split.rs`;
   - `wezboard/wezboard-gui/src/termwindow/render/pane.rs::paint_pane_border`;
   - `wezboard/wezboard-gui/src/termwindow/render/paint.rs`.

   The expected finding is that internal split dividers are already represented
   as shared one-cell grid regions and should be reused.

2. **Define active-pane border ownership in grid cells.**

   Use visible `PositionedPane` values from `get_panes_to_render()` and visible
   `PositionedSplit` values from `get_splits()`.

   For each visible pane, determine which of its four sides should be drawn:
   - if the side touches a shared split divider, that side is represented by the
     existing divider;
   - if the side touches the outer visible split area/window edge, draw a new
     one-cell perimeter segment;
   - do not draw duplicate borders on both sides of a shared divider.

   The active pane should have a continuous visual outline. When a shared
   divider is adjacent to the active pane, draw that divider using the active
   pane border color.

3. **Render outer perimeter segments in `render/pane.rs` or a new helper.**

   Add a helper that draws one-cell-thick perimeter segments for visible pane
   exterior edges. The helper should work in cell units:
   - horizontal border segment height = `cell_height`;
   - vertical border segment width = `cell_width`;
   - segment coordinates derive from `PositionedPane.left/top/width/height` and
     the existing padding/tab-bar/OS-border origin calculation;
   - segment color uses `focused_split_border_color` for the active pane and
     `unfocused_split_border_color` otherwise, falling back to `palette.split`.

   This helper must not alter pane rendering origin, pane dimensions, or overlay
   coordinates.

4. **Update shared divider coloring without changing hit regions.**

   In `render/split.rs`, keep the existing `paint_split()` signature and
   `UIItem` hit region geometry.

   Update only the color choice so a divider adjacent to the active pane is
   drawn with the focused border color. If determining adjacency inside
   `paint_split()` is awkward, pass enough context from `render/paint.rs` to
   choose the color without changing split layout or hit testing.

5. **Wire the render order in `render/paint.rs`.**

   Keep the existing order that paints pane content and overlays safely.

   Add perimeter border drawing after pane backgrounds/content are painted and
   before modal/tab/window border layers as appropriate. Shared split dividers
   should continue to be painted through the split path.

   The render order should make the outline visible without obscuring terminal
   content rows/columns. Because this experiment only draws in cells that are
   already outside pane content or in existing divider cells, no content should
   be covered.

6. **Leave `split_border_width` alone.**

   Do not reinterpret `split_border_width` as a cell count in this experiment.
   Do not remove it. Do not add a new config option yet.

   The result should state explicitly that Experiment 1 implements a one-cell
   grid-native outline independent of `split_border_width`.

#### Verification

1. Build Wezboard:

   ```bash
   scripts/build.sh wezboard
   ```

2. Configure visible colors:

   ```lua
   config.focused_split_border_color = "#7dcfff"
   config.unfocused_split_border_color = "#565f89"
   config.split_border_width = 4
   ```

   `split_border_width` should not control the new grid-native border thickness
   in this experiment.

3. Single-pane and zoomed panes:
   - open a single pane and confirm no split outline is drawn;
   - open a split, zoom one pane, and confirm the zoomed pane does not get a
     split outline;
   - unzoom and confirm outlines return.

4. Active pane outline:
   - create a two-pane horizontal split;
   - focus each pane in turn;
   - confirm the active pane has a complete visual outline, including the
     outside window edge and the shared divider edge;
   - repeat with a vertical split.

5. Nested splits:
   - create at least three panes with both horizontal and vertical splits;
   - focus each pane in turn;
   - confirm every active pane can be visually identified by a complete outline;
   - confirm shared internal dividers are not double-thick.

6. Bottom row and edge cells:
   - run `stty size` in split panes;
   - print content on the last visible row and rightmost column;
   - confirm the bottom row and rightmost column remain visible;
   - confirm Codex or Neovim bottom status lines are visible.

7. Mouse and split dragging:
   - drag shared split dividers;
   - click/select text near pane edges;
   - run a terminal mouse app and confirm mouse forwarding targets visible
     cells;
   - confirm border drawing did not steal terminal-cell clicks.

8. Browser overlays:
   - open a browser pane with `web`;
   - verify the overlay still aligns with the terminal pane;
   - resize splits and verify the overlay follows its pane.

9. `split_border_width` compatibility:
   - test with `split_border_width = 0`;
   - test with `split_border_width = 4`;
   - confirm the one-cell grid-native outline behavior is unchanged except for
     any old pre-existing divider behavior that still intentionally depends on
     the option.

#### Pass Criteria

The experiment passes if active split panes have a complete one-cell visual
outline, no terminal rows or columns are hidden, shared dividers remain
single-cell and draggable, browser overlays remain aligned, and
`scripts/build.sh wezboard` passes.

#### Partial Criteria

The experiment is Partial if the active-pane outline works for simple splits but
one secondary case needs follow-up, such as nested split coloring or an outer
edge segment missing in a complex layout. Partial is not acceptable if terminal
content is hidden or mouse/split dragging regresses.

#### Failure Criteria

The experiment fails if:

- pixel inset geometry is reintroduced;
- `paint_pane()` shrinks `RenderableDimensions`;
- any terminal row or column is hidden;
- shared dividers become double-thick;
- split dragging, mouse mapping, selection, terminal mouse forwarding, or
  browser overlay positioning regress;
- `split_border_width` is reinterpreted without an explicit follow-up design.
