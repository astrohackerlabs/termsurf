#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Rgb {
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct DynamicRgb {
    override_rgb: Option<Rgb>,
    default_rgb: Option<Rgb>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct DynamicPalette {
    current: Palette,
    original: Palette,
    mask: PaletteMask,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct PaletteMask {
    words: [u64; 4],
}

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

    pub(super) fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.is_empty() {
            return None;
        }

        if let Some(hex) = bytes.strip_prefix(b"#") {
            return parse_hash_rgb(hex);
        }

        if let Some(rgb) = super::x11_color::get(bytes) {
            return Some(rgb);
        }

        if !bytes.starts_with(b"rgb") {
            return None;
        }
        let mut offset = 3;
        let use_intensity = if bytes.get(offset) == Some(&b'i') {
            offset += 1;
            true
        } else {
            false
        };
        if bytes.get(offset) != Some(&b':') {
            return None;
        }
        offset += 1;

        let mut parts = bytes[offset..].split(|byte| *byte == b'/');
        let r = parse_component(parts.next()?, use_intensity)?;
        let g = parse_component(parts.next()?, use_intensity)?;
        let b = parse_component(parts.next()?, use_intensity)?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self::new(r, g, b))
    }
}

fn parse_hash_rgb(hex: &[u8]) -> Option<Rgb> {
    let width = match hex.len() {
        3 => 1,
        6 => 2,
        9 => 3,
        12 => 4,
        _ => return None,
    };
    Some(Rgb::new(
        parse_hex_channel(&hex[..width])?,
        parse_hex_channel(&hex[width..width * 2])?,
        parse_hex_channel(&hex[width * 2..])?,
    ))
}

fn parse_component(bytes: &[u8], use_intensity: bool) -> Option<u8> {
    if use_intensity {
        parse_intensity_channel(bytes)
    } else {
        parse_hex_channel(bytes)
    }
}

fn parse_hex_channel(bytes: &[u8]) -> Option<u8> {
    if !(1..=4).contains(&bytes.len()) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let value = u16::from_str_radix(text, 16).ok()? as u32;
    let max = match bytes.len() {
        1 => 0x0f,
        2 => 0xff,
        3 => 0x0fff,
        4 => 0xffff,
        _ => return None,
    };
    Some(((value * 0xff) / max) as u8)
}

fn parse_intensity_channel(bytes: &[u8]) -> Option<u8> {
    let text = std::str::from_utf8(bytes).ok()?;
    let value = text.parse::<f64>().ok()?;
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return None;
    }
    Some((value * 255.0) as u8)
}

impl DynamicRgb {
    pub(super) const fn unset() -> Self {
        Self {
            override_rgb: None,
            default_rgb: None,
        }
    }

    pub(super) const fn init(default: Rgb) -> Self {
        Self {
            override_rgb: None,
            default_rgb: Some(default),
        }
    }

    pub(super) const fn get(self) -> Option<Rgb> {
        match self.override_rgb {
            Some(rgb) => Some(rgb),
            None => self.default_rgb,
        }
    }

    pub(super) fn set(&mut self, rgb: Rgb) {
        self.override_rgb = Some(rgb);
    }

    pub(super) fn set_default(&mut self, rgb: Option<Rgb>) {
        self.default_rgb = rgb;
    }

    pub(super) const fn default_rgb(self) -> Option<Rgb> {
        self.default_rgb
    }

    pub(super) fn reset(&mut self) {
        self.override_rgb = self.default_rgb;
    }
}

impl DynamicPalette {
    pub(super) const fn init(default: Palette) -> Self {
        Self {
            current: default,
            original: default,
            mask: PaletteMask::empty(),
        }
    }

    pub(super) const fn current(&self) -> &Palette {
        &self.current
    }

    pub(super) const fn original(&self) -> &Palette {
        &self.original
    }

    pub(super) fn set(&mut self, index: u8, rgb: Rgb) {
        self.current[index as usize] = rgb;
        self.mask.set(index);
    }

    pub(super) fn reset(&mut self, index: u8) {
        self.current[index as usize] = self.original[index as usize];
        self.mask.unset(index);
    }

    pub(super) fn reset_all(&mut self) {
        *self = Self::init(self.original);
    }

    pub(super) fn change_default(&mut self, default: Palette) {
        self.original = default;
        if self.mask.is_empty() {
            self.current = self.original;
            return;
        }

        let old_current = self.current;
        let mut current = default;
        for index in self.mask.iter_set() {
            current[index as usize] = old_current[index as usize];
        }
        self.current = current;
    }
}

