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

# Experiment 474: the config Color CLI parser (Color::parse_cli)

## Description

Experiment 473 landed the hex half of the config `Color` parser
(`Color::from_hex`). This experiment lands the other half — `Color::parse_cli`,
the full `Color.parseCLI` — which is what the config parser actually calls for a
color value. It trims surrounding whitespace, looks the trimmed string up in the
X11 named-color map (already ported as `terminal::x11_color`), and falls back to
`from_hex`. A null input is the upstream `error.ValueRequired`.

## Upstream behavior

In `config/Config.zig`, `Color.parseCLI`:

```zig
pub fn parseCLI(input_: ?[]const u8) !Color {
    const input = input_ orelse return error.ValueRequired;
    // Trim any whitespace before processing
    const trimmed = std.mem.trim(u8, input, " \t");

    if (terminal.x11_color.map.get(trimmed)) |rgb| return .{
        .r = rgb.r,
        .g = rgb.g,
        .b = rgb.b,
    };

    return fromHex(trimmed);
}
```

A missing value is `error.ValueRequired`. Otherwise the input is trimmed of
leading/trailing spaces and tabs (`" \t"`); the trimmed string is looked up in
the X11 named-color map (`terminal.x11_color.map`, a case-insensitive lookup);
on a hit, its `Rgb` is returned as a `Color`; on a miss, the trimmed string is
passed to `fromHex`. Upstream's tests: `parseCLI("black")` → `{0,0,0}`;
`parseCLI(" #AABBCC   ")` → `{0xAA,0xBB,0xCC}`; `parseCLI("  black ")` →
`{0,0,0}`.

The X11 map is built from the embedded `res/rgb.txt` with an ASCII
case-insensitive lookup (`terminal/x11_color.zig`). roastty already ports it as
`terminal::x11_color::get(name: &[u8]) -> Option<Rgb>` — same data, same
edge-space trim, same `eq_ignore_ascii_case` matching.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing a config `Color`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ColorParseError {
    /// No value was supplied (upstream `error.ValueRequired`).
    ValueRequired,
    /// The input is not a valid hex color (wrong length or a non-hex digit).
    Invalid,
}

impl Color {
    /// Parse a config color value (upstream `Color.parseCLI`): trim surrounding
    /// spaces and tabs, look the result up in the X11 named-color map, and fall
    /// back to [`Color::from_hex`]. A missing value is
    /// `ColorParseError::ValueRequired`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<Color, ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        let trimmed = input.trim_matches(|c: char| c == ' ' || c == '\t');
        if let Some(rgb) = crate::terminal::x11_color::get(trimmed.as_bytes()) {
            return Ok(Color {
                r: rgb.r,
                g: rgb.g,
                b: rgb.b,
            });
        }
        Color::from_hex(trimmed)
    }
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard on a missing value, the
`" \t"` trim, the X11 named-color lookup with a `Color` constructed from the
returned `Rgb`, and the `from_hex` fallback. `ColorParseError` gains a
`ValueRequired` variant (upstream `error.ValueRequired`); the existing `Invalid`
(upstream `error.InvalidValue`) is unchanged and is what a `from_hex` miss
returns.

To call the X11 map from `config`, `terminal::x11_color` is widened from
`mod`/`pub(super) fn get` to `pub(crate) mod`/`pub(crate) fn get` — upstream's
`x11_color.map` is `pub`, so this is a faithful visibility widening, and the
existing in-`terminal` caller (`color::Rgb::parse`) is unaffected.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `Color` CLI parser (`Color::parse_cli`,
  upstream `Color.parseCLI`), and a `ValueRequired` variant on
  `ColorParseError`.
- **Faithful**: the `ValueRequired` guard on a missing value; the `" \t"` trim;
  the X11 named-color lookup before the hex fallback; the `Color`-from-`Rgb`
  construction on a name hit; the `from_hex` fallback on a miss — exactly
  upstream's `parseCLI`.
- **Faithful adaptation**: `?[]const u8` maps to `Option<&str>`;
  `error.ValueRequired` maps to `ColorParseError::ValueRequired`;
  `std.mem.trim(u8, input, " \t")` maps to `trim_matches(' ' | '\t')`;
  `terminal.x11_color.map.get` maps to the already-ported
  `terminal::x11_color::get`.
- **Faithful re-use**: the X11 named-color map is the existing
  `terminal::x11_color`, whose visibility is widened to `pub(crate)` (upstream's
  map is `pub`); no new color data is added.
- **Deferred**: `Color.formatBuf` / `formatEntry` (the formatter side), `cval` /
  the C extern struct, and the broader config parser (`loadCli` / per-field
  dispatch / file loading). (Consumed by later slices; this experiment lands the
  value parser.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/terminal/mod.rs`:
   - widen `mod x11_color;` to `pub(crate) mod x11_color;`.
2. `roastty/src/terminal/x11_color.rs`:
   - widen `get` from `pub(super)` to `pub(crate)`.
3. `roastty/src/config/mod.rs`:
   - add a `ValueRequired` variant to `ColorParseError`.
   - add
     `Color::parse_cli(input: Option<&str>) -> Result<Color, ColorParseError>`.
4. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` tests: `parse_cli(Some("black"))` → `{0,0,0}`;
     `parse_cli(Some(" #AABBCC   "))` → `{0xAA,0xBB,0xCC}`;
     `parse_cli(Some("  black "))` → `{0,0,0}`; plus a hex passthrough
     (`Some("#0A0B0C")` → `{10,11,12}`), a case-insensitive name
     (`Some("ForestGreen")` → `{34,139,34}`), a tab-trim case
     (`Some("\tblack\t")` → `{0,0,0}`), a missing value (`None` →
     `Err(ValueRequired)`), and a non-name non-hex input (`Some("nosuchcolor")`
     → `Err(Invalid)`).
5. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty parse_cli
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `Color::parse_cli` trims `" \t"`, looks the result up in the X11 named-color
  map, and falls back to `from_hex`, returning `ColorParseError::ValueRequired`
  on a missing value — faithful to upstream's `parseCLI`;
- the tests pass (the upstream cases; the hex passthrough; the case-insensitive
  name; the tab-trim; the missing-value and the non-name/non-hex error cases),
  and the existing tests still pass;
- `Color.formatBuf` and the broader config parser stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a named color or a hex value is parsed wrong, a
missing value does not error, the whitespace is not trimmed, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream (`Config.zig:5434`): the
`ValueRequired` guard on `None`, trimming only space and tab, the X11 lookup
before the hex fallback, and the field-for-field `Rgb` → `Color` all match
`Color.parseCLI`; `trim_matches(' ' | '\t')` is the right equivalent of
`std.mem.trim(u8, input, " \t")`, and since `x11_color::get` only trims edge
spaces, doing the tab trim in `parse_cli` is necessary and correct; widening
`terminal::x11_color`/`get` to `pub(crate)` is the right reuse path (duplicating
the map in config would be worse); keeping `from_hex` as the fallback preserves
the upstream order (named color first, then hex). It judged the planned tests
adequate (upstream cases, case-insensitive name, tab trimming, hex passthrough,
missing value, invalid fallback).

Review artifacts:

- Prompt: `logs/codex-review/20260604-125918-d474-prompt.md` (design)
- Result: `logs/codex-review/20260604-125918-d474-last-message.md` (design)
