//! Configuration types.
//!
//! The leaf config types consumed by roastty subsystems (renderer, font,
//! terminal, input, clipboard) and the aggregating [`Config`] struct (upstream
//! `config.Config`). `Config` is grown one coherent field group per slice; the
//! full key set, the parser, and file loading are ported in later slices.
#![allow(dead_code)]
// This config layer is consumed by later slices.

use crate::terminal::color::{Palette as TerminalPalette, PaletteMask, Rgb, DEFAULT_PALETTE};
use crate::terminal::style::BoldColor as TerminalBoldColor;

/// The aggregating config struct (upstream `config.Config`) — the home of the
/// config keys. Built up one coherent field group per slice; this lands the
/// clipboard group. The full key set, the parser, and file loading are ported
/// later.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Config {
    /// `copy-on-select`.
    pub copy_on_select: CopyOnSelect,
    /// `clipboard-read`.
    pub clipboard_read: ClipboardAccess,
    /// `clipboard-write`.
    pub clipboard_write: ClipboardAccess,
    /// `mouse-shift-capture`.
    pub mouse_shift_capture: MouseShiftCapture,
    /// `right-click-action`.
    pub right_click_action: RightClickAction,
    /// `middle-click-action`.
    pub middle_click_action: MiddleClickAction,
    /// `shell-integration`.
    pub shell_integration: ShellIntegration,
    /// `shell-integration-features`.
    pub shell_integration_features: ShellIntegrationFeatures,
    /// `notify-on-command-finish`.
    pub notify_on_command_finish: NotifyOnCommandFinish,
    /// `notify-on-command-finish-action`.
    pub notify_on_command_finish_action: NotifyOnCommandFinishAction,
    /// `window-colorspace`.
    pub window_colorspace: WindowColorspace,
    /// `alpha-blending`.
    pub alpha_blending: AlphaBlending,
    /// `background-blur`.
    pub background_blur: BackgroundBlur,
    /// `window-padding-color`.
    pub window_padding_color: WindowPaddingColor,
    /// `background-image-opacity`.
    pub bg_image_opacity: f32,
    /// `background-image-position`.
    pub bg_image_position: BackgroundImagePosition,
    /// `background-image-fit`.
    pub bg_image_fit: BackgroundImageFit,
    /// `background-image-repeat`.
    pub bg_image_repeat: bool,
    /// `cursor-color`.
    pub cursor_color: Option<TerminalColor>,
    /// `cursor-text`.
    pub cursor_text: Option<TerminalColor>,
    /// `selection-foreground`.
    pub selection_foreground: Option<TerminalColor>,
    /// `selection-background`.
    pub selection_background: Option<TerminalColor>,
    /// `bold-color`.
    pub bold_color: Option<BoldColor>,
    /// `confirm-close-surface`.
    pub confirm_close_surface: ConfirmCloseSurface,
    /// `link-previews`.
    pub link_previews: LinkPreviews,
    /// `window-subtitle`.
    pub window_subtitle: WindowSubtitle,
    /// `fullscreen`.
    pub fullscreen: Fullscreen,
    /// `macos-non-native-fullscreen`.
    pub macos_non_native_fullscreen: NonNativeFullscreen,
    /// `macos-titlebar-style`.
    pub macos_titlebar_style: MacTitlebarStyle,
    /// `macos-titlebar-proxy-icon`.
    pub macos_titlebar_proxy_icon: MacTitlebarProxyIcon,
    /// `macos-window-buttons`.
    pub macos_window_buttons: MacWindowButtons,
    /// `macos-hidden`.
    pub macos_hidden: MacHidden,
    /// `font-style`.
    pub font_style: FontStyle,
    /// `font-style-bold`.
    pub font_style_bold: FontStyle,
    /// `font-style-italic`.
    pub font_style_italic: FontStyle,
    /// `font-style-bold-italic`.
    pub font_style_bold_italic: FontStyle,
    /// `font-shaping-break`.
    pub font_shaping_break: FontShapingBreak,
    /// `grapheme-width-method`.
    pub grapheme_width_method: GraphemeWidthMethod,
    /// `osc-color-report-format`.
    pub osc_color_report_format: OscColorReportFormat,
    /// `scroll-to-bottom`.
    pub scroll_to_bottom: ScrollToBottom,
    /// `custom-shader-animation`.
    pub custom_shader_animation: CustomShaderAnimation,
    /// `background`.
    pub background: Color,
    /// `foreground`.
    pub foreground: Color,
    /// `theme`.
    pub theme: Option<Theme>,
}

impl Default for Config {
    /// Upstream's `Config` field defaults for the clipboard group (macOS):
    /// `copy-on-select` is `True`, `clipboard-read` is `Ask`, `clipboard-write`
    /// is `Allow`.
    fn default() -> Self {
        Self {
            copy_on_select: CopyOnSelect::True,
            clipboard_read: ClipboardAccess::Ask,
            clipboard_write: ClipboardAccess::Allow,
            mouse_shift_capture: MouseShiftCapture::False,
            right_click_action: RightClickAction::ContextMenu,
            middle_click_action: MiddleClickAction::PrimaryPaste,
            shell_integration: ShellIntegration::Detect,
            shell_integration_features: ShellIntegrationFeatures::default(),
            notify_on_command_finish: NotifyOnCommandFinish::Never,
            notify_on_command_finish_action: NotifyOnCommandFinishAction::default(),
            window_colorspace: WindowColorspace::Srgb,
            alpha_blending: AlphaBlending::Native,
            background_blur: BackgroundBlur::False,
            window_padding_color: WindowPaddingColor::Background,
            bg_image_opacity: 1.0,
            bg_image_position: BackgroundImagePosition::Center,
            bg_image_fit: BackgroundImageFit::Contain,
            bg_image_repeat: false,
            cursor_color: None,
            cursor_text: None,
            selection_foreground: None,
            selection_background: None,
            bold_color: None,
            confirm_close_surface: ConfirmCloseSurface::True,
            link_previews: LinkPreviews::True,
            window_subtitle: WindowSubtitle::False,
            fullscreen: Fullscreen::False,
            macos_non_native_fullscreen: NonNativeFullscreen::False,
            macos_titlebar_style: MacTitlebarStyle::Transparent,
            macos_titlebar_proxy_icon: MacTitlebarProxyIcon::Visible,
            macos_window_buttons: MacWindowButtons::Visible,
            macos_hidden: MacHidden::Never,
            font_style: FontStyle::Default,
            font_style_bold: FontStyle::Default,
            font_style_italic: FontStyle::Default,
            font_style_bold_italic: FontStyle::Default,
            font_shaping_break: FontShapingBreak::default(),
            grapheme_width_method: GraphemeWidthMethod::Unicode,
            osc_color_report_format: OscColorReportFormat::Bits16,
            scroll_to_bottom: ScrollToBottom::default(),
            custom_shader_animation: CustomShaderAnimation::True,
            background: Color {
                r: 0x28,
                g: 0x2C,
                b: 0x34,
            },
            foreground: Color {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF,
            },
            theme: None,
        }
    }
}

/// A config color value (upstream `Config.Color`): an RGB byte triple. The string
/// parsing (named colors / hex) and the C extern struct are ported in later
/// slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// An error parsing a config `Color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// The input is not a valid hex color (wrong length or a non-hex digit;
    /// upstream `error.InvalidValue`).
    Invalid,
}

impl Color {
    /// Convert to the terminal-native `Rgb` (upstream `Color.toTerminalRGB`): a
    /// field-for-field copy of the three channels.
    pub(crate) fn to_terminal_rgb(self) -> Rgb {
        Rgb::new(self.r, self.g, self.b)
    }

    /// Parse a hex color (upstream `Color.fromHex`): `#RRGGBB` / `RRGGBB` /
    /// `#RGB` / `RGB`. The leading `#` is optional; a 3-digit value doubles each
    /// digit; a bad length or non-hex digit is `ColorParseError::Invalid`.
    pub(crate) fn from_hex(input: &str) -> Result<Color, ColorParseError> {
        let trimmed = input.strip_prefix('#').unwrap_or(input);
        let bytes = trimmed.as_bytes();
        let expanded: [u8; 6] = match bytes.len() {
            6 => [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]],
            3 => [bytes[0], bytes[0], bytes[1], bytes[1], bytes[2], bytes[2]],
            _ => return Err(ColorParseError::Invalid),
        };
        let digit = |c: u8| -> Result<u8, ColorParseError> {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or(ColorParseError::Invalid)
        };
        Ok(Color {
            r: digit(expanded[0])? * 16 + digit(expanded[1])?,
            g: digit(expanded[2])? * 16 + digit(expanded[3])?,
            b: digit(expanded[4])? * 16 + digit(expanded[5])?,
        })
    }

    /// Parse a config color value (upstream `Color.parseCLI`): trim surrounding
    /// spaces and tabs, look the result up in the X11 named-color map, and fall
    /// back to [`Color::from_hex`]. A missing value is
    /// `ColorParseError::ValueRequired`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<Color, ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        let trimmed = input.trim_matches(|c: char| c == ' ' || c == '\t');
        if let Some(rgb) = crate::terminal::x11_color::get(trimmed.as_bytes()) {
            return Ok(Color {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
            });
        }
        Color::from_hex(trimmed)
    }

    /// Format the color as a `#rrggbb` string (upstream `Color.formatBuf`): a
    /// `#` followed by each channel as lowercase hex, zero-padded to two digits.
    /// The inverse of [`Color::from_hex`].
    pub(crate) fn format_buf(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }
}

