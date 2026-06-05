//! Byte-string encodings used by shell integration / OSC handling (port of upstream
//! `os/string_encoding`). Three transforms: decode a `bash` `printf %q` string, URL percent-decode,
//! and URL percent-encode. All are self-contained and dependency-free; output is appended to a
//! `&mut Vec<u8>` (the Zig `*std.Io.Writer`), and decoded bytes may be non-UTF-8.

/// A malformed-input error from the decoders (upstream `error{DecodeError}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodeError;

/// Decode a string encoded the way `bash`'s `printf %q` encodes it, appending to `out` (upstream
/// `printfQDecode`). Strips `$'…'` / `'…'` quoting and resolves `\`-escapes. On error, `out` may
/// have been partially written (matching upstream's "garbage may have been written").
pub(crate) fn printf_q_decode(buf: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    // Strip off `$'…'` or `'…'` quoting.
    let data: &[u8] = if buf.starts_with(b"$'") {
        if buf.len() < 3 || !buf.ends_with(b"'") {
            return Err(DecodeError);
        }
        &buf[2..buf.len() - 1]
    } else if buf.starts_with(b"'") {
        if buf.len() < 2 || !buf.ends_with(b"'") {
            return Err(DecodeError);
        }
        &buf[1..buf.len() - 1]
    } else {
        buf
    };

    let mut src = 0;
    while src < data.len() {
        match data[src] {
            b'\\' => {
                if src + 1 >= data.len() {
                    return Err(DecodeError);
                }
                let decoded = match data[src + 1] {
                    c @ (b' ' | b'\\' | b'"' | b'\'' | b'$') => c,
                    b'e' => 0x1b,
                    b'n' => b'\n',
                    b'r' => b'\r',
                    b't' => b'\t',
                    b'v' => 0x0b,
                    _ => return Err(DecodeError),
                };
                out.push(decoded);
                src += 2;
            }
            c => {
                out.push(c);
                src += 1;
            }
        }
    }
    Ok(())
}

/// URL percent-decode `buf`, appending to `out` (upstream `urlPercentDecode`). A `%` requires two
/// following hex digits.
pub(crate) fn url_percent_decode(buf: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    let mut src = 0;
    while src < buf.len() {
        match buf[src] {
            b'%' => {
                // Both following bytes must exist and be hex digits.
                if src + 2 >= buf.len() {
                    return Err(DecodeError);
                }
                let (h1, h2) = (buf[src + 1], buf[src + 2]);
                if !h1.is_ascii_hexdigit() || !h2.is_ascii_hexdigit() {
                    return Err(DecodeError);
                }
                out.push((hex(h1) << 4) | hex(h2));
                src += 3;
            }
            c => {
                out.push(c);
                src += 1;
            }
        }
    }
    Ok(())
}

/// URL percent-encode `data`, appending to `out` (upstream `urlPercentEncode` +
/// `std.Uri.Component.percentEncode`, inlined to avoid a URI dependency): valid bytes are copied,
/// others become `%` + two uppercase hex digits.
pub(crate) fn url_percent_encode(data: &[u8], out: &mut Vec<u8>) {
    for &c in data {
        if is_valid_char(c) {
            out.push(c);
        } else {
            out.push(b'%');
            out.push(upper_hex(c >> 4));
            out.push(upper_hex(c & 0xf));
        }
    }
}

/// Hex digit → value (upstream `hex`). Callers pre-check, so a non-hex byte is unreachable.
fn hex(c: u8) -> u8 {
    match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => unreachable!("hex called on a non-hex byte"),
    }
}

/// A `0..=15` nibble → its uppercase hex ASCII byte.
fn upper_hex(nibble: u8) -> u8 {
    match nibble {
        0..=9 => b'0' + nibble,
        _ => b'A' + (nibble - 10),
    }
}

