# Experiment 89: String diagnostic oracle

## Description

CFG-219 now has 23 incomplete diagnostic rows. Nine of those rows are string
scalar options that share Roastty's `parse_string_field` helper:

- `class`
- `enquiry-response`
- `gtk-quick-terminal-namespace`
- `language`
- `macos-custom-icon`
- `term`
- `title`
- `window-title-font-family`
- `x11-instance-name`

Unlike boolean, integer, and float scalar helpers, the string parser has no
invalid explicit value: `parse_string_field(Some(_))` accepts the provided text,
including empty strings and NUL bytes. Its diagnostic surface is a missing
value, which maps to `ConfigSetError::ValueRequired`. This experiment will add a
shared string diagnostic oracle that iterates the nine remaining string rows and
proves explicit string acceptance, empty reset behavior, missing-value
diagnostics, and state retention after missing-value diagnostics for both
config-file and CLI sources.

The scope is limited to the nine string rows currently marked `Audit covered` in
`config-diagnostic-inventory.md`. It will not promote font, duration, path,
command-palette, working-directory, finalization, reload, or runtime/UI rows.

## Changes

- `roastty/src/config/mod.rs`
  - Add a test-only table for the nine incomplete string config options.
  - Add `config_string_diagnostic_family_oracle` that verifies, for every row:
    - a representative non-empty string is accepted and formatted;
    - an explicit NUL-containing string is accepted, proving there is no invalid
      explicit string payload for this helper;
    - an empty value resets to the option's default;
    - missing config-file values report `ConfigSetError::ValueRequired` with the
      correct line/key/error;
    - missing CLI values report `ConfigSetError::ValueRequired` with the correct
      argument position/key/error;
    - missing-value diagnostics preserve the prior non-default formatted state.
  - Use formatted-state accessors so required and optional string rows are
    checked through the same user-visible config output surface.

- `issues/0805-roastty-ghostty-parity/config_diagnostic_inventory.py`
  - Add an exact Experiment 89 evidence override for the nine string options
    covered by the shared diagnostic oracle.
  - Fail generation if any listed override is missing from the canonical
    inventory or no longer has parser family `string`.
  - Reclassify parser-family `string` diagnostic rows as
    `required-value diagnostic` / missing-value coverage instead of
    `scalar invalid-value diagnostic`, because explicit string values are all
    accepted by this helper.
  - Ensure regenerated string evidence and missing-evidence wording does not
    claim invalid explicit-value coverage.

- `issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md`
  - Regenerate the inventory. The nine string rows should move from
    `Audit covered` to `Oracle complete`.

- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-219 from the diagnostic inventory. CFG-219 should remain
    `Gap`, because non-string diagnostic rows remain incomplete.

- `issues/0805-roastty-ghostty-parity/README.md`
  - Link this experiment as `Designed`.
  - Add a learning noting that string diagnostics are missing-value diagnostics,
    not invalid explicit-value diagnostics, if the implementation confirms that
    behavior.

## Verification

Pass criteria:

- The string diagnostic oracle test passes:

  ```bash
  cargo test --manifest-path roastty/Cargo.toml config_string_diagnostic_family_oracle
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
  - `oracle_complete=189`;
  - `audit_covered=14`;
  - `gap=0`.

- A matrix assertion verifies:
  - all nine string rows are `Oracle complete`;
  - every promoted string row cites the Experiment 89 string diagnostic oracle;
  - every string row uses the required-value/missing-value diagnostic family and
    does not claim invalid explicit-value coverage;
  - exactly 189 diagnostic rows are `Oracle complete`;
  - exactly 14 diagnostic rows remain incomplete;
  - CFG-219 remains `Gap`;
  - CFG-219 points to `config-diagnostic-inventory.md`;
  - CFG-219 notes the 189/14/0 generated counts.

- The generator must not disturb CFG-217 or CFG-218. Capture both full matrix
  rows before running the generator and assert they are byte-for-byte unchanged
  after generation and final Markdown formatting.

- Markdown formatting and whitespace checks pass:

  ```bash
  prettier --write --prose-wrap always --print-width 80 \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/89-string-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  prettier --check \
    issues/0805-roastty-ghostty-parity/README.md \
    issues/0805-roastty-ghostty-parity/89-string-diagnostic-oracle.md \
    issues/0805-roastty-ghostty-parity/config-diagnostic-inventory.md \
    issues/0805-roastty-ghostty-parity/config-matrix.md
  git diff --check
  ```

## Design Review

Adversarial reviewer: Codex subagent with fresh context.

Initial verdict: Changes required.

Required finding:

- The design correctly stated that explicit string values are all accepted and
  only missing values produce diagnostics, but it did not plan to update the
  diagnostic inventory family/wording, which still labeled string rows as
  invalid-value diagnostics.

Fixes:

- Added a generator change to reclassify string rows as
  `required-value diagnostic` / missing-value coverage instead of
  `scalar invalid-value diagnostic`.
- Added a requirement that regenerated string evidence and missing-evidence
  wording must not claim invalid explicit-value coverage.
- Added a matrix assertion for the string diagnostic family/wording.

Final verdict: Approved.

Re-review confirmed the required finding is resolved.

## Result

**Result:** Pass

The shared string diagnostic oracle now covers the nine string options that were
still `Audit covered` after Experiment 88. The oracle verifies every option's
representative non-empty string acceptance and formatted output, NUL-containing
string acceptance, empty reset to the option's default, missing-value
config-file diagnostics with line/key/error, missing-value CLI diagnostics with
argument position/key/error, and missing-value state retention.

The diagnostic inventory generator now has an exact Experiment 89 override list
for those nine options, validates that every override still maps to a canonical
string parser-family row, and reclassifies parser-family `string` rows as
`required-value diagnostic`. Regeneration moved the string diagnostic rows to
`Oracle complete`. CFG-219 remains `Gap` because 14 non-string diagnostic rows
are still incomplete.

Verification output:

```text
test config::tests::config_string_diagnostic_family_oracle ... ok
ghostty_canonical=203
diagnostic_rows=203
missing_canonical_diagnostic_rows=0
extra_diagnostic_rows=0
oracle_complete=189
audit_covered=14
gap=0
```

Additional checks passed:

```bash
cargo fmt --manifest-path roastty/Cargo.toml
cargo test --manifest-path roastty/Cargo.toml config_string_diagnostic_family_oracle
```

## Conclusion

String diagnostic parity is now proven for CFG-219. The useful lesson is that
string diagnostics are missing-value diagnostics, not invalid explicit-value
diagnostics: explicit string payloads, including NUL-containing strings, are
accepted and copied, while missing values produce
`ConfigSetError::ValueRequired` and must preserve the prior configured state.

## Completion Review

Adversarial reviewer: Codex subagent with fresh context.

Verdict: Approved.

Required findings: None.
