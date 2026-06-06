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

# Experiment 693: Surface Mouse Scroll Reporting

## Description

Experiment 692 wired surface mouse button and pointer-position callbacks into
terminal mouse reporting. `roastty_surface_mouse_scroll` still only stores the
last finite scroll offset and packed scroll modifiers.

Upstream Ghostty normalizes scroll callbacks into integer scroll steps, emits
terminal wheel-button reports when mouse reporting is active, and otherwise
falls through to alternate-scroll or viewport scrolling behavior. Roastty
already has the mouse report encoder and dispatch helper from Experiment 692, so
this experiment wires only the reporting half of scroll handling.

This does not implement alternate-scroll cursor-key conversion, viewport
scrolling when reporting is disabled, selection clearing while reporting, mouse
scroll multiplier configuration, or platform-specific minimum non-precision
scroll behavior. Those are separate policy/front-end slices. This experiment
only makes active terminal mouse reporting receive scroll wheel events.

## Changes

- `roastty/src/lib.rs`
  - Extend `SurfaceMouseState` with pending fractional scroll accumulators for
    horizontal and vertical scroll offsets.
  - Add a scroll normalization helper that:
    - stores finite `x` / `y` offsets and the low byte of
      `roastty_input_scroll_mods_t`, preserving the existing state behavior;
    - treats non-precision scroll offsets as wheel ticks with whole-step
      accumulation: add the new offset to the per-axis pending value, emit no
      report while `abs(total) < 1`, emit `trunc(total)` steps toward zero when
      the threshold is reached, and preserve `total - steps` as residue;
    - treats precision scroll offsets as pixels and converts them to cell-sized
      steps using the same geometry source as mouse reporting, with the same
      threshold/truncation rule using the cell width or height as the unit and
      preserving sub-cell residue;
    - ignores zero-step scrolls after updating pending state.
  - For each normalized step while terminal mouse reporting is active, dispatch
    a press event at the last finite mouse position:
    - vertical positive/up reports `ROASTTY_MOUSE_BUTTON_FOUR`;
    - vertical negative/down reports `ROASTTY_MOUSE_BUTTON_FIVE`;
    - horizontal positive/right reports `ROASTTY_MOUSE_BUTTON_SIX`;
    - horizontal negative/left reports `ROASTTY_MOUSE_BUTTON_SEVEN`.
  - Keep the existing no-op behavior for null surfaces, detached surfaces,
    nonfinite offsets, missing workers, missing mouse reporting, unsupported
    encoded events, and worker queue failures. Queue failures should continue to
    record the existing termio error.
  - Add focused tests for:
    - disabled reporting stores scroll state but does not report;
    - vertical and horizontal reports update the last reported cell when
      reporting is active;
    - non-precision fractional offsets accumulate before reporting;
    - precision pixel offsets require at least one cell of accumulated movement;
    - zero/nonfinite/no-position/no-worker/detached cases remain safe no-ops.

- `roastty/tests/abi_harness.c`
  - No C ABI shape change is expected. Keep the existing smoke coverage for
    `roastty_surface_mouse_scroll`.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty surface_mouse -- --nocapture`
- `cargo test -p roastty mouse -- --nocapture`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex approved the revised design after the scroll normalization rule was
changed to upstream-style whole-step accumulation with truncation toward zero
and residue preservation for both non-precision tick offsets and precision pixel
offsets.
