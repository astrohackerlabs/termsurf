//! Procedural box-drawing and block glyphs.
//!
//! Faithful port of upstream `font/sprite/draw/box.zig` (the `linesChar` line
//! primitive, the dash primitives) and `block.zig` (the Block Elements), plus
//! the shared `font/sprite/draw/common.zig` primitives (`Thickness`,
//! `Fraction`/`fill`, the `hline`/`vline` helpers, `Shade`/`Alignment`/`Quads`).
//! Covered so far: the box-drawing line glyphs (`U+2500`–`U+257F` `linesChar`
//! dispatch), the dashes, the Block Elements (`U+2580`–`U+259F`), the Braille
//! Patterns (`U+2800`–`U+28FF`), the legacy-computing Sextants
//! (`U+1FB00`–`U+1FB3B`), the Separated Block Quadrants (`U+1CC21`–`U+1CC2F`),
//! and the Octants (`U+1CD00`–`U+1CDE5`), plus the box-drawing **diagonals**
//! (`U+2571`–`U+2573`, the first `z2d`-rendered glyphs). The remaining
//! `z2d`-based primitives (arcs, the circle/ellipse pieces), the sprite
//! `hasCodepoint` inventory, and the other sprite categories (powerline, the
//! rest of legacy-computing, geometric) are later experiments.

use crate::font::metrics::Metrics;
use crate::font::sprite::canvas::{Canvas, Color, Point, Rect, Triangle};
use crate::font::sprite::raster;

/// Stroke thickness class. Faithful port of upstream `common.Thickness`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Thickness {
    SuperLight,
    Light,
    Heavy,
}

impl Thickness {
    /// The pixel height of a stroke of this thickness given a `base` (the
    /// font's `box_thickness`). Faithful port of `Thickness.height`.
    pub(crate) fn height(self, base: u32) -> u32 {
        match self {
            Thickness::SuperLight => (base / 2).max(1),
            Thickness::Light => base,
            Thickness::Heavy => base * 2,
        }
    }
}

/// A value that indicates some fraction across the cell, either horizontally or
/// vertically. Faithful port of upstream `common.Fraction`; the redundant names
/// exist so callers can use whichever reads most naturally, and collapse to the
/// same value only in [`fraction`](Fraction::fraction).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fraction {
    // Names for the min edge
    Start,
    Left,
    Top,
    Zero,
    // Names based on eighths
    Eighth,
    OneEighth,
    TwoEighths,
    ThreeEighths,
    FourEighths,
    FiveEighths,
    SixEighths,
    SevenEighths,
    // Names based on quarters
    Quarter,
    OneQuarter,
    TwoQuarters,
    ThreeQuarters,
    // Names based on thirds
    Third,
    OneThird,
    TwoThirds,
    // Names based on halves
    Half,
    OneHalf,
    // Alternative names for 1/2
    Center,
    Middle,
    // Names for the max edge
    End,
    Right,
    Bottom,
    One,
    Full,
}

impl Fraction {
    /// The `f64` value of this fraction.
    pub(crate) fn fraction(self) -> f64 {
        match self {
            Fraction::Start | Fraction::Left | Fraction::Top | Fraction::Zero => 0.0,
            Fraction::Eighth | Fraction::OneEighth => 0.125,
            Fraction::Quarter | Fraction::OneQuarter | Fraction::TwoEighths => 0.25,
            Fraction::Third | Fraction::OneThird => 1.0 / 3.0,
            Fraction::ThreeEighths => 0.375,
            Fraction::Half
            | Fraction::OneHalf
            | Fraction::TwoQuarters
            | Fraction::FourEighths
            | Fraction::Center
            | Fraction::Middle => 0.5,
            Fraction::FiveEighths => 0.625,
            Fraction::TwoThirds => 2.0 / 3.0,
            Fraction::ThreeQuarters | Fraction::SixEighths => 0.75,
            Fraction::SevenEighths => 0.875,
            Fraction::End | Fraction::Right | Fraction::Bottom | Fraction::One | Fraction::Full => {
                1.0
            }
        }
    }

    /// The left/top pixel for this fraction across `size`, used as the **min**
    /// (left/top) coordinate of a block. Rounds the complementary fraction from
    /// the far edge so adjacent blocks tile evenly. Faithful port of
    /// `Fraction.min`.
    pub(crate) fn min(self, size: u32) -> i32 {
        let s = size as f64;
        (s - ((1.0 - self.fraction()) * s).round()) as i32
    }

    /// The right/bottom pixel for this fraction across `size`, used as the
    /// **max** (right/bottom) coordinate of a block. Faithful port of
    /// `Fraction.max`.
    pub(crate) fn max(self, size: u32) -> i32 {
        (self.fraction() * size as f64).round() as i32
    }

    /// This fraction across `size`, unrounded (for path drawing). Faithful port
    /// of `Fraction.float`.
    pub(crate) fn float(self, size: u32) -> f64 {
        self.fraction() * size as f64
    }
}

/// Fill the rectangle between a horizontal and vertical pair of fraction lines.
/// Faithful port of upstream `common.fill`.
fn fill(
    metrics: &Metrics,
    canvas: &mut Canvas,
    x0: Fraction,
    x1: Fraction,
    y0: Fraction,
    y1: Fraction,
) {
    canvas.r#box(
        x0.min(metrics.cell_width),
        y0.min(metrics.cell_height),
        x1.max(metrics.cell_width),
        y1.max(metrics.cell_height),
        Color::ON,
    );
}

/// The style of a single line in a direction. Faithful port of upstream
/// `box.Lines.Style` (`enum(u2) { none, light, heavy, double }`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum LineStyle {
    #[default]
    None,
    Light,
    Heavy,
    Double,
}

/// The four directional line styles meeting at the cell center. Faithful port
/// of upstream `box.Lines`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Lines {
    pub up: LineStyle,
    pub right: LineStyle,
    pub down: LineStyle,
    pub left: LineStyle,
}

/// Draw the box-drawing line glyph described by `lines` into `canvas`. Faithful
/// port of upstream `linesChar`: it computes the light/heavy/double stroke edges
/// and the meeting points where perpendicular strokes join, then draws a filled
/// rectangle for each non-`None` direction (a `Double` direction draws two
/// parallel strokes). All arithmetic is saturating, matching Zig's `-|`/`+|`.
pub(crate) fn lines_char(metrics: &Metrics, canvas: &mut Canvas, lines: Lines) {
    let light_px = Thickness::Light.height(metrics.box_thickness);
    let heavy_px = Thickness::Heavy.height(metrics.box_thickness);

    // Top of light horizontal strokes
    let h_light_top = (metrics.cell_height.saturating_sub(light_px)) / 2;
    // Bottom of light horizontal strokes
    let h_light_bottom = h_light_top.saturating_add(light_px);

    // Top of heavy horizontal strokes
    let h_heavy_top = (metrics.cell_height.saturating_sub(heavy_px)) / 2;
    // Bottom of heavy horizontal strokes
    let h_heavy_bottom = h_heavy_top.saturating_add(heavy_px);

    // Top of the top doubled horizontal stroke (bottom is `h_light_top`)
    let h_double_top = h_light_top.saturating_sub(light_px);
    // Bottom of the bottom doubled horizontal stroke (top is `h_light_bottom`)
    let h_double_bottom = h_light_bottom.saturating_add(light_px);

    // Left of light vertical strokes
    let v_light_left = (metrics.cell_width.saturating_sub(light_px)) / 2;
    // Right of light vertical strokes
    let v_light_right = v_light_left.saturating_add(light_px);

    // Left of heavy vertical strokes
    let v_heavy_left = (metrics.cell_width.saturating_sub(heavy_px)) / 2;
    // Right of heavy vertical strokes
    let v_heavy_right = v_heavy_left.saturating_add(heavy_px);

    // Left of the left doubled vertical stroke (right is `v_light_left`)
    let v_double_left = v_light_left.saturating_sub(light_px);
    // Right of the right doubled vertical stroke (left is `v_light_right`)
    let v_double_right = v_light_right.saturating_add(light_px);

    // The bottom of the up line
    let up_bottom = if lines.left == LineStyle::Heavy || lines.right == LineStyle::Heavy {
        h_heavy_bottom
    } else if lines.left != lines.right || lines.down == lines.up {
        if lines.left == LineStyle::Double || lines.right == LineStyle::Double {
            h_double_bottom
        } else {
            h_light_bottom
        }
    } else if lines.left == LineStyle::None && lines.right == LineStyle::None {
        h_light_bottom
    } else {
        h_light_top
    };

    // The top of the down line
    let down_top = if lines.left == LineStyle::Heavy || lines.right == LineStyle::Heavy {
        h_heavy_top
    } else if lines.left != lines.right || lines.up == lines.down {
        if lines.left == LineStyle::Double || lines.right == LineStyle::Double {
            h_double_top
        } else {
            h_light_top
        }
    } else if lines.left == LineStyle::None && lines.right == LineStyle::None {
        h_light_top
    } else {
        h_light_bottom
    };

    // The right of the left line
    let left_right = if lines.up == LineStyle::Heavy || lines.down == LineStyle::Heavy {
        v_heavy_right
    } else if lines.up != lines.down || lines.left == lines.right {
        if lines.up == LineStyle::Double || lines.down == LineStyle::Double {
            v_double_right
        } else {
            v_light_right
        }
    } else if lines.up == LineStyle::None && lines.down == LineStyle::None {
        v_light_right
    } else {
        v_light_left
    };

    // The left of the right line
    let right_left = if lines.up == LineStyle::Heavy || lines.down == LineStyle::Heavy {
        v_heavy_left
    } else if lines.up != lines.down || lines.right == lines.left {
        if lines.up == LineStyle::Double || lines.down == LineStyle::Double {
            v_double_left
        } else {
            v_light_left
        }
    } else if lines.up == LineStyle::None && lines.down == LineStyle::None {
        v_light_left
    } else {
        v_light_right
    };

    match lines.up {
        LineStyle::None => {}
        LineStyle::Light => canvas.r#box(
            v_light_left as i32,
            0,
            v_light_right as i32,
            up_bottom as i32,
            Color::ON,
        ),
        LineStyle::Heavy => canvas.r#box(
            v_heavy_left as i32,
            0,
            v_heavy_right as i32,
            up_bottom as i32,
            Color::ON,
        ),
        LineStyle::Double => {
            let left_bottom = if lines.left == LineStyle::Double {
                h_light_top
            } else {
                up_bottom
            };
            let right_bottom = if lines.right == LineStyle::Double {
                h_light_top
            } else {
                up_bottom
            };

            canvas.r#box(
                v_double_left as i32,
                0,
                v_light_left as i32,
                left_bottom as i32,
                Color::ON,
            );
            canvas.r#box(
                v_light_right as i32,
                0,
                v_double_right as i32,
                right_bottom as i32,
                Color::ON,
            );
        }
    }

    match lines.right {
        LineStyle::None => {}
        LineStyle::Light => canvas.r#box(
            right_left as i32,
            h_light_top as i32,
            metrics.cell_width as i32,
            h_light_bottom as i32,
            Color::ON,
        ),
        LineStyle::Heavy => canvas.r#box(
            right_left as i32,
            h_heavy_top as i32,
            metrics.cell_width as i32,
            h_heavy_bottom as i32,
            Color::ON,
        ),
        LineStyle::Double => {
            let top_left = if lines.up == LineStyle::Double {
                v_light_right
            } else {
                right_left
            };
            let bottom_left = if lines.down == LineStyle::Double {
                v_light_right
            } else {
                right_left
            };

            canvas.r#box(
                top_left as i32,
                h_double_top as i32,
                metrics.cell_width as i32,
                h_light_top as i32,
                Color::ON,
            );
            canvas.r#box(
                bottom_left as i32,
                h_light_bottom as i32,
                metrics.cell_width as i32,
                h_double_bottom as i32,
                Color::ON,
            );
        }
    }

    match lines.down {
        LineStyle::None => {}
        LineStyle::Light => canvas.r#box(
            v_light_left as i32,
            down_top as i32,
            v_light_right as i32,
            metrics.cell_height as i32,
            Color::ON,
        ),
        LineStyle::Heavy => canvas.r#box(
            v_heavy_left as i32,
            down_top as i32,
            v_heavy_right as i32,
            metrics.cell_height as i32,
            Color::ON,
        ),
        LineStyle::Double => {
            let left_top = if lines.left == LineStyle::Double {
                h_light_bottom
            } else {
                down_top
            };
            let right_top = if lines.right == LineStyle::Double {
                h_light_bottom
            } else {
                down_top
            };

            canvas.r#box(
                v_double_left as i32,
                left_top as i32,
                v_light_left as i32,
                metrics.cell_height as i32,
                Color::ON,
            );
            canvas.r#box(
                v_light_right as i32,
                right_top as i32,
                v_double_right as i32,
                metrics.cell_height as i32,
                Color::ON,
            );
        }
    }

    match lines.left {
        LineStyle::None => {}
        LineStyle::Light => canvas.r#box(
            0,
            h_light_top as i32,
            left_right as i32,
            h_light_bottom as i32,
            Color::ON,
        ),
        LineStyle::Heavy => canvas.r#box(
            0,
            h_heavy_top as i32,
            left_right as i32,
            h_heavy_bottom as i32,
            Color::ON,
        ),
        LineStyle::Double => {
            let top_right = if lines.up == LineStyle::Double {
                v_light_left
            } else {
                left_right
            };
            let bottom_right = if lines.down == LineStyle::Double {
                v_light_left
            } else {
                left_right
            };

            canvas.r#box(
                0,
                h_double_top as i32,
                top_right as i32,
                h_light_top as i32,
                Color::ON,
            );
            canvas.r#box(
                0,
                h_light_bottom as i32,
                bottom_right as i32,
                h_double_bottom as i32,
                Color::ON,
            );
        }
    }
}

/// Construct a [`Lines`] from the four directional styles in `up, right, down,
/// left` order (matching the field order of upstream `Lines`).
const fn lines(up: LineStyle, right: LineStyle, down: LineStyle, left: LineStyle) -> Lines {
    Lines {
        up,
        right,
        down,
        left,
    }
}

// Short aliases for the line styles, used only to keep the `BOX_LINES` table
// readable. `N`one, `L`ight, `H`eavy, `D`ouble.
const N: LineStyle = LineStyle::None;
const L: LineStyle = LineStyle::Light;
const H: LineStyle = LineStyle::Heavy;
const D: LineStyle = LineStyle::Double;

