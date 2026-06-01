//! Terminal state.

use super::charsets;
use super::color;
#[cfg(test)]
use super::cursor;
use super::dcs;
use super::device_attributes;
use super::device_status;
use super::modes;
use super::mouse;
use super::osc;
use super::page::SemanticPrompt;
use super::page_list::{
    CodepointMapEntry, PageListAllocError, PageOutputFormat, PageStringWithPinMap,
};
use super::screen::{
    BasicPrintError, EraseDisplayError, Screen, ScreenCursorHyperlinkId, ScreenFormatter,
    ScreenFormatterContent, ScreenFormatterExtra, ScreenFormatterOptions,
};
use super::sgr;
use super::size::CellCountInt;
use super::stream::{self, Action, Handler};
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
    dcs: dcs::Handler,
    flags: TerminalFlags,
    title: TerminalTitle,
    pwd: TerminalPwd,
    mouse_shape: mouse::MouseShape,
    next_implicit_hyperlink_id: u32,
    previous_char: Option<char>,
}

#[derive(Debug)]
pub(super) struct TerminalScreens {
    primary: Screen,
    alternate: Option<Screen>,
    active: TerminalScreenKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TerminalScreenKey {
    Primary,
    Alternate,
}

impl TerminalScreens {
    fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        Ok(Self {
            primary: Screen::init(cols, rows, max_scrollback_rows)?,
            alternate: None,
            active: TerminalScreenKey::Primary,
        })
    }

    fn active(&self) -> &Screen {
        match self.active {
            TerminalScreenKey::Primary => &self.primary,
            TerminalScreenKey::Alternate => self
                .alternate
                .as_ref()
                .expect("alternate screen must exist when active"),
        }
    }

    fn active_mut(&mut self) -> &mut Screen {
        match self.active {
            TerminalScreenKey::Primary => &mut self.primary,
            TerminalScreenKey::Alternate => self
                .alternate
                .as_mut()
                .expect("alternate screen must exist when active"),
        }
    }

    fn active_key(&self) -> TerminalScreenKey {
        self.active
    }

    fn screen(&self, key: TerminalScreenKey) -> Option<&Screen> {
        match key {
            TerminalScreenKey::Primary => Some(&self.primary),
            TerminalScreenKey::Alternate => self.alternate.as_ref(),
        }
    }

    fn ensure_alternate(
        &mut self,
        cols: CellCountInt,
        rows: CellCountInt,
    ) -> Result<(), PageListAllocError> {
        if self.alternate.is_none() {
            self.alternate = Some(Screen::init(cols, rows, Some(0))?);
        }
        Ok(())
    }

    fn switch_to(
        &mut self,
        key: TerminalScreenKey,
        cols: CellCountInt,
        rows: CellCountInt,
    ) -> Result<Option<TerminalScreenKey>, PageListAllocError> {
        if self.active == key {
            return Ok(None);
        }

        if key == TerminalScreenKey::Alternate {
            self.ensure_alternate(cols, rows)?;
        }

        let old = self.active;
        let charset = self.active().charset_state();
        self.active_mut().clear_cursor_hyperlink();
        self.active = key;
        self.active_mut().set_charset_state(charset);
        self.active_mut().mark_active_rows_dirty();
        Ok(Some(old))
    }

    fn copy_cursor_from_to(&mut self, from: TerminalScreenKey, to: TerminalScreenKey) {
        if from == to {
            return;
        }

        match (from, to) {
            (TerminalScreenKey::Primary, TerminalScreenKey::Alternate) => {
                let alternate = self
                    .alternate
                    .as_mut()
                    .expect("alternate screen must exist before cursor copy");
                alternate.copy_cursor_from_without_hyperlink(&self.primary);
            }
            (TerminalScreenKey::Alternate, TerminalScreenKey::Primary) => {
                let alternate = self
                    .alternate
                    .as_ref()
                    .expect("alternate screen must exist before cursor copy");
                self.primary.copy_cursor_from_without_hyperlink(alternate);
            }
            _ => {}
        }
    }

    fn reset(&mut self) {
        self.primary.reset();
        self.alternate = None;
        self.active = TerminalScreenKey::Primary;
    }

