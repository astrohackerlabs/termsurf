use std::fmt;

use super::color::{Palette, Rgb};
use super::ref_counted_set::{self, AddError, Context, RefCountedSet};
use super::sgr::Underline;
use super::size::{BaseAddress, StyleCountInt};

pub(super) type Id = StyleCountInt;
pub(super) const DEFAULT_ID: Id = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Style {
    pub(super) fg_color: Color,
    pub(super) bg_color: Color,
    pub(super) underline_color: Color,
    pub(super) flags: Flags,
}

impl Style {
    pub(super) const fn default_style() -> Self {
        Self {
            fg_color: Color::None,
            bg_color: Color::None,
            underline_color: Color::None,
            flags: Flags::default_flags(),
        }
    }

    pub(super) fn is_default(self) -> bool {
        self == Self::default()
    }

    pub(super) fn fg(self, opts: Fg<'_>) -> Rgb {
        match self.fg_color {
            Color::None => {
                if self.flags.bold {
                    if let Some(BoldColor::Color(color)) = opts.bold {
                        return color;
                    }
                }

                opts.default
            }
            Color::Palette(idx) => {
                if self.flags.bold && opts.bold.is_some() {
                    let bright_offset = 8;
                    if idx < bright_offset {
                        return opts.palette[(idx + bright_offset) as usize];
                    }
                }

                opts.palette[idx as usize]
            }
            Color::Rgb(rgb) => {
                if self.flags.bold && rgb == opts.default {
                    if let Some(BoldColor::Color(color)) = opts.bold {
                        return color;
                    }
                }

                rgb
            }
        }
    }

    pub(super) fn bg_color(self, palette: &Palette) -> Option<Rgb> {
        match self.bg_color {
            Color::None => None,
            Color::Palette(idx) => Some(palette[idx as usize]),
            Color::Rgb(rgb) => Some(rgb),
        }
    }

    pub(super) fn underline_color(self, palette: &Palette) -> Option<Rgb> {
        match self.underline_color {
            Color::None => None,
            Color::Palette(idx) => Some(palette[idx as usize]),
            Color::Rgb(rgb) => Some(rgb),
        }
    }

    pub(super) fn formatter_vt(&self) -> VtFormatter<'_> {
        VtFormatter {
            style: self,
            palette: None,
        }
    }

    pub(super) fn formatter_html(&self) -> HtmlFormatter<'_> {
        HtmlFormatter {
            style: self,
            palette: None,
        }
    }

    pub(super) fn hash(self) -> u64 {
        StorageStyle::from_style(self).hash()
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::default_style()
    }
}

#[repr(C, align(4))]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct StorageStyle {
    bytes: [u8; 28],
}

impl StorageStyle {
    const FG_TAG: usize = 0;
    const BG_TAG: usize = 1;
    const UNDERLINE_TAG: usize = 2;
    const FG_DATA: usize = 4;
    const BG_DATA: usize = 8;
    const UNDERLINE_DATA: usize = 12;
    const FLAGS: usize = 16;

    fn from_style(style: Style) -> Self {
        let mut storage = Self::default();
        storage.write_color(Self::FG_TAG, Self::FG_DATA, style.fg_color);
        storage.write_color(Self::BG_TAG, Self::BG_DATA, style.bg_color);
        storage.write_color(
            Self::UNDERLINE_TAG,
            Self::UNDERLINE_DATA,
            style.underline_color,
        );
        storage.bytes[Self::FLAGS..Self::FLAGS + 2]
            .copy_from_slice(&flags_bits(style.flags).to_le_bytes());
        storage
    }

    fn to_style(self) -> Style {
        Style {
            fg_color: self.read_color(Self::FG_TAG, Self::FG_DATA),
            bg_color: self.read_color(Self::BG_TAG, Self::BG_DATA),
            underline_color: self.read_color(Self::UNDERLINE_TAG, Self::UNDERLINE_DATA),
            flags: flags_from_bits(u16::from_le_bytes([
                self.bytes[Self::FLAGS],
                self.bytes[Self::FLAGS + 1],
            ])),
        }
    }

    fn hash(self) -> u64 {
        fnv1a64(&self.bytes)
    }

