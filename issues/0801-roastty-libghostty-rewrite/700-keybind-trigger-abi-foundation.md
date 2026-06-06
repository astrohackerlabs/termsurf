+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 700: Keybind Trigger ABI Foundation

## Description

Upstream Ghostty exposes keybind trigger helpers at the C boundary:

- `ghostty_config_trigger(config, action, len)`;
- `ghostty_config_key_is_binding(config, key_event)`;
- `ghostty_surface_key_is_binding(surface, key_event, flags)`;
- `ghostty_app_has_global_keybinds(app)`.

Roastty currently has `roastty_surface_key_is_binding` and
`roastty_app_has_global_keybinds`, but both are hard false foundations and the
config-level trigger/query exports and C trigger structs are missing entirely.
The full upstream behavior depends on Ghostty's keybind parser, action parser,
reverse keybind map, active key tables, sequence set, key remaps, and
performable action checks. Roastty has not ported those pieces yet.

This experiment adds the missing ABI shape and makes the current "no keybinds
configured" behavior explicit, stable, and test-covered. It returns the same
empty trigger shape upstream uses when no trigger is found: physical
`Unidentified` with no modifiers. It also centralizes key-event validation and
flag zeroing so config-level and surface-level binding queries fail atomically
until a later experiment adds real keybind storage and parsing.

This does not implement keybind parsing, action parsing, reverse trigger lookup,
global keybind registration, active key tables, key sequences, key remaps, or
performable action dispatch.

## Changes

- `roastty/include/roastty.h`
  - Add Roastty-named equivalents of upstream trigger ABI types:
    - `roastty_input_trigger_tag_e`;
    - `roastty_input_trigger_key_u`;
    - `roastty_input_trigger_s`.
  - Add missing config-level keybind exports:
    - `roastty_config_trigger(roastty_config_t, const char*, uintptr_t)`;
    - `roastty_config_key_is_binding(roastty_config_t, roastty_key_event_t)`.

- `roastty/src/lib.rs`
  - Add matching `#[repr(C)]` trigger structs/unions and constants.
  - Add `empty_trigger()` returning physical `ROASTTY_KEY_UNIDENTIFIED` with
    `ROASTTY_MODS_NONE`, matching upstream's no-trigger fallback.
  - Implement `roastty_config_trigger` to validate null action pointers and
    return `empty_trigger()` for all current inputs because keybind/action
    parsing is not ported yet.
  - Implement `roastty_config_key_is_binding` to validate config and key event
    handles and return false while no keybind set exists.
  - Keep `roastty_app_has_global_keybinds` false, but route it through explicit
    config state so a later keybind parser can update one field.
  - Keep `roastty_surface_key_is_binding` false, but reuse the same key-event
    validation and atomic flag-zeroing behavior.
  - Preserve existing `roastty_surface_key` terminal dispatch behavior.

- `roastty/tests/abi_harness.c`
  - Add compile/link smoke coverage for the new trigger structs and config
    keybind exports.

- Tests in `roastty/src/lib.rs`
  - Cover trigger ABI layout/default values.
  - Cover `roastty_config_trigger` null and unknown-action cases returning the
    empty trigger.
  - Cover `roastty_config_key_is_binding` null/invalid event cases returning
    false.
  - Cover `roastty_surface_key_is_binding` flag zeroing on null surface, null
    event, detached surface, and valid no-binding event.
  - Cover `roastty_app_has_global_keybinds` remaining false for null/default
    config.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty trigger -- --nocapture`
- `cargo test -p roastty key_is_binding -- --nocapture`
- `cargo test -p roastty has_global_keybinds -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the staged Experiment 700 design and approved it with no blocking
findings. The review accepted the scoped ABI foundation, the upstream
empty-trigger fallback of physical `Unidentified` with no modifiers, the
explicit no-keybind behavior for config/surface binding queries and app global
keybind checks, and the proposed Rust and C harness coverage. The review noted
that the implementation should make trigger enum values explicit as
`Physical = 0`, `Unicode = 1`, and `CatchAll = 2`.
