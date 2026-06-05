//! ANSI / VT constants (port of upstream `terminal/ansi`).

/// C0 (7-bit) ANSI control characters (upstream `terminal.ansi.C0`). Only the
/// control codes the terminal handles are named.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum C0 {
    /// Null.
    Nul = 0x00,
    /// Start of heading.
    Soh = 0x01,
    /// Start of text.
    Stx = 0x02,
    /// Enquiry.
    Enq = 0x05,
    /// Bell.
    Bel = 0x07,
    /// Backspace.
    Bs = 0x08,
    /// Horizontal tab.
    Ht = 0x09,
    /// Line feed.
    Lf = 0x0A,
    /// Vertical tab.
    Vt = 0x0B,
    /// Form feed.
    Ff = 0x0C,
    /// Carriage return.
    Cr = 0x0D,
    /// Shift out.
    So = 0x0E,
    /// Shift in.
    Si = 0x0F,
}

impl C0 {
    /// The byte value of this control code.
    pub(crate) fn value(self) -> u8 {
        self as u8
    }

    /// The named C0 control code for a byte, or `None` for an unrecognized byte
    /// (upstream's non-exhaustive `@enumFromInt`: a parser matches the named codes and
    /// treats the rest as "not a recognized C0").
    pub(crate) fn from_byte(byte: u8) -> Option<C0> {
        Some(match byte {
            0x00 => C0::Nul,
            0x01 => C0::Soh,
            0x02 => C0::Stx,
            0x05 => C0::Enq,
            0x07 => C0::Bel,
            0x08 => C0::Bs,
            0x09 => C0::Ht,
            0x0A => C0::Lf,
            0x0B => C0::Vt,
            0x0C => C0::Ff,
            0x0D => C0::Cr,
            0x0E => C0::So,
            0x0F => C0::Si,
            _ => return None,
        })
    }
}

/// The SGR rendition aspects that can be set (upstream `terminal.ansi.RenditionAspect`).
/// The value is the SGR (`ESC [ m`) parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum RenditionAspect {
    /// Reset to default.
    Default = 0,
    /// Bold.
    Bold = 1,
    /// Default foreground color.
    DefaultFg = 39,
    /// Default background color.
    DefaultBg = 49,
}

impl RenditionAspect {
    /// The SGR parameter value of this aspect.
    pub(crate) fn value(self) -> u16 {
        self as u16
    }

    /// The named rendition aspect for an SGR parameter, or `None` for an unrecognized
    /// value (upstream's non-exhaustive `@enumFromInt`).
    pub(crate) fn from_value(value: u16) -> Option<RenditionAspect> {
        Some(match value {
            0 => RenditionAspect::Default,
            1 => RenditionAspect::Bold,
            39 => RenditionAspect::DefaultFg,
            49 => RenditionAspect::DefaultBg,
            _ => return None,
        })
    }
}

/// The status line type for DECSSDT (upstream `terminal.ansi.StatusLineType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum StatusLineType {
    /// No status line.
    None = 0,
    /// Indicator status line.
    Indicator = 1,
    /// Host-writable status line.
    HostWritable = 2,
}

impl StatusLineType {
    /// The DECSSDT parameter value of this type.
    pub(crate) fn value(self) -> u16 {
        self as u16
    }

    /// The named status line type for a DECSSDT parameter, or `None` for an
    /// unrecognized value (upstream's non-exhaustive `@enumFromInt`).
    pub(crate) fn from_value(value: u16) -> Option<StatusLineType> {
        Some(match value {
            0 => StatusLineType::None,
            1 => StatusLineType::Indicator,
            2 => StatusLineType::HostWritable,
            _ => return None,
        })
    }
}

/// Possible cursor styles set via `ESC [ q` (upstream `terminal.ansi.CursorStyle`).
/// Used by name; the `ESC [ q` parameter → style mapping lives in the parser, and the
/// upstream `lib.Enum` value is not a stable wire value, so no numeric value is exposed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CursorStyle {
    /// Default cursor style.
    Default,
    /// Blinking block.
    BlinkingBlock,
    /// Steady block.
    SteadyBlock,
    /// Blinking underline.
    BlinkingUnderline,
    /// Steady underline.
    SteadyUnderline,
    /// Blinking bar.
    BlinkingBar,
    /// Steady bar.
    SteadyBar,
}

/// The display to target for status updates via DECSASD (upstream
/// `terminal.ansi.StatusDisplay`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StatusDisplay {
    /// The main display.
    Main,
    /// The status line.
    StatusLine,
}

