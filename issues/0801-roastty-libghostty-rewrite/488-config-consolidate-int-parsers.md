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

# Experiment 488: consolidate the config integer parsers into one parse_uint

## Description

Experiments 478, 481, and 487 each ported a faithful slice of Zig's
`std.fmt.parseInt` for a different unsigned target — `parse_u8_with_sign` /
`char_to_digit` (base-0 `u8`, for the palette key), `parse_u32_dec` (base-10
`u32`, for window padding), and `parse_u21_hex` (base-16 `u21`, for the
clipboard codepoint map). They are near-identical (sign handling, interior-only
underscores, per-step overflow). This experiment **consolidates** them into a
single faithful generic `parse_uint(buf, base, max)` and reimplements the three
existing wrappers in terms of it. This is a behavior-preserving refactor: the
wrappers keep their signatures, so their existing tests (palette-key edges,
window-padding edges, clipboard-codepoint edges) are the regression guard. No
config value type's behavior changes.

## Upstream behavior

The generic is the unsigned subset of Zig's `std.fmt.parseInt` /
`parseIntWithSign` / `charToDigit` (verified against the Zig 0.16 `fmt.zig`
source, already ported piecewise in Experiments 478/481/487):

- An optional leading `+`/`-` sign. For an unsigned target, `-0` is `0` and any
  negative nonzero is overflow.
- When `base == 0`, auto-detect the base from a case-insensitive `0x` / `0o` /
  `0b` prefix (only when a digit follows it — i.e. `len > 2`); otherwise the
  fixed `base` is used (no prefix detection).
- `_` separators are allowed only **between** digits (a leading or trailing `_`
  is invalid).
- Digits accumulate in the base; a value above the target's max (or a digit `>=`
  the base, or a non-alphanumeric) is an error. Zig distinguishes
  `error.Overflow` from `error.InvalidCharacter`.

The three current call sites — `Palette` (needs the `Overflow` vs `Invalid`
distinction), `WindowPadding`, and `RepeatableClipboardCodepointMap` (both
collapse every error to `InvalidValue`) — keep their exact current behavior.

## Rust mapping (`roastty/src/config/mod.rs`)

Add a shared error and the generic parser:

```rust
/// An integer parse error (upstream Zig `parseInt`): a non-digit / bad form, or a
/// value exceeding the target's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IntParseError {
    Invalid,
    Overflow,
}

/// Parse an unsigned integer (the unsigned subset of Zig `std.fmt.parseInt`).
/// `base == 0` auto-detects a case-insensitive `0x`/`0o`/`0b` prefix (requiring a
/// following digit); otherwise the fixed `base` is used. An optional `+`/`-` sign
/// (`-0` → `0`, negative nonzero → `Overflow`), interior-only `_` separators
/// (leading/trailing `_` → `Invalid`), per-step accumulation, and a value above
/// `max` → `Overflow`. A non-digit / digit `>=` base is `Invalid`.
fn parse_uint(buf: &str, base: u32, max: u64) -> Result<u64, IntParseError> {
    let (neg, rest): (bool, &str) = match buf.as_bytes().first() {
        Some(b'+') => (false, &buf[1..]),
        Some(b'-') => (true, &buf[1..]),
        _ => (false, buf),
    };

    let mut radix = base;
    let mut bytes = rest.as_bytes();
    if base == 0 {
        radix = 10;
        if bytes.len() > 2 && bytes[0] == b'0' {
            match bytes[1].to_ascii_lowercase() {
                b'b' => (radix, bytes) = (2, &bytes[2..]),
                b'o' => (radix, bytes) = (8, &bytes[2..]),
                b'x' => (radix, bytes) = (16, &bytes[2..]),
                _ => {}
            }
        }
    }

    if bytes.is_empty() || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return Err(IntParseError::Invalid);
    }

    let limit = max as i128;
    let mut acc: i128 = 0;
    for &c in bytes {
        if c == b'_' {
            continue;
        }
        let digit = (c as char)
            .to_digit(radix)
            .ok_or(IntParseError::Invalid)? as i128;
        if acc != 0 {
            acc = acc
                .checked_mul(radix as i128)
                .filter(|&v| v <= limit)
                .ok_or(IntParseError::Overflow)?;
        } else if neg {
            // First digit of a negative number: only `-0` survives for unsigned.
            acc = -digit;
            if acc < 0 {
                return Err(IntParseError::Overflow);
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=limit).contains(&acc) {
            return Err(IntParseError::Overflow);
        }
    }
    Ok(acc as u64)
}
```

Reimplement the three wrappers (their signatures and call sites stay unchanged):

```rust
/// Base-0 `u8` palette key (upstream `parseInt(u8, _, 0)`).
fn parse_palette_key(buf: &str) -> Result<u8, PaletteParseError> {
    match parse_uint(buf, 0, 0xFF) {
        Ok(v) => Ok(v as u8),
        Err(IntParseError::Overflow) => Err(PaletteParseError::Overflow),
        Err(IntParseError::Invalid) => Err(PaletteParseError::InvalidValue),
    }
}

/// Base-10 `u32` (upstream `parseInt(u32, _, 10)`); every error → `None`.
fn parse_u32_dec(buf: &str) -> Option<u32> {
    parse_uint(buf, 10, u32::MAX as u64).ok().map(|v| v as u32)
}

/// Base-16 `u21` (upstream `parseInt(u21, _, 16)`); every error → `None`.
fn parse_u21_hex(buf: &str) -> Option<u32> {
    parse_uint(buf, 16, 0x1FFFFF).ok().map(|v| v as u32)
}
```

