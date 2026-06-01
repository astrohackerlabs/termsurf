//! DCS command handling.

use super::stream;

const DEFAULT_MAX_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Handler {
    state: State,
    max_bytes: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Command {
    XtGettcap(XtGettcap),
    Decrqss(Decrqss),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct XtGettcap {
    data: Vec<u8>,
    index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decrqss {
    None,
    Sgr,
    Decscusr,
    Decstbm,
    Decslrm,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum State {
    Inactive,
    Ignore,
    XtGettcap(Vec<u8>),
    Decrqss { data: [u8; 2], len: u8 },
}

impl Handler {
    pub(super) const fn new() -> Self {
        Self {
            state: State::Inactive,
            max_bytes: DEFAULT_MAX_BYTES,
        }
    }

    pub(super) fn hook(&mut self, dcs: stream::DcsHook) -> Option<Command> {
        self.state = State::Ignore;

        let command = match dcs.intermediates() {
            [] => match dcs.final_byte() {
                b'p' if dcs.params() == [1000] => None,
                _ => None,
            },
            [b'+'] => match dcs.final_byte() {
                b'q' => {
                    self.state = State::XtGettcap(Vec::with_capacity(128));
                    None
                }
                _ => None,
            },
            [b'$'] => match dcs.final_byte() {
                b'q' => {
                    self.state = State::Decrqss {
                        data: [0; 2],
                        len: 0,
                    };
                    None
                }
                _ => None,
            },
            _ => None,
        };

        if !matches!(self.state, State::XtGettcap(_) | State::Decrqss { .. }) {
            self.state = State::Ignore;
        }

        command
    }

    pub(super) fn put(&mut self, byte: u8) -> Option<Command> {
        match &mut self.state {
            State::Inactive | State::Ignore => {}
            State::XtGettcap(data) => {
                if data.len() >= self.max_bytes {
                    self.state = State::Ignore;
                    return None;
                }
                data.push(byte);
            }
            State::Decrqss { data, len } => {
                let index = usize::from(*len);
                if index >= data.len() {
                    self.state = State::Ignore;
                    return None;
                }
                data[index] = byte;
                *len += 1;
            }
        }
        None
    }

    pub(super) fn unhook(&mut self) -> Option<Command> {
        let state = std::mem::replace(&mut self.state, State::Inactive);
        match state {
            State::Inactive | State::Ignore => None,
            State::XtGettcap(mut data) => {
                data.make_ascii_uppercase();
                Some(Command::XtGettcap(XtGettcap { data, index: 0 }))
            }
            State::Decrqss { data, len } => Some(Command::Decrqss(match len {
                0 => Decrqss::None,
                1 => match data[0] {
                    b'm' => Decrqss::Sgr,
                    b'r' => Decrqss::Decstbm,
                    b's' => Decrqss::Decslrm,
                    _ => Decrqss::None,
                },
                2 => match data {
                    [b' ', b'q'] => Decrqss::Decscusr,
                    _ => Decrqss::None,
                },
                _ => unreachable!("DECRQSS buffer only stores two bytes"),
            })),
        }
    }

    #[cfg(test)]
    fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            max_bytes,
            ..Self::new()
        }
    }
}

impl Default for Handler {
    fn default() -> Self {
        Self::new()
    }
}

impl XtGettcap {
    pub(super) fn next(&mut self) -> Option<&[u8]> {
        if self.index >= self.data.len() {
            return None;
        }

        let remaining = &self.data[self.index..];
        let len = remaining
            .iter()
            .position(|byte| *byte == b';')
            .unwrap_or(remaining.len());
        self.index += len + 1;
        Some(&remaining[..len])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hook(intermediates: &[u8], params: &[u16], final_byte: u8) -> stream::DcsHook {
        stream::DcsHook::new_for_tests(intermediates, params, final_byte)
    }

    #[test]
    fn terminal_dcs_unknown_command_enters_ignore() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"", &[], b'A')), None);
        assert_eq!(handler.put(b'x'), None);
        assert_eq!(handler.unhook(), None);
        assert_eq!(handler.state, State::Inactive);
    }

    #[test]
    fn terminal_dcs_xtgettcap_command() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"+", &[], b'q')), None);
        for byte in b"536D756C78" {
            assert_eq!(handler.put(*byte), None);
        }
        let Some(Command::XtGettcap(mut command)) = handler.unhook() else {
            panic!("expected XTGETTCAP command");
        };
        assert_eq!(command.next(), Some(b"536D756C78".as_slice()));
        assert_eq!(command.next(), None);
    }

    #[test]
    fn terminal_dcs_xtgettcap_mixed_case_is_uppercased() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"+", &[], b'q')), None);
        for byte in b"536d756C78" {
            assert_eq!(handler.put(*byte), None);
        }
        let Some(Command::XtGettcap(mut command)) = handler.unhook() else {
            panic!("expected XTGETTCAP command");
        };
        assert_eq!(command.next(), Some(b"536D756C78".as_slice()));
        assert_eq!(command.next(), None);
    }

    #[test]
    fn terminal_dcs_xtgettcap_multiple_keys() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"+", &[], b'q')), None);
        for byte in b"536D756C78;536D756C79" {
            assert_eq!(handler.put(*byte), None);
        }
        let Some(Command::XtGettcap(mut command)) = handler.unhook() else {
            panic!("expected XTGETTCAP command");
        };
        assert_eq!(command.next(), Some(b"536D756C78".as_slice()));
        assert_eq!(command.next(), Some(b"536D756C79".as_slice()));
        assert_eq!(command.next(), None);
    }

    #[test]
    fn terminal_dcs_xtgettcap_invalid_looking_data_is_returned() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"+", &[], b'q')), None);
        for byte in b"who;536D756C78" {
            assert_eq!(handler.put(*byte), None);
        }
        let Some(Command::XtGettcap(mut command)) = handler.unhook() else {
            panic!("expected XTGETTCAP command");
        };
        assert_eq!(command.next(), Some(b"WHO".as_slice()));
        assert_eq!(command.next(), Some(b"536D756C78".as_slice()));
        assert_eq!(command.next(), None);
    }

    #[test]
    fn terminal_dcs_xtgettcap_over_capacity_is_ignored() {
        let mut handler = Handler::with_max_bytes(2);

        assert_eq!(handler.hook(hook(b"+", &[], b'q')), None);
        assert_eq!(handler.put(b'a'), None);
        assert_eq!(handler.put(b'b'), None);
        assert_eq!(handler.put(b'c'), None);
        assert_eq!(handler.unhook(), None);
    }

    #[test]
    fn terminal_dcs_decrqss_commands() {
        for (payload, expected) in [
            (b"m".as_slice(), Decrqss::Sgr),
            (b"r", Decrqss::Decstbm),
            (b"s", Decrqss::Decslrm),
            (b" q", Decrqss::Decscusr),
            (b"z", Decrqss::None),
            (b"", Decrqss::None),
        ] {
            let mut handler = Handler::new();
            assert_eq!(handler.hook(hook(b"$", &[], b'q')), None);
            for byte in payload {
                assert_eq!(handler.put(*byte), None);
            }
            assert_eq!(handler.unhook(), Some(Command::Decrqss(expected)));
        }
    }

    #[test]
    fn terminal_dcs_decrqss_over_capacity_is_ignored() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"$", &[], b'q')), None);
        assert_eq!(handler.put(b'"'), None);
        assert_eq!(handler.put(b' '), None);
        assert_eq!(handler.put(b'q'), None);
        assert_eq!(handler.unhook(), None);
    }

    #[test]
    fn terminal_dcs_tmux_control_mode_is_ignored() {
        let mut handler = Handler::new();

        assert_eq!(handler.hook(hook(b"", &[1000], b'p')), None);
        assert_eq!(handler.put(b'%'), None);
        assert_eq!(handler.unhook(), None);
    }
}
