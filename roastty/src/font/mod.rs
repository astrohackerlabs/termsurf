#![allow(dead_code)]
// The font subsystem is consumed by later font and renderer slices.

//! Font subsystem.
//!
//! Faithful port of upstream `font/`. This slice establishes the module, the
//! `Glyph` value type, the `Metrics` derivation, the texture `Atlas`, and the
//! `sprite` geometric primitives; rasterization, faces, and shaping land in
//! later experiments.

pub(crate) mod atlas;
pub(crate) mod glyph;
pub(crate) mod metrics;
pub(crate) mod sprite;

/// The style (weight/slant) of a font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Style {
    Regular = 0,
    Bold = 1,
    Italic = 2,
    BoldItalic = 3,
}

/// The presentation for an emoji.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum Presentation {
    /// Text presentation (U+FE0E).
    Text = 0,
    /// Emoji presentation (U+FE0F).
    Emoji = 1,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn style_discriminants() {
        assert_eq!(Style::Regular as u8, 0);
        assert_eq!(Style::Bold as u8, 1);
        assert_eq!(Style::Italic as u8, 2);
        assert_eq!(Style::BoldItalic as u8, 3);
    }

    #[test]
    fn presentation_discriminants() {
        assert_eq!(Presentation::Text as u8, 0);
        assert_eq!(Presentation::Emoji as u8, 1);
    }
}
