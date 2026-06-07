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

## Result

**Result:** Pass

The Issue 801 configuration checklist no longer describes the current Roastty
config layer as only a skeleton. The README now records the existing scoped
coverage while keeping every config row unchecked:

- `Config` has real grouped fields for app/window lifecycle, clipboard, mouse,
  config files/defaults, shell integration, notify/bell,
  window/color/background, font/style, terminal rendering, title/theme, and
  macOS options.
- Config parsing and loading cover config-file lines, CLI config arguments,
  default-file candidates, optional files, recursive `config-file` loading,
  cycle diagnostics, and C ABI load plumbing.
- Validation and diagnostics cover per-field parse errors,
  recursive/default-file load errors, path expansion, numeric/string/enum/flag
  validation, and focused finalizers.
- Keybind parsing/storage/diagnostics, theme parsing, conditionals,
  comma/string/unicode-range helpers, clipboard codepoint maps, and formatter
  foundations exist.

The rows intentionally remain incomplete because full Ghostty config key
coverage, full finalization parity, key-remap, and full formatter/export
coverage are still open.

Verification:

- Inspected:
  - `roastty/src/config/mod.rs`
  - `roastty/src/config/loader.rs`
  - `roastty/src/config/formatter.rs`
  - `roastty/src/config/comma_splitter.rs`
  - `roastty/src/config/conditional.rs`
  - `roastty/src/config/edit.rs`
  - `roastty/src/config/string.rs`
  - `roastty/src/config/unicode_range.rs`
- `cargo test -p roastty config_load -- --nocapture --test-threads=1` — 13
  passed
- `cargo test -p roastty config_set -- --nocapture --test-threads=1` — 9 passed
- `cargo test -p roastty config_format -- --nocapture --test-threads=1` — 1
  passed
- `cargo test -p roastty config_cli_keybind -- --nocapture --test-threads=1` — 9
  passed
- `cargo test -p roastty theme -- --nocapture --test-threads=1` — 9 passed
- `cargo test -p roastty config_get -- --nocapture --test-threads=1` — 33 passed
- `cargo test -p roastty config -- --nocapture --test-threads=1` — 253 passed
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/794-config-checklist-sync.md`
  — passed
- `git diff --check` — passed

## Conclusion

The config checklist was stale rather than wrong about remaining work. Roastty
now has a substantial config foundation, so the issue should track the remaining
Ghostty parity gaps instead of implying the whole config layer is absent. The
next experiment can continue syncing another stale unchecked section or pick one
open config parity row for implementation.

## Completion Review

Codex reviewed the completed experiment and found no blocking findings. The
review approved the Pass result because the README rows remain unchecked and
scoped as partial, the remaining Ghostty parity gaps are explicit, and the
recorded verification evidence covers the config filters, Prettier, and
`git diff --check`.
