# Experiment 214: Port Metal Pipeline State Builder

## Description

Experiments 211-213 built the pure Rust value layer for upstream's Metal
pipeline path:

- vertex descriptor values;
- attachment and blend values;
- standard pipeline descriptions;
- shader payload layouts and buffer eligibility.

The next upstream layer is `vendor/ghostty/src/renderer/metal/Pipeline.zig`
itself: convert those values into real Metal objects and create an
`MTLRenderPipelineState`.

This experiment ports the real pipeline-state builder, but only for standard
render pipelines. It should not yet port the full upstream shader collection,
embedded `ghostty_metallib`, postprocess shader support, render passes, command
encoders, frames, or target surfaces.

The experiment should prove the path end-to-end with a small test-only Metal
shader source compiled through `newLibraryWithSource(...)`. That avoids needing
the production embedded Metal library before the pipeline object path is proven.

All public names must use Roastty naming.

## Changes

1. Enable the needed Objective-C API bindings.

   In `roastty/Cargo.toml`, extend the existing dependencies:
   - `objc2-foundation`:
     - add `NSString`;
     - add `NSError`;
   - `objc2-metal`:
     - add `MTLLibrary`;
     - add `MTLRenderPipeline`;
     - add `MTLVertexDescriptor`.

   Keep `default-features = false`. Do not enable unrelated Metal feature
   families.

2. Extend Metal API conversions.

   In `roastty/src/renderer/metal/api.rs`, add Objective-C conversions for the
   value enums that will now be applied to real descriptors:
   - `MetalVertexFormat::to_objc() -> objc2_metal::MTLVertexFormat`;
   - `MetalVertexStepFunction::to_objc() -> objc2_metal::MTLVertexStepFunction`;
   - `MetalBlendFactor::to_objc() -> objc2_metal::MTLBlendFactor`;
   - `MetalBlendOperation::to_objc() -> objc2_metal::MTLBlendOperation`.

   Add tests proving each Objective-C value's raw representation matches the
   internal raw value.

3. Add real Metal descriptor builders.

   In `roastty/src/renderer/metal/pipeline.rs`, add helpers that consume the
   already-tested value layer:

   ```rust
   pub(crate) fn build_metal_vertex_descriptor(
       descriptor: &MetalVertexDescriptor,
   ) -> objc2::rc::Retained<objc2_metal::MTLVertexDescriptor>

   pub(crate) fn apply_pipeline_attachment_descriptor(
       target: &objc2_metal::MTLRenderPipelineColorAttachmentDescriptor,
       descriptor: MetalPipelineAttachmentDescriptor,
   )
   ```

   Behavior:
   - vertex attributes are written in order to `attributes[i]`;
   - every vertex attribute writes `format`, `offset`, and `bufferIndex`;
   - layout `0` writes `stride` and `stepFunction`;
   - attachment `0` writes `pixelFormat` and `blendingEnabled`;
   - if blending is enabled, write all six upstream premultiplied-alpha blend
     fields;
   - if blending is disabled, do not write blend factors or operations.

   Keep the value layer unchanged. These helpers are one-way application from
   tested values into Objective-C descriptors.

4. Add the `MetalPipeline` wrapper.

   In `roastty/src/renderer/metal/pipeline.rs`, add:

   ```rust
   pub(crate) struct MetalPipeline {
       state: objc2::rc::Retained<
           objc2::runtime::ProtocolObject<dyn objc2_metal::MTLRenderPipelineState>,
       >,
   }
   ```

   Add:

   ```rust
   pub(crate) struct MetalPipelineOptions<'a> {
       pub(crate) device: &'a objc2::runtime::ProtocolObject<dyn objc2_metal::MTLDevice>,
       pub(crate) vertex_library:
           &'a objc2::runtime::ProtocolObject<dyn objc2_metal::MTLLibrary>,
       pub(crate) fragment_library:
           &'a objc2::runtime::ProtocolObject<dyn objc2_metal::MTLLibrary>,
       pub(crate) values: MetalPipelineBuildValues,
   }

   pub(crate) enum MetalPipelineError {
       MissingVertexFunction(&'static str),
       MissingFragmentFunction(&'static str),
       PipelineCreationFailed(String),
   }

   impl MetalPipeline {
       pub(crate) fn new(options: MetalPipelineOptions<'_>) -> Result<Self, MetalPipelineError>
   }
   ```

   Behavior:
   - create an `MTLRenderPipelineDescriptor`;
   - resolve `values.vertex_function` from `vertex_library`;
   - resolve `values.fragment_function` from `fragment_library`;
   - set `vertexFunction` and `fragmentFunction`;
   - if `values.vertex_descriptor` is `Some`, build and set the
     `MTLVertexDescriptor`;
   - apply the attachment descriptor to color attachment `0`;
   - call `device.newRenderPipelineStateWithDescriptor_error(...)`;
   - preserve the returned retained pipeline state.

   Error handling should be explicit and testable:
   - missing shader functions produce `MissingVertexFunction` or
     `MissingFragmentFunction`;
   - Metal pipeline creation errors produce `PipelineCreationFailed(...)` with a
     useful message, if the NSError exposes one.