/// A config terminal-color value (upstream `Config.TerminalColor`): either an
/// explicit `Color` or a cell-relative sentinel (use the cell's own fg / bg).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TerminalColor {
    /// An explicit color.
    Color(Color),
    /// Use the cell's own foreground color.
    CellForeground,
    /// Use the cell's own background color.
    CellBackground,
}

impl TerminalColor {
    /// Resolve to the terminal-native `Rgb` (upstream
    /// `TerminalColor.toTerminalRGB`): an explicit `Color` resolves to its `Rgb`;
    /// the cell sentinels resolve to `None` (the consumer uses the cell's own fg /
    /// bg).
    pub(crate) fn to_terminal_rgb(self) -> Option<Rgb> {
        match self {
            TerminalColor::Color(c) => Some(c.to_terminal_rgb()),
            TerminalColor::CellForeground | TerminalColor::CellBackground => None,
        }
    }

    /// Parse a config terminal-color value (upstream `TerminalColor.parseCLI`):
    /// the keywords `cell-foreground` / `cell-background` yield the cell
    /// sentinels (exact match on the raw input); anything else delegates to
    /// [`Color::parse_cli`]. A missing value is `ColorParseError::ValueRequired`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<TerminalColor, ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        if input == "cell-foreground" {
            return Ok(TerminalColor::CellForeground);
        }
        if input == "cell-background" {
            return Ok(TerminalColor::CellBackground);
        }
        Ok(TerminalColor::Color(Color::parse_cli(Some(input))?))
    }
}

/// The `bold-color` config (upstream `Config.BoldColor`): the color to use for
/// bold text — either an explicit `Color` or the bright palette variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoldColor {
    /// An explicit color.
    Color(Color),
    /// Use the bright palette variant for bold text.
    Bright,
}

impl BoldColor {
    /// Convert to the terminal-native `BoldColor` (upstream
    /// `Config.BoldColor.toTerminal`): an explicit `Color` resolves through
    /// `to_terminal_rgb`; `Bright` maps to the terminal `Bright`.
    pub(crate) fn to_terminal(self) -> TerminalBoldColor {
        match self {
            BoldColor::Color(c) => TerminalBoldColor::Color(c.to_terminal_rgb()),
            BoldColor::Bright => TerminalBoldColor::Bright,
        }
    }

    /// Parse a config bold-color value (upstream `BoldColor.parseCLI`): the
    /// keyword `bright` yields the bright variant (exact match on the raw input);
    /// anything else delegates to [`Color::parse_cli`]. A missing value is
    /// `ColorParseError::ValueRequired`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<BoldColor, ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        if input == "bright" {
            return Ok(BoldColor::Bright);
        }
        Ok(BoldColor::Color(Color::parse_cli(Some(input))?))
    }
}

/// An error parsing the `palette` config (upstream `Palette.parseCLI` errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// No `=`, or a non-numeric key, or an unparseable color (upstream
    /// `error.InvalidValue`).
    InvalidValue,
    /// The palette index is greater than 255 (upstream `error.Overflow`).
    Overflow,
}

/// The `palette` config (upstream `Config.Palette`): the 256-entry color table
/// plus a mask of which indices the user set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Palette {
    pub value: TerminalPalette,
    pub mask: PaletteMask,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            value: DEFAULT_PALETTE,
            mask: PaletteMask::empty(),
        }
    }
}

impl Palette {
    /// Parse one `index=color` assignment (upstream `Palette.parseCLI`): split on
    /// the first `=`, parse the whitespace-trimmed base-0 `u8` key and the color
    /// (via [`Color::parse_cli`]), then set that entry and mark the mask. A
    /// missing value is `PaletteParseError::ValueRequired`; a missing `=` or a bad
    /// key/color is `InvalidValue`; a key `> 255` is `Overflow`. State is mutated
    /// only after both the key and the color parse successfully.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), PaletteParseError> {
        let value = input.ok_or(PaletteParseError::ValueRequired)?;
        let eql = value.find('=').ok_or(PaletteParseError::InvalidValue)?;

        let key = parse_palette_key(value[..eql].trim_matches(|c: char| c == ' ' || c == '\t'))?;
        let rgb = Color::parse_cli(Some(&value[eql + 1..]))
            .map_err(|_| PaletteParseError::InvalidValue)?;

        self.value[key as usize] = rgb.to_terminal_rgb();
        self.mask.set(key);
        Ok(())
    }
}

/// Parse a base-0 `u8` (upstream `std.fmt.parseInt(u8, _, 0)`). A faithful port of
/// Zig's `parseInt` / `parseIntWithSign`: an optional leading `+`/`-` sign, then
/// base auto-detection from a case-insensitive `0x`/`0o`/`0b` prefix (decimal
/// otherwise), `_` separators allowed only *between* digits (leading/trailing `_`
/// rejected). For an unsigned `u8`: `-0` is `0`, any negative nonzero is
/// `Overflow`, a value `> 255` is `Overflow`, and any other malformed input is
/// `InvalidValue` (Zig's `error.InvalidCharacter`).
fn parse_palette_key(buf: &str) -> Result<u8, PaletteParseError> {
    let bytes = buf.as_bytes();
    match bytes.first() {
        Some(b'+') => parse_u8_with_sign(&buf[1..], false),
        Some(b'-') => parse_u8_with_sign(&buf[1..], true),
        _ => parse_u8_with_sign(buf, false),
    }
}

fn parse_u8_with_sign(buf: &str, neg: bool) -> Result<u8, PaletteParseError> {
    let bytes = buf.as_bytes();
    if bytes.is_empty() {
        return Err(PaletteParseError::InvalidValue); // bare "+"/"-" or empty
    }

    // base 0: default decimal; detect a `0x`/`0o`/`0b` prefix (case-insensitive,
    // and only when there is at least one digit after it — `buf.len > 2`).
    let mut base: u32 = 10;
    let mut start: &[u8] = bytes;
    if bytes.len() > 2 && bytes[0] == b'0' {
        match bytes[1].to_ascii_lowercase() {
            b'b' => (base, start) = (2, &bytes[2..]),
            b'o' => (base, start) = (8, &bytes[2..]),
            b'x' => (base, start) = (16, &bytes[2..]),
            _ => {}
        }
    }

    // Leading/trailing underscores are rejected; interior ones are skipped.
    if start[0] == b'_' || start[start.len() - 1] == b'_' {
        return Err(PaletteParseError::InvalidValue);
    }

    // Accumulate is a `u8` for a `u8` result; every intermediate must fit 0..=255.
    let mut acc: i32 = 0;
    for &c in start {
        if c == b'_' {
            continue;
        }
        let digit = char_to_digit(c, base)? as i32;
        if acc != 0 {
            acc = acc
                .checked_mul(base as i32)
                .filter(|&v| v <= 255)
                .ok_or(PaletteParseError::Overflow)?;
        } else if neg {
            // First digit of a negative number: only `-0` survives for unsigned.
            acc = -digit;
            if acc < 0 {
                return Err(PaletteParseError::Overflow);
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=255).contains(&acc) {
            return Err(PaletteParseError::Overflow);
        }
    }
    Ok(acc as u8)
}

/// Upstream `std.fmt.charToDigit`: the digit value of an ASCII alphanumeric in the
/// given base, else `InvalidValue` (Zig's `error.InvalidCharacter`).
fn char_to_digit(c: u8, base: u32) -> Result<u32, PaletteParseError> {
    let value = match c {
        b'0'..=b'9' => (c - b'0') as u32,
        b'A'..=b'Z' => (c - b'A') as u32 + 10,
        b'a'..=b'z' => (c - b'a') as u32 + 10,
        _ => return Err(PaletteParseError::InvalidValue),
    };
    if value >= base {
        return Err(PaletteParseError::InvalidValue);
    }
    Ok(value)
}

/// The config `ColorList` (upstream `Config.ColorList`): a comma-separated list of
/// colors (1..=64). The `colors_c` C mirror and the `formatEntry` formatter are
/// ported in later slices.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ColorList {
    pub colors: Vec<Color>,
}

impl ColorList {
    /// Parse a comma-separated color list (upstream `ColorList.parseCLI`): a
    /// missing or empty value is `ColorParseError::ValueRequired`; the list is
    /// reset, then each comma-separated token (empties skipped) is trimmed and
    /// parsed via [`Color::parse_cli`]; more than 64 colors, or an all-empty
    /// input, is `Invalid`.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        if input.is_empty() {
            return Err(ColorParseError::ValueRequired);
        }

        // Always reset on parse.
        self.colors.clear();

        let mut count: usize = 0;
        for raw in input.split(',').filter(|tok| !tok.is_empty()) {
            count += 1;
            if count > 64 {
                return Err(ColorParseError::Invalid);
            }
            let trimmed = raw.trim_matches(|c: char| c == ' ' || c == '\t');
            let color = Color::parse_cli(Some(trimmed))?;
            self.colors.push(color);
        }

        if self.colors.is_empty() {
            return Err(ColorParseError::Invalid);
        }
        Ok(())
    }
}

/// The `notify-on-command-finish` config (upstream `NotifyOnCommandFinish`): when
/// to notify on a finished command. The `Config` default is `Never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotifyOnCommandFinish {
    /// Never notify.
    Never,
    /// Notify only when the window is unfocused.
    Unfocused,
    /// Always notify.
    Always,
}

