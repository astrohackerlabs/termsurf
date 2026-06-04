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

# Experiment 482: the config WindowDecoration CLI parser (WindowDecoration::parse_cli)

## Description

This experiment ports `WindowDecoration` (upstream `Config.WindowDecoration`) —
the `window-decoration` config — and, with it, the reusable boolean parser
`cli.args.parseBool` that many config fields use. `WindowDecoration` is an enum
(`auto` / `client` / `server` / `none`) whose parser first tries a boolean
(`true` → `auto`, `false` → `none`) and otherwise matches an enum-variant name.
A missing value is the `auto` default (not an error). The `enum(c_int)` FFI
discriminant and the GTK `getGObjectType` stay out of scope (the latter is
non-macOS).

## Upstream behavior

In `config/Config.zig`, `Config.WindowDecoration`, and `cli/args.zig`'s
`parseBool`:

```zig
pub const WindowDecoration = enum(c_int) {
    auto,
    client,
    server,
    none,

    pub fn parseCLI(input_: ?[]const u8) !WindowDecoration {
        const input = input_ orelse return .auto;

        return if (cli.args.parseBool(input)) |b|
            if (b) .auto else .none
        else |_| if (std.meta.stringToEnum(WindowDecoration, input)) |v|
            v
        else
            error.InvalidValue;
    }
    // ...
};
```

```zig
pub fn parseBool(v: []const u8) !bool {
    const t = &[_][]const u8{ "1", "t", "T", "true" };
    const f = &[_][]const u8{ "0", "f", "F", "false" };
    inline for (t) |str| if (mem.eql(u8, v, str)) return true;
    inline for (f) |str| if (mem.eql(u8, v, str)) return false;
    return error.InvalidValue;
}
```

- `WindowDecoration.parseCLI`: a **missing** value returns `.auto` (the default
  — _not_ an error). Otherwise it tries `parseBool`: a `true` yields `.auto`, a
  `false` yields `.none`. If the value is not a boolean, it is matched (exactly)
  against the enum-variant names `auto` / `client` / `server` / `none`; a match
  yields that variant, and no match is `error.InvalidValue`.
- `parseBool`: an exact match against `1` / `t` / `T` / `true` is `true`;
  against `0` / `f` / `F` / `false` is `false`; anything else is
  `error.InvalidValue`.

Upstream's tests: `null` → `auto`; `"true"` → `auto`; `"false"` → `none`;
`"server"` → `server`; `"client"` → `client`; `"auto"` → `auto`; `"none"` →
`none`; `""` and `"aaaa"` → `error.InvalidValue`.

The `enum(c_int)` repr (discriminants `auto = 0` … `none = 3`) is the config
C-API contract; `getGObjectType` is GTK-only (non-macOS). Both are out of scope
here.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
/// An error parsing `WindowDecoration` (upstream `error.InvalidValue`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowDecorationParseError {
    /// The value is neither a boolean nor a known variant name.
    InvalidValue,
}

/// The `window-decoration` config (upstream `Config.WindowDecoration`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowDecoration {
    Auto,
    Client,
    Server,
    None,
}

impl WindowDecoration {
    /// Parse the `window-decoration` value (upstream `WindowDecoration.parseCLI`):
    /// a missing value is `Auto`; a boolean (`true` → `Auto`, `false` → `None`)
    /// is honored first; otherwise the variant name `auto`/`client`/`server`/`none`
    /// is matched, else `InvalidValue`.
    pub(crate) fn parse_cli(
        input: Option<&str>,
    ) -> Result<WindowDecoration, WindowDecorationParseError> {
        let Some(input) = input else {
            return Ok(WindowDecoration::Auto);
        };

        if let Some(b) = parse_bool(input) {
            return Ok(if b {
                WindowDecoration::Auto
            } else {
                WindowDecoration::None
            });
        }

        match input {
            "auto" => Ok(WindowDecoration::Auto),
            "client" => Ok(WindowDecoration::Client),
            "server" => Ok(WindowDecoration::Server),
            "none" => Ok(WindowDecoration::None),
            _ => Err(WindowDecorationParseError::InvalidValue),
        }
    }
}

