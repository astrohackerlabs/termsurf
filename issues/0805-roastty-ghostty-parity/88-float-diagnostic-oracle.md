# Experiment 88: Float diagnostic oracle

## Description

CFG-219 now has 32 incomplete diagnostic rows. Nine of those rows are float
scalar options that share Roastty's `set_f32_field` and `set_f64_field` parser
helpers:

- `background-image-opacity`
- `background-opacity`
- `bell-audio-volume`
- `cursor-opacity`
- `faint-opacity`
- `font-size`
- `minimum-contrast`
- `quick-terminal-animation-duration`
- `unfocused-split-opacity`

The existing float parser-family oracle proves representative Zig float parsing
for selected fields, but CFG-219 still needs diagnostic proof for every
canonical float option. This experiment will add a shared float diagnostic
oracle that iterates the nine remaining float rows and proves config-file
diagnostics, CLI diagnostics, state retention after invalid values, empty reset
behavior, missing-value behavior, and successful non-default parsing.

The scope is limited to the nine float scalar rows currently marked
`Audit covered` in `config-diagnostic-inventory.md`. It will not promote string,
path, duration, font, command-palette, working-directory, finalization, reload,
or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add a test-only table for the nine incomplete float scalar config options.
  - Add `config_float_diagnostic_family_oracle` that verifies, for every row:
    - a representative valid non-default value is accepted and formatted;
    - an empty value resets to the option's default;
    - missing file and CLI values report `ConfigSetError::ValueRequired`;
    - invalid config-file values produce `ConfigSetError::InvalidValue` with the
      correct line/key/error;
    - invalid CLI values produce `ConfigSetError::InvalidValue` with the correct
      argument position/key/error;
    - invalid file and CLI values preserve the prior non-default formatted
      state.
  - Use formatted-state accessors so both `f32` and `f64` rows are checked
    through the same user-visible config output surface.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 88 evidence override for the nine float scalar
    options covered by the shared diagnostic oracle.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `float scalar`.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The nine float rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because non-float diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the implementation discovers a reusable diagnostic
    proof rule or a mismatch in float diagnostic behavior.

## Verification

Pass criteria:

- The float diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_float_diagnostic_family_oracle
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
  - `oracle_complete=180`;
  - `audit_covered=23`;
  - `gap=0`.

- A matrix assertion verifies:
  - all nine float scalar rows are `Oracle complete`;
  - every promoted float row cites the Experiment 88 float diagnostic oracle;
  - exactly 180 diagnostic rows are `Oracle complete`;
  - exactly 23 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 180/23/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/88-float-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/88-float-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Final verdict: Approved.

Findings: None.

The reviewer confirmed the README links Experiment 88 as `Designed`, the design
contains the required sections, scope is limited to the nine CFG-219 float
scalar diagnostic rows, CFG-219 remains explicitly `Gap`, the expected 180/23/0
counts are coherent, and the required cargo fmt, targeted cargo test, Prettier,
and `git diff --check` hygiene checks are present.

## Result

**Result:** Pass

The shared float diagnostic oracle now covers the nine float scalar options that
were still `Audit covered` after Experiment 87. The oracle verifies every
option's representative non-default parse and formatted output, empty reset to
the option's default, missing-value `ValueRequired` behavior, config-file
invalid-value diagnostics with line/key/error, CLI invalid-value diagnostics
with argument position/key/error, and invalid-value state retention.

The diagnostic inventory generator now has an exact Experiment 88 override list
for those nine options and validates that every override still maps to a
canonical float-scalar parser-family row. Regeneration moved the float
diagnostic rows to `Oracle complete`. CFG-219 remains `Gap` because 23 non-float
diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_float_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=180
audit_covered=23
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_float_diagnostic_family_oracle
```

## Conclusion

Float scalar diagnostic parity is now proven for CFG-219. The integer scalar
pattern transfers cleanly to float scalars when the oracle uses finite
non-default values and formatted output as the state surface, keeping parser
edge cases in the existing float parser-family oracle.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Final verdict: Approved.

Findings: None.

The reviewer confirmed the diff from plan commit `d4829990e` is scoped to
Experiment 88 artifacts and the Rust oracle, the result commit had not been
made, the targeted float diagnostic test and format checks passed, the generated
inventory has 203 rows with 180 `Oracle complete`, 23 `Audit covered`, and 0
`Gap`, exactly nine float rows cite Experiment 88 evidence, CFG-217 and CFG-218
are unchanged from the plan commit, and CFG-219 remains `Gap` with the 180/23/0
counts.
