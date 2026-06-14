# Experiment 83: Misc direct enum formatter oracle

## Description

Experiment 82 promoted the nine macOS enum formatter rows and left CFG-218 at
185 `Oracle complete` rows, 18 `Audit covered` rows, and 0 formatter gaps.

The next compact formatter family is the remaining direct enum set that is not
part of a larger platform or UI cluster:

- `async-backend`: `auto`, `epoll`, `io_uring`;
- `confirm-close-surface`: `false`, `true`, `always`;
- `custom-shader-animation`: `false`, `true`, `always`;
- `fullscreen`: `false`, `true`, `non-native`, `non-native-visible-menu`,
  `non-native-padded-notch`;
- `grapheme-width-method`: `legacy`, `unicode`;
- `link-previews`: `false`, `true`, `osc8`;
- `linux-cgroup`: `never`, `always`, `single-instance`;
- `shell-integration`: `none`, `detect`, `bash`, `elvish`, `fish`, `nushell`,
  `zsh`;
- `window-subtitle`: `false`, `working-directory`.

These rows all use straightforward keyword enum formatting. Roastty already has
direct enum `format_entry` coverage for most of them and parser coverage for the
larger config clusters around async/update, linux-cgroup, fullscreen, and shell
integration. This experiment should promote exactly these nine rows by proving
direct enum formatter output, `Config::set` plus `Config::format_config`, raw
empty resets, and representative local ordering. It should not promote custom
formatters such as `background-blur`, `mouse-scroll-multiplier`,
`quick-terminal-size`, `undo-timeout`, `window-decoration`, or collection rows
such as `env`, `input`, and `palette`.

CFG-218 should remain `Gap` because 9 formatter rows will still lack non-default
formatter oracles.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `misc_direct_enum_config_formatter_family_oracle` test.
  - Assert direct `format_entry` output for every `AsyncBackend`,
    `ConfirmCloseSurface`, `CustomShaderAnimation`, `Fullscreen`,
    `GraphemeWidthMethod`, `LinkPreviews`, `LinuxCgroup`, `ShellIntegration`,
    and `WindowSubtitle` keyword.
  - Assert representative `Config::set` plus `format_config` output for all nine
    rows.
  - Assert raw-empty reset behavior for all nine rows.
  - Assert representative ordering around the promoted rows without promoting
    adjacent custom rows:
    - `grapheme-width-method`, `adjust-cursor-thickness`,
      `adjust-cursor-height`;
    - `window-colorspace`, `link-previews`, `fullscreen`;
    - `fullscreen`, `title`, `class`, `x11-instance-name`;
    - `window-subtitle`, `window-theme`, `window-colorspace`;
    - `window-padding-color`, `confirm-close-surface`,
      `quit-after-last-window-closed`;
    - `quick-terminal-size`, `shell-integration`, `shell-integration-features`;
    - `osc-color-report-format`, `vt-kam-allowed`, `custom-shader`,
      `custom-shader-animation`, `bell-features`;
    - `linux-cgroup`, `linux-cgroup-memory-limit`;
    - `enquiry-response`, `async-backend`, `auto-update`.
- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Classify exactly the nine covered options as `misc direct enum`.
  - Detect `misc_direct_enum_config_formatter_family_oracle`.
  - Promote only formatter rows whose family is `misc direct enum`.
  - Make Experiment 83 the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 194 `Oracle complete` rows and 9
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml` is run after Rust edits.
- `cargo test --manifest-path roastty/Cargo.toml misc_direct_enum_config_formatter_family_oracle`
  passes and runs at least one test.
- Existing representative enum and parser tests still pass:
  - `cargo test --manifest-path roastty/Cargo.toml enum_format_entries`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_format_entries_2`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_format_entries_misc`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_format_entries_fullscreen`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_from_keyword_round_trips_and_rejects_unknown`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_from_keyword_round_trips_mac_bgimage_shader`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_from_keyword_round_trips_misc_fullscreen`;
  - `cargo test --manifest-path roastty/Cargo.toml enum_from_keyword_round_trips_shell_notify`;
  - `cargo test --manifest-path roastty/Cargo.toml async_update_config_parse_format_reset_and_diagnose`;
  - `cargo test --manifest-path roastty/Cargo.toml linux_cgroup_config_parse_format_reset_and_diagnose`;
  - `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=194`;
  - `audit_covered=9`;
  - `gap=0`.
- Run this matrix assertion:

  ```bash
  python3 - <<'PY'
  from pathlib import Path

  inventory = Path('issues/0805-roastty-ghostty-parity/config-formatter-inventory.md').read_text()
  matrix = Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text()

  expected = {
      'async-backend',
      'confirm-close-surface',
      'custom-shader-animation',
      'fullscreen',
      'grapheme-width-method',
      'link-previews',
      'linux-cgroup',
      'shell-integration',
      'window-subtitle',
  }

  promoted = set()
  still_audit = []
  for line in inventory.splitlines():
      if not line.startswith('| FORMAT-'):
          continue
      cells = [cell.strip() for cell in line.strip('|').split('|')]
      option = cells[1].strip('`')
      family = cells[3]
      status = cells[4]
      if family == 'misc direct enum' and status == 'Oracle complete':
          promoted.add(option)
      elif family == 'misc direct enum':
          still_audit.append((option, status))

  assert promoted == expected, promoted
  assert not still_audit, still_audit

  for option in [
      'background-blur',
      'env',
      'input',
      'mouse-scroll-multiplier',
      'palette',
      'quick-terminal-size',
      'selection-word-chars',
      'undo-timeout',
      'window-decoration',
  ]:
      row = next(
          line for line in inventory.splitlines()
          if line.startswith('| FORMAT-') and f'`{option}`' in line
      )
      assert 'misc direct enum' not in row, row

  cfg218 = next(line for line in matrix.splitlines() if '| CFG-218 |' in line)
  assert '| Gap |' in cfg218, cfg218
  assert 'Experiment 83 inventories formatter coverage: 194 rows Oracle complete; 9 rows are not Oracle complete and 0 rows are formatter gaps.' in cfg218, cfg218
  PY
  ```

- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  passes; remove any generated `__pycache__/` artifact afterward.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --check issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/83-misc-direct-enum-formatter-oracle.md`
  passes.
- `git diff --check` passes.

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings:

- No findings.

Reviewer evidence:

- The README links Experiment 83 as `Designed`.
- The experiment has Description, Changes, and Verification sections.
- The scope is exactly the nine remaining direct enum formatter rows.
- The current inventory supports the count transition from 185 `Oracle complete`
  and 18 `Audit covered` rows to 194 `Oracle complete` and 9 `Audit covered`
  rows.
- The matrix assertion is non-vacuous because it checks the exact promoted set
  plus excluded adjacent custom and collection rows.
