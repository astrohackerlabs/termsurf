# Issue 723: Add focused/unfocused split pane borders to Wezboard

## Goal

Add configurable colored borders around split panes in Wezboard that
differentiate active vs inactive panes, matching the Ghostboard feature set.

## Background

Ghostboard (Ghostty fork) implemented this feature across Issues 667-669
and 672. The final solution uses three config keys —
`focused-split-border-color`, `unfocused-split-border-color`, and
`split-border-width` — rendered as SwiftUI overlay rectangles with content inset
by the border width.

Wezboard (WezTerm fork) does not have this feature. WezTerm only draws a thin
1px split divider line using the palette's `split` color via `filled_rectangle`
on layer 2. There is no per-pane border, no focused/unfocused distinction, and
no configurable width.

The user's desired config:

```
focused_split_border_color = "#7dcfff"
unfocused_split_border_color = "#565f89"
split_border_width = 2
```

## Analysis

### Existing rendering infrastructure

Wezboard already has the exact rendering primitives needed:

- **`filled_rectangle`** — Draws colored quads on a specified layer. Used by
  window borders (`borders.rs`), split dividers (`split.rs`), and scrollbar
  thumbs (`pane.rs`).
- **Layer 2** — The overlay layer used by split dividers, drawn on top of pane
  content (layer 0).
- **`background_rect`** — Computed per-pane in `paint_pane` (lines 110-152 of
  `pane.rs`). Gives the full pixel region for each pane, accounting for padding,
  tab bar, OS borders, and edge detection.
- **Window border pattern** — `borders.rs` draws exactly 4 filled rectangles
  (top/bottom/left/right) using `filled_rectangle`. This is the pattern to
  follow.

### Config pattern

WezTerm's config uses `RgbaColor` for colors and `Dimension` with
`#[dynamic(try_from = "crate::units::PixelUnit")]` for pixel widths. The
`WindowFrameConfig` in `color.rs` shows the exact pattern for border width/color
fields.

### Key files

| File                                                     | Role                                                                 |
| -------------------------------------------------------- | -------------------------------------------------------------------- |
| `wezboard/config/src/config.rs`                          | Config struct — add 3 new fields near `inactive_pane_hsb` (line 621) |
| `wezboard/config/src/color.rs`                           | `WindowFrameConfig` pattern, `default_zero_pixel` helper             |
| `wezboard/wezboard-gui/src/termwindow/render/pane.rs`    | Pane rendering, `background_rect` computation, `is_active` flag      |
| `wezboard/wezboard-gui/src/termwindow/render/paint.rs`   | Paint loop — iterates panes (249-258) then splits (260-266)          |
| `wezboard/wezboard-gui/src/termwindow/render/split.rs`   | Current split divider rendering (thin `underline_height` line)       |
| `wezboard/wezboard-gui/src/termwindow/render/borders.rs` | Window border rendering — pattern to follow (4 rectangles)           |

### Content inset

Without insetting, the border (layer 2) paints over the outermost pixels of
terminal text (layer 0). Ghostboard solved this in Issue 672 by reducing the
content area by the border width on each interior edge.

In WezTerm, pane content positioning is controlled by two values in
`paint_pane`:

- **`left_pixel_x`** (line 340) — horizontal start of text rendering, computed
  from `padding_left + border.left + (pos.left * cell_width)`.
- **`top_pixel_y`** (line 78) — vertical start, computed from
  `top_bar_height + padding_top + border.top`.

The `background_rect` (lines 110-152) has edge detection logic: it checks
`pos.left == 0`, `pos.top == 0`, and whether `pos.left + pos.width` reaches the
terminal's column count to decide whether to extend to the window edge.

The inset should only apply on **interior edges** — edges where panes meet other
panes. Window-edge panes don't need inset on the side touching the window frame,
since the window's own padding already provides separation. The edge detection
logic in `background_rect` already identifies which edges are interior vs
window-edge.

### Approach

1. Add three config fields: `focused_split_border_color` (Option<RgbaColor>),
   `unfocused_split_border_color` (Option<RgbaColor>), `split_border_width`
   (Dimension, default 0).