/// The audited box-drawing line table: every codepoint in upstream's
/// `draw2500_257F` switch that routes through `linesChar`, paired with its exact
/// `Lines`. Faithful field-for-field transcription of the upstream switch arms.
/// The interleaved non-`linesChar` codepoints (dashes `0x2504`–`0x250B` and
/// `0x254C`–`0x254F`, rounded corners/diagonals `0x256D`–`0x2573`) are
/// deliberately absent — they use other primitives, deferred to later
/// experiments. Each tuple is `(codepoint, lines(up, right, down, left))`.
#[rustfmt::skip]
const BOX_LINES: &[(u32, Lines)] = &[
    // Straight lines
    (0x2500, lines(N, L, N, L)),
    (0x2501, lines(N, H, N, H)),
    (0x2502, lines(L, N, L, N)),
    (0x2503, lines(H, N, H, N)),
    // Corners
    (0x250C, lines(N, L, L, N)),
    (0x250D, lines(N, H, L, N)),
    (0x250E, lines(N, L, H, N)),
    (0x250F, lines(N, H, H, N)),
    (0x2510, lines(N, N, L, L)),
    (0x2511, lines(N, N, L, H)),
    (0x2512, lines(N, N, H, L)),
    (0x2513, lines(N, N, H, H)),
    (0x2514, lines(L, L, N, N)),
    (0x2515, lines(L, H, N, N)),
    (0x2516, lines(H, L, N, N)),
    (0x2517, lines(H, H, N, N)),
    (0x2518, lines(L, N, N, L)),
    (0x2519, lines(L, N, N, H)),
    (0x251A, lines(H, N, N, L)),
    (0x251B, lines(H, N, N, H)),
    // T-junctions (left)
    (0x251C, lines(L, L, L, N)),
    (0x251D, lines(L, H, L, N)),
    (0x251E, lines(H, L, L, N)),
    (0x251F, lines(L, L, H, N)),
    (0x2520, lines(H, L, H, N)),
    (0x2521, lines(H, H, L, N)),
    (0x2522, lines(L, H, H, N)),
    (0x2523, lines(H, H, H, N)),
    // T-junctions (right)
    (0x2524, lines(L, N, L, L)),
    (0x2525, lines(L, N, L, H)),
    (0x2526, lines(H, N, L, L)),
    (0x2527, lines(L, N, H, L)),
    (0x2528, lines(H, N, H, L)),
    (0x2529, lines(H, N, L, H)),
    (0x252A, lines(L, N, H, H)),
    (0x252B, lines(H, N, H, H)),
    // T-junctions (down)
    (0x252C, lines(N, L, L, L)),
    (0x252D, lines(N, L, L, H)),
    (0x252E, lines(N, H, L, L)),
    (0x252F, lines(N, H, L, H)),
    (0x2530, lines(N, L, H, L)),
    (0x2531, lines(N, L, H, H)),
    (0x2532, lines(N, H, H, L)),
    (0x2533, lines(N, H, H, H)),
    // T-junctions (up)
    (0x2534, lines(L, L, N, L)),
    (0x2535, lines(L, L, N, H)),
    (0x2536, lines(L, H, N, L)),
    (0x2537, lines(L, H, N, H)),
    (0x2538, lines(H, L, N, L)),
    (0x2539, lines(H, L, N, H)),
    (0x253A, lines(H, H, N, L)),
    (0x253B, lines(H, H, N, H)),
    // Crosses
    (0x253C, lines(L, L, L, L)),
    (0x253D, lines(L, L, L, H)),
    (0x253E, lines(L, H, L, L)),
    (0x253F, lines(L, H, L, H)),
    (0x2540, lines(H, L, L, L)),
    (0x2541, lines(L, L, H, L)),
    (0x2542, lines(H, L, H, L)),
    (0x2543, lines(H, L, L, H)),
    (0x2544, lines(H, H, L, L)),
    (0x2545, lines(L, L, H, H)),
    (0x2546, lines(L, H, H, L)),
    (0x2547, lines(H, H, L, H)),
    (0x2548, lines(L, H, H, H)),
    (0x2549, lines(H, L, H, H)),
    (0x254A, lines(H, H, H, L)),
    (0x254B, lines(H, H, H, H)),
    // Double lines, corners, T-junctions, crosses
    (0x2550, lines(N, D, N, D)),
    (0x2551, lines(D, N, D, N)),
    (0x2552, lines(N, D, L, N)),
    (0x2553, lines(N, L, D, N)),
    (0x2554, lines(N, D, D, N)),
    (0x2555, lines(N, N, L, D)),
    (0x2556, lines(N, N, D, L)),
    (0x2557, lines(N, N, D, D)),
    (0x2558, lines(L, D, N, N)),
    (0x2559, lines(D, L, N, N)),
    (0x255A, lines(D, D, N, N)),
    (0x255B, lines(L, N, N, D)),
    (0x255C, lines(D, N, N, L)),
    (0x255D, lines(D, N, N, D)),
    (0x255E, lines(L, D, L, N)),
    (0x255F, lines(D, L, D, N)),
    (0x2560, lines(D, D, D, N)),
    (0x2561, lines(L, N, L, D)),
    (0x2562, lines(D, N, D, L)),
    (0x2563, lines(D, N, D, D)),
    (0x2564, lines(N, D, L, D)),
    (0x2565, lines(N, L, D, L)),
    (0x2566, lines(N, D, D, D)),
    (0x2567, lines(L, D, N, D)),
    (0x2568, lines(D, L, N, L)),
    (0x2569, lines(D, D, N, D)),
    (0x256A, lines(L, D, L, D)),
    (0x256B, lines(D, L, D, L)),
    (0x256C, lines(D, D, D, D)),
    // Half-line stubs and light/heavy transitions
    (0x2574, lines(N, N, N, L)),
    (0x2575, lines(L, N, N, N)),
    (0x2576, lines(N, L, N, N)),
    (0x2577, lines(N, N, L, N)),
    (0x2578, lines(N, N, N, H)),
    (0x2579, lines(H, N, N, N)),
    (0x257A, lines(N, H, N, N)),
    (0x257B, lines(N, N, H, N)),
    (0x257C, lines(N, H, N, L)),
    (0x257D, lines(L, N, H, N)),
    (0x257E, lines(N, L, N, H)),
    (0x257F, lines(H, N, L, N)),
];

/// The [`Lines`] for a box-drawing line codepoint, or `None` if `cp` is not a
/// `linesChar` glyph. Linear lookup over the audited [`BOX_LINES`] table.
fn box_lines_styles(cp: u32) -> Option<Lines> {
    BOX_LINES
        .iter()
        .find(|(c, _)| *c == cp)
        .map(|(_, lines)| *lines)
}

/// Draw the box-drawing line glyph for `cp` into `canvas`, returning `true` if
/// `cp` is a dispatched line character. Covers every `linesChar` codepoint in
/// upstream's `draw2500_257F` switch; the non-`linesChar` primitives (dashes,
/// arcs, diagonals) and the other sprite categories are later experiments.
pub(crate) fn draw_box_lines(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    match box_lines_styles(cp) {
        Some(l) => {
            lines_char(metrics, canvas, l);
            true
        }
        None => false,
    }
}

/// Draw the light diagonal box-drawing glyph for `cp` (`U+2571 ╱`, `U+2572 ╲`,
/// `U+2573 ╳`) into `canvas`, returning `true` if `cp` is a diagonal. Faithful
/// port of upstream `lightDiagonalUpperRightToLowerLeft`/`…UpperLeftToLowerRight`/
/// `…Cross`: anti-aliased corner-to-corner lines (stroked via the `z2d` port),
/// overshooting the corners by `0.5·slope` to keep the slope true.
pub(crate) fn draw_box_diagonal(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    let float_width = metrics.cell_width as f64;
    let float_height = metrics.cell_height as f64;
    let slope_x = (float_width / float_height).min(1.0);
    let slope_y = (float_height / float_width).min(1.0);
    let thickness = Thickness::Light.height(metrics.box_thickness) as f64;

    let upper_right_to_lower_left = |canvas: &mut Canvas| {
        canvas.line(
            raster::Point::new(float_width + 0.5 * slope_x, -0.5 * slope_y),
            raster::Point::new(-0.5 * slope_x, float_height + 0.5 * slope_y),
            thickness,
        );
    };
    let upper_left_to_lower_right = |canvas: &mut Canvas| {
        canvas.line(
            raster::Point::new(-0.5 * slope_x, -0.5 * slope_y),
            raster::Point::new(float_width + 0.5 * slope_x, float_height + 0.5 * slope_y),
            thickness,
        );
    };

    match cp {
        0x2571 => upper_right_to_lower_left(canvas),
        0x2572 => upper_left_to_lower_right(canvas),
        0x2573 => {
            upper_right_to_lower_left(canvas);
            upper_left_to_lower_right(canvas);
        }
        _ => return false,
    }
    true
}

/// Which cell corner a box-drawing arc rounds. Faithful port of upstream
/// `box.zig`'s `Corner`.
#[derive(Clone, Copy)]
enum Corner {
    Tl,
    Tr,
    Bl,
    Br,
}

/// Box-drawing **arcs** (`╭ U+256D`, `╮ U+256E`, `╯ U+256F`, `╰ U+2570`) — the
/// first curved sprite glyphs. Each is a straight arm into the cell, a
/// quarter-circle `curve_to` corner, and a straight arm out, stroked with butt
/// caps. Faithful port of upstream `box.zig`'s `arc`. Returns `false` for any
/// other codepoint.
pub(crate) fn draw_box_arc(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    let corner = match cp {
        0x256d => Corner::Br,
        0x256e => Corner::Bl,
        0x256f => Corner::Tl,
        0x2570 => Corner::Tr,
        _ => return false,
    };

    let thick_px = Thickness::Light.height(metrics.box_thickness);
    let float_width = metrics.cell_width as f64;
    let float_height = metrics.cell_height as f64;
    let float_thick = thick_px as f64;
    // Integer arithmetic for the center (upstream's saturating sub + integer
    // div), then the float thickness offset.
    let center_x = (metrics.cell_width.saturating_sub(thick_px) / 2) as f64 + float_thick / 2.0;
    let center_y = (metrics.cell_height.saturating_sub(thick_px) / 2) as f64 + float_thick / 2.0;
    let r = float_width.min(float_height) / 2.0;
    // Fraction away from the center to place the middle control points.
    let s: f64 = 0.25;

    let mv = |x: f64, y: f64| raster::PathNode::MoveTo(raster::Point::new(x, y));
    let ln = |x: f64, y: f64| raster::PathNode::LineTo(raster::Point::new(x, y));
    let cv = |x1: f64, y1: f64, x2: f64, y2: f64, x3: f64, y3: f64| raster::PathNode::CurveTo {
        p1: raster::Point::new(x1, y1),
        p2: raster::Point::new(x2, y2),
        p3: raster::Point::new(x3, y3),
    };

    let nodes = match corner {
        Corner::Tl => vec![
            mv(center_x, 0.0),
            ln(center_x, center_y - r),
            cv(
                center_x,
                center_y - s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            ),
            ln(0.0, center_y),
        ],
        Corner::Tr => vec![
            mv(center_x, 0.0),
            ln(center_x, center_y - r),
            cv(
                center_x,
                center_y - s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            ),
            ln(float_width, center_y),
        ],
        Corner::Bl => vec![
            mv(center_x, float_height),
            ln(center_x, center_y + r),
            cv(
                center_x,
                center_y + s * r,
                center_x - s * r,
                center_y,
                center_x - r,
                center_y,
            ),
            ln(0.0, center_y),
        ],
        Corner::Br => vec![
            mv(center_x, float_height),
            ln(center_x, center_y + r),
            cv(
                center_x,
                center_y + s * r,
                center_x + s * r,
                center_y,
                center_x + r,
                center_y,
            ),
            ln(float_width, center_y),
        ],
    };

    canvas.stroke_path(&nodes, float_thick, raster::CapMode::Butt);
    true
}

/// The curly underline (undercurl): a single-cycle wave — two cubic Béziers —
/// peaking at the cell center, stroked with the underline thickness and round
/// caps. The first round-cap sprite glyph. Faithful port of upstream
/// `special.zig`'s `underline_curly`.
pub(crate) fn draw_underline_curly(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    let float_width = width as f64;
    let float_height = height as f64;
    let float_pos = metrics.underline_position as f64;
    let line_width = metrics.underline_thickness as f64;

    // Empirically this looks good.
    let amplitude = float_width / std::f64::consts::PI;

    // Clamp so the curl is not clipped below the drawable area.
    let padding = canvas.padding_y() as f64;
    let top = float_pos.min(float_height + padding - amplitude - line_width);
    let bottom = top + amplitude;

    // Curvature multiplier (0.4 gives a nice smooth wiggle) and the cell center.
    let r = 0.4;
    let center = 0.5 * float_width;

    // One wave cycle, peaking at the center.
    let nodes = [
        raster::PathNode::MoveTo(raster::Point::new(0.0, bottom)),
        raster::PathNode::CurveTo {
            p1: raster::Point::new(center * r, bottom),
            p2: raster::Point::new(center - center * r, top),
            p3: raster::Point::new(center, top),
        },
        raster::PathNode::CurveTo {
            p1: raster::Point::new(center + center * r, top),
            p2: raster::Point::new(float_width - center * r, bottom),
            p3: raster::Point::new(float_width, bottom),
        },
    ];

    canvas.stroke_path(&nodes, line_width, raster::CapMode::Round);
}

/// The filled corner triangles (`◢ U+25E2`, `◣ U+25E3`, `◤ U+25E4`,
/// `◥ U+25E5`) — the first filled sprite glyphs. Each fills a right triangle
/// covering one corner of the cell, opaque. Faithful port of upstream
/// `geometric_shapes.zig`'s `cornerTriangleShade` (`.on`). Returns `false` for
/// any other codepoint.
pub(crate) fn draw_corner_triangle(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    let corner = match cp {
        0x25e2 => Corner::Br,
        0x25e3 => Corner::Bl,
        0x25e4 => Corner::Tl,
        0x25e5 => Corner::Tr,
        _ => return false,
    };

    let w = metrics.cell_width as f64;
    let h = metrics.cell_height as f64;
    // The three triangle vertices per corner.
    let (v0, v1, v2) = match corner {
        Corner::Tl => ((0.0, 0.0), (0.0, h), (w, 0.0)),
        Corner::Tr => ((0.0, 0.0), (w, h), (w, 0.0)),
        Corner::Bl => ((0.0, 0.0), (0.0, h), (w, h)),
        Corner::Br => ((0.0, h), (w, h), (w, 0.0)),
    };

    let nodes = [
        raster::PathNode::MoveTo(raster::Point::new(v0.0, v0.1)),
        raster::PathNode::LineTo(raster::Point::new(v1.0, v1.1)),
        raster::PathNode::LineTo(raster::Point::new(v2.0, v2.1)),
        raster::PathNode::ClosePath,
    ];

    canvas.fill_path(&nodes);
    true
}

/// The outlined corner triangles (`◸ U+25F8`, `◹ U+25F9`, `◺ U+25FA`,
/// `◿ U+25FF`) — the right-triangle outlines, drawn with an **inner** stroke so
/// the outline stays inside the cell. Faithful port of upstream
/// `geometric_shapes.zig`'s `cornerTriangleOutline`. Returns `false` for any
/// other codepoint.
pub(crate) fn draw_corner_triangle_outline(
    cp: u32,
    metrics: &Metrics,
    canvas: &mut Canvas,
) -> bool {
    let corner = match cp {
        0x25f8 => Corner::Tl,
        0x25f9 => Corner::Tr,
        0x25fa => Corner::Bl,
        0x25ff => Corner::Br,
        _ => return false,
    };

    let w = metrics.cell_width as f64;
    let h = metrics.cell_height as f64;
    let (v0, v1, v2) = match corner {
        Corner::Tl => ((0.0, 0.0), (0.0, h), (w, 0.0)),
        Corner::Tr => ((0.0, 0.0), (w, h), (w, 0.0)),
        Corner::Bl => ((0.0, 0.0), (0.0, h), (w, h)),
        Corner::Br => ((0.0, h), (w, h), (w, 0.0)),
    };

    let nodes = [
        raster::PathNode::MoveTo(raster::Point::new(v0.0, v0.1)),
        raster::PathNode::LineTo(raster::Point::new(v1.0, v1.1)),
        raster::PathNode::LineTo(raster::Point::new(v2.0, v2.1)),
        raster::PathNode::ClosePath,
    ];

    let thick = Thickness::Light.height(metrics.box_thickness) as f64;
    canvas.inner_stroke_path(&nodes, thick);
    true
}

/// The plain underline: a full-width rect at the underline position, clamped
/// (saturating) so it stays within the drawable area. Faithful port of upstream
/// `special.zig`'s `underline`.
pub(crate) fn draw_underline(canvas: &mut Canvas, width: u32, height: u32, metrics: &Metrics) {
    let thick = metrics.underline_thickness;
    let limit = height
        .saturating_add(canvas.padding_y())
        .saturating_sub(thick);
    let y = metrics.underline_position.min(limit);
    hline(canvas, 0, width as i32, y as i32, thick);
}

/// The double underline: two full-width rects bracketing the underline position
/// with a one-thickness gap between them. Faithful port of upstream
/// `special.zig`'s `underline_double`.
pub(crate) fn draw_underline_double(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    let thick = metrics.underline_thickness;
    let limit = height
        .saturating_add(canvas.padding_y())
        .saturating_sub(thick.saturating_mul(2));
    let y = metrics.underline_position.min(limit);
    // One line above the position, one below by a thickness (the gap sits where
    // the single underline would).
    hline(
        canvas,
        0,
        width as i32,
        y.saturating_sub(thick) as i32,
        thick,
    );
    hline(
        canvas,
        0,
        width as i32,
        y.saturating_add(thick) as i32,
        thick,
    );
}

/// The strikethrough: a full-width rect at the strikethrough position. Faithful
/// port of upstream `special.zig`'s `strikethrough`.
pub(crate) fn draw_strikethrough(canvas: &mut Canvas, width: u32, _height: u32, metrics: &Metrics) {
    hline(
        canvas,
        0,
        width as i32,
        metrics.strikethrough_position as i32,
        metrics.strikethrough_thickness,
    );
}

/// The overline: a full-width rect at the overline position, allowed to extend
/// up to one quarter cell above the cell (into the top padding) but not past the
/// canvas. Faithful port of upstream `special.zig`'s `overline`.
pub(crate) fn draw_overline(canvas: &mut Canvas, width: u32, _height: u32, metrics: &Metrics) {
    let y = metrics.overline_position.max(-(canvas.padding_y() as i32));
    hline(canvas, 0, width as i32, y, metrics.overline_thickness);
}

/// The dashed underline: alternating full-thickness rects at the (clamped)
/// underline position, drawn at even-index slots. Faithful port of upstream
/// `special.zig`'s `underline_dashed`.
pub(crate) fn draw_underline_dashed(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    let thick = metrics.underline_thickness;
    let limit = height
        .saturating_add(canvas.padding_y())
        .saturating_sub(thick);
    let y = metrics.underline_position.min(limit);

    let dash_width = width / 3 + 1;
    let dash_count = width / dash_width + 1;
    let mut i = 0;
    while i < dash_count {
        let x = i * dash_width;
        canvas.rect(
            Rect {
                x: x as i32,
                y: y as i32,
                width: dash_width as i32,
                height: thick as i32,
            },
            Color::ON,
        );
        i += 2;
    }
}

