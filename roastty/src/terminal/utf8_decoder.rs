//! A DFA-based, non-allocating, error-replacing UTF-8 decoder (port of upstream
//! `terminal/UTF8Decoder`).
//!
//! Based on Bjoern Hoehrmann's DFA decoder (http://bjoern.hoehrmann.de/utf-8/decoder/dfa, MIT),
//! with error replacement. The two lookup tables are copied verbatim from upstream.

#[rustfmt::skip]
const CHAR_CLASSES: [u8; 256] = [
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,  0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,
   1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,1,  9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,9,
   7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,  7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,7,
   8,8,2,2,2,2,2,2,2,2,2,2,2,2,2,2,  2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,2,
  10,3,3,3,3,3,3,3,3,3,3,3,3,4,3,3, 11,6,6,6,5,8,8,8,8,8,8,8,8,8,8,8,
];

#[rustfmt::skip]
const TRANSITIONS: [u8; 108] = [
   0,12,24,36,60,96,84,12,12,12,48,72, 12,12,12,12,12,12,12,12,12,12,12,12,
  12, 0,12,12,12,12,12, 0,12, 0,12,12, 12,24,12,12,12,12,12,24,12,24,12,12,
  12,12,12,12,12,12,12,24,12,12,12,12, 12,24,12,12,12,12,12,12,12,24,12,12,
  12,12,12,12,12,12,12,36,12,36,12,12, 12,36,12,12,12,12,12,36,12,36,12,12,
  12,36,12,12,12,12,12,12,12,12,12,12,
];

const ACCEPT_STATE: u8 = 0;
const REJECT_STATE: u8 = 12;

/// A DFA-based error-replacing UTF-8 decoder (upstream `UTF8Decoder`).
#[derive(Debug, Default)]
pub(crate) struct Utf8Decoder {
    accumulator: u32, // the codepoint under construction (upstream `u21`)
    state: u8,        // the DFA state (starts at ACCEPT_STATE == 0, the `Default`)
}

impl Utf8Decoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Feed the next byte. Returns the decoded codepoint (if a full one or a replacement was
    /// produced) and whether the byte was consumed. An un-consumed byte must be re-fed (it begins
    /// a new sequence after an ill-formed continuation).
    pub(crate) fn next(&mut self, byte: u8) -> (Option<u32>, bool) {
        let char_class = CHAR_CLASSES[byte as usize];
        let initial_state = self.state;

        if self.state != ACCEPT_STATE {
            self.accumulator <<= 6;
            self.accumulator |= (byte & 0x3F) as u32;
        } else {
            self.accumulator = (0xFF_u32 >> char_class) & (byte as u32);
        }

        self.state = TRANSITIONS[(self.state + char_class) as usize];

        if self.state == ACCEPT_STATE {
            let cp = self.accumulator;
            self.accumulator = 0;
            (Some(cp), true)
        } else if self.state == REJECT_STATE {
            self.accumulator = 0;
            self.state = ACCEPT_STATE;
            // Replacement char. Consumed iff we rejected the first byte of a sequence.
            (Some(0xFFFD), initial_state == ACCEPT_STATE)
        } else {
            (None, true)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii() {
        let mut d = Utf8Decoder::new();
        let mut out = Vec::new();
        for &byte in b"Hello, World!" {
            let (cp, consumed) = d.next(byte);
            assert!(consumed);
            if let Some(cp) = cp {
                out.push(cp as u8);
            }
        }
        assert_eq!(out, b"Hello, World!");
    }

    #[test]
    fn well_formed_utf8() {
        let mut d = Utf8Decoder::new();
        let mut out = Vec::new();
        // 4-byte, 3-byte, 2-byte, 1-byte sequences.
        for &byte in "😄✤ÁA".as_bytes() {
            // No errors in this sequence, so every byte is consumed on the first try.
            let (cp, consumed) = d.next(byte);
            assert!(consumed);
            if let Some(cp) = cp {
                out.push(cp);
            }
        }
        assert_eq!(out, vec![0x1F604, 0x2724, 0xC1, 0x41]);
    }

    #[test]
    fn partially_invalid_utf8() {
        let mut d = Utf8Decoder::new();
        let mut out = Vec::new();
        let mut saw_unconsumed = false;
        // Illegally terminated sequence, valid sequence, illegal surrogate pair.
        let input: &[u8] = b"\xF0\x9F\xF0\x9F\x98\x84\xED\xA0\x80";
        for &byte in input {
            // Re-feed an un-consumed byte until it is accepted, as the caller contract requires.
            loop {
                let (cp, consumed) = d.next(byte);
                if let Some(cp) = cp {
                    out.push(cp);
                }
                if consumed {
                    break;
                }
                saw_unconsumed = true;
            }
        }
        assert_eq!(out, vec![0xFFFD, 0x1F604, 0xFFFD, 0xFFFD, 0xFFFD]);
        // The truncated lead byte forces at least one re-feed (un-consumed) case.
        assert!(saw_unconsumed);
    }
}
