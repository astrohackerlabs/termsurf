# Experiment 32: macOS Icon Screen Color Parser Oracle

## Description

CFG-217 still has 31 parser rows that are only `Audit covered`. The last
remaining `custom inline` row is canonical `macos-icon-screen-color`, whose
dispatch path is
`set_optional_value_field(value, default.macos_icon_screen_color, parse_color_list_field)`.

Pinned Ghostty defines `macos-icon-screen-color` as `?ColorList = null`.
`ColorList.parseCLI` accepts up to 64 comma-separated colors. Each non-empty
comma token is trimmed of spaces and tabs before passing to `Color.parseCLI`;
empty comma tokens are skipped; every parse resets the list; missing and direct
empty child-parser inputs are `ValueRequired`; an all-empty list, all-whitespace
token, invalid color, or more than 64 colors is `InvalidValue`. Through the
optional field wrapper, raw empty config values reset the option to `null`.

Roastty already has lower-level `ColorList` tests and a broad macOS app icon
config test. This experiment will add a focused CFG-217 oracle named for
inventory promotion, keep the existing lower-level and broad tests in the
verification set, and promote only canonical `macos-icon-screen-color`.

This experiment is limited to parser, formatter, reset/default, diagnostics,
CLI, and clone semantics for the config row. Runtime icon generation and
custom-style validation remain separate parity facets.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `macos_icon_screen_color_config_parser_family_oracle` test
    covering:
    - default/unset formatting as a blank entry;
    - named colors and hex colors;
    - comma-separated multiple colors;
    - spaces and tabs trimmed around color tokens;
    - leading, trailing, and doubled comma empty tokens skipped;
    - each parse resets the color list instead of appending;
    - the 64-color maximum and 65th-color rejection;
    - raw empty option values resetting to `None`;
    - direct missing values are `ValueRequired`;
    - all-empty comma lists, whitespace-only values, bad colors, and malformed
      colors are `InvalidValue`;
    - `load_str` diagnostics preserve earlier valid values while reporting
      invalid later lines;
    - CLI argument parsing reaches the same helper;
    - cloned configs retain parsed color lists.
  - Keep the existing `color_list_parse_cli_parses_comma_separated_colors` and
    broad macOS app icon config tests in the verification set.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark only the canonical `macos-icon-screen-color` parser row as
    `Oracle complete` when the macOS icon screen color oracle test is present.
  - Add macOS icon screen color oracle detection to CFG-217 ownership so the
    generated matrix records `Experiment 32` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 173 `Oracle complete`, 30
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 173 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting macOS icon screen color parser semantics after
    the result is proven.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml macos_icon_screen_color_config_parser_family_oracle
```

- Existing lower-level and broad macOS icon tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml color_list_parse_cli_parses_comma_separated_colors
cargo test --manifest-path roastty/Cargo.toml macos_app_icon_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml color_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=173`;
  - `audit_covered=30`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 173 rows are `Oracle complete`;
  - the `macos-icon-screen-color` row is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 32`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  passes.
- No `__pycache__` or other `py_compile` artifacts remain in the issue folder.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml macos_icon_screen_color_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml color_list_parse_cli_parses_comma_separated_colors
cargo test --manifest-path roastty/Cargo.toml macos_app_icon_config_parse_format_reset_and_diagnose
cargo test --manifest-path roastty/Cargo.toml color_config_parser_family_oracle
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
python3 - <<'PY'
from pathlib import Path

matrix_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-'):
        matrix_rows.append([cell.strip() for cell in line.strip('|').split('|')])
cfg217 = next(row for row in matrix_rows if row[0] == 'CFG-217')
assert cfg217[4] == 'Gap', cfg217
assert 'config-parser-inventory.md' in cfg217[6], cfg217
assert cfg217[11] == 'Experiment 32', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
screen_color = [row for row in parser_rows if row[1] == '`macos-icon-screen-color`']
assert len(screen_color) == 1, screen_color
assert screen_color[0][4] == 'Oracle complete', screen_color[0]
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 173
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} macos_icon_screen_color={screen_color[0][4]} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/32-macos-icon-screen-color-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial subagent review completed before implementation.

**Verdict:** Approved.

No required findings.
