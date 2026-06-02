# Experiment 217: Port Offscreen Metal Cell Background Readback

## Description

Experiment 216 proved that Roastty can execute a production Metal shader by
drawing `bg_color` into an offscreen texture and reading pixels back. That path
bound only the uniform buffer at Metal buffer index `1`.

The next production shader with useful terminal content is `cell_bg`. It uses
the same full-screen vertex shader and uniform buffer, plus a grid of `CellBg`
values bound at fragment buffer index `2`:

```metal
fragment float4 cell_bg_fragment(
  FullScreenVertexOut in [[stage_in]],
  constant Uniforms& uniforms [[buffer(1)]],
  constant uchar4 *cells [[buffer(2)]]
)
```

This experiment expands the offscreen render-pass step to bind the first
terminal content buffer and proves the production `cell_bg` shader by read-back.
It should remain offscreen and automated. It should not add text, glyph atlas
textures, image textures, samplers, background images, custom shaders,
IOSurface, CAMetalLayer, Swift integration, presentation, or public C ABI.

All public names must use Roastty naming.

## Changes

1. Extend render-pass buffer binding.

   In `roastty/src/renderer/metal/render_pass.rs`, extend `MetalRenderPassStep`
   with optional non-uniform buffers:

   ```rust
   pub(crate) buffers: &'a [Option<&'a ProtocolObject<dyn MTLBuffer>>],
   ```

   Behavior must match the upstream shape from
   `vendor/ghostty/src/renderer/metal/RenderPass.zig`:
   - if `buffers[0]` is present, bind it to vertex and fragment buffer index
     `0`;
   - bind `uniforms` to vertex and fragment buffer index `1`, as Experiment 216
     already does;
   - for `buffers[1..]`, bind each present buffer to vertex and fragment buffer
     indices starting at `2`;
   - preserve the zero-instance early return before any pipeline or buffer
     binding;
   - preserve the current `bg_color` behavior when `buffers` is empty.

   Add a helper if needed to keep index mapping easy to read. Do not add texture
   or sampler binding in this experiment.

2. Add a cell-background draw helper for tests.

   Use the existing `CellBg` shader payload type from
   `roastty/src/renderer/shader.rs`.

   The test helper should:
   - create a `Vec<CellBg>` grid;
   - upload it with `MetalBuffer::init_fill(...)` using
     `MetalStorageMode::Shared`;
   - bind it as `buffers[1]` so the render-pass code maps it to Metal buffer
     index `2`;
   - keep `buffers[0]` as `None`, because the full-screen vertex shader does not
     need a vertex buffer.

3. Add live `cell_bg` read-back tests.

   Add tests in `roastty/src/renderer/metal/render_pass.rs`.

   Required tests:
   - `cell_bg_render_pass_draws_per_cell_colors`:
     - create `MetalStandardPipelines`;
     - create a 4x4 BGRA8 unorm shared render target;
     - create `MetalUniforms::test_bg_color(...)` with:
       - `screen_size = [4, 4]`;
       - `cell_size = [1, 1]`;
       - `grid_size = [4, 4]`;
       - zero padding;
       - `use_display_p3 = true`;
       - `use_linear_blending = false`;
     - create 16 `CellBg` entries with distinct opaque RGBA values;
     - draw `pipelines.cell_bg` with a triangle, `vertex_count = 3`,
       `instance_count = 1`;
     - read back the 4x4 texture;
     - verify each pixel maps to its corresponding cell and appears in BGRA byte
       order.
   - `cell_bg_padding_without_extend_outputs_transparent`:
     - use a render target larger than the grid and non-zero grid padding;
     - set `grid_padding` explicitly as `[top, right, bottom, left]`;
     - use asymmetric top/left padding, for example top `1.0` and left `2.0`, so
       the test proves the shader's `uniforms.grid_padding.wx` mapping
       (`w = left`, `x = top`) rather than accidentally passing with symmetric
       padding;
     - leave `padding_extend = 0`;
     - clear the target to transparent;
     - draw `cell_bg`;
     - verify pixels outside the padded grid remain transparent while inside
       cells draw their expected colors.
   - `cell_bg_zero_instance_step_does_not_bind_or_draw`:
     - clear to one color;
     - provide a valid cell buffer with a different color;
     - draw with `instance_count = 0`;
     - verify the clear color remains.

   Use `MetalPixelFormat::Bgra8Unorm` and `MetalStorageMode::Shared` for all
   read-back tests. Read bytes only after `commit_and_wait(...)`.

