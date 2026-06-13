# Experiment 169: Phase F — font variation config

## Description

Remove the four upstream `font-variation*` fields from the remaining Phase F
public-config tail.

Upstream defines `font-variation`, `font-variation-bold`,
`font-variation-italic`, and `font-variation-bold-italic` as
`RepeatableFontVariation` fields immediately after `font-size` and before
`font-codepoint-map`. Each entry appends one variable-font axis override in
`id=value` form, where `id` is exactly four bytes and `value` is an `f64`.

This experiment wires parser/formatter/storage behavior only. Applying those
axis settings during font discovery or shaping remains later font/text runtime
work.

## Changes

- `roastty/src/config/mod.rs`
  - Add a config-local `FontVariation` value with a four-byte axis id and `f64`
    value.
  - Add `RepeatableFontVariation` with upstream parse behavior:
    - missing value reports `ValueRequired`;
    - missing `=`, axis ids whose trimmed byte length is not four, or invalid
      floats report `InvalidValue`;
    - parser trims only ASCII space and tab around the id and value;
    - each valid entry appends without the special CLI overwrite behavior used
      only by the `font-family*` repeatables;
    - set-but-empty values reset to the default empty list through upstream
      `parseIntoField` default-reset behavior before `parseCLI` is called.
  - Format an empty list as a void line and each entry as `id=value`, matching
    upstream's `RepeatableFontVariation.formatEntry`.
  - Add `Config.font_variation`, `font_variation_bold`, `font_variation_italic`,
    and `font_variation_bold_italic` fields with empty defaults.
  - Format the four fields in upstream declaration order after `font-size` and
    before `font-codepoint-map`.
  - Route `Config::set` for all four keys.
  - Update default/order tests and add focused tests for parse, whitespace,
    invalid values, empty reset, formatting, config-file diagnostics, CLI-append
    behavior, and clone/equality.

- `issues/0802-libroastty-completion-and-mac-app/README.md`
  - Mark Experiment 169 as `Designed`.
  - After result, update the Phase F remaining-public-options count from 20 to
    16 and remove font variation from the remaining-tail wording if this passes.

## Verification

Before implementation:

- Codex-native adversarial design review approves this experiment.
- Commit the reviewed plan separately from the result.

After implementation:

- `cargo test -p roastty font_variation_config`
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
- `cargo test -p roastty`
- `cargo fmt --check -p roastty`
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/169-font-variation-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
- `git diff --check`

**Pass** = all four `font-variation*` keys parse, format, reset, load,
CLI-append, clone, and report diagnostics with upstream
`RepeatableFontVariation` default/order semantics, and the full roastty suite
passes.

**Partial** = the direct parser/formatter fields land, but ordering, reset
behavior, diagnostics, or full-suite verification remains incomplete.

**Fail** = the fields cannot be added without conflicting with existing font
config storage, formatter ordering, or repeatable config semantics.

## Design Review

**Reviewer:** Codex-native adversarial review subagent `Hegel`, fresh context.

**Verdict:** Approved with no findings.

The reviewer verified that the README links Experiment 169 as `Designed`, the
experiment has the required sections, the scope is narrow and matches the Issue
802 objective, the plan is faithful to upstream `RepeatableFontVariation` and
`parseIntoField` semantics, and the verification includes the required focused
tests, full roastty suite, Rust formatting check, markdown prettier check, and
`git diff --check`.

## Result

**Result:** Pass

Roastty now stores, parses, and formats all four upstream `font-variation*`
fields: `font-variation`, `font-variation-bold`, `font-variation-italic`, and
`font-variation-bold-italic`. The fields live in upstream declaration order
after `font-size` and before `font-codepoint-map`, default to empty repeatable
lists, and format empty values as void lines.

The parser accepts `id=value` entries with exactly four bytes of axis id, trims
only ASCII space and tab around the id/value sides, parses the value as `f64`,
and appends valid entries. Direct repeatable parsing reports missing values as
`ValueRequired` and malformed or empty values as `InvalidValue`; config dispatch
preserves upstream `parseIntoField` behavior by resetting set-but-empty values
to the default empty list before parsing. CLI values append to file-loaded
values, matching upstream repeatable semantics for this field family.

The Phase F public-config tail is now 16 keys: font metric/freetype knobs,
`input`, and `keybind`.

Verification:

- `cargo test -p roastty font_variation_config` passed 1 filtered unit test plus
  the ABI harness filter.
- `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
  passed 1 filtered unit test plus the ABI harness filter.
- `cargo test -p roastty` passed 4,865 Rust unit tests, 0 failed, 4 ignored; the
  C ABI harness passed with the existing enum-conversion warnings; doc tests
  passed with 0 tests.
- `cargo fmt --check -p roastty` passed.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/169-font-variation-config.md issues/0802-libroastty-completion-and-mac-app/README.md`
  passed.
- `git diff --check` passed.

## Conclusion

`font-variation*` is no longer a public config field gap. The next Phase F slice
should continue with the remaining font metric/freetype config knobs before
moving on to `input` and `keybind`.

## Completion Review

**Reviewer:** Codex-native adversarial review subagent `Tesla`, fresh context.

**Verdict:** Approved with no findings.

The reviewer checked the uncommitted result diff against plan commit
`d2c34a5b67ca4`, compared the parser/formatter semantics with upstream
`RepeatableFontVariation` and `parseIntoField`, confirmed the README marks
Experiment 169 as `Pass`, and found no required, optional, or nit findings.
