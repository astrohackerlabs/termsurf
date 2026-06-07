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

# Experiment 808: Metal Sampler Wrapper

## Description

Port upstream `renderer/metal/Sampler.zig` into Roastty as a focused Metal
sampler wrapper.

The renderer checklist still lists `Sampler` as missing. Roastty already has
Metal API enum wrappers, textures, buffers, pipelines, frame state, and render
passes, but no `MTLSamplerDescriptor`/`MTLSamplerState` wrapper. This experiment
adds the sampler value/API layer needed by later render-pass image binding
without attempting full live frame orchestration.

## Changes

- `roastty/Cargo.toml`
  - Enable the `MTLSampler` feature on `objc2-metal`, because the generated
    sampler descriptor/state types are feature-gated.
- `roastty/src/renderer/metal/api.rs`
  - Add `MetalSamplerMinMagFilter` with `Nearest = 0`, `Linear = 1`, and
    `to_objc`.
  - Add `MetalSamplerAddressMode` with upstream/Metal values for `ClampToEdge`,
    `MirrorClampToEdge`, `Repeat`, `MirrorRepeat`, `ClampToZero`, and
    `ClampToBorderColor`, plus `to_objc`.
  - Add tests proving the raw values match upstream `renderer/metal/api.zig`.
- `roastty/src/renderer/metal/sampler.rs`
  - Add `MetalSamplerOptions` matching upstream `Sampler.Options`: device,
    min/mag filter, and S/T address modes.
  - Add `MetalSampler` owning a retained `MTLSamplerState`.
  - Create an `MTLSamplerDescriptor`, set min/mag filters and S/T address modes,
    call `newSamplerStateWithDescriptor`, and return a `SamplerCreationFailed`
    error if Metal returns null.
  - Expose the sampler state for later render-pass binding.
  - Add tests for option defaults/value preservation and a macOS Metal-device
    smoke test when `MTLCreateSystemDefaultDevice` returns a device.
- `roastty/src/renderer/metal/mod.rs`
  - Add the `sampler` module.
- `issues/0801-roastty-libghostty-rewrite/README.md`
  - After implementation, update the Metal checklist row to mention the sampler
    wrapper while keeping window `Target`, `IOSurfaceLayer`, and full live frame
    orchestration open.

## Verification

- Inspect:
  - `vendor/ghostty/src/renderer/metal/Sampler.zig`
  - `vendor/ghostty/src/renderer/metal/api.zig`
  - `roastty/src/renderer/metal/api.rs`
  - `roastty/src/renderer/metal/render_pass.rs`
- Run:
  - `cargo fmt -p roastty`
  - `cargo test -p roastty metal::sampler -- --nocapture --test-threads=1`
  - `cargo test -p roastty metal::api -- --nocapture --test-threads=1`
- Run markdown formatting:
  - `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/808-metal-sampler-wrapper.md`
- Run:
  - `git diff --check`

The experiment passes if Roastty has a tested Metal sampler wrapper and API enum
mapping while the Metal renderer row remains partial for `Target`,
`IOSurfaceLayer`, render-pass sampler binding, and full live frame
orchestration. It is Partial if enum mappings land but sampler state creation
needs follow-up. It fails if sampler creation cannot be cleanly expressed with
the current `objc2-metal` bindings.

## Design Review

Codex reviewed the design and approved it with no findings. The review confirmed
that the planned options and descriptor creation map directly to upstream
`Sampler.zig`, the sampler filter/address enum values match upstream `api.zig`,
enabling the `MTLSampler` feature is required for the local `objc2-metal`
bindings, the planned tests cover enum values and device-conditional sampler
creation, and render-pass sampler binding remains properly scoped out.

## Result

**Result:** Pass

Roastty now has a focused Metal sampler wrapper:

- `roastty/Cargo.toml` enables the `MTLSampler` feature for `objc2-metal`.
- `roastty/src/renderer/metal/api.rs` defines `MetalSamplerMinMagFilter` and
  `MetalSamplerAddressMode` with upstream raw values and `objc2-metal`
  conversions.
- `roastty/src/renderer/metal/sampler.rs` defines descriptor options,
  `MetalSamplerOptions`, `MetalSampler`, and `MetalSamplerError`.
- `MetalSampler::new` creates an `MTLSamplerDescriptor`, applies min/mag filter
  and S/T address mode values, calls `newSamplerStateWithDescriptor`, and owns
  the retained sampler state for later render-pass binding.

The implementation does not bind samplers in `render_pass.rs` yet and does not
claim `Target`, `IOSurfaceLayer`, or live frame orchestration.

The Issue 801 Metal checklist now records the sampler wrapper as present while
keeping render-pass sampler binding, window `Target`, `IOSurfaceLayer`, and full
live frame orchestration open.

Verification:

- Inspected `vendor/ghostty/src/renderer/metal/Sampler.zig`.
- Inspected `vendor/ghostty/src/renderer/metal/api.zig`.
- Inspected `roastty/src/renderer/metal/api.rs`.
- Inspected `roastty/src/renderer/metal/render_pass.rs`.
- `cargo fmt -p roastty` — passed.
- `cargo test -p roastty metal::sampler -- --nocapture --test-threads=1` —
  passed, 4 tests.
- `cargo test -p roastty metal::api -- --nocapture --test-threads=1` — passed,
  22 tests.
- `prettier --write --prose-wrap always --print-width 80 issues/0801-roastty-libghostty-rewrite/README.md issues/0801-roastty-libghostty-rewrite/808-metal-sampler-wrapper.md`
  — passed.
- `git diff --check` — passed.

## Conclusion

Experiment 808 fills the missing Metal sampler wrapper layer and leaves the next
Metal work focused on render-pass sampler binding, window `Target`,
`IOSurfaceLayer`, and live frame orchestration.

## Completion Review

Codex reviewed the staged result and approved it with no findings. The approval
confirmed that the wrapper maps faithfully to upstream `Sampler.zig`, sampler
enum raw values match upstream `api.zig`, `objc2-metal` has the required
`MTLSampler` feature enabled, `Retained` owns the `MTLSamplerState`, the device
smoke test skips cleanly when no Metal device is available, render-pass sampler
binding remains scoped out, and the docs keep the Metal row partial.
