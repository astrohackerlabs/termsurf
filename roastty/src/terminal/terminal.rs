//! Terminal state.

use super::color;
use super::modes;
use super::page_list::{
    CodepointMapEntry, PageListAllocError, PageOutputFormat, PageStringWithPinMap,
};
use super::screen::{
    BasicPrintError, EraseDisplayError, Screen, ScreenCursorHyperlinkId, ScreenFormatter,
    ScreenFormatterContent, ScreenFormatterExtra, ScreenFormatterOptions,
};
use super::size::CellCountInt;
use super::stream::{self, Action, Handler};
#[cfg(test)]
use super::style;
use super::tabstops;

const TABSTOP_INTERVAL: usize = 8;

#[derive(Debug)]
pub(super) struct Terminal {
    size: TerminalSize,
    screens: TerminalScreens,
    colors: TerminalColors,
    modes: modes::ModeState,
    scrolling_region: ScrollingRegion,
    tabstops: tabstops::Tabstops,
    pty_response: Vec<u8>,
    stream: stream::Stream,
    flags: TerminalFlags,
    title: TerminalTitle,
    pwd: TerminalPwd,
    next_implicit_hyperlink_id: u32,
}

#[derive(Debug)]
pub(super) struct TerminalScreens {
    active: Screen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    cols: CellCountInt,
    rows: CellCountInt,
}

