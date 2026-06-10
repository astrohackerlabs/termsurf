//! Ghostty-shaped Unicode codepoint properties.
//!
//! Ghostty generates this data from `vendor/ghostty/src/unicode/props*.zig`.
//! This module is the first Rust-side facade with the same property shape; the
//! full generated table and grapheme-break state machine can replace the local
//! representative classifiers without changing terminal call sites.

use unicode_width::UnicodeWidthChar;

mod grapheme_table;
mod tables;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Properties {
    /// Terminal cell width clamped to Ghostty's `[0, 2]` range.
    pub(crate) width: u8,
    /// Whether this codepoint contributes zero width inside a grapheme cluster.
    pub(crate) width_zero_in_grapheme: bool,
    /// Grapheme break property, excluding control-specific variants.
    pub(crate) grapheme_break: GraphemeBreak,
    /// Whether this codepoint can be the base of an emoji variation sequence.
    pub(crate) emoji_vs_base: bool,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphemeBreak {
    Other,
    Prepend,
    RegionalIndicator,
    SpacingMark,
    L,
    V,
    T,
    Lv,
    Lvt,
    Zwj,
    Zwnj,
    ExtendedPictographic,
    EmojiModifierBase,
    EmojiModifier,
    IndicConjunctBreakExtend,
    IndicConjunctBreakLinker,
    IndicConjunctBreakConsonant,
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum BreakState {
    #[default]
    Default,
    RegionalIndicator,
    ExtendedPictographic,
    IndicConjunctBreakConsonant,
    IndicConjunctBreakLinker,
}

pub(crate) fn get(codepoint: u32) -> Properties {
    let high = (codepoint >> 8) as usize;
    if high >= tables::STAGE1.len() {
        return fallback();
    }

    let low = (codepoint & 0xff) as usize;
    let stage2_index = usize::from(tables::STAGE1[high]) + low;
    let stage3_index = usize::from(tables::STAGE2[stage2_index]);
    tables::STAGE3[stage3_index]
}

pub(crate) fn grapheme_break(previous: u32, current: u32, state: &mut BreakState) -> bool {
    let previous_break = get(previous).grapheme_break;
    let current_break = get(current).grapheme_break;
    let index = (*state as usize * GRAPHEME_BREAK_COUNT * GRAPHEME_BREAK_COUNT)
        + (previous_break as usize * GRAPHEME_BREAK_COUNT)
        + current_break as usize;
    let transition = grapheme_table::BREAK_TRANSITIONS[index];
    *state = break_state_from_index(transition >> 1);
    transition & 1 == 1
}

const GRAPHEME_BREAK_COUNT: usize = 17;
const BREAK_STATE_COUNT: usize = 5;

const fn break_state_from_index(index: u8) -> BreakState {
    match index {
        0 => BreakState::Default,
        1 => BreakState::RegionalIndicator,
        2 => BreakState::ExtendedPictographic,
        3 => BreakState::IndicConjunctBreakConsonant,
        4 => BreakState::IndicConjunctBreakLinker,
        _ => BreakState::Default,
    }
}

fn standalone_width(ch: char, codepoint: u32) -> u8 {
    if is_hangul_v(codepoint)
        || is_hangul_t(codepoint)
        || is_nonspacing_or_enclosing_mark(codepoint)
        || is_spacing_mark(codepoint)
    {
        return 1;
    }

    UnicodeWidthChar::width(ch).unwrap_or(0).min(2) as u8
}

const fn fallback() -> Properties {
    Properties {
        width: 1,
        width_zero_in_grapheme: true,
        grapheme_break: GraphemeBreak::Other,
        emoji_vs_base: false,
    }
}

const fn width_zero_in_grapheme(codepoint: u32) -> bool {
    if is_hangul_v(codepoint) || is_hangul_t(codepoint) {
        return true;
    }

    matches!(
        codepoint,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x06DF..=0x06E4
            | 0x06E7..=0x06E8
            | 0x06EA..=0x06ED
            | 0x0711
            | 0x0730..=0x074A
            | 0x07A6..=0x07B0
            | 0x07EB..=0x07F3
            | 0x0816..=0x0819
            | 0x081B..=0x0823
            | 0x0825..=0x0827
            | 0x0829..=0x082D
            | 0x0859..=0x085B
            | 0x0898..=0x089F
            | 0x08CA..=0x08E1
            | 0x08E3..=0x0902
            | 0x093A
            | 0x093C
            | 0x0941..=0x0948
            | 0x094D
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0981
            | 0x09BC
            | 0x09C1..=0x09C4
            | 0x09CD
            | 0x09E2..=0x09E3
            | 0x09FE
            | 0x0A01..=0x0A02
            | 0x0A3C
            | 0x0A41..=0x0A42
            | 0x0A47..=0x0A48
            | 0x0A4B..=0x0A4D
            | 0x0A51
            | 0x0A70..=0x0A71
            | 0x0A75
            | 0x0A81..=0x0A82
            | 0x0ABC
            | 0x0AC1..=0x0AC5
            | 0x0AC7..=0x0AC8
            | 0x0ACD
            | 0x0AE2..=0x0AE3
            | 0x0AFA..=0x0AFF
            | 0x0B01
            | 0x0B3C
            | 0x0B3F
            | 0x0B41..=0x0B44
            | 0x0B4D
            | 0x0B55..=0x0B56
            | 0x0B62..=0x0B63
            | 0x0B82
            | 0x0BC0
            | 0x0BCD
            | 0x0C00
            | 0x0C04
            | 0x0C3C
            | 0x0C3E..=0x0C40
            | 0x0C46..=0x0C48
            | 0x0C4A..=0x0C4D
            | 0x0C55..=0x0C56
            | 0x0C62..=0x0C63
            | 0x0C81
            | 0x0CBC
            | 0x0CBF
            | 0x0CC6
            | 0x0CCC..=0x0CCD
            | 0x0CE2..=0x0CE3
            | 0x0D00..=0x0D01
            | 0x0D3B..=0x0D3C
            | 0x0D41..=0x0D44
            | 0x0D4D
            | 0x0D62..=0x0D63
            | 0x0D81
            | 0x0DCA
            | 0x0DD2..=0x0DD4
            | 0x0DD6
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x0EB1
            | 0x0EB4..=0x0EBC
            | 0x0EC8..=0x0ECE
            | 0x0F18..=0x0F19
            | 0x0F35
            | 0x0F37
            | 0x0F39
            | 0x0F71..=0x0F7E
            | 0x0F80..=0x0F84
            | 0x0F86..=0x0F87
            | 0x0F8D..=0x0F97
            | 0x0F99..=0x0FBC
            | 0x0FC6
            | 0x102D..=0x1030
            | 0x1032..=0x1037
            | 0x1039..=0x103A
            | 0x103D..=0x103E
            | 0x1058..=0x1059
            | 0x105E..=0x1060
            | 0x1071..=0x1074
            | 0x1082
            | 0x1085..=0x1086
            | 0x108D
            | 0x109D
            | 0x135D..=0x135F
            | 0x1712..=0x1715
            | 0x1732..=0x1734
            | 0x1752..=0x1753
            | 0x1772..=0x1773
            | 0x17B4..=0x17B5
            | 0x17B7..=0x17BD
            | 0x17C6
            | 0x17C9..=0x17D3
            | 0x17DD
            | 0x180B..=0x180D
            | 0x180F
            | 0x1885..=0x1886
            | 0x18A9
            | 0x1920..=0x1922
            | 0x1927..=0x1928
            | 0x1932
            | 0x1939..=0x193B
            | 0x1A17..=0x1A18
            | 0x1A1B
            | 0x1A56
            | 0x1A58..=0x1A5E
            | 0x1A60
            | 0x1A62
            | 0x1A65..=0x1A6C
            | 0x1A73..=0x1A7C
            | 0x1A7F
            | 0x1AB0..=0x1ACE
            | 0x1B00..=0x1B03
            | 0x1B34
            | 0x1B36..=0x1B3A
            | 0x1B3C
            | 0x1B42
            | 0x1B6B..=0x1B73
            | 0x1B80..=0x1B81
            | 0x1BA2..=0x1BA5
            | 0x1BA8..=0x1BA9
            | 0x1BAB..=0x1BAD
            | 0x1BE6
            | 0x1BE8..=0x1BE9
            | 0x1BED
            | 0x1BEF..=0x1BF1
            | 0x1C2C..=0x1C33
            | 0x1C36..=0x1C37
            | 0x1CD0..=0x1CD2
            | 0x1CD4..=0x1CE0
            | 0x1CE2..=0x1CE8
            | 0x1CED
            | 0x1CF4
            | 0x1CF8..=0x1CF9
            | 0x1DC0..=0x1DFF
            | 0x200C..=0x200D
            | 0x20D0..=0x20F0
            | 0x2CEF..=0x2CF1
            | 0x2D7F
            | 0x2DE0..=0x2DFF
            | 0x302A..=0x302D
            | 0x3099..=0x309A
            | 0xA66F
            | 0xA674..=0xA67D
            | 0xA69E..=0xA69F
            | 0xA6F0..=0xA6F1
            | 0xA802
            | 0xA806
            | 0xA80B
            | 0xA825..=0xA826
            | 0xA82C
            | 0xA8C4..=0xA8C5
            | 0xA8E0..=0xA8F1
            | 0xA8FF
            | 0xA926..=0xA92D
            | 0xA947..=0xA951
            | 0xA980..=0xA982
            | 0xA9B3
            | 0xA9B6..=0xA9B9
            | 0xA9BC
            | 0xA9E5
            | 0xAA29..=0xAA2E
            | 0xAA31..=0xAA32
            | 0xAA35..=0xAA36
            | 0xAA43
            | 0xAA4C
            | 0xAA7C
            | 0xAAB0
            | 0xAAB2..=0xAAB4
            | 0xAAB7..=0xAAB8
            | 0xAABE..=0xAABF
            | 0xAAC1
            | 0xAAEC..=0xAAED
            | 0xAAF6
            | 0xABE5
            | 0xABE8
            | 0xABED
            | 0xFB1E
            | 0xFE00..=0xFE0F
            | 0xFE20..=0xFE2F
            | 0x101FD
            | 0x102E0
            | 0x10376..=0x1037A
            | 0x10A01..=0x10A03
            | 0x10A05..=0x10A06
            | 0x10A0C..=0x10A0F
            | 0x10A38..=0x10A3A
            | 0x10A3F
            | 0x10AE5..=0x10AE6
            | 0x10D24..=0x10D27
            | 0x10EAB..=0x10EAC
            | 0x10EFD..=0x10EFF
            | 0x10F46..=0x10F50
            | 0x10F82..=0x10F85
            | 0x11001
            | 0x11038..=0x11046
            | 0x11070
            | 0x11073..=0x11074
            | 0x1107F..=0x11081
            | 0x110B3..=0x110B6
            | 0x110B9..=0x110BA
            | 0x110C2
            | 0x11100..=0x11102
            | 0x11127..=0x1112B
            | 0x1112D..=0x11134
            | 0x11173
            | 0x11180..=0x11181
            | 0x111B6..=0x111BE
            | 0x111C9..=0x111CC
            | 0x111CF
            | 0x1122F..=0x11231
            | 0x11234
            | 0x11236..=0x11237
            | 0x1123E
            | 0x11241
            | 0x112DF
            | 0x112E3..=0x112EA
            | 0x11300..=0x11301
            | 0x1133B..=0x1133C
            | 0x11340
            | 0x11366..=0x1136C
            | 0x11370..=0x11374
            | 0x11438..=0x1143F
            | 0x11442..=0x11444
            | 0x11446
            | 0x1145E
            | 0x114B3..=0x114B8
            | 0x114BA
            | 0x114BF..=0x114C0
            | 0x114C2..=0x114C3
            | 0x115B2..=0x115B5
            | 0x115BC..=0x115BD
            | 0x115BF..=0x115C0
            | 0x115DC..=0x115DD
            | 0x11633..=0x1163A
            | 0x1163D
            | 0x1163F..=0x11640
            | 0x116AB
            | 0x116AD
            | 0x116B0..=0x116B5
            | 0x116B7
            | 0x1171D..=0x1171F
            | 0x11722..=0x11725
            | 0x11727..=0x1172B
            | 0x1182F..=0x11837
            | 0x11839..=0x1183A
            | 0x1193B..=0x1193C
            | 0x1193E
            | 0x11943
            | 0x119D4..=0x119D7
            | 0x119DA..=0x119DB
            | 0x119E0
            | 0x11A01..=0x11A0A
            | 0x11A33..=0x11A38
            | 0x11A3B..=0x11A3E
            | 0x11A47
            | 0x11A51..=0x11A56
            | 0x11A59..=0x11A5B
            | 0x11A8A..=0x11A96
            | 0x11A98..=0x11A99
            | 0x11C30..=0x11C36
            | 0x11C38..=0x11C3D
            | 0x11C3F
            | 0x11C92..=0x11CA7
            | 0x11CAA..=0x11CB0
            | 0x11CB2..=0x11CB3
            | 0x11CB5..=0x11CB6
            | 0x11D31..=0x11D36
            | 0x11D3A
            | 0x11D3C..=0x11D3D
            | 0x11D3F..=0x11D45
            | 0x11D47
            | 0x11D90..=0x11D91
            | 0x11D95
            | 0x11D97
            | 0x11EF3..=0x11EF4
            | 0x11F00..=0x11F01
            | 0x11F36..=0x11F3A
            | 0x11F40
            | 0x11F42
            | 0x13440
            | 0x13447..=0x13455
            | 0x16AF0..=0x16AF4
            | 0x16B30..=0x16B36
            | 0x16F4F
            | 0x16F8F..=0x16F92
            | 0x16FE4
            | 0x1BC9D..=0x1BC9E
            | 0x1CF00..=0x1CF2D
            | 0x1CF30..=0x1CF46
            | 0x1D167..=0x1D169
            | 0x1D17B..=0x1D182
            | 0x1D185..=0x1D18B
            | 0x1D1AA..=0x1D1AD
            | 0x1D242..=0x1D244
            | 0x1DA00..=0x1DA36
            | 0x1DA3B..=0x1DA6C
            | 0x1DA75
            | 0x1DA84
            | 0x1DA9B..=0x1DA9F
            | 0x1DAA1..=0x1DAAF
            | 0x1E000..=0x1E006
            | 0x1E008..=0x1E018
            | 0x1E01B..=0x1E021
            | 0x1E023..=0x1E024
            | 0x1E026..=0x1E02A
            | 0x1E08F
            | 0x1E130..=0x1E136
            | 0x1E2AE
            | 0x1E2EC..=0x1E2EF
            | 0x1E4EC..=0x1E4EF
            | 0x1E8D0..=0x1E8D6
            | 0x1E944..=0x1E94A
            | 0x1F3FB..=0x1F3FF
            | 0xE0100..=0xE01EF
    )
}

const fn is_nonspacing_or_enclosing_mark(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x0300..=0x036F
            | 0x0483..=0x0489
            | 0x0591..=0x05BD
            | 0x05BF
            | 0x05C1..=0x05C2
            | 0x05C4..=0x05C5
            | 0x05C7
            | 0x0610..=0x061A
            | 0x064B..=0x065F
            | 0x0670
            | 0x06D6..=0x06DC
            | 0x06DF..=0x06E4
            | 0x06E7..=0x06E8
            | 0x06EA..=0x06ED
            | 0x0711
            | 0x0730..=0x074A
            | 0x07A6..=0x07B0
            | 0x07EB..=0x07F3
            | 0x0816..=0x0819
            | 0x081B..=0x0823
            | 0x0825..=0x0827
            | 0x0829..=0x082D
            | 0x0859..=0x085B
            | 0x0898..=0x089F
            | 0x08CA..=0x08E1
            | 0x08E3..=0x0902
            | 0x093A
            | 0x093C
            | 0x0941..=0x0948
            | 0x094D
            | 0x0951..=0x0957
            | 0x0962..=0x0963
            | 0x0981
            | 0x09BC
            | 0x09C1..=0x09C4
            | 0x09CD
            | 0x09E2..=0x09E3
            | 0x09FE
            | 0x0A01..=0x0A02
            | 0x0A3C
            | 0x0A41..=0x0A42
            | 0x0A47..=0x0A48
            | 0x0A4B..=0x0A4D
            | 0x0A51
            | 0x0A70..=0x0A71
            | 0x0A75
            | 0x0A81..=0x0A82
            | 0x0ABC
            | 0x0AC1..=0x0AC5
            | 0x0AC7..=0x0AC8
            | 0x0ACD
            | 0x0AE2..=0x0AE3
            | 0x0AFA..=0x0AFF
            | 0x0B01
            | 0x0B3C
            | 0x0B3F
            | 0x0B41..=0x0B44
            | 0x0B4D
            | 0x0B55..=0x0B56
            | 0x0B62..=0x0B63
            | 0x0B82
            | 0x0BC0
            | 0x0BCD
            | 0x0C00
            | 0x0C04
            | 0x0C3C
            | 0x0C3E..=0x0C40
            | 0x0C46..=0x0C48
            | 0x0C4A..=0x0C4D
            | 0x0C55..=0x0C56
            | 0x0C62..=0x0C63
            | 0x0C81
            | 0x0CBC
            | 0x0CBF
            | 0x0CC6
            | 0x0CCC..=0x0CCD
            | 0x0CE2..=0x0CE3
            | 0x0D00..=0x0D01
            | 0x0D3B..=0x0D3C
            | 0x0D41..=0x0D44
            | 0x0D4D
            | 0x0D62..=0x0D63
            | 0x0D81
            | 0x0DCA
            | 0x0DD2..=0x0DD4
            | 0x0DD6
            | 0x0E31
            | 0x0E34..=0x0E3A
            | 0x0E47..=0x0E4E
            | 0x0EB1
            | 0x0EB4..=0x0EBC
            | 0x0EC8..=0x0ECE
            | 0x0F18..=0x0F19
            | 0x0F35
            | 0x0F37
            | 0x0F39
            | 0x0F71..=0x0F7E
            | 0x0F80..=0x0F84
            | 0x0F86..=0x0F87
            | 0x0F8D..=0x0F97
            | 0x0F99..=0x0FBC
            | 0x0FC6
            | 0x102D..=0x1030
            | 0x1032..=0x1037
            | 0x1039..=0x103A
            | 0x103D..=0x103E
            | 0x1058..=0x1059
            | 0x105E..=0x1060
            | 0x1071..=0x1074
            | 0x1082
            | 0x1085..=0x1086
            | 0x108D
            | 0x109D
            | 0x135D..=0x135F
            | 0x1712..=0x1715
            | 0x1732..=0x1734
            | 0x1752..=0x1753
            | 0x1772..=0x1773
            | 0x17B4..=0x17B5
            | 0x17B7..=0x17BD
            | 0x17C6
            | 0x17C9..=0x17D3
            | 0x17DD
            | 0x180B..=0x180D
            | 0x180F
            | 0x1885..=0x1886
            | 0x18A9
            | 0x1920..=0x1922
            | 0x1927..=0x1928
            | 0x1932
            | 0x1939..=0x193B
            | 0x1A17..=0x1A18
            | 0x1A1B
            | 0x1A56
            | 0x1A58..=0x1A5E
            | 0x1A60
            | 0x1A62
            | 0x1A65..=0x1A6C
            | 0x1A73..=0x1A7C
            | 0x1A7F
            | 0x1AB0..=0x1ACE
            | 0x1B00..=0x1B03
            | 0x1B34
            | 0x1B36..=0x1B3A
            | 0x1B3C
            | 0x1B42
            | 0x1B6B..=0x1B73
            | 0x1B80..=0x1B81
            | 0x1BA2..=0x1BA5
            | 0x1BA8..=0x1BA9
            | 0x1BAB..=0x1BAD
            | 0x1BE6
            | 0x1BE8..=0x1BE9
            | 0x1BED
            | 0x1BEF..=0x1BF1
            | 0x1C2C..=0x1C33
            | 0x1C36..=0x1C37
            | 0x1CD0..=0x1CD2
            | 0x1CD4..=0x1CE0
            | 0x1CE2..=0x1CE8
            | 0x1CED
            | 0x1CF4
            | 0x1CF8..=0x1CF9
            | 0x1DC0..=0x1DFF
            | 0x20D0..=0x20F0
            | 0x2CEF..=0x2CF1
            | 0x2D7F
            | 0x2DE0..=0x2DFF
            | 0x302A..=0x302D
            | 0x3099..=0x309A
            | 0xA66F
            | 0xA674..=0xA67D
            | 0xA69E..=0xA69F
            | 0xA6F0..=0xA6F1
            | 0xA802
            | 0xA806
            | 0xA80B
            | 0xA825..=0xA826
            | 0xA82C
            | 0xA8C4..=0xA8C5
            | 0xA8E0..=0xA8F1
            | 0xA8FF
            | 0xA926..=0xA92D
            | 0xA947..=0xA951
            | 0xA980..=0xA982
            | 0xA9B3
            | 0xA9B6..=0xA9B9
            | 0xA9BC
            | 0xA9E5
            | 0xAA29..=0xAA2E
            | 0xAA31..=0xAA32
            | 0xAA35..=0xAA36
            | 0xAA43
            | 0xAA4C
            | 0xAA7C
            | 0xAAB0
            | 0xAAB2..=0xAAB4
            | 0xAAB7..=0xAAB8
            | 0xAABE..=0xAABF
            | 0xAAC1
            | 0xAAEC..=0xAAED
            | 0xAAF6
            | 0xABE5
            | 0xABE8
            | 0xABED
            | 0xFB1E
            | 0xFE20..=0xFE2F
            | 0x101FD
            | 0x102E0
            | 0x10376..=0x1037A
            | 0x10A01..=0x10A03
            | 0x10A05..=0x10A06
            | 0x10A0C..=0x10A0F
            | 0x10A38..=0x10A3A
            | 0x10A3F
            | 0x10AE5..=0x10AE6
            | 0x10D24..=0x10D27
            | 0x10EAB..=0x10EAC
            | 0x10EFD..=0x10EFF
            | 0x10F46..=0x10F50
            | 0x10F82..=0x10F85
            | 0x11001
            | 0x11038..=0x11046
            | 0x11070
            | 0x11073..=0x11074
            | 0x1107F..=0x11081
            | 0x110B3..=0x110B6
            | 0x110B9..=0x110BA
            | 0x110C2
            | 0x11100..=0x11102
            | 0x11127..=0x1112B
            | 0x1112D..=0x11134
            | 0x11173
            | 0x11180..=0x11181
            | 0x111B6..=0x111BE
            | 0x111C9..=0x111CC
            | 0x111CF
            | 0x1122F..=0x11231
            | 0x11234
            | 0x11236..=0x11237
            | 0x1123E
            | 0x11241
            | 0x112DF
            | 0x112E3..=0x112EA
            | 0x11300..=0x11301
            | 0x1133B..=0x1133C
            | 0x11340
            | 0x11366..=0x1136C
            | 0x11370..=0x11374
            | 0x11438..=0x1143F
            | 0x11442..=0x11444
            | 0x11446
            | 0x1145E
            | 0x114B3..=0x114B8
            | 0x114BA
            | 0x114BF..=0x114C0
            | 0x114C2..=0x114C3
            | 0x115B2..=0x115B5
            | 0x115BC..=0x115BD
            | 0x115BF..=0x115C0
            | 0x115DC..=0x115DD
            | 0x11633..=0x1163A
            | 0x1163D
            | 0x1163F..=0x11640
            | 0x116AB
            | 0x116AD
            | 0x116B0..=0x116B5
            | 0x116B7
            | 0x1171D..=0x1171F
            | 0x11722..=0x11725
            | 0x11727..=0x1172B
            | 0x1182F..=0x11837
            | 0x11839..=0x1183A
            | 0x1193B..=0x1193C
            | 0x1193E
            | 0x11943
            | 0x119D4..=0x119D7
            | 0x119DA..=0x119DB
            | 0x119E0
            | 0x11A01..=0x11A0A
            | 0x11A33..=0x11A38
            | 0x11A3B..=0x11A3E
            | 0x11A47
            | 0x11A51..=0x11A56
            | 0x11A59..=0x11A5B
            | 0x11A8A..=0x11A96
            | 0x11A98..=0x11A99
            | 0x11C30..=0x11C36
            | 0x11C38..=0x11C3D
            | 0x11C3F
            | 0x11C92..=0x11CA7
            | 0x11CAA..=0x11CB0
            | 0x11CB2..=0x11CB3
            | 0x11CB5..=0x11CB6
            | 0x11D31..=0x11D36
            | 0x11D3A
            | 0x11D3C..=0x11D3D
            | 0x11D3F..=0x11D45
            | 0x11D47
            | 0x11D90..=0x11D91
            | 0x11D95
            | 0x11D97
            | 0x11EF3..=0x11EF4
            | 0x11F00..=0x11F01
            | 0x11F36..=0x11F3A
            | 0x11F40
            | 0x11F42
            | 0x13440
            | 0x13447..=0x13455
            | 0x16AF0..=0x16AF4
            | 0x16B30..=0x16B36
            | 0x16F4F
            | 0x16F8F..=0x16F92
            | 0x16FE4
            | 0x1BC9D..=0x1BC9E
            | 0x1CF00..=0x1CF2D
            | 0x1CF30..=0x1CF46
            | 0x1D167..=0x1D169
            | 0x1D17B..=0x1D182
            | 0x1D185..=0x1D18B
            | 0x1D1AA..=0x1D1AD
            | 0x1D242..=0x1D244
            | 0x1DA00..=0x1DA36
            | 0x1DA3B..=0x1DA6C
            | 0x1DA75
            | 0x1DA84
            | 0x1DA9B..=0x1DA9F
            | 0x1DAA1..=0x1DAAF
            | 0x1E000..=0x1E006
            | 0x1E008..=0x1E018
            | 0x1E01B..=0x1E021
            | 0x1E023..=0x1E024
            | 0x1E026..=0x1E02A
            | 0x1E08F
            | 0x1E130..=0x1E136
            | 0x1E2AE
            | 0x1E2EC..=0x1E2EF
            | 0x1E4EC..=0x1E4EF
            | 0x1E8D0..=0x1E8D6
            | 0x1E944..=0x1E94A
            | 0x1F3FB..=0x1F3FF
            | 0xE0100..=0xE01EF
    )
}

const fn grapheme_break_property(codepoint: u32) -> GraphemeBreak {
    if codepoint == 0x200D {
        return GraphemeBreak::Zwj;
    }

    if is_hangul_l(codepoint) {
        return GraphemeBreak::L;
    }
    if is_hangul_v(codepoint) {
        return GraphemeBreak::V;
    }
    if is_hangul_t(codepoint) {
        return GraphemeBreak::T;
    }
    if is_hangul_lv(codepoint) {
        return GraphemeBreak::Lv;
    }
    if is_hangul_lvt(codepoint) {
        return GraphemeBreak::Lvt;
    }

    if is_regional_indicator(codepoint) {
        return GraphemeBreak::RegionalIndicator;
    }

    if is_spacing_mark(codepoint) {
        return GraphemeBreak::SpacingMark;
    }

    if width_zero_in_grapheme(codepoint) {
        return GraphemeBreak::IndicConjunctBreakExtend;
    }

    if is_extended_pictographic(codepoint) {
        return GraphemeBreak::ExtendedPictographic;
    }

    GraphemeBreak::Other
}

const fn is_spacing_mark(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x0903
            | 0x093B
            | 0x093E..=0x0940
            | 0x0949..=0x094C
            | 0x094E..=0x094F
            | 0x0982..=0x0983
            | 0x09BE..=0x09C0
            | 0x09C7..=0x09C8
            | 0x09CB..=0x09CC
            | 0x0A03
            | 0x0A83
            | 0x0ABE..=0x0AC0
            | 0x0AC9
            | 0x0ACB..=0x0ACC
            | 0x0B02..=0x0B03
            | 0x0B3E
            | 0x0B40
            | 0x0B47..=0x0B48
            | 0x0B4B..=0x0B4C
            | 0x0BBE..=0x0BBF
            | 0x0BC1..=0x0BC2
            | 0x0BC6..=0x0BC8
            | 0x0BCA..=0x0BCC
            | 0x0C01..=0x0C03
            | 0x0C41..=0x0C44
            | 0x0C82..=0x0C83
            | 0x0CBE
            | 0x0CC0..=0x0CC4
            | 0x0CC7..=0x0CC8
            | 0x0CCA..=0x0CCB
            | 0x0D02..=0x0D03
            | 0x0D3E..=0x0D40
            | 0x0D46..=0x0D48
            | 0x0D4A..=0x0D4C
            | 0x0D82..=0x0D83
            | 0x0DCF..=0x0DD1
            | 0x0DD8..=0x0DDF
            | 0x0DF2..=0x0DF3
            | 0x0F3E..=0x0F3F
            | 0x0F7F
            | 0x102B..=0x102C
            | 0x1031
            | 0x1038
            | 0x103B..=0x103C
            | 0x1056..=0x1057
            | 0x1062..=0x1064
            | 0x1067..=0x106D
            | 0x1083..=0x1084
            | 0x1087..=0x108C
            | 0x108F
            | 0x109A..=0x109C
            | 0x17B6
            | 0x17BE..=0x17C5
            | 0x17C7..=0x17C8
            | 0x1923..=0x1926
            | 0x1929..=0x192B
            | 0x1930..=0x1931
            | 0x1933..=0x1938
            | 0x1A19..=0x1A1A
            | 0x1A55
            | 0x1A57
            | 0x1A61
            | 0x1A63..=0x1A64
            | 0x1A6D..=0x1A72
            | 0x1B04
            | 0x1B35
            | 0x1B3B
            | 0x1B3D..=0x1B41
            | 0x1B43..=0x1B44
            | 0x1B82
            | 0x1BA1
            | 0x1BA6..=0x1BA7
            | 0x1BAA
            | 0x1BE7
            | 0x1BEA..=0x1BEC
            | 0x1BEE
            | 0x1BF2..=0x1BF3
            | 0x1C24..=0x1C2B
            | 0x1C34..=0x1C35
            | 0x1CE1
            | 0x1CF7
            | 0xA823..=0xA824
            | 0xA827
            | 0xA880..=0xA881
            | 0xA8B4..=0xA8C3
            | 0xA952..=0xA953
            | 0xA983
            | 0xA9B4..=0xA9B5
            | 0xA9BA..=0xA9BB
            | 0xA9BD..=0xA9C0
            | 0xAA2F..=0xAA30
            | 0xAA33..=0xAA34
            | 0xAA4D
            | 0xAA7B
            | 0xAA7D
            | 0xAAEB
            | 0xAAEE..=0xAAEF
            | 0xAAF5
            | 0xABE3..=0xABE4
            | 0xABE6..=0xABE7
            | 0xABE9..=0xABEA
            | 0xABEC
            | 0x11000
            | 0x11002
            | 0x11082
            | 0x110B0..=0x110B2
            | 0x110B7..=0x110B8
            | 0x1112C
            | 0x11182
            | 0x111B3..=0x111B5
            | 0x111BF..=0x111C0
            | 0x1122C..=0x1122E
            | 0x11232..=0x11233
            | 0x11235
            | 0x112E0..=0x112E2
            | 0x11302..=0x11303
            | 0x1133E..=0x1133F
            | 0x11341..=0x11344
            | 0x11347..=0x11348
            | 0x1134B..=0x1134D
            | 0x11357
            | 0x11362..=0x11363
            | 0x11435..=0x11437
            | 0x11440..=0x11441
            | 0x11445
            | 0x114B0..=0x114B2
            | 0x114B9
            | 0x114BB..=0x114BE
            | 0x114C1
            | 0x115AF..=0x115B1
            | 0x115B8..=0x115BB
            | 0x115BE
            | 0x11630..=0x11632
            | 0x1163B..=0x1163C
            | 0x1163E
            | 0x116AC
            | 0x116AE..=0x116AF
            | 0x116B6
            | 0x11720..=0x11721
            | 0x11726
            | 0x1182C..=0x1182E
            | 0x11838
            | 0x11930..=0x11935
            | 0x11937..=0x11938
            | 0x1193D
            | 0x11940
            | 0x11942
            | 0x119D1..=0x119D3
            | 0x119DC..=0x119DF
            | 0x119E4
            | 0x11A39
            | 0x11A57..=0x11A58
            | 0x11A97
            | 0x11C2F
            | 0x11C3E
            | 0x11CA9
            | 0x11CB1
            | 0x11CB4
            | 0x11D8A..=0x11D8E
            | 0x11D93..=0x11D94
            | 0x11D96
            | 0x11EF5..=0x11EF6
            | 0x11F03
            | 0x11F34..=0x11F35
            | 0x11F3E..=0x11F3F
            | 0x11F41
            | 0x16F51..=0x16F87
            | 0x16FF0..=0x16FF1
            | 0x1D165..=0x1D166
    )
}