    fn write_color(&mut self, tag_offset: usize, data_offset: usize, color: Color) {
        match color {
            Color::None => {
                self.bytes[tag_offset] = 0;
            }
            Color::Palette(idx) => {
                self.bytes[tag_offset] = 1;
                self.bytes[data_offset] = idx;
            }
            Color::Rgb(rgb) => {
                self.bytes[tag_offset] = 2;
                self.bytes[data_offset] = rgb.r;
                self.bytes[data_offset + 1] = rgb.g;
                self.bytes[data_offset + 2] = rgb.b;
            }
        }
    }

    fn read_color(self, tag_offset: usize, data_offset: usize) -> Color {
        match self.bytes[tag_offset] {
            0 => Color::None,
            1 => Color::Palette(self.bytes[data_offset]),
            2 => Color::Rgb(Rgb::new(
                self.bytes[data_offset],
                self.bytes[data_offset + 1],
                self.bytes[data_offset + 2],
            )),
            tag => panic!("invalid stored style color tag {tag}"),
        }
    }
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

fn flags_bits(flags: Flags) -> u16 {
    let mut bits = 0_u16;
    bits |= u16::from(flags.bold);
    bits |= u16::from(flags.italic) << 1;
    bits |= u16::from(flags.faint) << 2;
    bits |= u16::from(flags.blink) << 3;
    bits |= u16::from(flags.inverse) << 4;
    bits |= u16::from(flags.invisible) << 5;
    bits |= u16::from(flags.strikethrough) << 6;
    bits |= u16::from(flags.overline) << 7;
    bits | (underline_bits(flags.underline) << 8)
}

fn flags_from_bits(bits: u16) -> Flags {
    Flags {
        bold: bits & 1 != 0,
        italic: bits & (1 << 1) != 0,
        faint: bits & (1 << 2) != 0,
        blink: bits & (1 << 3) != 0,
        inverse: bits & (1 << 4) != 0,
        invisible: bits & (1 << 5) != 0,
        strikethrough: bits & (1 << 6) != 0,
        overline: bits & (1 << 7) != 0,
        underline: underline_from_bits((bits >> 8) & 0b111),
    }
}

fn underline_bits(underline: Underline) -> u16 {
    match underline {
        Underline::None => 0,
        Underline::Single => 1,
        Underline::Double => 2,
        Underline::Curly => 3,
        Underline::Dotted => 4,
        Underline::Dashed => 5,
    }
}

fn underline_from_bits(bits: u16) -> Underline {
    match bits {
        0 => Underline::None,
        1 => Underline::Single,
        2 => Underline::Double,
        3 => Underline::Curly,
        4 => Underline::Dotted,
        5 => Underline::Dashed,
        value => panic!("invalid stored style underline value {value}"),
    }
}

#[derive(Debug, Default)]
struct StyleContext;

impl Context<StorageStyle> for StyleContext {
    fn hash(&self, value: StorageStyle) -> u64 {
        value.hash()
    }

