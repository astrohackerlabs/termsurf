+++
[implementer]
agent = "claude-code"
model = "claude-opus-4-8"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5.5"
reasoning = "medium"
+++

# Experiment 609: os string_encoding (printf %q + URL percent decode/encode)

## Description

The search `Thread`'s remaining piece (the outer libxev event loop) is blocked
on a libxev port, so this experiment pivots to a self-contained,
**dependency-free** module that roastty has not yet ported: upstream
`os/string_encoding.zig`. It provides three byte-string transforms used by shell
integration / OSC handling:

- `printfQDecode` — decode a string encoded the way `bash`'s `printf %q` encodes
  it (strip `$'…'` / `'…'` quoting; handle `\`-escapes).
- `urlPercentDecode` — URL percent-decode (`%XX`).
- `urlPercentEncode` — URL percent-encode (delegates to Zig's
  `std.Uri.Component.percentEncode`, which is a trivial "valid byte → as-is,
  else `%XX`" loop that roastty inlines, avoiding a URI dependency).

It is fully self-contained (no `Terminal`, no `unsafe`, no external crates) and
comes with ~20 upstream tests to mirror exactly.

## Upstream behavior (`string_encoding.zig`)

- `printfQDecode(writer, buf)`:
  - Strip quoting: `$'…'` (require `len>=3` and trailing `'`, else
    `DecodeError`) → inner; `'…'` (require `len>=2` and trailing `'`) → inner;
    otherwise the whole buffer.
  - Walk the data: a `\` consumes the next byte — ` \ " ' $` map to themselves;
    `e`→ESC(0x1b), `n`→LF(0x0a), `r`→CR(0x0d), `t`→HT(0x09), `v`→VT(0x0b); any
    other escape, or a trailing `\`, → `DecodeError`. Every other byte is
    copied.
- `urlPercentDecode(writer, buf)`: a `%` requires two following hex digits
  (`src + 2 >= buf.len` → `DecodeError`; non-hex → `DecodeError`), decoded as
  `hex(h1)<<4 | hex(h2)`; every other byte is copied.
- `hex(c)`: `0-9`/`a-f`/`A-F` → value; else `unreachable` (callers pre-check).
- `isValidChar(c)`: `false` for space / `;` / `=`, else `std.ascii.isPrint(c)`
  (printable ASCII, `0x20..=0x7e`).
- `urlPercentEncode(writer, data)` =
  `std.Uri.Component.percentEncode(writer, data, isValidChar)`: copy each byte
  for which `isValidChar` holds, else emit `%` + the byte as two **uppercase**
  hex digits.

## Rust mapping (`roastty/src/os/string_encoding.rs`, new file)

The Zig `*std.Io.Writer` output becomes a `&mut Vec<u8>` (the transforms emit
raw bytes, which may be non-UTF-8 after decoding). `std.Io.Writer.Error` is
unreachable for an in-memory `Vec`, so the only error is the decode error.

```rust
//! Byte-string encodings used by shell integration (port of upstream `os/string_encoding`).

/// A malformed-input error from the decoders (upstream `error{DecodeError}`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DecodeError;