    #[cfg(test)]
    fn alternate_initialized_for_tests(&self) -> bool {
        self.alternate.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TerminalSize {
    cols: CellCountInt,
    rows: CellCountInt,
}

#[derive(Debug, Clone, Copy)]
struct TerminalColors {
    palette: color::Palette,
    foreground: color::DynamicRgb,
    background: color::DynamicRgb,
    cursor: color::DynamicRgb,
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
    mouse_event: mouse::MouseEventMode,
    mouse_format: mouse::MouseFormat,
    mouse_shift_capture: Option<bool>,
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
    screens: &'a mut TerminalScreens,
    size: TerminalSize,
    colors: &'a mut TerminalColors,
    modes: &'a mut modes::ModeState,
    scrolling_region: &'a mut ScrollingRegion,
    tabstops: &'a mut tabstops::Tabstops,
    pty_response: &'a mut Vec<u8>,
    title: &'a mut TerminalTitle,
    pwd: &'a mut TerminalPwd,
    mouse_shape: &'a mut mouse::MouseShape,
    next_implicit_hyperlink_id: &'a mut u32,
    previous_char: &'a mut Option<char>,
    dcs: &'a mut dcs::Handler,
    flags: &'a mut TerminalFlags,
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
            screens: TerminalScreens::init(cols, rows, max_scrollback_rows)?,
            colors: TerminalColors {
                palette: color::DEFAULT_PALETTE,
                foreground: color::DynamicRgb::init(color::DEFAULT_PALETTE[7]),
                background: color::DynamicRgb::init(color::DEFAULT_PALETTE[0]),
                cursor: color::DynamicRgb::unset(),
            },
            modes: modes::ModeState::default(),
            scrolling_region: ScrollingRegion::full(size),
            tabstops: tabstops::Tabstops::new(cols as usize, TABSTOP_INTERVAL)
                .map_err(|_| PageListAllocError::PageAlloc)?,
            pty_response: Vec::new(),
            stream: stream::Stream::init(),
            dcs: dcs::Handler::new(),
            flags: TerminalFlags::default(),
            title: TerminalTitle::default(),
            pwd: TerminalPwd::default(),
            mouse_shape: mouse::MouseShape::Text,
            next_implicit_hyperlink_id: 0,
            previous_char: None,
        })
    }

    pub(super) fn next_slice(&mut self, input: &[u8]) -> Result<(), TerminalStreamError> {
        let Terminal {
            size,
            screens,
            colors,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            stream,
            dcs,
            title,
            pwd,
            mouse_shape,
            next_implicit_hyperlink_id,
            previous_char,
            flags,
            ..
        } = self;
        let mut handler = TerminalStreamHandler {
            screens,
            size: *size,
            colors,
            modes,
            scrolling_region,
            tabstops,
            pty_response,
            title,
            pwd,
            mouse_shape,
            next_implicit_hyperlink_id,
            previous_char,
            dcs,
            flags,
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
    pub(super) fn mouse_event_for_tests(&self) -> mouse::MouseEventMode {
        self.flags.mouse_event
    }

    #[cfg(test)]
    pub(super) fn mouse_format_for_tests(&self) -> mouse::MouseFormat {
        self.flags.mouse_format
    }

    #[cfg(test)]
    pub(super) fn mouse_shift_capture_for_tests(&self) -> Option<bool> {
        self.flags.mouse_shift_capture
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
    pub(super) fn mouse_shape_for_tests(&self) -> mouse::MouseShape {
        self.mouse_shape
    }

    #[cfg(test)]
    fn active_screen_key_for_tests(&self) -> TerminalScreenKey {
        self.screens.active_key()
    }

    #[cfg(test)]
    fn alternate_initialized_for_tests(&self) -> bool {
        self.screens.alternate_initialized_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_position_for_tests(&self) -> (CellCountInt, CellCountInt) {
        self.screens.active().cursor_position_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_pending_wrap_for_tests(&self) -> bool {
        self.screens.active().cursor_pending_wrap_for_tests()
    }

    #[cfg(test)]
    pub(super) fn is_dirty_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().is_dirty_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn clear_dirty_for_tests(&mut self) {
        self.screens.active_mut().clear_dirty_for_tests();
    }

    #[cfg(test)]
    pub(super) fn set_cell_protected_for_tests(
        &mut self,
        x: CellCountInt,
        y: u32,
        protected: bool,
    ) {
        self.screens
            .active_mut()
            .set_cell_protected_for_tests(x, y, protected);
    }

    #[cfg(test)]
    pub(super) fn cell_protected_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().cell_protected_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn scrollback_rows_for_tests(&self) -> usize {
        self.screens.active().scrollback_rows_for_tests()
    }

    #[cfg(test)]
    pub(super) fn row_wrap_for_tests(&self, y: u32) -> bool {
        self.screens.active().row_wrap_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn row_wrap_continuation_for_tests(&self, y: u32) -> bool {
        self.screens.active().row_wrap_continuation_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens.active_mut().set_row_wrap_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn set_row_wrap_continuation_for_tests(&mut self, y: u32, wrap: bool) {
        self.screens
            .active_mut()
            .set_row_wrap_continuation_for_tests(y, wrap);
    }

    #[cfg(test)]
    pub(super) fn full_screen_plain_for_tests(&self, unwrap: bool) -> String {
        self.screens.active().full_screen_plain_for_tests(unwrap)
    }

    #[cfg(test)]
    pub(super) fn cursor_style_for_tests(&self) -> style::Style {
        self.screens.active().cursor_style_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_visual_style_for_tests(&self) -> cursor::VisualStyle {
        self.screens.active().cursor_visual_style_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_protected_for_tests(&self) -> bool {
        self.screens.active().cursor_protected_for_tests()
    }

    #[cfg(test)]
    pub(super) fn cursor_hyperlink_for_tests(&self) -> Option<(ScreenCursorHyperlinkId, &str)> {
        self.screens.active().cursor_hyperlink_for_tests()
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_for_tests(&self, x: CellCountInt, y: u32) -> style::Style {
        self.screens.active().active_cell_style_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_style_ref_count_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> style::Id {
        self.screens
            .active()
            .active_cell_style_ref_count_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.screens.active().active_cell_hyperlink_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_snapshot_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> Option<super::page::HyperlinkSnapshot> {
        self.screens
            .active()
            .active_cell_hyperlink_snapshot_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_hyperlink_ref_count_for_tests(&self, x: CellCountInt, y: u32) -> u16 {
        self.screens
            .active()
            .active_cell_hyperlink_ref_count_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn active_row_hyperlink_for_tests(&self, y: u32) -> bool {
        self.screens.active().active_row_hyperlink_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_row_styled_for_tests(&self, y: u32) -> bool {
        self.screens.active().active_row_styled_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_row_semantic_prompt_for_tests(&self, y: u32) -> SemanticPrompt {
        self.screens
            .active()
            .active_row_semantic_prompt_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn active_cell_semantic_content_for_tests(
        &self,
        x: CellCountInt,
        y: u32,
    ) -> super::page::SemanticContent {
        self.screens
            .active()
            .active_cell_semantic_content_for_tests(x, y)
    }

    #[cfg(test)]
    pub(super) fn verify_integrity_for_tests(&self) {
        self.screens.active().verify_integrity_for_tests();
    }
}

impl Handler for TerminalStreamHandler<'_> {
    type Error = TerminalStreamError;

    fn vt(&mut self, action: Action) -> Result<(), Self::Error> {
        match action {
            Action::Print { cp } => self.print(cp),
            Action::PrintRepeat { count } => self.print_repeat(count),
            Action::Enquiry => Ok(()),
            Action::LineFeed => self.line_feed(),
            Action::CarriageReturn => {
                self.screens.active_mut().carriage_return_basic();
                Ok(())
            }
            Action::Backspace => {
                self.screens.active_mut().backspace_basic();
                Ok(())
            }
            Action::HorizontalTab { count } => {
                self.screens.active_mut().horizontal_tab_count_basic(
                    self.size.cols,
                    self.tabstops,
                    count,
                );
                Ok(())
            }
            Action::HorizontalTabBack { count } => {
                let left_limit = if self.modes.get(modes::Mode::Origin) {
                    self.scrolling_region.left
                } else {
                    0
                };
                self.screens.active_mut().horizontal_tab_back_count_basic(
                    self.tabstops,
                    count,
                    left_limit,
                );
                Ok(())
            }
            Action::TabSet => {
                self.screens.active_mut().tab_set_basic(self.tabstops);
                Ok(())
            }
            Action::TabClearCurrent => {
                self.screens
                    .active_mut()
                    .tab_clear_current_basic(self.tabstops);
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
                self.screens.active_mut().carriage_return_basic();
                Ok(())
            }
            Action::CursorUp { count } => {
                self.screens.active_mut().cursor_up_basic(count);
                Ok(())
            }
            Action::CursorDown { count } => {
                self.screens
                    .active_mut()
                    .cursor_down_basic(self.size.rows, count);
                Ok(())
            }
            Action::CursorRight { count } => {
                self.screens
                    .active_mut()
                    .cursor_right_basic(self.size.cols, count);
                Ok(())
            }
            Action::CursorLeft { count } => {
                self.screens.active_mut().cursor_left_basic(count);
                Ok(())
            }
            Action::CursorColumn { col } => {
                self.screens
                    .active_mut()
                    .cursor_column_basic(self.size.cols, col);
                Ok(())
            }
            Action::CursorRow { row } => {
                self.screens
                    .active_mut()
                    .cursor_row_basic(self.size.rows, row);
                Ok(())
            }
            Action::CursorRowRelative { rows } => {
                self.screens
                    .active_mut()
                    .cursor_row_relative_basic(self.size.rows, rows);
                Ok(())
            }
            Action::CursorPosition { row, col } => {
                self.screens.active_mut().cursor_position_basic(
                    row,
                    col,
                    self.size.rows,
                    self.size.cols,
                );
                Ok(())
            }
            Action::EraseDisplay { mode, protected } => self
                .screens
                .active_mut()
                .erase_display_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::EraseLine { mode, protected } => self
                .screens
                .active_mut()
                .erase_line_basic(mode, self.size.rows, self.size.cols, protected)
                .map_err(TerminalStreamError::from),
            Action::InsertChars { count } => self
                .screens
                .active_mut()
                .insert_chars_basic(
                    count,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::DeleteChars { count } => self
                .screens
                .active_mut()
                .delete_chars_basic(
                    count,
                    self.size.rows,
                    self.scrolling_region.left,
                    self.scrolling_region.right,
                )
                .map_err(TerminalStreamError::from),
            Action::EraseChars { count } => self
                .screens
                .active_mut()
                .erase_chars_basic(count, self.size.rows, self.size.cols)
                .map_err(TerminalStreamError::from),
            Action::InsertLines { count } => self
                .screens
                .active_mut()
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
                .screens
                .active_mut()
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
                .screens
                .active_mut()
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
                .screens
                .active_mut()
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
            Action::SetMode { mode } => self.set_mode_basic(mode, true),
            Action::ResetMode { mode } => self.set_mode_basic(mode, false),
            Action::SaveMode { mode } => {
                self.modes.save(mode);
                Ok(())
            }
            Action::RestoreMode { mode } => {
                let enabled = self.modes.restore(mode);
                self.set_mode_basic(mode, enabled)
            }
            Action::MouseShiftCapture { enabled } => {
                self.flags.mouse_shift_capture = Some(enabled);
                Ok(())
            }
            Action::KittyKeyboardQuery => {
                let flags = self.screens.active().kitty_keyboard_flags().int();
                self.write_pty_response(&format!("\x1b[?{flags}u"));
                Ok(())
            }
            Action::KittyKeyboardPush { flags } => {
                self.screens.active_mut().push_kitty_keyboard(flags);
                Ok(())
            }
            Action::KittyKeyboardPop { count } => {
                self.screens
                    .active_mut()
                    .pop_kitty_keyboard(usize::from(count));
                Ok(())
            }
            Action::KittyKeyboardSet { mode, flags } => {
                self.screens.active_mut().set_kitty_keyboard(mode, flags);
                Ok(())
            }
            Action::SaveCursor => {
                self.screens
                    .active_mut()
                    .save_cursor(self.modes.get(modes::Mode::Origin));
                Ok(())
            }
            Action::RestoreCursor => {
                self.restore_cursor_from_active_saved();
                Ok(())
            }
            Action::ReverseIndex => self.reverse_index(),
            Action::FullReset => self.full_reset(),
            Action::ConfigureCharset { slot, charset } => {
                self.configure_charset(slot, charset);
                Ok(())
            }
            Action::InvokeCharset { bank, slot, single } => {
                self.invoke_charset(bank, slot, single);
                Ok(())
            }
            Action::CursorVisualStyle { style, blinking } => {
                self.modes.set(modes::Mode::CursorBlinking, blinking);
                self.screens.active_mut().set_cursor_visual_style(style);
                Ok(())
            }
            Action::DcsHook { value } => {
                if let Some(command) = self.dcs.hook(value) {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::DcsPut { byte } => {
                if let Some(command) = self.dcs.put(byte) {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::DcsUnhook => {
                if let Some(command) = self.dcs.unhook() {
                    self.dcs_command(command);
                }
                Ok(())
            }
            Action::ApcStart | Action::ApcPut { .. } | Action::ApcEnd => Ok(()),
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
            Action::DeviceAttributes { request } => {
                let response = device_attributes::Attributes::default().encode_vt(request);
                self.write_pty_response(&response);
                Ok(())
            }
            Action::DeviceStatus { request } => {
                self.device_status(request);
                Ok(())
            }
            Action::XtVersion => {
                self.write_pty_response("\x1bP>|libroastty\x1b\\");
                Ok(())
            }
            Action::SetAttribute { attr } => {
                self.screens.active_mut().set_attribute_basic(attr);
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
            stream::OscAction::ClipboardContents { .. } => {}
            stream::OscAction::ContextSignal { .. } => {}
            stream::OscAction::DesktopNotification { .. } => {}
            stream::OscAction::MouseShape { shape } => {
                *self.mouse_shape = shape;
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
                self.screens.active_mut().set_cursor_hyperlink(id, uri);
            }
            stream::OscAction::EndHyperlink => {
                self.screens.active_mut().clear_cursor_hyperlink();
            }
            stream::OscAction::ColorOperation { requests } => {
                self.color_operation(requests);
            }
            stream::OscAction::KittyColor {
                requests,
                terminator,
            } => {
                self.kitty_color_operation(requests, terminator);
            }
            stream::OscAction::KittyTextSizing { .. } => {}
            stream::OscAction::SemanticPrompt { value } => {
                self.semantic_prompt(value)?;
            }
            stream::OscAction::KittyClipboard { .. } => {}
        }
        Ok(())
    }
}

impl TerminalStreamHandler<'_> {
    fn print(&mut self, cp: char) -> Result<(), TerminalStreamError> {
        if cp.is_ascii_control() {
            return Err(TerminalStreamError::UnsupportedCodepoint(cp));
        }

        *self.previous_char = Some(cp);
        self.screens
            .active_mut()
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

    fn print_repeat(&mut self, count: u16) -> Result<(), TerminalStreamError> {
        let Some(cp) = *self.previous_char else {
            return Ok(());
        };

        for _ in 0..count.max(1) {
            self.print(cp)?;
        }
        Ok(())
    }

    fn line_feed(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .line_feed_basic(self.size.rows, self.size.cols)
            .map_err(TerminalStreamError::from)?;
        if self.modes.get(modes::Mode::Linefeed) {
            self.screens.active_mut().carriage_return_basic();
        }
        Ok(())
    }

    fn index(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .line_feed_basic(self.size.rows, self.size.cols)
            .map_err(TerminalStreamError::from)
    }

    fn reverse_index(&mut self) -> Result<(), TerminalStreamError> {
        let (x, y) = self.screens.active_mut().cursor_position();
        if y != self.scrolling_region.top
            || x < self.scrolling_region.left
            || x > self.scrolling_region.right
        {
            self.screens.active_mut().cursor_up_basic(1);
            return Ok(());
        }

        self.screens
            .active_mut()
            .scroll_down_basic(
                1,
                self.scrolling_region.top,
                self.scrolling_region.bottom,
                self.scrolling_region.left,
                self.scrolling_region.right,
                self.scrolling_region.left == 0
                    && self.scrolling_region.right == self.size.cols - 1,
            )
            .map_err(TerminalStreamError::from)
    }

    fn set_mode_basic(
        &mut self,
        mode: modes::Mode,
        enabled: bool,
    ) -> Result<(), TerminalStreamError> {
        match mode {
            modes::Mode::AltScreenLegacy => {
                self.switch_screen_47(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            modes::Mode::AltScreen => {
                self.switch_screen_1047(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            modes::Mode::AltScreenSaveCursorClearEnter => {
                self.switch_screen_1049(enabled)?;
                self.modes.set(mode, enabled);
                return Ok(());
            }
            _ => {}
        }

        self.modes.set(mode, enabled);
        self.set_mouse_runtime_mode_flag(mode, enabled);

        match mode {
            modes::Mode::Origin => self.move_cursor_to_origin_home(),
            modes::Mode::EnableLeftAndRightMargin if !enabled => {
                self.scrolling_region.left = 0;
                self.scrolling_region.right = self.size.cols.saturating_sub(1);
            }
            modes::Mode::SaveCursor => {
                if enabled {
                    self.screens
                        .active_mut()
                        .save_cursor(self.modes.get(modes::Mode::Origin));
                } else {
                    self.restore_cursor_from_active_saved();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn set_mouse_runtime_mode_flag(&mut self, mode: modes::Mode, enabled: bool) {
        match mode {
            modes::Mode::MouseEventX10 => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::X10
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventNormal => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Normal
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventButton => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Button
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseEventAny => {
                self.flags.mouse_event = if enabled {
                    mouse::MouseEventMode::Any
                } else {
                    mouse::MouseEventMode::None
                };
            }
            modes::Mode::MouseFormatUtf8 => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Utf8
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatSgr => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Sgr
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatUrxvt => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::Urxvt
                } else {
                    mouse::MouseFormat::X10
                };
            }
            modes::Mode::MouseFormatSgrPixels => {
                self.flags.mouse_format = if enabled {
                    mouse::MouseFormat::SgrPixels
                } else {
                    mouse::MouseFormat::X10
                };
            }
            _ => {}
        }
    }

    fn write_pty_response(&mut self, bytes: &str) {
        self.pty_response.extend_from_slice(bytes.as_bytes());
    }

    fn write_pty_response_bytes(&mut self, bytes: &[u8]) {
        self.pty_response.extend_from_slice(bytes);
    }

    fn dcs_command(&mut self, command: dcs::Command) {
        match command {
            dcs::Command::Decrqss(request) => self.decrqss(request),
            dcs::Command::XtGettcap(mut request) => while request.next().is_some() {},
        }
    }

    fn full_reset(&mut self) -> Result<(), TerminalStreamError> {
        self.screens.reset();
        self.modes.reset();
        *self.scrolling_region = ScrollingRegion::full(self.size);
        self.tabstops.reset(TABSTOP_INTERVAL);
        self.title.clear();
        self.pwd.clear();
        *self.dcs = dcs::Handler::new();
        *self.flags = TerminalFlags::default();
        *self.previous_char = None;
        Ok(())
    }

    fn restore_cursor_from_active_saved(&mut self) {
        let saved = self.screens.active().saved_cursor_or_default();
        self.modes.set(modes::Mode::Origin, saved.origin());
        self.screens
            .active_mut()
            .restore_saved_cursor(saved, self.size.cols, self.size.rows);
    }

    fn switch_screen_47(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        let target = if enabled {
            TerminalScreenKey::Alternate
        } else {
            TerminalScreenKey::Primary
        };
        self.switch_screen_copy_cursor(target)
    }

    fn switch_screen_1047(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        if !enabled && self.screens.active_key() == TerminalScreenKey::Alternate {
            self.erase_active_display_complete()?;
        }

        let target = if enabled {
            TerminalScreenKey::Alternate
        } else {
            TerminalScreenKey::Primary
        };
        self.switch_screen_copy_cursor(target)
    }

    fn switch_screen_1049(&mut self, enabled: bool) -> Result<(), TerminalStreamError> {
        if enabled {
            self.screens
                .active_mut()
                .save_cursor(self.modes.get(modes::Mode::Origin));
            let old = self.switch_screen(TerminalScreenKey::Alternate)?;
            self.erase_active_display_complete()?;
            if let Some(old) = old {
                self.screens
                    .copy_cursor_from_to(old, TerminalScreenKey::Alternate);
            }
        } else {
            self.switch_screen(TerminalScreenKey::Primary)?;
            self.restore_cursor_from_active_saved();
        }
        Ok(())
    }

    fn switch_screen_copy_cursor(
        &mut self,
        target: TerminalScreenKey,
    ) -> Result<(), TerminalStreamError> {
        if let Some(old) = self.switch_screen(target)? {
            self.screens.copy_cursor_from_to(old, target);
        }
        Ok(())
    }

    fn switch_screen(
        &mut self,
        target: TerminalScreenKey,
    ) -> Result<Option<TerminalScreenKey>, TerminalStreamError> {
        self.screens
            .switch_to(target, self.size.cols, self.size.rows)
            .map_err(|_| TerminalStreamError::PageAlloc)
    }

    fn erase_active_display_complete(&mut self) -> Result<(), TerminalStreamError> {
        self.screens
            .active_mut()
            .erase_display_basic(
                stream::EraseDisplayMode::Complete,
                self.size.rows,
                self.size.cols,
                false,
            )
            .map_err(TerminalStreamError::from)
    }

    fn configure_charset(&mut self, slot: charsets::CharsetSlot, charset: charsets::Charset) {
        self.screens.active_mut().configure_charset(slot, charset);
    }

    fn invoke_charset(
        &mut self,
        bank: charsets::CharsetBank,
        slot: charsets::CharsetSlot,
        single: bool,
    ) {
        self.screens.active_mut().invoke_charset(bank, slot, single);
    }

    fn decrqss(&mut self, request: dcs::Decrqss) {
        let payload = match request {
            dcs::Decrqss::None => None,
            dcs::Decrqss::Sgr => Some(format!("{}m", self.decrqss_sgr_payload())),
            dcs::Decrqss::Decscusr => Some(format!(
                "{} q",
                self.screens
                    .active_mut()
                    .cursor_visual_style()
                    .decscusr_report(self.modes.get(modes::Mode::CursorBlinking))
            )),
            dcs::Decrqss::Decstbm => Some(format!(
                "{};{}r",
                self.scrolling_region.top + 1,
                self.scrolling_region.bottom + 1
            )),
            dcs::Decrqss::Decslrm => {
                self.modes
                    .get(modes::Mode::EnableLeftAndRightMargin)
                    .then(|| {
                        format!(
                            "{};{}s",
                            self.scrolling_region.left + 1,
                            self.scrolling_region.right + 1
                        )
                    })
            }
        };

        match payload {
            Some(payload) => self.write_pty_response(&format!("\x1bP1$r{payload}\x1b\\")),
            None => self.write_pty_response("\x1bP0$r\x1b\\"),
        }
    }

    fn decrqss_sgr_payload(&self) -> String {
        let style = self.screens.active().cursor_text_style();
        let mut output = String::from("0");

        if style.flags.bold {
            output.push_str(";1");
        }
        if style.flags.faint {
            output.push_str(";2");
        }
        if style.flags.italic {
            output.push_str(";3");
        }
        if style.flags.underline != sgr::Underline::None {
            output.push_str(";4");
        }
        if style.flags.blink {
            output.push_str(";5");
        }
        if style.flags.inverse {
            output.push_str(";7");
        }
        if style.flags.invisible {
            output.push_str(";8");
        }
        if style.flags.strikethrough {
            output.push_str(";9");
        }

        push_decrqss_color(&mut output, 38, 3, 9, style.fg_color);
        push_decrqss_color(&mut output, 48, 4, 10, style.bg_color);

        output
    }

    fn device_status(&mut self, request: device_status::Request) {
        match request {
            device_status::Request::OperatingStatus => self.write_pty_response("\x1b[0n"),
            device_status::Request::CursorPosition => {
                let (cursor_x, cursor_y) = self.screens.active_mut().cursor_position();
                let (x, y) = if self.modes.get(modes::Mode::Origin) {
                    (
                        cursor_x.saturating_sub(self.scrolling_region.left),
                        cursor_y.saturating_sub(self.scrolling_region.top),
                    )
                } else {
                    (cursor_x, cursor_y)
                };
                self.write_pty_response(&format!("\x1b[{};{}R", y + 1, x + 1));
            }
            device_status::Request::ColorScheme => {}
        }
    }

    fn color_operation(&mut self, requests: osc::ColorRequests) {
        for request in requests.iter() {
            match request {
                osc::ColorRequest::SetPalette { index, rgb } => {
                    self.colors.palette[index as usize] = rgb;
                }
                osc::ColorRequest::QueryPalette { index, terminator } => {
                    self.write_palette_query_response(index, terminator);
                }
                osc::ColorRequest::ResetPalette { index } => {
                    self.colors.palette[index as usize] = color::DEFAULT_PALETTE[index as usize];
                }
                osc::ColorRequest::ResetAllPalette => {
                    self.colors.palette = color::DEFAULT_PALETTE;
                }
                osc::ColorRequest::SetDynamic { target, rgb } => {
                    self.dynamic_color_mut(target).set(rgb);
                }
                osc::ColorRequest::QueryDynamic { target, terminator } => {
                    self.write_dynamic_query_response(target, terminator);
                }
                osc::ColorRequest::ResetDynamic { target } => {
                    self.dynamic_color_mut(target).reset();
                }
            }
        }
    }

    fn dynamic_color_mut(&mut self, target: osc::DynamicColor) -> &mut color::DynamicRgb {
        match target {
            osc::DynamicColor::Foreground => &mut self.colors.foreground,
            osc::DynamicColor::Background => &mut self.colors.background,
            osc::DynamicColor::Cursor => &mut self.colors.cursor,
        }
    }

    fn dynamic_color(&self, target: osc::DynamicColor) -> Option<color::Rgb> {
        match target {
            osc::DynamicColor::Foreground => self.colors.foreground.get(),
            osc::DynamicColor::Background => self.colors.background.get(),
            osc::DynamicColor::Cursor => self
                .colors
                .cursor
                .get()
                .or_else(|| self.colors.foreground.get()),
        }
    }

    fn write_palette_query_response(&mut self, index: u8, terminator: osc::Terminator) {
        let rgb = self.colors.palette[index as usize];
        let response = format!(
            "\x1b]4;{};rgb:{:04x}/{:04x}/{:04x}",
            index,
            u16::from(rgb.r) * 257,
            u16::from(rgb.g) * 257,
            u16::from(rgb.b) * 257
        );
        self.write_pty_response(&response);
        self.write_pty_response_bytes(terminator.bytes());
    }

    fn write_dynamic_query_response(
        &mut self,
        target: osc::DynamicColor,
        terminator: osc::Terminator,
    ) {
        let Some(rgb) = self.dynamic_color(target) else {
            return;
        };
        let response = format!(
            "\x1b]{};rgb:{:04x}/{:04x}/{:04x}",
            target.number(),
            u16::from(rgb.r) * 257,
            u16::from(rgb.g) * 257,
            u16::from(rgb.b) * 257
        );
        self.write_pty_response(&response);
        self.write_pty_response_bytes(terminator.bytes());
    }

    fn kitty_color_operation(
        &mut self,
        requests: super::kitty::ColorRequests,
        terminator: osc::Terminator,
    ) {
        let mut response = String::new();
        for request in requests.iter() {
            match request {
                super::kitty::ColorRequest::Set { key, rgb } => {
                    self.set_kitty_color(key, rgb);
                }
                super::kitty::ColorRequest::Reset(key) => {
                    self.reset_kitty_color(key);
                }
                super::kitty::ColorRequest::Query(key) => {
                    self.write_kitty_color_query(&mut response, key);
                }
            }
        }

        if !response.is_empty() {
            self.write_pty_response(&response);
            self.write_pty_response_bytes(terminator.bytes());
        }
    }

    fn semantic_prompt(
        &mut self,
        prompt: super::semantic_prompt::SemanticPrompt<'_>,
    ) -> Result<(), TerminalStreamError> {
        use super::semantic_prompt::Action;

        match prompt.action {
            Action::FreshLine => self.semantic_prompt_fresh_line(),
            Action::FreshLineNewPrompt => {
                self.semantic_prompt_fresh_line()?;
                self.screens
                    .active_mut()
                    .set_cursor_semantic_prompt(
                        prompt
                            .prompt_kind()
                            .unwrap_or(super::semantic_prompt::PromptKind::Initial),
                    )
                    .map_err(TerminalStreamError::from)
            }
            Action::NewCommand => {
                self.semantic_prompt(super::semantic_prompt::SemanticPrompt::new(
                    Action::FreshLineNewPrompt,
                    prompt.options(),
                ))
            }
            Action::PromptStart => self
                .screens
                .active_mut()
                .set_cursor_semantic_prompt(
                    prompt
                        .prompt_kind()
                        .unwrap_or(super::semantic_prompt::PromptKind::Initial),
                )
                .map_err(TerminalStreamError::from),
            Action::EndPromptStartInput => {
                self.screens.active_mut().set_cursor_semantic_input(false);
                Ok(())
            }
            Action::EndPromptStartInputTerminateEol => {
                self.screens.active_mut().set_cursor_semantic_input(true);
                Ok(())
            }
            Action::EndInputStartOutput => {
                self.screens.active_mut().set_cursor_semantic_output();
                if self.screens.active_mut().cursor_position().0 == 0
                    && self.screens.active_mut().current_row_semantic_prompt()
                        != Some(SemanticPrompt::None)
                {
                    self.screens
                        .active_mut()
                        .clear_current_row_semantic_prompt()
                        .map_err(TerminalStreamError::from)?;
                }
                Ok(())
            }
            Action::EndCommand => {
                self.screens.active_mut().set_cursor_semantic_output();
                Ok(())
            }
        }
    }

    fn semantic_prompt_fresh_line(&mut self) -> Result<(), TerminalStreamError> {
        let left_margin =
            if self.screens.active_mut().cursor_position().0 < self.scrolling_region.left {
                0
            } else {
                self.scrolling_region.left
            };
        if self.screens.active_mut().cursor_position().0 == left_margin {
            return Ok(());
        }

        self.screens.active_mut().carriage_return_basic();
        self.index()
    }

    fn set_kitty_color(&mut self, key: super::kitty::ColorKind, rgb: color::Rgb) {
        match key {
            super::kitty::ColorKind::Palette(index) => {
                self.colors.palette[index as usize] = rgb;
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => self.colors.foreground.set(rgb),
                super::kitty::ColorSpecial::Background => self.colors.background.set(rgb),
                super::kitty::ColorSpecial::Cursor => self.colors.cursor.set(rgb),
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn reset_kitty_color(&mut self, key: super::kitty::ColorKind) {
        match key {
            super::kitty::ColorKind::Palette(index) => {
                self.colors.palette[index as usize] = color::DEFAULT_PALETTE[index as usize];
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => self.colors.foreground.reset(),
                super::kitty::ColorSpecial::Background => self.colors.background.reset(),
                super::kitty::ColorSpecial::Cursor => self.colors.cursor.reset(),
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn write_kitty_color_query(&self, response: &mut String, key: super::kitty::ColorKind) {
        if response.is_empty() {
            response.push_str("\x1b]21");
        }

        match key {
            super::kitty::ColorKind::Palette(index) => {
                append_kitty_color_response(
                    response,
                    key,
                    Some(self.colors.palette[index as usize]),
                );
            }
            super::kitty::ColorKind::Special(special) => match special {
                super::kitty::ColorSpecial::Foreground => {
                    append_kitty_color_response(response, key, self.colors.foreground.get());
                }
                super::kitty::ColorSpecial::Background => {
                    append_kitty_color_response(response, key, self.colors.background.get());
                }
                super::kitty::ColorSpecial::Cursor => {
                    append_kitty_color_response(response, key, self.colors.cursor.get());
                }
                super::kitty::ColorSpecial::SelectionForeground
                | super::kitty::ColorSpecial::SelectionBackground
                | super::kitty::ColorSpecial::CursorText
                | super::kitty::ColorSpecial::VisualBell
                | super::kitty::ColorSpecial::SecondTransparentBackground => {}
            },
        }
    }

    fn move_cursor_to_origin_home(&mut self) {
        let (x, y) = if self.modes.get(modes::Mode::Origin) {
            (self.scrolling_region.left, self.scrolling_region.top)
        } else {
            (0, 0)
        };
        self.screens.active_mut().cursor_position_basic(
            y.saturating_add(1),
            x.saturating_add(1),
            self.size.rows,
            self.size.cols,
        );
    }
}

fn push_decrqss_color(
    output: &mut String,
    extended_prefix: u8,
    normal_prefix: u8,
    bright_prefix: u8,
    color: style::Color,
) {
    match color {
        style::Color::None => {}
        style::Color::Palette(idx) if idx < 8 => {
            output.push_str(&format!(";{}{}", normal_prefix, idx));
        }
        style::Color::Palette(idx) if idx < 16 => {
            output.push_str(&format!(";{}{}", bright_prefix, idx - 8));
        }
        style::Color::Palette(idx) => {
            output.push_str(&format!(";{}:5:{}", extended_prefix, idx));
        }
        style::Color::Rgb(rgb) => {
            output.push_str(&format!(
                ";{}:2::{}:{}:{}",
                extended_prefix, rgb.r, rgb.g, rgb.b
            ));
        }
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

impl From<super::page_list::BasicCellWriteError> for TerminalStreamError {
    fn from(err: super::page_list::BasicCellWriteError) -> Self {
        match err {
            super::page_list::BasicCellWriteError::InvalidPoint => Self::InvalidPoint,
            super::page_list::BasicCellWriteError::ManagedCell => Self::ManagedCellUnsupported,
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

    fn clear(&mut self) {
        self.text.clear();
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
            &ScreenFormatter::init(self.terminal.screens.active(), self.options.screen)
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
        let mut output = ScreenFormatter::init(self.terminal.screens.active(), self.options.screen)
            .with_content(self.content)
            .with_extra(self.extra.screen)
            .format_with_pin_map();

        if !prefix.is_empty() {
            let top_left = self.terminal.screens.active().top_left_pin();
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
                .unwrap_or_else(|| self.terminal.screens.active().top_left_pin());
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

fn append_kitty_color_response(
    output: &mut String,
    key: super::kitty::ColorKind,
    rgb: Option<color::Rgb>,
) {
    output.push(';');
    key.append_to_string(output);
    output.push('=');
    if let Some(rgb) = rgb {
        output.push_str(&format!("rgb:{:02x}/{:02x}/{:02x}", rgb.r, rgb.g, rgb.b));
    }
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
    use crate::terminal::cursor;
    use crate::terminal::kitty::{KeyFlags, KeySetMode};
    use crate::terminal::modes::Mode;
    use crate::terminal::page::{HyperlinkSnapshotId, SemanticContent, SemanticPrompt};
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
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(lines);
        terminal
    }

    fn active_pin(terminal: &Terminal, x: CellCountInt, y: u32) -> Pin {
        terminal.screens.active().pin_for_tests(x, y)
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
            terminal.screens.active(),
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_cursor_style_for_tests(style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            });
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("idé".to_string()),
                "https://e.test/é",
            );
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
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

        terminal.next_slice(b"A\x0eB\x1bxC\x1b[?ZD").unwrap();

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
    fn terminal_stream_save_cursor_restore_cursor_round_trips_position_and_text_state() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x1b[1;31m").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal.next_slice(b"\x1b[3;32m").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(false);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 0));
        assert!(terminal.cursor_protected_for_tests());
        assert_eq!(
            terminal.cursor_style_for_tests(),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_round_trips_pending_wrap_origin_and_charset() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal.next_slice(b"hello").unwrap();
        assert!(terminal.cursor_pending_wrap_for_tests());
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal.next_slice(b"\x1b7").unwrap();

        terminal.set_mode_for_tests(Mode::Origin, false);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
        terminal.next_slice(b"\x1b[2 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::Utf8);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G0);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
        assert!(terminal.cursor_pending_wrap_for_tests());
        assert!(terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "hello\x1b(0\x0e"
        );
    }

    #[test]
    fn terminal_stream_restore_cursor_without_save_uses_ghostty_defaults() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();

        terminal
            .next_slice(b"hello\x1b[1;31m\x1b[?6h\x1b[5 q")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
        assert_eq!(terminal.cursor_style_for_tests(), style::Style::default());
        assert!(!terminal.cursor_protected_for_tests());
        assert!(!terminal.cursor_pending_wrap_for_tests());
        assert!(!terminal.get_mode_for_tests(Mode::Origin));
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Bar
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_does_not_restore_excluded_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("saved".to_string()),
                "https://saved.example",
            );
        terminal
            .next_slice(b"\x1b]133;A\x07\x1b7\x1b[4 q\x1b]133;B\x07")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("current".to_string()),
                "https://current.example",
            );
        terminal.next_slice(b"\x1b8X").unwrap();

        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Underline
        );
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("current".to_string()),
                "https://current.example"
            ))
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Input
        );
    }

    #[test]
    fn terminal_stream_save_cursor_restore_cursor_does_not_mutate_cells_dirty_rows_or_responses() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal.clear_dirty_for_tests();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 1);
        terminal.next_slice(b"\x1b8").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
    }

    #[test]
    fn terminal_stream_ris_full_reset_clears_screen_and_cursor_state() {
        let mut terminal = Terminal::init(10, 3, Some(10)).unwrap();

        terminal.next_slice(b"one\ntwo\nthree\nfour").unwrap();
        assert!(terminal.scrollback_rows_for_tests() > 0);
        terminal.next_slice(b"\x1b[1;31m\x1b[5 q").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("link".to_string()),
                "https://example.test",
            );
        terminal
            .next_slice(b"\x1b]133;A\x07prompt\x1b]133;B\x07input")
            .unwrap();
        terminal.next_slice(b"\x1b7").unwrap();

        terminal.next_slice(b"\x1bc").unwrap();
        terminal.next_slice(b"\x1b8X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X");
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
        assert_eq!(terminal.cursor_style_for_tests(), style::Style::default());
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Block
        );
        assert!(!terminal.cursor_protected_for_tests());
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Output
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::None
        );
    }

    #[test]
    fn terminal_stream_ris_full_reset_clears_terminal_global_state() {
        let mut terminal = Terminal::init(20, 4, None).unwrap();

        terminal.next_slice(b"\x1b]0;title\x07").unwrap();
        terminal.set_pwd_for_tests("file://host/home");
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.save_mode_for_tests(Mode::Insert);
        terminal.set_modify_other_keys_2_for_tests(true);
        terminal
            .next_slice(b"\x1b[?1003h\x1b[?1016h\x1b[>1s")
            .unwrap();
        terminal.clear_tabstops_for_tests();
        terminal.set_tabstop_for_tests(1);
        terminal.set_scrolling_region_for_tests(1, 2, 3, 10);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1bc").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Insert));
        assert!(!terminal.restore_mode_for_tests(Mode::Insert));
        assert!(!terminal.modify_other_keys_2_for_tests());
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
        assert_eq!(terminal.mouse_shift_capture_for_tests(), None);
        assert!(!terminal.get_tabstop_for_tests(1));
        assert!(terminal.get_tabstop_for_tests(8));
        assert!(terminal.get_tabstop_for_tests(16));
        assert_eq!(terminal.title_for_tests(), "");
        assert_eq!(terminal.pwd_for_tests(), None);
        assert_eq!(
            terminal.scrolling_region_for_tests(),
            ScrollingRegion::full(terminal.size)
        );
        assert!(terminal.pty_response_for_tests().is_empty());
        for row in 0..4 {
            assert!(terminal.is_dirty_for_tests(0, row));
            assert!(terminal.is_dirty_for_tests(19, row));
        }
    }

    #[test]
    fn terminal_stream_ris_full_reset_returns_to_primary_and_drops_alternate() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri\x1b[?47halt\x1bc").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(!terminal.alternate_initialized_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "");

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_charset_designation_maps_printed_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice("é\x1b(0`\x1b(A#\x1b(Bé".as_bytes())
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "é◆£é");
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_default_and_ascii_charset_preserve_non_ascii() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("😀\x1b(B😀".as_bytes()).unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "😀😀");
    }

    #[test]
    fn terminal_stream_mapped_charset_replaces_non_u8_codepoint_with_space() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("\x1b(0😀".as_bytes()).unwrap();

        assert_eq!(terminal.full_screen_plain_for_tests(false), " ");
    }

    #[test]
    fn terminal_stream_invoke_charset_switches_gl_slots() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b)0`\x0e``\x0f`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆◆`");
    }

    #[test]
    fn terminal_stream_charset_single_shift_affects_one_character() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b*0`\x1bN``").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆`");
    }

    #[test]
    fn terminal_stream_charset_gr_invocation_round_trips_without_affecting_print() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b|`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "`\x1b|"
        );
    }

    #[test]
    fn terminal_stream_charset_save_restore_preserves_designations_and_shifts() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"a\x1b*0\x1bN\x1b7b\x1b8``").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "a◆`");
    }

    #[test]
    fn terminal_stream_charset_save_restore_preserves_gl_and_gr_invocation() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b)0\x0e\x1b|\x1b7\x0f\x1b~\x1b8`")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "◆\x1b)0\x0e\x1b|"
        );
    }

    #[test]
    fn terminal_stream_charset_controls_do_not_dirty_rows_or_write_responses() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b(0\x1b)0\x0e\x1b~").unwrap();

        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(terminal.pty_response_for_tests().is_empty());

        terminal.next_slice(b"`").unwrap();
        assert!(terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_print_repeat_without_previous_char_is_noop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 1));
    }

    #[test]
    fn terminal_stream_print_repeat_uses_default_and_explicit_counts() {
        for (input, expected) in [
            (b"A\x1b[b".as_slice(), "AA"),
            (b"A\x1b[0b".as_slice(), "AA"),
            (b"A\x1b[3b".as_slice(), "AAAA"),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(plain_with_unwrap(&terminal, false), expected);
        }
    }

    #[test]
    fn terminal_stream_print_repeat_uses_normal_wrap_path() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"    A\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "    A\nA");
    }

    #[test]
    fn terminal_stream_print_repeat_respects_disabled_wraparound() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?7l    A\x1b[3b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "    A");
    }

    #[test]
    fn terminal_stream_print_repeat_uses_current_style_and_hyperlink() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"A\x1b[1m\x1b]8;;https://rep\x1b\\\x1b[b")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AA");
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style::default()
        );
        assert!(terminal.active_cell_style_for_tests(1, 0).flags.bold);
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(1, 0)
                .unwrap()
                .uri,
            b"https://rep"
        );
    }

    #[test]
    fn terminal_stream_print_repeat_maps_unmapped_char_through_current_charset() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b(0`\x1b(B\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆`");
    }

    #[test]
    fn terminal_stream_print_repeat_consumes_pending_single_shift_once() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b*0`\x1bN\x1b[b\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`◆`");
    }

    #[test]
    fn terminal_stream_repeat_previous_save_restore_cursor_does_not_restore_previous_char() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1b7B\x1b8\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
    }

    #[test]
    fn terminal_stream_repeat_previous_ris_resets_previous_char() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"A\x1bc\x1b[b").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_ris_resets_charset_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b(0\x1b*0\x1bN\x1b|\x1bc`")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "`");
        assert_eq!(
            formatter(&terminal, PageOutputFormat::Vt)
                .with_extra(
                    TerminalFormatterExtra::none()
                        .screen(ScreenFormatterExtra::none().charsets(true))
                )
                .format(),
            "`"
        );
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
    fn terminal_stream_mouse_event_modes_update_runtime_cache() {
        let cases = [
            (
                b"\x1b[?9h".as_slice(),
                b"\x1b[?9l".as_slice(),
                mouse::MouseEventMode::X10,
            ),
            (
                b"\x1b[?1000h".as_slice(),
                b"\x1b[?1000l".as_slice(),
                mouse::MouseEventMode::Normal,
            ),
            (
                b"\x1b[?1002h".as_slice(),
                b"\x1b[?1002l".as_slice(),
                mouse::MouseEventMode::Button,
            ),
            (
                b"\x1b[?1003h".as_slice(),
                b"\x1b[?1003l".as_slice(),
                mouse::MouseEventMode::Any,
            ),
        ];

        for (set, reset, expected) in cases {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            assert_eq!(
                terminal.mouse_event_for_tests(),
                mouse::MouseEventMode::None
            );

            terminal.next_slice(set).unwrap();
            assert_eq!(terminal.mouse_event_for_tests(), expected);

            terminal.next_slice(reset).unwrap();
            assert_eq!(
                terminal.mouse_event_for_tests(),
                mouse::MouseEventMode::None
            );
        }
    }

    #[test]
    fn terminal_stream_mouse_event_runtime_cache_uses_last_command() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1000h\x1b[?1003h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert!(terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(terminal.mouse_event_for_tests(), mouse::MouseEventMode::Any);

        terminal.next_slice(b"\x1b[?1003l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert!(!terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );
    }

    #[test]
    fn terminal_stream_mouse_format_modes_update_runtime_cache() {
        let cases = [
            (
                b"\x1b[?1005h".as_slice(),
                b"\x1b[?1005l".as_slice(),
                mouse::MouseFormat::Utf8,
            ),
            (
                b"\x1b[?1006h".as_slice(),
                b"\x1b[?1006l".as_slice(),
                mouse::MouseFormat::Sgr,
            ),
            (
                b"\x1b[?1015h".as_slice(),
                b"\x1b[?1015l".as_slice(),
                mouse::MouseFormat::Urxvt,
            ),
            (
                b"\x1b[?1016h".as_slice(),
                b"\x1b[?1016l".as_slice(),
                mouse::MouseFormat::SgrPixels,
            ),
        ];

        for (set, reset, expected) in cases {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);

            terminal.next_slice(set).unwrap();
            assert_eq!(terminal.mouse_format_for_tests(), expected);

            terminal.next_slice(reset).unwrap();
            assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
        }
    }

    #[test]
    fn terminal_stream_mouse_format_runtime_cache_uses_last_command() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[?1006h\x1b[?1016h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgrPixels));
        assert_eq!(
            terminal.mouse_format_for_tests(),
            mouse::MouseFormat::SgrPixels
        );

        terminal.next_slice(b"\x1b[?1016l").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert!(!terminal.get_mode_for_tests(Mode::MouseFormatSgrPixels));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
    }

    #[test]
    fn terminal_stream_mouse_runtime_cache_follows_mode_save_restore() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b[?1003h\x1b[?1003s\x1b[?1003l\x1b[?1003r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(terminal.mouse_event_for_tests(), mouse::MouseEventMode::Any);

        terminal
            .next_slice(b"\x1b[?1003l\x1b[?1003s\x1b[?1003h\x1b[?1003r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::MouseEventAny));
        assert_eq!(
            terminal.mouse_event_for_tests(),
            mouse::MouseEventMode::None
        );

        terminal
            .next_slice(b"\x1b[?1006h\x1b[?1006s\x1b[?1006l\x1b[?1006r")
            .unwrap();

        assert!(terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::Sgr);

        terminal
            .next_slice(b"\x1b[?1006l\x1b[?1006s\x1b[?1006h\x1b[?1006r")
            .unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::MouseFormatSgr));
        assert_eq!(terminal.mouse_format_for_tests(), mouse::MouseFormat::X10);
    }

    #[test]
    fn terminal_stream_mouse_shift_capture_updates_runtime_flag() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(terminal.mouse_shift_capture_for_tests(), None);

        terminal.next_slice(b"\x1b[>1s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(true));

        terminal.next_slice(b"\x1b[>s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(false));

        terminal.next_slice(b"\x1b[>1s\x1b[>2s").unwrap();
        assert_eq!(terminal.mouse_shift_capture_for_tests(), Some(true));
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

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
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
    fn terminal_stream_alt_screen_legacy_switches_without_clearing() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(!terminal.alternate_initialized_for_tests());
        terminal.next_slice(b"pri\x1b[?47halt\x1b[?47l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert!(terminal.alternate_initialized_for_tests());
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (6, 0));

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(plain_with_unwrap(&terminal, false).contains("alt"));

        terminal.next_slice(b"\x1b[?47h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_has_no_scrollback_and_formatter_reads_active_screen() {
        let mut terminal = Terminal::init(10, 2, Some(10)).unwrap();

        terminal.next_slice(b"pri\nmore\nnext").unwrap();
        assert!(terminal.scrollback_rows_for_tests() > 0);
        terminal.next_slice(b"\x1b[?47h").unwrap();
        terminal.next_slice(b"\x1b[Ha\nb\nc\nd").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert_eq!(terminal.scrollback_rows_for_tests(), 0);
        assert!(plain_with_unwrap(&terminal, false).contains('d'));
        assert!(!plain_with_unwrap(&terminal, false).contains("pri"));
    }

    #[test]
    fn terminal_stream_alt_screen_1047_clears_alternate_on_leave() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.next_slice(b"\x1b[?1047h").unwrap();
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        terminal.next_slice(b"alt").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(7, 2);
        terminal.next_slice(b"\x1b[?1047l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (7, 2));

        terminal.next_slice(b"\x1b[?1047h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_1049_saves_cursor_and_clears_on_entry() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"pri").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.next_slice(b"\x1b[?1049halt").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?1049l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(plain_with_unwrap(&terminal, false), "pri");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));

        terminal.next_slice(b"\x1b[?1049h").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert!(!plain_with_unwrap(&terminal, false).contains("alt"));
    }

    #[test]
    fn terminal_stream_alt_screen_1049_reset_does_not_corrupt_primary_save() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.next_slice(b"\x1b[?1049h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
        terminal.next_slice(b"\x1b[?1049halt\x1b[?1049l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
    }

    #[test]
    fn terminal_stream_alt_screen_1048_saves_active_screen_without_switching() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
        terminal.next_slice(b"\x1b[?1048h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[?1048l").unwrap();
        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));

        terminal.next_slice(b"\x1b[?47h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 1);
        terminal.next_slice(b"\x1b[?1048h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[?1048l").unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Alternate
        );
        assert_eq!(terminal.cursor_position_for_tests(), (3, 1));
    }

    #[test]
    fn terminal_stream_alt_screen_switch_clears_hyperlink_and_carries_charset() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_hyperlink_for_tests(
                ScreenCursorHyperlinkId::Explicit("primary".to_string()),
                "https://primary.example",
            );
        terminal.next_slice(b"\x1b(0\x1b[?47h`").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "◆");
        terminal.next_slice(b"\x1b[?47l").unwrap();
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
    }

    #[test]
    fn terminal_stream_kitty_keyboard_query_reports_active_flags() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");

        terminal.next_slice(b"\x1b[>3u\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_push_pop_and_oversized_pop_follow_stack() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[>1u\x1b[>2u\x1b[?u\x1b[<u\x1b[?u")
            .unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?2u\x1b[?1u");

        terminal.next_slice(b"\x1b[>4u\x1b[<100u\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_multiple_pop_restores_earlier_value() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[>1u\x1b[>2u\x1b[>4u\x1b[<2u\x1b[?u")
            .unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?1u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_set_or_and_not_mutate_current_flags() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?u\x1b[=2;2u\x1b[?u\x1b[=1;3u\x1b[?u")
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?1u\x1b[?3u\x1b[?2u"
        );
    }

    #[test]
    fn terminal_stream_kitty_keyboard_primary_and_alternate_states_are_independent() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"\x1b[=1u\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u");

        terminal.next_slice(b"\x1b[=3u\x1b[?47l\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?1u");

        terminal.next_slice(b"\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_ris_clears_primary_and_future_alternate_state() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?47h\x1b[=3u\x1bc")
            .unwrap();

        assert_eq!(
            terminal.active_screen_key_for_tests(),
            TerminalScreenKey::Primary
        );
        terminal.next_slice(b"\x1b[?u\x1b[?47h\x1b[?u").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?0u\x1b[?0u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_invalid_forms_do_not_mutate_or_respond() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=3u\x1b[>32u\x1b[=1;4u\x1b[=1:1u\x1b[?u")
            .unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?3u");
    }

    #[test]
    fn terminal_stream_kitty_keyboard_lenient_parameter_forms_match_upstream() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b[=1u\x1b[?123u\x1b[>3;4u\x1b[?u\x1b[<2;3u\x1b[?u\x1b[=3;2;1u\x1b[?u")
            .unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1b[?1u\x1b[?0u\x1b[?1u\x1b[?3u"
        );
    }

    #[test]
    fn terminal_stream_csi_u_still_restores_cursor() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
        terminal.next_slice(b"\x1b7").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[u").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (5, 2));
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_alt_screen_does_not_fake_unrelated_deferred_modes() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[?3h\x1b[?1000h").unwrap();

        assert!(terminal.get_mode_for_tests(Mode::Column132));
        assert!(terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (4, 1));
        assert_eq!(terminal.size.cols, 10);
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));

        terminal.next_slice(b"\x1b[?3l\x1b[?1000l").unwrap();

        assert!(!terminal.get_mode_for_tests(Mode::Column132));
        assert!(!terminal.get_mode_for_tests(Mode::MouseEventNormal));
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
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

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 2);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);
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
    fn terminal_stream_query_response_enq_and_color_scheme_are_inert() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x05\x1b[?996n").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_query_response_device_attributes_and_decid() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[c").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?62;22c");

        terminal.next_slice(b"\x1b[>c").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[>1;0;0c");

        terminal.next_slice(b"\x1b[=c").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP!|00000000\x1b\\"
        );

        terminal.next_slice(b"\x1bZ").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[?62;22c");
    }

    #[test]
    fn terminal_stream_query_response_xtversion_uses_roastty_name() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[>0q").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1bP>|libroastty\x1b\\"
        );
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"ghostty".len())
            .any(|window| window == b"ghostty"));
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"Ghostty".len())
            .any(|window| window == b"Ghostty"));
        assert!(!terminal
            .pty_response_for_tests()
            .windows(b"Roastty".len())
            .any(|window| window == b"Roastty"));
    }

    #[test]
    fn terminal_stream_decrqss_default_sgr_response() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn terminal_stream_decrqss_sgr_flags_use_ghostty_order() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[1;2;3;4;5;7;8;9m").unwrap();
        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r0;1;2;3;4;5;7;8;9m\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_sgr_palette_colors() {
        for (sgr, expected) in [
            (
                b"\x1b[30;40m".as_slice(),
                b"\x1bP1$r0;30;40m\x1b\\".as_slice(),
            ),
            (b"\x1b[91;103m", b"\x1bP1$r0;91;103m\x1b\\"),
            (
                b"\x1b[38;5;16;48;5;255m",
                b"\x1bP1$r0;38:5:16;48:5:255m\x1b\\",
            ),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(sgr).unwrap();
            terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_decrqss_sgr_rgb_colors() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[38;2;1;2;3;48;2;4;5;6m").unwrap();
        terminal.next_slice(b"\x1bP$qm\x1b\\").unwrap();

        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r0;38:2::1:2:3;48:2::4:5:6m\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_scrolling_region_responses() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 3, 2, 8);
        terminal.next_slice(b"\x1bP$qr\x1b\\").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r2;4r\x1b\\"
        );

        terminal.next_slice(b"\x1bP$qs\x1b\\").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP0$r\x1b\\");

        terminal.set_mode_for_tests(Mode::EnableLeftAndRightMargin, true);
        terminal.next_slice(b"\x1bP$qs\x1b\\").unwrap();
        assert_eq!(
            terminal.take_pty_response_for_tests(),
            b"\x1bP1$r3;9s\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_decrqss_unsupported_requests_return_invalid() {
        for input in [b"\x1bP$qz\x1b\\".as_slice(), b"\x1bP$qx\x1b\\"] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP0$r\x1b\\");
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
        }
    }

    #[test]
    fn terminal_stream_decscusr_sets_cursor_visual_style_and_blinking() {
        for (input, expected_style, expected_blinking) in [
            (b"\x1b[1 q".as_slice(), cursor::VisualStyle::Block, true),
            (b"\x1b[2 q", cursor::VisualStyle::Block, false),
            (b"\x1b[3 q", cursor::VisualStyle::Underline, true),
            (b"\x1b[4 q", cursor::VisualStyle::Underline, false),
            (b"\x1b[5 q", cursor::VisualStyle::Bar, true),
            (b"\x1b[6 q", cursor::VisualStyle::Bar, false),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_visual_style_for_tests(), expected_style);
            assert_eq!(
                terminal.get_mode_for_tests(Mode::CursorBlinking),
                expected_blinking
            );
        }
    }

    #[test]
    fn terminal_stream_decscusr_default_sets_steady_block() {
        for input in [b"\x1b[ q".as_slice(), b"\x1b[0 q"] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(b"\x1b[5 q").unwrap();
            terminal.next_slice(input).unwrap();

            assert_eq!(
                terminal.cursor_visual_style_for_tests(),
                cursor::VisualStyle::Block
            );
            assert!(!terminal.get_mode_for_tests(Mode::CursorBlinking));
        }
    }

    #[test]
    fn terminal_stream_decscusr_does_not_mutate_visible_terminal_content() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc\x1b[1;31m").unwrap();
        let text_style = terminal.cursor_style_for_tests();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1b[5 q").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert_eq!(terminal.cursor_style_for_tests(), text_style);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
        assert_eq!(
            terminal.cursor_visual_style_for_tests(),
            cursor::VisualStyle::Bar
        );
        assert!(terminal.get_mode_for_tests(Mode::CursorBlinking));
    }

    #[test]
    fn terminal_stream_decrqss_decscusr_reports_current_cursor_visual_style() {
        for (setup, expected) in [
            (b"".as_slice(), b"\x1bP1$r2 q\x1b\\".as_slice()),
            (b"\x1b[1 q", b"\x1bP1$r1 q\x1b\\"),
            (b"\x1b[4 q", b"\x1bP1$r4 q\x1b\\"),
            (b"\x1b[5 q", b"\x1bP1$r5 q\x1b\\"),
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();

            terminal.next_slice(setup).unwrap();
            terminal.next_slice(b"\x1bP$q q\x1b\\").unwrap();

            assert_eq!(terminal.take_pty_response_for_tests(), expected);
        }
    }

    #[test]
    fn terminal_stream_decscusr_split_feed_then_decrqss_reports_updated_style() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b[5").unwrap();
        terminal.next_slice(b" q").unwrap();
        terminal.next_slice(b"\x1bP$q q\x1b\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r5 q\x1b\\");
    }

    #[test]
    fn terminal_stream_decrqss_over_capacity_is_ignored() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bP$q\" q\x1b\\").unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_dcs_command_state_survives_split_feed() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1bP$q").unwrap();
        terminal.next_slice(b"m").unwrap();
        terminal.next_slice(b"\x1b").unwrap();
        terminal.next_slice(b"\\").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1bP1$r0m\x1b\\");
    }

    #[test]
    fn terminal_stream_dcs_command_xtgettcap_runtime_deferred() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.clear_dirty_for_tests();
        terminal.next_slice(b"\x1bP+q").unwrap();
        terminal.next_slice(b"536d756C78").unwrap();
        terminal.next_slice(b"\x1b\\").unwrap();

        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_dcs_command_tmux_and_unknown_are_ignored() {
        for input in [
            b"\x1bP1000ppayload\x1b\\".as_slice(),
            b"\x1bPApayload\x1b\\",
        ] {
            let mut terminal = Terminal::init(10, 2, None).unwrap();
            terminal.next_slice(b"abc").unwrap();
            terminal.clear_dirty_for_tests();

            terminal.next_slice(input).unwrap();

            assert!(terminal.pty_response_for_tests().is_empty());
            assert_eq!(plain_with_unwrap(&terminal, false), "abc");
            assert!(!terminal.is_dirty_for_tests(0, 0));
            assert!(!terminal.is_dirty_for_tests(9, 0));
        }
    }

    #[test]
    fn terminal_stream_query_response_device_status_reports() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.next_slice(b"\x1b[5n").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[0n");

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal.next_slice(b"\x1b[6n").unwrap();
        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[3;5R");
    }

    #[test]
    fn terminal_stream_query_response_cursor_report_respects_origin_mode() {
        let mut terminal = Terminal::init(10, 5, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 4, 2, 8);
        terminal.next_slice(b"\x1b[?6h").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 3);
        terminal.next_slice(b"\x1b[6n").unwrap();

        assert_eq!(terminal.take_pty_response_for_tests(), b"\x1b[3;4R");
    }

    #[test]
    fn terminal_stream_dcs_unknown_and_apc_sequences_are_runtime_noops() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        terminal.clear_dirty_for_tests();
        terminal
            .next_slice(b"\x1bPAignored\x1b\\\x1b_Gpayload\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
        assert!(!terminal.is_dirty_for_tests(0, 1));
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
    fn terminal_stream_osc1337_current_dir_updates_pwd() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]1337;CurrentDir=file://host/osc1337\x07")
            .unwrap();

        assert_eq!(terminal.pwd_for_tests(), Some("file://host/osc1337"));
    }

    #[test]
    fn terminal_stream_osc1337_copy_is_terminal_noop() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_pwd_for_tests("file://host/original");
        terminal.clear_dirty_for_tests();
        terminal
            .next_slice(b"\x1b]1337;Copy=:YWJjMTIz\x07")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/original"));
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_osc22_updates_mouse_shape() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Text);

        terminal.next_slice(b"\x1b]22;pointer\x07").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Pointer);

        terminal.next_slice(b"\x1b]22;pointer\x1b\\").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Pointer);

        terminal.next_slice(b"\x1b]22;left_ptr\x07").unwrap();
        assert_eq!(terminal.mouse_shape_for_tests(), mouse::MouseShape::Default);
    }

    #[test]
    fn terminal_stream_osc22_unknown_shape_does_not_mutate_state() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice(b"\x1b]22;crosshair\x07").unwrap();
        assert_eq!(
            terminal.mouse_shape_for_tests(),
            mouse::MouseShape::Crosshair
        );

        terminal.next_slice(b"\x1b]22;Crosshair\x07").unwrap();
        terminal.next_slice(b"\x1b]22;not-a-shape\x07").unwrap();

        assert_eq!(
            terminal.mouse_shape_for_tests(),
            mouse::MouseShape::Crosshair
        );
        assert!(terminal.pty_response_for_tests().is_empty());
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_osc66_text_sizing_is_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]66;s=2:w=7:n=13:d=15:v=1:h=2;wide\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        for row in 0..3 {
            assert!(!terminal.is_dirty_for_tests(0, row));
            assert!(!terminal.is_dirty_for_tests(9, row));
        }
    }

    #[test]
    fn terminal_stream_osc133_marks_prompt_input_and_output() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal
            .next_slice(b"abc\x1b]133;A\x07$ \x1b]133;B\x07cmd\x1b]133;C\x07out")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc\n$ cmdout");
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 1),
            SemanticContent::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 1),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(5, 1),
            SemanticContent::Output
        );
        assert!(terminal.pty_response_for_tests().is_empty());
    }

    #[test]
    fn terminal_stream_osc133_new_command_and_prompt_kinds() {
        let mut terminal = Terminal::init(10, 4, None).unwrap();

        terminal.next_slice(b"\x1b]133;N\x07one").unwrap();
        terminal.next_slice(b"\n\x1b]133;P;k=c\x07two").unwrap();
        terminal.next_slice(b"\n\x1b]133;P;k=s\x07tri").unwrap();

        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(2),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(0, 0),
            SemanticContent::Prompt
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(3, 1),
            SemanticContent::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_clear_eol_resets_on_newline_without_bulk_marking() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b]133;A\x07> \x1b]133;I\x07cmd\nout")
            .unwrap();

        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
        assert_eq!(
            terminal.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::None
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 0),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(7, 0),
            SemanticContent::Output
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(3, 1),
            SemanticContent::Output
        );
    }

    #[test]
    fn terminal_stream_osc133_marks_linefeed_and_soft_wrap_continuations() {
        let mut newline = Terminal::init(8, 3, None).unwrap();
        newline
            .next_slice(b"\x1b]133;A\x07> \x1b]133;B\x07cmd\nnext")
            .unwrap();
        assert_eq!(
            newline.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            newline.active_cell_semantic_content_for_tests(5, 1),
            SemanticContent::Input
        );

        let mut wrapped = Terminal::init(4, 3, None).unwrap();
        wrapped.next_slice(b"\x1b]133;A\x07abcde").unwrap();
        assert_eq!(
            wrapped.active_row_semantic_prompt_for_tests(1),
            SemanticPrompt::PromptContinuation
        );
        assert_eq!(
            wrapped.active_cell_semantic_content_for_tests(0, 1),
            SemanticContent::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_fresh_line_uses_ghostty_margin_rule() {
        let mut at_zero = Terminal::init(8, 4, None).unwrap();
        at_zero.set_scrolling_region_for_tests(0, 3, 2, 7);
        at_zero.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(at_zero.cursor_position_for_tests(), (0, 0));

        let mut at_margin = Terminal::init(8, 4, None).unwrap();
        at_margin.set_scrolling_region_for_tests(0, 3, 2, 7);
        at_margin
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        at_margin.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(at_margin.cursor_position_for_tests(), (2, 0));

        let mut left_of_margin = Terminal::init(8, 4, None).unwrap();
        left_of_margin.set_scrolling_region_for_tests(0, 3, 2, 7);
        left_of_margin
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        left_of_margin.next_slice(b"\x1b]133;L\x07").unwrap();
        assert_eq!(left_of_margin.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_osc133_c_clears_prompt_row_only_at_column_zero() {
        let mut at_zero = Terminal::init(8, 3, None).unwrap();
        at_zero.next_slice(b"\x1b]133;P\x07\x1b]133;C\x07").unwrap();
        assert_eq!(
            at_zero.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::None
        );

        let mut after_prompt = Terminal::init(8, 3, None).unwrap();
        after_prompt
            .next_slice(b"\x1b]133;P\x07$\x1b]133;C\x07")
            .unwrap();
        assert_eq!(
            after_prompt.active_row_semantic_prompt_for_tests(0),
            SemanticPrompt::Prompt
        );
    }

    #[test]
    fn terminal_stream_osc133_d_restores_output_semantic_content() {
        let mut terminal = Terminal::init(8, 3, None).unwrap();

        terminal
            .next_slice(b"\x1b]133;A\x07> \x1b]133;B\x07cmd\x1b]133;D\x07out")
            .unwrap();

        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(2, 0),
            SemanticContent::Input
        );
        assert_eq!(
            terminal.active_cell_semantic_content_for_tests(5, 0),
            SemanticContent::Output
        );
    }

    #[test]
    fn terminal_stream_notifications_are_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]9;Hello\x07\x1b]777;notify;Title;Body\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_clipboard_protocols_are_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(b"\x1b]52;s;?\x07\x1b]5522;type=read;payload\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_context_signals_are_ignored() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal
            .next_slice(b"\x1b]0;title\x07\x1b]7;file://host/home\x07\x1b]8;id=x;https://e\x07")
            .unwrap();
        terminal.next_slice(b"\x1b]10;#112233\x07").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
        let foreground = terminal.colors.foreground.get();
        terminal.clear_dirty_for_tests();

        terminal
            .next_slice(
                b"\x1b]3008;start=myctx;type=shell\x07\x1b]3008;end=myctx;exit=success\x1b\\",
            )
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "abc");
        assert_eq!(terminal.title_for_tests(), "title");
        assert_eq!(terminal.pwd_for_tests(), Some("file://host/home"));
        assert_eq!(
            terminal.cursor_hyperlink_for_tests(),
            Some((
                ScreenCursorHyperlinkId::Explicit("x".to_string()),
                "https://e"
            ))
        );
        assert_eq!(terminal.cursor_position_for_tests(), (5, 1));
        assert!(terminal.get_mode_for_tests(Mode::Insert));
        assert_eq!(terminal.colors.foreground.get(), foreground);
        assert!(terminal.pty_response_for_tests().is_empty());
        assert!(!terminal.is_dirty_for_tests(0, 0));
        assert!(!terminal.is_dirty_for_tests(9, 0));
    }

    #[test]
    fn terminal_stream_osc_actions_do_not_mutate_display_or_responses() {
        let mut terminal = Terminal::init(10, 3, None).unwrap();

        terminal.next_slice(b"abc").unwrap();
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 1);
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
    fn terminal_stream_osc_writes_page_hyperlink_metadata() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\AB\x1b]8;;\x1b\\")
            .unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(terminal.active_cell_hyperlink_for_tests(1, 0));
        let first = terminal
            .active_cell_hyperlink_snapshot_for_tests(0, 0)
            .unwrap();
        let second = terminal
            .active_cell_hyperlink_snapshot_for_tests(1, 0)
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(first.id, HyperlinkSnapshotId::Implicit(0));
        assert_eq!(first.uri, b"https://e");
        assert_eq!(terminal.active_cell_hyperlink_ref_count_for_tests(0, 0), 2);
        assert!(terminal.active_row_hyperlink_for_tests(0));
        assert_eq!(terminal.cursor_hyperlink_for_tests(), None);
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc_after_end_clears_destination_hyperlink() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\AB\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"Z").unwrap();

        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(!terminal.active_cell_hyperlink_for_tests(1, 0));
        assert_eq!(terminal.active_cell_hyperlink_ref_count_for_tests(0, 0), 1);
        assert!(terminal.active_row_hyperlink_for_tests(0));

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"Y").unwrap();
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        assert!(!terminal.active_row_hyperlink_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc_stores_explicit_ids_exactly() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;id=x&<y;https://example.com?a=1&b=<2>\x1b\\A")
            .unwrap();

        let link = terminal
            .active_cell_hyperlink_snapshot_for_tests(0, 0)
            .unwrap();
        assert_eq!(link.id, HyperlinkSnapshotId::Explicit(b"x&<y".to_vec()));
        assert_eq!(link.uri, b"https://example.com?a=1&b=<2>");
    }

    #[test]
    fn terminal_stream_osc_separate_implicit_ranges_get_distinct_ids() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://one\x1b\\A\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .next_slice(b"\x1b]8;;https://two\x1b\\B\x1b]8;;\x1b\\")
            .unwrap();

        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .id,
            HyperlinkSnapshotId::Implicit(0)
        );
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(1, 0)
                .unwrap()
                .id,
            HyperlinkSnapshotId::Implicit(1)
        );
    }

    #[test]
    fn terminal_stream_osc_and_sgr_compose_on_printed_cells() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]8;;https://e\x1b\\\x1b[1;31mA")
            .unwrap();

        assert!(terminal.active_cell_hyperlink_for_tests(0, 0));
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            }
        );
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_pending_wrap_overwrites_hyperlink_destination() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://old\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hello").unwrap();
        terminal.next_slice(b"\x1b]8;;https://new\x1b\\w").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 1)
                .unwrap()
                .uri,
            b"https://new"
        );
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_pending_wrap_without_active_link_clears_hyperlink_destination() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://old\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"hellow").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "hello\nw");
        assert!(!terminal.active_cell_hyperlink_for_tests(0, 1));
        assert!(!terminal.active_row_hyperlink_for_tests(1));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_insert_mode_shifts_existing_hyperlinks() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"A\x1b]8;;https://old\x1b\\B\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.set_mode_for_tests(Mode::Insert, true);
        terminal.next_slice(b"\x1b]8;;https://new\x1b\\Z").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ZAB");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .uri,
            b"https://new"
        );
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(2, 0)
                .unwrap()
                .uri,
            b"https://old"
        );
        assert!(!terminal.active_cell_hyperlink_for_tests(1, 0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_scroll_up_preserves_printed_hyperlinks() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal
            .next_slice(b"\x1b]8;;https://scroll\x1b\\X\x1b]8;;\x1b\\")
            .unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
        terminal.next_slice(b"\x1b[S").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "X");
        assert_eq!(
            terminal
                .active_cell_hyperlink_snapshot_for_tests(0, 0)
                .unwrap()
                .uri,
            b"https://scroll"
        );
        assert!(terminal.active_row_hyperlink_for_tests(0));
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc4_mutates_palette_entries() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]4;1;rgb:ff/00/80;2;#00ff00\x1b\\")
            .unwrap();

        assert_eq!(terminal.colors.palette[1], color::Rgb::new(255, 0, 128));
        assert_eq!(terminal.colors.palette[2], color::Rgb::new(0, 255, 0));
        assert_eq!(terminal.pty_response_for_tests(), b"");
        terminal.verify_integrity_for_tests();
    }

    #[test]
    fn terminal_stream_osc4_applies_repeated_palette_index_in_order() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]4;1;#ff0000;1;#0000ff\x1b\\")
            .unwrap();

        assert_eq!(terminal.colors.palette[1], color::Rgb::new(0, 0, 255));
    }

    #[test]
    fn terminal_stream_osc4_query_reports_palette_with_bel_terminator() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette[3] = color::Rgb::new(1, 0x80, 0xff);

        terminal.next_slice(b"\x1b]4;3;?\x07").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]4;3;rgb:0101/8080/ffff\x07"
        );
    }

    #[test]
    fn terminal_stream_osc4_query_reports_palette_with_st_terminator() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette[4] = color::Rgb::new(0x12, 0x34, 0x56);

        terminal.next_slice(b"\x1b]4;4;?\x1b\\").unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]4;4;rgb:1212/3434/5656\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_osc104_resets_indexed_palette_entry() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette[1] = color::Rgb::new(255, 0, 0);
        terminal.colors.palette[2] = color::Rgb::new(0, 255, 0);

        terminal.next_slice(b"\x1b]104;1\x1b\\").unwrap();

        assert_eq!(terminal.colors.palette[1], color::DEFAULT_PALETTE[1]);
        assert_eq!(terminal.colors.palette[2], color::Rgb::new(0, 255, 0));
    }

    #[test]
    fn terminal_stream_osc104_resets_all_palette_entries() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal.colors.palette[1] = color::Rgb::new(255, 0, 0);
        terminal.colors.palette[2] = color::Rgb::new(0, 255, 0);

        terminal.next_slice(b"\x1b]104\x1b\\").unwrap();

        assert_eq!(terminal.colors.palette, color::DEFAULT_PALETTE);
    }

    #[test]
    fn terminal_stream_osc_dynamic_colors_set_without_changing_palette() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original_palette = terminal.colors.palette;

        terminal
            .next_slice(b"\x1b]10;#112233\x1b\\\x1b]11;#445566\x1b\\\x1b]12;#778899\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::Rgb::new(0x11, 0x22, 0x33))
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::Rgb::new(0x44, 0x55, 0x66))
        );
        assert_eq!(
            terminal.colors.cursor.get(),
            Some(color::Rgb::new(0x77, 0x88, 0x99))
        );
        assert_eq!(terminal.colors.palette, original_palette);
    }

    #[test]
    fn terminal_stream_osc_dynamic_sequence_executes_in_order() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#010203;#040506;?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::Rgb::new(1, 2, 3))
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::Rgb::new(4, 5, 6))
        );
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:0101/0202/0303\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_osc_dynamic_resets_restore_defaults() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#010203\x1b\\\x1b]11;#040506\x1b\\\x1b]12;#070809\x1b\\")
            .unwrap();
        terminal
            .next_slice(b"\x1b]110\x1b\\\x1b]111;\x1b\\\x1b]112\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::DEFAULT_PALETTE[7])
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::DEFAULT_PALETTE[0])
        );
        assert_eq!(terminal.colors.cursor.get(), None);
    }

    #[test]
    fn terminal_stream_osc12_query_falls_back_to_foreground_when_cursor_unset() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]10;#123456\x1b\\\x1b]12;?\x07")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:1212/3434/5656\x07"
        );
    }

    #[test]
    fn terminal_stream_osc12_query_reports_cursor_override() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]12;#abcdef\x1b\\\x1b]12;?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]12;rgb:abab/cdcd/efef\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_palette_set_reset_and_query() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;2=#010203;2=?;2=\x1b\\")
            .unwrap();

        assert_eq!(terminal.colors.palette[2], color::DEFAULT_PALETTE[2]);
        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;2=rgb:01/02/03\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_dynamic_set_reset_and_query() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(
                b"\x1b]21;foreground=#112233;background=#445566;cursor=#778899;foreground=?;background=?;cursor=?\x07",
            )
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=rgb:11/22/33;background=rgb:44/55/66;cursor=rgb:77/88/99\x07"
        );

        terminal.take_pty_response_for_tests();
        terminal
            .next_slice(
                b"\x1b]21;foreground=;background=;cursor=;foreground=?;background=?;cursor=?\x1b\\",
            )
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=rgb:c5/c8/c6;background=rgb:1d/1f/21;cursor=\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_cursor_query_has_no_foreground_fallback() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;foreground=#123456;cursor=?\x1b\\")
            .unwrap();

        assert_eq!(terminal.pty_response_for_tests(), b"\x1b]21;cursor=\x1b\\");
    }

    #[test]
    fn terminal_stream_kitty_osc21_mixed_order_uses_current_state() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal
            .next_slice(b"\x1b]21;foreground=?;foreground=#010203;foreground=?\x1b\\")
            .unwrap();

        assert_eq!(
            terminal.pty_response_for_tests(),
            b"\x1b]21;foreground=rgb:c5/c8/c6;foreground=rgb:01/02/03\x1b\\"
        );
    }

    #[test]
    fn terminal_stream_kitty_osc21_unsupported_specials_are_inert() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original_palette = terminal.colors.palette;

        terminal
            .next_slice(
                b"\x1b]21;selection_foreground=#010203;cursor_text=;selection_background=?\x1b\\",
            )
            .unwrap();

        assert_eq!(terminal.colors.palette, original_palette);
        assert_eq!(
            terminal.colors.foreground.get(),
            Some(color::DEFAULT_PALETTE[7])
        );
        assert_eq!(
            terminal.colors.background.get(),
            Some(color::DEFAULT_PALETTE[0])
        );
        assert_eq!(terminal.colors.cursor.get(), None);
        assert_eq!(terminal.pty_response_for_tests(), b"\x1b]21\x1b\\");
    }

    #[test]
    fn terminal_stream_unsupported_color_osc_does_not_mutate_palette() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        let original = terminal.colors.palette;

        for input in [
            b"\x1b]5;0;#ff0000\x1b\\".as_slice(),
            b"\x1b]13;#ff0000\x1b\\".as_slice(),
            b"\x1b]19;?\x1b\\".as_slice(),
            b"\x1b]113\x1b\\".as_slice(),
            b"\x1b]119\x1b\\".as_slice(),
        ] {
            terminal.next_slice(input).unwrap();
        }

        assert_eq!(terminal.colors.palette, original);
    }

    #[test]
    fn terminal_stream_osc4_palette_change_affects_formatter_palette_output() {
        let mut terminal = terminal_with_lines(&["X"]);

        terminal.next_slice(b"\x1b]4;1;#123456\x1b\\").unwrap();
        let output = formatter(&terminal, PageOutputFormat::Html)
            .with_extra(TerminalFormatterExtra::none().palette(true))
            .format();

        assert!(output.contains("--vt-palette-1: #123456;"));
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[4h\x1b[31mB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC");
        assert_eq!(
            terminal.active_cell_style_for_tests(1, 0),
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            }
        );

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
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

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"\x1b[32mG").unwrap();
        assert_eq!(
            terminal.active_cell_style_for_tests(0, 1),
            style::Style {
                fg_color: style::Color::Palette(2),
                ..style::Style::default()
            }
        );

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
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
    fn terminal_stream_reverse_index_moves_cursor_up_outside_top_margin_case() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_reverse_index_clamps_at_top_when_not_in_scrolling_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();

        terminal.set_scrolling_region_for_tests(1, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
        assert_eq!(plain_with_unwrap(&terminal, false), "");
    }

    #[test]
    fn terminal_stream_reverse_index_scrolls_down_at_top_margin_inside_horizontal_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
        terminal.clear_dirty_for_tests();

        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AAAAA\nB   B\nCBBBC\nDDDDD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
        assert!(terminal.is_dirty_for_tests(1, 1));
        assert!(terminal.is_dirty_for_tests(3, 2));
        assert!(!terminal.is_dirty_for_tests(0, 0));
    }

    #[test]
    fn terminal_stream_reverse_index_does_not_scroll_top_margin_outside_horizontal_region() {
        let mut terminal = Terminal::init(5, 4, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);

        terminal.next_slice(b"\x1bM").unwrap();

        assert_eq!(
            plain_with_unwrap(&terminal, false),
            "AAAAA\nBBBBB\nCCCCC\nDDDDD"
        );
        assert_eq!(terminal.cursor_position_for_tests(), (4, 0));
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
                .active_mut()
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
                .active_mut()
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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(2, 2);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

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
                .active_mut()
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
                .active_mut()
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
                .active_mut()
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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2E").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 2));
    }

    #[test]
    fn terminal_stream_split_feed_csi_previous_line_moves_cursor() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 2);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2F").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_absolute_moves_to_default_column() {
        for input in [b"\x1b[G".as_slice(), b"\x1b[`".as_slice()] {
            let mut terminal = Terminal::init(5, 3, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(2, 1);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(4, 1);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 1);

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
                .active_mut()
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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 1);

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
                .active_mut()
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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(4, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 2);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(3, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(8, 0);

        terminal.next_slice(b"\x1b[Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_zero_count_does_not_move() {
        for input in [b"\x1b[0Z".as_slice(), b"\x1b[;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(16, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (16, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_count_moves_multiple_tabstops() {
        for input in [b"\x1b[2Z".as_slice(), b"\x1b[2;Z".as_slice()] {
            let mut terminal = Terminal::init(20, 2, None).unwrap();
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(19, 0);

            terminal.next_slice(input).unwrap();

            assert_eq!(terminal.cursor_position_for_tests(), (8, 0));
        }
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_large_count_clamps_to_left_edge() {
        let mut terminal = Terminal::init(20, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(10, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_non_origin_ignores_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_scrolling_region_for_tests(0, 2, 10, 19);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(16, 0);

        terminal.next_slice(b"\x1b[2Z").unwrap();

        assert_eq!(terminal.cursor_position_for_tests(), (0, 0));
    }

    #[test]
    fn terminal_stream_csi_horizontal_tab_back_origin_clamps_to_left_margin() {
        let mut terminal = Terminal::init(20, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Origin, true);
        terminal.set_scrolling_region_for_tests(0, 2, 5, 19);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(16, 0);

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
                .active_mut()
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(19, 0);

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
            terminal
                .screens
                .active_mut()
                .set_cursor_position_for_tests(16, 0);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(2, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[J").unwrap();

        assert!(!terminal.row_wrap_for_tests(2));
        assert!(!terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_display_protected_cells_survive() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\n   IJ\nKLMNO");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_clears_cursor_row_only() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[1K").unwrap();

        assert!(terminal.row_wrap_for_tests(1));
        assert!(terminal.row_wrap_continuation_for_tests(2));
    }

    #[test]
    fn terminal_stream_csi_erase_line_complete_preserves_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ"]);
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_insert_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC 13");
    }

    #[test]
    fn terminal_stream_csi_insert_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");
        assert!(terminal.cell_protected_for_tests(3, 0));
        assert!(!terminal.cell_protected_for_tests(2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_count_one_clears_without_shifting() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[0X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A CDE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_clamps_to_screen_edge() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

        terminal.next_slice(b"\x1b[99X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AB");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_ignores_horizontal_margins() {
        let mut terminal = Terminal::init(6, 2, None).unwrap();

        terminal.next_slice(b"ABCDEF").unwrap();
        terminal.set_scrolling_region_for_tests(0, 1, 2, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A  DEF");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_erase_chars_resets_wrap_metadata() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"@").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A BCD");

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 0);
        terminal.next_slice(b"\x1b[2").unwrap();
        terminal.next_slice(b"X").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A   D");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_count_one_shifts_suffix_left() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

        terminal.next_slice(b"\x1b[2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_csi_delete_chars_clamps_to_remaining_margin() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_row_wrap_for_tests(0, true);
        terminal.set_row_wrap_continuation_for_tests(1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABC2 3");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_ignores_vertical_margins() {
        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(terminal.scrollback_rows_for_tests(), scrollback_before);

        let mut terminal = terminal_with_lines(&["ABCDE", "FGHIJ", "KLMNO"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ABCDE\nGHIJ\nKLMNO");
    }

    #[test]
    fn terminal_stream_csi_delete_chars_moves_protected_bit_with_shifted_cell() {
        let mut terminal = Terminal::init(5, 2, None).unwrap();

        terminal.next_slice(b"ABCDE").unwrap();
        terminal.set_cell_protected_for_tests(2, 0, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 0);
        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"2P").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "ADE");
    }

    #[test]
    fn terminal_stream_csi_insert_lines_count_one_shifts_rows_down() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_insert_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[L").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"L").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_delete_lines_count_one_shifts_rows_up() {
        let mut terminal = Terminal::init(5, 5, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 2);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);

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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC\n\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (0, 1));
    }

    #[test]
    fn terminal_stream_csi_delete_lines_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[M").unwrap();

        assert!(!terminal.row_wrap_for_tests(1));
        assert!(!terminal.row_wrap_continuation_for_tests(2));

        let mut terminal = Terminal::init(5, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal.set_row_wrap_for_tests(1, true);
        terminal.set_row_wrap_continuation_for_tests(2, true);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(1, 1);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);

        terminal.next_slice(b"\x1b[").unwrap();
        terminal.next_slice(b"M").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\nCCCCC");
    }

    #[test]
    fn terminal_stream_csi_scroll_up_creates_scrollback() {
        let mut terminal = Terminal::init(5, 5, Some(10)).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD", "EEEEE"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
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
            .active_mut()
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
            .active_mut()
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
            .active_mut()
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

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
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABC", "DEF", "GHI"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(2, 2);
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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(1, 2, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(3, 0);

        terminal.next_slice(b"\x1b[T").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "AAAAA\n\nBBBBB\nDDDDD");
        assert_eq!(terminal.cursor_position_for_tests(), (3, 0));
    }

    #[test]
    fn terminal_stream_csi_scroll_down_honors_left_right_region() {
        let mut terminal = Terminal::init(6, 3, None).unwrap();
        terminal
            .screens
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 2);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC", "DDDDD"]);
        terminal.set_scrolling_region_for_tests(2, 3, 0, 4);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 0);
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
            .active_mut()
            .set_text_lines_for_tests(&["ABC123", "DEF456", "GHI789"]);
        terminal.set_scrolling_region_for_tests(0, 2, 1, 3);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(5, 0);

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
            .active_mut()
            .set_text_lines_for_tests(&["AAAAA", "BBBBB", "CCCCC"]);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
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
            .active_mut()
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
            .active_mut()
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
        terminal.next_slice(&bytes[1..]).unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "é");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
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

        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 1);
        terminal.next_slice(b"x").unwrap();
        terminal.set_cell_protected_for_tests(0, 1, true);
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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
    fn terminal_stream_non_ascii_prints_as_single_cell() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();

        terminal.next_slice("é".as_bytes()).unwrap();

        assert_eq!(formatter(&terminal, PageOutputFormat::Plain).format(), "é");
        assert_eq!(terminal.cursor_position_for_tests(), (1, 0));
    }

    #[test]
    fn terminal_stream_managed_cell_overwrite_returns_private_error() {
        let mut terminal = Terminal::init(10, 2, None).unwrap();
        terminal.next_slice(b"x").unwrap();
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(0, 0);
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
            terminal.screens.active(),
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
            .active_mut()
            .set_styled_cell_for_tests(0, 0, 'X', styled);
        let mut palette = color::DEFAULT_PALETTE;
        palette[1] = color::Rgb::new(1, 2, 3);
        let options = TerminalFormatterOptions::new(PageOutputFormat::Html)
            .trim(false)
            .palette(Some(&palette));

        let terminal_output = TerminalFormatter::init(&terminal, options).format();
        let screen_output = ScreenFormatter::init(
            terminal.screens.active(),
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_style_for_tests(style::Style {
                flags: style::Flags {
                    bold: true,
                    ..style::Flags::default()
                },
                ..style::Style::default()
            });
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
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
        terminal
            .screens
            .active_mut()
            .set_cursor_position_for_tests(4, 2);
        terminal
            .screens
            .active_mut()
            .set_cursor_protected_for_tests(true);
        terminal
            .screens
            .active_mut()
            .set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        terminal
            .screens
            .active_mut()
            .set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        terminal
            .screens
            .active_mut()
            .set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        terminal
            .screens
            .active_mut()
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
