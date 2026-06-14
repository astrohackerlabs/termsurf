# Experiment 66: Font repeatable string formatter oracle

## Description

Experiment 65 promoted the six scalar-shaped font formatter rows and left 86
formatter rows as `Audit covered`. The next compact font slice is the five rows
backed by `RepeatableString`: the four font-family rows and `font-feature`.

These rows share the same formatter behavior: an empty list emits one void line,
and a populated list emits one line per item in insertion order.

The target rows are:

- `font-family`;
- `font-family-bold`;
- `font-family-italic`;
- `font-family-bold-italic`;
- `font-feature`.

CFG-218 should remain `Gap` because 81 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `font_repeatable_string_config_formatter_family_oracle` test.
  - Cover empty-list void output for each target row.
  - Cover multiple formatted lines in insertion order for each target row.
  - Cover raw-empty reset behavior.
  - Cover byte-preserving string output for representative font-family and
    font-feature values.
  - Cover representative row order across the target rows.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the five target rows as `font repeatable string`.
  - Detect `font_repeatable_string_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `font repeatable string`.
  - Keep Experiment 66 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 122 `Oracle complete` rows and 81
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml font_repeatable_string_config_formatter_family_oracle`
  passes.
- Existing representative parser/formatter tests for the covered value shape
  still pass:
  - `cargo test --manifest-path roastty/Cargo.toml repeatable_string_font_config_parser_family_oracle`
  - `cargo test --manifest-path roastty/Cargo.toml config_font_family_finalize_inherits_regular_family`
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=122`;
  - `audit_covered=81`;
  - `gap=0`.
- Run this matrix assertion:

  ```bash
  python3 - <<'PY'
  from pathlib import Path

  matrix = Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text()
  rows = Path('issues/0805-roastty-ghostty-parity/config-formatter-inventory.md').read_text().splitlines()

  def row_for(option: str) -> str:
      for line in rows:
          if not line.startswith('| FORMAT-'):
              continue
          cells = [cell.strip() for cell in line.strip('|').split('|')]
          if len(cells) > 1 and cells[1] == f'`{option}`':
              return line
      raise AssertionError(f'missing row for {option}')

  cfg218 = matrix.split('| CFG-218 |', 1)[1].split('\n', 1)[0]
  assert '| Gap    |' in cfg218 or '| Gap |' in cfg218, cfg218
  assert 'Experiment 66 inventories formatter coverage: 122 rows Oracle complete; 81 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  for option in [
      'font-family',
      'font-family-bold',
      'font-family-italic',
      'font-family-bold-italic',
      'font-feature',
  ]:
      row = row_for(option)
      assert 'font repeatable string' in row and 'Oracle complete' in row, row

  for option in [
      'font-variation',
      'font-codepoint-map',
      'font-style',
      'font-synthetic-style',
      'font-shaping-break',
  ]:
      row = row_for(option)
      assert 'font' in row and 'Audit covered' in row, row

  for option in ['cursor-style', 'window-theme', 'env', 'input']:
      row = row_for(option)
      assert 'custom format_entry' in row and 'Audit covered' in row, row

  print('matrix assertions passed')
  PY
  ```

- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer verified that the README links Experiment 66 as `Designed`, the
experiment has the required sections, the five target rows match the current
`RepeatableString` font-family/font-feature shape, pinned Ghostty formatter
behavior supports the planned oracle, and the expected 117/86/0 to 122/81/0
count movement is plausible.

## Result

**Result:** Pass

Added `font_repeatable_string_config_formatter_family_oracle`, split the five
`RepeatableString` font rows into a `font repeatable string` formatter family,
and promoted only that family.

The new oracle proves:

- empty lists format as one void line for all five rows;
- populated lists format one line per item in insertion order;
- raw-empty values reset each row back to void output;
- representative font-family and font-feature strings are byte-preserving;
- representative row order is stable within formatter order.

The regenerated formatter inventory now reports:

```text
ghostty_canonical=203
roastty_formatter_rows=203
missing_canonical_formatter_rows=0
extra_formatter_rows=0
oracle_complete=122
audit_covered=81
gap=0
no_output_rows=1
```

CFG-218 remains `Gap`, as intended, because 81 formatter rows still need
dedicated non-default formatter oracles.

Verification:

- `cargo test --manifest-path roastty/Cargo.toml font_repeatable_string_config_formatter_family_oracle`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml repeatable_string_font_config_parser_family_oracle`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml config_font_family_finalize_inherits_regular_family`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  passed.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reported the expected 203/203 rows, 122 `Oracle complete`, 81 `Audit covered`,
  and 0 `Gap`.
- Matrix assertion passed: CFG-218 remains `Gap`; all five
  `font repeatable string` formatter rows are `Oracle complete`; representative
  complex font rows and custom `format_entry` rows remain `Audit covered`.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer reran the focused oracle, the related parser/finalize/default
formatter tests, `cargo fmt --check`, `git diff --check`, and the matrix
assertion. The reviewer confirmed that the generated formatter inventory has 203
rows, 122 `Oracle complete`, 81 `Audit covered`, and 0 gaps, only the five
target rows are classified as `font repeatable string`, README status is `Pass`,
the experiment has `Result` and `Conclusion`, and the result commit had not been
made before review.

## Conclusion

The repeatable-string font formatter rows are now oracle-complete. CFG-218
remains open with 81 audit-covered formatter rows: 11 complex font rows and 70
custom `format_entry` rows.
