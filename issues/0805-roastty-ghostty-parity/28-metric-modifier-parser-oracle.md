# Experiment 28: Metric Modifier Parser Oracle

## Description

CFG-217 still has 47 parser rows that are only `Audit covered`. The next bounded
shared parser helper is `parse_metric_modifier`, which covers 13 canonical
adjustment rows:

- `adjust-box-thickness`;
- `adjust-cell-height`;
- `adjust-cell-width`;
- `adjust-cursor-height`;
- `adjust-cursor-thickness`;
- `adjust-font-baseline`;
- `adjust-icon-height`;
- `adjust-overline-position`;
- `adjust-overline-thickness`;
- `adjust-strikethrough-position`;
- `adjust-strikethrough-thickness`;
- `adjust-underline-position`;
- `adjust-underline-thickness`.

Pinned Ghostty models these as optional `fontpkg.Metrics.Modifier` fields. The
parser is not a generic Rust numeric parser: `Modifier.parse` in
`vendor/ghostty/src/font/Metrics.zig` accepts either
`std.fmt.parseInt(i32, input, 10)` for absolute values or
`std.fmt.parseFloat(f64, percent_body)` for trailing-`%` percentage values.
Percentages store a multiplier: `25%` becomes `1.25`, `-50%` becomes `0.5`, and
values at or below `-100%` clamp to `0`. Missing values are `ValueRequired`;
invalid values are `InvalidValue`; raw empty option values reset the optional
field to `null`.

Roastty already has lower-level `MetricModifier::parse` tests and a broad config
test for the adjustment rows. This experiment will add a focused parser-helper
oracle named for CFG-217 inventory promotion, extend the lower-level parser if
needed so it matches Ghostty/Zig numeric syntax, keep the lower-level metric
tests in the verification set, and promote only rows whose parser path uses
`parse_metric_modifier`.

This experiment is limited to parser, formatter, reset, CLI, diagnostics, and
clone semantics for metric modifier config rows. Runtime font metric application
remains a separate parity facet.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused metric modifier parser oracle test covering:
    - all 13 canonical adjustment keys route through the parser;
    - signed absolute integer values using Ghostty's base-10 `i32` semantics,
      including range boundaries, overflow/range failures, accepted interior
      underscores, and rejected leading, trailing, doubled, and sign-adjacent
      separators;
    - percentage values using Ghostty's `std.fmt.parseFloat(f64, ...)`
      semantics, including positive percentages, negative percentages, and
      clamp-at-or-below-`-100%`;
    - accepted percentage float syntax that Rust's built-in parser does not
      fully cover: signed and case-mixed `nan`, `inf`, and `infinity`, overflow
      to infinity, interior underscores, and hexadecimal float syntax before
      `%`;
    - rejected malformed percentage float syntax: empty bodies, bare signs,
      malformed separators, malformed hexadecimal floats, and Zig-rejected C
      payload-NaN syntax;
    - formatter output for absolute and percentage values;
    - raw empty option values reset to `None`;
    - missing values are `ValueRequired`;
    - invalid decimals in absolute mode, non-base-10 absolute prefixes,
      malformed percentages, and empty direct parser values are `InvalidValue`;
    - `load_str` diagnostics preserve valid earlier values while reporting
      invalid later lines;
    - CLI argument parsing reaches the same helper;
    - cloned configs retain parsed metric modifiers.
  - Keep existing lower-level `MetricModifier::parse` tests in the verification
    set.
- `roastty/src/font/metrics.rs`
  - If the existing lower-level parser does not already match pinned Ghostty,
    update it so absolute values follow Zig base-10 `i32` parsing and percentage
    values follow Zig `f64` float parsing, including special values, hexadecimal
    floats, separator rules, overflow behavior, clamp behavior, and
    formatter-visible special values.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Mark parser rows whose parser path includes `parse_metric_modifier` as
    `Oracle complete` when the metric modifier oracle test is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 169 `Oracle complete`, 34
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 169 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting metric modifier parser semantics after the result
    is proven.

## Verification

Pass criteria:

- Focused Roastty tests pass:

```bash
cargo test --manifest-path roastty/Cargo.toml metric_modifier_config_parser_family_oracle
```

- The focused oracle proves Ghostty/Zig metric numeric compatibility, not just
  ordinary Rust numeric parsing:
  - absolute values use base-10 `i32` parsing, with signed values, boundary
    values, overflow/range failures, accepted interior underscores, rejected
    malformed separators, and rejected non-base-10 prefixes;
  - percentage bodies use Zig `f64` float parsing, with signed and case-mixed
    `nan`, `inf`, and `infinity`, overflow to infinity, interior underscores,
    hexadecimal floats before `%`, malformed separator rejection, malformed hex
    rejection, and payload-NaN rejection;
  - percentage formatter output is checked for accepted special values,
    including `nan%`, infinity output, and clamp-derived `-100%`.
- Existing metric adjustment and lower-level metric parser tests still pass:

```bash
cargo test --manifest-path roastty/Cargo.toml font_metric_adjust_config_parse_format_reset_load_and_clone
cargo test --manifest-path roastty/Cargo.toml modifier_parse
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=169`;
  - `audit_covered=34`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 169 rows are `Oracle complete`;
  - all 13 rows with parser path `parse_metric_modifier` are `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 28`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `git diff --check` passes.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml metric_modifier_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml font_metric_adjust_config_parse_format_reset_load_and_clone
