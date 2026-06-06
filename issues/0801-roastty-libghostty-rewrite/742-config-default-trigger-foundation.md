+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "gpt-5"
reasoning = "medium"

[review.result]
agent = "codex"
model = "gpt-5"
reasoning = "medium"
+++

# Experiment 742: Config Default Trigger Foundation

## Description

The `roastty_config_trigger` ABI currently validates inputs and always returns
the empty trigger. Upstream Ghostty's C API uses parsed binding actions plus the
default keybind reverse map, and its direct C API regression test verifies that
default `open_config` and `reload_config` triggers are visible while performable
or unsupported actions still return the empty trigger.

This experiment ports that first default-trigger behavior without introducing
full keybind parsing, storage, custom config files, key tables, sequences, or
surface key dispatch. Roastty is macOS-only, so the default modifier is the
macOS command/super modifier.

## Changes

- `roastty/src/lib.rs`
  - Add a small default trigger lookup used by `roastty_config_trigger`.
  - Return a unicode comma trigger with `ROASTTY_MODS_SUPER` for `open_config`.
  - Return a unicode comma trigger with
    `ROASTTY_MODS_SHIFT | ROASTTY_MODS_SUPER` for `reload_config`.
  - Preserve the empty trigger for null config, null action pointer, empty
    action strings, unknown actions, malformed action strings such as
    `open_config:`, `open_config:now`, `reload_config:`, and
    `reload_config:now`, and supported performable actions such as
    `adjust_selection:left`.
  - Keep `roastty_config_key_is_binding` unchanged; key-event lookup remains
    false until real keybind storage exists.
  - Do not add user keybind parsing, config-file loading, key tables, sequence
    handling, global keybinds, or surface key dispatch in this experiment.

- `roastty/tests/abi_harness.c`
  - Update config trigger coverage to assert the default open/reload triggers.
  - Keep empty-trigger coverage for null inputs and unsupported/malformed
    actions.

- Tests in `roastty/src/lib.rs`
  - Cover `open_config` and `reload_config` trigger tag/key/mods.
  - Cover null config, null action pointer, empty action string, unknown action,
    malformed action parameters (`open_config:`, `open_config:now`,
    `reload_config:`, `reload_config:now`), and performable
    `adjust_selection:left` returning the empty trigger.
  - Keep config key-is-binding false-path tests passing.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty config_trigger -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 742 design and found no technical blockers. The
review approved the narrow C API default-trigger foundation: return
command-comma for `open_config`, command-shift-comma for `reload_config`, and
keep unknown, malformed, performable, and missing-input cases on the empty
trigger until real keybind storage exists.

The review confirmed the macOS `SUPER` assumption is correct for Roastty's
current macOS-only scope. It asked that malformed trigger coverage explicitly
include parameterized forms such as `open_config:`, `open_config:now`,
`reload_config:`, and `reload_config:now`; the plan now lists those cases.

The review initially raised a stale process concern that Experiment 741 still
needed completion-review metadata and a result commit. Current git history shows
Experiment 741 has both required commits:
`13c2e09b9e597 Name windows for waiting paths` and
`a15eaf6394dc8 Open the paper roads`. No Experiment 741 blocker remains.

The remaining workflow requirement from the review was to record
`[review.design]`, this review section, and the README tuple before the
Experiment 742 plan commit; those records are now present.

## Result

**Result:** Pass

Experiment 742 moved `roastty_config_trigger` from a pure empty-trigger stub to
a small default-trigger lookup matching upstream Ghostty's C API regression
slice. `open_config` now returns a unicode comma trigger with
`ROASTTY_MODS_SUPER`, and `reload_config` returns unicode comma with
`ROASTTY_MODS_SHIFT | ROASTTY_MODS_SUPER`. This preserves Roastty's macOS-only
default modifier behavior.

Missing inputs, empty action strings, unknown action names, malformed
parameterized forms (`open_config:`, `open_config:now`, `reload_config:`,
`reload_config:now`), and performable `adjust_selection:left` continue to return
the empty physical-unidentified trigger. `roastty_config_key_is_binding` remains
unchanged and still returns `false` until real keybind storage exists.

The C ABI harness now checks the visible default triggers and the malformed /
performable empty-trigger fallbacks at the public C boundary.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty config_trigger -- --nocapture --test-threads=1`
  - 2 passed
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
  - 1 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
  - 129 passed
- `cargo test -p roastty --test abi_harness`
  - 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Roastty now exposes the first upstream-compatible default config triggers
through the C ABI. The next keybind experiments can build from this foundation
toward default trigger tables, user keybind parsing/storage, and real key-event
lookup without changing the empty-trigger ABI shape.

## Completion Review

Codex reviewed the completed Experiment 742 implementation and result diff. It
found no implementation blockers.

The review confirmed that exact `open_config` returns a unicode comma trigger
with `ROASTTY_MODS_SUPER`, exact `reload_config` returns a unicode comma trigger
with `ROASTTY_MODS_SHIFT | ROASTTY_MODS_SUPER`, and null, missing, empty,
unknown, malformed, and performable actions return the empty
physical-unidentified trigger. It also confirmed that the ABI harness and Rust
tests cover the new visible defaults plus parameterized malformed forms, and
that `roastty_config_key_is_binding` remains unchanged as planned.

The review's only blocker was missing workflow metadata: `[review.result]`, this
completion-review section, and the README tuple update. Those records are now
present.