/// The dotted underline: a row of filled circles (dots) at the (clamped)
/// underline position. Faithful port of upstream `special.zig`'s
/// `underline_dotted`.
pub(crate) fn draw_underline_dotted(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    use std::f64::consts::{FRAC_1_SQRT_2, TAU};

    let float_width = width as f64;
    let float_height = height as f64;
    let float_pos = metrics.underline_position as f64;
    let float_thick = metrics.underline_thickness as f64;

    // A slightly fatter radius so the dots don't look anemic.
    let radius = FRAC_1_SQRT_2 * float_thick;

    // Clamp the dot center so the dots are not clipped below the canvas.
    let padding = canvas.padding_y() as f64;
    let y = (float_pos + 0.5 * float_thick).min(float_height + padding - radius.ceil());

    // Enough dots that the spacing roughly matches their diameter, but never so
    // many that they crowd or overlap; at least one per cell.
    let dot_count = (float_width / (4.0 * radius))
        .ceil()
        .min((float_width / (3.0 * radius)).floor())
        .min((float_width / (2.0 * radius + 1.0)).floor())
        .max(1.0);

    // Divide the cell into dot_count areas with a dot centered in each.
    let mut x = (float_width / dot_count) / 2.0;
    let mut nodes = Vec::new();
    for _ in 0..(dot_count as usize) {
        nodes.extend(raster::arc(x, y, radius, 0.0, TAU, 0.1));
        nodes.push(raster::PathNode::ClosePath);
        x += float_width / dot_count;
    }
    canvas.fill_path(&nodes);
}

/// The solid powerline triangles (`E0B0` right arrow, `E0B2` left arrow, and the
/// four half-cell triangles `E0B8`/`E0BA`/`E0BC`/`E0BE`) — the filled powerline
/// separators. Faithful port of upstream `powerline.zig`'s solid triangles,
/// which use the glyph `width`/`height` (not the cell metrics). Returns `false`
/// for any other codepoint.
pub(crate) fn draw_powerline_triangle(
    cp: u32,
    width: u32,
    height: u32,
    canvas: &mut Canvas,
) -> bool {
    let w = width as f64;
    let h = height as f64;
    let p = |x: f64, y: f64| Point { x, y };
    let (p0, p1, p2) = match cp {
        0xe0b0 => (p(0.0, 0.0), p(w, h / 2.0), p(0.0, h)),
        0xe0b2 => (p(w, 0.0), p(0.0, h / 2.0), p(w, h)),
        0xe0b8 => (p(0.0, 0.0), p(w, h), p(0.0, h)),
        0xe0ba => (p(w, 0.0), p(w, h), p(0.0, h)),
        0xe0bc => (p(0.0, 0.0), p(w, 0.0), p(0.0, h)),
        0xe0be => (p(0.0, 0.0), p(w, 0.0), p(w, h)),
        _ => return false,
    };
    canvas.triangle(Triangle { p0, p1, p2 });
    true
}

/// The outlined powerline chevrons (`E0B1` `>`, `E0B3` `<`) — a stroked open
/// two-segment arrowhead. `E0B3` is `E0B1` mirrored. Faithful port of upstream
/// `powerline.zig`'s `drawE0B1`/`drawE0B3` (which use the glyph `width`/`height`
/// and the light box thickness). Returns `false` for any other codepoint.
pub(crate) fn draw_powerline_chevron(
    cp: u32,
    width: u32,
    height: u32,
    metrics: &Metrics,
    canvas: &mut Canvas,
) -> bool {
    let flip = match cp {
        0xe0b1 => false,
        0xe0b3 => true,
        _ => return false,
    };

    let w = width as f64;
    let h = height as f64;
    // The open `>` chevron path.
    let nodes = [
        raster::PathNode::MoveTo(raster::Point::new(0.0, 0.0)),
        raster::PathNode::LineTo(raster::Point::new(w, h / 2.0)),
        raster::PathNode::LineTo(raster::Point::new(0.0, h)),
    ];
    let thick = Thickness::Light.height(metrics.box_thickness) as f64;
    canvas.stroke_path(&nodes, thick, raster::CapMode::Butt);

    if flip {
        canvas.flip_horizontal();
    }
    true
}

/// The rounded powerline separators (`E0B4`/`E0B6` filled, `E0B5`/`E0B7`
/// outlined) — a rectangle with a rounded right (or, flipped, left) side, built
/// from cubic quarter-circle corners. Faithful port of upstream `powerline.zig`'s
/// `drawE0B4`–`drawE0B7`. Returns `false` for any other codepoint.
pub(crate) fn draw_powerline_rounded(
    cp: u32,
    width: u32,
    height: u32,
    metrics: &Metrics,
    canvas: &mut Canvas,
) -> bool {
    let (outlined, flip) = match cp {
        0xe0b4 => (false, false),
        0xe0b5 => (true, false),
        0xe0b6 => (false, true),
        0xe0b7 => (true, true),
        _ => return false,
    };

    let w = width as f64;
    let h = height as f64;
    // Coefficient for approximating a circular arc with a cubic.
    let c = (2.0_f64.sqrt() - 1.0) * 4.0 / 3.0;
    let r = w.min(h / 2.0);
    let pt = raster::Point::new;

    // The open rounded-right path.
    let mut nodes = vec![
        raster::PathNode::MoveTo(pt(0.0, 0.0)),
        raster::PathNode::CurveTo {
            p1: pt(r * c, 0.0),
            p2: pt(r, r - r * c),
            p3: pt(r, r),
        },
        raster::PathNode::LineTo(pt(r, h - r)),
        raster::PathNode::CurveTo {
            p1: pt(r, h - r + r * c),
            p2: pt(r * c, h),
            p3: pt(0.0, h),
        },
    ];

    if outlined {
        canvas.inner_stroke_path(&nodes, metrics.box_thickness as f64);
    } else {
        nodes.push(raster::PathNode::ClosePath);
        canvas.fill_path(&nodes);
    }

    if flip {
        canvas.flip_horizontal();
    }
    true
}

/// The thin diagonal powerline spacers (`E0B9`/`E0BF` `╲`, `E0BB`/`E0BD` `╱`).
/// Upstream `powerline.zig` draws these as box-drawing diagonals, so this maps
/// each to the equivalent box diagonal (`U+2572`/`U+2571`) and delegates to
/// [`draw_box_diagonal`]. Returns `false` for any other codepoint.
pub(crate) fn draw_powerline_diagonal(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    let box_cp = match cp {
        // upper-left to lower-right (╲)
        0xe0b9 | 0xe0bf => 0x2572,
        // upper-right to lower-left (╱)
        0xe0bb | 0xe0bd => 0x2571,
        _ => return false,
    };
    draw_box_diagonal(box_cp, metrics, canvas)
}

/// The flame powerline separators (`E0D2`, `E0D4`) — two filled quadrilaterals
/// tapering toward a thin gap at the cell center. `E0D4` is `E0D2` mirrored.
/// Faithful port of upstream `powerline.zig`'s `drawE0D2`/`drawE0D4`. Returns
/// `false` for any other codepoint.
pub(crate) fn draw_powerline_flame(
    cp: u32,
    width: u32,
    height: u32,
    metrics: &Metrics,
    canvas: &mut Canvas,
) -> bool {
    let flip = match cp {
        0xe0d2 => false,
        0xe0d4 => true,
        _ => return false,
    };

    let w = width as f64;
    let h = height as f64;
    let t = metrics.box_thickness as f64;
    let pt = raster::Point::new;

    // Top piece.
    let top = [
        raster::PathNode::MoveTo(pt(0.0, 0.0)),
        raster::PathNode::LineTo(pt(w, 0.0)),
        raster::PathNode::LineTo(pt(w / 2.0, h / 2.0 - t / 2.0)),
        raster::PathNode::LineTo(pt(0.0, h / 2.0 - t / 2.0)),
        raster::PathNode::ClosePath,
    ];
    canvas.fill_path(&top);

    // Bottom piece.
    let bottom = [
        raster::PathNode::MoveTo(pt(0.0, h)),
        raster::PathNode::LineTo(pt(w, h)),
        raster::PathNode::LineTo(pt(w / 2.0, h / 2.0 + t / 2.0)),
        raster::PathNode::LineTo(pt(0.0, h / 2.0 + t / 2.0)),
        raster::PathNode::ClosePath,
    ];
    canvas.fill_path(&bottom);

    if flip {
        canvas.flip_horizontal();
    }
    true
}

/// The block cursor: a full-cell rect. Faithful port of upstream `special.zig`'s
/// `cursor_rect`.
pub(crate) fn draw_cursor_rect(canvas: &mut Canvas, width: u32, height: u32, _metrics: &Metrics) {
    canvas.rect(
        Rect {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        },
        Color::ON,
    );
}

/// The hollow (outline) cursor: a full-cell rect with the interior punched
/// transparent, leaving a box outline. Faithful port of upstream `special.zig`'s
/// `cursor_hollow_rect`.
pub(crate) fn draw_cursor_hollow_rect(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    canvas.rect(
        Rect {
            x: 0,
            y: 0,
            width: width as i32,
            height: height as i32,
        },
        Color::ON,
    );
    let thick = metrics.cursor_thickness;
    canvas.rect(
        Rect {
            x: thick as i32,
            y: thick as i32,
            width: width.saturating_sub(thick.saturating_mul(2)) as i32,
            height: height.saturating_sub(thick.saturating_mul(2)) as i32,
        },
        Color::OFF,
    );
}

/// The bar cursor: a vertical bar shifted half its thickness (rounded up) over
/// the left cell edge, so it sits centered between cells. Faithful port of
/// upstream `special.zig`'s `cursor_bar`.
pub(crate) fn draw_cursor_bar(canvas: &mut Canvas, _width: u32, height: u32, metrics: &Metrics) {
    canvas.rect(
        Rect {
            x: -(((metrics.cursor_thickness + 1) / 2) as i32),
            y: 0,
            width: metrics.cursor_thickness as i32,
            height: height as i32,
        },
        Color::ON,
    );
}

/// The underline cursor: a full-width bar at the (clamped) underline position,
/// `cursor_thickness` tall. Faithful port of upstream `special.zig`'s
/// `cursor_underline`.
pub(crate) fn draw_cursor_underline(
    canvas: &mut Canvas,
    width: u32,
    height: u32,
    metrics: &Metrics,
) {
    let limit = height
        .saturating_add(canvas.padding_y())
        .saturating_sub(metrics.underline_thickness);
    let y = metrics.underline_position.min(limit);
    canvas.rect(
        Rect {
            x: 0,
            y: y as i32,
            width: width as i32,
            height: metrics.cursor_thickness as i32,
        },
        Color::ON,
    );
}

/// Horizontal line with the top edge at `y`, from `x1` to `x2`, `thick` pixels
/// tall. Faithful port of upstream `common.hline`.
fn hline(canvas: &mut Canvas, x1: i32, x2: i32, y: i32, thick: u32) {
    canvas.r#box(x1, y, x2, y + thick as i32, Color::ON);
}

/// Vertical line with the left edge at `x`, from `y1` to `y2`, `thick` pixels
/// wide. Faithful port of upstream `common.vline`.
fn vline(canvas: &mut Canvas, y1: i32, y2: i32, x: i32, thick: u32) {
    canvas.r#box(x, y1, x + thick as i32, y2, Color::ON);
}

/// Centered horizontal line of the given thickness across the full cell width.
/// Faithful port of upstream `common.hlineMiddle`.
fn hline_middle(metrics: &Metrics, canvas: &mut Canvas, thickness: Thickness) {
    let thick_px = thickness.height(metrics.box_thickness);
    hline(
        canvas,
        0,
        metrics.cell_width as i32,
        (metrics.cell_height.saturating_sub(thick_px) / 2) as i32,
        thick_px,
    );
}

/// Centered vertical line of the given thickness down the full cell height.
/// Faithful port of upstream `common.vlineMiddle`.
fn vline_middle(metrics: &Metrics, canvas: &mut Canvas, thickness: Thickness) {
    let thick_px = thickness.height(metrics.box_thickness);
    vline(
        canvas,
        0,
        metrics.cell_height as i32,
        (metrics.cell_width.saturating_sub(thick_px) / 2) as i32,
        thick_px,
    );
}

/// Draw `count` evenly-tiled horizontal dashes, centered vertically, with
/// half-gaps on each side so the pattern tiles seamlessly. Faithful port of
/// upstream `dashHorizontal`. Falls back to a solid light line when the cell is
/// too narrow to hold the dashes.
fn dash_horizontal(
    metrics: &Metrics,
    canvas: &mut Canvas,
    count: u32,
    thick_px: u32,
    desired_gap: u32,
) {
    assert!((2..=4).contains(&count));

    // For N dashes there are N - 1 gaps between them, plus half-gaps on either
    // side that add up to one more — so N total gaps.
    let gap_count = count;

    // Without at least 1px per dash and per gap we can't draw the pattern, so
    // fall back to a solid line.
    if metrics.cell_width < count + gap_count {
        hline_middle(metrics, canvas, Thickness::Light);
        return;
    }

    // Never let the gaps exceed 50% of the width, or the dashes look wrong.
    let gap_width: i32 = desired_gap.min(metrics.cell_width / (2 * count)) as i32;
    let total_gap_width: i32 = gap_count as i32 * gap_width;
    let total_dash_width: i32 = metrics.cell_width as i32 - total_gap_width;
    let dash_width: i32 = total_dash_width.div_euclid(count as i32);
    let remaining: i32 = total_dash_width.rem_euclid(count as i32);

    assert!(
        dash_width * count as i32 + gap_width * gap_count as i32 + remaining
            == metrics.cell_width as i32
    );

    // Dashes are centered vertically.
    let y: i32 = (metrics.cell_height.saturating_sub(thick_px) / 2) as i32;

    // Start half a gap from the left edge to center the pattern.
    let mut x: i32 = gap_width.div_euclid(2);

    // Distribute the leftover space into dash widths, 1px at a time — less
    // visually obvious there than in the gaps.
    let mut extra: i32 = remaining;

    for _ in 0..count {
        let mut x1 = x + dash_width;
        if extra > 0 {
            extra -= 1;
            x1 += 1;
        }
        hline(canvas, x, x1, y, thick_px);
        x = x1 + gap_width;
    }
}

/// Draw `count` evenly-tiled vertical dashes, centered horizontally, with a
/// single full extra gap at the bottom. Faithful port of upstream
/// `dashVertical`. Falls back to a solid light line when the cell is too short.
fn dash_vertical(
    metrics: &Metrics,
    canvas: &mut Canvas,
    count: u32,
    thick_px: u32,
    desired_gap: u32,
) {
    assert!((2..=4).contains(&count));

    // The extra gap at the bottom means there are as many gaps as dashes.
    let gap_count = count;

    if metrics.cell_height < count + gap_count {
        vline_middle(metrics, canvas, Thickness::Light);
        return;
    }

    let gap_height: i32 = desired_gap.min(metrics.cell_height / (2 * count)) as i32;
    let total_gap_height: i32 = gap_count as i32 * gap_height;
    let total_dash_height: i32 = metrics.cell_height as i32 - total_gap_height;
    let dash_height: i32 = total_dash_height.div_euclid(count as i32);
    let remaining: i32 = total_dash_height.rem_euclid(count as i32);

    assert!(
        dash_height * count as i32 + gap_height * gap_count as i32 + remaining
            == metrics.cell_height as i32
    );

    // Dashes are centered horizontally.
    let x: i32 = (metrics.cell_width.saturating_sub(thick_px) / 2) as i32;

    // Start at the top of the cell.
    let mut y: i32 = 0;

    let mut extra: i32 = remaining;

    for _ in 0..count {
        let mut y1 = y + dash_height;
        if extra > 0 {
            extra -= 1;
            y1 += 1;
        }
        vline(canvas, y, y1, x, thick_px);
        y = y1 + gap_height;
    }
}

/// Draw the box-drawing dash glyph for `cp` into `canvas`, returning `true` if
/// `cp` is a dispatched dash character. Covers the dash codepoints
/// `U+2504`–`U+250B` and `U+254C`–`U+254F` of upstream's `draw2500_257F`; the
/// non-dash primitives (lines, arcs, diagonals) and other sprite categories are
/// elsewhere.
pub(crate) fn draw_box_dashes(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    let light = Thickness::Light.height(metrics.box_thickness);
    let heavy = Thickness::Heavy.height(metrics.box_thickness);
    let wide_gap = light.max(4);
    match cp {
        0x2504 => dash_horizontal(metrics, canvas, 3, light, wide_gap),
        0x2505 => dash_horizontal(metrics, canvas, 3, heavy, wide_gap),
        0x2506 => dash_vertical(metrics, canvas, 3, light, wide_gap),
        0x2507 => dash_vertical(metrics, canvas, 3, heavy, wide_gap),
        0x2508 => dash_horizontal(metrics, canvas, 4, light, wide_gap),
        0x2509 => dash_horizontal(metrics, canvas, 4, heavy, wide_gap),
        0x250A => dash_vertical(metrics, canvas, 4, light, wide_gap),
        0x250B => dash_vertical(metrics, canvas, 4, heavy, wide_gap),
        0x254C => dash_horizontal(metrics, canvas, 2, light, light),
        0x254D => dash_horizontal(metrics, canvas, 2, heavy, heavy),
        0x254E => dash_vertical(metrics, canvas, 2, light, heavy),
        0x254F => dash_vertical(metrics, canvas, 2, heavy, heavy),
        _ => return false,
    }
    true
}