#[derive(Debug, Clone, Copy)]
struct TerminalColors {
    palette: color::Palette,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScrollingRegion {
    top: CellCountInt,
    bottom: CellCountInt,
    left: CellCountInt,
    right: CellCountInt,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct TerminalFlags {
    modify_other_keys_2: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TerminalTitle {
    text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct TerminalPwd {
    text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TerminalStreamError {
    PageAlloc,
    ManagedCellUnsupported,
    InvalidPoint,
    UnsupportedCodepoint(char),
}

struct TerminalStreamHandler<'a> {
    screen: &'a mut Screen,
    size: TerminalSize,
    modes: &'a mut modes::ModeState,
    scrolling_region: &'a mut ScrollingRegion,
    tabstops: &'a mut tabstops::Tabstops,
    pty_response: &'a mut Vec<u8>,
    title: &'a mut TerminalTitle,
    pwd: &'a mut TerminalPwd,
    next_implicit_hyperlink_id: &'a mut u32,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TerminalFormatterOptions<'a> {
    screen: ScreenFormatterOptions<'a>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TerminalFormatter<'a> {
    terminal: &'a Terminal,
    options: TerminalFormatterOptions<'a>,
    content: ScreenFormatterContent,
    extra: TerminalFormatterExtra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct TerminalFormatterExtra {
    palette: bool,
    modes: bool,
    scrolling_region: bool,
    tabstops: bool,
    keyboard: bool,
    pwd: bool,
    screen: ScreenFormatterExtra,
}

impl Terminal {
    pub(super) fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        let size = TerminalSize { cols, rows };
        Ok(Self {
            size,
            screens: TerminalScreens {
                active: Screen::init(cols, rows, max_scrollback_rows)?,
            },
            colors: TerminalColors {
                palette: color::DEFAULT_PALETTE,
            },
            modes: modes::ModeState::default(),
            scrolling_region: ScrollingRegion::full(size),
            tabstops: tabstops::Tabstops::new(cols as usize, TABSTOP_INTERVAL)
                .map_err(|_| PageListAllocError::PageAlloc)?,
            pty_response: Vec::new(),
            stream: stream::Stream::init(),
            flags: TerminalFlags::default(),
            title: TerminalTitle::default(),
            pwd: TerminalPwd::default(),
            next_implicit_hyperlink_id: 0,
        })
    }

    pub(super) fn next_slice(&mut self, input: &[u8]) -> Result<(), TerminalStreamError> {
        let Terminal {
            size,
            screens,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            stream,
            title,
            pwd,
            next_implicit_hyperlink_id,
            ..
        } = self;
        let mut handler = TerminalStreamHandler {
            screen: &mut screens.active,
            size: *size,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            title,
            pwd,
            next_implicit_hyperlink_id,
        };
        stream.next_slice(input, &mut handler)
    }

    #[cfg(test)]
    pub(super) fn pty_response_for_tests(&self) -> &[u8] {
        &self.pty_response
    }

    #[cfg(test)]
    pub(super) fn take_pty_response_for_tests(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pty_response)
    }

    #[cfg(test)]
    pub(super) fn set_palette_entry_for_tests(&mut self, index: usize, rgb: color::Rgb) {
        self.colors.palette[index] = rgb;
    }

    #[cfg(test)]
    pub(super) fn set_mode_for_tests(&mut self, mode: modes::Mode, value: bool) {
        self.modes.set(mode, value);
    }

    #[cfg(test)]
    pub(super) fn get_mode_for_tests(&self, mode: modes::Mode) -> bool {
        self.modes.get(mode)
    }

    #[cfg(test)]
    pub(super) fn save_mode_for_tests(&mut self, mode: modes::Mode) {
        self.modes.save(mode);
    }

    #[cfg(test)]
    pub(super) fn restore_mode_for_tests(&mut self, mode: modes::Mode) -> bool {
        self.modes.restore(mode)
    }

    #[cfg(test)]
    pub(super) fn set_scrolling_region_for_tests(
        &mut self,
        top: CellCountInt,
        bottom: CellCountInt,
        left: CellCountInt,
        right: CellCountInt,
    ) {
        let region = ScrollingRegion {
            top,
            bottom,
            left,
            right,
        };
        region.assert_valid(self.size);
        self.scrolling_region = region;
    }

    #[cfg(test)]
    fn scrolling_region_for_tests(&self) -> ScrollingRegion {
        self.scrolling_region
    }

    #[cfg(test)]
    pub(super) fn clear_tabstops_for_tests(&mut self) {
        self.tabstops.reset(0);
    }

    #[cfg(test)]
    pub(super) fn set_tabstop_for_tests(&mut self, col: usize) {
        assert!(col < self.tabstops.cols());
        self.tabstops.set(col);
    }

    #[cfg(test)]
    pub(super) fn clear_tabstop_for_tests(&mut self, col: usize) {
        assert!(col < self.tabstops.cols());
        if self.tabstops.get(col) {
            self.tabstops.unset(col);
        }
    }

    #[cfg(test)]
    pub(super) fn get_tabstop_for_tests(&self, col: usize) -> bool {
        assert!(col < self.tabstops.cols());
        self.tabstops.get(col)
    }

    #[cfg(test)]
    pub(super) fn set_modify_other_keys_2_for_tests(&mut self, modify_other_keys_2: bool) {
        self.flags.modify_other_keys_2 = modify_other_keys_2;
    }

    #[cfg(test)]
    pub(super) fn modify_other_keys_2_for_tests(&self) -> bool {
        self.flags.modify_other_keys_2
    }

    #[cfg(test)]
    pub(super) fn title_for_tests(&self) -> &str {
        self.title.as_str()
    }

    #[cfg(test)]
    pub(super) fn set_pwd_for_tests(&mut self, pwd: &str) {
        self.pwd.set(pwd);
    }

    #[cfg(test)]
    pub(super) fn clear_pwd_for_tests(&mut self) {
        self.pwd.clear();
    }

    #[cfg(test)]
    pub(super) fn pwd_for_tests(&self) -> Option<&str> {
        self.pwd.logical_str()
    }

    #[cfg(test)]
    pub(super) fn cursor_position_for_tests(&self) -> (CellCountInt, CellCountInt) {
        self.screens.active.cursor_position_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_pending_wrap_for_tests(&self) -> bool {
        self.screens.active.cursor_pending_wrap_for_tests()
    }

    #[cfg(test)]
    pub(super) fn is_dirty_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active.is_dirty_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn clear_dirty_for_tests(&mut self) {
        self.screens.active.clear_dirty_for_tests();
    }

    #[cfg(test)]
    pub(super) fn set_cell_protected_for_tests(
        &mut self,
        x: CellCountInt,
        y: u32,
        protected: bool,
    ) {
        self.screens
            .active
            .set_cell_protected_for_tests(x, y, protected);
    }

    #[cfg(test)]
    pub(super) fn cell_protected_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active.cell_protected_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn scrollback_rows_for_tests(&self) -> usize {
        self.screens.active.scrollback_rows_for_tests()
    }

    #[cfg(test)]
    pub(super) fn row_wrap_for_tests(&self, y: u32) -> bool {
        self.screens.active.row_wrap_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn row_wrap_continuation_for_tests(&self, y: u32) -> bool {
        self.screens.active.row_wrap_continuation_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens.active.set_row_wrap_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_continuation_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens
            .active
            .set_row_wrap_continuation_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn full_screen_plain_for_tests(&self, unwrap: bool) -> String {
        self.screens.active.full_screen_plain_for_tests(unwrap)
    }

    #[cfg(test)]
    pub(super) fn cursor_style_for_tests(&self) -> style::Style {
        self.screens.active.cursor_style_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_hyperlink_for_tests(&self) -> Option<(ScreenCursorHyperlinkId, &str)> {
        self.screens.active.cursor_hyperlink_for_tests()
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_for_tests(&self, x: CellCountInt, y: u32) -> style::Style {
        self.screens.active.active_cell_style_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_ref_count_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> style::Id {
        self.screens
            .active
            .active_cell_style_ref_count_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active.active_cell_hyperlink_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_row_styled_for_tests(&self, y: u32) -> bool {
        self.screens.active.active_row_styled_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn verify_integrity_for_tests(&self) {
        self.screens.active.verify_integrity_for_tests();
    }
}

impl Handler for TerminalStreamHandler<'_> {
    type Error = TerminalStreamError;

    fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
        match action {
            Action::Print { cp } => self.print(cp),
            Action::LineFeed => self.line_feed(),
            Action::CarriageReturn => {
                self.screen.carriage_return_basic();
                Ok(())
            }
            Action::Backspace => {
                self.screen.backspace_basic();
                Ok(())
            }
            Action::HorizontalTab { count } => {
                self.screen
                    .horizontal_tab_count_basic(self.size.cols, self.tabstops, count);
                Ok(())
            }
            Action::HorizontalTabBack { count } => {
                let left_limit = if self.modes.get(modes::Mode::Origin) {
                    self.scrolling_region.left
                } else {
                    0
                };
                self.screen
                    .horizontal_tab_back_count_basic(self.tabstops, count, left_limit);
                Ok(())
            }
            Action::TabSet => {
                self.screen.tab_set_basic(self.tabstops);
                Ok(())
            }
            Action::TabClearCurrent => {
                self.screen.tab_clear_current_basic(self.tabstops);
                Ok(())
            }
            Action::TabClearAll => {
                self.tabstops.reset(0);
                Ok(())
            }
            Action::TabReset => {
                self.tabstops.reset(TABSTOP_INTERVAL);
                Ok(())
            }
            Action::Index => self.index(),
            Action::NextLine => {
                self.line_feed()?;
                self.screen.carriage_return_basic();
                Ok(())
            }
            Action::CursorUp { count } => {
                self.screen.cursor_up_basic(count);
                Ok(())
            }
            Action::CursorDown { count } => {
                self.screen.cursor_down_basic(self.size.rows, count);
                Ok(())
            }
            Action::CursorRight { count } => {
                self.screen.cursor_right_basic(self.size.cols, count);
                Ok(())
            }
            Action::CursorLeft { count } => {
                self.screen.cursor_left_basic(count);
                Ok(())
            }
            Action::CursorColumn { col } => {
                self.screen.cursor_column_basic(self.size.cols, col);
                Ok(())
            }
            Action::CursorRow { row } => {
                self.screen.cursor_row_basic(self.size.rows, row);
                Ok(())
            }
            Action::CursorRowRelative { rows } => {
                self.screen.cursor_row_relative_basic(self.size.rows, rows);
                Ok(())
            }
            Action::CursorPosition { row, col } => {
                self.screen
                    .cursor_position_basic(row, col, self.size.rows, self.size.cols);
                Ok(())
            }
            Action::EraseDisplay { mode, protected } => self
                .screen
                .erase_display_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::EraseLine { mode, protected } => self
                .screen
                .erase_line_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::InsertChars { count } => self
                .screen
                .insert_chars_basic(
                    count,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::DeleteChars { count } => self
                .screen
                .delete_chars_basic(
                    count,
                    self.size.rows,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::EraseChars { count } => self
                .screen
                .erase_chars_basic(count, self.size.rows, self.size.cols)
                .map_err(TerminalStreamError::from),
            Action::InsertLines { count } => self
                .screen
                .insert_lines_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::DeleteLines { count } => self
                .screen
                .delete_lines_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::ScrollUp { count } => self
                .screen
                .scroll_up_basic(
                    count,
                    self.size.rows,
                    self.size.cols,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::ScrollDown { count } => self
                .screen
                .scroll_down_basic(
                    count,
                    self.scrolling_region.top,
                    self.scrolling_region.bottom,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                    self.scrolling_region.left == 0
                        && self.scrolling_region.right == self.size.cols - 1,
                )
                .map_err(TerminalStreamError::from),
            Action::SetMode { mode } => {
                self.set_mode_basic(mode, true);
                Ok(())
            }
            Action::ResetMode { mode } => {
                self.set_mode_basic(mode, false);
                Ok(())
            }
            Action::SaveMode { mode } => {
                self.modes.save(mode);
                Ok(())
            }
            Action::RestoreMode { mode } => {
                let enabled = self.modes.restore(mode);
                self.set_mode_basic(mode, enabled);
                Ok(())
            }
            Action::RequestMode { mode } => {
                let report = self.modes.get_report(modes::ModeTag::from_mode(mode));
                self.write_pty_response(&report.encode_vt());
                Ok(())
            }
            Action::RequestModeUnknown { value, ansi } => {
                let report = self.modes.get_report(modes::ModeTag::new(value, ansi));
                self.write_pty_response(&report.encode_vt());
                Ok(())
            }
            Action::SetAttribute { attr } => {
                self.screen.set_attribute_basic(attr);
                Ok(())
            }
        }
    }

    fn osc(&mut self, action: stream::OscAction<'_>) -> Result<(), Self::Error> {
        match action {
            stream::OscAction::WindowTitle { title } => {
                self.title.set(title);
            }
            stream::OscAction::ReportPwd { url } => {
                self.pwd.set(url);
            }
            stream::OscAction::StartHyperlink { id, uri } => {
                let id = match id {
                    Some(id) => ScreenCursorHyperlinkId::Explicit(id.to_string()),
                    None => {
                        let id = *self.next_implicit_hyperlink_id;
                        *self.next_implicit_hyperlink_id = id.wrapping_add(1);
                        ScreenCursorHyperlinkId::Implicit(id)
                    }
                };
                self.screen.set_cursor_hyperlink(id, uri);
            }
            stream::OscAction::EndHyperlink => {
                self.screen.clear_cursor_hyperlink();
            }
        }
        Ok(())
    }
}

impl TerminalStreamHandler<'_> {
    fn print(&mut self, cp: char) -> Result<(), TerminalStreamError> {
        if !(cp.is_ascii() && !cp.is_ascii_control()) && cp != char::REPLACEMENT_CHARACTER {
            return Err(TerminalStreamError::UnsupportedCodepoint(cp));
        }

        self.screen
            .print_basic_cell(
                self.size.cols,
                self.size.rows,
                cp,
                self.modes.get(modes::Mode::Insert),
                self.modes.get(modes::Mode::Wraparound),
                self.scrolling_region.left,
                self.scrolling_region.right,
            )
            .map_err(TerminalStreamError::from)
    }

    fn line_feed(&mut self) -> Result<(), TerminalStreamError> {
        self.screen
            .line_feed_basic(self.size.rows)
            .map_err(TerminalStreamError::from)?;
        if self.modes.get(modes::Mode::Linefeed) {
            self.screen.carriage_return_basic();
        }
        Ok(())
    }

    fn index(&mut self) -> Result<(), TerminalStreamError> {
        self.screen
            .line_feed_basic(self.size.rows)
            .map_err(TerminalStreamError::from)
    }

    fn set_mode_basic(&mut self, mode: modes::Mode, enabled: bool) {
        self.modes.set(mode, enabled);

        match mode {
            modes::Mode::Origin => self.move_cursor_to_origin_home(),
            modes::Mode::EnableLeftAndRightMargin if !enabled => {
                self.scrolling_region.left = 0;
                self.scrolling_region.right = self.size.cols.saturating_sub(1);
            }
            _ => {}
        }
    }

    fn write_pty_response(&mut self, bytes: &str) {
        self.pty_response.extend_from_slice(bytes.as_bytes());
    }

    fn move_cursor_to_origin_home(&mut self) {
        let (x, y) = if self.modes.get(modes::Mode::Origin) {
            (self.scrolling_region.left, self.scrolling_region.top)
        } else {
            (0, 0)
        };
        self.screen.cursor_position_basic(
            y.saturating_add(1),
            x.saturating_add(1),
            self.size.rows,
            self.size.cols,
        );
    }
}

impl From<BasicPrintError> for TerminalStreamError {
    fn from(err: BasicPrintError) -> Self {
        match err {
            BasicPrintError::PageAlloc => Self::PageAlloc,
            BasicPrintError::Cell(err) => match err {
                super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
                super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
            },
        }
    }
}

impl From<EraseDisplayError> for TerminalStreamError {
    fn from(err: EraseDisplayError) -> Self {
        match err {
            EraseDisplayError::PageAlloc => Self::PageAlloc,
            EraseDisplayError::Cell(err) => match err {
                super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
                super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
            },
        }
    }
}

impl ScrollingRegion {
    fn full(size: TerminalSize) -> Self {
        Self {
            top: 0,
            bottom: size.rows - 1,
            left: 0,
            right: size.cols - 1,
        }
    }

    fn assert_valid(self, size: TerminalSize) {
        assert!(self.top <= self.bottom);
        assert!(self.left <= self.right);
        assert!(self.bottom < size.rows);
        assert!(self.right < size.cols);
        if size.rows > 1 {
            assert!(self.top < self.bottom);
        }
        if size.cols > 1 {
            assert!(self.left < self.right);
        }
    }
}

impl TerminalTitle {
    fn set(&mut self, title: &str) {
        self.text.clear();
        self.text.push_str(title);
    }

    #[cfg(test)]
    fn as_str(&self) -> &str {
        &self.text
    }
}

impl TerminalPwd {
    fn set(&mut self, pwd: &str) {
        self.text.clear();
        if !pwd.is_empty() {
            self.text.push_str(pwd);
            self.text.push('\0');
        }
    }

    fn clear(&mut self) {
        self.text.clear();
    }

    fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    fn stored_str(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    fn logical_str(&self) -> Option<&str> {
        if self.text.is_empty() {
            return None;
        }

        Some(&self.text[..self.text.len() - 1])
    }
}

impl<'a> TerminalFormatterOptions<'a> {
    pub(super) const fn new(emit: PageOutputFormat) -> Self {
        Self {
            screen: ScreenFormatterOptions::new(emit),
        }
    }

    pub(super) const fn trim(mut self, trim: bool) -> Self {
        self.screen = self.screen.trim(trim);
        self
    }

    pub(super) const fn unwrap(mut self, unwrap: bool) -> Self {
        self.screen = self.screen.unwrap(unwrap);
        self
    }

    pub(super) const fn palette(mut self, palette: Option<&'a color::Palette>) -> Self {
        self.screen = self.screen.palette(palette);
        self
    }

    pub(super) const fn codepoint_map(
        mut self,
        codepoint_map: Option<&'a [CodepointMapEntry]>,
    ) -> Self {
        self.screen = self.screen.codepoint_map(codepoint_map);
        self
    }
}

impl<'a> TerminalFormatter<'a> {
    pub(super) fn init(terminal: &'a Terminal, options: TerminalFormatterOptions<'a>) -> Self {
        Self {
            terminal,
            options,
            content: ScreenFormatterContent::Selection(None),
            extra: TerminalFormatterExtra::none(),
        }
    }

    pub(super) const fn with_content(mut self, content: ScreenFormatterContent) -> Self {
        self.content = content;
        self
    }

    pub(super) const fn with_extra(mut self, extra: TerminalFormatterExtra) -> Self {
        self.extra = extra;
        self
    }

    pub(super) fn format(self) -> String {
        let mut output = self.terminal_prefix_string();
        output.push_str(
            &ScreenFormatter::init(&self.terminal.screens.active, self.options.screen)
                .with_content(self.content)
                .with_extra(self.extra.screen)
                .format(),
        );
        output.push_str(&self.terminal_suffix_string());
        output
    }

    pub(super) fn format_with_pin_map(self) -> PageStringWithPinMap {
        let prefix = self.terminal_prefix_string();
        let suffix = self.terminal_suffix_string();
        let mut output = ScreenFormatter::init(&self.terminal.screens.active, self.options.screen)
            .with_content(self.content)
            .with_extra(self.extra.screen)
            .format_with_pin_map();

        if !prefix.is_empty() {
            let top_left = self.terminal.screens.active.top_left_pin();
            let mut text = prefix;
            let mut pin_map = vec![top_left; text.len()];
            text.push_str(&output.text);
            pin_map.append(&mut output.pin_map);
            output = PageStringWithPinMap { text, pin_map };
        }

        if !suffix.is_empty() {
            let suffix_pin = output
                .pin_map
                .last()
                .copied()
                .unwrap_or_else(|| self.terminal.screens.active.top_left_pin());
            output
                .pin_map
                .extend(std::iter::repeat_n(suffix_pin, suffix.len()));
            output.text.push_str(&suffix);
        }

        output
    }

    fn terminal_prefix_string(&self) -> String {
        let mut output = self.palette_string();
        output.push_str(&self.modes_string());
        output
    }

    fn terminal_suffix_string(&self) -> String {
        let mut output = self.scrolling_region_string();
        output.push_str(&self.tabstops_string());
        output.push_str(&self.keyboard_string());
        output.push_str(&self.pwd_string());
        output
    }

    fn palette_string(&self) -> String {
        if !self.extra.palette {
            return String::new();
        }

        let palette = &self.terminal.colors.palette;
        match self.options.screen.emit() {
            PageOutputFormat::Plain => String::new(),
            PageOutputFormat::Vt => palette_vt_string(palette),
            PageOutputFormat::Html => palette_html_string(palette),
        }
    }

    fn modes_string(&self) -> String {
        if !self.extra.modes || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        modes_vt_string(&self.terminal.modes)
    }

    fn scrolling_region_string(&self) -> String {
        if !self.extra.scrolling_region || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        scrolling_region_vt_string(self.terminal.size, self.terminal.scrolling_region)
    }

    fn tabstops_string(&self) -> String {
        if !self.extra.tabstops || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        tabstops_vt_string(&self.terminal.tabstops)
    }

    fn keyboard_string(&self) -> String {
        if !self.extra.keyboard || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        keyboard_vt_string(self.terminal.flags)
    }

    fn pwd_string(&self) -> String {
        if !self.extra.pwd || self.options.screen.emit() != PageOutputFormat::Vt {
            return String::new();
        }

        pwd_vt_string(&self.terminal.pwd)
    }
}

impl TerminalFormatterExtra {
    pub(super) const fn none() -> Self {
        Self {
            palette: false,
            modes: false,
            scrolling_region: false,
            tabstops: false,
            keyboard: false,
            pwd: false,
            screen: ScreenFormatterExtra::none(),
        }
    }

    pub(super) const fn palette(mut self, palette: bool) -> Self {
        self.palette = palette;
        self
    }

    pub(super) const fn modes(mut self, modes: bool) -> Self {
        self.modes = modes;
        self
    }

    pub(super) const fn scrolling_region(mut self, scrolling_region: bool) -> Self {
        self.scrolling_region = scrolling_region;
        self
    }

    pub(super) const fn tabstops(mut self, tabstops: bool) -> Self {
        self.tabstops = tabstops;
        self
    }

    pub(super) const fn keyboard(mut self, keyboard: bool) -> Self {
        self.keyboard = keyboard;
        self
    }

    pub(super) const fn pwd(mut self, pwd: bool) -> Self {
        self.pwd = pwd;
        self
    }

    pub(super) const fn screen(mut self, screen: ScreenFormatterExtra) -> Self {
        self.screen = screen;
        self
    }
}

fn palette_vt_string(palette: &color::Palette) -> String {
    let mut output = String::new();
    for (index, rgb) in palette.iter().enumerate() {
        output.push_str(&format!(
            "\x1b]4;{};rgb:{:02x}/{:02x}/{:02x}\x1b\\",
            index, rgb.r, rgb.g, rgb.b
        ));
    }
    output
}

fn palette_html_string(palette: &color::Palette) -> String {
    let mut output = String::from("<style>:root{");
    for (index, rgb) in palette.iter().enumerate() {
        output.push_str(&format!(
            "--vt-palette-{}: #{:02x}{:02x}{:02x};",
            index, rgb.r, rgb.g, rgb.b
        ));
    }
    output.push_str("}</style>");
    output
}

fn modes_vt_string(state: &modes::ModeState) -> String {
    let mut output = String::new();
    for entry in modes::entries() {
        let current = state.get(entry.mode);
        if current == state.default_for(entry.mode) {
            continue;
        }

        output.push_str(&format!(
            "\x1b[{}{}{}",
            if entry.ansi { "" } else { "?" },
            entry.value,
            if current { "h" } else { "l" }
        ));
    }
    output
}

fn scrolling_region_vt_string(size: TerminalSize, region: ScrollingRegion) -> String {
    let mut output = String::new();
    if region.top != 0 || region.bottom != size.rows - 1 {
        output.push_str(&format!("\x1b[{};{}r", region.top + 1, region.bottom + 1));
    }
    if region.left != 0 || region.right != size.cols - 1 {
        output.push_str(&format!("\x1b[{};{}s", region.left + 1, region.right + 1));
    }
    output
}

fn tabstops_vt_string(tabstops: &tabstops::Tabstops) -> String {
    let mut output = String::from("\x1b[3g");
    for col in 0..tabstops.cols() {
        if tabstops.get(col) {
            output.push_str(&format!("\x1b[{}G\x1bH", col + 1));
        }
    }
    output
}

fn keyboard_vt_string(flags: TerminalFlags) -> String {
    if flags.modify_other_keys_2 {
        "\x1b[>4;2m".to_string()
    } else {
        String::new()
    }
}

fn pwd_vt_string(pwd: &TerminalPwd) -> String {
    if pwd.is_empty() {
        return String::new();
    }

    let mut output = String::from("\x1b]7;");
    output.push_str(pwd.stored_str());
    output.push_str("\x1b\\");
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::charsets;
    use crate::terminal::color;
    use crate::terminal::kitty::{KeyFlags, KeySetMode};
    use crate::terminal::modes::Mode;
    use crate::terminal::page_list::{CodepointReplacement, Pin};
    use crate::terminal::screen::ScreenCursorHyperlinkId;
    use crate::terminal::selection;
    use crate::terminal::style;

    fn terminal_with_lines(lines: &[&str]) -> Terminal {
        let rows = lines.len().max(1);
        let cols = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);
        let mut terminal = Terminal::init(cols.try_into().unwrap(), rows.try_into().unwrap(), None)
            .expect("test terminal must initialize");
        terminal.screens.active.set_text_lines_for_tests(lines);
        terminal
    }

    fn active_pin(terminal: &Terminal, x: CellCountInt, y: u32) -> Pin {
        terminal.screens.active.pin_for_tests(x, y)
    }

    fn active_selection(
        terminal: &Terminal,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) -> selection::Selection {
        selection::Selection::new(
            active_pin(terminal, start.0, start.1),
            active_pin(terminal, end.0, end.1),
            false,
        )
    }

    fn formatter<'a>(terminal: &'a Terminal, emit: PageOutputFormat) -> TerminalFormatter<'a> {
        TerminalFormatter::init(terminal, TerminalFormatterOptions::new(emit).unwrap(true))
    }

    fn plain_with_unwrap(terminal: &Terminal, unwrap: bool) -> String {
        TerminalFormatter::init(
            terminal,
            TerminalFormatterOptions::new(PageOutputFormat::Plain).unwrap(unwrap),
        )
        .format()
    }

    fn screen_formatter<'a>(terminal: &'a Terminal, emit: PageOutputFormat) -> ScreenFormatter<'a> {
        ScreenFormatter::init(
            &terminal.screens.active,
            ScreenFormatterOptions::new(emit).unwrap(true),
        )
    }

    fn pins(terminal: &Terminal, points: &[(CellCountInt, u32)]) -> Vec<Pin> {
        points
            .iter()
            .map(|&(x, y)| active_pin(terminal, x, y))
            .collect()
    }

    const KITTY_FLAGS_3: KeyFlags = KeyFlags {
        disambiguate: true,
        report_events: true,
        ..KeyFlags::DISABLED
    };

    fn set_active_screen_extras(terminal: &mut Terminal) {
        terminal.screens.active.set_cursor_position_for_tests(4, 2);
        terminal.screens.active.set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active
            .set_cursor_style_for_tests(style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            });
        terminal.screens.active.set_cursor_hyperlink_for_tests(
            ScreenCursorHyperlinkId::Explicit("idé".to_string()),
            "https://e.test/é",
        );
        terminal
            .screens
            .active
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
    }

    const fn all_screen_extras() -> ScreenFormatterExtra {
        ScreenFormatterExtra::none()
            .style(true)
            .hyperlink(true)
            .protection(true)
            .kitty_keyboard(true)
            .charsets(true)
            .cursor(true)
    }

    const fn terminal_screen_extras() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().screen(all_screen_extras())
    }

    const fn terminal_palette_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().palette(true)
    }

    const fn terminal_modes_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().modes(true)
    }

    const fn terminal_palette_modes_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().palette(true).modes(true)
    }

    const fn terminal_scrolling_region_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().scrolling_region(true)
    }

    const fn terminal_tabstops_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().tabstops(true)
    }

    const fn terminal_keyboard_pwd_extra() -> TerminalFormatterExtra {
        TerminalFormatterExtra::none().keyboard(true).pwd(true)
    }

    fn set_test_palette_entries(terminal: &mut Terminal) {
        terminal.set_palette_entry_for_tests(0, color::Rgb::new(0x12, 0x34, 0x56));
        terminal.set_palette_entry_for_tests(1, color::Rgb::new(0xab, 0xcd, 0xef));
        terminal.set_palette_entry_for_tests(255, color::Rgb::new(0xff, 0x00, 0xff));
    }

    fn palette_vt_prefix_len(terminal: &Terminal) -> usize {
        palette_vt_string(&terminal.colors.palette).len()
    }

    fn palette_html_prefix_len(terminal: &Terminal) -> usize {
        palette_html_string(&terminal.colors.palette).len()
    }

    fn modes_prefix_len(terminal: &Terminal) -> usize {
        modes_vt_string(&terminal.modes).len()
    }

    fn scrolling_region_suffix_len(terminal: &Terminal) -> usize {
        scrolling_region_vt_string(terminal.size, terminal.scrolling_region).len()
    }

    fn tabstops_suffix_len(terminal: &Terminal) -> usize {
        tabstops_vt_string(&terminal.tabstops).len()
    }

    fn keyboard_pwd_suffix_len(terminal: &Terminal) -> usize {
        keyboard_vt_string(terminal.flags).len() + pwd_vt_string(&terminal.pwd).len()
    }

    #[test]
    fn terminal_stream_ascii_prints_to_active_screen_and_advances_cursor() {
        let mut terminal = Terminal::init(40, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_print_marks_written_row_dirty() {
        let mut terminal = Terminal::init(40, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(39, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_invalid_utf8_writes_replacement_character() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(&[0xff]).unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            char::REPLACEMENT_CHARACTER.to_string()
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_controls_and_unsupported_escapes_do_not_write_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x0eB\x1bcC\x1b[?ZD").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "ABCD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_crlf_formats_basic_lines() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello\r\nworld").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_lf_preserves_column() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\nB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_vt_preserves_column_like_lf() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x0bB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_ff_preserves_column_like_lf() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x0cB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_d_moves_down_and_preserves_column() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1bDB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_e_moves_down_and_carriage_returns() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1bEB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_vt_honors_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0bB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_ff_honors_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0cB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_escape_d_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x1bDB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_escape_e_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x1bEB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_mode_set_and_reset_toggle_basic_mode_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[4h\x1b[20h\x1b[?7l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));

        terminal.next_slice(b"\x1b[4l\x1b[20l\x1b[?7h").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_csi_mode_set_updates_formatter_modes_extra() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?2004h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::BracketedPaste));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            "\x1b[?2004h"
        );

        terminal.next_slice(b"\x1b[?2004l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            ""
        );
    }

    #[test]
    fn terminal_stream_csi_mode_multi_params_skip_unknown_and_toggle_known_modes() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[9999;4;20h\x1b[?9999;7l\x1b[9998;4l")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_csi_origin_mode_moves_to_origin_home_and_clears_pending_wrap() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"0123456789").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?6h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());

        terminal.screens.active.set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?6l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_left_right_margin_reset_clears_horizontal_margins() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_mode_for_tests(Mode::EnableLeftAndRightMargin, true);
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?69l").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert!(!terminal.get_mode_for_tests(Mode::EnableLeftAndRightMargin));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 0);
        assert_eq!(region.right, 9);
    }

    #[test]
    fn terminal_stream_csi_deferred_modes_toggle_state_without_faked_side_effects() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(4, 1);
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b[?1049h\x1b[?1048h\x1b[?3h\x1b[?1000h")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::AltScreenSaveCursorClearEnter));
        assert!(terminal.get_mode_for_tests(Mode::SaveCursor));
        assert!(terminal.get_mode_for_tests(Mode::Column132));
        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert_eq!(terminal.size.cols, 10);
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));

        terminal.screens.active.set_cursor_position_for_tests(7, 2);
        terminal
            .next_slice(b"\x1b[?1049l\x1b[?1048l\x1b[?3l\x1b[?1000l")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::AltScreenSaveCursorClearEnter));
        assert!(!terminal.get_mode_for_tests(Mode::SaveCursor));
        assert!(!terminal.get_mode_for_tests(Mode::Column132));
        assert!(!terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 2));
        assert_eq!(terminal.size.cols, 10);
    }

    #[test]
    fn terminal_stream_unsupported_csi_mode_forms_do_not_mutate_mode_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[>4h\x1b[4:20h\x1b[?6:7h")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.get_mode_for_tests(Mode::Linefeed));
        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
    }

    #[test]
    fn terminal_stream_csi_mode_commands_do_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[4h\x1b[20h\x1b[?7l").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_reenables_wraparound_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7s\x1b[?7l\x1b[?7rabcX")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc\nX");
        assert_eq!(plain_with_unwrap(&terminal, true), "abcX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_redisables_wraparound_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7l\x1b[?7s\x1b[?7h\x1b[?7rabcX")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abX");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_origin_moves_to_restored_home() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?6s\x1b[?6h").unwrap();
        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal.screens.active.set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?6r").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_restore_left_right_margin_false_clears_horizontal_margins() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);

        terminal.next_slice(b"\x1b[?69s\x1b[?69h\x1b[?69r").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert!(!terminal.get_mode_for_tests(Mode::EnableLeftAndRightMargin));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 0);
        assert_eq!(region.right, 9);
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_bracketed_paste_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?2004s\x1b[?2004h\x1b[?2004r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));

        terminal
            .next_slice(b"\x1b[?2004h\x1b[?2004s\x1b[?2004l\x1b[?2004r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::BracketedPaste));
    }

    #[test]
    fn terminal_stream_csi_mode_save_restore_multi_params_skip_unknown_and_apply_in_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1;7;2004s").unwrap();
        terminal.next_slice(b"\x1b[?1h\x1b[?7l\x1b[?2004h").unwrap();
        terminal.next_slice(b"\x1b[?9999;1;7;2004r").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::CursorKeys));
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert!(!terminal.get_mode_for_tests(Mode::BracketedPaste));
    }

    #[test]
    fn terminal_stream_csi_mode_save_has_no_side_effect_until_restore() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal.screens.active.set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[?6s\x1b[?69s").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 2);
        assert_eq!(region.right, 8);
    }

    #[test]
    fn terminal_stream_csi_mode_restore_never_saved_uses_saved_false_default() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7rabcX").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Wraparound));
        assert_eq!(plain_with_unwrap(&terminal, false), "abX");
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_default_and_reset_wraparound() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7$p").unwrap();
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;1$y");

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?7;1$y".to_vec()
        );
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.next_slice(b"\x1b[?7l\x1b[?7$p").unwrap();
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;2$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_bracketed_paste_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?2004$p").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2004;2$y");

        terminal.next_slice(b"\x1b[?2004h\x1b[?2004$p").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2004;1$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_reports_unknown_dec_mode() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?9999$p").unwrap();

        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?9999;0$y");
    }

    #[test]
    fn terminal_stream_csi_mode_request_appends_multiple_responses_in_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?7$p\x1b[?2004h\x1b[?2004$p\x1b[?9999$p")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b[?7;1$y\x1b[?2004;1$y\x1b[?9999;0$y"
        );
    }

    #[test]
    fn terminal_stream_csi_mode_request_does_not_mutate_terminal_display_state() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal.screens.active.set_cursor_position_for_tests(5, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[?7$p").unwrap();

        let region = terminal.scrolling_region_for_tests();
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(terminal_modes_extra())
                .format(),
            "abc"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert_eq!(region.top, 1);
        assert_eq!(region.bottom, 3);
        assert_eq!(region.left, 2);
        assert_eq!(region.right, 8);
        assert!(terminal.get_mode_for_tests(Mode::Wraparound));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b[?7;1$y");
    }

    #[test]
    fn terminal_stream_sgr_mutates_cursor_style_without_visible_output() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"ab").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[1;3;31m").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ab");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(
            terminal.cursor_style_for_tests(),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    italic: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
    }

    #[test]
    fn terminal_stream_osc_updates_title_pwd_and_hyperlink_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(
                b"\x1b]0;window title\x07\x1b]7;file://host/home\x1b\\\x1b]8;id=tab;https://e\x07",
            )
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "window title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("tab".to_string()),
                "https://e"
            ))
        );