const fn is_hangul_l(codepoint: u32) -> bool {
    matches!(codepoint, 0x1100..=0x115F | 0xA960..=0xA97C)
}

const fn is_hangul_v(codepoint: u32) -> bool {
    matches!(codepoint, 0x1160..=0x11A7 | 0xD7B0..=0xD7C6)
}

const fn is_hangul_t(codepoint: u32) -> bool {
    matches!(codepoint, 0x11A8..=0x11FF | 0xD7CB..=0xD7FB)
}

const fn is_hangul_lv(codepoint: u32) -> bool {
    codepoint >= 0xAC00 && codepoint <= 0xD7A3 && (codepoint - 0xAC00) % 28 == 0
}

const fn is_hangul_lvt(codepoint: u32) -> bool {
    codepoint >= 0xAC00 && codepoint <= 0xD7A3 && (codepoint - 0xAC00) % 28 != 0
}

const fn is_regional_indicator(codepoint: u32) -> bool {
    matches!(codepoint, 0x1F1E6..=0x1F1FF)
}

const fn emoji_vs_base(codepoint: u32) -> bool {
    matches!(
        codepoint,
        0x0023
            | 0x002A
            | 0x0030..=0x0039
            | 0x00A9
            | 0x00AE
            | 0x203C
            | 0x2049
            | 0x2122
            | 0x2139
            | 0x2194..=0x2199
            | 0x21A9..=0x21AA
            | 0x231A..=0x231B
            | 0x2328
            | 0x23CF
            | 0x23E9..=0x23F3
            | 0x23F8..=0x23FA
            | 0x24C2
            | 0x25AA..=0x25AB
            | 0x25B6
            | 0x25C0
            | 0x25FB..=0x25FE
            | 0x2600..=0x2604
            | 0x260E
            | 0x2611
            | 0x2614..=0x2615
            | 0x2618
            | 0x261D
            | 0x2620
            | 0x2622..=0x2623
            | 0x2626
            | 0x262A
            | 0x262E..=0x262F
            | 0x2638..=0x263A
            | 0x2640
            | 0x2642
            | 0x2648..=0x2653
            | 0x265F..=0x2660
            | 0x2663
            | 0x2665..=0x2666
            | 0x2668
            | 0x267B
            | 0x267E..=0x267F
            | 0x2692..=0x2697
            | 0x2699
            | 0x269B..=0x269C
            | 0x26A0..=0x26A1
            | 0x26A7
            | 0x26AA..=0x26AB
            | 0x26B0..=0x26B1
            | 0x26BD..=0x26BE
            | 0x26C4..=0x26C5
            | 0x26C8
            | 0x26CE..=0x26CF
            | 0x26D1
            | 0x26D3..=0x26D4
            | 0x26E9..=0x26EA
            | 0x26F0..=0x26F5
            | 0x26F7..=0x26FA
            | 0x26FD
            | 0x2702
            | 0x2705
            | 0x2708..=0x270D
            | 0x270F
            | 0x2712
            | 0x2714
            | 0x2716
            | 0x271D
            | 0x2721
            | 0x2728
            | 0x2733..=0x2734
            | 0x2744
            | 0x2747
            | 0x274C
            | 0x274E
            | 0x2753..=0x2755
            | 0x2757
            | 0x2763..=0x2764
            | 0x2795..=0x2797
            | 0x27A1
            | 0x27B0
            | 0x27BF
            | 0x2934..=0x2935
            | 0x2B05..=0x2B07
            | 0x2B1B..=0x2B1C
            | 0x2B50
            | 0x2B55
            | 0x3030
            | 0x303D
            | 0x3297
            | 0x3299
            | 0x1F321
            | 0x1F324..=0x1F32C
            | 0x1F336
            | 0x1F378
            | 0x1F37D
            | 0x1F393
            | 0x1F396..=0x1F397
            | 0x1F399..=0x1F39B
    )
}

