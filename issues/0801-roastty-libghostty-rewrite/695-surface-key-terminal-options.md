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

# Experiment 695: Surface Key Terminal Options

## Description

Experiment 694 gave `roastty_surface_key` a working default terminal-write path,
but it always encodes with `key_encode::Options::default()`. That means active
terminal modes such as cursor-key application mode, keypad application mode,
backarrow mode, modify-other-keys, Alt-Esc prefixing, and Kitty keyboard flags
do not influence surface key dispatch yet.

Roastty's terminal already tracks these modes and flags. This experiment derives
the surface key encoder options from the attached worker terminal before
encoding key events.

This does not implement keybindings, app/global action dispatch, consumed-event
tracking, key remapping, trigger sequences, Option-as-Alt configuration, or
`roastty_surface_key_is_binding` beyond its current false/zero behavior.

## Changes

- `roastty/src/terminal/terminal.rs`
  - Add a public-to-crate `key_encode_options()` helper on `Terminal` that
    returns `key_encode::Options` populated from terminal state:
    - `cursor_key_application` from DEC cursor-key mode;
    - `keypad_key_application` from DEC keypad mode;
    - `backarrow_key_mode` from DEC backarrow key mode;
    - `ignore_keypad_with_numlock` from the existing terminal mode;
    - `alt_esc_prefix` from the existing terminal mode;
    - `modify_other_keys_state_2` from the runtime flag;
    - `kitty_flags` from the active screen's Kitty keyboard flags.
  - Leave `macos_option_as_alt` at the current default because Roastty does not
    have the full config/default keyboard-layout policy wired for surfaces yet.

- `roastty/src/lib.rs`
  - Change `Surface::key_encode_options()` to read options from the attached
    worker terminal when available, falling back to
    `key_encode::Options::default()` for no-worker and detached cases.
  - Preserve Experiment 694 behavior for empty encodings, write failures, and
    `last_key_event` storage.
  - Add focused tests that surface key dispatch reflects terminal state:
    - cursor-key application mode changes ArrowUp from normal to application
      cursor encoding;
    - keypad application mode changes a keypad key encoding;
    - backarrow mode changes Backspace encoding;
    - modify-other-keys level 2 affects modified key encoding;
    - Kitty keyboard flags affect encoded output.

- `roastty/tests/abi_harness.c`
  - No C ABI shape change is expected.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key -- --nocapture`
- `cargo test -p roastty key -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex approved the design: the requested encoder options map to existing
terminal modes, runtime flags, and active Kitty keyboard state, and the scope
correctly leaves Option-as-Alt policy and keybinding/action behavior for later.

## Result

**Result:** Pass.

Roastty now derives surface key encoder options from the attached worker
terminal. `roastty_surface_key` still stores `last_key_event`, encodes nonempty
key events, queues them to the worker, and returns `true` only on successful
queueing, but the encoding now reflects terminal cursor-key application mode,
keypad application mode, backarrow key mode, ignore-keypad-with-numlock, Alt-Esc
prefix mode, modify-other-keys state 2, and active Kitty keyboard flags.

No-worker surfaces still fall back to default key encoder options. Option-as-Alt
policy, keybinding/action dispatch, key remaps, trigger sequences, consumed
event tracking, and `roastty_surface_key_is_binding` remain out of scope for
this experiment.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_key -- --nocapture`
- `cargo test -p roastty key -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Surface key dispatch now honors the terminal keyboard modes that Roastty already
tracks. The remaining key work is the higher-level behavior upstream runs before
encoding: keybindings, action dispatch, trigger sequences, remaps, and consumed
event state.

## Completion Review

Codex reviewed the staged result and found no code correctness blockers. It
approved the terminal-derived key encoder options, the surface fallback
behavior, the deferred scope, and the verification results.
