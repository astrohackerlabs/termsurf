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

# Experiment 481: the config WindowPadding CLI parser (WindowPadding::parse_cli)

## Description

Continuing the non-color config value types, this experiment ports
`WindowPadding` (upstream `Config.WindowPadding`) — the `window-padding-x` /
`window-padding-y` pair value: either a single `u32` (applied to both edges) or
two comma-separated `u32`s (`top_left,bottom_right`). Its parser splits on the
first comma, trims each side, and parses each as a base-10 `u32`, mapping any
parse failure to `InvalidValue`. The `formatEntry` formatter stays deferred.

## Upstream behavior

In `config/Config.zig`, `Config.WindowPadding`:

```zig
pub const WindowPadding = struct {
    top_left: u32 = 0,
    bottom_right: u32 = 0,

    pub fn parseCLI(input_: ?[]const u8) !WindowPadding {
        const input = input_ orelse return error.ValueRequired;
        const whitespace = " \t";

        if (std.mem.indexOf(u8, input, ",")) |idx| {
            const input_left = std.mem.trim(u8, input[0..idx], whitespace);
            const input_right = std.mem.trim(u8, input[idx + 1 ..], whitespace);
            const left = std.fmt.parseInt(u32, input_left, 10) catch
                return error.InvalidValue;
            const right = std.fmt.parseInt(u32, input_right, 10) catch
                return error.InvalidValue;
            return .{ .top_left = left, .bottom_right = right };
        } else {
            const value = std.fmt.parseInt(
                u32,
                std.mem.trim(u8, input, whitespace),
                10,
            ) catch return error.InvalidValue;
            return .{ .top_left = value, .bottom_right = value };
        }
    }
    // ...
};
```

- A missing value is `error.ValueRequired`.
- If the input contains a `,`, it splits on the **first** comma, trims each side
  of `" \t"`, and parses both as base-10 `u32` →
  `{ top_left = left, bottom_right = right }`.
- Otherwise it trims the whole input and parses one base-10 `u32`, applied to
  both edges → `{ top_left = value, bottom_right = value }`.
- **Any** `parseInt` failure (bad digit, empty, overflow, a stray sign) is
  `error.InvalidValue`.

Upstream's tests: `"100"` → `{100, 100}`; `"100,200"` → `{100, 200}`;
`" 100 , 200 "` → `{100, 200}`; `null` → `error.ValueRequired`; `""` and `"a"` →
`error.InvalidValue`.

`parseInt(u32, _, 10)` is base-10 (no `0x`/`0o`/`0b` prefix detection): it
accepts an optional leading `+`/`-` sign (for an unsigned `u32`, only `+`, or
`-0`, succeeds), interior-only `_` digit separators (leading/trailing `_`
rejected), and errors on overflow — all of which collapse to `InvalidValue`
here.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// The `window-padding-*` config (upstream `Config.WindowPadding`): a padding pair
/// (a single value applies to both edges). The `formatEntry` formatter is ported
/// later.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct WindowPadding {
    pub top_left: u32,
    pub bottom_right: u32,
}

impl WindowPadding {
    /// Parse window padding (upstream `WindowPadding.parseCLI`): one base-10 `u32`
    /// applied to both edges, or two comma-separated `u32`s
    /// (`top_left,bottom_right`), each `" \t"`-trimmed. A missing value is
    /// `WindowPaddingParseError::ValueRequired`; any parse failure is
    /// `InvalidValue`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<WindowPadding, WindowPaddingParseError> {
        let input = input.ok_or(WindowPaddingParseError::ValueRequired)?;
        let trim = |s: &str| s.trim_matches(|c: char| c == ' ' || c == '\t').to_string();

        if let Some(idx) = input.find(',') {
            let left = parse_u32_dec(&trim(&input[..idx]))
                .ok_or(WindowPaddingParseError::InvalidValue)?;
            let right = parse_u32_dec(&trim(&input[idx + 1..]))
                .ok_or(WindowPaddingParseError::InvalidValue)?;
            Ok(WindowPadding {
                top_left: left,
                bottom_right: right,
            })
        } else {
            let value =
                parse_u32_dec(&trim(input)).ok_or(WindowPaddingParseError::InvalidValue)?;
            Ok(WindowPadding {
                top_left: value,
                bottom_right: value,
            })
        }
    }
}

/// An error parsing `WindowPadding` (upstream `WindowPadding.parseCLI`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowPaddingParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// A side did not parse as a base-10 `u32` (upstream `error.InvalidValue`).
    InvalidValue,
}

