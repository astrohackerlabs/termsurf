# Experiment 58: Command palette formatter oracle

## Description

Experiment 57 promoted the canonical no-output `link` formatter row and left 125
formatter rows as `Audit covered`. The next compact formatter family is
`command palette`, currently one row:

- `command-palette-entry`.

Roastty already has focused command-palette parse/format tests that prove
default entries, clear output, custom entries, quoted comma values, shorthand
actions, reset behavior, invalid-value diagnostics, and exact formatted output.
This experiment will connect that existing formatter oracle to the formatter
inventory and promote only the `command palette` formatter row.

CFG-218 should remain `Gap` because many formatter families still lack
non-default formatter oracles.

## Changes

- `issues/0805-roastty-ghostty-parity/config_formatter_inventory.py`
  - Detect the existing command-palette formatter oracle test.
  - Promote only formatter rows whose family is `command palette`.
  - Keep Experiment 58 as the CFG-218 owner when this oracle is present.
- `issues/0805-roastty-ghostty-parity/config-formatter-inventory.md`
  - Regenerate the formatter inventory.
  - Expected counts after implementation: 79 `Oracle complete` rows and 124
    `Audit covered` rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-218. It should remain `Gap` and report the new promotion
    counts.

No new Rust behavior should be necessary unless verification finds that the
existing command-palette tests do not actually prove the formatter row. If that
happens, add the missing focused assertions to `roastty/src/config/mod.rs`.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml command_palette_entry_config_parse_format_reset_and_diagnose`
  passes.
- `cargo test --manifest-path roastty/Cargo.toml command_palette_config_parser_family_oracle`
  passes.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle`
  still passes.
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_formatter_inventory.py --upstream vendor/ghostty/src/config/Config.zig --upstream-formatter-file vendor/ghostty/src/config/formatter_file.zig --upstream-formatter vendor/ghostty/src/config/formatter.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-formatter-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports:
  - `ghostty_canonical=203`;
  - `roastty_formatter_rows=203`;
  - `missing_canonical_formatter_rows=0`;
  - `extra_formatter_rows=0`;
  - `oracle_complete=79`;
  - `audit_covered=124`;
  - `gap=0`.
- A matrix assertion confirms:
  - CFG-217 remains `Pass`;
  - CFG-218 remains `Gap`;
  - all previously promoted formatter families remain `Oracle complete`;
  - the `command palette` formatter row is `Oracle complete`;
  - non-target formatter rows are not promoted by this oracle.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` is run on changed
  Markdown files.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.
