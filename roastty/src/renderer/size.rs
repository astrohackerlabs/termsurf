#![allow(dead_code)]
// Renderer sizing value types are consumed by later renderer slices.

//! Renderer sizing value types.
//!
//! Faithful port of the value types in upstream `renderer/size.zig`: the
//! `CellSize`, `ScreenSize`, `GridSize`, and `Padding` pixel/grid arithmetic.
//! The `Size` aggregate, the `Coordinate` conversions, and the `PaddingBalance`
//! enum build on these and are ported separately.

/// Grid dimension unit. Mirrors `terminal::size::CellCountInt` (`u16`), which is
/// private to the terminal module.
pub(crate) type Unit = u16;

/// The pixel size of a single glyph cell.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct CellSize {
    pub width: u32,
    pub height: u32,
}

/// The dimensions of the screen that the grid is rendered to, in pixels. This is
/// the terminal screen, likely a subset of the window size.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct ScreenSize {
    pub width: u32,
    pub height: u32,
}

impl ScreenSize {
    /// Subtract padding from the screen size (saturating).
    pub(crate) fn sub_padding(self, padding: Padding) -> ScreenSize {
        ScreenSize {
            width: self.width.saturating_sub(padding.left + padding.right),
            height: self.height.saturating_sub(padding.top + padding.bottom),
        }
    }

    /// Calculates the amount of blank space around the grid. This is possible
    /// when padding isn't balanced. `self` here should be the unpadded screen.
    pub(crate) fn blank_padding(self, padding: Padding, grid: GridSize, cell: CellSize) -> Padding {
        let grid_width = grid.columns as u32 * cell.width;
        let grid_height = grid.rows as u32 * cell.height;
        let padded_width = grid_width + (padding.left + padding.right);
        let padded_height = grid_height + (padding.top + padding.bottom);

        // Saturating subtraction avoids underflow: padding can make the padded
        // sizes larger than the real screen when the screen is shrunk to a
        // minimal size such as 1x1.
        let leftover_width = self.width.saturating_sub(padded_width);
        let leftover_height = self.height.saturating_sub(padded_height);

        Padding {
            top: 0,
            bottom: leftover_height,
            right: leftover_width,
            left: 0,
        }
    }

    /// Returns true if two sizes are equal.
    pub(crate) fn equals(self, other: ScreenSize) -> bool {
        self == other
    }
}

/// The dimensions of the grid itself, in rows/columns units.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GridSize {
    pub columns: Unit,
    pub rows: Unit,
}

impl GridSize {
    /// Initialize a grid size based on a screen and cell size.
    pub(crate) fn init(screen: ScreenSize, cell: CellSize) -> GridSize {
        let mut result = GridSize::default();
        result.update(screen, cell);
        result
    }

    /// Update the columns/rows for the grid based on the given screen and cell
    /// size.
    pub(crate) fn update(&mut self, screen: ScreenSize, cell: CellSize) {
        let cell_width = cell.width as f32;
        let cell_height = cell.height as f32;
        let screen_width = screen.width as f32;
        let screen_height = screen.height as f32;
        // `as` truncates toward zero (matching Zig `@intFromFloat`); it also
        // saturates impossible out-of-range quotients, an accepted divergence.
        let calc_cols = (screen_width / cell_width) as Unit;
        let calc_rows = (screen_height / cell_height) as Unit;
        self.columns = calc_cols.max(1);
        self.rows = calc_rows.max(1);
    }

    /// Returns true if two sizes are equal.
    pub(crate) fn equals(self, other: GridSize) -> bool {
        self == other
    }
}

/// The padding to add to a screen.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Padding {
    pub top: u32,
    pub bottom: u32,
    pub right: u32,
    pub left: u32,
}

impl Padding {
    /// Returns padding that balances the whitespace around the screen for the
    /// given grid and cell sizes.
    pub(crate) fn balanced(screen: ScreenSize, grid: GridSize, cell: CellSize) -> Padding {
        let cell_width = cell.width as f32;
        let cell_height = cell.height as f32;

        // The size of our full grid.
        let grid_width = grid.columns as f32 * cell_width;
        let grid_height = grid.rows as f32 * cell_height;

        // The empty space to the right of a line and bottom of the last row.
        let space_right = screen.width as f32 - grid_width;
        let space_bot = screen.height as f32 - grid_height;

        // The padding is split equally along both axes.
        let padding_right = (space_right / 2.0).floor();
        let padding_left = padding_right;
        let padding_bot = (space_bot / 2.0).floor();
        let padding_top = padding_bot;

        Padding {
            top: padding_top.max(0.0) as u32,
            bottom: padding_bot.max(0.0) as u32,
            right: padding_right.max(0.0) as u32,
            left: padding_left.max(0.0) as u32,
        }
    }

