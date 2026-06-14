# Experiment 102: Full load pipeline order

## Description

`LOAD-001` is the last incomplete CFG-221 load row. Pinned Ghostty's
`Config.zig::load` applies the complete configuration pipeline in this order:

1. construct defaults;
2. load default config files;
3. load CLI args;
4. load recursively referenced `config-file` entries;
5. finalize derived/validated values.

Experiments 99 through 101 proved each major piece independently, but CFG-221
still lacks an end-to-end oracle that one load entry point executes the pieces
in the pinned Ghostty order. This experiment will add a narrow pipeline helper
and focused tests that prove ordering by making each stage affect later stages
in an observable way.

## Changes

- `roastty/src/config/mod.rs`
  - Add a small internal load-pipeline helper that starts from
    `Config::default()`, then runs:
    - `load_default_files_from_paths`;
    - `set_cli_args_from_base`;
    - `load_recursive_files_from_config`;
    - `finalize_with_report`.
  - Return a report containing the default-file load report, CLI diagnostics,
    recursive load report, and finalization report so tests can assert every
    stage ran.
  - Keep the helper scoped to the config layer and avoid changing external app
    startup behavior unless an existing caller already needs the helper.
  - Add focused tests proving:
    - the pipeline starts from `Config::default()` by asserting an untouched
      field remains at its pinned default value;
    - default files load before CLI args by using a default file value that CLI
      overrides;
    - CLI args load before recursive files by supplying `--config-file` on the
      CLI and proving the recursive file applies afterward;
    - recursive files run after ordinary CLI values by setting the same scalar
      in CLI and the recursive file and asserting the recursive value wins;
    - recursive files load before finalization by using recursive `window-width`
      / `window-height` values below the minimum and asserting finalization
      clamps them to the deterministic minimum;
    - stage reports expose loaded default files, CLI diagnostics, recursive
      loaded files, and finalization output;
    - existing focused default-file, recursive, replay, and finalization tests
      still pass.

- `issues/0805-roastty-ghostty-parity/config_load_inventory.py`
  - Promote `LOAD-001` from `Audit covered` to `Oracle complete` only if the
    end-to-end pipeline test proves the pinned stage order.
  - Update evidence to name the new focused pipeline-order test.

- `issues/0805-roastty-ghostty-parity/config-load-inventory.md`
  - Regenerate the inventory.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-221 counts. CFG-221 should become `Pass` only when all 18
    load rows are `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning if the pipeline helper becomes the reusable config-load entry
    point for future CFG-222 reload work.

## Verification

Pass criteria:

- The new pipeline-order test proves all five pinned stages run in order:
  defaults, default files, CLI args, recursive files, finalization.
- The test is failure-sensitive:
  - if the pipeline did not start from `Config::default()`, an untouched pinned
    default value assertion would fail;
  - if CLI ran before default files, the default file value would override the
    CLI value and fail the assertion;
  - if recursive files ran before CLI, the CLI-provided `config-file` would not
    load and fail the recursive-load assertion;
  - if recursive files did not run after ordinary CLI values, the CLI scalar
    would win instead of the recursive file scalar and fail the same-key
    precedence assertion;
  - if finalization ran before recursive files, the recursive file value would
    not be clamped and the deterministic `window-width` / `window-height`
    minimum assertion would fail.
- The pipeline report proves every stage ran by exposing non-empty or expected
  default-file, CLI, recursive, and finalization artifacts.
- Existing focused guards still pass:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_load_pipeline
  cargo test --manifest-path roastty/Cargo.toml config_load_default_files
  cargo test --manifest-path roastty/Cargo.toml config_recursive
  cargo test --manifest-path roastty/Cargo.toml config_replay
  cargo test --manifest-path roastty/Cargo.toml config_finalize
  ```

- The generated load inventory reports:
  - 18 total rows;
  - 18 `Oracle complete` rows;
  - 0 `Audit covered` rows;
  - 0 `Gap` rows;
  - 0 incomplete rows.
- `LOAD-001` is `Oracle complete`.
- CFG-221 becomes `Pass`, points to `config-load-inventory.md`, and records the
  completed counts.
- CFG-217, CFG-218, CFG-219, and CFG-220 remain byte-for-byte unchanged from
  result commit `f2b2a0063` after final Markdown formatting.
