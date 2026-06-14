# Experiment 44: Font Variation Parser Oracle

## Description

CFG-217 still has 10 parser rows that are only `Audit covered`. Four of those
rows are the same pinned Ghostty `RepeatableFontVariation.parseCLI` surface:
`font-variation`, `font-variation-bold`, `font-variation-italic`, and
`font-variation-bold-italic`.

Pinned Ghostty's `RepeatableFontVariation.parseCLI` requires a value, requires
the first `=` separator, trims ASCII space and tab around both the axis id and
value, requires an axis id of exactly four bytes, parses the value with Zig
`std.fmt.parseFloat(f64, ...)`, and appends one variation. That value parser
must cover the same relevant Zig float classes already proven for direct float
config fields: decimal values, underscores, hexadecimal floats, special NaN/Inf
spellings, overflow, underflow, and invalid syntax. Formatting emits one blank
entry for an empty list or one `id=value` entry per stored variation. Roastty's
config dispatch wraps this helper so an exactly empty config value resets the
repeatable list before direct parser dispatch.

Roastty already has focused coverage for direct `RepeatableFontVariation`
parsing, formatting, reset, config-file loading, CLI append behavior, and clone
semantics. This experiment will make that coverage an explicit CFG-217 oracle,
extend it where needed, wire the parser inventory to recognize the oracle, and
promote only the four canonical `font-variation*` rows.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing font-variation regression as
    `font_variation_config_parser_family_oracle`.
  - Extend the oracle if needed to cover:
    - direct missing value returning `ValueRequired`;
    - direct empty value, missing `=`, short/long axis ids, and invalid numeric
      values returning `InvalidValue`;
    - first-equals splitting and ASCII space/tab trimming around the axis id and
      value;
    - Zig-compatible `f64` value syntax, including decimal values, underscores,
      hexadecimal floats, special NaN/Inf spellings, overflow, underflow, and
      invalid syntax;
    - fixing the parser before promotion if it currently uses Rust-only float
      syntax for variation values;
    - append order and formatter output;
    - set-but-empty config reset for repeatable fields;
    - config dispatch for all four `font-variation*` fields;
    - config-file diagnostics;
    - CLI parsing and clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `font_variation_config_parser_family_oracle`.
  - Mark only canonical `font-variation`, `font-variation-bold`,
    `font-variation-italic`, and `font-variation-bold-italic` as
    `Oracle complete` when the oracle test is present.
  - Add the oracle to CFG-217 ownership so the generated matrix records
    `Experiment 44` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 197 `Oracle complete`, 6
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 197 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `RepeatableFontVariation` parser semantics after
    the result is proven.

## Verification

Pass criteria:

- Focused Roastty font-variation oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml font_variation_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=197`;
  - `audit_covered=6`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 197 rows are `Oracle complete`;
  - the four canonical `font-variation*` rows are `Oracle complete`;
  - the remaining `Audit covered` set is exactly the pre-existing audit-only set
    minus those four rows;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 44`;
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
cargo test --manifest-path roastty/Cargo.toml font_variation_config_parser_family_oracle
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
        rows.append([cell.strip() for cell in line.strip('|').split('|')])

assert len(rows) == 203, len(rows)
assert sum(row[4] == 'Oracle complete' for row in rows) == 197
assert sum(row[4] == 'Audit covered' for row in rows) == 6
assert not [row for row in rows if row[4] == 'Gap']
expected_audit = {
    '`clipboard-codepoint-map`',
    '`config-default-files`',
    '`font-codepoint-map`',
    '`key-remap`',
    '`keybind`',
    '`theme`',
}
actual_audit = {row[1] for row in rows if row[4] == 'Audit covered'}
assert actual_audit == expected_audit, sorted(actual_audit ^ expected_audit)
for option in {
    '`font-variation`',
    '`font-variation-bold`',
    '`font-variation-italic`',
    '`font-variation-bold-italic`',
}:
    row = next(row for row in rows if row[1] == option)
    assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 44', cfg217