5. Add live Metal tests.

   Add tests in `roastty/src/renderer/metal/pipeline.rs` that use the system
   default Metal device and compile a small test-only Metal shader source
   through `device.newLibraryWithSource_options_error(...)`.

   The test shader source should define functions for the standard pipeline
   names needed by the tests. It does not need to be upstream's production
   shader source, but it must compile valid pipelines using the same function
   names and vertex descriptor shapes that Roastty applies.

   Required tests:
   - build an Objective-C vertex descriptor for `CellTextVertex` and read back
     every attribute's format, offset, and buffer index, plus layout stride and
     step function;
   - apply an enabled attachment and read back pixel format, blending flag, and
     all six blend fields;
   - apply a disabled attachment and read back pixel format and disabled blend
     flag;
   - create a live `MetalPipeline` for `bg_color` with no vertex descriptor;
   - create a live `MetalPipeline` for `image` with a vertex descriptor;
   - missing vertex and missing fragment functions return the explicit error
     variants without panicking.
   - when both shader functions resolve but Metal rejects the render pipeline
     descriptor, the result is `PipelineCreationFailed(...)` with a non-empty
     message. Use a deterministic invalid descriptor case such as
     `MetalPixelFormat::Invalid`, or an intentionally incompatible shader
     interface if Metal accepts the invalid pixel format before compilation.

   If Metal shader compilation fails for environment reasons, record the exact
   compiler error. Do not silently weaken this to a compile-only Rust test.

6. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/metal/api.rs roastty/src/renderer/metal/pipeline.rs
   cargo test -p roastty renderer::metal::api
   cargo test -p roastty renderer::metal::pipeline
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not port or embed upstream `ghostty_metallib` in this experiment.
- Do not add production shader source files yet.
- Do not add postprocess shader support.
- Do not add render passes, command encoders, command buffers, frames,
  IOSurface, CAMetalLayer, Swift integration, or public C ABI.
- Do not modify vendored Ghostty source.
- Do not modify image upload, image draw, texture, buffer, shader payload, or
  value-descriptor semantics from Experiments 206-213 except where Objective-C
  conversion methods are added.
- Do not expose `ghostty_*` symbols or comments in the Roastty public ABI.

## Pass Criteria

- Roastty can convert its tested Metal vertex, attachment, blend, and pipeline
  build values into real Objective-C Metal descriptors.
- Roastty can create a live `MTLRenderPipelineState` for at least one standard
  pipeline with no vertex descriptor and one standard pipeline with a vertex
  descriptor.
- Missing shader functions return explicit errors instead of panicking.
- Tests cover descriptor read-back, live pipeline creation, and error paths.
- Full verification passes, including the public no-`ghostty` gate.

## Failure Criteria

- The experiment skips live Metal pipeline creation and only tests Rust value
  composition.
- The experiment embeds or ports the production Metal library instead of keeping
  this focused on the pipeline object path.
- Descriptor application loses any vertex attribute, layout, attachment, or
  blend setting already proven by Experiments 211-213.
- Pipeline creation panics on missing functions or Metal errors.
- Existing renderer image, texture, buffer, shader payload, vertex descriptor,
  attachment, or standard table tests regress.

## Result

Not run yet.

## Conclusion

Pending.
