# Experiment 110: Phase G — keybind trigger prefix flags

## Description

Port the upstream keybind trigger-prefix flag foundation for configured
keybindings. Ghostty's binding parser accepts `global:`, `all:`, `unconsumed:`,
and `performable:` prefixes before the trigger, stores those flags on the
binding, and exposes them to app/surface binding queries through the C-facing
flag byte.

Roastty already has default-binding flag bits and partial binding-query
plumbing, but configured bindings currently store only an action and trigger.
This blocks faithful app-level key handling because
`roastty_app_has_global_keybinds` cannot be derived from parsed config, and
configured binding queries cannot report `unconsumed` or `performable` metadata.

This experiment adds the parser/storage/query foundation only. It does not
implement app-wide global shortcut registration, all-surface dispatch,
configured-binding unconsumed pass-through, performability checks for configured
actions, key sequences/chords, native keymaps, or `roastty_app_key` dispatch.

## Changes

- `roastty/src/lib.rs`
  - Add the missing configured-binding flag bits matching upstream
    `input.Binding.Flags`: consumed bit 0, all bit 1, global bit 2, performable
    bit 3.
  - Add a `flags: u8` field to `ConfigKeybind` and `ConfiguredBindingMatch`.
  - Parse repeated trigger prefixes before the trigger in
    `parse_config_keybind`, following upstream behavior:
    - recognized prefixes are `all`, `global`, `unconsumed`, and `performable`;
    - duplicate recognized prefixes are invalid;
    - recognized prefixes may appear in any order;
    - unknown prefixes stop flag parsing and fall through to normal trigger
      parsing;
    - `unconsumed:` clears the consumed bit;
    - the default flag byte remains consumed-only.
  - Update `Config::store_keybind` so any `global:` binding sets
    `has_global_keybinds`, and app creation / app config update continue to
    clone that value into `App`.
  - Update configured binding lookup so `Surface::key_is_binding` returns the
    configured binding's stored flags instead of always returning consumed-only.
  - Keep `Surface::key` consuming configured bindings for now, even when the
    binding has `unconsumed:`; runtime consumption semantics are a later action
    dispatch experiment.

## Verification

- Add parser/unit coverage for:
  - default configured keybind flags are consumed-only;
  - `unconsumed:` clears consumed;
  - `all:`, `global:`, and `performable:` set the expected bits;
  - multiple distinct prefixes compose;
  - duplicate recognized prefixes are rejected with a diagnostic;
  - an unknown prefix still falls through to trigger parsing and remains invalid
    unless it is a valid trigger.
- Add app/config coverage for:
  - `roastty_app_has_global_keybinds` is false without `global:` and true with
    `global:`;
  - `roastty_app_update_config` refreshes `has_global_keybinds`.
- Add surface binding-query coverage for:
  - `roastty_surface_key_is_binding` and the handle variant return configured
    binding flags rather than consumed-only;
  - default binding flag behavior is unchanged.
- Run:
  - `cargo test -p roastty keybind`
  - `cargo test -p roastty surface_key`
  - `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/110-keybind-trigger-prefix-flags.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

Codex-native adversarial review ran in a fresh-context subagent
(`multi_agent_v1.spawn_agent`, agent `019eb6b3-13e4-7921-85c2-9e8ac165772c`).

**Verdict:** Approved.

Findings: none.

## Result

**Result:** Pass.

Implemented the configured-keybind prefix flag foundation in
`roastty/src/lib.rs`. Configured keybinds now store the upstream C-facing flag
byte, with consumed bit 0, all bit 1, global bit 2, and performable bit 3. The
parser accepts `all:`, `global:`, `unconsumed:`, and `performable:` prefixes
before the trigger, rejects duplicate recognized prefixes, composes distinct
prefixes in any order, and lets unknown prefixes fall through to normal trigger
parsing.

`Config::store_keybind` now derives `has_global_keybinds` from parsed `global:`
bindings, and existing app creation / config-update paths clone that state into
`App`. Configured binding lookup carries stored flags through
`ConfiguredBindingMatch`, so `Surface::key_is_binding`,
`roastty_surface_key_is_binding`, and `roastty_surface_key_is_binding_handle`
report configured prefix flags instead of always reporting consumed-only.
Runtime dispatch still consumes configured bindings; global registration,
all-surface dispatch, unconsumed pass-through, performable configured-action
gating, sequences/chords, native keymaps, and `roastty_app_key` remain later
work as planned.

Verification:

- `cargo test -p roastty keybind` — pass: 20 unit tests passed; ABI harness
  filtered pass.
- `cargo test -p roastty surface_key` — pass: 44 unit tests passed; ABI harness
  filtered pass.
- `cargo test -p roastty -- --test-threads=1` — failed only on the known
  pre-existing
  `tests::surface_foreground_pid_reports_worker_foreground_pid_after_start`
  foreground-PID race. The final run had 4614 unit tests pass and 1 fail
  (`left: 44851`, `right: 44847`).
- `cargo test -p roastty surface_foreground_pid_reports_worker_foreground_pid_after_start -- --test-threads=1 --nocapture`
  — reproduced the same foreground-PID mismatch in isolation (`left: 60592`,
  `right: 60587`).
- `cargo test -p roastty -- --test-threads=1 --skip surface_foreground_pid_reports_worker_foreground_pid_after_start`
  — pass: 4614 unit tests passed, ABI harness passed with the known 10
  enum-conversion warnings, doc tests passed.
- `cargo fmt --check` — pass.
- `git diff --check` — pass.
- `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/110-keybind-trigger-prefix-flags.md issues/0802-libroastty-completion-and-mac-app/README.md`
  — pass after result formatting.

## Conclusion

The next keybinding prerequisite is in place: configured keybinds now preserve
the same prefix flag metadata that the embedded app needs to distinguish global,
all-surface, unconsumed, and performable bindings. The next Phase G slice can
build on this metadata for runtime semantics, most likely app-level/global
handling or the broader upstream binding table/action routing work.

## Completion Review

Codex-native adversarial review ran in a fresh-context subagent
(`multi_agent_v1.spawn_agent`, agent `019eb6c3-27ba-7a42-9d41-7f7254967e00`).

**Verdict:** Approved.

Findings: none.
