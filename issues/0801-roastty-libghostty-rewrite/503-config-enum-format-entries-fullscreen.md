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

# Experiment 503: the fullscreen-style enum-keyword config formatters (Fullscreen / NonNativeFullscreen)

## Description

Continuing the enum-keyword formatter pattern (Experiments 500–502), this
experiment ports `keyword()` + `format_entry` for the two fullscreen-style
config enums: `Fullscreen` (`fullscreen`) and `NonNativeFullscreen`
(`macos-non-native-fullscreen`). Each writes its variant's upstream tag name
(the config keyword) as a `name = keyword\n` entry — the generic enum `{t}`
format. Grounded by the `EntryFormatter` from Experiment 491.

These two enums are notable because their first two tags are the bool-aliased
`false` / `true` (Zig `enum(c_int)` with bare `false` / `true` tag names), and
the remaining tags are kebab-case (`non-native-visible-menu`, etc.). The keyword
strings must reproduce those exact tag names.

## Upstream behavior

The generic `formatEntry` enum branch (`config/formatter.zig`) writes
`name = {tag-name}\n`. The two enums and their tag names (verified against
`config/Config.zig`):

- `Fullscreen` (`fullscreen`, `Config.zig:5263`): `false`, `true`, `non-native`,
  `non-native-visible-menu`, `non-native-padded-notch`.
- `NonNativeFullscreen` (`macos-non-native-fullscreen`, `Config.zig:5253`):
  `false`, `true`, `visible-menu`, `padded-notch`.

Both are extern `enum(c_int)`, formatted through the same generic enum branch
(`@tagName`), which yields the literal tag text including the kebab-case
`@"..."` tags.

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets a `keyword(self) -> &'static str` (the exact upstream tag) and a
`format_entry`:

```rust
impl Fullscreen {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            Fullscreen::False => "false",
            Fullscreen::True => "true",
            Fullscreen::NonNative => "non-native",
            Fullscreen::NonNativeVisibleMenu => "non-native-visible-menu",
            Fullscreen::NonNativePaddedNotch => "non-native-padded-notch",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl NonNativeFullscreen {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            NonNativeFullscreen::False => "false",
            NonNativeFullscreen::True => "true",
            NonNativeFullscreen::VisibleMenu => "visible-menu",
            NonNativeFullscreen::PaddedNotch => "padded-notch",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}
```

Each `keyword` is the exact upstream tag name (verified), and `format_entry`
writes `name = keyword\n` (the generic `{t}` enum branch). Both enums are
`Copy`, so the methods take `self` by value.

## Scope / faithfulness notes

- **Ported (bridged)**: `keyword` + `format_entry` for `Fullscreen` and
  `NonNativeFullscreen` (upstream's generic enum `{t}` format for these two).
- **Faithful**: each variant maps to its exact upstream tag name — including the
  bool-aliased `false` / `true` and the kebab-case tags — written as
  `name = keyword\n`, exactly upstream's enum branch.
- **Faithful adaptation**: the comptime `{t}` (tag name) → an explicit
  `keyword(self)` match; `formatEntry` → `entry_str(self.keyword())`.
- **Deferred**: the remaining config enums' `keyword` / `format_entry`
  (`OscColorReportFormat`, `ConfirmCloseSurface`, `LinkPreviews`,
  `WindowSubtitle`, `WindowPaddingColor`, `BackgroundImageFit`,
  `BackgroundImagePosition`, `FontStyle`, `FontShapingBreak`,
  `CustomShaderAnimation`, `MouseShiftCapture`), the other generic
  field-dispatch cases (float `{d}`, optional recurse), `QuickTerminalSize`, and
  the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `keyword` + `format_entry` for `Fullscreen`
   and `NonNativeFullscreen` (each in a new `impl`).
2. Tests (in `config/mod.rs`): each variant of the two enums formats to
   `"a = {keyword}\n"` (e.g. `Fullscreen::NonNativeVisibleMenu` →
   `"a = non-native-visible-menu\n"`; `NonNativeFullscreen::PaddedNotch` →
   `"a = padded-notch\n"`; the bool-aliased `Fullscreen::False` →
   `"a = false\n"`).
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
  — faithful to upstream's enum branch, including the bool-aliased and
  kebab-case tags;
- the tests pass (every variant of the two enums), and the existing tests still
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
findings**. It confirmed the mappings are exact for both upstream enum tag sets,
including the bare `false` / `true` tags and the kebab-case fullscreen variants
(`Config.zig:5253`/`:5263`); and that `entry_str(self.keyword())` is the
faithful Rust shape for the generic enum formatter branch, which writes
`name = tag\n` via `{t}` (`formatter.zig:52`), with testing every variant
adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-161553-d503-prompt.md` (design)
- Result: `logs/codex-review/20260604-161553-d503-last-message.md` (design)
