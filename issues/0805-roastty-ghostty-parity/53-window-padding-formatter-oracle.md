# Experiment 53: Window padding formatter oracle

## Description

Experiment 52 promoted the 12 non-font metric modifier formatter rows and left
136 formatter rows as `Audit covered`. The next smallest coherent formatter
family is `window padding`: four rows that all format through local
`format_entry` implementations near each other in `Config::format_config`.

This experiment will add a focused formatter oracle for:

- `window-padding-x`;
- `window-padding-y`;
- `window-padding-balance`;
- `window-padding-color`.

The oracle should prove non-default output, compact one-value and two-value
padding output, every balance keyword, every color keyword, default reset/empty
behavior where applicable, and representative formatter order. CFG-218 should
remain `Gap` because many formatter families still lack non-default formatter
oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add `window_padding_config_formatter_family_oracle` or extend the existing
    window-padding tests with the same explicit formatter-family scope.
  - Prove `window-padding-x` and `window-padding-y` format a single value when
    both sides match and `left,right` when they differ.
  - Prove `window-padding-balance` formats `false`, `true`, and `equal`.
  - Prove `window-padding-color` formats `background`, `extend`, and
    `extend-always`.
  - Prove reset/empty behavior for the window-padding formatter rows where the
    parser supports it.
  - Prove representative formatter order: `window-padding-x`,
    `window-padding-y`, `window-padding-balance`, `window-padding-color`.
  - Correct any stale local comment that still says the `WindowPadding`
    formatter is pending if the implementation already exists.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Detect the window-padding formatter oracle.
  - Promote only formatter rows whose family is `window padding`.
  - Keep Experiment 53 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 71 `Oracle complete` rows and 132
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml window_padding_config_formatter_family_oracle`
  passes, or the equivalent renamed/extended focused formatter test passes.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=71`;
  - `audit_covered=132`;
  - `gap=0`.
- A matrix assertion confirms:
  - CFG-217 remains `Pass`;
  - CFG-218 remains `Gap`;
  - all `boolean`, `integer`, `float`, `string`, `metric modifier`, and
    `window padding` formatter rows are `Oracle complete`;
  - non-target formatter rows are not promoted by this oracle.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

## Result

**Result:** Pass

Added `window_padding_config_formatter_family_oracle` and promoted only
formatter inventory rows whose family is `window padding`.

The oracle proves representative window-padding formatter behavior through
`Config::format_config`:

- `window-padding-x` and `window-padding-y` format a single integer when both
  sides match;
- `window-padding-x` and `window-padding-y` format `left,right` when the two
  sides differ;
- `window-padding-balance` formats `false`, `true`, and `equal`;
- `window-padding-color` formats `background`, `extend`, and `extend-always`;
- raw-empty values reset the four rows to their defaults and then format those
  defaults;
- the four rows remain in formatter order: `window-padding-x`,
  `window-padding-y`, `window-padding-balance`, `window-padding-color`.

The regenerated formatter inventory now reports:

```text
ghostty_canonical=203
roastty_formatter_rows=203
missing_canonical_formatter_rows=0
extra_formatter_rows=0
oracle_complete=71
audit_covered=132
gap=0
no_output_rows=1
```

CFG-218 remains `Gap`, as intended, because 132 formatter rows still need
dedicated non-default formatter oracles.

Verification:

- `cargo test --manifest-path roastty/Cargo.toml window_padding_config_formatter_family_oracle`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  passed.
- Matrix assertion passed: CFG-217 remains `Pass`; CFG-218 remains `Gap`;
  primitive, metric modifier, and window padding formatter families are
  `Oracle complete`; representative non-target formatter families remain
  `Audit covered`.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passed.
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/53-window-padding-formatter-oracle.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-formatter-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  completed.
- `git diff --check` passed.

## Conclusion

The window-padding formatter family is now oracle-complete. CFG-218 remains open
with 132 audit-covered formatter rows. The next formatter experiments should
target another coherent family such as repeatable paths, optional values,
colors, or key remap formatting.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Required finding:

- `roastty/src/config/mod.rs` still had a stale `WindowPadding` doc comment
  saying the `formatEntry` formatter would be ported later, even though
  Experiment 53 explicitly included correcting stale local formatter-pending
  comments.

Fix:

- Updated the `WindowPadding` doc comment to remove the stale formatter-pending
  sentence.

Re-review verdict: **Approved**.

Remaining findings: none.
