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

# Experiment 728: Binding Action Undo Redo

## Description

Experiment 727 completed `copy_title_to_clipboard` runtime forwarding. The next
small upstream binding-action gap is `undo` / `redo`.

Upstream Ghostty documents `undo` and `redo` as app-scoped actions, but
`Surface.performBindingAction` special-cases them: when a binding is triggered
from a surface, they forward to the runtime app with the surface target and no
payload. This lets the runtime decide whether there is a surface-local or
terminal undo/redo operation available.

Roastty does not currently expose the `undo` / `redo` runtime action tags or
parse those binding actions. This experiment adds only the upstream-shaped
surface-triggered forwarding path.

## Changes

- `roastty/include/roastty.h`
  - Add `ROASTTY_ACTION_UNDO = 51` and `ROASTTY_ACTION_REDO = 52`, matching
    upstream `apprt.Action.Key`.
  - Document that both actions have zeroed storage.

- `roastty/src/lib.rs`
  - Add matching Rust action constants.
  - Extend `parse_binding_action` to accept `undo` and `redo` with no parameter
    and reject empty-colon or non-empty parameters.
  - Forward both actions through the existing surface-targeted `RuntimeAction`
    path with zeroed storage.
  - Preserve false-path behavior for null surfaces, detached surfaces, and
    missing runtime action callbacks.

- `roastty/tests/abi_harness.c`
  - Assert the new ABI action tags.
  - Add malformed `undo` / `redo` rejection checks.
  - Add valid no-callback coverage returning `false`.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for `undo:`, `undo:now`, `redo:`, and `redo:now`.
  - Cover null, detached, and missing-callback cases returning `false`.
  - Cover forwarding to the action callback with target surface, action tags
    51/52, and zeroed storage.
  - Cover callback result propagation.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty undo -- --nocapture --test-threads=1`
- `cargo test -p roastty redo -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 728 design and found no technical blockers. The
review approved the surface-triggered forwarding behavior, ABI tags 51/52,
zeroed storage, strict no-parameter parsing, existing runtime callback false
paths, and Rust/C ABI test plan.

The review found one workflow blocker: this design-review section still said
`Pending.` This section now records the review outcome, and the README tuple is
`Codex/Codex/-`.

## Result

**Result:** Pass

Roastty now exposes `ROASTTY_ACTION_UNDO = 51` and `ROASTTY_ACTION_REDO = 52`,
matching upstream `apprt.Action.Key`, and documents both as zero-storage runtime
actions.

`undo` and `redo` now parse as strict parameterless binding actions and forward
through the existing surface-targeted runtime action path. They return `false`
for null surfaces, detached surfaces, missing callbacks, or false callbacks, and
return the callback result when a runtime action callback is present.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty undo -- --nocapture --test-threads=1` — 3 passed
- `cargo test -p roastty redo -- --nocapture --test-threads=1` — 3 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1` — 97
  passed
- `cargo test -p roastty --test abi_harness` — 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

During verification, the broad binding-action filter exposed a timing-sensitive
existing text test that captured the first render tick before the expected child
PTY output arrived. That test now waits for its expected `line:hello` output
using the existing `surface_snapshot_text_until` helper, and the full
binding-action filter passes.

## Conclusion

`undo` and `redo` are complete for Roastty's current surface-triggered runtime
action model. The ABI tags, parser behavior, surface-targeted forwarding,
callback result propagation, Rust tests, and C ABI coverage all match the
experiment plan.

## Completion Review

Codex reviewed the completed experiment and found one workflow blocker: this
completion-review section still said `Pending.` The review found no
implementation blockers.

The review approved ABI tags 51/52, the zero-storage convention, strict parser
behavior, surface-targeted runtime forwarding, callback false/result behavior,
Rust tests, C ABI harness coverage, verification record, and README
status/provenance.
