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

# Experiment 719: Binding Action Font Size

## Description

Experiment 718 added clipboard paste binding actions. Upstream Ghostty's next
surface-scoped binding-action group adjusts runtime font size:

- `increase_font_size:<points>`
- `decrease_font_size:<points>`
- `reset_font_size`
- `set_font_size:<points>`

Roastty already carries `font_size` in `RoasttySurfaceConfig`, but the surface
does not store current/original font-size state yet. This experiment adds the
binding parser and state transition slice. It preserves Ghostty's point-size
clamping behavior and render invalidation, while leaving actual CoreText/font
stack rebuilds and renderer font atlas updates for the later font/render
subsystem work.

This does not add public font-size getter/setter ABI, font discovery, shaping,
glyph atlas rebuilds, inherited-window font-size behavior, config reload
font-size preservation, or renderer metric recomputation.

## Changes

- `roastty/src/lib.rs`
  - Add a `DEFAULT_FONT_SIZE_POINTS` constant of `13.0`, matching upstream
    Ghostty's macOS default `font-size`.
  - Store `font_size_points`, `original_font_size_points`, and
    `font_size_adjusted` on `Surface`.
  - Initialize both font-size point fields from `RoasttySurfaceConfig.font_size`
    when it is finite and positive, clamped to `1.0..=255.0`; otherwise use the
    macOS upstream default `13.0`.
  - Extend the internal parsed binding-action enum with:
    - `IncreaseFontSize(f32)`
    - `DecreaseFontSize(f32)`
    - `ResetFontSize`
    - `SetFontSize(f32)`
  - Extend `parse_binding_action` to accept:
    - `increase_font_size:<f32>`
    - `decrease_font_size:<f32>`
    - `reset_font_size`
    - `set_font_size:<f32>`
  - Add a font-size-specific finite, whitespace-rejecting ASCII `f32` parser so
    huge finite values are accepted and then clamped by font-size semantics
    instead of being rejected by the existing scroll-fraction parser's range
    cap.
  - Reject missing, empty, whitespace-padded, NaN, infinity, and extra-colon
    parameters where a number is required; reject any parameter for
    `reset_font_size`.
  - Add a surface helper that:
    - returns `false` for null and detached surfaces through the existing
      dispatcher guards;
    - clamps increase/decrease deltas to `0.0..=255.0`, matching upstream;
    - clamps current size to `1.0..=255.0`, matching upstream;
    - marks increase, decrease, and set actions as manually adjusted;
    - clears manual adjustment for reset;
    - requests render after every valid font-size action.
  - Keep clipboard, split, close, text/CSI/ESC, reset, clear-screen, scroll,
    prompt-jump, select-all, and adjust-selection semantics unchanged.

- `roastty/tests/abi_harness.c`
  - Add C ABI smoke coverage that malformed font-size action forms are rejected.
  - Add no-worker/no-renderer coverage that valid font-size actions still
    consume successfully without crashing.

- Tests in `roastty/src/lib.rs`
  - Cover parser false paths for missing, empty, whitespace-padded, NaN,
    infinity, extra-colon, and reset-with-parameter forms.
  - Cover huge finite font-size values clamping to `255.0` instead of being
    rejected.
  - Cover null and detached surfaces returning `false`.
  - Cover default and configured initial font-size state.
  - Cover increase, decrease, reset, and set state transitions, clamp bounds,
    manual-adjustment flag behavior, and render requests.
  - Re-run existing binding-action tests to prove previous action semantics did
    not change.

## Verification

Run:

- `cargo fmt -p roastty`
- `cargo test -p roastty font_size -- --nocapture --test-threads=1`
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1`
- `cargo test -p roastty --test abi_harness`
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Design Review

Codex reviewed the initial Experiment 719 design and found the scope otherwise
sound: a surface state and render-invalidation slice is acceptable while
CoreText/font atlas rebuilds remain deferred, and the planned tests cover state
transitions, clamps, manual-adjustment flags, render requests, null/detached
returns, no-worker consumption, and parser false paths.

The review raised one technical blocker: the design reused the existing
`parse_f32_ascii`, which carries a range cap for scroll-fraction parsing.
Upstream font-size actions parse finite values and then clamp them, so a huge
finite value should clamp to `255.0` instead of being rejected. The plan now
adds a font-size-specific finite parser without the scroll-fraction range cap
and tests huge finite values.

The review also raised the normal workflow provenance requirement. Design-review
frontmatter and this section are now present, and the README provenance tuple
will be updated to `Codex/Codex/-` before the plan commit. Result-review
provenance will be added only after implementation and completion review.

Codex re-reviewed the revised design and found no remaining blockers. The review
approved the font-size-specific parser and huge finite value clamp test plan.

## Result

**Result:** Pass

Implemented font-size binding-action parsing and dispatch for
`increase_font_size`, `decrease_font_size`, `reset_font_size`, and
`set_font_size`. Surfaces now store current font size, original configured font
size, and manual-adjustment state. The action behavior matches the upstream
state slice: deltas clamp to `0.0..=255.0`, current point size clamps to
`1.0..=255.0`, reset restores the original configured size, and each valid
action requests render.

Roastty now initializes surface font-size state from
`RoasttySurfaceConfig.font_size` when valid, clamps oversized configured values
to `255.0`, and falls back to Ghostty's macOS default `13.0` for unset or
invalid values. Actual CoreText/font stack rebuilds and renderer metric updates
remain deferred to the font/render subsystem.

The C ABI harness now covers malformed font-size action rejection and valid
font-size actions consuming successfully without a worker or renderer.

Verification:

- `cargo fmt -p roastty`
- `cargo test -p roastty font_size -- --nocapture --test-threads=1` — 4 passed
- `cargo test -p roastty binding_action -- --nocapture --test-threads=1` — 70
  passed
- `cargo test -p roastty --test abi_harness` — 1 passed
- `cargo fmt -p roastty -- --check`
- `git diff --check`

## Conclusion

Roastty now preserves the stateful behavior of Ghostty's font-size binding
actions at the surface level. The remaining font-size work is the larger
font/render integration: CoreText font stack rebuilds, glyph atlas and metrics
updates, inherited-window font-size behavior, config reload preservation, and
public app-facing notification or inspection as needed by the Swift frontend.

## Completion Review

Codex reviewed the completed Experiment 719 implementation and result record.
The review found one workflow blocker: result-review provenance was not yet
recorded in the experiment frontmatter or README tuple. This file now includes
`[review.result]`, and the README provenance tuple has been updated to
`Codex/Codex/Codex`.

The review found no implementation blockers. It approved the font-size-specific
parser, finite huge-value clamping, NaN/infinity/whitespace/extra-colon/missing
parameter rejection, configured/default initialization, state transitions,
manual-adjustment flag behavior, render invalidation, no-worker state-only
consumption, C ABI smoke coverage, and binding-action regression coverage.
