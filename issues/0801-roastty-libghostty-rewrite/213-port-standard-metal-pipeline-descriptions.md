# Experiment 213: Port Standard Metal Pipeline Descriptions

## Description

Experiments 211 and 212 ported the value pieces that upstream
`vendor/ghostty/src/renderer/metal/Pipeline.zig` needs before real pipeline
creation:

- vertex descriptor values;
- color attachment values;
- premultiplied-alpha blend values.

The next upstream layer is the standard pipeline description table in
`vendor/ghostty/src/renderer/metal/shaders.zig`. That table defines the
renderer's built-in Metal pipelines:

| Name        | Vertex function      | Fragment function    | Vertex input | Step function | Blending |
| ----------- | -------------------- | -------------------- | ------------ | ------------- | -------- |
| `bg_color`  | `full_screen_vertex` | `bg_color_fragment`  | none         | per vertex    | false    |
| `cell_bg`   | `full_screen_vertex` | `cell_bg_fragment`   | none         | per vertex    | true     |
| `cell_text` | `cell_text_vertex`   | `cell_text_fragment` | `CellText`   | per instance  | true     |
| `image`     | `image_vertex`       | `image_fragment`     | `Image`      | per instance  | true     |
| `bg_image`  | `bg_image_vertex`    | `bg_image_fragment`  | `BgImage`    | per instance  | true     |

Roastty already has `ImageVertex`. It does not yet have the value-level shader
input structs for `CellText`, `CellBg`, or `BgImage`, and it does not yet have
the standard pipeline description table that composes vertex input, shader
function names, step function, and attachment options.

This experiment ports those value definitions and the standard description
table. It must remain value-only: no `MTLLibrary`, no shader compilation, no
`MTLRenderPipelineDescriptor`, and no `MTLRenderPipelineState`.

All public names must use Roastty naming.

## Changes

1. Extend the shader value layer.

   In `roastty/src/renderer/shader.rs`, add the missing upstream shader input
   values:
   - `CellTextVertex`;
   - `CellTextAtlas`;
   - `CellTextFlags`;
   - `CellBg`;
   - `BgImageVertex`;
   - `BgImageInfo`;
   - `BgImagePosition`;
   - `BgImageFit`.

   Preserve upstream layout intent:
   - `CellTextVertex` is a C-compatible 32-byte value matching upstream
     `CellText`, with alignment `8`;
   - `CellTextAtlas` maps grayscale/color to raw values `0` and `1`;
   - `CellTextFlags` packs `no_min_contrast` and `is_cursor_glyph` into the low
     two bits of one byte;
   - `CellBg` is a transparent four-byte color value, with alignment `1`;
   - `BgImageVertex` is a C-compatible 8-byte value matching upstream `BgImage`,
     with alignment `4`;
   - `BgImageInfo` packs position, fit, and repeat into one byte:
     - position occupies bits `0..=3`;
     - fit occupies bits `4..=5`;
     - repeat occupies bit `6`;
     - bit `7` is padding.

   Use stable raw representations for all small enum and packed-byte wrappers:
   `#[repr(u8)]` for raw enums, and explicit one-byte wrapper structs for packed
   flag/info bytes. Keep these types internal. Do not add C ABI for them yet.

2. Extend the Metal vertex descriptor mapping.

   In `roastty/src/renderer/metal/pipeline.rs`, add `MetalVertexInput`
   implementations for:
   - `CellTextVertex`;
   - `BgImageVertex`.

   Match upstream `Pipeline.zig::autoAttribute` behavior:
   - field offsets come from Rust `offset_of!`;
   - buffer index is always `0`;
   - layout stride is `size_of::<T>()`;
   - `CellTextVertex` uses `MetalVertexFormat` values corresponding to upstream
     field backing types;
   - `BgImageVertex` maps `opacity` to `Float` and `info` to `UChar`.

   If a needed `MetalVertexFormat` already exists, reuse it. If the exact
   upstream format is missing, add only the missing enum value with a raw-value
   test.

3. Extend the Metal buffer element bound.

   In `roastty/src/renderer/metal/buffer.rs`, mark the newly ported shader
   payload values as valid Metal buffer elements:
   - `CellTextVertex`;
   - `CellBg`;
   - `BgImageVertex`.

   These unsafe impls are allowed only after the experiment adds layout, size,
   alignment, and packing tests for those types. Add a compile-only test/helper
   proving all three types satisfy `T: MetalBufferElement`, because upstream
   allocates buffers for `CellText`, `CellBg`, and `BgImage` through
   `Buffer(shaderpkg.*)`.