const fn is_extended_pictographic(codepoint: u32) -> bool {
    matches!(codepoint, 0x2600..=0x27BF | 0x1F000..=0x1FAFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn props(codepoint: u32) -> Properties {
        get(codepoint)
    }

    fn breaks(previous: u32, current: u32, state: &mut BreakState) -> bool {
        grapheme_break(previous, current, state)
    }

    #[test]
    fn unicode_generated_tables_have_expected_shape() {
        assert!(!tables::STAGE1.is_empty());
        assert!(!tables::STAGE2.is_empty());
        assert!(!tables::STAGE3.is_empty());
        assert_eq!(
            grapheme_table::BREAK_TRANSITIONS.len(),
            BREAK_STATE_COUNT * GRAPHEME_BREAK_COUNT * GRAPHEME_BREAK_COUNT
        );

        for codepoint in [0x0000, 0x00FF, 0x0100, 0x10FFFF] {
            let _ = props(codepoint);
        }
        assert_eq!(props(0x11_0000), fallback());
    }

    #[test]
    fn unicode_properties_ascii_fast_path_width() {
        assert_eq!(
            props(0x41),
            Properties {
                width: 1,
                width_zero_in_grapheme: false,
                grapheme_break: GraphemeBreak::Other,
                emoji_vs_base: false,
            }
        );
    }

    #[test]
    fn unicode_properties_combining_marks_are_extend_zero_width() {
        for codepoint in [0x0301, 0x0308, 0x0303, 0x0302] {
            let props = props(codepoint);
            assert_eq!(props.width, 0);
            assert!(props.width_zero_in_grapheme);
            assert_eq!(
                props.grapheme_break,
                GraphemeBreak::IndicConjunctBreakExtend
            );
            assert!(!props.emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_cjk_width_is_two() {
        for codepoint in [0x65E5, 0x8A9E, 0x6F22] {
            let props = props(codepoint);
            assert_eq!(props.width, 2);
            assert!(!props.width_zero_in_grapheme);
            assert_eq!(props.grapheme_break, GraphemeBreak::Other);
            assert!(!props.emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_live_recipe_emoji_are_wide_pictographs() {
        for codepoint in [0x1F389, 0x1F642] {
            let props = props(codepoint);
            assert_eq!(props.width, 2);
            assert!(!props.width_zero_in_grapheme);
            assert_eq!(props.grapheme_break, GraphemeBreak::ExtendedPictographic);
            assert!(!props.emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_variation_selectors_are_extend_zero_width() {
        for codepoint in [0xFE0E, 0xFE0F] {
            let props = props(codepoint);
            assert_eq!(props.width, 0);
            assert!(props.width_zero_in_grapheme);
            assert_eq!(
                props.grapheme_break,
                GraphemeBreak::IndicConjunctBreakExtend
            );
            assert!(!props.emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_box_symbols_and_pua_have_narrow_width() {
        for codepoint in [0x2500, 0x2502, 0xF101] {
            let props = props(codepoint);
            assert_eq!(props.width, 1);
            assert!(!props.width_zero_in_grapheme);
            assert_eq!(props.grapheme_break, GraphemeBreak::Other);
            assert!(!props.emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_representative_grapheme_break_classes() {
        assert_eq!(props(0x200D).grapheme_break, GraphemeBreak::Zwj);
        assert_eq!(
            props(0x1F1E6).grapheme_break,
            GraphemeBreak::RegionalIndicator
        );
        assert_eq!(props(0x0903).grapheme_break, GraphemeBreak::SpacingMark);
        assert_eq!(props(0x1100).grapheme_break, GraphemeBreak::L);
        assert_eq!(props(0x1161).grapheme_break, GraphemeBreak::V);
        assert_eq!(props(0x1161).width, 0);
        assert!(props(0x1161).width_zero_in_grapheme);
        assert_eq!(props(0x11A8).grapheme_break, GraphemeBreak::T);
        assert_eq!(props(0x11A8).width, 0);
        assert!(props(0x11A8).width_zero_in_grapheme);
        assert_eq!(props(0xAC00).grapheme_break, GraphemeBreak::Lv);
        assert_eq!(props(0xAC01).grapheme_break, GraphemeBreak::Lvt);
        assert_eq!(
            props(0x0915).grapheme_break,
            GraphemeBreak::IndicConjunctBreakConsonant
        );
        assert_eq!(
            props(0x094D).grapheme_break,
            GraphemeBreak::IndicConjunctBreakLinker
        );
    }

    #[test]
    fn unicode_properties_emoji_variation_bases_are_marked() {
        for codepoint in [0x2764, 0x2614, 0x231B, 0x1F327] {
            assert!(props(codepoint).emoji_vs_base);
        }

        for codepoint in [0x1F389, 0x1F642, 0x1F46C] {
            assert!(!props(codepoint).emoji_vs_base);
        }
    }

    #[test]
    fn unicode_properties_out_of_range_uses_ghostty_fallback() {
        assert_eq!(
            props(0x11_0000),
            Properties {
                width: 1,
                width_zero_in_grapheme: true,
                grapheme_break: GraphemeBreak::Other,
                emoji_vs_base: false,
            }
        );
    }

    #[test]
    fn unicode_grapheme_breaks_emoji_modifier_and_zwj_family() {
        let mut state = BreakState::default();
        assert!(!breaks(0x261D, 0x1F3FF, &mut state));

        let mut state = BreakState::default();
        assert!(breaks(0x22, 0x1F3FF, &mut state));

        let family = [0x1F469, 0x200D, 0x1F469, 0x200D, 0x1F467, 0x200D, 0x1F466];
        let mut state = BreakState::default();
        for pair in family.windows(2) {
            assert!(
                !breaks(pair[0], pair[1], &mut state),
                "unexpected break between U+{:04X} and U+{:04X}",
                pair[0],
                pair[1]
            );
        }
        assert!(breaks(*family.last().unwrap(), u32::from(b'_'), &mut state));
    }

    #[test]
    fn unicode_grapheme_breaks_regional_indicator_pairs() {
        let mut state = BreakState::default();
        assert!(!breaks(0x1F1E8, 0x1F1ED, &mut state));
        assert!(breaks(0x1F1ED, 0x1F1E6, &mut state));
        assert!(!breaks(0x1F1E6, 0x1F1E7, &mut state));
    }

    #[test]
    fn unicode_grapheme_breaks_hangul_clusters() {
        let mut state = BreakState::default();
        assert!(!breaks(0x1100, 0x1161, &mut state));
        assert!(!breaks(0x1161, 0x11A8, &mut state));
        assert!(breaks(0x11A8, u32::from(b'a'), &mut state));

        let mut state = BreakState::default();
        assert!(!breaks(0xAC00, 0x11A8, &mut state));
        assert!(breaks(0xAC01, u32::from(b'a'), &mut state));
    }

    #[test]
    fn unicode_grapheme_breaks_indic_conjunct_sequence() {
        let mut state = BreakState::default();
        assert!(!breaks(0x0915, 0x094D, &mut state));
        assert!(!breaks(0x094D, 0x0915, &mut state));
        assert!(breaks(0x0915, u32::from(b'a'), &mut state));
    }
}
