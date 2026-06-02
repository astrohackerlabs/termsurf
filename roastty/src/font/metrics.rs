//! Font metrics: recommended cell dimensions and decoration positions.
//!
//! Faithful port of the `Metrics` value type from upstream `font/Metrics.zig`.
//! The `FaceMetrics` input, the `Minimums` table, the `calc` derivation, and
//! constraint application are ported in later slices.

/// Recommended cell dimensions and decoration positions/thicknesses for a
/// monospace grid using a given font.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Metrics {
    /// Recommended cell width for a monospace grid using this font.
    pub cell_width: u32,
    /// Recommended cell height for a monospace grid using this font.
    pub cell_height: u32,

    /// Distance in pixels from the bottom of the cell to the text baseline.
    pub cell_baseline: u32,

    /// Distance in pixels from the top of the cell to the top of the underline.
    pub underline_position: u32,
    /// Thickness in pixels of the underline.
    pub underline_thickness: u32,

    /// Distance in pixels from the top of the cell to the top of the
    /// strikethrough.
    pub strikethrough_position: u32,
    /// Thickness in pixels of the strikethrough.
    pub strikethrough_thickness: u32,

    /// Distance in pixels from the top of the cell to the top of the overline.
    /// Can be negative to adjust the position above the top of the cell.
    pub overline_position: i32,
    /// Thickness in pixels of the overline.
    pub overline_thickness: u32,

    /// Thickness in pixels of box drawing characters.
    pub box_thickness: u32,

    /// The thickness in pixels of the cursor sprite. This is not determined by
    /// fonts but by user configuration; the deferred `calc`/config path applies
    /// the upstream default of `1`.
    pub cursor_thickness: u32,

    /// The height in pixels of the cursor sprite.
    pub cursor_height: u32,

    /// The constraint height for nerd fonts icons.
    pub icon_height: f64,

    /// The constraint height for nerd fonts icons limited to a single cell
    /// width.
    pub icon_height_single: f64,

    /// The unrounded face width, used in scaling calculations.
    pub face_width: f64,

    /// The unrounded face height, used in scaling calculations.
    pub face_height: f64,

    /// The offset from the bottom of the cell to the bottom of the face's
    /// bounding box, based on the rounded and potentially adjusted cell height.
    pub face_y: f64,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Metrics {
        Metrics {
            cell_width: 8,
            cell_height: 16,
            cell_baseline: 3,
            underline_position: 13,
            underline_thickness: 1,
            strikethrough_position: 8,
            strikethrough_thickness: 1,
            overline_position: 0,
            overline_thickness: 1,
            box_thickness: 2,
            cursor_thickness: 1,
            cursor_height: 16,
            icon_height: 12.5,
            icon_height_single: 11.0,
            face_width: 7.75,
            face_height: 15.5,
            face_y: 1.25,
        }
    }

    #[test]
    fn metrics_holds_fields() {
        let m = sample();
        assert_eq!(m.cell_width, 8);
        assert_eq!(m.cell_height, 16);
        assert_eq!(m.cell_baseline, 3);
        assert_eq!(m.underline_position, 13);
        assert_eq!(m.underline_thickness, 1);
        assert_eq!(m.strikethrough_position, 8);
        assert_eq!(m.strikethrough_thickness, 1);
        assert_eq!(m.overline_position, 0);
        assert_eq!(m.overline_thickness, 1);
        assert_eq!(m.box_thickness, 2);
        assert_eq!(m.cursor_thickness, 1);
        assert_eq!(m.cursor_height, 16);
        assert_eq!(m.icon_height, 12.5);
        assert_eq!(m.icon_height_single, 11.0);
        assert_eq!(m.face_width, 7.75);
        assert_eq!(m.face_height, 15.5);
        assert_eq!(m.face_y, 1.25);
    }

    #[test]
    fn metrics_overline_position_is_signed() {
        let mut m = sample();
        m.overline_position = -2;
        assert_eq!(m.overline_position, -2);
    }

    #[test]
    fn metrics_face_fields_are_f64() {
        let mut m = sample();
        m.face_width = 7.3;
        m.icon_height = 0.5;
        assert_eq!(m.face_width, 7.3);
        assert_eq!(m.icon_height, 0.5);
    }
}
