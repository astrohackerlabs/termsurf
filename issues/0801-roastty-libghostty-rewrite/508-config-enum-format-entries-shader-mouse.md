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

# Experiment 508: the last enum-keyword config formatters (CustomShaderAnimation / MouseShiftCapture)

## Description

Completing the plain-enum formatter sweep (Experiments 500–505), this experiment
ports `keyword()` + `format_entry` for the two remaining plain config enums:
`CustomShaderAnimation` (`custom-shader-animation`) and `MouseShiftCapture`
(`mouse-shift-capture`). Each writes its variant's upstream tag name (the config
keyword) as a `name = keyword\n` entry — the generic enum `{t}` format. Grounded
by the `EntryFormatter` from Experiment 491.

## Upstream behavior

The generic `formatEntry` enum branch (`config/formatter.zig`) writes
`name = {tag-name}\n`. The two enums and their tag names (verified against
`config/Config.zig`):

- `CustomShaderAnimation` (`custom-shader-animation`, `Config.zig:5244`):
  `false`, `true`, `always`.
- `MouseShiftCapture` (`mouse-shift-capture`, `Config.zig:9100`): `false`,
  `true`, `always`, `never`.

Both are extern/plain `enum`s with bool-aliased `false` / `true` tags, formatted
through the same generic enum branch (`@tagName`).

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets a `keyword(self) -> &'static str` (the exact upstream tag) and a
`format_entry`:

```rust
impl CustomShaderAnimation {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            CustomShaderAnimation::False => "false",
            CustomShaderAnimation::True => "true",
            CustomShaderAnimation::Always => "always",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl MouseShiftCapture {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MouseShiftCapture::False => "false",
            MouseShiftCapture::True => "true",
            MouseShiftCapture::Always => "always",
            MouseShiftCapture::Never => "never",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}
```

Each `keyword` is the exact upstream tag name (verified), and `format_entry`
writes `name = keyword\n` (the generic `{t}` enum branch). The new methods are
added to each enum's existing `impl`. Both enums are `Copy`, so the methods take
`self` by value.

## Scope / faithfulness notes

- **Ported (bridged)**: `keyword` + `format_entry` for `CustomShaderAnimation`
  and `MouseShiftCapture` (upstream's generic enum `{t}` format for each).
- **Faithful**: each variant maps to its exact upstream tag name — including the
  bool-aliased `false` / `true` — written as `name = keyword\n`, exactly
  upstream's enum branch.
- **Faithful adaptation**: the comptime `{t}` (tag name) → an explicit
  `keyword(self)` match; `formatEntry` → `entry_str(self.keyword())`.
- **Deferred**: the other generic field-dispatch cases (float `{d}`, optional
  recurse), `QuickTerminalSize`, and the broader config parser/formatter. With
  these two enums, every plain config enum now has a `format_entry`.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `keyword` + `format_entry` for the two enums
   (each in its existing `impl`).
2. Tests (in `config/mod.rs`): each variant of the two enums formats to
   `"a = {keyword}\n"` (e.g. `CustomShaderAnimation::Always` → `"a = always\n"`;
   `MouseShiftCapture::Never` → `"a = never\n"`).
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
  — faithful to upstream's enum branch, including the bool-aliased tags;
- the tests pass (every variant of the two enums), and the existing tests still
  pass;
- the remaining generic field-dispatch cases stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a keyword differs from the upstream tag name, an
unrelated item changes, or any public C API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the mappings are exact for both remaining enum tag sets
— `false` / `true` / `always` for `CustomShaderAnimation`, and `false` / `true`
/ `always` / `never` for `MouseShiftCapture` (`Config.zig:5244`/`:9100`); and
that `entry_str(self.keyword())` is the faithful Rust equivalent of the generic
enum formatter branch, which writes `name = tag\n` via `{t}`
(`formatter.zig:52`), with testing every variant adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-163207-d508-prompt.md` (design)
- Result: `logs/codex-review/20260604-163207-d508-last-message.md` (design)
