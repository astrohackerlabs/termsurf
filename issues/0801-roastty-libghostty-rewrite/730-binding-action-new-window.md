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

# Experiment 730: Binding Action New Window

## Description

Experiment 729 added the simple zero-storage app-scoped binding actions.
`new_window` remains the next app-related upstream binding-action gap, but it is
not part of that simple app-target group.

Upstream Ghostty handles a surface-triggered `new_window` binding specially:
`Surface.performBindingAction` calls `app.newWindow` with the current surface as
the parent, and `App.newWindow` then performs the runtime `new_window` action
with the parent surface target when the parent is still attached. In other
words, a binding action triggered from a surface reaches the runtime callback as
`new_window` with a surface target, not as a plain app-target action.

Roastty already forwards `new_tab`, `close_tab`, and split/window actions
through the runtime action callback using `ROASTTY_TARGET_SURFACE`. This
experiment adds `new_window` to that same surface-target forwarding path.

## Changes

- `roastty/include/roastty.h`
  - Add upstream-aligned `ROASTTY_ACTION_NEW_WINDOW = 1`.
  - Document `ROASTTY_ACTION_NEW_WINDOW` as zero-storage.

- `roastty/src/lib.rs`
  - Add matching `ROASTTY_ACTION_NEW_WINDOW = 1`.
  - Extend `parse_binding_action` to accept parameterless `new_window`.
  - Reject `new_window:` and non-empty parameters such as `new_window:now`.
  - Forward parsed `new_window` bindings through the existing surface-target
    runtime action path, producing `target.tag = ROASTTY_TARGET_SURFACE`,
    `target.surface = surface`, action tag `ROASTTY_ACTION_NEW_WINDOW`, and
    zeroed storage.
  - Preserve the existing app-target path from Experiment 729 for the eight
    simple app actions.

- `roastty/tests/abi_harness.c`
  - Assert `ROASTTY_ACTION_NEW_WINDOW == 1`.
  - Add malformed `new_window` parser rejection checks.
  - Add valid no-callback coverage returning `false`.

- Tests in `roastty/src/lib.rs`
  - Add the action constant assertion.
  - Cover parser false paths for `new_window:` and `new_window:now`.
  - Cover null, detached, and missing-callback cases returning `false`.
  - Cover forwarding to the runtime callback with surface target, the parent
    surface pointer, action tag `ROASTTY_ACTION_NEW_WINDOW`, and zeroed storage.
  - Cover callback result propagation.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty new_window -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 730 design and found one workflow blocker: the
review result had not yet been recorded in the experiment frontmatter, this
section, or the README tuple. This section and the `[review.design]` frontmatter
now record the design-review outcome, and the README tuple is `Codex/Codex/-`.

The review found no technical design blockers. It approved keeping `new_window`
out of the app-target group, using upstream action tag `1`, forwarding from
surface-triggered bindings with `ROASTTY_TARGET_SURFACE` and the triggering
surface pointer, rejecting parameters, and covering the Rust parser/runtime path
plus C ABI assertions.

## Result

**Result:** Pass

Experiment 730 added `new_window` as upstream action tag `1` and wired the
parameterless `new_window` binding action through the existing surface-target
runtime action path. When triggered from a surface, the runtime callback now
receives `target.tag = ROASTTY_TARGET_SURFACE`, `target.surface = surface`,
`action.tag = ROASTTY_ACTION_NEW_WINDOW`, and zeroed storage.

The parser rejects `new_window:` and non-empty parameters, while null surfaces,
detached surfaces, missing callbacks, and false callback results all return
`false`.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty new_window -- --nocapture --test-threads=1`
  - 3 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
  - 104 passed
- `cargo test -p roastty --test abi_harness`
  - 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Surface-triggered `new_window` bindings now match upstream's effective runtime
target shape: they create a `new_window` runtime action against the triggering
surface, allowing embedders to inherit parent-surface context. The app-target
actions added in Experiment 729 remain separate and unchanged.

## Completion Review

Codex reviewed the completed Experiment 730 diff and found one workflow blocker:
the result was recorded, but completion-review provenance had not yet been added
to the experiment frontmatter, this section, or the README tuple. This section,
the `[review.result]` frontmatter, and the README tuple now record that review.

The review found no implementation blockers. It approved the stable
`ROASTTY_ACTION_NEW_WINDOW = 1` tag, parameter rejection, surface-target
forwarding with the triggering surface pointer and zeroed storage, preservation
of separate app-target actions, and focused Rust plus C ABI coverage.
