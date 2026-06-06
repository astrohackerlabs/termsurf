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

# Experiment 746: Surface Default Key Is Binding

## Description

Experiment 745 taught `roastty_config_key_is_binding` to recognize Roastty's
static default keybind set, including performable defaults. The surface-level
query, `roastty_surface_key_is_binding`, still validates inputs, zeroes the
optional flags pointer, and always returns `false`.

Upstream Ghostty's `Surface.keyEventIsBinding` is similar to the config-level
query, but it returns `Binding.Flags` for the matching binding. At the C ABI
boundary, `ghostty_surface_key_is_binding` writes those flags with
`Binding.Flags.cval()` and returns `true`. Default flags encode as:

- `0b0001` for ordinary consumed bindings.
- `0b1001` for consumed performable bindings.

This experiment adds a static default surface keybind query that mirrors the
Experiment 745 event matching and returns representative upstream default flags
for the matched static default binding. It does not add user keybind parsing,
keybind storage, active key tables, key sequences, key remaps, performability
checks, or surface keybinding dispatch.

## Changes

- `roastty/src/lib.rs`
  - Replace the boolean-only default key-event matcher with a helper that
    returns default keybind flags for matching static defaults.
  - Keep `roastty_config_key_is_binding` as a bool wrapper over that helper.
  - Make `Surface::key_is_binding` return `false` for null events, detached
    surfaces, and release events.
  - Keep atomic flag zeroing for null surfaces, null events, detached surfaces,
    and nonmatching events.
  - Return `true` for press and repeat events matching static default keybinds.
  - Write `0b0001` for ordinary default bindings and `0b1001` for performable
    default bindings when the caller supplies a flags pointer.
  - Preserve physical-key precedence over UTF-8 and UTF-8 precedence over
    `unshifted_codepoint`.
  - Preserve binding-modifier normalization so lock keys and side-specific
    modifier bits do not prevent matches.
  - Keep key tables, sequences, key remaps, custom unbinds, and real surface
    binding dispatch out of scope.

- `roastty/tests/abi_harness.c`
  - Add representative C ABI checks that `roastty_surface_key_is_binding`
    returns ordinary and performable default flags, zeroes flags on false paths,
    tolerates null flags for true matches, and rejects release/nonmatching
    events.

- Tests in `roastty/src/lib.rs`
  - Cover ordinary defaults returning `0b0001`, such as command-Home or
    command-D.
  - Explicitly cover command-`=` returning `0b0001`, preserving the default
    font-size binding fixed in Experiment 745.
  - Cover performable defaults returning `0b1001`, such as command-C, command-K,
    command-F, Escape, and shift-arrow selection expansion.
  - Cover repeat events, lock-key/side-bit normalization, and physical
    precedence over UTF-8.
  - Cover release events, null events, detached surfaces, and nonmatching events
    returning `false` and zeroing flags.
  - Keep existing `config_key_is_binding`, `surface_key_is_binding`,
    `binding_action`, and ABI harness tests passing.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness -- --nocapture`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 746 design and found one real design gap:
surface-level tests must explicitly cover command-`=` returning ordinary
consumed flags. Experiment 745 fixed command-`=` as a clearly in-scope default
binding, and this experiment will replace the boolean default matcher with a
flag-returning helper. Without explicit surface coverage, the prior regression
could reappear in the new flag path while config-level bool tests still pass.

The plan now includes command-`=` coverage. The review accepted the experiment
scope: mirror the Experiment 745 static default matcher at the surface level,
keep `roastty_config_key_is_binding` as a bool wrapper, return `0b0001` for
ordinary consumed defaults and `0b1001` for consumed performable defaults, and
leave keybind storage, key tables, sequences, remaps, performability checks, and
dispatch out of scope.

## Result

**Result:** Pass

`roastty_surface_key_is_binding` now recognizes the same static default keybind
set as `roastty_config_key_is_binding`, but returns upstream-style binding flags
through the optional C ABI flags pointer. Ordinary consumed defaults return
`0b0001`; this includes the macOS natural text-editing `text`/`esc` bindings
because upstream defines those with ordinary `set.put` entries. Performable
consumed defaults return `0b1001`. False paths continue to zero the flags
pointer before returning `false`.

The implementation preserves the Experiment 745 lookup rules: release events do
not match, press and repeat events can match, physical keys take precedence over
UTF-8, UTF-8 takes precedence over `unshifted_codepoint`, and binding-modifier
normalization ignores lock keys and side-specific modifier bits. The
config-level query remains a bool wrapper over the shared default matcher.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty config_key_is_binding -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness -- --nocapture`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Surface keybind queries now have a useful static default implementation and can
report default binding flags to frontends. Full keybind storage, custom
configuration, active key tables, sequences, remaps, performability checks, and
actual keybinding dispatch remain for later experiments.

## Completion Review

Codex reviewed the completed Experiment 746 diff and initially flagged the
natural text-editing command/option arrow and command-backspace defaults as
performable. Re-review with the upstream `Config.zig` context confirmed those
bindings use ordinary `set.put` entries, not `putFlags(... performable = true)`,
so Roastty correctly reports them as ordinary consumed flags (`0b0001`).
Explicit Rust coverage now pins that behavior.

The re-review reported no remaining blocking technical issues. It confirmed the
static-default scope, flag values, precedence order, modifier normalization,
false-path flag zeroing, config-level bool wrapper, and Rust/C ABI coverage.
