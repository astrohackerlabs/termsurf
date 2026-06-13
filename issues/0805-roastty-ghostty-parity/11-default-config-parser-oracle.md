# Experiment 11: Default Config Parser Oracle

## Description

Experiments 8 through 10 proved that Roastty's default config formatter output
matches pinned Ghostty exactly after app-name normalization. The config matrix
still has 203 canonical `Gap` rows because name inventory and default formatter
parity do not prove parser behavior.

This experiment adds the next cheap config guard: every line in the pinned
Ghostty default config output must be accepted by Roastty's config parser. The
goal is not to prove all non-default values, diagnostics, precedence, reload, or
runtime effects. It is to prove the full default-format surface is also
loadable/parseable by the Rust config implementation, including repeatable
surfaces such as `palette`, `keybind`, and `command-palette-entry`.

The experiment should be careful about scope. `+show-config --default` is a
formatter artifact, not necessarily a user config recipe with exact repeatable
replacement semantics when loaded all at once over an already-default config.
Therefore the first oracle should prove parser acceptance of the emitted default
entries without claiming whole-file replacement semantics unless that is also
implemented and verified.

## Changes

- `roastty/src/config/mod.rs`
  - Add a focused unit test that iterates over every non-comment default config
    line from `roastty/testdata/issue805-ghostty-default-config.txt`.
  - Parse each `key = value` line with the existing `loader::parse_config_line`
    path, then call Roastty's config parser for that key/value.
  - Assert the fixture contains the expected 635 default config lines so fixture
    truncation cannot silently narrow coverage.
  - Treat parser rejection as a test failure that reports the key and line.
  - Preserve app-name normalization only where necessary for comparison; do not
    hide parser failures behind broad string rewriting.
  - Account for repeatable defaults explicitly. If the test parses each line
    independently, document that it proves per-entry parser acceptance only. If
    the test parses the whole file, prove and document the exact repeatable
    replacement semantics.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Add or update a config row for default config parser acceptance.
  - Keep canonical option rows as `Gap` unless the experiment proves the full
    row scope for that option.
- `issues/0805-roastty-ghostty-parity/default-config-oracle.md`
  - Add a short section naming the new parser oracle and its guard command.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning if the experiment proves a reusable way to parse the full
    default config surface.
  - Update the Experiment 11 status after the result is known.

## Verification

Pass criteria:

- A focused test passes and proves every default config line emitted by pinned
  Ghostty is accepted by Roastty's parser in the explicitly documented mode.
- The test asserts the expected 635-line fixture coverage before checking parser
  acceptance.
- The test failure output identifies the rejected line and key if a future
  regression breaks parser acceptance.
- `cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle -- --nocapture`
  passes.
- `cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle -- --nocapture`
  still passes, proving the parser oracle did not weaken the existing formatter
  oracle.
- `cargo fmt --manifest-path roastty/Cargo.toml --check` passes.
- `prettier --write --prose-wrap always --print-width 80` has been run on the
  changed issue markdown files.
- `git diff --check` passes.
- Matrix updates do not mark non-default parser behavior, diagnostics,
  precedence, reload, UI behavior, or runtime effects as passing from this
  default-line parser evidence.

Suggested commands:

```bash
cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle -- --nocapture
ROASTTY_DEFAULT_CONFIG_OUT=/Users/astrohacker/dev/termsurf/logs/issue805-exp11-roastty-default-config.txt \
  cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle -- --nocapture
cargo fmt --manifest-path roastty/Cargo.toml --check
prettier --write --prose-wrap always --print-width 80 \
  issues/0805-roastty-ghostty-parity/11-default-config-parser-oracle.md \
  issues/0805-roastty-ghostty-parity/README.md \
  issues/0805-roastty-ghostty-parity/default-config-oracle.md \
  issues/0805-roastty-ghostty-parity/config-matrix.md
git diff --check
```

## Design Review

Fresh-context adversarial design review approved the plan with no required
findings.

Reviewer verdict:

```text
VERDICT: APPROVED

No Required findings.
```

Accepted review suggestions:

- Added a pass criterion requiring the oracle to assert the known 635-line
  fixture count.
- Named the existing `loader::parse_config_line` parser path instead of leaving
  room for ad hoc line splitting.
