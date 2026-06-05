//! Configuration types.
//!
//! The leaf config types consumed by roastty subsystems (renderer, font,
//! terminal, input, clipboard) and the aggregating [`Config`] struct (upstream
//! `config.Config`). `Config` is grown one coherent field group per slice; the
//! full key set, the parser, and file loading are ported in later slices.
#![allow(dead_code)]
// This config layer is consumed by later slices.

mod comma_splitter;
mod formatter;
mod loader;
mod string;
mod unicode_range;

use crate::config::comma_splitter::CommaSplitter;
use crate::config::formatter::EntryFormatter;
use crate::config::string::{codepoint_iterator, parse_quoted_string};
use crate::terminal::color::{Palette as TerminalPalette, PaletteMask, Rgb, DEFAULT_PALETTE};
use crate::terminal::selection_codepoints::DEFAULT_WORD_BOUNDARIES;
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

impl Config {
    /// Format the whole config as `key = value\n` lines, one per field, in
    /// upstream `Config` declaration order (upstream `FileFormatter.format`,
    /// `config/formatter_file.zig`, the default non-docs / non-changed path).
    ///
    /// `background-image-opacity` (an `f32`) is omitted: the generic float `{d}`
    /// branch is float-formatting blocked (Experiment 509).
    pub(crate) fn format_config(&self, out: &mut String) {
        self.font_style
            .format_entry(&mut EntryFormatter::new("font-style", out));
        self.font_style_bold
            .format_entry(&mut EntryFormatter::new("font-style-bold", out));
        self.font_style_italic
            .format_entry(&mut EntryFormatter::new("font-style-italic", out));
        self.font_style_bold_italic
            .format_entry(&mut EntryFormatter::new("font-style-bold-italic", out));
        self.font_shaping_break
            .format_entry(&mut EntryFormatter::new("font-shaping-break", out));
        self.alpha_blending
            .format_entry(&mut EntryFormatter::new("alpha-blending", out));
        self.grapheme_width_method
            .format_entry(&mut EntryFormatter::new("grapheme-width-method", out));
        EntryFormatter::new("theme", out)
            .entry_optional(self.theme.clone(), |v, f| v.format_entry(f));
        self.background
            .format_entry(&mut EntryFormatter::new("background", out));
        self.foreground
            .format_entry(&mut EntryFormatter::new("foreground", out));
        // background-image-opacity (f32) — float-formatting blocked (Exp 509), deferred.
        self.bg_image_position
            .format_entry(&mut EntryFormatter::new("background-image-position", out));
        self.bg_image_fit
            .format_entry(&mut EntryFormatter::new("background-image-fit", out));
        EntryFormatter::new("background-image-repeat", out).entry_bool(self.bg_image_repeat);
        EntryFormatter::new("selection-foreground", out)
            .entry_optional(self.selection_foreground, |v, f| v.format_entry(f));
        EntryFormatter::new("selection-background", out)
            .entry_optional(self.selection_background, |v, f| v.format_entry(f));
        EntryFormatter::new("cursor-color", out)
            .entry_optional(self.cursor_color, |v, f| v.format_entry(f));
        EntryFormatter::new("cursor-text", out)
            .entry_optional(self.cursor_text, |v, f| v.format_entry(f));
        self.scroll_to_bottom
            .format_entry(&mut EntryFormatter::new("scroll-to-bottom", out));
        self.mouse_shift_capture
            .format_entry(&mut EntryFormatter::new("mouse-shift-capture", out));
        self.background_blur
            .format_entry(&mut EntryFormatter::new("background-blur", out));
        self.notify_on_command_finish
            .format_entry(&mut EntryFormatter::new("notify-on-command-finish", out));
        self.notify_on_command_finish_action
            .format_entry(&mut EntryFormatter::new(
                "notify-on-command-finish-action",
                out,
            ));
        self.link_previews
            .format_entry(&mut EntryFormatter::new("link-previews", out));
        self.fullscreen
            .format_entry(&mut EntryFormatter::new("fullscreen", out));
        self.window_padding_color
            .format_entry(&mut EntryFormatter::new("window-padding-color", out));
        self.window_subtitle
            .format_entry(&mut EntryFormatter::new("window-subtitle", out));
        self.window_colorspace
            .format_entry(&mut EntryFormatter::new("window-colorspace", out));
        self.clipboard_read
            .format_entry(&mut EntryFormatter::new("clipboard-read", out));
        self.clipboard_write
            .format_entry(&mut EntryFormatter::new("clipboard-write", out));
        self.copy_on_select
            .format_entry(&mut EntryFormatter::new("copy-on-select", out));
        self.right_click_action
            .format_entry(&mut EntryFormatter::new("right-click-action", out));
        self.middle_click_action
            .format_entry(&mut EntryFormatter::new("middle-click-action", out));
        self.confirm_close_surface
            .format_entry(&mut EntryFormatter::new("confirm-close-surface", out));
        self.shell_integration
            .format_entry(&mut EntryFormatter::new("shell-integration", out));
        self.shell_integration_features
            .format_entry(&mut EntryFormatter::new("shell-integration-features", out));
        self.osc_color_report_format
            .format_entry(&mut EntryFormatter::new("osc-color-report-format", out));
        self.custom_shader_animation
            .format_entry(&mut EntryFormatter::new("custom-shader-animation", out));
        self.macos_non_native_fullscreen
            .format_entry(&mut EntryFormatter::new("macos-non-native-fullscreen", out));
        self.macos_window_buttons
            .format_entry(&mut EntryFormatter::new("macos-window-buttons", out));
        self.macos_titlebar_style
            .format_entry(&mut EntryFormatter::new("macos-titlebar-style", out));
        self.macos_titlebar_proxy_icon
            .format_entry(&mut EntryFormatter::new("macos-titlebar-proxy-icon", out));
        self.macos_hidden
            .format_entry(&mut EntryFormatter::new("macos-hidden", out));
        EntryFormatter::new("bold-color", out)
            .entry_optional(self.bold_color, |v, f| v.format_entry(f));
    }

    /// Set one config field from a `key = value` pair (upstream
    /// `cli.args.parseIntoField` for `Config`). Built up by category across
    /// experiments; this slice routes the enum fields. A non-enum key currently
    /// returns `UnknownField` (wired in later experiments).
    pub(crate) fn set(&mut self, key: &str, value: Option<&str>) -> Result<(), ConfigSetError> {
        let default = Config::default();
        match key {
            "copy-on-select" => {
                self.copy_on_select =
                    set_enum_field(value, default.copy_on_select, CopyOnSelect::from_keyword)?
            }
            "clipboard-read" => {
                self.clipboard_read =
                    set_enum_field(value, default.clipboard_read, ClipboardAccess::from_keyword)?
            }
            "clipboard-write" => {
                self.clipboard_write = set_enum_field(
                    value,
                    default.clipboard_write,
                    ClipboardAccess::from_keyword,
                )?
            }
            "mouse-shift-capture" => {
                self.mouse_shift_capture = set_enum_field(
                    value,
                    default.mouse_shift_capture,
                    MouseShiftCapture::from_keyword,
                )?
            }
            "right-click-action" => {
                self.right_click_action = set_enum_field(
                    value,
                    default.right_click_action,
                    RightClickAction::from_keyword,
                )?
            }
            "middle-click-action" => {
                self.middle_click_action = set_enum_field(
                    value,
                    default.middle_click_action,
                    MiddleClickAction::from_keyword,
                )?
            }
            "shell-integration" => {
                self.shell_integration = set_enum_field(
                    value,
                    default.shell_integration,
                    ShellIntegration::from_keyword,
                )?
            }
            "notify-on-command-finish" => {
                self.notify_on_command_finish = set_enum_field(
                    value,
                    default.notify_on_command_finish,
                    NotifyOnCommandFinish::from_keyword,
                )?
            }
            "window-colorspace" => {
                self.window_colorspace = set_enum_field(
                    value,
                    default.window_colorspace,
                    WindowColorspace::from_keyword,
                )?
            }
            "alpha-blending" => {
                self.alpha_blending =
                    set_enum_field(value, default.alpha_blending, AlphaBlending::from_keyword)?
            }
            "window-padding-color" => {
                self.window_padding_color = set_enum_field(
                    value,
                    default.window_padding_color,
                    WindowPaddingColor::from_keyword,
                )?
            }
            "background-image-position" => {
                self.bg_image_position = set_enum_field(
                    value,
                    default.bg_image_position,
                    BackgroundImagePosition::from_keyword,
                )?
            }
            "background-image-fit" => {
                self.bg_image_fit = set_enum_field(
                    value,
                    default.bg_image_fit,
                    BackgroundImageFit::from_keyword,
                )?
            }
            "confirm-close-surface" => {
                self.confirm_close_surface = set_enum_field(
                    value,
                    default.confirm_close_surface,
                    ConfirmCloseSurface::from_keyword,
                )?
            }
            "link-previews" => {
                self.link_previews =
                    set_enum_field(value, default.link_previews, LinkPreviews::from_keyword)?
            }
            "window-subtitle" => {
                self.window_subtitle =
                    set_enum_field(value, default.window_subtitle, WindowSubtitle::from_keyword)?
            }
            "fullscreen" => {
                self.fullscreen =
                    set_enum_field(value, default.fullscreen, Fullscreen::from_keyword)?
            }
            "macos-non-native-fullscreen" => {
                self.macos_non_native_fullscreen = set_enum_field(
                    value,
                    default.macos_non_native_fullscreen,
                    NonNativeFullscreen::from_keyword,
                )?
            }
            "macos-titlebar-style" => {
                self.macos_titlebar_style = set_enum_field(
                    value,
                    default.macos_titlebar_style,
                    MacTitlebarStyle::from_keyword,
                )?
            }
            "macos-titlebar-proxy-icon" => {
                self.macos_titlebar_proxy_icon = set_enum_field(
                    value,
                    default.macos_titlebar_proxy_icon,
                    MacTitlebarProxyIcon::from_keyword,
                )?
            }
            "macos-window-buttons" => {
                self.macos_window_buttons = set_enum_field(
                    value,
                    default.macos_window_buttons,
                    MacWindowButtons::from_keyword,
                )?
            }
            "macos-hidden" => {
                self.macos_hidden =
                    set_enum_field(value, default.macos_hidden, MacHidden::from_keyword)?
            }
            "grapheme-width-method" => {
                self.grapheme_width_method = set_enum_field(
                    value,
                    default.grapheme_width_method,
                    GraphemeWidthMethod::from_keyword,
                )?
            }
            "osc-color-report-format" => {
                self.osc_color_report_format = set_enum_field(
                    value,
                    default.osc_color_report_format,
                    OscColorReportFormat::from_keyword,
                )?
            }
            "custom-shader-animation" => {
                self.custom_shader_animation = set_enum_field(
                    value,
                    default.custom_shader_animation,
                    CustomShaderAnimation::from_keyword,
                )?
            }
            "font-shaping-break" => {
                self.font_shaping_break = set_packed_field(
                    value,
                    default.font_shaping_break,
                    FontShapingBreak::parse_cli,
                )?
            }
            "scroll-to-bottom" => {
                self.scroll_to_bottom =
                    set_packed_field(value, default.scroll_to_bottom, ScrollToBottom::parse_cli)?
            }
            "shell-integration-features" => {
                self.shell_integration_features = set_packed_field(
                    value,
                    default.shell_integration_features,
                    ShellIntegrationFeatures::parse_cli,
                )?
            }
            "notify-on-command-finish-action" => {
                self.notify_on_command_finish_action = set_packed_field(
                    value,
                    default.notify_on_command_finish_action,
                    NotifyOnCommandFinishAction::parse_cli,
                )?
            }
            "background-image-repeat" => {
                self.bg_image_repeat = set_bool_field(value, default.bg_image_repeat)?
            }
            "background" => {
                self.background = set_value_field(value, default.background, Color::parse_cli)?
            }
            "foreground" => {
                self.foreground = set_value_field(value, default.foreground, Color::parse_cli)?
            }
            "cursor-color" => {
                self.cursor_color =
                    set_optional_value_field(value, default.cursor_color, TerminalColor::parse_cli)?
            }
            "cursor-text" => {
                self.cursor_text =
                    set_optional_value_field(value, default.cursor_text, TerminalColor::parse_cli)?
            }
            "selection-foreground" => {
                self.selection_foreground = set_optional_value_field(
                    value,
                    default.selection_foreground,
                    TerminalColor::parse_cli,
                )?
            }
            "selection-background" => {
                self.selection_background = set_optional_value_field(
                    value,
                    default.selection_background,
                    TerminalColor::parse_cli,
                )?
            }
            "bold-color" => {
                self.bold_color =
                    set_optional_value_field(value, default.bold_color, BoldColor::parse_cli)?
            }
            "font-style" => {
                self.font_style = set_value_field(value, default.font_style, FontStyle::parse_cli)?
            }
            "font-style-bold" => {
                self.font_style_bold =
                    set_value_field(value, default.font_style_bold, FontStyle::parse_cli)?
            }
            "font-style-italic" => {
                self.font_style_italic =
                    set_value_field(value, default.font_style_italic, FontStyle::parse_cli)?
            }
            "font-style-bold-italic" => {
                self.font_style_bold_italic =
                    set_value_field(value, default.font_style_bold_italic, FontStyle::parse_cli)?
            }
            // `BackgroundBlur::parse_cli` is `&mut self` (it overwrites `self` in
            // place), so its arm is inline: a set-but-empty value resets to the
            // default; otherwise parse in place (a missing value sets `.true`, the
            // bare-flag default).
            "background-blur" => {
                if value == Some("") {
                    self.background_blur = default.background_blur;
                } else {
                    self.background_blur.parse_cli(value)?;
                }
            }
            "theme" => {
                self.theme = set_optional_value_field(value, default.theme, Theme::parse_cli)?
            }
            _ => return Err(ConfigSetError::UnknownField),
        }
        Ok(())
    }

    /// Load config from a source string (upstream's config-file `parse` driving
    /// `LineIterator`): apply each `key = value` line via `Config::set`, skipping
    /// blank lines and `#` comments, and collect a diagnostic per failing line
    /// (continuing rather than aborting). Lines are 1-indexed, counting blanks and
    /// comments.
    pub(crate) fn load_str(&mut self, text: &str) -> Vec<ConfigDiagnostic> {
        let mut diagnostics = Vec::new();
        for (i, line) in text.split('\n').enumerate() {
            let Some((key, value)) = loader::parse_config_line(line) else {
                continue;
            };
            if let Err(error) = self.set(key, value) {
                diagnostics.push(ConfigDiagnostic {
                    line: i + 1,
                    key: key.to_string(),
                    error,
                });
            }
        }
        diagnostics
    }

    /// Load config from a file (upstream `Config.loadFile` → `loadReader`): read the
    /// file, skip a leading UTF-8 byte-order mark, and drive `load_str`. Returns the
    /// per-line diagnostics; an open/read error propagates as `io::Error`.
    pub(crate) fn load_file(
        &mut self,
        path: &std::path::Path,
    ) -> std::io::Result<Vec<ConfigDiagnostic>> {
        let text = std::fs::read_to_string(path)?;
        let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
        Ok(self.load_str(text))
    }

    /// Load a config file if it exists (upstream `Config.loadOptionalFile`): `Loaded`
    /// with the diagnostics on success, `NotFound` when the file does not exist, or
    /// `Error` for another IO error (the load is skipped, not aborted).
    pub(crate) fn load_optional_file(&mut self, path: &std::path::Path) -> OptionalFileAction {
        match self.load_file(path) {
            Ok(diagnostics) => OptionalFileAction::Loaded(diagnostics),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => OptionalFileAction::NotFound,
            Err(e) => OptionalFileAction::Error(e),
        }
    }

    /// Apply config from CLI arguments (upstream `cli.args.parse` over args): for each
    /// argument, parse the `--key=value` form (`parse_cli_arg`) and apply it via
    /// `Config::set`; a non-flag argument or a `Config::set` error records a diagnostic,
    /// and the loop continues. The diagnostic's `line` is the 1-based argument position.
    /// The caller passes the config arguments (the outer `+action`-arg filtering is a
    /// separate layer).
    pub(crate) fn set_cli_args<'a, I>(&mut self, args: I) -> Vec<ConfigDiagnostic>
    where
        I: IntoIterator<Item = &'a str>,
    {
        let mut diagnostics = Vec::new();
        for (i, arg) in args.into_iter().enumerate() {
            match loader::parse_cli_arg(arg) {
                Some((key, value)) => {
                    if let Err(error) = self.set(key, value) {
                        diagnostics.push(ConfigDiagnostic {
                            line: i + 1,
                            key: key.to_string(),
                            error,
                        });
                    }
                }
                // A non-flag argument is not a valid config field.
                None => diagnostics.push(ConfigDiagnostic {
                    line: i + 1,
                    key: arg.to_string(),
                    error: ConfigSetError::UnknownField,
                }),
            }
        }
        diagnostics
    }
}

/// The result of `Config::load_optional_file` (upstream `OptionalFileAction`).
#[derive(Debug)]
pub(crate) enum OptionalFileAction {
    /// The file was read and applied; carries its per-line diagnostics.
    Loaded(Vec<ConfigDiagnostic>),
    /// The file does not exist.
    NotFound,
    /// Another IO error occurred reading the file (the load is skipped).
    Error(std::io::Error),
}

/// An error from `Config::set` (upstream `parseIntoField`'s
/// `error.{InvalidField,InvalidValue,ValueRequired}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigSetError {
    /// No field matched the key (upstream `error.InvalidField`).
    UnknownField,
    /// The value was set but not a recognized value.
    InvalidValue,
    /// The field requires a value but none was given.
    ValueRequired,
}

/// A per-line config-load diagnostic (upstream's `parse` diagnostics): the 1-indexed
/// line, the offending key, and the `Config::set` error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConfigDiagnostic {
    pub line: usize,
    pub key: String,
    pub error: ConfigSetError,
}

