# Experiment 69: Codepoint map formatter oracle

## Description

Experiment 68 promoted the four `font-variation*` formatter rows and left
CFG-218 at 131 `Oracle complete` rows, 72 `Audit covered` rows, and 0 formatter
gaps.

The next compact formatter slice is the two codepoint-map rows:

- `font-codepoint-map`;
- `clipboard-codepoint-map`.

Both rows share the same upstream range formatter shape: an empty map emits one
void line, and a populated map emits one `U+XXXX[-U+YYYY]=value` line per entry
in insertion order. The font map value is the descriptor family string; the
clipboard map value is either a formatted `U+XXXX` replacement codepoint or the
literal replacement string.

This experiment should not promote `font-shaping-break`, even though it is the
last remaining `font` row after `font-codepoint-map`. `font-shaping-break` is a
packed flag formatter and needs its own proof.

CFG-218 should remain `Gap` because 70 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `codepoint_map_config_formatter_family_oracle` test.
  - Cover empty-map void output for both target rows.
  - Cover single-codepoint and range-key output.
  - Cover uppercase zero-padded hex formatting.
  - Cover font descriptor family output, clipboard codepoint replacement output,
    clipboard string replacement output, and empty-string replacement output.
  - Cover repeated entries in insertion order.
  - Cover raw-empty reset behavior through `Config::set`.
  - Cover representative `format_config` ordering around the font map, clipboard
    map, and the still-unpromoted `font-shaping-break` row.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly `font-codepoint-map` and `clipboard-codepoint-map` as
    `codepoint map`.
  - Detect `codepoint_map_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `codepoint map`.
  - Keep Experiment 69 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 133 `Oracle complete` rows and 70
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml codepoint_map_config_formatter_family_oracle`
  passes.
- Existing representative parser/formatter tests for the covered value shape
  still pass:
  - `cargo test --manifest-path roastty/Cargo.toml codepoint_map_config_parser_family_oracle`;
  - `cargo test --manifest-path roastty/Cargo.toml clipboard_codepoint_map_format_entry`;
  - `cargo test --manifest-path roastty/Cargo.toml config_codepoint_map_parses_ranges_and_formats_entries`;
  - `cargo test --manifest-path roastty/Cargo.toml config_clipboard_codepoint_map_routes_and_formats`.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=133`;
  - `audit_covered=70`;
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
  assert 'Experiment 69 inventories formatter coverage: 133 rows Oracle complete; 70 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  for option in ['font-codepoint-map', 'clipboard-codepoint-map']:
      row = row_for(option)
      assert 'codepoint map' in row and 'Oracle complete' in row, row

  row = row_for('font-shaping-break')
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

The reviewer verified that the design is linked from the issue README as
`Designed`, has the required `Description`, `Changes`, and `Verification`
sections, scopes promotion to exactly `font-codepoint-map` and
`clipboard-codepoint-map`, explicitly keeps `font-shaping-break` unpromoted,
matches pinned Ghostty formatter behavior, and has concrete count and matrix
assertions for the 131/72/0 to 133/70/0 transition.
