# Experiment 220: Port Offscreen Metal Cell Text Grayscale Readback

## Description

Experiments 216-219 proved the production Metal shaders for `bg_color`,
`cell_bg`, `image`, and `bg_image` by drawing into offscreen textures and
reading back exact pixels. The next production shader is `cell_text`.

`cell_text` is the first shader that combines all current render-pass resource
classes:

- a per-instance vertex buffer at Metal buffer index `0`;
- uniforms at Metal buffer index `1`;
- the cell-background color grid at Metal buffer index `2`;
- a grayscale glyph atlas at texture index `0`;
- a color glyph atlas at texture index `1`.

This experiment should prove the simplest production text path: one grayscale
glyph rendered from a synthetic atlas texture into an offscreen render target.
It should not attempt to implement the full font stack, glyph rasterization,
atlas allocation, font discovery, shaping, ligatures, color emoji, cursor
glyphs, min-contrast correction, linear alpha correction, or public C ABI.

The goal is to verify that Roastty can execute the production `cell_text`
pipeline with realistic resource binding and deterministic read-back. It is a
renderer proof, not a font-system proof.

All public names must use Roastty naming.

## Changes

1. Add cell-text draw helpers for tests.

   In `roastty/src/renderer/metal/render_pass.rs`, add test helpers that use the
   existing production types:
   - `CellTextVertex`;
   - `CellTextAtlas`;
   - `CellTextFlags`;
   - `CellBg`;
   - `MetalStandardPipelines::cell_text`;
   - `MetalRenderPassStep.buffers`;
   - `MetalRenderPassStep.textures`;
   - `MetalPrimitiveType::TriangleStrip`.

   The helper should upload one `CellTextVertex` with
   `MetalBuffer::init_fill(...)` using `MetalStorageMode::Shared`, bind it as
   `buffers[0]`, bind a `CellBg` grid as `buffers[1]` so render-pass mapping
   sends it to Metal buffer index `2`, bind uniforms at Metal buffer index `1`,
   bind the grayscale atlas texture at `textures[0]`, bind a dummy color atlas
   texture at `textures[1]`, and draw with `vertex_count = 4`.

   Add a dedicated `cell_text_uniforms(...)` test helper. It must explicitly set
   `cursor_pos` outside the tested grid, for example `[u16::MAX, u16::MAX]` or
   `[grid_width, grid_height]`. Do not rely on `MetalUniforms::test_with_grid`
   defaults here: its default cursor position is `[0, 0]`, and the production
   shader replaces non-cursor glyph color with `cursor_color` when the glyph is
   under the cursor.

   Do not add sampler-state plumbing. The production `cell_text_fragment` uses a
   `constexpr sampler`.

2. Add a synthetic grayscale atlas texture helper.

   Use `ImageTextureFormat::Gray` with `srgb = false` and
   `MetalStorageMode::Shared` to create a small one-channel texture directly via
   `MetalTexture::new(...)`.

   Required helper behavior:
   - accept width, height, and raw grayscale bytes;
   - validate through `MetalTexture::new(...)`'s existing length checks;
   - create textures with
     `image_texture_options(ImageTextureFormat::Gray, false, MetalStorageMode::Shared)`;
   - keep atlas bytes simple and deterministic, preferably endpoint values `0`
     and `255`.

   Do not route this through the renderer image upload backend; that path only
   accepts prepared RGBA image uploads and is not the glyph atlas path.

