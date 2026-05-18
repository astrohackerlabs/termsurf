+++
status = "closed"
opened = "2026-03-07"
closed = "2026-03-07"
+++

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

### Experiment 3: Reduce cell count upstream via resize

Experiments 1 and 2 tried to fix the inset downstream in the renderer — shifting
pixel offsets, reducing `pixel_width`, adjusting `background_rect`. None worked
because the terminal still thinks it has N columns and M rows and renders all of
them. The renderer is the wrong place to fix this.

Window padding works by subtracting padding pixels from available space BEFORE
computing cell count in `resize.rs`. The terminal simply gets fewer cols/rows,
and content can never overflow because the cells don't exist. Pane borders
should work the same way.

All panes always have a border (even single-pane windows). Subtract
`border_width * 2` (left + right, top + bottom) from available space before
computing cell count.

Also remove all downstream inset code from Experiment 1 (`background_rect`
adjustments, `left_pixel_x` shift, `inset_top_pixel_y` shift) since the upstream
fix makes them unnecessary and they would double-count.

#### Changes

**1. `wezboard/wezboard-gui/src/termwindow/resize.rs`** — Subtract border pixels
from available space in both branches of `apply_dimensions`.

In the `else` branch (window resize, ~line 250):

```rust
let split_border_pixels = config
    .split_border_width
    .evaluate_as_pixels(h_context) as usize;

let avail_width = dimensions.pixel_width.saturating_sub(
    (padding_left + padding_right) as usize
        + (border.left + border.right).get() as usize
        + split_border_pixels * 2,
);
let avail_height = dimensions
    .pixel_height
    .saturating_sub(
        (padding_top + padding_bottom) as usize
            + (border.top + border.bottom).get() as usize
            + split_border_pixels * 2,
    )
    .saturating_sub(tab_bar_height as usize);
```

In the `if` branch (explicit size, ~line 204), add `split_border_pixels * 2` to
the `pixel_height` and `pixel_width` computations so the window requests enough
pixels to fit both cells and borders.

**2. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Remove Experiment
1's downstream inset code:

**(a)** Remove the `border_width` computation and `background_rect` inset block
(lines 155-187). Replace with just `let border_width = 0.0_f32;` or remove
references entirely.

**(b)** Remove the `border_width` conditional from `left_pixel_x` (line 378) —
revert to:

```rust
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32);
```

**(c)** Remove `inset_top_pixel_y` (line 380-381) — use `top_pixel_y` directly
in the `LineRender` initializer.

**(d)** Remove `num_panes` parameter from `paint_pane` signature and update the
call site in `paint.rs`.

**3. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** —
`paint_pane_border`: Remove the `num_panes <= 1` early return (line 630-632)
since all panes always get borders.

**4. `wezboard/wezboard-gui/src/termwindow/render/paint.rs`** — Remove
`num_panes` variable and update `paint_pane` call. Keep `paint_pane_border`
call. Remove the `num_panes` argument from `paint_pane_border` too.

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch with border config, single pane — border visible, text does not extend
   under border
3. Create splits — borders on all panes, text inset on all edges
4. Resize window — content reflows correctly, borders stay consistent
5. Remove `split_border_width` from config — original behavior restored (no
   borders, no space subtracted)

**Result:** Fail