/// The modify-key format for `ESC [ > a;b m` (upstream `terminal.ansi.ModifyKeyFormat`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModifyKeyFormat {
    /// Legacy.
    Legacy,
    /// Cursor keys.
    CursorKeys,
    /// Function keys.
    FunctionKeys,
    /// Other keys: none.
    OtherKeysNone,
    /// Other keys: numeric except.
    OtherKeysNumericExcept,
    /// Other keys: numeric.
    OtherKeysNumeric,
}

/// The terminal protection modes set via DECSCA and `ESC V, W` (upstream
/// `terminal.ansi.ProtectedMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtectedMode {
    /// No protection.
    Off,
    /// ISO protection (`ESC V`, `ESC W`).
    Iso,
    /// DEC protection (`CSI Ps " q`).
    Dec,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c0_round_trips_and_rejects_unknown() {
        // Each named control code round-trips through its byte value.
        for c in [
            C0::Nul,
            C0::Soh,
            C0::Stx,
            C0::Enq,
            C0::Bel,
            C0::Bs,
            C0::Ht,
            C0::Lf,
            C0::Vt,
            C0::Ff,
            C0::Cr,
            C0::So,
            C0::Si,
        ] {
            assert_eq!(C0::from_byte(c.value()), Some(c));
        }

        // Exact byte values.
        assert_eq!(C0::Nul.value(), 0x00);
        assert_eq!(C0::Bel.value(), 0x07);
        assert_eq!(C0::Lf.value(), 0x0A);
        assert_eq!(C0::Cr.value(), 0x0D);
        assert_eq!(C0::Si.value(), 0x0F);

        // Unrecognized bytes are `None` (upstream's non-exhaustive `_`).
        assert_eq!(C0::from_byte(0x03), None); // ETX (not handled)
        assert_eq!(C0::from_byte(0x04), None); // EOT
        assert_eq!(C0::from_byte(0x20), None); // space
        assert_eq!(C0::from_byte(0x7F), None); // DEL
    }

    #[test]
    fn rendition_aspect_round_trips_and_rejects_unknown() {
        for a in [
            RenditionAspect::Default,
            RenditionAspect::Bold,
            RenditionAspect::DefaultFg,
            RenditionAspect::DefaultBg,
        ] {
            assert_eq!(RenditionAspect::from_value(a.value()), Some(a));
        }
        // Exact SGR parameter values.
        assert_eq!(RenditionAspect::Default.value(), 0);
        assert_eq!(RenditionAspect::Bold.value(), 1);
        assert_eq!(RenditionAspect::DefaultFg.value(), 39);
        assert_eq!(RenditionAspect::DefaultBg.value(), 49);
        // Unrecognized SGR parameters are `None`.
        assert_eq!(RenditionAspect::from_value(7), None);
        assert_eq!(RenditionAspect::from_value(38), None);
    }

    #[test]
    fn status_line_type_round_trips_and_rejects_unknown() {
        for t in [
            StatusLineType::None,
            StatusLineType::Indicator,
            StatusLineType::HostWritable,
        ] {
            assert_eq!(StatusLineType::from_value(t.value()), Some(t));
        }
        assert_eq!(StatusLineType::HostWritable.value(), 2);
        assert_eq!(StatusLineType::from_value(3), None);
    }

    #[test]
    fn ansi_mode_enums() {
        // The four mode enums have exactly the upstream variant sets (used by name).
        let cursor = [
            CursorStyle::Default,
            CursorStyle::BlinkingBlock,
            CursorStyle::SteadyBlock,
            CursorStyle::BlinkingUnderline,
            CursorStyle::SteadyUnderline,
            CursorStyle::BlinkingBar,
            CursorStyle::SteadyBar,
        ];
        assert_eq!(cursor.len(), 7);
        // Each variant is distinct from the others.
        for (i, a) in cursor.iter().enumerate() {
            for (j, b) in cursor.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }

        let display = [StatusDisplay::Main, StatusDisplay::StatusLine];
        assert_eq!(display.len(), 2);
        assert_ne!(StatusDisplay::Main, StatusDisplay::StatusLine);

        let modify = [
            ModifyKeyFormat::Legacy,
            ModifyKeyFormat::CursorKeys,
            ModifyKeyFormat::FunctionKeys,
            ModifyKeyFormat::OtherKeysNone,
            ModifyKeyFormat::OtherKeysNumericExcept,
            ModifyKeyFormat::OtherKeysNumeric,
        ];
        assert_eq!(modify.len(), 6);
        for (i, a) in modify.iter().enumerate() {
            for (j, b) in modify.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }

        let protected = [ProtectedMode::Off, ProtectedMode::Iso, ProtectedMode::Dec];
        assert_eq!(protected.len(), 3);
        assert_ne!(ProtectedMode::Off, ProtectedMode::Iso);
        assert_ne!(ProtectedMode::Iso, ProtectedMode::Dec);
        assert_ne!(ProtectedMode::Off, ProtectedMode::Dec);
    }
}
