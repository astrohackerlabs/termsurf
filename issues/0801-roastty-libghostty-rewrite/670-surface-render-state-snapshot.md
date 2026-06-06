+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"
+++

# Experiment 670: Surface Render-State Snapshot

## Description

Experiment 669 lets `roastty_app_tick` drain attached termio worker events into
surface presentation state, but the frontend still cannot turn a dirty surface
into a renderable terminal snapshot. Roastty already has render-state snapshot
machinery for standalone `roastty_terminal_t`; this experiment bridges an
attached surface worker terminal into that existing render-state ABI.

This is a narrow frontend-facing presentation slice. It does not start workers
from surface configuration, choose shells, schedule renderer frames, add a
mailbox, or implement the full Ghostty draw/refresh lifecycle. It only lets the
frontend ask whether a surface has dirty terminal state and copy the attached
worker terminal into a `roastty_render_state_t`.

## Changes

- `roastty/include/roastty.h`
  - Add `roastty_surface_needs_render(roastty_surface_t) -> bool`.
  - Add
    `roastty_surface_render_state_update(roastty_surface_t, roastty_render_state_t) -> roastty_result_e`.
- `roastty/src/lib.rs`
  - Implement `roastty_surface_needs_render(surface)` by returning the stored
    surface dirty flag, or `false` for null handles. Like the existing raw
    handle ABI, arbitrary stale non-null handles are caller misuse and are not
    validated in this experiment.
  - Implement `roastty_surface_render_state_update(surface, state)`:
    - return `ROASTTY_INVALID_VALUE` for null surface or render-state handles;
    - return `ROASTTY_NO_VALUE` when the surface has no attached termio worker;
    - use `TermioWorker::with_termio` to snapshot the worker terminal through
      the existing `render_state_from_terminal` helper;
    - clear the surface dirty flag only after a successful snapshot.
  - Keep worker attachment internal/test-only. Public worker launch and renderer
    wakeup remain deferred.
- Tests in `roastty/src/lib.rs`
  - Attach a test worker that prints `hello`, tick until the surface is dirty,
    call `roastty_surface_render_state_update`, and assert:
    - it returns `ROASTTY_SUCCESS`;
    - `roastty_surface_needs_render` becomes false;
    - the render-state rows contain `hello`.
  - Verify `roastty_surface_render_state_update` returns `ROASTTY_NO_VALUE` for
    a surface without an attached worker and leaves dirty state unchanged.
  - Verify null surface/null render-state arguments return
    `ROASTTY_INVALID_VALUE`.
  - Verify a dirty surface remains dirty when snapshot update fails.
  - Continue using `os::pty::PTY_COMMAND_LOCK` for worker subprocess tests.

## Design Review

**Result:** Approved after amendment.

Codex found one blocker: the initial design promised behavior for arbitrary
invalid non-null handles, but the current Roastty C ABI helpers only check null
before casting raw handles and cannot safely identify stale pointers.

The design now matches the existing ABI contract: null surface/render-state
handles are rejected, while arbitrary stale non-null handles remain caller
misuse and are not validated by this experiment.

## Verification

- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/670-surface-render-state-snapshot.md`
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `cargo test -p roastty surface`
- `cargo test -p roastty render_state`
- `cargo test -p roastty termio`
- `git diff --check`
