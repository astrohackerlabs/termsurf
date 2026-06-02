//! A single loaded glyph for a face.
//!
//! Faithful port of upstream `font/Glyph.zig`.

/// A single rasterized glyph: its pixel size, bearings, and position in the
/// glyph atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Glyph {
    /// Width of the glyph in pixels.
    pub width: u32,

    /// Height of the glyph in pixels.
    pub height: u32,

    /// Left bearing.
    pub offset_x: i32,

    /// Top bearing.
    pub offset_y: i32,

    /// X coordinate in the atlas of the top-left corner. These are raw pixel
    /// positions and must be normalized to `0..1` before use in shaders.
    pub atlas_x: u32,

    /// Y coordinate in the atlas of the top-left corner.
    pub atlas_y: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn glyph_holds_fields() {
        let g = Glyph {
            width: 7,
            height: 14,
            offset_x: 1,
            offset_y: 11,
            atlas_x: 32,
            atlas_y: 64,
        };
        assert_eq!(g.width, 7);
        assert_eq!(g.height, 14);
        assert_eq!(g.offset_x, 1);
        assert_eq!(g.offset_y, 11);
        assert_eq!(g.atlas_x, 32);
        assert_eq!(g.atlas_y, 64);
    }

    #[test]
    fn glyph_offsets_are_signed() {
        // Bearings can be negative (a glyph sitting left of / above its origin).
        let g = Glyph {
            width: 0,
            height: 0,
            offset_x: -3,
            offset_y: -5,
            atlas_x: 0,
            atlas_y: 0,
        };
        assert_eq!(g.offset_x, -3);
        assert_eq!(g.offset_y, -5);
    }
}
