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

# Experiment 478: the config Palette CLI parser (Palette::parse_cli)

## Description

With the scalar color value types parsing (`Color`, `TerminalColor`,
`BoldColor`, Experiments 473–477), this experiment ports the first **aggregate**
color config value: the `palette` config, upstream `Config.Palette`. A `Palette`
holds the 256-entry color table plus a mask of which indices the user set; its
parser reads one `index=color` assignment per call, mutating the table in place.
The key is a base-0 integer (decimal / `0x` / `0o` / `0b`); the color reuses
`Color::parse_cli`.

This is a mutating, accumulating parser (`self: *Self` upstream): each call sets
one palette index, so a config with many `palette = N=#hex` lines applies them
one by one. The `cval` C-struct and `formatEntry` formatter stay deferred.

## Upstream behavior

In `config/Config.zig`, `Config.Palette`:

```zig
pub const Palette = struct {
    value: terminal.color.Palette = terminal.color.default,
    mask: terminal.color.PaletteMask = .initEmpty(),

    pub fn parseCLI(self: *Self, input: ?[]const u8) !void {
        const value = input orelse return error.ValueRequired;
        const eqlIdx = std.mem.indexOf(u8, value, "=") orelse
            return error.InvalidValue;

        // Parse the key part (trim whitespace)
        const key = try std.fmt.parseInt(
            u8,
            std.mem.trim(u8, value[0..eqlIdx], " \t"),
            0,
        );

        // Parse the color part (Color.parseCLI will handle whitespace)
        const rgb = try Color.parseCLI(value[eqlIdx + 1 ..]);
        self.value[key] = .{ .r = rgb.r, .g = rgb.g, .b = rgb.b };
        self.mask.set(key);
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- The string is split on the first `=` (`std.mem.indexOf`); no `=` is
  `error.InvalidValue`.
- The key (left of `=`) is whitespace-trimmed (`" \t"`) and parsed as a
  **base-0** `u8` (`std.fmt.parseInt(u8, _, 0)`): the base is auto-detected from
  a `0x` / `0o` / `0b` prefix, otherwise decimal. A non-numeric key is
  `error.InvalidValue`; a key `> 255` is `error.Overflow`.
- The color (right of `=`, including any `#`) is parsed by `Color.parseCLI`
  (which trims and does the X11/hex logic); its error propagates.
- On success the entry is written (`self.value[key] = rgb`) and the index is
  marked in the mask (`self.mask.set(key)`). On any error the table and mask are
  unchanged (the writes happen only after all parsing succeeds).

Upstream tests: `"0=#AABBCC"` sets index 0 and marks the mask; `"0b1=#014589"` /
`"0o7=#234567"` / `"0xF=#ABCDEF"` exercise the base prefixes; `"256=#AABBCC"` is
`error.Overflow` with the mask left empty; whitespace around the key and color
(`"0 =  #AABBCC"`, `" 1= #DDEEFF    "`, `"  2  =  #123456 "`) is tolerated.

roastty already has the terminal-layer pieces: `terminal::color::Palette`
(`[Rgb; 256]`), `terminal::color::DEFAULT_PALETTE` (upstream
`terminal.color.default`), and `terminal::color::PaletteMask` (the `[u64; 4]`
bitset with `empty` / `set` / `get` / `is_empty`). The config `Palette` wraps
these, mirroring upstream.

## Rust mapping

`roastty/src/terminal/color.rs` — widen the already-present `PaletteMask` (and
the methods the config parser needs) to `pub(crate)`, since upstream's config
`Palette` uses `terminal.color.PaletteMask` directly:

```rust
pub(crate) struct PaletteMask { /* ... */ }
impl PaletteMask {
    pub(crate) const fn empty() -> Self { /* ... */ }
    pub(crate) fn is_empty(self) -> bool { /* ... */ }
    pub(crate) fn set(&mut self, index: u8) { /* ... */ }
    pub(crate) fn get(self, index: u8) -> bool { /* ... */ }
    // `unset` / `iter_set` stay as-is.
}
```

`roastty/src/config/mod.rs` — the config `Palette` struct, its `Default`, a
base-0 `u8` key parser, and `parse_cli`:

