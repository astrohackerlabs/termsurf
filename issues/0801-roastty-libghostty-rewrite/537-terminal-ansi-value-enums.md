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

# Experiment 537: the non-exhaustive ANSI value enums (RenditionAspect / StatusLineType)

## Description

Continuing the `terminal::ansi` module (Experiment 536 ported `C0`), this
experiment ports the two **non-exhaustive value enums** from upstream
`terminal/ansi.zig`: `RenditionAspect` (the SGR `ESC [ m` parameter values) and
`StatusLineType` (the DECSSDT status-line type). Like `C0`, each names a small
set of meaningful numeric values and is non-exhaustive (an unrecognized value is
not a failure), so each gets a `value()` / `from_value()` pair. The remaining
`ansi.zig` enums are exhaustive bare type definitions whose semantics live in
their parser consumers — they are deferred to those.

## Upstream behavior

`terminal/ansi.zig`:

```zig
/// The SGR rendition aspects (the value is the SGR `ESC [ m` parameter value).
pub const RenditionAspect = enum(u16) {
    default = 0,
    bold = 1,
    default_fg = 39,
    default_bg = 49,
    _,                 // non-exhaustive (user-generated input never fails)
};

/// The status line type for DECSSDT.
pub const StatusLineType = enum(u16) {
    none = 0,
    indicator = 1,
    host_writable = 2,
    _,                 // non-exhaustive
};
```

Both are non-exhaustive `enum(u16)`: the named values are the recognized ones;
any other `u16` is a valid-but-unnamed value (so `@enumFromInt` over user input
never fails). `RenditionAspect`'s value is the SGR parameter (e.g. `1` = bold,
`39` = default foreground); `StatusLineType`'s is the DECSSDT type.

## Rust mapping (`roastty/src/terminal/ansi.rs`)

Each is a `#[repr(u16)]` enum with `value()` returning the parameter and
`from_value(u16) -> Option<Self>` mapping a parameter to its named aspect or
`None` (the Rust shape for the non-exhaustive `@enumFromInt`):

```rust
/// The SGR rendition aspects that can be set (upstream `terminal.ansi.RenditionAspect`).
/// The value is the SGR (`ESC [ m`) parameter value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum RenditionAspect {
    Default = 0,
    Bold = 1,
    DefaultFg = 39,
    DefaultBg = 49,
}

impl RenditionAspect {
    pub(crate) fn value(self) -> u16 {
        self as u16
    }
    pub(crate) fn from_value(value: u16) -> Option<RenditionAspect> {
        Some(match value {
            0 => RenditionAspect::Default,
            1 => RenditionAspect::Bold,
            39 => RenditionAspect::DefaultFg,
            49 => RenditionAspect::DefaultBg,
            _ => return None,
        })
    }
}

/// The status line type for DECSSDT (upstream `terminal.ansi.StatusLineType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub(crate) enum StatusLineType {
    None = 0,
    Indicator = 1,
    HostWritable = 2,
}

impl StatusLineType {
    pub(crate) fn value(self) -> u16 {
        self as u16
    }
    pub(crate) fn from_value(value: u16) -> Option<StatusLineType> {
        Some(match value {
            0 => StatusLineType::None,
            1 => StatusLineType::Indicator,
            2 => StatusLineType::HostWritable,
            _ => return None,
        })
    }
}
```

`value` reads the `#[repr(u16)]` discriminant (the parameter); `from_value` maps
a parameter to its named aspect or `None`, the faithful Rust shape for the
non-exhaustive `@enumFromInt`.

## Scope / faithfulness notes

- **Ported (bridged)**: `terminal.ansi.RenditionAspect` and
  `terminal.ansi.StatusLineType`, as
  `terminal::ansi::{RenditionAspect, StatusLineType}` + `value` / `from_value`.
- **Faithful**: the named values with their exact `u16` parameters
  (`RenditionAspect` `0` / `1` / `39` / `49`; `StatusLineType` `0` / `1` / `2`);
  the non-exhaustive behavior (an unrecognized value ⇒ `None`).
- **Faithful adaptation**: Zig's non-exhaustive `enum(u16)` + `@enumFromInt` → a
  Rust `#[repr(u16)]` enum + `from_value(u16) -> Option<Self>`; the `_` tag →
  the `None` arm.
- **Deferred**: the exhaustive `ansi.zig` enums (`CursorStyle`, `StatusDisplay`,
  `ModifyKeyFormat`, `ProtectedMode`) — bare type definitions whose param
  mappings live in their parser consumers; the rest of the VT layer (`csi`,
  `apc`, `parse_table`, `Parser`).
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/terminal/ansi.rs`: add `RenditionAspect` and `StatusLineType` (+
   their `value` / `from_value`).
2. Tests (in `ansi.rs`): each named value round-trips
   (`from_value(v.value()) == Some(v)`) with the exact parameter
   (`RenditionAspect::Bold.value() == 1`,
   `RenditionAspect::DefaultBg.value() == 49`;
   `StatusLineType::HostWritable.value() == 2`); an unrecognized value
   (`RenditionAspect::from_value(7)`, `StatusLineType::from_value(3)`) ⇒ `None`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty rendition
cargo test -p roastty status_line_type
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config roastty/src/terminal/ansi.rs && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `RenditionAspect` / `StatusLineType` have their named values with the exact
  `u16` parameters, `value` returns the parameter, and `from_value` maps a
  parameter to its named aspect or `None` — faithful to upstream's
  non-exhaustive enums;
- the tests pass (round-trip each value + unrecognized ⇒ `None`), and the
  existing tests still pass;
- the exhaustive ANSI enums and the VT layer stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a value diverges from upstream, an unrelated item
changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. The values are exact — `RenditionAspect` `default = 0` / `bold = 1`
/ `default_fg = 39` / `default_bg = 49` (`ansi.zig:42`); `StatusLineType`
`none = 0` / `indicator = 1` / `host_writable = 2` (`ansi.zig:68`). Using
`#[repr(u16)]` plus `value()` and `from_value() -> Option<Self>` is the same
faithful Rust adaptation pattern as `C0` (named values ⇒ `Some`, the
non-exhaustive `_` / `@enumFromInt` else path ⇒ `None`). Deferring
`CursorStyle`, `StatusDisplay`, `ModifyKeyFormat`, and `ProtectedMode` is
reasonable — their useful behavior is tied to parser/dispatch consumers, so
porting bare sequential type definitions now would not buy much.

Review artifacts:

- Prompt: `logs/codex-review/20260604-194148-d537-prompt.md` (design)
- Result: `logs/codex-review/20260604-194148-d537-last-message.md` (design)
