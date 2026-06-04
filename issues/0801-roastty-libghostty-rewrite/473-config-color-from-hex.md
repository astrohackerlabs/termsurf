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

# Experiment 473: the config Color hex parser (Color::from_hex)

## Description

With the ported config types now consolidated into the `Config` aggregate
(Experiments 461–472), this experiment begins the **config parser** with a
clean, self-contained piece: the hex-color parser `Color::from_hex` — the
`fromHex` half of upstream's `Color.parseCLI`. It parses a `#RRGGBB` / `RRGGBB`
/ `#RGB` / `RGB` hex string into a `Color`, expanding the short 3-digit form and
erroring on a bad length or non-hex digit. The X11 named-color map (the other
`parseCLI` path) and the broader config parser stay deferred.

## Upstream behavior

In `config/Config.zig`, `Color.fromHex`:

```zig
pub fn fromHex(input: []const u8) !Color {
    // Trim the beginning '#' if it exists
    const trimmed = if (input.len != 0 and input[0] == '#') input[1..] else input;
    if (trimmed.len != 6 and trimmed.len != 3) return error.InvalidValue;

    // Expand short hex values to full hex values
    const rgb: []const u8 = if (trimmed.len == 3) &.{
        trimmed[0], trimmed[0],
        trimmed[1], trimmed[1],
        trimmed[2], trimmed[2],
    } else trimmed;

    // Parse the colors two at a time.
    var result: Color = undefined;
    comptime var i: usize = 0;
    inline while (i < 6) : (i += 2) {
        const v: u8 =
            ((try std.fmt.charToDigit(rgb[i], 16)) * 16) +
            try std.fmt.charToDigit(rgb[i + 1], 16);
        @field(result, switch (i) { 0 => "r", 2 => "g", 4 => "b", else => unreachable }) = v;
    }
    return result;
}
```

The leading `#` is trimmed; the remainder must be 6 or 3 hex digits (else
`error.InvalidValue`). A 3-digit value expands by doubling each digit. Each
channel is two hex digits, `high * 16 + low`; a non-hex digit is an error
(`charToDigit` fails). Upstream's tests: `#000000`→`{0,0,0}`, `#0A0B0C` and
`0A0B0C`→`{10,11,12}`, `FFFFFF` and `FFF`→`{255,255,255}`, `#345`→`{51,68,85}`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing a config `Color` (upstream `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorParseError {
    /// The input is not a valid hex color (wrong length or a non-hex digit).
    Invalid,
}

impl Color {
    /// Parse a hex color (upstream `Color.fromHex`): `#RRGGBB` / `RRGGBB` /
    /// `#RGB` / `RGB`. The leading `#` is optional; a 3-digit value doubles each
    /// digit; a bad length or non-hex digit is `ColorParseError::Invalid`.
    pub(crate) fn from_hex(input: &str) -> Result<Color, ColorParseError> {
        let trimmed = input.strip_prefix('#').unwrap_or(input);
        let bytes = trimmed.as_bytes();
        let expanded: [u8; 6] = match bytes.len() {
            6 => [bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5]],
            3 => [bytes[0], bytes[0], bytes[1], bytes[1], bytes[2], bytes[2]],
            _ => return Err(ColorParseError::Invalid),
        };
        let digit = |c: u8| -> Result<u8, ColorParseError> {
            (c as char)
                .to_digit(16)
                .map(|d| d as u8)
                .ok_or(ColorParseError::Invalid)
        };
        Ok(Color {
            r: digit(expanded[0])? * 16 + digit(expanded[1])?,
            g: digit(expanded[2])? * 16 + digit(expanded[3])?,
            b: digit(expanded[4])? * 16 + digit(expanded[5])?,
        })
    }
}
```

`from_hex` mirrors upstream: optional `#`, the 6-or-3-digit length check, the
short-form doubling, and `high * 16 + low` per channel with a non-hex digit
erroring. `digit(c) * 16 + digit(c)` is in `[0, 255]` (`15 * 16 + 15 = 255`), so
it fits `u8` without overflow. `ColorParseError::Invalid` is upstream's
`error.InvalidValue`.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `Color` hex parser (`Color::from_hex`,
  upstream `Color.fromHex`) and a `ColorParseError`.
- **Faithful**: optional leading `#`; the 6-or-3-digit length requirement (else
  `Invalid`); the 3-digit short-form doubling; `high * 16 + low` per channel; a
  non-hex digit is `Invalid` — exactly upstream's `fromHex`.
