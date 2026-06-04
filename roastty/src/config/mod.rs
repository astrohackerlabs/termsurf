//! Configuration types.
//!
//! The minimal entry point of the config layer: the leaf config types consumed
//! by roastty subsystems (renderer, font, terminal, input, clipboard). The
//! broader config subsystem (parsing, the full `Config` struct, the rest of the
//! config keys) is ported in later slices.
#![allow(dead_code)]
// This config layer is consumed by later slices.

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
        AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CopyOnSelect,
        CustomShaderAnimation, FontShapingBreak, FontStyle, GraphemeWidthMethod, MiddleClickAction,
        MouseShiftCapture, OscColorReportFormat, RightClickAction,
    };

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
}
