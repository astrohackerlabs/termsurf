//! Terminal state.

use super::color;
use super::modes;
use super::page_list::{
    CodepointMapEntry, PageListAllocError, PageOutputFormat, PageStringWithPinMap,
};
use super::screen::{
    Screen, ScreenFormatter, ScreenFormatterContent, ScreenFormatterExtra, ScreenFormatterOptions,
};
use super::size::CellCountInt;

#[derive(Debug)]
pub(super) struct Terminal {
    screens: TerminalScreens,
    colors: TerminalColors,
    modes: modes::ModeState,
}

#[derive(Debug)]
pub(super) struct TerminalScreens {
    active: Screen,
}

#[derive(Debug, Clone, Copy)]
struct TerminalColors {
    palette: color::Palette,
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
    screen: ScreenFormatterExtra,
}

impl Terminal {
    pub(super) fn init(
        cols: CellCountInt,
        rows: CellCountInt,
        max_scrollback_rows: Option<usize>,
    ) -> Result<Self, PageListAllocError> {
        Ok(Self {
            screens: TerminalScreens {
                active: Screen::init(cols, rows, max_scrollback_rows)?,
            },
            colors: TerminalColors {
                palette: color::DEFAULT_PALETTE,
            },
            modes: modes::ModeState::default(),
        })
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
        output
    }

    pub(super) fn format_with_pin_map(self) -> PageStringWithPinMap {
        let prefix = self.terminal_prefix_string();
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

        output
    }

    fn terminal_prefix_string(&self) -> String {
        let mut output = self.palette_string();
        output.push_str(&self.modes_string());
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
}

impl TerminalFormatterExtra {
    pub(super) const fn none() -> Self {
        Self {
            palette: false,
            modes: false,
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
