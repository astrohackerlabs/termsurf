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
use super::state::{Preedit, PreeditRange};
use crate::font::codepoint_resolver::ResolverRenderError;
use crate::font::collection::{Index, Special};
use crate::font::face::constraint::{Constraint, Size};
use crate::font::face::coretext::RenderOptions;
use crate::font::face::nerd_font_attributes::get_constraint;
use crate::font::metrics::Metrics;
use crate::font::run::{shape_row, RunCell, RunOptions, ShapedRun, Wide};
use crate::font::shape;
use crate::font::shared_grid::SharedGrid;
use crate::font::sprite::draw::Sprite;
use crate::font::{Presentation, Style};
use crate::terminal::color::{Palette, Rgb};
use crate::terminal::sgr::Underline;
use crate::terminal::style::{BoldColor, Color, Style as TermStyle};

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

/// A cell's final foreground and background colors. `bg = None` means the default
/// background (the transparent slot — the screen background shows through).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CellColors {
    pub fg: Rgb,
    pub bg: Option<Rgb>,
}

/// Compute a cell's final colors from its `style` and `codepoint`, applying
/// reverse-video (`inverse`). Faithful port of the base (non-selection) per-cell
/// color computation in upstream `rebuildCells`: the foreground swaps to the
/// (default-filled) background under `inverse`, and the background swaps to the
/// foreground on `inverse != is_covering(codepoint)` — a full block (U+2588)
/// paints its cell via the background even without inverse. The selection/search
/// colors and the minimum-contrast adjustment are deferred.
pub(crate) fn cell_colors(
    style: TermStyle,
    codepoint: u32,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
) -> CellColors {
    let fg_style = style.resolve_fg(default_fg, palette, bold);
    let bg_style = style.resolve_bg(palette);
    let inverse = style.flags.inverse;

    // The foreground swaps to the (default-filled) background under inverse.
    let fg = if inverse {
        bg_style.unwrap_or(default_bg)
    } else {
        fg_style
    };
    // The background swaps to the foreground on `inverse != is_covering`: a full
    // block (U+2588) paints its cell via the background even without inverse.
    let bg = if inverse != is_covering(codepoint) {
        Some(fg_style)
    } else {
        bg_style
    };

    CellColors { fg, bg }
}

/// A selection/search color configuration value (upstream `TerminalColor`):
/// either an explicit color, or the cell's own resolved foreground/background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionColor {
    Color(Rgb),
    CellForeground,
    CellBackground,
}

/// Compute a *selected* cell's final colors — upstream's `.selection` arms of the
/// per-cell background/foreground switches. `background`/`foreground` are the
/// `selection-background`/`selection-foreground` config (`None` → the default
/// selection colors: the default foreground for the background, the default
/// background for the foreground — a plain reverse). The covering (full-block)
/// twist does not apply to a selected cell, so this takes no codepoint. The
/// `.search`/`.search_selected` arms are deferred.
#[allow(clippy::too_many_arguments)]
pub(crate) fn selection_colors(
    style: TermStyle,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
    background: Option<SelectionColor>,
    foreground: Option<SelectionColor>,
) -> CellColors {
    let fg_style = style.resolve_fg(default_fg, palette, bold);
    let bg_style = style.resolve_bg(palette);
    let inverse = style.flags.inverse;
    let final_bg = bg_style.unwrap_or(default_bg);

    // Background: `None` → the default foreground (a plain reverse). The
    // `CellForeground`/`CellBackground` options can yield `bg_style` (possibly
    // `None`, i.e. the default background), faithful to upstream.
    let bg = match background {
        None => Some(default_fg),
        Some(SelectionColor::Color(c)) => Some(c),
        Some(SelectionColor::CellForeground) => {
            if inverse {
                bg_style
            } else {
                Some(fg_style)
            }
        }
        Some(SelectionColor::CellBackground) => {
            if inverse {
                Some(fg_style)
            } else {
                bg_style
            }
        }
    };

    // Foreground: `None` → the default background (a plain reverse). The
    // cell-color options use `final_bg` (the default-filled background).
    let fg = match foreground {
        None => default_bg,
        Some(SelectionColor::Color(c)) => c,
        Some(SelectionColor::CellForeground) => {
            if inverse {
                final_bg
            } else {
                fg_style
            }
        }
        Some(SelectionColor::CellBackground) => {
            if inverse {
                fg_style
            } else {
                final_bg
            }
        }
    };

    CellColors { fg, bg }
}

/// Compute the under-cursor text recolor — the color a **block** cursor's covered
/// text is redrawn with (upstream's block-cursor `uniforms.cursor_color`). Given
/// the under-cursor cell's `cursor_style` and the `cursor-text` config: an explicit
/// color, or the cell's resolved foreground/background swapped under `inverse`,
/// defaulting to the default background. Its resolution is identical to the
/// selection foreground arm (the shared `TerminalColor` foreground resolution), so
/// it reuses [`selection_colors`] and takes `.fg`.
pub(crate) fn cursor_text_color(
    cursor_style: TermStyle,
    cursor_text: Option<SelectionColor>,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
) -> Rgb {
    // The selection background config is unused — only `.fg` is taken.
    selection_colors(
        cursor_style,
        default_fg,
        default_bg,
        palette,
        bold,
        None,
        cursor_text,
    )
    .fg
}

/// Compute the cursor's own color — what [`add_cursor`] paints the cursor glyph
/// with (upstream's `cursor_color`). Precedence: the OSC 12 override
/// (`osc12_cursor`), then the `cursor-color` config (an explicit color or the
/// under-cursor cell's resolved foreground/background swapped under `inverse`),
/// then the default **foreground**. The configured `Some(...)` resolution is the
/// selection foreground arm (so it reuses [`selection_colors`] `.fg`); only the
/// OSC 12 override and the `None` default (foreground, not background) differ from
/// [`cursor_text_color`].
#[allow(clippy::too_many_arguments)]
pub(crate) fn cursor_color(
    osc12_cursor: Option<Rgb>,
    config: Option<SelectionColor>,
    cursor_style: TermStyle,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
) -> Rgb {
    // OSC 12 takes precedence over the config and the default.
    if let Some(rgb) = osc12_cursor {
        return rgb;
    }
    match config {
        // No configured cursor color → the default foreground.
        None => default_fg,
        // The `.color`/`.cell-foreground`/`.cell-background` resolution is the
        // selection foreground arm — `Some(...)` never reaches its `None`
        // (default-background) default, so this matches upstream's configured arm.
        Some(cfg) => {
            selection_colors(
                cursor_style,
                default_fg,
                default_bg,
                palette,
                bold,
                None,
                Some(cfg),
            )
            .fg
        }
    }
}

/// Compute a **block** cursor's position uniforms — `cursor_pos` (the cell it
/// covers) and `cursor_wide` (whether it spans two cells) — from the cursor's
/// viewport `(x, y)` and the under-cursor cell's [`Wide`] kind (upstream's
/// block-cursor `uniforms.cursor_pos`/`bools.cursor_wide`). A spacer tail moves
/// the cursor back one column (saturating — it sits over the wide character); the
/// cursor is "wide" for a wide cell or its spacer tail. The caller computes this
/// only for a block cursor.
pub(crate) fn block_cursor_pos(x: u16, y: u16, wide: Wide) -> ([u16; 2], bool) {
    let cursor_x = match wide {
        Wide::SpacerTail => x.saturating_sub(1),
        Wide::Narrow | Wide::SpacerHead | Wide::Wide => x,
    };
    let cursor_wide = matches!(wide, Wide::Wide | Wide::SpacerTail);
    ([cursor_x, y], cursor_wide)
}

/// The effective underline for a cell, applying the hovered-link override: a link
/// cell gets a single underline, unless it already has a **single** underline, in
/// which case it gets a **double** underline to distinguish the link from the
/// cell's own underline. A non-link cell keeps its SGR `underline`. Faithful port
/// of upstream's link underline logic; the hovered-link membership is supplied by
/// the caller as `is_link` (the link set is not yet modeled).
pub(crate) fn link_underline(is_link: bool, underline: Underline) -> Underline {
    if !is_link {
        return underline;
    }
    if matches!(underline, Underline::Single) {
        Underline::Double
    } else {
        Underline::Single
    }
}

/// The per-cell selected state (upstream's `selected` enum). `False` uses the
/// base [`cell_colors`]; the three selected states use [`selected_colors`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Selected {
    False,
    Selection,
    Search,
    SearchSelected,
}

/// A search highlight's tag (upstream `HighlightTag`): a plain match or a match
/// inside the active selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HighlightTag {
    SearchMatch,
    SearchMatchSelected,
}

/// A search highlight: an inclusive `[start, end]` column range and its tag. A
/// renderer input (upstream's per-row render-state highlights), not a shaper field
/// — highlights do not affect run breaking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Highlight {
    pub range: [u16; 2],
    pub tag: HighlightTag,
}

/// The selection/search color config. `selection-*` is optional (`None` → a plain
/// reverse); the `search-*`/`search-selected-*` values are non-optional (upstream
/// `TerminalColor`s with concrete defaults).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SelectionConfig {
    pub background: Option<SelectionColor>,
    pub foreground: Option<SelectionColor>,
    pub search_background: SelectionColor,
    pub search_foreground: SelectionColor,
    pub search_selected_background: SelectionColor,
    pub search_selected_foreground: SelectionColor,
}

impl Default for SelectionConfig {
    fn default() -> Self {
        Self {
            background: None,
            foreground: None,
            // Upstream `config/Config.zig` defaults.
            search_background: SelectionColor::Color(Rgb::new(0xFF, 0xE0, 0x82)),
            search_foreground: SelectionColor::Color(Rgb::new(0, 0, 0)),
            search_selected_background: SelectionColor::Color(Rgb::new(0xF2, 0xA5, 0x7E)),
            search_selected_foreground: SelectionColor::Color(Rgb::new(0, 0, 0)),
        }
    }
}

/// Compute a cell's colors for a `selected` state. `False` returns `None` (the
/// caller uses the base [`cell_colors`], covering twist intact); the three
/// selected states delegate to [`selection_colors`] with the matching config —
/// `Selection` uses the optional `selection-*` config (`None` → a plain reverse),
/// while `Search`/`SearchSelected` wrap their non-optional `search-*` config in
/// `Some`. The `.search`/`.search_selected` switch arms are the `.selection` arms
/// without the reverse default, so this reuses one computation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn selected_colors(
    selected: Selected,
    style: TermStyle,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
    config: &SelectionConfig,
) -> Option<CellColors> {
    let (background, foreground) = match selected {
        Selected::False => return None,
        Selected::Selection => (config.background, config.foreground),
        Selected::Search => (
            Some(config.search_background),
            Some(config.search_foreground),
        ),
        Selected::SearchSelected => (
            Some(config.search_selected_background),
            Some(config.search_selected_foreground),
        ),
    };
    Some(selection_colors(
        style, default_fg, default_bg, palette, bold, background, foreground,
    ))
}

/// A cell's comparison column: a wide cell's spacer tail compares one column to
/// the left (saturating), faithful to upstream's `x_compare`. Shared by the
/// selection and highlight range checks.
fn x_compare(x: u16, wide: Wide) -> u16 {
    if matches!(wide, Wide::SpacerTail) {
        x.saturating_sub(1)
    } else {
        x
    }
}

/// Whether column `x` of a row is selected, given the row's `[start, end]`
/// selection bounds (inclusive). Ports the `.selection` part of upstream's
/// per-cell `selected` derivation.
fn is_selected(selection: Option<[u16; 2]>, x: u16, wide: Wide) -> bool {
    let Some([start, end]) = selection else {
        return false;
    };
    let xc = x_compare(x, wide);
    xc >= start && xc <= end
}

