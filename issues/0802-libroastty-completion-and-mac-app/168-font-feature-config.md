# Experiment 168: Phase F — font feature config

## Description

Remove `font-feature` from the remaining Phase F public-config tail.

Upstream defines `font-feature` as a repeatable string field immediately after
`font-synthetic-style` and before `font-size`. The syntax is intentionally
loose: the config layer stores feature-setting strings, while deeper font code
may later interpret or ignore invalid feature settings. This experiment wires
parser/formatter/storage behavior only.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config.font_feature: RepeatableString`.
  - Use the upstream default empty repeatable list.
  - Format `font-feature` in upstream declaration order after
    `font-synthetic-style` and before `font-size`.
  - Route `Config::set("font-feature", ...)` through the existing
    `RepeatableString` parser semantics: missing values report `ValueRequired`,
    empty values clear the list, non-empty values append, and CLI replay
    overwrites prior file-loaded values before appending.
  - Update config field-order/default tests and add a focused
    `font_feature_config_*` parse/format/reset/load/CLI-overwrite/clone test.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Mark Experiment 168 as `Designed`.
  - After result, update the Phase F remaining-public-options count from 21 to
    20 and remove `font-feature` from the remaining-tail wording if this passes.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty font_feature_config`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo test -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/168-font-feature-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = `font-feature` parses, formats, resets, loads, CLI-overwrites,
clones, and reports diagnostics with upstream default/order/repeatable-string
semantics, and the full roastty test suite passes.

**Partial** = the direct parser/formatter field lands, but ordering, replay
behavior, diagnostics, or full-suite verification remains incomplete.

**Fail** = the field cannot be added without conflicting with existing font
config storage, formatter ordering, or repeatable-string semantics.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Avicenna`, fresh
context.

**Verdict:** Approved with no findings.

The reviewer verified that the README links Experiment 168 as `Designed`, the
experiment has the required sections, the scope is bounded to the single
`font-feature` public config field, upstream type/order match
`RepeatableString = .{}` between `font-synthetic-style` and `font-size`, and the
planned tests cover repeatable-string parse/reset/append/clone/CLI-overwrite
semantics plus hygiene checks.
