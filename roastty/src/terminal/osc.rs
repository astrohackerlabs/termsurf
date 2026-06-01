pub(super) const MAX_BUF: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Command<'a> {
    WindowTitle { title: &'a str },
    ReportPwd { url: &'a str },
    StartHyperlink { id: Option<&'a str>, uri: &'a str },
    EndHyperlink,
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

    pub(super) fn command(&self) -> Option<Command<'_>> {
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
            b"7" => valid_utf8(rest).map(|url| Command::ReportPwd { url }),
            b"8" => parse_hyperlink(rest),
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
            }
        }
    }

    fn parse(input: &[u8]) -> Option<OwnedCommand> {
        let mut parser = Parser::new();
        for &byte in input {
            parser.push(byte);
        }
        parser.command().map(OwnedCommand::from)
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
    fn osc_parser_over_capacity_invalidates() {
        let mut parser = Parser::new();
        for _ in 0..MAX_BUF + 1 {
            parser.push(b'a');
        }
        assert_eq!(parser.command(), None);
    }
}