    fn eql(&self, candidate: StorageStyle, resident: StorageStyle) -> bool {
        candidate == resident
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Set {
    inner: RefCountedSet<StorageStyle>,
}

impl Set {
    pub(super) const BASE_ALIGN: usize = RefCountedSet::<StorageStyle>::BASE_ALIGN;

    pub(super) fn capacity_for_count(count: usize) -> usize {
        RefCountedSet::<StorageStyle>::capacity_for_count(count)
    }

    pub(super) fn layout(capacity: usize) -> ref_counted_set::Layout {
        RefCountedSet::<StorageStyle>::layout(capacity)
    }

    pub(super) fn init<B>(base: B, layout: ref_counted_set::Layout) -> Self
    where
        B: BaseAddress + Copy,
    {
        Self {
            inner: RefCountedSet::init(base, layout),
        }
    }

    pub(super) fn add<B>(&mut self, base: B, style: Style) -> Result<Id, AddError>
    where
        B: BaseAddress + Copy,
    {
        self.inner
            .add(base, StorageStyle::from_style(style), &mut StyleContext)
    }

    pub(super) fn add_with_id<B>(
        &mut self,
        base: B,
        style: Style,
        id: Id,
    ) -> Result<Option<Id>, AddError>
    where
        B: BaseAddress + Copy,
    {
        self.inner
            .add_with_id(base, StorageStyle::from_style(style), id, &mut StyleContext)
    }

    pub(super) fn lookup<B>(&self, base: B, style: Style) -> Option<Id>
    where
        B: BaseAddress + Copy,
    {
        self.inner
            .lookup(base, StorageStyle::from_style(style), &StyleContext)
    }

    pub(super) fn get<B>(&self, base: B, id: Id) -> Style
    where
        B: BaseAddress + Copy,
    {
        self.inner.get(base, id).to_style()
    }

    pub(super) fn use_one<B>(&self, base: B, id: Id)
    where
        B: BaseAddress + Copy,
    {
        self.inner.use_one(base, id);
    }

    pub(super) fn use_multiple<B>(&self, base: B, id: Id, count: Id)
    where
        B: BaseAddress + Copy,
    {
        self.inner.use_multiple(base, id, count);
    }

    pub(super) fn release<B>(&mut self, base: B, id: Id)
    where
        B: BaseAddress + Copy,
    {
        self.inner.release(base, id);
    }

    pub(super) fn release_multiple<B>(&mut self, base: B, id: Id, count: Id)
    where
        B: BaseAddress + Copy,
    {
        self.inner.release_multiple(base, id, count);
    }

    pub(super) fn ref_count<B>(&self, base: B, id: Id) -> Id
    where
        B: BaseAddress + Copy,
    {
        self.inner.ref_count(base, id)
    }

    pub(super) fn contains_id<B>(&self, base: B, id: Id) -> bool
    where
        B: BaseAddress + Copy,
    {
        self.inner.contains_id(base, id)
    }

    pub(super) fn count(&self) -> usize {
        self.inner.count()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Color {
    #[default]
    None,
    Palette(u8),
    Rgb(Rgb),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Flags {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) faint: bool,
    pub(super) blink: bool,
    pub(super) inverse: bool,
    pub(super) invisible: bool,
    pub(super) strikethrough: bool,
    pub(super) overline: bool,
    pub(super) underline: Underline,
}

impl Flags {
    pub(super) const fn default_flags() -> Self {
        Self {
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            underline: Underline::None,
        }
    }
}

impl Default for Flags {
    fn default() -> Self {
        Self::default_flags()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoldColor {
    Color(Rgb),
    Bright,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Fg<'a> {
    pub(super) default: Rgb,
    pub(super) palette: &'a Palette,
    pub(super) bold: Option<BoldColor>,
}

pub(super) struct VtFormatter<'a> {
    style: &'a Style,
    palette: Option<&'a Palette>,
}

impl<'a> VtFormatter<'a> {
    pub(super) fn with_palette(mut self, palette: &'a Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    fn format_color(&self, f: &mut fmt::Formatter<'_>, prefix: u8, color: Color) -> fmt::Result {
        match color {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(palette) = self.palette {
                    let rgb = palette[idx as usize];
                    write!(f, "\x1b[{prefix};2;{};{};{}m", rgb.r, rgb.g, rgb.b)
                } else {
                    write!(f, "\x1b[{prefix};5;{idx}m")
                }
            }
            Color::Rgb(rgb) => write!(f, "\x1b[{prefix};2;{};{};{}m", rgb.r, rgb.g, rgb.b),
        }
    }
}

impl fmt::Display for VtFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[0m")?;

        if self.style.flags.bold {
            write!(f, "\x1b[1m")?;
        }
        if self.style.flags.faint {
            write!(f, "\x1b[2m")?;
        }
        if self.style.flags.italic {
            write!(f, "\x1b[3m")?;
        }
        if self.style.flags.blink {
            write!(f, "\x1b[5m")?;
        }
        if self.style.flags.inverse {
            write!(f, "\x1b[7m")?;
        }
        if self.style.flags.invisible {
            write!(f, "\x1b[8m")?;
        }
        if self.style.flags.strikethrough {
            write!(f, "\x1b[9m")?;
        }
        if self.style.flags.overline {
            write!(f, "\x1b[53m")?;
        }
        match self.style.flags.underline {
            Underline::None => {}
            Underline::Single => write!(f, "\x1b[4m")?,
            Underline::Double => write!(f, "\x1b[4:2m")?,
            Underline::Curly => write!(f, "\x1b[4:3m")?,
            Underline::Dotted => write!(f, "\x1b[4:4m")?,
            Underline::Dashed => write!(f, "\x1b[4:5m")?,
        }

        self.format_color(f, 38, self.style.fg_color)?;
        self.format_color(f, 48, self.style.bg_color)?;
        self.format_color(f, 58, self.style.underline_color)
    }
}

pub(super) struct HtmlFormatter<'a> {
    style: &'a Style,
    palette: Option<&'a Palette>,
}

impl<'a> HtmlFormatter<'a> {
    pub(super) fn with_palette(mut self, palette: &'a Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    fn format_color(
        &self,
        f: &mut fmt::Formatter<'_>,
        property: &str,
        color: Color,
    ) -> fmt::Result {
        match color {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(palette) = self.palette {
                    let rgb = palette[idx as usize];
                    write!(f, "{property}: rgb({}, {}, {});", rgb.r, rgb.g, rgb.b)
                } else {
                    write!(f, "{property}: var(--vt-palette-{idx});")
                }
            }
            Color::Rgb(rgb) => write!(f, "{property}: rgb({}, {}, {});", rgb.r, rgb.g, rgb.b),
        }
    }
}

impl fmt::Display for HtmlFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format_color(f, "color", self.style.fg_color)?;
        self.format_color(f, "background-color", self.style.bg_color)?;
        self.format_color(f, "text-decoration-color", self.style.underline_color)?;

