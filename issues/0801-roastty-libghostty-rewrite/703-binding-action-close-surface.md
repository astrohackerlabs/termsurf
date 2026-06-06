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

# Experiment 703: Binding Action Close Surface

## Description

Experiment 702 added `roastty_surface_binding_action` for the split action
strings Roastty could already forward through the runtime action callback.
Upstream Ghostty also supports `close_surface` through the same
`Binding.Action.parse` / `Surface.performBindingAction` path:

- `close_surface` has no parameter;
- `performBindingAction` calls `Surface.close()`;
- `Surface.close()` forwards the computed confirm-close state to the runtime
  surface close callback;
- after the switch completes, `performBindingAction` returns `true`.

Roastty already has the pieces needed for this one action:

- `roastty_surface_request_close`;
- `Surface::request_close`;
- confirm-close policy computation;
- close callback tests.

This experiment extends the existing binding-action foundation to support
`close_surface`:

- parse `close_surface` with no parameter;
- reject `close_surface:*` as malformed, matching upstream void-action parsing;
- invoke `Surface::request_close`;
- return `true` for attached surfaces after a parsed `close_surface`, even when
  the runtime has no close callback, matching upstream's runtime-optional close
  forwarding;
- return `false` for null or detached surfaces before invoking anything.

This does not implement `close_tab`, `close_window`, `quit`, app-scoped actions,
full `Binding.Action` parsing, frontend split/tab/window mutation, or surface
deallocation from the binding action itself.

## Changes

- `roastty/src/lib.rs`
  - Replace the split-only parsed action representation with a small internal
    parsed binding action enum that can represent:
    - runtime action callback forwarding for existing split actions;
    - surface close request for `close_surface`.
  - Extend `parse_binding_action` to accept `close_surface` with no parameter.
  - Update `roastty_surface_binding_action` to dispatch parsed close actions via
    `Surface::request_close` and return `true` for attached surfaces.
  - Keep existing split-action parsing and callback return semantics unchanged.

- `roastty/tests/abi_harness.c`
  - Add smoke coverage that `close_surface:now` is rejected and `close_surface`
    can be called through the binding-action ABI.

- Tests in `roastty/src/lib.rs`
  - Cover `close_surface` invoking the runtime close callback with surface
    userdata.
  - Cover confirm-close policy flowing through `close_surface` binding actions.
  - Cover `close_surface` returning true without a close callback on an attached
    surface.
  - Cover null and detached surfaces returning false with no close callback side
    effects.
  - Cover malformed `close_surface:*` returning false.
  - Re-run the existing split binding-action tests to prove their semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty request_close -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the staged Experiment 703 design and approved the technical scope
with no implementation blockers. The review confirmed that upstream
`close_surface` is a void binding action, rejects colon parameters, calls
`Surface.close()`, and then returns `true` from `performBindingAction`.

The review also confirmed that Roastty's `Surface::request_close` already has
optional runtime callback behavior, so returning `true` for an attached parsed
`close_surface` action even when no close callback is installed matches the
upstream action-performed semantics. The only initial findings were workflow
items: record this design-review section and update the README provenance tuple
before the plan commit.

## Result

**Result:** Pass

Implemented `close_surface` support in the binding-action ABI:

- Introduced an internal parsed binding-action enum so parsed strings can route
  either to runtime action callback forwarding or to surface close requests.
- Extended `parse_binding_action` to accept `close_surface` with no parameter.
- Kept `close_surface:*` malformed and unsupported, matching upstream void
  action parsing.
- Made `roastty_surface_binding_action(surface, "close_surface", 13)` call
  `Surface::request_close` and return `true` for attached surfaces.
- Preserved false return behavior for null and detached surfaces.
- Preserved split binding-action callback return semantics.
- Added Rust tests for close callback invocation, confirm-close policy,
  no-callback true behavior, null/detached false behavior, and malformed
  `close_surface:*`.
- Added C ABI harness smoke coverage for `close_surface` and
  `close_surface:now`.

Verification passed:

- `cargo fmt -p roastty`
- `cargo test -p roastty binding_action -- --nocapture`
- `cargo test -p roastty request_close -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Roastty binding-action invocation now covers both split actions and
`close_surface`, using existing surface close policy and runtime callback
plumbing. Remaining binding-action parity still requires complete
`Binding.Action` parsing, app-scoped actions, terminal text/CSI/ESC actions,
clipboard actions, close tab/window actions, key tables, and frontend
split/tab/window mutation.

## Completion Review

Codex reviewed the staged completed Experiment 703 result. The review found no
implementation blockers: `parse_binding_action` now accepts only parameterless
`close_surface`, malformed `close_surface:*` is rejected, the dispatch path
calls `Surface::request_close`, attached surfaces return `true`, detached
surfaces return `false`, and split action callback-return behavior is preserved.

The review accepted the Rust tests and C harness coverage for this slice. It
initially blocked the result commit only because the README provenance tuple
still showed the result review as pending. This section and the README tuple
update resolve that workflow finding.