impl PaletteMask {
    const fn empty() -> Self {
        Self { words: [0; 4] }
    }

    fn is_empty(self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    fn set(&mut self, index: u8) {
        let index = index as usize;
        self.words[index / 64] |= 1 << (index % 64);
    }

    fn unset(&mut self, index: u8) {
        let index = index as usize;
        self.words[index / 64] &= !(1 << (index % 64));
    }

    fn get(self, index: u8) -> bool {
        let index = index as usize;
        self.words[index / 64] & (1 << (index % 64)) != 0
    }

    fn iter_set(self) -> impl Iterator<Item = u8> {
        (0u16..256).filter_map(move |index| {
            let index = index as u8;
            self.get(index).then_some(index)
        })
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
    fn dynamic_rgb_set_reset_and_unset() {
        let mut rgb = DynamicRgb::unset();
        assert_eq!(rgb.get(), None);

        rgb.set(Rgb::new(1, 2, 3));
        assert_eq!(rgb.get(), Some(Rgb::new(1, 2, 3)));

        rgb.reset();
        assert_eq!(rgb.get(), None);
    }

    #[test]
    fn dynamic_rgb_reset_restores_default() {
        let mut rgb = DynamicRgb::init(Rgb::new(4, 5, 6));
        assert_eq!(rgb.get(), Some(Rgb::new(4, 5, 6)));

        rgb.set(Rgb::new(7, 8, 9));
        assert_eq!(rgb.get(), Some(Rgb::new(7, 8, 9)));

        rgb.reset();
        assert_eq!(rgb.get(), Some(Rgb::new(4, 5, 6)));
    }

    #[test]
    fn dynamic_palette_init() {
        let palette = DynamicPalette::init(DEFAULT_PALETTE);

        assert_eq!(palette.current(), &DEFAULT_PALETTE);
        assert_eq!(palette.original(), &DEFAULT_PALETTE);
        assert!(palette.mask.is_empty());
    }

    #[test]
    fn dynamic_palette_set() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);

        palette.set(3, Rgb::new(1, 2, 3));

        assert_eq!(palette.current()[3], Rgb::new(1, 2, 3));
        assert_eq!(palette.original()[3], DEFAULT_PALETTE[3]);
        assert!(palette.mask.get(3));
    }

