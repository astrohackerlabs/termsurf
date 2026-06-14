# Experiment 96: Unfocused split opacity finalization

## Description

Experiment 95 left FINAL-010 audit-covered because the finalization inventory
did not yet cite a focused oracle for `unfocused-split-opacity` clamping. The
existing `split_visual_config_defaults_parse_format_and_finalize` test already
proves the pinned Ghostty finalization behavior for this row:

- default value and formatting;
- below-minimum values remain parseable before finalization;
- finalization clamps below-minimum values to `0.15`;
- above-maximum values remain parseable before finalization;
- finalization clamps above-maximum values to `1.0`;
- config-file parsed out-of-range values clamp after finalization;
- raw empty reset and diagnostic behavior remain covered.

This experiment will promote only FINAL-010 from `Audit covered` to
`Oracle complete` by updating the finalization inventory generator evidence. It
will not modify Rust code, parser behavior, formatter behavior, reload behavior,
or runtime/UI config behavior.

## Changes

- `issues/0805-roastty-ghostty-parity/config_finalization_inventory.py`
  - Change only the `unfocused split opacity clamp` row from `Audit covered` to
    `Oracle complete`.
  - Replace the missing-evidence text with exact evidence citing
    `split_visual_config_defaults_parse_format_and_finalize`.
  - Preserve the click-repeat interval defaulting and auto-update-channel
    default rows as `Audit covered`.

- `issues/0805-roastty-ghostty-parity/config-finalization-inventory.md`
  - Regenerate the inventory. Counts should move from 14/3/0 to 15/2/0.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-220 from the finalization inventory. CFG-220 should remain
    `Gap`, with 15 rows `Oracle complete`, 2 rows not `Oracle complete`, and 0
    finalization gaps.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the verification discovers a reusable
    finalization-proof rule or a concrete mismatch.

## Verification

Pass criteria:

- The focused Rust oracle passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml split_visual_config_defaults_parse_format_and_finalize
  ```

- The finalization inventory generator exits successfully and reports:

  ```text
  finalization_rows=17
  oracle_complete=15
  audit_covered=2
  gap=0
  ```

- A matrix assertion verifies:
  - FINAL-010 is `Oracle complete`;
  - FINAL-010 cites `split_visual_config_defaults_parse_format_and_finalize`;
  - FINAL-008 and FINAL-015 remain `Audit covered`;
  - exactly 17 finalization rows exist;
  - exactly 15 rows are `Oracle complete`;
  - exactly 2 rows are not `Oracle complete`;
  - exactly 0 rows are `Gap`;
  - CFG-220 remains `Gap`;
  - CFG-220 points to `config-finalization-inventory.md`;
  - CFG-220 notes the 15/2/0 generated counts.

- The generator must not disturb CFG-217, CFG-218, or CFG-219. Capture all three
  full matrix rows before running the generator and assert they are
  byte-for-byte unchanged after generation and final Markdown formatting.

- Python and Markdown hygiene pass:

  ```bash
  PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
    issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
  rm -rf issues/0805-roastty-ghostty-parity/__pycache__
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/96-unfocused-split-opacity-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/96-unfocused-split-opacity-finalization.md \
    issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that the README links Experiment 96 as `Designed`, the
experiment has the required sections, only README/design-doc changes were
present, the scope is limited to FINAL-010 while FINAL-008 and FINAL-015 remain
`Audit covered`, `split_visual_config_defaults_parse_format_and_finalize`
already covers below-minimum, above-maximum, and config-file parsed clamp
behavior, and pinned Ghostty clamps `unfocused-split-opacity` with the same
0.15..1.0 bounds.

## Result

**Result:** Pass.

FINAL-010 now uses the existing split visual config oracle as explicit
finalization evidence and is marked `Oracle complete`. FINAL-008 click-repeat
interval defaulting and FINAL-015 auto-update-channel defaulting remain
`Audit covered`.

Verification output:

```text
finalization_rows=17
oracle_complete=15
audit_covered=2
gap=0
finalization_rows=17 oracle_complete=15 incomplete=2 gaps=0 cfg220=Gap protected_cfg217_219_unchanged=true
```

The focused Rust oracle passed:

```bash
cargo test --manifest-path roastty/Cargo.toml split_visual_config_defaults_parse_format_and_finalize
```

The test passed with 1 passed, 0 failed, and 4966 filtered out.

Additional checks passed:

```bash
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/config_finalization_inventory.py
prettier --check \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/96-unfocused-split-opacity-finalization.md \
  issues/0805-roastty-ghostty-parity/config-finalization-inventory.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Conclusion

Unfocused split opacity clamping is now complete for CFG-220. The finalization
inventory has 15 `Oracle complete` rows and 2 remaining audit-covered rows:
click-repeat interval app OS defaulting and auto-update-channel release-channel
defaulting.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The reviewer verified that the result commit had not been made, the diff was
limited to the five requested issue files with no Rust code changes, only
FINAL-010 changed from the plan commit, FINAL-010 is `Oracle complete` and cites
`split_visual_config_defaults_parse_format_and_finalize`, FINAL-008 and
FINAL-015 remain `Audit covered`, the generated counts are 17 rows, 15
`Oracle complete`, 2 incomplete, and 0 `Gap`, CFG-220 remains `Gap` with 15/2/0
counts, CFG-217 through CFG-219 are unchanged from the plan commit, and the
focused Rust, Python compile, Prettier, whitespace, and table/matrix checks
passed.
