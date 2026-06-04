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

# Experiment 447: the config BoldColor type and its terminal conversion (BoldColor, to_terminal)

## Description

This experiment ports the config `BoldColor` union — the `bold-color` config
value, either an explicit `Color` or the sentinel `bright` — and its
`to_terminal` conversion to the terminal-native `Style.BoldColor`. It builds on
the `Color` value type (Experiment 445), completing the trio of color config
types (`Color` → `TerminalColor` / `BoldColor`). The renderer's `DerivedConfig`
holds `bold_color: ?terminal.Style.BoldColor`, derived from the config
`BoldColor` via `toTerminal`; roastty already has the terminal-native
`BoldColor` (`crate::terminal::style::BoldColor`), so this slice lands the
config union and the conversion into it.

## Upstream behavior

In `config/Config.zig`:

```zig
@"bold-color": ?BoldColor = null,

pub const BoldColor = union(enum) {
    color: Color,
    bright,

    /// Convert to the terminal-native BoldColor type.
    pub fn toTerminal(self: BoldColor) terminal.style.Style.BoldColor {
        return switch (self) {
            .color => |col| .{ .color = col.toTerminalRGB() },
            .bright => .bright,
        };
    }

    pub fn parseCLI(input_: ?[]const u8) !BoldColor {
        const input = input_ orelse return error.ValueRequired;
        if (std.mem.eql(u8, input, "bright")) return .bright;
        return .{ .color = try Color.parseCLI(input) };
    }
    // ... formatEntry
};
```

`BoldColor` is either an explicit `color` or the sentinel `bright` (use the
bright palette variant for bold text). `toTerminal` maps it to the terminal's
own `Style.BoldColor`: an explicit color resolves through `Color.toTerminalRGB`,
and `bright` maps to the terminal `.bright`.

## Rust mapping (`roastty/src/config/mod.rs`)

roastty's terminal-native `BoldColor` (`crate::terminal::style::BoldColor`, an
`enum { Color(Rgb), Bright }`) is the conversion target; it is imported aliased
to disambiguate from the config type:

```rust
use crate::terminal::style::BoldColor as TerminalBoldColor;

/// The `bold-color` config (upstream `Config.BoldColor`): the color to use for
/// bold text — either an explicit `Color` or the bright palette variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoldColor {
    /// An explicit color.
    Color(Color),
    /// Use the bright palette variant for bold text.
    Bright,
}

impl BoldColor {
    /// Convert to the terminal-native `BoldColor` (upstream
    /// `Config.BoldColor.toTerminal`): an explicit `Color` resolves through
    /// `to_terminal_rgb`; `Bright` maps to the terminal `Bright`.
    pub(crate) fn to_terminal(self) -> TerminalBoldColor {
        match self {
            BoldColor::Color(c) => TerminalBoldColor::Color(c.to_terminal_rgb()),
            BoldColor::Bright => TerminalBoldColor::Bright,
        }
    }
}
```

`to_terminal` is upstream's `toTerminal`: `TerminalBoldColor::Color(c's rgb)`
for an explicit color, `TerminalBoldColor::Bright` for `Bright`. The `match` is
exhaustive. `BoldColor` is `Copy`/`Eq` (`Color` is `Copy`).

## Scope / faithfulness notes

- **Ported (bridged)**: the config `BoldColor` union (`config/Config.zig`) and
  its `to_terminal` conversion (upstream `Config.BoldColor.toTerminal`).
- **Faithful**: the union has the two upstream variants (`color`, `bright`);
  `to_terminal` maps `Color` to `TerminalBoldColor::Color(rgb)` and `Bright` to
  `TerminalBoldColor::Bright` — exactly upstream's `switch`.
- **Faithful adaptation**: the `color` payload is the `Color` value type
  (Experiment 445), resolved through `to_terminal_rgb`; the conversion target is
  roastty's `crate::terminal::style::BoldColor` (the analog of upstream
  `terminal.style.Style.BoldColor`), imported aliased as `TerminalBoldColor` to
  disambiguate the two same-named types (upstream also names both `BoldColor`,
  in different namespaces).
- **Deferred**: the string parsing (`parseCLI`), the `formatEntry`, the `Config`
  struct that holds the `?BoldColor` key, and the renderer's `DerivedConfig`
  wiring that calls `to_terminal`. (Consumed by a later slice; this experiment
  lands the union and the conversion.)
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`:
   - add `pub(crate) enum BoldColor { Color(Color), Bright }` (derive
     `Debug, Clone, Copy, PartialEq, Eq`) and
     `BoldColor::to_terminal(self) -> TerminalBoldColor`. Import
     `crate::terminal::style::BoldColor as TerminalBoldColor`.
2. Tests (in `config/mod.rs`):
   - `to_terminal`: `BoldColor::Color(Color { 10, 20, 30 })` converts to
     `TerminalBoldColor::Color(Rgb::new(10, 20, 30))`; `BoldColor::Bright`
     converts to `TerminalBoldColor::Bright`; the variants distinct
     (`Color(_) != Bright`, two `Color(_)` differ) and a `Copy`/`Eq` round-trip.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty bold_color
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `BoldColor` has the two upstream variants and `to_terminal` maps `Color` to
  `TerminalBoldColor::Color(rgb)` and `Bright` to `TerminalBoldColor::Bright`
  via an exhaustive `match` — faithful to upstream's union and `toTerminal`;
- the tests pass (the conversion; the `Bright` sentinel; the distinct variants),
  and the existing tests still pass;
- the parsing, the `Config` struct, and the `DerivedConfig` wiring stay
  deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a variant is missing/extra, `to_terminal` maps a
variant the wrong way, an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It verified against the vendored upstream: the variants match
exactly (`color: Color`, `bright`, `Config.zig:5614`); `to_terminal()` is an
exact port of `toTerminal` (`Config.zig:5618`, the explicit color maps through
`Color::to_terminal_rgb()` and `Bright` maps to the terminal-native bright
variant); reusing roastty's existing terminal-side `terminal::style::BoldColor`
(`style.rs:439`) is the right conversion target; aliasing the import as
`TerminalBoldColor` is the right way to keep config `BoldColor` and terminal
`BoldColor` readable in the same module; and deferring parsing, formatting, the
`Config` field wiring, and the `DerivedConfig` consumption is appropriate. It
judged the tests (both conversion branches, value semantics) adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-110447-d447-prompt.md` (design)
- Result: `logs/codex-review/20260604-110447-d447-last-message.md` (design)
