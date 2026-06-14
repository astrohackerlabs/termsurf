# Experiment 86: Boolean diagnostic oracle

## Description

CFG-219 has a generated diagnostic inventory, and 39 remaining incomplete rows
share the same `set_bool_field` parser family. The parser inventory already
proves upstream boolean token semantics for these options, but the diagnostic
inventory correctly keeps them `Audit covered` because parser acceptance alone
does not prove config-file or CLI `ConfigDiagnostic` behavior.

This experiment will add a shared boolean diagnostic oracle that iterates every
remaining incomplete direct boolean option and proves the diagnostic behavior
that CFG-219 requires:

- config-file invalid values report `ConfigSetError::InvalidValue` with the
  offending line and key;
- CLI invalid values report `ConfigSetError::InvalidValue` with the argument
  position and key;
- invalid config-file and CLI values do not overwrite the previously valid
  value;
- bare config-file and CLI keys set the field to `true`;
- empty config-file and CLI values reset the field to the default;
- the exact upstream true tokens `1`, `t`, `T`, and `true` and false tokens `0`,
  `f`, `F`, and `false` still parse successfully.

The scope is limited to direct boolean rows that currently have
`Status = Audit covered` in `config-diagnostic-inventory.md`. It will not
promote `config-default-files`, which already has option-specific diagnostic
coverage and CLI-only semantics, and it will not promote optional bool,
compatibility bool, enum, numeric, string, path, duration, or runtime/finalize
rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add a test-only table for the 39 incomplete direct boolean config options.
  - Add a focused `config_boolean_diagnostic_family_oracle` test that uses
    existing public-in-module config APIs to verify file diagnostics, CLI
    diagnostics, state retention, bare true, empty reset, and exact upstream
    true/false tokens for every table row.
  - Keep assertions option-specific so a missing option or wrong diagnostic key
    cannot pass through a generic helper.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an explicit Experiment 86 evidence override for the 39 boolean options
    covered by the shared diagnostic oracle.
  - Keep the override list exact, and fail generation if any listed option is no
    longer a boolean parser-family row or is not present in the canonical
    diagnostic inventory.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The 39 direct boolean rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because other diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning only if the implementation discovers a reusable diagnostic
    proof rule or a mismatch in direct boolean diagnostic behavior.

## Verification

Pass criteria:

- The boolean oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_boolean_diagnostic_family_oracle
  ```

- Rust formatting is applied:

  ```bash
  cargo fmt --manifest-path roastty/Cargo.toml
  ```

- The regenerated diagnostic inventory reports:
  - `ghostty_canonical=203`;
  - `diagnostic_rows=203`;
  - no missing canonical diagnostic rows;
  - no extra diagnostic rows outside the canonical inventory;
  - `oracle_complete=161`;
  - `audit_covered=42`;
  - `gap=0`.

- A matrix assertion verifies:
  - every direct boolean row except `config-default-files` is `Oracle complete`;
  - `config-default-files` remains `Oracle complete` with its existing
    option-specific evidence;
  - exactly 161 diagnostic rows are `Oracle complete`;
  - exactly 42 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 161/42/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/86-boolean-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/86-boolean-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required finding:

- The design planned to regenerate `config-matrix.md`, but the Markdown
  formatting verification omitted `config-matrix.md`.

Optional finding:

- The accepted-token proof was underspecified because the design said
  "representative upstream true and false tokens" without naming the tokens.

Fixes:

- Added `config-matrix.md` to the Prettier write/check commands.
- Replaced "representative" token wording with the exact upstream true tokens
  `1`, `t`, `T`, and `true` and false tokens `0`, `f`, `F`, and `false`.

Final verdict: Approved.

Re-review confirmed the required finding and optional finding are resolved. The
reviewer left a nit about one remaining "representative" wording in the Changes
section; that wording was updated to "exact upstream true/false tokens."

## Result

**Result:** Pass

The shared boolean diagnostic oracle now covers the 39 direct boolean options
that were still `Audit covered` after Experiment 85. The oracle verifies every
option's exact upstream boolean tokens, bare true behavior, empty reset to the
option's default, config-file invalid-value diagnostics with line/key/error, CLI
invalid-value diagnostics with argument position/key/error, and invalid-value
state retention.

The diagnostic inventory generator now has an exact Experiment 86 override list
for those 39 options and validates that every override still maps to a canonical
boolean parser-family row. Regeneration moved the direct boolean diagnostic rows
to `Oracle complete`; `config-default-files` remains `Oracle complete` through
its existing option-specific oracle. CFG-219 remains `Gap` because 42
non-boolean diagnostic rows are still incomplete.

Verification output:

```text
test config::tests::config_boolean_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=161
audit_covered=42
gap=0
diagnostic_rows=203 boolean_rows=40 incomplete=42 cfg219=Gap
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_boolean_diagnostic_family_oracle
PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py
```

## Conclusion

Direct boolean diagnostic parity is now proven for CFG-219. Future diagnostic
experiments can use the same shape for other scalar families: exact canonical
row list, option-specific state accessor, invalid file and CLI diagnostic
assertions, state-retention checks, regenerated inventory counts, and CFG-219
remaining `Gap` until all diagnostic rows are complete.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required findings:

- Invalid config-file diagnostics only proved state retention from a prior
  `true` value, so a regression that incorrectly coerced invalid values to
  `true` would still pass.
- Invalid CLI diagnostics had the same prior-`true` retention gap.

Fixes:

- Updated the boolean diagnostic oracle to run every invalid file and CLI
  diagnostic assertion after both prior `false` and prior `true` values and to
  assert the value remains equal to the prior state.

Final verdict: Approved.

Re-review confirmed both retention gaps are fixed and reran the targeted boolean
diagnostic test successfully.
