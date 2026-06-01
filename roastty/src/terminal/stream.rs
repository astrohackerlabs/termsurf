//! Terminal byte stream decoding.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Print { cp: char },
    LineFeed,
    CarriageReturn,
    Backspace,
    HorizontalTab,
    TabSet,
    TabClearCurrent,
    TabClearAll,
    TabReset,
    Index,
    NextLine,
    CursorUp { count: u16 },
    CursorDown { count: u16 },
    CursorRight { count: u16 },
    CursorLeft { count: u16 },
}

pub(super) trait Handler {
    type Error;

    fn vt(&mut self, action: Action) -> Result<(), Self::Error>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    Ground,
    Escape,
    EscapeInvalidIntermediate,
    Csi(CsiState),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stream {
    utf8: Utf8Decoder,
    escape: EscapeState,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Utf8Decoder {
    bytes: [u8; 4],
    len: usize,
    expected: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DecodeResult {
    cp: Option<char>,
    consumed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CsiState {
    private: Option<u8>,
    param: Option<u16>,
    invalid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsiDispatch {
    None,
    One(Action),
    Two(Action, Action),
}

impl Stream {
    pub(super) const fn init() -> Self {
        Self {
            utf8: Utf8Decoder::new(),
            escape: EscapeState::Ground,
        }
    }

    pub(super) fn next_slice<H: Handler>(
        &mut self,
        input: &[u8],
        handler: &mut H,
    ) -> Result<(), H::Error> {
        for &byte in input {
            self.next(byte, handler)?;
        }
        Ok(())
    }

    fn next<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        match self.escape {
            EscapeState::Ground => self.next_ground(byte, handler),
            EscapeState::Escape => self.next_escape(byte, handler),
            EscapeState::EscapeInvalidIntermediate => {
                self.next_escape_invalid_intermediate(byte);
                Ok(())
            }
            EscapeState::Csi(state) => self.next_csi(byte, state, handler),
        }
    }

    fn next_ground<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        if self.utf8.is_pending() {
            let result = self.utf8.next(byte);
            if let Some(cp) = result.cp {
                handler.vt(Action::Print { cp })?;
            }
            if !result.consumed {
                self.next_ground(byte, handler)?;
            }
            return Ok(());
        }

        match byte {
            0x1b => {
                self.escape = EscapeState::Escape;
            }
            0x08 => handler.vt(Action::Backspace)?,
            b'\t' => handler.vt(Action::HorizontalTab)?,
            b'\n' | 0x0b | 0x0c => handler.vt(Action::LineFeed)?,
            b'\r' => handler.vt(Action::CarriageReturn)?,
            0x00..=0x07 | 0x0e..=0x1a | 0x1c..=0x1f | 0x7f => {}
            _ => self.next_utf8(byte, handler)?,
        }
        Ok(())
    }

    fn next_escape<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        match byte {
            0x20..=0x2f => {
                self.escape = EscapeState::EscapeInvalidIntermediate;
                Ok(())
            }
            b'[' => {
                self.escape = EscapeState::Csi(CsiState::new());
                Ok(())
            }
            b'D' => {
                self.escape = EscapeState::Ground;
                handler.vt(Action::Index)
            }
            b'E' => {
                self.escape = EscapeState::Ground;
                handler.vt(Action::NextLine)
            }
            b'H' => {
                self.escape = EscapeState::Ground;
                handler.vt(Action::TabSet)
            }
            _ => {
                self.escape = EscapeState::Ground;
                Ok(())
            }
        }
    }

    fn next_escape_invalid_intermediate(&mut self, byte: u8) {
        if (0x30..=0x7e).contains(&byte) {
            self.escape = EscapeState::Ground;
        }
    }

    fn next_csi<H: Handler>(
        &mut self,
        byte: u8,
        mut state: CsiState,
        handler: &mut H,
    ) -> Result<(), H::Error> {
        if (0x40..=0x7e).contains(&byte) {
            self.escape = EscapeState::Ground;
            return state.dispatch(byte).handle(handler);
        }

        state.push(byte);
        self.escape = EscapeState::Csi(state);
        Ok(())
    }

    fn next_utf8<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        let result = self.utf8.next(byte);
        if let Some(cp) = result.cp {
            handler.vt(Action::Print { cp })?;
        }
        debug_assert!(result.consumed);
        Ok(())
    }
}

impl CsiState {
    const fn new() -> Self {
        Self {
            private: None,
            param: None,
            invalid: false,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.invalid {
            return;
        }

        match byte {
            b'?' if self.private.is_none() && self.param.is_none() => {
                self.private = Some(byte);
            }
            b'0'..=b'9' => {
                let digit = u16::from(byte - b'0');
                self.param = Some(
                    self.param
                        .unwrap_or(0)
                        .saturating_mul(10)
                        .saturating_add(digit),
                );
            }
            _ => self.invalid = true,
        }
    }

    fn dispatch(&self, final_byte: u8) -> CsiDispatch {
        if let Some(dispatch) = self.line_dispatch(final_byte) {
            return dispatch;
        }

        if let Some(action) = self.cursor_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if final_byte == b'W' {
            return self
                .tab_action()
                .map(CsiDispatch::One)
                .unwrap_or(CsiDispatch::None);
        }

        CsiDispatch::None
    }

    fn movement_count(&self) -> Option<u16> {
        if self.invalid || self.private.is_some() {
            return None;
        }

        Some(self.param.unwrap_or(1).max(1))
    }

    fn cursor_action(&self, final_byte: u8) -> Option<Action> {
        let count = self.movement_count()?;
        match final_byte {
            b'A' | b'k' => Some(Action::CursorUp { count }),
            b'B' => Some(Action::CursorDown { count }),
            b'C' | b'a' => Some(Action::CursorRight { count }),
            b'D' | b'j' => Some(Action::CursorLeft { count }),
            _ => None,
        }
    }

    fn line_dispatch(&self, final_byte: u8) -> Option<CsiDispatch> {
        let count = self.movement_count()?;
        match final_byte {
            b'E' => Some(CsiDispatch::Two(
                Action::CursorDown { count },
                Action::CarriageReturn,
            )),
            b'F' => Some(CsiDispatch::Two(
                Action::CursorUp { count },
                Action::CarriageReturn,
            )),
            _ => None,
        }
    }

    fn tab_action(&self) -> Option<Action> {
        if self.invalid {
            return None;
        }

        match (self.private, self.param) {
            (None, None | Some(0)) => Some(Action::TabSet),
            (None, Some(2)) => Some(Action::TabClearCurrent),
            (None, Some(5)) => Some(Action::TabClearAll),
            (Some(b'?'), Some(5)) => Some(Action::TabReset),
            (Some(_), _) | (None, Some(_)) => None,
        }
    }
}

impl CsiDispatch {
    fn handle<H: Handler>(self, handler: &mut H) -> Result<(), H::Error> {
        match self {
            Self::None => Ok(()),
            Self::One(action) => handler.vt(action),
            Self::Two(first, second) => {
                handler.vt(first)?;
                handler.vt(second)
            }
        }
    }
}

impl Utf8Decoder {
    const fn new() -> Self {
        Self {
            bytes: [0; 4],
            len: 0,
            expected: 0,
        }
    }

