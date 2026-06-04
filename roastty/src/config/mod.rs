//! Configuration types.
//!
//! The minimal entry point of the config layer: the leaf enums the renderer
//! consumes. The broader config subsystem (parsing, the full `Config` struct,
//! the rest of the config keys) is ported in later slices.
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

#[cfg(test)]
mod tests {
    use super::{AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition};

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
}