assert '197 parser rows Oracle complete' in cfg217[12], cfg217
print('font_variation_rows=4 oracle_complete=197 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/44-font-variation-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Reviewed by an adversarial Codex subagent with fresh context.

**Initial verdict:** Changes required.

Required finding:

- The design did not require proving the full Zig `std.fmt.parseFloat(f64, ...)`
  value space for variation values before promoting the four `font-variation*`
  rows.

Fix:

- Added explicit Zig-compatible `f64` coverage requirements for decimal values,
  underscores, hexadecimal floats, NaN/Inf spellings, overflow, underflow, and
  invalid syntax, plus a requirement to fix any Rust-only parser divergence
  before promotion.

**Re-review verdict:** Approved.

Findings after fix: none.

## Result

**Result:** Pass.

Roastty now exposes a focused `font_variation_config_parser_family_oracle` that
covers direct `RepeatableFontVariation::parse_cli`, formatter output, set-empty
reset dispatch, config-file diagnostics, CLI parsing, and clone semantics for
all four canonical `font-variation*` fields. The implementation now parses
variation values with the shared Zig-compatible `f64` parser instead of
Rust-only `str::parse`, matching pinned Ghostty's `std.fmt.parseFloat(f64, ...)`
value surface.

The oracle proves required values, missing separator errors, short and long axis
id rejection, invalid numeric rejection, first-equals splitting, ASCII space/tab
trimming, append order, decimal values, underscores, hexadecimal floats, NaN/Inf
spellings, overflow, underflow, empty config resets, file/CLI parsing,
formatting, diagnostics, and clone semantics.

The parser inventory generator now detects that oracle and promotes only the
four canonical `font-variation*` rows to `Oracle complete`. The regenerated
CFG-217 parser inventory reports 203 parser rows, 197 `Oracle complete`, 6
`Audit covered`, and 0 `Gap`. CFG-217 remains `Gap` because the remaining
audit-only parser rows still need their own upstream-derived oracles.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml font_variation_config_parser_family_oracle
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
        rows.append([cell.strip() for cell in line.strip('|').split('|')])

assert len(rows) == 203, len(rows)
assert sum(row[4] == 'Oracle complete' for row in rows) == 197
assert sum(row[4] == 'Audit covered' for row in rows) == 6
assert not [row for row in rows if row[4] == 'Gap']
expected_audit = {
    '`clipboard-codepoint-map`',
    '`config-default-files`',
    '`font-codepoint-map`',
    '`key-remap`',
    '`keybind`',
    '`theme`',
}
actual_audit = {row[1] for row in rows if row[4] == 'Audit covered'}
assert actual_audit == expected_audit, sorted(actual_audit ^ expected_audit)
for option in {
    '`font-variation`',
    '`font-variation-bold`',
    '`font-variation-italic`',
    '`font-variation-bold-italic`',
}:
    row = next(row for row in rows if row[1] == option)
    assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 44', cfg217
assert '197 parser rows Oracle complete' in cfg217[12], cfg217
print('font_variation_rows=4 oracle_complete=197 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/44-font-variation-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Observed key outputs:

- `test config::tests::font_variation_config_parser_family_oracle ... ok`
- `oracle_complete=197`
- `audit_covered=6`
- `gap=0`
- `font_variation_rows=4 oracle_complete=197 cfg217=Gap`

## Conclusion

The four `font-variation*` parser rows are no longer audit-only. Future parser
work can focus on the 6 remaining audit-only rows: codepoint maps,
`config-default-files`, key remap/keybind, and `theme`.

## Completion Review

Reviewed by an adversarial Codex subagent with fresh context.

**Verdict:** Approved.

Findings: none.

The reviewer independently verified the focused Rust oracle, matrix assertion,
Python compile check, `cargo fmt --check`, absence of `__pycache__`/`.pyc`
artifacts, `git diff --check`, and that the result commit had not yet been made.