    fn reset(&mut self) {
        self.bytes = [0; 4];
        self.len = 0;
        self.expected = 0;
    }

    fn is_pending(&self) -> bool {
        self.len > 0
    }

    fn next(&mut self, byte: u8) -> DecodeResult {
        if self.len == 0 {
            return self.start(byte);
        }

        if !self.accepts_continuation(byte) {
            self.reset();
            return DecodeResult {
                cp: Some(char::REPLACEMENT_CHARACTER),
                consumed: false,
            };
        }

        self.bytes[self.len] = byte;
        self.len += 1;

        if self.len < self.expected {
            return DecodeResult {
                cp: None,
                consumed: true,
            };
        }

        let cp = std::str::from_utf8(&self.bytes[..self.expected])
            .ok()
            .and_then(|value| value.chars().next())
            .unwrap_or(char::REPLACEMENT_CHARACTER);
        self.reset();
        DecodeResult {
            cp: Some(cp),
            consumed: true,
        }
    }

    fn start(&mut self, byte: u8) -> DecodeResult {
        match byte {
            0x00..=0x7f => DecodeResult {
                cp: Some(char::from(byte)),
                consumed: true,
            },
            0xc2..=0xdf => self.start_sequence(byte, 2),
            0xe0..=0xef => self.start_sequence(byte, 3),
            0xf0..=0xf4 => self.start_sequence(byte, 4),
            _ => DecodeResult {
                cp: Some(char::REPLACEMENT_CHARACTER),
                consumed: true,
            },
        }
    }

