# Experiment 95: Finalization facet inventory

## Description

CFG-220 is the next unresolved config facet. It is currently too broad to close
directly because Ghostty's pinned `Config.finalize` performs multiple distinct
post-parse behaviors:

- theme loading and light/dark theme conditional behavior;
- font-family inheritance into bold, italic, and bold-italic families;
- empty `term` fallback;
- working-directory, command, home, and probable-CLI defaults;
- GTK single-instance detection;
- click-repeat interval defaulting;
- mouse scroll, split opacity, contrast, window size, and faint opacity clamps;
- `link-url` pruning of the default URL matcher;
- quit-after-last-window delay warnings;
- auto-update channel defaulting;
- key-remap finalization.

Roastty already has focused finalization tests for many of these behaviors, but
Issue 805 does not yet have a row-level inventory that distinguishes proven
finalization oracles from remaining finalization gaps. This experiment will
build that inventory before attempting to close CFG-220.

The expected outcome is a generated finalization inventory and matrix
consistency guard. CFG-220 must remain `Gap` unless every finalization row is
`Oracle complete`.

## Changes

- `issues/0805-roastty-ghostty-parity/config_finalization_inventory.py`
  - Add a bounded inventory generator for CFG-220.
  - Encode the pinned Ghostty finalization operations from
    `vendor/ghostty/src/config/Config.zig::finalize` as explicit rows.
  - For each row, record:
    - the finalization behavior;
    - the pinned Ghostty source reference;
    - the Roastty implementation reference;
    - the current coverage status;
    - evidence from existing Roastty tests or issue artifacts;
    - the missing proof required before the row can become `Oracle complete`.
  - Mark rows `Oracle complete` only when there is focused evidence for the
    finalized value or report behavior, including relevant platform/context
    inputs.
  - Mark rows `Audit covered` when the behavior is identified and appears
    implemented but lacks sufficient oracle coverage.
  - Mark rows `Gap` when the generator cannot identify a Roastty implementation
    or the behavior appears materially unimplemented.
  - Update CFG-220 in `config-matrix.md` from generated row counts while leaving
    CFG-217, CFG-218, and CFG-219 unchanged.

- `issues/0805-roastty-ghostty-parity/config-finalization-inventory.md`
  - Record generated finalization rows, coverage counts, evidence, and missing
    proof.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Update CFG-220 to point at `config-finalization-inventory.md`.
  - Keep CFG-220 as `Gap` unless every finalization row is `Oracle complete`.
  - Include generated counts in the CFG-220 note.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the audit discovers a reusable finalization-proof
    rule or concrete mismatch.

## Verification

Pass criteria:

- The finalization inventory generator exits successfully and reports:
  - a nonzero finalization row count;
  - no duplicate row IDs;
  - no duplicate finalization behavior names;
  - coverage counts for `Oracle complete`, `Audit covered`, and `Gap`.
- Every generated finalization row names:
  - the behavior;
  - the pinned Ghostty source reference;
  - the Roastty implementation reference or missing implementation;
  - current coverage status;
  - concrete evidence or concrete missing evidence.
- A matrix assertion verifies that CFG-220 is internally consistent:
  - if every finalization inventory row is `Oracle complete`, CFG-220 may be
    `Pass`;
  - if any finalization row is `Audit covered` or `Gap`, CFG-220 remains `Gap`;
  - CFG-220 points to `config-finalization-inventory.md`;
  - CFG-220 notes the current `Oracle complete`, incomplete, and gap counts.
- The generator must not disturb CFG-217, CFG-218, or CFG-219. Capture all three
  full matrix rows before running the generator and assert they are
  byte-for-byte unchanged after generation and final Markdown formatting.
- Existing focused finalization tests referenced by `Oracle complete` rows pass
  with the narrowest relevant `cargo test` filters.
- Python and Markdown hygiene pass:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/95-finalization-facet-inventory.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/95-finalization-facet-inventory.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
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
]
assert len(protected) == 3, protected
Path('/tmp/issue805-exp95-cfg217-219-before.txt').write_text(
    '\n'.join(protected) + '\n'
)
PY
PYTHONDONTWRITEBYTECODE=1 python3 \
  issues/0805-roastty-ghostty-parity/config_finalization_inventory.py \
  --output issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
PYTHONDONTWRITEBYTECODE=1 python3 - <<'PY'
from pathlib import Path

issue = Path('issues/0805-roastty-ghostty-parity')
matrix = (issue / 'config-matrix.md').read_text()
protected_before = Path('/tmp/issue805-exp95-cfg217-219-before.txt').read_text()
protected_after = [
    line for line in matrix.splitlines()
    if line.startswith('| CFG-217 |')
    or line.startswith('| CFG-218 |')
    or line.startswith('| CFG-219 |')
]
assert protected_before == '\n'.join(protected_after) + '\n'