    /// Add another padding to this one.
    pub(crate) fn add(self, other: Padding) -> Padding {
        Padding {
            top: self.top + other.top,
            bottom: self.bottom + other.bottom,
            right: self.right + other.right,
            left: self.left + other.left,
        }
    }

    /// Equality test between two paddings.
    pub(crate) fn eql(self, other: Padding) -> bool {
        self == other
    }
}

/// How to balance padding around the grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaddingBalance {
    /// No balancing; padding is applied as specified explicitly.
    False,
    /// Balances padding but caps the top padding so the first row doesn't drift
    /// too far from the top of the window. Excess vertical space is shifted to
    /// the bottom.
    True,
    /// Distributes leftover space equally on all sides so the grid is centered
    /// within the screen.
    Equal,
}

/// All relevant sizes for a rendered terminal — enough to convert between any of
/// the coordinate systems. Pixel values are assumed already scaled to the
/// current DPI; the caller recalculates on DPI change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Size {
    pub screen: ScreenSize,
    pub cell: CellSize,
    pub padding: Padding,
}

impl Size {
    /// The grid size: the screen minus padding, divided by the cell size.
    pub(crate) fn grid(self) -> GridSize {
        GridSize::init(self.screen.sub_padding(self.padding), self.cell)
    }

    /// The size of the terminal: the screen without padding.
    pub(crate) fn terminal(self) -> ScreenSize {
        self.screen.sub_padding(self.padding)
    }

    /// Set the padding to be balanced around the grid. Balanced padding is
    /// calculated AFTER the explicit padding is taken into account.
    pub(crate) fn balance_padding(&mut self, explicit: Padding, mode: PaddingBalance) {
        // Ensure grid() accounts for the explicit padding.
        self.padding = explicit;

        // Now calculate the balanced padding.
        self.padding = Padding::balanced(self.screen, self.grid(), self.cell);

        match mode {
            PaddingBalance::False => unreachable!(),
            PaddingBalance::Equal => {}
            PaddingBalance::True => {
                // Cap the top padding to avoid excessive space above the first
                // row. The maximum is the balanced explicit horizontal padding
                // plus half a cell width; any excess shifts to the bottom.
                let max_top = (explicit.left + explicit.right + self.cell.width) / 2;
                let vshift = self.padding.top.saturating_sub(max_top);
                self.padding.top -= vshift;
                self.padding.bottom += vshift;
            }
        }
    }
}

/// The coordinate system a [`Coordinate`] is expressed in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CoordinateTag {
    Surface,
    Terminal,
    Grid,
}

/// A coordinate in one of the renderer coordinate systems. Only valid within the
/// context of a stable [`Size`]; if any size changes, the coordinate must be
/// recalculated.
///
///   * `Surface`: (0, 0) is the top-left of the surface (with padding); pixels,
///     may be negative or past the surface.
///   * `Terminal`: the surface with padding removed; pixels.
///   * `Grid`: (0, 0) is the top-left of the grid; cells, non-negative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Coordinate {
    Surface { x: f64, y: f64 },
    Terminal { x: f64, y: f64 },
    Grid { x: Unit, y: Unit },
}

impl Coordinate {
    fn tag(self) -> CoordinateTag {
        match self {
            Coordinate::Surface { .. } => CoordinateTag::Surface,
            Coordinate::Terminal { .. } => CoordinateTag::Terminal,
            Coordinate::Grid { .. } => CoordinateTag::Grid,
        }
    }

    /// Convert this coordinate to a different system within the same [`Size`].
    pub(crate) fn convert(self, to: CoordinateTag, size: Size) -> Coordinate {
        // Unlikely fast-path but avoids work.
        if self.tag() == to {
            return self;
        }

        // Normalize to the surface system, then reconvert from there, to avoid a
        // combinatorial explosion of conversion functions.
        let (sx, sy) = self.convert_to_surface(size);

        match to {
            CoordinateTag::Surface => Coordinate::Surface { x: sx, y: sy },
            CoordinateTag::Terminal => Coordinate::Terminal {
                x: sx - size.padding.left as f64,
                y: sy - size.padding.top as f64,
            },
            CoordinateTag::Grid => {
                // Get rid of the padding (surface -> terminal).
                let term_x = sx - size.padding.left as f64;
                let term_y = sy - size.padding.top as f64;

                let grid = size.grid();
                let cell_width = size.cell.width as f64;
                let cell_height = size.cell.height as f64;
                let clamped_x = term_x.max(0.0);
                let clamped_y = term_y.max(0.0);
                let col = (clamped_x / cell_width) as Unit;
                let row = (clamped_y / cell_height) as Unit;
                let clamped_col = col.min(grid.columns - 1);
                let clamped_row = row.min(grid.rows - 1);
                Coordinate::Grid {
                    x: clamped_col,
                    y: clamped_row,
                }
            }
        }
    }

