# Experiment 23: Window Padding Parser Oracle

## Description

CFG-217 still has 127 parser rows that are only `Audit covered`. The next
bounded family is the 2-row window-padding parser family:

- `window-padding-x`;
- `window-padding-y`.

Pinned Ghostty implements both options with `WindowPadding.parseCLI`. The direct
parser accepts one base-10 `u32` applied to both sides, or two comma-separated
base-10 `u32` values. It trims only spaces and tabs around each value, maps a
missing value to `ValueRequired`, and maps empty strings, bad numbers, overflow,
or malformed pairs to `InvalidValue`. At the config-option boundary, a raw empty
value resets the field to its default.

Roastty already has `WindowPadding::parse_cli` and option-level tests. This
experiment will add one focused family oracle for the two window-padding rows,
then promote them to `Oracle complete`.

This experiment is limited to parser, formatter, reset, and diagnostic semantics
for `window-padding-x/y`. `window-padding-balance` and `window-padding-color`
are enum rows and remain separate parser-family work.

CFG-217 must remain `Gap` because other parser families are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused window-padding parser family test covering:
    - one-value padding, pair padding, and space/tab trimming;
    - both `window-padding-x` and `window-padding-y` dispatch paths;
    - formatter output as one value when sides match, and `left,right` when
      sides differ;
    - raw empty field reset to default `2`;
    - missing values as `ValueRequired`;
    - invalid values: empty direct parser input, non-numeric values, bad pair
      sides, missing pair sides, too many comma-separated fields, overflow, edge
      underscores, and negative nonzero numbers;
    - Zig `parseInt(u32, _, 10)` compatibility cases already used elsewhere in
      the scalar parser work: interior underscores, leading `+`, and `-0`;
    - load-string diagnostics preserving valid earlier values while reporting
      invalid later lines.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark window-padding parser rows as `Oracle complete` when the window-padding
    family oracle test is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 78 `Oracle complete`, 125
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 78 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting window-padding parser semantics after the result
    is proven.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml window_padding_config_parser_family_oracle
```

- Existing window-padding option-level regression test still passes:

```bash
cargo test --manifest-path roastty/Cargo.toml window_padding_config_parse_format_reset_and_diagnose
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=78`;
  - `audit_covered=125`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 78 rows are `Oracle complete`;
  - both window-padding rows are `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 23`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `git diff --check` passes.

## Design Review

Fresh-context adversarial design review approved the experiment plan with no
findings.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml window_padding_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml window_padding_config_parse_format_reset_and_diagnose
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
assert cfg217[11] == 'Experiment 23', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
window_padding_rows = [row for row in parser_rows if row[3] == 'window padding']
assert len(window_padding_rows) == 2, len(window_padding_rows)
assert all(row[4] == 'Oracle complete' for row in window_padding_rows)
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 78
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} window_padding_oracle={len(window_padding_rows)} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/23-window-padding-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```
