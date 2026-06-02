//! Kitty protocol state.

pub(crate) mod graphics_command;
pub(crate) mod graphics_exec;
pub(crate) mod graphics_image;
pub(crate) mod graphics_storage;
pub(crate) mod graphics_unicode;

use super::color;

const KEY_FLAG_STACK_LEN: usize = 8;
const COLOR_SPECIAL_COUNT: usize = 8;
pub(super) const COLOR_REQUEST_CAPACITY: usize = (u8::MAX as usize + COLOR_SPECIAL_COUNT) * 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColorSpecial {
    Foreground,
    Background,
    SelectionForeground,
    SelectionBackground,
    Cursor,
    CursorText,
    VisualBell,
    SecondTransparentBackground,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColorKind {
    Palette(u8),
    Special(ColorSpecial),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColorRequest {
    Query(ColorKind),
    Set { key: ColorKind, rgb: color::Rgb },
    Reset(ColorKind),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ColorRequests {
    items: [Option<ColorRequest>; COLOR_REQUEST_CAPACITY],
    len: usize,
}

impl ColorSpecial {
    pub(super) fn parse(bytes: &[u8]) -> Option<Self> {
        match bytes {
            b"foreground" => Some(Self::Foreground),
            b"background" => Some(Self::Background),
            b"selection_foreground" => Some(Self::SelectionForeground),
            b"selection_background" => Some(Self::SelectionBackground),
            b"cursor" => Some(Self::Cursor),
            b"cursor_text" => Some(Self::CursorText),
            b"visual_bell" => Some(Self::VisualBell),
            b"second_transparent_background" => Some(Self::SecondTransparentBackground),
            _ => None,
        }
    }

    pub(super) const fn name(self) -> &'static str {
        match self {
            Self::Foreground => "foreground",
            Self::Background => "background",
            Self::SelectionForeground => "selection_foreground",
            Self::SelectionBackground => "selection_background",
            Self::Cursor => "cursor",
            Self::CursorText => "cursor_text",
            Self::VisualBell => "visual_bell",
            Self::SecondTransparentBackground => "second_transparent_background",
        }
    }
}

impl ColorKind {
    pub(super) fn parse(bytes: &[u8]) -> Option<Self> {
        if let Some(special) = ColorSpecial::parse(bytes) {
            return Some(Self::Special(special));
        }
        if !bytes.iter().all(u8::is_ascii_digit) {
            return None;
        }
        let text = std::str::from_utf8(bytes).ok()?;
        text.parse::<u8>().ok().map(Self::Palette)
    }

    pub(super) fn append_to_string(self, output: &mut String) {
        match self {
            Self::Palette(index) => output.push_str(&index.to_string()),
            Self::Special(special) => output.push_str(special.name()),
        }
    }
}

impl ColorRequests {
    pub(super) const fn new() -> Self {
        Self {
            items: [None; COLOR_REQUEST_CAPACITY],
            len: 0,
        }
    }

    pub(super) fn push(&mut self, request: ColorRequest) -> Result<(), ()> {
        let Some(slot) = self.items.get_mut(self.len) else {
            return Err(());
        };
        *slot = Some(request);
        self.len += 1;
        Ok(())
    }

    pub(super) const fn len(self) -> usize {
        self.len
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = ColorRequest> + '_ {
        self.items[..self.len]
            .iter()
            .map(|request| request.expect("color request slots below len must be initialized"))
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct KeyFlags {
    pub(crate) disambiguate: bool,
    pub(crate) report_events: bool,
    pub(crate) report_alternates: bool,
    pub(crate) report_all: bool,
    pub(crate) report_associated: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum KeySetMode {
    #[default]
    Set,
    Or,
    Not,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KeyFlagStack {
    flags: [KeyFlags; KEY_FLAG_STACK_LEN],
    idx: usize,
}

impl KeyFlags {
    pub(crate) const DISABLED: Self = Self {
        disambiguate: false,
        report_events: false,
        report_alternates: false,
        report_all: false,
        report_associated: false,
    };

    pub(crate) const TRUE: Self = Self {
        disambiguate: true,
        report_events: true,
        report_alternates: true,
        report_all: true,
        report_associated: true,
    };

    pub(crate) const fn int(self) -> u8 {
        (self.disambiguate as u8)
            | ((self.report_events as u8) << 1)
            | ((self.report_alternates as u8) << 2)
            | ((self.report_all as u8) << 3)
            | ((self.report_associated as u8) << 4)
    }

    pub(crate) const fn is_disabled(self) -> bool {
        self.int() == Self::DISABLED.int()
    }

    pub(crate) const fn from_protocol_int(value: u16) -> Option<Self> {
        if value <= Self::TRUE.int() as u16 {
            Some(Self::from_int(value as u8))
        } else {
            None
        }
    }

    pub(crate) const fn from_raw_int(value: u8) -> Option<Self> {
        if value <= Self::TRUE.int() {
            Some(Self::from_int(value))
        } else {
            None
        }
    }

    const fn from_int(value: u8) -> Self {
        Self {
            disambiguate: value & 0b00001 != 0,
            report_events: value & 0b00010 != 0,
            report_alternates: value & 0b00100 != 0,
            report_all: value & 0b01000 != 0,
            report_associated: value & 0b10000 != 0,
        }
    }
}

impl KeyFlagStack {
    pub(super) const fn current(self) -> KeyFlags {
        self.flags[self.idx]
    }

    pub(super) fn set(&mut self, mode: KeySetMode, flags: KeyFlags) {
        let current = self.current();
        self.flags[self.idx] = match mode {
            KeySetMode::Set => flags,
            KeySetMode::Or => KeyFlags::from_int(current.int() | flags.int()),
            KeySetMode::Not => KeyFlags::from_int(current.int() & !flags.int()),
        };
    }

    pub(super) fn push(&mut self, flags: KeyFlags) {
        self.idx = (self.idx + 1) % self.flags.len();
        self.flags[self.idx] = flags;
    }

    pub(super) fn pop(&mut self, n: usize) {
        if n >= self.flags.len() {
            *self = Self::default();
            return;
        }

        for _ in 0..n {
            self.flags[self.idx] = KeyFlags::DISABLED;
            self.idx = (self.idx + self.flags.len() - 1) % self.flags.len();
        }
    }
}

impl Default for KeyFlagStack {
    fn default() -> Self {
        Self {
            flags: [KeyFlags::DISABLED; KEY_FLAG_STACK_LEN],
            idx: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_kind_parse_and_format() {
        assert_eq!(
            ColorKind::parse(b"foreground"),
            Some(ColorKind::Special(ColorSpecial::Foreground))
        );
        assert_eq!(ColorKind::parse(b"Foreground"), None);
        assert_eq!(ColorKind::parse(b"255"), Some(ColorKind::Palette(255)));
        assert_eq!(ColorKind::parse(b"256"), None);
        assert_eq!(ColorKind::parse(b"+1"), None);

        let mut output = String::new();
        ColorKind::Special(ColorSpecial::SelectionBackground).append_to_string(&mut output);
        assert_eq!(output, "selection_background");
        output.clear();
        ColorKind::Palette(42).append_to_string(&mut output);
        assert_eq!(output, "42");
    }

    #[test]
    fn color_request_capacity_matches_ghostty_expression() {
        assert_eq!(COLOR_REQUEST_CAPACITY, 526);
    }

    fn flags(value: u8) -> KeyFlags {
        KeyFlags::from_int(value)
    }

    #[test]
    fn key_flags_pack_bits_in_upstream_order() {
        assert_eq!(
            KeyFlags {
                disambiguate: true,
                ..KeyFlags::DISABLED
            }
            .int(),
            0b00001
        );
        assert_eq!(
            KeyFlags {
                report_events: true,
                ..KeyFlags::DISABLED
            }
            .int(),
            0b00010
        );
        assert_eq!(
            KeyFlags {
                report_alternates: true,
                ..KeyFlags::DISABLED
            }
            .int(),
            0b00100
        );
        assert_eq!(
            KeyFlags {
                report_all: true,
                ..KeyFlags::DISABLED
            }
            .int(),
            0b01000
        );
        assert_eq!(
            KeyFlags {
                report_associated: true,
                ..KeyFlags::DISABLED
            }
            .int(),
            0b10000
        );
        assert_eq!(KeyFlags::TRUE.int(), 0b11111);
    }

    #[test]
    fn key_flag_stack_defaults_to_disabled() {
        let stack = KeyFlagStack::default();

        assert_eq!(stack.current(), KeyFlags::DISABLED);
        assert!(stack.current().is_disabled());
    }

    #[test]
    fn key_flag_stack_set_replaces_current_flags() {
        let mut stack = KeyFlagStack::default();

        stack.set(KeySetMode::Set, flags(0b10001));

        assert_eq!(stack.current().int(), 0b10001);
    }

    #[test]
    fn key_flag_stack_or_adds_flags_to_current() {
        let mut stack = KeyFlagStack::default();
        stack.set(KeySetMode::Set, flags(0b00001));

        stack.set(KeySetMode::Or, flags(0b01010));

        assert_eq!(stack.current().int(), 0b01011);
    }

    #[test]
    fn key_flag_stack_not_clears_flags_from_current() {
        let mut stack = KeyFlagStack::default();
        stack.set(KeySetMode::Set, KeyFlags::TRUE);

        stack.set(KeySetMode::Not, flags(0b01010));

        assert_eq!(stack.current().int(), 0b10101);
    }

    #[test]
    fn key_flag_stack_push_and_pop_restore_previous_flags() {
        let mut stack = KeyFlagStack::default();
        stack.set(KeySetMode::Set, flags(0b00001));
        stack.push(flags(0b00010));

        assert_eq!(stack.current().int(), 0b00010);

        stack.pop(1);

        assert_eq!(stack.current().int(), 0b00001);
    }

    #[test]
    fn key_flag_stack_pop_exact_length_resets_to_disabled() {
        let mut stack = KeyFlagStack::default();
        for value in 1..=8 {
            stack.push(flags(value));
        }

        stack.pop(8);

        assert_eq!(stack.current(), KeyFlags::DISABLED);
        assert_eq!(stack, KeyFlagStack::default());
    }

    #[test]
    fn key_flag_stack_pop_larger_than_length_resets_to_disabled() {
        let mut stack = KeyFlagStack::default();
        stack.push(flags(0b11111));

        stack.pop(100);

        assert_eq!(stack.current(), KeyFlags::DISABLED);
        assert_eq!(stack, KeyFlagStack::default());
    }

    #[test]
    fn key_flag_stack_wraps_and_evicts_oldest_slot() {
        let mut stack = KeyFlagStack::default();
        for value in 1..=9 {
            stack.push(flags(value));
        }

        assert_eq!(stack.current().int(), 9);

        for expected in (3..=9).rev() {
            stack.pop(1);
            assert_eq!(stack.current().int(), expected - 1);
        }

        stack.pop(1);
        assert_eq!(stack.current(), KeyFlags::DISABLED);
    }
}
