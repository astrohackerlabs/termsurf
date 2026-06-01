pub(super) const MAX_BUF: usize = 2048;
const OSC_COLOR_REQUEST_CAPACITY: usize = MAX_BUF / 2 + 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command<'a> {
    WindowTitle { title: &'a str },
    ReportPwd { url: &'a str },
    StartHyperlink { id: Option<&'a str>, uri: &'a str },
    EndHyperlink,
    ColorOperation { requests: ColorRequests },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Terminator {
    Bel,
    St,
}

impl Terminator {
    pub(super) const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Bel => b"\x07",
            Self::St => b"\x1b\\",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ColorRequests {
    items: [Option<ColorRequest>; OSC_COLOR_REQUEST_CAPACITY],
    len: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ColorRequest {
    SetPalette { index: u8, rgb: super::color::Rgb },
    QueryPalette { index: u8, terminator: Terminator },
    ResetPalette { index: u8 },
    ResetAllPalette,
}

impl ColorRequests {
    const fn new() -> Self {
        Self {
            items: [None; OSC_COLOR_REQUEST_CAPACITY],
            len: 0,
        }
    }

    fn push(&mut self, request: ColorRequest) -> Result<(), ()> {
        let Some(slot) = self.items.get_mut(self.len) else {
            return Err(());
        };
        *slot = Some(request);
        self.len += 1;
        Ok(())
    }

    pub(super) fn iter(&self) -> impl Iterator<Item = ColorRequest> + '_ {
        self.items[..self.len]
            .iter()
            .map(|request| request.expect("color request slots below len must be initialized"))
    }

    #[cfg(test)]
    fn as_slice(&self) -> &[Option<ColorRequest>] {
        &self.items[..self.len]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Parser {
    buffer: [u8; MAX_BUF],
    len: usize,
    invalid: bool,
}

impl Parser {
    pub(super) const fn new() -> Self {
        Self {
            buffer: [0; MAX_BUF],
            len: 0,
            invalid: false,
        }
    }

    pub(super) fn reset(&mut self) {
        self.len = 0;
        self.invalid = false;
    }

    pub(super) fn invalidate(&mut self) {
        self.invalid = true;
    }

    pub(super) fn push(&mut self, byte: u8) {
        if self.invalid {
            return;
        }
        if self.len >= self.buffer.len() {
            self.invalid = true;
            return;
        }
        self.buffer[self.len] = byte;
        self.len += 1;
    }

    pub(super) fn push_escape_and(&mut self, byte: u8) {
        self.push(0x1b);
        self.push(byte);
    }

    pub(super) fn command(&self, terminator: Terminator) -> Option<Command<'_>> {
        if self.invalid {
            return None;
        }

        let bytes = &self.buffer[..self.len];
        let (number, rest) =
            SplitOnce::split_once(bytes, |byte| *byte == b';').unwrap_or((bytes, &[]));

        match number {
            b"0" | b"2" => valid_utf8(rest).map(|title| Command::WindowTitle { title }),
            b"1" => {
                valid_utf8(rest)?;
                None
            }
            b"4" => {
                parse_osc4(rest, terminator).map(|requests| Command::ColorOperation { requests })
            }
            b"7" => valid_utf8(rest).map(|url| Command::ReportPwd { url }),
            b"8" => parse_hyperlink(rest),
            b"104" => parse_osc104(rest).map(|requests| Command::ColorOperation { requests }),
            _ => None,
        }
    }
}

fn valid_utf8(bytes: &[u8]) -> Option<&str> {
    std::str::from_utf8(bytes).ok()
}

fn parse_hyperlink(bytes: &[u8]) -> Option<Command<'_>> {
    let (params, uri_bytes) = SplitOnce::split_once(bytes, |byte| *byte == b';')?;
    let uri = valid_utf8(uri_bytes)?;
    if uri.is_empty() {
        return Some(Command::EndHyperlink);
    }

    let id = if params.is_empty() {
        None
    } else {
        let id = params.strip_prefix(b"id=")?;
        if id.is_empty() {
            return None;
        }
        Some(valid_utf8(id)?)
    };

    Some(Command::StartHyperlink { id, uri })
}

fn parse_osc4(bytes: &[u8], terminator: Terminator) -> Option<ColorRequests> {
    let mut result = ColorRequests::new();
    let mut parts = bytes.split(|byte| *byte == b';');

    while let Some(index_bytes) = parts.next() {
        let Some(spec) = parts.next() else {
            break;
        };
        let Ok(index) = parse_palette_index(index_bytes) else {
            break;
        };

        let request = if spec == b"?" {
            ColorRequest::QueryPalette { index, terminator }
        } else {
            let Some(rgb) = parse_rgb(spec) else {
                break;
            };
            ColorRequest::SetPalette { index, rgb }
        };

        if result.push(request).is_err() {
            return None;
        }
    }

    (result.len > 0).then_some(result)
}

fn parse_osc104(bytes: &[u8]) -> Option<ColorRequests> {
    let mut result = ColorRequests::new();
    let mut saw_field = false;

    for index_bytes in bytes.split(|byte| *byte == b';') {
        if index_bytes.is_empty() {
            continue;
        }
        saw_field = true;
        let Ok(index) = parse_palette_index(index_bytes) else {
            continue;
        };
        if result.push(ColorRequest::ResetPalette { index }).is_err() {
            return None;
        }
    }

    if !saw_field {
        result.push(ColorRequest::ResetAllPalette).ok()?;
    }

    (result.len > 0).then_some(result)
}

fn parse_palette_index(bytes: &[u8]) -> Result<u8, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    text.parse::<u8>().map_err(|_| ())
}

fn parse_rgb(bytes: &[u8]) -> Option<super::color::Rgb> {
    if let Some(hex) = bytes.strip_prefix(b"#") {
        return parse_hash_rgb(hex);
    }

    let hex = bytes.strip_prefix(b"rgb:")?;
    let mut parts = hex.split(|byte| *byte == b'/');
    let r = parse_hex_channel(parts.next()?)?;
    let g = parse_hex_channel(parts.next()?)?;
    let b = parse_hex_channel(parts.next()?)?;
    if parts.next().is_some() {
        return None;
    }
    Some(super::color::Rgb::new(r, g, b))
}

fn parse_hash_rgb(hex: &[u8]) -> Option<super::color::Rgb> {
    let width = match hex.len() {
        3 => 1,
        6 => 2,
        9 => 3,
        12 => 4,
        _ => return None,
    };
    Some(super::color::Rgb::new(
        parse_hex_channel(&hex[..width])?,
        parse_hex_channel(&hex[width..width * 2])?,
        parse_hex_channel(&hex[width * 2..])?,
    ))
}

fn parse_hex_channel(bytes: &[u8]) -> Option<u8> {
    if !(1..=4).contains(&bytes.len()) {
        return None;
    }
    let text = std::str::from_utf8(bytes).ok()?;
    let value = u16::from_str_radix(text, 16).ok()? as u32;
    let max = match bytes.len() {
        1 => 0x0f,
        2 => 0xff,
        3 => 0x0fff,
        4 => 0xffff,
        _ => return None,
    };
    Some(((value * 0xff) / max) as u8)
}

trait SplitOnce {
    fn split_once<P>(&self, predicate: P) -> Option<(&Self, &Self)>
    where
        P: FnMut(&u8) -> bool;
}

impl SplitOnce for [u8] {
    fn split_once<P>(&self, mut predicate: P) -> Option<(&Self, &Self)>
    where
        P: FnMut(&u8) -> bool,
    {
        let idx = self.iter().position(&mut predicate)?;
        Some((&self[..idx], &self[idx + 1..]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq)]
    enum OwnedCommand {
        WindowTitle { title: String },
        ReportPwd { url: String },
        StartHyperlink { id: Option<String>, uri: String },
        EndHyperlink,
        ColorOperation { requests: Vec<ColorRequest> },
    }

    impl From<Command<'_>> for OwnedCommand {
        fn from(command: Command<'_>) -> Self {
            match command {
                Command::WindowTitle { title } => Self::WindowTitle {
                    title: title.to_string(),
                },
                Command::ReportPwd { url } => Self::ReportPwd {
                    url: url.to_string(),
                },
                Command::StartHyperlink { id, uri } => Self::StartHyperlink {
                    id: id.map(ToString::to_string),
                    uri: uri.to_string(),
                },
                Command::EndHyperlink => Self::EndHyperlink,
                Command::ColorOperation { requests } => Self::ColorOperation {
                    requests: requests.iter().collect(),
                },
            }
        }
    }

    fn parse(input: &[u8]) -> Option<OwnedCommand> {
        parse_with_terminator(input, Terminator::St)
    }

    fn parse_with_terminator(input: &[u8], terminator: Terminator) -> Option<OwnedCommand> {
        let mut parser = Parser::new();
        for &byte in input {
            parser.push(byte);
        }
        parser.command(terminator).map(OwnedCommand::from)
    }

    #[test]
    fn osc_parser_basic_commands() {
        assert_eq!(
            parse(b"0;hello"),
            Some(OwnedCommand::WindowTitle {
                title: "hello".to_string(),
            })
        );
        assert_eq!(
            parse(b"2;world"),
            Some(OwnedCommand::WindowTitle {
                title: "world".to_string(),
            })
        );
        assert_eq!(parse(b"1;ignored"), None);
        assert_eq!(
            parse(b"7;file://host/path"),
            Some(OwnedCommand::ReportPwd {
                url: "file://host/path".to_string(),
            })
        );
    }

    #[test]
    fn osc_parser_hyperlinks() {
        assert_eq!(
            parse(b"8;;https://example.com"),
            Some(OwnedCommand::StartHyperlink {
                id: None,
                uri: "https://example.com".to_string(),
            })
        );
        assert_eq!(
            parse(b"8;id=tab;https://example.com"),
            Some(OwnedCommand::StartHyperlink {
                id: Some("tab".to_string()),
                uri: "https://example.com".to_string(),
            })
        );
        assert_eq!(parse(b"8;;"), Some(OwnedCommand::EndHyperlink));
    }

    #[test]
    fn osc_parser_rejects_invalid_or_unsupported() {
        assert_eq!(parse(b"9;notification"), None);
        assert_eq!(parse(b"8;bad=value;https://example.com"), None);
        assert_eq!(parse(b"8;id=;https://example.com"), None);
        assert_eq!(parse(b"8"), None);
        assert_eq!(parse(b"0;\xff"), None);
    }

    #[test]
    fn osc_parser_palette_color_operations() {
        assert_eq!(
            parse(b"4;1;rgb:ff/00/80"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(255, 0, 128),
                }],
            })
        );
        assert_eq!(
            parse_with_terminator(b"4;2;?", Terminator::Bel),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::QueryPalette {
                    index: 2,
                    terminator: Terminator::Bel,
                }],
            })
        );
        assert_eq!(
            parse(b"4;1;#f00;2;#0000ffff0000"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![
                    ColorRequest::SetPalette {
                        index: 1,
                        rgb: super::super::color::Rgb::new(255, 0, 0),
                    },
                    ColorRequest::SetPalette {
                        index: 2,
                        rgb: super::super::color::Rgb::new(0, 255, 0),
                    },
                ],
            })
        );
    }

    #[test]
    fn osc_parser_palette_color_scaling() {
        assert_eq!(
            parse(b"4;1;rgb:f/8/0"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(255, 136, 0),
                }],
            })
        );
        assert_eq!(
            parse(b"4;1;#800800800"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(127, 127, 127),
                }],
            })
        );
        assert_eq!(
            parse(b"4;1;rgb:800/800/800"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(127, 127, 127),
                }],
            })
        );
        assert_eq!(
            parse(b"4;1;rgb:8000/8000/8000"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(127, 127, 127),
                }],
            })
        );
    }

    #[test]
    fn osc_parser_palette_invalid_data_preserves_prior_requests() {
        assert_eq!(
            parse(b"4;1;#ff0000;2;red;3;#00ff00"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(255, 0, 0),
                }],
            })
        );
        assert_eq!(parse(b"4;300;#ff0000"), None);
    }

    #[test]
    fn osc_parser_palette_reset_operations() {
        assert_eq!(
            parse(b"104"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::ResetAllPalette],
            })
        );
        assert_eq!(
            parse(b"104;1;;bad;2;300"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![
                    ColorRequest::ResetPalette { index: 1 },
                    ColorRequest::ResetPalette { index: 2 },
                ],
            })
        );
        assert_eq!(parse(b"104;bad;300"), None);
        assert_eq!(
            parse(b"104;;"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::ResetAllPalette],
            })
        );
    }

    #[test]
    fn osc_parser_over_capacity_invalidates() {
        let mut parser = Parser::new();
        for _ in 0..MAX_BUF + 1 {
            parser.push(b'a');
        }
        assert_eq!(parser.command(Terminator::St), None);
    }

    #[test]
    fn osc_parser_color_request_capacity_covers_max_buffer() {
        let mut parser = Parser::new();
        for byte in b"104;" {
            parser.push(*byte);
        }
        let expected = (MAX_BUF - 4) / 2;
        for i in 0..expected {
            parser.push(b'1');
            if i + 1 < expected {
                parser.push(b';');
            }
        }

        let Some(Command::ColorOperation { requests }) = parser.command(Terminator::St) else {
            panic!("max-buffer dense reset command should parse");
        };
        assert_eq!(requests.as_slice().len(), expected);
    }
}
