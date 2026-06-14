# Experiment 62: Optional path formatter oracle

## Description

Experiment 61 promoted the 10 optional single-color formatter rows and left 102
formatter rows as `Audit covered`. The next compact optional slice is optional
single-path formatting: `entry_optional` rows whose value type is
`ConfigFilePath`.

Pinned Ghostty formats required config paths as the raw path string and optional
config paths with a leading `?`. Optional `null` values format as void lines
(`key = `). Roastty already has parser coverage for optional and required path
markers, quoted literal `?path` values, parsed-empty no-op behavior, raw-empty
resets, and embedded NULs. This experiment will add a formatter-specific oracle
for the two optional single-path rows before promoting them in CFG-218.

The target rows are:

- `background-image`;
- `bell-audio-path`.

Repeatable path rows (`config-file`, `custom-shader`, `gtk-custom-css`) were
already promoted by Experiment 54 and must not be affected by this experiment.

CFG-218 should remain `Gap` because 100 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `optional_path_config_formatter_family_oracle` test.
  - Cover optional void output, required path output, optional path output,
    quoted literal `?path` output, parsed-empty no-op behavior, raw-empty reset
    behavior, embedded NUL path output, and representative row order.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify only `background-image` and `bell-audio-path` as `optional path`.
  - Detect `optional_path_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `optional path`.
  - Keep Experiment 62 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 103 `Oracle complete` rows and 100
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml optional_path_config_formatter_family_oracle`
  passes.
- `cargo test --manifest-path roastty/Cargo.toml path_config_parser_family_oracle`
  still passes.
- `cargo test --manifest-path roastty/Cargo.toml repeatable_path_config_formatter_family_oracle`
  still passes.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=103`;
  - `audit_covered=100`;
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

  cfg217 = matrix.split('| CFG-217 |', 1)[1].split('\n', 1)[0]
  cfg218 = matrix.split('| CFG-218 |', 1)[1].split('\n', 1)[0]
  assert '| Pass   |' in cfg217 or '| Pass |' in cfg217, cfg217
  assert '| Gap    |' in cfg218 or '| Gap |' in cfg218, cfg218
  assert 'Experiment 62 inventories formatter coverage: 103 rows Oracle complete; 100 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  for option in ['background-image', 'bell-audio-path']:
      row = row_for(option)
      assert 'optional path' in row and 'Oracle complete' in row, row

  for option in ['config-file', 'custom-shader', 'gtk-custom-css']:
      row = row_for(option)
      assert 'repeatable path' in row and 'Oracle complete' in row, row

  for option in ['command', 'theme', 'working-directory', 'macos-icon-screen-color']:
      row = row_for(option)
      assert 'Audit covered' in row, row

  for option in ['font-size', 'font-feature', 'window-title-font-family']:
      row = row_for(option)
      assert 'font' in row and 'Audit covered' in row, row

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

The reviewer verified the README link, required sections, narrow scope, two-row
target set, exclusion of already promoted repeatable path rows, plausible
103/100/0 count movement, and verification criteria.
