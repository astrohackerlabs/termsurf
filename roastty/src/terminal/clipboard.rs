#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ClipboardContents<'a> {
    pub(super) kind: u8,
    pub(super) data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct KittyClipboard<'a> {
    pub(super) metadata: &'a [u8],
    pub(super) payload: Option<&'a [u8]>,
    pub(super) terminator: super::osc::Terminator,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Location {
    Primary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Operation {
    Read,
    WriteAlias,
    WriteData,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Data,
    Done,
    Busy,
    Invalid,
    Io,
    NotImplemented,
    Permission,
    Ok,
}

impl KittyClipboard<'_> {
    pub(super) fn id(&self) -> Option<&[u8]> {
        let value = read_option(self.metadata, b"id")?;
        is_valid_identifier(value).then_some(value)
    }

    pub(super) fn loc(&self) -> Option<Location> {
        match read_option(self.metadata, b"loc")? {
            b"primary" => Some(Location::Primary),
            _ => None,
        }
    }

    pub(super) fn mime(&self) -> Option<&[u8]> {
        read_option(self.metadata, b"mime")
    }

    pub(super) fn name(&self) -> Option<&[u8]> {
        read_option(self.metadata, b"name")
    }

    pub(super) fn password(&self) -> Option<&[u8]> {
        read_option(self.metadata, b"password")
    }

    pub(super) fn pw(&self) -> Option<&[u8]> {
        read_option(self.metadata, b"pw")
    }

    pub(super) fn status(&self) -> Option<Status> {
        match read_option(self.metadata, b"status")? {
            b"DATA" => Some(Status::Data),
            b"DONE" => Some(Status::Done),
            b"EBUSY" => Some(Status::Busy),
            b"EINVAL" => Some(Status::Invalid),
            b"EIO" => Some(Status::Io),
            b"ENOSYS" => Some(Status::NotImplemented),
            b"EPERM" => Some(Status::Permission),
            b"OK" => Some(Status::Ok),
            _ => None,
        }
    }

    pub(super) fn operation(&self) -> Option<Operation> {
        match read_option(self.metadata, b"type")? {
            b"read" => Some(Operation::Read),
            b"walias" => Some(Operation::WriteAlias),
            b"wdata" => Some(Operation::WriteData),
            b"write" => Some(Operation::Write),
            _ => None,
        }
    }
}

fn read_option<'a>(metadata: &'a [u8], key: &[u8]) -> Option<&'a [u8]> {
    let mut pos = 0;
    while pos < metadata.len() {
        while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= metadata.len() {
            return None;
        }

        if !metadata[pos..].starts_with(key) {
            pos = metadata[pos..].iter().position(|byte| *byte == b':')? + pos + 1;
            continue;
        }

        pos += key.len();
        while pos < metadata.len() && metadata[pos].is_ascii_whitespace() {
            pos += 1;
        }
        if pos >= metadata.len() || metadata[pos] != b'=' {
            return None;
        }

        let start = pos + 1;
        let end = metadata[start..]
            .iter()
            .position(|byte| *byte == b':')
            .map_or(metadata.len(), |idx| start + idx);
        return Some(trim_ascii_whitespace(&metadata[start..end]));
    }
    None
}

fn trim_ascii_whitespace(bytes: &[u8]) -> &[u8] {
    let mut start = 0;
    let mut end = bytes.len();
    while start < end && bytes[start].is_ascii_whitespace() {
        start += 1;
    }
    while end > start && bytes[end - 1].is_ascii_whitespace() {
        end -= 1;
    }
    &bytes[start..end]
}

fn is_valid_identifier(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|byte| matches!(byte, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'+' | b'.'))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn clip(metadata: &[u8]) -> KittyClipboard<'_> {
        KittyClipboard {
            metadata,
            payload: None,
            terminator: super::super::osc::Terminator::Bel,
        }
    }

    #[test]
    fn kitty_clipboard_options_read_raw_values() {
        let value = clip(
            b" type = read : mime = text/plain : name = file : password = secret : pw = short ",
        );

        assert_eq!(value.operation(), Some(Operation::Read));
        assert_eq!(value.mime(), Some(b"text/plain".as_slice()));
        assert_eq!(value.name(), Some(b"file".as_slice()));
        assert_eq!(value.password(), Some(b"secret".as_slice()));
        assert_eq!(value.pw(), Some(b"short".as_slice()));
    }

    #[test]
    fn kitty_clipboard_options_validate_enums_and_ids() {
        assert_eq!(
            clip(b"id=abcDEF012-_+.").id(),
            Some(b"abcDEF012-_+.".as_slice())
        );
        assert_eq!(clip(b"id=").id(), None);
        assert_eq!(clip(b"id=bad/slash").id(), None);
        assert_eq!(clip(b"loc=primary").loc(), Some(Location::Primary));
        assert_eq!(clip(b"loc=Primary").loc(), None);
        assert_eq!(clip(b"status=DATA").status(), Some(Status::Data));
        assert_eq!(clip(b"status=DONE").status(), Some(Status::Done));
        assert_eq!(clip(b"status=EBUSY").status(), Some(Status::Busy));
        assert_eq!(clip(b"status=EINVAL").status(), Some(Status::Invalid));
        assert_eq!(clip(b"status=EIO").status(), Some(Status::Io));
        assert_eq!(
            clip(b"status=ENOSYS").status(),
            Some(Status::NotImplemented)
        );
        assert_eq!(clip(b"status=EPERM").status(), Some(Status::Permission));
        assert_eq!(clip(b"status=OK").status(), Some(Status::Ok));
        assert_eq!(clip(b"status=ok").status(), None);
        assert_eq!(
            clip(b"type=walias").operation(),
            Some(Operation::WriteAlias)
        );
        assert_eq!(clip(b"type=wdata").operation(), Some(Operation::WriteData));
        assert_eq!(clip(b"type=write").operation(), Some(Operation::Write));
        assert_eq!(clip(b"type=READ").operation(), None);
    }
}