/// The per-cell [`Selected`] state for a rebuild. Faithful port of upstream's
/// `selected` derivation: the selection takes precedence (→ `Selection`), then the
/// **first** matching `highlights` range (→ `Search`/`SearchSelected` by its tag),
/// else `False`. Highlights use the same `x_compare` adjustment as selection.
fn selected_state(
    selection: Option<[u16; 2]>,
    highlights: &[Highlight],
    x: u16,
    wide: Wide,
) -> Selected {
    if is_selected(selection, x, wide) {
        return Selected::Selection;
    }
    let xc = x_compare(x, wide);
    for hl in highlights {
        if xc >= hl.range[0] && xc <= hl.range[1] {
            return match hl.tag {
                HighlightTag::SearchMatch => Selected::Search,
                HighlightTag::SearchMatchSelected => Selected::SearchSelected,
            };
        }
    }
    Selected::False
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

/// Assemble one viewport row's foreground text cells into `contents`. Derives the
/// row's [`CellInfo`] slice ([`cell_infos`]) and per-column `fg_colors` (each
/// cell's foreground + `alpha`), then emits the foreground in **one column-ordered
/// loop** (as upstream): per column — the underline and overline (underneath), the
/// glyph(s) at that column (walking the shaped runs with a monotonic cursor), then
/// the strikethrough (on top). Each cell's [`Selected`] state ([`selected_state`],
/// from the row's `selection` and `highlights`) drives its foreground: a
/// selected/search cell takes [`selected_colors`] (the `selection_config`);
/// otherwise [`cell_colors`] (so the foreground is inverse-aware — reverse-video
/// swaps the glyph color). That one per-cell foreground feeds the glyph and all
/// three decorations (via `fg_colors`). A cell inside the row's hovered-link
/// `link_ranges` (raw column, inclusive — no `x_compare`) has its underline
/// overridden ([`link_underline`]). A **concealed** cell (the `invisible` flag,
/// SGR 8) draws no foreground (its glyph cursor still advances). The per-row
/// foreground body of upstream `rebuildCells`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_row(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    y: u16,
    row_runs: &[ShapedRun],
    row_cells: &[RunCell],
    selection: Option<[u16; 2]>,
    highlights: &[Highlight],
    selection_config: &SelectionConfig,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
    alpha: u8,
    faint_opacity: u8,
    thicken: bool,
    thicken_strength: u8,
    link_ranges: &[[u16; 2]],
) -> Result<(), ResolverRenderError> {
    let cols = row_cells.len();
    let infos = cell_infos(row_cells);
    let fg_colors: Vec<[u8; 4]> = row_cells
        .iter()
        .enumerate()
        .map(|(col, cell)| {
            let x = u16::try_from(col).expect("viewport column fits u16");
            let state = selected_state(selection, highlights, x, cell.wide);
            // A selected cell's foreground comes from the selected-state colors;
            // otherwise the inverse-aware SGR foreground. This one color feeds the
            // glyph and every decoration below.
            let fg = selected_colors(
                state,
                cell.style,
                default_fg,
                default_bg,
                palette,
                bold,
                selection_config,
            )
            .map(|c| c.fg)
            .unwrap_or_else(|| {
                cell_colors(
                    cell.style,
                    cell.codepoint,
                    default_fg,
                    default_bg,
                    palette,
                    bold,
                )
                .fg
            });
            // A faint cell's foreground draws at the reduced faint opacity.
            let a = if cell.style.flags.faint {
                faint_opacity
            } else {
                alpha
            };
            [fg.r, fg.g, fg.b, a]
        })
        .collect();

    // One column-ordered pass (as upstream): per column, emit the underline and
    // overline (underneath), then the glyph(s) at that column, then the
    // strikethrough (on top). The glyph step walks the shaped runs with a cursor
    // that advances monotonically with `col`.
    let grid_metrics = grid.metrics;
    let mut run_i = 0usize;
    let mut glyph_i = 0usize;
    for (col, cell) in row_cells.iter().enumerate() {
        let grid_pos = [u16::try_from(col).expect("column fits u16"), y];
        let rgba = fg_colors[col];
        let fg = [rgba[0], rgba[1], rgba[2]];
        let flags = cell.style.flags;
        // A concealed cell (SGR 8, invisible) draws its background but no
        // foreground (no decorations and no glyph), matching xterm. The glyph
        // cursor still advances below, so its shaped glyph is consumed and later
        // cells stay aligned.
        let conceal = flags.invisible;

        // 1. Underline (its own color, else the foreground) — underneath. A
        //    hovered-link cell overrides the underline ([`link_underline`]); the
        //    link membership uses the raw column (no `x_compare`, unlike
        //    selection/highlights).
        if !conceal {
            let is_link = link_ranges
                .iter()
                .any(|&[start, end]| grid_pos[0] >= start && grid_pos[0] <= end);
            let underline = link_underline(is_link, flags.underline);
            if underline != Underline::None {
                let underline_color = cell
                    .style
                    .resolve_underline_color(palette)
                    .map(|rgb| [rgb.r, rgb.g, rgb.b])
                    .unwrap_or(fg);
                add_underline(
                    contents,
                    grid,
                    grid_pos,
                    underline,
                    underline_color,
                    rgba[3],
                )?;
            }
            // 2. Overline — underneath.
            if flags.overline {
                add_overline(contents, grid, grid_pos, fg, rgba[3])?;
            }
        }

        // 3. The glyph(s) at this column, walking the shaped runs in column order.
        while run_i < row_runs.len() && glyph_i >= row_runs[run_i].glyphs.len() {
            run_i += 1;
            glyph_i = 0;
        }
        if run_i < row_runs.len() {
            let run = &row_runs[run_i];
            // The cursor never falls behind `col` (monotonic, like upstream's
            // assert) — `shape_row` returns runs in row order with glyphs sorted by
            // absolute column.
            debug_assert!(
                glyph_i >= run.glyphs.len()
                    || usize::from(run.run.offset) + usize::from(run.glyphs[glyph_i].x) >= col
            );
            let opts = render_options(grid_metrics, &infos, col, cols, thicken, thicken_strength);
            let cp = infos[col].codepoint;
            while glyph_i < run.glyphs.len()
                && usize::from(run.run.offset) + usize::from(run.glyphs[glyph_i].x) == col
            {
                // Always advance the cursor; emit the glyph only when not concealed.
                if !conceal {
                    add_glyph(
                        contents,
                        grid,
                        grid_pos,
                        run.run.font_index,
                        &run.glyphs[glyph_i],
                        fg,
                        rgba[3],
                        no_min_contrast(cp),
                        &opts,
                    )?;
                }
                glyph_i += 1;
            }
        }

        // 4. Strikethrough — on top.
        if !conceal && flags.strikethrough {
            add_strikethrough(contents, grid, grid_pos, fg, rgba[3])?;
        }
    }
    Ok(())
}

/// Rebuild every viewport row's background **and** foreground into `contents`
/// from the viewport's per-row [`RunOptions`] (from `Terminal::shape_run_options`).
/// `highlights` is the per-row search highlight lists and `link_ranges` the per-row
/// hovered-link column ranges (both parallel to `rows`, like upstream's
/// `row_highlights`; a row beyond either array has none). For each row, write its
/// backgrounds ([`rebuild_bg_row`]) then shape it into [`ShapedRun`]s ([`shape_row`]
/// over the grid's resolver) and assemble its foreground ([`rebuild_row`]) — one
/// pass per row, as upstream `rebuildCells`. The decorations, cursor, and Metal
/// upload remain separate.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_viewport(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    rows: &[RunOptions],
    highlights: &[Vec<Highlight>],
    selection_config: &SelectionConfig,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
    alpha: u8,
    faint_opacity: u8,
    thicken: bool,
    thicken_strength: u8,
    link_ranges: &[Vec<[u16; 2]>],
) -> Result<(), ResolverRenderError> {
    for (y, opts) in rows.iter().enumerate() {
        // This row's search highlights and hovered-link ranges (empty for a row
        // beyond the array).
        let row_highlights = highlights.get(y).map(Vec::as_slice).unwrap_or(&[]);
        let row_links = link_ranges.get(y).map(Vec::as_slice).unwrap_or(&[]);
        let y = u16::try_from(y).expect("viewport row fits u16");

        // Backgrounds first (behind the glyphs); needs no shaping or grid.
        rebuild_bg_row(
            contents,
            y,
            &opts.cells,
            opts.selection,
            row_highlights,
            selection_config,
            default_fg,
            default_bg,
            palette,
            bold,
            alpha,
        );

        // Then the foreground: shape the row (this borrows the grid's resolver) —
        // `runs` is owned, releasing that borrow before `rebuild_row` borrows the
        // grid.
        let runs = shape_row(opts, &mut grid.resolver);
        rebuild_row(
            contents,
            grid,
            y,
            &runs,
            &opts.cells,
            opts.selection,
            row_highlights,
            selection_config,
            default_fg,
            default_bg,
            palette,
            bold,
            alpha,
            faint_opacity,
            thicken,
            thicken_strength,
            row_links,
        )?;
    }
    Ok(())
}

/// Write one viewport row's background cells into `contents`. Each cell's
/// [`Selected`] state ([`selected_state`], from the row's `selection` and
/// `highlights`) drives its background: a selected/search cell takes
/// [`selected_colors`] (the `selection_config`) and is forced **opaque**;
/// otherwise the background comes from [`cell_colors`] (reverse-video + the
/// full-block twist). The RGB falls back to `default_bg` when the resolved
/// background is `None` (upstream `bg orelse default`). The background cell is
/// written unconditionally with a per-cell `bg_alpha`: opaque (the base `alpha`)
/// for a selected, inverse, or explicit-background cell, transparent (`0`)
/// otherwise — so a covering-derived or default background lets the already-drawn
/// screen background show through, while an inverse cell stays opaque even when
/// its resolved background is `None`. The `background_opacity_cells` branch is
/// deferred. The background half of upstream `rebuildCells`'s per-cell work.
#[allow(clippy::too_many_arguments)]
pub(crate) fn rebuild_bg_row(
    contents: &mut Contents,
    y: u16,
    row_cells: &[RunCell],
    selection: Option<[u16; 2]>,
    highlights: &[Highlight],
    selection_config: &SelectionConfig,
    default_fg: Rgb,
    default_bg: Rgb,
    palette: &Palette,
    bold: Option<BoldColor>,
    alpha: u8,
) {
    let row = usize::from(y);
    for (col, cell) in row_cells.iter().enumerate() {
        let x = u16::try_from(col).expect("viewport column fits u16");
        let state = selected_state(selection, highlights, x, cell.wide);
        let colors = selected_colors(
            state,
            cell.style,
            default_fg,
            default_bg,
            palette,
            bold,
            selection_config,
        )
        .unwrap_or_else(|| {
            cell_colors(
                cell.style,
                cell.codepoint,
                default_fg,
                default_bg,
                palette,
                bold,
            )
        });
        // Opaque for a selected, inverse, or explicit-background cell; a
        // covering-derived or default background is transparent. The selection
        // branch is upstream's first `bg_alpha` arm (before inverse/explicit-bg);
        // all three yield the base `alpha`, so the OR is faithful. Evaluated
        // independently of whether the final background is `Some`.
        let has_explicit_bg = !matches!(cell.style.bg_color, Color::None);
        let selected = state != Selected::False;
        let bg_alpha = if selected || cell.style.flags.inverse || has_explicit_bg {
            alpha
        } else {
            0
        };
        // The RGB falls back to the default background (upstream `bg orelse
        // default`).
        let rgb = colors.bg.unwrap_or(default_bg);
        *contents.bg_cell_mut(row, col) = CellBg([rgb.r, rgb.g, rgb.b, bg_alpha]);
    }
}

