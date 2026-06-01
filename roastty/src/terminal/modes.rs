//! Terminal mode state.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(usize)]
pub(super) enum Mode {
    DisableKeyboard,
    Insert,
    SendReceiveMode,
    Linefeed,
    CursorKeys,
    Column132,
    SlowScroll,
    ReverseColors,
    Origin,
    Wraparound,
    Autorepeat,
    MouseEventX10,
    CursorBlinking,
    CursorVisible,
    EnableMode3,
    ReverseWrap,
    AltScreenLegacy,
    KeypadKeys,
    BackarrowKeyMode,
    EnableLeftAndRightMargin,
    MouseEventNormal,
    MouseEventButton,
    MouseEventAny,
    FocusEvent,
    MouseFormatUtf8,
    MouseFormatSgr,
    MouseAlternateScroll,
    MouseFormatUrxvt,
    MouseFormatSgrPixels,
    IgnoreKeypadWithNumlock,
    AltEscPrefix,
    AltSendsEscape,
    ReverseWrapExtended,
    AltScreen,
    SaveCursor,
    AltScreenSaveCursorClearEnter,
    BracketedPaste,
    SynchronizedOutput,
    GraphemeCluster,
    ReportColorScheme,
    InBandSizeReports,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ModeTag {
    pub(super) value: u16,
    pub(super) ansi: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ModeEntry {
    pub(super) mode: Mode,
    pub(super) name: &'static str,
    pub(super) value: u16,
    pub(super) ansi: bool,
    pub(super) default: bool,
    pub(super) disabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ModeState {
    values: [bool; MODE_COUNT],
    saved: [bool; MODE_COUNT],
    default: [bool; MODE_COUNT],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Report {
    tag: ModeTag,
    state: ReportState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ReportState {
    NotRecognized = 0,
    Set = 1,
    Reset = 2,
    PermanentlySet = 3,
    PermanentlyReset = 4,
}

const MODE_COUNT: usize = Mode::InBandSizeReports as usize + 1;

const ENTRIES: [ModeEntry; MODE_COUNT] = [
    ModeEntry::ansi(Mode::DisableKeyboard, "disable_keyboard", 2, false),
    ModeEntry::ansi(Mode::Insert, "insert", 4, false),
    ModeEntry::ansi(Mode::SendReceiveMode, "send_receive_mode", 12, true),
    ModeEntry::ansi(Mode::Linefeed, "linefeed", 20, false),
    ModeEntry::dec(Mode::CursorKeys, "cursor_keys", 1, false),
    ModeEntry::dec(Mode::Column132, "132_column", 3, false),
    ModeEntry::dec(Mode::SlowScroll, "slow_scroll", 4, false),
    ModeEntry::dec(Mode::ReverseColors, "reverse_colors", 5, false),
    ModeEntry::dec(Mode::Origin, "origin", 6, false),
    ModeEntry::dec(Mode::Wraparound, "wraparound", 7, true),
    ModeEntry::dec(Mode::Autorepeat, "autorepeat", 8, false),
    ModeEntry::dec(Mode::MouseEventX10, "mouse_event_x10", 9, false),
    ModeEntry::dec(Mode::CursorBlinking, "cursor_blinking", 12, false),
    ModeEntry::dec(Mode::CursorVisible, "cursor_visible", 25, true),
    ModeEntry::dec(Mode::EnableMode3, "enable_mode_3", 40, false),
    ModeEntry::dec(Mode::ReverseWrap, "reverse_wrap", 45, false),
    ModeEntry::dec(Mode::AltScreenLegacy, "alt_screen_legacy", 47, false),
    ModeEntry::dec(Mode::KeypadKeys, "keypad_keys", 66, false),
    ModeEntry::dec(Mode::BackarrowKeyMode, "backarrow_key_mode", 67, false),
    ModeEntry::dec(
        Mode::EnableLeftAndRightMargin,
        "enable_left_and_right_margin",
        69,
        false,
    ),
    ModeEntry::dec(Mode::MouseEventNormal, "mouse_event_normal", 1000, false),
    ModeEntry::dec(Mode::MouseEventButton, "mouse_event_button", 1002, false),
    ModeEntry::dec(Mode::MouseEventAny, "mouse_event_any", 1003, false),
    ModeEntry::dec(Mode::FocusEvent, "focus_event", 1004, false),
    ModeEntry::dec(Mode::MouseFormatUtf8, "mouse_format_utf8", 1005, false),
    ModeEntry::dec(Mode::MouseFormatSgr, "mouse_format_sgr", 1006, false),
    ModeEntry::dec(
        Mode::MouseAlternateScroll,
        "mouse_alternate_scroll",
        1007,
        true,
    ),
    ModeEntry::dec(Mode::MouseFormatUrxvt, "mouse_format_urxvt", 1015, false),
    ModeEntry::dec(
        Mode::MouseFormatSgrPixels,
        "mouse_format_sgr_pixels",
        1016,
        false,
    ),
    ModeEntry::dec(
        Mode::IgnoreKeypadWithNumlock,
        "ignore_keypad_with_numlock",
        1035,
        true,
    ),
    ModeEntry::dec(Mode::AltEscPrefix, "alt_esc_prefix", 1036, true),
    ModeEntry::dec(Mode::AltSendsEscape, "alt_sends_escape", 1039, false),
    ModeEntry::dec(
        Mode::ReverseWrapExtended,
        "reverse_wrap_extended",
        1045,
        false,
    ),
    ModeEntry::dec(Mode::AltScreen, "alt_screen", 1047, false),
    ModeEntry::dec(Mode::SaveCursor, "save_cursor", 1048, false),
    ModeEntry::dec(
        Mode::AltScreenSaveCursorClearEnter,
        "alt_screen_save_cursor_clear_enter",
        1049,
        false,
    ),
    ModeEntry::dec(Mode::BracketedPaste, "bracketed_paste", 2004, false),
    ModeEntry::dec(Mode::SynchronizedOutput, "synchronized_output", 2026, false),
    ModeEntry::dec(Mode::GraphemeCluster, "grapheme_cluster", 2027, false),
    ModeEntry::dec(Mode::ReportColorScheme, "report_color_scheme", 2031, false),
    ModeEntry::dec(Mode::InBandSizeReports, "in_band_size_reports", 2048, false),
];

impl ModeEntry {
    const fn ansi(mode: Mode, name: &'static str, value: u16, default: bool) -> Self {
        Self {
            mode,
            name,
            value,
            ansi: true,
            default,
            disabled: false,
        }
    }

    const fn dec(mode: Mode, name: &'static str, value: u16, default: bool) -> Self {
        Self {
            mode,
            name,
            value,
            ansi: false,
            default,
            disabled: false,
        }
    }

    pub(super) const fn tag(self) -> ModeTag {
        ModeTag {
            value: self.value,
            ansi: self.ansi,
        }
    }
}

impl Mode {
    const fn index(self) -> usize {
        self as usize
    }
}

impl ModeTag {
    pub(super) const fn new(value: u16, ansi: bool) -> Self {
        Self { value, ansi }
    }

    pub(super) fn from_mode(mode: Mode) -> Self {
        entry_for_mode(mode).tag()
    }
}

impl Default for ModeState {
    fn default() -> Self {
        let default = default_values();
        Self {
            values: default,
            saved: [false; MODE_COUNT],
            default,
        }
    }
}

impl ModeState {
    pub(super) fn reset(&mut self) {
        self.values = self.default;
        self.saved = [false; MODE_COUNT];
    }

    pub(super) fn set(&mut self, mode: Mode, value: bool) {
        self.values[mode.index()] = value;
    }

    pub(super) fn get(&self, mode: Mode) -> bool {
        self.values[mode.index()]
    }

    pub(super) fn default_for(&self, mode: Mode) -> bool {
        self.default[mode.index()]
    }

    pub(super) fn save(&mut self, mode: Mode) {
        self.saved[mode.index()] = self.values[mode.index()];
    }

    pub(super) fn restore(&mut self, mode: Mode) -> bool {
        self.values[mode.index()] = self.saved[mode.index()];
        self.values[mode.index()]
    }

    pub(super) fn get_report(&self, tag: ModeTag) -> Report {
        let Some(mode) = mode_from_int(tag.value, tag.ansi) else {
            return Report {
                tag,
                state: ReportState::NotRecognized,
            };
        };

        Report {
            tag,
            state: if self.get(mode) {
                ReportState::Set
            } else {
                ReportState::Reset
            },
        }
    }
}

impl Report {
    pub(super) const fn new(tag: ModeTag, state: ReportState) -> Self {
        Self { tag, state }
    }

    pub(super) fn encode_vt(self) -> String {
        format!(
            "\x1b[{}{};{}$y",
            if self.tag.ansi { "" } else { "?" },
            self.tag.value,
            self.state as u8
        )
    }
}

pub(super) fn entries() -> &'static [ModeEntry] {
    &ENTRIES
}

pub(super) fn entry_for_mode(mode: Mode) -> &'static ModeEntry {
    &ENTRIES[mode.index()]
}

pub(super) fn mode_from_int(value: u16, ansi: bool) -> Option<Mode> {
    ENTRIES
        .iter()
        .find(|entry| !entry.disabled && entry.value == value && entry.ansi == ansi)
        .map(|entry| entry.mode)
}

fn default_values() -> [bool; MODE_COUNT] {
    let mut values = [false; MODE_COUNT];
    for entry in ENTRIES {
        values[entry.mode.index()] = entry.default;
    }
    values
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_from_int_finds_known_ansi_and_dec_modes() {
        assert_eq!(mode_from_int(4, true), Some(Mode::Insert));
        assert_eq!(mode_from_int(9, false), Some(Mode::MouseEventX10));
    }

    #[test]
    fn mode_from_int_rejects_unknown_or_wrong_family_modes() {
        assert_eq!(mode_from_int(9, true), None);
        assert_eq!(mode_from_int(14, true), None);
        assert_eq!(mode_from_int(9999, false), None);
    }

    #[test]
    fn current_upstream_table_has_no_disabled_entries() {
        assert_eq!(entries().iter().filter(|entry| entry.disabled).count(), 0);
    }

    #[test]
    fn mode_state_preserves_upstream_defaults() {
        let state = ModeState::default();

        assert!(state.get(Mode::SendReceiveMode));
        assert!(state.get(Mode::Wraparound));
        assert!(state.get(Mode::CursorVisible));
        assert!(state.get(Mode::MouseAlternateScroll));
        assert!(state.get(Mode::IgnoreKeypadWithNumlock));
        assert!(state.get(Mode::AltEscPrefix));
        assert!(!state.get(Mode::Insert));
        assert!(!state.get(Mode::BracketedPaste));
    }

    #[test]
    fn mode_state_set_get_save_restore_and_reset() {
        let mut state = ModeState::default();

        assert!(!state.get(Mode::CursorKeys));
        state.set(Mode::CursorKeys, true);
        assert!(state.get(Mode::CursorKeys));
        state.save(Mode::CursorKeys);
        state.set(Mode::CursorKeys, false);
        assert!(!state.get(Mode::CursorKeys));
        assert!(state.restore(Mode::CursorKeys));
        assert!(state.get(Mode::CursorKeys));
        state.reset();
        assert!(!state.get(Mode::CursorKeys));
        assert!(state.get(Mode::Wraparound));
    }

    #[test]
    fn mode_state_get_report_known_modes_and_unknown_mode() {
        let mut state = ModeState::default();

        let reset = state.get_report(ModeTag::new(1, false));
        assert_eq!(reset.state, ReportState::Reset);
        assert_eq!(reset.tag, ModeTag::new(1, false));

        state.set(Mode::CursorKeys, true);
        let set = state.get_report(ModeTag::new(1, false));
        assert_eq!(set.state, ReportState::Set);

        state.set(Mode::Insert, true);
        let ansi = state.get_report(ModeTag::new(4, true));
        assert_eq!(ansi.state, ReportState::Set);
        assert_eq!(ansi.tag, ModeTag::new(4, true));

        let unknown = state.get_report(ModeTag::new(9999, false));
        assert_eq!(unknown.state, ReportState::NotRecognized);
    }

    #[test]
    fn report_encode_vt_matches_upstream_sequences() {
        assert_eq!(
            Report::new(ModeTag::new(1, false), ReportState::Set).encode_vt(),
            "\x1b[?1;1$y"
        );
        assert_eq!(
            Report::new(ModeTag::new(1, false), ReportState::Reset).encode_vt(),
            "\x1b[?1;2$y"
        );
        assert_eq!(
            Report::new(ModeTag::new(4, true), ReportState::Set).encode_vt(),
            "\x1b[4;1$y"
        );
        assert_eq!(
            Report::new(ModeTag::new(9999, false), ReportState::NotRecognized).encode_vt(),
            "\x1b[?9999;0$y"
        );
    }
}
