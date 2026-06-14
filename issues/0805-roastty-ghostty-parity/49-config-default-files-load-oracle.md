# Experiment 49: Config Default Files Load Oracle

## Description

Prove the remaining `config-default-files` parser row by testing the behavior
that makes it different from ordinary boolean config fields.

Pinned Ghostty treats `config-default-files` as a CLI-only switch. File-sourced
entries are documented as having no effect. During CLI loading, Ghostty resets
the field to `true`, parses CLI args, and if the CLI sets it to `false`, it
rebuilds the config from the CLI replay steps so values loaded from default
files are discarded.

Roastty already routes file-sourced `config-default-files` through a no-op path
and applies the boolean parser only for CLI source. This experiment will add a
targeted oracle that proves that direct parser behavior together with the
effective default-file discard/replay behavior. Passing this experiment should
promote the final parser inventory row from `Audit covered` to `Oracle complete`
and make CFG-217 pass. It should not claim full CFG-221 source-precedence parity
beyond the tested `config-default-files` semantics.

## Changes

- `roastty/src/config/mod.rs`
  - Implement Ghostty's CLI-only default-file discard path: reset
    `config_default_files` to `true` at the start of each CLI batch, remember
    the replay boundary for that batch, and when CLI parsing leaves
    `config_default_files = false`, rebuild from only the CLI replay entries
    added in that batch so default-file and other earlier file-sourced values
    are discarded.
  - Add a focused `config_default_files_parser_family_oracle` test.
  - Cover file-sourced no-op behavior for `config-default-files = false`.
  - Cover CLI boolean parsing for `false`, empty reset/default, and invalid
    values.
  - Cover default-file-loaded values being discarded when CLI disables default
    files while CLI-sourced values remain applied.
  - Cover the inverse case where `--config-default-files=` or
    `--config-default-files=true` keeps previously loaded default-file values.
- `issues/0805-roastty-ghostty-parity/config_parser_inventory.py`
  - Add a targeted oracle detector for
    `config_default_files_parser_family_oracle`.
  - Promote only the `config-default-files` row when that oracle exists.
  - Make Experiment 49 the CFG-217 owner when the targeted oracle is present.
- `issues/0805-roastty-ghostty-parity/config-parser-inventory.md`
  - Regenerate the parser inventory.
  - Expected counts after implementation: 203 canonical parser rows, 203 oracle
    complete rows, 0 audit-covered rows, 0 gap rows.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate the CFG-217 row so it becomes `Pass` only if every parser row is
    `Oracle complete`.
  - Keep CFG-221 as `Gap`; this experiment proves only the
    `config-default-files` subset of source/load semantics, not the full source
    precedence and repeated-file load facet.

## Verification

Pass criteria:

- `cargo test --manifest-path roastty/Cargo.toml config_default_files_parser_family_oracle`
  passes.
- `cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle`
  still passes, proving the default parser surface did not regress.
- `python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py --upstream vendor/ghostty/src/config/Config.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reports `oracle_complete=203`, `audit_covered=0`, `missing_dispatch_rows=0`,
  and `gap=0`.
- A matrix assertion confirms:
  - `CFG-217` is `Pass`.
  - `CFG-221` remains `Gap`, because the broader source-precedence facet still
    needs dedicated coverage.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/49-config-default-files-load-oracle.md issues/0805-roastty-ghostty-parity/README.md`
  leaves the edited Markdown files formatted.
- `git diff --check` passes.

## Design Review

Reviewed by a fresh-context Codex adversarial subagent.

Initial verdict: **Changes required**.

Required findings:

- The design claimed default-file discard/replay parity but did not explicitly
  include the needed Roastty implementation work. Fixed by adding the reset,
  replay-boundary, and CLI-only replay implementation scope.
- The verification expected `gap_rows=0`, but the generator prints `gap=...`.
  Fixed by changing the expected output to `gap=0`.

Optional finding:

- The Markdown formatting command did not name target files. Fixed by listing
  the edited issue files explicitly.

Re-review verdict: **Approved**. The reviewer confirmed all prior findings were
resolved and found no new required issues.

## Result

**Result:** Pass

Implemented Ghostty's `config-default-files` CLI-only discard behavior in
Roastty's CLI config loader. Each CLI batch now resets `config_default_files` to
`true`, records the batch replay boundary, and when the batch sets
`config-default-files = false`, rebuilds the config from only the successful CLI
entries in that batch. This discards previously loaded default-file values while
preserving CLI-sourced values, matching pinned Ghostty's replay behavior for
this switch.

Added `config_default_files_parser_family_oracle`, which proves:

- direct/file-sourced `config-default-files = false` is accepted but has no
  effect;
- CLI `false`, empty reset/default, `true`, and invalid values follow the
  boolean parser semantics;
- default-file-loaded values are discarded when CLI disables default files;
- default-file-loaded values are preserved when CLI resets/enables default
  files.

Updated the parser inventory generator with a targeted detector for this oracle.
The regenerated parser inventory now reports all 203 canonical parser rows as
`Oracle complete`; CFG-217 is `Pass`. CFG-221 intentionally remains `Gap`
because broader source precedence and repeated-file load semantics still need
dedicated coverage.

Verification:

- `cargo test --manifest-path roastty/Cargo.toml config_default_files_parser_family_oracle`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle`
  passed.
- `python3 issues/0805-roastty-ghostty-parity/config_parser_inventory.py --upstream vendor/ghostty/src/config/Config.zig --roastty roastty/src/config/mod.rs --config-inventory issues/0805-roastty-ghostty-parity/config-inventory.md --output issues/0805-roastty-ghostty-parity/config-parser-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
  reported `oracle_complete=203`, `audit_covered=0`, `missing_dispatch_rows=0`,
  and `gap=0`.
- Matrix assertion passed: CFG-217 is `Pass`; CFG-221 is `Gap`.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passed.
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/49-config-default-files-load-oracle.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-parser-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
  completed.
- `git diff --check` passed.

## Conclusion

The non-default parser facet is now complete: every canonical parser row has an
oracle, and CFG-217 can pass. The next experiments should move to another
unresolved config facet, most likely CFG-218 non-default formatter behavior or
CFG-221 source precedence/repeated-file load semantics.

## Completion Review

Reviewed by a fresh-context Codex adversarial subagent.

Verdict: **Approved**.

Findings: none.

The reviewer independently verified:

- `cargo test --manifest-path roastty/Cargo.toml config_default_files_parser_family_oracle`
  passed.
- `cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle`
  passed.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passed.
- `git diff --check` passed.

The reviewer did not run the parser inventory generator because it writes files,
but inspected the working-tree diff and approved the completed experiment.
