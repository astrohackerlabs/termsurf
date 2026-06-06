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

# Experiment 725: Binding Action Toggle Mouse Reporting

## Description

Experiment 724 finished the nearby runtime-control forwarding actions. Upstream
Ghostty's remaining adjacent `toggle_mouse_reporting` binding action is
different: it mutates surface-local configuration instead of forwarding an app
runtime action.

Upstream behavior:

- each surface has a `config.mouse_reporting` gate initialized from the
  `mouse-reporting` config option, defaulting to enabled;
- `toggle_mouse_reporting` flips that gate and consumes the binding action;
- mouse reports are sent only when both the surface gate is enabled and the
  terminal has an active mouse reporting mode;
- `surface_mouse_captured` follows the same combined gate.

Roastty already tracks terminal mouse modes and dispatches mouse reports, but it
does not have the separate surface-level gate yet. This experiment adds that
gate and the binding action. It does not implement full config parsing for the
`mouse-reporting` option; the initial Roastty value is the upstream default
`true`.

## Changes

- `roastty/src/lib.rs`
  - Add a `mouse_reporting: bool` field to `Surface`, initialized to `true`.
  - Extend `parse_binding_action` to accept `toggle_mouse_reporting` with no
    parameter and reject empty-colon or non-empty parameters.
  - Add a parsed binding-action variant or equivalent local action handling that
    toggles `Surface::mouse_reporting` and returns `true`.
  - Return `false` for null and detached surfaces, matching other local
    surface-mutating binding actions.
  - Update mouse reporting dispatch so mouse reports are suppressed when
    `mouse_reporting` is `false`, even if terminal mouse modes are active.
  - Update `roastty_surface_mouse_captured` so it returns `true` only when the
    surface is attached, has a worker, `mouse_reporting` is enabled, and the
    terminal has an active mouse reporting mode.
  - Keep all previously supported binding actions unchanged.

- `roastty/tests/abi_harness.c`
  - Add malformed `toggle_mouse_reporting` rejection checks.
  - Add valid no-worker coverage that `toggle_mouse_reporting` returns `true`
    without crashing.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for `toggle_mouse_reporting:` and
    `toggle_mouse_reporting:now`.
  - Cover null and detached surfaces returning `false`.
  - Cover toggling the surface gate from `true` to `false` and back to `true`.
  - Cover `roastty_surface_mouse_captured` honoring the combined surface gate
    plus terminal mouse mode.
  - Cover mouse button and scroll reporting being suppressed while the surface
    gate is disabled, without losing stored mouse position/button/scroll state.
  - Re-run existing binding-action and mouse tests to prove previous behavior
    remains unchanged.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty toggle_mouse_reporting -- --nocapture --test-threads=1`
- `cargo test -p roastty mouse -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the Experiment 725 design and found no technical blockers. The
review approved the surface-local gate, combined gate with terminal mouse mode,
`mouse_captured` behavior, no-worker consumption, parser false paths, and
suppression-without-state-loss test plan.

The review also accepted deferring full `mouse-reporting` config parsing while
initializing Roastty's surface-local gate to upstream's default `true`.

The review found one workflow blocker: this design-review section still said
`Pending.` This section now records the review outcome, and the README tuple is
`Codex/Codex/-`.
