# Experiment 99: Source precedence load inventory

## Description

CFG-221 is currently one broad gap: source precedence and repeated-file load
semantics. Pinned Ghostty's config loading is not a single parser behavior. It
is a load pipeline:

1. defaults are constructed;
2. default config files are loaded in XDG and platform-specific order;
3. CLI args are loaded, including CLI-only `config-default-files` replay
   behavior;
4. recursively referenced `config-file` entries are loaded after default files
   and CLI args;
5. path-valued config entries are expanded relative to the source that loaded
   them;
6. replay entries preserve enough source order to rebuild config during theme
   and conditional reload paths.

Roastty already has focused tests for many of these pieces, but Issue 805 does
not have a CFG-221 row-level inventory that distinguishes proven load oracles
from remaining source-precedence gaps. This experiment will build that inventory
before attempting to close CFG-221.

The expected outcome is a generated load/precedence inventory and matrix
consistency guard. CFG-221 must remain `Gap` unless every load row is
`Oracle complete`.

## Changes

- `issues/0805-roastty-ghostty-parity/config_load_inventory.py`
  - Add a bounded inventory generator for CFG-221.
  - Encode pinned Ghostty source-precedence/load operations from
    `vendor/ghostty/src/config/Config.zig::load`, `loadFile`, `loadReader`,
    `loadOptionalFile`, `loadDefaultFiles`, `loadCliArgs`, `loadRecursiveFiles`,
    and replay-driven rebuild paths as explicit rows.
  - Define an expected row manifest in the generator and fail generation unless
    the emitted row IDs exactly match it:
    - `LOAD-001` full load pipeline order;
    - `LOAD-002` config-file reader parsing and BOM skipping;
    - `LOAD-003` config-file path base expansion after file load;
    - `LOAD-004` optional file three-way loaded/not-found/error behavior;
    - `LOAD-005` default XDG and platform file load order;
    - `LOAD-006` default file duplicate reporting;
    - `LOAD-007` default file errors/diagnostics continue loading;
    - `LOAD-008` default template creation when no default file exists;
    - `LOAD-009` CLI diagnostics and good-argument continuation;
    - `LOAD-010` CLI repeatable font overwrite behavior;
    - `LOAD-011` CLI `config-default-files` discard/replay behavior;
    - `LOAD-012` CLI path base expansion;
    - `LOAD-013` recursive `config-file` load order and newly appended files;
    - `LOAD-014` recursive optional/required missing file behavior;
    - `LOAD-015` recursive non-file diagnostics;
    - `LOAD-016` recursive cycle diagnostics;
    - `LOAD-017` recursive replay placement before `-e`/initial command suffix;
    - `LOAD-018` replay preservation for theme/conditional rebuild behavior.
  - For each row, record:
    - the load or precedence behavior;
    - the pinned Ghostty source reference;
    - the Roastty implementation reference;
    - the current coverage status;
    - evidence from existing Roastty tests or issue artifacts;
    - the missing proof required before the row can become `Oracle complete`.
  - Mark rows `Oracle complete` only when there is focused evidence for source
    order, state mutation, diagnostics/errors, repeatable behavior, and path
    base behavior where relevant.
  - Mark rows `Audit covered` when the behavior is identified and appears
    implemented but lacks sufficient oracle coverage.
  - Mark rows `Gap` when the generator cannot identify a Roastty implementation
    or the behavior appears materially unimplemented.
  - Update CFG-221 in `config-matrix.md` from generated row counts while leaving
    CFG-217, CFG-218, CFG-219, and CFG-220 unchanged.

- `issues/0805-roastty-ghostty-parity/config-load-inventory.md`
  - Record generated load/precedence rows, coverage counts, evidence, and
    missing proof.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Update CFG-221 to point at `config-load-inventory.md`.
  - Keep CFG-221 as `Gap` unless every load row is `Oracle complete`.
  - Include generated counts in the CFG-221 note.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the inventory discovers a reusable source-precedence
    proof rule or concrete mismatch.

## Verification

Pass criteria:

- The load inventory generator exits successfully and reports:
  - the exact expected load row count from the manifest;
  - no duplicate row IDs;
  - no duplicate load behavior names;
  - generated row IDs exactly match the expected manifest;
  - coverage counts for `Oracle complete`, `Audit covered`, and `Gap`.
- Every generated load row names:
  - the behavior;
  - the pinned Ghostty source reference;
  - the Roastty implementation reference or missing implementation;
  - current coverage status;
  - concrete evidence or concrete missing evidence.
- A matrix assertion verifies that CFG-221 is internally consistent:
  - if every load inventory row is `Oracle complete`, CFG-221 may be `Pass`;
  - if any load row is `Audit covered` or `Gap`, CFG-221 remains `Gap`;
  - CFG-221 points to `config-load-inventory.md`;
  - CFG-221 notes the current `Oracle complete`, incomplete, and gap counts.
