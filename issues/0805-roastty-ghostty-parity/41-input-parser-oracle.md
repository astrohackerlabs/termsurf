# Experiment 41: Input Parser Oracle

## Description

CFG-217 still has 20 parser rows that are only `Audit covered`. Canonical
`input` is one of the remaining `custom parse_cli` rows, and it routes through
pinned Ghostty's `RepeatableReadableIO.parseCLI` semantics.

Pinned Ghostty's `RepeatableReadableIO` parser requires a value, treats an
exactly empty value as a reset that clears the list, and otherwise appends one
`ReadableIO`. `ReadableIO.parseCLI` first validates the full input with Zig
string-literal parsing, then parses tagged-union prefixes: `raw:` stores a raw
payload, `path:` stores a path payload, and any invalid/unknown tag falls back
to raw input. Formatting emits a blank entry for an empty list and one explicit
`raw:` or `path:` entry per stored item. CLI parsing and config-file parsing use
the same helper.

Roastty already has a focused `input` regression covering defaults, raw/path
entries, unknown-prefix raw fallback, raw-empty payloads, exact empty reset,
missing values, invalid string-literal diagnostics, CLI parsing, formatting, and
clone behavior. This experiment will make that coverage an explicit CFG-217
oracle, extend it where needed for the direct parser boundary, wire the parser
inventory to recognize the oracle, and promote only canonical `input`.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing `input` regression as
    `input_config_parser_family_oracle` so the inventory generator can detect it
    as the CFG-217 oracle for `RepeatableReadableIo::parse_cli`.
  - Extend the oracle if needed to cover:
    - missing direct parser values;
    - direct empty value clearing the list;
    - direct non-empty `ReadableIo::parse_cli` required-value behavior;
    - raw and path tagged values;
    - unknown tags falling back to raw;
    - `raw:` with an empty payload being valid;
    - invalid Zig string-literal escapes rejected before appending;
    - config-file diagnostics preserving/resetting state exactly as upstream;
    - CLI argument parsing through the same helper;
    - formatter output for empty, raw, path, and unknown-tag raw entries;
    - clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `input_config_parser_family_oracle`.
  - Mark only canonical `input` as `Oracle complete` when the oracle test is
    present.
  - Add input oracle detection to CFG-217 ownership so the generated matrix
    records `Experiment 41` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 184 `Oracle complete`, 19
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 184 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `input` parser semantics after the result is
    proven.

## Verification

Pass criteria:

- Focused Roastty input-family oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml input_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=184`;
  - `audit_covered=19`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 184 rows are `Oracle complete`;
  - `input` is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 41`;
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
cargo test --manifest-path roastty/Cargo.toml input_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 184
assert sum(row[4] == 'Audit covered' for row in rows) == 19
assert not [row for row in rows if row[4] == 'Gap']
row = next(row for row in rows if row[1] == '`input`')
assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 41', cfg217
assert '184 parser rows Oracle complete' in cfg217[12], cfg217
print('input_oracle_rows=1 oracle_complete=184 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/41-input-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Reviewed by an adversarial Codex subagent with fresh context.

**Verdict:** Approved.

Findings: none.

## Result

**Result:** Pass.

Roastty now exposes the existing `input` regression as
`input_config_parser_family_oracle` and extends it to cover the direct
`ReadableIo::parse_cli` and `RepeatableReadableIo::parse_cli` boundaries. The
oracle proves required missing values, exact-empty repeatable resets, raw/path
tag parsing, unknown-tag raw fallback, valid `raw:` empty payloads, invalid
Zig-string-literal rejection before append, config-file diagnostics, CLI
parsing, formatting, and clone semantics.

The parser inventory generator now detects that oracle and promotes only
canonical `input` to `Oracle complete`. The regenerated CFG-217 parser inventory
reports 203 parser rows, 184 `Oracle complete`, 19 `Audit covered`, and 0 `Gap`.
CFG-217 remains `Gap` because the remaining audit-only parser rows still need
their own upstream-derived oracles.

Verification run:

```bash
cargo test --manifest-path roastty/Cargo.toml input_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 184
assert sum(row[4] == 'Audit covered' for row in rows) == 19
assert not [row for row in rows if row[4] == 'Gap']
row = next(row for row in rows if row[1] == '`input`')
assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 41', cfg217
assert '184 parser rows Oracle complete' in cfg217[12], cfg217
print('input_oracle_rows=1 oracle_complete=184 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/41-input-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

Observed key outputs:

- `test config::tests::input_config_parser_family_oracle ... ok`
- `oracle_complete=184`
- `audit_covered=19`
- `gap=0`
- `input_oracle_rows=1 oracle_complete=184 cfg217=Gap`

## Conclusion

Canonical `input` is no longer an audit-only parser row. Future parser work can
focus on the 19 remaining audit-only rows: codepoint maps, font-family/style and
variation helpers, key remap/keybind, theme, and `config-default-files`.

## Completion Review

Reviewed by an adversarial Codex subagent with fresh context.

**Verdict:** Approved.

Findings: none.

The reviewer independently verified the focused Rust oracle, parser inventory
counts, matrix assertion, Python compile check, `cargo fmt --check`, and
`git diff --check`.
