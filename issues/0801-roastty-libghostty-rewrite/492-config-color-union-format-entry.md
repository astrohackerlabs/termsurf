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

# Experiment 492: the color-union formatters (TerminalColor / BoldColor format_entry)

## Description

With the config `EntryFormatter` stood up and `Color::format_entry` landed
(Experiment 491), this experiment ports the two color-**union** `formatEntry`
methods that build on it: `TerminalColor.formatEntry` and
`BoldColor.formatEntry` (upstream `Config.TerminalColor` / `Config.BoldColor`).
Each delegates to `Color::format_entry` for its explicit-color case and writes
its keyword (`cell-foreground` / `cell-background` / `bright`) as a string entry
otherwise.

## Upstream behavior

In `config/Config.zig`:

```zig
// TerminalColor
pub fn formatEntry(self: TerminalColor, formatter: formatterpkg.EntryFormatter) !void {
    switch (self) {
        .color => try self.color.formatEntry(formatter),
        .@"cell-foreground",
        .@"cell-background",
        => try formatter.formatEntry([:0]const u8, @tagName(self)),
    }
}

// BoldColor
pub fn formatEntry(self: BoldColor, formatter: formatterpkg.EntryFormatter) !void {
    switch (self) {
        .color => try self.color.formatEntry(formatter),
        .bright => try formatter.formatEntry([:0]const u8, @tagName(self)),
    }
}
```

- `TerminalColor`: an explicit `.color` delegates to `Color.formatEntry` (→
  `name = #rrggbb\n`); the cell sentinels write their tag name as a string entry
  (`name = cell-foreground\n` / `name = cell-background\n`).
- `BoldColor`: an explicit `.color` delegates to `Color.formatEntry`; `.bright`
  writes `name = bright\n`.

Upstream's tests: `TerminalColor` `formatConfig` for `cell-foreground` →
`a = cell-foreground\n`; `BoldColor` `formatConfig` for `bright` →
`a = bright\n`.

## Rust mapping (`roastty/src/config/mod.rs`)

```rust
impl TerminalColor {
    /// Format as a config entry (upstream `TerminalColor.formatEntry`): an explicit
    /// `Color` delegates to [`Color::format_entry`]; the cell sentinels write their
    /// keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        match self {
            TerminalColor::Color(c) => c.format_entry(formatter),
            TerminalColor::CellForeground => formatter.entry_str("cell-foreground"),
            TerminalColor::CellBackground => formatter.entry_str("cell-background"),
        }
    }
}

impl BoldColor {
    /// Format as a config entry (upstream `BoldColor.formatEntry`): an explicit
    /// `Color` delegates to [`Color::format_entry`]; `Bright` writes its keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        match self {
            BoldColor::Color(c) => c.format_entry(formatter),
            BoldColor::Bright => formatter.entry_str("bright"),
        }
    }
}
```

Both mirror upstream: the explicit-color arm delegates to `Color::format_entry`
(the `#rrggbb` string entry from Experiment 491), and the keyword arms write the
variant's keyword via `entry_str` (the Rust equivalent of upstream's
`formatEntry([:0]const u8, @tagName(self))`). Both `TerminalColor` and
`BoldColor` are `Copy`, so `format_entry` takes `self` by value (consistent with
`Color::format_entry`).

## Scope / faithfulness notes

- **Ported (bridged)**: `TerminalColor::format_entry` (upstream
  `TerminalColor.formatEntry`) and `BoldColor::format_entry` (upstream
  `BoldColor.formatEntry`).
- **Faithful**: the explicit-color delegation to `Color::format_entry`; the
  `cell-foreground` / `cell-background` keyword entries; the `bright` keyword
  entry — exactly upstream's `formatEntry` (the keyword strings are the variant
  `@tagName`s).
- **Faithful adaptation**: `formatter.formatEntry([:0]const u8, @tagName(self))`
  → `formatter.entry_str("<keyword>")`; the `union` switch → a `match`.
- **Deferred**: the remaining types' `formatEntry` methods (ported in later
  slices, each grounded by `EntryFormatter`) and the generic field-dispatch
  `formatEntry`, and the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `TerminalColor::format_entry` (in the
   existing `impl TerminalColor`) and `BoldColor::format_entry` (in the existing
   `impl BoldColor`).
2. Tests (in `config/mod.rs`):
   - `TerminalColor::Color(Color{10,11,12})` under `a` → `"a = #0a0b0c\n"`;
     `CellForeground` → `"a = cell-foreground\n"`; `CellBackground` →
     `"a = cell-background\n"`.
   - `BoldColor::Color(Color{10,11,12})` → `"a = #0a0b0c\n"`; `Bright` →
     `"a = bright\n"`.
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty format_entry
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- `TerminalColor::format_entry` / `BoldColor::format_entry` delegate the
  explicit color to `Color::format_entry` and write the correct keyword entries
  — faithful to upstream's `formatEntry`;
- the tests pass (the color delegation; the `cell-foreground` /
  `cell-background` / `bright` keywords), and the existing tests still pass;
- the other types' `formatEntry` and the generic field-dispatch stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a formatted entry differs from upstream (wrong
delegation, wrong keyword), an unrelated item changes, or any public C API/ABI
changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed both formatter methods are faithful: the explicit-color
arm delegates to `Color::format_entry` (preserving the `#rrggbb` string entry),
and the keyword arms write the exact upstream `@tagName` strings —
`cell-foreground`, `cell-background`, and `bright` (`Config.zig:5569`/`:5633`);
the proposed tests cover the upstream `formatConfig` expectations for
`TerminalColor` and `BoldColor` plus the explicit-color delegation path
(`:5602`/`:5662`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-151211-d492-prompt.md` (design)
- Result: `logs/codex-review/20260604-151211-d492-last-message.md` (design)
