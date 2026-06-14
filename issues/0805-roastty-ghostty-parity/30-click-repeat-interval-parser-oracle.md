# Experiment 30: Click Repeat Interval Parser Oracle

## Description

CFG-217 still has 33 parser rows that are only `Audit covered`. The next bounded
row is canonical `click-repeat-interval`, whose dispatch path is
`set_value_field(value, default.click_repeat_interval, parse_u32_field)`.

Pinned Ghostty defines `click-repeat-interval` as a `u32` with default `0`.
Ghostty's generic config field parser uses `std.fmt.parseInt(u32, input, 10)`
for `u32` fields. The parser boundary is distinct from finalization:

- direct missing values are `ValueRequired`;
- raw empty config values reset to the default `0`;
- valid base-10 `u32` values are accepted;
- base prefixes are rejected because this field uses base 10, not base 0;
- leading `+`, `-0`, and interior underscores follow Zig integer parsing;
- negative nonzero values, malformed values, and overflow are `InvalidValue`;
- formatting emits the decimal integer;
- finalization later resolves `0` to an OS/default click interval, but that is
  not part of the parser row.

Roastty already has broad mouse behavior tests and a finalization test. This
experiment will add a focused parser-helper oracle named for CFG-217 inventory
promotion, keep the existing mouse behavior/finalization tests in the
verification set, and promote only canonical `click-repeat-interval`.

This experiment is limited to parser, formatter, reset/default, diagnostics,
CLI, clone, and proving that parser-level `0` remains `0` before finalization.
Runtime click behavior and platform-specific OS click interval lookup remain
separate parity facets.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused `click_repeat_interval_config_parser_family_oracle` test
    covering:
    - base-10 `u32` parsing, including `0`, leading `+`, `-0`, interior
      underscores, and `u32::MAX`;
    - raw empty option values reset to the default `0`;
    - direct missing values are `ValueRequired`;
    - invalid decimals, non-base-10 prefixes, negative nonzero values, malformed
      separators, bare signs, leading/trailing whitespace, and overflow are
      `InvalidValue`;
    - formatter output emits decimal integers;
    - `load_str` diagnostics preserve earlier valid values while reporting
      invalid later lines;
    - CLI argument parsing reaches the same helper;
    - cloned configs retain parsed values;
    - parser-level `0` remains `0` before `finalize`, while existing
      finalization tests prove later `0` resolution separately.
  - Keep existing broad mouse behavior and finalization tests in the
    verification set.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark only the canonical `click-repeat-interval` parser row as
    `Oracle complete` when the click-repeat oracle test is present.
  - Add click-repeat oracle detection to CFG-217 ownership so the generated
    matrix records `Experiment 30` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 171 `Oracle complete`, 32
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 171 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting click repeat interval parser semantics after the
    result is proven.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml click_repeat_interval_config_parser_family_oracle
```

- Existing broad mouse behavior and finalization tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_config_routes_and_formats
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
cargo test --manifest-path roastty/Cargo.toml integer_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=171`;
  - `audit_covered=32`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 171 rows are `Oracle complete`;
  - the `click-repeat-interval` row is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 30`;
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
cargo test --manifest-path roastty/Cargo.toml click_repeat_interval_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_config_routes_and_formats
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
cargo test --manifest-path roastty/Cargo.toml integer_config_parser_family_oracle
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
assert cfg217[11] == 'Experiment 30', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
click_repeat = [row for row in parser_rows if row[1] == '`click-repeat-interval`']
assert len(click_repeat) == 1, click_repeat
assert click_repeat[0][4] == 'Oracle complete', click_repeat[0]
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 171
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} click_repeat={click_repeat[0][4]} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/30-click-repeat-interval-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Adversarial subagent `019ec411-9f46-72e3-ad2a-51fb77eb300d` reviewed the initial
design and returned `CHANGES REQUIRED`.

- **Required:** The planned oracle did not explicitly prove leading/trailing
  whitespace rejection for the `std.fmt.parseInt(u32, input, 10)` boundary.
- **Fix:** Added leading/trailing whitespace-padded values to the planned
  `InvalidValue` coverage.
- **Optional:** The generator changes under-specified the CFG-217 owner update
  needed for the verification assertion that owner becomes `Experiment 30`.
- **Fix:** Added an explicit generator change requiring click-repeat oracle
  detection to drive CFG-217 ownership.

The same adversarial subagent re-reviewed only those fixes and returned
`VERDICT: APPROVED`, confirming the Required and Optional findings were resolved
and that no new Required findings were introduced.

## Result

**Result:** Pass

Implemented the click repeat interval parser oracle and promoted the canonical
`click-repeat-interval` row to `Oracle complete`.

Changes made:

- `roastty/src/config/mod.rs`
  - Added `click_repeat_interval_config_parser_family_oracle`.
  - Covered base-10 `u32` parsing, raw-empty reset, missing value errors,
    invalid values, whitespace rejection, diagnostics, CLI parsing, formatting,
    clone behavior, and parser/finalization boundary.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Added the click-repeat oracle marker and Experiment 30 ownership.
  - Promotes only canonical `click-repeat-interval`.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerated with 171 `Oracle complete`, 32 `Audit covered`, and 0 `Gap`
    rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerated CFG-217 with Experiment 30 as owner and the updated parser
    counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Added the click repeat interval learning and updated this experiment to
    `Pass`.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml click_repeat_interval_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_config_routes_and_formats
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
cargo test --manifest-path roastty/Cargo.toml integer_config_parser_family_oracle
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
assert cfg217[11] == 'Experiment 30', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
click_repeat = [row for row in parser_rows if row[1] == '`click-repeat-interval`']
assert len(click_repeat) == 1, click_repeat
assert click_repeat[0][4] == 'Oracle complete', click_repeat[0]
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 171
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} click_repeat={click_repeat[0][4]} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/30-click-repeat-interval-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Observed verification output:

- `click_repeat_interval_config_parser_family_oracle`: passed.
- `mouse_behavior_config_routes_and_formats`: passed.
- `mouse_behavior_finalize_resolves_and_clamps`: passed.
- `integer_config_parser_family_oracle`: passed.
- Parser generator:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=171`;
  - `audit_covered=32`;
  - `gap=0`.
- Matrix assertion:
  - `parser_rows=203`;
  - `click_repeat=Oracle complete`;
  - `cfg217=Gap`.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`:
  passed, and the generated `__pycache__` directory was removed.
- `git diff --check`: passed.

## Conclusion

The canonical `click-repeat-interval` parser row now has a durable Tier 1
oracle. Roastty matches pinned Ghostty's parser boundary for base-10 `u32`
values, required missing values, raw-empty reset to `0`, invalid whitespace and
non-decimal forms, diagnostics, CLI, formatting, clone behavior, and the
parser/finalization distinction.

CFG-217 remains `Gap` because 32 parser rows are still only `Audit covered`. The
next experiment should continue with another bounded parser row or family from
those remaining rows.

## Completion Review

Adversarial subagent `019ec411-9f46-72e3-ad2a-51fb77eb300d` reviewed the
completed experiment and returned `VERDICT: APPROVED` with no findings.
