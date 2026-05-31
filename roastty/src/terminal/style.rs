use std::fmt;

use super::color::{Palette, Rgb};
use super::sgr::Underline;
use super::size::StyleCountInt;

pub(super) type Id = StyleCountInt;
pub(super) const DEFAULT_ID: Id = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Style {
    pub(super) fg_color: Color,
    pub(super) bg_color: Color,
    pub(super) underline_color: Color,
    pub(super) flags: Flags,
}

impl Style {
    pub(super) const fn default_style() -> Self {
        Self {
            fg_color: Color::None,
            bg_color: Color::None,
            underline_color: Color::None,
            flags: Flags::default_flags(),
        }
    }

    pub(super) fn is_default(self) -> bool {
        self == Self::default()
    }

    pub(super) fn fg(self, opts: Fg<'_>) -> Rgb {
        match self.fg_color {
            Color::None => {
                if self.flags.bold {
                    if let Some(BoldColor::Color(color)) = opts.bold {
                        return color;
                    }
                }

                opts.default
            }
            Color::Palette(idx) => {
                if self.flags.bold && opts.bold.is_some() {
                    let bright_offset = 8;
                    if idx < bright_offset {
                        return opts.palette[(idx + bright_offset) as usize];
                    }
                }

                opts.palette[idx as usize]
            }
            Color::Rgb(rgb) => {
                if self.flags.bold && rgb == opts.default {
                    if let Some(BoldColor::Color(color)) = opts.bold {
                        return color;
                    }
                }

                rgb
            }
        }
    }

    pub(super) fn bg_color(self, palette: &Palette) -> Option<Rgb> {
        match self.bg_color {
            Color::None => None,
            Color::Palette(idx) => Some(palette[idx as usize]),
            Color::Rgb(rgb) => Some(rgb),
        }
    }

    pub(super) fn underline_color(self, palette: &Palette) -> Option<Rgb> {
        match self.underline_color {
            Color::None => None,
            Color::Palette(idx) => Some(palette[idx as usize]),
            Color::Rgb(rgb) => Some(rgb),
        }
    }

    pub(super) fn formatter_vt(&self) -> VtFormatter<'_> {
        VtFormatter {
            style: self,
            palette: None,
        }
    }

    pub(super) fn formatter_html(&self) -> HtmlFormatter<'_> {
        HtmlFormatter {
            style: self,
            palette: None,
        }
    }
}

impl Default for Style {
    fn default() -> Self {
        Self::default_style()
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Color {
    #[default]
    None,
    Palette(u8),
    Rgb(Rgb),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Flags {
    pub(super) bold: bool,
    pub(super) italic: bool,
    pub(super) faint: bool,
    pub(super) blink: bool,
    pub(super) inverse: bool,
    pub(super) invisible: bool,
    pub(super) strikethrough: bool,
    pub(super) overline: bool,
    pub(super) underline: Underline,
}

impl Flags {
    pub(super) const fn default_flags() -> Self {
        Self {
            bold: false,
            italic: false,
            faint: false,
            blink: false,
            inverse: false,
            invisible: false,
            strikethrough: false,
            overline: false,
            underline: Underline::None,
        }
    }
}

impl Default for Flags {
    fn default() -> Self {
        Self::default_flags()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BoldColor {
    Color(Rgb),
    Bright,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Fg<'a> {
    pub(super) default: Rgb,
    pub(super) palette: &'a Palette,
    pub(super) bold: Option<BoldColor>,
}

pub(super) struct VtFormatter<'a> {
    style: &'a Style,
    palette: Option<&'a Palette>,
}

impl<'a> VtFormatter<'a> {
    pub(super) fn with_palette(mut self, palette: &'a Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    fn format_color(&self, f: &mut fmt::Formatter<'_>, prefix: u8, color: Color) -> fmt::Result {
        match color {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(palette) = self.palette {
                    let rgb = palette[idx as usize];
                    write!(f, "\x1b[{prefix};2;{};{};{}m", rgb.r, rgb.g, rgb.b)
                } else {
                    write!(f, "\x1b[{prefix};5;{idx}m")
                }
            }
            Color::Rgb(rgb) => write!(f, "\x1b[{prefix};2;{};{};{}m", rgb.r, rgb.g, rgb.b),
        }
    }
}

impl fmt::Display for VtFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "\x1b[0m")?;

        if self.style.flags.bold {
            write!(f, "\x1b[1m")?;
        }
        if self.style.flags.faint {
            write!(f, "\x1b[2m")?;
        }
        if self.style.flags.italic {
            write!(f, "\x1b[3m")?;
        }
        if self.style.flags.blink {
            write!(f, "\x1b[5m")?;
        }
        if self.style.flags.inverse {
            write!(f, "\x1b[7m")?;
        }
        if self.style.flags.invisible {
            write!(f, "\x1b[8m")?;
        }
        if self.style.flags.strikethrough {
            write!(f, "\x1b[9m")?;
        }
        if self.style.flags.overline {
            write!(f, "\x1b[53m")?;
        }
        match self.style.flags.underline {
            Underline::None => {}
            Underline::Single => write!(f, "\x1b[4m")?,
            Underline::Double => write!(f, "\x1b[4:2m")?,
            Underline::Curly => write!(f, "\x1b[4:3m")?,
            Underline::Dotted => write!(f, "\x1b[4:4m")?,
            Underline::Dashed => write!(f, "\x1b[4:5m")?,
        }

        self.format_color(f, 38, self.style.fg_color)?;
        self.format_color(f, 48, self.style.bg_color)?;
        self.format_color(f, 58, self.style.underline_color)
    }
}