```rust
use crate::terminal::color::{Palette as TerminalPalette, PaletteMask, DEFAULT_PALETTE};

/// An error parsing the `palette` config (upstream `Palette.parseCLI` errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PaletteParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// No `=`, or a non-numeric key, or an unparseable color (upstream
    /// `error.InvalidValue`).
    InvalidValue,
    /// The palette index is greater than 255 (upstream `error.Overflow`).
    Overflow,
}

/// The `palette` config (upstream `Config.Palette`): the 256-entry color table
/// plus a mask of which indices the user set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Palette {
    pub value: TerminalPalette,
    pub mask: PaletteMask,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            value: DEFAULT_PALETTE,
            mask: PaletteMask::empty(),
        }
    }
}

impl Palette {
    /// Parse one `index=color` assignment (upstream `Palette.parseCLI`): split on
    /// the first `=`, parse the whitespace-trimmed base-0 `u8` key and the color
    /// (via [`Color::parse_cli`]), then set that entry and mark the mask. A
    /// missing value is `PaletteParseError::ValueRequired`; a missing `=` or a
    /// bad key/color is `InvalidValue`; a key `> 255` is `Overflow`.
    pub(crate) fn parse_cli(&mut self, input: Option<&str>) -> Result<(), PaletteParseError> {
        let value = input.ok_or(PaletteParseError::ValueRequired)?;
        let eql = value.find('=').ok_or(PaletteParseError::InvalidValue)?;

        let key = parse_palette_key(value[..eql].trim_matches(|c: char| c == ' ' || c == '\t'))?;
        let rgb = Color::parse_cli(Some(&value[eql + 1..]))
            .map_err(|_| PaletteParseError::InvalidValue)?;

        self.value[key as usize] = rgb.to_terminal_rgb();
        self.mask.set(key);
        Ok(())
    }
}

/// Parse a base-0 `u8` (upstream `std.fmt.parseInt(u8, _, 0)`). A faithful port
/// of Zig's `parseInt` / `parseIntWithSign`: an optional leading `+`/`-` sign,
/// then base auto-detection from a case-insensitive `0x`/`0o`/`0b` prefix
/// (decimal otherwise), `_` separators allowed only *between* digits
/// (leading/trailing `_` rejected). For an unsigned `u8`: `-0` is `0`, any
/// negative nonzero is `Overflow`, a value `> 255` is `Overflow`, and any other
/// malformed input is `InvalidValue` (Zig's `error.InvalidCharacter`).
fn parse_palette_key(buf: &str) -> Result<u8, PaletteParseError> {
    let bytes = buf.as_bytes();
    match bytes.first() {
        Some(b'+') => parse_u8_with_sign(&buf[1..], false),
        Some(b'-') => parse_u8_with_sign(&buf[1..], true),
        _ => parse_u8_with_sign(buf, false),
    }
}

fn parse_u8_with_sign(buf: &str, neg: bool) -> Result<u8, PaletteParseError> {
    let bytes = buf.as_bytes();
    if bytes.is_empty() {
        return Err(PaletteParseError::InvalidValue); // bare "+"/"-" or empty
    }

    // base 0: default decimal; detect a `0x`/`0o`/`0b` prefix (case-insensitive,
    // and only when there is at least one digit after it — `buf.len > 2`).
    let mut base: u32 = 10;
    let mut start: &[u8] = bytes;
    if bytes.len() > 2 && bytes[0] == b'0' {
        match bytes[1].to_ascii_lowercase() {
            b'b' => (base, start) = (2, &bytes[2..]),
            b'o' => (base, start) = (8, &bytes[2..]),
            b'x' => (base, start) = (16, &bytes[2..]),
            _ => {}
        }
    }

    // Leading/trailing underscores are rejected; interior ones are skipped.
    if start[0] == b'_' || start[start.len() - 1] == b'_' {
        return Err(PaletteParseError::InvalidValue);
    }

    // Accumulate is `u8` for a `u8` result; every intermediate must fit 0..=255.
    let mut acc: i32 = 0;
    for &c in start {
        if c == b'_' {
            continue;
        }
        let digit = char_to_digit(c, base)? as i32;
        if acc != 0 {
            acc = acc
                .checked_mul(base as i32)
                .filter(|&v| v <= 255)
                .ok_or(PaletteParseError::Overflow)?;
        } else if neg {
            // First digit of a negative number: only `-0` survives for unsigned.
            acc = -digit;
            if acc < 0 {
                return Err(PaletteParseError::Overflow);
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=255).contains(&acc) {
            return Err(PaletteParseError::Overflow);
        }
    }
    Ok(acc as u8)
}

/// Upstream `std.fmt.charToDigit`: the digit value of an ASCII alphanumeric in
/// the given base, else `InvalidValue` (Zig's `error.InvalidCharacter`).
fn char_to_digit(c: u8, base: u32) -> Result<u32, PaletteParseError> {
    let value = match c {
        b'0'..=b'9' => (c - b'0') as u32,
        b'A'..=b'Z' => (c - b'A') as u32 + 10,
        b'a'..=b'z' => (c - b'a') as u32 + 10,
        _ => return Err(PaletteParseError::InvalidValue),
    };
    if value >= base {
        return Err(PaletteParseError::InvalidValue);
    }
    Ok(value)
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the first-`=` split
(else `InvalidValue`), the trimmed base-0 key, the color via `Color::parse_cli`,
and the write-then-mark, with writes only on full success.

`parse_palette_key` is a faithful port of Zig's `std.fmt.parseInt(u8, _, 0)`
(verified against the Zig 0.16 `fmt.zig` source and its own test cases): the
optional `+`/`-` sign; the case-insensitive `0x`/`0o`/`0b` prefix detection that
only applies with a digit following it (so a bare `"0x"` is `InvalidValue`);
interior-only `_` separators (leading/trailing rejected, but doubled interior
`__` allowed, matching Zig); the per-step `u8` overflow check (Zig accumulates
in a `u8` for a `u8` result); and the unsigned-sign semantics (`-0` → `0`, any
negative nonzero → `Overflow`). `char_to_digit` mirrors Zig's `charToDigit`
(ASCII alphanumerics, value `>= base` → invalid). Zig's `error.Overflow` →
`PaletteParseError::Overflow` and `error.InvalidCharacter` →
`PaletteParseError::InvalidValue`.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `Palette` struct (value + mask, default to
  `DEFAULT_PALETTE` + empty mask), `Palette::parse_cli` (upstream
  `Palette.parseCLI`), the base-0 key parser, and `PaletteParseError`.
- **Faithful**: the `ValueRequired` guard; the first-`=` split (`InvalidValue`
  on none); the `" \t"`-trimmed base-0 `u8` key with the `0x`/`0o`/`0b` prefixes
  and `Overflow` on `> 255`; the color via `Color::parse_cli`; the set-entry +
  mark-mask, applied only after parsing succeeds (so an error leaves the table
  and mask unchanged) — exactly upstream's `parseCLI`.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; `std.mem.indexOf(=)`
  → `str::find('=')`; `std.fmt.parseInt(u8, _, 0)` → `parse_palette_key`
  (`parse_u8_with_sign` + `char_to_digit`), a close port of Zig's
  `parseInt`/`parseIntWithSign`/`charToDigit` (optional `+`/`-` sign;
  case-insensitive `0x`/`0o`/`0b` prefix requiring a following digit;
  interior-only `_` separators; per-step `u8` overflow; `-0` → `0`, negative
  nonzero → `Overflow`); the distinct upstream error set (`ValueRequired` /
  `InvalidValue` / `Overflow`) → `PaletteParseError` (Zig's `InvalidCharacter` →
  `InvalidValue`); the color error (always the `Invalid` arm here, since the
  color slice is never `None`) folds into `InvalidValue`.
- **Faithful re-use**: `value` is `terminal::color::Palette` defaulting to
  `DEFAULT_PALETTE` (upstream `terminal.color.default`); `mask` is
  `terminal::color::PaletteMask` (upstream `terminal.color.PaletteMask`),
  widened to `pub(crate)` so config can use it (the existing in-`terminal`
  callers are unaffected). The color path reuses `Color::parse_cli`.
- **Deferred**: `Palette.cval` / the `ghostty_config_palette_s` C extern struct,
  and `Palette.formatEntry` (depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter (`loadCli` /
  per-field dispatch / file loading). (Consumed by later slices.)
- No C ABI/header/ABI-inventory change (internal Rust; the C-struct `cval` is
  deferred).

## Changes

1. `roastty/src/terminal/color.rs`:
   - widen `PaletteMask` and its `empty` / `is_empty` / `set` / `get` methods to
     `pub(crate)` (`unset` / `iter_set` unchanged).
2. `roastty/src/config/mod.rs`:
   - add `PaletteParseError { ValueRequired, InvalidValue, Overflow }`.
   - add the config `Palette` struct (`value: terminal::color::Palette`,
     `mask: PaletteMask`) with a `Default` of `DEFAULT_PALETTE` + empty mask.
   - add
     `Palette::parse_cli(&mut self, input: Option<&str>) -> Result<(), PaletteParseError>`
     and the private base-0 key helpers (`parse_palette_key` /
     `parse_u8_with_sign` / `char_to_digit`).
3. Tests (in `config/mod.rs`):
   - mirror upstream's `Palette.parseCLI` tests: `"0=#AABBCC"` sets index 0 to
     `{0xAA,0xBB,0xCC}` and `mask.get(0)` is set, `mask.get(1)` is not; the base
     prefixes `"0b1=#014589"` / `"0o7=#234567"` / `"0xF=#ABCDEF"` set indices
     1/7/15 to the right colors and mark only those; `"256=#AABBCC"` →
     `Err(Overflow)` with the mask still empty and the table unchanged at index
     0; the whitespace cases (`"0 =  #AABBCC"`, `" 1= #DDEEFF    "`,
     `"  2  =  #123456 "`); plus `None` → `Err(ValueRequired)`, a no-`=` input
     (`"0"`) → `Err(InvalidValue)`, and a bad color (`"0=nope"`) →
     `Err(InvalidValue)`.
   - exercise the base-0 key parser directly (the `parse_palette_key`
     faithfulness points the design review called out): uppercase prefixes
     (`"0XF=#ABCDEF"` / `"0B1=#014589"` / `"0O7=#234567"` parse as 15/1/7); a
     leading `+` (`"+0xF=#ABCDEF"` → index 15; `"+0=#AABBCC"` → index 0); the
     unsigned sign rules (`"-0=#AABBCC"` → index 0; `"-1=#AABBCC"` →
     `Err(Overflow)`); interior vs edge underscores (`"1_0=#AABBCC"` → index 10
     and `"0x1_0=#AABBCC"` → index 16; `"_0=…"` / `"0_=…"` / `"0x_10=…"` /
     `"0x10_=…"` → `Err(InvalidValue)`); and a bare prefix (`"0x=#AABBCC"` →
     `Err(InvalidValue)`).
4. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty palette
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Palette::parse_cli` reads one `index=color` assignment — splitting on the
  first `=`, parsing the trimmed base-0 `u8` key (with the `0x`/`0o`/`0b`
  prefixes and `Overflow` on `> 255`) and the color via `Color::parse_cli`, then
  setting the entry and marking the mask only on full success — faithful to
  upstream's `parseCLI`;