`parse_u8_with_sign` and `char_to_digit` are removed (subsumed by `parse_uint`;
`to_digit(radix)` is the faithful `charToDigit` equivalent — `Some` only for a
valid digit `<` the radix, matching Zig's `value >= base → error`). The `i128`
accumulator (replacing the per-parser `i32`/`i64`) comfortably holds every
target's `max * base + digit` intermediate. The `unicode_range` module's
pure-hex `parse_hex_u21` (which parses a _pre-scanned_ hex run with no
sign/underscore) is intentionally **not** folded in — its input contract
differs, and it stays local to that module.

## Scope / faithfulness notes

- **Refactor (behavior-preserving)**: the three wrappers (`parse_palette_key`,
  `parse_u32_dec`, `parse_u21_hex`) keep their exact signatures and behavior;
  only their shared internals are consolidated into `parse_uint` +
  `IntParseError`.
- **Faithful**: `parse_uint` reproduces the established per-parser logic exactly
  — the sign handling (`-0` → `0`, negative nonzero → `Overflow`), the base-0
  prefix detection (`len > 2`, case-insensitive `0x`/`0o`/`0b`), the
  interior-only underscores, the per-step overflow against `max`, and the
  non-digit / digit-`>=`-base `Invalid`. `parse_palette_key` preserves the
  `Overflow`-vs-`InvalidValue` distinction; the other two preserve their
  collapse-to-`None`.
- **Regression guard**: the existing tests
  (`palette_parse_cli_key_matches_zig_parse_int`,
  `window_padding_parse_cli_parses_single_and_pair`,
  `clipboard_codepoint_map_parse_cli_parses_entries`) already exercise the
  uppercase prefixes, `+`/`-0`/negative, interior-vs-edge underscores, bare
  prefix, and overflow cases, so they verify the consolidation preserves
  behavior.
- **Not folded in**: `unicode_range::parse_hex_u21` (pre-scanned pure-hex input
  — a different contract), kept local to its module.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `IntParseError { Invalid, Overflow }` and `parse_uint(buf, base, max)`.
   - reimplement `parse_palette_key` / `parse_u32_dec` / `parse_u21_hex` as thin
     wrappers over `parse_uint`; remove `parse_u8_with_sign` and
     `char_to_digit`.
2. Tests (in `config/mod.rs`): add a focused `parse_uint` test asserting the
   cross- base behavior directly — base-0 (`"0xFF"` → 255, `"0b101"` → 5,
   `"0o17"` → 15, `"42"` → 42, `"0x"` → `Invalid`), base-10/16 fixed
   (`"255"`/`"ff"`), the sign and underscore rules (`"+5"`, `"-0"` → 0, `"-1"` →
   `Overflow`, `"1_0"` → 10, `"_0"` / `"0_"` → `Invalid`), and overflow
   (`parse_uint("256", 0, 0xFF)` → `Overflow`, `parse_uint("100", 16, 0xFF)` →
   `Overflow`). The existing wrapper tests remain the primary behavior guard.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty parse_uint
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `parse_uint` faithfully implements the unsigned `parseInt` (sign, base-0
  prefix detection, interior underscores, per-step overflow against `max`,
  `Overflow` vs `Invalid`), and the three wrappers preserve their exact
  behavior;
- all existing tests still pass (the wrapper tests are unchanged and are the
  regression guard), and the new `parse_uint` test passes;
- `parse_u8_with_sign` / `char_to_digit` are removed and
  `unicode_range::parse_hex_u21` is left untouched;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if any wrapper's behavior changes (a
previously-accepted input rejected or vice versa, a wrong `Overflow`/`Invalid`
mapping), an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed `parse_uint` is a faithful consolidation for the three
approved wrappers: palette keys still map to `parseInt(u8, _, 0)` with base
autodetection and the distinct overflow-vs-invalid errors (`Config.zig:5867`);
window padding still maps to `parseInt(u32, _, 10)` with all errors collapsed to
`InvalidValue` (`:10127`); the clipboard replacement codepoint still maps to
`parseInt(u21, _, 16)` with all errors collapsed (`:8261`). It judged the `i128`
accumulator safe for all three targets, `to_digit(radix)` a faithful
`charToDigit` for bases 2/8/10/16, the base-0 path correctly isolated to palette
keys, and the per-step bounds checks as preserving the approved overflow
semantics. It agreed leaving `unicode_range::parse_hex_u21` separate is right
(pre-scanned pure-hex contract), and that the existing wrapper tests plus a
direct `parse_uint` edge test are adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-144509-d488-prompt.md` (design)
- Result: `logs/codex-review/20260604-144509-d488-last-message.md` (design)
