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

# Experiment 500: the enum-keyword config formatters (CopyOnSelect / MiddleClickAction / RightClickAction / ShellIntegration)

## Description

Continuing the config **formatter** layer (Experiments 491–499), this experiment
ports the generic enum-keyword format (`{t}` — the variant tag name) for four
config enums: `CopyOnSelect`, `MiddleClickAction`, `RightClickAction`, and
`ShellIntegration`. These have no custom `formatEntry`; upstream auto-formats
them via the generic formatter's enum branch, writing the variant's **tag name**
(the config keyword). In Rust the variant names are CamelCase while the keywords
are the upstream kebab-case tag names, so each enum gets a `keyword()` method
(the exact mapping) and a `format_entry` writing it. Grounded by the
`EntryFormatter` from Experiment 491.

## Upstream behavior

In `config/formatter.zig`, the generic `formatEntry` enum branch:

```zig
.@"enum" => {
    try writer.print("{s} = {t}\n", .{ name, value });
    return;
},
```

A config enum formats to `name = {tag-name}\n`, where the tag name is the
variant's upstream identifier (the config keyword). The four enums (upstream
`enum`s) and their tag names:

- `CopyOnSelect`: `false`, `true`, `clipboard` (`config/Config.zig`).
- `MiddleClickAction`: `primary-paste`, `ignore`.
- `RightClickAction`: `ignore`, `paste`, `copy`, `copy-or-paste`,
  `context-menu`.
- `ShellIntegration`: `none`, `detect`, `bash`, `elvish`, `fish`, `nushell`,
  `zsh`.

So e.g. `RightClickAction.@"context-menu"` formats to `name = context-menu\n`.

## Rust mapping (`roastty/src/config/mod.rs`)

Each enum gets a `keyword(self) -> &'static str` (the exact upstream tag name)
and a `format_entry` writing it:

```rust
impl CopyOnSelect {
    /// The config keyword (upstream tag name).
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            CopyOnSelect::False => "false",
            CopyOnSelect::True => "true",
            CopyOnSelect::Clipboard => "clipboard",
        }
    }
    /// Format as a config entry (upstream's enum branch): the keyword.
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl MiddleClickAction {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            MiddleClickAction::PrimaryPaste => "primary-paste",
            MiddleClickAction::Ignore => "ignore",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl RightClickAction {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            RightClickAction::Ignore => "ignore",
            RightClickAction::Paste => "paste",
            RightClickAction::Copy => "copy",
            RightClickAction::CopyOrPaste => "copy-or-paste",
            RightClickAction::ContextMenu => "context-menu",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}

impl ShellIntegration {
    pub(crate) fn keyword(self) -> &'static str {
        match self {
            ShellIntegration::None => "none",
            ShellIntegration::Detect => "detect",
            ShellIntegration::Bash => "bash",
            ShellIntegration::Elvish => "elvish",
            ShellIntegration::Fish => "fish",
            ShellIntegration::Nushell => "nushell",
            ShellIntegration::Zsh => "zsh",
        }
    }
    pub(crate) fn format_entry(self, formatter: &mut EntryFormatter) {
        formatter.entry_str(self.keyword());
    }
}
```

Each `keyword` is the exact upstream tag name (verified against
`config/Config.zig`), and `format_entry` writes it as a string entry
(`name = keyword\n`) — the Rust equivalent of the generic `{t}` enum branch. All
four enums are `Copy`, so the methods take `self` by value.

## Scope / faithfulness notes

- **Ported (bridged)**: `keyword` + `format_entry` for `CopyOnSelect`,
  `MiddleClickAction`, `RightClickAction`, and `ShellIntegration` (upstream's
  generic enum `{t}` format for these four).
- **Faithful**: each variant maps to its exact upstream tag name, written as
  `name = keyword\n` — exactly upstream's enum branch. (The keywords are
  verified against the upstream enum definitions and the kebab-cased ones —
  `primary-paste`, `copy-or-paste`, `context-menu` — match the `@"…"` tags.)
- **Faithful adaptation**: the comptime `{t}` (tag name) → an explicit
  `keyword(self)` match (Rust variant names are CamelCase, not the keywords);
  `formatEntry` → `entry_str(self.keyword())`.
- **Deferred**: the remaining config enums' `keyword` / `format_entry` (ported
  in later slices), the other generic field-dispatch cases (float `{d}`,
  optional recurse), and `QuickTerminalSize` (deferred with its
  `parseFloat`-blocked parser), and the broader config parser/formatter.
- No C ABI/header/ABI-inventory change (internal Rust).

## Changes

1. `roastty/src/config/mod.rs`: add `keyword` + `format_entry` for
   `CopyOnSelect`, `MiddleClickAction`, `RightClickAction`, and
   `ShellIntegration` (each in its existing or a new `impl`).
2. Tests (in `config/mod.rs`): each variant of the four enums formats to
   `"a = {keyword}\n"` (e.g. `CopyOnSelect::Clipboard` → `"a = clipboard\n"`;
   `RightClickAction::CopyOrPaste` → `"a = copy-or-paste\n"`;
   `MiddleClickAction::PrimaryPaste` → `"a = primary-paste\n"`;
   `ShellIntegration::Nushell` → `"a = nushell\n"`), plus a representative
   variant per enum.
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
- the tests pass (every variant of the four enums), and the existing tests still
  pass;
- the other config enums' formatters and the remaining generic field-dispatch
  stay deferred;
- `cargo fmt` accepted, `cargo build -p roastty` has no warnings, and
  `cargo test -p roastty` passes with no regressions;
- the no-`ghostty`-name gates and `git diff --check` pass;
- Codex reviews the design before implementation and the result after, with all
  real findings fixed.

The experiment **fails** if a keyword differs from the upstream tag name (wrong
spelling, wrong kebab-casing), an unrelated item changes, or any public C
API/ABI changes.

## Design Review

Codex reviewed this design before implementation and **approved** it with **no
findings**. It confirmed the `keyword` mappings match the upstream enum tag
names exactly — including the kebab-case tags (`primary-paste` / `copy-or-paste`
/ `context-menu`) and the lowercase `false` / `true` strings
(`Config.zig:8619`/`:8633`/ `:8652`/`:8661`); and that
`entry_str(self.keyword())` is the right Rust equivalent for the generic enum
branch's `{t}` tag-name output (`name = keyword\n` — `formatter.zig:52`);
testing every variant is adequate.

Review artifacts:

- Prompt: `logs/codex-review/20260604-155650-d500-prompt.md` (design)
- Result: `logs/codex-review/20260604-155650-d500-last-message.md` (design)