/// Parse a base-10 `u32` (upstream `std.fmt.parseInt(u32, _, 10)`): an optional
/// `+`/`-` sign, then decimal digits with interior-only `_` separators
/// (leading/trailing `_` rejected). `-0` is `0`; a negative nonzero, an overflow,
/// or any non-digit is `None`. (The whole string must parse — unlike the greedy
/// scan in [`Duration::parse_cli`].)
fn parse_u32_dec(buf: &str) -> Option<u32> {
    let (neg, rest): (bool, &str) = match buf.as_bytes().first() {
        Some(b'+') => (false, &buf[1..]),
        Some(b'-') => (true, &buf[1..]),
        _ => (false, buf),
    };
    let bytes = rest.as_bytes();
    if bytes.is_empty() || bytes[0] == b'_' || bytes[bytes.len() - 1] == b'_' {
        return None;
    }
    let mut acc: i64 = 0;
    for &c in bytes {
        if c == b'_' {
            continue;
        }
        if !c.is_ascii_digit() {
            return None;
        }
        let digit = (c - b'0') as i64;
        if acc != 0 {
            acc = acc.checked_mul(10).filter(|&v| v <= u32::MAX as i64)?;
        } else if neg {
            // First digit of a negative number: only `-0` survives for unsigned.
            acc = -digit;
            if acc < 0 {
                return None;
            }
            continue;
        }
        acc = if neg { acc - digit } else { acc + digit };
        if !(0..=(u32::MAX as i64)).contains(&acc) {
            return None;
        }
    }
    Some(acc as u32)
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the first-comma split
with per-side `" \t"` trim (or the whole-input trim for the single value), and
base-10 `u32` parsing with every failure folding to `InvalidValue`.
`parse_u32_dec` is a faithful port of Zig's `parseInt(u32, _, 10)` (base-10, no
prefix detection; the sign and interior-underscore rules and overflow handling
mirror the same Zig algorithm ported for the palette key in Experiment 478,
minus the base-0 prefix step) — but since upstream `catch`es every parse error
to `InvalidValue`, the helper returns `Option<u32>` (overflow and bad-digit are
indistinguishable here).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `WindowPadding` struct (`top_left` /
  `bottom_right`) and `WindowPadding::parse_cli` (upstream
  `WindowPadding.parseCLI`), plus `WindowPaddingParseError` and the base-10
  `u32` helper.
- **Faithful**: the `ValueRequired` guard; the first-comma split; the per-side
  and whole-input `" \t"` trim; the single-value-to-both-edges behavior; base-10
  `u32` parsing with every failure → `InvalidValue` — exactly upstream's
  `parseCLI`.
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`;
  `std.mem.indexOf(",")` → `str::find(',')`; `std.fmt.parseInt(u32, _, 10)` →
  `parse_u32_dec` (a faithful base-10 port of Zig's `parseInt`: optional sign,
  interior-only underscores, `-0` → `0`, overflow → fail); the two upstream
  errors → `WindowPaddingParseError`. Because upstream maps every `parseInt`
  error to `InvalidValue`, `parse_u32_dec` returns `Option` (no separate
  overflow error).
- **Deferred**: `WindowPadding.formatEntry` (renders one value when both edges
  are equal, else `left,right`; depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter. `clone` / `equal`
  are covered by the derives. (Consumed by later slices; this experiment lands
  the parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `WindowPaddingParseError { ValueRequired, InvalidValue }`, the
     `WindowPadding` struct (`top_left` / `bottom_right: u32`,
     `derive(Debug, Clone, Copy, Default, PartialEq, Eq)`),
     `WindowPadding::parse_cli`, and the private `parse_u32_dec` base-10 helper.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parse WindowPadding` test: `"100"` → `{100, 100}`;
     `"100,200"` → `{100, 200}`; `" 100 , 200 "` → `{100, 200}`; `None` →
     `Err(ValueRequired)`; `""` and `"a"` → `Err(InvalidValue)`.
   - exercise `parse_u32_dec` faithfulness: `"0"` → `{0, 0}`; the `u32::MAX`
     value parses; an overflow (`"4294967296"`) → `Err(InvalidValue)`; interior
     underscore (`"1_000"` → `{1000, 1000}`); leading/trailing underscore
     (`"_5"` / `"5_"`) → `Err(InvalidValue)`; a leading `+` (`"+5"` → `{5, 5}`);
     `-0` → `{0, 0}`; a negative nonzero (`"-5"`) → `Err(InvalidValue)`; and a
     bad side in a pair (`"100,x"` → `Err(InvalidValue)`).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty window_padding
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `WindowPadding::parse_cli` parses one `u32` (both edges) or two
  comma-separated `u32`s (`top_left,bottom_right`), each `" \t"`-trimmed,
  returning `ValueRequired` on a missing value and `InvalidValue` on any parse
  failure — faithful to upstream's `parseCLI`;
- the tests pass (the upstream cases; the `parse_u32_dec` faithfulness cases),
  and the existing tests still pass;
- `WindowPadding.formatEntry` and the broader config parser/formatter stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a value/pair is parsed wrong (wrong split/trim,
wrong single-to-both behavior, a `parseInt` failure not mapped to
`InvalidValue`), a missing value does not error, an unrelated item changes, or
any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream (`Config.zig:10120`):
`parse_cli` matches upstream — `None` → `ValueRequired`, the first-comma split
via `indexOf`, the space/tab trim on each side or the whole value, the single
value copied to both fields, and all integer parse failures collapsed to
`InvalidValue`; `parse_u32_dec` is the right fixed-base-10 adaptation of Zig
`parseInt(u32, _, 10)` (no base prefixes, optional `+`, `-0` allowed, negative
nonzero rejected, internal underscores allowed, edge underscores rejected,
overflow rejected); returning `Option<u32>` is fine since upstream maps every
`parseInt` error to `InvalidValue`; deferring `formatEntry` is the right scope;
and the planned tests cover the upstream cases plus the useful integer-parser
edge cases.

Review artifacts:

- Prompt: `logs/codex-review/20260604-134654-d481-prompt.md` (design)
- Result: `logs/codex-review/20260604-134654-d481-last-message.md` (design)