/// A pixel shade. The enum value is the pixel alpha. Faithful port of upstream
/// `common.Shade`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Shade {
    Off = 0x00,
    Light = 0x40,
    Medium = 0x80,
    Dark = 0xc0,
    On = 0xff,
}

impl Shade {
    /// The [`Color`] (alpha) for this shade.
    pub(crate) fn color(self) -> Color {
        Color(self as u8)
    }
}

/// Horizontal alignment of a figure within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HAlign {
    Left,
    Right,
    Center,
}

/// Vertical alignment of a figure within a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VAlign {
    Top,
    Bottom,
    Middle,
}

/// Alignment of a figure within a cell. Faithful port of upstream
/// `common.Alignment` (defaults to centered).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Alignment {
    pub horizontal: HAlign,
    pub vertical: VAlign,
}

impl Alignment {
    pub(crate) const UPPER: Alignment = Alignment {
        horizontal: HAlign::Center,
        vertical: VAlign::Top,
    };
    pub(crate) const LOWER: Alignment = Alignment {
        horizontal: HAlign::Center,
        vertical: VAlign::Bottom,
    };
    pub(crate) const LEFT: Alignment = Alignment {
        horizontal: HAlign::Left,
        vertical: VAlign::Middle,
    };
    pub(crate) const RIGHT: Alignment = Alignment {
        horizontal: HAlign::Right,
        vertical: VAlign::Middle,
    };

    /// The centered alignment (the upstream default).
    pub(crate) const fn center() -> Alignment {
        Alignment {
            horizontal: HAlign::Center,
            vertical: VAlign::Middle,
        }
    }
}

/// A set of cell quadrants that may each be present or not. Faithful port of
/// upstream `common.Quads`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Quads {
    pub tl: bool,
    pub tr: bool,
    pub bl: bool,
    pub br: bool,
}

/// Draw a `width × height` fraction of the cell, aligned per `align`, shaded by
/// `shade`. Faithful port of upstream `blockShade`.
fn block_shade(
    metrics: &Metrics,
    canvas: &mut Canvas,
    align: Alignment,
    width: f64,
    height: f64,
    shade: Shade,
) {
    let w = (metrics.cell_width as f64 * width).round() as u32;
    let h = (metrics.cell_height as f64 * height).round() as u32;

    let x = match align.horizontal {
        HAlign::Left => 0,
        HAlign::Right => metrics.cell_width - w,
        HAlign::Center => (metrics.cell_width - w) / 2,
    };
    let y = match align.vertical {
        VAlign::Top => 0,
        VAlign::Bottom => metrics.cell_height - h,
        VAlign::Middle => (metrics.cell_height - h) / 2,
    };

    canvas.rect(
        Rect {
            x: x as i32,
            y: y as i32,
            width: w as i32,
            height: h as i32,
        },
        shade.color(),
    );
}

/// Draw a solid (`.on`) `width × height` block aligned per `align`. Faithful
/// port of upstream `block`.
fn block(metrics: &Metrics, canvas: &mut Canvas, align: Alignment, width: f64, height: f64) {
    block_shade(metrics, canvas, align, width, height, Shade::On);
}

/// Shade the whole cell. Faithful port of upstream `fullBlockShade`.
fn full_block_shade(metrics: &Metrics, canvas: &mut Canvas, shade: Shade) {
    canvas.r#box(
        0,
        0,
        metrics.cell_width as i32,
        metrics.cell_height as i32,
        shade.color(),
    );
}

/// Fill the set quadrants of `quads`. Faithful port of upstream `quadrant`.
fn quadrant(metrics: &Metrics, canvas: &mut Canvas, quads: Quads) {
    use Fraction::{Full, Half, Zero};
    if quads.tl {
        fill(metrics, canvas, Zero, Half, Zero, Half);
    }
    if quads.tr {
        fill(metrics, canvas, Half, Full, Zero, Half);
    }
    if quads.bl {
        fill(metrics, canvas, Zero, Half, Half, Full);
    }
    if quads.br {
        fill(metrics, canvas, Half, Full, Half, Full);
    }
}

/// Draw the Block Elements glyph for `cp` (`U+2580`–`U+259F`) into `canvas`,
/// returning `true` if `cp` is a dispatched block glyph. Faithful port of
/// upstream `draw2580_259F`.
pub(crate) fn draw_block(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    // Utility fractions for the eighth/quarter blocks.
    const ONE_EIGHTH: f64 = 0.125;
    const ONE_QUARTER: f64 = 0.25;
    const THREE_EIGHTHS: f64 = 0.375;
    const HALF: f64 = 0.5;
    const FIVE_EIGHTHS: f64 = 0.625;
    const THREE_QUARTERS: f64 = 0.75;
    const SEVEN_EIGHTHS: f64 = 0.875;

    let q = |tl: bool, tr: bool, bl: bool, br: bool| Quads { tl, tr, bl, br };
    match cp {
        0x2580 => block(metrics, canvas, Alignment::UPPER, 1.0, HALF),
        0x2581 => block(metrics, canvas, Alignment::LOWER, 1.0, ONE_EIGHTH),
        0x2582 => block(metrics, canvas, Alignment::LOWER, 1.0, ONE_QUARTER),
        0x2583 => block(metrics, canvas, Alignment::LOWER, 1.0, THREE_EIGHTHS),
        0x2584 => block(metrics, canvas, Alignment::LOWER, 1.0, HALF),
        0x2585 => block(metrics, canvas, Alignment::LOWER, 1.0, FIVE_EIGHTHS),
        0x2586 => block(metrics, canvas, Alignment::LOWER, 1.0, THREE_QUARTERS),
        0x2587 => block(metrics, canvas, Alignment::LOWER, 1.0, SEVEN_EIGHTHS),
        0x2588 => full_block_shade(metrics, canvas, Shade::On),
        0x2589 => block(metrics, canvas, Alignment::LEFT, SEVEN_EIGHTHS, 1.0),
        0x258A => block(metrics, canvas, Alignment::LEFT, THREE_QUARTERS, 1.0),
        0x258B => block(metrics, canvas, Alignment::LEFT, FIVE_EIGHTHS, 1.0),
        0x258C => block(metrics, canvas, Alignment::LEFT, HALF, 1.0),
        0x258D => block(metrics, canvas, Alignment::LEFT, THREE_EIGHTHS, 1.0),
        0x258E => block(metrics, canvas, Alignment::LEFT, ONE_QUARTER, 1.0),
        0x258F => block(metrics, canvas, Alignment::LEFT, ONE_EIGHTH, 1.0),
        0x2590 => block(metrics, canvas, Alignment::RIGHT, HALF, 1.0),
        0x2591 => full_block_shade(metrics, canvas, Shade::Light),
        0x2592 => full_block_shade(metrics, canvas, Shade::Medium),
        0x2593 => full_block_shade(metrics, canvas, Shade::Dark),
        0x2594 => block(metrics, canvas, Alignment::UPPER, 1.0, ONE_EIGHTH),
        0x2595 => block(metrics, canvas, Alignment::RIGHT, ONE_EIGHTH, 1.0),
        0x2596 => quadrant(metrics, canvas, q(false, false, true, false)),
        0x2597 => quadrant(metrics, canvas, q(false, false, false, true)),
        0x2598 => quadrant(metrics, canvas, q(true, false, false, false)),
        0x2599 => quadrant(metrics, canvas, q(true, false, true, true)),
        0x259A => quadrant(metrics, canvas, q(true, false, false, true)),
        0x259B => quadrant(metrics, canvas, q(true, true, true, false)),
        0x259C => quadrant(metrics, canvas, q(true, true, false, true)),
        0x259D => quadrant(metrics, canvas, q(false, true, false, false)),
        0x259E => quadrant(metrics, canvas, q(false, true, true, false)),
        0x259F => quadrant(metrics, canvas, q(false, true, true, true)),
        _ => return false,
    }
    true
}

/// The 8 dot flags of a braille pattern. Faithful port of upstream
/// `braille.Pattern`: the bits of the codepoint's low byte, in the order
/// `tl, ul, ll, tr, ur, lr, bl, br`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BraillePattern {
    tl: bool,
    ul: bool,
    ll: bool,
    tr: bool,
    ur: bool,
    lr: bool,
    bl: bool,
    br: bool,
}

impl BraillePattern {
    /// Decode the low byte of `cp` into its dot flags.
    fn from_cp(cp: u32) -> BraillePattern {
        let b = (cp & 0xFF) as u8;
        BraillePattern {
            tl: b & 0x01 != 0,
            ul: b & 0x02 != 0,
            ll: b & 0x04 != 0,
            tr: b & 0x08 != 0,
            ur: b & 0x10 != 0,
            lr: b & 0x20 != 0,
            bl: b & 0x40 != 0,
            br: b & 0x80 != 0,
        }
    }
}

/// Draw the Braille Patterns glyph for `cp` (`U+2800`–`U+28FF`) into `canvas`,
/// returning `true` if `cp` is a braille codepoint. Faithful port of upstream
/// `draw2800_28FF`: it sizes the 8-dot grid to the cell with a fixed refinement
/// pass, then draws a `w × w` box at each set dot.
pub(crate) fn draw_braille(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    if !(0x2800..=0x28FF).contains(&cp) {
        return false;
    }

    let width = metrics.cell_width as i32;
    let height = metrics.cell_height as i32;

    let mut w: i32 = (metrics.cell_width / 4).min(metrics.cell_height / 8) as i32;
    let mut x_spacing: i32 = (metrics.cell_width / 4) as i32;
    let mut y_spacing: i32 = (metrics.cell_height / 8) as i32;
    let mut x_margin: i32 = x_spacing.div_euclid(2);
    let mut y_margin: i32 = y_spacing.div_euclid(2);

    let mut x_px_left: i32 = width - 2 * x_margin - x_spacing - 2 * w;
    let mut y_px_left: i32 = height - 2 * y_margin - 3 * y_spacing - 4 * w;

    // First, try hard to ensure the dot width is non-zero.
    if x_px_left >= 2 && y_px_left >= 4 && w == 0 {
        w += 1;
        x_px_left -= 2;
        y_px_left -= 4;
    }

    // Second, prefer a non-zero margin.
    if x_px_left >= 2 && x_margin == 0 {
        x_margin = 1;
        x_px_left -= 2;
    }
    if y_px_left >= 2 && y_margin == 0 {
        y_margin = 1;
        y_px_left -= 2;
    }

    // Third, increase spacing.
    if x_px_left >= 1 {
        x_spacing += 1;
        x_px_left -= 1;
    }
    if y_px_left >= 3 {
        y_spacing += 1;
        y_px_left -= 3;
    }

    // Fourth, margins ("spacing", but on the sides).
    if x_px_left >= 2 {
        x_margin += 1;
        x_px_left -= 2;
    }
    if y_px_left >= 2 {
        y_margin += 1;
        y_px_left -= 2;
    }

    // Last, increase dot width.
    if x_px_left >= 2 && y_px_left >= 4 {
        w += 1;
        x_px_left -= 2;
        y_px_left -= 4;
    }

    assert!(x_px_left <= 1 || y_px_left <= 1);
    assert!(2 * x_margin + 2 * w + x_spacing <= width);
    assert!(2 * y_margin + 4 * w + 3 * y_spacing <= height);

    let x = [x_margin, x_margin + w + x_spacing];
    let y = {
        let mut y = [0i32; 4];
        y[0] = y_margin;
        y[1] = y[0] + w + y_spacing;
        y[2] = y[1] + w + y_spacing;
        y[3] = y[2] + w + y_spacing;
        y
    };

    let p = BraillePattern::from_cp(cp);
    let mut dot = |col: usize, row: usize| {
        canvas.r#box(x[col], y[row], x[col] + w, y[row] + w, Color::ON);
    };
    if p.tl {
        dot(0, 0);
    }
    if p.ul {
        dot(0, 1);
    }
    if p.ll {
        dot(0, 2);
    }
    if p.bl {
        dot(0, 3);
    }
    if p.tr {
        dot(1, 0);
    }
    if p.ur {
        dot(1, 1);
    }
    if p.lr {
        dot(1, 2);
    }
    if p.br {
        dot(1, 3);
    }
    true
}

/// The six cell flags of a sextant glyph, in the upstream bit order
/// `tl, tr, ml, mr, bl, br`. Faithful port of upstream `Sextants`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sextants {
    tl: bool,
    tr: bool,
    ml: bool,
    mr: bool,
    bl: bool,
    br: bool,
}

impl Sextants {
    /// Decode the sextant pattern for `cp`. The pattern index skips the blank
    /// (`0`) and the two half-column patterns (`21`/`42`, which are the left/
    /// right half blocks) via `idx + idx/0x14 + 1`. Faithful port of upstream.
    fn from_cp(cp: u32) -> Sextants {
        let idx = cp - 0x1FB00;
        let sex = ((idx + idx / 0x14 + 1) & 0x3F) as u8;
        Sextants {
            tl: sex & 0x01 != 0,
            tr: sex & 0x02 != 0,
            ml: sex & 0x04 != 0,
            mr: sex & 0x08 != 0,
            bl: sex & 0x10 != 0,
            br: sex & 0x20 != 0,
        }
    }
}

/// Draw the sextant glyph for `cp` (`U+1FB00`–`U+1FB3B`) into `canvas`,
/// returning `true` if `cp` is a sextant. Faithful port of upstream
/// `draw1FB00_1FB3B`: a 2×3 grid of `fill`ed cells selected by the pattern.
pub(crate) fn draw_sextant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    if !(0x1FB00..=0x1FB3B).contains(&cp) {
        return false;
    }
    use Fraction::{End, Full, Half, OneThird, TwoThirds, Zero};
    let s = Sextants::from_cp(cp);
    if s.tl {
        fill(metrics, canvas, Zero, Half, Zero, OneThird);
    }
    if s.tr {
        fill(metrics, canvas, Half, Full, Zero, OneThird);
    }
    if s.ml {
        fill(metrics, canvas, Zero, Half, OneThird, TwoThirds);
    }
    if s.mr {
        fill(metrics, canvas, Half, Full, OneThird, TwoThirds);
    }
    if s.bl {
        fill(metrics, canvas, Zero, Half, TwoThirds, End);
    }
    if s.br {
        fill(metrics, canvas, Half, Full, TwoThirds, End);
    }
    true
}

/// Draw the Separated Block Quadrant glyph for `cp` (`U+1CC21`–`U+1CC2F`) into
/// `canvas`, returning `true` if `cp` is one. Faithful port of upstream
/// `draw1CC21_1CC2F`: a 2×2 grid of `w × h` boxes with gaps between them,
/// selected by the low nibble of `cp - 0x1CC20`.
pub(crate) fn draw_separated_quadrant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    if !(0x1CC21..=0x1CC2F).contains(&cp) {
        return false;
    }
    let q = (cp - 0x1CC20) as u8;
    let (tl, tr, bl, br) = (q & 0x01 != 0, q & 0x02 != 0, q & 0x04 != 0, q & 0x08 != 0);

    let width = metrics.cell_width as i32;
    let height = metrics.cell_height as i32;

    let gap: i32 = (metrics.cell_width / 12).max(1) as i32;
    let mid_gap_x: i32 = gap * 2 + (metrics.cell_width % 2) as i32;
    let mid_gap_y: i32 = gap * 2 + (metrics.cell_height % 2) as i32;

    // Upstream uses @divExact; the numerator is provably even (dim - dim%2 is
    // even and 4*gap is even), so an exact /2 with an assertion matches.
    let w_num = width - gap * 2 - mid_gap_x;
    let h_num = height - gap * 2 - mid_gap_y;
    assert!(w_num % 2 == 0 && h_num % 2 == 0);
    let w = w_num / 2;
    let h = h_num / 2;

    if tl {
        canvas.r#box(gap, gap, gap + w, gap + h, Color::ON);
    }
    if tr {
        canvas.r#box(
            gap + w + mid_gap_x,
            gap,
            gap + w + mid_gap_x + w,
            gap + h,
            Color::ON,
        );
    }
    if bl {
        canvas.r#box(
            gap,
            gap + h + mid_gap_y,
            gap + w,
            gap + h + mid_gap_y + h,
            Color::ON,
        );
    }
    if br {
        canvas.r#box(
            gap + w + mid_gap_x,
            gap + h + mid_gap_y,
            gap + w + mid_gap_x + w,
            gap + h + mid_gap_y + h,
            Color::ON,
        );
    }
    true
}