/// Resolve an enum field value (upstream's empty-reset + `stringToEnum` magic): a
/// set-but-empty value resets to the default; a missing value is `ValueRequired`;
/// otherwise `parse` (the enum's `from_keyword`) or `InvalidValue`.
fn set_enum_field<T: Copy>(
    value: Option<&str>,
    default_value: T,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<T, ConfigSetError> {
    match value {
        Some("") => Ok(default_value),
        None => Err(ConfigSetError::ValueRequired),
        Some(v) => parse(v).ok_or(ConfigSetError::InvalidValue),
    }
}

/// Resolve a packed-struct field value (upstream's empty-reset + `parseStruct`
/// magic): a set-but-empty value resets to the default; a missing value is
/// `ValueRequired`; otherwise `parse` (the struct's `parse_cli`) or `InvalidValue`.
fn set_packed_field<T>(
    value: Option<&str>,
    default_value: T,
    parse: impl FnOnce(&str) -> Result<T, FlagsParseError>,
) -> Result<T, ConfigSetError> {
    match value {
        Some("") => Ok(default_value),
        None => Err(ConfigSetError::ValueRequired),
        Some(v) => parse(v).map_err(|_| ConfigSetError::InvalidValue),
    }
}

/// Resolve a `bool` field value (upstream's empty-reset + `parseBool(value orelse
/// "t")`): a set-but-empty value resets to the default; otherwise `parse_bool_field`
/// (a missing value is a bare flag, which is `true`), with `InvalidValue` on a bad
/// value.
fn set_bool_field(value: Option<&str>, default_value: bool) -> Result<bool, ConfigSetError> {
    match value {
        Some("") => Ok(default_value),
        _ => parse_bool_field(value).map_err(|_| ConfigSetError::InvalidValue),
    }
}

impl From<ColorParseError> for ConfigSetError {
    fn from(e: ColorParseError) -> Self {
        match e {
            ColorParseError::ValueRequired => ConfigSetError::ValueRequired,
            ColorParseError::Invalid => ConfigSetError::InvalidValue,
        }
    }
}

impl From<FontStyleParseError> for ConfigSetError {
    fn from(e: FontStyleParseError) -> Self {
        match e {
            FontStyleParseError::ValueRequired => ConfigSetError::ValueRequired,
        }
    }
}

impl From<BackgroundBlurParseError> for ConfigSetError {
    fn from(e: BackgroundBlurParseError) -> Self {
        match e {
            BackgroundBlurParseError::InvalidValue => ConfigSetError::InvalidValue,
        }
    }
}

/// Resolve a field whose type has a `parse_cli(Option<&str>)` (upstream's
/// empty-reset + `parseCLI`): a set-but-empty value resets to the default;
/// otherwise the type's parser (which handles a missing value itself).
fn set_value_field<T, E: Into<ConfigSetError>>(
    value: Option<&str>,
    default_value: T,
    parse: impl FnOnce(Option<&str>) -> Result<T, E>,
) -> Result<T, ConfigSetError> {
    match value {
        Some("") => Ok(default_value),
        _ => parse(value).map_err(Into::into),
    }
}

/// Resolve an `Option<T>` field whose child has a `parse_cli` (upstream's
/// optional-as-child + empty-reset): a set-but-empty value resets to the default
/// (`None`); otherwise the parsed child wrapped in `Some`.
fn set_optional_value_field<T, E: Into<ConfigSetError>>(
    value: Option<&str>,
    default_value: Option<T>,
    parse: impl FnOnce(Option<&str>) -> Result<T, E>,
) -> Result<Option<T>, ConfigSetError> {
    match value {
        Some("") => Ok(default_value),
        _ => parse(value).map(Some).map_err(Into::into),
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

    /// Format the color as a config entry (upstream `Color.formatEntry`): write the
    /// `#rrggbb` string (via [`Color::format_buf`]) as the value.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(&self.format_buf());
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

    /// Format as a config entry (upstream `TerminalColor.formatEntry`): an explicit
    /// `Color` delegates to [`Color::format_entry`]; the cell sentinels write their
    /// keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        match self {
            TerminalColor::Color(c) => c.format_entry(formatter),
            TerminalColor::CellForeground => formatter.entry_str("cell-foreground"),
            TerminalColor::CellBackground => formatter.entry_str("cell-background"),
        }
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

    /// Format as a config entry (upstream `BoldColor.formatEntry`): an explicit
    /// `Color` delegates to [`Color::format_entry`]; `Bright` writes its keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        match self {
            BoldColor::Color(c) => c.format_entry(formatter),
            BoldColor::Bright => formatter.entry_str("bright"),
        }
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

    /// Format as config entries (upstream `Palette.formatEntry`): one
    /// `index=#rrggbb` entry per palette index (all 256, mask ignored).
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        for (k, rgb) in self.value.iter().enumerate() {
            formatter.entry_str(&format!("{}=#{:02x}{:02x}{:02x}", k, rgb.r, rgb.g, rgb.b));
        }
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
    match parse_uint(buf, 0, 0xFF) {
        Ok(v) => Ok(v as u8),
        Err(IntParseError::Overflow) => Err(PaletteParseError::Overflow),
        Err(IntParseError::Invalid) => Err(PaletteParseError::InvalidValue),
    }
}

/// An integer parse error (the unsigned subset of Zig `std.fmt.parseInt`): a
/// non-digit / bad form, or a value exceeding the target's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntParseError {
    Invalid,
    Overflow,
}

/// Parse an unsigned integer (the unsigned subset of Zig `std.fmt.parseInt`).
/// `base == 0` auto-detects a case-insensitive `0x`/`0o`/`0b` prefix (only when a
/// digit follows it — `len > 2`); otherwise the fixed `base` is used. An optional
/// `+`/`-` sign (`-0` → `0`, negative nonzero → `Overflow`), interior-only `_`
/// separators (leading/trailing `_` → `Invalid`), per-step accumulation, and a value
/// above `max` → `Overflow`. A non-digit / digit `>=` base is `Invalid`.
fn parse_uint(buf: &str, base: u32, max: u64) -> Result<u64, IntParseError> {
    let (neg, rest): (bool, &str) = match buf.as_bytes().first() {
        Some(b'+') => (false, &buf[1..]),
        Some(b'-') => (true, &buf[1..]),
        _ => (false, buf),
    };

    let mut radix = base;
    let mut bytes = rest.as_bytes();
    if base == 0 {
        radix = 10;
        if bytes.len() > 2 && bytes[0] == b'0' {
            match bytes[1].to_ascii_lowercase() {
                b'b' => (radix, bytes) = (2, &bytes[2..]),
                b'o' => (radix, bytes) = (8, &bytes[2..]),
                b'x' => (radix, bytes) = (16, &bytes[2..]),
                _ => {}
            }
        }
    }

    if bytes.is_empty() || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return Err(IntParseError::Invalid);
    }

    let limit = max as i128;
    let mut acc: i128 = 0;
    for &c in bytes {
        if c == b'_' {
            continue;
        }
        let digit = (c as char).to_digit(radix).ok_or(IntParseError::Invalid)? as i128;
        if acc != 0 {
            acc = acc
                .checked_mul(radix as i128)
                .filter(|&v| v <= limit)
                .ok_or(IntParseError::Overflow)?;
        } else if neg {
            // First digit of a negative number: only `-0` survives for unsigned.
            acc = -digit;
            if acc < 0 {
                return Err(IntParseError::Overflow);
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=limit).contains(&acc) {
            return Err(IntParseError::Overflow);
        }
    }
    Ok(acc as u64)
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

    /// Format as a config entry (upstream `ColorList.formatEntry`): an empty list
    /// writes one empty entry; otherwise the colors' `#rrggbb` joined by commas.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        if self.colors.is_empty() {
            formatter.entry_void();
            return;
        }
        let joined = self
            .colors
            .iter()
            .map(|c| c.format_buf())
            .collect::<Vec<_>>()
            .join(",");
        formatter.entry_str(&joined);
    }
}

/// An error parsing a `Duration` config value (upstream `Duration.parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurationParseError {
    /// No value, or an all-whitespace value (upstream `error.ValueRequired`).
    ValueRequired,
    /// A malformed segment (upstream `error.InvalidValue`).
    InvalidValue,
}

/// A `Duration` config value (upstream `Config.Duration`): a time span in
/// nanoseconds. `format` / `asMilliseconds` / `round` / `lte` are ported later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Duration {
    pub duration: u64,
}

const NS_PER_US: u64 = 1_000;
const NS_PER_MS: u64 = 1_000_000;
const NS_PER_S: u64 = 1_000_000_000;
const NS_PER_MIN: u64 = 60 * NS_PER_S;
const NS_PER_HOUR: u64 = 60 * NS_PER_MIN;
const NS_PER_DAY: u64 = 24 * NS_PER_HOUR;
const NS_PER_WEEK: u64 = 7 * NS_PER_DAY;

/// `(name, factor-in-ns)`, in upstream order (first match is the formatting unit;
/// the longest match wins when parsing, so `ms` beats `m`).
const DURATION_UNITS: &[(&[u8], u64)] = &[
    (b"y", 365 * NS_PER_DAY),
    (b"w", NS_PER_WEEK),
    (b"d", NS_PER_DAY),
    (b"h", NS_PER_HOUR),
    (b"m", NS_PER_MIN),
    (b"s", NS_PER_S),
    (b"ms", NS_PER_MS),
    ("µs".as_bytes(), NS_PER_US),
    (b"us", NS_PER_US),
    (b"ns", 1),
];

impl Duration {
    /// Parse a duration (upstream `Duration.parseCLI`): a sequence of `number+unit`
    /// segments summed in nanoseconds with saturating math. A missing/all-whitespace
    /// value is `ValueRequired`; a bad number/unit, or a nonzero number with no
    /// unit, is `InvalidValue`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<Duration, DurationParseError> {
        let mut remaining = input.ok_or(DurationParseError::ValueRequired)?.as_bytes();

        let mut value: Option<u64> = None;
        while !remaining.is_empty() {
            // Skip whitespace before the number.
            while let [first, rest @ ..] = remaining {
                if !is_ascii_ws_zig(*first) {
                    break;
                }
                remaining = rest;
            }
            if remaining.is_empty() {
                break; // trailing whitespace is fine
            }

            // Longest number: consume leading digits, stopping before u64 overflow.
            let mut number: Option<u64> = None;
            while let [d @ b'0'..=b'9', rest @ ..] = remaining {
                match number
                    .unwrap_or(0)
                    .checked_mul(10)
                    .and_then(|n| n.checked_add((d - b'0') as u64))
                {
                    Some(n) => {
                        number = Some(n);
                        remaining = rest;
                    }
                    None => break, // this digit would overflow; leave it in `remaining`
                }
            }
            let number = number.ok_or(DurationParseError::InvalidValue)?;

            if remaining.is_empty() {
                if number == 0 {
                    value = Some(0);
                    break;
                }
                return Err(DurationParseError::InvalidValue);
            }

            // Longest matching unit (so "ms" wins over "m").
            let mut factor: Option<u64> = None;
            let mut unit_len = 0usize;
            for index in 1..=remaining.len() {
                if let Some(&(_, f)) = DURATION_UNITS
                    .iter()
                    .find(|(name, _)| *name == &remaining[..index])
                {
                    factor = Some(f);
                    unit_len = index;
                }
            }
            let factor = factor.ok_or(DurationParseError::InvalidValue)?;
            remaining = &remaining[unit_len..];

            let diff = number.saturating_mul(factor);
            value = Some(value.unwrap_or(0).saturating_add(diff));
        }

        value
            .map(|duration| Duration { duration })
            .ok_or(DurationParseError::ValueRequired)
    }

    /// Decompose into the largest matching units (upstream `Duration.format`):
    /// `{quotient}{unit}` segments, space-separated (e.g. `1m 30s`). `0` → empty.
    fn format_value(self) -> String {
        use std::fmt::Write as _;
        let mut value = self.duration;
        let mut out = String::new();
        for &(name, factor) in DURATION_UNITS {
            if value >= factor {
                if !out.is_empty() {
                    out.push(' ');
                }
                let quotient = value / factor;
                // `name` is valid UTF-8 (a unit-name byte literal, incl. `µs`).
                let _ = write!(out, "{}{}", quotient, std::str::from_utf8(name).unwrap());
                value %= factor;
            }
        }
        out
    }

    /// Format as a config entry (upstream `Duration.formatEntry`): the decomposed
    /// duration string.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(&self.format_value());
    }
}

/// Zig's `std.ascii.isWhitespace` set: space, `\t`, `\n`, `\r`, vertical tab, and
/// form feed (note vertical tab `0x0B` is not in Rust's `is_ascii_whitespace`).
fn is_ascii_ws_zig(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r' | 0x0B | 0x0C)
}

/// An error parsing `WorkingDirectory` (upstream `error.ValueRequired`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkingDirectoryParseError {
    /// No value, or an empty/all-whitespace value (upstream `error.ValueRequired`).
    ValueRequired,
}

/// The `working-directory` config (upstream `Config.WorkingDirectory`): a keyword
/// (`home` / `inherit`) or an explicit path. The `finalize` (tilde expansion) and
/// `formatEntry` are ported later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum WorkingDirectory {
    Home,
    Inherit,
    Path(String),
}

impl WorkingDirectory {
    /// Parse the `working-directory` value (upstream `parseCLI`): trim whitespace,
    /// strip a surrounding pair of quotes, then match `home` / `inherit` or fall
    /// back to a `Path`. A missing or empty/all-whitespace value is `ValueRequired`.
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), WorkingDirectoryParseError> {
        let input = input.ok_or(WorkingDirectoryParseError::ValueRequired)?;
        let input = input.trim_matches(|c: char| c.is_ascii() && is_ascii_ws_zig(c as u8));
        if input.is_empty() {
            return Err(WorkingDirectoryParseError::ValueRequired);
        }

        // Match the path quoting behavior: strip a surrounding pair of quotes.
        let input = if input.len() >= 2 && input.starts_with('"') && input.ends_with('"') {
            &input[1..input.len() - 1]
        } else {
            input
        };

        *self = match input {
            "home" => WorkingDirectory::Home,
            "inherit" => WorkingDirectory::Inherit,
            other => WorkingDirectory::Path(other.to_string()),
        };
        Ok(())
    }

    /// The explicit path, if any (upstream `value`): `Some` for `Path`, else `None`.
    pub(crate) fn value(&self) -> Option<&str> {
        match self {
            WorkingDirectory::Path(path) => Some(path),
            WorkingDirectory::Home | WorkingDirectory::Inherit => None,
        }
    }

    /// Format as a config entry (upstream `WorkingDirectory.formatEntry`): the
    /// `home` / `inherit` keyword, or the path.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        match self {
            WorkingDirectory::Home => formatter.entry_str("home"),
            WorkingDirectory::Inherit => formatter.entry_str("inherit"),
            WorkingDirectory::Path(path) => formatter.entry_str(path),
        }
    }
}

/// An error parsing `WindowPadding` (upstream `WindowPadding.parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowPaddingParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// A side did not parse as a base-10 `u32` (upstream `error.InvalidValue`).
    InvalidValue,
}

/// The `window-padding-*` config (upstream `Config.WindowPadding`): a padding pair
/// (a single value applies to both edges). The `formatEntry` formatter is ported
/// later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WindowPadding {
    pub top_left: u32,
    pub bottom_right: u32,
}

impl WindowPadding {
    /// Parse window padding (upstream `WindowPadding.parseCLI`): one base-10 `u32`
    /// applied to both edges, or two comma-separated `u32`s
    /// (`top_left,bottom_right`), each `" \t"`-trimmed. A missing value is
    /// `WindowPaddingParseError::ValueRequired`; any parse failure is
    /// `InvalidValue`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<WindowPadding, WindowPaddingParseError> {
        let input = input.ok_or(WindowPaddingParseError::ValueRequired)?;
        let trim = |s: &str| s.trim_matches(|c: char| c == ' ' || c == '\t').to_string();

        if let Some(idx) = input.find(',') {
            let left =
                parse_u32_dec(&trim(&input[..idx])).ok_or(WindowPaddingParseError::InvalidValue)?;
            let right = parse_u32_dec(&trim(&input[idx + 1..]))
                .ok_or(WindowPaddingParseError::InvalidValue)?;
            Ok(WindowPadding {
                top_left: left,
                bottom_right: right,
            })
        } else {
            let value = parse_u32_dec(&trim(input)).ok_or(WindowPaddingParseError::InvalidValue)?;
            Ok(WindowPadding {
                top_left: value,
                bottom_right: value,
            })
        }
    }

    /// Format as a config entry (upstream `WindowPadding.formatEntry`): one value
    /// when both edges are equal, else `left,right`.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        if self.top_left == self.bottom_right {
            formatter.entry_int(self.top_left);
        } else {
            formatter.entry_str(&format!("{},{}", self.top_left, self.bottom_right));
        }
    }
}

/// Parse a base-10 `u32` (upstream `std.fmt.parseInt(u32, _, 10)`); every error is
/// `None`. (The whole string must parse — unlike the greedy scan in
/// [`Duration::parse_cli`].)
fn parse_u32_dec(buf: &str) -> Option<u32> {
    parse_uint(buf, 10, u32::MAX as u64).ok().map(|v| v as u32)
}

/// An error parsing `WindowDecoration` (upstream `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowDecorationParseError {
    /// The value is neither a boolean nor a known variant name.
    InvalidValue,
}

/// The `window-decoration` config (upstream `Config.WindowDecoration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowDecoration {
    Auto,
    Client,
    Server,
    None,
}

impl WindowDecoration {
    /// Parse the `window-decoration` value (upstream `WindowDecoration.parseCLI`):
    /// a missing value is `Auto`; a boolean (`true` → `Auto`, `false` → `None`) is
    /// honored first; otherwise the variant name `auto`/`client`/`server`/`none`
    /// is matched, else `InvalidValue`.
    pub(crate) fn parse_cli(
        input: Option<&str>,
    ) -> Result<WindowDecoration, WindowDecorationParseError> {
        let Some(input) = input else {
            return Ok(WindowDecoration::Auto);
        };

        if let Some(b) = parse_bool(input) {
            return Ok(if b {
                WindowDecoration::Auto
            } else {
                WindowDecoration::None
            });
        }

        match input {
            "auto" => Ok(WindowDecoration::Auto),
            "client" => Ok(WindowDecoration::Client),
            "server" => Ok(WindowDecoration::Server),
            "none" => Ok(WindowDecoration::None),
            _ => Err(WindowDecorationParseError::InvalidValue),
        }
    }

    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowDecoration::Auto => "auto",
            WindowDecoration::Client => "client",
            WindowDecoration::Server => "server",
            WindowDecoration::None => "none",
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

/// Parse a config boolean (upstream `cli.args.parseBool`): `1`/`t`/`T`/`true` are
/// `true`; `0`/`f`/`F`/`false` are `false`; anything else is `None` (upstream's
/// `error.InvalidValue`, surfaced as `None` for the try-then-fallback callers).
fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "1" | "t" | "T" | "true" => Some(true),
        "0" | "f" | "F" | "false" => Some(false),
        _ => None,
    }
}

