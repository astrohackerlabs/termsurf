# Experiment 87: Integer diagnostic oracle

## Description

CFG-219 now has 42 incomplete diagnostic rows. Ten of those rows are integer
scalar options that share Roastty's scalar integer parser helpers:

- `abnormal-command-exit-runtime`
- `font-thicken-strength`
- `image-storage-limit`
- `linux-cgroup-memory-limit`
- `linux-cgroup-processes-limit`
- `scrollback-limit`
- `window-height`
- `window-position-x`
- `window-position-y`
- `window-width`

The existing integer parser-family oracle proves representative parser
semantics, but CFG-219 still needs diagnostic proof for each canonical option.
This experiment will add a shared integer diagnostic oracle that iterates the
ten remaining integer rows and proves config-file diagnostics, CLI diagnostics,
state retention after invalid values, empty reset behavior, required-value
behavior where applicable, and successful non-default parsing.

The scope is limited to the ten integer scalar rows currently marked
`Audit covered` in `config-diagnostic-inventory.md`. It will not promote float,
string, path, duration, font, command-palette, working-directory, finalization,
reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add a test-only table for the ten incomplete integer scalar config options.
  - Add `config_integer_diagnostic_family_oracle` that verifies, for every row:
    - a representative valid non-default value is accepted;
    - an empty value resets to the option's default;
    - invalid config-file values produce `ConfigSetError::InvalidValue` with the
      correct line/key/error;
    - invalid CLI values produce `ConfigSetError::InvalidValue` with the correct
      argument position/key/error;
    - invalid file and CLI values preserve the prior non-default state;
    - missing file and CLI values report `ConfigSetError::ValueRequired` for
      required integer rows and reset optional integer rows to their default
      where upstream semantics do that.
  - Keep table entries option-specific so the test proves the exact covered
    canonical keys and fields.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 87 evidence override for the ten integer scalar
    options covered by the shared diagnostic oracle.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `integer scalar`.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The ten integer rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because non-integer diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the implementation discovers a reusable diagnostic
    proof rule or a mismatch in integer diagnostic behavior.

## Verification

Pass criteria:

- The integer diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_integer_diagnostic_family_oracle
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
  - `oracle_complete=171`;
  - `audit_covered=32`;
  - `gap=0`.

- A matrix assertion verifies:
  - all ten integer scalar rows are `Oracle complete`;
  - every promoted integer row cites the Experiment 87 integer diagnostic
    oracle;
  - exactly 171 diagnostic rows are `Oracle complete`;
  - exactly 32 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 171/32/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/87-integer-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/87-integer-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Final verdict: Approved.

Findings: None.

The reviewer confirmed the README links Experiment 87 as `Designed`, the design
contains the required sections, scope is limited to the ten current
`integer scalar` / `Audit covered` diagnostic rows, CFG-219 closure is not
overclaimed, the expected 171/32/0 counts are coherent, and required hygiene
checks are present.

## Result

**Result:** Pass

The shared integer diagnostic oracle now covers the ten integer scalar options
that were still `Audit covered` after Experiment 86. The oracle verifies every
option's representative non-default parse, empty reset to the option's default,
missing-value `ValueRequired` behavior, config-file invalid-value diagnostics
with line/key/error, CLI invalid-value diagnostics with argument
position/key/error, and invalid-value state retention.

The implementation confirmed that optional integer fields use the same bare
missing-value diagnostic behavior as required integer fields: empty values reset
optional integer fields to their defaults, but missing values report
`ConfigSetError::ValueRequired`.

The diagnostic inventory generator now has an exact Experiment 87 override list
for those ten options and validates that every override still maps to a
canonical integer-scalar parser-family row. Regeneration moved the integer
diagnostic rows to `Oracle complete`. CFG-219 remains `Gap` because 32
non-integer diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_integer_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=171
audit_covered=32
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_integer_diagnostic_family_oracle
```

## Conclusion

Integer scalar diagnostic parity is now proven for CFG-219. The reusable scalar
pattern is an exact row list plus per-option formatted-state accessors, because
state retention must be checked for each destination field and optional fields
can differ from required fields only in empty-reset output, not in bare missing
value diagnostics.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Final verdict: Approved.

Findings: None.

The reviewer confirmed the diff from plan commit `046a9c3c6` touches only the
requested six files, the result commit had not been made, the integer diagnostic
oracle covers the exact ten integer rows, the generated inventory has 203 rows
with 171 `Oracle complete`, 32 `Audit covered`, and 0 `Gap`, all ten integer
rows cite Experiment 87, CFG-219 remains `Gap` with the 171/32/0 counts, the
README marks Experiment 87 `Pass`, and the experiment file records the result,
conclusion, and missing-value learning.
