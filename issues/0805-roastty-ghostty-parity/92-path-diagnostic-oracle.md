# Experiment 92: Path diagnostic oracle

## Description

CFG-219 now has 9 incomplete diagnostic rows. Three of those rows are path
options that share Roastty's `ConfigFilePath` / `RepeatableConfigPath` parsing
surface:

- `background-image`
- `bell-audio-path`
- `config-file`

The path parser accepts required paths, optional-marker paths, quoted literal
markers, quoted optional paths, embedded NULs, and most explicit path text. Its
diagnostic surface is required-value behavior for missing values. Raw empty
values reset optional/repeatable storage, while parsed-empty paths such as `?`,
`""`, and `?""` are no-ops.

This experiment will add a shared path diagnostic oracle for those three rows
and update the diagnostic inventory so path rows are treated as required-value
diagnostics, not invalid explicit-value diagnostics.

The scope is limited to the three path rows. It will not promote font,
command-palette, finalization, reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add `config_path_diagnostic_family_oracle` that verifies:
    - `background-image` accepts required paths, optional paths, quoted literal
      optional markers, quoted optional paths, and embedded NUL paths;
    - `bell-audio-path` accepts the same optional single-path forms;
    - `config-file` accumulates repeatable required paths, optional paths,
      quoted literal optional markers, quoted optional paths, and embedded NUL
      paths;
    - raw empty values reset optional/repeatable storage to the default
      formatted state;
    - parsed-empty values (`?`, `""`, `?""`) preserve the prior formatted state;
    - bare config-file keys report `ConfigSetError::ValueRequired` with the
      correct line/key/error;
    - missing CLI values report `ConfigSetError::ValueRequired` with the correct
      argument position/key/error;
    - missing-value diagnostics preserve the prior non-default formatted state.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 92 evidence override for the three path options.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `path`.
  - Reclassify parser-family `path` diagnostic rows as
    `required-value diagnostic` / missing-value coverage instead of
    `stateful parser diagnostic`.
  - Use missing-value wording for completed path evidence instead of
    invalid-value wording.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The three path rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because font and command-palette diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning noting that path diagnostics are missing-value diagnostics if
    the implementation confirms that behavior.

## Verification

Pass criteria:

- The path diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_path_diagnostic_family_oracle
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
  - `oracle_complete=197`;
  - `audit_covered=6`;
  - `gap=0`.

- A matrix assertion verifies:
  - all three path rows are `Oracle complete`;
  - every promoted path row cites the Experiment 92 path diagnostic oracle;
  - every promoted path row uses diagnostic family `required-value diagnostic`;
  - generated path evidence and missing-evidence wording does not claim invalid
    explicit-value coverage;
  - exactly 197 diagnostic rows are `Oracle complete`;
  - exactly 6 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 197/6/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/92-path-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/92-path-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.

Reviewer summary:

- Confirmed the README links Experiment 92 as `Designed`.
- Confirmed the experiment has Description, Changes, and Verification.
- Confirmed the design is scoped to `background-image`, `bell-audio-path`, and
  `config-file`.
- Confirmed the plan treats path diagnostics as missing/required-value coverage
  rather than arbitrary invalid explicit-value coverage.
- Confirmed the verification criteria preserve CFG-219 as `Gap` with expected
  197/6/0 post-experiment counts and include the required hygiene checks.

## Result

**Result:** Pass

The shared path diagnostic oracle now covers the three parser-family `path`
rows. The oracle verifies required path, optional path, quoted literal optional
marker, quoted optional path, and embedded-NUL path acceptance; raw-empty reset
behavior; parsed-empty no-op behavior; bare missing-value errors; config-file
missing-value diagnostics with line/key/error; CLI missing-value diagnostics
with argument position/key/error; and missing-value state retention.

The diagnostic inventory generator now has an exact Experiment 92 override list
for `background-image`, `bell-audio-path`, and `config-file`, validates that
each override still maps to parser family `path`, and reclassifies path rows as
`required-value diagnostic`. Regeneration moved the three path diagnostic rows
to `Oracle complete`. CFG-219 remains `Gap` because 6 font and command-palette
diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_path_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=197
audit_covered=6
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_path_diagnostic_family_oracle
```

## Conclusion

Path diagnostic parity is now proven for CFG-219. The useful lesson is that path
diagnostics are missing-value diagnostics: explicit path payloads are broadly
accepted, including optional markers, quoted marker forms, and NUL-containing
paths. CLI processing can expand existing relative path state, so state
retention checks should use absolute setup paths when the diagnostic under test
is a missing CLI value.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Findings: None.
