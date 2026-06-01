//! Terminal state.

use super::color;
use super::modes;
use super::page_list::{
    CodepointMapEntry, PageListAllocError, PageOutputFormat, PageStringWithPinMap,
};
use super::screen::{
    BasicPrintError, Screen, ScreenFormatter, ScreenFormatterContent, ScreenFormatterExtra,
    ScreenFormatterOptions,
};
use super::size::CellCountInt;
use super::stream::{self, Action, Handler};
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
    stream: stream::Stream,
    flags: TerminalFlags,
    pwd: TerminalPwd,
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
    tabstops: &'a mut tabstops::Tabstops,
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
            stream: stream::Stream::init(),
            flags: TerminalFlags::default(),
            pwd: TerminalPwd::default(),
        })
    }

    pub(super) fn next_slice(&mut self, input: &[u8]) -> Result<(), TerminalStreamError> {
        let Terminal {
            size,
            screens,
            tabstops,
            stream,
            ..
        } = self;
        let mut handler = TerminalStreamHandler {
            screen: &mut screens.active,
            size: *size,
            tabstops,
        };
        stream.next_slice(input, &mut handler)
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
    pub(super) fn row_wrap_for_tests(&self, y: u32) -> bool {
        self.screens.active.row_wrap_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn row_wrap_continuation_for_tests(&self, y: u32) -> bool {
        self.screens.active.row_wrap_continuation_for_tests(y)
    }

    #[cfg(test)]
    pub(super) fn full_screen_plain_for_tests(&self, unwrap: bool) -> String {
        self.screens.active.full_screen_plain_for_tests(unwrap)
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
            Action::HorizontalTab => {
                self.screen
                    .horizontal_tab_basic(self.size.cols, self.tabstops);
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
            Action::Index => self.line_feed(),
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
        }
    }
}

impl TerminalStreamHandler<'_> {
    fn print(&mut self, cp: char) -> Result<(), TerminalStreamError> {
        if !(cp.is_ascii() && !cp.is_ascii_control()) && cp != char::REPLACEMENT_CHARACTER {
            return Err(TerminalStreamError::UnsupportedCodepoint(cp));
        }

        self.screen
            .print_basic_cell(self.size.cols, self.size.rows, cp)
            .map_err(TerminalStreamError::from)
    }

    fn line_feed(&mut self) -> Result<(), TerminalStreamError> {
        self.screen
            .line_feed_basic(self.size.rows)
            .map_err(TerminalStreamError::from)
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

        terminal.next_slice(b"A\x0eB\x1bcC\x1b[ZD").unwrap();

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
    fn terminal_stream_vt_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0bB").unwrap();

        assert_eq!(plain_with_unwrap(&terminal, false), "A\n B");
        assert_eq!(terminal.cursor_position_for_tests(), (2, 1));
    }

    #[test]
    fn terminal_stream_ff_bypasses_linefeed_mode() {
        let mut terminal = Terminal::init(4, 3, None).unwrap();
        terminal.set_mode_for_tests(Mode::Linefeed, true);

        terminal.next_slice(b"A\x0cB").unwrap();

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

        terminal.next_slice(b"A\x1b[").unwrap();
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

        terminal.next_slice(b"hello").unwrap();
        terminal.screens.active.set_styled_cell_for_tests(
            0,
            1,
            'x',
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            },
        );
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
        terminal.screens.active.set_styled_cell_for_tests(
            0,
            0,
            'x',
            style::Style {
                fg_color: style::Color::Palette(1),
                ..style::Style::default()
            },
        );

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
