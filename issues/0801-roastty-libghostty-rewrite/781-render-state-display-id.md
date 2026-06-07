+++
[implementer]
agent = "codex"
model = "gpt-5"
reasoning = "high"

[review.design]
agent = "codex"
model = "default"
reasoning = "medium"

[review.result]
agent = "codex"
model = "default"
reasoning = "medium"
+++

# Experiment 781: Render State Display ID

## Description

Deliver a surface display ID through the render-state C ABI.

The current surface ABI stores `display_id` through
`roastty_surface_set_display_id`, but render-state snapshots only expose
terminal-derived fields. A renderer consuming
`roastty_surface_render_state_update` therefore cannot associate the current
surface snapshot with the display ID the frontend assigned.

This experiment adds a narrow render-state scalar for the already-stored surface
display ID. It does not implement renderer presentation, frontend selection
routing, split trees, or OSC 52 request handling.

## Changes

- `roastty/include/roastty.h`
  - Add `ROASTTY_RENDER_STATE_DATA_DISPLAY_ID` as the next
    `roastty_render_state_data_e` value.
- `roastty/src/lib.rs`
  - Add the matching Rust constant.
  - Add `display_id: u32` to `RenderStateScalar`, defaulting to zero.
  - Keep terminal-only `roastty_render_state_update` snapshots at display ID
    zero.
  - Make `roastty_surface_render_state_update` copy `surface.display_id` into
    the render-state snapshot after reading the worker terminal.
  - Make `roastty_render_state_get` and `roastty_render_state_get_multi` expose
    the display ID as a `u32`.
  - Update render-state ABI layout/value tests and add focused surface snapshot
    coverage for default and changed display IDs.
  - Add direct coverage that a terminal-only `roastty_render_state_update`
    resets a state that previously held a nonzero surface display ID back to
    zero.
- `roastty/tests/abi_harness.c`
  - Update enum value assertions.
  - Assert pre-update/default render states expose display ID zero.
  - Assert `ROASTTY_RENDER_STATE_DATA_DISPLAY_ID` works in a multi-get batch,
    writes a `uint32_t`, and reports `out_written` correctly.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - Update checklist wording only if the implementation and tests prove the
    scalar delivery is complete.
  - Use scoped wording: render-state display ID scalar done, while broader
    renderer consumption/delivery, frontend routing, and full presentation stay
    missing.

## Verification

- Run focused Rust tests:
  - `cargo test -p roastty render_state_c_abi -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_render_state_update -- --nocapture --test-threads=1`
  - `cargo test -p roastty surface_set_display_id -- --nocapture --test-threads=1`
- Run the C ABI harness:
  - `cargo test -p roastty --test abi_harness -- --nocapture`
- New or updated assertions must cover:
  - pre-update render states expose display ID zero;
  - terminal-only render-state updates expose display ID zero;
  - terminal-only render-state updates reset a state previously populated by a
    surface render-state update with a nonzero display ID;
  - surface render-state updates expose the latest surface display ID;
  - direct `roastty_render_state_get` writes the display ID as `u32`;
  - `roastty_render_state_get_multi` can include display ID in a batch and
    updates `out_written` correctly.
- Run:
  - `cargo fmt -p roastty`
  - `cargo fmt -p roastty -- --check`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/781-render-state-display-id.md`
- Run:
  - `git diff --check`

The experiment passes if render-state snapshots expose display ID zero by
default, surface render-state updates expose the latest display ID stored on the
surface, terminal-only updates reset display ID to zero, existing render-state
data validation accepts the new enum value, `get_multi` handles the new scalar,
and focused Rust plus C ABI verification passes. It is Partial if only the Rust
ABI path can be proven. It fails if adding a display ID scalar requires broader
renderer or frontend changes to be meaningful.

## Design Review

Codex reviewed the initial design and approved the narrow slice, but found four
real plan gaps: the C ABI harness command was optional instead of required,
`get_multi` coverage for the new scalar was not explicit, terminal-only update
reset/staleness coverage was missing, and README checklist wording needed an
overclaim guard.

The design was updated to require
`cargo test -p roastty --test abi_harness -- --nocapture`, add explicit
`get_multi` and `out_written` assertions, test terminal-only reset from a
previous nonzero surface snapshot, and constrain checklist updates to
render-state display ID scalar delivery while leaving broader renderer and
frontend work missing.

## Result

**Result:** Pass

Render-state snapshots now carry a `display_id` scalar. The C ABI appends
`ROASTTY_RENDER_STATE_DATA_DISPLAY_ID = 19`, the Rust render-state snapshot
stores a `u32` display ID defaulting to zero, direct `roastty_render_state_get`
and batched `roastty_render_state_get_multi` expose it, and
`roastty_surface_render_state_update` copies the latest `surface.display_id`
into the snapshot after reading the worker terminal.

Terminal-only snapshots still expose zero, including after a render state
previously received a nonzero surface display ID. Surface snapshots expose zero
before a display ID is assigned and the latest nonzero display ID after
`roastty_surface_set_display_id`.

The C ABI harness also now runs with isolated `HOME` and `XDG_CONFIG_HOME`
directories, and its zero-diagnostic config sanity load uses `/dev/null` rather
than a missing `/tmp` path. This keeps the harness deterministic and exercises
the render-state ABI assertions without changing product config behavior.

Verification passed:

- `cargo test -p roastty render_state_c_abi -- --nocapture --test-threads=1` — 8
  passed
- `cargo test -p roastty surface_render_state_update -- --nocapture --test-threads=1`
  — 6 passed
- `cargo test -p roastty surface_set_display_id -- --nocapture --test-threads=1`
  — 5 passed
- `cargo test -p roastty --test abi_harness -- --nocapture` — 1 passed
- `cargo fmt -p roastty`
- `cargo fmt -p roastty -- --check`
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/781-render-state-display-id.md`
- `git diff --check`

## Conclusion

Roastty now has a tested ABI scalar for renderer-facing display ID snapshots.
This closes the data handoff from `Surface` storage into render state, while
leaving actual renderer consumption, frontend routing, and presentation work for
later experiments.