        let has_line = self.style.flags.underline != Underline::None
            || self.style.flags.strikethrough
            || self.style.flags.overline
            || self.style.flags.blink;
        if has_line {
            write!(f, "text-decoration-line:")?;
            if self.style.flags.underline != Underline::None {
                write!(f, " underline")?;
            }
            if self.style.flags.strikethrough {
                write!(f, " line-through")?;
            }
            if self.style.flags.overline {
                write!(f, " overline")?;
            }
            if self.style.flags.blink {
                write!(f, " blink")?;
            }
            write!(f, ";")?;
        }

        match self.style.flags.underline {
            Underline::None => {}
            Underline::Single => write!(f, "text-decoration-style: solid;")?,
            Underline::Double => write!(f, "text-decoration-style: double;")?,
            Underline::Curly => write!(f, "text-decoration-style: wavy;")?,
            Underline::Dotted => write!(f, "text-decoration-style: dotted;")?,
            Underline::Dashed => write!(f, "text-decoration-style: dashed;")?,
        }

        if self.style.flags.bold {
            write!(f, "font-weight: bold;")?;
        }
        if self.style.flags.italic {
            write!(f, "font-style: italic;")?;
        }
        if self.style.flags.faint {
            write!(f, "opacity: 0.5;")?;
        }
        if self.style.flags.invisible {
            write!(f, "visibility: hidden;")?;
        }
        if self.style.flags.inverse {
            write!(f, "filter: invert(100%);")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::mem::{align_of, size_of};

    use super::super::color::{Rgb, DEFAULT_PALETTE};
    use super::super::ref_counted_set;
    use super::*;

    fn style_with_flags(flags: Flags) -> Style {
        Style {
            flags,
            ..Style::default()
        }
    }

    fn palette_style(index: u8) -> Style {
        Style {
            fg_color: Color::Palette(index),
            ..Style::default()
        }
    }

    fn rgb_style(r: u8, g: u8, b: u8) -> Style {
        Style {
            fg_color: Color::Rgb(Rgb::new(r, g, b)),
            ..Style::default()
        }
    }

    fn init_set(capacity: usize) -> (Vec<u64>, Set) {
        let layout = Set::layout(capacity);
        let mut backing = vec![0_u64; layout.total_size.div_ceil(size_of::<u64>())];
        let set = Set::init(backing.as_mut_ptr().cast::<u8>(), layout);
        (backing, set)
    }

    fn backing_ptr(backing: &mut [u64]) -> *mut u8 {
        backing.as_mut_ptr().cast::<u8>()
    }

    fn same_bucket_styles(count: usize, mask: u16) -> Vec<Style> {
        let mut buckets: Vec<Vec<Style>> = vec![Vec::new(); mask as usize + 1];
        for index in 0..=u8::MAX {
            let style = palette_style(index);
            let bucket = (style.hash() as u16 & mask) as usize;
            buckets[bucket].push(style);
            if buckets[bucket].len() == count {
                return buckets[bucket].clone();
            }
        }
        panic!("unable to find {count} style values in the same bucket");
    }

    #[test]
    fn style_default_and_equality() {
        assert!(Style::default().is_default());
        assert_eq!(Style::default(), Style::default_style());

        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });
        assert!(!style.is_default());
        assert_eq!(DEFAULT_ID, 0);
    }