The `split_border_pixels * 2` subtraction in `resize.rs` correctly reduced the
cell count — the terminal got fewer cols/rows. However, the border still
rendered on top of content with no visible padding between border and text.
Subtracting from available space before cell count computation reserves pixels
at the window level, but those reserved pixels are not allocated as inset space
around individual panes. The upstream resize approach treats the border space
like window padding (edge of window), not like per-pane inset (around each
pane's content area).

#### Conclusion

The window-padding analogy was flawed. Window padding works because it subtracts
from the total window dimensions before the single content area is laid out.
Pane borders need per-pane inset — each pane's content must be independently
inset from its own borders. Reducing the global cell count just makes all panes
smaller without creating gaps between content and borders. A different approach
is needed — possibly modifying how `PositionedPane` coordinates are computed in
the mux layer, or applying per-pane pixel offsets at the point where each pane's
line rendering begins.

### Experiment 4: Per-pane pixel inset on all edges

Experiments 1-2 applied insets only on **interior edges** (checking
`pos.left !=
0`, `pos.top != 0`, etc.), but the border draws on **all 4 sides**
of every pane. Content at window edges was never inset, so the border always
covered it there. Experiment 3 was the wrong abstraction level entirely (global
vs per-pane).

The fix: treat border width as per-pane padding applied **unconditionally on all
sides**. This is the CSS container-with-padding model — the border is the
container edge, and the content area shrinks inward by the border width.

Three rendering adjustments:

1. Add `bw` to `left_pixel_x` — content starts `bw` pixels right of pane edge
2. Add `bw` to `top_pixel_y` — content starts `bw` pixels below pane edge
3. Reduce `pixel_width` by `2 * bw` — content clips at right border edge

The `background_rect` is also inset by `bw` on all sides so the pane background
fill doesn't extend under the border.

Also revert Experiment 3's `resize.rs` changes since the global subtraction is
the wrong level.

#### Changes

**1. `wezboard/wezboard-gui/src/termwindow/resize.rs`** — Revert Experiment 3.
Remove `split_border_pixels` computation and its addition to `pixel_height`,
`pixel_width`, `avail_width`, and `avail_height` in both branches.

**2. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Three changes in
`paint_pane`:

**(a)** After computing `background_rect` (line 152), compute `bw` and inset the
rect on all sides:

```rust
let bw = self.config.split_border_width.evaluate_as_pixels(
    config::DimensionContext {
        dpi: self.dimensions.dpi as f32,
        pixel_max: self.dimensions.pixel_width as f32,
        pixel_cell: self.render_metrics.cell_size.width as f32,
    },
) as f32;
if bw > 0.0 && !pos.is_zoomed {
    background_rect.origin.x += bw;
    background_rect.origin.y += bw;
    background_rect.size.width -= bw * 2.0;
    background_rect.size.height -= bw * 2.0;
}
```

**(b)** Add `bw` to `left_pixel_x` and `top_pixel_y` (around line 340):

```rust
let left_pixel_x = padding_left
    + border.left.get() as f32
    + (pos.left as f32 * self.render_metrics.cell_size.width as f32)
    + if bw > 0.0 && !pos.is_zoomed { bw } else { 0.0 };
```

And use an inset `top_pixel_y` in the `LineRender` initializer:

```rust
top_pixel_y: top_pixel_y
    + if bw > 0.0 && !pos.is_zoomed { bw } else { 0.0 },
```

**(c)** Reduce `pixel_width` in the `render_screen_line` call (line 495-496):

```rust
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32
    - if self.term_window.config.split_border_width.evaluate_as_pixels(
        config::DimensionContext {
            dpi: self.term_window.dimensions.dpi as f32,
            pixel_max: self.term_window.dimensions.pixel_width as f32,
            pixel_cell: self.term_window.render_metrics.cell_size.width as f32,
        },
    ) > 0.0 && !self.pos.is_zoomed { bw * 2.0 } else { 0.0 },
```

Actually, cleaner: store `bw` in the `LineRender` struct and use it:

```rust
// In LineRender struct:
border_inset: f32,

// In LineRender initializer:
border_inset: if bw > 0.0 && !pos.is_zoomed { bw } else { 0.0 },

// In render_screen_line call:
pixel_width: self.dims.cols as f32
    * self.term_window.render_metrics.cell_size.width as f32
    - self.border_inset * 2.0,
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Launch with border config, single pane — border visible on all sides, text
   does not render under border
3. Create splits — borders on all panes, text inset on all edges
4. Resize window — content reflows correctly
5. Zoom a pane — no borders, full content area
6. Remove `split_border_width` from config — no borders, no inset

**Result:** Pass

Content inset works on all edges. The border draws on layer 2, the background
rect and text rendering are both inset by `bw` pixels on all sides, so the
border never covers content. The key difference from Experiments 1-2: the inset
applies unconditionally on **all four sides** (not just interior edges), and
`pixel_width` is reduced by `2 * bw` to clip the right side. Storing `bw` as
`border_inset` in the `LineRender` struct keeps the render_screen_line call
clean.

### Experiment 5: Hide borders for single-pane windows

Experiment 4 draws borders and insets content unconditionally — even when
there's only one pane visible. Borders only make sense when multiple panes are
on screen, since their purpose is to visually distinguish panes from each other.
A single pane should have no border and no content inset.

The paint loop in `paint.rs` already uses `panes.len()` for background color
selection (line 233: `if panes.len() == 1`). Re-add `num_panes` as a parameter
to `paint_pane` and `paint_pane_border` so they can skip border logic when
there's only one pane.

#### Changes

**1. `wezboard/wezboard-gui/src/termwindow/render/paint.rs`** — Two changes:

**(a)** Re-add `num_panes` before the pane loop:

```rust
let num_panes = panes.len();
```

**(b)** Pass `num_panes` to both calls:

```rust
self.paint_pane(&pos, &mut layers, num_panes)
    .context("paint_pane")?;
self.paint_pane_border(&pos, &mut layers, num_panes)?;
```

**2. `wezboard/wezboard-gui/src/termwindow/render/pane.rs`** — Two changes:

**(a)** In `paint_pane`, add `num_panes: usize` parameter. Gate `bw` on
`num_panes > 1`:

```rust
let bw = if num_panes > 1 && !pos.is_zoomed {
    self.config.split_border_width.evaluate_as_pixels(...) as f32
} else {
    0.0
};
```

**(b)** In `paint_pane_border`, add `num_panes: usize` parameter. Restore the
early return:

```rust
if num_panes <= 1 || pos.is_zoomed {
    return Ok(());
}
```

#### Verification

1. `cd wezboard && cargo build -p wezboard-gui` — zero errors
2. Single pane — no border, no content inset, full content area
3. Create a split — borders appear on both panes, content inset active
4. Close split pane — borders disappear, content returns to full area
5. Zoom a pane — borders disappear while zoomed

**Result:** Pass

Single-pane windows no longer draw borders or inset content. Splitting creates
borders on all panes, closing the split removes them, and zooming hides them.
The `num_panes` parameter gates both `bw` in `paint_pane` and the early return
in `paint_pane_border`.

#### Conclusion

Borders now correctly appear only when multiple panes are visible. Combined with
Experiment 4's per-pane inset, the border system is complete: configurable
focused/unfocused colors, configurable width, content inset on all edges, and
automatic hide for single-pane and zoomed states.

## Conclusion

Wezboard now has configurable focused/unfocused split pane borders with content
inset, matching the Ghostboard feature set. Three config keys control the
behavior: `focused_split_border_color`, `unfocused_split_border_color`, and
`split_border_width`. When `split_border_width > 0`, the old thin split divider
is skipped and per-pane borders are drawn instead.

### What worked

- **Experiment 1** established the foundation: three config fields, a
  `paint_pane_border` method drawing 4 filled rectangles per pane on layer 2,
  and conditional skipping of `paint_split`. Border rendering, color selection,
  and the config integration all worked on the first attempt.
- **Experiment 4** solved the content inset by treating the border as a CSS-like
  container with padding on **all four sides** unconditionally. Three changes
  made it work: adding `bw` to `left_pixel_x`, adding `bw` to `top_pixel_y`, and
  reducing `pixel_width` by `2 * bw` via a `border_inset` field on the
  `LineRender` struct. The `background_rect` was also inset by `bw` on all
  sides.
- **Experiment 5** added `num_panes` gating so single-pane windows and zoomed
  panes have no borders and no content inset.

### What didn't work

- **Experiment 1's content inset** only applied on interior edges (checking
  `pos.left != 0`, `pos.top != 0`, etc.), but the border draws on all 4 sides.
  Content at window edges was never inset, so the border covered it there.
- **Experiment 2** tried reducing `pixel_width` alone (without the other
  adjustments from Experiment 4). Constraining just `pixel_width` passed to
  `render_screen_line` was not sufficient — background fills and text
  positioning also needed adjustment.
- **Experiment 3** tried the wrong abstraction level entirely: subtracting
  border pixels from the global window dimensions in `resize.rs` before
  computing cell count. This treated borders like window padding (edge of
  window) rather than per-pane inset (around each pane's content area). The
  terminal got fewer cols/rows but no gap appeared between content and borders.

### Key insight

The critical realization was that content inset must apply **unconditionally on
all four sides** of every pane, not selectively on interior edges. Earlier
experiments failed because they tried to be clever about which edges needed
inset. The simple approach — treat every pane as a container with uniform
padding equal to the border width — was the correct one.
