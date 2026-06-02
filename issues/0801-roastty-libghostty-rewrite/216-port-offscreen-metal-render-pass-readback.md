# Experiment 216: Port Offscreen Metal Render Pass Readback

## Description

Experiments 214-215 proved that Roastty can compile the production Metal shader
source and create live `MTLRenderPipelineState` objects for the five standard
pipelines. That still only proves pipeline construction. It does not prove that
the production shaders can be bound, encoded, drawn, stored into a render
target, and read back.

The next upstream layer is `vendor/ghostty/src/renderer/metal/RenderPass.zig`
plus the command-buffer shape from
`vendor/ghostty/src/renderer/metal/Frame.zig`. This experiment ports a minimal
offscreen version of that layer: enough to begin a command buffer, begin a
render pass against a texture, bind a pipeline and the production uniform
buffer, draw, end encoding, commit, wait, and read back pixels.

This remains intentionally offscreen. It should not add IOSurface, CAMetalLayer,
Swift integration, presentation, frame-completion callbacks, render-thread
health, samplers, texture binding, cell buffers, text rendering, image
rendering, or custom postprocess shaders.

The key proof is a live automated test: draw the standard `bg_color` pipeline
into a small BGRA render-target texture and verify the pixel bytes. This turns
the renderer from "can build Metal state" into "can execute a real standard
shader and observe output."

All public names must use Roastty naming.

## Changes

1. Enable the needed Objective-C Metal API bindings.

   In `roastty/Cargo.toml`, extend `objc2-metal` with the narrow feature set
   required for command encoding:
   - `MTLCommandBuffer`;
   - `MTLCommandEncoder`;
   - `MTLCommandQueue`;
   - `MTLRenderCommandEncoder`;
   - `MTLRenderPass`.

   Keep `default-features = false`. Do not enable unrelated Metal feature
   families.

2. Extend Metal API value coverage.

   In `roastty/src/renderer/metal/api.rs`, add the small enum/value subset used
   by the upstream render-pass path:
   - `MetalLoadAction::{Load, Clear}`;
   - `MetalStoreAction::Store`;
   - `MetalPrimitiveType::Triangle`;
   - `MetalCommandBufferStatus::{Completed, Error}` for required status
     checking;
   - `MetalClearColor { red, green, blue, alpha }`.

   Add `raw()` and `to_objc()` conversions where applicable. Tests must prove
   the raw values match upstream's `metal/api.zig` values:
   - load: `load = 1`, `clear = 2`;
   - store: `store = 1`;
   - primitive: `triangle = 3`;
   - command buffer: `completed = 4`, `error = 5`, if added.

3. Port standard Metal uniforms.

   Add a Rust representation of upstream `shaders.zig::Uniforms` in
   `roastty/src/renderer/metal/shaders.rs` or a dedicated nearby module.

   Requirements:
   - use Roastty naming, e.g. `MetalUniforms` and `MetalUniformBools`;
   - use `#[repr(C)]` and explicit named `_padding*` fields for every layout
     hole required to match the MSL-visible layout;
   - constructors must initialize every field, including padding, before the
     value is uploaded as raw bytes;
   - preserve the MSL-visible field order and alignment:
     - projection matrix;
     - screen size;
     - cell size;
     - grid size;
     - grid padding;
     - padding-extend bit mask;
     - minimum contrast;
     - cursor position;
     - cursor color;
     - background color;
     - booleans: cursor-wide, use-display-p3, use-linear-blending,
       use-linear-correction;
   - implement `MetalBufferElement` for `MetalUniforms`;
   - add tests for `size_of`, `align_of`, `offset_of` for every field and nested
     boolean field, and explicit padding placement against the expected upstream
     layout;
   - add a byte-level constructor test that creates a representative
     `MetalUniforms`, converts it to bytes, and proves the padding bytes are
     initialized to zero.

   Do not rely on implicit Rust padding for an unsafe `MetalBufferElement` impl.
   The uniform buffer is uploaded as raw bytes, so uninitialized padding would
   make the implementation unsound and could leak stale stack data to Metal.

   Add a small constructor for tests, such as
   `MetalUniforms::test_bg_color(width, height, [r, g, b, a])`, that sets:
   - an identity or otherwise harmless projection matrix;
   - `screen_size` matching the render target;
   - non-zero `cell_size` and `grid_size`;
   - zero padding;
   - `use_display_p3 = true`;
   - `use_linear_blending = false`;
   - the requested background RGBA color.

   Setting `use_display_p3 = true` and `use_linear_blending = false` is
   important for the read-back test because the shader then returns the provided
   color without sRGB-to-P3 matrix conversion or linear-output conversion.

