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

# Experiment 809: Render Pass Sampler Binding

## Description

Wire the Metal sampler wrapper from Experiment 808 into
`roastty/src/renderer/metal/render_pass.rs`.

Upstream `renderer/metal/RenderPass.zig::Step` carries a `samplers` slice and
binds each present sampler with `setFragmentSamplerState:atIndex:` before the
draw call. Roastty currently binds buffers and textures but has no sampler field
on `MetalRenderPassStep`. This experiment adds the sampler binding path without
attempting window `Target`, `IOSurfaceLayer`, or full live frame orchestration.

The current Roastty Metal shaders use constexpr samplers for the existing
texture-sampling paths, so this is parity plumbing for the render-pass API and
future shader paths rather than a new visual feature.

## Changes

- `roastty/src/renderer/metal/render_pass.rs`
  - Import `MetalSampler`.
  - Add `samplers: &[Option<&MetalSampler>]` to `MetalRenderPassStep`.
  - Add a helper equivalent to upstream's sampler loop that binds each present
    sampler to its fragment sampler slot with `setFragmentSamplerState_atIndex`.
  - Call sampler binding after texture binding and before the draw call.
  - Update existing tests and call sites with `samplers: &[]`.
  - Add a focused device-backed smoke test that creates a `MetalSampler`, passes
    it through a texture-sampling render-pass step, completes the command frame,
    and verifies the rendered output still matches the expected sampled pixels.
    This proves the binding path is valid with `objc2-metal` and does not break
    existing draw behavior even though current shaders use constexpr samplers.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - After implementation, update the Metal checklist row to mention render-pass
    sampler binding while keeping window `Target`, `IOSurfaceLayer`, and full
    live frame orchestration open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/metal/RenderPass.zig`
  - `roastty/src/renderer/metal/render_pass.rs`
  - `roastty/src/renderer/metal/sampler.rs`
  - `roastty/src/renderer/metal/shaders.metal`
- Run:
  - `cargo fmt -p roastty`
  - `cargo test -p roastty metal::render_pass -- --nocapture --test-threads=1`
  - `cargo test -p roastty metal::sampler -- --nocapture --test-threads=1`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/809-render-pass-sampler-binding.md`
- Run:
  - `git diff --check`

The experiment passes if `MetalRenderPassStep` supports sampler binding with
tested command-frame execution while the Metal row remains partial for window
`Target`, `IOSurfaceLayer`, and full live frame orchestration. It is Partial if
the step field lands but the smoke test exposes a binding issue needing
follow-up. It fails if sampler binding cannot be cleanly expressed with current
`objc2-metal` bindings.

## Design Review

Codex reviewed the design and approved it with no findings. The review confirmed
that adding a sampler slice to `MetalRenderPassStep`, binding present entries
fragment-only by index, and doing that before the draw call matches upstream
`RenderPass.zig`. The review also noted that the planned smoke test is
meaningful as a Metal encoder/API check, not proof of changed shader sampling
semantics, because current Roastty shaders use constexpr samplers. The scoped
README update keeps `Target`, `IOSurfaceLayer`, and full live frame
orchestration open.

## Result

**Result:** Pass

`roastty/src/renderer/metal/render_pass.rs` now wires Metal samplers into render
pass steps:

- `MetalRenderPassStep` has a `samplers: &[Option<&MetalSampler>]` field.
- `bind_step_samplers` binds each present sampler to its fragment sampler slot
  with `setFragmentSamplerState_atIndex`.
- `MetalRenderPass::step` binds samplers after textures and before the draw
  call.
- Existing render-pass call sites pass `samplers: &[]`.
- The image render-pass smoke test now creates a real `MetalSampler`, passes it
  through a texture-sampling step, completes the command frame, and verifies the
  rendered pixels remain correct.

The current Metal shaders still use constexpr samplers, so this proves the
encoder binding path is valid and non-regressive. It does not claim changed
shader sampling semantics.

The Issue 801 Metal checklist now records render-pass sampler binding as present
while keeping window `Target`, `IOSurfaceLayer`, and full live frame
orchestration open.

Verification:

- Inspected `vendor/ghostty/src/renderer/metal/RenderPass.zig`.
- Inspected `roastty/src/renderer/metal/render_pass.rs`.
- Inspected `roastty/src/renderer/metal/sampler.rs`.
- Inspected `roastty/src/renderer/metal/shaders.metal`.
- `cargo fmt -p roastty` — passed.
- `cargo test -p roastty metal::render_pass -- --nocapture --test-threads=1` —
  passed, 28 tests.
- `cargo test -p roastty metal::sampler -- --nocapture --test-threads=1` —
  passed, 4 tests.
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/809-render-pass-sampler-binding.md`
  — passed.
- `git diff --check` — passed.

## Conclusion

Experiment 809 completes the render-pass sampler binding path needed by future
shader and image work. The Metal renderer row remains partial because window
`Target`, `IOSurfaceLayer`, and full live frame orchestration are still missing.

## Completion Review

Codex reviewed the staged result and approved it with no findings. The approval
confirmed that `MetalRenderPassStep` and `bind_step_samplers` match upstream's
fragment-only sampler loop, `MetalSampler` ownership is sound for borrowed
encoding, the smoke test honestly verifies encoder/API plumbing without claiming
changed shader sampler semantics, and the docs keep `Target`, `IOSurfaceLayer`,
and live frame orchestration scoped out.