impl NotifyOnCommandFinish {
    /// Whether to notify on a finished command, given the window's focused state
    /// (the config's contribution to upstream's apprt notify path): `Never` never
    /// notifies, `Unfocused` notifies only when **not** `focused`, `Always` always
    /// notifies.
    pub(crate) fn should_notify(self, focused: bool) -> bool {
        match self {
            NotifyOnCommandFinish::Never => false,
            NotifyOnCommandFinish::Unfocused => !focused,
            NotifyOnCommandFinish::Always => true,
        }
    }
}

/// The `notify-on-command-finish-action` config (upstream
/// `NotifyOnCommandFinishAction`): what a command-finish notification does.
/// `bell` (default `true`) rings the bell; `notify` (default `false`) sends a
/// desktop notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NotifyOnCommandFinishAction {
    /// Ring the bell on a finished command.
    pub bell: bool,
    /// Send a desktop notification on a finished command.
    pub notify: bool,
}

impl Default for NotifyOnCommandFinishAction {
    /// Upstream's field defaults `bell = true`, `notify = false`.
    fn default() -> Self {
        Self {
            bell: true,
            notify: false,
        }
    }
}

/// The `shell-integration` config (upstream `ShellIntegration`): which shell's
/// integration to inject. The `Config` default is `Detect`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ShellIntegration {
    /// Shell integration disabled.
    None,
    /// Auto-detect the shell.
    Detect,
    Bash,
    Elvish,
    Fish,
    Nushell,
    Zsh,
}

impl ShellIntegration {
    /// Whether shell integration is active at all (upstream's `Exec` setup
    /// `!= .none` decision): `None` disables it; `Detect` and the explicit shells
    /// enable it.
    pub(crate) fn enabled(self) -> bool {
        !matches!(self, ShellIntegration::None)
    }
}

/// The `shell-integration-features` config (upstream `ShellIntegrationFeatures`):
/// which features the injected shell integration provides. Defaults: `cursor`,
/// `title`, `path` are `true`; `sudo`, `ssh_env`, `ssh_terminfo` are `false`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ShellIntegrationFeatures {
    /// Shell cursor reporting.
    pub cursor: bool,
    /// `sudo` wrapping.
    pub sudo: bool,
    /// Window-title updates.
    pub title: bool,
    /// SSH environment propagation (upstream `ssh-env`).
    pub ssh_env: bool,
    /// SSH terminfo install (upstream `ssh-terminfo`).
    pub ssh_terminfo: bool,
    /// PATH adjustments.
    pub path: bool,
}

impl Default for ShellIntegrationFeatures {
    /// Upstream's field defaults: `cursor`, `title`, `path` are `true`; `sudo`,
    /// `ssh_env`, `ssh_terminfo` are `false`.
    fn default() -> Self {
        Self {
            cursor: true,
            sudo: false,
            title: true,
            ssh_env: false,
            ssh_terminfo: false,
            path: true,
        }
    }
}

/// The `link-previews` config (upstream `LinkPreviews`): when to show a preview
/// for a link. The `Config` default is `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LinkPreviews {
    /// No link previews.
    False,
    /// Preview every link.
    True,
    /// Preview only OSC8 hyperlinks.
    Osc8,
}

impl LinkPreviews {
    /// Whether to preview a regular (detected) link (upstream's `link_previews ==
    /// .true` check): only when `True`.
    pub(crate) fn previews_regular_link(self) -> bool {
        matches!(self, LinkPreviews::True)
    }

    /// Whether to preview an OSC8 hyperlink (upstream's `link_previews != .false`
    /// check): when `True` or `Osc8`.
    pub(crate) fn previews_osc8_link(self) -> bool {
        !matches!(self, LinkPreviews::False)
    }
}

/// The `confirm-close-surface` config (upstream `ConfirmCloseSurface`): whether
/// closing a surface asks for confirmation. The `Config` default is `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfirmCloseSurface {
    /// Never confirm.
    False,
    /// Confirm only when a command appears to be running.
    True,
    /// Always confirm.
    Always,
}

impl ConfirmCloseSurface {
    /// Whether closing needs confirmation, given whether the terminal is at a
    /// shell prompt (the config's part of upstream `Surface.needsConfirmQuit`):
    /// `Always` always confirms, `False` never confirms, `True` confirms only when
    /// **not** `at_prompt`.
    pub(crate) fn needs_confirm(self, at_prompt: bool) -> bool {
        match self {
            ConfirmCloseSurface::Always => true,
            ConfirmCloseSurface::False => false,
            ConfirmCloseSurface::True => !at_prompt,
        }
    }
}

/// The `window-subtitle` config (upstream `WindowSubtitle`): what the window
/// subtitle shows. The `Config` default is `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowSubtitle {
    /// No subtitle.
    False,
    /// Show the working directory.
    WorkingDirectory,
}

impl WindowSubtitle {
    /// Whether the subtitle shows the working directory (upstream's apprt
    /// `== .working-directory` decision): `true` only for `WorkingDirectory`.
    pub(crate) fn shows_working_directory(self) -> bool {
        matches!(self, WindowSubtitle::WorkingDirectory)
    }
}

/// The `macos-titlebar-style` config (upstream `MacTitlebarStyle`): the macOS
/// titlebar appearance. The `Config` default is `Transparent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacTitlebarStyle {
    /// The standard macOS titlebar.
    Native,
    /// A translucent titlebar.
    Transparent,
    /// A tab-integrated titlebar.
    Tabs,
    /// No titlebar.
    Hidden,
}

/// The `macos-titlebar-proxy-icon` config (upstream `MacTitlebarProxyIcon`):
/// whether the document proxy icon is shown. The `Config` default is `Visible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacTitlebarProxyIcon {
    /// Show the document proxy icon.
    Visible,
    /// Hide the document proxy icon.
    Hidden,
}

/// The `fullscreen` config (upstream `Fullscreen`): the startup fullscreen mode.
/// The `Config` default is `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fullscreen {
    /// Windowed (not fullscreen).
    False,
    /// Native fullscreen.
    True,
    /// Non-native fullscreen.
    NonNative,
    /// Non-native fullscreen with the menu bar visible.
    NonNativeVisibleMenu,
    /// Non-native fullscreen padded around the notch.
    NonNativePaddedNotch,
}

/// The `macos-non-native-fullscreen` config (upstream `NonNativeFullscreen`): the
/// non-native fullscreen style. The `Config` default is `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NonNativeFullscreen {
    /// Disabled.
    False,
    /// Enabled.
    True,
    /// Enabled with the menu bar visible.
    VisibleMenu,
    /// Enabled, padded around the notch.
    PaddedNotch,
}

/// The `macos-window-buttons` config (upstream `MacWindowButtons`): whether the
/// window's traffic-light buttons are shown. The `Config` default is `Visible`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacWindowButtons {
    /// Show the window buttons.
    Visible,
    /// Hide the window buttons.
    Hidden,
}

/// The `macos-hidden` config (upstream `MacHidden`): whether the app starts
/// hidden. The `Config` default is `Never`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MacHidden {
    /// Never start hidden.
    Never,
    /// Always start hidden.
    Always,
}

/// The `theme` config (upstream `Theme`): the theme names for light mode and dark
/// mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Theme {
    /// The theme name used in light mode.
    pub light: String,
    /// The theme name used in dark mode.
    pub dark: String,
}

impl Theme {
    /// A single theme name used for both light and dark modes (upstream's
    /// `parseCLI` non-pair path: `light = dark = name`).
    pub(crate) fn single(name: String) -> Theme {
        Theme {
            light: name.clone(),
            dark: name,
        }
    }
}

/// The `clipboard-read` / `clipboard-write` config (upstream `ClipboardAccess`):
/// whether a clipboard operation is allowed, denied, or confirmed. The `Config`
/// defaults are `Ask` for read and `Allow` for write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardAccess {
    /// Proceed without asking.
    Allow,
    /// Deny the operation.
    Deny,
    /// Require a confirmation prompt.
    Ask,
}

impl ClipboardAccess {
    /// Whether the clipboard operation is denied (upstream's `== .deny` check).
    pub(crate) fn denied(self) -> bool {
        matches!(self, ClipboardAccess::Deny)
    }

    /// Whether the clipboard operation needs a confirmation prompt (upstream's
    /// `== .ask` check).
    pub(crate) fn needs_confirm(self) -> bool {
        matches!(self, ClipboardAccess::Ask)
    }
}

/// The color space the window renders in (upstream `WindowColorspace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowColorspace {
    /// Standard sRGB.
    Srgb,
    /// Display P3 wide-gamut.
    DisplayP3,
}

/// The alpha-blending mode for text compositing (upstream `AlphaBlending`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AlphaBlending {
    /// Native (non-linear) blending.
    Native,
    /// Linear blending.
    Linear,
    /// Linear blending with correction.
    LinearCorrected,
}

impl AlphaBlending {
    /// Whether this blending mode is linear (upstream `isLinear`): `Native` is
    /// not linear; `Linear` and `LinearCorrected` are.
    pub(crate) fn is_linear(self) -> bool {
        matches!(self, AlphaBlending::Linear | AlphaBlending::LinearCorrected)
    }
}

/// How the window padding around the grid is colored (upstream
/// `WindowPaddingColor`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowPaddingColor {
    /// The configured background color fills the padding.
    Background,
    /// The edge cells' background extends into the padding (subject to per-row
    /// heuristics).
    Extend,
    /// The edge cells' background always extends into the padding.
    ExtendAlways,
}