/// Render a decoration `sprite` through `grid` and add it to `contents` as a
/// `key` cell at `grid_pos` with `color`/`alpha`. The shared body of the
/// decoration writers (underline/strikethrough/overline): a sprite drawn at
/// `cell_width = 1` into the grayscale atlas, with the sprite glyph's own bearings
/// (a decoration has no shaper cell, so no shaper offset).
fn add_sprite_decoration(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    sprite: Sprite,
    key: Key,
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    let opts = RenderOptions {
        grid_metrics: grid.metrics,
        cell_width: Some(1),
        constraint: Constraint::default(),
        constraint_width: 1,
        thicken: false,
        thicken_strength: 255,
    };
    let render = grid.render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)?;

    contents.add(
        key,
        CellTextVertex {
            glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
            glyph_size: [render.glyph.width, render.glyph.height],
            bearings: [
                i16::try_from(render.glyph.offset_x).expect("decoration x bearing fits i16"),
                i16::try_from(render.glyph.offset_y).expect("decoration y bearing fits i16"),
            ],
            grid_pos,
            color: [color[0], color[1], color[2], alpha],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, false),
            _padding: [0, 0],
        },
    );
    Ok(())
}

/// Render a cell's underline as a sprite through `grid` and add it to `contents`
/// as a [`Key::Underline`] decoration cell at `grid_pos` with `color`/`alpha`.
/// `Underline::None` adds nothing. Faithful port of upstream `addUnderline`: the
/// sprite (one of five variants) is drawn at `cell_width = 1` into the grayscale
/// atlas.
pub(crate) fn add_underline(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    underline: Underline,
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    let sprite = match underline {
        Underline::None => return Ok(()),
        Underline::Single => Sprite::Underline,
        Underline::Double => Sprite::UnderlineDouble,
        Underline::Dotted => Sprite::UnderlineDotted,
        Underline::Dashed => Sprite::UnderlineDashed,
        Underline::Curly => Sprite::UnderlineCurly,
    };
    add_sprite_decoration(
        contents,
        grid,
        grid_pos,
        sprite,
        Key::Underline,
        color,
        alpha,
    )
}

/// Render a cell's strikethrough sprite and add a [`Key::Strikethrough`] cell.
/// Faithful port of upstream `addStrikethrough` (the caller guards the flag).
pub(crate) fn add_strikethrough(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    add_sprite_decoration(
        contents,
        grid,
        grid_pos,
        Sprite::Strikethrough,
        Key::Strikethrough,
        color,
        alpha,
    )
}

/// Render a cell's overline sprite and add a [`Key::Overline`] cell. Faithful
/// port of upstream `addOverline` (the caller guards the flag).
pub(crate) fn add_overline(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    add_sprite_decoration(
        contents,
        grid,
        grid_pos,
        Sprite::Overline,
        Key::Overline,
        color,
        alpha,
    )
}

/// Render `cursor_style`'s glyph through `grid` and set it as the cursor cell in
/// `contents` (via [`Contents::set_cursor`]) at `grid_pos`, with `color`/`alpha`.
/// `wide` widens the glyph to two cells. Faithful port of upstream `addCursor`:
/// the four sprite styles render a cursor sprite, while `CursorStyle::Lock`
/// renders the real Nerd Font lock symbol (`0xF023`) via [`SharedGrid::
/// render_codepoint`]. If no font has the lock glyph (roastty embeds no Nerd
/// Font), the cursor is cleared and nothing is drawn, as upstream does.
pub(crate) fn add_cursor(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    grid_pos: [u16; 2],
    cursor_style: CursorStyle,
    wide: bool,
    color: [u8; 3],
    alpha: u8,
) -> Result<(), ResolverRenderError> {
    let opts = RenderOptions {
        grid_metrics: grid.metrics,
        cell_width: Some(if wide { 2 } else { 1 }),
        constraint: Constraint::default(),
        constraint_width: 1,
        thicken: false,
        thicken_strength: 255,
    };

    // The sprite cursors render a cursor sprite; the lock cursor renders the real
    // lock symbol (0xF023). If no font has the lock glyph, clear the cursor and
    // return (upstream logs and returns — the same no-cursor outcome).
    let render = match cursor_style {
        CursorStyle::Block => grid.render_glyph(
            Index::special(Special::Sprite),
            Sprite::CursorRect as u32,
            &opts,
        )?,
        CursorStyle::BlockHollow => grid.render_glyph(
            Index::special(Special::Sprite),
            Sprite::CursorHollowRect as u32,
            &opts,
        )?,
        CursorStyle::Bar => grid.render_glyph(
            Index::special(Special::Sprite),
            Sprite::CursorBar as u32,
            &opts,
        )?,
        CursorStyle::Underline => grid.render_glyph(
            Index::special(Special::Sprite),
            Sprite::CursorUnderline as u32,
            &opts,
        )?,
        CursorStyle::Lock => {
            match grid.render_codepoint(0xF023, Style::Regular, Some(Presentation::Text), &opts)? {
                Some(render) => render,
                None => {
                    contents.set_cursor(None, Some(CursorStyle::Lock));
                    return Ok(());
                }
            }
        }
    };

    let vertex = CellTextVertex {
        glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
        glyph_size: [render.glyph.width, render.glyph.height],
        bearings: [
            i16::try_from(render.glyph.offset_x).expect("cursor x bearing fits i16"),
            i16::try_from(render.glyph.offset_y).expect("cursor y bearing fits i16"),
        ],
        grid_pos,
        color: [color[0], color[1], color[2], alpha],
        atlas: CellTextAtlas::Grayscale,
        // `is_cursor_glyph = true` — upstream marks the cursor vertex.
        flags: CellTextFlags::new(false, true),
        _padding: [0, 0],
    };
    contents.set_cursor(Some(vertex), Some(cursor_style));
    Ok(())
}

/// Render one preedit (IME) codepoint into `contents` at `coord` with `screen_fg`:
/// the glyph (via [`SharedGrid::render_codepoint`], skipped if no font has it) as a
/// grayscale text cell, plus a single underline — and a second underline on the
/// next column for a wide codepoint that is not in the last column. Faithful port
/// of upstream `addPreeditCell`. `cols` is the row's column count (for the
/// wide/last-column check). A render error propagates (the `?`, consistent with the
/// other renderer helpers); a missing glyph (`None`) draws nothing.
pub(crate) fn add_preedit_cell(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    codepoint: u32,
    wide: bool,
    coord: [u16; 2],
    cols: u16,
    screen_fg: [u8; 3],
) -> Result<(), ResolverRenderError> {
    let opts = RenderOptions {
        grid_metrics: grid.metrics,
        cell_width: None,
        constraint: Constraint::default(),
        constraint_width: 1,
        thicken: false,
        thicken_strength: 255,
    };
    let Some(render) =
        grid.render_codepoint(codepoint, Style::Regular, Some(Presentation::Text), &opts)?
    else {
        // No font has the codepoint — draw nothing (upstream logs and returns).
        return Ok(());
    };

    contents.add(
        Key::Text,
        CellTextVertex {
            glyph_pos: [render.glyph.atlas_x, render.glyph.atlas_y],
            glyph_size: [render.glyph.width, render.glyph.height],
            bearings: [
                i16::try_from(render.glyph.offset_x).expect("preedit x bearing fits i16"),
                i16::try_from(render.glyph.offset_y).expect("preedit y bearing fits i16"),
            ],
            grid_pos: coord,
            color: [screen_fg[0], screen_fg[1], screen_fg[2], 255],
            atlas: CellTextAtlas::Grayscale,
            flags: CellTextFlags::new(false, false),
            _padding: [0, 0],
        },
    );

    // A single underline at the cell, and a second on the next column for a wide
    // codepoint (when it fits).
    add_underline(contents, grid, coord, Underline::Single, screen_fg, 255)?;
    if wide && coord[0] + 1 < cols {
        add_underline(
            contents,
            grid,
            [coord[0] + 1, coord[1]],
            Underline::Single,
            screen_fg,
            255,
        )?;
    }
    Ok(())
}