    #[test]
    fn style_storage_layout() {
        assert_eq!(size_of::<Style>(), 21);
        assert_eq!(align_of::<Style>(), 1);
        assert_eq!(size_of::<StorageStyle>(), 28);
        assert_eq!(align_of::<StorageStyle>(), 4);
        assert_eq!(size_of::<ref_counted_set::Item<StorageStyle>>(), 36);
        assert_eq!(align_of::<ref_counted_set::Item<StorageStyle>>(), 4);

        let layout = Set::layout(16);
        assert_eq!(Set::BASE_ALIGN, 8);
        assert_eq!(layout.cap, 13);
        assert_eq!(layout.table_cap, 16);
        assert_eq!(layout.items_start, 32);
        assert_eq!(layout.total_size, 500);
    }

    #[test]
    fn style_storage_round_trip() {
        let style = Style {
            fg_color: Color::Palette(42),
            bg_color: Color::Rgb(Rgb::new(1, 2, 3)),
            underline_color: Color::Rgb(Rgb::new(4, 5, 6)),
            flags: Flags {
                bold: true,
                italic: true,
                underline: Underline::Curly,
                ..Flags::default()
            },
        };

        assert_eq!(StorageStyle::from_style(style).to_style(), style);
    }

    #[test]
    fn style_hash_known_vectors() {
        assert_eq!(Style::default().hash(), 0x17d9_c152_39d0_81d5);
        assert_eq!(
            style_with_flags(Flags {
                bold: true,
                ..Flags::default()
            })
            .hash(),
            0x2296_3db1_df02_4924
        );
        assert_eq!(
            style_with_flags(Flags {
                italic: true,
                ..Flags::default()
            })
            .hash(),
            0x0260_c892_ef6c_f337
        );
        assert_eq!(palette_style(42).hash(), 0x6eb0_318d_c584_608e);
        assert_eq!(rgb_style(255, 128, 64).hash(), 0xde06_3d18_a320_c0a8);
        assert_eq!(
            style_with_flags(Flags {
                bold: true,
                ..Flags::default()
            })
            .hash(),
            style_with_flags(Flags {
                bold: true,
                ..Flags::default()
            })
            .hash()
        );
    }

