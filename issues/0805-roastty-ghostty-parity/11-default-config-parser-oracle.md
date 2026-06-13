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

## Result

**Result:** Pass

Roastty now accepts every line in the pinned Ghostty default config fixture
through its config-line parser and per-key parser. The oracle initially found
six rejected default lines:

- `font-codepoint-map = `
- `clipboard-codepoint-map = `
- `background-image-opacity = 1`
- `keybind = super+==increase_font_size:1`
- `keybind = super++=increase_font_size:1`
- `keybind = super+ctrl+==equalize_splits`

Key changes:

- `roastty/src/config/mod.rs`
  - Added `config_default_parser_oracle`, which asserts the 635-line fixture
    count and checks every default line with `Config::parse_config_line` plus
    `Config::set` on a fresh config.
  - Made empty `font-codepoint-map` and `clipboard-codepoint-map` values reset
    their maps, matching Ghostty's valid default config output.
  - Routed `background-image-opacity` through the config parser.
  - Added focused regression coverage for codepoint-map empty resets and
    background image opacity parsing.
- `roastty/src/lib.rs`
  - Changed keybind parsing to choose the trigger/action separator by validating
    candidate `=` separators, preserving `=` as a trigger key while keeping
    `text:=...` actions valid.
  - Added plus-key trigger parsing for Ghostty's `super++` default syntax.
  - Added focused regression coverage for the default `=`, `+`, and
    `ctrl+super+=` keybind forms.
- Issue docs
  - Added the parser oracle to the default-config oracle docs.
  - Added `CFG-216` to the config matrix.
  - Added a README learning and updated Experiment 11 status.

Verification:

```bash
cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle -- --nocapture
ROASTTY_DEFAULT_CONFIG_OUT=/Users/astrohacker/dev/termsurf/logs/issue805-exp11-roastty-default-config.txt \
  cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle -- --nocapture
cargo test --manifest-path roastty/Cargo.toml parse_config_keybind_defaults_preserve_equal_and_plus_keys -- --nocapture
cargo test --manifest-path roastty/Cargo.toml config_codepoint_map_parses_ranges_and_formats_entries -- --nocapture
cargo test --manifest-path roastty/Cargo.toml config_clipboard_codepoint_map_routes_and_formats -- --nocapture
cargo test --manifest-path roastty/Cargo.toml background_opacity -- --nocapture
cargo test --manifest-path roastty/Cargo.toml keybind -- --test-threads=1 --nocapture
```

## Conclusion

The full pinned Ghostty default config surface now has two cheap durable guards
in Roastty: exact formatter parity and per-line parser acceptance. This still
does not prove non-default values, whole-file repeatable replacement semantics,
diagnostics, precedence, reload behavior, command palette UI behavior, or
runtime effects.

## Completion Review

Fresh-context adversarial completion review initially found one required issue:
the new doubled-plus trigger branch accepted `++=...`, while pinned Ghostty
rejects that form.

Fix:

- Updated `roastty/src/lib.rs` so the doubled-plus branch requires a non-empty
  modifier prefix. Bare `+=...` remains the plus key, `ctrl++=...` remains a
  modified plus key, and `++=...` is rejected.
- Added a focused regression assertion for `++=ignore`.

Re-review approved the result:

```text
VERDICT: APPROVED

Findings: none.
```
