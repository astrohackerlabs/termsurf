# Default Config Oracle

Experiment 8 records the first reusable A/B oracle for pinned Ghostty default
config formatting versus Roastty `Config::default().format_config(...)`.

## Reference

- Ghostty commit: `2c62d182cec246764ff725096a70b9ef44996f7f`
- Ghostty executable:
  `vendor/ghostty/zig-out/Ghostty.app/Contents/MacOS/ghostty`
- Ghostty fixture: `roastty/testdata/issue805-ghostty-default-config.txt`
- Roastty implementation: `roastty/src/config/mod.rs`

## Regeneration

Capture the pinned Ghostty default config:

```bash
vendor/ghostty/zig-out/Ghostty.app/Contents/MacOS/ghostty \
  +show-config --default --no-pager \
  > logs/issue805-exp8-ghostty-default-config.txt
cp logs/issue805-exp8-ghostty-default-config.txt \
  roastty/testdata/issue805-ghostty-default-config.txt
```

Capture Roastty's current default formatter output while running the oracle
test:

```bash
ROASTTY_DEFAULT_CONFIG_OUT=/Users/astrohacker/dev/termsurf/logs/issue805-exp10-roastty-default-config.txt \
  cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle -- --nocapture
```

Run the durable guard without regenerating the output log:

```bash
cargo test --manifest-path roastty/Cargo.toml config_default_format_oracle
```

Run the default-line parser guard:

```bash
cargo test --manifest-path roastty/Cargo.toml config_default_parser_oracle
```

## Normalization

The test normalizes only the application rename:

- `Ghostty` and `Roastty` become `{App}`.
- `ghostty` and `roastty` become `{app}`.

The test does not normalize semantic values. It compares every default config
line exactly and in order after app-name normalization.

## Current Result

Current counts from `logs/issue805-exp10-default-config-diff-summary.txt`:

- Ghostty raw lines: 635
- Roastty raw lines: 635
- Normalized ordered lines match after app-name normalization: true
- Normalized multiset mismatches: 0
- Ghostty `keybind` lines: 93
- Roastty `keybind` lines: 93
- `keybind` ordered match: true
- `keybind` multiset mismatches: 0
- Ghostty `command-palette-entry` lines: 88
- Roastty `command-palette-entry` lines: 88
- `command-palette-entry` ordered match: true
- `command-palette-entry` multiset mismatches: 0
- Total missing normalized lines: 0
- Total extra normalized lines: 0

## Gaps

There are no known default-format diffs after app-name normalization.

This oracle proves default values, formatter output, and formatter order only
for the full default-format surface. It does not prove parser behavior,
diagnostics, config-file precedence, CLI/env precedence, config reload, command
palette UI behavior, or runtime effects.

## Parser Oracle

Experiment 11 adds `config_default_parser_oracle`, which iterates over all 635
lines in the pinned Ghostty default-config fixture, parses each line through
Roastty's config-line parser, and applies that key/value pair to a fresh
`Config`. The test proves per-line parser acceptance for the full default
surface and reports the rejected line/key if a future regression breaks it.

This parser oracle intentionally does not prove non-default values, diagnostics,
whole-file repeatable replacement semantics, precedence, reload behavior,
command palette UI behavior, or runtime effects.
