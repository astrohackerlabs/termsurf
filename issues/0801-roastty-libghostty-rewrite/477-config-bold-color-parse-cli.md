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

# Experiment 477: the config BoldColor CLI parser (BoldColor::parse_cli)

## Description

Following `TerminalColor::parse_cli` (Experiment 476), this experiment ports the
other small color union's parser: `BoldColor.parseCLI` (the `bold-color`
config). A `BoldColor` is either the keyword `bright` (use the bright palette
variant) or an explicit `Color`; its parser checks the one keyword and otherwise
delegates to `Color::parse_cli`. The `formatEntry` side (which depends on the
not-yet-ported config `EntryFormatter`) stays deferred.

## Upstream behavior

In `config/Config.zig`, `BoldColor.parseCLI`:

```zig
pub const BoldColor = union(enum) {
    color: Color,
    bright,

    pub fn parseCLI(input_: ?[]const u8) !BoldColor {
        const input = input_ orelse return error.ValueRequired;
        if (std.mem.eql(u8, input, "bright")) return .bright;
        return .{ .color = try Color.parseCLI(input) };
    }
    // ...
};
```

A missing value is `error.ValueRequired`. The raw (un-trimmed) input is compared
exactly (`std.mem.eql`) against `bright`; a match yields the `bright` variant.
Otherwise the input is handed to `Color.parseCLI` (which does its own whitespace
trim, X11 name lookup, and hex fallback) and wrapped in `.color`. A value that
is neither `bright` nor a valid color propagates `Color.parseCLI`'s
`error.InvalidValue`. Upstream's tests: `"#4e2a84"` → `color {78,42,132}`;
`"black"` → `color {0,0,0}`; `"bright"` → `bright`; `"a"` →
`error.InvalidValue`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl BoldColor {
    /// Parse a config bold-color value (upstream `BoldColor.parseCLI`): the
    /// keyword `bright` yields the bright variant (exact match on the raw input);
    /// anything else delegates to [`Color::parse_cli`]. A missing value is
    /// `ColorParseError::ValueRequired`.
    pub(crate) fn parse_cli(input: Option<&str>) -> Result<BoldColor, ColorParseError> {
        let input = input.ok_or(ColorParseError::ValueRequired)?;
        if input == "bright" {
            return Ok(BoldColor::Bright);
        }
        Ok(BoldColor::Color(Color::parse_cli(Some(input))?))
    }
}
```

`parse_cli` mirrors upstream: the `ValueRequired` guard, the one exact `bright`
keyword check on the raw input (no trim — upstream uses `std.mem.eql`), and the
delegation to `Color::parse_cli` for everything else (where the trim, X11
lookup, and hex fallback live). The error type is the shared `ColorParseError`
(`ValueRequired` here, `Invalid` propagated from `Color::parse_cli`). This is
the same shape as `TerminalColor::parse_cli`, with one keyword instead of two.

## Scope / faithfulness notes

- **Ported (bridged)**: the config `BoldColor` CLI parser
  (`BoldColor::parse_cli`, upstream `BoldColor.parseCLI`).
- **Faithful**: the `ValueRequired` guard on a missing value; the exact
  (un-trimmed) `bright` keyword check before the color path; the delegation to
  `Color::parse_cli` for the explicit-color case, including the propagated
  `Invalid` — exactly upstream's `parseCLI`.
- **Faithful adaptation**: `?[]const u8` maps to `Option<&str>`;
  `std.mem.eql(u8, input, "bright")` maps to `input == "bright"`; the shared
  `ColorParseError` carries both `ValueRequired` and `Invalid`.
- **Faithful re-use**: the explicit-color path reuses the already-ported
  `Color::parse_cli` (Experiment 474), so the trim / X11 lookup / hex behavior
  is shared, not duplicated.
- **Deferred**: `BoldColor.formatEntry` (delegates to `Color.formatEntry` /
  writes the `bright` `@tagName`; depends on the not-yet-ported config
  `EntryFormatter`), and the broader config parser/formatter (`loadCli` /
  per-field dispatch / file loading). (Consumed by later slices; this experiment
  lands the value parser.) `BoldColor::to_terminal` is already ported.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add
     `BoldColor::parse_cli(input: Option<&str>) -> Result<BoldColor, ColorParseError>`.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parseCLI` test: `parse_cli(Some("#4e2a84"))` →
     `Color(Color{78,42,132})`; `parse_cli(Some("black"))` →
     `Color(Color{0,0,0})`; `parse_cli(Some("bright"))` → `Bright`;
     `parse_cli(Some("a"))` → `Err(Invalid)`; plus a missing value (`None` →
     `Err(ValueRequired)`); and a whitespace-padded keyword
     (`parse_cli(Some(" bright"))` → `Err(Invalid)`) confirming the keyword
     match is exact/un-trimmed and falls through to `Color::parse_cli` (parallel
     to the Experiment 476 sentinel check).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty bold_color_parse_cli
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `BoldColor::parse_cli` returns `Bright` for the exact `bright` keyword and
  otherwise delegates to `Color::parse_cli`, returning
  `ColorParseError::ValueRequired` on a missing value — faithful to upstream's
  `parseCLI`;
- the tests pass (the upstream cases; the missing-value error; the
  padded-keyword case), and the existing tests still pass;
- `BoldColor.formatEntry` and the broader config parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if the `bright` keyword or a color is parsed wrong, a
missing value does not error, the keyword check is incorrectly trimmed or
loosened, an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream (`Config.zig:5626`):
`None` → `ValueRequired`, the raw exact `bright` match, and the fallback to
`Color::parse_cli` all match `BoldColor.parseCLI`; the exact un-trimmed
`== "bright"` is correct (upstream uses `std.mem.eql` before delegating);
delegating the rest to `Color::parse_cli(Some(input))` preserves named colors,
hex colors, trimming for color values, and `Invalid` propagation; the planned
tests cover the upstream cases plus missing input and the padded `" bright"` to
lock the raw keyword behavior (`:5643`); and deferring `formatEntry` is the
right scope (it depends on the not-yet-ported formatter abstraction).

Review artifacts:

- Prompt: `logs/codex-review/20260604-131130-d477-prompt.md` (design)
- Result: `logs/codex-review/20260604-131130-d477-last-message.md` (design)
