#![allow(dead_code)]
// Cell codepoint classification is consumed by later renderer slices.

//! Renderer cell data and codepoint classification.
//!
//! Faithful port of upstream `renderer/cell.zig`: the codepoint-classification
//! predicates, `constraint_width`, and the `Contents` cell-render-data builder
//! (storage, cursor lists, and row mutation). `cell.zig` is fully ported.
//!
//! [`add_glyph`] is the emit half of upstream `renderer/generic.zig`'s `addGlyph`
//! (render a shaped glyph through the [`SharedGrid`] into a `Contents` text
//! cell). It is co-located with `Contents` here because the generic renderer that
//! owns it upstream is not ported yet.

use super::cursor::Style as CursorStyle;
use super::shader::{CellBg, CellTextAtlas, CellTextFlags, CellTextVertex};
use super::size::GridSize;
use crate::font::codepoint_resolver::ResolverRenderError;
use crate::font::collection::Index;
use crate::font::face::constraint::{Constraint, Size};
use crate::font::face::coretext::RenderOptions;
use crate::font::face::nerd_font_attributes::get_constraint;
use crate::font::metrics::Metrics;
use crate::font::run::{RunCell, ShapedRun, Wide};
use crate::font::shape;
use crate::font::shared_grid::SharedGrid;
use crate::font::Presentation;

/// True only for U+2588 FULL BLOCK.
pub(crate) fn is_covering(cp: u32) -> bool {
    cp == 0x2588
}

/// Whether minimum-contrast adjustment should be disabled for a glyph. True for
/// graphics elements such as block elements and Powerline glyphs.
pub(crate) fn no_min_contrast(cp: u32) -> bool {
    is_graphics_element(cp)
}

/// True if the codepoint is used for terminal graphics: box drawing, block
/// elements, legacy computing, or Powerline glyphs.
fn is_graphics_element(cp: u32) -> bool {
    is_box_drawing(cp) || is_block_element(cp) || is_legacy_computing(cp) || is_powerline(cp)
}

/// True if the codepoint is a box drawing character.
fn is_box_drawing(cp: u32) -> bool {
    matches!(cp, 0x2500..=0x257F)
}

/// True if the codepoint is a block element.
fn is_block_element(cp: u32) -> bool {
    matches!(cp, 0x2580..=0x259F)
}

/// True if the codepoint is in a Symbols for Legacy Computing block, including
/// the Unicode 16.0 supplement.
fn is_legacy_computing(cp: u32) -> bool {
    matches!(cp, 0x1FB00..=0x1FBFF | 0x1CC00..=0x1CEBF)
}

/// True if the codepoint is part of the Powerline range.
fn is_powerline(cp: u32) -> bool {
    matches!(cp, 0xE0B0..=0xE0D7)
}

/// Some general spaces, kept to force the font to render as a fixed width.
fn is_space(cp: u32) -> bool {
    matches!(cp, 0x0020 | 0x2002)
}

/// True if the codepoint is "symbol-like". Faithful to upstream's generated
/// `is_symbol` table, whose membership is defined in `uucode_config.zig` as the
/// Private-Use general category plus eight named Unicode blocks. Unicode block
/// membership is range-based (including unassigned codepoints inside a block),
/// so this is byte-for-byte identical to the generated table.
pub(crate) fn is_symbol(cp: u32) -> bool {
    is_private_use(cp)
        || matches!(cp,
            0x2190..=0x21FF      // Arrows
            | 0x2700..=0x27BF    // Dingbats
            | 0x1F600..=0x1F64F  // Emoticons
            | 0x2600..=0x26FF    // Miscellaneous Symbols
            | 0x2460..=0x24FF    // Enclosed Alphanumerics
            | 0x1F100..=0x1F1FF  // Enclosed Alphanumeric Supplement
            | 0x1F300..=0x1F5FF  // Miscellaneous Symbols and Pictographs
            | 0x1F680..=0x1F6FF  // Transport and Map Symbols
        )
}

/// True for the Private-Use general category (`Co`). The supplementary planes
/// stop at `..FFFD`; the last two code points of each plane are noncharacters
/// (`Cn`), not Private-Use.
fn is_private_use(cp: u32) -> bool {
    matches!(cp, 0xE000..=0xF8FF | 0xF0000..=0xFFFFD | 0x100000..=0x10FFFD)
}

/// The per-cell data `constraint_width` reads from a row of cells: a codepoint
/// and a grid width. The renderer maps its real cell source into this view at
/// the call site (a faithful adaptation of upstream operating on
/// `[]const terminal.page.Cell`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellInfo {
    pub codepoint: u32,
    pub grid_width: u8,
}

