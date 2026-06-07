+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 794: Config Checklist Sync

## Description

The Issue 801 configuration checklist still says the config layer is a skeleton:
`Config` has only a `finalized` flag, option parsing/file loading are stubbed,
validation/finalization/diagnostics are stubbed, and keybind/theme/formatter
pieces are missing. The current Roastty tree has moved well past that state.
`roastty/src/config/` contains a real `Config` with many field groups, config
line and CLI parsing, default-file and recursive file loading, diagnostics,
formatting/export, theme parsing, keybind parsing/storage, conditional helpers,
comma splitting, string parsing, and Unicode-range parsing.

This experiment verifies the existing config layer and updates the checklist
rows from "skeleton/stubbed/missing" to scoped partial wording. It does not mark
the rows complete because the full Ghostty key set, all validators/finalizers,
key-remap, clipboard-map completeness, and frontend presentation remain open.

## Changes

- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Update the `Config` struct row from "only finalized flag exists" to a
    partial field-set summary.
  - Update option parsing / CLI args / file loading from stubbed to partial
    implemented wording.
  - Update validation/finalization/diagnostics from stubbed to partial wording.
  - Update keybind parsing / theme loading / formatter/export from missing to
    partial wording.
  - Add the Experiment 794 index entry.
- `issues/0801-roastty-libghostty-rewrite/794-config-checklist-sync.md`
  - Record the verification evidence and review result.

## Verification

- Inspect current config modules:
  - `roastty/src/config/mod.rs`
  - `roastty/src/config/loader.rs`
  - `roastty/src/config/formatter.rs`
  - `roastty/src/config/comma_splitter.rs`
  - `roastty/src/config/conditional.rs`
  - `roastty/src/config/edit.rs`
  - `roastty/src/config/string.rs`
  - `roastty/src/config/unicode_range.rs`
- Run focused config tests:
  - `cargo test -p roastty config_load -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_set -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_format -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1`
  - `cargo test -p roastty theme -- --nocapture --test-threads=1`
  - `cargo test -p roastty config_get -- --nocapture --test-threads=1`
- Run the broader config filter:
  - `cargo test -p roastty config -- --nocapture --test-threads=1`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/794-config-checklist-sync.md`
- Run:
  - `git diff --check`

The experiment passes if the config modules and tests prove the old
"skeleton/stubbed/missing" wording is stale, and the README rows are updated to
scoped partial states without claiming full Ghostty config completion. It is
Partial if only parsing/loading or only field-set coverage verifies. It fails if
the original checklist wording remains accurate.

## Design Review

Codex reviewed the design and found no blocking findings. The review confirmed
that the README rows remain unchecked, the wording is scoped to partial
coverage, the open Ghostty parity gaps stay explicit, and the proposed test
filters are non-empty and relevant.