4. Add the standard pipeline description table.

   In `roastty/src/renderer/metal/pipeline.rs`, add internal value types for the
   standard pipeline table:

   ```rust
   pub(crate) enum MetalPipelineVertexInputKind {
       None,
       CellText,
       Image,
       BgImage,
   }

   pub(crate) struct MetalStandardPipelineDescription {
       pub(crate) name: &'static str,
       pub(crate) vertex_function: &'static str,
       pub(crate) fragment_function: &'static str,
       pub(crate) vertex_input: MetalPipelineVertexInputKind,
       pub(crate) step_function: MetalVertexStepFunction,
       pub(crate) blending_enabled: bool,
   }
   ```

   Add:

   ```rust
   pub(crate) const STANDARD_PIPELINE_DESCRIPTIONS: &[MetalStandardPipelineDescription]
   ```

   The table must match upstream order and values exactly:
   1. `bg_color`;
   2. `cell_bg`;
   3. `cell_text`;
   4. `image`;
   5. `bg_image`.

5. Add a value composer for a future real pipeline builder.

   Add a helper that converts one standard description plus a render target
   pixel format into the already-ported value pieces:

   ```rust
   pub(crate) struct MetalPipelineBuildValues {
       pub(crate) name: &'static str,
       pub(crate) vertex_function: &'static str,
       pub(crate) fragment_function: &'static str,
       pub(crate) vertex_input: MetalPipelineVertexInputKind,
       pub(crate) vertex_descriptor: Option<MetalVertexDescriptor>,
       pub(crate) attachment: MetalPipelineAttachmentDescriptor,
   }

   pub(crate) fn standard_pipeline_build_values(
       description: MetalStandardPipelineDescription,
       pixel_format: MetalPixelFormat,
   ) -> MetalPipelineBuildValues
   ```

   Behavior:
   - descriptions with `None` vertex input return `vertex_descriptor = None`;
   - `cell_text`, `image`, and `bg_image` return the matching vertex descriptor
     with the description's step function;
   - attachments use `pipeline_attachment_descriptor(...)`, preserving
     upstream's `blending_enabled` value for every pipeline;
   - no Objective-C objects are created.

6. Add tests.

   Add pure Rust tests proving:
   - shader value raw sizes and alignments match upstream exactly:
     - `CellTextVertex`: size 32, alignment 8;
     - `CellBg`: size 4, alignment 1;
     - `ImageVertex`: existing size 40 remains true;
     - `BgImageVertex`: size 8, alignment 4;
   - `CellTextAtlas` raw values are `0` and `1`;
   - `CellTextFlags` packs the two low bits correctly;
   - all `BgImagePosition` and `BgImageFit` raw values match upstream;
   - `BgImageInfo` packing matches upstream bit positions;
   - `CellTextVertex` and `BgImageVertex` vertex descriptors match upstream
     field order, formats, offsets, buffer index, stride, and step function;
   - `CellTextVertex`, `CellBg`, and `BgImageVertex` satisfy the
     `MetalBufferElement` bound;
   - `STANDARD_PIPELINE_DESCRIPTIONS` matches upstream order, names, shader
     function names, step function, vertex input kind, and blending flags;
   - `standard_pipeline_build_values(...)` composes the correct optional vertex
     descriptor and attachment descriptor for every standard pipeline.

7. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/shader.rs roastty/src/renderer/metal/api.rs roastty/src/renderer/metal/pipeline.rs roastty/src/renderer/metal/buffer.rs
   cargo test -p roastty renderer::shader
   cargo test -p roastty renderer::metal::pipeline
   cargo test -p roastty renderer::metal::api
   cargo test -p roastty renderer::metal::buffer
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not create Objective-C `MTLLibrary`, `MTLFunction`,
  `MTLRenderPipelineDescriptor`, `MTLRenderPipelineState`, or
  `MTLVertexDescriptor` objects.
- Do not compile shader source or embed Metal library bytes.
- Do not add render passes, command encoders, command buffers, IOSurface,
  CAMetalLayer, Swift integration, or public C ABI.
