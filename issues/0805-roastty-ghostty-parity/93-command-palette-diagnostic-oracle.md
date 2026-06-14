# Experiment 93: Command palette diagnostic oracle

## Description

CFG-219 now has 6 incomplete diagnostic rows. One of those rows is
`command-palette-entry`, the only remaining parser-family `command palette` row.
Its parser is structured: empty or missing values restore the default command
list, `clear` empties it, and malformed auto-struct entries report
`ConfigSetError::InvalidValue`.

This experiment will add a focused diagnostic oracle for `command-palette-entry`
that proves invalid structured-entry diagnostics, source position metadata,
continued config-file loading after an invalid entry, CLI argument position
metadata, and state retention after invalid CLI diagnostics.

The scope is limited to `command-palette-entry`. It will not promote font,
finalization, reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add `config_command_palette_diagnostic_oracle` that verifies:
    - `clear` empties the command list;
    - valid structured entries append canonicalized actions and format
      predictably;
    - empty and missing direct values restore the default command list;
    - malformed direct values report `ConfigSetError::InvalidValue`;
    - config-file invalid values report `ConfigSetError::InvalidValue` with the
      correct line/key/error;
    - config-file loading continues after an invalid command-palette entry;
    - CLI invalid values report `ConfigSetError::InvalidValue` with the correct
      argument position/key/error;
    - invalid CLI diagnostics preserve the prior command-palette state.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 93 evidence override for `command-palette-entry`.
  - Fail generation if the override is missing from the canonical inventory or
    no longer has parser family `command palette`.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The `command-palette-entry` row should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because font diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning noting the command-palette diagnostic shape if the
    implementation confirms it.

## Verification

Pass criteria:

- The command-palette diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_command_palette_diagnostic_oracle
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
  - `oracle_complete=198`;
  - `audit_covered=5`;
  - `gap=0`.

- A matrix assertion verifies:
  - the `command-palette-entry` row is `Oracle complete`;
  - the row cites the Experiment 93 command-palette diagnostic oracle;
  - the row keeps diagnostic family `structured value diagnostic`;
  - exactly 198 diagnostic rows are `Oracle complete`;
  - exactly 5 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 198/5/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/93-command-palette-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/93-command-palette-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.

Reviewer summary:

- Confirmed the README links Experiment 93 as `Designed`.
- Confirmed the experiment has Description, Changes, and Verification.
- Confirmed the design is scoped to `command-palette-entry`.
- Confirmed CFG-219 remains open and the expected 197/6/0 to 198/5/0 diagnostic
  inventory progression matches the current files.
- Confirmed the generator and hygiene checks are specific enough for the claimed
  progress.

## Result

**Result:** Pass

The command-palette diagnostic oracle now covers the only parser-family
`command palette` row. The oracle verifies `clear`, valid structured entries,
action canonicalization, empty direct reset, missing direct reset, malformed
direct values, config-file invalid-value diagnostics with line/key/error,
continued config-file loading after invalid entries, CLI invalid-value
diagnostics with argument position/key/error, and invalid-value state retention
around prior and later valid CLI entries.

The diagnostic inventory generator now has an exact Experiment 93 override for
`command-palette-entry` and validates that the override still maps to parser
family `command palette`. Regeneration moved `command-palette-entry` to
`Oracle complete`. CFG-219 remains `Gap` because 5 font diagnostic rows are
still incomplete.

Verification output:

```text
test config::tests::config_command_palette_diagnostic_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=198
audit_covered=5
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_command_palette_diagnostic_oracle
```

## Conclusion

Command-palette diagnostic parity is now proven for CFG-219. The useful lesson
is that `command-palette-entry` is an invalid-value diagnostic row for malformed
structured entries, while empty and missing direct values restore defaults
instead of reporting required-value diagnostics.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.