cargo test --manifest-path roastty/Cargo.toml modifier_parse
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
assert cfg217[11] == 'Experiment 28', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
metric_rows = [row for row in parser_rows if 'parse_metric_modifier' in row[2]]
assert len(metric_rows) == 13, len(metric_rows)
assert all(row[4] == 'Oracle complete' for row in metric_rows)
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 169
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} metric_modifier_oracle={len(metric_rows)} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/28-metric-modifier-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Adversarial subagent `019ec3f5-e8d4-7de1-ac20-cd67194a1679` reviewed the initial
design and returned `CHANGES REQUIRED`.

- **Required:** The numeric grammar oracle was too narrow to prove pinned
  Ghostty metric modifier parsing because Ghostty uses
  `std.fmt.parseInt(i32, input, 10)` for absolute values and
  `std.fmt.parseFloat(f64, percent_body)` for percentage bodies.
- **Fix:** Expanded the design and pass criteria to require base-10 `i32`
  boundaries, overflow, separator acceptance/rejection, Zig `f64` percentage
  syntax, special float values, hexadecimal floats, malformed percentage
  rejection, payload-NaN rejection, and formatter behavior for accepted special
  percentage values.

The same adversarial subagent re-reviewed only the fix and returned
`VERDICT: APPROVED`, confirming that the prior finding was resolved and that no
new Required findings were introduced.

## Result

**Result:** Pass

Implemented the metric modifier parser oracle and promoted all 13
`parse_metric_modifier` rows to `Oracle complete`.

Changes made:

- `roastty/src/font/metrics.rs`
  - Replaced Rust built-in numeric parsing in `Modifier::parse` with
    Zig-compatible parsing:
    - absolute values use base-10 signed `i32` semantics;
    - percentage bodies use Zig-compatible `f64` parsing;
    - hexadecimal floats, special values, overflow, and interior underscores are
      accepted where Zig accepts them;
    - malformed separators, malformed hexadecimal floats, invalid base prefixes
      in absolute mode, over-wide integers, and payload NaNs are rejected.
  - Extended lower-level metric parser tests for the Zig-specific numeric cases.
- `roastty/src/config/mod.rs`
  - Added `metric_modifier_config_parser_family_oracle`.
  - Covered all 13 adjustment keys, formatter output, empty reset, missing
    values, invalid values, diagnostics, CLI parsing, and clone behavior.
  - Verified Zig formatter parity for floating precision artifacts such as
    `0x1p4%` round-tripping as `15.999999999999993%`.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Added the metric modifier oracle marker and Experiment 28 ownership.
  - Promotes only rows whose parser path includes `parse_metric_modifier`.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerated with 169 `Oracle complete`, 34 `Audit covered`, and 0 `Gap`
    rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerated CFG-217 with Experiment 28 as owner and the updated parser
    counts.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Added the metric modifier learning and updated this experiment to `Pass`.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml metric_modifier_config_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml font_metric_adjust_config_parse_format_reset_load_and_clone
cargo test --manifest-path roastty/Cargo.toml modifier_parse
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
assert cfg217[11] == 'Experiment 28', cfg217

parser_rows = []
for line in Path('issues/0805-roastty-ghostty-parity/config-parser-inventory.md').read_text().splitlines():
    if line.startswith('| PARSE-'):
        parser_rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert len(parser_rows) == 203, len(parser_rows)
metric_rows = [row for row in parser_rows if 'parse_metric_modifier' in row[2]]
assert len(metric_rows) == 13, len(metric_rows)
assert all(row[4] == 'Oracle complete' for row in metric_rows)
assert sum(row[4] == 'Oracle complete' for row in parser_rows) == 169
assert all(row[4] != 'Gap' for row in parser_rows)
print(f'parser_rows={len(parser_rows)} metric_modifier_oracle={len(metric_rows)} cfg217={cfg217[4]}')
PY
cargo fmt --manifest-path roastty/Cargo.toml
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/28-metric-modifier-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Observed verification output:

- `metric_modifier_config_parser_family_oracle`: passed.
- `font_metric_adjust_config_parse_format_reset_load_and_clone`: passed.
- `modifier_parse`: passed.
- Parser generator:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=169`;
  - `audit_covered=34`;
  - `gap=0`.
- Matrix assertion:
  - `parser_rows=203`;
  - `metric_modifier_oracle=13`;
  - `cfg217=Gap`.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`:
  passed.
- `git diff --check`: passed.

## Conclusion

The metric modifier parser family now has a durable Tier 1 oracle. Roastty now
matches pinned Ghostty's metric modifier parser boundary for all 13 canonical
adjustment options, including Zig-specific integer and float syntax and
formatter-visible floating precision behavior.

CFG-217 remains `Gap` because 34 parser rows are still only `Audit covered`. The
next experiment should continue with another bounded parser family from those
remaining rows.

## Completion Review

Adversarial subagent `019ec3f5-e8d4-7de1-ac20-cd67194a1679` reviewed the
completed experiment and returned `VERDICT: APPROVED`.

- **Optional:** The recorded verification omitted the
  `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  check that was run during implementation.
- **Fix:** Added the `py_compile` command and observed pass result to this
  experiment file.