/// Parse the embedded `octants.txt` into the 230-entry octant lookup table at
/// compile time. Faithful port of upstream's comptime `@embedFile` + parse: each
/// non-comment line `BLOCK OCTANT-<digits>` sets bit `(digit - '1')` for each
/// digit after the `-`; the Nth data line is the pattern for codepoint
/// `0x1CD00 + N`.
const fn parse_octants(data: &str) -> [u8; 230] {
    let bytes = data.as_bytes();
    let mut table = [0u8; 230];
    let mut i = 0; // table (data-line) index
    let mut pos = 0; // byte cursor

    while pos < bytes.len() {
        // Find the end of the current line.
        let start = pos;
        while pos < bytes.len() && bytes[pos] != b'\n' {
            pos += 1;
        }
        let mut end = pos; // exclusive (at '\n' or EOF)
        pos += 1; // step past the '\n' (or past EOF)

        // Trim a trailing '\r' (CRLF checkouts).
        if end > start && bytes[end - 1] == b'\r' {
            end -= 1;
        }

        // Skip blank lines and comments.
        if end == start || bytes[start] == b'#' {
            continue;
        }

        // Find the '-' and OR in each trailing digit's bit.
        let mut k = start;
        while k < end && bytes[k] != b'-' {
            k += 1;
        }
        k += 1; // step past the '-'
        let mut oct = 0u8;
        while k < end {
            oct |= 1u8 << (bytes[k] - b'1');
            k += 1;
        }

        table[i] = oct;
        i += 1;
    }

    assert!(i == 230);
    table
}

/// The octant lookup table: one byte per codepoint `0x1CD00..=0x1CDE5`, the
/// cell bits `1..8` in bit positions `0..7`.
const OCTANTS: [u8; 230] = parse_octants(include_str!("octants.txt"));

