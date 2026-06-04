//! Configuration types.
//!
//! The minimal entry point of the config layer: the leaf config types consumed
//! by roastty subsystems (renderer, font, terminal, input, clipboard). The
//! broader config subsystem (parsing, the full `Config` struct, the rest of the
//! config keys) is ported in later slices.
#![allow(dead_code)]
// This config layer is consumed by later slices.

use crate::terminal::color::Rgb;
use crate::terminal::style::BoldColor as TerminalBoldColor;

/// A config color value (upstream `Config.Color`): an RGB byte triple. The string
/// parsing (named colors / hex) and the C extern struct are ported in later
/// slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Color {
    /// Convert to the terminal-native `Rgb` (upstream `Color.toTerminalRGB`): a
    /// field-for-field copy of the three channels.
    pub(crate) fn to_terminal_rgb(self) -> Rgb {
        Rgb::new(self.r, self.g, self.b)
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
        Color, ConfirmCloseSurface, CopyOnSelect, CustomShaderAnimation, FontShapingBreak,
        FontStyle, Fullscreen, GraphemeWidthMethod, LinkPreviews, MacTitlebarProxyIcon,
        MacTitlebarStyle, MiddleClickAction, MouseShiftCapture, NonNativeFullscreen,
        NotifyOnCommandFinish, NotifyOnCommandFinishAction, OscColorReportFormat, RightClickAction,
        ScrollToBottom, ShellIntegration, ShellIntegrationFeatures, TerminalBoldColor,
        TerminalColor, WindowSubtitle,
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
}