/// Returns the appropriate constraint width for the cell at `x` when rendering
/// its glyph(s). Symbol-like glyphs may extend to two cells when there is room
/// and the previous glyph was not also a non-graphics symbol.
///
/// `x` must be `< cols` and `raw_slice` must have at least `cols` entries;
/// `x + 1` is read only when `x != cols - 1`, matching upstream's access bounds.
pub(crate) fn constraint_width(raw_slice: &[CellInfo], x: usize, cols: usize) -> u8 {
    let cell = raw_slice[x];
    let cp = cell.codepoint;
    let grid_width = cell.grid_width;

    // If the grid width of the cell is 2, the constraint width is always 2.
    if grid_width > 1 {
        return grid_width;
    }

    // Only "symbol-like" glyphs may extend to 2 cells; others use the grid
    // width.
    if !is_symbol(cp) {
        return grid_width;
    }

    // At the end of the screen it must be constrained to one cell.
    if x == cols - 1 {
        return 1;
    }

    // If the previous cell was a symbol (but not a graphics element such as a
    // block element or Powerline glyph), constrain so multiple PUA glyphs align.
    if x > 0 {
        let prev_cp = raw_slice[x - 1].codepoint;
        if is_symbol(prev_cp) && !is_graphics_element(prev_cp) {
            return 1;
        }
    }

    // If the next cell is whitespace, allow the glyph to be up to two cells.
    let next_cp = raw_slice[x + 1].codepoint;
    if next_cp == 0 || is_space(next_cp) {
        return 2;
    }

    // Otherwise, this has to be 1 cell wide.
    1
}

/// The grid width of a cell from its [`Wide`] kind — upstream `Cell.gridWidth()`:
/// a wide cell spans two columns, everything else (narrow, spacer head/tail) one.
fn grid_width(wide: Wide) -> u8 {
    match wide {
        Wide::Wide => 2,
        Wide::Narrow | Wide::SpacerHead | Wide::SpacerTail => 1,
    }
}

/// Map a row's decoded [`RunCell`]s to the [`CellInfo`] slice the render options
/// read (each column's codepoint and grid width). The `CellInfo` half of the
/// per-row inputs the future `rebuildCells` feeds to [`add_run`].
pub(crate) fn cell_infos(cells: &[RunCell]) -> Vec<CellInfo> {
    cells
        .iter()
        .map(|cell| CellInfo {
            codepoint: cell.codepoint,
            grid_width: grid_width(cell.wide),
        })
        .collect()
}

/// Identifies which GPU buffer a cell belongs to. Conceptually maps to a cell
/// type (upstream `Key.CellType`): `Bg` → `CellBg`; the foreground kinds
/// (`Text`/`Underline`/`Strikethrough`/`Overline`) → `CellTextVertex`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Key {
    Bg,
    Text,
    Underline,
    Strikethrough,
    Overline,
}

/// The contents of all the cells in the terminal.
///
/// Holds per-cell GPU data and supports row-wise clearing/dirty tracking so the
/// GPU buffers need not be rebuilt each frame. Must be initialized by calling
/// `resize` before any other operation.
#[derive(Default)]
pub(crate) struct Contents {
    size: GridSize,

    /// Flat array of cell background colors, indexed
    /// `bg_cells[row * columns + col]`. Prefer `bg_cell`/`bg_cell_mut` over
    /// direct indexing to centralize the index arithmetic.
    bg_cells: Vec<CellBg>,

    /// Foreground cells per row. `fg_rows[0]` and `fg_rows[rows + 1]` are
    /// reserved for the cursor (it must be first and last in the GPU buffer);
    /// real rows are `fg_rows[1..=rows]`. Upstream uses an `ArrayListCollection`;
    /// a `Vec<Vec<_>>` with `Vec::clear` (which retains capacity) is the analog.
    fg_rows: Vec<Vec<CellTextVertex>>,
}

impl Contents {
    /// Resize the cell contents for the given grid size. This always invalidates
    /// the entire cell contents.
    pub(crate) fn resize(&mut self, size: GridSize) {
        let columns = size.columns as usize;
        let rows = size.rows as usize;

        let bg_cells = vec![CellBg([0, 0, 0, 0]); columns * rows];

        // `rows + 2` lists: indices 0 and `rows + 1` are the cursor-reserved
        // lists (small), and `1..=rows` are the real rows. Real rows get
        // capacity `columns * 3` (a glyph + underline + strikethrough per
        // column) to avoid reallocation in the common case.
        let mut fg_rows: Vec<Vec<CellTextVertex>> = Vec::with_capacity(rows + 2);
        for i in 0..rows + 2 {
            let capacity = if i == 0 || i == rows + 1 {
                1
            } else {
                columns * 3
            };
            fg_rows.push(Vec::with_capacity(capacity));
        }

        // Commit the new buffers and size together: no window of half-updated
        // state.
        self.size = size;
        self.bg_cells = bg_cells;
        self.fg_rows = fg_rows;
    }

    /// Reset the cell contents to an empty state without resizing.
    pub(crate) fn reset(&mut self) {
        for cell in &mut self.bg_cells {
            *cell = CellBg([0, 0, 0, 0]);
        }
        for list in &mut self.fg_rows {
            list.clear();
        }
    }

    /// Access a background cell. Prefer this over direct indexing of `bg_cells`.
    pub(crate) fn bg_cell(&self, row: usize, col: usize) -> &CellBg {
        &self.bg_cells[row * self.size.columns as usize + col]
    }

    /// Mutably access a background cell.
    pub(crate) fn bg_cell_mut(&mut self, row: usize, col: usize) -> &mut CellBg {
        &mut self.bg_cells[row * self.size.columns as usize + col]
    }