rows = []
for line in (issue / 'config-finalization-inventory.md').read_text().splitlines():
    if line.startswith('| FINAL-'):
        rows.append([cell.strip() for cell in line.strip('|').split('|')])
assert rows, 'expected finalization rows'
ids = [row[0] for row in rows]
behaviors = [row[1] for row in rows]
assert len(ids) == len(set(ids)), ids
assert len(behaviors) == len(set(behaviors)), behaviors
statuses = [row[5] for row in rows]
oracle_complete = sum(status == 'Oracle complete' for status in statuses)
incomplete = len(rows) - oracle_complete
gap_count = sum(status == 'Gap' for status in statuses)

cfg220 = next(line for line in matrix.splitlines() if line.startswith('| CFG-220 |'))
cfg220_cells = [cell.strip() for cell in cfg220.strip('|').split('|')]
assert 'config-finalization-inventory.md' in cfg220
assert (incomplete == 0 and cfg220_cells[4] == 'Pass') or (
    incomplete > 0 and cfg220_cells[4] == 'Gap'
)
assert f'{oracle_complete} rows Oracle complete' in cfg220
assert f'{incomplete} rows are not Oracle complete' in cfg220
assert f'{gap_count} rows are finalization gaps' in cfg220
print(
    f'finalization_rows={len(rows)} '
    f'incomplete={incomplete} gaps={gap_count} cfg220={cfg220_cells[4]}'
)
PY
```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that the README links Experiment 95 as `Designed`, the
experiment has the required sections, the design keeps CFG-220 as `Gap` unless
every finalization row is `Oracle complete`, the listed finalization facets
match the pinned Ghostty `Config.finalize` surface, and no implementation had
started beyond the README link and design document.

## Result

**Result:** Pass.

The finalization inventory generator now emits 17 explicit rows for the pinned
Ghostty `Config.finalize` surface and updates CFG-220 from the generated counts.
The generated inventory found 14 rows with existing oracle-grade finalization
evidence, 3 audit-covered rows that still need focused proof, and 0 structural
finalization gaps.

CFG-220 remains `Gap`, as intended, because not every finalization row is
`Oracle complete`.

Verification output:

```text
finalization_rows=17
oracle_complete=14
audit_covered=3
gap=0
finalization_rows=17 incomplete=3 gaps=0 cfg220=Gap
```

The remaining audit-covered rows are:

- click-repeat interval app OS defaulting;
- unfocused split opacity clamping;
- auto-update-channel release-channel defaulting.

Focused finalization test filters passed:

```bash
cargo test --manifest-path roastty/Cargo.toml config_theme_loading
cargo test --manifest-path roastty/Cargo.toml config_font_family_finalize_inherits_regular_family
cargo test --manifest-path roastty/Cargo.toml config_finalize_scalar_tail
cargo test --manifest-path roastty/Cargo.toml config_working_directory_finalize
cargo test --manifest-path roastty/Cargo.toml config_command_home_finalize
cargo test --manifest-path roastty/Cargo.toml working_directory_finalize
cargo test --manifest-path roastty/Cargo.toml config_gtk_single_instance_finalize
cargo test --manifest-path roastty/Cargo.toml mouse_behavior_finalize_resolves_and_clamps
cargo test --manifest-path roastty/Cargo.toml window_size_step_config_parse_format_reset_finalize_and_diagnose
cargo test --manifest-path roastty/Cargo.toml config_link_url_finalize
cargo test --manifest-path roastty/Cargo.toml config_quit_delay_finalize_warning
cargo test --manifest-path roastty/Cargo.toml key_remap_config_parser_family_oracle
```

Additional checks passed:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
prettier --check \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/95-finalization-facet-inventory.md \
  issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Conclusion

CFG-220 is now decomposed into a durable finalization inventory. The next
finalization experiment should close one of the three remaining audit-covered
rows with focused proof before CFG-220 can pass.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that the result commit had not been made, the scope was
limited to the expected issue docs, matrix row, generator, and generated
inventory, the inventory has 17 rows with 14 `Oracle complete`, 3
`Audit covered`, and 0 `Gap`, the audit-covered rows are exactly click-repeat
interval OS defaulting, unfocused split opacity clamp, and auto-update-channel
default, CFG-220 remains `Gap`, CFG-217 through CFG-219 are unchanged from the
plan commit, and the formatting, whitespace, matrix, and targeted Rust checks
passed.
