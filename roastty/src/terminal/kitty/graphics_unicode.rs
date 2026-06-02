//! Kitty graphics Unicode placeholder decoding.

use super::super::style::{self, Color};
use super::super::{page::Cell, page_list::Pin};

pub(in crate::terminal) const PLACEHOLDER: u32 = 0x10eeee;

const DIACRITICS: &[u32] = &[
    0x0305, 0x030d, 0x030e, 0x0310, 0x0312, 0x033d, 0x033e, 0x033f, 0x0346, 0x034a, 0x034b, 0x034c,
    0x0350, 0x0351, 0x0352, 0x0357, 0x035b, 0x0363, 0x0364, 0x0365, 0x0366, 0x0367, 0x0368, 0x0369,
    0x036a, 0x036b, 0x036c, 0x036d, 0x036e, 0x036f, 0x0483, 0x0484, 0x0485, 0x0486, 0x0487, 0x0592,
    0x0593, 0x0594, 0x0595, 0x0597, 0x0598, 0x0599, 0x059c, 0x059d, 0x059e, 0x059f, 0x05a0, 0x05a1,
    0x05a8, 0x05a9, 0x05ab, 0x05ac, 0x05af, 0x05c4, 0x0610, 0x0611, 0x0612, 0x0613, 0x0614, 0x0615,
    0x0616, 0x0617, 0x0657, 0x0658, 0x0659, 0x065a, 0x065b, 0x065d, 0x065e, 0x06d6, 0x06d7, 0x06d8,
    0x06d9, 0x06da, 0x06db, 0x06dc, 0x06df, 0x06e0, 0x06e1, 0x06e2, 0x06e4, 0x06e7, 0x06e8, 0x06eb,
    0x06ec, 0x0730, 0x0732, 0x0733, 0x0735, 0x0736, 0x073a, 0x073d, 0x073f, 0x0740, 0x0741, 0x0743,
    0x0745, 0x0747, 0x0749, 0x074a, 0x07eb, 0x07ec, 0x07ed, 0x07ee, 0x07ef, 0x07f0, 0x07f1, 0x07f3,
    0x0816, 0x0817, 0x0818, 0x0819, 0x081b, 0x081c, 0x081d, 0x081e, 0x081f, 0x0820, 0x0821, 0x0822,
    0x0823, 0x0825, 0x0826, 0x0827, 0x0829, 0x082a, 0x082b, 0x082c, 0x082d, 0x0951, 0x0953, 0x0954,
    0x0f82, 0x0f83, 0x0f86, 0x0f87, 0x135d, 0x135e, 0x135f, 0x17dd, 0x193a, 0x1a17, 0x1a75, 0x1a76,
    0x1a77, 0x1a78, 0x1a79, 0x1a7a, 0x1a7b, 0x1a7c, 0x1b6b, 0x1b6d, 0x1b6e, 0x1b6f, 0x1b70, 0x1b71,
    0x1b72, 0x1b73, 0x1cd0, 0x1cd1, 0x1cd2, 0x1cda, 0x1cdb, 0x1ce0, 0x1dc0, 0x1dc1, 0x1dc3, 0x1dc4,
    0x1dc5, 0x1dc6, 0x1dc7, 0x1dc8, 0x1dc9, 0x1dcb, 0x1dcc, 0x1dd1, 0x1dd2, 0x1dd3, 0x1dd4, 0x1dd5,
    0x1dd6, 0x1dd7, 0x1dd8, 0x1dd9, 0x1dda, 0x1ddb, 0x1ddc, 0x1ddd, 0x1dde, 0x1ddf, 0x1de0, 0x1de1,
    0x1de2, 0x1de3, 0x1de4, 0x1de5, 0x1de6, 0x1dfe, 0x20d0, 0x20d1, 0x20d4, 0x20d5, 0x20d6, 0x20d7,
    0x20db, 0x20dc, 0x20e1, 0x20e7, 0x20e9, 0x20f0, 0x2cef, 0x2cf0, 0x2cf1, 0x2de0, 0x2de1, 0x2de2,
    0x2de3, 0x2de4, 0x2de5, 0x2de6, 0x2de7, 0x2de8, 0x2de9, 0x2dea, 0x2deb, 0x2dec, 0x2ded, 0x2dee,
    0x2def, 0x2df0, 0x2df1, 0x2df2, 0x2df3, 0x2df4, 0x2df5, 0x2df6, 0x2df7, 0x2df8, 0x2df9, 0x2dfa,
    0x2dfb, 0x2dfc, 0x2dfd, 0x2dfe, 0x2dff, 0xa66f, 0xa67c, 0xa67d, 0xa6f0, 0xa6f1, 0xa8e0, 0xa8e1,
    0xa8e2, 0xa8e3, 0xa8e4, 0xa8e5, 0xa8e6, 0xa8e7, 0xa8e8, 0xa8e9, 0xa8ea, 0xa8eb, 0xa8ec, 0xa8ed,
    0xa8ee, 0xa8ef, 0xa8f0, 0xa8f1, 0xaab0, 0xaab2, 0xaab3, 0xaab7, 0xaab8, 0xaabe, 0xaabf, 0xaac1,
    0xfe20, 0xfe21, 0xfe22, 0xfe23, 0xfe24, 0xfe25, 0xfe26, 0x10a0f, 0x10a38, 0x1d185, 0x1d186,
    0x1d187, 0x1d188, 0x1d189, 0x1d1aa, 0x1d1ab, 0x1d1ac, 0x1d1ad, 0x1d242, 0x1d243, 0x1d244,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) struct VirtualPlacement {
    pub(in crate::terminal) pin: Pin,
    pub(in crate::terminal) image_id: u32,
    pub(in crate::terminal) placement_id: u32,
    pub(in crate::terminal) col: u32,
    pub(in crate::terminal) row: u32,
    pub(in crate::terminal) width: u32,
    pub(in crate::terminal) height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::terminal) struct IncompletePlacement {
    pin: Pin,
    image_id_low: u32,
    image_id_high: Option<u8>,
    placement_id: Option<u32>,
    row: Option<u32>,
    col: Option<u32>,
    width: u32,
}