2. Add a `paint_pane_border` method that draws 4 filled rectangles around each
   pane's `background_rect`, choosing color based on `pos.is_active`.
3. Inset pane content by adjusting `left_pixel_x`, `top_pixel_y`, and
   `background_rect` in `paint_pane` — shift text rendering inward by
   `border_width` on interior edges so the border doesn't cover terminal text.
4. Call `paint_pane_border` from the paint loop after `paint_pane`. When
   `split_border_width > 0`, skip `paint_split` since borders replace dividers.
5. Skip borders when there's only one visible pane (single pane or zoomed).

## Experiments

### Experiment 1: Config, border rendering, and content inset

Add three config fields, draw per-pane borders on layer 2, inset pane content so
borders don't cover terminal text, and conditionally skip the old split divider.

#### Changes

**1. `wezboard/config/src/config.rs`** — Add three fields after
`inactive_pane_hsb` (line 621):

```rust
#[dynamic(default)]
pub focused_split_border_color: Option<RgbaColor>,

#[dynamic(default)]
pub unfocused_split_border_color: Option<RgbaColor>,

#[dynamic(try_from = "crate::units::PixelUnit", default = "default_zero_pixel")]
pub split_border_width: Dimension,
```

Import `Dimension` from `crate::units` and `default_zero_pixel` from
`crate::color` (make it `pub` in `color.rs` if it isn't already).

**2. `wezboard/config/src/color.rs`** — Make `default_zero_pixel` public (line
611):

```rust
pub const fn default_zero_pixel() -> Dimension {
```

**3. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Two changes:

**(a) Add `paint_pane_border` method** on `impl crate::TermWindow`. Draws 4
filled rectangles around each pane's `background_rect`:

```rust
pub fn paint_pane_border(
    &mut self,
    pos: &PositionedPane,
    layers: &mut TripleLayerQuadAllocator,
    num_panes: usize,
) -> anyhow::Result<()>
```

Logic:

- Early return if `num_panes <= 1` or `pos.is_zoomed`.
- Evaluate `self.config.split_border_width` as pixels using `DimensionContext`
  (same pattern as `padding_left_top`). Early return if 0.
- Pick color based on `pos.is_active`:
  - Active: `self.config.focused_split_border_color.map(|c| c.to_linear())`
    falling back to `palette.split.to_linear()`.
  - Inactive: `self.config.unfocused_split_border_color.map(|c| c.to_linear())`
    falling back to `palette.split.to_linear()`.
- Compute `background_rect` using the same logic as `paint_pane` (lines
  110-152). This needs the same `padding_left`, `padding_top`, `top_pixel_y`,
  `border`, `cell_width`, `cell_height` setup.
- Draw 4 rectangles on layer 2 via `self.filled_rectangle`:
  - Top: `rect(x, y, width, bw)`
  - Bottom: `rect(x, y + height - bw, width, bw)`
  - Left: `rect(x, y, bw, height)`
  - Right: `rect(x + width - bw, y, bw, height)`

**(b) Inset content in `paint_pane`** — When `split_border_width > 0` and
`num_panes > 1` (and not zoomed), adjust `background_rect` and `left_pixel_x` to
push content inward on interior edges:

- `paint_pane` signature changes to accept `num_panes: usize`.
- After computing `background_rect` (line 152), if borders are active:
  - On interior left edge (`pos.left != 0`): shift `background_rect.origin.x`
    right by `bw`, reduce width by `bw`, add `bw` to `left_pixel_x`.
  - On interior top edge (`pos.top != 0`): shift `background_rect.origin.y` down
    by `bw`, reduce height by `bw`. The `top_pixel_y` used for line rendering is
    per-pane, so add `bw` to it.
  - On interior right edge (`pos.left + pos.width < self.terminal_size.cols`):
    reduce width by `bw`.
  - On interior bottom edge (`pos.top + pos.height < self.terminal_size.rows`):
    reduce height by `bw`.

**4. `wezboard/wezboard-gui/src/termwindow/render/paint.rs`** — Three changes in
`paint_pass()`:

**(a)** Capture `num_panes` before the pane loop (line 249):

```rust
let num_panes = panes.len();
```

**(b)** Update `paint_pane` call (line 257) and add `paint_pane_border`:

```rust
self.paint_pane(&pos, &mut layers, num_panes).context("paint_pane")?;
self.paint_pane_border(&pos, &mut layers, num_panes)?;
```

**(c)** Conditionally skip `paint_split` (lines 260-266) — only run when
`split_border_width` evaluates to 0:

```rust
let split_border_width = self.config.split_border_width.evaluate_as_pixels(...);
if split_border_width == 0. {
    if let Some(pane) = self.get_active_pane_or_overlay() {
        // ... existing paint_split loop ...
    }
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Add to `~/.wezterm.lua`:
   ```lua
   config.focused_split_border_color = "#7dcfff"
   config.unfocused_split_border_color = "#565f89"
   config.split_border_width = 2
   ```
3. Launch, create a horizontal split (`Ctrl+Shift+"`), verify:
   - Active pane has blue (`#7dcfff`) border
   - Inactive pane has gray (`#565f89`) border
   - Borders are 2px wide
   - Terminal text is not obscured by borders (content inset works)
   - Switching focus updates border colors immediately
   - Old thin split divider is not drawn
4. Create a vertical split (`Ctrl+Shift+%`), verify borders on all 3+ panes
5. Single pane — no borders drawn
6. Remove config options — original thin divider behavior restored
7. Zoom a pane (`Ctrl+Shift+Z`) — borders disappear while zoomed

**Result:** Partial

Border rendering works correctly: focused/unfocused colors apply, borders draw
at the configured width on layer 2, `paint_split` is skipped when
`split_border_width > 0`, and single-pane windows have no borders. However, the
content inset does not work — borders paint over pane content instead of pushing
it inward. The `background_rect`, `left_pixel_x`, and `top_pixel_y` adjustments
in `paint_pane` do not effectively prevent the border from covering terminal
text.

#### Conclusion

The border drawing infrastructure is solid. The content inset approach needs
rethinking — adjusting `background_rect` and pixel offsets inside `paint_pane`
is not sufficient to push the rendered terminal lines away from the border
region. Experiment 2 should focus on fixing the content inset so borders don't
obscure text.

### Experiment 2: Fix content inset via pixel_width reduction

Experiment 1's inset adjusted `background_rect`, `left_pixel_x`, and
`top_pixel_y` but missed a critical parameter: `pixel_width`. The
`render_screen_line` function receives `pixel_width` (currently
`dims.cols * cell_width` — the full pane width) and uses it to construct the
`bounding_rect` that clips background fills. Text glyphs are positioned relative
to `left_pixel_x` and `top_pixel_y`, which Experiment 1 already adjusts
correctly. The missing piece is that `pixel_width` still spans the full pane, so
background fills extend under the border on the right side, and there is no
signal to constrain rendering width.

The fix: reduce `pixel_width` by the horizontal border insets (left + right
interior edges). This mirrors how `window_padding` works — it reduces available
space rather than adding clipping.

#### Changes

**1. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Two changes:

**(a) Add `pixel_width` field to `LineRender` struct** (after `left_pixel_x`):

```rust
pixel_width: f32,
```

**(b) Compute `pixel_width` with border inset** — After `left_pixel_x` and
`inset_top_pixel_y` (around line 380), compute the inset pixel width:

```rust
let pixel_width = {
    let full = self.render_metrics.cell_size.width as f32 * dims.cols as f32;
    if border_width > 0.0 {
        let left_inset = if pos.left != 0 { border_width } else { 0.0 };
        let right_inset = if pos.left + pos.width
            < self.terminal_size.cols as usize
        {
            border_width
        } else {
            0.0
        };
        full - left_inset - right_inset
    } else {
        full
    }
};
```

Initialize the field in `LineRender`:

```rust
pixel_width,
```

**(c) Use `self.pixel_width` in `render_screen_line` call** — Replace the inline
`pixel_width` computation (line 534-535):

```rust
// Before:
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32,
// After:
pixel_width: self.pixel_width,
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch with border config, create splits, verify:
   - Terminal text does not extend under the border on any edge
   - Background fills stop at the border boundary
   - Content is visually inset from the border on all interior edges
3. Single pane — no change in behavior (no borders, no inset)
4. Zoom a pane — no borders, full content area restored

**Result:** Fail

The `pixel_width` reduction compiled cleanly but did not fix the content inset
visually. Reducing `pixel_width` passed to `render_screen_line` is not
sufficient to prevent terminal content from rendering under the border region.
The approach of constraining `pixel_width` does not achieve the desired
clipping/inset effect.

#### Conclusion

The `pixel_width` parameter alone does not control where background fills and
text are drawn relative to the border. A different approach is needed — possibly
adjusting `dims.cols` to reduce the number of rendered columns, or applying
actual clipping in the renderer, or modifying how `render_screen_line` uses
`pixel_width` internally.

### Experiment 3: Constrain visible rows + pixel_width + skip bottom lines

Experiments 1 and 2 each addressed part of the problem but missed the full
picture. The rendering pipeline has multiple independent content bounds that ALL
must be constrained simultaneously:

1. **`background_rect`** (layer 0 pane fill) — Exp 1 insets this. Works.
2. **`left_pixel_x`** — Exp 1 shifts this right on interior left edges. Works.
3. **`top_pixel_y`** (via `inset_top_pixel_y`) — Exp 1 shifts this down on
   interior top edges. Works.
4. **`pixel_width`** (horizontal clipping in `render_screen_line`) — Exp 2
   reduced this. Clips backgrounds via `bounding_rect.intersection()` and
   soft-clips glyphs via `pos_x > pixel_width` at `screen_line.rs:523`. Should
   work horizontally.
5. **Vertical extent** — **Never addressed.** The number of rendered lines is
   `dims.viewport_rows`, unchanged. Lines at the bottom still render under the
   bottom border. There is no `pixel_height` constraint.

The fix combines all three dimensions: horizontal (`pixel_width` reduction from
Exp 2), vertical top (already working `inset_top_pixel_y` from Exp 1), and
vertical bottom (skip lines that fall in the bottom border region).

#### Changes

**1. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Three changes:

**(a) Add `pixel_width` and `bottom_pixel_y` fields to `LineRender` struct**
(after `left_pixel_x`):

```rust
pixel_width: f32,
bottom_pixel_y: f32,
```

**(b) Compute `pixel_width` and `bottom_pixel_y` with border inset** — After
`inset_top_pixel_y` (around line 382), compute both values:

```rust
let cell_width = self.render_metrics.cell_size.width as f32;
let full_pixel_width = cell_width * dims.cols as f32;
let mut pixel_width = full_pixel_width;
if border_width > 0.0 && pos.left != 0 {
    pixel_width -= border_width;
}
if border_width > 0.0
    && pos.left + pos.width < self.terminal_size.cols as usize
{
    pixel_width -= border_width;
}

let bottom_pixel_y = if border_width > 0.0
    && pos.top + pos.height < self.terminal_size.rows as usize
{
    inset_top_pixel_y
        + (dims.viewport_rows as f32 * cell_height)
        - border_width
} else {
    f32::MAX
};
```

Initialize both fields in `LineRender`:

```rust
pixel_width,
bottom_pixel_y,
```

**(c) Use `self.pixel_width` in `render_screen_line` call** — Replace the inline
computation (line 534-535):

```rust
// Before:
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32,
// After:
pixel_width: self.pixel_width,
```

**(d) Skip lines that fall in the bottom border region** — In `render_line`,
early return when the line's top_pixel_y would place it in the bottom border:

```rust
let line_top = self.top_pixel_y
    + (line_idx + self.pos.top) as f32
        * self.term_window.render_metrics.cell_size.height as f32;
if line_top >= self.bottom_pixel_y {
    return Ok(());
}
```

Add this at the start of `render_line`, before the stable_row computation.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch with border config, create splits, verify:
   - Terminal text does not extend under the border on any edge
   - Background fills stop at the border boundary on left and right
   - Bottom rows don't render under the bottom border
   - Content is visually inset from the border on all interior edges
3. Single pane — no change in behavior (no borders, no inset)
4. Zoom a pane — no borders, full content area restored