    /// Convert this coordinate to the surface system.
    fn convert_to_surface(self, size: Size) -> (f64, f64) {
        match self {
            Coordinate::Surface { x, y } => (x, y),
            Coordinate::Terminal { x, y } => {
                (x + size.padding.left as f64, y + size.padding.top as f64)
            }
            Coordinate::Grid { x, y } => {
                let col = x as f64;
                let row = y as f64;
                let cell_width = size.cell.width as f64;
                let cell_height = size.cell.height as f64;
                (
                    col * cell_width + size.padding.left as f64,
                    row * cell_height + size.padding.top as f64,
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Upstream "Padding balanced on zero": a zero-sized screen yields no
    // negative padding.
    #[test]
    fn padding_balanced_on_zero() {
        let grid = GridSize {
            columns: 100,
            rows: 37,
        };
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let screen = ScreenSize {
            width: 0,
            height: 0,
        };
        assert_eq!(Padding::balanced(screen, grid, cell), Padding::default());
    }

    #[test]
    fn padding_balanced_nonzero() {
        // grid 100x80, screen 110x100 -> leftover 10 horizontal, 20 vertical.
        let grid = GridSize {
            columns: 10,
            rows: 4,
        };
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let screen = ScreenSize {
            width: 110,
            height: 100,
        };
        let padding = Padding::balanced(screen, grid, cell);
        assert_eq!(padding.left, padding.right);
        assert_eq!(padding.top, padding.bottom);
        assert_eq!(padding.left, 5);
        assert_eq!(padding.top, 10);
    }

    // Proves `floor` (not round/ceil): an odd 5px leftover yields 2px per side.
    #[test]
    fn padding_balanced_floor_odd_leftover() {
        let grid = GridSize {
            columns: 10,
            rows: 4,
        };
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        // grid 100x80; screen 105x80 -> horizontal leftover 5, vertical 0.
        let screen = ScreenSize {
            width: 105,
            height: 80,
        };
        let padding = Padding::balanced(screen, grid, cell);
        assert_eq!(padding.right, 2);
        assert_eq!(padding.left, 2);
        assert_eq!(padding.top, 0);
        assert_eq!(padding.bottom, 0);
    }

    // Upstream "GridSize update exact".
    #[test]
    fn grid_size_update_exact() {
        let mut grid = GridSize::default();
        grid.update(
            ScreenSize {
                width: 100,
                height: 40,
            },
            CellSize {
                width: 5,
                height: 10,
            },
        );
        assert_eq!(grid.columns, 20);
        assert_eq!(grid.rows, 4);
    }

    // Upstream "GridSize update rounding".
    #[test]
    fn grid_size_update_rounding() {
        let mut grid = GridSize::default();
        grid.update(
            ScreenSize {
                width: 20,
                height: 40,
            },
            CellSize {
                width: 6,
                height: 15,
            },
        );
        assert_eq!(grid.columns, 3);
        assert_eq!(grid.rows, 2);
    }

    #[test]
    fn grid_size_update_min_one() {
        // A screen smaller than a single cell still yields a 1x1 grid.
        let grid = GridSize::init(
            ScreenSize {
                width: 3,
                height: 3,
            },
            CellSize {
                width: 10,
                height: 10,
            },
        );
        assert_eq!(grid.columns, 1);
        assert_eq!(grid.rows, 1);
    }

    #[test]
    fn screen_sub_padding_saturates() {
        let screen = ScreenSize {
            width: 5,
            height: 5,
        };
        let padding = Padding {
            top: 10,
            bottom: 10,
            right: 10,
            left: 10,
        };
        assert_eq!(
            screen.sub_padding(padding),
            ScreenSize {
                width: 0,
                height: 0
            }
        );
    }

    #[test]
    fn screen_blank_padding() {
        // Unpadded screen larger than the grid: leftover on right/bottom only.
        let screen = ScreenSize {
            width: 110,
            height: 90,
        };
        let grid = GridSize {
            columns: 10,
            rows: 4,
        };
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let padding = screen.blank_padding(Padding::default(), grid, cell);
        assert_eq!(
            padding,
            Padding {
                top: 0,
                bottom: 10,
                right: 10,
                left: 0,
            }
        );
    }

    #[test]
    fn screen_blank_padding_saturates() {
        // Padded grid larger than the screen: right/bottom saturate to 0.
        let screen = ScreenSize {
            width: 50,
            height: 50,
        };
        let grid = GridSize {
            columns: 10,
            rows: 4,
        };
        let cell = CellSize {
            width: 10,
            height: 20,
        };
        let padding = screen.blank_padding(Padding::default(), grid, cell);
        assert_eq!(padding, Padding::default());
    }

    #[test]
    fn padding_add() {
        let a = Padding {
            top: 1,
            bottom: 2,
            right: 3,
            left: 4,
        };
        let b = Padding {
            top: 10,
            bottom: 20,
            right: 30,
            left: 40,
        };
        assert_eq!(
            a.add(b),
            Padding {
                top: 11,
                bottom: 22,
                right: 33,
                left: 44,
            }
        );
    }

    #[test]
    fn padding_eql() {
        let a = Padding {
            top: 1,
            bottom: 2,
            right: 3,
            left: 4,
        };
        assert!(a.eql(a));
        assert!(!a.eql(Padding::default()));
    }

    #[test]
    fn size_equals_helpers() {
        let s = ScreenSize {
            width: 10,
            height: 20,
        };
        assert!(s.equals(s));
        assert!(!s.equals(ScreenSize::default()));

        let g = GridSize {
            columns: 4,
            rows: 5,
        };
        assert!(g.equals(g));
        assert!(!g.equals(GridSize::default()));
    }

    // Upstream "Size.balancePadding equal distributes whitespace equally".
    #[test]
    fn size_balance_padding_equal_distributes_whitespace_equally() {
        let mut size = Size {
            screen: ScreenSize {
                width: 1050,
                height: 850,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding::default(),
        };
        size.balance_padding(
            Padding {
                top: 4,
                bottom: 4,
                left: 4,
                right: 4,
            },
            PaddingBalance::Equal,
        );
        assert_eq!(size.padding.left, size.padding.right);
        assert_eq!(size.padding.top, size.padding.bottom);
        assert!(size.padding.top > 0);
    }

    // Upstream "Size.balancePadding true shifts excess top to bottom".
    #[test]
    fn size_balance_padding_true_shifts_excess_top_to_bottom() {
        let mut size = Size {
            screen: ScreenSize {
                width: 1090,
                height: 1070,
            },
            cell: CellSize {
                width: 20,
                height: 40,
            },
            padding: Padding::default(),
        };
        size.balance_padding(Padding::default(), PaddingBalance::True);
        assert_eq!(size.padding.left, size.padding.right);
        assert!(size.padding.top < size.padding.bottom);
        assert_eq!(size.padding.top, 10);
        assert_eq!(size.padding.bottom, 20);
    }

    #[test]
    fn size_grid_and_terminal() {
        let size = Size {
            screen: ScreenSize {
                width: 100,
                height: 100,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding {
                top: 5,
                bottom: 5,
                left: 10,
                right: 10,
            },
        };
        // terminal = screen - padding = 80 x 90.
        assert_eq!(
            size.terminal(),
            ScreenSize {
                width: 80,
                height: 90
            }
        );
        // grid = 80/10 = 8 columns, 90/20 = 4 rows.
        assert_eq!(
            size.grid(),
            GridSize {
                columns: 8,
                rows: 4
            }
        );
    }

    // Upstream "coordinate conversion": surface -> grid with clamping.
    #[test]
    fn coordinate_conversion() {
        let size = Size {
            screen: ScreenSize {
                width: 100,
                height: 100,
            },
            cell: CellSize {
                width: 5,
                height: 10,
            },
            padding: Padding::default(),
        };
        let cols = size.grid().columns;
        let rows = size.grid().rows;
        let cases = [
            ((0.0, 0.0), (0u16, 0u16)),
            ((6.0, 0.0), (1, 0)),
            ((6.0, 10.0), (1, 1)),
            ((-10.0, -10.0), (0, 0)),
            ((100_000.0, 100_000.0), (cols - 1, rows - 1)),
        ];
        for ((sx, sy), (gx, gy)) in cases {
            let actual = (Coordinate::Surface { x: sx, y: sy }).convert(CoordinateTag::Grid, size);
            assert_eq!(actual, Coordinate::Grid { x: gx, y: gy });
        }
    }

    #[test]
    fn coordinate_surface_terminal_round_trip() {
        let size = Size {
            screen: ScreenSize {
                width: 100,
                height: 100,
            },
            cell: CellSize {
                width: 10,
                height: 20,
            },
            padding: Padding {
                top: 5,
                bottom: 0,
                left: 7,
                right: 0,
            },
        };
        let surface = Coordinate::Surface { x: 42.0, y: 24.0 };
        let term = surface.convert(CoordinateTag::Terminal, size);
        assert_eq!(term, Coordinate::Terminal { x: 35.0, y: 19.0 });
        assert_eq!(term.convert(CoordinateTag::Surface, size), surface);
    }
}
