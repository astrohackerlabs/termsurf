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

# Experiment 722: Binding Action Tab Navigation Forwarding

## Description

Experiment 721 added the first tab/window runtime action forwarding slice:
opening/closing tabs, window focus, split zoom, reset-window-size, maximize, and
fullscreen. Upstream Ghostty's adjacent tab-navigation actions use the same
surface-to-runtime forwarding boundary:

- `previous_tab`
- `next_tab`
- `last_tab`
- `goto_tab:<index>`
- `move_tab:<offset>`
- `toggle_tab_overview`

This experiment adds parser and callback forwarding for that tab-navigation
slice only. It does not implement the tab model, tab overview UI, tab wrapping,
or frontend mutations. The frontend/runtime remains responsible for consuming
the forwarded action tags and storage.

## Changes

- `roastty/include/roastty.h`
  - Add action tags matching upstream `ghostty_action_tag_e` values:
    - `ROASTTY_ACTION_TOGGLE_TAB_OVERVIEW = 8`
    - `ROASTTY_ACTION_MOVE_TAB = 14`
    - `ROASTTY_ACTION_GOTO_TAB = 15`
  - Add goto-tab selector constants matching upstream
    `ghostty_action_goto_tab_e` special values:
    - `ROASTTY_GOTO_TAB_PREVIOUS = -1`
    - `ROASTTY_GOTO_TAB_NEXT = -2`
    - `ROASTTY_GOTO_TAB_LAST = -3`
  - Document storage conventions:
    - goto tab: `storage[0]` stores either the parsed tab index or a special
      `roastty_goto_tab_e` value represented as a signed `intptr_t` cast to
      `uintptr_t`;
    - move tab: `storage[0]` stores the signed `intptr_t` offset cast to
      `uintptr_t`;
    - toggle tab overview leaves storage zeroed.

- `roastty/src/lib.rs`
  - Add matching constants.
  - Extend `parse_binding_action` to accept:
    - `previous_tab`
    - `next_tab`
    - `last_tab`
    - `goto_tab:<usize>`
    - `move_tab:<isize>`
    - `toggle_tab_overview`
  - Reject missing, empty, whitespace-padded, unknown, extra-colon, negative
    `goto_tab`, and overflowing numeric parameters.
  - Forward all actions through the existing runtime `action_cb`, returning
    `false` for null, detached, and no-callback surfaces and otherwise returning
    the callback result.
  - Forward `previous_tab`, `next_tab`, and `last_tab` as
    `ROASTTY_ACTION_GOTO_TAB` with `storage[0]` set to
    `ROASTTY_GOTO_TAB_PREVIOUS`, `ROASTTY_GOTO_TAB_NEXT`, and
    `ROASTTY_GOTO_TAB_LAST`, respectively, using the signed-to-unsigned storage
    convention documented in the header.
  - Keep all previously supported binding actions unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage for the new action constants and goto-tab special
    values.
  - Add malformed tab-navigation action rejection checks.
  - Add no-callback coverage that valid tab-navigation forwarding actions return
    `false` without crashing.

- Tests in `roastty/src/lib.rs`
  - Cover constants matching upstream values.
  - Cover invalid parser forms, including empty parameters, whitespace, extra
    colons, negative/overflowing goto-tab values, and overflowing move-tab
    values.
  - Cover null, detached, and no-callback surfaces returning `false`.
  - Cover valid tab-navigation actions forwarding expected tags, target,
    storage, signed goto-tab special values, signed move-tab representation, and
    callback result.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty tab_navigation -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 722 design and found the slice otherwise
well-scoped as parser plus runtime forwarding only, with no frontend/tab model
behavior included.

The review raised two ABI-spec blockers:

- signed `goto_tab` special values must document their `intptr_t` to `uintptr_t`
  storage representation and test that the forwarded values round-trip back to
  `-1`, `-2`, and `-3`;
- `previous_tab`, `next_tab`, and `last_tab` must explicitly forward as
  `ROASTTY_ACTION_GOTO_TAB` with the corresponding special selector in
  `storage[0]`.

The plan now documents both requirements and includes storage round-trip tests
for the signed goto-tab selectors.

The review also noted the normal workflow requirement to replace the pending
review body before the plan commit. This section records the design review, and
the README tuple is `Codex/Codex/-`.

Codex re-reviewed the revised design and found no remaining findings. The design
is approved for the plan commit.