/// An error parsing a packed-struct bool-flag value (upstream
/// `error.InvalidValue` from `cli.args.parsePackedStruct`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FlagsParseError {
    /// A comma part named no known flag.
    InvalidValue,
}

/// A token from a packed-struct flag value: `All` (a standalone bool) or `One`
/// (a single `[no-]flag` keyword).
enum FlagToken<'a> {
    All(bool),
    One(&'a str, bool),
}

/// Parse a packed-struct bool-flag value (upstream `cli.args.parsePackedStruct`):
/// a standalone bool yields `All(b)` (setting every flag); otherwise each comma
/// part yields `One(name, on)` (a `no-` prefix means `on = false`). `apply` sets
/// the flag(s) and returns `false` for an unknown name (upstream's
/// `error.InvalidValue`).
fn parse_packed_flags(
    value: &str,
    mut apply: impl FnMut(FlagToken) -> bool,
) -> Result<(), FlagsParseError> {
    if let Some(b) = parse_bool(value) {
        apply(FlagToken::All(b));
        return Ok(());
    }
    for part in value.split(',') {
        let trimmed = part.trim_matches(|c| c == ' ' || c == '\t');
        let (name, on) = match trimmed.strip_prefix("no-") {
            Some(rest) => (rest, false),
            None => (trimmed, true),
        };
        if !apply(FlagToken::One(name, on)) {
            return Err(FlagsParseError::InvalidValue);
        }
    }
    Ok(())
}

/// An error from a type-magic field parse (upstream `error.InvalidValue` /
/// `error.ValueRequired` from `cli.args.parseIntoField`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MagicParseError {
    /// The value was set but not a recognized value.
    InvalidValue,
    /// The field requires a value but none was given.
    ValueRequired,
}

/// Parse a `bool` field (upstream `parseIntoField`'s `bool => parseBool(value
/// orelse "t")`): a missing value is a bare flag, which is `true`; otherwise
/// `parse_bool` of the value, with `InvalidValue` for an unrecognized value.
pub(crate) fn parse_bool_field(value: Option<&str>) -> Result<bool, MagicParseError> {
    match value {
        None => Ok(true),
        Some(v) => parse_bool(v).ok_or(MagicParseError::InvalidValue),
    }
}

/// Parse a string field (upstream `parseIntoField`'s `[]const u8` / `[:0]const u8`
/// copy): a missing value is `ValueRequired`; otherwise an owned copy of the value.
pub(crate) fn parse_string_field(value: Option<&str>) -> Result<String, MagicParseError> {
    match value {
        None => Err(MagicParseError::ValueRequired),
        Some(v) => Ok(v.to_string()),
    }
}

/// An error parsing a `RepeatableString` (upstream `error.ValueRequired`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RepeatableStringParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
}

/// An accumulating string-list config (upstream `Config.RepeatableString`): each
/// parse appends one value; an empty value resets the list; `overwrite_next` clears
/// before the next append. The `formatEntry` formatter is ported later.
#[derive(Debug, Default)]
pub(crate) struct RepeatableString {
    pub list: Vec<String>,
    pub overwrite_next: bool,
}

// `Clone` and `PartialEq`/`Eq` follow upstream's `clone` / `equal` (which copy /
// compare only `list`, dropping / ignoring `overwrite_next`), so they are
// implemented manually rather than derived.
impl Clone for RepeatableString {
    fn clone(&self) -> Self {
        // Upstream `clone` returns `.{ .list = list }`: `overwrite_next` resets to
        // its default `false`.
        RepeatableString {
            list: self.list.clone(),
            overwrite_next: false,
        }
    }
}

impl PartialEq for RepeatableString {
    fn eq(&self, other: &Self) -> bool {
        // Upstream `equal` compares only the list contents.
        self.list == other.list
    }
}

impl Eq for RepeatableString {}

impl RepeatableString {
    /// Parse one repeatable string value (upstream `RepeatableString.parseCLI`): a
    /// missing value is `ValueRequired`; an empty value resets the list; an
    /// `overwrite_next` clears before appending (and resets the flag); otherwise
    /// the value is appended.
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), RepeatableStringParseError> {
        let value = input.ok_or(RepeatableStringParseError::ValueRequired)?;

        // An empty value resets the list.
        if value.is_empty() {
            self.list.clear();
            return Ok(());
        }

        // If we're overwriting, clear before appending.
        if self.overwrite_next {
            self.list.clear();
            self.overwrite_next = false;
        }

        self.list.push(value.to_string());
        Ok(())
    }

    /// The number of items in the list (upstream `RepeatableString.count`).
    pub(crate) fn count(&self) -> usize {
        self.list.len()
    }

    /// Format as config entries (upstream `RepeatableString.formatEntry`): an empty
    /// list writes one empty entry; otherwise one entry per item.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        if self.list.is_empty() {
            formatter.entry_void();
            return;
        }
        for value in &self.list {
            formatter.entry_str(value);
        }
    }
}

/// An error parsing `SelectionWordChars` (upstream `parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionWordCharsParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// A codepoint failed to parse (a bad escape / bad UTF-8; upstream
    /// `error.InvalidValue`).
    InvalidValue,
}

/// The `selection-word-chars` config (upstream `Config.SelectionWordChars`): the
/// word-boundary codepoints, always starting with the null codepoint. The
/// `formatEntry` formatter is ported later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionWordChars {
    pub codepoints: Vec<u32>,
}

impl Default for SelectionWordChars {
    fn default() -> Self {
        SelectionWordChars {
            codepoints: DEFAULT_WORD_BOUNDARIES.to_vec(),
        }
    }
}

impl SelectionWordChars {
    /// Parse the `selection-word-chars` value (upstream `parseCLI`): a missing value
    /// is `ValueRequired`; otherwise the codepoints (with escape support) are parsed
    /// into a fresh list that always starts with the null codepoint, and an iterator
    /// failure is `InvalidValue`.
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), SelectionWordCharsParseError> {
        let value = input.ok_or(SelectionWordCharsParseError::ValueRequired)?;

        // Always include null as the first boundary.
        let mut list = vec![0u32];
        for cp in codepoint_iterator(value.as_bytes()) {
            list.push(cp.map_err(|_| SelectionWordCharsParseError::InvalidValue)?);
        }

        self.codepoints = list;
        Ok(())
    }

    /// Format as a config entry (upstream `SelectionWordChars.formatEntry`):
    /// re-encode the codepoints (skipping the leading null) to UTF-8, skipping any
    /// that cannot be encoded, capped at the upstream 4096-byte buffer.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        let mut out = String::new();
        for &cp in self.codepoints.iter().skip(1) {
            if let Some(c) = char::from_u32(cp) {
                // Upstream caps the output at a [4096]u8 buffer: stop before a
                // codepoint that would exceed it (writing only the buffered prefix).
                if out.len() + c.len_utf8() > 4096 {
                    break;
                }
                out.push(c);
            }
        }
        formatter.entry_str(&out);
    }
}

/// A `clipboard-codepoint-map` replacement (upstream
/// `ClipboardCodepointMap.Replacement`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClipboardReplacement {
    /// Replace with another codepoint (`U+XXXX`).
    Codepoint(u32),
    /// Replace with a literal string.
    String(String),
}

/// One `clipboard-codepoint-map` entry (upstream `ClipboardCodepointMap.Entry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClipboardCodepointMapEntry {
    pub range: [u32; 2],
    pub replacement: ClipboardReplacement,
}

/// An error parsing a `clipboard-codepoint-map` (upstream `parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClipboardCodepointMapParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// No `=`, a bad range, or a bad codepoint replacement (upstream
    /// `error.InvalidValue`).
    InvalidValue,
}

/// The `clipboard-codepoint-map` config (upstream
/// `Config.RepeatableClipboardCodepointMap`): codepoint ranges mapped to a
/// replacement. The `formatEntry` formatter is ported later.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RepeatableClipboardCodepointMap {
    pub map: Vec<ClipboardCodepointMapEntry>,
}

impl RepeatableClipboardCodepointMap {
    /// Parse one `ranges=replacement` assignment (upstream `parseCLI`): split on the
    /// first `=`, trim, parse the replacement (a `U+XXXX` codepoint or a literal
    /// string), then append `{ range, replacement }` for each range the key yields.
    /// A missing value is `ValueRequired`; a missing `=`, a bad range, or a bad
    /// codepoint replacement is `InvalidValue`.
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), ClipboardCodepointMapParseError> {
        let input = input.ok_or(ClipboardCodepointMapParseError::ValueRequired)?;
        let eql = input
            .find('=')
            .ok_or(ClipboardCodepointMapParseError::InvalidValue)?;
        let ws = |c: char| c == ' ' || c == '\t';
        let key = input[..eql].trim_matches(ws);
        let value = input[eql + 1..].trim_matches(ws);

        let replacement = if let Some(cp_str) = value.strip_prefix("U+") {
            let cp = parse_u21_hex(cp_str).ok_or(ClipboardCodepointMapParseError::InvalidValue)?;
            ClipboardReplacement::Codepoint(cp)
        } else {
            // `value` is already valid UTF-8 (it is a `&str`).
            ClipboardReplacement::String(value.to_string())
        };

        let mut parser = unicode_range::UnicodeRangeParser::new(key.as_bytes());
        while let Some(range) = parser
            .next()
            .map_err(|_| ClipboardCodepointMapParseError::InvalidValue)?
        {
            self.map.push(ClipboardCodepointMapEntry {
                range,
                replacement: replacement.clone(),
            });
        }
        Ok(())
    }

    /// Format as config entries (upstream
    /// `RepeatableClipboardCodepointMap.formatEntry`): an empty map writes one empty
    /// entry; otherwise one `U+XXXX[-U+YYYY]=value` entry per mapping (uppercase
    /// 4-digit hex keys; the value is `U+XXXX` for a codepoint replacement, else the
    /// literal string).
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        if self.map.is_empty() {
            formatter.entry_void();
            return;
        }
        for entry in &self.map {
            let value = match &entry.replacement {
                ClipboardReplacement::Codepoint(cp) => format!("U+{:04X}", cp),
                ClipboardReplacement::String(s) => s.clone(),
            };
            let [start, end] = entry.range;
            let key = if start == end {
                format!("U+{:04X}", start)
            } else {
                format!("U+{:04X}-U+{:04X}", start, end)
            };
            formatter.entry_str(&format!("{}={}", key, value));
        }
    }
}

/// Parse a base-16 `u21` (upstream `std.fmt.parseInt(u21, _, 16)`); every error is
/// `None`. (The whole string must parse — distinct from `unicode_range`'s pure-hex
/// `parse_hex_u21`, whose input is pre-scanned to hex.)
fn parse_u21_hex(buf: &str) -> Option<u32> {
    parse_uint(buf, 16, 0x1FFFFF).ok().map(|v| v as u32)
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            NotifyOnCommandFinish::Never => "never",
            NotifyOnCommandFinish::Unfocused => "unfocused",
            NotifyOnCommandFinish::Always => "always",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "never" => Some(NotifyOnCommandFinish::Never),
            "unfocused" => Some(NotifyOnCommandFinish::Unfocused),
            "always" => Some(NotifyOnCommandFinish::Always),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }

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

impl NotifyOnCommandFinishAction {
    /// Format as a config entry (upstream's packed-struct branch): the `[no-]flag`
    /// keywords comma-joined.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_flags(&[("bell", self.bell), ("notify", self.notify)]);
    }

    /// Parse a packed-struct flag value (upstream `cli.args.parsePackedStruct`): a
    /// standalone bool sets every flag; otherwise a `[no-]flag` comma-list sets the
    /// named flags, with defaults for the rest.
    pub(crate) fn parse_cli(value: &str) -> Result<Self, FlagsParseError> {
        let mut result = NotifyOnCommandFinishAction::default();
        parse_packed_flags(value, |tok| match tok {
            FlagToken::All(b) => {
                result.bell = b;
                result.notify = b;
                true
            }
            FlagToken::One("bell", on) => {
                result.bell = on;
                true
            }
            FlagToken::One("notify", on) => {
                result.notify = on;
                true
            }
            FlagToken::One(_, _) => false,
        })?;
        Ok(result)
    }
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

    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ShellIntegration::None => "none",
            ShellIntegration::Detect => "detect",
            ShellIntegration::Bash => "bash",
            ShellIntegration::Elvish => "elvish",
            ShellIntegration::Fish => "fish",
            ShellIntegration::Nushell => "nushell",
            ShellIntegration::Zsh => "zsh",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "none" => Some(ShellIntegration::None),
            "detect" => Some(ShellIntegration::Detect),
            "bash" => Some(ShellIntegration::Bash),
            "elvish" => Some(ShellIntegration::Elvish),
            "fish" => Some(ShellIntegration::Fish),
            "nushell" => Some(ShellIntegration::Nushell),
            "zsh" => Some(ShellIntegration::Zsh),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
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

