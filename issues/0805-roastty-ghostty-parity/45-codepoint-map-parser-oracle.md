# Experiment 45: Codepoint Map Parser Oracle

## Description

CFG-217 still has 6 parser rows that are only `Audit covered`. Two of those are
codepoint-map parsers that share the pinned Ghostty Unicode range grammar:
`font-codepoint-map` and `clipboard-codepoint-map`.

Pinned Ghostty's `RepeatableCodepointMap.parseCLI` and
`RepeatableClipboardCodepointMap.parseCLI` require a value, split on the first
`=`, trim ASCII space and tab around the range side and replacement side, parse
one or more `U+...` codepoint ranges separated by commas, reject malformed
ranges and descending ranges, and append one entry per parsed range.
`font-codepoint-map` stores a font family descriptor string. Clipboard mappings
store either a `U+...` replacement `u21` codepoint or a literal UTF-8 string
replacement. Pinned Ghostty does not reject non-scalar-but-`u21` clipboard
ranges or replacement codepoints at parse time. Direct empty input reaches the
first-`=` split and is `InvalidValue`; the set-but-empty reset behavior belongs
to Roastty's higher-level config dispatch boundary, not to Ghostty's direct
`parseCLI`. Formatting emits one blank entry for an empty map or one
`U+XXXX[-U+YYYY]=...` entry per stored mapping.

Roastty already has focused coverage for both parsers, including empty resets,
range parsing, formatting, and invalid cases. This experiment will make that
coverage an explicit CFG-217 oracle, extend it where needed for the direct
parser boundary and file/CLI diagnostics, wire the parser inventory to recognize
the oracle, and promote only the two canonical codepoint-map rows.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing codepoint-map regressions as
    `codepoint_map_config_parser_family_oracle`.
  - Extend the oracle if needed to cover:
    - direct missing value returning `ValueRequired`;
    - direct exact empty values returning `InvalidValue`;
    - set-but-empty config dispatch resetting each map outside the direct parser
      helper;
    - first-equals splitting and ASCII space/tab trimming;
    - single ranges, inclusive ranges, comma-separated ranges, and mixed lists;
    - malformed prefixes, missing hex digits, bad trailing separators, and
      descending ranges returning `InvalidValue`;
    - `font-codepoint-map` font-family descriptor storage and formatting;
    - `clipboard-codepoint-map` codepoint replacements, literal string
      replacements, invalid codepoint replacements, and pinned Ghostty `u21`
      acceptance for non-scalar ranges/replacements;
    - fixing any current Roastty direct-empty or scalar-rejection divergence
      before promotion;
    - config-file diagnostics;
    - CLI parsing, formatting, and clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `codepoint_map_config_parser_family_oracle`.
  - Mark only canonical `font-codepoint-map` and `clipboard-codepoint-map` as
    `Oracle complete` when the oracle test is present.
  - Add the oracle to CFG-217 ownership so the generated matrix records
    `Experiment 45` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 199 `Oracle complete`, 4
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 199 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting codepoint-map parser semantics after the result
    is proven.

## Verification

Pass criteria:

- Focused Roastty codepoint-map oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml codepoint_map_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=199`;
  - `audit_covered=4`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 199 rows are `Oracle complete`;
  - `font-codepoint-map` and `clipboard-codepoint-map` are `Oracle complete`;
  - the remaining `Audit covered` set is exactly the pre-existing audit-only set
    minus those two rows;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 45`;
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
cargo test --manifest-path roastty/Cargo.toml codepoint_map_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 199
assert sum(row[4] == 'Audit covered' for row in rows) == 4
assert not [row for row in rows if row[4] == 'Gap']
expected_audit = {
    '`config-default-files`',
    '`key-remap`',
    '`keybind`',
    '`theme`',
}
actual_audit = {row[1] for row in rows if row[4] == 'Audit covered'}
assert actual_audit == expected_audit, sorted(actual_audit ^ expected_audit)
for option in {'`font-codepoint-map`', '`clipboard-codepoint-map`'}:
    row = next(row for row in rows if row[1] == option)
    assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 45', cfg217
