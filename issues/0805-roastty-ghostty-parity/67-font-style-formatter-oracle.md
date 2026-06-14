# Experiment 67: Font style formatter oracle

## Description

Experiment 66 promoted the repeatable-string font formatter rows and left 81
formatter rows as `Audit covered`. The next compact font slice is the five
style-shaped font rows: four `FontStyle` rows and the packed
`FontSyntheticStyle` row.

The target rows are:

- `font-style`;
- `font-style-bold`;
- `font-style-italic`;
- `font-style-bold-italic`;
- `font-synthetic-style`.

CFG-218 should remain `Gap` because 76 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `font_style_config_formatter_family_oracle` test.
  - Cover `FontStyle` `default`, `false`, named style, whitespace-preserving
    named style, and raw-empty reset output across the four style rows.
  - Cover `FontSyntheticStyle` default all-flags output, disabled all-flags
    output, mixed `[no-]flag` output, and raw-empty reset output.
  - Cover representative row order across the target rows.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the five target rows as `font style`.
  - Detect `font_style_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `font style`.
  - Keep Experiment 67 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 127 `Oracle complete` rows and 76
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml font_style_config_formatter_family_oracle`
  passes.
- Existing representative parser/formatter tests for the covered value shapes
  still pass:
  - `cargo test --manifest-path roastty/Cargo.toml font_style_format_entry`
  - `cargo test --manifest-path roastty/Cargo.toml font_style_config_parser_family_oracle`
  - `cargo test --manifest-path roastty/Cargo.toml config_font_synthetic_style_and_size_parse_and_format`
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=127`;
  - `audit_covered=76`;
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
  assert 'Experiment 67 inventories formatter coverage: 127 rows Oracle complete; 76 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  for option in [
      'font-style',
      'font-style-bold',
      'font-style-italic',
      'font-style-bold-italic',
      'font-synthetic-style',
  ]:
      row = row_for(option)
      assert 'font style' in row and 'Oracle complete' in row, row

  for option in [
      'font-variation',
      'font-codepoint-map',
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

The reviewer verified that the README links Experiment 67 as `Designed`, the
experiment has the required sections, the scope is limited to the five intended
style-shaped font rows, adjacent unpromoted rows are explicitly guarded, the
expected 122/81/0 to 127/76/0 count movement is plausible, and the referenced
existing test filters resolve in `roastty/src/config/mod.rs`.
