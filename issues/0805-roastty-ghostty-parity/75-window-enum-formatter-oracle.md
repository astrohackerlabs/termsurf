# Experiment 75: Window enum formatter oracle

## Description

Experiment 74 promoted the three click action formatter rows and left CFG-218 at
149 `Oracle complete` rows, 54 `Audit covered` rows, and 0 formatter gaps.

The next compact formatter shape is window enum output. Pinned Ghostty uses
simple enum tag formatting for these canonical options:

- `window-theme`: `auto`, `system`, `light`, `dark`, `ghostty`;
- `window-save-state`: `default`, `never`, `always`;
- `window-new-tab-position`: `current`, `end`;
- `window-show-tab-bar`: `always`, `auto`, `never`.

This experiment should promote exactly those four rows. It should not promote
`window-decoration`, window padding rows, optional window titlebar colors,
platform-specific window rows, resize-overlay rows, or other keyword-like
formatters.

CFG-218 should remain `Gap` because 50 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `window_enum_config_formatter_family_oracle` test.
  - Cover every upstream keyword for `WindowTheme`, `WindowSaveState`,
    `WindowNewTabPosition`, and `WindowShowTabBar`.
  - Assert direct enum `format_entry` output.
  - Assert `Config::set` plus `format_config` output for representative
    non-default values.
  - Assert raw-empty reset behavior for all four rows.
  - Assert representative order around `window-subtitle`, `window-theme`,
    `window-colorspace`, `window-save-state`, `window-new-tab-position`,
    `window-show-tab-bar`, `window-titlebar-background`, and `resize-overlay`.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the four covered options as `window enum`.
  - Detect `window_enum_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `window enum`.
  - Make Experiment 75 the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 153 `Oracle complete` rows and 50
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml` is run after Rust edits.
- `cargo test --manifest-path roastty/Cargo.toml window_enum_config_formatter_family_oracle`
  passes and runs at least one test.
- Existing representative window enum tests still pass:
  - `cargo test --manifest-path roastty/Cargo.toml window_theme_keywords_and_format_entry`;
  - `cargo test --manifest-path roastty/Cargo.toml window_save_state_keywords_and_format_entry`;
  - `cargo test --manifest-path roastty/Cargo.toml window_tab_keywords_and_format_entry`;
  - `cargo test --manifest-path roastty/Cargo.toml config_get_window_theme_returns_default_and_file_values`;
  - `cargo test --manifest-path roastty/Cargo.toml config_get_window_save_state_returns_default_and_file_values`;
  - `cargo test --manifest-path roastty/Cargo.toml window_tab_titlebar_config_parse_format_compat_and_diagnose`;
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=153`;
  - `audit_covered=50`;
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
  assert 'Experiment 75 inventories formatter coverage: 153 rows Oracle complete; 50 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  expected_window_enum = {
      'window-theme',
      'window-save-state',
      'window-new-tab-position',
      'window-show-tab-bar',
  }
  actual_window_enum = set()
  evidence_rows = set()
  for line in rows:
      if not line.startswith('| FORMAT-'):
          continue
      cells = [cell.strip() for cell in line.strip('|').split('|')]
      option = cells[1].strip('`')
      family = cells[3]
      evidence = cells[5]
      if family == 'window enum':
          actual_window_enum.add(option)
      if 'Window enum formatter oracle' in evidence:
          evidence_rows.add(option)
  assert actual_window_enum == expected_window_enum, actual_window_enum
  assert evidence_rows == expected_window_enum, evidence_rows

  for option in expected_window_enum:
      row = row_for(option)
      assert 'window enum' in row and 'Oracle complete' in row, row

  for option in ['window-decoration', 'window-padding-x', 'window-titlebar-background', 'resize-overlay', 'macos-window-buttons']:
      row = row_for(option)
      assert 'window enum' not in row, row

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

## Result

**Result:** Pass

Experiment 75 promoted exactly the four planned window enum formatter rows:
`window-theme`, `window-save-state`, `window-new-tab-position`, and
`window-show-tab-bar`.

Implementation:

- Added `window_enum_config_formatter_family_oracle` in
  `roastty/src/config/mod.rs`.
- Classified exactly those four rows as the `window enum` formatter family in
  `config_formatter_inventory.py`.
- Regenerated `config-formatter-inventory.md` and `config-matrix.md`.

Verification completed:

- `cargo fmt --manifest-path roastty/Cargo.toml`
- `cargo test --manifest-path roastty/Cargo.toml window_enum_config_formatter_family_oracle`
  passed with 1 test.
- Representative existing tests passed:
  - `cargo test --manifest-path roastty/Cargo.toml window_theme_keywords_and_format_entry`
  - `cargo test --manifest-path roastty/Cargo.toml window_save_state_keywords_and_format_entry`
  - `cargo test --manifest-path roastty/Cargo.toml window_tab_keywords_and_format_entry`
  - `cargo test --manifest-path roastty/Cargo.toml config_get_window_theme_returns_default_and_file_values`
  - `cargo test --manifest-path roastty/Cargo.toml config_get_window_save_state_returns_default_and_file_values`
  - `cargo test --manifest-path roastty/Cargo.toml window_tab_titlebar_config_parse_format_compat_and_diagnose`
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
- The formatter inventory generator reported:
  - `ghostty_canonical=203`
  - `roastty_formatter_rows=203`
  - `missing_canonical_formatter_rows=0`
  - `extra_formatter_rows=0`
  - `oracle_complete=153`
  - `audit_covered=50`
  - `gap=0`
  - `no_output_rows=1`
- The matrix assertion passed and verified:
  - CFG-218 remains `Gap`.
  - The CFG-218 count text is now 153 Oracle complete rows, 50 not Oracle
    complete rows, and 0 formatter gaps.
  - Exactly the four planned rows have family `window enum`.
  - Exactly the four planned rows cite `Window enum formatter oracle` evidence.
  - `window-decoration`, `window-padding-x`, `window-titlebar-background`,
    `resize-overlay`, and `macos-window-buttons` were not promoted as
    `window enum`.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passed.
- `prettier --write --prose-wrap always --print-width 80` was run on changed
  Markdown files after the generator run.
- `prettier --check --prose-wrap always --print-width 80` passed on changed
  Markdown files.

## Conclusion

The window enum formatter slice is now independently guarded. CFG-218 remains
open because 50 formatter rows still need non-default formatter oracles, but the
window enum family has no remaining formatter evidence gap.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.
