//! Terminal byte stream decoding.

use super::{modes, osc, sgr};

const CSI_PARAM_CAPACITY: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Action {
    Print {
        cp: char,
    },
    LineFeed,
    CarriageReturn,
    Backspace,
    HorizontalTab {
        count: u16,
    },
    HorizontalTabBack {
        count: u16,
    },
    TabSet,
    TabClearCurrent,
    TabClearAll,
    TabReset,
    Index,
    NextLine,
    CursorUp {
        count: u16,
    },
    CursorDown {
        count: u16,
    },
    CursorRight {
        count: u16,
    },
    CursorLeft {
        count: u16,
    },
    CursorColumn {
        col: u16,
    },
    CursorRow {
        row: u16,
    },
    CursorRowRelative {
        rows: u16,
    },
    CursorPosition {
        row: u16,
        col: u16,
    },
    EraseDisplay {
        mode: EraseDisplayMode,
        protected: bool,
    },
    EraseLine {
        mode: EraseLineMode,
        protected: bool,
    },
    InsertChars {
        count: u16,
    },
    DeleteChars {
        count: u16,
    },
    EraseChars {
        count: u16,
    },
    InsertLines {
        count: u16,
    },
    DeleteLines {
        count: u16,
    },
    ScrollUp {
        count: u16,
    },
    ScrollDown {
        count: u16,
    },
    SetMode {
        mode: modes::Mode,
    },
    ResetMode {
        mode: modes::Mode,
    },
    SaveMode {
        mode: modes::Mode,
    },
    RestoreMode {
        mode: modes::Mode,
    },
    RequestMode {
        mode: modes::Mode,
    },
    RequestModeUnknown {
        value: u16,
        ansi: bool,
    },
    SetAttribute {
        attr: sgr::Attribute,
    },
}

pub(super) type OscAction<'a> = osc::Command<'a>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EraseDisplayMode {
    Below,
    Above,
    Complete,
    Scrollback,
    ScrollComplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EraseLineMode {
    Right,
    Left,
    Complete,
}

pub(super) trait Handler {
    type Error;

    fn vt(&mut self, action: Action) -> Result<(), Self::Error>;

    fn osc(&mut self, _action: OscAction<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EscapeState {
    Ground,
    Escape,
    EscapeInvalidIntermediate,
    Csi(CsiState),
    Osc,
    OscEscape,
    OscInvalid,
    OscInvalidEscape,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Stream {
    utf8: Utf8Decoder,
    escape: EscapeState,
    osc: osc::Parser,
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
    intermediate: Option<u8>,
    params: [u16; CSI_PARAM_CAPACITY],
    param_separators: [sgr::Separator; CSI_PARAM_CAPACITY],
    params_len: u8,
    param_acc: u16,
    param_has_digits: bool,
    invalid: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CsiParams {
    values: [u16; CSI_PARAM_CAPACITY],
    separators: [sgr::Separator; CSI_PARAM_CAPACITY],
    len: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CsiDispatch {
    None,
    One(Action),
    Two(Action, Action),
    Many(CsiActionList),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CsiActionList {
    actions: [Option<Action>; CSI_PARAM_CAPACITY],
    len: u8,
}

impl Stream {
    pub(super) const fn init() -> Self {
        Self {
            utf8: Utf8Decoder::new(),
            escape: EscapeState::Ground,
            osc: osc::Parser::new(),
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
            EscapeState::Osc => self.next_osc(byte, handler),
            EscapeState::OscEscape => self.next_osc_escape(byte, handler),
            EscapeState::OscInvalid => {
                self.next_osc_invalid(byte);
                Ok(())
            }
            EscapeState::OscInvalidEscape => {
                self.next_osc_invalid_escape(byte);
                Ok(())
            }
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
            b'\t' => handler.vt(Action::HorizontalTab { count: 1 })?,
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
            b']' => {
                self.osc.reset();
                self.escape = EscapeState::Osc;
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

    fn next_osc<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        match byte {
            0x07 => self.finish_osc(handler, osc::Terminator::Bel),
            0x1b => {
                self.escape = EscapeState::OscEscape;
                Ok(())
            }
            _ => {
                self.osc.push(byte);
                Ok(())
            }
        }
    }

    fn next_osc_escape<H: Handler>(&mut self, byte: u8, handler: &mut H) -> Result<(), H::Error> {
        match byte {
            0x07 => self.finish_osc(handler, osc::Terminator::Bel),
            b'\\' => self.finish_osc(handler, osc::Terminator::St),
            b']' => {
                self.osc.invalidate();
                self.escape = EscapeState::OscInvalid;
                Ok(())
            }
            _ => {
                self.osc.push_escape_and(byte);
                self.escape = EscapeState::Osc;
                Ok(())
            }
        }
    }

    fn next_osc_invalid(&mut self, byte: u8) {
        match byte {
            0x07 => {
                self.osc.reset();
                self.escape = EscapeState::Ground;
            }
            0x1b => {
                self.escape = EscapeState::OscInvalidEscape;
            }
            _ => {}
        }
    }

    fn next_osc_invalid_escape(&mut self, byte: u8) {
        match byte {
            0x07 | b'\\' => {
                self.osc.reset();
                self.escape = EscapeState::Ground;
            }
            _ => {
                self.escape = EscapeState::OscInvalid;
            }
        }
    }

    fn finish_osc<H: Handler>(
        &mut self,
        handler: &mut H,
        terminator: osc::Terminator,
    ) -> Result<(), H::Error> {
        self.escape = EscapeState::Ground;
        let result = if let Some(action) = self.osc.command(terminator) {
            handler.osc(action)
        } else {
            Ok(())
        };
        self.osc.reset();
        result
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
            intermediate: None,
            params: [0; CSI_PARAM_CAPACITY],
            param_separators: [sgr::Separator::None; CSI_PARAM_CAPACITY],
            params_len: 0,
            param_acc: 0,
            param_has_digits: false,
            invalid: false,
        }
    }

    fn push(&mut self, byte: u8) {
        if self.invalid {
            return;
        }
        if self.intermediate.is_some() {
            self.invalid = true;
            return;
        }

        match byte {
            b'?' if self.private.is_none()
                && self.params_len == 0
                && !self.param_has_digits
                && !self.separator_seen() =>
            {
                self.private = Some(byte);
            }
            b'$' => self.intermediate = Some(byte),
            b';' => self.push_param_separator(sgr::Separator::Semicolon),
            b':' => self.push_param_separator(sgr::Separator::Colon),
            b'0'..=b'9' => {
                let digit = u16::from(byte - b'0');
                self.param_acc = self.param_acc.saturating_mul(10).saturating_add(digit);
                self.param_has_digits = true;
            }
            _ => self.invalid = true,
        }
    }

    fn push_param_separator(&mut self, separator: sgr::Separator) {
        if usize::from(self.params_len) >= self.params.len() {
            self.invalid = true;
            return;
        }

        let index = usize::from(self.params_len);
        self.params[index] = self.param_acc;
        self.param_separators[index] = separator;
        self.params_len += 1;
        self.param_acc = 0;
        self.param_has_digits = false;
    }

    fn separator_seen(&self) -> bool {
        self.param_separators[..usize::from(self.params_len)]
            .iter()
            .any(|separator| *separator != sgr::Separator::None)
    }

    fn dispatch(&self, final_byte: u8) -> CsiDispatch {
        if self.intermediate.is_some() {
            return self
                .mode_request_dispatch(final_byte)
                .unwrap_or(CsiDispatch::None);
        }

        if let Some(dispatch) = self.line_dispatch(final_byte) {
            return dispatch;
        }

        if let Some(action) = self.column_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.row_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.cursor_position_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.cursor_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.horizontal_tab_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.horizontal_tab_back_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.erase_display_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.erase_line_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.insert_chars_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.delete_chars_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.erase_chars_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.insert_lines_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.delete_lines_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.scroll_up_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(action) = self.scroll_down_action(final_byte) {
            return CsiDispatch::One(action);
        }

        if let Some(dispatch) = self.mode_dispatch(final_byte) {
            return dispatch;
        }

        if let Some(dispatch) = self.mode_save_restore_dispatch(final_byte) {
            return dispatch;
        }

        if let Some(dispatch) = self.sgr_dispatch(final_byte) {
            return dispatch;
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
        let param = self.single_param(false)?;

        Some(param.unwrap_or(1).max(1))
    }

    fn finalized_params(&self) -> Option<CsiParams> {
        if self.invalid {
            return None;
        }

        let mut values = self.params;
        let separators = self.param_separators;
        let mut len = self.params_len;
        if self.param_has_digits {
            if usize::from(len) >= values.len() {
                return None;
            }
            values[usize::from(len)] = self.param_acc;
            len += 1;
        }

        Some(CsiParams {
            values,
            separators,
            len,
        })
    }

    fn single_param(&self, allow_separator: bool) -> Option<Option<u16>> {
        if self.private.is_some() {
            return None;
        }

        let params = self.finalized_params()?;
        if params.len > 1 || params.colon_seen() || (!allow_separator && params.separator_seen()) {
            return None;
        }

        Some((params.len == 1).then_some(params.values[0]))
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

    fn position_value(&self) -> Option<u16> {
        Some(self.single_param(false)?.unwrap_or(1))
    }

    fn column_action(&self, final_byte: u8) -> Option<Action> {
        let col = self.position_value()?;
        match final_byte {
            b'G' | b'`' => Some(Action::CursorColumn { col }),
            _ => None,
        }
    }

    fn row_action(&self, final_byte: u8) -> Option<Action> {
        let value = self.position_value()?;
        match final_byte {
            b'd' => Some(Action::CursorRow { row: value }),
            b'e' => Some(Action::CursorRowRelative { rows: value }),
            _ => None,
        }
    }

    fn cursor_position_action(&self, final_byte: u8) -> Option<Action> {
        if self.private.is_some() {
            return None;
        }

        let params = self.finalized_params()?;
        if params.colon_seen() {
            return None;
        }
        let (row, col) = match params.len {
            0 => (1, 1),
            1 => (params.values[0], 1),
            2 => (params.values[0], params.values[1]),
            _ => return None,
        };

        match final_byte {
            b'H' | b'f' => Some(Action::CursorPosition { row, col }),
            _ => None,
        }
    }

    fn horizontal_tab_action(&self, final_byte: u8) -> Option<Action> {
        let count = self.single_param(true)?.unwrap_or(1);
        match final_byte {
            b'I' => Some(Action::HorizontalTab { count }),
            _ => None,
        }
    }

    fn horizontal_tab_back_action(&self, final_byte: u8) -> Option<Action> {
        let count = self.single_param(true)?.unwrap_or(1);
        match final_byte {
            b'Z' => Some(Action::HorizontalTabBack { count }),
            _ => None,
        }
    }

    fn erase_display_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'J' {
            return None;
        }

        let protected = match self.private {
            None => false,
            Some(b'?') => true,
            Some(_) => return None,
        };
        let params = self.finalized_params()?;
        if params.colon_seen() {
            return None;
        }
        if params.len > 1 {
            return None;
        }

        let param = (params.len == 1).then_some(params.values[0]).unwrap_or(0);
        let mode = match param {
            0 => EraseDisplayMode::Below,
            1 => EraseDisplayMode::Above,
            2 => EraseDisplayMode::Complete,
            3 => EraseDisplayMode::Scrollback,
            22 => EraseDisplayMode::ScrollComplete,
            _ => return None,
        };

        Some(Action::EraseDisplay { mode, protected })
    }

    fn erase_line_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'K' {
            return None;
        }

        let protected = match self.private {
            None => false,
            Some(b'?') => true,
            Some(_) => return None,
        };
        let params = self.finalized_params()?;
        if params.colon_seen() {
            return None;
        }
        if params.len > 1 {
            return None;
        }

        let param = (params.len == 1).then_some(params.values[0]).unwrap_or(0);
        let mode = match param {
            0 => EraseLineMode::Right,
            1 => EraseLineMode::Left,
            2 => EraseLineMode::Complete,
            _ => return None,
        };

        Some(Action::EraseLine { mode, protected })
    }

    fn delete_chars_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'P' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::DeleteChars { count })
    }

    fn insert_chars_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'@' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1).max(1);
        Some(Action::InsertChars { count })
    }

    fn erase_chars_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'X' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::EraseChars { count })
    }

    fn insert_lines_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'L' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::InsertLines { count })
    }

    fn delete_lines_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'M' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::DeleteLines { count })
    }

    fn scroll_up_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'S' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::ScrollUp { count })
    }

    fn scroll_down_action(&self, final_byte: u8) -> Option<Action> {
        if final_byte != b'T' {
            return None;
        }

        let count = self.single_param(true)?.unwrap_or(1);
        Some(Action::ScrollDown { count })
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
        let params = self.finalized_params()?;
        if params.separator_seen() || params.len > 1 {
            return None;
        }

        let param = (params.len == 1).then_some(params.values[0]);
        match (self.private, param) {
            (None, None | Some(0)) => Some(Action::TabSet),
            (None, Some(2)) => Some(Action::TabClearCurrent),
            (None, Some(5)) => Some(Action::TabClearAll),
            (Some(b'?'), Some(5)) => Some(Action::TabReset),
            (Some(_), _) | (None, Some(_)) => None,
        }
    }

    fn mode_dispatch(&self, final_byte: u8) -> Option<CsiDispatch> {
        let set = match final_byte {
            b'h' => true,
            b'l' => false,
            _ => return None,
        };
        let ansi = match self.private {
            None => true,
            Some(b'?') => false,
            Some(_) => return None,
        };
        let params = self.finalized_params()?;
        if params.colon_seen() {
            return None;
        }
        let mut actions = CsiActionList::new();

        for param in params.values[..usize::from(params.len)].iter().copied() {
            let Some(mode) = modes::mode_from_int(param, ansi) else {
                continue;
            };
            actions.push(if set {
                Action::SetMode { mode }
            } else {
                Action::ResetMode { mode }
            });
        }

        Some(if actions.is_empty() {
            CsiDispatch::None
        } else {
            CsiDispatch::Many(actions)
        })
    }

    fn mode_save_restore_dispatch(&self, final_byte: u8) -> Option<CsiDispatch> {
        let save = match final_byte {
            b's' => true,
            b'r' => false,
            _ => return None,
        };
        if self.private != Some(b'?') {
            return None;
        }

        let params = self.finalized_params()?;
        if params.colon_seen() {
            return None;
        }
        let mut actions = CsiActionList::new();

        for param in params.values[..usize::from(params.len)].iter().copied() {
            let Some(mode) = modes::mode_from_int(param, false) else {
                continue;
            };
            actions.push(if save {
                Action::SaveMode { mode }
            } else {
                Action::RestoreMode { mode }
            });
        }

        Some(if actions.is_empty() {
            CsiDispatch::None
        } else {
            CsiDispatch::Many(actions)
        })
    }

    fn mode_request_dispatch(&self, final_byte: u8) -> Option<CsiDispatch> {
        if final_byte != b'p' || self.private != Some(b'?') || self.intermediate != Some(b'$') {
            return None;
        }

        let params = self.finalized_params()?;
        if params.len != 1 || params.separator_seen() {
            return None;
        }

        let value = params.values[0];
        let action = if let Some(mode) = modes::mode_from_int(value, false) {
            Action::RequestMode { mode }
        } else {
            Action::RequestModeUnknown { value, ansi: false }
        };

        Some(CsiDispatch::One(action))
    }

    fn sgr_dispatch(&self, final_byte: u8) -> Option<CsiDispatch> {
        if final_byte != b'm' || self.private.is_some() || self.intermediate.is_some() {
            return None;
        }

        let params = self.finalized_params()?;
        let len = usize::from(params.len);
        let mut parser = sgr::Parser::new(&params.values[..len], &params.separators[..len]);
        let mut actions = CsiActionList::new();

        while let Some(attr) = parser.next() {
            if attr != sgr::Attribute::Unknown {
                actions.push(Action::SetAttribute { attr });
            }
        }

        Some(if actions.is_empty() {
            CsiDispatch::None
        } else {
            CsiDispatch::Many(actions)
        })
    }
}