impl IncompletePlacement {
    pub(in crate::terminal) fn init(
        pin: Pin,
        cell: Cell,
        style: style::Style,
        graphemes: &[u32],
    ) -> Self {
        debug_assert_eq!(cell.codepoint(), PLACEHOLDER);

        let mut result = Self {
            pin,
            image_id_low: color_to_id(style.fg_color),
            image_id_high: None,
            placement_id: match color_to_id(style.underline_color) {
                0 => None,
                id => Some(id),
            },
            row: None,
            col: None,
            width: 1,
        };

        if let Some(cp) = graphemes.first() {
            result.row = get_index(*cp);
        }
        if let Some(cp) = graphemes.get(1) {
            result.col = get_index(*cp);
        }
        if let Some(cp) = graphemes.get(2) {
            result.image_id_high = get_index(*cp).and_then(|value| value.try_into().ok());
        }

        result
    }

    pub(in crate::terminal) fn prepare_first(mut self) -> Self {
        self.row.get_or_insert(0);
        self.col.get_or_insert(0);
        self
    }

    pub(in crate::terminal) fn append(&mut self, other: &Self) -> bool {
        if !self.can_append(other) {
            return false;
        }
        self.width += 1;
        true
    }

    pub(in crate::terminal) fn complete(self) -> VirtualPlacement {
        let image_id = self.image_id_low | (u32::from(self.image_id_high.unwrap_or(0)) << 24);
        VirtualPlacement {
            pin: self.pin,
            image_id,
            placement_id: self.placement_id.unwrap_or(0),
            col: self.col.unwrap_or(0),
            row: self.row.unwrap_or(0),
            width: self.width,
            height: 1,
        }
    }

