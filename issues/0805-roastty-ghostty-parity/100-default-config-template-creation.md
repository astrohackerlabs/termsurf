# Experiment 100: Default config template creation

## Description

Experiment 99 identified `LOAD-008` as a structural CFG-221 gap. Pinned Ghostty
creates a template config file when none of the default config candidates are
found:

- on macOS, after both XDG and Application Support candidates are absent, it
  creates the template at the preferred Application Support config path;
- on non-macOS, after the XDG candidates are absent, it creates the template at
  the preferred XDG config path;
- creation errors are logged/warned but do not abort config loading.

Roastty currently loads the same default candidate families but does not record
or create the template file when all default candidates are missing. This
experiment will implement Roastty's equivalent template creation behavior and
promote only `LOAD-008` to `Oracle complete`.

## Changes

- `roastty/src/config/mod.rs`
  - Add an embedded default config template using pinned Ghostty's
    `vendor/ghostty/src/config/config-template`.
  - Add a small helper that creates parent directories, writes the template to
    the selected absolute/owned path, and substitutes the target path into the
    template placeholder.
  - Extend `DefaultConfigLoadReport` with fields that record whether a template
    was created and any nonfatal creation error.
  - Update `Config::load_default_files_from_paths` so that when no default XDG
    or app-support candidate is present:
    - if `preferred_app_support` is present, create the template there;
    - otherwise, if `preferred_xdg` is present, create the template there;
    - otherwise, do nothing because no writable default target is known.
  - Preserve existing duplicate reporting, error continuation, same-path
    app-support deduplication, and load order semantics.
  - Add focused unit tests for:
    - missing XDG plus missing app-support creates a template at preferred
      app-support with the pinned template text and selected path substituted
      into the template;
    - missing XDG with no app-support target creates a template at preferred XDG
      with the pinned template text and selected path substituted into the
      template;
    - any loaded or error default candidate suppresses template creation,
      matching Ghostty's `OptionalFileAction != .not_found` loaded flag;
    - template creation errors are recorded but do not abort loading.

- `issues/0805-roastty-ghostty-parity/config_load_inventory.py`
  - Promote `LOAD-008` from `Gap` to `Oracle complete`.
  - Update evidence to name the new focused Roastty unit tests, including the
    content oracle proving the created file matches the pinned template after
    path substitution.

- `issues/0805-roastty-ghostty-parity/config-load-inventory.md`
  - Regenerate the inventory.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-221 counts. CFG-221 must remain `Gap` because `LOAD-001` and
    `LOAD-017` still are not `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if implementation exposes a reusable rule beyond the
    expected template creation behavior.

## Verification

Pass criteria:

- New focused unit tests prove template creation target selection, suppression,
  nonfatal error recording, and created file contents.
- The created-file content oracle proves the generated file equals pinned
  Ghostty's template behavior with the selected target path substituted into the
  template placeholder. LOAD-008 must not be promoted by an empty-file or
  existence-only test.
- Existing default-file load tests still pass, proving no regression to load
  order, duplicate reporting, or error continuation.
- The generated load inventory reports:
  - 18 total rows;
  - 16 `Oracle complete` rows;
  - 1 `Audit covered` row;
  - 1 `Gap` row;
  - 2 incomplete rows.
- `LOAD-008` is `Oracle complete`.
- CFG-221 remains `Gap`, points to `config-load-inventory.md`, and records the
  updated counts.
- CFG-217, CFG-218, CFG-219, and CFG-220 remain byte-for-byte unchanged from
  result commit `55af75479` after final Markdown formatting.
- Hygiene passes:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  cargo test --manifest-path roastty/Cargo.toml config_load_default_files
  PYTHONDONTWRITEBYTECODE=1 python3 \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/100-default-config-template-creation.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/100-default-config-template-creation.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The initial design planned to embed pinned Ghostty's `config-template`, but
  verification only proved target selection, suppression, and nonfatal error
  recording. That could promote `LOAD-008` with an empty or wrong file.
- The inventory evidence needed to name a content oracle, not just creation
  mechanics.

Fix:

- Added pass criteria and test scope requiring the created file to match pinned
  Ghostty's template behavior with the selected target path substituted into the
  template placeholder.
- Added an explicit rule that `LOAD-008` must not be promoted by an empty-file
  or existence-only test.

Final verdict: Approved.

## Result

**Result:** Pass

Implemented default config template creation for the default-file load path.
Roastty now creates the pinned Ghostty template when no default config candidate
is loaded or errors:

- preferred Application Support target wins when it is available;
- preferred XDG target is used when no Application Support target is available;
- loaded or error default candidates suppress template creation;
- template creation errors are recorded in the load report and do not abort
  loading.

The focused tests also prove the generated file contents match the pinned
Ghostty template after substituting the selected path into the template
placeholder.

`LOAD-008` is now `Oracle complete`. The generated CFG-221 load inventory
reports:

- 18 total load rows;
- 16 `Oracle complete` rows;
- 1 `Audit covered` row;
- 1 `Gap` row;
- 2 rows not yet `Oracle complete`.

CFG-221 remains `Gap` because `LOAD-001` remains audit-covered and `LOAD-017`
remains a structural gap.

Verification run:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_load_default_files
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
old_matrix=subprocess.check_output(['git','show','55af75479:issues/0805-roastty-ghostty-parity/config-matrix.md'], text=True)
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
statuses={row[0]: row[5] for row in rows}
assert statuses['LOAD-008']=='Oracle complete', statuses['LOAD-008']
oracle=sum(s=='Oracle complete' for s in statuses.values())
incomplete=len(rows)-oracle
gaps=sum(s=='Gap' for s in statuses.values())
audit=sum(s=='Audit covered' for s in statuses.values())
assert (len(rows), oracle, audit, gaps, incomplete)==(18,16,1,1,2), (len(rows), oracle, audit, gaps, incomplete)
cfg221=next(line for line in matrix.splitlines() if line.startswith('| CFG-221 |'))
cells=[c.strip() for c in cfg221.strip('|').split('|')]
assert cells[4]=='Gap', cells[4]
assert 'config-load-inventory.md' in cfg221
assert '16 rows Oracle complete' in cfg221
assert '2 rows are not Oracle complete' in cfg221
assert '1 rows are load gaps' in cfg221
print('load_rows=18 oracle_complete=16 audit_covered=1 incomplete=2 gaps=1 cfg221=Gap load008=Oracle complete protected_cfg217_220_unchanged=true')
PY
```

The focused test filter passed with 10 tests. The matrix assertion printed:

```text
load_rows=18 oracle_complete=16 audit_covered=1 incomplete=2 gaps=1 cfg221=Gap load008=Oracle complete protected_cfg217_220_unchanged=true
```

## Conclusion

Default config template creation now matches pinned Ghostty's load behavior and
content generation closely enough for `LOAD-008` to be `Oracle complete`.
CFG-221 still needs follow-up work for end-to-end load pipeline order and
recursive replay placement before the initial command suffix.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

The reviewer found no required fixes before the result commit. The review
confirmed:

- the implementation matches pinned Ghostty `LOAD-008` behavior for selected
  target, parent creation, pinned template content, path substitution, and
  nonfatal write errors;
- the focused Rust tests pass;
- `LOAD-008` is promoted to `Oracle complete` while `LOAD-001` remains
  `Audit covered` and `LOAD-017` remains `Gap`;
- generated counts are 18 rows, 16 `Oracle complete`, 1 `Audit covered`, 1
  `Gap`, and 2 incomplete rows;
- CFG-217 through CFG-220 remain byte-for-byte unchanged from `55af75479`;
- the result docs and README status/learnings are accurate.
