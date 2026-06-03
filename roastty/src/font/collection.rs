//! A collection of font faces, grouped by style.
//!
//! Faithful port of upstream `font/Collection.zig`. This slice provides the
//! foundational [`Index`] value type — the packed handle that names a font
//! within a collection. The `Collection` struct itself, `Entry`, deferred-face
//! loading, and discovery land in later experiments.

use crate::font::Style;

/// Bits used for the face index within an [`Index`]. `Style` is a 3-bit field,
/// leaving 13 bits of a `u16` for the index (up to 8192 fonts per style).
const IDX_BITS: u32 = 13;
/// Bits used for the style within an [`Index`].
const STYLE_BITS: u32 = 3;
/// Mask for the index portion (the low `IDX_BITS` of the unshifted value).
const IDX_MASK: u16 = (1 << IDX_BITS) - 1;

/// The special-case "fonts" that don't map to a real font face.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Special {
    /// Sprite drawing, rendered just-in-time via 2D graphics APIs.
    Sprite,
}

impl Special {
    /// Special indices start here; all `idx` values `>= START` are special.
    const START: u16 = IDX_MASK;

    /// The `idx` value encoding this special font.
    const fn idx(self) -> u16 {
        match self {
            // `sprite = start` upstream.
            Special::Sprite => Special::START,
        }
    }
}

/// Names a specific font within a [`Collection`](self).
///
/// Faithful port of upstream's `packed struct(u16) { style: Style, idx: u13 }`:
/// the `style` occupies the low 3 bits and the `idx` the high 13 bits of the
/// `u16` backing. The fields are private so the 13-bit `idx` invariant (which
/// upstream gets for free from its `u13` field) is enforced at construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Index {
    style: Style,
    idx: u16,
}

impl Index {
    /// Create an index for `idx`-th face of `style`. Panics if `idx` exceeds the
    /// 13-bit range (`> 8191`) — upstream's `u13` makes that unrepresentable, so
    /// this is the runtime analog (a hard `assert!`, live in release too).
    pub(crate) fn new(style: Style, idx: u16) -> Index {
        assert!(idx <= IDX_MASK, "font index {idx} exceeds the 13-bit range");
        Index { style, idx }
    }

    /// Create a special (non-face) index, e.g. for sprite glyphs.
    pub(crate) fn special(v: Special) -> Index {
        // Upstream: `{ .style = .regular, .idx = @intFromEnum(v) }`.
        Index {
            style: Style::Regular,
            idx: v.idx(),
        }
    }

    /// The style component.
    pub(crate) fn style(&self) -> Style {
        self.style
    }

    /// The face index component (`0..=8191`).
    pub(crate) fn idx(&self) -> u16 {
        self.idx
    }

    /// The `u16` backing value (`style` in the low 3 bits, `idx` in the high 13).
    pub(crate) fn int(&self) -> u16 {
        // No masking: `idx` is a valid 13-bit value by construction.
        (self.style as u16) | (self.idx << STYLE_BITS)
    }

    /// Decode an [`Index`] from its `u16` backing. Any `u16` yields a valid
    /// 13-bit `idx` (`v >> 3 <= 8191`).
    pub(crate) fn from_int(v: u16) -> Index {
        let style = match v & ((1 << STYLE_BITS) - 1) {
            0 => Style::Regular,
            1 => Style::Bold,
            2 => Style::Italic,
            // Only 0..=3 are valid styles; 4..=7 are unused by upstream and
            // can't occur for a round-tripped `Index`.
            _ => Style::BoldItalic,
        };
        Index {
            style,
            idx: v >> STYLE_BITS,
        }
    }

    /// The special kind if this is a special index, else `None`. Faithful to
    /// upstream's `if (idx < start) null else @enumFromInt(idx)`.
    pub(crate) fn special_kind(&self) -> Option<Special> {
        if self.idx < Special::START {
            None
        } else {
            // Only one special value exists; `idx == START` is `Sprite`.
            Some(Special::Sprite)
        }
    }
}

impl Default for Index {
    /// Upstream's field defaults: `{ .style = .regular, .idx = 0 }`.
    fn default() -> Index {
        Index::new(Style::Regular, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_bit_layout() {
        // style=Bold(1) in the low 3 bits, idx=5 in the high 13 bits: 1 | (5<<3).
        let i = Index::new(Style::Bold, 5);
        assert_eq!(i.int(), 1 | (5 << 3));
        assert_eq!(i.int(), 41);
        assert_eq!(Index::from_int(41), i);
    }

    #[test]
    fn index_round_trips() {
        for style in [
            Style::Regular,
            Style::Bold,
            Style::Italic,
            Style::BoldItalic,
        ] {
            for idx in [0u16, 1, 42, 8190] {
                let i = Index::new(style, idx);
                assert_eq!(Index::from_int(i.int()), i);
            }
        }
    }

    #[test]
    fn index_default_is_zero() {
        assert_eq!(Index::default().int(), 0);
        assert_eq!(Index::default().style(), Style::Regular);
        assert_eq!(Index::default().idx(), 0);
    }

    #[test]
    fn idx_bits_is_13() {
        assert_eq!(IDX_BITS, 13);
        // The maximum non-special index round-trips.
        let i = Index::new(Style::Italic, 8190);
        assert_eq!(Index::from_int(i.int()), i);
    }

    #[test]
    fn special_index() {
        let sprite = Index::special(Special::Sprite);
        assert_eq!(sprite.idx(), 8191);
        assert_eq!(sprite.special_kind(), Some(Special::Sprite));

        // Normal indices are not special.
        for idx in [0u16, 1, 8190] {
            assert_eq!(Index::new(Style::Regular, idx).special_kind(), None);
        }
    }

    #[test]
    fn from_int_idx_is_valid() {
        // Any u16 decodes to a valid 13-bit idx.
        assert_eq!(Index::from_int(u16::MAX).idx(), 8191);
    }

    #[test]
    #[should_panic]
    fn new_rejects_out_of_range_idx() {
        let _ = Index::new(Style::Regular, 8192);
    }
}
