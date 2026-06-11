# Experiment 108: Phase G — key-remap config field

## Description

Wire the `key-remap` config key onto the `Config` object now that Exp 107 ported
the reusable `RemapSet` foundation.

Upstream declares `@"key-remap": input/key_mods.zig.RemapSet = .empty`, parses
each repeated `key-remap = from=to` line into the same set, formats one output
line per stored remap, and calls `self.@"key-remap".finalize()` at the tail of
`Config.finalize()`. Roastty now has the core `RemapSet` parser, formatter, and
finalizer, but `Config` still has no `key_remap` field and no config parser,
formatter, or finalizer route.

This experiment should add the config-owned field and finalize route only. It
must not apply remaps to runtime key events, clone remaps into `Surface`, expose
the field through the app C ABI, implement native keymaps, or wire the full
keybinding table. Runtime key-event remapping remains a later Phase G slice.

## Changes

- `roastty/src/config/mod.rs`
  - Import `RemapSet` from `crate::input::key_mods`.
  - Add `Config::key_remap: RemapSet` with upstream default empty state.
  - Add `key-remap` to `Config::format_config`.
    - Empty state should format as `key-remap = `, matching upstream
      `RemapSet.formatEntry`.
    - Non-empty state should emit deterministic `key-remap = from=to` lines
      using `RemapSet::format_entries()`.
    - Because Roastty has not ported full `keybind` formatting yet, place this
      entry just before `window-padding-x`, the current local anchor for the
      upstream position after `keybind`.
  - Add a `Config::set` arm for `key-remap` that delegates to
    `RemapSet::parse_cli`, preserving repeatable accumulation and empty-value
    reset behavior from Exp 107.
    - Map `RemapSetParseError` to the existing `ConfigSetError::InvalidValue`,
      matching upstream's `parseCLI` behavior and avoiding a new config error
      variant.
  - Call `self.key_remap.finalize()` in `Config::finalize_with_report` after the
    existing scalar tail finalizers, matching upstream's tail call.
  - Add config-focused tests proving:
    - default `Config` contains an empty remap set;
    - repeated `key-remap` entries accumulate;
    - empty `key-remap =` resets the set;
    - invalid remaps surface through `ConfigSetError::InvalidValue`;
    - `format_config` emits `key-remap` in the expected local order;
    - `finalize()` orders right-side remaps before generic left-side remaps.

## Verification

Pass criteria:

1. `cargo test -p roastty key_remap`
2. `cargo test -p roastty config_format_config_emits_fields_in_upstream_order`
3. `cargo test -p roastty -- --test-threads=1`
4. `cargo fmt --check`
5. `git diff --check`
6. `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/108-key-remap-config-field.md issues/0802-libroastty-completion-and-mac-app/README.md`

The serial full-suite gate is used because Exp 107 and its review reproduced an
unrelated parallel-only flake in
`tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`; that
test passed alone and in the serial full suite.

## Design Review

Codex-native adversarial review ran in fresh context with subagent
`019eb685-0518-74c2-a1ed-041e8504a679`.

Initial verdict: **CHANGES REQUIRED**

Findings and fixes:

- Required: the initial design expected a new `ConfigSetError::KeyRemap`, but
  local config errors use `InvalidValue` and upstream `RemapSet.parseCLI` maps
  remap parse failures to invalid value. Fixed the design to require explicit
  mapping from `RemapSetParseError` to `ConfigSetError::InvalidValue`.

Re-review ran in fresh context with subagent
`019eb686-7a40-7b72-bdbd-cda83a109d1d`.

Final verdict: **APPROVED**

Findings: None.

## Result

**Result:** Pass

Implemented the config-owned `key-remap` field in `roastty/src/config/mod.rs`:

- added `Config::key_remap: RemapSet` with the upstream empty default;
- added `key-remap` formatting through `RemapSet::format_entries()`, including
  the upstream empty `key-remap = ` line and deterministic repeated output;
- added `Config::set("key-remap", ...)` routing with repeatable accumulation,
  empty-value reset, and `RemapSetParseError` mapped to
  `ConfigSetError::InvalidValue`;
- added `Config::finalize()` wiring so `key_remap.finalize()` orders the set for
  application;
- added focused config tests for parse/reset/format/finalize/error behavior and
  the local formatter-order anchor.

The implementation deliberately does not apply remaps to surface key events,
clone remaps into `Surface`, expose the field through the app C ABI, implement
native keymaps, or wire the full keybinding table.

Verification:

1. `cargo test -p roastty key_remap` — pass: 12 tests passed; filtered ABI
   harness passed.
2. `cargo test -p roastty config_format_config_emits_fields_in_upstream_order` —
   pass: 1 test passed; filtered ABI harness passed.
3. `cargo test -p roastty -- --test-threads=1` — pass: 4602 unit tests passed;
   ABI harness passed; doc tests passed. The ABI harness printed the known 10
   enum-conversion warnings.

## Conclusion

`key-remap` is now a first-class config field with parser, formatter, and
finalize behavior. Runtime use of the finalized remap set remains later Phase G
work: surface/app key-event handling still needs to clone the config set and
apply it before keybinding matching and terminal input encoding.

## Completion Review

Codex-native adversarial review ran in fresh context with subagent
`019eb68d-dae6-7853-b9a1-d83f6f7b78a9`.

Verdict: **APPROVED**

Findings: None.