    /// Set the cursor value. A `None` value hides the cursor. Block cursors are
    /// stored in the first reserved list (drawn first); other styles go in the
    /// last reserved list (drawn last).
    pub(crate) fn set_cursor(
        &mut self,
        v: Option<CellTextVertex>,
        cursor_style: Option<CursorStyle>,
    ) {
        if self.size.rows == 0 {
            return;
        }
        let last = self.size.rows as usize + 1;
        self.fg_rows[0].clear();
        self.fg_rows[last].clear();

        let Some(cell) = v else {
            return;
        };
        let Some(style) = cursor_style else {
            return;
        };

        match style {
            // Block cursors are drawn first.
            CursorStyle::Block => self.fg_rows[0].push(cell),
            // Other cursor styles are drawn last.
            CursorStyle::BlockHollow
            | CursorStyle::Bar
            | CursorStyle::Underline
            | CursorStyle::Lock => self.fg_rows[last].push(cell),
        }
    }

    /// Returns the current cursor glyph if present, checking both cursor lists.
    pub(crate) fn get_cursor_glyph(&self) -> Option<CellTextVertex> {
        if self.size.rows == 0 {
            return None;
        }
        let last = self.size.rows as usize + 1;
        if !self.fg_rows[0].is_empty() {
            return Some(self.fg_rows[0][0]);
        }
        if !self.fg_rows[last].is_empty() {
            return Some(self.fg_rows[last][0]);
        }
        None
    }

    /// Add a foreground cell to the appropriate row list. Adding the same cell
    /// twice duplicates it in the vertex buffer; clear the row first with
    /// `clear`. Background cells use `bg_cell_mut`, never `add`.
    pub(crate) fn add(&mut self, key: Key, cell: CellTextVertex) {
        let y = cell.grid_pos[1];
        assert!(y < self.size.rows);
        match key {
            Key::Bg => unreachable!("background cells use bg_cell_mut, not add"),
            // The `+ 1` skips the reserved cursor list at index 0.
            Key::Text | Key::Underline | Key::Strikethrough | Key::Overline => {
                self.fg_rows[y as usize + 1].push(cell);
            }
        }
    }

    /// Clear all cell contents for a given row.
    pub(crate) fn clear(&mut self, y: u16) {
        assert!(y < self.size.rows);
        let columns = self.size.columns as usize;
        let start = y as usize * columns;
        for cell in &mut self.bg_cells[start..start + columns] {
            *cell = CellBg([0, 0, 0, 0]);
        }
        // The `+ 1` skips the reserved cursor list at index 0.
        self.fg_rows[y as usize + 1].clear();
    }
}

/// Build the [`RenderOptions`] for the glyph at column `x`, exactly as upstream
/// `addGlyph` does: the grid metrics and thicken config, the cell's grid width,
/// the constraint (Nerd Font lookup → else symbol `Fit` → else none), and the
/// symbol-aware [`constraint_width`]. The caller (the future `rebuildCells`)
/// supplies the row's [`CellInfo`] slice and the grid/thicken config.
pub(crate) fn render_options(
    grid_metrics: Metrics,
    raw_slice: &[CellInfo],
    x: usize,
    cols: usize,
    thicken: bool,
    thicken_strength: u8,
) -> RenderOptions {
    let cell = raw_slice[x];
    let cp = cell.codepoint;

    // Nerd Font constraint, else a symbol fits its cell, else no constraint.
    let constraint = get_constraint(cp).unwrap_or_else(|| {
        if is_symbol(cp) {
            Constraint {
                size: Size::Fit,
                ..Constraint::default()
            }
        } else {
            Constraint::default() // `.none`
        }
    });

    RenderOptions {
        grid_metrics,
        cell_width: Some(cell.grid_width),
        constraint,
        constraint_width: constraint_width(raw_slice, x, cols),
        thicken,
        thicken_strength,
    }
}

/// Render one shaped glyph through `grid` and add it to `contents` as a text
/// [`CellTextVertex`] at `grid_pos`. Invisible glyphs (0 width/height) are
/// skipped. Faithful port of the emit half of upstream `addGlyph`: the atlas
/// comes from the render's presentation, and the bearings sum the glyph's own
/// bearings and the shaper cell's per-glyph offsets. (`opts`, `color`/`alpha`,
/// and `no_min_contrast` are derived by the caller — the future `rebuildCells`.)
pub(crate) fn add_glyph(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    font_index: Index,
    shaper_cell: &shape::Cell,
    color: [u8; 3],
    alpha: u8,
    no_min_contrast: bool,
    opts: &RenderOptions,
) -> Result<(), ResolverRenderError> {
    let render = grid.render_glyph(font_index, shaper_cell.glyph_index, opts)?;

    // A 0-size glyph (e.g. a space) is invisible — don't add it to the buffer.
    if render.glyph.width == 0 || render.glyph.height == 0 {
        return Ok(());
    }

    // The glyph's own bearings plus the shaper's per-glyph offsets.
    let bearings = [
        i16::try_from(render.glyph.offset_x + i32::from(shaper_cell.x_offset))
            .expect("glyph x bearing fits i16"),
        i16::try_from(render.glyph.offset_y + i32::from(shaper_cell.y_offset))
            .expect("glyph y bearing fits i16"),
    ];

    contents.add(
        Key::Text,
        CellTextVertex {
            glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
            glyph_size: [render.glyph.width, render.glyph.height],
            bearings,
            grid_pos,
            color: [color[0], color[1], color[2], alpha],
            atlas: match render.presentation {
                Presentation::Emoji => CellTextAtlas::Color,
                Presentation::Text => CellTextAtlas::Grayscale,
            },
            flags: CellTextFlags::new(no_min_contrast, false),
            _padding: [0, 0],
        },
    );
    Ok(())
}

