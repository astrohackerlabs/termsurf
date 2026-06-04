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

# Experiment 505: the remaining small enum-keyword config formatters (OscColorReportFormat / ConfirmCloseSurface / LinkPreviews / WindowSubtitle / WindowPaddingColor)

## Description

Continuing the enum-keyword formatter pattern (Experiments 500–504), this
experiment ports `keyword()` + `format_entry` for the five remaining small
config enums: `OscColorReportFormat` (`osc-color-report-format`),
`ConfirmCloseSurface` (`confirm-close-surface`), `LinkPreviews`
(`link-previews`), `WindowSubtitle` (`window-subtitle`), and
`WindowPaddingColor` (`window-padding-color`). Each writes its variant's
upstream tag name (the config keyword) as a `name = keyword\n` entry — the
generic enum `{t}` format. Grounded by the `EntryFormatter` from Experiment 491.

These enums are notable for their bool-aliased (`false` / `true`) and digit-led
tags (`8-bit`, `16-bit`); the keyword strings must reproduce those exact tag
names.

## Upstream behavior

The generic `formatEntry` enum branch (`config/formatter.zig`) writes
`name = {tag-name}\n`. The five enums and their tag names (verified against
`config/Config.zig`):

- `OscColorReportFormat` (`osc-color-report-format`, upstream
  `OSCColorReportFormat`, `Config.zig:8966`): `none`, `8-bit`, `16-bit`.
- `ConfirmCloseSurface` (`confirm-close-surface`, `Config.zig:5235`): `false`,
  `true`, `always`.
- `LinkPreviews` (`link-previews`, `Config.zig:5282`): `false`, `true`, `osc8`.
- `WindowSubtitle` (`window-subtitle`, `Config.zig:5277`): `false`,
  `working-directory`.
- `WindowPaddingColor` (`window-padding-color`, `Config.zig:5271`):
  `background`, `extend`, `extend-always`.

All are plain/extern `enum`s, formatted through the same generic enum branch
(`@tagName`), which yields the literal tag text including the digit-led and
kebab-case `@"..."` tags.

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets a `keyword(self) -> &'static str` (the exact upstream tag) and a
`format_entry`:

```rust
impl OscColorReportFormat {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            OscColorReportFormat::None => "none",
            OscColorReportFormat::Bits8 => "8-bit",
            OscColorReportFormat::Bits16 => "16-bit",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl ConfirmCloseSurface {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ConfirmCloseSurface::False => "false",
            ConfirmCloseSurface::True => "true",
            ConfirmCloseSurface::Always => "always",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl LinkPreviews {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            LinkPreviews::False => "false",
            LinkPreviews::True => "true",
            LinkPreviews::Osc8 => "osc8",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl WindowSubtitle {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowSubtitle::False => "false",
            WindowSubtitle::WorkingDirectory => "working-directory",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl WindowPaddingColor {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowPaddingColor::Background => "background",
            WindowPaddingColor::Extend => "extend",
            WindowPaddingColor::ExtendAlways => "extend-always",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}
```

Each `keyword` is the exact upstream tag name (verified), and `format_entry`
writes `name = keyword\n` (the generic `{t}` enum branch). The new methods are
added to each enum's existing or a new `impl`. All five enums are `Copy`, so the
methods take `self` by value.

## Scope / faithfulness notes

- **Ported (bridged)**: `keyword` + `format_entry` for the five enums
  (upstream's generic enum `{t}` format for each).
- **Faithful**: each variant maps to its exact upstream tag name — including the
  bool-aliased `false` / `true`, the digit-led `8-bit` / `16-bit`, and the
  kebab-case `working-directory` / `extend-always` — written as
  `name = keyword\n`, exactly upstream's enum branch.
- **Faithful adaptation**: the comptime `{t}` (tag name) → an explicit
  `keyword(self)` match; `formatEntry` → `entry_str(self.keyword())`.
- **Deferred**: the remaining config enums' `keyword` / `format_entry`
  (`FontStyle`, `FontShapingBreak`, `CustomShaderAnimation`,
  `MouseShiftCapture`), the other generic field-dispatch cases (float `{d}`,
  optional recurse), `QuickTerminalSize`, and the broader config
  parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `keyword` + `format_entry` for the five
   enums (each in its existing or a new `impl`).
2. Tests (in `config/mod.rs`): each variant of the five enums formats to
   `"a = {keyword}\n"` (e.g. `OscColorReportFormat::Bits16` → `"a = 16-bit\n"`;
   `WindowSubtitle::WorkingDirectory` → `"a = working-directory\n"`;
   `WindowPaddingColor::ExtendAlways` → `"a = extend-always\n"`).
3. Format and test (`cargo fmt`, accept output).

## Verification

```bash
cargo fmt
cargo fmt -- --check
cargo test -p roastty enum_format
cargo test -p roastty
cargo build -p roastty            # no warnings
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/font roastty/src/renderer roastty/src/config && exit 1 || true
rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c && exit 1 || true
git diff --check
```

The experiment **passes** if:

- each enum's `keyword` / `format_entry` writes `name = {exact upstream tag}\n`
  — faithful to upstream's enum branch, including the bool-aliased, digit-led,
  and kebab-case tags;
- the tests pass (every variant of the five enums), and the existing tests still
  pass;
- the other config enums' formatters and the remaining generic field-dispatch
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a keyword differs from the upstream tag name, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the mappings are exact for all five upstream enum tag
sets, including `8-bit` / `16-bit`, bare `false` / `true`, `osc8` (no hyphen),
`working-directory`, and `extend-always` (`Config.zig:5235`/`:5271`/`:5277`/
`:5282`/`:8966`); and that `entry_str(self.keyword())` is the faithful Rust
equivalent of the generic enum formatter branch, which writes `name = tag\n` via
`{t}` (`formatter.zig:52`), with testing every variant adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-162209-d505-prompt.md` (design)
- Result: `logs/codex-review/20260604-162209-d505-last-message.md` (design)