    fn start_sequence(&mut self, byte: u8, expected: usize) -> DecodeResult {
        self.bytes[0] = byte;
        self.len = 1;
        self.expected = expected;
        DecodeResult {
            cp: None,
            consumed: true,
        }
    }

    fn accepts_continuation(&self, byte: u8) -> bool {
        match (self.bytes[0], self.len) {
            (0xe0, 1) => (0xa0..=0xbf).contains(&byte),
            (0xed, 1) => (0x80..=0x9f).contains(&byte),
            (0xf0, 1) => (0x90..=0xbf).contains(&byte),
            (0xf4, 1) => (0x80..=0x8f).contains(&byte),
            _ => (0x80..=0xbf).contains(&byte),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Default)]
    struct RecordingHandler {
        actions: Vec<Action>,
    }

    impl Handler for RecordingHandler {
        type Error = ();

        fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
            self.actions.push(action);
            Ok(())
        }
    }

    fn print_chars(handler: &RecordingHandler) -> Vec<char> {
        handler
            .actions
            .iter()
            .filter_map(|action| match action {
                Action::Print { cp } => Some(*cp),
                Action::LineFeed
                | Action::CarriageReturn
                | Action::Backspace
                | Action::HorizontalTab
                | Action::TabSet
                | Action::TabClearCurrent
                | Action::TabClearAll
                | Action::TabReset
                | Action::Index
                | Action::NextLine
                | Action::CursorUp { .. }
                | Action::CursorDown { .. }
                | Action::CursorRight { .. }
                | Action::CursorLeft { .. } => None,
            })
            .collect()
    }

    fn actions(handler: &RecordingHandler) -> &[Action] {
        &handler.actions
    }

    fn next_slice(stream: &mut Stream, handler: &mut RecordingHandler, input: &[u8]) {
        stream.next_slice(input, handler).unwrap();
    }

    #[test]
    fn stream_ascii_dispatches_one_print_action_per_character() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"Hello");

