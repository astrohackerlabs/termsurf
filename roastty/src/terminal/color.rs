#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Rgb {
    pub(super) r: u8,
    pub(super) g: u8,
    pub(super) b: u8,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct CRgb {
    r: u8,
    g: u8,
    b: u8,
}

pub(super) type Palette = [Rgb; 256];

impl Rgb {
    pub(super) const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub(super) const fn from_c(c: CRgb) -> Self {
        Self {
            r: c.r,
            g: c.g,
            b: c.b,
        }
    }

    pub(super) const fn cval(self) -> CRgb {
        CRgb {
            r: self.r,
            g: self.g,
            b: self.b,
        }
    }
}

pub(super) const DEFAULT_PALETTE: Palette = default_palette();

const fn default_palette() -> Palette {
    let mut result = [Rgb::new(0, 0, 0); 256];
    let mut i = 0;
    while i < 16 {
        result[i] = default_named(i as u8);
        i += 1;
    }

    let mut r = 0;
    while r < 6 {
        let mut g = 0;
        while g < 6 {
            let mut b = 0;
            while b < 6 {
                result[i] = Rgb::new(cube_value(r), cube_value(g), cube_value(b));
                i += 1;
                b += 1;
            }
            g += 1;
        }
        r += 1;
    }

    i = 232;
    while i < 256 {
        let value = ((i - 232) * 10 + 8) as u8;
        result[i] = Rgb::new(value, value, value);
        i += 1;
    }

    result
}

const fn cube_value(value: usize) -> u8 {
    if value == 0 {
        0
    } else {
        (value as u8) * 40 + 55
    }
}

const fn default_named(index: u8) -> Rgb {
    match index {
        0 => Rgb::new(0x1d, 0x1f, 0x21),
        1 => Rgb::new(0xcc, 0x66, 0x66),
        2 => Rgb::new(0xb5, 0xbd, 0x68),
        3 => Rgb::new(0xf0, 0xc6, 0x74),
        4 => Rgb::new(0x81, 0xa2, 0xbe),
        5 => Rgb::new(0xb2, 0x94, 0xbb),
        6 => Rgb::new(0x8a, 0xbe, 0xb7),
        7 => Rgb::new(0xc5, 0xc8, 0xc6),
        8 => Rgb::new(0x66, 0x66, 0x66),
        9 => Rgb::new(0xd5, 0x4e, 0x53),
        10 => Rgb::new(0xb9, 0xca, 0x4a),
        11 => Rgb::new(0xe7, 0xc5, 0x47),
        12 => Rgb::new(0x7a, 0xa6, 0xda),
        13 => Rgb::new(0xc3, 0x97, 0xd8),
        14 => Rgb::new(0x70, 0xc0, 0xb1),
        15 => Rgb::new(0xea, 0xea, 0xea),
        _ => panic!("only the first 16 palette entries have named defaults"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::{align_of, size_of};

    #[test]
    fn rgb_c_conversion() {
        let rgb = Rgb::new(1, 2, 3);
        let c = rgb.cval();

        assert_eq!(Rgb::from_c(c), rgb);
    }

    #[test]
    fn rgb_c_layout() {
        assert_eq!(size_of::<CRgb>(), 3);
        assert_eq!(align_of::<CRgb>(), 1);
    }

    #[test]
    fn default_palette_named_entries() {
        assert_eq!(DEFAULT_PALETTE[1], Rgb::new(204, 102, 102));
        assert_eq!(DEFAULT_PALETTE[2], Rgb::new(181, 189, 104));
        assert_eq!(DEFAULT_PALETTE[3], Rgb::new(240, 198, 116));
        assert_eq!(DEFAULT_PALETTE[7], Rgb::new(197, 200, 198));
    }

    #[test]
    fn default_palette_cube_and_grayscale_entries() {
        assert_eq!(DEFAULT_PALETTE[16], Rgb::new(0, 0, 0));
        assert_eq!(DEFAULT_PALETTE[17], Rgb::new(0, 0, 95));
        assert_eq!(DEFAULT_PALETTE[21], Rgb::new(0, 0, 255));
        assert_eq!(DEFAULT_PALETTE[232], Rgb::new(8, 8, 8));
        assert_eq!(DEFAULT_PALETTE[255], Rgb::new(238, 238, 238));
    }
}
