# Experiment 180: Custom Shader Animation Runtime

## Description

Experiment 179 narrowed the renderer residual to concrete remaining behavior.
The smallest renderer slice is `custom-shader-animation`: pinned Ghostty stores
the parsed config in `renderer/Thread.zig`, starts the draw timer only when the
renderer has animations, and applies the policy as:

- `always` — animate when focused or unfocused;
- `true` — animate only while focused;
- `false` — never animate.

Roastty already parses `custom-shader-animation` and has the
`CustomShaderAnimation::should_animate(focused)` truth-table helper, but the
live present loop is not yet guarded by that config. This experiment will wire
the helper into the live custom-shader tick/present path and prove the policy
with focused runtime tests.

The scope is only custom-shader animation scheduling. Background image
rendering/options, `window-colorspace`, `alpha-blending`, and
`scroll-to-bottom.output` remain in the renderer residual row.

## Changes

- `roastty/src/lib.rs`
  - Add a small runtime helper that decides whether a present tick should render
    an otherwise-clean custom-shader frame, based on whether custom shader
    pipelines are active, the parsed `custom_shader_animation` policy, and the
    current focus state.
  - Use that helper in the present tick path so animated custom shaders keep
    rendering under Ghostty's `always`/focused/never policy without forcing
    unrelated clean frames to redraw.
  - Add focused tests for `always`, `true`, and `false`, including no-pipeline
    behavior and config/focus changes.
- `issues/0805-roastty-ghostty-parity/custom_shader_animation_runtime_parity.py`
  - Add a static guard that checks pinned Ghostty's
    `renderer/Thread.zig::syncDrawTimer` policy, Roastty's parsed config helper,
    the runtime tick integration, the focused tests, and the inventory split.
- `issues/0805-roastty-ghostty-parity/config_runtime_inventory.py`
  - Split a new oracle-complete runtime row for `custom-shader-animation`
    focus/always/false draw-timer policy.
  - Narrow `RUNTIME-008B2B2B2B2B` to the remaining renderer-visible background
    image/options, `window-colorspace`, `alpha-blending`, and
    `scroll-to-bottom.output` behavior.
- `issues/0805-roastty-ghostty-parity/config-runtime-inventory.md`
  - Regenerate from the inventory script.
- `issues/0805-roastty-ghostty-parity/config-matrix.md`
  - Regenerate CFG-223 counts from the inventory script. CFG-223 should remain
    `Gap`.
- `issues/0805-roastty-ghostty-parity/README.md`
  - Add a learning for the draw-timer policy if the implementation succeeds.

## Verification

Pass criteria:

- `cargo fmt --manifest-path roastty/Cargo.toml`
- `cargo test --manifest-path roastty/Cargo.toml custom_shader_animation -- --test-threads=1`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/custom_shader_animation_runtime_parity.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/renderer_visual_residual_audit.py`
- `PYTHONDONTWRITEBYTECODE=1 python3 issues/0805-roastty-ghostty-parity/config_runtime_inventory.py --output issues/0805-roastty-ghostty-parity/config-runtime-inventory.md --matrix issues/0805-roastty-ghostty-parity/config-matrix.md`
- `PYTHONDONTWRITEBYTECODE=1 python3 -m py_compile issues/0805-roastty-ghostty-parity/*.py`
- `prettier --write --prose-wrap always --print-width 80 issues/0805-roastty-ghostty-parity/180-custom-shader-animation-runtime.md issues/0805-roastty-ghostty-parity/README.md issues/0805-roastty-ghostty-parity/config-runtime-inventory.md issues/0805-roastty-ghostty-parity/config-matrix.md`
- `git diff --check`

The experiment passes only if the runtime tick behavior is proven for all three
policy values and the remaining renderer gap no longer lists
`custom-shader-animation`.

## Design Review

Fresh-context adversarial design review returned **Approved** with no required
findings. The reviewer confirmed the README link, required sections, narrow
scope, fidelity to pinned Ghostty `syncDrawTimer`, and verification coverage for
all three policy values plus no-pipeline/config/focus behavior.