impl ShellIntegrationFeatures {
    /// Format as a config entry (upstream's packed-struct branch): the `[no-]flag`
    /// keywords comma-joined (the hyphenated `ssh-env` / `ssh-terminfo` keywords).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_flags(&[
            ("cursor", self.cursor),
            ("sudo", self.sudo),
            ("title", self.title),
            ("ssh-env", self.ssh_env),
            ("ssh-terminfo", self.ssh_terminfo),
            ("path", self.path),
        ]);
    }

    /// Parse a packed-struct flag value (upstream `cli.args.parsePackedStruct`): a
    /// standalone bool sets every flag; otherwise a `[no-]flag` comma-list sets the
    /// named flags, with defaults for the rest.
    pub(crate) fn parse_cli(value: &str) -> Result<Self, FlagsParseError> {
        let mut result = ShellIntegrationFeatures::default();
        parse_packed_flags(value, |tok| match tok {
            FlagToken::All(b) => {
                result.cursor = b;
                result.sudo = b;
                result.title = b;
                result.ssh_env = b;
                result.ssh_terminfo = b;
                result.path = b;
                true
            }
            FlagToken::One("cursor", on) => {
                result.cursor = on;
                true
            }
            FlagToken::One("sudo", on) => {
                result.sudo = on;
                true
            }
            FlagToken::One("title", on) => {
                result.title = on;
                true
            }
            FlagToken::One("ssh-env", on) => {
                result.ssh_env = on;
                true
            }
            FlagToken::One("ssh-terminfo", on) => {
                result.ssh_terminfo = on;
                true
            }
            FlagToken::One("path", on) => {
                result.path = on;
                true
            }
            FlagToken::One(_, _) => false,
        })?;
        Ok(result)
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            LinkPreviews::False => "false",
            LinkPreviews::True => "true",
            LinkPreviews::Osc8 => "osc8",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(LinkPreviews::False),
            "true" => Some(LinkPreviews::True),
            "osc8" => Some(LinkPreviews::Osc8),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ConfirmCloseSurface::False => "false",
            ConfirmCloseSurface::True => "true",
            ConfirmCloseSurface::Always => "always",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(ConfirmCloseSurface::False),
            "true" => Some(ConfirmCloseSurface::True),
            "always" => Some(ConfirmCloseSurface::Always),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowSubtitle::False => "false",
            WindowSubtitle::WorkingDirectory => "working-directory",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(WindowSubtitle::False),
            "working-directory" => Some(WindowSubtitle::WorkingDirectory),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl MacTitlebarStyle {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MacTitlebarStyle::Native => "native",
            MacTitlebarStyle::Transparent => "transparent",
            MacTitlebarStyle::Tabs => "tabs",
            MacTitlebarStyle::Hidden => "hidden",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "native" => Some(MacTitlebarStyle::Native),
            "transparent" => Some(MacTitlebarStyle::Transparent),
            "tabs" => Some(MacTitlebarStyle::Tabs),
            "hidden" => Some(MacTitlebarStyle::Hidden),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl MacTitlebarProxyIcon {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MacTitlebarProxyIcon::Visible => "visible",
            MacTitlebarProxyIcon::Hidden => "hidden",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "visible" => Some(MacTitlebarProxyIcon::Visible),
            "hidden" => Some(MacTitlebarProxyIcon::Hidden),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl Fullscreen {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Fullscreen::False => "false",
            Fullscreen::True => "true",
            Fullscreen::NonNative => "non-native",
            Fullscreen::NonNativeVisibleMenu => "non-native-visible-menu",
            Fullscreen::NonNativePaddedNotch => "non-native-padded-notch",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(Fullscreen::False),
            "true" => Some(Fullscreen::True),
            "non-native" => Some(Fullscreen::NonNative),
            "non-native-visible-menu" => Some(Fullscreen::NonNativeVisibleMenu),
            "non-native-padded-notch" => Some(Fullscreen::NonNativePaddedNotch),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl NonNativeFullscreen {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            NonNativeFullscreen::False => "false",
            NonNativeFullscreen::True => "true",
            NonNativeFullscreen::VisibleMenu => "visible-menu",
            NonNativeFullscreen::PaddedNotch => "padded-notch",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(NonNativeFullscreen::False),
            "true" => Some(NonNativeFullscreen::True),
            "visible-menu" => Some(NonNativeFullscreen::VisibleMenu),
            "padded-notch" => Some(NonNativeFullscreen::PaddedNotch),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl MacWindowButtons {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MacWindowButtons::Visible => "visible",
            MacWindowButtons::Hidden => "hidden",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "visible" => Some(MacWindowButtons::Visible),
            "hidden" => Some(MacWindowButtons::Hidden),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl MacHidden {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MacHidden::Never => "never",
            MacHidden::Always => "always",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "never" => Some(MacHidden::Never),
            "always" => Some(MacHidden::Always),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

/// An error parsing a `Theme` (upstream `parseAutoStruct` / `Theme.parseCLI`
/// `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ThemeParseError {
    /// The value was malformed (missing `:`, an unknown key, a missing required
    /// field, a quoted-value decode failure, or a comma-splitter error).
    Invalid,
    /// No value was supplied, or the value was empty.
    ValueRequired,
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

    /// Format as a config entry (upstream `Theme.formatEntry`): the single name
    /// when light and dark match, else `light:{light},dark:{dark}`.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        if self.light == self.dark {
            formatter.entry_str(&self.light);
            return;
        }
        formatter.entry_str(&format!("light:{},dark:{}", self.light, self.dark));
    }

    /// Parse a `light:…,dark:…` pair (upstream `cli.args.parseAutoStruct` for
    /// `Theme`): a comma-list of `key:value` pairs into the required `light` /
    /// `dark` fields. A missing `:`, an unknown key, a missing required field, a
    /// quoted-value decode failure, or a comma-splitter error is `Invalid`.
    pub(crate) fn parse_auto_struct(input: &str) -> Result<Theme, ThemeParseError> {
        let ws = |c: char| c == ' ' || c == '\t';
        let mut light: Option<String> = None;
        let mut dark: Option<String> = None;

        let mut splitter = CommaSplitter::new(input);
        while let Some(entry) = splitter.next().map_err(|_| ThemeParseError::Invalid)? {
            let idx = entry.find(':').ok_or(ThemeParseError::Invalid)?;
            let key = entry[..idx].trim_matches(ws);
            let raw = entry[idx + 1..].trim_matches(ws);
            let value = if raw.len() >= 2 && raw.starts_with('"') && raw.ends_with('"') {
                let bytes = parse_quoted_string(raw.as_bytes()).ok_or(ThemeParseError::Invalid)?;
                String::from_utf8(bytes).map_err(|_| ThemeParseError::Invalid)?
            } else {
                raw.to_string()
            };
            match key {
                "light" => light = Some(value),
                "dark" => dark = Some(value),
                _ => return Err(ThemeParseError::Invalid),
            }
        }

        Ok(Theme {
            light: light.ok_or(ThemeParseError::Invalid)?,
            dark: dark.ok_or(ThemeParseError::Invalid)?,
        })
    }

    /// Parse the `theme` value (upstream `Theme.parseCLI`): a missing or empty value
    /// is `ValueRequired`; a value with `,` / `=` / `:` is the light/dark pair
    /// (`parse_auto_struct`); otherwise the single-name form (`light = dark =
    /// trimmed`).
    pub(crate) fn parse_cli(value: Option<&str>) -> Result<Theme, ThemeParseError> {
        let input = value.ok_or(ThemeParseError::ValueRequired)?;
        if input.is_empty() {
            return Err(ThemeParseError::ValueRequired);
        }
        if input.contains(',') || input.contains('=') || input.contains(':') {
            return Theme::parse_auto_struct(input);
        }
        let trimmed = input.trim_matches(|c: char| c == ' ' || c == '\t');
        Ok(Theme::single(trimmed.to_string()))
    }
}

impl From<ThemeParseError> for ConfigSetError {
    fn from(e: ThemeParseError) -> Self {
        match e {
            ThemeParseError::Invalid => ConfigSetError::InvalidValue,
            ThemeParseError::ValueRequired => ConfigSetError::ValueRequired,
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ClipboardAccess::Allow => "allow",
            ClipboardAccess::Deny => "deny",
            ClipboardAccess::Ask => "ask",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "allow" => Some(ClipboardAccess::Allow),
            "deny" => Some(ClipboardAccess::Deny),
            "ask" => Some(ClipboardAccess::Ask),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }

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

impl WindowColorspace {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowColorspace::Srgb => "srgb",
            WindowColorspace::DisplayP3 => "display-p3",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "srgb" => Some(WindowColorspace::Srgb),
            "display-p3" => Some(WindowColorspace::DisplayP3),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            AlphaBlending::Native => "native",
            AlphaBlending::Linear => "linear",
            AlphaBlending::LinearCorrected => "linear-corrected",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "native" => Some(AlphaBlending::Native),
            "linear" => Some(AlphaBlending::Linear),
            "linear-corrected" => Some(AlphaBlending::LinearCorrected),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }

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

impl WindowPaddingColor {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowPaddingColor::Background => "background",
            WindowPaddingColor::Extend => "extend",
            WindowPaddingColor::ExtendAlways => "extend-always",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "background" => Some(WindowPaddingColor::Background),
            "extend" => Some(WindowPaddingColor::Extend),
            "extend-always" => Some(WindowPaddingColor::ExtendAlways),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

    /// Parse the `background-blur` value (upstream `parseCLI`): a missing value or a
    /// boolean resolves on/off (`true` / `false`); `macos-glass-regular` /
    /// `macos-glass-clear` select a glass style; anything else is a base-0 `u8`
    /// radius (a bad digit or overflow is `InvalidValue`).
    pub(crate) fn parse_cli(
        &mut self,
        input: Option<&str>,
    ) -> Result<(), BackgroundBlurParseError> {
        let Some(input) = input else {
            *self = BackgroundBlur::True; // emulate the bool default
            return Ok(());
        };

        if let Some(b) = parse_bool(input) {
            *self = if b {
                BackgroundBlur::True
            } else {
                BackgroundBlur::False
            };
            return Ok(());
        }

        match input {
            "macos-glass-regular" => {
                *self = BackgroundBlur::MacosGlassRegular;
                return Ok(());
            }
            "macos-glass-clear" => {
                *self = BackgroundBlur::MacosGlassClear;
                return Ok(());
            }
            _ => {}
        }

        let radius =
            parse_uint(input, 0, 0xFF).map_err(|_| BackgroundBlurParseError::InvalidValue)?;
        *self = BackgroundBlur::Radius(radius as u8);
        Ok(())
    }

    /// Format as a config entry (upstream `BackgroundBlur.formatEntry`): a bool, an
    /// int radius, or a glass keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        match self {
            BackgroundBlur::False => formatter.entry_bool(false),
            BackgroundBlur::True => formatter.entry_bool(true),
            BackgroundBlur::Radius(v) => formatter.entry_int(v),
            BackgroundBlur::MacosGlassRegular => formatter.entry_str("macos-glass-regular"),
            BackgroundBlur::MacosGlassClear => formatter.entry_str("macos-glass-clear"),
        }
    }
}

/// An error parsing `BackgroundBlur` (upstream `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackgroundBlurParseError {
    /// The value is neither a boolean, a glass keyword, nor a base-0 `u8` radius.
    InvalidValue,
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

impl BackgroundImageFit {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            BackgroundImageFit::Contain => "contain",
            BackgroundImageFit::Cover => "cover",
            BackgroundImageFit::Stretch => "stretch",
            BackgroundImageFit::None => "none",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "contain" => Some(BackgroundImageFit::Contain),
            "cover" => Some(BackgroundImageFit::Cover),
            "stretch" => Some(BackgroundImageFit::Stretch),
            "none" => Some(BackgroundImageFit::None),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl BackgroundImagePosition {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            BackgroundImagePosition::TopLeft => "top-left",
            BackgroundImagePosition::TopCenter => "top-center",
            BackgroundImagePosition::TopRight => "top-right",
            BackgroundImagePosition::CenterLeft => "center-left",
            BackgroundImagePosition::CenterCenter => "center-center",
            BackgroundImagePosition::CenterRight => "center-right",
            BackgroundImagePosition::BottomLeft => "bottom-left",
            BackgroundImagePosition::BottomCenter => "bottom-center",
            BackgroundImagePosition::BottomRight => "bottom-right",
            BackgroundImagePosition::Center => "center",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "top-left" => Some(BackgroundImagePosition::TopLeft),
            "top-center" => Some(BackgroundImagePosition::TopCenter),
            "top-right" => Some(BackgroundImagePosition::TopRight),
            "center-left" => Some(BackgroundImagePosition::CenterLeft),
            "center-center" => Some(BackgroundImagePosition::CenterCenter),
            "center-right" => Some(BackgroundImagePosition::CenterRight),
            "bottom-left" => Some(BackgroundImagePosition::BottomLeft),
            "bottom-center" => Some(BackgroundImagePosition::BottomCenter),
            "bottom-right" => Some(BackgroundImagePosition::BottomRight),
            "center" => Some(BackgroundImagePosition::Center),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

/// The `font-shaping-break` config (upstream `FontShapingBreak`): which
/// boundaries break a shaping run. `cursor` (default `true`) breaks the run
/// around the cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontShapingBreak {
    /// Break a shaping run around the cursor.
    pub cursor: bool,
}

impl FontShapingBreak {
    /// Format as a config entry (upstream's packed-struct branch): the `[no-]flag`
    /// keywords comma-joined.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_flags(&[("cursor", self.cursor)]);
    }

    /// Parse a packed-struct flag value (upstream `cli.args.parsePackedStruct`): a
    /// standalone bool sets every flag; otherwise a `[no-]flag` comma-list sets the
    /// named flags, with defaults for the rest.
    pub(crate) fn parse_cli(value: &str) -> Result<Self, FlagsParseError> {
        let mut result = FontShapingBreak::default();
        parse_packed_flags(value, |tok| match tok {
            FlagToken::All(b) => {
                result.cursor = b;
                true
            }
            FlagToken::One("cursor", on) => {
                result.cursor = on;
                true
            }
            FlagToken::One(_, _) => false,
        })?;
        Ok(result)
    }
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

impl ScrollToBottom {
    /// Format as a config entry (upstream's packed-struct branch): the `[no-]flag`
    /// keywords comma-joined.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_flags(&[("keystroke", self.keystroke), ("output", self.output)]);
    }

    /// Parse a packed-struct flag value (upstream `cli.args.parsePackedStruct`): a
    /// standalone bool sets every flag; otherwise a `[no-]flag` comma-list sets the
    /// named flags, with defaults for the rest.
    pub(crate) fn parse_cli(value: &str) -> Result<Self, FlagsParseError> {
        let mut result = ScrollToBottom::default();
        parse_packed_flags(value, |tok| match tok {
            FlagToken::All(b) => {
                result.keystroke = b;
                result.output = b;
                true
            }
            FlagToken::One("keystroke", on) => {
                result.keystroke = on;
                true
            }
            FlagToken::One("output", on) => {
                result.output = on;
                true
            }
            FlagToken::One(_, _) => false,
        })?;
        Ok(result)
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            GraphemeWidthMethod::Legacy => "legacy",
            GraphemeWidthMethod::Unicode => "unicode",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "legacy" => Some(GraphemeWidthMethod::Legacy),
            "unicode" => Some(GraphemeWidthMethod::Unicode),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }

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

    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            CustomShaderAnimation::False => "false",
            CustomShaderAnimation::True => "true",
            CustomShaderAnimation::Always => "always",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(CustomShaderAnimation::False),
            "true" => Some(CustomShaderAnimation::True),
            "always" => Some(CustomShaderAnimation::Always),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

/// An error parsing a `FontStyle` (upstream `error.ValueRequired`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontStyleParseError {
    /// No value was supplied.
    ValueRequired,
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

    /// Parse the `font-style*` value (upstream `FontStyle.parseCLI`): a missing
    /// value is `ValueRequired`; `default` / `false` select those variants; any
    /// other value is a named style (an owned copy).
    pub(crate) fn parse_cli(value: Option<&str>) -> Result<FontStyle, FontStyleParseError> {
        let value = value.ok_or(FontStyleParseError::ValueRequired)?;
        Ok(match value {
            "default" => FontStyle::Default,
            "false" => FontStyle::False,
            other => FontStyle::Name(other.to_string()),
        })
    }

    /// Format this value as a config entry (upstream's custom union
    /// `formatEntry`): the `default` / `false` tag names or the stored style
    /// name, each written as `name = value\n` via the string `formatEntry`.
    pub(crate) fn format_entry(&self, formatter: &mut EntryFormatter) {
        match self {
            FontStyle::Default => formatter.entry_str("default"),
            FontStyle::False => formatter.entry_str("false"),
            FontStyle::Name(name) => formatter.entry_str(name),
        }
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

    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MouseShiftCapture::False => "false",
            MouseShiftCapture::True => "true",
            MouseShiftCapture::Always => "always",
            MouseShiftCapture::Never => "never",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(MouseShiftCapture::False),
            "true" => Some(MouseShiftCapture::True),
            "always" => Some(MouseShiftCapture::Always),
            "never" => Some(MouseShiftCapture::Never),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
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

    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            CopyOnSelect::False => "false",
            CopyOnSelect::True => "true",
            CopyOnSelect::Clipboard => "clipboard",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "false" => Some(CopyOnSelect::False),
            "true" => Some(CopyOnSelect::True),
            "clipboard" => Some(CopyOnSelect::Clipboard),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
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

impl RightClickAction {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            RightClickAction::Ignore => "ignore",
            RightClickAction::Paste => "paste",
            RightClickAction::Copy => "copy",
            RightClickAction::CopyOrPaste => "copy-or-paste",
            RightClickAction::ContextMenu => "context-menu",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "ignore" => Some(RightClickAction::Ignore),
            "paste" => Some(RightClickAction::Paste),
            "copy" => Some(RightClickAction::Copy),
            "copy-or-paste" => Some(RightClickAction::CopyOrPaste),
            "context-menu" => Some(RightClickAction::ContextMenu),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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

impl MiddleClickAction {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MiddleClickAction::PrimaryPaste => "primary-paste",
            MiddleClickAction::Ignore => "ignore",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "primary-paste" => Some(MiddleClickAction::PrimaryPaste),
            "ignore" => Some(MiddleClickAction::Ignore),
            _ => None,
        }
    }

    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            OscColorReportFormat::None => "none",
            OscColorReportFormat::Bits8 => "8-bit",
            OscColorReportFormat::Bits16 => "16-bit",
        }
    }

    /// Parse the config keyword (upstream `std.meta.stringToEnum`): an exact tag
    /// match, else `None`.
    pub(crate) fn from_keyword(value: &str) -> Option<Self> {
        match value {
            "none" => Some(OscColorReportFormat::None),
            "8-bit" => Some(OscColorReportFormat::Bits8),
            "16-bit" => Some(OscColorReportFormat::Bits16),
            _ => None,
        }
    }

    /// Format this value as a config entry (upstream's generic enum branch).
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
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
    use super::EntryFormatter;
    use super::{parse_bool_field, parse_string_field};
    use super::{
        AlphaBlending, BackgroundBlur, BackgroundBlurParseError, BackgroundImageFit,
        BackgroundImagePosition, BoldColor, ClipboardAccess, ClipboardCodepointMapEntry,
        ClipboardCodepointMapParseError, ClipboardReplacement, Color, ColorList, ColorParseError,
        Config, ConfigDiagnostic, ConfigSetError, ConfirmCloseSurface, CopyOnSelect,
        CustomShaderAnimation, Duration, DurationParseError, FlagsParseError, FontShapingBreak,
        FontStyle, FontStyleParseError, Fullscreen, GraphemeWidthMethod, LinkPreviews, MacHidden,
        MacTitlebarProxyIcon, MacTitlebarStyle, MacWindowButtons, MagicParseError,
        MiddleClickAction, MouseShiftCapture, NonNativeFullscreen, NotifyOnCommandFinish,
        NotifyOnCommandFinishAction, OptionalFileAction, OscColorReportFormat, Palette,
        PaletteParseError, RepeatableClipboardCodepointMap, RepeatableString,
        RepeatableStringParseError, RightClickAction, ScrollToBottom, SelectionWordChars,
        SelectionWordCharsParseError, ShellIntegration, ShellIntegrationFeatures,
        TerminalBoldColor, TerminalColor, Theme, ThemeParseError, WindowColorspace,
        WindowDecoration, WindowDecorationParseError, WindowPadding, WindowPaddingColor,
        WindowPaddingParseError, WindowSubtitle, WorkingDirectory, WorkingDirectoryParseError,
    };
    use crate::terminal::color::Rgb;
    use crate::terminal::selection_codepoints::DEFAULT_WORD_BOUNDARIES;

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
    fn duration_parse_cli_sums_segments_in_nanoseconds() {
        let dur = |ns: u64| Ok(Duration { duration: ns });

        // Single units.
        assert_eq!(Duration::parse_cli(Some("1s")), dur(1_000_000_000));
        assert_eq!(Duration::parse_cli(Some("500ms")), dur(500_000_000));
        assert_eq!(Duration::parse_cli(Some("1ns")), dur(1));
        assert_eq!(Duration::parse_cli(Some("1us")), dur(1_000));
        assert_eq!(Duration::parse_cli(Some("1µs")), dur(1_000)); // multi-byte unit
        assert_eq!(Duration::parse_cli(Some("1h")), dur(3_600_000_000_000));
        assert_eq!(Duration::parse_cli(Some("1d")), dur(86_400_000_000_000));
        assert_eq!(Duration::parse_cli(Some("1w")), dur(604_800_000_000_000));
        assert_eq!(Duration::parse_cli(Some("1y")), dur(31_536_000_000_000_000));

        // The longest-unit match distinguishes `m` from `ms`.
        assert_eq!(Duration::parse_cli(Some("1m")), dur(60_000_000_000));
        assert_eq!(Duration::parse_cli(Some("1ms")), dur(1_000_000));

        // Multi-segment sums (including inner whitespace).
        assert_eq!(
            Duration::parse_cli(Some("1h30m")),
            dur(3_600_000_000_000 + 30 * 60_000_000_000)
        );
        assert_eq!(Duration::parse_cli(Some("1m30s")), dur(90_000_000_000));
        assert_eq!(
            Duration::parse_cli(Some("1d 12h")),
            dur(86_400_000_000_000 + 12 * 3_600_000_000_000)
        );

        // Zero without a unit is allowed; whitespace at the ends is fine.
        assert_eq!(Duration::parse_cli(Some("0")), dur(0));
        assert_eq!(Duration::parse_cli(Some(" 1s ")), dur(1_000_000_000));

        // An overflowing product saturates to `u64::MAX`.
        assert_eq!(Duration::parse_cli(Some("99999999y")), dur(u64::MAX));

        // Errors.
        assert_eq!(
            Duration::parse_cli(None),
            Err(DurationParseError::ValueRequired)
        );
        assert_eq!(
            Duration::parse_cli(Some("")),
            Err(DurationParseError::ValueRequired)
        );
        assert_eq!(
            Duration::parse_cli(Some("   ")),
            Err(DurationParseError::ValueRequired)
        );
        assert_eq!(
            Duration::parse_cli(Some("5")),
            Err(DurationParseError::InvalidValue)
        );
        assert_eq!(
            Duration::parse_cli(Some("5x")),
            Err(DurationParseError::InvalidValue)
        );
        assert_eq!(
            Duration::parse_cli(Some("abc")),
            Err(DurationParseError::InvalidValue)
        );

        // A bare number followed by whitespace is not a complete segment: the unit
        // match runs on the `" "` before the next loop's whitespace skip.
        assert_eq!(
            Duration::parse_cli(Some("1 ")),
            Err(DurationParseError::InvalidValue)
        );
        assert_eq!(
            Duration::parse_cli(Some("0 ")),
            Err(DurationParseError::InvalidValue)
        );
    }

    #[test]
    fn window_padding_parse_cli_parses_single_and_pair() {
        let pad = |tl: u32, br: u32| {
            Ok(WindowPadding {
                top_left: tl,
                bottom_right: br,
            })
        };

        // Upstream `WindowPadding.parseCLI` cases.
        assert_eq!(WindowPadding::parse_cli(Some("100")), pad(100, 100));
        assert_eq!(WindowPadding::parse_cli(Some("100,200")), pad(100, 200));
        assert_eq!(WindowPadding::parse_cli(Some(" 100 , 200 ")), pad(100, 200));
        assert_eq!(
            WindowPadding::parse_cli(None),
            Err(WindowPaddingParseError::ValueRequired)
        );
        assert_eq!(
            WindowPadding::parse_cli(Some("")),
            Err(WindowPaddingParseError::InvalidValue)
        );
        assert_eq!(
            WindowPadding::parse_cli(Some("a")),
            Err(WindowPaddingParseError::InvalidValue)
        );

        // `parse_u32_dec` faithfulness (Zig `parseInt(u32, _, 10)`).
        assert_eq!(WindowPadding::parse_cli(Some("0")), pad(0, 0));
        assert_eq!(
            WindowPadding::parse_cli(Some("4294967295")),
            pad(u32::MAX, u32::MAX)
        );
        assert_eq!(
            WindowPadding::parse_cli(Some("4294967296")), // overflow
            Err(WindowPaddingParseError::InvalidValue)
        );
        assert_eq!(WindowPadding::parse_cli(Some("1_000")), pad(1000, 1000)); // interior `_`
        assert_eq!(
            WindowPadding::parse_cli(Some("_5")),
            Err(WindowPaddingParseError::InvalidValue)
        );
        assert_eq!(
            WindowPadding::parse_cli(Some("5_")),
            Err(WindowPaddingParseError::InvalidValue)
        );
        assert_eq!(WindowPadding::parse_cli(Some("+5")), pad(5, 5)); // leading `+`
        assert_eq!(WindowPadding::parse_cli(Some("-0")), pad(0, 0)); // `-0` is `0`
        assert_eq!(
            WindowPadding::parse_cli(Some("-5")), // negative nonzero
            Err(WindowPaddingParseError::InvalidValue)
        );
        assert_eq!(
            WindowPadding::parse_cli(Some("100,x")), // bad side
            Err(WindowPaddingParseError::InvalidValue)
        );
    }

    #[test]
    fn window_decoration_parse_cli_resolves_bool_and_variants() {
        // Upstream `WindowDecoration.parseCLI` cases.
        assert_eq!(
            WindowDecoration::parse_cli(None),
            Ok(WindowDecoration::Auto)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("true")),
            Ok(WindowDecoration::Auto)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("false")),
            Ok(WindowDecoration::None)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("server")),
            Ok(WindowDecoration::Server)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("client")),
            Ok(WindowDecoration::Client)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("auto")),
            Ok(WindowDecoration::Auto)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("none")),
            Ok(WindowDecoration::None)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("")),
            Err(WindowDecorationParseError::InvalidValue)
        );
        assert_eq!(
            WindowDecoration::parse_cli(Some("aaaa")),
            Err(WindowDecorationParseError::InvalidValue)
        );

        // The other `parse_bool` true / false tokens.
        for t in ["1", "t", "T"] {
            assert_eq!(
                WindowDecoration::parse_cli(Some(t)),
                Ok(WindowDecoration::Auto),
                "{t}"
            );
        }
        for f in ["0", "f", "F"] {
            assert_eq!(
                WindowDecoration::parse_cli(Some(f)),
                Ok(WindowDecoration::None),
                "{f}"
            );
        }

        // `parse_bool` is case-sensitive: "True" is neither a boolean nor a
        // variant name.
        assert_eq!(
            WindowDecoration::parse_cli(Some("True")),
            Err(WindowDecorationParseError::InvalidValue)
        );
    }

    #[test]
    fn repeatable_string_parse_cli_accumulates_and_resets() {
        // Accumulation: each call appends one whole value.
        let mut rs = RepeatableString::default();
        assert_eq!(rs.parse_cli(Some("a")), Ok(()));
        assert_eq!(rs.parse_cli(Some("b")), Ok(()));
        assert_eq!(rs.list, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(rs.count(), 2);

        // A missing value is `ValueRequired`.
        assert_eq!(
            rs.parse_cli(None),
            Err(RepeatableStringParseError::ValueRequired)
        );
        assert_eq!(rs.count(), 2); // unchanged

        // An empty value resets the list (and does not append an empty string).
        assert_eq!(rs.parse_cli(Some("")), Ok(()));
        assert!(rs.list.is_empty());

        // `overwrite_next` clears before the append and resets the flag.
        let mut rs = RepeatableString::default();
        rs.parse_cli(Some("x")).unwrap();
        rs.parse_cli(Some("y")).unwrap();
        rs.overwrite_next = true;
        assert_eq!(rs.parse_cli(Some("c")), Ok(()));
        assert_eq!(rs.list, vec!["c".to_string()]);
        assert!(!rs.overwrite_next);
        assert_eq!(rs.parse_cli(Some("d")), Ok(()));
        assert_eq!(rs.list, vec!["c".to_string(), "d".to_string()]);

        // Empty-reset order: with `overwrite_next` set, an empty value clears the
        // list but leaves the flag set (the empty branch returns first); the next
        // non-empty parse then clears-and-resets.
        let mut rs = RepeatableString::default();
        rs.parse_cli(Some("p")).unwrap();
        rs.overwrite_next = true;
        assert_eq!(rs.parse_cli(Some("")), Ok(()));
        assert!(rs.list.is_empty());
        assert!(rs.overwrite_next); // still set
        rs.list.push("q".to_string()); // simulate a prior value surviving
        assert_eq!(rs.parse_cli(Some("r")), Ok(()));
        assert_eq!(rs.list, vec!["r".to_string()]); // overwrite cleared "q"
        assert!(!rs.overwrite_next);
    }

    #[test]
    fn repeatable_string_clone_and_eq_match_upstream() {
        // `clone` copies only the list; `overwrite_next` resets to `false`.
        let mut rs = RepeatableString::default();
        rs.list.push("a".to_string());
        rs.overwrite_next = true;
        let cloned = rs.clone();
        assert_eq!(cloned.list, vec!["a".to_string()]);
        assert!(!cloned.overwrite_next);

        // Equality compares only the list, ignoring `overwrite_next`.
        let mut x = RepeatableString::default();
        x.list.push("z".to_string());
        let mut y = RepeatableString::default();
        y.list.push("z".to_string());
        y.overwrite_next = true;
        assert_eq!(x, y);
    }

    #[test]
    fn selection_word_chars_parse_cli_parses_codepoints() {
        let sp = ' ' as u32;
        let tab = '\t' as u32;
        let semi = ';' as u32;
        let comma = ',' as u32;

        // Upstream `parseCLI` cases: null is always first.
        let mut chars = SelectionWordChars::default();
        assert_eq!(chars.parse_cli(Some(" \t;,")), Ok(()));
        assert_eq!(chars.codepoints, vec![0, sp, tab, semi, comma]);

        // The `\t` escape parses to a tab — same result as the literal tab.
        let mut chars = SelectionWordChars::default();
        assert_eq!(chars.parse_cli(Some(" \\t;,")), Ok(()));
        assert_eq!(chars.codepoints, vec![0, sp, tab, semi, comma]);

        // `\\` → a single backslash.
        let mut chars = SelectionWordChars::default();
        assert_eq!(chars.parse_cli(Some("\\\\;")), Ok(()));
        assert_eq!(chars.codepoints, vec![0, '\\' as u32, semi]);

        // `\u{2502}` → the box-drawing vertical line.
        let mut chars = SelectionWordChars::default();
        assert_eq!(chars.parse_cli(Some("\\u{2502};")), Ok(()));
        assert_eq!(chars.codepoints, vec![0, 0x2502, semi]);

        // A missing value is `ValueRequired`; a bad escape is `InvalidValue`.
        let mut chars = SelectionWordChars::default();
        assert_eq!(
            chars.parse_cli(None),
            Err(SelectionWordCharsParseError::ValueRequired)
        );

        // An empty value seeds only the null boundary (not `ValueRequired`).
        let mut chars = SelectionWordChars::default();
        assert_eq!(chars.parse_cli(Some("")), Ok(()));
        assert_eq!(chars.codepoints, vec![0]);

        // A bad codepoint errors and leaves the prior value intact.
        let mut chars = SelectionWordChars::default();
        chars.parse_cli(Some("ab")).unwrap();
        let before = chars.codepoints.clone();
        assert_eq!(
            chars.parse_cli(Some("\\q")),
            Err(SelectionWordCharsParseError::InvalidValue)
        );
        assert_eq!(chars.codepoints, before);

        // The default is the terminal word-boundary set.
        assert_eq!(
            SelectionWordChars::default().codepoints,
            DEFAULT_WORD_BOUNDARIES.to_vec()
        );
    }

    #[test]
    fn clipboard_codepoint_map_parse_cli_parses_entries() {
        let entry = |lo: u32, hi: u32, r: ClipboardReplacement| ClipboardCodepointMapEntry {
            range: [lo, hi],
            replacement: r,
        };

        // Upstream `parseCLI` cases.
        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(m.parse_cli(Some("U+2500=U+002D")), Ok(()));
        assert_eq!(
            m.map,
            vec![entry(0x2500, 0x2500, ClipboardReplacement::Codepoint(0x2D))]
        );

        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(m.parse_cli(Some("U+03A3=SUM")), Ok(()));
        assert_eq!(
            m.map,
            vec![entry(
                0x3A3,
                0x3A3,
                ClipboardReplacement::String("SUM".to_string())
            )]
        );

        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(m.parse_cli(Some("U+2500-U+2503=|")), Ok(()));
        assert_eq!(
            m.map,
            vec![entry(
                0x2500,
                0x2503,
                ClipboardReplacement::String("|".to_string())
            )]
        );

        // The map is not reset — repeated parses accumulate.
        let mut m = RepeatableClipboardCodepointMap::default();
        m.parse_cli(Some("U+2500=U+002D")).unwrap();
        m.parse_cli(Some("U+03A3=SUM")).unwrap();
        assert_eq!(m.map.len(), 2);

        // Whitespace around the key, `=`, and value is trimmed.
        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(m.parse_cli(Some(" U+2500 = U+002D ")), Ok(()));
        assert_eq!(
            m.map,
            vec![entry(0x2500, 0x2500, ClipboardReplacement::Codepoint(0x2D))]
        );

        // An empty value is a string replacement.
        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(m.parse_cli(Some("U+2500=")), Ok(()));
        assert_eq!(
            m.map,
            vec![entry(
                0x2500,
                0x2500,
                ClipboardReplacement::String(String::new())
            )]
        );

        // Errors.
        let mut m = RepeatableClipboardCodepointMap::default();
        assert_eq!(
            m.parse_cli(None),
            Err(ClipboardCodepointMapParseError::ValueRequired)
        );
        assert_eq!(
            m.parse_cli(Some("U+2500")), // no `=`
            Err(ClipboardCodepointMapParseError::InvalidValue)
        );
        assert_eq!(
            m.parse_cli(Some("X=A")), // bad range key
            Err(ClipboardCodepointMapParseError::InvalidValue)
        );
        assert_eq!(
            m.parse_cli(Some("U+2500=U+ZZ")), // bad codepoint replacement
            Err(ClipboardCodepointMapParseError::InvalidValue)
        );
        assert_eq!(
            m.parse_cli(Some("U+2500=U+")), // empty codepoint replacement
            Err(ClipboardCodepointMapParseError::InvalidValue)
        );

        // Replacement-codepoint parser edges (raw `parseInt(u21, _, 16)` path).
        let cp = |s: &str| {
            let mut m = RepeatableClipboardCodepointMap::default();
            m.parse_cli(Some(s)).map(|()| m.map[0].replacement.clone())
        };
        assert_eq!(
            cp("U+2500=U++2D"), // leading `+`
            Ok(ClipboardReplacement::Codepoint(0x2D))
        );
        assert_eq!(
            cp("U+2500=U+-0"), // unsigned `-0`
            Ok(ClipboardReplacement::Codepoint(0))
        );
        assert_eq!(
            cp("U+2500=U+2_D"), // interior underscore
            Ok(ClipboardReplacement::Codepoint(0x2D))
        );
        assert_eq!(
            cp("U+2500=U+200000"), // u21 overflow
            Err(ClipboardCodepointMapParseError::InvalidValue)
        );
    }

    #[test]
    fn parse_uint_consolidates_the_int_parsers() {
        use super::{parse_uint, IntParseError};

        // base 0: auto-detect the `0x`/`0o`/`0b` prefix, else decimal.
        assert_eq!(parse_uint("0xFF", 0, 0xFF), Ok(255));
        assert_eq!(parse_uint("0b101", 0, 0xFF), Ok(5));
        assert_eq!(parse_uint("0o17", 0, 0xFF), Ok(15));
        assert_eq!(parse_uint("42", 0, 0xFF), Ok(42));
        assert_eq!(parse_uint("0x", 0, 0xFF), Err(IntParseError::Invalid)); // bare prefix

        // Fixed base 10 / 16.
        assert_eq!(parse_uint("255", 10, u32::MAX as u64), Ok(255));
        assert_eq!(parse_uint("ff", 16, 0x1FFFFF), Ok(255));
        assert_eq!(
            parse_uint("0x10", 16, 0x1FFFFF),
            Err(IntParseError::Invalid)
        ); // 'x' not base-16

        // Signs and underscores.
        assert_eq!(parse_uint("+5", 10, 0xFF), Ok(5));
        assert_eq!(parse_uint("-0", 10, 0xFF), Ok(0));
        assert_eq!(parse_uint("-1", 10, 0xFF), Err(IntParseError::Overflow));
        assert_eq!(parse_uint("1_0", 10, 0xFF), Ok(10));
        assert_eq!(parse_uint("_0", 10, 0xFF), Err(IntParseError::Invalid));
        assert_eq!(parse_uint("0_", 10, 0xFF), Err(IntParseError::Invalid));

        // Overflow is distinct from invalid.
        assert_eq!(parse_uint("256", 0, 0xFF), Err(IntParseError::Overflow));
        assert_eq!(parse_uint("100", 16, 0xFF), Err(IntParseError::Overflow));
    }

    #[test]
    fn working_directory_parse_cli_parses_keywords_and_paths() {
        let parse = |s: Option<&str>| {
            let mut wd = WorkingDirectory::Inherit;
            wd.parse_cli(s).map(|()| wd)
        };

        // Upstream `parseCLI` cases (with a neutral path).
        assert_eq!(parse(Some("inherit")), Ok(WorkingDirectory::Inherit));
        assert_eq!(parse(Some("home")), Ok(WorkingDirectory::Home));
        assert_eq!(
            parse(Some("~/projects/app")),
            Ok(WorkingDirectory::Path("~/projects/app".to_string()))
        );
        // A quoted path has its surrounding quotes stripped.
        assert_eq!(
            parse(Some("\"/tmp path\"")),
            Ok(WorkingDirectory::Path("/tmp path".to_string()))
        );

        // Whitespace is trimmed; a plain path is a `Path`.
        assert_eq!(parse(Some(" home ")), Ok(WorkingDirectory::Home));
        assert_eq!(
            parse(Some("/usr/local")),
            Ok(WorkingDirectory::Path("/usr/local".to_string()))
        );

        // A missing or empty/all-whitespace value is `ValueRequired`.
        assert_eq!(parse(None), Err(WorkingDirectoryParseError::ValueRequired));
        assert_eq!(
            parse(Some("")),
            Err(WorkingDirectoryParseError::ValueRequired)
        );
        assert_eq!(
            parse(Some("   ")),
            Err(WorkingDirectoryParseError::ValueRequired)
        );

        // `value`: the path for `Path`, else `None`.
        assert_eq!(WorkingDirectory::Inherit.value(), None);
        assert_eq!(WorkingDirectory::Home.value(), None);
        assert_eq!(WorkingDirectory::Path("x".to_string()).value(), Some("x"));
    }

    #[test]
    fn background_blur_parse_cli_resolves_bool_glass_and_radius() {
        let parse = |s: Option<&str>| {
            let mut b = BackgroundBlur::False;
            b.parse_cli(s).map(|()| b)
        };

        // Upstream `parse BackgroundBlur` cases.
        assert_eq!(parse(None), Ok(BackgroundBlur::True)); // missing → True
        assert_eq!(parse(Some("true")), Ok(BackgroundBlur::True));
        assert_eq!(parse(Some("false")), Ok(BackgroundBlur::False));
        assert_eq!(parse(Some("42")), Ok(BackgroundBlur::Radius(42)));
        assert_eq!(
            parse(Some("macos-glass-regular")),
            Ok(BackgroundBlur::MacosGlassRegular)
        );
        assert_eq!(
            parse(Some("macos-glass-clear")),
            Ok(BackgroundBlur::MacosGlassClear)
        );
        assert_eq!(parse(Some("")), Err(BackgroundBlurParseError::InvalidValue));
        assert_eq!(
            parse(Some("aaaa")),
            Err(BackgroundBlurParseError::InvalidValue)
        );
        assert_eq!(
            parse(Some("420")), // overflows u8
            Err(BackgroundBlurParseError::InvalidValue)
        );

        // `parse_bool`-first order: "0"/"1" are booleans, not radii.
        assert_eq!(parse(Some("1")), Ok(BackgroundBlur::True));
        assert_eq!(parse(Some("0")), Ok(BackgroundBlur::False));

        // A real radius, including base-0.
        assert_eq!(parse(Some("5")), Ok(BackgroundBlur::Radius(5)));
        assert_eq!(parse(Some("0x10")), Ok(BackgroundBlur::Radius(16)));
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

    #[test]
    fn color_format_entry_writes_hex_string() {
        // Upstream `Color` `formatConfig`: `Color{10,11,12}` under name `a`.
        let mut out = String::new();
        let mut f = EntryFormatter::new("a", &mut out);
        Color {
            r: 10,
            g: 11,
            b: 12,
        }
        .format_entry(&mut f);
        assert_eq!(out, "a = #0a0b0c\n");
    }

    #[test]
    fn terminal_and_bold_color_format_entry() {
        let color = Color {
            r: 10,
            g: 11,
            b: 12,
        };
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        // TerminalColor: explicit color delegates; sentinels write their keyword.
        assert_eq!(
            fmt(&|f| TerminalColor::Color(color).format_entry(f)),
            "a = #0a0b0c\n"
        );
        assert_eq!(
            fmt(&|f| TerminalColor::CellForeground.format_entry(f)),
            "a = cell-foreground\n"
        );
        assert_eq!(
            fmt(&|f| TerminalColor::CellBackground.format_entry(f)),
            "a = cell-background\n"
        );

        // BoldColor: explicit color delegates; `Bright` writes its keyword.
        assert_eq!(
            fmt(&|f| BoldColor::Color(color).format_entry(f)),
            "a = #0a0b0c\n"
        );
        assert_eq!(fmt(&|f| BoldColor::Bright.format_entry(f)), "a = bright\n");
    }

    #[test]
    fn scalar_format_entries() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        // WorkingDirectory: keyword or path.
        assert_eq!(
            fmt(&|f| WorkingDirectory::Home.format_entry(f)),
            "a = home\n"
        );
        assert_eq!(
            fmt(&|f| WorkingDirectory::Inherit.format_entry(f)),
            "a = inherit\n"
        );
        assert_eq!(
            fmt(&|f| WorkingDirectory::Path("/x".to_string()).format_entry(f)),
            "a = /x\n"
        );

        // WindowPadding: one value when equal, else `left,right`.
        assert_eq!(
            fmt(&|f| WindowPadding {
                top_left: 5,
                bottom_right: 5
            }
            .format_entry(f)),
            "a = 5\n"
        );
        assert_eq!(
            fmt(&|f| WindowPadding {
                top_left: 3,
                bottom_right: 7
            }
            .format_entry(f)),
            "a = 3,7\n"
        );

        // BackgroundBlur: bool, radius int, or glass keyword.
        assert_eq!(
            fmt(&|f| BackgroundBlur::False.format_entry(f)),
            "a = false\n"
        );
        assert_eq!(fmt(&|f| BackgroundBlur::True.format_entry(f)), "a = true\n");
        assert_eq!(
            fmt(&|f| BackgroundBlur::Radius(42).format_entry(f)),
            "a = 42\n"
        );
        assert_eq!(
            fmt(&|f| BackgroundBlur::MacosGlassRegular.format_entry(f)),
            "a = macos-glass-regular\n"
        );
        assert_eq!(
            fmt(&|f| BackgroundBlur::MacosGlassClear.format_entry(f)),
            "a = macos-glass-clear\n"
        );
    }

    #[test]
    fn list_format_entries() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        // RepeatableString: empty → one empty entry; else one entry per item.
        let rs = |items: &[&str]| {
            let mut r = RepeatableString::default();
            for i in items {
                r.list.push(i.to_string());
            }
            r
        };
        assert_eq!(fmt(&|f| rs(&[]).format_entry(f)), "a = \n");
        assert_eq!(fmt(&|f| rs(&["x"]).format_entry(f)), "a = x\n");
        assert_eq!(fmt(&|f| rs(&["x", "y"]).format_entry(f)), "a = x\na = y\n");

        // ColorList: empty → one empty entry; else comma-joined `#rrggbb`.
        let cl = |colors: &[Color]| ColorList {
            colors: colors.to_vec(),
        };
        let black = Color { r: 0, g: 0, b: 0 };
        let white = Color {
            r: 255,
            g: 255,
            b: 255,
        };
        assert_eq!(fmt(&|f| cl(&[]).format_entry(f)), "a = \n");
        assert_eq!(fmt(&|f| cl(&[black]).format_entry(f)), "a = #000000\n");
        assert_eq!(
            fmt(&|f| cl(&[black, white]).format_entry(f)),
            "a = #000000,#ffffff\n"
        );
    }

    #[test]
    fn palette_format_entry_writes_all_256() {
        // A default palette: 256 lines, the first the default index-0 color.
        let mut out = String::new();
        Palette::default().format_entry(&mut EntryFormatter::new("a", &mut out));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 256);
        assert_eq!(lines[0], "a = 0=#1d1f21");

        // A set index 0 renders as that color.
        let mut p = Palette::default();
        p.value[0] = Rgb::new(0xAA, 0xBB, 0xCC);
        let mut out = String::new();
        p.format_entry(&mut EntryFormatter::new("a", &mut out));
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines.len(), 256);
        assert_eq!(lines[0], "a = 0=#aabbcc");
    }

    #[test]
    fn duration_format_entry_decomposes_units() {
        let fmt = |ns: u64| {
            let mut out = String::new();
            Duration { duration: ns }.format_entry(&mut EntryFormatter::new("a", &mut out));
            out
        };

        assert_eq!(fmt(1_000_000_000), "a = 1s\n");
        assert_eq!(fmt(500_000_000), "a = 500ms\n");
        assert_eq!(fmt(90_000_000_000), "a = 1m 30s\n");
        assert_eq!(fmt(1_000), "a = 1µs\n"); // µs preferred over us
        assert_eq!(fmt(3_600_000_000_000), "a = 1h\n");
        assert_eq!(fmt(0), "a = \n"); // empty span

        // A multi-segment value spanning several units.
        assert_eq!(fmt(93_784_000_000_000), "a = 1d 2h 3m 4s\n"); // 1d 2h 3m 4s

        // The upstream max-value case (the whole largest-first table).
        assert_eq!(
            fmt(u64::MAX),
            "a = 584y 49w 23h 34m 33s 709ms 551µs 615ns\n"
        );
    }

    #[test]
    fn selection_word_chars_format_entry_reencodes_codepoints() {
        let fmt = |cps: Vec<u32>| {
            let mut out = String::new();
            SelectionWordChars { codepoints: cps }
                .format_entry(&mut EntryFormatter::new("a", &mut out));
            out
        };

        // Round-trip: parse, then format back to the boundary chars (after the null).
        let mut sc = SelectionWordChars::default();
        sc.parse_cli(Some(" \t;,")).unwrap();
        assert_eq!(fmt(sc.codepoints), "a =  \t;,\n");

        // A multi-byte char is re-encoded.
        assert_eq!(fmt(vec![0, ';' as u32, ',' as u32, 0x2502]), "a = ;,│\n");

        // An un-encodable codepoint (a surrogate) is skipped.
        assert_eq!(fmt(vec![0, 'A' as u32, 0xD800, 'B' as u32]), "a = AB\n");

        // The null-only case formats to an empty entry.
        assert_eq!(fmt(vec![0]), "a = \n");

        // The 4096-byte cap: 4096 chars fit; the 4097th is dropped.
        let cap = |n: usize| {
            let mut cps = vec![0u32];
            cps.extend(std::iter::repeat_n('a' as u32, n));
            let out = fmt(cps);
            // The value sits between "a = " and the trailing "\n".
            out["a = ".len()..out.len() - 1].len()
        };
        assert_eq!(cap(4096), 4096);
        assert_eq!(cap(4097), 4096);
    }

    #[test]
    fn clipboard_codepoint_map_format_entry() {
        // Round-trips: parse, then format back.
        let rt = |input: &str| {
            let mut m = RepeatableClipboardCodepointMap::default();
            m.parse_cli(Some(input)).unwrap();
            let mut out = String::new();
            m.format_entry(&mut EntryFormatter::new("a", &mut out));
            out
        };
        assert_eq!(rt("U+2500=U+002D"), "a = U+2500=U+002D\n"); // codepoint replacement
        assert_eq!(rt("U+03A3=SUM"), "a = U+03A3=SUM\n"); // string replacement
        assert_eq!(rt("U+2500-U+2503=|"), "a = U+2500-U+2503=|\n"); // range key

        // The empty map writes one empty entry.
        let mut out = String::new();
        RepeatableClipboardCodepointMap::default()
            .format_entry(&mut EntryFormatter::new("a", &mut out));
        assert_eq!(out, "a = \n");

        // Accumulation: two entries → two lines.
        let mut m = RepeatableClipboardCodepointMap::default();
        m.parse_cli(Some("U+2500=U+002D")).unwrap();
        m.parse_cli(Some("U+03A3=SUM")).unwrap();
        let mut out = String::new();
        m.format_entry(&mut EntryFormatter::new("a", &mut out));
        assert_eq!(out, "a = U+2500=U+002D\na = U+03A3=SUM\n");
    }

    #[test]
    fn flag_struct_format_entries() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        // ShellIntegrationFeatures: the hyphenated keywords, in order.
        let sif = ShellIntegrationFeatures {
            cursor: true,
            sudo: false,
            title: true,
            ssh_env: false,
            ssh_terminfo: false,
            path: true,
        };
        assert_eq!(
            fmt(&|f| sif.format_entry(f)),
            "a = cursor,no-sudo,title,no-ssh-env,no-ssh-terminfo,path\n"
        );

        // ScrollToBottom.
        assert_eq!(
            fmt(&|f| ScrollToBottom {
                keystroke: true,
                output: false
            }
            .format_entry(f)),
            "a = keystroke,no-output\n"
        );
        assert_eq!(
            fmt(&|f| ScrollToBottom {
                keystroke: false,
                output: false
            }
            .format_entry(f)),
            "a = no-keystroke,no-output\n"
        );

        // NotifyOnCommandFinishAction.
        assert_eq!(
            fmt(&|f| NotifyOnCommandFinishAction {
                bell: true,
                notify: false
            }
            .format_entry(f)),
            "a = bell,no-notify\n"
        );
    }

    #[test]
    fn enum_format_entries() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        // CopyOnSelect.
        assert_eq!(fmt(&|f| CopyOnSelect::False.format_entry(f)), "a = false\n");
        assert_eq!(fmt(&|f| CopyOnSelect::True.format_entry(f)), "a = true\n");
        assert_eq!(
            fmt(&|f| CopyOnSelect::Clipboard.format_entry(f)),
            "a = clipboard\n"
        );

        // MiddleClickAction.
        assert_eq!(
            fmt(&|f| MiddleClickAction::PrimaryPaste.format_entry(f)),
            "a = primary-paste\n"
        );
        assert_eq!(
            fmt(&|f| MiddleClickAction::Ignore.format_entry(f)),
            "a = ignore\n"
        );

        // RightClickAction.
        assert_eq!(
            fmt(&|f| RightClickAction::Ignore.format_entry(f)),
            "a = ignore\n"
        );
        assert_eq!(
            fmt(&|f| RightClickAction::Paste.format_entry(f)),
            "a = paste\n"
        );
        assert_eq!(
            fmt(&|f| RightClickAction::Copy.format_entry(f)),
            "a = copy\n"
        );
        assert_eq!(
            fmt(&|f| RightClickAction::CopyOrPaste.format_entry(f)),
            "a = copy-or-paste\n"
        );
        assert_eq!(
            fmt(&|f| RightClickAction::ContextMenu.format_entry(f)),
            "a = context-menu\n"
        );

        // ShellIntegration.
        for (variant, kw) in [
            (ShellIntegration::None, "none"),
            (ShellIntegration::Detect, "detect"),
            (ShellIntegration::Bash, "bash"),
            (ShellIntegration::Elvish, "elvish"),
            (ShellIntegration::Fish, "fish"),
            (ShellIntegration::Nushell, "nushell"),
            (ShellIntegration::Zsh, "zsh"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn enum_format_entries_2() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (ClipboardAccess::Allow, "allow"),
            (ClipboardAccess::Deny, "deny"),
            (ClipboardAccess::Ask, "ask"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (NotifyOnCommandFinish::Never, "never"),
            (NotifyOnCommandFinish::Unfocused, "unfocused"),
            (NotifyOnCommandFinish::Always, "always"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (WindowColorspace::Srgb, "srgb"),
            (WindowColorspace::DisplayP3, "display-p3"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (AlphaBlending::Native, "native"),
            (AlphaBlending::Linear, "linear"),
            (AlphaBlending::LinearCorrected, "linear-corrected"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (GraphemeWidthMethod::Legacy, "legacy"),
            (GraphemeWidthMethod::Unicode, "unicode"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn enum_format_entries_mac() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (MacTitlebarStyle::Native, "native"),
            (MacTitlebarStyle::Transparent, "transparent"),
            (MacTitlebarStyle::Tabs, "tabs"),
            (MacTitlebarStyle::Hidden, "hidden"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (MacTitlebarProxyIcon::Visible, "visible"),
            (MacTitlebarProxyIcon::Hidden, "hidden"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (MacWindowButtons::Visible, "visible"),
            (MacWindowButtons::Hidden, "hidden"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [(MacHidden::Never, "never"), (MacHidden::Always, "always")] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn enum_format_entries_fullscreen() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (Fullscreen::False, "false"),
            (Fullscreen::True, "true"),
            (Fullscreen::NonNative, "non-native"),
            (Fullscreen::NonNativeVisibleMenu, "non-native-visible-menu"),
            (Fullscreen::NonNativePaddedNotch, "non-native-padded-notch"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (NonNativeFullscreen::False, "false"),
            (NonNativeFullscreen::True, "true"),
            (NonNativeFullscreen::VisibleMenu, "visible-menu"),
            (NonNativeFullscreen::PaddedNotch, "padded-notch"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn enum_format_entries_bgimage() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (BackgroundImageFit::Contain, "contain"),
            (BackgroundImageFit::Cover, "cover"),
            (BackgroundImageFit::Stretch, "stretch"),
            (BackgroundImageFit::None, "none"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (BackgroundImagePosition::TopLeft, "top-left"),
            (BackgroundImagePosition::TopCenter, "top-center"),
            (BackgroundImagePosition::TopRight, "top-right"),
            (BackgroundImagePosition::CenterLeft, "center-left"),
            (BackgroundImagePosition::CenterCenter, "center-center"),
            (BackgroundImagePosition::CenterRight, "center-right"),
            (BackgroundImagePosition::BottomLeft, "bottom-left"),
            (BackgroundImagePosition::BottomCenter, "bottom-center"),
            (BackgroundImagePosition::BottomRight, "bottom-right"),
            (BackgroundImagePosition::Center, "center"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn enum_format_entries_misc() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (OscColorReportFormat::None, "none"),
            (OscColorReportFormat::Bits8, "8-bit"),
            (OscColorReportFormat::Bits16, "16-bit"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (ConfirmCloseSurface::False, "false"),
            (ConfirmCloseSurface::True, "true"),
            (ConfirmCloseSurface::Always, "always"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (LinkPreviews::False, "false"),
            (LinkPreviews::True, "true"),
            (LinkPreviews::Osc8, "osc8"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (WindowSubtitle::False, "false"),
            (WindowSubtitle::WorkingDirectory, "working-directory"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (WindowPaddingColor::Background, "background"),
            (WindowPaddingColor::Extend, "extend"),
            (WindowPaddingColor::ExtendAlways, "extend-always"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn font_style_format_entry() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        assert_eq!(
            fmt(&|f| FontStyle::Default.format_entry(f)),
            "a = default\n"
        );
        assert_eq!(fmt(&|f| FontStyle::False.format_entry(f)), "a = false\n");
        assert_eq!(
            fmt(&|f| FontStyle::Name("bold".into()).format_entry(f)),
            "a = bold\n"
        );
    }

    #[test]
    fn font_style_parse_cli() {
        assert_eq!(
            FontStyle::parse_cli(None),
            Err(FontStyleParseError::ValueRequired)
        );
        assert_eq!(
            FontStyle::parse_cli(Some("default")),
            Ok(FontStyle::Default)
        );
        assert_eq!(FontStyle::parse_cli(Some("false")), Ok(FontStyle::False));
        assert_eq!(
            FontStyle::parse_cli(Some("bold")),
            Ok(FontStyle::Name("bold".to_string()))
        );
        // Any non-default/false value (including empty) is a named style; the
        // set-but-empty reset is a separate dispatch branch.
        assert_eq!(
            FontStyle::parse_cli(Some("")),
            Ok(FontStyle::Name(String::new()))
        );

        // Round-trip: parse_cli then format_entry recovers the formatted line for
        // each of the three formatted cases.
        for value in ["default", "false", "bold"] {
            let parsed = FontStyle::parse_cli(Some(value)).unwrap();
            let mut out = String::new();
            parsed.format_entry(&mut EntryFormatter::new("a", &mut out));
            assert_eq!(out, format!("a = {}\n", value));
        }
    }

    #[test]
    fn font_shaping_break_format_entry() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        assert_eq!(
            fmt(&|f| FontShapingBreak { cursor: true }.format_entry(f)),
            "a = cursor\n"
        );
        assert_eq!(
            fmt(&|f| FontShapingBreak { cursor: false }.format_entry(f)),
            "a = no-cursor\n"
        );
    }

    #[test]
    fn enum_format_entries_shader_mouse() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (CustomShaderAnimation::False, "false"),
            (CustomShaderAnimation::True, "true"),
            (CustomShaderAnimation::Always, "always"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
        for (variant, kw) in [
            (MouseShiftCapture::False, "false"),
            (MouseShiftCapture::True, "true"),
            (MouseShiftCapture::Always, "always"),
            (MouseShiftCapture::Never, "never"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn window_decoration_format_entry() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        for (variant, kw) in [
            (WindowDecoration::Auto, "auto"),
            (WindowDecoration::Client, "client"),
            (WindowDecoration::Server, "server"),
            (WindowDecoration::None, "none"),
        ] {
            assert_eq!(fmt(&|f| variant.format_entry(f)), format!("a = {}\n", kw));
        }
    }

    #[test]
    fn theme_format_entry() {
        let fmt = |v: &dyn Fn(&mut EntryFormatter)| {
            let mut out = String::new();
            let mut f = EntryFormatter::new("a", &mut out);
            v(&mut f);
            out
        };

        assert_eq!(
            fmt(&|f| Theme::single("foo".into()).format_entry(f)),
            "a = foo\n"
        );
        assert_eq!(
            fmt(&|f| Theme {
                light: "day".into(),
                dark: "night".into(),
            }
            .format_entry(f)),
            "a = light:day,dark:night\n"
        );
    }

    #[test]
    fn theme_parse_auto_struct() {
        let theme = |light: &str, dark: &str| Theme {
            light: light.to_string(),
            dark: dark.to_string(),
        };

        // A basic pair, and whitespace trimmed around keys and values.
        assert_eq!(
            Theme::parse_auto_struct("light:day,dark:night"),
            Ok(theme("day", "night"))
        );
        assert_eq!(
            Theme::parse_auto_struct(" light : day , dark : night "),
            Ok(theme("day", "night"))
        );
        // A quoted value protects a comma (the quotes are stripped).
        assert_eq!(
            Theme::parse_auto_struct("light:\"a,b\",dark:c"),
            Ok(theme("a,b", "c"))
        );
        // Setting a field again overwrites (later wins).
        assert_eq!(
            Theme::parse_auto_struct("light:a,light:b,dark:c"),
            Ok(theme("b", "c"))
        );
        // An empty value after the colon is an empty string.
        assert_eq!(
            Theme::parse_auto_struct("light:,dark:x"),
            Ok(theme("", "x"))
        );

        // Failures: a missing colon, an unknown key, a missing required field.
        assert_eq!(
            Theme::parse_auto_struct("light:day,nightonly"),
            Err(ThemeParseError::Invalid)
        );
        assert_eq!(
            Theme::parse_auto_struct("bright:x,dark:y"),
            Err(ThemeParseError::Invalid)
        );
        assert_eq!(
            Theme::parse_auto_struct("light:day"),
            Err(ThemeParseError::Invalid)
        );

        // Round-trip: a parsed pair formats back to `light:…,dark:…`.
        let parsed = Theme::parse_auto_struct("light:day,dark:night").unwrap();
        let mut out = String::new();
        parsed.format_entry(&mut EntryFormatter::new("theme", &mut out));
        assert_eq!(out, "theme = light:day,dark:night\n");
    }

    #[test]
    fn theme_parse_cli_single_and_pair() {
        let theme = |light: &str, dark: &str| Theme {
            light: light.to_string(),
            dark: dark.to_string(),
        };

        // A single name sets light = dark; whitespace is trimmed.
        assert_eq!(
            Theme::parse_cli(Some("catppuccin-mocha")),
            Ok(theme("catppuccin-mocha", "catppuccin-mocha"))
        );
        assert_eq!(
            Theme::parse_cli(Some("  solarized  ")),
            Ok(theme("solarized", "solarized"))
        );
        // A value with `,` / `:` is the light/dark pair form.
        assert_eq!(
            Theme::parse_cli(Some("light:day,dark:night")),
            Ok(theme("day", "night"))
        );
        // A `=` routes to the pair parser, which then fails (no `:` separator).
        assert_eq!(
            Theme::parse_cli(Some("light=day")),
            Err(ThemeParseError::Invalid)
        );
        // A missing or empty value is `ValueRequired`.
        assert_eq!(Theme::parse_cli(None), Err(ThemeParseError::ValueRequired));
        assert_eq!(
            Theme::parse_cli(Some("")),
            Err(ThemeParseError::ValueRequired)
        );
    }

    #[test]
    fn config_set_routes_theme_field() {
        let line = |cfg: &Config| -> String {
            let mut out = String::new();
            cfg.format_config(&mut out);
            out.lines()
                .find(|l| l.starts_with("theme = "))
                .unwrap()
                .to_string()
        };

        // A single name and a pair route to the `theme` field (via format_config).
        let mut cfg = Config::default();
        cfg.set("theme", Some("catppuccin-mocha")).unwrap();
        assert_eq!(line(&cfg), "theme = catppuccin-mocha");

        let mut cfg = Config::default();
        cfg.set("theme", Some("light:day,dark:night")).unwrap();
        assert_eq!(line(&cfg), "theme = light:day,dark:night");

        // `Some("")` resets to `None` (the void line).
        cfg.set("theme", Some("")).unwrap();
        assert_eq!(line(&cfg), "theme = ");

        // A missing value is `ValueRequired`; an invalid pair is `InvalidValue`.
        let mut cfg = Config::default();
        assert_eq!(cfg.set("theme", None), Err(ConfigSetError::ValueRequired));
        assert_eq!(
            cfg.set("theme", Some("bright:x,dark:y")),
            Err(ConfigSetError::InvalidValue)
        );
    }

    #[test]
    fn config_load_str_applies_lines_and_collects_diagnostics() {
        let has = |out: &str, key: &str, val: &str| {
            out.lines().any(|l| l == format!("{} = {}", key, val))
        };

        // A clean multi-line config (keys, a comment, a blank line, a quoted value)
        // applies every field with no diagnostics.
        let mut cfg = Config::default();
        let diags = cfg.load_str(
            "# a comment\n\
             fullscreen = non-native\n\
             \n\
             theme = \"catppuccin-mocha\"\n\
             background = #ff0000\n\
             background-image-repeat\n",
        );
        assert!(diags.is_empty());
        let mut out = String::new();
        cfg.format_config(&mut out);
        assert!(has(&out, "fullscreen", "non-native"));
        assert!(has(&out, "theme", "catppuccin-mocha")); // quotes stripped
        assert!(has(&out, "background", "#ff0000"));
        assert!(has(&out, "background-image-repeat", "true")); // bare key ⇒ true

        // A config with errors records a diagnostic per failing line (1-indexed,
        // counting the blank/comment lines) and still applies the good lines.
        let mut cfg = Config::default();
        let diags = cfg.load_str(
            "# header\n\
             copy-on-select = clipboard\n\
             \n\
             badkey = x\n\
             fullscreen = nope\n",
        );
        assert_eq!(
            diags,
            vec![
                ConfigDiagnostic {
                    line: 4,
                    key: "badkey".to_string(),
                    error: ConfigSetError::UnknownField,
                },
                ConfigDiagnostic {
                    line: 5,
                    key: "fullscreen".to_string(),
                    error: ConfigSetError::InvalidValue,
                },
            ]
        );
        // The good line still applied; the bad `fullscreen` kept its default.
        let mut out = String::new();
        cfg.format_config(&mut out);
        assert!(has(&out, "copy-on-select", "clipboard"));
        assert!(has(&out, "fullscreen", "false")); // the default
    }

    #[test]
    fn config_load_file_reads_and_skips_bom() {
        let has = |cfg: &Config, key: &str, val: &str| {
            let mut out = String::new();
            cfg.format_config(&mut out);
            out.lines().any(|l| l == format!("{} = {}", key, val))
        };

        let dir = std::env::temp_dir();
        let stamp = std::process::id();

        // A clean config file applies its fields with no diagnostics.
        let path = dir.join(format!("roastty-cfg-{stamp}-a.conf"));
        std::fs::write(
            &path,
            "fullscreen = non-native\nwindow-colorspace = display-p3\n",
        )
        .unwrap();
        let mut cfg = Config::default();
        let diags = cfg.load_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(diags.is_empty());
        assert!(has(&cfg, "fullscreen", "non-native"));
        assert!(has(&cfg, "window-colorspace", "display-p3"));

        // A file with a leading UTF-8 BOM still parses (the BOM is skipped).
        let path = dir.join(format!("roastty-cfg-{stamp}-b.conf"));
        std::fs::write(&path, b"\xEF\xBB\xBFmacos-hidden = always\n").unwrap();
        let mut cfg = Config::default();
        let diags = cfg.load_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert!(diags.is_empty());
        assert!(has(&cfg, "macos-hidden", "always"));

        // A file with a bad line yields the expected diagnostic.
        let path = dir.join(format!("roastty-cfg-{stamp}-c.conf"));
        std::fs::write(&path, "badkey = x\n").unwrap();
        let mut cfg = Config::default();
        let diags = cfg.load_file(&path).unwrap();
        std::fs::remove_file(&path).ok();
        assert_eq!(
            diags,
            vec![ConfigDiagnostic {
                line: 1,
                key: "badkey".to_string(),
                error: ConfigSetError::UnknownField,
            }]
        );

        // A nonexistent path is an `io::Error`.
        let path = dir.join(format!("roastty-cfg-{stamp}-does-not-exist.conf"));
        let mut cfg = Config::default();
        assert!(cfg.load_file(&path).is_err());
    }

    #[test]
    fn config_load_optional_file_three_way_action() {
        let dir = std::env::temp_dir();
        let stamp = std::process::id();

        // An existing file ⇒ `Loaded` with its diagnostics; the field applies.
        let path = dir.join(format!("roastty-opt-{stamp}.conf"));
        std::fs::write(&path, "macos-hidden = always\n").unwrap();
        let mut cfg = Config::default();
        let action = cfg.load_optional_file(&path);
        std::fs::remove_file(&path).ok();
        match action {
            OptionalFileAction::Loaded(diags) => assert!(diags.is_empty()),
            other => panic!("expected Loaded, got {other:?}"),
        }
        let mut out = String::new();
        cfg.format_config(&mut out);
        assert!(out.lines().any(|l| l == "macos-hidden = always"));

        // A nonexistent path ⇒ `NotFound`.
        let path = dir.join(format!("roastty-opt-{stamp}-missing.conf"));
        let mut cfg = Config::default();
        assert!(matches!(
            cfg.load_optional_file(&path),
            OptionalFileAction::NotFound
        ));

        // A directory (a non-`NotFound` IO error) ⇒ `Error`.
        let mut cfg = Config::default();
        assert!(matches!(
            cfg.load_optional_file(&dir),
            OptionalFileAction::Error(_)
        ));
    }

    #[test]
    fn config_set_cli_args_applies_and_collects_diagnostics() {
        let has = |cfg: &Config, key: &str, val: &str| {
            let mut out = String::new();
            cfg.format_config(&mut out);
            out.lines().any(|l| l == format!("{} = {}", key, val))
        };

        // A clean list of `--key=value` args (and a bare flag) applies every field
        // with no diagnostics.
        let mut cfg = Config::default();
        let diags = cfg.set_cli_args([
            "--fullscreen=non-native",
            "--theme=catppuccin-mocha",
            "--background-image-repeat", // bare flag ⇒ true
        ]);
        assert!(diags.is_empty());
        assert!(has(&cfg, "fullscreen", "non-native"));
        assert!(has(&cfg, "theme", "catppuccin-mocha"));
        assert!(has(&cfg, "background-image-repeat", "true"));

        // Errors record a diagnostic per failing arg (1-based position) and the good
        // args still apply. A non-flag arg is an invalid field; an unknown key and an
        // invalid value are field errors.
        let mut cfg = Config::default();
        let diags = cfg.set_cli_args([
            "--copy-on-select=clipboard", // arg 1, ok
            "not-a-flag",                 // arg 2, invalid field
            "--badkey=x",                 // arg 3, unknown field
            "--fullscreen=nope",          // arg 4, invalid value
        ]);
        assert_eq!(
            diags,
            vec![
                ConfigDiagnostic {
                    line: 2,
                    key: "not-a-flag".to_string(),
                    error: ConfigSetError::UnknownField,
                },
                ConfigDiagnostic {
                    line: 3,
                    key: "badkey".to_string(),
                    error: ConfigSetError::UnknownField,
                },
                ConfigDiagnostic {
                    line: 4,
                    key: "fullscreen".to_string(),
                    error: ConfigSetError::InvalidValue,
                },
            ]
        );
        // The good arg still applied; the bad `fullscreen` kept its default.
        assert!(has(&cfg, "copy-on-select", "clipboard"));
        assert!(has(&cfg, "fullscreen", "false"));
    }

    #[test]
    fn config_format_config_emits_fields_in_upstream_order() {
        let cfg = Config::default();
        let mut out = String::new();
        cfg.format_config(&mut out);

        // Every line is a `key = …` entry, and the keys appear in upstream
        // `Config` declaration order (omitting the float-blocked
        // `background-image-opacity`).
        let keys: Vec<&str> = out
            .lines()
            .map(|l| l.split(" = ").next().unwrap())
            .collect();
        assert_eq!(
            keys,
            vec![
                "font-style",
                "font-style-bold",
                "font-style-italic",
                "font-style-bold-italic",
                "font-shaping-break",
                "alpha-blending",
                "grapheme-width-method",
                "theme",
                "background",
                "foreground",
                "background-image-position",
                "background-image-fit",
                "background-image-repeat",
                "selection-foreground",
                "selection-background",
                "cursor-color",
                "cursor-text",
                "scroll-to-bottom",
                "mouse-shift-capture",
                "background-blur",
                "notify-on-command-finish",
                "notify-on-command-finish-action",
                "link-previews",
                "fullscreen",
                "window-padding-color",
                "window-subtitle",
                "window-colorspace",
                "clipboard-read",
                "clipboard-write",
                "copy-on-select",
                "right-click-action",
                "middle-click-action",
                "confirm-close-surface",
                "shell-integration",
                "shell-integration-features",
                "osc-color-report-format",
                "custom-shader-animation",
                "macos-non-native-fullscreen",
                "macos-window-buttons",
                "macos-titlebar-style",
                "macos-titlebar-proxy-icon",
                "macos-hidden",
                "bold-color",
            ]
        );

        // The float-blocked field is intentionally absent.
        assert!(!out.contains("background-image-opacity"));

        // The default optionals (all `None`) format as the void line, and `theme`
        // (default `None`) too.
        assert!(out.contains("cursor-color = \n"));
        assert!(out.contains("theme = \n"));
    }

    #[test]
    fn config_set_routes_enum_fields() {
        // Every enum key, set to a (mostly non-default) valid keyword, routes to
        // the right field — verified by reading it back through `format_config`.
        let cases = [
            ("copy-on-select", "clipboard"),
            ("clipboard-read", "deny"),
            ("clipboard-write", "deny"),
            ("mouse-shift-capture", "always"),
            ("right-click-action", "paste"),
            ("middle-click-action", "ignore"),
            ("shell-integration", "zsh"),
            ("notify-on-command-finish", "always"),
            ("window-colorspace", "display-p3"),
            ("alpha-blending", "linear"),
            ("window-padding-color", "extend"),
            ("background-image-position", "top-left"),
            ("background-image-fit", "stretch"),
            ("confirm-close-surface", "always"),
            ("link-previews", "osc8"),
            ("window-subtitle", "working-directory"),
            ("fullscreen", "non-native"),
            ("macos-non-native-fullscreen", "visible-menu"),
            ("macos-titlebar-style", "tabs"),
            ("macos-titlebar-proxy-icon", "hidden"),
            ("macos-window-buttons", "hidden"),
            ("macos-hidden", "always"),
            ("grapheme-width-method", "legacy"),
            ("osc-color-report-format", "8-bit"),
            ("custom-shader-animation", "always"),
        ];
        for (key, sample) in cases {
            let mut cfg = Config::default();
            cfg.set(key, Some(sample)).unwrap();
            let mut out = String::new();
            cfg.format_config(&mut out);
            let expected = format!("{} = {}", key, sample);
            assert!(
                out.lines().any(|l| l == expected.as_str()),
                "set({key:?}, Some({sample:?})) did not route to the matching field",
            );
        }

        // A missing value is `ValueRequired`; an invalid value is `InvalidValue`;
        // an unknown key is `UnknownField`.
        let mut cfg = Config::default();
        assert_eq!(
            cfg.set("fullscreen", None),
            Err(ConfigSetError::ValueRequired)
        );
        assert_eq!(
            cfg.set("fullscreen", Some("nope")),
            Err(ConfigSetError::InvalidValue)
        );
        assert_eq!(
            cfg.set("does-not-exist", Some("x")),
            Err(ConfigSetError::UnknownField)
        );

        // A set-but-empty value resets the field to its default.
        let mut cfg = Config::default();
        cfg.set("macos-hidden", Some("always")).unwrap(); // non-default
        cfg.set("macos-hidden", Some("")).unwrap(); // reset
        let mut out = String::new();
        cfg.format_config(&mut out);
        assert!(out.lines().any(|l| l == "macos-hidden = never")); // the default
    }

    #[test]
    fn config_set_routes_packed_and_bool_fields() {
        let line = |cfg: &Config, key: &str| -> String {
            let mut out = String::new();
            cfg.format_config(&mut out);
            out.lines()
                .find(|l| l.starts_with(&format!("{} = ", key)))
                .unwrap()
                .to_string()
        };

        // Packed structs: a `[no-]flag` comma-list and a standalone bool route to
        // the right field (verified via `format_config`).
        let mut cfg = Config::default();
        cfg.set("font-shaping-break", Some("no-cursor")).unwrap();
        assert_eq!(
            line(&cfg, "font-shaping-break"),
            "font-shaping-break = no-cursor"
        );

        let mut cfg = Config::default();
        cfg.set("scroll-to-bottom", Some("no-keystroke,output"))
            .unwrap();
        assert_eq!(
            line(&cfg, "scroll-to-bottom"),
            "scroll-to-bottom = no-keystroke,output"
        );

        let mut cfg = Config::default();
        cfg.set("shell-integration-features", Some("false"))
            .unwrap(); // standalone bool
        assert_eq!(
            line(&cfg, "shell-integration-features"),
            "shell-integration-features = no-cursor,no-sudo,no-title,no-ssh-env,no-ssh-terminfo,no-path"
        );

        let mut cfg = Config::default();
        cfg.set("notify-on-command-finish-action", Some("no-bell,notify"))
            .unwrap();
        assert_eq!(
            line(&cfg, "notify-on-command-finish-action"),
            "notify-on-command-finish-action = no-bell,notify"
        );

        // bool field: an explicit value, and a bare flag (None ⇒ true).
        let mut cfg = Config::default();
        cfg.set("background-image-repeat", Some("false")).unwrap();
        assert_eq!(
            line(&cfg, "background-image-repeat"),
            "background-image-repeat = false"
        );
        let mut cfg = Config::default();
        cfg.set("background-image-repeat", None).unwrap(); // bare flag ⇒ true
        assert_eq!(
            line(&cfg, "background-image-repeat"),
            "background-image-repeat = true"
        );

        // Asymmetry: a missing value is `ValueRequired` for a packed struct…
        let mut cfg = Config::default();
        assert_eq!(
            cfg.set("scroll-to-bottom", None),
            Err(ConfigSetError::ValueRequired)
        );
        // …but the bool's bare flag is `true` (handled above), and an invalid value
        // is `InvalidValue` for both.
        assert_eq!(
            cfg.set("scroll-to-bottom", Some("nope")),
            Err(ConfigSetError::InvalidValue)
        );
        assert_eq!(
            cfg.set("background-image-repeat", Some("nope")),
            Err(ConfigSetError::InvalidValue)
        );

        // `Some("")` resets a packed struct to its default.
        let mut cfg = Config::default();
        cfg.set("scroll-to-bottom", Some("no-keystroke,output"))
            .unwrap();
        cfg.set("scroll-to-bottom", Some("")).unwrap(); // reset
        assert_eq!(
            line(&cfg, "scroll-to-bottom"),
            "scroll-to-bottom = keystroke,no-output"
        );
    }

    #[test]
    fn config_set_routes_color_and_fontstyle_fields() {
        let line = |cfg: &Config, key: &str| -> String {
            let mut out = String::new();
            cfg.format_config(&mut out);
            out.lines()
                .find(|l| l.starts_with(&format!("{} = ", key)))
                .unwrap()
                .to_string()
        };

        // Non-optional Color: parse a hex value; reset to default on `Some("")`.
        let mut cfg = Config::default();
        cfg.set("background", Some("#ff0000")).unwrap();
        assert_eq!(line(&cfg, "background"), "background = #ff0000");
        cfg.set("foreground", Some("#ff0000")).unwrap();
        cfg.set("foreground", Some("")).unwrap(); // reset
        assert_eq!(line(&cfg, "foreground"), "foreground = #ffffff"); // the default

        // Optional TerminalColor: a hex value and a `cell-foreground` keyword wrap
        // in `Some`; `Some("")` resets to `None` (the void line).
        let mut cfg = Config::default();
        cfg.set("cursor-color", Some("#00ff00")).unwrap();
        assert_eq!(line(&cfg, "cursor-color"), "cursor-color = #00ff00");
        cfg.set("cursor-color", Some("cell-foreground")).unwrap();
        assert_eq!(line(&cfg, "cursor-color"), "cursor-color = cell-foreground");
        cfg.set("cursor-color", Some("")).unwrap(); // reset to None
        assert_eq!(line(&cfg, "cursor-color"), "cursor-color = ");

        // FontStyle: `default` / `false` / a named style.
        let mut cfg = Config::default();
        cfg.set("font-style", Some("bold")).unwrap();
        assert_eq!(line(&cfg, "font-style"), "font-style = bold");
        cfg.set("font-style-italic", Some("false")).unwrap();
        assert_eq!(line(&cfg, "font-style-italic"), "font-style-italic = false");

        // BackgroundBlur: an explicit bool, a bare flag (None ⇒ true), and a radius.
        let mut cfg = Config::default();
        cfg.set("background-blur", Some("true")).unwrap();
        assert_eq!(line(&cfg, "background-blur"), "background-blur = true");
        let mut cfg = Config::default();
        cfg.set("background-blur", None).unwrap(); // bare flag ⇒ true
        assert_eq!(line(&cfg, "background-blur"), "background-blur = true");
        let mut cfg = Config::default();
        cfg.set("background-blur", Some("64")).unwrap();
        assert_eq!(line(&cfg, "background-blur"), "background-blur = 64");

        // Errors: a Color needs a value (`ValueRequired`), an invalid value is
        // `InvalidValue`; `background-blur` accepts a bare flag but rejects garbage.
        let mut cfg = Config::default();
        assert_eq!(
            cfg.set("background", None),
            Err(ConfigSetError::ValueRequired)
        );
        assert_eq!(
            cfg.set("background", Some("notacolor")),
            Err(ConfigSetError::InvalidValue)
        );
        assert_eq!(
            cfg.set("background-blur", Some("xyz")),
            Err(ConfigSetError::InvalidValue)
        );
    }

    #[test]
    fn enum_from_keyword_round_trips_and_rejects_unknown() {
        // Each variant's keyword round-trips through `from_keyword`, and an
        // unknown string is `None` (upstream `std.meta.stringToEnum`).
        for v in [
            CopyOnSelect::False,
            CopyOnSelect::True,
            CopyOnSelect::Clipboard,
        ] {
            assert_eq!(CopyOnSelect::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(CopyOnSelect::from_keyword("nope"), None);
        // The bool-like tags only match as literal tag strings, not `1`/`t`.
        assert_eq!(CopyOnSelect::from_keyword("1"), None);
        assert_eq!(CopyOnSelect::from_keyword("t"), None);

        for v in [
            ClipboardAccess::Allow,
            ClipboardAccess::Deny,
            ClipboardAccess::Ask,
        ] {
            assert_eq!(ClipboardAccess::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(ClipboardAccess::from_keyword("nope"), None);

        for v in [
            RightClickAction::Ignore,
            RightClickAction::Paste,
            RightClickAction::Copy,
            RightClickAction::CopyOrPaste,
            RightClickAction::ContextMenu,
        ] {
            assert_eq!(RightClickAction::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(RightClickAction::from_keyword("nope"), None);

        for v in [MiddleClickAction::PrimaryPaste, MiddleClickAction::Ignore] {
            assert_eq!(MiddleClickAction::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MiddleClickAction::from_keyword("nope"), None);

        for v in [WindowColorspace::Srgb, WindowColorspace::DisplayP3] {
            assert_eq!(WindowColorspace::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(WindowColorspace::from_keyword("nope"), None);

        for v in [
            AlphaBlending::Native,
            AlphaBlending::Linear,
            AlphaBlending::LinearCorrected,
        ] {
            assert_eq!(AlphaBlending::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(AlphaBlending::from_keyword("nope"), None);

        for v in [GraphemeWidthMethod::Legacy, GraphemeWidthMethod::Unicode] {
            assert_eq!(GraphemeWidthMethod::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(GraphemeWidthMethod::from_keyword("nope"), None);
    }

    #[test]
    fn enum_from_keyword_round_trips_mac_bgimage_shader() {
        for v in [
            MacTitlebarStyle::Native,
            MacTitlebarStyle::Transparent,
            MacTitlebarStyle::Tabs,
            MacTitlebarStyle::Hidden,
        ] {
            assert_eq!(MacTitlebarStyle::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MacTitlebarStyle::from_keyword("nope"), None);

        for v in [MacTitlebarProxyIcon::Visible, MacTitlebarProxyIcon::Hidden] {
            assert_eq!(MacTitlebarProxyIcon::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MacTitlebarProxyIcon::from_keyword("nope"), None);

        for v in [MacWindowButtons::Visible, MacWindowButtons::Hidden] {
            assert_eq!(MacWindowButtons::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MacWindowButtons::from_keyword("nope"), None);

        for v in [MacHidden::Never, MacHidden::Always] {
            assert_eq!(MacHidden::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MacHidden::from_keyword("nope"), None);

        for v in [
            BackgroundImageFit::Contain,
            BackgroundImageFit::Cover,
            BackgroundImageFit::Stretch,
            BackgroundImageFit::None,
        ] {
            assert_eq!(BackgroundImageFit::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(BackgroundImageFit::from_keyword("nope"), None);

        for v in [
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
        ] {
            assert_eq!(BackgroundImagePosition::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(BackgroundImagePosition::from_keyword("nope"), None);

        for v in [
            CustomShaderAnimation::False,
            CustomShaderAnimation::True,
            CustomShaderAnimation::Always,
        ] {
            assert_eq!(CustomShaderAnimation::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(CustomShaderAnimation::from_keyword("nope"), None);
        assert_eq!(CustomShaderAnimation::from_keyword("1"), None);

        for v in [
            MouseShiftCapture::False,
            MouseShiftCapture::True,
            MouseShiftCapture::Always,
            MouseShiftCapture::Never,
        ] {
            assert_eq!(MouseShiftCapture::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(MouseShiftCapture::from_keyword("nope"), None);
        assert_eq!(MouseShiftCapture::from_keyword("t"), None);
    }

    #[test]
    fn enum_from_keyword_round_trips_misc_fullscreen() {
        for v in [
            OscColorReportFormat::None,
            OscColorReportFormat::Bits8,
            OscColorReportFormat::Bits16,
        ] {
            assert_eq!(OscColorReportFormat::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(OscColorReportFormat::from_keyword("nope"), None);

        for v in [
            ConfirmCloseSurface::False,
            ConfirmCloseSurface::True,
            ConfirmCloseSurface::Always,
        ] {
            assert_eq!(ConfirmCloseSurface::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(ConfirmCloseSurface::from_keyword("nope"), None);
        assert_eq!(ConfirmCloseSurface::from_keyword("1"), None);

        for v in [LinkPreviews::False, LinkPreviews::True, LinkPreviews::Osc8] {
            assert_eq!(LinkPreviews::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(LinkPreviews::from_keyword("nope"), None);

        for v in [WindowSubtitle::False, WindowSubtitle::WorkingDirectory] {
            assert_eq!(WindowSubtitle::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(WindowSubtitle::from_keyword("nope"), None);

        for v in [
            WindowPaddingColor::Background,
            WindowPaddingColor::Extend,
            WindowPaddingColor::ExtendAlways,
        ] {
            assert_eq!(WindowPaddingColor::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(WindowPaddingColor::from_keyword("nope"), None);

        for v in [
            Fullscreen::False,
            Fullscreen::True,
            Fullscreen::NonNative,
            Fullscreen::NonNativeVisibleMenu,
            Fullscreen::NonNativePaddedNotch,
        ] {
            assert_eq!(Fullscreen::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(Fullscreen::from_keyword("nope"), None);

        for v in [
            NonNativeFullscreen::False,
            NonNativeFullscreen::True,
            NonNativeFullscreen::VisibleMenu,
            NonNativeFullscreen::PaddedNotch,
        ] {
            assert_eq!(NonNativeFullscreen::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(NonNativeFullscreen::from_keyword("nope"), None);
    }

    #[test]
    fn enum_from_keyword_round_trips_shell_notify() {
        for v in [
            ShellIntegration::None,
            ShellIntegration::Detect,
            ShellIntegration::Bash,
            ShellIntegration::Elvish,
            ShellIntegration::Fish,
            ShellIntegration::Nushell,
            ShellIntegration::Zsh,
        ] {
            assert_eq!(ShellIntegration::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(ShellIntegration::from_keyword("nope"), None);

        for v in [
            NotifyOnCommandFinish::Never,
            NotifyOnCommandFinish::Unfocused,
            NotifyOnCommandFinish::Always,
        ] {
            assert_eq!(NotifyOnCommandFinish::from_keyword(v.keyword()), Some(v));
        }
        assert_eq!(NotifyOnCommandFinish::from_keyword("nope"), None);
    }

    #[test]
    fn packed_flags_parse_cli() {
        // Standalone bools set every flag.
        assert_eq!(
            ScrollToBottom::parse_cli("true"),
            Ok(ScrollToBottom {
                keystroke: true,
                output: true,
            })
        );
        assert_eq!(
            ScrollToBottom::parse_cli("false"),
            Ok(ScrollToBottom {
                keystroke: false,
                output: false,
            })
        );
        assert_eq!(
            ScrollToBottom::parse_cli("1"),
            Ok(ScrollToBottom {
                keystroke: true,
                output: true,
            })
        );
        assert_eq!(
            ScrollToBottom::parse_cli("0"),
            Ok(ScrollToBottom {
                keystroke: false,
                output: false,
            })
        );

        // A `[no-]flag` comma-list sets the named flags; the rest keep defaults
        // (`keystroke = true`, `output = false`).
        assert_eq!(
            ScrollToBottom::parse_cli("output"),
            Ok(ScrollToBottom {
                keystroke: true,
                output: true,
            })
        );
        assert_eq!(
            ScrollToBottom::parse_cli("no-keystroke,output"),
            Ok(ScrollToBottom {
                keystroke: false,
                output: true,
            })
        );
        assert_eq!(
            ScrollToBottom::parse_cli(" keystroke , no-output "),
            Ok(ScrollToBottom {
                keystroke: true,
                output: false,
            })
        );

        // Unknown flag → InvalidValue.
        assert_eq!(
            ScrollToBottom::parse_cli("nope"),
            Err(FlagsParseError::InvalidValue)
        );

        // FontShapingBreak (single `cursor` flag; default `true`).
        assert_eq!(
            FontShapingBreak::parse_cli("no-cursor"),
            Ok(FontShapingBreak { cursor: false })
        );
        assert_eq!(
            FontShapingBreak::parse_cli("cursor"),
            Ok(FontShapingBreak { cursor: true })
        );
        assert_eq!(
            FontShapingBreak::parse_cli("false"),
            Ok(FontShapingBreak { cursor: false })
        );
        assert_eq!(
            FontShapingBreak::parse_cli("nope"),
            Err(FlagsParseError::InvalidValue)
        );

        // Round-trip: format_entry then parse_cli recovers the value.
        let original = ScrollToBottom {
            keystroke: false,
            output: true,
        };
        let mut out = String::new();
        original.format_entry(&mut EntryFormatter::new("scroll-to-bottom", &mut out));
        let rendered = out.trim_end().split(" = ").nth(1).unwrap().to_string();
        assert_eq!(ScrollToBottom::parse_cli(&rendered), Ok(original));
    }

    #[test]
    fn packed_flags_parse_cli_shell_notify() {
        // Standalone bool sets every flag.
        assert_eq!(
            ShellIntegrationFeatures::parse_cli("false"),
            Ok(ShellIntegrationFeatures {
                cursor: false,
                sudo: false,
                title: false,
                ssh_env: false,
                ssh_terminfo: false,
                path: false,
            })
        );

        // A `[no-]flag` comma-list, incl. the kebab `ssh-env` / `ssh-terminfo`
        // keywords → snake fields; the rest keep defaults (cursor/title/path true,
        // sudo false).
        assert_eq!(
            ShellIntegrationFeatures::parse_cli("ssh-env,ssh-terminfo,no-title"),
            Ok(ShellIntegrationFeatures {
                cursor: true,
                sudo: false,
                title: false,
                ssh_env: true,
                ssh_terminfo: true,
                path: true,
            })
        );

        // Unknown flag → InvalidValue (incl. the snake form, which is not a keyword).
        assert_eq!(
            ShellIntegrationFeatures::parse_cli("ssh_env"),
            Err(FlagsParseError::InvalidValue)
        );
        assert_eq!(
            ShellIntegrationFeatures::parse_cli("nope"),
            Err(FlagsParseError::InvalidValue)
        );

        // NotifyOnCommandFinishAction (bell default true, notify default false).
        assert_eq!(
            NotifyOnCommandFinishAction::parse_cli("no-bell,notify"),
            Ok(NotifyOnCommandFinishAction {
                bell: false,
                notify: true,
            })
        );
        assert_eq!(
            NotifyOnCommandFinishAction::parse_cli("true"),
            Ok(NotifyOnCommandFinishAction {
                bell: true,
                notify: true,
            })
        );
        assert_eq!(
            NotifyOnCommandFinishAction::parse_cli("nope"),
            Err(FlagsParseError::InvalidValue)
        );

        // Round-trip: format_entry then parse_cli recovers the value.
        let original = ShellIntegrationFeatures {
            cursor: false,
            sudo: true,
            title: false,
            ssh_env: true,
            ssh_terminfo: false,
            path: true,
        };
        let mut out = String::new();
        original.format_entry(&mut EntryFormatter::new(
            "shell-integration-features",
            &mut out,
        ));
        let rendered = out.trim_end().split(" = ").nth(1).unwrap().to_string();
        assert_eq!(ShellIntegrationFeatures::parse_cli(&rendered), Ok(original));
    }

    #[test]
    fn parse_bool_field_and_string_field() {
        // bool: a bare flag (no value) is `true`.
        assert_eq!(parse_bool_field(None), Ok(true));
        // Recognized values (upstream `parseBool`).
        assert_eq!(parse_bool_field(Some("1")), Ok(true));
        assert_eq!(parse_bool_field(Some("t")), Ok(true));
        assert_eq!(parse_bool_field(Some("T")), Ok(true));
        assert_eq!(parse_bool_field(Some("true")), Ok(true));
        assert_eq!(parse_bool_field(Some("0")), Ok(false));
        assert_eq!(parse_bool_field(Some("f")), Ok(false));
        assert_eq!(parse_bool_field(Some("F")), Ok(false));
        assert_eq!(parse_bool_field(Some("false")), Ok(false));
        // Unrecognized value → InvalidValue. The set-but-empty `""` reset is a
        // separate dispatch branch, so in isolation `Some("")` is InvalidValue.
        assert_eq!(
            parse_bool_field(Some("x")),
            Err(MagicParseError::InvalidValue)
        );
        assert_eq!(
            parse_bool_field(Some("")),
            Err(MagicParseError::InvalidValue)
        );

        // string: a missing value is `ValueRequired`; otherwise an owned copy.
        assert_eq!(
            parse_string_field(None),
            Err(MagicParseError::ValueRequired)
        );
        assert_eq!(parse_string_field(Some("hi")), Ok("hi".to_string()));
        // The set-but-empty `""` reset is a separate dispatch branch, so in
        // isolation `Some("")` is a copy of the empty string.
        assert_eq!(parse_string_field(Some("")), Ok(String::new()));
    }
}