3. Add live production `cell_text` read-back tests.

   Add tests in `roastty/src/renderer/metal/render_pass.rs`.

   Required tests:
   - `cell_text_grayscale_render_pass_draws_atlas_mask`:
     - create `MetalStandardPipelines`;
     - create a 2x2 grayscale atlas texture with bytes `[255, 0, 0, 255]`;
     - create a 1x1 dummy RGBA color atlas texture;
     - create a 2x2 BGRA8 unorm shared render target, cleared transparent;
     - create uniforms for a 2x2 screen with:
       - `screen_size = [2, 2]`;
       - `grid_size = [1, 1]`;
       - `cell_size = [2.0, 2.0]`;
       - zero `grid_padding`;
       - `padding_extend = 0`;
       - `bg_color = [0, 0, 0, 0]`;
       - `min_contrast = 0.0`;
       - `use_display_p3 = true`;
       - `use_linear_blending = false`;
       - `use_linear_correction = false`;
       - `cursor_pos` outside the tested grid;
     - create a one-cell `CellBg` grid with transparent background;
     - create one `CellTextVertex`:
       - `glyph_pos = [0, 0]`;
       - `glyph_size = [2, 2]`;
       - `bearings = [0, 2]`, so the glyph's top-left lands at the cell's
         top-left for a 2-pixel-high cell;
       - `grid_pos = [0, 0]`;
       - `color = [255, 0, 0, 255]`;
       - `atlas = CellTextAtlas::Grayscale`;
       - `flags = CellTextFlags::new(false, false)`;
       - zero padding;
     - draw `pipelines.cell_text` with `MetalPrimitiveType::TriangleStrip`,
       `vertex_count = 4`, and `instance_count = 1`;
     - read back the 2x2 target after `commit_and_wait(...)`;
     - verify the atlas mask produces red pixels where the atlas byte is `255`
       and transparent pixels where the atlas byte is `0`, in exact BGRA order.

   - `cell_text_grayscale_respects_bearings_and_glyph_size`:
     - create a 1x1 grayscale atlas texture with byte `[255]`;
     - create a 1x1 dummy RGBA color atlas texture;
     - create a 3x3 transparent render target;
     - create uniforms for a 3x3 screen with `grid_size = [1, 1]` and
       `cell_size = [3.0, 3.0]`, with `cursor_pos` outside the tested grid;
     - create one `CellTextVertex` with:
       - `glyph_size = [1, 1]`;
       - `bearings = [1, 2]`, so the one-pixel glyph lands at `(1, 1)`;
     - verify only the center pixel is colored and all surrounding pixels remain
       transparent.

   - `cell_text_zero_instance_step_does_not_bind_or_draw`:
     - clear the render target to one color;
     - provide valid uniforms, `CellTextVertex`, `CellBg`, grayscale atlas, and
       dummy RGBA color atlas resources with a different color;
     - draw `pipelines.cell_text` with `instance_count = 0`;
     - verify the clear color remains.

4. Keep color expectations deterministic.

   The grayscale branch of `cell_text_fragment` loads the vertex color in linear
   form, optionally unlinearizes when `use_linear_blending = false`, then
   multiplies by the grayscale atlas alpha mask.

   To keep exact read-back deterministic:
   - use endpoint-only text colors (`0` / `255`);
   - use endpoint-only grayscale atlas bytes (`0` / `255`);
   - keep `min_contrast = 0.0`;
   - keep `use_linear_correction = false`;
   - set `cursor_pos` outside the tested grid. This is mandatory because the
     production shader changes non-cursor glyph color when `grid_pos` matches
     `cursor_pos`.

   If exact bytes differ, do not loosen the test silently. Record the observed
   bytes and either adjust the deterministic setup or close the experiment as
   Partial/Fail.

5. Verification commands.

   Run:

   ```bash
   cargo fmt -- roastty/src/renderer/metal/render_pass.rs
   cargo test -p roastty renderer::metal::render_pass
   cargo test -p roastty renderer::metal::shaders
   cargo test -p roastty renderer::metal::pipeline
   cargo test -p roastty renderer::shader
   cargo test -p roastty
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
   if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/metal; then exit 1; else exit 0; fi
   git diff --check
   ```

   `cargo fmt` is required for Rust edits; accept formatter output as-is.

## Non-Negotiable Invariants

- Do not add font loading, font discovery, shaping, glyph rasterization, or
  atlas allocation.
- Do not add color glyph rendering beyond a dummy texture required by the shader
  signature.
