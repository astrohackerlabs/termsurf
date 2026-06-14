# Experiment 27: Color Parser Oracle

## Description

CFG-217 still has 63 parser rows that are only `Audit covered`. The next bounded
shared parser family is the 16-row color family:

- required `Color` rows such as `background` and `foreground`;
- optional `Color` rows such as `macos-icon-ghost-color` and
  `window-titlebar-background`;
- required and optional `TerminalColor` rows such as `search-background`,
  `cursor-color`, and `selection-foreground`;
- the optional `BoldColor` row `bold-color`.

Pinned Ghostty's color parser accepts named colors and hex forms through the
shared color parser, while `TerminalColor` adds the exact `cell-foreground` and
`cell-background` sentinels and `BoldColor` adds the exact `bright` keyword.
Missing values are `ValueRequired`; invalid values are `InvalidValue`; raw empty
option values reset fields through the surrounding required or optional dispatch
helper.

Roastty already has focused lower-level color parser tests for named colors, hex
forms, terminal sentinels, bold-color `bright`, list parsing, and formatter
output. This experiment will add the missing config-option boundary oracle for
the 16 direct color rows, keep the existing color parser tests as verification,
and promote the color family to `Oracle complete`.

This experiment is limited to parser, formatter, reset, and diagnostic semantics
for direct color config rows. Runtime rendering effects of the colors remain
separate parity facets.

CFG-217 must remain `Gap` because other parser families are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused color parser family oracle test covering:
    - required `Color` dispatch accepts named colors and hex values;
    - optional `Color` dispatch accepts values and raw-empty resets to default;
    - required and optional `TerminalColor` dispatch accepts normal colors plus
      `cell-foreground` and `cell-background` sentinels;
    - optional `BoldColor` dispatch accepts `bright` and normal colors;
    - missing values are `ValueRequired`;
    - raw empty required and optional values reset to defaults;
    - plain `Color` rows reject terminal sentinels;
    - invalid names, malformed hex, wrong sentinel keywords, and invalid
      whitespace-padded sentinel keywords are rejected as `InvalidValue`;
    - formatter output is canonical for representative color, terminal color,
      and bold-color fields;
    - load-string diagnostics preserve valid earlier values while reporting
      invalid later color lines.
  - Keep the existing lower-level color parser and formatter tests in the
    verification set.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark color parser rows as `Oracle complete` when the color family oracle
    test is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 156 `Oracle complete`, 47
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 156 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting color parser semantics after the result is
    proven.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml color_config_parser_family_oracle
```

- Existing color parser and formatter regression tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml color_parse_cli
cargo test --manifest-path roastty/Cargo.toml terminal_color_parse_cli
cargo test --manifest-path roastty/Cargo.toml bold_color_parse_cli
cargo test --manifest-path roastty/Cargo.toml color_format_entry
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=156`;
  - `audit_covered=47`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 156 rows are `Oracle complete`;
  - all 16 color rows are `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 27`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml color_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml color_parse_cli
cargo test --manifest-path roastty/Cargo.toml terminal_color_parse_cli
cargo test --manifest-path roastty/Cargo.toml bold_color_parse_cli
cargo test --manifest-path roastty/Cargo.toml color_format_entry
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
assert cfg217[11] == 'Experiment 27', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
color_rows = [row for row in parser_rows if row[3] == 'color']
assert len(color_rows) == 16, len(color_rows)
assert all(row[4] == 'Oracle complete' for row in color_rows)
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 156
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} color_oracle={len(color_rows)} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/27-color-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial design review found one required issue:

- The draft wrongly implied required `TerminalColor` rows reject
  `cell-foreground` and `cell-background`; pinned Ghostty accepts those
  sentinels for any `TerminalColor`, including required rows such as
  `search-foreground` and `search-background`.

Fix:

- Updated the design to require both required and optional `TerminalColor`
  dispatch to accept normal colors plus `cell-foreground` and `cell-background`;
  sentinel rejection is now scoped to plain `Color` rows and invalid sentinel
  spellings.

Re-review approved the fixed design:

```text
VERDICT: APPROVED

No Required findings remain.
```
