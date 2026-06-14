# Experiment 42: Repeatable String Font Parser Oracle

## Description

CFG-217 still has 19 parser rows that are only `Audit covered`. Five of those
rows are the same pinned Ghostty `RepeatableString` parser surface:
`font-family`, `font-family-bold`, `font-family-italic`,
`font-family-bold-italic`, and `font-feature`.

Pinned Ghostty's `RepeatableString.parseCLI` requires a value, treats an exactly
empty value as a reset that clears the list, clears once before appending when
`overwrite_next` is set, appends non-empty values without tokenization or
validation, clones only the list, and compares only the list. Ghostty sets
`overwrite_next` around CLI parsing for the four `font-family*` fields so CLI
font families replace config-file values instead of appending. `font-feature`
uses the same helper but does not participate in that CLI overwrite hack, so CLI
features append to prior file values.

Roastty already has focused coverage for font-family repeat/reset/CLI overwrite
and font-feature parse/format/reset/load/CLI append/clone behavior. This
experiment will make that coverage an explicit CFG-217 oracle, extend it where
needed for the direct `RepeatableString` helper boundary, wire the parser
inventory to recognize the oracle, and promote only the five canonical
`RepeatableString` font rows.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing font-family and font-feature regressions as a
    detectable `repeatable_string_font_config_parser_family_oracle`.
  - Extend the oracle if needed to cover:
    - direct missing value returning `ValueRequired`;
    - exact empty value clearing the list;
    - non-empty values appending byte-for-byte, including spaces and
      punctuation;
    - `overwrite_next` clearing only before the next append and resetting to
      false afterward;
    - clone and equality ignoring `overwrite_next`;
    - file-loaded `font-family*` values being replaced by CLI values;
    - `font-feature` CLI values appending to file values;
    - formatter output for empty and repeated font-feature/font-family entries.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `repeatable_string_font_config_parser_family_oracle`.
  - Mark only canonical `font-family`, `font-family-bold`, `font-family-italic`,
    `font-family-bold-italic`, and `font-feature` as `Oracle complete` when the
    oracle test is present.
  - Add the oracle to CFG-217 ownership so the generated matrix records
    `Experiment 42` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 189 `Oracle complete`, 14
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 189 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `RepeatableString` font parser semantics after
    the result is proven.

## Verification

Pass criteria:

- Focused Roastty repeatable-string font oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml repeatable_string_font_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=189`;
  - `audit_covered=14`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 189 rows are `Oracle complete`;
  - the five canonical repeatable-string font rows are `Oracle complete`;
  - the remaining `Audit covered` set is exactly the pre-existing audit-only set
    minus those five rows;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 42`;
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
cargo test --manifest-path roastty/Cargo.toml repeatable_string_font_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 189
assert sum(row[4] == 'Audit covered' for row in rows) == 14
assert not [row for row in rows if row[4] == 'Gap']
expected_audit = {
    '`clipboard-codepoint-map`',
    '`config-default-files`',
    '`font-codepoint-map`',
    '`font-style`',
    '`font-style-bold`',
    '`font-style-bold-italic`',
    '`font-style-italic`',
    '`font-variation`',
    '`font-variation-bold`',
    '`font-variation-bold-italic`',
    '`font-variation-italic`',
    '`key-remap`',
    '`keybind`',
    '`theme`',
}
actual_audit = {row[1] for row in rows if row[4] == 'Audit covered'}
assert actual_audit == expected_audit, sorted(actual_audit ^ expected_audit)
for option in {
    '`font-family`',
    '`font-family-bold`',
    '`font-family-italic`',
    '`font-family-bold-italic`',
    '`font-feature`',
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
assert cfg217[11] == 'Experiment 42', cfg217
assert '189 parser rows Oracle complete' in cfg217[12], cfg217
print('repeatable_string_font_rows=5 oracle_complete=189 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/42-repeatable-string-font-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Reviewed by an adversarial Codex subagent with fresh context.

**Initial verdict:** Changes required.

Required finding:

- The matrix assertion did not prove exactly the five intended parser rows were
  promoted; counts plus target-row checks could miss an unintended status swap.

Fix:

- Added an exact expected `Audit covered` set assertion for the 14 rows that
  should remain audit-only after promoting the five repeatable-string font rows.

**Re-review verdict:** Approved.

Findings after fix: none.