/// The `background-blur` config (upstream `BackgroundBlur`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundBlur {
    /// Blur disabled.
    False,
    /// Blur enabled at the default radius.
    True,
    /// macOS regular glass style.
    MacosGlassRegular,
    /// macOS clear glass style.
    MacosGlassClear,
    /// Blur enabled at the given radius (disabled when `0`).
    Radius(u8),
}

impl BackgroundBlur {
    /// Whether background blur is enabled (upstream `enabled`): `False` off;
    /// `True` and the two glass styles on; `Radius(v)` on when `v > 0`.
    pub(crate) fn enabled(self) -> bool {
        match self {
            BackgroundBlur::False => false,
            BackgroundBlur::True => true,
            BackgroundBlur::Radius(v) => v > 0,
            BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => true,
        }
    }

    /// Whether this is a macOS glass style — the condition for the glass
    /// `bg_color` alpha override (upstream's `updateFrame` glass `switch`).
    pub(crate) fn is_macos_glass(self) -> bool {
        matches!(
            self,
            BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear
        )
    }
}

/// How a background image is scaled to the window (upstream
/// `BackgroundImageFit`; the `Config` default is `Contain`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundImageFit {
    /// Scale to fit inside the window, preserving aspect ratio.
    Contain,
    /// Scale to fill the window, preserving aspect ratio (cropping overflow).
    Cover,
    /// Stretch to fill the window, ignoring aspect ratio.
    Stretch,
    /// No scaling; the image is drawn at its native size.
    None,
}

/// Where a background image is anchored in the window (upstream
/// `BackgroundImagePosition`; the `Config` default is `Center`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundImagePosition {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    CenterCenter,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
    Center,
}

/// The `font-shaping-break` config (upstream `FontShapingBreak`): which
/// boundaries break a shaping run. `cursor` (default `true`) breaks the run
/// around the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontShapingBreak {
    /// Break a shaping run around the cursor.
    pub cursor: bool,
}

impl Default for FontShapingBreak {
    /// Upstream's field default `cursor: bool = true`.
    fn default() -> Self {
        Self { cursor: true }
    }
}

/// The `scroll-to-bottom` config (upstream `ScrollToBottom`): when the viewport
/// snaps to the bottom. `keystroke` (default `true`) snaps on a keystroke;
/// `output` (default `false`) snaps on new output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ScrollToBottom {
    /// Scroll to the bottom on a keystroke.
    pub keystroke: bool,
    /// Scroll to the bottom on new output.
    pub output: bool,
}

impl Default for ScrollToBottom {
    /// Upstream's field defaults `keystroke = true`, `output = false`.
    fn default() -> Self {
        Self {
            keystroke: true,
            output: false,
        }
    }
}

/// The `grapheme-width-method` config (upstream `GraphemeWidthMethod`): how the
/// terminal measures grapheme width. The `Config` default is `Unicode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GraphemeWidthMethod {
    /// Legacy per-codepoint width (grapheme clustering off).
    Legacy,
    /// Full grapheme-cluster width (grapheme clustering on).
    Unicode,
}

impl GraphemeWidthMethod {
    /// Whether this method enables the terminal's grapheme-cluster mode (upstream
    /// termio init switch): `Unicode` enables it, `Legacy` does not.
    pub(crate) fn grapheme_cluster(self) -> bool {
        match self {
            GraphemeWidthMethod::Unicode => true,
            GraphemeWidthMethod::Legacy => false,
        }
    }
}

/// The `custom-shader-animation` config (upstream `CustomShaderAnimation`):
/// whether custom-shader animations run. The `Config` default is `True`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CustomShaderAnimation {
    /// Never animate.
    False,
    /// Animate only when the window is focused.
    True,
    /// Always animate, focused or not.
    Always,
}

impl CustomShaderAnimation {
    /// Whether the custom-shader animation draw timer should run, given the
    /// window's focused state (upstream `Thread.zig`'s `syncDrawTimer` switch):
    /// `Always` always animates, `True` animates only when `focused`, `False`
    /// never animates.
    pub(crate) fn should_animate(self, focused: bool) -> bool {
        match self {
            CustomShaderAnimation::Always => true,
            CustomShaderAnimation::True => focused,
            CustomShaderAnimation::False => false,
        }
    }
}

/// The `font-style*` config (upstream `FontStyle`): how a font style (bold,
/// italic, …) is selected. The `Config` default is `Default`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FontStyle {
    /// Use the default font style that font discovery finds.
    Default,
    /// Disable this style completely; fall back to the regular font.
    False,
    /// Use a specific named font style.
    Name(String),
}

impl FontStyle {
    /// Whether this style is enabled (upstream `DerivedConfig.init`'s
    /// `config.@"font-style-*" != .false`): enabled unless `False` — `Default`
    /// and `Name` both leave the style enabled.
    pub(crate) fn enabled(&self) -> bool {
        !matches!(self, FontStyle::False)
    }
}

/// The `mouse-shift-capture` config (upstream `MouseShiftCapture`): whether the
/// shift modifier may be captured by mouse events. The `Config` default is
/// `False`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MouseShiftCapture {
    /// Default off, but a terminal request can override.
    False,
    /// Default on, but a terminal request can override.
    True,
    /// Always capture (no terminal override).
    Always,
    /// Never capture (no terminal override).
    Never,
}

impl MouseShiftCapture {
    /// Whether the shift modifier may be captured, given the terminal's own
    /// `mouse_shift_capture` request (upstream `Surface.mouseShiftCapture`):
    /// `Never`/`Always` decide outright; otherwise the terminal request
    /// (`Some(false)` / `Some(true)`) decides, and only when it is `None` does
    /// the config `False`/`True` provide the default.
    pub(crate) fn capture_shift(self, terminal_request: Option<bool>) -> bool {
        match self {
            MouseShiftCapture::Never => false,
            MouseShiftCapture::Always => true,
            MouseShiftCapture::False | MouseShiftCapture::True => match terminal_request {
                Some(v) => v,
                None => matches!(self, MouseShiftCapture::True),
            },
        }
    }
}

/// The `copy-on-select` config (upstream `CopyOnSelect`): whether selecting text
/// copies it, and to which clipboards. The `Config` default is OS-dependent
/// (`True` on macOS / Linux, `False` elsewhere).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CopyOnSelect {
    /// Copy-on-select disabled.
    False,
    /// Enabled; the selection goes to the selection clipboard.
    True,
    /// Enabled; the selection goes to both the system and selection clipboards.
    Clipboard,
}

impl CopyOnSelect {
    /// Whether copy-on-select is active at all (upstream's `copy_on_select !=
    /// .false` guard): `False` is off; `True` and `Clipboard` are on.
    pub(crate) fn enabled(self) -> bool {
        !matches!(self, CopyOnSelect::False)
    }
}

/// The `right-click-action` config (upstream `RightClickAction`): what a
/// right-click does. The `Config` default is `ContextMenu`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RightClickAction {
    /// No action on right-click.
    Ignore,
    /// Paste from the system clipboard.
    Paste,
    /// Copy the selected text to the system clipboard.
    Copy,
    /// Copy the selected text, or paste the clipboard if no text is selected.
    CopyOrPaste,
    /// Show a context menu.
    ContextMenu,
}

/// The `middle-click-action` config (upstream `MiddleClickAction`): what a
/// middle-click does. The `Config` default is `PrimaryPaste`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MiddleClickAction {
    /// Paste from the selection/standard clipboard per `copy-on-select`.
    PrimaryPaste,
    /// No action on middle-click.
    Ignore,
}

/// The `osc-color-report-format` config (upstream `OSCColorReportFormat`): the
/// precision of OSC color query reports. The `Config` default is `Bits16`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OscColorReportFormat {
    /// Color reports disabled.
    None,
    /// Report at 8-bit channel precision (upstream `8-bit`).
    Bits8,
    /// Report at 16-bit channel precision (upstream `16-bit`).
    Bits16,
}

impl OscColorReportFormat {
    /// Whether OSC color queries are answered at all (upstream's
    /// `osc_color_report_format == .none` guard): `None` disables reports;
    /// `Bits8` and `Bits16` enable them.
    pub(crate) fn reports(self) -> bool {
        !matches!(self, OscColorReportFormat::None)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, BoldColor,
        ClipboardAccess, Color, ColorList, ColorParseError, Config, ConfirmCloseSurface,
        CopyOnSelect, CustomShaderAnimation, FontShapingBreak, FontStyle, Fullscreen,
        GraphemeWidthMethod, LinkPreviews, MacHidden, MacTitlebarProxyIcon, MacTitlebarStyle,
        MacWindowButtons, MiddleClickAction, MouseShiftCapture, NonNativeFullscreen,
        NotifyOnCommandFinish, NotifyOnCommandFinishAction, OscColorReportFormat, Palette,
        PaletteParseError, RightClickAction, ScrollToBottom, ShellIntegration,
        ShellIntegrationFeatures, TerminalBoldColor, TerminalColor, Theme, WindowColorspace,
        WindowPaddingColor, WindowSubtitle,
    };
    use crate::terminal::color::Rgb;

    #[test]
    fn alpha_blending_is_linear_truth_table() {
        assert!(!AlphaBlending::Native.is_linear());
        assert!(AlphaBlending::Linear.is_linear());
        assert!(AlphaBlending::LinearCorrected.is_linear());
    }

