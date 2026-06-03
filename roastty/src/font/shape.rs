//! Text shaping — the value types.
//!
//! Faithful port of upstream `font/shape.zig`, the shaper's output contract. The
//! shaper turns a run of terminal cells into positioned glyphs ([`Cell`]s); this
//! module defines that output ([`Cell`]) and the shaping [`Options`]. The
//! run iterator, the shaping hook, and the CoreText shaping pipeline
//! (`CFAttributedString` → `CTLine` → `CTRun` → `Cell`) are later sub-areas.

/// A single shaped glyph to render, output by the shaper. Only cells with a
/// glyph to render are present. Faithful port of upstream `shape.Cell`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Cell {
    /// The X position of this cell relative to the run's offset. Runs are always
    /// within a single row, so the caller reconstructs the full position from the
    /// run offset, the row's Y, and this X.
    pub x: u16,
    /// An additional offset to apply when rendering.
    pub x_offset: i16,
    /// An additional offset to apply when rendering.
    pub y_offset: i16,
    /// The glyph index for this cell (valid for the run's font).
    pub glyph_index: u32,
}

/// One input codepoint paired with its cluster (the source cell), the shaper's
/// input contract. Mirrors upstream's `RunState.codepoints` entries, fed by
/// `addCodepoint(cp, cluster)`: the caller (the run iterator) supplies the
/// cluster, grouping a grapheme's codepoints into one terminal cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Codepoint {
    /// The Unicode scalar to shape.
    pub codepoint: u32,
    /// The terminal cell this codepoint belongs to. Drives the shaped `Cell.x`.
    pub cluster: u32,
}

/// Options controlling shaping. Faithful port of upstream `shape.Options`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Options {
    /// Font features to apply when shaping (e.g. `"liga"`, `"calt"`). Applied
    /// globally for now (upstream notes this may move to the face later).
    pub features: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_defaults() {
        let c = Cell::default();
        assert_eq!(c.x, 0);
        assert_eq!(c.x_offset, 0);
        assert_eq!(c.y_offset, 0);
        assert_eq!(c.glyph_index, 0);
    }

    #[test]
    fn cell_construction() {
        // The set fields are kept and the offsets zero-default.
        let c = Cell {
            x: 3,
            glyph_index: 42,
            ..Default::default()
        };
        assert_eq!(c.x, 3);
        assert_eq!(c.glyph_index, 42);
        assert_eq!(c.x_offset, 0);
        assert_eq!(c.y_offset, 0);

        // The offsets are signed and hold negatives.
        let c = Cell {
            x: 1,
            x_offset: -2,
            y_offset: -5,
            glyph_index: 7,
        };
        assert_eq!(c.x_offset, -2);
        assert_eq!(c.y_offset, -5);
    }

    #[test]
    fn cell_eq_copy() {
        let a = Cell {
            x: 2,
            x_offset: 1,
            y_offset: -1,
            glyph_index: 9,
        };
        let b = a; // Copy
        assert_eq!(a, b);
        let mut c = a;
        c.glyph_index = 10;
        assert_ne!(a, c, "a differing glyph index is unequal");
    }

    #[test]
    fn options_default_empty() {
        assert!(Options::default().features.is_empty());
        let o = Options {
            features: vec!["liga".to_string(), "calt".to_string()],
        };
        assert_eq!(o.features, vec!["liga".to_string(), "calt".to_string()]);
    }
}
