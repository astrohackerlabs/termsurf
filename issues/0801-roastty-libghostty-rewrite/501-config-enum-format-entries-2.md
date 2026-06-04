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

# Experiment 501: more enum-keyword config formatters (ClipboardAccess / NotifyOnCommandFinish / WindowColorspace / AlphaBlending / GraphemeWidthMethod)

## Description

Continuing the enum-keyword formatter pattern from Experiment 500, this
experiment ports `keyword()` + `format_entry` for five more config enums:
`ClipboardAccess`, `NotifyOnCommandFinish`, `WindowColorspace`, `AlphaBlending`,
and `GraphemeWidthMethod`. Each writes its variant's upstream tag name (the
config keyword) as a `name = keyword\n` entry — the generic enum `{t}` format.
Grounded by the `EntryFormatter` from Experiment 491.

## Upstream behavior

The generic `formatEntry` enum branch (`config/formatter.zig`) writes
`name = {tag-name}\n`. The five enums (upstream `enum`s) and their tag names
(verified against `config/Config.zig`):

- `ClipboardAccess`: `allow`, `deny`, `ask`.
- `NotifyOnCommandFinish`: `never`, `unfocused`, `always`.
- `WindowColorspace`: `srgb`, `display-p3`.
- `AlphaBlending`: `native`, `linear`, `linear-corrected`.
- `GraphemeWidthMethod`: `legacy`, `unicode`.

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets a `keyword(self) -> &'static str` (the exact upstream tag) and a
`format_entry`:

```rust
impl ClipboardAccess {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ClipboardAccess::Allow => "allow",
            ClipboardAccess::Deny => "deny",
            ClipboardAccess::Ask => "ask",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl NotifyOnCommandFinish {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            NotifyOnCommandFinish::Never => "never",
            NotifyOnCommandFinish::Unfocused => "unfocused",
            NotifyOnCommandFinish::Always => "always",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl WindowColorspace {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            WindowColorspace::Srgb => "srgb",
            WindowColorspace::DisplayP3 => "display-p3",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl AlphaBlending {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            AlphaBlending::Native => "native",
            AlphaBlending::Linear => "linear",
            AlphaBlending::LinearCorrected => "linear-corrected",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl GraphemeWidthMethod {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            GraphemeWidthMethod::Legacy => "legacy",
            GraphemeWidthMethod::Unicode => "unicode",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}
```

Each `keyword` is the exact upstream tag name (verified), and `format_entry`
writes `name = keyword\n` (the generic `{t}` enum branch). All five enums are
`Copy`, so the methods take `self` by value.

## Scope / faithfulness notes

- **Ported (bridged)**: `keyword` + `format_entry` for `ClipboardAccess`,
  `NotifyOnCommandFinish`, `WindowColorspace`, `AlphaBlending`, and
  `GraphemeWidthMethod` (upstream's generic enum `{t}` format for these five).
- **Faithful**: each variant maps to its exact upstream tag name (incl. the
  kebab `display-p3` / `linear-corrected`), written as `name = keyword\n` —
  exactly upstream's enum branch.
- **Faithful adaptation**: the comptime `{t}` (tag name) → an explicit
  `keyword(self)` match; `formatEntry` → `entry_str(self.keyword())`.
- **Deferred**: the remaining config enums' `keyword` / `format_entry` (ported
  in later slices), the other generic field-dispatch cases (float `{d}`,
  optional recurse), `QuickTerminalSize`, and the broader config
  parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `keyword` + `format_entry` for the five
   enums (each in its existing or a new `impl`).
2. Tests (in `config/mod.rs`): each variant of the five enums formats to
   `"a = {keyword}\n"` (e.g. `WindowColorspace::DisplayP3` →
   `"a = display-p3\n"`; `AlphaBlending::LinearCorrected` →
   `"a = linear-corrected\n"`; `ClipboardAccess::Ask` → `"a = ask\n"`;
   `NotifyOnCommandFinish::Unfocused` → `"a = unfocused\n"`;
   `GraphemeWidthMethod::Legacy` → `"a = legacy\n"`).
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
  — faithful to upstream's enum branch;
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
sets, including `display-p3` and `linear-corrected`
(`Config.zig:8982`/`:9591`/`:9597`/ `:9210`/`:10214`); and that
`entry_str(self.keyword())` remains the right equivalent of the generic enum
`{t}` branch, with testing every variant adequate (`formatter.zig:52`).

Review artifacts:

- Prompt: `logs/codex-review/20260604-160222-d501-prompt.md` (design)
- Result: `logs/codex-review/20260604-160222-d501-last-message.md` (design)