/// Decode a string encoded like `bash`'s `printf %q`, appending to `out` (upstream `printfQDecode`).
/// On error, `out` may have been partially written (matching upstream).
pub(crate) fn printf_q_decode(buf: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    let data: &[u8] = if let Some(rest) = buf.strip_prefix(b"$'") {
        // `$'…'`
        if buf.len() < 3 || !buf.ends_with(b"'") {
            return Err(DecodeError);
        }
        rest.strip_suffix(b"'").ok_or(DecodeError)?
    } else if let Some(rest) = buf.strip_prefix(b"'") {
        // `'…'`
        if buf.len() < 2 || !buf.ends_with(b"'") {
            return Err(DecodeError);
        }
        rest.strip_suffix(b"'").ok_or(DecodeError)?
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

/// URL percent-decode `buf`, appending to `out` (upstream `urlPercentDecode`).
pub(crate) fn url_percent_decode(buf: &[u8], out: &mut Vec<u8>) -> Result<(), DecodeError> {
    let mut src = 0;
    while src < buf.len() {
        match buf[src] {
            b'%' => {
                // Two hex digits must follow.
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
/// `std.Uri.Component.percentEncode`, inlined to avoid a URI dependency).
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

/// Hex digit → value (upstream `hex`). Callers pre-check, so non-hex is unreachable.
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

/// Whether `c` is left as-is by percent-encoding (upstream `isValidChar`): not space/`;`/`=`, and
/// printable ASCII.
fn is_valid_char(c: u8) -> bool {
    match c {
        b' ' | b';' | b'=' => false,
        _ => (0x20..=0x7e).contains(&c),
    }
}
```

Registered in `os/mod.rs` as `pub(crate) mod string_encoding;` (the module's
`#![allow(dead_code)]` covers the not-yet-wired entry points).

### Notes / deviations

- `&mut Vec<u8>` replaces `*std.Io.Writer`; the infallible writer-error arm
  disappears, leaving only `DecodeError`.
- `urlPercentEncode` inlines `std.Uri.Component.percentEncode` (valid byte →
  copy, else `%` + two **uppercase** hex digits — Zig's `{X:0>2}` formatting),
  so no URI/URL crate is pulled in.
- `is_ascii_hexdigit()` replaces upstream's explicit `0-9/a-f/A-F` ranges in the
  percent decoder's guard; `hex` keeps the explicit ranges (and `unreachable!`)
  to mirror upstream exactly.

## Verification

- `cargo build -p roastty` — no warnings.
- `cargo test -p roastty` — no regressions; new tests mirror all upstream cases:
  - **printf %q**: `\ `→space, `\n`→LF, `\r`/`\t`/`\v`/`\e`, the literal escapes
    (`\\`, `\"`, `\'`, `\$`); `$'…'` and `'…'` stripping; the error cases (`\d`,
    trailing `\`, unterminated `$'…'` / `'…'`, lone `$'`, `'`).
  - **percent decode**: every byte `0..=255` round-trips via `%xx` and `%XX`;
    `%20`→space, multiple, and the error cases (`%2k`, `%`, `%%`, trailing `%2`
    / `%`).
  - **percent encode**: `is_valid_char` table; space→`%20`, `;`→`%3B`,
    `=`→`%3D`; an encode→decode round-trip over a mixed buffer.
- `cargo fmt -p roastty -- --check` — clean.
- no-ghostty grep on `os/string_encoding.rs` / `os/mod.rs` — clean.
- `git diff --check` — clean.

Pass = all three transforms match upstream byte-for-byte on the mirrored test
corpus, including every error case.

## Design Review

Codex reviewed the design and **APPROVED** it with **no Required, Optional, or
Nit findings**, confirming: `printf_q_decode` matches upstream's quote-stripping
and escape handling (including the malformed-quote / trailing-backslash / error
cases and the partial-write behavior); `url_percent_decode` preserves the exact
two-following-byte requirement and hex validation before `hex`;
`url_percent_encode` as an inline valid-byte-copy-else-`%XX` loop is the right
dependency-free port and uppercase hex is the expected Zig mapping;
`is_valid_char` correctly models printable ASCII while excluding space/`;`/`=`;
`&mut Vec<u8>` is the right writer adaptation (writer errors dropped as
impossible); and `DecodeError` as a unit struct is idiomatic. The planned tests
cover the upstream corpus plus round-trip/table coverage.

Review artifacts:

- Prompt: `logs/codex-review/20260605-d609-prompt.md`
- Result: `logs/codex-review/20260605-d609-last-message.md`

## Result

**Result:** Pass

Implemented `roastty/src/os/string_encoding.rs` (registered in `os/mod.rs`),
porting all of upstream `os/string_encoding.zig`: `printf_q_decode`,
`url_percent_decode`, `url_percent_encode`, and the `hex` / `upper_hex` /
`is_valid_char` helpers + `DecodeError`. Output appends to a `&mut Vec<u8>` (the
Zig `*std.Io.Writer`), the infallible writer-error arm drops, and
`url_percent_encode` inlines `std.Uri.Component.percentEncode` (uppercase `%XX`)
so no URI/URL crate is pulled in. Fully self-contained — no `Terminal`, no
`unsafe`, no new dependencies.

Twenty-one tests mirror the upstream corpus (every `printf %q` escape and quote
case incl. all errors; every byte `0..=255` percent-round-trip in both hex
cases; the percent-decode error cases; the encode table + an encode→decode
round-trip). Gates: `cargo fmt --check` clean, `cargo build -p roastty` no
warnings, `cargo test -p roastty` **3355 passed / 0 failed** (3334 → 3355, +21),
no-ghostty grep clean, `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **APPROVED** it with **no Required,
Optional, or Nit findings** (beyond recording this result), confirming the port
is faithful byte-for-byte: the `$'…'` / `'…'` stripping guards and slice bounds
(incl. unterminated/lone-quote errors); the escape handling (literal, control,
trailing-backslash, invalid); the `%` bound check and hex validation in
`url_percent_decode`; uppercase-hex emission and valid-char pass-through in
`url_percent_encode`; the `is_valid_char` printable-ASCII model; and the
partial-output-on-error behavior. The test coverage includes the upstream corpus
plus useful supersets.

Review artifacts:

- Prompt: `logs/codex-review/20260605-r609-prompt.md`
- Result: `logs/codex-review/20260605-r609-last-message.md`

## Conclusion

`os/string_encoding` is fully ported — a clean, dependency-free slice taken
while the search subsystem's last piece (the outer libxev `Thread`) stays
blocked. roastty's `os/` still lacks several upstream modules (`locale`,
`kernel_info`, `resourcesdir`, `args`, …) and `datastruct/` lacks `comparison` /
`array_list_collection`; these self-contained, unblocked modules are the natural
next slices for Issue 801 while the libxev, regex/oniguruma, and URI (`std.Uri`)
dependency boundaries remain.