/// Draw the octant glyph for `cp` (`U+1CD00`–`U+1CDE5`) into `canvas`, returning
/// `true` if `cp` is an octant. Faithful port of upstream `draw1CD00_1CDE5`: a
/// 2×4 (quarter-height) grid of `fill`ed cells selected by the [`OCTANTS`]
/// pattern.
pub(crate) fn draw_octant(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    if !(0x1CD00..=0x1CDE5).contains(&cp) {
        return false;
    }
    use Fraction::{Full, Half, OneQuarter, ThreeQuarters, TwoQuarters, Zero};
    let oct = OCTANTS[(cp - 0x1CD00) as usize];
    if oct & 0x01 != 0 {
        fill(metrics, canvas, Zero, Half, Zero, OneQuarter);
    }
    if oct & 0x02 != 0 {
        fill(metrics, canvas, Half, Full, Zero, OneQuarter);
    }
    if oct & 0x04 != 0 {
        fill(metrics, canvas, Zero, Half, OneQuarter, TwoQuarters);
    }
    if oct & 0x08 != 0 {
        fill(metrics, canvas, Half, Full, OneQuarter, TwoQuarters);
    }
    if oct & 0x10 != 0 {
        fill(metrics, canvas, Zero, Half, TwoQuarters, ThreeQuarters);
    }
    if oct & 0x20 != 0 {
        fill(metrics, canvas, Half, Full, TwoQuarters, ThreeQuarters);
    }
    if oct & 0x40 != 0 {
        fill(metrics, canvas, Zero, Half, ThreeQuarters, Fraction::End);
    }
    if oct & 0x80 != 0 {
        fill(metrics, canvas, Half, Full, ThreeQuarters, Fraction::End);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_metrics() -> Metrics {
        Metrics {
            cell_width: 9,
            cell_height: 18,
            cell_baseline: 4,
            underline_position: 15,
            underline_thickness: 1,
            strikethrough_position: 9,
            strikethrough_thickness: 1,
            overline_position: 0,
            overline_thickness: 1,
            box_thickness: 2,
            cursor_thickness: 1,
            cursor_height: 18,
            icon_height: 16.0,
            icon_height_single: 16.0,
            face_width: 9.0,
            face_height: 18.0,
            face_y: 0.0,
        }
    }

    /// A fresh unpadded canvas sized to the fixture cell.
    fn cell_canvas() -> Canvas {
        Canvas::new(9, 18, 0, 0)
    }

    fn inked(canvas: &Canvas, x: i32, y: i32) -> bool {
        canvas.get(x, y) != 0
    }

    #[test]
    fn thickness_heights() {
        assert_eq!(Thickness::Light.height(2), 2);
        assert_eq!(Thickness::Heavy.height(2), 4);
        assert_eq!(Thickness::SuperLight.height(2), 1);
        assert_eq!(Thickness::SuperLight.height(1), 1);
    }

    #[test]
    fn box_light_horizontal() {
        // box_thickness = 2 -> light stroke 2px tall, centered: rows 8,9.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2500, &m, &mut c));
        let top = (m.cell_height - 2) / 2; // 8
                                           // The band spans the full width at rows [top, top+2).
        for x in 0..m.cell_width as i32 {
            assert!(inked(&c, x, top as i32), "band at x={x}");
            assert!(inked(&c, x, top as i32 + 1), "band at x={x}");
        }
        // Nothing above the band or below it.
        for x in 0..m.cell_width as i32 {
            assert!(!inked(&c, x, top as i32 - 1), "above band at x={x}");
            assert!(!inked(&c, x, top as i32 + 2), "below band at x={x}");
        }
        // Top and bottom rows are empty.
        for x in 0..m.cell_width as i32 {
            assert!(!inked(&c, x, 0));
            assert!(!inked(&c, x, m.cell_height as i32 - 1));
        }
    }

    #[test]
    fn box_light_vertical() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2502, &m, &mut c));
        let left = (m.cell_width - 2) / 2; // 3
                                           // The band spans the full height at columns [left, left+2).
        for y in 0..m.cell_height as i32 {
            assert!(inked(&c, left as i32, y), "band at y={y}");
            assert!(inked(&c, left as i32 + 1, y), "band at y={y}");
        }
        // Empty columns to either side.
        for y in 0..m.cell_height as i32 {
            assert!(!inked(&c, left as i32 - 1, y), "left of band at y={y}");
            assert!(!inked(&c, left as i32 + 2, y), "right of band at y={y}");
            assert!(!inked(&c, 0, y));
            assert!(!inked(&c, m.cell_width as i32 - 1, y));
        }
    }

    #[test]
    fn box_light_cross() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x253C, &m, &mut c));
        let h_top = (m.cell_height - 2) / 2; // 8
        let v_left = (m.cell_width - 2) / 2; // 3
                                             // Horizontal band across the full width at the center rows.
        for x in 0..m.cell_width as i32 {
            assert!(inked(&c, x, h_top as i32), "h band at x={x}");
        }
        // Vertical band down the full height at the center columns.
        for y in 0..m.cell_height as i32 {
            assert!(inked(&c, v_left as i32, y), "v band at y={y}");
        }
        // The center is filled (both strokes overlap there).
        assert!(inked(&c, v_left as i32, h_top as i32));
    }

    #[test]
    fn box_heavy_horizontal() {
        // Heavy stroke = 2 * light = 4px tall.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2501, &m, &mut c));
        let top = (m.cell_height - 4) / 2; // 7
        let mut rows = 0;
        for y in 0..m.cell_height as i32 {
            if inked(&c, 0, y) {
                rows += 1;
            }
        }
        assert_eq!(rows, 4, "heavy horizontal is twice the light height");
        for x in 0..m.cell_width as i32 {
            for y in top as i32..top as i32 + 4 {
                assert!(inked(&c, x, y), "heavy band at ({x},{y})");
            }
        }
    }

    #[test]
    fn box_double_horizontal() {
        // box_thickness = 2: light_px = 2. h_light_top = 8, h_light_bottom = 10,
        // h_double_top = 6, h_double_bottom = 12. Two bands: [6,8) and [10,12),
        // with a 2px gap [8,10).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2550, &m, &mut c));
        for x in 0..m.cell_width as i32 {
            // Upper band rows 6,7.
            assert!(inked(&c, x, 6), "upper band at x={x}");
            assert!(inked(&c, x, 7), "upper band at x={x}");
            // Gap rows 8,9.
            assert!(!inked(&c, x, 8), "gap at x={x}");
            assert!(!inked(&c, x, 9), "gap at x={x}");
            // Lower band rows 10,11.
            assert!(inked(&c, x, 10), "lower band at x={x}");
            assert!(inked(&c, x, 11), "lower band at x={x}");
        }
    }

    #[test]
    fn box_double_vertical() {
        // light_px = 2. v_light_left = 3, v_light_right = 5, v_double_left = 1,
        // v_double_right = 7. Two bands: cols [1,3) and [5,7), gap [3,5).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2551, &m, &mut c));
        for y in 0..m.cell_height as i32 {
            assert!(inked(&c, 1, y), "left band at y={y}");
            assert!(inked(&c, 2, y), "left band at y={y}");
            assert!(!inked(&c, 3, y), "gap at y={y}");
            assert!(!inked(&c, 4, y), "gap at y={y}");
            assert!(inked(&c, 5, y), "right band at y={y}");
            assert!(inked(&c, 6, y), "right band at y={y}");
        }
    }

    #[test]
    fn box_double_cross() {
        // All four double: the perpendicular meeting points notch each arm so
        // the center light-stroke rectangle ([v_light_left,v_light_right) x
        // [h_light_top,h_light_bottom)) stays unfilled. Center cell pixel off.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x256C, &m, &mut c));
        let v_left = (m.cell_width - 2) / 2; // 3
        let h_top = (m.cell_height - 2) / 2; // 8
                                             // The center rectangle [3,5) x [8,10) is the unfilled hole.
        for x in v_left as i32..v_left as i32 + 2 {
            for y in h_top as i32..h_top as i32 + 2 {
                assert!(!inked(&c, x, y), "center hole at ({x},{y})");
            }
        }
        // But the four double arms still drew ink (sanity: top-left vertical
        // stroke and a left horizontal stroke are present).
        assert!(inked(&c, 1, 0), "up-left stroke at top edge");
        assert!(inked(&c, 0, 6), "left-upper stroke at left edge");
    }

    #[test]
    fn draw_box_lines_unknown() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(!draw_box_lines('M' as u32, &m, &mut c));
        for y in 0..m.cell_height as i32 {
            for x in 0..m.cell_width as i32 {
                assert!(!inked(&c, x, y), "nothing drawn at ({x},{y})");
            }
        }
    }

    /// The four contiguous `linesChar` codepoint ranges, written independently
    /// of the `BOX_LINES` table to guard its exact codepoint set.
    fn expected_cps() -> Vec<u32> {
        let mut v = Vec::new();
        v.extend(0x2500..=0x2503);
        v.extend(0x250C..=0x254B);
        v.extend(0x2550..=0x256C);
        v.extend(0x2574..=0x257F);
        v
    }

    #[test]
    fn table_codepoint_set() {
        let expected = expected_cps();
        assert_eq!(expected.len(), 4 + 64 + 29 + 12);
        assert_eq!(expected.len(), 109);

        // No duplicate codepoints in the table.
        let mut cps: Vec<u32> = BOX_LINES.iter().map(|(c, _)| *c).collect();
        let unique: std::collections::BTreeSet<u32> = cps.iter().copied().collect();
        assert_eq!(
            unique.len(),
            cps.len(),
            "BOX_LINES has duplicate codepoints"
        );

        // The table's codepoints, sorted, equal the expected set exactly.
        cps.sort_unstable();
        assert_eq!(cps, expected);
    }

    #[test]
    fn table_exact_mappings() {
        // Independently transcribed representatives from every block.
        let cases: &[(u32, Lines)] = &[
            (0x2501, lines(N, H, N, H)), // ━ heavy horizontal
            (0x250D, lines(N, H, L, N)), // ┍ corner: down light, right heavy
            (0x251C, lines(L, L, L, N)), // ├ tee: up/down/right light
            (0x2540, lines(H, L, L, L)), // ╀ cross: up heavy, rest light
            (0x254B, lines(H, H, H, H)), // ╋ heavy cross
            (0x2552, lines(N, D, L, N)), // ╒ down light, right double
            (0x256B, lines(D, L, D, L)), // ╫ up/down double, left/right light
            (0x257C, lines(N, H, N, L)), // ╼ left light, right heavy
            (0x257F, lines(H, N, L, N)), // ╿ up heavy, down light
        ];
        for (cp, expected) in cases {
            assert_eq!(
                box_lines_styles(*cp),
                Some(*expected),
                "mapping for {cp:#06x}"
            );
        }
    }

    #[test]
    fn dispatch_covers_all_line_chars() {
        let m = fixture_metrics();
        for cp in expected_cps() {
            let mut c = cell_canvas();
            assert!(draw_box_lines(cp, &m, &mut c), "dispatched {cp:#06x}");
            // Every line glyph draws at least one inked pixel.
            let any = (0..m.cell_height as i32)
                .any(|y| (0..m.cell_width as i32).any(|x| inked(&c, x, y)));
            assert!(any, "{cp:#06x} drew no ink");
        }
    }

    #[test]
    fn dispatch_excludes_non_line_chars() {
        let m = fixture_metrics();
        let mut deferred: Vec<u32> = Vec::new();
        deferred.extend(0x2504..=0x250B); // dashes
        deferred.extend(0x254C..=0x254F); // double/triple dashes
        deferred.extend(0x256D..=0x2573); // rounded corners + diagonals
        deferred.push('M' as u32);
        for cp in deferred {
            let mut c = cell_canvas();
            assert!(!draw_box_lines(cp, &m, &mut c), "{cp:#06x} must defer");
            let any = (0..m.cell_height as i32)
                .any(|y| (0..m.cell_width as i32).any(|x| inked(&c, x, y)));
            assert!(!any, "{cp:#06x} drew ink but should defer");
        }
    }

    #[test]
    fn tee_right_light() {
        // ├ (0x251C): up+down+right light. Full-height vertical band, right-half
        // horizontal band; no left stub.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x251C, &m, &mut c));
        let v_left = (m.cell_width - 2) / 2; // 3
        let h_top = (m.cell_height - 2) / 2; // 8
                                             // Vertical band spans full height.
        for y in 0..m.cell_height as i32 {
            assert!(inked(&c, v_left as i32, y), "vertical band at y={y}");
        }
        // Right-half horizontal band present at the center row.
        assert!(
            inked(&c, m.cell_width as i32 - 1, h_top as i32),
            "right arm"
        );
        // Left half of the center row is empty (no left stub).
        assert!(!inked(&c, 0, h_top as i32), "no left stub");
    }

    #[test]
    fn tee_down_light() {
        // ┬ (0x252C): down+left+right light. Full-width horizontal band,
        // down-half vertical band; no up stub.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x252C, &m, &mut c));
        let v_left = (m.cell_width - 2) / 2; // 3
        let h_top = (m.cell_height - 2) / 2; // 8
                                             // Horizontal band spans full width.
        for x in 0..m.cell_width as i32 {
            assert!(inked(&c, x, h_top as i32), "horizontal band at x={x}");
        }
        // Down-half vertical band present at the bottom.
        assert!(
            inked(&c, v_left as i32, m.cell_height as i32 - 1),
            "down arm"
        );
        // Up half of the center column is empty (no up stub).
        assert!(!inked(&c, v_left as i32, 0), "no up stub");
    }

    #[test]
    fn stub_left_light() {
        // ╴ (0x2574): left light only. Only the left half of the center row is
        // inked (x in [0, v_light_right)), the right half empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2574, &m, &mut c));
        let h_top = (m.cell_height - 2) / 2; // 8
        assert!(inked(&c, 0, h_top as i32), "left edge inked");
        assert!(inked(&c, 4, h_top as i32), "up to center inked");
        // The right half is empty.
        assert!(!inked(&c, 5, h_top as i32), "right of center empty");
        assert!(
            !inked(&c, m.cell_width as i32 - 1, h_top as i32),
            "right edge empty"
        );
    }

    #[test]
    fn stub_up_light() {
        // ╵ (0x2575): up light only. Only the top half of the center column is
        // inked, the bottom half empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_lines(0x2575, &m, &mut c));
        let v_left = (m.cell_width - 2) / 2; // 3
        assert!(inked(&c, v_left as i32, 0), "top edge inked");
        assert!(inked(&c, v_left as i32, 4), "down to center inked");
        // The bottom half is empty.
        assert!(!inked(&c, v_left as i32, 10), "below center empty");
        assert!(
            !inked(&c, v_left as i32, m.cell_height as i32 - 1),
            "bottom edge empty"
        );
    }

    /// The fixture metrics with a custom cell width (for the dash fallback).
    fn fixture_metrics_width(cell_width: u32) -> Metrics {
        Metrics {
            cell_width,
            ..fixture_metrics()
        }
    }

    /// All contiguous inked spans of a row `y` (as `[start, end)` ranges).
    fn row_spans(c: &Canvas, y: i32, width: u32) -> Vec<(i32, i32)> {
        let mut spans = Vec::new();
        let mut start: Option<i32> = None;
        for x in 0..width as i32 {
            if inked(c, x, y) {
                start.get_or_insert(x);
            } else if let Some(s) = start.take() {
                spans.push((s, x));
            }
        }
        if let Some(s) = start {
            spans.push((s, width as i32));
        }
        spans
    }

    /// All contiguous inked spans of a column `x`.
    fn col_spans(c: &Canvas, x: i32, height: u32) -> Vec<(i32, i32)> {
        let mut spans = Vec::new();
        let mut start: Option<i32> = None;
        for y in 0..height as i32 {
            if inked(c, x, y) {
                start.get_or_insert(y);
            } else if let Some(s) = start.take() {
                spans.push((s, y));
            }
        }
        if let Some(s) = start {
            spans.push((s, height as i32));
        }
        spans
    }

    #[test]
    fn dash_horizontal_3() {
        // 0x2504: count 3, light (2px), gap max(4,2)=4 clamped to 9/6=1.
        // dashes [0,2),[3,5),[6,8) on rows 8,9; gaps at x=2,5,8.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_dashes(0x2504, &m, &mut c));
        assert_eq!(row_spans(&c, 8, m.cell_width), vec![(0, 2), (3, 5), (6, 8)]);
        assert_eq!(row_spans(&c, 9, m.cell_width), vec![(0, 2), (3, 5), (6, 8)]);
        // Vertically centered: nothing on rows 7 or 10.
        assert!(row_spans(&c, 7, m.cell_width).is_empty());
        assert!(row_spans(&c, 10, m.cell_width).is_empty());
    }

    #[test]
    fn dash_vertical_3() {
        // 0x2506: count 3, light (2px), gap clamped to 18/6=3.
        // dashes [0,3),[6,9),[12,15) on cols 3,4.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_dashes(0x2506, &m, &mut c));
        assert_eq!(
            col_spans(&c, 3, m.cell_height),
            vec![(0, 3), (6, 9), (12, 15)]
        );
        assert_eq!(
            col_spans(&c, 4, m.cell_height),
            vec![(0, 3), (6, 9), (12, 15)]
        );
        // Horizontally centered: nothing on cols 2 or 5.
        assert!(col_spans(&c, 2, m.cell_height).is_empty());
        assert!(col_spans(&c, 5, m.cell_height).is_empty());
    }

    #[test]
    fn dash_count_4() {
        // 0x2508: count 4, light (2px), gap clamped to 9/8=1. total_dash=5,
        // dash_width=1, remaining=1 -> first dash gets the extra pixel.
        // dashes [0,2),[3,4),[5,6),[7,8) on rows 8,9.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_dashes(0x2508, &m, &mut c));
        assert_eq!(
            row_spans(&c, 8, m.cell_width),
            vec![(0, 2), (3, 4), (5, 6), (7, 8)]
        );
    }

    #[test]
    fn dash_double_2() {
        // 0x254C: count 2, light (2px), gap light=2 (clamped to 9/4=2).
        // total_gap=4, total_dash=5, dash_width=2, remaining=1, x0=gap/2=1.
        // dashes [1,4),[6,8) on rows 8,9.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_dashes(0x254C, &m, &mut c));
        assert_eq!(row_spans(&c, 8, m.cell_width), vec![(1, 4), (6, 8)]);
    }

    #[test]
    fn dash_heavy_thickness() {
        // 0x2505: count 3, heavy (4px). Same x-pattern as 0x2504 but the band is
        // 4px tall (rows 7..11), centered at y=(18-4)/2=7.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_dashes(0x2505, &m, &mut c));
        // A dash column (x=0) is inked on rows 7,8,9,10 only.
        assert_eq!(col_spans(&c, 0, m.cell_height), vec![(7, 11)]);
        assert!(!inked(&c, 0, 6));
        assert!(!inked(&c, 0, 11));
    }

    #[test]
    fn dash_fallback_solid() {
        // A cell too narrow for the dashes (cell_width 5 < count 3 + gaps 3)
        // falls back to a solid light line across the full width.
        let m = fixture_metrics_width(5);
        let mut c = Canvas::new(5, 18, 0, 0);
        assert!(draw_box_dashes(0x2504, &m, &mut c));
        // Solid: rows 8,9 inked continuously, no gaps.
        assert_eq!(row_spans(&c, 8, m.cell_width), vec![(0, 5)]);
        assert_eq!(row_spans(&c, 9, m.cell_width), vec![(0, 5)]);
    }

    #[test]
    fn draw_box_dashes_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x253C, 0x2550, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(!draw_box_dashes(cp, &m, &mut c), "{cp:#06x} not a dash");
            let any = (0..m.cell_height as i32)
                .any(|y| (0..m.cell_width as i32).any(|x| inked(&c, x, y)));
            assert!(!any, "{cp:#06x} drew ink");
        }
    }

    #[test]
    fn fraction_values() {
        assert_eq!(Fraction::Zero.fraction(), 0.0);
        assert_eq!(Fraction::OneEighth.fraction(), 0.125);
        assert_eq!(Fraction::TwoEighths.fraction(), 0.25);
        assert_eq!(Fraction::Quarter.fraction(), 0.25);
        assert_eq!(Fraction::ThreeEighths.fraction(), 0.375);
        assert_eq!(Fraction::Half.fraction(), 0.5);
        assert_eq!(Fraction::Center.fraction(), 0.5);
        assert_eq!(Fraction::FiveEighths.fraction(), 0.625);
        assert_eq!(Fraction::TwoThirds.fraction(), 2.0 / 3.0);
        assert_eq!(Fraction::ThreeQuarters.fraction(), 0.75);
        assert_eq!(Fraction::SevenEighths.fraction(), 0.875);
        assert_eq!(Fraction::Full.fraction(), 1.0);
        // Aliases collapse to the same value.
        for f in [
            Fraction::Start,
            Fraction::Left,
            Fraction::Top,
            Fraction::Zero,
        ] {
            assert_eq!(f.fraction(), 0.0);
        }
        for f in [
            Fraction::End,
            Fraction::Right,
            Fraction::Bottom,
            Fraction::One,
            Fraction::Full,
        ] {
            assert_eq!(f.fraction(), 1.0);
        }
        assert_eq!(Fraction::OneThird.fraction(), 1.0 / 3.0);
        assert_eq!(Fraction::SixEighths.fraction(), 0.75);
        // float() scales without rounding.
        assert_eq!(Fraction::Half.float(7), 3.5);
    }

    #[test]
    fn min_max_even_tiling() {
        // The upstream doc example: size 7 splits into two even 4px halves.
        assert_eq!(Fraction::Half.min(7), 3);
        assert_eq!(Fraction::Half.max(7), 4);
        assert_eq!(Fraction::Zero.min(7), 0);
        assert_eq!(Fraction::Full.max(7), 7);
        // start->half and half->end are both 4px (even tiling).
        assert_eq!(Fraction::Half.max(7) - Fraction::Zero.min(7), 4);
        assert_eq!(Fraction::Full.max(7) - Fraction::Half.min(7), 4);
    }

    #[test]
    fn min_max_exact_half() {
        // Even size splits cleanly.
        assert_eq!(Fraction::Half.max(8), 4);
        assert_eq!(Fraction::Half.min(8), 4);
        // Odd size: max rounds 4.5 away from zero -> 5; min is the complement.
        assert_eq!(Fraction::Half.max(9), 5);
        assert_eq!(Fraction::Half.min(9), 4);
    }

    #[test]
    fn fill_top_left_quadrant() {
        // fill(Zero, Half, Zero, Half) on 9x18 -> x[0,5) y[0,9).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        fill(
            &m,
            &mut c,
            Fraction::Zero,
            Fraction::Half,
            Fraction::Zero,
            Fraction::Half,
        );
        assert!(inked(&c, 0, 0));
        assert!(inked(&c, 4, 8));
        assert!(!inked(&c, 5, 0), "x=5 is outside [0,5)");
        assert!(!inked(&c, 0, 9), "y=9 is outside [0,9)");
        // Exact span on row 0 and column 0.
        assert_eq!(row_spans(&c, 0, m.cell_width), vec![(0, 5)]);
        assert_eq!(col_spans(&c, 0, m.cell_height), vec![(0, 9)]);
    }

    #[test]
    fn fill_bottom_right_quadrant() {
        // fill(Half, Full, Half, Full) on 9x18 -> x[4,9) y[9,18).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        fill(
            &m,
            &mut c,
            Fraction::Half,
            Fraction::Full,
            Fraction::Half,
            Fraction::Full,
        );
        assert!(inked(&c, 4, 9));
        assert!(inked(&c, 8, 17));
        assert!(!inked(&c, 3, 17), "x=3 is outside [4,9)");
        assert!(!inked(&c, 4, 8), "y=8 is outside [9,18)");
        assert_eq!(row_spans(&c, 17, m.cell_width), vec![(4, 9)]);
        assert_eq!(col_spans(&c, 8, m.cell_height), vec![(9, 18)]);
    }

    /// Whether every pixel of the cell has the given alpha.
    fn all_alpha(c: &Canvas, m: &Metrics, alpha: u8) -> bool {
        (0..m.cell_height as i32).all(|y| (0..m.cell_width as i32).all(|x| c.get(x, y) == alpha))
    }

    #[test]
    fn block_upper_half() {
        // 0x2580: w=9, h=round(9.0)=9, upper -> x[0,9) y[0,9).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2580, &m, &mut c));
        for x in 0..m.cell_width as i32 {
            assert_eq!(col_spans(&c, x, m.cell_height), vec![(0, 9)], "col {x}");
        }
    }

    #[test]
    fn block_lower_eighth() {
        // 0x2581: h=round(18*0.125)=round(2.25)=2, lower -> y[16,18).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2581, &m, &mut c));
        for x in 0..m.cell_width as i32 {
            assert_eq!(col_spans(&c, x, m.cell_height), vec![(16, 18)], "col {x}");
        }
    }

    #[test]
    fn block_lower_three_eighths() {
        // 0x2583: h=round(18*0.375)=round(6.75)=7, lower -> y[11,18).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2583, &m, &mut c));
        assert_eq!(col_spans(&c, 0, m.cell_height), vec![(11, 18)]);
    }

    #[test]
    fn block_left_half() {
        // 0x258C: w=round(9*0.5)=round(4.5)=5, left -> x[0,5) y[0,18).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x258C, &m, &mut c));
        for y in 0..m.cell_height as i32 {
            assert_eq!(row_spans(&c, y, m.cell_width), vec![(0, 5)], "row {y}");
        }
    }

    #[test]
    fn block_right_eighth() {
        // 0x2595: w=round(9*0.125)=round(1.125)=1, right -> x[8,9).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2595, &m, &mut c));
        assert_eq!(row_spans(&c, 0, m.cell_width), vec![(8, 9)]);
    }

    #[test]
    fn full_block_on() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2588, &m, &mut c));
        assert!(all_alpha(&c, &m, 0xFF));
    }

    #[test]
    fn full_block_shades() {
        // 0x2591/2/3 -> light/medium/dark alpha.
        let m = fixture_metrics();
        for (cp, alpha) in [(0x2591u32, 0x40u8), (0x2592, 0x80), (0x2593, 0xC0)] {
            let mut c = cell_canvas();
            assert!(draw_block(cp, &m, &mut c));
            assert!(all_alpha(&c, &m, alpha), "{cp:#06x} -> alpha {alpha:#x}");
        }
    }

    #[test]
    fn quadrant_bl() {
        // 0x2596: bottom-left -> x[0,5) y[9,18).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x2596, &m, &mut c));
        assert!(inked(&c, 0, 9));
        assert!(inked(&c, 4, 17));
        assert!(!inked(&c, 5, 17), "x=5 outside [0,5)");
        assert!(!inked(&c, 0, 8), "y=8 outside [9,18)");
        assert_eq!(col_spans(&c, 0, m.cell_height), vec![(9, 18)]);
        assert_eq!(row_spans(&c, 17, m.cell_width), vec![(0, 5)]);
    }

    #[test]
    fn quadrant_diagonal() {
        // 0x259A: tl + br. TL x[0,5)y[0,9), BR x[4,9)y[9,18). TR and BL empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x259A, &m, &mut c));
        // TL present.
        assert!(inked(&c, 0, 0));
        assert!(inked(&c, 4, 8));
        // BR present.
        assert!(inked(&c, 8, 17));
        assert!(inked(&c, 4, 9));
        // TR (x[5,9) y[0,9)) empty.
        assert!(!inked(&c, 8, 0), "TR empty");
        // BL (x[0,4) y[9,18)) empty.
        assert!(!inked(&c, 0, 17), "BL empty");
    }

    #[test]
    fn quadrant_three() {
        // 0x259F: tr + bl + br. TL (x[0,4) y[0,9)) empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_block(0x259F, &m, &mut c));
        assert!(inked(&c, 8, 0), "TR present");
        assert!(inked(&c, 0, 17), "BL present");
        assert!(inked(&c, 8, 17), "BR present");
        assert!(!inked(&c, 0, 0), "TL empty");
    }

    #[test]
    fn draw_block_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x257F, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(!draw_block(cp, &m, &mut c), "{cp:#06x} not a block");
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    /// The exact `[start, end)` span (single dot) expected at column `col`,
    /// row `row` for the `9×18` braille layout: `x=[1,6]`, `y=[2,6,10,14]`,
    /// `w=2`.
    fn braille_dot(col: usize, row: usize) -> (i32, i32, i32, i32) {
        let x = [1, 6];
        let y = [2, 6, 10, 14];
        (x[col], y[row], x[col] + 2, y[row] + 2)
    }

    fn only_dots_inked(c: &Canvas, m: &Metrics, dots: &[(usize, usize)]) {
        // Every inked pixel must belong to one of the expected dot rectangles.
        let rects: Vec<(i32, i32, i32, i32)> =
            dots.iter().map(|&(c, r)| braille_dot(c, r)).collect();
        for y in 0..m.cell_height as i32 {
            for x in 0..m.cell_width as i32 {
                let want = rects
                    .iter()
                    .any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
                assert_eq!(inked(c, x, y), want, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn braille_layout_blank() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_braille(0x2800, &m, &mut c));
        assert!(all_alpha(&c, &m, 0), "blank braille draws nothing");
    }

    #[test]
    fn braille_dot_tl() {
        // 0x2801: tl only -> x[1,3) y[2,4).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_braille(0x2801, &m, &mut c));
        only_dots_inked(&c, &m, &[(0, 0)]);
    }

    #[test]
    fn braille_dot_br() {
        // 0x2880: br only -> x[6,8) y[14,16).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_braille(0x2880, &m, &mut c));
        only_dots_inked(&c, &m, &[(1, 3)]);
    }

    #[test]
    fn braille_bit_mapping() {
        // 0x284D = 0x4D = bits tl, ll, tr, bl -> (0,0),(0,2),(1,0),(0,3).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_braille(0x284D, &m, &mut c));
        only_dots_inked(&c, &m, &[(0, 0), (0, 2), (1, 0), (0, 3)]);
    }

    #[test]
    fn braille_all() {
        // 0x28FF: all eight dots.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_braille(0x28FF, &m, &mut c));
        only_dots_inked(
            &c,
            &m,
            &[
                (0, 0),
                (0, 1),
                (0, 2),
                (0, 3),
                (1, 0),
                (1, 1),
                (1, 2),
                (1, 3),
            ],
        );
    }

    #[test]
    fn draw_braille_excludes() {
        let m = fixture_metrics();
        for cp in [0x27FFu32, 0x2900, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(!draw_braille(cp, &m, &mut c), "{cp:#06x} not braille");
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    /// Sextant cell rectangle for the `9×18` fixture. Columns: left `[0,5)`,
    /// right `[4,9)`. Rows: top `[0,6)`, middle `[6,12)`, bottom `[12,18)`.
    /// `cell` is one of "tl","tr","ml","mr","bl","br".
    fn sextant_cell(cell: &str) -> (i32, i32, i32, i32) {
        let (x0, x1) = if cell.ends_with('l') { (0, 5) } else { (4, 9) };
        let (y0, y1) = match &cell[..1] {
            "t" => (0, 6),
            "m" => (6, 12),
            _ => (12, 18),
        };
        (x0, y0, x1, y1)
    }

    /// Assert every pixel belongs to exactly the union of the given cells.
    /// (Cells overlap by 1px at the center column for the odd 9px width — that
    /// is upstream-intentional; a pixel inside any listed cell must be inked.)
    fn cells_inked(c: &Canvas, m: &Metrics, cells: &[&str]) {
        let rects: Vec<(i32, i32, i32, i32)> = cells.iter().map(|s| sextant_cell(s)).collect();
        for y in 0..m.cell_height as i32 {
            for x in 0..m.cell_width as i32 {
                let want = rects
                    .iter()
                    .any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
                assert_eq!(inked(c, x, y), want, "pixel ({x},{y})");
            }
        }
    }

    #[test]
    fn sextant_first() {
        // 0x1FB00: idx 0 -> sex 1 -> tl.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_sextant(0x1FB00, &m, &mut c));
        cells_inked(&c, &m, &["tl"]);
    }

    #[test]
    fn sextant_second() {
        // 0x1FB01: idx 1 -> sex 2 -> tr.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_sextant(0x1FB01, &m, &mut c));
        cells_inked(&c, &m, &["tr"]);
    }

    #[test]
    fn sextant_tl_tr() {
        // 0x1FB02: idx 2 -> sex 3 -> tl+tr (whole top row).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_sextant(0x1FB02, &m, &mut c));
        cells_inked(&c, &m, &["tl", "tr"]);
    }

    #[test]
    fn sextant_index_jump() {
        // idx 19 -> sex 20 -> ml+bl; idx 20 -> sex 22 -> tr+ml+bl. The jump
        // (idx/0x14) skips sex value 21 between them.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_sextant(0x1FB13, &m, &mut c));
        cells_inked(&c, &m, &["ml", "bl"]);

        let mut c2 = cell_canvas();
        assert!(draw_sextant(0x1FB14, &m, &mut c2));
        cells_inked(&c2, &m, &["tr", "ml", "bl"]);
    }

    #[test]
    fn sextant_last() {
        // 0x1FB3B: idx 59 -> sex 62 -> all but tl.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_sextant(0x1FB3B, &m, &mut c));
        cells_inked(&c, &m, &["tr", "ml", "mr", "bl", "br"]);
    }

    #[test]
    fn draw_sextant_excludes() {
        let m = fixture_metrics();
        for cp in [0x1FAFFu32, 0x1FB3C, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(!draw_sextant(cp, &m, &mut c), "{cp:#07x} not a sextant");
            assert!(all_alpha(&c, &m, 0), "{cp:#07x} drew ink");
        }
    }

    /// Assert every cell pixel belongs to exactly the union of the given
    /// `[x0, y0, x1, y1)` rectangles.
    fn rects_inked(c: &Canvas, m: &Metrics, rects: &[(i32, i32, i32, i32)]) {
        for y in 0..m.cell_height as i32 {
            for x in 0..m.cell_width as i32 {
                let want = rects
                    .iter()
                    .any(|&(x0, y0, x1, y1)| x >= x0 && x < x1 && y >= y0 && y < y1);
                assert_eq!(inked(c, x, y), want, "pixel ({x},{y})");
            }
        }
    }

    // The four separated-quadrant boxes for the 9x18 fixture.
    const SQ_TL: (i32, i32, i32, i32) = (1, 1, 3, 8);
    const SQ_TR: (i32, i32, i32, i32) = (6, 1, 8, 8);
    const SQ_BL: (i32, i32, i32, i32) = (1, 10, 3, 17);
    const SQ_BR: (i32, i32, i32, i32) = (6, 10, 8, 17);

    #[test]
    fn sep_quad_tl() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_separated_quadrant(0x1CC21, &m, &mut c));
        rects_inked(&c, &m, &[SQ_TL]);
    }

    #[test]
    fn sep_quad_tr() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_separated_quadrant(0x1CC22, &m, &mut c));
        rects_inked(&c, &m, &[SQ_TR]);
    }

    #[test]
    fn sep_quad_bl() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_separated_quadrant(0x1CC24, &m, &mut c));
        rects_inked(&c, &m, &[SQ_BL]);
    }

    #[test]
    fn sep_quad_br() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_separated_quadrant(0x1CC28, &m, &mut c));
        rects_inked(&c, &m, &[SQ_BR]);
    }

    #[test]
    fn sep_quad_all() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_separated_quadrant(0x1CC2F, &m, &mut c));
        rects_inked(&c, &m, &[SQ_TL, SQ_TR, SQ_BL, SQ_BR]);
    }

    #[test]
    fn draw_separated_quadrant_excludes() {
        let m = fixture_metrics();
        for cp in [0x1CC20u32, 0x1CC30, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_separated_quadrant(cp, &m, &mut c),
                "{cp:#07x} not a separated quadrant"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#07x} drew ink");
        }
    }

    /// An `8×16` fixture so the octant halves/quarters divide cleanly.
    fn fixture_8x16() -> Metrics {
        Metrics {
            cell_width: 8,
            cell_height: 16,
            ..fixture_metrics()
        }
    }

    /// The `[x0, y0, x1, y1)` rect for octant cell `n` (1..8) on the `8×16`
    /// fixture: left column `[0,4)`, right `[4,8)`; rows `[0,4)`, `[4,8)`,
    /// `[8,12)`, `[12,16)`.
    fn octant_cell(n: u8) -> (i32, i32, i32, i32) {
        let (x0, x1) = if n % 2 == 1 { (0, 4) } else { (4, 8) };
        let row = ((n - 1) / 2) as i32; // 0..3
        let y0 = row * 4;
        (x0, y0, x1, y0 + 4)
    }

    fn octant_cells_inked(c: &Canvas, m: &Metrics, cells: &[u8]) {
        let rects: Vec<(i32, i32, i32, i32)> = cells.iter().map(|&n| octant_cell(n)).collect();
        rects_inked(c, m, &rects);
    }

    #[test]
    fn octant_table_first_entries() {
        // Validate the parser directly against known octants.txt lines.
        assert_eq!(OCTANTS[0], 0b0000_0100, "OCTANT-3");
        assert_eq!(OCTANTS[1], 0b0000_0110, "OCTANT-23");
        assert_eq!(OCTANTS[15], 0b0001_0111, "OCTANT-1235");
        assert_eq!(OCTANTS[229], 0b1111_1110, "OCTANT-2345678");
        assert_eq!(OCTANTS.len(), 230);
    }

    #[test]
    fn octant_first() {
        // 0x1CD00 -> OCTANTS[0] -> cell 3.
        let m = fixture_8x16();
        let mut c = Canvas::new(8, 16, 0, 0);
        assert!(draw_octant(0x1CD00, &m, &mut c));
        octant_cells_inked(&c, &m, &[3]);
    }

    #[test]
    fn octant_second() {
        // 0x1CD01 -> OCTANTS[1] -> cells 2, 3.
        let m = fixture_8x16();
        let mut c = Canvas::new(8, 16, 0, 0);
        assert!(draw_octant(0x1CD01, &m, &mut c));
        octant_cells_inked(&c, &m, &[2, 3]);
    }

    #[test]
    fn octant_multi() {
        // 0x1CD0F -> OCTANTS[15] -> cells 1, 2, 3, 5.
        let m = fixture_8x16();
        let mut c = Canvas::new(8, 16, 0, 0);
        assert!(draw_octant(0x1CD0F, &m, &mut c));
        octant_cells_inked(&c, &m, &[1, 2, 3, 5]);
    }

    #[test]
    fn octant_last() {
        // 0x1CDE5 -> OCTANTS[229] -> cells 2..8 (all but 1).
        let m = fixture_8x16();
        let mut c = Canvas::new(8, 16, 0, 0);
        assert!(draw_octant(0x1CDE5, &m, &mut c));
        octant_cells_inked(&c, &m, &[2, 3, 4, 5, 6, 7, 8]);
    }

    #[test]
    fn draw_octant_excludes() {
        let m = fixture_8x16();
        for cp in [0x1CCFFu32, 0x1CDE6, 'M' as u32] {
            let mut c = Canvas::new(8, 16, 0, 0);
            assert!(!draw_octant(cp, &m, &mut c), "{cp:#07x} not an octant");
            assert!(all_alpha(&c, &m, 0), "{cp:#07x} drew ink");
        }
    }

    // Box-drawing diagonals (Canvas::line + the z2d pipeline).

    #[test]
    fn diagonal_2572_orientation() {
        // ╲ : top-left to bottom-right. Passes through the center, not the
        // top-right corner.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_diagonal(0x2572, &m, &mut c));
        assert!(inked(&c, 4, 9), "center on the backslash");
        assert!(!inked(&c, 8, 1), "top-right corner off the backslash");
    }

    #[test]
    fn diagonal_2571_orientation() {
        // ╱ : bottom-left to top-right. Passes through the center, not the
        // top-left corner.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_diagonal(0x2571, &m, &mut c));
        assert!(inked(&c, 4, 9), "center on the slash");
        assert!(!inked(&c, 0, 1), "top-left corner off the slash");
    }

    #[test]
    fn diagonal_2573_cross() {
        // ╳ : both diagonals cross at the center.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_diagonal(0x2573, &m, &mut c));
        assert!(inked(&c, 4, 9), "center where the diagonals cross");
    }

    #[test]
    fn canvas_line_horizontal() {
        // A direct Canvas::line check: a 2px horizontal line centered at y=4
        // across a 9-wide unpadded canvas.
        let mut c = Canvas::new(9, 9, 0, 0);
        c.line(
            raster::Point::new(0.0, 4.0),
            raster::Point::new(9.0, 4.0),
            2.0,
        );
        // The band straddles y=4 (the line center): rows 3 and 4 inked across.
        for x in 1..8 {
            assert!(inked(&c, x, 3) || inked(&c, x, 4), "band at x={x}");
        }
        // Top and bottom rows are empty.
        for x in 0..9 {
            assert!(!inked(&c, x, 0), "top row empty at x={x}");
            assert!(!inked(&c, x, 8), "bottom row empty at x={x}");
        }
    }

    #[test]
    fn draw_box_diagonal_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x2570, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_box_diagonal(cp, &m, &mut c),
                "{cp:#06x} not a diagonal"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // Box-drawing arcs (Canvas::stroke_path + the z2d curve pipeline). The
    // fixture cell is 9×18, box_thickness 2, so center = (4, 9), r = 4.5. Each
    // arc is pinned on both axes: the vertical arm (up vs down) and the
    // horizontal side arm at y = center_y (left vs right).

    #[test]
    fn arc_2570_tr() {
        // ╰ : up + right. Top-center arm and right-center arm.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_arc(0x2570, &m, &mut c));
        assert!(inked(&c, 4, 2), "top-center arm");
        assert!(inked(&c, 7, 9), "right-center arm");
        assert!(!inked(&c, 1, 9), "left-center empty");
        assert!(!inked(&c, 1, 16), "bottom-left corner empty");
    }

    #[test]
    fn arc_256d_br() {
        // ╭ : down + right. Bottom-center arm and right-center arm.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_arc(0x256d, &m, &mut c));
        assert!(inked(&c, 4, 16), "bottom-center arm");
        assert!(inked(&c, 7, 9), "right-center arm");
        assert!(!inked(&c, 1, 9), "left-center empty");
        assert!(!inked(&c, 1, 2), "top-left corner empty");
    }

    #[test]
    fn arc_256e_bl() {
        // ╮ : down + left. Bottom-center arm and left-center arm.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_arc(0x256e, &m, &mut c));
        assert!(inked(&c, 4, 16), "bottom-center arm");
        assert!(inked(&c, 1, 9), "left-center arm");
        assert!(!inked(&c, 7, 9), "right-center empty");
        assert!(!inked(&c, 7, 2), "top-right corner empty");
    }

    #[test]
    fn arc_256f_tl() {
        // ╯ : up + left. Top-center arm and left-center arm.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_box_arc(0x256f, &m, &mut c));
        assert!(inked(&c, 4, 2), "top-center arm");
        assert!(inked(&c, 1, 9), "left-center arm");
        assert!(!inked(&c, 7, 9), "right-center empty");
        assert!(!inked(&c, 7, 16), "bottom-right corner empty");
    }

    // The curly underline (Canvas::stroke_path + the curve/round-cap pipeline).
    // The fixture 9×18 cell, underline_position 15, thickness 1, unpadded:
    // amplitude ≈ 2.86, top ≈ 14.14, bottom ≈ 17.0 — a wave peaking at the
    // center (rows ~13–14) with troughs at the ends (rows ~16–17).

    #[test]
    fn underline_curly_wave() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline_curly(&mut c, 9, 18, &m);
        // The peak: at the high row the center is inked but the ends are not.
        assert!(inked(&c, 4, 13), "center peak");
        assert!(!inked(&c, 0, 13), "left end above the trough");
        assert!(!inked(&c, 8, 13), "right end above the trough");
        // The troughs: at the low row the ends are inked but the center is not.
        assert!(inked(&c, 0, 16), "left trough");
        assert!(inked(&c, 8, 16), "right trough");
        assert!(!inked(&c, 4, 16), "center above the troughs");
    }

    #[test]
    fn underline_curly_shape() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline_curly(&mut c, 9, 18, &m);
        // The curl sits in the lower band: a row well above it is empty.
        for x in 0..9 {
            assert!(!inked(&c, x, 10), "upper cell empty at x={x}");
        }
    }

    #[test]
    fn canvas_closed_square_ring() {
        // A closed square stroked via Canvas::stroke_path (NonZero) inks its
        // border but leaves the center hole empty — the ring fill that
        // distinguishes a closed stroke from a filled shape. Square (2,2)-(8,8),
        // thickness 2 → an outer 1..9 / inner 3..7 ring.
        let mut c = Canvas::new(11, 11, 0, 0);
        let nodes = [
            raster::PathNode::MoveTo(raster::Point::new(2.0, 2.0)),
            raster::PathNode::LineTo(raster::Point::new(8.0, 2.0)),
            raster::PathNode::LineTo(raster::Point::new(8.0, 8.0)),
            raster::PathNode::LineTo(raster::Point::new(2.0, 8.0)),
            raster::PathNode::ClosePath,
        ];
        c.stroke_path(&nodes, 2.0, raster::CapMode::Butt);
        // The four border arms are inked.
        assert!(inked(&c, 5, 1), "top border");
        assert!(inked(&c, 5, 8), "bottom border");
        assert!(inked(&c, 1, 5), "left border");
        assert!(inked(&c, 8, 5), "right border");
        // The center hole is empty.
        assert!(!inked(&c, 5, 5), "center hole empty");
        assert!(!inked(&c, 4, 4), "inner corner empty");
    }

    // The filled corner triangles (Canvas::fill_path + the fill pipeline). The
    // fixture 9×18 cell: each triangle fills its corner and leaves the opposite
    // corner empty, confirmed against the render.

    #[test]
    fn corner_triangle_25e4_tl() {
        // ◤ : top-left filled.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle(0x25e4, &m, &mut c));
        assert!(inked(&c, 1, 1), "top-left filled");
        assert!(!inked(&c, 7, 16), "bottom-right empty");
    }

    #[test]
    fn corner_triangle_25e5_tr() {
        // ◥ : top-right filled.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle(0x25e5, &m, &mut c));
        assert!(inked(&c, 7, 1), "top-right filled");
        assert!(!inked(&c, 1, 16), "bottom-left empty");
    }

    #[test]
    fn corner_triangle_25e3_bl() {
        // ◣ : bottom-left filled.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle(0x25e3, &m, &mut c));
        assert!(inked(&c, 1, 16), "bottom-left filled");
        assert!(!inked(&c, 7, 1), "top-right empty");
    }

    #[test]
    fn corner_triangle_25e2_br() {
        // ◢ : bottom-right filled.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle(0x25e2, &m, &mut c));
        assert!(inked(&c, 7, 16), "bottom-right filled");
        assert!(!inked(&c, 1, 1), "top-left empty");
    }

    #[test]
    fn draw_corner_triangle_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x25e6, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_corner_triangle(cp, &m, &mut c),
                "{cp:#06x} not a corner triangle"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The outlined corner triangles (Canvas::inner_stroke_path). The fixture
    // 9×18 cell: each inks its three sides but leaves the interior hollow (a
    // point the *filled* version inks) and the opposite corner empty.

    #[test]
    fn corner_outline_25f8_tl() {
        // ◸ : top-left outline.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle_outline(0x25f8, &m, &mut c));
        assert!(inked(&c, 1, 1), "top edge inked");
        assert!(!inked(&c, 3, 3), "interior hollow");
        assert!(!inked(&c, 7, 16), "opposite corner empty");
    }

    #[test]
    fn corner_outline_25f9_tr() {
        // ◹ : top-right outline.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle_outline(0x25f9, &m, &mut c));
        assert!(inked(&c, 7, 1), "top edge inked");
        assert!(!inked(&c, 5, 3), "interior hollow");
        assert!(!inked(&c, 1, 16), "opposite corner empty");
    }

    #[test]
    fn corner_outline_25fa_bl() {
        // ◺ : bottom-left outline.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle_outline(0x25fa, &m, &mut c));
        assert!(inked(&c, 1, 16), "bottom edge inked");
        assert!(!inked(&c, 3, 14), "interior hollow");
        assert!(!inked(&c, 7, 1), "opposite corner empty");
    }

    #[test]
    fn corner_outline_25ff_br() {
        // ◿ : bottom-right outline.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_corner_triangle_outline(0x25ff, &m, &mut c));
        assert!(inked(&c, 7, 16), "bottom edge inked");
        assert!(!inked(&c, 5, 13), "interior hollow");
        assert!(!inked(&c, 1, 1), "opposite corner empty");
    }

    #[test]
    fn inner_stroke_hollow() {
        // The inner stroke of a closed square (2,2)-(8,8) clips to the interior:
        // the border is inked, the center hole is empty, AND the stroke does not
        // spill past the outer edge (col 1 stays empty) — unlike a plain closed
        // stroke, which would spill outward by the half-width.
        let nodes = [
            raster::PathNode::MoveTo(raster::Point::new(2.0, 2.0)),
            raster::PathNode::LineTo(raster::Point::new(8.0, 2.0)),
            raster::PathNode::LineTo(raster::Point::new(8.0, 8.0)),
            raster::PathNode::LineTo(raster::Point::new(2.0, 8.0)),
            raster::PathNode::ClosePath,
        ];
        let mut c = Canvas::new(11, 11, 0, 0);
        c.inner_stroke_path(&nodes, 2.0);
        assert!(inked(&c, 2, 4), "left border inked");
        assert!(inked(&c, 4, 2), "top border inked");
        assert!(!inked(&c, 5, 5), "center hole empty");
        assert!(!inked(&c, 1, 4), "no outward spill past the edge");
    }

    #[test]
    fn draw_corner_triangle_outline_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x25e2, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_corner_triangle_outline(cp, &m, &mut c),
                "{cp:#06x} not an outlined triangle"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The rect-based special sprites (underline / double / strikethrough /
    // overline). The fixture 9×18 cell: underline_position 15, thickness 1,
    // strikethrough_position 9, overline_position 0.

    fn row_inked(c: &Canvas, y: i32, width: i32) -> bool {
        (0..width).all(|x| inked(c, x, y))
    }

    #[test]
    fn underline_row() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 15, 9), "underline row 15 full width");
        for x in 0..9 {
            assert!(!inked(&c, x, 14), "row above clear");
            assert!(!inked(&c, x, 16), "row below clear");
        }
    }

    #[test]
    fn underline_double_gap() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline_double(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 14, 9), "upper line");
        assert!(row_inked(&c, 16, 9), "lower line");
        for x in 0..9 {
            assert!(!inked(&c, x, 15), "gap row clear");
        }
    }

    #[test]
    fn strikethrough_row() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_strikethrough(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 9, 9), "strikethrough row 9 full width");
    }

    #[test]
    fn overline_row() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_overline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 0, 9), "overline top row full width");
    }

    #[test]
    fn underline_clamp() {
        // A large underline_position clamps to the saturating limit (18 - 1 =
        // 17), drawing at the last row instead of off the bottom.
        let mut m = fixture_metrics();
        m.underline_position = 100;
        let mut c = cell_canvas();
        draw_underline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 17, 9), "clamped to row 17");
    }

    #[test]
    fn overline_negative() {
        // overline_position = -1 with vertical padding draws into the top
        // padding (cell y = -1), not clamped to 0.
        let mut m = fixture_metrics();
        m.overline_position = -1;
        let mut c = Canvas::new(9, 18, 0, 2);
        draw_overline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, -1, 9), "overline drawn above the cell");
        for x in 0..9 {
            assert!(!inked(&c, x, 0), "cell top row clear");
        }
    }

    // The cursor sprites (block / hollow / bar / underline).

    #[test]
    fn cursor_rect_full() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_cursor_rect(&mut c, 9, 18, &m);
        for y in 0..18 {
            for x in 0..9 {
                assert!(inked(&c, x, y), "block filled at ({x},{y})");
            }
        }
    }

    #[test]
    fn cursor_hollow_border() {
        // cursor_thickness 1: the border is inked, the interior (1..8 × 1..17)
        // is punched off.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_cursor_hollow_rect(&mut c, 9, 18, &m);
        // The four edges.
        assert!(inked(&c, 0, 0), "top-left");
        assert!(inked(&c, 8, 0), "top-right");
        assert!(inked(&c, 0, 17), "bottom-left");
        assert!(inked(&c, 8, 17), "bottom-right");
        assert!(row_inked(&c, 0, 9), "top edge full width");
        // The interior is hollow.
        assert!(!inked(&c, 4, 9), "center empty");
        assert!(!inked(&c, 1, 1), "just inside the corner empty");
    }

    #[test]
    fn cursor_bar_left() {
        // cursor_thickness 1 -> x = -1, width 1: the bar sits entirely over the
        // left cell edge (cell x = -1, in the left padding), the cell itself
        // empty.
        let m = fixture_metrics();
        let mut c = Canvas::new(9, 18, 1, 0);
        draw_cursor_bar(&mut c, 9, 18, &m);
        assert!(inked(&c, -1, 0), "bar at the left edge");
        assert!(inked(&c, -1, 9), "bar spans the height");
        for y in 0..18 {
            assert!(!inked(&c, 0, y), "cell col 0 empty");
            assert!(!inked(&c, 4, y), "cell interior empty");
        }
    }

    #[test]
    fn cursor_underline_row() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_cursor_underline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 15, 9), "underline bar at y=15");
    }

    #[test]
    fn cursor_underline_clamp() {
        // A large underline_position clamps to the saturating limit (18 - 1 =
        // 17) instead of drawing off the bottom.
        let mut m = fixture_metrics();
        m.underline_position = 100;
        let mut c = cell_canvas();
        draw_cursor_underline(&mut c, 9, 18, &m);
        assert!(row_inked(&c, 17, 9), "clamped to row 17");
    }

    // The dashed underline. The fixture 9×18 cell: width 9 -> dash_width 4,
    // dash_count 3; dashes at i=0 (cols 0-3) and i=2 (col 8), gap cols 4-7.

    #[test]
    fn underline_dashed_dashes() {
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline_dashed(&mut c, 9, 18, &m);
        // First dash (cols 0-3) inked.
        for x in 0..4 {
            assert!(inked(&c, x, 15), "first dash at x={x}");
        }
        // Gap (cols 4-7) empty.
        for x in 4..8 {
            assert!(!inked(&c, x, 15), "gap at x={x}");
        }
        // Third slot (col 8) inked.
        assert!(inked(&c, 8, 15), "third dash at col 8");
    }

    #[test]
    fn underline_dashed_clamp() {
        // A large underline_position clamps the dashes to the saturating limit
        // (18 - 1 = 17) instead of drawing off the bottom.
        let mut m = fixture_metrics();
        m.underline_position = 100;
        let mut c = cell_canvas();
        draw_underline_dashed(&mut c, 9, 18, &m);
        assert!(inked(&c, 0, 17), "first dash clamped to row 17");
    }

    // The arc primitive + the dotted underline.

    #[test]
    fn arc_fill_disc() {
        // Filling a radius-4 circle path inks a disc: the center is inked, a
        // point well outside the radius is not.
        let mut nodes = raster::arc(5.0, 5.0, 4.0, 0.0, std::f64::consts::TAU, 0.1);
        nodes.push(raster::PathNode::ClosePath);
        let mut c = Canvas::new(11, 11, 0, 0);
        c.fill_path(&nodes);
        assert!(inked(&c, 5, 5), "disc center inked");
        assert!(!inked(&c, 0, 0), "outside the radius empty");
    }

    #[test]
    fn underline_dotted_dots() {
        // Three dots centered at x = 1.5, 4.5, 7.5 in the underline band, with
        // gaps between them and the upper cell empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        draw_underline_dotted(&mut c, 9, 18, &m);
        // The three dot centers are inked (row 14, off the merged center row).
        assert!(inked(&c, 1, 14), "first dot");
        assert!(inked(&c, 4, 14), "second dot");
        assert!(inked(&c, 7, 14), "third dot");
        // Gaps between the dots.
        assert!(!inked(&c, 2, 14), "gap after the first dot");
        assert!(!inked(&c, 3, 14), "gap before the second dot");
        // The upper cell is empty.
        for x in 0..9 {
            assert!(!inked(&c, x, 10), "upper cell empty at x={x}");
        }
    }

    // The solid powerline triangles (Canvas::triangle + fill). The fixture 9×18
    // cell; each fills its region and leaves the opposite empty.

    #[test]
    fn powerline_e0b0_right() {
        // Right arrow: full left base, tapering to the right point.
        let mut c = cell_canvas();
        assert!(draw_powerline_triangle(0xe0b0, 9, 18, &mut c));
        assert!(inked(&c, 0, 9), "left base inked");
        assert!(!inked(&c, 8, 1), "top-right empty");
    }

    #[test]
    fn powerline_e0b2_left() {
        // Left arrow: full right base, tapering to the left point.
        let mut c = cell_canvas();
        assert!(draw_powerline_triangle(0xe0b2, 9, 18, &mut c));
        assert!(inked(&c, 8, 9), "right base inked");
        assert!(!inked(&c, 0, 1), "top-left empty");
    }

    #[test]
    fn powerline_half_cell_triangles() {
        for (cp, inked_pt, empty_pt) in [
            (0xe0bcu32, (1, 1), (7, 16)), // upper-left
            (0xe0be, (7, 1), (1, 16)),    // upper-right
            (0xe0b8, (1, 16), (7, 1)),    // lower-left
            (0xe0ba, (7, 16), (1, 1)),    // lower-right
        ] {
            let mut c = cell_canvas();
            assert!(draw_powerline_triangle(cp, 9, 18, &mut c));
            assert!(inked(&c, inked_pt.0, inked_pt.1), "{cp:#06x} corner inked");
            assert!(
                !inked(&c, empty_pt.0, empty_pt.1),
                "{cp:#06x} opposite empty"
            );
        }
    }

    #[test]
    fn powerline_uses_dimensions() {
        // E0BC (upper-left) at width/height 6 on a larger canvas fills only the
        // 6×6 triangle — a point inside it inked, a point past (6,6) empty —
        // confirming it uses the width/height parameters, not the cell metrics.
        let mut c = Canvas::new(11, 11, 0, 0);
        assert!(draw_powerline_triangle(0xe0bc, 6, 6, &mut c));
        assert!(inked(&c, 1, 1), "inside the 6x6 triangle");
        assert!(!inked(&c, 8, 8), "past the 6x6 region empty");
    }

    #[test]
    fn draw_powerline_triangle_excludes() {
        for cp in [0x2500u32, 0xe0b1, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_powerline_triangle(cp, 9, 18, &mut c),
                "{cp:#06x} not a solid powerline triangle"
            );
            let m = fixture_metrics();
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The outlined powerline chevrons (Canvas::stroke_path + flip_horizontal).

    #[test]
    fn powerline_e0b1_chevron() {
        // The `>` chevron: the tip is near the right, the interior is hollow,
        // and the vertical mid-left is empty (the arms start at the corners).
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_chevron(0xe0b1, 9, 18, &m, &mut c));
        assert!(inked(&c, 8, 9), "right tip inked");
        assert!(!inked(&c, 4, 9), "interior hollow");
        assert!(!inked(&c, 0, 9), "mid-left empty");
    }

    #[test]
    fn powerline_e0b3_flipped() {
        // The `<` chevron is E0B1 mirrored: the tip is now near the left.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_chevron(0xe0b3, 9, 18, &m, &mut c));
        assert!(inked(&c, 0, 9), "left tip inked");
        assert!(!inked(&c, 4, 9), "interior hollow");
        assert!(!inked(&c, 8, 9), "mid-right empty");
    }

    #[test]
    fn flip_horizontal_mirrors() {
        // On a padded canvas, an asymmetric mark at cell (0,0) mirrors to the
        // opposite cell column (8,0) after flip_horizontal.
        let mut c = Canvas::new(9, 18, 1, 0);
        c.pixel(0, 0, Color::ON);
        c.flip_horizontal();
        assert!(inked(&c, 8, 0), "mark mirrored to the right edge");
        assert!(!inked(&c, 0, 0), "original column cleared");
    }

    #[test]
    fn draw_powerline_chevron_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0xe0b0, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_powerline_chevron(cp, 9, 18, &m, &mut c),
                "{cp:#06x} not a chevron"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The rounded powerline separators (fill_path / inner_stroke_path + flip).

    #[test]
    fn powerline_e0b4_filled() {
        // The filled rounded-right separator: left side + interior filled, the
        // top-right corner (outside the curve) empty.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_rounded(0xe0b4, 9, 18, &m, &mut c));
        assert!(inked(&c, 0, 9), "left side filled");
        assert!(inked(&c, 4, 9), "interior filled");
        assert!(!inked(&c, 8, 0), "top-right corner empty");
    }

    #[test]
    fn powerline_e0b5_outlined() {
        // The outlined rounded separator strokes the rounded edge but leaves the
        // interior hollow.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_rounded(0xe0b5, 9, 18, &m, &mut c));
        assert!(inked(&c, 8, 9), "right curve stroked");
        assert!(!inked(&c, 4, 9), "interior hollow");
    }

    #[test]
    fn powerline_e0b6_flipped() {
        // E0B6 is E0B4 mirrored: the filled body on the right.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_rounded(0xe0b6, 9, 18, &m, &mut c));
        assert!(inked(&c, 8, 9), "filled body on the right");
        assert!(!inked(&c, 0, 0), "top-left corner empty");
    }

    #[test]
    fn powerline_e0b7_outlined_flipped() {
        // E0B7 is E0B5 mirrored: the rounded edge on the left, interior hollow.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_rounded(0xe0b7, 9, 18, &m, &mut c));
        assert!(inked(&c, 0, 9), "left curve stroked");
        assert!(!inked(&c, 4, 9), "interior hollow");
    }

    #[test]
    fn powerline_rounded_radius() {
        // width 8, height 6 -> r = min(8, 3) = 3: the rounded corner fits a
        // radius-3 arc, not the full width (a w-only radius would reach x=8).
        let m = fixture_metrics();
        let mut c = Canvas::new(9, 7, 0, 0);
        assert!(draw_powerline_rounded(0xe0b4, 8, 6, &m, &mut c));
        assert!(inked(&c, 1, 3), "inside the r=3 body");
        assert!(!inked(&c, 6, 3), "past the r=3 curve empty");
    }

    #[test]
    fn draw_powerline_rounded_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0xe0b0, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_powerline_rounded(cp, 9, 18, &m, &mut c),
                "{cp:#06x} not a rounded separator"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The powerline diagonal spacers (delegating to draw_box_diagonal).

    #[test]
    fn powerline_backslash_diagonals() {
        // E0B9/E0BF -> the ╲ diagonal (U+2572): center inked, top-right empty.
        let m = fixture_metrics();
        for cp in [0xe0b9u32, 0xe0bf] {
            let mut c = cell_canvas();
            assert!(draw_powerline_diagonal(cp, &m, &mut c));
            assert!(inked(&c, 4, 9), "{cp:#06x} center on the backslash");
            assert!(!inked(&c, 8, 1), "{cp:#06x} top-right empty");
        }
    }

    #[test]
    fn powerline_slash_diagonals() {
        // E0BB/E0BD -> the ╱ diagonal (U+2571): center inked, top-left empty.
        let m = fixture_metrics();
        for cp in [0xe0bbu32, 0xe0bd] {
            let mut c = cell_canvas();
            assert!(draw_powerline_diagonal(cp, &m, &mut c));
            assert!(inked(&c, 4, 9), "{cp:#06x} center on the slash");
            assert!(!inked(&c, 0, 1), "{cp:#06x} top-left empty");
        }
    }

    #[test]
    fn powerline_diagonal_matches_box() {
        // Each powerline diagonal's buffer equals the delegated box diagonal.
        let m = fixture_metrics();
        for (pcp, bcp) in [(0xe0b9u32, 0x2572u32), (0xe0bb, 0x2571)] {
            let mut pc = cell_canvas();
            assert!(draw_powerline_diagonal(pcp, &m, &mut pc));
            let mut bc = cell_canvas();
            assert!(draw_box_diagonal(bcp, &m, &mut bc));
            for y in 0..18 {
                for x in 0..9 {
                    assert_eq!(
                        pc.get(x, y),
                        bc.get(x, y),
                        "{pcp:#06x} vs {bcp:#06x} at ({x},{y})"
                    );
                }
            }
        }
    }

    #[test]
    fn draw_powerline_diagonal_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0xe0b0, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_powerline_diagonal(cp, &m, &mut c),
                "{cp:#06x} not a powerline diagonal"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    // The flame powerline separators (two filled quads + flip).

    #[test]
    fn powerline_e0d2_flame() {
        // The top and bottom pieces fill the left side, with a thin gap at the
        // center.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_flame(0xe0d2, 9, 18, &m, &mut c));
        assert!(inked(&c, 0, 2), "top piece");
        assert!(inked(&c, 0, 16), "bottom piece");
        assert!(!inked(&c, 0, 9), "center gap empty");
    }

    #[test]
    fn powerline_e0d4_flipped() {
        // E0D4 is E0D2 mirrored: the wide side on the right.
        let m = fixture_metrics();
        let mut c = cell_canvas();
        assert!(draw_powerline_flame(0xe0d4, 9, 18, &m, &mut c));
        assert!(inked(&c, 8, 2), "top piece on the right");
        assert!(inked(&c, 8, 16), "bottom piece on the right");
        assert!(!inked(&c, 8, 9), "center gap empty");
    }

    #[test]
    fn draw_powerline_flame_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0xe0b0, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(
                !draw_powerline_flame(cp, 9, 18, &m, &mut c),
                "{cp:#06x} not a flame"
            );
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }

    #[test]
    fn draw_box_arc_excludes() {
        let m = fixture_metrics();
        for cp in [0x2500u32, 0x2571, 'M' as u32] {
            let mut c = cell_canvas();
            assert!(!draw_box_arc(cp, &m, &mut c), "{cp:#06x} not an arc");
            assert!(all_alpha(&c, &m, 0), "{cp:#06x} drew ink");
        }
    }
}