- The generator must not disturb CFG-217, CFG-218, CFG-219, or CFG-220. Capture
  all four full matrix rows before running the generator and assert they are
  byte-for-byte unchanged after generation and final Markdown formatting.
- Existing focused load/precedence tests referenced by `Oracle complete` rows
  pass with the narrowest relevant `cargo test` filters.
- Python and Markdown hygiene pass:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/99-source-precedence-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/99-source-precedence-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

Suggested matrix assertion:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

matrix = Path('issues/0805-roastty-ghostty-parity/config-matrix.md').read_text()
protected = [
    line for line in matrix.splitlines()
    if line.startswith('| CFG-217 |')
    or line.startswith('| CFG-218 |')
    or line.startswith('| CFG-219 |')
    or line.startswith('| CFG-220 |')
]
assert len(protected) == 4, protected
Path('/tmp/issue805-exp99-cfg217-220-before.txt').write_text(
    '\n'.join(protected) + '\n'
)
PY
PYTHONDONTWRITEBYTECODE=1 python3 \
  issues/0805-roastty-ghostty-parity/config_load_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-load-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-load-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

issue = Path('issues/0805-roastty-ghostty-parity')
matrix = (issue / 'config-matrix.md').read_text()
protected_before = Path('/tmp/issue805-exp99-cfg217-220-before.txt').read_text()
protected_after = [
    line for line in matrix.splitlines()
    if line.startswith('| CFG-217 |')
    or line.startswith('| CFG-218 |')
    or line.startswith('| CFG-219 |')
    or line.startswith('| CFG-220 |')
]
assert protected_before == '\n'.join(protected_after) + '\n'

rows = []
for line in (issue / 'config-load-inventory.md').read_text().splitlines():
    if line.startswith('| LOAD-'):
        rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert rows, 'expected load rows'
expected_ids = [
    'LOAD-001',
    'LOAD-002',
    'LOAD-003',
    'LOAD-004',
    'LOAD-005',
    'LOAD-006',
    'LOAD-007',
    'LOAD-008',
    'LOAD-009',
    'LOAD-010',
    'LOAD-011',
    'LOAD-012',
    'LOAD-013',
    'LOAD-014',
    'LOAD-015',
    'LOAD-016',
    'LOAD-017',
    'LOAD-018',
]
ids = [row[0] for row in rows]
behaviors = [row[1] for row in rows]
assert ids == expected_ids, ids
assert len(ids) == len(set(ids)), ids
assert len(behaviors) == len(set(behaviors)), behaviors
statuses = [row[5] for row in rows]
oracle_complete = sum(status == 'Oracle complete' for status in statuses)
incomplete = len(rows) - oracle_complete
gap_count = sum(status == 'Gap' for status in statuses)

