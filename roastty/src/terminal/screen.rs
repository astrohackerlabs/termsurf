//! Terminal screen state.

use super::charsets;
use super::color;
use super::kitty;
use super::page_list::{
    BasicCellWriteError, CodepointMapEntry, PageList, PageListAllocError, PageOutputFormat,
    PageStringWithPinMap,
};
use super::point;
use super::selection;
use super::size::CellCountInt;
use super::style;

#[derive(Debug)]
pub(super) struct Screen {
    cursor: ScreenCursor,
    charset: ScreenCharsetState,
    kitty_keyboard: kitty::KeyFlagStack,
    pages: PageList,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BasicPrintError {
    RightEdge,
    Cell(BasicCellWriteError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScreenCursor {
    x: CellCountInt,
    y: CellCountInt,
    style: style::Style,
    protected: bool,
    hyperlink: Option<ScreenCursorHyperlink>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScreenCursorHyperlink {
    id: ScreenCursorHyperlinkId,
    uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ScreenCursorHyperlinkId {
    Explicit(String),
    Implicit(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ScreenCharsetState {
    g0: charsets::Charset,
    g1: charsets::Charset,
    g2: charsets::Charset,
    g3: charsets::Charset,
    gl: charsets::CharsetSlot,
    gr: charsets::CharsetGrSlot,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum ScreenFormatterContent {
    None,
    Selection(Option<selection::Selection>),
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScreenFormatterOptions<'a> {
    emit: PageOutputFormat,
    trim: bool,
    unwrap: bool,
    palette: Option<&'a color::Palette>,
    codepoint_map: Option<&'a [CodepointMapEntry]>,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct ScreenFormatter<'a> {
    screen: &'a Screen,
    options: ScreenFormatterOptions<'a>,
    content: ScreenFormatterContent,
    extra: ScreenFormatterExtra,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct ScreenFormatterExtra {
    cursor: bool,
    style: bool,
    hyperlink: bool,
    protection: bool,
    kitty_keyboard: bool,
    charsets: bool,
}

impl Screen {
    pub(super) fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        Ok(Self {
            cursor: ScreenCursor::default(),
            charset: ScreenCharsetState::default(),
            kitty_keyboard: kitty::KeyFlagStack::default(),
            pages: PageList::init(cols, rows, max_scrollback_rows)?,
        })
    }

    pub(super) fn top_left_pin(&self) -> super::page_list::Pin {
        self.pages
            .pin(point::Point::screen(point::Coordinate::new(0, 0)))
            .expect("screen top-left pin must resolve")
    }

    pub(super) fn print_basic_cell(
        &mut self,
        cols: CellCountInt,
        codepoint: char,
    ) -> Result<(), BasicPrintError> {
        if self.cursor.x >= cols - 1 {
            return Err(BasicPrintError::RightEdge);
        }

        self.pages
            .write_basic_screen_cell(self.cursor.x, self.cursor.y.into(), codepoint)
            .map_err(BasicPrintError::Cell)?;
        self.cursor.x += 1;
        Ok(())
    }

    #[cfg(test)]
    pub(super) fn set_cursor_position_for_tests(&mut self, x: CellCountInt, y: CellCountInt) {
        self.cursor.x = x;
        self.cursor.y = y;
    }

    #[cfg(test)]
    pub(super) fn set_cursor_style_for_tests(&mut self, style: style::Style) {
        self.cursor.style = style;
    }

    #[cfg(test)]
    pub(super) fn set_cursor_protected_for_tests(&mut self, protected: bool) {
        self.cursor.protected = protected;
    }

    #[cfg(test)]
    pub(super) fn set_cursor_hyperlink_for_tests(
        &mut self,
        id: ScreenCursorHyperlinkId,
        uri: &str,
    ) {
        self.cursor.hyperlink = Some(ScreenCursorHyperlink {
            id,
            uri: uri.to_string(),
        });
    }

    #[cfg(test)]
    pub(super) fn clear_cursor_hyperlink_for_tests(&mut self) {
        self.cursor.hyperlink = None;
    }

    #[cfg(test)]
    pub(super) fn set_charset_for_tests(
        &mut self,
        slot: charsets::CharsetSlot,
        charset: charsets::Charset,
    ) {
        self.charset.set(slot, charset);
    }

    #[cfg(test)]
    pub(super) fn set_charset_gl_for_tests(&mut self, slot: charsets::CharsetSlot) {
        self.charset.gl = slot;
    }

    #[cfg(test)]
    pub(super) fn set_charset_gr_for_tests(&mut self, slot: charsets::CharsetGrSlot) {
        self.charset.gr = slot;
    }

    #[cfg(test)]
    pub(super) fn set_kitty_keyboard_for_tests(
        &mut self,
        mode: kitty::KeySetMode,
        flags: kitty::KeyFlags,
    ) {
        self.kitty_keyboard.set(mode, flags);
    }

    #[cfg(test)]
    pub(super) fn push_kitty_keyboard_for_tests(&mut self, flags: kitty::KeyFlags) {
        self.kitty_keyboard.push(flags);
    }

    #[cfg(test)]
    pub(super) fn pop_kitty_keyboard_for_tests(&mut self, n: usize) {
        self.kitty_keyboard.pop(n);
    }

    #[cfg(test)]
    pub(super) fn set_text_lines_for_tests(&mut self, lines: &[&str]) {
        self.pages.set_screen_text_lines_for_tests(lines);
    }

    #[cfg(test)]
    pub(super) fn set_styled_cell_for_tests(
        &mut self,
        x: CellCountInt,
        y: u32,
        codepoint: char,
        style: super::style::Style,
    ) {
        self.pages
            .set_screen_styled_cell_for_tests(x, y, codepoint, style);
    }

    #[cfg(test)]
    pub(super) fn pin_for_tests(&self, x: CellCountInt, y: u32) -> super::page_list::Pin {
        self.pages
            .pin(super::point::Point::screen(super::point::Coordinate::new(
                x, y,
            )))
            .expect("screen pin must resolve")
    }

    #[cfg(test)]
    pub(super) fn cursor_position_for_tests(&self) -> (CellCountInt, CellCountInt) {
        (self.cursor.x, self.cursor.y)
    }

    #[cfg(test)]
    pub(super) fn is_dirty_for_tests(&self, x: CellCountInt, y: u32) -> bool {
        self.pages
            .is_dirty_for_tests(point::Point::screen(point::Coordinate::new(x, y)))
    }
}

impl Default for ScreenCursor {
    fn default() -> Self {
        Self {
            x: 0,
            y: 0,
            style: style::Style::default(),
            protected: false,
            hyperlink: None,
        }
    }
}

impl ScreenCharsetState {
    const fn get(self, slot: charsets::CharsetSlot) -> charsets::Charset {
        match slot {
            charsets::CharsetSlot::G0 => self.g0,
            charsets::CharsetSlot::G1 => self.g1,
            charsets::CharsetSlot::G2 => self.g2,
            charsets::CharsetSlot::G3 => self.g3,
        }
    }

    #[cfg(test)]
    fn set(&mut self, slot: charsets::CharsetSlot, charset: charsets::Charset) {
        match slot {
            charsets::CharsetSlot::G0 => self.g0 = charset,
            charsets::CharsetSlot::G1 => self.g1 = charset,
            charsets::CharsetSlot::G2 => self.g2 = charset,
            charsets::CharsetSlot::G3 => self.g3 = charset,
        }
    }
}

impl Default for ScreenCharsetState {
    fn default() -> Self {
        Self {
            g0: charsets::Charset::Utf8,
            g1: charsets::Charset::Utf8,
            g2: charsets::Charset::Utf8,
            g3: charsets::Charset::Utf8,
            gl: charsets::CharsetSlot::G0,
            gr: charsets::CharsetGrSlot::G2,
        }
    }
}

impl ScreenFormatterExtra {
    pub(super) const fn none() -> Self {
        Self {
            cursor: false,
            style: false,
            hyperlink: false,
            protection: false,
            kitty_keyboard: false,
            charsets: false,
        }
    }

    pub(super) const fn cursor(mut self, cursor: bool) -> Self {
        self.cursor = cursor;
        self
    }

    pub(super) const fn style(mut self, style: bool) -> Self {
        self.style = style;
        self
    }

    pub(super) const fn hyperlink(mut self, hyperlink: bool) -> Self {
        self.hyperlink = hyperlink;
        self
    }

    pub(super) const fn protection(mut self, protection: bool) -> Self {
        self.protection = protection;
        self
    }

    pub(super) const fn kitty_keyboard(mut self, kitty_keyboard: bool) -> Self {
        self.kitty_keyboard = kitty_keyboard;
        self
    }

    pub(super) const fn charsets(mut self, charsets: bool) -> Self {
        self.charsets = charsets;
        self
    }

    const fn is_empty(self) -> bool {
        !self.cursor
            && !self.style
            && !self.hyperlink
            && !self.protection
            && !self.kitty_keyboard
            && !self.charsets
    }
}

impl<'a> ScreenFormatterOptions<'a> {
    pub(super) const fn new(emit: PageOutputFormat) -> Self {
        Self {
            emit,
            trim: true,
            unwrap: false,
            palette: None,
            codepoint_map: None,
        }
    }

    pub(super) const fn trim(mut self, trim: bool) -> Self {
        self.trim = trim;
        self
    }

    pub(super) const fn unwrap(mut self, unwrap: bool) -> Self {
        self.unwrap = unwrap;
        self
    }

    pub(super) const fn palette(mut self, palette: Option<&'a color::Palette>) -> Self {
        self.palette = palette;
        self
    }

    pub(super) const fn codepoint_map(
        mut self,
        codepoint_map: Option<&'a [CodepointMapEntry]>,
    ) -> Self {
        self.codepoint_map = codepoint_map;
        self
    }

    pub(super) const fn emit(&self) -> PageOutputFormat {
        self.emit
    }
}

impl<'a> ScreenFormatter<'a> {
    pub(super) fn init(screen: &'a Screen, options: ScreenFormatterOptions<'a>) -> Self {
        Self {
            screen,
            options,
            content: ScreenFormatterContent::Selection(None),
            extra: ScreenFormatterExtra::none(),
        }
    }

    pub(super) const fn with_content(mut self, content: ScreenFormatterContent) -> Self {
        self.content = content;
        self
    }

    pub(super) const fn with_extra(mut self, extra: ScreenFormatterExtra) -> Self {
        self.extra = extra;
        self
    }

    pub(super) fn format(self) -> String {
        let mut output = match self.content {
            ScreenFormatterContent::None => String::new(),
            ScreenFormatterContent::Selection(selection) => self.screen.pages.screen_format_string(
                selection,
                self.options.trim,
                self.options.unwrap,
                self.options.emit,
                self.options.palette,
                self.options.codepoint_map,
            ),
        };
        output.push_str(&self.extra_string());
        output
    }

    pub(super) fn format_with_pin_map(self) -> PageStringWithPinMap {
        let mut output = match self.content {
            ScreenFormatterContent::None => PageStringWithPinMap {
                text: String::new(),
                pin_map: Vec::new(),
            },
            ScreenFormatterContent::Selection(selection) => {
                self.screen.pages.screen_format_string_with_pin_map(
                    selection,
                    self.options.trim,
                    self.options.unwrap,
                    self.options.emit,
                    self.options.palette,
                    self.options.codepoint_map,
                )
            }
        };
        let extra = self.extra_string();
        if !extra.is_empty() {
            let extra_pin = output
                .pin_map
                .last()
                .copied()
                .unwrap_or_else(|| self.screen.top_left_pin());
            output
                .pin_map
                .extend(std::iter::repeat_n(extra_pin, extra.len()));
            output.text.push_str(&extra);
        }
        output
    }

    fn extra_string(self) -> String {
        if self.options.emit != PageOutputFormat::Vt || self.extra.is_empty() {
            return String::new();
        }

        let mut output = String::new();
        if self.extra.style {
            output.push_str(&self.screen.cursor.style.formatter_vt().to_string());
        }
        if self.extra.hyperlink {
            self.push_hyperlink_extra(&mut output);
        }
        if self.extra.protection && self.screen.cursor.protected {
            output.push_str("\x1b[1\"q");
        }
        if self.extra.kitty_keyboard {
            let flags = self.screen.kitty_keyboard.current();
            if !flags.is_disabled() {
                output.push_str(&format!("\x1b[={};1u", flags.int()));
            }
        }
        if self.extra.charsets {
            self.push_charset_extras(&mut output);
        }
        if self.extra.cursor {
            output.push_str(&format!(
                "\x1b[{};{}H",
                self.screen.cursor.y + 1,
                self.screen.cursor.x + 1
            ));
        }
        output
    }

    fn push_hyperlink_extra(self, output: &mut String) {
        let Some(link) = &self.screen.cursor.hyperlink else {
            return;
        };

        match &link.id {
            ScreenCursorHyperlinkId::Explicit(id) => {
                output.push_str("\x1b]8;id=");
                output.push_str(id);
                output.push(';');
                output.push_str(&link.uri);
                output.push_str("\x1b\\");
            }
            ScreenCursorHyperlinkId::Implicit(_) => {
                output.push_str("\x1b]8;;");
                output.push_str(&link.uri);
                output.push_str("\x1b\\");
            }
        }
    }

    fn push_charset_extras(self, output: &mut String) {
        for slot in [
            charsets::CharsetSlot::G0,
            charsets::CharsetSlot::G1,
            charsets::CharsetSlot::G2,
            charsets::CharsetSlot::G3,
        ] {
            let charset = self.screen.charset.get(slot);
            if let Some(final_byte) = charset.designation_final() {
                output.push('\x1b');
                output.push(char::from(slot.designation_intermediate()));
                output.push(char::from(final_byte));
            }
        }

        match self.screen.charset.gl {
            charsets::CharsetSlot::G0 => {}
            charsets::CharsetSlot::G1 => output.push('\x0e'),
            charsets::CharsetSlot::G2 => output.push_str("\x1bn"),
            charsets::CharsetSlot::G3 => output.push_str("\x1bo"),
        }

        if let Some(sequence) = self.screen.charset.gr.invocation_sequence() {
            output.push_str(sequence);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::kitty::{KeyFlags, KeySetMode};
    use crate::terminal::page_list::CodepointReplacement;
    use crate::terminal::page_list::Pin;
    use crate::terminal::point;

    fn screen_with_lines(lines: &[&str]) -> Screen {
        let rows = lines.len().max(1);
        let cols = lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(1)
            .max(1);
        let mut screen = Screen::init(cols.try_into().unwrap(), rows.try_into().unwrap(), None)
            .expect("test screen must initialize");
        screen.pages.set_screen_text_lines_for_tests(lines);
        screen
    }

    fn screen_pin(screen: &Screen, x: CellCountInt, y: u32) -> Pin {
        screen
            .pages
            .pin(point::Point::screen(point::Coordinate::new(x, y)))
            .expect("screen pin must resolve")
    }

    fn screen_selection(
        screen: &Screen,
        start: (CellCountInt, u32),
        end: (CellCountInt, u32),
    ) -> selection::Selection {
        selection::Selection::new(
            screen_pin(screen, start.0, start.1),
            screen_pin(screen, end.0, end.1),
            false,
        )
    }

    fn formatter<'a>(screen: &'a Screen, emit: PageOutputFormat) -> ScreenFormatter<'a> {
        ScreenFormatter::init(screen, ScreenFormatterOptions::new(emit).unwrap(true))
    }

    fn pins(screen: &Screen, points: &[(CellCountInt, u32)]) -> Vec<Pin> {
        points
            .iter()
            .map(|&(x, y)| screen_pin(screen, x, y))
            .collect()
    }

    const KITTY_FLAGS_3: KeyFlags = KeyFlags {
        disambiguate: true,
        report_events: true,
        ..KeyFlags::DISABLED
    };

    #[test]
    fn screen_formatter_plain_full_screen_single_line() {
        let screen = screen_with_lines(&["hello"]);

        assert_eq!(
            formatter(&screen, PageOutputFormat::Plain).format(),
            "hello"
        );
    }

    #[test]
    fn screen_formatter_plain_full_screen_multiline() {
        let screen = screen_with_lines(&["hello", "world"]);

        assert_eq!(
            formatter(&screen, PageOutputFormat::Plain).format(),
            "hello\nworld"
        );
    }

    #[test]
    fn screen_formatter_plain_selected_line() {
        let screen = screen_with_lines(&["line1", "line2", "line3"]);
        let selection = screen_selection(&screen, (0, 1), (4, 1));

        let actual = formatter(&screen, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format();

        assert_eq!(actual, "line2");
    }

    #[test]
    fn screen_formatter_no_content_emits_empty_output_and_pin_map() {
        let screen = screen_with_lines(&["hello"]);

        let formatter =
            formatter(&screen, PageOutputFormat::Plain).with_content(ScreenFormatterContent::None);

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
    fn screen_formatter_vt_content_delegates_to_page_list() {
        let screen = screen_with_lines(&["hello", "world"]);

        let screen_output = formatter(&screen, PageOutputFormat::Vt).format();
        let page_output =
            screen
                .pages
                .screen_format_string(None, true, true, PageOutputFormat::Vt, None, None);

        assert_eq!(screen_output, page_output);
        assert_eq!(screen_output, "hello\r\nworld");
    }

    #[test]
    fn screen_formatter_html_content_delegates_to_page_list() {
        let screen = screen_with_lines(&["<hi"]);

        let screen_output = formatter(&screen, PageOutputFormat::Html).format();
        let page_output =
            screen
                .pages
                .screen_format_string(None, true, true, PageOutputFormat::Html, None, None);

        assert_eq!(screen_output, page_output);
        assert_eq!(
            screen_output,
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;hi</div>"
        );
    }

    #[test]
    fn screen_formatter_plain_pin_map_single_line() {
        let screen = screen_with_lines(&["hello"]);

        let actual = formatter(&screen, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello");
        assert_eq!(
            actual.pin_map,
            pins(&screen, &[(0, 0), (1, 0), (2, 0), (3, 0), (4, 0)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn screen_formatter_plain_pin_map_multiline() {
        let screen = screen_with_lines(&["hello", "world"]);

        let actual = formatter(&screen, PageOutputFormat::Plain).format_with_pin_map();

        assert_eq!(actual.text, "hello\nworld");
        assert_eq!(
            actual.pin_map,
            pins(
                &screen,
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
    fn screen_formatter_selected_plain_pin_map() {
        let screen = screen_with_lines(&["line1", "line2", "line3"]);
        let selection = screen_selection(&screen, (0, 1), (4, 1));

        let actual = formatter(&screen, PageOutputFormat::Plain)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .format_with_pin_map();

        assert_eq!(actual.text, "line2");
        assert_eq!(
            actual.pin_map,
            pins(&screen, &[(0, 1), (1, 1), (2, 1), (3, 1), (4, 1)])
        );
        assert_eq!(actual.text.len(), actual.pin_map.len());
    }

    #[test]
    fn screen_formatter_codepoint_map_delegates_output_and_pin_map() {
        let screen = screen_with_lines(&["ao"]);
        let map = [CodepointMapEntry::new(
            'o' as u32,
            'o' as u32,
            CodepointReplacement::String("<é".to_string()),
        )
        .unwrap()];
        let options = ScreenFormatterOptions::new(PageOutputFormat::Html).codepoint_map(Some(&map));

        let screen_output = ScreenFormatter::init(&screen, options).format_with_pin_map();
        let page_output = screen.pages.screen_format_string_with_pin_map(
            None,
            true,
            false,
            PageOutputFormat::Html,
            None,
            Some(&map),
        );

        assert_eq!(screen_output, page_output);
        assert_eq!(
            screen_output.text,
            "<div style=\"font-family: monospace; white-space: pre;\">a&lt;&#233;</div>"
        );
        assert_eq!(screen_output.text.len(), screen_output.pin_map.len());
    }

    #[test]
    fn screen_formatter_vt_and_html_pin_maps_delegate_to_page_list() {
        let screen = screen_with_lines(&["<é"]);

        for emit in [PageOutputFormat::Vt, PageOutputFormat::Html] {
            let screen_output = formatter(&screen, emit).format_with_pin_map();
            let page_output = screen
                .pages
                .screen_format_string_with_pin_map(None, true, true, emit, None, None);

            assert_eq!(screen_output, page_output);
            assert_eq!(screen_output.text.len(), screen_output.pin_map.len());
        }
    }

    #[test]
    fn screen_formatter_vt_cursor_extra_appends_cup_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(4, 2);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().cursor(true))
            .format();

        assert_eq!(actual, "hi\x1b[3;5H");
    }

    #[test]
    fn screen_formatter_vt_style_extra_appends_active_sgr_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_style_for_tests(style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        });

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().style(true))
            .format();

        assert_eq!(actual, "hi\x1b[0m\x1b[1m");
    }

    #[test]
    fn screen_formatter_vt_style_and_cursor_extras_keep_upstream_order() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(4, 2);
        screen.set_cursor_style_for_tests(style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        });

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().style(true).cursor(true))
            .format();

        assert_eq!(actual, "hi\x1b[0m\x1b[38;5;1m\x1b[3;5H");
    }

    #[test]
    fn screen_formatter_vt_protection_extra_appends_decsca_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_protected_for_tests(true);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().protection(true))
            .format();

        assert_eq!(actual, "hi\x1b[1\"q");
    }

    #[test]
    fn screen_formatter_vt_protection_extra_ignores_unprotected_cursor() {
        let screen = screen_with_lines(&["hi"]);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().protection(true))
            .format();

        assert_eq!(actual, "hi");
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_ignores_disabled_state() {
        let screen = screen_with_lines(&["hi"]);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format();

        assert_eq!(actual, "hi");
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_appends_csi_equal_u_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format();

        assert_eq!(actual, "hi\x1b[=3;1u");
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_combines_flag_bits() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_kitty_keyboard_for_tests(
            KeySetMode::Set,
            KeyFlags {
                report_events: true,
                report_all: true,
                report_associated: true,
                ..KeyFlags::DISABLED
            },
        );

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format();

        assert_eq!(actual, "hi\x1b[=26;1u");
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_ignores_absent_state() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(42), "http://e");
        screen.clear_cursor_hyperlink_for_tests();

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format();

        assert_eq!(actual, "hi");
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_appends_implicit_osc8_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(42), "http://e");

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format();

        assert_eq!(actual, "hi\x1b]8;;http://e\x1b\\");
        assert!(!actual.contains("42"));
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_appends_explicit_osc8_after_content() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(
            ScreenCursorHyperlinkId::Explicit("tab-1".to_string()),
            "http://e",
        );

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format();

        assert_eq!(actual, "hi\x1b]8;id=tab-1;http://e\x1b\\");
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_emits_raw_osc8_payload() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(
            ScreenCursorHyperlinkId::Explicit("x&<y".to_string()),
            "https://example.com?a=1&b=<2>",
        );

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format();

        assert_eq!(
            actual,
            "hi\x1b]8;id=x&<y;https://example.com?a=1&b=<2>\x1b\\"
        );
    }

    #[test]
    fn screen_kitty_keyboard_helpers_preserve_stack_behavior() {
        let mut screen = screen_with_lines(&["hi"]);

        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        screen.push_kitty_keyboard_for_tests(KeyFlags {
            report_all: true,
            ..KeyFlags::DISABLED
        });
        screen.pop_kitty_keyboard_for_tests(1);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format();

        assert_eq!(actual, "hi\x1b[=3;1u");
    }

    #[test]
    fn screen_formatter_vt_style_protection_and_cursor_extras_keep_upstream_order() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(4, 2);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");
        screen.set_cursor_protected_for_tests(true);
        screen.set_cursor_style_for_tests(style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        });

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(
                ScreenFormatterExtra::none()
                    .style(true)
                    .hyperlink(true)
                    .protection(true)
                    .cursor(true),
            )
            .format();

        assert_eq!(
            actual,
            "hi\x1b[0m\x1b[38;5;1m\x1b]8;;http://e\x1b\\\x1b[1\"q\x1b[3;5H"
        );
    }

    #[test]
    fn screen_formatter_vt_default_charset_extra_emits_nothing() {
        let screen = screen_with_lines(&["hi"]);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format();

        assert_eq!(actual, "hi");
    }

    #[test]
    fn screen_formatter_vt_charset_designations_emit_upstream_sequences() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::Ascii);
        screen.set_charset_for_tests(charsets::CharsetSlot::G1, charsets::Charset::British);
        screen.set_charset_for_tests(charsets::CharsetSlot::G2, charsets::Charset::DecSpecial);
        screen.set_charset_for_tests(charsets::CharsetSlot::G3, charsets::Charset::Ascii);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format();

        assert_eq!(actual, "hi\x1b(B\x1b)A\x1b*0\x1b+B");
    }

    #[test]
    fn screen_formatter_vt_charset_gl_invocations_emit_upstream_sequences() {
        for (slot, expected) in [
            (charsets::CharsetSlot::G1, "hi\x0e"),
            (charsets::CharsetSlot::G2, "hi\x1bn"),
            (charsets::CharsetSlot::G3, "hi\x1bo"),
        ] {
            let mut screen = screen_with_lines(&["hi"]);
            screen.set_charset_gl_for_tests(slot);

            let actual = formatter(&screen, PageOutputFormat::Vt)
                .with_extra(ScreenFormatterExtra::none().charsets(true))
                .format();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn screen_formatter_vt_charset_gr_invocations_emit_upstream_sequences() {
        for (slot, expected) in [
            (charsets::CharsetGrSlot::G1, "hi\x1b~"),
            (charsets::CharsetGrSlot::G3, "hi\x1b|"),
        ] {
            let mut screen = screen_with_lines(&["hi"]);
            screen.set_charset_gr_for_tests(slot);

            let actual = formatter(&screen, PageOutputFormat::Vt)
                .with_extra(ScreenFormatterExtra::none().charsets(true))
                .format();

            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn screen_formatter_vt_style_protection_charset_and_cursor_extras_keep_upstream_order() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(4, 2);
        screen.set_cursor_protected_for_tests(true);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        screen.set_cursor_style_for_tests(style::Style {
            fg_color: style::Color::Palette(1),
            ..style::Style::default()
        });
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        screen.set_charset_gl_for_tests(charsets::CharsetSlot::G1);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(
                ScreenFormatterExtra::none()
                    .style(true)
                    .hyperlink(true)
                    .protection(true)
                    .kitty_keyboard(true)
                    .charsets(true)
                    .cursor(true),
            )
            .format();

        assert_eq!(
            actual,
            "hi\x1b[0m\x1b[38;5;1m\x1b]8;;http://e\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x0e\x1b[3;5H"
        );
    }

    #[test]
    fn screen_formatter_plain_and_html_ignore_cursor_and_style_extras() {
        let mut screen = screen_with_lines(&["<hi"]);
        screen.set_cursor_position_for_tests(4, 2);
        screen.set_cursor_protected_for_tests(true);
        screen.set_cursor_hyperlink_for_tests(
            ScreenCursorHyperlinkId::Explicit("x&<y".to_string()),
            "https://example.com?a=1&b=<2>",
        );
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        screen.set_charset_gl_for_tests(charsets::CharsetSlot::G1);
        screen.set_cursor_style_for_tests(style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        });
        let extra = ScreenFormatterExtra::none()
            .style(true)
            .hyperlink(true)
            .protection(true)
            .kitty_keyboard(true)
            .charsets(true)
            .cursor(true);

        assert_eq!(
            formatter(&screen, PageOutputFormat::Plain)
                .with_extra(extra)
                .format(),
            "<hi"
        );
        assert_eq!(
            formatter(&screen, PageOutputFormat::Html)
                .with_extra(extra)
                .format(),
            "<div style=\"font-family: monospace; white-space: pre;\">&lt;hi</div>"
        );
    }

    #[test]
    fn screen_formatter_no_content_can_emit_vt_extras() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(2, 1);
        screen.set_cursor_protected_for_tests(true);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        screen.set_cursor_style_for_tests(style::Style {
            flags: style::Flags {
                bold: true,
                ..style::Flags::default()
            },
            ..style::Style::default()
        });

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(
                ScreenFormatterExtra::none()
                    .style(true)
                    .hyperlink(true)
                    .protection(true)
                    .kitty_keyboard(true)
                    .charsets(true)
                    .cursor(true),
            )
            .format();

        assert_eq!(
            actual,
            "\x1b[0m\x1b[1m\x1b]8;;http://e\x1b\\\x1b[1\"q\x1b[=3;1u\x1b(0\x1b[2;3H"
        );
    }

    #[test]
    fn screen_formatter_no_content_can_emit_only_kitty_keyboard_extra() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format();

        assert_eq!(actual, "\x1b[=3;1u");
    }

    #[test]
    fn screen_formatter_no_content_can_emit_only_hyperlink_extra() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format();

        assert_eq!(actual, "\x1b]8;;http://e\x1b\\");
    }

    #[test]
    fn screen_formatter_vt_extra_pin_map_uses_last_content_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(2, 1);
        screen.set_cursor_protected_for_tests(true);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b(0");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert_eq!(actual.pin_map[0], screen_pin(&screen, 0, 0));
        assert_eq!(actual.pin_map[1], screen_pin(&screen, 1, 0));
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_cursor_extra_pin_map_uses_last_content_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(2, 1);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().cursor(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b[2;3H");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert_eq!(actual.pin_map[0], screen_pin(&screen, 0, 0));
        assert_eq!(actual.pin_map[1], screen_pin(&screen, 1, 0));
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_protection_extra_pin_map_uses_last_content_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_protected_for_tests(true);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().protection(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b[1\"q");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert_eq!(actual.pin_map[0], screen_pin(&screen, 0, 0));
        assert_eq!(actual.pin_map[1], screen_pin(&screen, 1, 0));
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_pin_map_uses_last_content_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b]8;;http://e\x1b\\");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert_eq!(actual.pin_map[0], screen_pin(&screen, 0, 0));
        assert_eq!(actual.pin_map[1], screen_pin(&screen, 1, 0));
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_pin_map_is_byte_indexed_for_multibyte_uri() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(
            ScreenCursorHyperlinkId::Explicit("idé".to_string()),
            "https://e.test/é",
        );
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b]8;id=idé;https://e.test/é\x1b\\");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert!(actual.text.chars().count() < actual.text.len());
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_pin_map_uses_last_content_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "hi\x1b[=3;1u");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        assert_eq!(actual.pin_map[0], screen_pin(&screen, 0, 0));
        assert_eq!(actual.pin_map[1], screen_pin(&screen, 1, 0));
        for pin in &actual.pin_map[2..] {
            assert_eq!(*pin, screen_pin(&screen, 1, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_extra_pin_map_without_content_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(2, 1);
        screen.set_cursor_protected_for_tests(true);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b(0");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_pin_map_without_content_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b[=3;1u");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_pin_map_without_content_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b]8;;http://e\x1b\\");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_cursor_extra_pin_map_without_content_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        screen.set_cursor_position_for_tests(2, 1);
        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::None)
            .with_extra(ScreenFormatterExtra::none().cursor(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b[2;3H");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_extra_pin_map_after_invalid_selection_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        let other = screen_with_lines(&["other"]);
        let invalid = screen_pin(&other, 0, 0);
        let valid = screen_pin(&screen, 0, 0);
        screen.set_cursor_position_for_tests(2, 1);
        screen.set_cursor_protected_for_tests(true);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(
                selection::Selection::new(invalid, valid, false),
            )))
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b(0");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_pin_map_after_invalid_selection_uses_top_left_pin()
    {
        let mut screen = screen_with_lines(&["hi"]);
        let other = screen_with_lines(&["other"]);
        let invalid = screen_pin(&other, 0, 0);
        let valid = screen_pin(&screen, 0, 0);
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(
                selection::Selection::new(invalid, valid, false),
            )))
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b[=3;1u");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_pin_map_after_invalid_selection_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["hi"]);
        let other = screen_with_lines(&["other"]);
        let invalid = screen_pin(&other, 0, 0);
        let valid = screen_pin(&screen, 0, 0);
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(
                selection::Selection::new(invalid, valid, false),
            )))
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b]8;;http://e\x1b\\");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_extra_pin_map_after_empty_selection_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["  "]);
        let selection = screen_selection(&screen, (0, 0), (1, 0));
        screen.set_cursor_protected_for_tests(true);
        screen.set_charset_for_tests(charsets::CharsetSlot::G0, charsets::Charset::DecSpecial);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(ScreenFormatterExtra::none().charsets(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b(0");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_hyperlink_extra_pin_map_after_empty_selection_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["  "]);
        let selection = screen_selection(&screen, (0, 0), (1, 0));
        screen.set_cursor_hyperlink_for_tests(ScreenCursorHyperlinkId::Implicit(1), "http://e");

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(ScreenFormatterExtra::none().hyperlink(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b]8;;http://e\x1b\\");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_vt_kitty_keyboard_extra_pin_map_after_empty_selection_uses_top_left_pin() {
        let mut screen = screen_with_lines(&["  "]);
        let selection = screen_selection(&screen, (0, 0), (1, 0));
        screen.set_kitty_keyboard_for_tests(KeySetMode::Set, KITTY_FLAGS_3);

        let actual = formatter(&screen, PageOutputFormat::Vt)
            .with_content(ScreenFormatterContent::Selection(Some(selection)))
            .with_extra(ScreenFormatterExtra::none().kitty_keyboard(true))
            .format_with_pin_map();

        assert_eq!(actual.text, "\x1b[=3;1u");
        assert_eq!(actual.text.len(), actual.pin_map.len());
        for pin in actual.pin_map {
            assert_eq!(pin, screen_pin(&screen, 0, 0));
        }
    }

    #[test]
    fn screen_formatter_invalid_or_garbage_selection_returns_empty_output_and_map() {
        let screen = screen_with_lines(&["hello"]);
        let other = screen_with_lines(&["other"]);
        let valid = screen_pin(&screen, 0, 0);
        let invalid = screen_pin(&other, 0, 0);
        let mut garbage = valid;
        garbage.mark_garbage_for_tests();

        for selection in [
            selection::Selection::new(invalid, valid, false),
            selection::Selection::new(valid, invalid, false),
            selection::Selection::new(garbage, valid, false),
            selection::Selection::new(valid, garbage, false),
        ] {
            let actual = formatter(&screen, PageOutputFormat::Plain)
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