    #[test]
    fn background_blur_enabled_truth_table() {
        assert!(!BackgroundBlur::False.enabled());
        assert!(BackgroundBlur::True.enabled());
        assert!(!BackgroundBlur::Radius(0).enabled());
        assert!(BackgroundBlur::Radius(5).enabled());
        assert!(BackgroundBlur::MacosGlassRegular.enabled());
        assert!(BackgroundBlur::MacosGlassClear.enabled());
    }

    #[test]
    fn background_blur_is_macos_glass_only_for_glass_styles() {
        assert!(BackgroundBlur::MacosGlassRegular.is_macos_glass());
        assert!(BackgroundBlur::MacosGlassClear.is_macos_glass());
        assert!(!BackgroundBlur::False.is_macos_glass());
        assert!(!BackgroundBlur::True.is_macos_glass());
        assert!(!BackgroundBlur::Radius(5).is_macos_glass());
    }

    #[test]
    fn background_image_fit_has_the_four_upstream_variants() {
        let fits = [
            BackgroundImageFit::Contain,
            BackgroundImageFit::Cover,
            BackgroundImageFit::Stretch,
            BackgroundImageFit::None,
        ];
        assert_eq!(fits.len(), 4);
        assert_ne!(BackgroundImageFit::Contain, BackgroundImageFit::None);
        // `Copy` + `Eq`: a trivial round-trip.
        let f = BackgroundImageFit::Cover;
        let copied = f;
        assert_eq!(f, copied);
    }