    fn can_append(self, other: &Self) -> bool {
        self.image_id_low == other.image_id_low
            && self.placement_id == other.placement_id
            && other.row.is_none_or(|row| Some(row) == self.row)
            && other
                .col
                .is_none_or(|col| Some(col) == self.col.map(|value| value + self.width))
            && other
                .image_id_high
                .is_none_or(|high| Some(high) == self.image_id_high)
    }
}

pub(in crate::terminal) fn get_index(codepoint: u32) -> Option<u32> {
    DIACRITICS
        .binary_search(&codepoint)
        .ok()
        .and_then(|index| index.try_into().ok())
}

fn color_to_id(color: Color) -> u32 {
    match color {
        Color::None => 0,
        Color::Palette(index) => u32::from(index),
        Color::Rgb(rgb) => (u32::from(rgb.r) << 16) | (u32::from(rgb.g) << 8) | u32::from(rgb.b),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::color::Rgb;
    use crate::terminal::page_list::Pin;

    fn test_pin() -> Pin {
        Pin::test_invalid_for_tests()
    }

    fn decode(style: style::Style, graphemes: &[u32]) -> VirtualPlacement {
        let cell = Cell::init(PLACEHOLDER);
        IncompletePlacement::init(test_pin(), cell, style, graphemes)
            .prepare_first()
            .complete()
    }

    #[test]
    fn kitty_graphics_unicode_diacritics_are_sorted() {
        assert_eq!(DIACRITICS.len(), 297);
        assert!(DIACRITICS.windows(2).all(|window| window[0] < window[1]));
    }

    #[test]
    fn kitty_graphics_unicode_diacritic_indexes() {
        assert_eq!(get_index(0x0483), Some(30));
        assert_eq!(get_index(0x06d7), Some(70));
        assert_eq!(get_index(0xaab0), Some(268));
        assert_eq!(get_index(0x1d242), Some(294));
        assert_eq!(get_index(0x0300), None);
    }

    #[test]
    fn kitty_graphics_unicode_decodes_row_col_defaults() {
        let placement = decode(style::Style::default(), &[0x0305, 0x0305]);
        assert_eq!(placement.image_id, 0);
        assert_eq!(placement.placement_id, 0);
        assert_eq!(placement.row, 0);
        assert_eq!(placement.col, 0);
        assert_eq!(placement.width, 1);
        assert_eq!(placement.height, 1);
    }

    #[test]
    fn kitty_graphics_unicode_decodes_ids_from_colors_and_high_bits() {
        let style = style::Style {
            fg_color: Color::Rgb(Rgb::new(1, 2, 3)),
            underline_color: Color::Palette(21),
            ..style::Style::default()
        };
        let placement = decode(style, &[0x0305, 0x0305, 0x030e]);
        assert_eq!(placement.image_id, 0x0201_0203);
        assert_eq!(placement.placement_id, 21);
    }

    #[test]
    fn kitty_graphics_unicode_invalid_diacritics_are_ignored() {
        let placement = decode(style::Style::default(), &[0x0300, 0x0301, 0x0302]);
        assert_eq!(placement.row, 0);
        assert_eq!(placement.col, 0);
        assert_eq!(placement.image_id, 0);
    }

    #[test]
    fn kitty_graphics_unicode_continuation_rules() {
        let cell = Cell::init(PLACEHOLDER);
        let style = style::Style::default();
        let mut first =
            IncompletePlacement::init(test_pin(), cell, style, &[0x0305, 0x0305]).prepare_first();
        let second = IncompletePlacement::init(test_pin(), cell, style, &[0x0305, 0x030d]);
        let third = IncompletePlacement::init(test_pin(), cell, style, &[0x0305]);
        let jump = IncompletePlacement::init(test_pin(), cell, style, &[0x0305, 0x0312]);

        assert!(first.append(&second));
        assert!(first.append(&third));
        assert_eq!(first.width, 3);
        assert!(!first.append(&jump));
    }
}