    #[test]
    fn dynamic_palette_reset() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);
        palette.set(3, Rgb::new(1, 2, 3));

        palette.reset(3);

        assert_eq!(palette.current()[3], DEFAULT_PALETTE[3]);
        assert_eq!(palette.original()[3], DEFAULT_PALETTE[3]);
        assert!(!palette.mask.get(3));
    }

    #[test]
    fn dynamic_palette_reset_all() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);
        palette.set(3, Rgb::new(1, 2, 3));
        palette.set(4, Rgb::new(4, 5, 6));

        palette.reset_all();

        assert_eq!(palette.current(), &DEFAULT_PALETTE);
        assert_eq!(palette.original(), &DEFAULT_PALETTE);
        assert!(palette.mask.is_empty());
    }

    #[test]
    fn dynamic_palette_change_default_with_no_changes() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);
        let mut new_palette = DEFAULT_PALETTE;
        new_palette[3] = Rgb::new(1, 2, 3);

        palette.change_default(new_palette);

        assert_eq!(palette.current(), &new_palette);
        assert_eq!(palette.original(), &new_palette);
    }

    #[test]
    fn dynamic_palette_change_default_preserves_one_changed_entry() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);
        let override_rgb = Rgb::new(1, 2, 3);
        palette.set(3, override_rgb);
        let mut new_palette = DEFAULT_PALETTE;
        new_palette[3] = Rgb::new(4, 5, 6);
        new_palette[4] = Rgb::new(7, 8, 9);

        palette.change_default(new_palette);

        assert_eq!(palette.current()[3], override_rgb);
        assert_eq!(palette.current()[4], new_palette[4]);
        assert_eq!(palette.original(), &new_palette);
        assert!(palette.mask.get(3));
    }

    #[test]
    fn dynamic_palette_change_default_preserves_multiple_changed_entries() {
        let mut palette = DynamicPalette::init(DEFAULT_PALETTE);
        let override_three = Rgb::new(1, 2, 3);
        let override_four = Rgb::new(4, 5, 6);
        palette.set(3, override_three);
        palette.set(4, override_four);
        let mut new_palette = DEFAULT_PALETTE;
        new_palette[3] = Rgb::new(7, 8, 9);
        new_palette[4] = Rgb::new(10, 11, 12);
        new_palette[5] = Rgb::new(13, 14, 15);

        palette.change_default(new_palette);

        assert_eq!(palette.current()[3], override_three);
        assert_eq!(palette.current()[4], override_four);
        assert_eq!(palette.current()[5], new_palette[5]);
        assert_eq!(palette.original(), &new_palette);
        assert!(palette.mask.get(3));
        assert!(palette.mask.get(4));
    }

    #[test]
    fn rgb_parse_hex_forms() {
        assert_eq!(Rgb::parse(b"rgb:f/ff/fff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"rgb:7f/a0a0/0"), Some(Rgb::new(127, 160, 0)));
        assert_eq!(Rgb::parse(b"#fff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"#ffffff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"#fffffffff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"#ffffffffffff"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"#ff0010"), Some(Rgb::new(255, 0, 16)));
    }

    #[test]
    fn rgb_parse_rgbi_intensity_forms() {
        assert_eq!(Rgb::parse(b"rgbi:1.0/0/0"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(Rgb::parse(b"rgbi:0.5/0.25/0"), Some(Rgb::new(127, 63, 0)));
    }

    #[test]
    fn rgb_parse_x11_named_colors() {
        assert_eq!(Rgb::parse(b"red"), Some(Rgb::new(255, 0, 0)));
        assert_eq!(Rgb::parse(b"white"), Some(Rgb::new(255, 255, 255)));
        assert_eq!(Rgb::parse(b"black"), Some(Rgb::new(0, 0, 0)));
        assert_eq!(Rgb::parse(b"blue"), Some(Rgb::new(0, 0, 255)));
        assert_eq!(
            Rgb::parse(b"medium spring green"),
            Some(Rgb::new(0, 250, 154))
        );
        assert_eq!(
            Rgb::parse(b"mediumspringgreen"),
            Some(Rgb::new(0, 250, 154))
        );
        assert_eq!(Rgb::parse(b"ForestGreen"), Some(Rgb::new(34, 139, 34)));
        assert_eq!(Rgb::parse(b"FoReStGReen"), Some(Rgb::new(34, 139, 34)));
        assert_eq!(Rgb::parse(b" red "), Some(Rgb::new(255, 0, 0)));
    }

    #[test]
    fn rgb_parse_rejects_invalid_forms() {
        assert_eq!(Rgb::parse(b""), None);
        assert_eq!(Rgb::parse(b"RGB:f/0/0"), None);
        assert_eq!(Rgb::parse(b"RGBI:1.0/0/0"), None);
        assert_eq!(Rgb::parse(b"rgb:"), None);
        assert_eq!(Rgb::parse(b"rgb:a/a/a/"), None);
        assert_eq!(Rgb::parse(b"rgb:00000///"), None);
        assert_eq!(Rgb::parse(b"rgb:000/"), None);
        assert_eq!(Rgb::parse(b"rgbi:a/a/a"), None);
        assert_eq!(Rgb::parse(b"rgbi:-0.1/0/0"), None);
        assert_eq!(Rgb::parse(b"rgbi:1.1/0/0"), None);
        assert_eq!(Rgb::parse(b"rgbi:NaN/0/0"), None);
        assert_eq!(Rgb::parse(b"rgbi:inf/0/0"), None);
        assert_eq!(Rgb::parse(b"rgbi:0.5/0.0/1.0/0"), None);
        assert_eq!(Rgb::parse(b"rgb:0.5/0.0/1.0"), None);
        assert_eq!(Rgb::parse(b"rgb:not/hex/zz"), None);
        assert_eq!(Rgb::parse(b"#"), None);
        assert_eq!(Rgb::parse(b"#ff"), None);
        assert_eq!(Rgb::parse(b"#ffff"), None);
        assert_eq!(Rgb::parse(b"#fffff"), None);
        assert_eq!(Rgb::parse(b"#gggggg"), None);
        assert_eq!(Rgb::parse(b"\tred\t"), None);
        assert_eq!(Rgb::parse(b"\nred\n"), None);
        assert_eq!(Rgb::parse(b"nosuchcolor"), None);
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