/// Place every glyph of one [`ShapedRun`] into `contents` on row `y`. For each
/// shaped cell, the absolute column is `run.offset + cell.x`; its
/// [`RenderOptions`] come from [`render_options`] over `row_cells`, its
/// color/alpha from `fg_colors[col]`, and its `no_min_contrast` from the cell's
/// codepoint. The per-run inner loop of upstream `rebuildCells`.
pub(crate) fn add_run(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    y: u16,
    run: &ShapedRun,
    row_cells: &[CellInfo],
    fg_colors: &[[u8; 4]],
    cols: usize,
    thicken: bool,
    thicken_strength: u8,
) -> Result<(), ResolverRenderError> {
    let grid_metrics = grid.metrics;
    for cell in &run.glyphs {
        let col = usize::from(run.run.offset) + usize::from(cell.x);
        debug_assert!(col < cols && cols <= row_cells.len() && cols <= fg_colors.len());
        // Checked, like upstream's `@intCast` (and the bearings in `add_glyph`).
        let grid_x = u16::try_from(col).expect("glyph column fits u16");
        let opts = render_options(
            grid_metrics,
            row_cells,
            col,
            cols,
            thicken,
            thicken_strength,
        );
        let cp = row_cells[col].codepoint;
        let rgba = fg_colors[col];
        add_glyph(
            contents,
            grid,
            [grid_x, y],
            run.run.font_index,
            cell,
            [rgba[0], rgba[1], rgba[2]],
            rgba[3],
            no_min_contrast(cp),
            &opts,
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_box_drawing_bounds() {
        assert!(!is_box_drawing(0x24FF));
        assert!(is_box_drawing(0x2500));
        assert!(is_box_drawing(0x257F));
        assert!(!is_box_drawing(0x2580));
    }

    #[test]
    fn is_block_element_bounds() {
        assert!(!is_block_element(0x257F));
        assert!(is_block_element(0x2580));
        assert!(is_block_element(0x259F));
        assert!(!is_block_element(0x25A0));
    }

    #[test]
    fn is_legacy_computing_bounds() {
        assert!(!is_legacy_computing(0x1FAFF));
        assert!(is_legacy_computing(0x1FB00));
        assert!(is_legacy_computing(0x1FBFF));
        assert!(!is_legacy_computing(0x1FC00));

        assert!(!is_legacy_computing(0x1CBFF));
        assert!(is_legacy_computing(0x1CC00));
        assert!(is_legacy_computing(0x1CEBF));
        assert!(!is_legacy_computing(0x1CEC0));
    }

    #[test]
    fn is_powerline_bounds() {
        assert!(!is_powerline(0xE0AF));
        assert!(is_powerline(0xE0B0));
        assert!(is_powerline(0xE0D7));
        assert!(!is_powerline(0xE0D8));
    }

    #[test]
    fn is_graphics_element_covers_each_block() {
        assert!(is_graphics_element(0x2500)); // box drawing
        assert!(is_graphics_element(0x2580)); // block element
        assert!(is_graphics_element(0x1FB00)); // legacy computing
        assert!(is_graphics_element(0x1CC00)); // legacy computing supplement
        assert!(is_graphics_element(0xE0B0)); // powerline
        assert!(!is_graphics_element('a' as u32));
    }

    #[test]
    fn is_covering_only_full_block() {
        assert!(is_covering(0x2588));
        // Both neighbors are still inside the block-element range, proving
        // `is_covering` is U+2588-only and not a range.
        assert!(!is_covering(0x2587));
        assert!(!is_covering(0x2589));
    }

    #[test]
    fn no_min_contrast_matches_graphics() {
        assert!(no_min_contrast(0x2500));
        assert!(!no_min_contrast('a' as u32));
    }

    #[test]
    fn is_space_fixed_width() {
        assert!(is_space(0x0020));
        assert!(is_space(0x2002));
        assert!(!is_space(0x2003));
        assert!(!is_space('a' as u32));
    }

    #[test]
    fn is_symbol_private_use() {
        // BMP Private Use Area.
        assert!(!is_symbol(0xDFFF));
        assert!(is_symbol(0xE000));
        assert!(is_symbol(0xF8FF));
        assert!(!is_symbol(0xF900));

        // Plane 15 Supplementary PUA-A, excluding the plane noncharacters.
        assert!(!is_symbol(0xEFFFF));
        assert!(is_symbol(0xF0000));
        assert!(is_symbol(0xFFFFD));
        assert!(!is_symbol(0xFFFFE));

        // Plane 16 Supplementary PUA-B, excluding the plane noncharacters.
        assert!(is_symbol(0x100000));
        assert!(is_symbol(0x10FFFD));
        assert!(!is_symbol(0x10FFFE));
    }

    #[test]
    fn is_symbol_blocks() {
        // Arrows 0x2190..=0x21FF.
        assert!(!is_symbol(0x218F));
        assert!(is_symbol(0x2190));
        assert!(is_symbol(0x21FF));
        assert!(!is_symbol(0x2200));

        // Dingbats 0x2700..=0x27BF.
        assert!(is_symbol(0x2700));
        assert!(is_symbol(0x27BF));
        assert!(!is_symbol(0x27C0));

        // Emoticons 0x1F600..=0x1F64F.
        assert!(is_symbol(0x1F600));
        assert!(is_symbol(0x1F64F));
        assert!(!is_symbol(0x1F650));

        // Miscellaneous Symbols 0x2600..=0x26FF.
        assert!(!is_symbol(0x25FF));
        assert!(is_symbol(0x2600));
        assert!(is_symbol(0x26FF));

        // Enclosed Alphanumerics 0x2460..=0x24FF.
        assert!(!is_symbol(0x245F));
        assert!(is_symbol(0x2460));
        assert!(is_symbol(0x24FF));
        assert!(!is_symbol(0x2500));

        // Enclosed Alphanumeric Supplement 0x1F100..=0x1F1FF.
        assert!(!is_symbol(0x1F0FF));
        assert!(is_symbol(0x1F100));
        assert!(is_symbol(0x1F1FF));

        // Miscellaneous Symbols and Pictographs 0x1F300..=0x1F5FF.
        assert!(!is_symbol(0x1F2FF));
        assert!(is_symbol(0x1F300));
        assert!(is_symbol(0x1F5FF));

        // Transport and Map Symbols 0x1F680..=0x1F6FF.
        assert!(!is_symbol(0x1F67F));
        assert!(is_symbol(0x1F680));
        assert!(is_symbol(0x1F6FF));
        assert!(!is_symbol(0x1F700));
    }

    #[test]
    fn is_symbol_excludes_general_symbols() {
        // Block-scoped definition: Unicode general symbol categories (e.g. `+`
        // is Sm, `$` is Sc) are not symbols here.
        assert!(!is_symbol('+' as u32));
        assert!(!is_symbol('$' as u32));
        assert!(!is_symbol('a' as u32));
    }

    fn ci(codepoint: u32, grid_width: u8) -> CellInfo {
        CellInfo {
            codepoint,
            grid_width,
        }
    }

    // A non-graphics symbol (Arrows block): is_symbol true, is_graphics false.
    const SYMBOL: u32 = 0x2190;
    // A symbol that is also a graphics element: Powerline is inside the PUA, so
    // is_symbol true AND is_graphics_element true.
    const GRAPHICS_SYMBOL: u32 = 0xE0B0;

    #[test]
    fn constraint_width_wide_cell_is_two() {
        // Wide cells return 2 regardless of being a symbol or of neighbors.
        let row = [ci(SYMBOL, 2), ci(0, 1)];
        assert_eq!(constraint_width(&row, 0, 2), 2);
    }

    #[test]
    fn constraint_width_non_symbol_uses_grid_width() {
        let row = [ci('a' as u32, 1)];
        assert_eq!(constraint_width(&row, 0, 1), 1);
    }

    #[test]
    fn constraint_width_symbol_at_last_column_is_one() {
        let row = [ci('a' as u32, 1), ci(SYMBOL, 1)];
        assert_eq!(constraint_width(&row, 1, 2), 1);
    }

    #[test]
    fn constraint_width_symbol_after_non_graphics_symbol_is_one() {
        let row = [ci(SYMBOL, 1), ci(0x2191, 1), ci(0, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 1);
    }

    #[test]
    fn constraint_width_symbol_after_graphics_symbol_not_constrained() {
        // Previous cell is a graphics-element symbol, so the previous-symbol
        // rule does not apply; the next-cell check (blank) yields 2.
        let row = [ci(GRAPHICS_SYMBOL, 1), ci(SYMBOL, 1), ci(0, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 2);
    }

    #[test]
    fn constraint_width_symbol_before_blank_is_two() {
        let row = [ci('a' as u32, 1), ci(SYMBOL, 1), ci(0, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 2);
    }

    #[test]
    fn constraint_width_symbol_before_space_is_two() {
        let row = [ci('a' as u32, 1), ci(SYMBOL, 1), ci(0x0020, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 2);
    }

    #[test]
    fn constraint_width_symbol_before_non_space_is_one() {
        let row = [ci('a' as u32, 1), ci(SYMBOL, 1), ci('b' as u32, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 1);
    }

    #[test]
    fn constraint_width_symbol_before_nbsp_is_one() {
        // No-break space (U+00A0) is not `is_space`, so it does not expand the
        // glyph — guards that `is_space` stays the narrow predicate.
        let row = [ci('a' as u32, 1), ci(SYMBOL, 1), ci(0x00A0, 1)];
        assert_eq!(constraint_width(&row, 1, 3), 1);
    }

    fn grid(columns: u16, rows: u16) -> GridSize {
        GridSize { columns, rows }
    }

    fn dummy_vertex() -> CellTextVertex {
        use crate::renderer::shader::{CellTextAtlas, CellTextFlags};
        CellTextVertex {
            glyph_pos: [0, 0],
            glyph_size: [0, 0],
            bearings: [0, 0],
            grid_pos: [0, 0],
            color: [0, 0, 0, 0],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::default(),
            _padding: [0, 0],
        }
    }

    fn menlo_grid() -> SharedGrid {
        use crate::font::codepoint_resolver::CodepointResolver;
        use crate::font::collection::Collection;
        use crate::font::face::coretext::Face;
        use crate::font::Style;
        let mut c = Collection::new();
        c.add(Face::new("Menlo", 32.0), Style::Regular, false)
            .unwrap();
        c.update_metrics().unwrap();
        let metrics = *c.metrics().unwrap();
        SharedGrid::new(CodepointResolver::new(c), metrics)
    }

    fn menlo_opts() -> RenderOptions {
        use crate::font::face::constraint::Constraint;
        use crate::font::face::coretext::Face;
        use crate::font::metrics::Metrics;
        let face = Face::new("Menlo", 32.0);
        RenderOptions {
            grid_metrics: Metrics::calc(face.get_metrics()),
            cell_width: None,
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        }
    }

    fn glyph_for(ch: u8) -> u32 {
        use crate::font::face::coretext::Face;
        u32::from(Face::new("Menlo", 32.0).glyphs_for_characters(&[u16::from(ch)])[0])
    }

    fn sample_metrics() -> Metrics {
        use crate::font::face::coretext::Face;
        Metrics::calc(Face::new("Menlo", 32.0).get_metrics())
    }

    #[test]
    fn render_options_plain_letter_has_no_constraint() {
        let m = sample_metrics();
        let row = [ci('a' as u32, 1)];
        let opts = render_options(m, &row, 0, 1, true, 200);
        assert_eq!(opts.constraint, Constraint::default());
        assert_eq!(opts.cell_width, Some(1));
        assert_eq!(opts.constraint_width, 1);
        // Passthrough fields.
        assert_eq!(opts.grid_metrics, m);
        assert!(opts.thicken);
        assert_eq!(opts.thicken_strength, 200);
    }

    #[test]
    fn render_options_symbol_without_nerd_entry_fits() {
        // 0x1F600 is symbol-like but has no Nerd Font constraint.
        assert_eq!(get_constraint(0x1F600), None);
        let row = [ci(0x1F600, 1)];
        let opts = render_options(sample_metrics(), &row, 0, 1, false, 255);
        assert_eq!(opts.constraint.size, Size::Fit);
    }

    #[test]
    fn render_options_nerd_entry_overrides_symbol_fit() {
        // 0x2630 has a Nerd Font constraint, which takes precedence over the
        // generic symbol-fit path (0x2630 is also symbol-like).
        let expected = get_constraint(0x2630).expect("0x2630 is a Nerd glyph");
        let row = [ci(0x2630, 1)];
        let opts = render_options(sample_metrics(), &row, 0, 1, false, 255);
        assert_eq!(opts.constraint, expected);
        assert_ne!(opts.constraint.size, Size::Fit);
    }

    #[test]
    fn add_run_places_glyphs_at_absolute_columns() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 2));

        // A run at column offset 2 with two glyphs at run-relative x 0/1.
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 2,
                cells: 2,
                font_index: Index::default(),
            },
            glyphs: vec![
                shape::Cell {
                    x: 0,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: glyph_for(b'A'),
                },
                shape::Cell {
                    x: 1,
                    x_offset: 0,
                    y_offset: 0,
                    glyph_index: glyph_for(b'B'),
                },
            ],
        };
        // 'A'/'B' live at columns 2/3 of a 4-wide row.
        let row = [
            ci('x' as u32, 1),
            ci('x' as u32, 1),
            ci('A' as u32, 1),
            ci('B' as u32, 1),
        ];
        let fg = [
            [0, 0, 0, 255],
            [0, 0, 0, 255],
            [10, 20, 30, 255],
            [40, 50, 60, 255],
        ];

        add_run(&mut c, &mut shared, 1, &run, &row, &fg, 4, false, 255).expect("add_run");

        // Two glyphs land at absolute columns 2 and 3 (offset 2 + x 0/1).
        assert_eq!(c.fg_rows[2].len(), 2);
        let v0 = c.fg_rows[2][0];
        let v1 = c.fg_rows[2][1];
        assert_eq!(v0.grid_pos, [2, 1]);
        assert_eq!(v1.grid_pos, [3, 1]);
        assert_eq!(v0.color, [10, 20, 30, 255]);
        assert_eq!(v1.color, [40, 50, 60, 255]);
        assert_eq!(v0.atlas, CellTextAtlas::Grayscale);
        assert_eq!(v1.atlas, CellTextAtlas::Grayscale);
    }

    #[test]
    fn cell_infos_maps_codepoint_and_grid_width() {
        use crate::terminal::style::Style as TermStyle;
        let run_cell = |cp: u32, wide: Wide, is_empty: bool| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle::default(),
            style_id: 0,
            wide,
            is_empty,
            is_codepoint: !is_empty,
        };
        let row = [
            run_cell('A' as u32, Wide::Narrow, false),
            run_cell('W' as u32, Wide::Wide, false),
            run_cell(0, Wide::SpacerTail, false),
            run_cell(0, Wide::SpacerHead, false),
            run_cell(0, Wide::Narrow, true),
        ];

        let infos = cell_infos(&row);

        let codepoints: Vec<u32> = infos.iter().map(|c| c.codepoint).collect();
        assert_eq!(codepoints, vec!['A' as u32, 'W' as u32, 0, 0, 0]);

        let widths: Vec<u8> = infos.iter().map(|c| c.grid_width).collect();
        // Narrow 1, Wide 2, both spacer kinds 1 (not 2), empty 1.
        assert_eq!(widths, vec![1, 2, 1, 1, 1]);
    }

    #[test]
    fn add_glyph_emits_text_cell() {
        use crate::font::collection::Index;
        let mut shared = menlo_grid();
        let opts = menlo_opts();
        let mut c = Contents::default();
        c.resize(grid(4, 2));

        // The expected placement, rendered through a fresh identical grid.
        let expected = menlo_grid()
            .render_glyph(Index::default(), glyph_for(b'M'), &opts)
            .expect("'M' renders")
            .glyph;

        // Non-zero shaper offsets to prove they are summed into the bearings.
        let shaper_cell = shape::Cell {
            x: 0,
            x_offset: 3,
            y_offset: -2,
            glyph_index: glyph_for(b'M'),
        };
        add_glyph(
            &mut c,
            &mut shared,
            [2, 1],
            Index::default(),
            &shaper_cell,
            [10, 20, 30],
            255,
            false,
            &opts,
        )
        .expect("add_glyph");

        // y = 1 -> fg_rows[y + 1] = fg_rows[2].
        assert_eq!(c.fg_rows[2].len(), 1);
        assert!(c.fg_rows[1].is_empty());
        let v = c.fg_rows[2][0];
        assert_eq!(v.grid_pos, [2, 1]);
        assert_eq!(v.atlas, CellTextAtlas::Grayscale);
        assert_eq!(v.color, [10, 20, 30, 255]);
        assert_eq!(v.glyph_pos, [expected.atlas_x, expected.atlas_y]);
        assert_eq!(v.glyph_size, [expected.width, expected.height]);
        assert_eq!(
            v.bearings,
            [
                (expected.offset_x + 3) as i16,
                (expected.offset_y - 2) as i16,
            ]
        );
    }

    #[test]
    fn add_glyph_skips_invisible() {
        use crate::font::collection::Index;
        let mut shared = menlo_grid();
        let opts = menlo_opts();
        let mut c = Contents::default();
        c.resize(grid(4, 2));

        // A space rasterizes to a 0-size glyph, so no cell should be added.
        let shaper_cell = shape::Cell {
            x: 0,
            x_offset: 0,
            y_offset: 0,
            glyph_index: glyph_for(b' '),
        };
        add_glyph(
            &mut c,
            &mut shared,
            [0, 0],
            Index::default(),
            &shaper_cell,
            [0, 0, 0],
            255,
            false,
            &opts,
        )
        .expect("add_glyph");

        assert!(c.fg_rows[1].is_empty());
    }

    #[test]
    fn contents_resize_allocates() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        assert_eq!(c.bg_cells.len(), 6);
        assert!(c.bg_cells.iter().all(|b| *b == CellBg([0, 0, 0, 0])));
        assert_eq!(c.fg_rows.len(), 4); // rows + 2
    }

    #[test]
    fn contents_resize_capacity_layout() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        // Real rows (indices 1..=rows) hold a glyph + underline + strikethrough
        // per column.
        assert!(c.fg_rows[1].capacity() >= 3 * 3);
        assert!(c.fg_rows[2].capacity() >= 3 * 3);
        // The cursor-reserved lists (0 and rows + 1) are smaller.
        assert!(c.fg_rows[0].capacity() < c.fg_rows[1].capacity());
        assert!(c.fg_rows[3].capacity() < c.fg_rows[1].capacity());
    }

    #[test]
    fn contents_bg_cell_indexing() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        *c.bg_cell_mut(1, 2) = CellBg([9, 9, 9, 9]);
        // row * columns + col = 1 * 3 + 2 = 5.
        assert_eq!(c.bg_cells[5], CellBg([9, 9, 9, 9]));
        assert_eq!(*c.bg_cell(1, 2), CellBg([9, 9, 9, 9]));
    }

    #[test]
    fn contents_reset_zeroes_bg() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        *c.bg_cell_mut(0, 0) = CellBg([1, 2, 3, 4]);
        c.reset();
        assert!(c.bg_cells.iter().all(|b| *b == CellBg([0, 0, 0, 0])));
        assert_eq!(c.fg_rows.len(), 4);
    }

    #[test]
    fn contents_reset_clears_fg_rows() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.fg_rows[1].push(dummy_vertex()); // a real row
        c.fg_rows[0].push(dummy_vertex()); // a cursor-reserved row
        c.reset();
        assert!(c.fg_rows.iter().all(|list| list.is_empty()));
    }

    #[test]
    fn contents_resize_zero_sized() {
        let mut c = Contents::default();
        c.resize(grid(0, 0));
        assert!(c.bg_cells.is_empty());
        assert_eq!(c.fg_rows.len(), 2); // the two cursor lists
    }

    #[test]
    fn contents_resize_reinvalidates() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        *c.bg_cell_mut(0, 0) = CellBg([1, 1, 1, 1]);
        c.resize(grid(3, 2));
        assert_eq!(*c.bg_cell(0, 0), CellBg([0, 0, 0, 0]));
    }

    #[test]
    fn set_cursor_block_uses_first_list() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Block));
        assert_eq!(c.fg_rows[0].len(), 1);
        assert!(c.fg_rows[3].is_empty()); // rows + 1 = 3
        assert_eq!(c.get_cursor_glyph(), Some(dummy_vertex()));
    }

    #[test]
    fn set_cursor_other_styles_use_last_list() {
        for style in [
            CursorStyle::BlockHollow,
            CursorStyle::Bar,
            CursorStyle::Underline,
            CursorStyle::Lock,
        ] {
            let mut c = Contents::default();
            c.resize(grid(3, 2));
            c.set_cursor(Some(dummy_vertex()), Some(style));
            assert!(c.fg_rows[0].is_empty());
            assert_eq!(c.fg_rows[3].len(), 1);
            assert_eq!(c.get_cursor_glyph(), Some(dummy_vertex()));
        }
    }

    #[test]
    fn set_cursor_none_value_clears() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Block));
        c.set_cursor(None, Some(CursorStyle::Block));
        assert!(c.fg_rows[0].is_empty());
        assert!(c.fg_rows[3].is_empty());
        assert_eq!(c.get_cursor_glyph(), None);
    }

    #[test]
    fn set_cursor_none_style_clears() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Bar));
        c.set_cursor(Some(dummy_vertex()), None);
        assert!(c.fg_rows[0].is_empty());
        assert!(c.fg_rows[3].is_empty());
        assert_eq!(c.get_cursor_glyph(), None);
    }

    #[test]
    fn set_cursor_replaces_previous() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Block));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Bar));
        // The block list was cleared; only the bar cursor remains — one glyph.
        assert_eq!(c.fg_rows[0].len() + c.fg_rows[3].len(), 1);
        assert!(c.fg_rows[0].is_empty());
        assert_eq!(c.fg_rows[3].len(), 1);
    }

    #[test]
    fn set_cursor_zero_rows_is_noop() {
        let mut c = Contents::default();
        c.resize(grid(0, 0));
        c.set_cursor(Some(dummy_vertex()), Some(CursorStyle::Block));
        assert_eq!(c.get_cursor_glyph(), None);
    }

    #[test]
    fn get_cursor_glyph_empty_is_none() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        assert_eq!(c.get_cursor_glyph(), None);
    }

    fn vertex_at(y: u16) -> CellTextVertex {
        let mut v = dummy_vertex();
        v.grid_pos = [0, y];
        v
    }

    #[test]
    fn add_routes_each_fg_key_to_row() {
        for key in [Key::Text, Key::Underline, Key::Strikethrough, Key::Overline] {
            let mut c = Contents::default();
            c.resize(grid(3, 2));
            c.add(key, vertex_at(1));
            // y = 1 -> fg_rows[y + 1] = fg_rows[2].
            assert_eq!(c.fg_rows[2].len(), 1);
            assert!(c.fg_rows[1].is_empty());
        }
    }

    #[test]
    fn add_appends_multiple() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.add(Key::Text, vertex_at(1));
        c.add(Key::Text, vertex_at(1));
        assert_eq!(c.fg_rows[2].len(), 2);
    }

    #[test]
    fn add_different_rows_route_separately() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.add(Key::Text, vertex_at(0));
        c.add(Key::Text, vertex_at(1));
        assert_eq!(c.fg_rows[1].len(), 1);
        assert_eq!(c.fg_rows[2].len(), 1);
    }

    #[test]
    fn clear_clears_row() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        *c.bg_cell_mut(1, 0) = CellBg([1, 1, 1, 1]);
        c.add(Key::Text, vertex_at(1));
        c.clear(1);
        // Row 1's background span is zeroed.
        for col in 0..3 {
            assert_eq!(*c.bg_cell(1, col), CellBg([0, 0, 0, 0]));
        }
        assert!(c.fg_rows[2].is_empty());
    }

    #[test]
    fn clear_only_affects_its_row() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.add(Key::Text, vertex_at(0));
        c.add(Key::Text, vertex_at(1));
        *c.bg_cell_mut(0, 0) = CellBg([5, 5, 5, 5]);
        *c.bg_cell_mut(1, 0) = CellBg([6, 6, 6, 6]);
        c.clear(1);
        // Row 0 background and foreground are untouched.
        assert_eq!(*c.bg_cell(0, 0), CellBg([5, 5, 5, 5]));
        assert_eq!(c.fg_rows[1].len(), 1);
        // Row 1 is cleared.
        assert_eq!(*c.bg_cell(1, 0), CellBg([0, 0, 0, 0]));
        assert!(c.fg_rows[2].is_empty());
    }

    #[test]
    #[should_panic]
    fn add_bg_key_panics() {
        let mut c = Contents::default();
        c.resize(grid(3, 2));
        c.add(Key::Bg, vertex_at(0));
    }
}