/// Whether `c` is left as-is by percent-encoding (upstream `isValidChar`): not space / `;` / `=`,
/// and printable ASCII.
fn is_valid_char(c: u8) -> bool {
    match c {
        b' ' | b';' | b'=' => false,
        _ => (0x20..=0x7e).contains(&c),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_q(s: &[u8]) -> Result<Vec<u8>, DecodeError> {
        let mut out = Vec::new();
        printf_q_decode(s, &mut out)?;
        Ok(out)
    }

    fn decode_pct(s: &[u8]) -> Result<Vec<u8>, DecodeError> {
        let mut out = Vec::new();
        url_percent_decode(s, &mut out)?;
        Ok(out)
    }

    fn encode_pct(s: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        url_percent_encode(s, &mut out);
        out
    }

    #[test]
    fn printf_q_escaped_space() {
        assert_eq!(decode_q(b"bobr\\ kurwa").unwrap(), b"bobr kurwa");
    }

    #[test]
    fn printf_q_escaped_newline() {
        assert_eq!(decode_q(b"bobr\\nkurwa").unwrap(), b"bobr\nkurwa");
    }

    #[test]
    fn printf_q_control_escapes() {
        assert_eq!(decode_q(b"a\\eb").unwrap(), b"a\x1bb");
        assert_eq!(decode_q(b"a\\rb").unwrap(), b"a\rb");
        assert_eq!(decode_q(b"a\\tb").unwrap(), b"a\tb");
        assert_eq!(decode_q(b"a\\vb").unwrap(), b"a\x0bb");
    }

    #[test]
    fn printf_q_literal_escapes() {
        assert_eq!(decode_q(b"a\\\\b").unwrap(), b"a\\b");
        assert_eq!(decode_q(b"a\\\"b").unwrap(), b"a\"b");
        assert_eq!(decode_q(b"a\\'b").unwrap(), b"a'b");
        assert_eq!(decode_q(b"a\\$b").unwrap(), b"a$b");
    }

    #[test]
    fn printf_q_invalid_escape_errors() {
        assert_eq!(decode_q(b"bobr\\dkurwa"), Err(DecodeError));
    }

    #[test]
    fn printf_q_trailing_backslash_errors() {
        assert_eq!(decode_q(b"bobr kurwa\\"), Err(DecodeError));
    }

    #[test]
    fn printf_q_dollar_quote_stripped() {
        assert_eq!(decode_q(b"$'bobr kurwa'").unwrap(), b"bobr kurwa");
    }

    #[test]
    fn printf_q_single_quote_stripped() {
        assert_eq!(decode_q(b"'bobr kurwa'").unwrap(), b"bobr kurwa");
    }

    #[test]
    fn printf_q_unterminated_dollar_quote_errors() {
        assert_eq!(decode_q(b"$'bobr kurwa"), Err(DecodeError));
    }

    #[test]
    fn printf_q_lone_dollar_quote_errors() {
        assert_eq!(decode_q(b"$'"), Err(DecodeError));
    }

    #[test]
    fn printf_q_unterminated_single_quote_errors() {
        assert_eq!(decode_q(b"'bobr kurwa"), Err(DecodeError));
    }

    #[test]
    fn printf_q_lone_single_quote_errors() {
        assert_eq!(decode_q(b"'"), Err(DecodeError));
    }

    #[test]
    fn printf_q_empty_quotes_decode_to_empty() {
        assert_eq!(decode_q(b"''").unwrap(), b"");
        assert_eq!(decode_q(b"$''").unwrap(), b"");
    }

    #[test]
    fn percent_decode_every_byte_roundtrips_both_cases() {
        for c in 0u8..=255 {
            let lower = format!("%{c:02x}");
            assert_eq!(decode_pct(lower.as_bytes()).unwrap(), vec![c]);
            let upper = format!("%{c:02X}");
            assert_eq!(decode_pct(upper.as_bytes()).unwrap(), vec![c]);
        }
    }

    #[test]
    fn percent_decode_space() {
        assert_eq!(decode_pct(b"bobr%20kurwa").unwrap(), b"bobr kurwa");
    }

    #[test]
    fn percent_decode_multiple() {
        assert_eq!(decode_pct(b"bobr%20kurwa%20").unwrap(), b"bobr kurwa ");
    }

    #[test]
    fn percent_decode_errors() {
        assert_eq!(decode_pct(b"bobr%2kurwa"), Err(DecodeError)); // non-hex second digit
        assert_eq!(decode_pct(b"bobr%kurwa"), Err(DecodeError)); // non-hex first digit
        assert_eq!(decode_pct(b"bobr%%kurwa"), Err(DecodeError)); // `%` is not hex
        assert_eq!(decode_pct(b"bobr%20kurwa%2"), Err(DecodeError)); // truncated at end
        assert_eq!(decode_pct(b"bobr%20kurwa%"), Err(DecodeError)); // lone trailing `%`
    }

    #[test]
    fn percent_encode_invalid_chars() {
        assert_eq!(encode_pct(b" "), b"%20");
        assert_eq!(encode_pct(b";"), b"%3B");
        assert_eq!(encode_pct(b"="), b"%3D");
        // A control byte and a high byte are both encoded.
        assert_eq!(encode_pct(b"\x00"), b"%00");
        assert_eq!(encode_pct(&[0xff]), b"%FF");
    }

    #[test]
    fn percent_encode_valid_chars_pass_through() {
        assert_eq!(encode_pct(b"abcXYZ0189/:_-.~"), b"abcXYZ0189/:_-.~");
    }

    #[test]
    fn percent_encode_then_decode_roundtrips() {
        let original: &[u8] = b"a b;c=d/e?f#g\x00\xff";
        let encoded = encode_pct(original);
        let mut decoded = Vec::new();
        url_percent_decode(&encoded, &mut decoded).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn is_valid_char_table() {
        assert!(!is_valid_char(b' '));
        assert!(!is_valid_char(b';'));
        assert!(!is_valid_char(b'='));
        assert!(!is_valid_char(0x1f)); // below printable
        assert!(!is_valid_char(0x7f)); // DEL
        assert!(!is_valid_char(0x80)); // high byte
        assert!(is_valid_char(b'a'));
        assert!(is_valid_char(b'~'));
        assert!(is_valid_char(b'!'));
    }
}
