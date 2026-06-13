# Experiment 167: Phase F — language config

## Description

Remove `language` from the remaining Phase F public-config tail.

Upstream defines `language` as an optional GUI language override string with
default `null`, placed immediately before the font-family group. This experiment
wires parser/formatter/storage behavior only. Runtime localization, GTK restart
semantics, and platform UI integration remain out of scope.

## Changes

- `roastty/src/config/mod.rs`
  - Add `Config.language: Option<String>`.
  - Use the upstream default `None`.
  - Format `language` immediately before `font-family`, preserving its upstream
    relative position ahead of the font-family group within Roastty's current
    partial formatter order.
  - Route `Config::set("language", ...)` through the existing optional-string
    helper semantics: a value stores `Some(value)`, an empty value resets to the
    default `None`, a missing value reports `ValueRequired`, and NUL-containing
    input reports `InvalidValue`.
  - Update config field-order/default tests and add a focused
    `language_config_*` parse/format/reset/load/clone test.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Mark Experiment 167 as `Designed`.
  - After result, update the Phase F remaining-public-options count from 22 to
    21 and remove `language` from the remaining-tail wording if this passes.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty language_config`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo test -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/167-language-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = `language` parses, formats, resets, loads, clones, and reports
diagnostics with upstream default/order/optional-string semantics, and the full
roastty test suite passes.

**Partial** = the direct parser/formatter field lands, but ordering, load/replay
behavior, diagnostics, or full-suite verification remains incomplete.

**Fail** = the field cannot be added without conflicting with existing config
storage, formatter ordering, or optional-string semantics.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Confucius`, fresh
context.

**Verdict:** Approved after one required wording fix.

The first review found that the design overstated upstream order by saying
`language` should format after `quick-terminal-keyboard-interactivity`; upstream
only establishes that `language` appears immediately before the `font-family`
group. The design now says to format `language` immediately before
`font-family`, preserving that upstream relative position inside Roastty's
current partial formatter order. The review also suggested making the focused
test name explicit; the design now requires a `language_config_*` test.

The re-review approved the fixes with no remaining required findings.

## Result

**Result:** Pass

Roastty now stores, parses, and formats `language` as an upstream optional
string config field. The default is `None`, formatting emits the void
`language = ` line, non-empty values store the GUI language override, empty
values reset to the default, missing values report `ValueRequired`, and
NUL-containing values report `InvalidValue`.

The formatter places `language` immediately before `font-family`, preserving the
upstream relative position ahead of the font-family group inside Roastty's
current partial formatter order. Runtime localization, GTK restart semantics,
and platform UI integration remain out of scope.

The Phase F public-config tail is now 21 keys: font
feature/variation/metric/freetype knobs, `input`, and `keybind`.

Verification:

- `cargo test -p roastty language_config` passed 1 filtered unit test plus the
  ABI harness filter.
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
  passed 1 filtered unit test plus the ABI harness filter.
- `cargo test -p roastty` passed 4,863 Rust unit tests, 0 failed, 4 ignored; the
  C ABI harness passed with the existing enum-conversion warnings; doc tests
  passed with 0 tests.
- `cargo fmt --check -p roastty` passed.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/167-language-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
  passed.
- `git diff --check` passed.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `McClintock`, fresh
context.

**Verdict:** Approved with no findings.

The reviewer verified that the working-tree diff was limited to the experiment
doc, issue README, and `roastty/src/config/mod.rs`; the result commit had not
been made; upstream fidelity, optional-string semantics, test coverage, README
status/count, and result-gate state were correct; and all claimed checks passed.

## Conclusion

The `language` config surface is complete at the parser/formatter/storage layer.
Actual GUI localization remains app/platform runtime behavior, not a missing
public config field.
