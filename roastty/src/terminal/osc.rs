pub(super) const MAX_BUF: usize = 2048;
const OSC_COLOR_REQUEST_CAPACITY: usize = MAX_BUF / 2 + 1;
const KITTY_TEXT_SIZING_MAX_PAYLOAD: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command<'a> {
    WindowTitle {
        title: &'a str,
    },
    ReportPwd {
        url: &'a str,
    },
    MouseShape {
        shape: super::mouse::MouseShape,
    },
    StartHyperlink {
        id: Option<&'a str>,
        uri: &'a str,
    },
    EndHyperlink,
    ColorOperation {
        requests: ColorRequests,
    },
    KittyColor {
        requests: super::kitty::ColorRequests,
        terminator: Terminator,
    },
    KittyTextSizing {
        value: KittyTextSizing<'a>,
    },
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
    SetPalette {
        index: u8,
        rgb: super::color::Rgb,
    },
    QueryPalette {
        index: u8,
        terminator: Terminator,
    },
    ResetPalette {
        index: u8,
    },
    ResetAllPalette,
    SetDynamic {
        target: DynamicColor,
        rgb: super::color::Rgb,
    },
    QueryDynamic {
        target: DynamicColor,
        terminator: Terminator,
    },
    ResetDynamic {
        target: DynamicColor,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DynamicColor {
    Foreground,
    Background,
    Cursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KittyTextSizing<'a> {
    pub(super) scale: u8,
    pub(super) width: u8,
    pub(super) numerator: u8,
    pub(super) denominator: u8,
    pub(super) valign: KittyTextVerticalAlign,
    pub(super) halign: KittyTextHorizontalAlign,
    pub(super) text: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KittyTextVerticalAlign {
    Top,
    Bottom,
    Center,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum KittyTextHorizontalAlign {
    Left,
    Right,
    Center,
}

impl DynamicColor {
    pub(super) const fn number(self) -> u8 {
        match self {
            Self::Foreground => 10,
            Self::Background => 11,
            Self::Cursor => 12,
        }
    }

    const fn next(self) -> Option<Self> {
        match self {
            Self::Foreground => Some(Self::Background),
            Self::Background => Some(Self::Cursor),
            Self::Cursor => None,
        }
    }
}

impl<'a> KittyTextSizing<'a> {
    fn new(text: &'a str) -> Self {
        Self {
            scale: 1,
            width: 0,
            numerator: 0,
            denominator: 0,
            valign: KittyTextVerticalAlign::Top,
            halign: KittyTextHorizontalAlign::Left,
            text,
        }
    }

    fn update(&mut self, key: u8, value: &[u8]) {
        let Some(number) = parse_kitty_text_sizing_value(value) else {
            return;
        };

        match key {
            b's' if (1..=7).contains(&number) => self.scale = number,
            b'w' if number <= 7 => self.width = number,
            b'n' => self.numerator = number,
            b'd' => self.denominator = number,
            b'v' => {
                if let Some(align) = KittyTextVerticalAlign::from_number(number) {
                    self.valign = align;
                }
            }
            b'h' => {
                if let Some(align) = KittyTextHorizontalAlign::from_number(number) {
                    self.halign = align;
                }
            }
            _ => {}
        }
    }
}

impl KittyTextVerticalAlign {
    const fn from_number(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Top),
            1 => Some(Self::Bottom),
            2 => Some(Self::Center),
            _ => None,
        }
    }
}

impl KittyTextHorizontalAlign {
    const fn from_number(value: u8) -> Option<Self> {
        match value {
            0 => Some(Self::Left),
            1 => Some(Self::Right),
            2 => Some(Self::Center),
            _ => None,
        }
    }
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
            b"10" => parse_dynamic_set_query(rest, DynamicColor::Foreground, terminator)
                .map(|requests| Command::ColorOperation { requests }),
            b"11" => parse_dynamic_set_query(rest, DynamicColor::Background, terminator)
                .map(|requests| Command::ColorOperation { requests }),
            b"12" => parse_dynamic_set_query(rest, DynamicColor::Cursor, terminator)
                .map(|requests| Command::ColorOperation { requests }),
            b"21" => parse_kitty_color(rest).map(|requests| Command::KittyColor {
                requests,
                terminator,
            }),
            b"22" => {
                super::mouse::MouseShape::parse(rest).map(|shape| Command::MouseShape { shape })
            }
            b"66" => parse_kitty_text_sizing(rest).map(|value| Command::KittyTextSizing { value }),
            b"104" => parse_osc104(rest).map(|requests| Command::ColorOperation { requests }),
            b"110" => parse_dynamic_reset(rest, DynamicColor::Foreground)
                .map(|requests| Command::ColorOperation { requests }),
            b"111" => parse_dynamic_reset(rest, DynamicColor::Background)
                .map(|requests| Command::ColorOperation { requests }),
            b"112" => parse_dynamic_reset(rest, DynamicColor::Cursor)
                .map(|requests| Command::ColorOperation { requests }),
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
            let Some(rgb) = super::color::Rgb::parse(spec) else {
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

fn parse_dynamic_set_query(
    bytes: &[u8],
    start: DynamicColor,
    terminator: Terminator,
) -> Option<ColorRequests> {
    let mut result = ColorRequests::new();
    let mut target = start;

    for spec in bytes.split(|byte| *byte == b';') {
        if spec.is_empty() {
            continue;
        }
        let request = if spec == b"?" {
            ColorRequest::QueryDynamic { target, terminator }
        } else {
            let Some(rgb) = super::color::Rgb::parse(spec) else {
                break;
            };
            ColorRequest::SetDynamic { target, rgb }
        };
        if result.push(request).is_err() {
            return None;
        }
        let Some(next) = target.next() else {
            break;
        };
        target = next;
    }

    (result.len > 0).then_some(result)
}

fn parse_dynamic_reset(bytes: &[u8], target: DynamicColor) -> Option<ColorRequests> {
    if bytes
        .split(|byte| *byte == b';')
        .any(|field| !field.is_empty())
    {
        return None;
    }
    let mut result = ColorRequests::new();
    result.push(ColorRequest::ResetDynamic { target }).ok()?;
    Some(result)
}

fn parse_kitty_color(bytes: &[u8]) -> Option<super::kitty::ColorRequests> {
    let mut result = super::kitty::ColorRequests::new();

    for item in bytes.split(|byte| *byte == b';') {
        if result.len() >= super::kitty::COLOR_REQUEST_CAPACITY {
            return None;
        }

        let (key_bytes, value_bytes) =
            SplitOnce::split_once(item, |byte| *byte == b'=').unwrap_or((item, &[]));
        if key_bytes.is_empty() {
            continue;
        }
        let Some(key) = super::kitty::ColorKind::parse(key_bytes) else {
            continue;
        };

        let value = trim_edge_spaces(value_bytes);
        let request = if value.is_empty() {
            super::kitty::ColorRequest::Reset(key)
        } else if value == b"?" {
            super::kitty::ColorRequest::Query(key)
        } else {
            let Some(rgb) = super::color::Rgb::parse(value) else {
                continue;
            };
            super::kitty::ColorRequest::Set { key, rgb }
        };
        result.push(request).ok()?;
    }

    Some(result)
}

fn parse_kitty_text_sizing(bytes: &[u8]) -> Option<KittyTextSizing<'_>> {
    let (params, payload) = SplitOnce::split_once(bytes, |byte| *byte == b';')?;
    if payload.len() > KITTY_TEXT_SIZING_MAX_PAYLOAD {
        return None;
    }
    let text = valid_safe_utf8(payload)?;
    let mut result = KittyTextSizing::new(text);

    if !params.is_empty() {
        for param in params.split(|byte| *byte == b':') {
            let mut parts = param.split(|byte| *byte == b'=');
            let Some(key) = parts.next() else {
                continue;
            };
            if key.len() != 1 {
                continue;
            }
            let Some(value) = parts.next() else {
                continue;
            };
            result.update(key[0], value);
        }
    }

    Some(result)
}

fn parse_kitty_text_sizing_value(bytes: &[u8]) -> Option<u8> {
    let text = std::str::from_utf8(bytes).ok()?;
    let value = text.parse::<u8>().ok()?;
    (value <= 15).then_some(value)
}

fn valid_safe_utf8(bytes: &[u8]) -> Option<&str> {
    let text = valid_utf8(bytes)?;
    if text
        .chars()
        .any(|cp| matches!(cp as u32, 0x00..=0x1f | 0x7f | 0x80..=0x9f))
    {
        return None;
    }
    Some(text)
}

fn parse_palette_index(bytes: &[u8]) -> Result<u8, ()> {
    let text = std::str::from_utf8(bytes).map_err(|_| ())?;
    text.parse::<u8>().map_err(|_| ())
}

fn trim_edge_spaces(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start] == b' ' {
        start += 1;
    }
    while end > start && bytes[end - 1] == b' ' {
        end -= 1;
    }
    &bytes[start..end]
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
        WindowTitle {
            title: String,
        },
        ReportPwd {
            url: String,
        },
        MouseShape {
            shape: super::super::mouse::MouseShape,
        },
        StartHyperlink {
            id: Option<String>,
            uri: String,
        },
        EndHyperlink,
        ColorOperation {
            requests: Vec<ColorRequest>,
        },
        KittyColor {
            requests: Vec<super::super::kitty::ColorRequest>,
            terminator: Terminator,
        },
        KittyTextSizing {
            scale: u8,
            width: u8,
            numerator: u8,
            denominator: u8,
            valign: KittyTextVerticalAlign,
            halign: KittyTextHorizontalAlign,
            text: String,
        },
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
                Command::MouseShape { shape } => Self::MouseShape { shape },
                Command::StartHyperlink { id, uri } => Self::StartHyperlink {
                    id: id.map(ToString::to_string),
                    uri: uri.to_string(),
                },
                Command::EndHyperlink => Self::EndHyperlink,
                Command::ColorOperation { requests } => Self::ColorOperation {
                    requests: requests.iter().collect(),
                },
                Command::KittyColor {
                    requests,
                    terminator,
                } => Self::KittyColor {
                    requests: requests.iter().collect(),
                    terminator,
                },
                Command::KittyTextSizing { value } => Self::KittyTextSizing {
                    scale: value.scale,
                    width: value.width,
                    numerator: value.numerator,
                    denominator: value.denominator,
                    valign: value.valign,
                    halign: value.halign,
                    text: value.text.to_string(),
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
    fn osc_parser_mouse_shape() {
        assert_eq!(
            parse(b"22;pointer"),
            Some(OwnedCommand::MouseShape {
                shape: super::super::mouse::MouseShape::Pointer,
            })
        );
        assert_eq!(
            parse(b"22;left_ptr"),
            Some(OwnedCommand::MouseShape {
                shape: super::super::mouse::MouseShape::Default,
            })
        );
        assert_eq!(parse(b"22;Pointer"), None);
        assert_eq!(parse(b"22;unknown"), None);
    }

    #[test]
    fn osc_parser_kitty_text_sizing_defaults_and_parameters() {
        assert_eq!(
            parse(b"66;;bobr"),
            Some(OwnedCommand::KittyTextSizing {
                scale: 1,
                width: 0,
                numerator: 0,
                denominator: 0,
                valign: KittyTextVerticalAlign::Top,
                halign: KittyTextHorizontalAlign::Left,
                text: "bobr".to_string(),
            })
        );
        assert_eq!(
            parse(b"66;s=2;kurwa"),
            Some(OwnedCommand::KittyTextSizing {
                scale: 2,
                width: 0,
                numerator: 0,
                denominator: 0,
                valign: KittyTextVerticalAlign::Top,
                halign: KittyTextHorizontalAlign::Left,
                text: "kurwa".to_string(),
            })
        );
        assert_eq!(
            parse(b"66;s=2:w=7:n=13:d=15:v=1:h=2;long"),
            Some(OwnedCommand::KittyTextSizing {
                scale: 2,
                width: 7,
                numerator: 13,
                denominator: 15,
                valign: KittyTextVerticalAlign::Bottom,
                halign: KittyTextHorizontalAlign::Center,
                text: "long".to_string(),
            })
        );
    }

    #[test]
    fn osc_parser_kitty_text_sizing_invalid_parameters_are_field_local() {
        assert_eq!(
            parse(b"66;s=0:w=8:v=3:n=16;ok"),
            Some(OwnedCommand::KittyTextSizing {
                scale: 1,
                width: 0,
                numerator: 0,
                denominator: 0,
                valign: KittyTextVerticalAlign::Top,
                halign: KittyTextHorizontalAlign::Left,
                text: "ok".to_string(),
            })
        );
        assert_eq!(
            parse(b"66;s=+2:s=-3:s=4=ignored:xx=1:h=1;ok"),
            Some(OwnedCommand::KittyTextSizing {
                scale: 4,
                width: 0,
                numerator: 0,
                denominator: 0,
                valign: KittyTextVerticalAlign::Top,
                halign: KittyTextHorizontalAlign::Right,
                text: "ok".to_string(),
            })
        );
    }

    #[test]
    fn osc_parser_kitty_text_sizing_safe_utf8_payload() {
        assert_eq!(
            parse("66;;👻魑魅魍魉ゴースッティ".as_bytes()),
            Some(OwnedCommand::KittyTextSizing {
                scale: 1,
                width: 0,
                numerator: 0,
                denominator: 0,
                valign: KittyTextVerticalAlign::Top,
                halign: KittyTextHorizontalAlign::Left,
                text: "👻魑魅魍魉ゴースッティ".to_string(),
            })
        );

        assert_eq!(parse(b"66;;\n"), None);
        assert_eq!(parse(b"66;;\x07bell"), None);
        assert_eq!(parse(b"66;;\x1b]9;bad\x1b\\"), None);
        assert_eq!(parse(b"66;;\x7f"), None);
        assert_eq!(parse("66;;\u{80}".as_bytes()), None);
        assert_eq!(parse(b"66"), None);
        assert_eq!(parse(b"66;params-without-payload"), None);
    }

    #[test]
    fn osc_parser_kitty_text_sizing_overlong_payload_rejects() {
        let mut input = b";".to_vec();
        input.resize(KITTY_TEXT_SIZING_MAX_PAYLOAD + 2, b'a');

        assert_eq!(parse_kitty_text_sizing(&input), None);
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
            parse(b"4;1;#ff0000;2;nosuchcolor;3;#00ff00"),
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
    fn osc_parser_palette_accepts_named_colors() {
        assert_eq!(
            parse(b"4;1;red"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetPalette {
                    index: 1,
                    rgb: super::super::color::Rgb::new(255, 0, 0),
                }],
            })
        );
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
    fn osc_parser_dynamic_color_set_query_operations() {
        assert_eq!(
            parse(b"10;#112233"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetDynamic {
                    target: DynamicColor::Foreground,
                    rgb: super::super::color::Rgb::new(0x11, 0x22, 0x33),
                }],
            })
        );
        assert_eq!(
            parse(b"11;#445566;#778899"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![
                    ColorRequest::SetDynamic {
                        target: DynamicColor::Background,
                        rgb: super::super::color::Rgb::new(0x44, 0x55, 0x66),
                    },
                    ColorRequest::SetDynamic {
                        target: DynamicColor::Cursor,
                        rgb: super::super::color::Rgb::new(0x77, 0x88, 0x99),
                    },
                ],
            })
        );
        assert_eq!(
            parse_with_terminator(b"12;?", Terminator::Bel),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::QueryDynamic {
                    target: DynamicColor::Cursor,
                    terminator: Terminator::Bel,
                }],
            })
        );
        assert_eq!(
            parse_with_terminator(b"12;?", Terminator::St),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::QueryDynamic {
                    target: DynamicColor::Cursor,
                    terminator: Terminator::St,
                }],
            })
        );
    }

    #[test]
    fn osc_parser_dynamic_color_sequences_advance_and_skip_empty_fields() {
        assert_eq!(
            parse(b"10;?;#010203;?"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![
                    ColorRequest::QueryDynamic {
                        target: DynamicColor::Foreground,
                        terminator: Terminator::St,
                    },
                    ColorRequest::SetDynamic {
                        target: DynamicColor::Background,
                        rgb: super::super::color::Rgb::new(1, 2, 3),
                    },
                    ColorRequest::QueryDynamic {
                        target: DynamicColor::Cursor,
                        terminator: Terminator::St,
                    },
                ],
            })
        );
        assert_eq!(
            parse(b"10;;#0a0b0c"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetDynamic {
                    target: DynamicColor::Foreground,
                    rgb: super::super::color::Rgb::new(0x0a, 0x0b, 0x0c),
                }],
            })
        );
    }

    #[test]
    fn osc_parser_dynamic_color_invalid_data_preserves_prior_requests() {
        assert_eq!(
            parse(b"10;#010203;nosuchcolor;#040506"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetDynamic {
                    target: DynamicColor::Foreground,
                    rgb: super::super::color::Rgb::new(1, 2, 3),
                }],
            })
        );
        assert_eq!(parse(b"10;nosuchcolor"), None);
    }

    #[test]
    fn osc_parser_dynamic_colors_accept_rgbi() {
        assert_eq!(
            parse(b"10;rgbi:1.0/0.5/0"),
            Some(OwnedCommand::ColorOperation {
                requests: vec![ColorRequest::SetDynamic {
                    target: DynamicColor::Foreground,
                    rgb: super::super::color::Rgb::new(255, 127, 0),
                }],
            })
        );
    }

    #[test]
    fn osc_parser_dynamic_reset_operations() {
        for (input, target) in [
            (b"110".as_slice(), DynamicColor::Foreground),
            (b"111".as_slice(), DynamicColor::Background),
            (b"112".as_slice(), DynamicColor::Cursor),
            (b"110;".as_slice(), DynamicColor::Foreground),
            (b"111;".as_slice(), DynamicColor::Background),
            (b"112;".as_slice(), DynamicColor::Cursor),
        ] {
            assert_eq!(
                parse(input),
                Some(OwnedCommand::ColorOperation {
                    requests: vec![ColorRequest::ResetDynamic { target }],
                })
            );
        }

        assert_eq!(parse(b"110;0"), None);
        assert_eq!(parse(b"111;?"), None);
        assert_eq!(parse(b"112;#ffffff"), None);
    }

    #[test]
    fn osc_parser_unsupported_dynamic_color_families_are_ignored() {
        assert_eq!(parse(b"13;#ffffff"), None);
        assert_eq!(parse(b"19;?"), None);
        assert_eq!(parse(b"113"), None);
        assert_eq!(parse(b"119"), None);
    }

    #[test]
    fn osc_parser_kitty_color_protocol_mixed_example() {
        use super::super::kitty::{ColorKind, ColorRequest, ColorSpecial};

        assert_eq!(
            parse(b"21;foreground=?;background=rgb:f0/f8/ff;cursor=aliceblue;cursor_text;visual_bell=;selection_foreground=#xxxyyzz;selection_background=?;selection_background=#aabbcc;2=?;3=rgbi:1.0/1.0/1.0"),
            Some(OwnedCommand::KittyColor {
                terminator: Terminator::St,
                requests: vec![
                    ColorRequest::Query(ColorKind::Special(ColorSpecial::Foreground)),
                    ColorRequest::Set {
                        key: ColorKind::Special(ColorSpecial::Background),
                        rgb: super::super::color::Rgb::new(0xf0, 0xf8, 0xff),
                    },
                    ColorRequest::Set {
                        key: ColorKind::Special(ColorSpecial::Cursor),
                        rgb: super::super::color::Rgb::new(0xf0, 0xf8, 0xff),
                    },
                    ColorRequest::Reset(ColorKind::Special(ColorSpecial::CursorText)),
                    ColorRequest::Reset(ColorKind::Special(ColorSpecial::VisualBell)),
                    ColorRequest::Query(ColorKind::Special(ColorSpecial::SelectionBackground)),
                    ColorRequest::Set {
                        key: ColorKind::Special(ColorSpecial::SelectionBackground),
                        rgb: super::super::color::Rgb::new(0xaa, 0xbb, 0xcc),
                    },
                    ColorRequest::Query(ColorKind::Palette(2)),
                    ColorRequest::Set {
                        key: ColorKind::Palette(3),
                        rgb: super::super::color::Rgb::new(0xff, 0xff, 0xff),
                    },
                ],
            })
        );
    }

    #[test]
    fn osc_parser_kitty_color_protocol_edge_cases() {
        use super::super::kitty::{ColorKind, ColorRequest, ColorSpecial};

        assert_eq!(
            parse(b"21;"),
            Some(OwnedCommand::KittyColor {
                requests: vec![],
                terminator: Terminator::St,
            })
        );
        assert_eq!(
            parse_with_terminator(
                b"21;;unknown=?;foreground= ? ;Foreground=?;+1=red;255=red;256=red;foreground=\t?\t",
                Terminator::Bel
            ),
            Some(OwnedCommand::KittyColor {
                terminator: Terminator::Bel,
                requests: vec![
                    ColorRequest::Query(ColorKind::Special(ColorSpecial::Foreground)),
                    ColorRequest::Set {
                        key: ColorKind::Palette(255),
                        rgb: super::super::color::Rgb::new(255, 0, 0),
                    },
                ],
            })
        );
    }

    #[test]
    fn osc_parser_kitty_color_protocol_cap_overflow_invalidates() {
        let mut input = b"21;".to_vec();
        for index in 0..=super::super::kitty::COLOR_REQUEST_CAPACITY {
            if index > 0 {
                input.push(b';');
            }
            input.push(b'0');
        }

        assert_eq!(parse(&input), None);
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
