//! Procedural box-drawing glyphs.
//!
//! Faithful port of the box-drawing **line** primitive of upstream
//! `font/sprite/draw/box.zig` (`linesChar`), plus the `Thickness` helper from
//! `font/sprite/draw/common.zig` and the per-direction line style. `linesChar`
//! is the foundation the line glyphs (`U+2500`–`U+254B` straight lines, corners,
//! T-junctions, crosses) and the double-line glyphs build on. The remaining
//! box-drawing primitives (dashes, arcs, diagonals), the full `draw2500_257F`
//! dispatch, the sprite `hasCodepoint` inventory, and the other sprite
//! categories (block, braille, powerline, legacy) are later experiments.

use crate::font::metrics::Metrics;
use crate::font::sprite::canvas::{Canvas, Color};

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
}