4. Keep uniform layout sound.

   If the tests need `MetalUniforms` constructors beyond `test_bg_color(...)`,
   add explicit initialized constructors or setters that preserve zeroed
   padding. Do not mutate padding through raw pointers. Do not introduce
   implicit padding into `MetalUniforms`.

5. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/metal/render_pass.rs roastty/src/renderer/metal/shaders.rs
   cargo test -p roastty renderer::metal::render_pass
   cargo test -p roastty renderer::metal::shaders
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/metal; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not add text rendering, glyph atlases, font textures, image rendering,
  background-image rendering, custom shaders, postprocess shaders, samplers, or
  texture binding.
- Do not add IOSurface, CAMetalLayer, Swift integration, presentation, frame
  completion callbacks, draw threads, or public C ABI.
- Do not change production shader source.
- Do not modify vendored Ghostty source.
- Do not weaken Experiment 216's `bg_color` tests.
- Do not introduce public or app-facing `ghostty_*` names in Roastty.

## Pass Criteria

- The render-pass step implements upstream-compatible non-uniform buffer index
  mapping for the supported buffer subset.
- The production `cell_bg` shader draws per-cell colors into an offscreen render
  target.
- Read-back proves exact BGRA byte output for the per-cell grid.
- Padding-without-extend behavior is verified by read-back.
- Zero-instance behavior remains no-draw.
- Full verification passes, including both no-`ghostty` gates.

## Failure Criteria

- The experiment changes shader source or uses a fake fragment shader instead of
  production `cell_bg`.
- The cell buffer is bound directly through an ad hoc one-off path instead of
  the render-pass buffer-index mapping.
- The test only checks that commands encode and does not read pixels back.
- The experiment grows into text, image, texture, sampler, presentation, or
  public C ABI work.
- Existing Metal `bg_color`, shader-library, pipeline, texture, buffer, image,
  or full Roastty tests regress.

## Result

**Result:** Pass

Experiment 217 extended `MetalRenderPassStep` with the supported non-uniform
buffer slice and implemented the upstream-compatible index mapping:

- `buffers[0]` binds to vertex and fragment buffer index `0`;
- `uniforms` still bind to vertex and fragment buffer index `1`;
- `buffers[1..]` bind to vertex and fragment buffer indices starting at `2`;
- zero-instance steps still return before pipeline or buffer binding.

The production `cell_bg` shader now has automated offscreen read-back coverage.
The new tests draw per-cell `CellBg` colors through `pipelines.cell_bg`, bind
the cell buffer through the generic render-pass mapping path as
`buffers: &[None, Some(cells.buffer())]`, and verify exact BGRA pixels after
`commit_and_wait(...)`.

The padding test uses asymmetric `grid_padding = [top, right, bottom, left]`
with top `1.0` and left `2.0`, proving the shader's existing
`uniforms.grid_padding.wx` interpretation (`w = left`, `x = top`). Pixels
outside the padded grid remain transparent when `padding_extend = 0`.

Verification passed:

```bash
cargo fmt -- roastty/src/renderer/metal/render_pass.rs roastty/src/renderer/metal/shaders.rs
cargo test -p roastty renderer::metal::render_pass
cargo test -p roastty renderer::metal::shaders
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/metal; then exit 1; else exit 0; fi
git diff --check
```

Observed results:

- `renderer::metal::render_pass`: 8 passed.
- `renderer::metal::shaders`: 8 passed.
- Full `roastty`: 2192 library tests passed, plus the C ABI harness passed.
- Both no-`ghostty` gates passed.
- `git diff --check` passed.

Codex reviewed the completed implementation and found no blocking issues. It
confirmed that the buffer mapping, production `cell_bg` read-back coverage,
asymmetric padding semantics, zero-instance behavior, and scope boundaries
satisfy the experiment.

## Conclusion

Roastty now has a tested Metal render-pass path for the first real terminal
content buffer. The renderer can bind `CellBg` data through the production
buffer-index layout and prove the shader output by reading exact pixels back
from a shared offscreen target.

The next renderer experiment can build on this by adding the next production
resource class needed for terminal rendering, without revisiting the basic
uniform-plus-content-buffer binding path.
