#![allow(dead_code)]
// Shader input value types are consumed by later renderer slices.

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct ImageVertex {
    pub(crate) grid_pos: [f32; 2],
    pub(crate) cell_offset: [f32; 2],
    pub(crate) source_rect: [f32; 4],
    pub(crate) dest_size: [f32; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(C, align(8))]
pub(crate) struct CellTextVertex {
    pub(crate) glyph_pos: [u32; 2],
    pub(crate) glyph_size: [u32; 2],
    pub(crate) bearings: [i16; 2],
    pub(crate) grid_pos: [u16; 2],
    pub(crate) color: [u8; 4],
    pub(crate) atlas: CellTextAtlas,
    pub(crate) flags: CellTextFlags,
    pub(crate) _padding: [u8; 2],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CellTextAtlas {
    Grayscale = 0,
    Color = 1,
}

impl CellTextAtlas {
    pub(crate) fn raw(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct CellTextFlags(u8);

impl CellTextFlags {
    pub(crate) fn new(no_min_contrast: bool, is_cursor_glyph: bool) -> Self {
        Self((no_min_contrast as u8) | ((is_cursor_glyph as u8) << 1))
    }

    pub(crate) fn raw(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct CellBg(pub(crate) [u8; 4]);

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(C)]
pub(crate) struct BgImageVertex {
    pub(crate) opacity: f32,
    pub(crate) info: BgImageInfo,
    pub(crate) _padding: [u8; 3],
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(transparent)]
pub(crate) struct BgImageInfo(u8);

impl BgImageInfo {
    pub(crate) fn new(position: BgImagePosition, fit: BgImageFit, repeat: bool) -> Self {
        Self(position.raw() | (fit.raw() << 4) | ((repeat as u8) << 6))
    }

    pub(crate) fn raw(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum BgImagePosition {
    TopLeft = 0,
    TopCenter = 1,
    TopRight = 2,
    MiddleLeft = 3,
    MiddleCenter = 4,
    MiddleRight = 5,
    BottomLeft = 6,
    BottomCenter = 7,
    BottomRight = 8,
}

impl BgImagePosition {
    pub(crate) fn raw(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum BgImageFit {
    Contain = 0,
    Cover = 1,
    Stretch = 2,
    None = 3,
}

impl BgImageFit {
    pub(crate) fn raw(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PrimitiveType {
    TriangleStrip,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImageDrawCall {
    pub(crate) vertex: ImageVertex,
    pub(crate) primitive: PrimitiveType,
    pub(crate) vertex_count: u32,
    pub(crate) instance_count: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_vertex_layout_matches_upstream_shader_parameter() {
        assert_eq!(std::mem::size_of::<ImageVertex>(), 40);
        assert_eq!(std::mem::align_of::<ImageVertex>(), 4);
    }

    #[test]
    fn cell_text_vertex_layout_matches_upstream_shader_parameter() {
        assert_eq!(std::mem::size_of::<CellTextVertex>(), 32);
        assert_eq!(std::mem::align_of::<CellTextVertex>(), 8);
        assert_eq!(std::mem::offset_of!(CellTextVertex, glyph_pos), 0);
        assert_eq!(std::mem::offset_of!(CellTextVertex, glyph_size), 8);
        assert_eq!(std::mem::offset_of!(CellTextVertex, bearings), 16);
        assert_eq!(std::mem::offset_of!(CellTextVertex, grid_pos), 20);
        assert_eq!(std::mem::offset_of!(CellTextVertex, color), 24);
        assert_eq!(std::mem::offset_of!(CellTextVertex, atlas), 28);
        assert_eq!(std::mem::offset_of!(CellTextVertex, flags), 29);
        assert_eq!(std::mem::offset_of!(CellTextVertex, _padding), 30);
    }

    #[test]
    fn cell_bg_layout_matches_upstream_shader_parameter() {
        assert_eq!(std::mem::size_of::<CellBg>(), 4);
        assert_eq!(std::mem::align_of::<CellBg>(), 1);
    }

    #[test]
    fn bg_image_vertex_layout_matches_upstream_shader_parameter() {
        assert_eq!(std::mem::size_of::<BgImageVertex>(), 8);
        assert_eq!(std::mem::align_of::<BgImageVertex>(), 4);
        assert_eq!(std::mem::offset_of!(BgImageVertex, opacity), 0);
        assert_eq!(std::mem::offset_of!(BgImageVertex, info), 4);
        assert_eq!(std::mem::offset_of!(BgImageVertex, _padding), 5);
    }

    #[test]
    fn cell_text_atlas_raw_values_match_upstream() {
        assert_eq!(CellTextAtlas::Grayscale.raw(), 0);
        assert_eq!(CellTextAtlas::Color.raw(), 1);
    }

    #[test]
    fn cell_text_flags_pack_low_bits() {
        assert_eq!(CellTextFlags::new(false, false).raw(), 0b0000_0000);
        assert_eq!(CellTextFlags::new(true, false).raw(), 0b0000_0001);
        assert_eq!(CellTextFlags::new(false, true).raw(), 0b0000_0010);
        assert_eq!(CellTextFlags::new(true, true).raw(), 0b0000_0011);
    }

    #[test]
    fn bg_image_position_raw_values_match_upstream() {
        assert_eq!(BgImagePosition::TopLeft.raw(), 0);
        assert_eq!(BgImagePosition::TopCenter.raw(), 1);
        assert_eq!(BgImagePosition::TopRight.raw(), 2);
        assert_eq!(BgImagePosition::MiddleLeft.raw(), 3);
        assert_eq!(BgImagePosition::MiddleCenter.raw(), 4);
        assert_eq!(BgImagePosition::MiddleRight.raw(), 5);
        assert_eq!(BgImagePosition::BottomLeft.raw(), 6);
        assert_eq!(BgImagePosition::BottomCenter.raw(), 7);
        assert_eq!(BgImagePosition::BottomRight.raw(), 8);
    }

    #[test]
    fn bg_image_fit_raw_values_match_upstream() {
        assert_eq!(BgImageFit::Contain.raw(), 0);
        assert_eq!(BgImageFit::Cover.raw(), 1);
        assert_eq!(BgImageFit::Stretch.raw(), 2);
        assert_eq!(BgImageFit::None.raw(), 3);
    }

    #[test]
    fn bg_image_info_packs_position_fit_and_repeat() {
        assert_eq!(
            BgImageInfo::new(BgImagePosition::TopLeft, BgImageFit::Contain, false).raw(),
            0b0000_0000
        );
        assert_eq!(
            BgImageInfo::new(BgImagePosition::BottomRight, BgImageFit::None, true).raw(),
            0b0111_1000
        );
        assert_eq!(
            BgImageInfo::new(BgImagePosition::MiddleCenter, BgImageFit::Cover, true).raw(),
            0b0101_0100
        );
    }
}