assert '199 parser rows Oracle complete' in cfg217[12], cfg217
print('codepoint_map_rows=2 oracle_complete=199 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/45-codepoint-map-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Reviewed by an adversarial Codex subagent with fresh context.

**Initial verdict:** Changes required.

Required findings:

- The design required clipboard scalar-range rejection even though pinned
  Ghostty accepts non-scalar-but-`u21` ranges and replacement codepoints at
  parse time.
- The design blurred direct `parseCLI` empty-input behavior with higher-level
  config set-empty reset behavior.

Fixes:

- Updated the design to require pinned Ghostty `u21` clipboard semantics and
  fixing any current stricter scalar rejection before promotion.
- Separated direct empty input, which must be `InvalidValue`, from the
  higher-level set-but-empty config reset boundary.

**Re-review verdict:** Approved.

Findings after fix: none.

## Result

**Result:** Pass.

Roastty now exposes a focused `codepoint_map_config_parser_family_oracle` that
covers direct parser behavior and config-boundary behavior for
`font-codepoint-map` and `clipboard-codepoint-map`. The implementation now keeps
direct empty input as `InvalidValue`, moves set-but-empty reset behavior to the
higher-level config dispatch boundary, and preserves pinned Ghostty's `u21`
clipboard behavior instead of rejecting non-scalar-but-in-range codepoints.

The oracle proves missing-value errors, direct empty invalidity, config empty
resets, first-equals splitting, ASCII space/tab trimming, single ranges,
inclusive ranges, comma-separated ranges, malformed range rejection, descending
range rejection, font descriptor storage, clipboard codepoint replacements,
clipboard literal string replacements, `u21` non-scalar range/replacement
acceptance, diagnostics, CLI parsing, formatting, and clone semantics.

The parser inventory generator now detects that oracle and promotes only
canonical `font-codepoint-map` and `clipboard-codepoint-map` to
`Oracle complete`. The regenerated CFG-217 parser inventory reports 203 parser
rows, 199 `Oracle complete`, 4 `Audit covered`, and 0 `Gap`. CFG-217 remains
`Gap` because the remaining audit-only parser rows still need their own
upstream-derived oracles.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml codepoint_map_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 199
assert sum(row[4] == 'Audit covered' for row in rows) == 4
assert not [row for row in rows if row[4] == 'Gap']
expected_audit = {
    '`config-default-files`',
    '`key-remap`',
    '`keybind`',
    '`theme`',
}
actual_audit = {row[1] for row in rows if row[4] == 'Audit covered'}
assert actual_audit == expected_audit, sorted(actual_audit ^ expected_audit)
for option in {'`font-codepoint-map`', '`clipboard-codepoint-map`'}:
    row = next(row for row in rows if row[1] == option)
    assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 45', cfg217
assert '199 parser rows Oracle complete' in cfg217[12], cfg217
print('codepoint_map_rows=2 oracle_complete=199 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/45-codepoint-map-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Observed key outputs:

- `test config::tests::codepoint_map_config_parser_family_oracle ... ok`
- `oracle_complete=199`
- `audit_covered=4`
- `gap=0`
- `codepoint_map_rows=2 oracle_complete=199 cfg217=Gap`

## Conclusion

The two codepoint-map parser rows are no longer audit-only. Future parser work
can focus on the 4 remaining audit-only rows: `config-default-files`,
`key-remap`, `keybind`, and `theme`.

## Completion Review

Reviewed by an adversarial Codex subagent with fresh context.

**Verdict:** Approved.

Findings: none.

The reviewer independently verified the focused Rust oracle, matrix assertion,
Python compile check, `cargo fmt --check`, `git diff --check`, and that the
result commit had not yet been made. The reviewer noted that Python compile
created an `__pycache__` artifact during read-only verification; it was removed
before the result commit.
