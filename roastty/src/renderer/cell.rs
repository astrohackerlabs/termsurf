#![allow(dead_code)]
// Cell codepoint classification is consumed by later renderer slices.

//! Renderer cell codepoint classification.
//!
//! Faithful port of the pure codepoint-classification predicates in upstream
//! `renderer/cell.zig`. The `Contents` cell-render-data builder,
//! `constraintWidth`, and `isSymbol` depend on shader/font/terminal types and a
//! generated Unicode table, and are ported separately.

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
}
