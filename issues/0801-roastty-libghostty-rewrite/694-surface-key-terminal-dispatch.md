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

# Experiment 694: Surface Key Terminal Dispatch

## Description

Experiment 690 added the state-only surface key callback foundation:
`roastty_surface_key` validates a `roastty_key_event_t`, stores the last event
on the surface, and returns `false`; `roastty_surface_key_is_binding` zeros
flags and returns `false`.

Upstream Ghostty routes `ghostty_surface_key` through app-level keybinding and
surface key handling, then encodes unconsumed keys to terminal input. Roastty
does not have the full binding/action system, trigger sequence state, key-remap
configuration, or terminal-derived key encoder options yet. This experiment
wires only the first terminal dispatch path: valid surface key events encode to
bytes and queue to the attached termio worker.

This does not implement keybindings, app/global action dispatch, consumed-event
tracking, key remapping, trigger sequences, `roastty_surface_key_is_binding`
beyond the current false/zero behavior, or full terminal-derived encoder option
coverage. It also does not change the key-event C ABI shape.

## Changes

- `roastty/src/lib.rs`
  - Add a `Surface::key_encode_options()` helper returning
    `key_encode::Options::default()` for this slice. The helper exists as the
    future seam for terminal-derived options such as cursor-key application
    mode, keypad application mode, backarrow mode, modify-other-keys, Kitty
    keyboard flags, and Option-as-Alt policy.
  - Change `Surface::key` so valid attached-surface events:
    - still store an owned clone in `last_key_event`;
    - encode the event with `key_encode::encode`;
    - queue nonempty encoded bytes to the attached termio worker;
    - return `true` only when bytes were encoded and queued successfully.
  - Preserve safe no-op / false behavior for null surfaces, detached surfaces,
    invalid event handles, missing workers, empty encodings such as unsupported
    key releases under default options, and worker queue failures. Queue
    failures should continue to record the existing termio error.
  - Keep `roastty_surface_key_is_binding` state-only: it zeros non-null flags
    and returns `false`.
  - Add focused tests for:
    - UTF-8 printable key events reaching a child process through the worker;
    - Enter/Backspace/Tab default legacy encodings where practical;
    - release events that encode to no bytes returning `false` while still
      storing `last_key_event`;
    - no-worker, detached, null, and worker-failure cases remaining safe;
    - `roastty_surface_key_is_binding` retaining false/zero behavior.

- `roastty/tests/abi_harness.c`
  - Keep the existing surface key smoke calls compiling against the same ABI. No
    C ABI shape change is expected.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key -- --nocapture`
- `cargo test -p roastty key -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex approved the design as a correct incremental slice after Experiment 690:
surface key events will keep storing `last_key_event`, encode with default key
encoder options, queue only nonempty bytes, and leave bindings/actions and
terminal-derived encoder options for later experiments.

## Result

**Result:** Pass.

`roastty_surface_key` now stores a cloned key event, encodes it with
`key_encode::Options::default()`, queues nonempty encoded bytes to the attached
termio worker, and returns `true` only when the write is queued successfully.
Printable UTF-8 key events now reach a child PTY through the surface callback.

The implementation preserves false/no-op behavior for null surfaces, detached
surfaces, invalid key-event handles, missing workers, empty encodings such as
default release events, and worker write failures. Worker write failures record
the existing termio error. `roastty_surface_key_is_binding` remains state-only:
it zeroes non-null flags and returns `false`.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key -- --nocapture`
- `cargo test -p roastty key -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Surface key callbacks now have a working default terminal-write path. The next
key slices should derive encoder options from terminal/runtime state and then
add the keybinding/action-dispatch behavior that upstream runs before terminal
encoding.

## Completion Review

Codex reviewed the staged result and found no code correctness blockers. It
approved the default terminal key dispatch path, the retained `key_is_binding`
false/zero behavior, the deferred scope, and the verification results.