/// Place a `preedit`'s codepoints over the cursor: from `range.start`, render each
/// codepoint (from `range.cp_offset` onward) via [`add_preedit_cell`] at `(x, y)`
/// with `screen_fg`, advancing `x` by the codepoint's cell width (2 wide / 1
/// narrow). `y`/`cols` are the cursor row and the row's column count. Faithful port
/// of upstream's preedit placement loop in `rebuildCells`.
pub(crate) fn add_preedit(
    contents: &mut Contents,
    grid: &mut SharedGrid,
    preedit: &Preedit,
    range: PreeditRange,
    y: u16,
    cols: u16,
    screen_fg: [u8; 3],
) -> Result<(), ResolverRenderError> {
    let mut x = range.start;
    for cp in &preedit.codepoints[range.cp_offset..] {
        add_preedit_cell(
            contents,
            grid,
            cp.codepoint,
            cp.wide,
            [x, y],
            cols,
            screen_fg,
        )?;
        x += if cp.wide { 2 } else { 1 };
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
    fn rebuild_row_places_glyphs_at_absolute_columns() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 2));

        // A run at column offset 2 with two glyphs ('A'/'B') at run-relative x 0/1,
        // landing at absolute columns 2/3 of a 4-wide row (the run cursor's
        // offset/column mapping, now via `rebuild_row`).
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
        // Columns 2/3 carry explicit foreground colors; columns 0/1 are plain (no
        // glyphs, since the run starts at offset 2).
        let cell = |cp: u32, fg: Color| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                fg_color: fg,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [
            cell('x' as u32, Color::None),
            cell('x' as u32, Color::None),
            cell('A' as u32, Color::Rgb(Rgb::new(10, 20, 30))),
            cell('B' as u32, Color::Rgb(Rgb::new(40, 50, 60))),
        ];

        rebuild_row(
            &mut c,
            &mut shared,
            1,
            &[run],
            &row_cells,
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

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
    fn rebuild_row_emits_foreground_column_ordered() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Flags, Style as TermStyle};

        // Two cells, each with an underline AND a glyph. The column-ordered
        // emission interleaves per column — `[col0 underline, col0 glyph, col1
        // underline, col1 glyph]` (grid-pos columns `[0, 0, 1, 1]`) — not the old
        // three-pass order `[col0 ul, col1 ul, col0 glyph, col1 glyph]` (columns
        // `[0, 1, 0, 1]`).
        let cell = |cp: u32| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                flags: Flags {
                    underline: Underline::Single,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [cell('A' as u32), cell('B' as u32)];
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
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

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run],
            &row_cells,
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        // Four foreground cells (two underlines + two glyphs), interleaved by
        // column: the grid-pos column sequence is `[0, 0, 1, 1]`.
        let columns: Vec<u16> = c.fg_rows[1].iter().map(|v| v.grid_pos[0]).collect();
        assert_eq!(columns, [0, 0, 1, 1]);
    }

    #[test]
    fn rebuild_row_applies_link_underline() {
        use crate::font::collection::Index;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Flags, Style as TermStyle};

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);

        let cell = |underline: Underline, wide: Wide| RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle {
                flags: Flags {
                    underline,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide,
            is_empty: false,
            is_codepoint: true,
        };

        // No glyphs (empty runs), so only a drawn underline appears in `fg_rows`.
        let build = |row_cells: &[RunCell], links: &[[u16; 2]]| {
            let mut shared = menlo_grid();
            let mut c = Contents::default();
            c.resize(grid(u16::try_from(row_cells.len()).unwrap(), 1));
            rebuild_row(
                &mut c,
                &mut shared,
                0,
                &[],
                row_cells,
                None,
                &[],
                &SelectionConfig::default(),
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
                255,
                255,
                false,
                255,
                links,
            )
            .expect("rebuild_row");
            (shared, c)
        };
        let sprite_glyph = |shared: &mut SharedGrid, sprite: Sprite| {
            let opts = underline_opts(shared);
            shared
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .expect("sprite renders")
                .glyph
        };

        // A link over an un-underlined cell → a single underline (cache identity
        // vs the directly-rendered single-underline sprite).
        let (mut shared, c) = build(&[cell(Underline::None, Wide::Narrow)], &[[0, 0]]);
        assert_eq!(c.fg_rows[1].len(), 1);
        let g = sprite_glyph(&mut shared, Sprite::Underline);
        assert_eq!(c.fg_rows[1][0].glyph_pos, [g.atlas_x, g.atlas_y]);

        // A link over a single-underlined cell → a double underline.
        let (mut shared, c) = build(&[cell(Underline::Single, Wide::Narrow)], &[[0, 0]]);
        assert_eq!(c.fg_rows[1].len(), 1);
        let g = sprite_glyph(&mut shared, Sprite::UnderlineDouble);
        assert_eq!(c.fg_rows[1][0].glyph_pos, [g.atlas_x, g.atlas_y]);

        // No link → the un-underlined cell draws nothing.
        let (_shared, c) = build(&[cell(Underline::None, Wide::Narrow)], &[]);
        assert!(c.fg_rows[1].is_empty());

        // Raw column: a `SpacerTail` at column 1 with link range `[0, 0]` is NOT
        // linked (raw column 1 ∉ `[0, 0]`; an `x_compare` of 0 would wrongly link
        // it). Column 0 IS linked → exactly one underline, at column 0 only.
        let (_shared, c) = build(
            &[
                cell(Underline::None, Wide::Narrow),
                cell(Underline::None, Wide::SpacerTail),
            ],
            &[[0, 0]],
        );
        assert_eq!(c.fg_rows[1].len(), 1);
        assert_eq!(c.fg_rows[1][0].grid_pos[0], 0);
    }

    #[test]
    fn rebuild_row_skips_concealed_foreground() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Flags, Style as TermStyle};

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);

        let styled_cell = |cp: u32, flags: Flags| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                flags,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        // A concealed cell carrying every foreground decoration (to prove they are
        // all skipped).
        let concealed = |cp: u32| {
            styled_cell(
                cp,
                Flags {
                    invisible: true,
                    underline: Underline::Single,
                    overline: true,
                    strikethrough: true,
                    ..Flags::default()
                },
            )
        };
        // A plain visible cell with no decorations (so its only foreground is the
        // glyph).
        let plain = |cp: u32| styled_cell(cp, Flags::default());
        let glyph = |x: u16, ch: u8| shape::Cell {
            x,
            x_offset: 0,
            y_offset: 0,
            glyph_index: glyph_for(ch),
        };
        let run = |offset: u16, cells: u16, glyphs: Vec<shape::Cell>| ShapedRun {
            run: TextRun {
                hash: 0,
                offset,
                cells,
                font_index: Index::default(),
            },
            glyphs,
        };
        let build = |row_cells: &[RunCell], runs: &[ShapedRun]| {
            let mut shared = menlo_grid();
            let mut c = Contents::default();
            c.resize(grid(u16::try_from(row_cells.len()).unwrap(), 1));
            rebuild_row(
                &mut c,
                &mut shared,
                0,
                runs,
                row_cells,
                None,
                &[],
                &SelectionConfig::default(),
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
                255,
                255,
                false,
                255,
                &[],
            )
            .expect("rebuild_row");
            c
        };

        // A concealed cell with a glyph and underline + overline + strikethrough
        // draws no foreground at all.
        let c = build(&[concealed('A' as u32)], &[run(0, 1, vec![glyph(0, b'A')])]);
        assert!(c.fg_rows[1].is_empty());

        // Cursor alignment: cell 0 is concealed (shaped glyph), cell 1 is a plain
        // visible cell (shaped glyph, no decorations). Its only foreground is the
        // glyph, which must land at column 1 — proving the cursor advanced past the
        // concealed glyph (else the visible cell would emit the concealed glyph at
        // column 0, or emit nothing).
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run(0, 2, vec![glyph(0, b'A'), glyph(1, b'B')])],
            &[concealed('A' as u32), plain('B' as u32)],
            None,
            &[],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");
        // Exactly one foreground cell — the visible 'B' glyph at column 1.
        assert_eq!(c.fg_rows[1].len(), 1);
        let v = c.fg_rows[1][0];
        assert_eq!(v.grid_pos, [1, 0]);
        // It is 'B' (not the concealed 'A'): the cursor consumed 'A' and emitted
        // 'B'. Cache identity using the exact options `rebuild_row` used for column
        // 1 of this row.
        let infos = cell_infos(&[concealed('A' as u32), plain('B' as u32)]);
        let opts = render_options(shared.metrics, &infos, 1, 2, false, 255);
        let b_glyph = shared
            .render_glyph(Index::default(), u32::from(glyph_for(b'B')), &opts)
            .expect("'B' renders")
            .glyph;
        assert_eq!(v.glyph_pos, [b_glyph.atlas_x, b_glyph.atlas_y]);
    }

    #[test]
    fn rebuild_row_derives_infos_and_colors() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 2));

        // 'A' uses the default style; 'B' carries its own foreground color.
        let b_style = TermStyle {
            fg_color: Color::Rgb(Rgb::new(11, 22, 33)),
            ..TermStyle::default()
        };
        let run_cell = |cp: u32, style: TermStyle| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style,
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [
            run_cell('A' as u32, TermStyle::default()),
            run_cell('B' as u32, b_style),
        ];
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
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

        let default_fg = Rgb::new(200, 200, 200);
        rebuild_row(
            &mut c,
            &mut shared,
            1,
            &[run],
            &row_cells,
            None,
            &[],
            &SelectionConfig::default(),
            default_fg,
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        assert_eq!(c.fg_rows[2].len(), 2);
        let v0 = c.fg_rows[2][0];
        let v1 = c.fg_rows[2][1];
        assert_eq!(v0.grid_pos, [0, 1]);
        assert_eq!(v1.grid_pos, [1, 1]);
        assert_eq!(v0.atlas, CellTextAtlas::Grayscale);
        assert_eq!(v1.atlas, CellTextAtlas::Grayscale);
        // Column 0 (default style) resolves to default_fg; column 1 carries its
        // own color — proving fg_colors is per-cell, not a flat default_fg.
        assert_eq!(v0.color, [200, 200, 200, 255]);
        assert_eq!(v1.color, [11, 22, 33, 255]);
        assert_ne!(v1.color, [200, 200, 200, 255]);
    }

    #[test]
    fn rebuild_row_emits_decorations_layered() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(1, 1));

        // One cell 'A' with an underline (its own color), an overline, and a
        // strikethrough.
        let underline_rgb = Rgb::new(1, 2, 3);
        let style = TermStyle {
            underline_color: Color::Rgb(underline_rgb),
            flags: Flags {
                underline: Underline::Single,
                overline: true,
                strikethrough: true,
                ..Flags::default()
            },
            ..TermStyle::default()
        };
        let row_cells = [RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style,
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        }];
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };

        let default_fg = Rgb::new(200, 200, 200);
        let fg = [200, 200, 200, 255];
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run],
            &row_cells,
            None,
            &[],
            &SelectionConfig::default(),
            default_fg,
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        // Draw order in fg_rows[1]: underline, overline (underneath), glyph 'A',
        // strikethrough (on top).
        assert_eq!(c.fg_rows[1].len(), 4);
        let cells = c.fg_rows[1].clone();

        let opts = underline_opts(&shared);
        let sprite_pos = |grid: &mut SharedGrid, sprite: Sprite| {
            let g = grid
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .unwrap()
                .glyph;
            [g.atlas_x, g.atlas_y]
        };

        // [0] underline — its own color, the underline sprite.
        assert_eq!(cells[0].color, [1, 2, 3, 255]);
        assert_eq!(
            cells[0].glyph_pos,
            sprite_pos(&mut shared, Sprite::Underline)
        );
        // [1] overline — foreground color, the overline sprite.
        assert_eq!(cells[1].color, fg);
        assert_eq!(
            cells[1].glyph_pos,
            sprite_pos(&mut shared, Sprite::Overline)
        );
        // [3] strikethrough — foreground color, the strikethrough sprite.
        assert_eq!(cells[3].color, fg);
        assert_eq!(
            cells[3].glyph_pos,
            sprite_pos(&mut shared, Sprite::Strikethrough)
        );
        // [2] is the glyph 'A' (foreground color), distinct from the decorations.
        assert_eq!(cells[2].color, fg);
        assert_ne!(cells[2].glyph_pos, cells[0].glyph_pos);
    }

    #[test]
    fn rebuild_row_applies_faint_alpha_to_glyph_and_decorations() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Flags, Style as TermStyle};

        // A faint cell 'A' with underline + overline + strikethrough.
        let faint_cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle {
                flags: Flags {
                    faint: true,
                    underline: Underline::Single,
                    overline: true,
                    strikethrough: true,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(1, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run],
            &[faint_cell],
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            128, // faint_opacity
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        // The glyph and all three decorations (4 cells) draw at the faint alpha.
        assert_eq!(c.fg_rows[1].len(), 4);
        for v in &c.fg_rows[1] {
            assert_eq!(v.color[3], 128, "faint alpha");
        }

        // A non-faint cell draws its glyph at the base alpha (255).
        let plain_cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle::default(),
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let run2 = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };
        let mut c2 = Contents::default();
        c2.resize(grid(1, 1));
        rebuild_row(
            &mut c2,
            &mut shared,
            0,
            &[run2],
            &[plain_cell],
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            128,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");
        assert_eq!(c2.fg_rows[1][0].color[3], 255);
    }

    #[test]
    fn rebuild_row_recolors_selected_foreground() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Flags, Style as TermStyle};

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(9, 8, 7);

        // A cell 'A' with underline + overline + strikethrough and no explicit
        // colors: its SGR foreground is `default_fg`; the default selection
        // foreground (a plain reverse) is `default_bg`.
        let cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle {
                flags: Flags {
                    underline: Underline::Single,
                    overline: true,
                    strikethrough: true,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let run = || ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };

        let mut shared = menlo_grid();

        // Selected (default config): the glyph and all three decorations draw
        // with the selection foreground (= default_bg, a plain reverse).
        let mut c = Contents::default();
        c.resize(grid(1, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run()],
            &[cell.clone()],
            Some([0, 0]),
            &[],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");
        assert_eq!(c.fg_rows[1].len(), 4);
        for v in &c.fg_rows[1] {
            assert_eq!(
                [v.color[0], v.color[1], v.color[2]],
                [default_bg.r, default_bg.g, default_bg.b],
                "selected foreground"
            );
        }

        // Not selected: the same cell keeps its SGR foreground (= default_fg).
        let mut c2 = Contents::default();
        c2.resize(grid(1, 1));
        rebuild_row(
            &mut c2,
            &mut shared,
            0,
            &[run()],
            &[cell],
            None,
            &[],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");
        for v in &c2.fg_rows[1] {
            assert_eq!(
                [v.color[0], v.color[1], v.color[2]],
                [default_fg.r, default_fg.g, default_fg.b],
                "unselected foreground"
            );
        }
    }

    #[test]
    fn rebuild_row_selected_underline_keeps_explicit_color() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(9, 8, 7);
        let uc = Rgb::new(1, 2, 3);

        // A selected cell 'A' with an underline that has an EXPLICIT SGR color.
        let cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle {
                underline_color: Color::Rgb(uc),
                flags: Flags {
                    underline: Underline::Single,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(1, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run],
            &[cell],
            Some([0, 0]),
            &[],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        // The underline (emitted first, underneath) keeps its explicit SGR color;
        // the glyph (emitted after) uses the selection foreground (= default_bg).
        assert_eq!(c.fg_rows[1].len(), 2);
        assert_eq!(
            [
                c.fg_rows[1][0].color[0],
                c.fg_rows[1][0].color[1],
                c.fg_rows[1][0].color[2]
            ],
            [uc.r, uc.g, uc.b],
            "explicit underline color wins"
        );
        assert_eq!(
            [
                c.fg_rows[1][1].color[0],
                c.fg_rows[1][1].color[1],
                c.fg_rows[1][1].color[2]
            ],
            [default_bg.r, default_bg.g, default_bg.b],
            "glyph uses selection foreground"
        );
    }

    #[test]
    fn rebuild_viewport_fills_each_row() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::Style as TermStyle;

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 2));

        let cell = |cp: u32, is_empty: bool| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle::default(),
            style_id: 0,
            wide: Wide::Narrow,
            is_empty,
            is_codepoint: !is_empty,
        };
        // Row 0 "AB" (two visible glyphs); row 1 "C " (one visible glyph — the
        // empty cell shapes to a 0-size glyph and is skipped). Distinct rows.
        let rows = vec![
            RunOptions {
                cells: vec![cell('A' as u32, false), cell('B' as u32, false)],
                ..Default::default()
            },
            RunOptions {
                cells: vec![cell('C' as u32, false), cell(0, true)],
                ..Default::default()
            },
        ];

        rebuild_viewport(
            &mut c,
            &mut shared,
            &rows,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_viewport");

        // Row 0 -> fg_rows[1] (two glyphs); row 1 -> fg_rows[2] (one glyph). The
        // distinct counts prove each row is shaped from its own RunOptions.
        assert_eq!(c.fg_rows[1].len(), 2);
        assert_eq!(c.fg_rows[2].len(), 1);
        assert_eq!(c.fg_rows[1][0].grid_pos, [0, 0]);
        assert_eq!(c.fg_rows[1][1].grid_pos, [1, 0]);
        assert_eq!(c.fg_rows[2][0].grid_pos, [0, 1]);
    }

    #[test]
    fn rebuild_viewport_fills_background_and_foreground() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 1));

        // Column 0 has an explicit background; both columns are visible glyphs.
        let cell = |cp: u32, bg: Color| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                bg_color: bg,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let rows = vec![RunOptions {
            cells: vec![
                cell('A' as u32, Color::Palette(1)),
                cell('B' as u32, Color::None),
            ],
            ..Default::default()
        }];

        rebuild_viewport(
            &mut c,
            &mut shared,
            &rows,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_viewport");

        // Foreground: the row's glyphs are present (one pass filled fg_rows).
        assert_eq!(c.fg_rows[1].len(), 2);
        // Background: the same pass wrote column 0's explicit background.
        let p1 = DEFAULT_PALETTE[1];
        assert_eq!(*c.bg_cell(0, 0), CellBg([p1.r, p1.g, p1.b, 255]));
        // Column 1's default background is transparent.
        assert_eq!(*c.bg_cell(0, 1), CellBg([0, 0, 0, 0]));
    }

    #[test]
    fn rebuild_bg_row_writes_and_clears() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut c = Contents::default();
        c.resize(grid(2, 2));

        // Pre-seed column 1's background with a stale color to prove the default
        // (`None`) cell is actively cleared, not merely left untouched.
        *c.bg_cell_mut(0, 1) = CellBg([1, 2, 3, 4]);

        let cell = |bg: Color| RunCell {
            codepoint: 'x' as u32,
            graphemes: vec![],
            style: TermStyle {
                bg_color: bg,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [cell(Color::Palette(1)), cell(Color::None)];

        rebuild_bg_row(
            &mut c,
            0,
            &row_cells,
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
        );

        let p1 = DEFAULT_PALETTE[1];
        assert_eq!(*c.bg_cell(0, 0), CellBg([p1.r, p1.g, p1.b, 255]));
        // The default-background cell is cleared to transparent.
        assert_eq!(*c.bg_cell(0, 1), CellBg([0, 0, 0, 0]));
    }

    #[test]
    fn rebuild_viewport_applies_inverse() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(1, 1));

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        // 'A' with explicit fg/bg and the inverse flag set.
        let cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle {
                fg_color: Color::Rgb(a),
                bg_color: Color::Rgb(b),
                flags: Flags {
                    inverse: true,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let rows = vec![RunOptions {
            cells: vec![cell],
            ..Default::default()
        }];

        rebuild_viewport(
            &mut c,
            &mut shared,
            &rows,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_viewport");

        // Inverse swaps: the glyph takes the background color, the bg cell takes
        // the foreground color.
        assert_eq!(c.fg_rows[1][0].color, [b.r, b.g, b.b, 255]);
        assert_eq!(*c.bg_cell(0, 0), CellBg([a.r, a.g, a.b, 255]));
    }

    #[test]
    fn rebuild_bg_row_applies_full_block_twist() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut c = Contents::default();
        c.resize(grid(1, 1));

        let a = Rgb::new(11, 22, 33);
        let b = Rgb::new(44, 55, 66);
        // A full block (U+2588), non-inverse: the bg twist paints the cell with
        // the foreground color via the background (proving the codepoint is
        // threaded into `cell_colors`).
        let cell = RunCell {
            codepoint: 0x2588,
            graphemes: vec![],
            style: TermStyle {
                fg_color: Color::Rgb(a),
                bg_color: Color::Rgb(b),
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };

        rebuild_bg_row(
            &mut c,
            0,
            &[cell],
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
        );

        // The full block paints its bg with the foreground color (a), not b.
        assert_eq!(*c.bg_cell(0, 0), CellBg([a.r, a.g, a.b, 255]));
    }

    #[test]
    fn rebuild_bg_row_full_block_without_bg_is_transparent() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut c = Contents::default();
        c.resize(grid(1, 1));

        let a = Rgb::new(11, 22, 33);
        // A full block (U+2588) with an explicit fg but NO explicit background,
        // non-inverse: the covering twist makes the final bg `Some(fg)`, but with
        // no explicit `bg_style` and no inverse the `bg_alpha` is 0 — the cell is
        // transparent (the screen background shows through), carrying the
        // foreground RGB at alpha 0.
        let cell = RunCell {
            codepoint: 0x2588,
            graphemes: vec![],
            style: TermStyle {
                fg_color: Color::Rgb(a),
                bg_color: Color::None,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };

        rebuild_bg_row(
            &mut c,
            0,
            &[cell],
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
        );

        // Covering-derived bg, no explicit bg, not inverse → transparent.
        assert_eq!(*c.bg_cell(0, 0), CellBg([a.r, a.g, a.b, 0]));
    }

    #[test]
    fn rebuild_bg_row_inverse_without_bg_is_opaque_default() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let mut c = Contents::default();
        c.resize(grid(1, 1));

        let a = Rgb::new(11, 22, 33);
        let default_bg = Rgb::new(7, 8, 9);
        // An inverse full block (U+2588) with an explicit fg but NO explicit
        // background. `inverse != is_covering` cancels, so the final bg is `None`;
        // the RGB falls back to `default_bg`, and the inverse branch makes the
        // `bg_alpha` opaque even though the final bg is `None`.
        let cell = RunCell {
            codepoint: 0x2588,
            graphemes: vec![],
            style: TermStyle {
                fg_color: Color::Rgb(a),
                bg_color: Color::None,
                flags: Flags {
                    inverse: true,
                    ..Flags::default()
                },
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };

        rebuild_bg_row(
            &mut c,
            0,
            &[cell],
            None,
            &[],
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
        );

        // Inverse, no explicit bg → opaque default background (proves the inverse
        // branch fires though the final bg is `None`, and the RGB falls back to
        // `default_bg`).
        assert_eq!(
            *c.bg_cell(0, 0),
            CellBg([default_bg.r, default_bg.g, default_bg.b, 255])
        );
    }

    #[test]
    fn is_selected_matches_the_x_compare_derivation() {
        // No bounds → never selected.
        assert!(!is_selected(None, 0, Wide::Narrow));
        assert!(!is_selected(None, 3, Wide::SpacerTail));

        // Inclusive [1, 3] bounds for a narrow cell.
        let bounds = Some([1, 3]);
        assert!(!is_selected(bounds, 0, Wide::Narrow)); // before
        assert!(is_selected(bounds, 1, Wide::Narrow)); // at start
        assert!(is_selected(bounds, 2, Wide::Narrow)); // inside
        assert!(is_selected(bounds, 3, Wide::Narrow)); // at end
        assert!(!is_selected(bounds, 4, Wide::Narrow)); // after

        // A spacer tail compares one column to the left: at `end + 1 = 4` its
        // `x_compare = 3` is in-bounds, where a narrow cell at column 4 is not.
        assert!(is_selected(bounds, 4, Wide::SpacerTail));
        assert!(!is_selected(bounds, 4, Wide::Narrow));
        // Saturating: a spacer tail at column 0 compares 0 (no underflow).
        assert!(!is_selected(Some([1, 3]), 0, Wide::SpacerTail));
        assert!(is_selected(Some([0, 0]), 0, Wide::SpacerTail));
    }

    #[test]
    fn selected_state_yields_selection_or_false() {
        let no_hl: &[Highlight] = &[];

        // No bounds, no highlights → `False`.
        assert_eq!(
            selected_state(None, no_hl, 0, Wide::Narrow),
            Selected::False
        );

        // Inside the inclusive [1, 3] bounds → `Selection`.
        let bounds = Some([1, 3]);
        assert_eq!(
            selected_state(bounds, no_hl, 0, Wide::Narrow),
            Selected::False
        ); // before
        assert_eq!(
            selected_state(bounds, no_hl, 1, Wide::Narrow),
            Selected::Selection
        ); // at start
        assert_eq!(
            selected_state(bounds, no_hl, 3, Wide::Narrow),
            Selected::Selection
        ); // at end
        assert_eq!(
            selected_state(bounds, no_hl, 4, Wide::Narrow),
            Selected::False
        ); // after

        // A spacer tail at `end + 1 = 4` compares `x_compare = 3` → `Selection`,
        // where a narrow cell at column 4 is `False`.
        assert_eq!(
            selected_state(bounds, no_hl, 4, Wide::SpacerTail),
            Selected::Selection
        );
        assert_eq!(
            selected_state(bounds, no_hl, 4, Wide::Narrow),
            Selected::False
        );
    }

    #[test]
    fn selected_state_consults_highlights() {
        // A plain match and a match-inside-selection highlight, columns [2, 4]
        // and [6, 8].
        let highlights = [
            Highlight {
                range: [2, 4],
                tag: HighlightTag::SearchMatch,
            },
            Highlight {
                range: [6, 8],
                tag: HighlightTag::SearchMatchSelected,
            },
        ];

        // A cell inside a `SearchMatch` highlight → `Search`; a
        // `SearchMatchSelected` highlight → `SearchSelected`; outside both →
        // `False`.
        assert_eq!(
            selected_state(None, &highlights, 3, Wide::Narrow),
            Selected::Search
        );
        assert_eq!(
            selected_state(None, &highlights, 7, Wide::Narrow),
            Selected::SearchSelected
        );
        assert_eq!(
            selected_state(None, &highlights, 5, Wide::Narrow),
            Selected::False
        );

        // Selection takes precedence over a highlight at the same column.
        assert_eq!(
            selected_state(Some([3, 3]), &highlights, 3, Wide::Narrow),
            Selected::Selection
        );

        // First-match-wins: two overlapping highlights with different tags → the
        // first listed.
        let overlap = [
            Highlight {
                range: [0, 5],
                tag: HighlightTag::SearchMatch,
            },
            Highlight {
                range: [0, 5],
                tag: HighlightTag::SearchMatchSelected,
            },
        ];
        assert_eq!(
            selected_state(None, &overlap, 2, Wide::Narrow),
            Selected::Search
        );

        // The spacer-tail adjustment applies to highlight matching: a spacer tail
        // at `end + 1 = 5` compares `x_compare = 4` → in [2, 4] → `Search`.
        assert_eq!(
            selected_state(None, &highlights, 5, Wide::SpacerTail),
            Selected::Search
        );
    }

    #[test]
    fn rebuild_bg_row_recolors_selected_cells_opaque() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut c = Contents::default();
        c.resize(grid(2, 1));

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);

        // Two narrow cells with NO explicit background. Without selection both
        // would be transparent (the Exp 384 path).
        let cell = || RunCell {
            codepoint: 'x' as u32,
            graphemes: vec![],
            style: TermStyle {
                bg_color: Color::None,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [cell(), cell()];

        // Select only column 0, default selection config (a plain reverse).
        rebuild_bg_row(
            &mut c,
            0,
            &row_cells,
            Some([0, 0]),
            &[],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
        );

        // Column 0 is selected: the default selection background is the default
        // foreground, made opaque (both the recolor and the selection → opaque
        // alpha — the Exp 384 path would have left this transparent).
        assert_eq!(
            *c.bg_cell(0, 0),
            CellBg([default_fg.r, default_fg.g, default_fg.b, 255])
        );
        // Column 1 is not selected: unchanged (no explicit bg, not inverse →
        // transparent).
        assert_eq!(*c.bg_cell(0, 1), CellBg([0, 0, 0, 0]));
    }

    #[test]
    fn rebuild_bg_row_recolors_highlighted_cells() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);
        let amber = Rgb::new(0xFF, 0xE0, 0x82);
        let salmon = Rgb::new(0xF2, 0xA5, 0x7E);

        let cell = || RunCell {
            codepoint: 'x' as u32,
            graphemes: vec![],
            style: TermStyle {
                bg_color: Color::None,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let row_cells = [cell(), cell()];

        // Column 0 is a plain search match (amber bg, opaque), column 1 is
        // un-highlighted (transparent).
        let mut c = Contents::default();
        c.resize(grid(2, 1));
        rebuild_bg_row(
            &mut c,
            0,
            &row_cells,
            None,
            &[Highlight {
                range: [0, 0],
                tag: HighlightTag::SearchMatch,
            }],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
        );
        assert_eq!(*c.bg_cell(0, 0), CellBg([amber.r, amber.g, amber.b, 255]));
        assert_eq!(*c.bg_cell(0, 1), CellBg([0, 0, 0, 0]));

        // A search-match-selected highlight → the salmon background.
        let mut c2 = Contents::default();
        c2.resize(grid(2, 1));
        rebuild_bg_row(
            &mut c2,
            0,
            &row_cells,
            None,
            &[Highlight {
                range: [0, 0],
                tag: HighlightTag::SearchMatchSelected,
            }],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
        );
        assert_eq!(
            *c2.bg_cell(0, 0),
            CellBg([salmon.r, salmon.g, salmon.b, 255])
        );
    }

    #[test]
    fn rebuild_row_recolors_highlighted_foreground() {
        use crate::font::collection::Index;
        use crate::font::run::TextRun;
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::Style as TermStyle;

        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);
        let black = Rgb::new(0, 0, 0); // the default search foreground

        let cell = RunCell {
            codepoint: 'A' as u32,
            graphemes: vec![],
            style: TermStyle::default(),
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let run = ShapedRun {
            run: TextRun {
                hash: 0,
                offset: 0,
                cells: 1,
                font_index: Index::default(),
            },
            glyphs: vec![shape::Cell {
                x: 0,
                x_offset: 0,
                y_offset: 0,
                glyph_index: glyph_for(b'A'),
            }],
        };

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(1, 1));
        rebuild_row(
            &mut c,
            &mut shared,
            0,
            &[run],
            &[cell],
            None,
            &[Highlight {
                range: [0, 0],
                tag: HighlightTag::SearchMatch,
            }],
            &SelectionConfig::default(),
            default_fg,
            default_bg,
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_row");

        // The glyph draws with the search foreground (black), not its SGR fg.
        assert_eq!(
            [
                c.fg_rows[1][0].color[0],
                c.fg_rows[1][0].color[1],
                c.fg_rows[1][0].color[2]
            ],
            [black.r, black.g, black.b]
        );
    }

    #[test]
    fn rebuild_viewport_threads_per_row_highlights() {
        use crate::terminal::color::{Rgb, DEFAULT_PALETTE};
        use crate::terminal::style::{Color, Style as TermStyle};

        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 1));

        let amber = Rgb::new(0xFF, 0xE0, 0x82);
        let black = Rgb::new(0, 0, 0);

        let cell = |cp: u32| RunCell {
            codepoint: cp,
            graphemes: vec![],
            style: TermStyle {
                bg_color: Color::None,
                ..TermStyle::default()
            },
            style_id: 0,
            wide: Wide::Narrow,
            is_empty: false,
            is_codepoint: true,
        };
        let rows = vec![RunOptions {
            cells: vec![cell('A' as u32), cell('B' as u32)],
            ..Default::default()
        }];
        // Row 0 highlights column 1 only as a search match.
        let highlights = vec![vec![Highlight {
            range: [1, 1],
            tag: HighlightTag::SearchMatch,
        }]];

        rebuild_viewport(
            &mut c,
            &mut shared,
            &rows,
            &highlights,
            &SelectionConfig::default(),
            Rgb::new(200, 200, 200),
            Rgb::new(0, 0, 0),
            &DEFAULT_PALETTE,
            None,
            255,
            255,
            false,
            255,
            &[],
        )
        .expect("rebuild_viewport");

        // Column 1 (highlighted) → amber background; column 0 (un-highlighted) →
        // transparent. Two glyphs present; the highlighted column's glyph is black.
        assert_eq!(*c.bg_cell(0, 1), CellBg([amber.r, amber.g, amber.b, 255]));
        assert_eq!(*c.bg_cell(0, 0), CellBg([0, 0, 0, 0]));
        // The glyph at column 1 carries the search foreground (black). Its
        // `grid_pos` is `[col, y]` = `[1, 0]` (stored in `fg_rows[y + 1]`).
        let col1 = c.fg_rows[1]
            .iter()
            .find(|v| v.grid_pos == [1, 0])
            .expect("column 1 glyph");
        assert_eq!(
            [col1.color[0], col1.color[1], col1.color[2]],
            [black.r, black.g, black.b]
        );
    }

    fn underline_opts(grid: &SharedGrid) -> RenderOptions {
        RenderOptions {
            grid_metrics: grid.metrics,
            cell_width: Some(1),
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        }
    }

    #[test]
    fn add_underline_maps_each_variant_to_its_sprite() {
        for (variant, sprite) in [
            (Underline::Single, Sprite::Underline),
            (Underline::Double, Sprite::UnderlineDouble),
            (Underline::Dotted, Sprite::UnderlineDotted),
            (Underline::Dashed, Sprite::UnderlineDashed),
            (Underline::Curly, Sprite::UnderlineCurly),
        ] {
            let mut shared = menlo_grid();
            let mut c = Contents::default();
            c.resize(grid(2, 1));

            add_underline(&mut c, &mut shared, [0, 0], variant, [5, 6, 7], 255)
                .expect("add_underline");

            // One Key::Underline cell, routed like text to fg_rows[y + 1].
            assert_eq!(c.fg_rows[1].len(), 1, "{variant:?}");
            let v = c.fg_rows[1][0];
            assert_eq!(v.grid_pos, [0, 0]);
            assert_eq!(v.atlas, CellTextAtlas::Grayscale);
            assert_eq!(v.color, [5, 6, 7, 255]);

            // Direct-render the expected sprite on the SAME grid: the cache is
            // keyed by the sprite codepoint, so this is a hit (identical atlas
            // region) iff `add_underline` rendered exactly this sprite.
            let opts = underline_opts(&shared);
            let expected = shared
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .expect("expected sprite renders")
                .glyph;
            assert_eq!(
                v.glyph_pos,
                [expected.atlas_x, expected.atlas_y],
                "{variant:?} selected the wrong sprite"
            );
            assert_eq!(
                v.glyph_size,
                [expected.width, expected.height],
                "{variant:?}"
            );
            assert_eq!(
                v.bearings,
                [expected.offset_x as i16, expected.offset_y as i16],
                "{variant:?}"
            );
        }
    }

    #[test]
    fn add_underline_none_adds_nothing() {
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(2, 1));
        add_underline(&mut c, &mut shared, [0, 0], Underline::None, [5, 6, 7], 255)
            .expect("add_underline");
        assert!(c.fg_rows[1].is_empty());
    }

    #[test]
    fn add_strikethrough_and_overline_render_their_sprites() {
        // (writer, expected sprite). Each is checked independently via the
        // same-grid cache-identity technique used for underlines.
        type Writer = fn(
            &mut Contents,
            &mut SharedGrid,
            [u16; 2],
            [u8; 3],
            u8,
        ) -> Result<(), ResolverRenderError>;
        let cases: [(Writer, Sprite); 2] = [
            (add_strikethrough, Sprite::Strikethrough),
            (add_overline, Sprite::Overline),
        ];

        for (writer, sprite) in cases {
            let mut shared = menlo_grid();
            let mut c = Contents::default();
            c.resize(grid(2, 1));

            writer(&mut c, &mut shared, [1, 0], [9, 8, 7], 255).expect("decoration");

            assert_eq!(c.fg_rows[1].len(), 1, "{sprite:?}");
            let v = c.fg_rows[1][0];
            assert_eq!(v.grid_pos, [1, 0]);
            assert_eq!(v.atlas, CellTextAtlas::Grayscale);
            assert_eq!(v.color, [9, 8, 7, 255]);

            let opts = underline_opts(&shared);
            let expected = shared
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .expect("expected sprite renders")
                .glyph;
            assert_eq!(
                v.glyph_pos,
                [expected.atlas_x, expected.atlas_y],
                "{sprite:?} mismatch"
            );
            assert_eq!(
                v.glyph_size,
                [expected.width, expected.height],
                "{sprite:?}"
            );
            assert_eq!(
                v.bearings,
                [expected.offset_x as i16, expected.offset_y as i16],
                "{sprite:?}"
            );
        }
    }

    fn cursor_opts(grid: &SharedGrid, wide: bool) -> RenderOptions {
        RenderOptions {
            grid_metrics: grid.metrics,
            cell_width: Some(if wide { 2 } else { 1 }),
            constraint: Constraint::default(),
            constraint_width: 1,
            thicken: false,
            thicken_strength: 255,
        }
    }

    #[test]
    fn add_cursor_maps_styles_and_routes() {
        // (style, expected sprite, target cursor list). Block -> fg_rows[0];
        // the others -> fg_rows[last] (rows + 1 = 4 for a 3-row grid).
        let cases = [
            (CursorStyle::Block, Sprite::CursorRect, 0usize),
            (CursorStyle::BlockHollow, Sprite::CursorHollowRect, 4usize),
            (CursorStyle::Bar, Sprite::CursorBar, 4usize),
            (CursorStyle::Underline, Sprite::CursorUnderline, 4usize),
        ];
        for (style, sprite, list) in cases {
            let mut shared = menlo_grid();
            let mut c = Contents::default();
            c.resize(grid(4, 3));

            add_cursor(&mut c, &mut shared, [2, 1], style, false, [9, 0, 9], 255)
                .expect("add_cursor");

            assert_eq!(c.fg_rows[list].len(), 1, "{style:?}");
            let other = if list == 0 { 4 } else { 0 };
            assert!(c.fg_rows[other].is_empty(), "{style:?}");

            let v = c.fg_rows[list][0];
            assert_eq!(v.grid_pos, [2, 1]);
            assert_eq!(v.atlas, CellTextAtlas::Grayscale);
            assert_eq!(v.color, [9, 0, 9, 255]);
            assert_eq!(v.flags, CellTextFlags::new(false, true), "{style:?}");

            let opts = cursor_opts(&shared, false);
            let expected = shared
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .unwrap()
                .glyph;
            assert_eq!(
                v.glyph_pos,
                [expected.atlas_x, expected.atlas_y],
                "{style:?}"
            );
            assert_eq!(v.glyph_size, [expected.width, expected.height], "{style:?}");
        }
    }

    #[test]
    fn add_cursor_wide_uses_two_cells() {
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 3));

        add_cursor(
            &mut c,
            &mut shared,
            [0, 0],
            CursorStyle::Block,
            true,
            [9, 0, 9],
            255,
        )
        .expect("add_cursor");
        let v = c.fg_rows[0][0];

        // The wide cursor rendered with cell_width 2 (same-grid cache identity).
        let wide = shared
            .render_glyph(
                Index::special(Special::Sprite),
                Sprite::CursorRect as u32,
                &cursor_opts(&shared, true),
            )
            .unwrap()
            .glyph;
        assert_eq!(v.glyph_pos, [wide.atlas_x, wide.atlas_y]);
        assert_eq!(v.glyph_size, [wide.width, wide.height]);

        // A narrow (cell_width 1) cursor is a different (narrower) glyph.
        let narrow = menlo_grid()
            .render_glyph(
                Index::special(Special::Sprite),
                Sprite::CursorRect as u32,
                &cursor_opts(&shared, false),
            )
            .unwrap()
            .glyph;
        assert_ne!(wide.width, narrow.width);
    }

    #[test]
    fn add_cursor_lock_falls_back_when_glyph_absent() {
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 3));

        // Pre-seed a block cursor, then a Lock cursor: Menlo has no lock symbol
        // (U+F023) and discovery is disabled, so `render_codepoint` returns `None`
        // and the lock cursor draws nothing — clearing the prior cursor and
        // returning `Ok` (upstream's no-cursor outcome). A font embedding the lock
        // glyph would instead draw it via the same vertex path as the sprites.
        add_cursor(
            &mut c,
            &mut shared,
            [2, 1],
            CursorStyle::Block,
            false,
            [9, 0, 9],
            255,
        )
        .expect("add_cursor");
        assert_eq!(c.fg_rows[0].len(), 1);

        add_cursor(
            &mut c,
            &mut shared,
            [2, 1],
            CursorStyle::Lock,
            false,
            [9, 0, 9],
            255,
        )
        .expect("add_cursor");
        assert!(c.fg_rows[0].is_empty());
        assert!(c.fg_rows[4].is_empty());
    }

    #[test]
    fn add_preedit_cell_renders_glyph_and_underline() {
        use crate::font::collection::Index;

        let screen_fg = [9, 8, 7];

        let sprite_glyph = |shared: &mut SharedGrid, sprite: Sprite| {
            let opts = underline_opts(shared);
            shared
                .render_glyph(Index::special(Special::Sprite), sprite as u32, &opts)
                .expect("sprite renders")
                .glyph
        };
        let direct_glyph = |shared: &mut SharedGrid| {
            let opts = RenderOptions {
                grid_metrics: shared.metrics,
                cell_width: None,
                constraint: Constraint::default(),
                constraint_width: 1,
                thicken: false,
                thicken_strength: 255,
            };
            shared
                .render_codepoint('A' as u32, Style::Regular, Some(Presentation::Text), &opts)
                .expect("render ok")
                .expect("'A' present")
                .glyph
        };

        // A narrow preedit 'A' at column 1 → the glyph + one underline at [1, 0].
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 1));
        add_preedit_cell(&mut c, &mut shared, 'A' as u32, false, [1, 0], 4, screen_fg)
            .expect("add_preedit_cell");
        assert_eq!(c.fg_rows[1].len(), 2);
        // The glyph: grayscale, screen_fg at alpha 255, matching a direct render.
        let glyph = c.fg_rows[1][0];
        assert_eq!(glyph.grid_pos, [1, 0]);
        assert_eq!(glyph.atlas, CellTextAtlas::Grayscale);
        assert_eq!(glyph.color, [9, 8, 7, 255]);
        let dg = direct_glyph(&mut shared);
        assert_eq!(glyph.glyph_pos, [dg.atlas_x, dg.atlas_y]);
        assert_eq!(glyph.glyph_size, [dg.width, dg.height]);
        // The underline: a single-underline sprite at [1, 0].
        let underline = c.fg_rows[1][1];
        assert_eq!(underline.grid_pos, [1, 0]);
        let su = sprite_glyph(&mut shared, Sprite::Underline);
        assert_eq!(underline.glyph_pos, [su.atlas_x, su.atlas_y]);

        // A wide preedit cell at column 1 → a second underline at column 2.
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 1));
        add_preedit_cell(&mut c, &mut shared, 'A' as u32, true, [1, 0], 4, screen_fg)
            .expect("add_preedit_cell");
        assert_eq!(c.fg_rows[1].len(), 3);
        let cols: Vec<u16> = c.fg_rows[1].iter().map(|v| v.grid_pos[0]).collect();
        assert_eq!(cols, [1, 1, 2]); // glyph@1, underline@1, underline@2

        // A wide preedit cell in the LAST column → no second underline.
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 1));
        add_preedit_cell(&mut c, &mut shared, 'A' as u32, true, [3, 0], 4, screen_fg)
            .expect("add_preedit_cell");
        assert_eq!(c.fg_rows[1].len(), 2); // glyph + one underline, no second

        // A codepoint no font has → nothing drawn.
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(4, 1));
        add_preedit_cell(&mut c, &mut shared, 0xE000, false, [1, 0], 4, screen_fg)
            .expect("add_preedit_cell");
        assert!(c.fg_rows[1].is_empty());
    }

    #[test]
    fn add_preedit_places_codepoints_with_widths() {
        use crate::renderer::state::Codepoint;

        let screen_fg = [9, 8, 7];
        let cp = |c: char, wide: bool| Codepoint {
            codepoint: c as u32,
            wide,
        };
        // The glyph columns in `fg_rows[1]` (each glyph is followed by its
        // underline(s), so the glyph is at every position whose grid_pos differs
        // from the previous one for a text cell — but here we read all columns).
        let glyph_cols =
            |c: &Contents| -> Vec<u16> { c.fg_rows[1].iter().map(|v| v.grid_pos[0]).collect() };

        // Two narrow codepoints from start column 1 → glyphs at columns 1 and 2,
        // each with its single underline: `[1(glyph), 1(ul), 2(glyph), 2(ul)]`.
        let preedit = Preedit {
            codepoints: vec![cp('A', false), cp('B', false)],
        };
        let range = PreeditRange {
            start: 1,
            end: 2,
            cp_offset: 0,
        };
        let mut shared = menlo_grid();
        let mut c = Contents::default();
        c.resize(grid(8, 1));
        add_preedit(&mut c, &mut shared, &preedit, range, 0, 8, screen_fg).expect("add_preedit");
        assert_eq!(glyph_cols(&c), [1, 1, 2, 2]);

        // `cp_offset = 1` skips the leading codepoint → only `'B'` renders, at the
        // start column 1: `[1(glyph), 1(ul)]`.
        let range = PreeditRange {
            start: 1,
            end: 1,
            cp_offset: 1,
        };
        let mut c = Contents::default();
        c.resize(grid(8, 1));
        add_preedit(&mut c, &mut shared, &preedit, range, 0, 8, screen_fg).expect("add_preedit");
        assert_eq!(glyph_cols(&c), [1, 1]);

        // A wide-then-narrow preedit from start 0 → `'A'` at column 0 (glyph +
        // underline at 0 and a second underline at 1), then `x += 2`, `'B'` at
        // column 2: columns `[0(glyph), 0(ul), 1(ul), 2(glyph), 2(ul)]`.
        let preedit = Preedit {
            codepoints: vec![cp('A', true), cp('B', false)],
        };
        let range = PreeditRange {
            start: 0,
            end: 2,
            cp_offset: 0,
        };
        let mut c = Contents::default();
        c.resize(grid(8, 1));
        add_preedit(&mut c, &mut shared, &preedit, range, 0, 8, screen_fg).expect("add_preedit");
        assert_eq!(glyph_cols(&c), [0, 0, 1, 2, 2]);
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
    fn cell_colors_applies_reverse_video() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(0, 0, 0);

        let styled = |inverse: bool, bg: Color| TermStyle {
            fg_color: Color::Rgb(a),
            bg_color: bg,
            flags: Flags {
                inverse,
                ..Flags::default()
            },
            ..TermStyle::default()
        };

        // 'A' is not a covering codepoint, so the bg twist reduces to `inverse`.
        let plain = 'A' as u32;
        let colors = |inverse, bg, cp| {
            cell_colors(
                styled(inverse, bg),
                cp,
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
            )
        };

        // Non-inverse, explicit bg: colors unchanged.
        assert_eq!(
            colors(false, Color::Rgb(b), plain),
            CellColors { fg: a, bg: Some(b) }
        );
        // Inverse, explicit bg: fg and bg swap.
        assert_eq!(
            colors(true, Color::Rgb(b), plain),
            CellColors { fg: b, bg: Some(a) }
        );
        // Inverse, no bg: the default background fills the foreground.
        assert_eq!(
            colors(true, Color::None, plain),
            CellColors {
                fg: default_bg,
                bg: Some(a)
            }
        );
        // Non-inverse, no bg: background stays the default (None).
        assert_eq!(
            colors(false, Color::None, plain),
            CellColors { fg: a, bg: None }
        );

        // The full block U+2588: the background swaps on `inverse != covering`.
        let block = 0x2588;
        // Non-inverse full block: the block paints via the background with the
        // foreground color (the twist), even without inverse.
        assert_eq!(
            colors(false, Color::Rgb(b), block),
            CellColors { fg: a, bg: Some(a) }
        );
        // Inverse full block: inverse and covering cancel for the background, so
        // it swaps to the explicit background while the foreground still swaps.
        assert_eq!(
            colors(true, Color::Rgb(b), block),
            CellColors { fg: b, bg: Some(b) }
        );
    }

    #[test]
    fn selection_colors_applies_the_selection_arms() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        let c1 = Rgb::new(1, 2, 3);
        let c2 = Rgb::new(4, 5, 6);
        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(7, 8, 9);

        // A cell with an explicit SGR fg=a / bg=b.
        let styled = |inverse: bool, bg: Color| TermStyle {
            fg_color: Color::Rgb(a),
            bg_color: bg,
            flags: Flags {
                inverse,
                ..Flags::default()
            },
            ..TermStyle::default()
        };
        let sel = |inverse, bg, bg_cfg, fg_cfg| {
            selection_colors(
                styled(inverse, bg),
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
                bg_cfg,
                fg_cfg,
            )
        };

        // Default config (None/None): a plain reverse — bg is the default
        // foreground, fg is the default background.
        assert_eq!(
            sel(false, Color::Rgb(b), None, None),
            CellColors {
                fg: default_bg,
                bg: Some(default_fg)
            }
        );

        // Explicit colors are used verbatim.
        assert_eq!(
            sel(
                false,
                Color::Rgb(b),
                Some(SelectionColor::Color(c1)),
                Some(SelectionColor::Color(c2)),
            ),
            CellColors {
                fg: c2,
                bg: Some(c1)
            }
        );

        // CellForeground/CellBackground, non-inverse: the cell's own resolved
        // colors. bg: CellForeground→fg(a), CellBackground→bg(b);
        // fg: CellForeground→fg(a), CellBackground→final_bg(b).
        assert_eq!(
            sel(
                false,
                Color::Rgb(b),
                Some(SelectionColor::CellForeground),
                Some(SelectionColor::CellForeground),
            ),
            CellColors { fg: a, bg: Some(a) }
        );
        assert_eq!(
            sel(
                false,
                Color::Rgb(b),
                Some(SelectionColor::CellBackground),
                Some(SelectionColor::CellBackground),
            ),
            CellColors { fg: b, bg: Some(b) }
        );

        // CellForeground/CellBackground, inverse: the swap.
        // bg: CellForeground→bg(b), CellBackground→fg(a);
        // fg: CellForeground→final_bg(b), CellBackground→fg(a).
        assert_eq!(
            sel(
                true,
                Color::Rgb(b),
                Some(SelectionColor::CellForeground),
                Some(SelectionColor::CellForeground),
            ),
            CellColors { fg: b, bg: Some(b) }
        );
        assert_eq!(
            sel(
                true,
                Color::Rgb(b),
                Some(SelectionColor::CellBackground),
                Some(SelectionColor::CellBackground),
            ),
            CellColors { fg: a, bg: Some(a) }
        );

        // No explicit SGR bg: CellForeground background under inverse yields the
        // cell's (None) background — falls back to the default background; the
        // CellBackground foreground non-inverse yields final_bg = default_bg.
        assert_eq!(
            sel(
                true,
                Color::None,
                Some(SelectionColor::CellForeground),
                None,
            )
            .bg,
            None
        );
        assert_eq!(
            sel(
                false,
                Color::None,
                None,
                Some(SelectionColor::CellBackground),
            )
            .fg,
            default_bg
        );
    }

    #[test]
    fn selected_colors_dispatches_selection_and_search() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(7, 8, 9);
        let amber = Rgb::new(0xFF, 0xE0, 0x82);
        let salmon = Rgb::new(0xF2, 0xA5, 0x7E);
        let black = Rgb::new(0, 0, 0);

        // A cell with an explicit SGR fg=a / bg=b.
        let styled = |inverse: bool| TermStyle {
            fg_color: Color::Rgb(a),
            bg_color: Color::Rgb(b),
            flags: Flags {
                inverse,
                ..Flags::default()
            },
            ..TermStyle::default()
        };
        let cfg = SelectionConfig::default();
        let colors = |selected, inverse, cfg: &SelectionConfig| {
            selected_colors(
                selected,
                styled(inverse),
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
                cfg,
            )
        };

        // False → None (the caller falls back to `cell_colors`).
        assert_eq!(colors(Selected::False, false, &cfg), None);

        // Selection with the default config → a plain reverse (bg = default fg,
        // fg = default bg) — identical to `selection_colors(..., None, None)`.
        assert_eq!(
            colors(Selected::Selection, false, &cfg),
            Some(CellColors {
                fg: default_bg,
                bg: Some(default_fg)
            })
        );

        // Search with the default config → the amber background, black foreground
        // (the `.color` arms).
        assert_eq!(
            colors(Selected::Search, false, &cfg),
            Some(CellColors {
                fg: black,
                bg: Some(amber)
            })
        );

        // SearchSelected with the default config → the salmon background, black
        // foreground.
        assert_eq!(
            colors(Selected::SearchSelected, false, &cfg),
            Some(CellColors {
                fg: black,
                bg: Some(salmon)
            })
        );

        // A search config using CellForeground/CellBackground reuses the same
        // inner switch as selection: non-inverse bg→fg(a), fg→fg(a); inverse
        // swaps. This proves search shares the selection arm (not just `.color`).
        let cell_cfg = SelectionConfig {
            search_background: SelectionColor::CellForeground,
            search_foreground: SelectionColor::CellForeground,
            ..SelectionConfig::default()
        };
        assert_eq!(
            colors(Selected::Search, false, &cell_cfg),
            Some(CellColors { fg: a, bg: Some(a) })
        );
        // Inverse: CellForeground bg → bg_style(b), fg → final_bg(b).
        assert_eq!(
            colors(Selected::Search, true, &cell_cfg),
            Some(CellColors { fg: b, bg: Some(b) })
        );
    }

    #[test]
    fn cursor_text_color_resolves_the_cursor_text_config() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        let c1 = Rgb::new(1, 2, 3);
        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(7, 8, 9);

        // The under-cursor cell: explicit SGR fg=a / bg=b.
        let styled = |inverse: bool, bg: Color| TermStyle {
            fg_color: Color::Rgb(a),
            bg_color: bg,
            flags: Flags {
                inverse,
                ..Flags::default()
            },
            ..TermStyle::default()
        };
        let color = |inverse, bg, cfg| {
            cursor_text_color(
                styled(inverse, bg),
                cfg,
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
            )
        };

        // No cursor-text config → the default background.
        assert_eq!(color(false, Color::Rgb(b), None), default_bg);
        // An explicit color is used verbatim.
        assert_eq!(
            color(false, Color::Rgb(b), Some(SelectionColor::Color(c1))),
            c1
        );

        // CellForeground: the cell's foreground (a); inverse swaps to its
        // background (b).
        assert_eq!(
            color(false, Color::Rgb(b), Some(SelectionColor::CellForeground)),
            a
        );
        assert_eq!(
            color(true, Color::Rgb(b), Some(SelectionColor::CellForeground)),
            b
        );
        // CellBackground: the cell's background (b); inverse swaps to its
        // foreground (a).
        assert_eq!(
            color(false, Color::Rgb(b), Some(SelectionColor::CellBackground)),
            b
        );
        assert_eq!(
            color(true, Color::Rgb(b), Some(SelectionColor::CellBackground)),
            a
        );

        // No explicit SGR background: CellBackground non-inverse falls back to the
        // default background.
        assert_eq!(
            color(false, Color::None, Some(SelectionColor::CellBackground)),
            default_bg
        );
    }

    #[test]
    fn cursor_color_resolves_with_precedence() {
        use crate::terminal::color::DEFAULT_PALETTE;
        use crate::terminal::style::{Color, Flags, Style as TermStyle};

        let a = Rgb::new(10, 20, 30);
        let b = Rgb::new(40, 50, 60);
        let c1 = Rgb::new(1, 2, 3);
        let osc = Rgb::new(99, 88, 77);
        let default_fg = Rgb::new(200, 200, 200);
        let default_bg = Rgb::new(7, 8, 9);

        // The under-cursor cell: explicit SGR fg=a / bg=b.
        let styled = |inverse: bool, bg: Color| TermStyle {
            fg_color: Color::Rgb(a),
            bg_color: bg,
            flags: Flags {
                inverse,
                ..Flags::default()
            },
            ..TermStyle::default()
        };
        let color = |osc12, inverse, bg, cfg| {
            cursor_color(
                osc12,
                cfg,
                styled(inverse, bg),
                default_fg,
                default_bg,
                &DEFAULT_PALETTE,
                None,
            )
        };

        // OSC 12 takes precedence, even with a config set.
        assert_eq!(
            color(
                Some(osc),
                false,
                Color::Rgb(b),
                Some(SelectionColor::Color(c1))
            ),
            osc
        );

        // No OSC 12, no config → the default foreground (NOT the background).
        assert_eq!(color(None, false, Color::Rgb(b), None), default_fg);
        // An explicit color is used verbatim.
        assert_eq!(
            color(None, false, Color::Rgb(b), Some(SelectionColor::Color(c1))),
            c1
        );

        // CellForeground: the cell's foreground (a); inverse swaps to its
        // background (b).
        assert_eq!(
            color(
                None,
                false,
                Color::Rgb(b),
                Some(SelectionColor::CellForeground)
            ),
            a
        );
        assert_eq!(
            color(
                None,
                true,
                Color::Rgb(b),
                Some(SelectionColor::CellForeground)
            ),
            b
        );
        // CellBackground: the cell's background (b); inverse swaps to its
        // foreground (a).
        assert_eq!(
            color(
                None,
                false,
                Color::Rgb(b),
                Some(SelectionColor::CellBackground)
            ),
            b
        );
        assert_eq!(
            color(
                None,
                true,
                Color::Rgb(b),
                Some(SelectionColor::CellBackground)
            ),
            a
        );

        // No explicit SGR background: CellBackground non-inverse falls back to the
        // default background.
        assert_eq!(
            color(
                None,
                false,
                Color::None,
                Some(SelectionColor::CellBackground)
            ),
            default_bg
        );
    }

    #[test]
    fn block_cursor_pos_adjusts_for_wide_kind() {
        // Narrow / spacer head / wide keep the column; the cursor is "wide" only
        // for a wide cell or its spacer tail.
        assert_eq!(block_cursor_pos(5, 2, Wide::Narrow), ([5, 2], false));
        assert_eq!(block_cursor_pos(5, 2, Wide::Wide), ([5, 2], true));
        assert_eq!(block_cursor_pos(5, 2, Wide::SpacerHead), ([5, 2], false));

        // A spacer tail moves the cursor back one column (it sits over the wide
        // character) and is wide.
        assert_eq!(block_cursor_pos(5, 2, Wide::SpacerTail), ([4, 2], true));

        // Saturating: a spacer tail at column 0 does not underflow.
        assert_eq!(block_cursor_pos(0, 0, Wide::SpacerTail), ([0, 0], true));
    }

    #[test]
    fn link_underline_applies_the_hovered_link_override() {
        // A non-link cell keeps its SGR underline (every variant unchanged).
        for u in [
            Underline::None,
            Underline::Single,
            Underline::Double,
            Underline::Curly,
            Underline::Dotted,
            Underline::Dashed,
        ] {
            assert_eq!(link_underline(false, u), u, "non-link {u:?}");
        }

        // A link cell with a single underline → double (to distinguish it).
        assert_eq!(link_underline(true, Underline::Single), Underline::Double);
        // A link cell with no underline → a single underline.
        assert_eq!(link_underline(true, Underline::None), Underline::Single);
        // A link cell with any other underline → a single underline.
        for u in [
            Underline::Double,
            Underline::Curly,
            Underline::Dotted,
            Underline::Dashed,
        ] {
            assert_eq!(link_underline(true, u), Underline::Single, "link {u:?}");
        }
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