4. Expose the needed Metal object accessors.

   Add narrow crate-visible accessors required by render-pass encoding:
   - `MetalPipeline::state(&self) -> &ProtocolObject<dyn MTLRenderPipelineState>`
     should become available outside tests;
   - `MetalBuffer<T>::buffer(&self) -> &ProtocolObject<dyn MTLBuffer>`;
   - `MetalTexture::texture(&self) -> &ProtocolObject<dyn MTLTexture>`;
   - a test-only or crate-visible texture read-back helper, preserving the
     existing byte-count validation.

   Do not expose these through the public C ABI.

5. Add render-target texture options.

   In `roastty/src/renderer/metal/texture.rs`, add a helper for offscreen render
   targets:

   ```rust
   pub(crate) fn render_target_texture_options(
       pixel_format: MetalPixelFormat,
       storage_mode: MetalStorageMode,
   ) -> ImageTextureOptions
   ```

   Behavior:
   - use the provided pixel format;
   - use `MetalResourceOptions::image(storage_mode)`;
   - set `usage.render_target = true`;
   - leave `usage.shader_read = false` for this experiment.

   Add tests proving the usage bits and pixel format match expectations.

   For read-back tests in this experiment, always use
   `MetalStorageMode::Shared`. Read bytes only after
   `MetalCommandFrame::commit_and_wait(...)` has waited for command-buffer
   completion. Do not use `Managed` storage in this experiment; managed
   read-back would require adding a blit encoder and `synchronizeResource`,
   which is a separate synchronization slice.

6. Add the offscreen command frame and render pass.

   Add `roastty/src/renderer/metal/render_pass.rs` and export it from
   `roastty/src/renderer/metal/mod.rs`.

   Port the upstream render-pass shape, but limit this experiment to the
   standard background-color draw path:

   ```rust
   pub(crate) struct MetalCommandFrame { ... }

   impl MetalCommandFrame {
       pub(crate) fn begin(
           queue: &ProtocolObject<dyn MTLCommandQueue>,
       ) -> Result<Self, MetalCommandFrameError>;

       pub(crate) fn render_pass(
           &self,
           attachments: &[MetalRenderPassAttachment<'_>],
       ) -> Result<MetalRenderPass, MetalRenderPassError>;

       pub(crate) fn commit_and_wait(self) -> Result<(), MetalCommandFrameError>;
   }

   pub(crate) struct MetalRenderPassAttachment<'a> {
       pub(crate) texture: &'a MetalTexture,
       pub(crate) clear_color: Option<MetalClearColor>,
   }

   pub(crate) struct MetalRenderPassStep<'a> {
       pub(crate) pipeline: &'a MetalPipeline,
       pub(crate) uniforms: Option<&'a ProtocolObject<dyn MTLBuffer>>,
       pub(crate) draw: MetalDraw,
   }

   pub(crate) struct MetalDraw {
       pub(crate) primitive_type: MetalPrimitiveType,
       pub(crate) vertex_count: usize,
       pub(crate) instance_count: usize,
   }
   ```

   Behavior:
   - create `MTLRenderPassDescriptor`;
   - for each attachment:
     - use `Clear` load action if `clear_color` is present;
     - use `Load` load action otherwise;
     - use `Store` store action;
     - set the attachment texture;
     - set clear color when present;
   - create a render command encoder from the command buffer;
   - `step(...)` returns immediately when `instance_count == 0`, matching
     upstream;
   - set the render pipeline state;
   - if uniforms are present, bind them at vertex and fragment buffer index `1`,
     matching upstream;
   - draw with `drawPrimitives:vertexStart:vertexCount:instanceCount:`;
   - end encoding on `complete()`;
   - commit and wait synchronously in tests;
   - after waiting, inspect the command-buffer status;
   - return success only for `Completed`;
   - map `Error` and every non-`Completed` status to explicit
     `MetalCommandFrameError` values;
   - report explicit errors for missing command queue/buffer/encoder creation.

   Do not add generic buffer lists, texture lists, sampler lists, or image/text
   draw support in this experiment. They belong to later renderer slices once
   this basic encoded draw path is proven.