        assert_eq!(print_chars(&handler), vec!['H', 'e', 'l', 'l', 'o']);
    }

    #[test]
    fn stream_unicode_scalars_dispatch_correctly() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, "😄✤ÁA".as_bytes());

        assert_eq!(print_chars(&handler), vec!['😄', '✤', 'Á', 'A']);
    }

    #[test]
    fn stream_split_multibyte_scalar_dispatches_after_final_byte() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();
        let bytes = "😄".as_bytes();

        next_slice(&mut stream, &mut handler, &bytes[..2]);
        assert!(handler.actions.is_empty());

        next_slice(&mut stream, &mut handler, &bytes[2..]);
        assert_eq!(print_chars(&handler), vec!['😄']);
    }

    #[test]
    fn stream_invalid_utf8_dispatches_replacement_character() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0xff]);

        assert_eq!(print_chars(&handler), vec![char::REPLACEMENT_CHARACTER]);
    }

    #[test]
    fn stream_partial_invalid_utf8_retries_rejecting_starter_byte() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\xf0\x9f\xf0\x9f\x98\x84\xed\xa0\x80",
        );

        assert_eq!(
            print_chars(&handler),
            vec![
                char::REPLACEMENT_CHARACTER,
                '😄',
                char::REPLACEMENT_CHARACTER,
                char::REPLACEMENT_CHARACTER,
                char::REPLACEMENT_CHARACTER,
            ]
        );
    }

    #[test]
    fn stream_invalid_utf8_retries_rejecting_ascii_byte() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9fA");

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A']
        );
    }

    #[test]
    fn stream_incomplete_utf8_held_at_slice_boundary_completes_on_next_slice() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();
        let bytes = "✤".as_bytes();

        next_slice(&mut stream, &mut handler, &bytes[..1]);
        next_slice(&mut stream, &mut handler, &bytes[1..2]);
        assert!(handler.actions.is_empty());

        next_slice(&mut stream, &mut handler, &bytes[2..]);
        assert_eq!(print_chars(&handler), vec!['✤']);
    }

    #[test]
    fn stream_lf_and_cr_dispatch_control_actions() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\nB\rC");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::LineFeed,
                Action::Print { cp: 'B' },
                Action::CarriageReturn,
                Action::Print { cp: 'C' },
            ]
        );
    }

    #[test]
    fn stream_backspace_dispatches_control_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x08B");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::Backspace,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_horizontal_tab_dispatches_control_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\tB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::HorizontalTab,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_vt_and_ff_dispatch_linefeed_actions() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x0bB\x0cC");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::LineFeed,
                Action::Print { cp: 'B' },
                Action::LineFeed,
                Action::Print { cp: 'C' },
            ]
        );
    }

    #[test]
    fn stream_escape_h_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1bHB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_escape_d_dispatches_index_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1bDB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::Index,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_escape_e_dispatches_next_line_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1bEB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::NextLine,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_other_c0_controls_do_not_dispatch_print_actions() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x0eB");

        assert_eq!(print_chars(&handler), vec!['A', 'B']);
        assert_eq!(
            actions(&handler),
            &[Action::Print { cp: 'A' }, Action::Print { cp: 'B' }]
        );
    }

    #[test]
    fn stream_raw_c1_bytes_are_handled_by_utf8_decoder() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x80, b'A']);

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A']
        );
    }

    #[test]
    fn stream_raw_c1_ind_and_nel_bytes_do_not_dispatch_escape_actions() {
        for byte in [0x84, 0x85] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, &[byte, b'A']);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_lf() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\nA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::LineFeed,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_vt() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x0bA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::LineFeed,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_ff() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x0cA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::LineFeed,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_cr() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\rA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CarriageReturn,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_backspace() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x08A");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Backspace,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_horizontal_tab() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\tA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::HorizontalTab,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_escape_h_tab_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1bHA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::TabSet,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_escape_d_and_e() {
        for (input, expected) in [
            (b"\xf0\x9f\x1bDA".as_slice(), Action::Index),
            (b"\xf0\x9f\x1bEA".as_slice(), Action::NextLine),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    expected,
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_escape_h_tab_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"HA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::TabSet,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_escape_d_and_e() {
        for (second, expected) in [
            (b"DA".as_slice(), Action::Index),
            (b"EA".as_slice(), Action::NextLine),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b");
            assert_eq!(
                actions(&handler),
                &[Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                }]
            );

            next_slice(&mut stream, &mut handler, second);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    expected,
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_ignoring_del() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0xf0, 0x9f, 0x7f, b'A']);

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A']
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_direct_escape() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1bcA");

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A']
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_escape() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[ZA");

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A']
        );
    }

    #[test]
    fn stream_direct_unsupported_escape_final_does_not_leak_as_printable_text() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1bcB");

        assert_eq!(print_chars(&handler), vec!['A', 'B']);
    }

    #[test]
    fn stream_intermediate_escape_forms_do_not_leak_or_dispatch_d_and_e() {
        for input in [b"A\x1b(DB".as_slice(), b"A\x1b#EB".as_slice()] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[Action::Print { cp: 'A' }, Action::Print { cp: 'B' }]
            );
        }
    }

    #[test]
    fn stream_escape_m_remains_unsupported() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1bMB");

        assert_eq!(
            actions(&handler),
            &[Action::Print { cp: 'A' }, Action::Print { cp: 'B' }]
        );
    }

    #[test]
    fn stream_split_escape_h_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b");
        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        next_slice(&mut stream, &mut handler, b"HB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_split_escape_d_and_e_dispatch_actions() {
        for (second, expected) in [
            (b"DB".as_slice(), Action::Index),
            (b"EB".as_slice(), Action::NextLine),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, b"A\x1b");
            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
            next_slice(&mut stream, &mut handler, second);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    expected,
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_unsupported_csi_sequence_does_not_leak_bytes_as_printable_text() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b[ZB");

        assert_eq!(print_chars(&handler), vec!['A', 'B']);
    }

    #[test]
    fn stream_csi_w_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b[WB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_csi_zero_w_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b[0WB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_csi_tab_clear_and_reset_dispatch_actions() {
        for (input, expected) in [
            (b"A\x1b[2WB".as_slice(), Action::TabClearCurrent),
            (b"A\x1b[5WB".as_slice(), Action::TabClearAll),
            (b"A\x1b[?5WB".as_slice(), Action::TabReset),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    expected,
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_split_csi_w_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b[");
        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        next_slice(&mut stream, &mut handler, b"WB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_split_csi_zero_w_dispatches_tab_set_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"A\x1b[0");
        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        next_slice(&mut stream, &mut handler, b"WB");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::TabSet,
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_split_csi_tab_clear_and_reset_dispatch_actions() {
        for (first, second, expected) in [
            (
                b"A\x1b[2".as_slice(),
                b"WB".as_slice(),
                Action::TabClearCurrent,
            ),
            (b"A\x1b[5".as_slice(), b"WB".as_slice(), Action::TabClearAll),
            (b"A\x1b[?".as_slice(), b"5WB".as_slice(), Action::TabReset),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, first);
            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
            next_slice(&mut stream, &mut handler, second);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    expected,
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_cardinal_cursor_movement_dispatches_default_count() {
        for (input, expected) in [
            (b"A\x1b[AB".as_slice(), Action::CursorUp { count: 1 }),
            (b"A\x1b[BB".as_slice(), Action::CursorDown { count: 1 }),
            (b"A\x1b[CB".as_slice(), Action::CursorRight { count: 1 }),
            (b"A\x1b[DB".as_slice(), Action::CursorLeft { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    expected,
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_cursor_movement_aliases_dispatch_actions() {
        for (input, expected) in [
            (b"\x1b[kA".as_slice(), Action::CursorUp { count: 1 }),
            (b"\x1b[aA".as_slice(), Action::CursorRight { count: 1 }),
            (b"\x1b[jA".as_slice(), Action::CursorLeft { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_cursor_movement_dispatches_explicit_and_zero_counts() {
        for (input, expected) in [
            (b"\x1b[5CA".as_slice(), Action::CursorRight { count: 5 }),
            (b"\x1b[0CA".as_slice(), Action::CursorRight { count: 1 }),
            (b"\x1b[12AA".as_slice(), Action::CursorUp { count: 12 }),
            (b"\x1b[0DA".as_slice(), Action::CursorLeft { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_next_and_previous_line_dispatch_ordered_action_pairs() {
        for (input, first) in [
            (b"A\x1b[EB".as_slice(), Action::CursorDown { count: 1 }),
            (b"A\x1b[FB".as_slice(), Action::CursorUp { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    first,
                    Action::CarriageReturn,
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_next_and_previous_line_dispatch_explicit_zero_and_overflowing_counts() {
        for (input, first) in [
            (b"\x1b[5EA".as_slice(), Action::CursorDown { count: 5 }),
            (b"\x1b[0EA".as_slice(), Action::CursorDown { count: 1 }),
            (b"\x1b[3FA".as_slice(), Action::CursorUp { count: 3 }),
            (b"\x1b[0FA".as_slice(), Action::CursorUp { count: 1 }),
            (
                b"\x1b[999999999999999999999999EA".as_slice(),
                Action::CursorDown { count: u16::MAX },
            ),
            (
                b"\x1b[999999999999999999999999FA".as_slice(),
                Action::CursorUp { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[first, Action::CarriageReturn, Action::Print { cp: 'A' }]
            );
        }
    }

    #[test]
    fn stream_csi_cursor_movement_saturates_overflowing_count() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999CA",
        );

        assert_eq!(
            actions(&handler),
            &[
                Action::CursorRight { count: u16::MAX },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_split_csi_cursor_movement_dispatches_actions() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"AA".as_slice(),
                Action::CursorUp { count: 1 },
            ),
            (
                b"\x1b[5".as_slice(),
                b"BA".as_slice(),
                Action::CursorDown { count: 5 },
            ),
            (
                b"\x1b[12".as_slice(),
                b"CA".as_slice(),
                Action::CursorRight { count: 12 },
            ),
            (
                b"\x1b[0".as_slice(),
                b"DA".as_slice(),
                Action::CursorLeft { count: 1 },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, first);
            assert!(actions(&handler).is_empty());
            next_slice(&mut stream, &mut handler, second);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_split_csi_next_and_previous_line_dispatches_action_pairs() {
        for (first, second, action) in [
            (
                b"\x1b[".as_slice(),
                b"EA".as_slice(),
                Action::CursorDown { count: 1 },
            ),
            (
                b"\x1b[5".as_slice(),
                b"EA".as_slice(),
                Action::CursorDown { count: 5 },
            ),
            (
                b"\x1b[".as_slice(),
                b"FA".as_slice(),
                Action::CursorUp { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"FA".as_slice(),
                Action::CursorUp { count: 3 },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, first);
            assert!(actions(&handler).is_empty());
            next_slice(&mut stream, &mut handler, second);

            assert_eq!(
                actions(&handler),
                &[action, Action::CarriageReturn, Action::Print { cp: 'A' }]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_cursor_movement() {
        for (input, expected) in [
            (b"\xf0\x9f\x1b[AA".as_slice(), Action::CursorUp { count: 1 }),
            (
                b"\xf0\x9f\x1b[BA".as_slice(),
                Action::CursorDown { count: 1 },
            ),
            (
                b"\xf0\x9f\x1b[CA".as_slice(),
                Action::CursorRight { count: 1 },
            ),
            (
                b"\xf0\x9f\x1b[DA".as_slice(),
                Action::CursorLeft { count: 1 },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    expected,
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_cursor_movement() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"CA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CursorRight { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_line_pairs() {
        for (input, first) in [
            (
                b"\xf0\x9f\x1b[EA".as_slice(),
                Action::CursorDown { count: 1 },
            ),
            (b"\xf0\x9f\x1b[FA".as_slice(), Action::CursorUp { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    first,
                    Action::CarriageReturn,
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_line_pair() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"EA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CursorDown { count: 1 },
                Action::CarriageReturn,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_unsupported_csi_cursor_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3CA".as_slice(),
            b"\x1b[>3CA".as_slice(),
            b"\x1b[5;4CA".as_slice(),
            b"\x1b[ CA".as_slice(),
            b"\x1b[?AA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_line_pair_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3EA".as_slice(),
            b"\x1b[?3FA".as_slice(),
            b"\x1b[>3EA".as_slice(),
            b"\x1b[>3FA".as_slice(),
            b"\x1b[5;4EA".as_slice(),
            b"\x1b[5;4FA".as_slice(),
            b"\x1b[1:2EA".as_slice(),
            b"\x1b[1:2FA".as_slice(),
            b"\x1b[ EA".as_slice(),
            b"\x1b[ FA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_cursor_actions() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'A']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_line_pair_actions() {
        for final_byte in [b'E', b'F'] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, &[0x9b, final_byte]);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    Action::Print {
                        cp: final_byte as char,
                    },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_w_tab_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[WA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::TabSet,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_w_tab_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"WA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::TabSet,
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_tab_clear_and_reset() {
        for (input, expected) in [
            (b"\xf0\x9f\x1b[2WA".as_slice(), Action::TabClearCurrent),
            (b"\xf0\x9f\x1b[5WA".as_slice(), Action::TabClearAll),
            (b"\xf0\x9f\x1b[?5WA".as_slice(), Action::TabReset),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    expected,
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_unsupported_csi_w_variants_do_not_dispatch_tab_actions() {
        for input in [
            b"\x1b[>WA".as_slice(),
            b"\x1b[?WA".as_slice(),
            b"\x1b[99WA".as_slice(),
            b"\x1b[1WA".as_slice(),
            b"\x1b[0;0WA".as_slice(),
            b"\x1b[?2WA".as_slice(),
            b"\x1b[>5WA".as_slice(),
            b"\x1b[?1WA".as_slice(),
            b"\x1b[0;5WA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_overflowing_csi_w_parameter_does_not_dispatch_tab_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999999WZ",
        );

        assert_eq!(actions(&handler), &[Action::Print { cp: 'Z' }]);
    }

    #[derive(Debug, Default)]
    struct ErrorOnActionHandler {
        fail: Option<Action>,
        actions: Vec<Action>,
    }

    impl ErrorOnActionHandler {
        fn new(fail: Action) -> Self {
            Self {
                fail: Some(fail),
                actions: Vec::new(),
            }
        }
    }

    impl Handler for ErrorOnActionHandler {
        type Error = ();

        fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
            if self.fail == Some(action) {
                return Err(());
            }

            self.actions.push(action);
            Ok(())
        }
    }

    #[derive(Debug, Default)]
    struct ErrorOnActionWithAttemptsHandler {
        fail: Option<Action>,
        attempts: Vec<Action>,
    }

    impl ErrorOnActionWithAttemptsHandler {
        fn new(fail: Action) -> Self {
            Self {
                fail: Some(fail),
                attempts: Vec::new(),
            }
        }
    }

    impl Handler for ErrorOnActionWithAttemptsHandler {
        type Error = ();

        fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
            self.attempts.push(action);
            if self.fail == Some(action) {
                return Err(());
            }
            Ok(())
        }
    }

    #[test]
    fn stream_escape_h_restores_ground_before_handler_error() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnActionHandler::new(Action::TabSet);

        assert_eq!(stream.next_slice(b"\x1bH", &mut handler), Err(()));
        stream.next_slice(b"A", &mut handler).unwrap();

        assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_escape_d_and_e_restore_ground_before_handler_error() {
        for fail in [Action::Index, Action::NextLine] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);
            let input = match fail {
                Action::Index => b"\x1bD".as_slice(),
                Action::NextLine => b"\x1bE".as_slice(),
                Action::Print { .. }
                | Action::LineFeed
                | Action::CarriageReturn
                | Action::Backspace
                | Action::HorizontalTab
                | Action::TabSet
                | Action::TabClearCurrent
                | Action::TabClearAll
                | Action::TabReset => unreachable!("loop only uses D/E actions"),
                Action::CursorUp { .. }
                | Action::CursorDown { .. }
                | Action::CursorRight { .. }
                | Action::CursorLeft { .. } => unreachable!("loop only uses D/E actions"),
            };

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_cursor_actions_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[A".as_slice(), Action::CursorUp { count: 1 }),
            (b"\x1b[B".as_slice(), Action::CursorDown { count: 1 }),
            (b"\x1b[C".as_slice(), Action::CursorRight { count: 1 }),
            (b"\x1b[D".as_slice(), Action::CursorLeft { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_line_pair_first_action_error_restores_ground_and_skips_cr() {
        for (input, fail) in [
            (b"\x1b[E".as_slice(), Action::CursorDown { count: 1 }),
            (b"\x1b[F".as_slice(), Action::CursorUp { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionWithAttemptsHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(
                handler.attempts,
                &[fail, Action::Print { cp: 'A' }],
                "carriage return must not be invoked after first action error"
            );
        }
    }

    #[test]
    fn stream_csi_line_pair_second_action_error_restores_ground() {
        for (input, first) in [
            (b"\x1b[E".as_slice(), Action::CursorDown { count: 1 }),
            (b"\x1b[F".as_slice(), Action::CursorUp { count: 1 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(Action::CarriageReturn);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[first, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_w_restores_ground_before_handler_error() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnActionHandler::new(Action::TabSet);

        assert_eq!(stream.next_slice(b"\x1b[W", &mut handler), Err(()));
        stream.next_slice(b"A", &mut handler).unwrap();

        assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_csi_tab_clear_and_reset_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[2W".as_slice(), Action::TabClearCurrent),
            (b"\x1b[5W".as_slice(), Action::TabClearAll),
            (b"\x1b[?5W".as_slice(), Action::TabReset),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_state_remains_usable_after_invalid_utf8_and_unsupported_escape() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xffA\x1b[ZB");

        assert_eq!(
            print_chars(&handler),
            vec![char::REPLACEMENT_CHARACTER, 'A', 'B']
        );
    }
}