    #[test]
    fn background_image_position_has_the_ten_upstream_variants() {
        let positions = [
            BackgroundImagePosition::TopLeft,
            BackgroundImagePosition::TopCenter,
            BackgroundImagePosition::TopRight,
            BackgroundImagePosition::CenterLeft,
            BackgroundImagePosition::CenterCenter,
            BackgroundImagePosition::CenterRight,
            BackgroundImagePosition::BottomLeft,
            BackgroundImagePosition::BottomCenter,
            BackgroundImagePosition::BottomRight,
            BackgroundImagePosition::Center,
        ];
        assert_eq!(positions.len(), 10);
        assert_ne!(
            BackgroundImagePosition::CenterCenter,
            BackgroundImagePosition::Center
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let p = BackgroundImagePosition::TopLeft;
        let copied = p;
        assert_eq!(p, copied);
    }

    #[test]
    fn font_shaping_break_defaults_cursor_true() {
        assert!(FontShapingBreak::default().cursor);
        let off = FontShapingBreak { cursor: false };
        assert_ne!(off, FontShapingBreak::default());
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = off;
        assert_eq!(off, copied);
    }

    #[test]
    fn fullscreen_has_the_five_upstream_variants() {
        let modes = [
            Fullscreen::False,
            Fullscreen::True,
            Fullscreen::NonNative,
            Fullscreen::NonNativeVisibleMenu,
            Fullscreen::NonNativePaddedNotch,
        ];
        assert_eq!(modes.len(), 5);
        assert_ne!(Fullscreen::False, Fullscreen::NonNativePaddedNotch);
        // `Copy` + `Eq`: a trivial round-trip.
        let m = Fullscreen::NonNative;
        let copied = m;
        assert_eq!(m, copied);
    }

    #[test]
    fn non_native_fullscreen_has_the_four_upstream_variants() {
        let modes = [
            NonNativeFullscreen::False,
            NonNativeFullscreen::True,
            NonNativeFullscreen::VisibleMenu,
            NonNativeFullscreen::PaddedNotch,
        ];
        assert_eq!(modes.len(), 4);
        assert_ne!(NonNativeFullscreen::False, NonNativeFullscreen::PaddedNotch);
        // `Copy` + `Eq`: a trivial round-trip.
        let m = NonNativeFullscreen::VisibleMenu;
        let copied = m;
        assert_eq!(m, copied);
    }

    #[test]
    fn mac_window_buttons_has_the_two_upstream_variants() {
        let buttons = [MacWindowButtons::Visible, MacWindowButtons::Hidden];
        assert_eq!(buttons.len(), 2);
        assert_ne!(MacWindowButtons::Visible, MacWindowButtons::Hidden);
        // `Copy` + `Eq`: a trivial round-trip.
        let b = MacWindowButtons::Visible;
        let copied = b;
        assert_eq!(b, copied);
    }

    #[test]
    fn mac_hidden_has_the_two_upstream_variants() {
        let modes = [MacHidden::Never, MacHidden::Always];
        assert_eq!(modes.len(), 2);
        assert_ne!(MacHidden::Never, MacHidden::Always);
        // `Copy` + `Eq`: a trivial round-trip.
        let m = MacHidden::Never;
        let copied = m;
        assert_eq!(m, copied);
    }

    #[test]
    fn mac_titlebar_style_has_the_four_upstream_variants() {
        let styles = [
            MacTitlebarStyle::Native,
            MacTitlebarStyle::Transparent,
            MacTitlebarStyle::Tabs,
            MacTitlebarStyle::Hidden,
        ];
        assert_eq!(styles.len(), 4);
        assert_ne!(MacTitlebarStyle::Native, MacTitlebarStyle::Hidden);
        // `Copy` + `Eq`: a trivial round-trip.
        let s = MacTitlebarStyle::Transparent;
        let copied = s;
        assert_eq!(s, copied);
    }

    #[test]
    fn mac_titlebar_proxy_icon_has_the_two_upstream_variants() {
        let icons = [MacTitlebarProxyIcon::Visible, MacTitlebarProxyIcon::Hidden];
        assert_eq!(icons.len(), 2);
        assert_ne!(MacTitlebarProxyIcon::Visible, MacTitlebarProxyIcon::Hidden);
        // `Copy` + `Eq`: a trivial round-trip.
        let i = MacTitlebarProxyIcon::Visible;
        let copied = i;
        assert_eq!(i, copied);
    }

    #[test]
    fn config_default_clipboard_group() {
        let d = Config::default();
        assert_eq!(d.copy_on_select, CopyOnSelect::True);
        assert_eq!(d.clipboard_read, ClipboardAccess::Ask);
        assert_eq!(d.clipboard_write, ClipboardAccess::Allow);
        // Mouse / click group (Experiment 462).
        assert_eq!(d.mouse_shift_capture, MouseShiftCapture::False);
        assert_eq!(d.right_click_action, RightClickAction::ContextMenu);
        assert_eq!(d.middle_click_action, MiddleClickAction::PrimaryPaste);
        // Shell-integration group (Experiment 463).
        assert_eq!(d.shell_integration, ShellIntegration::Detect);
        assert_eq!(
            d.shell_integration_features,
            ShellIntegrationFeatures::default()
        );
        // Notification group (Experiment 464).
        assert_eq!(d.notify_on_command_finish, NotifyOnCommandFinish::Never);
        assert_eq!(
            d.notify_on_command_finish_action,
            NotifyOnCommandFinishAction::default()
        );
        // Renderer-appearance group (Experiment 465).
        assert_eq!(d.window_colorspace, WindowColorspace::Srgb);
        assert_eq!(d.alpha_blending, AlphaBlending::Native);
        assert_eq!(d.background_blur, BackgroundBlur::False);
        assert_eq!(d.window_padding_color, WindowPaddingColor::Background);
        // Background-image group (Experiment 466).
        assert_eq!(d.bg_image_opacity, 1.0);
        assert_eq!(d.bg_image_position, BackgroundImagePosition::Center);
        assert_eq!(d.bg_image_fit, BackgroundImageFit::Contain);
        assert!(!d.bg_image_repeat);
        // Optional-colors group (Experiment 467).
        assert_eq!(d.cursor_color, None);
        assert_eq!(d.cursor_text, None);
        assert_eq!(d.selection_foreground, None);
        assert_eq!(d.selection_background, None);
        assert_eq!(d.bold_color, None);
        // Surface-policy group (Experiment 468).
        assert_eq!(d.confirm_close_surface, ConfirmCloseSurface::True);
        assert_eq!(d.link_previews, LinkPreviews::True);
        assert_eq!(d.window_subtitle, WindowSubtitle::False);
        // macOS-window group (Experiment 469).
        assert_eq!(d.fullscreen, Fullscreen::False);
        assert_eq!(d.macos_non_native_fullscreen, NonNativeFullscreen::False);
        assert_eq!(d.macos_titlebar_style, MacTitlebarStyle::Transparent);
        assert_eq!(d.macos_titlebar_proxy_icon, MacTitlebarProxyIcon::Visible);
        assert_eq!(d.macos_window_buttons, MacWindowButtons::Visible);
        assert_eq!(d.macos_hidden, MacHidden::Never);
        // Font group (Experiment 470).
        assert_eq!(d.font_style, FontStyle::Default);
        assert_eq!(d.font_style_bold, FontStyle::Default);
        assert_eq!(d.font_style_italic, FontStyle::Default);
        assert_eq!(d.font_style_bold_italic, FontStyle::Default);
        assert_eq!(d.font_shaping_break, FontShapingBreak::default());
        // Terminal/render-behavior group (Experiment 471).
        assert_eq!(d.grapheme_width_method, GraphemeWidthMethod::Unicode);
        assert_eq!(d.osc_color_report_format, OscColorReportFormat::Bits16);
        assert_eq!(d.scroll_to_bottom, ScrollToBottom::default());
        assert_eq!(d.custom_shader_animation, CustomShaderAnimation::True);
        // Base-colors group (Experiment 472).
        assert_eq!(
            d.background,
            Color {
                r: 0x28,
                g: 0x2C,
                b: 0x34
            }
        );
        assert_eq!(
            d.foreground,
            Color {
                r: 0xFF,
                g: 0xFF,
                b: 0xFF
            }
        );
        assert_eq!(d.theme, None);

        // A modified config differs from the default and round-trips Clone/PartialEq.
        let mut modified = Config::default();
        modified.clipboard_read = ClipboardAccess::Deny;
        assert_ne!(modified, d);
        let cloned = modified.clone();
        assert_eq!(modified, cloned);
    }

    #[test]
    fn clipboard_access_denied_and_needs_confirm() {
        // (denied, needs_confirm) per variant.
        assert!(!ClipboardAccess::Allow.denied());
        assert!(!ClipboardAccess::Allow.needs_confirm());

        assert!(ClipboardAccess::Deny.denied());
        assert!(!ClipboardAccess::Deny.needs_confirm());

        assert!(!ClipboardAccess::Ask.denied());
        assert!(ClipboardAccess::Ask.needs_confirm());

        assert_ne!(ClipboardAccess::Allow, ClipboardAccess::Deny);
        // `Copy` + `Eq`: a trivial round-trip.
        let c = ClipboardAccess::Ask;
        let copied = c;
        assert_eq!(c, copied);
    }

    #[test]
    fn config_theme_single_sets_both_modes() {
        let single = Theme::single("foo".to_string());
        assert_eq!(single.light, "foo");
        assert_eq!(single.dark, "foo");

        // A light/dark pair has distinct names and differs from the single.
        let pair = Theme {
            light: "a".to_string(),
            dark: "b".to_string(),
        };
        assert_ne!(pair.light, pair.dark);
        assert_ne!(pair, Theme::single("a".to_string()));
        // `Clone` + `Eq`: a round-trip (the names are owned).
        let cloned = single.clone();
        assert_eq!(single, cloned);
    }

    #[test]
    fn window_subtitle_shows_working_directory_only_for_that_variant() {
        assert!(!WindowSubtitle::False.shows_working_directory());
        assert!(WindowSubtitle::WorkingDirectory.shows_working_directory());

        assert_ne!(WindowSubtitle::False, WindowSubtitle::WorkingDirectory);
        // `Copy` + `Eq`: a trivial round-trip.
        let w = WindowSubtitle::WorkingDirectory;
        let copied = w;
        assert_eq!(w, copied);
    }

    #[test]
    fn confirm_close_surface_needs_confirm_truth_table() {
        use ConfirmCloseSurface::{Always, False, True};

        // Always: always confirm regardless of prompt state.
        assert!(Always.needs_confirm(true));
        assert!(Always.needs_confirm(false));
        // False: never confirm.
        assert!(!False.needs_confirm(true));
        assert!(!False.needs_confirm(false));
        // True: confirm only when not at the prompt (a command is running).
        assert!(!True.needs_confirm(true));
        assert!(True.needs_confirm(false));

        assert_ne!(False, Always);
        // `Copy` + `Eq`: a trivial round-trip.
        let c = True;
        let copied = c;
        assert_eq!(c, copied);
    }

    #[test]
    fn link_previews_predicates_by_link_kind() {
        // (previews_regular_link, previews_osc8_link) per variant.
        assert!(!LinkPreviews::False.previews_regular_link());
        assert!(!LinkPreviews::False.previews_osc8_link());

        assert!(LinkPreviews::True.previews_regular_link());
        assert!(LinkPreviews::True.previews_osc8_link());

        assert!(!LinkPreviews::Osc8.previews_regular_link());
        assert!(LinkPreviews::Osc8.previews_osc8_link());

        assert_ne!(LinkPreviews::True, LinkPreviews::Osc8);
        // `Copy` + `Eq`: a trivial round-trip.
        let l = LinkPreviews::Osc8;
        let copied = l;
        assert_eq!(l, copied);
    }

    #[test]
    fn shell_integration_features_default_mixed_flags() {
        let d = ShellIntegrationFeatures::default();
        assert!(d.cursor);
        assert!(!d.sudo);
        assert!(d.title);
        assert!(!d.ssh_env);
        assert!(!d.ssh_terminfo);
        assert!(d.path);

        // All flags flipped from the default.
        let flipped = ShellIntegrationFeatures {
            cursor: false,
            sudo: true,
            title: false,
            ssh_env: true,
            ssh_terminfo: true,
            path: false,
        };
        assert_ne!(flipped, d);
        // The flags are independent: differing only in `sudo` is `!=`.
        assert_ne!(
            ShellIntegrationFeatures {
                sudo: true,
                ..ShellIntegrationFeatures::default()
            },
            d
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = flipped;
        assert_eq!(flipped, copied);
    }

    #[test]
    fn shell_integration_enabled_unless_none() {
        let all = [
            ShellIntegration::None,
            ShellIntegration::Detect,
            ShellIntegration::Bash,
            ShellIntegration::Elvish,
            ShellIntegration::Fish,
            ShellIntegration::Nushell,
            ShellIntegration::Zsh,
        ];
        assert_eq!(all.len(), 7);

        assert!(!ShellIntegration::None.enabled());
        assert!(ShellIntegration::Detect.enabled());
        assert!(ShellIntegration::Bash.enabled());
        assert!(ShellIntegration::Elvish.enabled());
        assert!(ShellIntegration::Fish.enabled());
        assert!(ShellIntegration::Nushell.enabled());
        assert!(ShellIntegration::Zsh.enabled());

        assert_ne!(ShellIntegration::None, ShellIntegration::Detect);
        // `Copy` + `Eq`: a trivial round-trip.
        let s = ShellIntegration::Zsh;
        let copied = s;
        assert_eq!(s, copied);
    }

    #[test]
    fn notify_on_command_finish_should_notify_truth_table() {
        use NotifyOnCommandFinish::{Always, Never, Unfocused};

        // Never: never notifies regardless of focus.
        assert!(!Never.should_notify(true));
        assert!(!Never.should_notify(false));
        // Unfocused: notifies only when not focused.
        assert!(!Unfocused.should_notify(true));
        assert!(Unfocused.should_notify(false));
        // Always: always notifies.
        assert!(Always.should_notify(true));
        assert!(Always.should_notify(false));

        assert_ne!(Never, Always);
        // `Copy` + `Eq`: a trivial round-trip.
        let n = Unfocused;
        let copied = n;
        assert_eq!(n, copied);
    }

    #[test]
    fn notify_on_command_finish_action_defaults_bell_true_notify_false() {
        let d = NotifyOnCommandFinishAction::default();
        assert!(d.bell);
        assert!(!d.notify);

        let both = NotifyOnCommandFinishAction {
            bell: false,
            notify: true,
        };
        assert_ne!(both, d);
        // The two flags are independent: differing only in `notify` is `!=`.
        assert_ne!(
            NotifyOnCommandFinishAction {
                bell: true,
                notify: true,
            },
            d
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = both;
        assert_eq!(both, copied);
    }

    #[test]
    fn scroll_to_bottom_defaults_keystroke_true_output_false() {
        let d = ScrollToBottom::default();
        assert!(d.keystroke);
        assert!(!d.output);

        let both = ScrollToBottom {
            keystroke: false,
            output: true,
        };
        assert_ne!(both, d);
        // The two flags are independent: differing only in `output` is `!=`.
        assert_ne!(
            ScrollToBottom {
                keystroke: true,
                output: true,
            },
            d
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = both;
        assert_eq!(both, copied);
    }

    #[test]
    fn grapheme_width_method_maps_to_grapheme_cluster() {
        assert!(GraphemeWidthMethod::Unicode.grapheme_cluster());
        assert!(!GraphemeWidthMethod::Legacy.grapheme_cluster());
        assert_ne!(GraphemeWidthMethod::Unicode, GraphemeWidthMethod::Legacy);
        // `Copy` + `Eq`: a trivial round-trip.
        let m = GraphemeWidthMethod::Unicode;
        let copied = m;
        assert_eq!(m, copied);
    }

    #[test]
    fn custom_shader_animation_should_animate_truth_table() {
        // Always: animate regardless of focus.
        assert!(CustomShaderAnimation::Always.should_animate(true));
        assert!(CustomShaderAnimation::Always.should_animate(false));
        // True: animate only when focused.
        assert!(CustomShaderAnimation::True.should_animate(true));
        assert!(!CustomShaderAnimation::True.should_animate(false));
        // False: never animate.
        assert!(!CustomShaderAnimation::False.should_animate(true));
        assert!(!CustomShaderAnimation::False.should_animate(false));

        assert_ne!(CustomShaderAnimation::Always, CustomShaderAnimation::True);
        // `Copy` + `Eq`: a trivial round-trip.
        let a = CustomShaderAnimation::Always;
        let copied = a;
        assert_eq!(a, copied);
    }

    #[test]
    fn font_style_enabled_unless_false() {
        assert!(FontStyle::Default.enabled());
        assert!(FontStyle::Name("Bold".to_string()).enabled());
        assert!(!FontStyle::False.enabled());

        assert_ne!(FontStyle::Default, FontStyle::False);
        assert_ne!(
            FontStyle::Name("a".to_string()),
            FontStyle::Name("b".to_string())
        );
        // `Clone` + `Eq`: a round-trip (the `Name` payload is owned).
        let s = FontStyle::Name("Italic".to_string());
        let cloned = s.clone();
        assert_eq!(s, cloned);
    }

    #[test]
    fn mouse_shift_capture_decision_truth_table() {
        use MouseShiftCapture::{Always, False, Never, True};

        // Never / Always short-circuit regardless of the terminal request.
        for req in [None, Some(false), Some(true)] {
            assert!(!Never.capture_shift(req));
            assert!(Always.capture_shift(req));
        }

        // False: terminal request wins, else default false.
        assert!(!False.capture_shift(None));
        assert!(!False.capture_shift(Some(false)));
        assert!(False.capture_shift(Some(true)));

        // True: terminal request wins, else default true.
        assert!(True.capture_shift(None));
        assert!(!True.capture_shift(Some(false)));
        assert!(True.capture_shift(Some(true)));

        assert_ne!(False, True);
        // `Copy` + `Eq`: a trivial round-trip.
        let c = Always;
        let copied = c;
        assert_eq!(c, copied);
    }

    #[test]
    fn copy_on_select_enabled_unless_false() {
        assert!(!CopyOnSelect::False.enabled());
        assert!(CopyOnSelect::True.enabled());
        assert!(CopyOnSelect::Clipboard.enabled());

        assert_ne!(CopyOnSelect::True, CopyOnSelect::Clipboard);
        // `Copy` + `Eq`: a trivial round-trip.
        let c = CopyOnSelect::Clipboard;
        let copied = c;
        assert_eq!(c, copied);
    }

    #[test]
    fn right_click_action_has_the_five_upstream_variants() {
        let actions = [
            RightClickAction::Ignore,
            RightClickAction::Paste,
            RightClickAction::Copy,
            RightClickAction::CopyOrPaste,
            RightClickAction::ContextMenu,
        ];
        assert_eq!(actions.len(), 5);
        assert_ne!(RightClickAction::Ignore, RightClickAction::ContextMenu);
        // `Copy` + `Eq`: a trivial round-trip.
        let a = RightClickAction::CopyOrPaste;
        let copied = a;
        assert_eq!(a, copied);
    }

    #[test]
    fn middle_click_action_has_the_two_upstream_variants() {
        let actions = [MiddleClickAction::PrimaryPaste, MiddleClickAction::Ignore];
        assert_eq!(actions.len(), 2);
        assert_ne!(MiddleClickAction::PrimaryPaste, MiddleClickAction::Ignore);
        // `Copy` + `Eq`: a trivial round-trip.
        let a = MiddleClickAction::PrimaryPaste;
        let copied = a;
        assert_eq!(a, copied);
    }

    #[test]
    fn osc_color_report_format_reports_unless_none() {
        let formats = [
            OscColorReportFormat::None,
            OscColorReportFormat::Bits8,
            OscColorReportFormat::Bits16,
        ];
        assert_eq!(formats.len(), 3);

        assert!(!OscColorReportFormat::None.reports());
        assert!(OscColorReportFormat::Bits8.reports());
        assert!(OscColorReportFormat::Bits16.reports());

        assert_ne!(OscColorReportFormat::Bits8, OscColorReportFormat::Bits16);
        // `Copy` + `Eq`: a trivial round-trip.
        let f = OscColorReportFormat::Bits16;
        let copied = f;
        assert_eq!(f, copied);
    }

    #[test]
    fn config_color_converts_to_terminal_rgb() {
        let c = Color {
            r: 10,
            g: 20,
            b: 30,
        };
        assert_eq!(c.to_terminal_rgb(), Rgb::new(10, 20, 30));

        // A boundary case across the channel range.
        let edge = Color {
            r: 0,
            g: 128,
            b: 255,
        };
        assert_eq!(edge.to_terminal_rgb(), Rgb::new(0, 128, 255));

        // `Copy` + `Eq`: a round-trip and a differing value.
        let copied = c;
        assert_eq!(c, copied);
        assert_ne!(c, edge);
    }

    #[test]
    fn terminal_color_resolves_explicit_and_sentinels() {
        let explicit = TerminalColor::Color(Color {
            r: 10,
            g: 20,
            b: 30,
        });
        assert_eq!(explicit.to_terminal_rgb(), Some(Rgb::new(10, 20, 30)));

        // The cell sentinels resolve to `None` (the consumer uses the cell's fg/bg).
        assert_eq!(TerminalColor::CellForeground.to_terminal_rgb(), None);
        assert_eq!(TerminalColor::CellBackground.to_terminal_rgb(), None);

        assert_ne!(TerminalColor::CellForeground, TerminalColor::CellBackground);
        assert_ne!(
            TerminalColor::Color(Color { r: 1, g: 2, b: 3 }),
            TerminalColor::Color(Color { r: 4, g: 5, b: 6 })
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = explicit;
        assert_eq!(explicit, copied);
    }

    #[test]
    fn terminal_color_parse_cli_parses_sentinels_and_colors() {
        // Upstream `TerminalColor.parseCLI` cases.
        assert_eq!(
            TerminalColor::parse_cli(Some("#4e2a84")),
            Ok(TerminalColor::Color(Color {
                r: 78,
                g: 42,
                b: 132
            }))
        );
        assert_eq!(
            TerminalColor::parse_cli(Some("black")),
            Ok(TerminalColor::Color(Color { r: 0, g: 0, b: 0 }))
        );
        assert_eq!(
            TerminalColor::parse_cli(Some("cell-foreground")),
            Ok(TerminalColor::CellForeground)
        );
        assert_eq!(
            TerminalColor::parse_cli(Some("cell-background")),
            Ok(TerminalColor::CellBackground)
        );

        // A non-sentinel non-color is `Invalid`; a missing value is `ValueRequired`.
        assert_eq!(
            TerminalColor::parse_cli(Some("a")),
            Err(ColorParseError::Invalid)
        );
        assert_eq!(
            TerminalColor::parse_cli(None),
            Err(ColorParseError::ValueRequired)
        );

        // The sentinel match is exact/un-trimmed: a padded keyword falls through
        // to `Color::parse_cli` (which trims, then fails to parse it as a color).
        assert_eq!(
            TerminalColor::parse_cli(Some(" cell-foreground")),
            Err(ColorParseError::Invalid)
        );
    }

    #[test]
    fn bold_color_converts_to_terminal() {
        let explicit = BoldColor::Color(Color {
            r: 10,
            g: 20,
            b: 30,
        });
        assert_eq!(
            explicit.to_terminal(),
            TerminalBoldColor::Color(Rgb::new(10, 20, 30))
        );
        assert_eq!(BoldColor::Bright.to_terminal(), TerminalBoldColor::Bright);

        assert_ne!(
            BoldColor::Bright,
            BoldColor::Color(Color { r: 0, g: 0, b: 0 })
        );
        assert_ne!(
            BoldColor::Color(Color { r: 1, g: 2, b: 3 }),
            BoldColor::Color(Color { r: 4, g: 5, b: 6 })
        );
        // `Copy` + `Eq`: a trivial round-trip.
        let copied = explicit;
        assert_eq!(explicit, copied);
    }

    #[test]
    fn bold_color_parse_cli_parses_keyword_and_colors() {
        // Upstream `BoldColor.parseCLI` cases.
        assert_eq!(
            BoldColor::parse_cli(Some("#4e2a84")),
            Ok(BoldColor::Color(Color {
                r: 78,
                g: 42,
                b: 132
            }))
        );
        assert_eq!(
            BoldColor::parse_cli(Some("black")),
            Ok(BoldColor::Color(Color { r: 0, g: 0, b: 0 }))
        );
        assert_eq!(BoldColor::parse_cli(Some("bright")), Ok(BoldColor::Bright));

        // A non-keyword non-color is `Invalid`; a missing value is `ValueRequired`.
        assert_eq!(
            BoldColor::parse_cli(Some("a")),
            Err(ColorParseError::Invalid)
        );
        assert_eq!(
            BoldColor::parse_cli(None),
            Err(ColorParseError::ValueRequired)
        );

        // The keyword match is exact/un-trimmed: a padded keyword falls through
        // to `Color::parse_cli` (which trims, then fails to parse it as a color).
        assert_eq!(
            BoldColor::parse_cli(Some(" bright")),
            Err(ColorParseError::Invalid)
        );
    }

    #[test]
    fn palette_parse_cli_sets_indices_and_mask() {
        // Upstream `Palette.parseCLI`: `"0=#AABBCC"` sets index 0 and marks it.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("0=#AABBCC")), Ok(()));
        assert_eq!(p.value[0], Rgb::new(0xAA, 0xBB, 0xCC));
        assert!(p.mask.get(0));
        assert!(!p.mask.get(1));

        // The base prefixes set indices 1 / 7 / 15.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("0b1=#014589")), Ok(()));
        assert_eq!(p.parse_cli(Some("0o7=#234567")), Ok(()));
        assert_eq!(p.parse_cli(Some("0xF=#ABCDEF")), Ok(()));
        assert_eq!(p.value[1], Rgb::new(0x01, 0x45, 0x89));
        assert_eq!(p.value[7], Rgb::new(0x23, 0x45, 0x67));
        assert_eq!(p.value[15], Rgb::new(0xAB, 0xCD, 0xEF));
        assert!(p.mask.get(1) && p.mask.get(7) && p.mask.get(15));
        assert!(!p.mask.get(0) && !p.mask.get(2));

        // An overflowing key errors and leaves the table and mask unchanged.
        let mut p = Palette::default();
        let before = p.value[0];
        assert_eq!(
            p.parse_cli(Some("256=#AABBCC")),
            Err(PaletteParseError::Overflow)
        );
        assert!(p.mask.is_empty());
        assert_eq!(p.value[0], before);

        // Whitespace around the key and color is tolerated.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("0 =  #AABBCC")), Ok(()));
        assert_eq!(p.parse_cli(Some(" 1= #DDEEFF    ")), Ok(()));
        assert_eq!(p.parse_cli(Some("  2  =  #123456 ")), Ok(()));
        assert_eq!(p.value[0], Rgb::new(0xAA, 0xBB, 0xCC));
        assert_eq!(p.value[1], Rgb::new(0xDD, 0xEE, 0xFF));
        assert_eq!(p.value[2], Rgb::new(0x12, 0x34, 0x56));
        assert!(p.mask.get(0) && p.mask.get(1) && p.mask.get(2) && !p.mask.get(3));

        // Top-level errors: missing value, no `=`, and a bad color.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(None), Err(PaletteParseError::ValueRequired));
        assert_eq!(p.parse_cli(Some("0")), Err(PaletteParseError::InvalidValue));
        assert_eq!(
            p.parse_cli(Some("0=nope")),
            Err(PaletteParseError::InvalidValue)
        );
        assert!(p.mask.is_empty());
    }

    #[test]
    fn palette_parse_cli_key_matches_zig_parse_int() {
        // Uppercase base prefixes parse the same as lowercase.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("0XF=#ABCDEF")), Ok(()));
        assert_eq!(p.parse_cli(Some("0B1=#014589")), Ok(()));
        assert_eq!(p.parse_cli(Some("0O7=#234567")), Ok(()));
        assert!(p.mask.get(15) && p.mask.get(1) && p.mask.get(7));

        // A leading `+` is accepted.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("+0xF=#ABCDEF")), Ok(()));
        assert_eq!(p.parse_cli(Some("+0=#AABBCC")), Ok(()));
        assert!(p.mask.get(15) && p.mask.get(0));

        // Unsigned sign rules: `-0` is index 0; any negative nonzero overflows.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("-0=#AABBCC")), Ok(()));
        assert!(p.mask.get(0));
        assert_eq!(
            p.parse_cli(Some("-1=#AABBCC")),
            Err(PaletteParseError::Overflow)
        );

        // Interior underscores are allowed; leading/trailing/bare-prefix are not.
        let mut p = Palette::default();
        assert_eq!(p.parse_cli(Some("1_0=#AABBCC")), Ok(())); // decimal 10
        assert_eq!(p.parse_cli(Some("0x1_0=#AABBCC")), Ok(())); // hex 0x10 = 16
        assert_eq!(p.value[10], Rgb::new(0xAA, 0xBB, 0xCC));
        assert_eq!(p.value[16], Rgb::new(0xAA, 0xBB, 0xCC));
        for bad in ["_0=#AABBCC", "0_=#AABBCC", "0x_10=#AABBCC", "0x10_=#AABBCC"] {
            assert_eq!(
                p.parse_cli(Some(bad)),
                Err(PaletteParseError::InvalidValue),
                "{bad}"
            );
        }
        // A bare prefix has no digits and is invalid.
        assert_eq!(
            p.parse_cli(Some("0x=#AABBCC")),
            Err(PaletteParseError::InvalidValue)
        );
    }

    #[test]
    fn color_list_parse_cli_parses_comma_separated_colors() {
        let black = Color { r: 0, g: 0, b: 0 };
        let white = Color {
            r: 255,
            g: 255,
            b: 255,
        };

        // Upstream `ColorList.parseCLI`: `"black,white"` → 2 colors.
        let mut list = ColorList::default();
        assert_eq!(list.parse_cli(Some("black,white")), Ok(()));
        assert_eq!(list.colors, vec![black, white]);

        // Whitespace around the commas and ends is trimmed per token.
        for input in ["black, white", "black , white", " black , white "] {
            let mut list = ColorList::default();
            assert_eq!(list.parse_cli(Some(input)), Ok(()));
            assert_eq!(list.colors, vec![black, white], "{input}");
        }

        // Empty tokens (doubled / leading / trailing commas) are skipped.
        for input in ["black,,white", ",black,white,", "black,white,"] {
            let mut list = ColorList::default();
            assert_eq!(list.parse_cli(Some(input)), Ok(()));
            assert_eq!(list.colors, vec![black, white], "{input}");
        }

        // Each parse resets the list rather than appending.
        let mut list = ColorList::default();
        assert_eq!(list.parse_cli(Some("black,white")), Ok(()));
        assert_eq!(list.parse_cli(Some("white")), Ok(()));
        assert_eq!(list.colors, vec![white]);

        // Missing / empty input is `ValueRequired`; a whitespace-only token,
        // an all-empty input, and a bad color are `Invalid`.
        let mut list = ColorList::default();
        assert_eq!(list.parse_cli(None), Err(ColorParseError::ValueRequired));
        assert_eq!(
            list.parse_cli(Some("")),
            Err(ColorParseError::ValueRequired)
        );
        assert_eq!(list.parse_cli(Some(" ")), Err(ColorParseError::Invalid));
        assert_eq!(list.parse_cli(Some(",,")), Err(ColorParseError::Invalid));
        assert_eq!(
            list.parse_cli(Some("black,nope")),
            Err(ColorParseError::Invalid)
        );

        // The cap is 64 colors; the 65th token is `Invalid`.
        let ok = ["black"; 64].join(",");
        let mut list = ColorList::default();
        assert_eq!(list.parse_cli(Some(&ok)), Ok(()));
        assert_eq!(list.colors.len(), 64);
        let too_many = ["black"; 65].join(",");
        let mut list = ColorList::default();
        assert_eq!(
            list.parse_cli(Some(&too_many)),
            Err(ColorParseError::Invalid)
        );
    }

    #[test]
    fn from_hex_parses_hex_colors() {
        // Upstream `Color.fromHex` cases.
        assert_eq!(Color::from_hex("#000000"), Ok(Color { r: 0, g: 0, b: 0 }));
        assert_eq!(
            Color::from_hex("#0A0B0C"),
            Ok(Color {
                r: 10,
                g: 11,
                b: 12
            })
        );
        assert_eq!(
            Color::from_hex("0A0B0C"),
            Ok(Color {
                r: 10,
                g: 11,
                b: 12
            })
        );
        assert_eq!(
            Color::from_hex("FFFFFF"),
            Ok(Color {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(
            Color::from_hex("FFF"),
            Ok(Color {
                r: 255,
                g: 255,
                b: 255
            })
        );
        assert_eq!(
            Color::from_hex("#345"),
            Ok(Color {
                r: 51,
                g: 68,
                b: 85
            })
        );

        // Lowercase parses the same as uppercase.
        assert_eq!(
            Color::from_hex("0a0b0c"),
            Ok(Color {
                r: 10,
                g: 11,
                b: 12
            })
        );

        // Errors: a wrong length and a non-hex digit are both `Invalid`.
        assert_eq!(Color::from_hex("12345"), Err(ColorParseError::Invalid));
        assert_eq!(Color::from_hex("ZZZZZZ"), Err(ColorParseError::Invalid));
    }

    #[test]
    fn parse_cli_parses_names_and_hex() {
        // Upstream `Color.parseCLI` cases.
        assert_eq!(
            Color::parse_cli(Some("black")),
            Ok(Color { r: 0, g: 0, b: 0 })
        );
        assert_eq!(
            Color::parse_cli(Some(" #AABBCC   ")),
            Ok(Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC
            })
        );
        assert_eq!(
            Color::parse_cli(Some("  black ")),
            Ok(Color { r: 0, g: 0, b: 0 })
        );

        // A hex value passes straight through to `from_hex`.
        assert_eq!(
            Color::parse_cli(Some("#0A0B0C")),
            Ok(Color {
                r: 10,
                g: 11,
                b: 12
            })
        );

        // Named-color lookup is ASCII case-insensitive.
        assert_eq!(
            Color::parse_cli(Some("ForestGreen")),
            Ok(Color {
                r: 34,
                g: 139,
                b: 34
            })
        );

        // Tabs are trimmed too (the X11 map alone trims only spaces).
        assert_eq!(
            Color::parse_cli(Some("\tblack\t")),
            Ok(Color { r: 0, g: 0, b: 0 })
        );

        // A missing value is `ValueRequired`; a non-name non-hex input is `Invalid`.
        assert_eq!(Color::parse_cli(None), Err(ColorParseError::ValueRequired));
        assert_eq!(
            Color::parse_cli(Some("nosuchcolor")),
            Err(ColorParseError::Invalid)
        );
    }

    #[test]
    fn format_buf_renders_lowercase_hex() {
        // Upstream `formatEntry` example.
        assert_eq!(
            Color {
                r: 10,
                g: 11,
                b: 12
            }
            .format_buf(),
            "#0a0b0c"
        );
        assert_eq!(Color { r: 0, g: 0, b: 0 }.format_buf(), "#000000");
        assert_eq!(
            Color {
                r: 255,
                g: 255,
                b: 255
            }
            .format_buf(),
            "#ffffff"
        );
        assert_eq!(
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC
            }
            .format_buf(),
            "#aabbcc"
        );

        // The formatter is the inverse of the hex parser.
        for c in [
            Color { r: 0, g: 0, b: 0 },
            Color {
                r: 10,
                g: 11,
                b: 12,
            },
            Color {
                r: 0xAA,
                g: 0xBB,
                b: 0xCC,
            },
            Color {
                r: 255,
                g: 255,
                b: 255,
            },
        ] {
            assert_eq!(Color::from_hex(&c.format_buf()), Ok(c));
        }
    }
}
