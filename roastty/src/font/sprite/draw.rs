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

/// Draw the box-drawing line glyph for `cp` into `canvas`, returning `true` if
/// `cp` is a (dispatched) line character. A representative subset of the
/// upstream `draw2500_257F` switch — the full inventory (dashes, arcs,
/// diagonals, and the rest of the line glyphs) is a later experiment.
pub(crate) fn draw_box_lines(cp: u32, metrics: &Metrics, canvas: &mut Canvas) -> bool {
    use LineStyle::{Double, Heavy, Light};
    let lines = match cp {
        // Straight lines
        0x2500 => Lines {
            left: Light,
            right: Light,
            ..Lines::default()
        },
        0x2501 => Lines {
            left: Heavy,
            right: Heavy,
            ..Lines::default()
        },
        0x2502 => Lines {
            up: Light,
            down: Light,
            ..Lines::default()
        },
        0x2503 => Lines {
            up: Heavy,
            down: Heavy,
            ..Lines::default()
        },
        // Light corners
        0x250C => Lines {
            down: Light,
            right: Light,
            ..Lines::default()
        },
        0x2510 => Lines {
            down: Light,
            left: Light,
            ..Lines::default()
        },
        0x2514 => Lines {
            up: Light,
            right: Light,
            ..Lines::default()
        },
        0x2518 => Lines {
            up: Light,
            left: Light,
            ..Lines::default()
        },
        // Crosses
        0x253C => Lines {
            up: Light,
            right: Light,
            down: Light,
            left: Light,
        },
        0x254B => Lines {
            up: Heavy,
            right: Heavy,
            down: Heavy,
            left: Heavy,
        },
        // Double lines
        0x2550 => Lines {
            left: Double,
            right: Double,
            ..Lines::default()
        },
        0x2551 => Lines {
            up: Double,
            down: Double,
            ..Lines::default()
        },
        0x256C => Lines {
            up: Double,
            right: Double,
            down: Double,
            left: Double,
        },
        _ => return false,
    };
    lines_char(metrics, canvas, lines);
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
}