- the tests pass (the upstream key/base/overflow/whitespace cases; the
  missing-value, no-`=`, and bad-color errors; the error-leaves-state-unchanged
  case), and the existing tests still pass;
- `Palette.cval` / `formatEntry` and the broader config parser/formatter stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a key (including the base prefixes), a color, or the
mask is parsed/marked wrong, an overflowing key does not error (or mutates
state), a missing `=` or value does not error, an unrelated item changes, or any
public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation across two rounds.

**Round 1 — one Required finding (fixed).** Codex found the first-draft base-0
key parser unfaithful to Zig's `std.fmt.parseInt(u8, _, 0)`: Zig accepts
uppercase base prefixes (it lowercases the prefix byte), a leading `+`, and `-0`
(with negative nonzero overflowing for an unsigned target), and permits only
_interior_ `_` separators (rejecting leading/trailing). The first draft
(lowercase-only prefixes, a permissive `_` filter, no sign handling) diverged on
all four. Fixed by reading the actual Zig 0.16 `fmt.zig` source
(`parseInt`/`parseIntWithGenericCharacter`/`parseIntWithSign`/`charToDigit`) and
rewriting the helper as a close port (`parse_palette_key` / `parse_u8_with_sign`
/ `char_to_digit`), and by expanding the tests to cover uppercase prefixes, the
leading `+`, `-0` / `-1`, interior-vs-edge underscores, and the bare-prefix
case.

