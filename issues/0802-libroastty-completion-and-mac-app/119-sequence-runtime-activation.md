# Experiment 119: Phase G — sequence runtime activation

## Description

Activate the configured-keybind sequence trie added in Experiment 118 for
surface key handling.

Upstream Ghostty treats a matching sequence prefix as a leader: the leader key
is consumed, its encoded bytes are queued, and the next key is looked up in the
nested binding set. A matching leaf performs the configured action and ends the
sequence. An invalid non-modifier key flushes queued prefix bytes to the pty and
then lets the invalid key continue through normal encoding.

This experiment wires that runtime behavior for Roastty's configured surface
keybindings only. It does not implement `ignore`, `end_key_sequence`, `chain=`,
native keymaps, native global shortcuts, `roastty_app_key` sequence handling, or
key-table sequence runtime lookup. Table sequence activation is left for a
follow-up slice because it needs to combine the active key-table stack with
sequence state and one-shot table popping.

## Changes

- `roastty/src/lib.rs`
  - Add surface runtime state for the currently active configured key sequence:
    - the active nested `ConfigKeybindSet`;
    - queued encoded leader-key bytes.
  - Add lookup helpers on `ConfigKeybindSet` that can return either:
    - a leader entry with a nested set; or
    - a leaf `ConfiguredBindingMatch`.
  - Update `Surface::key_is_binding` so a configured sequence leader is reported
    as a binding with empty flags, matching upstream's `keyEventIsBinding`
    leader behavior.
  - In `Surface::key`, check active sequence state before key
    tables/root/default bindings:
    - release events do not advance a sequence;
    - matching leader entries update the active nested set, queue the encoded
      leader bytes, notify `ROASTTY_ACTION_KEY_SEQUENCE` active with the leader
      trigger, and consume the key;
    - matching leaf entries dispatch the configured action, clear sequence
      state, drop queued leader bytes when consumed, and flush queued leader
      bytes when the leaf falls through as unconsumed or unperformed;
    - invalid modifier keys leave the sequence active and fall through without
      flushing queued prefix bytes;
    - invalid non-modifier keys flush queued prefix bytes, clear sequence state,
      and continue through the normal non-sequence key path so the invalid key
      can be encoded.
  - In normal root configured lookup, treat sequence leaders from
    `app.keybind_sequences` before flat root configured bindings:
    - a leader consumes the key, queues its encoded bytes, and starts the active
      sequence;
    - a leaf is not expected at the root sequence set for direct bindings
      because the flat runtime vector remains authoritative for single-key
      runtime behavior in this slice.
  - Add sequence end handling that clears active sequence state, either flushes
    or drops queued leader bytes, and sends `ROASTTY_ACTION_KEY_SEQUENCE` with
    `active = false`.
  - Add Rust `RoasttyActionKeySequence` / union conversion support for
    `ROASTTY_ACTION_KEY_SEQUENCE`, including the test-only inverse used by
    action-record tests.
  - Keep `roastty_app_key` sequence handling out of scope.
  - Keep key-table sequence runtime lookup out of scope; table sequences remain
    stored but inert until the follow-up key-table sequence experiment.
- `roastty/tests/abi_harness.c`
  - Add C ABI coverage for the typed `key_sequence` action payload, proving
    active/start and inactive/end notifications are visible through the public
    action callback.
- Tests in `roastty/src/lib.rs`
  - A root sequence leader such as `ctrl+a>n=new_window` consumes `ctrl+a`,
    sends a `key_sequence` active notification with the leader trigger, and does
    not perform `new_window` yet.
  - Pressing the matching final key performs the configured leaf action, sends a
    `key_sequence` inactive notification, and drops queued leader bytes.
  - A nested sequence such as `ctrl+a>ctrl+b>c=toggle_fullscreen` advances
    through both leaders, notifies both active leader triggers, and performs
    only after the final `c`.
  - Release events during an active sequence neither advance nor clear the
    sequence, and do not flush queued leader bytes.
  - An invalid non-modifier key after a leader flushes queued leader bytes,
    clears sequence state, sends the inactive notification, and then encodes the
    invalid key normally.
  - A modifier key during an active sequence does not flush or clear the active
    sequence.
  - `surface_key_is_binding` reports a configured sequence leader as a binding
    with empty flags, while sequence-only leaf keys remain non-bindings until
    their leader is active.
  - A direct configured binding added after a sequence prefix still overrides
    and prevents runtime sequence activation for that prefix.
  - Runtime key-table sequence bindings remain inert in this experiment.
  - `roastty_app_key` still ignores sequence-only bindings in this experiment.

## Verification

- Run:
  - `cargo test -p roastty sequence`
  - `cargo test -p roastty parse_config_keybind`
  - `cargo test -p roastty key_table`
  - `cargo test -p roastty surface_key`
  - `cargo test -p roastty app_key`
  - `cargo test -p roastty --test abi_harness`
  - `cargo test -p roastty -- --test-threads=1`
  - `cargo fmt`
  - `cargo fmt --check`
  - `git diff --check`
  - `prettier --check --prose-wrap always --print-width 80 issues/0802-libroastty-completion-and-mac-app/119-sequence-runtime-activation.md issues/0802-libroastty-completion-and-mac-app/README.md`

## Design Review

**Reviewer:** Codex-native adversarial reviewer, fresh context
(`multi_agent_v1.spawn_agent`, agent `019eb775-51c9-7db3-9277-42ecea4b1e81`)

**Initial verdict:** Changes required

**Required finding:** The original verification plan did not prove nested
runtime sequence behavior or release-event behavior. The reviewer pointed out
that the design proposed active nested-set updates and release handling, but the
tests only covered a two-key sequence and had no release-event case.

**Fix:** Added explicit tests for a nested sequence
`ctrl+a>ctrl+b>c=toggle_fullscreen` and for release events during an active
sequence neither advancing, clearing, nor flushing the sequence.

**Final verdict:** Approved

**Final findings:** None.

The reviewer confirmed that the prior required finding was resolved and that the
issue README still links Experiment 119 as `Designed`.
