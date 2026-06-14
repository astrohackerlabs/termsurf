# Experiment 78: Command-finish notification formatter oracle

## Description

Experiment 77 promoted the five quick-terminal enum formatter rows and left
CFG-218 at 161 `Oracle complete` rows, 42 `Audit covered` rows, and 0 formatter
gaps.

The next compact formatter cluster is command-finish notification output. Pinned
Ghostty has three adjacent canonical options:

- `notify-on-command-finish`: enum keywords `never`, `unfocused`, `always`;
- `notify-on-command-finish-action`: packed flags `bell` and `notify`, with
  default output `bell,no-notify`;
- `notify-on-command-finish-after`: duration output, defaulting to `5s`.

This experiment should promote exactly those three rows. It should not promote
other packed-flag rows such as `bell-features`, `freetype-load-flags`,
`scroll-to-bottom`, `shell-integration-features`, or `split-preserve-zoom`, and
it should not promote unrelated duration rows such as `undo-timeout`.

CFG-218 should remain `Gap` because 39 formatter rows will still lack
non-default formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `command_finish_notification_config_formatter_family_oracle`
    test.
  - Cover every upstream keyword for `NotifyOnCommandFinish`.
  - Assert direct enum `format_entry` output.
  - Assert direct `NotifyOnCommandFinishAction::format_entry` output for
    default, enabled-notify, all-enabled, and all-disabled packed flag states.
  - Assert `Config::set` plus `format_config` output for representative
    non-default values across all three rows.
  - Assert `notify-on-command-finish-after` formats representative decomposed
    duration output.
  - Assert raw-empty reset behavior for all three rows.
  - Assert representative order around `command`, `initial-command`,
    `notify-on-command-finish`, `notify-on-command-finish-action`,
    `notify-on-command-finish-after`, `env`, and `input`.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the three covered options as `command notification`.
  - Detect `command_finish_notification_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `command notification`.
  - Make Experiment 78 the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 164 `Oracle complete` rows and 39
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml` is run after Rust edits.
- `cargo test --manifest-path roastty/Cargo.toml command_finish_notification_config_formatter_family_oracle`
  passes and runs at least one test.
- Existing representative command-finish notification tests still pass:
  - `cargo test --manifest-path roastty/Cargo.toml notify_on_command_finish_should_notify_truth_table`;
  - `cargo test --manifest-path roastty/Cargo.toml notify_on_command_finish_action_defaults_bell_true_notify_false`;
  - `cargo test --manifest-path roastty/Cargo.toml packed_flags_parse_cli_shell_notify`;
  - `cargo test --manifest-path roastty/Cargo.toml config_set_routes_notify_on_command_finish_after_duration`;
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=164`;
  - `audit_covered=39`;
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
  assert 'Experiment 78 inventories formatter coverage: 164 rows Oracle complete; 39 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218

  expected_command_notification = {
      'notify-on-command-finish',
      'notify-on-command-finish-action',
      'notify-on-command-finish-after',
  }
  actual_command_notification = set()
  evidence_rows = set()
  for line in rows:
      if not line.startswith('| FORMAT-'):
          continue
      cells = [cell.strip() for cell in line.strip('|').split('|')]
      option = cells[1].strip('`')
      family = cells[3]
      evidence = cells[5]
      if family == 'command notification':
          actual_command_notification.add(option)
      if 'Command-finish notification formatter oracle' in evidence:
          evidence_rows.add(option)
  assert actual_command_notification == expected_command_notification, actual_command_notification
  assert evidence_rows == expected_command_notification, evidence_rows

  for option in expected_command_notification:
      row = row_for(option)
      assert 'command notification' in row and 'Oracle complete' in row, row

  for option in ['bell-features', 'freetype-load-flags', 'scroll-to-bottom', 'shell-integration-features', 'split-preserve-zoom', 'undo-timeout']:
      row = row_for(option)
      assert 'command notification' not in row, row

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