- Do not add postprocess shader support in this experiment.
- Do not modify image upload, image draw, texture behavior, buffer allocation or
  sync behavior, or existing pipeline attachment semantics from Experiments
  206-212. The only allowed buffer change is adding `MetalBufferElement` impls
  for the newly ported shader payload value types after layout tests prove they
  are stable.
- Do not modify vendored Ghostty source.
- Do not expose `ghostty_*` symbols or comments in the Roastty public ABI.

## Pass Criteria

- Roastty has internal value definitions for the standard Metal shader input
  values needed by upstream's standard pipeline table.
- Roastty has a standard pipeline description table matching upstream order and
  values exactly.
- The table composes with the existing vertex descriptor and attachment value
  layers into pure Rust build values.
- Newly ported shader payloads satisfy the existing Metal buffer element bound
  without changing buffer allocation or sync behavior.
- Tests cover layout, raw packing, vertex descriptor mapping, standard table
  values, and build-value composition.
- Full verification passes, including the public no-`ghostty` gate.

## Failure Criteria

- The experiment creates real Metal pipeline/shader Objective-C objects instead
  of staying at the value layer.
- The standard pipeline table omits or reorders an upstream standard pipeline.
- A shader input struct has a layout or packing mismatch with upstream.
- A standard pipeline uses the wrong vertex input, shader function name, step
  function, or blending flag.
- Existing renderer image, texture, buffer, vertex descriptor, or attachment
  tests regress.

## Result

**Result:** Pass

Experiment 213 added the internal value layer for upstream's standard Metal
pipeline table.

The implementation added:

- `CellTextVertex`, `CellTextAtlas`, `CellTextFlags`, and explicit padding for
  the upstream `CellText` shader payload shape;
- `CellBg` as a transparent four-byte color payload;
- `BgImageVertex`, `BgImageInfo`, `BgImagePosition`, `BgImageFit`, and explicit
  padding for the upstream `BgImage` shader payload shape;
- `MetalBufferElement` impls for `CellTextVertex`, `CellBg`, and
  `BgImageVertex`;
- vertex descriptor mappings for `CellTextVertex` and `BgImageVertex`;
- `STANDARD_PIPELINE_DESCRIPTIONS`, matching upstream's five standard Metal
  pipeline descriptions in order;
- `standard_pipeline_build_values(...)`, which composes a standard pipeline
  description, vertex descriptor values, and attachment descriptor values
  without creating real Metal objects.

The first Codex result review found a real unsafe-contract issue:
`CellTextVertex` and `BgImageVertex` had implicit trailing padding while also
being marked as `MetalBufferElement`. Because the buffer upload path copies
`size_of::<T>()` bytes into Metal buffers, uploadable payloads need stable,
initialized representation bytes. The implementation was fixed by making the
padding explicit:

- `CellTextVertex::_padding: [u8; 2]`;
- `BgImageVertex::_padding: [u8; 3]`.

The layout tests now assert the explicit padding offsets.

Verification passed after the fix:

```bash
cargo fmt -- roastty/src/renderer/shader.rs roastty/src/renderer/metal/api.rs roastty/src/renderer/metal/pipeline.rs roastty/src/renderer/metal/buffer.rs
cargo test -p roastty renderer::shader
cargo test -p roastty renderer::metal::pipeline
cargo test -p roastty renderer::metal::api
cargo test -p roastty renderer::metal::buffer
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
git diff --check
```

Observed test results:

- `cargo test -p roastty renderer::shader`: 9 passed, 0 failed;
- `cargo test -p roastty renderer::metal::pipeline`: 9 passed, 0 failed;
- `cargo test -p roastty renderer::metal::api`: 11 passed, 0 failed;
- `cargo test -p roastty renderer::metal::buffer`: 10 passed, 0 failed;
- `cargo test -p roastty`: 2158 library tests passed, 1 ABI harness test passed,
  0 doc tests.

Codex re-reviewed the corrected result and reported no blocking findings. The
review explicitly approved recording Experiment 213 as Pass.

## Conclusion

Roastty now has the standard Metal pipeline description table and the shader
payload value types that table needs. The renderer value stack can now describe
the five built-in upstream pipelines without Objective-C pipeline objects:

- `bg_color`;
- `cell_bg`;
- `cell_text`;
- `image`;
- `bg_image`.

This sets up the next renderer slice: creating real Metal shader libraries,
functions, descriptors, and pipeline state from these already-tested values.