impl CsiParams {
    fn separator_seen(&self) -> bool {
        self.separators[..usize::from(self.len)]
            .iter()
            .any(|separator| *separator != sgr::Separator::None)
    }

    fn colon_seen(&self) -> bool {
        self.separators[..usize::from(self.len)]
            .iter()
            .any(|separator| *separator == sgr::Separator::Colon)
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
            Self::Many(actions) => actions.handle(handler),
        }
    }
}

impl CsiActionList {
    const fn new() -> Self {
        Self {
            actions: [None; CSI_PARAM_CAPACITY],
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn push(&mut self, action: Action) {
        let index = usize::from(self.len);
        debug_assert!(index < self.actions.len());
        self.actions[index] = Some(action);
        self.len += 1;
    }

    fn handle<H: Handler>(self, handler: &mut H) -> Result<(), H::Error> {
        for action in self.actions[..usize::from(self.len)].iter().flatten() {
            handler.vt(*action)?;
        }
        Ok(())
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
    use crate::terminal::color;

    #[derive(Debug, Default)]
    struct RecordingHandler {
        actions: Vec<Action>,
        osc_actions: Vec<OwnedOscAction>,
    }

    impl Handler for RecordingHandler {
        type Error = ();

        fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
            self.actions.push(action);
            Ok(())
        }

        fn osc(&mut self, action: OscAction<'_>) -> Result<(), Self::Error> {
            self.osc_actions.push(OwnedOscAction::from(action));
            Ok(())
        }
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum OwnedOscAction {
        WindowTitle { title: String },
        ReportPwd { url: String },
        StartHyperlink { id: Option<String>, uri: String },
        EndHyperlink,
        ColorOperation { requests: Vec<osc::ColorRequest> },
    }

    impl From<OscAction<'_>> for OwnedOscAction {
        fn from(action: OscAction<'_>) -> Self {
            match action {
                OscAction::WindowTitle { title } => Self::WindowTitle {
                    title: title.to_string(),
                },
                OscAction::ReportPwd { url } => Self::ReportPwd {
                    url: url.to_string(),
                },
                OscAction::StartHyperlink { id, uri } => Self::StartHyperlink {
                    id: id.map(ToString::to_string),
                    uri: uri.to_string(),
                },
                OscAction::EndHyperlink => Self::EndHyperlink,
                OscAction::ColorOperation { requests } => Self::ColorOperation {
                    requests: requests.iter().collect(),
                },
            }
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
                | Action::HorizontalTab { .. }
                | Action::HorizontalTabBack { .. }
                | Action::TabSet
                | Action::TabClearCurrent
                | Action::TabClearAll
                | Action::TabReset
                | Action::Index
                | Action::NextLine
                | Action::CursorUp { .. }
                | Action::CursorDown { .. }
                | Action::CursorRight { .. }
                | Action::CursorLeft { .. }
                | Action::CursorColumn { .. }
                | Action::CursorRow { .. }
                | Action::CursorRowRelative { .. }
                | Action::CursorPosition { .. }
                | Action::EraseDisplay { .. }
                | Action::EraseLine { .. }
                | Action::InsertChars { .. }
                | Action::DeleteChars { .. }
                | Action::EraseChars { .. }
                | Action::InsertLines { .. }
                | Action::DeleteLines { .. }
                | Action::ScrollUp { .. }
                | Action::ScrollDown { .. }
                | Action::SetMode { .. }
                | Action::ResetMode { .. }
                | Action::SaveMode { .. }
                | Action::RestoreMode { .. }
                | Action::RequestMode { .. }
                | Action::RequestModeUnknown { .. }
                | Action::SetAttribute { .. } => None,
            })
            .collect()
    }

    fn actions(handler: &RecordingHandler) -> &[Action] {
        &handler.actions
    }

    fn osc_actions(handler: &RecordingHandler) -> &[OwnedOscAction] {
        &handler.osc_actions
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
                Action::HorizontalTab { count: 1 },
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_csi_horizontal_tabulation_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[IB".as_slice(), Action::HorizontalTab { count: 1 }),
            (b"\x1b[3IA".as_slice(), Action::HorizontalTab { count: 3 }),
            (b"\x1b[0IA".as_slice(), Action::HorizontalTab { count: 0 }),
            (b"\x1b[;IA".as_slice(), Action::HorizontalTab { count: 0 }),
            (b"\x1b[3;IA".as_slice(), Action::HorizontalTab { count: 3 }),
            (
                b"\x1b[999999999999999999999999IA".as_slice(),
                Action::HorizontalTab { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_horizontal_tab_back_dispatches_counts() {
        for (input, expected) in [
            (
                b"A\x1b[ZB".as_slice(),
                Action::HorizontalTabBack { count: 1 },
            ),
            (
                b"\x1b[3ZA".as_slice(),
                Action::HorizontalTabBack { count: 3 },
            ),
            (
                b"\x1b[0ZA".as_slice(),
                Action::HorizontalTabBack { count: 0 },
            ),
            (
                b"\x1b[;ZA".as_slice(),
                Action::HorizontalTabBack { count: 0 },
            ),
            (
                b"\x1b[1ZA".as_slice(),
                Action::HorizontalTabBack { count: 1 },
            ),
            (
                b"\x1b[1;ZA".as_slice(),
                Action::HorizontalTabBack { count: 1 },
            ),
            (
                b"\x1b[999999999999999999999999ZA".as_slice(),
                Action::HorizontalTabBack { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_mode_set_and_reset_dispatch_known_ansi_and_dec_modes() {
        for (input, expected) in [
            (
                b"\x1b[4hA".as_slice(),
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
            ),
            (
                b"\x1b[4lA".as_slice(),
                Action::ResetMode {
                    mode: modes::Mode::Insert,
                },
            ),
            (
                b"\x1b[?6hA".as_slice(),
                Action::SetMode {
                    mode: modes::Mode::Origin,
                },
            ),
            (
                b"\x1b[?6lA".as_slice(),
                Action::ResetMode {
                    mode: modes::Mode::Origin,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_set_and_reset_dispatch_multi_params_in_order() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b[4;20hA\x1b[?1;7;2004lB");

        assert_eq!(
            actions(&handler),
            &[
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
                Action::SetMode {
                    mode: modes::Mode::Linefeed,
                },
                Action::Print { cp: 'A' },
                Action::ResetMode {
                    mode: modes::Mode::CursorKeys,
                },
                Action::ResetMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::ResetMode {
                    mode: modes::Mode::BracketedPaste,
                },
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_csi_mode_set_dispatches_exactly_twenty_four_params() {
        let input = format!("\x1b[{}hA", ["4"; 24].join(";"));
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, input.as_bytes());

        assert_eq!(actions(&handler).len(), 25);
        assert!(actions(&handler)[..24].iter().all(|action| {
            *action
                == Action::SetMode {
                    mode: modes::Mode::Insert,
                }
        }));
        assert_eq!(actions(&handler)[24], Action::Print { cp: 'A' });
    }

    #[test]
    fn stream_csi_mode_set_over_capacity_params_do_not_dispatch_or_leak_final_byte() {
        let input = format!("\x1b[{}hA", ["4"; 25].join(";"));
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, input.as_bytes());

        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_csi_mode_set_skips_unknown_modes_without_aborting_known_modes() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b[9999;4;9998;20hA");

        assert_eq!(
            actions(&handler),
            &[
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
                Action::SetMode {
                    mode: modes::Mode::Linefeed,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_csi_mode_save_and_restore_dispatch_known_dec_modes() {
        for (input, expected) in [
            (
                b"\x1b[?7sA".as_slice(),
                Action::SaveMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
            (
                b"\x1b[?7rA".as_slice(),
                Action::RestoreMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_dispatch_multi_params_in_order() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[?1;7;2004sA\x1b[?1;7;2004rB",
        );

        assert_eq!(
            actions(&handler),
            &[
                Action::SaveMode {
                    mode: modes::Mode::CursorKeys,
                },
                Action::SaveMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::SaveMode {
                    mode: modes::Mode::BracketedPaste,
                },
                Action::Print { cp: 'A' },
                Action::RestoreMode {
                    mode: modes::Mode::CursorKeys,
                },
                Action::RestoreMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::RestoreMode {
                    mode: modes::Mode::BracketedPaste,
                },
                Action::Print { cp: 'B' },
            ]
        );
    }

    #[test]
    fn stream_csi_mode_save_and_restore_skip_unknown_modes_without_aborting_known_modes() {
        for (input, expected) in [
            (
                b"\x1b[?9999;7;9998;2004sA".as_slice(),
                [
                    Action::SaveMode {
                        mode: modes::Mode::Wraparound,
                    },
                    Action::SaveMode {
                        mode: modes::Mode::BracketedPaste,
                    },
                    Action::Print { cp: 'A' },
                ],
            ),
            (
                b"\x1b[?9999;7;9998;2004rA".as_slice(),
                [
                    Action::RestoreMode {
                        mode: modes::Mode::Wraparound,
                    },
                    Action::RestoreMode {
                        mode: modes::Mode::BracketedPaste,
                    },
                    Action::Print { cp: 'A' },
                ],
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &expected);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_empty_params_dispatch_no_action() {
        for input in [b"\x1b[?sA".as_slice(), b"\x1b[?rA".as_slice()] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_dispatch_exactly_twenty_four_params() {
        for (final_byte, expected_mode_action) in [(b's', true), (b'r', false)] {
            let input = format!("\x1b[?{}{}A", ["7"; 24].join(";"), final_byte as char);
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input.as_bytes());

            assert_eq!(actions(&handler).len(), 25);
            assert!(actions(&handler)[..24].iter().all(|action| {
                *action
                    == if expected_mode_action {
                        Action::SaveMode {
                            mode: modes::Mode::Wraparound,
                        }
                    } else {
                        Action::RestoreMode {
                            mode: modes::Mode::Wraparound,
                        }
                    }
            }));
            assert_eq!(actions(&handler)[24], Action::Print { cp: 'A' });
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_over_capacity_params_do_not_dispatch_or_leak_final_byte() {
        for final_byte in [b's', b'r'] {
            let input = format!("\x1b[?{}{}A", ["7"; 25].join(";"), final_byte as char);
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input.as_bytes());

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_plain_s_and_r_remain_unsupported_without_leaking_final_byte() {
        for input in [
            b"\x1b[sA".as_slice(),
            b"\x1b[rA".as_slice(),
            b"\x1b[2rA".as_slice(),
            b"\x1b[1;3rA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_invalid_forms_do_not_dispatch_or_leak_final_byte() {
        for input in [
            b"\x1b[>7sA".as_slice(),
            b"\x1b[>7rA".as_slice(),
            b"\x1b[?7:8sA".as_slice(),
            b"\x1b[?7:8rA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_split_feed_dispatches() {
        for (parts, expected) in [
            (
                [b"\x1b[?".as_slice(), b"7sA".as_slice()],
                Action::SaveMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
            (
                [b"\x1b[?7".as_slice(), b"rA".as_slice()],
                Action::RestoreMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, parts[0]);
            next_slice(&mut stream, &mut handler, parts[1]);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_flush_pending_invalid_utf8_first() {
        for input in [
            [b"\xf0".as_slice(), b"\x1b[?7sA".as_slice()],
            [b"\xf0\x1b[?".as_slice(), b"7rA".as_slice()],
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input[0]);
            next_slice(&mut stream, &mut handler, input[1]);

            assert_eq!(
                actions(&handler)[0],
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER
                }
            );
            assert_eq!(actions(&handler).last(), Some(&Action::Print { cp: 'A' }));
        }
    }

    #[test]
    fn stream_csi_mode_request_dispatches_known_and_unknown_dec_modes() {
        for (input, expected) in [
            (
                b"\x1b[?7$pA".as_slice(),
                Action::RequestMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
            (
                b"\x1b[?2004$pA".as_slice(),
                Action::RequestMode {
                    mode: modes::Mode::BracketedPaste,
                },
            ),
            (
                b"\x1b[?9999$pA".as_slice(),
                Action::RequestModeUnknown {
                    value: 9999,
                    ansi: false,
                },
            ),
            (
                b"\x1b[?0$pA".as_slice(),
                Action::RequestModeUnknown {
                    value: 0,
                    ansi: false,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_request_rejects_malformed_forms_without_leaking_final_byte() {
        for input in [
            b"\x1b[?$pA".as_slice(),
            b"\x1b[?7;8$pA".as_slice(),
            b"\x1b[?7;$pA".as_slice(),
            b"\x1b[?7pA".as_slice(),
            b"\x1b[4$pA".as_slice(),
            b"\x1b[>7$pA".as_slice(),
            b"\x1b[?7:8$pA".as_slice(),
            b"\x1b[?7$$pA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_dollar_intermediate_does_not_enable_existing_command_families() {
        for input in [
            b"\x1b[4$hA".as_slice(),
            b"\x1b[?7$hA".as_slice(),
            b"\x1b[3$AA".as_slice(),
            b"\x1b[2$JA".as_slice(),
            b"\x1b[2$KA".as_slice(),
            b"\x1b[0$WA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_sgr_dispatches_basic_and_color_attributes() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[mA\x1b[1;3;31;44mB\x1b[38;2;1;2;3mC\x1b[58;2;4;5;6mD",
        );

        assert_eq!(
            actions(&handler),
            &[
                Action::SetAttribute {
                    attr: sgr::Attribute::Unset,
                },
                Action::Print { cp: 'A' },
                Action::SetAttribute {
                    attr: sgr::Attribute::Bold,
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::Italic,
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::PaletteFg(1),
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::PaletteBg(4),
                },
                Action::Print { cp: 'B' },
                Action::SetAttribute {
                    attr: sgr::Attribute::DirectColorFg(color::Rgb::new(1, 2, 3)),
                },
                Action::Print { cp: 'C' },
                Action::SetAttribute {
                    attr: sgr::Attribute::UnderlineColor(color::Rgb::new(4, 5, 6)),
                },
                Action::Print { cp: 'D' },
            ]
        );
    }

    #[test]
    fn stream_csi_sgr_dispatches_colon_forms() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[4:3mA\x1b[38:2:0:1:2:3mB\x1b[58:2:0:4:5:6mC",
        );

        assert_eq!(
            actions(&handler),
            &[
                Action::SetAttribute {
                    attr: sgr::Attribute::Underline(sgr::Underline::Curly),
                },
                Action::Print { cp: 'A' },
                Action::SetAttribute {
                    attr: sgr::Attribute::DirectColorFg(color::Rgb::new(1, 2, 3)),
                },
                Action::Print { cp: 'B' },
                Action::SetAttribute {
                    attr: sgr::Attribute::UnderlineColor(color::Rgb::new(4, 5, 6)),
                },
                Action::Print { cp: 'C' },
            ]
        );
    }

    #[test]
    fn stream_csi_sgr_pins_empty_trailing_and_malformed_forms() {
        for (input, expected) in [
            (
                b"\x1b[;mA".as_slice(),
                vec![
                    Action::SetAttribute {
                        attr: sgr::Attribute::Unset,
                    },
                    Action::Print { cp: 'A' },
                ],
            ),
            (
                b"\x1b[1;mA".as_slice(),
                vec![
                    Action::SetAttribute {
                        attr: sgr::Attribute::Bold,
                    },
                    Action::Print { cp: 'A' },
                ],
            ),
            (
                b"\x1b[1;;31mA".as_slice(),
                vec![
                    Action::SetAttribute {
                        attr: sgr::Attribute::Bold,
                    },
                    Action::SetAttribute {
                        attr: sgr::Attribute::Unset,
                    },
                    Action::SetAttribute {
                        attr: sgr::Attribute::PaletteFg(1),
                    },
                    Action::Print { cp: 'A' },
                ],
            ),
            (b"\x1b[58:4:mA".as_slice(), vec![Action::Print { cp: 'A' }]),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), expected.as_slice());
        }
    }

    #[test]
    fn stream_csi_sgr_dispatches_kakoune_long_underline_color_sequence() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b[4:3;38;2;51;51;51;48;2;170;170;170;58;2;255;97;136;0mA",
        );

        assert_eq!(
            actions(&handler),
            &[
                Action::SetAttribute {
                    attr: sgr::Attribute::Underline(sgr::Underline::Curly),
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::DirectColorFg(color::Rgb::new(51, 51, 51)),
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::DirectColorBg(color::Rgb::new(170, 170, 170)),
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::UnderlineColor(color::Rgb::new(255, 97, 136)),
                },
                Action::SetAttribute {
                    attr: sgr::Attribute::Unset,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_csi_sgr_rejects_private_intermediate_and_preserves_colon_rejections() {
        for input in [
            b"\x1b[?1mA".as_slice(),
            b"\x1b[1$mA".as_slice(),
            b"\x1b[3:4AA".as_slice(),
            b"\x1b[2:3JA".as_slice(),
            b"\x1b[4:20hA".as_slice(),
            b"\x1b[?7:2004$pA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_sgr_split_feed_dispatches() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b[38:2:");
        next_slice(&mut stream, &mut handler, b"0:1:2:3");
        next_slice(&mut stream, &mut handler, b"mA");

        assert_eq!(
            actions(&handler),
            &[
                Action::SetAttribute {
                    attr: sgr::Attribute::DirectColorFg(color::Rgb::new(1, 2, 3)),
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_csi_sgr_restores_ground_before_handler_error() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnActionHandler::new(Action::SetAttribute {
            attr: sgr::Attribute::Bold,
        });

        assert_eq!(stream.next_slice(b"\x1b[1m", &mut handler), Err(()));
        stream.next_slice(b"A", &mut handler).unwrap();

        assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_csi_mode_request_split_feed_dispatches() {
        for parts in [
            [b"\x1b[?7".as_slice(), b"$pA".as_slice()],
            [b"\x1b[?7$".as_slice(), b"pA".as_slice()],
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, parts[0]);
            next_slice(&mut stream, &mut handler, parts[1]);

            assert_eq!(
                actions(&handler),
                &[
                    Action::RequestMode {
                        mode: modes::Mode::Wraparound,
                    },
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_mode_request_flushes_pending_invalid_utf8_first() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0");
        next_slice(&mut stream, &mut handler, b"\x1b[?7$pA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::RequestMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_csi_erase_display_dispatches_modes() {
        for (input, expected) in [
            (
                b"A\x1b[JB".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
            ),
            (
                b"\x1b[0JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
            ),
            (
                b"\x1b[;JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
            ),
            (
                b"\x1b[1JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Above,
                    protected: false,
                },
            ),
            (
                b"\x1b[1;JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Above,
                    protected: false,
                },
            ),
            (
                b"\x1b[2JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Complete,
                    protected: false,
                },
            ),
            (
                b"\x1b[3JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Scrollback,
                    protected: false,
                },
            ),
            (
                b"\x1b[22JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::ScrollComplete,
                    protected: false,
                },
            ),
            (
                b"\x1b[?2JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Complete,
                    protected: true,
                },
            ),
            (
                b"\x1b[?;JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: true,
                },
            ),
            (
                b"\x1b[?1;JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Above,
                    protected: true,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_erase_line_dispatches_modes() {
        for (input, expected) in [
            (
                b"A\x1b[KB".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
            ),
            (
                b"\x1b[0KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
            ),
            (
                b"\x1b[;KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
            ),
            (
                b"\x1b[1KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: false,
                },
            ),
            (
                b"\x1b[1;KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: false,
                },
            ),
            (
                b"\x1b[2KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Complete,
                    protected: false,
                },
            ),
            (
                b"\x1b[?KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: true,
                },
            ),
            (
                b"\x1b[?0KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: true,
                },
            ),
            (
                b"\x1b[?;KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: true,
                },
            ),
            (
                b"\x1b[?1KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: true,
                },
            ),
            (
                b"\x1b[?1;KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: true,
                },
            ),
            (
                b"\x1b[?2KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Complete,
                    protected: true,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_delete_chars_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[PB".as_slice(), Action::DeleteChars { count: 1 }),
            (b"\x1b[PA".as_slice(), Action::DeleteChars { count: 1 }),
            (b"\x1b[0PA".as_slice(), Action::DeleteChars { count: 0 }),
            (b"\x1b[;PA".as_slice(), Action::DeleteChars { count: 0 }),
            (b"\x1b[1PA".as_slice(), Action::DeleteChars { count: 1 }),
            (b"\x1b[1;PA".as_slice(), Action::DeleteChars { count: 1 }),
            (b"\x1b[3PA".as_slice(), Action::DeleteChars { count: 3 }),
            (
                b"\x1b[999999999999999999999999PA".as_slice(),
                Action::DeleteChars { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_insert_chars_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[@B".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[@A".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[0@A".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[;@A".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[1@A".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[1;@A".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[3@A".as_slice(), Action::InsertChars { count: 3 }),
            (
                b"\x1b[999999999999999999999999@A".as_slice(),
                Action::InsertChars { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_erase_chars_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[XB".as_slice(), Action::EraseChars { count: 1 }),
            (b"\x1b[XA".as_slice(), Action::EraseChars { count: 1 }),
            (b"\x1b[0XA".as_slice(), Action::EraseChars { count: 0 }),
            (b"\x1b[;XA".as_slice(), Action::EraseChars { count: 0 }),
            (b"\x1b[1XA".as_slice(), Action::EraseChars { count: 1 }),
            (b"\x1b[1;XA".as_slice(), Action::EraseChars { count: 1 }),
            (b"\x1b[3XA".as_slice(), Action::EraseChars { count: 3 }),
            (
                b"\x1b[999999999999999999999999XA".as_slice(),
                Action::EraseChars { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_insert_lines_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[LB".as_slice(), Action::InsertLines { count: 1 }),
            (b"\x1b[LA".as_slice(), Action::InsertLines { count: 1 }),
            (b"\x1b[0LA".as_slice(), Action::InsertLines { count: 0 }),
            (b"\x1b[;LA".as_slice(), Action::InsertLines { count: 0 }),
            (b"\x1b[1LA".as_slice(), Action::InsertLines { count: 1 }),
            (b"\x1b[1;LA".as_slice(), Action::InsertLines { count: 1 }),
            (b"\x1b[3LA".as_slice(), Action::InsertLines { count: 3 }),
            (
                b"\x1b[999999999999999999999999LA".as_slice(),
                Action::InsertLines { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_delete_lines_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[MB".as_slice(), Action::DeleteLines { count: 1 }),
            (b"\x1b[MA".as_slice(), Action::DeleteLines { count: 1 }),
            (b"\x1b[0MA".as_slice(), Action::DeleteLines { count: 0 }),
            (b"\x1b[;MA".as_slice(), Action::DeleteLines { count: 0 }),
            (b"\x1b[1MA".as_slice(), Action::DeleteLines { count: 1 }),
            (b"\x1b[1;MA".as_slice(), Action::DeleteLines { count: 1 }),
            (b"\x1b[3MA".as_slice(), Action::DeleteLines { count: 3 }),
            (
                b"\x1b[999999999999999999999999MA".as_slice(),
                Action::DeleteLines { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_scroll_up_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[SB".as_slice(), Action::ScrollUp { count: 1 }),
            (b"\x1b[SA".as_slice(), Action::ScrollUp { count: 1 }),
            (b"\x1b[0SA".as_slice(), Action::ScrollUp { count: 0 }),
            (b"\x1b[;SA".as_slice(), Action::ScrollUp { count: 0 }),
            (b"\x1b[1SA".as_slice(), Action::ScrollUp { count: 1 }),
            (b"\x1b[1;SA".as_slice(), Action::ScrollUp { count: 1 }),
            (b"\x1b[3SA".as_slice(), Action::ScrollUp { count: 3 }),
            (
                b"\x1b[999999999999999999999999SA".as_slice(),
                Action::ScrollUp { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
    }

    #[test]
    fn stream_csi_scroll_down_dispatches_counts() {
        for (input, expected) in [
            (b"A\x1b[TB".as_slice(), Action::ScrollDown { count: 1 }),
            (b"\x1b[TA".as_slice(), Action::ScrollDown { count: 1 }),
            (b"\x1b[0TA".as_slice(), Action::ScrollDown { count: 0 }),
            (b"\x1b[;TA".as_slice(), Action::ScrollDown { count: 0 }),
            (b"\x1b[1TA".as_slice(), Action::ScrollDown { count: 1 }),
            (b"\x1b[1;TA".as_slice(), Action::ScrollDown { count: 1 }),
            (b"\x1b[3TA".as_slice(), Action::ScrollDown { count: 3 }),
            (
                b"\x1b[999999999999999999999999TA".as_slice(),
                Action::ScrollDown { count: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            if input.starts_with(b"A") {
                assert_eq!(
                    actions(&handler),
                    &[
                        Action::Print { cp: 'A' },
                        expected,
                        Action::Print { cp: 'B' },
                    ]
                );
            } else {
                assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
            }
        }
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
                Action::HorizontalTab { count: 1 },
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
    fn stream_split_csi_horizontal_tabulation_dispatches_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"IA".as_slice(),
                Action::HorizontalTab { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"IA".as_slice(),
                Action::HorizontalTab { count: 3 },
            ),
            (
                b"\x1b[;".as_slice(),
                b"IA".as_slice(),
                Action::HorizontalTab { count: 0 },
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
    fn stream_split_csi_horizontal_tab_back_dispatches_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"ZA".as_slice(),
                Action::HorizontalTabBack { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"ZA".as_slice(),
                Action::HorizontalTabBack { count: 3 },
            ),
            (
                b"\x1b[;".as_slice(),
                b"ZA".as_slice(),
                Action::HorizontalTabBack { count: 0 },
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
    fn stream_split_csi_mode_set_and_reset_dispatch_actions() {
        for (first, second, expected) in [
            (
                b"\x1b[4".as_slice(),
                b"hA".as_slice(),
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
            ),
            (
                b"\x1b[?6".as_slice(),
                b"lA".as_slice(),
                Action::ResetMode {
                    mode: modes::Mode::Origin,
                },
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
    fn stream_split_csi_erase_display_dispatches_modes() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
            ),
            (
                b"\x1b[22".as_slice(),
                b"JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::ScrollComplete,
                    protected: false,
                },
            ),
            (
                b"\x1b[?".as_slice(),
                b"2JA".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Complete,
                    protected: true,
                },
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
    fn stream_split_csi_erase_line_dispatches_modes() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
            ),
            (
                b"\x1b[2".as_slice(),
                b"KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Complete,
                    protected: false,
                },
            ),
            (
                b"\x1b[?".as_slice(),
                b"1KA".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: true,
                },
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
    fn stream_split_csi_delete_chars_dispatches_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"PA".as_slice(),
                Action::DeleteChars { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"PA".as_slice(),
                Action::DeleteChars { count: 3 },
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
    fn stream_split_csi_insert_and_erase_chars_dispatch_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"@A".as_slice(),
                Action::InsertChars { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"@A".as_slice(),
                Action::InsertChars { count: 3 },
            ),
            (
                b"\x1b[".as_slice(),
                b"XA".as_slice(),
                Action::EraseChars { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"XA".as_slice(),
                Action::EraseChars { count: 3 },
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
    fn stream_split_csi_insert_lines_dispatches_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"LA".as_slice(),
                Action::InsertLines { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"LA".as_slice(),
                Action::InsertLines { count: 3 },
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
    fn stream_split_csi_delete_lines_dispatches_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"MA".as_slice(),
                Action::DeleteLines { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"MA".as_slice(),
                Action::DeleteLines { count: 3 },
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
    fn stream_split_csi_scroll_up_and_down_dispatch_counts() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"SA".as_slice(),
                Action::ScrollUp { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"SA".as_slice(),
                Action::ScrollUp { count: 3 },
            ),
            (
                b"\x1b[".as_slice(),
                b"TA".as_slice(),
                Action::ScrollDown { count: 1 },
            ),
            (
                b"\x1b[3".as_slice(),
                b"TA".as_slice(),
                Action::ScrollDown { count: 3 },
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
    fn stream_csi_horizontal_absolute_dispatches_default_column() {
        for input in [b"A\x1b[GB".as_slice(), b"A\x1b[`B".as_slice()] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    Action::CursorColumn { col: 1 },
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_horizontal_absolute_dispatches_explicit_zero_and_overflowing_columns() {
        for (input, expected) in [
            (b"\x1b[5GA".as_slice(), Action::CursorColumn { col: 5 }),
            (b"\x1b[6`A".as_slice(), Action::CursorColumn { col: 6 }),
            (b"\x1b[0GA".as_slice(), Action::CursorColumn { col: 0 }),
            (b"\x1b[0`A".as_slice(), Action::CursorColumn { col: 0 }),
            (
                b"\x1b[999999999999999999999999GA".as_slice(),
                Action::CursorColumn { col: u16::MAX },
            ),
            (
                b"\x1b[999999999999999999999999`A".as_slice(),
                Action::CursorColumn { col: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_vertical_positioning_dispatches_default_values() {
        for (input, expected) in [
            (b"A\x1b[dB".as_slice(), Action::CursorRow { row: 1 }),
            (
                b"A\x1b[eB".as_slice(),
                Action::CursorRowRelative { rows: 1 },
            ),
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
    fn stream_csi_vertical_positioning_dispatches_explicit_zero_and_overflowing_values() {
        for (input, expected) in [
            (b"\x1b[5dA".as_slice(), Action::CursorRow { row: 5 }),
            (
                b"\x1b[6eA".as_slice(),
                Action::CursorRowRelative { rows: 6 },
            ),
            (b"\x1b[0dA".as_slice(), Action::CursorRow { row: 0 }),
            (
                b"\x1b[0eA".as_slice(),
                Action::CursorRowRelative { rows: 0 },
            ),
            (
                b"\x1b[999999999999999999999999dA".as_slice(),
                Action::CursorRow { row: u16::MAX },
            ),
            (
                b"\x1b[999999999999999999999999eA".as_slice(),
                Action::CursorRowRelative { rows: u16::MAX },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_cursor_position_dispatches_default_values() {
        for input in [b"A\x1b[HB".as_slice(), b"A\x1b[fB".as_slice()] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print { cp: 'A' },
                    Action::CursorPosition { row: 1, col: 1 },
                    Action::Print { cp: 'B' },
                ]
            );
        }
    }

    #[test]
    fn stream_csi_cursor_position_dispatches_params_and_empty_semantics() {
        for (input, expected) in [
            (
                b"\x1b[5HA".as_slice(),
                Action::CursorPosition { row: 5, col: 1 },
            ),
            (
                b"\x1b[5;6HA".as_slice(),
                Action::CursorPosition { row: 5, col: 6 },
            ),
            (
                b"\x1b[5;6fA".as_slice(),
                Action::CursorPosition { row: 5, col: 6 },
            ),
            (
                b"\x1b[0;0HA".as_slice(),
                Action::CursorPosition { row: 0, col: 0 },
            ),
            (
                b"\x1b[;HA".as_slice(),
                Action::CursorPosition { row: 0, col: 1 },
            ),
            (
                b"\x1b[5;HA".as_slice(),
                Action::CursorPosition { row: 5, col: 1 },
            ),
            (
                b"\x1b[;7HA".as_slice(),
                Action::CursorPosition { row: 0, col: 7 },
            ),
            (
                b"\x1b[;;HA".as_slice(),
                Action::CursorPosition { row: 0, col: 0 },
            ),
            (
                b"\x1b[5;;HA".as_slice(),
                Action::CursorPosition { row: 5, col: 0 },
            ),
            (
                b"\x1b[5;6;HA".as_slice(),
                Action::CursorPosition { row: 5, col: 6 },
            ),
            (
                b"\x1b[999999999999999999999999;999999999999999999999999HA".as_slice(),
                Action::CursorPosition {
                    row: u16::MAX,
                    col: u16::MAX,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[expected, Action::Print { cp: 'A' }]);
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
    fn stream_split_csi_horizontal_absolute_dispatches_actions() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"GA".as_slice(),
                Action::CursorColumn { col: 1 },
            ),
            (
                b"\x1b[5".as_slice(),
                b"GA".as_slice(),
                Action::CursorColumn { col: 5 },
            ),
            (
                b"\x1b[".as_slice(),
                b"`A".as_slice(),
                Action::CursorColumn { col: 1 },
            ),
            (
                b"\x1b[0".as_slice(),
                b"`A".as_slice(),
                Action::CursorColumn { col: 0 },
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
    fn stream_split_csi_vertical_positioning_dispatches_actions() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"dA".as_slice(),
                Action::CursorRow { row: 1 },
            ),
            (
                b"\x1b[5".as_slice(),
                b"dA".as_slice(),
                Action::CursorRow { row: 5 },
            ),
            (
                b"\x1b[".as_slice(),
                b"eA".as_slice(),
                Action::CursorRowRelative { rows: 1 },
            ),
            (
                b"\x1b[0".as_slice(),
                b"eA".as_slice(),
                Action::CursorRowRelative { rows: 0 },
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
    fn stream_split_csi_cursor_position_dispatches_actions() {
        for (first, second, expected) in [
            (
                b"\x1b[".as_slice(),
                b"HA".as_slice(),
                Action::CursorPosition { row: 1, col: 1 },
            ),
            (
                b"\x1b[5".as_slice(),
                b"HA".as_slice(),
                Action::CursorPosition { row: 5, col: 1 },
            ),
            (
                b"\x1b[5;".as_slice(),
                b"6HA".as_slice(),
                Action::CursorPosition { row: 5, col: 6 },
            ),
            (
                b"\x1b[;".as_slice(),
                b"fA".as_slice(),
                Action::CursorPosition { row: 0, col: 1 },
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
    fn stream_pending_utf8_replacement_dispatches_before_csi_horizontal_tabulation() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[IA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::HorizontalTab { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_horizontal_tabulation() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"IA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::HorizontalTab { count: 3 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_horizontal_tab_back() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[ZA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::HorizontalTabBack { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_horizontal_tab_back() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"ZA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::HorizontalTabBack { count: 3 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_mode_set() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[4hA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_mode_reset() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[?6");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"lA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::ResetMode {
                    mode: modes::Mode::Origin,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_horizontal_absolute() {
        for (input, expected) in [
            (
                b"\xf0\x9f\x1b[GA".as_slice(),
                Action::CursorColumn { col: 1 },
            ),
            (
                b"\xf0\x9f\x1b[`A".as_slice(),
                Action::CursorColumn { col: 1 },
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
    fn stream_pending_utf8_replacement_dispatches_before_csi_vertical_positioning() {
        for (input, expected) in [
            (b"\xf0\x9f\x1b[dA".as_slice(), Action::CursorRow { row: 1 }),
            (
                b"\xf0\x9f\x1b[eA".as_slice(),
                Action::CursorRowRelative { rows: 1 },
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
    fn stream_pending_utf8_replacement_dispatches_before_csi_erase_display() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[JA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_erase_line() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[KA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_delete_chars() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[PA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::DeleteChars { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_insert_and_erase_chars() {
        for (input, expected) in [
            (
                b"\xf0\x9f\x1b[@A".as_slice(),
                Action::InsertChars { count: 1 },
            ),
            (
                b"\xf0\x9f\x1b[XA".as_slice(),
                Action::EraseChars { count: 1 },
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
    fn stream_pending_utf8_replacement_dispatches_before_csi_insert_lines() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[LA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::InsertLines { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_delete_lines() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[MA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::DeleteLines { count: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_scroll_up_and_down() {
        for (input, expected) in [
            (b"\xf0\x9f\x1b[SA".as_slice(), Action::ScrollUp { count: 1 }),
            (
                b"\xf0\x9f\x1b[TA".as_slice(),
                Action::ScrollDown { count: 1 },
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
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_erase_display() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[22");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"JA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::EraseDisplay {
                    mode: EraseDisplayMode::ScrollComplete,
                    protected: false,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_erase_line() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[2");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"KA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::EraseLine {
                    mode: EraseLineMode::Complete,
                    protected: false,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_delete_chars() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"PA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::DeleteChars { count: 3 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_insert_and_erase_chars() {
        for (second, expected) in [
            (b"@A".as_slice(), Action::InsertChars { count: 3 }),
            (b"XA".as_slice(), Action::EraseChars { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
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
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_insert_lines() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"LA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::InsertLines { count: 3 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_delete_lines() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"MA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::DeleteLines { count: 3 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_scroll_up_and_down() {
        for (second, expected) in [
            (b"SA".as_slice(), Action::ScrollUp { count: 3 }),
            (b"TA".as_slice(), Action::ScrollDown { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[3");
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
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_horizontal_absolute() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"GA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CursorColumn { col: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_vertical_positioning() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"dA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CursorRow { row: 1 },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_csi_cursor_position() {
        for input in [b"\xf0\x9f\x1b[HA".as_slice(), b"\xf0\x9f\x1b[fA".as_slice()] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    Action::CursorPosition { row: 1, col: 1 },
                    Action::Print { cp: 'A' },
                ]
            );
        }
    }

    #[test]
    fn stream_pending_utf8_replacement_dispatches_before_split_csi_cursor_position() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\xf0\x9f\x1b[5;");
        assert_eq!(
            actions(&handler),
            &[Action::Print {
                cp: char::REPLACEMENT_CHARACTER,
            }]
        );

        next_slice(&mut stream, &mut handler, b"6HA");

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::CursorPosition { row: 5, col: 6 },
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
            b"\x1b[5;CA".as_slice(),
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
            b"\x1b[5;EA".as_slice(),
            b"\x1b[5;FA".as_slice(),
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
    fn stream_unsupported_csi_horizontal_tabulation_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3IA".as_slice(),
            b"\x1b[>3IA".as_slice(),
            b"\x1b[5;4IA".as_slice(),
            b"\x1b[1:2IA".as_slice(),
            b"\x1b[1;2:3IA".as_slice(),
            b"\x1b[ IA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_horizontal_absolute_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3GA".as_slice(),
            b"\x1b[?3`A".as_slice(),
            b"\x1b[>3GA".as_slice(),
            b"\x1b[>3`A".as_slice(),
            b"\x1b[5;4GA".as_slice(),
            b"\x1b[5;4`A".as_slice(),
            b"\x1b[5;GA".as_slice(),
            b"\x1b[5;`A".as_slice(),
            b"\x1b[1:2GA".as_slice(),
            b"\x1b[1:2`A".as_slice(),
            b"\x1b[ GA".as_slice(),
            b"\x1b[ `A".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_vertical_positioning_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3dA".as_slice(),
            b"\x1b[?3eA".as_slice(),
            b"\x1b[>3dA".as_slice(),
            b"\x1b[>3eA".as_slice(),
            b"\x1b[5;4dA".as_slice(),
            b"\x1b[5;4eA".as_slice(),
            b"\x1b[5;dA".as_slice(),
            b"\x1b[5;eA".as_slice(),
            b"\x1b[1:2dA".as_slice(),
            b"\x1b[1:2eA".as_slice(),
            b"\x1b[ dA".as_slice(),
            b"\x1b[ eA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_horizontal_tab_back_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?ZA".as_slice(),
            b"\x1b[>ZA".as_slice(),
            b"\x1b[5;4ZA".as_slice(),
            b"\x1b[5;;ZA".as_slice(),
            b"\x1b[1:2ZA".as_slice(),
            b"\x1b[1;2:3ZA".as_slice(),
            b"\x1b[ ZA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_mode_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[>4hA".as_slice(),
            b"\x1b[>4lA".as_slice(),
            b"\x1b[4 hA".as_slice(),
            b"\x1b[4 lA".as_slice(),
            b"\x1b[4:20hA".as_slice(),
            b"\x1b[?6:7lA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_erase_display_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[>3JA".as_slice(),
            b"\x1b[5;4JA".as_slice(),
            b"\x1b[5;;JA".as_slice(),
            b"\x1b[1:2JA".as_slice(),
            b"\x1b[1;2:3JA".as_slice(),
            b"\x1b[4JA".as_slice(),
            b"\x1b[23JA".as_slice(),
            b"\x1b[ JA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_erase_line_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[>2KA".as_slice(),
            b"\x1b[5;4KA".as_slice(),
            b"\x1b[5;;KA".as_slice(),
            b"\x1b[1:2KA".as_slice(),
            b"\x1b[1;2:3KA".as_slice(),
            b"\x1b[3KA".as_slice(),
            b"\x1b[4KA".as_slice(),
            b"\x1b[999KA".as_slice(),
            b"\x1b[ KA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_delete_chars_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?PA".as_slice(),
            b"\x1b[>PA".as_slice(),
            b"\x1b[5;4PA".as_slice(),
            b"\x1b[5;;PA".as_slice(),
            b"\x1b[1:2PA".as_slice(),
            b"\x1b[1;2:3PA".as_slice(),
            b"\x1b[ PA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_insert_and_erase_chars_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?@A".as_slice(),
            b"\x1b[>@A".as_slice(),
            b"\x1b[5;4@A".as_slice(),
            b"\x1b[5;;@A".as_slice(),
            b"\x1b[1:2@A".as_slice(),
            b"\x1b[1;2:3@A".as_slice(),
            b"\x1b[ @A".as_slice(),
            b"\x1b[?XA".as_slice(),
            b"\x1b[>XA".as_slice(),
            b"\x1b[5;4XA".as_slice(),
            b"\x1b[5;;XA".as_slice(),
            b"\x1b[1:2XA".as_slice(),
            b"\x1b[1;2:3XA".as_slice(),
            b"\x1b[ XA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_insert_lines_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?LA".as_slice(),
            b"\x1b[>LA".as_slice(),
            b"\x1b[5;4LA".as_slice(),
            b"\x1b[5;;LA".as_slice(),
            b"\x1b[1:2LA".as_slice(),
            b"\x1b[1;2:3LA".as_slice(),
            b"\x1b[ LA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_delete_lines_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?MA".as_slice(),
            b"\x1b[>MA".as_slice(),
            b"\x1b[5;4MA".as_slice(),
            b"\x1b[5;;MA".as_slice(),
            b"\x1b[1:2MA".as_slice(),
            b"\x1b[1;2:3MA".as_slice(),
            b"\x1b[ MA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_scroll_up_and_down_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?SA".as_slice(),
            b"\x1b[>SA".as_slice(),
            b"\x1b[5;4SA".as_slice(),
            b"\x1b[5;;SA".as_slice(),
            b"\x1b[1:2SA".as_slice(),
            b"\x1b[1;2:3SA".as_slice(),
            b"\x1b[ SA".as_slice(),
            b"\x1b[?TA".as_slice(),
            b"\x1b[>TA".as_slice(),
            b"\x1b[5;4TA".as_slice(),
            b"\x1b[5;;TA".as_slice(),
            b"\x1b[1:2TA".as_slice(),
            b"\x1b[1;2:3TA".as_slice(),
            b"\x1b[ TA".as_slice(),
        ] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, input);

            assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_unsupported_csi_cursor_position_variants_do_not_dispatch_actions() {
        for input in [
            b"\x1b[?3HA".as_slice(),
            b"\x1b[?3fA".as_slice(),
            b"\x1b[>3HA".as_slice(),
            b"\x1b[>3fA".as_slice(),
            b"\x1b[5;6;7HA".as_slice(),
            b"\x1b[5;6;7fA".as_slice(),
            b"\x1b[1:2HA".as_slice(),
            b"\x1b[1:2fA".as_slice(),
            b"\x1b[1;2:3HA".as_slice(),
            b"\x1b[1;2:3fA".as_slice(),
            b"\x1b[ HA".as_slice(),
            b"\x1b[ fA".as_slice(),
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
    fn stream_raw_c1_csi_byte_does_not_dispatch_horizontal_tabulation_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'I']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'I' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_horizontal_tab_back_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'Z']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'Z' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_mode_actions() {
        for final_byte in [b'h', b'l'] {
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
    fn stream_raw_c1_csi_byte_does_not_dispatch_mode_save_restore_actions() {
        for final_byte in [b's', b'r'] {
            let mut stream = Stream::init();
            let mut handler = RecordingHandler::default();

            next_slice(&mut stream, &mut handler, &[0x9b, b'?', b'7', final_byte]);

            assert_eq!(
                actions(&handler),
                &[
                    Action::Print {
                        cp: char::REPLACEMENT_CHARACTER,
                    },
                    Action::Print { cp: '?' },
                    Action::Print { cp: '7' },
                    Action::Print {
                        cp: final_byte as char,
                    },
                ]
            );
        }
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_mode_request_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'?', b'7', b'$', b'p']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: '?' },
                Action::Print { cp: '7' },
                Action::Print { cp: '$' },
                Action::Print { cp: 'p' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_erase_display_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'J']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'J' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_erase_line_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'K']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'K' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_delete_chars_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'P']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'P' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_insert_or_erase_chars_action() {
        for final_byte in [b'@', b'X'] {
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
                        cp: char::from(final_byte),
                    },
                ]
            );
        }
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_insert_lines_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'L']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'L' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_delete_lines_action() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, &[0x9b, b'M']);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print {
                    cp: char::REPLACEMENT_CHARACTER,
                },
                Action::Print { cp: 'M' },
            ]
        );
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_scroll_up_or_down_action() {
        for final_byte in [b'S', b'T'] {
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
                        cp: char::from(final_byte),
                    },
                ]
            );
        }
    }

    #[test]
    fn stream_raw_c1_csi_byte_does_not_dispatch_horizontal_absolute_actions() {
        for final_byte in [b'G', b'`'] {
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
    fn stream_raw_c1_csi_byte_does_not_dispatch_vertical_positioning_actions() {
        for final_byte in [b'd', b'e'] {
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
    fn stream_raw_c1_csi_byte_does_not_dispatch_cursor_position_actions() {
        for final_byte in [b'H', b'f'] {
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

    #[test]
    fn stream_osc_dispatches_basic_commands() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b]0;zero\x07\x1b]2;two\x1b\\\x1b]1;icon\x07\x1b]7;file://host/p\x07",
        );

        assert_eq!(
            osc_actions(&handler),
            &[
                OwnedOscAction::WindowTitle {
                    title: "zero".to_string(),
                },
                OwnedOscAction::WindowTitle {
                    title: "two".to_string(),
                },
                OwnedOscAction::ReportPwd {
                    url: "file://host/p".to_string(),
                },
            ]
        );
        assert_eq!(actions(&handler), &[]);
    }

    #[test]
    fn stream_osc_dispatches_hyperlinks() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(
            &mut stream,
            &mut handler,
            b"\x1b]8;;https://implicit\x1b\\\x1b]8;id=explicit;https://explicit\x07\x1b]8;;\x1b\\",
        );

        assert_eq!(
            osc_actions(&handler),
            &[
                OwnedOscAction::StartHyperlink {
                    id: None,
                    uri: "https://implicit".to_string(),
                },
                OwnedOscAction::StartHyperlink {
                    id: Some("explicit".to_string()),
                    uri: "https://explicit".to_string(),
                },
                OwnedOscAction::EndHyperlink,
            ]
        );
        assert_eq!(actions(&handler), &[]);
    }

    #[test]
    fn stream_osc_consumes_invalid_unsupported_and_over_capacity_without_leak() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();
        let mut input = b"A\x1b]9;notify\x07B\x1b]8;bad=value;https://bad\x1b\\C\x1b]0;".to_vec();
        input.extend(std::iter::repeat_n(b'x', osc::MAX_BUF + 1));
        input.extend_from_slice(b"\x07D\x1b]0;\xff\x07E");

        next_slice(&mut stream, &mut handler, &input);

        assert_eq!(
            actions(&handler),
            &[
                Action::Print { cp: 'A' },
                Action::Print { cp: 'B' },
                Action::Print { cp: 'C' },
                Action::Print { cp: 'D' },
                Action::Print { cp: 'E' },
            ]
        );
        assert_eq!(osc_actions(&handler), &[]);
    }

    #[test]
    fn stream_osc_split_feed_and_nested_escape_behavior() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b]");
        next_slice(&mut stream, &mut handler, b"0;split");
        next_slice(&mut stream, &mut handler, b"\x1b");
        next_slice(&mut stream, &mut handler, b"\\A");
        next_slice(&mut stream, &mut handler, b"\x1b]0;bad\x1b]ignored");
        next_slice(&mut stream, &mut handler, b"\x07B");

        assert_eq!(
            osc_actions(&handler),
            &[OwnedOscAction::WindowTitle {
                title: "split".to_string(),
            }]
        );
        assert_eq!(
            actions(&handler),
            &[Action::Print { cp: 'A' }, Action::Print { cp: 'B' }]
        );
    }

    #[test]
    fn stream_osc_bel_terminates_after_pending_escape() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b]0;title\x1b\x07A");

        assert_eq!(
            osc_actions(&handler),
            &[OwnedOscAction::WindowTitle {
                title: "title".to_string(),
            }]
        );
        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_osc_invalid_bel_terminates_after_pending_escape() {
        let mut stream = Stream::init();
        let mut handler = RecordingHandler::default();

        next_slice(&mut stream, &mut handler, b"\x1b]0;bad\x1b]\x1b\x07A");

        assert_eq!(osc_actions(&handler), &[]);
        assert_eq!(actions(&handler), &[Action::Print { cp: 'A' }]);
    }

    #[derive(Debug, Default)]
    struct ErrorOnOscHandler {
        fail: Option<OwnedOscAction>,
        osc_actions: Vec<OwnedOscAction>,
        actions: Vec<Action>,
    }

    impl ErrorOnOscHandler {
        fn new(fail: OwnedOscAction) -> Self {
            Self {
                fail: Some(fail),
                osc_actions: Vec::new(),
                actions: Vec::new(),
            }
        }
    }

    impl Handler for ErrorOnOscHandler {
        type Error = ();

        fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
            self.actions.push(action);
            Ok(())
        }

        fn osc(&mut self, action: OscAction<'_>) -> Result<(), Self::Error> {
            let owned = OwnedOscAction::from(action);
            if self.fail == Some(owned.clone()) {
                self.fail = None;
                return Err(());
            }
            self.osc_actions.push(owned);
            Ok(())
        }
    }

    #[test]
    fn stream_osc_restores_ground_before_handler_error() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnOscHandler::new(OwnedOscAction::WindowTitle {
            title: "fail".to_string(),
        });

        assert_eq!(stream.next_slice(b"\x1b]0;fail\x07", &mut handler), Err(()));
        stream.next_slice(b"A\x1b]0;ok\x07", &mut handler).unwrap();

        assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        assert_eq!(
            handler.osc_actions,
            &[OwnedOscAction::WindowTitle {
                title: "ok".to_string(),
            }]
        );
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
                | Action::HorizontalTab { .. }
                | Action::HorizontalTabBack { .. }
                | Action::TabSet
                | Action::TabClearCurrent
                | Action::TabClearAll
                | Action::TabReset => unreachable!("loop only uses D/E actions"),
                Action::CursorUp { .. }
                | Action::CursorDown { .. }
                | Action::CursorRight { .. }
                | Action::CursorLeft { .. }
                | Action::CursorColumn { .. }
                | Action::CursorRow { .. }
                | Action::CursorRowRelative { .. }
                | Action::CursorPosition { .. }
                | Action::EraseDisplay { .. }
                | Action::EraseLine { .. }
                | Action::InsertChars { .. }
                | Action::DeleteChars { .. }
                | Action::EraseChars { .. }
                | Action::InsertLines { .. }
                | Action::DeleteLines { .. }
                | Action::ScrollUp { .. }
                | Action::ScrollDown { .. }
                | Action::SetMode { .. }
                | Action::ResetMode { .. }
                | Action::SaveMode { .. }
                | Action::RestoreMode { .. }
                | Action::RequestMode { .. }
                | Action::RequestModeUnknown { .. }
                | Action::SetAttribute { .. } => unreachable!("loop only uses D/E actions"),
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
    fn stream_csi_horizontal_absolute_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[G".as_slice(), Action::CursorColumn { col: 1 }),
            (b"\x1b[`".as_slice(), Action::CursorColumn { col: 1 }),
            (b"\x1b[0G".as_slice(), Action::CursorColumn { col: 0 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_vertical_positioning_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[d".as_slice(), Action::CursorRow { row: 1 }),
            (b"\x1b[e".as_slice(), Action::CursorRowRelative { rows: 1 }),
            (b"\x1b[0d".as_slice(), Action::CursorRow { row: 0 }),
            (b"\x1b[0e".as_slice(), Action::CursorRowRelative { rows: 0 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_cursor_position_restore_ground_before_handler_error() {
        for (input, fail) in [
            (
                b"\x1b[H".as_slice(),
                Action::CursorPosition { row: 1, col: 1 },
            ),
            (
                b"\x1b[f".as_slice(),
                Action::CursorPosition { row: 1, col: 1 },
            ),
            (
                b"\x1b[0;0H".as_slice(),
                Action::CursorPosition { row: 0, col: 0 },
            ),
            (
                b"\x1b[5;6f".as_slice(),
                Action::CursorPosition { row: 5, col: 6 },
            ),
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
    fn stream_csi_horizontal_tabulation_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[I".as_slice(), Action::HorizontalTab { count: 1 }),
            (b"\x1b[0I".as_slice(), Action::HorizontalTab { count: 0 }),
            (b"\x1b[3;I".as_slice(), Action::HorizontalTab { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_horizontal_tab_back_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[Z".as_slice(), Action::HorizontalTabBack { count: 1 }),
            (
                b"\x1b[0Z".as_slice(),
                Action::HorizontalTabBack { count: 0 },
            ),
            (
                b"\x1b[3;Z".as_slice(),
                Action::HorizontalTabBack { count: 3 },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_set_and_reset_restore_ground_before_handler_error() {
        for (input, fail) in [
            (
                b"\x1b[4h".as_slice(),
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
            ),
            (
                b"\x1b[?6l".as_slice(),
                Action::ResetMode {
                    mode: modes::Mode::Origin,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_save_and_restore_restore_ground_before_handler_error() {
        for (input, fail) in [
            (
                b"\x1b[?7s".as_slice(),
                Action::SaveMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
            (
                b"\x1b[?7r".as_slice(),
                Action::RestoreMode {
                    mode: modes::Mode::Wraparound,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_mode_multi_action_dispatch_stops_after_first_failing_action() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnActionWithAttemptsHandler::new(Action::SetMode {
            mode: modes::Mode::Linefeed,
        });

        assert_eq!(stream.next_slice(b"\x1b[4;20;12h", &mut handler), Err(()));
        stream.next_slice(b"A", &mut handler).unwrap();

        assert_eq!(
            handler.attempts,
            &[
                Action::SetMode {
                    mode: modes::Mode::Insert,
                },
                Action::SetMode {
                    mode: modes::Mode::Linefeed,
                },
                Action::Print { cp: 'A' },
            ]
        );
    }

    #[test]
    fn stream_csi_mode_save_restore_multi_action_dispatch_stops_after_first_failing_action() {
        for (input, fail, expected_first) in [
            (
                b"\x1b[?1;7;2004s".as_slice(),
                Action::SaveMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::SaveMode {
                    mode: modes::Mode::CursorKeys,
                },
            ),
            (
                b"\x1b[?1;7;2004r".as_slice(),
                Action::RestoreMode {
                    mode: modes::Mode::Wraparound,
                },
                Action::RestoreMode {
                    mode: modes::Mode::CursorKeys,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionWithAttemptsHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(
                handler.attempts,
                &[expected_first, fail, Action::Print { cp: 'A' }]
            );
        }
    }

    #[test]
    fn stream_csi_mode_request_restores_ground_before_handler_error() {
        let mut stream = Stream::init();
        let mut handler = ErrorOnActionHandler::new(Action::RequestMode {
            mode: modes::Mode::Wraparound,
        });

        assert_eq!(stream.next_slice(b"\x1b[?7$p", &mut handler), Err(()));
        stream.next_slice(b"A", &mut handler).unwrap();

        assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
    }

    #[test]
    fn stream_csi_erase_display_restore_ground_before_handler_error() {
        for (input, fail) in [
            (
                b"\x1b[J".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Below,
                    protected: false,
                },
            ),
            (
                b"\x1b[?2J".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::Complete,
                    protected: true,
                },
            ),
            (
                b"\x1b[22J".as_slice(),
                Action::EraseDisplay {
                    mode: EraseDisplayMode::ScrollComplete,
                    protected: false,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_erase_line_restore_ground_before_handler_error() {
        for (input, fail) in [
            (
                b"\x1b[K".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Right,
                    protected: false,
                },
            ),
            (
                b"\x1b[?1K".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Left,
                    protected: true,
                },
            ),
            (
                b"\x1b[2K".as_slice(),
                Action::EraseLine {
                    mode: EraseLineMode::Complete,
                    protected: false,
                },
            ),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_delete_chars_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[P".as_slice(), Action::DeleteChars { count: 1 }),
            (b"\x1b[0P".as_slice(), Action::DeleteChars { count: 0 }),
            (b"\x1b[3P".as_slice(), Action::DeleteChars { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_insert_and_erase_chars_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[@".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[0@".as_slice(), Action::InsertChars { count: 1 }),
            (b"\x1b[3@".as_slice(), Action::InsertChars { count: 3 }),
            (b"\x1b[X".as_slice(), Action::EraseChars { count: 1 }),
            (b"\x1b[0X".as_slice(), Action::EraseChars { count: 0 }),
            (b"\x1b[3X".as_slice(), Action::EraseChars { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_insert_lines_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[L".as_slice(), Action::InsertLines { count: 1 }),
            (b"\x1b[0L".as_slice(), Action::InsertLines { count: 0 }),
            (b"\x1b[3L".as_slice(), Action::InsertLines { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_delete_lines_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[M".as_slice(), Action::DeleteLines { count: 1 }),
            (b"\x1b[0M".as_slice(), Action::DeleteLines { count: 0 }),
            (b"\x1b[3M".as_slice(), Action::DeleteLines { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
        }
    }

    #[test]
    fn stream_csi_scroll_up_and_down_restore_ground_before_handler_error() {
        for (input, fail) in [
            (b"\x1b[S".as_slice(), Action::ScrollUp { count: 1 }),
            (b"\x1b[0S".as_slice(), Action::ScrollUp { count: 0 }),
            (b"\x1b[3S".as_slice(), Action::ScrollUp { count: 3 }),
            (b"\x1b[T".as_slice(), Action::ScrollDown { count: 1 }),
            (b"\x1b[0T".as_slice(), Action::ScrollDown { count: 0 }),
            (b"\x1b[3T".as_slice(), Action::ScrollDown { count: 3 }),
        ] {
            let mut stream = Stream::init();
            let mut handler = ErrorOnActionHandler::new(fail);

            assert_eq!(stream.next_slice(input, &mut handler), Err(()));
            stream.next_slice(b"A", &mut handler).unwrap();

            assert_eq!(handler.actions, &[Action::Print { cp: 'A' }]);
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
