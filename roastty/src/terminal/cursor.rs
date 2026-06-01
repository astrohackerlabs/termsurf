//! Cursor visual style state.

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum VisualStyle {
    Bar,
    #[default]
    Block,
    Underline,
    BlockHollow,
}

impl VisualStyle {
    pub(super) const fn decscusr_report(self, blinking: bool) -> u8 {
        match self {
            Self::Block | Self::BlockHollow => {
                if blinking {
                    1
                } else {
                    2
                }
            }
            Self::Underline => {
                if blinking {
                    3
                } else {
                    4
                }
            }
            Self::Bar => {
                if blinking {
                    5
                } else {
                    6
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_visual_default_is_block() {
        assert_eq!(VisualStyle::default(), VisualStyle::Block);
    }

    #[test]
    fn cursor_visual_decscusr_report_mapping() {
        assert_eq!(VisualStyle::Block.decscusr_report(true), 1);
        assert_eq!(VisualStyle::Block.decscusr_report(false), 2);
        assert_eq!(VisualStyle::BlockHollow.decscusr_report(true), 1);
        assert_eq!(VisualStyle::BlockHollow.decscusr_report(false), 2);
        assert_eq!(VisualStyle::Underline.decscusr_report(true), 3);
        assert_eq!(VisualStyle::Underline.decscusr_report(false), 4);
        assert_eq!(VisualStyle::Bar.decscusr_report(true), 5);
        assert_eq!(VisualStyle::Bar.decscusr_report(false), 6);
    }
}
