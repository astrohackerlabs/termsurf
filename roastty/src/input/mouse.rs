//! Mouse input types (port of upstream `input/mouse`).

/// The type of action associated with a mouse event (upstream `input.mouse.Action`).
/// Backed by `c_int` for the embedding API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Action {
    Press,
    Release,
    Motion,
}

/// The state of a mouse button (upstream `input.mouse.ButtonState`). Backed by `c_int`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum ButtonState {
    Release,
    Press,
}

/// Possible mouse buttons; we track up to 11 because that's the maximum button input
/// terminal mouse tracking handles without becoming ambiguous (upstream
/// `input.mouse.Button`). Backed by `c_int`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Button {
    Unknown = 0,
    Left = 1,
    Right = 2,
    Middle = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Eleven = 11,
}

impl Button {
    /// The maximum value in this enum (upstream `Button.max`), e.g. to size a densely
    /// packed array.
    pub(crate) const MAX: i32 = 11;
}

/// The "momentum" of a mouse scroll event (upstream `input.mouse.Momentum`), matching the
/// macOS `NSEventPhase` used for inertial scrolling (i.e. flicking).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Momentum {
    #[default]
    None = 0,
    Began = 1,
    Stationary = 2,
    Changed = 3,
    Ended = 4,
    Cancelled = 5,
    MayBegin = 6,
}

/// The pressure stage of a pressure-sensitive input device (upstream
/// `input.mouse.PressureStage`); this currently only supports the stages macOS supports.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum PressureStage {
    /// The input device is unpressed.
    #[default]
    None = 0,
    /// The input device is pressed a normal amount (a "click" on a macOS trackpad).
    Normal = 1,
    /// The input device is pressed a deep amount (a "force click" on a macOS trackpad).
    Deep = 2,
}

/// The modifier bitmask for scroll events (upstream `input.mouse.ScrollMods`, a
/// `packed struct(u8)`).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ScrollMods {
    /// True for a high-precision scroll event (Apple Magic Mouse, trackpads, etc., which
    /// send very detailed scroll events).
    pub(crate) precision: bool,
    /// The momentum phase of the scroll event, used to handle inertial scrolling.
    pub(crate) momentum: Momentum,
}

impl ScrollMods {
    /// The `u8` bit-encoding of this mask, mirroring upstream's `packed struct(u8)`:
    /// bit 0 = `precision`, bits 1-3 = `momentum`, bits 4-7 = padding (0).
    pub(crate) fn int(self) -> u8 {
        (self.precision as u8) | ((self.momentum as u8) << 1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scroll_mods_int_matches_packed_layout() {
        // Default is all-zero (upstream's bitcast test).
        assert_eq!(ScrollMods::default().int(), 0b0);
        // Precision is bit 0 (upstream's bitcast test).
        assert_eq!(
            ScrollMods {
                precision: true,
                momentum: Momentum::None,
            }
            .int(),
            0b0000_0001,
        );
        // Momentum occupies bits 1-3.
        assert_eq!(
            ScrollMods {
                precision: false,
                momentum: Momentum::Began,
            }
            .int(),
            0b0000_0010,
        );
        // MayBegin (6 = 0b110) shifted into bits 1-3 is 0b1100, plus precision in bit 0.
        assert_eq!(
            ScrollMods {
                precision: true,
                momentum: Momentum::MayBegin,
            }
            .int(),
            0b0000_1101,
        );
    }

    #[test]
    fn mouse_enum_discriminants_match_upstream() {
        assert_eq!(Action::Press as i32, 0);
        assert_eq!(Action::Motion as i32, 2);
        assert_eq!(ButtonState::Release as i32, 0);
        assert_eq!(ButtonState::Press as i32, 1);
        assert_eq!(Button::Unknown as i32, 0);
        assert_eq!(Button::Eleven as i32, 11);
        assert_eq!(Button::MAX, 11);
        assert_eq!(Momentum::None as u8, 0);
        assert_eq!(Momentum::MayBegin as u8, 6);
        assert_eq!(PressureStage::None as u8, 0);
        assert_eq!(PressureStage::Deep as u8, 2);
    }
}