pub(super) struct HtmlFormatter<'a> {
    style: &'a Style,
    palette: Option<&'a Palette>,
}

impl<'a> HtmlFormatter<'a> {
    pub(super) fn with_palette(mut self, palette: &'a Palette) -> Self {
        self.palette = Some(palette);
        self
    }

    fn format_color(
        &self,
        f: &mut fmt::Formatter<'_>,
        property: &str,
        color: Color,
    ) -> fmt::Result {
        match color {
            Color::None => Ok(()),
            Color::Palette(idx) => {
                if let Some(palette) = self.palette {
                    let rgb = palette[idx as usize];
                    write!(f, "{property}: rgb({}, {}, {});", rgb.r, rgb.g, rgb.b)
                } else {
                    write!(f, "{property}: var(--vt-palette-{idx});")
                }
            }
            Color::Rgb(rgb) => write!(f, "{property}: rgb({}, {}, {});", rgb.r, rgb.g, rgb.b),
        }
    }
}

impl fmt::Display for HtmlFormatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.format_color(f, "color", self.style.fg_color)?;
        self.format_color(f, "background-color", self.style.bg_color)?;
        self.format_color(f, "text-decoration-color", self.style.underline_color)?;

        let has_line = self.style.flags.underline != Underline::None
            || self.style.flags.strikethrough
            || self.style.flags.overline
            || self.style.flags.blink;
        if has_line {
            write!(f, "text-decoration-line:")?;
            if self.style.flags.underline != Underline::None {
                write!(f, " underline")?;
            }
            if self.style.flags.strikethrough {
                write!(f, " line-through")?;
            }
            if self.style.flags.overline {
                write!(f, " overline")?;
            }
            if self.style.flags.blink {
                write!(f, " blink")?;
            }
            write!(f, ";")?;
        }

        match self.style.flags.underline {
            Underline::None => {}
            Underline::Single => write!(f, "text-decoration-style: solid;")?,
            Underline::Double => write!(f, "text-decoration-style: double;")?,
            Underline::Curly => write!(f, "text-decoration-style: wavy;")?,
            Underline::Dotted => write!(f, "text-decoration-style: dotted;")?,
            Underline::Dashed => write!(f, "text-decoration-style: dashed;")?,
        }

        if self.style.flags.bold {
            write!(f, "font-weight: bold;")?;
        }
        if self.style.flags.italic {
            write!(f, "font-style: italic;")?;
        }
        if self.style.flags.faint {
            write!(f, "opacity: 0.5;")?;
        }
        if self.style.flags.invisible {
            write!(f, "visibility: hidden;")?;
        }
        if self.style.flags.inverse {
            write!(f, "filter: invert(100%);")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::color::{Rgb, DEFAULT_PALETTE};
    use super::*;

    fn style_with_flags(flags: Flags) -> Style {
        Style {
            flags,
            ..Style::default()
        }
    }

    #[test]
    fn style_default_and_equality() {
        assert!(Style::default().is_default());
        assert_eq!(Style::default(), Style::default_style());

        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });
        assert!(!style.is_default());
        assert_eq!(DEFAULT_ID, 0);
    }

    #[test]
    fn foreground_bold_behavior() {
        let default = Rgb::new(1, 2, 3);
        let bold = Rgb::new(4, 5, 6);
        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });

        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Color(bold)),
            }),
            bold
        );

        let style = Style {
            fg_color: Color::Palette(1),
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Bright),
            }),
            DEFAULT_PALETTE[9]
        );

        let style = Style {
            fg_color: Color::Rgb(default),
            flags: Flags {
                bold: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        assert_eq!(
            style.fg(Fg {
                default,
                palette: &DEFAULT_PALETTE,
                bold: Some(BoldColor::Color(bold)),
            }),
            bold
        );
    }

    #[test]
    fn background_and_underline_color_lookup() {
        let style = Style {
            bg_color: Color::Palette(2),
            underline_color: Color::Rgb(Rgb::new(8, 9, 10)),
            ..Style::default()
        };

        assert_eq!(style.bg_color(&DEFAULT_PALETTE), Some(DEFAULT_PALETTE[2]));
        assert_eq!(
            style.underline_color(&DEFAULT_PALETTE),
            Some(Rgb::new(8, 9, 10))
        );
    }

    #[test]
    fn style_vt_formatting_empty() {
        let style = Style::default();
        assert_eq!(style.formatter_vt().to_string(), "\x1b[0m");
    }

    #[test]
    fn style_vt_formatting_single_flags() {
        let cases = [
            (
                Flags {
                    bold: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[1m",
            ),
            (
                Flags {
                    faint: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[2m",
            ),
            (
                Flags {
                    italic: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[3m",
            ),
            (
                Flags {
                    blink: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[5m",
            ),
            (
                Flags {
                    inverse: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[7m",
            ),
            (
                Flags {
                    invisible: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[8m",
            ),
            (
                Flags {
                    strikethrough: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[9m",
            ),
            (
                Flags {
                    overline: true,
                    ..Flags::default()
                },
                "\x1b[0m\x1b[53m",
            ),
        ];

        for (flags, expected) in cases {
            assert_eq!(style_with_flags(flags).formatter_vt().to_string(), expected);
        }
    }

    #[test]
    fn style_vt_formatting_flags() {
        let style = style_with_flags(Flags {
            bold: true,
            faint: true,
            italic: true,
            blink: true,
            inverse: true,
            invisible: true,
            strikethrough: true,
            overline: true,
            underline: Underline::Curly,
        });
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[1m\x1b[2m\x1b[3m\x1b[5m\x1b[7m\x1b[8m\x1b[9m\x1b[53m\x1b[4:3m"
        );
    }

    #[test]
    fn style_vt_formatting_each_underline() {
        let cases = [
            (Underline::Single, "\x1b[0m\x1b[4m"),
            (Underline::Double, "\x1b[0m\x1b[4:2m"),
            (Underline::Curly, "\x1b[0m\x1b[4:3m"),
            (Underline::Dotted, "\x1b[0m\x1b[4:4m"),
            (Underline::Dashed, "\x1b[0m\x1b[4:5m"),
        ];

        for (underline, expected) in cases {
            let style = style_with_flags(Flags {
                underline,
                ..Flags::default()
            });
            assert_eq!(style.formatter_vt().to_string(), expected);
        }
    }

    #[test]
    fn style_vt_formatting_colors() {
        let cases = [
            (
                Style {
                    fg_color: Color::Palette(42),
                    ..Style::default()
                },
                "\x1b[0m\x1b[38;5;42m",
            ),
            (
                Style {
                    fg_color: Color::Rgb(Rgb::new(255, 128, 64)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[38;2;255;128;64m",
            ),
            (
                Style {
                    bg_color: Color::Palette(7),
                    ..Style::default()
                },
                "\x1b[0m\x1b[48;5;7m",
            ),
            (
                Style {
                    bg_color: Color::Rgb(Rgb::new(32, 64, 96)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[48;2;32;64;96m",
            ),
            (
                Style {
                    underline_color: Color::Palette(15),
                    ..Style::default()
                },
                "\x1b[0m\x1b[58;5;15m",
            ),
            (
                Style {
                    underline_color: Color::Rgb(Rgb::new(200, 100, 50)),
                    ..Style::default()
                },
                "\x1b[0m\x1b[58;2;200;100;50m",
            ),
        ];

        for (style, expected) in cases {
            assert_eq!(style.formatter_vt().to_string(), expected);
        }

        let style = Style {
            fg_color: Color::Rgb(Rgb::new(10, 20, 30)),
            bg_color: Color::Rgb(Rgb::new(40, 50, 60)),
            underline_color: Color::Rgb(Rgb::new(70, 80, 90)),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[38;2;10;20;30m\x1b[48;2;40;50;60m\x1b[58;2;70;80;90m"
        );

        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[38;5;1m\x1b[48;5;2m\x1b[58;5;3m"
        );
        assert_eq!(
            Style {
                fg_color: Color::Palette(1),
                ..Style::default()
            }
            .formatter_vt()
            .with_palette(&DEFAULT_PALETTE)
            .to_string(),
            "\x1b[0m\x1b[38;2;204;102;102m"
        );
        assert_eq!(
            style
                .formatter_vt()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "\x1b[0m\x1b[38;2;204;102;102m\x1b[48;2;181;189;104m\x1b[58;2;240;198;116m"
        );
    }

    #[test]
    fn style_vt_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 0, 0)),
            bg_color: Color::Palette(8),
            underline_color: Color::Rgb(Rgb::new(0, 255, 0)),
            flags: Flags {
                bold: true,
                italic: true,
                underline: Underline::Double,
                ..Flags::default()
            },
        };
        assert_eq!(
            style.formatter_vt().to_string(),
            "\x1b[0m\x1b[1m\x1b[3m\x1b[4:2m\x1b[38;2;255;0;0m\x1b[48;5;8m\x1b[58;2;0;255;0m"
        );
    }

    #[test]
    fn style_html_formatting_basic_bold() {
        let style = style_with_flags(Flags {
            bold: true,
            ..Flags::default()
        });
        assert_eq!(style.formatter_html().to_string(), "font-weight: bold;");
    }

    #[test]
    fn style_html_formatting_colors() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 128, 64)),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_html().to_string(),
            "color: rgb(255, 128, 64);"
        );

        let style = Style {
            bg_color: Color::Palette(7),
            ..Style::default()
        };
        assert_eq!(
            style.formatter_html().to_string(),
            "background-color: var(--vt-palette-7);"
        );
        assert_eq!(
            style
                .formatter_html()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "background-color: rgb(197, 200, 198);"
        );
    }

    #[test]
    fn style_html_formatting_combined_colors_and_flags() {
        let style = Style {
            fg_color: Color::Rgb(Rgb::new(255, 0, 0)),
            bg_color: Color::Rgb(Rgb::new(0, 0, 255)),
            flags: Flags {
                bold: true,
                italic: true,
                ..Flags::default()
            },
            ..Style::default()
        };
        let result = style.formatter_html().to_string();
        assert!(result.contains("color: rgb(255, 0, 0);"));
        assert!(result.contains("background-color: rgb(0, 0, 255);"));
        assert!(result.contains("font-weight: bold;"));
        assert!(result.contains("font-style: italic;"));
    }

    #[test]
    fn style_html_formatting_decoration() {
        let style = style_with_flags(Flags {
            underline: Underline::Single,
            ..Flags::default()
        });
        let result = style.formatter_html().to_string();
        assert!(result.contains("text-decoration-line: underline;"));
        assert!(result.contains("text-decoration-style: solid;"));

        let style = style_with_flags(Flags {
            underline: Underline::Curly,
            strikethrough: true,
            overline: true,
            ..Flags::default()
        });
        let result = style.formatter_html().to_string();
        assert!(result.contains("text-decoration-line: underline line-through overline;"));
        assert!(result.contains("text-decoration-style: wavy;"));
    }

    #[test]
    fn style_html_formatting_all_palette_colors_with_palette_set() {
        let style = Style {
            fg_color: Color::Palette(1),
            bg_color: Color::Palette(2),
            underline_color: Color::Palette(3),
            ..Style::default()
        };
        assert_eq!(
            style
                .formatter_html()
                .with_palette(&DEFAULT_PALETTE)
                .to_string(),
            "color: rgb(204, 102, 102);background-color: rgb(181, 189, 104);text-decoration-color: rgb(240, 198, 116);"
        );
    }
}