cfg221 = next(line for line in matrix.splitlines() if line.startswith('| CFG-221 |'))
cfg221_cells = [cell.strip() for cell in cfg221.strip('|').split('|')]
assert 'config-load-inventory.md' in cfg221
assert (incomplete == 0 and cfg221_cells[4] == 'Pass') or (
    incomplete > 0 and cfg221_cells[4] == 'Gap'
)
assert f'{oracle_complete} rows Oracle complete' in cfg221
assert f'{incomplete} rows are not Oracle complete' in cfg221
assert f'{gap_count} rows are load gaps' in cfg221
print(
    f'load_rows={len(rows)} '
    f'incomplete={incomplete} gaps={gap_count} cfg221={cfg221_cells[4]}'
)
PY
```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The design allowed the generator to emit any nonzero set of rows and then make
  CFG-221 internally consistent over only those rows. That could falsely pass
  CFG-221 while omitting material pinned Ghostty load operations such as
  `loadFile`, `loadOptionalFile`, recursive cycle/non-file diagnostics, or
  replay suffix placement.

Fix:

- Added an explicit expected load row manifest covering pinned Ghostty's load
  pipeline, file reader/BOM behavior, optional/default file loading, CLI
  replay/default-file discard/font overwrite/path-base behavior, recursive
  `config-file` order/error/cycle/replay placement, and theme/conditional replay
  preservation.
- Updated verification so the generator must report the exact manifest row count
  and the matrix assertion must verify generated row IDs exactly match the
  manifest before CFG-221 can pass.

Final verdict: Approved.

Re-review confirmed that the explicit `LOAD-001` through `LOAD-018` manifest and
exact row-ID assertion prevent CFG-221 from passing over an arbitrary incomplete
subset of load rows.

## Result

**Result:** Pass

Implemented the generated CFG-221 load/precedence inventory and matrix guard.
The generator emits exactly 18 load rows:

- 15 `Oracle complete`;
- 1 `Audit covered`;
- 2 `Gap`;
- 3 rows not yet `Oracle complete` in total.

CFG-221 remains `Gap`, as intended, because not every load row is
`Oracle complete`. The incomplete rows are:

- `LOAD-001` full load pipeline order — `Audit covered`;
- `LOAD-008` default template creation when no default file exists — `Gap`;
- `LOAD-017` recursive replay placement before `-e`/initial command suffix —
  `Gap`.

Verification run:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 \
  issues/0805-roastty-ghostty-parity/config_load_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-load-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-load-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
import subprocess
from pathlib import Path
issue=Path('issues/0805-roastty-ghostty-parity')
matrix=(issue/'config-matrix.md').read_text()
old_matrix=subprocess.check_output(['git','show','958be6502:issues/0805-roastty-ghostty-parity/config-matrix.md'], text=True)
for cfg in ['CFG-217','CFG-218','CFG-219','CFG-220']:
    old=next(line for line in old_matrix.splitlines() if line.startswith(f'| {cfg} |'))
    new=next(line for line in matrix.splitlines() if line.startswith(f'| {cfg} |'))
    assert old == new, cfg
rows=[]
for line in (issue/'config-load-inventory.md').read_text().splitlines():
    if line.startswith('| LOAD-'):
        rows.append([cell.strip() for cell in line.strip('|').split('|')])
expected_ids=[f'LOAD-{i:03d}' for i in range(1,19)]
ids=[row[0] for row in rows]
assert ids == expected_ids, ids
assert len(set(ids)) == len(ids)
behaviors=[row[1] for row in rows]
assert len(set(behaviors)) == len(behaviors)
statuses=[row[5] for row in rows]
oracle=sum(s=='Oracle complete' for s in statuses)
incomplete=len(rows)-oracle
gaps=sum(s=='Gap' for s in statuses)
audit=sum(s=='Audit covered' for s in statuses)
assert (len(rows), oracle, audit, gaps, incomplete)==(18,15,1,2,3), (len(rows), oracle, audit, gaps, incomplete)
cfg221=next(line for line in matrix.splitlines() if line.startswith('| CFG-221 |'))
cells=[c.strip() for c in cfg221.strip('|').split('|')]
assert cells[4]=='Gap', cells[4]
assert 'config-load-inventory.md' in cfg221
assert '15 rows Oracle complete' in cfg221
assert '3 rows are not Oracle complete' in cfg221
assert '2 rows are load gaps' in cfg221
print('load_rows=18 oracle_complete=15 audit_covered=1 incomplete=3 gaps=2 cfg221=Gap protected_cfg217_220_unchanged=true')
PY
cargo test --manifest-path roastty/Cargo.toml \
  config_load_str_applies_lines_and_collects_diagnostics
cargo test --manifest-path roastty/Cargo.toml config_load_file_reads_and_skips_bom
cargo test --manifest-path roastty/Cargo.toml \
  config_load_optional_file_three_way_action
cargo test --manifest-path roastty/Cargo.toml config_load_default_files
cargo test --manifest-path roastty/Cargo.toml \
  config_set_cli_args_applies_and_collects_diagnostics
cargo test --manifest-path roastty/Cargo.toml \
  config_default_files_parser_family_oracle
cargo test --manifest-path roastty/Cargo.toml \
  config_path_cli_expands_relative_optional_absolute_home_and_missing
cargo test --manifest-path roastty/Cargo.toml config_recursive
cargo test --manifest-path roastty/Cargo.toml config_replay
cargo test --manifest-path roastty/Cargo.toml \
  config_theme_loading_preserves_user_replay_entries
cargo test --manifest-path roastty/Cargo.toml \
  config_conditional_theme_rebuild_preserves_replay_entries_without_duplication
cargo test --manifest-path roastty/Cargo.toml \
  bell_audio_path_expands_from_default_and_recursive_file_bases
```

All focused tests passed. The matrix assertion printed:

```text
load_rows=18 oracle_complete=15 audit_covered=1 incomplete=3 gaps=2 cfg221=Gap protected_cfg217_220_unchanged=true
```

## Conclusion

CFG-221 now has a bounded proof surface instead of one broad unresolved row. The
next CFG-221 work should promote the remaining rows one at a time: end-to-end
load pipeline order, default template creation, and recursive replay placement
before the initial command suffix.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The first formatted `config-matrix.md` widened a table column, changing
  CFG-217 through CFG-220 byte-for-byte from plan commit `958be6502`, even
  though the experiment required those protected rows to remain unchanged after
  formatting.
- The first generator version derived `EXPECTED_IDS` from `len(ROWS)`, so a
  deleted load row would have silently shrunk the expected manifest instead of
  failing validation.

Fix:

- Shortened the generated CFG-221 verification-method cell so Prettier no longer
  repads CFG-217 through CFG-220, then reran the final formatted protected-row
  assertion.
- Replaced the derived expected-ID list with a literal `LOAD-001` through
  `LOAD-018` manifest.

Final verdict: Approved.

Re-review confirmed CFG-217 through CFG-220 are byte-for-byte unchanged from
`958be6502` in the final formatted matrix, the expected load row manifest is
independent and fixed, and CFG-221 remains honest and internally consistent.