- Hygiene passes:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  PYTHONDONTWRITEBYTECODE=1 python3 \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py \
    --output issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/102-full-load-pipeline-order.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_load_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/102-full-load-pipeline-order.md \
    issues/0805-roastty-ghostty-parity/config-load-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- The initial plan claimed to prove defaults but only had concrete artifacts for
  default files, CLI, recursive files, and finalization.
- The initial CLI-before-recursive assertion proved CLI-provided `config-file`
  path discovery but did not prove recursive files beat ordinary CLI values.
- The initial finalization assertion did not name a deterministic field/effect.

Fix:

- Added an untouched pinned default assertion proving the pipeline starts from
  `Config::default()`.
- Added same-key precedence: CLI sets a scalar, the recursive file sets that
  same scalar differently, and the recursive value must win.
- Made the finalization assertion deterministic by using recursive
  `window-width` / `window-height` values below the minimum and requiring
  finalization to clamp them.

Final verdict: Approved.

## Result

**Result:** Pass

Implemented a config-layer pipeline helper and a focused end-to-end load order
test. The helper starts from `Config::default()`, then runs default files, CLI
args, recursive config files, and finalization, returning a stage report for
each step.

The pipeline test proves the pinned Ghostty order with failure-sensitive
assertions:

- an untouched field remains at its pinned default, proving the pipeline starts
  from `Config::default()`;
- a default-file `title` is overridden by CLI `title`, proving default files run
  before CLI;
- a CLI-provided `config-file` path is loaded recursively, proving CLI runs
  before recursive config files;
- the recursive file sets `title` after CLI sets `title`, and the recursive
  value wins, proving recursive config files run after ordinary CLI values;
- the recursive file sets `window-width = 3` and `window-height = 2`, and
  finalization clamps them to deterministic minimums `10` and `4`, proving
  finalization runs after recursive files.

`LOAD-001` is now `Oracle complete`. The generated CFG-221 load inventory
reports:

- 18 total load rows;
- 18 `Oracle complete` rows;
- 0 `Audit covered` rows;
- 0 `Gap` rows;
- 0 incomplete rows.

CFG-221 is now `Pass`.

Verification run:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_load_pipeline
cargo test --manifest-path roastty/Cargo.toml config_load_default_files
cargo test --manifest-path roastty/Cargo.toml config_recursive
cargo test --manifest-path roastty/Cargo.toml config_replay
cargo test --manifest-path roastty/Cargo.toml config_finalize
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
old_matrix=subprocess.check_output(['git','show','f2b2a0063:issues/0805-roastty-ghostty-parity/config-matrix.md'], text=True)
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
assert statuses['LOAD-001']=='Oracle complete', statuses['LOAD-001']
oracle=sum(s=='Oracle complete' for s in statuses.values())
incomplete=len(rows)-oracle
gaps=sum(s=='Gap' for s in statuses.values())
audit=sum(s=='Audit covered' for s in statuses.values())
assert (len(rows), oracle, audit, gaps, incomplete)==(18,18,0,0,0), (len(rows), oracle, audit, gaps, incomplete)
cfg221=next(line for line in matrix.splitlines() if line.startswith('| CFG-221 |'))
cells=[c.strip() for c in cfg221.strip('|').split('|')]
assert cells[4]=='Pass', cells[4]
assert 'config-load-inventory.md' in cfg221
assert '18 rows Oracle complete' in cfg221
assert '0 rows are not Oracle complete' in cfg221
assert '0 rows are load gaps' in cfg221
print('load_rows=18 oracle_complete=18 audit_covered=0 incomplete=0 gaps=0 cfg221=Pass load001=Oracle complete protected_cfg217_220_unchanged=true')
PY
```

The focused test filters passed. The matrix assertion printed:

```text
load_rows=18 oracle_complete=18 audit_covered=0 incomplete=0 gaps=0 cfg221=Pass load001=Oracle complete protected_cfg217_220_unchanged=true
```

## Conclusion

CFG-221 is complete. Config source precedence and repeated-file load semantics
now have row-level proofs for every pinned Ghostty load behavior plus an
end-to-end pipeline-order oracle.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

The reviewer found no required fixes before the result commit. The review
confirmed:

- the helper/test honestly covers the pinned Ghostty load order;
- assertions are failure-sensitive for default state, precedence, and
  finalization cases;
- `LOAD-001` promotion and CFG-221 `Pass` are consistent;
- CFG-217 through CFG-220 are byte-for-byte unchanged from `f2b2a0063`.

The reviewer also independently ran:

```bash
cargo test --manifest-path roastty/Cargo.toml config_load_pipeline
git diff --check
```

Both passed.