/// Parse a config boolean (upstream `cli.args.parseBool`): `1`/`t`/`T`/`true` are
/// `true`; `0`/`f`/`F`/`false` are `false`; anything else is `None` (upstream's
/// `error.InvalidValue`, surfaced as `None` for the try-then-fallback callers).
fn parse_bool(v: &str) -> Option<bool> {
    match v {
        "1" | "t" | "T" | "true" => Some(true),
        "0" | "f" | "F" | "false" => Some(false),
        _ => None,
    }
}
```

`parse_cli` mirrors upstream: the missing-value `Auto` default, the
boolean-first resolution (`true` → `Auto`, `false` → `None`), the exact
variant-name match, and the `InvalidValue` fallthrough. `parse_bool` mirrors
`cli.args.parseBool` exactly (the four true tokens, the four false tokens, else
failure), returning `Option<bool>` — the faithful Rust shape for upstream's
`if (parseBool(...)) |b| … else |_| …` try-then-fallback (the error becomes
`None`).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `WindowDecoration` enum and
  `WindowDecoration::parse_cli` (upstream `WindowDecoration.parseCLI`), plus the
  reusable `parse_bool` (upstream `cli.args.parseBool`) and
  `WindowDecorationParseError`.
- **Faithful**: the missing-value `Auto` default (not an error); the
  boolean-first resolution mapping `true`→`Auto` / `false`→`None`; the exact
  variant-name match for `auto`/`client`/`server`/`none`; the `InvalidValue`
  fallthrough — exactly upstream's `parseCLI`. `parse_bool` matches upstream's
  exact token sets (`1`/`t`/`T`/`true`, `0`/`f`/`F`/`false`), including the
  case-sensitivity (only `T`/`F` have an upper form; `True`/`TRUE` are not
  booleans and fall through to the variant match → `InvalidValue`).
- **Faithful adaptation**: `?[]const u8` → `Option<&str>`; `parseBool`'s `!bool`
  → `Option<bool>` (the error becomes `None` for the try-then-fallback caller);
  `std.meta.stringToEnum` → an explicit exact `match` on the variant names; the
  one upstream error → `WindowDecorationParseError`.
- **Deferred**: the `enum(c_int)` FFI discriminant / config C-API export
  (`auto = 0` … `none = 3`) and `getGObjectType` (GTK-only, non-macOS), and the
  broader config parser/formatter. (Consumed by later slices; this experiment
  lands the parser and the shared `parse_bool`.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `WindowDecorationParseError { InvalidValue }`, the `WindowDecoration`
     enum (`Auto` / `Client` / `Server` / `None`,
     `derive(Debug, Clone, Copy, PartialEq, Eq)`),
     `WindowDecoration::parse_cli`, and the private `parse_bool` helper.
2. Tests (in `config/mod.rs`):
   - mirror upstream's `parse WindowDecoration` test: `None` → `Auto`; `"true"`
     → `Auto`; `"false"` → `None`; `"server"` → `Server`; `"client"` → `Client`;
     `"auto"` → `Auto`; `"none"` → `None`; `""` and `"aaaa"` →
     `Err(InvalidValue)`.
   - exercise `parse_bool` through the parser: `"1"`/`"t"`/`"T"` → `Auto`,
     `"0"`/`"f"`/`"F"` → `None`; and case-sensitivity — `"True"` →
     `Err(InvalidValue)` (not a boolean and not a variant name).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty window_decoration
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `WindowDecoration::parse_cli` returns `Auto` for a missing value, resolves a
  boolean first (`true`→`Auto`, `false`→`None`), matches the variant names
  otherwise, and is `InvalidValue` on no match — faithful to upstream's
  `parseCLI`; `parse_bool` matches upstream's exact token sets;
- the tests pass (the upstream cases; the `parse_bool` and case-sensitivity
  cases), and the existing tests still pass;
- the `enum(c_int)` FFI repr / `getGObjectType` and the broader config
  parser/formatter stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a value is parsed wrong (wrong missing-value
default, wrong boolean mapping, wrong variant match, a non-boolean accepted as
one), an unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream:
`WindowDecoration { Auto, Client, Server, None }` matches
`auto/client/server/none` (`Config.zig:9782`); a `None` input returning `Auto`
is correct (not an error, `:9798`); the bool-first parsing is exact
(`true`→`Auto`, `false`→`None`, with a failed bool falling through to exact
enum-string matching, `:9801`); `parse_bool` matches the upstream tokens exactly
(`1/t/T/true`, `0/f/F/false`, no broader case folding, `args.zig:654`);
`Option<bool>` is the right shape for the try-then-fallback use; deferring the
`enum(c_int)` ABI and the GTK `getGObjectType` is appropriate for this internal
macOS-only slice; and the planned tests cover the upstream cases plus the
case-sensitivity edge.

Review artifacts:

- Prompt: `logs/codex-review/20260604-135258-d482-prompt.md` (design)
- Result: `logs/codex-review/20260604-135258-d482-last-message.md` (design)

## Result

**Result:** Pass

`WindowDecoration::parse_cli` and the reusable `parse_bool` were added to
`roastty/src/config/mod.rs` exactly as designed — the missing-value `Auto`
default, the boolean-first resolution (`true`→`Auto`, `false`→`None`), the exact
variant-name match, and the `InvalidValue` fallthrough; `parse_bool` matches
upstream's exact `1/t/T/true` and `0/f/F/false` token sets. The new test
`window_decoration_parse_cli_resolves_bool_and_variants` asserts the upstream
cases, the single-letter bool tokens, and the case-sensitivity edge (`"True"` →
`InvalidValue`).

Gates:

- `cargo fmt -p roastty` accepted; `--check` clean.
- `cargo test -p roastty`: 2962 passed, 0 failed (one new test; no regressions).
- `cargo build -p roastty`: no warnings.
- no-`ghostty`-name greps (font/renderer/config + lib.rs/header/abi_harness.c)
  clean; `git diff --check` clean.

## Completion Review

Codex reviewed the completed experiment and **approved** it with **no
findings**: `WindowDecoration::parse_cli` faithfully ports upstream (missing
input → `Auto`, bool tokens tried first with `true`→`Auto` / `false`→`None`,
exact enum names as the fallback); `parse_bool` matches the upstream token sets
and case-sensitivity; the test covers the upstream cases plus the single-letter
and case-sensitivity edges; and deferring the C-ABI repr, the GTK
`getGObjectType`, and the broader config parser/formatter remains properly
scoped. "Approved for the result commit."

Review artifacts:

- Prompt: `logs/codex-review/20260604-135542-r482-prompt.md` (result)
- Result: `logs/codex-review/20260604-135542-r482-last-message.md` (result)

## Conclusion

`WindowDecoration` now parses, and this slice lands the reusable `parse_bool`
(upstream `cli.args.parseBool`) — the boolean primitive that many config fields
(and the per-field parser dispatch) will use. The config parse layer now spans
`Color`, `TerminalColor`, `BoldColor`, `Palette`, `ColorList`, `Duration`,
`WindowPadding`, and `WindowDecoration`, plus reusable base-0 `u8`, base-10
`u32`, and boolean parsers. The next slice can port another self-contained
config value type's `parseCLI`, or begin the per-field parser dispatch that ties
these parsers to the aggregate `Config`, continuing toward the full config
loader.
