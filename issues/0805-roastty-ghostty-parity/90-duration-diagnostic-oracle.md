# Experiment 90: Duration diagnostic oracle

## Description

CFG-219 now has 14 incomplete diagnostic rows. Four of those rows are duration
options that share Roastty's `Duration::parse_cli` helper:

- `notify-on-command-finish-after`
- `quit-after-last-window-closed-delay`
- `resize-overlay-duration`
- `undo-timeout`

Experiment 49 already proved duration parser/formatter behavior, but CFG-219
requires diagnostic-specific proof: missing values, invalid values, source
position metadata, and state retention after errors. This experiment will add a
shared duration diagnostic oracle for those four options and then promote only
those rows in the diagnostic inventory.

The scope is limited to duration diagnostics. It will not promote path, font,
command-palette, working-directory, finalization, reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add a test-only table for the four incomplete duration config options.
  - Add `config_duration_diagnostic_family_oracle` that verifies, for every row:
    - a representative non-default duration is accepted and formatted;
    - an empty value resets to the option's default formatted state;
    - missing config-file values report `ConfigSetError::ValueRequired` with the
      correct line/key/error;
    - invalid config-file values report `ConfigSetError::InvalidValue` with the
      correct line/key/error;
    - missing CLI values report `ConfigSetError::ValueRequired` with the correct
      argument position/key/error;
    - invalid CLI values report `ConfigSetError::InvalidValue` with the correct
      argument position/key/error;
    - missing-value and invalid-value diagnostics preserve the prior non-default
      formatted state.
  - Include a zero-duration case where relevant so the oracle does not confuse
    zero formatting with empty-reset semantics.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 90 evidence override for the four duration options.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `duration`.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The four duration rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because non-duration diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning for any duration-specific diagnostic behavior that the
    implementation confirms and future experiments should preserve.

## Verification

Pass criteria:

- The duration diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_duration_diagnostic_family_oracle
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
  - `oracle_complete=193`;
  - `audit_covered=10`;
  - `gap=0`.

- A matrix assertion verifies:
  - all four duration rows are `Oracle complete`;
  - every promoted duration row cites the Experiment 90 duration diagnostic
    oracle;
  - every promoted duration row keeps diagnostic family
    `duration invalid-value diagnostic`;
  - exactly 193 diagnostic rows are `Oracle complete`;
  - exactly 10 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 193/10/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/90-duration-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/90-duration-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

Read-only checks performed by the reviewer:

- Confirmed the README links Experiment 90 as `Designed`.
- Confirmed the experiment has Description, Changes, and Verification.
- Confirmed the current diagnostic inventory has 203 rows, 189
  `Oracle complete`, 14 `Audit covered`, and 0 `Gap`.
- Confirmed the four scoped duration rows are the only parser-family `duration`
  rows and are currently `Audit covered`.
- Confirmed CFG-219 is still `Gap`.
- Confirmed `prettier --check` passed for the README and this experiment.
- Confirmed `git diff --check` passed for the reviewed diff.

## Result

**Result:** Pass

The shared duration diagnostic oracle now covers the four duration options that
were still `Audit covered` after Experiment 89. The oracle verifies every
option's representative non-default duration acceptance and formatted output,
zero-duration formatting plus internal zero state, empty reset to the option's
default, missing-value config-file diagnostics with line/key/error,
invalid-value config-file diagnostics with line/key/error, missing-value CLI
diagnostics with argument position/key/error, invalid-value CLI diagnostics with
argument position/key/error, and diagnostic state retention.

The diagnostic inventory generator now has an exact Experiment 90 override list
for those four options and validates that every override still maps to a
canonical duration parser-family row. Regeneration moved the duration diagnostic
rows to `Oracle complete`. CFG-219 remains `Gap` because 10 non-duration
diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_duration_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=193
audit_covered=10
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_duration_diagnostic_family_oracle
```

## Conclusion

Duration diagnostic parity is now proven for CFG-219. The useful lesson is that
duration zero values are formatted as empty values, so tests must assert the
internal zero state separately from formatted output when distinguishing zero
from empty-reset behavior.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.