        terminal.next_slice(b"\x1b]8;;\x1b\\").unwrap();
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
    }

    #[test]
    fn terminal_stream_osc_icon_unsupported_and_malformed_leave_state_unchanged() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]0;original\x07\x1b]7;file://host/original\x07")
            .unwrap();
        terminal
            .next_slice(
                b"\x1b]1;icon\x07\x1b]9;notify\x07\x1b]8;bad=value;https://bad\x1b\\\x1b]0;\xff\x07",
            )
            .unwrap();

        assert_eq!(terminal.title_for_tests(), "original");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/original"));
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_osc_actions_do_not_mutate_display_or_responses() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.screens.active.set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]0;t\x07\x1b]7;file://host/p\x07\x1b]8;;https://e\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_osc_hyperlink_formatter_observes_active_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"hi\x1b]8;;https://implicit\x1b\\")
            .unwrap();

        let actual = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none().screen(ScreenFormatterExtra::none().hyperlink(true)),
            )
            .format();

        assert_eq!(actual, "hi\x1b]8;;https://implicit\x1b\\");
    }

    #[test]
    fn terminal_stream_osc_does_not_write_page_hyperlink_metadata() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\AB\x1b]8;;\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(!terminal.active_cell_hyperlink_for_tests(1, 0));
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc_split_feed() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b]").unwrap();
        terminal.next_slice(b"0;split").unwrap();
        terminal.next_slice(b"\x1b").unwrap();
        terminal.next_slice(b"\\").unwrap();
        terminal.next_slice(b"\x1b]7;file://host/s").unwrap();
        terminal.next_slice(b"plit\x07").unwrap();

        assert_eq!(terminal.title_for_tests(), "split");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/split"));
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_sgr_prints_styled_cells_and_resets() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[1;31mA\x1b[0mB").unwrap();

        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        };
        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.active_cell_style_for_tests(0, 0), styled);
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        assert_eq!(
            terminal.active_cell_style_for_tests(1, 0),
            style::Style::default()
        );
        terminal.verify_integrity_for_tests();

        let vt = formatter(&terminal, PageOutputFormat::Vt).format();
        assert_eq!(vt, "\x1b[0m\x1b[1m\x1b[38;5;1mA\x1b[0mB");
        let html = formatter(&terminal, PageOutputFormat::Html).format();
        assert!(html.contains("font-weight: bold;"));
        assert!(html.contains("color: var(--vt-palette-1);"));
    }

    #[test]
    fn terminal_stream_sgr_overwrites_styled_cells_with_correct_refs() {
        let mut terminal = Terminal::init(2, 1, None).unwrap();

        terminal.next_slice(b"\x1b[31mA").unwrap();
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[32mB").unwrap();

        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style {
                fg_color: style::Color::Palette(2),
                ..style::Style::default()
            }
        );
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 1);
        terminal.verify_integrity_for_tests();

        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[0mC").unwrap();

        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style::default()
        );
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 0), 0);
        assert!(!terminal.active_row_styled_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_sgr_styled_printing_keeps_insert_and_wrap_behavior() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal.next_slice(b"AC").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[4h\x1b[31mB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC");
        assert_eq!(
            terminal.active_cell_style_for_tests(1, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );

        terminal.screens.active.set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"X").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"Y").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert_eq!(
            terminal.active_cell_style_for_tests(2, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 1),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_sgr_pending_wrap_overwrites_styled_target_cell() {
        let mut terminal = Terminal::init(3, 2, None).unwrap();

        terminal.screens.active.set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"\x1b[32mG").unwrap();
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 1),
            style::Style {
                fg_color: style::Color::Palette(2),
                ..style::Style::default()
            }
        );

        terminal.screens.active.set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1b[31mX").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"Y").unwrap();

        let red = style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        };
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert_eq!(terminal.active_cell_style_for_tests(2, 0), red);
        assert_eq!(terminal.active_cell_style_for_tests(0, 1), red);
        assert_eq!(terminal.active_cell_style_ref_count_for_tests(0, 1), 2);
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_csi_insert_mode_inserts_before_printing() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXello");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(terminal.get_mode_for_tests(Mode::Insert));

        terminal.next_slice(b"\x1b[4lY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXYllo");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.get_mode_for_tests(Mode::Insert));
    }

    #[test]
    fn terminal_stream_csi_insert_mode_discards_pushed_edge_content() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXell");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_mode_at_right_edge_uses_print_wrap_path() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nX");
        assert_eq!(plain_with_unwrap(&terminal, true), "helloX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_mode_honors_horizontal_margins() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"0123456789").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 5);

        terminal.next_slice(b"\x1b[1;3H\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "01X2346789");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));

        terminal.next_slice(b"\x1b[1;7HY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "01X234Y789");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_mode_outside_margin_clears_pending_wrap_without_shift() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 0, 6);

        terminal.next_slice(b"\x1b[1;7HA").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 7, 8);
        terminal.next_slice(b"\x1b[?7l\x1b[4hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "      X");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_mode_applies_to_print() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x1b[1;2H\x1b[4").unwrap();
        terminal.next_slice(b"hX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hXello");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_linefeed_mode_changes_lf_runtime_behavior() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();

        terminal.next_slice(b"A\x1b[20h\nB\x1b[20l\nC").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\nB\n C");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
    }

    #[test]
    fn terminal_stream_csi_linefeed_mode_scrolls_then_carriage_returns() {
        let mut terminal = Terminal::init(4, 2, None).unwrap();

        terminal.next_slice(b"abc\x1b[20h\nX\nY").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X\nY");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "abc\nX\nY");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_wraparound_mode_controls_pending_wrap_runtime_behavior() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?7lhelloX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellX");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));

        terminal.next_slice(b"YZ").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));

        terminal.next_slice(b"\x1b[?7hA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellZ\nA");
        assert_eq!(plain_with_unwrap(&terminal, true), "hellZA");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_wraparound_with_horizontal_margins_wraps_to_left_margin() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 2, 5);

        terminal.next_slice(b"\x1b[1;6HA").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"B").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "     A\n  B");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_disabled_wraparound_dirties_current_row_only() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?7lhello").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hellX");
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_disabled_wraparound_bottom_right_does_not_scroll() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"\x1b[2;1H\x1b[?7lworld").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\nworlX");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "\nworlX");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(1));
    }

    #[test]
    fn terminal_stream_lf_clears_pending_wrap_without_soft_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_vt_and_ff_clear_pending_wrap_without_soft_wrap() {
        for input in [b"\x0b".as_slice(), b"\x0c".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello");
            assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
            assert!(!terminal.cursor_pending_wrap_for_tests());
            assert!(!terminal.row_wrap_for_tests(0));
            assert!(!terminal.row_wrap_continuation_for_tests(1));
        }
    }

    #[test]
    fn terminal_stream_escape_d_and_e_clear_pending_wrap_without_soft_wrap() {
        for (input, expected_cursor) in
            [(b"\x1bD".as_slice(), (4, 1)), (b"\x1bE".as_slice(), (0, 1))]
        {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello");
            assert_eq!(terminal.cursor_position_for_tests(), expected_cursor);
            assert!(!terminal.cursor_pending_wrap_for_tests());
            assert!(!terminal.row_wrap_for_tests(0));
            assert!(!terminal.row_wrap_continuation_for_tests(1));
        }
    }

    #[test]
    fn terminal_stream_lf_marks_old_and_new_rows_dirty() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"A").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\n").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_escape_d_and_e_mark_same_rows_dirty_as_lf() {
        for input in [b"\x1bD".as_slice(), b"\x1bE".as_slice()] {
            let mut lf = Terminal::init(5, 3, None).unwrap();
            lf.next_slice(b"A").unwrap();
            lf.clear_dirty_for_tests();
            lf.next_slice(b"\n").unwrap();

            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal.next_slice(b"A").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            for point in [(0, 0), (4, 0), (0, 1), (4, 1), (0, 2)] {
                assert_eq!(
                    terminal.is_dirty_for_tests(point.0, point.1),
                    lf.is_dirty_for_tests(point.0, point.1),
                    "dirty state mismatch for {input:?} at {point:?}",
                );
            }
        }
    }

    #[test]
    fn terminal_stream_cr_clears_pending_wrap_and_does_not_dirty_rows() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\r").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_bottom_row_lf_scrolls_and_preserves_column() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_vt_and_ff_scroll_and_preserve_column() {
        for input in [b"\x0b".as_slice(), b"\x0c".as_slice()] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ab\r\ncd").unwrap();
            assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "cd");
            assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
            assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_bottom_row_escape_d_scrolls_and_preserves_column() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\x1bD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_escape_e_scrolls_and_carriage_returns() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        terminal.next_slice(b"\x1bE").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "cd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_lf_marks_visible_rows_dirty() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ab\r\ncd").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\n").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_bottom_row_escape_d_and_e_mark_visible_rows_dirty() {
        for input in [b"\x1bD".as_slice(), b"\x1bE".as_slice()] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ab\r\ncd").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert!(terminal.is_dirty_for_tests(0, 0));
            assert!(terminal.is_dirty_for_tests(4, 0));
            assert!(terminal.is_dirty_for_tests(0, 1));
            assert!(terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_crlf_formats_basic_lines() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello\r").unwrap();
        terminal.next_slice(b"\nworld").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
    }

    #[test]
    fn terminal_stream_split_feed_vt_and_ff_preserve_column() {
        for input in [b"\x0bworld".as_slice(), b"\x0cworld".as_slice()] {
            let mut terminal = Terminal::init(10, 3, None).unwrap();

            terminal.next_slice(b"hello").unwrap();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "hello\n     world");
            assert_eq!(terminal.cursor_position_for_tests(), (9, 1));
            assert!(terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_split_feed_escape_d_and_e_move_down() {
        for (second, expected_plain, expected_cursor) in [
            (b"Dworld".as_slice(), "hello\n     world", (9, 1)),
            (b"Eworld".as_slice(), "hello\nworld", (5, 1)),
        ] {
            let mut terminal = Terminal::init(10, 3, None).unwrap();

            terminal.next_slice(b"hello\x1b").unwrap();
            terminal.next_slice(second).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), expected_plain);
            assert_eq!(terminal.cursor_position_for_tests(), expected_cursor);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_moves_default_counts() {
        for (input, start, expected) in [
            (b"\x1b[A".as_slice(), (2, 1), (2, 0)),
            (b"\x1b[B".as_slice(), (2, 1), (2, 2)),
            (b"\x1b[C".as_slice(), (2, 1), (3, 1)),
            (b"\x1b[D".as_slice(), (2, 1), (1, 1)),
            (b"\x1b[k".as_slice(), (2, 1), (2, 0)),
            (b"\x1b[a".as_slice(), (2, 1), (3, 1)),
            (b"\x1b[j".as_slice(), (2, 1), (1, 1)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_uses_explicit_and_zero_counts() {
        for (input, start, expected) in [
            (b"\x1b[2A".as_slice(), (2, 3), (2, 1)),
            (b"\x1b[0A".as_slice(), (2, 3), (2, 2)),
            (b"\x1b[2B".as_slice(), (2, 1), (2, 3)),
            (b"\x1b[0B".as_slice(), (2, 1), (2, 2)),
            (b"\x1b[2C".as_slice(), (1, 1), (3, 1)),
            (b"\x1b[0C".as_slice(), (1, 1), (2, 1)),
            (b"\x1b[2D".as_slice(), (3, 1), (1, 1)),
            (b"\x1b[0D".as_slice(), (3, 1), (2, 1)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clamps_oversized_counts_to_edges() {
        for (input, expected) in [
            (b"\x1b[999999999999999999999999A".as_slice(), (2, 0)),
            (b"\x1b[999999999999999999999999B".as_slice(), (2, 4)),
            (b"\x1b[999999999999999999999999C".as_slice(), (4, 2)),
            (b"\x1b[999999999999999999999999D".as_slice(), (0, 2)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(2, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clears_pending_wrap() {
        for input in [
            b"\x1b[A".as_slice(),
            b"\x1b[B".as_slice(),
            b"\x1b[C".as_slice(),
            b"\x1b[D".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_does_not_modify_cells_or_dirty_rows() {
        for input in [
            b"\x1b[A".as_slice(),
            b"\x1b[B".as_slice(),
            b"\x1b[C".as_slice(),
            b"\x1b[D".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_movement_clamps_without_scrolling_or_reverse_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.next_slice(b"ab\r\ncd").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[100D").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");

        terminal.next_slice(b"\x1b[100B").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
    }

    #[test]
    fn terminal_stream_split_feed_csi_cursor_movement_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2C").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_move_and_carriage_return() {
        for (input, start, expected) in [
            (b"\x1b[E".as_slice(), (3, 1), (0, 2)),
            (b"\x1b[F".as_slice(), (3, 1), (0, 0)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_use_explicit_and_zero_counts() {
        for (input, start, expected) in [
            (b"\x1b[2E".as_slice(), (3, 1), (0, 3)),
            (b"\x1b[0E".as_slice(), (3, 1), (0, 2)),
            (b"\x1b[2F".as_slice(), (3, 3), (0, 1)),
            (b"\x1b[0F".as_slice(), (3, 3), (0, 2)),
        ] {
            let mut terminal = Terminal::init(5, 5, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_clamp_without_scrolling() {
        for (input, start, expected) in [
            (b"\x1b[100E".as_slice(), (3, 1), (0, 1)),
            (b"\x1b[100F".as_slice(), (3, 0), (0, 0)),
        ] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();
            terminal.next_slice(b"ab\r\ncd").unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
            assert_eq!(plain_with_unwrap(&terminal, false), "ab\ncd");
            assert_eq!(terminal.full_screen_plain_for_tests(false), "ab\ncd");
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_clear_pending_wrap() {
        for input in [b"\x1b[E".as_slice(), b"\x1b[F".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests().0, 0);
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_next_and_previous_line_do_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[E".as_slice(), b"\x1b[F".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 1);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_next_line_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2E").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_split_feed_csi_previous_line_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(3, 2);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2F").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_moves_to_default_column() {
        for input in [b"\x1b[G".as_slice(), b"\x1b[`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_uses_one_indexed_columns() {
        for (input, expected) in [
            (b"\x1b[1G".as_slice(), (0, 1)),
            (b"\x1b[2G".as_slice(), (1, 1)),
            (b"\x1b[5G".as_slice(), (4, 1)),
            (b"\x1b[1`".as_slice(), (0, 1)),
            (b"\x1b[3`".as_slice(), (2, 1)),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_zero_and_oversized_columns_clamp() {
        for (input, expected_x) in [
            (b"\x1b[0G".as_slice(), 0),
            (b"\x1b[0`".as_slice(), 0),
            (b"\x1b[999999999999999999999999G".as_slice(), 4),
            (b"\x1b[999999999999999999999999`".as_slice(), 4),
        ] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(2, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (expected_x, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_clears_pending_wrap() {
        for input in [b"\x1b[G".as_slice(), b"\x1b[`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_does_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[2G".as_slice(), b"\x1b[3`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal.screens.active.set_cursor_position_for_tests(4, 1);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests().1, 1);
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_absolute_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(4, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"3G").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal.next_slice(b"\x1b[0").unwrap();
        terminal.next_slice(b"`").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_moves_to_default_rows() {
        for (input, start, expected) in [
            (b"\x1b[d".as_slice(), (3, 2), (3, 0)),
            (b"\x1b[e".as_slice(), (3, 1), (3, 2)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_uses_one_indexed_absolute_rows() {
        for (input, expected) in [
            (b"\x1b[1d".as_slice(), (3, 0)),
            (b"\x1b[2d".as_slice(), (3, 1)),
            (b"\x1b[4d".as_slice(), (3, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_uses_relative_rows() {
        for (input, expected) in [
            (b"\x1b[0e".as_slice(), (3, 1)),
            (b"\x1b[1e".as_slice(), (3, 2)),
            (b"\x1b[2e".as_slice(), (3, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_zero_and_oversized_values_clamp() {
        for (input, start, expected_y) in [
            (b"\x1b[0d".as_slice(), (2, 2), 0),
            (b"\x1b[0e".as_slice(), (2, 2), 2),
            (b"\x1b[999999999999999999999999d".as_slice(), (2, 1), 3),
            (b"\x1b[999999999999999999999999e".as_slice(), (2, 1), 3),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start.0, start.1);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (2, expected_y));
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_clears_pending_wrap() {
        for input in [b"\x1b[d".as_slice(), b"\x1b[0e".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert!(!terminal.cursor_pending_wrap_for_tests());
        }
    }

    #[test]
    fn terminal_stream_csi_vertical_positioning_does_not_modify_cells_or_dirty_rows() {
        for input in [b"\x1b[2d".as_slice(), b"\x1b[1e".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();

            terminal.next_slice(b"abc").unwrap();
            terminal.screens.active.set_cursor_position_for_tests(4, 0);
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests().0, 4);
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(4, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
            assert!(!terminal.is_dirty_for_tests(4, 1));
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_vertical_positioning_moves_cursor() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"3d").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 2));

        terminal.next_slice(b"\x1b[1").unwrap();
        terminal.next_slice(b"e").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 3));
    }

    #[test]
    fn terminal_stream_csi_cursor_position_moves_to_default_home() {
        for input in [b"\x1b[H".as_slice(), b"\x1b[f".as_slice()] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_uses_one_indexed_coordinates() {
        for (input, expected) in [
            (b"\x1b[2H".as_slice(), (0, 1)),
            (b"\x1b[2;3H".as_slice(), (2, 1)),
            (b"\x1b[4;5f".as_slice(), (4, 3)),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_empty_and_zero_params_clamp_to_top_left() {
        for input in [
            b"\x1b[0;0H".as_slice(),
            b"\x1b[;H".as_slice(),
            b"\x1b[;;H".as_slice(),
            b"\x1b[3;;H".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 4, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 2);

            terminal.next_slice(input).unwrap();

            let expected = if input == b"\x1b[3;;H" {
                (0, 2)
            } else {
                (0, 0)
            };
            assert_eq!(terminal.cursor_position_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_csi_cursor_position_oversized_values_clamp_to_edges() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal
            .next_slice(b"\x1b[999999999999999999999999;999999999999999999999999H")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 3));
    }

    #[test]
    fn terminal_stream_csi_cursor_position_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[H").unwrap();

        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_cursor_position_does_not_modify_cells_or_dirty_rows() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2;4H").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_split_feed_csi_cursor_position_moves_cursor() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal.next_slice(b"\x1b[2;").unwrap();
        terminal.next_slice(b"4H").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"f").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_backspace_overwrites_previous_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello\x08y").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "helly");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_moves_to_next_default_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1       A");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_uses_custom_tabstops() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1  A");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_without_later_tabstop_clamps_to_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"1\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1   A");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_repeated_horizontal_tabs_advance_and_clamp() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_starting_on_tabstop_moves_to_next_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"12345678\tA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "12345678        A");
        assert_eq!(terminal.cursor_position_for_tests(), (17, 0));
    }

    #[test]
    fn terminal_stream_horizontal_tab_at_right_edge_stays_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCD\t").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_horizontal_tab_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nX");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_horizontal_tab_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\t").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_moves_to_next_default_tabstop() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"1\x1b[IA").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "1       A");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_zero_count_does_not_move() {
        for input in [b"\x1b[0I".as_slice(), b"\x1b[;I".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(3, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_count_moves_multiple_tabstops() {
        for input in [b"\x1b[2I".as_slice(), b"\x1b[2;I".as_slice()] {
            let mut terminal = Terminal::init(24, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_stops_at_right_edge() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[999999999999999999999999I")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (19, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_uses_custom_tabstops() {
        let mut terminal = Terminal::init(12, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);
        terminal.set_tabstop_for_tests(7);

        terminal.next_slice(b"\x1b[2I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (7, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_starting_on_tabstop_moves_next() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(8, 0);

        terminal.next_slice(b"\x1b[I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tabulation_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2I").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_moves_to_previous_default_tabstops() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(19, 0);

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));

        terminal.next_slice(b"\x1b[Z").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_starting_on_tabstop_moves_previous() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(8, 0);

        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_zero_count_does_not_move() {
        for input in [b"\x1b[0Z".as_slice(), b"\x1b[;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(16, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_count_moves_multiple_tabstops() {
        for input in [b"\x1b[2Z".as_slice(), b"\x1b[2;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal.screens.active.set_cursor_position_for_tests(19, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_large_count_clamps_to_left_edge() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(19, 0);

        terminal
            .next_slice(b"\x1b[999999999999999999999999Z")
            .unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_uses_custom_tabstops() {
        let mut terminal = Terminal::init(12, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(3);
        terminal.set_tabstop_for_tests(7);
        terminal.screens.active.set_cursor_position_for_tests(10, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_non_origin_ignores_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 10, 19);
        terminal.screens.active.set_cursor_position_for_tests(16, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_origin_clamps_to_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal.set_scrolling_region_for_tests(0, 2, 5, 19);
        terminal.screens.active.set_cursor_position_for_tests(16, 0);

        terminal.next_slice(b"\x1b[9Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_origin_at_or_before_left_margin_does_not_move() {
        for start in [4, 5] {
            let mut terminal = Terminal::init(20, 3, None).unwrap();
            terminal.set_mode_for_tests(Mode::Origin, true);
            terminal.set_scrolling_region_for_tests(0, 2, 5, 19);
            terminal
                .screens
                .active
                .set_cursor_position_for_tests(start, 0);

            terminal.next_slice(b"\x1b[Z").unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (start, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_preserves_pending_wrap_without_moving() {
        let mut terminal = Terminal::init(1, 2, None).unwrap();

        terminal.next_slice(b"A").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_preserves_pending_wrap_after_moving() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"ABCDEFGHIJKLMNOPQRST").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(19, 0);
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(19, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_tab_back_moves_cursor() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal.screens.active.set_cursor_position_for_tests(19, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));

        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_horizontal_tab_back_does_not_mutate_state() {
        for input in [
            b"\x1b[?ZA".as_slice(),
            b"\x1b[>ZA".as_slice(),
            b"\x1b[1;2ZA".as_slice(),
            b"\x1b[1:2ZA".as_slice(),
            b"\x1b[ ZA".as_slice(),
        ] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal.screens.active.set_cursor_position_for_tests(16, 0);
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc             A");
            assert_eq!(terminal.cursor_position_for_tests(), (17, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_csi_erase_display_below_clears_cursor_to_end() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nFG");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_above_clears_start_to_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\n   IJ\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_complete_clears_screen() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[2J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_csi_erase_display_scrollback_preserves_active_screen() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        assert!(terminal.scrollback_rows_for_tests() > 0);

        terminal.next_slice(b"\x1b[3J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), active_before);
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
    }

    #[test]
    fn terminal_stream_csi_erase_display_scroll_complete_clears_active_and_resets_cursor() {
        let mut terminal = Terminal::init(5, 3, Some(10)).unwrap();

        terminal.next_slice(b"A\nB").unwrap();
        terminal.next_slice(b"\x1b[22J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_display_dirty_rows_match_affected_range() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[J").unwrap();

        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_display_full_rows_clear_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(2, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[J").unwrap();

        assert!(!terminal.row_wrap_for_tests(2));
        assert!(!terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_display_protected_cells_survive() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal.screens.active.set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[?J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "  C");
    }

    #[test]
    fn terminal_stream_unsupported_csi_erase_display_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[4JCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_erase_display_clears_screen() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB").unwrap();
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2J").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_csi_erase_line_right_clears_cursor_row_suffix() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nFG\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_left_clears_cursor_row_prefix() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n   IJ\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_clears_cursor_row_only() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_does_not_mutate_scrollback() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC").unwrap();
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let active_before = plain_with_unwrap(&terminal, false);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert_ne!(plain_with_unwrap(&terminal, false), "");
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_erase_line_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_line_right_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal.screens.active.set_cursor_position_for_tests(2, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[K").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_left_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n\nKLMNO");
        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_protected_cells_survive() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[?K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A C\nFGHIJ");
    }

    #[test]
    fn terminal_stream_unsupported_csi_erase_line_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[4KCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_erase_line_clears_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB").unwrap();
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_csi_insert_chars_count_one_shifts_suffix_right() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_zero_count_behaves_as_one() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[99@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_insert_chars_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_outside_horizontal_margin_clears_pending_wrap_only() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_honors_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABC123").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC 13");
    }

    #[test]
    fn terminal_stream_csi_insert_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert!(terminal.cell_protected_for_tests(3, 0));
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_count_one_clears_without_shifting() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A CDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_erase_chars_zero_count_behaves_as_one() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A CDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clamps_to_screen_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[99X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_ignores_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABCDEF").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A  DEF");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[X").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clears_stored_protected_cells() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal.screens.active.set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB DE");
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_insert_and_erase_chars_do_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[?@CD\x1b[?X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_and_erase_chars_mutates_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");

        terminal.screens.active.set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A   D");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_count_one_shifts_suffix_left() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_count_two_shifts_suffix_left() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[99P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_clears_pending_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_delete_chars_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_honors_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABC123").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC2 3");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_ignores_vertical_margins() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE\nFGHIJ\nKLMNO");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_preserves_scrollback_and_other_rows() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"A\nB\nC\nD").unwrap();
        let scrollback_before = terminal.scrollback_rows_for_tests();
        terminal.screens.active.set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);

        let mut terminal = terminal_with_lines(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nGHIJ\nKLMNO");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal.screens.active.set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ACDE");
        assert!(terminal.cell_protected_for_tests(1, 0));
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_unsupported_csi_delete_chars_does_not_mutate_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"AB\x1b[?PCD").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCD");
    }

    #[test]
    fn terminal_stream_split_feed_csi_delete_chars_shifts_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
    }

    #[test]
    fn terminal_stream_csi_insert_lines_count_one_shifts_rows_down() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal.screens.active.set_cursor_position_for_tests(1, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC\n\nDEF\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
        assert!(terminal.is_dirty_for_tests(0, 3));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_clamps_to_remaining_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.screens.active.set_cursor_position_for_tests(0, 2);

        terminal.next_slice(b"\x1b[99L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_preserves_scrollback_content() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"AAAAA\nBBBBB\nCCCCC\nDDDDD").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        let full_before = terminal.full_screen_plain_for_tests(false);
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let history_before = full_before
            .strip_suffix(&active_before)
            .expect("full screen output should end with active output")
            .to_owned();
        terminal.screens.active.set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert!(terminal
            .full_screen_plain_for_tests(false)
            .starts_with(&history_before));
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_insert_lines_outside_vertical_margin_is_noop() {
        let mut terminal = terminal_with_lines(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nD   56\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_wrap_metadata_depends_on_width() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_unsupported_csi_insert_lines_does_not_mutate_state() {
        let mut terminal = terminal_with_lines(&["abc"]);

        terminal.next_slice(b"\x1b[?L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_split_feed_csi_insert_lines_shifts_rows() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_delete_lines_count_one_shifts_rows_up() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal.screens.active.set_cursor_position_for_tests(1, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(0, 2));
        assert!(terminal.is_dirty_for_tests(0, 3));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_clamps_to_remaining_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.screens.active.set_cursor_position_for_tests(0, 2);

        terminal.next_slice(b"\x1b[99M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_preserves_scrollback_content() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();

        terminal.next_slice(b"AAAAA\nBBBBB\nCCCCC\nDDDDD").unwrap();
        let active_before = plain_with_unwrap(&terminal, false);
        let full_before = terminal.full_screen_plain_for_tests(false);
        let scrollback_before = terminal.scrollback_rows_for_tests();
        let history_before = full_before
            .strip_suffix(&active_before)
            .expect("full screen output should end with active output")
            .to_owned();
        terminal.screens.active.set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);
        assert!(terminal
            .full_screen_plain_for_tests(false)
            .starts_with(&history_before));
        assert_ne!(plain_with_unwrap(&terminal, false), active_before);
    }

    #[test]
    fn terminal_stream_csi_delete_lines_outside_vertical_margin_is_noop() {
        let mut terminal = terminal_with_lines(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nBBBBB\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_outside_horizontal_margin_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_scrolling_region_for_tests(0, 1, 0, 3);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC\n\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nDHI756\nG   89"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_left_right_high_count_clamps() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[99M").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "ABC123\nD   56\nG   89"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_wrap_metadata_depends_on_width() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(1, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_unsupported_csi_delete_lines_does_not_mutate_state() {
        let mut terminal = terminal_with_lines(&["abc"]);

        terminal.next_slice(b"\x1b[?M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_split_feed_csi_delete_lines_shifts_rows() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.screens.active.set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_scroll_up_creates_scrollback() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD", "EEEEE"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "BBBBB\nCCCCC\nDDDDD\nEEEEE"
        );
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC\nDDDDD\nEEEEE"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 1);
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        for y in 0..5 {
            assert!(terminal.is_dirty_for_tests(0, y));
        }
    }

    #[test]
    fn terminal_stream_csi_scroll_up_max_scrollback_zero_scrolls_without_history() {
        let mut terminal = Terminal::init(5, 5, Some(0)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "BBBBB\nCCCCC");
        assert_eq!(terminal.full_screen_plain_for_tests(false), "BBBBB\nCCCCC");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
    }

    #[test]
    fn terminal_stream_csi_scroll_up_clamps_to_region_height() {
        let mut terminal = Terminal::init(5, 3, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[99S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 3);
    }

    #[test]
    fn terminal_stream_csi_scroll_up_preserves_rows_below_bottom_margin() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD", "EEEEE"]);
        terminal.set_scrolling_region_for_tests(0, 2, 0, 4);

        terminal.next_slice(b"\x1b[2S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "CCCCC\n\n\nDDDDD\nEEEEE"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 2);
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "AAAAA\nBBBBB\nCCCCC\n\n\nDDDDD\nEEEEE"
        );
    }

    #[test]
    fn terminal_stream_csi_scroll_up_with_top_margin_uses_delete_lines_path() {
        let mut terminal = Terminal::init(5, 4, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 3, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC\nDDDDD");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_up_with_left_right_margin_uses_delete_lines_path() {
        let mut terminal = Terminal::init(6, 3, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AEF423\nDHI756\nG   89"
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
    }

    #[test]
    fn terminal_stream_csi_scroll_up_preserves_pending_wrap() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.screens.active.set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_up_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, Some(10)).unwrap();
        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_count_one_shifts_rows_down() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal.screens.active.set_cursor_position_for_tests(2, 2);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "\nABC\nDEF\nGHI");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 2));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        for y in 0..4 {
            assert!(terminal.is_dirty_for_tests(0, y));
        }
    }

    #[test]
    fn terminal_stream_csi_scroll_down_honors_top_bottom_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(5, 2);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "A   23\nDBC156\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_original_cursor_outside_vertical_margin_still_scrolls() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(2, 3, 0, 4);
        terminal.screens.active.set_cursor_position_for_tests(4, 0);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAZ\nBBBBB\n\nCCCCC");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_down_original_cursor_outside_horizontal_margin_still_scrolls() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal.screens.active.set_cursor_position_for_tests(5, 0);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "A   23\nDBC156\nGEF489"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_preserves_pending_wrap() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.screens.active.set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"Z").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_csi_scroll_down_zero_count_is_noop() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[0T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_clamps_to_region_height() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[99T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_unsupported_csi_scroll_up_and_down_does_not_mutate_state() {
        for input in [b"\x1b[?S".as_slice(), b"\x1b[?T".as_slice()] {
            let mut terminal = terminal_with_lines(&["abc"]);

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        }
    }

    #[test]
    fn terminal_stream_split_feed_csi_scroll_up_and_down_mutates_rows() {
        let mut terminal = Terminal::init(5, 4, Some(10)).unwrap();
        terminal
            .screens
            .active
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"S").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "BBBBB\nCCCCC");

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"T").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "\nBBBBB\nCCCCC");
    }

    #[test]
    fn terminal_stream_split_feed_csi_horizontal_tabulation_moves_cursor() {
        let mut terminal = Terminal::init(24, 2, None).unwrap();

        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"I").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_split_feed_horizontal_tab_writes_at_next_tabstop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\tX").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello   X");
        assert_eq!(terminal.cursor_position_for_tests(), (9, 0));
    }

    #[test]
    fn terminal_stream_escape_h_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1bH").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_escape_h_tabstop_is_used_by_horizontal_tab() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1bH\r1\tZ").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(plain_with_unwrap(&terminal, false), "1bcZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_escape_h_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1bH").unwrap();

        assert!(terminal.get_tabstop_for_tests(4));
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
    }

    #[test]
    fn terminal_stream_escape_h_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bH").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_escape_h_sets_tabstop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b").unwrap();
        terminal.next_slice(b"H").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_w_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[W").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_csi_zero_w_sets_tabstop_at_current_column() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[0W").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert!(!terminal.get_tabstop_for_tests(2));
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
    }

    #[test]
    fn terminal_stream_csi_w_tabstop_is_used_by_horizontal_tab() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"abc\x1b[W\r1\tZ").unwrap();

        assert!(terminal.get_tabstop_for_tests(3));
        assert_eq!(plain_with_unwrap(&terminal, false), "1bcZ");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
    }

    #[test]
    fn terminal_stream_csi_w_preserves_pending_wrap_at_right_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.clear_tabstops_for_tests();

        terminal.next_slice(b"ABCDE").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x1b[W").unwrap();

        assert!(terminal.get_tabstop_for_tests(4));
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
    }

    #[test]
    fn terminal_stream_csi_w_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[W").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_current_removes_current_tabstop() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\t").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        assert!(terminal.get_tabstop_for_tests(8));

        terminal.next_slice(b"\x1b[2W\r\t").unwrap();

        assert!(!terminal.get_tabstop_for_tests(8));
        assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_all_removes_all_tabstops() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5W\r\t").unwrap();

        assert!(!terminal.get_tabstop_for_tests(8));
        assert!(!terminal.get_tabstop_for_tests(16));
        assert!(!terminal.get_tabstop_for_tests(24));
        assert_eq!(terminal.cursor_position_for_tests(), (29, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_reset_restores_default_tabstops() {
        let mut terminal = Terminal::init(30, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5W\x1b[?5W\r\t").unwrap();

        assert!(terminal.get_tabstop_for_tests(8));
        assert!(terminal.get_tabstop_for_tests(16));
        assert!(terminal.get_tabstop_for_tests(24));
        assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_preserve_cursor_position() {
        for input in [
            b"abc\x1bH\x1b[2W".as_slice(),
            b"abc\x1b[5W".as_slice(),
            b"abc\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        }
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_preserve_pending_wrap() {
        for input in [
            b"\x1b[2W".as_slice(),
            b"\x1b[5W".as_slice(),
            b"\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(5, 2, None).unwrap();

            terminal.next_slice(b"ABCDE").unwrap();
            assert!(terminal.cursor_pending_wrap_for_tests());
            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
            assert!(terminal.cursor_pending_wrap_for_tests());
            assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE");
        }
    }

    #[test]
    fn terminal_stream_csi_tab_clear_and_reset_do_not_dirty_rows_or_modify_cells() {
        for input in [
            b"\x1b[2W".as_slice(),
            b"\x1b[5W".as_slice(),
            b"\x1b[?5W".as_slice(),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(b"abc\x1bH").unwrap();
            terminal.clear_dirty_for_tests();
            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
            assert!(!terminal.is_dirty_for_tests(0, 1));
        }
    }

    #[test]
    fn terminal_stream_backspace_at_column_zero_clamps() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x08A").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_backspace_clears_pending_wrap_without_soft_wrap() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"\x08").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCXE");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_backspace_does_not_dirty_rows_or_modify_cells() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x08").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(4, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_split_feed_backspace_overwrites_previous_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x08y").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "helly");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
    }

    #[test]
    fn terminal_stream_split_utf8_state_survives_feed_calls() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\xf0\x9f").unwrap();
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        terminal.next_slice(b"A").unwrap();

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            format!("{}A", char::REPLACEMENT_CHARACTER)
        );
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_valid_split_utf8_errors_only_after_completion() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        let bytes = "é".as_bytes();

        terminal.next_slice(&bytes[..1]).unwrap();
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        assert_eq!(
            terminal.next_slice(&bytes[1..]),
            Err(TerminalStreamError::UnsupportedCodepoint('é'))
        );

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_split_csi_state_survives_feed_calls() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1b[?").unwrap();
        terminal.next_slice(b"ZB").unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_right_edge_writes_cell_and_sets_pending_wrap() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(0));
    }

    #[test]
    fn terminal_stream_pending_wrap_prints_next_cell_on_next_row() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"w").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert_eq!(plain_with_unwrap(&terminal, true), "hellow");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_basic_wraparound_matches_upstream_case() {
        let mut terminal = Terminal::init(5, 40, None).unwrap();

        terminal.next_slice(b"helloworldabc12").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld\nabc12");
        assert_eq!(plain_with_unwrap(&terminal, true), "helloworldabc12");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 2));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_for_tests(2));
        assert!(!terminal.row_wrap_continuation_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_pending_wrap_marks_old_and_new_rows_dirty() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"w").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
        assert!(!terminal.is_dirty_for_tests(0, 2));
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_scrolls_and_writes() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworldabc12").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\nabc12");
        assert_eq!(plain_with_unwrap(&terminal, true), "worldabc12");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\nabc12"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_survives_feed_boundary() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworld").unwrap();
        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nworld");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert!(terminal.cursor_pending_wrap_for_tests());

        terminal.next_slice(b"a").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\na");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\na"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (1, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.row_wrap_for_tests(0));
        assert!(terminal.row_wrap_continuation_for_tests(1));
    }

    #[test]
    fn terminal_stream_after_scroll_writes_to_active_bottom_row() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworlda").unwrap();
        terminal.next_slice(b"bc").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "world\nabc");
        assert_eq!(
            terminal.full_screen_plain_for_tests(false),
            "hello\nworld\nabc"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
        assert!(!terminal.cursor_pending_wrap_for_tests());
    }

    #[test]
    fn terminal_stream_bottom_row_pending_wrap_marks_visible_rows_dirty() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"helloworld").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"a").unwrap();

        assert!(terminal.is_dirty_for_tests(0, 0));
        assert!(terminal.is_dirty_for_tests(4, 0));
        assert!(terminal.is_dirty_for_tests(0, 1));
        assert!(terminal.is_dirty_for_tests(4, 1));
    }

    #[test]
    fn terminal_stream_pending_wrap_managed_destination_errors_without_mutating() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.screens.active.set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"x").unwrap();
        terminal.set_cell_protected_for_tests(0, 1, true);
        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hello").unwrap();
        terminal.clear_dirty_for_tests();

        assert_eq!(
            terminal.next_slice(b"w"),
            Err(TerminalStreamError::ManagedCellUnsupported)
        );

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nx");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.row_wrap_for_tests(0));
        assert!(!terminal.row_wrap_continuation_for_tests(1));
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_non_ascii_print_returns_private_error() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(
            terminal.next_slice("é".as_bytes()),
            Err(TerminalStreamError::UnsupportedCodepoint('é'))
        );
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_managed_cell_overwrite_returns_private_error() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"x").unwrap();
        terminal.screens.active.set_cursor_position_for_tests(0, 0);
        terminal.set_cell_protected_for_tests(0, 0, true);

        assert_eq!(
            terminal.next_slice(b"A"),
            Err(TerminalStreamError::ManagedCellUnsupported)
        );
        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "x");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_formatter_plain_full_active_screen_single_line() {
        let terminal = terminal_with_lines(&["hello"]);

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello"
        );
    }

    #[test]
    fn terminal_formatter_plain_full_active_screen_multiline() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        assert_eq!(
            formatter(&terminal, PageOutputFormat::Plain).format(),
            "hello\nworld"
        );
    }

    #[test]
    fn terminal_formatter_plain_selected_line() {
        let terminal = terminal_with_lines(&["line1", "line2", "line3"]);
        let selection = active_selection(&terminal, (0, 1), (4, 1));

        let actual = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format();

        assert_eq!(actual, "line2");
    }

    #[test]
    fn terminal_formatter_no_content_emits_empty_output_and_pin_map() {
        let terminal = terminal_with_lines(&["hello"]);

        let formatter = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None);

        assert_eq!(formatter.format(), "");
        assert_eq!(
            formatter.format_with_pin_map(),
            PageStringWithPinMap {
                text: String::new(),
                pin_map: Vec::new(),
            }
        );
    }

    #[test]
    fn terminal_formatter_vt_content_delegates_to_screen_formatter() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output, "hello\r\nworld");
    }

    #[test]
    fn terminal_formatter_html_content_delegates_to_screen_formatter() {
        let terminal = terminal_with_lines(&["<hi"]);

        let terminal_output = formatter(&terminal, PageOutputFormat::Html).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Html).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;hi</div>"
        );
    }

    #[test]
    fn terminal_formatter_codepoint_map_delegates_output_and_pin_map() {
        let terminal = terminal_with_lines(&["ao"]);
        let map = [CodepointMapEntry::new(
            'o' as u32,
            'o' as u32,
            CodepointReplacement::String("<é".to_string()),
        )
        .unwrap()];
        let options =
            TerminalFormatterOptions::new(PageOutputFormat::Html).codepoint_map(Some(&map));

        let terminal_output = TerminalFormatter::init(&terminal, options).format_with_pin_map();
        let screen_output = ScreenFormatter::init(
            &terminal.screens.active,
            ScreenFormatterOptions::new(PageOutputFormat::Html).codepoint_map(Some(&map)),
        )
        .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output.text,
            "<div style=\"font-family: monospace; white-space: pre;\">a&lt;&#233;</div>"
        );
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
    }

    #[test]
    fn terminal_formatter_trim_and_palette_delegate_to_screen_formatter() {
        let mut terminal = terminal_with_lines(&["X  "]);
        let styled = style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        };
        terminal
            .screens
            .active
            .set_styled_cell_for_tests(0, 0, 'X', styled);
        let mut palette = color::DEFAULT_PALETTE;
        palette[1] = color::Rgb::new(1, 2, 3);
        let options = TerminalFormatterOptions::new(PageOutputFormat::Html)
            .trim(false)
            .palette(Some(&palette));

        let terminal_output = TerminalFormatter::init(&terminal, options).format();
        let screen_output = ScreenFormatter::init(
            &terminal.screens.active,
            ScreenFormatterOptions::new(PageOutputFormat::Html)
                .trim(false)
                .palette(Some(&palette)),
        )
        .format();

        assert_eq!(terminal_output, screen_output);
        assert!(terminal_output.contains("rgb(1, 2, 3)"));
        assert!(terminal_output.contains("</div>  </div>"));
    }

    #[test]
    fn terminal_formatter_plain_pin_map_single_line() {
        let terminal = terminal_with_lines(&["hello"]);

        let actual = formatter(&terminal, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello");
        assert_eq!(
            actual.pin_map,
            pins(&terminal, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_plain_pin_map_multiline() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let actual = formatter(&terminal, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello\nworld");
        assert_eq!(
            actual.pin_map,
            pins(
                &terminal,
                &[
                    (0, 0),
                    (1, 0),
                    (2, 0),
                    (3, 0),
                    (4, 0),
                    (4, 0),
                    (0, 1),
                    (1, 1),
                    (2, 1),
                    (3, 1),
                    (4, 1)
                ]
            )
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_selected_plain_pin_map() {
        let terminal = terminal_with_lines(&["line1", "line2", "line3"]);
        let selection = active_selection(&terminal, (0, 1), (4, 1));

        let actual = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format_with_pin_map();

        assert_eq!(actual.text, "line2");
        assert_eq!(
            actual.pin_map,
            pins(&terminal, &[(0, 1), (1, 1), (2, 1), (3, 1), (4, 1)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn terminal_formatter_vt_and_html_pin_maps_delegate_to_screen_formatter() {
        let terminal = terminal_with_lines(&["<é"]);

        for emit in [PageOutputFormat::Vt, PageOutputFormat::Html] {
            let terminal_output = formatter(&terminal, emit).format_with_pin_map();
            let screen_output = screen_formatter(&terminal, emit).format_with_pin_map();

            assert_eq!(terminal_output, screen_output);
            assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        }
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal.screens.active.set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active
            .set_cursor_style_for_tests(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            });
        terminal.screens.active.set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active
            .set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output, "hi");
        assert!(!terminal_output.contains("\x1b]4;"));
        assert!(!terminal_output.contains("--vt-palette-"));
    }

    #[test]
    fn terminal_formatter_default_pin_map_does_not_emit_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal.screens.active.set_cursor_position_for_tests(4, 2);
        terminal.screens.active.set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active
            .set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text, "hi");
        assert_eq!(terminal_output.pin_map, pins(&terminal, &[(0, 0), (1, 0)]));
    }

    #[test]
    fn terminal_formatter_vt_palette_extra_emits_before_content() {
        let mut terminal = terminal_with_lines(&["content"]);
        set_test_palette_entries(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_palette_extra())
            .format();

        assert!(output.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert_eq!(output.matches("\x1b]4;").count(), 256);
        assert!(output.contains("\x1b]4;1;rgb:ab/cd/ef\x1b\\"));
        assert!(output.contains("\x1b]4;255;rgb:ff/00/ff\x1b\\"));
        assert!(output.ends_with("content"));
        assert!(output.find("\x1b]4;255;").unwrap() < output.find("content").unwrap());
    }

    #[test]
    fn terminal_formatter_html_palette_extra_emits_before_content() {
        let mut terminal = terminal_with_lines(&["<content"]);
        set_test_palette_entries(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_extra(terminal_palette_extra())
            .format();

        assert!(output.starts_with("<style>:root{"));
        assert_eq!(output.matches("--vt-palette-").count(), 256);
        assert!(output.contains("--vt-palette-0: #123456;"));
        assert!(output.contains("--vt-palette-1: #abcdef;"));
        assert!(output.contains("--vt-palette-255: #ff00ff;"));
        assert!(output.contains("}</style><div"));
        assert!(output.ends_with("&lt;content</div>"));
    }

    #[test]
    fn terminal_formatter_plain_ignores_palette_extra() {
        let mut terminal = terminal_with_lines(&["plain"]);
        set_test_palette_entries(&mut terminal);

        let default_output = formatter(&terminal, PageOutputFormat::Plain).format();
        let palette_output = formatter(&terminal, PageOutputFormat::Plain)
            .with_extra(terminal_palette_extra())
            .format();
        let palette_pin_map = formatter(&terminal, PageOutputFormat::Plain)
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();

        assert_eq!(palette_output, default_output);
        assert_eq!(palette_output, "plain");
        assert_eq!(palette_pin_map.text, "plain");
        assert_eq!(
            palette_pin_map.pin_map,
            pins(&terminal, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
        );
    }

    #[test]
    fn terminal_formatter_palette_extra_without_content_emits_for_vt_and_html() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        set_test_palette_entries(&mut terminal);

        let vt = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();
        let html = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();
        let plain = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_palette_extra())
            .format();

        assert_eq!(vt.matches("\x1b]4;").count(), 256);
        assert!(vt.ends_with("\x1b]4;255;rgb:ff/00/ff\x1b\\"));
        assert_eq!(html.matches("--vt-palette-").count(), 256);
        assert!(html.ends_with("--vt-palette-255: #ff00ff;}</style>"));
        assert_eq!(plain, "");
    }

    #[test]
    fn terminal_formatter_vt_palette_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        set_test_palette_entries(&mut terminal);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();
        let prefix_len = palette_vt_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert!(output.text.ends_with("éB"));
        assert!(prefix_len < output.text.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[prefix_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_html_palette_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "<B"]);
        set_test_palette_entries(&mut terminal);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_extra())
            .format_with_pin_map();
        let prefix_len = palette_html_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("<style>:root{"));
        assert!(output.text.ends_with("&lt;B</div>"));
        assert!(prefix_len < output.text.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        let content_start = output.text.find("&lt;B").unwrap();
        assert_eq!(output.pin_map[content_start], active_pin(&terminal, 0, 1));
        assert_eq!(
            output.pin_map.last().copied(),
            Some(active_pin(&terminal, 1, 1))
        );
    }

    #[test]
    fn terminal_formatter_palette_pin_map_without_content_uses_top_left() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        set_test_palette_entries(&mut terminal);

        for emit in [PageOutputFormat::Vt, PageOutputFormat::Html] {
            let output = formatter(&terminal, emit)
                .with_content(ScreenFormatterContent::None)
                .with_extra(terminal_palette_extra())
                .format_with_pin_map();

            assert!(!output.text.is_empty());
            assert_eq!(output.text.len(), output.pin_map.len());
            for pin in output.pin_map {
                assert_eq!(pin, active_pin(&terminal, 0, 0));
            }
        }
    }

    #[test]
    fn terminal_formatter_vt_palette_combines_before_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .screen(all_screen_extras()),
            )
            .format();
        let prefix_len = palette_vt_prefix_len(&terminal);

        assert_eq!(output.matches("\x1b]4;").count(), 256);
        assert_eq!(&output[prefix_len..prefix_len + 2], "hi");
        assert!(output[prefix_len + 2..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[3;5H"));
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_modes() {
        let mut terminal = terminal_with_lines(&["hi"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_mode_for_tests(Mode::Wraparound, false);

        let output = formatter(&terminal, PageOutputFormat::Vt).format();

        assert_eq!(output, "hi");
        assert!(!output.contains("\x1b[?2004h"));
        assert!(!output.contains("\x1b[?7l"));
    }

    #[test]
    fn terminal_formatter_vt_modes_extra_emits_only_differences_before_content() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.set_mode_for_tests(Mode::SendReceiveMode, false);
        terminal.set_mode_for_tests(Mode::CursorKeys, true);
        terminal.set_mode_for_tests(Mode::Wraparound, false);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(output, "\x1b[4h\x1b[12l\x1b[?1h\x1b[?7l\x1b[?2004hhello");
        assert!(output.find("\x1b[4h").unwrap() < output.find("\x1b[12l").unwrap());
        assert!(output.find("\x1b[12l").unwrap() < output.find("\x1b[?1h").unwrap());
        assert!(output.find("\x1b[?1h").unwrap() < output.find("\x1b[?7l").unwrap());
        assert!(output.find("\x1b[?7l").unwrap() < output.find("\x1b[?2004h").unwrap());
        assert!(output.find("\x1b[?2004h").unwrap() < output.find("hello").unwrap());
    }

    #[test]
    fn terminal_formatter_vt_modes_extra_ignores_default_true_modes_at_default() {
        let terminal = terminal_with_lines(&["hello"]);

        assert!(terminal.get_mode_for_tests(Mode::SendReceiveMode));
        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(output, "hello");
        assert!(!output.contains("\x1b[12"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_modes_extra() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let modes_output = formatter(&terminal, emit)
                .with_extra(terminal_modes_extra())
                .format();

            assert_eq!(modes_output, default_output);
            assert!(!modes_output.contains("\x1b[4h"));
            assert!(!modes_output.contains("\x1b[?2004h"));
        }
    }

    #[test]
    fn terminal_formatter_modes_extra_without_content_emits_for_vt_only() {
        let mut terminal = terminal_with_lines(&["ignored"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);

        let vt = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();
        let html = formatter(&terminal, PageOutputFormat::Html)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();
        let plain = formatter(&terminal, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_modes_extra())
            .format();

        assert_eq!(vt, "\x1b[?2004h");
        assert_eq!(html, "");
        assert_eq!(plain, "");
    }

    #[test]
    fn terminal_formatter_modes_pin_map_uses_top_left_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_modes_extra())
            .format_with_pin_map();
        let prefix_len = modes_prefix_len(&terminal);

        assert_eq!(output.text, "\x1b[?2004héB");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in &output.pin_map[..prefix_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[prefix_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_palette_and_modes_pin_map_order_before_selected_content() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_palette_modes_extra())
            .format_with_pin_map();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(output.text.len(), output.pin_map.len());
        assert!(output.text.starts_with("\x1b]4;0;rgb:12/34/56\x1b\\"));
        assert_eq!(
            &output.text[palette_len..palette_len + modes_len],
            "\x1b[?2004h"
        );
        assert!(output.text.ends_with("éB"));
        for pin in &output.pin_map[..palette_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        for pin in &output.pin_map[palette_len..palette_len + modes_len] {
            assert_eq!(*pin, active_pin(&terminal, 0, 0));
        }
        assert_eq!(
            &output.pin_map[palette_len + modes_len..],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
    }

    #[test]
    fn terminal_formatter_palette_modes_content_and_screen_extras_order() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras()),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 2],
            "hi"
        );
        assert!(output[palette_len + modes_len + 2..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[3;5H"));
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_scrolling_region_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello", "world", "again"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 4);

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert_eq!(terminal.scrolling_region_for_tests().top, 1);
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello\r\nworld\r\nagain");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_scrolling_region_full_screen_emits_nothing() {
        let terminal = terminal_with_lines(&["hello", "world"]);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "hello\r\nworld");
        assert_eq!(scrolling_region_suffix_len(&terminal), 0);
    }

    #[test]
    fn terminal_formatter_scrolling_region_vertical_only_emits_decstbm_after_content() {
        let mut terminal = terminal_with_lines(&["one", "two", "three", "four"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "one\r\ntwo\r\nthree\r\nfour\x1b[2;3r");
    }

    #[test]
    fn terminal_formatter_scrolling_region_horizontal_only_emits_decslrm_after_content() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert_eq!(output, "hello\r\nworld\x1b[2;4s");
    }

    #[test]
    fn terminal_formatter_scrolling_region_combined_emits_decstbm_then_decslrm() {
        let mut terminal = terminal_with_lines(&["hello", "world", "again"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_scrolling_region_extra())
            .format();

        assert!(output.ends_with("\x1b[2;3r\x1b[2;5s"));
        assert!(output.find("\x1b[2;3r").unwrap() < output.find("\x1b[2;5s").unwrap());
        assert!(output.find("again").unwrap() < output.find("\x1b[2;3r").unwrap());
    }

    #[test]
    fn terminal_formatter_scrolling_region_emits_after_forwarded_screen_extras() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .screen(all_screen_extras())
                    .scrolling_region(true),
            )
            .format();

        assert!(output.contains("hey\r\nyou\x1b[0m"));
        assert!(output.ends_with("\x1b[1;2s"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
    }

    #[test]
    fn terminal_formatter_palette_modes_screen_extras_and_scrolling_region_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.ends_with("\x1b[1;2s"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_scrolling_region_extra() {
        let mut terminal = terminal_with_lines(&["<hi", "row"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let region_output = formatter(&terminal, emit)
                .with_extra(terminal_scrolling_region_extra())
                .format();

            assert_eq!(region_output, default_output);
            assert!(!region_output.contains("\x1b["));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_scrolling_region_extra())
            .format_with_pin_map();

        assert_eq!(output.text, "\x1b[2;4s");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_scrolling_region_extra())
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[2;3s");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_rejects_invalid_test_regions() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);

        let invalid = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            terminal.set_scrolling_region_for_tests(1, 0, 0, 4);
        }));

        assert!(invalid.is_err());
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_tabstops_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert!(terminal.get_tabstop_for_tests(1));
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_tabstops_default_interval_emits_clear_and_ascending_hts() {
        let terminal = terminal_with_lines(&["0123456789abcdefghi"]);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(
            output,
            "0123456789abcdefghi\x1b[3g\x1b[9G\x1bH\x1b[17G\x1bH"
        );
    }

    #[test]
    fn terminal_formatter_tabstops_custom_columns_emit_one_indexed_columns() {
        let mut terminal = terminal_with_lines(&["0123456789012345678901234567890"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(29);
        terminal.set_tabstop_for_tests(4);
        terminal.set_tabstop_for_tests(14);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(output, "\x1b[3g\x1b[5G\x1bH\x1b[15G\x1bH\x1b[30G\x1bH");
    }

    #[test]
    fn terminal_formatter_tabstops_empty_state_emits_only_clear_all() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.clear_tabstop_for_tests(1);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format();

        assert_eq!(output, "\x1b[3g");
        assert!(!terminal.get_tabstop_for_tests(1));
    }

    #[test]
    fn terminal_formatter_tabstops_emit_after_content_and_screen_extras() {
        let mut terminal = terminal_with_lines(&["hi"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .screen(all_screen_extras())
                    .tabstops(true),
            )
            .format();

        assert!(output.contains("hi\x1b[0m"));
        assert!(output.ends_with("\x1b[3g\x1b[2G\x1bH"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[3g").unwrap());
    }

    #[test]
    fn terminal_formatter_tabstops_emit_after_scrolling_region() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(4);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format();

        assert_eq!(output, "hello\r\nworld\x1b[2;4s\x1b[3g\x1b[5G\x1bH");
    }

    #[test]
    fn terminal_formatter_all_suffix_extras_keep_upstream_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
        assert!(output.find("\x1b[1;2s").unwrap() < output.find("\x1b[3g").unwrap());
        assert!(output.ends_with("\x1b[3g\x1b[2G\x1bH"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_tabstops_extra() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let tabstops_output = formatter(&terminal, emit)
                .with_extra(terminal_tabstops_extra())
                .format();

            assert_eq!(tabstops_output, default_output);
            assert!(!tabstops_output.contains("\x1b[3g"));
        }
    }

    #[test]
    fn terminal_formatter_tabstops_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_tabstops_extra())
            .format_with_pin_map();

        assert_eq!(output.text, "\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_tabstops_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_tabstops_extra())
            .format_with_pin_map();
        let suffix_len = tabstops_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_scrolling_region_and_tabstops_pin_map_share_final_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true),
            )
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal) + tabstops_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(output.text, "éB\x1b[2;3s\x1b[3g\x1b[2G\x1bH");
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_default_path_does_not_emit_keyboard_or_pwd_or_change_pin_map() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let default_text = formatter(&terminal, PageOutputFormat::Vt).format();
        let default_pin_map = formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();
        let screen_text = screen_formatter(&terminal, PageOutputFormat::Vt).format();
        let screen_pin_map =
            screen_formatter(&terminal, PageOutputFormat::Vt).format_with_pin_map();

        assert!(terminal.modify_other_keys_2_for_tests());
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home/user"));
        assert_eq!(default_text, screen_text);
        assert_eq!(default_text, "hello");
        assert_eq!(default_pin_map, screen_pin_map);
    }

    #[test]
    fn terminal_formatter_keyboard_extra_emits_modify_other_keys_2_only_when_enabled() {
        let mut terminal = terminal_with_lines(&["hello"]);

        let disabled = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().keyboard(true))
            .format();
        assert_eq!(disabled, "");

        terminal.set_modify_other_keys_2_for_tests(true);
        let enabled = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().keyboard(true))
            .format();

        assert_eq!(enabled, "\x1b[>4;2m");
    }

    #[test]
    fn terminal_formatter_pwd_extra_emits_raw_stored_bytes_with_nul_and_st() {
        let mut terminal = terminal_with_lines(&["hello"]);

        let empty = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().pwd(true))
            .format();
        assert_eq!(empty, "");

        terminal.set_pwd_for_tests("file://host/home/user");
        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(TerminalFormatterExtra::none().pwd(true))
            .format();

        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home/user"));
        assert_eq!(output.as_bytes(), b"\x1b]7;file://host/home/user\0\x1b\\");
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_emit_after_tabstops() {
        let mut terminal = terminal_with_lines(&["hello", "world"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 3);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(4);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format();

        assert_eq!(
            output.as_bytes(),
            b"hello\r\nworld\x1b[2;4s\x1b[3g\x1b[5G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
    }

    #[test]
    fn terminal_formatter_all_terminal_extras_keep_upstream_order() {
        let mut terminal = terminal_with_lines(&["hey", "you"]);
        set_test_palette_entries(&mut terminal);
        terminal.set_mode_for_tests(Mode::BracketedPaste, true);
        terminal.set_scrolling_region_for_tests(0, 1, 0, 1);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        set_active_screen_extras(&mut terminal);

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(
                TerminalFormatterExtra::none()
                    .palette(true)
                    .modes(true)
                    .screen(all_screen_extras())
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format();
        let palette_len = palette_vt_prefix_len(&terminal);
        let modes_len = modes_prefix_len(&terminal);

        assert_eq!(&output[palette_len..palette_len + modes_len], "\x1b[?2004h");
        assert_eq!(
            &output[palette_len + modes_len..palette_len + modes_len + 8],
            "hey\r\nyou"
        );
        assert!(output[palette_len + modes_len + 8..].starts_with("\x1b[0m"));
        assert!(output.find("\x1b[3;5H").unwrap() < output.find("\x1b[1;2s").unwrap());
        assert!(output.find("\x1b[1;2s").unwrap() < output.find("\x1b[3g").unwrap());
        assert!(output.find("\x1b[3g").unwrap() < output.find("\x1b[>4;2m").unwrap());
        assert!(output.find("\x1b[>4;2m").unwrap() < output.find("\x1b]7;").unwrap());
        assert!(output
            .as_bytes()
            .ends_with(b"\x1b[3g\x1b[2G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"));
    }

    #[test]
    fn terminal_formatter_plain_and_html_ignore_keyboard_and_pwd_extras() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let default_output = formatter(&terminal, emit).format();
            let keyboard_pwd_output = formatter(&terminal, emit)
                .with_extra(terminal_keyboard_pwd_extra())
                .format();

            assert_eq!(keyboard_pwd_output, default_output);
            assert!(!keyboard_pwd_output.contains("\x1b[>4;2m"));
            assert!(!keyboard_pwd_output.contains("\x1b]7;"));
        }
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_without_content_maps_to_top_left() {
        let mut terminal = terminal_with_lines(&["hello"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_keyboard_pwd_extra())
            .format_with_pin_map();

        assert_eq!(
            output.text.as_bytes(),
            b"\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        for pin in output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_keyboard_and_pwd_pin_map_uses_last_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(terminal_keyboard_pwd_extra())
            .format_with_pin_map();
        let suffix_len = keyboard_pwd_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(
            output.text.as_bytes(),
            b"\xc3\xa9B\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_prior_suffixes_keyboard_and_pwd_pin_map_share_final_content_pin() {
        let mut terminal = terminal_with_lines(&["top", "éB"]);
        terminal.set_scrolling_region_for_tests(0, 1, 1, 2);
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal.set_pwd_for_tests("file://host/home/user");
        let selection = active_selection(&terminal, (0, 1), (1, 1));

        let output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(
                TerminalFormatterExtra::none()
                    .scrolling_region(true)
                    .tabstops(true)
                    .keyboard(true)
                    .pwd(true),
            )
            .format_with_pin_map();
        let suffix_len = scrolling_region_suffix_len(&terminal)
            + tabstops_suffix_len(&terminal)
            + keyboard_pwd_suffix_len(&terminal);
        let content_len = output.text.len() - suffix_len;

        assert_eq!(
            output.text.as_bytes(),
            b"\xc3\xa9B\x1b[2;3s\x1b[3g\x1b[2G\x1bH\x1b[>4;2m\x1b]7;file://host/home/user\0\x1b\\"
        );
        assert_eq!(output.text.len(), output.pin_map.len());
        assert_eq!(
            &output.pin_map[..content_len],
            pins(&terminal, &[(0, 1), (0, 1), (1, 1)])
        );
        for pin in &output.pin_map[content_len..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 1));
        }
    }

    #[test]
    fn terminal_formatter_forwards_screen_extras_to_vt_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_screen_extras())
            .format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(all_screen_extras())
            .format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "hi\x1b[0m\x1b[38;5;1m\x1b]8;id=idé;https://e.test/é\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x0e\x1b[3;5H"
        );
    }

    #[test]
    fn terminal_formatter_forwards_screen_extras_with_no_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_screen_extras())
            .format();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(all_screen_extras())
            .format();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(
            terminal_output,
            "\x1b[0m\x1b[38;5;1m\x1b]8;id=idé;https://e.test/é\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x0e\x1b[3;5H"
        );
    }

    #[test]
    fn terminal_formatter_forwards_screen_extra_pin_maps_with_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(terminal_screen_extras())
            .format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_extra(all_screen_extras())
            .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        assert!(terminal_output.text.chars().count() < terminal_output.text.len());
        assert_eq!(terminal_output.pin_map[0], active_pin(&terminal, 0, 0));
        assert_eq!(terminal_output.pin_map[1], active_pin(&terminal, 1, 0));
        for pin in &terminal_output.pin_map[2..] {
            assert_eq!(*pin, active_pin(&terminal, 1, 0));
        }
    }

    #[test]
    fn terminal_formatter_forwards_screen_extra_pin_maps_without_content() {
        let mut terminal = terminal_with_lines(&["hi"]);
        set_active_screen_extras(&mut terminal);

        let terminal_output = formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(terminal_screen_extras())
            .format_with_pin_map();
        let screen_output = screen_formatter(&terminal, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(all_screen_extras())
            .format_with_pin_map();

        assert_eq!(terminal_output, screen_output);
        assert_eq!(terminal_output.text.len(), terminal_output.pin_map.len());
        for pin in terminal_output.pin_map {
            assert_eq!(pin, active_pin(&terminal, 0, 0));
        }
    }

    #[test]
    fn terminal_formatter_forwarded_screen_extras_follow_screen_formatter_for_plain_and_html() {
        let mut terminal = terminal_with_lines(&["<hi"]);
        set_active_screen_extras(&mut terminal);

        for emit in [PageOutputFormat::Plain, PageOutputFormat::Html] {
            let terminal_output = formatter(&terminal, emit)
                .with_extra(terminal_screen_extras())
                .format();
            let screen_output = screen_formatter(&terminal, emit)
                .with_extra(all_screen_extras())
                .format();
            let default_output = formatter(&terminal, emit).format();

            assert_eq!(terminal_output, screen_output);
            assert_eq!(terminal_output, default_output);
        }
    }

    #[test]
    fn terminal_formatter_invalid_or_garbage_selection_returns_empty_output_and_map() {
        let terminal = terminal_with_lines(&["hello"]);
        let other = terminal_with_lines(&["other"]);
        let valid = active_pin(&terminal, 0, 0);
        let invalid = active_pin(&other, 0, 0);
        let mut garbage = valid;
        garbage.mark_garbage_for_tests();

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            let actual = formatter(&terminal, PageOutputFormat::Plain)
                .with_content(ScreenFormatterContent::Selection(Some(selection)))
                .format_with_pin_map();
            assert_eq!(
                actual,
                PageStringWithPinMap {
                    text: String::new(),
                    pin_map: Vec::new(),
                }
            );
        }
    }
}
