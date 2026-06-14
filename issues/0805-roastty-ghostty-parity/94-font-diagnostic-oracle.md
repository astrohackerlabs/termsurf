# Experiment 94: Font diagnostic oracle

## Description

CFG-219 now has 5 incomplete diagnostic rows. All five are parser-family `font`
rows backed by Roastty's `RepeatableString` parser:

- `font-family`
- `font-family-bold`
- `font-family-italic`
- `font-family-bold-italic`
- `font-feature`

`RepeatableString` accepts explicit string payloads, including NUL-containing
strings. Its diagnostic surface is required-value behavior for missing values.
Raw empty values reset the list, and non-empty values append.

This experiment will add a shared font diagnostic oracle for those five rows and
update the diagnostic inventory so these font rows are treated as required-value
diagnostics, not invalid explicit-value diagnostics. If this passes, CFG-219
should move from `Gap` to `Pass`.

The scope is limited to the five remaining font rows. It will not modify
finalization, reload, runtime/UI, or non-diagnostic config facets.

## Changes

- `roastty/src/config/mod.rs`
  - Add `config_font_diagnostic_family_oracle` that verifies, for every row:
    - representative explicit values append and format in order;
    - NUL-containing explicit values append and format, proving there is no
      invalid explicit string payload for this helper;
    - raw empty values reset to the default formatted state;
    - direct missing values report `ConfigSetError::ValueRequired`;
    - bare config-file keys report `ConfigSetError::ValueRequired` with the
      correct line/key/error;
    - missing CLI values report `ConfigSetError::ValueRequired` with the correct
      argument position/key/error;
    - missing-value diagnostics preserve the prior non-default formatted state.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 94 evidence override for the five font options.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `font`.
  - Reclassify only these five `RepeatableString` font diagnostic rows as
    `required-value diagnostic` / missing-value coverage. Do not reclassify
    other parser-family `font` rows, such as metric modifier or font-variation
    rows whose existing oracles prove invalid-value diagnostics.
  - Use missing-value wording for completed font evidence instead of
    invalid-value wording.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The five font rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should move to
    `Pass`, because every diagnostic inventory row should now be
    `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning noting that these font diagnostics are missing-value
    diagnostics if the implementation confirms that behavior.

## Verification

Pass criteria:

- The font diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_font_diagnostic_family_oracle
  ```

- Rust formatting is applied and checked:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  cargo fmt --manifest-path roastty/Cargo.toml -- --check
  ```

- The regenerated diagnostic inventory reports:
  - `ghostty_canonical=203`;
  - `diagnostic_rows=203`;
  - no missing canonical diagnostic rows;
  - no extra diagnostic rows outside the canonical inventory;
  - `oracle_complete=203`;
  - `audit_covered=0`;
  - `gap=0`.

- A matrix assertion verifies:
  - all five font rows are `Oracle complete`;
  - every promoted font row cites the Experiment 94 font diagnostic oracle;
  - every promoted font row uses diagnostic family `required-value diagnostic`;
  - already-complete non-Experiment-94 parser-family `font` rows keep their
    existing diagnostic families and evidence;
  - generated font evidence and missing-evidence wording does not claim invalid
    explicit-value coverage;
  - exactly 203 diagnostic rows are `Oracle complete`;
  - exactly 0 diagnostic rows remain incomplete;
  - CFG-219 moves to `Pass`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 203/0/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/94-font-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/94-font-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

Optional findings: None.

Nit findings: None.

## Design Amendment Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

The amendment narrows the generator plan so only the five remaining
`RepeatableString` font rows are reclassified as `required-value diagnostic`.
The reviewer confirmed this preserves already-complete parser-family `font` rows
whose existing oracles prove invalid-value or stateful diagnostics.

## Result

**Result:** Pass.

The shared `config_font_diagnostic_family_oracle` now covers the five remaining
font diagnostic rows:

- `font-family`
- `font-family-bold`
- `font-family-italic`
- `font-family-bold-italic`
- `font-feature`

For each row, the oracle verifies explicit values append and format in order,
NUL-containing explicit values are accepted and formatted, raw empty values
reset to the default formatted state, direct missing values report
`ConfigSetError::ValueRequired`, config-file bare keys report `ConfigDiagnostic`
with the expected line/key/error, CLI missing values report the expected
argument position/key/error, and missing-value diagnostics preserve the prior
non-default formatted state.

The diagnostic inventory generator now treats only those five exact
parser-family `font` rows as `required-value diagnostic` rows with Experiment 94
evidence. Other parser-family `font` rows keep their prior diagnostic families
and evidence.

Verification performed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_font_diagnostic_family_oracle
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile \
  issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py
PYTHONDONTWRITEBYTECODE=1 python3 \
  issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py \
  --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md \
  --parser-inventory issues/0805-roastty-ghostty-parity/config-parser-inventory.md \
  --roastty roastty/src/config/mod.rs \
  --output issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
  --matrix issues/0805-roastty-ghostty-parity/config-matrix.md
```

The targeted Rust test passed with 1 passed, 0 failed, and 4966 filtered out.
The regenerated inventory reported:

```text
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=203
audit_covered=0
gap=0
```

The matrix assertion confirmed that all five Experiment 94 font rows are
`Oracle complete`, use diagnostic family `required-value diagnostic`, cite
Experiment 94 evidence, and avoid invalid explicit-value wording. It also
confirmed non-target parser-family `font` rows preserved their pre-existing
diagnostic family and evidence, CFG-219 moved to `Pass` with the 203/0/0 counts,
and CFG-217 plus CFG-218 were unchanged from the reviewed plan commit.

## Conclusion

Font `RepeatableString` diagnostics are missing-value diagnostics, not invalid
explicit-value diagnostics. CFG-219 now has complete oracle coverage for all 203
canonical diagnostic rows and no remaining diagnostic inventory gaps.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

Optional findings: None.

Nit findings: None.

The reviewer independently confirmed that the result diff was still uncommitted,
Rust formatting passed, the targeted font diagnostic oracle test passed, the
matrix assertions passed for 203 `Oracle complete` rows and exact five-row font
promotion, CFG-217 and CFG-218 were unchanged, `git diff --check` passed, and
Prettier checks passed for the changed Markdown files.
