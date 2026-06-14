# Experiment 72: Clipboard access formatter oracle

## Description

Experiment 71 promoted four direct keyword enum formatter rows and left CFG-218
at 138 `Oracle complete` rows, 65 `Audit covered` rows, and 0 formatter gaps.

The next narrow formatter shape is Ghostty's `ClipboardAccess` enum. Pinned
Ghostty defines one enum with three tag keywords, `allow`, `deny`, and `ask`,
and uses it for exactly two canonical config options:

- `clipboard-read`, default `ask`;
- `clipboard-write`, default `allow`.

This experiment should promote exactly those two rows. It should not promote
other keyword enum-like rows such as `copy-on-select`, click actions, window
enums, platform enums, or packed-struct/list formatters.

CFG-218 should remain `Gap` because 63 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `clipboard_access_config_formatter_family_oracle` test.
  - Cover every upstream `ClipboardAccess` keyword: `allow`, `deny`, and `ask`.
  - Assert direct enum `format_entry` output.
  - Assert `Config::set` plus `format_config` output for both `clipboard-read`
    and `clipboard-write`, including their different defaults.
  - Assert raw-empty reset behavior:
    - `clipboard-read` resets to `ask`;
    - `clipboard-write` resets to `allow`.
  - Assert representative order around `focus-follows-mouse`, `clipboard-read`,
    `clipboard-write`, `clipboard-trim-trailing-spaces`, and `copy-on-select`.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly `clipboard-read` and `clipboard-write` as
    `clipboard access`.
  - Detect `clipboard_access_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `clipboard access`.
  - Make Experiment 72 the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 140 `Oracle complete` rows and 63
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml clipboard_access_config_formatter_family_oracle`
  passes and runs at least one test.
- Existing representative parser/formatter tests for the covered value shape
  still pass:
  - `cargo test --manifest-path roastty/Cargo.toml enum_format_entries_2`;
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=140`;
  - `audit_covered=63`;
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
  assert 'Experiment 72 inventories formatter coverage: 140 rows Oracle complete; 63 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  expected_clipboard_access = {'clipboard-read', 'clipboard-write'}
  actual_clipboard_access = set()
  evidence_rows = set()
  for line in rows:
      if not line.startswith('| FORMAT-'):
          continue
      cells = [cell.strip() for cell in line.strip('|').split('|')]
      option = cells[1].strip('`')
      family = cells[3]
      evidence = cells[5]
      if family == 'clipboard access':
          actual_clipboard_access.add(option)
      if 'Clipboard access formatter oracle' in evidence:
          evidence_rows.add(option)
  assert actual_clipboard_access == expected_clipboard_access, actual_clipboard_access
  assert evidence_rows == expected_clipboard_access, evidence_rows

  for option in expected_clipboard_access:
      row = row_for(option)
      assert 'clipboard access' in row and 'Oracle complete' in row, row

  for option in ['copy-on-select', 'right-click-action', 'window-theme', 'env']:
      row = row_for(option)
      assert 'custom format_entry' in row and 'Audit covered' in row, row

  print('matrix assertions passed')
  PY
  ```

- `cargo fmt --manifest-path roastty/Cargo.toml` is run after Rust edits.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files after the final generator run.
- `prettier --check --prose-wrap always --print-width 80` passes on changed
  Markdown files.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Required findings:

- Add an explicit `cargo fmt --manifest-path roastty/Cargo.toml` verification
  step before the Rust formatter `--check`.
- Strengthen the matrix assertion to prove exactly `clipboard-read` and
  `clipboard-write` receive the new `clipboard access` family and formatter
  oracle evidence.

Fixes:

- Added the explicit `cargo fmt --manifest-path roastty/Cargo.toml` verification
  step.
- Updated the matrix assertion to compare the exact `clipboard access` family
  set and exact `Clipboard access formatter oracle` evidence set against
  `{'clipboard-read', 'clipboard-write'}`.

Final verdict after re-review: **Approved**.

Findings: none remaining.

## Result

**Result:** Pass

Implemented `clipboard_access_config_formatter_family_oracle` and promoted
exactly the two `ClipboardAccess` formatter rows:

- `clipboard-read`;
- `clipboard-write`.

Verification commands:

- `cargo fmt --manifest-path roastty/Cargo.toml` ran after the Rust edits.
- `cargo test --manifest-path roastty/Cargo.toml clipboard_access_config_formatter_family_oracle`
  passed and ran 1 unit test.
- `cargo test --manifest-path roastty/Cargo.toml enum_format_entries_2` passed
  and ran 1 unit test.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  passed and ran 1 unit test.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reported:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=140`;
  - `audit_covered=63`;
  - `gap=0`.
- The planned matrix assertion passed.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passed.
- `prettier --check --prose-wrap always --print-width 80` passed on the changed
  Markdown files.
- `git diff --check` passed.

## Conclusion

The clipboard access formatter family is now proven for `clipboard-read` and
`clipboard-write`. CFG-218 remains `Gap` because 63 custom formatter rows still
need non-default formatter oracles.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.
