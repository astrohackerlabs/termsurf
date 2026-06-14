# Experiment 76: Resize overlay formatter oracle

## Description

Experiment 75 promoted the four window enum formatter rows and left CFG-218 at
153 `Oracle complete` rows, 50 `Audit covered` rows, and 0 formatter gaps.

The next compact formatter cluster is resize overlay output. Pinned Ghostty has
three adjacent canonical options:

- `resize-overlay`: enum keywords `always`, `never`, `after-first`;
- `resize-overlay-position`: enum keywords `center`, `top-left`, `top-center`,
  `top-right`, `bottom-left`, `bottom-center`, `bottom-right`;
- `resize-overlay-duration`: duration output, defaulting to `750ms`.

This experiment should promote exactly those three rows. It should not promote
other duration rows such as `notify-on-command-finish-after` or `undo-timeout`,
other window rows, quick-terminal rows, or unrelated enum-like custom
formatters.

CFG-218 should remain `Gap` because 47 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `resize_overlay_config_formatter_family_oracle` test.
  - Cover every upstream keyword for `ResizeOverlay` and
    `ResizeOverlayPosition`.
  - Assert direct enum `format_entry` output.
  - Assert `Config::set` plus `format_config` output for representative
    non-default values across all three rows.
  - Assert `resize-overlay-duration` formats representative decomposed duration
    output.
  - Assert raw-empty reset behavior for all three rows.
  - Assert representative order around `window-titlebar-background`,
    `resize-overlay`, `resize-overlay-position`, `resize-overlay-duration`, and
    `focus-follows-mouse`.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the three covered options as `resize overlay`.
  - Detect `resize_overlay_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `resize overlay`.
  - Make Experiment 76 the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 156 `Oracle complete` rows and 47
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml` is run after Rust edits.
- `cargo test --manifest-path roastty/Cargo.toml resize_overlay_config_formatter_family_oracle`
  passes and runs at least one test.
- Existing representative resize overlay tests still pass:
  - `cargo test --manifest-path roastty/Cargo.toml resize_overlay_keywords_and_format_entry`;
  - `cargo test --manifest-path roastty/Cargo.toml resize_overlay_config_parse_format_reset_and_diagnose`;
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=156`;
  - `audit_covered=47`;
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
  assert 'Experiment 76 inventories formatter coverage: 156 rows Oracle complete; 47 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  expected_resize_overlay = {
      'resize-overlay',
      'resize-overlay-position',
      'resize-overlay-duration',
  }
  actual_resize_overlay = set()
  evidence_rows = set()
  for line in rows:
      if not line.startswith('| FORMAT-'):
          continue
      cells = [cell.strip() for cell in line.strip('|').split('|')]
      option = cells[1].strip('`')
      family = cells[3]
      evidence = cells[5]
      if family == 'resize overlay':
          actual_resize_overlay.add(option)
      if 'Resize overlay formatter oracle' in evidence:
          evidence_rows.add(option)
  assert actual_resize_overlay == expected_resize_overlay, actual_resize_overlay
  assert evidence_rows == expected_resize_overlay, evidence_rows

  for option in expected_resize_overlay:
      row = row_for(option)
      assert 'resize overlay' in row and 'Oracle complete' in row, row

  for option in ['notify-on-command-finish-after', 'undo-timeout', 'window-decoration', 'quick-terminal-size', 'quick-terminal-position']:
      row = row_for(option)
      assert 'resize overlay' not in row, row

  print('matrix assertions passed')
  PY
  ```

- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files after the final generator run.
- `prettier --check --prose-wrap always --print-width 80` passes on changed
  Markdown files.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.
