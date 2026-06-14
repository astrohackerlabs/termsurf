# Experiment 39: Env Parser Oracle

## Description

CFG-217 still has 23 parser rows that are only `Audit covered`. Canonical `env`
is one of the remaining `custom parse_cli` rows, and it routes through pinned
Ghostty's shared `RepeatableStringMap.parseCLI` semantics.

Pinned Ghostty's `RepeatableStringMap` parser requires a value, treats an empty
value as a reset that clears the map, requires the first `=`, trims ASCII
whitespace around the key and value, removes a key when the parsed value is
empty, overwrites existing keys without changing map equality semantics, and
otherwise inserts a key/value entry. Formatting emits a blank entry for an empty
map and one `key=value` entry per stored map entry.

Roastty already has a focused `env` regression covering defaults, insert,
overwrite, key deletion, empty reset, missing/no-`=` diagnostics, formatting,
order-insensitive equality, and clone behavior. This experiment will make that
coverage an explicit CFG-217 oracle, extend it where needed for the direct map
parser boundary, wire the parser inventory to recognize the oracle, and promote
only canonical `env`.

CFG-217 must remain `Gap` because other parser helpers are still audit-only.

## Changes

- `roastty/src/config/mod.rs`
  - Rename or wrap the existing `env` regression as
    `env_config_parser_family_oracle` so the inventory generator can detect it
    as the CFG-217 oracle for `RepeatableStringMap::parse_cli`.
  - Extend the oracle if needed to cover:
    - missing direct parser values;
    - direct empty value clearing the map;
    - missing `=` returning `ValueRequired`;
    - whitespace-only direct values returning `ValueRequired` rather than
      clearing the map;
    - first-`=` splitting so values may contain additional `=`;
    - ASCII whitespace trimming around key and value;
    - empty keys being accepted;
    - empty parsed values deleting a key;
    - repeated keys overwriting prior values;
    - formatting of empty and non-empty maps;
    - config-file diagnostics preserving earlier valid values;
    - order-insensitive equality and clone semantics.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Detect `env_config_parser_family_oracle`.
  - Mark only canonical `env` as `Oracle complete` when the oracle test is
    present.
  - Add env oracle detection to CFG-217 ownership so the generated matrix
    records `Experiment 39` when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the inventory. Expected status counts: 181 `Oracle complete`, 22
    `Audit covered`, 0 `Gap`.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Keep CFG-217 as `Gap`, but update the note to show 181 parser rows are now
    `Oracle complete`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning documenting `env` / `RepeatableStringMap` parser semantics
    after the result is proven.

## Verification

Pass criteria:

- Focused Roastty env-family oracle passes:

```bash
cargo test --manifest-path roastty/Cargo.toml env_config_parser_family_oracle
```

- Parser inventory generator succeeds and reports:
  - `ghostty_canonical=203`;
  - `roastty_parser_rows=203`;
  - `missing_dispatch_rows=0`;
  - `extra_parser_rows=0`;
  - `oracle_complete=181`;
  - `audit_covered=22`;
  - `gap=0`.
- Matrix assertion verifies:
  - `config-parser-inventory.md` has 203 `PARSE-` rows;
  - exactly 181 rows are `Oracle complete`;
  - `env` is `Oracle complete`;
  - no row is `Gap`;
  - CFG-217 remains `Gap`;
  - CFG-217 owner is `Experiment 39`;
  - CFG-217 evidence points to `config-parser-inventory.md`.
- `cargo fmt --manifest-path roastty/Cargo.toml` is run.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  markdown files.
- `python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  passes.
- No `__pycache__` or other `py_compile` artifacts remain in the issue folder.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Required findings: none.

Optional finding accepted:

- The reviewer noted that the oracle checklist should explicitly name
  whitespace-only direct input as a rejection case. Upstream resets only an
  exactly empty value; whitespace-only input lacks `=` and returns
  `ValueRequired`.

Fix:

- Added whitespace-only direct values returning `ValueRequired` to the planned
  oracle coverage list.

Nit not applied:

- The reviewer noted that `PYTHONDONTWRITEBYTECODE=1 py_compile` should not
  create `__pycache__`. The explicit cleanup remains as a harmless hygiene guard
  consistent with prior Issue 805 experiments.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml env_config_parser_family_oracle
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
assert sum(row[4] == 'Oracle complete' for row in rows) == 181
assert sum(row[4] == 'Audit covered' for row in rows) == 22
assert not [row for row in rows if row[4] == 'Gap']
row = next(row for row in rows if row[1] == '`env`')
assert row[4] == 'Oracle complete', row

cfg217 = None
for line in Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text().splitlines():
    if line.startswith('| CFG-217 |'):
        cfg217 = [cell.strip() for cell in line.strip('|').split('|')]
        break
assert cfg217 is not None
assert cfg217[4] == 'Gap', cfg217
assert cfg217[11] == 'Experiment 39', cfg217
assert '181 parser rows Oracle complete' in cfg217[12], cfg217
print('env_oracle_rows=1 oracle_complete=181 cfg217=Gap')
PY
cargo fmt --manifest-path roastty/Cargo.toml
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/39-env-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Result

**Result:** Pass

Roastty now has a focused env parser family oracle for the canonical `env` row.

Implementation notes:

- Renamed and extended the existing config-routing regression as
  `env_config_parser_family_oracle`.
- Added direct parser coverage for missing values, whitespace-only values,
  missing `=`, and exactly empty reset behavior.
- Taught `config_parser_inventory.py` to detect the env oracle, promote only
  `env`, and make CFG-217's owner `Experiment 39` when this oracle is present.

Verification commands run:

```bash
cargo test --manifest-path roastty/Cargo.toml env_config_parser_family_oracle
```

Result:

```text
test config::tests::env_config_parser_family_oracle ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 4926 filtered out
```

```bash
PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py \
  --upstream vendor/ghostty/src/config/Config.zig \
  --roastty roastty/src/config/mod.rs \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

Result:

```text
ghostty_canonical=203
roastty_parser_rows=203
missing_canonical_parser_rows=0
missing_dispatch_rows=0
extra_parser_rows=0
compatibility_only_parser_arms=5
noncanonical_noncompat_parser_arms=0
oracle_complete=181
audit_covered=22
gap=0
```

The matrix assertion passed and printed:

```text
env_oracle_rows=1 oracle_complete=181 cfg217=Gap
```

Additional hygiene checks passed:

```bash
python3 -m py_compile issues/0805-roastty-ghostty-parity/config_parser_inventory.py
rm -rf issues/0805-roastty-ghostty-parity/__pycache__
cargo fmt --manifest-path roastty/Cargo.toml
```

## Conclusion

The `env` parser row is now oracle-complete for CFG-217. The important upstream
boundary is that an exactly empty value resets the whole map, while a
whitespace-only direct value is not empty, has no `=`, and is therefore
`ValueRequired`. CFG-217 remains `Gap` because 22 parser rows are still only
audit-covered.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer independently verified the focused env oracle test, Rust format
check, `git diff --check`, generated inventory counts, the promoted `env` row,
CFG-217's `Gap` status and Experiment 39 ownership, absence of Python cache
artifacts, and that the result commit had not yet been made.
