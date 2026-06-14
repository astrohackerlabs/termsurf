# Experiment 91: Working directory diagnostic oracle

## Description

CFG-219 now has 10 incomplete diagnostic rows. One of those rows is
`working-directory`, the only remaining parser-family `working directory` row.
Its parser accepts the `home` and `inherit` keywords and otherwise treats
non-empty input as a path. The diagnostic surface is required-value behavior:
missing or all-whitespace values report `ConfigSetError::ValueRequired`.

This experiment will add a focused diagnostic oracle for `working-directory`
that proves accepted non-default values, empty reset behavior, required-value
diagnostics for config-file and CLI sources, source position metadata, and state
retention after diagnostics. During implementation, this experiment found that
config-file whitespace after `=` is normalized as an empty reset rather than a
diagnostic; the oracle records that source-specific behavior.

The scope is limited to `working-directory`. It will not promote path, font,
command-palette, finalization, reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add `config_working_directory_diagnostic_oracle` that verifies:
    - the `home` keyword is accepted and formatted;
    - the `inherit` keyword is accepted and formatted;
    - quoted paths are accepted, unquoted for storage, and formatted;
    - an empty value resets to the default formatted state;
    - a bare config-file key reports `ConfigSetError::ValueRequired` with the
      correct line/key/error;
    - an all-whitespace config-file value after `=` resets to the default
      formatted state without a diagnostic;
    - a missing CLI value reports `ConfigSetError::ValueRequired` with the
      correct argument position/key/error;
    - an all-whitespace CLI value reports `ConfigSetError::ValueRequired` with
      the correct argument position/key/error;
    - required-value diagnostics preserve the prior non-default formatted state.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 91 evidence override for `working-directory`.
  - Fail generation if the override is missing from the canonical inventory or
    no longer has parser family `working directory`.
  - Use missing-value wording for completed working-directory evidence instead
    of invalid-value wording.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The `working-directory` row should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because path, font, and command-palette diagnostic rows remain
    incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning noting that `working-directory` diagnostics are
    required-value diagnostics if the implementation confirms that behavior.

## Verification

Pass criteria:

- The working-directory diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_working_directory_diagnostic_oracle
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
  - `oracle_complete=194`;
  - `audit_covered=9`;
  - `gap=0`.

- A matrix assertion verifies:
  - the `working-directory` row is `Oracle complete`;
  - the `working-directory` row cites the Experiment 91 oracle;
  - the `working-directory` row uses diagnostic family
    `required-value diagnostic`;
  - the generated evidence and missing-evidence wording for `working-directory`
    does not claim invalid explicit-value coverage;
  - the generated evidence records config-file whitespace reset behavior rather
    than claiming config-file all-whitespace diagnostics;
  - exactly 194 diagnostic rows are `Oracle complete`;
  - exactly 9 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 194/9/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/91-working-directory-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/91-working-directory-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

Evidence checked by the reviewer:

- Confirmed the README links Experiment 91 as `Designed`.
- Confirmed the experiment has Description, Changes, and Verification.
- Confirmed the scope is limited to `working-directory` and keeps CFG-219 open.
- Confirmed the current inventory supports the claimed progression from 193/10/0
  to 194/9/0.
- Confirmed CFG-219 is currently still `Gap`, and the design requires it to
  remain `Gap`.
- Confirmed the generator plan covers exact override membership/family and
  missing-value wording without claiming arbitrary invalid path coverage.
- Confirmed the required hygiene checks are present.
- Confirmed `git diff --check` passed on the changed design files.

## Result

**Result:** Pass

The working-directory diagnostic oracle now covers the only remaining
parser-family `working directory` row. The oracle verifies `home`, `inherit`,
and quoted path acceptance and formatting, empty reset to the default formatted
state, direct missing/all-whitespace required-value errors, config-file bare-key
diagnostics with line/key/error, config-file whitespace-after-`=` reset
behavior, CLI missing/all-whitespace diagnostics with argument
position/key/error, and required-value state retention after diagnostics.

The diagnostic inventory generator now has an exact Experiment 91 override for
`working-directory` and validates that the override still maps to the canonical
`working directory` parser-family row. Regeneration moved `working-directory` to
`Oracle complete`. CFG-219 remains `Gap` because 9 path, font, and
command-palette diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_working_directory_diagnostic_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=194
audit_covered=9
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_working_directory_diagnostic_oracle
```

## Conclusion

Working-directory diagnostic parity is now proven for CFG-219. The useful lesson
is that required-value behavior depends on the source path: `working-directory`
as a bare config-file key produces a required-value diagnostic, but whitespace
after `=` in a config file is normalized as an empty reset. CLI missing and
all-whitespace values both report `ConfigSetError::ValueRequired`.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.
