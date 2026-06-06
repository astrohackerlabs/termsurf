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

# Experiment 724: Binding Action Runtime Control Forwarding

## Description

Experiment 723 added no-storage runtime UI toggle forwarding. Upstream Ghostty's
next nearby surface-scoped controls include a small group that still forwards to
the app runtime but carries either a simple enum payload or no storage:

- `toggle_window_float_on_top`
- `toggle_secure_input`
- `inspector:toggle|show|hide`
- `close_window`

This experiment adds parser and callback forwarding for those runtime controls
only. It does not implement floating windows, secure input APIs, inspector UI
rendering, close-window frontend behavior, or Swift frontend state. The
frontend/runtime remains responsible for consuming the forwarded action tags and
storage.

`toggle_mouse_reporting` is intentionally left for a later experiment because
upstream mutates local surface configuration instead of forwarding a runtime
action.

## Changes

- `roastty/include/roastty.h`
  - Add action tags matching upstream `ghostty_action_tag_e` values:
    - `ROASTTY_ACTION_INSPECTOR = 28`
    - `ROASTTY_ACTION_FLOAT_WINDOW = 42`
    - `ROASTTY_ACTION_SECURE_INPUT = 43`
    - `ROASTTY_ACTION_CLOSE_WINDOW = 49`
  - Add enum constants matching upstream payload values:
    - `ROASTTY_INSPECTOR_TOGGLE = 0`
    - `ROASTTY_INSPECTOR_SHOW = 1`
    - `ROASTTY_INSPECTOR_HIDE = 2`
    - `ROASTTY_FLOAT_WINDOW_ON = 0`
    - `ROASTTY_FLOAT_WINDOW_OFF = 1`
    - `ROASTTY_FLOAT_WINDOW_TOGGLE = 2`
    - `ROASTTY_SECURE_INPUT_ON = 0`
    - `ROASTTY_SECURE_INPUT_OFF = 1`
    - `ROASTTY_SECURE_INPUT_TOGGLE = 2`
  - Document storage conventions:
    - inspector: `storage[0] = roastty_inspector_mode_e`
    - float window: `storage[0] = roastty_float_window_e`
    - secure input: `storage[0] = roastty_secure_input_e`
    - close window leaves storage zeroed.

- `roastty/src/lib.rs`
  - Add matching constants.
  - Extend `parse_binding_action` to accept:
    - `toggle_window_float_on_top`
    - `toggle_secure_input`
    - `inspector:toggle`
    - `inspector:show`
    - `inspector:hide`
    - `close_window`
  - Reject missing, empty, whitespace-padded, unknown, and extra-colon inspector
    parameters.
  - Reject empty-colon and non-empty parameters for the no-parameter actions.
  - Forward `toggle_window_float_on_top` as `ROASTTY_ACTION_FLOAT_WINDOW` with
    `ROASTTY_FLOAT_WINDOW_TOGGLE`.
  - Forward `toggle_secure_input` as `ROASTTY_ACTION_SECURE_INPUT` with
    `ROASTTY_SECURE_INPUT_TOGGLE`.
  - Forward `close_window` as `ROASTTY_ACTION_CLOSE_WINDOW` with zeroed storage.
  - Forward all actions through the existing runtime `action_cb`, returning
    `false` for null, detached, and no-callback surfaces and otherwise returning
    the callback result.
  - Keep all previously supported binding actions unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage for the new action and enum constants.
  - Add malformed runtime-control action rejection checks.
  - Add no-callback coverage that valid runtime-control forwarding actions
    return `false` without crashing.

- Tests in `roastty/src/lib.rs`
  - Cover constants matching upstream values.
  - Cover invalid parser forms, including missing/empty/unknown/whitespace/
    extra-colon inspector values and no-parameter action parameters.
  - Cover null, detached, and no-callback surfaces returning `false`.
  - Cover valid runtime-control actions forwarding expected tags, target,
    storage payloads, zeroed storage tails, and callback result.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty runtime_control -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 724 design and found no technical blockers. The
review approved the planned action tags, enum payload storage, `close_window`
zero-storage forwarding, parser rejection cases, null/detached/no-callback
behavior, and test plan.

The review found one workflow blocker: this design-review section still said
`Pending.` This section now records the review outcome, and the README tuple is
`Codex/Codex/-`.

## Result

**Result:** Pass

Implemented runtime-control binding-action forwarding through the existing
runtime action callback path. Roastty now exposes upstream-matching action tags
for inspector, float-window, secure-input, and close-window actions, plus
inspector, float-window, and secure-input payload enums in
`roastty/include/roastty.h`.

`parse_binding_action` now accepts:

- `toggle_window_float_on_top`
- `toggle_secure_input`
- `inspector:toggle`
- `inspector:show`
- `inspector:hide`
- `close_window`

`toggle_window_float_on_top` forwards `ROASTTY_ACTION_FLOAT_WINDOW` with
`ROASTTY_FLOAT_WINDOW_TOGGLE`. `toggle_secure_input` forwards
`ROASTTY_ACTION_SECURE_INPUT` with `ROASTTY_SECURE_INPUT_TOGGLE`. Inspector
actions forward `ROASTTY_ACTION_INSPECTOR` with the selected inspector mode.
`close_window` forwards `ROASTTY_ACTION_CLOSE_WINDOW` with zeroed storage.

Invalid missing, empty, whitespace-padded, unknown, and extra-colon inspector
forms are rejected, and no-parameter actions reject empty-colon and non-empty
parameters.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty runtime_control -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Runtime-control binding actions now reach the app runtime with stable
upstream-shaped action tags and storage. The remaining nearby binding-action
work includes local surface state such as `toggle_mouse_reporting`, app-scoped
actions that need app/runtime dispatch semantics, and larger subsystems such as
real keybinding storage/dispatch.

## Completion Review

Codex reviewed the completed Experiment 724 result and found no implementation
blockers. The review approved the action constants, enum payload storage,
`close_window` zero-storage forwarding, parser false paths, callback-result
behavior, and Rust/C ABI coverage.

The review found one workflow blocker: result-review provenance was missing from
the experiment frontmatter and README tuple. This section, the `[review.result]`
frontmatter, and the README tuple now record the completion review.

Codex re-reviewed the revised result and found no remaining findings. The
completion review approved the result for commit.