- **Faithful adaptation**: `error.InvalidValue` maps to
  `ColorParseError::Invalid`; Zig's `charToDigit(_, 16)` maps to Rust's
  `char::to_digit(16)`. ASCII hex input is assumed (as upstream); `as_bytes()`
  indexing matches upstream's byte slicing.
- **Deferred**: the X11 named-color map (the other `Color.parseCLI` path), the
  whitespace trimming and the rest of `parseCLI`, and the broader config parser
  (`loadCli` / per-field parsing / file loading). (Consumed by later slices;
  this experiment lands the hex parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum ColorParseError { Invalid }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `Color::from_hex(input: &str) -> Result<Color, ColorParseError>`.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `fromHex` test: `#000000`→`{0,0,0}`, `#0A0B0C` and
     `0A0B0C`→`{10,11,12}`, `FFFFFF` and `FFF`→`{255,255,255}`,
     `#345`→`{51,68,85}`; plus the error cases: a wrong length (`"12345"`) and a
     non-hex digit (`"ZZZZZZ"`) each `Err(ColorParseError::Invalid)`; and a
     lowercase input (`"0a0b0c"`) parsing the same as uppercase.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty from_hex
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Color::from_hex` parses `#RRGGBB` / `RRGGBB` / `#RGB` / `RGB` (optional `#`,
  short-form doubling, `high * 16 + low` per channel) and returns
  `ColorParseError::Invalid` on a bad length or non-hex digit — faithful to
  upstream's `fromHex`;
- the tests pass (the upstream cases; the error cases; the lowercase case), and
  the existing tests still pass;
- the X11 named-color map and the broader config parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a valid hex input is parsed wrong, an invalid input
is accepted (or a valid one rejected), the short form is not doubled, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the optional leading `#`,
the exact 6-or-3 length validation, the short-form doubling, and the per-channel
`high * 16 + low` all match `Color.fromHex` (`Config.zig:5478`); the proposed
tests include the upstream success cases (`Config.zig:5509`);
`char::to_digit(16)` on byte-derived chars is a reasonable Rust equivalent to
`std.fmt.charToDigit(_, 16)` for ASCII hex (both cases); the `u8` arithmetic is
safe (each digit `0..=15`, so `15 * 16 + 15 == 255`); and deferring the X11
names, whitespace trimming, and the broader `parseCLI` path is the right scope.
It judged the planned tests adequate (wrong length, bad digit, lowercase, the
upstream examples).

Review artifacts:

- Prompt: `logs/codex-review/20260604-125105-d473-prompt.md` (design)
- Result: `logs/codex-review/20260604-125105-d473-last-message.md` (design)

## Result

**Result:** Pass

`ColorParseError { Invalid }` and `Color::from_hex` were added to
`roastty/src/config/mod.rs` exactly as designed — optional leading `#`, the
6-or-3-digit length check (else `Invalid`), the 3-digit short-form doubling, and
`high * 16 + low` per channel with a non-hex digit erroring. The new test
`from_hex_parses_hex_colors` asserts the upstream `fromHex` cases (`#000000`,
`#0A0B0C`, `0A0B0C`, `FFFFFF`, `FFF`, `#345`), a lowercase input parsing the
same as uppercase, and the two error paths (wrong length, non-hex digit).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2952 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: `Color::from_hex` faithfully ports `Color.fromHex` (optional `#`,
3-or-6-digit inputs only, short-form expansion, two-hex-digit channel parsing);
`ColorParseError::Invalid` is an appropriate mapping for `error.InvalidValue`;
`to_digit(16)` handles upper/lowercase ASCII hex; the test covers all upstream
success cases plus lowercase and the key error paths; and deferring the X11
names, trimming, and the full `parseCLI` remains the right scope. "Approved for
the result commit."

Review artifacts:

- Prompt: `logs/codex-review/20260604-125604-r473-prompt.md` (result)
- Result: `logs/codex-review/20260604-125604-r473-last-message.md` (result)

## Conclusion

The config parser is under way: the hex-color parser `Color::from_hex` is
ported, faithful to upstream's `fromHex`, with `ColorParseError` as the
parse-error type. The next slice can extend the config `Color` parser with the
X11 named-color map (the other `Color.parseCLI` path) or move on to a per-field
`parseCLI`, building toward the full config parser (`loadCli` / file loading)
over later experiments.
