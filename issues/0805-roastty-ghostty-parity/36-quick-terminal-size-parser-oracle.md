# Experiment 36: Quick Terminal Size Parser Oracle

## Description

CFG-217 still has 27 parser rows that are only `Audit covered`. Canonical
`quick-terminal-size` is one of the remaining `custom parse_cli` rows.

Pinned Ghostty defines `quick-terminal-size` as `QuickTerminalSize = .{}`.
`QuickTerminalSize.parseCLI` requires a value, splits on a comma, trims CLI
whitespace around each part, requires a primary size, accepts an optional
secondary size, and rejects a third part. Each size must end with `px` or `%`.
Pixel values parse as Zig-compatible base-10 `u32`, including accepted interior
underscores, leading `+`, and `-0`; percentage values parse as Zig `f32` and
must be non-negative. The formatter emits no entry when primary is unset and
otherwise writes `primary[,secondary]`. Calculation uses the same defaults and
position-dependent dimension mapping as upstream.

Roastty already has lower-level parser/calculate and config-routing tests, but
percentage values currently use Rust `f32::parse` rather than the shared
Zig-compatible float grammar. This experiment will add a focused CFG-217 oracle,
fix percentage parsing where needed, keep the existing lower-level/routing tests
in the verification set, and promote only canonical `quick-terminal-size`.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Update `QuickTerminalSizeValue::parse` so percentage values use Roastty's
    existing Zig-compatible `f32` parser and pixel values use the existing
    Zig-compatible base-10 `u32` helper.
  - Add a focused `quick_terminal_size_config_parser_family_oracle` test
    covering:
    - default unset formatting emits no entry;
    - primary percentage and primary pixel forms;
    - accepted Zig base-10 pixel forms `1_000px`, `+5px`, `-0px`, and
      `4294967295px`;
    - rejected pixel overflow, negative nonzero values, malformed separators,
      and non-base-10 prefixes;
    - primary plus secondary comma forms;
    - spaces/tabs trimmed around comma-separated parts;
    - Zig percentage float syntax such as `0x1p4%`, `+inf%`, and `nan%`;
    - negative percentages rejected;
    - missing values, explicit empty values, empty secondary values, too many
      arguments, missing units, invalid pixel values, malformed percentage
      floats, and malformed Zig float separators;
    - config empty value resets to default through the surrounding dispatch;
    - config-file diagnostics preserve an earlier valid value after a later
      invalid value;
    - CLI argument parsing reaches the same helper;
    - formatter output, clone semantics, and representative calculation
      behavior.
  - Keep the existing `quick_terminal_size_parse_format_and_calculate` and
    `quick_terminal_size_config_parse_format_reset_and_diagnose` tests in the
    verification set.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark only canonical `quick-terminal-size` as `Oracle complete` when the
    quick terminal size oracle test is present.
  - Add quick terminal size oracle detection to CFG-217 ownership so the
    generated matrix records `Experiment 36` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 177 `Oracle complete`, 26
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 177 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `quick-terminal-size` parser semantics after the
    result is proven.

## Verification

Pass criteria:

- Focused Roastty test passes:

```bash
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_config_parser_family_oracle
```

- Existing lower-level and routing tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_parse_format_and_calculate
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_config_parse_format_reset_and_diagnose
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=177`;
  - `audit_covered=26`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 177 rows are `Oracle complete`;
  - the `quick-terminal-size` row is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 36`;
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
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_parse_format_and_calculate
cargo test --manifest-path roastty/Cargo.toml quick_terminal_size_config_parse_format_reset_and_diagnose
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
assert cfg217[11] == 'Experiment 36', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
quick_size = [row for row in parser_rows if row[1] == '`quick-terminal-size`']
assert len(quick_size) == 1, quick_size
assert quick_size[0][4] == 'Oracle complete', quick_size[0]
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 177
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} quick_terminal_size={quick_size[0][4]} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/36-quick-terminal-size-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial subagent review completed before implementation.

**Initial verdict:** Changes required.

Required finding and fix:

- Pixel parsing was under-specified. The plan now requires using the existing
  Zig-compatible base-10 `u32` helper and explicitly covers accepted `1_000px`,
  `+5px`, `-0px`, and `4294967295px` plus overflow, negative,
  malformed-separator, and non-base-10 rejection cases.

**Re-review verdict:** Approved.

No required findings remain.
