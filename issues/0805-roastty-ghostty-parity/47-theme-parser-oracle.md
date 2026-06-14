# Experiment 47: Theme Parser Oracle

## Description

CFG-217 still has 3 parser rows that are only `Audit covered`. One of those is
`theme`, whose parser accepts either one theme name for both light and dark
modes or a light/dark pair.

Pinned Ghostty's `Theme.parseCLI` requires a non-empty value. A plain value with
no comma, equals sign, or colon is trimmed using CLI whitespace and assigned to
both `light` and `dark`. On macOS and other non-Windows builds, any value
containing a comma, equals sign, or colon routes to
`parseAutoStruct(Theme, ...)`; that pair parser requires both `light` and
`dark`, trims whitespace around keys and values, accepts quoted values, lets
later duplicate fields win, accepts empty values after a colon, and rejects
unknown keys, missing colons, missing required fields, and malformed quoted
values. Pinned Ghostty has a Windows-only exception for `C:\...`-style
drive-letter colons; that exception is outside this macOS app parity oracle and
should be tracked separately if Roastty grows Windows config parity coverage.
Formatting emits the single name when light and dark match, otherwise
`light:{light},dark:{dark}`.

Roastty already has focused tests for direct theme parsing, auto-struct parsing,
formatting, and config routing. This experiment will consolidate that coverage
under the explicit CFG-217 oracle name, extend it where needed for file/CLI
diagnostics and clone semantics, wire the parser inventory to recognize the
oracle, and promote only the canonical `theme` row.

CFG-217 must remain `Gap` because `config-default-files` and `keybind` will
still be audit-only after this experiment.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing theme parser/config tests as
    `theme_config_parser_family_oracle`.
  - Extend the oracle if needed to cover:
    - direct missing and empty values returning `ValueRequired`;
    - single-name parsing with ASCII space/tab trimming;
    - comma, equals, and colon values routing to the pair parser on macOS /
      non-Windows builds;
    - `light`/`dark` pair parsing with whitespace around keys and values;
    - quoted pair values with embedded commas;
    - duplicate fields with later values winning;
    - empty values after a colon;
    - invalid unknown keys, missing colons, missing required fields, and bad
      quoted values;
    - config empty reset to `None`;
    - config missing value diagnostics;
    - config-file diagnostics;
    - CLI parsing;
    - formatting and clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `theme_config_parser_family_oracle`.
  - Mark only canonical `theme` as `Oracle complete` when the oracle test is
    present.
  - Add the oracle to CFG-217 ownership so the generated matrix records
    `Experiment 47` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 201 `Oracle complete`, 2
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 201 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting theme parser semantics after the result is
    proven.

## Verification

Pass criteria:

- Focused Roastty theme oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml theme_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=201`;
  - `audit_covered=2`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 201 rows are `Oracle complete`;
  - `theme` is `Oracle complete`;
  - the remaining `Audit covered` set is exactly `config-default-files` and
    `keybind`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 47`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run if any Rust file is
  edited.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  passes, and any generated `__pycache__` is removed.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml theme_config_parser_family_oracle
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
python3 - <<'PY'
from pathlib import Path

rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        cells = [cell.strip() for cell in line.strip('|').split('|')]
        rows.append(cells)

by_option = {row[1].strip('`'): row for row in rows}
audit = {row[1].strip('`') for row in rows if row[4] == 'Audit covered'}

assert len(rows) == 203, len(rows)
assert sum(row[4] == 'Oracle complete' for row in rows) == 201
assert by_option['theme'][4] == 'Oracle complete'
assert audit == {'config-default-files', 'keybind'}, audit
assert not any(row[4] == 'Gap' for row in rows)

matrix = Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text()
cfg217 = next(line for line in matrix.splitlines() if line.startswith('| CFG-217 '))
cfg217_cells = [cell.strip() for cell in cfg217.strip('|').split('|')]
assert cfg217_cells[4] == 'Gap'
assert cfg217_cells[11] == 'Experiment 47'
assert 'config-parser-inventory.md' in cfg217_cells[6]
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
test -z "$(find issues/0805-roastty-ghostty-parity -name __pycache__ -prune -print)"
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/47-theme-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: Changes required.

Finding:

- Required: the design described colon routing too broadly and missed pinned
  Ghostty's Windows-only drive-letter exception for `C:\...` paths. Fixed by
  explicitly scoping this parser oracle to macOS/non-Windows behavior for Issue
  805 and noting that Windows config parity should track the exception
  separately.

Re-review verdict: Approved.

The re-review confirmed the macOS/non-Windows scope matches pinned Ghostty's
platform-specific parser branch and Issue 805's macOS app parity target.