**Round 2 — approved, no findings.** Codex confirmed the rewritten parser
matches Zig's `parseInt(u8, _, 0)` for palette keys: the optional `+`/`-` (incl.
`-0` → 0 and negative-nonzero → `Overflow`); the case-insensitive `0b`/`0o`/`0x`
detection with the `len > 2` guard; interior `_` accepted (incl. doubled) with
leading/ trailing rejected; the `i32` accumulator with explicit `0..=255` checks
reproducing Zig's `u8` overflow points at every mul/add; and the
`InvalidCharacter` → `InvalidValue` / `Overflow` → `Overflow` mapping. It
re-confirmed the rest of `parse_cli` faithful to upstream (`Config.zig:5858`) —
the value-required guard, the first-`=` split, the `" \t"` key-only trim,
`Color` parsing for the value, and mutation only after both parses succeed — and
that reusing terminal `Palette` / `DEFAULT_PALETTE` / `PaletteMask` is the right
approach, with the expanded tests adequate. "Approved with no findings."

Review artifacts:

- Round 1 prompt: `logs/codex-review/20260604-131757-d478-prompt.md`
- Round 1 result: `logs/codex-review/20260604-131757-d478-last-message.md`
- Round 2 prompt: `logs/codex-review/20260604-132352-d478b-prompt.md`
- Round 2 result: `logs/codex-review/20260604-132352-d478b-last-message.md`
