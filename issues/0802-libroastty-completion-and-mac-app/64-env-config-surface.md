+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
+++

# Experiment 64: Phase F — env config surface

## Description

Experiment 63 added the adjacent launch-command config surface for `command` and
`initial-command`. The next upstream launch config field after the existing
command-finish notification group is:

- `env`

Upstream represents `env` as `RepeatableStringMap` in
`vendor/ghostty/src/config/RepeatableStringMap.zig`: a repeatable
insertion-order map of `KEY=VALUE` strings that can be reset wholesale, can
remove one key by assigning it an empty value, and formats as one
`env = KEY=VALUE` line per entry.

This experiment ports that config surface only: the repeatable string-map type,
the `env` field, parser/formatter behavior, diagnostics, clone/equality, and
focused tests. Applying the resulting environment map to launched terminal
processes is intentionally deferred because the current config surface is not
yet wired into runtime launch snapshots and that belongs with the broader
command application/finalize work.

## Changes

- `roastty/src/config/mod.rs`
  - Add `RepeatableStringMap` as a cloneable/equatable insertion-order map.
  - Port upstream `RepeatableStringMap.parseCLI` behavior:
    - missing values return `ValueRequired`;
    - an empty value clears the whole map;
    - values without `=` return `ValueRequired`;
    - the first `=` separates key from value, so later `=` bytes are part of the
      value;
    - key and value sides are trimmed with upstream ASCII whitespace semantics;
    - an empty trimmed key is accepted, matching upstream's lack of key
      validation;
    - a non-empty value inserts or overwrites the key while preserving the map's
      insertion position;
    - an empty trimmed value removes that key from the map.
  - Port upstream `RepeatableStringMap.formatEntry` behavior:
    - an empty map writes `env = `;
    - non-empty maps write one `env = KEY=VALUE` line per entry in insertion
      order.
  - Add `Config::env: RepeatableStringMap = .{}`.
  - Route `env` through defaults, `Config::set`, `format_config`,
    clone/equality, and diagnostics.
  - Preserve upstream declaration/formatter order relative to the local emitted
    config: after the `notify-on-command-finish*` entries and before the later
    launch/runtime fields still to be ported.

Out of scope:

- Passing `env` to `Surface::start_termio` or inherited surface config.
- Runtime environment precedence against Ghostty/Roastty injected variables.
- `input`, `wait-after-command`, `abnormal-command-exit-runtime`, and
  `scrollback-limit`.
- `Config::finalize` or default command lookup.

## Verification

- Run formatting:
  - `cargo fmt -- roastty/src/config/mod.rs`
  - `prettier --write --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/README.md issues/0802-libroastty-completion-and-mac-app/64-env-config-surface.md`
- Run targeted tests:
  - `cargo test -p roastty env_config`
  - `cargo test -p roastty config_format_config`
- Add concrete test cases proving:
  - the default map is empty and formats as `env = `;
  - `env = A=B` inserts one entry;
  - repeated keys overwrite the value without creating duplicates;
  - multiple distinct keys format in insertion order;
  - values are split on the first `=`, so `A=B=C` formats as `A=B=C`;
  - empty keys such as `=VALUE` are accepted and format as `=VALUE`;
  - key/value edge ASCII whitespace is trimmed;
  - `env = A=` removes key `A`;
  - `env =` clears the whole map;
  - direct `Config::set` missing values and values without `=` return
    `ValueRequired`;
  - `Config::load_str` records `ConfigDiagnostic` line/key/error entries for
    invalid `env` lines while keeping valid neighboring lines;
  - clone/equality preserves the map;
  - formatter order places `env` after the `notify-on-command-finish*` entries
    and before the next locally emitted launch/runtime field.
- Run full Roastty tests:
  - `cargo test -p roastty`
- Run `cargo fmt --check`.
- Run `git diff --check`.
- Run `git status --short` and verify only intended source/docs are present.

**Pass** = `env` is represented faithfully on `Config`, round-trips through
config loading/formatting, matches upstream `RepeatableStringMap` parser
semantics, and has targeted and full tests passing.

**Partial** = the map surface lands but a parser or formatter edge case needs a
follow-up before using it for runtime launch.

**Fail** = `env` cannot be represented faithfully without first porting broader
launch/finalize infrastructure.

## Design Review

Codex adversarial reviewer `019eb3ac-a725-7333-97ec-4a88424f0311` returned
**Changes Required** with three required verification-scope findings:

- The design promised diagnostics but only required direct `Config::set` errors,
  not concrete `ConfigDiagnostic` assertions.
- The design did not require first-`=` splitting or upstream's accepted
  empty-key behavior to be tested.
- The design required formatter placement but did not explicitly require a
  config-order assertion.

The design was updated to add those parser, diagnostic, and formatter-order
requirements before implementation.

Re-review returned **Approved** with no findings after confirming the updated
verification list now requires concrete diagnostics, first-`=` and empty-key
parser coverage, and formatter-order coverage.

## Result

**Result:** Pass

Experiment 64 added the config-only `env` surface to
`roastty/src/config/mod.rs`. `Config` now owns a `RepeatableStringMap`
defaulting to empty, `Config::set("env", ...)` routes to the map parser, and
`format_config` emits `env` after the command-finish notification group.

The parser follows upstream `RepeatableStringMap` semantics: missing values and
values without `=` report `ValueRequired`, an empty value resets the map, the
first `=` separates key from value, key and value are trimmed with Zig ASCII
whitespace, empty keys are accepted, empty values remove that key, and repeated
keys update in place without duplicating entries. Formatting emits `env = ` for
an empty map or one `env = KEY=VALUE` line per entry in insertion order.

Runtime application of the environment map remains out of scope; this experiment
does not pass `env` into surface launch snapshots or alter inherited config.

Verification run:

- `cargo fmt -- roastty/src/config/mod.rs`
- `cargo test -p roastty env_config`
- `cargo test -p roastty config_format_config`
- `cargo test -p roastty`
- `cargo fmt --check`
- `git diff --check`

`cargo test -p roastty` passed with 4,499 unit tests, the C ABI harness, and doc
tests. The C ABI harness still emits existing enum-conversion warnings unrelated
to this config change.

## Conclusion

`env` now has a faithful parser/formatter config surface with defaults,
diagnostics, reset/removal behavior, first-`=` and empty-key coverage,
formatter-order coverage, and clone/equality coverage. The next config-surface
experiment can continue with the following upstream launch fields such as
`input`, `wait-after-command`, `abnormal-command-exit-runtime`, or
`scrollback-limit`, depending on the desired slice size.

## Completion Review

Codex-native adversarial reviewer `019eb3b4-fdd5-72e1-9de9-86a495eecf5e`
returned **Approved** with no findings.

The reviewer checked the completed experiment with fresh context, including the
workflow contract, issue README, experiment file, implementation diff since the
plan commit, `roastty/src/config/mod.rs`, and upstream
`vendor/ghostty/src/config/RepeatableStringMap.zig` / `Config.zig`.