- Do not add min-contrast, cursor-glyph, linear-correction, ligature, or
  multi-cell text behavior.
- Do not add sampler binding; the production shader uses a `constexpr sampler`.
- Do not add IOSurface, CAMetalLayer, Swift integration, presentation, frame
  callbacks, draw threads, renderer health, or public C ABI.
- Do not change production shader source.
- Do not modify vendored Ghostty source.
- Do not weaken Experiments 216-219 render-pass read-back tests.
- Do not introduce public or app-facing `ghostty_*` names in Roastty.

## Pass Criteria

- The production `cell_text` shader draws a grayscale atlas glyph into an
  offscreen render target.
- Read-back proves exact BGRA output for a grayscale atlas mask.
- Read-back proves `bearings` / `glyph_size` placement for a one-pixel glyph.
- The test binds vertex buffer `0`, uniforms `1`, background colors `2`,
  grayscale texture `0`, and dummy color texture `1` through the existing
  render-pass mapping.
- Zero-instance behavior remains no-draw.
- Full verification passes, including both no-`ghostty` gates.

## Failure Criteria

- The experiment changes shader source or uses a fake fragment shader instead of
  production `cell_text`.
- The glyph textures or background color buffer are bound through an ad hoc path
  instead of the existing render-pass mappings.
- The test only checks command encoding and does not read pixels back.
- The experiment grows into font loading, glyph rasterization, atlas allocation,
  color glyphs, min contrast, cursor rendering, presentation, Swift/app
  integration, or public C ABI.
- Existing Metal `bg_color`, `cell_bg`, `image`, `bg_image`, shader-library,
  pipeline, texture, buffer, image, or full Roastty tests regress.

## Result

**Result:** Pass

Implemented focused offscreen read-back tests for the production Metal
`cell_text` grayscale path in `roastty/src/renderer/metal/render_pass.rs`.

The experiment added test-only helpers for:

- cursor-safe `cell_text` uniforms;
- `CellTextVertex` upload through the existing Metal buffer wrapper;
- synthetic R8 grayscale atlas textures;
- the required dummy RGBA color atlas texture.

The new tests prove:

- a 2x2 grayscale atlas mask renders exact red/transparent BGRA pixels through
  `pipelines.cell_text`;
- `bearings` and `glyph_size` place a one-pixel glyph at the expected target
  pixel;
- `instance_count = 0` remains a no-draw path and preserves the clear color.

The render-pass step binds the text vertex buffer through `buffers[0]`, uniforms
through Metal buffer `1`, the `CellBg` grid through `buffers[1]` so the existing
mapping sends it to Metal buffer `2`, the grayscale atlas at texture `0`, and a
dummy color atlas at texture `1`. The dummy color atlas binding is structural
coverage only; this experiment does not prove color-atlas sampling.

Verification passed:

```bash
cargo fmt -- roastty/src/renderer/metal/render_pass.rs
cargo test -p roastty renderer::metal::render_pass
cargo test -p roastty renderer::metal::shaders
cargo test -p roastty renderer::metal::pipeline
cargo test -p roastty renderer::shader
cargo test -p roastty
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/lib.rs roastty/include/roastty.h roastty/tests/abi_harness.c; then exit 1; else exit 0; fi
if rg -n 'ghostty|Ghostty|GHOSTTY' roastty/src/renderer/metal; then exit 1; else exit 0; fi
git diff --check
```

Codex reviewed the implementation and found no blocking issues. It confirmed the
resource binding shape, the cursor-position guard, and the read-back tests'
coverage of the grayscale path.

## Conclusion

Roastty can now execute the production Metal `cell_text` shader's grayscale
atlas branch in an offscreen render pass and verify exact pixels. The current
renderer proof covers grayscale masks, placement via bearings/glyph size, and
zero-instance behavior. It intentionally leaves color glyph sampling, font
loading, glyph rasterization, atlas allocation, min-contrast behavior, cursor
glyph behavior, presentation, and public C ABI integration for later
experiments.