7. Add live offscreen read-back tests.

   Required tests:
   - creating a command queue from the system default Metal device succeeds;
   - a clear-only render pass stores the clear color into a small BGRA8 unorm
     texture, and read-back bytes match the expected BGRA byte order;
   - a `bg_color` render pass:
     - creates `MetalStandardPipelines`;
     - creates a `MetalUniforms` buffer with RGBA background color
       `[32, 64, 128, 255]`;
     - creates a 4x4 BGRA8 unorm render-target texture;
     - encodes one step using `pipelines.bg_color`;
     - draws a triangle with `vertex_count = 3`, `instance_count = 1`;
     - commits and waits;
     - reads back every pixel;
     - verifies every pixel is BGRA `[128, 64, 32, 255]`.
   - a zero-instance step performs no draw: clear the target to one color, run a
     zero-instance `bg_color` step with a different uniform color, commit, and
     verify the clear color remains.
   - command-buffer status mapping is deterministic: add a pure unit test for
     the status-to-error helper proving `Completed` maps to success, `Error`
     maps to command failure, and other known statuses map to a non-completed
     error. A live command-buffer failure does not need to be manufactured.

   Use `MetalPixelFormat::Bgra8Unorm` for read-back tests. Do not use the sRGB
   variant for byte-exact verification in this experiment. Use
   `MetalStorageMode::Shared` for all read-back textures in this experiment.

8. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/metal/api.rs roastty/src/renderer/metal/buffer.rs roastty/src/renderer/metal/mod.rs roastty/src/renderer/metal/pipeline.rs roastty/src/renderer/metal/render_pass.rs roastty/src/renderer/metal/shaders.rs roastty/src/renderer/metal/texture.rs
   cargo test -p roastty renderer::metal::api
   cargo test -p roastty renderer::metal::buffer
   cargo test -p roastty renderer::metal::texture
   cargo test -p roastty renderer::metal::shaders
   cargo test -p roastty renderer::metal::render_pass
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/metal; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not add IOSurface, CAMetalLayer, Swift integration, presentation, or public
  C ABI.
- Do not add frame-completion callbacks, renderer health integration, draw
  threads, or app/window lifecycle.
- Do not add text, image, cell-background, background-image, custom shader, or
  postprocess rendering.
- Do not add sampler support.
- Do not add generic texture lists or generic buffer lists beyond the uniform
  binding required for `bg_color`.
- Do not modify vendored Ghostty source.
- Do not change the standard shader source or standard pipeline descriptions
  except where tests prove a real mismatch.
- Do not introduce public or app-facing `ghostty_*` names in Roastty.

## Pass Criteria

- Roastty can create a Metal command queue, command buffer, render pass
  descriptor, and render command encoder.
- Roastty can render into an offscreen Metal texture and read bytes back.
- A clear-only pass produces the expected BGRA bytes.
- The production `bg_color` shader produces the expected BGRA bytes through a
  real uniform buffer and the real standard pipeline.
- Zero-instance draw behavior matches upstream by performing no draw.
- Full verification passes, including both no-`ghostty` gates.

## Failure Criteria

- The experiment only creates encoders but does not read pixels back.
- The experiment uses the small Experiment 214 test shader instead of the
  production standard shader source.
- The `bg_color` test bypasses the production uniform buffer by changing shader
  source or using a fake fragment function.
- The experiment grows into window presentation, IOSurface, CAMetalLayer, text
  rendering, image rendering, or custom shaders.
- Existing Metal shader, pipeline, texture, buffer, image, or full Roastty tests
  regress.

## Result

Not run yet.

## Conclusion

Pending.