    #[test]
    fn foreground_bold_behavior() {
        let default = Rgb::new(1, 2, 3);
        let bold = Rgb::new(4, 5, 6);
        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });

        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Color(bold)),
            }),
            bold
        );

        let style = Style {
            fg_color: Color::Palette(1),
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Bright),
            }),
            DEFAULT_PALETTE[9]
        );

        let style = Style {
            fg_color: Color::Rgb(default),
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Color(bold)),
            }),
            bold
        );
    }

    #[test]
    fn background_and_underline_color_lookup() {
        let style = Style {
            bg_color: Color::Palette(2),
            underline_color: Color::Rgb(Rgb::new(8, 9, 10)),
            ..Style::default()
        };

        assert_eq!(style.bg_color(&DEFAULT_PALETTE), Some(DEFAULT_PALETTE[2]));
        assert_eq!(
            style.underline_color(&DEFAULT_PALETTE),
            Some(Rgb::new(8, 9, 10))
        );
    }

    #[test]
    fn style_set_basic_usage() {
        let (mut backing, mut set) = init_set(16);
        let base = backing_ptr(&mut backing);
        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });
        let style2 = style_with_flags(Flags {
            italic: true,
            ..Flags::default()
        });

        let id = set.add(base, style).unwrap();
        assert!(id > 0);

        let id_again = set.add(base, style).unwrap();
        assert_eq!(id, id_again);
        assert_eq!(set.lookup(base, style), Some(id));

        let found = set.get(base, id);
        assert!(found.flags.bold);
        assert_eq!(found, set.get(base, id));

        let id2 = set.add(base, style2).unwrap();
        assert!(set.get(base, id2).flags.italic);
        assert_eq!(set.count(), 2);
        assert_eq!(set.ref_count(base, id), 2);
        assert_eq!(set.ref_count(base, id2), 1);

        set.use_one(base, id2);
        set.use_multiple(base, id2, 2);
        assert_eq!(set.ref_count(base, id2), 4);
        set.release_multiple(base, id2, 3);
        assert_eq!(set.ref_count(base, id2), 1);

        set.release(base, id);
        assert_eq!(set.ref_count(base, id), 1);
        set.release(base, id2);
        assert_eq!(set.ref_count(base, id2), 0);
        set.release(base, id);
        assert_eq!(set.ref_count(base, id), 0);
    }

    #[test]
    fn style_set_capacities() {
        let layout = Set::layout(16_384);

        assert_eq!(Set::capacity_for_count(0), 0);
        assert!(layout.total_size > 0);
    }

    #[test]
    fn style_set_add_with_id_uses_requested_id() {
        let (mut backing, mut set) = init_set(16);
        let base = backing_ptr(&mut backing);
        let style = palette_style(1);
        let id = set.add(base, style).unwrap();
        set.release(base, id);

        let result = set.add_with_id(base, palette_style(2), id).unwrap();

        assert_eq!(result, None);
        assert_eq!(set.get(base, id), palette_style(2));
    }

    #[test]
    fn style_set_add_with_id_returns_alternate_id() {
        let (mut backing, mut set) = init_set(16);
        let base = backing_ptr(&mut backing);
        let styles = same_bucket_styles(4, Set::layout(16).table_mask);
        let id1 = set.add(base, styles[0]).unwrap();
        let _id2 = set.add(base, styles[1]).unwrap();
        let id3 = set.add(base, styles[2]).unwrap();
        set.release(base, id1);
        set.release(base, id3);

        let result = set.add_with_id(base, styles[3], id3).unwrap();

        assert_eq!(result, Some(id1));
        assert_eq!(set.get(base, id1), styles[3]);
    }

    #[test]
    fn style_vt_formatting_empty() {
        let style = Style::default();
        assert_eq!(style.formatter_vt().to_string(), "\x1b[0m");
    }

    #[test]
    fn style_vt_formatting_single_flags() {
        let cases = [
            (
                Flags {
                    bold: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[1m",
            ),
            (
                Flags {
                    faint: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[2m",
            ),
            (
                Flags {
                    italic: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[3m",
            ),
            (
                Flags {
                    blink: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[5m",
            ),
            (
                Flags {
                    inverse: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[7m",
            ),
            (
                Flags {
                    invisible: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[8m",
            ),
            (
                Flags {
                    strikethrough: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[9m",
            ),
            (
                Flags {
                    overline: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[53m",
            ),
        ];

        for (flags, expected) in cases {
            assert_eq!(style_with_flags(flags).formatter_vt().to_string(), expected);
        }
    }

    #[test]
    fn style_vt_formatting_flags() {
        let style = style_with_flags(Flags {
            bold: true,
            faint: true,
            italic: true,
            blink: true,
            inverse: true,
            invisible: true,
            strikethrough: true,
            overline: true,
            underline: Underline::Curly,
        });
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[1m\x1b[2m\x1b[3m\x1b[5m\x1b[7m\x1b[8m\x1b[9m\x1b[53m\x1b[4:3m"
        );
    }

    #[test]
    fn style_vt_formatting_each_underline() {
        let cases = [
            (Underline::Single, "\x1b[0m\x1b[4m"),
            (Underline::Double, "\x1b[0m\x1b[4:2m"),
            (Underline::Curly, "\x1b[0m\x1b[4:3m"),
            (Underline::Dotted, "\x1b[0m\x1b[4:4m"),
            (Underline::Dashed, "\x1b[0m\x1b[4:5m"),
        ];

        for (underline, expected) in cases {
            let style = style_with_flags(Flags {
                underline,
                ..Flags::default()
            });
            assert_eq!(style.formatter_vt().to_string(), expected);
        }
    }

    #[test]
    fn style_vt_formatting_colors() {
        let cases = [
            (
                Style {
                    fg_color: Color::Palette(42),
                    ..Style::default()
                },
                "\x1b[0m\x1b[38;5;42m",
            ),
            (
                Style {
                    fg_color: Color::Rgb(Rgb::new(255, 128, 64)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[38;2;255;128;64m",
            ),
            (
                Style {
                    bg_color: Color::Palette(7),
                    ..Style::default()
                },
                "\x1b[0m\x1b[48;5;7m",
            ),
            (
                Style {
                    bg_color: Color::Rgb(Rgb::new(32, 64, 96)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[48;2;32;64;96m",
            ),
            (
                Style {
                    underline_color: Color::Palette(15),
                    ..Style::default()
                },
                "\x1b[0m\x1b[58;5;15m",
            ),
            (
                Style {
                    underline_color: Color::Rgb(Rgb::new(200, 100, 50)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[58;2;200;100;50m",
            ),
        ];

        for (style, expected) in cases {
            assert_eq!(style.formatter_vt().to_string(), expected);
        }

        let style = Style {
            fg_color: Color::Rgb(Rgb::new(10, 20, 30)),
            bg_color: Color::Rgb(Rgb::new(40, 50, 60)),
            underline_color: Color::Rgb(Rgb::new(70, 80, 90)),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[38;2;10;20;30m\x1b[48;2;40;50;60m\x1b[58;2;70;80;90m"
        );

        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[38;5;1m\x1b[48;5;2m\x1b[58;5;3m"
        );
        assert_eq!(
            Style {
                fg_color: Color::Palette(1),
                ..Style::default()
            }
            .formatter_vt()
            .with_palette(&DEFAULT_PALETTE)
            .to_string(),
            "\x1b[0m\x1b[38;2;204;102;102m"
        );
        assert_eq!(
            style
                .formatter_vt()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "\x1b[0m\x1b[38;2;204;102;102m\x1b[48;2;181;189;104m\x1b[58;2;240;198;116m"
        );
    }

    #[test]
    fn style_vt_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 0, 0)),
            bg_color: Color::Palette(8),
            underline_color: Color::Rgb(Rgb::new(0, 255, 0)),
            flags: Flags {
                bold: true,
                italic: true,
                underline: Underline::Double,
                ..Flags::default()
            },
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[1m\x1b[3m\x1b[4:2m\x1b[38;2;255;0;0m\x1b[48;5;8m\x1b[58;2;0;255;0m"
        );
    }

    #[test]
    fn style_html_formatting_basic_bold() {
        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });
        assert_eq!(style.formatter_html().to_string(), "font-weight: bold;");
    }

    #[test]
    fn style_html_formatting_colors() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 128, 64)),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_html().to_string(),
            "color: rgb(255, 128, 64);"
        );

        let style = Style {
            bg_color: Color::Palette(7),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_html().to_string(),
            "background-color: var(--vt-palette-7);"
        );
        assert_eq!(
            style
                .formatter_html()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "background-color: rgb(197, 200, 198);"
        );
    }

    #[test]
    fn style_html_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 0, 0)),
            bg_color: Color::Rgb(Rgb::new(0, 0, 255)),
            flags: Flags {
                bold: true,
                italic: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        let result = style.formatter_html().to_string();
        assert!(result.contains("color: rgb(255, 0, 0);"));
        assert!(result.contains("background-color: rgb(0, 0, 255);"));
        assert!(result.contains("font-weight: bold;"));
        assert!(result.contains("font-style: italic;"));
    }

    #[test]
    fn style_html_formatting_decoration() {
        let style = style_with_flags(Flags {
            underline: Underline::Single,
            ..Flags::default()
        });
        let result = style.formatter_html().to_string();
        assert!(result.contains("text-decoration-line: underline;"));
        assert!(result.contains("text-decoration-style: solid;"));

        let style = style_with_flags(Flags {
            underline: Underline::Curly,
            strikethrough: true,
            overline: true,
            ..Flags::default()
        });
        let result = style.formatter_html().to_string();
        assert!(result.contains("text-decoration-line: underline line-through overline;"));
        assert!(result.contains("text-decoration-style: wavy;"));
    }

    #[test]
    fn style_html_formatting_all_palette_colors_with_palette_set() {
        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            style
                .formatter_html()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "color: rgb(204, 102, 102);background-color: rgb(181, 189, 104);text-decoration-color: rgb(240, 198, 116);"
        );
    }
}
